//! Inertia (Donald Dorsey).

use crate::error::{Error, Result};
use crate::indicators::linreg::LinearRegression;
use crate::indicators::rvi_volatility::RviVolatility;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Donald Dorsey's Inertia — a Linear-Regression-smoothed Relative
/// **Volatility** Index (Dorsey, Technical Analysis of Stocks & Commodities,
/// 1995). The endpoint of an `n`-bar least-squares fit of the
/// [`RviVolatility`](crate::RviVolatility) series of the close is taken as the
/// indicator's reading: the direction of volatility, smoothed into a trend gauge
/// (above `50` bullish inertia, below `50` bearish).
///
/// ```text
/// Inertia_t = LinearRegression(RelativeVolatilityIndex(close, rvi_period), linreg_period)_t
/// ```
///
/// Dorsey's recommended defaults are `(rvi_period = 14, linreg_period = 20)`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Inertia};
///
/// let mut inertia = Inertia::new(14, 20).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     let o = 100.0 + f64::from(i);
///     let c = o + 0.5;
///     let candle = Candle::new(o, c + 0.2, o - 0.2, c, 1.0, i64::from(i)).unwrap();
///     last = inertia.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Inertia {
    rvi_period: usize,
    linreg_period: usize,
    rvi: RviVolatility,
    linreg: LinearRegression,
}

impl Inertia {
    /// # Errors
    /// Returns [`Error::PeriodZero`] if either period is zero.
    pub fn new(rvi_period: usize, linreg_period: usize) -> Result<Self> {
        if rvi_period == 0 || linreg_period == 0 {
            return Err(Error::PeriodZero);
        }
        Ok(Self {
            rvi_period,
            linreg_period,
            rvi: RviVolatility::new(rvi_period)?,
            linreg: LinearRegression::new(linreg_period)?,
        })
    }

    /// Dorsey's recommended defaults `(rvi_period = 14, linreg_period = 20)`.
    pub fn classic() -> Self {
        Self::new(14, 20).expect("classic Inertia parameters are valid")
    }

    /// Configured `(rvi_period, linreg_period)`.
    pub const fn periods(&self) -> (usize, usize) {
        (self.rvi_period, self.linreg_period)
    }
}

