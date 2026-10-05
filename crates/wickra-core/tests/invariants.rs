//! Property-based invariants every indicator must uphold.
//!
//! Three properties are checked against random input sequences, rolled out over
//! the whole catalogue so a regression in any single indicator surfaces here:
//!
//! 1. **batch == streaming** — `batch()` must replay `update()` exactly.
//! 2. **reset == fresh** — after `reset()`, re-feeding the same data must match
//!    a freshly constructed instance.
//! 3. **non-finite is rejected without poisoning** — a NaN/inf tick returns
//!    `None` and leaves state identical to never having seen it. `Candle` and the
//!    other validated input types cannot be non-finite, so this applies to the
//!    `f64` and `(f64, f64)` families. This is the regression net that would have
//!    caught the pairwise non-finite bug (#251).

use proptest::prelude::*;
use wickra_core::*;

// --- generic invariant bodies ------------------------------------------------

// One generic body for every `Indicator` family. Inputs are compared via Debug
// so bit-identical NaN outputs count as equal (NaN != NaN would otherwise break
// equality for indicators that emit NaN on degenerate input — these properties
// are about determinism, not NaN-freeness).
fn check_seq<I>(make: impl Fn() -> I, xs: &[I::Input]) -> std::result::Result<(), TestCaseError>
where
    I: Indicator,
    I::Input: Clone,
    I::Output: std::fmt::Debug,
{
    let batch = make().batch(xs);
    let mut s = make();
    let stream: Vec<_> = xs.iter().map(|x| s.update(x.clone())).collect();
    prop_assert_eq!(
        format!("{batch:?}"),
        format!("{stream:?}"),
        "batch != streaming"
    );

    let mut a = make();
    let _ = a.batch(xs);
    a.reset();
    let after: Vec<_> = xs.iter().map(|x| a.update(x.clone())).collect();
    prop_assert_eq!(
        format!("{after:?}"),
        format!("{stream:?}"),
        "reset != fresh"
    );
    Ok(())
}

// BarBuilder has a different shape (`update(Candle) -> Vec<Bar>`), so it gets its
// own body. `batch` concatenates per-candle bars; streaming flat-maps them.
fn check_bars<B>(make: impl Fn() -> B, candles: &[Candle]) -> std::result::Result<(), TestCaseError>
where
    B: BarBuilder,
    B::Bar: std::fmt::Debug,
{
    let batch = make().batch(candles);
    let mut s = make();
    let stream: Vec<_> = candles.iter().flat_map(|&c| s.update(c)).collect();
    prop_assert_eq!(
        format!("{batch:?}"),
        format!("{stream:?}"),
        "batch != streaming"
    );

    let mut a = make();
    let _ = a.batch(candles);
    a.reset();
    let after: Vec<_> = candles.iter().flat_map(|&c| a.update(c)).collect();
    prop_assert_eq!(
        format!("{after:?}"),
        format!("{stream:?}"),
        "reset != fresh"
    );
    Ok(())
}

// Non-finite rejection for the `f64` family: a NaN/inf tick returns `None` and
// does not poison state (the poisoned run must match a clean one). Only applies
// to scalar/pairwise inputs — `Candle` and the exotic types validate finiteness
// at construction.
fn check_scalar_nonfinite<I>(
    make: impl Fn() -> I,
    xs: &[f64],
) -> std::result::Result<(), TestCaseError>
where
    I: Indicator<Input = f64>,
    I::Output: std::fmt::Debug,
{
    const BAD: [f64; 3] = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY];

    let mut s = make();
    let clean: Vec<_> = xs.iter().map(|&x| s.update(x)).collect();

    // Injected before any real input: the state has nothing to lose yet.
    let mut g = make();
    for bad in BAD {
        prop_assert!(g.update(bad).is_none(), "{} not rejected", bad);
    }
    let poisoned: Vec<_> = xs.iter().map(|&x| g.update(x)).collect();
    prop_assert_eq!(
        format!("{poisoned:?}"),
        format!("{clean:?}"),
        "non-finite poisoned state"
    );

    // Injected mid-stream, after the indicator has warmed up and has a value to
    // hand back. This is the case that went untested, and the one where
    // returning a cached value instead of `None` diverges.
    let mut g = make();
    let mut interleaved = Vec::with_capacity(xs.len());
    for (i, &x) in xs.iter().enumerate() {
        interleaved.push(g.update(x));
        for bad in BAD {
            prop_assert!(
                g.update(bad).is_none(),
                "{} accepted after {} inputs",
                bad,
                i + 1
            );
        }
    }
    prop_assert_eq!(
        format!("{interleaved:?}"),
        format!("{clean:?}"),
        "a rejected input changed the values around it"
    );
    Ok(())
}

// Non-finite rejection for the `(f64, f64)` family.
fn check_pairwise_nonfinite<I>(
    make: impl Fn() -> I,
    xs: &[(f64, f64)],
) -> std::result::Result<(), TestCaseError>
where
    I: Indicator<Input = (f64, f64)>,
    I::Output: std::fmt::Debug,
{
    const BAD: [(f64, f64); 4] = [
        (f64::NAN, 1.0),
        (1.0, f64::NAN),
        (f64::INFINITY, 1.0),
        (1.0, f64::NEG_INFINITY),
    ];

    let mut s = make();
    let clean: Vec<_> = xs.iter().map(|&p| s.update(p)).collect();

    let mut g = make();
    for bad in BAD {
        prop_assert!(g.update(bad).is_none(), "{:?} not rejected", bad);
    }
    let poisoned: Vec<_> = xs.iter().map(|&p| g.update(p)).collect();
    prop_assert_eq!(
        format!("{poisoned:?}"),
        format!("{clean:?}"),
        "non-finite poisoned state"
    );

    // The same injection after warmup, where a cached-value policy would show.
    let mut g = make();
    let mut interleaved = Vec::with_capacity(xs.len());
    for (i, &p) in xs.iter().enumerate() {
        interleaved.push(g.update(p));
        for bad in BAD {
            prop_assert!(
                g.update(bad).is_none(),
                "{:?} accepted after {} inputs",
                bad,
                i + 1
            );
        }
    }
    prop_assert_eq!(
        format!("{interleaved:?}"),
        format!("{clean:?}"),
        "a rejected input changed the values around it"
    );
    Ok(())
}

/// `is_ready` must mean exactly what the trait says it means: at least one value
/// emitted since the last reset. Nothing checked this, and four indicators keyed
/// it off something else — a full window, a seeded bootstrap state, or every
/// output component being present — each of which changes at a different moment
/// than the first emission.
fn check_ready<I>(make: impl Fn() -> I, xs: &[I::Input]) -> std::result::Result<(), TestCaseError>
where
    I: Indicator,
    I::Input: Clone,
{
    let mut ind = make();
    prop_assert!(!ind.is_ready(), "ready before any input");
    let mut emitted = false;
    for (i, x) in xs.iter().enumerate() {
        emitted |= ind.update(x.clone()).is_some();
        prop_assert_eq!(
            ind.is_ready(),
            emitted,
            "is_ready disagrees with having emitted, at input {}",
            i
        );
    }
    ind.reset();
    prop_assert!(!ind.is_ready(), "ready after reset");
    Ok(())
}

fn check_warmup<I>(make: impl Fn() -> I, xs: &[I::Input]) -> std::result::Result<(), TestCaseError>
where
    I: Indicator,
    I::Input: Clone,
{
    let mut ind = make();
    let declared = ind.warmup_period();
    for (i, x) in xs.iter().enumerate() {
        let emitted = ind.update(x.clone()).is_some();
        // The trait defines `warmup_period` as the number of inputs required
        // before the first non-`None` output, so emitting earlier than that is
        // a broken promise. Emitting LATER is not: several indicators declare a
        // bound that is correct for their intended bar size and only look late
        // on a probe series of a different one, so only this direction is
        // asserted.
        prop_assert!(
            !emitted || i + 1 >= declared,
            "emitted at input {} but declares a warmup of {}",
            i + 1,
            declared
        );
    }
    Ok(())
}

