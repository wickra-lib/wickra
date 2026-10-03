//! Cross-library Criterion benchmark: Wickra vs `kand` vs `ta` (ta-rs) vs `yata`.
//!
//! All four are pure-Rust technical-analysis crates, so this is a like-for-like
//! Rust-vs-Rust comparison with no language-binding overhead. It feeds the exact
//! same BTCUSDT 1-minute candle series used by `crates/wickra/benches/indicators.rs`.
//!
//! Two arenas, kept honest:
//!
//! * **Streaming** (`*/stream`): one value fed at a time. Wickra (`Indicator::update`),
//!   ta-rs (`Next::next`) and yata (`Method::next`) carry their own state; `kand`
//!   exposes stateless `*_inc` helpers, so the per-tick state is threaded manually
//!   here, seeded from `kand`'s own batch output (the seed is computed outside the
//!   timed closure). yata only appears for SMA/EMA — its RSI/MACD/Bollinger/ATR are
//!   exposed through a heavier signal-oriented indicator API, not a raw-value method,
//!   so they are intentionally left out rather than compared unfairly.
//! * **Batch** (`*/batch`): the whole series at once, into an output buffer
//!   allocated outside the timed loop. Only Wickra (the exact `*_into` batches,
//!   bit for bit the streaming result) and `kand` (TA-Lib-style
//!   fill-the-output-slice functions) have a real batch API; ta-rs and yata are
//!   streaming-only and are deliberately absent from this arena.
//! * **Fast batch** (`wickra/fast`): Wickra's opt-in `batch_fast` SIMD kernels,
//!   within a few units in the last place of the exact batch, into the same
//!   kind of buffer.
//!
//! Run: `cargo bench -p wickra-bench`

// Each indicator's benchmark group spells out every library arm explicitly, which
// runs a few groups over the 100-line lint threshold; that verbosity is the point.
#![allow(clippy::too_many_lines)]

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use wickra::{Atr, BollingerBands, Candle, Ema, Indicator, MacdIndicator, Rsi, Sma};
use wickra_data::csv::CandleReader;
use yata::prelude::Method;

const SIZES: &[usize] = &[1_000, 10_000, 50_000];

const SMA_PERIOD: usize = 20;
const EMA_PERIOD: usize = 20;
const RSI_PERIOD: usize = 14;
const ATR_PERIOD: usize = 14;
const BB_PERIOD: usize = 20;
const BB_DEV: f64 = 2.0;
const MACD_FAST: usize = 12;
const MACD_SLOW: usize = 26;
const MACD_SIGNAL: usize = 9;

fn load_candles() -> Vec<Candle> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/data/btcusdt-1m.csv"
    );
    CandleReader::open(path)
        .expect("dataset present")
        .read_all()
        .expect("valid OHLCV rows")
}

/// Mean of the first `period` samples — the warmup seed for `kand`'s SMA/EMA `*_inc`.
fn window_mean(series: &[f64], period: usize) -> f64 {
    series[..period].iter().sum::<f64>() / period as f64
}

