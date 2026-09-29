//! Relative Strength Index using Wilder's smoothing.

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Relative Strength Index (Wilder, 1978).
///
/// Uses Wilder's smoothing (an EMA with `alpha = 1 / period`). The first output
/// is produced after `period + 1` inputs: the seed averages the first `period`
/// gains and losses, and the first emitted RSI corresponds to the input at
/// index `period`.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Rsi};
///
/// let mut indicator = Rsi::new(3).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Rsi {
    period: usize,
    /// `period - 1` as `f64`, precomputed for the Wilder smoothing step.
    n_minus_1: f64,
    /// `1 / period`, precomputed so the per-tick smoothing multiplies instead of
    /// divides (a reciprocal is hoisted out of the hot path).
    inv_period: f64,
    /// Previous close, valid once `has_prev` is set. Bare `f64` + flag instead of
    /// `Option<f64>` to avoid an enum-tag read on every tick.
    prev_close: f64,
    has_prev: bool,
    // Wilder seeds with the simple average of the first `period` gains/losses,
    // then transitions to recursive smoothing.
    seed_buf_gains: Vec<f64>,
    seed_buf_losses: Vec<f64>,
    /// Smoothed average gain / loss, valid once `avgs_seeded` is set. Bare `f64`s
    /// + flag so the hot recurrence avoids reading two `Option<f64>` tags per tick.
    avg_gain: f64,
    avg_loss: f64,
    avgs_seeded: bool,
    last_value: Option<f64>,
}

impl Rsi {
    /// Construct an RSI with the given Wilder period.
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
            prev_close: 0.0,
            has_prev: false,
            seed_buf_gains: Vec::with_capacity(period),
            seed_buf_losses: Vec::with_capacity(period),
            avg_gain: 0.0,
            avg_loss: 0.0,
            avgs_seeded: false,
            last_value: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }

    /// Vectorized batch returning one `f64` per input (`NaN` during warmup).
    ///
    /// Kept as an inherent method so existing callers need no trait import; it
    /// allocates the result and fills it through
    /// [`batch_nan_into`](Indicator::batch_nan_into), which carries the fast path.
    pub fn batch_nan(&mut self, inputs: &[f64]) -> Vec<f64> {
        crate::traits::BatchNanExt::batch_nan(self, inputs)
    }

    fn rsi_from_avgs(avg_gain: f64, avg_loss: f64) -> f64 {
        // Algebraically `100 - 100/(1 + ag/al)` collapses to `100·ag/(ag+al)`,
        // which needs a single division instead of two and removes the separate
        // `rs` step. Edge cases stay exact: `al == 0, ag > 0` gives `100·ag/ag =
        // 100`; `ag == 0, al > 0` gives `0`; both zero (no movement) is the
        // undefined case and returns the neutral 50.
        let denom = avg_gain + avg_loss;
        if denom == 0.0 {
            50.0
        } else {
            100.0 * avg_gain / denom
        }
    }
}

