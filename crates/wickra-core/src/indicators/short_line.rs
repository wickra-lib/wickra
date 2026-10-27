//! Short Line candlestick pattern.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;
use std::collections::VecDeque;

/// Short Line — a single candle whose range is *shorter* than the recent average
/// while its body still dominates that (small) range: a compact directional bar.
/// As with [`LongLine`](crate::LongLine), "short" only has meaning relative to
/// recent activity, so the detector compares each candle's range against a rolling
/// average of the previous `period` ranges.
///
/// ```text
/// avg = mean range of the previous `period` candles
/// short line = range < avg  AND  |close − open| >= 0.5 * range
/// white -> +1.0,  black -> −1.0
/// ```
///
/// Output is `+1.0` (short white line), `−1.0` (short black line), or `0.0`
/// otherwise. The first `period` candles return `0.0` while the rolling average
/// fills. `period` defaults to `5` and must be at least `1`. Pattern-shape check
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
/// use wickra_core::{Candle, Indicator, ShortLine};
///
/// let mut indicator = ShortLine::new();
/// // Five wide bars fill the rolling average.
/// for ts in 0..5 {
///     indicator.update(Candle::new(10.0, 13.0, 9.5, 12.9, 1.0, ts).unwrap());
/// }
/// // A compact solid white bar is a short white line.
/// let out = indicator
///     .update(Candle::new(10.0, 11.0, 9.9, 10.9, 1.0, 5).unwrap());
/// assert_eq!(out, Some(1.0));
/// ```
#[derive(Debug, Clone)]
pub struct ShortLine {
    period: usize,
    ranges: VecDeque<f64>,
    /// Whether a value has been emitted since the last reset. The trait
    /// defines `is_ready` as exactly that, and the state this used to key
    /// off changed at a different moment.
    has_emitted: bool,
}

impl Default for ShortLine {
    fn default() -> Self {
        Self::new()
    }
}

impl ShortLine {
    /// Construct a Short Line detector with the default 5-candle rolling average.
    pub const fn new() -> Self {
        Self {
            period: 5,
            ranges: VecDeque::new(),
            has_emitted: false,
        }
    }

    /// Construct a Short Line detector with a custom averaging period.
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

impl Indicator for ShortLine {
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
        if range < avg && body.abs() >= 0.5 * range {
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
        "ShortLine"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(open: f64, high: f64, low: f64, close: f64, ts: i64) -> Candle {
        Candle::new(open, high, low, close, 1.0, ts).unwrap()
    }

    fn warm(t: &mut ShortLine) {
        for ts in 0..5 {
            assert_eq!(t.update(c(10.0, 13.0, 9.5, 12.9, ts)), None);
        }
    }

    #[test]
    fn rejects_zero_period() {
        assert!(ShortLine::with_period(0).is_err());
    }

    #[test]
    fn accepts_valid_period() {
        let t = ShortLine::with_period(10).unwrap();
        assert_eq!(t.period(), 10);
    }

    #[test]
    fn accessors_and_metadata() {
        let t = ShortLine::new();
        assert_eq!(t.name(), "ShortLine");
        assert_eq!(t.warmup_period(), 5);
        assert!(!t.is_ready());
        assert_eq!(t.period(), 5);
    }

    #[test]
    fn short_white_line_is_plus_one() {
        let mut t = ShortLine::new();
        warm(&mut t);
        // The window is full but nothing has been emitted yet, so the
        // indicator is not ready until the next bar produces a value.
        assert!(!t.is_ready());
        assert_eq!(t.update(c(10.0, 11.0, 9.9, 10.9, 5)), Some(1.0));
    }

    #[test]
    fn short_black_line_is_minus_one() {
        let mut t = ShortLine::new();
        warm(&mut t);
        assert_eq!(t.update(c(10.9, 11.0, 9.9, 10.0, 5)), Some(-1.0));
    }

    #[test]
    fn wide_range_yields_zero() {
        let mut t = ShortLine::new();
        warm(&mut t);
        // Range as wide as the average -> not a short line.
        assert_eq!(t.update(c(10.0, 13.0, 9.5, 12.9, 5)), Some(0.0));
    }

    #[test]
    fn short_range_small_body_yields_zero() {
        let mut t = ShortLine::new();
        warm(&mut t);
        // Compact range but a tiny body -> not a solid short line.
        assert_eq!(t.update(c(10.4, 11.0, 9.9, 10.5, 5)), Some(0.0));
    }

    #[test]
    fn warmup_withholds() {
        let mut t = ShortLine::new();
        for ts in 0..5 {
            assert_eq!(t.update(c(10.0, 11.0, 9.9, 10.9, ts)), None);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let base = 100.0 + i as f64;
                if i % 7 == 0 {
                    c(base, base + 0.6, base - 0.1, base + 0.5, i)
                } else {
                    c(base, base + 3.0, base - 1.0, base + 2.8, i)
                }
            })
            .collect();
        let mut a = ShortLine::new();
        let mut b = ShortLine::new();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut t = ShortLine::new();
        warm(&mut t);
        t.update(c(10.0, 11.0, 9.9, 10.9, 5));
        assert!(t.is_ready());
        t.reset();
        assert!(!t.is_ready());
        assert_eq!(t.update(c(10.0, 11.0, 9.9, 10.9, 0)), None);
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(ShortLine::default().period(), ShortLine::new().period());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            ShortLine::with_period(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
