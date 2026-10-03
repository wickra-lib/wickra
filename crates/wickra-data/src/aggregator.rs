//! Roll trade ticks up into candles of an arbitrary timeframe.

use crate::error::{Error, Result};
use wickra_core::{Candle, Tick};

/// Hard cap on the number of placeholder candles a single
/// [`TickAggregator::push`] call may emit when gap-fill is enabled. One
/// million minute-candles is roughly 1.9 years of contiguous one-minute bars
/// — orders of magnitude beyond any realistic missing-data window in
/// production while still keeping the resulting `Vec<Candle>` to well under
/// 50 MB. Any larger gap is treated as malformed input rather than allowed
/// to OOM the process.
pub const MAX_GAP_FILL_CANDLES: i64 = 1_000_000;

/// A candle bucket size measured in the same unit as the tick timestamps.
///
/// Wickra is unit-agnostic about timestamps: choose whichever makes sense for
/// your source (milliseconds for Binance trade events, microseconds for IB,
/// seconds for daily bars).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeframe {
    bucket: i64,
}

impl Timeframe {
    /// Construct a timeframe with the given bucket size in the chosen unit.
    ///
    /// # Errors
    /// Returns [`Error::InvalidTimeframe`] if `bucket <= 0`.
    pub fn new(bucket: i64) -> Result<Self> {
        if bucket <= 0 {
            return Err(Error::InvalidTimeframe(format!(
                "bucket size must be positive, got {bucket}"
            )));
        }
        Ok(Self { bucket })
    }

    /// Convenience: build a millisecond timeframe.
    pub fn millis(ms: i64) -> Result<Self> {
        Self::new(ms)
    }

    /// Convenience: build a seconds-resolution timeframe.
    pub fn seconds(s: i64) -> Result<Self> {
        Self::new(s)
    }

    /// One-minute timeframe in milliseconds (`60_000`).
    pub fn one_minute_ms() -> Self {
        Self::new(60_000).expect("60_000 > 0")
    }

    /// Convenience: build a timeframe of `n` whole minutes, measured in
    /// seconds — consistent with [`Timeframe::seconds`].
    ///
    /// `minutes(5)` yields a bucket of `300`, for use with second-resolution
    /// timestamps. For millisecond timestamps (Binance) multiply yourself or
    /// use [`Timeframe::millis`].
    ///
    /// # Errors
    /// Returns [`Error::InvalidTimeframe`] if `n` is not positive or if
    /// `n * 60` overflows `i64`.
    ///
    /// ```
    /// use wickra_data::aggregator::Timeframe;
    /// assert_eq!(Timeframe::minutes(5)?.bucket(), 300);
    /// # Ok::<(), wickra_data::Error>(())
    /// ```
    pub fn minutes(n: i64) -> Result<Self> {
        let bucket = n
            .checked_mul(60)
            .ok_or_else(|| Error::InvalidTimeframe(format!("{n} minutes overflows i64 seconds")))?;
        Self::new(bucket)
    }

    /// Convenience: build a timeframe of `n` whole hours, measured in seconds
    /// (`hours(2)` → a bucket of `7_200`).
    ///
    /// # Errors
    /// Returns [`Error::InvalidTimeframe`] if `n` is not positive or if
    /// `n * 3_600` overflows `i64`.
    ///
    /// ```
    /// use wickra_data::aggregator::Timeframe;
    /// assert_eq!(Timeframe::hours(2)?.bucket(), 7_200);
    /// # Ok::<(), wickra_data::Error>(())
    /// ```
    pub fn hours(n: i64) -> Result<Self> {
        let bucket = n
            .checked_mul(3_600)
            .ok_or_else(|| Error::InvalidTimeframe(format!("{n} hours overflows i64 seconds")))?;
        Self::new(bucket)
    }

    /// Convenience: build a timeframe of `n` whole days, measured in seconds
    /// (`days(1)` → a bucket of `86_400`).
    ///
    /// # Errors
    /// Returns [`Error::InvalidTimeframe`] if `n` is not positive or if
    /// `n * 86_400` overflows `i64`.
    ///
    /// ```
    /// use wickra_data::aggregator::Timeframe;
    /// assert_eq!(Timeframe::days(1)?.bucket(), 86_400);
    /// # Ok::<(), wickra_data::Error>(())
    /// ```
    pub fn days(n: i64) -> Result<Self> {
        let bucket = n
            .checked_mul(86_400)
            .ok_or_else(|| Error::InvalidTimeframe(format!("{n} days overflows i64 seconds")))?;
        Self::new(bucket)
    }

