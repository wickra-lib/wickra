//! Heikin-Ashi Oscillator — the (smoothed) Heikin-Ashi candle body as a zero-line oscillator.

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::indicators::heikin_ashi::HeikinAshi;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Heikin-Ashi Oscillator — the body of the [`HeikinAshi`](crate::HeikinAshi)
/// candle (`ha_close − ha_open`), optionally EMA-smoothed, as an oscillator around
/// zero.
///
/// ```text
/// body = ha_close − ha_open
/// HAO  = EMA(body, period)
/// ```
///
/// A Heikin-Ashi candle is bullish when its close is above its open and bearish
/// when below; the size of that body measures conviction. Plotting the body as an
/// oscillator turns the visual HA colour/strength into a number: positive =
/// bullish HA candles, negative = bearish, and the magnitude is trend strength.
/// Smoothing the body with an EMA (`period`) damps single-bar noise so zero-line
/// crosses mark cleaner trend changes. With `period == 1` the oscillator is the raw
/// HA body.
///
/// The output is centred on zero (price units). The first value lands after
/// `period` inputs (the HA transform itself needs only one). Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, HeikinAshiOscillator};
///
/// let mut indicator = HeikinAshiOscillator::new(5).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let c = Candle::new(base, base + 1.0, base - 1.0, base + 0.5, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct HeikinAshiOscillator {
    period: usize,
    ha: HeikinAshi,
    ema: Ema,
    last: Option<f64>,
}

impl HeikinAshiOscillator {
    /// Construct a Heikin-Ashi Oscillator with the given EMA smoothing `period`
    /// (use `1` for the raw body).
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
            ha: HeikinAshi::new(),
            ema: Ema::new(period)?,
            last: None,
        })
    }

    /// Configured smoothing period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for HeikinAshiOscillator {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let ha = self.ha.update(candle).expect("HeikinAshi emits every bar");
        let body = ha.close - ha.open;
        let v = self.ema.update(body)?;
        self.last = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.ha.reset();
        self.ema.reset();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "HeikinAshiOscillator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(open: f64, high: f64, low: f64, close: f64) -> Candle {
        Candle::new_unchecked(open, high, low, close, 1_000.0, 0)
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            HeikinAshiOscillator::new(0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let h = HeikinAshiOscillator::new(5).unwrap();
        assert_eq!(h.period(), 5);
        assert_eq!(h.warmup_period(), 5);
        assert_eq!(h.name(), "HeikinAshiOscillator");
        assert!(!h.is_ready());
        assert_eq!(h.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut h = HeikinAshiOscillator::new(3).unwrap();
        let candles: Vec<Candle> = (0..6)
            .map(|i| {
                let b = 100.0 + f64::from(i);
                c(b, b + 1.0, b - 1.0, b + 0.5)
            })
            .collect();
        let out = h.batch(&candles);
        for v in out.iter().take(2) {
            assert!(v.is_none());
        }
        assert!(out[2].is_some());
    }

    #[test]
    fn uptrend_is_positive() {
        let mut h = HeikinAshiOscillator::new(3).unwrap();
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let b = 100.0 + 2.0 * f64::from(i);
                c(b, b + 1.0, b - 1.0, b + 1.5)
            })
            .collect();
        let last = h.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(
            last > 0.0,
            "uptrend should give a positive HA body, got {last}"
        );
    }

    #[test]
    fn downtrend_is_negative() {
        let mut h = HeikinAshiOscillator::new(3).unwrap();
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let b = 200.0 - 2.0 * f64::from(i);
                c(b, b + 1.0, b - 1.0, b - 1.5)
            })
            .collect();
        let last = h.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(
            last < 0.0,
            "downtrend should give a negative HA body, got {last}"
        );
    }

    #[test]
    fn flat_market_near_zero() {
        let mut h = HeikinAshiOscillator::new(3).unwrap();
        let last = h
            .batch(&[c(100.0, 100.5, 99.5, 100.0); 30])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn reset_clears_state() {
        let mut h = HeikinAshiOscillator::new(3).unwrap();
        h.batch(
            &(0..10)
                .map(|i| {
                    let b = 100.0 + f64::from(i);
                    c(b, b + 1.0, b - 1.0, b)
                })
                .collect::<Vec<_>>(),
        );
        assert!(h.is_ready());
        h.reset();
        assert!(!h.is_ready());
        assert_eq!(h.value(), None);
        assert_eq!(h.update(c(100.0, 101.0, 99.0, 100.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let b = 100.0 + (f64::from(i) * 0.25).sin() * 9.0;
                c(b, b + 1.0, b - 1.0, b + 0.3)
            })
            .collect();
        let batch = HeikinAshiOscillator::new(5).unwrap().batch(&candles);
        let mut b = HeikinAshiOscillator::new(5).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            HeikinAshiOscillator::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
