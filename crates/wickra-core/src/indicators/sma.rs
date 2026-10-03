//! Simple Moving Average.

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Simple Moving Average over a fixed window.
///
/// Maintains a rolling sum so each update is O(1). Output equals
/// `sum(last `period` prices) / period` once the window is full; `None` before.
///
/// On long-running streams a single-subtract incremental sum can accumulate
/// rounding error (catastrophic cancellation when values of very different
/// magnitudes are alternately added and removed). To keep drift bounded, the
/// running sum is reseeded from the live window every `16 · period` updates —
/// O(1) amortised cost (`O(period)` work amortised over `O(period)` updates),
/// zero observable behaviour change on inputs that did not drift to begin
/// with, and a strict cap on accumulated rounding for streams that did.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Sma};
///
/// let mut indicator = Sma::new(3).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Sma {
    period: usize,
    /// Fixed-capacity ring buffer of the last `period` finite inputs. A flat
    /// `Box<[f64]>` with a manual write cursor beats `VecDeque` on this hot path:
    /// sequential storage, branchless wraparound, no per-call bookkeeping.
    buf: Box<[f64]>,
    /// Index of the next slot to write — also the oldest element once full.
    head: usize,
    /// Number of slots filled, saturating at `period`.
    count: usize,
    sum: f64,
    /// Number of finite updates since the running `sum` was last reseeded from
    /// the live window. Caps accumulated floating-point drift on long streams.
    /// See [`RECOMPUTE_EVERY`] below.
    updates_since_recompute: usize,
}

/// How often (in finite updates) the incremental sum is reseeded from the live
/// window. The multiplier `16` is the smallest power of two that keeps the
/// amortised cost flat under any `period` while still bounding any drift to
/// roughly `16 · period · ULP · max(|x|)` — sub-picodollar on real-world price
/// scales.
const RECOMPUTE_EVERY: usize = 16;

impl Sma {
    /// Construct a new SMA with the given window length.
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
            buf: vec![0.0; period].into_boxed_slice(),
            head: 0,
            count: 0,
            sum: 0.0,
            updates_since_recompute: 0,
        })
    }

    /// Configured window length.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Whether the SMA has taken no input since construction or reset.
    pub(crate) fn is_fresh(&self) -> bool {
        self.count == 0 && self.updates_since_recompute == 0
    }

    /// Current value if available.
    pub fn value(&self) -> Option<f64> {
        if self.count == self.period {
            Some(self.sum / self.period as f64)
        } else {
            None
        }
    }

    /// Vectorized batch returning one `f64` per input (`NaN` during warmup).
    ///
    /// Kept as an inherent method so existing callers need no trait import; it
    /// allocates the result and fills it through
    /// [`batch_nan_into`](Indicator::batch_nan_into), which carries the fast path.
    pub fn batch_nan(&mut self, inputs: &[f64]) -> Vec<f64> {
        crate::traits::BatchNanExt::batch_nan(self, inputs)
    }
}

