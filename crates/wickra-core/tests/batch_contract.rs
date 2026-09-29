//! The batch contract, held for every scalar indicator on adversarial input.
//!
//! `batch_nan` is the exact batch: bit for bit what streaming `update` gives,
//! `NaN` where it gives `None`. `batch_fast` may reassociate the arithmetic, so
//! it is held to its structural promises here -- the same length and `NaN` in
//! the same places on any input, and the exact batch itself whenever an input is
//! non-finite or out of the kernels' range -- while the value tolerance on
//! well-conditioned data is pinned by each kernel's own tests and by every
//! binding's suite. The list mirrors the scalar fuzz target.

use wickra_core::{
    AdaptiveCycle, AdaptiveLaguerreFilter, AdaptiveRsi, Alma, AnchoredRsi, Apo, Autocorrelation,
    AutocorrelationPeriodogram, AverageDrawdown, BandpassFilter, BatchNanExt, BipowerVariation,
    BollingerBandwidth, BurkeRatio, CalmarRatio, CenterOfGravity, Cfo, Cmo, CoefficientOfVariation,
    CommonSenseRatio, ConditionalValueAtRisk, ConnorsRsi, Coppock, CorrelationTrendIndicator,
    CyberneticCycle, Decycler, DecyclerOscillator, Dema, DerivativeOscillator, DetrendedStdDev,
    DisparityIndex, Dpo, DynamicMomentumIndex, EhlersStochastic, Ehma, ElderImpulse, Ema,
    EmpiricalModeDecomposition, EvenBetterSinewave, EwmaVolatility, Expectancy, Fama, FisherRsi,
    FisherTransform, Frama, GainLossRatio, GainToPainRatio, Garch11, GeneralizedDema, GeometricMa,
    HighpassFilter, HilbertDominantCycle, HistoricalVolatility, Hma, HoltWinters, HtDcPhase,
    HtTrendMode, HurstExponent, Indicator, InstantaneousTrendline, InverseFisherTransform,
    JarqueBera, Jma, JumpIndicator, KRatio, Kama, KellyCriterion, Kurtosis, LaguerreRsi,
    LinRegAngle, LinRegIntercept, LinRegSlope, LinearRegression, LogReturn, M2Measure,
    MacdHistogram, MartinRatio, MaxDrawdown, McGinleyDynamic, MedianAbsoluteDeviation, MedianMa,
    MidPoint, Mom, OmegaRatio, PainIndex, PercentB, PercentageTrailingStop, Pmo,
    PolarizedFractalEfficiency, Ppo, PpoHistogram, ProfitFactor, RSquared, RealizedVolatility,
    Reflex, RegimeLabel, RenkoTrailingStop, Rmi, Roc, Rocp, Rocr, Rocr100, RollingIqr,
    RollingMinMaxScaler, RollingPercentileRank, RollingQuantile, RoofingFilter, Rsi, Rsx,
    RviVolatility, SampleEntropy, ShannonEntropy, SharpeRatio, SineWave, SineWeightedMa, Skewness,
    Sma, Smma, SortinoRatio, StandardError, Stc, StdDev, StepTrailingStop, SterlingRatio, StochRsi,
    SuperSmoother, TailRatio, Tema, Tii, TrendLabel, TrendStrengthIndex, Trendflex, Trima, Trix,
    Tsf, TsfOscillator, Tsi, UlcerIndex, UniversalOscillator, UpsidePotentialRatio, ValueAtRisk,
    Variance, VerticalHorizontalFilter, Vidya, VolatilityOfVolatility, WavePm, WinRate, Wma,
    ZScore, Zlema, T3,
};

/// Deterministic pseudo-random values in `[-1, 1)`.
fn lcg(n: usize, seed: u64) -> Vec<f64> {
    let mut state = seed;
    (0..n)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let top = u32::try_from(state >> 32).unwrap();
            f64::from(top) / 2_147_483_648.0 - 1.0
        })
        .collect()
}

