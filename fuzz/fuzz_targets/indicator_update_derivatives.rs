#![no_main]
//! Fuzz derivatives `Indicator<Input = DerivativesTick>` implementations with
//! arbitrary perpetual / futures tick streams.
//!
//! Each iteration consumes a byte stream, interprets it as a sequence of `f64`
//! values (8 bytes each), and packs consecutive groups of eleven into a
//! [`DerivativesTick`]'s numeric fields. Ticks are built with `new_unchecked`
//! so the fuzzer can explore degenerate values (non-finite, negative, zero
//! prices) that the validating constructor would reject — the indicators must
//! never panic, streaming or batched.

use libfuzzer_sys::fuzz_target;
use wickra_core::{BatchExt, CalendarSpread, DerivativesTick, FundingBasis, FundingImpliedApr, FundingRate, FundingRateMean, FundingRateZScore, Indicator, LiquidationFeatures, LongShortRatio, OIPriceDivergence, OIWeighted, OiToVolumeRatio, OpenInterestDelta, OpenInterestMomentum, PerpetualPremiumIndex, TakerBuySellRatio, TermStructureBasis};

#[inline(never)]
fn drive<I>(make: impl Fn() -> I, ticks: &[DerivativesTick])
where
    I: Indicator<Input = DerivativesTick, Output = f64> + BatchExt,
{
    let mut streaming = make();
    for &tick in ticks {
        let _ = streaming.update(tick);
    }
    let _ = make().batch(ticks);
}

fuzz_target!(|data: &[u8]| {
    let floats: Vec<f64> = data
        .chunks_exact(8)
        .map(|c| f64::from_le_bytes(c.try_into().expect("8 bytes")))
        .collect();
    let ticks: Vec<DerivativesTick> = floats
        .chunks_exact(11)
        .map(|c| {
            DerivativesTick::new_unchecked(
                c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7], c[8], c[9], c[10], 0,
            )
        })
        .collect();

    drive(FundingRate::new, &ticks);
    drive(|| FundingRateMean::new(5).unwrap(), &ticks);
    drive(|| FundingRateZScore::new(5).unwrap(), &ticks);
    drive(FundingBasis::new, &ticks);
    drive(OpenInterestDelta::new, &ticks);
    drive(|| OIPriceDivergence::new(5).unwrap(), &ticks);
    drive(OIWeighted::new, &ticks);
    drive(LongShortRatio::new, &ticks);
    drive(TakerBuySellRatio::new, &ticks);
    drive(TermStructureBasis::new, &ticks);
    drive(CalendarSpread::new, &ticks);
    drive(OiToVolumeRatio::new, &ticks);
    drive(PerpetualPremiumIndex::new, &ticks);
    drive(|| FundingImpliedApr::new(1095.0).unwrap(), &ticks);
    drive(|| OpenInterestMomentum::new(14).unwrap(), &ticks);

    // LiquidationFeatures emits a struct, not an f64, so drive it directly.
    let mut liq = LiquidationFeatures::new();
    for &tick in &ticks {
        let _ = liq.update(tick);
    }
    let _ = LiquidationFeatures::new().batch(&ticks);
});
