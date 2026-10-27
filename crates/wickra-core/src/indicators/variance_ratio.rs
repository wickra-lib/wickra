//! Lo–MacKinlay variance-ratio test on the spread of two series.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Lo–MacKinlay variance ratio of the spread `a − b` at horizon `q`.
///
/// Each `update` takes one `(a, b)` price pair and forms the spread
/// `sₜ = aₜ − bₜ`. Over the trailing window of `period` spreads the indicator
/// compares the variance of `q`-step changes against `q` times the variance of
/// one-step changes:
///
/// ```text
/// rₜ   = sₜ − sₜ₋₁                                  (one-step changes)
/// VR(q) = Var(Σ of q consecutive r) / (q · Var(r))
/// ```
///
/// Under a random walk the variance of returns grows linearly with the horizon,
/// so `VR(q) = 1`. Departures reveal autocorrelation structure:
///
/// * `VR(q) < 1` — **mean reversion** (negatively autocorrelated changes): the
///   spread's moves partly cancel, the regime pairs traders exploit.
/// * `VR(q) ≈ 1` — a **random walk**: no exploitable structure.
/// * `VR(q) > 1` — **momentum / trending** (positively autocorrelated changes).
///
/// The estimator uses overlapping `q`-step windows. When the one-step changes
/// have zero variance (a flat spread) the ratio is undefined and the indicator
/// returns the null value `1`. The output is always `≥ 0`.
///
/// Each `update` is `O(period)`, bounded by the fixed window.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, VarianceRatio};
///
/// let mut vr = VarianceRatio::new(60, 2).unwrap();
/// let mut last = None;
/// for t in 0..200 {
///     let b = 100.0 + f64::from(t);
///     // A fast, choppy spread mean-reverts (negatively autocorrelated
///     // changes) ⇒ VR(2) < 1.
///     let a = b + 2.0 * (f64::from(t) * 2.5).sin();
///     last = vr.update((a, b));
/// }
/// assert!(last.unwrap() < 1.0);
/// ```
#[derive(Debug, Clone)]
pub struct VarianceRatio {
    period: usize,
    q: usize,
    window: VecDeque<f64>,
    /// Reusable scratch buffer to avoid allocating per `update`.
    scratch: Vec<f64>,
    /// Reusable buffer for the one-step changes.
    returns: Vec<f64>,
}

impl VarianceRatio {
    /// Construct a new variance-ratio test.
    ///
    /// `period` is the look-back window of spreads; `q` is the aggregation
    /// horizon (number of one-step changes summed per long-horizon change).
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `q < 2` or if `period < q + 2`
    /// (which would leave fewer than two long-horizon observations).
    pub fn new(period: usize, q: usize) -> Result<Self> {
        if q < 2 {
            return Err(Error::InvalidPeriod {
                message: "variance ratio needs q >= 2",
            });
        }
        if q > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if period < q + 2 {
            return Err(Error::InvalidPeriod {
                message: "variance ratio needs period >= q + 2",
            });
        }
        Ok(Self {
            period,
            q,
            window: VecDeque::with_capacity(period),
            scratch: Vec::with_capacity(period),
            returns: Vec::with_capacity(period),
        })
    }

    /// Configured look-back window of spreads.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured aggregation horizon `q`.
    pub const fn q(&self) -> usize {
        self.q
    }
}