    /// Bucket size.
    pub const fn bucket(self) -> i64 {
        self.bucket
    }

    /// Floor a raw timestamp to this timeframe's bucket boundary.
    ///
    /// For a timestamp within one bucket of [`i64::MIN`] the mathematically
    /// exact boundary lies below `i64::MIN` and cannot be represented; in that
    /// (practically unreachable) case the result saturates at `i64::MIN`
    /// rather than overflowing and panicking in debug builds. `bucket` is
    /// always positive, so `rem_euclid` itself cannot panic.
    pub fn floor(self, ts: i64) -> i64 {
        ts.saturating_sub(ts.rem_euclid(self.bucket))
    }
}

/// Incrementally builds candles out of arriving ticks.
///
/// Each call to [`TickAggregator::push`] returns the candles that closed as a
/// result of the new tick — normally at most one. Use
/// [`TickAggregator::flush`] at the end of a stream to capture the final open
/// bar.
///
/// # Gaps
///
/// By default a tick that jumps across one or more empty buckets simply opens
/// the next non-empty bar — the skipped buckets produce no candle, so the
/// output series can have time holes. Enable [`TickAggregator::with_gap_fill`]
/// to instead emit a flat placeholder candle for every skipped bucket, giving
/// downstream indicators an unbroken, evenly spaced series. To bound memory
/// against an adversarial timestamp jump, gap-filling refuses to emit more
/// than [`MAX_GAP_FILL_CANDLES`] placeholders in a single step; a larger gap
/// surfaces as an `Error::Malformed` so the caller can decide how to handle
/// the discontinuity.
#[derive(Debug, Clone)]
pub struct TickAggregator {
    timeframe: Timeframe,
    open_bar: Option<OpenBar>,
    fill_gaps: bool,
}

#[derive(Debug, Clone, Copy)]
struct OpenBar {
    bucket_start: i64,
    /// Timestamp of the most recently absorbed tick. Used to reject ticks that
    /// arrive out of order *within* the current bucket — without it an older
    /// tick would silently overwrite `close` with a stale price.
    last_ts: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

impl OpenBar {
    fn from_tick(t: Tick, bucket_start: i64) -> Self {
        Self {
            bucket_start,
            last_ts: t.timestamp,
            open: t.price,
            high: t.price,
            low: t.price,
            close: t.price,
            volume: t.volume,
        }
    }

    fn absorb(&mut self, t: Tick) {
        if t.price > self.high {
            self.high = t.price;
        }
        if t.price < self.low {
            self.low = t.price;
        }
        self.close = t.price;
        self.volume += t.volume;
        self.last_ts = t.timestamp;
    }

    /// Finalise the bar into a validated [`Candle`].
    ///
    /// # Errors
    /// Returns [`Error::Core`] if the accumulated `volume` is no longer finite.
    /// `volume` is summed across every absorbed tick, so an astronomically
    /// long or large run can drift it to `inf`; emitting such a candle would
    /// silently poison every downstream indicator, so it is surfaced instead.
    /// The OHLC fields are finite and correctly ordered by construction, so
    /// `Candle::new` only ever rejects this bar for a non-finite volume.
    fn into_candle(self) -> Result<Candle> {
        Candle::new(
            self.open,
            self.high,
            self.low,
            self.close,
            self.volume,
            self.bucket_start,
        )
        .map_err(Error::from)
    }
}

impl TickAggregator {
    /// Construct a new aggregator for the given timeframe.
    pub fn new(timeframe: Timeframe) -> Self {
        Self {
            timeframe,
            open_bar: None,
            fill_gaps: false,
        }
    }

    /// Enable or disable gap filling, returning the (re)configured aggregator.
    ///
    /// When enabled, [`push`](Self::push) emits a flat candle
    /// (`open == high == low == close`, `volume == 0`) for every bucket that is
    /// skipped between two consecutive ticks. The flat candle's price is the
    /// close of the bar that preceded the gap, so the series stays continuous.
    #[must_use]
    pub fn with_gap_fill(mut self, fill: bool) -> Self {
        self.fill_gaps = fill;
        self
    }

