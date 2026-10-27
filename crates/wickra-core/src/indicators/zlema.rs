//! Zero-Lag Exponential Moving Average.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

use super::Ema;

/// Zero-Lag Exponential Moving Average (Ehlers & Way).
///
/// A standard EMA applied to a *de-lagged* price series. The de-lagged input
/// is `2·price_t − price_{t−lag}` with `lag = (period − 1) / 2`; adding that
/// momentum term to the current price cancels most of the EMA's group delay,
/// so the average tracks turns far more tightly than a plain [`Ema`].
///
/// The first output lands after exactly `lag + period` inputs: `lag` inputs
/// are needed before the de-lagged series is defined, then `period` de-lagged
/// values seed the inner EMA.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Zlema};
///
/// let mut indicator = Zlema::new(10).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Zlema {
    period: usize,
    lag: usize,
    /// Rolling buffer of the last `lag + 1` raw inputs, oldest at the front.
    window: VecDeque<f64>,
    ema: Ema,
}

impl Zlema {
    /// Construct a new ZLEMA with the given period.
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
        let lag = (period - 1) / 2;
        Ok(Self {
            period,
            lag,
            window: VecDeque::with_capacity(lag + 1),
            ema: Ema::new(period)?,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Lag offset `(period − 1) / 2` used to de-lag the price series.
    pub const fn lag(&self) -> usize {
        self.lag
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.ema.value()
    }
}

impl Indicator for Zlema {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            // Non-finite input is ignored; state is left untouched.
            return None;
        }
        if self.window.len() == self.lag + 1 {
            self.window.pop_front();
        }
        self.window.push_back(input);
        if self.window.len() < self.lag + 1 {
            return None;
        }
        let lagged = *self.window.front().expect("window is non-empty");
        let de_lagged = 2.0f64.mul_add(input, -lagged);
        self.ema.update(de_lagged)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.ema.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.lag + self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.ema.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ZLEMA"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(Zlema::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `value` (62-64, 72-74) and
    /// the Indicator-impl `name` body (111-113). `lag` is already covered
    /// by `lag_is_half_of_period_minus_one`.
    #[test]
    fn accessors_and_metadata() {
        let mut z = Zlema::new(5).unwrap();
        assert_eq!(z.period(), 5);
        assert_eq!(z.name(), "ZLEMA");
        assert_eq!(z.value(), None);
        for i in 1..=z.warmup_period() {
            z.update(f64::from(u32::try_from(i).unwrap()));
        }
        assert!(z.value().is_some());
    }

    #[test]
    fn lag_is_half_of_period_minus_one() {
        assert_eq!(Zlema::new(3).unwrap().lag(), 1);
        assert_eq!(Zlema::new(10).unwrap().lag(), 4);
        assert_eq!(Zlema::new(1).unwrap().lag(), 0);
    }

    #[test]
    fn reference_values() {
        // ZLEMA(3): lag = 1, de_lagged_t = 2·xt − x_{t-1}, then EMA(3).
        // [1,2,3,4,5] -> de-lagged [_, 3, 4, 5, 6]; EMA(3) seeds at the third
        // de-lagged value: mean(3,4,5) = 4.0; next = 0.5·6 + 0.5·4 = 5.0.
        let mut zlema = Zlema::new(3).unwrap();
        let out = zlema.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(zlema.warmup_period(), 4);
        assert_eq!(out[0], None);
        assert_eq!(out[1], None);
        assert_eq!(out[2], None);
        assert_relative_eq!(out[3].unwrap(), 4.0, epsilon = 1e-12);
        assert_relative_eq!(out[4].unwrap(), 5.0, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_yields_the_constant() {
        // De-lagging a constant gives the same constant (2c − c = c).
        let mut zlema = Zlema::new(7).unwrap();
        let out = zlema.batch(&[33.0; 60]);
        for x in out.iter().skip(zlema.warmup_period() - 1).flatten() {
            assert_relative_eq!(*x, 33.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut zlema = Zlema::new(3).unwrap();
        let out = zlema.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let last = out[4];
        assert!(last.is_some());
        assert_eq!(zlema.update(f64::NAN), None);
        assert_eq!(zlema.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut zlema = Zlema::new(5).unwrap();
        zlema.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        assert!(zlema.is_ready());
        zlema.reset();
        assert!(!zlema.is_ready());
        assert_eq!(zlema.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=60)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 8.0)
            .collect();
        let batch = Zlema::new(9).unwrap().batch(&prices);
        let mut b = Zlema::new(9).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Zlema::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
