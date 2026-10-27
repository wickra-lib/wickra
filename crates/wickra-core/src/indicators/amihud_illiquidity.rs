//! Amihud Illiquidity — average price impact per unit traded value.

use std::collections::VecDeque;

use crate::indicators::rolling_moments::RollingSum;
use crate::microstructure::Trade;
use crate::traits::Indicator;
use crate::{Error, Result};

/// Amihud Illiquidity — the average absolute log return per unit of traded
/// value over the last `period` trades (Amihud, 2002).
///
/// ```text
/// rₜ      = ln(priceₜ / priceₜ₋₁)
/// ILLIQₜ  = |rₜ| / (priceₜ · sizeₜ)        (return per dollar of volume)
/// Amihud  = mean of ILLIQ over the last `period` trades
/// ```
///
/// Amihud's measure captures how much the price moves for a given amount of
/// traded value: a **high** reading means small volume already shifts the price
/// a lot (an illiquid, easily-moved market), a **low** reading means it takes
/// large volume to move the price (a deep, liquid market). It is the workhorse
/// cross-sectional liquidity proxy in market-microstructure research.
///
/// `Input = Trade`. Trades with zero size carry no traded value and are skipped
/// (the ratio is undefined); the last value is returned and state is untouched.
/// The first valid trade only seeds the reference price.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Side, Trade, AmihudIlliquidity};
///
/// let mut amihud = AmihudIlliquidity::new(20).unwrap();
/// assert_eq!(amihud.update(Trade::new(100.0, 5.0, Side::Buy, 0).unwrap()), None);
/// ```
#[derive(Debug, Clone)]
pub struct AmihudIlliquidity {
    period: usize,
    prev_price: Option<f64>,
    window: VecDeque<f64>,
    sum: RollingSum,
    last: Option<f64>,
}

impl AmihudIlliquidity {
    /// Construct a new Amihud Illiquidity over the given trade window.
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
            prev_price: None,
            window: VecDeque::with_capacity(period),
            sum: RollingSum::new(),
            last: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for AmihudIlliquidity {
    type Input = Trade;
    type Output = f64;

    #[inline]
    fn update(&mut self, trade: Trade) -> Option<f64> {
        // A zero-size trade has no traded value: the ratio is undefined, so the
        // trade is skipped without touching the reference price.
        if trade.size == 0.0 {
            return self.last;
        }
        let Some(prev) = self.prev_price else {
            self.prev_price = Some(trade.price);
            return None;
        };
        self.prev_price = Some(trade.price);
        // `prev` and `trade.price` are both finite and strictly positive
        // (enforced by `Trade::new`), so the log return is well-defined and the
        // traded value is strictly positive.
        let ret = (trade.price / prev).ln().abs();
        let illiq = ret / (trade.price * trade.size);
        if self.window.len() == self.period {
            let old = self.window.pop_front().expect("window is non-empty");
            self.sum.evict(old);
        }
        self.window.push_back(illiq);
        self.sum.push(illiq);
        if self.sum.needs_reseed(self.period) {
            self.sum.reseed(self.window.iter().copied());
        }
        if self.window.len() < self.period {
            return None;
        }
        let value = self.sum.value() / self.period as f64;
        self.last = Some(value);
        Some(value)
    }

    fn reset(&mut self) {
        self.prev_price = None;
        self.window.clear();
        self.sum.reset();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "AmihudIlliquidity"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::microstructure::Side;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn trade(price: f64, size: f64) -> Trade {
        Trade::new(price, size, Side::Buy, 0).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(AmihudIlliquidity::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let a = AmihudIlliquidity::new(20).unwrap();
        assert_eq!(a.period(), 20);
        assert_eq!(a.warmup_period(), 21);
        assert_eq!(a.name(), "AmihudIlliquidity");
        assert!(!a.is_ready());
    }

    #[test]
    fn known_value() {
        // period 1. Seed at 100, then 101 with size 10:
        // |ln(101/100)| / (101 * 10).
        let mut a = AmihudIlliquidity::new(1).unwrap();
        assert_eq!(a.update(trade(100.0, 10.0)), None);
        let out = a.update(trade(101.0, 10.0)).unwrap();
        let expected = (101.0_f64 / 100.0).ln().abs() / (101.0 * 10.0);
        assert_relative_eq!(out, expected, epsilon = 1e-15);
    }

    #[test]
    fn higher_for_thinner_volume() {
        // Same price move on smaller volume => larger illiquidity reading.
        let thin = {
            let mut a = AmihudIlliquidity::new(1).unwrap();
            a.update(trade(100.0, 1.0));
            a.update(trade(101.0, 1.0)).unwrap()
        };
        let thick = {
            let mut a = AmihudIlliquidity::new(1).unwrap();
            a.update(trade(100.0, 1000.0));
            a.update(trade(101.0, 1000.0)).unwrap()
        };
        assert!(thin > thick, "thin {thin} should exceed thick {thick}");
    }

    #[test]
    fn flat_price_is_zero() {
        let mut a = AmihudIlliquidity::new(5).unwrap();
        for v in a.batch(&[trade(100.0, 3.0); 20]).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-15);
        }
    }

    #[test]
    fn skips_zero_size_trades() {
        let mut a = AmihudIlliquidity::new(1).unwrap();
        a.update(trade(100.0, 10.0));
        let baseline = a.update(trade(101.0, 10.0)).unwrap();
        // A zero-size trade is ignored; the previous reference price is kept.
        assert_eq!(a.update(trade(200.0, 0.0)), Some(baseline));
        // The next real trade still references price 101, not 200.
        let mut control = a.clone();
        let after = a.update(trade(102.0, 10.0)).unwrap();
        assert_eq!(control.update(trade(102.0, 10.0)).unwrap(), after);
    }

    #[test]
    fn output_is_non_negative() {
        let mut a = AmihudIlliquidity::new(10).unwrap();
        let trades: Vec<Trade> = (0..100)
            .map(|i| {
                trade(
                    100.0 + (f64::from(i) * 0.3).sin() * 5.0,
                    1.0 + f64::from(i % 7),
                )
            })
            .collect();
        for v in a.batch(&trades).into_iter().flatten() {
            assert!(v >= 0.0, "illiquidity must be non-negative, got {v}");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut a = AmihudIlliquidity::new(5).unwrap();
        for i in 0..20 {
            a.update(trade(100.0 + f64::from(i), 2.0));
        }
        assert!(a.is_ready());
        a.reset();
        assert!(!a.is_ready());
        assert_eq!(a.update(trade(100.0, 1.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let trades: Vec<Trade> = (0..80)
            .map(|i| {
                trade(
                    100.0 + (f64::from(i) * 0.25).sin() * 4.0,
                    1.0 + f64::from(i % 5),
                )
            })
            .collect();
        let batch = AmihudIlliquidity::new(14).unwrap().batch(&trades);
        let mut b = AmihudIlliquidity::new(14).unwrap();
        let streamed: Vec<_> = trades.iter().map(|t| b.update(*t)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            AmihudIlliquidity::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
