//! Tick bar builder — aggregate a fixed number of candles into one OHLCV bar.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::BarBuilder;

/// One completed tick bar (an OHLCV aggregate of `ticks` input candles).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickBar {
    /// Open of the first candle in the group.
    pub open: f64,
    /// Highest high across the group.
    pub high: f64,
    /// Lowest low across the group.
    pub low: f64,
    /// Close of the last candle in the group.
    pub close: f64,
    /// Summed volume across the group.
    pub volume: f64,
}

/// Tick bar builder — emits one OHLCV bar for every `ticks` input candles.
///
/// Classic time bars (1-minute, 1-hour) sample the market on a clock; tick bars
/// sample it on *activity* by grouping a fixed number of trades — here modelled as a
/// fixed number of input candles. In fast markets a tick bar closes quickly; in
/// quiet markets it takes longer, so each bar carries roughly equal information
/// content. This is the simplest of the information-driven bar types; the
/// [`VolumeBars`](crate::VolumeBars) and [`DollarBars`](crate::DollarBars) builders
/// extend the idea to equal traded volume and equal traded value respectively.
///
/// The open is the first candle's open, the high and low are the extremes across the
/// group, the close is the last candle's close, and the volume is the group sum.
/// Exactly one bar completes every `ticks` candles, so [`BarBuilder::update`]
/// returns either an empty vector or a single [`TickBar`].
///
/// # Example
///
/// ```
/// use wickra_core::{BarBuilder, Candle, TickBars};
///
/// let c = |o, h, l, cl, v| Candle::new(o, h, l, cl, v, 0).unwrap();
/// let mut bars = TickBars::new(3).unwrap();
/// assert!(bars.update(c(10.0, 11.0, 9.0, 10.5, 100.0)).is_empty());
/// assert!(bars.update(c(10.5, 12.0, 10.0, 11.0, 150.0)).is_empty());
/// let out = bars.update(c(11.0, 11.5, 10.8, 11.2, 120.0));
/// assert_eq!(out.len(), 1);
/// assert_eq!(out[0].volume, 370.0);
/// ```
#[derive(Debug, Clone)]
pub struct TickBars {
    ticks: usize,
    count: usize,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

impl TickBars {
    /// Construct a tick-bar builder that groups `ticks` candles per bar.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `ticks == 0`.
    pub fn new(ticks: usize) -> Result<Self> {
        if ticks == 0 {
            return Err(Error::PeriodZero);
        }
        if ticks > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            ticks,
            count: 0,
            open: 0.0,
            high: 0.0,
            low: 0.0,
            close: 0.0,
            volume: 0.0,
        })
    }

    /// Configured number of candles per bar.
    pub const fn ticks(&self) -> usize {
        self.ticks
    }

    /// Number of candles accumulated into the in-progress bar.
    pub const fn count(&self) -> usize {
        self.count
    }
}

impl BarBuilder for TickBars {
    type Bar = TickBar;

    #[inline]
    fn update(&mut self, candle: Candle) -> Vec<TickBar> {
        if self.count == 0 {
            self.open = candle.open;
            self.high = candle.high;
            self.low = candle.low;
            self.volume = 0.0;
        } else {
            self.high = self.high.max(candle.high);
            self.low = self.low.min(candle.low);
        }
        self.close = candle.close;
        self.volume += candle.volume;
        self.count += 1;
        if self.count < self.ticks {
            return Vec::new();
        }
        self.count = 0;
        vec![TickBar {
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
        }]
    }

    fn reset(&mut self) {
        self.count = 0;
        self.volume = 0.0;
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TickBars"
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
    fn rejects_zero_ticks() {
        assert!(matches!(TickBars::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let bars = TickBars::new(5).unwrap();
        assert_eq!(bars.ticks(), 5);
        assert_eq!(bars.count(), 0);
        assert_eq!(bars.name(), "TickBars");
    }

    #[test]
    fn emits_every_n_candles() {
        let mut bars = TickBars::new(2).unwrap();
        assert_eq!(bars.update(candle(10.0, 10.0, 10.0, 10.0, 1.0)), Vec::new());
        assert_eq!(bars.update(candle(10.0, 10.0, 10.0, 10.0, 1.0)).len(), 1);
        assert_eq!(bars.update(candle(10.0, 10.0, 10.0, 10.0, 1.0)), Vec::new());
        assert_eq!(bars.update(candle(10.0, 10.0, 10.0, 10.0, 1.0)).len(), 1);
    }

    #[test]
    fn aggregates_ohlcv() {
        let mut bars = TickBars::new(3).unwrap();
        bars.update(candle(10.0, 11.0, 9.0, 10.5, 100.0));
        bars.update(candle(10.5, 12.0, 10.0, 11.0, 150.0));
        let out = bars.update(candle(11.0, 11.5, 10.8, 11.2, 120.0));
        assert_eq!(out.len(), 1);
        assert_relative_eq!(out[0].open, 10.0, epsilon = 1e-12);
        assert_relative_eq!(out[0].high, 12.0, epsilon = 1e-12);
        assert_relative_eq!(out[0].low, 9.0, epsilon = 1e-12);
        assert_relative_eq!(out[0].close, 11.2, epsilon = 1e-12);
        assert_relative_eq!(out[0].volume, 370.0, epsilon = 1e-12);
    }

    #[test]
    fn partial_group_emits_nothing() {
        let mut bars = TickBars::new(4).unwrap();
        bars.update(candle(10.0, 10.0, 10.0, 10.0, 1.0));
        bars.update(candle(10.0, 10.0, 10.0, 10.0, 1.0));
        assert_eq!(bars.count(), 2);
    }

    #[test]
    fn reset_clears_state() {
        let mut bars = TickBars::new(3).unwrap();
        bars.update(candle(10.0, 10.0, 10.0, 10.0, 1.0));
        bars.update(candle(10.0, 10.0, 10.0, 10.0, 1.0));
        bars.reset();
        assert_eq!(bars.count(), 0);
        // After reset the next candle starts a fresh group.
        assert_eq!(bars.update(candle(20.0, 20.0, 20.0, 20.0, 5.0)), Vec::new());
        assert_eq!(bars.count(), 1);
    }

    #[test]
    fn batch_concatenates_completed_bars() {
        let mut bars = TickBars::new(2).unwrap();
        let candles = [
            candle(10.0, 10.0, 10.0, 10.0, 1.0),
            candle(10.0, 10.0, 10.0, 10.0, 1.0),
            candle(10.0, 10.0, 10.0, 10.0, 1.0),
            candle(10.0, 10.0, 10.0, 10.0, 1.0),
        ];
        let out = bars.batch(&candles);
        assert_eq!(out.len(), 2);
    }
}
