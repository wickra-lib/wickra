//! Point-and-Figure bar builder — box-size columns with an N-box reversal.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::BarBuilder;

/// One completed Point-and-Figure column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PnfColumn {
    /// `+1` for a rising (X) column, `-1` for a falling (O) column.
    pub direction: i8,
    /// Upper box edge of the column.
    pub high: f64,
    /// Lower box edge of the column.
    pub low: f64,
}

/// Point-and-Figure bar builder using the fixed box-size, N-box reversal method.
///
/// Price is quantised to a `box_size` grid (each close maps to the box that
/// contains it). An X column extends upward while price makes new box highs; an
/// O column extends downward while price makes new box lows. A reversal needs
/// price to move `reversal` boxes against the column, at which point the current
/// column is closed (returned from [`BarBuilder::update`]) and a new column
/// starts one box offset from the prior extreme.
///
/// - The first candle seeds the grid box and prints no column.
/// - The first one-box move sets the initial column direction.
/// - At most one column completes per candle, so `update` returns an empty
///   vector or a single [`PnfColumn`].
///
/// Closes are mapped to their containing box via `floor(close / box_size)` for
/// both directions, so the construction is fully deterministic.
///
/// # Example
///
/// ```
/// use wickra_core::{BarBuilder, Candle, PointAndFigureBars};
///
/// let flat = |price: f64| Candle::new(price, price, price, price, 1.0, 0).unwrap();
/// let mut pnf = PointAndFigureBars::new(1.0, 3).unwrap();
/// pnf.update(flat(10.0)); // seed
/// pnf.update(flat(15.0)); // X column up to 15
/// let cols = pnf.update(flat(12.0)); // 3-box reversal closes the X column
/// assert_eq!(cols.len(), 1);
/// assert_eq!(cols[0].direction, 1);
/// ```
#[derive(Debug, Clone)]
pub struct PointAndFigureBars {
    box_size: f64,
    reversal: usize,
    dir: i8,
    col_top: f64,
    col_bottom: f64,
    seeded: bool,
}

impl PointAndFigureBars {
    /// Construct a Point-and-Figure builder with the given box size and reversal
    /// (in boxes).
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `box_size` is not finite and positive,
    /// and [`Error::PeriodZero`] if `reversal` is zero.
    pub fn new(box_size: f64, reversal: usize) -> Result<Self> {
        if !box_size.is_finite() || box_size <= 0.0 {
            return Err(Error::InvalidPeriod {
                message: "box_size must be finite and positive",
            });
        }
        if reversal == 0 {
            return Err(Error::PeriodZero);
        }
        if reversal > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            box_size,
            reversal,
            dir: 0,
            col_top: 0.0,
            col_bottom: 0.0,
            seeded: false,
        })
    }

    /// Configured box size.
    pub const fn box_size(&self) -> f64 {
        self.box_size
    }

    /// Configured reversal, in boxes.
    pub const fn reversal(&self) -> usize {
        self.reversal
    }

    fn floor_box(&self, price: f64) -> f64 {
        (price / self.box_size).floor() * self.box_size
    }
}

impl BarBuilder for PointAndFigureBars {
    type Bar = PnfColumn;

    fn update(&mut self, candle: Candle) -> Vec<PnfColumn> {
        let box_price = self.floor_box(candle.close);
        if !self.seeded {
            self.seeded = true;
            self.col_top = box_price;
            self.col_bottom = box_price;
            return Vec::new();
        }
        let box_size = self.box_size;
        let reversal = self.reversal as f64 * box_size;
        let mut cols = Vec::new();
        match self.dir {
            0 => {
                if box_price >= self.col_top + box_size {
                    self.dir = 1;
                    self.col_top = box_price;
                } else if box_price <= self.col_bottom - box_size {
                    self.dir = -1;
                    self.col_bottom = box_price;
                }
            }
            1 => {
                if box_price > self.col_top {
                    self.col_top = box_price;
                } else if box_price <= self.col_top - reversal {
                    cols.push(PnfColumn {
                        direction: 1,
                        high: self.col_top,
                        low: self.col_bottom,
                    });
                    self.dir = -1;
                    self.col_top -= box_size;
                    self.col_bottom = box_price;
                }
            }
            _ => {
                if box_price < self.col_bottom {
                    self.col_bottom = box_price;
                } else if box_price >= self.col_bottom + reversal {
                    cols.push(PnfColumn {
                        direction: -1,
                        high: self.col_top,
                        low: self.col_bottom,
                    });
                    self.dir = 1;
                    self.col_bottom += box_size;
                    self.col_top = box_price;
                }
            }
        }
        cols
    }

