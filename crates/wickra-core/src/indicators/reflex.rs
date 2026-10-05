//! Ehlers Reflex — a zero-lag cycle oscillator built on a SuperSmoother prefilter.
#![allow(clippy::doc_markdown)]

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::super_smoother::SuperSmoother;
use crate::traits::Indicator;

/// Ehlers' **Reflex** — a near-zero-lag oscillator that measures how far the
/// smoothed price has deviated from the straight line connecting its endpoints
/// over the lookback.
///
/// From John Ehlers, "Reflex: A New Zero-Lag Indicator" (*Stocks & Commodities*,
/// Feb 2020):
///
/// ```text
/// Filt   = SuperSmoother(price, 0.5 · period)
/// slope  = (Filt[period] − Filt[0]) / period          (line over the window)
/// sum    = mean over i=1..period of ( Filt[0] + i·slope − Filt[i] )
/// ms     = 0.04·sum² + 0.96·ms[−1]                     (adaptive normaliser)
/// Reflex = sum / sqrt(ms)                              (0 if ms == 0)
/// ```
///
/// Reflex fits a straight line across the SuperSmoothed price over `period` bars
/// and averages the deviation of the curve from that line. Because the line uses
/// both endpoints, the measure has almost no lag — it crosses zero essentially at
/// the cycle turns. The adaptive mean-square normaliser rescales the output to a
/// roughly `±3` range regardless of price, so the same thresholds work on any
/// instrument. Its sibling [`Trendflex`](crate::Trendflex) uses the deviation from
/// the *current* value instead of the line, making it trend- rather than
/// cycle-sensitive.
///
/// The first value lands after `period + 1` SuperSmoothed samples. Each `update`
/// is O(`period`).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Reflex};
///
/// let mut indicator = Reflex::new(20).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Reflex {
    period: usize,
    smoother: SuperSmoother,
    filt: VecDeque<f64>,
    ms: f64,
    last: Option<f64>,
}

