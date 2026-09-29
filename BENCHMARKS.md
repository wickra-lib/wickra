# Benchmarks

Read these as **relative** speedups on identical input — absolute µs depend on
CPU, memory clock and OS scheduler, not a universal contract. **Streaming is the
headline**: it is where Wickra's design pays off and where the gap is measured in
orders of magnitude, not percent. The batch numbers are shown in full: the exact
`batch` — bit for bit what streaming gives — and the opt-in `batch_fast`, whose
SIMD kernels reorder the arithmetic and agree with it to within a few units in
the last place.

- **Reproduced on:** Windows 11 Pro 26200, AMD Ryzen 9 9950X, 64 GB DDR5,
  Rust 1.92 (release: `lto = "fat"`, `codegen-units = 1`), Python 3.12. Every
  number in a table comes from the same session; the Rust tables are the better
  of two criterion runs.
- **Reproduce yourself:**
  - Rust core vs Rust crates: `cargo bench -p wickra-bench`
  - Python vs Python libs: `pip install -e bindings/python[bench]` then
    `python -m benchmarks.compare_libraries` (auto-detects installed peers).

## 1. Streaming — the structural win

Live trading feeds one tick at a time. Wickra updates every indicator
incrementally, with no pass over the history behind the tick;
batch-only libraries (TA-Lib, tulipy, finta, pandas-ta) have no incremental API
and must recompute the whole history on every tick. Only `talipp` (Python) and
`ta-rs` / `yata` (Rust) carry real per-tick state. This is the gap the library
was built to expose.

**Python — per-tick latency** (seed 5 000 bars, then feed 10 000 ticks one at a
time):

| Indicator        | **★&nbsp;Wickra** | talipp           | TA-Lib (recompute)    |
|------------------|------------------:|------------------|-----------------------|
| SMA(20)          | **0.070 µs ★**    | 0.54 µs (8×)     | 225 µs (3 200×)       |
| EMA(20)          | **0.074 µs ★**    | 0.85 µs (12×)    | 236 µs (3 200×)       |
| RSI(14)          | **0.113 µs ★**    | 1.33 µs (12×)    | 259 µs (2 300×)       |
| MACD(12, 26, 9)  | **0.121 µs ★**    | 4.37 µs (36×)    | 283 µs (2 300×)       |
| Bollinger(20, 2) | **0.102 µs ★**    | 6.75 µs (66×)    | 290 µs (2 800×)       |

Against the only other incremental Python peer Wickra is **8–66× faster**;
against the recompute-on-every-tick libraries it is **1 600–10 500× faster**
(`finta` Bollinger hits 10 500×). tulipy / pandas-ta land in the same recompute
band as TA-Lib.

**Rust — per-tick latency** (whole 50 000-bar series, µs, lower = faster):

| Indicator        | **★&nbsp;Wickra** | kand    | ta-rs  | yata   |
|------------------|------------------:|--------:|-------:|-------:|
| SMA(20)          | 51                | 37      | 45     | **30** |
| EMA(20)          | 70                | 68      | **54** | 70     |
| RSI(14)          | 170               | 211     | **73** | —      |
| MACD(12, 26, 9)  | 226               | 165     | **62** | —      |
| Bollinger(20, 2) | 175               | 275     | **154** | —     |
| ATR(14)          | 84                | 156     | **62** | —      |

`ta-rs` hands back a bare `f64` from the first tick with no warmup and no
validation; it leads the table by giving those guarantees up. Against `kand`,
Wickra wins streaming RSI, Bollinger and ATR and ties EMA. `yata` exposes only
SMA/EMA as raw-value methods, so its other rows are omitted rather than faked.

## 2. Batch — the exact batch, and the opt-in fast one

Whole series in one call. `batch` is bit for bit what `update` gives — the hot
indicators run fused paths that perform the same arithmetic in the same order —
and `batch_fast` runs SIMD kernels within a few units in the last place of it,
with identical `NaN` placement and the same result on every platform.

**Python** (20 000-bar pass, µs/op, lower = faster):

