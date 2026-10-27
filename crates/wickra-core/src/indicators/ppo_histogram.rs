//! Percentage Price Oscillator Histogram.

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::indicators::ppo::Ppo;
use crate::traits::Indicator;

/// PPO Histogram — the `ppo − signal` bar of the Percentage Price Oscillator.
///
/// ```text
/// ppo       = 100 · (EMA_fast − EMA_slow) / EMA_slow
/// signal    = EMA(ppo, signal_period)
/// histogram = ppo − signal
/// ```
///
/// [`Ppo`](crate::Ppo) itself only emits the percentage line; this indicator
/// adds the classic 9-period signal EMA on top and reports the resulting
/// zero-centered histogram. Because PPO is scale-free (the EMA gap is divided
/// by the slow EMA), the histogram is **comparable across instruments** — a
/// PPO histogram of `0.4` means the same relative momentum on any asset, unlike
/// the price-unit [`MacdHistogram`](crate::MacdHistogram).
///
/// With Appel's defaults `fast = 12`, `slow = 26`, `signal = 9`, the first
/// value lands after `slow + signal − 1` inputs — the point at which the slow
/// EMA and then the signal EMA are both seeded.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, PpoHistogram};
///
/// let mut indicator = PpoHistogram::new(12, 26, 9).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct PpoHistogram {
    ppo: Ppo,
    signal_ema: Ema,
    signal_period: usize,
    current: Option<f64>,
}

impl PpoHistogram {
    /// Construct a PPO histogram with the `fast`/`slow` EMA periods and the
    /// `signal` EMA period.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if any period is `0`, or
    /// [`Error::InvalidPeriod`] if `fast >= slow`.
    pub fn new(fast: usize, slow: usize, signal: usize) -> Result<Self> {
        if signal == 0 {
            return Err(Error::PeriodZero);
        }
        if signal > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            ppo: Ppo::new(fast, slow)?,
            signal_ema: Ema::new(signal)?,
            signal_period: signal,
            current: None,
        })
    }

    /// Default `(12, 26, 9)` configuration.
    pub fn classic() -> Self {
        Self::new(12, 26, 9).expect("classic PPO periods are valid")
    }

    /// Configured periods as `(fast, slow, signal)`.
    pub const fn periods(&self) -> (usize, usize, usize) {
        let (fast, slow) = self.ppo.periods();
        (fast, slow, self.signal_period)
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.current
    }
}

impl Indicator for PpoHistogram {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        // Guard before touching either stage so a non-finite input never
        // advances the signal EMA on a stale, re-fed PPO value.
        if !input.is_finite() {
            return None;
        }
        let ppo = self.ppo.update(input)?;
        let signal = self.signal_ema.update(ppo)?;
        let histogram = ppo - signal;
        self.current = Some(histogram);
        Some(histogram)
    }

    fn reset(&mut self) {
        self.ppo.reset();
        self.signal_ema.reset();
        self.current = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // Slow EMA seeds the PPO, then the signal EMA needs `signal − 1` more.
        self.ppo.warmup_period() + self.signal_period - 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.current.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "PpoHistogram"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_invalid_periods() {
        assert!(matches!(
            PpoHistogram::new(0, 26, 9),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            PpoHistogram::new(12, 0, 9),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            PpoHistogram::new(12, 26, 0),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            PpoHistogram::new(26, 12, 9),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let osc = PpoHistogram::classic();
        assert_eq!(osc.periods(), (12, 26, 9));
        assert_eq!(osc.name(), "PpoHistogram");
        assert_eq!(osc.warmup_period(), 26 + 9 - 1);
        assert_eq!(osc.value(), None);
        assert!(!osc.is_ready());
    }

    #[test]
    fn equals_ppo_minus_signal_ema() {
        // The histogram must equal PPO minus an EMA(signal) composed by hand.
        let prices: Vec<f64> = (1..=120)
            .map(|i| 100.0 + (f64::from(i) * 0.2).sin() * 6.0)
            .collect();
        let got = PpoHistogram::new(12, 26, 9).unwrap().batch(&prices);

        let mut ppo = Ppo::new(12, 26).unwrap();
        let mut sig = Ema::new(9).unwrap();
        let mut expected = Vec::with_capacity(prices.len());
        for p in &prices {
            let out = ppo
                .update(*p)
                .and_then(|line| sig.update(line).map(|signal| line - signal));
            expected.push(out);
        }
        assert_eq!(got, expected);
    }

    #[test]
    fn warmup_emits_first_value_at_warmup_period() {
        let mut osc = PpoHistogram::new(3, 6, 3).unwrap();
        let warmup = osc.warmup_period();
        assert_eq!(warmup, 6 + 3 - 1);
        for i in 1..warmup {
            assert!(osc.update(100.0 + i as f64).is_none());
        }
        assert!(osc.update(100.0 + warmup as f64).is_some());
        assert!(osc.is_ready());
    }

    #[test]
    fn constant_series_converges_to_zero() {
        let mut osc = PpoHistogram::classic();
        let out = osc.batch(&[100.0_f64; 200]);
        let last = out.iter().rev().flatten().next().expect("emits a value");
        assert_relative_eq!(*last, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut osc = PpoHistogram::new(3, 6, 3).unwrap();
        let out = osc.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        let before = *out.last().unwrap();
        assert!(before.is_some());
        assert_eq!(osc.update(f64::NAN), None);
        assert_eq!(osc.update(f64::INFINITY), None);
        assert_eq!(osc.value(), before);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=100)
            .map(|i| 100.0 + (f64::from(i) * 0.4).cos() * 10.0)
            .collect();
        let mut a = PpoHistogram::classic();
        let mut b = PpoHistogram::classic();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut osc = PpoHistogram::classic();
        osc.batch(&(1..=80).map(f64::from).collect::<Vec<_>>());
        assert!(osc.is_ready());
        osc.reset();
        assert!(!osc.is_ready());
        assert_eq!(osc.update(1.0), None);
    }

    #[test]
    fn rejects_signal_above_maximum() {
        assert!(matches!(
            PpoHistogram::new(12, 26, crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
