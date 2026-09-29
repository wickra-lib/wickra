//! Rolling historical Value-at-Risk (`VaR`).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::sorted_window;
use crate::traits::Indicator;

/// Rolling historical Value-at-Risk.
///
/// Input is treated as a period return. Over the trailing window of `period`
/// returns the indicator reports the empirical lower-tail quantile at the
/// given `confidence` level (e.g. `0.95` = the 95 %-confident worst-case
/// loss). The output is the **magnitude** of that loss, sign-flipped to be a
/// non-negative number (so a 5 % `VaR` is reported as `0.05`, not `-0.05`):
///
/// ```text
/// q       = (1 − confidence)
/// VaR_t   = − percentile(returns over window, q · 100)   if it is negative
/// VaR_t   = 0                                            otherwise
/// ```
///
/// `percentile` uses linear interpolation between the two closest order
/// statistics ("type 7" in R / `NumPy` default). If the q-quantile of the
/// window is itself non-negative (a window where every return was at or above
/// zero) the indicator returns `0.0` — there is no loss to report.
///
/// Each `update` is O(period · log period) due to the window-sort. Good
/// enough for the typical `period ≤ 252` rolling-VaR workflow.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, ValueAtRisk};
///
/// let mut var = ValueAtRisk::new(100, 0.95).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     last = var.update((f64::from(i) * 0.1).sin() * 0.02);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct ValueAtRisk {
    period: usize,
    confidence: f64,
    window: VecDeque<f64>,
    /// The window's values in `total_cmp` order, kept sorted as it slides:
    /// bit for bit what sorting a copy of the window would give.
    scratch: Vec<f64>,
}

impl ValueAtRisk {
    /// Construct a new rolling historical `VaR`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2`, or if
    /// `confidence` is outside the open interval `(0, 1)`.
    pub fn new(period: usize, confidence: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "value-at-risk needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !confidence.is_finite() || confidence <= 0.0 || confidence >= 1.0 {
            return Err(Error::InvalidPeriod {
                message: "confidence must lie strictly between 0 and 1",
            });
        }
        Ok(Self {
            period,
            confidence,
            window: VecDeque::with_capacity(period),
            scratch: Vec::with_capacity(period),
        })
    }

    /// Configured window length.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured confidence level.
    pub const fn confidence(&self) -> f64 {
        self.confidence
    }
}

/// Linear-interpolated percentile (type 7 / `NumPy` default) on a sorted slice.
fn percentile_sorted(sorted: &[f64], q: f64) -> f64 {
    let n = sorted.len();
    let pos = q * (n - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = pos - lo as f64;
        sorted[lo] + (sorted[hi] - sorted[lo]) * frac
    }
}

impl Indicator for ValueAtRisk {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let oldest = self.window.pop_front().expect("window is full");
            sorted_window::remove(&mut self.scratch, oldest);
        }
        self.window.push_back(input);
        sorted_window::insert(&mut self.scratch, input);
        if self.window.len() < self.period {
            return None;
        }
        let q = 1.0 - self.confidence;
        let cut = percentile_sorted(&self.scratch, q);
        // Loss magnitude (sign-flipped); 0 if quantile is non-negative.
        Some((-cut).max(0.0))
    }

    fn reset(&mut self) {
        self.window.clear();
        self.scratch.clear();
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
        "ValueAtRisk"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_invalid_params() {
        assert!(matches!(
            ValueAtRisk::new(1, 0.95),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            ValueAtRisk::new(20, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            ValueAtRisk::new(20, 1.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            ValueAtRisk::new(20, f64::NAN),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let v = ValueAtRisk::new(100, 0.95).unwrap();
        assert_eq!(v.period(), 100);
        assert_relative_eq!(v.confidence(), 0.95, epsilon = 1e-12);
        assert_eq!(v.name(), "ValueAtRisk");
        assert_eq!(v.warmup_period(), 100);
    }

    #[test]
    fn reference_value() {
        // returns = -5,-4,-3,-2,-1,0,1,2,3,4 (each *0.01), confidence 0.95.
        // q = 0.05, sorted positions 0..9, pos = 0.05*9 = 0.45,
        // -> -0.05 + (-0.04 - (-0.05))*0.45 = -0.05 + 0.0045 = -0.0455.
        // VaR = 0.0455.
        let mut v = ValueAtRisk::new(10, 0.95).unwrap();
        let returns: Vec<f64> = (-5..5).map(|i| f64::from(i) * 0.01).collect();
        let out = v.batch(&returns);
        assert_relative_eq!(out[9].unwrap(), 0.0455, epsilon = 1e-9);
    }

    #[test]
    fn all_positive_returns_yield_zero() {
        let mut v = ValueAtRisk::new(5, 0.95).unwrap();
        let out = v.batch(&[0.01, 0.02, 0.03, 0.04, 0.05]);
        assert_eq!(out[4], Some(0.0));
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut v = ValueAtRisk::new(3, 0.95).unwrap();
        assert_eq!(v.update(f64::NAN), None);
        assert_eq!(v.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut v = ValueAtRisk::new(3, 0.95).unwrap();
        v.batch(&[-0.01, -0.02, -0.03]);
        assert!(v.is_ready());
        v.reset();
        assert!(!v.is_ready());
        assert_eq!(v.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let returns: Vec<f64> = (0..50).map(|i| (f64::from(i) * 0.2).sin() * 0.02).collect();
        let batch = ValueAtRisk::new(10, 0.95).unwrap().batch(&returns);
        let mut s = ValueAtRisk::new(10, 0.95).unwrap();
        let streamed: Vec<_> = returns.iter().map(|r| s.update(*r)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn integer_position_quantile_branch() {
        // period=5, confidence=0.75 -> q=0.25, n-1=4 -> pos=1.0 (integer),
        // so the percentile helper takes the `lo == hi` branch.
        let mut v = ValueAtRisk::new(5, 0.75).unwrap();
        let out = v.batch(&[-0.05, -0.04, -0.03, -0.02, -0.01]);
        // sorted = same order; sorted[1] = -0.04, so VaR = 0.04 exactly.
        assert_relative_eq!(out[4].unwrap(), 0.04, epsilon = 1e-12);
    }
}
