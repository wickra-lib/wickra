//! Rolling population standard deviation.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::traits::Indicator;

/// Rolling population standard deviation over the last `period` values.
///
/// ```text
/// mean     = (1/n) · Σ price
/// variance = (1/n) · Σ (price − mean)²
/// StdDev   = √variance
/// ```
///
/// This is the **population** standard deviation (divisor `n`, not `n − 1`) —
/// the same dispersion measure that drives [`BollingerBands`](crate::BollingerBands).
/// It is maintained as an O(1) rolling state machine: running first and second
/// moments, updated by one add and one subtract per bar. The moments are
/// accumulated relative to a reference point inside the window
/// (`ShiftedMoments`) rather than around zero, because `E[x²] - E[x]²` on raw
/// price levels cancels catastrophically — at a level of 1e5 with a tight range
/// it loses most of its significant digits, and at 1e8 it collapses to exactly
/// zero.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, StdDev};
///
/// let mut indicator = StdDev::new(20).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct StdDev {
    period: usize,
    window: VecDeque<f64>,
    moments: ShiftedMoments,
    last: Option<f64>,
}

impl StdDev {
    /// Construct a new rolling standard deviation with the given period.
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
            window: VecDeque::with_capacity(period),
            moments: ShiftedMoments::new(),
            last: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for StdDev {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            // Non-finite input is ignored; the window is left untouched.
            return None;
        }
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("window is non-empty");
            self.moments.evict(old);
        }
        self.window.push_back(input);
        self.moments.push(input);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let sd = self.moments.std_dev(self.period);
        self.last = Some(sd);
        Some(sd)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.moments.reset();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "StdDev"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Two-pass population standard deviation — the numerically stable form,
    /// used as the reference the rolling state machine must match.
    fn reference_std_dev(window: &[f64]) -> f64 {
        let n = window.len() as f64;
        let mean = window.iter().sum::<f64>() / n;
        (window.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n).sqrt()
    }

    /// The rolling moments must stay accurate when the values are large relative
    /// to their spread — exactly the shape of a real price series. The textbook
    /// `E[x²] - E[x]²` form cancels catastrophically here: at a level of 1e5 it
    /// loses most of its significant digits, and at 1e8 it collapses to zero.
    #[test]
    fn stays_accurate_when_the_level_dwarfs_the_spread() {
        for level in [1.0e2_f64, 1.0e5, 1.0e8] {
            let prices: Vec<f64> = (0..60)
                .map(|i| level + (f64::from(i) * 0.7).sin())
                .collect();
            let mut sd = StdDev::new(20).unwrap();
            let mut got = 0.0;
            for price in &prices {
                if let Some(v) = sd.update(*price) {
                    got = v;
                }
            }
            assert_relative_eq!(got, reference_std_dev(&prices[40..]), max_relative = 1e-9);
        }
    }

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(StdDev::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `value` and the Indicator-impl
    /// `warmup_period` / `name` methods (lines 64-71, 110-112, 118-120).
    /// Existing tests only inspect numeric outputs of `update` / `batch`.
    #[test]
    fn accessors_and_metadata() {
        let mut sd = StdDev::new(14).unwrap();
        assert_eq!(sd.period(), 14);
        assert_eq!(sd.warmup_period(), 14);
        assert_eq!(sd.name(), "StdDev");
        assert_eq!(sd.value(), None);
        for i in 1..=14 {
            sd.update(f64::from(i));
        }
        assert!(sd.value().is_some());
    }

    #[test]
    fn reference_value() {
        // StdDev(3) of [2, 4, 6]: mean = 4, variance = (4+0+4)/3 = 8/3.
        let mut sd = StdDev::new(3).unwrap();
        let out = sd.batch(&[2.0, 4.0, 6.0]);
        assert_eq!(out[0], None);
        assert_eq!(out[1], None);
        assert_relative_eq!(out[2].unwrap(), (8.0_f64 / 3.0).sqrt(), epsilon = 1e-12);
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut sd = StdDev::new(5).unwrap();
        let out = sd.batch(&[42.0; 20]);
        for v in out.iter().skip(4).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn matches_naive_definition() {
        let prices: Vec<f64> = (1..=60)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 8.0)
            .collect();
        let period = 10;
        let got = StdDev::new(period).unwrap().batch(&prices);
        for (i, g) in got.iter().enumerate() {
            if let Some(value) = g {
                let window = &prices[i + 1 - period..=i];
                let mean = window.iter().sum::<f64>() / period as f64;
                let var = window.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / period as f64;
                assert_relative_eq!(*value, var.sqrt(), epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut sd = StdDev::new(3).unwrap();
        let out = sd.batch(&[2.0, 4.0, 6.0]);
        let last = out[2];
        assert!(last.is_some());
        assert_eq!(sd.update(f64::NAN), None);
        assert_eq!(sd.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut sd = StdDev::new(3).unwrap();
        sd.batch(&[1.0, 2.0, 3.0, 4.0]);
        assert!(sd.is_ready());
        sd.reset();
        assert!(!sd.is_ready());
        assert_eq!(sd.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=60)
            .map(|i| 100.0 + (f64::from(i) * 0.3).cos() * 7.0)
            .collect();
        let batch = StdDev::new(14).unwrap().batch(&prices);
        let mut b = StdDev::new(14).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            StdDev::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
