//! MACD with fixed 12/26 periods (MACDFIX).

use crate::error::Result;
use crate::indicators::macd::{MacdIndicator, MacdOutput};
use crate::traits::Indicator;

/// MACD Fix (`MACDFIX`): the classic MACD with the fast and slow EMAs fixed at
/// 12 and 26, leaving only the signal period configurable.
///
/// This is TA-Lib's `MACDFIX`: the 12- and 26-bar EMAs smooth with Gerald
/// Appel's rounded constants `0.15` and `0.075` instead of `2 / 13` and
/// `2 / 27` (both still seeded with their simple means), so it differs slightly
/// from [`MacdIndicator::new(12, 26, signal)`](crate::MacdIndicator). The signal
/// line is an ordinary `signal`-period EMA of the MACD line. The output is the
/// usual [`MacdOutput`] triple `{ macd, signal, histogram }`.
///
/// ```text
/// fast = EMA(close; seed SMA(12), α = 0.15)
/// slow = EMA(close; seed SMA(26), α = 0.075)
/// macd = fast − slow,  signal = EMA(macd, signal),  histogram = macd − signal
/// ```
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, MacdFix};
///
/// let mut indicator = MacdFix::new(9).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct MacdFix {
    inner: MacdIndicator,
}

impl MacdFix {
    /// Construct a MACDFIX with fast = 12, slow = 26 and the given signal period.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`](crate::Error::PeriodZero) if `signal == 0`.
    pub fn new(signal: usize) -> Result<Self> {
        Ok(Self {
            inner: MacdIndicator::fixed_12_26(signal)?,
        })
    }

    /// Configured signal period.
    pub fn signal_period(&self) -> usize {
        self.inner.periods().2
    }

    /// Exact flat batch, `[macd, signal, histogram]` per input row, warmup rows
    /// `NaN` — the fused path of [`MacdIndicator::batch_macd_into`].
    ///
    /// # Panics
    ///
    /// Panics if `out.len() != inputs.len() * 3`.
    pub fn batch_macd_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        self.inner.batch_macd_into(inputs, out);
    }

    /// [`batch_macd_into`](Self::batch_macd_into) into a fresh vector.
    pub fn batch_macd(&mut self, inputs: &[f64]) -> Vec<f64> {
        self.inner.batch_macd(inputs)
    }

    /// The opt-in SIMD batch of [`MacdIndicator::batch_macd_fast_into`].
    ///
    /// # Panics
    ///
    /// Panics if `out.len() != inputs.len() * 3`.
    pub fn batch_macd_fast_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        self.inner.batch_macd_fast_into(inputs, out);
    }

    /// [`batch_macd_fast_into`](Self::batch_macd_fast_into) into a fresh vector.
    pub fn batch_macd_fast(&mut self, inputs: &[f64]) -> Vec<f64> {
        self.inner.batch_macd_fast(inputs)
    }
}

impl Indicator for MacdFix {
    type Input = f64;
    type Output = MacdOutput;

    #[inline]
    fn update(&mut self, value: f64) -> Option<MacdOutput> {
        self.inner.update(value)
    }

    fn reset(&mut self) {
        self.inner.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "MACDFIX"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    #[test]
    fn rejects_zero_signal() {
        assert!(MacdFix::new(0).is_err());
    }

    #[test]
    fn accessors_report_config() {
        let m = MacdFix::new(9).unwrap();
        assert_eq!(m.signal_period(), 9);
        assert_eq!(m.name(), "MACDFIX");
        assert!(!m.is_ready());
        assert_eq!(
            m.warmup_period(),
            MacdIndicator::new(12, 26, 9).unwrap().warmup_period()
        );
    }

    #[test]
    fn uses_the_fixed_smoothing_constants() {
        let prices: Vec<f64> = (0..80)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        // Reference: SMA-seeded EMAs with alpha 0.15 / 0.075.
        let ema = |n: usize, a: f64| -> Vec<Option<f64>> {
            let mut out = vec![None; prices.len()];
            let mut v = prices[..n].iter().sum::<f64>() / n as f64;
            out[n - 1] = Some(v);
            for i in n..prices.len() {
                v = a * prices[i] + (1.0 - a) * v;
                out[i] = Some(v);
            }
            out
        };
        let (fast, slow) = (ema(12, 0.15), ema(26, 0.075));
        let fix: Vec<Option<MacdOutput>> = MacdFix::new(9).unwrap().batch(&prices);
        // The first MACD line value lands with the slow EMA; once the signal
        // has seeded every output carries it.
        for (i, out) in fix.iter().enumerate() {
            if let Some(o) = out {
                let expected = fast[i].unwrap() - slow[i].unwrap();
                assert!((o.macd - expected).abs() < 1e-9, "at {i}");
            }
        }
        assert!(fix.iter().any(Option::is_some));
        // And it is not the period-derived MACD.
        let classic: Vec<Option<MacdOutput>> =
            MacdIndicator::new(12, 26, 9).unwrap().batch(&prices);
        assert_ne!(fix, classic);
    }

    #[test]
    fn flat_batch_matches_streaming() {
        let prices: Vec<f64> = (0..80)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let streamed: Vec<f64> = MacdFix::new(9)
            .unwrap()
            .batch(&prices)
            .into_iter()
            .flat_map(|o| o.map_or([f64::NAN; 3], |o| [o.macd, o.signal, o.histogram]))
            .collect();
        let flat = MacdFix::new(9).unwrap().batch_macd(&prices);
        assert_eq!(flat.len(), streamed.len());
        for (a, b) in flat.iter().zip(&streamed) {
            assert!(a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()));
        }
    }

