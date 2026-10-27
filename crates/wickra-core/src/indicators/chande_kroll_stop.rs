//! Chande Kroll Stop.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::atr::Atr;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Chande Kroll Stop output: the long-side and short-side stop levels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChandeKrollStopOutput {
    /// Long-position stop — the lowest preliminary low-stop over `stop_period`.
    pub stop_long: f64,
    /// Short-position stop — the highest preliminary high-stop over `stop_period`.
    pub stop_short: f64,
}

/// Chande Kroll Stop — Tushar Chande and Stanley Kroll's two-stage ATR stop.
///
/// ```text
/// preliminary (window p = atr_period, x = atr_multiplier):
///   high_stop = highest_high(p) − x · ATR(p)
///   low_stop  = lowest_low(p)   + x · ATR(p)
///
/// final (window q = stop_period):
///   stop_short = highest(high_stop, q)
///   stop_long  = lowest(low_stop,  q)
/// ```
///
/// The first stage builds an ATR stop off the recent extreme, exactly like a
/// [`ChandelierExit`](crate::ChandelierExit); the second stage smooths it by
/// taking the most extreme preliminary stop over a shorter window, which keeps
/// the stop from whipsawing on a single wide bar. The classic configuration
/// from *The New Technical Trader* is `ATR(10)`, multiplier `1.0`, smoothing
/// window `9`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, ChandeKrollStop};
///
/// let mut indicator = ChandeKrollStop::new(10, 1.0, 9).unwrap();
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
pub struct ChandeKrollStop {
    atr_period: usize,
    atr_multiplier: f64,
    stop_period: usize,
    atr: Atr,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
    high_stops: VecDeque<f64>,
    low_stops: VecDeque<f64>,
}

impl ChandeKrollStop {
    /// Construct a Chande Kroll Stop with explicit ATR and smoothing windows.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `atr_period` or `stop_period` is zero,
    /// and [`Error::NonPositiveMultiplier`] if `atr_multiplier` is not strictly
    /// positive and finite.
    pub fn new(atr_period: usize, atr_multiplier: f64, stop_period: usize) -> Result<Self> {
        if !atr_multiplier.is_finite() || atr_multiplier <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        if stop_period == 0 {
            return Err(Error::PeriodZero);
        }
        if stop_period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            atr_period,
            atr_multiplier,
            stop_period,
            atr: Atr::new(atr_period)?,
            highs: VecDeque::with_capacity(atr_period),
            lows: VecDeque::with_capacity(atr_period),
            high_stops: VecDeque::with_capacity(stop_period),
            low_stops: VecDeque::with_capacity(stop_period),
        })
    }

    /// The classic configuration: `ATR(10)`, multiplier `1.0`, window `9`.
    pub fn classic() -> Self {
        Self::new(10, 1.0, 9).expect("classic Chande Kroll Stop params are valid")
    }

    /// Configured `(atr_period, atr_multiplier, stop_period)`.
    pub const fn params(&self) -> (usize, f64, usize) {
        (self.atr_period, self.atr_multiplier, self.stop_period)
    }
}

impl Indicator for ChandeKrollStop {
    type Input = Candle;
    type Output = ChandeKrollStopOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<ChandeKrollStopOutput> {
        let atr = self.atr.update(candle);
        if self.highs.len() == self.atr_period {
            self.highs.pop_front();
            self.lows.pop_front();
        }
        self.highs.push_back(candle.high);
        self.lows.push_back(candle.low);
        if self.highs.len() < self.atr_period {
            return None;
        }
        // ATR(atr_period) becomes ready on exactly the candle that fills the
        // preliminary window, so this never discards a value.
        let atr = atr?;
        let highest = self.highs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let lowest = self.lows.iter().copied().fold(f64::INFINITY, f64::min);
        let high_stop = highest - self.atr_multiplier * atr;
        let low_stop = lowest + self.atr_multiplier * atr;

        if self.high_stops.len() == self.stop_period {
            self.high_stops.pop_front();
            self.low_stops.pop_front();
        }
        self.high_stops.push_back(high_stop);
        self.low_stops.push_back(low_stop);
        if self.high_stops.len() < self.stop_period {
            return None;
        }
        let stop_short = self
            .high_stops
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let stop_long = self.low_stops.iter().copied().fold(f64::INFINITY, f64::min);
        Some(ChandeKrollStopOutput {
            stop_long,
            stop_short,
        })
    }

