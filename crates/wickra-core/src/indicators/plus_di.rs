//! Plus Directional Indicator (+DI), Wilder-smoothed.

use crate::error::{Error, Result};
use crate::indicators::adx::directional_movement;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Wilder's Plus Directional Indicator (`PLUS_DI`).
///
/// `+DI = 100 · smoothed(+DM) / smoothed(TR)`, where both the plus directional
/// movement and the true range are Wilder-smoothed over `period` bars. It is the
/// bullish half of the directional system that drives [`Adx`](crate::Adx);
/// readings above [`MinusDi`](crate::MinusDi) mark an up-trending regime.
///
/// The first `period` raw values seed the two running sums; from then on each
/// applies the Wilder recursion `smoothed − smoothed / period + raw`. Because a
/// bar's directional movement and true range both need the previous bar, the
/// first value is emitted after `period + 1` candles. When the smoothed true
/// range is zero (a perfectly flat market) the indicator returns `0`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, PlusDi};
///
/// let mut indicator = PlusDi::new(5).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct PlusDi {
    period: usize,
    prev: Option<Candle>,
    dm_seed: f64,
    tr_seed: f64,
    seed_count: usize,
    dm_smooth: Option<f64>,
    tr_smooth: Option<f64>,
}

impl PlusDi {
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
            dm_seed: 0.0,
            tr_seed: 0.0,
            seed_count: 0,
            dm_smooth: None,
            tr_smooth: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for PlusDi {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let Some(prev) = self.prev else {
            self.prev = Some(candle);
            return None;
        };
        self.prev = Some(candle);

        let (plus_dm, _) = directional_movement(&prev, &candle);
        let tr = candle.true_range(Some(prev.close));
        let n = self.period as f64;

        let (dm_v, tr_v) = if let (Some(d), Some(t)) = (self.dm_smooth, self.tr_smooth) {
            let d_new = d - d / n + plus_dm;
            let t_new = t - t / n + tr;
            self.dm_smooth = Some(d_new);
            self.tr_smooth = Some(t_new);
            (d_new, t_new)
        } else {
            self.dm_seed += plus_dm;
            self.tr_seed += tr;
            self.seed_count += 1;
            if self.seed_count < self.period {
                return None;
            }
            self.dm_smooth = Some(self.dm_seed);
            self.tr_smooth = Some(self.tr_seed);
            (self.dm_seed, self.tr_seed)
        };

        let di = if tr_v == 0.0 {
            0.0
        } else {
            100.0 * dm_v / tr_v
        };
        Some(di)
    }

    fn reset(&mut self) {
        self.prev = None;
        self.dm_seed = 0.0;
        self.tr_seed = 0.0;
        self.seed_count = 0;
        self.dm_smooth = None;
        self.tr_smooth = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.dm_smooth.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "PLUS_DI"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(h: f64, l: f64, cl: f64) -> Candle {
        Candle::new(cl, h, l, cl, 1.0, 0).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(PlusDi::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_report_config() {
        let di = PlusDi::new(7).unwrap();
        assert_eq!(di.period(), 7);
        assert_eq!(di.name(), "PLUS_DI");
        assert_eq!(di.warmup_period(), 8);
        assert!(!di.is_ready());
    }

    #[test]
    fn warmup_period_matches_the_first_emitted_value() {
        // The first candle only seeds `prev`, so seeding starts on bar 2 and the
        // first value lands on bar `period + 1`. Pin that against the declared
        // warmup so the two can never drift apart again.
        let candles: Vec<Candle> = (0..12)
            .map(|i| {
                let x = f64::from(i);
                c(11.0 + x, 9.0 + 0.5 * x, 10.0 + x)
            })
            .collect();
        for period in 1..=5 {
            let mut di = PlusDi::new(period).unwrap();
            let out: Vec<Option<f64>> = di.batch(&candles);
            let first = out.iter().position(Option::is_some).unwrap();
            assert_eq!(first + 1, di.warmup_period());
        }
    }

    #[test]
    fn uptrend_drives_plus_di_high() {
        // Strict uptrend: +DM dominates, so +DI is large and bounded by 100.
        let candles: Vec<Candle> = (0..12)
            .map(|i| {
                let base = 100.0 + f64::from(i) * 2.0;
                c(base + 1.0, base - 0.5, base + 0.5)
            })
            .collect();
        let mut di = PlusDi::new(3).unwrap();
        let out: Vec<Option<f64>> = di.batch(&candles);
        assert_eq!(out[0], None);
        // Seeds after `period` directional moves (candle index `period`).
        assert!(out[3].is_some());
        let last = out.into_iter().flatten().last().unwrap();
        assert!(last > 0.0 && last <= 100.0);
        assert!(di.is_ready());
    }

    #[test]
    fn flat_market_returns_zero() {
        // No range and no movement: smoothed true range is zero -> +DI is zero.
        let candles: Vec<Candle> = (0..6).map(|_| c(50.0, 50.0, 50.0)).collect();
        let mut di = PlusDi::new(3).unwrap();
        let last = di.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_restores_initial_state() {
        let candles: Vec<Candle> = (0..6)
            .map(|i| {
                let base = 100.0 + f64::from(i) * 2.0;
                c(base + 1.0, base - 0.5, base + 0.5)
            })
            .collect();
        let mut di = PlusDi::new(3).unwrap();
        let _ = di.batch(&candles);
        assert!(di.is_ready());
        di.reset();
        assert!(!di.is_ready());
        assert_eq!(di.update(candles[0]), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            PlusDi::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