    #[test]
    fn reset_clears_state() {
        let prices: Vec<f64> = (0..80).map(|i| 100.0 + f64::from(i)).collect();
        let mut m = MacdFix::new(9).unwrap();
        let _ = m.batch(&prices);
        assert!(m.is_ready());
        m.reset();
        assert!(!m.is_ready());
    }

    fn prices(len: i32) -> Vec<f64> {
        (0..len)
            .map(|i| 100.0 + (f64::from(i) * 0.21).sin() * 7.0 + f64::from(i % 11) * 0.3)
            .collect()
    }

    fn streamed_flat(signal: usize, inputs: &[f64]) -> Vec<f64> {
        let mut m = MacdFix::new(signal).unwrap();
        inputs
            .iter()
            .flat_map(|&x| {
                m.update(x)
                    .map_or([f64::NAN; 3], |o| [o.macd, o.signal, o.histogram])
            })
            .collect()
    }

    fn bits(v: &[f64]) -> Vec<u64> {
        v.iter().map(|x| x.to_bits()).collect()
    }

    #[test]
    fn rejects_signal_above_max() {
        let too_big = crate::error::MAX_PERIOD + 1;
        assert!(matches!(
            MacdFix::new(too_big),
            Err(crate::Error::InvalidPeriod { .. })
        ));
        assert!(matches!(MacdFix::new(0), Err(crate::Error::PeriodZero)));
    }

    #[test]
    fn warmup_is_exact() {
        let xs = prices(60);
        let mut m = MacdFix::new(9).unwrap();
        assert_eq!(m.warmup_period(), 34);
        let out = m.batch(&xs);
        assert!(out[..33].iter().all(Option::is_none));
        assert!(out[33..].iter().all(Option::is_some));
    }

