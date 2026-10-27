//! Ornstein–Uhlenbeck half-life of mean reversion for the spread of two series.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::centred_moments;
use crate::traits::Indicator;

/// Half-life of mean reversion of the spread `a − b`, from an Ornstein–Uhlenbeck
/// fit.
///
/// Each `update` takes one `(a, b)` price pair and forms the spread
/// `sₜ = aₜ − bₜ`. Over the trailing window of `period` spreads the indicator
/// fits the discrete Ornstein–Uhlenbeck (mean-reverting AR(1)) model by
/// ordinary least squares of the change on the level:
///
/// ```text
/// Δsₜ = λ · sₜ₋₁ + c + εₜ
/// half_life = −ln(2) / λ        (only when λ < 0)
/// ```
///
/// `λ` is the speed of mean reversion: a more negative `λ` pulls the spread back
/// to its mean faster. The **half-life** is the number of bars for a deviation
/// to decay by half — the single most useful number for sizing a pairs trade's
/// holding period and look-back. When the spread is not mean-reverting
/// (`λ ≥ 0`, a random walk or a trend) or the regression is degenerate (a flat
/// spread), the indicator returns `0`, meaning "no finite half-life".
///
/// Each `update` is `O(period)`: the OLS slope is recomputed from the window's
/// running geometry. Output is in bars and is always `≥ 0`.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, OuHalfLife};
///
/// let mut hl = OuHalfLife::new(40).unwrap();
/// let mut last = None;
/// for t in 0..120 {
///     let b = 100.0 + f64::from(t);
///     // `a` hugs `b` with a fast mean-reverting wobble ⇒ short half-life.
///     let a = b + 2.0 * (f64::from(t) * 0.9).sin();
///     last = hl.update((a, b));
/// }
/// let half_life = last.unwrap();
/// assert!(half_life > 0.0 && half_life < 40.0);
/// ```
#[derive(Debug, Clone)]
pub struct OuHalfLife {
    period: usize,
    window: VecDeque<f64>,
}

