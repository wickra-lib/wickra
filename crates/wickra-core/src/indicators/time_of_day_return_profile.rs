//! Time-of-Day Return Profile — the mean bar return in each intraday time bucket.

use crate::calendar::civil_from_timestamp;
use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Time-of-Day Return Profile output: the per-bucket mean return.
///
/// `bins[i]` is the mean simple return of all bars whose local time-of-day fell
/// in bucket `i`, where bucket `i` spans the minutes
/// `[i * 1440 / bins.len(), (i + 1) * 1440 / bins.len())`. Empty buckets read
/// `0.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeOfDayReturnProfileOutput {
    /// Per-bucket mean return, earliest bucket first. Length equals `buckets`.
    pub bins: Vec<f64>,
}

/// Mean bar return bucketed by local time of day.
///
/// The local day (the wall-clock day of [`Candle::timestamp`](crate::Candle)
/// shifted by `utc_offset_minutes`) is divided into `buckets` equal slices. Each
/// bar's simple return `close / previous_close - 1` is accumulated into the bucket
/// of its time-of-day, and the profile reports the running mean per bucket. The
/// first bar produces no output (no return yet).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, TimeOfDayReturnProfile};
///
/// let hour = 3_600_000;
/// let mut prof = TimeOfDayReturnProfile::new(24, 0).unwrap();
/// assert!(prof.update(Candle::new(100.0, 100.0, 100.0, 100.0, 1.0, 0).unwrap()).is_none());
/// let out = prof.update(Candle::new(101.0, 101.0, 101.0, 101.0, 1.0, hour).unwrap()).unwrap();
/// assert_eq!(out.bins.len(), 24);
/// ```
#[derive(Debug, Clone)]
pub struct TimeOfDayReturnProfile {
    buckets: usize,
    utc_offset_minutes: i32,
    prev_close: Option<f64>,
    sum: Vec<f64>,
    count: Vec<u64>,
    last: Option<TimeOfDayReturnProfileOutput>,
}

impl TimeOfDayReturnProfile {
    ///
    /// The offset is a constant and does not follow daylight saving: for a
    /// venue that observes it, one value is correct for part of the year and an
    /// hour out for the rest, which shifts every session boundary by an hour.
    /// Either pass the offset in force for the span being analysed and keep
    /// spans that cross a transition apart, or convert the timestamps to the
    /// venue's wall clock upstream and pass `0`.
    /// Construct a Time-of-Day Return Profile with `buckets` intraday slices.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `buckets == 0`.
    pub fn new(buckets: usize, utc_offset_minutes: i32) -> Result<Self> {
        if buckets == 0 {
            return Err(Error::PeriodZero);
        }
        if buckets > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            buckets,
            utc_offset_minutes,
            prev_close: None,
            sum: vec![0.0; buckets],
            count: vec![0; buckets],
            last: None,
        })
    }

    /// Configured `(buckets, utc_offset_minutes)`.
    pub const fn params(&self) -> (usize, i32) {
        (self.buckets, self.utc_offset_minutes)
    }

    /// How many values every emitted profile carries -- the bucket count fixed at construction.
    ///
    /// Fixed for the lifetime of the indicator, so a caller can size a
    /// buffer once instead of guessing.
    pub const fn width(&self) -> usize {
        self.buckets
    }

    /// Most recent profile if at least one return has been recorded.
    pub fn value(&self) -> Option<&TimeOfDayReturnProfileOutput> {
        self.last.as_ref()
    }

    fn bucket_of(&self, minute_of_day: u32) -> usize {
        let raw = (minute_of_day as usize * self.buckets) / 1440;
        raw.min(self.buckets - 1)
    }

    fn snapshot(&self) -> TimeOfDayReturnProfileOutput {
        let bins = self
            .sum
            .iter()
            .zip(&self.count)
            .map(|(total, n)| if *n > 0 { total / *n as f64 } else { 0.0 })
            .collect();
        TimeOfDayReturnProfileOutput { bins }
    }
}