    #[test]
    fn hand_computed_signal_two() {
        // 26 bars at 100 seed fast and slow at 100, MACD 0 at idx25. Then 110, 110:
        //   idx26 fast 101.5, slow 100.75 -> macd 0.75; signal(2) seeds on the
        //         mean (0 + 0.75)/2 = 0.375, histogram 0.375
        //   idx27 fast 0.15·110 + 0.85·101.5 = 102.775,
        //         slow 0.075·110 + 0.925·100.75 = 101.44375 -> macd 1.33125
        //         signal 2/3·1.33125 + 1/3·0.375 = 1.0125, histogram 0.31875
        let mut xs = vec![100.0; 26];
        xs.extend_from_slice(&[110.0, 110.0]);
        let out = MacdFix::new(2).unwrap().batch(&xs);
        assert!(out[..26].iter().all(Option::is_none));
        let o26 = out[26].unwrap();
        approx::assert_relative_eq!(o26.macd, 0.75, epsilon = 1e-12);
        approx::assert_relative_eq!(o26.signal, 0.375, epsilon = 1e-12);
        approx::assert_relative_eq!(o26.histogram, 0.375, epsilon = 1e-12);
        let o27 = out[27].unwrap();
        approx::assert_relative_eq!(o27.macd, 1.331_25, epsilon = 1e-12);
        approx::assert_relative_eq!(o27.signal, 1.0125, epsilon = 1e-12);
        approx::assert_relative_eq!(o27.histogram, 0.318_75, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let xs = prices(120);
        let mut streaming = MacdFix::new(9).unwrap();
        let expected: Vec<Option<MacdOutput>> = xs.iter().map(|&x| streaming.update(x)).collect();
        assert_eq!(MacdFix::new(9).unwrap().batch(&xs), expected);
    }

    #[test]
    fn batch_macd_into_is_bit_identical_to_streaming() {
        let xs = prices(200);
        let mut out = vec![-7.0; xs.len() * 3];
        let mut m = MacdFix::new(9).unwrap();
        m.batch_macd_into(&xs, &mut out);
        assert_eq!(bits(&out), bits(&streamed_flat(9, &xs)));
        // The indicator is left where a replay leaves it.
        let mut replay = MacdFix::new(9).unwrap();
        let _ = replay.batch(&xs);
        assert_eq!(m.update(103.0), replay.update(103.0));
        // batch_macd allocates the same result.
        assert_eq!(bits(&MacdFix::new(9).unwrap().batch_macd(&xs)), bits(&out));
    }

    #[test]
    fn batch_macd_into_short_input_falls_back_to_replay() {
        let xs = prices(20);
        let mut out = vec![0.0; xs.len() * 3];
        MacdFix::new(9).unwrap().batch_macd_into(&xs, &mut out);
        assert_eq!(bits(&out), bits(&streamed_flat(9, &xs)));
    }

    #[test]
    fn batch_macd_fast_is_within_tolerance_with_identical_nans() {
        let xs = prices(500);
        let exact = streamed_flat(9, &xs);
        let mut m = MacdFix::new(9).unwrap();
        let fast = m.batch_macd_fast(&xs);
        assert_eq!(fast.len(), exact.len());
        assert!(fast
            .iter()
            .zip(&exact)
            .all(|(f, e)| f.is_nan() == e.is_nan()));
        let close = fast
            .iter()
            .zip(&exact)
            .filter(|(f, _)| !f.is_nan())
            .all(|(f, e)| (f - e).abs() <= 1e-12 * e.abs().max(1.0));
        assert!(
            close,
            "fast batch must stay within 1e-12 relative of the exact batch"
        );
        // Streaming continues from the kernel's final state.
        let mut replay = MacdFix::new(9).unwrap();
        let _ = replay.batch(&xs);
        let (a, b) = (m.update(104.0).unwrap(), replay.update(104.0).unwrap());
        approx::assert_relative_eq!(a.macd, b.macd, max_relative = 1e-12);
        approx::assert_relative_eq!(a.signal, b.signal, max_relative = 1e-12);
    }

    #[test]
    fn batch_macd_fast_into_fills_a_caller_buffer() {
        let xs = prices(300);
        let mut out = vec![5.0; xs.len() * 3];
        MacdFix::new(5).unwrap().batch_macd_fast_into(&xs, &mut out);
        let exact = MacdFix::new(5).unwrap().batch_macd(&xs);
        assert!(out
            .iter()
            .zip(&exact)
            .all(|(f, e)| f.is_nan() == e.is_nan()));
        let close = out
            .iter()
            .zip(&exact)
            .filter(|(f, _)| !f.is_nan())
            .all(|(f, e)| (f - e).abs() <= 1e-12 * e.abs().max(1.0));
        assert!(
            close,
            "fast batch must stay within 1e-12 relative of the exact batch"
        );
    }

    #[test]
    #[should_panic(expected = "three values per input")]
    fn batch_macd_into_rejects_length_mismatch() {
        let xs = prices(40);
        let mut out = vec![0.0; xs.len() * 3 - 1];
        MacdFix::new(9).unwrap().batch_macd_into(&xs, &mut out);
    }

    #[test]
    #[should_panic(expected = "three values per input")]
    fn batch_macd_fast_into_rejects_length_mismatch() {
        let xs = prices(40);
        let mut out = vec![0.0; xs.len()];
        MacdFix::new(9).unwrap().batch_macd_fast_into(&xs, &mut out);
    }

    #[test]
    fn reset_reproduces_a_fresh_run() {
        let xs = prices(90);
        let mut m = MacdFix::new(9).unwrap();
        let first = m.batch(&xs);
        m.reset();
        let second = m.batch(&xs);
        assert_eq!(first, second);
        assert_eq!(second, MacdFix::new(9).unwrap().batch(&xs));
    }
}
