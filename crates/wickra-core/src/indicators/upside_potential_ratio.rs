//! Upside Potential Ratio (Sortino, van der Meer & Plantinga) — upside mean over downside deviation.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Upside Potential Ratio over a trailing window of `period` returns, measured
/// relative to a minimal acceptable return (`mar`).
///
/// ```text
/// upside     = mean( max(r − mar, 0) )            over the window
/// downside   = sqrt( mean( min(r − mar, 0)² ) )   over the window
/// UPR        = upside / downside
/// ```
///
/// Where the [`SharpeRatio`](crate::SharpeRatio) divides excess return by *total*
/// volatility (penalising upside and downside symmetrically), the Upside Potential
/// Ratio rewards only the average outperformance above the threshold while
/// penalising solely the downside deviation below it. It is the purest expression
/// of the Sortino philosophy: investors do not dislike upside variance, only
/// shortfall risk.
///
/// `mar` (minimal acceptable return) is the per-period hurdle the caller supplies
/// (e.g. `0.0` for break-even, or a target rate matching the return frequency). A
/// window that never breaches the threshold has zero downside deviation; the
/// indicator then reports `0.0` rather than dividing by zero.
///
/// Each `update` is O(1) — running sums maintain the upside total and the
/// downside sum-of-squares as the window slides.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, UpsidePotentialRatio};
///
/// let mut indicator = UpsidePotentialRatio::new(20, 0.0).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update((f64::from(i) * 0.3).sin() * 0.02);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct UpsidePotentialRatio {
    period: usize,
    mar: f64,
    window: VecDeque<f64>,
    sum_upside: f64,
    sum_downside_sq: f64,
}

impl UpsidePotentialRatio {
    /// Construct an Upside Potential Ratio over `period` returns with minimal
    /// acceptable return `mar`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `period < 2`, or
    /// [`Error::InvalidParameter`] if `mar` is not finite.
    pub fn new(period: usize, mar: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "upside potential ratio needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !mar.is_finite() {
            return Err(Error::InvalidParameter {
                message: "mar must be finite",
            });
        }
        Ok(Self {
            period,
            mar,
            window: VecDeque::with_capacity(period),
            sum_upside: 0.0,
            sum_downside_sq: 0.0,
        })
    }

    /// Configured window of returns.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured minimal acceptable return.
    pub const fn mar(&self) -> f64 {
        self.mar
    }
}

impl Indicator for UpsidePotentialRatio {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, ret: f64) -> Option<f64> {
        if !ret.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("non-empty");
            let excess = old - self.mar;
            self.sum_upside -= excess.max(0.0);
            self.sum_downside_sq -= excess.min(0.0).powi(2);
        }
        let excess = ret - self.mar;
        self.sum_upside += excess.max(0.0);
        self.sum_downside_sq += excess.min(0.0).powi(2);
        self.window.push_back(ret);
        if self.window.len() < self.period {
            return None;
        }
        let n = self.period as f64;
        let upside_mean = self.sum_upside / n;
        let downside_dev = (self.sum_downside_sq / n).sqrt();
        if downside_dev > 0.0 {
            Some(upside_mean / downside_dev)
        } else {
            Some(0.0)
        }
    }

    fn reset(&mut self) {
        self.window.clear();
        self.sum_upside = 0.0;
        self.sum_downside_sq = 0.0;
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
        "UpsidePotentialRatio"
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
            UpsidePotentialRatio::new(1, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn rejects_non_finite_mar() {
        assert!(matches!(
            UpsidePotentialRatio::new(10, f64::NAN),
            Err(Error::InvalidParameter { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let upr = UpsidePotentialRatio::new(20, 0.001).unwrap();
        assert_eq!(upr.period(), 20);
        assert_relative_eq!(upr.mar(), 0.001, epsilon = 1e-12);
        assert_eq!(upr.warmup_period(), 20);
        assert_eq!(upr.name(), "UpsidePotentialRatio");
    }

    #[test]
    fn reference_value() {
        // returns [0.02, -0.01, 0.03, -0.02], mar = 0.
        // upside = (0.02 + 0 + 0.03 + 0)/4 = 0.0125.
        // downside = sqrt((0 + 0.0001 + 0 + 0.0004)/4) = sqrt(0.000125).
        // UPR = 0.0125 / sqrt(0.000125).
        let mut upr = UpsidePotentialRatio::new(4, 0.0).unwrap();
        let out = upr.batch(&[0.02, -0.01, 0.03, -0.02]);
        let expected = 0.0125_f64 / (0.000_125_f64).sqrt();
        assert_relative_eq!(out[3].unwrap(), expected, epsilon = 1e-9);
    }

    #[test]
    fn no_downside_is_zero() {
        let mut upr = UpsidePotentialRatio::new(3, 0.0).unwrap();
        let last = upr
            .batch(&[0.01, 0.02, 0.03])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut upr = UpsidePotentialRatio::new(3, 0.0).unwrap();
        assert_eq!(upr.update(0.01), None);
        assert_eq!(upr.update(f64::INFINITY), None);
        assert_eq!(upr.update(-0.02), None);
        assert!(upr.update(0.03).is_some());
    }

    #[test]
    fn reset_clears_state() {
        let mut upr = UpsidePotentialRatio::new(2, 0.0).unwrap();
        upr.batch(&[0.02, -0.01]);
        assert!(upr.is_ready());
        upr.reset();
        assert!(!upr.is_ready());
        assert_eq!(upr.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let rets: Vec<f64> = (0..60)
            .map(|i| (f64::from(i) * 0.25).sin() * 0.02)
            .collect();
        let batch = UpsidePotentialRatio::new(12, 0.0).unwrap().batch(&rets);
        let mut streamer = UpsidePotentialRatio::new(12, 0.0).unwrap();
        let streamed: Vec<_> = rets.iter().map(|r| streamer.update(*r)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            UpsidePotentialRatio::new(crate::error::MAX_PERIOD + 1, 0.001),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
