//! Triangular Moving Average.

use crate::error::{Error, Result};
use crate::traits::Indicator;

use super::Sma;

/// Triangular Moving Average — a simple moving average applied twice, which
/// triangular-weights the window so the middle bars carry the most weight and
/// the edges the least.
///
/// For period `n` the two stacked SMAs use lengths `n1` and `n2`:
/// an odd `n` uses `n1 = n2 = (n + 1) / 2`; an even `n` uses `n1 = n / 2` and
/// `n2 = n / 2 + 1`. Either way the first output lands after exactly `n`
/// inputs.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Trima};
///
/// let mut indicator = Trima::new(5).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Trima {
    period: usize,
    inner: Sma,
    outer: Sma,
}

impl Trima {
    /// Construct a new TRIMA with the given period.
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
        let (n1, n2) = if period % 2 == 1 {
            (period.div_ceil(2), period.div_ceil(2))
        } else {
            (period / 2, period / 2 + 1)
        };
        Ok(Self {
            period,
            inner: Sma::new(n1)?,
            outer: Sma::new(n2)?,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub fn value(&self) -> Option<f64> {
        self.outer.value()
    }
}

impl Indicator for Trima {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            // Non-finite input is ignored; do not double-feed the inner SMA's
            // stale value into the outer SMA.
            return None;
        }
        // Genuine stacking: the outer SMA consumes the inner SMA's output.
        match self.inner.update(input) {
            Some(v) => self.outer.update(v),
            None => None,
        }
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.outer.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.outer.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TRIMA"
    }

    /// SIMD kernel: the inner and outer SMAs as the rolling-sum prefix-scan
    /// kernel. Agrees with the exact batch to within a few units in the last
    /// place; warmup `NaN`s and length are identical. TRIMA only remembers its
    /// last `inner + outer − 1` inputs, so afterwards its state is rebuilt
    /// exactly by replaying them.
    fn batch_fast_into(&mut self, inputs: &[f64], out: &mut [f64]) {
        assert_eq!(
            inputs.len(),
            out.len(),
            "batch output length must equal input length"
        );
        let (inner, outer) = (self.inner.period(), self.outer.period());
        let n = inputs.len();
        let span = inner + outer - 1;
        if !(self.inner.is_fresh() && self.outer.is_fresh())
            || n < span
            || !crate::fast::in_range(inputs)
        {
            self.batch_nan_into(inputs, out);
            return;
        }
        crate::fast::with_scratch(n, |tmp| {
            wickra_simd::dispatch(crate::fast::TrimaFast {
                x: inputs,
                inner,
                outer,
                tmp,
                out,
                _borrow: std::marker::PhantomData,
            });
        });
        crate::fast::replay_tail(self, &inputs[n - span..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(Trima::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `value` (59-66) and the
    /// Indicator-impl `name` body (99-101). Existing tests inspect
    /// TRIMA output but never query the metadata.
    #[test]
    fn accessors_and_metadata() {
        let mut t = Trima::new(5).unwrap();
        assert_eq!(t.period(), 5);
        assert_eq!(t.name(), "TRIMA");
        assert_eq!(t.value(), None);
        for i in 1..=t.warmup_period() {
            t.update(f64::from(u32::try_from(i).unwrap()));
        }
        assert!(t.value().is_some());
    }

    #[test]
    fn odd_period_reference_values() {
        // TRIMA(5) is SMA(3) of SMA(3).
        // SMA(3) of 1..=7 -> [_,_,2,3,4,5,6]; SMA(3) of that -> [_,_,_,_,3,4,5].
        let mut trima = Trima::new(5).unwrap();
        let out = trima.batch(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
        assert_eq!(out[0], None);
        assert_eq!(out[3], None);
        assert_relative_eq!(out[4].unwrap(), 3.0, epsilon = 1e-12);
        assert_relative_eq!(out[5].unwrap(), 4.0, epsilon = 1e-12);
        assert_relative_eq!(out[6].unwrap(), 5.0, epsilon = 1e-12);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        // Even period: TRIMA(6) -> SMA(3) of SMA(4); first value at input 6.
        let mut trima = Trima::new(6).unwrap();
        let out = trima.batch(&(1..=10).map(f64::from).collect::<Vec<_>>());
        assert_eq!(trima.warmup_period(), 6);
        for v in out.iter().take(5) {
            assert!(v.is_none());
        }
        assert!(out[5].is_some());
    }

    #[test]
    fn constant_series_yields_the_constant() {
        let mut trima = Trima::new(7).unwrap();
        let out = trima.batch(&[42.0; 20]);
        for x in out.iter().skip(6) {
            assert_relative_eq!(x.unwrap(), 42.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut trima = Trima::new(5).unwrap();
        let ready = trima.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let last = ready[4];
        assert!(last.is_some());
        assert_eq!(trima.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut trima = Trima::new(5).unwrap();
        trima.batch(&(1..=10).map(f64::from).collect::<Vec<_>>());
        assert!(trima.is_ready());
        trima.reset();
        assert!(!trima.is_ready());
        assert_eq!(trima.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=40).map(f64::from).collect();
        let batch = Trima::new(8).unwrap().batch(&prices);
        let mut b = Trima::new(8).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }
}
