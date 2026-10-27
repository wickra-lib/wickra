//! Rate of Change (ROC).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Rate of Change as a percentage: `(close - close[period]) / close[period] * 100`.
///
/// Non-finite inputs are ignored and leave the window untouched; the last
/// computed value is returned instead, matching the SMA / EMA convention.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Roc};
///
/// let mut indicator = Roc::new(3).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Roc {
    period: usize,
    window: VecDeque<f64>,
    last: Option<f64>,
}

impl Roc {
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

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Roc {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        // Non-finite inputs are ignored: return the last value, leave state as is.
        if !input.is_finite() {
            return None;
        }
        if self.window.len() == self.period + 1 {
            self.window.pop_front();
        }
        self.window.push_back(input);
        if self.window.len() < self.period + 1 {
            return None;
        }
        let prev = *self.window.front().expect("non-empty");
        let roc = if prev == 0.0 {
            0.0
        } else {
            (input - prev) / prev * 100.0
        };
        self.last = Some(roc);
        Some(roc)
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
        "ROC"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn constant_series_yields_zero() {
        let mut roc = Roc::new(5).unwrap();
        let out = roc.batch(&[10.0_f64; 20]);
        for v in out.iter().skip(5).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn known_value() {
        // ROC(3) where prev = 100, now = 110 -> 10%
        let mut roc = Roc::new(3).unwrap();
        let out = roc.batch(&[100.0, 105.0, 108.0, 110.0]);
        assert_relative_eq!(out[3].unwrap(), 10.0, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=30).map(|i| f64::from(i) * 2.0).collect();
        let mut a = Roc::new(5).unwrap();
        let mut b = Roc::new(5).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut roc = Roc::new(5).unwrap();
        roc.batch(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert!(roc.is_ready());
        roc.reset();
        assert!(!roc.is_ready());
    }

    #[test]
    fn rejects_zero_period() {
        assert!(Roc::new(0).is_err());
    }

    /// Cover the const accessor `period` (47-49) and the Indicator-impl
    /// `warmup_period` (83-85) + `name` (91-93). Existing tests never
    /// inspect these metadata methods.
    #[test]
    fn accessors_and_metadata() {
        let roc = Roc::new(5).unwrap();
        assert_eq!(roc.period(), 5);
        assert_eq!(roc.warmup_period(), 6);
        assert_eq!(roc.name(), "ROC");
    }

    /// Cover the `prev == 0.0` defensive branch (line 70). All existing
    /// tests use prices ≥ 1.0, so the divide-by-zero guard was never
    /// triggered. Feed a leading zero followed by `period` more values
    /// so the front of the window is exactly 0.0, then assert the next
    /// emission is the flat-momentum fallback 0.0 (not NaN).
    #[test]
    fn zero_previous_price_yields_zero_roc() {
        let mut roc = Roc::new(3).unwrap();
        let out = roc.batch(&[0.0, 5.0, 7.0, 9.0]);
        let v = out[3].expect("ready after period + 1 inputs");
        assert_eq!(v, 0.0);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut roc = Roc::new(3).unwrap();
        let out = roc.batch(&[100.0, 105.0, 108.0, 110.0]);
        out[3].expect("ROC(3) ready after four inputs");
        // Non-finite inputs return the last value without sliding the window.
        assert_eq!(roc.update(f64::NAN), None);
        assert_eq!(roc.update(f64::INFINITY), None);
        // Window untouched: the next finite input still references prev = 105.
        assert_relative_eq!(
            roc.update(115.0).unwrap(),
            (115.0 - 105.0) / 105.0 * 100.0,
            epsilon = 1e-12
        );
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Roc::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
