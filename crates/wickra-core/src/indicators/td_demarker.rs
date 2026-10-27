#![allow(clippy::doc_markdown)]

//! Tom DeMark DeMarker (TD DeMarker) — bounded [0, 1] oscillator built from
//! highs and lows.
//!
//! For each bar `i`:
//!
//! ```text
//! DeMax(i) = max(high[i] - high[i-1], 0)
//! DeMin(i) = max(low[i-1]  - low[i],  0)
//! ```
//!
//! Then the indicator is the simple moving average of `DeMax` over `period`
//! bars divided by the sum of the simple moving averages of `DeMax` and
//! `DeMin` over the same window:
//!
//! ```text
//! DeMarker = SMA(DeMax, period) / (SMA(DeMax, period) + SMA(DeMin, period))
//! ```
//!
//! When both averages are zero (a perfectly flat market) the indicator emits
//! the neutral midpoint `0.5`. Values above `0.7` mark overbought conditions,
//! values below `0.3` mark oversold.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// TD DeMarker bounded oscillator.
#[derive(Debug, Clone)]
pub struct TdDeMarker {
    period: usize,
    prev: Option<Candle>,
    demax: VecDeque<f64>,
    demin: VecDeque<f64>,
    last_value: Option<f64>,
}

impl TdDeMarker {
    /// Construct a TD DeMarker with the given window length.
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
            prev: None,
            demax: VecDeque::with_capacity(period),
            demin: VecDeque::with_capacity(period),
            last_value: None,
        })
    }

    /// Configured window.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Latest emitted value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }
}

impl Indicator for TdDeMarker {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let Some(prev) = self.prev else {
            self.prev = Some(candle);
            return None;
        };
        let demax = (candle.high - prev.high).max(0.0);
        let demin = (prev.low - candle.low).max(0.0);
        self.prev = Some(candle);
        if self.demax.len() == self.period {
            self.demax.pop_front();
            self.demin.pop_front();
        }
        self.demax.push_back(demax);
        self.demin.push_back(demin);
        if self.demax.len() < self.period {
            return None;
        }
        let n = self.period as f64;
        let sum_max: f64 = self.demax.iter().sum::<f64>() / n;
        let sum_min: f64 = self.demin.iter().sum::<f64>() / n;
        let denom = sum_max + sum_min;
        let v = if denom == 0.0 { 0.5 } else { sum_max / denom };
        self.last_value = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.prev = None;
        self.demax.clear();
        self.demin.clear();
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TDDeMarker"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(high: f64, low: f64, close: f64, ts: i64) -> Candle {
        Candle::new_unchecked(close, high, low, close, 0.0, ts)
    }

    #[test]
    fn flat_market_emits_neutral_05() {
        // All highs and lows equal -> DeMax == DeMin == 0 every bar -> the
        // denominator is zero and the indicator must fall back to 0.5.
        let candles: Vec<Candle> = (0..30).map(|i| c(11.0, 9.0, 10.0, i)).collect();
        let mut dm = TdDeMarker::new(14).unwrap();
        let out = dm.batch(&candles);
        for v in out.iter().skip(14).copied().flatten() {
            assert_relative_eq!(v, 0.5, epsilon = 1e-12);
        }
    }

    #[test]
    fn pure_uptrend_pegs_indicator_at_one() {
        // Every bar makes a higher high and higher low. DeMax is always
        // positive, DeMin is always zero -> indicator = 1.
        let candles: Vec<Candle> = (0..20)
            .map(|i: i32| {
                c(
                    11.0 + f64::from(i),
                    9.0 + f64::from(i),
                    10.0 + f64::from(i),
                    i64::from(i),
                )
            })
            .collect();
        let mut dm = TdDeMarker::new(5).unwrap();
        let out = dm.batch(&candles);
        for v in out.iter().skip(6).copied().flatten() {
            assert_relative_eq!(v, 1.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn pure_downtrend_pegs_indicator_at_zero() {
        let candles: Vec<Candle> = (0..20)
            .map(|i: i32| {
                c(
                    11.0 - f64::from(i),
                    9.0 - f64::from(i),
                    10.0 - f64::from(i),
                    i64::from(i),
                )
            })
            .collect();
        let mut dm = TdDeMarker::new(5).unwrap();
        let out = dm.batch(&candles);
        for v in out.iter().skip(6).copied().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn stays_in_unit_interval() {
        let candles: Vec<Candle> = (0..200)
            .map(|i| {
                let m = 50.0 + (f64::from(i) * 0.2).sin() * 5.0;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut dm = TdDeMarker::new(14).unwrap();
        for v in dm.batch(&candles).into_iter().flatten() {
            assert!((0.0..=1.0).contains(&v), "out of range: {v}");
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
                c(m + 1.0, m - 1.0, m, i64::from(i))
            })
            .collect();
        let mut a = TdDeMarker::new(14).unwrap();
        let mut b = TdDeMarker::new(14).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(TdDeMarker::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..30)
            .map(|i: i32| {
                c(
                    11.0 + f64::from(i),
                    9.0 + f64::from(i),
                    10.0 + f64::from(i),
                    i64::from(i),
                )
            })
            .collect();
        let mut dm = TdDeMarker::new(14).unwrap();
        dm.batch(&candles);
        assert!(dm.is_ready());
        dm.reset();
        assert!(!dm.is_ready());
        assert_eq!(dm.update(candles[0]), None);
        assert_eq!(dm.value(), None);
    }

    #[test]
    fn accessors_and_metadata() {
        let dm = TdDeMarker::new(14).unwrap();
        assert_eq!(dm.period(), 14);
        assert_eq!(dm.warmup_period(), 15);
        assert_eq!(dm.name(), "TDDeMarker");
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            TdDeMarker::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
