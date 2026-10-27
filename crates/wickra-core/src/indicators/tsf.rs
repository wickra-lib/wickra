//! Time Series Forecast (TSF).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedTrend;
use crate::traits::Indicator;

/// Time Series Forecast (`TSF`): the rolling least-squares line projected one bar
/// past the window.
///
/// Over the last `period` inputs, indexed `x = 0, 1, …, period − 1`, it fits
/// `y = a + b·x` by ordinary least squares and reports the line's value at
/// `x = period` (one step beyond the most recent point):
///
/// ```text
/// b (slope)     = (n·Σxy − Σx·Σy) / (n·Σxx − (Σx)²)
/// a (intercept) = (Σy − b·Σx) / n
/// TSF           = a + b·period
/// ```
///
/// Where [`LinearRegression`](crate::LinearRegression) evaluates the fit at the
/// current bar (`a + b·(period − 1)`), `TSF` advances it one further bar, giving a
/// trend-following one-step-ahead forecast. Each update is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Tsf};
///
/// let mut indicator = Tsf::new(14).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Tsf {
    period: usize,
    window: VecDeque<f64>,
    sum_x: f64,
    denom: f64,
    trend: ShiftedTrend,
}

impl Tsf {
    /// Construct a new rolling time-series forecast over `period` inputs.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` — a regression line is
    /// undefined for fewer than two points.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "time series forecast needs period >= 2",
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

impl Indicator for Tsf {
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
        // A forecast of an absolute price level, so the reference point comes
        // back here; the slope it is projected along is invariant.
        let intercept = (self.trend.sum_y() - slope * self.sum_x) / n + self.trend.offset();
        Some(intercept + slope * n)
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
        "TSF"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_short_period() {
        assert!(matches!(Tsf::new(1), Err(Error::InvalidPeriod { .. })));
    }

    #[test]
    fn accessors_report_config() {
        let tsf = Tsf::new(5).unwrap();
        assert_eq!(tsf.period(), 5);
        assert_eq!(tsf.name(), "TSF");
        assert_eq!(tsf.warmup_period(), 5);
        assert!(!tsf.is_ready());
    }

    #[test]
    fn reference_value() {
        // period 3 over [1, 2, 9]: fit y = 0 + 4x, forecast at x = 3 is 12.
        let mut tsf = Tsf::new(3).unwrap();
        let out: Vec<Option<f64>> = tsf.batch(&[1.0, 2.0, 9.0]);
        assert!(out[0].is_none());
        assert!(out[1].is_none());
        assert_relative_eq!(out[2].unwrap(), 12.0, epsilon = 1e-9);
        assert!(tsf.is_ready());
    }

    #[test]
    fn forecasts_a_clean_line_one_step_ahead() {
        // Window [10, 12, 14]: y = 10 + 2x, forecast at x = 3 is 16.
        let mut tsf = Tsf::new(3).unwrap();
        let out: Vec<Option<f64>> = tsf.batch(&[1.0, 10.0, 12.0, 14.0]);
        assert_relative_eq!(out[3].unwrap(), 16.0, epsilon = 1e-9);
    }

    #[test]
    fn reset_clears_state() {
        let mut tsf = Tsf::new(3).unwrap();
        let _ = tsf.batch(&[1.0, 2.0, 9.0]);
        assert!(tsf.is_ready());
        tsf.reset();
        assert!(!tsf.is_ready());
        assert_eq!(tsf.update(1.0), None);
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

    /// The forecast is an absolute price level, so it restores the reference
    /// point the sums are held relative to. Matching a centred fit at a level
    /// of 1e8 pins that restoration together with the slope it projects along.
    #[test]
    fn forecast_at_a_high_price_level_matches_a_centred_fit() {
        const P: usize = 20;
        let data = high_level_series(400);
        let mut ind = Tsf::new(P).unwrap();
        let mut compared = 0_usize;
        for (i, &v) in data.iter().enumerate() {
            let Some(forecast) = ind.update(v) else {
                continue;
            };
            let (slope, mean, _) = centred_fit(&data[i + 1 - P..=i]);
            let mean_x = (P as f64 - 1.0) / 2.0;
            let want = mean + slope * (P as f64 - mean_x);
            compared += 1;
            assert_relative_eq!(forecast, want, max_relative = 1e-14);
        }
        assert_eq!(compared, data.len() - ind.warmup_period() + 1);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Tsf::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
