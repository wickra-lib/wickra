//! Ehlers two-pole Highpass Filter — removes the trend, keeps the cycles.
#![allow(clippy::doc_markdown)]

use std::f64::consts::PI;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Ehlers' two-pole Highpass Filter — strips the low-frequency trend from a price
/// series, leaving the higher-frequency cyclic and noise content.
///
/// From John Ehlers' *Cycle Analytics for Traders* (2013):
///
/// ```text
/// a = 0.707 · 2π / period
/// alpha1 = (cos(a) + sin(a) − 1) / cos(a)
/// HP_t = (1 − alpha1/2)² · (price_t − 2·price_{t−1} + price_{t−2})
///        + 2·(1 − alpha1)·HP_{t−1} − (1 − alpha1)²·HP_{t−2}
/// ```
///
/// A highpass filter is the complement of a smoother: where a lowpass keeps the
/// trend, the highpass keeps everything *faster* than the cutoff `period`. The
/// two-pole design gives a steep roll-off so frequencies below the cutoff are
/// firmly removed, detrending the series into a zero-mean wave. This differs from
/// the [`Decycler`](crate::Decycler), which is `price − highpass` (the *trend* that
/// remains); the highpass is the cyclic part that the decycler discards.
///
/// The recursion needs two prior prices and two prior outputs; until then it emits
/// `0`, so `warmup_period` is `1`. Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, HighpassFilter};
///
/// let mut indicator = HighpassFilter::new(48).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     last = indicator.update(100.0 + f64::from(i) + (f64::from(i) * 0.5).sin() * 3.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct HighpassFilter {
    period: usize,
    alpha1: f64,
    prev_price_1: Option<f64>,
    prev_price_2: Option<f64>,
    hp1: f64,
    hp2: f64,
    last: Option<f64>,
}

impl HighpassFilter {
    /// Construct a two-pole highpass filter with the given cutoff `period`.
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
        let a = 0.707 * 2.0 * PI / period as f64;
        let alpha1 = (a.cos() + a.sin() - 1.0) / a.cos();
        Ok(Self {
            period,
            alpha1,
            prev_price_1: None,
            prev_price_2: None,
            hp1: 0.0,
            hp2: 0.0,
            last: None,
        })
    }

    /// Configured cutoff period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for HighpassFilter {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, price: f64) -> Option<f64> {
        if !price.is_finite() {
            return None;
        }
        let hp = match (self.prev_price_1, self.prev_price_2) {
            (Some(p1), Some(p2)) => {
                let one_minus = 1.0 - self.alpha1;
                let half = 1.0 - self.alpha1 / 2.0;
                half * half * (price - 2.0 * p1 + p2) + 2.0 * one_minus * self.hp1
                    - one_minus * one_minus * self.hp2
            }
            _ => 0.0,
        };
        self.prev_price_2 = self.prev_price_1;
        self.prev_price_1 = Some(price);
        self.hp2 = self.hp1;
        self.hp1 = hp;
        self.last = Some(hp);
        Some(hp)
    }

    fn reset(&mut self) {
        self.prev_price_1 = None;
        self.prev_price_2 = None;
        self.hp1 = 0.0;
        self.hp2 = 0.0;
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
        "HighpassFilter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(HighpassFilter::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let hp = HighpassFilter::new(48).unwrap();
        assert_eq!(hp.period(), 48);
        assert_eq!(hp.warmup_period(), 1);
        assert_eq!(hp.name(), "HighpassFilter");
        assert!(!hp.is_ready());
        assert_eq!(hp.value(), None);
    }

    #[test]
    fn first_bars_are_zero() {
        let mut hp = HighpassFilter::new(48).unwrap();
        assert_eq!(hp.update(100.0), Some(0.0));
        assert_eq!(hp.update(101.0), Some(0.0));
        assert!(hp.is_ready());
    }

    #[test]
    fn constant_input_stays_zero() {
        let mut hp = HighpassFilter::new(48).unwrap();
        for v in hp.batch(&[50.0; 200]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn pure_trend_is_attenuated() {
        // A straight ramp is low-frequency -> the highpass should drive its
        // output small after warmup (the trend is removed).
        let mut hp = HighpassFilter::new(20).unwrap();
        let out: Vec<f64> = hp
            .batch(&(0..400).map(f64::from).collect::<Vec<_>>())
            .into_iter()
            .flatten()
            .skip(200)
            .collect();
        for v in out {
            assert!(v.abs() < 5.0, "trend should be attenuated, got {v}");
        }
    }

    #[test]
    fn ignores_non_finite() {
        let mut hp = HighpassFilter::new(48).unwrap();
        hp.batch(&(0..40).map(f64::from).collect::<Vec<_>>());
        let before = hp.value();
        assert_eq!(hp.update(f64::NAN), None);
        // The rejected input must not have disturbed the state.
        assert_eq!(hp.value(), before);
    }

    #[test]
    fn reset_clears_state() {
        let mut hp = HighpassFilter::new(48).unwrap();
        hp.batch(&(0..40).map(f64::from).collect::<Vec<_>>());
        assert!(hp.is_ready());
        hp.reset();
        assert!(!hp.is_ready());
        assert_eq!(hp.update(100.0), Some(0.0));
    }

    #[test]
    fn batch_equals_streaming() {
        let xs: Vec<f64> = (0..120)
            .map(|i| 100.0 + f64::from(i) + (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let batch = HighpassFilter::new(48).unwrap().batch(&xs);
        let mut b = HighpassFilter::new(48).unwrap();
        let streamed: Vec<_> = xs.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            HighpassFilter::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
