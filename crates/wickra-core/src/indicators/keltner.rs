//! Keltner Channels.

use crate::error::{Error, Result};
use crate::indicators::atr::Atr;
use crate::indicators::ema::Ema;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Keltner Channels output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeltnerOutput {
    /// Upper band = middle + multiplier * ATR.
    pub upper: f64,
    /// Middle band = EMA of the close.
    pub middle: f64,
    /// Lower band = middle - multiplier * ATR.
    pub lower: f64,
}

/// Keltner Channels: an EMA centerline with bands sized by ATR.
///
/// This is the modern (Linda Raschke) form used by `TradingView`, `StockCharts` and
/// most libraries: `middle = EMA(close, ema_period)`,
/// `upper / lower = middle ± multiplier · ATR(atr_period)`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Keltner};
///
/// let mut indicator = Keltner::new(5, 5, 2.0).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Keltner {
    ema: Ema,
    atr: Atr,
    multiplier: f64,
    ema_period: usize,
    atr_period: usize,
}

impl Keltner {
    /// # Errors
    /// Returns [`Error::PeriodZero`] / [`Error::NonPositiveMultiplier`] on invalid inputs.
    pub fn new(ema_period: usize, atr_period: usize, multiplier: f64) -> Result<Self> {
        if !multiplier.is_finite() || multiplier <= 0.0 {
            return Err(Error::NonPositiveMultiplier);
        }
        Ok(Self {
            ema: Ema::new(ema_period)?,
            atr: Atr::new(atr_period)?,
            multiplier,
            ema_period,
            atr_period,
        })
    }

    /// Classic configuration: EMA(20), ATR(10), 2.0x multiplier.
    pub fn classic() -> Self {
        Self::new(20, 10, 2.0).expect("classic Keltner parameters are valid")
    }

    /// Configured `(ema_period, atr_period, multiplier)`.
    pub const fn periods(&self) -> (usize, usize, f64) {
        (self.ema_period, self.atr_period, self.multiplier)
    }
}

impl Indicator for Keltner {
    type Input = Candle;
    type Output = KeltnerOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<KeltnerOutput> {
        // Feed both sub-indicators on every candle so they warm up in parallel.
        // Gating `atr.update` behind `ema.update(...)?` would starve the ATR of
        // every candle consumed during the EMA's warmup, delaying the first
        // emission past `warmup_period()` and seeding the ATR over the wrong
        // window.
        let mid = self.ema.update(candle.close);
        let atr = self.atr.update(candle);
        let (mid, atr) = (mid?, atr?);
        Some(KeltnerOutput {
            upper: mid + self.multiplier * atr,
            middle: mid,
            lower: mid - self.multiplier * atr,
        })
    }

