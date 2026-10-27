//! Linear Regression Channel — OLS endpoint ± k · stddev of residuals.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Linear Regression Channel output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinRegChannelOutput {
    /// Upper channel: regression endpoint plus `multiplier · stddev` of the
    /// residuals.
    pub upper: f64,
    /// Middle line: OLS endpoint over the window.
    pub middle: f64,
    /// Lower channel: regression endpoint minus `multiplier · stddev` of the
    /// residuals.
    pub lower: f64,
}

/// Linear Regression Channel: rolling least-squares line with `±k·σ` bands
/// sized by the residuals about the fitted line.
///
/// ```text
/// fit y = a + b·x by OLS over the last `period` closes
/// residual_i = y_i − (a + b · x_i)
/// sigma      = sqrt( Σ residual_i² / period )      // population stddev
/// middle     = a + b · (period − 1)                // endpoint of the line
/// upper      = middle + multiplier · sigma
/// lower      = middle − multiplier · sigma
/// ```
///
/// Where [`BollingerBands`](crate::BollingerBands) measures dispersion about
/// the *mean*, the `LinReg` Channel measures it about the *trend*: detrended
/// residuals, so a steady drift up or down does not bias the band width. The
/// resulting envelope tracks the trend without flaring on momentum bursts —
/// breakouts are statistically meaningful in the direction of trend, not just
/// in absolute price.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, LinRegChannel};
///
/// let mut indicator = LinRegChannel::new(20, 2.0).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct LinRegChannel {
    period: usize,
    multiplier: f64,
    window: VecDeque<f64>,
    sum_x: f64,
    sum_xx: f64,
}

impl LinRegChannel {
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` and
    /// [`Error::NonPositiveMultiplier`] if `multiplier` is not strictly
    /// positive and finite.
    pub fn new(period: usize, multiplier: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "linear regression channel needs period >= 2",
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

impl Indicator for LinRegChannel {
    type Input = f64;
    type Output = LinRegChannelOutput;

    fn update(&mut self, value: f64) -> Option<LinRegChannelOutput> {
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
        // Recompute over the live window every bar. The OLS endpoint *could*
        // be maintained incrementally (see `LinearRegression`) but the
        // residual-stddev cannot be slid in closed form without storing each
        // residual; recomputing both keeps the code simple and is O(period)
        // per update — entirely acceptable for the periods used in practice.
        let n = self.period as f64;
        // The fit runs on deviations from the window mean. The slope is
        // invariant under that shift, and the residuals -- which the channel
        // width is built from -- stay on their own scale instead of being
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

        // Residuals about the fitted line, on the same centred scale.
        let mut sum_sq = 0.0;
        for (i, &y) in self.window.iter().enumerate() {
            let fitted = intercept + slope * (i as f64);
            let r = (y - mean) - fitted;
            sum_sq += r * r;
        }
        let sigma = (sum_sq / n).sqrt();
        // An absolute price level, so the window mean comes back here.
        let middle = mean + intercept + slope * (n - 1.0);
        Some(LinRegChannelOutput {
            upper: middle + self.multiplier * sigma,
            middle,
            lower: middle - self.multiplier * sigma,
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
        "LinRegChannel"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_two() {
        assert!(LinRegChannel::new(0, 2.0).is_err());
        assert!(LinRegChannel::new(1, 2.0).is_err());
        assert!(LinRegChannel::new(2, 2.0).is_ok());
    }

    #[test]
    fn rejects_non_positive_multiplier() {
        assert!(matches!(
            LinRegChannel::new(20, 0.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            LinRegChannel::new(20, -1.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            LinRegChannel::new(20, f64::NAN),
            Err(Error::NonPositiveMultiplier)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let lc = LinRegChannel::new(20, 2.0).unwrap();
        assert_eq!(lc.period(), 20);
        assert_relative_eq!(lc.multiplier(), 2.0, epsilon = 1e-12);
        assert_eq!(lc.warmup_period(), 20);
        assert_eq!(lc.name(), "LinRegChannel");
    }

    #[test]
    fn perfect_line_collapses_channel() {
        // A perfectly linear series has zero residuals, so upper == middle == lower.
        let prices: Vec<f64> = (0..40).map(|i| 2.0 * f64::from(i) + 5.0).collect();
        let mut lc = LinRegChannel::new(10, 2.0).unwrap();
        for o in lc.batch(&prices).into_iter().flatten() {
            assert_relative_eq!(o.upper, o.middle, epsilon = 1e-9);
            assert_relative_eq!(o.middle, o.lower, epsilon = 1e-9);
        }
    }

    #[test]
    fn constant_series_collapses_channel() {
        let mut lc = LinRegChannel::new(8, 2.0).unwrap();
        let out = lc.batch(&[42.0; 20]);
        let v = out.iter().rev().flatten().next().unwrap();
        assert_relative_eq!(v.middle, 42.0, epsilon = 1e-9);
        assert_relative_eq!(v.upper, 42.0, epsilon = 1e-9);
        assert_relative_eq!(v.lower, 42.0, epsilon = 1e-9);
    }

    #[test]
    fn upper_above_middle_above_lower() {
        let prices: Vec<f64> = (0..80)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        let mut lc = LinRegChannel::new(20, 2.0).unwrap();
        for o in lc.batch(&prices).into_iter().flatten() {
            assert!(o.upper >= o.middle);
            assert!(o.middle >= o.lower);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 50.0 + (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        let mut a = LinRegChannel::new(14, 2.0).unwrap();
        let mut b = LinRegChannel::new(14, 2.0).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut lc = LinRegChannel::new(5, 2.0).unwrap();
        lc.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(lc.is_ready());
        lc.reset();
        assert!(!lc.is_ready());
        assert_eq!(lc.update(1.0), None);
    }

    /// Reference: period 3 over `[1, 2, 9]`. Fitted line `y = 0 + 4·x`,
    /// endpoint at `x = 2` is `8`. Residuals: `1 − 0 = 1`, `2 − 4 = −2`,
    /// `9 − 8 = 1`. Population variance = (1 + 4 + 1) / 3 = 2, sigma = sqrt(2).
    /// With multiplier 2.0, upper = 8 + 2·sqrt(2), lower = 8 − 2·sqrt(2).
    #[test]
    fn reference_values() {
        let mut lc = LinRegChannel::new(3, 2.0).unwrap();
        let out = lc.batch(&[1.0, 2.0, 9.0]);
        let v = out[2].unwrap();
        let s2 = f64::sqrt(2.0);
        assert_relative_eq!(v.middle, 8.0, epsilon = 1e-9);
        assert_relative_eq!(v.upper, 8.0 + 2.0 * s2, epsilon = 1e-9);
        assert_relative_eq!(v.lower, 8.0 - 2.0 * s2, epsilon = 1e-9);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            LinRegChannel::new(crate::error::MAX_PERIOD + 1, 2.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
