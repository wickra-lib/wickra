//! Volume bar builder — close a bar each time accumulated volume reaches a threshold.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::BarBuilder;

/// One completed volume bar (an OHLCV aggregate spanning ~`volume_per_bar` of volume).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumeBar {
    /// Open of the first candle in the bar.
    pub open: f64,
    /// Highest high across the bar.
    pub high: f64,
    /// Lowest low across the bar.
    pub low: f64,
    /// Close of the candle that closed the bar.
    pub close: f64,
    /// Accumulated volume in the bar (`>= volume_per_bar`; the crossing candle's
    /// overshoot is kept in the bar that closes).
    pub volume: f64,
}

/// Volume bar builder — emits a bar each time accumulated volume reaches
/// `volume_per_bar`.
///
/// Where [`TickBars`](crate::TickBars) sample on trade *count*, volume bars sample on
/// traded *quantity*: a bar closes once the candles fed into it have accumulated at
/// least `volume_per_bar` of volume. This gives each bar roughly equal participation,
/// which de-emphasises quiet periods and resolves bursts of heavy trading into more
/// bars. The companion [`DollarBars`](crate::DollarBars) builder uses traded *value*
/// (`price × volume`) instead, which is more robust to price-level drift over long
/// histories.
///
/// The bar is candle-granular: at most one bar closes per candle, and the candle
/// that crosses the threshold closes the bar with its overshoot included (the next
/// bar starts fresh). [`BarBuilder::update`] therefore returns either an empty vector
/// or a single [`VolumeBar`].
///
/// # Example
///
/// ```
/// use wickra_core::{BarBuilder, Candle, VolumeBars};
///
/// let c = |cl, v| Candle::new(cl, cl, cl, cl, v, 0).unwrap();
/// let mut bars = VolumeBars::new(100.0).unwrap();
/// assert!(bars.update(c(10.0, 60.0)).is_empty());
/// let out = bars.update(c(10.5, 60.0)); // 120 >= 100 -> close
/// assert_eq!(out.len(), 1);
/// assert_eq!(out[0].volume, 120.0);
/// ```
#[derive(Debug, Clone)]
pub struct VolumeBars {
    volume_per_bar: f64,
    count: usize,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    accumulated: f64,
}

impl VolumeBars {
    /// Construct a volume-bar builder with the given volume threshold.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `volume_per_bar` is not finite and positive.
    pub fn new(volume_per_bar: f64) -> Result<Self> {
        if !volume_per_bar.is_finite() || volume_per_bar <= 0.0 {
            return Err(Error::InvalidPeriod {
                message: "volume_per_bar must be finite and positive",
            });
        }
        Ok(Self {
            volume_per_bar,
            count: 0,
            open: 0.0,
            high: 0.0,
            low: 0.0,
            close: 0.0,
            accumulated: 0.0,
        })
    }

    /// Configured volume threshold per bar.
    pub const fn volume_per_bar(&self) -> f64 {
        self.volume_per_bar
    }

    /// Volume accumulated into the in-progress bar.
    pub const fn accumulated(&self) -> f64 {
        self.accumulated
    }
}

impl BarBuilder for VolumeBars {
    type Bar = VolumeBar;

    #[inline]
    fn update(&mut self, candle: Candle) -> Vec<VolumeBar> {
        if self.count == 0 {
            self.open = candle.open;
            self.high = candle.high;
            self.low = candle.low;
        } else {
            self.high = self.high.max(candle.high);
            self.low = self.low.min(candle.low);
        }
        self.close = candle.close;
        self.accumulated += candle.volume;
        self.count += 1;
        if self.accumulated < self.volume_per_bar {
            return Vec::new();
        }
        let bar = VolumeBar {
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.accumulated,
        };
        self.count = 0;
        self.accumulated = 0.0;
        vec![bar]
    }

    fn reset(&mut self) {
        self.count = 0;
        self.accumulated = 0.0;
    }

    #[inline]
    fn name(&self) -> &'static str {
        "VolumeBars"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn candle(open: f64, high: f64, low: f64, close: f64, volume: f64) -> Candle {
        Candle::new(open, high, low, close, volume, 0).unwrap()
    }

    #[test]
    fn rejects_invalid_threshold() {
        assert!(matches!(
            VolumeBars::new(0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            VolumeBars::new(-100.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            VolumeBars::new(f64::INFINITY),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let bars = VolumeBars::new(1000.0).unwrap();
        assert_relative_eq!(bars.volume_per_bar(), 1000.0, epsilon = 1e-12);
        assert_relative_eq!(bars.accumulated(), 0.0, epsilon = 1e-12);
        assert_eq!(bars.name(), "VolumeBars");
    }

    #[test]
    fn closes_when_threshold_reached() {
        let mut bars = VolumeBars::new(100.0).unwrap();
        assert_eq!(
            bars.update(candle(10.0, 10.0, 10.0, 10.0, 60.0)),
            Vec::new()
        );
        let out = bars.update(candle(10.5, 10.5, 10.5, 10.5, 60.0));
        assert_eq!(out.len(), 1);
        assert_relative_eq!(out[0].volume, 120.0, epsilon = 1e-12);
    }

    #[test]
    fn aggregates_ohlc() {
        let mut bars = VolumeBars::new(100.0).unwrap();
        bars.update(candle(10.0, 11.0, 9.0, 10.5, 50.0));
        let out = bars.update(candle(10.5, 12.0, 10.0, 11.0, 60.0));
        assert_relative_eq!(out[0].open, 10.0, epsilon = 1e-12);
        assert_relative_eq!(out[0].high, 12.0, epsilon = 1e-12);
        assert_relative_eq!(out[0].low, 9.0, epsilon = 1e-12);
        assert_relative_eq!(out[0].close, 11.0, epsilon = 1e-12);
    }

    #[test]
    fn below_threshold_emits_nothing() {
        let mut bars = VolumeBars::new(100.0).unwrap();
        bars.update(candle(10.0, 10.0, 10.0, 10.0, 30.0));
        assert_relative_eq!(bars.accumulated(), 30.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut bars = VolumeBars::new(100.0).unwrap();
        bars.update(candle(10.0, 10.0, 10.0, 10.0, 60.0));
        bars.reset();
        assert_relative_eq!(bars.accumulated(), 0.0, epsilon = 1e-12);
        assert_eq!(
            bars.update(candle(20.0, 20.0, 20.0, 20.0, 60.0)),
            Vec::new()
        );
    }

    #[test]
    fn batch_concatenates_completed_bars() {
        let mut bars = VolumeBars::new(100.0).unwrap();
        let candles = [
            candle(10.0, 10.0, 10.0, 10.0, 60.0),
            candle(10.0, 10.0, 10.0, 10.0, 60.0),
            candle(10.0, 10.0, 10.0, 10.0, 60.0),
            candle(10.0, 10.0, 10.0, 10.0, 60.0),
        ];
        let out = bars.batch(&candles);
        assert_eq!(out.len(), 2);
    }
}