// --- per-family roll-out macros ----------------------------------------------

macro_rules! scalar_inv {
    ($name:ident, $($ctor:tt)+) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(24))]
            #[test]
            fn $name(xs in prop::collection::vec(-1.0e6f64..1.0e6, 0..160)) {
                check_seq(|| { $($ctor)+ }, &xs)?;
                check_scalar_nonfinite(|| { $($ctor)+ }, &xs)?;
                check_ready(|| { $($ctor)+ }, &xs)?;
                check_warmup(|| { $($ctor)+ }, &xs)?;
            }
        }
    };
}

macro_rules! candle_inv {
    ($name:ident, $($ctor:tt)+) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(24))]
            #[test]
            fn $name(
                raw in prop::collection::vec(
                    (1.0e1f64..1.0e4, 0.0f64..200.0, 0.0f64..1.0, 0.0f64..1.0),
                    0..160,
                )
            ) {
                // (low, range, open_frac, close_frac) -> a valid OHLC candle.
                let candles: Vec<Candle> = raw
                    .iter()
                    .map(|&(low, range, of, cf)| {
                        Candle::new(low + range * of, low + range, low, low + range * cf, 1.0, 0)
                            .expect("constructed valid candle")
                    })
                    .collect();
                check_seq(|| { $($ctor)+ }, &candles)?;
                check_ready(|| { $($ctor)+ }, &candles)?;
                check_warmup(|| { $($ctor)+ }, &candles)?;
            }
        }
    };
}

macro_rules! pair_inv {
    ($name:ident, $($ctor:tt)+) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(24))]
            #[test]
            fn $name(
                xs in prop::collection::vec((-1.0e6f64..1.0e6, -1.0e6f64..1.0e6), 0..160)
            ) {
                check_seq(|| { $($ctor)+ }, &xs)?;
                check_ready(|| { $($ctor)+ }, &xs)?;
                check_warmup(|| { $($ctor)+ }, &xs)?;
                check_pairwise_nonfinite(|| { $($ctor)+ }, &xs)?;
            }
        }
    };
}

macro_rules! cross_section_inv {
    ($name:ident, $($ctor:tt)+) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(16))]
            #[test]
            fn $name(
                raw in prop::collection::vec(
                    prop::collection::vec(
                        (-1.0e3f64..1.0e3, 0.0f64..1.0e6, any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()),
                        4..=4,
                    ),
                    0..80,
                )
            ) {
                let ticks: Vec<CrossSection> = raw
                    .iter()
                    .map(|members| {
                        let m = members
                            .iter()
                            .map(|&(c, v, nh, nl, a, o)| Member::with_signals(c, v, nh, nl, a, o))
                            .collect();
                        CrossSection::new(m, 0).expect("valid cross-section")
                    })
                    .collect();
                check_seq(|| { $($ctor)+ }, &ticks)?;
                check_ready(|| { $($ctor)+ }, &ticks)?;
                check_warmup(|| { $($ctor)+ }, &ticks)?;
            }
        }
    };
}

macro_rules! trade_inv {
    ($name:ident, $($ctor:tt)+) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(24))]
            #[test]
            fn $name(raw in prop::collection::vec((1.0f64..1.0e5, 0.0f64..1.0e4, any::<bool>()), 0..160)) {
                let ticks: Vec<Trade> = raw
                    .iter()
                    .map(|&(p, s, b)| Trade::new(p, s, if b { Side::Buy } else { Side::Sell }, 0).expect("valid trade"))
                    .collect();
                check_seq(|| { $($ctor)+ }, &ticks)?;
                check_ready(|| { $($ctor)+ }, &ticks)?;
                check_warmup(|| { $($ctor)+ }, &ticks)?;
            }
        }
    };
}

macro_rules! deriv_inv {
    ($name:ident, $($ctor:tt)+) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(16))]
            #[test]
            fn $name(
                raw in prop::collection::vec(
                    (
                        (-1.0f64..1.0, 1.0f64..1.0e5, 1.0f64..1.0e5, 1.0f64..1.0e5),
                        (0.0f64..1.0e6, 0.0f64..1.0e6, 0.0f64..1.0e6, 0.0f64..1.0e6),
                        (0.0f64..1.0e6, 0.0f64..1.0e6, 0.0f64..1.0e6),
                    ),
                    0..120,
                )
            ) {
                let ticks: Vec<DerivativesTick> = raw
                    .iter()
                    .map(|&((fr, mk, ix, fu), (oi, ls, ss, tbv), (tsv, ll, sl))| {
                        DerivativesTick::new(fr, mk, ix, fu, oi, ls, ss, tbv, tsv, ll, sl, 0).expect("valid deriv tick")
                    })
                    .collect();
                check_seq(|| { $($ctor)+ }, &ticks)?;
                check_ready(|| { $($ctor)+ }, &ticks)?;
                check_warmup(|| { $($ctor)+ }, &ticks)?;
            }
        }
    };
}

macro_rules! orderbook_inv {
    ($name:ident, $($ctor:tt)+) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(16))]
            #[test]
            fn $name(raw in prop::collection::vec((1.0e2f64..1.0e4, 0.1f64..10.0, 0.0f64..1.0e4), 0..100)) {
                // mid +/- k*gap gives strictly-monotonic, uncrossed books (best_bid < best_ask).
                let books: Vec<OrderBook> = raw
                    .iter()
                    .map(|&(mid, gap, sz)| {
                        let asks = (1..=3).map(|k| Level::new(mid + f64::from(k) * gap, sz).expect("ask")).collect();
                        let bids = (1..=3).map(|k| Level::new(mid - f64::from(k) * gap, sz).expect("bid")).collect();
                        OrderBook::new(bids, asks).expect("valid book")
                    })
                    .collect();
                check_seq(|| { $($ctor)+ }, &books)?;
                check_ready(|| { $($ctor)+ }, &books)?;
                check_warmup(|| { $($ctor)+ }, &books)?;
            }
        }
    };
}

macro_rules! tradequote_inv {
    ($name:ident, $($ctor:tt)+) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(24))]
            #[test]
            fn $name(raw in prop::collection::vec((1.0f64..1.0e5, 0.0f64..1.0e4, any::<bool>(), 1.0f64..1.0e5), 0..160)) {
                let tqs: Vec<TradeQuote> = raw
                    .iter()
                    .map(|&(p, s, b, mid)| {
                        let tr = Trade::new(p, s, if b { Side::Buy } else { Side::Sell }, 0).expect("trade");
                        TradeQuote::new(tr, mid).expect("valid trade-quote")
                    })
                    .collect();
                check_seq(|| { $($ctor)+ }, &tqs)?;
                check_ready(|| { $($ctor)+ }, &tqs)?;
                check_warmup(|| { $($ctor)+ }, &tqs)?;
            }
        }
    };
}

macro_rules! bar_inv {
    ($name:ident, $($ctor:tt)+) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(24))]
            #[test]
            fn $name(
                raw in prop::collection::vec(
                    (1.0e1f64..1.0e4, 0.0f64..200.0, 0.0f64..1.0, 0.0f64..1.0),
                    0..160,
                )
            ) {
                let candles: Vec<Candle> = raw
                    .iter()
                    .map(|&(low, range, of, cf)| {
                        Candle::new(low + range * of, low + range, low, low + range * cf, 1.0, 0)
                            .expect("constructed valid candle")
                    })
                    .collect();
                check_bars(|| { $($ctor)+ }, &candles)?;
            }
        }
    };
}

