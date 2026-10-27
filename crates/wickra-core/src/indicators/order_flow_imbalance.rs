//! Order Flow Imbalance (OFI) from best-level order-book changes.

use std::collections::VecDeque;

use crate::indicators::rolling_moments::RollingSum;
use crate::microstructure::OrderBook;
use crate::traits::Indicator;
use crate::{Error, Result};

/// Order Flow Imbalance — the rolling sum of best-level order-flow events over
/// the last `period` order-book snapshots.
///
/// Following Cont, Kukanov & Stoikov (2014), each new snapshot contributes a
/// signed event from how the best bid and ask moved versus the previous one:
///
/// ```text
/// Δᵇ = qᵇₙ·1{Pᵇₙ ≥ Pᵇₙ₋₁} − qᵇₙ₋₁·1{Pᵇₙ ≤ Pᵇₙ₋₁}     (bid pressure)
/// Δᵃ = qᵃₙ·1{Pᵃₙ ≤ Pᵃₙ₋₁} − qᵃₙ₋₁·1{Pᵃₙ ≥ Pᵃₙ₋₁}     (ask pressure)
/// eₙ = Δᵇ − Δᵃ
/// OFI = Σ eₙ  over the last `period` snapshots
/// ```
///
/// A rising bid (or replenished bid size) and a falling/depleting ask both add
/// positive flow; the mirror subtracts. The rolling sum is a strong
/// short-horizon predictor of price moves: a large positive `OFI` reflects net
/// buying pressure at the top of book, a large negative `OFI` net selling.
///
/// `Input = OrderBook`. Each `update` is O(1) (only the best levels are read).
/// The first snapshot only seeds the reference quotes and emits `None`.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Level, OrderBook, OrderFlowImbalance};
///
/// let mut ofi = OrderFlowImbalance::new(20).unwrap();
/// let book = OrderBook::new(
///     vec![Level::new(100.0, 5.0).unwrap()],
///     vec![Level::new(101.0, 4.0).unwrap()],
/// )
/// .unwrap();
/// assert_eq!(ofi.update(book), None); // first snapshot seeds the reference
/// ```
#[derive(Debug, Clone)]
pub struct OrderFlowImbalance {
    period: usize,
    prev: Option<(f64, f64, f64, f64)>, // (bid_px, bid_sz, ask_px, ask_sz)
    window: VecDeque<f64>,
    sum: RollingSum,
}

