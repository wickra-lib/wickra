//! Rolling Jensen's Alpha (CAPM).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedPairMoments;
use crate::traits::Indicator;

/// Rolling Jensen's Alpha.
///
/// Each `update` receives one `(asset_return, benchmark_return)` pair. Over
/// the trailing window of `period` pairs:
///
/// ```text
/// Beta  = cov(asset, bench) / var(bench)
/// Alpha = mean(asset) − ( risk_free + Beta · (mean(bench) − risk_free) )
/// ```
///
/// Alpha is the *risk-adjusted excess return* — the slice of the asset's
/// performance that cannot be explained by simple exposure to the
/// benchmark. A positive alpha indicates outperformance net of the market
/// premium implied by the asset's beta; negative alpha is the opposite.
///
/// Population covariance and variance are used (matching common
/// implementations in pandas-ta / quantstats); the rolling estimator stays
/// unbiased in the steady state for fixed `period`.
///
/// If the benchmark is flat (`var(bench) = 0`) the indicator falls back to
/// `alpha = mean(asset) − risk_free` — the asset's mean excess return, with
/// no market-risk adjustment, since the regression slope is undefined.
///
/// Each `update` is O(1).
#[derive(Debug, Clone)]
pub struct Alpha {
    period: usize,
    risk_free: f64,
    window: VecDeque<(f64, f64)>,
    moments: ShiftedPairMoments,
}

impl Alpha {
    /// Construct a new rolling Alpha.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2`.
    pub fn new(period: usize, risk_free: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "alpha needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            risk_free,
            window: VecDeque::with_capacity(period),
            moments: ShiftedPairMoments::new(),
        })
    }

    /// Configured window length.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured per-period risk-free rate.
    pub const fn risk_free(&self) -> f64 {
        self.risk_free
    }
}

impl Indicator for Alpha {
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
        let mean_a = self.moments.mean_a(self.period);
        let mean_b = self.moments.mean_b(self.period);
        let var_b = self.moments.var_b(self.period);
        if var_b <= 0.0 {
            // Undefined beta: report unadjusted excess.
            return Some(mean_a - self.risk_free);
        }
        let cov_ab = self.moments.cov(self.period);
        let beta = cov_ab / var_b;
        Some(mean_a - (self.risk_free + beta * (mean_b - self.risk_free)))
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
        "Alpha"
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
            Alpha::new(1, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let a = Alpha::new(20, 0.001).unwrap();
        assert_eq!(a.period(), 20);
        assert_relative_eq!(a.risk_free(), 0.001, epsilon = 1e-12);
        assert_eq!(a.name(), "Alpha");
        assert_eq!(a.warmup_period(), 20);
    }

    #[test]
    fn capm_perfect_fit_yields_zero_alpha() {
        // asset = 2 * bench - constant beta of 2, no alpha; with rf = 0 the
        // CAPM-implied return matches the asset's mean perfectly.
        let mut a = Alpha::new(20, 0.0).unwrap();
        let inputs: Vec<(f64, f64)> = (1..=20)
            .map(|i| (2.0 * f64::from(i) * 0.01, f64::from(i) * 0.01))
            .collect();
        let out = a.batch(&inputs);
        assert_relative_eq!(out[19].unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn constant_alpha_offset_recovered() {
        // asset = bench + 0.005 (additive alpha of 0.5%), beta == 1.
        // Expected alpha = 0.005.
        let mut a = Alpha::new(20, 0.0).unwrap();
        let inputs: Vec<(f64, f64)> = (1..=20)
            .map(|i| (f64::from(i) * 0.01 + 0.005, f64::from(i) * 0.01))
            .collect();
        let out = a.batch(&inputs);
        assert_relative_eq!(out[19].unwrap(), 0.005, epsilon = 1e-9);
    }

    #[test]
    fn flat_benchmark_falls_back_to_excess_return() {
        // Benchmark all 0 -> beta undefined -> alpha = mean_a - rf.
        let mut a = Alpha::new(4, 0.001).unwrap();
        let out = a.batch(&[(0.01, 0.0), (0.02, 0.0), (-0.01, 0.0), (0.04, 0.0)]);
        let mean = (0.01 + 0.02 - 0.01 + 0.04) / 4.0;
        assert_relative_eq!(out[3].unwrap(), mean - 0.001, epsilon = 1e-12);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut a = Alpha::new(3, 0.0).unwrap();
        assert_eq!(a.update((f64::NAN, 0.0)), None);
        assert_eq!(a.update((0.0, f64::INFINITY)), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut a = Alpha::new(3, 0.0).unwrap();
        a.batch(&[(0.01, 0.005), (0.02, 0.01), (-0.01, -0.005)]);
        assert!(a.is_ready());
        a.reset();
        assert!(!a.is_ready());
        assert_eq!(a.update((0.01, 0.005)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let inputs: Vec<(f64, f64)> = (0..50)
            .map(|i| {
                let b = (f64::from(i) * 0.2).sin() * 0.01;
                (1.5 * b + 0.002, b)
            })
            .collect();
        let batch = Alpha::new(10, 0.0).unwrap().batch(&inputs);
        let mut s = Alpha::new(10, 0.0).unwrap();
        let streamed: Vec<_> = inputs.iter().map(|x| s.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Alpha::new(crate::error::MAX_PERIOD + 1, 0.001),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