// --- catalogue: full roll-out over the whole catalogue (513 of 514 mod entries:
//     503 Indicator + 10 BarBuilder; pattern_swing is neither, see below). ---

// --- scalar (167) ---
scalar_inv!(inv_adaptive_cycle, AdaptiveCycle::new());
scalar_inv!(
    inv_adaptive_laguerre_filter,
    AdaptiveLaguerreFilter::new(13).unwrap()
);
scalar_inv!(inv_adaptive_rsi, AdaptiveRsi::new(14).unwrap());
scalar_inv!(inv_alma, Alma::new(9, 0.85, 6.0).unwrap());
scalar_inv!(inv_anchored_rsi, AnchoredRsi::new());
scalar_inv!(inv_apo, Apo::new(12, 26).unwrap());
scalar_inv!(inv_autocorrelation, Autocorrelation::new(20, 1).unwrap());
scalar_inv!(
    inv_autocorrelation_periodogram,
    AutocorrelationPeriodogram::new(10, 48).unwrap()
);
scalar_inv!(inv_average_drawdown, AverageDrawdown::new(10).unwrap());
scalar_inv!(inv_bandpass_filter, BandpassFilter::new(20, 0.3).unwrap());
scalar_inv!(inv_bipower_variation, BipowerVariation::new(20).unwrap());
scalar_inv!(inv_bollinger, BollingerBands::new(5, 2.0).unwrap());
scalar_inv!(
    inv_bollinger_bandwidth,
    BollingerBandwidth::new(20, 2.0).unwrap()
);
scalar_inv!(inv_bomar_bands, BomarBands::new(20, 0.85).unwrap());
scalar_inv!(inv_burke_ratio, BurkeRatio::new(12).unwrap());
scalar_inv!(inv_calmar_ratio, CalmarRatio::new(20).unwrap());
scalar_inv!(inv_center_of_gravity, CenterOfGravity::new(10).unwrap());
scalar_inv!(inv_cfo, Cfo::new(14).unwrap());
scalar_inv!(inv_cmo, Cmo::new(14).unwrap());
scalar_inv!(
    inv_coefficient_of_variation,
    CoefficientOfVariation::new(20).unwrap()
);
scalar_inv!(inv_common_sense_ratio, CommonSenseRatio::new(20).unwrap());
scalar_inv!(
    inv_conditional_value_at_risk,
    ConditionalValueAtRisk::new(100, 0.95).unwrap()
);
scalar_inv!(inv_connors_rsi, ConnorsRsi::new(3, 2, 5).unwrap());
scalar_inv!(inv_coppock, Coppock::new(14, 11, 10).unwrap());
scalar_inv!(
    inv_correlation_trend_indicator,
    CorrelationTrendIndicator::new(20).unwrap()
);
scalar_inv!(inv_cybernetic_cycle, CyberneticCycle::new(10).unwrap());
scalar_inv!(inv_decycler, Decycler::new(20).unwrap());
scalar_inv!(
    inv_decycler_oscillator,
    DecyclerOscillator::new(10, 30).unwrap()
);
scalar_inv!(inv_dema, Dema::new(3).unwrap());
scalar_inv!(
    inv_derivative_oscillator,
    DerivativeOscillator::new(14, 5, 3, 9).unwrap()
);
scalar_inv!(inv_detrended_std_dev, DetrendedStdDev::new(14).unwrap());
scalar_inv!(inv_disparity_index, DisparityIndex::new(14).unwrap());
scalar_inv!(
    inv_double_bollinger,
    DoubleBollinger::new(20, 1.0, 2.0).unwrap()
);
scalar_inv!(inv_dpo, Dpo::new(20).unwrap());
scalar_inv!(inv_drawdown_duration, DrawdownDuration::new());
scalar_inv!(
    inv_dynamic_momentum_index,
    DynamicMomentumIndex::new(14).unwrap()
);
scalar_inv!(inv_ehlers_stochastic, EhlersStochastic::new(20).unwrap());
scalar_inv!(inv_ehma, Ehma::new(9).unwrap());
scalar_inv!(inv_elder_impulse, ElderImpulse::new(3, 2, 4, 3).unwrap());
scalar_inv!(inv_ema, Ema::new(3).unwrap());
scalar_inv!(
    inv_empirical_mode_decomposition,
    EmpiricalModeDecomposition::new(20, 0.5).unwrap()
);
scalar_inv!(
    inv_even_better_sinewave,
    EvenBetterSinewave::new(40, 10).unwrap()
);
scalar_inv!(inv_ewma_volatility, EwmaVolatility::new(0.94).unwrap());
scalar_inv!(inv_expectancy, Expectancy::new(4).unwrap());
scalar_inv!(inv_fama, Fama::new(0.5, 0.05).unwrap());
scalar_inv!(inv_fisher_rsi, FisherRsi::new(9).unwrap());
scalar_inv!(inv_fisher_transform, FisherTransform::new(10).unwrap());
scalar_inv!(inv_frama, Frama::new(16).unwrap());
scalar_inv!(inv_gain_loss_ratio, GainLossRatio::new(10).unwrap());
scalar_inv!(inv_gain_to_pain_ratio, GainToPainRatio::new(12).unwrap());
scalar_inv!(inv_garch11, Garch11::new(0.000_002, 0.10, 0.88).unwrap());
scalar_inv!(inv_generalized_dema, GeneralizedDema::new(5, 0.7).unwrap());
scalar_inv!(inv_geometric_ma, GeometricMa::new(5).unwrap());
scalar_inv!(inv_highpass_filter, HighpassFilter::new(48).unwrap());
scalar_inv!(inv_hilbert_dominant_cycle, HilbertDominantCycle::new());
scalar_inv!(
    inv_historical_volatility,
    HistoricalVolatility::new(20, 252).unwrap()
);
scalar_inv!(inv_hma, Hma::new(9).unwrap());
scalar_inv!(inv_holt_winters, HoltWinters::new(0.2, 0.1).unwrap());
scalar_inv!(inv_ht_dcphase, HtDcPhase::new());
scalar_inv!(inv_ht_phasor, HtPhasor::new());
scalar_inv!(inv_ht_trendmode, HtTrendMode::new());
scalar_inv!(inv_hurst_exponent, HurstExponent::new(100, 4).unwrap());
scalar_inv!(
    inv_instantaneous_trendline,
    InstantaneousTrendline::new(20).unwrap()
);
scalar_inv!(
    inv_inverse_fisher_transform,
    InverseFisherTransform::new(1.0).unwrap()
);
scalar_inv!(inv_jarque_bera, JarqueBera::new(50).unwrap());
scalar_inv!(inv_jma, Jma::new(14, 0.0, 2).unwrap());
scalar_inv!(inv_jump_indicator, JumpIndicator::new(20, 3.0).unwrap());
scalar_inv!(inv_k_ratio, KRatio::new(30).unwrap());
scalar_inv!(inv_kama, Kama::new(10, 2, 30).unwrap());
scalar_inv!(inv_kelly_criterion, KellyCriterion::new(10).unwrap());
scalar_inv!(inv_kst, Kst::new(2, 3, 4, 5, 2, 2, 2, 3, 2).unwrap());
scalar_inv!(inv_kurtosis, Kurtosis::new(20).unwrap());
scalar_inv!(inv_laguerre_rsi, LaguerreRsi::new(0.5).unwrap());
scalar_inv!(inv_linreg, LinearRegression::new(14).unwrap());
scalar_inv!(inv_linreg_angle, LinRegAngle::new(14).unwrap());
scalar_inv!(inv_linreg_channel, LinRegChannel::new(20, 2.0).unwrap());
scalar_inv!(inv_linreg_intercept, LinRegIntercept::new(14).unwrap());
scalar_inv!(inv_linreg_slope, LinRegSlope::new(14).unwrap());
scalar_inv!(inv_log_return, LogReturn::new(1).unwrap());
scalar_inv!(inv_m2_measure, M2Measure::new(20, 0.0, 0.02).unwrap());
scalar_inv!(inv_ma_envelope, MaEnvelope::new(20, 0.025).unwrap());
scalar_inv!(inv_macd, MacdIndicator::new(3, 6, 3).unwrap());
scalar_inv!(
    inv_macd_ext,
    MacdExt::new(12, MaType::Ema, 26, MaType::Ema, 9, MaType::Sma).unwrap()
);
scalar_inv!(inv_macd_fix, MacdFix::new(9).unwrap());
scalar_inv!(inv_macd_histogram, MacdHistogram::new(12, 26, 9).unwrap());
scalar_inv!(inv_mama, Mama::new(0.5, 0.05).unwrap());
scalar_inv!(inv_martin_ratio, MartinRatio::new(14).unwrap());
scalar_inv!(inv_max_drawdown, MaxDrawdown::new(10).unwrap());
scalar_inv!(inv_mcginley_dynamic, McGinleyDynamic::new(10).unwrap());
scalar_inv!(
    inv_median_absolute_deviation,
    MedianAbsoluteDeviation::new(20).unwrap()
);
scalar_inv!(inv_median_channel, MedianChannel::new(20, 2.0).unwrap());
scalar_inv!(inv_median_ma, MedianMa::new(5).unwrap());
scalar_inv!(inv_mid_point, MidPoint::new(5).unwrap());
scalar_inv!(inv_mom, Mom::new(3).unwrap());
scalar_inv!(inv_omega_ratio, OmegaRatio::new(20, 0.0).unwrap());
scalar_inv!(inv_pain_index, PainIndex::new(10).unwrap());
scalar_inv!(inv_percent_b, PercentB::new(20, 2.0).unwrap());
scalar_inv!(
    inv_percentage_trailing_stop,
    PercentageTrailingStop::new(5.0).unwrap()
);
scalar_inv!(inv_pmo, Pmo::new(35, 20).unwrap());
scalar_inv!(
    inv_polarized_fractal_efficiency,
    PolarizedFractalEfficiency::new(10, 5).unwrap()
);
scalar_inv!(inv_ppo, Ppo::new(12, 26).unwrap());
scalar_inv!(inv_ppo_histogram, PpoHistogram::new(12, 26, 9).unwrap());
scalar_inv!(inv_profit_factor, ProfitFactor::new(20).unwrap());
scalar_inv!(inv_qqe, Qqe::new(14, 5, 4.236).unwrap());
scalar_inv!(inv_quartile_bands, QuartileBands::new(20).unwrap());
scalar_inv!(inv_r_squared, RSquared::new(14).unwrap());
scalar_inv!(
    inv_realized_volatility,
    RealizedVolatility::new(20).unwrap()
);
scalar_inv!(inv_recovery_factor, RecoveryFactor::new());
scalar_inv!(inv_reflex, Reflex::new(20).unwrap());
scalar_inv!(inv_regime_label, RegimeLabel::new(5, 20).unwrap());
scalar_inv!(
    inv_renko_trailing_stop,
    RenkoTrailingStop::new(1.0).unwrap()
);
scalar_inv!(inv_rmi, Rmi::new(14, 5).unwrap());
scalar_inv!(inv_roc, Roc::new(3).unwrap());
scalar_inv!(inv_rocp, Rocp::new(3).unwrap());
scalar_inv!(inv_rocr, Rocr::new(3).unwrap());
scalar_inv!(inv_rocr100, Rocr100::new(3).unwrap());
scalar_inv!(inv_rolling_iqr, RollingIqr::new(20).unwrap());
scalar_inv!(
    inv_rolling_min_max_scaler,
    RollingMinMaxScaler::new(14).unwrap()
);
scalar_inv!(
    inv_rolling_percentile_rank,
    RollingPercentileRank::new(20).unwrap()
);
scalar_inv!(inv_rolling_quantile, RollingQuantile::new(5, 0.5).unwrap());
scalar_inv!(inv_roofing_filter, RoofingFilter::new(10, 48).unwrap());
scalar_inv!(inv_rsi, Rsi::new(3).unwrap());
scalar_inv!(inv_rsx, Rsx::new(14).unwrap());
scalar_inv!(inv_rvi_volatility, RviVolatility::new(10).unwrap());
scalar_inv!(inv_sample_entropy, SampleEntropy::new(50, 2, 0.2).unwrap());
scalar_inv!(inv_shannon_entropy, ShannonEntropy::new(32, 8).unwrap());
scalar_inv!(inv_sharpe_ratio, SharpeRatio::new(20, 0.0).unwrap());
scalar_inv!(inv_sine_wave, SineWave::new());
scalar_inv!(inv_sine_weighted_ma, SineWeightedMa::new(5).unwrap());
scalar_inv!(inv_skewness, Skewness::new(20).unwrap());
scalar_inv!(inv_sma, Sma::new(3).unwrap());
scalar_inv!(inv_smma, Smma::new(3).unwrap());
scalar_inv!(inv_sortino_ratio, SortinoRatio::new(20, 0.0).unwrap());
scalar_inv!(inv_standard_error, StandardError::new(14).unwrap());
scalar_inv!(
    inv_standard_error_bands,
    StandardErrorBands::new(21, 2.0).unwrap()
);
scalar_inv!(inv_stc, Stc::new(3, 5, 4, 0.5).unwrap());
scalar_inv!(inv_std_dev, StdDev::new(20).unwrap());
scalar_inv!(inv_step_trailing_stop, StepTrailingStop::new(1.0).unwrap());
scalar_inv!(inv_sterling_ratio, SterlingRatio::new(12).unwrap());
scalar_inv!(inv_stoch_rsi, StochRsi::new(14, 14).unwrap());
scalar_inv!(inv_super_smoother, SuperSmoother::new(10).unwrap());
scalar_inv!(inv_t3, T3::new(5, 0.7).unwrap());
scalar_inv!(inv_tail_ratio, TailRatio::new(20).unwrap());
scalar_inv!(inv_tema, Tema::new(3).unwrap());
scalar_inv!(inv_tii, Tii::new(20, 10).unwrap());
scalar_inv!(inv_trend_label, TrendLabel::new(10).unwrap());
scalar_inv!(
    inv_trend_strength_index,
    TrendStrengthIndex::new(20).unwrap()
);
scalar_inv!(inv_trendflex, Trendflex::new(20).unwrap());
scalar_inv!(inv_trima, Trima::new(5).unwrap());
scalar_inv!(inv_trix, Trix::new(3).unwrap());
scalar_inv!(inv_tsf, Tsf::new(14).unwrap());
scalar_inv!(inv_tsf_oscillator, TsfOscillator::new(14).unwrap());
scalar_inv!(inv_tsi, Tsi::new(25, 13).unwrap());
scalar_inv!(inv_ulcer_index, UlcerIndex::new(14).unwrap());
scalar_inv!(
    inv_universal_oscillator,
    UniversalOscillator::new(20).unwrap()
);
scalar_inv!(
    inv_upside_potential_ratio,
    UpsidePotentialRatio::new(20, 0.0).unwrap()
);
scalar_inv!(inv_value_at_risk, ValueAtRisk::new(100, 0.95).unwrap());
scalar_inv!(inv_variance, Variance::new(20).unwrap());
scalar_inv!(
    inv_vertical_horizontal_filter,
    VerticalHorizontalFilter::new(28).unwrap()
);
scalar_inv!(inv_vidya, Vidya::new(14, 9).unwrap());
scalar_inv!(
    inv_volatility_of_volatility,
    VolatilityOfVolatility::new(20, 20).unwrap()
);
scalar_inv!(inv_wave_pm, WavePm::new(10, 3).unwrap());
scalar_inv!(inv_win_rate, WinRate::new(4).unwrap());
scalar_inv!(inv_wma, Wma::new(3).unwrap());
scalar_inv!(inv_z_score, ZScore::new(20).unwrap());
scalar_inv!(inv_zero_lag_macd, ZeroLagMacd::new(3, 5, 3).unwrap());
scalar_inv!(inv_zlema, Zlema::new(10).unwrap());

