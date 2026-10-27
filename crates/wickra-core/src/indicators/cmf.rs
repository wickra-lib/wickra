//! Chaikin Money Flow (CMF).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Chaikin Money Flow — Marc Chaikin's `period`-window money-flow oscillator.
///
/// Each bar produces a *money-flow volume*: the bar's volume weighted by where
/// the close fell within its range (the same money-flow multiplier the
/// [`Adl`](crate::Adl) uses). CMF is the ratio of summed money-flow volume to
/// summed volume over the lookback window:
///
/// ```text
/// MFM_t = ((close − low) − (high − close)) / (high − low)   (−1..+1)
/// MFV_t = MFM_t · volume_t
/// CMF_t = Σ(MFV, period) / Σ(volume, period)
/// ```
///
/// The result lives in `[−1, +1]`: sustained closes near the high push CMF
/// toward `+1` (accumulation), near the low toward `−1` (distribution). A bar
/// with `high == low` carries no positional information and contributes a
/// money-flow volume of `0`; a window whose total volume is zero yields `0.0`
/// by convention.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, ChaikinMoneyFlow};
///
/// let mut indicator = ChaikinMoneyFlow::new(20).unwrap();
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
pub struct ChaikinMoneyFlow {
    period: usize,
    mfv_window: VecDeque<f64>,
    vol_window: VecDeque<f64>,
    mfv_sum: f64,
    vol_sum: f64,
}

impl ChaikinMoneyFlow {
    /// Construct a new Chaikin Money Flow over `period` bars.
    ///
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
            mfv_window: VecDeque::with_capacity(period),
            vol_window: VecDeque::with_capacity(period),
            mfv_sum: 0.0,
            vol_sum: 0.0,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for ChaikinMoneyFlow {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let range = candle.high - candle.low;
        let mfv = if range == 0.0 {
            // A zero-range bar carries no positional information.
            0.0
        } else {
            let mfm = ((candle.close - candle.low) - (candle.high - candle.close)) / range;
            mfm * candle.volume
        };

        if self.mfv_window.len() == self.period {
            self.mfv_sum -= self.mfv_window.pop_front().expect("non-empty");
            self.vol_sum -= self.vol_window.pop_front().expect("non-empty");
        }
        self.mfv_window.push_back(mfv);
        self.vol_window.push_back(candle.volume);
        self.mfv_sum += mfv;
        self.vol_sum += candle.volume;

        if self.mfv_window.len() < self.period {
            return None;
        }
        if self.vol_sum == 0.0 {
            // No volume traded across the whole window — no flow to report.
            return Some(0.0);
        }
        Some(self.mfv_sum / self.vol_sum)
    }

