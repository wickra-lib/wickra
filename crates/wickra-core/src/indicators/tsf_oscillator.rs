//! Time Series Forecast Oscillator (TSF Oscillator).

use crate::error::{Error, Result};
use crate::indicators::tsf::Tsf;
use crate::traits::Indicator;

/// Time Series Forecast Oscillator — the percentage gap between the close and
/// the **one-bar-ahead** time-series forecast of the close.
///
/// ```text
/// TSFOsc_t = 100 · (close_t − TSF(close, period)_t) / close_t
/// ```
///
/// where [`Tsf`](crate::Tsf) projects the rolling least-squares line one bar
/// past the window (`a + b·period`). It is the close-relative companion to
/// [`Cfo`](crate::Cfo), which measures the same percentage gap against the
/// regression value at the *current* bar (`a + b·(period − 1)`). Because `TSF`
/// advances one bar further than `LinearRegression`, the two differ by exactly
/// the slope term `100·b/close`: on a trending series `TSFOsc` reads more
/// negative in an uptrend (the forecast has already stepped above price) and
/// more positive in a downtrend.
///
/// Positive readings mean the close sits *above* its forward forecast (price
/// has overshot the projected trend); negative readings mean it sits below.
/// Wraps the existing `Tsf` so the warmup matches.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, TsfOscillator};
///
/// let mut indicator = TsfOscillator::new(14).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct TsfOscillator {
    period: usize,
    tsf: Tsf,
    current: Option<f64>,
}

impl TsfOscillator {
    /// Construct a new TSF oscillator over `period` inputs.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` — a regression line is
    /// undefined for fewer than two points.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "TSF oscillator needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            tsf: Tsf::new(period)?,
            current: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for TsfOscillator {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        let forecast = self.tsf.update(input)?;
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
        self.tsf.reset();
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
        "TsfOscillator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_short_period() {
        assert!(matches!(
            TsfOscillator::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            TsfOscillator::new(0),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let osc = TsfOscillator::new(14).unwrap();
        assert_eq!(osc.period(), 14);
        assert_eq!(osc.warmup_period(), 14);
        assert_eq!(osc.name(), "TsfOscillator");
        assert!(!osc.is_ready());
    }

    #[test]
    fn reference_value() {
        // period 3 over [1, 2, 9]: fit y = 0 + 4x, one-bar-ahead TSF at x = 3
        // is 12. With close = 9, TSFOsc = 100·(9 − 12)/9 = −33.3333…%.
        let mut osc = TsfOscillator::new(3).unwrap();
        let out = osc.batch(&[1.0_f64, 2.0, 9.0]);
        assert!(out[0].is_none());
        assert!(out[1].is_none());
        assert_relative_eq!(out[2].unwrap(), -100.0 / 3.0, epsilon = 1e-9);
        assert!(osc.is_ready());
    }

    #[test]
    fn constant_series_yields_zero() {
        // On a flat series the regression slope is 0, so the one-bar-ahead TSF
        // equals the constant and close − forecast is exactly 0.
        let mut osc = TsfOscillator::new(5).unwrap();
        let out = osc.batch(&[42.0_f64; 30]);
        for v in out.iter().skip(4).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn linear_uptrend_reads_negative() {
        // Unlike CFO (evaluated at the current bar), the forecast steps one bar
        // ahead, so on a rising line the projection sits above the close and the
        // oscillator is negative: TSFOsc = −100·slope/close.
        let mut osc = TsfOscillator::new(5).unwrap();
        let prices: Vec<f64> = (1..=20).map(|i| f64::from(i) * 2.0).collect();
        let out = osc.batch(&prices);
        for v in out.iter().skip(4).flatten() {
            assert!(*v < 0.0, "uptrend forecast overshoots close, got {v}");
        }
    }

    #[test]
    fn warmup_emits_first_value_at_period() {
        let mut osc = TsfOscillator::new(3).unwrap();
        assert_eq!(osc.update(1.0), None);
        assert_eq!(osc.update(2.0), None);
        assert!(osc.update(3.0).is_some());
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=80)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let mut a = TsfOscillator::new(14).unwrap();
        let mut b = TsfOscillator::new(14).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut osc = TsfOscillator::new(5).unwrap();
        osc.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        assert!(osc.is_ready());
        osc.reset();
        assert!(!osc.is_ready());
        assert_eq!(osc.update(1.0), None);
    }

    #[test]
    fn zero_close_holds_value() {
        let mut osc = TsfOscillator::new(3).unwrap();
        osc.batch(&[1.0_f64, 2.0, 3.0]);
        let before = osc.current;
        assert_eq!(osc.update(0.0), before);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            TsfOscillator::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
