//! Initial Balance (IB): the high / low established over the first N bars of
//! a session.
//!
//! Tracks the running session high and session low across the first `period`
//! candles received since construction or [`InitialBalance::reset`]. Once the
//! `period`th candle has been ingested the value is frozen and every
//! subsequent call to [`Indicator::update`] returns the same locked
//! [`InitialBalanceOutput`] until the caller invokes `reset()` at the start of
//! a new session.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Initial Balance output: the high / low of the first N bars of a session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InitialBalanceOutput {
    /// Session-opening high established over the IB window.
    pub high: f64,
    /// Session-opening low established over the IB window.
    pub low: f64,
}

/// Session Initial Balance (first N bars).
///
/// `period` defaults to **12** — the canonical one-hour IB on 5-minute bars
/// for U.S. equities. Callers MUST invoke [`Indicator::reset`] at every new
/// session boundary; otherwise the IB locks after the first `period` bars and
/// stays fixed for the entire lifetime of the instance.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, InitialBalance, Indicator};
///
/// let mut ib = InitialBalance::new(3).unwrap();
/// let bars = [
///     Candle::new(100.0, 102.0, 99.0, 101.0, 10.0, 0).unwrap(),
///     Candle::new(101.0, 103.0, 100.0, 102.0, 10.0, 1).unwrap(),
///     Candle::new(102.0, 104.0, 101.0, 103.0, 10.0, 2).unwrap(),
///     // Locked after period bars — subsequent bars do not modify IB.
///     Candle::new(103.0, 120.0, 80.0, 105.0, 10.0, 3).unwrap(),
/// ];
/// for b in bars {
///     ib.update(b);
/// }
/// let v = ib.value().unwrap();
/// assert_eq!(v.high, 104.0);
/// assert_eq!(v.low, 99.0);
/// ```
#[derive(Debug, Clone)]
pub struct InitialBalance {
    period: usize,
    bars_seen: usize,
    high: f64,
    low: f64,
    locked: bool,
}

impl InitialBalance {
    /// Construct an Initial Balance indicator with the given window length.
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
            locked: false,
        })
    }

    /// Classic 12-bar Initial Balance.
    pub fn classic() -> Self {
        Self::new(12).expect("classic IB period is valid")
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Most recent output if at least one bar has been seen.
    pub fn value(&self) -> Option<InitialBalanceOutput> {
        if self.bars_seen == 0 {
            None
        } else {
            Some(InitialBalanceOutput {
                high: self.high,
                low: self.low,
            })
        }
    }

    /// True once `period` bars have been ingested and the IB is locked.
    pub const fn is_locked(&self) -> bool {
        self.locked
    }
}

impl Indicator for InitialBalance {
    type Input = Candle;
    type Output = InitialBalanceOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<InitialBalanceOutput> {
        if self.locked {
            return Some(InitialBalanceOutput {
                high: self.high,
                low: self.low,
            });
        }
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
        Some(InitialBalanceOutput {
            high: self.high,
            low: self.low,
        })
    }

    fn reset(&mut self) {
        self.bars_seen = 0;
        self.high = f64::NEG_INFINITY;
        self.low = f64::INFINITY;
        self.locked = false;
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
        "InitialBalance"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(high: f64, low: f64, ts: i64) -> Candle {
        // open / close pinned inside [low, high] so the candle validates.
        let mid = f64::midpoint(high, low);
        Candle::new(mid, high, low, mid, 10.0, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(InitialBalance::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut ib = InitialBalance::new(12).unwrap();
        assert_eq!(ib.period(), 12);
        assert_eq!(ib.name(), "InitialBalance");
        assert_eq!(ib.warmup_period(), 1);
        assert!(ib.value().is_none());
        assert!(!ib.is_locked());
        // After the first bar, value() returns Some with that bar's H/L.
        ib.update(c(102.0, 100.0, 0));
        let v = ib.value().unwrap();
        assert_relative_eq!(v.high, 102.0);
        assert_relative_eq!(v.low, 100.0);
    }

    #[test]
    fn classic_is_constructible() {
        let ib = InitialBalance::classic();
        assert_eq!(ib.period(), 12);
    }

    #[test]
    fn tracks_high_low_during_window() {
        let mut ib = InitialBalance::new(3).unwrap();
        let o1 = ib.update(c(102.0, 100.0, 0)).unwrap();
        assert_relative_eq!(o1.high, 102.0);
        assert_relative_eq!(o1.low, 100.0);
        let o2 = ib.update(c(105.0, 99.0, 1)).unwrap();
        assert_relative_eq!(o2.high, 105.0);
        assert_relative_eq!(o2.low, 99.0);
        let o3 = ib.update(c(103.0, 99.5, 2)).unwrap();
        assert_relative_eq!(o3.high, 105.0);
        assert_relative_eq!(o3.low, 99.0);
        assert!(ib.is_locked());
    }

    #[test]
    fn locks_after_period_and_ignores_subsequent_bars() {
        let mut ib = InitialBalance::new(2).unwrap();
        ib.update(c(102.0, 100.0, 0));
        ib.update(c(103.0, 101.0, 1));
        assert!(ib.is_locked());
        // Wide bar after lock must not modify the IB.
        let after = ib.update(c(200.0, 50.0, 2)).unwrap();
        assert_relative_eq!(after.high, 103.0);
        assert_relative_eq!(after.low, 100.0);
    }

    #[test]
    fn reset_unlocks_and_clears_state() {
        let mut ib = InitialBalance::new(2).unwrap();
        ib.update(c(102.0, 100.0, 0));
        ib.update(c(103.0, 101.0, 1));
        assert!(ib.is_locked());
        ib.reset();
        assert!(!ib.is_locked());
        assert!(!ib.is_ready());
        // After reset the next session's first bar drives the IB anew.
        let o = ib.update(c(50.0, 49.0, 2)).unwrap();
        assert_relative_eq!(o.high, 50.0);
        assert_relative_eq!(o.low, 49.0);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..20)
            .map(|i| c(100.0 + i as f64, 99.0 + i as f64 * 0.5, i))
            .collect();
        let mut a = InitialBalance::new(5).unwrap();
        let mut b = InitialBalance::new(5).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn is_ready_after_first_bar() {
        let mut ib = InitialBalance::new(5).unwrap();
        assert!(!ib.is_ready());
        ib.update(c(101.0, 99.0, 0));
        assert!(ib.is_ready());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            InitialBalance::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