    fn reset(&mut self) {
        self.mfv_window.clear();
        self.vol_window.clear();
        self.mfv_sum = 0.0;
        self.vol_sum = 0.0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.mfv_window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "CMF"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn candle(open: f64, high: f64, low: f64, close: f64, volume: f64, ts: i64) -> Candle {
        Candle::new(open, high, low, close, volume, ts).unwrap()
    }

    #[test]
    fn reference_values() {
        // CMF(2): bar 1 closes at the high -> MFM = +1, MFV = +100.
        //         bar 2 closes mid-range -> MFM = 0, MFV = 0.
        // CMF = (100 + 0) / (100 + 100) = 0.5.
        let mut cmf = ChaikinMoneyFlow::new(2).unwrap();
        let out = cmf.batch(&[
            candle(8.0, 10.0, 8.0, 10.0, 100.0, 0),
            candle(10.0, 12.0, 8.0, 10.0, 100.0, 1),
        ]);
        assert!(out[0].is_none());
        assert_relative_eq!(out[1].unwrap(), 0.5, epsilon = 1e-12);
    }

    #[test]
    fn stays_within_unit_range() {
        let candles: Vec<Candle> = (0..120)
            .map(|i| {
                let mid = 100.0 + (i as f64 * 0.25).sin() * 10.0;
                candle(
                    mid,
                    mid + 3.0,
                    mid - 3.0,
                    mid + (i as f64 * 0.5).cos() * 2.0,
                    10.0 + (i % 7) as f64,
                    i,
                )
            })
            .collect();
        let mut cmf = ChaikinMoneyFlow::new(20).unwrap();
        for v in cmf.batch(&candles).into_iter().flatten() {
            assert!((-1.0..=1.0).contains(&v), "CMF {v} outside [-1, 1]");
        }
    }

    #[test]
    fn closes_at_high_yield_cmf_one() {
        // Every bar closes on its high -> MFM = +1 -> CMF saturates at +1.
        let candles: Vec<Candle> = (0..30)
            .map(|i| candle(9.0, 10.0, 8.0, 10.0, 50.0, i))
            .collect();
        let mut cmf = ChaikinMoneyFlow::new(14).unwrap();
        for v in cmf.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 1.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn zero_volume_window_yields_zero() {
        // A window with no traded volume divides 0/0 — defined as 0.0.
        let candles: Vec<Candle> = (0..20)
            .map(|i| candle(9.0, 10.0, 8.0, 10.0, 0.0, i))
            .collect();
        let mut cmf = ChaikinMoneyFlow::new(10).unwrap();
        for v in cmf.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn first_value_on_period_th_candle() {
        let candles: Vec<Candle> = (0..10)
            .map(|i| candle(9.0, 10.0, 8.0, 9.5, 50.0, i))
            .collect();
        let mut cmf = ChaikinMoneyFlow::new(5).unwrap();
        let out = cmf.batch(&candles);
        for (i, v) in out.iter().enumerate().take(4) {
            assert!(v.is_none(), "index {i} must be None during warmup");
        }
        assert!(out[4].is_some(), "first CMF lands at index period - 1");
        assert_eq!(cmf.warmup_period(), 5);
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(ChaikinMoneyFlow::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessor `period` (71-73) and the Indicator-impl
    /// `name` body (124-126). `warmup_period` is covered elsewhere.
    #[test]
    fn accessors_and_metadata() {
        let cmf = ChaikinMoneyFlow::new(20).unwrap();
        assert_eq!(cmf.period(), 20);
        assert_eq!(cmf.name(), "CMF");
    }

    /// Cover the `range == 0.0` defensive branch (line 84). All other
    /// tests use H != L candles; feed all-flat candles (H == L) so the
    /// MFV computation must take the zero-range fallback and emit MFV = 0.
    #[test]
    fn zero_range_candle_contributes_zero_mfv() {
        let mut cmf = ChaikinMoneyFlow::new(3).unwrap();
        let candles: Vec<Candle> = (0..5)
            .map(|i| Candle::new(10.0, 10.0, 10.0, 10.0, 5.0, i).unwrap())
            .collect();
        let last = cmf
            .batch(&candles)
            .into_iter()
            .flatten()
            .last()
            .expect("emits");
        // Every bar contributed 0 to mfv_sum, so the ratio is 0.
        assert_eq!(last, 0.0);
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..20)
            .map(|i| candle(9.0, 11.0, 8.0, 10.0, 50.0, i))
            .collect();
        let mut cmf = ChaikinMoneyFlow::new(10).unwrap();
        cmf.batch(&candles);
        assert!(cmf.is_ready());
        cmf.reset();
        assert!(!cmf.is_ready());
        assert_eq!(cmf.update(candles[0]), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let mid = 100.0 + (i as f64 * 0.3).sin() * 8.0;
                candle(
                    mid,
                    mid + 2.0,
                    mid - 2.0,
                    mid + 0.5,
                    10.0 + (i % 5) as f64,
                    i,
                )
            })
            .collect();
        let mut a = ChaikinMoneyFlow::new(20).unwrap();
        let mut b = ChaikinMoneyFlow::new(20).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            ChaikinMoneyFlow::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