impl Indicator for Sma {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        if self.count == self.period {
            // Window full: overwrite the oldest slot (at `head`). Each step is a
            // single f64 add/subtract — O(1) but introduces ~1 ULP of rounding
            // noise. The periodic reseed below caps the accumulated drift.
            self.sum -= self.buf[self.head];
            self.buf[self.head] = input;
            self.sum += input;
        } else {
            self.buf[self.head] = input;
            self.sum += input;
            self.count += 1;
        }
        // Branchless-ish wraparound, cheaper than `% period`.
        self.head += 1;
        if self.head == self.period {
            self.head = 0;
        }
        self.updates_since_recompute += 1;
        if self.updates_since_recompute >= RECOMPUTE_EVERY * self.period {
            // Reseed in chronological order (oldest at `head`) so the running sum
            // tracks a fresh from-scratch mean to the bit on stable inputs.
            self.sum = self.buf[self.head..]
                .iter()
                .chain(&self.buf[..self.head])
                .copied()
                .sum();
            self.updates_since_recompute = 0;
        }
        self.value()
    }

    fn reset(&mut self) {
        self.head = 0;
        self.count = 0;
        self.sum = 0.0;
        self.updates_since_recompute = 0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.count == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "SMA"
    }

    /// For a fresh, all-finite slice this inlines `update`'s rolling sum and
    /// drift-reseed, writing the mean as a bare `f64` (warmup → `NaN`) straight
    /// into `out`. Same add/subtract order, same reseed cadence, same
    /// `sum / period` division — so it is *bit-for-bit* equal to replaying
    /// `update`, including the long-stream drift bound. Any other state, or a
    /// non-finite element, defers to the exact `update` replay.
    fn batch_nan_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        assert_eq!(
            inputs.len(),
            out.len(),
            "batch output length must equal input length"
        );
        let p = self.period;
        if self.count != 0
            || self.updates_since_recompute != 0
            || !inputs.iter().all(|x| x.is_finite())
        {
            for (slot, &x) in out.iter_mut().zip(inputs) {
                *slot = self.update(x).unwrap_or(f64::NAN);
            }
            return;
        }

        let p_f64 = p as f64;
        // Walk the ring one lap at a time and step through it with an iterator
        // rather than indexing. Indexing put a bounds check in the hot loop,
        // which under `panic = "unwind"` becomes an unwind edge carrying drop
        // glue for `out` and blocks vectorisation; the same loop is roughly 40%
        // faster without it.
        //
        // A lap is exactly `period` inputs, which is what makes this equivalent:
        // the fast path only runs from a fresh state, so `head` is 0 at every
        // lap boundary, and `RECOMPUTE_EVERY * period` is a whole multiple of
        // `period`, so the drift reseed can only ever fall on one. At a reseed
        // `head` is therefore 0 and the chronological order the reseed needs is
        // simply the buffer in order. Only the final lap can be partial, since
        // any shorter chunk means the input ran out.
        let mut rest = inputs;
        let mut written: &mut [f64] = out;
        let mut lap = 0_usize;
        while !rest.is_empty() {
            let take = rest.len().min(p);
            let (chunk, tail) = rest.split_at(take);
            rest = tail;
            let (lap_out, out_tail) = written.split_at_mut(take);
            written = out_tail;
            if lap == 0 {
                for ((slot, &x), cell) in self.buf.iter_mut().zip(chunk).zip(lap_out.iter_mut()) {
                    *slot = x;
                    self.sum += x;
                    self.count += 1;
                    *cell = if self.count == p {
                        self.sum / p_f64
                    } else {
                        f64::NAN
                    };
                }
            } else {
                for ((slot, &x), cell) in self.buf.iter_mut().zip(chunk).zip(lap_out.iter_mut()) {
                    self.sum -= *slot;
                    *slot = x;
                    self.sum += x;
                    *cell = self.sum / p_f64;
                }
            }
            self.updates_since_recompute += take;
            if self.updates_since_recompute >= RECOMPUTE_EVERY * p {
                self.sum = self.buf.iter().copied().sum();
                self.updates_since_recompute = 0;
                // `update` reseeds *before* emitting the value for the input
                // that tripped it, so this lap's last value has to come from
                // the reseeded sum rather than the incremental one. The reseed
                // cannot fire before `RECOMPUTE_EVERY` complete laps, so the
                // window is full and this lap wrote a value for every input.
                *lap_out
                    .last_mut()
                    .expect("a lap writes at least one value before it can reseed") =
                    self.sum / p_f64;
            }
            lap += 1;
        }
        self.head = inputs.len() % p;
    }

    /// SIMD kernel: rolling sums as a prefix scan of `x[i] - x[i - period]`,
    /// re-anchored on an exact window sum every `16 · period` values like the
    /// exact path's reseed, each sum scaled by `1 / period`. Agrees with the
    /// exact batch to within a few units in the last place (a multiply by the
    /// reciprocal replaces the division, and the running sum is reassociated);
    /// warmup `NaN`s and length are identical. Afterwards the window holds the
    /// last `period` inputs and its sum is recomputed exactly, so streaming
    /// continues from a freshly reseeded state.
    fn batch_fast_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        assert_eq!(
            inputs.len(),
            out.len(),
            "batch output length must equal input length"
        );
        let p = self.period;
        let n = inputs.len();
        if self.count != 0
            || self.updates_since_recompute != 0
            || n < p
            || !crate::fast::in_range(inputs)
        {
            self.batch_nan_into(inputs, out);
            return;
        }
        wickra_simd::dispatch(crate::fast::SmaFast {
            x: inputs,
            period: p,
            out,
            _borrow: std::marker::PhantomData,
        });
        for (idx, &x) in inputs.iter().enumerate().skip(n - p) {
            self.buf[idx % p] = x;
        }
        self.head = n % p;
        self.count = p;
        self.sum = self.buf[self.head..]
            .iter()
            .chain(&self.buf[..self.head])
            .copied()
            .sum();
        self.updates_since_recompute = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same bound applies to every constructor that sizes a buffer from its
    /// period; SMA allocates eagerly, so it is the sharpest case.
    #[test]
    fn rejects_a_period_above_the_maximum() {
        assert!(matches!(
            Sma::new(usize::MAX),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Sma::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(Sma::new(20).is_ok());
    }
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;
    use std::collections::VecDeque;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(Sma::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessor `period` (70-72) and the Indicator-impl
    /// `warmup_period` (115-117) + `name` (123-125). Existing tests
    /// inspect SMA output but never query the metadata.
    #[test]
    fn accessors_and_metadata() {
        let sma = Sma::new(20).unwrap();
        assert_eq!(sma.period(), 20);
        assert_eq!(sma.warmup_period(), 20);
        assert_eq!(sma.name(), "SMA");
    }

    #[test]
    fn warmup_returns_none() {
        let mut sma = Sma::new(3).unwrap();
        assert_eq!(sma.update(1.0), None);
        assert_eq!(sma.update(2.0), None);
        assert_eq!(sma.update(3.0), Some(2.0));
    }

    #[test]
    fn rolls_window_after_full() {
        let mut sma = Sma::new(3).unwrap();
        let out: Vec<_> = [1.0, 2.0, 3.0, 4.0, 5.0]
            .iter()
            .map(|p| sma.update(*p))
            .collect();
        assert_eq!(out, vec![None, None, Some(2.0), Some(3.0), Some(4.0)]);
    }

    #[test]
    fn period_one_is_pass_through() {
        let mut sma = Sma::new(1).unwrap();
        assert_eq!(sma.update(5.0), Some(5.0));
        assert_eq!(sma.update(10.0), Some(10.0));
    }

    #[test]
    fn ignores_non_finite_input_but_keeps_state() {
        let mut sma = Sma::new(3).unwrap();
        sma.update(1.0);
        sma.update(2.0);
        sma.update(3.0);
        assert_eq!(sma.update(f64::NAN), None);
        assert_eq!(sma.update(f64::INFINITY), None);
        // Non-finite inputs were not pushed; window still holds 1,2,3.
        assert_eq!(sma.update(6.0), Some((2.0 + 3.0 + 6.0) / 3.0));
    }

    #[test]
    fn reset_clears_state() {
        let mut sma = Sma::new(3).unwrap();
        sma.batch(&[1.0, 2.0, 3.0]);
        assert!(sma.is_ready());
        sma.reset();
        assert!(!sma.is_ready());
        assert_eq!(sma.update(10.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=20).map(f64::from).collect();
        let mut a = Sma::new(5).unwrap();
        let batch = a.batch(&prices);
        let mut b = Sma::new(5).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn known_reference_values() {
        // SMA(3) of [2, 4, 6, 8, 10] -> [_, _, 4, 6, 8]
        let mut sma = Sma::new(3).unwrap();
        let out = sma.batch(&[2.0, 4.0, 6.0, 8.0, 10.0]);
        assert_eq!(out[2], Some(4.0));
        assert_eq!(out[3], Some(6.0));
        assert_eq!(out[4], Some(8.0));
    }

    #[test]
    fn constant_series_yields_constant_sma() {
        let mut sma = Sma::new(5).unwrap();
        let v = sma.batch(&[7.0; 10]);
        for x in v.iter().skip(4) {
            assert_relative_eq!(x.unwrap(), 7.0, epsilon = 1e-12);
        }
    }

    /// NaN-aware bit-equality for the `f64`-with-NaN-warmup batch outputs.
    fn bits_eq(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| x == y || (x.is_nan() && y.is_nan()))
    }

    fn sma_replay(period: usize, series: &[f64]) -> Vec<f64> {
        let mut s = Sma::new(period).unwrap();
        series
            .iter()
            .map(|&x| s.update(x).unwrap_or(f64::NAN))
            .collect()
    }

    #[test]
    fn batch_nan_fast_path_is_bit_identical_with_reseed() {
        // > 16*period inputs so the drift-reseed branch fires inside batch_nan.
        let series: Vec<f64> = (0..500)
            .map(|i| (f64::from(i) * 0.2).sin() * 10.0 + 50.0)
            .collect();
        let mut sma = Sma::new(14).unwrap();
        let got = sma.batch_nan(&series);
        assert!(bits_eq(&got, &sma_replay(14, &series)));
        // State left where the replay would: continued updates agree.
        let mut ref_sma = Sma::new(14).unwrap();
        for &x in &series {
            ref_sma.update(x);
        }
        assert_eq!(sma.update(42.0), ref_sma.update(42.0));
    }

    /// Into a caller buffer that already holds values, the fast path must write
    /// every cell — warmup positions included — exactly as the replay would.
    #[test]
    fn batch_nan_into_overwrites_a_dirty_buffer() {
        let series: Vec<f64> = (0..300).map(|i| f64::from(i % 17) * 1.5 + 3.0).collect();
        let mut out = vec![123.0; series.len()];
        Sma::new(9).unwrap().batch_nan_into(&series, &mut out);
        assert!(bits_eq(&out, &sma_replay(9, &series)));
    }

    #[test]
    fn batch_nan_falls_back_on_non_finite() {
        let series = [1.0, 2.0, f64::NAN, 4.0, 5.0, 6.0];
        let mut sma = Sma::new(3).unwrap();
        assert!(bits_eq(&sma.batch_nan(&series), &sma_replay(3, &series)));
    }

    #[test]
    fn batch_nan_falls_back_when_not_fresh() {
        let mut sma = Sma::new(3).unwrap();
        sma.update(99.0);
        let series = [1.0, 2.0, 3.0, 4.0];
        let mut ref_sma = Sma::new(3).unwrap();
        ref_sma.update(99.0);
        let want: Vec<f64> = series
            .iter()
            .map(|&x| ref_sma.update(x).unwrap_or(f64::NAN))
            .collect();
        assert!(bits_eq(&sma.batch_nan(&series), &want));
    }

    #[test]
    fn batch_nan_sub_period_slice_is_all_nan() {
        let series = [1.0, 2.0, 3.0];
        let mut sma = Sma::new(10).unwrap();
        let got = sma.batch_nan(&series);
        assert!(bits_eq(&got, &sma_replay(10, &series)));
        assert!(got.iter().all(|x| x.is_nan()));
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(64))]
        #[test]
        fn sma_matches_naive_definition(
            period in 1usize..20,
            prices in proptest::collection::vec(-1000.0_f64..1000.0, 0..200),
        ) {
            let mut sma = Sma::new(period).unwrap();
            let stream: Vec<_> = prices.iter().map(|p| sma.update(*p)).collect();
            for (i, got) in stream.iter().enumerate() {
                if i + 1 < period {
                    proptest::prop_assert!(got.is_none());
                } else {
                    let window = &prices[i + 1 - period..=i];
                    let expected = window.iter().sum::<f64>() / period as f64;
                    let actual = got.expect("ready");
                    proptest::prop_assert!(
                        (actual - expected).abs() < 1e-9,
                        "i={i} actual={actual} expected={expected}"
                    );
                }
            }
        }
    }

    /// Long-running stability check. Runs more updates than `RECOMPUTE_EVERY *
    /// period` so the periodic reseed must fire several times, then asserts
    /// that the reported SMA still equals a fresh from-scratch mean over the
    /// live window to within tight floating-point tolerance. Inputs swing
    /// between two magnitudes (`1e9` and `1.0`) — a pattern designed to
    /// expose catastrophic cancellation in a naive single-subtract sum.
    #[test]
    fn long_stream_drift_stays_bounded() {
        let period = 20;
        let mut sma = Sma::new(period).unwrap();
        let mut window: VecDeque<f64> = VecDeque::with_capacity(period);
        // `RECOMPUTE_EVERY * period * 5` updates → recompute fires 5+ times.
        let n_updates = 16 * period * 5;
        for i in 0..n_updates {
            let v = if i % 2 == 0 { 1e9 } else { 1.0 };
            sma.update(v);
            if window.len() == period {
                window.pop_front();
            }
            window.push_back(v);
        }
        let from_scratch: f64 = window.iter().sum::<f64>() / period as f64;
        let got = sma.value().expect("warmed up");
        assert!(
            (got - from_scratch).abs() < 1e-6,
            "SMA drift exceeds 1e-6 over {n_updates} updates: got={got}, scratch={from_scratch}"
        );
    }
}
