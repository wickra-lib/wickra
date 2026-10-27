//! Pairwise Beta — rolling OLS slope of one asset's log-returns on another's.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedPairMoments;
use crate::traits::Indicator;

/// Rolling Beta of asset `a`'s **log-returns** on asset `b`'s log-returns.
///
/// Each `update` receives one `(a, b)` pair of raw **prices**. Internally the
/// indicator differences consecutive prices into log-returns
/// `rₜ = ln(pₜ / pₜ₋₁)` and runs a rolling ordinary-least-squares regression of
/// `a`'s returns on `b`'s returns over the trailing window of `period` return
/// pairs:
///
/// ```text
/// cov_ab = (1/n) · Σ rₐ·r_b − r̄ₐ·r̄_b
/// var_b  = (1/n) · Σ r_b²   − r̄_b²
/// Beta   = cov_ab / var_b
/// ```
///
/// This is the slope of the OLS line and measures how much asset `a` moves, in
/// return space, for a unit return of asset `b`. A reading of `1.0` means the
/// two move together one-for-one; `2.0` means `a` typically doubles `b`'s
/// moves; negative readings signal an inverse relationship and the basis for a
/// hedge.
///
/// This differs from [`crate::Beta`], which regresses the raw inputs it is
/// fed. `PairwiseBeta` always works in return space: feed it raw price levels
/// and it computes the returns for you, which is the conventional way to
/// measure cross-asset Beta (a Beta on price *levels* is dominated by the
/// shared trend and rarely what you want).
///
/// Each `update` is O(1): four running sums (`Σrₐ`, `Σr_b`, `Σr_b²`,
/// `Σrₐ·r_b`) are maintained as the window of returns slides. A flat `b`
/// window has zero return variance and Beta is undefined; the indicator
/// returns `0` in that case rather than producing `NaN`.
///
/// Prices must be strictly positive and finite for the log-return to be
/// defined. A non-positive or non-finite price breaks the return chain: that
/// sample is dropped and the next valid price re-seeds the previous-price
/// reference, exactly as a real feed would resume after a bad tick.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, PairwiseBeta};
///
/// let mut indicator = PairwiseBeta::new(10).unwrap();
/// let mut last = None;
/// for i in 0..30 {
///     // A varying (non-constant-return) positive price path.
///     let b = 100.0 + 10.0 * (f64::from(i) * 0.5).sin();
///     // `a = b²`, so a's log-returns are exactly twice b's.
///     last = indicator.update((b * b, b));
/// }
/// assert!((last.unwrap() - 2.0).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct PairwiseBeta {
    period: usize,
    prev: Option<(f64, f64)>,
    window: VecDeque<(f64, f64)>,
    moments: ShiftedPairMoments,
}

impl PairwiseBeta {
    /// Construct a new rolling pairwise Beta over `period` return pairs.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 2` (variance needs at
    /// least two returns).
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "pairwise beta needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            prev: None,
            window: VecDeque::with_capacity(period),
            moments: ShiftedPairMoments::new(),
        })
    }

    /// Configured period (number of return pairs in the rolling window).
    pub const fn period(&self) -> usize {
        self.period
    }

    fn push_return(&mut self, ra: f64, rb: f64) -> Option<f64> {
        if self.window.len() == self.period {
            let (oa, ob) = self.window.pop_front().expect("non-empty");
            self.moments.evict(oa, ob);
        }
        self.window.push_back((ra, rb));
        self.moments.push(ra, rb);
        if self.moments.needs_reseed(self.period) {
            self.moments.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let var_b = self.moments.var_b(self.period);
        let cov = self.moments.cov(self.period);
        if var_b == 0.0 {
            // A flat benchmark-return window has no defined beta.
            return Some(0.0);
        }
        Some(cov / var_b)
    }
}

