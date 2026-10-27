//! Standard Error of the rolling least-squares regression.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Standard Error of the regression line fit over the last `period` inputs.
///
/// Over the trailing window indexed `x = 0, 1, …, period − 1` the OLS line
/// `y = a + b·x` is fitted, then:
///
/// ```text
/// slope     = (n·Σxy − Σx·Σy) / (n·Σxx − (Σx)²)
/// SS_total  = Σy² − n·ȳ²                            // total sum of squares
/// RSS       = SS_total − slope² · S_xx              // residual sum of squares
/// StdErr    = √( RSS / (n − 2) )                    // n − 2 residual d.o.f.
/// ```
///
/// where `S_xx = (n·Σxx − (Σx)²) / n` is the centred sum of squares of the
/// design.
///
/// This is the textbook **standard error of estimate** of OLS: it measures
/// the typical distance between the observed prices and the fitted line,
/// using the residual degrees of freedom `n − 2`. It is the spread that
/// drives [`crate::BollingerBands`]-style bands around a regression instead of
/// around an SMA — when the price hugs its trend, `StdErr` is small.
///
/// Each `update` is O(period): the `Σx` and `Σxx` terms depend only on
/// `period` and are precomputed once, but the residuals are summed directly
/// over the window rather than reconstructed from rolling sums.
///
/// That is a deliberate trade. The residual sum of squares *can* be written as
/// `Σ(y − ȳ)² − slope²·S_xx`, which slides in constant time, and this indicator
/// did exactly that. But the two terms converge as the fit improves, so the
/// subtraction cancels precisely when the answer is smallest: on a line
/// carrying a wobble of 1e-4 around a price of 100 the constant-time form was
/// 6.2e-08 out, and at a wobble of 1e-8 it was off by 215% — for the case the
/// indicator is most likely to be asked about, a market hugging its trend.
/// Summing the residuals costs one further pass over a window the indicator
/// already holds, and is what the sibling [`crate::StandardErrorBands`] and
/// [`crate::LinRegChannel`] have always done.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, StandardError};
///
/// let mut indicator = StandardError::new(14).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i) + (f64::from(i) * 0.5).sin());
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct StandardError {
    period: usize,
    window: VecDeque<f64>,
    /// `n·Σxx − (Σx)²` — OLS denominator, constant in `period`.
    denom: f64,
}

