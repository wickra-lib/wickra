//! Resample an existing candle stream from a finer timeframe to a coarser one.

use crate::aggregator::{fill_flat_candles, Timeframe};
use crate::error::{Error, Result};
use wickra_core::Candle;

/// Roll a stream of candles up to a coarser timeframe.
///
/// Used to derive 5m bars from a 1m feed, or 1h bars from 5m bars, without
/// touching the original tick stream. The output timeframe's bucket must be a
/// strict multiple of the input timeframe's bucket, but this is not enforced
/// — callers are responsible for picking sensible aggregations.
///
/// An input series with holes produces an output series with holes: a bucket
/// that receives no candle is simply not emitted. Enable
/// [`with_gap_fill`](Self::with_gap_fill) to emit a flat placeholder for every
/// skipped bucket instead, exactly as
/// [`TickAggregator`](crate::aggregator::TickAggregator) does.
#[derive(Debug, Clone)]
pub struct Resampler {
    timeframe: Timeframe,
    open: Option<RolledBar>,
    fill_gaps: bool,
}

#[derive(Debug, Clone, Copy)]
struct RolledBar {
    bucket_start: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

impl RolledBar {
    fn from_candle(c: Candle, bucket_start: i64) -> Self {
        Self {
            bucket_start,
            open: c.open,
            high: c.high,
            low: c.low,
            close: c.close,
            volume: c.volume,
        }
    }

    fn absorb(&mut self, c: Candle) {
        if c.high > self.high {
            self.high = c.high;
        }
        if c.low < self.low {
            self.low = c.low;
        }
        self.close = c.close;
        self.volume += c.volume;
    }

    /// Finalise the rolled bar into a validated [`Candle`].
    ///
    /// # Errors
    /// Returns [`Error::Core`] if the accumulated `volume` is no longer finite.
    /// `volume` is summed across every absorbed candle, so a long or large run
    /// can drift it to `inf`; emitting such a candle would silently poison
    /// every downstream indicator, so it is surfaced instead. The OHLC fields
    /// are finite and correctly ordered by construction.
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

impl Resampler {
    /// Build a resampler targeting the given output timeframe.
    pub fn new(timeframe: Timeframe) -> Self {
        Self {
            timeframe,
            open: None,
            fill_gaps: false,
        }
    }

    /// Enable or disable gap filling, returning the (re)configured resampler.
    ///
    /// When enabled, [`push`](Self::push) emits a flat candle
    /// (`open == high == low == close`, `volume == 0`) for every output bucket
    /// that no input candle fell into. The flat candle's price is the close of
    /// the bar that preceded the gap, so the series stays continuous. This is
    /// the same behaviour, and the same implementation, as
    /// [`TickAggregator::with_gap_fill`](crate::aggregator::TickAggregator::with_gap_fill).
    #[must_use]
    pub fn with_gap_fill(mut self, fill: bool) -> Self {
        self.fill_gaps = fill;
        self
    }

    /// Whether gap filling is enabled.
    pub const fn fills_gaps(&self) -> bool {
        self.fill_gaps
    }

    /// Push a finer-grained candle. Returns every coarser candle that closed as
    /// a result — an empty vector while the open bar keeps growing, one candle
    /// when a bucket boundary is crossed, and (with gap filling enabled)
    /// additionally one flat candle per skipped bucket.
    ///
    /// # Errors
    /// Returns [`Error::Malformed`] if `candle.timestamp` falls into a bucket
    /// strictly before the currently open bar — out-of-order candles are not
    /// supported, matching [`crate::aggregator::TickAggregator::push`] — or if
    /// gap filling would exceed
    /// [`MAX_GAP_FILL_CANDLES`](crate::aggregator::MAX_GAP_FILL_CANDLES).
    pub fn push(&mut self, candle: Candle) -> Result<Vec<Candle>> {
        let bucket = self.timeframe.floor(candle.timestamp);
        match self.open {
            Some(mut bar) if bucket == bar.bucket_start => {
                bar.absorb(candle);
                self.open = Some(bar);
                Ok(Vec::new())
            }
            Some(bar) if bucket > bar.bucket_start => {
                let closed = bar.into_candle()?;
                let mut out = vec![closed];
                if self.fill_gaps {
                    fill_flat_candles(closed, bucket, self.timeframe.bucket(), &mut out)?;
                }
                self.open = Some(RolledBar::from_candle(candle, bucket));
                Ok(out)
            }
            Some(bar) => Err(Error::Malformed(format!(
                "candle timestamp {} is older than the open bar start {}",
                candle.timestamp, bar.bucket_start
            ))),
            None => {
                self.open = Some(RolledBar::from_candle(candle, bucket));
                Ok(Vec::new())
            }
        }
    }