/// The adversarial inputs, each paired with whether `batch_fast` must fall back
/// to the exact batch bit for bit on it.
fn cases() -> Vec<(&'static str, Vec<f64>, bool)> {
    let n = 700;
    let smooth: Vec<f64> = (0..n)
        .map(|i| {
            let t = f64::from(u32::try_from(i).unwrap());
            100.0 + (t * 0.05).sin() * 5.0 + (t * 0.37).cos()
        })
        .collect();
    let noise: Vec<f64> = lcg(n, 7).iter().map(|v| v * 1e3).collect();
    let mut with_nan = smooth.clone();
    with_nan[350] = f64::NAN;
    let mut with_inf = smooth.clone();
    with_inf[200] = f64::INFINITY;
    with_inf[400] = f64::NEG_INFINITY;
    let huge: Vec<f64> = smooth.iter().map(|v| v * 1e150).collect();
    let flat = vec![42.0; n];
    let steps: Vec<f64> = (0..n)
        .map(|i| if (i / 50) % 2 == 0 { 10.0 } else { 1e6 })
        .collect();
    let tiny: Vec<f64> = lcg(n, 11).iter().map(|v| v.abs() * 1e-300).collect();
    let zeros_and_signs: Vec<f64> = lcg(n, 13).iter().map(|v| (v * 3.0).round()).collect();
    vec![
        ("smooth", smooth, false),
        ("noise", noise, false),
        ("nan", with_nan, true),
        ("inf", with_inf, true),
        ("huge", huge, true),
        ("flat", flat, false),
        ("steps", steps, false),
        ("tiny", tiny, false),
        ("zeros_and_signs", zeros_and_signs, false),
    ]
}

fn bits(values: &[f64]) -> Vec<u64> {
    values.iter().map(|v| v.to_bits()).collect()
}

fn check<I>(make: impl Fn() -> I, cases: &[(&str, Vec<f64>, bool)])
where
    I: Indicator<Input = f64, Output = f64>,
{
    let name = make().name();
    for (case, data, falls_back) in cases {
        let mut streaming = make();
        let streamed: Vec<f64> = data
            .iter()
            .map(|&x| streaming.update(x).unwrap_or(f64::NAN))
            .collect();
        let exact = make().batch_nan(data);
        assert_eq!(
            bits(&exact),
            bits(&streamed),
            "{name} on {case}: batch_nan differs from streaming"
        );
        let fast = make().batch_fast(data);
        assert_eq!(fast.len(), exact.len(), "{name} on {case}: length");
        let placed = exact
            .iter()
            .zip(&fast)
            .all(|(x, y)| x.is_nan() == y.is_nan());
        assert!(placed, "{name} on {case}: NaN placement differs");
        if *falls_back {
            assert_eq!(bits(&fast), bits(&exact), "{name} on {case}: no fallback");
        }
    }
}

#[test]
fn scalar_batches_keep_the_contract_1_of_4() {
    let cases = cases();
    check(|| Sma::new(14).unwrap(), &cases);
    check(|| Ema::new(20).unwrap(), &cases);
    check(|| Wma::new(14).unwrap(), &cases);
    check(|| Rsi::new(14).unwrap(), &cases);
    check(AnchoredRsi::new, &cases);
    check(|| Dema::new(14).unwrap(), &cases);
    check(|| Tema::new(14).unwrap(), &cases);
    check(|| Hma::new(14).unwrap(), &cases);
    check(|| SineWeightedMa::new(14).unwrap(), &cases);
    check(|| GeometricMa::new(14).unwrap(), &cases);
    check(|| Ehma::new(9).unwrap(), &cases);
    check(|| MedianMa::new(14).unwrap(), &cases);
    check(|| AdaptiveLaguerreFilter::new(13).unwrap(), &cases);
    check(|| GeneralizedDema::new(5, 0.7).unwrap(), &cases);
    check(|| HoltWinters::new(0.2, 0.1).unwrap(), &cases);
    check(|| Roc::new(14).unwrap(), &cases);
    check(|| Rocp::new(14).unwrap(), &cases);
    check(|| Rocr::new(14).unwrap(), &cases);
    check(|| Rocr100::new(14).unwrap(), &cases);
    check(|| Trix::new(14).unwrap(), &cases);
    check(|| Smma::new(14).unwrap(), &cases);
    check(|| Trima::new(14).unwrap(), &cases);
    check(|| Zlema::new(14).unwrap(), &cases);
    check(|| Kama::new(10, 2, 30).unwrap(), &cases);
    check(|| Alma::new(9, 0.85, 6.0).unwrap(), &cases);
    check(|| McGinleyDynamic::new(10).unwrap(), &cases);
    check(|| Frama::new(16).unwrap(), &cases);
    check(|| Vidya::new(14, 9).unwrap(), &cases);
    check(|| Jma::new(14, 0.0, 2).unwrap(), &cases);
    check(|| T3::new(14, 0.7).unwrap(), &cases);
    check(|| Mom::new(14).unwrap(), &cases);
    check(|| Cmo::new(14).unwrap(), &cases);
    check(|| DisparityIndex::new(14).unwrap(), &cases);
    check(|| FisherRsi::new(14).unwrap(), &cases);
    check(|| Rsx::new(14).unwrap(), &cases);
    check(|| DynamicMomentumIndex::new(14).unwrap(), &cases);
    check(|| Rmi::new(14, 5).unwrap(), &cases);
    check(|| DerivativeOscillator::new(14, 5, 3, 9).unwrap(), &cases);
    check(|| TrendStrengthIndex::new(20).unwrap(), &cases);
    check(|| PolarizedFractalEfficiency::new(10, 5).unwrap(), &cases);
}

