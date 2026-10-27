//! VPIN — Volume-Synchronised Probability of Informed Trading.

use std::collections::VecDeque;

use crate::indicators::rolling_moments::RollingSum;
use crate::microstructure::{Side, Trade};
use crate::traits::Indicator;
use crate::{Error, Result};

/// VPIN — the Volume-Synchronised Probability of Informed Trading
/// (Easley, López de Prado & O'Hara, 2012).
///
/// Trades are bucketed into equal-volume buckets of size `bucket_volume`. For
/// each completed bucket the order-flow imbalance is the absolute difference
/// between buy and sell volume; VPIN is that imbalance averaged over the last
/// `num_buckets` buckets and normalised by the bucket size:
///
/// ```text
/// VPIN = ( Σ |Vᴮ_τ − Vˢ_τ| ) / (num_buckets · bucket_volume)
/// ```
///
/// The aggressor [`Side`] of each [`Trade`] classifies its volume directly (no
/// bulk-volume classification needed). A single trade may span several buckets;
/// its volume is split across bucket boundaries. The result lies in `[0, 1]`:
/// values near `1` signal a strongly one-sided, likely-informed flow (a toxic
/// regime), values near `0` a balanced two-sided flow.
///
/// `Input = Trade`. Because bucket completion is driven by cumulative volume,
/// readiness is data-dependent; [`warmup_period`](Indicator::warmup_period)
/// reports `num_buckets` as the minimum number of trades (one per bucket) and
/// [`is_ready`](Indicator::is_ready) reflects the true bucket count.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, Side, Trade, Vpin};
///
/// let mut vpin = Vpin::new(10.0, 2).unwrap();
/// // Two buckets of pure buying => imbalance == bucket size => VPIN 1.
/// let mut last = None;
/// for _ in 0..4 {
///     last = vpin.update(Trade::new(100.0, 5.0, Side::Buy, 0).unwrap());
/// }
/// assert_eq!(last, Some(1.0));
/// ```
#[derive(Debug, Clone)]
pub struct Vpin {
    bucket_volume: f64,
    num_buckets: usize,
    cur_buy: f64,
    cur_sell: f64,
    cur_total: f64,
    window: VecDeque<f64>,
    sum_imbalance: RollingSum,
}

impl Vpin {
    /// Construct a new VPIN estimator.
    ///
    /// # Errors
    /// Returns [`Error::PeriodZero`] if `num_buckets == 0`, or
    /// [`Error::InvalidParameter`] if `bucket_volume` is not finite and
    /// positive.
    pub fn new(bucket_volume: f64, num_buckets: usize) -> Result<Self> {
        if num_buckets == 0 {
            return Err(Error::PeriodZero);
        }
        if num_buckets > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        if !bucket_volume.is_finite() || bucket_volume <= 0.0 {
            return Err(Error::InvalidParameter {
                message: "VPIN bucket_volume must be finite and positive",
            });
        }
        Ok(Self {
            bucket_volume,
            num_buckets,
            cur_buy: 0.0,
            cur_sell: 0.0,
            cur_total: 0.0,
            window: VecDeque::with_capacity(num_buckets),
            sum_imbalance: RollingSum::new(),
        })
    }

    /// Configured `(bucket_volume, num_buckets)`.
    pub const fn params(&self) -> (f64, usize) {
        (self.bucket_volume, self.num_buckets)
    }

    fn close_bucket(&mut self) {
        let imbalance = (self.cur_buy - self.cur_sell).abs();
        if self.window.len() == self.num_buckets {
            let old = self.window.pop_front().expect("window is non-empty");
            self.sum_imbalance.evict(old);
        }
        self.window.push_back(imbalance);
        self.sum_imbalance.push(imbalance);
        if self.sum_imbalance.needs_reseed(self.num_buckets) {
            self.sum_imbalance.reseed(self.window.iter().copied());
        }
        self.cur_buy = 0.0;
        self.cur_sell = 0.0;
        self.cur_total = 0.0;
    }
}

impl Indicator for Vpin {
    type Input = Trade;
    type Output = f64;

