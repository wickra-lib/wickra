//! Frying Pan Bottom — a rounded bottom (U) confirmed by recovery.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Frying Pan Bottom — a gently rounded bottom across the lookback window: prices
/// decline, flatten near the centre, then recover above where they started.
///
/// ```text
/// over the last `period` closes:
///   the minimum close sits in the middle third of the window (the "bowl")
///   the latest close is above the first close (the rim is recovered)
/// signal = +1 when both hold, else 0
/// ```
///
/// The frying pan is a bullish accumulation pattern: a saucer-shaped base where
/// selling dries up, the curve flattens, and price lifts off the rim. Detecting it
/// requires the low point to be central (a symmetric bowl, not a one-sided drop)
/// and the close to have climbed back above the window's opening level, confirming
/// the breakout from the base. The output is `+1.0` (pattern) or `0.0`.
///
/// The first value lands after `period` inputs; each `update` scans the window in
/// O(`period`).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, FryPanBottom};
///
/// let mut indicator = FryPanBottom::new(9).unwrap();
/// // A U-shaped base then recovery.
/// let closes = [100.0, 98.0, 96.0, 95.0, 96.0, 98.0, 101.0, 103.0, 105.0];
/// let mut last = None;
/// for &cl in &closes {
///     let c = Candle::new(cl, cl + 0.5, cl - 0.5, cl, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert_eq!(last, Some(1.0));
/// ```
#[derive(Debug, Clone)]
pub struct FryPanBottom {
    period: usize,
    closes: VecDeque<f64>,
    last: Option<f64>,
}

impl FryPanBottom {
    /// Construct a Frying Pan Bottom over `period` bars.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `period < 5` (a bowl needs room for a
    /// central low between recovering sides).
    pub fn new(period: usize) -> Result<Self> {
        if period < 5 {
            return Err(Error::InvalidPeriod {
                message: "frying pan bottom needs period >= 5",
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

impl Indicator for FryPanBottom {
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
        // Index of the minimum close.
        let mut min_idx = 0;
        let mut min_val = f64::INFINITY;
        for (i, &v) in self.closes.iter().enumerate() {
            if v < min_val {
                min_val = v;
                min_idx = i;
            }
        }
        let lo = self.period / 4;
        let hi = self.period - self.period / 4;
        let bowl = min_idx >= lo && min_idx < hi;
        let recovered = last > first && last > min_val;
        let v = if bowl && recovered { 1.0 } else { 0.0 };
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
        "FryPanBottom"
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
            FryPanBottom::new(4),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(FryPanBottom::new(5).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let f = FryPanBottom::new(9).unwrap();
        assert_eq!(f.period(), 9);
        assert_eq!(f.warmup_period(), 9);
        assert_eq!(f.name(), "FryPanBottom");
        assert!(!f.is_ready());
        assert_eq!(f.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut f = FryPanBottom::new(5).unwrap();
        let out = f.batch(&[c(100.0), c(99.0), c(98.0), c(99.0), c(101.0), c(102.0)]);
        for v in out.iter().take(4) {
            assert!(v.is_none());
        }
        assert!(out[4].is_some());
    }

    #[test]
    fn rounded_bottom_then_recovery_signals() {
        let mut f = FryPanBottom::new(9).unwrap();
        let closes = [100.0, 98.0, 96.0, 95.0, 96.0, 98.0, 101.0, 103.0, 105.0];
        let candles: Vec<Candle> = closes.iter().map(|&x| c(x)).collect();
        let last = f.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, 1.0);
    }

    #[test]
    fn one_sided_drop_is_zero() {
        // A straight decline (min at the end) is not a bowl.
        let mut f = FryPanBottom::new(9).unwrap();
        let candles: Vec<Candle> = (0..9).map(|i| c(100.0 - f64::from(i))).collect();
        let last = f.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, 0.0);
    }

    #[test]
    fn no_recovery_is_zero() {
        // Bowl shape but the last close never climbs above the first.
        let mut f = FryPanBottom::new(9).unwrap();
        let closes = [100.0, 98.0, 96.0, 95.0, 96.0, 97.0, 98.0, 99.0, 99.5];
        let candles: Vec<Candle> = closes.iter().map(|&x| c(x)).collect();
        let last = f.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, 0.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut f = FryPanBottom::new(5).unwrap();
        f.batch(&[c(100.0), c(99.0), c(98.0), c(99.0), c(101.0)]);
        assert!(f.is_ready());
        f.reset();
        assert!(!f.is_ready());
        assert_eq!(f.value(), None);
        assert_eq!(f.update(c(100.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| c(100.0 + (f64::from(i) * 0.3).sin() * 5.0))
            .collect();
        let batch = FryPanBottom::new(9).unwrap().batch(&candles);
        let mut b = FryPanBottom::new(9).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            FryPanBottom::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
