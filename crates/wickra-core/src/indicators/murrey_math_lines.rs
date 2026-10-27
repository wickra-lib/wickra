//! Murrey Math Lines — the eighths grid over the recent trading range.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Output of [`MurreyMathLines`]: the nine Murrey Math levels from the bottom
/// (`mm0_8`, ultimate support) to the top (`mm8_8`, ultimate resistance).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MurreyMathLinesOutput {
    /// 8/8 — ultimate resistance (top of the frame).
    pub mm8_8: f64,
    /// 7/8 — "weak, stall and reverse" (overbought).
    pub mm7_8: f64,
    /// 6/8 — upper pivot / reversal line.
    pub mm6_8: f64,
    /// 5/8 — top of the normal trading range.
    pub mm5_8: f64,
    /// 4/8 — the major pivot (mean) line.
    pub mm4_8: f64,
    /// 3/8 — bottom of the normal trading range.
    pub mm3_8: f64,
    /// 2/8 — lower pivot / reversal line.
    pub mm2_8: f64,
    /// 1/8 — "weak, stall and reverse" (oversold).
    pub mm1_8: f64,
    /// 0/8 — ultimate support (bottom of the frame).
    pub mm0_8: f64,
}

/// Murrey Math Lines — T. H. Murrey's grid that divides the recent trading range
/// into eighths, each acting as support/resistance.
///
/// ```text
/// HH = highest high over `period`,  LL = lowest low over `period`
/// step = (HH − LL) / 8
/// mm{i}_8 = LL + i · step       for i = 0..8
/// ```
///
/// Murrey Math (a Gann-derived framework) holds that price gravitates to and
/// reverses at the eighth divisions of its range. The **4/8** line is the major
/// pivot (mean); **0/8** and **8/8** are the strongest support and resistance;
/// **3/8** and **5/8** bound the "normal" trading range, while **1/8**/**7/8** are
/// the weak "stall and reverse" lines. This implementation uses the price-derived
/// eighths over a rolling high-low frame (the practical core of the method) rather
/// than Murrey's full octave-quantised frame sizing, so the levels track the
/// instrument's actual recent range.
///
/// The first value lands after `period` inputs; each `update` rescans the frame in
/// O(`period`). A degenerate flat frame (`HH == LL`) collapses every line onto the
/// price.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, MurreyMathLines};
///
/// let mut indicator = MurreyMathLines::new(64).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     let base = 100.0 + (f64::from(i) * 0.3).sin() * 10.0;
///     let c = Candle::new(base, base + 1.0, base - 1.0, base, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct MurreyMathLines {
    period: usize,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
    last: Option<MurreyMathLinesOutput>,
}

impl MurreyMathLines {
    /// Construct Murrey Math Lines over a `period`-bar high-low frame.
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
            highs: VecDeque::with_capacity(period),
            lows: VecDeque::with_capacity(period),
            last: None,
        })
    }

    /// Configured frame period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<MurreyMathLinesOutput> {
        self.last
    }
}

impl Indicator for MurreyMathLines {
    type Input = Candle;
    type Output = MurreyMathLinesOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<MurreyMathLinesOutput> {
        if self.highs.len() == self.period {
            self.highs.pop_front();
            self.lows.pop_front();
        }
        self.highs.push_back(candle.high);
        self.lows.push_back(candle.low);
        if self.highs.len() < self.period {
            return None;
        }
        let hh = self.highs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let ll = self.lows.iter().copied().fold(f64::INFINITY, f64::min);
        let step = (hh - ll) / 8.0;
        let level = |i: f64| ll + i * step;
        let out = MurreyMathLinesOutput {
            mm0_8: level(0.0),
            mm1_8: level(1.0),
            mm2_8: level(2.0),
            mm3_8: level(3.0),
            mm4_8: level(4.0),
            mm5_8: level(5.0),
            mm6_8: level(6.0),
            mm7_8: level(7.0),
            mm8_8: level(8.0),
        };
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.highs.clear();
        self.lows.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "MurreyMathLines"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(high: f64, low: f64) -> Candle {
        Candle::new_unchecked(low, high, low, f64::midpoint(high, low), 1_000.0, 0)
    }

    #[test]
    fn rejects_zero_period() {
        assert!(matches!(MurreyMathLines::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let m = MurreyMathLines::new(64).unwrap();
        assert_eq!(m.period(), 64);
        assert_eq!(m.warmup_period(), 64);
        assert_eq!(m.name(), "MurreyMathLines");
        assert!(!m.is_ready());
        assert_eq!(m.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut m = MurreyMathLines::new(4).unwrap();
        let candles: Vec<Candle> = (0..6)
            .map(|i| c(101.0 + f64::from(i), 99.0 + f64::from(i)))
            .collect();
        let out = m.batch(&candles);
        for v in out.iter().take(3) {
            assert!(v.is_none());
        }
        assert!(out[3].is_some());
    }

    #[test]
    fn eighths_are_evenly_spaced() {
        // Frame [100, 180] over the window -> step = 10.
        let mut m = MurreyMathLines::new(2).unwrap();
        let out = m
            .batch(&[c(180.0, 100.0), c(180.0, 100.0)])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(out.mm0_8, 100.0, epsilon = 1e-9);
        assert_relative_eq!(out.mm4_8, 140.0, epsilon = 1e-9);
        assert_relative_eq!(out.mm8_8, 180.0, epsilon = 1e-9);
        assert_relative_eq!(out.mm1_8 - out.mm0_8, 10.0, epsilon = 1e-9);
    }

    #[test]
    fn levels_are_ordered() {
        let mut m = MurreyMathLines::new(10).unwrap();
        let candles: Vec<Candle> = (0..30)
            .map(|i| {
                c(
                    110.0 + (f64::from(i) * 0.3).sin() * 8.0,
                    90.0 + (f64::from(i) * 0.3).cos() * 8.0,
                )
            })
            .collect();
        for o in m.batch(&candles).into_iter().flatten() {
            assert!(o.mm0_8 <= o.mm4_8 && o.mm4_8 <= o.mm8_8);
            assert!(o.mm3_8 <= o.mm5_8);
        }
    }

    #[test]
    fn flat_frame_collapses() {
        let mut m = MurreyMathLines::new(3).unwrap();
        let out = m
            .batch(&[c(50.0, 50.0), c(50.0, 50.0), c(50.0, 50.0)])
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(out.mm0_8, 50.0, epsilon = 1e-12);
        assert_relative_eq!(out.mm8_8, 50.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut m = MurreyMathLines::new(4).unwrap();
        m.batch(
            &(0..6)
                .map(|i| c(101.0 + f64::from(i), 99.0 + f64::from(i)))
                .collect::<Vec<_>>(),
        );
        assert!(m.is_ready());
        m.reset();
        assert!(!m.is_ready());
        assert_eq!(m.value(), None);
        assert_eq!(m.update(c(101.0, 99.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..120)
            .map(|i| {
                c(
                    110.0 + (f64::from(i) * 0.25).sin() * 9.0,
                    90.0 + (f64::from(i) * 0.25).cos() * 9.0,
                )
            })
            .collect();
        let batch = MurreyMathLines::new(64).unwrap().batch(&candles);
        let mut b = MurreyMathLines::new(64).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            MurreyMathLines::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