    fn reset(&mut self) {
        self.ema.reset();
        self.atr.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.ema_period.max(self.atr_period)
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.ema.is_ready() && self.atr.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "KeltnerChannels"
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
    fn flat_market_collapses_bands() {
        let candles: Vec<Candle> = (0..50).map(|_| c(10.0, 10.0, 10.0)).collect();
        let mut k = Keltner::new(20, 10, 2.0).unwrap();
        let last = k.batch(&candles).into_iter().flatten().last().unwrap();
        assert_relative_eq!(last.upper, last.middle, epsilon = 1e-9);
        assert_relative_eq!(last.lower, last.middle, epsilon = 1e-9);
    }

    #[test]
    fn upper_above_middle_above_lower() {
        let candles: Vec<Candle> = (0..100)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.2).sin() * 5.0;
                c(m + 1.0, m - 1.0, m)
            })
            .collect();
        let mut k = Keltner::classic();
        for o in k.batch(&candles).into_iter().flatten() {
            assert!(o.upper >= o.middle);
            assert!(o.middle >= o.lower);
        }
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..50)
            .map(|i| c(f64::from(i) + 1.0, f64::from(i) - 1.0, f64::from(i)))
            .collect();
        let mut a = Keltner::classic();
        let mut b = Keltner::classic();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_invalid_input() {
        assert!(Keltner::new(0, 10, 2.0).is_err());
        assert!(Keltner::new(20, 10, 0.0).is_err());
        assert!(Keltner::new(20, 10, -1.0).is_err());
    }

    /// Cover the const accessor `periods` (68-70) and the Indicator-impl
    /// `name` body (106-108). Existing tests inspect band output but
    /// never query the metadata.
    #[test]
    fn accessors_and_metadata() {
        let k = Keltner::new(20, 10, 2.0).unwrap();
        let (ema, atr, mult) = k.periods();
        assert_eq!(ema, 20);
        assert_eq!(atr, 10);
        assert!((mult - 2.0).abs() < 1e-12);
        assert_eq!(k.name(), "KeltnerChannels");
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..50)
            .map(|i| c(f64::from(i) + 1.0, f64::from(i) - 1.0, f64::from(i)))
            .collect();
        let mut k = Keltner::classic();
        k.batch(&candles);
        assert!(k.is_ready());
        k.reset();
        assert!(!k.is_ready());
        assert_eq!(k.update(candles[0]), None);
    }

    #[test]
    fn first_emission_matches_warmup_period() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let base = 100.0 + f64::from(i);
                c(base + 1.0, base - 1.0, base)
            })
            .collect();
        let mut k = Keltner::classic();
        let out = k.batch(&candles);
        let warmup = k.warmup_period();
        assert_eq!(warmup, 20);
        for (i, v) in out.iter().enumerate().take(warmup - 1) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        assert!(
            out[warmup - 1].is_some(),
            "first KeltnerOutput must land at warmup_period - 1"
        );
    }

    #[test]
    fn matches_independent_ema_and_atr() {
        // The EMA (on the close) and the ATR (on the candle) run as
        // independent siblings; Keltner must equal feeding two standalone
        // instances and combining them once both are ready.
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.2).sin() * 5.0;
                c(m + 1.5, m - 1.5, m)
            })
            .collect();
        let mut k = Keltner::classic();
        let mut ema = Ema::new(20).unwrap();
        let mut atr = Atr::new(10).unwrap();
        for candle in &candles {
            let got = k.update(*candle);
            let mid = ema.update(candle.close);
            let a = atr.update(*candle);
            assert_eq!(got.is_some(), mid.is_some() && a.is_some());
            if let (Some(o), Some(m), Some(av)) = (got, mid, a) {
                assert_relative_eq!(o.middle, m, epsilon = 1e-9);
                assert_relative_eq!(o.upper, m + 2.0 * av, epsilon = 1e-9);
                assert_relative_eq!(o.lower, m - 2.0 * av, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn rejects_every_invalid_parameter() {
        assert!(matches!(Keltner::new(0, 10, 2.0), Err(Error::PeriodZero)));
        assert!(matches!(Keltner::new(20, 0, 2.0), Err(Error::PeriodZero)));
        assert!(matches!(
            Keltner::new(20, 10, f64::NAN),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            Keltner::new(20, 10, f64::INFINITY),
            Err(Error::NonPositiveMultiplier)
        ));
        assert!(matches!(
            Keltner::new(20, 10, 0.0),
            Err(Error::NonPositiveMultiplier)
        ));
        let too_big = crate::error::MAX_PERIOD + 1;
        assert!(matches!(
            Keltner::new(too_big, 10, 2.0),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Keltner::new(20, too_big, 2.0),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn warmup_follows_the_longer_atr_period() {
        // ATR(12) is slower than EMA(5): the first value lands at index 11.
        let candles: Vec<Candle> = (0..30)
            .map(|i| c(f64::from(i) + 1.0, f64::from(i) - 1.0, f64::from(i)))
            .collect();
        let mut k = Keltner::new(5, 12, 2.0).unwrap();
        assert_eq!(k.warmup_period(), 12);
        let out = k.batch(&candles);
        assert!(out[..11].iter().all(Option::is_none));
        assert!(out[11..].iter().all(Option::is_some));
    }

    #[test]
    fn hand_computed_reference() {
        // EMA(2) (alpha = 2/3, seeded with the mean), ATR(2) (Wilder, seeded
        // with the mean true range; the first bar's TR is H − L), mult 1.5.
        //   b0 H 11 L 9  C 10    TR 2
        //   b1 H 12 L 10 C 11    TR 2   EMA 10.5   ATR 2
        //   b2 H 14 L 11 C 13    TR 3   EMA 2/3·13 + 1/3·10.5 = 73/6   ATR (2 + 3)/2 = 2.5
        //   b3 H 10 L 9  C 9.5   TR max(1, |10 − 13|, |9 − 13|) = 4
        //                              EMA 2/3·9.5 + 1/3·73/6 = 187/18  ATR (2.5 + 4)/2 = 3.25
        let candles = [
            c(11.0, 9.0, 10.0),
            c(12.0, 10.0, 11.0),
            c(14.0, 11.0, 13.0),
            c(10.0, 9.0, 9.5),
        ];
        let out = Keltner::new(2, 2, 1.5).unwrap().batch(&candles);
        assert_eq!(out[0], None);
        let b1 = out[1].unwrap();
        assert_relative_eq!(b1.middle, 10.5, epsilon = 1e-12);
        assert_relative_eq!(b1.upper, 13.5, epsilon = 1e-12);
        assert_relative_eq!(b1.lower, 7.5, epsilon = 1e-12);
        let b2 = out[2].unwrap();
        assert_relative_eq!(b2.middle, 73.0 / 6.0, epsilon = 1e-12);
        assert_relative_eq!(b2.upper, 73.0 / 6.0 + 3.75, epsilon = 1e-12);
        assert_relative_eq!(b2.lower, 73.0 / 6.0 - 3.75, epsilon = 1e-12);
        let b3 = out[3].unwrap();
        assert_relative_eq!(b3.middle, 187.0 / 18.0, epsilon = 1e-12);
        assert_relative_eq!(b3.upper, 187.0 / 18.0 + 4.875, epsilon = 1e-12);
        assert_relative_eq!(b3.lower, 187.0 / 18.0 - 4.875, epsilon = 1e-12);
    }

    #[test]
    fn reset_reproduces_a_fresh_run() {
        let candles: Vec<Candle> = (0..60)
            .map(|i| {
                let m = 100.0 + (f64::from(i) * 0.3).sin() * 4.0;
                c(m + 1.2, m - 0.8, m)
            })
            .collect();
        let mut k = Keltner::new(7, 4, 1.5).unwrap();
        let first = k.batch(&candles);
        k.reset();
        let second = k.batch(&candles);
        assert_eq!(first, second);
        assert_eq!(second, Keltner::new(7, 4, 1.5).unwrap().batch(&candles));
    }
}
