//! Martin Ratio (Ulcer Performance Index) — mean return over the Ulcer Index.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Martin Ratio — also called the Ulcer Performance Index (UPI) — over a trailing
/// window of `period` returns.
///
/// ```text
/// equity_t = Π_{i<=t} (1 + return_i)               (compounded curve)
/// peak_t   = max_{s<=t} equity_s
/// dd_t%    = 100 · (peak_t − equity_t) / peak_t      (percentage drawdown)
/// UlcerIdx = sqrt( mean( dd_t%² ) )
/// Martin   = mean(returns) / UlcerIdx
/// ```
///
/// The Martin Ratio divides the average per-period return by the **Ulcer Index** —
/// the root-mean-square of the *percentage* drawdowns. The Ulcer Index, by
/// construction, measures the depth *and* duration of the time spent under water:
/// a long shallow slump and a short deep one can score the same. Compared to
/// Wickra's other drawdown ratios, Martin uses the RMS (not the average as in the
/// [`SterlingRatio`](crate::SterlingRatio), nor the un-normalised sum-norm as in the
/// [`BurkeRatio`](crate::BurkeRatio)) and expresses drawdowns in **percent**, so its
/// denominator is on a `0..100` scale and its output is numerically smaller than
/// the fractional-drawdown ratios. A window that never draws down has an Ulcer Index
/// of zero and the indicator reports `0.0`.
///
/// The first value lands after `period` returns; each `update` rebuilds the equity
/// curve over the window (O(period)), which is O(1) in the length of the overall
/// series.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, MartinRatio};
///
/// let mut indicator = MartinRatio::new(14).unwrap();
/// let mut last = None;
/// for i in 0..28 {
///     last = indicator.update((f64::from(i) * 0.5).sin() * 0.05);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct MartinRatio {
    period: usize,
    window: VecDeque<f64>,
}

impl MartinRatio {
    /// Construct a Martin Ratio over `period` returns.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `period < 2`.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "martin ratio needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            window: VecDeque::with_capacity(period),
        })
    }

    /// Configured window of returns.
    pub const fn period(&self) -> usize {
        self.period
    }

    fn compute(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let length = self.window.len() as f64;
        let mut sum_return = 0.0;
        let mut sum_drawdown_pct_sq = 0.0;
        let mut equity = 1.0;
        let mut peak: f64 = 1.0;
        for ret in &self.window {
            sum_return += *ret;
            equity *= 1.0 + *ret;
            peak = peak.max(equity);
            let drawdown_pct = 100.0 * (peak - equity) / peak;
            sum_drawdown_pct_sq += drawdown_pct * drawdown_pct;
        }
        let ulcer_index = (sum_drawdown_pct_sq / length).sqrt();
        if ulcer_index > 0.0 {
            (sum_return / length) / ulcer_index
        } else {
            0.0
        }
    }
}

impl Indicator for MartinRatio {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, ret: f64) -> Option<f64> {
        if !ret.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(ret);
        if self.window.len() < self.period {
            return None;
        }
        Some(self.compute())
    }

    fn reset(&mut self) {
        self.window.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "MartinRatio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_less_than_two() {
        assert!(matches!(
            MartinRatio::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let mr = MartinRatio::new(14).unwrap();
        assert_eq!(mr.period(), 14);
        assert_eq!(mr.warmup_period(), 14);
        assert_eq!(mr.name(), "MartinRatio");
        assert!(!mr.is_ready());
    }

    #[test]
    fn reference_value() {
        // returns [0.1, -0.1, 0.1]: drawdowns% = [0, 10, 1].
        // Ulcer Index = sqrt((0 + 100 + 1)/3) = sqrt(101/3).
        // Martin = (0.1/3) / sqrt(101/3).
        let mut mr = MartinRatio::new(3).unwrap();
        let out = mr.batch(&[0.1, -0.1, 0.1]);
        let expected = (0.1_f64 / 3.0) / (101.0_f64 / 3.0).sqrt();
        assert_relative_eq!(out[2].unwrap(), expected, epsilon = 1e-9);
    }

    #[test]
    fn no_drawdown_is_zero() {
        let mut mr = MartinRatio::new(3).unwrap();
        let last = mr
            .batch(&[0.01, 0.02, 0.03])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn losing_window_is_negative() {
        let mut mr = MartinRatio::new(3).unwrap();
        let last = mr
            .batch(&[-0.05, -0.02, -0.03])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last < 0.0);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut mr = MartinRatio::new(3).unwrap();
        assert_eq!(mr.update(0.1), None);
        assert_eq!(mr.update(f64::NAN), None);
        assert_eq!(mr.update(-0.1), None);
        assert!(mr.update(0.1).is_some());
    }

    #[test]
    fn reset_clears_state() {
        let mut mr = MartinRatio::new(3).unwrap();
        mr.batch(&[0.1, -0.1, 0.1]);
        assert!(mr.is_ready());
        mr.reset();
        assert!(!mr.is_ready());
        assert_eq!(mr.update(0.1), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let rets: Vec<f64> = (0..60)
            .map(|i| (f64::from(i) * 0.25).sin() * 0.05)
            .collect();
        let batch = MartinRatio::new(14).unwrap().batch(&rets);
        let mut streamer = MartinRatio::new(14).unwrap();
        let streamed: Vec<_> = rets.iter().map(|r| streamer.update(*r)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            MartinRatio::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