// --- candle (261) ---
candle_inv!(inv_abandoned_baby, AbandonedBaby::new());
candle_inv!(inv_abcd, Abcd::new());
candle_inv!(
    inv_acceleration_bands,
    AccelerationBands::new(20, 4.0).unwrap()
);
candle_inv!(
    inv_accelerator_oscillator,
    AcceleratorOscillator::new(5, 34, 5).unwrap()
);
candle_inv!(inv_ad_oscillator, AdOscillator::new());
candle_inv!(inv_adaptive_cci, AdaptiveCci::new(20).unwrap());
candle_inv!(inv_adl, Adl::new());
candle_inv!(inv_advance_block, AdvanceBlock::new());
candle_inv!(inv_adx, Adx::new(5).unwrap());
candle_inv!(inv_adxr, Adxr::new(5).unwrap());
candle_inv!(inv_alligator, Alligator::new(5, 3, 2).unwrap());
candle_inv!(inv_anchored_vwap, AnchoredVwap::new());
candle_inv!(inv_andrews_pitchfork, AndrewsPitchfork::new(2).unwrap());
candle_inv!(inv_aroon, Aroon::new(5).unwrap());
candle_inv!(inv_aroon_oscillator, AroonOscillator::new(5).unwrap());
candle_inv!(inv_atr, Atr::new(5).unwrap());
candle_inv!(inv_atr_bands, AtrBands::new(14, 3.0).unwrap());
candle_inv!(inv_atr_ratchet, AtrRatchet::new(14, 4.0, 0.1).unwrap());
candle_inv!(
    inv_atr_trailing_stop,
    AtrTrailingStop::new(14, 3.0).unwrap()
);
candle_inv!(inv_auto_fib, AutoFib::new());
candle_inv!(
    inv_average_daily_range,
    AverageDailyRange::new(2, 0).unwrap()
);
candle_inv!(inv_avg_price, AvgPrice::new());
candle_inv!(
    inv_awesome_oscillator,
    AwesomeOscillator::new(3, 10).unwrap()
);
candle_inv!(
    inv_awesome_oscillator_histogram,
    AwesomeOscillatorHistogram::new(3, 5, 3).unwrap()
);
candle_inv!(inv_balance_of_power, BalanceOfPower::new());
candle_inv!(inv_bat, Bat::new());
candle_inv!(inv_belt_hold, BeltHold::new());
candle_inv!(inv_better_volume, BetterVolume::new(20).unwrap());
candle_inv!(inv_body_size_pct, BodySizePct::new());
candle_inv!(inv_breakaway, Breakaway::new());
candle_inv!(inv_butterfly, Butterfly::new());
candle_inv!(inv_camarilla_pivots, Camarilla::new());
candle_inv!(inv_candle_volume, CandleVolume::new(14).unwrap());
candle_inv!(inv_cci, Cci::new(5).unwrap());
candle_inv!(inv_central_pivot_range, CentralPivotRange::new());
candle_inv!(
    inv_chaikin_oscillator,
    ChaikinOscillator::new(3, 10).unwrap()
);
candle_inv!(
    inv_chaikin_volatility,
    ChaikinVolatility::new(10, 10).unwrap()
);
candle_inv!(
    inv_chande_kroll_stop,
    ChandeKrollStop::new(10, 1.0, 9).unwrap()
);
candle_inv!(inv_chandelier_exit, ChandelierExit::new(22, 3.0).unwrap());
candle_inv!(inv_choppiness_index, ChoppinessIndex::new(14).unwrap());
candle_inv!(inv_classic_pivots, ClassicPivots::new());
candle_inv!(inv_close_vs_open, CloseVsOpen::new());
candle_inv!(inv_closing_marubozu, ClosingMarubozu::new());
candle_inv!(inv_cmf, ChaikinMoneyFlow::new(20).unwrap());
candle_inv!(
    inv_composite_profile,
    CompositeProfile::new(100, 50, 0.70).unwrap()
);
candle_inv!(inv_concealing_baby_swallow, ConcealingBabySwallow::new());
candle_inv!(inv_counterattack, Counterattack::new());
candle_inv!(inv_crab, Crab::new());
candle_inv!(inv_cup_and_handle, CupAndHandle::new());
candle_inv!(inv_cypher, Cypher::new());
candle_inv!(inv_day_of_week_profile, DayOfWeekProfile::new(0));
candle_inv!(inv_demand_index, DemandIndex::new(10).unwrap());
candle_inv!(inv_demark_pivots, DemarkPivots::new());
candle_inv!(inv_doji, Doji::new());
candle_inv!(inv_doji_star, DojiStar::new());
candle_inv!(inv_donchian, Donchian::new(5).unwrap());
candle_inv!(inv_donchian_stop, DonchianStop::new(10).unwrap());
candle_inv!(inv_double_top_bottom, DoubleTopBottom::new());
candle_inv!(
    inv_downside_gap_three_methods,
    DownsideGapThreeMethods::new()
);
candle_inv!(inv_dragonfly_doji, DragonflyDoji::new());
candle_inv!(inv_dumpling_top, DumplingTop::new(9).unwrap());
candle_inv!(inv_dx, Dx::new(5).unwrap());
candle_inv!(inv_ease_of_movement, EaseOfMovement::new(14).unwrap());
candle_inv!(inv_elder_ray, ElderRay::new(13).unwrap());
candle_inv!(inv_elder_safezone, ElderSafeZone::new(14, 2.0).unwrap());
candle_inv!(inv_engulfing, Engulfing::new());
candle_inv!(inv_equivolume, Equivolume::new(14).unwrap());
candle_inv!(inv_evening_doji_star, EveningDojiStar::new());
candle_inv!(inv_evwma, Evwma::new(20).unwrap());
candle_inv!(inv_falling_three_methods, FallingThreeMethods::new());
candle_inv!(inv_fib_arcs, FibArcs::new());
candle_inv!(inv_fib_channel, FibChannel::new());
candle_inv!(inv_fib_confluence, FibConfluence::new());
candle_inv!(inv_fib_extension, FibExtension::new());
candle_inv!(inv_fib_fan, FibFan::new());
candle_inv!(inv_fib_projection, FibProjection::new());
candle_inv!(inv_fib_retracement, FibRetracement::new());
candle_inv!(inv_fib_time_zones, FibTimeZones::new());
candle_inv!(inv_fibonacci_pivots, FibonacciPivots::new());
candle_inv!(inv_flag_pennant, FlagPennant::new());
candle_inv!(inv_force_index, ForceIndex::new(13).unwrap());
candle_inv!(inv_fractal_chaos_bands, FractalChaosBands::new(2).unwrap());
candle_inv!(inv_fry_pan_bottom, FryPanBottom::new(9).unwrap());
candle_inv!(inv_gap_side_by_side_white, GapSideBySideWhite::new());
candle_inv!(
    inv_garman_klass,
    GarmanKlassVolatility::new(20, 252).unwrap()
);
candle_inv!(inv_gartley, Gartley::new());
candle_inv!(inv_gator_oscillator, GatorOscillator::new(5, 3, 2).unwrap());
candle_inv!(inv_golden_pocket, GoldenPocket::new());
candle_inv!(inv_gravestone_doji, GravestoneDoji::new());
candle_inv!(inv_hammer, Hammer::new());
candle_inv!(inv_hanging_man, HangingMan::new());
candle_inv!(inv_harami, Harami::new());
candle_inv!(inv_harami_cross, HaramiCross::new());
candle_inv!(inv_head_and_shoulders, HeadAndShoulders::new());
candle_inv!(inv_heikin_ashi, HeikinAshi::new());
candle_inv!(
    inv_heikin_ashi_oscillator,
    HeikinAshiOscillator::new(5).unwrap()
);
candle_inv!(inv_high_low_range, HighLowRange::new());
candle_inv!(
    inv_high_low_volume_nodes,
    HighLowVolumeNodes::new(20, 24).unwrap()
);
candle_inv!(inv_high_wave, HighWave::new());
candle_inv!(inv_hikkake, Hikkake::new());
candle_inv!(inv_hikkake_modified, HikkakeModified::new());
candle_inv!(inv_hilo_activator, HiLoActivator::new(3).unwrap());
candle_inv!(inv_homing_pigeon, HomingPigeon::new());
candle_inv!(inv_hurst_channel, HurstChannel::new(10, 0.5).unwrap());
candle_inv!(inv_ichimoku, Ichimoku::new(5, 10, 20, 10).unwrap());
candle_inv!(inv_identical_three_crows, IdenticalThreeCrows::new());
candle_inv!(inv_in_neck, InNeck::new());
candle_inv!(inv_inertia, Inertia::new(14, 20).unwrap());
candle_inv!(inv_initial_balance, InitialBalance::new(3).unwrap());
candle_inv!(inv_intraday_intensity, IntradayIntensity::new());
candle_inv!(
    inv_intraday_momentum_index,
    IntradayMomentumIndex::new(14).unwrap()
);
candle_inv!(
    inv_intraday_volatility_profile,
    IntradayVolatilityProfile::new(24, 0).unwrap()
);
candle_inv!(inv_inverted_hammer, InvertedHammer::new());
candle_inv!(inv_kase_devstop, KaseDevStop::new(30, 1.0).unwrap());
candle_inv!(
    inv_kase_permission_stochastic,
    KasePermissionStochastic::new(9, 3).unwrap()
);
candle_inv!(inv_keltner, Keltner::new(5, 5, 2.0).unwrap());
candle_inv!(inv_kicking, Kicking::new());
candle_inv!(inv_kicking_by_length, KickingByLength::new());
candle_inv!(inv_kvo, Kvo::new(34, 55).unwrap());
candle_inv!(inv_ladder_bottom, LadderBottom::new());
candle_inv!(inv_long_legged_doji, LongLeggedDoji::new());
candle_inv!(inv_long_line, LongLine::new());
candle_inv!(
    inv_market_facilitation_index,
    MarketFacilitationIndex::new()
);
candle_inv!(inv_marubozu, Marubozu::new());
candle_inv!(inv_mass_index, MassIndex::new(9, 25).unwrap());
candle_inv!(inv_mat_hold, MatHold::new());
candle_inv!(inv_matching_low, MatchingLow::new());
candle_inv!(inv_median_price, MedianPrice::new());
candle_inv!(inv_mfi, Mfi::new(5).unwrap());
candle_inv!(inv_mid_price, MidPrice::new(5).unwrap());
candle_inv!(inv_minus_di, MinusDi::new(5).unwrap());
candle_inv!(inv_minus_dm, MinusDm::new(5).unwrap());
candle_inv!(inv_modified_ma_stop, ModifiedMaStop::new(14).unwrap());
candle_inv!(inv_morning_doji_star, MorningDojiStar::new());
candle_inv!(inv_morning_evening_star, MorningEveningStar::new());
candle_inv!(inv_murrey_math_lines, MurreyMathLines::new(64).unwrap());
candle_inv!(inv_naked_poc, NakedPoc::new(20, 24).unwrap());
candle_inv!(inv_natr, Natr::new(14).unwrap());
candle_inv!(inv_new_price_lines, NewPriceLines::new(8).unwrap());
candle_inv!(inv_nrtr, Nrtr::new(2.0).unwrap());
candle_inv!(inv_nvi, Nvi::new());
candle_inv!(inv_obv, Obv::new());
candle_inv!(inv_on_neck, OnNeck::new());
candle_inv!(inv_opening_marubozu, OpeningMarubozu::new());
candle_inv!(inv_opening_range, OpeningRange::new(2).unwrap());
candle_inv!(inv_overnight_gap, OvernightGap::new(0));
candle_inv!(
    inv_overnight_intraday_return,
    OvernightIntradayReturn::new(0)
);
candle_inv!(inv_parkinson, ParkinsonVolatility::new(20, 252).unwrap());
candle_inv!(inv_pgo, Pgo::new(14).unwrap());
candle_inv!(inv_piercing_dark_cloud, PiercingDarkCloud::new());
candle_inv!(inv_pivot_reversal, PivotReversal::new(2, 2).unwrap());
candle_inv!(inv_plus_di, PlusDi::new(5).unwrap());
candle_inv!(inv_plus_dm, PlusDm::new(5).unwrap());
candle_inv!(inv_profile_shape, ProfileShape::new(20, 24).unwrap());
candle_inv!(inv_projection_bands, ProjectionBands::new(14).unwrap());
candle_inv!(
    inv_projection_oscillator,
    ProjectionOscillator::new(14).unwrap()
);
candle_inv!(inv_psar, Psar::new(0.02, 0.02, 0.2).unwrap());
candle_inv!(inv_pvi, Pvi::new());
candle_inv!(inv_qstick, Qstick::new(5).unwrap());
candle_inv!(inv_rectangle_range, RectangleRange::new());
candle_inv!(inv_rickshaw_man, RickshawMan::new());
candle_inv!(inv_rising_three_methods, RisingThreeMethods::new());
candle_inv!(
    inv_rogers_satchell,
    RogersSatchellVolatility::new(20, 252).unwrap()
);
candle_inv!(inv_rvi, Rvi::new(10).unwrap());
candle_inv!(inv_rwi, Rwi::new(14).unwrap());
candle_inv!(
    inv_sar_ext,
    SarExt::new(0.0, 0.0, 0.02, 0.02, 0.2, 0.02, 0.02, 0.2).unwrap()
);
candle_inv!(inv_seasonal_z_score, SeasonalZScore::new(0));
candle_inv!(inv_separating_lines, SeparatingLines::new());
candle_inv!(inv_session_high_low, SessionHighLow::new(0));
candle_inv!(inv_session_range, SessionRange::new(0));
candle_inv!(inv_session_vwap, SessionVwap::new(0));
candle_inv!(inv_shark, Shark::new());
candle_inv!(inv_shooting_star, ShootingStar::new());
candle_inv!(inv_short_line, ShortLine::new());
candle_inv!(inv_single_prints, SinglePrints::new(20, 24).unwrap());
candle_inv!(inv_smi, Smi::new(5, 3, 3).unwrap());
candle_inv!(
    inv_smoothed_heikin_ashi,
    SmoothedHeikinAshi::new(10).unwrap()
);
candle_inv!(inv_spinning_top, SpinningTop::new());
candle_inv!(inv_stalled_pattern, StalledPattern::new());
candle_inv!(inv_starc_bands, StarcBands::new(6, 15, 2.0).unwrap());
candle_inv!(inv_stick_sandwich, StickSandwich::new());
candle_inv!(inv_stochastic, Stochastic::new(5, 3).unwrap());
candle_inv!(inv_stochastic_cci, StochasticCci::new(14).unwrap());
candle_inv!(inv_super_trend, SuperTrend::new(10, 3.0).unwrap());
candle_inv!(inv_takuri, Takuri::new());
candle_inv!(inv_tasuki_gap, TasukiGap::new());
candle_inv!(inv_td_camouflage, TdCamouflage::new());
candle_inv!(inv_td_clop, TdClop::new());
candle_inv!(inv_td_clopwin, TdClopwin::new());
candle_inv!(inv_td_combo, TdCombo::new(4, 9, 2, 13).unwrap());
candle_inv!(inv_td_countdown, TdCountdown::new(4, 9, 2, 13).unwrap());
candle_inv!(inv_td_demarker, TdDeMarker::new(14).unwrap());
candle_inv!(inv_td_differential, TdDifferential::new());
candle_inv!(inv_td_dwave, TdDWave::new(2).unwrap());
candle_inv!(inv_td_lines, TdLines::new(4, 9).unwrap());
candle_inv!(inv_td_moving_average, TdMovingAverage::new(5, 13).unwrap());
candle_inv!(inv_td_open, TdOpen::new());
candle_inv!(inv_td_pressure, TdPressure::new(5).unwrap());
candle_inv!(inv_td_propulsion, TdPropulsion::new());
candle_inv!(inv_td_range_projection, TdRangeProjection::new());
candle_inv!(inv_td_rei, TdRei::new(5).unwrap());
candle_inv!(inv_td_risk_level, TdRiskLevel::new(4, 9).unwrap());
candle_inv!(inv_td_sequential, TdSequential::new(4, 9, 2, 13).unwrap());
candle_inv!(inv_td_setup, TdSetup::new(4, 9).unwrap());
candle_inv!(inv_td_trap, TdTrap::new());
candle_inv!(inv_three_drives, ThreeDrives::new());
candle_inv!(inv_three_inside, ThreeInside::new());
candle_inv!(inv_three_line_break, ThreeLineBreak::new(3).unwrap());
candle_inv!(inv_three_line_strike, ThreeLineStrike::new());
candle_inv!(inv_three_outside, ThreeOutside::new());
candle_inv!(inv_three_soldiers_or_crows, ThreeSoldiersOrCrows::new());
candle_inv!(inv_three_stars_in_south, ThreeStarsInSouth::new());
candle_inv!(inv_thrusting, Thrusting::new());
candle_inv!(inv_time_based_stop, TimeBasedStop::new(5).unwrap());
candle_inv!(
    inv_time_of_day_return_profile,
    TimeOfDayReturnProfile::new(24, 0).unwrap()
);
candle_inv!(inv_tower_top_bottom, TowerTopBottom::new());
candle_inv!(inv_tpo_profile, TpoProfile::new(5, 10).unwrap());
candle_inv!(inv_trade_volume_index, TradeVolumeIndex::new(0.5).unwrap());
candle_inv!(inv_triangle, Triangle::new());
candle_inv!(inv_triple_top_bottom, TripleTopBottom::new());
candle_inv!(inv_tristar, Tristar::new());
candle_inv!(inv_true_range, TrueRange::new());
candle_inv!(inv_tsv, Tsv::new(18).unwrap());
candle_inv!(inv_ttm_squeeze, TtmSqueeze::new(20, 2.0, 1.5).unwrap());
candle_inv!(inv_ttm_trend, TtmTrend::new(6).unwrap());
candle_inv!(inv_turn_of_month, TurnOfMonth::new(3, 1, 0).unwrap());
candle_inv!(inv_tweezer, Tweezer::new());
candle_inv!(inv_twiggs_money_flow, TwiggsMoneyFlow::new(21).unwrap());
candle_inv!(inv_two_crows, TwoCrows::new());
candle_inv!(inv_typical_price, TypicalPrice::new());
candle_inv!(
    inv_ultimate_oscillator,
    UltimateOscillator::new(7, 14, 28).unwrap()
);
candle_inv!(inv_unique_three_river, UniqueThreeRiver::new());
candle_inv!(inv_upside_gap_three_methods, UpsideGapThreeMethods::new());
candle_inv!(inv_upside_gap_two_crows, UpsideGapTwoCrows::new());
candle_inv!(inv_value_area, ValueArea::new(5, 50, 0.70).unwrap());
candle_inv!(inv_volatility_cone, VolatilityCone::new(20, 60).unwrap());
candle_inv!(inv_volatility_ratio, VolatilityRatio::new(14).unwrap());
candle_inv!(inv_volty_stop, VoltyStop::new(14, 2.0).unwrap());
candle_inv!(
    inv_volume_by_time_profile,
    VolumeByTimeProfile::new(24, 0).unwrap()
);
candle_inv!(
    inv_volume_oscillator,
    VolumeOscillator::new(14, 28).unwrap()
);
candle_inv!(inv_volume_profile, VolumeProfile::new(5, 10).unwrap());
candle_inv!(inv_volume_rsi, VolumeRsi::new(14).unwrap());
candle_inv!(
    inv_volume_weighted_macd,
    VolumeWeightedMacd::new(12, 26, 9).unwrap()
);
candle_inv!(inv_volume_weighted_sr, VolumeWeightedSr::new(20).unwrap());
candle_inv!(inv_vortex, Vortex::new(14).unwrap());
candle_inv!(inv_vpt, VolumePriceTrend::new());
candle_inv!(inv_rolling_vwap, RollingVwap::new(14).unwrap());
candle_inv!(inv_vwap, Vwap::new());
candle_inv!(inv_vwap_stddev_bands, VwapStdDevBands::new(2.0).unwrap());
candle_inv!(inv_vwma, Vwma::new(5).unwrap());
candle_inv!(inv_vzo, Vzo::new(14).unwrap());
candle_inv!(inv_wad, Wad::new());
candle_inv!(inv_wave_trend, WaveTrend::new(5, 8, 3).unwrap());
candle_inv!(inv_wedge, Wedge::new());
candle_inv!(inv_weighted_close, WeightedClose::new());
candle_inv!(inv_wick_ratio, WickRatio::new());
candle_inv!(inv_williams_fractals, WilliamsFractals::new());
candle_inv!(inv_williams_r, WilliamsR::new(5).unwrap());
candle_inv!(inv_woodie_pivots, WoodiePivots::new());
candle_inv!(inv_yang_zhang, YangZhangVolatility::new(20, 252).unwrap());
candle_inv!(inv_yoyo_exit, YoyoExit::new(14, 2.0).unwrap());
candle_inv!(inv_zig_zag, ZigZag::new(0.10).unwrap());

