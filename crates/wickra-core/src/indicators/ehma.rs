//! Exponential Hull Moving Average (EHMA).

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::traits::Indicator;

/// Exponential Hull Moving Average: the Hull construction built from EMAs
/// instead of WMAs.
///
/// ```text
/// EHMA = EMA( 2 · EMA(price, period/2) − EMA(price, period), round(sqrt(period)) )
/// ```
///
/// Alan Hull's [`crate::Hma`](crate::Hma) uses weighted moving averages; replacing them
/// with exponential moving averages keeps the same lag-reduction trick — a fast
/// half-length average minus a full-length one, smoothed over `sqrt(period)` —
/// while inheriting the EMA's strictly recursive O(1) update and infinite
/// (exponentially decaying) memory. The result is marginally smoother than the
/// WMA-based Hull at the cost of a little more lag.
///
/// The half period is `(period / 2).max(1)` and the smoothing period is
/// `round(sqrt(period)).max(1)`, matching the rounding used by [`crate::Hma`].
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Ehma};
///
/// let mut indicator = Ehma::new(9).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Ehma {
    period: usize,
    half_ema: Ema,
    full_ema: Ema,
    smooth_ema: Ema,
}

impl Ehma {
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
            half_ema: Ema::new(half)?,
            full_ema: Ema::new(period)?,
            smooth_ema: Ema::new(smooth)?,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Ehma {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        // Feed both component EMAs on every input so they warm up in parallel;
        // gating the longer one behind the shorter would delay the first
        // emission past `warmup_period()`.
        let h = self.half_ema.update(input);
        let f = self.full_ema.update(input);
        let (h, f) = (h?, f?);
        let diff = 2.0 * h - f;
        self.smooth_ema.update(diff)
    }

    fn reset(&mut self) {
        self.half_ema.reset();
        self.full_ema.reset();
        self.smooth_ema.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // full_ema seeds at `period`, then smooth_ema needs another
        // (round(sqrt(period)) - 1) values to seed.
        let sm = (self.period as f64).sqrt().round() as usize;
        self.period + sm.max(1) - 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.smooth_ema.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "EHMA"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn constant_series_yields_constant_ehma() {
        let mut ehma = Ehma::new(9).unwrap();
        let out = ehma.batch(&[10.0_f64; 80]);
        let last = out.iter().rev().flatten().next().unwrap();
        assert_relative_eq!(*last, 10.0, epsilon = 1e-9);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=100).map(|i| f64::from(i) * 0.7).collect();
        let mut a = Ehma::new(9).unwrap();
        let mut b = Ehma::new(9).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut ehma = Ehma::new(9).unwrap();
        ehma.batch(&(1..=80).map(f64::from).collect::<Vec<_>>());
        assert!(ehma.is_ready());
        ehma.reset();
        assert!(!ehma.is_ready());
    }

    #[test]
    fn rejects_zero_period() {
        assert!(Ehma::new(0).is_err());
    }

    /// Cover the const accessor `period` and the Indicator-impl `name`.
    /// `warmup_period` is covered by `first_emission_matches_warmup_period`.
    #[test]
    fn accessors_and_metadata() {
        let ehma = Ehma::new(9).unwrap();
        assert_eq!(ehma.period(), 9);
        assert_eq!(ehma.name(), "EHMA");
    }

    #[test]
    fn first_emission_matches_warmup_period() {
        let prices: Vec<f64> = (1..=40).map(f64::from).collect();
        let mut ehma = Ehma::new(9).unwrap();
        let out = ehma.batch(&prices);
        let warmup = ehma.warmup_period();
        // full EMA seeds at 9, smooth EMA round(sqrt(9))=3 needs 2 more -> 11.
        assert_eq!(warmup, 11);
        for (i, v) in out.iter().enumerate().take(warmup - 1) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        assert!(
            out[warmup - 1].is_some(),
            "first EHMA value must land at warmup_period - 1"
        );
    }

    #[test]
    fn matches_independent_emas() {
        // The two component EMAs run as independent siblings on the price
        // stream; EHMA must equal feeding three standalone EMAs and combining.
        let prices: Vec<f64> = (1..=50)
            .map(|i| (f64::from(i) * 0.3).sin() * 10.0 + 50.0)
            .collect();
        let mut ehma = Ehma::new(9).unwrap();
        let mut half = Ema::new(4).unwrap(); // (9 / 2).max(1)
        let mut full = Ema::new(9).unwrap();
        let mut smooth = Ema::new(3).unwrap(); // round(sqrt(9))
        for (i, &p) in prices.iter().enumerate() {
            let got = ehma.update(p);
            let want = match (half.update(p), full.update(p)) {
                (Some(h), Some(f)) => smooth.update(2.0 * h - f),
                _ => None,
            };
            assert_eq!(got.is_some(), want.is_some(), "readiness mismatch at {i}");
            if let (Some(a), Some(b)) = (got, want) {
                assert_relative_eq!(a, b, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn period_one_collapses_to_pass_through() {
        // period 1: half=1, full=1, smooth=round(sqrt(1))=1; every EMA seeds on
        // the first input, so EHMA(1) passes the price straight through.
        let mut ehma = Ehma::new(1).unwrap();
        assert_relative_eq!(ehma.update(5.0).unwrap(), 5.0, epsilon = 1e-12);
        assert_relative_eq!(ehma.update(8.0).unwrap(), 8.0, epsilon = 1e-12);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Ehma::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
