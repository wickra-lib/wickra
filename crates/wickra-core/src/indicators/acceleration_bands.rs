//! Acceleration Bands (Price Headley).

use crate::error::{Error, Result};
use crate::indicators::sma::Sma;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Acceleration Bands output: SMA of close with momentum-biased envelopes
/// driven by the bar's high/low geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccelerationBandsOutput {
    /// Upper band: SMA of `high · (1 + factor · (high − low) / (high + low))`.
    pub upper: f64,
    /// Middle band: SMA of close.
    pub middle: f64,
    /// Lower band: SMA of `low · (1 − factor · (high − low) / (high + low))`.
    pub lower: f64,
}

/// Acceleration Bands (Price Headley): SMA-smoothed bands that widen with each
/// bar's relative range `(high − low) / (high + low)`.
///
/// ```text
/// ratio  = (high − low) / (high + low)
/// raw_up = high · (1 + factor · ratio)
/// raw_lo = low  · (1 − factor · ratio)
/// upper  = SMA(raw_up, period)
/// middle = SMA(close,  period)
/// lower  = SMA(raw_lo, period)
/// ```
///
/// Headley's reference parameters are `period = 20`, `factor = 4` — in this
/// form `raw_up = high · (1 + 4 · (high − low) / (high + low))`, the expression
/// TA-Lib's `ACCBANDS` uses. (Headley's scaled spelling
/// `2 · ((H − L) / ((H + L) / 2)) · 1000 · 0.001` is the same factor of 4.) The
/// bands compress in
/// quiet markets and flare on impulsive bars, making them a momentum-biased
/// alternative to the volatility-driven Bollinger or Keltner envelopes.
///
/// # Example
///
/// ```
/// use wickra_core::{AccelerationBands, Candle, Indicator};
///
/// let mut indicator = AccelerationBands::new(20, 4.0).unwrap();
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
pub struct AccelerationBands {
    upper_sma: Sma,
    middle_sma: Sma,
    lower_sma: Sma,
    factor: f64,
    period: usize,
}

impl AccelerationBands {
    /// Construct a new Acceleration Bands indicator.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `period == 0` and
    /// [`Error::NonPositiveMultiplier`] if `factor` is not strictly positive
    /// and finite.
    pub fn new(period: usize, factor: f64) -> Result<Self> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        Ok(Self {
            upper_sma: Sma::new(period)?,
            middle_sma: Sma::new(period)?,
            lower_sma: Sma::new(period)?,
            factor,
            period,
        })
    }

    /// Headley's classic configuration: `period = 20`, `factor = 4`.
    pub fn classic() -> Self {
        Self::new(20, 4.0).expect("classic Acceleration Bands parameters are valid")
    }

    /// Configured `(period, factor)`.
    pub const fn parameters(&self) -> (usize, f64) {
        (self.period, self.factor)
    }
}

impl Indicator for AccelerationBands {
    type Input = Candle;
    type Output = AccelerationBandsOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<AccelerationBandsOutput> {
        // (high + low) == 0 is geometrically impossible for valid OHLC
        // (high >= low and a zero-sum requires both equal to 0, which would
        // make the bar degenerate). Guard anyway so a hypothetical zero-price
        // bar collapses the ratio to zero rather than emitting NaN.
        let sum_hl = candle.high + candle.low;
        let ratio = if sum_hl == 0.0 {
            0.0
        } else {
            (candle.high - candle.low) / sum_hl
        };
        let raw_up = candle.high * self.factor.mul_add(ratio, 1.0);
        let raw_lo = candle.low * (-self.factor).mul_add(ratio, 1.0);

        // Feed all three SMAs unconditionally so they warm up in lock-step.
        let upper = self.upper_sma.update(raw_up);
        let middle = self.middle_sma.update(candle.close);
        let lower = self.lower_sma.update(raw_lo);
        let (upper, middle, lower) = (upper?, middle?, lower?);
        Some(AccelerationBandsOutput {
            upper,
            middle,
            lower,
        })
    }

