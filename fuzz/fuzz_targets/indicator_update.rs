#![no_main]
//! Fuzz scalar-input indicator updates with arbitrary `f64` sequences.
//!
//! Every scalar indicator must tolerate any finite-or-not input stream — NaN,
//! ±inf, subnormals, abrupt jumps — without panicking. Each fuzz iteration
//! runs the **same** input sequence through every scalar indicator twice:
//! once as a streaming `update` loop and once as a full `batch` call. Neither
//! path may panic; `batch` is also expected to agree with the streaming path
//! (the `BatchExt` blanket implementation replays `update` internally, so the
//! agreement is structural — but exercising both paths surfaces any
//! state-mutation bugs in `update` that would only manifest mid-batch).
//!
//! It also runs `batch_nan`, whose fused paths must reproduce the streaming
//! result bit for bit on any input, and `batch_fast`, which must keep the
//! exact batch's length and `NaN` placement.
//!
//! Audit finding R9: the previous version covered only `Rsi(14)` and
//! `Ema(20)`. This target now covers every scalar indicator in the catalogue.

use libfuzzer_sys::fuzz_target;
use wickra_core::{AdaptiveCycle, BatchNanExt, AdaptiveLaguerreFilter, AdaptiveRsi, Alma, AnchoredRsi, Apo, Autocorrelation, AutocorrelationPeriodogram, AverageDrawdown, BandpassFilter, BatchExt, Beta, BipowerVariation, BollingerBands, BollingerBandwidth, BomarBands, BurkeRatio, CalmarRatio, CenterOfGravity, Cfo, Cmo, CoefficientOfVariation, CommonSenseRatio, ConditionalValueAtRisk, ConnorsRsi, Coppock, CorrelationTrendIndicator, CyberneticCycle, Decycler, DecyclerOscillator, Dema, DerivativeOscillator, DetrendedStdDev, DisparityIndex, DoubleBollinger, Dpo, DrawdownDuration, DynamicMomentumIndex, EhlersStochastic, Ehma, ElderImpulse, Ema, EmpiricalModeDecomposition, EvenBetterSinewave, EwmaVolatility, Expectancy, Fama, FisherRsi, FisherTransform, Frama, GainLossRatio, GainToPainRatio, Garch11, GeneralizedDema, GeometricMa, HighpassFilter, HilbertDominantCycle, HistoricalVolatility, Hma, HoltWinters, HtDcPhase, HtPhasor, HtTrendMode, HurstExponent, Indicator, InstantaneousTrendline, InverseFisherTransform, JarqueBera, Jma, JumpIndicator, KRatio, Kama, KellyCriterion, Kst, Kurtosis, LaguerreRsi, LinRegAngle, LinRegChannel, LinRegIntercept, LinRegSlope, LinearRegression, LogReturn, M2Measure, MaEnvelope, MaType, MacdExt, MacdFix, MacdHistogram, MacdIndicator, Mama, MartinRatio, MaxDrawdown, McGinleyDynamic, MedianAbsoluteDeviation, MedianChannel, MedianMa, MidPoint, Mom, OmegaRatio, PainIndex, PearsonCorrelation, PercentB, PercentageTrailingStop, Pmo, PolarizedFractalEfficiency, Ppo, PpoHistogram, ProfitFactor, Qqe, QuartileBands, RSquared, RealizedVolatility, RecoveryFactor, Reflex, RegimeLabel, RenkoTrailingStop, Rmi, Roc, Rocp, Rocr, Rocr100, RollingIqr, RollingMinMaxScaler, RollingPercentileRank, RollingQuantile, RoofingFilter, Rsi, Rsx, RviVolatility, SampleEntropy, ShannonEntropy, SharpeRatio, SineWave, SineWeightedMa, Skewness, Sma, Smma, SortinoRatio, SpearmanCorrelation, StandardError, StandardErrorBands, Stc, StdDev, StepTrailingStop, SterlingRatio, StochRsi, SuperSmoother, TailRatio, Tema, Tii, TrendLabel, TrendStrengthIndex, Trendflex, Trima, Trix, Tsf, TsfOscillator, Tsi, UlcerIndex, UniversalOscillator, UpsidePotentialRatio, ValueAtRisk, Variance, VerticalHorizontalFilter, Vidya, VolatilityOfVolatility, WavePm, WinRate, Wma, ZScore, ZeroLagMacd, Zlema, T3};

