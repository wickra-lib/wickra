//! Rolling Sharpe Ratio.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::traits::Indicator;

/// Rolling Sharpe Ratio over `period` period-returns.
///
/// The input is treated as a single period-return (e.g. one day's percentage
/// return). Over the trailing window of `period` returns the indicator
/// computes:
///
/// ```text
/// Sharpe = (mean(returns) − risk_free_per_period) / stddev(returns)
/// ```
///
/// `stddev` is the sample standard deviation with `n − 1` in the denominator.
/// `risk_free_per_period` is the per-period risk-free rate the caller supplies
/// (e.g. `0.0` for excess-of-zero or a daily-equivalent rate to match the
/// return frequency). Wickra does not annualise: feed already-annualised
/// returns and supply an annual risk-free rate if you want an annualised
/// Sharpe.
///
/// A flat window has zero standard deviation and Sharpe is undefined; the
/// indicator returns `0.0` in that case rather than producing `NaN`.
///
/// Each `update` is O(1) — Welford-style running sums maintain `Σr`, `Σr²`
/// as the window slides.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, SharpeRatio};
///
/// let mut sr = SharpeRatio::new(20, 0.0).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = sr.update(0.001 + (f64::from(i) * 0.1).sin() * 0.01);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct SharpeRatio {
    period: usize,
    risk_free: f64,
    window: VecDeque<f64>,
    moments: ShiftedMoments,
}

impl SharpeRatio {
    /// Construct a new rolling Sharpe Ratio with the given window and
    /// per-period risk-free rate.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` (sample standard
    /// deviation needs at least two observations).
    pub fn new(period: usize, risk_free: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "sharpe ratio needs period >= 2",
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
            moments: ShiftedMoments::new(),
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

impl Indicator for SharpeRatio {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("non-empty");
            self.moments.evict(old);
        }
        self.window.push_back(input);
        self.moments.push(input);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let mean = self.moments.mean(self.period);
        let var = self.moments.sample_variance(self.period);
        let sd = var.sqrt();
        if sd == 0.0 {
            return Some(0.0);
        }
        Some((mean - self.risk_free) / sd)
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
        "SharpeRatio"
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
            SharpeRatio::new(1, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            SharpeRatio::new(0, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let sr = SharpeRatio::new(20, 0.001).unwrap();
        assert_eq!(sr.period(), 20);
        assert_relative_eq!(sr.risk_free(), 0.001, epsilon = 1e-12);
        assert_eq!(sr.name(), "SharpeRatio");
        assert_eq!(sr.warmup_period(), 20);
    }

    #[test]
    fn constant_returns_yield_zero() {
        let mut sr = SharpeRatio::new(5, 0.0).unwrap();
        let out = sr.batch(&[0.01; 10]);
        for v in out.into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn reference_value() {
        // returns = [0.01, 0.02, 0.03, 0.04], rf = 0.
        // mean = 0.025, var = ((0.01-.025)^2 + (.02-.025)^2 + (.03-.025)^2
        // + (.04-.025)^2) / 3 = 0.00016666..., sd = sqrt(0.000166..) =
        // 0.01290994..., Sharpe = 0.025 / 0.01290994 ≈ 1.936491673.
        let mut sr = SharpeRatio::new(4, 0.0).unwrap();
        let out = sr.batch(&[0.01, 0.02, 0.03, 0.04]);
        let expected = 0.025_f64 / (0.000_166_666_666_666_666_67_f64).sqrt();
        assert_relative_eq!(out[3].unwrap(), expected, epsilon = 1e-9);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut sr = SharpeRatio::new(3, 0.0).unwrap();
        assert_eq!(sr.update(0.01), None);
        assert_eq!(sr.update(f64::NAN), None);
        assert_eq!(sr.update(0.02), None);
        assert!(sr.update(0.03).is_some());
    }

    #[test]
    fn warmup_returns_none() {
        let mut sr = SharpeRatio::new(5, 0.0).unwrap();
        for i in 0..4 {
            assert_eq!(sr.update(f64::from(i) * 0.01), None);
        }
        assert!(sr.update(0.05).is_some());
    }

    #[test]
    fn reset_clears_state() {
        let mut sr = SharpeRatio::new(3, 0.0).unwrap();
        sr.batch(&[0.01, 0.02, 0.03]);
        assert!(sr.is_ready());
        sr.reset();
        assert!(!sr.is_ready());
        assert_eq!(sr.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let returns: Vec<f64> = (0..50)
            .map(|i| 0.001 + (f64::from(i) * 0.2).sin() * 0.01)
            .collect();
        let batch = SharpeRatio::new(10, 0.0).unwrap().batch(&returns);
        let mut s = SharpeRatio::new(10, 0.0).unwrap();
        let streamed: Vec<_> = returns.iter().map(|p| s.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            SharpeRatio::new(crate::error::MAX_PERIOD + 1, 0.001),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
