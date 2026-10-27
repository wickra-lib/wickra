//! Momentum (absolute price change over a fixed lookback).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Momentum: the raw price change over `period` bars, `price_t − price_{t−period}`.
///
/// Unlike [`Roc`](crate::Roc), which divides by the old price to give a
/// percentage, `Mom` reports the change in absolute price units. It is the
/// simplest momentum primitive: positive values mean price is higher than it
/// was `period` bars ago, negative values mean lower.
///
/// Non-finite inputs are ignored and leave the window untouched; the last
/// computed value is returned instead.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Mom};
///
/// let mut indicator = Mom::new(3).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Mom {
    period: usize,
    /// Rolling buffer of the last `period + 1` inputs, oldest at the front.
    window: VecDeque<f64>,
    last: Option<f64>,
}

impl Mom {
    /// Construct a new momentum indicator with the given lookback period.
    ///
    /// # Errors
    ///
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

    /// Configured lookback period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for Mom {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            // Non-finite input is ignored; the window is left untouched.
            return None;
        }
        if self.window.len() == self.period + 1 {
            self.window.pop_front();
        }
        self.window.push_back(input);
        if self.window.len() < self.period + 1 {
            return None;
        }
        let prev = *self.window.front().expect("window is non-empty");
        let mom = input - prev;
        self.last = Some(mom);
        Some(mom)
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
        "MOM"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(Mom::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `value` (56-63) and the
    /// Indicator-impl `name` body (101-103). Existing tests inspect
    /// momentum output but never query the metadata.
    #[test]
    fn accessors_and_metadata() {
        let mut m = Mom::new(5).unwrap();
        assert_eq!(m.period(), 5);
        assert_eq!(m.name(), "MOM");
        assert_eq!(m.value(), None);
        for i in 1..=6 {
            m.update(f64::from(i));
        }
        assert!(m.value().is_some());
    }

    #[test]
    fn reference_values() {
        // MOM(3): price_t − price_{t-3}.
        let mut mom = Mom::new(3).unwrap();
        let out = mom.batch(&[1.0, 2.0, 3.0, 4.0, 7.0]);
        assert_eq!(mom.warmup_period(), 4);
        assert_eq!(out[0], None);
        assert_eq!(out[2], None);
        assert_relative_eq!(out[3].unwrap(), 4.0 - 1.0, epsilon = 1e-12);
        assert_relative_eq!(out[4].unwrap(), 7.0 - 2.0, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut mom = Mom::new(5).unwrap();
        let out = mom.batch(&[10.0; 20]);
        for v in out.iter().skip(5).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut mom = Mom::new(3).unwrap();
        let out = mom.batch(&[1.0, 2.0, 3.0, 4.0]);
        out[3].expect("MOM(3) ready after four inputs");
        assert_eq!(mom.update(f64::NAN), None);
        assert_eq!(mom.update(f64::INFINITY), None);
        // Window untouched: the next finite input still references price 2.
        assert_relative_eq!(mom.update(10.0).unwrap(), 10.0 - 2.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut mom = Mom::new(3).unwrap();
        mom.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(mom.is_ready());
        mom.reset();
        assert!(!mom.is_ready());
        assert_eq!(mom.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=40).map(|i| f64::from(i) * 1.5).collect();
        let batch = Mom::new(7).unwrap().batch(&prices);
        let mut b = Mom::new(7).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Mom::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
