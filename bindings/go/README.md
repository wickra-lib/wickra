<p align="center">
  <a href="https://wickra.org"><img src="https://raw.githubusercontent.com/wickra-lib/.github/main/profile/wickra-banner.webp?v=514-7" alt="Wickra — streaming-first technical indicators" width="100%"></a>
</p>

[![CI](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/ci.svg)](https://github.com/wickra-lib/wickra/actions/workflows/ci.yml)
[![codecov](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/codecov.svg)](https://codecov.io/gh/wickra-lib/wickra)
[![Go module](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/go.svg)](https://pkg.go.dev/github.com/wickra-lib/wickra/bindings/go)
[![License: MIT OR Apache-2.0](https://raw.githubusercontent.com/wickra-lib/.github/main/profile/badges/wickra/license.svg)](https://github.com/wickra-lib/wickra#license)

# Wickra — Go

---

> **▶ Live demo:** all 514 indicators over real Binance market data, computed live in your browser — **[live.wickra.org](https://live.wickra.org)** · zero backend, powered by `wickra-wasm`.

**Streaming-first technical indicators for Go, over the Wickra C ABI hub via cgo.**

Wickra is a multi-language technical-analysis library with a Rust core and
bindings for Python, Node.js and WASM, plus a C ABI for C, C++, C#, Go, Java, R and
any other C-capable language. Every indicator is an incremental streaming state machine,
so live trading bots and historical backtests share the exact same
implementation. This package is the Go binding; it consumes the C ABI hub through
cgo and exposes all 514 streaming-first indicators as idiomatic types.

## Install

Use the published **`wickra-go`** module, which bundles the prebuilt C ABI
library for every platform, so `go get` + `go build` works with no extra steps
(a C compiler is still required, as the binding uses cgo):

```bash
go get github.com/wickra-lib/wickra-go
```

```go
import wickra "github.com/wickra-lib/wickra-go"
```

`wickra-go` is generated from this directory by the release pipeline: it mirrors
the Go sources, the vendored C ABI header (`include/wickra.h`) and the prebuilt
libraries under `lib/<goos>_<goarch>/`. On Linux/macOS the library path is baked
in via rpath; on Windows the DLL must be discoverable at run time (next to the
executable or on `PATH`).

### Building from this repository (contributors)

This `bindings/go` directory is the development source. To build it directly,
compile the C ABI and stage the library into the per-platform directory cgo
links against:

```bash
cargo build -p wickra-c --release
mkdir -p bindings/go/lib/linux_amd64                 # match your GOOS_GOARCH
cp target/release/libwickra.so    bindings/go/lib/linux_amd64/    # Linux
cp target/release/libwickra.dylib bindings/go/lib/darwin_arm64/   # macOS (arm64)
cp target/release/wickra.dll      bindings/go/lib/windows_amd64/  # Windows
```

## Quick start

```go
package main

import (
	"fmt"

	wickra "github.com/wickra-lib/wickra/bindings/go"
)

func main() {
	// Batch: run an indicator over a whole series (NaN at warmup positions).
	prices := make([]float64, 1000)
	for i := range prices {
		prices[i] = 100.0 + float64(i)*0.1
	}
	sma, _ := wickra.NewSma(20)
	defer sma.Close()
	values := sma.Batch(prices)

	// Streaming: the same indicator, fed tick by tick.
	rsi, _ := wickra.NewRsi(14)
	defer rsi.Close()
	for _, price := range prices {
		value := rsi.Update(price) // NaN during warmup, no recomputation
		if value > 70 {
			fmt.Println("overbought")
		}
	}
	_ = values
}
```

`Batch(prices)` and feeding the same prices through `Update()` produce identical
values — the equivalence is enforced by the test suite. Multi-output indicators
(MACD, Bollinger, ADX, …) return `(Output, bool)`, with `false` while warming up.
Every indicator owns a native handle freed by `Close()`; a finalizer is wired as
a backstop, but call `Close()` (e.g. with `defer`) to release memory promptly.

## Benchmark

`benchmarks/throughput.go` reports streaming and batch updates-per-second for
`SMA`, `ATR` and `MACD`. It measures this binding's FFI overhead, not a
cross-library ratio (the same Rust core runs under every binding) — see the
repository [BENCHMARKS.md](https://github.com/wickra-lib/wickra/blob/main/BENCHMARKS.md) §3.

```bash
cd benchmarks && go run .
```

## Documentation

The full indicator catalogue, guides, quickstarts, and API reference live in the
main repository and documentation site:

- **Repository & full indicator list:** <https://github.com/wickra-lib/wickra>
- **Docs** (quickstarts, cookbook, TA-Lib migration): <https://docs.wickra.org>
- **Runnable examples:** [`examples/go/`](https://github.com/wickra-lib/wickra/tree/main/examples/go)

Wickra ships native bindings for Python, Node.js, WASM and Rust, plus a
C ABI hub that any C-capable language (C, C++, C#, Go, Java, R) links against —
all exposing the same indicators from the shared, `unsafe`-forbidden Rust core.

## Security

Found a security issue? **Please don't open a public issue.** Report it privately
via the affected repository's *Security* tab (*"Report a vulnerability"*) or email
**support@wickra.org** with a subject line starting `[wickra security]`. Full
policy: <https://github.com/wickra-lib/wickra/blob/main/SECURITY.md>.

## Disclaimer

Wickra is an indicator toolkit, not a trading system. The values it computes are
deterministic transforms of the input data — they are not financial advice and
do not predict the market. Any use in a live trading context is at your own risk.
The library is provided **as is**, without warranty of any kind.

## License

Licensed under either of [Apache-2.0](https://github.com/wickra-lib/wickra/blob/main/LICENSE-APACHE)
or [MIT](https://github.com/wickra-lib/wickra/blob/main/LICENSE-MIT) at your option.
