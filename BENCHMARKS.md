# Benchmarks

Read these as **relative** speedups on identical input — absolute µs depend on
CPU, memory clock and OS scheduler, not a universal contract. **Streaming is the
headline**: it is where Wickra's design pays off and where the gap is measured in
orders of magnitude, not percent. The batch numbers are shown in full: the exact
`batch` — bit for bit what streaming gives — and the opt-in `batch_fast`, whose
SIMD kernels reorder the arithmetic and agree with it to within a few units in
the last place.

- **Reproduced on:** Windows 11 Pro 26200, AMD Ryzen 9 9950X, 64 GB DDR5,
  Rust 1.92 (release: `lto = "fat"`, `codegen-units = 1`), Python 3.12, .NET 10.
  Every number in a table comes from the same session; the Rust tables are the
  better of two criterion runs.
- **Reproduce yourself:**
  - Rust core vs Rust crates: `cargo bench -p wickra-bench`
  - Python vs Python libs: `pip install -e bindings/python[bench]` then
    `python -m benchmarks.compare_libraries` (auto-detects installed peers).
  - .NET vs .NET libs: `dotnet run -c Release --project bindings/csharp/cross-library`.

## 1. Streaming — the structural win

Live trading feeds one tick at a time. Wickra updates every indicator
incrementally, with no pass over the history behind the tick;
batch-only libraries (TA-Lib, tulipy, finta, pandas-ta) have no incremental API
and must recompute the whole history on every tick. Only `talipp` (Python),
`ta-rs` / `yata` (Rust) and QuanTAlib (.NET, section 2) carry real per-tick
state. This is the gap the library was built to expose.

**Python — per-tick latency** (seed 5 000 bars, then feed 10 000 ticks one at a
time):

| Indicator        | **★&nbsp;Wickra** | talipp           | TA-Lib (recompute)    |
|------------------|------------------:|------------------|-----------------------|
| SMA(20)          | **0.061 µs ★**    | 0.54 µs (9×)     | 234 µs (3 800×)       |
| EMA(20)          | **0.064 µs ★**    | 0.73 µs (11×)    | 242 µs (3 800×)       |
| RSI(14)          | **0.068 µs ★**    | 1.11 µs (16×)    | 265 µs (3 900×)       |
| MACD(12, 26, 9)  | **0.084 µs ★**    | 4.30 µs (51×)    | 278 µs (3 300×)       |
| Bollinger(20, 2) | **0.102 µs ★**    | 5.69 µs (56×)    | 270 µs (2 600×)       |

Against the only other incremental Python peer Wickra is **9–56× faster**;
against the recompute-on-every-tick libraries it is **1 800–15 400× faster**
(`finta` RSI hits 15 400×). tulipy / pandas-ta land in the same recompute
band as TA-Lib.

**Rust — per-tick latency** (whole 50 000-bar series, µs, lower = faster):

| Indicator        | **★&nbsp;Wickra** | kand    | ta-rs  | yata   |
|------------------|------------------:|--------:|-------:|-------:|
| SMA(20)          | 48                | 37      | 46     | **37** |
| EMA(20)          | 72                | 69      | **54** | 72     |
| RSI(14)          | 171               | 202     | **76** | —      |
| MACD(12, 26, 9)  | 228               | 178     | **64** | —      |
| Bollinger(20, 2) | 175               | 290     | **163** | —     |
| ATR(14)          | 86                | 164     | **68** | —      |

`ta-rs` hands back a bare `f64` from the first tick with no warmup and no
validation; it leads the table by giving those guarantees up. Against `kand`,
Wickra wins streaming RSI, Bollinger and ATR and is within 5 % on EMA. `yata`
exposes only SMA/EMA as raw-value methods, so its other rows are omitted rather
than faked.

## 2. Batch — the exact batch, and the opt-in fast one

Whole series in one call. `batch` is bit for bit what `update` gives — the hot
indicators run fused paths that perform the same arithmetic in the same order —
and `batch_fast` runs SIMD kernels within a few units in the last place of it,
with identical `NaN` placement and the same result on every platform.

**Python** (20 000-bar pass, µs/op, lower = faster):

