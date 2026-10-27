//! Aroon Up / Down indicator.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Aroon output: up and down strengths in [0, 100].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AroonOutput {
    /// Time since the highest high, expressed as a percentage of the window.
    pub up: f64,
    /// Time since the lowest low, same convention.
    pub down: f64,
}

/// Aroon indicator: tracks how many bars since the highest high and lowest low
/// inside a `period + 1`-bar window. Returned as a percentage.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Aroon};
///
/// let mut indicator = Aroon::new(5).unwrap();
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
pub struct Aroon {
    period: usize,
    candles: VecDeque<Candle>,
}

impl Aroon {
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
            candles: VecDeque::with_capacity(period + 1),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Aroon {
    type Input = Candle;
    type Output = AroonOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<AroonOutput> {
        if self.candles.len() == self.period + 1 {
            self.candles.pop_front();
        }
        self.candles.push_back(candle);
        if self.candles.len() < self.period + 1 {
            return None;
        }
        // Find the index (0 = oldest) of the highest high and lowest low.
        let (mut hh_idx, mut ll_idx) = (0_usize, 0_usize);
        let (mut hh, mut ll) = (f64::NEG_INFINITY, f64::INFINITY);
        for (i, c) in self.candles.iter().enumerate() {
            if c.high >= hh {
                hh = c.high;
                hh_idx = i;
            }
            if c.low <= ll {
                ll = c.low;
                ll_idx = i;
            }
        }
        let n = self.period as f64;
        let up = 100.0 * hh_idx as f64 / n;
        let down = 100.0 * ll_idx as f64 / n;
        Some(AroonOutput { up, down })
    }

    fn reset(&mut self) {
        self.candles.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.candles.len() == self.period + 1
    }

    #[inline]
    fn name(&self) -> &'static str {
        "Aroon"
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
    fn pure_uptrend_aroon_up_100() {
        let candles: Vec<Candle> = (1..=15)
            .map(|i| c(f64::from(i) + 1.0, f64::from(i) - 1.0, f64::from(i)))
            .collect();
        let mut a = Aroon::new(14).unwrap();
        let last = a.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last.up, 100.0, epsilon = 1e-9);
        // The lowest low is at the oldest position (index 0).
        assert_relative_eq!(last.down, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn pure_downtrend_aroon_down_100() {
        let candles: Vec<Candle> = (1..=15)
            .rev()
            .map(|i| c(f64::from(i) + 1.0, f64::from(i) - 1.0, f64::from(i)))
            .collect();
        let mut a = Aroon::new(14).unwrap();
        let last = a.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last.down, 100.0, epsilon = 1e-9);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let m = 50.0 + (f64::from(i) * 0.3).sin() * 5.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut a = Aroon::new(14).unwrap();
        let mut b = Aroon::new(14).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn outputs_in_range() {
        let candles: Vec<Candle> = (0..200)
            .map(|i| {
                let m = 50.0 + (f64::from(i) * 0.2).sin() * 5.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut a = Aroon::new(14).unwrap();
        for o in a.batch(&candles).into_iter().flatten() {
            assert!((0.0..=100.0).contains(&o.up));
            assert!((0.0..=100.0).contains(&o.down));
        }
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (1..=20)
            .map(|i| c(f64::from(i) + 1.0, f64::from(i) - 1.0, f64::from(i)))
            .collect();
        let mut a = Aroon::new(14).unwrap();
        a.batch(&candles);
        assert!(a.is_ready());
        a.reset();
        assert!(!a.is_ready());
        assert_eq!(a.update(candles[0]), None);
    }

    /// Cover the const accessor `period` (56-58) and the Indicator-impl
    /// `name` body (104-106). `warmup_period` is exercised elsewhere.
    #[test]
    fn accessors_and_metadata() {
        let a = Aroon::new(14).unwrap();
        assert_eq!(a.period(), 14);
        assert_eq!(a.name(), "Aroon");
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Aroon::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
