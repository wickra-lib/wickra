//! Volatility Cone — current realized volatility within its historical envelope.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedMoments;
use crate::indicators::sorted_window;
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// Output of [`VolatilityCone`]: the current realized volatility together with
/// the envelope (the "cone") it sits inside over the lookback window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolatilityConeOutput {
    /// Latest realized volatility (sample stddev of log returns over `window`).
    pub current: f64,
    /// Lowest realized volatility seen over the `lookback` window.
    pub min: f64,
    /// Median realized volatility over the `lookback` window.
    pub median: f64,
    /// Highest realized volatility seen over the `lookback` window.
    pub max: f64,
    /// Percentile rank of `current` within the lookback distribution, in
    /// `[0, 100]` — the share of stored volatilities `<= current`, times 100.
    pub percentile: f64,
}

/// Volatility Cone — the current realized volatility positioned within the
/// historical range ("cone") of realized volatilities over a lookback window.
///
/// ```text
/// r_t     = ln(close_t / close_{t−1})
/// vol_t   = stddev_sample(r over window)            (rolling realized volatility)
/// cone    = { min, median, max, percentile } of vol over the last `lookback`
/// ```
///
/// A volatility cone (Burghardt & Lane 1990) shows whether current volatility is
/// high or low *relative to its own history*, rather than as an absolute number.
/// This streaming form tracks one horizon: it maintains the rolling realized
/// volatility of log returns over `window`, then reports the latest reading
/// (`current`) alongside the `min`, `median`, `max` and percentile rank of that
/// volatility series over the trailing `lookback`. `current` always lies within
/// `[min, max]` because it is itself the newest member of the lookback set.
///
/// Only the candle's **close** is used (the log-return series); the high and low
/// are ignored. The volatility is per-period (sample stddev of log returns, not
/// annualised) — multiply by `√trading_periods` for an annual figure. Each
/// `update` is O(`lookback log lookback`) from sorting the envelope.
///
/// Non-positive closes are ignored (the log return would be undefined): the tick
/// is dropped, state is left untouched, and the last value is returned.
///
/// # Example
///
/// ```
/// use wickra_core::{Candle, Indicator, VolatilityCone};
///
/// let mut indicator = VolatilityCone::new(20, 60).unwrap();
/// let mut last = None;
/// for i in 0..120 {
///     let c = 100.0 + (f64::from(i) * 0.3).sin() * 5.0;
///     let candle = Candle::new(c, c + 1.0, c - 1.0, c, 1_000.0, 0).unwrap();
///     last = indicator.update(candle);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct VolatilityCone {
    window: usize,
    lookback: usize,
    prev_close: Option<f64>,
    /// Rolling window of log returns for the inner realized-volatility series.
    returns: VecDeque<f64>,
    ret_moments: ShiftedMoments,
    /// Rolling window of realized-volatility readings (the cone envelope).
    vols: VecDeque<f64>,
    /// The window's values in `total_cmp` order, kept sorted as it slides:
    /// bit for bit what sorting a copy of the window would give.
    scratch: Vec<f64>,
    last: Option<VolatilityConeOutput>,
}

impl VolatilityCone {
    /// Construct a new volatility-cone indicator.
    ///
    /// `window` is the realized-volatility estimation window; `lookback` is the
    /// number of volatility readings forming the historical cone.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if either argument is `0`, or
    /// [`Error::InvalidPeriod`] if `window < 2` (a sample stddev needs two
    /// returns) or `lookback < 2` (an envelope needs at least two readings).
    pub fn new(window: usize, lookback: usize) -> Result<Self> {
        if window == 0 || lookback == 0 {
            return Err(Error::PeriodZero);
        }
        if window < 2 || lookback < 2 {
            return Err(Error::InvalidPeriod {
                message: "volatility cone window and lookback must both be >= 2",
            });
        }
        Ok(Self {
            window,
            lookback,
            prev_close: None,
            returns: VecDeque::with_capacity(window),
            ret_moments: ShiftedMoments::new(),
            vols: VecDeque::with_capacity(lookback),
            scratch: Vec::with_capacity(lookback),
            last: None,
        })
    }

    /// Configured `(window, lookback)`.
    pub const fn windows(&self) -> (usize, usize) {
        (self.window, self.lookback)
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<VolatilityConeOutput> {
        self.last
    }
}

impl Indicator for VolatilityCone {
    type Input = Candle;
    type Output = VolatilityConeOutput;

