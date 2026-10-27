#![allow(clippy::doc_markdown)]

//! Tom DeMark TD Pressure — volume-weighted buying / selling pressure
//! oscillator.
//!
//! For each bar `i` with strictly positive range:
//!
//! ```text
//! bar_pressure(i) = ((close[i] - open[i]) / (high[i] - low[i])) * volume[i]
//! ```
//!
//! Bars whose range is zero (`high == low`) contribute zero pressure (the
//! ratio is undefined; DeMark's convention is to treat such bars as neutral).
//! The output is the SMA of bar pressure normalised by the SMA of volume over
//! a configurable `period`, scaled by 100:
//!
//! ```text
//! TD_Pressure = 100 * SMA(bar_pressure, period) / SMA(volume, period)
//! ```
//!
//! When the windowed volume is zero (a flat zero-volume window) the
//! indicator emits `0`. Positive readings indicate net buying pressure;
//! negative readings indicate net selling pressure. The numerator is bounded
//! by `± volume_per_bar`, so the result is bounded by `±100`.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// TD Pressure volume-weighted pressure oscillator.
#[derive(Debug, Clone)]
pub struct TdPressure {
    period: usize,
    pressures: VecDeque<f64>,
    volumes: VecDeque<f64>,
    last_value: Option<f64>,
}

impl TdPressure {
    /// Construct a TD Pressure with the given averaging window. A common
    /// default in DeMark's literature is `period = 5`.
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
            pressures: VecDeque::with_capacity(period),
            volumes: VecDeque::with_capacity(period),
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

impl Indicator for TdPressure {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let range = candle.high - candle.low;
        let bar_pressure = if range > 0.0 {
            ((candle.close - candle.open) / range) * candle.volume
        } else {
            0.0
        };

        if self.pressures.len() == self.period {
            self.pressures.pop_front();
            self.volumes.pop_front();
        }
        self.pressures.push_back(bar_pressure);
        self.volumes.push_back(candle.volume);
        if self.pressures.len() < self.period {
            return None;
        }
        let n = self.period as f64;
        let mean_p: f64 = self.pressures.iter().sum::<f64>() / n;
        let mean_v: f64 = self.volumes.iter().sum::<f64>() / n;
        let v = if mean_v == 0.0 {
            0.0
        } else {
            100.0 * mean_p / mean_v
        };
        self.last_value = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.pressures.clear();
        self.volumes.clear();
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TDPressure"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(open: f64, high: f64, low: f64, close: f64, volume: f64, ts: i64) -> Candle {
        Candle::new_unchecked(open, high, low, close, volume, ts)
    }

    #[test]
    fn pure_bullish_candles_yield_full_positive_pressure() {
        // Every bar closes at its high (close == high, open == low), so the
        // per-bar pressure ratio is +1. Volume cancels in the ratio and the
        // indicator must read +100.
        let candles: Vec<Candle> = (0..20)
            .map(|i| c(9.0, 11.0, 9.0, 11.0, 100.0, i64::from(i)))
            .collect();
        let mut p = TdPressure::new(5).unwrap();
        let last = p.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 100.0, epsilon = 1e-12);
    }

    #[test]
    fn pure_bearish_candles_yield_full_negative_pressure() {
        let candles: Vec<Candle> = (0..20)
            .map(|i| c(11.0, 11.0, 9.0, 9.0, 100.0, i64::from(i)))
            .collect();
        let mut p = TdPressure::new(5).unwrap();
        let last = p.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, -100.0, epsilon = 1e-12);
    }

    #[test]
    fn neutral_doji_close_eq_open_yields_zero() {
        let candles: Vec<Candle> = (0..20)
            .map(|i| c(10.0, 11.0, 9.0, 10.0, 100.0, i64::from(i)))
            .collect();
        let mut p = TdPressure::new(5).unwrap();
        let last = p.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn zero_range_bars_contribute_zero() {
        // Mix one zero-range bar with otherwise-bullish bars; the zero-range
        // bar must be silently skipped (not produce NaN or inf).
        let mut candles = Vec::new();
        for i in 0..5 {
            candles.push(c(9.0, 11.0, 9.0, 11.0, 100.0, i64::from(i)));
        }
        // Zero-range, zero-volume bar in the middle.
        candles.push(c(10.0, 10.0, 10.0, 10.0, 0.0, 5));
        for i in 6..11 {
            candles.push(c(9.0, 11.0, 9.0, 11.0, 100.0, i64::from(i)));
        }
        let mut p = TdPressure::new(5).unwrap();
        for v in p.batch(&candles).into_iter().flatten() {
            assert!(v.is_finite(), "non-finite output: {v}");
            assert!((-100.0..=100.0).contains(&v), "out of range: {v}");
        }
    }

    #[test]
    fn flat_zero_volume_window_emits_zero() {
        let candles: Vec<Candle> = (0..10)
            .map(|i| c(10.0, 11.0, 9.0, 10.5, 0.0, i64::from(i)))
            .collect();
        let mut p = TdPressure::new(5).unwrap();
        // Every bar has zero volume -> per-bar pressure is zero AND the
        // denominator is zero. The indicator must fall back to 0.
        let last = p.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
                c(m, m + 1.0, m - 1.0, m + 0.3, 100.0, i64::from(i))
            })
            .collect();
        let mut a = TdPressure::new(5).unwrap();
        let mut b = TdPressure::new(5).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(TdPressure::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..20)
            .map(|i| c(9.0, 11.0, 9.0, 11.0, 100.0, i64::from(i)))
            .collect();
        let mut p = TdPressure::new(5).unwrap();
        p.batch(&candles);
        assert!(p.is_ready());
        p.reset();
        assert!(!p.is_ready());
        assert_eq!(p.update(candles[0]), None);
        assert_eq!(p.value(), None);
    }

    #[test]
    fn accessors_and_metadata() {
        let p = TdPressure::new(5).unwrap();
        assert_eq!(p.period(), 5);
        assert_eq!(p.warmup_period(), 5);
        assert_eq!(p.name(), "TDPressure");
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            TdPressure::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
