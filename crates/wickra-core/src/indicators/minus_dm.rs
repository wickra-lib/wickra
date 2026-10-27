//! Minus Directional Movement (-DM), Wilder-smoothed.

use crate::error::{Error, Result};
use crate::indicators::adx::directional_movement;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Wilder's Minus Directional Movement (`MINUS_DM`).
///
/// The raw minus directional movement of a bar is `max(low_prev − low, 0)` when
/// the down-move exceeds the up-move `high − high_prev`, and `0` otherwise. This
/// indicator returns the Wilder-smoothed running total of that raw `-DM` over
/// `period` bars, the same accumulation that feeds [`Adx`](crate::Adx) and
/// [`MinusDi`](crate::MinusDi).
///
/// The first `period` raw values seed the sum; from then on each update applies
/// the Wilder recursion `smoothed − smoothed / period + raw`. Because a bar's
/// directional movement needs the previous bar, the first value is emitted after
/// `period + 1` candles.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, MinusDm};
///
/// let mut indicator = MinusDm::new(5).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 - f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base - 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct MinusDm {
    period: usize,
    prev: Option<Candle>,
    seed: f64,
    seed_count: usize,
    smooth: Option<f64>,
}

impl MinusDm {
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            prev: None,
            seed: 0.0,
            seed_count: 0,
            smooth: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for MinusDm {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let Some(prev) = self.prev else {
            self.prev = Some(candle);
            return None;
        };
        self.prev = Some(candle);

        let (_, minus_dm) = directional_movement(&prev, &candle);
        let n = self.period as f64;

        if let Some(s) = self.smooth {
            let s_new = s - s / n + minus_dm;
            self.smooth = Some(s_new);
            return Some(s_new);
        }

        self.seed += minus_dm;
        self.seed_count += 1;
        if self.seed_count < self.period {
            return None;
        }
        self.smooth = Some(self.seed);
        Some(self.seed)
    }

    fn reset(&mut self) {
        self.prev = None;
        self.seed = 0.0;
        self.seed_count = 0;
        self.smooth = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.smooth.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "MINUS_DM"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Candle with explicit high/low; open and close are pinned to `cl`.
    fn c(h: f64, l: f64, cl: f64) -> Candle {
        Candle::new(cl, h, l, cl, 1.0, 0).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(MinusDm::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_report_config() {
        let dm = MinusDm::new(7).unwrap();
        assert_eq!(dm.period(), 7);
        assert_eq!(dm.name(), "MINUS_DM");
        assert_eq!(dm.warmup_period(), 8);
        assert!(!dm.is_ready());
    }

    #[test]
    fn warmup_period_matches_the_first_emitted_value() {
        // The first candle only seeds `prev`, so seeding starts on bar 2 and the
        // first value lands on bar `period + 1`. Pin that against the declared
        // warmup so the two can never drift apart again.
        let candles: Vec<Candle> = (0..12)
            .map(|i| {
                let x = f64::from(i);
                c(20.0 - x, 5.0 - x, 12.0 - x)
            })
            .collect();
        for period in 1..=5 {
            let mut dm = MinusDm::new(period).unwrap();
            let out: Vec<Option<f64>> = dm.batch(&candles);
            let first = out.iter().position(Option::is_some).unwrap();
            assert_eq!(first + 1, dm.warmup_period());
        }
    }

    #[test]
    fn seeds_then_smooths_a_constant_minus_dm() {
        // Low falls by 1 each bar (down = +1); high falls by 0.5 each bar, so the
        // up-move is negative and -DM equals the down-move (1.0) on every bar.
        let candles: Vec<Candle> = (0..5)
            .map(|i| {
                c(
                    20.0 - 0.5 * f64::from(i),
                    18.0 - f64::from(i),
                    19.0 - f64::from(i),
                )
            })
            .collect();
        let mut dm = MinusDm::new(3).unwrap();
        let out: Vec<Option<f64>> = dm.batch(&candles);
        assert_eq!(out[0], None);
        assert_eq!(out[1], None);
        assert_eq!(out[2], None);
        // Seed = sum of three unit -DM values.
        assert_relative_eq!(out[3].unwrap(), 3.0, epsilon = 1e-12);
        // Wilder step: 3 - 3/3 + 1 = 3.
        assert_relative_eq!(out[4].unwrap(), 3.0, epsilon = 1e-12);
        assert!(dm.is_ready());
    }

    #[test]
    fn up_moves_contribute_zero() {
        // Strict uptrend: lows rise, so every raw -DM is zero.
        let candles: Vec<Candle> = (0..6)
            .map(|i| c(20.0 + f64::from(i), 5.0 + f64::from(i), 12.0 + f64::from(i)))
            .collect();
        let mut dm = MinusDm::new(3).unwrap();
        let last = dm.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_restores_initial_state() {
        let candles: Vec<Candle> = (0..5)
            .map(|i| {
                c(
                    20.0 - 0.5 * f64::from(i),
                    18.0 - f64::from(i),
                    19.0 - f64::from(i),
                )
            })
            .collect();
        let mut dm = MinusDm::new(3).unwrap();
        let _ = dm.batch(&candles);
        assert!(dm.is_ready());
        dm.reset();
        assert!(!dm.is_ready());
        assert_eq!(dm.update(candles[0]), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            MinusDm::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
