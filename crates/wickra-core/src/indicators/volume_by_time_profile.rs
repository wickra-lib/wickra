//! Volume-by-Time Profile — the mean traded volume in each intraday bucket.

use crate::calendar::civil_from_timestamp;
use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Volume-by-Time Profile output: the per-bucket mean volume.
///
/// `bins[i]` is the mean volume of all bars whose local time-of-day fell in
/// bucket `i`. Empty buckets read `0.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeByTimeProfileOutput {
    /// Per-bucket mean volume, earliest bucket first. Length equals `buckets`.
    pub bins: Vec<f64>,
}

/// Mean traded volume bucketed by local time of day.
///
/// The local day (the wall-clock day of [`Candle::timestamp`](crate::Candle)
/// shifted by `utc_offset_minutes`) is split into `buckets` equal slices. Each
/// bar's volume is accumulated into the bucket of its time-of-day, and the
/// profile reports the running mean volume per bucket. Unlike the return
/// profiles, the first bar already produces output (volume needs no prior bar).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, VolumeByTimeProfile};
///
/// let hour = 3_600_000;
/// let mut prof = VolumeByTimeProfile::new(24, 0).unwrap();
/// let out = prof.update(Candle::new(100.0, 100.0, 100.0, 100.0, 500.0, hour).unwrap()).unwrap();
/// assert_eq!(out.bins.len(), 24);
/// assert_eq!(out.bins[1], 500.0);
/// ```
#[derive(Debug, Clone)]
pub struct VolumeByTimeProfile {
    buckets: usize,
    utc_offset_minutes: i32,
    sum: Vec<f64>,
    count: Vec<u64>,
    last: Option<VolumeByTimeProfileOutput>,
}

impl VolumeByTimeProfile {
    ///
    /// The offset is a constant and does not follow daylight saving: for a
    /// venue that observes it, one value is correct for part of the year and an
    /// hour out for the rest, which shifts every session boundary by an hour.
    /// Either pass the offset in force for the span being analysed and keep
    /// spans that cross a transition apart, or convert the timestamps to the
    /// venue's wall clock upstream and pass `0`.
    /// Construct a Volume-by-Time Profile with `buckets` intraday slices.
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

    /// Most recent profile if at least one bar has been seen.
    pub fn value(&self) -> Option<&VolumeByTimeProfileOutput> {
        self.last.as_ref()
    }

    fn bucket_of(&self, minute_of_day: u32) -> usize {
        let raw = (minute_of_day as usize * self.buckets) / 1440;
        raw.min(self.buckets - 1)
    }

    fn snapshot(&self) -> VolumeByTimeProfileOutput {
        let bins = self
            .sum
            .iter()
            .zip(&self.count)
            .map(|(total, n)| if *n > 0 { total / *n as f64 } else { 0.0 })
            .collect();
        VolumeByTimeProfileOutput { bins }
    }
}

impl Indicator for VolumeByTimeProfile {
    type Input = Candle;
    type Output = VolumeByTimeProfileOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<VolumeByTimeProfileOutput> {
        let civil = civil_from_timestamp(candle.timestamp, self.utc_offset_minutes);
        let bucket = self.bucket_of(civil.minute_of_day());
        self.sum[bucket] += candle.volume;
        self.count[bucket] += 1;
        let out = self.snapshot();
        self.last = Some(out.clone());
        Some(out)
    }

    fn reset(&mut self) {
        self.sum.iter_mut().for_each(|x| *x = 0.0);
        self.count.iter_mut().for_each(|x| *x = 0);
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "VolumeByTimeProfile"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_matches_the_emitted_payload() {
        // The point of `width` is that a caller can size a buffer before
        // the first value arrives, so it has to equal what update emits.
        let mut ind = VolumeByTimeProfile::new(6, 0).unwrap();
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

    fn c(volume: f64, ts: i64) -> Candle {
        Candle::new(100.0, 100.0, 100.0, 100.0, volume, ts).unwrap()
    }

    #[test]
    fn rejects_zero_buckets() {
        assert!(matches!(
            VolumeByTimeProfile::new(0, 0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn metadata_and_accessors() {
        let prof = VolumeByTimeProfile::new(24, -60).unwrap();
        assert_eq!(prof.params(), (24, -60));
        assert_eq!(prof.name(), "VolumeByTimeProfile");
        assert_eq!(prof.warmup_period(), 1);
        assert!(!prof.is_ready());
        assert!(prof.value().is_none());
    }

    #[test]
    fn emits_from_first_bar_and_means_volume() {
        let mut prof = VolumeByTimeProfile::new(24, 0).unwrap();
        let out = prof.update(c(500.0, HOUR)).unwrap(); // 01:00 -> bucket 1
        assert_eq!(out.bins.len(), 24);
        assert_relative_eq!(out.bins[1], 500.0);
        assert_relative_eq!(out.bins[0], 0.0);
        assert!(prof.is_ready());
        // Next day 01:00, volume 700 -> mean (500 + 700) / 2 = 600.
        let out = prof.update(c(700.0, 25 * HOUR)).unwrap();
        assert_relative_eq!(out.bins[1], 600.0);
    }

    #[test]
    fn last_bucket_clamped() {
        let mut prof = VolumeByTimeProfile::new(24, 0).unwrap();
        // 23:59 -> minute 1439 -> bucket 23.
        let out = prof.update(c(300.0, 23 * HOUR + 59 * 60_000)).unwrap();
        assert_relative_eq!(out.bins[23], 300.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut prof = VolumeByTimeProfile::new(24, 0).unwrap();
        prof.update(c(500.0, HOUR));
        prof.reset();
        assert!(!prof.is_ready());
        assert!(prof.value().is_none());
        let out = prof.update(c(100.0, 2 * HOUR)).unwrap();
        assert_relative_eq!(out.bins[2], 100.0);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..50)
            .map(|i| c(100.0 + f64::from(i % 8), i64::from(i) * HOUR))
            .collect();
        let mut a = VolumeByTimeProfile::new(12, 0).unwrap();
        let mut b = VolumeByTimeProfile::new(12, 0).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_buckets_above_maximum() {
        assert!(matches!(
            VolumeByTimeProfile::new(crate::error::MAX_PERIOD + 1, 0),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
