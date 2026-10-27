//! Generalized DEMA (GD) — Tim Tillson's volume-factor double EMA.

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::traits::Indicator;

/// Generalized DEMA — the building block of Tillson's [`T3`](crate::T3),
/// exposed on its own.
///
/// ```text
/// GD = (1 + v) · EMA(price) − v · EMA(EMA(price))
/// ```
///
/// where both EMAs share the same `period` and `v ∈ [0, 1]` is the *volume
/// factor*. `v` controls how much of the second-order lag correction is
/// applied:
///
/// - `v = 0` collapses GD to a plain [`Ema`](crate::Ema) (no correction).
/// - `v = 1` recovers the standard [`Dema`](crate::Dema) `2·EMA − EMA(EMA)`.
/// - intermediate values (Tillson uses `0.7`) trade a little lag reduction for
///   less overshoot than DEMA.
///
/// Because the coefficients `(1 + v)` and `−v` always sum to `1`, a constant
/// series maps to itself. The first output lands after `2·period − 1` inputs —
/// EMA1 seeds at `period`, then EMA2 needs another `period − 1` of EMA1's
/// outputs to seed, exactly like DEMA.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, GeneralizedDema};
///
/// let mut indicator = GeneralizedDema::new(5, 0.7).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct GeneralizedDema {
    ema1: Ema,
    ema2: Ema,
    period: usize,
    v: f64,
}

impl GeneralizedDema {
    /// Construct a generalized DEMA with the given `period` and volume factor
    /// `v`.
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
                message: "GD volume factor must be a finite value in [0.0, 1.0]",
            });
        }
        Ok(Self {
            ema1: Ema::new(period)?,
            ema2: Ema::new(period)?,
            period,
            v,
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
}

impl Indicator for GeneralizedDema {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        let e1 = self.ema1.update(input)?;
        let e2 = self.ema2.update(e1)?;
        Some((1.0 + self.v) * e1 - self.v * e2)
    }

    fn reset(&mut self) {
        self.ema1.reset();
        self.ema2.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // EMA1 seeds at period, then EMA2 needs another (period - 1) values.
        2 * self.period - 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.ema2.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "GD"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::Dema;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            GeneralizedDema::new(0, 0.7),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn rejects_invalid_volume_factor() {
        assert!(matches!(
            GeneralizedDema::new(5, -0.1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            GeneralizedDema::new(5, 1.5),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            GeneralizedDema::new(5, f64::NAN),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(GeneralizedDema::new(5, 0.0).is_ok());
        assert!(GeneralizedDema::new(5, 1.0).is_ok());
    }

    /// Cover the const accessors `period` + `volume_factor` and the
    /// Indicator-impl `warmup_period` + `name`.
    #[test]
    fn accessors_and_metadata() {
        let gd = GeneralizedDema::new(5, 0.7).unwrap();
        assert_eq!(gd.period(), 5);
        assert_relative_eq!(gd.volume_factor(), 0.7, epsilon = 1e-12);
        // EMA1 seeds at 5, EMA2 needs another 4 -> 2*period - 1 = 9.
        assert_eq!(gd.warmup_period(), 9);
        assert_eq!(gd.name(), "GD");
    }

    #[test]
    fn constant_series_yields_constant() {
        let mut gd = GeneralizedDema::new(5, 0.7).unwrap();
        let out = gd.batch(&[100.0_f64; 60]);
        let last = out.iter().rev().flatten().next().unwrap();
        assert_relative_eq!(*last, 100.0, epsilon = 1e-9);
    }

    #[test]
    fn v_one_equals_dema() {
        // GD with v = 1 is exactly the standard DEMA.
        let prices: Vec<f64> = (1..=80)
            .map(|i| (f64::from(i) * 0.3).sin() * 10.0 + 50.0)
            .collect();
        let mut gd = GeneralizedDema::new(7, 1.0).unwrap();
        let mut dema = Dema::new(7).unwrap();
        let gd_out = gd.batch(&prices);
        let dema_out = dema.batch(&prices);
        for (g, d) in gd_out.iter().zip(dema_out.iter()) {
            assert_eq!(g.is_some(), d.is_some());
            if let (Some(a), Some(b)) = (g, d) {
                assert_relative_eq!(*a, *b, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn v_zero_equals_ema() {
        // GD with v = 0 is a plain EMA (no second-order correction).
        let prices: Vec<f64> = (1..=60).map(|i| f64::from(i) * 0.5).collect();
        let mut gd = GeneralizedDema::new(6, 0.0).unwrap();
        let mut ema = Ema::new(6).unwrap();
        let gd_out = gd.batch(&prices);
        for (i, (g, p)) in gd_out.iter().zip(prices.iter()).enumerate() {
            // GD(v=0) feeds EMA1 into EMA2 but outputs EMA1 alone (coefficient
            // 1 on e1, 0 on e2); it is only ready once EMA2 is, so compare
            // against a standalone EMA chained the same way.
            let want = ema.update(*p).filter(|_| i + 1 >= gd.warmup_period());
            if let (Some(a), Some(b)) = (g, want) {
                assert_relative_eq!(*a, b, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=80).map(|i| f64::from(i) * 0.5).collect();
        let mut a = GeneralizedDema::new(7, 0.7).unwrap();
        let mut b = GeneralizedDema::new(7, 0.7).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut gd = GeneralizedDema::new(5, 0.7).unwrap();
        gd.batch(&(1..=50).map(f64::from).collect::<Vec<_>>());
        assert!(gd.is_ready());
        gd.reset();
        assert!(!gd.is_ready());
        assert_eq!(gd.update(1.0), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            GeneralizedDema::new(crate::error::MAX_PERIOD + 1, 0.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
