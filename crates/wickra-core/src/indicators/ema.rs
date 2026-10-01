//! Exponential Moving Average.

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Exponential Moving Average with smoothing factor `alpha = 2 / (period + 1)`.
///
/// The first value is seeded with the simple mean of the first `period` inputs
/// (the classical TA-Lib convention). From then on each new input contributes
/// `alpha * input + (1 - alpha) * previous`.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Ema};
///
/// let mut indicator = Ema::new(3).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Ema {
    period: usize,
    alpha: f64,
    /// `1 - alpha`, precomputed so the recurrence avoids a subtraction per tick.
    /// Cached value, so the steady-state output is bit-for-bit unchanged.
    one_minus_alpha: f64,
    /// Latest EMA value, valid only once `seeded` is true. Stored as a bare `f64`
    /// (plus the `seeded` flag) rather than `Option<f64>` so the steady-state
    /// recurrence reads and writes 8 bytes with no enum-tag handling per tick.
    current: f64,
    /// Whether `current` holds a real value yet (warmup complete).
    seeded: bool,
    /// Running sum of the warmup inputs, the numerator of the seed mean. It
    /// starts at `-0.0`, the neutral element `f64::sum` folds from, and adds in
    /// input order, so the seed is bit-for-bit the mean a buffered
    /// `iter().sum()` would give — without a heap buffer that would keep the
    /// state out of registers on the per-tick path.
    warmup_sum: f64,
    /// Number of warmup inputs taken so far (saturates at `period` on seeding).
    warmup_count: usize,
}

