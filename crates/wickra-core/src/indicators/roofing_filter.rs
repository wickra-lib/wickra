//! Ehlers Roofing Filter (high-pass followed by SuperSmoother).
#![allow(clippy::doc_markdown)]

use std::f64::consts::PI;

use crate::error::{Error, Result};
use crate::indicators::super_smoother::SuperSmoother;
use crate::traits::Indicator;

/// Ehlers' Roofing Filter — a bandpass formed by feeding a 1-pole high-pass
/// into a [`SuperSmoother`].
///
/// Defined in *Cycle Analytics for Traders* (Ehlers 2013, ch. 7) as the
/// canonical pre-filter for cycle-aware oscillators: the high-pass strips out
/// the trend (periods longer than `hp_period`), and the SuperSmoother removes
/// noise (periods shorter than `lp_period`). The result is essentially the
/// 10–48 bar cycle band by default.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, RoofingFilter};
///
/// let mut rf = RoofingFilter::new(10, 48).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     last = rf.update(100.0 + (f64::from(i) * 0.2).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct RoofingFilter {
    lp_period: usize,
    hp_period: usize,
    alpha: f64,
    prev_in_1: Option<f64>,
    prev_hp_1: f64,
    prev_hp_2: f64,
    smoother: SuperSmoother,
    last_value: Option<f64>,
}

impl RoofingFilter {
    /// Construct with `lp_period` (SuperSmoother critical period) and
    /// `hp_period` (high-pass cutoff). Defaults in Ehlers are `(10, 48)`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if either period is zero, and
    /// [`Error::InvalidPeriod`] if `lp_period >= hp_period`.
    pub fn new(lp_period: usize, hp_period: usize) -> Result<Self> {
        if lp_period == 0 || hp_period == 0 {
            return Err(Error::PeriodZero);
        }
        if lp_period >= hp_period {
            return Err(Error::InvalidPeriod {
                message: "lp_period must be strictly less than hp_period",
            });
        }
        // Single-pole high-pass alpha from Ehlers ch. 7.
        let arg = 2.0 * PI / hp_period as f64;
        let alpha = (arg.cos() + arg.sin() - 1.0) / arg.cos();
        Ok(Self {
            lp_period,
            hp_period,
            alpha,
            prev_in_1: None,
            prev_hp_1: 0.0,
            prev_hp_2: 0.0,
            smoother: SuperSmoother::new(lp_period)?,
            last_value: None,
        })
    }

    /// Configured `(lp_period, hp_period)`.
    pub const fn periods(&self) -> (usize, usize) {
        (self.lp_period, self.hp_period)
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }
}

