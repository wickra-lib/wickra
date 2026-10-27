//! Rolling Beta — sensitivity of an asset to a benchmark.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedPairMoments;
use crate::traits::Indicator;

/// Rolling Beta of an `asset` series relative to a `benchmark` series.
///
/// Each `update` receives one `(asset, benchmark)` pair. Over the trailing
/// window of `period` pairs:
///
/// ```text
/// cov_ab = (1/n) · Σ a·b − ā·b̄
/// var_b  = (1/n) · Σ b² − b̄²
/// Beta   = cov_ab / var_b
/// ```
///
/// Beta measures how much the asset moves for a unit move in the
/// benchmark. A reading of `1.0` means the two move together one-for-one;
/// `2.0` means the asset typically doubles the benchmark's moves;
/// `0.5` means it moves only half as much; `0.0` means moves are
/// uncorrelated; negative Betas signal a hedge. It is the slope of the
/// OLS regression of the asset on the benchmark and the foundation of the
/// CAPM. Unlike [`crate::PearsonCorrelation`], Beta is *not* unit-free —
/// it carries the ratio of standard deviations.
///
/// Each `update` is O(1): four running sums (`Σa`, `Σb`, `Σb²`, `Σa·b`)
/// are maintained as the window slides. A flat benchmark window has zero
/// variance and Beta is undefined; the indicator returns `0` in that
/// case rather than producing `NaN`.
///
/// Conventionally Beta is computed on **returns** (typically log-returns)
/// rather than raw prices; feed the indicator pre-computed returns if
/// that is your convention. The pure rolling OLS slope is the same
/// either way.
///
/// # Example
///
/// ```
/// use wickra_core::{Beta, Indicator};
///
/// let mut indicator = Beta::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     // Asset doubles every benchmark move.
///     last = indicator.update((2.0 * f64::from(i), f64::from(i)));
/// }
/// assert!((last.unwrap() - 2.0).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct Beta {
    period: usize,
    window: VecDeque<(f64, f64)>,
    moments: ShiftedPairMoments,
}

impl Beta {
    /// Construct a new rolling Beta.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2`.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "beta needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            window: VecDeque::with_capacity(period),
            moments: ShiftedPairMoments::new(),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Beta {
    /// `(asset, benchmark)` pair.
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (a, b) = input;
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let (oa, ob) = self.window.pop_front().expect("non-empty");
            self.moments.evict(oa, ob);
        }
        self.window.push_back((a, b));
        self.moments.push(a, b);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let var_b = self.moments.var_b(self.period);
        let cov = self.moments.cov(self.period);
        if var_b == 0.0 {
            // A flat benchmark has no defined beta.
            return Some(0.0);
        }
        Some(cov / var_b)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.moments.reset();
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
        "Beta"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_two() {
        assert!(Beta::new(0).is_err());
        assert!(Beta::new(1).is_err());
        assert!(Beta::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let b = Beta::new(14).unwrap();
        assert_eq!(b.period(), 14);
        assert_eq!(b.warmup_period(), 14);
        assert_eq!(b.name(), "Beta");
    }

    #[test]
    fn perfect_two_to_one_relationship() {
        let pairs: Vec<(f64, f64)> = (0..10)
            .map(|i| (2.0 * f64::from(i), f64::from(i)))
            .collect();
        let last = Beta::new(5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 2.0, epsilon = 1e-9);
    }

    #[test]
    fn perfect_negative_one() {
        let pairs: Vec<(f64, f64)> = (0..10).map(|i| (-f64::from(i), f64::from(i))).collect();
        let last = Beta::new(5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, -1.0, epsilon = 1e-9);
    }

    #[test]
    fn constant_benchmark_yields_zero() {
        let pairs: Vec<(f64, f64)> = (0..10).map(|i| (f64::from(i), 7.0)).collect();
        let last = Beta::new(5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut b = Beta::new(5).unwrap();
        b.batch(&[(1.0, 2.0), (2.0, 4.0), (3.0, 6.0), (4.0, 8.0), (5.0, 10.0)]);
        assert!(b.is_ready());
        b.reset();
        assert!(!b.is_ready());
        assert_eq!(b.update((1.0, 1.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..60)
            .map(|i| {
                let t = f64::from(i);
                (t.sin() * 2.0 + 0.3 * t.cos(), t.sin())
            })
            .collect();
        let batch = Beta::new(14).unwrap().batch(&pairs);
        let mut b = Beta::new(14).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut b = Beta::new(3).unwrap();
        assert_eq!(b.update((f64::NAN, 1.0)), None);
        assert_eq!(b.update((1.0, f64::INFINITY)), None);
        // The rejected ticks leave no trace: a fresh window still warms up.
        assert_eq!(b.update((1.0, 2.0)), None);
        assert_eq!(b.update((2.0, 5.0)), None);
        assert!(b.update((3.0, 7.0)).is_some());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Beta::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
