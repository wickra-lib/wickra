//! Kase Permission Stochastic — a double-smoothed stochastic used as a
//! trade-permission filter.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Kase Permission Stochastic output: a fast and a slow line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KasePermissionStochasticOutput {
    /// Fast line: EMA of the raw `%K` over the smoothing period.
    pub fast: f64,
    /// Slow line: EMA of the fast line over the smoothing period.
    pub slow: f64,
}

/// Cynthia Kase's Permission Stochastic: a stochastic oscillator smoothed twice,
/// whose fast/slow relationship grants or denies "permission" to trade in the
/// direction of a higher-timeframe signal.
///
/// ```text
/// raw%K = 100 * (close - LL) / (HH - LL)     over `length` (50 when HH == LL)
/// fast  = EMA(raw%K, smooth)
/// slow  = EMA(fast,  smooth)
/// ```
///
/// The raw stochastic is the usual `%K`, then an EMA produces the *fast* line
/// and a second EMA of that produces the *slow* line. Kase uses the pair as a
/// gate: a fast line above the slow line (and rising) gives permission for
/// longs, the reverse for shorts. When the lookback window is perfectly flat
/// (`HH == LL`), the raw stochastic is undefined and defaults to the neutral
/// `50`.
///
/// Reference: Cynthia Kase, *Trading with the Odds*, 1996.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, KasePermissionStochastic};
///
/// let mut indicator = KasePermissionStochastic::new(9, 3).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 1.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct KasePermissionStochastic {
    length: usize,
    smooth: usize,
    window: VecDeque<(f64, f64)>,
    fast_ema: Ema,
    slow_ema: Ema,
}

impl KasePermissionStochastic {
    /// Construct with the stochastic `length` and the EMA `smooth` period
    /// applied twice.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `length == 0` or `smooth == 0`.
    pub fn new(length: usize, smooth: usize) -> Result<Self> {
        if length == 0 {
            return Err(Error::PeriodZero);
        }
        if length > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            length,
            smooth,
            window: VecDeque::with_capacity(length),
            fast_ema: Ema::new(smooth)?,
            slow_ema: Ema::new(smooth)?,
        })
    }

    /// Cynthia Kase's classic parameters: `length = 9`, `smooth = 3`.
    pub fn classic() -> Self {
        Self::new(9, 3).expect("classic Kase Permission Stochastic parameters are valid")
    }

    /// Configured `(length, smooth)`.
    pub const fn periods(&self) -> (usize, usize) {
        (self.length, self.smooth)
    }
}

impl Indicator for KasePermissionStochastic {
    type Input = Candle;
    type Output = KasePermissionStochasticOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<KasePermissionStochasticOutput> {
        self.window.push_back((candle.high, candle.low));
        if self.window.len() > self.length {
            self.window.pop_front();
        }
        if self.window.len() < self.length {
            return None;
        }

        let highest = self.window.iter().map(|w| w.0).fold(f64::MIN, f64::max);
        let lowest = self.window.iter().map(|w| w.1).fold(f64::MAX, f64::min);
        let raw_k = if highest > lowest {
            100.0 * (candle.close - lowest) / (highest - lowest)
        } else {
            50.0
        };

        let fast = self.fast_ema.update(raw_k)?;
        let slow = self.slow_ema.update(fast)?;
        Some(KasePermissionStochasticOutput { fast, slow })
    }

    fn reset(&mut self) {
        self.window.clear();
        self.fast_ema.reset();
        self.slow_ema.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // raw%K ready after `length` bars; each EMA seeds over `smooth` values.
        self.length + 2 * self.smooth - 2
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.slow_ema.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "KasePermissionStochastic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn candle(high: f64, low: f64, close: f64, ts: i64) -> Candle {
        Candle::new(f64::midpoint(high, low), high, low, close, 1.0, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(
            KasePermissionStochastic::new(0, 3),
            Err(Error::PeriodZero)
        ));
        assert!(matches!(
            KasePermissionStochastic::new(9, 0),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let k = KasePermissionStochastic::classic();
        assert_eq!(k.periods(), (9, 3));
        // 9 + 2*3 - 2 = 13.
        assert_eq!(k.warmup_period(), 13);
        assert_eq!(k.name(), "KasePermissionStochastic");
        assert!(!k.is_ready());
    }

    #[test]
    fn warmup_emits_at_expected_bar() {
        let mut k = KasePermissionStochastic::new(3, 2).unwrap();
        // warmup = 3 + 2*2 - 2 = 5 -> first value at input 5 (index 4).
        let candles: Vec<Candle> = (0..8).map(|i| candle(11.0, 9.0, 10.5, i)).collect();
        let out = k.batch(&candles);
        assert!(out[3].is_none());
        assert!(out[4].is_some());
    }

    #[test]
    fn top_of_range_is_high() {
        // Close pinned at the top of a rising range -> raw%K near 100, both
        // smoothed lines high.
        let mut k = KasePermissionStochastic::new(5, 3).unwrap();
        let candles: Vec<Candle> = (0_i64..40)
            .map(|i| {
                let base = 100.0 + i as f64;
                candle(base + 2.0, base - 2.0, base + 2.0, i)
            })
            .collect();
        let last = k.batch(&candles).last().unwrap().unwrap();
        assert!(last.fast > 80.0, "fast {} should be high", last.fast);
        assert!(last.slow > 80.0, "slow {} should be high", last.slow);
    }

    #[test]
    fn flat_window_defaults_to_neutral() {
        // Constant high/low/close -> HH == LL -> raw%K defaults to 50, so both
        // EMAs converge to 50.
        let mut k = KasePermissionStochastic::new(4, 2).unwrap();
        let candles: Vec<Candle> = (0..20).map(|i| candle(10.0, 10.0, 10.0, i)).collect();
        let last = k.batch(&candles).last().unwrap().unwrap();
        assert_relative_eq!(last.fast, 50.0, epsilon = 1e-9);
        assert_relative_eq!(last.slow, 50.0, epsilon = 1e-9);
    }

    #[test]
    fn reset_clears_state() {
        let mut k = KasePermissionStochastic::classic();
        let candles: Vec<Candle> = (0..40).map(|i| candle(11.0, 9.0, 10.5, i)).collect();
        k.batch(&candles);
        assert!(k.is_ready());
        k.reset();
        assert!(!k.is_ready());
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80_i64)
            .map(|i| {
                let base = 100.0 + (i as f64 * 0.2).sin() * 5.0;
                candle(base + 2.0, base - 2.0, base + (i as f64 * 0.3).cos(), i)
            })
            .collect();
        let mut a = KasePermissionStochastic::classic();
        let mut b = KasePermissionStochastic::classic();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|c| b.update(*c)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_length_above_maximum() {
        assert!(matches!(
            KasePermissionStochastic::new(crate::error::MAX_PERIOD + 1, 2),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
