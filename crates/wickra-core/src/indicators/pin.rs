//! PIN — Probability of Informed Trading (single-window EKOP estimate).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::microstructure::Trade;
use crate::traits::Indicator;

/// PIN — the **Probability of Informed Trading**, estimated from the buy/sell order
/// imbalance over a rolling window of trades.
///
/// ```text
/// over the last `window` trades: B = buys, S = sells   (B + S = window)
/// PIN ≈ |B − S| / (B + S)        ∈ [0, 1]
/// ```
///
/// The Easley-Kiefer-O'Hara-Paperman (EKOP) model splits order flow into an
/// uninformed component (balanced buys and sells, rate `ε` per side) and an
/// informed component that trades one-directionally when private information
/// arrives (rate `μ`, probability `α`). The probability that any given trade is
/// information-motivated is `PIN = αμ / (αμ + 2ε)`. Estimated over a single window,
/// the informed flow shows up as the **net imbalance** `|B − S|` and the uninformed
/// flow as the balanced remainder, giving the moment estimator above. A high PIN
/// flags a one-sided, likely-informed market; a low PIN flags balanced, uninformed
/// flow.
///
/// This is distinct from [`Vpin`](crate::Vpin), the volume-synchronised variant
/// that buckets by volume and uses bulk-volume classification; here trades are
/// counted in event time and classified by their tagged aggressor side. The full
/// PIN is fit by maximum likelihood over many periods — this single-window
/// estimator is the streaming moment approximation. The output is in `[0, 1]`; the
/// first value lands after `window` trades.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Pin, Side, Trade};
///
/// let mut indicator = Pin::new(20).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     // All buys -> maximally one-sided -> PIN 1.
///     last = indicator.update(Trade::new(100.0, 1.0, Side::Buy, i).unwrap());
/// }
/// assert!((last.unwrap() - 1.0).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct Pin {
    window: usize,
    sides: VecDeque<f64>,
    buy_count: usize,
    last: Option<f64>,
}

impl Pin {
    /// Construct a PIN estimator over `window` trades.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `window == 0`.
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
            sides: VecDeque::with_capacity(window),
            buy_count: 0,
            last: None,
        })
    }

    /// Configured window of trades.
    pub const fn window(&self) -> usize {
        self.window
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for Pin {
    type Input = Trade;
    type Output = f64;

    #[inline]
    fn update(&mut self, trade: Trade) -> Option<f64> {
        let is_buy = trade.side.sign() > 0.0;
        if self.sides.len() == self.window {
            let old = self.sides.pop_front().expect("non-empty");
            if old > 0.0 {
                self.buy_count -= 1;
            }
        }
        self.sides.push_back(if is_buy { 1.0 } else { 0.0 });
        if is_buy {
            self.buy_count += 1;
        }
        if self.sides.len() < self.window {
            return None;
        }
        // The window is full and `window >= 1` (zero is rejected at
        // construction), so the trade count is always positive — `|B - S| / N`
        // needs no zero guard.
        let buys = self.buy_count as f64;
        let sells = self.window as f64 - buys;
        let total = self.window as f64;
        let pin = (buys - sells).abs() / total;
        self.last = Some(pin);
        Some(pin)
    }

    fn reset(&mut self) {
        self.sides.clear();
        self.buy_count = 0;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.window
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "PIN"
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
    fn rejects_zero_window() {
        assert!(matches!(Pin::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let p = Pin::new(20).unwrap();
        assert_eq!(p.window(), 20);
        assert_eq!(p.warmup_period(), 20);
        assert_eq!(p.name(), "PIN");
        assert!(!p.is_ready());
        assert_eq!(p.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut p = Pin::new(4).unwrap();
        let out = p.batch(&[buy(), buy(), buy(), buy(), buy()]);
        for v in out.iter().take(3) {
            assert!(v.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn one_sided_flow_is_one() {
        let mut p = Pin::new(10).unwrap();
        let trades: Vec<Trade> = (0..20).map(|_| buy()).collect();
        let last = p.batch(&trades).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 1.0, epsilon = 1e-12);
    }

    #[test]
    fn balanced_flow_is_zero() {
        let mut p = Pin::new(10).unwrap();
        let trades: Vec<Trade> = (0..20)
            .map(|i| if i % 2 == 0 { buy() } else { sell() })
            .collect();
        let last = p.batch(&trades).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn output_in_range() {
        let mut p = Pin::new(16).unwrap();
        let trades: Vec<Trade> = (0..200)
            .map(|i| if (i * 5 % 13) < 8 { buy() } else { sell() })
            .collect();
        for v in p.batch(&trades).into_iter().flatten() {
            assert!((0.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut p = Pin::new(4).unwrap();
        p.batch(&[buy(), buy(), sell(), buy()]);
        assert!(p.is_ready());
        p.reset();
        assert!(!p.is_ready());
        assert_eq!(p.value(), None);
        assert_eq!(p.update(buy()), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let trades: Vec<Trade> = (0..120)
            .map(|i| if i % 3 == 0 { sell() } else { buy() })
            .collect();
        let batch = Pin::new(16).unwrap().batch(&trades);
        let mut b = Pin::new(16).unwrap();
        let streamed: Vec<_> = trades.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_window_above_maximum() {
        assert!(matches!(
            Pin::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
