//! Trade-Sign Autocorrelation — lag-1 persistence of the trade-aggressor side.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::microstructure::Trade;
use crate::traits::Indicator;

/// Trade-Sign Autocorrelation — the lag-1 autocorrelation of the **trade sign**
/// (`+1` buy, `−1` sell), measuring how strongly signed order flow persists.
///
/// ```text
/// s_t  = +1 if the trade is a buy, −1 if a sell
/// ρ1   = mean over the window of ( s_t · s_{t−1} )      ∈ [−1, +1]
/// ```
///
/// In real markets trade signs are strongly **positively** autocorrelated: a buy
/// tends to be followed by another buy (and a sell by a sell), because large
/// parent orders are split into many child trades and because of order-splitting
/// and herding. A high reading therefore indicates persistent directional pressure
/// — a footprint of informed or algorithmic execution — while a reading near zero
/// signals balanced, uninformed flow and a negative reading signals alternating
/// (bid-ask bounce) flow.
///
/// The output is the mean product of consecutive signs, bounded in `[−1, +1]`. The
/// first value lands after `period` trades. Each `update` is O(`period`).
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Side, Trade, TradeSignAutocorrelation};
///
/// let mut indicator = TradeSignAutocorrelation::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
///     last = indicator.update(Trade::new(100.0, 1.0, side, i).unwrap());
/// }
/// // Perfectly alternating signs -> autocorrelation -1.
/// assert!((last.unwrap() + 1.0).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct TradeSignAutocorrelation {
    period: usize,
    signs: VecDeque<f64>,
    last: Option<f64>,
}

impl TradeSignAutocorrelation {
    /// Construct a trade-sign autocorrelation over `period` trades.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `period < 2` (a lag-1 product needs two
    /// trades).
    pub fn new(period: usize) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "trade-sign autocorrelation needs period >= 2",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            period,
            signs: VecDeque::with_capacity(period),
            last: None,
        })
    }

    /// Configured window of trades.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for TradeSignAutocorrelation {
    type Input = Trade;
    type Output = f64;

    #[inline]
    fn update(&mut self, trade: Trade) -> Option<f64> {
        if self.signs.len() == self.period {
            self.signs.pop_front();
        }
        self.signs.push_back(trade.side.sign());
        if self.signs.len() < self.period {
            return None;
        }
        let mut product_sum = 0.0;
        let mut prev: Option<f64> = None;
        for &s in &self.signs {
            if let Some(p) = prev {
                product_sum += s * p;
            }
            prev = Some(s);
        }
        let rho = product_sum / (self.period as f64 - 1.0);
        self.last = Some(rho);
        Some(rho)
    }

    fn reset(&mut self) {
        self.signs.clear();
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
        "TradeSignAutocorrelation"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::microstructure::Side;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn buy() -> Trade {
        Trade::new_unchecked(100.0, 1.0, Side::Buy, 0)
    }

    fn sell() -> Trade {
        Trade::new_unchecked(100.0, 1.0, Side::Sell, 0)
    }

    #[test]
    fn rejects_period_below_two() {
        assert!(matches!(
            TradeSignAutocorrelation::new(1),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(TradeSignAutocorrelation::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let t = TradeSignAutocorrelation::new(20).unwrap();
        assert_eq!(t.period(), 20);
        assert_eq!(t.warmup_period(), 20);
        assert_eq!(t.name(), "TradeSignAutocorrelation");
        assert!(!t.is_ready());
        assert_eq!(t.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut t = TradeSignAutocorrelation::new(4).unwrap();
        let out = t.batch(&[buy(), buy(), buy(), buy(), buy()]);
        for v in out.iter().take(3) {
            assert!(v.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn persistent_flow_is_one() {
        let mut t = TradeSignAutocorrelation::new(10).unwrap();
        let trades: Vec<Trade> = (0..20).map(|_| buy()).collect();
        let last = t.batch(&trades).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 1.0, epsilon = 1e-12);
    }

    #[test]
    fn alternating_flow_is_minus_one() {
        let mut t = TradeSignAutocorrelation::new(10).unwrap();
        let trades: Vec<Trade> = (0..20)
            .map(|i| if i % 2 == 0 { buy() } else { sell() })
            .collect();
        let last = t.batch(&trades).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, -1.0, epsilon = 1e-12);
    }

    #[test]
    fn output_in_range() {
        let mut t = TradeSignAutocorrelation::new(16).unwrap();
        let trades: Vec<Trade> = (0..200)
            .map(|i| if (i * 7 % 13) < 6 { buy() } else { sell() })
            .collect();
        for v in t.batch(&trades).into_iter().flatten() {
            assert!((-1.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut t = TradeSignAutocorrelation::new(4).unwrap();
        t.batch(&[buy(), buy(), buy(), buy()]);
        assert!(t.is_ready());
        t.reset();
        assert!(!t.is_ready());
        assert_eq!(t.value(), None);
        assert_eq!(t.update(buy()), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let trades: Vec<Trade> = (0..120)
            .map(|i| if i % 3 == 0 { sell() } else { buy() })
            .collect();
        let batch = TradeSignAutocorrelation::new(16).unwrap().batch(&trades);
        let mut b = TradeSignAutocorrelation::new(16).unwrap();
        let streamed: Vec<_> = trades.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            TradeSignAutocorrelation::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
