//! Dollar bar builder — close a bar each time accumulated traded value reaches a threshold.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::BarBuilder;

/// One completed dollar bar (an OHLC aggregate spanning ~`dollar_per_bar` of traded value).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DollarBar {
    /// Open of the first candle in the bar.
    pub open: f64,
    /// Highest high across the bar.
    pub high: f64,
    /// Lowest low across the bar.
    pub low: f64,
    /// Close of the candle that closed the bar.
    pub close: f64,
    /// Summed volume across the bar.
    pub volume: f64,
    /// Accumulated traded value (`Σ close · volume`, `>= dollar_per_bar`).
    pub dollar: f64,
}

/// Dollar bar builder — emits a bar each time accumulated traded value
/// (`price × volume`) reaches `dollar_per_bar`.
///
/// Dollar bars are the most drift-robust of the information-driven bar types. Where
/// [`VolumeBars`](crate::VolumeBars) close on a fixed *quantity* of shares/contracts,
/// dollar bars close on a fixed *value*: each candle contributes `close × volume` to
/// the running total. As a market's price level rises over years, a fixed share
/// count buys ever more value and volume bars drift in meaning; dollar bars stay
/// economically comparable across the whole history, which is why they are the
/// preferred sampling for long backtests and machine-learning features.
///
/// The bar is candle-granular: at most one bar closes per candle, and the candle
/// that crosses the threshold closes the bar with its overshoot included.
/// [`BarBuilder::update`] returns either an empty vector or a single [`DollarBar`].
///
/// # Example
///
/// ```
/// use wickra_core::{BarBuilder, Candle, DollarBars};
///
/// let c = |cl, v| Candle::new(cl, cl, cl, cl, v, 0).unwrap();
/// let mut bars = DollarBars::new(1000.0).unwrap();
/// assert!(bars.update(c(10.0, 60.0)).is_empty()); // 600
/// let out = bars.update(c(10.0, 60.0)); // 1200 >= 1000 -> close
/// assert_eq!(out.len(), 1);
/// assert_eq!(out[0].dollar, 1200.0);
/// ```
#[derive(Debug, Clone)]
pub struct DollarBars {
    dollar_per_bar: f64,
    count: usize,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    dollar: f64,
}

impl DollarBars {
    /// Construct a dollar-bar builder with the given traded-value threshold.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `dollar_per_bar` is not finite and positive.
    pub fn new(dollar_per_bar: f64) -> Result<Self> {
        if !dollar_per_bar.is_finite() || dollar_per_bar <= 0.0 {
            return Err(Error::InvalidPeriod {
                message: "dollar_per_bar must be finite and positive",
            });
        }
        Ok(Self {
            dollar_per_bar,
            count: 0,
            open: 0.0,
            high: 0.0,
            low: 0.0,
            close: 0.0,
            volume: 0.0,
            dollar: 0.0,
        })
    }

    /// Configured traded-value threshold per bar.
    pub const fn dollar_per_bar(&self) -> f64 {
        self.dollar_per_bar
    }

    /// Traded value accumulated into the in-progress bar.
    pub const fn accumulated(&self) -> f64 {
        self.dollar
    }
}

impl BarBuilder for DollarBars {
    type Bar = DollarBar;

    #[inline]
    fn update(&mut self, candle: Candle) -> Vec<DollarBar> {
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
        self.dollar += candle.close * candle.volume;
        self.count += 1;
        if self.dollar < self.dollar_per_bar {
            return Vec::new();
        }
        let bar = DollarBar {
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
            dollar: self.dollar,
        };
        self.count = 0;
        self.dollar = 0.0;
        vec![bar]
    }

    fn reset(&mut self) {
        self.count = 0;
        self.volume = 0.0;
        self.dollar = 0.0;
    }

    #[inline]
    fn name(&self) -> &'static str {
        "DollarBars"
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
            DollarBars::new(0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            DollarBars::new(-1000.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            DollarBars::new(f64::NAN),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let bars = DollarBars::new(50_000.0).unwrap();
        assert_relative_eq!(bars.dollar_per_bar(), 50_000.0, epsilon = 1e-6);
        assert_relative_eq!(bars.accumulated(), 0.0, epsilon = 1e-12);
        assert_eq!(bars.name(), "DollarBars");
    }

    #[test]
    fn closes_when_value_reached() {
        let mut bars = DollarBars::new(1000.0).unwrap();
        assert_eq!(
            bars.update(candle(10.0, 10.0, 10.0, 10.0, 60.0)),
            Vec::new()
        ); // 600
        let out = bars.update(candle(10.0, 10.0, 10.0, 10.0, 60.0)); // 1200
        assert_eq!(out.len(), 1);
        assert_relative_eq!(out[0].dollar, 1200.0, epsilon = 1e-9);
        assert_relative_eq!(out[0].volume, 120.0, epsilon = 1e-12);
    }

    #[test]
    fn aggregates_ohlc() {
        let mut bars = DollarBars::new(1000.0).unwrap();
        bars.update(candle(10.0, 11.0, 9.0, 10.0, 50.0)); // 500
        let out = bars.update(candle(10.0, 12.0, 9.5, 11.0, 60.0)); // 500 + 660 = 1160
        assert_relative_eq!(out[0].open, 10.0, epsilon = 1e-12);
        assert_relative_eq!(out[0].high, 12.0, epsilon = 1e-12);
        assert_relative_eq!(out[0].low, 9.0, epsilon = 1e-12);
        assert_relative_eq!(out[0].close, 11.0, epsilon = 1e-12);
    }

    #[test]
    fn below_threshold_emits_nothing() {
        let mut bars = DollarBars::new(1000.0).unwrap();
        bars.update(candle(10.0, 10.0, 10.0, 10.0, 30.0)); // 300
        assert_relative_eq!(bars.accumulated(), 300.0, epsilon = 1e-9);
    }

    #[test]
    fn reset_clears_state() {
        let mut bars = DollarBars::new(1000.0).unwrap();
        bars.update(candle(10.0, 10.0, 10.0, 10.0, 60.0));
        bars.reset();
        assert_relative_eq!(bars.accumulated(), 0.0, epsilon = 1e-12);
        assert_eq!(
            bars.update(candle(20.0, 20.0, 20.0, 20.0, 10.0)),
            Vec::new()
        );
    }

    #[test]
    fn batch_concatenates_completed_bars() {
        let mut bars = DollarBars::new(1000.0).unwrap();
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
