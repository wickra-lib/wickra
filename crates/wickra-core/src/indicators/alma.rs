//! Arnaud Legoux Moving Average (ALMA).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Arnaud Legoux Moving Average — a Gaussian-weighted moving average.
///
/// Each output is a weighted sum of the last `period` inputs:
///
/// ```text
/// w[i] = exp(-(i - m)^2 / (2 * s^2))   for i in 0..period
/// m    = offset * (period - 1)
/// s    = period / sigma
/// ALMA = sum(price[i] * w[i]) / sum(w[i])
/// ```
///
/// The Gaussian is centred on the relative index `offset * (period - 1)`, so
/// `offset = 0.85` puts the peak near the newest sample (responsive), while
/// `offset = 0.5` centres the peak in the middle of the window (smooth).
/// `sigma` controls how concentrated the Gaussian is: larger `sigma` ->
/// narrower kernel, smaller `sigma` -> broader (closer to SMA).
///
/// Reference: Arnaud Legoux and Dimitrios Kouzis-Loukas, 2009.
///
/// # Defaults
///
/// The community-standard parameters are `period = 9`, `offset = 0.85`,
/// `sigma = 6.0`. The first output lands after exactly `period` inputs.
///
/// # Example
///
/// ```
/// use wickra_core::{Alma, Indicator};
///
/// let mut alma = Alma::new(9, 0.85, 6.0).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = alma.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Alma {
    period: usize,
    offset: f64,
    sigma: f64,
    /// Pre-computed, normalised weights (sum to 1). `weights[0]` is the oldest
    /// sample in the window, `weights[period - 1]` the newest.
    weights: Vec<f64>,
    window: VecDeque<f64>,
    current: Option<f64>,
}

impl Alma {
    /// Construct a new ALMA with the given period, offset and sigma.
    ///
    /// # Errors
    ///
    /// - [`Error::PeriodZero`] if `period == 0`.
    /// - [`Error::InvalidPeriod`] if `offset` is outside `[0.0, 1.0]` or
    ///   `sigma <= 0.0` or either of `offset` / `sigma` is non-finite.
    pub fn new(period: usize, offset: f64, sigma: f64) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !offset.is_finite() || !(0.0..=1.0).contains(&offset) {
            return Err(Error::InvalidPeriod {
                message: "ALMA offset must be a finite value in [0, 1]",
            });
        }
        if !sigma.is_finite() || sigma <= 0.0 {
            return Err(Error::InvalidPeriod {
                message: "ALMA sigma must be a finite positive value",
            });
        }
        let m = offset * (period as f64 - 1.0);
        let s = period as f64 / sigma;
        let denom = 2.0 * s * s;
        // The raw Gaussian weights sum to a strictly positive value because
        // every term is `exp(_) > 0`, so the normalisation below cannot divide
        // by zero.
        let mut raw: Vec<f64> = (0..period)
            .map(|i| (-((i as f64 - m).powi(2)) / denom).exp())
            .collect();
        let sum: f64 = raw.iter().sum();
        for w in &mut raw {
            *w /= sum;
        }
        Ok(Self {
            period,
            offset,
            sigma,
            weights: raw,
            window: VecDeque::with_capacity(period),
            current: None,
        })
    }

    /// Construct ALMA with the community-standard parameters
    /// `(period = 9, offset = 0.85, sigma = 6.0)`.
    pub fn classic() -> Self {
        Self::new(9, 0.85, 6.0).expect("classic ALMA parameters are valid")
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured offset.
    pub const fn offset(&self) -> f64 {
        self.offset
    }

    /// Configured sigma.
    pub const fn sigma(&self) -> f64 {
        self.sigma
    }
}

