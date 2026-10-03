//! Renko bar builder — fixed box-size bricks with the classic reversal rule.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::BarBuilder;

/// One completed Renko brick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenkoBrick {
    /// Price at the brick's origin edge.
    pub open: f64,
    /// Price at the brick's far edge (`open ± box_size`).
    pub close: f64,
    /// `+1` for an up brick, `-1` for a down brick.
    pub direction: i8,
}

/// Renko bar builder using the fixed box-size method on close prices.
///
/// Construction follows the classic Renko rules:
///
/// - The first candle seeds the reference level and prints no brick.
/// - While the trend continues, every additional `box_size` of close movement
///   prints one more brick in the trend direction.
/// - A reversal requires `2 * box_size` against the trend (one box to unwind the
///   last brick's body, one to print the first opposite brick); thereafter each
///   further `box_size` prints another brick.
///
/// A single candle whose close gaps several boxes prints all the bricks it
/// completes in one [`BarBuilder::update`] call. Bricks are perfectly aligned to
/// the `box_size` grid relative to the seed price.
///
/// # Example
///
/// ```
/// use wickra_core::{BarBuilder, Candle, RenkoBars};
///
/// let flat = |price: f64| Candle::new(price, price, price, price, 1.0, 0).unwrap();
/// let mut renko = RenkoBars::new(1.0).unwrap();
/// assert!(renko.update(flat(10.0)).is_empty()); // seed
/// let bricks = renko.update(flat(13.0)); // +3 boxes
/// assert_eq!(bricks.len(), 3);
/// assert!(bricks.iter().all(|b| b.direction == 1));
/// ```
#[derive(Debug, Clone)]
pub struct RenkoBars {
    box_size: f64,
    level: Option<f64>,
    dir: i8,
}

impl RenkoBars {
    /// Construct a Renko builder with the given brick size.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `box_size` is not finite and positive.
    pub fn new(box_size: f64) -> Result<Self> {
        if !box_size.is_finite() || box_size <= 0.0 {
            return Err(Error::InvalidPeriod {
                message: "box_size must be finite and positive",
            });
        }
        Ok(Self {
            box_size,
            level: None,
            dir: 0,
        })
    }

    /// Configured brick size.
    pub const fn box_size(&self) -> f64 {
        self.box_size
    }

    /// Current reference level (the close of the last completed brick, or the
    /// seed price before any brick has formed).
    pub const fn level(&self) -> Option<f64> {
        self.level
    }
}

impl BarBuilder for RenkoBars {
    type Bar = RenkoBrick;

    fn update(&mut self, candle: Candle) -> Vec<RenkoBrick> {
        let close = candle.close;
        let Some(mut level) = self.level else {
            self.level = Some(close);
            return Vec::new();
        };
        let box_size = self.box_size;
        let two = 2.0 * box_size;
        let mut bricks = Vec::new();
        loop {
            if self.dir >= 0 && close >= level + box_size {
                bricks.push(RenkoBrick {
                    open: level,
                    close: level + box_size,
                    direction: 1,
                });
                level += box_size;
                self.dir = 1;
            } else if self.dir <= 0 && close <= level - box_size {
                bricks.push(RenkoBrick {
                    open: level,
                    close: level - box_size,
                    direction: -1,
                });
                level -= box_size;
                self.dir = -1;
            } else if self.dir > 0 && close <= level - two {
                bricks.push(RenkoBrick {
                    open: level - box_size,
                    close: level - two,
                    direction: -1,
                });
                level -= two;
                self.dir = -1;
            } else if self.dir < 0 && close >= level + two {
                bricks.push(RenkoBrick {
                    open: level + box_size,
                    close: level + two,
                    direction: 1,
                });
                level += two;
                self.dir = 1;
            } else {
                break;
            }
        }
        self.level = Some(level);
        bricks
    }

