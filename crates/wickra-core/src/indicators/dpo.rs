//! Detrended Price Oscillator.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::traits::Indicator;

/// Detrended Price Oscillator — strips the trend out of price to expose its
/// shorter cycles.
///
/// Instead of comparing price to a *current* moving average, DPO compares a
/// **past** price — shifted back by `period / 2 + 1` bars — to the moving
/// average of the window:
///
/// ```text
/// shift = period / 2 + 1
/// DPO_t = price_{t − shift} − SMA(period)_t
/// ```
///
/// Because the price is taken from roughly half a cycle back, the dominant
/// trend cancels out and what remains oscillates around zero — making the
/// peak-to-peak cycle length easy to read. DPO is **not** a momentum
/// indicator and is not meant to track the latest bar.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Dpo};
///
/// let mut indicator = Dpo::new(20).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.3).sin() * 10.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Dpo {
    period: usize,
    shift: usize,
    /// Window of the most recent `capacity` prices, oldest at the front.
    capacity: usize,
    window: VecDeque<f64>,
    sum: RollingSum,
    last: Option<f64>,
}

impl Dpo {
    /// Construct a new DPO with the given period.
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
        let shift = period / 2 + 1;
        // The window must cover both the SMA (`period` prices) and the
        // look-back (`shift + 1` prices: the current bar plus `shift` history).
        let capacity = period.max(shift + 1);
        Ok(Self {
            period,
            shift,
            capacity,
            window: VecDeque::with_capacity(capacity),
            sum: RollingSum::new(),
            last: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// The look-back shift `period / 2 + 1`.
    pub const fn shift(&self) -> usize {
        self.shift
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for Dpo {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            // Non-finite input is ignored; the window is left untouched.
            return None;
        }
        self.window.push_back(input);
        self.sum.push(input);
        let len = self.window.len();
        if len > self.period {
            // The price that just left the SMA window.
            self.sum.evict(self.window[len - 1 - self.period]);
        }
        if self.window.len() > self.capacity {
            self.window.pop_front();
        }
        if self.sum.needs_reseed(self.period) {
            // The running total covers the newest `period` prices — a suffix of
            // the window, not the whole of it, because the window is kept longer
            // than the SMA to serve the displacement.
            let live = self.window.len().saturating_sub(self.period);
            self.sum.reseed(self.window.iter().skip(live).copied());
        }
        if self.window.len() < self.capacity {
            return None;
        }
        let sma = self.sum.value() / self.period as f64;
        // `price_{t - shift}` — index counts back from the newest bar.
        let shifted = self.window[self.window.len() - 1 - self.shift];
        let dpo = shifted - sma;
        self.last = Some(dpo);
        Some(dpo)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.sum.reset();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.capacity
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "DPO"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(Dpo::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `value` (73-85) and the
    /// Indicator-impl `name` body (132-134). `shift` is already covered
    /// by `shift_is_half_period_plus_one`; `warmup_period` by
    /// `reference_values`.
    #[test]
    fn accessors_and_metadata() {
        let mut dpo = Dpo::new(20).unwrap();
        assert_eq!(dpo.period(), 20);
        assert_eq!(dpo.name(), "DPO");
        assert_eq!(dpo.value(), None);
        for i in 1..=dpo.warmup_period() {
            dpo.update(f64::from(u32::try_from(i).unwrap()));
        }
        assert!(dpo.value().is_some());
    }

    #[test]
    fn shift_is_half_period_plus_one() {
        assert_eq!(Dpo::new(20).unwrap().shift(), 11);
        assert_eq!(Dpo::new(4).unwrap().shift(), 3);
    }

    #[test]
    fn reference_values() {
        // DPO(4): shift = 3, capacity = max(4, 4) = 4.
        // At input 4: window [1,2,3,4], SMA = 2.5, price[t-3] = 1 -> 1 - 2.5 = -1.5.
        let mut dpo = Dpo::new(4).unwrap();
        let out = dpo.batch(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(dpo.warmup_period(), 4);
        assert_eq!(out[0], None);
        assert_eq!(out[2], None);
        assert_relative_eq!(out[3].unwrap(), -1.5, epsilon = 1e-12);
        assert_relative_eq!(out[4].unwrap(), -1.5, epsilon = 1e-12);
        assert_relative_eq!(out[5].unwrap(), -1.5, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_yields_zero() {
        // A flat series: the shifted price equals the SMA, so DPO is 0.
        let mut dpo = Dpo::new(10).unwrap();
        let out = dpo.batch(&[50.0; 40]);
        for v in out.iter().skip(dpo.warmup_period() - 1).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut dpo = Dpo::new(4).unwrap();
        let out = dpo.batch(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let last = *out.last().unwrap();
        assert!(last.is_some());
        assert_eq!(dpo.update(f64::NAN), None);
        assert_eq!(dpo.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut dpo = Dpo::new(4).unwrap();
        dpo.batch(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert!(dpo.is_ready());
        dpo.reset();
        assert!(!dpo.is_ready());
        assert_eq!(dpo.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=80)
            .map(|i| 100.0 + (f64::from(i) * 0.4).sin() * 7.0)
            .collect();
        let batch = Dpo::new(20).unwrap().batch(&prices);
        let mut b = Dpo::new(20).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Dpo::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
