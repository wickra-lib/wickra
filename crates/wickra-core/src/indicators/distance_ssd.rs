//! Gatev distance (sum of squared deviations) between two normalised series.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Sum of squared deviations between two price series, normalised to a common
/// start — the classic Gatev et al. pairs-selection distance.
///
/// Each `update` takes one `(a, b)` price pair. Over the trailing window of
/// `period` pairs each series is rebased to `1` at the window's first bar and
/// the squared gap between the two normalised paths is summed:
///
/// ```text
/// ãᵢ = aᵢ / a_first        b̃ᵢ = bᵢ / b_first
/// SSD = Σ (ãᵢ − b̃ᵢ)²
/// ```
///
/// Rebasing puts the two series on the same scale (both start at `1`), so the
/// distance measures how far their *relative* paths drift apart. A **small**
/// SSD means the two assets track each other tightly — the screen Gatev,
/// Goetzmann and Rouwenhorst use to pick tradeable pairs; a large SSD means
/// they have decoupled. The output is always `≥ 0`. If either series is `0` at
/// the start of the window the normalisation is undefined and the indicator
/// returns `0`.
///
/// Each `update` is `O(period)`, bounded by the fixed window.
///
/// # Example
///
/// ```
/// use wickra_core::{DistanceSsd, Indicator};
///
/// let mut d = DistanceSsd::new(20).unwrap();
/// let mut last = None;
/// for t in 0..40 {
///     let base = 100.0 + f64::from(t);
///     // Two near-identical paths ⇒ tiny distance.
///     last = d.update((base, base * 1.0001));
/// }
/// assert!(last.unwrap() < 1e-3);
/// ```
#[derive(Debug, Clone)]
pub struct DistanceSsd {
    period: usize,
    window: VecDeque<(f64, f64)>,
}

impl DistanceSsd {
    /// Construct a new Gatev distance estimator.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` — a distance needs at
    /// least two points.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "distance SSD needs period >= 2",
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
        })
    }

    /// Configured look-back window.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for DistanceSsd {
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        if !input.0.is_finite() || !input.1.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(input);
        if self.window.len() < self.period {
            return None;
        }
        let &(a_first, b_first) = self.window.front().expect("window is full");
        if a_first == 0.0 || b_first == 0.0 {
            // Cannot rebase a series that starts at zero.
            return Some(0.0);
        }
        let ssd = self
            .window
            .iter()
            .map(|&(a, b)| {
                let gap = a / a_first - b / b_first;
                gap * gap
            })
            .sum();
        Some(ssd)
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
        "DistanceSsd"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_two() {
        assert!(DistanceSsd::new(1).is_err());
        assert!(DistanceSsd::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let d = DistanceSsd::new(20).unwrap();
        assert_eq!(d.period(), 20);
        assert_eq!(d.warmup_period(), 20);
        assert_eq!(d.name(), "DistanceSsd");
        assert!(!d.is_ready());
    }

    #[test]
    fn warmup_returns_none() {
        let mut d = DistanceSsd::new(3).unwrap();
        assert_eq!(d.update((1.0, 1.0)), None);
        assert_eq!(d.update((2.0, 2.0)), None);
        assert!(d.update((3.0, 3.0)).is_some());
        assert!(d.is_ready());
    }

    #[test]
    fn identical_normalised_paths_have_zero_distance() {
        // b = 2·a ⇒ both rebase to the same path ⇒ SSD = 0.
        let pairs: Vec<(f64, f64)> = (0..20)
            .map(|t| {
                let a = 100.0 + f64::from(t);
                (a, 2.0 * a)
            })
            .collect();
        let last = DistanceSsd::new(10)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn diverging_paths_have_positive_distance() {
        let pairs: Vec<(f64, f64)> = (0..20)
            .map(|t| (100.0 + f64::from(t), 100.0 + 3.0 * f64::from(t)))
            .collect();
        let last = DistanceSsd::new(10)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last > 0.0, "ssd {last}");
    }

    #[test]
    fn hand_computed_value() {
        // Window of three pairs, a_first = b_first = 1:
        //   (1,1) → 0; (2,4) → (2−4)² = 4; (3,9) → (3−9)² = 36 ⇒ SSD = 40.
        let pairs = [(1.0, 1.0), (2.0, 4.0), (3.0, 9.0)];
        let last = DistanceSsd::new(3)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 40.0, epsilon = 1e-12);
    }

    #[test]
    fn zero_start_returns_zero() {
        // First bar of the window has a = 0 ⇒ rebasing undefined ⇒ 0.
        let pairs = [(0.0, 1.0), (2.0, 2.0), (3.0, 3.0)];
        let last = DistanceSsd::new(3)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(last, 0.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut d = DistanceSsd::new(4).unwrap();
        d.batch(&[(1.0, 1.0), (2.0, 2.0), (3.0, 4.0), (4.0, 5.0), (5.0, 6.0)]);
        assert!(d.is_ready());
        d.reset();
        assert!(!d.is_ready());
        assert_eq!(d.update((1.0, 1.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..60)
            .map(|t| {
                let a = 100.0 + f64::from(t);
                (a, 100.0 + 1.2 * f64::from(t) + (f64::from(t) * 0.5).sin())
            })
            .collect();
        let batch = DistanceSsd::new(15).unwrap().batch(&pairs);
        let mut d = DistanceSsd::new(15).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| d.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut d = DistanceSsd::new(3).unwrap();
        assert_eq!(d.update((f64::NAN, 1.0)), None);
        assert_eq!(d.update((1.0, f64::INFINITY)), None);
        // The rejected ticks leave no trace: a fresh window still warms up.
        assert_eq!(d.update((1.0, 1.0)), None);
        assert_eq!(d.update((2.0, 4.0)), None);
        assert!(d.update((3.0, 9.0)).is_some());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            DistanceSsd::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
