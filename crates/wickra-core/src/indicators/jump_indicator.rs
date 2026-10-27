//! Jump Indicator — detects return outliers relative to trailing volatility.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::traits::Indicator;

/// Jump Indicator — a discrete `{−1, 0, +1}` flag for whether the current log
/// return is an outlier relative to the trailing volatility of returns.
///
/// ```text
/// rₜ   = ln(priceₜ / priceₜ₋₁)
/// μ, σ = sample mean and stddev of the `period` returns *before* rₜ (trailing)
/// flag = +1 if rₜ − μ >  threshold · σ
///        −1 if rₜ − μ < −threshold · σ
///         0 otherwise
/// ```
///
/// The baseline is the trailing return distribution and **excludes** the current
/// return, so a genuine jump cannot inflate the band it is tested against.
/// Measuring the deviation from the trailing mean `μ` (not the raw return) means
/// a steady drift is *not* flagged — only moves that are large relative to the
/// recent return distribution count. `+1` marks an up jump, `−1` a down jump,
/// and `0` an ordinary move. When the trailing window has zero dispersion
/// (`σ = 0`, e.g. a perfectly constant drift) there is no defined baseline and
/// the indicator returns `0` rather than flagging every move.
///
/// This is the generic, threshold-tunable detector; downstream models keep any
/// regime-specific sensitivity by choosing `threshold`. Non-finite and
/// non-positive prices are ignored (the log return is undefined): the tick is
/// dropped and the last value returned.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, JumpIndicator};
///
/// let mut indicator = JumpIndicator::new(20, 3.0).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.5).sin());
/// }
/// // A calm sinusoid produces no jumps.
/// assert_eq!(last, Some(0.0));
/// ```
#[derive(Debug, Clone)]
pub struct JumpIndicator {
    period: usize,
    threshold: f64,
    prev_price: Option<f64>,
    /// Trailing window of the `period` returns preceding the current one.
    window: VecDeque<f64>,
    moments: ShiftedMoments,
    last: Option<f64>,
}

impl JumpIndicator {
    /// Construct a new Jump Indicator.
    ///
    /// `threshold` is the number of trailing standard deviations a return must
    /// exceed to be flagged.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` (the sample standard
    /// deviation needs at least two returns), or [`Error::InvalidParameter`] if
    /// `threshold` is not finite and positive.
    pub fn new(period: usize, threshold: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "jump indicator needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !threshold.is_finite() || threshold <= 0.0 {
            return Err(Error::InvalidParameter {
                message: "jump indicator threshold must be finite and positive",
            });
        }
        Ok(Self {
            period,
            threshold,
            prev_price: None,
            window: VecDeque::with_capacity(period),
            moments: ShiftedMoments::new(),
            last: None,
        })
    }

    /// Configured `(period, threshold)`.
    pub const fn params(&self) -> (usize, f64) {
        (self.period, self.threshold)
    }
}

