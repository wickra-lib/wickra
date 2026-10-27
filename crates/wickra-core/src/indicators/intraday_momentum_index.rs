//! Intraday Momentum Index (IMI).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Intraday Momentum Index — Tushar Chande's RSI built from the open-to-close
/// move instead of the close-to-close move.
///
/// For each bar the body is an up-move when `close > open` and a down-move
/// otherwise; the IMI sums those bodies over `period` bars and forms the
/// RSI-style ratio:
///
/// ```text
/// gain = max(close - open, 0),  loss = max(open - close, 0)
/// IMI  = 100 * Σ gain / (Σ gain + Σ loss)        over the last `period` bars
/// ```
///
/// Because it measures *intraday* (body) momentum rather than the gap-inclusive
/// close-to-close change, the IMI is a candle-pattern-flavoured overbought /
/// oversold gauge: persistent white bodies push it up, black bodies down. It is
/// bounded in `[0, 100]`; a window of doji-like bars (no net bodies) returns the
/// neutral `50`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, IntradayMomentumIndex, Indicator};
///
/// let mut imi = IntradayMomentumIndex::new(14).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let c = Candle::new(base, base + 1.0, base - 1.0, base + 0.5, 1.0, i64::from(i)).unwrap();
///     last = imi.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct IntradayMomentumIndex {
    period: usize,
    /// Per-bar `(gain, loss)` bodies, oldest at the front.
    window: VecDeque<(f64, f64)>,
    sum_gain: f64,
    sum_loss: f64,
}

impl IntradayMomentumIndex {
    /// Construct an IMI over `period` bars.
    ///
    /// # Errors
    ///
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
            window: VecDeque::with_capacity(period),
            sum_gain: 0.0,
            sum_loss: 0.0,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if the window is full.
    pub fn value(&self) -> Option<f64> {
        if self.window.len() != self.period {
            return None;
        }
        let denom = self.sum_gain + self.sum_loss;
        if denom == 0.0 {
            Some(50.0)
        } else {
            Some(100.0 * self.sum_gain / denom)
        }
    }
}

impl Indicator for IntradayMomentumIndex {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let body = candle.close - candle.open;
        let gain = if body > 0.0 { body } else { 0.0 };
        let loss = if body < 0.0 { -body } else { 0.0 };

        if self.window.len() == self.period {
            let (old_g, old_l) = self.window.pop_front().expect("window full");
            self.sum_gain -= old_g;
            self.sum_loss -= old_l;
        }
        self.window.push_back((gain, loss));
        self.sum_gain += gain;
        self.sum_loss += loss;
        self.value()
    }

    fn reset(&mut self) {
        self.window.clear();
        self.sum_gain = 0.0;
        self.sum_loss = 0.0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "IMI"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn candle(open: f64, close: f64) -> Candle {
        let hi = open.max(close) + 1.0;
        let lo = open.min(close) - 1.0;
        Candle::new(open, hi, lo, close, 1.0, 0).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            IntradayMomentumIndex::new(0),
            Err(Error::PeriodZero)
        ));
    }

    /// Cover the const accessor `period` and the Indicator-impl `warmup_period`
    /// + `name`.
    #[test]
    fn accessors_and_metadata() {
        let imi = IntradayMomentumIndex::new(14).unwrap();
        assert_eq!(imi.period(), 14);
        assert_eq!(imi.warmup_period(), 14);
        assert_eq!(imi.name(), "IMI");
    }

    #[test]
    fn all_up_bodies_is_one_hundred() {
        let mut imi = IntradayMomentumIndex::new(3).unwrap();
        let bars = [candle(10.0, 11.0), candle(11.0, 13.0), candle(13.0, 14.0)];
        let out = imi.batch(&bars);
        assert!(out[0].is_none());
        assert!(out[1].is_none());
        assert_relative_eq!(out[2].unwrap(), 100.0, epsilon = 1e-12);
    }

    #[test]
    fn all_down_bodies_is_zero() {
        let mut imi = IntradayMomentumIndex::new(3).unwrap();
        let bars = [candle(14.0, 13.0), candle(13.0, 11.0), candle(11.0, 10.0)];
        assert_relative_eq!(imi.batch(&bars)[2].unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn known_value_mixed_bodies() {
        // bodies: +1, -1, +2 -> sum_gain = 3, sum_loss = 1 -> 100*3/4 = 75.
        let mut imi = IntradayMomentumIndex::new(3).unwrap();
        let bars = [candle(10.0, 11.0), candle(11.0, 10.0), candle(10.0, 12.0)];
        assert_relative_eq!(imi.batch(&bars)[2].unwrap(), 75.0, epsilon = 1e-12);
    }

    #[test]
    fn doji_window_is_neutral() {
        // close == open every bar -> no bodies -> neutral 50.
        let mut imi = IntradayMomentumIndex::new(3).unwrap();
        let bars = [candle(10.0, 10.0), candle(11.0, 11.0), candle(12.0, 12.0)];
        assert_relative_eq!(imi.batch(&bars)[2].unwrap(), 50.0, epsilon = 1e-12);
    }

    #[test]
    fn slides_window() {
        // After [+1,-1,+2] (75) add +0 body window -> [-1,+2,0]: gain 2, loss 1 -> 66.67.
        let mut imi = IntradayMomentumIndex::new(3).unwrap();
        let bars = [
            candle(10.0, 11.0),
            candle(11.0, 10.0),
            candle(10.0, 12.0),
            candle(12.0, 12.0),
        ];
        let out = imi.batch(&bars);
        assert_relative_eq!(out[3].unwrap(), 100.0 * 2.0 / 3.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut imi = IntradayMomentumIndex::new(3).unwrap();
        imi.batch(&[candle(10.0, 11.0), candle(11.0, 12.0), candle(12.0, 13.0)]);
        assert!(imi.is_ready());
        imi.reset();
        assert!(!imi.is_ready());
        assert_eq!(imi.update(candle(1.0, 2.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let bars: Vec<Candle> = (0..30)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                candle(base, base + (f64::from(i) * 0.5).sin())
            })
            .collect();
        let mut a = IntradayMomentumIndex::new(7).unwrap();
        let mut b = IntradayMomentumIndex::new(7).unwrap();
        assert_eq!(
            a.batch(&bars),
            bars.iter().map(|c| b.update(*c)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            IntradayMomentumIndex::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
