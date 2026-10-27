//! Rolling Information Ratio.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::traits::Indicator;

/// Rolling Information Ratio.
///
/// Each `update` receives one `(asset_return, benchmark_return)` pair. Over
/// the trailing window of `period` pairs:
///
/// ```text
/// active_t       = asset_t − benchmark_t
/// tracking_error = stddev(active over window)            (sample)
/// IR             = mean(active) / tracking_error
/// ```
///
/// The Information Ratio quantifies skill in beating a benchmark per unit
/// of active-return volatility. A high IR means consistent (low-noise)
/// outperformance; a near-zero IR means the asset moves with the benchmark
/// regardless of any small alpha.
///
/// If the tracking error is zero (asset perfectly tracks the benchmark over
/// the window) the indicator returns `0.0` rather than `NaN`.
///
/// Each `update` is O(1).
#[derive(Debug, Clone)]
pub struct InformationRatio {
    period: usize,
    window: VecDeque<f64>,
    moments: ShiftedMoments,
}

impl InformationRatio {
    /// Construct a new rolling Information Ratio.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2`.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "information ratio needs period >= 2",
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
            moments: ShiftedMoments::new(),
        })
    }

    /// Configured window length.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for InformationRatio {
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (a, b) = input;
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        let active = a - b;
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("non-empty");
            self.moments.evict(old);
        }
        self.window.push_back(active);
        self.moments.push(active);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let mean = self.moments.mean(self.period);
        let var = self.moments.sample_variance(self.period);
        let te = var.sqrt();
        if te == 0.0 {
            return Some(0.0);
        }
        Some(mean / te)
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
        "InformationRatio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_less_than_two() {
        assert!(matches!(
            InformationRatio::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let i = InformationRatio::new(10).unwrap();
        assert_eq!(i.period(), 10);
        assert_eq!(i.name(), "InformationRatio");
        assert_eq!(i.warmup_period(), 10);
    }

    #[test]
    fn perfect_tracking_yields_zero() {
        // asset == benchmark every bar -> active = 0 -> te = 0 -> 0.
        let mut i = InformationRatio::new(5).unwrap();
        let inputs: Vec<(f64, f64)> = (0..5)
            .map(|j| (f64::from(j) * 0.01, f64::from(j) * 0.01))
            .collect();
        let out = i.batch(&inputs);
        assert_eq!(out[4], Some(0.0));
    }

    #[test]
    fn reference_value() {
        // asset=[0.02,0.04,0.06,0.08], bench=[0.01,0.02,0.03,0.04].
        // active=[0.01,0.02,0.03,0.04]; mean=0.025;
        // var = ((0.01-.025)^2 + ... ) / 3 = 0.0001666...;
        // te = sqrt(0.0001666...); IR = 0.025/te.
        let mut i = InformationRatio::new(4).unwrap();
        let inputs = vec![(0.02, 0.01), (0.04, 0.02), (0.06, 0.03), (0.08, 0.04)];
        let out = i.batch(&inputs);
        let expected = 0.025 / (0.000_166_666_666_666_666_67_f64).sqrt();
        assert_relative_eq!(out[3].unwrap(), expected, epsilon = 1e-9);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut i = InformationRatio::new(3).unwrap();
        assert_eq!(i.update((f64::NAN, 0.01)), None);
        assert_eq!(i.update((0.01, f64::INFINITY)), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut i = InformationRatio::new(3).unwrap();
        i.batch(&[(0.01, 0.005), (0.02, 0.01), (-0.01, -0.005)]);
        assert!(i.is_ready());
        i.reset();
        assert!(!i.is_ready());
        assert_eq!(i.update((0.01, 0.005)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let inputs: Vec<(f64, f64)> = (0..50)
            .map(|j| {
                let b = (f64::from(j) * 0.2).sin() * 0.01;
                (b + 0.001, b)
            })
            .collect();
        let batch = InformationRatio::new(10).unwrap().batch(&inputs);
        let mut s = InformationRatio::new(10).unwrap();
        let streamed: Vec<_> = inputs.iter().map(|x| s.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            InformationRatio::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
