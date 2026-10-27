//! Kase `DevStop` — a volatility trailing stop on the standard deviation of the
//! two-bar true range.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Output of [`KaseDevStop`]: the active trailing-stop level and the trend
/// direction it protects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KaseDevStopOutput {
    /// The `DevStop` level — below price in an uptrend, above price in a downtrend.
    pub value: f64,
    /// Trend direction: `+1.0` long (stop below price), `-1.0` short.
    pub direction: f64,
}

/// Kase `DevStop` — Cynthia Kase's volatility stop, built on the **standard
/// deviation of the two-bar true range** rather than a single-bar ATR.
///
/// ```text
/// DTR_t = max(high_t, high_{t−1}) − min(low_t, low_{t−1})   (two-bar range)
/// band  = mean(DTR, period) + dev · stddev(DTR, period)
/// long  stop = ratchet_up(  highest_high_since_flip − band )
/// short stop = ratchet_down( lowest_low_since_flip  + band )
/// ```
///
/// Kase observed that range expansion is better captured by a two-bar range than
/// a one-bar one, and that subtracting a *standard-deviation* band (not a fixed
/// ATR multiple) adapts the stop to changing volatility. The stop trails the
/// extreme reached since the last reversal — ratcheting only in the trend's favour
/// — and flips sides when price closes through it. `dev` selects which `DevStop`
/// line to follow (`1`, `2` or `3` standard deviations are Kase's warning lines).
///
/// The first bar seeds the prior candle; the next `period` two-bar ranges seed the
/// mean and standard deviation, so the first stop lands after `period + 1` inputs.
/// Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, KaseDevStop};
///
/// let mut indicator = KaseDevStop::new(30, 1.0).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     let base = 100.0 + f64::from(i);
///     let c = Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct KaseDevStop {
    period: usize,
    dev: f64,
    prev: Option<Candle>,
    window: VecDeque<f64>,
    moments: ShiftedMoments,
    direction: f64,
    extreme: f64,
    stop: f64,
    last: Option<KaseDevStopOutput>,
}

impl KaseDevStop {
    /// Construct a Kase `DevStop` with the given lookback `period` and
    /// standard-deviation multiplier `dev`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `period < 2` (a standard deviation
    /// needs at least two samples) and [`Error::NonPositiveMultiplier`] if `dev`
    /// is not finite and positive.
    pub fn new(period: usize, dev: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "Kase DevStop period must be >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !dev.is_finite() || dev <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        Ok(Self {
            period,
            dev,
            prev: None,
            window: VecDeque::with_capacity(period),
            moments: ShiftedMoments::new(),
            direction: 0.0,
            extreme: 0.0,
            stop: 0.0,
            last: None,
        })
    }

    /// Configured `(period, dev)`.
    pub const fn params(&self) -> (usize, f64) {
        (self.period, self.dev)
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<KaseDevStopOutput> {
        self.last
    }
}

impl Indicator for KaseDevStop {
    type Input = Candle;
    type Output = KaseDevStopOutput;

    fn update(&mut self, candle: Candle) -> Option<KaseDevStopOutput> {
        let Some(prev) = self.prev else {
            self.prev = Some(candle);
            return None;
        };
        let dtr = candle.high.max(prev.high) - candle.low.min(prev.low);
        self.prev = Some(candle);

        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("non-empty");
            self.moments.evict(old);
        }
        self.window.push_back(dtr);
        self.moments.push(dtr);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let mean = self.moments.mean(self.period);
        let band = mean + self.dev * self.moments.sample_variance(self.period).sqrt();

        if self.direction == 0.0 {
            // Seed the trend as long off the first fully-warmed bar.
            self.direction = 1.0;
            self.extreme = candle.high;
            self.stop = candle.high - band;
        } else if self.direction > 0.0 {
            self.extreme = self.extreme.max(candle.high);
            let raw = self.extreme - band;
            self.stop = self.stop.max(raw);
            if candle.close < self.stop {
                self.direction = -1.0;
                self.extreme = candle.low;
                self.stop = candle.low + band;
            }
        } else {
            self.extreme = self.extreme.min(candle.low);
            let raw = self.extreme + band;
            self.stop = self.stop.min(raw);
            if candle.close > self.stop {
                self.direction = 1.0;
                self.extreme = candle.high;
                self.stop = candle.high - band;
            }
        }