#[test]
fn scalar_batches_keep_the_contract_2_of_4() {
    let cases = cases();
    check(|| WavePm::new(32, 3).unwrap(), &cases);
    check(|| Tsi::new(25, 13).unwrap(), &cases);
    check(|| Pmo::new(35, 20).unwrap(), &cases);
    check(|| Tii::new(60, 30).unwrap(), &cases);
    check(|| StochRsi::new(14, 14).unwrap(), &cases);
    check(|| Dpo::new(14).unwrap(), &cases);
    check(|| Ppo::new(12, 26).unwrap(), &cases);
    check(|| Apo::new(12, 26).unwrap(), &cases);
    check(|| Cfo::new(14).unwrap(), &cases);
    check(|| TsfOscillator::new(14).unwrap(), &cases);
    check(|| MacdHistogram::new(12, 26, 9).unwrap(), &cases);
    check(|| PpoHistogram::new(12, 26, 9).unwrap(), &cases);
    check(ElderImpulse::classic, &cases);
    check(Stc::classic, &cases);
    check(|| Coppock::new(14, 11, 10).unwrap(), &cases);
    check(|| StdDev::new(14).unwrap(), &cases);
    check(|| UlcerIndex::new(14).unwrap(), &cases);
    check(|| HistoricalVolatility::new(14, 252).unwrap(), &cases);
    check(|| LinearRegression::new(14).unwrap(), &cases);
    check(|| MidPoint::new(14).unwrap(), &cases);
    check(|| LinRegSlope::new(14).unwrap(), &cases);
    check(|| LinRegIntercept::new(14).unwrap(), &cases);
    check(|| Tsf::new(14).unwrap(), &cases);
    check(|| LinRegAngle::new(14).unwrap(), &cases);
    check(|| VerticalHorizontalFilter::new(14).unwrap(), &cases);
    check(|| ZScore::new(14).unwrap(), &cases);
    check(|| Variance::new(14).unwrap(), &cases);
    check(|| CoefficientOfVariation::new(14).unwrap(), &cases);
    check(|| Skewness::new(14).unwrap(), &cases);
    check(|| Kurtosis::new(14).unwrap(), &cases);
    check(|| StandardError::new(14).unwrap(), &cases);
    check(|| DetrendedStdDev::new(14).unwrap(), &cases);
    check(|| RSquared::new(14).unwrap(), &cases);
    check(|| MedianAbsoluteDeviation::new(14).unwrap(), &cases);
    check(|| Autocorrelation::new(14, 2).unwrap(), &cases);
    check(|| HurstExponent::new(16, 4).unwrap(), &cases);
    check(|| LogReturn::new(1).unwrap(), &cases);
    check(|| RealizedVolatility::new(20).unwrap(), &cases);
    check(|| EwmaVolatility::new(0.94).unwrap(), &cases);
    check(|| Garch11::new(0.000_002, 0.1, 0.88).unwrap(), &cases);
}

