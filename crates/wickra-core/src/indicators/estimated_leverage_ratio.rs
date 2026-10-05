//! Estimated Leverage Ratio — open interest per unit of exchange reserve.

use crate::traits::Indicator;

/// Estimated Leverage Ratio (ELR) — a derivatives exchange's open interest
/// divided by its coin reserve, `CryptoQuant`'s measure of how much leverage
/// traders use on average.
///
/// ```text
/// ELR = open_interest / exchange_reserve
/// ```
///
/// Each update takes one `(open_interest, exchange_reserve)` pair: the open
/// interest of the exchange's derivatives and the amount of the coin it holds
/// in reserve, in the same unit. A rising ELR means a given reserve backs more
/// outstanding contracts — traders are taking on more leverage, and the market
/// is more fragile to liquidation cascades; a falling ELR marks deleveraging.
///
/// The ratio is non-negative for non-negative inputs; a non-positive reserve
/// reports `0` rather than dividing by zero. It is stateless — each pair yields
/// one value (no warmup). Each `update` is O(1).
///
/// # Example
///
/// ```
/// use wickra_core::{EstimatedLeverageRatio, Indicator};
///
/// let mut indicator = EstimatedLeverageRatio::new();
/// // Open interest 25,000 coins against a 100,000-coin reserve.
/// let elr = indicator.update((25_000.0, 100_000.0)).unwrap();
/// assert!((elr - 0.25).abs() < 1e-12);
/// ```
#[derive(Debug, Clone, Default)]
pub struct EstimatedLeverageRatio {
    ready: bool,
}

impl EstimatedLeverageRatio {
    /// Construct a new Estimated Leverage Ratio. The indicator is parameter-free.
    #[must_use]
    pub const fn new() -> Self {
        Self { ready: false }
    }
}

impl Indicator for EstimatedLeverageRatio {
    type Input = (f64, f64);
    type Output = f64;

    #[inline]
    fn update(&mut self, input: (f64, f64)) -> Option<f64> {
        let (open_interest, reserve) = input;
        if !open_interest.is_finite() || !reserve.is_finite() {
            return None;
        }
        let elr = if reserve > 0.0 {
            open_interest / reserve
        } else {
            0.0
        };
        self.ready = true;
        Some(elr)
    }

    fn reset(&mut self) {
        self.ready = false;
    }

    #[inline]
    fn warmup_period(&self) -> usize {
        1
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.ready
    }

    #[inline]
    fn name(&self) -> &'static str {
        "EstimatedLeverageRatio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::BatchExt;
    use approx::assert_relative_eq;

    #[test]
    fn accessors_and_metadata() {
        let e = EstimatedLeverageRatio::new();
        assert_eq!(e.warmup_period(), 1);
        assert_eq!(e.name(), "EstimatedLeverageRatio");
        assert!(!e.is_ready());
    }

    #[test]
    fn ratio_reference_value() {
        let mut e = EstimatedLeverageRatio::new();
        // 1000 / 4000 = 0.25.
        assert_relative_eq!(e.update((1_000.0, 4_000.0)).unwrap(), 0.25, epsilon = 1e-12);
    }

    #[test]
    fn higher_oi_raises_ratio() {
        let mut e = EstimatedLeverageRatio::new();
        let low = e.update((1_000.0, 10_000.0)).unwrap();
        let high = e.update((3_000.0, 10_000.0)).unwrap();
        assert!(high > low);
    }