// --- pairwise (24) ---
pair_inv!(inv_alpha, Alpha::new(20, 0.001).unwrap());
pair_inv!(inv_beta, Beta::new(20).unwrap());
pair_inv!(inv_beta_neutral_spread, BetaNeutralSpread::new(20).unwrap());
pair_inv!(inv_cointegration, Cointegration::new(30, 1).unwrap());
pair_inv!(inv_distance_ssd, DistanceSsd::new(20).unwrap());
pair_inv!(inv_estimated_leverage_ratio, EstimatedLeverageRatio::new());
pair_inv!(inv_granger_causality, GrangerCausality::new(60, 1).unwrap());
pair_inv!(
    inv_hasbrouck_information_share,
    HasbrouckInformationShare::new(20).unwrap()
);
pair_inv!(inv_information_ratio, InformationRatio::new(10).unwrap());
pair_inv!(
    inv_kalman_hedge_ratio,
    KalmanHedgeRatio::new(1e-2, 1e-3).unwrap()
);
pair_inv!(inv_kendall_tau, KendallTau::new(20).unwrap());
pair_inv!(
    inv_lead_lag_cross_correlation,
    LeadLagCrossCorrelation::new(12, 5).unwrap()
);
pair_inv!(inv_ou_half_life, OuHalfLife::new(40).unwrap());
pair_inv!(inv_pair_spread_zscore, PairSpreadZScore::new(2, 2).unwrap());
pair_inv!(inv_pairwise_beta, PairwiseBeta::new(10).unwrap());
pair_inv!(
    inv_pearson_correlation,
    PearsonCorrelation::new(20).unwrap()
);
pair_inv!(
    inv_relative_strength_ab,
    RelativeStrengthAB::new(5, 5).unwrap()
);
pair_inv!(
    inv_rolling_correlation,
    RollingCorrelation::new(10).unwrap()
);
pair_inv!(inv_rolling_covariance, RollingCovariance::new(5).unwrap());
pair_inv!(
    inv_spearman_correlation,
    SpearmanCorrelation::new(10).unwrap()
);
pair_inv!(
    inv_spread_ar1_coefficient,
    SpreadAr1Coefficient::new(40).unwrap()
);
pair_inv!(
    inv_spread_bollinger_bands,
    SpreadBollingerBands::new(20, 2.0).unwrap()
);
pair_inv!(inv_spread_hurst, SpreadHurst::new(60).unwrap());
pair_inv!(inv_treynor_ratio, TreynorRatio::new(20, 0.001).unwrap());
pair_inv!(inv_variance_ratio, VarianceRatio::new(60, 2).unwrap());

