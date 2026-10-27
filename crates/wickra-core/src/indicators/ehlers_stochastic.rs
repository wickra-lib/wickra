//! Ehlers Stochastic — Stochastic computed on a Roofing-Filter pre-filtered input.
#![allow(clippy::doc_markdown)]

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::roofing_filter::RoofingFilter;
use crate::traits::Indicator;

/// Ehlers' Adaptive Stochastic.
///
/// Implements the construction described in *Cycle Analytics for Traders*
/// (Ehlers 2013, ch. 7): the raw price is first passed through a
/// [`RoofingFilter`] (high-pass + SuperSmoother bandpass) to isolate the
/// tradable cycle band, then the classic Stochastic %K formula is applied
/// to the filtered output over `period` bars and finally re-smoothed by a
/// 2-bar SuperSmoother. The result is a ±1-normalised oscillator that
/// reacts to cycles without trending bias from low-frequency drift.
///
/// The output uses Ehlers' `2 * (X - MinX) / (MaxX - MinX) - 1` convention,
/// so the range is `[-1, +1]` rather than the conventional `[0, 100]`.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, EhlersStochastic};
///
/// let mut es = EhlersStochastic::new(20).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     last = es.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct EhlersStochastic {
    period: usize,
    roofing: RoofingFilter,
    filtered_buf: VecDeque<f64>,
    // Tiny 2-tap IIR (Ehlers uses a simple SMA(2) for the final smoothing).
    prev_stoch: f64,
    has_prev: bool,
    last_value: Option<f64>,
}

impl EhlersStochastic {
    /// Construct with the rolling window length used by the inner stochastic.
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
            // Defaults match Ehlers' (10, 48) roofing filter cutoffs.
            roofing: RoofingFilter::new(10, 48)?,
            filtered_buf: VecDeque::with_capacity(period),
            prev_stoch: 0.0,
            has_prev: false,
            last_value: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }
}

impl Indicator for EhlersStochastic {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        let filtered = self.roofing.update(input)?;
        if self.filtered_buf.len() == self.period {
            self.filtered_buf.pop_front();
        }
        self.filtered_buf.push_back(filtered);
        if self.filtered_buf.len() < self.period {
            return None;
        }
        let max = self
            .filtered_buf
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let min = self
            .filtered_buf
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let range = max - min;
        let raw = if range > 0.0 {
            ((filtered - min) / range).mul_add(2.0, -1.0)
        } else {
            0.0
        };
        // 2-bar SMA smoothing.
        let smoothed = if self.has_prev {
            f64::midpoint(raw, self.prev_stoch)
        } else {
            raw
        };
        self.prev_stoch = raw;
        self.has_prev = true;
        self.last_value = Some(smoothed);
        Some(smoothed)
    }

    fn reset(&mut self) {
        self.roofing.reset();
        self.filtered_buf.clear();
        self.prev_stoch = 0.0;
        self.has_prev = false;
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // The roofing filter feeds the stochastic, so its last warmup bar is
        // the stochastic's first input; the two overlap by one.
        self.period + self.roofing.warmup_period() - 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "EhlersStochastic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(EhlersStochastic::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut es = EhlersStochastic::new(20).unwrap();
        assert_eq!(es.period(), 20);
        assert_eq!(es.warmup_period(), 20);
        assert_eq!(es.name(), "EhlersStochastic");
        assert!(!es.is_ready());
        let prices: Vec<f64> = (0..150)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 5.0)
            .collect();
        es.batch(&prices);
        assert!(es.is_ready());
        assert!(es.value().is_some());
    }

    #[test]
    fn output_bounded_in_unit_interval() {
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let mut es = EhlersStochastic::new(20).unwrap();
        for v in es.batch(&prices).into_iter().flatten() {
            assert!((-1.0..=1.0).contains(&v), "value out of band: {v}");
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..150)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let mut a = EhlersStochastic::new(20).unwrap();
        let mut b = EhlersStochastic::new(20).unwrap();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut es = EhlersStochastic::new(20).unwrap();
        let prices: Vec<f64> = (0..150)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        es.batch(&prices);
        let before = es.value();
        assert!(before.is_some());
        assert_eq!(es.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut es = EhlersStochastic::new(20).unwrap();
        let prices: Vec<f64> = (0..150)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        es.batch(&prices);
        assert!(es.is_ready());
        es.reset();
        assert!(!es.is_ready());
    }

    #[test]
    fn flat_window_emits_zero() {
        // A constant series has zero high-pass output, so `max == min` and the
        // `range > 0.0` guard takes the `0.0` fallback rather than dividing.
        let mut es = EhlersStochastic::new(20).unwrap();
        for v in es.batch(&[100.0_f64; 150]).into_iter().flatten() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            EhlersStochastic::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
