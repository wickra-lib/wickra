//! Sine-Weighted Moving Average (SWMA).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Sine-Weighted Moving Average — a windowed average whose weights follow one
/// half-cycle of a sine wave.
///
/// Over the last `period` inputs the weight of the value at position
/// `i = 0, 1, …, period − 1` (oldest to newest) is
///
/// ```text
/// w_i = sin(π · (i + 1) / (period + 1))
/// SWMA = Σ (w_i · value_i) / Σ w_i
/// ```
///
/// The window is symmetric: weights rise to a peak in the middle of the window
/// and fall off at both ends, so the central observations dominate while the
/// extremes are de-emphasised. Every weight is strictly positive because the
/// argument `(i + 1) / (period + 1)` lies in the open interval `(0, 1)`, so the
/// normaliser is always non-zero.
///
/// Each `update` is O(`period`): the fixed weight vector is dotted with the
/// trailing window, mirroring the way [`Alma`](crate::Alma) recomputes its
/// Gaussian weights. `period == 1` collapses to a pass-through
/// (`w_0 = sin(π/2) = 1`).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, SineWeightedMa};
///
/// let mut indicator = SineWeightedMa::new(5).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct SineWeightedMa {
    period: usize,
    window: VecDeque<f64>,
    /// Sine weights for positions `0..period` (oldest to newest), constant in
    /// `period`.
    weights: Vec<f64>,
    weights_total: f64,
}

impl SineWeightedMa {
    /// Construct a new sine-weighted moving average over `period` inputs.
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
        let denom = period as f64 + 1.0;
        let weights: Vec<f64> = (0..period)
            .map(|i| (std::f64::consts::PI * (i as f64 + 1.0) / denom).sin())
            .collect();
        let weights_total = weights.iter().sum();
        Ok(Self {
            period,
            window: VecDeque::with_capacity(period),
            weights,
            weights_total,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if the window is full.
    pub fn value(&self) -> Option<f64> {
        if self.window.len() == self.period {
            let dot: f64 = self
                .window
                .iter()
                .zip(&self.weights)
                .map(|(v, w)| v * w)
                .sum();
            Some(dot / self.weights_total)
        } else {
            None
        }
    }
}

impl Indicator for SineWeightedMa {
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
        self.value()
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
        "SWMA"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Reference implementation: explicit sine-weighted average over a window.
    fn swma_naive(prices: &[f64], period: usize) -> Vec<Option<f64>> {
        let denom = period as f64 + 1.0;
        let weights: Vec<f64> = (0..period)
            .map(|i| (std::f64::consts::PI * (i as f64 + 1.0) / denom).sin())
            .collect();
        let total: f64 = weights.iter().sum();
        prices
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if i + 1 < period {
                    None
                } else {
                    let window = &prices[i + 1 - period..=i];
                    let dot: f64 = window.iter().zip(&weights).map(|(v, w)| v * w).sum();
                    Some(dot / total)
                }
            })
            .collect()
    }

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(SineWeightedMa::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessor `period` and the Indicator-impl `warmup_period`
    /// + `name`.
    #[test]
    fn accessors_and_metadata() {
        let swma = SineWeightedMa::new(7).unwrap();
        assert_eq!(swma.period(), 7);
        assert_eq!(swma.warmup_period(), 7);
        assert_eq!(swma.name(), "SWMA");
    }

    #[test]
    fn warmup_returns_none() {
        let mut swma = SineWeightedMa::new(3).unwrap();
        assert_eq!(swma.update(1.0), None);
        assert_eq!(swma.update(2.0), None);
        // SWMA(3): weights sin(pi/4), sin(pi/2), sin(3pi/4) = [√½, 1, √½].
        // Over [1,2,3]: (√½·1 + 1·2 + √½·3) / (√½ + 1 + √½).
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let total = s + 1.0 + s;
        let want = (s * 1.0 + 1.0 * 2.0 + s * 3.0) / total;
        assert_relative_eq!(swma.update(3.0).unwrap(), want, epsilon = 1e-12);
    }

    #[test]
    fn symmetric_weights_give_midpoint_on_linear_window() {
        // For a perfectly linear window the symmetric weighting reproduces the
        // arithmetic centre of the window.
        let mut swma = SineWeightedMa::new(5).unwrap();
        let v = swma.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_relative_eq!(v[4].unwrap(), 3.0, epsilon = 1e-12);
    }

    #[test]
    fn period_one_is_pass_through() {
        let mut swma = SineWeightedMa::new(1).unwrap();
        assert_relative_eq!(swma.update(5.5).unwrap(), 5.5, epsilon = 1e-12);
        assert_relative_eq!(swma.update(7.5).unwrap(), 7.5, epsilon = 1e-12);
    }

    #[test]
    fn matches_naive_over_inputs() {
        let prices: Vec<f64> = (1..=30).map(|i| f64::from(i) * 1.7 - 5.0).collect();
        let mut swma = SineWeightedMa::new(7).unwrap();
        let got = swma.batch(&prices);
        let want = swma_naive(&prices, 7);
        for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
            assert_eq!(g.is_some(), w.is_some(), "warmup mismatch at index {i}");
            if let (Some(a), Some(b)) = (g, w) {
                assert_relative_eq!(*a, *b, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut swma = SineWeightedMa::new(4).unwrap();
        swma.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(swma.is_ready());
        swma.reset();
        assert!(!swma.is_ready());
        assert_eq!(swma.update(10.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=20).map(|i| f64::from(i) * 0.5).collect();
        let mut a = SineWeightedMa::new(5).unwrap();
        let mut b = SineWeightedMa::new(5).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ignores_non_finite_input_but_keeps_state() {
        let mut swma = SineWeightedMa::new(3).unwrap();
        swma.update(1.0);
        swma.update(2.0);
        swma.update(3.0).expect("SWMA(3) ready after three inputs");
        assert_eq!(swma.update(f64::NAN), None);
        assert_eq!(swma.update(f64::INFINITY), None);
        // The window still holds 1, 2, 3 -> next real input slides it to 2, 3, 4.
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let total = s + 1.0 + s;
        let want = (s * 2.0 + 1.0 * 3.0 + s * 4.0) / total;
        assert_relative_eq!(swma.update(4.0).unwrap(), want, epsilon = 1e-12);
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(48))]
        #[test]
        fn proptest_matches_naive(
            period in 1usize..15,
            prices in proptest::collection::vec(-500.0_f64..500.0, 0..120),
        ) {
            let mut swma = SineWeightedMa::new(period).unwrap();
            let got = swma.batch(&prices);
            let want = swma_naive(&prices, period);
            proptest::prop_assert_eq!(got.len(), want.len());
            for (g, w) in got.iter().zip(want.iter()) {
                match (g, w) {
                    (None, None) => {}
                    (Some(a), Some(b)) => proptest::prop_assert!(
                        (a - b).abs() < 1e-7,
                        "got={a} want={b}"
                    ),
                    _ => proptest::prop_assert!(false, "warmup mismatch"),
                }
            }
        }
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            SineWeightedMa::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