// --- cross_section_inv (15) ---
cross_section_inv!(inv_absolute_breadth_index, AbsoluteBreadthIndex::new());
cross_section_inv!(inv_ad_volume_line, AdVolumeLine::new());
cross_section_inv!(inv_advance_decline, AdvanceDecline::new());
cross_section_inv!(inv_advance_decline_ratio, AdvanceDeclineRatio::new());
cross_section_inv!(inv_breadth_thrust, BreadthThrust::new(2).unwrap());
cross_section_inv!(inv_bullish_percent_index, BullishPercentIndex::new());
cross_section_inv!(inv_cumulative_volume_index, CumulativeVolumeIndex::new());
cross_section_inv!(inv_high_low_index, HighLowIndex::new(2).unwrap());
cross_section_inv!(inv_mcclellan_oscillator, McClellanOscillator::new());
cross_section_inv!(
    inv_mcclellan_summation_index,
    McClellanSummationIndex::new()
);
cross_section_inv!(inv_new_highs_new_lows, NewHighsNewLows::new());
cross_section_inv!(inv_percent_above_ma, PercentAboveMa::new());
cross_section_inv!(inv_tick_index, TickIndex::new());
cross_section_inv!(inv_trin, Trin::new());
cross_section_inv!(inv_up_down_volume_ratio, UpDownVolumeRatio::new());

