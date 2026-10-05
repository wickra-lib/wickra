//! Fibonacci Fan — trendlines fanning from a swing start through the
//! retracement levels at the swing end, extended to the current bar.

use crate::indicators::pattern_swing::{SwingTracker, SWING_THRESHOLD};
use crate::ohlcv::Candle;
use crate::traits::Indicator;

/// The three fan ratios drawn (38.2% / 50% / 61.8%).
const RATIOS: [f64; 3] = [0.382, 0.5, 0.618];

/// Fibonacci Fan line prices evaluated at the current bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FibFanOutput {
    /// Price of the 38.2% fan line at the current bar.
    pub fan_382: f64,
    /// Price of the 50% fan line at the current bar.
    pub fan_500: f64,
    /// Price of the 61.8% fan line at the current bar.
    pub fan_618: f64,
}

/// Fibonacci Fan (`FibFan`).
///
/// Anchored at the start of the most recent confirmed swing leg, three lines fan
/// out through the 38.2% / 50% / 61.8% retracement levels located at the leg's
/// end bar, then extend to the current bar. The retracement is measured back from
/// the end of the leg (the same convention as [`FibRetracement`](crate::FibRetracement)),
/// so at the end bar the `r` line sits at `end − r·(end − start)`. Each line's
/// price is reported as the fan opens with elapsed time.
///
/// ```text
/// line(r) = start + (1 − r) * (end - start) * (cur - start_bar) / (end_bar - start_bar)
/// ```
///
/// Parameter-free; construction is infallible. Returns `None` until the first
/// leg is complete.
///
/// See `crates/wickra-core/src/indicators/fib_fan.rs`.
#[derive(Debug, Clone)]
pub struct FibFan {
    swing: SwingTracker,
}

impl FibFan {
    /// Construct a new Fibonacci Fan tracker.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            swing: SwingTracker::new(SWING_THRESHOLD, 2),
        }
    }

    fn fan(&self) -> Option<FibFanOutput> {
        let pivots = self.swing.pivots();
        let start = pivots.first()?;
        let end = pivots.get(1)?;
        // Consecutive pivots occur at strictly increasing bars, so the span is
        // always at least one bar — no division by zero.
        let span_bars = (end.bar - start.bar) as f64;
        let elapsed = (self.swing.current_bar() - start.bar) as f64;
        let progress = elapsed / span_bars;
        let line = |r: f64| start.price + (1.0 - r) * (end.price - start.price) * progress;
        Some(FibFanOutput {
            fan_382: line(RATIOS[0]),
            fan_500: line(RATIOS[1]),
            fan_618: line(RATIOS[2]),
        })
    }
}

impl Default for FibFan {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for FibFan {
    type Input = Candle;
    type Output = FibFanOutput;

    #[inline]
    fn update(&mut self, candle: Candle) -> Option<FibFanOutput> {
        self.swing.update(candle);
        self.fan()
    }

    fn reset(&mut self) {
        self.swing.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        2
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.swing.pivots().len() >= 2
    }

    #[inline]
    fn name(&self) -> &'static str {
        "FibFan"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn c(high: f64, low: f64, ts: i64) -> Candle {
        Candle::new(low, high, low, low, 1.0, ts).unwrap()
    }

    /// Drive a leg start=200 (bar 0) -> end=100 (bar 2), confirmed at bar 3, so
    /// the fan is first reported at bar 3 with `progress = 3 / 2 = 1.5`.
    fn down_leg() -> Vec<Candle> {
        vec![
            c(200.0, 199.0, 0), // bootstrap high @200 (bar 0)
            c(190.0, 160.0, 1), // confirm high @200, low candidate @160
            c(150.0, 100.0, 2), // extend low to 100 (bar 2)
            c(110.0, 105.0, 3), // confirm low @100 -> two pivots
        ]
    }

    #[test]
    fn accessors_and_metadata() {
        let indicator = FibFan::new();
        assert_eq!(indicator.name(), "FibFan");
        assert_eq!(indicator.warmup_period(), 2);
        assert!(!indicator.is_ready());
        assert!(!FibFan::default().is_ready());
    }

    #[test]
    fn no_output_before_two_pivots() {
        let mut indicator = FibFan::new();
        // Only the high confirms here; no end pivot yet.
        let outputs: Vec<_> = [c(200.0, 199.0, 0), c(190.0, 150.0, 1)]
            .into_iter()
            .map(|x| indicator.update(x))
            .collect();
        assert!(outputs.iter().all(Option::is_none));
        assert!(!indicator.is_ready());
    }

