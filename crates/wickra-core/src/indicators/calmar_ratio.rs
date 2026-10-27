//! Rolling Calmar Ratio — return over max drawdown.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::traits::Indicator;

/// Rolling Calmar Ratio.
///
/// Input is treated as a single period return. Over the trailing window of
/// `period` returns the indicator reconstructs the implied equity curve
/// (cumulative-compounded), measures the worst peak-to-trough drawdown, and
/// divides the mean return by that drawdown:
///
/// ```text
/// equity_t = ∏(1 + r_i) for i in window up to t
/// mdd      = max peak-to-trough decline of equity over window
/// Calmar   = mean(returns) / mdd
/// ```
///
/// If the drawdown is zero (monotonically non-decreasing equity in the
/// window) the indicator returns `0.0` rather than `NaN` / `Inf`.
///
/// The equity curve is recomputed inside the window each `update`, which
/// keeps each call O(period) — acceptable for typical backtest windows
/// (`period ≤ 252`).
///
/// # Example
///
/// ```
/// use wickra_core::{CalmarRatio, Indicator};
///
/// let mut cr = CalmarRatio::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = cr.update(0.001 + (f64::from(i) * 0.1).sin() * 0.005);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct CalmarRatio {
    period: usize,
    window: VecDeque<f64>,
    sum: RollingSum,
}

impl CalmarRatio {
    /// Construct a new rolling Calmar Ratio.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2`.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "calmar ratio needs period >= 2",
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
            sum: RollingSum::new(),
        })
    }

    /// Configured window length.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for CalmarRatio {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("non-empty");
            self.sum.evict(old);
        }
        self.window.push_back(input);
        self.sum.push(input);
        if self.sum.needs_reseed(self.period) {
            self.sum.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let n = self.period as f64;
        let mean = self.sum.value() / n;
        // Build equity curve and track the worst peak-to-trough drawdown.
        let mut equity = 1.0_f64;
        let mut peak = 1.0_f64;
        let mut mdd = 0.0_f64;
        for &r in &self.window {
            equity *= 1.0 + r;
            if equity > peak {
                peak = equity;
            }
            // peak starts at 1.0 and never decreases, so peak > 0 by construction.
            let dd = (peak - equity) / peak;
            if dd > mdd {
                mdd = dd;
            }
        }
        if mdd == 0.0 {
            return Some(0.0);
        }
        Some(mean / mdd)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.sum.reset();
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
        "CalmarRatio"
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
            CalmarRatio::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let c = CalmarRatio::new(10).unwrap();
        assert_eq!(c.period(), 10);
        assert_eq!(c.name(), "CalmarRatio");
        assert_eq!(c.warmup_period(), 10);
    }

    #[test]
    fn pure_uptrend_yields_zero() {
        // All positive returns -> no drawdown -> Calmar = 0 by convention.
        let mut c = CalmarRatio::new(5).unwrap();
        let out = c.batch(&[0.01; 10]);
        for v in out.into_iter().flatten() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn reference_value() {
        // returns = [0.10, -0.20, 0.05]
        // equity: 1.0 -> 1.10 -> 0.88 -> 0.924
        // peak 1.10, trough 0.88 -> mdd = 0.20.
        // mean = (0.10 - 0.20 + 0.05) / 3 ≈ -0.01666...
        // Calmar = -0.01666... / 0.20 ≈ -0.08333...
        let mut c = CalmarRatio::new(3).unwrap();
        let out = c.batch(&[0.10, -0.20, 0.05]);
        let mean = (0.10 - 0.20 + 0.05) / 3.0;
        let expected = mean / 0.20;
        assert_relative_eq!(out[2].unwrap(), expected, epsilon = 1e-9);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut c = CalmarRatio::new(3).unwrap();
        assert_eq!(c.update(f64::NAN), None);
        assert_eq!(c.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut c = CalmarRatio::new(3).unwrap();
        c.batch(&[0.10, -0.20, 0.05]);
        assert!(c.is_ready());
        c.reset();
        assert!(!c.is_ready());
        assert_eq!(c.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let returns: Vec<f64> = (0..50)
            .map(|i| 0.001 + (f64::from(i) * 0.25).sin() * 0.02)
            .collect();
        let batch = CalmarRatio::new(10).unwrap().batch(&returns);
        let mut s = CalmarRatio::new(10).unwrap();
        let streamed: Vec<_> = returns.iter().map(|r| s.update(*r)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            CalmarRatio::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