    fn update(&mut self, candle: Candle) -> Option<VolatilityConeOutput> {
        let price = candle.close;
        // A log return is undefined for a non-positive close; skip the tick.
        if price <= 0.0 {
            return self.last;
        }
        let Some(prev) = self.prev_close else {
            self.prev_close = Some(price);
            return None;
        };
        self.prev_close = Some(price);
        // `prev` came from `self.prev_close`, gated by the guard above, so it is
        // positive — the log return is always well-defined.
        let r = (price / prev).ln();

        // Stage one: rolling sample volatility of log returns.
        if self.returns.len() == self.window {
            let old = self.returns.pop_front().expect("returns window non-empty");
            self.ret_moments.evict(old);
        }
        self.returns.push_back(r);
        self.ret_moments.push(r);
        if self.ret_moments.needs_reseed(self.window) {
            self.ret_moments.reseed(self.returns.iter().copied());
        }
        if self.returns.len() < self.window {
            return None;
        }
        let current = self.ret_moments.sample_variance(self.window).sqrt();

        // Stage two: maintain the lookback envelope of volatility readings.
        if self.vols.len() == self.lookback {
            let oldest = self.vols.pop_front().expect("window is full");
            sorted_window::remove(&mut self.scratch, oldest);
        }
        self.vols.push_back(current);
        sorted_window::insert(&mut self.scratch, current);
        if self.vols.len() < self.lookback {
            return None;
        }

        let min = self.scratch[0];
        let max = self.scratch[self.lookback - 1];
        let mid = self.lookback / 2;
        let median = if self.lookback % 2 == 1 {
            self.scratch[mid]
        } else {
            f64::midpoint(self.scratch[mid - 1], self.scratch[mid])
        };
        let count_le = self.vols.iter().filter(|&&v| v <= current).count();
        let percentile = count_le as f64 / self.lookback as f64 * 100.0;

        let out = VolatilityConeOutput {
            current,
            min,
            median,
            max,
            percentile,
        };
        self.last = Some(out);
        Some(out)
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.returns.clear();
        self.ret_moments.reset();
        self.vols.clear();
        self.scratch.clear();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // One previous close for the first return, `window` returns for the
        // first volatility, then `lookback` volatilities for the envelope.
        self.window + self.lookback
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "VolatilityCone"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Candle whose close drives the indicator (open = high = low = close here).
    fn close_candle(close: f64) -> Candle {
        Candle::new_unchecked(close, close, close, close, 1_000.0, 0)
    }

    #[test]
    fn rejects_zero_window() {
        assert!(matches!(VolatilityCone::new(0, 10), Err(Error::PeriodZero)));
        assert!(matches!(VolatilityCone::new(10, 0), Err(Error::PeriodZero)));
    }

    #[test]
    fn rejects_window_one() {
        assert!(matches!(
            VolatilityCone::new(1, 10),
            Err(Error::InvalidPeriod { .. })
        ));
        assert!(matches!(
            VolatilityCone::new(10, 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let vc = VolatilityCone::new(20, 60).unwrap();
        assert_eq!(vc.windows(), (20, 60));
        assert_eq!(vc.warmup_period(), 80);
        assert_eq!(vc.name(), "VolatilityCone");
        assert!(!vc.is_ready());
        assert_eq!(vc.value(), None);
    }

    #[test]
    fn first_emission_at_warmup_period() {
        let mut vc = VolatilityCone::new(2, 2).unwrap();
        let prices = [100.0, 110.0, 121.0, 100.0, 105.0, 99.0];
        let candles: Vec<Candle> = prices.iter().map(|p| close_candle(*p)).collect();
        let out = vc.batch(&candles);
        let warmup = vc.warmup_period(); // 4
        assert_eq!(warmup, 4);
        for v in out.iter().take(warmup - 1) {
            assert!(v.is_none());
        }
        assert!(out[warmup - 1].is_some());
    }

    #[test]
    fn known_value() {
        // window = 2 -> vol = |r_t − r_{t−1}| / √2; lookback = 2.
        // prices: r1 = r2 = ln(1.1), r3 = ln(100/121).
        let mut vc = VolatilityCone::new(2, 2).unwrap();
        let candles: Vec<Candle> = [100.0, 110.0, 121.0, 100.0]
            .iter()
            .map(|p| close_candle(*p))
            .collect();
        let out = vc.batch(&candles);
        let r2 = (121.0_f64 / 110.0).ln();
        let r3 = (100.0_f64 / 121.0).ln();
        let vol2 = (r2 - r3).abs() / 2.0_f64.sqrt();
        let o = out[3].unwrap();
        assert_relative_eq!(o.current, vol2, epsilon = 1e-9);
        assert_relative_eq!(o.min, 0.0, epsilon = 1e-9); // vol1 = 0 (r1 == r2)
        assert_relative_eq!(o.max, vol2, epsilon = 1e-9);
        assert_relative_eq!(o.median, vol2 / 2.0, epsilon = 1e-9);
        assert_relative_eq!(o.percentile, 100.0, epsilon = 1e-9);
    }

    #[test]
    fn odd_lookback_median_is_middle() {
        // lookback = 3 picks the middle of the sorted envelope.
        let mut vc = VolatilityCone::new(2, 3).unwrap();
        let candles: Vec<Candle> = [100.0, 101.0, 103.0, 100.0, 104.0, 99.0, 106.0]
            .iter()
            .map(|p| close_candle(*p))
            .collect();
        let out = vc.batch(&candles);
        let o = out.last().unwrap().unwrap();
        assert!(o.min <= o.median && o.median <= o.max);
    }

    #[test]
    fn envelope_brackets_current() {
        let mut vc = VolatilityCone::new(10, 30).unwrap();
        let candles: Vec<Candle> = (0..200)
            .map(|i| close_candle(100.0 + (f64::from(i) * 0.3).sin() * 12.0))
            .collect();
        for o in vc.batch(&candles).into_iter().flatten() {
            assert!(o.min <= o.current && o.current <= o.max);
            assert!(o.min <= o.median && o.median <= o.max);
            assert!(o.percentile > 0.0 && o.percentile <= 100.0);
        }
    }

    #[test]
    fn constant_series_yields_zero_cone() {
        let mut vc = VolatilityCone::new(5, 5).unwrap();
        let candles: Vec<Candle> = (0..40).map(|_| close_candle(100.0)).collect();
        for o in vc.batch(&candles).into_iter().flatten() {
            assert_relative_eq!(o.current, 0.0, epsilon = 1e-12);
            assert_relative_eq!(o.min, 0.0, epsilon = 1e-12);
            assert_relative_eq!(o.max, 0.0, epsilon = 1e-12);
            assert_relative_eq!(o.median, 0.0, epsilon = 1e-12);
            assert_relative_eq!(o.percentile, 100.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn skips_non_positive_close() {
        let mut vc = VolatilityCone::new(2, 2).unwrap();
        let candles: Vec<Candle> = [100.0, 110.0, 121.0, 100.0]
            .iter()
            .map(|p| close_candle(*p))
            .collect();
        let warmup = vc.batch(&candles);
        let baseline = warmup.last().copied().flatten().expect("warmed up");
        // A non-positive close is skipped and the previous value is returned.
        assert_eq!(vc.update(close_candle(0.0)), Some(baseline));
        // State untouched: a clone advanced by the same real tick agrees.
        let mut control = vc.clone();
        let after = vc.update(close_candle(105.0)).expect("ready");
        assert_eq!(control.update(close_candle(105.0)).expect("ready"), after);
    }

    #[test]
    fn skips_non_positive_before_first_close() {
        let mut vc = VolatilityCone::new(2, 2).unwrap();
        assert_eq!(vc.update(close_candle(0.0)), None);
        assert_eq!(vc.update(close_candle(100.0)), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut vc = VolatilityCone::new(2, 2).unwrap();
        let candles: Vec<Candle> = [100.0, 110.0, 121.0, 100.0, 105.0]
            .iter()
            .map(|p| close_candle(*p))
            .collect();
        vc.batch(&candles);
        assert!(vc.is_ready());
        vc.reset();
        assert!(!vc.is_ready());
        assert_eq!(vc.value(), None);
        assert_eq!(vc.update(close_candle(100.0)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let candles: Vec<Candle> = (0..200)
            .map(|i| close_candle(100.0 + (f64::from(i) * 0.25).sin() * 9.0))
            .collect();
        let batch = VolatilityCone::new(10, 30).unwrap().batch(&candles);
        let mut b = VolatilityCone::new(10, 30).unwrap();
        let streamed: Vec<_> = candles.iter().map(|c| b.update(*c)).collect();
        assert_eq!(batch, streamed);
    }
}
