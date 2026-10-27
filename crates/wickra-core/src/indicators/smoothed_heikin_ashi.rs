//! Smoothed Heikin-Ashi — Heikin-Ashi computed on EMA-smoothed OHLC.

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// One smoothed Heikin-Ashi candle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothedHeikinAshiOutput {
    /// Smoothed Heikin-Ashi open.
    pub open: f64,
    /// Smoothed Heikin-Ashi high.
    pub high: f64,
    /// Smoothed Heikin-Ashi low.
    pub low: f64,
    /// Smoothed Heikin-Ashi close.
    pub close: f64,
}

/// Smoothed Heikin-Ashi — the [`HeikinAshi`](crate::HeikinAshi) transform applied
/// to **EMA-smoothed** OHLC, for an even cleaner trend view.
///
/// ```text
/// eo, eh, el, ec = EMA(open|high|low|close, period)
/// ha_close = (eo + eh + el + ec) / 4
/// ha_open  = (prev_ha_open + prev_ha_close) / 2     (seeded with (eo + ec)/2)
/// ha_high  = max(eh, ha_open, ha_close)
/// ha_low   = min(el, ha_open, ha_close)
/// ```
///
/// Standard Heikin-Ashi already averages the OHLC; smoothing each input series
/// with an EMA *before* the transform removes still more noise, producing long,
/// uninterrupted runs of same-colour candles in a trend and crisp colour flips at
/// turns. The trade-off is added lag proportional to `period`. The output uses the
/// same OHLC field layout as a candle so it can be charted directly.
///
/// The first value lands once the EMAs are seeded (`period` inputs). Each `update`
/// is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, SmoothedHeikinAshi};
///
/// let mut indicator = SmoothedHeikinAshi::new(10).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let c = Candle::new(base, base + 1.0, base - 1.0, base + 0.5, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct SmoothedHeikinAshi {
    period: usize,
    ema_open: Ema,
    ema_high: Ema,
    ema_low: Ema,
    ema_close: Ema,
    prev: Option<SmoothedHeikinAshiOutput>,
    last: Option<SmoothedHeikinAshiOutput>,
}

impl SmoothedHeikinAshi {
    /// Construct a smoothed Heikin-Ashi with the given EMA `period`.
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
            ema_open: Ema::new(period)?,
            ema_high: Ema::new(period)?,
            ema_low: Ema::new(period)?,
            ema_close: Ema::new(period)?,
            prev: None,
            last: None,
        })
    }

    /// Configured smoothing period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<SmoothedHeikinAshiOutput> {
        self.last
    }
}

impl Indicator for SmoothedHeikinAshi {
    type Input = Candle;
    type Output = SmoothedHeikinAshiOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<SmoothedHeikinAshiOutput> {
        let eo = self.ema_open.update(candle.open);
        let eh = self.ema_high.update(candle.high);
        let el = self.ema_low.update(candle.low);
        let ec = self.ema_close.update(candle.close);
        let (Some(eo), Some(eh), Some(el), Some(ec)) = (eo, eh, el, ec) else {
            return None;
        };
        let ha_close = (eo + eh + el + ec) / 4.0;
        let ha_open = match self.prev {
            Some(p) => f64::midpoint(p.open, p.close),
            None => f64::midpoint(eo, ec),
        };
        let ha_high = eh.max(ha_open).max(ha_close);
        let ha_low = el.min(ha_open).min(ha_close);
        let out = SmoothedHeikinAshiOutput {
            open: ha_open,
            high: ha_high,
            low: ha_low,
            close: ha_close,
        };
        self.prev = Some(out);
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.ema_open.reset();
        self.ema_high.reset();
        self.ema_low.reset();
        self.ema_close.reset();
        self.prev = None;
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
        "SmoothedHeikinAshi"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(open: f64, high: f64, low: f64, close: f64) -> Candle {
        Candle::new_unchecked(open, high, low, close, 1_000.0, 0)
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(SmoothedHeikinAshi::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let s = SmoothedHeikinAshi::new(10).unwrap();
        assert_eq!(s.period(), 10);
        assert_eq!(s.warmup_period(), 10);
        assert_eq!(s.name(), "SmoothedHeikinAshi");
        assert!(!s.is_ready());
        assert_eq!(s.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut s = SmoothedHeikinAshi::new(3).unwrap();
        let candles: Vec<Candle> = (0..6)
            .map(|i| {
                let b = 100.0 + f64::from(i);
                c(b, b + 1.0, b - 1.0, b + 0.5)
            })
            .collect();
        let out = s.batch(&candles);
        for v in out.iter().take(2) {
            assert!(v.is_none());
        }
        assert!(out[2].is_some());
    }

    #[test]
    fn high_brackets_open_close() {
        let mut s = SmoothedHeikinAshi::new(3).unwrap();
        let candles: Vec<Candle> = (0..30)
            .map(|i| {
                let b = 100.0 + f64::from(i);
                c(b, b + 2.0, b - 2.0, b + 0.5)
            })
            .collect();
        for o in s.batch(&candles).into_iter().flatten() {
            assert!(o.high >= o.open && o.high >= o.close);
            assert!(o.low <= o.open && o.low <= o.close);
        }
    }

    #[test]
    fn uptrend_close_above_open() {
        let mut s = SmoothedHeikinAshi::new(3).unwrap();
        let candles: Vec<Candle> = (0..30)
            .map(|i| {
                let b = 100.0 + 2.0 * f64::from(i);
                c(b, b + 1.0, b - 1.0, b + 0.5)
            })
            .collect();
        let o = s.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(
            o.close > o.open,
            "an uptrend should print a bullish smoothed HA candle"
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut s = SmoothedHeikinAshi::new(3).unwrap();
        s.batch(
            &(0..10)
                .map(|i| {
                    let b = 100.0 + f64::from(i);
                    c(b, b + 1.0, b - 1.0, b)
                })
                .collect::<Vec<_>>(),
        );
        assert!(s.is_ready());
        s.reset();
        assert!(!s.is_ready());
        assert_eq!(s.value(), None);
        assert_eq!(s.update(c(100.0, 101.0, 99.0, 100.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let b = 100.0 + (f64::from(i) * 0.25).sin() * 9.0;
                c(b, b + 1.0, b - 1.0, b + 0.3)
            })
            .collect();
        let batch = SmoothedHeikinAshi::new(10).unwrap().batch(&candles);
        let mut b = SmoothedHeikinAshi::new(10).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            SmoothedHeikinAshi::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
