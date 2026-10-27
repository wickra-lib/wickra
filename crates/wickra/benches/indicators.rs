//! Microbenchmarks for a curated subset of the indicator catalogue.
//!
//! Run with:
//! ```text
//! cargo bench -p wickra
//! ```
//!
//! Each benchmark feeds real BTCUSDT 1-minute candles — read from the
//! checked-in dataset at the workspace `examples/data/btcusdt-1m.csv` —
//! through both the streaming (`update` loop) and batch APIs of an
//! indicator. Sizes cover small (1 000), medium (10 000), and large
//! (50 000) workloads, taken as prefixes of that dataset.
//!
//! ## Why curated rather than exhaustive
//!
//! The indicator catalogue has 214 entries; benching every single one
//! at three sizes inflates `cargo bench` to >10 minutes for diminishing
//! signal. The selection below picks the cheapest baseline and the
//! most-expensive representative in each family — a regression in any
//! of those is the meaningful signal; per-family redundancy benches
//! mostly produce noise.
//!
//! If you need a benchmark for a specific indicator that is not in this
//! list, add it locally and run `cargo bench -- <name>` to target just
//! that bench.
//!
//! Regenerate the dataset with:
//! ```text
//! cargo run -p wickra-examples --bin fetch_btcusdt
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use wickra::{
    Adx, AnchoredRsi, Atr, Autocorrelation, BatchExt, BollingerBands, BollingerOutput, CalmarRatio,
    Candle, Cci, ClassicPivots, ConnorsRsi, DepthSlope, DerivativesTick, EffectiveSpread, Ema,
    EmpiricalModeDecomposition, Engulfing, Frama, FundingRate, FundingRateZScore,
    HilbertDominantCycle, HurstExponent, Ichimoku, IchimokuOutput, Indicator, Jma, KylesLambda,
    Level, LinearRegression, MacdIndicator, MacdOutput, Mama, MamaOutput, MaxDrawdown, Microprice,
    Obv, OrderBook, OrderBookImbalanceFull, OrderBookImbalanceTop1, ParkinsonVolatility, Ppo, Psar,
    RollingVwap, Rsi, RunningDrawdown, SharpeRatio, Side, SignedVolume, Sma, Stc, SuperTrend,
    SuperTrendOutput, TdSequential, TdSequentialOutput, TpoProfile, TpoProfileOutput, Trade,
    TradeImbalance, TradeQuote, TtmSqueeze, TtmSqueezeOutput, ValueArea, ValueAreaOutput,
    ValueAtRisk, VolumeProfile, VolumeProfileOutput, Vwap, VwapStdDevBands, VwapStdDevBandsOutput,
    WaveTrend, YangZhangVolatility, T3,
};
use wickra_data::csv::CandleReader;

/// Workload sizes, in candles. Each is taken as a prefix of the dataset.
const SIZES: &[usize] = &[1_000, 10_000, 50_000];

/// Load the checked-in BTCUSDT 1-minute candle dataset from the workspace
/// `examples/data/` directory.
fn load_candles() -> Vec<Candle> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/data/btcusdt-1m.csv"
    );
    let mut reader = CandleReader::open(path).unwrap_or_else(|e| {
        panic!(
            "could not open the benchmark dataset {path}: {e}\n\
             generate it with `cargo run -p wickra-examples --bin fetch_btcusdt`"
        )
    });
    reader
        .read_all()
        .expect("the benchmark dataset is valid OHLCV")
}

fn bench_scalar<I, F>(c: &mut Criterion, name: &str, prices: &[f64], make: F)
where
    F: Fn() -> I,
    I: Indicator<Input = f64, Output = f64> + BatchExt,
{
    let mut group = c.benchmark_group(name);
    for &n in SIZES {
        let n = n.min(prices.len());
        let series = &prices[..n];
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("streaming", n), series, |b, prices| {
            b.iter(|| {
                let mut ind = make();
                for p in prices {
                    black_box(ind.update(*p));
                }
            });
        });
        group.bench_with_input(BenchmarkId::new("batch", n), series, |b, prices| {
            b.iter(|| {
                let mut ind = make();
                black_box(ind.batch(prices));
            });
        });
    }
    group.finish();
}

fn bench_candle_input<I, F, O>(c: &mut Criterion, name: &str, candles: &[Candle], make: F)
where
    F: Fn() -> I,
    I: Indicator<Input = Candle, Output = O>,
{
    let mut group = c.benchmark_group(name);
    for &n in SIZES {
        let n = n.min(candles.len());
        let series = &candles[..n];
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("streaming", n), series, |b, candles| {
            b.iter(|| {
                let mut ind = make();
                for c in candles {
                    black_box(ind.update(*c));
                }
            });
        });
    }
    group.finish();
}

