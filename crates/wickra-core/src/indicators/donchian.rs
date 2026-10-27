//! Donchian Channels.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Donchian Channels output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DonchianOutput {
    /// Highest high over the lookback.
    pub upper: f64,
    /// Average of upper and lower.
    pub middle: f64,
    /// Lowest low over the lookback.
    pub lower: f64,
}

/// Donchian Channels: rolling highest high / lowest low envelopes.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Donchian};
///
/// let mut indicator = Donchian::new(5).unwrap();
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
pub struct Donchian {
    period: usize,
    candles: VecDeque<Candle>,
}

impl Donchian {
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

impl Indicator for Donchian {
    type Input = Candle;
    type Output = DonchianOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<DonchianOutput> {
        if self.candles.len() == self.period {
            self.candles.pop_front();
        }
        self.candles.push_back(candle);
        if self.candles.len() < self.period {
            return None;
        }
        let upper = self
            .candles
            .iter()
            .map(|c| c.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let lower = self
            .candles
            .iter()
            .map(|c| c.low)
            .fold(f64::INFINITY, f64::min);
        Some(DonchianOutput {
            upper,
            middle: f64::midpoint(upper, lower),
            lower,
        })
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
        "DonchianChannels"
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
    fn flat_market_yields_equal_bands() {
        let candles: Vec<Candle> = (0..20).map(|_| c(11.0, 9.0, 10.0)).collect();
        let mut d = Donchian::new(5).unwrap();
        let last = d.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last.upper, 11.0, epsilon = 1e-12);
        assert_relative_eq!(last.lower, 9.0, epsilon = 1e-12);
        assert_relative_eq!(last.middle, 10.0, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| c(f64::from(i) + 2.0, f64::from(i), f64::from(i) + 1.0))
            .collect();
        let mut a = Donchian::new(10).unwrap();
        let mut b = Donchian::new(10).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn upper_above_middle_above_lower() {
        let candles: Vec<Candle> = (0..50)
            .map(|i| c(f64::from(i) + 1.0, f64::from(i) - 1.0, f64::from(i)))
            .collect();
        let mut d = Donchian::new(10).unwrap();
        for o in d.batch(&candles).into_iter().flatten() {
            assert!(o.upper >= o.middle);
            assert!(o.middle >= o.lower);
        }
    }

    #[test]
    fn rejects_zero_period() {
        assert!(Donchian::new(0).is_err());
    }

    /// Cover the const accessor `period` (57-59) and the Indicator-impl
    /// `warmup_period` (95-97) + `name` (103-105). Existing tests never
    /// inspect these metadata methods.
    #[test]
    fn accessors_and_metadata() {
        let d = Donchian::new(20).unwrap();
        assert_eq!(d.period(), 20);
        assert_eq!(d.warmup_period(), 20);
        assert_eq!(d.name(), "DonchianChannels");
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..20)
            .map(|i| c(f64::from(i) + 1.0, f64::from(i) - 1.0, f64::from(i)))
            .collect();
        let mut d = Donchian::new(5).unwrap();
        d.batch(&candles);
        assert!(d.is_ready());
        d.reset();
        assert!(!d.is_ready());
        assert_eq!(d.update(candles[0]), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Donchian::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
