/*
 * Throughput benchmark for the Wickra C ABI.
 *
 * Measures how many indicator updates per second the C ABI sustains, per tick
 * (streaming `_update`), bulk (`_batch`) and bulk through the opt-in SIMD
 * kernels (`_batch_fast`), over a synthetic OHLCV series. It is the C
 * counterpart of the Node throughput.js and the Rust criterion benches: it
 * benchmarks Wickra's own O(1) streaming engine through the raw C boundary
 * (there is no comparable streaming TA library to compare against), so the
 * headline number is raw throughput, not a cross-library ratio. C is the
 * thinnest binding, so these numbers are the floor of the per-binding FFI
 * overhead the higher-level bindings build on.
 *
 * Three indicators are timed, chosen by call-signature archetype rather than
 * algorithm: SMA (1-in -> 1-out), ATR (multi-in -> 1-out), and MACD
 * (1-in -> multi-out). All three are timed streaming, batch and fast batch,
 * the batches into caller buffers reused across runs.
 *
 * Build the C ABI library first, then build and run the benchmark:
 *
 *   cargo build -p wickra-c --release
 *   cmake -S bindings/c/benchmarks -B build/cbench -DCMAKE_BUILD_TYPE=Release
 *   cmake --build build/cbench
 *   ./build/cbench/throughput            # 200k bars (default)
 *   ./build/cbench/throughput 1000000
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <stdint.h>

#include "wickra.h"

#ifdef _WIN32
#include <windows.h>
static double now_ns(void) {
    static LARGE_INTEGER freq;
    static int init = 0;
    LARGE_INTEGER counter;
    if (!init) {
        QueryPerformanceFrequency(&freq);
        init = 1;
    }
    QueryPerformanceCounter(&counter);
    return (double)counter.QuadPart * 1e9 / (double)freq.QuadPart;
}
#else
#include <time.h>
static double now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec * 1e9 + (double)ts.tv_nsec;
}
#endif

static double median3(double a, double b, double c) {
    if ((a <= b && b <= c) || (c <= b && b <= a)) return b;
    if ((b <= a && a <= c) || (c <= a && a <= b)) return a;
    return c;
}

/* Run `body` once as warmup, then time three repetitions and store the median
 * elapsed nanoseconds in `dst`. `body` is a brace-enclosed statement block. */
#define MEASURE(dst, body)                                  \
    do {                                                    \
        body;                                               \
        double s0, s1, s2, t0;                              \
        t0 = now_ns(); body; s0 = now_ns() - t0;            \
        t0 = now_ns(); body; s1 = now_ns() - t0;            \
        t0 = now_ns(); body; s2 = now_ns() - t0;            \
        (dst) = median3(s0, s1, s2);                        \
    } while (0)

