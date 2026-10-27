//! Volume RSI — Wilder's RSI applied to the volume stream.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Volume RSI — the Relative Strength Index computed on **volume** changes
/// instead of price changes.
///
/// Wilder's [`Rsi`](crate::Rsi) measures the balance of up- versus down-*price*
/// moves; the Volume RSI applies the identical accumulator to the bar-over-bar
/// change in volume:
///
/// ```text
/// change_t = volume_t − volume_{t−1}
/// gain     = max(change, 0),  loss = max(−change, 0)
/// avg_gain, avg_loss = Wilder-smoothed over `period`
/// VolumeRSI = 100 * avg_gain / (avg_gain + avg_loss)
/// ```
///
/// Readings above `50` mean volume is expanding (more was added than removed over
/// the smoothing window) and tend to confirm the prevailing move; readings below
/// `50` mark contracting participation. Output is bounded in `[0, 100]`; a stretch
/// of unchanged volume drives both averages to `0` and the indicator reports the
/// neutral `50` rather than an undefined `0 / 0`.
///
/// Only the candle's **volume** is used. The first bar sets the previous volume,
/// then `period` changes seed Wilder's averages, so the first value lands after
/// `period + 1` inputs. Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, VolumeRsi};
///
/// let mut indicator = VolumeRsi::new(14).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let v = 1_000.0 + (f64::from(i) * 0.3).sin() * 400.0;
///     let c = Candle::new(100.0, 101.0, 99.0, 100.5, v, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct VolumeRsi {
    period: usize,
    prev_volume: Option<f64>,
    seed_gains: f64,
    seed_losses: f64,
    seed_count: usize,
    avg_gain: Option<f64>,
    avg_loss: Option<f64>,
    last: Option<f64>,
}

impl VolumeRsi {
    /// Construct a Volume RSI with the given Wilder smoothing `period`.
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
            prev_volume: None,
            seed_gains: 0.0,
            seed_losses: 0.0,
            seed_count: 0,
            avg_gain: None,
            avg_loss: None,
            last: None,
        })
    }

    /// Configured smoothing period.
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
}

impl Indicator for VolumeRsi {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let volume = candle.volume;
        let Some(prev) = self.prev_volume else {
            self.prev_volume = Some(volume);
            return None;
        };
        let change = volume - prev;
        self.prev_volume = Some(volume);
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };

        if let (Some(ag), Some(al)) = (self.avg_gain, self.avg_loss) {
            let n = self.period as f64;
            let new_ag = (ag * (n - 1.0) + gain) / n;
            let new_al = (al * (n - 1.0) + loss) / n;
            self.avg_gain = Some(new_ag);
            self.avg_loss = Some(new_al);
            let v = Self::rsi_from_avgs(new_ag, new_al);
            self.last = Some(v);
            return Some(v);
        }

        self.seed_gains += gain;
        self.seed_losses += loss;
        self.seed_count += 1;
        if self.seed_count == self.period {
            let n = self.period as f64;
            let ag = self.seed_gains / n;
            let al = self.seed_losses / n;
            self.avg_gain = Some(ag);
            self.avg_loss = Some(al);
            let v = Self::rsi_from_avgs(ag, al);
            self.last = Some(v);
            return Some(v);
        }
        None
    }

    fn reset(&mut self) {
        self.prev_volume = None;
        self.seed_gains = 0.0;
        self.seed_losses = 0.0;
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
        "VolumeRsi"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Candle whose only material field here is `volume`.
    fn vol_candle(volume: f64) -> Candle {
        Candle::new_unchecked(100.0, 101.0, 99.0, 100.5, volume, 0)
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(VolumeRsi::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let v = VolumeRsi::new(14).unwrap();
        assert_eq!(v.period(), 14);
        assert_eq!(v.warmup_period(), 15);
        assert_eq!(v.name(), "VolumeRsi");
        assert!(!v.is_ready());
        assert_eq!(v.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut v = VolumeRsi::new(3).unwrap();
        let candles: Vec<Candle> = (0..6).map(|i| vol_candle(1_000.0 + f64::from(i))).collect();
        let out = v.batch(&candles);
        // warmup_period == period + 1 == 4: first emission at index 3.
        for o in out.iter().take(3) {
            assert!(o.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn rising_volume_is_one_hundred() {
        // Every change positive -> avg_loss 0 -> RSI 100.
        let mut v = VolumeRsi::new(5).unwrap();
        let candles: Vec<Candle> = (1..=40).map(|i| vol_candle(f64::from(i) * 100.0)).collect();
        let last = v.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 100.0, epsilon = 1e-9);
    }

    #[test]
    fn falling_volume_is_zero() {
        let mut v = VolumeRsi::new(5).unwrap();
        let candles: Vec<Candle> = (1..=40)
            .map(|i| vol_candle(5_000.0 - f64::from(i) * 100.0))
            .collect();
        let last = v.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn flat_volume_is_neutral() {
        // Unchanged volume -> no gains and no losses -> neutral 50.
        let mut v = VolumeRsi::new(3).unwrap();
        let candles: Vec<Candle> = (0..20).map(|_| vol_candle(2_000.0)).collect();
        let last = v.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 50.0, epsilon = 1e-12);
    }

    #[test]
    fn output_in_range() {
        let mut v = VolumeRsi::new(14).unwrap();
        let candles: Vec<Candle> = (0..200)
            .map(|i| vol_candle(1_000.0 + (f64::from(i) * 0.3).sin() * 600.0))
            .collect();
        for o in v.batch(&candles).into_iter().flatten() {
            assert!((0.0..=100.0).contains(&o));
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut v = VolumeRsi::new(3).unwrap();
        let candles: Vec<Candle> = (0..20)
            .map(|i| vol_candle(1_000.0 + f64::from(i)))
            .collect();
        v.batch(&candles);
        assert!(v.is_ready());
        v.reset();
        assert!(!v.is_ready());
        assert_eq!(v.value(), None);
        assert_eq!(v.update(vol_candle(1_000.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..120)
            .map(|i| vol_candle(1_000.0 + (f64::from(i) * 0.25).sin() * 500.0))
            .collect();
        let batch = VolumeRsi::new(14).unwrap().batch(&candles);
        let mut b = VolumeRsi::new(14).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            VolumeRsi::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
