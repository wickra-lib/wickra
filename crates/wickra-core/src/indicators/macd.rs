//! Moving Average Convergence Divergence (MACD).

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::traits::Indicator;

/// MACD output: the three classic series at a given step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacdOutput {
    /// Fast EMA − slow EMA.
    pub macd: f64,
    /// EMA of `macd` over the signal period.
    pub signal: f64,
    /// `macd − signal`.
    pub histogram: f64,
}

/// MACD = EMA(fast) − EMA(slow), with a signal EMA on top.
///
/// Standard parameters are `fast = 12`, `slow = 26`, `signal = 9`. The signal EMA
/// is seeded from the first `signal` raw MACD values, so the first full
/// [`MacdOutput`] is emitted after `slow + signal − 1` inputs (assuming the
/// slow EMA seeded by then).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, MacdIndicator};
///
/// let mut indicator = MacdIndicator::new(3, 6, 3).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct MacdIndicator {
    fast: Ema,
    slow: Ema,
    signal_ema: Ema,
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
    last: Option<MacdOutput>,
}

impl MacdIndicator {
    /// Construct a MACD with the given periods.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if any period is zero, and
    /// [`Error::InvalidPeriod`] if `fast >= slow`.
    pub fn new(fast: usize, slow: usize, signal: usize) -> Result<Self> {
        if fast == 0 || slow == 0 || signal == 0 {
            return Err(Error::PeriodZero);
        }
        if fast >= slow {
            return Err(Error::InvalidPeriod {
                message: "fast period must be strictly less than slow period",
            });
        }
        Ok(Self {
            fast: Ema::new(fast)?,
            slow: Ema::new(slow)?,
            signal_ema: Ema::new(signal)?,
            fast_period: fast,
            slow_period: slow,
            signal_period: signal,
            last: None,
        })
    }

    /// Default `(12, 26, 9)` configuration, matching every classical chart package.
    pub fn classic() -> Self {
        Self::new(12, 26, 9).expect("classic MACD periods are valid")
    }

    /// Configured periods as `(fast, slow, signal)`.
    pub const fn periods(&self) -> (usize, usize, usize) {
        (self.fast_period, self.slow_period, self.signal_period)
    }

    /// Most recent fully-computed output if available.
    pub const fn value(&self) -> Option<MacdOutput> {
        self.last
    }

    /// Vectorized flat batch for bindings: `n * 3` values laid out as
    /// `[macd, signal, histogram]` per input row, warmup rows all `NaN`.
    ///
    /// Allocates the result and fills it through
    /// [`batch_macd_into`](Self::batch_macd_into). Separate from the trait
    /// [`batch`](crate::BatchExt::batch), which stays a bit-identical `update`
    /// replay.
    pub fn batch_macd(&mut self, inputs: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; inputs.len() * 3];
        self.batch_macd_into(inputs, &mut out);
        out
    }

    /// [`batch_macd`](Self::batch_macd) into a caller-owned buffer of
    /// `inputs.len() * 3` values, overwriting every cell.
    ///
    /// For a fresh slice long enough for a full output, whose values are all
    /// finite with magnitude at most `1e300`, it runs the fast EMA, slow EMA
    /// and signal EMA as three recurrences in one pass: the warmup phases run
    /// on their own so the steady-state loop has no per-row branch, and the
    /// three independent chains overlap in the pipeline. The seeds are the same
    /// running means `Ema` keeps (summed from `-0.0` in input order) and the
    /// recurrences the same `mul_add`, so every value is *bit-for-bit* equal to
    /// replaying `update`. The magnitude bound keeps every EMA, sum and MACD
    /// difference finite, which is what lets the signal EMA take every MACD
    /// value (streaming skips a non-finite one). Anything else (not fresh, a
    /// value out of range, or too short to emit) replays `update`.
    ///
    /// # Panics
    ///
    /// Panics if `out.len() != inputs.len() * 3`.
    pub fn batch_macd_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        let n = inputs.len();
        assert_eq!(
            out.len(),
            n * 3,
            "batch_macd output must hold three values per input"
        );
        let (fp, sp, gp) = (self.fast_period, self.slow_period, self.signal_period);
        // First full output needs the slow EMA seeded (index sp-1) plus gp signal
        // values: index sp + gp - 2. Below that, or non-fresh/out-of-range, replay.
        if self.last.is_some()
            || !self.fast.is_fresh()
            || !self.slow.is_fresh()
            || !self.signal_ema.is_fresh()
            || n < sp + gp - 1
            || !inputs.iter().all(|x| x.abs() <= 1e300)
        {
            for (row, &x) in out.chunks_exact_mut(3).zip(inputs) {
                match self.update(x) {
                    Some(o) => row.copy_from_slice(&[o.macd, o.signal, o.histogram]),
                    None => row.fill(f64::NAN),
                }
            }
            return;
        }

        let (fast_val, slow_val, sig) = wickra_simd::dispatch(FusedMacd {
            inputs,
            out,
            periods: (fp, sp, gp),
            alphas: (
                self.fast.alpha(),
                self.slow.alpha(),
                self.signal_ema.alpha(),
            ),
        });

        // Leave every sub-EMA and `last` where a full `update` replay would.
        self.fast.seed_to(fast_val);
        self.slow.seed_to(slow_val);
        self.signal_ema.seed_to(sig);
        let tail = &out[(n - 1) * 3..];
        self.last = Some(MacdOutput {
            macd: tail[0],
            signal: tail[1],
            histogram: tail[2],
        });
    }
}

