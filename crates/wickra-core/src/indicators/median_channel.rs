//! Median Channel — a robust median ± MAD envelope.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_quantile::quantile_sorted;
use crate::indicators::sorted_window;
use crate::traits::Indicator;

/// Median Channel output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MedianChannelOutput {
    /// Upper band: `median + multiplier · MAD`.
    pub upper: f64,
    /// Middle line: the rolling median.
    pub middle: f64,
    /// Lower band: `median − multiplier · MAD`.
    pub lower: f64,
}

/// Median Channel: a robust analogue of Bollinger Bands built from the rolling
/// median and the median absolute deviation (MAD).
///
/// ```text
/// middle = median(close, period)
/// MAD    = median( | close_i − middle | )
/// upper  = middle + multiplier · MAD
/// lower  = middle − multiplier · MAD
/// ```
///
/// Where [`BollingerBands`](crate::BollingerBands) centre on the mean and scale
/// by the standard deviation — both of which a single spike can drag
/// arbitrarily far — the Median Channel uses two order statistics. The
/// breakdown point of the median and MAD is 50%: up to half the window can be
/// contaminated before the centre or width is materially distorted. That makes
/// the channel well suited to noisy, gap-prone, or fat-tailed series where
/// Bollinger Bands flare on every outlier. Both quantiles use the type-7
/// interpolation shared with [`RollingQuantile`](crate::RollingQuantile).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, MedianChannel};
///
/// let mut indicator = MedianChannel::new(20, 2.0).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i % 5));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct MedianChannel {
    period: usize,
    multiplier: f64,
    window: VecDeque<f64>,
    /// The window's values in `total_cmp` order, kept sorted as it slides.
    scratch: Vec<f64>,
    /// The absolute deviations from the median, in ascending order.
    deviations: Vec<f64>,
}

impl MedianChannel {
    /// Construct a new Median Channel.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0`, or
    /// [`Error::NonPositiveMultiplier`] if `multiplier` is not strictly
    /// positive and finite.
    pub fn new(period: usize, multiplier: f64) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !multiplier.is_finite() || multiplier <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        Ok(Self {
            period,
            multiplier,
            window: VecDeque::with_capacity(period),
            scratch: Vec::with_capacity(period),
            deviations: Vec::with_capacity(period),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured multiplier.
    pub const fn multiplier(&self) -> f64 {
        self.multiplier
    }
}

impl Indicator for MedianChannel {
    type Input = f64;
    type Output = MedianChannelOutput;

    #[inline]
    fn update(&mut self, value: f64) -> Option<MedianChannelOutput> {
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
        let median = quantile_sorted(&self.scratch, 0.5);

        // The absolute deviations, sorted by merging the runs either side of
        // the median.
        sorted_window::abs_deviations(&self.scratch, median, &mut self.deviations);
        let mad = quantile_sorted(&self.deviations, 0.5);
        let offset = self.multiplier * mad;

        Some(MedianChannelOutput {
            upper: median + offset,
            middle: median,
            lower: median - offset,
        })
    }

    fn reset(&mut self) {
        self.window.clear();
        self.scratch.clear();
        self.deviations.clear();
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
        "MedianChannel"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(MedianChannel::new(0, 2.0), Err(Error::PeriodZero)));
        assert!(MedianChannel::new(1, 2.0).is_ok());
    }

    #[test]
    fn rejects_non_positive_multiplier() {
        assert!(matches!(
            MedianChannel::new(20, 0.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            MedianChannel::new(20, -1.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            MedianChannel::new(20, f64::NAN),
            Err(Error::NonPositiveMultiplier)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let mc = MedianChannel::new(20, 2.0).unwrap();
        assert_eq!(mc.period(), 20);
        assert_relative_eq!(mc.multiplier(), 2.0, epsilon = 1e-12);
        assert_eq!(mc.warmup_period(), 20);
        assert_eq!(mc.name(), "MedianChannel");
        assert!(!mc.is_ready());
    }

    #[test]
    fn warms_up_then_emits() {
        let mut mc = MedianChannel::new(5, 2.0).unwrap();
        for v in [1.0, 2.0, 3.0, 4.0] {
            assert!(mc.update(v).is_none());
        }
        assert!(mc.update(5.0).is_some());
        assert!(mc.is_ready());
    }

    #[test]
    fn known_channel() {
        // [1,2,3,4,5]: median 3; |dev| sorted [0,1,1,2,2] -> MAD 1.
        // upper = 3 + 2*1 = 5; lower = 3 - 2*1 = 1.
        let mut mc = MedianChannel::new(5, 2.0).unwrap();
        let out = mc.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let last = out[4].unwrap();
        assert_relative_eq!(last.middle, 3.0, epsilon = 1e-12);
        assert_relative_eq!(last.upper, 5.0, epsilon = 1e-12);
        assert_relative_eq!(last.lower, 1.0, epsilon = 1e-12);
    }

    #[test]
    fn robust_to_outlier() {
        // Replacing the last value with a huge spike leaves the median centre
        // unchanged (still the middle order statistic).
        let mut mc = MedianChannel::new(5, 2.0).unwrap();
        let out = mc.batch(&[1.0, 2.0, 3.0, 4.0, 1_000.0]);
        assert_relative_eq!(out[4].unwrap().middle, 3.0, epsilon = 1e-12);
    }

    #[test]
    fn rolling_window_evicts_oldest() {
        // Ten values through a period-5 window: only the last five survive,
        // reproducing the `known_channel` window.
        let mut mc = MedianChannel::new(5, 2.0).unwrap();
        let out = mc.batch(&[10.0, 10.0, 10.0, 10.0, 10.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        let last = out[9].unwrap();
        assert_relative_eq!(last.middle, 3.0, epsilon = 1e-12);
        assert_relative_eq!(last.upper, 5.0, epsilon = 1e-12);
        assert_relative_eq!(last.lower, 1.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut mc = MedianChannel::new(5, 2.0).unwrap();
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            mc.update(v);
        }
        assert!(mc.is_ready());
        mc.reset();
        assert!(!mc.is_ready());
        assert!(mc.update(1.0).is_none());
    }
}
