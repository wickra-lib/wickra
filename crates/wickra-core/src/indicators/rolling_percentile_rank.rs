//! Rolling Percentile Rank of the latest value within its trailing window.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Percentile rank of the most-recent value within the last `period` values,
/// in `[0, 100]`.
///
/// ```text
/// rank = 100 · (#below + 0.5 · #equal) / period
/// ```
///
/// where `#below` counts window values strictly less than the current value and
/// `#equal` counts those equal to it (including the current value itself). This
/// is the "mean" method of `percentileofscore`: ties are split symmetrically,
/// so a flat window scores exactly `50`, the strict window maximum scores just
/// under `100`, and the strict minimum just over `0`.
///
/// Percentile rank turns any series into a bounded, self-normalising oscillator:
/// "where does today sit relative to its own recent history" — high readings
/// mark stretched extremes, mid readings mark the typical range. It is the
/// scale-free cousin of the z-score that makes no distributional assumption.
///
/// Each `update` is O(period): one linear pass tallies the comparisons.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, RollingPercentileRank};
///
/// let mut indicator = RollingPercentileRank::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// // A strictly rising series puts the newest value near the top.
/// assert!(last.unwrap() > 90.0);
/// ```
#[derive(Debug, Clone)]
pub struct RollingPercentileRank {
    period: usize,
    window: VecDeque<f64>,
}

impl RollingPercentileRank {
    /// Construct a new rolling percentile rank with the given period.
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

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for RollingPercentileRank {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, value: f64) -> Option<f64> {
        if !value.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(value);
        if self.window.len() < self.period {
            return None;
        }
        let mut below = 0_usize;
        let mut equal = 0_usize;
        for &x in &self.window {
            if x < value {
                below += 1;
            } else if x == value {
                equal += 1;
            }
        }
        let score = (below as f64 + 0.5 * equal as f64) / self.period as f64 * 100.0;
        Some(score)
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
        "RollingPercentileRank"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            RollingPercentileRank::new(0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let pr = RollingPercentileRank::new(14).unwrap();
        assert_eq!(pr.period(), 14);
        assert_eq!(pr.warmup_period(), 14);
        assert_eq!(pr.name(), "RollingPercentileRank");
        assert!(!pr.is_ready());
    }

    #[test]
    fn flat_window_scores_fifty() {
        // All values equal: #below = 0, #equal = period → 0.5 → 50.
        let mut pr = RollingPercentileRank::new(10).unwrap();
        for v in pr.batch(&[7.0; 20]).into_iter().flatten() {
            assert_relative_eq!(v, 50.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn current_is_strict_maximum() {
        // Window [1,2,3,4,5], current = 5: #below = 4, #equal = 1.
        // (4 + 0.5) / 5 * 100 = 90.
        let mut pr = RollingPercentileRank::new(5).unwrap();
        let out = pr.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_relative_eq!(out[4].unwrap(), 90.0, epsilon = 1e-12);
    }

    #[test]
    fn current_is_strict_minimum() {
        // Window [5,4,3,2,1], current = 1: #below = 0, #equal = 1.
        // (0 + 0.5) / 5 * 100 = 10.
        let mut pr = RollingPercentileRank::new(5).unwrap();
        let out = pr.batch(&[5.0, 4.0, 3.0, 2.0, 1.0]);
        assert_relative_eq!(out[4].unwrap(), 10.0, epsilon = 1e-12);
    }

    #[test]
    fn output_within_bounds() {
        let mut pr = RollingPercentileRank::new(20).unwrap();
        let prices: Vec<f64> = (1..=200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 12.0)
            .collect();
        for v in pr.batch(&prices).into_iter().flatten() {
            assert!((0.0..=100.0).contains(&v), "out of bounds: {v}");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut pr = RollingPercentileRank::new(5).unwrap();
        pr.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(pr.is_ready());
        pr.reset();
        assert!(!pr.is_ready());
        assert_eq!(pr.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let batch = RollingPercentileRank::new(14).unwrap().batch(&prices);
        let mut b = RollingPercentileRank::new(14).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            RollingPercentileRank::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
