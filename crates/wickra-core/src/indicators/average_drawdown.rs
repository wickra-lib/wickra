//! Rolling Average Drawdown.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Rolling Average Drawdown.
///
/// Input is treated as an equity-curve sample. Over the trailing window of
/// `period` values the indicator identifies each **distinct drawdown episode**
/// — a stretch where equity is below the running peak — and reports the **mean
/// of the episodes' maximum depths**:
///
/// ```text
/// episode opens when equity < running peak
/// episode closes when equity reaches a new peak (full recovery)
/// depth(episode) = (episode_peak − episode_trough) / episode_peak
/// AvgDD          = mean(depth over episodes in window)   (0 if no drawdown)
/// ```
///
/// This is the conventional "average drawdown" (mean depth across separate
/// drawdowns), which is distinct from the [`crate::PainIndex`] — the latter
/// averages the under-water fraction at *every* bar, so a long shallow
/// drawdown weighs more there than here. Output is a non-negative fraction
/// (`0.05` ≈ 5 % mean episode depth).
///
/// Each `update` is O(period).
#[derive(Debug, Clone)]
pub struct AverageDrawdown {
    period: usize,
    window: VecDeque<f64>,
}

impl AverageDrawdown {
    /// Construct a new rolling Average Drawdown.
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

impl Indicator for AverageDrawdown {
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
        let mut peak = f64::NEG_INFINITY;
        let mut sum_depth = 0.0_f64;
        let mut episodes = 0_u32;
        let mut in_dd = false;
        let mut episode_peak = 0.0_f64;
        let mut episode_trough = 0.0_f64;
        for &v in &self.window {
            if v >= peak {
                if in_dd {
                    if episode_peak > 0.0 {
                        sum_depth += (episode_peak - episode_trough) / episode_peak;
                        episodes += 1;
                    }
                    in_dd = false;
                }
                peak = v;
            } else if in_dd {
                if v < episode_trough {
                    episode_trough = v;
                }
            } else {
                in_dd = true;
                episode_peak = peak;
                episode_trough = v;
            }
        }
        if in_dd && episode_peak > 0.0 {
            sum_depth += (episode_peak - episode_trough) / episode_peak;
            episodes += 1;
        }
        Some(if episodes == 0 {
            0.0
        } else {
            sum_depth / f64::from(episodes)
        })
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
        "AverageDrawdown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(AverageDrawdown::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let a = AverageDrawdown::new(10).unwrap();
        assert_eq!(a.period(), 10);
        assert_eq!(a.name(), "AverageDrawdown");
        assert_eq!(a.warmup_period(), 10);
    }

    #[test]
    fn pure_uptrend_yields_zero() {
        let mut a = AverageDrawdown::new(5).unwrap();
        let out = a.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        for v in out.into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn reference_value() {
        // window [100, 120, 90, 110]: one drawdown episode, opened at 90 (peak
        // 120) and never recovering to 120 within the window. Its depth is
        // (120 - 90) / 120 = 0.25; 110 stays inside the same episode and does
        // not deepen the trough. One episode -> AvgDD = 0.25.
        let mut a = AverageDrawdown::new(4).unwrap();
        let out = a.batch(&[100.0, 120.0, 90.0, 110.0]);
        assert_relative_eq!(out[3].unwrap(), 0.25, epsilon = 1e-12);
    }

    #[test]
    fn averages_distinct_episodes() {
        // [100, 90, 100, 80, 100]: episode 1 troughs at 90 then recovers to 100
        // -> depth 0.10; episode 2 troughs at 80 then recovers -> depth 0.20.
        // Mean of the two episode depths = 0.15 (distinct from the Pain Index,
        // which would weight every under-water bar instead).
        let mut a = AverageDrawdown::new(5).unwrap();
        let out = a.batch(&[100.0, 90.0, 100.0, 80.0, 100.0]);
        assert_relative_eq!(out[4].unwrap(), 0.15, epsilon = 1e-12);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut a = AverageDrawdown::new(3).unwrap();
        assert_eq!(a.update(f64::NAN), None);
        assert_eq!(a.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut a = AverageDrawdown::new(3).unwrap();
        a.batch(&[100.0, 90.0, 110.0]);
        assert!(a.is_ready());
        a.reset();
        assert!(!a.is_ready());
        assert_eq!(a.update(100.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..40)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 8.0)
            .collect();
        let batch = AverageDrawdown::new(10).unwrap().batch(&prices);
        let mut s = AverageDrawdown::new(10).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| s.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_positive_peak_yields_zero() {
        let mut a = AverageDrawdown::new(3).unwrap();
        let out = a.batch(&[0.0_f64; 6]);
        for v in out.into_iter().flatten() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            AverageDrawdown::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
