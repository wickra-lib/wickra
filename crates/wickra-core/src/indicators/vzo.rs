//! Volume Zone Oscillator (Walid Khalil).

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Walid Khalil's Volume Zone Oscillator — a normalised version of OBV-style
/// volume flow that swings within `[−100, 100]`.
///
/// Each bar contributes a *signed volume*: `+volume` on an up day, `−volume` on
/// a down day, `0` on an unchanged close. The VZO is the ratio of an EMA of
/// that signed volume to an EMA of the absolute volume, scaled by `100`:
///
/// ```text
/// R_t   = sign(close_t − close_{t−1}) · volume_t
/// VP_t  = EMA(R, period)_t                     (smoothed signed volume)
/// TV_t  = EMA(volume, period)_t                (smoothed absolute volume)
/// VZO_t = 100 · VP_t / TV_t
/// ```
///
/// Khalil's interpretation: `VZO > +60` overbought, `< −60` oversold, with the
/// zero line acting as a trend filter. The first bar only seeds the previous
/// close; both EMAs then need `period` samples to seed, so the first emission
/// lands at bar `period + 1`. A `TV_t == 0` (every bar had zero volume)
/// collapses the output to `0` instead of NaN.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Vzo};
///
/// let mut indicator = Vzo::new(14).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 50.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Vzo {
    period: usize,
    vp: Ema,
    tv: Ema,
    prev_close: Option<f64>,
}

impl Vzo {
    /// Construct a new VZO with the given EMA smoothing period.
    ///
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
            vp: Ema::new(period)?,
            tv: Ema::new(period)?,
            prev_close: None,
        })
    }

    /// Configured EMA smoothing period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Vzo {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let signed_volume = match self.prev_close {
            None => {
                self.prev_close = Some(candle.close);
                return None;
            }
            Some(prev) => {
                if candle.close > prev {
                    candle.volume
                } else if candle.close < prev {
                    -candle.volume
                } else {
                    0.0
                }
            }
        };
        self.prev_close = Some(candle.close);
        let vp = self.vp.update(signed_volume);
        let tv = self.tv.update(candle.volume);
        let (vp_v, tv_v) = (vp?, tv?);
        if tv_v == 0.0 {
            // No volume in the smoothing window -> ratio undefined; report 0.
            return Some(0.0);
        }
        Some(100.0 * vp_v / tv_v)
    }

    fn reset(&mut self) {
        self.vp.reset();
        self.tv.reset();
        self.prev_close = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One seed bar plus the EMA seed.
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.vp.is_ready() && self.tv.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "VZO"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(close: f64, volume: f64, ts: i64) -> Candle {
        Candle::new(close, close, close, close, volume, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Vzo::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let v = Vzo::new(14).unwrap();
        assert_eq!(v.period(), 14);
        assert_eq!(v.name(), "VZO");
        assert_eq!(v.warmup_period(), 15);
    }

    #[test]
    fn strictly_rising_series_saturates_to_plus_100() {
        // Every bar is an up-day with identical volume -> signed_volume == volume
        // on every bar -> VP and TV EMAs are equal -> ratio = 1 -> VZO = +100.
        let candles: Vec<Candle> = (0..60i64).map(|i| c(10.0 + i as f64, 100.0, i)).collect();
        let mut v = Vzo::new(5).unwrap();
        let out = v.batch(&candles);
        let last = out.iter().filter_map(|x| *x).next_back().unwrap();
        assert_relative_eq!(last, 100.0, epsilon = 1e-9);
    }

    #[test]
    fn strictly_falling_series_saturates_to_minus_100() {
        let candles: Vec<Candle> = (0..60i64).map(|i| c(200.0 - i as f64, 100.0, i)).collect();
        let mut v = Vzo::new(5).unwrap();
        let out = v.batch(&candles);
        let last = out.iter().filter_map(|x| *x).next_back().unwrap();
        assert_relative_eq!(last, -100.0, epsilon = 1e-9);
    }

    #[test]
    fn flat_close_yields_zero() {
        // signed_volume = 0 forever -> VP_EMA stays at 0 -> ratio = 0.
        let candles: Vec<Candle> = (0..40).map(|i| c(10.0, 100.0, i)).collect();
        let mut v = Vzo::new(5).unwrap();
        for x in v.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(x, 0.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn zero_volume_window_yields_zero() {
        // All bars carry zero volume -> tv_v == 0 -> defensive branch fires.
        let candles: Vec<Candle> = (0..20i64).map(|i| c(10.0 + i as f64, 0.0, i)).collect();
        let mut v = Vzo::new(3).unwrap();
        let out = v.batch(&candles);
        let last = out.iter().filter_map(|x| *x).next_back().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..100i64)
            .map(|i| {
                let f = i as f64;
                c(
                    100.0 + (f * 0.3).sin() * 5.0,
                    50.0 + (i % 7) as f64 * 10.0,
                    i,
                )
            })
            .collect();
        let mut a = Vzo::new(14).unwrap();
        let mut b = Vzo::new(14).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..40i64).map(|i| c(10.0 + i as f64, 100.0, i)).collect();
        let mut v = Vzo::new(5).unwrap();
        v.batch(&candles);
        assert!(v.is_ready());
        v.reset();
        assert!(!v.is_ready());
        assert_eq!(v.update(candles[0]), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Vzo::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
