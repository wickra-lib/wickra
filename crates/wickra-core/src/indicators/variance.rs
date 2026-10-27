//! Rolling population variance.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::traits::Indicator;

/// Rolling population variance over the last `period` values.
///
/// ```text
/// mean     = (1/n) · Σ price
/// Variance = (1/n) · Σ price² − mean²
/// ```
///
/// Variance is the squared standard deviation. It is the second central
/// moment of the rolling distribution and the natural input to risk
/// calculations that expect squared returns (e.g. portfolio variance,
/// covariance matrices). Use [`crate::StdDev`] when you need the
/// scale-preserving square root instead.
///
/// Floating-point cancellation can drive the running expression slightly
/// negative on perfectly constant inputs; the result is clamped to zero
/// before being returned so it stays a valid variance.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Variance};
///
/// let mut indicator = Variance::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Variance {
    period: usize,
    window: VecDeque<f64>,
    moments: ShiftedMoments,
}

impl Variance {
    /// Construct a new rolling variance with the given period.
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

impl Indicator for Variance {
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
        Some(self.moments.variance(self.period))
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
        "Variance"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Variance::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let v = Variance::new(14).unwrap();
        assert_eq!(v.period(), 14);
        assert_eq!(v.warmup_period(), 14);
        assert_eq!(v.name(), "Variance");
    }

    #[test]
    fn reference_value() {
        // Variance(3) of [2, 4, 6]: mean = 4, variance = (4 + 0 + 4) / 3 = 8/3.
        let mut v = Variance::new(3).unwrap();
        let out = v.batch(&[2.0, 4.0, 6.0]);
        assert_eq!(out[0], None);
        assert_eq!(out[1], None);
        assert_relative_eq!(out[2].unwrap(), 8.0 / 3.0, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut v = Variance::new(5).unwrap();
        for o in v.batch(&[42.0; 20]).into_iter().flatten() {
            assert_relative_eq!(o, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn first_value_on_period_th_input() {
        let mut v = Variance::new(5).unwrap();
        let out = v.batch(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        for (i, x) in out.iter().enumerate().take(4) {
            assert!(x.is_none(), "index {i} must be None during warmup");
        }
        assert!(out[4].is_some());
    }

    #[test]
    fn reset_clears_state() {
        let mut v = Variance::new(5).unwrap();
        v.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(v.is_ready());
        v.reset();
        assert!(!v.is_ready());
        assert_eq!(v.update(1.0), None);
    }

    #[test]
    fn equals_stddev_squared() {
        // The rolling Variance must equal the rolling population StdDev squared.
        let prices: Vec<f64> = (0..60)
            .map(|i| 50.0 + (f64::from(i) * 0.3).sin() * 7.0)
            .collect();
        let mut var = Variance::new(14).unwrap();
        let mut sd = crate::StdDev::new(14).unwrap();
        for &p in &prices {
            let (v, s) = (var.update(p), sd.update(p));
            assert_eq!(v.is_some(), s.is_some());
            if let (Some(v), Some(s)) = (v, s) {
                assert_relative_eq!(v, s * s, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 50.0 + (f64::from(i) * 0.3).cos() * 10.0)
            .collect();
        let batch = Variance::new(14).unwrap().batch(&prices);
        let mut b = Variance::new(14).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Variance::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
