//! Ehlers Fisher Transform.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Ehlers' Fisher Transform of price.
///
/// Normalises the most recent price to `[-1, +1]` via min/max over a `period`
/// window, smooths the normalised value with a 0.33 / 0.67 IIR step, and
/// applies the Fisher transform with Ehlers' own output smoothing,
/// `Fisher_t = 0.5 * ln((1+x)/(1-x)) + 0.5 * Fisher_{t-1}`. The result has a
/// near-Gaussian distribution, so extreme readings stand out cleanly. A
/// secondary signal is produced by lagging the Fisher value by one bar (the
/// classic trigger), making the indicator a two-line crossover system in
/// charts.
///
/// Only the primary Fisher value is exposed here as a scalar; the lagged
/// trigger is one update behind by construction.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, FisherTransform};
///
/// let mut ft = FisherTransform::new(10).unwrap();
/// let mut last = None;
/// for i in 0..30 {
///     last = ft.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct FisherTransform {
    period: usize,
    window: VecDeque<f64>,
    smoothed: f64,
    last_fisher: Option<f64>,
}

impl FisherTransform {
    /// Construct with the rolling extrema window length.
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
            window: VecDeque::with_capacity(period),
            smoothed: 0.0,
            last_fisher: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current Fisher value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_fisher
    }
}

impl Indicator for FisherTransform {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(input);
        if self.window.len() < self.period {
            return None;
        }
        let max = self
            .window
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let min = self.window.iter().copied().fold(f64::INFINITY, f64::min);
        let range = max - min;
        // Normalise to roughly [-1, +1]; centred midpoint when range == 0.
        let raw = if range > 0.0 {
            ((input - min) / range).mul_add(2.0, -1.0)
        } else {
            0.0
        };
        // Ehlers IIR: 0.33 * raw + 0.67 * prev_smoothed, then clamp. The clamped
        // value is what recurs, as in Ehlers' code (Value1 is overwritten).
        let clamped = 0.33f64
            .mul_add(raw, 0.67 * self.smoothed)
            .clamp(-0.999, 0.999);
        self.smoothed = clamped;
        // Fisher transform plus Ehlers' half-weight carry of the previous value.
        let prev = self.last_fisher.unwrap_or(0.0);
        let fisher = 0.5f64.mul_add(((1.0 + clamped) / (1.0 - clamped)).ln(), 0.5 * prev);
        self.last_fisher = Some(fisher);
        Some(fisher)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.smoothed = 0.0;
        self.last_fisher = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_fisher.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "FisherTransform"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(FisherTransform::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut ft = FisherTransform::new(10).unwrap();
        assert_eq!(ft.period(), 10);
        assert_eq!(ft.warmup_period(), 10);
        assert_eq!(ft.name(), "FisherTransform");
        assert!(ft.value().is_none());
        for i in 1..=10 {
            ft.update(f64::from(i));
        }
        assert!(ft.value().is_some());
        assert!(ft.is_ready());
    }

    #[test]
    fn warmup_returns_none_until_seed() {
        let mut ft = FisherTransform::new(5).unwrap();
        for i in 1..=4 {
            assert_eq!(ft.update(f64::from(i)), None);
        }
        assert!(ft.update(5.0).is_some());
    }

