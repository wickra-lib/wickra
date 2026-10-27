//! Average Daily Range (ADR) — the mean high-minus-low range of the last `period`
//! completed calendar-day sessions.

use std::collections::VecDeque;

use crate::calendar::civil_from_timestamp;
use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Average Daily Range over the last `period` completed sessions.
///
/// The indicator tracks the running high / low of the current session (the
/// wall-clock day of [`Candle::timestamp`](crate::Candle) shifted by
/// `utc_offset_minutes`). When a new day begins, the just-finished session's
/// range (`high - low`) joins a rolling window of the last `period` completed
/// days, and the reported value is their mean. The current, still-forming day is
/// excluded until it closes. No value is produced until the first session
/// completes.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, AverageDailyRange};
///
/// let hour = 3_600_000;
/// let mut adr = AverageDailyRange::new(2, 0).unwrap();
/// // Day 1 range 10 (high 110, low 100) — still forming, so None.
/// assert!(adr.update(Candle::new(105.0, 110.0, 100.0, 108.0, 1.0, 0).unwrap()).is_none());
/// // First bar of day 2 closes day 1: ADR = 10.
/// let v = adr.update(Candle::new(108.0, 112.0, 106.0, 109.0, 1.0, 24 * hour).unwrap()).unwrap();
/// assert!((v - 10.0).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct AverageDailyRange {
    period: usize,
    utc_offset_minutes: i32,
    day_key: Option<(i64, u32, u32)>,
    cur_high: f64,
    cur_low: f64,
    completed: VecDeque<f64>,
    sum: f64,
}

impl AverageDailyRange {
    ///
    /// The offset is a constant and does not follow daylight saving: for a
    /// venue that observes it, one value is correct for part of the year and an
    /// hour out for the rest, which shifts every session boundary by an hour.
    /// Either pass the offset in force for the span being analysed and keep
    /// spans that cross a transition apart, or convert the timestamps to the
    /// venue's wall clock upstream and pass `0`.
    /// Construct an ADR indicator over `period` completed days.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0`.
    pub fn new(period: usize, utc_offset_minutes: i32) -> Result<Self> {
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
            utc_offset_minutes,
            day_key: None,
            cur_high: f64::NEG_INFINITY,
            cur_low: f64::INFINITY,
            completed: VecDeque::with_capacity(period),
            sum: 0.0,
        })
    }

    /// Configured `(period, utc_offset_minutes)`.
    pub const fn params(&self) -> (usize, i32) {
        (self.period, self.utc_offset_minutes)
    }

    /// Most recent ADR if at least one session has completed.
    pub fn value(&self) -> Option<f64> {
        if self.completed.is_empty() {
            None
        } else {
            Some(self.sum / self.completed.len() as f64)
        }
    }
}

impl Indicator for AverageDailyRange {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let civil = civil_from_timestamp(candle.timestamp, self.utc_offset_minutes);
        let key = (civil.year, civil.month, civil.day);
        match self.day_key {
            Some(prev) if prev == key => {
                if candle.high > self.cur_high {
                    self.cur_high = candle.high;
                }
                if candle.low < self.cur_low {
                    self.cur_low = candle.low;
                }
            }
            Some(_) => {
                let range = self.cur_high - self.cur_low;
                self.completed.push_back(range);
                self.sum += range;
                if self.completed.len() > self.period {
                    self.sum -= self
                        .completed
                        .pop_front()
                        .expect("len > period implies a front element");
                }
                self.day_key = Some(key);
                self.cur_high = candle.high;
                self.cur_low = candle.low;
            }
            None => {
                self.day_key = Some(key);
                self.cur_high = candle.high;
                self.cur_low = candle.low;
            }
        }
        self.value()
    }

    fn reset(&mut self) {
        self.day_key = None;
        self.cur_high = f64::NEG_INFINITY;
        self.cur_low = f64::INFINITY;
        self.completed.clear();
        self.sum = 0.0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        !self.completed.is_empty()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "AverageDailyRange"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    const HOUR: i64 = 3_600_000;
    const DAY: i64 = 24 * HOUR;

    fn c(high: f64, low: f64, ts: i64) -> Candle {
        let mid = f64::midpoint(high, low);
        Candle::new(mid, high, low, mid, 1.0, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            AverageDailyRange::new(0, 0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn metadata_and_accessors() {
        let adr = AverageDailyRange::new(5, -60).unwrap();
        assert_eq!(adr.params(), (5, -60));
        assert_eq!(adr.name(), "AverageDailyRange");
        assert_eq!(adr.warmup_period(), 5);
        assert!(!adr.is_ready());
        assert!(adr.value().is_none());
    }

    #[test]
    fn averages_completed_day_ranges() {
        let mut adr = AverageDailyRange::new(3, 0).unwrap();
        // Day 1: range 10.
        assert!(adr.update(c(110.0, 100.0, 0)).is_none());
        assert!(adr.update(c(108.0, 104.0, HOUR)).is_none());
        // Day 2 opens -> day 1 (range 10) completes.
        let v = adr.update(c(120.0, 110.0, DAY)).unwrap();
        assert_relative_eq!(v, 10.0);
        assert!(adr.is_ready());
        // Day 3 opens -> day 2 (range 10) completes: mean of [10, 10] = 10.
        let v = adr.update(c(130.0, 100.0, 2 * DAY)).unwrap();
        assert_relative_eq!(v, 10.0);
    }

    #[test]
    fn rolls_off_oldest_day_beyond_period() {
        let mut adr = AverageDailyRange::new(2, 0).unwrap();
        adr.update(c(110.0, 100.0, 0)); // day 1 range 10
        let v = adr.update(c(125.0, 110.0, DAY)).unwrap(); // close day 1 -> [10]
        assert_relative_eq!(v, 10.0);
        // Close day 2 (range 125-110=15) -> window [10, 15], mean 12.5.
        let v = adr.update(c(130.0, 110.0, 2 * DAY)).unwrap();
        assert_relative_eq!(v, 12.5);
        // Close day 3 (range 130-110=20) -> window [15, 20], oldest (10) rolled off.
        let v = adr.update(c(140.0, 138.0, 3 * DAY)).unwrap();
        assert_relative_eq!(v, 17.5);
    }

    #[test]
    fn reset_clears_state() {
        let mut adr = AverageDailyRange::new(2, 0).unwrap();
        adr.update(c(110.0, 100.0, 0));
        adr.update(c(120.0, 110.0, DAY));
        adr.reset();
        assert!(!adr.is_ready());
        assert!(adr.value().is_none());
        assert!(adr.update(c(50.0, 40.0, 2 * DAY)).is_none());
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                c(
                    110.0 + f64::from(i % 5),
                    100.0 - f64::from(i % 3),
                    i64::from(i) * 6 * HOUR,
                )
            })
            .collect();
        let mut a = AverageDailyRange::new(4, 0).unwrap();
        let mut b = AverageDailyRange::new(4, 0).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            AverageDailyRange::new(crate::error::MAX_PERIOD + 1, -60),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
