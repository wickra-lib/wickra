//! Vortex Indicator.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Vortex Indicator output: the two directional movement lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VortexOutput {
    /// `VI+` — strength of upward (positive) vortex movement.
    pub plus: f64,
    /// `VI−` — strength of downward (negative) vortex movement.
    pub minus: f64,
}

/// Vortex Indicator — Botes & Siepman's pair of oscillators (`VI+`, `VI−`) that
/// capture the relationship between two consecutive bars.
///
/// Two "vortex movements" measure how far price travelled against the opposite
/// extreme of the previous bar; each is normalised by the summed true range:
///
/// ```text
/// VM+_t = |high_t − low_{t−1}|
/// VM−_t = |low_t  − high_{t−1}|
/// VI+   = Σ VM+ over n / Σ TR over n
/// VI−   = Σ VM− over n / Σ TR over n
/// ```
///
/// `VI+` crossing above `VI−` is a bullish signal, the reverse a bearish one;
/// the wider the gap, the stronger the trend. A fully flat window (zero true
/// range) reports `(0, 0)`.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, Vortex};
///
/// let mut indicator = Vortex::new(14).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     let base = 100.0 + i as f64;
///     let candle =
///         Candle::new(base, base + 2.0, base - 2.0, base + 1.0, 10.0, i64::from(i)).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct Vortex {
    period: usize,
    prev: Option<Candle>,
    /// Rolling window of `(VM+, VM−, TR)` triples.
    window: VecDeque<(f64, f64, f64)>,
    sum_vm_plus: f64,
    sum_vm_minus: f64,
    sum_tr: f64,
    last: Option<VortexOutput>,
}

impl Vortex {
    /// Construct a new Vortex Indicator with the given period.
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
            prev: None,
            window: VecDeque::with_capacity(period),
            sum_vm_plus: 0.0,
            sum_vm_minus: 0.0,
            sum_tr: 0.0,
            last: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<VortexOutput> {
        self.last
    }
}

impl Indicator for Vortex {
    type Input = Candle;
    type Output = VortexOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<VortexOutput> {
        let Some(prev) = self.prev else {
            // The first bar has no predecessor to measure against.
            self.prev = Some(candle);
            return None;
        };
        let vm_plus = (candle.high - prev.low).abs();
        let vm_minus = (candle.low - prev.high).abs();
        let tr = candle.true_range(Some(prev.close));
        self.prev = Some(candle);

        if self.window.len() == self.period {
            let (old_p, old_m, old_tr) = self.window.pop_front().expect("window is non-empty");
            self.sum_vm_plus -= old_p;
            self.sum_vm_minus -= old_m;
            self.sum_tr -= old_tr;
        }
        self.window.push_back((vm_plus, vm_minus, tr));
        self.sum_vm_plus += vm_plus;
        self.sum_vm_minus += vm_minus;
        self.sum_tr += tr;

        if self.window.len() < self.period {
            return None;
        }
        let out = if self.sum_tr == 0.0 {
            // A perfectly flat window has no range to normalise against.
            VortexOutput {
                plus: 0.0,
                minus: 0.0,
            }
        } else {
            VortexOutput {
                plus: self.sum_vm_plus / self.sum_tr,
                minus: self.sum_vm_minus / self.sum_tr,
            }
        };
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.prev = None;
        self.window.clear();
        self.sum_vm_plus = 0.0;
        self.sum_vm_minus = 0.0;
        self.sum_tr = 0.0;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // The first VM/TR triple needs a previous bar, then the window fills.
        self.period + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "Vortex"
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
    fn new_rejects_zero_period() {
        assert!(matches!(Vortex::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `value` (84-91) and the
    /// Indicator-impl `name` body (157-159). `warmup_period` is covered
    /// elsewhere.
    #[test]
    fn accessors_and_metadata() {
        let mut v = Vortex::new(14).unwrap();
        assert_eq!(v.period(), 14);
        assert_eq!(v.name(), "Vortex");
        assert!(v.value().is_none());
        let warmup = i64::try_from(v.warmup_period()).unwrap();
        let candles: Vec<Candle> = (0..warmup)
            .map(|i| {
                let p = 100.0 + (i as f64 * 0.3).sin() * 5.0;
                Candle::new(p, p + 1.0, p - 1.0, p, 1.0, i).unwrap()
            })
            .collect();
        for c in &candles {
            v.update(*c);
        }
        assert!(v.value().is_some());
    }

    #[test]
    fn reference_values() {
        // Vortex(2) over three explicit candles (high, low, close):
        //   c1 = (10, 8, 9), c2 = (12, 9, 11), c3 = (13, 11, 12).
        // bar 2: VM+ = |12-8| = 4, VM- = |9-10| = 1, TR = 3.
        // bar 3: VM+ = |13-9| = 4, VM- = |11-12| = 1, TR = 2.
        // window sums: VM+ = 8, VM- = 2, TR = 5 -> VI+ = 1.6, VI- = 0.4.
        let candles = [
            candle(9.0, 10.0, 8.0, 9.0, 0),
            candle(10.0, 12.0, 9.0, 11.0, 1),
            candle(12.0, 13.0, 11.0, 12.0, 2),
        ];
        let mut v = Vortex::new(2).unwrap();
        let out = v.batch(&candles);
        assert_eq!(v.warmup_period(), 3);
        assert_eq!(out[0], None);
        assert_eq!(out[1], None);
        let o = out[2].unwrap();
        assert_relative_eq!(o.plus, 1.6, epsilon = 1e-12);
        assert_relative_eq!(o.minus, 0.4, epsilon = 1e-12);
    }

    #[test]
    fn perfectly_flat_market_yields_zero() {
        let mut v = Vortex::new(5).unwrap();
        let candles: Vec<Candle> = (0..20).map(|i| candle(10.0, 10.0, 10.0, 10.0, i)).collect();
        for o in v.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(o.plus, 0.0, epsilon = 1e-12);
            assert_relative_eq!(o.minus, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn outputs_are_non_negative() {
        let mut v = Vortex::new(14).unwrap();
        let candles: Vec<Candle> = (0..120)
            .map(|i| {
                let mid = 100.0 + (i as f64 * 0.3).sin() * 10.0;
                candle(mid, mid + 3.0, mid - 3.0, mid + 1.0, i)
            })
            .collect();
        for o in v.batch(&candles).into_iter().flatten() {
            assert!(o.plus >= 0.0 && o.minus >= 0.0, "negative VI: {o:?}");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut v = Vortex::new(5).unwrap();
        let candles: Vec<Candle> = (0..20)
            .map(|i| candle(100.0, 102.0, 98.0, 101.0, i))
            .collect();
        v.batch(&candles);
        assert!(v.is_ready());
        v.reset();
        assert!(!v.is_ready());
        assert_eq!(v.update(candles[0]), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..80)
            .map(|i| {
                let mid = 100.0 + (i as f64 * 0.35).sin() * 9.0;
                candle(mid, mid + 2.5, mid - 2.5, mid + 0.5, i)
            })
            .collect();
        let batch = Vortex::new(14).unwrap().batch(&candles);
        let mut b = Vortex::new(14).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            Vortex::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