fn sma_group(crit: &mut Criterion, closes: &[f64]) {
    let mut group = crit.benchmark_group("sma_20");
    for &len in SIZES {
        let len = len.min(closes.len());
        let series: &[f64] = &closes[..len];
        group.throughput(Throughput::Elements(len as u64));

        group.bench_with_input(
            BenchmarkId::new("wickra/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind = Sma::new(SMA_PERIOD).unwrap();
                    for &price in series {
                        black_box(ind.update(price));
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("wickra/batch", len),
            &series,
            |bencher, &series| {
                // Into a buffer allocated outside the timed loop, as kand's arm
                // fills the caller's output slice.
                let mut out = vec![0.0; series.len()];
                bencher.iter(|| {
                    let mut ind = Sma::new(SMA_PERIOD).unwrap();
                    ind.batch_nan_into(series, &mut out);
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("wickra/fast", len),
            &series,
            |bencher, &series| {
                let mut out = vec![0.0; series.len()];
                bencher.iter(|| {
                    let mut ind = Sma::new(SMA_PERIOD).unwrap();
                    ind.batch_fast_into(series, &mut out);
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("kand/stream", len),
            &series,
            |bencher, &series| {
                let seed = window_mean(series, SMA_PERIOD);
                bencher.iter(|| {
                    let mut prev = seed;
                    for idx in SMA_PERIOD..series.len() {
                        prev = kand::ohlcv::sma::sma_inc(
                            prev,
                            series[idx],
                            series[idx - SMA_PERIOD],
                            SMA_PERIOD,
                        )
                        .unwrap();
                        black_box(prev);
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("kand/batch", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut out = vec![0.0; series.len()];
                    kand::ohlcv::sma::sma(series, SMA_PERIOD, &mut out).unwrap();
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("ta-rs/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind = ta::indicators::SimpleMovingAverage::new(SMA_PERIOD).unwrap();
                    for &price in series {
                        black_box(ta::Next::next(&mut ind, price));
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("yata/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind = yata::methods::SMA::new(SMA_PERIOD as u8, &series[0]).unwrap();
                    for price in series {
                        black_box(ind.next(price));
                    }
                });
            },
        );
    }
    group.finish();
}

fn ema_group(crit: &mut Criterion, closes: &[f64]) {
    let mut group = crit.benchmark_group("ema_20");
    for &len in SIZES {
        let len = len.min(closes.len());
        let series: &[f64] = &closes[..len];
        group.throughput(Throughput::Elements(len as u64));

        group.bench_with_input(
            BenchmarkId::new("wickra/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind = Ema::new(EMA_PERIOD).unwrap();
                    for &price in series {
                        black_box(ind.update(price));
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("wickra/batch", len),
            &series,
            |bencher, &series| {
                // Into a buffer allocated outside the timed loop, as kand's arm
                // fills the caller's output slice.
                let mut out = vec![0.0; series.len()];
                bencher.iter(|| {
                    let mut ind = Ema::new(EMA_PERIOD).unwrap();
                    ind.batch_nan_into(series, &mut out);
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("wickra/fast", len),
            &series,
            |bencher, &series| {
                let mut out = vec![0.0; series.len()];
                bencher.iter(|| {
                    let mut ind = Ema::new(EMA_PERIOD).unwrap();
                    ind.batch_fast_into(series, &mut out);
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("kand/stream", len),
            &series,
            |bencher, &series| {
                let seed = window_mean(series, EMA_PERIOD);
                bencher.iter(|| {
                    let mut prev = seed;
                    for &price in &series[EMA_PERIOD..] {
                        prev = kand::ohlcv::ema::ema_inc(price, prev, EMA_PERIOD, None).unwrap();
                        black_box(prev);
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("kand/batch", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut out = vec![0.0; series.len()];
                    kand::ohlcv::ema::ema(series, EMA_PERIOD, None, &mut out).unwrap();
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("ta-rs/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind =
                        ta::indicators::ExponentialMovingAverage::new(EMA_PERIOD).unwrap();
                    for &price in series {
                        black_box(ta::Next::next(&mut ind, price));
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("yata/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind = yata::methods::EMA::new(EMA_PERIOD as u8, &series[0]).unwrap();
                    for price in series {
                        black_box(ind.next(price));
                    }
                });
            },
        );
    }
    group.finish();
}

fn rsi_group(crit: &mut Criterion, closes: &[f64]) {
    let mut group = crit.benchmark_group("rsi_14");
    for &len in SIZES {
        let len = len.min(closes.len());
        let series: &[f64] = &closes[..len];
        group.throughput(Throughput::Elements(len as u64));

        group.bench_with_input(
            BenchmarkId::new("wickra/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind = Rsi::new(RSI_PERIOD).unwrap();
                    for &price in series {
                        black_box(ind.update(price));
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("wickra/batch", len),
            &series,
            |bencher, &series| {
                // Into a buffer allocated outside the timed loop, as kand's arm
                // fills the caller's output slice.
                let mut out = vec![0.0; series.len()];
                bencher.iter(|| {
                    let mut ind = Rsi::new(RSI_PERIOD).unwrap();
                    ind.batch_nan_into(series, &mut out);
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("wickra/fast", len),
            &series,
            |bencher, &series| {
                let mut out = vec![0.0; series.len()];
                bencher.iter(|| {
                    let mut ind = Rsi::new(RSI_PERIOD).unwrap();
                    ind.batch_fast_into(series, &mut out);
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("kand/stream", len),
            &series,
            |bencher, &series| {
                // Wilder seed: simple average of the first `period` gains and losses.
                let mut gain = 0.0;
                let mut loss = 0.0;
                for idx in 1..=RSI_PERIOD {
                    let delta = series[idx] - series[idx - 1];
                    if delta > 0.0 {
                        gain += delta;
                    } else {
                        loss -= delta;
                    }
                }
                let seed_gain = gain / RSI_PERIOD as f64;
                let seed_loss = loss / RSI_PERIOD as f64;
                bencher.iter(|| {
                    let mut avg_gain = seed_gain;
                    let mut avg_loss = seed_loss;
                    let mut prev_price = series[RSI_PERIOD];
                    for &price in &series[RSI_PERIOD + 1..] {
                        let (rsi, next_gain, next_loss) = kand::ohlcv::rsi::rsi_inc(
                            price, prev_price, avg_gain, avg_loss, RSI_PERIOD,
                        )
                        .unwrap();
                        avg_gain = next_gain;
                        avg_loss = next_loss;
                        prev_price = price;
                        black_box(rsi);
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("kand/batch", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut rsi = vec![0.0; series.len()];
                    let mut avg_gain = vec![0.0; series.len()];
                    let mut avg_loss = vec![0.0; series.len()];
                    kand::ohlcv::rsi::rsi(
                        series,
                        RSI_PERIOD,
                        &mut rsi,
                        &mut avg_gain,
                        &mut avg_loss,
                    )
                    .unwrap();
                    black_box(&rsi);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("ta-rs/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind = ta::indicators::RelativeStrengthIndex::new(RSI_PERIOD).unwrap();
                    for &price in series {
                        black_box(ta::Next::next(&mut ind, price));
                    }
                });
            },
        );
    }
    group.finish();
}

fn macd_group(crit: &mut Criterion, closes: &[f64]) {
    let mut group = crit.benchmark_group("macd_12_26_9");
    for &len in SIZES {
        let len = len.min(closes.len());
        let series: &[f64] = &closes[..len];
        group.throughput(Throughput::Elements(len as u64));

        group.bench_with_input(
            BenchmarkId::new("wickra/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind = MacdIndicator::classic();
                    for &price in series {
                        black_box(ind.update(price));
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("wickra/batch", len),
            &series,
            |bencher, &series| {
                // Into a buffer allocated outside the timed loop, as kand's arm
                // fills the caller's output slice.
                let mut out = vec![0.0; series.len() * 3];
                bencher.iter(|| {
                    let mut ind = MacdIndicator::classic();
                    ind.batch_macd_into(series, &mut out);
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("wickra/fast", len),
            &series,
            |bencher, &series| {
                let mut out = vec![0.0; series.len() * 3];
                bencher.iter(|| {
                    let mut ind = MacdIndicator::classic();
                    ind.batch_macd_fast_into(series, &mut out);
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("kand/stream", len),
            &series,
            |bencher, &series| {
                // Seed the fast/slow/signal EMAs from kand's own warmed-up batch state.
                let lookback =
                    kand::ohlcv::macd::lookback(MACD_FAST, MACD_SLOW, MACD_SIGNAL).unwrap();
                let mut macd_line = vec![0.0; series.len()];
                let mut signal_line = vec![0.0; series.len()];
                let mut histogram = vec![0.0; series.len()];
                let mut fast_ema = vec![0.0; series.len()];
                let mut slow_ema = vec![0.0; series.len()];
                kand::ohlcv::macd::macd(
                    series,
                    MACD_FAST,
                    MACD_SLOW,
                    MACD_SIGNAL,
                    &mut macd_line,
                    &mut signal_line,
                    &mut histogram,
                    &mut fast_ema,
                    &mut slow_ema,
                )
                .unwrap();
                let seed_fast = fast_ema[lookback];
                let seed_slow = slow_ema[lookback];
                let seed_signal = signal_line[lookback];
                bencher.iter(|| {
                    // macd_inc returns (macd, signal, hist) but not the new EMAs, so the
                    // fast/slow/signal state is threaded with kand's own ema_inc primitive.
                    let mut prev_fast = seed_fast;
                    let mut prev_slow = seed_slow;
                    let mut prev_signal = seed_signal;
                    for &price in &series[lookback + 1..] {
                        let fast =
                            kand::ohlcv::ema::ema_inc(price, prev_fast, MACD_FAST, None).unwrap();
                        let slow =
                            kand::ohlcv::ema::ema_inc(price, prev_slow, MACD_SLOW, None).unwrap();
                        let macd = fast - slow;
                        let signal =
                            kand::ohlcv::ema::ema_inc(macd, prev_signal, MACD_SIGNAL, None)
                                .unwrap();
                        prev_fast = fast;
                        prev_slow = slow;
                        prev_signal = signal;
                        black_box((macd, signal, macd - signal));
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("kand/batch", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut macd_line = vec![0.0; series.len()];
                    let mut signal_line = vec![0.0; series.len()];
                    let mut histogram = vec![0.0; series.len()];
                    let mut fast_ema = vec![0.0; series.len()];
                    let mut slow_ema = vec![0.0; series.len()];
                    kand::ohlcv::macd::macd(
                        series,
                        MACD_FAST,
                        MACD_SLOW,
                        MACD_SIGNAL,
                        &mut macd_line,
                        &mut signal_line,
                        &mut histogram,
                        &mut fast_ema,
                        &mut slow_ema,
                    )
                    .unwrap();
                    black_box(&macd_line);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("ta-rs/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind = ta::indicators::MovingAverageConvergenceDivergence::new(
                        MACD_FAST,
                        MACD_SLOW,
                        MACD_SIGNAL,
                    )
                    .unwrap();
                    for &price in series {
                        black_box(ta::Next::next(&mut ind, price));
                    }
                });
            },
        );
    }
    group.finish();
}

fn bbands_group(crit: &mut Criterion, closes: &[f64]) {
    let mut group = crit.benchmark_group("bollinger_20_2");
    for &len in SIZES {
        let len = len.min(closes.len());
        let series: &[f64] = &closes[..len];
        group.throughput(Throughput::Elements(len as u64));

        group.bench_with_input(
            BenchmarkId::new("wickra/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind = BollingerBands::new(BB_PERIOD, BB_DEV).unwrap();
                    for &price in series {
                        black_box(ind.update(price));
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("wickra/batch", len),
            &series,
            |bencher, &series| {
                // Into a buffer allocated outside the timed loop, as kand's arm
                // fills the caller's output slice.
                let mut out = vec![0.0; series.len() * 4];
                bencher.iter(|| {
                    let mut ind = BollingerBands::new(BB_PERIOD, BB_DEV).unwrap();
                    ind.batch_bands_into(series, &mut out);
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("wickra/fast", len),
            &series,
            |bencher, &series| {
                let mut out = vec![0.0; series.len() * 4];
                bencher.iter(|| {
                    let mut ind = BollingerBands::new(BB_PERIOD, BB_DEV).unwrap();
                    ind.batch_bands_fast_into(series, &mut out);
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("kand/stream", len),
            &series,
            |bencher, &series| {
                // Seed running sma/sum/sum_sq from kand's batch state at the warmup edge.
                let mut upper = vec![0.0; series.len()];
                let mut middle = vec![0.0; series.len()];
                let mut lower = vec![0.0; series.len()];
                let mut sma = vec![0.0; series.len()];
                let mut variance = vec![0.0; series.len()];
                let mut sum = vec![0.0; series.len()];
                let mut sum_sq = vec![0.0; series.len()];
                kand::ohlcv::bbands::bbands(
                    series,
                    BB_PERIOD,
                    BB_DEV,
                    BB_DEV,
                    &mut upper,
                    &mut middle,
                    &mut lower,
                    &mut sma,
                    &mut variance,
                    &mut sum,
                    &mut sum_sq,
                )
                .unwrap();
                let seed_sma = sma[BB_PERIOD - 1];
                let seed_sum = sum[BB_PERIOD - 1];
                let seed_sum_sq = sum_sq[BB_PERIOD - 1];
                bencher.iter(|| {
                    let mut prev_sma = seed_sma;
                    let mut prev_sum = seed_sum;
                    let mut prev_sum_sq = seed_sum_sq;
                    for idx in BB_PERIOD..series.len() {
                        let result = kand::ohlcv::bbands::bbands_inc(
                            series[idx],
                            prev_sma,
                            prev_sum,
                            prev_sum_sq,
                            series[idx - BB_PERIOD],
                            BB_PERIOD,
                            BB_DEV,
                            BB_DEV,
                        )
                        .unwrap();
                        prev_sma = result.1;
                        prev_sum = result.4;
                        prev_sum_sq = result.5;
                        black_box((result.0, result.1, result.2));
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("kand/batch", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut upper = vec![0.0; series.len()];
                    let mut middle = vec![0.0; series.len()];
                    let mut lower = vec![0.0; series.len()];
                    let mut sma = vec![0.0; series.len()];
                    let mut variance = vec![0.0; series.len()];
                    let mut sum = vec![0.0; series.len()];
                    let mut sum_sq = vec![0.0; series.len()];
                    kand::ohlcv::bbands::bbands(
                        series,
                        BB_PERIOD,
                        BB_DEV,
                        BB_DEV,
                        &mut upper,
                        &mut middle,
                        &mut lower,
                        &mut sma,
                        &mut variance,
                        &mut sum,
                        &mut sum_sq,
                    )
                    .unwrap();
                    black_box(&upper);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("ta-rs/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind = ta::indicators::BollingerBands::new(BB_PERIOD, BB_DEV).unwrap();
                    for &price in series {
                        black_box(ta::Next::next(&mut ind, price));
                    }
                });
            },
        );
    }
    group.finish();
}

fn atr_group(crit: &mut Criterion, candles: &[Candle]) {
    let mut group = crit.benchmark_group("atr_14");
    for &len in SIZES {
        let len = len.min(candles.len());
        let series: &[Candle] = &candles[..len];
        group.throughput(Throughput::Elements(len as u64));

        group.bench_with_input(
            BenchmarkId::new("wickra/stream", len),
            &series,
            |bencher, &series| {
                bencher.iter(|| {
                    let mut ind = Atr::new(ATR_PERIOD).unwrap();
                    for &candle in series {
                        black_box(ind.update(candle));
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("wickra/batch", len),
            &series,
            |bencher, &series| {
                // Column extraction is outside the timed loop, mirroring kand's arm.
                let high: Vec<f64> = series.iter().map(|candle| candle.high).collect();
                let low: Vec<f64> = series.iter().map(|candle| candle.low).collect();
                let close: Vec<f64> = series.iter().map(|candle| candle.close).collect();
                let mut out = vec![0.0; series.len()];
                bencher.iter(|| {
                    let mut ind = Atr::new(ATR_PERIOD).unwrap();
                    ind.batch_atr_into(&high, &low, &close, &mut out);
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("wickra/fast", len),
            &series,
            |bencher, &series| {
                let high: Vec<f64> = series.iter().map(|candle| candle.high).collect();
                let low: Vec<f64> = series.iter().map(|candle| candle.low).collect();
                let close: Vec<f64> = series.iter().map(|candle| candle.close).collect();
                let mut out = vec![0.0; series.len()];
                bencher.iter(|| {
                    let mut ind = Atr::new(ATR_PERIOD).unwrap();
                    ind.batch_atr_fast_into(&high, &low, &close, &mut out);
                    black_box(&out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("kand/stream", len),
            &series,
            |bencher, &series| {
                let high: Vec<f64> = series.iter().map(|candle| candle.high).collect();
                let low: Vec<f64> = series.iter().map(|candle| candle.low).collect();
                let close: Vec<f64> = series.iter().map(|candle| candle.close).collect();
                // Seed prev_atr from kand's batch ATR at the first valid index (= period).
                let mut atr_out = vec![0.0; series.len()];
                kand::ohlcv::atr::atr(&high, &low, &close, ATR_PERIOD, &mut atr_out).unwrap();
                let seed_atr = atr_out[ATR_PERIOD];
                bencher.iter(|| {
                    let mut prev_atr = seed_atr;
                    for idx in ATR_PERIOD + 1..series.len() {
                        prev_atr = kand::ohlcv::atr::atr_inc(
                            high[idx],
                            low[idx],
                            close[idx - 1],
                            prev_atr,
                            ATR_PERIOD,
                        )
                        .unwrap();
                        black_box(prev_atr);
                    }
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("kand/batch", len),
            &series,
            |bencher, &series| {
                let high: Vec<f64> = series.iter().map(|candle| candle.high).collect();
                let low: Vec<f64> = series.iter().map(|candle| candle.low).collect();
                let close: Vec<f64> = series.iter().map(|candle| candle.close).collect();
                bencher.iter(|| {
                    let mut atr_out = vec![0.0; series.len()];
                    kand::ohlcv::atr::atr(&high, &low, &close, ATR_PERIOD, &mut atr_out).unwrap();
                    black_box(&atr_out);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("ta-rs/stream", len),
            &series,
            |bencher, &series| {
                let items: Vec<ta::DataItem> = series
                    .iter()
                    .map(|candle| {
                        ta::DataItem::builder()
                            .open(candle.open)
                            .high(candle.high)
                            .low(candle.low)
                            .close(candle.close)
                            .volume(candle.volume)
                            .build()
                            .unwrap()
                    })
                    .collect();
                bencher.iter(|| {
                    let mut ind = ta::indicators::AverageTrueRange::new(ATR_PERIOD).unwrap();
                    for item in &items {
                        black_box(ta::Next::next(&mut ind, item));
                    }
                });
            },
        );
    }
    group.finish();
}

fn benches(crit: &mut Criterion) {
    let candles = load_candles();
    let closes: Vec<f64> = candles.iter().map(|candle| candle.close).collect();
    sma_group(crit, &closes);
    ema_group(crit, &closes);
    rsi_group(crit, &closes);
    macd_group(crit, &closes);
    bbands_group(crit, &closes);
    atr_group(crit, &candles);
}

criterion_group!(name = cross_lib; config = Criterion::default(); targets = benches);
criterion_main!(cross_lib);
