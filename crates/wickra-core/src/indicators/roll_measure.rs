//! Roll Measure — effective spread implied by serial covariance of price changes.

use std::collections::VecDeque;

use crate::microstructure::Trade;
use crate::traits::Indicator;
use crate::{Error, Result};

/// Roll Measure — the effective bid-ask spread implied by the negative
/// first-order serial covariance of trade-price changes (Roll, 1984).
///
/// ```text
/// Δpₜ  = priceₜ − priceₜ₋₁
/// γ    = sample lag-1 autocovariance of Δp over the last `period` changes
/// spread = 2 · √(−γ)   if γ < 0,   else 0
/// ```
///
/// Roll's insight: in a frictionless market price changes are serially
/// uncorrelated, but the *bid-ask bounce* — trades alternating between buying at
/// the ask and selling at the bid — induces a **negative** autocovariance whose
/// magnitude pins the spread. The measure recovers an effective spread from
/// trade prices alone, with no quote data. When the serial covariance is
/// non-negative (a trending or frictionless tape) the model implies no spread
/// and the indicator returns `0`.
///
/// `Input = Trade` (only the price is used). Each `update` is `O(period)`: the
/// autocovariance is recomputed from the window of price changes.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Side, Trade, RollMeasure};
///
/// let mut roll = RollMeasure::new(20).unwrap();
/// let mut last = None;
/// // A clean bid-ask bounce of ±0.5 around 100 implies a spread near 1.0.
/// for i in 0..40 {
///     let price = if i % 2 == 0 { 100.0 } else { 101.0 };
///     last = roll.update(Trade::new(price, 1.0, Side::Buy, 0).unwrap());
/// }
/// assert!(last.unwrap() > 0.0);
/// ```
#[derive(Debug, Clone)]
pub struct RollMeasure {
    period: usize,
    prev_price: Option<f64>,
    window: VecDeque<f64>,
    /// Reusable scratch buffer to avoid allocating per `update`.
    scratch: Vec<f64>,
}

impl RollMeasure {
    /// Construct a new Roll Measure over the given window of price changes.
    ///
    /// # Errors
    /// Returns [`Error::InvalidPeriod`] if `period < 3` — the lag-1
    /// autocovariance needs at least two consecutive change pairs.
    pub fn new(period: usize) -> Result<Self> {
        if period < 3 {
            return Err(Error::InvalidPeriod {
                message: "Roll measure needs period >= 3",
            });
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
            scratch: Vec::with_capacity(period),
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for RollMeasure {
    type Input = Trade;
    type Output = f64;

    #[inline]
    fn update(&mut self, trade: Trade) -> Option<f64> {
        let Some(prev) = self.prev_price else {
            self.prev_price = Some(trade.price);
            return None;
        };
        let change = trade.price - prev;
        self.prev_price = Some(trade.price);
        if self.window.len() == self.period {
            self.window.pop_front();
        }
        self.window.push_back(change);
        if self.window.len() < self.period {
            return None;
        }
        // Sample lag-1 autocovariance of the price changes over the window.
        self.scratch.clear();
        self.scratch.extend(self.window.iter().copied());
        let changes = &self.scratch;
        let count = changes.len() as f64;
        let mean = changes.iter().sum::<f64>() / count;
        let pairs = (changes.len() - 1) as f64;
        let mut cov = 0.0;
        for pair in changes.windows(2) {
            cov += (pair[0] - mean) * (pair[1] - mean);
        }
        cov /= pairs;
        let spread = if cov < 0.0 { 2.0 * (-cov).sqrt() } else { 0.0 };
        Some(spread)
    }

    fn reset(&mut self) {
        self.prev_price = None;
        self.window.clear();
        self.scratch.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RollMeasure"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::microstructure::Side;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn trade(price: f64) -> Trade {
        Trade::new(price, 1.0, Side::Buy, 0).unwrap()
    }

    #[test]
    fn rejects_period_below_three() {
        assert!(matches!(
            RollMeasure::new(2),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(RollMeasure::new(3).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let roll = RollMeasure::new(20).unwrap();
        assert_eq!(roll.period(), 20);
        assert_eq!(roll.warmup_period(), 21);
        assert_eq!(roll.name(), "RollMeasure");
        assert!(!roll.is_ready());
    }

    #[test]
    fn bid_ask_bounce_implies_spread() {
        // Prices bounce 100/101 => Δp alternates +1/-1 => mean 0, lag-1
        // autocov = -5/(6-1) = -1 over a 6-change window => spread = 2.
        let mut roll = RollMeasure::new(6).unwrap();
        let prices: Vec<Trade> = (0..20)
            .map(|i| trade(if i % 2 == 0 { 100.0 } else { 101.0 }))
            .collect();
        let last = roll.batch(&prices).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last, 2.0, epsilon = 1e-12);
    }

    #[test]
    fn trending_prices_imply_no_spread() {
        // Monotone prices => constant Δp => zero-centred deviations => cov 0
        // => spread 0.
        let mut roll = RollMeasure::new(6).unwrap();
        let prices: Vec<Trade> = (0..20).map(|i| trade(100.0 + f64::from(i))).collect();
        for v in roll.batch(&prices).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn output_is_non_negative() {
        let mut roll = RollMeasure::new(20).unwrap();
        let prices: Vec<Trade> = (0..200)
            .map(|i| trade(100.0 + (f64::from(i) * 0.7).sin() * 2.0))
            .collect();
        for v in roll.batch(&prices).into_iter().flatten() {
            assert!(v >= 0.0, "spread must be non-negative, got {v}");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut roll = RollMeasure::new(5).unwrap();
        for i in 0..20 {
            roll.update(trade(100.0 + f64::from(i % 2)));
        }
        assert!(roll.is_ready());
        roll.reset();
        assert!(!roll.is_ready());
        assert_eq!(roll.update(trade(100.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<Trade> = (0..80)
            .map(|i| trade(100.0 + (f64::from(i) * 0.6).sin() * 3.0))
            .collect();
        let batch = RollMeasure::new(14).unwrap().batch(&prices);
        let mut b = RollMeasure::new(14).unwrap();
        let streamed: Vec<_> = prices.iter().map(|t| b.update(*t)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            RollMeasure::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
