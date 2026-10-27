//! Maximum Drawdown over a rolling window.

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// Rolling Maximum Drawdown — the deepest peak-to-trough decline within the
/// trailing window.
///
/// The input is treated as an equity-curve sample (or any non-negative value
/// series). For each bar the indicator computes the largest fractional decline
/// from any prior peak inside the trailing `period`-bar window:
///
/// ```text
/// drawdown_t = (equity_t − peak_t) / peak_t        (a negative number)
/// MaxDrawdown = min(drawdown_t over window)        (most-negative value)
/// ```
///
/// `peak_t` is the running maximum from the oldest value in the window up to
/// `t`, and only a positive peak counts.
///
/// Output is the magnitude of the worst drawdown as a non-negative fraction
/// (`0.20` = 20 % drop from peak). A monotonically rising equity curve has a
/// max drawdown of `0`. Setting `period` greater than or equal to the number of
/// bars you will ever feed makes the metric effectively *cumulative* — the
/// indicator never forgets the global peak.
///
/// Each `update` is amortised O(1). Within the window the running peaks split
/// the values into segments, each a peak followed by the values below it, and a
/// segment's deepest drawdown is `(peak − low) / peak`. The window is held as
/// two halves: the older one is rebuilt from the newer one every `period`
/// updates, storing for each value the peak and worst drawdown from it to the
/// end of the half, and the newer one keeps its own running peaks with the low
/// of each segment. Because the subtraction and the division both round
/// monotonically, the result is bit for bit the one a rescan of the window
/// gives.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, MaxDrawdown};
///
/// let mut mdd = MaxDrawdown::new(10).unwrap();
/// // Equity peaks at 110 then drops to 88 — a 20% drawdown.
/// for v in [100.0, 110.0, 100.0, 95.0, 88.0, 90.0, 92.0, 95.0, 100.0, 105.0] {
///     mdd.update(v);
/// }
/// assert!((mdd.update(106.0).unwrap() - 0.20).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct MaxDrawdown {
    period: usize,
    /// Older half of the window, oldest value last so `pop` drops it.
    front: Vec<FrontEntry>,
    /// Newer half of the window, in arrival order.
    back: Vec<f64>,
    /// Running peaks of `back`, each with the low of its segment.
    records: Vec<Record>,
    /// First record above the front's peak; the records before it sit in the
    /// segment of that peak.
    cursor: usize,
    /// Worst drawdown among the closed records from `cursor` on (the last
    /// record's segment is still open).
    closed_worst: f64,
    /// Running peaks of the run walked so far while rebuilding `front`.
    scratch: Vec<Segment>,
    last: Option<f64>,
}

/// One value of the older half.
#[derive(Debug, Clone, Copy)]
struct FrontEntry {
    /// Highest value from this one to the newest of the older half.
    peak: f64,
    /// Max drawdown of the run from this value to the newest of the older half.
    worst: f64,
}

/// A running peak of the newer half.
#[derive(Debug, Clone, Copy)]
struct Record {
    peak: f64,
    /// Lowest value from the peak up to the next running peak.
    low: f64,
    /// Lowest value of the newer half up to the next running peak.
    low_so_far: f64,
}

/// A running peak of the run walked while rebuilding the older half.
#[derive(Debug, Clone, Copy)]
struct Segment {
    peak: f64,
    low: f64,
    /// Max drawdown of the run from this peak to the newest of the older half.
    worst: f64,
}

/// Drawdown of `low` from `peak`; a non-positive peak counts as none.
fn drawdown(peak: f64, low: f64) -> f64 {
    if peak > 0.0 {
        (peak - low) / peak
    } else {
        0.0
    }
}