fn bench_orderbook_input<I, F, O>(c: &mut Criterion, name: &str, books: &[OrderBook], make: F)
where
    F: Fn() -> I,
    I: Indicator<Input = OrderBook, Output = O>,
{
    let mut group = c.benchmark_group(name);
    for &n in SIZES {
        let n = n.min(books.len());
        let series = &books[..n];
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("streaming", n), series, |b, books| {
            b.iter(|| {
                let mut ind = make();
                for book in books {
                    black_box(ind.update(book.clone()));
                }
            });
        });
    }
    group.finish();
}

fn bench_trade_input<I, F, O>(c: &mut Criterion, name: &str, trades: &[Trade], make: F)
where
    F: Fn() -> I,
    I: Indicator<Input = Trade, Output = O>,
{
    let mut group = c.benchmark_group(name);
    for &n in SIZES {
        let n = n.min(trades.len());
        let series = &trades[..n];
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("streaming", n), series, |b, trades| {
            b.iter(|| {
                let mut ind = make();
                for t in trades {
                    black_box(ind.update(*t));
                }
            });
        });
    }
    group.finish();
}

fn bench_tradequote_input<I, F, O>(c: &mut Criterion, name: &str, quotes: &[TradeQuote], make: F)
where
    F: Fn() -> I,
    I: Indicator<Input = TradeQuote, Output = O>,
{
    let mut group = c.benchmark_group(name);
    for &n in SIZES {
        let n = n.min(quotes.len());
        let series = &quotes[..n];
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("streaming", n), series, |b, quotes| {
            b.iter(|| {
                let mut ind = make();
                for q in quotes {
                    black_box(ind.update(*q));
                }
            });
        });
    }
    group.finish();
}

fn bench_derivatives_input<I, F, O>(
    c: &mut Criterion,
    name: &str,
    ticks: &[DerivativesTick],
    make: F,
) where
    F: Fn() -> I,
    I: Indicator<Input = DerivativesTick, Output = O>,
{
    let mut group = c.benchmark_group(name);
    for &n in SIZES {
        let n = n.min(ticks.len());
        let series = &ticks[..n];
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("streaming", n), series, |b, ticks| {
            b.iter(|| {
                let mut ind = make();
                for tick in ticks {
                    black_box(ind.update(*tick));
                }
            });
        });
    }
    group.finish();
}

fn bench_scalar_multi<I, F, O>(c: &mut Criterion, name: &str, prices: &[f64], make: F)
where
    F: Fn() -> I,
    I: Indicator<Input = f64, Output = O>,
{
    let mut group = c.benchmark_group(name);
    for &n in SIZES {
        let n = n.min(prices.len());
        let series = &prices[..n];
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("streaming", n), series, |b, prices| {
            b.iter(|| {
                let mut ind = make();
                for p in prices {
                    black_box(ind.update(*p));
                }
            });
        });
    }
    group.finish();
}

