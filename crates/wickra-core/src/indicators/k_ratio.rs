//! K-Ratio (Kestner) — slope of the cumulative-return curve over the standard error of that slope.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// K-Ratio over a trailing window of `period` returns.
///
/// Lars Kestner's K-Ratio measures the *consistency* of an equity curve, not just
/// its return. It builds the cumulative-return curve over the window, fits an
/// ordinary-least-squares trend line through it against time, and divides the
/// fitted slope by the standard error of that slope:
///
/// ```text
/// equity_t = Σ_{i<=t} return_i           (cumulative curve, t = 1..period)
/// slope, intercept = OLS(equity_t ~ t)
/// SE(slope) = sqrt( (Σ residual² / (period − 2)) / Σ(t − t̄)² )
/// K-Ratio   = slope / SE(slope)
/// ```
///
/// A high K-Ratio means the equity curve climbs *steadily* — a steep slope with
/// little scatter around the trend. A strategy that earns the same total return in
/// a few lucky jumps scores lower because its residual scatter inflates the
/// standard error. This is the original 1996 form; later Kestner revisions scale by
/// the number of periods (`slope / (SE · period)` in 2003, `slope · √period / SE`
/// in 2013) — apply that scaling downstream if you need to compare across window
/// lengths.
///
/// A perfectly straight window (e.g. constant returns) has zero residual scatter,
/// so the slope's standard error is zero and the K-Ratio is undefined; the
/// indicator reports `0.0` in that degenerate case. The statistic therefore needs
/// some dispersion in the returns to be meaningful.
///
/// The first value lands after `period` returns; each `update` re-fits the line
/// over the window (O(period)), which is O(1) in the length of the overall series.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, KRatio};
///
/// let mut indicator = KRatio::new(30).unwrap();
/// let mut last = None;
/// for i in 0..60 {
///     last = indicator.update(0.001 + (f64::from(i) * 0.3).sin() * 0.01);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct KRatio {
    period: usize,
    window: VecDeque<f64>,
}

impl KRatio {
    /// Construct a K-Ratio over `period` returns.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `period < 3` (the slope's standard error
    /// divides by `period − 2`).
    pub fn new(period: usize) -> Result<Self> {
        if period < 3 {
            return Err(Error::InvalidPeriod {
                message: "k-ratio needs period >= 3",
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
        let count = self.window.len();
        #[allow(clippy::cast_precision_loss)]
        let length = count as f64;
        // Build the cumulative-equity curve and its mean.
        let mut equity = 0.0;
        let mut curve: Vec<f64> = Vec::with_capacity(count);
        let mut sum_equity = 0.0;
        for ret in &self.window {
            equity += *ret;
            curve.push(equity);
            sum_equity += equity;
        }
        // Times are 1..=count, so Σt = count(count+1)/2 in closed form.
        let mean_time = f64::midpoint(length, 1.0);
        let mean_equity = sum_equity / length;
        let mut sxx = 0.0;
        let mut sxy = 0.0;
        for (index, value) in curve.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let time = (index + 1) as f64;
            let dt = time - mean_time;
            sxx += dt * dt;
            sxy += dt * (value - mean_equity);
        }
        // sxx > 0 for count >= 2 (distinct integer times), guaranteed by period >= 3.
        let slope = sxy / sxx;
        let intercept = mean_equity - slope * mean_time;
        let mut sse = 0.0;
        for (index, value) in curve.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let time = (index + 1) as f64;
            let residual = value - (intercept + slope * time);
            sse += residual * residual;
        }
        if sse <= 0.0 {
            return 0.0;
        }
        let se_slope = (sse / (length - 2.0) / sxx).sqrt();
        slope / se_slope
    }
}

impl Indicator for KRatio {
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
        "KRatio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_less_than_three() {
        assert!(matches!(KRatio::new(2), Err(Error::InvalidPeriod { .. })));
        assert!(matches!(KRatio::new(0), Err(Error::InvalidPeriod { .. })));
    }

    #[test]
    fn accessors_and_metadata() {
        let kr = KRatio::new(30).unwrap();
        assert_eq!(kr.period(), 30);
        assert_eq!(kr.warmup_period(), 30);
        assert_eq!(kr.name(), "KRatio");
        assert!(!kr.is_ready());
    }

    #[test]
    fn reference_value() {
        // returns [0.01, 0.02, 0.03] -> equity curve [0.01, 0.03, 0.06].
        // slope = 0.025, SE(slope) = sqrt((1/60000)/1/2) = 1/sqrt(120000).
        // K-Ratio = 0.025 * sqrt(120000) = 5*sqrt(3) ≈ 8.660254.
        let mut kr = KRatio::new(3).unwrap();
        let out = kr.batch(&[0.01, 0.02, 0.03]);
        let expected = 0.025_f64 / (1.0_f64 / 120_000.0).sqrt();
        assert_relative_eq!(out[2].unwrap(), expected, epsilon = 1e-6);
    }

    #[test]
    fn constant_returns_are_degenerate_zero() {
        // A perfectly linear equity curve has zero residual scatter -> undefined.
        let mut kr = KRatio::new(4).unwrap();
        let last = kr.batch(&[0.01; 4]).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn rising_curve_is_positive() {
        let mut kr = KRatio::new(5).unwrap();
        let last = kr
            .batch(&[0.01, 0.012, 0.009, 0.011, 0.013])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert!(last > 0.0);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut kr = KRatio::new(3).unwrap();
        assert_eq!(kr.update(0.01), None);
        assert_eq!(kr.update(f64::NAN), None);
        assert_eq!(kr.update(0.02), None);
        assert!(kr.update(0.03).is_some());
    }

    #[test]
    fn reset_clears_state() {
        let mut kr = KRatio::new(3).unwrap();
        kr.batch(&[0.01, 0.02, 0.03]);
        assert!(kr.is_ready());
        kr.reset();
        assert!(!kr.is_ready());
        assert_eq!(kr.update(0.01), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let rets: Vec<f64> = (0..60)
            .map(|i| 0.001 + (f64::from(i) * 0.25).sin() * 0.01)
            .collect();
        let batch = KRatio::new(20).unwrap().batch(&rets);
        let mut streamer = KRatio::new(20).unwrap();
        let streamed: Vec<_> = rets.iter().map(|r| streamer.update(*r)).collect();
        assert_eq!(batch, streamed);
    }
}
