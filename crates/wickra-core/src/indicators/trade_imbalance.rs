//! Trade Imbalance — rolling buy/sell volume imbalance over a trade window.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::microstructure::Trade;
use crate::traits::Indicator;

/// Trade Imbalance — the signed buy/sell volume imbalance over the trailing
/// window of `window` trades.
///
/// ```text
/// buyVol  = Σ size of buyer-initiated trades in the window
/// sellVol = Σ size of seller-initiated trades in the window
/// imbalance = (buyVol − sellVol) / (buyVol + sellVol)
/// ```
///
/// The output lies in `[−1, +1]`: `+1` means the window was all aggressive
/// buying, `−1` all aggressive selling, `0` balanced (or no volume). The
/// indicator warms up for `window` trades — `update` returns `None` until the
/// window is full — then emits the rolling imbalance, maintained in O(1) per
/// trade.
///
/// `Input = Trade`, `Output = f64`.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Side, Trade, TradeImbalance};
///
/// let mut ti = TradeImbalance::new(2).unwrap();
/// assert_eq!(ti.update(Trade::new(100.0, 3.0, Side::Buy, 0).unwrap()), None);
/// // Window full: buyVol 3, sellVol 1 -> (3 - 1) / 4 = 0.5.
/// let out = ti.update(Trade::new(100.0, 1.0, Side::Sell, 1).unwrap());
/// assert_eq!(out, Some(0.5));
/// ```
#[derive(Debug, Clone)]
pub struct TradeImbalance {
    window: usize,
    history: VecDeque<(f64, f64)>,
    buy_sum: f64,
    sell_sum: f64,
}

impl TradeImbalance {
    /// Construct a trade-imbalance indicator over a window of `window` trades.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `window` is zero.
    pub fn new(window: usize) -> Result<Self> {
        if window == 0 {
            return Err(Error::PeriodZero);
        }
        if window > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            window,
            history: VecDeque::with_capacity(window),
            buy_sum: 0.0,
            sell_sum: 0.0,
        })
    }

    /// The configured window length, in trades.
    pub fn window(&self) -> usize {
        self.window
    }
}

impl Indicator for TradeImbalance {
    type Input = Trade;
    type Output = f64;

    #[inline]
    fn update(&mut self, trade: Trade) -> Option<f64> {
        let (buy, sell) = if trade.side.sign() > 0.0 {
            (trade.size, 0.0)
        } else {
            (0.0, trade.size)
        };
        self.history.push_back((buy, sell));
        self.buy_sum += buy;
        self.sell_sum += sell;
        if self.history.len() > self.window {
            let (old_buy, old_sell) = self.history.pop_front().expect("window >= 1, len > window");
            self.buy_sum -= old_buy;
            self.sell_sum -= old_sell;
        }
        if self.history.len() < self.window {
            return None;
        }
        let total = self.buy_sum + self.sell_sum;
        if total <= 0.0 {
            return Some(0.0);
        }
        Some((self.buy_sum - self.sell_sum) / total)
    }

    fn reset(&mut self) {
        self.history.clear();
        self.buy_sum = 0.0;
        self.sell_sum = 0.0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.window
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.history.len() >= self.window
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TradeImbalance"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::microstructure::Side;
    use crate::traits::BatchExt;

    fn trade(size: f64, side: Side, ts: i64) -> Trade {
        Trade::new(100.0, size, side, ts).unwrap()
    }

    #[test]
    fn rejects_zero_window() {
        assert!(matches!(TradeImbalance::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let ti = TradeImbalance::new(5).unwrap();
        assert_eq!(ti.name(), "TradeImbalance");
        assert_eq!(ti.warmup_period(), 5);
        assert_eq!(ti.window(), 5);
        assert!(!ti.is_ready());
    }

    #[test]
    fn warms_up_then_emits() {
        let mut ti = TradeImbalance::new(2).unwrap();
        assert_eq!(ti.update(trade(3.0, Side::Buy, 0)), None);
        assert!(!ti.is_ready());
        // Window full: buyVol 3, sellVol 1 -> 0.5.
        assert_eq!(ti.update(trade(1.0, Side::Sell, 1)), Some(0.5));
        assert!(ti.is_ready());
    }

    #[test]
    fn rolls_off_old_trades() {
        let mut ti = TradeImbalance::new(2).unwrap();
        ti.update(trade(3.0, Side::Buy, 0));
        ti.update(trade(1.0, Side::Sell, 1)); // [buy 3, sell 1] -> 0.5
                                              // Third trade drops the first: window now [sell 1, buy 5] -> (5-1)/6.
        let out = ti.update(trade(5.0, Side::Buy, 2)).unwrap();
        assert!((out - (4.0 / 6.0)).abs() < 1e-12);
    }

    #[test]
    fn zero_volume_window_is_zero() {
        let mut ti = TradeImbalance::new(2).unwrap();
        ti.update(trade(0.0, Side::Buy, 0));
        assert_eq!(ti.update(trade(0.0, Side::Sell, 1)), Some(0.0));
    }

    #[test]
    fn batch_equals_streaming() {
        let trades: Vec<Trade> = (0..30)
            .map(|i| {
                let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
                trade(1.0 + (i % 5) as f64, side, i)
            })
            .collect();
        let mut a = TradeImbalance::new(5).unwrap();
        let mut b = TradeImbalance::new(5).unwrap();
        assert_eq!(
            a.batch(&trades),
            trades.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut ti = TradeImbalance::new(2).unwrap();
        ti.update(trade(3.0, Side::Buy, 0));
        ti.update(trade(1.0, Side::Sell, 1));
        assert!(ti.is_ready());
        ti.reset();
        assert!(!ti.is_ready());
        assert_eq!(ti.update(trade(2.0, Side::Buy, 2)), None);
    }

    #[test]
    fn rejects_window_above_maximum() {
        assert!(matches!(
            TradeImbalance::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