impl Indicator for PairwiseBeta {
    /// `(a, b)` price pair.
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (a, b) = input;
        if !(a > 0.0 && b > 0.0 && a.is_finite() && b.is_finite()) {
            // Bad tick: skipped without touching `prev`, so the next good pair
            // measures its return from the last price actually observed. The
            // previous behaviour cleared `prev` and silently dropped one
            // return, which changed the values after the bad tick -- the one
            // thing a rejected input must not do.
            return None;
        }
        let Some((pa, pb)) = self.prev else {
            self.prev = Some((a, b));
            return None;
        };
        self.prev = Some((a, b));
        let ra = (a / pa).ln();
        let rb = (b / pb).ln();
        self.push_return(ra, rb)
    }

    fn reset(&mut self) {
        self.prev = None;
        self.window.clear();
        self.moments.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One prior price to seed, then `period` return pairs.
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "PairwiseBeta"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_period_below_two() {
        assert!(PairwiseBeta::new(0).is_err());
        assert!(PairwiseBeta::new(1).is_err());
        assert!(PairwiseBeta::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let b = PairwiseBeta::new(14).unwrap();
        assert_eq!(b.period(), 14);
        assert_eq!(b.warmup_period(), 15);
        assert_eq!(b.name(), "PairwiseBeta");
    }

    #[test]
    fn squared_price_gives_beta_two() {
        // a = b² ⇒ a's log-returns are exactly 2× b's ⇒ beta = 2.
        let pairs: Vec<(f64, f64)> = (0..20)
            .map(|i| {
                let b = 100.0 + 10.0 * (f64::from(i) * 0.5).sin();
                (b * b, b)
            })
            .collect();
        let last = PairwiseBeta::new(5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 2.0, epsilon = 1e-9);
    }

    #[test]
    fn inverse_price_gives_beta_minus_one() {
        // a = 1/b ⇒ a's log-returns are −1× b's ⇒ beta = −1.
        let pairs: Vec<(f64, f64)> = (0..20)
            .map(|i| {
                let b = 100.0 + 10.0 * (f64::from(i) * 0.5).sin();
                (1.0 / b, b)
            })
            .collect();
        let last = PairwiseBeta::new(5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, -1.0, epsilon = 1e-9);
    }

    #[test]
    fn flat_benchmark_returns_zero() {
        // b constant ⇒ zero return variance ⇒ beta defined as 0.
        let pairs: Vec<(f64, f64)> = (0..10).map(|i| (100.0 * 1.01_f64.powi(i), 7.0)).collect();
        let last = PairwiseBeta::new(5)
            .unwrap()
            .batch(&pairs)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn bad_tick_breaks_return_chain() {
        let mut b = PairwiseBeta::new(3).unwrap();
        // Seed, one good return, then a non-positive price drops the chain.
        assert_eq!(b.update((100.0, 100.0)), None);
        assert_eq!(b.update((101.0, 101.0)), None);
        assert_eq!(b.update((0.0, 50.0)), None); // bad tick, prev reset
        assert!(!b.is_ready());
        // A non-finite price is rejected the same way.
        assert_eq!(b.update((f64::NAN, 50.0)), None);
        assert!(!b.is_ready());
        // Recovery: subsequent valid prices rebuild the window cleanly.
        for i in 0..5 {
            let p = 100.0 * 1.01_f64.powi(i);
            b.update((p * p, p));
        }
        assert!(b.is_ready());
    }

    #[test]
    fn reset_clears_state() {
        let mut b = PairwiseBeta::new(3).unwrap();
        for i in 0..6 {
            let p = 100.0 * 1.01_f64.powi(i);
            b.update((p * p, p));
        }
        assert!(b.is_ready());
        b.reset();
        assert!(!b.is_ready());
        assert_eq!(b.update((100.0, 100.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..60)
            .map(|i| {
                let t = f64::from(i);
                let b = 100.0 + 5.0 * t.sin();
                let a = 100.0 + 3.0 * t.sin() + 0.5 * t.cos();
                (a, b)
            })
            .collect();
        let batch = PairwiseBeta::new(14).unwrap().batch(&pairs);
        let mut b = PairwiseBeta::new(14).unwrap();
        let streamed: Vec<_> = pairs.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            PairwiseBeta::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
