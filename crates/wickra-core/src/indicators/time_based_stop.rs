//! Time-Based Stop — a holding-period timer that fires after a fixed bar count.

use crate::error::{Error, Result};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Time-Based Stop — exits a position purely on **elapsed bars**, independent of
/// price.
///
/// ```text
/// bars_held increments by 1 each bar (since the last reset)
/// progress  = min(bars_held / max_bars, 1.0)     in [0, 1]
/// stop fires when progress == 1.0  (bars_held >= max_bars)
/// ```
///
/// Some setups should not be given unlimited time to work: a mean-reversion entry
/// that has not reverted within `max_bars`, or an event trade whose catalyst has
/// passed, is best closed regardless of price. This indicator is a pure timer —
/// it ignores the candle's prices entirely and reports the fraction of the
/// holding window that has elapsed, reaching `1.0` (the stop) after `max_bars`
/// bars. **Call [`reset`](Indicator::reset) on each new entry** so the timer
/// restarts from the position open.
///
/// Each `update` is O(1) and the first bar already emits a value
/// (`1 / max_bars`).
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, TimeBasedStop};
///
/// let mut indicator = TimeBasedStop::new(5).unwrap();
/// let c = Candle::new(100.0, 101.0, 99.0, 100.0, 1.0, 0).unwrap();
/// // Five bars reach the stop.
/// let mut last = 0.0;
/// for _ in 0..5 {
///     last = indicator.update(c).unwrap();
/// }
/// assert_eq!(last, 1.0);
/// ```
#[derive(Debug, Clone)]
pub struct TimeBasedStop {
    max_bars: usize,
    bars_held: usize,
    last: Option<f64>,
}

impl TimeBasedStop {
    /// Construct a time-based stop that fires after `max_bars` bars.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PeriodZero`] if `max_bars == 0`.
    pub fn new(max_bars: usize) -> Result<Self> {
        if max_bars == 0 {
            return Err(Error::PeriodZero);
        }
        if max_bars > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            max_bars,
            bars_held: 0,
            last: None,
        })
    }

    /// Configured maximum holding period in bars.
    pub const fn max_bars(&self) -> usize {
        self.max_bars
    }

    /// Number of bars held since the last reset.
    pub const fn bars_held(&self) -> usize {
        self.bars_held
    }

    /// Whether the stop has fired (the holding period has fully elapsed).
    pub const fn triggered(&self) -> bool {
        self.bars_held >= self.max_bars
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for TimeBasedStop {
    type Input = Candle;
    type Output = f64;

    #[inline]
    fn update(&mut self, _candle: Candle) -> Option<f64> {
        self.bars_held += 1;
        let progress = (self.bars_held as f64 / self.max_bars as f64).min(1.0);
        self.last = Some(progress);
        Some(progress)
    }

    fn reset(&mut self) {
        self.bars_held = 0;
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "TimeBasedStop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c() -> Candle {
        Candle::new_unchecked(100.0, 101.0, 99.0, 100.0, 1.0, 0)
    }

    #[test]
    fn rejects_zero_max_bars() {
        assert!(matches!(TimeBasedStop::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let t = TimeBasedStop::new(5).unwrap();
        assert_eq!(t.max_bars(), 5);
        assert_eq!(t.bars_held(), 0);
        assert!(!t.triggered());
        assert_eq!(t.warmup_period(), 1);
        assert_eq!(t.name(), "TimeBasedStop");
        assert!(!t.is_ready());
        assert_eq!(t.value(), None);
    }

    #[test]
    fn progress_climbs_to_one() {
        let mut t = TimeBasedStop::new(4).unwrap();
        let out = t.batch(&[c(), c(), c(), c()]);
        assert_relative_eq!(out[0].unwrap(), 0.25, epsilon = 1e-12);
        assert_relative_eq!(out[1].unwrap(), 0.50, epsilon = 1e-12);
        assert_relative_eq!(out[2].unwrap(), 0.75, epsilon = 1e-12);
        assert_relative_eq!(out[3].unwrap(), 1.00, epsilon = 1e-12);
    }

    #[test]
    fn triggers_after_max_bars() {
        let mut t = TimeBasedStop::new(3).unwrap();
        t.update(c());
        assert!(!t.triggered());
        t.update(c());
        assert!(!t.triggered());
        t.update(c());
        assert!(t.triggered());
    }

    #[test]
    fn progress_saturates_at_one() {
        // Beyond max_bars the progress stays clamped at 1.0.
        let mut t = TimeBasedStop::new(2).unwrap();
        let out = t.batch(&[c(), c(), c(), c()]);
        assert_relative_eq!(out[2].unwrap(), 1.0, epsilon = 1e-12);
        assert_relative_eq!(out[3].unwrap(), 1.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_restarts_timer() {
        let mut t = TimeBasedStop::new(3).unwrap();
        t.batch(&[c(), c(), c()]);
        assert!(t.triggered());
        t.reset();
        assert!(!t.is_ready());
        assert_eq!(t.bars_held(), 0);
        assert!(!t.triggered());
        assert_relative_eq!(t.update(c()).unwrap(), 1.0 / 3.0, epsilon = 1e-12);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles = [c(); 10];
        let batch = TimeBasedStop::new(4).unwrap().batch(&candles);
        let mut b = TimeBasedStop::new(4).unwrap();
        let streamed: Vec<_> = candles.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn rejects_max_bars_above_maximum() {
        assert!(matches!(
            TimeBasedStop::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
