//! Rolling Coefficient of Variation (`StdDev / Mean`).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::traits::Indicator;

/// Coefficient of Variation — the rolling population standard deviation
/// divided by the rolling mean.
///
/// ```text
/// mean = (1/n) · Σ price
/// sd   = √( (1/n) · Σ price² − mean² )
/// CV   = sd / mean
/// ```
///
/// CV is a dimensionless dispersion measure: it scales `StdDev` by the price
/// level so two assets at very different price magnitudes can be compared
/// directly. A higher CV means more relative variability for the same
/// average price.
///
/// When the rolling mean is exactly zero the ratio is undefined; the
/// indicator returns `0.0` in that degenerate case rather than producing a
/// `NaN`/infinity.
///
/// # Example
///
/// ```
/// use wickra_core::{CoefficientOfVariation, Indicator};
///
/// let mut indicator = CoefficientOfVariation::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct CoefficientOfVariation {
    period: usize,
    window: VecDeque<f64>,
    moments: ShiftedMoments,
}

impl CoefficientOfVariation {
    /// Construct a new rolling CV with the given period.
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
            moments: ShiftedMoments::new(),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for CoefficientOfVariation {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, value: f64) -> Option<f64> {
        if !value.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("non-empty");
            self.moments.evict(old);
        }
        self.window.push_back(value);
        self.moments.push(value);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let mean = self.moments.mean(self.period);
        let sd = self.moments.std_dev(self.period);
        if mean == 0.0 {
            // Undefined ratio: return 0 instead of NaN/inf so downstream
            // consumers can keep arithmetic going on flat or zeroed series.
            return Some(0.0);
        }
        Some(sd / mean)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.moments.reset();
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
        "CoefficientOfVariation"
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
            CoefficientOfVariation::new(0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let cv = CoefficientOfVariation::new(14).unwrap();
        assert_eq!(cv.period(), 14);
        assert_eq!(cv.warmup_period(), 14);
        assert_eq!(cv.name(), "CoefficientOfVariation");
    }

    #[test]
    fn reference_value() {
        // CV(3) of [2, 4, 6]: mean = 4, variance = 8/3, sd = √(8/3); CV = sd / 4.
        let mut cv = CoefficientOfVariation::new(3).unwrap();
        let out = cv.batch(&[2.0, 4.0, 6.0]);
        assert_eq!(out[0], None);
        let expected = (8.0_f64 / 3.0).sqrt() / 4.0;
        assert_relative_eq!(out[2].unwrap(), expected, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut cv = CoefficientOfVariation::new(5).unwrap();
        for o in cv.batch(&[42.0; 20]).into_iter().flatten() {
            assert_relative_eq!(o, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn zero_mean_returns_zero() {
        // [-1, 0, 1] has mean 0; the CV is defined to be 0 rather than NaN.
        let mut cv = CoefficientOfVariation::new(3).unwrap();
        let out = cv.batch(&[-1.0, 0.0, 1.0]);
        assert_relative_eq!(out[2].unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut cv = CoefficientOfVariation::new(5).unwrap();
        cv.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(cv.is_ready());
        cv.reset();
        assert!(!cv.is_ready());
        assert_eq!(cv.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 5.0)
            .collect();
        let batch = CoefficientOfVariation::new(14).unwrap().batch(&prices);
        let mut b = CoefficientOfVariation::new(14).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            CoefficientOfVariation::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
