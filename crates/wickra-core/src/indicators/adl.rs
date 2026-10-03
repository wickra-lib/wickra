//! Accumulation/Distribution Line.

use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Accumulation/Distribution Line — Marc Chaikin's cumulative volume-flow
/// indicator.
///
/// Each bar contributes a *money-flow volume*: the bar's volume weighted by
/// where the close fell within the bar's range.
///
/// ```text
/// MFM_t = ((close − low) − (high − close)) / (high − low)   (the money-flow multiplier, −1..+1)
/// MFV_t = MFM_t · volume_t
/// ADL_t = ADL_{t−1} + MFV_t
/// ```
///
/// A close near the high makes the multiplier near `+1` (accumulation), near
/// the low near `−1` (distribution). The running total is unbounded and drifts
/// with cumulative volume — what matters is its slope and its divergence from
/// price. A bar with `high == low` contributes `0`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Adl};
///
/// let mut indicator = Adl::new();
/// let mut last = None;
/// for i in 0..80 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct Adl {
    total: f64,
    has_emitted: bool,
}

impl Adl {
    /// Construct a new Accumulation/Distribution Line starting at zero.
    pub const fn new() -> Self {
        Self {
            total: 0.0,
            has_emitted: false,
        }
    }

    /// Continue from an accumulation `total` computed elsewhere (a batch
    /// kernel), as if every earlier bar had been fed.
    pub(crate) fn resume_at(&mut self, total: f64) {
        self.total = total;
        self.has_emitted = true;
    }

    /// Current cumulative value if at least one candle has been ingested.
    pub const fn value(&self) -> Option<f64> {
        if self.has_emitted {
            Some(self.total)
        } else {
            None
        }
    }
}

impl Indicator for Adl {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let range = candle.high - candle.low;
        let mfv = if range == 0.0 {
            // A zero-range bar carries no positional information.
            0.0
        } else {
            let mfm = ((candle.close - candle.low) - (candle.high - candle.close)) / range;
            mfm * candle.volume
        };
        self.total += mfv;
        self.has_emitted = true;
        Some(self.total)
    }

    fn reset(&mut self) {
        self.total = 0.0;
        self.has_emitted = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ADL"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn candle(open: f64, high: f64, low: f64, close: f64, volume: f64, ts: i64) -> Candle {
        Candle::new(open, high, low, close, volume, ts).unwrap()
    }

    #[test]
    fn reference_values() {
        // bar 1: close at high -> MFM = +1 -> MFV = +100; ADL = 100.
        // bar 2: h=12 l=8 c=9  -> MFM = ((9-8)-(12-9))/4 = -0.5 -> MFV = -100;
        //        ADL = 100 - 100 = 0.
        let mut adl = Adl::new();
        let out = adl.batch(&[
            candle(8.0, 10.0, 8.0, 10.0, 100.0, 0),
            candle(10.0, 12.0, 8.0, 9.0, 200.0, 1),
        ]);
        assert_relative_eq!(out[0].unwrap(), 100.0, epsilon = 1e-12);
        assert_relative_eq!(out[1].unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn emits_from_first_candle() {
        let mut adl = Adl::new();
        assert_eq!(adl.warmup_period(), 1);
        assert!(adl.update(candle(8.0, 10.0, 8.0, 9.0, 50.0, 0)).is_some());
    }

    /// Cover the Indicator-impl `name` body (94-96). The other accessors
    /// are exercised by existing tests; `name` was never queried.
    #[test]
    fn accessors_and_metadata() {
        let adl = Adl::new();
        assert_eq!(adl.name(), "ADL");
    }

    #[test]
    fn close_at_high_accumulates_full_volume() {
        // Every bar closes at its high: MFM = +1, so ADL grows by `volume`.
        let mut adl = Adl::new();
        let mut expected = 0.0;
        for i in 0..10 {
            let c = candle(8.0, 10.0, 8.0, 10.0, 25.0, i);
            expected += 25.0;
            assert_relative_eq!(adl.update(c).unwrap(), expected, epsilon = 1e-9);
        }
    }

    #[test]
    fn zero_range_bar_contributes_nothing() {
        let mut adl = Adl::new();
        adl.update(candle(8.0, 10.0, 8.0, 10.0, 100.0, 0));
        let before = adl.value().unwrap();
        // A flat candle (high == low) adds zero.
        let after = adl.update(candle(9.0, 9.0, 9.0, 9.0, 999.0, 1)).unwrap();
        assert_relative_eq!(after, before, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut adl = Adl::new();
        adl.batch(&[
            candle(8.0, 10.0, 8.0, 9.0, 100.0, 0),
            candle(9.0, 11.0, 9.0, 10.0, 100.0, 1),
        ]);
        assert!(adl.is_ready());
        adl.reset();
        assert!(!adl.is_ready());
        assert_eq!(adl.value(), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let mid = 100.0 + (i as f64 * 0.3).sin() * 8.0;
                candle(
                    mid,
                    mid + 2.0,
                    mid - 2.0,
                    mid + 0.5,
                    10.0 + (i % 5) as f64,
                    i,
                )
            })
            .collect();
        let batch = Adl::new().batch(&candles);
        let mut b = Adl::new();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }
}