impl Reflex {
    /// Construct a Reflex with the given lookback `period`.
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
            // Ehlers smooths with half the cycle length (`a1 = exp(-1.414·π / (0.5·Length))`).
            smoother: SuperSmoother::with_critical_period(period, 0.5 * period as f64),
            filt: VecDeque::with_capacity(period + 1),
            ms: 0.0,
            last: None,
        })
    }

    /// Configured lookback period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for Reflex {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, price: f64) -> Option<f64> {
        if !price.is_finite() {
            return None;
        }
        let filt = self.smoother.update(price)?;
        if self.filt.len() == self.period + 1 {
            self.filt.pop_front();
        }
        self.filt.push_back(filt);
        if self.filt.len() < self.period + 1 {
            return None;
        }
        // Newest at index `period`, oldest (period bars ago) at index 0.
        let newest = self.filt[self.period];
        let oldest = self.filt[0];
        let slope = (oldest - newest) / self.period as f64;
        let mut sum = 0.0;
        for i in 1..=self.period {
            sum += (newest + i as f64 * slope) - self.filt[self.period - i];
        }
        sum /= self.period as f64;
        self.ms = 0.04 * sum * sum + 0.96 * self.ms;
        let reflex = if self.ms > 0.0 {
            sum / self.ms.sqrt()
        } else {
            0.0
        };
        self.last = Some(reflex);
        Some(reflex)
    }

    fn reset(&mut self) {
        self.smoother.reset();
        self.filt.clear();
        self.ms = 0.0;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "Reflex"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Reflex::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let r = Reflex::new(20).unwrap();
        assert_eq!(r.period(), 20);
        assert_eq!(r.warmup_period(), 21);
        assert_eq!(r.name(), "Reflex");
        assert!(!r.is_ready());
        assert_eq!(r.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut r = Reflex::new(5).unwrap();
        let xs: Vec<f64> = (0..12)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 3.0)
            .collect();
        let out = r.batch(&xs);
        for v in out.iter().take(5) {
            assert!(v.is_none());
        }
        assert!(out[5].is_some());
    }

    #[test]
    fn constant_input_is_zero() {
        // A flat price is exactly its own straight line -> zero deviation -> 0.
        let mut r = Reflex::new(10).unwrap();
        for v in r.batch(&[50.0; 100]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn cyclic_input_oscillates_around_zero() {
        let mut r = Reflex::new(20).unwrap();
        let xs: Vec<f64> = (0..400)
            .map(|i| 100.0 + (std::f64::consts::TAU * f64::from(i) / 20.0).sin() * 5.0)
            .collect();
        let out: Vec<f64> = r.batch(&xs).into_iter().flatten().skip(100).collect();
        assert!(out.iter().any(|&v| v > 0.5));
        assert!(out.iter().any(|&v| v < -0.5));
    }

    #[test]
    fn ignores_non_finite() {
        let mut r = Reflex::new(10).unwrap();
        r.batch(
            &(0..40)
                .map(|i| 100.0 + (f64::from(i) * 0.3).sin())
                .collect::<Vec<_>>(),
        );
        let before = r.value();
        assert_eq!(r.update(f64::NAN), None);
        // The rejected input must not have disturbed the state.
        assert_eq!(r.value(), before);
    }

    #[test]
    fn reset_clears_state() {
        let mut r = Reflex::new(10).unwrap();
        r.batch(
            &(0..40)
                .map(|i| 100.0 + (f64::from(i) * 0.3).sin())
                .collect::<Vec<_>>(),
        );
        assert!(r.is_ready());
        r.reset();
        assert!(!r.is_ready());
        assert_eq!(r.value(), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let xs: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let batch = Reflex::new(20).unwrap().batch(&xs);
        let mut b = Reflex::new(20).unwrap();
        let streamed: Vec<_> = xs.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    use crate::traits::BatchNanExt;

    #[test]
    fn rejects_period_above_max() {
        assert!(matches!(
            Reflex::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_for_several_periods() {
        for period in [1_usize, 2, 7] {
            let mut r = Reflex::new(period).unwrap();
            let xs: Vec<f64> = (0..20)
                .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 3.0)
                .collect();
            let out = r.batch(&xs);
            let warmup = r.warmup_period();
            assert!(out[..warmup - 1].iter().all(Option::is_none));
            assert!(out[warmup - 1].is_some());
        }
    }

    #[test]
    fn reset_replays_identically() {
        let xs: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let fresh = Reflex::new(13).unwrap().batch(&xs);
        let mut r = Reflex::new(13).unwrap();
        let first = r.batch(&xs);
        r.reset();
        let second = r.batch(&xs);
        assert_eq!(first, fresh);
        assert_eq!(second, fresh);
    }

    #[test]
    fn batch_nan_paths_match_streaming_bitwise() {
        let xs: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let mut out = vec![0.0; xs.len()];
        Reflex::new(13).unwrap().batch_nan_into(&xs, &mut out);
        let nan = Reflex::new(13).unwrap().batch_nan(&xs);
        let fast = Reflex::new(13).unwrap().batch_fast(&xs);
        let mut stream = Reflex::new(13).unwrap();
        let expected: Vec<u64> = xs
            .iter()
            .map(|&p| stream.update(p).unwrap_or(f64::NAN).to_bits())
            .collect();
        assert!(out.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
        assert!(nan.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
        assert!(fast.iter().zip(&expected).all(|(v, e)| v.to_bits() == *e));
    }

    #[test]
    fn smoother_uses_half_period_critical() {
        // Ehlers: a1 = exp(-1.414 * pi / (0.5 * Length)); for an odd length the
        // critical period is fractional (7 -> 3.5).
        let r = Reflex::new(7).unwrap();
        let got = r.smoother.coefficients();
        let want = SuperSmoother::with_critical_period(7, 3.5).coefficients();
        assert_eq!(
            (got.0.to_bits(), got.1.to_bits(), got.2.to_bits()),
            (want.0.to_bits(), want.1.to_bits(), want.2.to_bits())
        );
        assert_eq!(r.smoother.period(), 7);
        let full = SuperSmoother::new(7).unwrap().coefficients();
        assert!((got.0 - full.0).abs() > 1e-3);
    }

    #[test]
    fn first_value_hand_computed() {
        // period = 2, inputs 0, 0, 6. SuperSmoother(critical 1.0) outputs
        // 0, 0 (seed), then c1 * (6 + 0)/2 = 3*c1. Window filt = [0, 0, 3c1].
        // slope = (oldest - newest)/2 = -1.5*c1.
        // i=1: 3c1 + 1*(-1.5c1) - filt[1] = 1.5c1 ; i=2: 3c1 - 3c1 - filt[0] = 0.
        // sum = 1.5c1 / 2 = 0.75c1; ms = 0.04 * sum^2; reflex = sum / (0.2*|sum|) = 5.
        let mut r = Reflex::new(2).unwrap();
        let (c1, _, _) = r.smoother.coefficients();
        assert!(c1 > 0.0);
        let out = r.batch(&[0.0, 0.0, 6.0]);
        assert_eq!(out[1], None);
        assert_relative_eq!(out[2].unwrap(), 5.0, epsilon = 1e-12);
        assert_relative_eq!(r.ms, 0.04 * (0.75 * c1) * (0.75 * c1), epsilon = 1e-12);
        // The mirrored step gives -5.
        let mut r = Reflex::new(2).unwrap();
        let out = r.batch(&[0.0, 0.0, -6.0]);
        assert_relative_eq!(out[2].unwrap(), -5.0, epsilon = 1e-12);
    }

    #[test]
    fn zero_series_takes_zero_normaliser_branch() {
        // All-zero input keeps every filt value at exactly 0, so ms stays 0 and
        // the guarded division returns 0.
        let mut r = Reflex::new(4).unwrap();
        let out = r.batch(&[0.0; 30]);
        assert!(out
            .iter()
            .flatten()
            .all(|v| v.to_bits() == 0.0_f64.to_bits()));
        assert_eq!(out.iter().flatten().count(), 30 - 4);
    }
}