    #[test]
    fn constant_series_zero_range_yields_zero() {
        let mut ft = FisherTransform::new(5).unwrap();
        let out = ft.batch(&[42.0_f64; 30]);
        for x in out.iter().skip(5).flatten() {
            assert!(x.abs() < 1e-6, "expected near-zero, got {x}");
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 100.0 + (f64::from(i) * 0.2).sin() * 8.0)
            .collect();
        let mut a = FisherTransform::new(10).unwrap();
        let mut b = FisherTransform::new(10).unwrap();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut ft = FisherTransform::new(5).unwrap();
        ft.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let before = ft.value();
        assert!(before.is_some());
        assert_eq!(ft.update(f64::NAN), None);
        assert_eq!(ft.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut ft = FisherTransform::new(5).unwrap();
        ft.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        assert!(ft.is_ready());
        ft.reset();
        assert!(!ft.is_ready());
        assert_eq!(ft.update(1.0), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            FisherTransform::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_minus_one() {
        let prices: Vec<f64> = (0..40)
            .map(|i| 50.0 + (f64::from(i) * 0.4).sin() * 3.0)
            .collect();
        for period in [1usize, 4, 9] {
            let mut ft = FisherTransform::new(period).unwrap();
            let out = ft.batch(&prices);
            let warm = ft.warmup_period();
            assert!(out[..warm - 1].iter().all(Option::is_none));
            assert!(out[warm - 1].is_some());
        }
    }

    #[test]
    fn hand_computed_recursive_values() {
        let mut ft = FisherTransform::new(3).unwrap();
        assert_eq!(ft.update(1.0), None);
        assert_eq!(ft.update(2.0), None);
        // Window [1, 2, 3], input 3 is the max -> raw = 2 * 1 - 1 = 1.
        // x1 = 0.33 * 1 + 0.67 * 0 = 0.33
        // F1 = 0.5 * ln(1.33 / 0.67) + 0.5 * 0 = 0.342_828_254...
        let f1 = ft.update(3.0).unwrap();
        assert_relative_eq!(f1, 0.5 * (1.33f64 / 0.67).ln(), epsilon = 1e-12);
        assert_relative_eq!(f1, 0.342_828_254_415_393_8, epsilon = 1e-12);
        // Window [2, 3, 3], input 3 is the max -> raw = 1.
        // x2 = 0.33 + 0.67 * 0.33 = 0.5511
        // F2 = 0.5 * ln(1.5511 / 0.4489) + 0.5 * F1 = 0.791_373_872...
        let f2 = ft.update(3.0).unwrap();
        assert_relative_eq!(
            f2,
            0.5 * (1.5511f64 / 0.4489).ln() + 0.5 * f1,
            epsilon = 1e-12
        );
        assert_relative_eq!(f2, 0.791_373_872_129_106_3, epsilon = 1e-12);
        // Window [3, 3, 1], input 1 is the min -> raw = -1.
        // x3 = -0.33 + 0.67 * 0.5511 = 0.039_237
        // F3 = 0.5 * ln(1.039_237 / 0.960_763) + 0.5 * F2 = 0.434_944_090...
        let f3 = ft.update(1.0).unwrap();
        assert_relative_eq!(f3, 0.434_944_090_356_889_4, epsilon = 1e-12);
        assert_eq!(ft.value(), Some(f3));
    }

    #[test]
    fn clamped_value_is_what_recurs() {
        // A strictly rising series keeps raw = 1, so the smoothed value climbs
        // toward 1 and is clamped at 0.999; the clamped value is stored.
        let mut ft = FisherTransform::new(3).unwrap();
        for i in 0..60 {
            ft.update(f64::from(i));
        }
        assert_eq!(ft.smoothed.to_bits(), 0.999f64.to_bits());
        let prev = ft.value().unwrap();
        // Steady state: F = 0.5 * ln(1.999 / 0.001) + 0.5 * F -> F -> ln(1999) = 7.600_402.
        assert_relative_eq!(prev, 1999f64.ln(), epsilon = 1e-9);
        // A drop to the window minimum: raw = -1,
        // x = -0.33 + 0.67 * 0.999 = 0.339_33 (recurring from the clamped 0.999).
        let next = ft.update(-100.0).unwrap();
        assert_relative_eq!(ft.smoothed, 0.339_33, epsilon = 1e-12);
        assert_relative_eq!(
            next,
            0.5 * (1.339_33f64 / 0.660_67).ln() + 0.5 * prev,
            epsilon = 1e-12
        );
    }

    #[test]
    fn falling_series_clamps_at_lower_bound() {
        let mut ft = FisherTransform::new(3).unwrap();
        for i in 0..60 {
            ft.update(-f64::from(i));
        }
        assert_eq!(ft.smoothed.to_bits(), (-0.999f64).to_bits());
        assert_relative_eq!(ft.value().unwrap(), -(1999f64.ln()), epsilon = 1e-9);
    }

    #[test]
    fn flat_window_decays_previous_reading() {
        // range == 0 -> raw = 0: x_t = 0.67 * x_{t-1}, and F carries half of its
        // previous value, so a flat stretch decays toward zero.
        let mut ft = FisherTransform::new(2).unwrap();
        ft.update(1.0);
        let f1 = ft.update(2.0).unwrap();
        // Window [2, 2]: raw = 0 -> x = 0.67 * 0.33 = 0.2211.
        let f2 = ft.update(2.0).unwrap();
        assert_relative_eq!(
            f2,
            0.5 * (1.2211f64 / 0.7789).ln() + 0.5 * f1,
            epsilon = 1e-12
        );
        // F2 = 0.5 * 0.449_6 + 0.5 * 0.342_8 = 0.396; then a long flat stretch decays.
        assert!(f2 > f1);
        let tail = ft.batch(&[2.0; 40]);
        assert!(tail.iter().flatten().all(|v| *v > 0.0));
        assert!(ft.value().unwrap() < 1e-6);
    }

    #[test]
    fn reset_replays_identically_and_batch_nan_into_matches() {
        let prices: Vec<f64> = (0..80)
            .map(|i| 100.0 + (f64::from(i) * 0.27).sin() * 6.0 + f64::from(i % 3))
            .collect();
        let mut ft = FisherTransform::new(8).unwrap();
        let first = ft.batch(&prices);
        ft.reset();
        let second = ft.batch(&prices);
        assert_eq!(first, second);
        assert_eq!(second, FisherTransform::new(8).unwrap().batch(&prices));
        let mut out = vec![0.0; prices.len()];
        FisherTransform::new(8)
            .unwrap()
            .batch_nan_into(&prices, &mut out);
        assert!(out
            .iter()
            .zip(&first)
            .all(|(a, b)| a.to_bits() == b.unwrap_or(f64::NAN).to_bits()));
    }
}
