//! Ulcer Index.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::RollingSum;
use crate::traits::Indicator;

/// Ulcer Index — Peter Martin's downside-only volatility / risk measure.
///
/// Standard deviation punishes upside and downside moves equally; the Ulcer
/// Index measures only the **pain of drawdowns**. For each bar it computes the
/// percentage drop from the highest price of the trailing window, squares it,
/// and reports the root-mean-square over the window:
///
/// ```text
/// drawdown_t = 100 · (price_t − max(price, period)_t) / max(price, period)_t
/// UlcerIndex = √( mean( drawdown² over period ) )
/// ```
///
/// A pure up-trend never trades below its own running high, so its Ulcer Index
/// is `0`; the deeper and longer the drawdowns, the higher the reading. It is
/// the volatility measure of choice for risk-adjusted return ratios (the
/// "Martin ratio" / UPI).
///
/// Each `update` is amortised O(1): the trailing maximum is tracked with a
/// monotonically-decreasing deque of `(index, price)` pairs, so the indicator
/// honours the `Indicator` trait's O(1)-per-tick contract even for long
/// windows.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, UlcerIndex};
///
/// let mut indicator = UlcerIndex::new(14).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + (f64::from(i) * 0.3).sin() * 8.0);
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct UlcerIndex {
    period: usize,
    /// 1-based count of finite inputs seen so far; used as the monotonic index
    /// that expires entries from `max_dq`.
    count: u64,
    /// Monotonically-decreasing deque of `(index, price)` over the trailing
    /// `period` inputs. The front holds the current trailing maximum in O(1).
    max_dq: VecDeque<(u64, f64)>,
    /// Rolling window of the last `period` squared percentage drawdowns.
    drawdowns_sq: VecDeque<f64>,
    sum_sq: RollingSum,
    last: Option<f64>,
}

impl UlcerIndex {
    /// Construct a new Ulcer Index with the given period.
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
            count: 0,
            max_dq: VecDeque::with_capacity(period),
            drawdowns_sq: VecDeque::with_capacity(period),
            sum_sq: RollingSum::new(),
            last: None,
        })
    }

    /// Configured period.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if available.
    pub const fn value(&self) -> Option<f64> {
        self.last
    }
}

impl Indicator for UlcerIndex {
    type Input = f64;
    type Output = f64;

    fn update(&mut self, input: f64) -> Option<f64> {
        if !input.is_finite() {
            // Non-finite input is ignored; state is left untouched.
            return None;
        }
        self.count += 1;
        // Drop tail entries that can never be the trailing max again — every
        // entry `≤ input` is dominated by `input` and at least as old.
        while let Some(&(_, back)) = self.max_dq.back() {
            if back <= input {
                self.max_dq.pop_back();
            } else {
                break;
            }
        }
        self.max_dq.push_back((self.count, input));
        // Expire the head once it falls out of the trailing `period`-window.
        let window_lo = self.count.saturating_sub(self.period as u64 - 1);
        while let Some(&(idx, _)) = self.max_dq.front() {
            if idx < window_lo {
                self.max_dq.pop_front();
            } else {
                break;
            }
        }
        if self.count < self.period as u64 {
            return None;
        }
        // Front is the trailing max in O(1).
        let max_price = self.max_dq.front().expect("non-empty").1;
        let drawdown = if max_price == 0.0 {
            0.0
        } else {
            100.0 * (input - max_price) / max_price
        };
        let sq = drawdown * drawdown;

        if self.drawdowns_sq.len() == self.period {
            let oldest = self.drawdowns_sq.pop_front().expect("window is non-empty");
            self.sum_sq.evict(oldest);
        }
        self.drawdowns_sq.push_back(sq);
        self.sum_sq.push(sq);
        if self.sum_sq.needs_reseed(self.period) {
            self.sum_sq.reseed(self.drawdowns_sq.iter().copied());
        }
        if self.drawdowns_sq.len() < self.period {
            return None;
        }
        let ui = (self.sum_sq.value() / self.period as f64).sqrt();
        self.last = Some(ui);
        Some(ui)
    }

    fn reset(&mut self) {
        self.count = 0;
        self.max_dq.clear();
        self.drawdowns_sq.clear();
        self.sum_sq.reset();
        self.last = None;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // `period` inputs fill the trailing-max window; the first drawdown is
        // computable on bar `period` (the window is full for the first time);
        // another `period - 1` drawdowns then fill the RMS window. The two
        // windows overlap by one bar, so `warmup_period() == 2 * period - 1`.
        2 * self.period - 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.last.is_some()
    }

