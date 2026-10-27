//! Time Segmented Volume (Worden).

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Time Segmented Volume (Don Worden) — a rolling sum of *signed* volume
/// weighted by the bar's close-to-close move.
///
/// Each bar's contribution is the close change times the bar volume. Summed
/// over a fixed window, the result quantifies the net accumulation (positive)
/// or distribution (negative) over that span:
///
/// ```text
/// flow_t = (close_t − close_{t−1}) · volume_t          (signed money flow)
/// TSV_t  = Σ_{i = t−period+1}^{t} flow_i               (rolling window sum)
/// ```
///
/// The first candle only seeds `close_{t−1}`; the first flow lands at bar 2,
/// and the first TSV emission lands once the window has accumulated `period`
/// flows — i.e. at bar `period + 1`. Worden's original TC2000 implementation
/// often charts an additional EMA smoothing of TSV as a signal line; that is
/// left to the caller via [`crate::Ema`] composition.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Tsv};
///
/// let mut indicator = Tsv::new(18).unwrap();
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
pub struct Tsv {
    period: usize,
    prev_close: Option<f64>,
    window: VecDeque<f64>,
    sum: f64,
}

impl Tsv {
    /// Construct a new TSV with the given rolling window length.
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
            prev_close: None,
            window: VecDeque::with_capacity(period),
            sum: 0.0,
        })
    }

    /// Configured window length.
    pub const fn period(&self) -> usize {
        self.period
    }
}

impl Indicator for Tsv {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let Some(prev) = self.prev_close else {
            self.prev_close = Some(candle.close);
            return None;
        };
        let flow = (candle.close - prev) * candle.volume;
        self.prev_close = Some(candle.close);

        if self.window.len() == self.period {
            self.sum -= self.window.pop_front().expect("non-empty");
        }
        self.window.push_back(flow);
        self.sum += flow;
        if self.window.len() < self.period {
            return None;
        }
        Some(self.sum)
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.window.clear();
        self.sum = 0.0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One seed bar for `prev_close`, then `period` flows to fill the window.
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TSV"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(close: f64, volume: f64, ts: i64) -> Candle {
        Candle::new(close, close, close, close, volume, ts).unwrap()
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(Tsv::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let t = Tsv::new(18).unwrap();
        assert_eq!(t.period(), 18);
        assert_eq!(t.name(), "TSV");
        assert_eq!(t.warmup_period(), 19);
    }

    #[test]
    fn constant_close_yields_zero() {
        // Flat close -> every flow is zero -> rolling sum stays at zero.
        let candles: Vec<Candle> = (0..30).map(|i| c(10.0, 100.0, i)).collect();
        let mut t = Tsv::new(5).unwrap();
        for v in t.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn reference_window_sum() {
        // closes  = [10, 11, 13, 12, 14, 15]
        // volumes = [.., 100, 200, 150, 50, 200]
        // flows   = [None, (1)*100=100, (2)*200=400, (-1)*150=-150, (2)*50=100, (1)*200=200]
        // period = 3: first emission at bar index 3 (the 4th flow, since one bar seeds).
        // Wait: bar 0 seeds, bars 1..5 produce 5 flows. Window of 3 fills at the
        // 3rd flow, i.e. bar index 3.
        //   bar 3 -> window = [100, 400, -150] -> sum = 350.
        //   bar 4 -> window = [400, -150, 100] -> sum = 350.
        //   bar 5 -> window = [-150, 100, 200] -> sum = 150.
        let mut t = Tsv::new(3).unwrap();
        let out = t.batch(&[
            c(10.0, 50.0, 0),
            c(11.0, 100.0, 1),
            c(13.0, 200.0, 2),
            c(12.0, 150.0, 3),
            c(14.0, 50.0, 4),
            c(15.0, 200.0, 5),
        ]);
        assert!(out[0].is_none() && out[1].is_none() && out[2].is_none());
        assert_relative_eq!(out[3].unwrap(), 350.0, epsilon = 1e-9);
        assert_relative_eq!(out[4].unwrap(), 350.0, epsilon = 1e-9);
        assert_relative_eq!(out[5].unwrap(), 150.0, epsilon = 1e-9);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80i64)
            .map(|i| {
                let f = i as f64;
                c(
                    100.0 + (f * 0.3).sin() * 5.0,
                    50.0 + (i % 7) as f64 * 10.0,
                    i,
                )
            })
            .collect();
        let mut a = Tsv::new(18).unwrap();
        let mut b = Tsv::new(18).unwrap();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reset_clears_state() {
        let candles: Vec<Candle> = (0..40).map(|i| c(10.0 + i as f64, 100.0, i)).collect();
        let mut t = Tsv::new(10).unwrap();
        t.batch(&candles);
        assert!(t.is_ready());
        t.reset();
        assert!(!t.is_ready());
        assert_eq!(t.update(candles[0]), None);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Tsv::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