int main(int argc, char **argv) {
    size_t bars = 200000;
    if (argc > 1) {
        long n = strtol(argv[1], NULL, 10);
        if (n >= 1000) {
            bars = (size_t)n;
        }
    }
    const size_t n = bars;

    /* Deterministic synthetic OHLCV (no RNG, so runs are comparable). */
    double *open = malloc(n * sizeof(double));
    double *high = malloc(n * sizeof(double));
    double *low = malloc(n * sizeof(double));
    double *close = malloc(n * sizeof(double));
    double *volume = malloc(n * sizeof(double));
    int64_t *timestamp = malloc(n * sizeof(int64_t));
    double *out = malloc(n * sizeof(double)); /* reused batch scratch buffer */
    struct WickraMacdOutput *rows = malloc(n * sizeof(struct WickraMacdOutput));
    if (!open || !high || !low || !close || !volume || !timestamp || !out || !rows) {
        fprintf(stderr, "allocation failed\n");
        return 1;
    }
    for (size_t i = 0; i < n; i++) {
        double mid = 100 + sin((double)i * 0.001) * 20 + (double)i * 1e-4;
        double c = mid + sin((double)i * 0.05) * 2;
        close[i] = c;
        open[i] = mid;
        high[i] = fmax(c, mid) + 1.5;
        low[i] = fmin(c, mid) - 1.5;
        volume[i] = 1000 + (double)(i % 97) * 13;
        timestamp[i] = (int64_t)i;
    }

    double ns;
    /* [indicator][streaming, batch, fast] in Mupd/s. */
    double mups[3][3];

    MEASURE(ns, {
        struct Sma *ind = wickra_sma_new(20);
        for (size_t i = 0; i < n; i++) wickra_sma_update(ind, close[i]);
        wickra_sma_free(ind);
    });
    mups[0][0] = (double)n / (ns / 1e9) / 1e6;
    MEASURE(ns, {
        struct Sma *ind = wickra_sma_new(20);
        wickra_sma_batch(ind, close, out, n);
        wickra_sma_free(ind);
    });
    mups[0][1] = (double)n / (ns / 1e9) / 1e6;
    MEASURE(ns, {
        struct Sma *ind = wickra_sma_new(20);
        wickra_sma_batch_fast(ind, close, out, n);
        wickra_sma_free(ind);
    });
    mups[0][2] = (double)n / (ns / 1e9) / 1e6;

    MEASURE(ns, {
        struct Atr *ind = wickra_atr_new(14);
        for (size_t i = 0; i < n; i++)
            wickra_atr_update(ind, open[i], high[i], low[i], close[i], volume[i], timestamp[i]);
        wickra_atr_free(ind);
    });
    mups[1][0] = (double)n / (ns / 1e9) / 1e6;
    MEASURE(ns, {
        struct Atr *ind = wickra_atr_new(14);
        wickra_atr_batch(ind, open, high, low, close, volume, timestamp, out, n);
        wickra_atr_free(ind);
    });
    mups[1][1] = (double)n / (ns / 1e9) / 1e6;
    MEASURE(ns, {
        struct Atr *ind = wickra_atr_new(14);
        wickra_atr_batch_fast(ind, open, high, low, close, volume, timestamp, out, n);
        wickra_atr_free(ind);
    });
    mups[1][2] = (double)n / (ns / 1e9) / 1e6;

    MEASURE(ns, {
        struct MacdIndicator *ind = wickra_macd_indicator_new(12, 26, 9);
        struct WickraMacdOutput value;
        for (size_t i = 0; i < n; i++) wickra_macd_indicator_update(ind, close[i], &value);
        wickra_macd_indicator_free(ind);
    });
    mups[2][0] = (double)n / (ns / 1e9) / 1e6;
    MEASURE(ns, {
        struct MacdIndicator *ind = wickra_macd_indicator_new(12, 26, 9);
        wickra_macd_indicator_batch(ind, close, rows, n);
        wickra_macd_indicator_free(ind);
    });
    mups[2][1] = (double)n / (ns / 1e9) / 1e6;
    MEASURE(ns, {
        struct MacdIndicator *ind = wickra_macd_indicator_new(12, 26, 9);
        wickra_macd_indicator_batch_fast(ind, close, rows, n);
        wickra_macd_indicator_free(ind);
    });
    mups[2][2] = (double)n / (ns / 1e9) / 1e6;

    static const char *const names[3] = {"SMA(20)", "ATR(14)", "MACD(12,26,9)"};
    printf("Wickra C throughput - %zu bars (median of 3 runs)\n\n", n);
    printf("%-22s%20s%18s%18s\n", "Indicator", "streaming (Mupd/s)", "batch (Mupd/s)", "fast (Mupd/s)");
    printf("------------------------------------------------------------------------------\n");
    for (int k = 0; k < 3; k++) {
        printf("%-22s%20.1f%18.1f%18.1f\n", names[k], mups[k][0], mups[k][1], mups[k][2]);
    }

    printf("\nMupd/s = million indicator updates per second. Streaming is the per-tick\n"
           "`_update` path (one C call per value); batch is the bulk array path (one\n"
           "C call) and fast the opt-in `_batch_fast` (SIMD kernels within a few units\n"
           "in the last place of batch), both into reused caller buffers. Higher is\n"
           "better. Numbers are machine-dependent - use them for relative comparison,\n"
           "not as a speed claim.\n");

    free(open);
    free(high);
    free(low);
    free(close);
    free(volume);
    free(timestamp);
    free(out);
    free(rows);
    return 0;
}