impl Indicator for RoofingFilter {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        let hp = if let Some(x1) = self.prev_in_1 {
            let one_minus_half_alpha = 1.0 - self.alpha / 2.0;
            one_minus_half_alpha * (input - x1) + (1.0 - self.alpha) * self.prev_hp_1
        } else {
            0.0
        };
        self.prev_hp_2 = self.prev_hp_1;
        self.prev_hp_1 = hp;
        self.prev_in_1 = Some(input);
        let v = self.smoother.update(hp)?;
        self.last_value = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.prev_in_1 = None;
        self.prev_hp_1 = 0.0;
        self.prev_hp_2 = 0.0;
        self.smoother.reset();
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // The super-smoother emits on its first input, so this filter does
        // too. Two bars are what the high-pass stage needs to be meaningful,
        // which is a different question from when a value first appears.
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RoofingFilter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_invalid_periods() {
        assert!(matches!(RoofingFilter::new(0, 48), Err(Error::PeriodZero)));
        assert!(matches!(RoofingFilter::new(10, 0), Err(Error::PeriodZero)));
        assert!(matches!(
            RoofingFilter::new(48, 10),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            RoofingFilter::new(10, 10),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut rf = RoofingFilter::new(10, 48).unwrap();
        assert_eq!(rf.periods(), (10, 48));
        assert_eq!(rf.warmup_period(), 1);
        assert_eq!(rf.name(), "RoofingFilter");
        assert!(!rf.is_ready());
        rf.update(100.0);
        rf.update(101.0);
        assert!(rf.is_ready());
        assert!(rf.value().is_some());
    }

    #[test]
    fn constant_series_converges_to_zero() {
        // High-pass on a flat input is zero, and the smoother of zero is zero.
        let mut rf = RoofingFilter::new(10, 48).unwrap();
        let out = rf.batch(&[42.0_f64; 300]);
        for x in out.iter().skip(100).flatten() {
            assert_relative_eq!(*x, 0.0, epsilon = 1e-6);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.15).sin() * 5.0)
            .collect();
        let mut a = RoofingFilter::new(10, 48).unwrap();
        let mut b = RoofingFilter::new(10, 48).unwrap();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut rf = RoofingFilter::new(10, 48).unwrap();
        rf.batch(&(1..=100).map(f64::from).collect::<Vec<_>>());
        let before = rf.value();
        assert!(before.is_some());
        assert_eq!(rf.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut rf = RoofingFilter::new(10, 48).unwrap();
        rf.batch(&(1..=100).map(f64::from).collect::<Vec<_>>());
        assert!(rf.is_ready());
        rf.reset();
        assert!(!rf.is_ready());
    }

    use crate::traits::BatchNanExt;

    #[test]
    fn new_propagates_smoother_period_error() {
        // Both periods pass the zero and ordering checks, but the
        // SuperSmoother rejects an lp_period above MAX_PERIOD.
        let max = crate::error::MAX_PERIOD;
        assert!(matches!(
            RoofingFilter::new(max + 1, max + 2),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn first_value_lands_exactly_at_warmup() {
        let mut rf = RoofingFilter::new(10, 48).unwrap();
        let out = rf.batch(&[5.0, 6.0, 7.0]);
        assert_eq!(rf.warmup_period(), 1);
        // No previous input: the high-pass seeds at 0 and the smoother passes it.
        assert_eq!(out[0], Some(0.0));
    }

    #[test]
    fn reset_replays_identically() {
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.15).sin() * 5.0)
            .collect();
        let fresh = RoofingFilter::new(10, 48).unwrap().batch(&prices);
        let mut rf = RoofingFilter::new(10, 48).unwrap();
        let first = rf.batch(&prices);
        rf.reset();
        let second = rf.batch(&prices);
        assert_eq!(first, fresh);
        assert_eq!(second, fresh);
    }

    #[test]
    fn batch_nan_paths_match_streaming_bitwise() {
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.15).sin() * 5.0)
            .collect();
        let mut out = vec![0.0; prices.len()];
        RoofingFilter::new(10, 48)
            .unwrap()
            .batch_nan_into(&prices, &mut out);
        let nan = RoofingFilter::new(10, 48).unwrap().batch_nan(&prices);
        let fast = RoofingFilter::new(10, 48).unwrap().batch_fast(&prices);
        let mut stream = RoofingFilter::new(10, 48).unwrap();
        let expected: Vec<u64> = prices
            .iter()
            .map(|&p| stream.update(p).unwrap_or(f64::NAN).to_bits())
            .collect();
        assert!(out.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
        assert!(nan.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
        assert!(fast.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
    }

    #[test]
    fn step_response_hand_computed() {
        // hp_period = 6: arg = pi/3, cos = 0.5, sin = sqrt(3)/2, so
        // alpha = (0.5 + sqrt(3)/2 - 1) / 0.5 = sqrt(3) - 1 = 0.732_050_8.
        let mut rf = RoofingFilter::new(2, 6).unwrap();
        assert_relative_eq!(rf.alpha, 3.0_f64.sqrt() - 1.0, epsilon = 1e-12);
        let (c1, c2, _) = SuperSmoother::new(2).unwrap().coefficients();
        // Inputs 0, 10, 10:
        //   hp0 = 0 (no previous input)                   -> out0 = 0
        //   hp1 = (1 - alpha/2) * (10 - 0) = 6.339_746     -> out1 = hp1 (seed)
        //   hp2 = (1 - alpha/2) * 0 + (1 - alpha) * hp1 = 0.267_949 * 6.339_746 = 1.698_730
        //   out2 = c1 * (hp2 + hp1)/2 + c2 * out1 + c3 * out0, where out0 = 0
        let out = rf.batch(&[0.0, 10.0, 10.0]);
        let hp1 = 6.339_745_962_156;
        let hp2 = 1.698_729_810_778;
        assert_eq!(out[0], Some(0.0));
        assert_relative_eq!(out[1].unwrap(), hp1, epsilon = 1e-12);
        assert_relative_eq!(rf.prev_hp_1, hp2, epsilon = 1e-12);
        assert_relative_eq!(
            out[2].unwrap(),
            c1 * f64::midpoint(hp2, hp1) + c2 * hp1,
            epsilon = 1e-12
        );
    }
}
