//! Equivolume — the price box height and its volume-scaled width.

use crate::error::{Error, Result};
use crate::indicators::sma::Sma;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Output of [`Equivolume`]: the box's price height and its volume-relative width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EquivolumeOutput {
    /// Box height — the bar's price range `high − low`.
    pub height: f64,
    /// Box width — volume relative to its `period` average (`1.0` = average).
    pub width: f64,
}

/// Equivolume — Richard Arms' charting style rendered as numbers: each bar is a
/// "box" whose **height** is its price range and whose **width** is its volume
/// relative to the recent average.
///
/// ```text
/// height = high − low
/// width  = volume / SMA(volume, period)        (1.0 = average volume)
/// ```
///
/// Equivolume discards time and substitutes volume for the horizontal axis: a tall
/// narrow box is an easy move (big range on light volume), while a short wide box
/// is churn (small range on heavy volume) that often marks support/resistance.
/// Reporting the two dimensions lets you reconstruct that shape programmatically:
/// the height/width relationship is Arms' "ease of movement" read. The width is
/// normalised by the volume SMA so it self-scales across instruments.
///
/// The first value lands after `period` inputs (to seed the volume average). Each
/// `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Equivolume};
///
/// let mut indicator = Equivolume::new(14).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let c = Candle::new(base, base + 2.0, base - 2.0, base, 1_000.0 + f64::from(i), 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Equivolume {
    period: usize,
    vol_sma: Sma,
    last: Option<EquivolumeOutput>,
}

impl Equivolume {
    /// Construct an Equivolume with the given volume-averaging `period`.
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
            vol_sma: Sma::new(period)?,
            last: None,
        })
    }

    /// Configured volume-averaging period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<EquivolumeOutput> {
        self.last
    }
}

impl Indicator for Equivolume {
    type Input = Candle;
    type Output = EquivolumeOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<EquivolumeOutput> {
        let avg_vol = self.vol_sma.update(candle.volume)?;
        let height = candle.high - candle.low;
        let width = if avg_vol > 0.0 {
            candle.volume / avg_vol
        } else {
            0.0
        };
        let out = EquivolumeOutput { height, width };
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.vol_sma.reset();
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
        "Equivolume"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(high: f64, low: f64, volume: f64) -> Candle {
        Candle::new_unchecked(low, high, low, f64::midpoint(high, low), volume, 0)
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Equivolume::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let e = Equivolume::new(14).unwrap();
        assert_eq!(e.period(), 14);
        assert_eq!(e.warmup_period(), 14);
        assert_eq!(e.name(), "Equivolume");
        assert!(!e.is_ready());
        assert_eq!(e.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut e = Equivolume::new(3).unwrap();
        let candles: Vec<Candle> = (0..6).map(|_| c(102.0, 98.0, 1_000.0)).collect();
        let out = e.batch(&candles);
        for v in out.iter().take(2) {
            assert!(v.is_none());
        }
        assert!(out[2].is_some());
    }

    #[test]
    fn height_is_range() {
        let mut e = Equivolume::new(2).unwrap();
        let out = e
            .batch(&[c(105.0, 100.0, 1_000.0), c(105.0, 100.0, 1_000.0)])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(out.height, 5.0, epsilon = 1e-9);
    }

    #[test]
    fn average_volume_width_is_one() {
        let mut e = Equivolume::new(3).unwrap();
        let out = e
            .batch(&[c(102.0, 98.0, 1_000.0); 6])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(out.width, 1.0, epsilon = 1e-9);
    }

    #[test]
    fn heavy_bar_is_wide() {
        let mut e = Equivolume::new(3).unwrap();
        let candles = [
            c(102.0, 98.0, 1_000.0),
            c(102.0, 98.0, 1_000.0),
            c(102.0, 98.0, 4_000.0),
        ];
        let out = e.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(
            out.width > 1.0,
            "a heavy bar should be wider than average, got {}",
            out.width
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut e = Equivolume::new(3).unwrap();
        e.batch(&[c(102.0, 98.0, 1_000.0); 6]);
        assert!(e.is_ready());
        e.reset();
        assert!(!e.is_ready());
        assert_eq!(e.value(), None);
        assert_eq!(e.update(c(102.0, 98.0, 1_000.0)), None);
    }

    #[test]
    fn zero_volume_gives_zero_width() {
        let mut e = Equivolume::new(2).unwrap();
        let out = e
            .batch(&[c(11.0, 9.0, 0.0), c(12.0, 10.0, 0.0), c(13.0, 11.0, 0.0)])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_eq!(out.width, 0.0);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                c(
                    110.0 + (f64::from(i) * 0.25).sin() * 5.0,
                    90.0,
                    1_000.0 + f64::from(i),
                )
            })
            .collect();
        let batch = Equivolume::new(14).unwrap().batch(&candles);
        let mut b = Equivolume::new(14).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Equivolume::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
