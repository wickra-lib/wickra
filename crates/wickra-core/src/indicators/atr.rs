//! Average True Range (Wilder).

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Average True Range with Wilder smoothing.
///
/// The first emitted value, by convention, appears after `period` candles: the
/// first `period − 1` true-range values seed the Wilder average alongside the
/// `period`-th, then the smoothed update begins.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Atr};
///
/// let mut indicator = Atr::new(5).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Atr {
    period: usize,
    /// `period - 1` as `f64`, precomputed for the Wilder smoothing step.
    n_minus_1: f64,
    /// `1 / period`, precomputed so the per-tick smoothing multiplies instead of
    /// divides.
    inv_period: f64,
    prev_close: Option<f64>,
    /// Running sum of the seed true ranges, from `-0.0` in bar order — exactly
    /// how `f64::sum` folds, so the seed mean is the one a buffered
    /// `iter().sum()` gives, without a heap buffer keeping the per-tick state
    /// out of registers.
    seed_sum: f64,
    /// Number of seed true ranges taken so far.
    seed_count: usize,
    /// Smoothed ATR, valid once `seeded` is set. Bare `f64` + flag rather than
    /// `Option<f64>` so the hot recurrence avoids an enum-tag read per tick.
    avg: f64,
    seeded: bool,
}

impl Atr {
    /// Construct an ATR with the given Wilder period.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
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
            n_minus_1: (period - 1) as f64,
            inv_period: 1.0 / period as f64,
            prev_close: None,
            seed_sum: -0.0,
            seed_count: 0,
            avg: 0.0,
            seeded: false,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        if self.seeded {
            Some(self.avg)
        } else {
            None
        }
    }

    /// Vectorized batch over raw high/low/close columns: one `f64` per bar
    /// (`NaN` during warmup). The caller guarantees the three slices are equal
    /// length and finite with valid OHLC ordering (the binding validates once up
    /// front); ATR only reads high, low and the previous close.
    ///
    /// Allocates the result and fills it through
    /// [`batch_atr_into`](Self::batch_atr_into).
    pub fn batch_atr(&mut self, high: &[f64], low: &[f64], close: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; high.len()];
        self.batch_atr_into(high, low, close, &mut out);
        out
    }

    /// [`batch_atr`](Self::batch_atr) into a caller-owned buffer, overwriting
    /// every cell.
    ///
    /// For a fresh indicator long enough to seed (`n >= period`) it runs the
    /// true-range seed once and then the bare Wilder recurrence as a dispatched
    /// kernel (hardware FMA where the CPU has it) — no per-bar `Candle`
    /// construction/validation, no `Option`, identical division at the seed and
    /// `mul_add` afterwards, so every value is *bit-for-bit* equal to replaying
    /// `update` over the same candles. Shorter or non-fresh inputs defer to an
    /// exact `update` replay.
    ///
    /// # Panics
    ///
    /// Panics if the four slices differ in length.
    pub fn batch_atr_into(&mut self, high: &[f64], low: &[f64], close: &[f64], out: &mut [f64]) {
        let n = high.len();
        assert!(
            low.len() == n && close.len() == n && out.len() == n,
            "high, low, close and the output must be equal length"
        );
        let p = self.period;
        if self.seeded || self.seed_count != 0 || self.prev_close.is_some() || n < p {
            for (i, slot) in out.iter_mut().enumerate() {
                let candle = Candle::new_unchecked(close[i], high[i], low[i], close[i], 0.0, 0);
                *slot = self.update(candle).unwrap_or(f64::NAN);
            }
            return;
        }

        // Warmup `[0, p-1)` is `NaN`; the first ATR is emitted at index `p - 1`.
        out[..p - 1].fill(f64::NAN);
        // Seed: mean of the first `period` true ranges. TR₀ has no previous close.
        let mut prev_close = close[0];
        let mut sum_tr = -0.0 + (high[0] - low[0]);
        for i in 1..p {
            let (h, l) = (high[i], low[i]);
            let tr = (h - l)
                .max((h - prev_close).abs())
                .max((l - prev_close).abs());
            prev_close = close[i];
            sum_tr += tr;
        }
        let avg = sum_tr / p as f64;
        out[p - 1] = avg;
        // Steady state: Wilder smoothing, reciprocal hoisted out of the loop.
        let (prev_close, avg) = wickra_simd::dispatch(AtrTail {
            high: &high[p..],
            low: &low[p..],
            close: &close[p..],
            out: &mut out[p..],
            state: (prev_close, avg),
            n_minus_1: self.n_minus_1,
            inv_period: self.inv_period,
        });

        // Leave state where a full `update` replay would.
        self.prev_close = Some(prev_close);
        self.seed_sum = sum_tr;
        self.seed_count = p;
        self.avg = avg;
        self.seeded = true;
    }
}

