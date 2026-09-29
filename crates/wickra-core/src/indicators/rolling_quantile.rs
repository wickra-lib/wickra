//! Rolling Quantile over a trailing window.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::sorted_window;
use crate::traits::Indicator;

/// The `quantile`-th quantile of the last `period` values, with linear
/// interpolation between order statistics.
///
/// ```text
/// h        = (period − 1) · quantile
/// lower    = ⌊h⌋
/// result   = sorted[lower] + (h − lower) · (sorted[lower + 1] − sorted[lower])
/// ```
///
/// This is the type-7 / NumPy-default `quantile` definition: `quantile = 0.0`
/// returns the window minimum, `0.5` the median, `1.0` the maximum, and
/// fractional values interpolate linearly between the bracketing order
/// statistics. Rolling quantiles are the building block for distribution-aware
/// thresholds — a price sitting above its rolling 90th-percentile, a volatility
/// regime split at the 25th/75th percentiles, robust band edges that ignore the
/// tails.
///
/// Each `update` is O(period log period): the window is copied into a scratch
/// buffer and sorted with total ordering (NaN-safe).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, RollingQuantile};
///
/// // Rolling median of the last 5 values.
/// let mut indicator = RollingQuantile::new(5, 0.5).unwrap();
/// let out = indicator.update(1.0);
/// assert!(out.is_none()); // warming up
/// ```
#[derive(Debug, Clone)]
pub struct RollingQuantile {
    period: usize,
    quantile: f64,
    window: VecDeque<f64>,
    /// The window's values in `total_cmp` order, kept sorted as it slides:
    /// bit for bit what sorting a copy of the window would give.
    scratch: Vec<f64>,
}

impl RollingQuantile {
    /// Construct a new rolling quantile.
    ///
    /// `quantile` selects the order statistic in `[0.0, 1.0]`.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0`, or
    /// [`Error::InvalidParameter`] if `quantile` is not a finite value in
    /// `[0.0, 1.0]`.
    pub fn new(period: usize, quantile: f64) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !quantile.is_finite() || !(0.0..=1.0).contains(&quantile) {
            return Err(Error::InvalidParameter {
                message: "rolling quantile must be a finite value in [0.0, 1.0]",
            });
        }
        Ok(Self {
            period,
            quantile,
            window: VecDeque::with_capacity(period),
            scratch: Vec::with_capacity(period),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured quantile in `[0.0, 1.0]`.
    pub const fn quantile(&self) -> f64 {
        self.quantile
    }
}

/// Linearly-interpolated quantile of a sorted, non-empty slice (type-7).
pub(crate) fn quantile_sorted(sorted: &[f64], quantile: f64) -> f64 {
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }
    let h = (n - 1) as f64 * quantile;
    let lower = h.floor();
    let idx = lower as usize;
    // `idx <= n - 1`: when `quantile == 1.0`, `h == n - 1` and `idx == n - 1`,
    // so the interpolation neighbour would be out of bounds — return the top.
    if idx >= n - 1 {
        return sorted[n - 1];
    }
    let frac = h - lower;
    sorted[idx] + frac * (sorted[idx + 1] - sorted[idx])
}

impl Indicator for RollingQuantile {
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
        Some(quantile_sorted(&self.scratch, self.quantile))
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
        "RollingQuantile"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            RollingQuantile::new(0, 0.5),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn rejects_out_of_range_quantile() {
        assert!(matches!(
            RollingQuantile::new(5, -0.1),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            RollingQuantile::new(5, 1.1),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            RollingQuantile::new(5, f64::NAN),
            Err(Error::InvalidParameter { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let q = RollingQuantile::new(14, 0.25).unwrap();
        assert_eq!(q.period(), 14);
        assert_relative_eq!(q.quantile(), 0.25, epsilon = 1e-12);
        assert_eq!(q.warmup_period(), 14);
        assert_eq!(q.name(), "RollingQuantile");
        assert!(!q.is_ready());
    }

    #[test]
    fn median_of_window() {
        // Window [5, 1, 3, 2, 4] sorted [1,2,3,4,5] → median 3.
        let mut q = RollingQuantile::new(5, 0.5).unwrap();
        let out = q.batch(&[5.0, 1.0, 3.0, 2.0, 4.0]);
        assert_relative_eq!(out[4].unwrap(), 3.0, epsilon = 1e-12);
    }

    #[test]
    fn min_and_max_quantiles() {
        let prices = [5.0, 1.0, 3.0, 2.0, 4.0];
        let lo = RollingQuantile::new(5, 0.0).unwrap().batch(&prices)[4].unwrap();
        let hi = RollingQuantile::new(5, 1.0).unwrap().batch(&prices)[4].unwrap();
        assert_relative_eq!(lo, 1.0, epsilon = 1e-12);
        assert_relative_eq!(hi, 5.0, epsilon = 1e-12);
    }

    #[test]
    fn interpolated_quantile() {
        // sorted [10,20,30,40]: q=0.25 → h=(4-1)*0.25=0.75 → 10 + 0.75*(20-10)=17.5.
        let mut q = RollingQuantile::new(4, 0.25).unwrap();
        let out = q.batch(&[40.0, 30.0, 20.0, 10.0]);
        assert_relative_eq!(out[3].unwrap(), 17.5, epsilon = 1e-12);
    }

    #[test]
    fn single_period_returns_value() {
        // period 1: window holds one value; quantile of a singleton is itself.
        let mut q = RollingQuantile::new(1, 0.3).unwrap();
        assert_relative_eq!(q.update(7.0).unwrap(), 7.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut q = RollingQuantile::new(5, 0.5).unwrap();
        q.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(q.is_ready());
        q.reset();
        assert!(!q.is_ready());
        assert_eq!(q.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let batch = RollingQuantile::new(14, 0.75).unwrap().batch(&prices);
        let mut b = RollingQuantile::new(14, 0.75).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }
}