    #[test]
    fn fan_lines_open_with_elapsed_time() {
        let mut indicator = FibFan::new();
        let mut last = None;
        for candle in down_leg() {
            last = indicator.update(candle);
        }
        let v = last.unwrap();
        assert!(indicator.is_ready());
        // progress = (3 - 0) / (2 - 0) = 1.5; line(r) = 200 + (1 - r)*(-100)*1.5.
        assert_relative_eq!(v.fan_382, 200.0 - (1.0 - 0.382) * 150.0);
        assert_relative_eq!(v.fan_500, 125.0);
        assert_relative_eq!(v.fan_618, 200.0 - (1.0 - 0.618) * 150.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut indicator = FibFan::new();
        for candle in down_leg() {
            let _ = indicator.update(candle);
        }
        assert!(indicator.is_ready());
        indicator.reset();
        assert!(!indicator.is_ready());
        assert!(indicator.update(c(100.0, 99.5, 0)).is_none());
    }

    #[test]
    fn batch_equals_streaming() {
        let candles = down_leg();
        let mut a = FibFan::new();
        let mut b = FibFan::new();
        assert_eq!(
            a.batch(&candles),
            candles.iter().map(|x| b.update(*x)).collect::<Vec<_>>()
        );
    }

    /// The quickest possible two-pivot sequence, then an up leg that rolls the
    /// two-pivot window forward.
    fn fast_series() -> Vec<Candle> {
        vec![
            c(200.0, 199.0, 0), // bootstrap high @200 (bar 0)
            c(190.0, 150.0, 1), // 150 <= 200 * 0.95 -> confirm high 200 @0, track low 150 @1
            c(160.0, 155.0, 2), // 160 >= 150 * 1.05 -> confirm low 150 @1
            c(180.0, 170.0, 3), // extend candidate high to 180 @3
            c(175.0, 165.0, 4), // 165 <= 180 * 0.95 -> confirm high 180 @3 (cap 2 drops 200)
            c(170.0, 168.0, 5), // no new pivot
        ]
    }

    #[test]
    fn first_value_lands_on_the_earliest_possible_bar() {
        let out = FibFan::new().batch(&fast_series());
        // warmup_period() == 2, and nothing is emitted before index 2 - 1 = 1.
        assert!(out[..1].iter().all(Option::is_none));
        // Two pivots need a bootstrap bar plus two confirming bars, so even the
        // fastest series emits first at index 2 (bar index 1 is still None).
        assert!(out[1].is_none());
        assert!(out[2].is_some());
    }

    #[test]
    fn hand_computed_down_then_up_leg() {
        let mut indicator = FibFan::new();
        let out = indicator.batch(&fast_series());
        // Bar 2: start 200 @0, end 150 @1, progress = (2 - 0) / (1 - 0) = 2.
        // line(r) = 200 + (1 - r) * (-50) * 2 = 200 - 100 * (1 - r)
        // -> 138.2 / 150 / 161.8.
        let v2 = out[2].unwrap();
        assert_relative_eq!(v2.fan_382, 138.2, epsilon = 1e-9);
        assert_relative_eq!(v2.fan_500, 150.0, epsilon = 1e-9);
        assert_relative_eq!(v2.fan_618, 161.8, epsilon = 1e-9);
        // Bar 3: progress = 3 / 1 = 3 -> 200 - 150 * (1 - r) -> 107.3 / 125 / 142.7.
        let v3 = out[3].unwrap();
        assert_relative_eq!(v3.fan_382, 107.3, epsilon = 1e-9);
        assert_relative_eq!(v3.fan_500, 125.0, epsilon = 1e-9);
        assert_relative_eq!(v3.fan_618, 142.7, epsilon = 1e-9);
        // Bar 4: new leg start 150 @1, end 180 @3, progress = (4 - 1) / (3 - 1) = 1.5.
        // line(r) = 150 + (1 - r) * 30 * 1.5 = 150 + 45 * (1 - r) -> 177.81 / 172.5 / 167.19.
        let v4 = out[4].unwrap();
        assert_relative_eq!(v4.fan_382, 177.81, epsilon = 1e-9);
        assert_relative_eq!(v4.fan_500, 172.5, epsilon = 1e-9);
        assert_relative_eq!(v4.fan_618, 167.19, epsilon = 1e-9);
        // Bar 5: progress = 4 / 2 = 2 -> 150 + 60 * (1 - r) -> 187.08 / 180 / 172.92.
        let v5 = out[5].unwrap();
        assert_relative_eq!(v5.fan_382, 187.08, epsilon = 1e-9);
        assert_relative_eq!(v5.fan_500, 180.0, epsilon = 1e-9);
        assert_relative_eq!(v5.fan_618, 172.92, epsilon = 1e-9);
        // On an up leg the 38.2% line is the steepest (closest to the leg end).
        assert!(v5.fan_382 > v5.fan_500 && v5.fan_500 > v5.fan_618);
    }

    #[test]
    fn line_at_unit_progress_is_measured_back_from_leg_end() {
        // Evaluate the fan with the current bar on the leg end (progress == 1):
        // line(r) = end - r * (end - start). Feed the down leg through bar 2 only
        // via the tracker so the end pivot exists, then check the formula identity
        // on the reported bar-3 value scaled back to progress 1.
        let mut indicator = FibFan::new();
        let v = indicator.batch(&down_leg())[3].unwrap();
        // progress 1.5; (line - start) / 1.5 + start == end - r * (end - start).
        let at_end = |line: f64| (line - 200.0) / 1.5 + 200.0;
        assert_relative_eq!(at_end(v.fan_382), 100.0 + 0.382 * 100.0, epsilon = 1e-9);
        assert_relative_eq!(at_end(v.fan_500), 150.0, epsilon = 1e-9);
        assert_relative_eq!(at_end(v.fan_618), 100.0 + 0.618 * 100.0, epsilon = 1e-9);
    }

    #[test]
    fn reset_replays_identically_to_fresh_instance() {
        let candles = fast_series();
        let mut indicator = FibFan::new();
        let first = indicator.batch(&candles);
        indicator.reset();
        let second = indicator.batch(&candles);
        assert_eq!(first, second);
        assert_eq!(second, FibFan::default().batch(&candles));
    }

    #[test]
    fn batch_equals_streaming_over_multiple_legs() {
        let candles = fast_series();
        let mut streaming = FibFan::new();
        let streamed: Vec<_> = candles.iter().map(|x| streaming.update(*x)).collect();
        assert_eq!(FibFan::new().batch(&candles), streamed);
    }
}