impl Indicator for Alma {
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
        let mut acc = 0.0;
        for (w, p) in self.weights.iter().zip(self.window.iter()) {
            acc += w * p;
        }
        self.current = Some(acc);
        Some(acc)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.current = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.current.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ALMA"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Alma::new(0, 0.85, 6.0), Err(Error::PeriodZero)));
    }

    #[test]
    fn rejects_invalid_offset() {
        assert!(matches!(
            Alma::new(9, -0.1, 6.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Alma::new(9, 1.1, 6.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Alma::new(9, f64::NAN, 6.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn rejects_invalid_sigma() {
        assert!(matches!(
            Alma::new(9, 0.85, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Alma::new(9, 0.85, -1.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Alma::new(9, 0.85, f64::INFINITY),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let alma = Alma::new(9, 0.85, 6.0).unwrap();
        assert_eq!(alma.period(), 9);
        assert_eq!(alma.warmup_period(), 9);
        assert_eq!(alma.name(), "ALMA");
        assert!((alma.offset() - 0.85).abs() < 1e-12);
        assert!((alma.sigma() - 6.0).abs() < 1e-12);
        // Weights are normalised by construction.
        let sum: f64 = alma.weights.iter().sum();
        assert_relative_eq!(sum, 1.0, epsilon = 1e-12);
    }

    #[test]
    fn classic_factory() {
        let a = Alma::classic();
        assert_eq!(a.period(), 9);
        assert!((a.offset() - 0.85).abs() < 1e-12);
        assert!((a.sigma() - 6.0).abs() < 1e-12);
    }

    #[test]
    fn constant_series_yields_the_constant() {
        // Normalised weights sum to 1, so any constant is reproduced exactly.
        let mut alma = Alma::new(9, 0.85, 6.0).unwrap();
        let out = alma.batch(&[42.0_f64; 40]);
        for v in out.iter().skip(8).flatten() {
            assert_relative_eq!(*v, 42.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn warmup_emits_first_value_at_period() {
        let mut alma = Alma::new(5, 0.85, 6.0).unwrap();
        for i in 0..4 {
            assert_eq!(alma.update(f64::from(i)), None);
        }
        assert!(alma.update(4.0).is_some());
    }

    #[test]
    fn reference_value_period_3() {
        // ALMA(period=3, offset=0.85, sigma=6) on [10, 20, 30].
        // m = 0.85 * 2 = 1.7;  s = 3 / 6 = 0.5;  2*s^2 = 0.5.
        // Independently compute the normalised Gaussian weights and the
        // expected weighted sum, then check the indicator output matches.
        // Computing the expectation here (rather than pinning a printed
        // constant) keeps the test stable across libm `exp` implementations.
        let mut alma = Alma::new(3, 0.85, 6.0).unwrap();
        alma.update(10.0);
        alma.update(20.0);
        let v = alma.update(30.0).expect("ALMA emits after period");

        let w0 = (-((0.0_f64 - 1.7).powi(2)) / 0.5).exp();
        let w1 = (-((1.0_f64 - 1.7).powi(2)) / 0.5).exp();
        let w2 = (-((2.0_f64 - 1.7).powi(2)) / 0.5).exp();
        let s = w0 + w1 + w2;
        let expected = (10.0 * w0 + 20.0 * w1 + 30.0 * w2) / s;

        // The weighted sum is heavily skewed toward the newest sample so the
        // output must sit close to but below the latest input (30).
        assert!(v > 25.0 && v < 30.0, "ALMA(3) on [10,20,30] = {v}");
        assert_relative_eq!(v, expected, epsilon = 1e-12);
    }

    #[test]
    fn offset_zero_centres_on_oldest_sample() {
        // With offset = 0 the Gaussian peaks at index 0, so ALMA leans toward
        // the oldest sample in the window and away from the newest.
        let mut alma = Alma::new(5, 0.0, 6.0).unwrap();
        let series: Vec<f64> = (1..=5).map(f64::from).collect();
        let mut last = None;
        for p in &series {
            last = alma.update(*p);
        }
        let v = last.unwrap();
        let mean = series.iter().sum::<f64>() / series.len() as f64;
        // Oldest sample is 1.0, mean is 3.0; an offset-0 ALMA should sit
        // strictly below the mean.
        assert!(v < mean, "{v} should be less than {mean}");
    }

    #[test]
    fn offset_one_centres_on_newest_sample() {
        // Symmetric to the above: offset = 1 leans toward the newest sample.
        let mut alma = Alma::new(5, 1.0, 6.0).unwrap();
        let series: Vec<f64> = (1..=5).map(f64::from).collect();
        let mut last = None;
        for p in &series {
            last = alma.update(*p);
        }
        let v = last.unwrap();
        let mean = series.iter().sum::<f64>() / series.len() as f64;
        assert!(v > mean, "{v} should exceed {mean}");
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=100)
            .map(|i| (f64::from(i) * 0.2).sin() * 5.0 + f64::from(i) * 0.1)
            .collect();
        let mut a = Alma::new(9, 0.85, 6.0).unwrap();
        let mut b = Alma::new(9, 0.85, 6.0).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut alma = Alma::new(9, 0.85, 6.0).unwrap();
        alma.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        assert!(alma.is_ready());
        alma.reset();
        assert!(!alma.is_ready());
        assert_eq!(alma.update(1.0), None);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut alma = Alma::new(5, 0.85, 6.0).unwrap();
        alma.batch(&(1..=5).map(f64::from).collect::<Vec<_>>());
        alma.update(6.0).unwrap();
        // Non-finite inputs leave the window/current untouched.
        assert_eq!(alma.update(f64::NAN), None);
        assert_eq!(alma.update(f64::INFINITY), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Alma::new(crate::error::MAX_PERIOD + 1, 0.85, 6.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