    #[inline]
    fn name(&self) -> &'static str {
        "UlcerIndex"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(UlcerIndex::new(0), Err(Error::PeriodZero)));
    }

    /// Cover the const accessors `period` / `value` (lines 77-85) and the
    /// Indicator-impl `name` body (162-164). `warmup_period` is covered
    /// already by `reference_values`.
    #[test]
    fn accessors_and_metadata() {
        let mut ui = UlcerIndex::new(14).unwrap();
        assert_eq!(ui.period(), 14);
        assert_eq!(ui.name(), "UlcerIndex");
        assert_eq!(ui.value(), None);
        // Drive past warmup so value() flips to Some.
        for i in 0..ui.warmup_period() {
            ui.update(100.0 + (i as f64).sin() * 5.0);
        }
        assert!(ui.value().is_some());
    }

    /// Cover the `max_price == 0.0` defensive branch (line 123). All
    /// other tests use prices > 0, so the trailing-max divisor is always
    /// positive. Feed a stream of zeros — the trailing max is exactly
    /// 0.0 and the drawdown computation would otherwise hit a 0/0 NaN.
    /// The indicator must emit exactly 0.0 (drawdown is 0% by convention).
    #[test]
    fn zero_max_price_yields_zero_drawdown() {
        let mut ui = UlcerIndex::new(3).unwrap();
        let out = ui.batch(&[0.0_f64; 10]);
        let last = out.into_iter().flatten().last().expect("emits");
        assert_eq!(last, 0.0);
    }

    #[test]
    fn reference_values() {
        // UlcerIndex(2): warmup = 3.
        // [10, 8, 12, 9]:
        //   bar 3: window [8,12], max 12, drawdown 0; sq window [400, 0]
        //          -> UI = sqrt(200).
        //   bar 4: window [12,9], max 12, drawdown -25, sq 625; sq window [0, 625]
        //          -> UI = sqrt(312.5).
        let mut ui = UlcerIndex::new(2).unwrap();
        let out = ui.batch(&[10.0, 8.0, 12.0, 9.0]);
        assert_eq!(ui.warmup_period(), 3);
        assert_eq!(out[0], None);
        assert_eq!(out[1], None);
        assert_relative_eq!(out[2].unwrap(), 200.0_f64.sqrt(), epsilon = 1e-12);
        assert_relative_eq!(out[3].unwrap(), 312.5_f64.sqrt(), epsilon = 1e-12);
    }

    #[test]
    fn pure_uptrend_yields_zero() {
        // Price never trades below its own running high: no drawdown at all.
        let mut ui = UlcerIndex::new(5).unwrap();
        let out = ui.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        for v in out.iter().skip(ui.warmup_period() - 1).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn constant_series_yields_zero() {
        let mut ui = UlcerIndex::new(5).unwrap();
        let out = ui.batch(&[50.0; 30]);
        for v in out.iter().skip(ui.warmup_period() - 1).flatten() {
            assert_relative_eq!(*v, 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn output_is_non_negative() {
        let mut ui = UlcerIndex::new(14).unwrap();
        let prices: Vec<f64> = (1..=120)
            .map(|i| 100.0 + (f64::from(i) * 0.25).sin() * 15.0)
            .collect();
        for v in ui.batch(&prices).into_iter().flatten() {
            assert!(v >= 0.0, "Ulcer Index must be non-negative, got {v}");
        }
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut ui = UlcerIndex::new(2).unwrap();
        let out = ui.batch(&[10.0, 8.0, 12.0, 9.0]);
        let last = *out.last().unwrap();
        assert!(last.is_some());
        assert_eq!(ui.update(f64::NAN), None);
        assert_eq!(ui.update(f64::INFINITY), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut ui = UlcerIndex::new(3).unwrap();
        ui.batch(&[10.0, 8.0, 12.0, 9.0, 11.0, 7.0]);
        assert!(ui.is_ready());
        ui.reset();
        assert!(!ui.is_ready());
        assert_eq!(ui.update(10.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=80)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 10.0)
            .collect();
        let batch = UlcerIndex::new(14).unwrap().batch(&prices);
        let mut b = UlcerIndex::new(14).unwrap();
        let streamed: Vec<_> = prices.iter().map(|p| b.update(*p)).collect();
        assert_eq!(batch, streamed);
    }

    /// Monotone-deque equivalence: the O(1) implementation must produce exactly
    /// the same per-tick values as a naive O(n) trailing-max scan, on inputs
    /// chosen to exercise every deque-maintenance path:
    /// strictly increasing (everything is dominated and gets popped),
    /// strictly decreasing (nothing is popped, head expires when the window
    /// slides),
    /// and constants (ties — the `<= input` pop rule keeps a single newest
    /// entry).
    #[test]
    fn monotone_deque_matches_naive_max_on_adversarial_inputs() {
        fn naive_max(prices: &[f64], period: usize, t: usize) -> f64 {
            let lo = t + 1 - period;
            prices[lo..=t]
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max)
        }

        fn check(prices: &[f64], period: usize) {
            let mut ui = UlcerIndex::new(period).unwrap();
            for (i, p) in prices.iter().enumerate() {
                let _ = ui.update(*p);
                if i + 1 >= period {
                    let trailing_max = ui.max_dq.front().expect("non-empty").1;
                    let naive = naive_max(prices, period, i);
                    assert!(
                        (trailing_max - naive).abs() < 1e-12,
                        "trailing max diverges at t={i}: deque={trailing_max}, naive={naive}",
                    );
                }
            }
        }

        // Strictly increasing — every push pops the entire deque tail.
        let increasing: Vec<f64> = (1..=50).map(f64::from).collect();
        check(&increasing, 5);
        check(&increasing, 14);

        // Strictly decreasing — pushes never pop the tail; the head expires.
        let decreasing: Vec<f64> = (1..=50).rev().map(f64::from).collect();
        check(&decreasing, 5);
        check(&decreasing, 14);

        // All-equal — `back <= input` pops on equality, leaving a length-1
        // deque containing only the most recent index.
        let constant = vec![42.0; 50];
        check(&constant, 5);
        check(&constant, 14);

        // Mixed sawtooth — exercises every code path.
        let mixed: Vec<f64> = (0..120)
            .map(|i| 100.0 + (f64::from(i) * 0.7).sin() * 20.0)
            .collect();
        check(&mixed, 7);
        check(&mixed, 30);
    }

    #[test]
    fn rejects_period_above_maximum() {
        assert!(matches!(
            UlcerIndex::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
