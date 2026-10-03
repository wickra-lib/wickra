//! Range bar builder — fixed price-range bars with no reversal penalty.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::BarBuilder;

/// One completed range bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeBar {
    /// Price at the bar's origin edge.
    pub open: f64,
    /// Price at the bar's far edge (`open ± range`).
    pub close: f64,
    /// `+1` for an up bar, `-1` for a down bar.
    pub direction: i8,
}

/// Range bar builder using a fixed price increment on close prices.
///
/// A range bar completes every time price travels a fixed `range` from the current
/// anchor, in *either* direction. This is the key difference from
/// [`RenkoBars`](crate::RenkoBars): Renko imposes a `2 * box_size` penalty to
/// reverse direction, so it filters out small oscillations; range bars have **no
/// reversal penalty** — a move of exactly `range` against the trend prints a bar
/// immediately. Range bars therefore track every leg of price movement, while Renko
/// smooths them.
///
/// Construction rules:
///
/// - The first candle seeds the anchor and prints no bar.
/// - Each subsequent candle prints one bar for every `range` of close movement away
///   from the anchor; a candle that gaps several ranges prints them all in one
///   [`BarBuilder::update`] call.
/// - Bars are aligned to the `range` grid relative to the seed price.
///
/// # Example
///
/// ```
/// use wickra_core::{BarBuilder, Candle, RangeBars};
///
/// let flat = |price: f64| Candle::new(price, price, price, price, 1.0, 0).unwrap();
/// let mut bars = RangeBars::new(1.0).unwrap();
/// assert!(bars.update(flat(10.0)).is_empty()); // seed
/// let up = bars.update(flat(12.0)); // +2 ranges
/// assert_eq!(up.len(), 2);
/// let down = bars.update(flat(11.0)); // -1 range, no penalty
/// assert_eq!(down.len(), 1);
/// ```
#[derive(Debug, Clone)]
pub struct RangeBars {
    range: f64,
    anchor: Option<f64>,
}

impl RangeBars {
    /// Construct a range-bar builder with the given price increment.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `range` is not finite and positive.
    pub fn new(range: f64) -> Result<Self> {
        if !range.is_finite() || range <= 0.0 {
            return Err(Error::InvalidPeriod {
                message: "range must be finite and positive",
            });
        }
        Ok(Self {
            range,
            anchor: None,
        })
    }

    /// Configured price range.
    pub const fn range(&self) -> f64 {
        self.range
    }

    /// Current anchor level (the close of the last completed bar, or the seed
    /// price before any bar has formed).
    pub const fn anchor(&self) -> Option<f64> {
        self.anchor
    }
}

impl BarBuilder for RangeBars {
    type Bar = RangeBar;

    #[inline]
    fn update(&mut self, candle: Candle) -> Vec<RangeBar> {
        let close = candle.close;
        let Some(mut anchor) = self.anchor else {
            self.anchor = Some(close);
            return Vec::new();
        };
        let range = self.range;
        let mut bars = Vec::new();
        while close >= anchor + range {
            bars.push(RangeBar {
                open: anchor,
                close: anchor + range,
                direction: 1,
            });
            anchor += range;
        }
        while close <= anchor - range {
            bars.push(RangeBar {
                open: anchor,
                close: anchor - range,
                direction: -1,
            });
            anchor -= range;
        }
        self.anchor = Some(anchor);
        bars
    }

    fn reset(&mut self) {
        self.anchor = None;
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RangeBars"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn flat(price: f64) -> Candle {
        Candle::new(price, price, price, price, 1.0, 0).unwrap()
    }

    #[test]
    fn rejects_invalid_range() {
        assert!(matches!(
            RangeBars::new(0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            RangeBars::new(-1.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            RangeBars::new(f64::NAN),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let bars = RangeBars::new(2.5).unwrap();
        assert_eq!(bars.name(), "RangeBars");
        assert_relative_eq!(bars.range(), 2.5, epsilon = 1e-12);
        assert_eq!(bars.anchor(), None);
    }

    #[test]
    fn first_candle_seeds_without_bar() {
        let mut bars = RangeBars::new(1.0).unwrap();
        assert_eq!(bars.update(flat(10.0)), Vec::new());
        assert_eq!(bars.anchor(), Some(10.0));
    }

    #[test]
    fn up_move_prints_aligned_bars() {
        let mut bars = RangeBars::new(1.0).unwrap();
        bars.update(flat(10.0));
        let up = bars.update(flat(13.0));
        assert_eq!(up.len(), 3);
        assert_relative_eq!(up[0].open, 10.0, epsilon = 1e-12);
        assert_relative_eq!(up[2].close, 13.0, epsilon = 1e-12);
        assert!(up.iter().all(|b| b.direction == 1));
        assert_eq!(bars.anchor(), Some(13.0));
    }

    #[test]
    fn down_move_prints_aligned_bars() {
        let mut bars = RangeBars::new(1.0).unwrap();
        bars.update(flat(10.0));
        let down = bars.update(flat(7.0));
        assert_eq!(down.len(), 3);
        assert!(down.iter().all(|b| b.direction == -1));
        assert_relative_eq!(down[2].close, 7.0, epsilon = 1e-12);
    }

    #[test]
    fn reversal_needs_only_one_range() {
        // Unlike Renko, a single-range move against the trend prints immediately.
        let mut bars = RangeBars::new(1.0).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(12.0)); // anchor 12, up
        let down = bars.update(flat(11.0)); // drop of exactly one range
        assert_eq!(down.len(), 1);
        assert_eq!(down[0].direction, -1);
        assert_relative_eq!(down[0].close, 11.0, epsilon = 1e-12);
        assert_eq!(bars.anchor(), Some(11.0));
    }

    #[test]
    fn small_move_prints_nothing() {
        let mut bars = RangeBars::new(1.0).unwrap();
        bars.update(flat(10.0));
        assert_eq!(bars.update(flat(10.5)), Vec::new());
        assert_eq!(bars.anchor(), Some(10.0));
    }

    #[test]
    fn reset_clears_state() {
        let mut bars = RangeBars::new(1.0).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(13.0));
        bars.reset();
        assert_eq!(bars.anchor(), None);
        assert_eq!(bars.update(flat(50.0)), Vec::new());
        assert_eq!(bars.anchor(), Some(50.0));
    }

    #[test]
    fn batch_concatenates_completed_bars() {
        let mut bars = RangeBars::new(1.0).unwrap();
        let candles = [flat(10.0), flat(12.0), flat(13.0)];
        let out = bars.batch(&candles);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|b| b.direction == 1));
    }
}
