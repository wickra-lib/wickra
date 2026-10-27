//! Ehlers Center of Gravity Oscillator.
#![allow(clippy::manual_midpoint)]

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Ehlers' Center of Gravity (CG) oscillator.
///
/// Treats the most recent `period` prices as masses and reports the
/// weighted "center" of that mass distribution, negated so positive readings
/// correspond to recent strength:
///
/// ```text
/// num = sum_{k=0..period-1} (1 + k) * price[t - k]
/// den = sum_{k=0..period-1} price[t - k]
/// cg  = - num / den + (period + 1) / 2
/// ```
///
/// The constant offset centres the oscillator around zero. From Ehlers,
/// *Cybernetic Analysis for Stocks and Futures* (2004, ch. 7).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, CenterOfGravity};
///
/// let mut cg = CenterOfGravity::new(10).unwrap();
/// let mut last = None;
/// for i in 0..30 {
///     last = cg.update(100.0 + (f64::from(i) * 0.2).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct CenterOfGravity {
    period: usize,
    window: VecDeque<f64>,
    last_value: Option<f64>,
}

impl CenterOfGravity {
    /// Construct with the rolling window length.
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

impl Indicator for CenterOfGravity {
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
        // Most recent has weight 1; oldest has weight `period`.
        let mut num = 0.0;
        let mut den = 0.0;
        for (k, p) in self.window.iter().rev().enumerate() {
            let w = 1.0 + k as f64;
            num += w * p;
            den += p;
        }
        let v = if den.abs() > f64::EPSILON {
            -num / den + (self.period as f64 + 1.0) / 2.0
        } else {
            0.0
        };
        self.last_value = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "CenterOfGravity"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(CenterOfGravity::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut cg = CenterOfGravity::new(10).unwrap();
        assert_eq!(cg.period(), 10);
        assert_eq!(cg.warmup_period(), 10);
        assert_eq!(cg.name(), "CenterOfGravity");
        assert!(!cg.is_ready());
        for i in 1..=10 {
            cg.update(f64::from(i));
        }
        assert!(cg.is_ready());
        assert!(cg.value().is_some());
    }

    #[test]
    fn constant_series_yields_zero() {
        // num = sum k * p, den = period * p, ratio = (period + 1) / 2,
        // so cg = - (period+1)/2 + (period+1)/2 = 0.
        let mut cg = CenterOfGravity::new(5).unwrap();
        let out = cg.batch(&[7.0_f64; 30]);
        for x in out.iter().skip(5).flatten() {
            assert_relative_eq!(*x, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=50).map(f64::from).collect();
        let mut a = CenterOfGravity::new(10).unwrap();
        let mut b = CenterOfGravity::new(10).unwrap();
        let batch = a.batch(&prices);
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut cg = CenterOfGravity::new(5).unwrap();
        cg.batch(&(1..=10).map(f64::from).collect::<Vec<_>>());
        let before = cg.value();
        assert!(before.is_some());
        assert_eq!(cg.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut cg = CenterOfGravity::new(5).unwrap();
        cg.batch(&(1..=10).map(f64::from).collect::<Vec<_>>());
        assert!(cg.is_ready());
        cg.reset();
        assert!(!cg.is_ready());
    }

    #[test]
    fn warmup_returns_none_until_seed() {
        let mut cg = CenterOfGravity::new(4).unwrap();
        assert_eq!(cg.update(1.0), None);
        assert_eq!(cg.update(2.0), None);
        assert_eq!(cg.update(3.0), None);
        assert!(cg.update(4.0).is_some());
    }

    #[test]
    fn zero_window_uses_zero_fallback() {
        // den == sum(prices) == 0 when the rolling window is all zeros, which
        // exercises the protective fallback in the divisor guard.
        let mut cg = CenterOfGravity::new(5).unwrap();
        let out = cg.batch(&[0.0_f64; 10]);
        for x in out.iter().skip(5).flatten() {
            assert_relative_eq!(*x, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            CenterOfGravity::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
