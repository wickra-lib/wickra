//! Regime Label — volatility-quantile classification of the current bar.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::indicators::rolling_quantile::quantile_sorted;
use crate::indicators::sorted_window;
use crate::traits::Indicator;

/// Regime Label — a discrete `{−1, 0, +1}` classification of the current
/// volatility regime by where the latest rolling volatility falls within its
/// own recent distribution.
///
/// ```text
/// σₜ    = sample stddev of the last `vol_period` log returns
/// q1,q3 = 25th / 75th percentile of the last `lookback` σ readings
/// label = −1 if σₜ < q1   (calm regime)
///         +1 if σₜ > q3   (stressed regime)
///          0 otherwise    (normal regime)
/// ```
///
/// This is the canonical rolling-volatility-quantile regime split: rather than
/// thresholding absolute volatility (which is not comparable across instruments
/// or epochs), it asks whether *today's* volatility is unusually low or high
/// **relative to its own recent history**. `−1` is a calm regime, `+1` a
/// stressed / high-volatility regime, `0` the normal middle. Because the latest
/// reading is included in its own reference window, a freshly elevated
/// volatility prints `+1` until the window catches up to the new level — it
/// flags the *transition*, not just the absolute level. When the recent
/// volatilities are all equal (`q1 == q3`, e.g. a constant drift) there is no
/// spread to classify against and the label is `0`.
///
/// Each `update` is `O(vol_period + lookback log lookback)`. Non-finite and
/// non-positive prices are ignored.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, RegimeLabel};
///
/// let mut indicator = RegimeLabel::new(5, 20).unwrap();
/// let mut last = None;
/// for i in 0..60 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.5).sin());
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct RegimeLabel {
    vol_period: usize,
    lookback: usize,
    prev_price: Option<f64>,
    /// Trailing window of the last `vol_period` log returns.
    ret_window: VecDeque<f64>,
    ret_moments: ShiftedMoments,
    /// Trailing window of the last `lookback` volatility readings.
    vol_window: VecDeque<f64>,
    /// The window's values in `total_cmp` order, kept sorted as it slides:
    /// bit for bit what sorting a copy of the window would give.
    scratch: Vec<f64>,
    last: Option<f64>,
}

impl RegimeLabel {
    /// Construct a new Regime Label classifier.
    ///
    /// `vol_period` is the window for the rolling volatility; `lookback` is the
    /// window of volatility readings whose quartiles set the regime bands.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `vol_period < 2` (the sample standard
    /// deviation needs at least two returns) or if `lookback < 2` (the quartile
    /// split needs at least two readings).
    pub fn new(vol_period: usize, lookback: usize) -> Result<Self> {
        if vol_period < 2 {
            return Err(Error::InvalidPeriod {
                message: "regime label needs vol_period >= 2",
            });
        }
        if vol_period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if lookback < 2 {
            return Err(Error::InvalidPeriod {
                message: "regime label needs lookback >= 2",
            });
        }
        if lookback > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            vol_period,
            lookback,
            prev_price: None,
            ret_window: VecDeque::with_capacity(vol_period),
            ret_moments: ShiftedMoments::new(),
            vol_window: VecDeque::with_capacity(lookback),
            scratch: Vec::with_capacity(lookback),
            last: None,
        })
    }

    /// Configured `(vol_period, lookback)`.
    pub const fn params(&self) -> (usize, usize) {
        (self.vol_period, self.lookback)
    }
}

