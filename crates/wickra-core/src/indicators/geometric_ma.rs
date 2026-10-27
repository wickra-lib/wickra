//! Geometric Moving Average (GMA).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::traits::Indicator;

/// Geometric Moving Average — the rolling geometric mean of the last `period`
/// inputs.
///
/// ```text
/// GMA = (Π value_i)^(1/period) = exp( (1/period) · Σ ln(value_i) )
/// ```
///
/// The geometric mean is the natural average for *multiplicative* quantities
/// such as prices and growth factors: averaging in log-space weights relative
/// (percentage) moves symmetrically, so a `+10%` followed by a `−10%` move
/// pulls the average below the start, exactly as compounded returns do. It is
/// always less than or equal to the arithmetic mean of the same window.
///
/// Maintained incrementally in O(1): the running sum of natural logs is updated
/// by adding the newcomer's log and subtracting the departing value's log as
/// the window slides.
///
/// The geometric mean is only defined for **strictly positive** inputs. A
/// non-finite or non-positive input is ignored (it leaves the window unchanged
/// and returns the current value), mirroring the non-finite handling of the
/// other moving averages.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, GeometricMa};
///
/// let mut indicator = GeometricMa::new(5).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct GeometricMa {
    period: usize,
    /// Natural logs of the values currently in the window (oldest at front).
    logs: VecDeque<f64>,
    sum_logs: RollingSum,
}

impl GeometricMa {
    /// Construct a new geometric moving average over `period` inputs.
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
            logs: VecDeque::with_capacity(period),
            sum_logs: RollingSum::new(),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if the window is full.
    pub fn value(&self) -> Option<f64> {
        if self.logs.len() == self.period {
            Some((self.sum_logs.value() / self.period as f64).exp())
        } else {
            None
        }
    }
}

impl Indicator for GeometricMa {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() || input <= 0.0 {
            return None;
        }
        if self.logs.len() == self.period {
            let oldest = self.logs.pop_front().expect("window non-empty");
            self.sum_logs.evict(oldest);
        }
        let ln = input.ln();
        self.logs.push_back(ln);
        self.sum_logs.push(ln);
        if self.sum_logs.needs_reseed(self.period) {
            self.sum_logs.reseed(self.logs.iter().copied());
        }
        self.value()
    }

    fn reset(&mut self) {
        self.logs.clear();
        self.sum_logs.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.logs.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "GMA"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Reference implementation: explicit geometric mean over a window.
    fn gma_naive(prices: &[f64], period: usize) -> Vec<Option<f64>> {
        prices
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if i + 1 < period {
                    None
                } else {
                    let window = &prices[i + 1 - period..=i];
                    let product: f64 = window.iter().product();
                    Some(product.powf(1.0 / period as f64))
                }
            })
            .collect()
    }

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(GeometricMa::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessor `period` and the Indicator-impl `warmup_period`
    /// + `name`.
    #[test]
    fn accessors_and_metadata() {
        let gma = GeometricMa::new(7).unwrap();
        assert_eq!(gma.period(), 7);
        assert_eq!(gma.warmup_period(), 7);
        assert_eq!(gma.name(), "GMA");
    }

    #[test]
    fn warmup_returns_none() {
        let mut gma = GeometricMa::new(3).unwrap();
        assert_eq!(gma.update(1.0), None);
        assert_eq!(gma.update(4.0), None);
        // GMA(3) of [1, 4, 2] = (1·4·2)^(1/3) = 8^(1/3) = 2.
        assert_relative_eq!(gma.update(2.0).unwrap(), 2.0, epsilon = 1e-12);
    }

    #[test]
    fn known_value_period_2() {
        // GMA(2) of [4, 9] = sqrt(36) = 6.
        let mut gma = GeometricMa::new(2).unwrap();
        let v = gma.batch(&[4.0, 9.0]);
        assert_relative_eq!(v[1].unwrap(), 6.0, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_returns_the_constant() {
        let mut gma = GeometricMa::new(5).unwrap();
        for v in gma.batch(&[42.0; 20]).into_iter().flatten() {
            assert_relative_eq!(v, 42.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn period_one_is_pass_through() {
        let mut gma = GeometricMa::new(1).unwrap();
        assert_relative_eq!(gma.update(5.5).unwrap(), 5.5, epsilon = 1e-12);
        assert_relative_eq!(gma.update(7.5).unwrap(), 7.5, epsilon = 1e-12);
    }

    #[test]
    fn below_or_equal_arithmetic_mean() {
        // The geometric mean never exceeds the arithmetic mean of the same set.
        let mut gma = GeometricMa::new(4).unwrap();
        let prices = [10.0, 20.0, 5.0, 40.0];
        let g = gma.batch(&prices)[3].unwrap();
        let arithmetic = prices.iter().sum::<f64>() / 4.0;
        assert!(
            g < arithmetic,
            "geometric {g} should be below arithmetic {arithmetic}"
        );
    }

    #[test]
    fn matches_naive_over_inputs() {
        let prices: Vec<f64> = (1..=30).map(|i| f64::from(i) * 1.7 + 1.0).collect();
        let mut gma = GeometricMa::new(7).unwrap();
        let got = gma.batch(&prices);
        let want = gma_naive(&prices, 7);
        for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
            assert_eq!(g.is_some(), w.is_some(), "warmup mismatch at index {i}");
            if let (Some(a), Some(b)) = (g, w) {
                assert_relative_eq!(*a, *b, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut gma = GeometricMa::new(4).unwrap();
        gma.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(gma.is_ready());
        gma.reset();
        assert!(!gma.is_ready());
        assert_eq!(gma.update(10.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=20).map(|i| f64::from(i) * 0.5 + 1.0).collect();
        let mut a = GeometricMa::new(5).unwrap();
        let mut b = GeometricMa::new(5).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ignores_non_finite_and_non_positive_input() {
        let mut gma = GeometricMa::new(3).unwrap();
        gma.update(1.0);
        gma.update(4.0);
        gma.update(2.0).expect("GMA(3) ready after three inputs");
        // Non-finite and non-positive inputs are skipped (geometric mean needs
        // strictly positive values) and the window is left unchanged.
        assert_eq!(gma.update(f64::NAN), None);
        assert_eq!(gma.update(0.0), None);
        assert_eq!(gma.update(-3.0), None);
        // The window still holds 1, 4, 2 -> next real input slides it to 4, 2, 16.
        let want = (4.0_f64 * 2.0 * 16.0).powf(1.0 / 3.0);
        assert_relative_eq!(gma.update(16.0).unwrap(), want, epsilon = 1e-9);
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(48))]
        #[test]
        fn proptest_matches_naive(
            period in 1usize..15,
            prices in proptest::collection::vec(0.01_f64..1000.0, 0..120),
        ) {
            let mut gma = GeometricMa::new(period).unwrap();
            let got = gma.batch(&prices);
            let want = gma_naive(&prices, period);
            proptest::prop_assert_eq!(got.len(), want.len());
            for (g, w) in got.iter().zip(want.iter()) {
                match (g, w) {
                    (None, None) => {}
                    (Some(a), Some(b)) => proptest::prop_assert!(
                        (a - b).abs() <= 1e-6 * b.abs().max(1.0),
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
            GeometricMa::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
