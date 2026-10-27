#![allow(clippy::doc_markdown)]
//! CandleVolume — candlestick body with a volume-scaled width.

use crate::error::{Error, Result};
use crate::indicators::sma::Sma;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Output of [`CandleVolume`]: the signed candle body and its volume-relative width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandleVolumeOutput {
    /// Signed body `close − open` (positive = bullish candle).
    pub body: f64,
    /// Box width — volume relative to its `period` average (`1.0` = average).
    pub width: f64,
}

/// CandleVolume — the candlestick analogue of [`Equivolume`](crate::Equivolume):
/// each bar's **body** (`close − open`) paired with a **width** proportional to its
/// volume relative to the recent average.
///
/// ```text
/// body  = close − open                          (signed; + bullish, − bearish)
/// width = volume / SMA(volume, period)          (1.0 = average volume)
/// ```
///
/// Where Equivolume uses the high-low *range* for the box height, CandleVolume uses
/// the candlestick *body*, preserving direction: a wide bullish body (long up
/// candle on heavy volume) is strong demand, a wide bearish body strong supply, and
/// a narrow body on heavy volume (wide but short) is churn. The signed body plus
/// the normalised width capture both the move's direction and the participation
/// behind it.
///
/// The first value lands after `period` inputs (to seed the volume average). Each
/// `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, CandleVolume};
///
/// let mut indicator = CandleVolume::new(14).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let c = Candle::new(base, base + 1.0, base - 1.0, base + 0.5, 1_000.0 + f64::from(i), 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct CandleVolume {
    period: usize,
    vol_sma: Sma,
    last: Option<CandleVolumeOutput>,
}

impl CandleVolume {
    /// Construct a CandleVolume with the given volume-averaging `period`.
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
    pub const fn value(&self) -> Option<CandleVolumeOutput> {
        self.last
    }
}

impl Indicator for CandleVolume {
    type Input = Candle;
    type Output = CandleVolumeOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<CandleVolumeOutput> {
        let avg_vol = self.vol_sma.update(candle.volume)?;
        let body = candle.close - candle.open;
        let width = if avg_vol > 0.0 {
            candle.volume / avg_vol
        } else {
            0.0
        };
        let out = CandleVolumeOutput { body, width };
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
        "CandleVolume"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(open: f64, close: f64, volume: f64) -> Candle {
        let high = open.max(close) + 1.0;
        let low = open.min(close) - 1.0;
        Candle::new_unchecked(open, high, low, close, volume, 0)
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(CandleVolume::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let cv = CandleVolume::new(14).unwrap();
        assert_eq!(cv.period(), 14);
        assert_eq!(cv.warmup_period(), 14);
        assert_eq!(cv.name(), "CandleVolume");
        assert!(!cv.is_ready());
        assert_eq!(cv.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut cv = CandleVolume::new(3).unwrap();
        let candles: Vec<Candle> = (0..6).map(|_| c(100.0, 101.0, 1_000.0)).collect();
        let out = cv.batch(&candles);
        for v in out.iter().take(2) {
            assert!(v.is_none());
        }
        assert!(out[2].is_some());
    }

    #[test]
    fn bullish_body_positive() {
        let mut cv = CandleVolume::new(2).unwrap();
        let out = cv
            .batch(&[c(100.0, 103.0, 1_000.0), c(100.0, 103.0, 1_000.0)])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(out.body, 3.0, epsilon = 1e-9);
    }

    #[test]
    fn bearish_body_negative() {
        let mut cv = CandleVolume::new(2).unwrap();
        let out = cv
            .batch(&[c(103.0, 100.0, 1_000.0), c(103.0, 100.0, 1_000.0)])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(out.body, -3.0, epsilon = 1e-9);
    }

    #[test]
    fn heavy_bar_is_wide() {
        let mut cv = CandleVolume::new(3).unwrap();
        let candles = [
            c(100.0, 101.0, 1_000.0),
            c(100.0, 101.0, 1_000.0),
            c(100.0, 101.0, 4_000.0),
        ];
        let out = cv.batch(&candles).into_iter().flatten().last().unwrap();
        assert!(out.width > 1.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut cv = CandleVolume::new(3).unwrap();
        cv.batch(&[c(100.0, 101.0, 1_000.0); 6]);
        assert!(cv.is_ready());
        cv.reset();
        assert!(!cv.is_ready());
        assert_eq!(cv.value(), None);
        assert_eq!(cv.update(c(100.0, 101.0, 1_000.0)), None);
    }

    #[test]
    fn zero_volume_gives_zero_width() {
        let mut cv = CandleVolume::new(2).unwrap();
        let out = cv
            .batch(&[c(10.0, 11.0, 0.0), c(11.0, 12.0, 0.0), c(12.0, 13.0, 0.0)])
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
                let b = 100.0 + (f64::from(i) * 0.25).sin() * 5.0;
                c(b, b + 0.5, 1_000.0 + f64::from(i))
            })
            .collect();
        let batch = CandleVolume::new(14).unwrap().batch(&candles);
        let mut b = CandleVolume::new(14).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            CandleVolume::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
