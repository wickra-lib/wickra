//! Relative Vigor Index (RVI).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Relative Vigor Index — John Ehlers' ratio of intra-bar drive (close − open)
/// to intra-bar range (high − low) (Technical Analysis of Stocks & Commodities,
/// January 2002).
///
/// Both series are first smoothed with Ehlers' symmetric four-bar `1-2-2-1`
/// weighting, then summed over `period` bars:
///
/// ```text
/// num_t = ((C−O)_t + 2·(C−O)_{t−1} + 2·(C−O)_{t−2} + (C−O)_{t−3}) / 6
/// den_t = ((H−L)_t + 2·(H−L)_{t−1} + 2·(H−L)_{t−2} + (H−L)_{t−3}) / 6
/// RVI_t = Σ_period num / Σ_period den
/// ```
///
/// A positive value means the bars in the window closed above where they opened
/// (bullish "vigor"); a negative value means they closed below. Ehlers' signal
/// line is the same `1-2-2-1` weighting of the RVI itself. The denominator sum
/// can fall to zero on a perfectly flat stretch, in which case the ratio is
/// undefined and the indicator holds its previous value. The first value lands
/// on bar `period + 3`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Rvi};
///
/// let mut rvi = Rvi::new(10).unwrap();
/// let mut last = None;
/// for i in 0..40 {
///     let o = 100.0 + f64::from(i);
///     let c = o + 0.5;
///     let candle = Candle::new(o, c + 0.2, o - 0.2, c, 1.0, i64::from(i)).unwrap();
///     last = rvi.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Rvi {
    period: usize,
    /// The last four raw `(close − open, high − low)` pairs, oldest first.
    raw: VecDeque<(f64, f64)>,
    /// The last `period` weighted pairs.
    window: VecDeque<(f64, f64)>,
    sum_num: f64,
    sum_den: f64,
    current: Option<f64>,
}

impl Rvi {
    /// # Errors
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
            raw: VecDeque::with_capacity(4),
            window: VecDeque::with_capacity(period),
            sum_num: 0.0,
            sum_den: 0.0,
            current: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.current
    }
}

