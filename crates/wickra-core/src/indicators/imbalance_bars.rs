//! Tick-imbalance bar builder (simplified López de Prado) — sample on cumulative signed order flow.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::BarBuilder;

/// One completed imbalance bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImbalanceBar {
    /// Open of the first candle in the bar.
    pub open: f64,
    /// Highest high across the bar.
    pub high: f64,
    /// Lowest low across the bar.
    pub low: f64,
    /// Close of the candle that closed the bar.
    pub close: f64,
    /// Signed cumulative tick imbalance at the close (`Σ sign`).
    pub imbalance: f64,
    /// `+1` if buy-side imbalance closed the bar, `-1` if sell-side.
    pub direction: i8,
}

/// Tick-imbalance bar builder — a **simplified** form of López de Prado's
/// imbalance bars.
///
/// Each candle is assigned a tick sign by the tick rule: `+1` if its close is above
/// the previous close, `-1` if below, and the previous sign is carried on an
/// unchanged close. The signed imbalance `θ = Σ sign` accumulates until its absolute
/// value reaches a fixed `threshold`, at which point a bar closes. Imbalance bars
/// therefore sample the market when order flow becomes *one-sided* — a burst of
/// persistent buying or selling — rather than on time, count, or volume. This makes
/// them sensitive to informed, directional trading.
///
/// **Simplification.** The full method estimates a *dynamic* threshold
/// `E[T] · |2P − 1|` from an EWMA of the expected bar length `E[T]` and the buy-tick
/// probability `P`, and can weight each sign by volume (volume-imbalance bars) or
/// traded value (dollar-imbalance bars). This builder uses a **fixed** threshold on
/// the unweighted tick imbalance. For the adaptive estimator and the volume/dollar
/// variants, see López de Prado (2018), ch. 2.
///
/// At most one bar closes per candle, so [`BarBuilder::update`] returns either an
/// empty vector or a single [`ImbalanceBar`].
///
/// # Example
///
/// ```
/// use wickra_core::{BarBuilder, Candle, ImbalanceBars};
///
/// let flat = |price: f64| Candle::new(price, price, price, price, 1.0, 0).unwrap();
/// let mut bars = ImbalanceBars::new(3.0).unwrap();
/// bars.update(flat(10.0));            // seed, no sign
/// bars.update(flat(11.0));            // +1
/// bars.update(flat(12.0));            // +2
/// let out = bars.update(flat(13.0));  // +3 -> close
/// assert_eq!(out.len(), 1);
/// assert_eq!(out[0].direction, 1);
/// ```
#[derive(Debug, Clone)]
pub struct ImbalanceBars {
    threshold: f64,
    count: usize,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    prev_close: Option<f64>,
    last_sign: i8,
    theta: f64,
}

impl ImbalanceBars {
    /// Construct an imbalance-bar builder with the given absolute imbalance threshold.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `threshold` is not finite and positive.
    pub fn new(threshold: f64) -> Result<Self> {
        if !threshold.is_finite() || threshold <= 0.0 {
            return Err(Error::InvalidPeriod {
                message: "threshold must be finite and positive",
            });
        }
        Ok(Self {
            threshold,
            count: 0,
            open: 0.0,
            high: 0.0,
            low: 0.0,
            close: 0.0,
            prev_close: None,
            last_sign: 0,
            theta: 0.0,
        })
    }

    /// Configured absolute imbalance threshold.
    pub const fn threshold(&self) -> f64 {
        self.threshold
    }

    /// Signed imbalance accumulated into the in-progress bar.
    pub const fn imbalance(&self) -> f64 {
        self.theta
    }
}

impl BarBuilder for ImbalanceBars {
    type Bar = ImbalanceBar;

    #[inline]
    fn update(&mut self, candle: Candle) -> Vec<ImbalanceBar> {
        if self.count == 0 {
            self.open = candle.open;
            self.high = candle.high;
            self.low = candle.low;
        } else {
            self.high = self.high.max(candle.high);
            self.low = self.low.min(candle.low);
        }
        self.close = candle.close;
        self.count += 1;
        if let Some(prev) = self.prev_close {
            let sign = if candle.close > prev {
                1
            } else if candle.close < prev {
                -1
            } else {
                self.last_sign
            };
            self.last_sign = sign;
            self.theta += f64::from(sign);
        }
        self.prev_close = Some(candle.close);
        if self.theta.abs() < self.threshold {
            return Vec::new();
        }
        let direction = if self.theta > 0.0 { 1 } else { -1 };
        let bar = ImbalanceBar {
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            imbalance: self.theta,
            direction,
        };
        self.count = 0;
        self.theta = 0.0;
        vec![bar]
    }

    fn reset(&mut self) {
        self.count = 0;
        self.prev_close = None;
        self.last_sign = 0;
        self.theta = 0.0;
    }

    #[inline]
    fn name(&self) -> &'static str {
        "ImbalanceBars"
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
    fn rejects_invalid_threshold() {
        assert!(matches!(
            ImbalanceBars::new(0.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            ImbalanceBars::new(-3.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            ImbalanceBars::new(f64::NAN),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let bars = ImbalanceBars::new(10.0).unwrap();
        assert_relative_eq!(bars.threshold(), 10.0, epsilon = 1e-12);
        assert_relative_eq!(bars.imbalance(), 0.0, epsilon = 1e-12);
        assert_eq!(bars.name(), "ImbalanceBars");
    }

    #[test]
    fn buy_imbalance_closes_up_bar() {
        let mut bars = ImbalanceBars::new(3.0).unwrap();
        bars.update(flat(10.0)); // seed
        bars.update(flat(11.0)); // +1
        bars.update(flat(12.0)); // +2
        let out = bars.update(flat(13.0)); // +3
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].direction, 1);
        assert_relative_eq!(out[0].imbalance, 3.0, epsilon = 1e-12);
    }

    #[test]
    fn sell_imbalance_closes_down_bar() {
        let mut bars = ImbalanceBars::new(3.0).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(9.0)); // -1
        bars.update(flat(8.0)); // -2
        let out = bars.update(flat(7.0)); // -3
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].direction, -1);
    }

    #[test]
    fn flat_tick_carries_previous_sign() {
        let mut bars = ImbalanceBars::new(3.0).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(11.0)); // +1
        bars.update(flat(11.0)); // flat -> carries +1 -> +2
        assert_relative_eq!(bars.imbalance(), 2.0, epsilon = 1e-12);
    }

    #[test]
    fn oscillation_does_not_reach_threshold() {
        let mut bars = ImbalanceBars::new(3.0).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(11.0)); // +1
        bars.update(flat(10.0)); // -1 -> theta 0
        assert_eq!(bars.update(flat(11.0)), Vec::new()); // +1
        assert_relative_eq!(bars.imbalance(), 1.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut bars = ImbalanceBars::new(3.0).unwrap();
        bars.update(flat(10.0));
        bars.update(flat(11.0));
        bars.reset();
        assert_relative_eq!(bars.imbalance(), 0.0, epsilon = 1e-12);
        // After reset the next candle re-seeds (no previous close).
        assert_eq!(bars.update(flat(50.0)), Vec::new());
    }

    #[test]
    fn batch_concatenates_completed_bars() {
        let mut bars = ImbalanceBars::new(2.0).unwrap();
        let candles = [
            flat(10.0),
            flat(11.0), // +1
            flat(12.0), // +2 -> close
            flat(13.0), // +1
            flat(14.0), // +2 -> close
        ];
        let out = bars.batch(&candles);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|b| b.direction == 1));
    }
}