    fn reset(&mut self) {
        self.level = None;
        self.dir = 0;
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RenkoBars"
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
    fn rejects_invalid_box_size() {
        assert!(matches!(
            RenkoBars::new(0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            RenkoBars::new(-1.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            RenkoBars::new(f64::NAN),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let renko = RenkoBars::new(2.5).unwrap();
        assert_eq!(renko.name(), "RenkoBars");
        assert_relative_eq!(renko.box_size(), 2.5, epsilon = 1e-12);
        assert_eq!(renko.level(), None);
    }

    #[test]
    fn first_candle_seeds_without_brick() {
        let mut renko = RenkoBars::new(1.0).unwrap();
        assert_eq!(renko.update(flat(10.0)), Vec::new());
        assert_eq!(renko.level(), Some(10.0));
    }

    #[test]
    fn up_trend_prints_aligned_bricks() {
        let mut renko = RenkoBars::new(1.0).unwrap();
        renko.update(flat(10.0));
        let bricks = renko.update(flat(13.0));
        assert_eq!(bricks.len(), 3);
        assert_relative_eq!(bricks[0].open, 10.0, epsilon = 1e-12);
        assert_relative_eq!(bricks[0].close, 11.0, epsilon = 1e-12);
        assert_relative_eq!(bricks[2].close, 13.0, epsilon = 1e-12);
        assert!(bricks.iter().all(|b| b.direction == 1));
        assert_eq!(renko.level(), Some(13.0));
    }

    #[test]
    fn down_trend_prints_aligned_bricks() {
        let mut renko = RenkoBars::new(1.0).unwrap();
        renko.update(flat(10.0));
        let bricks = renko.update(flat(7.0));
        assert_eq!(bricks.len(), 3);
        assert!(bricks.iter().all(|b| b.direction == -1));
        assert_relative_eq!(bricks[2].close, 7.0, epsilon = 1e-12);
        assert_eq!(renko.level(), Some(7.0));
    }

    #[test]
    fn reversal_down_needs_two_boxes() {
        let mut renko = RenkoBars::new(1.0).unwrap();
        renko.update(flat(10.0));
        renko.update(flat(13.0)); // level 13, dir up
        let bricks = renko.update(flat(10.0)); // drop of 3 -> reversal eats one box
        assert_eq!(bricks.len(), 2);
        assert!(bricks.iter().all(|b| b.direction == -1));
        assert_relative_eq!(bricks[0].open, 12.0, epsilon = 1e-12);
        assert_relative_eq!(bricks[0].close, 11.0, epsilon = 1e-12);
        assert_relative_eq!(bricks[1].close, 10.0, epsilon = 1e-12);
        assert_eq!(renko.level(), Some(10.0));
    }

    #[test]
    fn reversal_up_needs_two_boxes() {
        let mut renko = RenkoBars::new(1.0).unwrap();
        renko.update(flat(10.0));
        renko.update(flat(7.0)); // level 7, dir down
        let bricks = renko.update(flat(10.0)); // rise of 3 -> reversal
        assert_eq!(bricks.len(), 2);
        assert!(bricks.iter().all(|b| b.direction == 1));
        assert_relative_eq!(bricks[0].open, 8.0, epsilon = 1e-12);
        assert_relative_eq!(bricks[0].close, 9.0, epsilon = 1e-12);
        assert_relative_eq!(bricks[1].close, 10.0, epsilon = 1e-12);
    }

    #[test]
    fn small_move_prints_nothing() {
        let mut renko = RenkoBars::new(1.0).unwrap();
        renko.update(flat(10.0));
        renko.update(flat(13.0));
        assert_eq!(renko.update(flat(12.5)), Vec::new()); // less than a reversal
        assert_eq!(renko.level(), Some(13.0));
    }

    #[test]
    fn reset_clears_state() {
        let mut renko = RenkoBars::new(1.0).unwrap();
        renko.update(flat(10.0));
        renko.update(flat(13.0));
        renko.reset();
        assert_eq!(renko.level(), None);
        // After reset the next candle seeds again.
        assert_eq!(renko.update(flat(50.0)), Vec::new());
        assert_eq!(renko.level(), Some(50.0));
    }

    #[test]
    fn batch_concatenates_completed_bricks() {
        let mut renko = RenkoBars::new(1.0).unwrap();
        let candles = [flat(10.0), flat(12.0), flat(13.0)];
        let bricks = renko.batch(&candles);
        // seed at 10, then +2 then +1 => 3 up bricks total.
        assert_eq!(bricks.len(), 3);
        assert!(bricks.iter().all(|b| b.direction == 1));
    }
}