| Indicator        | Wickra | **Wickra fast** | TA-Lib | tulipy | pandas-ta | finta  |
|------------------|-------:|----------------:|-------:|-------:|----------:|-------:|
| SMA(20)          | 20.5   | **9.0 ★**       | 15.5   | 16.1   | 32.7      | 286.5  |
| EMA(20)          | 32.0   | **9.7 ★**       | 30.3   | 31.4   | 50.5      | 224.7  |
| RSI(14)          | 38.3   | **22.9 ★**      | 74.7   | 34.8   | 106.9     | 962.9  |
| MACD(12, 26, 9)  | 40.1   | **27.9 ★**      | 102.1  | 33.4   | 234.4     | 574.4  |
| Bollinger(20, 2) | 75.0   | **33.1 ★**      | 71.5   | 34.2   | 351.1     | 871.1  |
| ATR(14)          | 41.1   | **28.1 ★**      | 84.4   | 34.4   | —         | 3182.6 |

The fast batch leads every row, tulipy's SIMD C included; the exact batch beats
TA-Lib on RSI, MACD and ATR and every row of pandas-ta and finta. A
contiguous `float64` NumPy array or `array.array('d')` of 8 192 values or more is
read in place, without a copy.

**Rust** (50 000-bar pass, µs, lower = faster, into a caller buffer on both
sides). Only Wickra and `kand` expose a batch API; `ta-rs` and `yata` are
streaming-only:

| Indicator        | Wickra     | **Wickra fast** | kand    |
|------------------|-----------:|----------------:|--------:|
| SMA(20)          | 48         | **23 ★**        | 41      |
| EMA(20)          | 82         | **18 ★**        | 67      |
| RSI(14)          | 85         | **47 ★**        | 222     |
| MACD(12, 26, 9)  | 83         | **58 ★**        | 249     |
| Bollinger(20, 2) | 195        | **78 ★**        | 408     |
| ATR(14)          | 73         | **42 ★**        | 165     |

The fast batch wins every row; the exact batch wins RSI, MACD, Bollinger and
ATR by 2.1–3.0× and trails `kand` by a few µs on the two pure recurrences, SMA
and EMA, where keeping streaming's bits fixes the order of the additions.

**.NET** (500 000 bars, period 220, QuanTAlib's own setup: its
geometric-Brownian-motion feed, seed 42, BenchmarkDotNet `ShortRun` on .NET 10,
mean µs, lower = faster). Every library writes into a buffer the caller keeps:
Wickra's `Batch` and `BatchFast` into a `Span`, QuanTAlib's span `Batch`,
TA-Lib's output array:

| Indicator   | Wickra | Wickra fast | QuanTAlib | TA-Lib | Wickra streaming | QuanTAlib streaming |
|-------------|-------:|------------:|----------:|-------:|-----------------:|--------------------:|
| SMA         | 512    | **197 ★**   | 298       | 361    | **1 801**        | 1 904               |
| EMA         | 763    | **186 ★**   | 430       | 730    | 1 654            | **1 581**           |
| WMA         | 653    | 358         | **312 ★** | 384    | **2 020**        | 2 962               |
| HMA ¹       | 1 472  | 1 079       | **1 021 ★** | —    | **5 565**        | 20 436              |
| ADOSC       | 1 048  | 687         | **650 ★** | 745    | **3 501**        | 14 122              |
| Correlation | 3 464  | **1 322 ★** | 1 464 ²   | 2 154  | **4 323**        | 18 328              |
| Skewness ³  | 3 600  | 603         | **584 ★** | —      | **4 282**        | 5 523               |

¹ The two HMAs are different series: Wickra rounds √220 to 15, QuanTAlib — like
TradingView and pandas-ta — truncates it to 14. ² QuanTAlib's span correlation
returns `NaN` for most bars of this series (its streaming one is correct), so
its time is not a comparable result. ³ Wickra's skewness is the population
skewness; QuanTAlib's default is the sample skewness (its population form, 600).