/// The fused MACD fast path as a [`wickra_simd::Kernel`], so its three
/// `mul_add` chains compile to hardware FMA where the CPU has it (the baseline
/// build calls the C library's `fma`, which serialises the chains). Returns the
/// final fast EMA, slow EMA and signal EMA.
struct FusedMacd<'a> {
    inputs: &'a [f64],
    out: &'a mut [f64],
    periods: (usize, usize, usize),
    alphas: (f64, f64, f64),
}

// Inlining into the dispatching function is what compiles the body with its
// features; see `wickra_simd::Kernel`.
#[allow(clippy::inline_always)]
impl wickra_simd::Kernel for FusedMacd<'_> {
    type Output = (f64, f64, f64);

    #[inline(always)]
    fn run<S: wickra_simd::Simd>(self, _simd: S) -> (f64, f64, f64) {
        let Self {
            inputs,
            out,
            periods: (fp, sp, gp),
            alphas: (fa, sa, ga),
        } = self;
        let (fo, so, go) = (1.0 - fa, 1.0 - sa, 1.0 - ga);
        let first_full = sp + gp - 2;

        // Warmup rows carry no full output.
        out[..first_full * 3].fill(f64::NAN);

        // Fast EMA seed (the mean of the first fp inputs), then its recurrence
        // up to the slow seed; the slow EMA only accumulates until index sp-1.
        let mut fsum = -0.0_f64;
        for &x in &inputs[..fp] {
            fsum += x;
        }
        let mut fast_val = fsum / fp as f64;
        let mut ssum = -0.0_f64;
        for &x in &inputs[..sp] {
            ssum += x;
        }
        for &x in &inputs[fp..sp] {
            fast_val = fa.mul_add(x, fo * fast_val);
        }
        let mut slow_val = ssum / sp as f64;

        // The signal EMA seeds on the mean of the first gp MACD values
        // (indices sp-1 ..= sp+gp-2).
        let mut gsum = -0.0_f64 + (fast_val - slow_val);
        for &x in &inputs[sp..=first_full] {
            fast_val = fa.mul_add(x, fo * fast_val);
            slow_val = sa.mul_add(x, so * slow_val);
            gsum += fast_val - slow_val;
        }
        let mut sig = gsum / gp as f64;
        let macd = fast_val - slow_val;
        out[first_full * 3..first_full * 3 + 3].copy_from_slice(&[macd, sig, macd - sig]);

        // Steady state: three independent recurrences per row, no branch.
        for (row, &x) in out[(first_full + 1) * 3..]
            .chunks_exact_mut(3)
            .zip(&inputs[first_full + 1..])
        {
            fast_val = fa.mul_add(x, fo * fast_val);
            slow_val = sa.mul_add(x, so * slow_val);
            let macd = fast_val - slow_val;
            sig = ga.mul_add(macd, go * sig);
            row.copy_from_slice(&[macd, sig, macd - sig]);
        }
        (fast_val, slow_val, sig)
    }
}

