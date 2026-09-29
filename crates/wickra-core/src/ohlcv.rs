//! OHLCV value types: candles and ticks.

use crate::error::{Error, Result};

/// A single OHLCV bar.
///
/// Timestamps are unitless `i64` values so callers can use whatever epoch resolution
/// they prefer (milliseconds, microseconds, seconds…). Wickra never inspects them
/// numerically beyond passing them through.
///
/// # Construction and the limits of its guarantee
///
/// The struct is `#[non_exhaustive]`, so code outside this crate cannot build
/// one from a field literal and must go through [`new`](Self::new), which
/// validates, or [`new_unchecked`](Self::new_unchecked), which is an explicit
/// opt-out for values already known to be sound.
///
/// The fields stay public because reading them is by far the common operation
/// and an accessor on each would buy nothing. That does mean a validated value
/// can still be *written* into an invalid state afterwards, and nothing detects
/// it: the indicators that consume this type rely on the constructor's
/// guarantee rather than re-checking every bar. Treat a mutation the way you
/// would treat `new_unchecked` — you are asserting the invariants still hold.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Candle {
    /// Bar open price.
    pub open: f64,
    /// Bar high price.
    pub high: f64,
    /// Bar low price.
    pub low: f64,
    /// Bar close price.
    pub close: f64,
    /// Bar volume.
    pub volume: f64,
    /// Bar timestamp (caller-defined epoch / resolution).
    pub timestamp: i64,
}

impl Candle {
    /// Construct a new candle, validating the OHLC relationships and finiteness.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidCandle`] if any of these invariants are violated:
    /// - `high >= max(open, close, low)`
    /// - `low  <= min(open, close, high)`
    /// - all of `open`, `high`, `low`, `close`, `volume` are finite
    /// - `volume >= 0`
    pub fn new(
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> Result<Self> {
        if !(open.is_finite() && high.is_finite() && low.is_finite() && close.is_finite()) {
            return Err(Error::InvalidCandle {
                message: "open, high, low, close must all be finite",
            });
        }
        if !volume.is_finite() {
            return Err(Error::InvalidCandle {
                message: "volume must be finite",
            });
        }
        if volume < 0.0 {
            return Err(Error::InvalidCandle {
                message: "volume must be non-negative",
            });
        }
        if high < low {
            return Err(Error::InvalidCandle {
                message: "high must be >= low",
            });
        }
        if high < open || high < close {
            return Err(Error::InvalidCandle {
                message: "high must be >= open and >= close",
            });
        }
        if low > open || low > close {
            return Err(Error::InvalidCandle {
                message: "low must be <= open and <= close",
            });
        }
        Ok(Self {
            open,
            high,
            low,
            close,
            volume,
            timestamp,
        })
    }

    /// Construct a candle without validation. The caller asserts that all OHLC
    /// invariants hold and that no field is NaN or infinite.
    pub const fn new_unchecked(
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> Self {
        Self {
            open,
            high,
            low,
            close,
            volume,
            timestamp,
        }
    }

    /// Whether [`Candle::new`] would accept every bar `(open[i], high[i],
    /// low[i], close[i], volume[i])`: all finite, a non-negative volume, and
    /// `low <= open, close <= high`.
    ///
    /// The bars are checked in eight independent lanes reduced at the end, with
    /// no early exit and no branch per rule, inside a kernel compiled for AVX2
    /// where the CPU has it: a batch over OHLCV columns otherwise spends as long
    /// validating as computing.
    ///
    /// # Panics
    ///
    /// Panics if the columns differ in length.
    pub fn all_valid(
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> bool {
        let n = open.len();
        assert!(
            high.len() == n && low.len() == n && close.len() == n && volume.len() == n,
            "every column must be equally long"
        );
        wickra_simd::dispatch(AllValid {
            open,
            high: &high[..n],
            low: &low[..n],
            close: &close[..n],
            volume: &volume[..n],
        })
    }

    /// The typical price `(high + low + close) / 3`. Used by CCI, MFI, VWAP, etc.
    #[inline]
    pub fn typical_price(&self) -> f64 {
        (self.high + self.low + self.close) / 3.0
    }

    /// The mid price `(high + low) / 2`.
    #[inline]
    pub fn median_price(&self) -> f64 {
        f64::midpoint(self.high, self.low)
    }

    /// The weighted close `(high + low + 2*close) / 4`.
    #[inline]
    pub fn weighted_close(&self) -> f64 {
        (self.high + self.low + 2.0 * self.close) / 4.0
    }

    /// The average price `(open + high + low + close) / 4`.
    #[inline]
    pub fn avg_price(&self) -> f64 {
        (self.open + self.high + self.low + self.close) / 4.0
    }

    /// True range of this candle relative to a previous close: `max(H-L, |H-prev|, |L-prev|)`.
    /// If no previous close is supplied, falls back to `high - low`.
    #[inline]
    pub fn true_range(&self, prev_close: Option<f64>) -> f64 {
        let hl = self.high - self.low;
        match prev_close {
            Some(prev) => {
                let hp = (self.high - prev).abs();
                let lp = (self.low - prev).abs();
                hl.max(hp).max(lp)
            }
            None => hl,
        }
    }
}

/// A single trade tick.
///
/// # Construction and the limits of its guarantee
///
/// The struct is `#[non_exhaustive]`, so code outside this crate cannot build
/// one from a field literal and must go through [`new`](Self::new), which
/// validates. A tick has no unchecked constructor.
///
/// The fields stay public because reading them is by far the common operation
/// and an accessor on each would buy nothing. That does mean a validated value
/// can still be *written* into an invalid state afterwards, and nothing detects
/// it: the code that consumes this type relies on the constructor's guarantee
/// rather than re-checking. A mutation is an assertion that the invariants
/// still hold.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Tick {
    /// Trade price.
    pub price: f64,
    /// Trade size.
    pub volume: f64,
    /// Trade timestamp (caller-defined epoch / resolution).
    pub timestamp: i64,
}

impl Tick {
    /// Construct a new tick, validating finiteness and non-negativity of volume.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NonFiniteInput`] if `price` or `volume` is NaN or infinite,
    /// or [`Error::InvalidTick`] for `volume < 0`. (Audit finding R14 — previously
    /// returned [`Error::InvalidCandle`], which is semantically wrong for a tick.)
    pub fn new(price: f64, volume: f64, timestamp: i64) -> Result<Self> {
        if !price.is_finite() || !volume.is_finite() {
            return Err(Error::NonFiniteInput);
        }
        if volume < 0.0 {
            return Err(Error::InvalidTick {
                message: "tick volume must be non-negative",
            });
        }
        Ok(Self {
            price,
            volume,
            timestamp,
        })
    }
}

/// [`Candle::all_valid`] over equally long columns, as a dispatched kernel:
/// compiled into its own function, it vectorizes the same whatever calls it,
/// where inlined into a caller it did or did not depending on the caller.
struct AllValid<'a> {
    open: &'a [f64],
    high: &'a [f64],
    low: &'a [f64],
    close: &'a [f64],
    volume: &'a [f64],
}

// Inlining into the dispatching function is what compiles the body with its
// features; see `wickra_simd::Kernel`. `&` rather than `&&` on purpose: every
// rule is evaluated for every bar, which is what lets the loop vectorize
// instead of branching.
#[allow(clippy::inline_always, clippy::needless_bitwise_bool)]
impl wickra_simd::Kernel for AllValid<'_> {
    type Output = bool;