impl MaxDrawdown {
    /// Construct a new rolling Max Drawdown.
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
            front: Vec::new(),
            back: Vec::new(),
            records: Vec::new(),
            cursor: 0,
            closed_worst: 0.0,
            scratch: Vec::new(),
            last: None,
        })
    }

    /// Configured rolling-window length.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }

    fn push_back(&mut self, value: f64) {
        self.back.push(value);
        if let Some(open) = self.records.last_mut().filter(|r| value <= r.peak) {
            open.low = open.low.min(value);
            open.low_so_far = open.low_so_far.min(value);
            return;
        }
        let mut low_so_far = value;
        if let Some(closed) = self.records.last() {
            low_so_far = low_so_far.min(closed.low_so_far);
            if self.cursor < self.records.len() {
                self.closed_worst = self.closed_worst.max(drawdown(closed.peak, closed.low));
            }
        }
        self.records.push(Record {
            peak: value,
            low: value,
            low_so_far,
        });
    }

    /// Move the newer half into the older one, walking it newest first.
    fn refill_front(&mut self) {
        self.scratch.clear();
        let mut peak = f64::NEG_INFINITY;
        for &value in self.back.iter().rev() {
            // Running peaks no higher than `value` fall into its segment.
            let mut low = value;
            while let Some(seg) = self.scratch.last().copied().filter(|s| s.peak <= value) {
                low = low.min(seg.low);
                self.scratch.pop();
            }
            let worst = self
                .scratch
                .last()
                .map_or(0.0, |s| s.worst)
                .max(drawdown(value, low));
            self.scratch.push(Segment {
                peak: value,
                low,
                worst,
            });
            peak = peak.max(value);
            self.front.push(FrontEntry { peak, worst });
        }
        self.back.clear();
        self.records.clear();
        self.cursor = 0;
        self.closed_worst = 0.0;
    }

    fn worst(&mut self) -> f64 {
        let (peak, front_worst) = self
            .front
            .last()
            .map_or((f64::NEG_INFINITY, 0.0), |e| (e.peak, e.worst));
        // The front's peak only falls between refills, so the cursor only
        // moves left, except past a new open record the peak still covers.
        while self.cursor < self.records.len() && self.records[self.cursor].peak <= peak {
            self.cursor += 1;
        }
        while self.cursor > 0 && self.records[self.cursor - 1].peak > peak {
            self.cursor -= 1;
            if self.cursor + 1 < self.records.len() {
                let r = self.records[self.cursor];
                self.closed_worst = self.closed_worst.max(drawdown(r.peak, r.low));
            }
        }
        let mut worst = front_worst.max(self.closed_worst);
        if self.cursor < self.records.len() {
            let open = self.records[self.records.len() - 1];
            worst = worst.max(drawdown(open.peak, open.low));
        }
        if self.cursor > 0 {
            // The values before the cursor still have the front's peak.
            worst = worst.max(drawdown(peak, self.records[self.cursor - 1].low_so_far));
        }
        worst
    }
}