    #[test]
    fn zero_reserve_is_zero() {
        let mut e = EstimatedLeverageRatio::new();
        assert_relative_eq!(e.update((1_000.0, 0.0)).unwrap(), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn non_finite_input_returns_none() {
        let mut e = EstimatedLeverageRatio::new();
        assert_eq!(e.update((f64::NAN, 1.0)), None);
        assert_eq!(e.update((1.0, f64::INFINITY)), None);
        assert!(!e.is_ready());
    }

    #[test]
    fn ready_after_first_update() {
        let mut e = EstimatedLeverageRatio::new();
        assert!(!e.is_ready());
        e.update((1_000.0, 10_000.0));
        assert!(e.is_ready());
    }

    #[test]
    fn reset_clears_state() {
        let mut e = EstimatedLeverageRatio::new();
        e.update((1_000.0, 10_000.0));
        assert!(e.is_ready());
        e.reset();
        assert!(!e.is_ready());
    }

    #[test]
    fn batch_equals_streaming() {
        let pairs: Vec<(f64, f64)> = (0..40)
            .map(|i| (1_000.0 + f64::from(i) * 10.0, 10_000.0 - f64::from(i)))
            .collect();
        let batch = EstimatedLeverageRatio::new().batch(&pairs);
        let mut b = EstimatedLeverageRatio::new();
        let streamed: Vec<_> = pairs.iter().map(|x| b.update(*x)).collect();
        assert_eq!(batch, streamed);
    }

    #[test]
    fn warmup_first_value_at_index_zero() {
        let mut e = EstimatedLeverageRatio::new();
        // warmup_period() - 1 == 0: the first finite pair already emits.
        assert_eq!(e.update((50.0, 200.0)), Some(0.25));
    }

    #[test]
    fn hand_computed_series() {
        let mut e = EstimatedLeverageRatio::new();
        // 30,000 / 120,000 = 0.25; 45,000 / 90,000 = 0.5; 90,000 / 60,000 = 1.5.
        assert_eq!(e.update((30_000.0, 120_000.0)), Some(0.25));
        assert_eq!(e.update((45_000.0, 90_000.0)), Some(0.5));
        assert_eq!(e.update((90_000.0, 60_000.0)), Some(1.5));
        // Zero open interest against a positive reserve is a genuine 0.
        assert_eq!(e.update((0.0, 60_000.0)), Some(0.0));
    }

    #[test]
    fn negative_reserve_is_zero() {
        let mut e = EstimatedLeverageRatio::new();
        assert_eq!(
            e.update((1_000.0, -5.0)).unwrap().to_bits(),
            0.0f64.to_bits()
        );
        assert_eq!(
            e.update((1_000.0, -0.0)).unwrap().to_bits(),
            0.0f64.to_bits()
        );
        assert!(e.is_ready());
    }

    #[test]
    fn every_non_finite_combination_returns_none() {
        let mut e = EstimatedLeverageRatio::new();
        assert_eq!(e.update((f64::INFINITY, 1.0)), None);
        assert_eq!(e.update((f64::NEG_INFINITY, 1.0)), None);
        assert_eq!(e.update((1.0, f64::NAN)), None);
        assert_eq!(e.update((1.0, f64::NEG_INFINITY)), None);
        assert_eq!(e.update((f64::NAN, f64::NAN)), None);
        // A non-finite pair must not flip a ready indicator back, nor emit.
        e.update((1.0, 2.0));
        assert!(e.is_ready());
        assert_eq!(e.update((f64::NAN, 2.0)), None);
        assert!(e.is_ready());
    }

    #[test]
    fn reset_replays_identically_to_fresh_instance() {
        let pairs = [
            (1_000.0, 4_000.0),
            (f64::NAN, 1.0),
            (2_000.0, 0.0),
            (500.0, 250.0),
        ];
        let mut e = EstimatedLeverageRatio::new();
        let first = e.batch(&pairs);
        e.reset();
        let second = e.batch(&pairs);
        let fresh = EstimatedLeverageRatio::default().batch(&pairs);
        assert_eq!(first, second);
        assert_eq!(second, fresh);
        assert_eq!(first, vec![Some(0.25), None, Some(0.0), Some(2.0)]);
    }

    #[test]
    fn batch_nan_into_is_bit_identical_to_streaming() {
        let pairs: Vec<(f64, f64)> = (0..30)
            .map(|i| match i % 5 {
                0 => (f64::NAN, 10.0),
                1 => (100.0, 0.0),
                _ => (100.0 + f64::from(i) * 3.3, 7.0 + f64::from(i) * 0.7),
            })
            .collect();
        let mut out = vec![0.0; pairs.len()];
        EstimatedLeverageRatio::new().batch_nan_into(&pairs, &mut out);
        let mut s = EstimatedLeverageRatio::new();
        assert!(pairs
            .iter()
            .zip(&out)
            .all(|(p, o)| s.update(*p).unwrap_or(f64::NAN).to_bits() == o.to_bits()));
    }
}
