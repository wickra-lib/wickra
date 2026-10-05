//! Ehlers' Adaptive Laguerre Filter.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::traits::Indicator;

/// John Ehlers' Adaptive Laguerre Filter — a four-stage Laguerre polynomial
/// smoother whose smoothing factor `alpha` is recomputed every bar from how well
/// the filter is currently tracking price.
///
/// The Laguerre cascade is the same one used by [`LaguerreRsi`](crate::LaguerreRsi)
/// (with `gamma = 1 − alpha`), but instead of a fixed factor the filter adapts:
/// it measures its tracking error `|price − filter|`, normalises the newest error
/// against the range of the last `period` errors, and takes the **median of the
/// last five** normalised errors as `alpha`. A large tracking error relative to
/// the recent range gives a large `alpha` — the filter speeds up to catch price;
/// small errors give a small `alpha` and heavy smoothing.
///
/// ```text
/// diff_t  = |price_t − filter_{t-1}|
/// HH_t, LL_t = max / min of the last `period` diffs
/// mid_t   = (diff_t − LL_t) / (HH_t − LL_t)          (only when HH_t ≠ LL_t)
/// alpha_t = median(mid over the last 5 bars)          (held when HH_t = LL_t)
/// L0_t = alpha·price_t + (1 − alpha)·L0_{t-1}
/// L1_t = −(1 − alpha)·L0_t + L0_{t-1} + (1 − alpha)·L1_{t-1}
/// L2_t = −(1 − alpha)·L1_t + L1_{t-1} + (1 − alpha)·L2_{t-1}
/// L3_t = −(1 − alpha)·L2_t + L2_{t-1} + (1 − alpha)·L3_{t-1}
/// filter_t = (L0_t + 2·L1_t + 2·L2_t + L3_t) / 6
/// ```
///
/// The four stages are seeded with the first price. Until a first `alpha` can
/// be measured it is `1`, which makes the cascade a plain `(1, 2, 2, 1) / 6`
/// weighting of the last four prices. The output is a
/// smoothed price on the same scale as the input. The first emission lands once
/// the error window holds `period` values.
///
/// Reference: John F. Ehlers, *"Adaptive Laguerre Filter"*, Technical Analysis
/// of Stocks & Commodities, 2007; *Cybernetic Analysis for Stocks and Futures*
/// (Wiley, 2004), ch. 14.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, AdaptiveLaguerreFilter};
///
/// let mut indicator = AdaptiveLaguerreFilter::new(13).unwrap();
/// let mut last = None;
/// for i in 0..80 {
///     last = indicator.update(100.0 + f64::from(i));
/// }
/// assert!(last.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct AdaptiveLaguerreFilter {
    period: usize,
    l0: f64,
    l1: f64,
    l2: f64,
    l3: f64,
    /// Current smoothing factor (`1` until the first one is measured).
    alpha: f64,
    /// Previous filter output, or `None` before the first bar.
    filter: Option<f64>,
    /// The last `period` absolute errors `|price − filter|`.
    diffs: VecDeque<f64>,
    /// The last (up to) five normalised errors, oldest first.
    mids: [f64; MEDIAN_LEN],
    /// How many entries of `mids` are filled (saturates at five).
    mid_count: usize,
}

/// Length of Ehlers' median over the normalised errors.
const MEDIAN_LEN: usize = 5;

