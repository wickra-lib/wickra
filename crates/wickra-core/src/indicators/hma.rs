//! Hull Moving Average (HMA).

use crate::error::{Error, Result};
use crate::indicators::wma::Wma;
use crate::traits::Indicator;

/// Hull Moving Average: `WMA(2 * WMA(n/2) - WMA(n), sqrt(n))`.
///
/// Designed by Alan Hull as a lag-free moving average that is also responsive.
/// The square root of the period is rounded to the nearest integer (minimum 1).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Hma};
///
/// let mut indicator = Hma::new(9).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Hma {
    period: usize,
    half_wma: Wma,
    full_wma: Wma,
    smooth_wma: Wma,
}

impl Hma {
    /// # Errors
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
        let half = (period / 2).max(1);
        let smooth = (period as f64).sqrt().round() as usize;
        let smooth = smooth.max(1);
        Ok(Self {
            period,
            half_wma: Wma::new(half)?,
            full_wma: Wma::new(period)?,
            smooth_wma: Wma::new(smooth)?,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Hma {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        // Feed both windowed WMAs on every input so they warm up in parallel.
        // Gating `full_wma.update` behind `half_wma.update(...)?` would starve
        // the longer WMA during the shorter one's warmup, delaying the first
        // emission past `warmup_period()`.
        let h = self.half_wma.update(input);
        let f = self.full_wma.update(input);
        let (h, f) = (h?, f?);
        let diff = 2.0 * h - f;
        self.smooth_wma.update(diff)
    }

    fn reset(&mut self) {
        self.half_wma.reset();
        self.full_wma.reset();
        self.smooth_wma.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        let sm = (self.period as f64).sqrt().round() as usize;
        self.period + sm.max(1) - 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.smooth_wma.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "HMA"
    }

    /// SIMD kernel: the half and full WMAs, their `2 · half − full`
    /// difference and the smoothing WMA over it, each as the WMA prefix-scan
    /// kernel. Agrees with the exact batch to within a few units in the last
    /// place; warmup `NaN`s and length are identical. HMA only remembers its
    /// last `period + smooth − 1` inputs, so afterwards its state is rebuilt
    /// exactly by replaying them.
    fn batch_fast_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        assert_eq!(
            inputs.len(),
            out.len(),
            "batch output length must equal input length"
        );
        let (half, full, smooth) = (
            self.half_wma.period(),
            self.full_wma.period(),
            self.smooth_wma.period(),
        );
        let n = inputs.len();
        let span = full + smooth - 1;
        if !(self.half_wma.is_empty() && self.full_wma.is_empty() && self.smooth_wma.is_empty())
            || n < span
            || !crate::fast::in_range(inputs)
        {
            self.batch_nan_into(inputs, out);
            return;
        }
        crate::fast::with_scratch(n, |tmp| {
            wickra_simd::dispatch(crate::fast::HmaFast {
                x: inputs,
                half,
                full,
                smooth,
                tmp,
                out,
                _borrow: std::marker::PhantomData,
            });
        });
        crate::fast::replay_tail(self, &inputs[n - span..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn constant_series_yields_constant_hma() {
        let mut hma = Hma::new(9).unwrap();
        let out = hma.batch(&[10.0_f64; 80]);
        let last = out.iter().rev().flatten().next().unwrap();
        assert_relative_eq!(*last, 10.0, epsilon = 1e-9);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=100).map(|i| f64::from(i) * 0.7).collect();
        let mut a = Hma::new(9).unwrap();
        let mut b = Hma::new(9).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut hma = Hma::new(9).unwrap();
        hma.batch(&(1..=80).map(f64::from).collect::<Vec<_>>());
        assert!(hma.is_ready());
        hma.reset();
        assert!(!hma.is_ready());
    }

    #[test]
    fn rejects_zero_period() {
        assert!(Hma::new(0).is_err());
    }

    /// Cover the const accessor `period` (51-53) and the Indicator-impl
    /// `name` body (87-89). `warmup_period` is covered by
    /// `first_emission_matches_warmup_period`.
    #[test]
    fn accessors_and_metadata() {
        let hma = Hma::new(9).unwrap();
        assert_eq!(hma.period(), 9);
        assert_eq!(hma.name(), "HMA");
    }

    #[test]
    fn first_emission_matches_warmup_period() {
        let prices: Vec<f64> = (1..=40).map(f64::from).collect();
        let mut hma = Hma::new(9).unwrap();
        let out = hma.batch(&prices);
        let warmup = hma.warmup_period();
        assert_eq!(warmup, 11);
        for (i, v) in out.iter().enumerate().take(warmup - 1) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        assert!(
            out[warmup - 1].is_some(),
            "first HMA value must land at warmup_period - 1"
        );
    }

    #[test]
    fn matches_independent_wmas() {
        // The two inner WMAs run as independent siblings on the price stream;
        // HMA must equal feeding three standalone WMAs and combining them.
        let prices: Vec<f64> = (1..=50)
            .map(|i| (f64::from(i) * 0.3).sin() * 10.0 + 50.0)
            .collect();
        let mut hma = Hma::new(9).unwrap();
        let mut half = Wma::new(4).unwrap(); // (9 / 2).max(1)
        let mut full = Wma::new(9).unwrap();
        let mut smooth = Wma::new(3).unwrap(); // round(sqrt(9))
        for (i, &p) in prices.iter().enumerate() {
            let got = hma.update(p);
            let want = match (half.update(p), full.update(p)) {
                (Some(h), Some(f)) => smooth.update(2.0 * h - f),
                _ => None,
            };
            // HMA and the independent WMA chain share a warmup formula.
            assert_eq!(got.is_some(), want.is_some(), "readiness mismatch at {i}");
            if let (Some(a), Some(b)) = (got, want) {
                assert_relative_eq!(a, b, epsilon = 1e-9);
            }
        }
    }
}