impl Indicator for MacdIndicator {
    type Input = f64;
    type Output = MacdOutput;

    #[inline]
    fn update(&mut self, input: f64) -> Option<MacdOutput> {
        if !input.is_finite() {
            return None;
        }

        let fast = self.fast.update(input);
        let slow = self.slow.update(input);

        match (fast, slow) {
            (Some(f), Some(s)) => {
                let macd = f - s;
                let signal = self.signal_ema.update(macd)?;
                let out = MacdOutput {
                    macd,
                    signal,
                    histogram: macd - signal,
                };
                self.last = Some(out);
                Some(out)
            }
            _ => None,
        }
    }

    fn reset(&mut self) {
        self.fast.reset();
        self.slow.reset();
        self.signal_ema.reset();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // Slow EMA needs `slow` inputs to seed; signal EMA needs another `signal - 1`.
        self.slow_period + self.signal_period - 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "MACD"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_fast_geq_slow() {
        assert!(matches!(
            MacdIndicator::new(26, 12, 9),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            MacdIndicator::new(12, 12, 9),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    /// Cover the const accessors `periods` / `value` (81-88) and the
    /// Indicator-impl `name` body (135-137). `warmup_period` is exercised
    /// elsewhere.
    #[test]
    fn accessors_and_metadata() {
        let mut m = MacdIndicator::new(12, 26, 9).unwrap();
        assert_eq!(m.periods(), (12, 26, 9));
        assert_eq!(m.name(), "MACD");
        assert!(m.value().is_none());
        for i in 1..=m.warmup_period() {
            m.update(100.0 + f64::from(u32::try_from(i).unwrap()));
        }
        assert!(m.value().is_some());
    }

    #[test]
    fn rejects_zero_periods() {
        assert!(matches!(
            MacdIndicator::new(0, 26, 9),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            MacdIndicator::new(12, 0, 9),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            MacdIndicator::new(12, 26, 0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn first_emission_matches_warmup_period() {
        let prices: Vec<f64> = (1..=60).map(f64::from).collect();
        let mut macd = MacdIndicator::classic();
        let out = macd.batch(&prices);
        let warmup = macd.warmup_period();
        // Indices 0..warmup-1 are None, index warmup-1 might be Some or might still need
        // the signal EMA's seeding. Our warmup_period is the index at which the first
        // signal value appears: slow + signal - 1.
        for x in out.iter().take(warmup - 1) {
            assert!(x.is_none(), "expected None within warmup");
        }
        assert!(
            out[warmup - 1].is_some(),
            "expected first emission at warmup_period - 1 ({warmup} idx)"
        );
    }

    #[test]
    fn histogram_equals_macd_minus_signal() {
        let prices: Vec<f64> = (1..=80).map(|i| f64::from(i) * 0.5).collect();
        let mut macd = MacdIndicator::classic();
        for v in macd.batch(&prices).into_iter().flatten() {
            assert_relative_eq!(v.histogram, v.macd - v.signal, epsilon = 1e-12);
        }
    }

    #[test]
    fn constant_series_yields_zero_macd_eventually() {
        let mut macd = MacdIndicator::classic();
        let out = macd.batch(&[100.0_f64; 200]);
        // Both EMAs converge to 100, so MACD must approach 0.
        let last = out.iter().rev().flatten().next().expect("emits a value");
        assert_relative_eq!(last.macd, 0.0, epsilon = 1e-9);
        assert_relative_eq!(last.signal, 0.0, epsilon = 1e-9);
        assert_relative_eq!(last.histogram, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn rising_series_macd_positive_then_signal_catches_up() {
        let prices: Vec<f64> = (1..=200).map(f64::from).collect();
        let mut macd = MacdIndicator::classic();
        let out = macd.batch(&prices);
        let last = out.iter().rev().flatten().next().unwrap();
        assert!(last.macd > 0.0, "rising series must yield positive MACD");
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=100)
            .map(|i| (f64::from(i) * 0.4).cos() * 10.0)
            .collect();
        let mut a = MacdIndicator::classic();
        let mut b = MacdIndicator::classic();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut macd = MacdIndicator::classic();
        macd.batch(&(1..=80).map(f64::from).collect::<Vec<_>>());
        assert!(macd.is_ready());
        macd.reset();
        assert!(!macd.is_ready());
        assert_eq!(macd.update(1.0), None);
    }

    fn bits_eq(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| x == y || (x.is_nan() && y.is_nan()))
    }

    /// Flat `n*3` `[macd, signal, histogram]` replay of `update`.
    fn macd_replay(series: &[f64]) -> Vec<f64> {
        let mut m = MacdIndicator::classic();
        let mut out = Vec::with_capacity(series.len() * 3);
        for &x in series {
            match m.update(x) {
                Some(o) => out.extend_from_slice(&[o.macd, o.signal, o.histogram]),
                None => out.extend_from_slice(&[f64::NAN; 3]),
            }
        }
        out
    }

    #[test]
    fn batch_macd_fast_path_is_bit_identical() {
        let series: Vec<f64> = (0..300)
            .map(|i| (f64::from(i) * 0.4).cos() * 10.0 + 100.0)
            .collect();
        let mut macd = MacdIndicator::classic();
        let got = macd.batch_macd(&series);
        assert!(bits_eq(&got, &macd_replay(&series)));
        // Sub-EMA + last state left where the replay would: continued update agrees.
        let mut ref_macd = MacdIndicator::classic();
        for &x in &series {
            ref_macd.update(x);
        }
        let (a, b) = (macd.update(101.0), ref_macd.update(101.0));
        assert_eq!(a.is_some(), b.is_some());
        assert_relative_eq!(a.unwrap().macd, b.unwrap().macd, epsilon = 1e-12);
    }

    /// Strict bit equality (`-0.0` differs from `0.0`; the only `NaN`s here are
    /// the warmup `f64::NAN`s both sides write).
    fn to_bits(v: &[f64]) -> Vec<u64> {
        v.iter().map(|x| x.to_bits()).collect()
    }

    /// The fused kernel run through the dispatcher (AVX2 + FMA where available)
    /// and in the baseline build must write the same bits and end in the same
    /// state.
    #[test]
    fn fused_kernel_is_identical_on_every_dispatch_path() {
        let series: Vec<f64> = (0..3000)
            .map(|i| (f64::from(i) * 0.093).sin() * 7.0 + f64::from(i % 13) * 0.4 + 80.0)
            .collect();
        let alphas = (2.0 / 13.0, 2.0 / 27.0, 2.0 / 10.0);
        let mut dispatched = vec![0.0; series.len() * 3];
        let mut baseline = vec![0.0; series.len() * 3];
        let a = wickra_simd::dispatch(FusedMacd {
            inputs: &series,
            out: &mut dispatched,
            periods: (12, 26, 9),
            alphas,
        });
        let b = wickra_simd::run_baseline(FusedMacd {
            inputs: &series,
            out: &mut baseline,
            periods: (12, 26, 9),
            alphas,
        });
        assert_eq!(to_bits(&dispatched), to_bits(&baseline));
        assert_eq!(
            [a.0.to_bits(), a.1.to_bits(), a.2.to_bits()],
            [b.0.to_bits(), b.1.to_bits(), b.2.to_bits()]
        );
    }

    /// An all-negative-zero window must seed exactly like the streaming EMAs,
    /// which sum from `-0.0`: every output keeps its sign bit.
    #[test]
    fn batch_macd_negative_zero_series_matches_to_the_bit() {
        let series = vec![-0.0_f64; 60];
        let got = MacdIndicator::classic().batch_macd(&series);
        assert_eq!(to_bits(&got), to_bits(&macd_replay(&series)));
    }

    /// Into a buffer that already holds values, every cell is overwritten —
    /// warmup rows with `NaN`, the rest with the replay's values.
    #[test]
    fn batch_macd_into_overwrites_a_dirty_buffer() {
        let series: Vec<f64> = (0..200)
            .map(|i| (f64::from(i) * 0.21).sin() * 3.0 + 50.0)
            .collect();
        let mut out = vec![9.0; series.len() * 3];
        MacdIndicator::classic().batch_macd_into(&series, &mut out);
        assert_eq!(to_bits(&out), to_bits(&macd_replay(&series)));
    }

    /// A signal period of one seeds the signal on the first MACD value, so the
    /// signal-warmup phase is empty.
    #[test]
    fn batch_macd_with_signal_period_one_matches_replay() {
        let series: Vec<f64> = (0..80).map(|i| f64::from(i % 9) * 1.25 + 30.0).collect();
        let mut fused = MacdIndicator::new(3, 7, 1).unwrap();
        let mut replay = MacdIndicator::new(3, 7, 1).unwrap();
        let want: Vec<f64> = series
            .iter()
            .flat_map(|&x| match replay.update(x) {
                Some(o) => [o.macd, o.signal, o.histogram],
                None => [f64::NAN; 3],
            })
            .collect();
        assert_eq!(to_bits(&fused.batch_macd(&series)), to_bits(&want));
    }

    /// Values beyond `1e300` could overflow a MACD difference, which the
    /// streaming signal EMA would skip; the fused path must hand those to the
    /// exact replay instead.
    #[test]
    fn batch_macd_hands_huge_values_to_the_replay() {
        let mut series: Vec<f64> = (0..60).map(|i| f64::from(i) + 100.0).collect();
        series[45] = 1.7e308;
        series[46] = -1.7e308;
        let got = MacdIndicator::classic().batch_macd(&series);
        assert!(bits_eq(&got, &macd_replay(&series)));
    }

    #[test]
    #[should_panic(expected = "batch_macd output must hold three values per input")]
    fn batch_macd_into_rejects_a_short_buffer() {
        let mut out = vec![0.0; 5];
        MacdIndicator::classic().batch_macd_into(&[1.0, 2.0], &mut out);
    }

    #[test]
    fn batch_macd_falls_back_on_non_finite() {
        let mut series: Vec<f64> = (0..60).map(|i| f64::from(i) + 100.0).collect();
        series[40] = f64::NAN;
        let mut macd = MacdIndicator::classic();
        assert!(bits_eq(&macd.batch_macd(&series), &macd_replay(&series)));
    }

    #[test]
    fn batch_macd_falls_back_when_not_fresh() {
        let series: Vec<f64> = (0..60).map(|i| f64::from(i) + 100.0).collect();
        let mut macd = MacdIndicator::classic();
        macd.update(50.0);
        let mut ref_macd = MacdIndicator::classic();
        ref_macd.update(50.0);
        let mut want = Vec::new();
        for &x in &series {
            match ref_macd.update(x) {
                Some(o) => want.extend_from_slice(&[o.macd, o.signal, o.histogram]),
                None => want.extend_from_slice(&[f64::NAN; 3]),
            }
        }
        assert!(bits_eq(&macd.batch_macd(&series), &want));
    }

    #[test]
    fn batch_macd_too_short_for_output_falls_back() {
        // n < slow + signal - 1 (= 34): no full output, routed to the replay.
        let series: Vec<f64> = (0..20).map(|i| f64::from(i) + 100.0).collect();
        let mut macd = MacdIndicator::classic();
        let got = macd.batch_macd(&series);
        assert!(bits_eq(&got, &macd_replay(&series)));
        assert!(got.iter().all(|x| x.is_nan()));
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut macd = MacdIndicator::classic();
        macd.batch(&(1..=80).map(f64::from).collect::<Vec<_>>());
        let before = macd.value();
        assert!(before.is_some());
        // Non-finite inputs return the last value without advancing any EMA.
        assert_eq!(macd.update(f64::NAN), None);
        assert_eq!(macd.update(f64::INFINITY), None);
        assert_eq!(macd.value(), before);
    }
}
