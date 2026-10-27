//! TTM Squeeze (John Carter).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::atr::Atr;
use crate::indicators::bollinger::BollingerBands;
use crate::indicators::sma::Sma;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// TTM Squeeze output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TtmSqueezeOutput {
    /// `1.0` while the squeeze is *on* (Bollinger Bands sit inside the Keltner
    /// Channel), `0.0` otherwise. The squeeze releases — the signal flips back
    /// to `0.0` — when volatility expands and BB pierce KC.
    pub squeeze: f64,
    /// Detrended momentum: linear-regression endpoint of
    /// `close − (midpoint(highest_high, lowest_low, period) + SMA(close, period)) / 2`.
    /// Histogram-like reading that swings positive in a breakout up, negative
    /// in a breakout down; trade direction on the squeeze release follows the
    /// sign of `momentum`.
    pub momentum: f64,
}

/// TTM Squeeze (John Carter): a Bollinger-vs-Keltner volatility squeeze paired
/// with a detrended-close momentum reading.
///
/// Carter's setup detects coiled markets (low realised volatility relative to
/// ATR) and the *direction* of the breakout when they uncoil:
///
/// ```text
/// squeeze  = 1.0 if BollingerBands(period, bb_mult)
///                ⊂ KeltnerChannels-like(SMA(period), ATR(period), kc_mult)
///            else 0.0
///
/// hl_mid   = (max(high, period) + min(low, period)) / 2
/// detrend  = close − (hl_mid + SMA(close, period)) / 2
/// momentum = LinearRegression(detrend, period)        // endpoint
/// ```
///
/// The "Keltner-like" envelope here uses an *SMA* centerline (not the EMA of
/// typical price that [`Keltner`](crate::Keltner) uses) plus an ATR offset,
/// exactly as Carter's original publication and every chart-vendor
/// implementation define it. Common parameters: `period = 20`, `bb_mult = 2.0`,
/// `kc_mult = 1.5`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, TtmSqueeze};
///
/// let mut indicator = TtmSqueeze::new(20, 2.0, 1.5).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct TtmSqueeze {
    period: usize,
    kc_mult: f64,
    bb: BollingerBands,
    sma_close: Sma,
    atr: Atr,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
    closes: VecDeque<f64>,
    // Pre-computed OLS constants over `x = 0..period − 1`.
    sum_x: f64,
    denom: f64,
}

