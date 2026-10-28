//! Holt's linear (double exponential) smoothing.

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Holt's linear method — double exponential smoothing with a level and a
/// trend component.
///
/// A single [`Ema`](crate::Ema) tracks only a *level* and therefore lags any
/// sustained trend. Holt's method adds a second smoothed state, the trend, and
/// reports the one-step-ahead forecast `level + trend`, which removes that lag
/// on trending data while still smoothing noise.
///
/// ```text
/// level_t = α · price_t        + (1 − α) · (level_{t-1} + trend_{t-1})
/// trend_t = β · (level_t − level_{t-1}) + (1 − β) · trend_{t-1}
/// output  = level_t + trend_t          (one-step-ahead forecast)
/// ```
///
/// `α ∈ (0, 1]` is the level smoothing constant and `β ∈ (0, 1]` the trend
/// smoothing constant. The state is seeded from the first two inputs
/// (`level = price_1`, `trend = price_1 − price_0`), so the first output lands
/// on the **second** input.
///
/// On a perfectly linear series the forecast is exact from the second bar
/// onward (for any `α`, `β`): if the level equals the current value and the
/// trend equals the slope, both invariants are preserved and `level + trend`
/// equals the next value.
///
/// # Example
///
/// ```
/// use wickra_core::{HoltWinters, Indicator};
///
/// let mut indicator = HoltWinters::new(0.2, 0.1).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct HoltWinters {
    alpha: f64,
    beta: f64,
    /// `(level, trend)` once seeded.
    state: Option<(f64, f64)>,
    /// First input, held until the second arrives to seed the trend.
    prev_price: Option<f64>,
}

impl HoltWinters {
    /// Construct Holt's linear smoother with level constant `alpha` and trend
    /// constant `beta`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if either constant is non-finite or
    /// outside `(0.0, 1.0]`.
    pub fn new(alpha: f64, beta: f64) -> Result<Self> {
        if !alpha.is_finite() || alpha <= 0.0 || alpha > 1.0 {
            return Err(Error::InvalidPeriod {
                message: "HoltWinters alpha must be in (0.0, 1.0]",
            });
        }
        if !beta.is_finite() || beta <= 0.0 || beta > 1.0 {
            return Err(Error::InvalidPeriod {
                message: "HoltWinters beta must be in (0.0, 1.0]",
            });
        }
        Ok(Self {
            alpha,
            beta,
            state: None,
            prev_price: None,
        })
    }

    /// Level smoothing constant `alpha`.
    pub const fn alpha(&self) -> f64 {
        self.alpha
    }

    /// Trend smoothing constant `beta`.
    pub const fn beta(&self) -> f64 {
        self.beta
    }

    /// Current smoothed level, if seeded.
    pub fn level(&self) -> Option<f64> {
        self.state.map(|(level, _)| level)
    }

    /// Current smoothed trend, if seeded.
    pub fn trend(&self) -> Option<f64> {
        self.state.map(|(_, trend)| trend)
    }

    /// Current one-step-ahead forecast `level + trend`, if seeded.
    pub fn value(&self) -> Option<f64> {
        self.state.map(|(level, trend)| level + trend)
    }
}

