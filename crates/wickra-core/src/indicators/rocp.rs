//! Rate of Change Percentage (ROCP).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Rate of Change Percentage (`ROCP`): `(close - close[period]) / close[period]`.
///
/// The same momentum measure as [`Roc`](crate::Roc) but expressed as a raw
/// fraction rather than a percentage — `Roc` is exactly `100 · ROCP`. Where the
/// reference price is zero the result is reported as `0`.
///
/// Non-finite inputs are ignored and leave the window untouched; the last
/// computed value is returned instead, matching the SMA / EMA convention.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Rocp};
///
/// let mut indicator = Rocp::new(3).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Rocp {
    period: usize,
    window: VecDeque<f64>,
    last: Option<f64>,
}

impl Rocp {
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

impl Indicator for Rocp {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
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
        let rocp = if prev == 0.0 {
            0.0
        } else {
            (input - prev) / prev
        };
        self.last = Some(rocp);
        Some(rocp)
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
        "ROCP"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Rocp::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_report_config() {
        let r = Rocp::new(3).unwrap();
        assert_eq!(r.period(), 3);
        assert_eq!(r.name(), "ROCP");
        assert_eq!(r.warmup_period(), 4);
        assert!(!r.is_ready());
    }

    #[test]
    fn known_value_is_a_fraction() {
        // period 1 over [10, 11]: (11 - 10) / 10 = 0.1.
        let mut r = Rocp::new(1).unwrap();
        let out: Vec<Option<f64>> = r.batch(&[10.0, 11.0]);
        assert_eq!(out[0], None);
        assert_relative_eq!(out[1].unwrap(), 0.1, epsilon = 1e-12);
        assert!(r.is_ready());
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut r = Rocp::new(3).unwrap();
        for v in r.batch(&[10.0_f64; 12]).iter().skip(4).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn zero_reference_price_reports_zero() {
        // period 1 over [0, 5]: reference price is zero -> guarded to 0.
        let mut r = Rocp::new(1).unwrap();
        let out: Vec<Option<f64>> = r.batch(&[0.0, 5.0]);
        assert_relative_eq!(out[1].unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn non_finite_input_holds_last() {
        let mut r = Rocp::new(1).unwrap();
        assert_eq!(r.update(10.0), None);
        r.update(11.0).unwrap();
        assert_eq!(r.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut r = Rocp::new(1).unwrap();
        let _ = r.batch(&[10.0, 11.0]);
        assert!(r.is_ready());
        r.reset();
        assert!(!r.is_ready());
        assert_eq!(r.update(10.0), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Rocp::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
