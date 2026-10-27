//! AR(1) autoregression coefficient of the spread of two series.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::centred_moments;
use crate::traits::Indicator;

/// First-order autoregression coefficient `ρ` of the spread `a − b`.
///
/// Each `update` takes one `(a, b)` price pair and forms the spread
/// `sₜ = aₜ − bₜ`. Over the trailing window of `period` spreads the indicator
/// fits the discrete AR(1) model by ordinary least squares of the level on its
/// own lag:
///
/// ```text
/// sₜ = ρ · sₜ₋₁ + c + εₜ
/// ρ  = cov(sₜ₋₁, sₜ) / var(sₜ₋₁)
/// ```
///
/// `ρ` is the direct measure of cointegration / mean-reversion strength of the
/// pair:
///
/// - `ρ` near `0` — the spread snaps back to its mean almost instantly (very
///   strong mean reversion).
/// - `ρ` near `1` — the spread behaves like a random walk (a unit root: no
///   reliable reversion, the pair is *not* cointegrated).
/// - `ρ > 1` — the spread is explosive (diverging).
///
/// This is the complement of [`OuHalfLife`](crate::OuHalfLife): the OU half-life
/// is `−ln(2) / ln(ρ)` for `0 < ρ < 1`, but `ρ` itself is the raw, unbounded
/// stationarity statistic many pairs-trading screens threshold on directly
/// (e.g. "trade only pairs with `ρ < 0.9`"). When the spread is flat over the
/// window (`var(sₜ₋₁) = 0`) the regression slope is undefined and the indicator
/// returns `0`.
///
/// Each `update` is `O(period)`: the OLS slope is recomputed from the window's
/// running geometry.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, SpreadAr1Coefficient};
///
/// let mut ar1 = SpreadAr1Coefficient::new(40).unwrap();
/// let mut last = None;
/// for t in 0..120 {
///     let b = 100.0 + f64::from(t);
///     // `a` hugs `b` with a fast mean-reverting wobble ⇒ ρ well below 1.
///     let a = b + 2.0 * (f64::from(t) * 0.9).sin();
///     last = ar1.update((a, b));
/// }
/// let rho = last.unwrap();
/// assert!(rho > 0.0 && rho < 1.0);
/// ```
#[derive(Debug, Clone)]
pub struct SpreadAr1Coefficient {
    period: usize,
    window: VecDeque<f64>,
}

