//! Rolling Treynor Ratio.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedPairMoments;
use crate::traits::Indicator;

/// Rolling Treynor Ratio.
///
/// Each `update` receives one `(asset_return, benchmark_return)` pair. Over
/// the trailing window of `period` pairs:
///
/// ```text
/// cov_ab = (1/n) · Σ a·b − ā·b̄
/// var_b  = (1/n) · Σ b² − b̄²
/// Beta   = cov_ab / var_b
/// Treynor = (mean(asset) − risk_free) / Beta
/// ```
///
/// Treynor is Sharpe's market-risk cousin: it divides excess return by the
/// asset's sensitivity to the benchmark (Beta) rather than by the asset's
/// own volatility. Useful for diversified portfolios where idiosyncratic
/// volatility has been mostly diversified away and the dominant remaining
/// risk is systematic / market exposure.
///
/// A flat benchmark window has zero variance and the indicator returns
/// `0.0` rather than `NaN`. A near-zero `Beta` makes the ratio explode by
/// construction; callers should treat extreme values with the usual care.
///
/// Each `update` is O(1) — running sums maintain `Σa`, `Σb`, `Σb²`, `Σa·b`
/// as the window slides.
#[derive(Debug, Clone)]
pub struct TreynorRatio {
    period: usize,
    risk_free: f64,
    window: VecDeque<(f64, f64)>,
    moments: ShiftedPairMoments,
}

impl TreynorRatio {
    /// Construct a new rolling Treynor Ratio.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2`.
    pub fn new(period: usize, risk_free: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "treynor ratio needs period >= 2",
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

impl Indicator for TreynorRatio {
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
        let var_b = self.moments.var_b(self.period);
        if var_b <= 0.0 {
            return Some(0.0);
        }
        let cov_ab = self.moments.cov(self.period);
        let beta = cov_ab / var_b;
        if beta == 0.0 {
            return Some(0.0);
        }
        Some((mean_a - self.risk_free) / beta)
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
        "TreynorRatio"
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
            TreynorRatio::new(1, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let t = TreynorRatio::new(20, 0.001).unwrap();
        assert_eq!(t.period(), 20);
        assert_relative_eq!(t.risk_free(), 0.001, epsilon = 1e-12);
        assert_eq!(t.name(), "TreynorRatio");
        assert_eq!(t.warmup_period(), 20);
    }

    #[test]
    fn reference_beta_two_payoff() {
        // a_i = 2 * b_i with non-zero mean.
        // Beta should be 2; mean_a = 2 * mean_b; Treynor = mean_b.
        let mut t = TreynorRatio::new(20, 0.0).unwrap();
        let inputs: Vec<(f64, f64)> = (1..=20)
            .map(|i| (2.0 * f64::from(i) * 0.01, f64::from(i) * 0.01))
            .collect();
        let out = t.batch(&inputs);
        let last = out[19].unwrap();
        let expected = inputs.iter().map(|(_, b)| *b).sum::<f64>() / 20.0;
        assert_relative_eq!(last, expected, epsilon = 1e-9);
    }

    #[test]
    fn flat_benchmark_yields_zero() {
        // Benchmark all 0 -> var_b = 0 -> indicator returns 0.0.
        let mut t = TreynorRatio::new(4, 0.0).unwrap();
        let out = t.batch(&[(0.01, 0.0), (0.02, 0.0), (-0.01, 0.0), (0.03, 0.0)]);
        assert_eq!(out[3], Some(0.0));
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut t = TreynorRatio::new(3, 0.0).unwrap();
        assert_eq!(t.update((f64::NAN, 0.0)), None);
        assert_eq!(t.update((0.0, f64::INFINITY)), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut t = TreynorRatio::new(3, 0.0).unwrap();
        t.batch(&[(0.01, 0.005), (0.02, 0.01), (-0.01, -0.005)]);
        assert!(t.is_ready());
        t.reset();
        assert!(!t.is_ready());
        assert_eq!(t.update((0.01, 0.005)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let inputs: Vec<(f64, f64)> = (0..50)
            .map(|i| {
                let b = (f64::from(i) * 0.2).sin() * 0.01;
                (1.5 * b + 0.001, b)
            })
            .collect();
        let batch = TreynorRatio::new(10, 0.0).unwrap().batch(&inputs);
        let mut s = TreynorRatio::new(10, 0.0).unwrap();
        let streamed: Vec<_> = inputs.iter().map(|x| s.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn zero_beta_returns_zero() {
        // Constant asset returns vs varying benchmark force cov(a,b) = 0,
        // hence beta = 0 — the explicit zero-beta short-circuit.
        let mut t = TreynorRatio::new(4, 0.0).unwrap();
        let pairs: [(f64, f64); 4] = [(0.01, 0.005), (0.01, -0.002), (0.01, 0.001), (0.01, 0.003)];
        let mut last = None;
        for p in pairs {
            last = t.update(p);
        }
        assert_eq!(last, Some(0.0));
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            TreynorRatio::new(crate::error::MAX_PERIOD + 1, 0.001),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