impl Indicator for JumpIndicator {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() || input <= 0.0 {
            return None;
        }
        let Some(prev) = self.prev_price else {
            self.prev_price = Some(input);
            return None;
        };
        self.prev_price = Some(input);
        let r = (input / prev).ln();
        if self.window.len() < self.period {
            // Still filling the trailing window; no baseline yet.
            self.window.push_back(r);
            self.moments.push(r);
            return None;
        }
        // Trailing window is full: classify `r` against the volatility of the
        // `period` returns that precede it.
        let mean = self.moments.mean(self.period);
        let sd = self.moments.sample_variance(self.period).sqrt();
        let deviation = r - mean;
        let label = if sd == 0.0 {
            0.0
        } else if deviation > self.threshold * sd {
            1.0
        } else if deviation < -self.threshold * sd {
            -1.0
        } else {
            0.0
        };
        // Slide the trailing window forward to include `r`.
        let old = self.window.pop_front().expect("window is non-empty");
        self.moments.evict(old);
        self.window.push_back(r);
        self.moments.push(r);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        self.last = Some(label);
        Some(label)
    }

    fn reset(&mut self) {
        self.prev_price = None;
        self.window.clear();
        self.moments.reset();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One price seeds `prev`, `period` returns fill the trailing window,
        // then the next return is the first one classified.
        self.period + 2
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "JumpIndicator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    #[test]
    fn rejects_bad_params() {
        assert!(matches!(
            JumpIndicator::new(1, 3.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            JumpIndicator::new(20, 0.0),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            JumpIndicator::new(20, f64::NAN),
            Err(Error::InvalidParameter { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let ji = JumpIndicator::new(20, 3.0).unwrap();
        assert_eq!(ji.params(), (20, 3.0));
        assert_eq!(ji.warmup_period(), 22);
        assert_eq!(ji.name(), "JumpIndicator");
        assert!(!ji.is_ready());
    }

    #[test]
    fn detects_upward_jump() {
        let mut ji = JumpIndicator::new(10, 3.0).unwrap();
        // Calm oscillating warmup (small, varied returns), then a +20% spike.
        let mut prices: Vec<f64> = (0..20)
            .map(|i| 100.0 + (f64::from(i) * 0.7).sin() * 0.2)
            .collect();
        let last_calm = *prices.last().unwrap();
        prices.push(last_calm * 1.2);
        let out = ji.batch(&prices);
        assert_eq!(out.last().copied().flatten(), Some(1.0));
    }

    #[test]
    fn detects_downward_jump() {
        let mut ji = JumpIndicator::new(10, 3.0).unwrap();
        let mut prices: Vec<f64> = (0..20)
            .map(|i| 100.0 + (f64::from(i) * 0.7).sin() * 0.2)
            .collect();
        let last_calm = *prices.last().unwrap();
        prices.push(last_calm * 0.8);
        let out = ji.batch(&prices);
        assert_eq!(out.last().copied().flatten(), Some(-1.0));
    }

    #[test]
    fn calm_series_has_no_jumps() {
        let mut ji = JumpIndicator::new(20, 3.0).unwrap();
        let prices: Vec<f64> = (0..80)
            .map(|i| 100.0 + (f64::from(i) * 0.5).sin())
            .collect();
        for v in ji.batch(&prices).into_iter().flatten() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn zero_trailing_volatility_returns_zero() {
        // A constant price has exactly-zero returns => zero trailing dispersion
        // => no defined baseline => label 0. (Pins the `sd == 0` branch with an
        // exact-zero series; a geometric drift is conceptually zero-vol too but
        // floating-point rounding of the log returns leaves ~1e-16 noise.)
        let mut ji = JumpIndicator::new(10, 3.0).unwrap();
        for v in ji.batch(&[100.0; 30]).into_iter().flatten() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn steady_drift_is_not_flagged() {
        // A near-constant positive drift (small, equal-ish returns) must not be
        // flagged: the deviation from the trailing mean stays well inside the
        // band even though the raw return is non-zero every bar.
        let mut ji = JumpIndicator::new(10, 3.0).unwrap();
        let prices: Vec<f64> = (0..40).map(|i| 100.0 + f64::from(i) * 0.5).collect();
        for v in ji.batch(&prices).into_iter().flatten() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn ignores_non_finite_and_non_positive() {
        let mut ji = JumpIndicator::new(5, 3.0).unwrap();
        let prices: Vec<f64> = (0..20)
            .map(|i| 100.0 + (f64::from(i) * 0.6).sin())
            .collect();
        let out = ji.batch(&prices);
        let last = *out.last().unwrap();
        assert!(last.is_some());
        assert_eq!(ji.update(f64::NAN), None);
        assert_eq!(ji.update(-1.0), None);
        assert_eq!(ji.update(0.0), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut ji = JumpIndicator::new(5, 3.0).unwrap();
        ji.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        assert!(ji.is_ready());
        ji.reset();
        assert!(!ji.is_ready());
        assert_eq!(ji.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 3.0)
            .collect();
        let batch = JumpIndicator::new(20, 3.0).unwrap().batch(&prices);
        let mut b = JumpIndicator::new(20, 3.0).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            JumpIndicator::new(crate::error::MAX_PERIOD + 1, 3.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
