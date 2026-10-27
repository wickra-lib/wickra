//! Rolling Pearson correlation of the period-over-period *returns* of two series.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedPairMoments;
use crate::traits::Indicator;

/// Rolling correlation of the **returns** of two synchronised series.
///
/// Where [`crate::PearsonCorrelation`] correlates the raw *levels* `(x, y)`,
/// this indicator first differences each channel into a one-step return and
/// correlates those returns over the trailing window:
///
/// ```text
/// rxₜ = xₜ − xₜ₋₁          ryₜ = yₜ − yₜ₋₁
/// corr = cov(rx, ry) / √(var(rx) · var(ry))
/// ```
///
/// Return correlation is the quantity that matters for hedging and portfolio
/// risk: two assets can trend together (high level correlation) while their
/// day-to-day moves are nearly independent (low return correlation). The output
/// is in `[−1, +1]`; a flat return channel makes the ratio undefined and the
/// indicator reports `0` rather than `NaN`. The value is clamped to `[−1, +1]`
/// to absorb tiny floating-point overshoots near the boundaries.
///
/// Each `update` is O(1): the five running sums (`Σrx`, `Σry`, `Σrx²`, `Σry²`,
/// `Σrxry`) are maintained as the window of returns slides. The first level in
/// each channel produces no return, so a `period`-pair correlation needs
/// `period + 1` updates of warmup.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, RollingCorrelation};
///
/// let mut rc = RollingCorrelation::new(10).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     // A varying path where y always moves with x ⇒ return correlation +1.
///     let x = (f64::from(i) * 0.5).sin() * 10.0;
///     last = rc.update((x, 2.0 * x));
/// }
/// assert!((last.unwrap() - 1.0).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct RollingCorrelation {
    period: usize,
    prev: Option<(f64, f64)>,
    window: VecDeque<(f64, f64)>,
    moments: ShiftedPairMoments,
}

impl RollingCorrelation {
    /// Construct a new rolling return-correlation.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` — correlation is
    /// undefined for fewer than two return pairs.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "rolling correlation needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            prev: None,
            window: VecDeque::with_capacity(period),
            moments: ShiftedPairMoments::new(),
        })
    }

    /// Configured window of returns.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for RollingCorrelation {
    type Input = (f64, f64);
    type Output = f64;

    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (x, y) = input;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let Some((px, py)) = self.prev else {
            // First level in each channel: store it, no return yet.
            self.prev = Some((x, y));
            return None;
        };
        self.prev = Some((x, y));
        let (rx, ry) = (x - px, y - py);
        if self.window.len() == self.period {
            let (ox, oy) = self.window.pop_front().expect("non-empty");
            self.moments.evict(ox, oy);
        }
        self.window.push_back((rx, ry));
        self.moments.push(rx, ry);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let var_x = self.moments.var_a(self.period);
        let var_y = self.moments.var_b(self.period);
        let cov = self.moments.cov(self.period);
        let denom = (var_x * var_y).sqrt();
        if denom == 0.0 {
            // At least one return channel is flat: correlation is undefined.
            return Some(0.0);
        }
        Some((cov / denom).clamp(-1.0, 1.0))
    }

    fn reset(&mut self) {
        self.prev = None;
        self.window.clear();
        self.moments.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RollingCorrelation"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_two() {
        assert!(RollingCorrelation::new(0).is_err());
        assert!(RollingCorrelation::new(1).is_err());
        assert!(RollingCorrelation::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let rc = RollingCorrelation::new(14).unwrap();
        assert_eq!(rc.period(), 14);
        assert_eq!(rc.warmup_period(), 15);
        assert_eq!(rc.name(), "RollingCorrelation");
        assert!(!rc.is_ready());
    }

    #[test]
    fn warmup_needs_period_plus_one() {
        let mut rc = RollingCorrelation::new(3).unwrap();
        // First update only seeds the previous level ⇒ None.
        assert_eq!(rc.update((1.0, 1.0)), None);
        assert_eq!(rc.update((2.0, 3.0)), None); // 1 return
        assert_eq!(rc.update((3.0, 5.0)), None); // 2 returns
        assert!(rc.update((4.0, 7.0)).is_some()); // 3 returns ⇒ ready
        assert!(rc.is_ready());
    }

    #[test]
    fn comoving_returns_are_plus_one() {
        // y always moves by 2x x's move ⇒ perfectly correlated returns.
        let pairs: Vec<(f64, f64)> = (0..20)
            .map(|i| {
                let x = (f64::from(i) * 0.5).sin() * 10.0;
                (x, 2.0 * x + 100.0)
            })
            .collect();
        let last = RollingCorrelation::new(8)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 1.0, epsilon = 1e-9);
    }

    #[test]
    fn opposing_returns_are_minus_one() {
        let pairs: Vec<(f64, f64)> = (0..20)
            .map(|i| {
                let x = (f64::from(i) * 0.5).sin() * 10.0;
                (x, -1.5 * x + 50.0)
            })
            .collect();
        let last = RollingCorrelation::new(8)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, -1.0, epsilon = 1e-9);
    }

    #[test]
    fn flat_return_channel_yields_zero() {
        // y is constant ⇒ its returns are all zero ⇒ undefined ⇒ 0.
        let pairs: Vec<(f64, f64)> = (0..20).map(|i| (f64::from(i), 7.0)).collect();
        let last = RollingCorrelation::new(6)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn output_in_range() {
        let pairs: Vec<(f64, f64)> = (0..80)
            .map(|i| {
                let t = f64::from(i);
                (100.0 + t.sin() * 5.0, 50.0 + (t * 0.3).cos() * 3.0)
            })
            .collect();
        let mut rc = RollingCorrelation::new(20).unwrap();
        for v in rc.batch(&pairs).into_iter().flatten() {
            assert!((-1.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut rc = RollingCorrelation::new(4).unwrap();
        rc.batch(&[(1.0, 2.0), (2.0, 4.0), (3.0, 6.0), (4.0, 8.0), (5.0, 10.0)]);
        assert!(rc.is_ready());
        rc.reset();
        assert!(!rc.is_ready());
        assert_eq!(rc.update((1.0, 1.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..60)
            .map(|i| {
                let t = f64::from(i);
                (t.sin(), (t * 0.5).cos())
            })
            .collect();
        let batch = RollingCorrelation::new(14).unwrap().batch(&pairs);
        let mut rc = RollingCorrelation::new(14).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| rc.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut rc = RollingCorrelation::new(2).unwrap();
        assert_eq!(rc.update((f64::NAN, 1.0)), None);
        assert_eq!(rc.update((1.0, f64::INFINITY)), None);
        // First finite tick seeds prev; two more returns fill the window.
        assert_eq!(rc.update((1.0, 1.0)), None);
        assert_eq!(rc.update((2.0, 3.0)), None);
        assert!(rc.update((3.0, 5.0)).is_some());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            RollingCorrelation::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