impl AdaptiveLaguerreFilter {
    /// Construct a new adaptive Laguerre filter with the given error-window
    /// length.
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
            l0: 0.0,
            l1: 0.0,
            l2: 0.0,
            l3: 0.0,
            alpha: 1.0,
            filter: None,
            diffs: VecDeque::with_capacity(period),
            mids: [0.0; MEDIAN_LEN],
            mid_count: 0,
        })
    }

    /// Configured error-window length.
    pub const fn period(&self) -> usize {
        self.period
    }

    /// Current value if the error window is full.
    pub fn value(&self) -> Option<f64> {
        if self.diffs.len() == self.period {
            self.filter
        } else {
            None
        }
    }

    /// Update `alpha` from the newest error: normalise it against the window's
    /// range and take the median of the last five normalised errors. A flat
    /// window (`HH == LL`) leaves `alpha` unchanged, as in Ehlers' code.
    fn adapt_alpha(&mut self, diff: f64) {
        let mut hh = f64::MIN;
        let mut ll = f64::MAX;
        for &d in &self.diffs {
            if d > hh {
                hh = d;
            }
            if d < ll {
                ll = d;
            }
        }
        let range = hh - ll;
        if range <= 0.0 {
            return;
        }
        let mid = (diff - ll) / range;
        if self.mid_count < MEDIAN_LEN {
            self.mids[self.mid_count] = mid;
            self.mid_count += 1;
        } else {
            self.mids.copy_within(1.., 0);
            self.mids[MEDIAN_LEN - 1] = mid;
        }
        let mut sorted = self.mids;
        let filled = &mut sorted[..self.mid_count];
        // `total_cmp` never panics — under pathological (e.g. overflowing) fuzz
        // inputs a normalised error can be non-finite.
        filled.sort_unstable_by(f64::total_cmp);
        let half = filled.len() / 2;
        self.alpha = if filled.len() % 2 == 1 {
            filled[half]
        } else {
            f64::midpoint(filled[half - 1], filled[half])
        };
    }
}

impl Indicator for AdaptiveLaguerreFilter {
    type Input = f64;
    type Output = f64;

    #[inline]
    fn update(&mut self, price: f64) -> Option<f64> {
        if !price.is_finite() {
            return None;
        }
        // The first bar seeds the four Laguerre stages with the price, so the
        // cascade starts settled instead of climbing up from zero.
        if self.filter.is_none() {
            self.l0 = price;
            self.l1 = price;
            self.l2 = price;
            self.l3 = price;
        }
        // Absolute tracking error against the previous filter (0 on the first
        // bar, where there is no prior filter value).
        let diff = self.filter.map_or(0.0, |f| (price - f).abs());
        if self.diffs.len() == self.period {
            self.diffs.pop_front();
        }
        self.diffs.push_back(diff);
        self.adapt_alpha(diff);

        let alpha = self.alpha;
        let gamma = 1.0 - alpha;
        let l0 = alpha * price + gamma * self.l0;
        let l1 = -gamma * l0 + self.l0 + gamma * self.l1;
        let l2 = -gamma * l1 + self.l1 + gamma * self.l2;
        let l3 = -gamma * l2 + self.l2 + gamma * self.l3;
        self.l0 = l0;
        self.l1 = l1;
        self.l2 = l2;
        self.l3 = l3;

        let filter = (l0 + 2.0 * l1 + 2.0 * l2 + l3) / 6.0;
        self.filter = Some(filter);
        self.value()
    }

    fn reset(&mut self) {
        self.l0 = 0.0;
        self.l1 = 0.0;
        self.l2 = 0.0;
        self.l3 = 0.0;
        self.alpha = 1.0;
        self.filter = None;
        self.diffs.clear();
        self.mids = [0.0; MEDIAN_LEN];
        self.mid_count = 0;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.period
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.diffs.len() == self.period
    }

