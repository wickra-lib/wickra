//! Logarithmic Return over a fixed lag.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Logarithmic return over a `period`-bar lag: `ln(price_t / price_{t−period})`.
///
/// The natural-log analogue of [`Roc`](crate::Roc) (which reports the simple
/// percentage change). Log returns are the canonical input for volatility and
/// statistical models because they are additive across time — the log return
/// over `k` bars equals the sum of the `k` one-bar log returns — and symmetric
/// around zero (a `+x` move and the reverse `−x` move cancel exactly).
///
/// ```text
/// r_t = ln(price_t / price_{t−period})
/// ```
///
/// Non-finite and non-positive prices are ignored: the input is dropped, state
/// is left untouched, and the last computed value is returned instead. The log
/// of a non-positive price is undefined, so such ticks must not enter the
/// window — mirroring [`HistoricalVolatility`](crate::HistoricalVolatility).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, LogReturn};
///
/// let mut indicator = LogReturn::new(1).unwrap();
/// indicator.update(100.0);
/// // ln(110 / 100) ≈ 0.09531
/// let r = indicator.update(110.0).unwrap();
/// assert!((r - (110.0_f64 / 100.0).ln()).abs() < 1e-12);
/// ```
#[derive(Debug, Clone)]
pub struct LogReturn {
    period: usize,
    window: VecDeque<f64>,
    last: Option<f64>,
}

impl LogReturn {
    /// Construct a new log-return indicator with the given lag.
    ///
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
            window: VecDeque::with_capacity(period + 1),
            last: None,
        })
    }

    /// Configured lag.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for LogReturn {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        // Non-finite or non-positive prices are ignored: `ln` of a non-positive
        // price is undefined, so the tick must not enter the window. Return the
        // last value and leave state untouched (SMA / EMA / HV convention).
        if !input.is_finite() || input <= 0.0 {
            return None;
        }
        if self.window.len() == self.period + 1 {
            self.window.pop_front();
        }
        self.window.push_back(input);
        if self.window.len() < self.period + 1 {
            return None;
        }
        // `prev` was pushed through the same guard, so it is finite and > 0 and
        // `(input / prev).ln()` is always well-defined.
        let prev = *self.window.front().expect("non-empty");
        let r = (input / prev).ln();
        self.last = Some(r);
        Some(r)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period + 1
    }

    #[inline]
    fn name(&self) -> &'static str {
        "LogReturn"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(LogReturn::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let lr = LogReturn::new(5).unwrap();
        assert_eq!(lr.period(), 5);
        assert_eq!(lr.warmup_period(), 6);
        assert_eq!(lr.name(), "LogReturn");
        assert!(!lr.is_ready());
    }

    #[test]
    fn known_value() {
        // LogReturn(1): ln(110 / 100).
        let mut lr = LogReturn::new(1).unwrap();
        let out = lr.batch(&[100.0, 110.0]);
        assert!(out[0].is_none());
        assert_relative_eq!(out[1].unwrap(), (110.0_f64 / 100.0).ln(), epsilon = 1e-12);
    }

    #[test]
    fn multi_bar_lag() {
        // LogReturn(3): at index 3, ln(price_3 / price_0).
        let mut lr = LogReturn::new(3).unwrap();
        let out = lr.batch(&[100.0, 105.0, 108.0, 121.0]);
        for v in out.iter().take(3) {
            assert!(v.is_none());
        }
        assert_relative_eq!(out[3].unwrap(), (121.0_f64 / 100.0).ln(), epsilon = 1e-12);
    }

    #[test]
    fn additive_across_time() {
        // The 2-bar log return equals the sum of the two 1-bar log returns.
        let prices = [50.0, 55.0, 60.5];
        let mut lag2 = LogReturn::new(2).unwrap();
        let two_bar = lag2.batch(&prices)[2].unwrap();
        let mut lag1 = LogReturn::new(1).unwrap();
        let ones = lag1.batch(&prices);
        let sum = ones[1].unwrap() + ones[2].unwrap();
        assert_relative_eq!(two_bar, sum, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut lr = LogReturn::new(4).unwrap();
        for v in lr.batch(&[42.0; 20]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut lr = LogReturn::new(1).unwrap();
        let out = lr.batch(&[100.0, 110.0]);
        out[1].expect("ready after two inputs");
        assert_eq!(lr.update(f64::NAN), None);
        assert_eq!(lr.update(f64::INFINITY), None);
        // Window untouched: the next finite price still references prev = 110.
        assert_relative_eq!(
            lr.update(121.0).unwrap(),
            (121.0_f64 / 110.0).ln(),
            epsilon = 1e-12
        );
    }

    #[test]
    fn skips_non_positive_prices() {
        let mut lr = LogReturn::new(1).unwrap();
        let out = lr.batch(&[100.0, 110.0]);
        out[1].expect("ready");
        // A non-positive tick is ignored and the previous valid price is kept.
        assert_eq!(lr.update(-5.0), None);
        assert_eq!(lr.update(0.0), None);
        let mut control = lr.clone();
        let after = lr.update(121.0).expect("ready");
        assert_eq!(control.update(121.0).expect("ready"), after);
        assert_relative_eq!(after, (121.0_f64 / 110.0).ln(), epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut lr = LogReturn::new(3).unwrap();
        lr.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(lr.is_ready());
        lr.reset();
        assert!(!lr.is_ready());
        assert_eq!(lr.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=60)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let batch = LogReturn::new(5).unwrap().batch(&prices);
        let mut b = LogReturn::new(5).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            LogReturn::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
