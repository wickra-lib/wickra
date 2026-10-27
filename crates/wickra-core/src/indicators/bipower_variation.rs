//! Realized Bipower Variation — a jump-robust quadratic-variation estimator.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::traits::Indicator;

/// Realized Bipower Variation — the sum of *adjacent* absolute log-return
/// products over the trailing `period` returns, scaled to estimate integrated
/// variance.
///
/// ```text
/// r_t = ln(price_t / price_{t−1})
/// BV  = (π / 2) · Σ |r_t| · |r_{t−1}|   over the window
/// ```
///
/// Bipower variation (Barndorff-Nielsen & Shephard 2004) estimates the same
/// integrated variance as [`RealizedVolatility`](crate::RealizedVolatility)'s
/// `Σ r²`, but by multiplying *neighbouring* absolute returns rather than
/// squaring a single one. A price jump inflates exactly one return; because that
/// return appears in a product with its (ordinary) neighbour rather than squared,
/// its contribution stays bounded — so `BV` is **robust to jumps** while realized
/// variance is not. The constant `π / 2 = μ₁⁻²` (with `μ₁ = E|Z| = √(2/π)` for a
/// standard normal) debiases the product of two half-normal magnitudes back to a
/// variance scale.
///
/// The output is on the **variance** scale (the jump-robust counterpart of
/// realized *variance*, not volatility); take its square root for a volatility,
/// and compare `RV − BV` to isolate the jump contribution. A window of `period`
/// returns contributes `period − 1` adjacent products; each `update` is O(1) via
/// a running sum.
///
/// Non-finite and non-positive prices are ignored (the log return would be
/// undefined): the tick is dropped, state is left untouched, and the last value
/// is returned.
///
/// # Example
///
/// ```
/// use wickra_core::{BipowerVariation, Indicator};
///
/// let mut indicator = BipowerVariation::new(20).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct BipowerVariation {
    period: usize,
    prev_price: Option<f64>,
    /// Rolling window of the last `period` log returns.
    window: VecDeque<f64>,
    /// Running sum of adjacent absolute-return products inside the window.
    sum_adjacent: RollingSum,
    last: Option<f64>,
}

