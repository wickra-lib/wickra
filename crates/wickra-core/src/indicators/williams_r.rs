//! Williams %R.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Williams %R: `-100 * (HH - close) / (HH - LL)` over the lookback window.
///
/// Values lie in `[-100, 0]` and approximate the mirror image of the fast
/// Stochastic %K.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, WilliamsR};
///
/// let mut indicator = WilliamsR::new(5).unwrap();
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
pub struct WilliamsR {
    period: usize,
    candles: VecDeque<Candle>,
}

impl WilliamsR {
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
            candles: VecDeque::with_capacity(period),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for WilliamsR {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        if self.candles.len() == self.period {
            self.candles.pop_front();
        }
        self.candles.push_back(candle);
        if self.candles.len() < self.period {
            return None;
        }
        let hh = self
            .candles
            .iter()
            .map(|c| c.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let ll = self
            .candles
            .iter()
            .map(|c| c.low)
            .fold(f64::INFINITY, f64::min);
        let range = hh - ll;
        if range == 0.0 {
            return Some(-50.0);
        }
        Some(-100.0 * (hh - candle.close) / range)
    }

    fn reset(&mut self) {
        self.candles.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.candles.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "WilliamsR"
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
    fn close_at_high_yields_zero() {
        let candles = vec![c(10.0, 8.0, 9.0), c(11.0, 9.0, 10.0), c(12.0, 10.0, 12.0)];
        let mut w = WilliamsR::new(3).unwrap();
        let out = w.batch(&candles);
        assert_relative_eq!(out[2].unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn close_at_low_yields_minus_100() {
        let candles = vec![c(12.0, 10.0, 11.0), c(11.0, 9.0, 10.0), c(10.0, 8.0, 8.0)];
        let mut w = WilliamsR::new(3).unwrap();
        let out = w.batch(&candles);
        assert_relative_eq!(out[2].unwrap(), -100.0, epsilon = 1e-12);
    }

    #[test]
    fn within_range() {
        let candles: Vec<Candle> = (0..100)
            .map(|i| {
                let m = 50.0 + (f64::from(i) * 0.3).sin() * 5.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut w = WilliamsR::new(14).unwrap();
        for v in w.batch(&candles).into_iter().flatten() {
            assert!((-100.0..=0.0).contains(&v), "%R out of range: {v}");
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..30)
            .map(|i| c(f64::from(i) + 2.0, f64::from(i), f64::from(i) + 1.0))
            .collect();
        let mut a = WilliamsR::new(5).unwrap();
        let mut b = WilliamsR::new(5).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_zero_period() {
        assert!(WilliamsR::new(0).is_err());
    }

    /// Cover the const accessor `period` (49-51) and the Indicator-impl
    /// `warmup_period` (87-89) + `name` (95-97). Existing tests never
    /// inspect these metadata methods.
    #[test]
    fn accessors_and_metadata() {
        let w = WilliamsR::new(14).unwrap();
        assert_eq!(w.period(), 14);
        assert_eq!(w.warmup_period(), 14);
        assert_eq!(w.name(), "WilliamsR");
    }

    /// Cover the `range == 0.0` defensive branch (line 78). All other
    /// tests use H != L candles so the lookback range is always positive.
    /// Feed a stream of perfectly flat candles (H == L == close) — the
    /// lookback hi/lo coincide and the divide-by-zero guard fires,
    /// returning the neutral mid-range value -50.0.
    #[test]
    fn zero_range_yields_minus_fifty() {
        let candles: Vec<Candle> = (0..5).map(|_| c(10.0, 10.0, 10.0)).collect();
        let mut w = WilliamsR::new(3).unwrap();
        let last = w
            .batch(&candles)
            .into_iter()
            .flatten()
            .last()
            .expect("emits");
        assert_eq!(last, -50.0);
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..20)
            .map(|i| c(f64::from(i) + 2.0, f64::from(i), f64::from(i) + 1.0))
            .collect();
        let mut w = WilliamsR::new(5).unwrap();
        w.batch(&candles);
        assert!(w.is_ready());
        w.reset();
        assert!(!w.is_ready());
        assert_eq!(w.update(candles[0]), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            WilliamsR::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