impl Ema {
    /// Construct an EMA with the given period.
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
        let alpha = 2.0 / (period as f64 + 1.0);
        Ok(Self {
            period,
            alpha,
            one_minus_alpha: 1.0 - alpha,
            current: 0.0,
            seeded: false,
            warmup_sum: -0.0,
            warmup_count: 0,
        })
    }

    /// Construct an EMA with a custom smoothing factor `alpha in (0, 1]`.
    ///
    /// The reported `period` is derived from `alpha` via `2/alpha - 1` and rounded;
    /// `warmup_period()` falls back to `1` because the implementation seeds from the
    /// very first input.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `alpha` is not in `(0.0, 1.0]` or non-finite.
    pub fn with_alpha(alpha: f64) -> Result<Self> {
        if !alpha.is_finite() || alpha <= 0.0 || alpha > 1.0 {
            return Err(Error::InvalidPeriod {
                message: "alpha must be in (0.0, 1.0]",
            });
        }
        Ok(Self {
            period: 1,
            alpha,
            one_minus_alpha: 1.0 - alpha,
            current: 0.0,
            seeded: false,
            warmup_sum: -0.0,
            warmup_count: 0,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Smoothing factor.
    pub const fn alpha(&self) -> f64 {
        self.alpha
    }

    /// The cached `1 - alpha` the recurrence multiplies the previous value by.
    pub(crate) const fn one_minus_alpha(&self) -> f64 {
        self.one_minus_alpha
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        if self.seeded {
            Some(self.current)
        } else {
            None
        }
    }

    /// Whether the EMA has seen no input yet (neither seeded nor mid-warmup).
    /// Lets composite indicators (e.g. MACD) decide if a fast batch path is safe.
    pub(crate) fn is_fresh(&self) -> bool {
        !self.seeded && self.warmup_count == 0
    }

    /// Force the EMA into its seeded steady state with `current` as the latest
    /// value. Used by composite fused batch paths (MACD) to leave each sub-EMA
    /// where a per-tick `update` replay would, so a later `update` continues
    /// correctly. The post-seed recurrence never re-reads the warmup sum, so it
    /// is left as-is.
    pub(crate) fn seed_to(&mut self, current: f64) {
        self.current = current;
        self.seeded = true;
    }

    /// Vectorized batch returning one `f64` per input (`NaN` during warmup).
    ///
    /// Kept as an inherent method so existing callers need no trait import; it
    /// allocates the result and fills it through
    /// [`batch_nan_into`](Indicator::batch_nan_into), which carries the fast path.
    pub fn batch_nan(&mut self, inputs: &[f64]) -> Vec<f64> {
        crate::traits::BatchNanExt::batch_nan(self, inputs)
    }

    /// Internal helper that feeds a value without finiteness validation. The caller
    /// guarantees `input.is_finite()`. Used by MACD which has already validated.
    pub(crate) fn step_unchecked(&mut self, input: f64) -> Option<f64> {
        if self.seeded {
            let new = self
                .alpha
                .mul_add(input, self.one_minus_alpha * self.current);
            self.current = new;
            return Some(new);
        }
        self.warmup_sum += input;
        self.warmup_count += 1;
        if self.warmup_count == self.period {
            let seed = self.warmup_sum / self.period as f64;
            self.current = seed;
            self.seeded = true;
            return Some(seed);
        }
        None
    }
}

impl Indicator for Ema {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        self.step_unchecked(input)
    }

    fn reset(&mut self) {
        self.current = 0.0;
        self.seeded = false;
        self.warmup_sum = -0.0;
        self.warmup_count = 0;
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
        "EMA"
    }

    /// For a fresh indicator over an all-finite slice this runs the seed (mean
    /// of the first `period`) once and then the bare
    /// `alpha * x + (1 - alpha) * prev` recurrence in a tight loop with no
    /// per-element `is_finite`/`seeded` branch and no `Option` — yet uses the
    /// identical `mul_add`, so every value is *bit-for-bit* equal to replaying
    /// `update`. Any other state, or a non-finite element, defers to the exact
    /// `update` replay.
    fn batch_nan_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        assert_eq!(
            inputs.len(),
            out.len(),
            "batch output length must equal input length"
        );
        let p = self.period;
        if self.seeded || self.warmup_count != 0 || !inputs.iter().all(|x| x.is_finite()) {
            for (slot, &x) in out.iter_mut().zip(inputs) {
                *slot = self.update(x).unwrap_or(f64::NAN);
            }
            return;
        }

        let n = inputs.len();
        if n < p {
            // Not enough to seed; mirror `update` accumulating the warmup.
            for &x in inputs {
                self.warmup_sum += x;
            }
            self.warmup_count = n;
            out.fill(f64::NAN);
            return;
        }

        // Warmup `[0, p-1)` is `NaN`; the seed lands on `p-1`, the recurrence after.
        out[..p - 1].fill(f64::NAN);
        let seed_sum = inputs[..p].iter().copied().sum::<f64>();
        let seed = seed_sum / p as f64;
        out[p - 1] = seed;
        let mut cur = seed;
        let (alpha, oma) = (self.alpha, self.one_minus_alpha);
        for (slot, &x) in out[p..].iter_mut().zip(&inputs[p..]) {
            cur = alpha.mul_add(x, oma * cur);
            *slot = cur;
        }

        // Leave state exactly where `update` would: seeded on `current`, the
        // warmup sum and count covering the first `period` inputs.
        self.current = cur;
        self.seeded = true;
        self.warmup_sum = seed_sum;
        self.warmup_count = p;
    }

    /// SIMD kernel: after the same seed (the mean of the first `period`
    /// inputs) the recurrence `y = alpha * x + (1 - alpha) * y` runs as a
    /// linear-recurrence scan, eight values per step. Agrees with the exact
    /// batch to within a few units in the last place (the scan reassociates
    /// the recurrence through powers of `1 - alpha`); warmup `NaN`s and length
    /// are identical. Afterwards the EMA continues streaming from the kernel's
    /// last value.
    fn batch_fast_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        assert_eq!(
            inputs.len(),
            out.len(),
            "batch output length must equal input length"
        );
        let p = self.period;
        if self.seeded
            || self.warmup_count != 0
            || inputs.len() < p
            || !crate::fast::in_range(inputs)
        {
            self.batch_nan_into(inputs, out);
            return;
        }
        let (last, seed_sum) = wickra_simd::dispatch(crate::fast::EmaFast {
            x: inputs,
            period: p,
            alpha: self.alpha,
            one_minus_alpha: self.one_minus_alpha,
            out,
            _borrow: std::marker::PhantomData,
        });
        self.current = last;
        self.seeded = true;
        self.warmup_sum = seed_sum;
        self.warmup_count = p;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructors size their buffers from the period, so an absurd value used
    /// to abort the process before the caller saw anything: `usize::MAX` hit a
    /// capacity overflow inside `Vec`, and a billion quietly reserved eight
    /// gigabytes. Both are now rejected as an ordinary error.
    #[test]
    fn rejects_a_period_above_the_maximum() {
        assert!(matches!(
            Ema::new(usize::MAX),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Ema::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Ema::new(1_000_000_000),
            Err(Error::InvalidPeriod { .. })
        ));
        // A sane period is untouched.
        assert!(Ema::new(14).is_ok());
    }
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Independent reference: SMA-seeded EMA computed straight from the definition.
    fn ema_naive(prices: &[f64], period: usize) -> Vec<Option<f64>> {
        let alpha = 2.0 / (period as f64 + 1.0);
        let mut out = Vec::with_capacity(prices.len());
        let mut state: Option<f64> = None;
        for (i, &p) in prices.iter().enumerate() {
            if let Some(prev) = state {
                let v = alpha * p + (1.0 - alpha) * prev;
                state = Some(v);
                out.push(Some(v));
            } else if i + 1 == period {
                let seed = prices[..period].iter().sum::<f64>() / period as f64;
                state = Some(seed);
                out.push(Some(seed));
            } else {
                out.push(None);
            }
        }
        out
    }

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(Ema::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessor `period` (74-77) and the Indicator-impl
    /// `warmup_period` (123-125) + `name` (131-133). `alpha` and `value`
    /// are exercised by other tests and downstream consumers; only the
    /// three metadata methods were dead.
    #[test]
    fn accessors_and_metadata() {
        let ema = Ema::new(14).unwrap();
        assert_eq!(ema.period(), 14);
        assert_eq!(ema.warmup_period(), 14);
        assert_eq!(ema.name(), "EMA");
    }

    #[test]
    fn warmup_returns_none_until_seed() {
        let mut ema = Ema::new(3).unwrap();
        assert_eq!(ema.update(1.0), None);
        assert_eq!(ema.update(2.0), None);
        assert_eq!(ema.update(3.0), Some(2.0)); // seed = SMA([1,2,3]) = 2
    }

    #[test]
    fn first_value_equals_sma_seed() {
        let mut ema = Ema::new(5).unwrap();
        let inputs = [10.0, 20.0, 30.0, 40.0, 50.0];
        let mut last = None;
        for v in inputs {
            last = ema.update(v);
        }
        assert_relative_eq!(last.unwrap(), 30.0, epsilon = 1e-12);
    }

    #[test]
    fn alpha_matches_period_formula() {
        let ema = Ema::new(10).unwrap();
        assert_relative_eq!(ema.alpha(), 2.0 / 11.0, epsilon = 1e-15);
    }

    #[test]
    fn step_after_seed_uses_alpha_formula() {
        // period=3 => alpha = 0.5; seed = mean([1,2,3]) = 2; next input 10
        // expected = 0.5*10 + 0.5*2 = 6
        let mut ema = Ema::new(3).unwrap();
        ema.batch(&[1.0, 2.0, 3.0]);
        assert_relative_eq!(ema.update(10.0).unwrap(), 6.0, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_converges_to_constant() {
        let mut ema = Ema::new(10).unwrap();
        let out = ema.batch(&[42.0_f64; 100]);
        for x in out.iter().skip(9) {
            assert_relative_eq!(x.unwrap(), 42.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn with_alpha_validates_range() {
        assert!(Ema::with_alpha(0.5).is_ok());
        assert!(Ema::with_alpha(1.0).is_ok());
        assert!(matches!(
            Ema::with_alpha(0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Ema::with_alpha(1.5),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Ema::with_alpha(f64::NAN),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn reset_clears_state() {
        let mut ema = Ema::new(3).unwrap();
        ema.batch(&[1.0, 2.0, 3.0]);
        assert!(ema.is_ready());
        ema.reset();
        assert!(!ema.is_ready());
        assert_eq!(ema.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=30).map(f64::from).collect();
        let mut a = Ema::new(5).unwrap();
        let mut b = Ema::new(5).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut ema = Ema::new(3).unwrap();
        ema.batch(&[1.0, 2.0, 3.0]);
        let before = ema.value();
        assert_eq!(ema.update(f64::NAN), None);
        assert_eq!(ema.update(f64::INFINITY), None);
        // The rejected input must not have disturbed the state.
        assert_eq!(ema.value(), before);
    }

    fn bits_eq(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| x == y || (x.is_nan() && y.is_nan()))
    }

    fn ema_replay(period: usize, series: &[f64]) -> Vec<f64> {
        let mut e = Ema::new(period).unwrap();
        series
            .iter()
            .map(|&x| e.update(x).unwrap_or(f64::NAN))
            .collect()
    }

    #[test]
    fn batch_nan_fast_path_is_bit_identical() {
        let series: Vec<f64> = (0..300)
            .map(|i| (f64::from(i) * 0.25).cos() * 8.0 + 40.0)
            .collect();
        let mut ema = Ema::new(14).unwrap();
        let got = ema.batch_nan(&series);
        assert!(bits_eq(&got, &ema_replay(14, &series)));
        let mut ref_ema = Ema::new(14).unwrap();
        for &x in &series {
            ref_ema.update(x);
        }
        assert_eq!(ema.update(7.5), ref_ema.update(7.5));
    }

    /// The seed comes from a running sum rather than a buffered
    /// `iter().sum()`; the two must agree to the bit on every toolchain the
    /// crate supports, the sign of an all-negative-zero window included.
    #[test]
    fn seed_matches_a_buffered_sum_bit_for_bit() {
        let windows: [&[f64]; 3] = [
            &[-0.0, -0.0, -0.0],
            &[1e16, 1.0, -1e16, 3.5, 0.1],
            &[0.3, 0.1, 0.7, 0.2, 0.9, 0.4, 0.6],
        ];
        for window in windows {
            let buffered = window.iter().copied().sum::<f64>() / window.len() as f64;
            let mut ema = Ema::new(window.len()).unwrap();
            let seed = window.iter().filter_map(|&x| ema.update(x)).last().unwrap();
            assert_eq!(seed.to_bits(), buffered.to_bits());
            let mut out = vec![0.0; window.len()];
            Ema::new(window.len())
                .unwrap()
                .batch_nan_into(window, &mut out);
            assert_eq!(out[window.len() - 1].to_bits(), buffered.to_bits());
        }
    }

    /// Into a caller buffer that already holds values, both the seeded path and
    /// the sub-period path must write every cell exactly as the replay would.
    #[test]
    fn batch_nan_into_overwrites_a_dirty_buffer() {
        let series: Vec<f64> = (0..120).map(|i| f64::from(i % 11) * 0.75 + 20.0).collect();
        let mut out = vec![-1.0; series.len()];
        Ema::new(10).unwrap().batch_nan_into(&series, &mut out);
        assert!(bits_eq(&out, &ema_replay(10, &series)));
        let mut short = [-1.0; 4];
        Ema::new(10)
            .unwrap()
            .batch_nan_into(&series[..4], &mut short);
        assert!(short.iter().all(|x| x.is_nan()));
    }

    #[test]
    fn batch_nan_falls_back_on_non_finite() {
        let series = [1.0, 2.0, 3.0, f64::INFINITY, 5.0, 6.0, 7.0];
        let mut ema = Ema::new(3).unwrap();
        assert!(bits_eq(&ema.batch_nan(&series), &ema_replay(3, &series)));
    }

    #[test]
    fn batch_nan_falls_back_when_warming() {
        let mut ema = Ema::new(3).unwrap();
        ema.update(10.0); // mid-warmup: one input taken, not seeded
        let series = [1.0, 2.0, 3.0, 4.0];
        let mut ref_ema = Ema::new(3).unwrap();
        ref_ema.update(10.0);
        let want: Vec<f64> = series
            .iter()
            .map(|&x| ref_ema.update(x).unwrap_or(f64::NAN))
            .collect();
        assert!(bits_eq(&ema.batch_nan(&series), &want));
    }

    #[test]
    fn batch_nan_sub_period_slice_stays_unseeded() {
        let series = [1.0, 2.0];
        let mut ema = Ema::new(5).unwrap();
        let got = ema.batch_nan(&series);
        assert!(got.iter().all(|x| x.is_nan()) && got.len() == 2);
        assert!(!ema.is_ready());
        // Warmup state was stashed: feeding the rest seeds exactly as a full stream.
        assert!(bits_eq(
            &[ema.update(3.0).unwrap_or(f64::NAN)],
            &[ema_replay(5, &[1.0, 2.0, 3.0])[2]]
        ));
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(48))]
        #[test]
        fn ema_matches_naive(
            period in 1usize..20,
            prices in proptest::collection::vec(-1000.0_f64..1000.0, 0..150),
        ) {
            let mut ema = Ema::new(period).unwrap();
            let got = ema.batch(&prices);
            let want = ema_naive(&prices, period);
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