impl OrderFlowImbalance {
    /// Construct a new Order Flow Imbalance over the given snapshot window.
    ///
    /// # Errors
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
            prev: None,
            window: VecDeque::with_capacity(period),
            sum: RollingSum::new(),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for OrderFlowImbalance {
    type Input = OrderBook;
    type Output = f64;

    #[inline]
    fn update(&mut self, book: OrderBook) -> Option<f64> {
        // A book with no levels on a side carries no best-level information.
        let (Some(bid), Some(ask)) = (book.best_bid(), book.best_ask()) else {
            return None;
        };
        let curr = (bid.price, bid.size, ask.price, ask.size);
        let Some((pb_px, pb_sz, pa_px, pa_sz)) = self.prev else {
            self.prev = Some(curr);
            return None;
        };
        self.prev = Some(curr);
        let (bid_px, bid_sz, ask_px, ask_sz) = curr;
        // Bid pressure: size added when the bid does not retreat, minus size
        // removed when the bid does not advance.
        let delta_b = f64::from(u8::from(bid_px >= pb_px)) * bid_sz
            - f64::from(u8::from(bid_px <= pb_px)) * pb_sz;
        // Ask pressure: size added when the ask does not advance, minus size
        // removed when the ask does not retreat.
        let delta_a = f64::from(u8::from(ask_px <= pa_px)) * ask_sz
            - f64::from(u8::from(ask_px >= pa_px)) * pa_sz;
        let event = delta_b - delta_a;
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("window is non-empty");
            self.sum.evict(old);
        }
        self.window.push_back(event);
        self.sum.push(event);
        if self.sum.needs_reseed(self.period) {
            self.sum.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        Some(self.sum.value())
    }

    fn reset(&mut self) {
        self.prev = None;
        self.window.clear();
        self.sum.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One snapshot seeds the reference quotes, then `period` events fill the
        // window.
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "OrderFlowImbalance"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::microstructure::Level;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn book(bid_px: f64, bid_sz: f64, ask_px: f64, ask_sz: f64) -> OrderBook {
        OrderBook::new(
            vec![Level::new(bid_px, bid_sz).unwrap()],
            vec![Level::new(ask_px, ask_sz).unwrap()],
        )
        .unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(OrderFlowImbalance::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let ofi = OrderFlowImbalance::new(20).unwrap();
        assert_eq!(ofi.period(), 20);
        assert_eq!(ofi.warmup_period(), 21);
        assert_eq!(ofi.name(), "OrderFlowImbalance");
        assert!(!ofi.is_ready());
    }

    #[test]
    fn first_snapshot_is_none() {
        let mut ofi = OrderFlowImbalance::new(2).unwrap();
        assert_eq!(ofi.update(book(100.0, 5.0, 101.0, 4.0)), None);
    }

    #[test]
    fn empty_book_side_is_none() {
        // A book with no levels on a side (only constructible via
        // `new_unchecked`, since `OrderBook::new` rejects empty sides) carries
        // no best-level information and emits `None` without advancing state.
        let mut ofi = OrderFlowImbalance::new(2).unwrap();
        let empty = OrderBook::new_unchecked(vec![], vec![]);
        assert_eq!(ofi.update(empty), None);
        // A real book afterwards still seeds the reference (state untouched).
        assert_eq!(ofi.update(book(100.0, 5.0, 101.0, 4.0)), None);
    }

    #[test]
    fn rising_bid_adds_positive_flow() {
        // period 1. Reference book, then the bid lifts (price up) with size 6:
        // Δᵇ = 6 (bid_px > prev), Δᵃ = (ask unchanged px=) ask_sz - ask_sz = 0
        // when ask is identical => e = 6.
        let mut ofi = OrderFlowImbalance::new(1).unwrap();
        ofi.update(book(100.0, 5.0, 101.0, 4.0));
        let out = ofi.update(book(100.5, 6.0, 101.0, 4.0)).unwrap();
        assert_relative_eq!(out, 6.0, epsilon = 1e-12);
    }

    #[test]
    fn falling_bid_adds_negative_flow() {
        // The bid drops in price: Δᵇ = −prev_bid_sz (bid_px < prev) = −5,
        // ask identical => Δᵃ = 0 => e = −5.
        let mut ofi = OrderFlowImbalance::new(1).unwrap();
        ofi.update(book(100.0, 5.0, 101.0, 4.0));
        let out = ofi.update(book(99.5, 3.0, 101.0, 4.0)).unwrap();
        assert_relative_eq!(out, -5.0, epsilon = 1e-12);
    }

    #[test]
    fn rolling_sum_accumulates() {
        let mut ofi = OrderFlowImbalance::new(2).unwrap();
        ofi.update(book(100.0, 5.0, 101.0, 4.0));
        let a = ofi.update(book(100.5, 6.0, 101.0, 4.0)); // warming (1 event)
        assert!(a.is_none());
        let b = ofi.update(book(101.0, 2.0, 101.5, 4.0)).unwrap(); // 2 events
                                                                   // Second event: bid_px 101 > 100.5 => Δᵇ = 2; ask_px 101.5 > 101 =>
                                                                   // Δᵃ = −prev_ask_sz = −4 => e2 = 2 − (−4) = 6. Sum = 6 + 6 = 12.
        assert_relative_eq!(b, 12.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut ofi = OrderFlowImbalance::new(2).unwrap();
        ofi.update(book(100.0, 5.0, 101.0, 4.0));
        ofi.update(book(100.5, 6.0, 101.0, 4.0));
        ofi.update(book(101.0, 2.0, 101.5, 4.0));
        assert!(ofi.is_ready());
        ofi.reset();
        assert!(!ofi.is_ready());
        assert_eq!(ofi.update(book(100.0, 5.0, 101.0, 4.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let books: Vec<OrderBook> = (0..30)
            .map(|i| {
                let f = f64::from(i);
                book(
                    100.0 + (f * 0.3).sin(),
                    5.0 + (f * 0.5).cos().abs(),
                    101.0 + (f * 0.3).sin(),
                    4.0 + (f * 0.4).sin().abs(),
                )
            })
            .collect();
        let batch = OrderFlowImbalance::new(10).unwrap().batch(&books);
        let mut b = OrderFlowImbalance::new(10).unwrap();
        let streamed: Vec<_> = books.iter().map(|x| b.update(x.clone())).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            OrderFlowImbalance::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
