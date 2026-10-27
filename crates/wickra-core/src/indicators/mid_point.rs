//! Midpoint (MIDPOINT) over a rolling window of a scalar series.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Midpoint (`MIDPOINT`): the average of the highest and lowest value of the
/// input series over the last `period` points.
///
/// ```text
/// MIDPOINT = (highest(value, period) + lowest(value, period)) / 2
/// ```
///
/// Where [`MidPrice`](crate::MidPrice) takes the window extremes from a candle's
/// high/low, `MIDPOINT` works on a single scalar stream (typically the close),
/// taking the max and min of that stream over the window. The first value is
/// emitted once `period` points have been seen.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, MidPoint};
///
/// let mut indicator = MidPoint::new(5).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct MidPoint {
    period: usize,
    window: VecDeque<f64>,
}

impl MidPoint {
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
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for MidPoint {
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
        let highest = self
            .window
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let lowest = self.window.iter().copied().fold(f64::INFINITY, f64::min);
        Some(f64::midpoint(highest, lowest))
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
        "MIDPOINT"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(MidPoint::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_report_config() {
        let mp = MidPoint::new(7).unwrap();
        assert_eq!(mp.period(), 7);
        assert_eq!(mp.name(), "MIDPOINT");
        assert_eq!(mp.warmup_period(), 7);
        assert!(!mp.is_ready());
    }

    #[test]
    fn averages_window_min_and_max() {
        // Window {8, 12, 10}: highest 12, lowest 8 -> 10.
        let mut mp = MidPoint::new(3).unwrap();
        let out: Vec<Option<f64>> = mp.batch(&[8.0, 12.0, 10.0]);
        assert_eq!(out[0], None);
        assert_eq!(out[1], None);
        assert_relative_eq!(out[2].unwrap(), 10.0, epsilon = 1e-12);
        assert!(mp.is_ready());
    }

    #[test]
    fn window_slides_and_drops_old_values() {
        // After the 30 spike leaves the window, the midpoint falls back.
        let mut mp = MidPoint::new(3).unwrap();
        let out: Vec<Option<f64>> = mp.batch(&[30.0, 8.0, 12.0, 10.0]);
        // Last window {8, 12, 10}: (12 + 8) / 2 = 10.
        assert_relative_eq!(out[3].unwrap(), 10.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut mp = MidPoint::new(3).unwrap();
        let _ = mp.batch(&[8.0, 12.0, 10.0]);
        assert!(mp.is_ready());
        mp.reset();
        assert!(!mp.is_ready());
        assert_eq!(mp.update(8.0), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            MidPoint::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