impl Indicator for HoltWinters {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, price: f64) -> Option<f64> {
        if !price.is_finite() {
            return None;
        }
        match self.state {
            None => {
                if let Some(prev) = self.prev_price {
                    // Second input: seed level and trend.
                    let level = price;
                    let trend = price - prev;
                    self.state = Some((level, trend));
                    Some(level + trend)
                } else {
                    // First input: hold it to seed the trend next time.
                    self.prev_price = Some(price);
                    None
                }
            }
            Some((level, trend)) => {
                let level_new = self.alpha * price + (1.0 - self.alpha) * (level + trend);
                let trend_new = self.beta * (level_new - level) + (1.0 - self.beta) * trend;
                self.state = Some((level_new, trend_new));
                Some(level_new + trend_new)
            }
        }
    }

    fn reset(&mut self) {
        self.state = None;
        self.prev_price = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // Two inputs are needed to seed the level and the trend.
        2
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.state.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "HoltWinters"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Independent reference for the steady-state recurrence.
    fn naive(prices: &[f64], alpha: f64, beta: f64) -> Vec<Option<f64>> {
        let mut state: Option<(f64, f64)> = None;
        let mut prev: Option<f64> = None;
        let mut out = Vec::with_capacity(prices.len());
        for &price in prices {
            let v = match state {
                None => {
                    if let Some(p0) = prev {
                        let level = price;
                        let trend = price - p0;
                        state = Some((level, trend));
                        Some(level + trend)
                    } else {
                        prev = Some(price);
                        None
                    }
                }
                Some((level, trend)) => {
                    let ln = alpha * price + (1.0 - alpha) * (level + trend);
                    let tn = beta * (ln - level) + (1.0 - beta) * trend;
                    state = Some((ln, tn));
                    Some(ln + tn)
                }
            };
            out.push(v);
        }
        out
    }

    #[test]
    fn rejects_invalid_alpha() {
        assert!(matches!(
            HoltWinters::new(0.0, 0.1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            HoltWinters::new(1.5, 0.1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            HoltWinters::new(f64::NAN, 0.1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn rejects_invalid_beta() {
        assert!(matches!(
            HoltWinters::new(0.2, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            HoltWinters::new(0.2, 1.5),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            HoltWinters::new(0.2, f64::INFINITY),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    /// Cover the const accessors `alpha` + `beta` and the Indicator-impl
    /// `warmup_period` + `name`.
    #[test]
    fn accessors_and_metadata() {
        let hw = HoltWinters::new(0.2, 0.1).unwrap();
        assert_relative_eq!(hw.alpha(), 0.2, epsilon = 1e-12);
        assert_relative_eq!(hw.beta(), 0.1, epsilon = 1e-12);
        assert_eq!(hw.warmup_period(), 2);
        assert_eq!(hw.name(), "HoltWinters");
    }

    #[test]
    fn warmup_then_seed_on_second_input() {
        let mut hw = HoltWinters::new(0.2, 0.1).unwrap();
        assert_eq!(hw.update(10.0), None);
        // Second input seeds level = 12, trend = 12 - 10 = 2 -> forecast 14.
        assert_relative_eq!(hw.update(12.0).unwrap(), 14.0, epsilon = 1e-12);
        assert_relative_eq!(hw.level().unwrap(), 12.0, epsilon = 1e-12);
        assert_relative_eq!(hw.trend().unwrap(), 2.0, epsilon = 1e-12);
    }

    #[test]
    fn linear_series_forecasts_exactly() {
        // On a perfect ramp the one-step forecast equals the next value, for
        // any alpha/beta, from the second bar onward.
        let prices: Vec<f64> = (1..=20).map(f64::from).collect();
        let mut hw = HoltWinters::new(0.3, 0.4).unwrap();
        let out = hw.batch(&prices);
        assert!(out[0].is_none());
        for (i, v) in out.iter().enumerate().skip(1) {
            // forecast at index i is the price at index i + 1 = (i + 2).
            assert_relative_eq!(v.unwrap(), (i + 2) as f64, epsilon = 1e-9);
        }
    }

    #[test]
    fn constant_series_yields_constant() {
        let mut hw = HoltWinters::new(0.2, 0.1).unwrap();
        let out = hw.batch(&[42.0_f64; 30]);
        for v in out.into_iter().skip(1).flatten() {
            assert_relative_eq!(v, 42.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn matches_naive_recurrence() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 10.0 + f64::from(i) * 0.2)
            .collect();
        let mut hw = HoltWinters::new(0.25, 0.15).unwrap();
        let got = hw.batch(&prices);
        let want = naive(&prices, 0.25, 0.15);
        for (g, w) in got.iter().zip(want.iter()) {
            assert_eq!(g.is_some(), w.is_some());
            if let (Some(a), Some(b)) = (g, w) {
                assert_relative_eq!(a, b, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut hw = HoltWinters::new(0.2, 0.1).unwrap();
        hw.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        assert!(hw.is_ready());
        hw.reset();
        assert!(!hw.is_ready());
        assert_eq!(hw.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=30).map(|i| f64::from(i) * 0.5).collect();
        let mut a = HoltWinters::new(0.3, 0.2).unwrap();
        let mut b = HoltWinters::new(0.3, 0.2).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut hw = HoltWinters::new(0.2, 0.1).unwrap();
        // Non-finite before any state returns None.
        assert_eq!(hw.update(f64::NAN), None);
        hw.update(10.0);
        hw.update(12.0).expect("seeded on second finite input");
        // Non-finite after seeding returns the current forecast unchanged.
        assert_eq!(hw.update(f64::NAN), None);
        assert_eq!(hw.update(f64::INFINITY), None);
    }

    #[test]
    fn value_is_the_last_forecast() {
        let mut hw = HoltWinters::new(0.2, 0.1).unwrap();
        assert_eq!(hw.value(), None);
        hw.update(10.0);
        assert_eq!(hw.value(), None);
        let seeded = hw.update(12.0);
        assert_eq!(hw.value(), seeded);
        let next = hw.update(13.0);
        assert_eq!(hw.value(), next);
    }
}