// --- trade_inv (9) ---
trade_inv!(inv_amihud_illiquidity, AmihudIlliquidity::new(20).unwrap());
trade_inv!(inv_cvd, CumulativeVolumeDelta::new());
trade_inv!(inv_footprint, Footprint::new(1.0).unwrap());
trade_inv!(inv_pin, Pin::new(20).unwrap());
trade_inv!(inv_roll_measure, RollMeasure::new(20).unwrap());
trade_inv!(inv_signed_volume, SignedVolume::new());
trade_inv!(inv_trade_imbalance, TradeImbalance::new(2).unwrap());
trade_inv!(
    inv_trade_sign_autocorrelation,
    TradeSignAutocorrelation::new(20).unwrap()
);
trade_inv!(inv_vpin, Vpin::new(10.0, 2).unwrap());

// --- deriv_inv (17) ---
deriv_inv!(inv_calendar_spread, CalendarSpread::new());
deriv_inv!(inv_funding_basis, FundingBasis::new());
deriv_inv!(
    inv_funding_implied_apr,
    FundingImpliedApr::new(1095.0).unwrap()
);
deriv_inv!(inv_funding_rate, FundingRate::new());
deriv_inv!(inv_funding_rate_mean, FundingRateMean::new(2).unwrap());
deriv_inv!(inv_funding_rate_zscore, FundingRateZScore::new(2).unwrap());
deriv_inv!(inv_liquidation_features, LiquidationFeatures::new());
deriv_inv!(inv_long_short_ratio, LongShortRatio::new());
deriv_inv!(inv_oi_delta, OpenInterestDelta::new());
deriv_inv!(inv_oi_price_divergence, OIPriceDivergence::new(1).unwrap());
deriv_inv!(inv_oi_to_volume_ratio, OiToVolumeRatio::new());
deriv_inv!(inv_oi_weighted, OIWeighted::new());
deriv_inv!(
    inv_open_interest_momentum,
    OpenInterestMomentum::new(5).unwrap()
);
deriv_inv!(inv_perpetual_premium_index, PerpetualPremiumIndex::new());
deriv_inv!(inv_taker_buy_sell_ratio, TakerBuySellRatio::new());
deriv_inv!(inv_term_structure_basis, TermStructureBasis::new());

