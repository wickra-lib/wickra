//! Opening Range (OR): high / low of the first N session bars plus the
//! current bar's breakout distance from the range midpoint.
//!
//! Conceptually identical to [`crate::InitialBalance`] but with two
//! differences: the default window is shorter (6 = 30 min on 5-minute bars)
//! and the output carries a third field, `breakout_distance`, which is the
//! signed distance from the current candle's close to the range midpoint —
//! positive for breakouts above the OR, negative for breakdowns. Callers
//! MUST invoke [`Indicator::reset`] at every new session boundary to start
//! a fresh OR.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Opening Range output: high, low and breakout distance from the OR midpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpeningRangeOutput {
    /// Session-opening high established over the OR window.
    pub high: f64,
    /// Session-opening low established over the OR window.
    pub low: f64,
    /// Current bar's close minus the OR midpoint. Positive once price
    /// trades above the range mid, negative below.
    pub breakout_distance: f64,
}

/// Session Opening Range (first N bars + breakout distance).
///
/// `period` defaults to **6** — the canonical 30-minute opening range on
/// 5-minute bars. Callers MUST invoke [`Indicator::reset`] at session
/// boundaries; otherwise the OR locks after the first `period` bars and
/// stays fixed for the remainder of the instance's life.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, OpeningRange};
///
/// let mut or = OpeningRange::new(2).unwrap();
/// let bars = [
///     Candle::new(100.0, 102.0, 99.0, 101.0, 10.0, 0).unwrap(),
///     Candle::new(101.0, 103.0, 100.0, 102.0, 10.0, 1).unwrap(),
///     // Now locked — breakout distance reflects close - (high + low) / 2.
///     Candle::new(102.0, 110.0, 102.0, 105.0, 10.0, 2).unwrap(),
/// ];
/// for b in bars {
///     or.update(b);
/// }
/// let v = or.value().unwrap();
/// assert_eq!(v.high, 103.0);
/// assert_eq!(v.low, 99.0);
/// assert_eq!(v.breakout_distance, 105.0 - (103.0 + 99.0) / 2.0);
/// ```
#[derive(Debug, Clone)]
pub struct OpeningRange {
    period: usize,
    bars_seen: usize,
    high: f64,
    low: f64,
    last_close: f64,
    locked: bool,
    last: Option<OpeningRangeOutput>,
}

impl OpeningRange {
    /// Construct an Opening Range indicator with the given window length.
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
            bars_seen: 0,
            high: f64::NEG_INFINITY,
            low: f64::INFINITY,
            last_close: 0.0,
            locked: false,
            last: None,
        })
    }

    /// Classic 6-bar Opening Range.
    pub fn classic() -> Self {
        Self::new(6).expect("classic OR period is valid")
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Most recent output if at least one bar has been seen.
    pub const fn value(&self) -> Option<OpeningRangeOutput> {
        self.last
    }

    /// True once `period` bars have been ingested and the OR is locked.
    pub const fn is_locked(&self) -> bool {
        self.locked
    }

    fn snapshot(&self) -> OpeningRangeOutput {
        let mid = f64::midpoint(self.high, self.low);
        OpeningRangeOutput {
            high: self.high,
            low: self.low,
            breakout_distance: self.last_close - mid,
        }
    }
}

impl Indicator for OpeningRange {
    type Input = Candle;
    type Output = OpeningRangeOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<OpeningRangeOutput> {
        if !self.locked {
            if candle.high > self.high {
                self.high = candle.high;
            }
            if candle.low < self.low {
                self.low = candle.low;
            }
            self.bars_seen += 1;
            if self.bars_seen >= self.period {
                self.locked = true;
            }
        }
        self.last_close = candle.close;
        let out = self.snapshot();
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.bars_seen = 0;
        self.high = f64::NEG_INFINITY;
        self.low = f64::INFINITY;
        self.last_close = 0.0;
        self.locked = false;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.bars_seen > 0
    }