impl OuHalfLife {
    /// Construct a new Ornstein–Uhlenbeck half-life estimator.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 3` — the AR(1) regression
    /// needs at least two observations (a slope and an intercept).
    pub fn new(period: usize) -> Result<Self> {
        if period < 3 {
            return Err(Error::InvalidPeriod {
                message: "OU half-life needs period >= 3",
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

    /// Configured look-back window of spreads.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for OuHalfLife {
    type Input = (f64, f64);
    type Output = f64;

    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (a, b) = input;
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(a - b);
        if self.window.len() < self.period {
            return None;
        }
        // OLS slope λ of Δsₜ on sₜ₋₁ over the window. The (level, change)
        // pairs are produced lazily and traversed twice rather than collected,
        // so this no longer allocates per update either.
        let moments = centred_moments(
            self.window
                .iter()
                .zip(self.window.iter().skip(1))
                .map(|(&level, &next)| (level, next - level)),
        );
        if moments.var_x <= 0.0 {
            // Flat spread: the regression has no defined slope.
            return Some(0.0);
        }
        let lambda = moments.cov / moments.var_x;
        if lambda >= 0.0 {
            // Not mean-reverting (random walk or diverging): no finite half-life.
            return Some(0.0);
        }
        Some(-std::f64::consts::LN_2 / lambda)
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
        "OuHalfLife"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_three() {
        assert!(OuHalfLife::new(2).is_err());
        assert!(OuHalfLife::new(3).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let hl = OuHalfLife::new(30).unwrap();
        assert_eq!(hl.period(), 30);
        assert_eq!(hl.warmup_period(), 30);
        assert_eq!(hl.name(), "OuHalfLife");
        assert!(!hl.is_ready());
    }

    #[test]
    fn warmup_returns_none() {
        let mut hl = OuHalfLife::new(4).unwrap();
        assert_eq!(hl.update((1.0, 0.0)), None);
        assert_eq!(hl.update((2.0, 0.0)), None);
        assert_eq!(hl.update((3.0, 0.0)), None);
        assert!(hl.update((4.0, 0.0)).is_some());
        assert!(hl.is_ready());
    }

    #[test]
    fn mean_reverting_spread_has_positive_half_life() {
        // Fast sinusoidal spread around zero ⇒ strong mean reversion.
        let pairs: Vec<(f64, f64)> = (0..120)
            .map(|t| {
                let b = 100.0 + f64::from(t);
                let a = b + 2.0 * (f64::from(t) * 0.9).sin();
                (a, b)
            })
            .collect();
        let last = OuHalfLife::new(40)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last > 0.0 && last < 40.0, "half-life {last}");
    }

    #[test]
    fn trending_spread_has_zero_half_life() {
        // Spread = a − b grows monotonically (λ ≥ 0) ⇒ no finite half-life.
        let pairs: Vec<(f64, f64)> = (0..40)
            .map(|t| (2.0 * f64::from(t), f64::from(t)))
            .collect();
        let last = OuHalfLife::new(20)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(last, 0.0);
    }

    #[test]
    fn flat_spread_returns_zero() {
        // a − b is constant ⇒ var(level) = 0 ⇒ undefined ⇒ 0.
        let pairs: Vec<(f64, f64)> = (0..30)
            .map(|t| (5.0 + f64::from(t), f64::from(t)))
            .collect();
        let last = OuHalfLife::new(10)
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
        let mut hl = OuHalfLife::new(5).unwrap();
        for t in 0..10 {
            hl.update((f64::from(t) + (f64::from(t) * 0.7).sin(), f64::from(t)));
        }
        assert!(hl.is_ready());
        hl.reset();
        assert!(!hl.is_ready());
        assert_eq!(hl.update((1.0, 0.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..80)
            .map(|t| {
                let b = 50.0 + 0.5 * f64::from(t);
                (b + (f64::from(t) * 0.6).sin(), b)
            })
            .collect();
        let batch = OuHalfLife::new(25).unwrap().batch(&pairs);
        let mut hl = OuHalfLife::new(25).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| hl.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut hl = OuHalfLife::new(4).unwrap();
        assert_eq!(hl.update((f64::NAN, 1.0)), None);
        assert_eq!(hl.update((1.0, f64::INFINITY)), None);
        // The rejected ticks leave no trace: a fresh window still warms up.
        assert_eq!(hl.update((1.0, 0.0)), None);
        assert_eq!(hl.update((2.0, 0.0)), None);
        assert_eq!(hl.update((3.0, 0.0)), None);
        assert!(hl.update((4.0, 0.0)).is_some());
    }

    /// Same defect and same regime as `SpreadAr1Coefficient`: the spread of a
    /// cointegrated pair sits at a large constant offset with only a small
    /// wobble on top, and the regression accumulated raw power sums of that
    /// offset level. On the series below -- a spread of 5000 wobbling by 0.1 --
    /// the one-pass form deviates from a two-pass reference by 2.8e-06, and it
    /// degrades as the spread tightens: 2.6e-04 at a wobble of 0.01. The
    /// half-life is the more sensitive of the two spread regressions because it
    /// inverts the slope. Centring the window makes it exact.
    #[test]
    fn offset_spread_matches_a_two_pass_reference() {
        const PERIOD: usize = 20;
        const BARS: usize = 400;

        // The wobble oscillates fast enough to mean-revert several times inside
        // a 20-bar window, so the regression reports a finite half-life rather
        // than taking the not-mean-reverting branch.
        let series: Vec<(f64, f64)> = (0..BARS)
            .map(|i| {
                let t = i as f64;
                let base = 1e5 * (1.0 + 0.002 * (t * 0.01).sin());
                (base + 5000.0 + 0.1 * (t * 0.9).sin(), base)
            })
            .collect();

        let mut ind = OuHalfLife::new(PERIOD).unwrap();
        let mut spreads: Vec<f64> = Vec::new();
        let mut mean_reverting = 0_usize;
        for &(a, b) in &series {
            let got = ind.update((a, b));
            spreads.push(a - b);
            let Some(half_life) = got else { continue };
            let window = &spreads[spreads.len() - PERIOD..];
            let levels = &window[..PERIOD - 1];
            let deltas: Vec<f64> = window.windows(2).map(|p| p[1] - p[0]).collect();
            let n = (PERIOD - 1) as f64;
            let mean_level = levels.iter().sum::<f64>() / n;
            let mean_delta = deltas.iter().sum::<f64>() / n;
            let var_level = levels
                .iter()
                .map(|v| (v - mean_level) * (v - mean_level))
                .sum::<f64>()
                / n;
            let lambda = levels
                .iter()
                .zip(&deltas)
                .map(|(u, v)| (u - mean_level) * (v - mean_delta))
                .sum::<f64>()
                / n
                / var_level;
            assert!(lambda < 0.0, "the probe series must mean-revert");
            mean_reverting += 1;
            assert_relative_eq!(
                half_life,
                -std::f64::consts::LN_2 / lambda,
                max_relative = 1e-12
            );
        }
        assert_eq!(mean_reverting, BARS - ind.warmup_period() + 1);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            OuHalfLife::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