impl Indicator for RegimeLabel {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() || input <= 0.0 {
            return None;
        }
        let Some(prev) = self.prev_price else {
            self.prev_price = Some(input);
            return None;
        };
        self.prev_price = Some(input);
        let r = (input / prev).ln();
        // Roll the return window and its running moments.
        if self.ret_window.len() == self.vol_period {
            let old = self.ret_window.pop_front().expect("non-empty");
            self.ret_moments.evict(old);
        }
        self.ret_window.push_back(r);
        self.ret_moments.push(r);
        if self.ret_moments.needs_reseed(self.vol_period) {
            self.ret_moments.reseed(self.ret_window.iter().copied());
        }
        if self.ret_window.len() < self.vol_period {
            return None;
        }
        let vol = self.ret_moments.sample_variance(self.vol_period).sqrt();
        // Roll the volatility window.
        if self.vol_window.len() == self.lookback {
            let oldest = self.vol_window.pop_front().expect("window is full");
            sorted_window::remove(&mut self.scratch, oldest);
        }
        self.vol_window.push_back(vol);
        sorted_window::insert(&mut self.scratch, vol);
        if self.vol_window.len() < self.lookback {
            return None;
        }
        // Classify the latest volatility against the quartiles of the window.
        let q1 = quantile_sorted(&self.scratch, 0.25);
        let q3 = quantile_sorted(&self.scratch, 0.75);
        let label = if vol < q1 {
            -1.0
        } else if vol > q3 {
            1.0
        } else {
            0.0
        };
        self.last = Some(label);
        Some(label)
    }

    fn reset(&mut self) {
        self.prev_price = None;
        self.ret_window.clear();
        self.ret_moments.reset();
        self.vol_window.clear();
        self.scratch.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One price seeds `prev`, `vol_period` returns yield the first vol, then
        // `lookback` vols fill the regime window.
        self.vol_period + self.lookback
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RegimeLabel"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    #[test]
    fn rejects_bad_periods() {
        assert!(matches!(
            RegimeLabel::new(1, 20),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            RegimeLabel::new(5, 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let rl = RegimeLabel::new(5, 20).unwrap();
        assert_eq!(rl.params(), (5, 20));
        assert_eq!(rl.warmup_period(), 25);
        assert_eq!(rl.name(), "RegimeLabel");
        assert!(!rl.is_ready());
    }

    #[test]
    fn detects_stressed_regime_on_volatility_spike() {
        // Calm warmup, then a burst of large moves: the elevated volatility
        // prints +1 while the lookback window still holds the calm readings.
        let mut rl = RegimeLabel::new(4, 8).unwrap();
        let mut prices: Vec<f64> = (0..24)
            .map(|i| 100.0 + (f64::from(i) * 0.7).sin() * 0.2)
            .collect();
        let mut base = *prices.last().unwrap();
        for i in 0..8 {
            base *= if i % 2 == 0 { 1.08 } else { 0.93 };
            prices.push(base);
        }
        let out = rl.batch(&prices);
        assert!(
            out.iter().flatten().any(|&v| v == 1.0),
            "expected a stressed (+1) regime label"
        );
    }

    #[test]
    fn detects_calm_regime_after_volatility_drop() {
        // Volatile warmup, then a calm tail: the depressed volatility prints -1.
        let mut rl = RegimeLabel::new(4, 8).unwrap();
        let mut prices: Vec<f64> = Vec::new();
        let mut base = 100.0;
        for i in 0..24 {
            base *= if i % 2 == 0 { 1.05 } else { 0.96 };
            prices.push(base);
        }
        for i in 0..12 {
            prices.push(base + (f64::from(i) * 0.7).sin() * 0.05);
        }
        let out = rl.batch(&prices);
        assert!(
            out.iter().flatten().any(|&v| v == -1.0),
            "expected a calm (-1) regime label"
        );
    }

    #[test]
    fn zero_volatility_is_neutral() {
        // A constant price has exactly-zero returns => zero volatility on every
        // window => q1 == q3 == 0 => neutral 0 throughout. (A geometric drift is
        // *conceptually* constant-vol too, but floating-point rounding of the
        // log returns leaves ~1e-16 dispersion, so the exactly-flat series is
        // the clean way to pin the q1 == q3 branch.)
        let mut rl = RegimeLabel::new(4, 8).unwrap();
        for v in rl.batch(&[100.0; 40]).into_iter().flatten() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn output_is_ternary() {
        let mut rl = RegimeLabel::new(5, 20).unwrap();
        let prices: Vec<f64> = (0..300)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * (1.0 + (f64::from(i) * 0.05).sin() * 5.0))
            .collect();
        for v in rl.batch(&prices).into_iter().flatten() {
            assert!(v == -1.0 || v == 0.0 || v == 1.0, "non-ternary label {v}");
        }
    }

    #[test]
    fn ignores_non_finite_and_non_positive() {
        let mut rl = RegimeLabel::new(4, 6).unwrap();
        let prices: Vec<f64> = (0..40)
            .map(|i| 100.0 + (f64::from(i) * 0.5).sin() * 2.0)
            .collect();
        let out = rl.batch(&prices);
        let last = *out.last().unwrap();
        assert!(last.is_some());
        assert_eq!(rl.update(f64::NAN), None);
        assert_eq!(rl.update(-1.0), None);
        assert_eq!(rl.update(0.0), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut rl = RegimeLabel::new(4, 6).unwrap();
        rl.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        assert!(rl.is_ready());
        rl.reset();
        assert!(!rl.is_ready());
        assert_eq!(rl.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=160)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 4.0)
            .collect();
        let batch = RegimeLabel::new(5, 20).unwrap().batch(&prices);
        let mut b = RegimeLabel::new(5, 20).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }
}