impl Indicator for VarianceRatio {
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (a, b) = input;
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(a - b);
        if self.window.len() < self.period {
            return None;
        }
        self.scratch.clear();
        self.scratch.extend(self.window.iter().copied());
        // One-step changes.
        let returns = &mut self.returns;
        returns.clear();
        returns.extend(self.scratch.windows(2).map(|w| w[1] - w[0]));
        let m = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / m;
        let var_one = returns.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / m;
        if var_one <= 0.0 {
            // Flat spread: the random-walk null value.
            return Some(1.0);
        }
        // Overlapping q-step changes; their mean is q·mean by construction.
        let q_mean = self.q as f64 * mean;
        // Summed in one pass rather than materialised: the q-step changes are
        // only ever reduced, never revisited.
        let mut sum_sq = 0.0;
        let mut count = 0.0;
        for window in returns.windows(self.q) {
            let deviation = window.iter().sum::<f64>() - q_mean;
            sum_sq += deviation * deviation;
            count += 1.0;
        }
        let var_q = sum_sq / count;
        Some(var_q / (self.q as f64 * var_one))
    }

    fn reset(&mut self) {
        self.window.clear();
        self.scratch.clear();
        self.returns.clear();
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
        "VarianceRatio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_bad_parameters() {
        assert!(VarianceRatio::new(10, 1).is_err()); // q must be >= 2
        assert!(VarianceRatio::new(3, 2).is_err()); // period must be >= q + 2
        assert!(VarianceRatio::new(4, 2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let vr = VarianceRatio::new(60, 4).unwrap();
        assert_eq!(vr.period(), 60);
        assert_eq!(vr.q(), 4);
        assert_eq!(vr.warmup_period(), 60);
        assert_eq!(vr.name(), "VarianceRatio");
        assert!(!vr.is_ready());
    }

    #[test]
    fn warmup_returns_none() {
        let mut vr = VarianceRatio::new(4, 2).unwrap();
        assert_eq!(vr.update((1.0, 0.0)), None);
        assert_eq!(vr.update((2.0, 0.0)), None);
        assert_eq!(vr.update((3.0, 0.0)), None);
        assert!(vr.update((4.0, 0.0)).is_some());
        assert!(vr.is_ready());
    }

    #[test]
    fn alternating_changes_give_zero_ratio() {
        // Spreads 0,2,1,3,2 ⇒ changes 2,-1,2,-1; q = 2 overlapping sums are all
        // 1 (constant) ⇒ Var(q) = 0 ⇒ VR = 0 (perfect mean reversion).
        let pairs = [(0.0, 0.0), (2.0, 0.0), (1.0, 0.0), (3.0, 0.0), (2.0, 0.0)];
        let last = VarianceRatio::new(5, 2)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn oscillating_spread_is_below_one() {
        let pairs: Vec<(f64, f64)> = (0..200)
            .map(|t| {
                let b = 100.0 + f64::from(t);
                (b + 2.0 * (f64::from(t) * 2.5).sin(), b)
            })
            .collect();
        let last = VarianceRatio::new(60, 2)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last < 1.0, "VR {last}");
    }

    #[test]
    fn flat_spread_returns_one() {
        let pairs: Vec<(f64, f64)> = (0..30)
            .map(|t| (5.0 + f64::from(t), f64::from(t)))
            .collect();
        let last = VarianceRatio::new(10, 3)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(last, 1.0);
    }

    #[test]
    fn output_non_negative() {
        let pairs: Vec<(f64, f64)> = (0..150)
            .map(|t| {
                let b = 50.0 + 0.3 * f64::from(t);
                (b + (f64::from(t) * 0.5).sin() * 2.0, b)
            })
            .collect();
        let mut vr = VarianceRatio::new(40, 4).unwrap();
        for v in vr.batch(&pairs).into_iter().flatten() {
            assert!(v >= 0.0, "VR {v}");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut vr = VarianceRatio::new(6, 2).unwrap();
        for t in 0..12 {
            vr.update((f64::from(t) + (f64::from(t) * 0.7).sin(), f64::from(t)));
        }
        assert!(vr.is_ready());
        vr.reset();
        assert!(!vr.is_ready());
        assert_eq!(vr.update((1.0, 0.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..100)
            .map(|t| {
                let b = 30.0 + 0.7 * f64::from(t);
                (b + (f64::from(t) * 0.4).sin() * 1.5, b)
            })
            .collect();
        let batch = VarianceRatio::new(32, 3).unwrap().batch(&pairs);
        let mut vr = VarianceRatio::new(32, 3).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| vr.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut vr = VarianceRatio::new(4, 2).unwrap();
        assert_eq!(vr.update((f64::NAN, 1.0)), None);
        assert_eq!(vr.update((1.0, f64::INFINITY)), None);
        // The rejected ticks leave no trace: a fresh window still warms up.
        assert_eq!(vr.update((1.0, 0.0)), None);
        assert_eq!(vr.update((2.0, 0.0)), None);
        assert_eq!(vr.update((3.0, 0.0)), None);
        assert!(vr.update((4.0, 0.0)).is_some());
    }

    #[test]
    fn rejects_q_above_maximum() {
        assert!(matches!(
            VarianceRatio::new(4, crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