/// ATR's steady-state Wilder smoothing as a [`wickra_simd::Kernel`], so the
/// `mul_add` becomes a hardware FMA where the CPU has one. Returns the final
/// previous close and average.
struct AtrTail<'a> {
    high: &'a [f64],
    low: &'a [f64],
    close: &'a [f64],
    out: &'a mut [f64],
    state: (f64, f64),
    n_minus_1: f64,
    inv_period: f64,
}

// Inlining into the dispatching function is what compiles the body with its
// features; see `wickra_simd::Kernel`.
#[allow(clippy::inline_always)]
impl wickra_simd::Kernel for AtrTail<'_> {
    type Output = (f64, f64);

    #[inline(always)]
    fn run<S: wickra_simd::Simd>(self, _simd: S) -> (f64, f64) {
        let (mut prev_close, mut avg) = self.state;
        let (n_minus_1, inv_period) = (self.n_minus_1, self.inv_period);
        for (((slot, &h), &l), &c) in self
            .out
            .iter_mut()
            .zip(self.high)
            .zip(self.low)
            .zip(self.close)
        {
            let tr = (h - l)
                .max((h - prev_close).abs())
                .max((l - prev_close).abs());
            prev_close = c;
            avg = avg.mul_add(n_minus_1, tr) * inv_period;
            *slot = avg;
        }
        (prev_close, avg)
    }
}

impl Indicator for Atr {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let tr = candle.true_range(self.prev_close);
        self.prev_close = Some(candle.close);

        if self.seeded {
            // Wilder smoothing with the reciprocal hoisted out of the hot path.
            let new_avg = self.avg.mul_add(self.n_minus_1, tr) * self.inv_period;
            self.avg = new_avg;
            return Some(new_avg);
        }

