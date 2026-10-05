//! Ehlers SuperSmoother filter.
#![allow(clippy::doc_markdown)]

use std::f64::consts::PI;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Ehlers' 2-pole Butterworth-style "SuperSmoother" lowpass filter.
///
/// From John Ehlers' *Cycle Analytics for Traders* (2013, ch. 3). For a given
/// critical period `period`, the filter coefficients are:
///
/// ```text
/// a1 = exp(-sqrt(2) * pi / period)
/// b1 = 2 * a1 * cos(sqrt(2) * pi / period)
/// c2 = b1
/// c3 = -a1 * a1
/// c1 = 1 - c2 - c3
/// y[t] = c1 * (x[t] + x[t-1]) / 2 + c2 * y[t-1] + c3 * y[t-2]
/// ```
///
/// The implementation needs two prior inputs and two prior outputs to begin
/// running; until then it returns the input itself (a common Ehlers initial
/// condition), which lets downstream filters warm up without long delays.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, SuperSmoother};
///
/// let mut ss = SuperSmoother::new(10).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = ss.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct SuperSmoother {
    period: usize,
    c1: f64,
    c2: f64,
    c3: f64,
    prev_input: Option<f64>,
    prev_output_1: Option<f64>,
    prev_output_2: Option<f64>,
    count: usize,
}

impl SuperSmoother {
    /// Construct a new SuperSmoother with the given critical period.
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
        Ok(Self::with_critical_period(period, period as f64))
    }

    /// Build a SuperSmoother whose coefficients use a fractional `critical`
    /// period, reporting `period` from [`period`](Self::period). Ehlers' Reflex
    /// and Trendflex smooth with half their lookback (`0.5 · Length`), which is
    /// fractional for an odd length. The caller validates `period`.
    pub(crate) fn with_critical_period(period: usize, critical: f64) -> Self {
        let arg = std::f64::consts::SQRT_2 * PI / critical;
        let a1 = (-arg).exp();
        let b1 = 2.0 * a1 * arg.cos();
        let c2 = b1;
        let c3 = -a1 * a1;
        let c1 = 1.0 - c2 - c3;
        Self {
            period,
            c1,
            c2,
            c3,
            prev_input: None,
            prev_output_1: None,
            prev_output_2: None,
            count: 0,
        }
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Filter coefficients `(c1, c2, c3)`.
    pub const fn coefficients(&self) -> (f64, f64, f64) {
        (self.c1, self.c2, self.c3)
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.prev_output_1
    }
}