    #[inline]
    fn update(&mut self, trade: Trade) -> Option<f64> {
        let mut remaining = trade.size;
        let buy = trade.side == Side::Buy;

        // Bound the work at what can still be observed. A trade is one-sided,
        // so every complete bucket it fills carries an imbalance of exactly
        // `bucket_volume`, and the window keeps only the last `num_buckets`:
        // closing more than that pushes values identical to the ones it
        // evicts. Dropping them is exact rather than approximate, because the
        // remainder that decides where the next bucket boundary falls is kept.
        //
        // Without the bound the loop does not merely do useless work. Once the
        // size is large enough that `bucket_volume` falls below one ULP of it
        // -- 1.4e277 against a bucket of 8 -- `remaining -= take` leaves
        // `remaining` unchanged, so `while remaining > 0.0` never terminates.
        // A single malformed trade hung the caller forever. Found by
        // fuzz/fuzz_targets/indicator_update_trade.rs.
        let capacity = self.bucket_volume - self.cur_total;
        let window_full = self.num_buckets as f64 * self.bucket_volume;
        if remaining.is_infinite() {
            // Infinitely one-sided: it fills the window and leaves nothing over.
            remaining = capacity + window_full;
        } else if remaining > capacity {
            let beyond = remaining - capacity;
            if beyond / self.bucket_volume > self.num_buckets as f64 {
                remaining = capacity + window_full + beyond % self.bucket_volume;
            }
        }

        // Distribute the trade's volume across one or more buckets.
        while remaining > 0.0 {
            let capacity = self.bucket_volume - self.cur_total;
            let take = remaining.min(capacity);
            if buy {
                self.cur_buy += take;
            } else {
                self.cur_sell += take;
            }
            self.cur_total += take;
            remaining -= take;
            if self.cur_total >= self.bucket_volume {
                self.close_bucket();
            }
        }
        if self.window.len() < self.num_buckets {
            return None;
        }
        Some(self.sum_imbalance.value() / (self.num_buckets as f64 * self.bucket_volume))
    }

    fn reset(&mut self) {
        self.cur_buy = 0.0;
        self.cur_sell = 0.0;
        self.cur_total = 0.0;
        self.window.clear();
        self.sum_imbalance.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        // Buckets close on cumulative volume, so one large trade can fill
        // every bucket at once and no input count guarantees readiness. The
        // honest lower bound is one; `is_ready` is what callers should test.
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.window.len() == self.num_buckets
    }

