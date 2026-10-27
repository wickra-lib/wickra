//! Chande Momentum Oscillator.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Chande Momentum Oscillator — Tushar Chande's bounded momentum gauge.
///
/// Over the last `period` price *changes* it sums the gains and the losses
/// separately and reports:
///
/// ```text
/// CMO = 100 · (Σ gains − Σ losses) / (Σ gains + Σ losses)
/// ```
///
/// The result is bounded in `[−100, 100]`: `+100` is a window of pure gains,
/// `−100` a window of pure losses, `0` a perfect balance. Unlike RSI the sums
/// are *unsmoothed* — every change in the window carries equal weight — so CMO
/// reacts faster and swings wider.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Cmo};
///
/// let mut indicator = Cmo::new(14).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert_eq!(last, Some(100.0)); // pure uptrend saturates at +100
/// ```
#[derive(Debug, Clone)]
pub struct Cmo {
    period: usize,
    prev_price: Option<f64>,
    /// Rolling window of `(gain, loss)` pairs, oldest at the front.
    window: VecDeque<(f64, f64)>,
    sum_gain: f64,
    sum_loss: f64,
    current: Option<f64>,
}

impl Cmo {
    /// Construct a new CMO with the given period.
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
            prev_price: None,
            window: VecDeque::with_capacity(period),
            sum_gain: 0.0,
            sum_loss: 0.0,
            current: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.current
    }
}

impl Indicator for Cmo {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            // Non-finite input is ignored; state is left untouched.
            return None;
        }
        let Some(prev) = self.prev_price else {
            self.prev_price = Some(input);
            return None;
        };
        self.prev_price = Some(input);

        let change = input - prev;
        let gain = change.max(0.0);
        let loss = (-change).max(0.0);

        if self.window.len() == self.period {
            let (old_gain, old_loss) = self.window.pop_front().expect("window is non-empty");
            self.sum_gain -= old_gain;
            self.sum_loss -= old_loss;
        }
        self.window.push_back((gain, loss));
        self.sum_gain += gain;
        self.sum_loss += loss;

        if self.window.len() < self.period {
            return None;
        }
        let denom = self.sum_gain + self.sum_loss;
        let cmo = if denom == 0.0 {
            // A flat window (no gains and no losses): momentum is exactly zero.
            0.0
        } else {
            100.0 * (self.sum_gain - self.sum_loss) / denom
        };
        self.current = Some(cmo);
        Some(cmo)
    }

    fn reset(&mut self) {
        self.prev_price = None;
        self.window.clear();
        self.sum_gain = 0.0;
        self.sum_loss = 0.0;
        self.current = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.current.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "CMO"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(Cmo::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `value` (66-73) and the
    /// Indicator-impl `name` body (134-136). Existing tests inspect
    /// CMO output but never query the metadata.
    #[test]
    fn accessors_and_metadata() {
        let mut cmo = Cmo::new(14).unwrap();
        assert_eq!(cmo.period(), 14);
        assert_eq!(cmo.name(), "CMO");
        assert_eq!(cmo.value(), None);
        for i in 1..=15 {
            cmo.update(f64::from(i));
        }
        assert!(cmo.value().is_some());
    }

    #[test]
    fn reference_value() {
        // CMO(3) over [10, 11, 10, 12]: changes +1, −1, +2.
        // Σgain = 3, Σloss = 1 -> 100·(3−1)/(3+1) = 50.
        let mut cmo = Cmo::new(3).unwrap();
        let out = cmo.batch(&[10.0, 11.0, 10.0, 12.0]);
        assert_eq!(cmo.warmup_period(), 4);
        assert_eq!(out[0], None);
        assert_eq!(out[2], None);
        assert_relative_eq!(out[3].unwrap(), 50.0, epsilon = 1e-12);
    }

    #[test]
    fn pure_uptrend_saturates_at_plus_100() {
        let mut cmo = Cmo::new(5).unwrap();
        let out = cmo.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        for v in out.iter().skip(6).flatten() {
            assert_relative_eq!(*v, 100.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn pure_downtrend_saturates_at_minus_100() {
        let mut cmo = Cmo::new(5).unwrap();
        let out = cmo.batch(&(1..=20).rev().map(f64::from).collect::<Vec<_>>());
        for v in out.iter().skip(6).flatten() {
            assert_relative_eq!(*v, -100.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut cmo = Cmo::new(5).unwrap();
        let out = cmo.batch(&[42.0; 20]);
        for v in out.iter().skip(6).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut cmo = Cmo::new(3).unwrap();
        let out = cmo.batch(&[10.0, 11.0, 10.0, 12.0]);
        out[3].expect("CMO(3) ready after four inputs");
        assert_eq!(cmo.update(f64::NAN), None);
        assert_eq!(cmo.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut cmo = Cmo::new(3).unwrap();
        cmo.batch(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        assert!(cmo.is_ready());
        cmo.reset();
        assert!(!cmo.is_ready());
        assert_eq!(cmo.update(10.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=60)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 6.0)
            .collect();
        let batch = Cmo::new(9).unwrap().batch(&prices);
        let mut b = Cmo::new(9).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Cmo::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
