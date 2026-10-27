//! Order-Book Imbalance over the top-N levels.

use crate::error::{Error, Result};
use crate::microstructure::OrderBook;
use crate::traits::Indicator;

/// Order-Book Imbalance aggregated over the top-N levels of each side.
///
/// Generalises [`crate::OrderBookImbalanceTop1`] to a configurable depth: it
/// sums the resting size of the best `levels` bids and the best `levels` asks
/// and compares them:
///
/// ```text
/// bidDepth  = Σ size of the best `levels` bids
/// askDepth  = Σ size of the best `levels` asks
/// imbalance = (bidDepth − askDepth) / (bidDepth + askDepth)
/// ```
///
/// If a side has fewer than `levels` levels, all available levels are summed.
/// The output lies in `[−1, +1]`; a book with zero size across the summed
/// levels yields `0`.
///
/// `Input = OrderBook`, `Output = f64`. Stateless; ready after the first
/// snapshot.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Level, OrderBook, OrderBookImbalanceTopN};
///
/// let book = OrderBook::new(
///     vec![Level::new(100.0, 2.0).unwrap(), Level::new(99.0, 1.0).unwrap()],
///     vec![Level::new(101.0, 1.0).unwrap(), Level::new(102.0, 1.0).unwrap()],
/// )
/// .unwrap();
/// let mut obi = OrderBookImbalanceTopN::new(2).unwrap();
/// assert_eq!(obi.update(book), Some(0.2)); // (3 − 2) / (3 + 2)
/// ```
#[derive(Debug, Clone)]
pub struct OrderBookImbalanceTopN {
    levels: usize,
    has_emitted: bool,
}

impl OrderBookImbalanceTopN {
    /// Construct a top-N imbalance indicator.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `levels` is zero.
    pub fn new(levels: usize) -> Result<Self> {
        if levels == 0 {
            return Err(Error::PeriodZero);
        }
        if levels > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            levels,
            has_emitted: false,
        })
    }

    /// The configured number of levels summed per side.
    pub fn levels(&self) -> usize {
        self.levels
    }
}

impl Indicator for OrderBookImbalanceTopN {
    type Input = OrderBook;
    type Output = f64;

    #[inline]
    fn update(&mut self, book: OrderBook) -> Option<f64> {
        self.has_emitted = true;
        let bid_depth: f64 = book.bids.iter().take(self.levels).map(|l| l.size).sum();
        let ask_depth: f64 = book.asks.iter().take(self.levels).map(|l| l.size).sum();
        let total = bid_depth + ask_depth;
        if total <= 0.0 {
            return Some(0.0);
        }
        Some((bid_depth - ask_depth) / total)
    }

    fn reset(&mut self) {
        self.has_emitted = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "OrderBookImbalanceTopN"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::microstructure::Level;
    use crate::traits::BatchExt;

    fn book(bids: &[(f64, f64)], asks: &[(f64, f64)]) -> OrderBook {
        let to_levels = |xs: &[(f64, f64)]| {
            xs.iter()
                .map(|&(p, s)| Level::new(p, s).unwrap())
                .collect::<Vec<_>>()
        };
        OrderBook::new(to_levels(bids), to_levels(asks)).unwrap()
    }

    #[test]
    fn rejects_zero_levels() {
        assert!(matches!(
            OrderBookImbalanceTopN::new(0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let obi = OrderBookImbalanceTopN::new(3).unwrap();
        assert_eq!(obi.name(), "OrderBookImbalanceTopN");
        assert_eq!(obi.warmup_period(), 1);
        assert_eq!(obi.levels(), 3);
        assert!(!obi.is_ready());
    }

    #[test]
    fn sums_top_two_levels() {
        let mut obi = OrderBookImbalanceTopN::new(2).unwrap();
        let b = book(&[(100.0, 2.0), (99.0, 1.0)], &[(101.0, 1.0), (102.0, 1.0)]);
        // bidDepth 3, askDepth 2 -> (3 - 2) / 5 = 0.2.
        assert_eq!(obi.update(b), Some(0.2));
        assert!(obi.is_ready());
    }

    #[test]
    fn caps_at_available_depth() {
        // Only one level per side, N = 5 -> uses what exists.
        let mut obi = OrderBookImbalanceTopN::new(5).unwrap();
        assert_eq!(
            obi.update(book(&[(100.0, 3.0)], &[(101.0, 1.0)])),
            Some(0.5)
        );
    }

    #[test]
    fn zero_size_is_zero() {
        let mut obi = OrderBookImbalanceTopN::new(2).unwrap();
        assert_eq!(
            obi.update(book(&[(100.0, 0.0)], &[(101.0, 0.0)])),
            Some(0.0)
        );
    }

    #[test]
    fn batch_equals_streaming() {
        let books: Vec<OrderBook> = (0..20)
            .map(|i| {
                let ask = 1.0 + f64::from(i % 4);
                book(&[(100.0, 2.0), (99.0, 1.0)], &[(101.0, ask), (102.0, 1.0)])
            })
            .collect();
        let mut a = OrderBookImbalanceTopN::new(2).unwrap();
        let mut b = OrderBookImbalanceTopN::new(2).unwrap();
        assert_eq!(
            a.batch(&books),
            books
                .iter()
                .map(|x| b.update(x.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut obi = OrderBookImbalanceTopN::new(2).unwrap();
        obi.update(book(&[(100.0, 1.0)], &[(101.0, 1.0)]));
        assert!(obi.is_ready());
        obi.reset();
        assert!(!obi.is_ready());
    }

    #[test]
    fn rejects_levels_above_maximum() {
        assert!(matches!(
            OrderBookImbalanceTopN::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
