//! Chande Forecast Oscillator (CFO).

use crate::error::{Error, Result};
use crate::indicators::linreg::LinearRegression;
use crate::traits::Indicator;

/// Tushar Chande's Forecast Oscillator — the percentage difference between
/// the close and the endpoint of an `n`-bar linear-regression forecast of the
/// close.
///
/// ```text
/// CFO_t = 100 · (close_t − LinearRegression(close, period)_t) / close_t
/// ```
///
/// Positive readings mean the close is *above* the linear forecast (price has
/// overshot trend); negative readings mean it sits below. Wraps the existing
/// `LinearRegression` so the warmup matches.
///
/// # Example
///
/// ```
/// use wickra_core::{Cfo, Indicator};
///
/// let mut cfo = Cfo::new(14).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = cfo.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Cfo {
    period: usize,
    linreg: LinearRegression,
    current: Option<f64>,
}

impl Cfo {
    /// # Errors
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
            linreg: LinearRegression::new(period)?,
            current: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Cfo {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        let forecast = self.linreg.update(input)?;
        // Hold the previous value if the close is zero — the percentage form
        // is undefined and a return of inf would propagate badly.
        if input == 0.0 {
            return self.current;
        }
        let value = 100.0 * (input - forecast) / input;
        self.current = Some(value);
        Some(value)
    }

    fn reset(&mut self) {
        self.linreg.reset();
        self.current = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.current.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "CFO"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Cfo::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let cfo = Cfo::new(14).unwrap();
        assert_eq!(cfo.period(), 14);
        assert_eq!(cfo.warmup_period(), 14);
        assert_eq!(cfo.name(), "CFO");
    }

    #[test]
    fn constant_series_yields_zero() {
        // LinReg of a constant series equals the constant, so close − forecast
        // is 0 and CFO is 0.
        let mut cfo = Cfo::new(5).unwrap();
        let out = cfo.batch(&[42.0_f64; 30]);
        for v in out.iter().skip(4).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn perfect_linear_series_yields_zero() {
        // LinReg of a perfectly linear input fits the line exactly, so the
        // close lands on the forecast and CFO = 0.
        let mut cfo = Cfo::new(5).unwrap();
        let prices: Vec<f64> = (1..=20).map(|i| f64::from(i) * 2.0).collect();
        let out = cfo.batch(&prices);
        for v in out.iter().skip(4).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn warmup_emits_first_value_at_period() {
        let mut cfo = Cfo::new(3).unwrap();
        for i in 1..=2 {
            assert_eq!(cfo.update(f64::from(i)), None);
        }
        assert!(cfo.update(3.0).is_some());
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=80)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let mut a = Cfo::new(14).unwrap();
        let mut b = Cfo::new(14).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut cfo = Cfo::new(5).unwrap();
        cfo.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        assert!(cfo.is_ready());
        cfo.reset();
        assert!(!cfo.is_ready());
        assert_eq!(cfo.update(1.0), None);
    }

    #[test]
    fn zero_close_holds_value() {
        let mut cfo = Cfo::new(3).unwrap();
        cfo.batch(&[1.0_f64, 2.0, 3.0]);
        let before = cfo.current;
        assert_eq!(cfo.update(0.0), before);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Cfo::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
