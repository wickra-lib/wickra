//! Intraday Volatility Profile — the return volatility in each intraday bucket.

use crate::calendar::civil_from_timestamp;
use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Intraday Volatility Profile output: the per-bucket return standard deviation.
///
/// `bins[i]` is the sample standard deviation of the simple returns of all bars
/// whose local time-of-day fell in bucket `i`. Buckets with fewer than two
/// samples read `0.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct IntradayVolatilityProfileOutput {
    /// Per-bucket return standard deviation, earliest bucket first.
    pub bins: Vec<f64>,
}

/// Return volatility bucketed by local time of day.
///
/// The local day (the wall-clock day of [`Candle::timestamp`](crate::Candle)
/// shifted by `utc_offset_minutes`) is split into `buckets` equal slices. Each
/// bar's simple return `close / previous_close - 1` updates the per-bucket
/// running variance (Welford), and the profile reports the per-bucket sample
/// standard deviation. The first bar produces no output.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, IntradayVolatilityProfile};
///
/// let hour = 3_600_000;
/// let mut prof = IntradayVolatilityProfile::new(24, 0).unwrap();
/// assert!(prof.update(Candle::new(100.0, 100.0, 100.0, 100.0, 1.0, 0).unwrap()).is_none());
/// let out = prof.update(Candle::new(101.0, 101.0, 101.0, 101.0, 1.0, hour).unwrap()).unwrap();
/// assert_eq!(out.bins.len(), 24);
/// ```
#[derive(Debug, Clone)]
pub struct IntradayVolatilityProfile {
    buckets: usize,
    utc_offset_minutes: i32,
    prev_close: Option<f64>,
    count: Vec<u64>,
    mean: Vec<f64>,
    m2: Vec<f64>,
    last: Option<IntradayVolatilityProfileOutput>,
}

impl IntradayVolatilityProfile {
    ///
    /// The offset is a constant and does not follow daylight saving: for a
    /// venue that observes it, one value is correct for part of the year and an
    /// hour out for the rest, which shifts every session boundary by an hour.
    /// Either pass the offset in force for the span being analysed and keep
    /// spans that cross a transition apart, or convert the timestamps to the
    /// venue's wall clock upstream and pass `0`.
    /// Construct an Intraday Volatility Profile with `buckets` intraday slices.
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
            count: vec![0; buckets],
            mean: vec![0.0; buckets],
            m2: vec![0.0; buckets],
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
    pub fn value(&self) -> Option<&IntradayVolatilityProfileOutput> {
        self.last.as_ref()
    }

    fn bucket_of(&self, minute_of_day: u32) -> usize {
        let raw = (minute_of_day as usize * self.buckets) / 1440;
        raw.min(self.buckets - 1)
    }

    fn snapshot(&self) -> IntradayVolatilityProfileOutput {
        let bins = self
            .count
            .iter()
            .zip(&self.m2)
            .map(|(n, m2)| {
                if *n >= 2 {
                    (m2 / (*n - 1) as f64).sqrt()
                } else {
                    0.0
                }
            })
            .collect();
        IntradayVolatilityProfileOutput { bins }
    }
}

impl Indicator for IntradayVolatilityProfile {
    type Input = Candle;
    type Output = IntradayVolatilityProfileOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<IntradayVolatilityProfileOutput> {
        let civil = civil_from_timestamp(candle.timestamp, self.utc_offset_minutes);
        let result = if let Some(prev) = self.prev_close {
            let ret = if prev == 0.0 {
                0.0
            } else {
                candle.close / prev - 1.0
            };
            let bucket = self.bucket_of(civil.minute_of_day());
            self.count[bucket] += 1;
            let delta = ret - self.mean[bucket];
            self.mean[bucket] += delta / self.count[bucket] as f64;
            let delta2 = ret - self.mean[bucket];
            self.m2[bucket] += delta * delta2;
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
        self.count.iter_mut().for_each(|x| *x = 0);
        self.mean.iter_mut().for_each(|x| *x = 0.0);
        self.m2.iter_mut().for_each(|x| *x = 0.0);
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
        "IntradayVolatilityProfile"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_matches_the_emitted_payload() {
        // The point of `width` is that a caller can size a buffer before
        // the first value arrives, so it has to equal what update emits.
        let mut ind = IntradayVolatilityProfile::new(12, 0).unwrap();
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
    const DAY: i64 = 24 * HOUR;

    fn c(close: f64, ts: i64) -> Candle {
        Candle::new(close, close, close, close, 1.0, ts).unwrap()
    }

    #[test]
    fn rejects_zero_buckets() {
        assert!(matches!(
            IntradayVolatilityProfile::new(0, 0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn metadata_and_accessors() {
        let prof = IntradayVolatilityProfile::new(24, 90).unwrap();
        assert_eq!(prof.params(), (24, 90));
        assert_eq!(prof.name(), "IntradayVolatilityProfile");
        assert_eq!(prof.warmup_period(), 2);
        assert!(!prof.is_ready());
        assert!(prof.value().is_none());
    }

    #[test]
    fn single_sample_bucket_has_zero_vol() {
        let mut prof = IntradayVolatilityProfile::new(24, 0).unwrap();
        assert!(prof.update(c(100.0, 0)).is_none());
        let out = prof.update(c(101.0, HOUR)).unwrap();
        assert_eq!(out.bins.len(), 24);
        assert_relative_eq!(out.bins[1], 0.0); // only one sample in bucket 1
        assert!(prof.is_ready());
    }

    #[test]
    fn std_matches_manual_two_samples() {
        let mut prof = IntradayVolatilityProfile::new(24, 0).unwrap();
        prof.update(c(100.0, 0)); // 00:00
        prof.update(c(101.0, HOUR)); // 01:00 r=0.01 into bucket 1
                                     // Next day 01:00, r2 = 0.03 into bucket 1.
        let out = prof.update(c(101.0 * 1.03, 25 * HOUR)).unwrap();
        // sample std of {0.01, 0.03} = sqrt(((.01-.02)^2+(.03-.02)^2)/1) = 0.01414..
        let mean = 0.02;
        let expected = (((0.01_f64 - mean).powi(2) + (0.03 - mean).powi(2)) / 1.0).sqrt();
        assert_relative_eq!(out.bins[1], expected, epsilon = 1e-9);
    }

    #[test]
    fn zero_prev_close_uses_zero_return() {
        let mut prof = IntradayVolatilityProfile::new(4, 0).unwrap();
        prof.update(c(0.0, 0));
        let out = prof.update(c(5.0, HOUR)).unwrap();
        assert_relative_eq!(out.bins[0], 0.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut prof = IntradayVolatilityProfile::new(24, 0).unwrap();
        prof.update(c(100.0, 0));
        prof.update(c(101.0, HOUR));
        prof.reset();
        assert!(!prof.is_ready());
        assert!(prof.value().is_none());
        assert!(prof.update(c(100.0, DAY)).is_none());
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..50)
            .map(|i| c(100.0 + f64::from(i % 6), i64::from(i) * HOUR))
            .collect();
        let mut a = IntradayVolatilityProfile::new(12, 0).unwrap();
        let mut b = IntradayVolatilityProfile::new(12, 0).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_buckets_above_maximum() {
        assert!(matches!(
            IntradayVolatilityProfile::new(crate::error::MAX_PERIOD + 1, 0),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
