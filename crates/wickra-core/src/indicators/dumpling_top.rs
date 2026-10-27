//! Dumpling Top — a rounded top (dome) confirmed by a breakdown.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Dumpling Top — the bearish mirror of the [`FryPanBottom`](crate::FryPanBottom):
/// a gently rounded **top** (dome) across the window, confirmed by a close back
/// below where it started.
///
/// ```text
/// over the last `period` closes:
///   the maximum close sits in the middle third of the window (the "dome")
///   the latest close is below the first close (the breakdown)
/// signal = −1 when both hold, else 0
/// ```
///
/// The dumpling top is a distribution pattern: price rounds over at the top as
/// buying fades, then rolls down through the level it rose from. Detection requires
/// a *central* high (a symmetric dome, not a one-sided spike) and a close below the
/// window's opening level. The output is `−1.0` (pattern) or `0.0`.
///
/// The first value lands after `period` inputs; each `update` scans the window in
/// O(`period`).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, DumplingTop};
///
/// let mut indicator = DumplingTop::new(9).unwrap();
/// let closes = [100.0, 102.0, 104.0, 105.0, 104.0, 102.0, 99.0, 97.0, 95.0];
/// let mut last = None;
/// for &cl in &closes {
///     let c = Candle::new(cl, cl + 0.5, cl - 0.5, cl, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert_eq!(last, Some(-1.0));
/// ```
#[derive(Debug, Clone)]
pub struct DumplingTop {
    period: usize,
    closes: VecDeque<f64>,
    last: Option<f64>,
}

impl DumplingTop {
    /// Construct a Dumpling Top over `period` bars.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `period < 5`.
    pub fn new(period: usize) -> Result<Self> {
        if period < 5 {
            return Err(Error::InvalidPeriod {
                message: "dumpling top needs period >= 5",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            closes: VecDeque::with_capacity(period),
            last: None,
        })
    }

    /// Configured window period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for DumplingTop {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        if self.closes.len() == self.period {
            self.closes.pop_front();
        }
        self.closes.push_back(candle.close);
        if self.closes.len() < self.period {
            return None;
        }
        let first = *self.closes.front().expect("non-empty");
        let last = *self.closes.back().expect("non-empty");
        let mut max_idx = 0;
        let mut max_val = f64::NEG_INFINITY;
        for (i, &v) in self.closes.iter().enumerate() {
            if v > max_val {
                max_val = v;
                max_idx = i;
            }
        }
        let lo = self.period / 4;
        let hi = self.period - self.period / 4;
        let dome = max_idx >= lo && max_idx < hi;
        let broke_down = last < first && last < max_val;
        let v = if dome && broke_down { -1.0 } else { 0.0 };
        self.last = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.closes.clear();
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
        "DumplingTop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(close: f64) -> Candle {
        Candle::new_unchecked(close, close + 0.5, close - 0.5, close, 1_000.0, 0)
    }

    #[test]
    fn rejects_small_period() {
        assert!(matches!(
            DumplingTop::new(4),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(DumplingTop::new(5).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let d = DumplingTop::new(9).unwrap();
        assert_eq!(d.period(), 9);
        assert_eq!(d.warmup_period(), 9);
        assert_eq!(d.name(), "DumplingTop");
        assert!(!d.is_ready());
        assert_eq!(d.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut d = DumplingTop::new(5).unwrap();
        let out = d.batch(&[c(100.0), c(101.0), c(102.0), c(101.0), c(99.0), c(98.0)]);
        for v in out.iter().take(4) {
            assert!(v.is_none());
        }
        assert!(out[4].is_some());
    }

    #[test]
    fn rounded_top_then_breakdown_signals() {
        let mut d = DumplingTop::new(9).unwrap();
        let closes = [100.0, 102.0, 104.0, 105.0, 104.0, 102.0, 99.0, 97.0, 95.0];
        let candles: Vec<Candle> = closes.iter().map(|&x| c(x)).collect();
        let last = d.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, -1.0);
    }

    #[test]
    fn one_sided_rise_is_zero() {
        let mut d = DumplingTop::new(9).unwrap();
        let candles: Vec<Candle> = (0..9).map(|i| c(100.0 + f64::from(i))).collect();
        let last = d.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, 0.0);
    }

    #[test]
    fn no_breakdown_is_zero() {
        let mut d = DumplingTop::new(9).unwrap();
        let closes = [
            100.0, 102.0, 104.0, 105.0, 104.0, 103.0, 102.0, 101.0, 100.5,
        ];
        let candles: Vec<Candle> = closes.iter().map(|&x| c(x)).collect();
        let last = d.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, 0.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut d = DumplingTop::new(5).unwrap();
        d.batch(&[c(100.0), c(101.0), c(102.0), c(101.0), c(99.0)]);
        assert!(d.is_ready());
        d.reset();
        assert!(!d.is_ready());
        assert_eq!(d.value(), None);
        assert_eq!(d.update(c(100.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| c(100.0 + (f64::from(i) * 0.3).sin() * 5.0))
            .collect();
        let batch = DumplingTop::new(9).unwrap().batch(&candles);
        let mut b = DumplingTop::new(9).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            DumplingTop::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