#[allow(clippy::too_many_lines)]
fn benches(c: &mut Criterion) {
    let candles = load_candles();
    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();

    // === Family 01 — Moving Averages ===
    // Sma: cheapest baseline; Ema: recursive baseline; Frama / Jma / T3: adaptive / expensive.
    bench_scalar(c, "sma", &closes, || Sma::new(14).unwrap());
    bench_scalar(c, "ema", &closes, || Ema::new(14).unwrap());
    bench_scalar(c, "frama", &closes, || Frama::new(16).unwrap());
    bench_scalar(c, "jma", &closes, || Jma::new(14, 0.0, 2).unwrap());
    bench_scalar(c, "t3", &closes, || T3::new(14, 0.7).unwrap());

    // === Family 02 — Momentum Oscillators ===
    // Rsi: textbook baseline; ConnorsRsi: three-component composite.
    bench_scalar(c, "rsi", &closes, || Rsi::new(14).unwrap());
    bench_scalar(c, "anchored_rsi", &closes, AnchoredRsi::new);
    bench_candle_input(c, "cci", &candles, || Cci::new(20).unwrap());
    bench_scalar(c, "connors_rsi", &closes, ConnorsRsi::classic);

    // === Family 03 — Trend & Directional ===
    // Adx is multi-component (DI+/DI-/ADX); WaveTrend is the heaviest in this group.
    bench_candle_input(c, "adx", &candles, || Adx::new(14).unwrap());
    bench_candle_input(c, "wave_trend", &candles, || WaveTrend::classic().unwrap());

    // === Family 04 — Price Oscillators ===
    // Macd: multi-output baseline; Stc: deeply recursive (most expensive in family).
    bench_scalar_multi::<_, _, MacdOutput>(c, "macd", &closes, MacdIndicator::classic);
    bench_scalar(c, "ppo", &closes, || Ppo::new(12, 26).unwrap());
    bench_scalar(c, "stc", &closes, Stc::classic);

    // === Family 05 — Volatility & Bands ===
    // Atr: cheap baseline; Bollinger: stddev-heavy; YangZhang: most-expensive volatility metric.
    bench_candle_input(c, "atr", &candles, || Atr::new(14).unwrap());
    bench_scalar_multi::<_, _, BollingerOutput>(c, "bollinger", &closes, || {
        BollingerBands::new(20, 2.0).unwrap()
    });
    bench_candle_input(c, "parkinson", &candles, || {
        ParkinsonVolatility::new(20, 252).unwrap()
    });
    bench_candle_input(c, "yang_zhang", &candles, || {
        YangZhangVolatility::new(20, 252).unwrap()
    });

    // === Family 06 — Bands & Channels ===
    // TtmSqueeze: multi-indicator composite; VwapStdDevBands: volume-weighted.
    bench_candle_input::<_, _, TtmSqueezeOutput>(c, "ttm_squeeze", &candles, || {
        TtmSqueeze::new(20, 2.0, 1.5).unwrap()
    });
    bench_candle_input::<_, _, VwapStdDevBandsOutput>(c, "vwap_stddev_bands", &candles, || {
        VwapStdDevBands::new(2.0).unwrap()
    });

    // === Family 07 — Trailing Stops ===
    // Psar: textbook trailing stop; SuperTrend: ATR-anchored band.
    bench_candle_input(c, "psar", &candles, || Psar::new(0.02, 0.02, 0.2).unwrap());
    bench_candle_input::<_, _, SuperTrendOutput>(c, "super_trend", &candles, || {
        SuperTrend::new(10, 3.0).unwrap()
    });

    // === Family 08 — Volume ===
    // Obv: simplest volume cumul; Vwap: session cumul; RollingVwap: rolling window.
    bench_candle_input(c, "obv", &candles, Obv::new);
    bench_candle_input(c, "vwap", &candles, Vwap::new);
    bench_candle_input(c, "rolling_vwap", &candles, || {
        RollingVwap::new(20).unwrap()
    });

    // === Family 09 — Price Statistics ===
    // LinearRegression: OLS baseline; HurstExponent: R/S analysis (most expensive in family);
    // Autocorrelation: lag-correlation.
    bench_scalar(c, "linear_regression", &closes, || {
        LinearRegression::new(14).unwrap()
    });
    bench_scalar(c, "hurst_exponent", &closes, || {
        HurstExponent::new(100, 4).unwrap()
    });
    bench_scalar(c, "autocorrelation", &closes, || {
        Autocorrelation::new(20, 1).unwrap()
    });

    // === Family 10 — Ehlers / Cycle (DSP) ===
    // Mama: paired adaptive MA (multi-output); HilbertDominantCycle: cycle estimation;
    // EmpiricalModeDecomposition: heaviest DSP indicator in the catalogue.
    bench_scalar_multi::<_, _, MamaOutput>(c, "mama", &closes, Mama::classic);
    bench_scalar(
        c,
        "hilbert_dominant_cycle",
        &closes,
        HilbertDominantCycle::new,
    );
    bench_scalar(c, "empirical_mode_decomposition", &closes, || {
        EmpiricalModeDecomposition::new(20, 0.5).unwrap()
    });

    // === Family 11 — Pivots & Support/Resistance ===
    bench_candle_input(c, "classic_pivots", &candles, ClassicPivots::new);

    // === Family 12 — DeMark ===
    // TdSequential is the most complex in the family (state machine + countdown).
    bench_candle_input::<_, _, TdSequentialOutput>(
        c,
        "td_sequential",
        &candles,
        TdSequential::classic,
    );

    // === Family 13 — Ichimoku & Charts ===
    bench_candle_input::<_, _, IchimokuOutput>(c, "ichimoku", &candles, Ichimoku::classic);

    // === Family 14 — Candlestick Patterns ===
    // Engulfing is two-bar so representative across the candlestick family.
    bench_candle_input(c, "engulfing", &candles, Engulfing::new);

    // === Family 15 — Market Profile ===
    bench_candle_input::<_, _, ValueAreaOutput>(c, "value_area", &candles, || {
        ValueArea::new(20, 50, 0.70).unwrap()
    });
    bench_candle_input::<_, _, VolumeProfileOutput>(c, "volume_profile", &candles, || {
        VolumeProfile::new(20, 50).unwrap()
    });
    bench_candle_input::<_, _, TpoProfileOutput>(c, "tpo_profile", &candles, || {
        TpoProfile::new(20, 50).unwrap()
    });

    // === Family 16 — Risk / Performance Metrics ===
    // Close-prices stand in for the equity curve / return stream; absolute
    // numbers aren't meaningful here — what matters is the per-update cost.
    bench_scalar(c, "sharpe_ratio", &closes, || {
        SharpeRatio::new(20, 0.0).unwrap()
    });
    bench_scalar(c, "max_drawdown", &closes, || MaxDrawdown::new(20).unwrap());
    bench_scalar(c, "running_drawdown", &closes, RunningDrawdown::new);
    bench_scalar(c, "calmar_ratio", &closes, || CalmarRatio::new(20).unwrap());
    bench_scalar(c, "value_at_risk", &closes, || {
        ValueAtRisk::new(50, 0.95).unwrap()
    });

    // === Family — Microstructure ===
    // No order-book dataset ships with the repo, so synthesise a five-level
    // book around each candle close. Benches the cheapest (top-of-book) and the
    // most-expensive (full-depth sum) representatives of the family.
    let books: Vec<OrderBook> = candles
        .iter()
        .map(|candle| {
            let mid = candle.close;
            let tick = (mid * 0.0001).max(0.01);
            let bids = (0..5u32)
                .map(|i| Level::new_unchecked(mid - tick * f64::from(i + 1), 1.0 + f64::from(i)))
                .collect();
            let asks = (0..5u32)
                .map(|i| Level::new_unchecked(mid + tick * f64::from(i + 1), 1.0 + f64::from(i)))
                .collect();
            OrderBook::new_unchecked(bids, asks)
        })
        .collect();
    bench_orderbook_input(c, "ob_imbalance_top1", &books, OrderBookImbalanceTop1::new);
    bench_orderbook_input(c, "ob_imbalance_full", &books, OrderBookImbalanceFull::new);
    bench_orderbook_input(c, "microprice", &books, Microprice::new);
    bench_orderbook_input(c, "depth_slope", &books, DepthSlope::new);

    // Synthesise a trade tape from candles: one trade per bar, sided by the
    // candle's direction. SignedVolume is the cheapest; TradeImbalance carries
    // a rolling window and is the most expensive.
    let trades: Vec<Trade> = candles
        .iter()
        .map(|candle| {
            let side = if candle.close >= candle.open {
                Side::Buy
            } else {
                Side::Sell
            };
            Trade::new_unchecked(candle.close, candle.volume, side, candle.timestamp)
        })
        .collect();
    bench_trade_input(c, "signed_volume", &trades, SignedVolume::new);
    bench_trade_input(c, "trade_imbalance", &trades, || {
        TradeImbalance::new(50).unwrap()
    });

    // Pair each synthetic trade with the candle close as the prevailing mid to
    // exercise the price-impact family. EffectiveSpread is the stateless
    // representative.
    let quotes: Vec<TradeQuote> = trades
        .iter()
        .map(|trade| TradeQuote::new_unchecked(*trade, trade.price))
        .collect();
    bench_tradequote_input(c, "effective_spread", &quotes, EffectiveSpread::new);
    bench_tradequote_input(c, "kyles_lambda", &quotes, || KylesLambda::new(50).unwrap());

    // === Family — Derivatives ===
    // No derivatives feed ships with the repo, so synthesise a tick per candle:
    // the close drives the mark price, funding tracks the candle's body, and
    // open interest follows volume. FundingRate is the cheapest (passthrough);
    // FundingRateZScore carries a rolling window and is the most expensive.
    let ticks: Vec<DerivativesTick> = candles
        .iter()
        .map(|candle| {
            let funding = (candle.close - candle.open) / candle.open * 0.01;
            DerivativesTick::new_unchecked(
                funding,
                candle.close,
                candle.close * 0.999,
                candle.close * 1.001,
                candle.volume * 100.0,
                candle.volume * 0.6,
                candle.volume * 0.4,
                candle.volume * 0.5,
                candle.volume * 0.5,
                0.0,
                0.0,
                candle.timestamp,
            )
        })
        .collect();
    bench_derivatives_input(c, "funding_rate", &ticks, FundingRate::new);
    bench_derivatives_input(c, "funding_rate_zscore", &ticks, || {
        FundingRateZScore::new(50).unwrap()
    });
}

criterion_group!(name = wickra_benches; config = Criterion::default(); targets = benches);
criterion_main!(wickra_benches);
