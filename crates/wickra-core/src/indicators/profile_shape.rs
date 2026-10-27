//! Profile Shape — classifies the volume profile as b-shape, P-shape, or D/normal.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Profile Shape — classifies a rolling volume profile by where its point of
/// control (POC) sits within the range: `b`, `P`, or `D` (normal).
///
/// ```text
/// build a `bins`-bucket volume profile over the last `period` candles
/// poc_idx = bin with the most volume
/// +1  P-shape : POC in the upper third  (heavy top, thin tail down) — short-covering / accumulation
/// −1  b-shape : POC in the lower third  (heavy bottom, thin tail up) — long-liquidation / distribution
///  0  D/normal: POC in the middle third (balanced bell)
/// ```
///
/// Market Profile readers classify the day's shape by the location of the heaviest
/// trading. A **P-shape** (control high, a thin tail beneath) typically marks
/// short-covering or the start of accumulation; a **b-shape** (control low, thin
/// tail above) marks long liquidation or distribution; a **D-shape** is a balanced,
/// two-sided day. Reducing the profile to this three-way code gives a compact,
/// streaming read of market posture.
///
/// The output is `+1` / `0` / `−1`. The first value lands after `period` candles;
/// each `update` rebuilds the profile in O(`period · bins`).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, ProfileShape};
///
/// let mut indicator = ProfileShape::new(20, 24).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
///     let c = Candle::new(base, base + 1.0, base - 1.0, base, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct ProfileShape {
    period: usize,
    bins: usize,
    window: VecDeque<Candle>,
    last: Option<f64>,
}

impl ProfileShape {
    /// Construct a Profile Shape classifier.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `period` is zero, or
    /// [`Error::InvalidPeriod`] if `bins < 3` (the three-way split needs three
    /// zones).
    pub fn new(period: usize, bins: usize) -> Result<Self> {
        if period == 0 {
            return Err(Error::PeriodZero);
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if bins < 3 {
            return Err(Error::InvalidPeriod {
                message: "profile shape needs bins >= 3",
            });
        }
        if bins > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            bins,
            window: VecDeque::with_capacity(period),
            last: None,
        })
    }

    /// Configured `(period, bins)`.
    pub const fn params(&self) -> (usize, usize) {
        (self.period, self.bins)
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn poc_index(&self) -> usize {
        let mut low = f64::INFINITY;
        let mut high = f64::NEG_INFINITY;
        for c in &self.window {
            low = low.min(c.low);
            high = high.max(c.high);
        }
        let mut hist = vec![0.0; self.bins];
        let span = high - low;
        if span > 0.0 {
            let width = span / self.bins as f64;
            for c in &self.window {
                if c.volume == 0.0 {
                    continue;
                }
                let lo_idx = (((c.low - low) / width).floor() as usize).min(self.bins - 1);
                let hi_idx = (((c.high - low) / width).floor() as usize).min(self.bins - 1);
                let share = c.volume / (hi_idx - lo_idx + 1) as f64;
                for bin in hist.iter_mut().take(hi_idx + 1).skip(lo_idx) {
                    *bin += share;
                }
            }
        }
        let mut poc_idx = 0;
        let mut poc_vol = f64::NEG_INFINITY;
        for (idx, &vol) in hist.iter().enumerate() {
            if vol > poc_vol {
                poc_vol = vol;
                poc_idx = idx;
            }
        }
        poc_idx
    }
}

impl Indicator for ProfileShape {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(candle);
        if self.window.len() < self.period {
            return None;
        }
        let poc = self.poc_index();
        let lower = self.bins / 3;
        let upper = self.bins - self.bins / 3;
        let shape = if poc >= upper {
            1.0
        } else if poc < lower {
            -1.0
        } else {
            0.0
        };
        self.last = Some(shape);
        Some(shape)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ProfileShape"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(high: f64, low: f64, volume: f64) -> Candle {
        Candle::new_unchecked(
            f64::midpoint(high, low),
            high,
            low,
            f64::midpoint(high, low),
            volume,
            0,
        )
    }

    #[test]
    fn rejects_invalid_params() {
        assert!(matches!(ProfileShape::new(0, 24), Err(Error::PeriodZero)));
        assert!(matches!(
            ProfileShape::new(20, 2),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let p = ProfileShape::new(20, 24).unwrap();
        assert_eq!(p.params(), (20, 24));
        assert_eq!(p.warmup_period(), 20);
        assert_eq!(p.name(), "ProfileShape");
        assert!(!p.is_ready());
        assert_eq!(p.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut p = ProfileShape::new(4, 9).unwrap();
        let candles: Vec<Candle> = (0..6).map(|_| c(110.0, 90.0, 1_000.0)).collect();
        let out = p.batch(&candles);
        for v in out.iter().take(3) {
            assert!(v.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn heavy_top_is_p_shape() {
        // Volume concentrated near the top of the range -> P-shape -> +1.
        let mut p = ProfileShape::new(6, 9).unwrap();
        let mut candles: Vec<Candle> = (0..5).map(|_| c(119.0, 117.0, 5_000.0)).collect();
        candles.push(c(119.0, 80.0, 50.0)); // a thin tail down to 80
        let last = p.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, 1.0);
    }

    #[test]
    fn heavy_bottom_is_b_shape() {
        let mut p = ProfileShape::new(6, 9).unwrap();
        let mut candles: Vec<Candle> = (0..5).map(|_| c(83.0, 81.0, 5_000.0)).collect();
        candles.push(c(120.0, 81.0, 50.0)); // a thin tail up to 120
        let last = p.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, -1.0);
    }

    #[test]
    fn balanced_is_d_shape() {
        // Volume concentrated in the middle -> D/normal -> 0.
        let mut p = ProfileShape::new(6, 9).unwrap();
        let mut candles: Vec<Candle> = (0..5).map(|_| c(101.0, 99.0, 5_000.0)).collect();
        candles.push(c(120.0, 80.0, 50.0)); // thin tails both ways, POC in the middle
        let last = p.batch(&candles).into_iter().flatten().last().unwrap();
        assert_eq!(last, 0.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut p = ProfileShape::new(4, 9).unwrap();
        p.batch(&[c(110.0, 90.0, 1_000.0); 6]);
        assert!(p.is_ready());
        p.reset();
        assert!(!p.is_ready());
        assert_eq!(p.value(), None);
        assert_eq!(p.update(c(110.0, 90.0, 1_000.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                c(
                    110.0 + (f64::from(i) * 0.25).sin() * 9.0,
                    90.0,
                    1_000.0 + f64::from(i),
                )
            })
            .collect();
        let batch = ProfileShape::new(20, 24).unwrap().batch(&candles);
        let mut b = ProfileShape::new(20, 24).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn flat_window_is_handled() {
        // Zero high-low span skips the histogram pass entirely.
        let mut p = ProfileShape::new(2, 4).unwrap();
        p.update(c(50.0, 50.0, 10.0));
        assert!(p.update(c(50.0, 50.0, 10.0)).is_some());
    }

    #[test]
    fn zero_volume_window_is_handled() {
        // Non-flat window of zero-volume candles hits the skip path.
        let mut p = ProfileShape::new(2, 4).unwrap();
        p.update(c(60.0, 40.0, 0.0));
        assert!(p.update(c(60.0, 40.0, 0.0)).is_some());
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            ProfileShape::new(crate::error::MAX_PERIOD + 1, 24),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn rejects_bins_above_maximum() {
        assert!(matches!(
            ProfileShape::new(20, crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
