//! Tail Ratio — the right tail (95th percentile) over the absolute left tail (5th percentile).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::sorted_window;
use crate::traits::Indicator;

/// Tail Ratio over a trailing window of `period` returns.
///
/// ```text
/// TailRatio = P95(returns) / |P5(returns)|
/// ```
///
/// The Tail Ratio contrasts the magnitude of the best outcomes against the worst:
/// the 95th percentile of the return distribution divided by the absolute value of
/// the 5th percentile. A value above `1.0` means the right tail (upside surprises)
/// is fatter than the left tail (downside surprises); below `1.0` means crashes are
/// larger than rallies. It is a distribution-shape statistic, distinct from the
/// average-based [`SharpeRatio`](crate::SharpeRatio): two series with the same mean
/// and variance can have very different tail ratios.
///
/// Percentiles are computed by linear interpolation over the sorted window
/// (the same rule `NumPy` uses by default). A window whose 5th percentile is exactly
/// zero has no measurable left tail and the indicator reports `0.0` rather than
/// dividing by zero.
///
/// The first value lands after `period` returns; each `update` re-sorts the window
/// (O(period log period)), which is O(1) in the length of the overall series.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, TailRatio};
///
/// let mut indicator = TailRatio::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update((f64::from(i) * 0.3).sin() * 0.02);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct TailRatio {
    period: usize,
    window: VecDeque<f64>,
    /// The window's values in `total_cmp` order, kept sorted as it slides:
    /// bit for bit what sorting a copy of the window would give.
    scratch: Vec<f64>,
}

impl TailRatio {
    /// Construct a Tail Ratio over `period` returns.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `period < 2` (percentiles need at least
    /// two observations to interpolate).
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "tail ratio needs period >= 2",
            });
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

    /// Configured window of returns.
    pub const fn period(&self) -> usize {
        self.period
    }

    fn compute(&mut self) -> f64 {
        let upper = percentile(&self.scratch, 95.0);
        let lower = percentile(&self.scratch, 5.0).abs();
        if lower > 0.0 {
            upper / lower
        } else {
            0.0
        }
    }
}

/// Linear-interpolation percentile of an ascending, non-empty slice.
fn percentile(sorted: &[f64], pct: f64) -> f64 {
    let last_index = sorted.len() - 1;
    #[allow(clippy::cast_precision_loss)]
    let rank = pct / 100.0 * last_index as f64;
    let floor = rank.floor();
    // `rank` lies in `[0, last_index]`, so its floor is a valid in-bounds index.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lower = floor as usize;
    if lower >= last_index {
        return sorted[last_index];
    }
    let frac = rank - floor;
    sorted[lower] + frac * (sorted[lower + 1] - sorted[lower])
}

impl Indicator for TailRatio {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, ret: f64) -> Option<f64> {
        if !ret.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let oldest = self.window.pop_front().expect("window is full");
            sorted_window::remove(&mut self.scratch, oldest);
        }
        self.window.push_back(ret);
        sorted_window::insert(&mut self.scratch, ret);
        if self.window.len() < self.period {
            return None;
        }
        Some(self.compute())
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
        "TailRatio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_less_than_two() {
        assert!(matches!(
            TailRatio::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            TailRatio::new(0),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let tr = TailRatio::new(20).unwrap();
        assert_eq!(tr.period(), 20);
        assert_eq!(tr.warmup_period(), 20);
        assert_eq!(tr.name(), "TailRatio");
        assert!(!tr.is_ready());
    }

    #[test]
    fn reference_value() {
        // sorted window [-0.04, -0.02, 0.0, 0.02, 0.04], last_index = 4.
        // P95: rank 3.8 -> 0.02 + 0.8*(0.04-0.02) = 0.036.
        // P5:  rank 0.2 -> -0.04 + 0.2*(0.02)     = -0.036, abs 0.036.
        // ratio = 0.036 / 0.036 = 1.0.
        let mut tr = TailRatio::new(5).unwrap();
        let out = tr.batch(&[-0.04, -0.02, 0.0, 0.02, 0.04]);
        assert_relative_eq!(out[4].unwrap(), 1.0, epsilon = 1e-9);
    }

    #[test]
    fn fatter_right_tail_exceeds_one() {
        let mut tr = TailRatio::new(5).unwrap();
        let out = tr.batch(&[-0.01, 0.0, 0.01, 0.02, 0.10]);
        assert!(out[4].unwrap() > 1.0);
    }

    #[test]
    fn flat_window_is_zero() {
        let mut tr = TailRatio::new(4).unwrap();
        let last = tr.batch(&[0.0; 4]).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut tr = TailRatio::new(3).unwrap();
        assert_eq!(tr.update(0.01), None);
        assert_eq!(tr.update(f64::NAN), None);
        assert_eq!(tr.update(0.02), None);
        assert!(tr.update(0.03).is_some());
    }

    #[test]
    fn reset_clears_state() {
        let mut tr = TailRatio::new(3).unwrap();
        tr.batch(&[-0.01, 0.0, 0.02]);
        assert!(tr.is_ready());
        tr.reset();
        assert!(!tr.is_ready());
        assert_eq!(tr.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let rets: Vec<f64> = (0..60)
            .map(|i| (f64::from(i) * 0.25).sin() * 0.02)
            .collect();
        let batch = TailRatio::new(15).unwrap().batch(&rets);
        let mut streamer = TailRatio::new(15).unwrap();
        let streamed: Vec<_> = rets.iter().map(|r| streamer.update(*r)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn percentile_at_top_returns_last() {
        // When the rank floor reaches the final index (the 100th percentile), the
        // helper returns the largest element without interpolating past the end.
        assert_relative_eq!(percentile(&[1.0, 2.0, 3.0], 100.0), 3.0, epsilon = 1e-12);
    }
}
