//! Rolling Gain/Loss Ratio.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Rolling Gain/Loss Ratio.
///
/// Over the trailing window:
///
/// ```text
/// avg_win  = mean(r for r in window if r > 0)
/// avg_loss = mean(−r for r in window if r < 0)
/// GLR      = avg_win / avg_loss
/// ```
///
/// Where Profit Factor sums gains and losses, the Gain/Loss Ratio averages
/// them: it answers "for the typical winning bar, how big is the win
/// compared to the typical losing bar?".
///
/// # Unbounded output
///
/// A window with winners but no losers has no denominator, and the indicator
/// returns `f64::INFINITY`. This is not an edge case to be discovered in
/// production: any `period`-bar window without a single down bar produces it,
/// which on a trending instrument happens routinely. The value is correct --
/// the ratio really is unbounded -- but it propagates, and `inf - inf` is
/// `NaN`, so a caller feeding this into further arithmetic should test for it.
/// `f64::is_finite` is the guard.
///
/// A window with neither winners nor losers is break-even and returns `1.0`,
/// the same value a window whose typical win matches its typical loss returns.
/// It used to return `0.0`, which is also what a window that lost on every
/// single bar returns -- the two are opposite states and were indistinguishable.
///
/// Each `update` is O(period).
#[derive(Debug, Clone)]
pub struct GainLossRatio {
    period: usize,
    window: VecDeque<f64>,
}

impl GainLossRatio {
    /// Construct a new rolling Gain/Loss Ratio.
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

impl Indicator for GainLossRatio {
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
        let mut sum_win = 0.0_f64;
        let mut n_win = 0_u32;
        let mut sum_loss = 0.0_f64;
        let mut n_loss = 0_u32;
        for &r in &self.window {
            if r > 0.0 {
                sum_win += r;
                n_win += 1;
            } else if r < 0.0 {
                sum_loss += -r;
                n_loss += 1;
            }
        }
        if n_loss == 0 {
            // Neither gains nor losses: the window is break-even, which is
            // what 1.0 means here. Returning 0.0 made a flat window
            // indistinguishable from one that lost on every bar.
            return Some(if n_win == 0 { 1.0 } else { f64::INFINITY });
        }
        let avg_win = if n_win == 0 {
            0.0
        } else {
            sum_win / f64::from(n_win)
        };
        let avg_loss = sum_loss / f64::from(n_loss);
        Some(avg_win / avg_loss)
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
        "GainLossRatio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(GainLossRatio::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let g = GainLossRatio::new(10).unwrap();
        assert_eq!(g.period(), 10);
        assert_eq!(g.name(), "GainLossRatio");
        assert_eq!(g.warmup_period(), 10);
    }

    #[test]
    fn reference_value() {
        // returns = [0.02, -0.01, 0.04, -0.03]
        // avg_win = 0.03, avg_loss = 0.02, GLR = 1.5.
        let mut g = GainLossRatio::new(4).unwrap();
        let out = g.batch(&[0.02, -0.01, 0.04, -0.03]);
        assert_relative_eq!(out[3].unwrap(), 1.5, epsilon = 1e-9);
    }

    #[test]
    fn no_losses_yields_infinity() {
        let mut g = GainLossRatio::new(3).unwrap();
        let out = g.batch(&[0.01, 0.02, 0.03]);
        assert!(out[2].unwrap().is_infinite());
    }

    #[test]
    fn flat_window_is_break_even() {
        let mut g = GainLossRatio::new(3).unwrap();
        let out = g.batch(&[0.0_f64; 3]);
        assert_eq!(out[2], Some(1.0));
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut g = GainLossRatio::new(3).unwrap();
        assert_eq!(g.update(f64::NAN), None);
        assert_eq!(g.update(f64::INFINITY), None);
    }

    #[test]
    fn no_wins_but_losses_yields_zero() {
        // Window with only losses: avg_win is 0, GLR = 0.
        let mut g = GainLossRatio::new(3).unwrap();
        let out = g.batch(&[-0.01, -0.02, -0.03]);
        assert_eq!(out[2], Some(0.0));
    }

    #[test]
    fn reset_clears_state() {
        let mut g = GainLossRatio::new(3).unwrap();
        g.batch(&[0.01, -0.02, 0.03]);
        assert!(g.is_ready());
        g.reset();
        assert!(!g.is_ready());
        assert_eq!(g.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let returns: Vec<f64> = (0..40).map(|i| (f64::from(i) * 0.3).sin() * 0.01).collect();
        let batch = GainLossRatio::new(10).unwrap().batch(&returns);
        let mut s = GainLossRatio::new(10).unwrap();
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

        let mut a = GainLossRatio::new(14).unwrap();
        let mut b = GainLossRatio::new(14).unwrap();
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
            GainLossRatio::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