    #[inline]
    fn name(&self) -> &'static str {
        "AdaptiveLaguerre"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    /// Independent reference: replays Ehlers' recurrence from scratch.
    fn naive(prices: &[f64], period: usize) -> Vec<Option<f64>> {
        let (mut l0, mut l1, mut l2, mut l3) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
        let mut filter: Option<f64> = None;
        let mut diffs: Vec<f64> = Vec::new();
        let mut mids: Vec<f64> = Vec::new();
        let mut alpha = 1.0_f64;
        let mut out = Vec::with_capacity(prices.len());
        for &price in prices {
            if filter.is_none() {
                (l0, l1, l2, l3) = (price, price, price, price);
            }
            let diff = filter.map_or(0.0, |f: f64| (price - f).abs());
            diffs.push(diff);
            if diffs.len() > period {
                diffs.remove(0);
            }
            let hh = diffs.iter().copied().fold(f64::MIN, f64::max);
            let ll = diffs.iter().copied().fold(f64::MAX, f64::min);
            let range = hh - ll;
            if range > 0.0 {
                mids.push((diff - ll) / range);
                if mids.len() > 5 {
                    mids.remove(0);
                }
                let mut sorted = mids.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let mid = sorted.len() / 2;
                alpha = if sorted.len() % 2 == 1 {
                    sorted[mid]
                } else {
                    f64::midpoint(sorted[mid - 1], sorted[mid])
                };
            }
            let gamma = 1.0 - alpha;
            let n0 = alpha * price + gamma * l0;
            let n1 = -gamma * n0 + l0 + gamma * l1;
            let n2 = -gamma * n1 + l1 + gamma * l2;
            let n3 = -gamma * n2 + l2 + gamma * l3;
            l0 = n0;
            l1 = n1;
            l2 = n2;
            l3 = n3;
            let f = (n0 + 2.0 * n1 + 2.0 * n2 + n3) / 6.0;
            filter = Some(f);
            out.push(if diffs.len() == period { Some(f) } else { None });
        }
        out
    }

    #[test]
    fn new_rejects_zero_period() {
        assert!(matches!(
            AdaptiveLaguerreFilter::new(0),
            Err(Error::PeriodZero)
        ));
    }

    /// Cover the const accessor `period` and the Indicator-impl `warmup_period`
    /// + `name`.
    #[test]
    fn accessors_and_metadata() {
        let alf = AdaptiveLaguerreFilter::new(13).unwrap();
        assert_eq!(alf.period(), 13);
        assert_eq!(alf.warmup_period(), 13);
        assert_eq!(alf.name(), "AdaptiveLaguerre");
    }

    #[test]
    fn warmup_returns_none_until_window_full() {
        let mut alf = AdaptiveLaguerreFilter::new(3).unwrap();
        assert_eq!(alf.update(10.0), None);
        assert_eq!(alf.update(11.0), None);
        assert!(alf.update(12.0).is_some());
    }

    #[test]
    fn constant_series_converges_to_constant() {
        // Errors are all zero -> alpha stays 1 -> the 4-stage delay line fills
        // with the constant and the filter settles on it.
        let mut alf = AdaptiveLaguerreFilter::new(5).unwrap();
        let out = alf.batch(&[42.0_f64; 40]);
        let last = out.iter().rev().flatten().next().unwrap();
        assert_relative_eq!(*last, 42.0, epsilon = 1e-9);
    }

    #[test]
    fn converged_output_stays_within_price_range() {
        // The Laguerre stages are seeded with the first price, so the filter is
        // a convex blend of recent prices and must stay inside the data range.
        let prices: Vec<f64> = (0..120)
            .map(|i| 50.0 + (f64::from(i) * 0.4).sin() * 10.0)
            .collect();
        let lo = prices.iter().copied().fold(f64::MAX, f64::min);
        let hi = prices.iter().copied().fold(f64::MIN, f64::max);
        let period = 8;
        let mut alf = AdaptiveLaguerreFilter::new(period).unwrap();
        for (i, v) in alf.batch(&prices).into_iter().enumerate() {
            // Skip the cold-start transient (a few multiples of the window).
            if i < 4 * period {
                continue;
            }
            let v = v.expect("filter is ready well past warmup");
            assert!(v >= lo - 1e-6 && v <= hi + 1e-6, "filter out of range");
        }
    }

