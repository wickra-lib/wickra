//! Realized Volatility from the sum of squared log returns.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::traits::Indicator;

/// Realized Volatility — the square root of the sum of squared log returns over
/// the trailing `period` bars.
///
/// ```text
/// r_t = ln(price_t / price_{t−1})
/// RV  = √( Σ r_t²  over the last `period` returns )
/// ```
///
/// Unlike [`HistoricalVolatility`](crate::HistoricalVolatility) — which reports
/// the *annualised sample standard deviation* of log returns (mean-centred,
/// divided by `n − 1`, scaled by `√trading_periods` and ×100) — realized
/// volatility is the **raw, un-centred, un-annualised** quadratic variation
/// estimator used in high-frequency econometrics. It makes no Gaussian
/// assumption and no mean subtraction: it simply accumulates squared returns,
/// which converges to the integrated variance of the price path as the
/// sampling frequency rises. Multiply by `√trading_periods` yourself if an
/// annual figure is wanted.
///
/// Non-finite and non-positive prices are ignored (the log return would be
/// undefined): the tick is dropped, state is left untouched, and the last
/// value is returned.
///
/// Each `update` is O(1): a running sum of squared returns is maintained over
/// the rolling window.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, RealizedVolatility};
///
/// let mut indicator = RealizedVolatility::new(20).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct RealizedVolatility {
    period: usize,
    prev_price: Option<f64>,
    /// Rolling window of the last `period` log returns.
    window: VecDeque<f64>,
    sum_sq: RollingSum,
    last: Option<f64>,
}

impl RealizedVolatility {
    /// Construct a new realized-volatility indicator.
    ///
    /// `period` is the number of squared log returns accumulated in the window.
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
            prev_price: None,
            window: VecDeque::with_capacity(period),
            sum_sq: RollingSum::new(),
            last: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for RealizedVolatility {
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
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("window is non-empty");
            self.sum_sq.evict(old * old);
        }
        self.window.push_back(r);
        self.sum_sq.push(r * r);
        if self.sum_sq.needs_reseed(self.period) {
            self.sum_sq.reseed(self.window.iter().map(|v| v * v));
        }
        if self.window.len() < self.period {
            return None;
        }
        // Floating-point subtraction in the rolling sum can leave a tiny
        // negative residual when every return is ~0; clamp before the sqrt.
        let rv = self.sum_sq.value().max(0.0).sqrt();
        self.last = Some(rv);
        Some(rv)
    }

    fn reset(&mut self) {
        self.prev_price = None;
        self.window.clear();
        self.sum_sq.reset();
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
        "RealizedVolatility"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(RealizedVolatility::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let rv = RealizedVolatility::new(20).unwrap();
        assert_eq!(rv.period(), 20);
        assert_eq!(rv.warmup_period(), 21);
        assert_eq!(rv.name(), "RealizedVolatility");
        assert!(!rv.is_ready());
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut rv = RealizedVolatility::new(5).unwrap();
        let out = rv.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        for v in out.iter().take(5) {
            assert!(v.is_none());
        }
        assert!(out[5].is_some());
    }

    #[test]
    fn known_value() {
        // Two equal +10% steps: r = ln(1.1) each. RV = √(2·ln(1.1)²).
        let mut rv = RealizedVolatility::new(2).unwrap();
        let out = rv.batch(&[100.0, 110.0, 121.0]);
        let expected = (2.0 * (1.1_f64).ln().powi(2)).sqrt();
        assert_relative_eq!(out[2].unwrap(), expected, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut rv = RealizedVolatility::new(10).unwrap();
        for v in rv.batch(&[100.0; 40]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn output_is_non_negative() {
        let mut rv = RealizedVolatility::new(20).unwrap();
        let prices: Vec<f64> = (1..=200)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 12.0)
            .collect();
        for v in rv.batch(&prices).into_iter().flatten() {
            assert!(
                v >= 0.0,
                "realized volatility must be non-negative, got {v}"
            );
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut rv = RealizedVolatility::new(5).unwrap();
        let out = rv.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        let last = *out.last().unwrap();
        assert!(last.is_some());
        assert_eq!(rv.update(f64::NAN), None);
        assert_eq!(rv.update(f64::INFINITY), None);
    }

    #[test]
    fn skips_non_positive_prices() {
        let mut rv = RealizedVolatility::new(5).unwrap();
        let warmup = rv.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        warmup.last().copied().flatten().expect("warmed up");
        assert_eq!(rv.update(-5.0), None);
        assert_eq!(rv.update(0.0), None);
        // State untouched: a clone advanced by the same real tick agrees.
        let mut control = rv.clone();
        let after = rv.update(21.0).expect("ready");
        assert_eq!(control.update(21.0).expect("ready"), after);
    }

    #[test]
    fn reset_clears_state() {
        let mut rv = RealizedVolatility::new(5).unwrap();
        rv.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        assert!(rv.is_ready());
        rv.reset();
        assert!(!rv.is_ready());
        assert_eq!(rv.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let batch = RealizedVolatility::new(20).unwrap().batch(&prices);
        let mut b = RealizedVolatility::new(20).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            RealizedVolatility::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
