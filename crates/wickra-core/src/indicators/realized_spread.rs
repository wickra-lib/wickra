//! Realized Spread — the post-trade liquidity revenue of a trade in basis
//! points.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::microstructure::TradeQuote;
use crate::traits::Indicator;

/// Realized Spread — twice the signed deviation of a trade price from the mid
/// that prevails `horizon` trades *later*, expressed in basis points of the
/// trade's contemporaneous mid.
///
/// ```text
/// realizedSpread = 2 · D · (tradePrice − mid_{t+horizon}) / mid_t · 10_000   (bps)
/// ```
///
/// where `D` is the aggressor sign (`+1` for a buy, `−1` for a sell), `mid_t`
/// is the mid at the time of the trade, and `mid_{t+horizon}` is the mid
/// `horizon` trade-quotes later. Where the [effective spread] measures the full
/// cost paid by the aggressor against the contemporaneous mid, the realized
/// spread measures the share of that cost a liquidity provider *keeps* after
/// the mid has moved: it is the effective spread net of the price impact
/// (`effective = realized + 2 · priceImpact`). A high realized spread means
/// the quote was not picked off; a low or negative one is the signature of
/// adverse selection, the trade preceding a move in its own direction.
///
/// The indicator buffers each incoming trade-quote and emits the realized
/// spread for the trade made `horizon` updates ago, once that future mid is
/// known. It warms up for `horizon + 1` trade-quotes — `update` returns `None`
/// until the first trade can be resolved — and then emits one value per update
/// in O(1).
///
/// `Input = TradeQuote`, `Output = f64`.
///
/// [effective spread]: crate::EffectiveSpread
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, RealizedSpread, Side, Trade, TradeQuote};
///
/// let mut rs = RealizedSpread::new(1).unwrap();
/// let tq = |price: f64, side, mid| TradeQuote::new(Trade::new(price, 1.0, side, 0).unwrap(), mid).unwrap();
/// // First trade buffered; nothing to resolve yet.
/// assert_eq!(rs.update(tq(100.10, Side::Buy, 100.0)), None);
/// // One trade later the mid is 100.20, resolving the first buy:
/// // 2 · (+1) · (100.10 − 100.20) / 100.0 · 10_000 = −20 bps (adverse selection).
/// let out = rs.update(tq(99.90, Side::Sell, 100.20)).unwrap();
/// assert!((out - (-20.0)).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct RealizedSpread {
    horizon: usize,
    // Each pending entry is (aggressor sign, trade price, contemporaneous mid).
    pending: VecDeque<(f64, f64, f64)>,
    has_emitted: bool,
}

impl RealizedSpread {
    /// Construct a realized-spread indicator that resolves each trade against
    /// the mid `horizon` trade-quotes later.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `horizon` is zero (the realized spread
    /// is defined against a strictly future mid).
    pub fn new(horizon: usize) -> Result<Self> {
        if horizon == 0 {
            return Err(Error::PeriodZero);
        }
        if horizon > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            horizon,
            pending: VecDeque::with_capacity(horizon + 1),
            has_emitted: false,
        })
    }

    /// The configured horizon, in trade-quotes.
    pub const fn horizon(&self) -> usize {
        self.horizon
    }
}

impl Indicator for RealizedSpread {
    type Input = TradeQuote;
    type Output = f64;

    #[inline]
    fn update(&mut self, quote: TradeQuote) -> Option<f64> {
        let sign = quote.trade.side.sign();
        self.pending.push_back((sign, quote.trade.price, quote.mid));
        if self.pending.len() <= self.horizon {
            return None;
        }
        let (old_sign, old_price, old_mid) = self.pending.pop_front().expect("len > horizon >= 1");
        self.has_emitted = true;
        // `quote.mid` is the mid prevailing `horizon` trades after the resolved
        // trade; normalise by that trade's own contemporaneous mid.
        Some(2.0 * old_sign * (old_price - quote.mid) / old_mid * 10_000.0)
    }

    fn reset(&mut self) {
        self.pending.clear();
        self.has_emitted = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.horizon + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.has_emitted
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RealizedSpread"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::microstructure::{Side, Trade};
    use crate::traits::BatchExt;

    fn tq(price: f64, side: Side, mid: f64) -> TradeQuote {
        TradeQuote::new(Trade::new(price, 1.0, side, 0).unwrap(), mid).unwrap()
    }

    #[test]
    fn rejects_zero_horizon() {
        assert!(matches!(RealizedSpread::new(0), Err(Error::PeriodZero)));
        assert!(RealizedSpread::new(1).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let rs = RealizedSpread::new(3).unwrap();
        assert_eq!(rs.name(), "RealizedSpread");
        assert_eq!(rs.horizon(), 3);
        assert_eq!(rs.warmup_period(), 4);
        assert!(!rs.is_ready());
    }

    #[test]
    fn resolves_against_future_mid() {
        let mut rs = RealizedSpread::new(1).unwrap();
        assert_eq!(rs.update(tq(100.10, Side::Buy, 100.0)), None);
        assert!(!rs.is_ready());
        // 2 · (+1) · (100.10 − 100.20) / 100.0 · 10_000 = −20 bps.
        let out = rs.update(tq(99.90, Side::Sell, 100.20)).unwrap();
        assert!((out - (-20.0)).abs() < 1e-9);
        assert!(rs.is_ready());
    }

    #[test]
    fn no_adverse_move_equals_effective_spread() {
        // If the mid does not move over the horizon, realized == effective.
        let mut rs = RealizedSpread::new(1).unwrap();
        rs.update(tq(100.05, Side::Buy, 100.0));
        // mid stays at 100.0 -> 2 · (100.05 − 100.0) / 100.0 · 10_000 = 10 bps.
        let out = rs.update(tq(100.0, Side::Buy, 100.0)).unwrap();
        assert!((out - 10.0).abs() < 1e-9);
    }

    #[test]
    fn longer_horizon_warms_up() {
        let mut rs = RealizedSpread::new(3).unwrap();
        for _ in 0..3 {
            assert_eq!(rs.update(tq(100.0, Side::Buy, 100.0)), None);
        }
        assert!(!rs.is_ready());
        assert!(rs.update(tq(100.0, Side::Buy, 100.0)).is_some());
        assert!(rs.is_ready());
    }

    #[test]
    fn batch_equals_streaming() {
        let quotes: Vec<TradeQuote> = (0..30)
            .map(|i| {
                let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
                let mid = 100.0 + f64::from(i % 5) * 0.05;
                tq(mid + 0.02, side, mid)
            })
            .collect();
        let mut a = RealizedSpread::new(4).unwrap();
        let mut b = RealizedSpread::new(4).unwrap();
        assert_eq!(
            a.batch(&quotes),
            quotes.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut rs = RealizedSpread::new(1).unwrap();
        rs.update(tq(100.05, Side::Buy, 100.0));
        rs.update(tq(100.0, Side::Buy, 100.0));
        assert!(rs.is_ready());
        rs.reset();
        assert!(!rs.is_ready());
        assert_eq!(rs.update(tq(100.05, Side::Buy, 100.0)), None);
    }

    #[test]
    fn rejects_horizon_above_maximum() {
        assert!(matches!(
            RealizedSpread::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
