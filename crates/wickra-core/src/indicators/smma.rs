//! Smoothed Moving Average (Wilder's RMA).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Smoothed Moving Average — Wilder's running moving average, also known as
/// RMA.
///
/// Seeded with the simple average of the first `period` inputs, then advanced
/// by `SMMA_t = (SMMA_{t-1} * (period - 1) + price_t) / period`. This is an
/// exponential average with a slow `1 / period` smoothing factor and is the
/// average underlying Wilder's RSI and ATR. The first output lands after
/// exactly `period` inputs.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Smma};
///
/// let mut indicator = Smma::new(3).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Smma {
    period: usize,
    /// Inputs collected while seeding (before the first value is produced).
    seed: VecDeque<f64>,
    seed_sum: f64,
    current: Option<f64>,
}

impl Smma {
    /// Construct a new SMMA with the given period.
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
            seed: VecDeque::with_capacity(period),
            seed_sum: 0.0,
            current: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.current
    }
}

impl Indicator for Smma {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            // Non-finite input is ignored, leaving state untouched.
            return None;
        }
        if let Some(prev) = self.current {
            let period = self.period as f64;
            self.current = Some((prev * (period - 1.0) + input) / period);
        } else {
            self.seed.push_back(input);
            self.seed_sum += input;
            if self.seed.len() == self.period {
                self.current = Some(self.seed_sum / self.period as f64);
            }
        }
        self.current
    }

    fn reset(&mut self) {
        self.seed.clear();
        self.seed_sum = 0.0;
        self.current = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.current.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "SMMA"
    }

    /// SIMD kernel: the same seed mean, then Wilder's smoothing
    /// `y = ((period − 1) / period) · y + x / period` as a linear-recurrence
    /// scan. Agrees with the exact batch to within a few units in the last
    /// place (the division per step becomes two multiplies, and the scan
    /// reassociates the recurrence); warmup `NaN`s and length are identical.
    /// Afterwards the SMMA continues streaming from the kernel's last value.
    fn batch_fast_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        assert_eq!(
            inputs.len(),
            out.len(),
            "batch output length must equal input length"
        );
        let p = self.period;
        if !self.seed.is_empty()
            || self.current.is_some()
            || inputs.len() < p
            || !crate::fast::in_range(inputs)
        {
            self.batch_nan_into(inputs, out);
            return;
        }
        let seed_sum = inputs[..p].iter().fold(0.0, |acc, &x| acc + x);
        let last = wickra_simd::dispatch(crate::fast::SmmaFast {
            x: inputs,
            period: p,
            seed_sum,
            out,
            _borrow: std::marker::PhantomData,
        });
        self.seed.extend(&inputs[..p]);
        self.seed_sum = seed_sum;
        self.current = Some(last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(Smma::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `value` and the Indicator-impl
    /// `warmup_period` / `name` methods. Existing tests only exercise the
    /// numeric output of `update` / `batch` / `reset`, never query the
    /// metadata surface.
    #[test]
    fn accessors_and_metadata() {
        let mut smma = Smma::new(7).unwrap();
        assert_eq!(smma.period(), 7);
        assert_eq!(smma.warmup_period(), 7);
        assert_eq!(smma.name(), "SMMA");
        // value() must report both the pre-warmup None and post-warmup Some branches.
        assert_eq!(smma.value(), None);
        for i in 1..=7 {
            smma.update(f64::from(i));
        }
        assert!(smma.value().is_some());
    }

    #[test]
    fn warmup_then_recurrence() {
        // SMMA(3): seed = SMA(1,2,3) = 2.0; then (prev*2 + x) / 3.
        let mut smma = Smma::new(3).unwrap();
        assert_eq!(smma.update(1.0), None);
        assert_eq!(smma.update(2.0), None);
        assert_eq!(smma.update(3.0), Some(2.0));
        assert_relative_eq!(
            smma.update(4.0).unwrap(),
            (2.0 * 2.0 + 4.0) / 3.0,
            epsilon = 1e-12
        );
        assert_relative_eq!(
            smma.update(5.0).unwrap(),
            ((2.0 * 2.0 + 4.0) / 3.0 * 2.0 + 5.0) / 3.0,
            epsilon = 1e-12
        );
    }

    #[test]
    fn period_one_is_pass_through() {
        let mut smma = Smma::new(1).unwrap();
        assert_eq!(smma.update(5.0), Some(5.0));
        assert_eq!(smma.update(10.0), Some(10.0));
    }

    #[test]
    fn constant_series_yields_the_constant() {
        let mut smma = Smma::new(5).unwrap();
        let out = smma.batch(&[7.0; 20]);
        for x in out.iter().skip(4) {
            assert_relative_eq!(x.unwrap(), 7.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut smma = Smma::new(3).unwrap();
        smma.batch(&[1.0, 2.0, 3.0]);
        assert_eq!(smma.update(f64::NAN), None);
        assert_eq!(smma.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut smma = Smma::new(3).unwrap();
        smma.batch(&[1.0, 2.0, 3.0, 4.0]);
        assert!(smma.is_ready());
        smma.reset();
        assert!(!smma.is_ready());
        assert_eq!(smma.update(10.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=30).map(f64::from).collect();
        let batch = Smma::new(7).unwrap().batch(&prices);
        let mut b = Smma::new(7).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }
}