/// Drive a single streaming + batch run through one scalar indicator. Marked
/// `#[inline(never)]` so a panic backtrace pin-points the specific indicator.
#[inline(never)]
fn drive<I>(make: impl Fn() -> I, data: &[f64])
where
    I: Indicator<Input = f64, Output = f64> + BatchExt,
{
    let mut streaming = make();
    let streamed: Vec<u64> = data
        .iter()
        .map(|&x| streaming.update(x).unwrap_or(f64::NAN).to_bits())
        .collect();
    let _ = make().batch(data);
    // The exact batch is streaming bit for bit; the fast batch keeps its length
    // and NaN placement on any input (its value tolerance is a property of
    // well-conditioned data, not of arbitrary bytes).
    let exact = make().batch_nan(data);
    let exact_bits: Vec<u64> = exact.iter().map(|v| v.to_bits()).collect();
    assert_eq!(exact_bits, streamed);
    let fast = make().batch_fast(data);
    assert_eq!(fast.len(), exact.len());
    assert!(exact.iter().zip(&fast).all(|(x, y)| x.is_nan() == y.is_nan()));
}

fuzz_target!(|data: Vec<f64>| {
    // Bounded periods keep each iteration cheap and bias the fuzzer toward
    // adversarial input patterns rather than enormous windows. The constants
    // mirror the README's "common defaults" so we cover the parameterisations
    // most users actually instantiate.
    drive(|| Sma::new(14).unwrap(), &data);
    drive(|| Ema::new(20).unwrap(), &data);
    drive(|| Wma::new(14).unwrap(), &data);
    drive(|| Rsi::new(14).unwrap(), &data);
    drive(AnchoredRsi::new, &data);
    drive(|| Dema::new(14).unwrap(), &data);
    drive(|| Tema::new(14).unwrap(), &data);
    drive(|| Hma::new(14).unwrap(), &data);
    drive(|| SineWeightedMa::new(14).unwrap(), &data);
    drive(|| GeometricMa::new(14).unwrap(), &data);
    drive(|| Ehma::new(9).unwrap(), &data);
    drive(|| MedianMa::new(14).unwrap(), &data);
    drive(|| AdaptiveLaguerreFilter::new(13).unwrap(), &data);
    drive(|| GeneralizedDema::new(5, 0.7).unwrap(), &data);
    drive(|| HoltWinters::new(0.2, 0.1).unwrap(), &data);
    drive(|| Roc::new(14).unwrap(), &data);
    drive(|| Rocp::new(14).unwrap(), &data);
    drive(|| Rocr::new(14).unwrap(), &data);
    drive(|| Rocr100::new(14).unwrap(), &data);
    drive(|| Trix::new(14).unwrap(), &data);
    drive(|| Smma::new(14).unwrap(), &data);
    drive(|| Trima::new(14).unwrap(), &data);
    drive(|| Zlema::new(14).unwrap(), &data);
    drive(|| Kama::new(10, 2, 30).unwrap(), &data);
    drive(|| Alma::new(9, 0.85, 6.0).unwrap(), &data);
    drive(|| McGinleyDynamic::new(10).unwrap(), &data);
    drive(|| Frama::new(16).unwrap(), &data);
    drive(|| Vidya::new(14, 9).unwrap(), &data);
    drive(|| Jma::new(14, 0.0, 2).unwrap(), &data);
    drive(|| T3::new(14, 0.7).unwrap(), &data);
    drive(|| Mom::new(14).unwrap(), &data);
    drive(|| Cmo::new(14).unwrap(), &data);
    drive(|| DisparityIndex::new(14).unwrap(), &data);
    drive(|| FisherRsi::new(14).unwrap(), &data);
    drive(|| Rsx::new(14).unwrap(), &data);
    drive(|| DynamicMomentumIndex::new(14).unwrap(), &data);
    drive(|| Rmi::new(14, 5).unwrap(), &data);
    drive(|| DerivativeOscillator::new(14, 5, 3, 9).unwrap(), &data);
    drive(|| TrendStrengthIndex::new(20).unwrap(), &data);
    drive(|| PolarizedFractalEfficiency::new(10, 5).unwrap(), &data);
    drive(|| WavePm::new(32, 3).unwrap(), &data);
    drive(|| Tsi::new(25, 13).unwrap(), &data);
    drive(|| Pmo::new(35, 20).unwrap(), &data);
    drive(|| Tii::new(60, 30).unwrap(), &data);
    drive(|| StochRsi::new(14, 14).unwrap(), &data);
    drive(|| Dpo::new(14).unwrap(), &data);
    drive(|| Ppo::new(12, 26).unwrap(), &data);
    drive(|| Apo::new(12, 26).unwrap(), &data);
    drive(|| Cfo::new(14).unwrap(), &data);
    drive(|| TsfOscillator::new(14).unwrap(), &data);
    drive(|| MacdHistogram::new(12, 26, 9).unwrap(), &data);
    drive(|| PpoHistogram::new(12, 26, 9).unwrap(), &data);
    drive(|| ElderImpulse::classic(), &data);
    drive(|| Stc::classic(), &data);
    drive(|| Coppock::new(14, 11, 10).unwrap(), &data);
    drive(|| StdDev::new(14).unwrap(), &data);
    drive(|| UlcerIndex::new(14).unwrap(), &data);
    drive(|| HistoricalVolatility::new(14, 252).unwrap(), &data);
    drive(|| LinearRegression::new(14).unwrap(), &data);
    drive(|| MidPoint::new(14).unwrap(), &data);
    drive(|| LinRegSlope::new(14).unwrap(), &data);
    drive(|| LinRegIntercept::new(14).unwrap(), &data);
    drive(|| Tsf::new(14).unwrap(), &data);
    drive(|| LinRegAngle::new(14).unwrap(), &data);
    drive(|| VerticalHorizontalFilter::new(14).unwrap(), &data);
    drive(|| ZScore::new(14).unwrap(), &data);
    drive(|| Variance::new(14).unwrap(), &data);
    drive(|| CoefficientOfVariation::new(14).unwrap(), &data);
    drive(|| Skewness::new(14).unwrap(), &data);
    drive(|| Kurtosis::new(14).unwrap(), &data);
    drive(|| StandardError::new(14).unwrap(), &data);
    drive(|| DetrendedStdDev::new(14).unwrap(), &data);
    drive(|| RSquared::new(14).unwrap(), &data);
    drive(|| MedianAbsoluteDeviation::new(14).unwrap(), &data);
    drive(|| Autocorrelation::new(14, 2).unwrap(), &data);
    // HurstExponent needs `period >= 2 * chunks`; 16/4 is the cheapest fit
    // that still exercises every code path.
    drive(|| HurstExponent::new(16, 4).unwrap(), &data);
    drive(|| LogReturn::new(1).unwrap(), &data);
    drive(|| RealizedVolatility::new(20).unwrap(), &data);
    drive(|| EwmaVolatility::new(0.94).unwrap(), &data);
    drive(|| Garch11::new(0.000_002, 0.1, 0.88).unwrap(), &data);
    drive(|| BipowerVariation::new(20).unwrap(), &data);
    drive(|| VolatilityOfVolatility::new(20, 20).unwrap(), &data);
    drive(|| RollingQuantile::new(20, 0.5).unwrap(), &data);
    drive(|| RollingIqr::new(14).unwrap(), &data);
    drive(|| RollingPercentileRank::new(14).unwrap(), &data);
    drive(|| JarqueBera::new(20).unwrap(), &data);
    drive(|| RollingMinMaxScaler::new(20).unwrap(), &data);
    drive(|| ShannonEntropy::new(20, 8).unwrap(), &data);
    drive(|| SampleEntropy::new(20, 2, 0.2).unwrap(), &data);
    drive(|| TrendLabel::new(14).unwrap(), &data);
    drive(|| JumpIndicator::new(20, 3.0).unwrap(), &data);
    drive(|| RegimeLabel::new(5, 20).unwrap(), &data);
    drive(|| RviVolatility::new(10).unwrap(), &data);
    drive(|| LaguerreRsi::new(0.5).unwrap(), &data);
    drive(|| ConnorsRsi::classic(), &data);

    // KST is scalar-input but emits `KstOutput`, so it bypasses the generic
    // `drive` helper. Streaming + batch are still both exercised.
    {
        let mut kst = Kst::classic();
        for &x in &data {
            let _ = kst.update(x);
        }
        let _ = Kst::classic().batch(&data);
    }

    // QQE is scalar-input but emits `QqeOutput`, so it bypasses the generic
    // `drive` helper. Streaming + batch are still both exercised.
    {
        let mut qqe = Qqe::new(14, 5, 4.236).unwrap();
        for &x in &data {
            let _ = qqe.update(x);
        }
        let _ = Qqe::new(14, 5, 4.236).unwrap().batch(&data);
    }

    // Zero-Lag MACD shares MACD's multi-output topology, so it gets the
    // same hand-rolled streaming + batch drive as classic MACD below.
    {
        let mut z = ZeroLagMacd::classic();
        for &x in &data {
            let _ = z.update(x);
        }
        let _ = ZeroLagMacd::classic().batch(&data);
    }

    // --- Trailing Stops (scalar) ---
    drive(|| PercentageTrailingStop::new(5.0).unwrap(), &data);
    drive(|| StepTrailingStop::new(1.0).unwrap(), &data);
    drive(|| RenkoTrailingStop::new(1.0).unwrap(), &data);

    // Family 10 — Ehlers / Cycle scalar indicators.
    drive(|| SuperSmoother::new(10).unwrap(), &data);
    drive(|| FisherTransform::new(10).unwrap(), &data);
    drive(|| InverseFisherTransform::new(1.0).unwrap(), &data);
    drive(|| Decycler::new(20).unwrap(), &data);
    drive(|| DecyclerOscillator::new(10, 30).unwrap(), &data);
    drive(|| RoofingFilter::new(10, 48).unwrap(), &data);
    drive(|| CenterOfGravity::new(10).unwrap(), &data);
    drive(|| CyberneticCycle::new(10).unwrap(), &data);
    drive(|| InstantaneousTrendline::new(20).unwrap(), &data);
    drive(|| EhlersStochastic::new(20).unwrap(), &data);
    drive(|| HighpassFilter::new(48).unwrap(), &data);
    drive(|| Reflex::new(20).unwrap(), &data);
    drive(|| Trendflex::new(20).unwrap(), &data);
    drive(|| CorrelationTrendIndicator::new(20).unwrap(), &data);
    drive(|| AdaptiveRsi::new(14).unwrap(), &data);
    drive(|| UniversalOscillator::new(20).unwrap(), &data);
    drive(|| BandpassFilter::new(20, 0.3).unwrap(), &data);
    drive(|| EvenBetterSinewave::new(40, 10).unwrap(), &data);
    drive(|| AutocorrelationPeriodogram::new(10, 48).unwrap(), &data);
    drive(|| EmpiricalModeDecomposition::new(20, 0.5).unwrap(), &data);
    drive(HilbertDominantCycle::new, &data);
    drive(HtDcPhase::new, &data);
    drive(HtTrendMode::new, &data);
    drive(AdaptiveCycle::new, &data);
    drive(SineWave::new, &data);
    drive(|| Fama::new(0.5, 0.05).unwrap(), &data);

    // Family 15 — Risk / Performance metrics (scalar inputs).
    drive(|| SharpeRatio::new(20, 0.0).unwrap(), &data);
    drive(|| SortinoRatio::new(20, 0.0).unwrap(), &data);
    drive(|| CalmarRatio::new(20).unwrap(), &data);
    drive(|| OmegaRatio::new(20, 0.0).unwrap(), &data);
    drive(|| MaxDrawdown::new(20).unwrap(), &data);
    drive(|| AverageDrawdown::new(20).unwrap(), &data);
    drive(|| PainIndex::new(20).unwrap(), &data);
    drive(|| ValueAtRisk::new(20, 0.95).unwrap(), &data);
    drive(|| ConditionalValueAtRisk::new(20, 0.95).unwrap(), &data);
    drive(|| ProfitFactor::new(20).unwrap(), &data);
    drive(|| GainLossRatio::new(20).unwrap(), &data);
    drive(|| KellyCriterion::new(20).unwrap(), &data);
    drive(|| WinRate::new(20).unwrap(), &data);
    drive(|| Expectancy::new(20).unwrap(), &data);
    drive(|| SterlingRatio::new(12).unwrap(), &data);
    drive(|| BurkeRatio::new(12).unwrap(), &data);
    drive(|| MartinRatio::new(14).unwrap(), &data);
    drive(|| TailRatio::new(20).unwrap(), &data);
    drive(|| KRatio::new(30).unwrap(), &data);
    drive(|| CommonSenseRatio::new(20).unwrap(), &data);
    drive(|| GainToPainRatio::new(12).unwrap(), &data);
    drive(|| UpsidePotentialRatio::new(20, 0.0).unwrap(), &data);
    drive(|| M2Measure::new(20, 0.0, 0.02).unwrap(), &data);

    // RecoveryFactor and DrawdownDuration produce non-`f64` outputs / have
    // no `period` knob, so they cannot use the `drive` helper directly.
    {
        let mut rf = RecoveryFactor::new();
        for &x in &data {
            let _ = rf.update(x);
        }
        let _ = RecoveryFactor::new().batch(&data);
    }
    {
        let mut dd = DrawdownDuration::new();
        for &x in &data {
            let _ = dd.update(x);
        }
        let _ = DrawdownDuration::new().batch(&data);
    }

    // MACD, Bollinger Bands and MAMA have non-`f64` outputs, so they cannot
    // use the generic `drive` helper above. Streaming + batch are still both
    // exercised.
    {
        let mut macd = MacdIndicator::new(12, 26, 9).unwrap();
        for &x in &data {
            let _ = macd.update(x);
        }
        let _ = MacdIndicator::new(12, 26, 9).unwrap().batch(&data);
    }

    // MACDFIX wraps MacdIndicator(12, 26, signal); same multi-output topology.
    {
        let mut fix = MacdFix::new(9).unwrap();
        for &x in &data {
            let _ = fix.update(x);
        }
        let _ = MacdFix::new(9).unwrap().batch(&data);
    }

    // MACDEXT: selectable MA types per line, multi-output topology.
    {
        let mut ext = MacdExt::new(12, MaType::Ema, 26, MaType::Ema, 9, MaType::Sma).unwrap();
        for &x in &data {
            let _ = ext.update(x);
        }
        let _ = MacdExt::new(12, MaType::Ema, 26, MaType::Ema, 9, MaType::Sma)
            .unwrap()
            .batch(&data);
    }

    // HT_PHASOR is scalar-input but emits a {inphase, quadrature} struct, so it
    // bypasses the generic `drive` helper.
    {
        let mut ph = HtPhasor::new();
        for &x in &data {
            let _ = ph.update(x);
        }
        let _ = HtPhasor::new().batch(&data);
    }
    {
        let mut bb = BollingerBands::new(20, 2.0).unwrap();
        for &x in &data {
            let _ = bb.update(x);
        }
        let _ = BollingerBands::new(20, 2.0).unwrap().batch(&data);
    }
    // Both derive from the same bands but reduce to a single `f64`, so they
    // take the generic helper. They were the only two catalogue entries no fuzz
    // target reached.
    drive(|| BollingerBandwidth::new(20, 2.0).unwrap(), &data);
    drive(|| PercentB::new(20, 2.0).unwrap(), &data);
    {
        let mut mama = Mama::new(0.5, 0.05).unwrap();
        for &x in &data {
            let _ = mama.update(x);
        }
        let _ = Mama::new(0.5, 0.05).unwrap().batch(&data);
    }

    // --- Family 05: scalar-input band/channel indicators (multi-output) ---
    {
        let mut medianchannel = MedianChannel::new(5, 2.0).unwrap();
        for &x in &data {
            let _ = medianchannel.update(x);
        }
        let _ = MedianChannel::new(5, 2.0).unwrap().batch(&data);
    }
    {
        let mut bomarbands = BomarBands::new(4, 0.85).unwrap();
        for &x in &data {
            let _ = bomarbands.update(x);
        }
        let _ = BomarBands::new(4, 0.85).unwrap().batch(&data);
    }
    {
        let mut quartilebands = QuartileBands::new(4).unwrap();
        for &x in &data {
            let _ = quartilebands.update(x);
        }
        let _ = QuartileBands::new(4).unwrap().batch(&data);
    }
    {
        let mut env = MaEnvelope::new(20, 0.025).unwrap();
        for &x in &data {
            let _ = env.update(x);
        }
        let _ = MaEnvelope::new(20, 0.025).unwrap().batch(&data);
    }
    {
        let mut ch = LinRegChannel::new(20, 2.0).unwrap();
        for &x in &data {
            let _ = ch.update(x);
        }
        let _ = LinRegChannel::new(20, 2.0).unwrap().batch(&data);
    }
    {
        let mut seb = StandardErrorBands::new(21, 2.0).unwrap();
        for &x in &data {
            let _ = seb.update(x);
        }
        let _ = StandardErrorBands::new(21, 2.0).unwrap().batch(&data);
    }
    {
        let mut db = DoubleBollinger::new(20, 1.0, 2.0).unwrap();
        for &x in &data {
            let _ = db.update(x);
        }
        let _ = DoubleBollinger::new(20, 1.0, 2.0).unwrap().batch(&data);
    }

    // Family 12: Two-series indicators — pair adjacent samples of `data`.
    {
        let mut p = PearsonCorrelation::new(14).unwrap();
        let mut b = Beta::new(14).unwrap();
        let mut s = SpearmanCorrelation::new(14).unwrap();
        for w in data.windows(2) {
            let pair = (w[0], w[1]);
            let _ = p.update(pair);
            let _ = b.update(pair);
            let _ = s.update(pair);
        }
    }
});
