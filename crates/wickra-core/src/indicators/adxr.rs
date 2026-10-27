//! Average Directional Movement Index Rating (ADXR).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::adx::Adx;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Wilder's Average Directional Movement Index Rating.
///
/// `ADXR` smooths the [`Adx`] line by averaging its current value with the value
/// it had `period` bars ago:
///
/// ```text
/// ADXR_t = (ADX_t + ADX_{t - (period - 1)}) / 2
/// ```
///
/// The lookback length is the same `period` that feeds the underlying ADX.
/// Wilder introduced ADXR alongside ADX in *New Concepts in Technical Trading
/// Systems* (1978) as a more stable directional-strength reading: because the
/// older `ADX` is `period - 1` bars stale, ADXR responds more slowly than ADX
/// and is used to compare trend-strength between different instruments.
///
/// The first complete `ADXR` is emitted after `3 * period - 1` candles
/// (`2 * period` to seed the ADX plus another `period - 1` to fill the
/// lookback ring).
///
/// # Example
///
/// ```
/// use wickra_core::{Adxr, Candle, Indicator};
///
/// let mut indicator = Adxr::new(5).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Adxr {
    period: usize,
    adx: Adx,
    /// Ring buffer of the most recent `period` `ADX` values; the front is the
    /// oldest, the back is the newest. ADXR is `(back + front) / 2` once the
    /// ring is full.
    window: VecDeque<f64>,
    last: Option<f64>,
}

impl Adxr {
    /// Construct a new ADXR with the given Wilder smoothing period.
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
            adx: Adx::new(period)?,
            window: VecDeque::with_capacity(period),
            last: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for Adxr {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let adx_value = self.adx.update(candle)?.adx;
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(adx_value);
        if self.window.len() < self.period {
            return None;
        }
        let oldest = *self.window.front().expect("ring is full");
        let adxr = f64::midpoint(adx_value, oldest);
        self.last = Some(adxr);
        Some(adxr)
    }

    fn reset(&mut self) {
        self.adx.reset();
        self.window.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // ADX warmup is `2 * period` and emits one `ADX` per subsequent candle;
        // the ADXR ring then needs `period - 1` more candles to fill, so the
        // first ADXR lands at `2 * period + (period - 1) = 3 * period - 1`.
        3 * self.period - 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ADXR"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn candle(h: f64, l: f64, c: f64, ts: i64) -> Candle {
        Candle::new(c, h, l, c, 1.0, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Adxr::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut a = Adxr::new(14).unwrap();
        assert_eq!(a.period(), 14);
        assert_eq!(a.warmup_period(), 41);
        assert_eq!(a.name(), "ADXR");
        assert!(a.value().is_none());
        // Drive past warmup.
        for i in 0..50_i64 {
            let base = 100.0 + (i as f64) * 2.0;
            a.update(candle(base + 1.0, base - 0.5, base + 0.5, i));
        }
        assert!(a.value().is_some());
    }

    #[test]
    fn pure_uptrend_yields_finite_positive_adxr() {
        let candles: Vec<Candle> = (0..80_i64)
            .map(|i| {
                let base = 100.0 + (i as f64) * 2.0;
                candle(base + 1.0, base - 0.5, base + 0.5, i)
            })
            .collect();
        let mut a = Adxr::new(14).unwrap();
        let last = a.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(last > 0.0 && last <= 100.0 + 1e-9);
    }

    #[test]
    fn constant_series_yields_zero_adxr() {
        let candles: Vec<Candle> = (0..50_i64).map(|i| candle(10.0, 10.0, 10.0, i)).collect();
        let mut a = Adxr::new(5).unwrap();
        let last = a.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, 0.0);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let candles: Vec<Candle> = (0..80_i64)
            .map(|i| {
                let p = 100.0 + ((i as f64) * 0.3).sin() * 5.0;
                candle(p + 1.0, p - 1.0, p, i)
            })
            .collect();
        let mut a = Adxr::new(5).unwrap();
        let out = a.batch(&candles);
        let warmup = 3 * 5 - 1; // 14
        for v in out.iter().take(warmup - 1) {
            assert!(v.is_none());
        }
        assert!(out[warmup - 1].is_some());
    }

    #[test]
    fn reference_value_against_explicit_adx_average() {
        // The first ADXR(p) emits at index `3p - 2` (0-based), and equals
        // (ADX[index] + ADX[index - (p - 1)]) / 2. Verify against a separate
        // ADX run.
        let candles: Vec<Candle> = (0..60_i64)
            .map(|i| {
                let p = 100.0 + ((i as f64) * 0.2).sin() * 6.0;
                candle(p + 1.5, p - 1.5, p, i)
            })
            .collect();
        let period = 5;
        let mut adx = Adx::new(period).unwrap();
        let adx_out: Vec<_> = adx
            .batch(&candles)
            .into_iter()
            .map(|o| o.map(|x| x.adx))
            .collect();
        let mut adxr = Adxr::new(period).unwrap();
        let adxr_out = adxr.batch(&candles);
        // First ADXR index (0-based) = 3 * period - 2 = 13.
        let first = 3 * period - 2;
        let prev = first - (period - 1);
        let expected = f64::midpoint(adx_out[first].unwrap(), adx_out[prev].unwrap());
        assert_relative_eq!(adxr_out[first].unwrap(), expected, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60_i64)
            .map(|i| {
                let p = 100.0 + ((i as f64) * 0.25).sin() * 5.0;
                candle(p + 1.0, p - 1.0, p, i)
            })
            .collect();
        let mut a = Adxr::new(7).unwrap();
        let mut b = Adxr::new(7).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|c| b.update(*c)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..60_i64).map(|i| candle(11.0, 9.0, 10.0, i)).collect();
        let mut a = Adxr::new(5).unwrap();
        a.batch(&candles);
        assert!(a.is_ready());
        a.reset();
        assert!(!a.is_ready());
        assert_eq!(a.update(candles[0]), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Adxr::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