impl TtmSqueeze {
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0` and
    /// [`Error::NonPositiveMultiplier`] if either multiplier is not strictly
    /// positive and finite. `period >= 2` is required for the linear-regression
    /// momentum component.
    pub fn new(period: usize, bb_mult: f64, kc_mult: f64) -> Result<Self> {
        if period < 2 {
            return Err(Error::InvalidPeriod {
                message: "TTM squeeze needs period >= 2 for the momentum regression",
            });
        }
        if period > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !bb_mult.is_finite() || bb_mult <= 0.0 || !kc_mult.is_finite() || kc_mult <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        let n = period as f64;
        let sum_x = n * (n - 1.0) / 2.0;
        let sum_xx = (n - 1.0) * n * (2.0 * n - 1.0) / 6.0;
        Ok(Self {
            period,
            kc_mult,
            bb: BollingerBands::new(period, bb_mult)?,
            sma_close: Sma::new(period)?,
            atr: Atr::new(period)?,
            highs: VecDeque::with_capacity(period),
            lows: VecDeque::with_capacity(period),
            closes: VecDeque::with_capacity(period),
            sum_x,
            denom: n * sum_xx - sum_x * sum_x,
        })
    }

    /// John Carter's classic configuration: `period = 20`, `bb_mult = 2.0`,
    /// `kc_mult = 1.5`.
    pub fn classic() -> Self {
        Self::new(20, 2.0, 1.5).expect("classic TTM Squeeze parameters are valid")
    }

    /// Configured `(period, bb_mult, kc_mult)`.
    pub fn parameters(&self) -> (usize, f64, f64) {
        (self.period, self.bb.multiplier(), self.kc_mult)
    }
}

impl Indicator for TtmSqueeze {
    type Input = Candle;
    type Output = TtmSqueezeOutput;

    fn update(&mut self, candle: Candle) -> Option<TtmSqueezeOutput> {
        if self.highs.len() == self.period {
            self.highs.pop_front();
            self.lows.pop_front();
            self.closes.pop_front();
        }
        self.highs.push_back(candle.high);
        self.lows.push_back(candle.low);
        self.closes.push_back(candle.close);

        // Feed all three sub-indicators unconditionally so they warm up in
        // lock-step. ATR returns its first value at bar `period` (Wilder
        // seeds), the SMA and BB on bar `period` as well.
        let bb = self.bb.update(candle.close);
        let mid = self.sma_close.update(candle.close);
        let atr = self.atr.update(candle);
        let (bb, mid, atr) = (bb?, mid?, atr?);

        let kc_upper = mid + self.kc_mult * atr;
        let kc_lower = mid - self.kc_mult * atr;
        let squeeze = f64::from(bb.upper <= kc_upper && bb.lower >= kc_lower);

        // Detrended close. The reference forms it as the deviation of close
        // from the average of the rolling high-low midpoint and the SMA of
        // close, then runs a linear regression of that series.
        let hi = self.highs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let lo = self.lows.iter().copied().fold(f64::INFINITY, f64::min);
        let hl_mid = f64::midpoint(hi, lo);
        // Build the detrended window over the closes currently in `closes`.
        // We need all `period` closes to fit the regression, which is
        // guaranteed once `bb` / `mid` are ready.
        let baseline = f64::midpoint(hl_mid, mid);
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        for (i, &c) in self.closes.iter().enumerate() {
            let y = c - baseline;
            let x = i as f64;
            sum_y += y;
            sum_xy += x * y;
        }
        let n = self.period as f64;
        let slope = (n * sum_xy - self.sum_x * sum_y) / self.denom;
        let intercept = (sum_y - slope * self.sum_x) / n;
        let momentum = intercept + slope * (n - 1.0);

        Some(TtmSqueezeOutput { squeeze, momentum })
    }

    fn reset(&mut self) {
        self.bb.reset();
        self.sma_close.reset();
        self.atr.reset();
        self.highs.clear();
        self.lows.clear();
        self.closes.clear();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.bb.is_ready() && self.sma_close.is_ready() && self.atr.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TtmSqueeze"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(h: f64, l: f64, cl: f64) -> Candle {
        Candle::new(cl, h, l, cl, 1.0, 0).unwrap()
    }

    #[test]
    fn rejects_invalid_period() {
        assert!(TtmSqueeze::new(0, 2.0, 1.5).is_err());
        assert!(TtmSqueeze::new(1, 2.0, 1.5).is_err());
    }

    #[test]
    fn rejects_non_positive_multipliers() {
        assert!(matches!(
            TtmSqueeze::new(20, 0.0, 1.5),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            TtmSqueeze::new(20, 2.0, -1.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            TtmSqueeze::new(20, f64::NAN, 1.5),
            Err(Error::NonPositiveMultiplier)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let s = TtmSqueeze::classic();
        let (p, b, k) = s.parameters();
        assert_eq!(p, 20);
        assert_relative_eq!(b, 2.0, epsilon = 1e-12);
        assert_relative_eq!(k, 1.5, epsilon = 1e-12);
        assert_eq!(s.warmup_period(), 20);
        assert_eq!(s.name(), "TtmSqueeze");
    }

    #[test]
    fn flat_market_has_zero_momentum() {
        let candles: Vec<Candle> = (0..30).map(|_| c(10.0, 10.0, 10.0)).collect();
        let mut s = TtmSqueeze::new(20, 2.0, 1.5).unwrap();
        let last = s.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last.momentum, 0.0, epsilon = 1e-9);
        // With zero volatility both BB and KC collapse to a point, so the
        // squeeze is trivially "on".
        assert_relative_eq!(last.squeeze, 1.0, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| c(f64::from(i) + 2.0, f64::from(i), f64::from(i) + 1.0))
            .collect();
        let mut a = TtmSqueeze::new(20, 2.0, 1.5).unwrap();
        let mut b = TtmSqueeze::new(20, 2.0, 1.5).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..30)
            .map(|i| c(f64::from(i) + 1.0, f64::from(i) - 1.0, f64::from(i)))
            .collect();
        let mut s = TtmSqueeze::classic();
        s.batch(&candles);
        assert!(s.is_ready());
        s.reset();
        assert!(!s.is_ready());
        assert_eq!(s.update(candles[0]), None);
    }

    /// Squeeze fires only after `period` candles, never before.
    #[test]
    fn warmup_returns_none() {
        let mut s = TtmSqueeze::new(20, 2.0, 1.5).unwrap();
        for i in 0..19 {
            let base = 100.0 + f64::from(i);
            assert!(s.update(c(base + 1.0, base - 1.0, base)).is_none());
        }
        assert!(s.update(c(121.0, 119.0, 120.0)).is_some());
    }

    /// Squeeze flag is binary — `0.0` or `1.0`.
    #[test]
    fn squeeze_is_binary() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.4).sin() * 2.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut s = TtmSqueeze::new(20, 2.0, 1.5).unwrap();
        for o in s.batch(&candles).into_iter().flatten() {
            assert!(o.squeeze == 0.0 || o.squeeze == 1.0);
        }
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            TtmSqueeze::new(crate::error::MAX_PERIOD + 1, 2.0, 1.5),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
