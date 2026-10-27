//! Beta-neutral spread: the rolling OLS regression residual of two series.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedPairMoments;
use crate::traits::Indicator;

/// The beta-neutral spread between two assets — the residual of a rolling
/// ordinary-least-squares regression of `a` on `b`.
///
/// Each `update` takes one `(a, b)` price pair. Over the trailing window of
/// `period` pairs the indicator fits the hedge ratio `β` (and intercept `α`) by
/// OLS and reports the **current** residual:
///
/// ```text
/// β = cov(a, b) / var(b)        α = ā − β · b̄
/// spread = a_now − (α + β · b_now)
/// ```
///
/// Subtracting `β · b` removes `a`'s exposure to `b`, so the spread is market-
/// (beta-)neutral: it is what is left after the common factor is hedged out.
/// Positive means `a` is rich relative to its hedge, negative means cheap — the
/// raw signal a pairs trade fades. Where [`crate::PairSpreadZScore`] standardises
/// this residual into a z-score and [`crate::Cointegration`] bundles it with an
/// ADF test, this indicator returns the residual itself, in price units.
///
/// If `b` is flat over the window (`var(b) = 0`) there is no defined slope; the
/// indicator falls back to `β = 0`, so the spread becomes `a_now − ā`.
///
/// Each `update` is `O(1)`: four running sums (`Σa`, `Σb`, `Σb²`, `Σab`) are
/// maintained as the window slides.
///
/// # Example
///
/// ```
/// use wickra_core::{BetaNeutralSpread, Indicator};
///
/// let mut s = BetaNeutralSpread::new(20).unwrap();
/// let mut last = None;
/// for t in 0..40 {
///     let b = 100.0 + f64::from(t);
///     // a = 2·b + 5 exactly ⇒ the regression explains a fully ⇒ spread ≈ 0.
///     last = s.update((2.0 * b + 5.0, b));
/// }
/// assert!(last.unwrap().abs() < 1e-6);
/// ```
#[derive(Debug, Clone)]
pub struct BetaNeutralSpread {
    period: usize,
    window: VecDeque<(f64, f64)>,
    moments: ShiftedPairMoments,
}

impl BetaNeutralSpread {
    /// Construct a new beta-neutral spread.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` — a regression slope
    /// needs at least two points.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "beta-neutral spread needs period >= 2",
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
            moments: ShiftedPairMoments::new(),
        })
    }

    /// Configured look-back window.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for BetaNeutralSpread {
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (a, b) = input;
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let (oa, ob) = self.window.pop_front().expect("non-empty");
            self.moments.evict(oa, ob);
        }
        self.window.push_back((a, b));
        self.moments.push(a, b);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let mean_a = self.moments.mean_a(self.period);
        let mean_b = self.moments.mean_b(self.period);
        let var_b = self.moments.var_b(self.period);
        let (beta, intercept) = if var_b == 0.0 {
            (0.0, mean_a)
        } else {
            let cov = self.moments.cov(self.period);
            let slope = cov / var_b;
            (slope, mean_a - slope * mean_b)
        };
        Some(a - (intercept + beta * b))
    }

    fn reset(&mut self) {
        self.window.clear();
        self.moments.reset();
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
        "BetaNeutralSpread"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_two() {
        assert!(BetaNeutralSpread::new(1).is_err());
        assert!(BetaNeutralSpread::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let s = BetaNeutralSpread::new(20).unwrap();
        assert_eq!(s.period(), 20);
        assert_eq!(s.warmup_period(), 20);
        assert_eq!(s.name(), "BetaNeutralSpread");
        assert!(!s.is_ready());
    }

    #[test]
    fn warmup_returns_none() {
        let mut s = BetaNeutralSpread::new(3).unwrap();
        assert_eq!(s.update((1.0, 1.0)), None);
        assert_eq!(s.update((2.0, 2.0)), None);
        assert!(s.update((3.0, 3.0)).is_some());
        assert!(s.is_ready());
    }

    #[test]
    fn perfect_linear_relationship_has_zero_spread() {
        let pairs: Vec<(f64, f64)> = (0..40)
            .map(|t| {
                let b = 100.0 + f64::from(t);
                (2.0 * b + 5.0, b)
            })
            .collect();
        let last = BetaNeutralSpread::new(20)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-6);
    }

    #[test]
    fn dislocation_produces_nonzero_spread() {
        // a tracks 2·b, then the last bar jumps up ⇒ positive residual.
        let mut pairs: Vec<(f64, f64)> = (0..19)
            .map(|t| {
                let b = 100.0 + f64::from(t);
                (2.0 * b + 5.0, b)
            })
            .collect();
        pairs.push((2.0 * 119.0 + 5.0 + 10.0, 119.0));
        let last = BetaNeutralSpread::new(20)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last > 1.0, "spread {last}");
    }

    #[test]
    fn flat_b_falls_back_to_demeaned_a() {
        // b constant ⇒ β = 0 ⇒ spread = a − mean(a). Last window of a = 0..9,
        // mean = 4.5, last a = 9 ⇒ spread = 4.5.
        let pairs: Vec<(f64, f64)> = (0..10).map(|t| (f64::from(t), 7.0)).collect();
        let last = BetaNeutralSpread::new(10)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 4.5, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut s = BetaNeutralSpread::new(4).unwrap();
        s.batch(&[(1.0, 2.0), (2.0, 4.0), (3.0, 5.0), (4.0, 9.0), (5.0, 2.0)]);
        assert!(s.is_ready());
        s.reset();
        assert!(!s.is_ready());
        assert_eq!(s.update((1.0, 1.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..60)
            .map(|t| {
                let b = 30.0 + 0.7 * f64::from(t);
                (1.8 * b + 2.0 + (f64::from(t) * 0.4).sin(), b)
            })
            .collect();
        let batch = BetaNeutralSpread::new(20).unwrap().batch(&pairs);
        let mut s = BetaNeutralSpread::new(20).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| s.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut s = BetaNeutralSpread::new(3).unwrap();
        assert_eq!(s.update((f64::NAN, 1.0)), None);
        assert_eq!(s.update((1.0, f64::INFINITY)), None);
        // The rejected ticks leave no trace: a fresh window still warms up.
        assert_eq!(s.update((1.0, 2.0)), None);
        assert_eq!(s.update((2.0, 5.0)), None);
        assert!(s.update((3.0, 7.0)).is_some());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            BetaNeutralSpread::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
