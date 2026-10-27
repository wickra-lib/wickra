//! Rolling Min-Max Scaler — normalises the latest value to `[0, 1]` over a window.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Rolling Min-Max Scaler — maps the current value onto `[0, 1]` relative to the
/// minimum and maximum of the trailing window.
///
/// ```text
/// scaled = (x − min(window)) / (max(window) − min(window))
/// ```
///
/// This is the streaming form of scikit-learn's `MinMaxScaler` applied over a
/// sliding window: `0` means the value is the lowest in the window, `1` the
/// highest, `0.5` the midpoint of the range. It is the engine behind oscillators
/// like the Stochastic %K and a handy normaliser for feeding any indicator into a
/// bounded model input. Because it rescales to the window's own range it is
/// scale-free across instruments.
///
/// The output is in `[0, 1]`. A flat window (`max == min`) has no range to scale
/// against and returns the neutral `0.5`. The first value lands after `period`
/// inputs; each `update` scans the window in O(`period`).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, RollingMinMaxScaler};
///
/// let mut indicator = RollingMinMaxScaler::new(14).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct RollingMinMaxScaler {
    period: usize,
    window: VecDeque<f64>,
    last: Option<f64>,
}

impl RollingMinMaxScaler {
    /// Construct a rolling min-max scaler over `period` values.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0` and
    /// [`Error::InvalidPeriod`] if `period < 2` (a range needs two points).
    pub fn new(period: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "min-max scaler needs period >= 2",
            });
        }
        Ok(Self {
            period,
            window: VecDeque::with_capacity(period),
            last: None,
        })
    }

    /// Configured window length.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for RollingMinMaxScaler {
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
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for &v in &self.window {
            min = min.min(v);
            max = max.max(v);
        }
        let range = max - min;
        let scaled = if range > 0.0 {
            (input - min) / range
        } else {
            0.5
        };
        self.last = Some(scaled);
        Some(scaled)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RollingMinMaxScaler"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_invalid_period() {
        assert!(matches!(
            RollingMinMaxScaler::new(0),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            RollingMinMaxScaler::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(RollingMinMaxScaler::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let s = RollingMinMaxScaler::new(14).unwrap();
        assert_eq!(s.period(), 14);
        assert_eq!(s.warmup_period(), 14);
        assert_eq!(s.name(), "RollingMinMaxScaler");
        assert!(!s.is_ready());
        assert_eq!(s.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut s = RollingMinMaxScaler::new(4).unwrap();
        let out = s.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        for v in out.iter().take(3) {
            assert!(v.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn highest_in_window_is_one() {
        let mut s = RollingMinMaxScaler::new(4).unwrap();
        // last value is the highest -> 1.0.
        let last = s
            .batch(&[1.0, 2.0, 3.0, 4.0])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 1.0, epsilon = 1e-12);
    }

    #[test]
    fn lowest_in_window_is_zero() {
        let mut s = RollingMinMaxScaler::new(4).unwrap();
        let last = s
            .batch(&[4.0, 3.0, 2.0, 1.0])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn midpoint_is_half() {
        let mut s = RollingMinMaxScaler::new(3).unwrap();
        // window [0, 2, 1]: min 0, max 2, current 1 -> 0.5.
        let last = s
            .batch(&[0.0, 2.0, 1.0])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.5, epsilon = 1e-12);
    }

    #[test]
    fn flat_window_is_half() {
        let mut s = RollingMinMaxScaler::new(4).unwrap();
        let last = s.batch(&[7.0; 8]).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.5, epsilon = 1e-12);
    }

    #[test]
    fn output_in_range() {
        let mut s = RollingMinMaxScaler::new(14).unwrap();
        for v in s
            .batch(
                &(0..200)
                    .map(|i| (f64::from(i) * 0.3).sin() * 10.0)
                    .collect::<Vec<_>>(),
            )
            .into_iter()
            .flatten()
        {
            assert!((0.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn ignores_non_finite() {
        let mut s = RollingMinMaxScaler::new(4).unwrap();
        let _ready = s
            .batch(&[1.0, 2.0, 3.0, 4.0])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(s.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut s = RollingMinMaxScaler::new(4).unwrap();
        s.batch(&[1.0, 2.0, 3.0, 4.0]);
        assert!(s.is_ready());
        s.reset();
        assert!(!s.is_ready());
        assert_eq!(s.value(), None);
        assert_eq!(s.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let xs: Vec<f64> = (0..120)
            .map(|i| (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let batch = RollingMinMaxScaler::new(14).unwrap().batch(&xs);
        let mut b = RollingMinMaxScaler::new(14).unwrap();
        let streamed: Vec<_> = xs.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            RollingMinMaxScaler::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
