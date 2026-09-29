//! Throughput benchmark for the Wickra Rust core — the zero-FFI baseline.
//!
//! Reports streaming (`update`), batch and opt-in fast-batch (`batch_fast`)
//! updates-per-second over a synthetic OHLCV series, in the same format as
//! every binding's `throughput` benchmark. Rust has no FFI boundary — it calls
//! the core directly — so these numbers are the ceiling the per-binding
//! benchmarks are measured against, and the value their `batch` paths converge
//! towards. See the repository BENCHMARKS.md §3.
//!
//! For per-update latency and the cross-library comparison, use the criterion
//! harnesses instead: `cargo bench -p wickra` and `cargo bench -p wickra-bench`.
//!
//! Run:
//!   cargo run -p wickra-examples --release --bin throughput            # 200k bars
//!   cargo run -p wickra-examples --release --bin throughput -- 1000000

use std::time::Instant;

use std::hint::black_box;
use wickra::{Atr, Candle, Indicator, MacdIndicator, Sma};

/// Median elapsed-ns over a few repetitions, after one warmup pass.
fn time_ns(mut run: impl FnMut()) -> u128 {
    run(); // warmup
    let mut samples = [0u128; 3];
    for sample in &mut samples {
        let start = Instant::now();
        run();
        *sample = start.elapsed().as_nanos();
    }
    samples.sort_unstable();
    samples[1]
}

fn main() {
    let bars: usize = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .filter(|&n| n >= 1000)
        .unwrap_or(200_000);

    // Deterministic synthetic OHLCV (no RNG, so runs are comparable).
    let mut open = Vec::with_capacity(bars);
    let mut high = Vec::with_capacity(bars);
    let mut low = Vec::with_capacity(bars);
    let mut close = Vec::with_capacity(bars);
    let mut volume = Vec::with_capacity(bars);
    for i in 0..bars {
        let mid = 100.0 + (i as f64 * 0.001).sin() * 20.0 + i as f64 * 1e-4;
        let c = mid + (i as f64 * 0.05).sin() * 2.0;
        close.push(c);
        open.push(mid);
        high.push(c.max(mid) + 1.5);
        low.push(c.min(mid) - 1.5);
        volume.push(1000.0 + (i % 97) as f64 * 13.0);
    }
    // ATR streams a Candle per tick; build them once, outside the timed loop.
    let candles: Vec<Candle> = (0..bars)
        .map(|i| {
            Candle::new(
                open[i],
                high[i],
                low[i],
                close[i],
                volume[i],
                i64::try_from(i).unwrap(),
            )
            .unwrap()
        })
        .collect();

    // SMA (scalar 1-in/1-out), ATR (multi-in/1-out), MACD (1-in/multi-out).
    // Every batch writes into a buffer reused across runs, as the C ABI's
    // callers do, so the table is the ceiling rather than an allocator test.
    let mut out = vec![0.0; bars];
    let mut rows = vec![0.0; bars * 3];
    let sma_stream = time_ns(|| {
        let mut ind = Sma::new(20).unwrap();
        for &price in &close {
            black_box(ind.update(price));
        }
    });
    let sma_batch = time_ns(|| {
        let mut ind = Sma::new(20).unwrap();
        ind.batch_nan_into(&close, &mut out);
        black_box(&out);
    });
    let sma_fast = time_ns(|| {
        let mut ind = Sma::new(20).unwrap();
        ind.batch_fast_into(&close, &mut out);
        black_box(&out);
    });
    let atr_stream = time_ns(|| {
        let mut ind = Atr::new(14).unwrap();
        for &candle in &candles {
            black_box(ind.update(candle));
        }
    });
    let atr_batch = time_ns(|| {
        let mut ind = Atr::new(14).unwrap();
        ind.batch_atr_into(&high, &low, &close, &mut out);
        black_box(&out);
    });
    let atr_fast = time_ns(|| {
        let mut ind = Atr::new(14).unwrap();
        ind.batch_atr_fast_into(&high, &low, &close, &mut out);
        black_box(&out);
    });
    let macd_stream = time_ns(|| {
        let mut ind = MacdIndicator::new(12, 26, 9).unwrap();
        for &price in &close {
            black_box(ind.update(price));
        }
    });
    let macd_batch = time_ns(|| {
        let mut ind = MacdIndicator::new(12, 26, 9).unwrap();
        ind.batch_macd_into(&close, &mut rows);
        black_box(&rows);
    });
    let macd_fast = time_ns(|| {
        let mut ind = MacdIndicator::new(12, 26, 9).unwrap();
        ind.batch_macd_fast_into(&close, &mut rows);
        black_box(&rows);
    });

    report(
        bars,
        &[
            ("SMA(20)", sma_stream, sma_batch, sma_fast),
            ("ATR(14)", atr_stream, atr_batch, atr_fast),
            ("MACD(12,26,9)", macd_stream, macd_batch, macd_fast),
        ],
    );
}

/// Print the table: per row the streaming, batch and fast-batch timings in ns.
fn report(bars: usize, rows: &[(&str, u128, u128, u128)]) {
    let mups = |ns: u128| bars as f64 / (ns as f64 / 1e9) / 1e6;
    println!("Wickra Rust core throughput - {bars} bars (median of 3 runs)\n");
    println!(
        "{:<22}{:>20}{:>18}{:>18}",
        "Indicator", "streaming (Mupd/s)", "batch (Mupd/s)", "fast (Mupd/s)"
    );
    println!("{}", "-".repeat(78));
    for &(name, stream, batch, fast) in rows {
        println!(
            "{:<22}{:>20.1}{:>18.1}{:>18.1}",
            name,
            mups(stream),
            mups(batch),
            mups(fast)
        );
    }
    println!(
        "\nMupd/s = million indicator updates per second. This is the Rust core with\n\
         no FFI boundary, so it is the ceiling for the per-binding benchmarks and\n\
         the value their batch paths converge towards. \"fast\" is the opt-in\n\
         batch_fast: SIMD kernels within a few units in the last place of batch.\n\
         Numbers are machine-dependent - use them for relative comparison, not as\n\
         a speed claim."
    );
}
