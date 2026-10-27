//! Shannon Entropy — the information content of a price window's distribution.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Shannon Entropy — the Shannon information entropy (in **bits**) of the
/// distribution of values in a rolling window, after binning them into a fixed
/// number of equal-width buckets.
///
/// ```text
/// bucket each of the last `period` values into `bins` equal-width bins over
///   [min, max] of the window
/// p_i = count_i / period
/// H   = − Σ p_i · log2(p_i)            (over non-empty bins)
/// ```
///
/// Entropy measures how *spread out* and unpredictable the recent values are. A
/// window concentrated in one bin (a flat or tightly-ranging market) has low
/// entropy near `0`; a window whose values are spread evenly across all bins (a
/// noisy, directionless market) approaches the maximum `log2(bins)`. Traders use
/// it as a **regime filter**: low entropy favours trend/breakout strategies, high
/// entropy favours mean-reversion or standing aside.
///
/// The output lies in `[0, log2(bins)]`. A degenerate window where every value is
/// identical (`max == min`) returns `0`. The first value lands after `period`
/// inputs; each `update` rebins the window in O(`period`).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, ShannonEntropy};
///
/// let mut indicator = ShannonEntropy::new(32, 8).unwrap();
/// let mut last = None;
/// for i in 0..64 {
///     last = indicator.update((f64::from(i) * 0.7).sin() * 10.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct ShannonEntropy {
    period: usize,
    bins: usize,
    window: VecDeque<f64>,
    last: Option<f64>,
}

impl ShannonEntropy {
    /// Construct a Shannon entropy over `period` values binned into `bins`
    /// buckets.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if either argument is `0`, or
    /// [`Error::InvalidPeriod`] if `bins < 2` (entropy needs at least two bins).
    pub fn new(period: usize, bins: usize) -> Result<Self> {
        if period == 0 || bins == 0 {
            return Err(Error::PeriodZero);
        }
        if bins < 2 {
            return Err(Error::InvalidPeriod {
                message: "Shannon entropy needs bins >= 2",
            });
        }
        if bins > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            bins,
            window: VecDeque::with_capacity(period),
            last: None,
        })
    }

    /// Configured `(period, bins)`.
    pub const fn params(&self) -> (usize, usize) {
        (self.period, self.bins)
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for ShannonEntropy {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(input);
        if self.window.len() < self.period {
            return None;
        }

        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for &v in &self.window {
            min = min.min(v);
            max = max.max(v);
        }
        if max <= min {
            // Degenerate window: all values identical -> zero entropy.
            self.last = Some(0.0);
            return Some(0.0);
        }
        let width = (max - min) / self.bins as f64;
        let mut counts = vec![0usize; self.bins];
        for &v in &self.window {
            // `(v - min) / width` is in [0, bins]; the cast truncates toward zero
            // (intended) and the value is non-negative, then clamped to the last
            // bin so the index is always valid.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let raw = ((v - min) / width) as usize;
            let idx = raw.min(self.bins - 1);
            counts[idx] += 1;
        }
        let n = self.period as f64;
        let mut h = 0.0;
        for &count in &counts {
            if count > 0 {
                let p = count as f64 / n;
                h -= p * p.log2();
            }
        }
        self.last = Some(h);
        Some(h)
    }

    fn reset(&mut self) {
        self.window.clear();
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
        "ShannonEntropy"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_invalid_params() {
        assert!(matches!(ShannonEntropy::new(0, 8), Err(Error::PeriodZero)));
        assert!(matches!(ShannonEntropy::new(32, 0), Err(Error::PeriodZero)));
        assert!(matches!(
            ShannonEntropy::new(32, 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let e = ShannonEntropy::new(32, 8).unwrap();
        assert_eq!(e.params(), (32, 8));
        assert_eq!(e.warmup_period(), 32);
        assert_eq!(e.name(), "ShannonEntropy");
        assert!(!e.is_ready());
        assert_eq!(e.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut e = ShannonEntropy::new(4, 4).unwrap();
        let out = e.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        for v in out.iter().take(3) {
            assert!(v.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn constant_window_is_zero() {
        let mut e = ShannonEntropy::new(8, 4).unwrap();
        let last = e.batch(&[5.0; 12]).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn uniform_window_is_max_entropy() {
        // One value per bin -> uniform distribution -> H = log2(bins).
        let mut e = ShannonEntropy::new(4, 4).unwrap();
        // Values 0,1,2,3 with min=0,max=3,width=0.75 -> bins 0,1,2,3.
        let last = e
            .batch(&[0.0, 1.0, 2.0, 3.0])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 2.0, epsilon = 1e-9); // log2(4) = 2
    }

    #[test]
    fn output_in_range() {
        let mut e = ShannonEntropy::new(32, 8).unwrap();
        let max_h = 8f64.log2();
        for v in e
            .batch(
                &(0..200)
                    .map(|i| (f64::from(i) * 0.3).sin() * 10.0)
                    .collect::<Vec<_>>(),
            )
            .into_iter()
            .flatten()
        {
            assert!((0.0..=max_h + 1e-9).contains(&v));
        }
    }

    #[test]
    fn ignores_non_finite() {
        let mut e = ShannonEntropy::new(4, 4).unwrap();
        let _ready = e
            .batch(&[1.0, 2.0, 3.0, 4.0])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(e.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut e = ShannonEntropy::new(4, 4).unwrap();
        e.batch(&[1.0, 2.0, 3.0, 4.0]);
        assert!(e.is_ready());
        e.reset();
        assert!(!e.is_ready());
        assert_eq!(e.value(), None);
        assert_eq!(e.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let xs: Vec<f64> = (0..120)
            .map(|i| (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let batch = ShannonEntropy::new(32, 8).unwrap().batch(&xs);
        let mut b = ShannonEntropy::new(32, 8).unwrap();
        let streamed: Vec<_> = xs.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_bins_above_maximum() {
        assert!(matches!(
            ShannonEntropy::new(32, crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
