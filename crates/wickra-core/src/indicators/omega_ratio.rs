//! Rolling Omega Ratio — gain-to-loss ratio above a threshold.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Rolling Omega Ratio.
///
/// Over the trailing window of `period` returns and a target `threshold`:
///
/// ```text
/// gains  = Σ max(0, r − threshold)
/// losses = Σ max(0, threshold − r)
/// Omega  = gains / losses
/// ```
///
/// Omega expresses how many units of "above-threshold" return the strategy
/// produces per unit of "below-threshold" shortfall. By construction
/// `Omega ≥ 0`. The Sharpe Ratio collapses risk into a single second-moment
/// number; Omega keeps the full shape of the loss tail.
///
/// # Unbounded output
///
/// A window where every return clears the threshold has zero shortfall, and
/// the indicator returns `f64::INFINITY`, in keeping with the standard
/// definition. This is not an edge case to be discovered in production: any
/// `period`-bar window that stays above the threshold produces it. The value
/// is correct -- the ratio really is unbounded -- but it propagates, and
/// `inf - inf` is `NaN`, so a caller feeding this into further arithmetic
/// should test for it. `f64::is_finite` is the guard.
///
/// The threshold decides what "flat" means here, and the two ends differ:
/// with `threshold = 0.0` a window of zero returns has neither gains nor
/// shortfall, which is break-even and yields `1.0`, while with a *negative*
/// threshold every zero return clears it, so the same flat window yields
/// `f64::INFINITY`.
///
/// Each `update` is O(period) because the partial sums are recomputed across
/// the window — adequate for typical backtest windows (`period ≤ 252`).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, OmegaRatio};
///
/// let mut o = OmegaRatio::new(20, 0.0).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = o.update((f64::from(i) * 0.2).sin() * 0.01);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct OmegaRatio {
    period: usize,
    threshold: f64,
    window: VecDeque<f64>,
}

impl OmegaRatio {
    /// Construct a new rolling Omega Ratio.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0`.
    pub fn new(period: usize, threshold: f64) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            threshold,
            window: VecDeque::with_capacity(period),
        })
    }

    /// Configured window length.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured threshold (per-period).
    pub const fn threshold(&self) -> f64 {
        self.threshold
    }
}

impl Indicator for OmegaRatio {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(input);
        if self.window.len() < self.period {
            return None;
        }
        let mut gains = 0.0_f64;
        let mut losses = 0.0_f64;
        for &r in &self.window {
            let d = r - self.threshold;
            if d >= 0.0 {
                gains += d;
            } else {
                losses += -d;
            }
        }
        if losses == 0.0 {
            // Neither gains nor losses: the window is break-even, which is
            // what 1.0 means here. Returning 0.0 made a flat window
            // indistinguishable from one that lost on every bar.
            return Some(if gains == 0.0 { 1.0 } else { f64::INFINITY });
        }
        Some(gains / losses)
    }

    fn reset(&mut self) {
        self.window.clear();
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
        "OmegaRatio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(OmegaRatio::new(0, 0.0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let o = OmegaRatio::new(10, 0.001).unwrap();
        assert_eq!(o.period(), 10);
        assert_relative_eq!(o.threshold(), 0.001, epsilon = 1e-12);
        assert_eq!(o.name(), "OmegaRatio");
        assert_eq!(o.warmup_period(), 10);
    }

    #[test]
    fn all_above_threshold_yields_infinity() {
        let mut o = OmegaRatio::new(4, 0.0).unwrap();
        let out = o.batch(&[0.01, 0.02, 0.03, 0.04]);
        assert!(out[3].unwrap().is_infinite());
    }

    #[test]
    fn flat_at_threshold_is_break_even() {
        // Every return equals threshold -> gains = losses = 0 -> 0 by
        // convention.
        let mut o = OmegaRatio::new(4, 0.01).unwrap();
        let out = o.batch(&[0.01; 4]);
        assert_eq!(out[3], Some(1.0));
    }

    #[test]
    fn reference_value() {
        // returns = [-0.02, 0.01, -0.01, 0.03], threshold = 0.
        // gains  = 0.01 + 0.03 = 0.04
        // losses = 0.02 + 0.01 = 0.03
        // Omega = 0.04 / 0.03 ≈ 1.3333...
        let mut o = OmegaRatio::new(4, 0.0).unwrap();
        let out = o.batch(&[-0.02, 0.01, -0.01, 0.03]);
        assert_relative_eq!(out[3].unwrap(), 0.04 / 0.03, epsilon = 1e-9);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut o = OmegaRatio::new(3, 0.0).unwrap();
        assert_eq!(o.update(f64::NAN), None);
        assert_eq!(o.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut o = OmegaRatio::new(3, 0.0).unwrap();
        o.batch(&[0.01, -0.02, 0.005]);
        assert!(o.is_ready());
        o.reset();
        assert!(!o.is_ready());
        assert_eq!(o.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let returns: Vec<f64> = (0..50).map(|i| (f64::from(i) * 0.4).sin() * 0.01).collect();
        let batch = OmegaRatio::new(10, 0.0).unwrap().batch(&returns);
        let mut s = OmegaRatio::new(10, 0.0).unwrap();
        let streamed: Vec<_> = returns.iter().map(|r| s.update(*r)).collect();
        assert_eq!(batch, streamed);
    }
    /// With a negative threshold every flat return counts as clearing it, so a
    /// window that did not move at all reports an unbounded ratio rather than
    /// the break-even `1.0` the same window gives at a threshold of zero.
    /// Worth pinning because it is the opposite answer to the obvious one.
    #[test]
    fn a_negative_threshold_makes_a_flat_window_unbounded() {
        let flat = [0.0_f64; 20];

        let mut at_zero = OmegaRatio::new(14, 0.0).unwrap();
        let mut below = OmegaRatio::new(14, -0.005).unwrap();
        let (mut last_at_zero, mut last_below) = (None, None);
        for &r in &flat {
            last_at_zero = at_zero.update(r).or(last_at_zero);
            last_below = below.update(r).or(last_below);
        }

        assert_eq!(last_at_zero, Some(1.0));
        assert_eq!(last_below, Some(f64::INFINITY));
    }
    /// A flat window and a window that lost on every single bar are opposite
    /// states, and both used to report `0.0`. Asserting each value on its own
    /// could never catch that; asserting they differ is the property that
    /// matters.
    #[test]
    fn a_flat_window_is_not_confused_with_an_all_losing_one() {
        let flat = [0.0_f64; 20];
        let losing = [-0.01_f64; 20];

        let mut a = OmegaRatio::new(14, 0.0).unwrap();
        let mut b = OmegaRatio::new(14, 0.0).unwrap();
        let (mut flat_value, mut losing_value) = (None, None);
        for i in 0..flat.len() {
            flat_value = a.update(flat[i]).or(flat_value);
            losing_value = b.update(losing[i]).or(losing_value);
        }

        assert_eq!(flat_value, Some(1.0), "a flat window is break-even");
        assert_eq!(losing_value, Some(0.0), "an all-losing window has no gains");
        assert_ne!(flat_value, losing_value);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            OmegaRatio::new(crate::error::MAX_PERIOD + 1, 0.001),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
