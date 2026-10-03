//! Triple Exponential Moving Average (TEMA).

use crate::error::Result;
use crate::indicators::ema::Ema;
use crate::traits::Indicator;

/// Triple Exponential Moving Average: `3 * EMA1 - 3 * EMA2 + EMA3`,
/// where `EMA2 = EMA(EMA1)` and `EMA3 = EMA(EMA2)`.
///
/// Reduces lag further than DEMA at the cost of more responsiveness to noise.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Tema};
///
/// let mut indicator = Tema::new(3).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Tema {
    ema1: Ema,
    ema2: Ema,
    ema3: Ema,
    period: usize,
}

impl Tema {
    /// # Errors
    /// Returns [`crate::Error::PeriodZero`] if `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
        Ok(Self {
            ema1: Ema::new(period)?,
            ema2: Ema::new(period)?,
            ema3: Ema::new(period)?,
            period,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Tema {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        let e1 = self.ema1.update(input)?;
        let e2 = self.ema2.update(e1)?;
        let e3 = self.ema3.update(e2)?;
        Some(3.0 * e1 - 3.0 * e2 + e3)
    }

    fn reset(&mut self) {
        self.ema1.reset();
        self.ema2.reset();
        self.ema3.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        3 * self.period - 2
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.ema3.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TEMA"
    }

    /// SIMD kernel: like DEMA's, with a third chained EMA scan. Agrees with the
    /// exact batch to within a few units in the last place; warmup `NaN`s and
    /// length are identical. Afterwards the three EMAs continue streaming from
    /// the kernel's last values.
    fn batch_fast_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        assert_eq!(
            inputs.len(),
            out.len(),
            "batch output length must equal input length"
        );
        let p = self.period;
        if !(self.ema1.is_fresh() && self.ema2.is_fresh() && self.ema3.is_fresh())
            || inputs.len() < 3 * p - 2
            || !crate::fast::in_range(inputs)
        {
            self.batch_nan_into(inputs, out);
            return;
        }
        let (e1, e2, e3) = wickra_simd::dispatch(crate::fast::TemaFast {
            x: inputs,
            period: p,
            alpha: self.ema1.alpha(),
            out,
            _borrow: std::marker::PhantomData,
        });
        self.ema1.seed_to(e1);
        self.ema2.seed_to(e2);
        self.ema3.seed_to(e3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn constant_series_yields_constant_tema() {
        let mut tema = Tema::new(5).unwrap();
        let out = tema.batch(&[42.0_f64; 80]);
        let last = out.iter().rev().flatten().next().unwrap();
        assert_relative_eq!(*last, 42.0, epsilon = 1e-9);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=80)
            .map(|i| (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        let mut a = Tema::new(5).unwrap();
        let mut b = Tema::new(5).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut tema = Tema::new(5).unwrap();
        tema.batch(&(1..=80).map(f64::from).collect::<Vec<_>>());
        assert!(tema.is_ready());
        tema.reset();
        assert!(!tema.is_ready());
    }

    #[test]
    fn rejects_zero_period() {
        assert!(Tema::new(0).is_err());
    }

    /// Cover the const accessor `period` (45-47) and the Indicator-impl
    /// `warmup_period` (67-69) + `name` (75-77). Existing tests inspect
    /// TEMA output but never query the metadata.
    #[test]
    fn accessors_and_metadata() {
        let tema = Tema::new(5).unwrap();
        assert_eq!(tema.period(), 5);
        // EMA1 seeds at period (5), each cascade stage needs another (period-1) inputs.
        assert_eq!(tema.warmup_period(), 3 * 5 - 2);
        assert_eq!(tema.name(), "TEMA");
    }
}
