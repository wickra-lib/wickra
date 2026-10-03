//! The unit phasor the Hilbert-transform indicators integrate the smoothed
//! price against to recover the dominant-cycle phase.

use std::f64::consts::PI;
use std::sync::OnceLock;

/// The longest dominant-cycle window integrated over: the smoothed-price
/// history holds this many values, and the window is clamped to it.
pub(crate) const MAX_DC_PERIOD: usize = 50;

/// One window length's `(sin, cos)` terms.
type Terms = Box<[(f64, f64)]>;

/// `(sin, cos)` of `i · 2π / dc_period` for `i` in `0..dc_period`.
///
/// The terms depend on nothing but the two integers, so every window length's
/// are computed once, by the expression the integration used in place, and
/// shared: the same bits, without a sine and a cosine per bar per update.
pub(crate) fn phasor(dc_period: usize) -> &'static [(f64, f64)] {
    static TABLE: OnceLock<Vec<Terms>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        (0..=MAX_DC_PERIOD)
            .map(|period| {
                (0..period)
                    .map(|i| {
                        let angle = (i as f64) * 2.0 * PI / (period as f64);
                        (angle.sin(), angle.cos())
                    })
                    .collect()
            })
            .collect()
    });
    &table[dc_period]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_are_the_in_place_expression() {
        for period in 1..=MAX_DC_PERIOD {
            let terms = phasor(period);
            assert_eq!(terms.len(), period);
            for (i, &(sin, cos)) in terms.iter().enumerate() {
                let angle = (i as f64) * 2.0 * PI / (period as f64);
                assert_eq!(sin.to_bits(), angle.sin().to_bits());
                assert_eq!(cos.to_bits(), angle.cos().to_bits());
            }
        }
    }
}
