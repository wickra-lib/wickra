//! Modified-MA Stop — a trailing stop riding the Modified Moving Average (SMMA).

use crate::error::{Error, Result};
use crate::indicators::smma::Smma;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Output of [`ModifiedMaStop`]: the active stop level and the trend direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModifiedMaStopOutput {
    /// The stop level (a directionally-ratcheted Modified Moving Average).
    pub value: f64,
    /// Trend direction: `+1.0` long (stop below price), `-1.0` short.
    pub direction: f64,
}

/// Modified-MA Stop — a trailing stop whose line is the **Modified Moving
/// Average** (SMMA / Wilder's RMA) of price, allowed to move only in the trend's
/// favour.
///
/// ```text
/// ma = SMMA(close, period)                 (Modified Moving Average)
/// long:  stop = max(prev_stop, ma);  flip short when close < stop
/// short: stop = min(prev_stop, ma);  flip long  when close > stop
/// ```
///
/// The Modified Moving Average (also called the smoothed or running moving
/// average) is the slow, low-lag average Wilder used throughout his systems. Using
/// it directly as a trailing line — but **ratcheting** so the long stop never
/// falls and the short stop never rises — turns the smooth average into a stop
/// that hugs price in a trend and flips when price decisively crosses it. Because
/// the SMMA lags, the stop gives trends room while still exiting clean reversals.
///
/// The first stop lands once the SMMA is ready (`period` inputs). Each `update` is
/// O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, ModifiedMaStop};
///
/// let mut indicator = ModifiedMaStop::new(14).unwrap();
/// let mut last = None;
/// for i in 0..60 {
///     let base = 100.0 + f64::from(i);
///     let c = Candle::new(base, base + 1.0, base - 1.0, base + 0.5, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct ModifiedMaStop {
    smma: Smma,
    period: usize,
    direction: f64,
    stop: f64,
    last: Option<ModifiedMaStopOutput>,
}

impl ModifiedMaStop {
    /// Construct a Modified-MA stop with the given SMMA `period`.
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
            smma: Smma::new(period)?,
            period,
            direction: 0.0,
            stop: 0.0,
            last: None,
        })
    }

    /// Configured SMMA period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<ModifiedMaStopOutput> {
        self.last
    }
}

impl Indicator for ModifiedMaStop {
    type Input = Candle;
    type Output = ModifiedMaStopOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<ModifiedMaStopOutput> {
        let ma = self.smma.update(candle.close)?;
        let close = candle.close;

        if self.direction == 0.0 {
            self.direction = if close >= ma { 1.0 } else { -1.0 };
            self.stop = ma;
        } else if self.direction > 0.0 {
            self.stop = self.stop.max(ma);
            if close < self.stop {
                self.direction = -1.0;
                self.stop = ma;
            }
        } else {
            self.stop = self.stop.min(ma);
            if close > self.stop {
                self.direction = 1.0;
                self.stop = ma;
            }
        }

        let out = ModifiedMaStopOutput {
            value: self.stop,
            direction: self.direction,
        };
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.smma.reset();
        self.direction = 0.0;
        self.stop = 0.0;
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
        "ModifiedMaStop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(close: f64) -> Candle {
        Candle::new_unchecked(close, close + 1.0, close - 1.0, close, 1_000.0, 0)
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(ModifiedMaStop::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let m = ModifiedMaStop::new(14).unwrap();
        assert_eq!(m.period(), 14);
        assert_eq!(m.warmup_period(), 14);
        assert_eq!(m.name(), "ModifiedMaStop");
        assert!(!m.is_ready());
        assert_eq!(m.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut m = ModifiedMaStop::new(5).unwrap();
        let candles: Vec<Candle> = (0..12).map(|i| c(100.0 + f64::from(i))).collect();
        let out = m.batch(&candles);
        for v in out.iter().take(4) {
            assert!(v.is_none());
        }
        assert!(out[4].is_some());
    }

    #[test]
    fn uptrend_keeps_stop_below_price() {
        let mut m = ModifiedMaStop::new(5).unwrap();
        let candles: Vec<Candle> = (0..60).map(|i| c(100.0 + 2.0 * f64::from(i))).collect();
        for (o, candle) in m.batch(&candles).into_iter().zip(candles.iter()) {
            if let Some(o) = o {
                assert_eq!(o.direction, 1.0);
                assert!(o.value < candle.close);
            }
        }
    }

    #[test]
    fn long_stop_ratchets_up() {
        let mut m = ModifiedMaStop::new(5).unwrap();
        let candles: Vec<Candle> = (0..60).map(|i| c(100.0 + 2.0 * f64::from(i))).collect();
        let mut prev = f64::NEG_INFINITY;
        for o in m.batch(&candles).into_iter().flatten() {
            assert_eq!(o.direction, 1.0, "pure uptrend stays long");
            assert!(o.value >= prev, "long stop must not fall");
            prev = o.value;
        }
    }

    #[test]
    fn flips_on_reversal() {
        let mut candles: Vec<Candle> = (0..40).map(|i| c(100.0 + f64::from(i))).collect();
        candles.extend((0..40).map(|i| c(140.0 - f64::from(i))));
        let mut m = ModifiedMaStop::new(5).unwrap();
        let dirs: Vec<f64> = m
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
        let mut m = ModifiedMaStop::new(5).unwrap();
        m.batch(&(0..40).map(|i| c(100.0 + f64::from(i))).collect::<Vec<_>>());
        assert!(m.is_ready());
        m.reset();
        assert!(!m.is_ready());
        assert_eq!(m.value(), None);
        assert_eq!(m.update(c(100.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..120)
            .map(|i| c(100.0 + (f64::from(i) * 0.25).sin() * 9.0))
            .collect();
        let batch = ModifiedMaStop::new(14).unwrap().batch(&candles);
        let mut b = ModifiedMaStop::new(14).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            ModifiedMaStop::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
