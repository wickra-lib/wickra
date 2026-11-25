//! Stochastic RSI.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

use super::Rsi;

/// Stochastic RSI — the Stochastic Oscillator formula applied to the RSI series
/// instead of to price.
///
/// RSI itself rarely reaches its `[0, 100]` extremes, so it spends most of its
/// life bunched in the middle of the range. `StochRSI` re-scales it: it reports
/// where the *current* RSI sits within its own high/low range over the last
/// `stoch_period` bars, which makes overbought/oversold turns far easier to
/// see.
///
/// ```text
/// StochRSI = 100 · (RSI − min(RSI, stoch_period)) / (max(RSI, …) − min(RSI, …))
/// ```
///
/// The output is bounded in `[0, 100]`. A flat RSI window (zero range) is
/// reported as the neutral `50.0`, matching the [`Stochastic`](crate::Stochastic)
/// convention.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, StochRsi};
///
/// let mut indicator = StochRsi::new(14, 14).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.5).sin() * 10.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct StochRsi {
    rsi_period: usize,
    stoch_period: usize,
    rsi: Rsi,
    /// Rolling window of the last `stoch_period` RSI values.
    window: VecDeque<f64>,
    last: Option<f64>,
}

impl StochRsi {
    /// Construct a new `StochRSI` with the RSI period and the stochastic lookback.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if either period is `0`.
    pub fn new(rsi_period: usize, stoch_period: usize) -> Result<Self> {
        if rsi_period == 0 || stoch_period == 0 {
            return Err(Error::PeriodZero);
        }
        Ok(Self {
            rsi_period,
            stoch_period,
            rsi: Rsi::new(rsi_period)?,
            window: VecDeque::with_capacity(stoch_period),
            last: None,
        })
    }

    /// The `(rsi_period, stoch_period)` pair.
    pub const fn periods(&self) -> (usize, usize) {
        (self.rsi_period, self.stoch_period)
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for StochRsi {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            // Non-finite input is ignored; state is left untouched.
            return None;
        }
        let rsi_value = self.rsi.update(input)?;

        if self.window.len() == self.stoch_period {
            self.window.pop_front();
        }
        self.window.push_back(rsi_value);
        if self.window.len() < self.stoch_period {
            return None;
        }

        let max = self
            .window
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let min = self.window.iter().copied().fold(f64::INFINITY, f64::min);
        let range = max - min;
        let stoch = if range == 0.0 {
            // Flat RSI window: report the neutral midpoint.
            50.0
        } else {
            // Divide before scaling: at the window maximum the quotient is
            // exactly 1, so the output is exactly 100 and never one ulp above.
            100.0 * ((rsi_value - min) / range)
        };
        self.last = Some(stoch);
        Some(stoch)
    }

    fn reset(&mut self) {
        self.rsi.reset();
        self.window.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // RSI emits its first value at input `rsi_period + 1`; the stochastic
        // window then needs `stoch_period` RSI values.
        self.rsi_period + self.stoch_period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "StochRSI"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(StochRsi::new(0, 14), Err(Error::PeriodZero)));
        assert!(matches!(StochRsi::new(14, 0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `periods` / `value` (69-76) and the
    /// Indicator-impl `name` body (131-133). `warmup_period` is already
    /// covered by `first_emission_at_warmup_period`.
    #[test]
    fn accessors_and_metadata() {
        let mut sr = StochRsi::new(14, 14).unwrap();
        assert_eq!(sr.periods(), (14, 14));
        assert_eq!(sr.name(), "StochRSI");
        assert_eq!(sr.value(), None);
        for i in 1..=sr.warmup_period() {
            sr.update(100.0 + f64::from(u32::try_from(i).unwrap()));
        }
        assert!(sr.value().is_some());
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut sr = StochRsi::new(5, 4).unwrap();
        assert_eq!(sr.warmup_period(), 9);
        let prices: Vec<f64> = (1..=40)
            .map(|i| 100.0 + (f64::from(i) * 0.6).sin() * 8.0)
            .collect();
        let out = sr.batch(&prices);
        for v in out.iter().take(8) {
            assert!(v.is_none());
        }
        assert!(out[8].is_some());
    }

    #[test]
    fn flat_rsi_window_yields_50() {
        // A constant price series gives a constant RSI (50.0), so the StochRSI
        // window has zero range and reports the neutral midpoint.
        let mut sr = StochRsi::new(5, 4).unwrap();
        let out = sr.batch(&[100.0; 40]);
        for v in out.iter().skip(9).flatten() {
            assert_relative_eq!(*v, 50.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn pure_uptrend_yields_50() {
        // A pure uptrend pins RSI at 100, so its window is again flat.
        let mut sr = StochRsi::new(5, 4).unwrap();
        let out = sr.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        for v in out.iter().skip(9).flatten() {
            assert_relative_eq!(*v, 50.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn irregular_uptrend_yields_exactly_50() {
        // Uneven up-steps keep RSI saturated at 100, but the average gain takes
        // many values; one-ulp noise on the saturated RSI used to give the
        // window a tiny range, so StochRSI read 0 or 100 instead of the flat 50
        // (#509).
        let mut price = 100.0;
        let prices: Vec<f64> = (0..200)
            .map(|i| {
                price += 0.1 + 0.37 * (f64::from(i) * 1.3).sin().abs();
                price
            })
            .collect();
        let mut sr = StochRsi::new(7, 10).unwrap();
        let out: Vec<f64> = sr.batch(&prices).into_iter().flatten().collect();
        assert_eq!(out.len(), 200 - 16);
        assert!(out.iter().all(|&v| v == 50.0));
    }

    #[test]
    fn window_maximum_reads_exactly_100() {
        // Whenever the current RSI is the window maximum the output is
        // `100 · (range / range)`; scaling before dividing put it one ulp above
        // 100 on some bars.
        let prices: Vec<f64> = (1..=600)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 15.0 + (f64::from(i) * 0.07).cos() * 6.0)
            .collect();
        let out: Vec<f64> = StochRsi::new(14, 14)
            .unwrap()
            .batch(&prices)
            .into_iter()
            .flatten()
            .collect();
        assert!(out.iter().all(|&v| (0.0..=100.0).contains(&v)));
        assert!(out.iter().filter(|&&v| v == 100.0).count() > 10);
    }

    #[test]
    fn output_stays_within_0_100() {
        let mut sr = StochRsi::new(14, 14).unwrap();
        let prices: Vec<f64> = (1..=200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 15.0 + (f64::from(i) * 0.07).cos() * 6.0)
            .collect();
        for v in sr.batch(&prices).into_iter().flatten() {
            assert!((0.0..=100.0).contains(&v), "StochRSI out of range: {v}");
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut sr = StochRsi::new(5, 4).unwrap();
        let prices: Vec<f64> = (1..=40)
            .map(|i| 100.0 + (f64::from(i) * 0.6).sin() * 8.0)
            .collect();
        let out = sr.batch(&prices);
        let last = *out.last().unwrap();
        assert!(last.is_some());
        assert_eq!(sr.update(f64::NAN), None);
        assert_eq!(sr.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut sr = StochRsi::new(5, 4).unwrap();
        sr.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        assert!(sr.is_ready());
        sr.reset();
        assert!(!sr.is_ready());
        assert_eq!(sr.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 12.0)
            .collect();
        let batch = StochRsi::new(14, 14).unwrap().batch(&prices);
        let mut b = StochRsi::new(14, 14).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }
}
