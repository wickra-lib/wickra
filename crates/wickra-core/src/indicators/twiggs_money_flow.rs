//! Twiggs Money Flow (TMF) — Colin Twiggs' Wilder-smoothed money-flow oscillator.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Twiggs Money Flow — a refinement of Chaikin Money Flow that uses **true range**
/// boundaries and **Wilder (exponential) smoothing** instead of a simple sum.
///
/// ```text
/// TRH   = max(high, prev_close)          (true high)
/// TRL   = min(low,  prev_close)          (true low)
/// ad    = volume * (2*close − TRH − TRL) / (TRH − TRL)   (0 if TRH == TRL)
/// TMF   = WilderEMA(ad, period) / WilderEMA(volume, period)
/// ```
///
/// Colin Twiggs' money flow fixes two issues with [`ChaikinMoneyFlow`](crate::ChaikinMoneyFlow): it replaces
/// the bar's raw high/low with the *true* high/low (folding in the prior close so
/// gaps count), and it smooths the accumulated money flow and the volume with a
/// Wilder exponential average rather than a flat `period`-sum, so the oscillator
/// reacts faster and never jumps when a large bar drops out of a window. The
/// output is bounded in roughly `[−1, +1]`: positive means buying pressure
/// (closes biased toward the true high), negative means selling pressure.
///
/// The first candle seeds the reference close; the next `period` bars seed both
/// Wilder averages, so the first value lands after `period + 1` inputs. A stretch
/// of zero volume makes the denominator average `0`, in which case the oscillator
/// reports `0` rather than `0 / 0`. Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, TwiggsMoneyFlow};
///
/// let mut indicator = TwiggsMoneyFlow::new(21).unwrap();
/// let mut last = None;
/// for i in 0..60 {
///     let base = 100.0 + (f64::from(i) * 0.2).sin() * 5.0;
///     let c = Candle::new(base, base + 1.0, base - 1.0, base + 0.5, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct TwiggsMoneyFlow {
    period: usize,
    prev_close: Option<f64>,
    seed_ad: f64,
    seed_vol: f64,
    seed_count: usize,
    ad_ema: Option<f64>,
    vol_ema: Option<f64>,
    last: Option<f64>,
}

impl TwiggsMoneyFlow {
    /// Construct a new Twiggs Money Flow with the given smoothing `period`.
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
            prev_close: None,
            seed_ad: 0.0,
            seed_vol: 0.0,
            seed_count: 0,
            ad_ema: None,
            vol_ema: None,
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

    fn ratio(ad_ema: f64, vol_ema: f64) -> f64 {
        if vol_ema == 0.0 {
            0.0
        } else {
            ad_ema / vol_ema
        }
    }
}

impl Indicator for TwiggsMoneyFlow {
    type Input = Candle;
    type Output = f64;

