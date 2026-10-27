//! Ehlers Universal Oscillator — whitened, SuperSmoothed, AGC-normalised cycle.
#![allow(clippy::doc_markdown)]

use crate::error::{Error, Result};
use crate::indicators::super_smoother::SuperSmoother;
use crate::traits::Indicator;

/// Ehlers' **Universal Oscillator** — a cycle oscillator that whitens the price
/// series, SuperSmooths it, then normalises with an automatic gain control (AGC)
/// to swing in `[−1, +1]`.
///
/// From John Ehlers' *Cycle Analytics for Traders* (2013):
///
/// ```text
/// WhiteNoise = (price_t − price_{t−2}) / 2          (flat-spectrum prewhitening)
/// Filt       = SuperSmoother(WhiteNoise, period)
/// Peak       = max(|Filt|, 0.991 · Peak_{t−1})      (decaying peak / AGC)
/// Universal  = Filt / Peak                          (0 if Peak == 0)
/// ```
///
/// "Whitening" the input (a two-bar difference) flattens its power spectrum so the
/// SuperSmoother responds equally to all cycles rather than being dominated by the
/// trend. The automatic gain control divides by a slowly-decaying running peak, so
/// the output is amplitude-normalised to `[−1, +1]` and behaves consistently
/// across instruments and volatility regimes — hence "universal". Read it like any
/// bounded oscillator: turns near the rails flag cycle extremes, zero-crossings
/// flag cycle direction changes.
///
/// The first value lands once a two-bar difference exists (`warmup_period == 3`).
/// Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, UniversalOscillator};
///
/// let mut indicator = UniversalOscillator::new(20).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct UniversalOscillator {
    period: usize,
    smoother: SuperSmoother,
    prev_price_1: Option<f64>,
    prev_price_2: Option<f64>,
    peak: f64,
    last: Option<f64>,
}

impl UniversalOscillator {
    /// Construct a Universal Oscillator with the given SuperSmoother `period`.
    ///
    /// # Errors
    ///
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
            smoother: SuperSmoother::new(period)?,
            prev_price_1: None,
            prev_price_2: None,
            peak: 0.0,
            last: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for UniversalOscillator {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, price: f64) -> Option<f64> {
        if !price.is_finite() {
            return None;
        }
        let Some(p2) = self.prev_price_2 else {
            self.prev_price_2 = self.prev_price_1;
            self.prev_price_1 = Some(price);
            return None;
        };
        let white_noise = (price - p2) / 2.0;
        if !white_noise.is_finite() {
            // `price - p2` can overflow to +/-inf even when both are finite;
            // skip the bar rather than feeding a non-finite value downstream.
            self.prev_price_2 = self.prev_price_1;
            self.prev_price_1 = Some(price);
            return self.last;
        }
        let filt = self
            .smoother
            .update(white_noise)
            .expect("supersmoother emits");
        self.peak = filt.abs().max(0.991 * self.peak);
        let universal = if self.peak > 0.0 {
            (filt / self.peak).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        self.prev_price_2 = self.prev_price_1;
        self.prev_price_1 = Some(price);
        self.last = Some(universal);
        Some(universal)
    }

    fn reset(&mut self) {
        self.smoother.reset();
        self.prev_price_1 = None;
        self.prev_price_2 = None;
        self.peak = 0.0;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        3
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "UniversalOscillator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            UniversalOscillator::new(0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let u = UniversalOscillator::new(20).unwrap();
        assert_eq!(u.period(), 20);
        assert_eq!(u.warmup_period(), 3);
        assert_eq!(u.name(), "UniversalOscillator");
        assert!(!u.is_ready());
        assert_eq!(u.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut u = UniversalOscillator::new(20).unwrap();
        let out = u.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(out[0].is_none());
        assert!(out[1].is_none());
        assert!(out[2].is_some());
    }

    #[test]
    fn constant_input_is_zero() {
        // A flat input whitens to zero -> output 0.
        let mut u = UniversalOscillator::new(20).unwrap();
        for v in u.batch(&[50.0; 200]).into_iter().flatten() {
            assert!(v.abs() < 1e-9);
        }
    }

    #[test]
    fn output_in_range() {
        let mut u = UniversalOscillator::new(20).unwrap();
        let xs: Vec<f64> = (0..400)
            .map(|i| 100.0 + (std::f64::consts::TAU * f64::from(i) / 20.0).sin() * 5.0)
            .collect();
        for v in u.batch(&xs).into_iter().flatten() {
            assert!((-1.0..=1.0).contains(&v), "out of range: {v}");
        }
    }

    #[test]
    fn cyclic_input_swings_both_signs() {
        let mut u = UniversalOscillator::new(20).unwrap();
        let xs: Vec<f64> = (0..400)
            .map(|i| 100.0 + (std::f64::consts::TAU * f64::from(i) / 20.0).sin() * 5.0)
            .collect();
        let out: Vec<f64> = u.batch(&xs).into_iter().flatten().skip(100).collect();
        assert!(out.iter().any(|&v| v > 0.5));
        assert!(out.iter().any(|&v| v < -0.5));
    }

    #[test]
    fn ignores_non_finite() {
        let mut u = UniversalOscillator::new(20).unwrap();
        u.batch(
            &(0..40)
                .map(|i| 100.0 + (f64::from(i) * 0.3).sin())
                .collect::<Vec<_>>(),
        );
        let before = u.value();
        assert_eq!(u.update(f64::NAN), None);
        // The rejected input must not have disturbed the state.
        assert_eq!(u.value(), before);
    }

    #[test]
    fn reset_clears_state() {
        let mut u = UniversalOscillator::new(20).unwrap();
        u.batch(
            &(0..40)
                .map(|i| 100.0 + (f64::from(i) * 0.3).sin())
                .collect::<Vec<_>>(),
        );
        assert!(u.is_ready());
        u.reset();
        assert!(!u.is_ready());
        assert_eq!(u.value(), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let xs: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let batch = UniversalOscillator::new(20).unwrap().batch(&xs);
        let mut b = UniversalOscillator::new(20).unwrap();
        let streamed: Vec<_> = xs.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_finite_white_noise_is_skipped() {
        // `price - p2` can overflow to infinity even when both prices are
        // finite; the non-finite white-noise term must be skipped, not fed to
        // the smoother (which would otherwise yield `None` on the first bar).
        let mut u = UniversalOscillator::new(20).unwrap();
        assert_eq!(u.update(-1e308), None);
        assert_eq!(u.update(0.0), None);
        // (1e308 - (-1e308)) overflows to +inf -> white_noise non-finite.
        assert_eq!(u.update(1e308), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            UniversalOscillator::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
