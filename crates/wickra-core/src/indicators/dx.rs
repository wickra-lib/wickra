//! Directional Movement Index (DX), Wilder-smoothed.

use crate::error::{Error, Result};
use crate::indicators::adx::directional_movement;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Wilder's Directional Movement Index (`DX`).
///
/// `DX = 100 · |+DI − −DI| / (+DI + −DI)`, the un-smoothed precursor to
/// [`Adx`](crate::Adx) (which is the Wilder average of `DX`). Both directional
/// indicators are derived from Wilder-smoothed `+DM`, `−DM` and true range over
/// `period` bars, so the first value is emitted after `period + 1` candles.
///
/// `DX` ranges over `[0, 100]`: high when one side of the directional system
/// clearly dominates (a strong trend) and near zero when `+DI` and `−DI` are
/// balanced (a range). When both directional indicators are zero — a perfectly
/// flat market — the index returns `0`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Dx};
///
/// let mut indicator = Dx::new(5).unwrap();
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
pub struct Dx {
    period: usize,
    prev: Option<Candle>,
    plus_dm_seed: f64,
    minus_dm_seed: f64,
    tr_seed: f64,
    seed_count: usize,
    plus_dm_smooth: Option<f64>,
    minus_dm_smooth: Option<f64>,
    tr_smooth: Option<f64>,
}

impl Dx {
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
            plus_dm_seed: 0.0,
            minus_dm_seed: 0.0,
            tr_seed: 0.0,
            seed_count: 0,
            plus_dm_smooth: None,
            minus_dm_smooth: None,
            tr_smooth: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Dx {
    type Input = Candle;
    type Output = f64;

    fn update(&mut self, candle: Candle) -> Option<f64> {
        let Some(prev) = self.prev else {
            self.prev = Some(candle);
            return None;
        };
        self.prev = Some(candle);

        let (plus_dm, minus_dm) = directional_movement(&prev, &candle);
        let tr = candle.true_range(Some(prev.close));
        let n = self.period as f64;

        let (plus_v, minus_v, tr_v) = if let (Some(p), Some(m), Some(t)) =
            (self.plus_dm_smooth, self.minus_dm_smooth, self.tr_smooth)
        {
            let p_new = p - p / n + plus_dm;
            let m_new = m - m / n + minus_dm;
            let t_new = t - t / n + tr;
            self.plus_dm_smooth = Some(p_new);
            self.minus_dm_smooth = Some(m_new);
            self.tr_smooth = Some(t_new);
            (p_new, m_new, t_new)
        } else {
            self.plus_dm_seed += plus_dm;
            self.minus_dm_seed += minus_dm;
            self.tr_seed += tr;
            self.seed_count += 1;
            if self.seed_count < self.period {
                return None;
            }
            self.plus_dm_smooth = Some(self.plus_dm_seed);
            self.minus_dm_smooth = Some(self.minus_dm_seed);
            self.tr_smooth = Some(self.tr_seed);
            (self.plus_dm_seed, self.minus_dm_seed, self.tr_seed)
        };

        let (plus_di, minus_di) = if tr_v == 0.0 {
            (0.0, 0.0)
        } else {
            (100.0 * plus_v / tr_v, 100.0 * minus_v / tr_v)
        };
        let di_sum = plus_di + minus_di;
        let dx = if di_sum == 0.0 {
            0.0
        } else {
            100.0 * (plus_di - minus_di).abs() / di_sum
        };
        Some(dx)
    }

    fn reset(&mut self) {
        self.prev = None;
        self.plus_dm_seed = 0.0;
        self.minus_dm_seed = 0.0;
        self.tr_seed = 0.0;
        self.seed_count = 0;
        self.plus_dm_smooth = None;
        self.minus_dm_smooth = None;
        self.tr_smooth = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.tr_smooth.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "DX"
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
        assert!(matches!(Dx::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_report_config() {
        let dx = Dx::new(7).unwrap();
        assert_eq!(dx.period(), 7);
        assert_eq!(dx.name(), "DX");
        assert_eq!(dx.warmup_period(), 8);
        assert!(!dx.is_ready());
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
            let mut dx = Dx::new(period).unwrap();
            let out: Vec<Option<f64>> = dx.batch(&candles);
            let first = out.iter().position(Option::is_some).unwrap();
            assert_eq!(first + 1, dx.warmup_period());
        }
    }

    #[test]
    fn strong_trend_drives_dx_high() {
        // A clean uptrend has one-sided directional movement, so DX is large.
        let candles: Vec<Candle> = (0..12)
            .map(|i| {
                let base = 100.0 + f64::from(i) * 2.0;
                c(base + 1.0, base - 0.5, base + 0.5)
            })
            .collect();
        let mut dx = Dx::new(3).unwrap();
        let out: Vec<Option<f64>> = dx.batch(&candles);
        assert_eq!(out[0], None);
        assert!(out[3].is_some());
        let last = out.into_iter().flatten().last().unwrap();
        assert!(last > 50.0 && last <= 100.0);
        assert!(dx.is_ready());
    }

    #[test]
    fn flat_market_returns_zero() {
        // Both directional indicators collapse to zero -> DX is zero.
        let candles: Vec<Candle> = (0..6).map(|_| c(50.0, 50.0, 50.0)).collect();
        let mut dx = Dx::new(3).unwrap();
        let last = dx.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn balanced_directional_movement_is_low() {
        // Alternating up and down bars of equal magnitude keep +DI and -DI close,
        // so DX stays well below a trending reading.
        let candles: Vec<Candle> = (0..30)
            .map(|i| {
                let base = if i % 2 == 0 { 100.0 } else { 101.0 };
                c(base + 1.0, base - 1.0, base)
            })
            .collect();
        let mut dx = Dx::new(5).unwrap();
        let last = dx.batch(&candles).into_iter().flatten().last().unwrap();
        assert!((0.0..=100.0).contains(&last));
    }

    #[test]
    fn reset_restores_initial_state() {
        let candles: Vec<Candle> = (0..6)
            .map(|i| {
                let base = 100.0 + f64::from(i) * 2.0;
                c(base + 1.0, base - 0.5, base + 0.5)
            })
            .collect();
        let mut dx = Dx::new(3).unwrap();
        let _ = dx.batch(&candles);
        assert!(dx.is_ready());
        dx.reset();
        assert!(!dx.is_ready());
        assert_eq!(dx.update(candles[0]), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Dx::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
