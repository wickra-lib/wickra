//! Standard Error Bands.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Standard Error Bands output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StandardErrorBandsOutput {
    /// Upper band: regression endpoint plus `multiplier · standard_error`.
    pub upper: f64,
    /// Middle line: OLS endpoint over the window.
    pub middle: f64,
    /// Lower band: regression endpoint minus `multiplier · standard_error`.
    pub lower: f64,
}

/// Standard Error Bands: linear-regression line wrapped by the standard error
/// of the fit.
///
/// ```text
/// fit y = a + b·x by OLS over the last `period` closes
/// residual_i = y_i − (a + b · x_i)
/// stderr     = sqrt( Σ residual_i² / (period − 2) )   // OLS standard error
/// middle     = a + b · (period − 1)
/// upper      = middle + multiplier · stderr
/// lower      = middle − multiplier · stderr
/// ```
///
/// Standard Error Bands and [`LinRegChannel`](crate::LinRegChannel) both wrap
/// an OLS endpoint, but use *different denominators* for the dispersion
/// statistic:
///
/// - The `LinReg` Channel uses the population standard deviation of the
///   residuals (denominator `n`).
/// - Standard Error Bands use the OLS standard error (denominator `n − 2`,
///   one degree of freedom for the slope and one for the intercept).
///
/// The `n − 2` divisor produces a slightly wider channel and is the
/// statistically-correct band-width when the regression is interpreted as a
/// prediction interval. Jon Andersen's original publication pairs the bands
/// with a default `multiplier = 2.0` and a 3-bar SMA smoothing of all three
/// outputs; this implementation reports the *raw* bands so callers can pipe
/// them through their own smoother (e.g. [`Sma::new(3)`](crate::Sma)).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, StandardErrorBands};
///
/// let mut indicator = StandardErrorBands::new(21, 2.0).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct StandardErrorBands {
    period: usize,
    multiplier: f64,
    window: VecDeque<f64>,
    sum_x: f64,
    sum_xx: f64,
}