    #[inline]
    fn name(&self) -> &'static str {
        "Vpin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn trade(size: f64, side: Side) -> Trade {
        Trade::new(100.0, size, side, 0).unwrap()
    }

    #[test]
    fn rejects_bad_params() {
        assert!(matches!(Vpin::new(10.0, 0), Err(Error::PeriodZero)));
        assert!(matches!(
            Vpin::new(0.0, 5),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            Vpin::new(f64::NAN, 5),
            Err(Error::InvalidParameter { .. })
        ));
    }

    #[test]
    fn accessors_and_metadata() {
        let vpin = Vpin::new(10.0, 50).unwrap();
        assert_eq!(vpin.params(), (10.0, 50));
        assert_eq!(vpin.warmup_period(), 1);
        assert_eq!(vpin.name(), "Vpin");
        assert!(!vpin.is_ready());
    }

    #[test]
    fn one_sided_flow_is_one() {
        // Every bucket is pure buying => |buy - sell| == bucket size => VPIN 1.
        let mut vpin = Vpin::new(10.0, 2).unwrap();
        let mut last = None;
        for _ in 0..4 {
            last = vpin.update(trade(5.0, Side::Buy));
        }
        assert_relative_eq!(last.unwrap(), 1.0, epsilon = 1e-12);
        assert!(vpin.is_ready());
    }

    #[test]
    fn balanced_flow_is_zero() {
        // Each bucket holds equal buy and sell volume => imbalance 0 => VPIN 0.
        let mut vpin = Vpin::new(10.0, 2).unwrap();
        let mut last = None;
        for _ in 0..4 {
            vpin.update(trade(5.0, Side::Buy));
            last = vpin.update(trade(5.0, Side::Sell));
        }
        assert_relative_eq!(last.unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn large_trade_spans_multiple_buckets() {
        // A single 25-unit buy fills 2 full buckets (size 10) plus 5 into a
        // third. Two buckets close => both pure buy => imbalance 10 each.
        let mut vpin = Vpin::new(10.0, 2).unwrap();
        let out = vpin.update(trade(25.0, Side::Buy));
        // After 2 closed buckets the window is full: VPIN = (10+10)/(2*10) = 1.
        assert_relative_eq!(out.unwrap(), 1.0, epsilon = 1e-12);
    }

    #[test]
    fn output_within_bounds() {
        let mut vpin = Vpin::new(7.0, 4).unwrap();
        for i in 0..200 {
            let side = if i % 3 == 0 { Side::Sell } else { Side::Buy };
            if let Some(v) = vpin.update(trade(1.0 + f64::from(i % 5), side)) {
                assert!((0.0..=1.0).contains(&v), "out of bounds: {v}");
            }
        }
    }

    #[test]
    fn zero_size_trade_is_noop() {
        let mut vpin = Vpin::new(10.0, 1).unwrap();
        assert_eq!(vpin.update(trade(0.0, Side::Buy)), None);
        // A full bucket of buying then closes it: VPIN 1.
        let out = vpin.update(trade(10.0, Side::Buy));
        assert_relative_eq!(out.unwrap(), 1.0, epsilon = 1e-12);
    }

    #[test]
    fn reset_clears_state() {
        let mut vpin = Vpin::new(10.0, 2).unwrap();
        for _ in 0..4 {
            vpin.update(trade(5.0, Side::Buy));
        }
        assert!(vpin.is_ready());
        vpin.reset();
        assert!(!vpin.is_ready());
        assert_eq!(vpin.update(trade(5.0, Side::Buy)), None);
    }

    #[test]
    fn batch_equals_streaming() {
        let trades: Vec<Trade> = (0..120)
            .map(|i| {
                let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
                trade(1.0 + f64::from(i % 4), side)
            })
            .collect();
        let batch = Vpin::new(8.0, 5).unwrap().batch(&trades);
        let mut b = Vpin::new(8.0, 5).unwrap();
        let streamed: Vec<_> = trades.iter().map(|t| b.update(*t)).collect();
        assert_eq!(batch, streamed);
    }

    // The exact input libFuzzer found: a size so large that subtracting a
    // bucket from it is below one ULP, so the distribution loop could never
    // make progress and `update` never returned. Any assertion here is
    // secondary to the test completing at all.
    #[test]
    fn enormous_size_terminates() {
        let mut vpin = Vpin::new(8.0, 5).unwrap();
        let value = vpin.update(trade(1.397_926_697_262_895_6e277, Side::Buy));
        assert_eq!(
            value,
            Some(1.0),
            "a one-sided flood is maximally imbalanced"
        );
    }

    // `Trade::new` rejects a non-finite size, so this one is only reachable
    // through `new_unchecked` -- which is what the fuzz harness uses, and what
    // a binding that has already validated upstream may use too. The finite
    // case above needs no such help: 1.4e277 passes the validating constructor
    // unchanged, so the hang was reachable through the ordinary API.
    #[test]
    fn infinite_size_terminates() {
        let mut vpin = Vpin::new(8.0, 5).unwrap();
        let flood = Trade::new_unchecked(100.0, f64::INFINITY, Side::Buy, 0);
        assert_eq!(vpin.update(flood), Some(1.0));
    }

    // Bounding the loop must not move the next bucket boundary. A size well
    // past the window still leaves a remainder, and the bucket after it has to
    // start where it would have without the bound.
    #[test]
    fn bounding_preserves_the_remainder() {
        let mut bounded = Vpin::new(8.0, 5).unwrap();
        bounded.update(trade(1000.5, Side::Buy));

        // The same volume delivered as many small trades, which never triggers
        // the bound, must leave the estimator in the same state.
        let mut unbounded = Vpin::new(8.0, 5).unwrap();
        for _ in 0..2001 {
            unbounded.update(trade(0.5, Side::Buy));
        }
        assert_eq!(bounded.cur_total, unbounded.cur_total);
        assert_eq!(
            bounded.update(trade(1.0, Side::Sell)),
            unbounded.update(trade(1.0, Side::Sell))
        );
    }

    #[test]
    fn a_size_below_the_bound_is_untouched() {
        // 6 buckets' worth against a 5-bucket window: right at the edge, and
        // the bound must not engage where the loop still terminates on its own.
        let mut vpin = Vpin::new(8.0, 5).unwrap();
        assert_eq!(vpin.update(trade(48.0, Side::Buy)), Some(1.0));
        assert_eq!(vpin.cur_total, 0.0);
    }

    #[test]
    fn rejects_num_buckets_above_maximum() {
        assert!(matches!(
            Vpin::new(10.0, crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
