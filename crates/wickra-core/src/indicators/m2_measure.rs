//! M² / Modigliani–Modigliani measure — Sharpe expressed in benchmark return units.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::traits::Indicator;

/// M² (Modigliani–Modigliani) measure over a trailing window of `period` returns.
///
/// ```text
/// Sharpe = (mean(returns) − risk_free) / stddev(returns)
/// M²     = risk_free + Sharpe · benchmark_stddev
/// ```
///
/// The [`SharpeRatio`](crate::SharpeRatio) is dimensionless, which makes it hard to
/// communicate: "0.8" means little to a client. M² rescales the Sharpe ratio back
/// into *return units* by levering (or de-levering) the portfolio to the
/// benchmark's volatility. The result answers a concrete question: "if this
/// strategy had run at the market's risk level, what return would it have
/// produced?" Two portfolios can then be ranked on the same risk-adjusted scale,
/// and M² preserves the Sharpe ordering while being quoted as a percentage.
///
/// `stddev` is the sample standard deviation (Bessel's `n − 1`).
/// `risk_free` is the per-period risk-free rate and `benchmark_stddev` the
/// per-period volatility of the benchmark, both supplied by the caller at the
/// return frequency. A flat window has zero volatility and the Sharpe ratio is
/// undefined; the indicator returns `0.0` in that case rather than producing `NaN`.
///
/// Each `update` is O(1) — running sums maintain `Σr` and `Σr²` as the window slides.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, M2Measure};
///
/// let mut indicator = M2Measure::new(20, 0.0, 0.02).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(0.001 + (f64::from(i) * 0.1).sin() * 0.01);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct M2Measure {
    period: usize,
    risk_free: f64,
    benchmark_stddev: f64,
    window: VecDeque<f64>,
    moments: ShiftedMoments,
}

impl M2Measure {
    /// Construct an M² measure over `period` returns with the given per-period
    /// risk-free rate and benchmark standard deviation.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `period < 2`, or
    /// [`Error::InvalidParameter`] if `risk_free` is not finite or
    /// `benchmark_stddev` is negative or not finite.
    pub fn new(period: usize, risk_free: f64, benchmark_stddev: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "m2 measure needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !risk_free.is_finite() || !benchmark_stddev.is_finite() || benchmark_stddev < 0.0 {
            return Err(Error::InvalidParameter {
                message: "risk_free must be finite and benchmark_stddev finite and non-negative",
            });
        }
        Ok(Self {
            period,
            risk_free,
            benchmark_stddev,
            window: VecDeque::with_capacity(period),
            moments: ShiftedMoments::new(),
        })
    }

    /// Configured window of returns.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured per-period risk-free rate.
    pub const fn risk_free(&self) -> f64 {
        self.risk_free
    }

    /// Configured per-period benchmark standard deviation.
    pub const fn benchmark_stddev(&self) -> f64 {
        self.benchmark_stddev
    }
}

impl Indicator for M2Measure {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, ret: f64) -> Option<f64> {
        if !ret.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("non-empty");
            self.moments.evict(old);
        }
        self.window.push_back(ret);
        self.moments.push(ret);
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
        let sharpe = (mean - self.risk_free) / sd;
        Some(self.risk_free + sharpe * self.benchmark_stddev)
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
        "M2Measure"
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
            M2Measure::new(1, 0.0, 0.02),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn rejects_invalid_benchmark_stddev() {
        assert!(matches!(
            M2Measure::new(10, 0.0, -0.01),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            M2Measure::new(10, f64::NAN, 0.02),
            Err(Error::InvalidParameter { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let m2 = M2Measure::new(20, 0.001, 0.02).unwrap();
        assert_eq!(m2.period(), 20);
        assert_relative_eq!(m2.risk_free(), 0.001, epsilon = 1e-12);
        assert_relative_eq!(m2.benchmark_stddev(), 0.02, epsilon = 1e-12);
        assert_eq!(m2.warmup_period(), 20);
        assert_eq!(m2.name(), "M2Measure");
    }

    #[test]
    fn reference_value() {
        // returns [0.01, 0.02, 0.03, 0.04], rf = 0, benchmark_stddev = 0.02.
        // mean = 0.025, sd = sqrt(0.000166666...), Sharpe = 0.025 / sd.
        // M2 = 0 + Sharpe * 0.02.
        let mut m2 = M2Measure::new(4, 0.0, 0.02).unwrap();
        let out = m2.batch(&[0.01, 0.02, 0.03, 0.04]);
        let sharpe = 0.025_f64 / (0.000_166_666_666_666_666_67_f64).sqrt();
        assert_relative_eq!(out[3].unwrap(), sharpe * 0.02, epsilon = 1e-9);
    }

    #[test]
    fn constant_returns_yield_zero() {
        let mut m2 = M2Measure::new(5, 0.0, 0.02).unwrap();
        for v in m2.batch(&[0.01; 10]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut m2 = M2Measure::new(3, 0.0, 0.02).unwrap();
        assert_eq!(m2.update(0.01), None);
        assert_eq!(m2.update(f64::NAN), None);
        assert_eq!(m2.update(0.02), None);
        assert!(m2.update(0.03).is_some());
    }

    #[test]
    fn reset_clears_state() {
        let mut m2 = M2Measure::new(3, 0.0, 0.02).unwrap();
        m2.batch(&[0.01, 0.02, 0.03]);
        assert!(m2.is_ready());
        m2.reset();
        assert!(!m2.is_ready());
        assert_eq!(m2.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let rets: Vec<f64> = (0..50)
            .map(|i| 0.001 + (f64::from(i) * 0.2).sin() * 0.01)
            .collect();
        let batch = M2Measure::new(10, 0.0, 0.02).unwrap().batch(&rets);
        let mut streamer = M2Measure::new(10, 0.0, 0.02).unwrap();
        let streamed: Vec<_> = rets.iter().map(|r| streamer.update(*r)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            M2Measure::new(crate::error::MAX_PERIOD + 1, 0.001, 0.02),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