        self.seed_sum += tr;
        self.seed_count += 1;
        if self.seed_count == self.period {
            let seed = self.seed_sum / self.period as f64;
            self.avg = seed;
            self.seeded = true;
            return Some(seed);
        }
        None
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.seed_sum = -0.0;
        self.seed_count = 0;
        self.avg = 0.0;
        self.seeded = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.seeded
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ATR"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(h: f64, l: f64, cl: f64) -> Candle {
        // ts/open/volume don't affect ATR; use safe placeholders.
        Candle::new(cl, h, l, cl, 1.0, 0).unwrap()
    }

    /// Independent reference: Wilder ATR computed straight from the definition.
    fn atr_naive(hlc: &[(f64, f64, f64)], period: usize) -> Vec<Option<f64>> {
        let n = period as f64;
        let mut out = Vec::with_capacity(hlc.len());
        let mut trs: Vec<f64> = Vec::new();
        let mut avg: Option<f64> = None;
        let mut prev_close: Option<f64> = None;
        for &(h, l, cl) in hlc {
            let tr = match prev_close {
                None => h - l,
                Some(pc) => (h - l).max((h - pc).abs()).max((l - pc).abs()),
            };
            prev_close = Some(cl);
            if let Some(a) = avg {
                let na = (a * (n - 1.0) + tr) / n;
                avg = Some(na);
                out.push(Some(na));
            } else {
                trs.push(tr);
                if trs.len() == period {
                    avg = Some(trs.iter().sum::<f64>() / n);
                    out.push(avg);
                } else {
                    out.push(None);
                }
            }
        }
        out
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Atr::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `value` (54-62) and the
    /// Indicator-impl `name` body (103-105). Existing tests inspect
    /// numeric ATR output but never query the metadata.
    #[test]
    fn accessors_and_metadata() {
        let mut atr = Atr::new(14).unwrap();
        assert_eq!(atr.period(), 14);
        assert_eq!(atr.name(), "ATR");
        assert_eq!(atr.value(), None);
        for _ in 0..14 {
            atr.update(c(11.0, 9.0, 10.0));
        }
        assert!(atr.value().is_some());
    }

    #[test]
    fn warmup_emits_on_period_th_candle() {
        let candles = vec![
            c(2.0, 1.0, 1.5),
            c(3.0, 2.0, 2.5),
            c(4.0, 3.0, 3.5),
            c(5.0, 4.0, 4.5),
            c(6.0, 5.0, 5.5),
        ];
        let mut atr = Atr::new(3).unwrap();
        let out = atr.batch(&candles);
        assert!(out[0].is_none());
        assert!(out[1].is_none());
        assert!(out[2].is_some());
        assert!(out[3].is_some());
    }

    #[test]
    fn constant_range_yields_constant_atr() {
        // Every candle has H=11, L=9, C=10 -> TR=2 (no gaps).
        let candles: Vec<Candle> = (0..30).map(|_| c(11.0, 9.0, 10.0)).collect();
        let mut atr = Atr::new(14).unwrap();
        let out = atr.batch(&candles);
        for v in out.iter().skip(13).flatten() {
            assert_relative_eq!(*v, 2.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn gap_up_uses_high_minus_prev_close() {
        // Previous close 5, current candle H=10 L=9 C=9.5 -> TR = max(1, 5, 4) = 5.
        let candles = vec![
            c(6.0, 4.0, 5.0),  // prev close = 5
            c(10.0, 9.0, 9.5), // TR = 5
        ];
        let mut atr = Atr::new(2).unwrap();
        let out = atr.batch(&candles);
        // Seed window covers TR_1 and TR_2. TR_1 = H1-L1 = 2 (no prev close). TR_2 = 5.
        // Seed = (2+5)/2 = 3.5
        assert_relative_eq!(out[1].unwrap(), 3.5, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| {
                let mid = f64::from(i) + 10.0;
                c(mid + 0.5, mid - 0.5, mid)
            })
            .collect();
        let mut a = Atr::new(14).unwrap();
        let mut b = Atr::new(14).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..20).map(|_| c(11.0, 9.0, 10.0)).collect();
        let mut atr = Atr::new(5).unwrap();
        atr.batch(&candles);
        assert!(atr.is_ready());
        atr.reset();
        assert!(!atr.is_ready());
        assert_eq!(atr.update(candles[0]), None);
    }

    #[test]
    fn never_negative() {
        let candles: Vec<Candle> = (0..200)
            .map(|i| {
                let base = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
                c(base + 1.0, base - 1.0, base)
            })
            .collect();
        let mut atr = Atr::new(14).unwrap();
        for v in atr.batch(&candles).into_iter().flatten() {
            assert!(v >= 0.0, "ATR must be non-negative: {v}");
        }
    }

    fn bits_eq(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| x == y || (x.is_nan() && y.is_nan()))
    }

    fn atr_replay(period: usize, high: &[f64], low: &[f64], close: &[f64]) -> Vec<f64> {
        let mut a = Atr::new(period).unwrap();
        (0..high.len())
            .map(|i| {
                let candle = Candle::new_unchecked(close[i], high[i], low[i], close[i], 0.0, 0);
                a.update(candle).unwrap_or(f64::NAN)
            })
            .collect()
    }

    /// Valid OHLC columns from a wandering base price.
    fn columns(n: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        let base: Vec<f64> = (0..n)
            .map(|i| (f64::from(u32::try_from(i).unwrap()) * 0.3).sin() * 5.0 + 100.0)
            .collect();
        let high = base.iter().map(|b| b + 1.0).collect();
        let low = base.iter().map(|b| b - 1.0).collect();
        (high, low, base)
    }

    fn to_bits(v: &[f64]) -> Vec<u64> {
        v.iter().map(|x| x.to_bits()).collect()
    }

    /// Into a buffer that already holds values, every cell is overwritten —
    /// warmup with `NaN`, the rest with the replay's values, bit for bit.
    #[test]
    fn batch_atr_into_overwrites_a_dirty_buffer() {
        let (high, low, close) = columns(260);
        let mut out = vec![3.0; high.len()];
        Atr::new(14)
            .unwrap()
            .batch_atr_into(&high, &low, &close, &mut out);
        assert_eq!(to_bits(&out), to_bits(&atr_replay(14, &high, &low, &close)));
    }

    #[test]
    #[should_panic(expected = "high, low, close and the output must be equal length")]
    fn batch_atr_into_rejects_mismatched_lengths() {
        let (high, low, close) = columns(20);
        let mut out = vec![0.0; 19];
        Atr::new(5)
            .unwrap()
            .batch_atr_into(&high, &low, &close, &mut out);
    }

    /// The seed is a running sum from `-0.0`; it must equal the buffered
    /// `iter().sum()` the indicator used to keep, the sign of a zero-range
    /// window included.
    #[test]
    fn seed_matches_a_buffered_sum_bit_for_bit() {
        let flat = [-0.0_f64; 6];
        let mut atr = Atr::new(6).unwrap();
        let seed = flat
            .iter()
            .filter_map(|&c| atr.update(Candle::new_unchecked(c, c, c, c, 0.0, 0)))
            .last()
            .unwrap();
        let trs: Vec<f64> = std::iter::once(-0.0 - -0.0)
            .chain(std::iter::repeat_n(0.0_f64, 5))
            .collect();
        let buffered = trs.iter().copied().sum::<f64>() / 6.0;
        assert_eq!(seed.to_bits(), buffered.to_bits());
        let (high, low, close) = columns(40);
        let want = atr_replay(9, &high, &low, &close);
        let got = Atr::new(9).unwrap().batch_atr(&high, &low, &close);
        assert_eq!(to_bits(&got), to_bits(&want));
    }

    /// The dispatched Wilder tail and the baseline build write the same bits
    /// and end in the same state.
    #[test]
    fn atr_tail_is_identical_on_every_dispatch_path() {
        let (high, low, close) = columns(3000);
        let atr = Atr::new(14).unwrap();
        let (mut a, mut b) = (vec![0.0; 2990], vec![0.0; 2990]);
        let make = |out: &mut [f64]| -> (f64, f64) {
            wickra_simd::run_baseline(AtrTail {
                high: &high[10..],
                low: &low[10..],
                close: &close[10..],
                out,
                state: (close[9], 1.7),
                n_minus_1: atr.n_minus_1,
                inv_period: atr.inv_period,
            })
        };
        let rb = make(&mut b);
        let ra = wickra_simd::dispatch(AtrTail {
            high: &high[10..],
            low: &low[10..],
            close: &close[10..],
            out: &mut a,
            state: (close[9], 1.7),
            n_minus_1: atr.n_minus_1,
            inv_period: atr.inv_period,
        });
        assert_eq!(to_bits(&a), to_bits(&b));
        assert_eq!(to_bits(&[ra.0, ra.1]), to_bits(&[rb.0, rb.1]));
    }

    #[test]
    fn batch_atr_fast_path_is_bit_identical() {
        let (high, low, close) = columns(300);
        let mut atr = Atr::new(14).unwrap();
        let got = atr.batch_atr(&high, &low, &close);
        assert!(bits_eq(&got, &atr_replay(14, &high, &low, &close)));
        let mut ref_atr = Atr::new(14).unwrap();
        for i in 0..high.len() {
            ref_atr.update(Candle::new_unchecked(
                close[i], high[i], low[i], close[i], 0.0, 0,
            ));
        }
        let next = Candle::new_unchecked(101.0, 102.0, 100.0, 101.0, 0.0, 0);
        assert_eq!(atr.update(next), ref_atr.update(next));
    }

    #[test]
    fn batch_atr_falls_back_when_not_fresh() {
        let (high, low, close) = columns(40);
        let mut atr = Atr::new(14).unwrap();
        atr.update(Candle::new_unchecked(
            close[0], high[0], low[0], close[0], 0.0, 0,
        ));
        let mut ref_atr = Atr::new(14).unwrap();
        ref_atr.update(Candle::new_unchecked(
            close[0], high[0], low[0], close[0], 0.0, 0,
        ));
        let want: Vec<f64> = (0..high.len())
            .map(|i| {
                ref_atr
                    .update(Candle::new_unchecked(
                        close[i], high[i], low[i], close[i], 0.0, 0,
                    ))
                    .unwrap_or(f64::NAN)
            })
            .collect();
        assert!(bits_eq(&atr.batch_atr(&high, &low, &close), &want));
    }

    #[test]
    fn batch_atr_sub_period_slice_falls_back() {
        let (high, low, close) = columns(5);
        let mut atr = Atr::new(14).unwrap();
        let got = atr.batch_atr(&high, &low, &close);
        assert!(bits_eq(&got, &atr_replay(14, &high, &low, &close)));
        assert!(got.iter().all(|x| x.is_nan()));
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(48))]
        #[test]
        fn atr_matches_naive(
            period in 1usize..15,
            bars in proptest::collection::vec(
                (10.0_f64..1000.0, 0.0_f64..50.0, 0.0_f64..1.0),
                0..120,
            ),
        ) {
            // bars: (low, range, close_fraction) -> a valid OHLC candle.
            let hlc: Vec<(f64, f64, f64)> = bars
                .iter()
                .map(|&(low, range, frac)| (low + range, low, low + range * frac))
                .collect();
            let candles: Vec<Candle> = hlc.iter().map(|&(h, l, cl)| c(h, l, cl)).collect();
            let mut atr = Atr::new(period).unwrap();
            let got = atr.batch(&candles);
            let want = atr_naive(&hlc, period);
            proptest::prop_assert_eq!(got.len(), want.len());
            for (g, w) in got.iter().zip(want.iter()) {
                match (g, w) {
                    (None, None) => {}
                    (Some(a), Some(b)) => proptest::prop_assert!(
                        (a - b).abs() <= 1e-9 * a.abs().max(1.0),
                        "got={a} want={b}"
                    ),
                    _ => proptest::prop_assert!(false, "warmup mismatch"),
                }
            }
        }
    }
}