    fn reset(&mut self) {
        self.atr.reset();
        self.highs.clear();
        self.lows.clear();
        self.high_stops.clear();
        self.low_stops.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // The preliminary stop first appears on candle `atr_period`; the
        // smoothing window then needs `stop_period` of them.
        self.atr_period + self.stop_period - 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.high_stops.len() == self.stop_period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ChandeKrollStop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(high: f64, low: f64, close: f64, ts: i64) -> Candle {
        Candle::new(f64::midpoint(high, low), high, low, close, 1.0, ts).unwrap()
    }

    #[test]
    fn reference_values_flat_market() {
        // Flat candles H=11, L=9, C=10 -> TR=2 -> ATR=2; HH=11, LL=9.
        // high_stop = 11 - 1·2 = 9;  low_stop = 9 + 1·2 = 11.
        // stop_short = highest(high_stop, q) = 9; stop_long = lowest(low_stop, q) = 11.
        let candles: Vec<Candle> = (0..20).map(|i| c(11.0, 9.0, 10.0, i)).collect();
        let mut cks = ChandeKrollStop::new(5, 1.0, 3).unwrap();
        let last = cks.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last.stop_short, 9.0, epsilon = 1e-12);
        assert_relative_eq!(last.stop_long, 11.0, epsilon = 1e-12);
    }

    #[test]
    fn first_emission_matches_warmup_period() {
        let candles: Vec<Candle> = (0..16)
            .map(|i| {
                let base = 100.0 + i as f64;
                c(base + 1.0, base - 1.0, base, i)
            })
            .collect();
        let mut cks = ChandeKrollStop::new(4, 1.0, 3).unwrap();
        let out = cks.batch(&candles);
        assert_eq!(cks.warmup_period(), 6);
        for (i, v) in out.iter().enumerate().take(5) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        assert!(out[5].is_some(), "first value lands at warmup_period - 1");
    }

    #[test]
    fn rejects_invalid_params() {
        assert!(ChandeKrollStop::new(0, 1.0, 9).is_err());
        assert!(ChandeKrollStop::new(10, 1.0, 0).is_err());
        assert!(ChandeKrollStop::new(10, 0.0, 9).is_err());
        assert!(ChandeKrollStop::new(10, -1.0, 9).is_err());
        assert!(ChandeKrollStop::new(10, f64::NAN, 9).is_err());
    }

    /// Cover the const accessor `params` (97-99) and the Indicator-impl
    /// `name` body (164-166). `warmup_period` is exercised elsewhere.
    #[test]
    fn accessors_and_metadata() {
        let s = ChandeKrollStop::new(10, 1.0, 9).unwrap();
        let (p, m, q) = s.params();
        assert_eq!(p, 10);
        assert!((m - 1.0).abs() < 1e-12);
        assert_eq!(q, 9);
        assert_eq!(s.name(), "ChandeKrollStop");
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + i as f64;
                c(base + 1.0, base - 1.0, base, i)
            })
            .collect();
        let mut cks = ChandeKrollStop::classic();
        cks.batch(&candles);
        assert!(cks.is_ready());
        cks.reset();
        assert!(!cks.is_ready());
        assert_eq!(cks.update(candles[0]), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let mid = 100.0 + (i as f64 * 0.3).sin() * 8.0;
                c(mid + 1.5, mid - 1.5, mid + 0.5, i)
            })
            .collect();
        let mut a = ChandeKrollStop::classic();
        let mut b = ChandeKrollStop::classic();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_stop_period_above_maximum() {
        assert!(matches!(
            ChandeKrollStop::new(5, 1.0, crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
