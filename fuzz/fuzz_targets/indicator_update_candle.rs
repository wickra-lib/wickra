#![no_main]
//! Fuzz OHLCV-input indicator updates with arbitrary candle sequences.
//!
//! Every candle-input indicator must tolerate any sequence of validated OHLCV
//! candles — extreme magnitudes, micro-spreads, zero-volume bars, abrupt
//! reversals — without panicking. The fuzzer chunks the raw `f64` stream into
//! `[open, high, low, close, volume]` tuples and constructs each candle via
//! `Candle::new`; entries that fail OHLCV-invariant validation are skipped so
//! the indicator only ever sees structurally-valid candles. Each iteration
//! then drives that candle stream through every candle-input indicator twice
//! (streaming `update` + batch).
//!
//! Audit finding R9: the previous fuzz suite had no candle-input coverage at
//! all. This target now covers every candle-input indicator including the
//! ones the audit named explicitly (ATR, ADX, Stochastic, PSAR) plus the
//! complete catalogue: Keltner, Donchian, SuperTrend, Chandelier Exit, ATR
//! Trailing Stop, Aroon, AwesomeOscillator, CCI, WilliamsR, MFI, OBV, VWAP,
//! RollingVWAP, ADL, VPT, ChaikinMoneyFlow, ChaikinOscillator, ForceIndex,
//! EaseOfMovement, NATR, AroonOscillator, ChandeKrollStop, Vortex, MassIndex,
//! ChoppinessIndex, TrueRange, ChaikinVolatility, AcceleratorOscillator,
//! BalanceOfPower, UltimateOscillator, VWMA, TypicalPrice, MedianPrice,
//! WeightedClose.

