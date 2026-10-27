//! Tillson T3 Moving Average.

use crate::error::{Error, Result};
use crate::traits::Indicator;

use super::Ema;

/// Tillson's T3 — a six-fold cascaded EMA recombined with a *volume factor* `v`.
///
/// T3 is the generalised DEMA applied three times. Tim Tillson's expansion of
/// that triple application over six chained EMAs (`e1 … e6`, each of the same
/// `period`) gives the closed form used here:
///
/// ```text
/// c1 = −v³
/// c2 = 3v² + 3v³
/// c3 = −6v² − 3v − 3v³
/// c4 = 1 + 3v + v³ + 3v²
/// T3 = c1·e6 + c2·e5 + c3·e4 + c4·e3
/// ```
///
/// The volume factor `v ∈ [0, 1]` controls the lag/smoothness trade-off:
/// `v = 0` collapses T3 to the plain triple-cascaded EMA `e3`, while the
/// conventional `v = 0.7` adds a hump that sharpens the response to turns.
/// The coefficients always sum to `1`, so a constant series maps to itself.
///
/// The first output lands after `6·period − 5` inputs — the index at which the
/// sixth cascaded EMA seeds.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, T3};
///
/// let mut indicator = T3::new(5, 0.7).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct T3 {
    period: usize,
    v: f64,
    c1: f64,
    c2: f64,
    c3: f64,
    c4: f64,
    e1: Ema,
    e2: Ema,
    e3: Ema,
    e4: Ema,
    e5: Ema,
    e6: Ema,
    current: Option<f64>,
}

impl T3 {
    /// Construct a new T3 with the given `period` and volume factor `v`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period == 0`, or
    /// [`Error::InvalidPeriod`] if `v` is non-finite or outside `[0.0, 1.0]`.
    pub fn new(period: usize, v: f64) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !v.is_finite() || !(0.0..=1.0).contains(&v) {
            return Err(Error::InvalidPeriod {
                message: "T3 volume factor must be a finite value in [0.0, 1.0]",
            });
        }
        let v2 = v * v;
        let v3 = v2 * v;
        Ok(Self {
            period,
            v,
            c1: -v3,
            c2: 3.0 * v2 + 3.0 * v3,
            c3: -6.0 * v2 - 3.0 * v - 3.0 * v3,
            c4: 1.0 + 3.0 * v + v3 + 3.0 * v2,
            e1: Ema::new(period)?,
            e2: Ema::new(period)?,
            e3: Ema::new(period)?,
            e4: Ema::new(period)?,
            e5: Ema::new(period)?,
            e6: Ema::new(period)?,
            current: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Configured volume factor `v`.
    pub const fn volume_factor(&self) -> f64 {
        self.v
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.current
    }
}

impl Indicator for T3 {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            // Non-finite input is ignored; the cascade is not advanced.
            return None;
        }
        let e1 = self.e1.update(input)?;
        let e2 = self.e2.update(e1)?;
        let e3 = self.e3.update(e2)?;
        let e4 = self.e4.update(e3)?;
        let e5 = self.e5.update(e4)?;
        let e6 = self.e6.update(e5)?;
        let out = self.c1 * e6 + self.c2 * e5 + self.c3 * e4 + self.c4 * e3;
        self.current = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.e1.reset();
        self.e2.reset();
        self.e3.reset();
        self.e4.reset();
        self.e5.reset();
        self.e6.reset();
        self.current = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        6 * self.period - 5
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.current.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "T3"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(T3::new(0, 0.7), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `volume_factor` / `value` and
    /// the Indicator-impl `name` (lines 95-107, 148-150). Existing tests
    /// query `warmup_period` (covered by `first_emission_at_warmup_period`)
    /// but never inspect period, v, value, or name.
    #[test]
    fn accessors_and_metadata() {
        let mut t3 = T3::new(5, 0.7).unwrap();
        assert_eq!(t3.period(), 5);
        assert_relative_eq!(t3.volume_factor(), 0.7, epsilon = 1e-12);
        assert_eq!(t3.name(), "T3");
        assert_eq!(t3.value(), None);
        for _ in 0..t3.warmup_period() {
            t3.update(50.0);
        }
        assert!(t3.value().is_some());
    }

    #[test]
    fn new_rejects_out_of_range_volume_factor() {
        assert!(matches!(T3::new(5, -0.1), Err(Error::InvalidPeriod { .. })));
        assert!(matches!(T3::new(5, 1.5), Err(Error::InvalidPeriod { .. })));
        assert!(matches!(
            T3::new(5, f64::NAN),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(T3::new(5, 0.0).is_ok());
        assert!(T3::new(5, 1.0).is_ok());
    }

    #[test]
    fn coefficients_sum_to_one() {
        // c1 + c2 + c3 + c4 == 1 for any v, so a constant series is preserved.
        for &v in &[0.0, 0.3, 0.7, 1.0] {
            let t3 = T3::new(5, v).unwrap();
            assert_relative_eq!(t3.c1 + t3.c2 + t3.c3 + t3.c4, 1.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut t3 = T3::new(4, 0.7).unwrap();
        assert_eq!(t3.warmup_period(), 6 * 4 - 5);
        let out = t3.batch(&(1..=60).map(f64::from).collect::<Vec<_>>());
        for v in out.iter().take(t3.warmup_period() - 1) {
            assert!(v.is_none());
        }
        assert!(out[t3.warmup_period() - 1].is_some());
    }

    #[test]
    fn constant_series_yields_the_constant() {
        let mut t3 = T3::new(6, 0.7).unwrap();
        let out = t3.batch(&[50.0; 80]);
        let last = out.iter().rev().flatten().next().unwrap();
        assert_relative_eq!(*last, 50.0, epsilon = 1e-9);
    }

    #[test]
    fn zero_volume_factor_collapses_to_triple_cascaded_ema() {
        // With v = 0 the coefficients are c1=c2=c3=0, c4=1, so T3 == e3,
        // the third stage of the EMA cascade.
        let prices: Vec<f64> = (1..=80)
            .map(|i| 100.0 + (f64::from(i) * 0.2).sin() * 9.0)
            .collect();
        let mut t3 = T3::new(5, 0.0).unwrap();
        let got = t3.batch(&prices);

        let mut e1 = Ema::new(5).unwrap();
        let mut e2 = Ema::new(5).unwrap();
        let mut e3 = Ema::new(5).unwrap();
        let want: Vec<Option<f64>> = prices
            .iter()
            .map(|p| {
                e1.update(*p)
                    .and_then(|a| e2.update(a))
                    .and_then(|b| e3.update(b))
            })
            .collect();

        for i in (t3.warmup_period() - 1)..prices.len() {
            assert_relative_eq!(got[i].unwrap(), want[i].unwrap(), epsilon = 1e-9);
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut t3 = T3::new(4, 0.7).unwrap();
        let out = t3.batch(&(1..=60).map(f64::from).collect::<Vec<_>>());
        let last = *out.last().unwrap();
        assert!(last.is_some());
        assert_eq!(t3.update(f64::NAN), None);
        assert_eq!(t3.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut t3 = T3::new(4, 0.7).unwrap();
        t3.batch(&(1..=60).map(f64::from).collect::<Vec<_>>());
        assert!(t3.is_ready());
        t3.reset();
        assert!(!t3.is_ready());
        assert_eq!(t3.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 7.0)
            .collect();
        let batch = T3::new(7, 0.7).unwrap().batch(&prices);
        let mut b = T3::new(7, 0.7).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            T3::new(crate::error::MAX_PERIOD + 1, 0.7),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
