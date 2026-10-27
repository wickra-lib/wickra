//! Trend Label — the sign of the rolling least-squares slope.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Trend Label — a discrete `{−1, 0, +1}` classification of the local trend from
/// the sign of the ordinary-least-squares slope over the last `period` values.
///
/// ```text
/// slope = Σ (tᵢ − t̄)(xᵢ − x̄) / Σ (tᵢ − t̄)²      (regress price on bar index)
/// label = +1 if slope > 0,  −1 if slope < 0,  0 if slope == 0
/// ```
///
/// The sign of the regression slope is *scale-invariant* — it does not depend on
/// the nominal price level — which makes it a clean, comparable trend state
/// across instruments. `+1` marks a rising regression line, `−1` a falling one,
/// and `0` a perfectly flat window. It is the discrete companion to
/// [`LinRegSlope`](crate::LinRegSlope) (which returns the continuous slope): use
/// the label when a feature pipeline wants a categorical trend direction and
/// keys any magnitude / dead-band tuning on the raw slope itself.
///
/// Each `update` is `O(period)`: the slope numerator is recomputed from the
/// window. The denominator `Σ(tᵢ − t̄)²` is strictly positive for `period ≥ 2`,
/// so the sign is always well-defined.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, TrendLabel};
///
/// let mut indicator = TrendLabel::new(10).unwrap();
/// let mut last = None;
/// for i in 0..20 {
///     last = indicator.update(100.0 + f64::from(i)); // strictly rising
/// }
/// assert_eq!(last, Some(1.0));
/// ```
#[derive(Debug, Clone)]
pub struct TrendLabel {
    period: usize,
    window: VecDeque<f64>,
}

impl TrendLabel {
    /// Construct a new Trend Label classifier.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` — a slope needs at least
    /// two points.
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "trend label needs period >= 2",
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

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for TrendLabel {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, value: f64) -> Option<f64> {
        if !value.is_finite() {
            return None;
        }
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(value);
        if self.window.len() < self.period {
            return None;
        }
        let count = self.period as f64;
        let mean_t = (count - 1.0) / 2.0;
        let mean_x = self.window.iter().sum::<f64>() / count;
        // Slope numerator: Σ (t − t̄)(x − x̄). The denominator Σ(t − t̄)² > 0 for
        // period >= 2, so the slope sign equals the numerator sign.
        let mut numerator = 0.0;
        for (t, &x) in self.window.iter().enumerate() {
            numerator += (t as f64 - mean_t) * (x - mean_x);
        }
        let label = if numerator > 0.0 {
            1.0
        } else if numerator < 0.0 {
            -1.0
        } else {
            0.0
        };
        Some(label)
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
        "TrendLabel"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    #[test]
    fn rejects_period_below_two() {
        assert!(matches!(
            TrendLabel::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(TrendLabel::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let tl = TrendLabel::new(10).unwrap();
        assert_eq!(tl.period(), 10);
        assert_eq!(tl.warmup_period(), 10);
        assert_eq!(tl.name(), "TrendLabel");
        assert!(!tl.is_ready());
    }

    #[test]
    fn rising_series_is_plus_one() {
        let mut tl = TrendLabel::new(10).unwrap();
        let prices: Vec<f64> = (0..20).map(f64::from).collect();
        assert_eq!(tl.batch(&prices).into_iter().flatten().last(), Some(1.0));
    }

    #[test]
    fn falling_series_is_minus_one() {
        let mut tl = TrendLabel::new(10).unwrap();
        let prices: Vec<f64> = (0..20).map(|i| 100.0 - f64::from(i)).collect();
        assert_eq!(tl.batch(&prices).into_iter().flatten().last(), Some(-1.0));
    }

    #[test]
    fn flat_series_is_zero() {
        let mut tl = TrendLabel::new(8).unwrap();
        for v in tl.batch(&[42.0; 16]).into_iter().flatten() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn scale_invariant_sign() {
        // Multiplying the whole series by a constant cannot change the trend sign.
        let prices: Vec<f64> = (0..30)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 5.0)
            .collect();
        let small = TrendLabel::new(12).unwrap().batch(&prices);
        let scaled: Vec<f64> = prices.iter().map(|p| p * 1000.0).collect();
        let large = TrendLabel::new(12).unwrap().batch(&scaled);
        assert_eq!(small, large);
    }

    #[test]
    fn output_is_ternary() {
        let mut tl = TrendLabel::new(14).unwrap();
        let prices: Vec<f64> = (0..200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        for v in tl.batch(&prices).into_iter().flatten() {
            assert!(v == -1.0 || v == 0.0 || v == 1.0, "non-ternary label {v}");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut tl = TrendLabel::new(5).unwrap();
        tl.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(tl.is_ready());
        tl.reset();
        assert!(!tl.is_ready());
        assert_eq!(tl.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let batch = TrendLabel::new(14).unwrap().batch(&prices);
        let mut b = TrendLabel::new(14).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            TrendLabel::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
