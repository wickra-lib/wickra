//! Kyle's Lambda — rolling price impact per unit of signed order flow.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::indicators::rolling_moments::ShiftedPairMoments;
use crate::microstructure::TradeQuote;
use crate::traits::Indicator;

/// Kyle's Lambda — the rolling ordinary-least-squares slope of mid-price changes
/// on signed trade volume, the canonical measure of market depth / price
/// impact.
///
/// Each `update` receives a [`TradeQuote`] — a trade plus the mid prevailing at
/// execution. Internally the indicator forms, per trade, the mid change since
/// the previous trade (`Δmid = midₜ − midₜ₋₁`) and the signed volume
/// (`q = size · D`, with `D` the aggressor sign), then runs a rolling OLS
/// regression of `Δmid` on `q` over the trailing window of `window` trades:
///
/// ```text
/// cov = (1/n) · Σ q·Δmid − q̄·Δ̄mid
/// var = (1/n) · Σ q²      − q̄²
/// λ   = cov / var
/// ```
///
/// `λ` is the estimated price move per unit of signed volume: a deep, liquid
/// book absorbs flow with little movement and reads a small `λ`; a thin book
/// moves sharply per unit traded and reads a large `λ`. It is a direct,
/// model-light proxy for the slope of the demand curve in Kyle's microstructure
/// model.
///
/// Each `update` is O(1): four running sums (`Σq`, `ΣΔmid`, `Σq²`, `Σq·Δmid`)
/// are maintained as the window slides. A window of constant signed volume has
/// zero variance and `λ` is undefined; the indicator returns `0` in that case
/// rather than producing `NaN`.
///
/// `Input = TradeQuote`, `Output = f64`. It warms up for `window + 1`
/// trade-quotes: one to seed the previous mid, then `window` paired
/// observations.
///
/// # Example
///
/// ```
/// use wickra_core::{Indicator, KylesLambda, Side, Trade, TradeQuote};
///
/// // A book where each trade moves the mid by exactly 0.5 per unit of signed
/// // volume gives λ = 0.5.
/// let mut lambda = KylesLambda::new(8).unwrap();
/// let mut mid = 100.0;
/// let mut last = None;
/// for i in 0..20 {
///     let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
///     let size = 1.0 + f64::from(i % 3);
///     let signed = size * side.sign();
///     mid += 0.5 * signed;
///     let trade = Trade::new(mid, size, side, 0).unwrap();
///     last = lambda.update(TradeQuote::new(trade, mid).unwrap());
/// }
/// assert!((last.unwrap() - 0.5).abs() < 1e-9);
/// ```
#[derive(Debug, Clone)]
pub struct KylesLambda {
    window: usize,
    prev_mid: Option<f64>,
    pairs: VecDeque<(f64, f64)>,
    moments: ShiftedPairMoments,
}

impl KylesLambda {
    /// Construct a rolling Kyle's lambda over `window` paired observations.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPeriod`] if `window < 2` (the regression
    /// variance needs at least two observations).
    pub fn new(window: usize) -> Result<Self> {
        if window < 2 {
            return Err(Error::InvalidPeriod {
                message: "kyle's lambda needs window >= 2",
            });
        }
        if window > crate::error::MAX_PERIOD {
            return Err(Error::InvalidPeriod {
                message: crate::error::PERIOD_ABOVE_MAX,
            });
        }
        Ok(Self {
            window,
            prev_mid: None,
            pairs: VecDeque::with_capacity(window),
            moments: ShiftedPairMoments::new(),
        })
    }

    /// The configured window length, in paired observations.
    pub const fn window(&self) -> usize {
        self.window
    }

    fn push_pair(&mut self, signed_vol: f64, delta_mid: f64) -> Option<f64> {
        if self.pairs.len() == self.window {
            let (old_q, old_dm) = self.pairs.pop_front().expect("non-empty");
            self.moments.evict(old_q, old_dm);
        }
        self.pairs.push_back((signed_vol, delta_mid));
        self.moments.push(signed_vol, delta_mid);
        if self.moments.needs_reseed(self.window) {
            self.moments.reseed(self.pairs.iter().copied());
        }
        if self.pairs.len() < self.window {
            return None;
        }
        let var_q = self.moments.var_a(self.window);
        if var_q == 0.0 {
            // Constant signed-volume window has no defined slope.
            return Some(0.0);
        }
        Some(self.moments.cov(self.window) / var_q)
    }
}

impl Indicator for KylesLambda {
    type Input = TradeQuote;
    type Output = f64;

    #[inline]
    fn update(&mut self, quote: TradeQuote) -> Option<f64> {
        let mid = quote.mid;
        let signed_vol = quote.trade.size * quote.trade.side.sign();
        let Some(prev) = self.prev_mid else {
            self.prev_mid = Some(mid);
            return None;
        };
        self.prev_mid = Some(mid);
        self.push_pair(signed_vol, mid - prev)
    }

    fn reset(&mut self) {
        self.prev_mid = None;
        self.pairs.clear();
        self.moments.reset();
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        self.window + 1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.pairs.len() == self.window
    }

    #[inline]
    fn name(&self) -> &'static str {
        "KylesLambda"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::microstructure::{Side, Trade};
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    fn quotes_with_impact(n: usize, impact: f64) -> Vec<TradeQuote> {
        let mut mid = 100.0;
        (0..n)
            .map(|i| {
                let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
                let size = 1.0 + (i % 3) as f64;
                let signed = size * side.sign();
                mid += impact * signed;
                let trade = Trade::new(mid, size, side, 0).unwrap();
                TradeQuote::new(trade, mid).unwrap()
            })
            .collect()
    }