impl Indicator for Rsi {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }

        if !self.has_prev {
            self.prev_close = input;
            self.has_prev = true;
            return None;
        }
        let prev = self.prev_close;
        self.prev_close = input;

        let diff = input - prev;
        let gain = if diff > 0.0 { diff } else { 0.0 };
        let loss = if diff < 0.0 { -diff } else { 0.0 };

        if self.avgs_seeded {
            // Wilder smoothing `(prev·(n-1) + x) / n` with the reciprocal hoisted:
            // a fused multiply-add then a multiply by `1/n`, no per-tick division.
            let new_ag = self.avg_gain.mul_add(self.n_minus_1, gain) * self.inv_period;
            let new_al = self.avg_loss.mul_add(self.n_minus_1, loss) * self.inv_period;
            self.avg_gain = new_ag;
            self.avg_loss = new_al;
            let v = Self::rsi_from_avgs(new_ag, new_al);
            self.last_value = Some(v);
            return Some(v);
        }

        self.seed_buf_gains.push(gain);
        self.seed_buf_losses.push(loss);
        if self.seed_buf_gains.len() == self.period {
            let ag = self.seed_buf_gains.iter().sum::<f64>() / self.period as f64;
            let al = self.seed_buf_losses.iter().sum::<f64>() / self.period as f64;
            self.avg_gain = ag;
            self.avg_loss = al;
            self.avgs_seeded = true;
            let v = Self::rsi_from_avgs(ag, al);
            self.last_value = Some(v);
            return Some(v);
        }
        None
    }

    fn reset(&mut self) {
        self.prev_close = 0.0;
        self.has_prev = false;
        self.seed_buf_gains.clear();
        self.seed_buf_losses.clear();
        self.avg_gain = 0.0;
        self.avg_loss = 0.0;
        self.avgs_seeded = false;
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RSI"
    }

    /// RSI is a recursive (IIR) filter — Wilder smoothing — so its exact form
    /// cannot run in SIMD lanes; the win is in stripping per-tick overhead. For
    /// a fresh indicator over an all-finite slice long enough to seed
    /// (`n > period`) it runs the seed once and then the bare smoothing
    /// recurrence in a tight loop with no per-tick `is_finite`/`has_prev`/
    /// `avgs_seeded` branch and no `Option`, using the identical division at the
    /// seed and `mul_add`/`rsi_from_avgs` afterwards — so every value is
    /// *bit-for-bit* equal to replaying `update`. Shorter or non-fresh/non-finite
    /// inputs defer to the exact `update` replay.
    fn batch_nan_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        assert_eq!(
            inputs.len(),
            out.len(),
            "batch output length must equal input length"
        );
        let p = self.period;
        let n = inputs.len();
        if self.has_prev
            || self.avgs_seeded
            || !self.seed_buf_gains.is_empty()
            || n <= p
            || !inputs.iter().all(|x| x.is_finite())
        {
            for (slot, &x) in out.iter_mut().zip(inputs) {
                *slot = self.update(x).unwrap_or(f64::NAN);
            }
            return;
        }

        // Warmup `[0, p)` is `NaN`; the seed lands on `p`, the recurrence after.
        out[..p].fill(f64::NAN);
        // Seed from the first `period` diffs (inputs[1..=p]); index 0 only sets the
        // baseline. Retain the seed gains/losses exactly as `update` leaves them.
        let mut prev = inputs[0];
        let (mut sum_gain, mut sum_loss) = (0.0_f64, 0.0_f64);
        for &x in &inputs[1..=p] {
            let diff = x - prev;
            prev = x;
            let gain = if diff > 0.0 { diff } else { 0.0 };
            let loss = if diff < 0.0 { -diff } else { 0.0 };
            self.seed_buf_gains.push(gain);
            self.seed_buf_losses.push(loss);
            sum_gain += gain;
            sum_loss += loss;
        }
        let p_f64 = p as f64;
        let mut ag = sum_gain / p_f64;
        let mut al = sum_loss / p_f64;
        out[p] = Self::rsi_from_avgs(ag, al);

        // Steady state: Wilder smoothing, reciprocal hoisted, one `rsi_from_avgs`.
        for (slot, &x) in out[p + 1..].iter_mut().zip(&inputs[p + 1..]) {
            let diff = x - prev;
            prev = x;
            let gain = if diff > 0.0 { diff } else { 0.0 };
            let loss = if diff < 0.0 { -diff } else { 0.0 };
            ag = ag.mul_add(self.n_minus_1, gain) * self.inv_period;
            al = al.mul_add(self.n_minus_1, loss) * self.inv_period;
            *slot = Self::rsi_from_avgs(ag, al);
        }

        // Leave state where a full `update` replay would.
        self.prev_close = prev;
        self.has_prev = true;
        self.avg_gain = ag;
        self.avg_loss = al;
        self.avgs_seeded = true;
        self.last_value = Some(out[n - 1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Independent reference: Wilder RSI computed straight from the definition.
    fn rsi_naive(prices: &[f64], period: usize) -> Vec<Option<f64>> {
        let n = period as f64;
        let mut out = vec![None; prices.len()];
        let mut gains: Vec<f64> = Vec::new();
        let mut losses: Vec<f64> = Vec::new();
        let mut avg_gain: Option<f64> = None;
        let mut avg_loss: Option<f64> = None;
        let rsi_val = |ag: f64, al: f64| -> f64 {
            if al == 0.0 {
                if ag == 0.0 {
                    50.0
                } else {
                    100.0
                }
            } else {
                100.0 - 100.0 / (1.0 + ag / al)
            }
        };
        for i in 1..prices.len() {
            let diff = prices[i] - prices[i - 1];
            let gain = if diff > 0.0 { diff } else { 0.0 };
            let loss = if diff < 0.0 { -diff } else { 0.0 };
            if let (Some(ag), Some(al)) = (avg_gain, avg_loss) {
                let nag = (ag * (n - 1.0) + gain) / n;
                let nal = (al * (n - 1.0) + loss) / n;
                avg_gain = Some(nag);
                avg_loss = Some(nal);
                out[i] = Some(rsi_val(nag, nal));
            } else {
                gains.push(gain);
                losses.push(loss);
                if gains.len() == period {
                    let ag = gains.iter().sum::<f64>() / n;
                    let al = losses.iter().sum::<f64>() / n;
                    avg_gain = Some(ag);
                    avg_loss = Some(al);
                    out[i] = Some(rsi_val(ag, al));
                }
            }
        }
        out
    }

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(Rsi::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `value` (60-67) and the
    /// Indicator-impl `name` body (145-147). `warmup_period` is covered
    /// already by `warmup_period_is_period_plus_one`.
    #[test]
    fn accessors_and_metadata() {
        let mut rsi = Rsi::new(14).unwrap();
        assert_eq!(rsi.period(), 14);
        assert_eq!(rsi.name(), "RSI");
        assert_eq!(rsi.value(), None);
        for i in 1..=15 {
            rsi.update(100.0 + f64::from(i));
        }
        assert!(rsi.value().is_some());
    }

    /// Cover the `ag == 0` branch (line 167) of the test-helper `rsi_naive`:
    /// when both `avg_gain` and `avg_loss` are 0 (a perfectly flat series),
    /// the helper must return the neutral 50.0. The proptest reference uses
    /// random inputs that essentially never hit zero gains AND zero losses
    /// simultaneously, leaving this branch dead in the helper.
    #[test]
    fn naive_helper_flat_series_yields_50() {
        let ks = rsi_naive(&[42.0; 20], 5);
        for r in ks.into_iter().skip(5) {
            assert_eq!(r.expect("ready after period+1 inputs"), 50.0);
        }
    }

    /// Cover the `100.0` branch (line 169) of the test-helper `rsi_naive`:
    /// strictly increasing prices give `avg_loss == 0` while `avg_gain > 0`,
    /// the textbook overbought saturation case. Random proptest inputs
    /// virtually never satisfy `al == 0 && ag != 0`, so this needs an
    /// explicit monotone series.
    #[test]
    fn naive_helper_monotone_up_yields_100() {
        let prices: Vec<f64> = (1..=20).map(f64::from).collect();
        let ks = rsi_naive(&prices, 5);
        for r in ks.into_iter().skip(5) {
            assert_eq!(r.expect("ready after period+1 inputs"), 100.0);
        }
    }

    #[test]
    fn warmup_period_is_period_plus_one() {
        let rsi = Rsi::new(14).unwrap();
        assert_eq!(rsi.warmup_period(), 15);
    }

    #[test]
    fn first_emission_at_index_period() {
        // RSI(14) needs 14 diffs => 15 inputs before first value.
        let prices: Vec<f64> = (1..=20).map(f64::from).collect();
        let mut rsi = Rsi::new(14).unwrap();
        let out = rsi.batch(&prices);
        // indices 0..14 -> None, index 14 -> first Some
        for x in &out[..14] {
            assert!(x.is_none());
        }
        assert!(out[14].is_some());
    }

    #[test]
    fn pure_uptrend_yields_rsi_100() {
        let prices: Vec<f64> = (1..=20).map(f64::from).collect();
        let mut rsi = Rsi::new(14).unwrap();
        let out = rsi.batch(&prices);
        // All diffs are positive => avg_loss == 0 => RSI == 100
        for v in out.iter().filter_map(|x| x.as_ref()) {
            assert_relative_eq!(*v, 100.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn pure_downtrend_yields_rsi_0() {
        let prices: Vec<f64> = (1..=20).rev().map(f64::from).collect();
        let mut rsi = Rsi::new(14).unwrap();
        let out = rsi.batch(&prices);
        for v in out.iter().filter_map(|x| x.as_ref()) {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn flat_series_yields_rsi_50() {
        let prices = [10.0_f64; 30];
        let mut rsi = Rsi::new(14).unwrap();
        let out = rsi.batch(&prices);
        for v in out.iter().filter_map(|x| x.as_ref()) {
            assert_relative_eq!(*v, 50.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn classic_wilder_textbook_values() {
        // Wilder's original example from "New Concepts in Technical Trading Systems",
        // 14-period RSI. We compute the first value at index 14 and compare to the
        // value Wilder publishes (~70.46).
        // Source: classic textbook table, reproduced in many references (e.g. Investopedia).
        let prices = [
            44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03,
            45.61, 46.28, 46.28,
        ];
        let mut rsi = Rsi::new(14).unwrap();
        let out = rsi.batch(&prices);
        let first = out[14].expect("first RSI emitted at index period");
        assert_relative_eq!(first, 70.464, epsilon = 0.05);
    }

    #[test]
    fn rsi_stays_in_0_100_range() {
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.7).sin() * 10.0)
            .collect();
        let mut rsi = Rsi::new(14).unwrap();
        for x in rsi.batch(&prices).into_iter().flatten() {
            assert!((0.0..=100.0).contains(&x), "RSI out of range: {x}");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut rsi = Rsi::new(5).unwrap();
        rsi.batch(&[1.0, 2.0, 3.0, 2.0, 4.0, 5.0, 6.0]);
        assert!(rsi.is_ready());
        rsi.reset();
        assert!(!rsi.is_ready());
        assert_eq!(rsi.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=40)
            .map(|i| (f64::from(i) * 0.3).sin() * 5.0 + f64::from(i))
            .collect();
        let mut a = Rsi::new(7).unwrap();
        let mut b = Rsi::new(7).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut rsi = Rsi::new(3).unwrap();
        rsi.batch(&[1.0, 2.0, 3.0, 4.0]);
        let before = rsi.value();
        assert!(before.is_some());
        assert_eq!(rsi.update(f64::NAN), None);
        assert_eq!(rsi.update(f64::INFINITY), None);
        assert_eq!(rsi.value(), before);
    }

    fn bits_eq(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| x == y || (x.is_nan() && y.is_nan()))
    }

    fn rsi_replay(period: usize, series: &[f64]) -> Vec<f64> {
        let mut r = Rsi::new(period).unwrap();
        series
            .iter()
            .map(|&x| r.update(x).unwrap_or(f64::NAN))
            .collect()
    }

    #[test]
    fn batch_nan_fast_path_is_bit_identical() {
        let series: Vec<f64> = (0..300)
            .map(|i| (f64::from(i) * 0.3).sin() * 5.0 + f64::from(i) * 0.1 + 100.0)
            .collect();
        let mut rsi = Rsi::new(14).unwrap();
        let got = rsi.batch_nan(&series);
        assert!(bits_eq(&got, &rsi_replay(14, &series)));
        let mut ref_rsi = Rsi::new(14).unwrap();
        for &x in &series {
            ref_rsi.update(x);
        }
        assert_eq!(rsi.update(123.0), ref_rsi.update(123.0));
    }

    /// Into a caller buffer that already holds values, the fast path must write
    /// every cell — the warmup `NaN`s included — exactly as the replay would.
    #[test]
    fn batch_nan_into_overwrites_a_dirty_buffer() {
        let series: Vec<f64> = (0..200)
            .map(|i| (f64::from(i) * 0.3).sin() * 5.0 + 60.0)
            .collect();
        let mut out = vec![7.0; series.len()];
        Rsi::new(14).unwrap().batch_nan_into(&series, &mut out);
        assert!(bits_eq(&out, &rsi_replay(14, &series)));
    }

    #[test]
    fn batch_nan_falls_back_on_non_finite() {
        let series = [10.0, 11.0, 9.0, f64::NAN, 12.0, 13.0, 8.0];
        let mut rsi = Rsi::new(3).unwrap();
        assert!(bits_eq(&rsi.batch_nan(&series), &rsi_replay(3, &series)));
    }

    #[test]
    fn batch_nan_falls_back_when_not_fresh() {
        let mut rsi = Rsi::new(3).unwrap();
        rsi.update(50.0);
        let series = [51.0, 49.0, 52.0, 53.0, 50.0];
        let mut ref_rsi = Rsi::new(3).unwrap();
        ref_rsi.update(50.0);
        let want: Vec<f64> = series
            .iter()
            .map(|&x| ref_rsi.update(x).unwrap_or(f64::NAN))
            .collect();
        assert!(bits_eq(&rsi.batch_nan(&series), &want));
    }

    #[test]
    fn batch_nan_too_short_to_seed_falls_back() {
        // n <= period: routed to the exact replay (cannot seed yet).
        let series = [10.0, 11.0, 12.0];
        let mut rsi = Rsi::new(3).unwrap();
        assert!(bits_eq(&rsi.batch_nan(&series), &rsi_replay(3, &series)));
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(48))]
        #[test]
        fn rsi_matches_naive(
            period in 1usize..20,
            prices in proptest::collection::vec(1.0_f64..1000.0, 0..150),
        ) {
            let mut rsi = Rsi::new(period).unwrap();
            let got = rsi.batch(&prices);
            let want = rsi_naive(&prices, period);
            proptest::prop_assert_eq!(got.len(), want.len());
            for (g, w) in got.iter().zip(want.iter()) {
                match (g, w) {
                    (None, None) => {}
                    (Some(a), Some(b)) => proptest::prop_assert!(
                        (a - b).abs() < 1e-7,
                        "got={a} want={b}"
                    ),
                    _ => proptest::prop_assert!(false, "warmup mismatch"),
                }
            }
        }
    }
}
