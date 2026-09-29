//! Bollinger Bands.

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::traits::Indicator;

/// Bollinger Bands output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BollingerOutput {
    /// Upper band: `middle + multiplier * stddev`.
    pub upper: f64,
    /// Middle band: SMA over the window.
    pub middle: f64,
    /// Lower band: `middle − multiplier * stddev`.
    pub lower: f64,
    /// Sample standard deviation (denominator `period`, population stddev) used to build
    /// the bands. Reported separately because some callers compute their own bands.
    pub stddev: f64,
}

/// Bollinger Bands with SMA middle band and population standard deviation envelopes.
///
/// Standard parameters are `period = 20`, `multiplier = 2.0`. Bollinger's original
/// publication uses population (not sample) standard deviation, which matches every
/// reference implementation (TA-Lib, pandas-ta, etc.).
///
/// The running `sum` and `sum_sq` are reseeded from the live window every
/// `16 · period` updates to cap floating-point drift on long streams. This is
/// amortised O(1), preserves bit-equivalence with the previous behaviour on
/// inputs that did not drift, and is particularly important for `sum_sq`,
/// where catastrophic cancellation between large add/subtract pairs can drive
/// the computed variance negative (the `.max(0.0)` clamp below is the
/// safety-net for the rare cases where the reseed has not happened yet).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, BollingerBands};
///
/// let mut indicator = BollingerBands::new(5, 2.0).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct BollingerBands {
    period: usize,
    multiplier: f64,
    /// Fixed-capacity ring buffer of the last `period` finite inputs. A flat
    /// `Box<[f64]>` with a manual write cursor beats `VecDeque` on this hot path.
    buf: Box<[f64]>,
    /// Index of the next slot to write — also the oldest element once full.
    head: usize,
    /// Number of slots filled, saturating at `period`.
    count: usize,
    /// Rolling first and second moments, accumulated around a reference point
    /// inside the window. See `ShiftedMoments` for why the textbook
    /// `E[x²] − E[x]²` form is not usable on raw price levels.
    moments: ShiftedMoments,
}

impl BollingerBands {
    /// Construct a new Bollinger Bands indicator.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] for `period == 0` and
    /// [`Error::NonPositiveMultiplier`] for `multiplier <= 0`.
    pub fn new(period: usize, multiplier: f64) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !multiplier.is_finite() || multiplier <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        Ok(Self {
            period,
            multiplier,
            buf: vec![0.0; period].into_boxed_slice(),
            head: 0,
            count: 0,
            moments: ShiftedMoments::new(),
        })
    }

    /// Classic configuration: `period = 20`, `multiplier = 2.0`.
    pub fn classic() -> Self {
        Self::new(20, 2.0).expect("classic Bollinger parameters are valid")
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured multiplier.
    pub const fn multiplier(&self) -> f64 {
        self.multiplier
    }

    /// Vectorized flat batch for bindings: returns `n * 4` values laid out as
    /// `[upper, middle, lower, stddev]` per input row, warmup rows all `NaN`.
    ///
    /// Allocates the result and fills it through
    /// [`batch_bands_into`](Self::batch_bands_into). This is a *separate* entry
    /// point from the trait [`batch`](crate::BatchExt::batch), which returns
    /// `Vec<Option<BollingerOutput>>`.
    pub fn batch_bands(&mut self, inputs: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; inputs.len() * 4];
        self.batch_bands_into(inputs, &mut out);
        out
    }

    /// [`batch_bands`](Self::batch_bands) into a caller-owned buffer of
    /// `inputs.len() * 4` values, overwriting every cell.
    ///
    /// For a fresh, all-finite slice it inlines `update`'s rolling moments and
    /// drift-reseed on local copies of the ring cursor and the accumulator —
    /// the state is written back once at the end rather than through `self` on
    /// every element — and writes the four band values straight into the row.
    /// Same add/subtract order, same reseed cadence, same variance/`sqrt` math,
    /// so it is *bit-for-bit* equal to replaying `update`, including the
    /// long-stream drift bound. Any other state, or a non-finite element,
    /// defers to the exact `update` replay.
    ///
    /// # Panics
    ///
    /// Panics if `out.len() != inputs.len() * 4`.
    pub fn batch_bands_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        assert_eq!(
            out.len(),
            inputs.len() * 4,
            "batch_bands output must hold four values per input"
        );
        // `count == 0` is the only pristine state: the reseed counter can only
        // be non-zero once a value has been pushed, so it adds nothing here.
        if self.count != 0 || !inputs.iter().all(|x| x.is_finite()) {
            for (row, &x) in out.chunks_exact_mut(4).zip(inputs) {
                match self.update(x) {
                    Some(o) => row.copy_from_slice(&[o.upper, o.middle, o.lower, o.stddev]),
                    None => row.fill(f64::NAN),
                }
            }
            return;
        }

        let p = self.period;
        let mult = self.multiplier;
        let mut moments = self.moments.clone();
        let (mut head, mut count) = (self.head, self.count);
        let buf = &mut self.buf[..];
        for (row, &x) in out.chunks_exact_mut(4).zip(inputs) {
            if count == p {
                moments.evict(buf[head]);
                buf[head] = x;
                moments.push(x);
            } else {
                buf[head] = x;
                moments.push(x);
                count += 1;
            }
            head += 1;
            if head == p {
                head = 0;
            }
            if moments.needs_reseed(p) {
                let (older, newer) = if count == p {
                    (&buf[head..], &buf[..head])
                } else {
                    (&buf[..count], &buf[..0])
                };
                moments.reseed(older.iter().chain(newer).copied());
            }
            if count == p {
                let mean = moments.mean(p);
                let stddev = moments.std_dev(p);
                let band = mult * stddev;
                row.copy_from_slice(&[mean + band, mean, mean - band, stddev]);
            } else {
                row.fill(f64::NAN);
            }
        }
        self.moments = moments;
        self.head = head;
        self.count = count;
    }

    fn current(&self) -> Option<BollingerOutput> {
        if self.count != self.period {
            return None;
        }
        let mean = self.moments.mean(self.period);
        let stddev = self.moments.std_dev(self.period);
        Some(BollingerOutput {
            upper: mean + self.multiplier * stddev,
            middle: mean,
            lower: mean - self.multiplier * stddev,
            stddev,
        })
    }
}