    #[inline(always)]
    fn run<S: wickra_simd::Simd>(self, _simd: S) -> bool {
        const LANES: usize = 8;
        // Whole blocks as fixed-size arrays: their length is in the type, so
        // the loop carries no bounds check and vectorizes; indexing the slices
        // directly kept a check per element, which no reslicing removed.
        fn blocks(col: &[f64]) -> impl Iterator<Item = &[f64; LANES]> {
            col.chunks_exact(LANES)
                .map(|block| <&[f64; LANES]>::try_from(block).expect("a whole block"))
        }
        let (open, high, low, close, volume) =
            (self.open, self.high, self.low, self.close, self.volume);
        let n = open.len();
        // `x * 0.0 == 0.0` is `x.is_finite()` -- false exactly for NaN and the
        // infinities -- as a plain comparison, which vectorizes where
        // `is_finite` did not.
        let bar = |o: f64, h: f64, l: f64, c: f64, v: f64| {
            let finite = (o * 0.0 == 0.0)
                & (h * 0.0 == 0.0)
                & (l * 0.0 == 0.0)
                & (c * 0.0 == 0.0)
                & (v * 0.0 == 0.0);
            let ordered = (h >= l) & (h >= o) & (h >= c) & (l <= o) & (l <= c);
            finite & (v >= 0.0) & ordered
        };
        let mut lanes = [true; LANES];
        for ((((o, h), l), c), v) in blocks(open)
            .zip(blocks(high))
            .zip(blocks(low))
            .zip(blocks(close))
            .zip(blocks(volume))
        {
            for (k, lane) in lanes.iter_mut().enumerate() {
                *lane &= bar(o[k], h[k], l[k], c[k], v[k]);
            }
        }
        let full = n - n % LANES;
        (full..n).fold(lanes.iter().all(|&ok| ok), |ok, i| {
            ok & bar(open[i], high[i], low[i], close[i], volume[i])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_valid_agrees_with_candle_new_bar_by_bar() {
        // Each bar breaks at most one rule, or none; every rule is broken once.
        let bars: [(f64, f64, f64, f64, f64); 12] = [
            (10.0, 11.0, 9.0, 10.5, 100.0),
            (10.0, 10.0, 10.0, 10.0, 0.0),
            (-0.0, 0.0, -0.0, 0.0, -0.0),
            (f64::NAN, 11.0, 9.0, 10.0, 1.0),
            (10.0, f64::INFINITY, 9.0, 10.0, 1.0),
            (10.0, 11.0, f64::NEG_INFINITY, 10.0, 1.0),
            (10.0, 11.0, 9.0, f64::NAN, 1.0),
            (10.0, 11.0, 9.0, 10.0, f64::INFINITY),
            (10.0, 11.0, 9.0, 10.0, -1.0),
            (10.0, 9.0, 9.5, 9.2, 1.0),
            (12.0, 11.0, 9.0, 10.0, 1.0),
            (10.0, 11.0, 9.0, 8.0, 1.0),
        ];
        for &(o, h, l, c, v) in &bars {
            let single = Candle::all_valid(&[o], &[h], &[l], &[c], &[v]);
            assert_eq!(
                single,
                Candle::new(o, h, l, c, v, 0).is_ok(),
                "{o} {h} {l} {c} {v}"
            );
        }
        // The same bars at every position of a 21-bar run of valid ones: every
        // lane of the vector loop and every slot of its tail.
        let fine = bars[0];
        for &(o, h, l, c, v) in &bars {
            let want = Candle::new(o, h, l, c, v, 0).is_ok();
            for at in 0..21 {
                let mut run = [fine; 21];
                run[at] = (o, h, l, c, v);
                let column = |pick: fn(&(f64, f64, f64, f64, f64)) -> f64| {
                    run.iter().map(pick).collect::<Vec<_>>()
                };
                let got = Candle::all_valid(
                    &column(|b| b.0),
                    &column(|b| b.1),
                    &column(|b| b.2),
                    &column(|b| b.3),
                    &column(|b| b.4),
                );
                assert_eq!(got, want, "bar {o} {h} {l} {c} {v} at {at}");
            }
        }
        // A long run of valid bars; one bad bar anywhere fails it.
        let n = 1_300;
        let close: Vec<f64> = (0..n).map(|i| 100.0 + f64::from(i % 17)).collect();
        let high: Vec<f64> = close.iter().map(|c| c + 1.0).collect();
        let low: Vec<f64> = close.iter().map(|c| c - 1.0).collect();
        let volume = vec![5.0; close.len()];
        assert!(Candle::all_valid(&close, &high, &low, &close, &volume));
        let mut bad = low.clone();
        bad[1_100] = high[1_100] + 1.0;
        assert!(!Candle::all_valid(&close, &high, &bad, &close, &volume));
        assert!(Candle::all_valid(&[], &[], &[], &[], &[]));
    }

    #[test]
    #[should_panic(expected = "every column must be equally long")]
    fn all_valid_rejects_ragged_columns() {
        let _ = Candle::all_valid(&[1.0], &[1.0], &[1.0], &[1.0], &[]);
    }

    #[test]
    fn candle_new_accepts_valid_ohlc() {
        let c = Candle::new(10.0, 11.0, 9.0, 10.5, 100.0, 1).unwrap();
        assert_eq!(c.open, 10.0);
        assert_eq!(c.high, 11.0);
        assert_eq!(c.low, 9.0);
        assert_eq!(c.close, 10.5);
        assert_eq!(c.volume, 100.0);
        assert_eq!(c.timestamp, 1);
    }

    #[test]
    fn candle_new_rejects_high_below_low() {
        let err = Candle::new(10.0, 9.0, 10.0, 10.0, 1.0, 0).unwrap_err();
        assert!(matches!(err, Error::InvalidCandle { .. }));
    }

    #[test]
    fn candle_new_rejects_high_below_close() {
        let err = Candle::new(10.0, 10.0, 9.0, 11.0, 1.0, 0).unwrap_err();
        assert!(matches!(err, Error::InvalidCandle { .. }));
    }

    #[test]
    fn candle_new_rejects_low_above_open() {
        let err = Candle::new(10.0, 11.0, 10.5, 10.5, 1.0, 0).unwrap_err();
        assert!(matches!(err, Error::InvalidCandle { .. }));
    }

    #[test]
    fn candle_new_rejects_negative_volume() {
        let err = Candle::new(10.0, 11.0, 9.0, 10.5, -1.0, 0).unwrap_err();
        assert!(matches!(err, Error::InvalidCandle { .. }));
    }

    #[test]
    fn candle_new_rejects_nan_price() {
        let err = Candle::new(f64::NAN, 11.0, 9.0, 10.5, 1.0, 0).unwrap_err();
        assert!(matches!(err, Error::InvalidCandle { .. }));
    }

    /// Cover the unchecked constructor `Candle::new_unchecked` (lines 86-102).
    /// Every existing test routes through the validating `Candle::new`, so the
    /// unchecked path is dead.
    ///
    /// The first assertion shows that a valid set of fields round-trips
    /// verbatim. The second feeds `high < low` (which `Candle::new` would
    /// reject with `Error::InvalidCandle`) and asserts the unchecked
    /// constructor still produces the struct as-is — documenting and
    /// enforcing the API contract that the unchecked variant performs no
    /// validation and is the caller's responsibility.
    #[test]
    fn candle_new_unchecked_preserves_fields_verbatim() {
        let c = Candle::new_unchecked(1.0, 2.0, 0.5, 1.5, 100.0, 42);
        assert_eq!(c.open, 1.0);
        assert_eq!(c.high, 2.0);
        assert_eq!(c.low, 0.5);
        assert_eq!(c.close, 1.5);
        assert_eq!(c.volume, 100.0);
        assert_eq!(c.timestamp, 42);

        // Skip-validation contract: an OHLC combination that the checked
        // constructor rejects (high < low) is still built without error.
        assert!(Candle::new(10.0, 9.0, 10.0, 10.0, 1.0, 0).is_err());
        let unchecked = Candle::new_unchecked(10.0, 9.0, 10.0, 10.0, 1.0, 0);
        assert_eq!(unchecked.high, 9.0);
        assert_eq!(unchecked.low, 10.0);
    }

    #[test]
    fn candle_typical_price() {
        let c = Candle::new(10.0, 12.0, 9.0, 11.0, 1.0, 0).unwrap();
        assert_eq!(c.typical_price(), (12.0 + 9.0 + 11.0) / 3.0);
    }

    #[test]
    fn candle_median_price() {
        let c = Candle::new(10.0, 12.0, 8.0, 11.0, 1.0, 0).unwrap();
        assert_eq!(c.median_price(), 10.0);
    }

    #[test]
    fn candle_weighted_close() {
        let c = Candle::new(10.0, 12.0, 8.0, 11.0, 1.0, 0).unwrap();
        assert_eq!(c.weighted_close(), (12.0 + 8.0 + 22.0) / 4.0);
    }

    #[test]
    fn candle_true_range_without_prev() {
        let c = Candle::new(10.0, 12.0, 8.0, 11.0, 1.0, 0).unwrap();
        assert_eq!(c.true_range(None), 4.0);
    }

    #[test]
    fn candle_true_range_with_gap_up() {
        // Previous close 6, today's range 8-12: gap covered by |H-prev|=6
        let c = Candle::new(10.0, 12.0, 8.0, 11.0, 1.0, 0).unwrap();
        assert_eq!(c.true_range(Some(6.0)), 6.0);
    }

    #[test]
    fn candle_true_range_with_gap_down() {
        // Previous close 14, today's range 8-12: gap covered by |L-prev|=6
        let c = Candle::new(10.0, 12.0, 8.0, 11.0, 1.0, 0).unwrap();
        assert_eq!(c.true_range(Some(14.0)), 6.0);
    }

    #[test]
    fn tick_new_accepts_valid() {
        let t = Tick::new(100.5, 0.5, 42).unwrap();
        assert_eq!(t.price, 100.5);
        assert_eq!(t.volume, 0.5);
        assert_eq!(t.timestamp, 42);
    }

    #[test]
    fn tick_new_rejects_nan() {
        assert!(matches!(
            Tick::new(f64::NAN, 1.0, 0),
            Err(Error::NonFiniteInput)
        ));
    }

    #[test]
    fn tick_new_rejects_inf() {
        assert!(matches!(
            Tick::new(f64::INFINITY, 1.0, 0),
            Err(Error::NonFiniteInput)
        ));
    }

    #[test]
    fn tick_new_rejects_negative_volume() {
        // Audit R14: the variant is `InvalidTick`, not `InvalidCandle` — a tick
        // is not a candle, and downstream pipelines should be able to match on
        // the correct semantic.
        let err = Tick::new(100.0, -1.0, 0).unwrap_err();
        assert!(matches!(err, Error::InvalidTick { .. }));
        assert!(
            err.to_string().contains("tick volume"),
            "expected the InvalidTick message in the formatted error, got {err}"
        );
    }
}
