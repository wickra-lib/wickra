//! Rolling Conditional Value-at-Risk (`CVaR` / Expected Shortfall).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::sorted_window;
use crate::traits::Indicator;

/// Rolling Conditional Value-at-Risk (Expected Shortfall).
///
/// Where [`crate::ValueAtRisk`] reports the loss at the lower-tail quantile,
/// `CVaR` averages **all** returns below that quantile — the expected loss
/// conditional on being in the bad tail:
///
/// ```text
/// q       = 1 − confidence
/// tail    = returns over window with rank fraction ≤ q
/// CVaR    = − mean(tail)                        if mean is negative
/// CVaR    = 0                                   otherwise
/// ```
///
/// The tail comprises the `floor(q · n)` smallest returns; if `floor` rounds
/// down to zero the smallest single return is used so the metric stays
/// defined for any `period ≥ 2`. Output is the magnitude of the expected
/// shortfall (sign-flipped to be non-negative). `CVaR` is by construction
/// `≥ VaR` because it averages losses *beyond* the `VaR` threshold.
///
/// Each `update` is O(period · log period).
///
/// # Example
///
/// ```
/// use wickra_core::{ConditionalValueAtRisk, Indicator};
///
/// let mut c = ConditionalValueAtRisk::new(100, 0.95).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     last = c.update((f64::from(i) * 0.1).sin() * 0.02);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct ConditionalValueAtRisk {
    period: usize,
    confidence: f64,
    window: VecDeque<f64>,
    /// The window's values in `total_cmp` order, kept sorted as it slides:
    /// bit for bit what sorting a copy of the window would give.
    scratch: Vec<f64>,
}

impl ConditionalValueAtRisk {
    /// Construct a new rolling `CVaR`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2`, or if
    /// `confidence` is outside `(0, 1)`.
    pub fn new(period: usize, confidence: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "conditional value-at-risk needs period >= 2",
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

impl Indicator for ConditionalValueAtRisk {
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
        let n = self.scratch.len();
        // Number of samples in the tail. Floor, with a min of 1 so the
        // expectation is always defined.
        let k = ((q * n as f64).floor() as usize).max(1);
        let tail = &self.scratch[..k];
        let mean = tail.iter().sum::<f64>() / k as f64;
        Some((-mean).max(0.0))
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
        "ConditionalValueAtRisk"
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
            ConditionalValueAtRisk::new(1, 0.95),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            ConditionalValueAtRisk::new(20, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            ConditionalValueAtRisk::new(20, 1.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let c = ConditionalValueAtRisk::new(100, 0.95).unwrap();
        assert_eq!(c.period(), 100);
        assert_relative_eq!(c.confidence(), 0.95, epsilon = 1e-12);
        assert_eq!(c.name(), "ConditionalValueAtRisk");
        assert_eq!(c.warmup_period(), 100);
    }

    #[test]
    fn reference_value() {
        // 20 returns -10..9 (each *0.01); confidence 0.95.
        // q = 0.05, n = 20, k = floor(0.05*20) = 1.
        // Tail = {-0.10}, CVaR = 0.10.
        let mut c = ConditionalValueAtRisk::new(20, 0.95).unwrap();
        let returns: Vec<f64> = (-10..10).map(|i| f64::from(i) * 0.01).collect();
        let out = c.batch(&returns);
        assert_relative_eq!(out[19].unwrap(), 0.10, epsilon = 1e-9);
    }

    #[test]
    fn cvar_geq_var_on_same_window() {
        // Sanity: with confidence 0.9, the tail of 10 returns has 1 sample;
        // VaR uses interpolation between 0 and 1, so CVaR (mean of just the
        // worst) >= VaR.
        use crate::ValueAtRisk;
        let returns: Vec<f64> = vec![
            -0.05, -0.02, -0.01, 0.0, 0.005, 0.01, 0.02, 0.03, 0.04, 0.05,
        ];
        let mut v = ValueAtRisk::new(10, 0.9).unwrap();
        let mut c = ConditionalValueAtRisk::new(10, 0.9).unwrap();
        let v_out = v.batch(&returns);
        let c_out = c.batch(&returns);
        let var = v_out[9].unwrap();
        let cvar = c_out[9].unwrap();
        assert!(cvar >= var - 1e-12, "CVaR {cvar} should be >= VaR {var}");
    }

    #[test]
    fn all_positive_returns_yield_zero() {
        let mut c = ConditionalValueAtRisk::new(5, 0.95).unwrap();
        let out = c.batch(&[0.01, 0.02, 0.03, 0.04, 0.05]);
        assert_eq!(out[4], Some(0.0));
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut c = ConditionalValueAtRisk::new(3, 0.95).unwrap();
        assert_eq!(c.update(f64::NAN), None);
        assert_eq!(c.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut c = ConditionalValueAtRisk::new(3, 0.95).unwrap();
        c.batch(&[-0.01, -0.02, -0.03]);
        assert!(c.is_ready());
        c.reset();
        assert!(!c.is_ready());
        assert_eq!(c.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let returns: Vec<f64> = (0..50).map(|i| (f64::from(i) * 0.2).sin() * 0.02).collect();
        let batch = ConditionalValueAtRisk::new(10, 0.95)
            .unwrap()
            .batch(&returns);
        let mut s = ConditionalValueAtRisk::new(10, 0.95).unwrap();
        let streamed: Vec<_> = returns.iter().map(|r| s.update(*r)).collect();
        assert_eq!(batch, streamed);
    }
}