impl Indicator for Inertia {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let rvi = self.rvi.update(candle.close)?;
        self.linreg.update(rvi)
    }

    fn reset(&mut self) {
        self.rvi.reset();
        self.linreg.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // The Relative Volatility Index emits at its own warmup; the
        // LinearRegression then needs `linreg_period − 1` more values.
        self.rvi.warmup_period() + self.linreg_period - 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.linreg.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "Inertia"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn candle(open: f64, high: f64, low: f64, close: f64, ts: i64) -> Candle {
        Candle::new(open, high, low, close, 1.0, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Inertia::new(0, 20), Err(Error::PeriodZero)));
        assert!(matches!(Inertia::new(14, 0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let inertia = Inertia::classic();
        assert_eq!(inertia.periods(), (14, 20));
        assert_eq!(inertia.warmup_period(), 46);
        assert_eq!(inertia.name(), "Inertia");
    }

    #[test]
    fn classic_factory() {
        assert_eq!(Inertia::classic().periods(), (14, 20));
    }

    #[test]
    fn warmup_emits_first_value_at_warmup_period() {
        // Smaller periods for a fast test: the Relative Volatility Index (3)
        // emits at 2·3 − 1 = 5 candles, then LinReg(4) needs 4 values ->
        // total 5 + 4 - 1 = 8.
        let mut inertia = Inertia::new(3, 4).unwrap();
        assert_eq!(inertia.warmup_period(), 8);
        for i in 0..7 {
            assert_eq!(inertia.update(candle(10.0, 11.0, 9.0, 10.5, i)), None);
        }
        assert!(inertia.update(candle(10.0, 11.0, 9.0, 10.5, 7)).is_some());
    }

    #[test]
    fn constant_rvi_yields_constant_inertia() {
        // Every bar identical -> the close never moves, the Relative
        // Volatility Index sits at its neutral 50, and LinReg of a constant
        // series equals that constant after warmup.
        let mut inertia = Inertia::new(3, 4).unwrap();
        let mut last = None;
        for i in 0..40 {
            last = inertia.update(candle(10.0, 11.0, 9.0, 10.5, i));
        }
        let v = last.unwrap();
        assert_relative_eq!(v, 50.0, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80_i64)
            .map(|i| {
                let o = 100.0 + (i as f64 * 0.3).sin() * 5.0;
                let c = o + (i as f64 * 0.1).cos();
                candle(o, o.max(c) + 0.5, o.min(c) - 0.5, c, i)
            })
            .collect();
        let batch = Inertia::classic().batch(&candles);
        let mut b = Inertia::classic();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn reset_clears_state() {
        let mut inertia = Inertia::classic();
        for i in 0..50 {
            inertia.update(candle(10.0, 11.0, 9.0, 10.5, i));
        }
        assert!(inertia.is_ready());
        inertia.reset();
        assert!(!inertia.is_ready());
        assert_eq!(inertia.update(candle(10.0, 11.0, 9.0, 10.5, 0)), None);
    }

    fn wave(len: i64) -> Vec<Candle> {
        (0..len)
            .map(|i| {
                let step = f64::from(i32::try_from(i).unwrap());
                let o = 100.0 + (step * 0.37).sin() * 6.0;
                let cl = o + (step * 0.11).cos() * 1.5;
                candle(o, o.max(cl) + 0.4, o.min(cl) - 0.4, cl, i)
            })
            .collect()
    }

    fn close_only(close: f64, ts: i64) -> Candle {
        candle(close, close, close, close, ts)
    }

    #[test]
    fn rejects_invalid_sub_periods() {
        // The Relative Volatility Index needs period >= 2 and LinReg needs
        // period >= 2: both surface as InvalidPeriod through the `?`.
        assert!(matches!(
            Inertia::new(1, 20),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Inertia::new(14, 1),
            Err(Error::InvalidPeriod { .. })
        ));
        let too_big = crate::error::MAX_PERIOD + 1;
        assert!(matches!(
            Inertia::new(too_big, 20),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            Inertia::new(14, too_big),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn classic_first_value_lands_at_index_45() {
        // Classic: RVI(14) emits at 2·14 − 1 = 27 candles, LinReg(20) needs 19
        // more -> warmup 46, first value at input index 45.
        let candles = wave(60);
        let out = Inertia::classic().batch(&candles);
        let warmup = Inertia::classic().warmup_period();
        assert_eq!(warmup, 46);
        assert!(out[..warmup - 1].iter().all(Option::is_none));
        assert!(out[warmup - 1..].iter().all(Option::is_some));
    }

    #[test]
    fn hand_computed_reference_rvi2_linreg3() {
        // RVI(2) uses the population stddev of the last two closes, so
        // sd = |a − b| / 2, classed as up / down volatility by the close move,
        // seeded with the mean of the first 2 samples and Wilder-smoothed after.
        // closes: 10, 12, 11, 15, 14, 18
        //   idx1: sd 1   up   (seed)
        //   idx2: sd 0.5 down -> au = 0.5,  ad = 0.25   RVI = 200/3
        //   idx3: sd 2   up   -> au = 1.25, ad = 0.125  RVI = 1000/11
        //   idx4: sd 0.5 down -> au = 0.625, ad = 0.3125 RVI = 200/3
        //   idx5: sd 2   up   -> au = 1.3125, ad = 0.15625 RVI = 4200/47
        // LinReg(3) endpoint of (y0, y1, y2) = (5·y2 + 2·y1 − y0) / 6.
        //   idx4: (5·200/3 + 2·1000/11 − 200/3) / 6 = 14800/198
        //   idx5: (5·4200/47 + 2·200/3 − 1000/11) / 6
        let closes = [10.0, 12.0, 11.0, 15.0, 14.0, 18.0];
        let mut inertia = Inertia::new(2, 3).unwrap();
        assert_eq!(inertia.warmup_period(), 5);
        let out: Vec<Option<f64>> = closes
            .iter()
            .zip(0_i64..)
            .map(|(&cl, ts)| inertia.update(close_only(cl, ts)))
            .collect();
        assert!(out[..4].iter().all(Option::is_none));
        assert_relative_eq!(out[4].unwrap(), 14800.0 / 198.0, epsilon = 1e-9);
        let expected5 = (5.0 * 4200.0 / 47.0 + 2.0 * 200.0 / 3.0 - 1000.0 / 11.0) / 6.0;
        assert_relative_eq!(out[5].unwrap(), expected5, epsilon = 1e-9);
    }

    #[test]
    fn reset_reproduces_a_fresh_run() {
        let candles = wave(90);
        let mut inertia = Inertia::classic();
        let first = inertia.batch(&candles);
        inertia.reset();
        let second = inertia.batch(&candles);
        let fresh = Inertia::classic().batch(&candles);
        assert_eq!(first, second);
        assert_eq!(second, fresh);
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let candles = wave(90);
        let mut streaming = Inertia::new(5, 7).unwrap();
        let expected: Vec<u64> = candles
            .iter()
            .map(|c| streaming.update(*c).unwrap_or(f64::NAN).to_bits())
            .collect();
        let mut out = vec![0.0; candles.len()];
        Inertia::new(5, 7)
            .unwrap()
            .batch_nan_into(&candles, &mut out);
        let got: Vec<u64> = out.iter().map(|v| v.to_bits()).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn flat_closes_hold_neutral_fifty() {
        // A flat close has zero stddev on both sides: RVI's undefined ratio is
        // 50, and LinReg of a constant is the constant.
        let mut inertia = Inertia::new(2, 3).unwrap();
        let out: Vec<Option<f64>> = (0..8)
            .map(|ts| inertia.update(close_only(7.0, ts)))
            .collect();
        assert!(out[4..]
            .iter()
            .all(|v| v.is_some_and(|x| (x - 50.0).abs() < 1e-12)));
    }
}