    #[test]
    fn rejects_window_below_two() {
        assert!(KylesLambda::new(0).is_err());
        assert!(KylesLambda::new(1).is_err());
        assert!(KylesLambda::new(2).is_ok());
    }

    #[test]
    fn accessors_and_metadata() {
        let kl = KylesLambda::new(14).unwrap();
        assert_eq!(kl.name(), "KylesLambda");
        assert_eq!(kl.window(), 14);
        assert_eq!(kl.warmup_period(), 15);
        assert!(!kl.is_ready());
    }

    #[test]
    fn recovers_constant_impact_slope() {
        // mid moves exactly 0.5 per unit signed volume -> lambda = 0.5.
        let last = KylesLambda::new(6)
            .unwrap()
            .batch(&quotes_with_impact(20, 0.5))
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.5, epsilon = 1e-9);
    }

    #[test]
    fn negative_impact_reads_negative() {
        let last = KylesLambda::new(6)
            .unwrap()
            .batch(&quotes_with_impact(20, -0.3))
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, -0.3, epsilon = 1e-9);
    }

    #[test]
    fn constant_signed_volume_is_zero() {
        // Every trade is a buy of size 1: signed volume is constant -> var 0 -> 0.
        let mut mid = 100.0;
        let quotes: Vec<TradeQuote> = (0..10)
            .map(|_| {
                mid += 0.01;
                let trade = Trade::new(mid, 1.0, Side::Buy, 0).unwrap();
                TradeQuote::new(trade, mid).unwrap()
            })
            .collect();
        let last = KylesLambda::new(5)
            .unwrap()
            .batch(&quotes)
            .into_iter()
            .flatten()
            .last()
            .unwrap();
        assert_relative_eq!(last, 0.0, epsilon = 1e-12);
    }

    #[test]
    fn warms_up_after_window_plus_one() {
        let mut kl = KylesLambda::new(3).unwrap();
        let quotes = quotes_with_impact(4, 0.2);
        assert_eq!(kl.update(quotes[0]), None); // seeds prev mid
        assert_eq!(kl.update(quotes[1]), None);
        assert_eq!(kl.update(quotes[2]), None);
        assert!(!kl.is_ready());
        assert!(kl.update(quotes[3]).is_some());
        assert!(kl.is_ready());
    }

    #[test]
    fn batch_equals_streaming() {
        let quotes = quotes_with_impact(40, 0.15);
        let batch = KylesLambda::new(10).unwrap().batch(&quotes);
        let mut kl = KylesLambda::new(10).unwrap();
        let streamed: Vec<_> = quotes.iter().map(|q| kl.update(*q)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn reset_clears_state() {
        let mut kl = KylesLambda::new(3).unwrap();
        for q in quotes_with_impact(6, 0.2) {
            kl.update(q);
        }
        assert!(kl.is_ready());
        kl.reset();
        assert!(!kl.is_ready());
        assert_eq!(kl.update(quotes_with_impact(1, 0.2)[0]), None);
    }

    /// Signed volume is only centred on zero while order flow is balanced. A
    /// persistent one-sided imbalance moves its mean far from zero, and the
    /// variance was then computed as `E[q^2] - E[q]^2` over running sums. At a
    /// trade size around 1e8 with a 99% buy imbalance that measured 2.9e-09
    /// against a two-pass reference; a centred accumulator gives 2.1e-12.
    #[test]
    fn one_sided_flow_at_size_matches_a_two_pass_reference() {
        const WINDOW: usize = 20;
        const BARS: i64 = 600;

        let mut ind = KylesLambda::new(WINDOW).unwrap();
        let (mut qs, mut deltas): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
        let mut prev_mid = 100.0;
        let mut compared = 0_usize;
        for i in 0..BARS {
            let t = i as f64;
            // Sells only where the sine dips below -0.99: a 99% buy imbalance.
            let side = if (t * 0.37).sin() > -0.99 {
                Side::Buy
            } else {
                Side::Sell
            };
            let size = 1e8 * (1.0 + 0.02 * (t * 0.19).cos());
            let mid = 100.0 + 0.5 * (t * 0.05).sin();
            let quote = TradeQuote::new(Trade::new(mid, size, side, i).unwrap(), mid).unwrap();
            let got = ind.update(quote);
            qs.push(if matches!(side, Side::Buy) {
                size
            } else {
                -size
            });
            deltas.push(mid - prev_mid);
            prev_mid = mid;
            let Some(lambda) = got else { continue };
            let k = qs.len();
            let (xs, ys) = (&qs[k - WINDOW..], &deltas[k - WINDOW..]);
            let n = WINDOW as f64;
            let mean_q = xs.iter().sum::<f64>() / n;
            let mean_d = ys.iter().sum::<f64>() / n;
            let var_q = xs.iter().map(|v| (v - mean_q) * (v - mean_q)).sum::<f64>() / n;
            let cov = xs
                .iter()
                .zip(ys)
                .map(|(u, v)| (u - mean_q) * (v - mean_d))
                .sum::<f64>()
                / n;
            compared += 1;
            assert_relative_eq!(lambda, cov / var_q, max_relative = 1e-10);
        }
        assert_eq!(compared, BARS as usize - ind.warmup_period() + 1);
    }

    #[test]
    fn rejects_window_above_maximum() {
        assert!(matches!(
            KylesLambda::new(crate::error::MAX_PERIOD + 1),
            Err(Error::InvalidPeriod { .. })
        ));
    }
}