impl Indicator for MaxDrawdown {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            return None;
        }
        self.push_back(input);
        if self.front.len() + self.back.len() > self.period {
            if self.front.is_empty() {
                self.refill_front();
            }
            self.front.pop();
        }
        if self.front.len() + self.back.len() < self.period {
            return None;
        }
        let worst = self.worst();
        self.last = Some(worst);
        Some(worst)
    }

    fn reset(&mut self) {
        self.front.clear();
        self.back.clear();
        self.records.clear();
        self.cursor = 0;
        self.closed_worst = 0.0;
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
        "MaxDrawdown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// The definition, rescanning every window.
    fn rescan(prices: &[f64], period: usize) -> Vec<Option<f64>> {
        (0..prices.len())
            .map(|i| {
                if i + 1 < period {
                    return None;
                }
                let mut peak = f64::NEG_INFINITY;
                let mut worst = 0.0_f64;
                for &v in &prices[i + 1 - period..=i] {
                    if v > peak {
                        peak = v;
                    }
                    if peak > 0.0 {
                        let dd = (peak - v) / peak;
                        if dd > worst {
                            worst = dd;
                        }
                    }
                }
                Some(worst)
            })
            .collect()
    }

    fn bits(out: &[Option<f64>]) -> Vec<Option<u64>> {
        out.iter().map(|v| v.map(f64::to_bits)).collect()
    }

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(MaxDrawdown::new(0), Err(Error::PeriodZero)));
    }

    #[test]
    fn accessors_and_metadata() {
        let mut mdd = MaxDrawdown::new(10).unwrap();
        assert_eq!(mdd.period(), 10);
        assert_eq!(mdd.name(), "MaxDrawdown");
        assert_eq!(mdd.value(), None);
        assert_eq!(mdd.warmup_period(), 10);
        for v in 1..=10 {
            mdd.update(f64::from(v));
        }
        assert!(mdd.value().is_some());
    }

    #[test]
    fn pure_uptrend_yields_zero() {
        let mut mdd = MaxDrawdown::new(5).unwrap();
        let out = mdd.batch(&(1..=20).map(f64::from).collect::<Vec<_>>());
        for v in out.into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn reference_drawdown() {
        // Window [100, 120, 90]: peak 120, trough 90 -> 25% drawdown.
        let mut mdd = MaxDrawdown::new(3).unwrap();
        let out = mdd.batch(&[100.0, 120.0, 90.0]);
        assert_eq!(out[0], None);
        assert_eq!(out[1], None);
        assert_relative_eq!(out[2].unwrap(), 0.25, epsilon = 1e-12);
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut mdd = MaxDrawdown::new(4).unwrap();
        let out = mdd.batch(&[50.0; 12]);
        for v in out.into_iter().flatten() {
            assert_relative_eq!(v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut mdd = MaxDrawdown::new(3).unwrap();
        mdd.batch(&[100.0, 90.0, 80.0]);
        let last = mdd.value();
        assert_eq!(mdd.update(f64::NAN), None);
        assert_eq!(mdd.update(f64::INFINITY), None);
        // The rejected input must not have disturbed the state.
        assert_eq!(mdd.value(), last);
    }

    #[test]
    fn reset_clears_state() {
        let mut mdd = MaxDrawdown::new(3).unwrap();
        mdd.batch(&[100.0, 90.0, 80.0]);
        assert!(mdd.is_ready());
        mdd.reset();
        assert!(!mdd.is_ready());
        assert_eq!(mdd.update(100.0), None);
    }

    #[test]
    fn reset_then_replay_matches_fresh() {
        let prices: Vec<f64> = (0..90)
            .map(|i| 100.0 + (f64::from(i) * 0.7).sin() * 15.0)
            .collect();
        let mut mdd = MaxDrawdown::new(8).unwrap();
        let first = mdd.batch(&prices);
        mdd.reset();
        assert_eq!(bits(&mdd.batch(&prices)), bits(&first));
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (0..60)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        let batch = MaxDrawdown::new(10).unwrap().batch(&prices);
        let mut s = MaxDrawdown::new(10).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| s.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn non_positive_peak_yields_zero() {
        // All-zero stream: peak is 0, division skipped, result stays 0.
        let mut mdd = MaxDrawdown::new(3).unwrap();
        let out = mdd.batch(&[0.0_f64; 6]);
        for v in out.into_iter().flatten() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn matches_rescan_on_long_series() {
        // Swings, a long slide (one segment spanning many refills), a negative
        // stretch and a flat run, at periods from 1 to longer than a swing.
        let mut prices: Vec<f64> = (0..1500)
            .map(|i| 100.0 + (f64::from(i) * 0.05).sin() * 40.0 + (f64::from(i) * 0.9).cos() * 3.0)
            .collect();
        prices.extend((0..400).map(|i| 500.0 - f64::from(i)));
        prices.extend((0..200).map(|i| -20.0 + (f64::from(i) * 0.4).sin() * 25.0));
        prices.extend([42.0; 50]);
        for period in [1, 2, 3, 7, 64, 250, 701] {
            let got = MaxDrawdown::new(period).unwrap().batch(&prices);
            assert_eq!(
                bits(&got),
                bits(&rescan(&prices, period)),
                "period {period}"
            );
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]
        #[test]
        fn matches_rescan_bit_for_bit(
            period in 1usize..24,
            // Whole numbers make ties and repeated peaks common.
            ticks in proptest::collection::vec(-30i32..90, 0..300),
            scale in proptest::sample::select(vec![1.0_f64, 0.37, 1e-3, 1e6]),
        ) {
            let prices: Vec<f64> = ticks.iter().map(|&t| f64::from(t) * scale).collect();
            let got = MaxDrawdown::new(period).unwrap().batch(&prices);
            proptest::prop_assert_eq!(bits(&got), bits(&rescan(&prices, period)));
        }
    }
}