impl StandardError {
    /// Construct a new rolling standard error of regression.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 3` — the residual
    /// degrees of freedom `n − 2` would be non-positive.
    pub fn new(period: usize) -> Result<Self> {
        if period < 3 {
            return Err(Error::InvalidPeriod {
                message: "standard error needs period >= 3",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        let n = period as f64;
        let sum_x = n * (n - 1.0) / 2.0;
        let sum_xx = (n - 1.0) * n * (2.0 * n - 1.0) / 6.0;
        Ok(Self {
            period,
            window: VecDeque::with_capacity(period),
            denom: n * sum_xx - sum_x * sum_x,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for StandardError {
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
        let n = self.period as f64;
        // Two passes over the window, on deviations from its mean. On that
        // scale the fitted line passes through `(mean_x, 0)`, so a residual is
        // never formed as the difference of two numbers the size of the price,
        // and the residual sum of squares is never rebuilt by subtraction.
        let mean_x = (n - 1.0) / 2.0;
        // `S_xx = Σ(x − x̄)²` over the index, which is `denom / n` and depends
        // only on `period`.
        let s_xx = self.denom / n;
        // Anchored on a value from inside the window rather than on its mean.
        // The mean is a computed quantity carrying rounding at the scale of the
        // price -- around 1e-08 at a level of 1e8 -- and every residual would
        // inherit it. Subtracting a stored input instead is exact whenever the
        // two share an exponent, which prices within one window always do.
        let anchor = *self.window.front().expect("the window is full");
        let mut sum_z = 0.0;
        for &y in &self.window {
            sum_z += y - anchor;
        }
        let mean_z = sum_z / n;
        let mut sum_xz = 0.0;
        for (i, &y) in self.window.iter().enumerate() {
            sum_xz += (i as f64 - mean_x) * (y - anchor - mean_z);
        }
        let slope = sum_xz / s_xx;
        let mut rss = 0.0;
        for (i, &y) in self.window.iter().enumerate() {
            let residual = (y - anchor - mean_z) - slope * (i as f64 - mean_x);
            rss += residual * residual;
        }
        Some((rss / (n - 2.0)).sqrt())
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
        "StandardError"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_three() {
        assert!(StandardError::new(0).is_err());
        assert!(StandardError::new(2).is_err());
        assert!(StandardError::new(3).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let se = StandardError::new(14).unwrap();
        assert_eq!(se.period(), 14);
        assert_eq!(se.warmup_period(), 14);
        assert_eq!(se.name(), "StandardError");
    }

    #[test]
    fn perfect_line_has_zero_error() {
        // Residuals from a perfectly linear fit are zero, so SE = 0.
        let prices: Vec<f64> = (0..30).map(|i| 2.0 * f64::from(i) + 5.0).collect();
        let mut se = StandardError::new(10).unwrap();
        for v in se.batch(&prices).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut se = StandardError::new(5).unwrap();
        for v in se.batch(&[42.0; 20]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn matches_naive_definition() {
        // Compare the O(1) update against a fresh-from-scratch OLS refit each bar.
        fn naive(window: &[f64]) -> f64 {
            let n = window.len() as f64;
            let mean_y = window.iter().sum::<f64>() / n;
            let mut sum_xy = 0.0;
            let mut sum_x = 0.0;
            let mut sum_xx = 0.0;
            for (i, &y) in window.iter().enumerate() {
                let x = i as f64;
                sum_xy += x * y;
                sum_x += x;
                sum_xx += x * x;
            }
            let mean_x = sum_x / n;
            let s_xx = sum_xx - n * mean_x * mean_x;
            let slope = (sum_xy - n * mean_x * mean_y) / s_xx;
            let intercept = mean_y - slope * mean_x;
            let rss: f64 = window
                .iter()
                .enumerate()
                .map(|(i, &y)| {
                    let r = y - (intercept + slope * i as f64);
                    r * r
                })
                .sum();
            (rss / (n - 2.0)).sqrt()
        }

        let prices: Vec<f64> = (0..60)
            .map(|i| 100.0 + f64::from(i) * 0.5 + (f64::from(i) * 0.7).sin() * 3.0)
            .collect();
        let period = 14;
        let got = StandardError::new(period).unwrap().batch(&prices);
        for (i, g) in got.iter().enumerate() {
            if let Some(v) = g {
                let expected = naive(&prices[i + 1 - period..=i]);
                assert_relative_eq!(*v, expected, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut se = StandardError::new(5).unwrap();
        se.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(se.is_ready());
        se.reset();
        assert!(!se.is_ready());
        assert_eq!(se.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 10.0)
            .collect();
        let batch = StandardError::new(14).unwrap().batch(&prices);
        let mut b = StandardError::new(14).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    /// Least-squares fit of a window against its own index, computed entirely
    /// on deviations. Returns `(slope, mean, sse)`.
    ///
    /// Forming the residuals as `y - (intercept + slope*i)` instead, with both
    /// sides the size of the price, is exactly what this file stopped doing:
    /// at a price level of 1e8 that subtraction alone costs eight digits. On
    /// the centred scale the fitted line is just `slope * (i - mean_x)`.
    fn centred_fit(window: &[f64]) -> (f64, f64, f64) {
        let n = window.len() as f64;
        let mean = window.iter().sum::<f64>() / n;
        let mean_x = (n - 1.0) / 2.0;
        let (mut sxy, mut sxx) = (0.0, 0.0);
        for (i, &y) in window.iter().enumerate() {
            let dx = i as f64 - mean_x;
            sxy += dx * (y - mean);
            sxx += dx * dx;
        }
        let slope = sxy / sxx;
        let mut sse = 0.0;
        for (i, &y) in window.iter().enumerate() {
            let r = (y - mean) - slope * (i as f64 - mean_x);
            sse += r * r;
        }
        (slope, mean, sse)
    }

    /// A one-unit wobble on top of a large price level: the level-to-deviation
    /// ratio is what drives the cancellation, and scaling the wobble with the
    /// level instead keeps that ratio constant and hides the defect entirely.
    fn high_level_series(bars: usize) -> Vec<f64> {
        (0..bars)
            .map(|i| {
                let t = i as f64;
                1e8 + ((t * 0.11).sin() + 0.4 * (t * 0.37).cos())
            })
            .collect()
    }

    /// The residual sum of squares was reconstructed by subtracting the
    /// explained variation from a total that was itself computed from raw power
    /// sums of the price. At a level of 1e8 the total collapsed far enough that
    /// the subtraction clamped to zero, so the indicator reported a *perfect*
    /// fit for a series it had not fitted at all -- a relative error of exactly
    /// 1. Scored against an exact rational computation it is now 3.1e-16.
    #[test]
    fn standard_error_at_a_high_price_level_does_not_collapse() {
        const P: usize = 20;
        let data = high_level_series(400);
        let mut ind = StandardError::new(P).unwrap();
        let mut compared = 0_usize;
        for (i, &v) in data.iter().enumerate() {
            let Some(stderr) = ind.update(v) else {
                continue;
            };
            let (_, _, sse) = centred_fit(&data[i + 1 - P..=i]);
            let want = (sse / (P as f64 - 2.0)).sqrt();
            assert!(stderr > 0.0, "collapsed to zero at bar {i}");
            compared += 1;
            assert_relative_eq!(stderr, want, max_relative = 1e-9);
        }
        assert_eq!(compared, data.len() - ind.warmup_period() + 1);
    }
    /// The residual sum of squares used to be rebuilt as
    /// `Σ(y − ȳ)² − slope²·S_xx`, which slides in constant time but cancels
    /// exactly when the fit is good and the answer is smallest. Scored against
    /// exact rational arithmetic on a straight line carrying a small wobble:
    ///
    /// ```text
    ///   wobble 1e-4 on a price of 100     6.2e-08  ->  5.5e-14
    ///   wobble 1e-8 on a price of 100     2.148    ->  7.4e-10
    ///   wobble 1e-4 on a price of 1e8     3.4e-02  ->  7.2e-11
    /// ```
    ///
    /// A relative error of 2.148 is not a rounding problem; the reported spread
    /// had no relationship to the data. A market hugging its trend is precisely
    /// what this indicator is asked about, so the constant-time form failed in
    /// its own best case.
    #[test]
    fn a_near_perfect_fit_still_reports_a_meaningful_spread() {
        const P: usize = 20;
        const BARS: usize = 240;
        const WOBBLE: f64 = 1e-8;

        let data: Vec<f64> = (0..BARS)
            .map(|i| {
                let t = i as f64;
                100.0 + 0.05 * t + WOBBLE * ((t * 1.7).sin() + 0.3 * (t * 0.41).cos())
            })
            .collect();

        let mut ind = StandardError::new(P).unwrap();
        let mean_x = (P as f64 - 1.0) / 2.0;
        let mut compared = 0_usize;
        for (i, &v) in data.iter().enumerate() {
            let Some(stderr) = ind.update(v) else {
                continue;
            };
            let window = &data[i + 1 - P..=i];
            let mean = window.iter().sum::<f64>() / P as f64;
            let (mut sxy, mut sxx) = (0.0, 0.0);
            for (j, &y) in window.iter().enumerate() {
                let dx = j as f64 - mean_x;
                sxy += dx * (y - mean);
                sxx += dx * dx;
            }
            let slope = sxy / sxx;
            let sse: f64 = window
                .iter()
                .enumerate()
                .map(|(j, &y)| {
                    let r = (y - mean) - slope * (j as f64 - mean_x);
                    r * r
                })
                .sum();
            let want = (sse / (P as f64 - 2.0)).sqrt();
            // The old form clamped to zero here and reported a perfect fit.
            assert!(stderr > 0.0, "collapsed to zero at bar {i}");
            compared += 1;
            assert_relative_eq!(stderr, want, max_relative = 1e-6);
        }
        assert_eq!(compared, data.len() - ind.warmup_period() + 1);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            StandardError::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