    fn reset(&mut self) {
        self.upper_sma.reset();
        self.middle_sma.reset();
        self.lower_sma.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.middle_sma.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "AccelerationBands"
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
    fn rejects_zero_period() {
        assert!(matches!(
            AccelerationBands::new(0, 0.001),
            Err(Error::PeriodZero)
        ));
    }

    #[test]
    fn rejects_non_positive_factor() {
        assert!(matches!(
            AccelerationBands::new(20, 0.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            AccelerationBands::new(20, -1.0),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            AccelerationBands::new(20, f64::NAN),
            Err(Error::NonPositiveMultiplier)
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let ab = AccelerationBands::classic();
        let (p, f) = ab.parameters();
        assert_eq!(p, 20);
        assert_relative_eq!(f, 4.0, epsilon = 1e-12);
        assert_eq!(ab.warmup_period(), 20);
        assert_eq!(ab.name(), "AccelerationBands");
    }

    #[test]
    fn flat_market_collapses_to_constant() {
        // high == low so the ratio term is zero; all three SMAs converge to
        // the same constant.
        let candles: Vec<Candle> = (0..30).map(|_| c(10.0, 10.0, 10.0)).collect();
        let mut ab = AccelerationBands::new(5, 0.5).unwrap();
        let last = ab.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last.middle, 10.0, epsilon = 1e-9);
        assert_relative_eq!(last.upper, 10.0, epsilon = 1e-9);
        assert_relative_eq!(last.lower, 10.0, epsilon = 1e-9);
    }

    #[test]
    fn warmup_returns_none() {
        let mut ab = AccelerationBands::new(5, 0.001).unwrap();
        for i in 0..4 {
            let base = 100.0 + f64::from(i);
            assert!(ab.update(c(base + 1.0, base - 1.0, base)).is_none());
        }
        assert!(ab.update(c(105.0, 103.0, 104.0)).is_some());
    }

    #[test]
    fn upper_above_middle_above_lower() {
        let candles: Vec<Candle> = (0..50)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.2).sin() * 5.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut ab = AccelerationBands::new(20, 0.5).unwrap();
        for o in ab.batch(&candles).into_iter().flatten() {
            assert!(o.upper >= o.middle, "{} < {}", o.upper, o.middle);
            assert!(o.middle >= o.lower, "{} < {}", o.middle, o.lower);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40)
            .map(|i| c(f64::from(i) + 2.0, f64::from(i), f64::from(i) + 1.0))
            .collect();
        let mut a = AccelerationBands::new(10, 0.5).unwrap();
        let mut b = AccelerationBands::new(10, 0.5).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..10)
            .map(|i| c(f64::from(i) + 2.0, f64::from(i), f64::from(i) + 1.0))
            .collect();
        let mut ab = AccelerationBands::new(5, 0.5).unwrap();
        ab.batch(&candles);
        assert!(ab.is_ready());
        ab.reset();
        assert!(!ab.is_ready());
        assert_eq!(ab.update(candles[0]), None);
    }

    #[test]
    fn zero_price_candle_collapses_ratio_to_zero() {
        // `high + low == 0` is geometrically only reachable with a fully-zero
        // bar (high >= low and both non-negative for a real market, but
        // `Candle::new` accepts the degenerate `(0, 0, 0, 0)` case). The
        // ratio guard must fire and the bands all collapse to zero.
        let zero = Candle::new(0.0, 0.0, 0.0, 0.0, 1.0, 0).unwrap();
        let mut ab = AccelerationBands::new(1, 0.5).unwrap();
        let v = ab.update(zero).unwrap();
        assert_relative_eq!(v.upper, 0.0, epsilon = 1e-12);
        assert_relative_eq!(v.middle, 0.0, epsilon = 1e-12);
        assert_relative_eq!(v.lower, 0.0, epsilon = 1e-12);
    }

    /// Hand-computed reference. Single bar with `high = 12`, `low = 8`,
    /// `close = 10`, `factor = 0.5`, `period = 1`.
    /// `ratio  = (12 − 8) / (12 + 8) = 0.2`
    /// `raw_up = 12 · (1 + 0.5 · 0.2) = 12 · 1.1 = 13.2`
    /// `raw_lo = 8  · (1 − 0.5 · 0.2) = 8  · 0.9 = 7.2`
    /// `middle = SMA(close, 1) = 10`
    #[test]
    fn reference_value_single_bar() {
        let mut ab = AccelerationBands::new(1, 0.5).unwrap();
        let v = ab.update(c(12.0, 8.0, 10.0)).unwrap();
        assert_relative_eq!(v.upper, 13.2, epsilon = 1e-12);
        assert_relative_eq!(v.middle, 10.0, epsilon = 1e-12);
        assert_relative_eq!(v.lower, 7.2, epsilon = 1e-12);
    }

    #[test]
    fn rejects_infinite_factor_and_oversized_period() {
        assert!(matches!(
            AccelerationBands::new(20, f64::INFINITY),
            Err(Error::NonPositiveMultiplier)
        ));
        let too_long = crate::error::MAX_PERIOD + 1;
        assert!(matches!(
            AccelerationBands::new(too_long, 4.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    fn wavy_candles(len: i32) -> Vec<Candle> {
        (0..len)
            .map(|i| {
                let mid = 100.0 + (f64::from(i) * 0.4).sin() * 6.0;
                let half = 0.5 + (f64::from(i) * 0.9).cos().abs() * 2.0;
                c(mid + half, mid - half, mid + 0.3)
            })
            .collect()
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_index() {
        let candles = wavy_candles(30);
        let mut ab = AccelerationBands::new(7, 4.0).unwrap();
        let warmup = ab.warmup_period();
        let out = ab.batch(&candles);
        assert!(out.iter().take(warmup - 1).all(Option::is_none));
        assert!(out.iter().skip(warmup - 1).all(Option::is_some));
    }

    #[test]
    fn reset_replays_identically_to_fresh_instance() {
        let candles = wavy_candles(40);
        let mut used = AccelerationBands::classic();
        let first = used.batch(&candles);
        used.reset();
        let replay = used.batch(&candles);
        let fresh = AccelerationBands::classic().batch(&candles);
        assert_eq!(replay, fresh);
        assert_eq!(first, fresh);
    }

    #[test]
    fn batch_equals_streaming_bit_identical() {
        let candles = wavy_candles(60);
        let batch = AccelerationBands::new(9, 4.0).unwrap().batch(&candles);
        let mut streamer = AccelerationBands::new(9, 4.0).unwrap();
        let identical = candles.iter().zip(&batch).all(|(candle, b)| {
            let s = streamer.update(*candle);
            s.map(|o| (o.upper.to_bits(), o.middle.to_bits(), o.lower.to_bits()))
                == b.map(|o| (o.upper.to_bits(), o.middle.to_bits(), o.lower.to_bits()))
        });
        assert!(identical);
    }

    /// Hand-computed two-bar reference with Headley's `factor = 4`, `period = 2`.
    /// Bar 1: `H = 12, L = 8, C = 10` -> `ratio = 4 / 20 = 0.2`,
    ///   `raw_up = 12 · (1 + 0.8) = 21.6`, `raw_lo = 8 · (1 − 0.8) = 1.6`.
    /// Bar 2: `H = 22, L = 18, C = 20` -> `ratio = 4 / 40 = 0.1`,
    ///   `raw_up = 22 · 1.4 = 30.8`, `raw_lo = 18 · 0.6 = 10.8`.
    /// `upper = (21.6 + 30.8) / 2 = 26.2`, `middle = (10 + 20) / 2 = 15`,
    /// `lower = (1.6 + 10.8) / 2 = 6.2`.
    #[test]
    fn reference_value_two_bars_headley_factor() {
        let mut ab = AccelerationBands::new(2, 4.0).unwrap();
        assert_eq!(ab.update(c(12.0, 8.0, 10.0)), None);
        let v = ab.update(c(22.0, 18.0, 20.0)).unwrap();
        assert_relative_eq!(v.upper, 26.2, epsilon = 1e-12);
        assert_relative_eq!(v.middle, 15.0, epsilon = 1e-12);
        assert_relative_eq!(v.lower, 6.2, epsilon = 1e-12);
    }
}
