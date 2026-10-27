//! New Price Lines — the "eight/ten new price lines" exhaustion count.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// New Price Lines — the Japanese "shinne" (new-price) exhaustion count: when the
/// close has made `count` consecutive new highs (or lows), the trend is considered
/// stretched and ripe for a pause or reversal.
///
/// ```text
/// consecutive higher closes form "new price lines" up
/// consecutive lower  closes form "new price lines" down
/// signal = −1 once `count` consecutive higher closes (overbought / sell warning)
/// signal = +1 once `count` consecutive lower  closes (oversold / buy warning)
/// signal =  0 otherwise
/// ```
///
/// Traditional Japanese practice flags **eight** new price lines (and a stronger
/// **ten** or twelve) as the point where a directional run becomes exhausted —
/// the market has gone up (or down) so many bars in a row that a corrective pause
/// is statistically due. The signal stays active for every bar the streak remains
/// at or above `count`, and clears the moment a close breaks the streak.
///
/// The first value lands on the second bar (one prior close is needed). The
/// output is `+1` / `0` / `−1`. Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, NewPriceLines};
///
/// let mut indicator = NewPriceLines::new(8).unwrap();
/// let mut last = None;
/// for i in 0..12 {
///     let close = 100.0 + f64::from(i); // 11 consecutive higher closes
///     let c = Candle::new(close, close, close, close, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert_eq!(last, Some(-1.0));
/// ```
#[derive(Debug, Clone)]
pub struct NewPriceLines {
    count: usize,
    prev_close: Option<f64>,
    consec_up: usize,
    consec_down: usize,
    last: Option<f64>,
}

impl NewPriceLines {
    /// Construct a New Price Lines counter that fires at `count` consecutive new
    /// closes (classic `8`, stronger `10`/`12`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `count < 2`.
    pub fn new(count: usize) -> Result<Self> {
        if count < 2 {
            return Err(Error::InvalidPeriod {
                message: "new price lines count must be >= 2",
            });
        }
        if count > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            count,
            prev_close: None,
            consec_up: 0,
            consec_down: 0,
            last: None,
        })
    }

    /// Configured count threshold.
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Current consecutive streak `(up, down)`.
    pub const fn streak(&self) -> (usize, usize) {
        (self.consec_up, self.consec_down)
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for NewPriceLines {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let close = candle.close;
        let Some(prev) = self.prev_close else {
            self.prev_close = Some(close);
            return None;
        };
        if close > prev {
            self.consec_up += 1;
            self.consec_down = 0;
        } else if close < prev {
            self.consec_down += 1;
            self.consec_up = 0;
        } else {
            self.consec_up = 0;
            self.consec_down = 0;
        }
        self.prev_close = Some(close);

        let v = if self.consec_up >= self.count {
            -1.0
        } else if self.consec_down >= self.count {
            1.0
        } else {
            0.0
        };
        self.last = Some(v);
        Some(v)
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.consec_up = 0;
        self.consec_down = 0;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        2
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "NewPriceLines"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(close: f64) -> Candle {
        Candle::new_unchecked(close, close, close, close, 1_000.0, 0)
    }

    #[test]
    fn rejects_small_count() {
        assert!(matches!(
            NewPriceLines::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(NewPriceLines::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let n = NewPriceLines::new(8).unwrap();
        assert_eq!(n.count(), 8);
        assert_eq!(n.streak(), (0, 0));
        assert_eq!(n.warmup_period(), 2);
        assert_eq!(n.name(), "NewPriceLines");
        assert!(!n.is_ready());
        assert_eq!(n.value(), None);
    }

    #[test]
    fn first_bar_seeds_without_signal() {
        let mut n = NewPriceLines::new(3).unwrap();
        assert_eq!(n.update(c(100.0)), None);
        assert!(n.update(c(101.0)).is_some());
    }

    #[test]
    fn eight_higher_closes_signal_sell() {
        let mut n = NewPriceLines::new(8).unwrap();
        // 11 consecutive higher closes -> by the 9th the count reaches 8 -> -1.
        let candles: Vec<Candle> = (0..12).map(|i| c(100.0 + f64::from(i))).collect();
        let last = n.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, -1.0);
    }

    #[test]
    fn eight_lower_closes_signal_buy() {
        let mut n = NewPriceLines::new(8).unwrap();
        let candles: Vec<Candle> = (0..12).map(|i| c(200.0 - f64::from(i))).collect();
        let last = n.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, 1.0);
    }

    #[test]
    fn break_in_streak_clears_signal() {
        let mut n = NewPriceLines::new(3).unwrap();
        n.batch(&[c(100.0), c(101.0), c(102.0), c(103.0)]); // streak 3 -> -1
        assert_eq!(n.value(), Some(-1.0));
        // A lower close breaks the up streak.
        assert_eq!(n.update(c(102.0)), Some(0.0));
        assert_eq!(n.streak(), (0, 1));
    }

    #[test]
    fn unchanged_close_resets_streak() {
        let mut n = NewPriceLines::new(3).unwrap();
        n.batch(&[c(100.0), c(101.0), c(102.0)]);
        assert_eq!(n.update(c(102.0)), Some(0.0)); // equal -> reset
        assert_eq!(n.streak(), (0, 0));
    }

    #[test]
    fn reset_clears_state() {
        let mut n = NewPriceLines::new(3).unwrap();
        n.batch(&[c(100.0), c(101.0), c(102.0), c(103.0)]);
        assert!(n.is_ready());
        n.reset();
        assert!(!n.is_ready());
        assert_eq!(n.value(), None);
        assert_eq!(n.streak(), (0, 0));
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| c(100.0 + (f64::from(i) * 0.25).sin() * 9.0))
            .collect();
        let batch = NewPriceLines::new(8).unwrap().batch(&candles);
        let mut b = NewPriceLines::new(8).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_count_above_maximum() {
        assert!(matches!(
            NewPriceLines::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
