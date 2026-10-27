//! Coefficient of determination R² for the rolling OLS fit.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedTrend;
use crate::traits::Indicator;

/// R² (coefficient of determination) of the rolling least-squares fit.
///
/// Over the trailing window indexed `x = 0, 1, …, period − 1` the OLS line
/// `y = a + b·x` is fitted and the ratio of variance explained by the line
/// to total variance is reported:
///
/// ```text
/// slope        = (n·Σxy − Σx·Σy) / (n·Σxx − (Σx)²)
/// SS_total     = Σy² − n·ȳ²
/// SS_explained = slope² · ( denom / n )
/// R²           = SS_explained / SS_total                  if SS_total > 0
///              = 1                                        otherwise (flat window)
/// ```
///
/// A reading of `1.0` means the window lies on a straight line — perfect
/// linear fit. `0.0` means the slope is irrelevant; the trend explains none
/// of the variance. Mid-range values quantify how trending the recent price
/// action is, independent of the slope's sign or magnitude. Use it as a
/// trend-quality filter: a strategy that needs a clear trend can require
/// `R² > 0.7`, while a mean-reversion strategy can prefer `R² < 0.3`.
///
/// A flat window has `SS_total = 0`; the line is also flat and the fit is
/// trivially perfect, so the indicator returns `1.0` rather than dividing
/// by zero.
///
/// Each `update` is O(1) via the same rolling sums as
/// [`crate::LinearRegression`], plus a running `Σy²`. The output is
/// clamped to `[0, 1]` to absorb tiny floating-point cancellation.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, RSquared};
///
/// let mut indicator = RSquared::new(14).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct RSquared {
    period: usize,
    window: VecDeque<f64>,
    sum_x: f64,
    /// `n·Σxx − (Σx)²` — OLS denominator, constant in `period`.
    denom: f64,
    trend: ShiftedTrend,
}

impl RSquared {
    /// Construct a new rolling R² over `period` inputs.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` — a regression line
    /// is undefined for fewer than two points.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "R² needs period >= 2",
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
            sum_x,
            denom: n * sum_xx - sum_x * sum_x,
            trend: ShiftedTrend::new(),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for RSquared {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, value: f64) -> Option<f64> {
        if !value.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let front = self.window.pop_front().expect("non-empty");
            self.trend.slide(front);
        }
        let index = self.window.len();
        self.window.push_back(value);
        self.trend.push(value, index);
        if self.trend.needs_reseed(self.period) {
            self.trend.reseed(self.window.iter().copied());
        }

        if self.window.len() < self.period {
            return None;
        }
        let n = self.period as f64;
        let slope = (n * self.trend.sum_xy() - self.sum_x * self.trend.sum_y()) / self.denom;
        // Invariant under the shift, and this is the expression that was
        // collapsing: on shifted values its terms are of the order of the
        // deviation inside the window rather than of the price level.
        let mean_y = self.trend.sum_y() / n;
        let ss_total = (self.trend.sum_y_sq() - n * mean_y * mean_y).max(0.0);
        let s_xx = self.denom / n;
        let ss_explained = slope * slope * s_xx;
        if ss_total <= 0.0 {
            // Flat window: the fit is trivially perfect.
            return Some(1.0);
        }
        Some((ss_explained / ss_total).clamp(0.0, 1.0))
    }

    fn reset(&mut self) {
        self.window.clear();
        self.trend.reset();
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
        "RSquared"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_two() {
        assert!(RSquared::new(0).is_err());
        assert!(RSquared::new(1).is_err());
        assert!(RSquared::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let r = RSquared::new(14).unwrap();
        assert_eq!(r.period(), 14);
        assert_eq!(r.warmup_period(), 14);
        assert_eq!(r.name(), "RSquared");
    }

    #[test]
    fn perfect_line_is_one() {
        let prices: Vec<f64> = (0..30).map(|i| 2.0 * f64::from(i) + 5.0).collect();
        let mut r = RSquared::new(10).unwrap();
        for v in r.batch(&prices).into_iter().flatten() {
            assert_relative_eq!(v, 1.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn constant_series_is_one() {
        // SS_total is zero; the indicator must return 1 instead of NaN.
        let mut r = RSquared::new(5).unwrap();
        for v in r.batch(&[42.0; 20]).into_iter().flatten() {
            assert_relative_eq!(v, 1.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn output_stays_in_zero_one_range() {
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 5.0 + (f64::from(i) * 0.07).cos() * 12.0)
            .collect();
        let mut r = RSquared::new(20).unwrap();
        for v in r.batch(&prices).into_iter().flatten() {
            assert!((0.0..=1.0).contains(&v), "R² out of range: {v}");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut r = RSquared::new(5).unwrap();
        r.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(r.is_ready());
        r.reset();
        assert!(!r.is_ready());
        assert_eq!(r.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 50.0 + (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        let batch = RSquared::new(14).unwrap().batch(&prices);
        let mut b = RSquared::new(14).unwrap();
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

    /// This was the worst of the family. The coefficient divides one quantity
    /// built from raw power sums by another, so both collapse and the ratio
    /// keeps no meaning at all: scored against an exact rational computation
    /// over 301 windows at a price level of 1e8, the old form was 5.5e+04 out
    /// -- on a value defined to lie in [0, 1] -- and the clamp was the only
    /// thing keeping the output in range. It now sits at 1.1e-14.
    #[test]
    fn coefficient_at_a_high_price_level_matches_a_centred_fit() {
        const P: usize = 20;
        let data = high_level_series(400);
        let mut ind = RSquared::new(P).unwrap();
        let mut compared = 0_usize;
        let mut saw_imperfect_fit = false;
        for (i, &v) in data.iter().enumerate() {
            let Some(r2) = ind.update(v) else { continue };
            let window = &data[i + 1 - P..=i];
            let (_, mean, sse) = centred_fit(window);
            let tss: f64 = window.iter().map(|y| (y - mean) * (y - mean)).sum();
            let want = 1.0 - sse / tss;
            if want < 0.99 {
                saw_imperfect_fit = true;
            }
            compared += 1;
            assert_relative_eq!(r2, want, max_relative = 1e-9);
        }
        assert_eq!(compared, data.len() - ind.warmup_period() + 1);
        // Without this the clamp at 1.0 could carry the whole assertion.
        assert!(saw_imperfect_fit);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            RSquared::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
