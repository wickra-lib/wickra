//! Rolling Pain Index — mean depth of drawdowns.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Rolling Pain Index — Thomas Becker's continuous-pain risk measure.
///
/// Input is treated as an equity-curve sample. The Pain Index is the **mean**
/// drawdown depth over the trailing window of `period` bars, expressed as a
/// non-negative fraction:
///
/// ```text
/// peak_t   = running max over window up to t
/// dd_t     = (peak_t − equity_t) / peak_t          (0 if no drawdown)
/// PainIdx  = mean(dd_t over window)
/// ```
///
/// Where Ulcer Index uses an RMS aggregation that punishes deep drawdowns
/// disproportionately, the Pain Index uses a plain arithmetic mean. The two
/// are normally similar; the Pain Index reads slightly lower on stresses with
/// a few large drawdowns and similar elsewhere.
///
/// Each `update` is O(period).
#[derive(Debug, Clone)]
pub struct PainIndex {
    period: usize,
    window: VecDeque<f64>,
}

impl PainIndex {
    /// Construct a new rolling Pain Index.
    ///
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
        Ok(Self {
            period,
            window: VecDeque::with_capacity(period),
        })
    }

    /// Configured window length.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for PainIndex {
    type Input = f64;
    type Output = f64;

    #[inline]
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
        let mut peak = f64::NEG_INFINITY;
        let mut sum_dd = 0.0_f64;
        for &v in &self.window {
            if v > peak {
                peak = v;
            }
            if peak > 0.0 {
                sum_dd += (peak - v) / peak;
            }
        }
        Some(sum_dd / self.period as f64)
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
        "PainIndex"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(PainIndex::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let p = PainIndex::new(10).unwrap();
        assert_eq!(p.period(), 10);
        assert_eq!(p.name(), "PainIndex");
        assert_eq!(p.warmup_period(), 10);
    }

    #[test]
    fn pure_uptrend_yields_zero() {
        let mut p = PainIndex::new(5).unwrap();
        let out = p.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        for v in out.into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn reference_value() {
        // window [100, 120, 90]: peaks 100,120,120; dd: 0, 0, 0.25.
        // Pain = 0.25 / 3 ≈ 0.08333...
        let mut p = PainIndex::new(3).unwrap();
        let out = p.batch(&[100.0, 120.0, 90.0]);
        assert_relative_eq!(out[2].unwrap(), 0.25 / 3.0, epsilon = 1e-12);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut p = PainIndex::new(3).unwrap();
        assert_eq!(p.update(f64::NAN), None);
        assert_eq!(p.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut p = PainIndex::new(3).unwrap();
        p.batch(&[100.0, 90.0, 110.0]);
        assert!(p.is_ready());
        p.reset();
        assert!(!p.is_ready());
        assert_eq!(p.update(100.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..40)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 8.0)
            .collect();
        let batch = PainIndex::new(10).unwrap().batch(&prices);
        let mut s = PainIndex::new(10).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| s.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_positive_peak_yields_zero() {
        let mut p = PainIndex::new(3).unwrap();
        let out = p.batch(&[0.0_f64; 6]);
        for v in out.into_iter().flatten() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            PainIndex::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
