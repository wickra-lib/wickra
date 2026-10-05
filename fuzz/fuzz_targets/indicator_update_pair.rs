#![no_main]
//! Fuzz two-input `Indicator<(f64, f64)>` implementations with arbitrary
//! `(asset, benchmark)` return pairs.
//!
//! Each iteration consumes a byte stream and interprets it as a sequence of
//! `(f64, f64)` pairs (8 bytes per `f64`), then drives every two-series
//! indicator over the sequence both streaming and as a batch. No path may
//! panic.

use libfuzzer_sys::fuzz_target;
use wickra_core::{Alpha, BatchExt, BetaNeutralSpread, Cointegration, DistanceSsd, EstimatedLeverageRatio, GrangerCausality, HasbrouckInformationShare, Indicator, InformationRatio, KalmanHedgeRatio, KendallTau, LeadLagCrossCorrelation, OuHalfLife, PairSpreadZScore, PairwiseBeta, RelativeStrengthAB, RollingCorrelation, RollingCovariance, SpreadAr1Coefficient, SpreadBollingerBands, SpreadHurst, TreynorRatio, VarianceRatio};

#[inline(never)]
fn drive<I>(make: impl Fn() -> I, data: &[(f64, f64)])
where
    I: Indicator<Input = (f64, f64), Output = f64> + BatchExt,
{
    let mut streaming = make();
    for &x in data {
        let _ = streaming.update(x);
    }
    let _ = make().batch(data);
}

fuzz_target!(|data: &[u8]| {
    // Pack two consecutive 8-byte chunks into one `(f64, f64)` pair.
    let pairs: Vec<(f64, f64)> = data
        .chunks_exact(16)
        .map(|c| {
            let a = f64::from_le_bytes(c[..8].try_into().expect("8 bytes"));
            let b = f64::from_le_bytes(c[8..].try_into().expect("8 bytes"));
            (a, b)
        })
        .collect();

    drive(|| TreynorRatio::new(10, 0.0).unwrap(), &pairs);
    drive(|| InformationRatio::new(10).unwrap(), &pairs);
    drive(|| Alpha::new(10, 0.0).unwrap(), &pairs);
    drive(|| PairwiseBeta::new(10).unwrap(), &pairs);
    drive(|| PairSpreadZScore::new(10, 10).unwrap(), &pairs);
    drive(|| RollingCorrelation::new(20).unwrap(), &pairs);
    drive(|| RollingCovariance::new(20).unwrap(), &pairs);
    drive(|| OuHalfLife::new(60).unwrap(), &pairs);
    drive(|| SpreadHurst::new(60).unwrap(), &pairs);
    drive(|| DistanceSsd::new(20).unwrap(), &pairs);
    drive(|| BetaNeutralSpread::new(20).unwrap(), &pairs);
    drive(|| VarianceRatio::new(60, 2).unwrap(), &pairs);
    drive(|| GrangerCausality::new(60, 1).unwrap(), &pairs);
    drive(|| SpreadAr1Coefficient::new(40).unwrap(), &pairs);
    drive(|| KendallTau::new(20).unwrap(), &pairs);
    drive(|| HasbrouckInformationShare::new(6).unwrap(), &pairs);
    drive(EstimatedLeverageRatio::new, &pairs);

    // Struct-output pair indicator: drive update + batch directly (the generic
    // `drive` above only covers `Output = f64`).
    let mut ll = LeadLagCrossCorrelation::new(8, 3).unwrap();
    for &x in &pairs {
        let _ = ll.update(x);
    }
    let _ = LeadLagCrossCorrelation::new(8, 3).unwrap().batch(&pairs);

    let mut co = Cointegration::new(12, 1).unwrap();
    for &x in &pairs {
        let _ = co.update(x);
    }
    let _ = Cointegration::new(12, 1).unwrap().batch(&pairs);

    let mut rs = RelativeStrengthAB::new(10, 14).unwrap();
    for &x in &pairs {
        let _ = rs.update(x);
    }
    let _ = RelativeStrengthAB::new(10, 14).unwrap().batch(&pairs);
    let mut kalman_hedge_ratio = KalmanHedgeRatio::new(0.001, 0.001).unwrap();
    for &x in &pairs {
        let _ = kalman_hedge_ratio.update(x);
    }
    let _ = KalmanHedgeRatio::new(0.001, 0.001).unwrap().batch(&pairs);

    let mut spread_bollinger_bands = SpreadBollingerBands::new(20, 2.0).unwrap();
    for &x in &pairs {
        let _ = spread_bollinger_bands.update(x);
    }
    let _ = SpreadBollingerBands::new(20, 2.0).unwrap().batch(&pairs);

});