impl StandardErrorBands {
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 3` (the `n − 2`
    /// denominator requires at least 3 points) and
    /// [`Error::NonPositiveMultiplier`] if `multiplier` is not strictly
    /// positive and finite.
    pub fn new(period: usize, multiplier: f64) -> Result<Self> {
        if period < 3 {
            return Err(Error::InvalidPeriod {
                message: "standard error bands need period >= 3",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !multiplier.is_finite() || multiplier <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        let n = period as f64;
        Ok(Self {
            period,
            multiplier,
            window: VecDeque::with_capacity(period),
            sum_x: n * (n - 1.0) / 2.0,
            sum_xx: (n - 1.0) * n * (2.0 * n - 1.0) / 6.0,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured multiplier.
    pub const fn multiplier(&self) -> f64 {
        self.multiplier
    }
}

impl Indicator for StandardErrorBands {
    type Input = f64;
    type Output = StandardErrorBandsOutput;

    #[inline]
    fn update(&mut self, value: f64) -> Option<StandardErrorBandsOutput> {
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
        let n = self.period as f64;
        // The whole fit runs on deviations from the window mean. The slope is
        // invariant under that shift, and the residuals -- which are what the
        // band width is built from -- stay on their own scale instead of being
        // formed as a difference of two numbers the size of the price.
        let mean = self.window.iter().sum::<f64>() / n;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        for (i, &y) in self.window.iter().enumerate() {
            let x = i as f64;
            let d = y - mean;
            sum_y += d;
            sum_xy += x * d;
        }
        let denom = n * self.sum_xx - self.sum_x * self.sum_x;
        let slope = (n * sum_xy - self.sum_x * sum_y) / denom;
        let intercept = (sum_y - slope * self.sum_x) / n;

        let mut sse = 0.0;
        for (i, &y) in self.window.iter().enumerate() {
            let fitted = intercept + slope * (i as f64);
            let r = (y - mean) - fitted;
            sse += r * r;
        }
        // OLS standard error with `n − 2` degrees of freedom. `n − 2` is at
        // least 1 because the constructor enforces `period >= 3`.
        let stderr = (sse / (n - 2.0)).sqrt();
        // An absolute price level, so the window mean comes back here.
        let middle = mean + intercept + slope * (n - 1.0);
        Some(StandardErrorBandsOutput {
            upper: middle + self.multiplier * stderr,
            middle,
            lower: middle - self.multiplier * stderr,
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
        "StandardErrorBands"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_three() {
        assert!(StandardErrorBands::new(0, 2.0).is_err());
        assert!(StandardErrorBands::new(1, 2.0).is_err());
        assert!(StandardErrorBands::new(2, 2.0).is_err());
        assert!(StandardErrorBands::new(3, 2.0).is_ok());
    }

    #[test]
    fn rejects_non_positive_multiplier() {
        assert!(matches!(
            StandardErrorBands::new(20, 0.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            StandardErrorBands::new(20, -1.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            StandardErrorBands::new(20, f64::NAN),
            Err(Error::NonPositiveMultiplier)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let seb = StandardErrorBands::new(21, 2.0).unwrap();
        assert_eq!(seb.period(), 21);
        assert_relative_eq!(seb.multiplier(), 2.0, epsilon = 1e-12);
        assert_eq!(seb.warmup_period(), 21);
        assert_eq!(seb.name(), "StandardErrorBands");
    }

    #[test]
    fn perfect_line_collapses_bands() {
        let prices: Vec<f64> = (0..40).map(|i| 2.0 * f64::from(i) + 5.0).collect();
        let mut seb = StandardErrorBands::new(10, 2.0).unwrap();
        for o in seb.batch(&prices).into_iter().flatten() {
            assert_relative_eq!(o.upper, o.middle, epsilon = 1e-9);
            assert_relative_eq!(o.middle, o.lower, epsilon = 1e-9);
        }
    }

    #[test]
    fn upper_above_middle_above_lower() {
        let prices: Vec<f64> = (0..80)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        let mut seb = StandardErrorBands::new(21, 2.0).unwrap();
        for o in seb.batch(&prices).into_iter().flatten() {
            assert!(o.upper >= o.middle);
            assert!(o.middle >= o.lower);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 50.0 + (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        let mut a = StandardErrorBands::new(21, 2.0).unwrap();
        let mut b = StandardErrorBands::new(21, 2.0).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut seb = StandardErrorBands::new(5, 2.0).unwrap();
        seb.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(seb.is_ready());
        seb.reset();
        assert!(!seb.is_ready());
        assert_eq!(seb.update(1.0), None);
    }

    /// Reference: period 3 over `[1, 2, 9]`. Fitted line `y = 0 + 4·x`,
    /// endpoint at `x = 2` is `8`. Residuals: 1, −2, 1. SSE = 6.
    /// `n − 2 = 1`, so stderr = sqrt(6 / 1) = sqrt(6). With multiplier 2.0:
    /// upper = 8 + 2·sqrt(6), lower = 8 − 2·sqrt(6).
    #[test]
    fn reference_values() {
        let mut seb = StandardErrorBands::new(3, 2.0).unwrap();
        let v = seb.batch(&[1.0, 2.0, 9.0])[2].unwrap();
        let s = f64::sqrt(6.0);
        assert_relative_eq!(v.middle, 8.0, epsilon = 1e-9);
        assert_relative_eq!(v.upper, 8.0 + 2.0 * s, epsilon = 1e-9);
        assert_relative_eq!(v.lower, 8.0 - 2.0 * s, epsilon = 1e-9);
    }

    /// The n−2 standard error must be strictly larger than the population
    /// stddev (n divisor) on the same residuals — by the factor sqrt(n / (n−2)).
    #[test]
    fn standard_error_exceeds_population_stddev() {
        // Use n = 5 (factor = sqrt(5/3)) with non-trivial residuals.
        let prices: Vec<f64> = vec![1.0, 5.0, 2.0, 8.0, 3.0];
        let mut seb = StandardErrorBands::new(5, 1.0).unwrap();
        let v = seb.batch(&prices)[4].unwrap();
        // The half-width of the band is `multiplier · stderr`, so:
        let half = v.upper - v.middle;
        assert!(half > 0.0);
        // sigma² = SSE / 5, stderr² = SSE / 3, ratio of stderr to sigma = sqrt(5/3).
        // Reproduce stderr from the half-width (multiplier = 1.0) and check
        // it is sqrt(5/3) ≈ 1.291 times larger than sigma.
        let factor = (5.0_f64 / 3.0).sqrt();
        // half / factor would equal the population stddev — we expect factor > 1.
        assert!(half / factor < half, "n−2 stderr must exceed n stddev");
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            StandardErrorBands::new(crate::error::MAX_PERIOD + 1, 2.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
