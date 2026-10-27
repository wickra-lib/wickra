//! Long Line candlestick pattern.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;
use std::collections::VecDeque;

/// Long Line — a single candle whose range is *longer* than the recent average and
/// whose body dominates that range (a solid directional bar). Because "long" only
/// has meaning relative to recent activity, the detector compares each candle's
/// range against a rolling average of the previous `period` ranges.
///
/// ```text
/// avg = mean range of the previous `period` candles
/// long line = range > avg  AND  |close − open| >= 0.5 * range
/// white -> +1.0,  black -> −1.0
/// ```
///
/// Output is `+1.0` (long white line), `−1.0` (long black line), or `0.0`
/// otherwise. The first `period` candles return `0.0` while the rolling average
/// fills. `period` defaults to `5` and must be at least `1`. This rolling baseline
/// is the one place the family departs from a purely intra-candle rule, since a
/// short/long classification is inherently scale-relative. Pattern-shape check
/// only — no trend filter is applied; combine with a trend indicator for
/// actionable signals.
///
/// # Signed ±1 encoding
///
/// This detector emits the uniform candlestick sign convention shared across the
/// pattern family — `+1.0` bullish, `−1.0` bearish, `0.0` no pattern — so it
/// drops straight into a machine-learning feature matrix as a single dimension.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, LongLine};
///
/// let mut indicator = LongLine::new();
/// // Five quiet bars fill the rolling average.
/// for ts in 0..5 {
///     indicator.update(Candle::new(10.0, 10.5, 9.5, 10.2, 1.0, ts).unwrap());
/// }
/// // A wide solid white bar is a long white line.
/// let out = indicator
///     .update(Candle::new(10.0, 13.0, 9.9, 12.9, 1.0, 5).unwrap());
/// assert_eq!(out, Some(1.0));
/// ```
#[derive(Debug, Clone)]
pub struct LongLine {
    period: usize,
    ranges: VecDeque<f64>,
    /// Whether a value has been emitted since the last reset. The trait
    /// defines `is_ready` as exactly that, and the state this used to key
    /// off changed at a different moment.
    has_emitted: bool,
}

impl Default for LongLine {
    fn default() -> Self {
        Self::new()
    }
}

impl LongLine {
    /// Construct a Long Line detector with the default 5-candle rolling average.
    pub const fn new() -> Self {
        Self {
            period: 5,
            ranges: VecDeque::new(),
            has_emitted: false,
        }
    }

    /// Construct a Long Line detector with a custom averaging period.
    ///
    /// `period` must be at least `1`.
    pub fn with_period(period: usize) -> Result<Self> {
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
            ranges: VecDeque::new(),
            has_emitted: false,
        })
    }

    /// Configured averaging period.
    pub fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for LongLine {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let range = candle.high - candle.low;
        let body = candle.close - candle.open;
        if self.ranges.len() < self.period {
            self.ranges.push_back(range);
            return None;
        }
        // Past the window gate every path emits, so readiness starts here.
        self.has_emitted = true;
        let avg = self.ranges.iter().sum::<f64>() / self.period as f64;
        self.ranges.push_back(range);
        self.ranges.pop_front();
        if range > avg && body.abs() >= 0.5 * range {
            return Some(if body > 0.0 { 1.0 } else { -1.0 });
        }
        Some(0.0)
    }

    fn reset(&mut self) {
        self.has_emitted = false;
        self.ranges.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "LongLine"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(open: f64, high: f64, low: f64, close: f64, ts: i64) -> Candle {
        Candle::new(open, high, low, close, 1.0, ts).unwrap()
    }

    fn warm(t: &mut LongLine) {
        for ts in 0..5 {
            assert_eq!(t.update(c(10.0, 10.5, 9.5, 10.2, ts)), None);
        }
    }

    #[test]
    fn rejects_zero_period() {
        assert!(LongLine::with_period(0).is_err());
    }

    #[test]
    fn accepts_valid_period() {
        let t = LongLine::with_period(10).unwrap();
        assert_eq!(t.period(), 10);
    }

    #[test]
    fn accessors_and_metadata() {
        let t = LongLine::new();
        assert_eq!(t.name(), "LongLine");
        assert_eq!(t.warmup_period(), 5);
        assert!(!t.is_ready());
        assert_eq!(t.period(), 5);
    }

    #[test]
    fn long_white_line_is_plus_one() {
        let mut t = LongLine::new();
        warm(&mut t);
        // The window is full but nothing has been emitted yet, so the
        // indicator is not ready until the next bar produces a value.
        assert!(!t.is_ready());
        assert_eq!(t.update(c(10.0, 13.0, 9.9, 12.9, 5)), Some(1.0));
    }

    #[test]
    fn long_black_line_is_minus_one() {
        let mut t = LongLine::new();
        warm(&mut t);
        assert_eq!(t.update(c(13.0, 13.1, 9.9, 10.0, 5)), Some(-1.0));
    }

    #[test]
    fn short_range_yields_zero() {
        let mut t = LongLine::new();
        warm(&mut t);
        // Range no bigger than the average -> not a long line.
        assert_eq!(t.update(c(10.0, 10.5, 9.5, 10.2, 5)), Some(0.0));
    }

    #[test]
    fn wide_range_small_body_yields_zero() {
        let mut t = LongLine::new();
        warm(&mut t);
        // Wide range but a tiny body -> a spinning top, not a long line.
        assert_eq!(t.update(c(10.5, 13.0, 9.9, 10.6, 5)), Some(0.0));
    }

    #[test]
    fn warmup_withholds() {
        let mut t = LongLine::new();
        for ts in 0..5 {
            assert_eq!(t.update(c(10.0, 13.0, 9.9, 12.9, ts)), None);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + i as f64;
                if i % 7 == 0 {
                    c(base, base + 4.0, base - 0.1, base + 3.9, i)
                } else {
                    c(base, base + 0.5, base - 0.5, base + 0.2, i)
                }
            })
            .collect();
        let mut a = LongLine::new();
        let mut b = LongLine::new();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut t = LongLine::new();
        warm(&mut t);
        t.update(c(10.0, 13.0, 9.9, 12.9, 5));
        assert!(t.is_ready());
        t.reset();
        assert!(!t.is_ready());
        assert_eq!(t.update(c(10.0, 13.0, 9.9, 12.9, 0)), None);
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(LongLine::default().period(), LongLine::new().period());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            LongLine::with_period(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
