//! Rolling Profit Factor.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Rolling Profit Factor.
///
/// Input is treated as a per-period return (or a per-trade P&L). Over the
/// trailing window:
///
/// ```text
/// gross_profit = Σ max(0, r) over window
/// gross_loss   = Σ max(0, −r) over window
/// PF           = gross_profit / gross_loss
/// ```
///
/// `PF > 1` means the strategy made more than it lost in the window.
///
/// # Unbounded output
///
/// With no losing returns the gross loss is zero and the indicator returns
/// `f64::INFINITY`. This is not an edge case to be discovered in production:
/// any `period`-bar window without a single down bar produces it, which on a
/// trending instrument happens routinely. The value is correct -- the ratio
/// really is unbounded -- but it propagates, and `inf - inf` is `NaN`, so a
/// caller feeding this into further arithmetic should test for it.
/// `f64::is_finite` is the guard.
///
/// A flat window, with neither gains nor losses, is break-even and returns
/// `1.0` -- the same value the ratio takes when gross profit equals gross loss.
/// It used to return `0.0`, which is also what a window of nothing but losses
/// returns; the two are opposite states and were indistinguishable.
///
/// Each `update` is O(period).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, ProfitFactor};
///
/// let mut pf = ProfitFactor::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = pf.update((f64::from(i) * 0.2).sin() * 0.01);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct ProfitFactor {
    period: usize,
    window: VecDeque<f64>,
}

impl ProfitFactor {
    /// Construct a new rolling Profit Factor.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
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
            window: VecDeque::with_capacity(period),
        })
    }

    /// Configured window length.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for ProfitFactor {
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
            if r > 0.0 {
                gains += r;
            } else if r < 0.0 {
                losses += -r;
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
        "ProfitFactor"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(ProfitFactor::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let p = ProfitFactor::new(10).unwrap();
        assert_eq!(p.period(), 10);
        assert_eq!(p.name(), "ProfitFactor");
        assert_eq!(p.warmup_period(), 10);
    }

    #[test]
    fn reference_value() {
        // returns = [0.02, -0.01, 0.03, -0.02]
        // gains = 0.05, losses = 0.03, PF = 5/3.
        let mut p = ProfitFactor::new(4).unwrap();
        let out = p.batch(&[0.02, -0.01, 0.03, -0.02]);
        assert_relative_eq!(out[3].unwrap(), 5.0 / 3.0, epsilon = 1e-9);
    }

    #[test]
    fn no_losses_yields_infinity() {
        let mut p = ProfitFactor::new(3).unwrap();
        let out = p.batch(&[0.01, 0.02, 0.03]);
        assert!(out[2].unwrap().is_infinite());
    }

    #[test]
    fn flat_window_is_break_even() {
        let mut p = ProfitFactor::new(3).unwrap();
        let out = p.batch(&[0.0_f64; 3]);
        assert_eq!(out[2], Some(1.0));
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut p = ProfitFactor::new(3).unwrap();
        assert_eq!(p.update(f64::NAN), None);
        assert_eq!(p.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut p = ProfitFactor::new(3).unwrap();
        p.batch(&[0.01, -0.02, 0.03]);
        assert!(p.is_ready());
        p.reset();
        assert!(!p.is_ready());
        assert_eq!(p.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let returns: Vec<f64> = (0..40).map(|i| (f64::from(i) * 0.3).sin() * 0.01).collect();
        let batch = ProfitFactor::new(10).unwrap().batch(&returns);
        let mut s = ProfitFactor::new(10).unwrap();
        let streamed: Vec<_> = returns.iter().map(|r| s.update(*r)).collect();
        assert_eq!(batch, streamed);
    }
    /// A flat window and a window that lost on every single bar are opposite
    /// states, and both used to report `0.0`. Asserting each value on its own
    /// could never catch that; asserting they differ is the property that
    /// matters.
    #[test]
    fn a_flat_window_is_not_confused_with_an_all_losing_one() {
        let flat = [0.0_f64; 20];
        let losing = [-0.01_f64; 20];

        let mut a = ProfitFactor::new(14).unwrap();
        let mut b = ProfitFactor::new(14).unwrap();
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
            ProfitFactor::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