    /// Whether gap filling is enabled.
    pub const fn fills_gaps(&self) -> bool {
        self.fill_gaps
    }

    /// Push a tick. Returns every candle that closed as a result — an empty
    /// vector while the open bar keeps growing, one candle when a bar boundary
    /// is crossed, and (with gap filling enabled) additionally one flat candle
    /// per skipped bucket.
    ///
    /// # Errors
    /// Returns [`Error::Malformed`] if `tick.timestamp` goes backwards — both
    /// across buckets (older than the open bar's start) and within a bucket
    /// (older than the last tick absorbed into it) — or if gap filling
    /// overflows the timestamp range. Ticks sharing a timestamp are accepted.
    pub fn push(&mut self, tick: Tick) -> Result<Vec<Candle>> {
        let bucket = self.timeframe.floor(tick.timestamp);
        if let Some(mut bar) = self.open_bar {
            if bucket < bar.bucket_start {
                return Err(Error::Malformed(format!(
                    "tick timestamp {} is older than the open bar start {}",
                    tick.timestamp, bar.bucket_start
                )));
            }
            if bucket > bar.bucket_start {
                // Close the previous bar and start a new one with this tick.
                let closed = bar.into_candle()?;
                let mut out = Vec::with_capacity(1);
                out.push(closed);
                if self.fill_gaps {
                    self.fill_between(closed, bucket, &mut out)?;
                }
                self.open_bar = Some(OpenBar::from_tick(tick, bucket));
                return Ok(out);
            }
            // Same bucket: reject a tick that predates the last one absorbed,
            // which would otherwise overwrite `close` with a stale price.
            // Equal timestamps are allowed — several trades can share a
            // millisecond.
            if tick.timestamp < bar.last_ts {
                return Err(Error::Malformed(format!(
                    "tick timestamp {} predates the last tick {} in the same bucket",
                    tick.timestamp, bar.last_ts
                )));
            }
            bar.absorb(tick);
            self.open_bar = Some(bar);
            return Ok(Vec::new());
        }
        self.open_bar = Some(OpenBar::from_tick(tick, bucket));
        Ok(Vec::new())
    }

    /// Emit one flat candle per bucket skipped between `prev` and the bucket
    /// starting at `next_bucket`.
    ///
    /// # Errors
    /// Propagates the cap and overflow errors of [`fill_flat_candles`].
    fn fill_between(&self, prev: Candle, next_bucket: i64, out: &mut Vec<Candle>) -> Result<()> {
        fill_flat_candles(prev, next_bucket, self.timeframe.bucket(), out)
    }

    /// Drain the currently open bar (if any) and return it. Useful at the end of
    /// a backtest or when shutting down a live aggregator.
    ///
    /// # Errors
    /// Returns an error if the open bar's accumulated volume is non-finite
    /// (see the internal `OpenBar::into_candle`).
    pub fn flush(&mut self) -> Result<Option<Candle>> {
        self.open_bar.take().map(OpenBar::into_candle).transpose()
    }