    fn reset(&mut self) {
        self.dir = 0;
        self.col_top = 0.0;
        self.col_bottom = 0.0;
        self.seeded = false;
    }

    #[inline]
    fn name(&self) -> &'static str {
        "PointAndFigureBars"
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
            PointAndFigureBars::new(0.0, 3),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            PointAndFigureBars::new(f64::NAN, 3),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn rejects_zero_reversal() {
        assert!(matches!(
            PointAndFigureBars::new(1.0, 0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let pnf = PointAndFigureBars::new(0.5, 3).unwrap();
        assert_eq!(pnf.name(), "PointAndFigureBars");
        assert_relative_eq!(pnf.box_size(), 0.5, epsilon = 1e-12);
        assert_eq!(pnf.reversal(), 3);
    }

    #[test]
    fn first_candle_seeds_without_column() {
        let mut pnf = PointAndFigureBars::new(1.0, 3).unwrap();
        assert_eq!(pnf.update(flat(10.0)), Vec::new());
    }

    #[test]
    fn establishes_up_then_extends() {
        let mut pnf = PointAndFigureBars::new(1.0, 3).unwrap();
        pnf.update(flat(10.0));
        assert_eq!(pnf.update(flat(13.0)), Vec::new()); // start X column
        assert_eq!(pnf.update(flat(15.0)), Vec::new()); // extend up, no completed column
    }

    #[test]
    fn establishes_down_direction() {
        let mut pnf = PointAndFigureBars::new(1.0, 3).unwrap();
        pnf.update(flat(10.0));
        assert_eq!(pnf.update(flat(7.0)), Vec::new()); // start O column
    }

    #[test]
    fn reversal_closes_x_column() {
        let mut pnf = PointAndFigureBars::new(1.0, 3).unwrap();
        pnf.update(flat(10.0));
        pnf.update(flat(13.0));
        pnf.update(flat(15.0));
        let cols = pnf.update(flat(12.0)); // 3-box drop from 15
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].direction, 1);
        assert_relative_eq!(cols[0].high, 15.0, epsilon = 1e-12);
        assert_relative_eq!(cols[0].low, 10.0, epsilon = 1e-12);
    }

    #[test]
    fn reversal_closes_o_column() {
        let mut pnf = PointAndFigureBars::new(1.0, 3).unwrap();
        pnf.update(flat(10.0));
        pnf.update(flat(13.0));
        pnf.update(flat(15.0));
        pnf.update(flat(12.0)); // now O column from 14 down
        pnf.update(flat(10.0)); // extend O down to 10
        let cols = pnf.update(flat(15.0)); // 3-box rise -> closes O column
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].direction, -1);
        assert_relative_eq!(cols[0].low, 10.0, epsilon = 1e-12);
    }

    #[test]
    fn small_move_prints_nothing() {
        let mut pnf = PointAndFigureBars::new(1.0, 3).unwrap();
        pnf.update(flat(10.0));
        pnf.update(flat(13.0));
        pnf.update(flat(15.0));
        assert_eq!(pnf.update(flat(14.0)), Vec::new()); // 1-box pullback < 3
    }

    #[test]
    fn down_column_small_bounce_prints_nothing() {
        let mut pnf = PointAndFigureBars::new(1.0, 3).unwrap();
        pnf.update(flat(10.0));
        pnf.update(flat(7.0)); // O column
        pnf.update(flat(5.0)); // extend down
        assert_eq!(pnf.update(flat(6.0)), Vec::new()); // 1-box bounce < 3
    }

    #[test]
    fn reset_clears_state() {
        let mut pnf = PointAndFigureBars::new(1.0, 3).unwrap();
        pnf.update(flat(10.0));
        pnf.update(flat(15.0));
        pnf.reset();
        assert_eq!(pnf.update(flat(99.0)), Vec::new()); // re-seeds
        assert_eq!(pnf.update(flat(100.0)), Vec::new()); // first move after reseed
    }

    #[test]
    fn batch_collects_completed_columns() {
        let mut pnf = PointAndFigureBars::new(1.0, 3).unwrap();
        let candles = [
            flat(10.0),
            flat(15.0), // X column
            flat(12.0), // reversal -> closes X
            flat(9.0),  // extend O
            flat(15.0), // reversal -> closes O
        ];
        let cols = pnf.batch(&candles);
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].direction, 1);
        assert_eq!(cols[1].direction, -1);
    }
}