use libfuzzer_sys::fuzz_target;
use wickra_core::{AbandonedBaby, Abcd, AccelerationBands, AcceleratorOscillator, AdOscillator, AdaptiveCci, Adl, AdvanceBlock, Adx, Adxr, Alligator, AnchoredVwap, AndrewsPitchfork, Aroon, AroonOscillator, Atr, AtrBands, AtrRatchet, AtrTrailingStop, AutoFib, AverageDailyRange, AwesomeOscillator, AwesomeOscillatorHistogram, BalanceOfPower, Bat, BatchExt, BeltHold, BetterVolume, BodySizePct, Breakaway, Butterfly, Camarilla, Candle, CandleVolume, Cci, CentralPivotRange, ChaikinMoneyFlow, ChaikinOscillator, ChaikinVolatility, ChandeKrollStop, ChandelierExit, ChoppinessIndex, ClassicPivots, CloseVsOpen, ClosingMarubozu, CompositeProfile, ConcealingBabySwallow, Counterattack, Crab, CupAndHandle, Cypher, DayOfWeekProfile, DemandIndex, DemarkPivots, Doji, DojiStar, Donchian, DonchianStop, DoubleTopBottom, DownsideGapThreeMethods, DragonflyDoji, DumplingTop, Dx, EaseOfMovement, ElderRay, ElderSafeZone, Engulfing, Equivolume, EveningDojiStar, Evwma, FallingThreeMethods, FibArcs, FibChannel, FibConfluence, FibExtension, FibFan, FibProjection, FibRetracement, FibTimeZones, FibonacciPivots, FlagPennant, ForceIndex, FractalChaosBands, FryPanBottom, GapSideBySideWhite, GarmanKlassVolatility, Gartley, GatorOscillator, GoldenPocket, GravestoneDoji, Hammer, HangingMan, Harami, HaramiCross, HeadAndShoulders, HeikinAshi, HeikinAshiOscillator, HiLoActivator, HighLowRange, HighLowVolumeNodes, HighWave, Hikkake, HikkakeModified, HomingPigeon, HurstChannel, Ichimoku, IdenticalThreeCrows, InNeck, Indicator, Inertia, InitialBalance, IntradayIntensity, IntradayMomentumIndex, IntradayVolatilityProfile, InvertedHammer, KaseDevStop, KasePermissionStochastic, Keltner, Kicking, KickingByLength, Kvo, LadderBottom, LongLeggedDoji, LongLine, MarketFacilitationIndex, Marubozu, MassIndex, MatHold, MatchingLow, AvgPrice, MedianPrice, Mfi, MidPrice, MinusDi, MinusDm, ModifiedMaStop, MorningDojiStar, MorningEveningStar, MurreyMathLines, NakedPoc, Natr, NewPriceLines, Nrtr, Nvi, Obv, OnNeck, OpeningMarubozu, OpeningRange, OvernightGap, OvernightIntradayReturn, ParkinsonVolatility, Pgo, PiercingDarkCloud, PivotReversal, PlusDi, PlusDm, ProfileShape, ProjectionBands, ProjectionOscillator, Psar, Pvi, Qstick, RectangleRange, RickshawMan, RisingThreeMethods, RogersSatchellVolatility, RollingVwap, Rvi, Rwi, SarExt, SeasonalZScore, SeparatingLines, SessionHighLow, SessionRange, SessionVwap, Shark, ShootingStar, ShortLine, SinglePrints, Smi, SmoothedHeikinAshi, SpinningTop, StalledPattern, StarcBands, StickSandwich, Stochastic, StochasticCci, SuperTrend, Takuri, TasukiGap, TdCamouflage, TdClop, TdClopwin, TdCombo, TdCountdown, TdDWave, TdDeMarker, TdDifferential, TdLines, TdMovingAverage, TdOpen, TdPressure, TdPropulsion, TdRangeProjection, TdRei, TdRiskLevel, TdSequential, TdSetup, TdTrap, ThreeDrives, ThreeInside, ThreeLineBreak, ThreeLineStrike, ThreeOutside, ThreeSoldiersOrCrows, ThreeStarsInSouth, Thrusting, TimeBasedStop, TimeOfDayReturnProfile, TowerTopBottom, TpoProfile, TradeVolumeIndex, Triangle, TripleTopBottom, Tristar, TrueRange, Tsv, TtmSqueeze, TtmTrend, TurnOfMonth, Tweezer, TwiggsMoneyFlow, TwoCrows, TypicalPrice, UltimateOscillator, UniqueThreeRiver, UpsideGapThreeMethods, UpsideGapTwoCrows, ValueArea, VolatilityCone, VolatilityRatio, VoltyStop, VolumeByTimeProfile, VolumeOscillator, VolumePriceTrend, VolumeProfile, VolumeRsi, VolumeWeightedMacd, VolumeWeightedSr, Vortex, Vwap, VwapStdDevBands, Vwma, Vzo, WaveTrend, Wedge, WeightedClose, WickRatio, Wad, WilliamsFractals, WilliamsR, WoodiePivots, YangZhangVolatility, YoyoExit, ZigZag};

/// Convert a flat `f64` stream into a `Vec<Candle>` by chunking it into
/// `[open, high, low, close, volume]` groups. Tuples that fail OHLCV
/// validation are dropped so the indicator under test only ever sees a
/// structurally-valid candle stream (the *parser* is fuzz-tested elsewhere;
/// this target focuses on indicator robustness).
fn candles_from(data: &[f64]) -> Vec<Candle> {
    data.chunks_exact(5)
        .enumerate()
        .filter_map(|(i, ch)| {
            // A monotonic timestamp avoids surprising any indicator that might
            // care about ordering. The fuzz input drives OHLCV; time is just a
            // tie-breaker.
            Candle::new(ch[0], ch[1], ch[2], ch[3], ch[4], i as i64).ok()
        })
        .collect()
}

/// Streaming + batch sweep through one candle-input indicator. `#[inline(never)]`
/// keeps each indicator on its own frame in any panic backtrace.
#[inline(never)]
fn drive<I, O>(make: impl Fn() -> I, candles: &[Candle])
where
    I: Indicator<Input = Candle, Output = O> + BatchExt,
{
    let mut streaming = make();
    for c in candles {
        let _ = streaming.update(*c);
    }
    let _ = make().batch(candles);
}