    fn update(&mut self, candle: Candle) -> Option<f64> {
        let Some(prev_close) = self.prev_close else {
            self.prev_close = Some(candle.close);
            return None;
        };
        let trh = candle.high.max(prev_close);
        let trl = candle.low.min(prev_close);
        let range = trh - trl;
        let ad = if range > 0.0 {
            candle.volume * (2.0 * candle.close - trh - trl) / range
        } else {
            0.0
        };
        self.prev_close = Some(candle.close);

        if let (Some(ad_ema), Some(vol_ema)) = (self.ad_ema, self.vol_ema) {
            let n = self.period as f64;
            let new_ad = ad_ema + (ad - ad_ema) / n;
            let new_vol = vol_ema + (candle.volume - vol_ema) / n;
            self.ad_ema = Some(new_ad);
            self.vol_ema = Some(new_vol);
            let v = Self::ratio(new_ad, new_vol);
            self.last = Some(v);
            return Some(v);
        }

        self.seed_ad += ad;
        self.seed_vol += candle.volume;
        self.seed_count += 1;
        if self.seed_count == self.period {
            let n = self.period as f64;
            let ad_ema = self.seed_ad / n;
            let vol_ema = self.seed_vol / n;
            self.ad_ema = Some(ad_ema);
            self.vol_ema = Some(vol_ema);
            let v = Self::ratio(ad_ema, vol_ema);
            self.last = Some(v);
            return Some(v);
        }
        None
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.seed_ad = 0.0;
        self.seed_vol = 0.0;
        self.seed_count = 0;
        self.ad_ema = None;
        self.vol_ema = None;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TwiggsMoneyFlow"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn candle(high: f64, low: f64, close: f64, volume: f64) -> Candle {
        Candle::new_unchecked(low, high, low, close, volume, 0)
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(TwiggsMoneyFlow::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn flat_bars_drive_tmf_to_zero() {
        // A flat bar (high == low == close == prior close) gives a zero two-bar
        // range, so the accumulation term falls back to 0.0 and TMF settles at
        // zero. Exercises the `range == 0` guard.
        let mut tmf = TwiggsMoneyFlow::new(2).unwrap();
        let flat: Vec<Candle> = (0..6)
            .map(|_| candle(100.0, 100.0, 100.0, 1_000.0))
            .collect();
        let last = tmf.batch(&flat).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn accessors_and_metadata() {
        let tmf = TwiggsMoneyFlow::new(21).unwrap();
        assert_eq!(tmf.period(), 21);
        assert_eq!(tmf.warmup_period(), 22);
        assert_eq!(tmf.name(), "TwiggsMoneyFlow");
        assert!(!tmf.is_ready());
        assert_eq!(tmf.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut tmf = TwiggsMoneyFlow::new(3).unwrap();
        let candles: Vec<Candle> = (0..8)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                candle(base + 1.0, base - 1.0, base, 1_000.0)
            })
            .collect();
        let out = tmf.batch(&candles);
        // warmup_period == period + 1 == 4: first emission at index 3.
        for o in out.iter().take(3) {
            assert!(o.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn closes_at_true_high_is_positive() {
        // Every bar closes at its high -> strong buying pressure -> TMF -> +1.
        let mut tmf = TwiggsMoneyFlow::new(3).unwrap();
        let candles: Vec<Candle> = (0..12)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                // open=low=base-1, high=close=base+1 -> closes at the top.
                Candle::new_unchecked(base - 1.0, base + 1.0, base - 1.0, base + 1.0, 1_000.0, 0)
            })
            .collect();
        let last = tmf.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(
            last > 0.9,
            "closing at the high should drive TMF near +1, got {last}"
        );
    }

    #[test]
    fn closes_at_true_low_is_negative() {
        let mut tmf = TwiggsMoneyFlow::new(3).unwrap();
        let candles: Vec<Candle> = (0..12)
            .map(|i| {
                let base = 100.0 - f64::from(i);
                // closes at the low.
                Candle::new_unchecked(base + 1.0, base + 1.0, base - 1.0, base - 1.0, 1_000.0, 0)
            })
            .collect();
        let last = tmf.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(
            last < -0.5,
            "closing at the low should drive TMF negative, got {last}"
        );
    }

    #[test]
    fn zero_volume_yields_zero() {
        let mut tmf = TwiggsMoneyFlow::new(3).unwrap();
        let candles: Vec<Candle> = (0..10)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                candle(base + 1.0, base - 1.0, base, 0.0)
            })
            .collect();
        for v in tmf.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn output_in_range() {
        let mut tmf = TwiggsMoneyFlow::new(21).unwrap();
        let candles: Vec<Candle> = (0..200)
            .map(|i| {
                let base = 100.0 + (f64::from(i) * 0.3).sin() * 12.0;
                candle(base + 2.0, base - 2.0, base + 0.5, 1_000.0)
            })
            .collect();
        for v in tmf.batch(&candles).into_iter().flatten() {
            assert!((-1.0..=1.0).contains(&v), "TMF out of range: {v}");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut tmf = TwiggsMoneyFlow::new(3).unwrap();
        let candles: Vec<Candle> = (0..12)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                candle(base + 1.0, base - 1.0, base, 1_000.0)
            })
            .collect();
        tmf.batch(&candles);
        assert!(tmf.is_ready());
        tmf.reset();
        assert!(!tmf.is_ready());
        assert_eq!(tmf.value(), None);
        assert_eq!(tmf.update(candle(101.0, 99.0, 100.0, 1_000.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..120)
            .map(|i| {
                let base = 100.0 + (f64::from(i) * 0.25).sin() * 9.0;
                candle(base + 2.0, base - 1.5, base + 0.5, 1_000.0 + f64::from(i))
            })
            .collect();
        let batch = TwiggsMoneyFlow::new(21).unwrap().batch(&candles);
        let mut b = TwiggsMoneyFlow::new(21).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            TwiggsMoneyFlow::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
