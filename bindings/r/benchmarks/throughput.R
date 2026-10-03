#!/usr/bin/env Rscript
#
# Throughput benchmark for the Wickra R bindings.
#
# Measures how many indicator updates per second the R binding sustains, per
# tick (streaming `update`), bulk (`batch`) and bulk through the opt-in SIMD
# kernels (`batch_fast`), over a synthetic OHLCV series. It is the R
# counterpart of the Node `throughput.js` and the Rust criterion benches: it
# benchmarks Wickra's own O(1) streaming engine across the R<->C-ABI boundary
# (there is no comparable streaming TA library on CRAN to compare against), so
# the headline number is raw per-binding throughput / FFI overhead, not a
# cross-library ratio.
#
# Three indicators are timed, chosen by FFI call-signature archetype rather
# than algorithm: SMA (1-in -> 1-out), ATR (multi-in -> 1-out), and MACD
# (1-in -> multi-out). All three are timed streaming, batch and fast batch.
#
# Install the package first (it links the C ABI; see bindings/r/README.md),
# then run:
#
#   Rscript bindings/r/benchmarks/throughput.R               # 200k bars (default)
#   Rscript bindings/r/benchmarks/throughput.R --bars 1000000

suppressMessages(library(wickra))

parse_bars <- function() {
  args <- commandArgs(trailingOnly = TRUE)
  i <- match("--bars", args)
  if (!is.na(i) && length(args) >= i + 1L) {
    n <- suppressWarnings(as.integer(args[i + 1L]))
    if (!is.na(n) && n >= 1000L) {
      return(n)
    }
    stop("--bars must be an integer >= 1000")
  }
  200000L
}

bars <- parse_bars()

# Deterministic synthetic OHLCV (no RNG, so runs are comparable).
idx <- seq.int(0L, bars - 1L)
mid <- 100 + sin(idx * 0.001) * 20 + idx * 1e-4
close <- mid + sin(idx * 0.05) * 2
high <- pmax(close, mid) + 1.5
low <- pmin(close, mid) - 1.5
open <- mid
volume <- 1000 + (idx %% 97L) * 13
# `numeric` (double), not integer: the candle batch path coerces the timestamp
# column with REAL(), which rejects an integer vector.
timestamp <- as.numeric(idx)

# Median elapsed-ns over a few repetitions, after one warmup pass.
time_ns <- function(fn, reps = 3L) {
  fn() # warmup
  samples <- numeric(reps)
  for (r in seq_len(reps)) {
    t0 <- Sys.time()
    fn()
    samples[r] <- as.numeric(Sys.time() - t0, units = "secs") * 1e9
  }
  median(samples)
}

mups_from_ns <- function(ns) bars / (ns / 1e9) / 1e6

# SMA (scalar 1-in/1-out), ATR (multi-in/1-out), MACD (1-in/multi-out).
indicators <- list(
  list(
    name = "SMA(20)",
    stream = function() {
      ind <- Sma(20)
      for (i in seq_len(bars)) update(ind, close[i])
    },
    batch = function() {
      batch(Sma(20), close)
    },
    fast = function() {
      batch_fast(Sma(20), close)
    }
  ),
  list(
    name = "ATR(14)",
    stream = function() {
      ind <- Atr(14)
      for (i in seq_len(bars)) {
        update(ind, open[i], high[i], low[i], close[i], volume[i], timestamp[i])
      }
    },
    batch = function() {
      batch(Atr(14), open, high, low, close, volume, timestamp)
    },
    fast = function() {
      batch_fast(Atr(14), open, high, low, close, volume, timestamp)
    }
  ),
  list(
    name = "MACD(12,26,9)",
    stream = function() {
      ind <- MacdIndicator(12, 26, 9)
      for (i in seq_len(bars)) update(ind, close[i])
    },
    batch = function() {
      batch(MacdIndicator(12, 26, 9), close)
    },
    fast = function() {
      batch_fast(MacdIndicator(12, 26, 9), close)
    }
  )
)

cat(sprintf(
  "Wickra R throughput - %s bars (median of 3 runs)\n\n",
  format(bars, big.mark = ",")
))
cat(sprintf(
  "%-22s%20s%18s%18s\n", "Indicator", "streaming (Mupd/s)", "batch (Mupd/s)", "fast (Mupd/s)"
))
cat(strrep("-", 78), "\n", sep = "")

for (ind in indicators) {
  cells <- vapply(
    list(ind$stream, ind$batch, ind$fast),
    function(run) sprintf("%.1f", mups_from_ns(time_ns(run))),
    character(1L)
  )
  cat(sprintf("%-22s%20s%18s%18s\n", ind$name, cells[1L], cells[2L], cells[3L]))
}

cat(paste0(
  "\nMupd/s = million indicator updates per second. Streaming is the per-tick\n",
  "`update` path crossing the R<->C-ABI boundary once per value; batch is the\n",
  "bulk vector path (one boundary crossing) and fast the opt-in batch_fast\n",
  "(SIMD kernels within a few units in the last place of batch). Higher is\n",
  "better. Numbers are machine-dependent - use them for relative comparison,\n",
  "not as a speed claim.\n"
))
