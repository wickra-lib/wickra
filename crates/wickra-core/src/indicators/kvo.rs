//! Klinger Volume Oscillator.

use crate::error::{Error, Result};
use crate::indicators::ema::Ema;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Stephen J. Klinger's Volume Oscillator — a long/short-term volume-force
/// MACD with trend-aware cumulative-money-flow weighting.
///
/// Each bar produces a "volume force" (`vf`) whose sign tracks the daily trend
/// (`+1` on an up day, `−1` on a down day, carry-over otherwise) and whose
/// magnitude scales with how the current bar's range compares to the range
/// accumulated since the trend last flipped. The KVO line is the difference of two EMAs of `vf`:
///
/// ```text
/// hlc_t  = high_t + low_t + close_t                     (decides the trend)
/// trend  = sign(hlc_t − hlc_{t−1})   (carried over when equal)
/// dm_t   = high_t − low_t                               (the "daily measurement")
/// cm_t   = cm_{t−1} + dm_t          if trend unchanged
/// cm_t   = dm_{t−1} + dm_t          if trend just flipped
/// vf_t   = volume_t · |2·(dm_t/cm_t − 1)| · trend · 100
/// KVO_t  = EMA(vf, fast)_t − EMA(vf, slow)_t
/// ```
///
/// Klinger's textbook configuration is `fast = 34, slow = 55` on daily bars.
/// The first bar only seeds `dm_{t−1}`, so the very first `vf` lands at bar 2;
/// the slow EMA then needs `slow` raw `vf` values to seed, putting the first
/// KVO emission at bar `slow + 1`. A zero `cm_t` (only possible when every bar
/// since the last flip has a zero range) collapses `vf` to `0`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Kvo};
///
/// let mut indicator = Kvo::new(34, 55).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     let base = 100.0 + f64::from(i);
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Kvo {
    fast_period: usize,
    slow_period: usize,
    fast: Ema,
    slow: Ema,
    prev_dm: Option<f64>,
    prev_hlc: f64,
    trend: i8,
    cm: f64,
}

impl Kvo {
    /// Construct a new KVO with the given EMA periods.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if either period is zero, or
    /// [`Error::InvalidPeriod`] if `fast >= slow`.
    pub fn new(fast: usize, slow: usize) -> Result<Self> {
        if fast == 0 || slow == 0 {
            return Err(Error::PeriodZero);
        }
        if fast >= slow {
            return Err(Error::InvalidPeriod {
                message: "KVO needs fast < slow",
            });
        }
        Ok(Self {
            fast_period: fast,
            slow_period: slow,
            fast: Ema::new(fast)?,
            slow: Ema::new(slow)?,
            prev_dm: None,
            prev_hlc: 0.0,
            trend: 0,
            cm: 0.0,
        })
    }

    /// Klinger's classic configuration: `EMA(vf, 34) − EMA(vf, 55)`.
    pub fn classic() -> Self {
        Self::new(34, 55).expect("classic Klinger periods are valid")
    }

    /// Configured `(fast, slow)` periods.
    pub const fn periods(&self) -> (usize, usize) {
        (self.fast_period, self.slow_period)
    }
}

impl Indicator for Kvo {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let hlc = candle.high + candle.low + candle.close;
        let dm = candle.high - candle.low;
        let Some(prev_dm) = self.prev_dm else {
            // The first bar only establishes the previous bar's measurements.
            self.prev_dm = Some(dm);
            self.prev_hlc = hlc;
            return None;
        };

        // The trend sign compares H + L + C with the previous bar's.
        let new_trend: i8 = if hlc > self.prev_hlc {
            1
        } else if hlc < self.prev_hlc {
            -1
        } else {
            self.trend
        };

        // Cumulative measurement resets to (prev_dm + dm) whenever the trend
        // flips. On the very first sign read (trend was 0) we also seed from
        // the two-bar sum, matching the textbook definition.
        if new_trend != self.trend || self.trend == 0 {
            self.cm = prev_dm + dm;
        } else {
            self.cm += dm;
        }
        self.trend = new_trend;

        let vf = if self.cm == 0.0 {
            // Zero-range stretch since the flip — no force to register.
            0.0
        } else {
            candle.volume * (2.0 * (dm / self.cm - 1.0)).abs() * f64::from(new_trend) * 100.0
        };

        self.prev_dm = Some(dm);
        self.prev_hlc = hlc;