| Indicator        | Wickra | **Wickra fast** | TA-Lib | tulipy   | pandas-ta | finta  |
|------------------|-------:|----------------:|-------:|---------:|----------:|-------:|
| SMA(20)          | 21.7   | **10.4 ★**      | 15.2   | 15.8     | 32.6      | 269.8  |
| EMA(20)          | 33.9   | **10.6 ★**      | 29.2   | 29.6     | 51.2      | 195.4  |
| RSI(14)          | 36.4   | **21.3 ★**      | 69.8   | 35.3     | 107.6     | 792.0  |
| MACD(12, 26, 9)  | 36.0   | **26.1 ★**      | 96.1   | 32.5     | 203.4     | 503.1  |
| Bollinger(20, 2) | 71.6   | 36.4            | 68.4   | **35.9** | 397.0     | 753.5  |
| ATR(14)          | 49.3   | 34.0            | 76.8   | **30.8** | —         | 2094.3 |

The fast batch leads four of six rows outright and is within 1.5 % and 11 % of
tulipy's SIMD C on the other two; the exact batch beats TA-Lib on RSI, MACD and
ATR and every row of pandas-ta and finta.

**Rust** (50 000-bar pass, µs, lower = faster, into a caller buffer on both
sides). Only Wickra and `kand` expose a batch API; `ta-rs` and `yata` are
streaming-only:

| Indicator        | Wickra     | **Wickra fast** | kand    |
|------------------|-----------:|----------------:|--------:|
| SMA(20)          | 47         | **21 ★**        | 39      |
| EMA(20)          | 75         | **16 ★**        | 67      |
| RSI(14)          | 90         | **46 ★**        | 221     |
| MACD(12, 26, 9)  | 82         | **58 ★**        | 228     |
| Bollinger(20, 2) | 194        | **93 ★**        | 346     |
| ATR(14)          | 71         | **38 ★**        | 157     |

The fast batch wins every row; the exact batch wins RSI, MACD, Bollinger and
ATR by 1.8–2.8× and trails `kand` by a few µs on the two pure recurrences, SMA
and EMA, where keeping streaming's bits fixes the order of the additions.

Run the suite yourself:

```bash
cargo bench -p wickra-bench            # Rust core vs kand / ta-rs / yata
pip install -e bindings/python[bench]  # Python peers
python -m benchmarks.compare_libraries
```

## 3. Per-binding throughput — the cost of the boundary

The sections above compare Wickra against other libraries, which only exists for
Python and Rust (there is no comparable streaming TA library for C, C++, C#, Go, Java,
R or WASM to benchmark against). Every binding calls the **same** Rust
core, so these per-binding benchmarks are **not** a speed claim and **not** a
cross-library ratio — they document the raw cost of crossing each language's FFI
boundary, in million updates per second (Mupd/s).

