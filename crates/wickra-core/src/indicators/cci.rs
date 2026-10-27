//! Commodity Channel Index (CCI).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Commodity Channel Index.
///
/// `CCI = (TP - SMA(TP)) / (0.015 * mean absolute deviation of TP)`, where
/// `TP = (high + low + close) / 3`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Cci};
///
/// let mut indicator = Cci::new(5).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Cci {
    period: usize,
    factor: f64,
    window: VecDeque<f64>,
    sum: RollingSum,
}

impl Cci {
    /// Construct a new CCI with the canonical 0.015 scaling factor.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
        Self::with_factor(period, 0.015)
    }

    /// Construct a CCI with a custom scaling factor (the standard literature
    /// uses 0.015 to put roughly 70 % of values inside ±100).
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0` and
    /// [`Error::NonPositiveMultiplier`] if `factor <= 0`.
    pub fn with_factor(period: usize, factor: f64) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !factor.is_finite() || factor <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        Ok(Self {
            period,
            factor,
            window: VecDeque::with_capacity(period),
            sum: RollingSum::new(),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Cci {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let tp = candle.typical_price();
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("non-empty");
            self.sum.evict(old);
        }
        self.window.push_back(tp);
        self.sum.push(tp);
        if self.sum.needs_reseed(self.period) {
            self.sum.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let n = self.period as f64;
        let mean = self.sum.value() / n;
        let mad: f64 = self.window.iter().map(|v| (v - mean).abs()).sum::<f64>() / n;
        if mad == 0.0 {
            return Some(0.0);
        }
        Some((tp - mean) / (self.factor * mad))
    }

    fn reset(&mut self) {
        self.window.clear();
        self.sum.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "CCI"
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
    fn flat_candles_yield_zero() {
        let candles: Vec<Candle> = (0..30).map(|_| c(10.0, 10.0, 10.0)).collect();
        let mut cci = Cci::new(20).unwrap();
        for v in cci.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn rejects_invalid_input() {
        assert!(Cci::new(0).is_err());
        assert!(Cci::with_factor(20, 0.0).is_err());
        assert!(Cci::with_factor(20, -1.0).is_err());
    }

    /// Cover the const accessor `period` (68-70) and the Indicator-impl
    /// `warmup_period` (102-104) + `name` (110-112). Existing tests never
    /// inspect these metadata methods.
    #[test]
    fn accessors_and_metadata() {
        let cci = Cci::new(20).unwrap();
        assert_eq!(cci.period(), 20);
        assert_eq!(cci.warmup_period(), 20);
        assert_eq!(cci.name(), "CCI");
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 50.0 + (f64::from(i) * 0.2).sin() * 10.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut a = Cci::new(20).unwrap();
        let mut b = Cci::new(20).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..30).map(|_| c(10.0, 10.0, 10.0)).collect();
        let mut cci = Cci::new(20).unwrap();
        cci.batch(&candles);
        assert!(cci.is_ready());
        cci.reset();
        assert!(!cci.is_ready());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Cci::with_factor(crate::error::MAX_PERIOD + 1, 0.015),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