The fast batch leads SMA, EMA and correlation; QuanTAlib's span batch leads WMA,
HMA, ADOSC and skewness by 3–15 % (WMA's gap reaches 20 % in other runs), and
TA-Lib trails both on ADOSC. Streaming — one call per value from C# into the
native core — is level on SMA and EMA and 1.3–4.2× faster than QuanTAlib's on
the other five. The harness's `verify` mode checks the libraries agree before
their times are compared, and CI runs it:

```bash
cargo build -p wickra-c --release
dotnet run -c Release --project bindings/csharp/cross-library -- verify   # numerical cross-check
dotnet run -c Release --project bindings/csharp/cross-library             # every benchmark
```

Run the Rust and Python suites yourself:

```bash
cargo bench -p wickra-bench            # Rust core vs kand / ta-rs / yata
pip install -e bindings/python[bench]  # Python peers
python -m benchmarks.compare_libraries
```

## 3. Per-binding throughput — the cost of the boundary

The sections above compare Wickra against other libraries, which exists for
Python, Rust and .NET (there is no comparable streaming TA library for C, C++,
Go, Java, R or WASM to benchmark against). Every binding calls the **same** Rust
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
  returns a `Float64Array` instead and runs over 100× faster.
- **A reused buffer is worth as much as the kernel.** Writing a fresh multi-megabyte
  result costs page faults on the order of the computation, so the caller-buffer
  forms — C#'s `Span` overloads, Go's `BatchFastInto`, Java's `batchInto` over
  native `MemorySegment`s, Node's `batchFastInto`, the C ABI itself — reach the
  Rust ceiling. Java's array batches hand their arrays to the C ABI in place, and
  Python reads a long NumPy array or `array.array` in place and, from 3.11,
  writes its result in place.
- **Streaming reveals the boundary.** A per-tick `update` crosses the boundary
  once per value, so streaming throughput is where the bindings differ: the raw C
  ABI is nearly free; C# passes its handle without reference counting it and
  Java calls without a thread-state transition; cgo, napi, PyO3 and the R and
  WASM boundaries cost more per tick.

The Rust core ships the same benchmark with **no** FFI boundary
(`examples/rust/.../throughput.rs`) — it is the ceiling each binding is measured
against, its batches writing into a reused buffer as the C ABI's callers do.

`SMA(20)`, 200 000 bars, the better of two runs (each the median of 3), on the
reference machine (Windows 11, AMD Ryzen 9 9950X), all targets measured in one
session:

| Target               | streaming | batch  | fast batch | fast into a reused buffer |
|----------------------|----------:|-------:|-----------:|--------------------------:|
| Rust core (no FFI)   |     1 362 | 1 144¹ |    3 068¹  |                     3 068 |
| C / C++              |       397 | 1 131¹ |    3 140¹  |                     3 140 |
| C#                   |       345 |    739 |      1 263 |       3 072 (`Span<double>`) |
| Go                   |      24.5 |    998 |      2 290 |          2 960 (`BatchFastInto`) |
| Java                 |       255 |    950 |      1 120 |     2 778 (`MemorySegment`) |
| R                    |       0.4 |    623 |      1 031 |                         — |
| WASM                 |      34.8 |    380 |        402 |                         — |
| Python               |      27.5 |    530 |        752 |                         — |
| Node.js              |       5.4 |  11 ²  |      1 256 |     3 160 (`batchFastInto`) |

¹ Into a reused buffer — the Rust benchmark and the C ABI have no allocating form.
² A plain JS `Array` in and out; from a `Float64Array` read in place, 19.

`ATR(14)` and `MACD(12,26,9)` follow the same shape at lower rates, their kernels
being recurrences rather than window sums (Rust core, fast batch into a reused
buffer: ATR 1 424, MACD 936; batch 704 and 632). On the same machine the
previous release, 1.0.6, measures: C 399 streaming and 381 batch, C# 64 and 304,
Java 61 and 166, Python 29 and 46, R 0.1 and 287, WASM 35 and 197, Node 5.4
and 11.

These are throughput numbers, not competitive numbers — the "Wickra is fast"
claim lives in sections 1 and 2 (Rust core + the Python, Rust and .NET
cross-library runs).

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
| CSV parse (`CandleReader`)         |   2.6 M candles/s   |      380 ns |
| Tick aggregate → 1m (`TickAggregator`) |  39 M ticks/s   |     25.9 ns |
| Resample 1m → 5m (`Resampler`)     | 115 M candles/s     |      8.7 ns |

Reading and validating a 50 000-row CSV into typed candles takes ~19 ms;
aggregating 50 000 ticks into one-minute bars ~1.3 ms; resampling 50 000
one-minute candles to five-minute bars ~0.4 ms. CSV parsing is the floor because
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