impl Indicator for BollingerBands {
    type Input = f64;
    type Output = BollingerOutput;

    #[inline]
    fn update(&mut self, input: f64) -> Option<BollingerOutput> {
        if !input.is_finite() {
            return None;
        }
        if self.count == self.period {
            self.moments.evict(self.buf[self.head]);
            self.buf[self.head] = input;
            self.moments.push(input);
        } else {
            self.buf[self.head] = input;
            self.moments.push(input);
            self.count += 1;
        }
        self.head += 1;
        if self.head == self.period {
            self.head = 0;
        }
        if self.moments.needs_reseed(self.period) {
            // Reseed in chronological order (oldest at `head`) so the accumulator
            // matches a fresh from-scratch pass and the reference point is
            // re-anchored on the live window.
            let (older, newer) = if self.count == self.period {
                (&self.buf[self.head..], &self.buf[..self.head])
            } else {
                (&self.buf[..self.count], &self.buf[..0])
            };
            self.moments.reseed(older.iter().chain(newer).copied());
        }
        self.current()
    }

    fn reset(&mut self) {
        self.head = 0;
        self.count = 0;
        self.moments.reset();
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
        "BollingerBands"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;
    use std::collections::VecDeque;

    fn naive(prices: &[f64], period: usize, mult: f64) -> BollingerOutput {
        assert!(
            prices.len() >= period,
            "naive requires at least `period` prices"
        );
        let w = &prices[prices.len() - period..];
        let mean = w.iter().sum::<f64>() / period as f64;
        let var = w.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / period as f64;
        let s = var.sqrt();
        BollingerOutput {
            upper: mean + mult * s,
            middle: mean,
            lower: mean - mult * s,
            stddev: s,
        }
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            BollingerBands::new(0, 2.0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn rejects_non_positive_multiplier() {
        assert!(matches!(
            BollingerBands::new(20, 0.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            BollingerBands::new(20, -1.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            BollingerBands::new(20, f64::NAN),
            Err(Error::NonPositiveMultiplier)
        ));
    }

    /// Cover the convenience constructor `BollingerBands::classic()` plus the
    /// const accessors `period` / `multiplier` and the Indicator-impl
    /// metadata methods `warmup_period` / `name`. Existing tests never
    /// invoked `classic()` (every test passed explicit parameters to
    /// `new`) and never queried any of the four getters.
    #[test]
    fn classic_and_accessors_and_metadata() {
        let bb = BollingerBands::classic();
        assert_eq!(bb.period(), 20);
        assert_relative_eq!(bb.multiplier(), 2.0, epsilon = 1e-12);
        assert_eq!(bb.warmup_period(), 20);
        assert_eq!(bb.name(), "BollingerBands");
    }

    #[test]
    fn warmup_returns_none() {
        let mut bb = BollingerBands::new(5, 2.0).unwrap();
        for v in [1.0, 2.0, 3.0, 4.0] {
            assert!(bb.update(v).is_none());
        }
        assert!(bb.update(5.0).is_some());
    }

    /// The band width is a standard deviation, so it inherits the accumulator's
    /// numerics. With the textbook `E[x²] − E[x]²` form this drifted by 4.3e-06
    /// at a price level of 1e5 and collapsed to exactly zero at 1e8 — bands of
    /// zero width, and a permanent squeeze reading downstream.
    #[test]
    fn bands_stay_accurate_when_the_level_dwarfs_the_spread() {
        for level in [1.0e2_f64, 1.0e5, 1.0e8] {
            let prices: Vec<f64> = (0..60)
                .map(|i| level + (f64::from(i) * 0.7).sin())
                .collect();
            let mut bb = BollingerBands::new(20, 2.0).unwrap();
            let mut got = 0.0;
            for price in &prices {
                if let Some(o) = bb.update(*price) {
                    got = o.stddev;
                }
            }
            let window = &prices[40..];
            let n = window.len() as f64;
            let mean = window.iter().sum::<f64>() / n;
            let want = (window.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n).sqrt();
            assert_relative_eq!(got, want, max_relative = 1e-9);
        }
    }

    #[test]
    fn constant_series_yields_zero_stddev() {
        let mut bb = BollingerBands::new(10, 2.0).unwrap();
        let out = bb.batch(&[5.0_f64; 30]);
        let last = out.iter().rev().flatten().next().unwrap();
        assert_relative_eq!(last.middle, 5.0, epsilon = 1e-12);
        assert_relative_eq!(last.stddev, 0.0, epsilon = 1e-12);
        assert_relative_eq!(last.upper, 5.0, epsilon = 1e-12);
        assert_relative_eq!(last.lower, 5.0, epsilon = 1e-12);
    }

    #[test]
    fn matches_naive_definition() {
        let prices: Vec<f64> = (1..=60)
            .map(|i| (f64::from(i) * 0.3).sin() * 10.0 + 50.0)
            .collect();
        let mut bb = BollingerBands::new(20, 2.0).unwrap();
        let out = bb.batch(&prices);
        for i in 19..prices.len() {
            let got = out[i].unwrap();
            let want = naive(&prices[..=i], 20, 2.0);
            assert_relative_eq!(got.middle, want.middle, epsilon = 1e-9);
            assert_relative_eq!(got.stddev, want.stddev, epsilon = 1e-9);
            assert_relative_eq!(got.upper, want.upper, epsilon = 1e-9);
            assert_relative_eq!(got.lower, want.lower, epsilon = 1e-9);
        }
    }

    #[test]
    fn upper_above_middle_above_lower() {
        let prices: Vec<f64> = (1..=100).map(f64::from).collect();
        let mut bb = BollingerBands::new(20, 2.0).unwrap();
        for o in bb.batch(&prices).into_iter().flatten() {
            assert!(o.upper >= o.middle);
            assert!(o.middle >= o.lower);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=50).map(|i| f64::from(i) * 0.7).collect();
        let mut a = BollingerBands::new(10, 2.0).unwrap();
        let mut b = BollingerBands::new(10, 2.0).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut bb = BollingerBands::new(5, 2.0).unwrap();
        bb.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(bb.is_ready());
        bb.reset();
        assert!(!bb.is_ready());
    }

    /// Long-running stability check. After several recompute cycles the
    /// reported Bollinger bands must still equal a fresh from-scratch
    /// computation over the live window — even on inputs designed to cause
    /// catastrophic cancellation in the `sum_sq` accumulator (alternating
    /// between two very different magnitudes).
    #[test]
    fn long_stream_drift_stays_bounded() {
        let period = 20;
        let mult = 2.0;
        let mut bb = BollingerBands::new(period, mult).unwrap();
        let mut window: VecDeque<f64> = VecDeque::with_capacity(period);
        // Forces the periodic reseed to fire 5+ times.
        let n_updates = 16 * period * 5;
        let mut last = None;
        for i in 0..n_updates {
            let v = if i % 2 == 0 { 1e6 } else { 1.0 };
            last = bb.update(v);
            if window.len() == period {
                window.pop_front();
            }
            window.push_back(v);
        }
        let scratch = naive(&window.iter().copied().collect::<Vec<_>>(), period, mult);
        let got = last.expect("warmed up");
        assert!(
            (got.middle - scratch.middle).abs() < 1e-3,
            "middle drift: got={}, scratch={}",
            got.middle,
            scratch.middle,
        );
        assert!(
            (got.stddev - scratch.stddev).abs() < 1e-3,
            "stddev drift: got={}, scratch={}",
            got.stddev,
            scratch.stddev,
        );
    }

    fn bits_eq(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| x == y || (x.is_nan() && y.is_nan()))
    }

    /// Flat `n*4` `[upper, middle, lower, stddev]` replay of `update`.
    fn bb_replay(period: usize, mult: f64, series: &[f64]) -> Vec<f64> {
        let mut bb = BollingerBands::new(period, mult).unwrap();
        let mut out = Vec::with_capacity(series.len() * 4);
        for &x in series {
            match bb.update(x) {
                Some(o) => out.extend_from_slice(&[o.upper, o.middle, o.lower, o.stddev]),
                None => out.extend_from_slice(&[f64::NAN; 4]),
            }
        }
        out
    }

    /// Into a buffer that already holds values, every cell is overwritten:
    /// warmup rows with `NaN`, the rest with the replay's values, bit for bit.
    #[test]
    fn batch_bands_into_overwrites_a_dirty_buffer() {
        let series: Vec<f64> = (0..400)
            .map(|i| (f64::from(i) * 0.17).sin() * 6.0 + f64::from(i % 5) + 90.0)
            .collect();
        let mut out = vec![5.5; series.len() * 4];
        BollingerBands::new(20, 2.0)
            .unwrap()
            .batch_bands_into(&series, &mut out);
        let want = bb_replay(20, 2.0, &series);
        let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&out), bits(&want));
    }

    #[test]
    #[should_panic(expected = "batch_bands output must hold four values per input")]
    fn batch_bands_into_rejects_a_short_buffer() {
        let mut out = vec![0.0; 7];
        BollingerBands::new(3, 2.0)
            .unwrap()
            .batch_bands_into(&[1.0, 2.0], &mut out);
    }

    #[test]
    fn batch_bands_fast_path_is_bit_identical_with_reseed() {
        // > 16*period inputs so the drift-reseed branch fires inside batch_bands.
        let series: Vec<f64> = (0..500)
            .map(|i| (f64::from(i) * 0.2).sin() * 10.0 + 50.0)
            .collect();
        let mut bb = BollingerBands::new(20, 2.0).unwrap();
        let got = bb.batch_bands(&series);
        assert!(bits_eq(&got, &bb_replay(20, 2.0, &series)));
        // State continues identically.
        let mut ref_bb = BollingerBands::new(20, 2.0).unwrap();
        for &x in &series {
            ref_bb.update(x);
        }
        assert_eq!(bb.update(55.0), ref_bb.update(55.0));
    }

    #[test]
    fn batch_bands_falls_back_on_non_finite() {
        let series = [1.0, 2.0, 3.0, f64::NAN, 5.0, 6.0, 7.0];
        let mut bb = BollingerBands::new(3, 2.0).unwrap();
        assert!(bits_eq(
            &bb.batch_bands(&series),
            &bb_replay(3, 2.0, &series)
        ));
    }

    #[test]
    fn batch_bands_falls_back_when_not_fresh() {
        let mut bb = BollingerBands::new(3, 2.0).unwrap();
        bb.update(99.0);
        let series = [1.0, 2.0, 3.0, 4.0];
        let mut ref_bb = BollingerBands::new(3, 2.0).unwrap();
        ref_bb.update(99.0);
        let mut want = Vec::new();
        for &x in &series {
            match ref_bb.update(x) {
                Some(o) => want.extend_from_slice(&[o.upper, o.middle, o.lower, o.stddev]),
                None => want.extend_from_slice(&[f64::NAN; 4]),
            }
        }
        assert!(bits_eq(&bb.batch_bands(&series), &want));
    }

    #[test]
    fn batch_bands_sub_period_slice_is_all_nan() {
        let series = [1.0, 2.0, 3.0];
        let mut bb = BollingerBands::new(10, 2.0).unwrap();
        let got = bb.batch_bands(&series);
        assert!(bits_eq(&got, &bb_replay(10, 2.0, &series)));
        assert!(got.iter().all(|x| x.is_nan()) && got.len() == 12);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut bb = BollingerBands::new(5, 2.0).unwrap();
        bb.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        // A non-finite input has no value and does not mutate the window.
        assert_eq!(bb.update(f64::NAN), None);
        assert_eq!(bb.update(f64::INFINITY), None);
        // The window still holds 1..=5, so a real input slides it to 2..=6.
        let after = bb.update(6.0).unwrap();
        assert_relative_eq!(
            after.middle,
            (2.0 + 3.0 + 4.0 + 5.0 + 6.0) / 5.0,
            epsilon = 1e-12
        );
    }
}