impl Indicator for Rvi {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        if self.raw.len() == 4 {
            self.raw.pop_front();
        }
        self.raw
            .push_back((candle.close - candle.open, candle.high - candle.low));
        if self.raw.len() < 4 {
            return None;
        }
        // Symmetric 1-2-2-1 weighting, newest at index 3.
        let (n3, d3) = self.raw[0];
        let (n2, d2) = self.raw[1];
        let (n1, d1) = self.raw[2];
        let (n0, d0) = self.raw[3];
        let num = (n0 + 2.0 * n1 + 2.0 * n2 + n3) / 6.0;
        let den = (d0 + 2.0 * d1 + 2.0 * d2 + d3) / 6.0;
        if self.window.len() == self.period {
            let (old_n, old_d) = self.window.pop_front().expect("window is non-empty");
            self.sum_num -= old_n;
            self.sum_den -= old_d;
        }
        self.window.push_back((num, den));
        self.sum_num += num;
        self.sum_den += den;
        if self.window.len() < self.period {
            return None;
        }
        if self.sum_den <= 0.0 {
            // Window of perfectly flat (zero-range) bars: ratio undefined.
            // Hold the previous value rather than emitting NaN / inf.
            return self.current;
        }
        let value = self.sum_num / self.sum_den;
        self.current = Some(value);
        Some(value)
    }

    fn reset(&mut self) {
        self.raw.clear();
        self.window.clear();
        self.sum_num = 0.0;
        self.sum_den = 0.0;
        self.current = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period + 3
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.current.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "RVI"
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
        assert!(matches!(Rvi::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut r = Rvi::new(10).unwrap();
        assert_eq!(r.period(), 10);
        assert_eq!(r.warmup_period(), 13);
        assert_eq!(r.name(), "RVI");
        assert_eq!(r.value(), None);
        for i in 0..13 {
            r.update(candle(10.0, 11.0, 9.0, 10.5, i));
        }
        assert!(r.value().is_some());
    }

    #[test]
    fn reference_value_period_2() {
        // Four bars (10, 11, 9, 10.5): num 0.5, den 2.0 each, so the first
        // weighted pair is (0.5, 2.0). A fifth bar (10.5, 11.5, 10, 11.5) has
        // num 1.0, den 1.5: weighted num = (1 + 2·0.5 + 2·0.5 + 0.5) / 6 = 3.5/6,
        // den = (1.5 + 2·2 + 2·2 + 2) / 6 = 11.5/6.
        //   RVI = (0.5 + 3.5/6) / (2 + 11.5/6) = 6.5 / 23.5
        let mut r = Rvi::new(2).unwrap();
        for i in 0..4 {
            assert_eq!(r.update(candle(10.0, 11.0, 9.0, 10.5, i)), None);
        }
        let v = r.update(candle(10.5, 11.5, 10.0, 11.5, 4)).unwrap();
        assert_relative_eq!(v, 6.5 / 23.5, epsilon = 1e-12);
    }

    #[test]
    fn warmup_emits_first_value_at_period_plus_three() {
        let mut r = Rvi::new(3).unwrap();
        for i in 0..5 {
            assert_eq!(r.update(candle(10.0, 11.0, 9.0, 10.5, i)), None);
        }
        assert!(r.update(candle(10.5, 11.5, 10.0, 11.0, 5)).is_some());
    }

    #[test]
    fn pure_uptrend_is_positive() {
        // Every bar closes above its open and has a non-zero range: RVI > 0.
        let mut r = Rvi::new(5).unwrap();
        for i in 0..10 {
            let o = 10.0 + f64::from(i);
            let c = o + 0.5;
            r.update(candle(o, c + 0.2, o - 0.2, c, i64::from(i)));
        }
        let v = r.value().unwrap();
        assert!(v > 0.0, "uptrend RVI should be positive: {v}");
    }

    #[test]
    fn zero_range_window_holds_value() {
        // Window of perfectly flat bars (high == low): ratio undefined,
        // indicator holds.
        let mut r = Rvi::new(3).unwrap();
        for i in 0..5 {
            r.update(candle(10.0, 10.0, 10.0, 10.0, i));
        }
        assert_eq!(r.update(candle(10.0, 10.0, 10.0, 10.0, 5)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..40_i64)
            .map(|i| {
                let o = 100.0 + (i as f64 * 0.3).sin() * 5.0;
                let c = o + (i as f64 * 0.1).cos();
                candle(o, o.max(c) + 0.5, o.min(c) - 0.5, c, i)
            })
            .collect();
        let batch = Rvi::new(10).unwrap().batch(&candles);
        let mut b = Rvi::new(10).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn reset_clears_state() {
        let mut r = Rvi::new(5).unwrap();
        for i in 0..10 {
            r.update(candle(10.0, 11.0, 9.0, 10.5, i));
        }
        assert!(r.is_ready());
        r.reset();
        assert!(!r.is_ready());
        assert_eq!(r.update(candle(10.0, 11.0, 9.0, 10.5, 0)), None);
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

    #[test]
    fn rejects_period_above_max() {
        let too_big = crate::error::MAX_PERIOD + 1;
        assert!(matches!(
            Rvi::new(too_big),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn warmup_is_exact_for_several_periods() {
        let candles = wave(40);
        for period in [1_usize, 2, 5, 10] {
            let mut r = Rvi::new(period).unwrap();
            let warmup = r.warmup_period();
            let out = r.batch(&candles);
            assert!(out[..warmup - 1].iter().all(Option::is_none));
            assert!(out[warmup - 1..].iter().all(Option::is_some));
        }
    }

    #[test]
    fn reference_value_period_1_is_one_weighted_bar() {
        // period = 1: RVI is the single 1-2-2-1 weighted ratio of the last 4 bars.
        //   bars (C−O, H−L): (1, 2), (−0.5, 1), (2, 4), (0.5, 3)
        //   num = (0.5 + 2·2 + 2·(−0.5) + 1) / 6 = 4.5 / 6
        //   den = (3 + 2·4 + 2·1 + 2) / 6 = 15 / 6
        //   RVI = 4.5 / 15 = 0.3
        let mut r = Rvi::new(1).unwrap();
        assert_eq!(r.update(candle(10.0, 11.5, 9.5, 11.0, 0)), None);
        assert_eq!(r.update(candle(11.0, 11.5, 10.5, 10.5, 1)), None);
        assert_eq!(r.update(candle(10.0, 13.0, 9.0, 12.0, 2)), None);
        let v = r.update(candle(12.0, 14.0, 11.0, 12.5, 3)).unwrap();
        assert_relative_eq!(v, 0.3, epsilon = 1e-12);
    }

    #[test]
    fn downtrend_is_negative() {
        let mut r = Rvi::new(3).unwrap();
        let mut last = None;
        for i in 0..10_i32 {
            let o = 50.0 - f64::from(i);
            let cl = o - 0.5;
            last = r.update(candle(o, o + 0.2, cl - 0.2, cl, i64::from(i)));
        }
        assert!(last.unwrap() < 0.0);
    }

    #[test]
    fn flat_stretch_after_movement_holds_previous_value() {
        // period = 1: once the last four bars are all zero-range, the weighted
        // denominator is exactly zero and the previous value is held.
        let mut r = Rvi::new(1).unwrap();
        for i in 0..4 {
            r.update(candle(10.0, 11.0, 9.0, 10.5, i));
        }
        let before = r.value().unwrap();
        assert_relative_eq!(before, 0.25, epsilon = 1e-12);
        for i in 4..10 {
            let v = r.update(candle(10.0, 10.0, 10.0, 10.0, i)).unwrap();
            assert_eq!(v.to_bits(), before.to_bits());
        }
        assert!(r.is_ready());
    }

    #[test]
    fn reset_reproduces_a_fresh_run() {
        let candles = wave(50);
        let mut r = Rvi::new(10).unwrap();
        let first = r.batch(&candles);
        r.reset();
        assert_eq!(r.value(), None);
        let second = r.batch(&candles);
        assert_eq!(first, second);
        assert_eq!(second, Rvi::new(10).unwrap().batch(&candles));
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let candles = wave(50);
        let mut streaming = Rvi::new(4).unwrap();
        let expected: Vec<u64> = candles
            .iter()
            .map(|c| streaming.update(*c).unwrap_or(f64::NAN).to_bits())
            .collect();
        let mut out = vec![0.0; candles.len()];
        Rvi::new(4).unwrap().batch_nan_into(&candles, &mut out);
        let got: Vec<u64> = out.iter().map(|v| v.to_bits()).collect();
        assert_eq!(got, expected);
    }
}