impl Indicator for SuperSmoother {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        self.count += 1;
        let output = match (self.prev_input, self.prev_output_1, self.prev_output_2) {
            (Some(p_in), Some(y1), Some(y2)) => {
                let avg = f64::midpoint(input, p_in);
                self.c1 * avg + self.c2 * y1 + self.c3 * y2
            }
            _ => input,
        };
        self.prev_output_2 = self.prev_output_1;
        self.prev_output_1 = Some(output);
        self.prev_input = Some(input);
        Some(output)
    }

    fn reset(&mut self) {
        self.prev_input = None;
        self.prev_output_1 = None;
        self.prev_output_2 = None;
        self.count = 0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.prev_output_1.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "SuperSmoother"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(SuperSmoother::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut ss = SuperSmoother::new(10).unwrap();
        assert_eq!(ss.period(), 10);
        assert_eq!(ss.name(), "SuperSmoother");
        assert_eq!(ss.warmup_period(), 1);
        let (c1, c2, c3) = ss.coefficients();
        // Coefficients sum to 1 by construction (steady-state gain == 1).
        assert_relative_eq!(c1 + c2 + c3, 1.0, epsilon = 1e-12);
        assert!(ss.value().is_none());
        ss.update(42.0);
        assert!(ss.value().is_some());
        assert!(ss.is_ready());
    }

    #[test]
    fn first_output_equals_input_then_filters() {
        let mut ss = SuperSmoother::new(10).unwrap();
        // Initial condition: first two outputs equal their inputs.
        assert_eq!(ss.update(100.0), Some(100.0));
        assert_eq!(ss.update(101.0), Some(101.0));
        let third = ss.update(102.0).unwrap();
        // From step 3 onward, the recursive filter activates and the result
        // is no longer the raw input.
        assert!((third - 102.0).abs() < 5.0);
    }

    #[test]
    fn constant_series_converges_to_constant() {
        // Steady-state gain is 1 (c1 + c2 + c3 = 1), so a flat input yields a
        // flat output after warmup.
        let mut ss = SuperSmoother::new(20).unwrap();
        let out = ss.batch(&[50.0_f64; 200]);
        for x in out.iter().skip(50).flatten() {
            assert_relative_eq!(*x, 50.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.2).sin() * 5.0)
            .collect();
        let mut a = SuperSmoother::new(15).unwrap();
        let mut b = SuperSmoother::new(15).unwrap();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut ss = SuperSmoother::new(10).unwrap();
        ss.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        let before = ss.value();
        assert!(before.is_some());
        assert_eq!(ss.update(f64::NAN), None);
        assert_eq!(ss.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut ss = SuperSmoother::new(10).unwrap();
        ss.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        assert!(ss.is_ready());
        ss.reset();
        assert!(!ss.is_ready());
        assert_eq!(ss.update(50.0), Some(50.0));
    }

    use crate::traits::BatchNanExt;

    #[test]
    fn new_rejects_period_above_max() {
        assert!(matches!(
            SuperSmoother::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn first_value_lands_exactly_at_warmup() {
        let mut ss = SuperSmoother::new(10).unwrap();
        let out = ss.batch(&[5.0, 6.0, 7.0]);
        assert_eq!(ss.warmup_period(), 1);
        assert_eq!(out[0], Some(5.0));
    }

    #[test]
    fn reset_replays_identically() {
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.2).sin() * 5.0)
            .collect();
        let fresh = SuperSmoother::new(12).unwrap().batch(&prices);
        let mut ss = SuperSmoother::new(12).unwrap();
        let first = ss.batch(&prices);
        ss.reset();
        let second = ss.batch(&prices);
        assert_eq!(first, fresh);
        assert_eq!(second, fresh);
    }

    #[test]
    fn batch_nan_paths_match_streaming_bitwise() {
        let prices: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.2).sin() * 5.0)
            .collect();
        let mut out = vec![0.0; prices.len()];
        SuperSmoother::new(12)
            .unwrap()
            .batch_nan_into(&prices, &mut out);
        let nan = SuperSmoother::new(12).unwrap().batch_nan(&prices);
        let fast = SuperSmoother::new(12).unwrap().batch_fast(&prices);
        let mut stream = SuperSmoother::new(12).unwrap();
        let expected: Vec<u64> = prices
            .iter()
            .map(|&p| stream.update(p).unwrap_or(f64::NAN).to_bits())
            .collect();
        assert!(out.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
        assert!(nan.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
        assert!(fast.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
    }

    #[test]
    fn with_critical_period_hand_computed() {
        // critical = 2*sqrt(2) makes arg = sqrt(2)*pi / (2*sqrt(2)) = pi/2, so
        // cos(arg) ~ 0 and b1 = c2 ~ 0, a1 = exp(-pi/2) = 0.207_879_576,
        // c3 = -a1^2 = -0.043_213_918, c1 = 1 - c2 - c3 = 1.043_213_918.
        let ss = SuperSmoother::with_critical_period(7, 2.0 * std::f64::consts::SQRT_2);
        assert_eq!(ss.period(), 7);
        let (c1, c2, c3) = ss.coefficients();
        assert_relative_eq!(c2, 0.0, epsilon = 1e-12);
        assert_relative_eq!(c3, -0.043_213_918_264, epsilon = 1e-12);
        assert_relative_eq!(c1, 1.043_213_918_264, epsilon = 1e-12);
        // `new(period)` is `with_critical_period(period, period)`.
        let a = SuperSmoother::new(9).unwrap().coefficients();
        let b = SuperSmoother::with_critical_period(9, 9.0).coefficients();
        assert_eq!(
            (a.0.to_bits(), a.1.to_bits(), a.2.to_bits()),
            (b.0.to_bits(), b.1.to_bits(), b.2.to_bits())
        );
        // A fractional critical period yields different coefficients.
        let half = SuperSmoother::with_critical_period(9, 4.5).coefficients();
        assert!((half.0 - a.0).abs() > 1e-3);
    }

    #[test]
    fn third_output_is_the_recursion_hand_computed() {
        // Inputs 100, 101, 102: y0 = 100, y1 = 101 (pass-through seed), then
        // y2 = c1 * (102 + 101)/2 + c2 * 101 + c3 * 100.
        let mut ss = SuperSmoother::with_critical_period(7, 2.0 * std::f64::consts::SQRT_2);
        let (c1, c2, c3) = ss.coefficients();
        let out = ss.batch(&[100.0, 101.0, 102.0]);
        assert_eq!(out[0], Some(100.0));
        assert_eq!(out[1], Some(101.0));
        let expected = c1 * 101.5 + c2 * 101.0 + c3 * 100.0;
        assert_eq!(out[2].unwrap().to_bits(), expected.to_bits());
        // Numerically: 1.043_213_918 * 101.5 - 0.043_213_918 * 100 = 101.564_820_88.
        assert_relative_eq!(out[2].unwrap(), 101.564_820_877, epsilon = 1e-6);
    }
}
