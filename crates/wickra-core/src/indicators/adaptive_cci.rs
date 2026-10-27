//! Adaptive CCI — a CCI whose centre line adapts to the efficiency ratio.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Adaptive CCI — Lambert's Commodity Channel Index whose centre line is an
/// **efficiency-ratio-adaptive** moving average of typical price instead of a
/// plain SMA, so it leads in trends and stays calm in chop.
///
/// ```text
/// TP   = (high + low + close) / 3
/// ER   = |TP_t − TP_oldest| / Σ |ΔTP| over the window      (0..1)
/// sc   = ( ER·(2/3 − 2/31) + 2/31 )²
/// mean += sc·(TP_t − mean)                                  (adaptive centre, seeded with SMA)
/// MD   = mean(|TP_i − mean|) over the window               (mean deviation)
/// CCI  = (TP_t − mean) / (0.015 · MD)
/// ```
///
/// The classic [`Cci`](crate::Cci) centres typical price on its simple moving
/// average; the lag of that SMA delays the oscillator in fast moves. Replacing it
/// with a KAMA-style adaptive average — driven by Kaufman's efficiency ratio —
/// lets the centre line accelerate toward price in a clean trend (so the CCI
/// reaches its `±100` bands sooner) and slow down in noise (fewer false pokes).
/// The `0.015` scaling keeps Lambert's convention that roughly 70–80% of readings
/// fall in `[−100, +100]`.
///
/// The output is unbounded around `0`; a flat window (zero mean deviation) returns
/// `0`. The first value lands after `period` inputs; each `update` is O(`period`).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, AdaptiveCci};
///
/// let mut indicator = AdaptiveCci::new(20).unwrap();
/// let mut last = None;
/// for i in 0..60 {
///     let base = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
///     let c = Candle::new(base, base + 1.0, base - 1.0, base, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct AdaptiveCci {
    period: usize,
    window: VecDeque<f64>,
    mean: Option<f64>,
    last: Option<f64>,
}

impl AdaptiveCci {
    /// Construct an adaptive CCI with the given `period`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0` and
    /// [`Error::InvalidPeriod`] if `period < 2` (the efficiency ratio needs a
    /// path of at least one step).
    pub fn new(period: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "adaptive CCI needs period >= 2",
            });
        }
        Ok(Self {
            period,
            window: VecDeque::with_capacity(period),
            mean: None,
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

impl Indicator for AdaptiveCci {
    type Input = Candle;
    type Output = f64;

    fn update(&mut self, candle: Candle) -> Option<f64> {
        let tp = candle.typical_price();
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(tp);
        if self.window.len() < self.period {
            return None;
        }
        let n = self.period as f64;

        // Efficiency ratio over the window.
        let oldest = self.window[0];
        let direction = (tp - oldest).abs();
        let mut path = 0.0;
        for pair in self.window.iter().collect::<Vec<_>>().windows(2) {
            path += (pair[1] - pair[0]).abs();
        }
        let er = if path > 0.0 {
            (direction / path).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fast = 2.0 / 3.0;
        let slow = 2.0 / 31.0;
        let sc = (er * (fast - slow) + slow).powi(2);

        let mean = match self.mean {
            None => self.window.iter().sum::<f64>() / n,
            Some(prev) => prev + sc * (tp - prev),
        };
        self.mean = Some(mean);

        let md = self.window.iter().map(|&v| (v - mean).abs()).sum::<f64>() / n;
        let cci = if md > 0.0 {
            (tp - mean) / (0.015 * md)
        } else {
            0.0
        };
        self.last = Some(cci);
        Some(cci)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.mean = None;
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
        "AdaptiveCci"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn candle(tp: f64) -> Candle {
        // open=high=low=close=tp -> typical price == tp.
        Candle::new_unchecked(tp, tp, tp, tp, 1_000.0, 0)
    }

    #[test]
    fn rejects_invalid_period() {
        assert!(matches!(AdaptiveCci::new(0), Err(Error::PeriodZero)));
        assert!(matches!(
            AdaptiveCci::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let c = AdaptiveCci::new(20).unwrap();
        assert_eq!(c.period(), 20);
        assert_eq!(c.warmup_period(), 20);
        assert_eq!(c.name(), "AdaptiveCci");
        assert!(!c.is_ready());
        assert_eq!(c.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut c = AdaptiveCci::new(4).unwrap();
        let candles: Vec<Candle> = (0..6).map(|i| candle(100.0 + f64::from(i))).collect();
        let out = c.batch(&candles);
        for v in out.iter().take(3) {
            assert!(v.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn uptrend_is_positive() {
        let mut c = AdaptiveCci::new(10).unwrap();
        let candles: Vec<Candle> = (0..40).map(|i| candle(100.0 + f64::from(i))).collect();
        let last = c.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(last > 0.0, "uptrend should give positive CCI, got {last}");
    }

    #[test]
    fn downtrend_is_negative() {
        let mut c = AdaptiveCci::new(10).unwrap();
        let candles: Vec<Candle> = (0..40).map(|i| candle(200.0 - f64::from(i))).collect();
        let last = c.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(last < 0.0, "downtrend should give negative CCI, got {last}");
    }

    #[test]
    fn flat_window_is_zero() {
        let mut c = AdaptiveCci::new(5).unwrap();
        let candles: Vec<Candle> = (0..10).map(|_| candle(100.0)).collect();
        for v in c.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut c = AdaptiveCci::new(5).unwrap();
        let candles: Vec<Candle> = (0..20).map(|i| candle(100.0 + f64::from(i))).collect();
        c.batch(&candles);
        assert!(c.is_ready());
        c.reset();
        assert!(!c.is_ready());
        assert_eq!(c.value(), None);
        assert_eq!(c.update(candle(100.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..120)
            .map(|i| candle(100.0 + (f64::from(i) * 0.25).sin() * 9.0))
            .collect();
        let batch = AdaptiveCci::new(20).unwrap().batch(&candles);
        let mut b = AdaptiveCci::new(20).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            AdaptiveCci::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