impl BipowerVariation {
    /// Construct a new bipower-variation indicator.
    ///
    /// `period` is the number of log returns in the rolling window; the estimate
    /// uses the `period − 1` adjacent products between them.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0`, or
    /// [`Error::InvalidPeriod`] if `period == 1` (an adjacent product needs at
    /// least two returns).
    pub fn new(period: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "bipower variation period must be >= 2",
            });
        }
        Ok(Self {
            period,
            prev_price: None,
            window: VecDeque::with_capacity(period),
            sum_adjacent: RollingSum::new(),
            last: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

/// `μ₁⁻² = π / 2`, the debiasing constant for a product of half-normal returns.
const MU1_INV_SQ: f64 = std::f64::consts::FRAC_PI_2;

impl Indicator for BipowerVariation {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        // Non-finite / non-positive prices are skipped: `ln(input / prev)` is
        // undefined, so the tick must not enter the return window.
        if !input.is_finite() || input <= 0.0 {
            return None;
        }
        let Some(prev) = self.prev_price else {
            self.prev_price = Some(input);
            return None;
        };
        self.prev_price = Some(input);
        // `prev` came from `self.prev_price`, gated by the guard above, so it is
        // finite and positive — the log return is always well-defined.
        let r = (input / prev).ln();
        // The incoming return forms a product with the current last return.
        if let Some(&back) = self.window.back() {
            self.sum_adjacent.push(back.abs() * r.abs());
        }
        self.window.push_back(r);
        if self.window.len() > self.period {
            let first = self.window.pop_front().expect("window is non-empty");
            // The product between the dropped return and the new front leaves.
            let second = *self.window.front().expect("window still has >= 1 element");
            self.sum_adjacent.evict(first.abs() * second.abs());
            if self.sum_adjacent.needs_reseed(self.period) {
                self.sum_adjacent.reseed(
                    self.window
                        .iter()
                        .zip(self.window.iter().skip(1))
                        .map(|(a, b)| a.abs() * b.abs()),
                );
            }
        }
        if self.window.len() < self.period {
            return None;
        }
        // Products are non-negative; the rolling subtraction can leave a tiny
        // negative residual when returns are ~0, so clamp before scaling.
        let bv = MU1_INV_SQ * self.sum_adjacent.value().max(0.0);
        self.last = Some(bv);
        Some(bv)
    }

    fn reset(&mut self) {
        self.prev_price = None;
        self.window.clear();
        self.sum_adjacent.reset();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // The first log return needs a previous price, then the window fills.
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "BipowerVariation"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(BipowerVariation::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn rejects_period_one() {
        assert!(matches!(
            BipowerVariation::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let bv = BipowerVariation::new(20).unwrap();
        assert_eq!(bv.period(), 20);
        assert_eq!(bv.warmup_period(), 21);
        assert_eq!(bv.name(), "BipowerVariation");
        assert!(!bv.is_ready());
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut bv = BipowerVariation::new(5).unwrap();
        let out = bv.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        for v in out.iter().take(5) {
            assert!(v.is_none());
        }
        assert!(out[5].is_some());
    }

    #[test]
    fn known_value() {
        // period = 2: one adjacent product. r1 = ln(1.1), r2 = ln(0.9).
        // BV = (π/2)·|r1|·|r2|.
        let mut bv = BipowerVariation::new(2).unwrap();
        let out = bv.batch(&[100.0, 110.0, 99.0]);
        assert!(out[1].is_none());
        let r1 = (110.0_f64 / 100.0).ln();
        let r2 = (99.0_f64 / 110.0).ln();
        let expected = std::f64::consts::FRAC_PI_2 * r1.abs() * r2.abs();
        assert_relative_eq!(out[2].unwrap(), expected, epsilon = 1e-12);
    }

    #[test]
    fn rolling_window_drops_oldest_product() {
        // period = 2, four prices -> two emissions, each a single product.
        let mut bv = BipowerVariation::new(2).unwrap();
        let out = bv.batch(&[100.0, 110.0, 99.0, 105.0]);
        let r2 = (99.0_f64 / 110.0).ln();
        let r3 = (105.0_f64 / 99.0).ln();
        let expected = std::f64::consts::FRAC_PI_2 * r2.abs() * r3.abs();
        assert_relative_eq!(out[3].unwrap(), expected, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut bv = BipowerVariation::new(10).unwrap();
        for v in bv.batch(&[100.0; 40]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn output_is_non_negative() {
        let mut bv = BipowerVariation::new(20).unwrap();
        let prices: Vec<f64> = (1..=200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 12.0)
            .collect();
        for v in bv.batch(&prices).into_iter().flatten() {
            assert!(v >= 0.0, "bipower variation must be non-negative, got {v}");
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut bv = BipowerVariation::new(5).unwrap();
        let out = bv.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        let last = *out.last().unwrap();
        assert!(last.is_some());
        assert_eq!(bv.update(f64::NAN), None);
        assert_eq!(bv.update(f64::INFINITY), None);
    }

    #[test]
    fn skips_non_positive_prices() {
        let mut bv = BipowerVariation::new(5).unwrap();
        let warmup = bv.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        warmup.last().copied().flatten().expect("warmed up");
        assert_eq!(bv.update(-5.0), None);
        assert_eq!(bv.update(0.0), None);
        // State untouched: a clone advanced by the same real tick agrees.
        let mut control = bv.clone();
        let after = bv.update(21.0).expect("ready");
        assert_eq!(control.update(21.0).expect("ready"), after);
    }

    #[test]
    fn reset_clears_state() {
        let mut bv = BipowerVariation::new(5).unwrap();
        bv.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        assert!(bv.is_ready());
        bv.reset();
        assert!(!bv.is_ready());
        assert_eq!(bv.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let batch = BipowerVariation::new(20).unwrap().batch(&prices);
        let mut b = BipowerVariation::new(20).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            BipowerVariation::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
