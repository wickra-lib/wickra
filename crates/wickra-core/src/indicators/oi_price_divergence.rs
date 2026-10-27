//! Open-Interest / Price Divergence — relative OI change minus relative price
//! change over a window.

use std::collections::VecDeque;

use crate::derivatives::DerivativesTick;
use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Open-Interest / Price Divergence — the gap between how fast open interest and
/// the mark price have moved over the trailing window of `window` ticks.
///
/// ```text
/// oiChange    = (openInterestₜ − openInterestₜ₋ₙ) / openInterestₜ₋ₙ
/// priceChange = (markPriceₜ    − markPriceₜ₋ₙ)    / markPriceₜ₋ₙ
/// divergence  = oiChange − priceChange                          (n = window)
/// ```
///
/// Reading the two together is a classic positioning signal: open interest
/// rising while price falls (a positive divergence) marks fresh shorts piling
/// in; open interest falling while price rises marks a short squeeze / unwind.
/// A value near zero means OI and price moved in step. If the reference open
/// interest is zero, the OI term contributes zero (no base to grow from).
///
/// The indicator warms up for `window + 1` ticks — `update` returns `None` until
/// the window spans a full `window`-tick lookback — then emits the divergence,
/// maintained in O(1) per tick via a ring buffer.
///
/// `Input = DerivativesTick`, `Output = f64`.
///
/// # Example
///
/// ```
/// use wickra_core::{DerivativesTick, Indicator, OIPriceDivergence};
///
/// fn tick(oi: f64, mark: f64) -> DerivativesTick {
///     DerivativesTick::new(0.0, mark, mark, mark, oi, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0)
///         .unwrap()
/// }
///
/// let mut div = OIPriceDivergence::new(1).unwrap();
/// assert_eq!(div.update(tick(1_000.0, 100.0)), None);
/// // OI +10% while price flat -> divergence +0.1.
/// assert!((div.update(tick(1_100.0, 100.0)).unwrap() - 0.1).abs() < 1e-12);
/// ```
#[derive(Debug, Clone)]
pub struct OIPriceDivergence {
    window: usize,
    history: VecDeque<(f64, f64)>,
}

impl OIPriceDivergence {
    /// Construct an OI / price divergence over a window of `window` ticks.
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
            history: VecDeque::with_capacity(window + 1),
        })
    }

    /// The configured window length, in ticks.
    #[must_use]
    pub fn window(&self) -> usize {
        self.window
    }
}

impl Indicator for OIPriceDivergence {
    type Input = DerivativesTick;
    type Output = f64;

    #[inline]
    fn update(&mut self, tick: DerivativesTick) -> Option<f64> {
        self.history
            .push_back((tick.open_interest, tick.mark_price));
        if self.history.len() > self.window + 1 {
            self.history.pop_front();
        }
        if self.history.len() < self.window + 1 {
            return None;
        }
        let (old_oi, old_mark) = *self.history.front().expect("len == window + 1");
        let (cur_oi, cur_mark) = *self.history.back().expect("len == window + 1");
        // Open interest can legitimately be zero; with no base there is no
        // relative change to report from it.
        let oi_change = if old_oi == 0.0 {
            0.0
        } else {
            (cur_oi - old_oi) / old_oi
        };
        // The mark price is finite and positive by `DerivativesTick`
        // construction, so the denominator is always well-defined.
        let price_change = (cur_mark - old_mark) / old_mark;
        Some(oi_change - price_change)
    }

    fn reset(&mut self) {
        self.history.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.window + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.history.len() == self.window + 1
    }

    #[inline]
    fn name(&self) -> &'static str {
        "OIPriceDivergence"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn tick(oi: f64, mark: f64) -> DerivativesTick {
        DerivativesTick::new_unchecked(0.0, mark, mark, mark, oi, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0)
    }

    #[test]
    fn rejects_zero_window() {
        assert!(matches!(OIPriceDivergence::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let div = OIPriceDivergence::new(5).unwrap();
        assert_eq!(div.name(), "OIPriceDivergence");
        assert_eq!(div.warmup_period(), 6);
        assert_eq!(div.window(), 5);
        assert!(!div.is_ready());
    }

    #[test]
    fn oi_up_price_flat_is_positive() {
        let mut div = OIPriceDivergence::new(1).unwrap();
        assert_eq!(div.update(tick(1_000.0, 100.0)), None);
        let out = div.update(tick(1_100.0, 100.0)).unwrap();
        assert!((out - 0.1).abs() < 1e-12);
        assert!(div.is_ready());
    }

    #[test]
    fn oi_flat_price_up_is_negative() {
        let mut div = OIPriceDivergence::new(1).unwrap();
        div.update(tick(1_000.0, 100.0));
        // OI flat, price +10% -> divergence -0.1.
        let out = div.update(tick(1_000.0, 110.0)).unwrap();
        assert!((out + 0.1).abs() < 1e-12);
    }

    #[test]
    fn zero_reference_oi_drops_oi_term() {
        let mut div = OIPriceDivergence::new(1).unwrap();
        div.update(tick(0.0, 100.0));
        // Reference OI is zero -> only the price term contributes: -(110-100)/100.
        let out = div.update(tick(500.0, 110.0)).unwrap();
        assert!((out + 0.1).abs() < 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let ticks: Vec<DerivativesTick> = (0..30)
            .map(|i| tick(1_000.0 + f64::from(i % 7) * 10.0, 100.0 + f64::from(i % 5)))
            .collect();
        let mut a = OIPriceDivergence::new(4).unwrap();
        let mut b = OIPriceDivergence::new(4).unwrap();
        assert_eq!(
            a.batch(&ticks),
            ticks.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut div = OIPriceDivergence::new(1).unwrap();
        div.update(tick(1_000.0, 100.0));
        div.update(tick(1_100.0, 100.0));
        assert!(div.is_ready());
        div.reset();
        assert!(!div.is_ready());
        assert_eq!(div.update(tick(1_000.0, 100.0)), None);
    }

    #[test]
    fn rejects_window_above_maximum() {
        assert!(matches!(
            OIPriceDivergence::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