    /// Configured timeframe.
    pub const fn timeframe(&self) -> Timeframe {
        self.timeframe
    }
}

/// Emit one flat candle per bucket skipped between `prev` and `next_bucket`.
///
/// Shared by [`TickAggregator`] and [`crate::resample::Resampler`] so the two
/// cannot drift apart: the placeholder price, the zero volume and the cap are
/// one implementation rather than two that happen to agree today.
///
/// Each filler carries `prev.close` in all four OHLC fields and a volume of
/// zero, so the series stays continuous and evenly spaced without inventing
/// price action.
///
/// Returns `Error::Malformed` when the gap would exceed
/// [`MAX_GAP_FILL_CANDLES`] — an adversarial timestamp jump (a clock-glitch
/// out-of-memory panic from allocating millions of placeholder candles.
pub(crate) fn fill_flat_candles(
    prev: Candle,
    next_bucket: i64,
    step: i64,
    out: &mut Vec<Candle>,
) -> Result<()> {
    let start = prev
        .timestamp
        .checked_add(step)
        .ok_or_else(|| Error::Malformed("timestamp overflow while gap-filling".to_string()))?;
    if start >= next_bucket {
        return Ok(());
    }

    // Compute the gap size up-front so an adversarial timestamp delta
    // is refused before we allocate. `step > 0` by `Timeframe::new`'s
    // invariant, so the divisor is safe. Saturating the subtraction
    // makes the arithmetic infallible; an overflowed-saturated span is
    // still far above the cap so the limit check below catches it.
    let span = next_bucket.saturating_sub(start);
    let gap_count = span / step + i64::from(span % step != 0);

    if gap_count > MAX_GAP_FILL_CANDLES {
        return Err(Error::Malformed(format!(
            "gap-fill between bucket {} and {next_bucket} would emit {gap_count} \
             flat candles at step {step}, exceeding the {MAX_GAP_FILL_CANDLES} \
             cap; reject the discontinuity instead of allocating",
            prev.timestamp
        )));
    }

    out.reserve(gap_count as usize);
    // Bucket alignment guarantees start + (gap_count - 1) * step ≤
    // next_bucket - step < i64::MAX, so iterating `gap_count` times
    // with `saturating_add(step)` cannot reach i64::MAX inside the
    // loop body. `prev.close` is finite (it came from a validated
    // bar) and volume is exactly 0.0, so the OHLCV invariants hold
    // by construction — skip re-validation via Candle::new_unchecked.
    let mut t = start;
    for _ in 0..gap_count {
        out.push(Candle::new_unchecked(
            prev.close, prev.close, prev.close, prev.close, 0.0, t,
        ));
        t = t.saturating_add(step);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(price: f64, ts: i64) -> Tick {
        Tick::new(price, 1.0, ts).unwrap()
    }

    #[test]
    fn timeframe_rejects_non_positive() {
        assert!(Timeframe::new(0).is_err());
        assert!(Timeframe::new(-1).is_err());
    }

    /// Cover the `Timeframe::millis`, `Timeframe::seconds`, and
    /// `Timeframe::one_minute_ms` convenience constructors (lines 40-52).
    /// All existing tests build Timeframes via `new` / `minutes` / `hours` /
    /// `days`, never via the three thin convenience wrappers.
    #[test]
    fn timeframe_convenience_constructors() {
        assert_eq!(Timeframe::millis(250).unwrap().bucket(), 250);
        assert!(Timeframe::millis(0).is_err());
        assert_eq!(Timeframe::seconds(30).unwrap().bucket(), 30);
        assert!(Timeframe::seconds(-1).is_err());
        // one_minute_ms is the infallible 60_000-ms shortcut.
        assert_eq!(Timeframe::one_minute_ms().bucket(), 60_000);
    }

    /// Cover the `TickAggregator::timeframe` const accessor (lines 353-355).
    /// Existing tests only inspect emitted candles, never query the
    /// configured timeframe back out.
    #[test]
    fn aggregator_timeframe_getter() {
        let tf = Timeframe::new(60).unwrap();
        let agg = TickAggregator::new(tf);
        assert_eq!(agg.timeframe().bucket(), 60);
    }

    #[test]
    fn minute_hour_day_constructors_compute_seconds() {
        assert_eq!(Timeframe::minutes(1).unwrap().bucket(), 60);
        assert_eq!(Timeframe::minutes(5).unwrap().bucket(), 300);
        assert_eq!(Timeframe::hours(1).unwrap().bucket(), 3_600);
        assert_eq!(Timeframe::hours(4).unwrap().bucket(), 14_400);
        assert_eq!(Timeframe::days(1).unwrap().bucket(), 86_400);
        assert_eq!(Timeframe::days(7).unwrap().bucket(), 604_800);
    }

    #[test]
    fn minute_hour_day_constructors_reject_non_positive() {
        for n in [0, -1, -60] {
            assert!(Timeframe::minutes(n).is_err());
            assert!(Timeframe::hours(n).is_err());
            assert!(Timeframe::days(n).is_err());
        }
    }

    #[test]
    fn minute_hour_day_constructors_reject_overflow() {
        // `n * unit` overflows i64 long before `new`'s sign check runs.
        assert!(matches!(
            Timeframe::minutes(i64::MAX),
            Err(Error::InvalidTimeframe(_))
        ));
        assert!(matches!(
            Timeframe::hours(i64::MAX),
            Err(Error::InvalidTimeframe(_))
        ));
        assert!(matches!(
            Timeframe::days(i64::MAX),
            Err(Error::InvalidTimeframe(_))
        ));
    }

    #[test]
    fn floors_to_bucket_boundary() {
        let tf = Timeframe::new(100).unwrap();
        assert_eq!(tf.floor(0), 0);
        assert_eq!(tf.floor(99), 0);
        assert_eq!(tf.floor(100), 100);
        assert_eq!(tf.floor(150), 100);
        assert_eq!(tf.floor(250), 200);
        // Negative timestamps still floor toward negative infinity.
        assert_eq!(tf.floor(-1), -100);
        assert_eq!(tf.floor(-100), -100);
        assert_eq!(tf.floor(-101), -200);
    }

    #[test]
    fn floor_saturates_instead_of_overflowing_at_min() {
        let tf = Timeframe::new(100).unwrap();
        // The exact boundary lies below i64::MIN — must not panic.
        assert_eq!(tf.floor(i64::MIN), i64::MIN);
        // i64::MAX must not overflow either (subtracting a non-negative).
        let hi = tf.floor(i64::MAX);
        assert!(hi > i64::MAX - 100 && hi % 100 == 0);
    }

    #[test]
    fn aggregates_ticks_into_one_candle_within_bucket() {
        let mut agg = TickAggregator::new(Timeframe::new(60).unwrap());
        assert_eq!(agg.push(t(10.0, 0)).unwrap(), Vec::new());
        assert_eq!(agg.push(t(12.0, 15)).unwrap(), Vec::new());
        assert_eq!(agg.push(t(8.0, 30)).unwrap(), Vec::new());
        assert_eq!(agg.push(t(11.0, 50)).unwrap(), Vec::new());
        let bar = agg.flush().unwrap().expect("open bar");
        assert_eq!(bar.open, 10.0);
        assert_eq!(bar.high, 12.0);
        assert_eq!(bar.low, 8.0);
        assert_eq!(bar.close, 11.0);
        assert!((bar.volume - 4.0).abs() < 1e-12);
        assert_eq!(bar.timestamp, 0);
    }

    #[test]
    fn emits_candle_on_bucket_crossing() {
        let mut agg = TickAggregator::new(Timeframe::new(60).unwrap());
        agg.push(t(10.0, 0)).unwrap();
        agg.push(t(12.0, 30)).unwrap();
        let closed = agg.push(t(15.0, 60)).unwrap();
        assert_eq!(closed.len(), 1);
        let closed = closed[0];
        assert_eq!(closed.open, 10.0);
        assert_eq!(closed.high, 12.0);
        assert_eq!(closed.low, 10.0);
        assert_eq!(closed.close, 12.0);

        // The new tick at ts=60 opens the next bar.
        let still_open = agg.flush().unwrap().unwrap();
        assert_eq!(still_open.open, 15.0);
        assert_eq!(still_open.timestamp, 60);
    }

    #[test]
    fn rejects_out_of_order_ticks() {
        let mut agg = TickAggregator::new(Timeframe::new(60).unwrap());
        agg.push(t(10.0, 100)).unwrap();
        let err = agg.push(t(11.0, 30)).unwrap_err();
        assert!(matches!(err, Error::Malformed(_)));
    }

    #[test]
    fn rejects_same_bucket_out_of_order_tick() {
        let mut agg = TickAggregator::new(Timeframe::new(60).unwrap());
        agg.push(t(10.0, 50)).unwrap();
        // ts=10 is still bucket 0 but predates the tick at ts=50 — rejecting
        // it prevents a stale price silently overwriting `close`.
        let err = agg.push(t(99.0, 10)).unwrap_err();
        assert!(matches!(err, Error::Malformed(_)));
        // The open bar is untouched: close is still the ts=50 price.
        assert_eq!(agg.flush().unwrap().unwrap().close, 10.0);
    }

    #[test]
    fn accepts_same_bucket_ticks_sharing_a_timestamp() {
        let mut agg = TickAggregator::new(Timeframe::new(60).unwrap());
        agg.push(t(10.0, 20)).unwrap();
        // Two trades in the same millisecond are legitimate.
        agg.push(t(12.0, 20)).unwrap();
        agg.push(t(11.0, 20)).unwrap();
        let bar = agg.flush().unwrap().unwrap();
        assert_eq!(bar.high, 12.0);
        assert_eq!(bar.close, 11.0);
    }

    #[test]
    fn flushes_a_non_finite_volume_as_an_error() {
        let mut agg = TickAggregator::new(Timeframe::new(60).unwrap());
        // Two near-max volumes sum to +inf — the closed candle would carry a
        // non-finite volume that poisons every downstream indicator.
        agg.push(Tick::new(10.0, f64::MAX, 0).unwrap()).unwrap();
        agg.push(Tick::new(10.0, f64::MAX, 1).unwrap()).unwrap();
        let err = agg.flush().unwrap_err();
        assert!(matches!(err, Error::Core(_)));
    }

    #[test]
    fn skips_empty_buckets_without_gap_fill() {
        let mut agg = TickAggregator::new(Timeframe::new(60).unwrap());
        assert!(!agg.fills_gaps());
        agg.push(t(10.0, 0)).unwrap();
        // Jump from bucket 0 straight to bucket 180 — buckets 60 and 120 empty.
        let closed = agg.push(t(20.0, 200)).unwrap();
        assert_eq!(closed.len(), 1, "only the real bar closes");
        assert_eq!(closed[0].timestamp, 0);
    }

    #[test]
    fn gap_fill_rejects_runaway_timestamp_jump() {
        // An adversarial clock-glitch tick years in the future must surface
        // as an Error::Malformed rather than allocating millions of flat
        // candles and OOMing. Found by the `tick_aggregator` fuzz target.
        let mut agg = TickAggregator::new(Timeframe::new(60).unwrap()).with_gap_fill(true);
        agg.push(t(10.0, 0)).unwrap();
        // Two-billion-second jump = ~63 years of minute bars = ~33 million
        // candles, well above the 1_000_000 cap.
        let err = agg.push(t(20.0, 2_000_000_000)).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("gap-fill") && msg.contains("cap"),
            "expected a malformed-gap error, got: {msg}"
        );
    }

    #[test]
    fn gap_fill_at_the_cap_succeeds() {
        // Exactly one million minute-buckets between the two ticks (one real
        // bar + one million flat fillers + the third tick's open bar) — the
        // limit is inclusive, so this must succeed.
        let mut agg = TickAggregator::new(Timeframe::new(60).unwrap()).with_gap_fill(true);
        agg.push(t(10.0, 0)).unwrap();
        // bucket 0 closes; jump straight to bucket 60_000_060 (1_000_001 buckets
        // away). fill_between emits 1_000_000 flat candles between them, then
        // the new tick opens its own bucket. Output: 1 real bar + 1_000_000 fillers.
        let out = agg.push(t(20.0, 60_000_060)).unwrap();
        assert_eq!(out.len(), 1 + MAX_GAP_FILL_CANDLES as usize);
    }

    #[test]
    fn gap_fill_emits_flat_candles_for_skipped_buckets() {
        let mut agg = TickAggregator::new(Timeframe::new(60).unwrap()).with_gap_fill(true);
        assert!(agg.fills_gaps());
        agg.push(t(10.0, 0)).unwrap();
        agg.push(t(13.0, 30)).unwrap(); // still bucket 0, close = 13.0
                                        // Next tick lands in bucket 180 — buckets 60 and 120 are skipped.
        let out = agg.push(t(20.0, 200)).unwrap();
        assert_eq!(out.len(), 3, "real bar + two flat fillers");

        let real = out[0];
        assert_eq!(real.timestamp, 0);
        assert_eq!(real.close, 13.0);

        for (filler, ts) in out[1..].iter().zip([60, 120]) {
            assert_eq!(filler.timestamp, ts);
            assert_eq!(filler.open, 13.0);
            assert_eq!(filler.high, 13.0);
            assert_eq!(filler.low, 13.0);
            assert_eq!(filler.close, 13.0);
            assert_eq!(filler.volume, 0.0);
        }

        // The tick at ts=200 opens bucket 180.
        assert_eq!(agg.flush().unwrap().unwrap().timestamp, 180);
    }

    #[test]
    fn gap_fill_emits_nothing_extra_for_adjacent_buckets() {
        let mut agg = TickAggregator::new(Timeframe::new(60).unwrap()).with_gap_fill(true);
        agg.push(t(10.0, 0)).unwrap();
        // Bucket 60 directly follows bucket 0 — no gap to fill.
        let out = agg.push(t(11.0, 70)).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].timestamp, 0);
    }
}