        let out = KaseDevStopOutput {
            value: self.stop,
            direction: self.direction,
        };
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.prev = None;
        self.window.clear();
        self.moments.reset();
        self.direction = 0.0;
        self.extreme = 0.0;
        self.stop = 0.0;
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
        "KaseDevStop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(high: f64, low: f64, close: f64) -> Candle {
        Candle::new_unchecked(f64::midpoint(high, low), high, low, close, 1_000.0, 0)
    }

    #[test]
    fn rejects_invalid_params() {
        assert!(matches!(
            KaseDevStop::new(1, 1.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            KaseDevStop::new(30, 0.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            KaseDevStop::new(30, -1.0),
            Err(Error::NonPositiveMultiplier)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let k = KaseDevStop::new(30, 1.0).unwrap();
        assert_eq!(k.params(), (30, 1.0));
        assert_eq!(k.warmup_period(), 31);
        assert_eq!(k.name(), "KaseDevStop");
        assert!(!k.is_ready());
        assert_eq!(k.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut k = KaseDevStop::new(3, 1.0).unwrap();
        let candles: Vec<Candle> = (0..8)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 1.0, base - 1.0, base)
            })
            .collect();
        let out = k.batch(&candles);
        let warmup = k.warmup_period(); // 4
        assert_eq!(warmup, 4);
        for v in out.iter().take(warmup - 1) {
            assert!(v.is_none());
        }
        assert!(out[warmup - 1].is_some());
    }

    #[test]
    fn uptrend_keeps_stop_below_price() {
        let mut k = KaseDevStop::new(5, 1.0).unwrap();
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let base = 100.0 + 2.0 * f64::from(i);
                c(base + 1.0, base - 1.0, base + 0.5)
            })
            .collect();
        for (o, candle) in k.batch(&candles).into_iter().zip(candles.iter()) {
            if let Some(o) = o {
                assert_eq!(o.direction, 1.0, "pure uptrend stays long");
                assert!(o.value < candle.close, "stop below price");
            }
        }
    }

    #[test]
    fn stop_ratchets_up_in_uptrend() {
        let mut k = KaseDevStop::new(5, 1.0).unwrap();
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let base = 100.0 + 2.0 * f64::from(i);
                c(base + 1.0, base - 1.0, base + 0.5)
            })
            .collect();
        let mut prev = f64::NEG_INFINITY;
        for o in k.batch(&candles).into_iter().flatten() {
            assert!(o.value >= prev, "long stop must not fall");
            prev = o.value;
        }
    }

    #[test]
    fn flips_on_reversal() {
        let mut candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 1.0, base - 1.0, base + 0.5)
            })
            .collect();
        candles.extend((0..40).map(|i| {
            let base = 140.0 - f64::from(i);
            c(base + 1.0, base - 1.0, base - 0.5)
        }));
        let mut k = KaseDevStop::new(5, 1.0).unwrap();
        let dirs: Vec<f64> = k
            .batch(&candles)
            .into_iter()
            .flatten()
            .map(|o| o.direction)
            .collect();
        assert!(dirs.iter().any(|&d| d > 0.0));
        assert!(dirs.iter().any(|&d| d < 0.0));
    }

    #[test]
    fn reset_clears_state() {
        let mut k = KaseDevStop::new(5, 1.0).unwrap();
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 1.0, base - 1.0, base + 0.5)
            })
            .collect();
        k.batch(&candles);
        assert!(k.is_ready());
        k.reset();
        assert!(!k.is_ready());
        assert_eq!(k.value(), None);
        assert_eq!(k.update(candles[0]), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..120)
            .map(|i| {
                let base = 100.0 + (f64::from(i) * 0.25).sin() * 9.0;
                c(base + 2.0, base - 1.5, base + 0.5)
            })
            .collect();
        let batch = KaseDevStop::new(20, 2.0).unwrap().batch(&candles);
        let mut b = KaseDevStop::new(20, 2.0).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            KaseDevStop::new(crate::error::MAX_PERIOD + 1, 1.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
