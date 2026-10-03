//! Rolling Interquartile Range (IQR) over a trailing window.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_quantile::quantile_sorted;
use crate::indicators::sorted_window;
use crate::traits::Indicator;

/// Interquartile Range of the last `period` values: `Q3 − Q1`.
///
/// ```text
/// IQR = quantile(0.75) − quantile(0.25)
/// ```
///
/// The IQR is the width of the central 50% of the window — the spread between
/// the third and first quartiles. It is a robust dispersion measure: unlike the
/// standard deviation it ignores the extreme tails entirely, so a single spike
/// barely moves it. That makes it the natural scale for outlier rules (the
/// classic *Tukey fence* flags points more than `1.5 · IQR` beyond a quartile)
/// and for volatility-regime splits that must not be dominated by one shock.
///
/// Both quartiles use the type-7 / NumPy-default linearly-interpolated
/// definition, identical to [`RollingQuantile`](crate::RollingQuantile). Each
/// `update` is O(period log period): the window is copied into a scratch buffer
/// and sorted once.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, RollingIqr};
///
/// let mut indicator = RollingIqr::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct RollingIqr {
    period: usize,
    window: VecDeque<f64>,
    /// The window's values in `total_cmp` order, kept sorted as it slides:
    /// bit for bit what sorting a copy of the window would give.
    scratch: Vec<f64>,
}

impl RollingIqr {
    /// Construct a new rolling IQR with the given period.
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

impl Indicator for RollingIqr {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, value: f64) -> Option<f64> {
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
        let q1 = quantile_sorted(&self.scratch, 0.25);
        let q3 = quantile_sorted(&self.scratch, 0.75);
        Some(q3 - q1)
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
        "RollingIqr"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(RollingIqr::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let iqr = RollingIqr::new(14).unwrap();
        assert_eq!(iqr.period(), 14);
        assert_eq!(iqr.warmup_period(), 14);
        assert_eq!(iqr.name(), "RollingIqr");
        assert!(!iqr.is_ready());
    }

    #[test]
    fn reference_value() {
        // sorted [10,20,30,40,50]: Q1 = q(0.25)= 10 + (4*0.25)*(...)= h=1.0 →20,
        // Q3 = q(0.75): h = 4*0.75 = 3.0 → 40. IQR = 40 - 20 = 20.
        let mut iqr = RollingIqr::new(5).unwrap();
        let out = iqr.batch(&[50.0, 40.0, 30.0, 20.0, 10.0]);
        assert_relative_eq!(out[4].unwrap(), 20.0, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut iqr = RollingIqr::new(8).unwrap();
        for v in iqr.batch(&[42.0; 20]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn output_is_non_negative() {
        let mut iqr = RollingIqr::new(20).unwrap();
        let prices: Vec<f64> = (1..=200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 12.0)
            .collect();
        for v in iqr.batch(&prices).into_iter().flatten() {
            assert!(v >= 0.0, "IQR must be non-negative, got {v}");
        }
    }

    #[test]
    fn ignores_single_extreme_outlier() {
        // 19 tightly-clustered values plus one huge spike: the central 50%
        // is unaffected, so the IQR stays small (well below the spike scale).
        let mut iqr = RollingIqr::new(20).unwrap();
        let mut prices = vec![5.0; 19];
        prices.push(10_000.0);
        let last = iqr.batch(&prices).into_iter().flatten().last().unwrap();
        assert!(last < 1.0, "spike leaked into IQR: {last}");
    }

    #[test]
    fn reset_clears_state() {
        let mut iqr = RollingIqr::new(5).unwrap();
        iqr.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(iqr.is_ready());
        iqr.reset();
        assert!(!iqr.is_ready());
        assert_eq!(iqr.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let batch = RollingIqr::new(14).unwrap().batch(&prices);
        let mut b = RollingIqr::new(14).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }
}
