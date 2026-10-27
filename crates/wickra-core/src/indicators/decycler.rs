//! Ehlers Decycler (single-pole high-pass complement).

use std::f64::consts::PI;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Ehlers' Decycler: price minus the dominant cycle component.
///
/// Implemented as `decycler = input - HP(input)`, where `HP` is a 2-pole
/// high-pass filter with critical period `period`. Subtracting the high-pass
/// from the raw price leaves the slow component — equivalent to a smoothed
/// trend line with no group delay at low frequencies. From *Cycle Analytics
/// for Traders* (Ehlers 2013, ch. 4).
///
/// The high-pass uses the standard 2-pole formulation:
///
/// ```text
/// alpha = (cos(.707*2*pi/period) + sin(.707*2*pi/period) - 1) / cos(.707*2*pi/period)
/// HP[t] = (1 - alpha/2)^2 * (x[t] - 2*x[t-1] + x[t-2])
///       + 2*(1 - alpha) * HP[t-1]
///       - (1 - alpha)^2 * HP[t-2]
/// ```
///
/// The first two outputs simply equal the input (warmup buffering), which is
/// the conventional Ehlers initialisation and keeps downstream consumers
/// reactive while the recursion fills.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Decycler};
///
/// let mut dc = Decycler::new(20).unwrap();
/// let mut last = None;
/// for i in 0..50 {
///     last = dc.update(100.0 + f64::from(i) * 0.5);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Decycler {
    period: usize,
    alpha: f64,
    prev_in_1: Option<f64>,
    prev_in_2: Option<f64>,
    prev_hp_1: f64,
    prev_hp_2: f64,
    last_value: Option<f64>,
}

impl Decycler {
    /// Construct a Decycler with the given critical period for the high-pass filter.
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
        let arg = 0.707 * 2.0 * PI / period as f64;
        let c = arg.cos();
        let alpha = (c + arg.sin() - 1.0) / c;
        Ok(Self {
            period,
            alpha,
            prev_in_1: None,
            prev_in_2: None,
            prev_hp_1: 0.0,
            prev_hp_2: 0.0,
            last_value: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// High-pass `alpha` coefficient derived from the period.
    pub const fn alpha(&self) -> f64 {
        self.alpha
    }

    /// Current decycler value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }

    /// Compute and store the high-pass output for the latest input.
    fn step_hp(&mut self, input: f64) -> f64 {
        let (Some(x1), Some(x2)) = (self.prev_in_1, self.prev_in_2) else {
            self.prev_hp_2 = self.prev_hp_1;
            self.prev_hp_1 = 0.0;
            return 0.0;
        };
        let one_minus_half_alpha = 1.0 - self.alpha / 2.0;
        let one_minus_alpha = 1.0 - self.alpha;
        let drv = one_minus_half_alpha * one_minus_half_alpha;
        let term1 = drv * (input - 2.0 * x1 + x2);
        let term2 = 2.0 * one_minus_alpha * self.prev_hp_1;
        let term3 = one_minus_alpha * one_minus_alpha * self.prev_hp_2;
        let hp = term1 + term2 - term3;
        self.prev_hp_2 = self.prev_hp_1;
        self.prev_hp_1 = hp;
        hp
    }
}

impl Indicator for Decycler {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        let hp = self.step_hp(input);
        let v = input - hp;
        self.prev_in_2 = self.prev_in_1;
        self.prev_in_1 = Some(input);
        self.last_value = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.prev_in_1 = None;
        self.prev_in_2 = None;
        self.prev_hp_1 = 0.0;
        self.prev_hp_2 = 0.0;
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "Decycler"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(Decycler::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut dc = Decycler::new(20).unwrap();
        assert_eq!(dc.period(), 20);
        assert_eq!(dc.warmup_period(), 1);
        assert_eq!(dc.name(), "Decycler");
        assert!(dc.alpha() > 0.0 && dc.alpha() < 1.0);
        assert!(!dc.is_ready());
        dc.update(100.0);
        assert!(dc.is_ready());
        assert!(dc.value().is_some());
    }

    #[test]
    fn constant_series_passes_through() {
        // For a flat input, the high-pass output is zero, so the decycler
        // equals the input.
        let mut dc = Decycler::new(20).unwrap();
        let out = dc.batch(&[42.0_f64; 80]);
        for x in out.iter().flatten() {
            assert_relative_eq!(*x, 42.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..100)
            .map(|i| 100.0 + (f64::from(i) * 0.15).sin() * 5.0)
            .collect();
        let mut a = Decycler::new(20).unwrap();
        let mut b = Decycler::new(20).unwrap();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut dc = Decycler::new(20).unwrap();
        dc.batch(&(1..=30).map(f64::from).collect::<Vec<_>>());
        let before = dc.value();
        assert!(before.is_some());
        assert_eq!(dc.update(f64::NAN), None);
        assert_eq!(dc.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut dc = Decycler::new(20).unwrap();
        dc.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        assert!(dc.is_ready());
        dc.reset();
        assert!(!dc.is_ready());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Decycler::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