    /// Flush the currently open coarser bar, if any.
    ///
    /// # Errors
    /// Returns an error if the open bar's accumulated volume is non-finite
    /// (see the internal `RolledBar::into_candle`).
    pub fn flush(&mut self) -> Result<Option<Candle>> {
        self.open.take().map(RolledBar::into_candle).transpose()
    }
}

/// Roll an entire iterator of candles into a `Vec` of coarser candles. The final
/// open bar (if any) is appended via [`Resampler::flush`].
pub fn resample_all<I>(timeframe: Timeframe, iter: I) -> Result<Vec<Candle>>
where
    I: IntoIterator<Item = Result<Candle>>,
{
    let mut r = Resampler::new(timeframe);
    let mut out = Vec::new();
    for c in iter {
        let c = c?;
        out.extend(r.push(c)?);
    }
    if let Some(last) = r.flush()? {
        out.push(last);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(ts: i64, o: f64, h: f64, l: f64, cl: f64, v: f64) -> Candle {
        Candle::new(o, h, l, cl, v, ts).unwrap()
    }

    #[test]
    fn resamples_1m_to_5m() {
        let tf = Timeframe::new(5).unwrap();
        let one_m = vec![
            c(0, 10.0, 11.0, 9.0, 10.5, 10.0),
            c(1, 10.5, 12.0, 10.0, 11.5, 12.0),
            c(2, 11.5, 13.0, 11.0, 12.5, 15.0),
            c(3, 12.5, 12.8, 11.5, 12.0, 8.0),
            c(4, 12.0, 12.2, 11.0, 11.5, 6.0),
            c(5, 11.5, 11.9, 11.0, 11.5, 4.0),
        ];
        let rolled = resample_all(tf, one_m.into_iter().map(Ok)).unwrap();
        // First 5 candles share bucket 0 -> aggregate. Last candle opens bucket 5.
        assert_eq!(rolled.len(), 2);
        let a = rolled[0];
        assert_eq!(a.open, 10.0);
        assert_eq!(a.close, 11.5);
        assert_eq!(a.high, 13.0);
        assert_eq!(a.low, 9.0);
        assert!((a.volume - 51.0).abs() < 1e-12);
        let b = rolled[1];
        assert_eq!(b.open, 11.5);
        assert_eq!(b.timestamp, 5);
    }

    #[test]
    fn rejects_out_of_order_candle() {
        let mut r = Resampler::new(Timeframe::new(5).unwrap());
        assert_eq!(
            r.push(c(10, 10.0, 11.0, 9.0, 10.5, 1.0)).unwrap(),
            Vec::new()
        );
        // A candle in an earlier bucket than the open bar is rejected.
        let err = r.push(c(2, 10.0, 11.0, 9.0, 10.5, 1.0)).unwrap_err();
        assert!(matches!(err, Error::Malformed(_)));
    }

    #[test]
    fn same_bucket_candles_aggregate() {
        let mut r = Resampler::new(Timeframe::new(5).unwrap());
        assert_eq!(
            r.push(c(0, 10.0, 11.0, 9.0, 10.5, 1.0)).unwrap(),
            Vec::new()
        );
        assert_eq!(
            r.push(c(3, 10.5, 12.0, 10.0, 11.0, 1.0)).unwrap(),
            Vec::new()
        );
        let bar = r.flush().unwrap().unwrap();
        assert_eq!(bar.high, 12.0);
        assert_eq!(bar.low, 9.0);
    }

    #[test]
    fn absorb_lowers_low_on_dipping_candle() {
        // The first candle in the bucket sets low = 10.0; the second dips to
        // 8.0 and must overwrite. Exercises the `c.low < self.low` branch in
        // RolledBar::absorb that the other resampler tests never trigger
        // because their follow-up candles always have a higher low.
        let mut r = Resampler::new(Timeframe::new(5).unwrap());
        r.push(c(0, 10.0, 11.0, 10.0, 10.5, 1.0)).unwrap();
        r.push(c(1, 10.5, 11.5, 8.0, 9.0, 1.0)).unwrap();
        let bar = r.flush().unwrap().unwrap();
        assert_eq!(bar.low, 8.0);
        assert_eq!(bar.high, 11.5);
    }

    #[test]
    fn flushes_a_non_finite_volume_as_an_error() {
        let mut r = Resampler::new(Timeframe::new(5).unwrap());
        // Two near-max volumes in the same bucket sum to +inf.
        assert_eq!(
            r.push(c(0, 10.0, 11.0, 9.0, 10.5, f64::MAX)).unwrap(),
            Vec::new()
        );
        assert_eq!(
            r.push(c(1, 10.0, 11.0, 9.0, 10.5, f64::MAX)).unwrap(),
            Vec::new()
        );
        let err = r.flush().unwrap_err();
        assert!(matches!(err, Error::Core(_)));
    }
    #[test]
    fn skips_empty_buckets_without_gap_fill() {
        let mut r = Resampler::new(Timeframe::new(5).unwrap());
        assert!(!r.fills_gaps(), "off unless asked for");
        r.push(c(0, 10.0, 11.0, 9.0, 10.5, 1.0)).unwrap();
        // Jump three output buckets ahead: the two in between stay absent.
        let out = r.push(c(20, 12.0, 13.0, 11.0, 12.5, 1.0)).unwrap();
        assert_eq!(out.len(), 1, "only the bar that actually closed");
        assert_eq!(out[0].timestamp, 0);
    }

    #[test]
    fn gap_fill_emits_flat_candles_for_skipped_buckets() {
        let mut r = Resampler::new(Timeframe::new(5).unwrap()).with_gap_fill(true);
        assert!(r.fills_gaps());
        r.push(c(0, 10.0, 11.0, 9.0, 10.5, 1.0)).unwrap();
        r.push(c(2, 10.5, 12.0, 10.0, 11.0, 2.0)).unwrap(); // still bucket 0
                                                            // Next candle lands in bucket 15; buckets 5 and 10 were skipped.
        let out = r.push(c(16, 20.0, 21.0, 19.0, 20.5, 1.0)).unwrap();
        assert_eq!(out.len(), 3, "real bar + two flat fillers");

        let real = out[0];
        assert_eq!(real.timestamp, 0);
        assert_eq!(real.close, 11.0);
        assert_eq!(real.volume, 3.0, "both input candles absorbed");

        for (filler, ts) in out[1..].iter().zip([5, 10]) {
            assert_eq!(filler.timestamp, ts);
            assert_eq!(filler.open, 11.0, "carries the close before the gap");
            assert_eq!(filler.high, 11.0);
            assert_eq!(filler.low, 11.0);
            assert_eq!(filler.close, 11.0);
            assert_eq!(filler.volume, 0.0, "a placeholder invents no volume");
        }
    }

    #[test]
    fn gap_fill_emits_nothing_extra_for_adjacent_buckets() {
        let mut r = Resampler::new(Timeframe::new(5).unwrap()).with_gap_fill(true);
        r.push(c(0, 10.0, 11.0, 9.0, 10.5, 1.0)).unwrap();
        let out = r.push(c(5, 12.0, 13.0, 11.0, 12.5, 1.0)).unwrap();
        assert_eq!(out.len(), 1, "nothing was skipped, so nothing is filled");
    }

    #[test]
    fn gap_fill_rejects_runaway_timestamp_jump() {
        // A clock-glitch candle far in the future must surface as an error
        // rather than allocate millions of placeholders.
        let mut r = Resampler::new(Timeframe::new(5).unwrap()).with_gap_fill(true);
        r.push(c(0, 10.0, 11.0, 9.0, 10.5, 1.0)).unwrap();
        let err = r
            .push(c(i64::MAX / 2, 20.0, 21.0, 19.0, 20.5, 1.0))
            .expect_err("a runaway gap must be refused");
        let msg = err.to_string();
        assert!(
            msg.contains("gap-fill") && msg.contains("cap"),
            "expected a malformed-gap error, got: {msg}"
        );
    }

    #[test]
    fn gap_fill_matches_the_aggregator_on_the_same_gap() {
        // The two share one implementation; this pins that they agree rather
        // than merely look similar.
        let mut r = Resampler::new(Timeframe::new(5).unwrap()).with_gap_fill(true);
        r.push(c(0, 10.0, 11.0, 9.0, 11.0, 1.0)).unwrap();
        let out = r.push(c(16, 20.0, 21.0, 19.0, 20.5, 1.0)).unwrap();

        let mut agg =
            crate::aggregator::TickAggregator::new(Timeframe::new(5).unwrap()).with_gap_fill(true);
        agg.push(wickra_core::Tick::new(11.0, 1.0, 0).unwrap())
            .unwrap();
        let agg_out = agg
            .push(wickra_core::Tick::new(20.5, 1.0, 16).unwrap())
            .unwrap();

        let fillers: Vec<_> = out[1..].iter().map(|c| (c.timestamp, c.close)).collect();
        let agg_fillers: Vec<_> = agg_out[1..]
            .iter()
            .map(|c| (c.timestamp, c.close))
            .collect();
        assert_eq!(fillers, agg_fillers);
    }
}