Each binding ships a small `throughput` benchmark that feeds a synthetic OHLCV
series through three indicators chosen by call-signature archetype — `SMA(20)`
(1-in → 1-out), `ATR(14)` (multi-in → 1-out) and `MACD(12,26,9)` (1-in →
multi-out) — streaming, as a `batch`, and as the opt-in `batch_fast` (SIMD
kernels within a few units in the last place of `batch`; see
[the README](README.md#benchmarks)). Three things fall out of the numbers:

- **Batch crosses once.** A `batch` call crosses the boundary a single time and the
  Rust core computes the whole series internally, so batch throughput stays high
  for every binding that hands back a contiguous buffer. Node is the exception:
  its `batch` still returns a JS `Array`, boxing every element — `batchFast`
  returns a `Float64Array` instead and runs 110× faster.
- **A reused buffer is worth as much as the kernel.** Writing a fresh multi-megabyte
  result costs page faults on the order of the computation, so the caller-buffer
  forms — C#'s `Span` overloads, Go's `BatchInto`, Java's `batchInto` over native
  `MemorySegment`s, the C ABI itself — reach the Rust ceiling.
- **Streaming reveals the boundary.** A per-tick `update` crosses the boundary
  once per value, so streaming throughput is where the bindings differ: the raw C
  ABI is nearly free, while managed or interpreted per-call marshalling (P/Invoke's
  `SafeHandle`, cgo, FFM, napi, the R/WASM boundary) costs more per tick.

The Rust core ships the same benchmark with **no** FFI boundary
(`examples/rust/.../throughput.rs`) — it is the ceiling each binding is measured
against, its batches writing into a reused buffer as the C ABI's callers do.

`SMA(20)`, 200 000 bars, the better of two runs (each the median of 3), on the
reference machine (Windows 11, AMD Ryzen 9 9950X), all targets measured in one
session:

| Target               | streaming | batch  | fast batch | fast into a reused buffer |
|----------------------|----------:|-------:|-----------:|--------------------------:|
| Rust core (no FFI)   |     1 374 | 1 151¹ |    3 115¹  |                     3 115 |
| C / C++              |       399 | 1 126¹ |    3 160¹  |                     3 160 |
| C#                   |        63 |    744 |      1 409 |       3 145 (`Span<double>`) |
| Go                   |        24 |  1 046 |      2 435 |          3 005 (`BatchFastInto`) |
| Java                 |        64 |    314 |        367 |     2 744 (`MemorySegment`) |
| R                    |       0.1 |    601 |      1 021 |                         — |
| WASM                 |        34 |    424 |        406 |                         — |
| Python               |        29 |    248 |        314 |                         — |
| Node.js              |       5.4 |  11 ²  |      1 255 |                         — |

¹ Into a reused buffer — the Rust benchmark and the C ABI have no allocating form.
² A plain JS `Array` in and out; from a `Float64Array` read in place, 19.

`ATR(14)` and `MACD(12,26,9)` follow the same shape at lower rates, their kernels
being recurrences rather than window sums (Rust core, fast batch into a reused
buffer: ATR 1 457, MACD 865; batch 705 and 640). The managed runtimes' streaming
rates are what their call boundary costs on this machine today — the previous
release measures the same (C#: 63 streaming, 297 batch).

These are throughput numbers, not competitive numbers — the "Wickra is fast"
claim lives in sections 1 and 2 (Rust core + the Python/Rust cross-library runs).

Run any target's benchmark (build the C ABI library first where it links one):

```bash
cargo run -p wickra-examples --release --bin throughput           # Rust core baseline (no FFI)

node bindings/node/benchmarks/throughput.js                       # native napi-rs
( cd bindings/python && python -m benchmarks.throughput )         # native PyO3
( cd bindings/wasm && wasm-pack build --target nodejs --out-dir pkg-node --release ) \
  && node bindings/wasm/benchmarks/throughput.mjs                 # wasm boundary

cargo build -p wickra-c --release                                 # the C ABI hub
cmake -S bindings/c/benchmarks -B build/cbench && cmake --build build/cbench \
  && ./build/cbench/throughput                                    # raw C ABI
dotnet run -c Release --project bindings/csharp/benchmarks        # C# (P/Invoke)
( cd bindings/go/benchmarks && go run . )                         # Go (cgo)
mvn -q -f bindings/java install -DskipTests \
  && mvn -q -f bindings/java/benchmarks compile exec:exec -Dexec.mainClass=org.wickra.benchmarks.Throughput
Rscript bindings/r/benchmarks/throughput.R                        # R (.Call)
```

## 4. Data layer — native I/O throughput

Wickra ships its own data layer — a CSV candle reader, a tick-to-candle
aggregator, and a timeframe resampler — so loading and reshaping market data
needs **no third-party package** (`pandas`, `csv-parse`, manual bucketing,
`pandas.resample`, …) in any of the ten languages. These run on the same Rust
core as the indicators, so every binding reaches these speeds minus the FFI
boundary characterised in section 3 (a `read` / `push` / `flush` call crosses the
boundary once per batch, like `batch`, so bindings land close to the core).

Rust core, 50 000 real BTCUSDT one-minute candles
(`examples/data/btcusdt-1m.csv`), median of 100 samples, on the reference machine
(Windows 11, AMD Ryzen 9 9950X):

| Operation                          | Throughput          | Per element |
|------------------------------------|--------------------:|------------:|
| CSV parse (`CandleReader`)         |   3.0 M candles/s   |      329 ns |
| Tick aggregate → 1m (`TickAggregator`) |  44 M ticks/s   |     22.6 ns |
| Resample 1m → 5m (`Resampler`)     | 234 M candles/s     |      4.3 ns |

Reading and validating a 50 000-row CSV into typed candles takes ~16 ms;
aggregating 50 000 ticks into one-minute bars ~1.1 ms; resampling 50 000
one-minute candles to five-minute bars ~0.2 ms. CSV parsing is the floor because
it does the most per row (UTF-8 scan, field split, six `f64` parses, finiteness
checks); aggregation and resampling are pure arithmetic over already-typed
candles.

The live and historical Binance feeds (`BinanceFeed`, `fetch_binance_klines`) are
network-bound — their throughput is set by the exchange and the socket, not by
Wickra — so they are not micro-benchmarked here; the relevant Wickra cost is the
per-event parse, which is the same arithmetic measured above.

Run it with:

```bash
cargo bench -p wickra --bench data_layer
```
