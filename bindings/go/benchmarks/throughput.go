// Throughput benchmark for the Wickra Go bindings.
//
// Measures how many indicator updates per second the cgo binding sustains,
// per-tick (streaming Update), bulk (Batch) and bulk through the opt-in SIMD
// kernels (BatchFast), each allocating its result and into a reused slice
// (BatchInto / BatchFastInto), over a synthetic OHLCV series. It is the Go
// counterpart of the Node throughput.js and the Rust criterion benches: it
// benchmarks Wickra's own O(1) streaming engine across the Go<->C-ABI boundary
// (there is no comparable streaming TA library to compare against), so the
// headline number is raw per-binding throughput / FFI overhead, not a
// cross-library ratio.
//
// Three indicators are timed, chosen by FFI call-signature archetype rather
// than algorithm: SMA (1-in -> 1-out), ATR (multi-in -> 1-out), and MACD
// (1-in -> multi-out, whose batches return records and have no Into form).
//
// Provision the C ABI library first (see bindings/go/README.md), then run:
//
//	cd bindings/go/benchmarks
//	go run .                 # 200k bars (default)
//	go run . -bars 1000000
package main

import (
	"flag"
	"fmt"
	"math"
	"sort"
	"strings"
	"time"

	wickra "github.com/wickra-lib/wickra/bindings/go"
)

func main() {
	bars := flag.Int("bars", 200_000, "number of synthetic bars to feed")
	flag.Parse()
	n := *bars
	if n < 1000 {
		fmt.Println("-bars must be >= 1000")
		return
	}

	// Deterministic synthetic OHLCV (no RNG, so runs are comparable).
	open := make([]float64, n)
	high := make([]float64, n)
	low := make([]float64, n)
	closeP := make([]float64, n)
	volume := make([]float64, n)
	timestamp := make([]int64, n)
	for i := 0; i < n; i++ {
		mid := 100 + math.Sin(float64(i)*0.001)*20 + float64(i)*1e-4
		c := mid + math.Sin(float64(i)*0.05)*2
		closeP[i] = c
		open[i] = mid
		high[i] = math.Max(c, mid) + 1.5
		low[i] = math.Min(c, mid) - 1.5
		volume[i] = 1000 + float64(i%97)*13
		timestamp[i] = int64(i)
	}
	// The reused output of the Into forms.
	out := make([]float64, n)

	mups := func(d time.Duration) float64 {
		return float64(n) / d.Seconds() / 1e6
	}

	// Median elapsed over a few repetitions, after one warmup pass.
	// Each sample repeats `fn` until it spans at least 20 ms: a batch over the
	// series can finish inside one tick of the Windows clock, which would
	// otherwise time it as zero.
	timeFn := func(fn func()) time.Duration {
		fn() // warmup
		runs := 1
		for {
			t0 := time.Now()
			for k := 0; k < runs; k++ {
				fn()
			}
			if time.Since(t0) >= 20*time.Millisecond {
				break
			}
			runs *= 2
		}
		const reps = 3
		samples := make([]time.Duration, reps)
		for r := 0; r < reps; r++ {
			t0 := time.Now()
			for k := 0; k < runs; k++ {
				fn()
			}
			samples[r] = time.Since(t0) / time.Duration(runs)
		}
		sort.Slice(samples, func(a, b int) bool { return samples[a] < samples[b] })
		return samples[reps/2]
	}

	type indicator struct {
		name string
		// streaming, batch, fast, batch into, fast into; nil -> not offered
		runs [5]func()
	}

	sma := func(use func(*wickra.Sma)) func() {
		return func() {
			ind, _ := wickra.NewSma(20)
			use(ind)
			ind.Close()
		}
	}
	atr := func(use func(*wickra.Atr)) func() {
		return func() {
			ind, _ := wickra.NewAtr(14)
			use(ind)
			ind.Close()
		}
	}
	macd := func(use func(*wickra.MacdIndicator)) func() {
		return func() {
			ind, _ := wickra.NewMacdIndicator(12, 26, 9)
			use(ind)
			ind.Close()
		}
	}

	indicators := []indicator{
		{"SMA(20)", [5]func(){
			sma(func(ind *wickra.Sma) {
				for i := 0; i < n; i++ {
					ind.Update(closeP[i])
				}
			}),
			sma(func(ind *wickra.Sma) { ind.Batch(closeP) }),
			sma(func(ind *wickra.Sma) { ind.BatchFast(closeP) }),
			sma(func(ind *wickra.Sma) { ind.BatchInto(out, closeP) }),
			sma(func(ind *wickra.Sma) { ind.BatchFastInto(out, closeP) }),
		}},
		{"ATR(14)", [5]func(){
			atr(func(ind *wickra.Atr) {
				for i := 0; i < n; i++ {
					ind.Update(open[i], high[i], low[i], closeP[i], volume[i], timestamp[i])
				}
			}),
			atr(func(ind *wickra.Atr) { ind.Batch(open, high, low, closeP, volume, timestamp) }),
			atr(func(ind *wickra.Atr) { ind.BatchFast(open, high, low, closeP, volume, timestamp) }),
			atr(func(ind *wickra.Atr) { ind.BatchInto(out, open, high, low, closeP, volume, timestamp) }),
			atr(func(ind *wickra.Atr) { ind.BatchFastInto(out, open, high, low, closeP, volume, timestamp) }),
		}},
		{"MACD(12,26,9)", [5]func(){
			macd(func(ind *wickra.MacdIndicator) {
				for i := 0; i < n; i++ {
					ind.Update(closeP[i])
				}
			}),
			macd(func(ind *wickra.MacdIndicator) { ind.Batch(closeP) }),
			macd(func(ind *wickra.MacdIndicator) { ind.BatchFast(closeP) }),
			nil,
			nil,
		}},
	}

	fmt.Printf("Wickra Go throughput - %d bars (median of 3 samples)\n\n", n)
	header := fmt.Sprintf("%-18s%12s%12s%12s%12s%12s", "Indicator", "streaming", "batch", "fast", "batch into", "fast into")
	fmt.Println(header)
	fmt.Println(strings.Repeat("-", len(header)))
	for _, ind := range indicators {
		cells := make([]string, len(ind.runs))
		for k, run := range ind.runs {
			if run == nil {
				cells[k] = "-"
			} else {
				cells[k] = fmt.Sprintf("%.1f", mups(timeFn(run)))
			}
		}
		fmt.Printf("%-18s%12s%12s%12s%12s%12s\n", ind.name, cells[0], cells[1], cells[2], cells[3], cells[4])
	}

	fmt.Println("\nMupd/s (million indicator updates per second). Streaming is the per-tick\n" +
		"Update path crossing the Go<->C-ABI boundary once per value; batch is the\n" +
		"bulk path (one boundary crossing) and fast the opt-in BatchFast (SIMD\n" +
		"kernels within a few units in the last place of Batch), each allocating its\n" +
		"result or, in the into columns, writing into a reused slice. Higher is\n" +
		"better. Numbers are machine-dependent - use them for relative comparison,\n" +
		"not as a speed claim.")
}
