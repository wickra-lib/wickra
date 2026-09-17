# Wickra — C / C++ examples

The Wickra C ABI is a single shared/static library plus a generated header
([`bindings/c/include/wickra.h`](../../bindings/c/include/wickra.h)). Any
C-capable language links against the same artifact; these examples show the
plain-C path.

## Build the library

From the workspace root:

```sh
cargo build -p wickra-c --release
```

This produces, in `target/release/`:

| Platform | Shared library | Link target |
|----------|----------------|-------------|
| Linux    | `libwickra.so`     | `-lwickra` |
| macOS    | `libwickra.dylib`  | `-lwickra` |
| Windows (MSVC) | `wickra.dll` | `wickra.dll.lib` (import lib) |

A static library (`libwickra.a` / `wickra.lib`) is emitted alongside.

## Build and run the examples

### With CMake (portable, used by CI)

```sh
cmake -S examples/c -B examples/c/build -DWICKRA_LIB_DIR="$PWD/target/release"
cmake --build examples/c/build
ctest --test-dir examples/c/build --output-on-failure
```

### Directly with a compiler

```sh
# Linux / macOS
cc examples/c/smoke.c -I bindings/c/include -L target/release -lwickra -lm -o smoke
LD_LIBRARY_PATH=target/release ./smoke        # macOS: DYLD_LIBRARY_PATH

# Windows (MinGW gcc, linking the DLL directly)
gcc examples/c/smoke.c -I bindings/c/include target/release/wickra.dll -lm -o smoke.exe
```

Expected output:

```
OK: wickra C ABI smoke passed (SMA streaming + batch + reset + NULL-safety + free)
```

## The examples

| Example | What it does |
|---------|--------------|
| `smoke.c` | Links the boundary and asserts SMA streaming / batch / reset / NULL-safety values. |
| `streaming.c` | Feeds a synthetic price series through SMA / EMA / RSI / MACD tick by tick. |
| `backtest.c` | Runs an indicator basket over an OHLCV CSV (defaults to the bundled daily dataset). |
| `multi_timeframe.c` | Resamples the bundled 1-minute CSV to 5m / 15m / 1h / 4h / 1d and prints indicators per timeframe. |
| `parallel_assets.c` | Serial vs OpenMP fan-out over a synthetic panel (one handle per asset), with speedup. |
| `strategy_rsi_mean_reversion.c` | Hourly RSI(14) mean-reversion with a PnL / Sharpe / max-drawdown summary. |
| `strategy_macd_adx.c` | Hourly MACD crossover gated by ADX(14) > 20. |
| `strategy_bollinger_squeeze.c` | Daily Bollinger-squeeze breakout with an ATR(14) stop. |
| `fetch_btcusdt.c` | Downloads BTCUSDT klines from the Binance REST API into `examples/data/` (shells out to `curl`). |
| `live_binance.c` | Polls the Binance REST klines endpoint via `curl` and streams closed candles through RSI(14). |
| `smoke.cpp` | C++ RAII via `wickra::Handle` from [`wickra.hpp`](../../bindings/c/include/wickra.hpp). |

`ctest` builds and runs every example except `fetch_btcusdt` and `live_binance`,
which reach the network and are built only — run those two by hand. The C ABI
exposes only the indicators, not the `wickra-data` IO layer, so the examples read
CSV ([`wickra_csv.h`](wickra_csv.h)) and resample themselves; the network ones
shell out to the system `curl` rather than adding an HTTP/TLS dependency.

## Usage shape

Every indicator follows the same five-function pattern over an opaque handle:

```c
#include "wickra.h"

struct Sma *sma = wickra_sma_new(14);     /* NULL on invalid params */
double v = wickra_sma_update(sma, 42.0);  /* NaN during warmup */
wickra_sma_reset(sma);                     /* back to fresh state */
wickra_sma_free(sma);                      /* exactly once per _new */
```

There is no RAII across the C boundary: every `wickra_<ind>_new` must be paired
with exactly one `wickra_<ind>_free`. All functions are NULL-safe (a NULL handle
yields `NaN` / a no-op, never a crash).