#[test]
fn scalar_batches_keep_the_contract_3_of_4() {
    let cases = cases();
    check(|| BipowerVariation::new(20).unwrap(), &cases);
    check(|| VolatilityOfVolatility::new(20, 20).unwrap(), &cases);
    check(|| RollingQuantile::new(20, 0.5).unwrap(), &cases);
    check(|| RollingIqr::new(14).unwrap(), &cases);
    check(|| RollingPercentileRank::new(14).unwrap(), &cases);
    check(|| JarqueBera::new(20).unwrap(), &cases);
    check(|| RollingMinMaxScaler::new(20).unwrap(), &cases);
    check(|| ShannonEntropy::new(20, 8).unwrap(), &cases);
    check(|| SampleEntropy::new(20, 2, 0.2).unwrap(), &cases);
    check(|| TrendLabel::new(14).unwrap(), &cases);
    check(|| JumpIndicator::new(20, 3.0).unwrap(), &cases);
    check(|| RegimeLabel::new(5, 20).unwrap(), &cases);
    check(|| RviVolatility::new(10).unwrap(), &cases);
    check(|| LaguerreRsi::new(0.5).unwrap(), &cases);
    check(ConnorsRsi::classic, &cases);
    check(|| PercentageTrailingStop::new(5.0).unwrap(), &cases);
    check(|| StepTrailingStop::new(1.0).unwrap(), &cases);
    check(|| RenkoTrailingStop::new(1.0).unwrap(), &cases);
    check(|| SuperSmoother::new(10).unwrap(), &cases);
    check(|| FisherTransform::new(10).unwrap(), &cases);
    check(|| InverseFisherTransform::new(1.0).unwrap(), &cases);
    check(|| Decycler::new(20).unwrap(), &cases);
    check(|| DecyclerOscillator::new(10, 30).unwrap(), &cases);
    check(|| RoofingFilter::new(10, 48).unwrap(), &cases);
    check(|| CenterOfGravity::new(10).unwrap(), &cases);
    check(|| CyberneticCycle::new(10).unwrap(), &cases);
    check(|| InstantaneousTrendline::new(20).unwrap(), &cases);
    check(|| EhlersStochastic::new(20).unwrap(), &cases);
    check(|| HighpassFilter::new(48).unwrap(), &cases);
    check(|| Reflex::new(20).unwrap(), &cases);
    check(|| Trendflex::new(20).unwrap(), &cases);
    check(|| CorrelationTrendIndicator::new(20).unwrap(), &cases);
    check(|| AdaptiveRsi::new(14).unwrap(), &cases);
    check(|| UniversalOscillator::new(20).unwrap(), &cases);
    check(|| BandpassFilter::new(20, 0.3).unwrap(), &cases);
    check(|| EvenBetterSinewave::new(40, 10).unwrap(), &cases);
    check(|| AutocorrelationPeriodogram::new(10, 48).unwrap(), &cases);
    check(|| EmpiricalModeDecomposition::new(20, 0.5).unwrap(), &cases);
    check(HilbertDominantCycle::new, &cases);
    check(HtDcPhase::new, &cases);
}

#[test]
fn scalar_batches_keep_the_contract_4_of_4() {
    let cases = cases();
    check(HtTrendMode::new, &cases);
    check(AdaptiveCycle::new, &cases);
    check(SineWave::new, &cases);
    check(|| Fama::new(0.5, 0.05).unwrap(), &cases);
    check(|| SharpeRatio::new(20, 0.0).unwrap(), &cases);
    check(|| SortinoRatio::new(20, 0.0).unwrap(), &cases);
    check(|| CalmarRatio::new(20).unwrap(), &cases);
    check(|| OmegaRatio::new(20, 0.0).unwrap(), &cases);
    check(|| MaxDrawdown::new(20).unwrap(), &cases);
    check(|| AverageDrawdown::new(20).unwrap(), &cases);
    check(|| PainIndex::new(20).unwrap(), &cases);
    check(|| ValueAtRisk::new(20, 0.95).unwrap(), &cases);
    check(|| ConditionalValueAtRisk::new(20, 0.95).unwrap(), &cases);
    check(|| ProfitFactor::new(20).unwrap(), &cases);
    check(|| GainLossRatio::new(20).unwrap(), &cases);
    check(|| KellyCriterion::new(20).unwrap(), &cases);
    check(|| WinRate::new(20).unwrap(), &cases);
    check(|| Expectancy::new(20).unwrap(), &cases);
    check(|| SterlingRatio::new(12).unwrap(), &cases);
    check(|| BurkeRatio::new(12).unwrap(), &cases);
    check(|| MartinRatio::new(14).unwrap(), &cases);
    check(|| TailRatio::new(20).unwrap(), &cases);
    check(|| KRatio::new(30).unwrap(), &cases);
    check(|| CommonSenseRatio::new(20).unwrap(), &cases);
    check(|| GainToPainRatio::new(12).unwrap(), &cases);
    check(|| UpsidePotentialRatio::new(20, 0.0).unwrap(), &cases);
    check(|| M2Measure::new(20, 0.0, 0.02).unwrap(), &cases);
    check(|| BollingerBandwidth::new(20, 2.0).unwrap(), &cases);
    check(|| PercentB::new(20, 2.0).unwrap(), &cases);
}
