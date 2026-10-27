//! Donchian Channel Stop (Turtle).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Donchian Channel Stop output: the long-side and short-side trailing stops.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DonchianStopOutput {
    /// Long-position stop: the lowest low over the lookback.
    pub stop_long: f64,
    /// Short-position stop: the highest high over the lookback.
    pub stop_short: f64,
}

/// Donchian Channel Stop — the original Turtle-trader exit rule. A long is
/// trailed at the lowest low of the last `period` bars; a short at the highest
/// high. There is no ATR, no multiplier, and no flip-bit — the two levels are
/// always emitted and the caller selects whichever side matches the position.
///
/// ```text
/// stop_long  = min(low,  over period bars)
/// stop_short = max(high, over period bars)
/// ```
///
/// Richard Dennis' original Turtle System used a 20-bar entry channel and a
/// 10-bar exit channel — feed this indicator the exit window. The first
/// `period` candles are warmup; on the bar that fills the window it begins
/// emitting both stops.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, DonchianStop};
///
/// let mut indicator = DonchianStop::new(10).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct DonchianStop {
    period: usize,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
}

impl DonchianStop {
    /// Construct a Donchian Channel Stop with an explicit lookback.
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
            highs: VecDeque::with_capacity(period),
            lows: VecDeque::with_capacity(period),
        })
    }

    /// The Turtle-system exit window: a `10`-bar lookback.
    pub fn classic() -> Self {
        Self::new(10).expect("classic Donchian Stop period is valid")
    }

    /// Configured lookback.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for DonchianStop {
    type Input = Candle;
    type Output = DonchianStopOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<DonchianStopOutput> {
        if self.highs.len() == self.period {
            self.highs.pop_front();
            self.lows.pop_front();
        }
        self.highs.push_back(candle.high);
        self.lows.push_back(candle.low);
        if self.highs.len() < self.period {
            return None;
        }
        let stop_short = self.highs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let stop_long = self.lows.iter().copied().fold(f64::INFINITY, f64::min);
        Some(DonchianStopOutput {
            stop_long,
            stop_short,
        })
    }

    fn reset(&mut self) {
        self.highs.clear();
        self.lows.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.highs.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "DonchianStop"
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
    fn rejects_zero_period() {
        assert!(DonchianStop::new(0).is_err());
    }

    #[test]
    fn accessors_and_metadata() {
        let s = DonchianStop::classic();
        assert_eq!(s.period(), 10);
        assert_eq!(s.warmup_period(), 10);
        assert_eq!(s.name(), "DonchianStop");
    }

    #[test]
    fn first_emission_matches_warmup() {
        let candles: Vec<Candle> = (0..10)
            .map(|i| {
                let base = 100.0 + i as f64;
                c(base + 1.0, base - 1.0, base, i)
            })
            .collect();
        let mut s = DonchianStop::new(5).unwrap();
        let out = s.batch(&candles);
        for (i, v) in out.iter().enumerate().take(4) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        assert!(out[4].is_some());
    }

    #[test]
    fn reference_values_uptrend_window() {
        // Highs 0..5 = 1..6; lowest low = 0, highest high = 5.
        let candles: Vec<Candle> = (0..5)
            .map(|i| {
                let base = i as f64 + 0.5;
                c(base + 0.5, base - 0.5, base, i)
            })
            .collect();
        let mut s = DonchianStop::new(5).unwrap();
        let out = s.batch(&candles);
        let v = out[4].expect("ready at index 4");
        assert_relative_eq!(v.stop_short, 5.0, epsilon = 1e-12);
        assert_relative_eq!(v.stop_long, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_holds_both_stops() {
        let candles: Vec<Candle> = (0..30).map(|i| c(11.0, 9.0, 10.0, i)).collect();
        let mut s = DonchianStop::new(5).unwrap();
        for v in s.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v.stop_short, 11.0, epsilon = 1e-12);
            assert_relative_eq!(v.stop_long, 9.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..30)
            .map(|i| {
                let base = 100.0 + i as f64;
                c(base + 1.0, base - 1.0, base, i)
            })
            .collect();
        let mut s = DonchianStop::classic();
        s.batch(&candles);
        assert!(s.is_ready());
        s.reset();
        assert!(!s.is_ready());
        assert_eq!(s.update(candles[0]), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let mid = 100.0 + (i as f64 * 0.3).sin() * 8.0;
                c(mid + 1.5, mid - 1.5, mid + 0.5, i)
            })
            .collect();
        let mut a = DonchianStop::classic();
        let mut b = DonchianStop::classic();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            DonchianStop::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