        let fast = self.fast.update(vf);
        let slow = self.slow.update(vf);
        Some(fast? - slow?)
    }

    fn reset(&mut self) {
        self.fast.reset();
        self.slow.reset();
        self.prev_dm = None;
        self.prev_hlc = 0.0;
        self.trend = 0;
        self.cm = 0.0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One bar to seed `prev_dm`, then the slow EMA needs `slow` raw `vf` values.
        self.slow_period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.fast.is_ready() && self.slow.is_ready()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "KVO"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(high: f64, low: f64, close: f64, volume: f64, ts: i64) -> Candle {
        Candle::new(low, high, low, close, volume, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Kvo::new(0, 10), Err(Error::PeriodZero)));
        assert!(matches!(Kvo::new(3, 0), Err(Error::PeriodZero)));
    }

    #[test]
    fn rejects_fast_geq_slow() {
        assert!(matches!(Kvo::new(34, 34), Err(Error::InvalidPeriod { .. })));
        assert!(matches!(Kvo::new(55, 34), Err(Error::InvalidPeriod { .. })));
    }

    #[test]
    fn accessors_and_metadata() {
        let k = Kvo::classic();
        assert_eq!(k.periods(), (34, 55));
        assert_eq!(k.name(), "KVO");
        assert_eq!(k.warmup_period(), 56);
    }

    #[test]
    fn zero_ohlc_collapses_vf_to_zero() {
        // Two consecutive all-zero bars: dm = 0 for both, so prev_dm + dm = 0
        // and `cm == 0.0` fires the defensive branch, holding vf at zero.
        let mut k = Kvo::new(3, 6).unwrap();
        let zero = Candle::new(0.0, 0.0, 0.0, 0.0, 100.0, 0).unwrap();
        assert_eq!(k.update(zero), None);
        assert_eq!(k.update(zero), None);
        assert_eq!(k.update(zero), None);
    }

    #[test]
    fn constant_series_yields_zero() {
        // dm flat -> trend never sets to a nonzero sign and vf collapses to 0
        // for every bar; both EMAs hold at 0 once seeded.
        let candles: Vec<Candle> = (0..120).map(|i| c(10.0, 10.0, 10.0, 100.0, i)).collect();
        let mut k = Kvo::new(3, 6).unwrap();
        for v in k.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn warmup_emits_at_slow_plus_one() {
        let candles: Vec<Candle> = (0..30i64)
            .map(|i| {
                let f = i as f64;
                c(10.0 + f, 8.0 + f, 9.0 + f, 100.0, i)
            })
            .collect();
        let mut k = Kvo::new(3, 5).unwrap();
        let out = k.batch(&candles);
        for (i, v) in out.iter().enumerate().take(5) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        // First emission lands at index slow_period (one seed bar + slow EMA seeding from there).
        assert!(out[5].is_some(), "first value lands at slow_period");
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..100i64)
            .map(|i| {
                let f = i as f64;
                let mid = 100.0 + (f * 0.2).sin() * 4.0;
                c(mid + 1.0, mid - 1.0, mid, 10.0 + ((i % 5) as f64), i)
            })
            .collect();
        let mut a = Kvo::classic();
        let mut b = Kvo::classic();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..80i64)
            .map(|i| {
                let f = i as f64;
                c(11.0 + f, 9.0 + f, 10.0 + f, 100.0, i)
            })
            .collect();
        let mut k = Kvo::classic();
        k.batch(&candles);
        assert!(k.is_ready());
        k.reset();
        assert!(!k.is_ready());
        assert_eq!(k.update(candles[0]), None);
    }

    fn wave(len: i64) -> Vec<Candle> {
        (0..len)
            .map(|i| {
                let step = f64::from(i32::try_from(i).unwrap());
                let mid = 100.0 + (step * 0.31).sin() * 4.0;
                let half = 0.6 + (step * 0.17).cos().abs();
                c(
                    mid + half,
                    mid - half,
                    mid + 0.3 * half,
                    50.0 + (step * 0.7).sin() * 20.0,
                    i,
                )
            })
            .collect()
    }

    #[test]
    fn rejects_period_above_max() {
        let too_big = crate::error::MAX_PERIOD + 1;
        assert!(matches!(
            Kvo::new(3, too_big),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn hand_computed_reference() {
        // fast EMA(2) alpha 2/3, slow EMA(3) alpha 1/2; both seeded with the mean.
        //   bar  H    L    C     V    hlc   dm   trend  cm               vf
        //   b0   10   8    9     100  27    2    seed
        //   b1   11   9    10    100  30    2    +1     2 + 2 = 4 (first) 100·|2(2/4 − 1)|·100  = 10000
        //   b2   12   9    11    200  32    3    +1     4 + 3 = 7         200·|2(3/7 − 1)|·100  = 160000/7
        //   b3   11   10   10.5  100  31.5  1    −1     3 + 1 = 4 (flip)  −100·|2(1/4 − 1)|·100 = −15000
        //   b4   11   10   10.5  100  31.5  1    −1     4 + 1 = 5 (carry) −100·|2(1/5 − 1)|·100 = −16000
        //   b5   12   9.5  11    100  32.5  2.5  +1     1 + 2.5 = 3.5     100·|2(2.5/3.5 − 1)|·100 = 40000/7
        // fast: f2 = (10000 + 160000/7)/2 = 115000/7
        //       f3 = 2/3·(−15000) + 1/3·f2 = −95000/21
        //       f4 = 2/3·(−16000) + 1/3·f3 = −767000/63
        // slow: s3 = (10000 + 160000/7 − 15000)/3 = 125000/21
        //       s4 = 1/2·(−16000) + 1/2·s3 = −316500/63
        // KVO3 = f3 − s3 = −220000/21, KVO4 = f4 − s4 = −450500/63
        // b5:  f5 = 2/3·40000/7 + 1/3·f4, s5 = 1/2·40000/7 + 1/2·s4
        let candles = [
            c(10.0, 8.0, 9.0, 100.0, 0),
            c(11.0, 9.0, 10.0, 100.0, 1),
            c(12.0, 9.0, 11.0, 200.0, 2),
            c(11.0, 10.0, 10.5, 100.0, 3),
            c(11.0, 10.0, 10.5, 100.0, 4),
            c(12.0, 9.5, 11.0, 100.0, 5),
        ];
        let mut k = Kvo::new(2, 3).unwrap();
        assert_eq!(k.warmup_period(), 4);
        let out = k.batch(&candles);
        assert!(out[..3].iter().all(Option::is_none));
        assert_relative_eq!(out[3].unwrap(), -220_000.0 / 21.0, max_relative = 1e-12);
        assert_relative_eq!(out[4].unwrap(), -450_500.0 / 63.0, max_relative = 1e-12);
        let f5 = 2.0 / 3.0 * (40_000.0 / 7.0) + (-767_000.0 / 63.0) / 3.0;
        let s5 = 0.5 * (40_000.0 / 7.0) + 0.5 * (-316_500.0 / 63.0);
        assert_relative_eq!(out[5].unwrap(), f5 - s5, max_relative = 1e-12);
    }

    #[test]
    fn zero_volume_yields_zero() {
        // vf scales with volume, so a zero-volume series gives KVO = 0.
        let candles: Vec<Candle> = wave(30)
            .into_iter()
            .map(|x| Candle::new(x.open, x.high, x.low, x.close, 0.0, x.timestamp).unwrap())
            .collect();
        let out = Kvo::new(3, 5).unwrap().batch(&candles);
        assert!(out[5..].iter().all(|v| v.is_some_and(|x| x.abs() < 1e-12)));
    }

    #[test]
    fn zero_range_after_a_flip_registers_no_force() {
        // b1 sets an up trend, b2 flips down with b1 and b2 both zero-range,
        // so cm = 0 + 0 and vf is 0 instead of a division by zero.
        let mut k = Kvo::new(1, 2).unwrap();
        assert_eq!(k.update(c(10.0, 8.0, 9.0, 100.0, 0)), None);
        assert_eq!(k.update(c(11.0, 11.0, 11.0, 100.0, 1)), None);
        // vf1 = 100·|2(0/2 − 1)|·100 = 20000, vf2 = 0 -> fast = 0, slow = 10000.
        let v = k.update(c(10.0, 10.0, 10.0, 100.0, 2)).unwrap();
        assert_relative_eq!(v, -10_000.0, epsilon = 1e-9);
    }

    #[test]
    fn reset_reproduces_a_fresh_run() {
        let candles = wave(80);
        let mut k = Kvo::new(5, 13).unwrap();
        let first = k.batch(&candles);
        k.reset();
        let second = k.batch(&candles);
        assert_eq!(first, second);
        assert_eq!(second, Kvo::new(5, 13).unwrap().batch(&candles));
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let candles = wave(80);
        let mut streaming = Kvo::new(5, 13).unwrap();
        let expected: Vec<u64> = candles
            .iter()
            .map(|x| streaming.update(*x).unwrap_or(f64::NAN).to_bits())
            .collect();
        let mut out = vec![0.0; candles.len()];
        Kvo::new(5, 13).unwrap().batch_nan_into(&candles, &mut out);
        let got: Vec<u64> = out.iter().map(|v| v.to_bits()).collect();
        assert_eq!(got, expected);
    }
}
