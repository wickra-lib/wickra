#![allow(clippy::doc_markdown)]

//! Tom DeMark TD D-Wave — a simplified Elliott-style swing-wave counter.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Tom DeMark **TD D-Wave** — a streaming wave counter that labels the market's
/// swing sequence with an Elliott-style `1–5` impulse / `A–C` correction count.
///
/// TD D-Wave is DeMark's objective alternative to discretionary Elliott Wave
/// counting. This streaming implementation detects alternating swing pivots with a
/// symmetric fractal of half-width `strength`, and advances a counter through the
/// eight-leg cycle each time a new swing leg is confirmed:
///
/// ```text
/// legs:  1 → 2 → 3 → 4 → 5 → A(6) → B(7) → C(8) → 1 …
/// output = current wave number, 1.0..8.0   (6/7/8 = corrective A/B/C)
/// ```
///
/// The number tells you which wave of the cycle price is currently working on — a
/// running map of impulse versus correction that updates as each swing confirms.
/// This is a **simplified** swing-leg count (it does not enforce Elliott's price
/// ratio and overlap rules); treat it as a structural guide, not a strict wave
/// label.
///
/// Readiness is data-dependent: the first value appears once the first swing pivot
/// confirms (`strength` bars after it forms). `warmup_period` returns the minimum
/// bars to confirm one pivot. Each `update` is O(`strength`).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, TdDWave};
///
/// let mut indicator = TdDWave::new(2).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     let base = 100.0 + (f64::from(i) * 0.5).sin() * 10.0;
///     let c = Candle::new(base, base + 1.0, base - 1.0, base, 1_000.0, 0).unwrap();
///     last = indicator.update(c);
/// }
/// let _ = last;
/// ```
#[derive(Debug, Clone)]
pub struct TdDWave {
    strength: usize,
    window: VecDeque<Candle>,
    last_is_high: Option<bool>,
    last_extreme: f64,
    wave: usize,
    last_value: Option<f64>,
}

impl TdDWave {
    /// Construct a TD D-Wave with the given fractal `strength`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `strength == 0`.
    pub fn new(strength: usize) -> Result<Self> {
        if strength == 0 {
            return Err(Error::PeriodZero);
        }
        if strength > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            strength,
            window: VecDeque::with_capacity(2 * strength + 1),
            last_is_high: None,
            last_extreme: 0.0,
            wave: 0,
            last_value: None,
        })
    }

    /// Configured fractal strength.
    pub const fn strength(&self) -> usize {
        self.strength
    }

    /// Current wave number if available.
    pub const fn value(&self) -> Option<f64> {
        self.last_value
    }

    fn advance(&mut self, is_high: bool, price: f64) {
        match self.last_is_high {
            Some(prev) if prev == is_high => {
                // Same-direction extreme: extend the current leg if more extreme.
                let extends = if is_high {
                    price > self.last_extreme
                } else {
                    price < self.last_extreme
                };
                if extends {
                    self.last_extreme = price;
                }
            }
            _ => {
                // A new alternating leg: advance the wave counter (1..8 cycle).
                self.wave = self.wave % 8 + 1;
                self.last_is_high = Some(is_high);
                self.last_extreme = price;
                self.last_value = Some(self.wave as f64);
            }
        }
    }
}

impl Indicator for TdDWave {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<f64> {
        let span = 2 * self.strength + 1;
        if self.window.len() == span {
            self.window.pop_front();
        }
        self.window.push_back(candle);
        if self.window.len() == span {
            let center = self.window[self.strength];
            let is_high = self
                .window
                .iter()
                .enumerate()
                .all(|(i, c)| i == self.strength || c.high < center.high);
            let is_low = self
                .window
                .iter()
                .enumerate()
                .all(|(i, c)| i == self.strength || c.low > center.low);
            if is_high && !is_low {
                self.advance(true, center.high);
            } else if is_low && !is_high {
                self.advance(false, center.low);
            }
        }
        self.last_value
    }

