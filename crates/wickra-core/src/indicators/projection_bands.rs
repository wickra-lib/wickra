//! Projection Bands (Mel Widner) — a high/low linear-regression projection
//! envelope.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Projection Bands output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectionBandsOutput {
    /// Upper band: the maximum forward-projected high in the window.
    pub upper: f64,
    /// Middle line: the midpoint of the upper and lower bands.
    pub middle: f64,
    /// Lower band: the minimum forward-projected low in the window.
    pub lower: f64,
}

/// Projection Bands: forward-projected high/low envelope.
///
/// Mel Widner ("Projection Bands and the Projection Oscillator", *Technical
/// Analysis of Stocks & Commodities*, May 1995) fits a separate linear
/// regression to the highs and to the lows over the last `period` bars, then
/// slides every bar's high and low forward to the current bar along its own
/// slope. The upper band is the maximum of the projected highs, the lower band
/// the minimum of the projected lows:
///
/// ```text
/// slope_h = OLS slope of (x, high) over the window
/// slope_l = OLS slope of (x, low)  over the window
/// // bar i (0 = oldest, period-1 = newest) is (period-1-i) bars in the past
/// upper   = max over i of [ high_i + slope_h · (period-1-i) ]
/// lower   = min over i of [ low_i  + slope_l · (period-1-i) ]
/// middle  = (upper + lower) / 2
/// ```
///
/// Unlike [`LinRegChannel`](crate::LinRegChannel) and
/// [`StandardErrorBands`](crate::StandardErrorBands) — which wrap a single
/// close-regression endpoint by a dispersion statistic — Projection Bands are
/// built from the *extremes*: the envelope adapts to the trend's slope yet
/// always contains every projected high and low, so by construction price never
/// pierces the bands within the window. A flat slope reduces the bands to the
/// rolling highest-high / lowest-low (a Donchian channel); a steep slope tilts
/// the whole envelope with the trend.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, ProjectionBands};
///
/// let mut indicator = ProjectionBands::new(14).unwrap();
/// let mut last = None;
/// for i in 0..30 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct ProjectionBands {
    period: usize,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
    sum_x: f64,
    sum_xx: f64,
}

impl ProjectionBands {
    /// Construct new Projection Bands.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` (a regression slope
    /// needs at least two points).
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "projection bands need period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        let n = period as f64;
        Ok(Self {
            period,
            highs: VecDeque::with_capacity(period),
            lows: VecDeque::with_capacity(period),
            sum_x: n * (n - 1.0) / 2.0,
            sum_xx: (n - 1.0) * n * (2.0 * n - 1.0) / 6.0,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// OLS slope of `(0..period, values)` over the live window.
    ///
    /// Computed about the window mean rather than from raw power sums: the
    /// slope is invariant under a shift of `y`, and the shifted form keeps
    /// every term on the scale of the deviation inside the window instead of
    /// the price level.
    fn slope(&self, values: &VecDeque<f64>) -> f64 {
        let n = self.period as f64;
        let mean_y = values.iter().sum::<f64>() / n;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        for (i, &y) in values.iter().enumerate() {
            let d = y - mean_y;
            sum_y += d;
            sum_xy += (i as f64) * d;
        }
        let denom = n * self.sum_xx - self.sum_x * self.sum_x;
        (n * sum_xy - self.sum_x * sum_y) / denom
    }
}

impl Indicator for ProjectionBands {
    type Input = Candle;
    type Output = ProjectionBandsOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<ProjectionBandsOutput> {
        if self.highs.len() == self.period {
            self.highs.pop_front();
            self.lows.pop_front();
        }
        self.highs.push_back(candle.high);
        self.lows.push_back(candle.low);
        if self.highs.len() < self.period {
            return None;
        }

        let slope_h = self.slope(&self.highs);
        let slope_l = self.slope(&self.lows);
        let last = (self.period - 1) as f64;

        let mut upper = f64::NEG_INFINITY;
        let mut lower = f64::INFINITY;
        for (i, (&high, &low)) in self.highs.iter().zip(self.lows.iter()).enumerate() {
            let forward = last - (i as f64);
            let projected_high = high + slope_h * forward;
            let projected_low = low + slope_l * forward;
            if projected_high > upper {
                upper = projected_high;
            }
            if projected_low < lower {
                lower = projected_low;
            }
        }

        Some(ProjectionBandsOutput {
            upper,
            middle: f64::midpoint(upper, lower),
            lower,
        })
    }

    fn reset(&mut self) {
        self.highs.clear();
        self.lows.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.highs.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ProjectionBands"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn candle(high: f64, low: f64, close: f64, ts: i64) -> Candle {
        Candle::new(low, high, low, close, 10.0, ts).unwrap()
    }

    #[test]
    fn rejects_period_below_two() {
        assert!(matches!(
            ProjectionBands::new(0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            ProjectionBands::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(ProjectionBands::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let pb = ProjectionBands::new(14).unwrap();
        assert_eq!(pb.period(), 14);
        assert_eq!(pb.warmup_period(), 14);
        assert_eq!(pb.name(), "ProjectionBands");
        assert!(!pb.is_ready());
    }

    #[test]
    fn warms_up_then_emits() {
        let mut pb = ProjectionBands::new(3).unwrap();
        assert!(pb.update(candle(10.0, 8.0, 9.0, 0)).is_none());
        assert!(pb.update(candle(12.0, 9.0, 11.0, 1)).is_none());
        assert!(pb.update(candle(11.0, 10.0, 11.0, 2)).is_some());
        assert!(pb.is_ready());
    }

    #[test]
    fn known_projection() {
        // highs 10,12,11 -> slope_h = 0.5; projected = 11, 12.5, 11 -> upper 12.5
        // lows   8, 9,10 -> slope_l = 1.0; projected = 10, 10,  10 -> lower 10
        let mut pb = ProjectionBands::new(3).unwrap();
        pb.update(candle(10.0, 8.0, 9.0, 0));
        pb.update(candle(12.0, 9.0, 11.0, 1));
        let out = pb.update(candle(11.0, 10.0, 11.0, 2)).unwrap();
        assert_relative_eq!(out.upper, 12.5, epsilon = 1e-9);
        assert_relative_eq!(out.lower, 10.0, epsilon = 1e-9);
        assert_relative_eq!(out.middle, 11.25, epsilon = 1e-9);
    }

    #[test]
    fn perfect_trend_pins_bands_to_current_extremes() {
        // High_i and Low_i both rise by exactly 1 per bar: every projected high
        // collapses onto the current high, every projected low onto the current
        // low.
        let mut pb = ProjectionBands::new(5).unwrap();
        let mut last = None;
        for i in 0..10 {
            let high = 100.0 + f64::from(i);
            let low = 95.0 + f64::from(i);
            last = pb.update(candle(high, low, high, i64::from(i)));
        }
        let out = last.unwrap();
        assert_relative_eq!(out.upper, 109.0, epsilon = 1e-9);
        assert_relative_eq!(out.lower, 104.0, epsilon = 1e-9);
        assert_relative_eq!(out.middle, 106.5, epsilon = 1e-9);
    }

    #[test]
    fn reset_clears_state() {
        let mut pb = ProjectionBands::new(3).unwrap();
        pb.update(candle(10.0, 8.0, 9.0, 0));
        pb.update(candle(12.0, 9.0, 11.0, 1));
        pb.update(candle(11.0, 10.0, 11.0, 2));
        assert!(pb.is_ready());
        pb.reset();
        assert!(!pb.is_ready());
        assert!(pb.update(candle(10.0, 8.0, 9.0, 3)).is_none());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            ProjectionBands::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