// --- orderbook_inv (7) ---
orderbook_inv!(inv_depth_slope, DepthSlope::new());
orderbook_inv!(inv_microprice, Microprice::new());
orderbook_inv!(inv_ob_imbalance_full, OrderBookImbalanceFull::new());
orderbook_inv!(inv_ob_imbalance_top1, OrderBookImbalanceTop1::new());
orderbook_inv!(
    inv_ob_imbalance_topn,
    OrderBookImbalanceTopN::new(2).unwrap()
);
orderbook_inv!(
    inv_order_flow_imbalance,
    OrderFlowImbalance::new(20).unwrap()
);
orderbook_inv!(inv_quoted_spread, QuotedSpread::new());

// --- tradequote_inv (3) ---
tradequote_inv!(inv_effective_spread, EffectiveSpread::new());
tradequote_inv!(inv_kyles_lambda, KylesLambda::new(8).unwrap());
tradequote_inv!(inv_realized_spread, RealizedSpread::new(1).unwrap());

// --- bar_inv (10) ---
bar_inv!(inv_dollar_bars, DollarBars::new(1000.0).unwrap());
bar_inv!(inv_imbalance_bars, ImbalanceBars::new(3.0).unwrap());
bar_inv!(inv_kagi_bars, KagiBars::new(2.0).unwrap());
bar_inv!(
    inv_point_and_figure_bars,
    PointAndFigureBars::new(1.0, 3).unwrap()
);
bar_inv!(inv_range_bars, RangeBars::new(1.0).unwrap());
bar_inv!(inv_renko_bars, RenkoBars::new(1.0).unwrap());
bar_inv!(inv_run_bars, RunBars::new(3).unwrap());
bar_inv!(
    inv_three_line_break_bars,
    ThreeLineBreakBars::new(3).unwrap()
);
bar_inv!(inv_tick_bars, TickBars::new(3).unwrap());
bar_inv!(inv_volume_bars, VolumeBars::new(100.0).unwrap());

// The suite is a long list of macro calls, and `vwap.rs` defines two indicators
// while only one of them was ever listed -- so `RollingVwap` alone was never
// held to the warmup, readiness, reset and non-finite contract the other 513
// are. Nothing noticed, because a missing entry is a test that does not exist.
//
// `FAMILIES` is the catalogue, so it can say what the list should contain.
// Reading this file back at compile time is the cheapest way to compare the two
// without a build script.
#[test]
fn every_catalogued_indicator_has_an_invariant_test() {
    const SOURCE: &str = include_str!("invariants.rs");

    // One catalogued name can be the suffix of another -- `Vwap` and
    // `RollingVwap` -- so a plain `contains` would let the shorter one ride on
    // the longer one's entry. Require that the character before the match
    // cannot continue an identifier.
    //
    // For the same reason this function must not spell any constructor out in
    // prose: the haystack is this very file, so a name written in a comment
    // would answer for itself.
    fn constructs(name: &str) -> bool {
        let needle = format!("{name}::new");
        SOURCE.match_indices(&needle).any(|(at, _)| {
            at == 0
                || !SOURCE[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
    }

    let missing: Vec<&str> = FAMILIES
        .iter()
        .flat_map(|(_, members)| members.iter().copied())
        .filter(|name| !constructs(name))
        .collect();
    assert!(
        missing.is_empty(),
        "catalogued indicators with no invariant test: {missing:?}"
    );
}
