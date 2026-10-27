//! Vertical Horizontal Filter.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Vertical Horizontal Filter — Adam White's trend-versus-range gauge.
///
/// ```text
/// VHF = (highest_close(n) − lowest_close(n)) / Σ|close − close_prev|(n)
/// ```
///
/// The numerator is the *net* distance price covered over the window; the
/// denominator is the *total* distance it walked. Their ratio lives in
/// `[0, 1]`: a clean trend walks almost only in its net direction, so `VHF`
/// approaches `1`; a choppy market doubles back constantly, inflating the
/// denominator and pushing `VHF` toward `0`. It answers the same question as
/// the [`ChoppinessIndex`](crate::ChoppinessIndex) on an inverted scale.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, VerticalHorizontalFilter};
///
/// let mut indicator = VerticalHorizontalFilter::new(28).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct VerticalHorizontalFilter {
    period: usize,
    closes: VecDeque<f64>,
    prev_close: Option<f64>,
    diffs: VecDeque<f64>,
    diff_sum: f64,
}

impl VerticalHorizontalFilter {
    /// Construct a new Vertical Horizontal Filter over `period` closes.
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
            closes: VecDeque::with_capacity(period),
            prev_close: None,
            diffs: VecDeque::with_capacity(period),
            diff_sum: 0.0,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for VerticalHorizontalFilter {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, value: f64) -> Option<f64> {
        if !value.is_finite() {
            return None;
        }
        if self.closes.len() == self.period {
            self.closes.pop_front();
        }
        self.closes.push_back(value);

        if let Some(prev) = self.prev_close {
            let diff = (value - prev).abs();
            if self.diffs.len() == self.period {
                self.diff_sum -= self.diffs.pop_front().expect("non-empty");
            }
            self.diffs.push_back(diff);
            self.diff_sum += diff;
        }
        self.prev_close = Some(value);

        if self.closes.len() < self.period || self.diffs.len() < self.period {
            return None;
        }
        let highest = self
            .closes
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let lowest = self.closes.iter().copied().fold(f64::INFINITY, f64::min);
        if self.diff_sum == 0.0 {
            // A flat window walked nowhere — no trend to filter.
            return Some(0.0);
        }
        Some((highest - lowest) / self.diff_sum)
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.prev_close = None;
        self.diffs.clear();
        self.diff_sum = 0.0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // `period` closes fill the high/low window; the `period`-th diff needs
        // one extra input because the first input has nothing to diff against.
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.diffs.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "VerticalHorizontalFilter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn reference_values_pure_uptrend() {
        // Closes 1,2,…: every diff is 1 (Σ = period), the n-close span is
        // period − 1, so VHF = (period − 1) / period. For period 5: 4/5 = 0.8.
        let mut vhf = VerticalHorizontalFilter::new(5).unwrap();
        let out = vhf.batch(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        for (i, v) in out.iter().enumerate().take(5) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        assert_relative_eq!(out[5].unwrap(), 0.8, epsilon = 1e-12);
        assert_eq!(vhf.warmup_period(), 6);
    }

    #[test]
    fn choppy_series_reads_low() {
        // A market that oscillates between two prices covers a tiny net span
        // while walking a long way -> VHF near zero.
        let prices: Vec<f64> = (0..40)
            .map(|i| if i % 2 == 0 { 10.0 } else { 11.0 })
            .collect();
        let mut vhf = VerticalHorizontalFilter::new(10).unwrap();
        for v in vhf.batch(&prices).into_iter().flatten() {
            assert!(v < 0.2, "a choppy series should read low, got {v}");
        }
    }

    #[test]
    fn flat_series_yields_zero() {
        let mut vhf = VerticalHorizontalFilter::new(8).unwrap();
        for v in vhf.batch(&[50.0; 20]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn stays_within_unit_range() {
        let prices: Vec<f64> = (0..120)
            .map(|i| 50.0 + (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        let mut vhf = VerticalHorizontalFilter::new(28).unwrap();
        for v in vhf.batch(&prices).into_iter().flatten() {
            assert!((0.0..=1.0).contains(&v), "VHF {v} outside [0, 1]");
        }
    }

    #[test]
    fn rejects_zero_period() {
        assert!(VerticalHorizontalFilter::new(0).is_err());
    }

    /// Cover the const accessor `period` (61-63) and the Indicator-impl
    /// `name` body (119-121). `warmup_period` is exercised elsewhere.
    #[test]
    fn accessors_and_metadata() {
        let vhf = VerticalHorizontalFilter::new(28).unwrap();
        assert_eq!(vhf.period(), 28);
        assert_eq!(vhf.name(), "VerticalHorizontalFilter");
    }

    #[test]
    fn reset_clears_state() {
        let mut vhf = VerticalHorizontalFilter::new(8).unwrap();
        vhf.batch(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
        assert!(vhf.is_ready());
        vhf.reset();
        assert!(!vhf.is_ready());
        assert_eq!(vhf.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 50.0 + (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        let mut a = VerticalHorizontalFilter::new(28).unwrap();
        let mut b = VerticalHorizontalFilter::new(28).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            VerticalHorizontalFilter::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