fuzz_target!(|data: Vec<f64>| {
    let candles = candles_from(&data);
    if candles.is_empty() {
        return;
    }

    // --- Volatility & ATR family ---
    drive(|| VolatilityRatio::new(14).unwrap(), &candles);
    drive(|| Atr::new(14).unwrap(), &candles);
    drive(|| Natr::new(14).unwrap(), &candles);
    drive(TrueRange::new, &candles);
    drive(|| ChaikinVolatility::new(10, 10).unwrap(), &candles);
    drive(|| ParkinsonVolatility::new(20, 252).unwrap(), &candles);
    drive(|| GarmanKlassVolatility::new(20, 252).unwrap(), &candles);
    drive(|| RogersSatchellVolatility::new(20, 252).unwrap(), &candles);
    drive(|| YangZhangVolatility::new(20, 252).unwrap(), &candles);

    // --- Bands & Channels ---
    drive(|| ProjectionOscillator::new(14).unwrap(), &candles);
    drive(|| ProjectionBands::new(3).unwrap(), &candles);
    drive(|| Keltner::new(20, 10, 2.0).unwrap(), &candles);
    drive(|| Donchian::new(20).unwrap(), &candles);

    // --- Trailing Stops ---
    drive(|| ModifiedMaStop::new(14).unwrap(), &candles);
    drive(|| TimeBasedStop::new(5).unwrap(), &candles);
    drive(|| Nrtr::new(2.0).unwrap(), &candles);
    drive(|| AtrRatchet::new(14, 4.0, 0.1).unwrap(), &candles);
    drive(|| ElderSafeZone::new(14, 2.0).unwrap(), &candles);
    drive(|| Psar::new(0.02, 0.02, 0.20).unwrap(), &candles);
    drive(SarExt::classic, &candles);
    drive(|| SuperTrend::new(14, 3.0).unwrap(), &candles);
    drive(|| ChandelierExit::new(22, 3.0).unwrap(), &candles);
    drive(|| ChandeKrollStop::new(10, 1.0, 9).unwrap(), &candles);
    drive(|| AtrTrailingStop::new(14, 3.0).unwrap(), &candles);
    drive(|| HiLoActivator::new(3).unwrap(), &candles);
    drive(|| VoltyStop::new(14, 2.0).unwrap(), &candles);
    drive(|| YoyoExit::new(14, 2.0).unwrap(), &candles);
    drive(|| KaseDevStop::new(30, 1.0).unwrap(), &candles);

    // --- Trend & Directional ---
    drive(|| KasePermissionStochastic::new(9, 3).unwrap(), &candles);
    drive(|| GatorOscillator::new(13, 8, 5).unwrap(), &candles);
    drive(|| Qstick::new(10).unwrap(), &candles);
    drive(|| TtmTrend::new(6).unwrap(), &candles);
    drive(|| Adx::new(14).unwrap(), &candles);
    drive(|| Adxr::new(14).unwrap(), &candles);
    drive(|| PlusDm::new(14).unwrap(), &candles);
    drive(|| MinusDm::new(14).unwrap(), &candles);
    drive(|| PlusDi::new(14).unwrap(), &candles);
    drive(|| MinusDi::new(14).unwrap(), &candles);
    drive(|| Dx::new(14).unwrap(), &candles);
    drive(|| Aroon::new(14).unwrap(), &candles);
    drive(|| Alligator::new(13, 8, 5).unwrap(), &candles);
    drive(|| AroonOscillator::new(14).unwrap(), &candles);
    drive(|| Vortex::new(14).unwrap(), &candles);
    drive(|| Rwi::new(14).unwrap(), &candles);
    drive(|| WaveTrend::classic().unwrap(), &candles);
    drive(|| MassIndex::new(9, 25).unwrap(), &candles);
    drive(|| ChoppinessIndex::new(14).unwrap(), &candles);

    // --- Momentum & Oscillators ---
    drive(|| Cci::new(20).unwrap(), &candles);
    drive(|| AdaptiveCci::new(20).unwrap(), &candles);
    drive(|| StochasticCci::new(14).unwrap(), &candles);
    drive(|| ElderRay::new(13).unwrap(), &candles);
    drive(|| IntradayMomentumIndex::new(14).unwrap(), &candles);
    drive(|| Rvi::new(10).unwrap(), &candles);
    drive(|| Inertia::new(14, 20).unwrap(), &candles);
    drive(|| Pgo::new(14).unwrap(), &candles);
    drive(|| Smi::classic(), &candles);
    drive(|| WilliamsR::new(14).unwrap(), &candles);
    drive(|| AwesomeOscillator::new(5, 34).unwrap(), &candles);
    drive(
        || AwesomeOscillatorHistogram::new(5, 34, 5).unwrap(),
        &candles,
    );
    drive(|| AcceleratorOscillator::new(5, 34, 5).unwrap(), &candles);
    drive(|| UltimateOscillator::new(7, 14, 28).unwrap(), &candles);
    drive(BalanceOfPower::new, &candles);
    drive(CloseVsOpen::new, &candles);
    drive(BodySizePct::new, &candles);
    drive(WickRatio::new, &candles);
    drive(HighLowRange::new, &candles);

    // --- Volume ---
    drive(|| BetterVolume::new(14).unwrap(), &candles);
    drive(IntradayIntensity::new, &candles);
    drive(|| TradeVolumeIndex::new(0.25).unwrap(), &candles);
    drive(|| TwiggsMoneyFlow::new(21).unwrap(), &candles);
    drive(Wad::new, &candles);
    drive(|| VolumeRsi::new(14).unwrap(), &candles);
    drive(|| VolumeWeightedMacd::new(12, 26, 9).unwrap(), &candles);
    drive(Obv::new, &candles);
    drive(|| Mfi::new(14).unwrap(), &candles);
    drive(Vwap::new, &candles);
    drive(|| RollingVwap::new(20).unwrap(), &candles);
    drive(|| Vwma::new(20).unwrap(), &candles);
    drive(|| Evwma::new(20).unwrap(), &candles);
    drive(Adl::new, &candles);
    drive(VolumePriceTrend::new, &candles);
    drive(|| ChaikinMoneyFlow::new(20).unwrap(), &candles);
    drive(|| ChaikinOscillator::new(3, 10).unwrap(), &candles);
    drive(|| ForceIndex::new(13).unwrap(), &candles);
    drive(|| EaseOfMovement::with_divisor(14, 1e8).unwrap(), &candles);
    drive(|| Kvo::new(34, 55).unwrap(), &candles);
    drive(|| VolumeOscillator::new(14, 28).unwrap(), &candles);
    drive(Nvi::new, &candles);
    drive(Pvi::new, &candles);
    drive(AdOscillator::new, &candles);
    drive(AnchoredVwap::new, &candles);
    drive(|| DemandIndex::new(10).unwrap(), &candles);
    drive(|| Tsv::new(18).unwrap(), &candles);
    drive(|| Vzo::new(14).unwrap(), &candles);
    drive(MarketFacilitationIndex::new, &candles);

    // --- Price transformations ---
    drive(TypicalPrice::new, &candles);
    drive(MedianPrice::new, &candles);
    drive(WeightedClose::new, &candles);
    drive(|| MidPrice::new(14).unwrap(), &candles);
    drive(AvgPrice::new, &candles);

    // --- Stochastic (multi-output) ---
    {
        let mut s = Stochastic::new(14, 3).unwrap();
        for c in &candles {
            let _ = s.update(*c);
        }
        let _ = Stochastic::new(14, 3).unwrap().batch(&candles);
    }

    // --- Market Profile (multi-output) ---
    drive(|| CompositeProfile::new(20, 24, 0.7).unwrap(), &candles);
    drive(|| HighLowVolumeNodes::new(20, 24).unwrap(), &candles);
    drive(|| NakedPoc::new(20, 24).unwrap(), &candles);
    drive(|| SinglePrints::new(20, 24).unwrap(), &candles);
    drive(|| ProfileShape::new(20, 24).unwrap(), &candles);
    drive(|| ValueArea::new(20, 50, 0.70).unwrap(), &candles);
    drive(|| VolumeProfile::new(20, 50).unwrap(), &candles);
    drive(|| TpoProfile::new(20, 50).unwrap(), &candles);
    drive(|| InitialBalance::new(12).unwrap(), &candles);
    drive(|| OpeningRange::new(6).unwrap(), &candles);

    // --- Ichimoku (5 lines, hand-rolled because of multi-Option output) ---
    {
        let mut ichi = Ichimoku::classic();
        for c in &candles {
            let _ = ichi.update(*c);
        }
        let _ = Ichimoku::classic().batch(&candles);
    }

    // --- Heikin-Ashi (4-field candle transform) ---
    {
        let mut ha = HeikinAshi::new();
        for c in &candles {
            let _ = ha.update(*c);
        }
        let _ = HeikinAshi::new().batch(&candles);
    }

    // --- DeMark family ---
    drive(|| TdSetup::new(4, 9).unwrap(), &candles);
    drive(|| TdDeMarker::new(14).unwrap(), &candles);
    drive(|| TdRei::new(5).unwrap(), &candles);
    drive(|| TdPressure::new(5).unwrap(), &candles);
    drive(|| TdCombo::new(4, 9, 2, 13).unwrap(), &candles);
    drive(|| TdCountdown::new(4, 9, 2, 13).unwrap(), &candles);
    drive(TdDifferential::new, &candles);
    drive(TdOpen::new, &candles);
    drive(TdRangeProjection::new, &candles);
    {
        let mut s = TdSequential::new(4, 9, 2, 13).unwrap();
        for c in &candles {
            let _ = s.update(*c);
        }
        let _ = TdSequential::new(4, 9, 2, 13).unwrap().batch(&candles);
    }
    {
        let mut s = TdLines::new(4, 9).unwrap();
        for c in &candles {
            let _ = s.update(*c);
        }
        let _ = TdLines::new(4, 9).unwrap().batch(&candles);
    }
    {
        let mut s = TdRiskLevel::new(4, 9).unwrap();
        for c in &candles {
            let _ = s.update(*c);
        }
        let _ = TdRiskLevel::new(4, 9).unwrap().batch(&candles);
    }

    // --- Pivots & Support/Resistance (multi-output) ---
    drive(ClassicPivots::new, &candles);
    drive(FibonacciPivots::new, &candles);
    drive(Camarilla::new, &candles);
    drive(WoodiePivots::new, &candles);
    drive(DemarkPivots::new, &candles);
    drive(WilliamsFractals::new, &candles);
    drive(|| ZigZag::new(0.05).unwrap(), &candles);

    // --- Donchian Stop (multi-output) ---
    {
        let mut s = DonchianStop::new(10).unwrap();
        for c in &candles {
            let _ = s.update(*c);
        }
        let _ = DonchianStop::new(10).unwrap().batch(&candles);
    }

    // --- Family 05: candle-input band/channel indicators (multi-output) ---
    drive(|| VolatilityCone::new(20, 60).unwrap(), &candles);
    {
        let mut ab = AccelerationBands::new(20, 4.0).unwrap();
        for c in &candles {
            let _ = ab.update(*c);
        }
        let _ = AccelerationBands::new(20, 4.0).unwrap().batch(&candles);
    }
    {
        let mut sb = StarcBands::new(6, 15, 2.0).unwrap();
        for c in &candles {
            let _ = sb.update(*c);
        }
        let _ = StarcBands::new(6, 15, 2.0).unwrap().batch(&candles);
    }
    {
        let mut atrb = AtrBands::new(14, 3.0).unwrap();
        for c in &candles {
            let _ = atrb.update(*c);
        }
        let _ = AtrBands::new(14, 3.0).unwrap().batch(&candles);
    }
    {
        let mut hc = HurstChannel::new(10, 0.5).unwrap();
        for c in &candles {
            let _ = hc.update(*c);
        }
        let _ = HurstChannel::new(10, 0.5).unwrap().batch(&candles);
    }
    {
        let mut ts = TtmSqueeze::new(20, 2.0, 1.5).unwrap();
        for c in &candles {
            let _ = ts.update(*c);
        }
        let _ = TtmSqueeze::new(20, 2.0, 1.5).unwrap().batch(&candles);
    }
    {
        let mut fc = FractalChaosBands::new(2).unwrap();
        for c in &candles {
            let _ = fc.update(*c);
        }
        let _ = FractalChaosBands::new(2).unwrap().batch(&candles);
    }
    {
        let mut vb = VwapStdDevBands::new(2.0).unwrap();
        for c in &candles {
            let _ = vb.update(*c);
        }
        let _ = VwapStdDevBands::new(2.0).unwrap().batch(&candles);
    }

    // --- Candlestick Patterns (family 14) ---
    drive(TowerTopBottom::new, &candles);
    drive(HaramiCross::new, &candles);
    drive(Tristar::new, &candles);
    drive(|| FryPanBottom::new(9).unwrap(), &candles);
    drive(|| DumplingTop::new(9).unwrap(), &candles);
    drive(|| NewPriceLines::new(5).unwrap(), &candles);
    drive(TdTrap::new, &candles);
    drive(TdPropulsion::new, &candles);
    drive(TdClopwin::new, &candles);
    drive(TdClop::new, &candles);
    drive(TdCamouflage::new, &candles);
    drive(|| TdDWave::new(2).unwrap(), &candles);
    drive(|| TdMovingAverage::new(5, 13).unwrap(), &candles);

    // --- Ichimoku & Charts ---
    drive(|| HeikinAshiOscillator::new(5).unwrap(), &candles);
    drive(|| ThreeLineBreak::new(3).unwrap(), &candles);
    drive(|| SmoothedHeikinAshi::new(5).unwrap(), &candles);
    drive(|| Equivolume::new(20).unwrap(), &candles);
    drive(|| CandleVolume::new(20).unwrap(), &candles);
    drive(ConcealingBabySwallow::new, &candles);
    drive(UniqueThreeRiver::new, &candles);
    drive(TasukiGap::new, &candles);
    drive(OpeningMarubozu::new, &candles);
    drive(ClosingMarubozu::new, &candles);
    drive(Takuri::new, &candles);
    drive(StickSandwich::new, &candles);
    drive(StalledPattern::new, &candles);
    drive(DownsideGapThreeMethods::new, &candles);
    drive(UpsideGapThreeMethods::new, &candles);
    drive(FallingThreeMethods::new, &candles);
    drive(RisingThreeMethods::new, &candles);
    drive(ShortLine::new, &candles);
    drive(LongLine::new, &candles);
    drive(MatchingLow::new, &candles);
    drive(MatHold::new, &candles);
    drive(LadderBottom::new, &candles);
    drive(KickingByLength::new, &candles);
    drive(Kicking::new, &candles);
    drive(SeparatingLines::new, &candles);
    drive(Thrusting::new, &candles);
    drive(InNeck::new, &candles);
    drive(OnNeck::new, &candles);
    drive(HomingPigeon::new, &candles);
    drive(HikkakeModified::new, &candles);
    drive(Hikkake::new, &candles);
    drive(HighWave::new, &candles);
    drive(GapSideBySideWhite::new, &candles);
    drive(MorningDojiStar::new, &candles);
    drive(EveningDojiStar::new, &candles);
    drive(RickshawMan::new, &candles);
    drive(LongLeggedDoji::new, &candles);
    drive(GravestoneDoji::new, &candles);
    drive(DragonflyDoji::new, &candles);
    drive(DojiStar::new, &candles);
    drive(Counterattack::new, &candles);
    drive(Breakaway::new, &candles);
    drive(BeltHold::new, &candles);
    drive(AbandonedBaby::new, &candles);
    drive(AdvanceBlock::new, &candles);
    drive(Doji::new, &candles);
    drive(|| Doji::new().signed(), &candles);
    drive(Hammer::new, &candles);
    drive(InvertedHammer::new, &candles);
    drive(HangingMan::new, &candles);
    drive(ShootingStar::new, &candles);
    drive(Engulfing::new, &candles);
    drive(Harami::new, &candles);
    drive(MorningEveningStar::new, &candles);
    drive(ThreeSoldiersOrCrows::new, &candles);
    drive(ThreeStarsInSouth::new, &candles);
    drive(PiercingDarkCloud::new, &candles);
    drive(Marubozu::new, &candles);
    drive(Tweezer::new, &candles);
    drive(SpinningTop::new, &candles);
    drive(ThreeInside::new, &candles);
    drive(ThreeLineStrike::new, &candles);
    drive(ThreeOutside::new, &candles);
    drive(TwoCrows::new, &candles);
    drive(UpsideGapTwoCrows::new, &candles);
    drive(IdenticalThreeCrows::new, &candles);
    // --- Seasonality & Session ---
    drive(|| VolumeByTimeProfile::new(24, 0).unwrap(), &candles);

    drive(|| IntradayVolatilityProfile::new(24, 0).unwrap(), &candles);

    drive(|| DayOfWeekProfile::new(0), &candles);

    drive(|| TimeOfDayReturnProfile::new(24, 0).unwrap(), &candles);

    drive(|| SeasonalZScore::new(0), &candles);

    drive(|| TurnOfMonth::new(3, 1, 0).unwrap(), &candles);

    drive(|| OvernightIntradayReturn::new(0), &candles);

    drive(|| OvernightGap::new(0), &candles);

    drive(|| AverageDailyRange::new(14, 0).unwrap(), &candles);

    drive(|| SessionRange::new(0), &candles);

    drive(|| SessionHighLow::new(0), &candles);

    drive(|| SessionVwap::new(0), &candles);

    // --- Chart Patterns ---
    drive(CupAndHandle::new, &candles);
    drive(RectangleRange::new, &candles);
    drive(FlagPennant::new, &candles);
    drive(Wedge::new, &candles);
    drive(Triangle::new, &candles);
    drive(HeadAndShoulders::new, &candles);
    drive(TripleTopBottom::new, &candles);
    drive(DoubleTopBottom::new, &candles);

    // --- Harmonic Patterns ---
    drive(ThreeDrives::new, &candles);
    drive(Cypher::new, &candles);
    drive(Shark::new, &candles);
    drive(Crab::new, &candles);
    drive(Bat::new, &candles);
    drive(Butterfly::new, &candles);
    drive(Gartley::new, &candles);
    drive(Abcd::new, &candles);

    // --- Fibonacci (multi-output) ---
    drive(FibTimeZones::new, &candles);
    drive(FibChannel::new, &candles);
    drive(FibArcs::new, &candles);
    drive(FibFan::new, &candles);
    drive(FibConfluence::new, &candles);
    drive(GoldenPocket::new, &candles);
    drive(AutoFib::new, &candles);
    drive(FibProjection::new, &candles);
    drive(FibExtension::new, &candles);
    drive(FibRetracement::new, &candles);

    // --- Pivots & S/R ---
    drive(CentralPivotRange::new, &candles);
    drive(|| MurreyMathLines::new(4).unwrap(), &candles);
    drive(|| AndrewsPitchfork::new(2).unwrap(), &candles);
    drive(|| VolumeWeightedSr::new(3).unwrap(), &candles);
    drive(|| PivotReversal::new(1, 1).unwrap(), &candles);

});