    #[inline]
    fn name(&self) -> &'static str {
        "OpeningRange"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(high: f64, low: f64, close: f64, ts: i64) -> Candle {
        let open = f64::midpoint(high, low);
        Candle::new(open, high, low, close, 10.0, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(OpeningRange::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let or = OpeningRange::new(6).unwrap();
        assert_eq!(or.period(), 6);
        assert_eq!(or.name(), "OpeningRange");
        assert_eq!(or.warmup_period(), 1);
        assert!(or.value().is_none());
        assert!(!or.is_locked());
    }

    #[test]
    fn classic_is_constructible() {
        let or = OpeningRange::classic();
        assert_eq!(or.period(), 6);
    }

    #[test]
    fn tracks_range_during_window() {
        let mut or = OpeningRange::new(3).unwrap();
        let o1 = or.update(c(102.0, 100.0, 101.0, 0)).unwrap();
        assert_relative_eq!(o1.high, 102.0);
        assert_relative_eq!(o1.low, 100.0);
        // close 101 vs mid 101 → breakout 0.
        assert_relative_eq!(o1.breakout_distance, 0.0, epsilon = 1e-12);
        let o2 = or.update(c(105.0, 99.0, 104.0, 1)).unwrap();
        assert_relative_eq!(o2.high, 105.0);
        assert_relative_eq!(o2.low, 99.0);
        // close 104 vs mid 102 → breakout 2.
        assert_relative_eq!(o2.breakout_distance, 2.0, epsilon = 1e-12);
    }

    #[test]
    fn locks_after_period_and_breakout_reflects_close_minus_mid() {
        let mut or = OpeningRange::new(2).unwrap();
        or.update(c(102.0, 100.0, 101.0, 0));
        or.update(c(103.0, 101.0, 102.0, 1));
        assert!(or.is_locked());
        // OR locked at high 103, low 100, mid 101.5.
        // Bar 2: wide candle ignored for high/low; close 105 -> breakout 3.5.
        let after = or.update(c(200.0, 50.0, 105.0, 2)).unwrap();
        assert_relative_eq!(after.high, 103.0);
        assert_relative_eq!(after.low, 100.0);
        assert_relative_eq!(after.breakout_distance, 3.5, epsilon = 1e-12);
    }

    #[test]
    fn breakout_distance_is_negative_below_range() {
        let mut or = OpeningRange::new(2).unwrap();
        or.update(c(102.0, 100.0, 101.0, 0));
        or.update(c(103.0, 101.0, 102.0, 1));
        // mid 101.5, close 90 -> -11.5.
        let out = or.update(c(110.0, 89.0, 90.0, 2)).unwrap();
        assert_relative_eq!(out.breakout_distance, -11.5, epsilon = 1e-12);
    }

    #[test]
    fn reset_unlocks_and_clears_state() {
        let mut or = OpeningRange::new(2).unwrap();
        or.update(c(102.0, 100.0, 101.0, 0));
        or.update(c(103.0, 101.0, 102.0, 1));
        assert!(or.is_locked());
        or.reset();
        assert!(!or.is_locked());
        assert!(!or.is_ready());
        let o = or.update(c(50.0, 49.0, 49.5, 2)).unwrap();
        assert_relative_eq!(o.high, 50.0);
        assert_relative_eq!(o.low, 49.0);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..20)
            .map(|i| {
                let base = 100.0 + i as f64 * 0.25;
                c(base + 1.0, base - 1.0, base, i)
            })
            .collect();
        let mut a = OpeningRange::new(5).unwrap();
        let mut b = OpeningRange::new(5).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn is_ready_after_first_bar() {
        let mut or = OpeningRange::new(5).unwrap();
        assert!(!or.is_ready());
        or.update(c(101.0, 99.0, 100.0, 0));
        assert!(or.is_ready());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            OpeningRange::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
