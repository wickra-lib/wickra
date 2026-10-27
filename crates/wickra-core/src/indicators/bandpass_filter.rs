//! Ehlers Bandpass Filter — isolates the cyclic component around a target period.
#![allow(clippy::doc_markdown)]

use std::f64::consts::PI;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Ehlers' Bandpass Filter — a two-pole resonator that passes the cyclic content
/// around a target `period` and rejects both the trend (low frequencies) and the
/// noise (high frequencies).
///
/// From John Ehlers' *Cycle Analytics for Traders* (2013):
///
/// ```text
/// beta  = cos(2π / period)
/// gamma = 1 / cos(4π · bandwidth / period)
/// alpha = gamma − sqrt(gamma² − 1)
/// BP_t  = 0.5·(1 − alpha)·(price_t − price_{t−2})
///         + beta·(1 + alpha)·BP_{t−1} − alpha·BP_{t−2}
/// ```
///
/// `bandwidth` (a fraction, typically `0.3`) sets how wide a band of periods is
/// admitted: narrow bandwidth gives a sharp, ringing resonator tuned tightly to
/// `period`; wide bandwidth lets more of the spectrum through. The output is a
/// zero-mean oscillator — it swings symmetrically around `0`, peaking when the
/// dominant cycle aligns with `period`. It is the building block for cycle-phase
/// and cycle-amplitude work.
///
/// The recursion needs two prior prices and two prior outputs; until then it emits
/// `0` (Ehlers' initial condition), so `warmup_period` is `1` and a value is
/// produced every bar. Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, BandpassFilter};
///
/// let mut indicator = BandpassFilter::new(20, 0.3).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct BandpassFilter {
    period: usize,
    bandwidth: f64,
    beta: f64,
    alpha: f64,
    prev_price_1: Option<f64>,
    prev_price_2: Option<f64>,
    bp1: f64,
    bp2: f64,
    last: Option<f64>,
}

impl BandpassFilter {
    /// Construct a bandpass filter tuned to `period` with the given `bandwidth`
    /// fraction.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0` and
    /// [`Error::InvalidParameter`] if `bandwidth` is not finite or outside
    /// `(0, 1)`.
    pub fn new(period: usize, bandwidth: f64) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !bandwidth.is_finite() || bandwidth <= 0.0 || bandwidth >= 1.0 {
            return Err(Error::InvalidParameter {
                message: "bandpass bandwidth must be in (0, 1)",
            });
        }
        let period_f = period as f64;
        let beta = (2.0 * PI / period_f).cos();
        let gamma = 1.0 / (4.0 * PI * bandwidth / period_f).cos();
        let alpha = gamma - (gamma * gamma - 1.0).sqrt();
        Ok(Self {
            period,
            bandwidth,
            beta,
            alpha,
            prev_price_1: None,
            prev_price_2: None,
            bp1: 0.0,
            bp2: 0.0,
            last: None,
        })
    }

    /// Configured `(period, bandwidth)`.
    pub const fn params(&self) -> (usize, f64) {
        (self.period, self.bandwidth)
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for BandpassFilter {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, price: f64) -> Option<f64> {
        if !price.is_finite() {
            return None;
        }
        let bp = match self.prev_price_2 {
            Some(p2) => {
                0.5 * (1.0 - self.alpha) * (price - p2) + self.beta * (1.0 + self.alpha) * self.bp1
                    - self.alpha * self.bp2
            }
            None => 0.0,
        };
        self.prev_price_2 = self.prev_price_1;
        self.prev_price_1 = Some(price);
        self.bp2 = self.bp1;
        self.bp1 = bp;
        self.last = Some(bp);
        Some(bp)
    }

    fn reset(&mut self) {
        self.prev_price_1 = None;
        self.prev_price_2 = None;
        self.bp1 = 0.0;
        self.bp2 = 0.0;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "BandpassFilter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_invalid_params() {
        assert!(matches!(
            BandpassFilter::new(0, 0.3),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            BandpassFilter::new(20, 0.0),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            BandpassFilter::new(20, 1.0),
            Err(Error::InvalidParameter { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let bp = BandpassFilter::new(20, 0.3).unwrap();
        assert_eq!(bp.params(), (20, 0.3));
        assert_eq!(bp.warmup_period(), 1);
        assert_eq!(bp.name(), "BandpassFilter");
        assert!(!bp.is_ready());
        assert_eq!(bp.value(), None);
    }

    #[test]
    fn first_bars_are_zero() {
        let mut bp = BandpassFilter::new(20, 0.3).unwrap();
        assert_eq!(bp.update(100.0), Some(0.0));
        assert_eq!(bp.update(101.0), Some(0.0));
        // From the third bar the recursion is active.
        assert!(bp.is_ready());
    }

    #[test]
    fn constant_input_stays_zero() {
        // A trend-free flat input has no cyclic content -> output stays 0.
        let mut bp = BandpassFilter::new(20, 0.3).unwrap();
        for v in bp.batch(&[50.0; 200]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn cyclic_input_oscillates_around_zero() {
        let mut bp = BandpassFilter::new(20, 0.3).unwrap();
        let xs: Vec<f64> = (0..400)
            .map(|i| 100.0 + (2.0 * PI * f64::from(i) / 20.0).sin() * 5.0)
            .collect();
        let out: Vec<f64> = bp.batch(&xs).into_iter().flatten().skip(100).collect();
        let mean = out.iter().sum::<f64>() / out.len() as f64;
        assert!(
            mean.abs() < 1.0,
            "bandpass output should be ~zero mean, got {mean}"
        );
        assert!(out.iter().any(|&v| v > 0.5));
        assert!(out.iter().any(|&v| v < -0.5));
    }

    #[test]
    fn ignores_non_finite() {
        let mut bp = BandpassFilter::new(20, 0.3).unwrap();
        bp.batch(&(0..40).map(f64::from).collect::<Vec<_>>());
        let before = bp.value();
        assert_eq!(bp.update(f64::NAN), None);
        // The rejected input must not have disturbed the state.
        assert_eq!(bp.value(), before);
    }

    #[test]
    fn reset_clears_state() {
        let mut bp = BandpassFilter::new(20, 0.3).unwrap();
        bp.batch(&(0..40).map(f64::from).collect::<Vec<_>>());
        assert!(bp.is_ready());
        bp.reset();
        assert!(!bp.is_ready());
        assert_eq!(bp.update(100.0), Some(0.0));
    }

    #[test]
    fn batch_equals_streaming() {
        let xs: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let batch = BandpassFilter::new(20, 0.3).unwrap().batch(&xs);
        let mut b = BandpassFilter::new(20, 0.3).unwrap();
        let streamed: Vec<_> = xs.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            BandpassFilter::new(crate::error::MAX_PERIOD + 1, 0.3),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
