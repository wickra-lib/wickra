//! Adaptive RSI — an RSI whose up/down averaging adapts to the efficiency ratio.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Adaptive RSI — Wilder's RSI in which the smoothing of the average gain and
/// average loss **adapts to trendiness** via Kaufman's efficiency ratio, so the
/// oscillator reacts fast in a clean move and smooths through chop.
///
/// ```text
/// ER     = |price_t − price_{t−period}| / Σ |Δprice| over the window   (efficiency ratio, 0..1)
/// sc     = ( ER·(2/3 − 2/31) + 2/31 )²                                  (KAMA smoothing constant)
/// avg_gain += sc·(gain − avg_gain),  avg_loss += sc·(loss − avg_loss)
/// RSI    = 100 · avg_gain / (avg_gain + avg_loss)
/// ```
///
/// A fixed-period [`Rsi`](crate::Rsi) is a compromise: short periods whip in
/// ranges, long ones lag in trends. This adaptive form borrows Kaufman's
/// efficiency ratio (`directional move / total path`) to set the smoothing each
/// bar — near `1` (a clean trend) the averages track gains and losses almost
/// immediately; near `0` (noise) they barely move, filtering the chop. The result
/// is an RSI that is responsive when it should be and quiet when it should be. It
/// is the efficiency-ratio cousin of Ehlers' cycle-adaptive RSI, which instead
/// sets the lookback from the measured dominant cycle.
///
/// Output is bounded in `[0, 100]`; a flat market returns the neutral `50`. The
/// first value lands after `period + 1` inputs. Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, AdaptiveRsi};
///
/// let mut indicator = AdaptiveRsi::new(14).unwrap();
/// let mut last = None;
/// for i in 0..60 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.3).sin() * 5.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct AdaptiveRsi {
    period: usize,
    prices: VecDeque<f64>,
    abs_changes: VecDeque<f64>,
    abs_sum: f64,
    prev: Option<f64>,
    seed_gain: f64,
    seed_loss: f64,
    seed_count: usize,
    avg_gain: Option<f64>,
    avg_loss: Option<f64>,
    last: Option<f64>,
}

impl AdaptiveRsi {
    /// Construct an adaptive RSI with the given efficiency-ratio `period`.
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
            prices: VecDeque::with_capacity(period + 1),
            abs_changes: VecDeque::with_capacity(period),
            abs_sum: 0.0,
            prev: None,
            seed_gain: 0.0,
            seed_loss: 0.0,
            seed_count: 0,
            avg_gain: None,
            avg_loss: None,
            last: None,
        })
    }

    /// Configured efficiency-ratio period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }

    fn rsi_from_avgs(avg_gain: f64, avg_loss: f64) -> f64 {
        let denom = avg_gain + avg_loss;
        if denom == 0.0 {
            50.0
        } else {
            100.0 * (avg_gain / denom)
        }
    }

    fn efficiency_ratio(&self, price: f64) -> f64 {
        let oldest = *self.prices.front().expect("window non-empty");
        let direction = (price - oldest).abs();
        if self.abs_sum == 0.0 {
            0.0
        } else {
            (direction / self.abs_sum).clamp(0.0, 1.0)
        }
    }
}

impl Indicator for AdaptiveRsi {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, price: f64) -> Option<f64> {
        if !price.is_finite() {
            return None;
        }
        let Some(prev) = self.prev else {
            self.prev = Some(price);
            self.prices.push_back(price);
            return None;
        };
        let change = price - prev;
        self.prev = Some(price);
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };

        // Maintain the price window (period + 1) and the |Δ| window (period).
        self.prices.push_back(price);
        if self.prices.len() > self.period + 1 {
            self.prices.pop_front();
        }
        if self.abs_changes.len() == self.period {
            self.abs_sum -= self.abs_changes.pop_front().expect("non-empty");
        }
        self.abs_changes.push_back(change.abs());
        self.abs_sum += change.abs();

        if let (Some(ag), Some(al)) = (self.avg_gain, self.avg_loss) {
            let er = self.efficiency_ratio(price);
            let fast = 2.0 / 3.0;
            let slow = 2.0 / 31.0;
            let sc = (er * (fast - slow) + slow).powi(2);
            let new_ag = ag + sc * (gain - ag);
            let new_al = al + sc * (loss - al);
            self.avg_gain = Some(new_ag);
            self.avg_loss = Some(new_al);
            let v = Self::rsi_from_avgs(new_ag, new_al);
            self.last = Some(v);
            return Some(v);
        }

        self.seed_gain += gain;
        self.seed_loss += loss;
        self.seed_count += 1;
        if self.seed_count == self.period {
            let ag = self.seed_gain / self.period as f64;
            let al = self.seed_loss / self.period as f64;
            self.avg_gain = Some(ag);
            self.avg_loss = Some(al);
            let v = Self::rsi_from_avgs(ag, al);
            self.last = Some(v);
            return Some(v);
        }
        None
    }

    fn reset(&mut self) {
        self.prices.clear();
        self.abs_changes.clear();
        self.abs_sum = 0.0;
        self.prev = None;
        self.seed_gain = 0.0;
        self.seed_loss = 0.0;
        self.seed_count = 0;
        self.avg_gain = None;
        self.avg_loss = None;
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
        "AdaptiveRsi"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(AdaptiveRsi::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let r = AdaptiveRsi::new(14).unwrap();
        assert_eq!(r.period(), 14);
        assert_eq!(r.warmup_period(), 15);
        assert_eq!(r.name(), "AdaptiveRsi");
        assert!(!r.is_ready());
        assert_eq!(r.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut r = AdaptiveRsi::new(4).unwrap();
        let out = r.batch(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        for v in out.iter().take(4) {
            assert!(v.is_none());
        }
        assert!(out[4].is_some());
    }

    #[test]
    fn pure_uptrend_is_one_hundred() {
        let mut r = AdaptiveRsi::new(5).unwrap();
        let last = r
            .batch(&(1..=40).map(f64::from).collect::<Vec<_>>())
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 100.0, epsilon = 1e-9);
    }

    #[test]
    fn flat_market_is_neutral() {
        let mut r = AdaptiveRsi::new(4).unwrap();
        let last = r.batch(&[7.0; 20]).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 50.0, epsilon = 1e-9);
    }

    #[test]
    fn output_in_range() {
        let mut r = AdaptiveRsi::new(14).unwrap();
        for v in r
            .batch(
                &(0..200)
                    .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 8.0)
                    .collect::<Vec<_>>(),
            )
            .into_iter()
            .flatten()
        {
            assert!((0.0..=100.0).contains(&v));
        }
    }

    #[test]
    fn ignores_non_finite() {
        let mut r = AdaptiveRsi::new(4).unwrap();
        let _ready = r
            .batch(&[1.0, 2.0, 3.0, 4.0, 5.0])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(r.update(f64::NAN), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut r = AdaptiveRsi::new(4).unwrap();
        r.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        assert!(r.is_ready());
        r.reset();
        assert!(!r.is_ready());
        assert_eq!(r.value(), None);
        assert_eq!(r.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let xs: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 9.0)
            .collect();
        let batch = AdaptiveRsi::new(14).unwrap().batch(&xs);
        let mut b = AdaptiveRsi::new(14).unwrap();
        let streamed: Vec<_> = xs.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            AdaptiveRsi::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