impl Indicator for TimeOfDayReturnProfile {
    type Input = Candle;
    type Output = TimeOfDayReturnProfileOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<TimeOfDayReturnProfileOutput> {
        let civil = civil_from_timestamp(candle.timestamp, self.utc_offset_minutes);
        let result = if let Some(prev) = self.prev_close {
            let ret = if prev == 0.0 {
                0.0
            } else {
                candle.close / prev - 1.0
            };
            let bucket = self.bucket_of(civil.minute_of_day());
            self.sum[bucket] += ret;
            self.count[bucket] += 1;
            let out = self.snapshot();
            self.last = Some(out.clone());
            Some(out)
        } else {
            None
        };
        self.prev_close = Some(candle.close);
        result
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.sum.iter_mut().for_each(|x| *x = 0.0);
        self.count.iter_mut().for_each(|x| *x = 0);
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
        "TimeOfDayReturnProfile"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_matches_the_emitted_payload() {
        // The point of `width` is that a caller can size a buffer before
        // the first value arrives, so it has to equal what update emits.
        let mut ind = TimeOfDayReturnProfile::new(24, 0).unwrap();
        let width = ind.width();
        let mut emitted = 0;
        for i in 0..40 {
            #[allow(clippy::cast_precision_loss)]
            let step = i as f64;
            let price = 100.0 + step;
            let candle =
                Candle::new(price, price + 1.0, price - 1.0, price, 10.0, i * 3_600_000).unwrap();
            if let Some(out) = ind.update(candle) {
                assert_eq!(out.bins.len(), width);
                emitted += 1;
            }
        }
        assert!(emitted > 0, "the fixture must clear warmup");
    }
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    const HOUR: i64 = 3_600_000;

    fn c(close: f64, ts: i64) -> Candle {
        Candle::new(close, close, close, close, 1.0, ts).unwrap()
    }

    #[test]
    fn rejects_zero_buckets() {
        assert!(matches!(
            TimeOfDayReturnProfile::new(0, 0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn metadata_and_accessors() {
        let prof = TimeOfDayReturnProfile::new(24, -300).unwrap();
        assert_eq!(prof.params(), (24, -300));
        assert_eq!(prof.name(), "TimeOfDayReturnProfile");
        assert_eq!(prof.warmup_period(), 2);
        assert!(!prof.is_ready());
        assert!(prof.value().is_none());
    }

    #[test]
    fn buckets_by_hour_and_means_returns() {
        let mut prof = TimeOfDayReturnProfile::new(24, 0).unwrap();
        assert!(prof.update(c(100.0, 0)).is_none()); // 00:00, no return
                                                     // 01:00 return +0.01 -> bucket 1.
        let out = prof.update(c(101.0, HOUR)).unwrap();
        assert_eq!(out.bins.len(), 24);
        assert_relative_eq!(out.bins[1], 0.01);
        assert_relative_eq!(out.bins[0], 0.0);
        assert!(prof.is_ready());
        // 01:00 next day, return -> averages into bucket 1.
        let out = prof.update(c(102.01, 25 * HOUR)).unwrap();
        // two returns in bucket 1: 0.01 and 0.01 -> mean 0.01.
        assert_relative_eq!(out.bins[1], 0.01);
    }

    #[test]
    fn last_bucket_clamped_for_end_of_day() {
        let mut prof = TimeOfDayReturnProfile::new(24, 0).unwrap();
        prof.update(c(100.0, 23 * HOUR));
        // 23:59 -> minute 1439 -> bucket min(23, 23) = 23.
        let out = prof.update(c(110.0, 23 * HOUR + 59 * 60_000)).unwrap();
        assert_relative_eq!(out.bins[23], 0.10);
    }

    #[test]
    fn zero_prev_close_uses_zero_return() {
        let mut prof = TimeOfDayReturnProfile::new(4, 0).unwrap();
        prof.update(c(0.0, 0));
        let out = prof.update(c(5.0, HOUR)).unwrap();
        assert_relative_eq!(out.bins[0], 0.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut prof = TimeOfDayReturnProfile::new(24, 0).unwrap();
        prof.update(c(100.0, 0));
        prof.update(c(101.0, HOUR));
        prof.reset();
        assert!(!prof.is_ready());
        assert!(prof.value().is_none());
        assert!(prof.update(c(100.0, 2 * HOUR)).is_none());
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..50)
            .map(|i| c(100.0 + f64::from(i % 7), i64::from(i) * HOUR))
            .collect();
        let mut a = TimeOfDayReturnProfile::new(12, 0).unwrap();
        let mut b = TimeOfDayReturnProfile::new(12, 0).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_buckets_above_maximum() {
        assert!(matches!(
            TimeOfDayReturnProfile::new(crate::error::MAX_PERIOD + 1, 0),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
