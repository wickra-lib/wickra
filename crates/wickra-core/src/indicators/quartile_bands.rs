//! Quartile Bands — rolling 25th / 50th / 75th percentile envelope.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_quantile::quantile_sorted;
use crate::indicators::sorted_window;
use crate::traits::Indicator;

/// Quartile Bands output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuartileBandsOutput {
    /// Upper band: the rolling third quartile (75th percentile, `Q3`).
    pub upper: f64,
    /// Middle line: the rolling median (50th percentile, `Q2`).
    pub middle: f64,
    /// Lower band: the rolling first quartile (25th percentile, `Q1`).
    pub lower: f64,
}

/// Quartile Bands: a distribution-based envelope drawn at the rolling quartiles.
///
/// ```text
/// lower  = Q1  = 25th percentile of the last `period` values
/// middle = Q2  = 50th percentile (median)
/// upper  = Q3  = 75th percentile
/// ```
///
/// Quantiles use the type-7 (`NumPy`/`R-7`) linear interpolation shared with
/// [`RollingQuantile`](crate::RollingQuantile). Where Bollinger Bands assume an
/// approximately normal distribution and size the envelope by the mean and
/// standard deviation, Quartile Bands are fully **non-parametric**: the band
/// edges are order statistics, so a single outlier shifts at most one rank
/// rather than inflating the whole width, and the inter-quartile span between
/// the bands is exactly the [`RollingIqr`](crate::RollingIqr). The middle line
/// is the robust median rather than the mean, so it is unmoved by spikes.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, QuartileBands};
///
/// let mut indicator = QuartileBands::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct QuartileBands {
    period: usize,
    window: VecDeque<f64>,
    /// The window's values in `total_cmp` order, kept sorted as it slides:
    /// bit for bit what sorting a copy of the window would give.
    scratch: Vec<f64>,
}

impl QuartileBands {
    /// Construct new Quartile Bands.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            window: VecDeque::with_capacity(period),
            scratch: Vec::with_capacity(period),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for QuartileBands {
    type Input = f64;
    type Output = QuartileBandsOutput;

    #[inline]
    fn update(&mut self, value: f64) -> Option<QuartileBandsOutput> {
        if !value.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let oldest = self.window.pop_front().expect("window is full");
            sorted_window::remove(&mut self.scratch, oldest);
        }
        self.window.push_back(value);
        sorted_window::insert(&mut self.scratch, value);
        if self.window.len() < self.period {
            return None;
        }
        Some(QuartileBandsOutput {
            upper: quantile_sorted(&self.scratch, 0.75),
            middle: quantile_sorted(&self.scratch, 0.5),
            lower: quantile_sorted(&self.scratch, 0.25),
        })
    }

    fn reset(&mut self) {
        self.window.clear();
        self.scratch.clear();
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
        "QuartileBands"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(QuartileBands::new(0), Err(Error::PeriodZero)));
        assert!(QuartileBands::new(1).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let qb = QuartileBands::new(20).unwrap();
        assert_eq!(qb.period(), 20);
        assert_eq!(qb.warmup_period(), 20);
        assert_eq!(qb.name(), "QuartileBands");
        assert!(!qb.is_ready());
    }

    #[test]
    fn warms_up_then_emits() {
        let mut qb = QuartileBands::new(4).unwrap();
        assert!(qb.update(10.0).is_none());
        assert!(qb.update(20.0).is_none());
        assert!(qb.update(30.0).is_none());
        assert!(qb.update(40.0).is_some());
        assert!(qb.is_ready());
    }

    #[test]
    fn known_quartiles() {
        // sorted [10,20,30,40]:
        //   Q1 h=(4-1)*0.25=0.75 -> 10 + 0.75*10 = 17.5
        //   Q2 h=1.5            -> 20 + 0.5*10  = 25.0
        //   Q3 h=2.25           -> 30 + 0.25*10 = 32.5
        let mut qb = QuartileBands::new(4).unwrap();
        let out = qb.batch(&[40.0, 30.0, 20.0, 10.0]);
        let last = out[3].unwrap();
        assert_relative_eq!(last.lower, 17.5, epsilon = 1e-9);
        assert_relative_eq!(last.middle, 25.0, epsilon = 1e-9);
        assert_relative_eq!(last.upper, 32.5, epsilon = 1e-9);
    }

    #[test]
    fn median_robust_to_outlier() {
        // A single spike shifts the mean a lot but the median by at most one rank.
        let mut qb = QuartileBands::new(5).unwrap();
        let out = qb.batch(&[1.0, 2.0, 3.0, 4.0, 1000.0]);
        assert_relative_eq!(out[4].unwrap().middle, 3.0, epsilon = 1e-12);
    }

    #[test]
    fn rolling_window_evicts_oldest() {
        // Eight values through a period-4 window: only the last four survive,
        // reproducing the `known_quartiles` window.
        let mut qb = QuartileBands::new(4).unwrap();
        let out = qb.batch(&[1.0, 2.0, 3.0, 4.0, 40.0, 30.0, 20.0, 10.0]);
        let last = out[7].unwrap();
        assert_relative_eq!(last.lower, 17.5, epsilon = 1e-9);
        assert_relative_eq!(last.middle, 25.0, epsilon = 1e-9);
        assert_relative_eq!(last.upper, 32.5, epsilon = 1e-9);
    }

    #[test]
    fn reset_clears_state() {
        let mut qb = QuartileBands::new(4).unwrap();
        for v in [10.0, 20.0, 30.0, 40.0] {
            qb.update(v);
        }
        assert!(qb.is_ready());
        qb.reset();
        assert!(!qb.is_ready());
        assert!(qb.update(10.0).is_none());
    }
}
