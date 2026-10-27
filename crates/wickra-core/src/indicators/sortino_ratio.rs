//! Rolling Sortino Ratio — Sharpe with downside-only volatility.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::traits::Indicator;

/// Rolling Sortino Ratio.
///
/// Like the Sharpe Ratio but only penalises **downside** volatility — returns
/// below the minimum acceptable return (`mar`). The numerator is excess return
/// over `mar`; the denominator is the downside deviation:
///
/// ```text
/// downside_dev = sqrt( mean( min(0, r − mar)² over period ) )
/// Sortino      = (mean(r) − mar) / downside_dev
/// ```
///
/// Downside variance uses the population formula (`n` in the denominator)
/// since the negative-shortfall samples are treated as the full population.
/// If every return in the window is ≥ `mar` the downside deviation is `0`
/// and the indicator returns `0.0` rather than `NaN`.
///
/// Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, SortinoRatio};
///
/// let mut sr = SortinoRatio::new(20, 0.0).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = sr.update((f64::from(i) * 0.1).sin() * 0.01);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct SortinoRatio {
    period: usize,
    mar: f64,
    window: VecDeque<f64>,
    sum: RollingSum,
}

impl SortinoRatio {
    /// Construct a new rolling Sortino Ratio.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2`.
    pub fn new(period: usize, mar: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "sortino ratio needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            mar,
            window: VecDeque::with_capacity(period),
            sum: RollingSum::new(),
        })
    }

    /// Configured window length.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured minimum-acceptable return.
    pub const fn mar(&self) -> f64 {
        self.mar
    }
}

impl Indicator for SortinoRatio {
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
        let mut downside_sq = 0.0;
        for &r in &self.window {
            let d = r - self.mar;
            if d < 0.0 {
                downside_sq += d * d;
            }
        }
        let dd = (downside_sq / n).sqrt();
        if dd == 0.0 {
            return Some(0.0);
        }
        Some((mean - self.mar) / dd)
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
        "SortinoRatio"
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
            SortinoRatio::new(1, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let s = SortinoRatio::new(10, 0.001).unwrap();
        assert_eq!(s.period(), 10);
        assert_relative_eq!(s.mar(), 0.001, epsilon = 1e-12);
        assert_eq!(s.name(), "SortinoRatio");
        assert_eq!(s.warmup_period(), 10);
    }

    #[test]
    fn all_returns_above_mar_yields_zero_downside() {
        let mut s = SortinoRatio::new(5, 0.0).unwrap();
        let out = s.batch(&[0.01, 0.02, 0.03, 0.04, 0.05]);
        // Downside deviation is 0 -> indicator returns 0.0.
        assert_eq!(out[4], Some(0.0));
    }

    #[test]
    fn reference_value() {
        // returns = [-0.02, 0.01, -0.01, 0.03], mar = 0.
        // mean = 0.0025, downside_sq = (0.02)^2 + (0.01)^2 = 0.0005;
        // downside_dev = sqrt(0.0005 / 4) = sqrt(0.000125) ≈ 0.01118033...
        // Sortino = 0.0025 / 0.011180339887 ≈ 0.2236068.
        let mut s = SortinoRatio::new(4, 0.0).unwrap();
        let out = s.batch(&[-0.02, 0.01, -0.01, 0.03]);
        let expected = 0.0025 / (0.000_125_f64).sqrt();
        assert_relative_eq!(out[3].unwrap(), expected, epsilon = 1e-9);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut s = SortinoRatio::new(3, 0.0).unwrap();
        assert_eq!(s.update(f64::NAN), None);
        assert_eq!(s.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut s = SortinoRatio::new(3, 0.0).unwrap();
        s.batch(&[-0.01, -0.02, -0.005]);
        assert!(s.is_ready());
        s.reset();
        assert!(!s.is_ready());
        assert_eq!(s.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let returns: Vec<f64> = (0..50)
            .map(|i| 0.001 + (f64::from(i) * 0.3).sin() * 0.02)
            .collect();
        let batch = SortinoRatio::new(10, 0.0).unwrap().batch(&returns);
        let mut s = SortinoRatio::new(10, 0.0).unwrap();
        let streamed: Vec<_> = returns.iter().map(|r| s.update(*r)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            SortinoRatio::new(crate::error::MAX_PERIOD + 1, 0.001),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
