<p align="center">
  <a href="https://wickra.org"><img src="https://raw.githubusercontent.com/wickra-lib/.github/main/profile/wickra-banner.webp?v=514-7" alt="Wickra — streaming-first technical indicators" width="100%"></a>
</p>

[![CI](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/ci.svg)](https://github.com/wickra-lib/wickra/actions/workflows/ci.yml)
[![codecov](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/codecov.svg)](https://codecov.io/gh/wickra-lib/wickra)
[![GitHub release](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/release.svg)](https://github.com/wickra-lib/wickra/releases/latest)
[![License: MIT OR Apache-2.0](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/license.svg)](https://github.com/wickra-lib/wickra#license)

# Wickra — C / C++

---

> **▶ Live demo:** all 514 indicators over real Binance market data, computed live in your browser — **[live.wickra.org](https://live.wickra.org)** · zero backend, powered by `wickra-wasm`.

**Streaming-first technical indicators for C and C++. A prebuilt shared/static
library plus a generated `wickra.h` — no system dependencies.**

Wickra is a multi-language technical-analysis library with a Rust core and
bindings for Python, Node.js and WASM, plus a C ABI for C, C++, C#, Go, Java, R and any
other C-capable language. Every indicator is an incremental
streaming state machine, so live trading bots and historical backtests share
the exact same implementation. This package is the **C ABI hub**: it compiles the
core to a C-compatible shared/static library plus a generated header, so any
C-capable language (C, C++, C#, Go, Java, R) links against one artifact instead
of re-wrapping every indicator natively.

## Install

Grab the prebuilt header + library for your platform from the
[GitHub releases](https://github.com/wickra-lib/wickra/releases) — each archive
has `wickra.h`, the optional `wickra.hpp` C++ wrapper, and the shared/static
library — or build from source:

```bash
cargo build -p wickra-c --release
# -> target/release/libwickra.{so,dylib} or wickra.dll (+ import lib) + a staticlib
```

Then compile against the header and link the library
(`cc app.c -I include -L lib -lwickra -lm -o app`).

## Quick start

```c
#include "wickra.h"

struct Rsi *rsi = wickra_rsi_new(14);              /* NULL on invalid params */
for (size_t i = 0; i < n; ++i) {
    double v = wickra_rsi_update(rsi, prices[i]);  /* NaN during warmup       */
    if (v == v && v > 70.0)                        /* v == v is the NaN check */
        printf("overbought\n");
}
wickra_rsi_free(rsi);                              /* exactly once per _new   */
```

Every indicator is an opaque handle with the same five functions —
`_new` / `_update` / `_batch` / `_reset` / `_free`. `update` touches buffered state only, never the history behind the tick; there is no
RAII across the C boundary, so each `_new` needs exactly one `_free`, and every
function is NULL-safe (a NULL handle yields `NaN` or a no-op, never a crash).
Multi-output indicators (MACD, Bollinger, ADX, …) take a pointer to a `#[repr(C)]`
struct and return a `bool`. The optional `wickra.hpp` wraps any handle in a
move-only `wickra::Handle` for exception-safe C++ lifetimes.

## Benchmark

`benchmarks/throughput.c` reports streaming and batch updates-per-second for
`SMA`, `ATR` and `MACD`. As the thinnest binding it is the floor of the
per-binding FFI overhead — not a cross-library ratio (the same Rust core runs
under every binding); see the repository
[BENCHMARKS.md](https://github.com/wickra-lib/wickra/blob/main/BENCHMARKS.md) §3.

```bash
cargo build -p wickra-c --release
cmake -S benchmarks -B build && cmake --build build
./build/throughput
```

## Documentation

The full indicator catalogue, guides, quickstarts, and API reference live in
the main repository and documentation site:

- **Repository & full indicator list:** <https://github.com/wickra-lib/wickra>
- **Docs** (C quickstart, cookbook, TA-Lib migration): <https://docs.wickra.org/Quickstart-C>
- **Runnable examples:** [`examples/c/`](https://github.com/wickra-lib/wickra/tree/main/examples/c)

Wickra ships native bindings for Python, Node.js, WASM and Rust, plus this
C ABI hub that any C-capable language (C, C++, C#, Go, Java, R) links against —
all exposing the same indicators from the shared Rust core.

## Security

Found a security issue? **Please don't open a public issue.** Report it privately
via the affected repository's *Security* tab (*"Report a vulnerability"*) or email
**support@wickra.org** with a subject line starting `[wickra security]`. Full
policy: <https://github.com/wickra-lib/wickra/blob/main/SECURITY.md>.

## Disclaimer

Wickra is an indicator toolkit, not a trading system. The values it computes
are deterministic transforms of the input data — they are not financial advice
and do not predict the market. Any use in a live trading context is at your own
risk. The library is provided **as is**, without warranty of any kind.

## License

Licensed under either of [Apache-2.0](https://github.com/wickra-lib/wickra/blob/main/LICENSE-APACHE)
or [MIT](https://github.com/wickra-lib/wickra/blob/main/LICENSE-MIT) at your option.