impl SpreadAr1Coefficient {
    /// Construct a new AR(1) spread-coefficient estimator.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 3` — the AR(1) regression
    /// needs at least two `(level, next)` observations (a slope and an
    /// intercept).
    pub fn new(period: usize) -> Result<Self> {
        if period < 3 {
            return Err(Error::InvalidPeriod {
                message: "AR(1) spread coefficient needs period >= 3",
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

impl Indicator for SpreadAr1Coefficient {
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
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
        // OLS slope ρ of the level on its own lag over the window. The pairs
        // are produced lazily and traversed twice rather than collected, so
        // this no longer allocates per update either.
        let moments = centred_moments(
            self.window
                .iter()
                .zip(self.window.iter().skip(1))
                .map(|(&level, &next)| (level, next)),
        );
        if moments.var_x <= 0.0 {
            // Flat spread: the regression has no defined slope.
            return Some(0.0);
        }
        Some(moments.cov / moments.var_x)
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
        "SpreadAr1Coefficient"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_three() {
        assert!(SpreadAr1Coefficient::new(2).is_err());
        assert!(SpreadAr1Coefficient::new(3).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let ar1 = SpreadAr1Coefficient::new(30).unwrap();
        assert_eq!(ar1.period(), 30);
        assert_eq!(ar1.warmup_period(), 30);
        assert_eq!(ar1.name(), "SpreadAr1Coefficient");
        assert!(!ar1.is_ready());
    }

    #[test]
    fn warmup_returns_none() {
        let mut ar1 = SpreadAr1Coefficient::new(4).unwrap();
        assert_eq!(ar1.update((1.0, 0.0)), None);
        assert_eq!(ar1.update((2.0, 0.0)), None);
        assert_eq!(ar1.update((3.0, 0.0)), None);
        assert!(ar1.update((4.0, 0.0)).is_some());
        assert!(ar1.is_ready());
    }

    #[test]
    fn mean_reverting_spread_has_rho_below_one() {
        // Fast sinusoidal spread around zero ⇒ stationary ⇒ 0 < ρ < 1.
        let pairs: Vec<(f64, f64)> = (0..120)
            .map(|t| {
                let b = 100.0 + f64::from(t);
                let a = b + 2.0 * (f64::from(t) * 0.9).sin();
                (a, b)
            })
            .collect();
        let last = SpreadAr1Coefficient::new(40)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last > 0.0 && last < 1.0, "rho {last}");
    }

    #[test]
    fn random_walk_spread_has_rho_near_one() {
        // Spread = a − b grows by exactly 1 each bar ⇒ next = level + 1 ⇒
        // the OLS slope is exactly 1 (unit root).
        let pairs: Vec<(f64, f64)> = (0..40)
            .map(|t| (2.0 * f64::from(t), f64::from(t)))
            .collect();
        let last = SpreadAr1Coefficient::new(20)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 1.0, epsilon = 1e-9);
    }

    #[test]
    fn flat_spread_returns_zero() {
        // a − b is constant ⇒ var(level) = 0 ⇒ undefined ⇒ 0.
        let pairs: Vec<(f64, f64)> = (0..30)
            .map(|t| (5.0 + f64::from(t), f64::from(t)))
            .collect();
        let last = SpreadAr1Coefficient::new(10)
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
        let mut ar1 = SpreadAr1Coefficient::new(5).unwrap();
        for t in 0..10 {
            ar1.update((f64::from(t) + (f64::from(t) * 0.7).sin(), f64::from(t)));
        }
        assert!(ar1.is_ready());
        ar1.reset();
        assert!(!ar1.is_ready());
        assert_eq!(ar1.update((1.0, 0.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..80)
            .map(|t| {
                let b = 50.0 + 0.5 * f64::from(t);
                (b + (f64::from(t) * 0.6).sin(), b)
            })
            .collect();
        let batch = SpreadAr1Coefficient::new(25).unwrap().batch(&pairs);
        let mut ar1 = SpreadAr1Coefficient::new(25).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| ar1.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut ar1 = SpreadAr1Coefficient::new(4).unwrap();
        assert_eq!(ar1.update((f64::NAN, 1.0)), None);
        assert_eq!(ar1.update((1.0, f64::INFINITY)), None);
        // The rejected ticks leave no trace: a fresh window still warms up.
        assert_eq!(ar1.update((1.0, 0.0)), None);
        assert_eq!(ar1.update((2.0, 0.0)), None);
        assert_eq!(ar1.update((3.0, 0.0)), None);
        assert!(ar1.update((4.0, 0.0)).is_some());
    }

    /// A cointegrated pair trades at two different price levels, so the spread
    /// carries a large constant offset with only a small wobble on top -- and
    /// the regression was accumulating raw power sums of that offset level.
    /// Two legs around 1e5 whose spread wobbles by 1e-3 measured 4.9e-08
    /// against a two-pass reference. Centring the window makes it exact.
    #[test]
    fn offset_spread_matches_a_two_pass_reference() {
        const PERIOD: usize = 20;
        let series: Vec<(f64, f64)> = (0..400)
            .map(|i| {
                let t = f64::from(i);
                let base = 1e5 * (1.0 + 0.01 * (t * 0.03).sin());
                (base * 1.05 + 1e-3 * (t * 0.23).sin(), base)
            })
            .collect();

        let mut ind = SpreadAr1Coefficient::new(PERIOD).unwrap();
        let mut spreads: Vec<f64> = Vec::new();
        let mut compared = 0_usize;
        for &(a, b) in &series {
            let got = ind.update((a, b));
            spreads.push(a - b);
            let Some(rho) = got else { continue };
            let window = &spreads[spreads.len() - PERIOD..];
            let levels = &window[..PERIOD - 1];
            let nexts = &window[1..];
            let n = (PERIOD - 1) as f64;
            let mean_level = levels.iter().sum::<f64>() / n;
            let mean_next = nexts.iter().sum::<f64>() / n;
            let var_level = levels
                .iter()
                .map(|v| (v - mean_level) * (v - mean_level))
                .sum::<f64>()
                / n;
            let cov = levels
                .iter()
                .zip(nexts)
                .map(|(u, v)| (u - mean_level) * (v - mean_next))
                .sum::<f64>()
                / n;
            compared += 1;
            assert_relative_eq!(rho, cov / var_level, max_relative = 1e-12);
        }
        assert_eq!(compared, series.len() - ind.warmup_period() + 1);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            SpreadAr1Coefficient::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
