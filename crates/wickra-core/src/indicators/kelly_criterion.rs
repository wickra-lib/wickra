//! Rolling Kelly Criterion.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Rolling Kelly Criterion fraction.
///
/// Input is treated as a per-period (or per-trade) return. Over the trailing
/// window the indicator estimates the optimal capital fraction to allocate
/// using the **even-money** Kelly formula generalised by the payoff ratio:
///
/// ```text
/// win_rate     = P(r > 0)                          over window
/// avg_win      = mean(r for r > 0)
/// avg_loss     = mean(−r for r < 0)
/// payoff_ratio = avg_win / avg_loss
/// Kelly        = win_rate − (1 − win_rate) / payoff_ratio
/// ```
///
/// The output is the recommended **fraction** of capital to bet (typically
/// `(0, 1)`; can go negative if the estimated edge is negative, in which
/// case the position should be reversed or sized to zero). Most
/// practitioners use a "half-Kelly" or "quarter-Kelly" multiplier in
/// practice to reduce variance — Wickra reports raw Kelly and leaves the
/// scaling to the caller.
///
/// Edge cases:
///   * No winners and no losers ⇒ `0.0` (no information).
///   * No losers (`payoff_ratio = ∞`) ⇒ Kelly collapses to the win rate.
///   * No winners but losers present ⇒ Kelly = `−(1 − 0) / payoff = …`,
///     which is negative — bet nothing (or short).
///
/// Each `update` is O(period).
#[derive(Debug, Clone)]
pub struct KellyCriterion {
    period: usize,
    window: VecDeque<f64>,
}

impl KellyCriterion {
    /// Construct a new rolling Kelly Criterion.
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

impl Indicator for KellyCriterion {
    type Input = f64;
    type Output = f64;

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
        let n = self.period as f64;
        let win_rate = f64::from(n_win) / n;
        if n_loss == 0 {
            // No losses in window: payoff ratio is infinite; Kelly collapses
            // to the win rate (limit of w - (1-w)/r as r -> ∞).
            return Some(win_rate);
        }
        let avg_loss = sum_loss / f64::from(n_loss);
        if n_win == 0 {
            // All losses: avg_win = 0 -> payoff = 0 -> -(1)/0 -> -inf.
            // Bet nothing (or reverse); clamp to -1 for sanity.
            return Some(-1.0);
        }
        let avg_win = sum_win / f64::from(n_win);
        let payoff = avg_win / avg_loss;
        Some(win_rate - (1.0 - win_rate) / payoff)
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
        "KellyCriterion"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(KellyCriterion::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let k = KellyCriterion::new(10).unwrap();
        assert_eq!(k.period(), 10);
        assert_eq!(k.name(), "KellyCriterion");
        assert_eq!(k.warmup_period(), 10);
    }

    #[test]
    fn reference_value() {
        // returns = [0.02, 0.04, -0.01, -0.02] (n=4).
        // n_win=2, n_loss=2; win_rate = 0.5.
        // avg_win=0.03, avg_loss=0.015, payoff=2.
        // Kelly = 0.5 - (0.5/2) = 0.25.
        let mut k = KellyCriterion::new(4).unwrap();
        let out = k.batch(&[0.02, 0.04, -0.01, -0.02]);
        assert_relative_eq!(out[3].unwrap(), 0.25, epsilon = 1e-9);
    }

    #[test]
    fn all_winners_returns_win_rate() {
        let mut k = KellyCriterion::new(3).unwrap();
        let out = k.batch(&[0.01, 0.02, 0.03]);
        assert_relative_eq!(out[2].unwrap(), 1.0, epsilon = 1e-12);
    }

    #[test]
    fn all_losers_returns_negative_one() {
        let mut k = KellyCriterion::new(3).unwrap();
        let out = k.batch(&[-0.01, -0.02, -0.03]);
        assert_relative_eq!(out[2].unwrap(), -1.0, epsilon = 1e-12);
    }

    #[test]
    fn flat_window_yields_zero() {
        let mut k = KellyCriterion::new(3).unwrap();
        let out = k.batch(&[0.0_f64; 3]);
        assert_eq!(out[2], Some(0.0));
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut k = KellyCriterion::new(3).unwrap();
        assert_eq!(k.update(f64::NAN), None);
        assert_eq!(k.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut k = KellyCriterion::new(3).unwrap();
        k.batch(&[0.01, -0.02, 0.03]);
        assert!(k.is_ready());
        k.reset();
        assert!(!k.is_ready());
        assert_eq!(k.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let returns: Vec<f64> = (0..40).map(|i| (f64::from(i) * 0.3).sin() * 0.01).collect();
        let batch = KellyCriterion::new(10).unwrap().batch(&returns);
        let mut s = KellyCriterion::new(10).unwrap();
        let streamed: Vec<_> = returns.iter().map(|r| s.update(*r)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            KellyCriterion::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