    #[test]
    fn matches_naive_recurrence() {
        let prices: Vec<f64> = (0..80)
            .map(|i| 100.0 + (f64::from(i) * 0.5).sin() * 8.0 + f64::from(i) * 0.1)
            .collect();
        let mut alf = AdaptiveLaguerreFilter::new(10).unwrap();
        let got = alf.batch(&prices);
        let want = naive(&prices, 10);
        for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
            assert_eq!(g.is_some(), w.is_some(), "readiness mismatch at {i}");
            if let (Some(a), Some(b)) = (g, w) {
                assert_relative_eq!(*a, *b, epsilon = 1e-9);
            }
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut alf = AdaptiveLaguerreFilter::new(5).unwrap();
        alf.batch(&(1..=40).map(f64::from).collect::<Vec<_>>());
        assert!(alf.is_ready());
        alf.reset();
        assert!(!alf.is_ready());
        assert_eq!(alf.update(1.0), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let prices: Vec<f64> = (1..=50).map(|i| f64::from(i) * 0.7).collect();
        let mut a = AdaptiveLaguerreFilter::new(7).unwrap();
        let mut b = AdaptiveLaguerreFilter::new(7).unwrap();
        assert_eq!(
            a.batch(&prices),
            prices.iter().map(|p| b.update(*p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut alf = AdaptiveLaguerreFilter::new(3).unwrap();
        alf.update(10.0);
        alf.update(11.0);
        alf.update(12.0).expect("ready after three inputs");
        assert_eq!(alf.update(f64::NAN), None);
        assert_eq!(alf.update(f64::INFINITY), None);
    }

    #[test]
    fn new_rejects_oversized_period() {
        let too_long = crate::error::MAX_PERIOD + 1;
        assert!(matches!(
            AdaptiveLaguerreFilter::new(too_long),
            Err(Error::InvalidPeriod { .. })
        ));
    }

    fn wavy(len: i32) -> Vec<f64> {
        (0..len)
            .map(|i| 100.0 + (f64::from(i) * 0.45).sin() * 7.0 + (f64::from(i) * 1.3).cos())
            .collect()
    }

    #[test]
    fn first_value_lands_exactly_at_warmup_index() {
        let prices = wavy(30);
        let mut alf = AdaptiveLaguerreFilter::new(6).unwrap();
        let warmup = alf.warmup_period();
        let out = alf.batch(&prices);
        assert!(out.iter().take(warmup - 1).all(Option::is_none));
        assert!(out.iter().skip(warmup - 1).all(Option::is_some));
        assert_eq!(alf.value(), *out.last().unwrap());
    }

    #[test]
    fn reset_replays_identically_to_fresh_instance() {
        let prices = wavy(80);
        let mut used = AdaptiveLaguerreFilter::new(9).unwrap();
        used.batch(&prices);
        used.reset();
        assert_eq!(used.value(), None);
        let replay = used.batch(&prices);
        assert_eq!(
            replay,
            AdaptiveLaguerreFilter::new(9).unwrap().batch(&prices)
        );
    }

    #[test]
    fn batch_nan_into_matches_streaming_bits() {
        let prices = wavy(90);
        let mut nan_out = vec![0.0; prices.len()];
        AdaptiveLaguerreFilter::new(8)
            .unwrap()
            .batch_nan_into(&prices, &mut nan_out);
        let mut streamer = AdaptiveLaguerreFilter::new(8).unwrap();
        let identical = prices
            .iter()
            .zip(&nan_out)
            .all(|(p, v)| streamer.update(*p).unwrap_or(f64::NAN).to_bits() == v.to_bits());
        assert!(identical);
    }

    /// Hand-computed reference, `period = 2`, prices `10, 12, 11`.
    /// Bar 0: stages seeded with `10`, `diff = 0`, window `[0]` is flat so
    ///   `alpha` stays at its initial `1`; filter `= 10`; window not full.
    /// Bar 1: `diff = |12 − 10| = 2`, window `[0, 2]`, `mid = (2 − 0) / 2 = 1`,
    ///   `alpha = median([1]) = 1`, `gamma = 0`: `L0 = 12`, `L1 = 10`, `L2 = 10`,
    ///   `L3 = 10`; filter `= (12 + 20 + 20 + 10) / 6 = 62 / 6`.
    /// Bar 2: `diff = |11 − 62/6| = 2/3`, window `[2, 2/3]`, `mid = 0`,
    ///   `alpha = median([1, 0]) = 0.5`, `gamma = 0.5`:
    ///   `L0 = 0.5·11 + 0.5·12 = 11.5`
    ///   `L1 = −0.5·11.5 + 12 + 0.5·10 = 11.25`
    ///   `L2 = −0.5·11.25 + 10 + 0.5·10 = 9.375`
    ///   `L3 = −0.5·9.375 + 10 + 0.5·10 = 10.3125`
    ///   filter `= (11.5 + 22.5 + 18.75 + 10.3125) / 6 = 63.0625 / 6`.
    #[test]
    fn reference_values_with_unit_then_adapted_alpha() {
        let mut alf = AdaptiveLaguerreFilter::new(2).unwrap();
        assert_eq!(alf.update(10.0), None);
        assert_relative_eq!(alf.alpha, 1.0, epsilon = 1e-15);
        assert_relative_eq!(alf.update(12.0).unwrap(), 62.0 / 6.0, epsilon = 1e-12);
        assert_relative_eq!(alf.alpha, 1.0, epsilon = 1e-15);
        assert_relative_eq!(alf.update(11.0).unwrap(), 63.0625 / 6.0, epsilon = 1e-12);
        assert_relative_eq!(alf.alpha, 0.5, epsilon = 1e-15);
    }

    #[test]
    fn unit_alpha_weights_last_four_prices_with_seed_padding() {
        // With alpha held at 1 (a flat diff window never adapts it) the cascade is
        // the `(1, 2, 2, 1) / 6` weighting of the last four prices, the seed price
        // filling in for missing history. A single bar: `(5 + 10 + 10 + 5) / 6 = 5`.
        let mut alf = AdaptiveLaguerreFilter::new(1).unwrap();
        assert_relative_eq!(alf.update(5.0).unwrap(), 5.0, epsilon = 1e-12);
        assert_relative_eq!(alf.alpha, 1.0, epsilon = 1e-15);
        // period 1: every window holds one diff, so HH == LL and alpha is held at 1.
        // Prices 5, 8: `L0 = 8`, `L1 = L2 = L3 = 5` -> `(8 + 10 + 10 + 5) / 6 = 33 / 6`.
        assert_relative_eq!(alf.update(8.0).unwrap(), 33.0 / 6.0, epsilon = 1e-12);
        // Then 2: `L0 = 2`, `L1 = 8`, `L2 = L3 = 5` -> `(2 + 16 + 10 + 5) / 6 = 33 / 6`.
        assert_relative_eq!(alf.update(2.0).unwrap(), 33.0 / 6.0, epsilon = 1e-12);
        assert_eq!(alf.mid_count, 0);
    }

    #[test]
    fn flat_diff_window_holds_a_previously_adapted_alpha() {
        let mut alf = AdaptiveLaguerreFilter::new(3).unwrap();
        alf.alpha = 0.3;
        alf.diffs.extend([1.5, 1.5, 1.5]);
        alf.adapt_alpha(1.5);
        assert_relative_eq!(alf.alpha, 0.3, epsilon = 1e-15);
        assert_eq!(alf.mid_count, 0);
    }

    #[test]
    fn median_uses_the_middle_of_five_once_saturated() {
        // Window `[0, 10]` so `mid = diff / 10`. Mids 0.9, 0.1, 0.5, 0.7, 0.3:
        // sorted 0.1, 0.3, 0.5, 0.7, 0.9 -> median 0.5. A sixth mid 0.2 drops the
        // oldest (0.9): sorted 0.1, 0.2, 0.3, 0.5, 0.7 -> median 0.3.
        let mut alf = AdaptiveLaguerreFilter::new(2).unwrap();
        alf.diffs.extend([0.0, 10.0]);
        for diff in [9.0, 1.0, 5.0, 7.0, 3.0] {
            alf.adapt_alpha(diff);
        }
        assert_relative_eq!(alf.alpha, 0.5, epsilon = 1e-15);
        alf.adapt_alpha(2.0);
        assert_relative_eq!(alf.alpha, 0.3, epsilon = 1e-15);
        assert_eq!(alf.mid_count, 5);
    }
}