    fn reset(&mut self) {
        self.window.clear();
        self.last_is_high = None;
        self.last_extreme = 0.0;
        self.wave = 0;
        self.last_value = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        2 * self.strength + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last_value.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TDDWave"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;

    fn c(high: f64, low: f64) -> Candle {
        Candle::new_unchecked(
            f64::midpoint(high, low),
            high,
            low,
            f64::midpoint(high, low),
            1_000.0,
            0,
        )
    }

    fn zigzag() -> Vec<Candle> {
        (0..200)
            .map(|i| {
                let base = 100.0 + (f64::from(i) * 0.5).sin() * 10.0;
                c(base + 1.0, base - 1.0)
            })
            .collect()
    }

    #[test]
    fn rejects_zero_strength() {
        assert!(matches!(TdDWave::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let td = TdDWave::new(2).unwrap();
        assert_eq!(td.strength(), 2);
        assert_eq!(td.warmup_period(), 5);
        assert_eq!(td.name(), "TDDWave");
        assert!(!td.is_ready());
        assert_eq!(td.value(), None);
    }

    #[test]
    fn counts_waves_on_swings() {
        let mut td = TdDWave::new(2).unwrap();
        let out = td.batch(&zigzag());
        assert!(out.iter().any(Option::is_some));
        assert!(td.is_ready());
    }

    #[test]
    fn same_direction_pivots_extend_one_leg() {
        // Strictly decreasing lows mean no bar is ever a low pivot, so the
        // confirmed pivots are all highs. Consecutive same-direction highs
        // exercise the `extends` branch (true at 30 > 20, false at 25 < 30)
        // without ever advancing the wave past leg 1.
        let mut td = TdDWave::new(1).unwrap();
        let bars = [
            (10.0, 100.0),
            (20.0, 99.0),
            (12.0, 98.0),
            (30.0, 97.0),
            (15.0, 96.0),
            (25.0, 95.0),
            (14.0, 94.0),
            (14.0, 93.0),
        ];
        let vals: Vec<f64> = bars
            .iter()
            .filter_map(|&(high, low)| td.update(c(high, low)))
            .collect();
        assert_ne!(vals, Vec::new());
        assert!(vals.iter().all(|&v| v == 1.0));
    }

    #[test]
    fn same_direction_low_pivots_extend_one_leg() {
        // Mirror of the high-pivot case: strictly increasing highs mean no bar
        // is ever a high pivot, so the confirmed pivots are all lows. The
        // `extends` else-branch fires (true at 2 < 5, false at 4 > 2).
        let mut td = TdDWave::new(1).unwrap();
        let bars = [
            (100.0, 10.0),
            (101.0, 5.0),
            (102.0, 8.0),
            (103.0, 2.0),
            (104.0, 6.0),
            (105.0, 4.0),
            (106.0, 7.0),
            (107.0, 7.0),
        ];
        let vals: Vec<f64> = bars
            .iter()
            .filter_map(|&(high, low)| td.update(c(high, low)))
            .collect();
        assert_ne!(vals, Vec::new());
        assert!(vals.iter().all(|&v| v == 1.0));
    }

    #[test]
    fn wave_stays_in_one_to_eight() {
        let mut td = TdDWave::new(2).unwrap();
        for v in td.batch(&zigzag()).into_iter().flatten() {
            assert!((1.0..=8.0).contains(&v), "wave out of range: {v}");
        }
    }

    #[test]
    fn flat_input_never_counts() {
        // A perfectly flat series has no distinct swing highs/lows.
        let mut td = TdDWave::new(2).unwrap();
        let candles: Vec<Candle> = (0..40).map(|_| c(100.0, 100.0)).collect();
        assert!(td.batch(&candles).iter().all(Option::is_none));
    }

    #[test]
    fn reset_clears_state() {
        let mut td = TdDWave::new(2).unwrap();
        td.batch(&zigzag());
        assert!(td.is_ready());
        td.reset();
        assert!(!td.is_ready());
        assert_eq!(td.value(), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles = zigzag();
        let batch = TdDWave::new(2).unwrap().batch(&candles);
        let mut b = TdDWave::new(2).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }
}
