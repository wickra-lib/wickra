package org.wickra.benchmarks;

import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static java.lang.foreign.ValueLayout.JAVA_LONG;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.util.Arrays;
import java.util.Locale;
import org.wickra.Atr;
import org.wickra.MacdIndicator;
import org.wickra.Sma;

/**
 * Throughput benchmark for the Wickra Java binding.
 *
 * <p>Measures how many indicator updates per second the binding sustains, per
 * tick (streaming {@code update}), bulk ({@code batch}) and bulk through the
 * opt-in SIMD kernels ({@code batchFast}), each allocating its result, into a
 * reused array, and zero-copy over native memory segments, over a synthetic
 * OHLCV series. It is the Java counterpart of the Node {@code throughput.js}
 * and the Rust criterion benches: it benchmarks Wickra's own O(1) streaming
 * engine across the Java FFM &lt;-&gt; C-ABI boundary (there is no comparable
 * streaming TA library on Maven Central to compare against), so the headline
 * number is raw per-binding throughput / FFI overhead, not a cross-library
 * ratio.
 *
 * <p>Three indicators are timed, chosen by FFI call-signature archetype rather
 * than algorithm: SMA (1-in -&gt; 1-out), ATR (multi-in -&gt; 1-out), and MACD
 * (1-in -&gt; multi-out, whose into forms write three doubles per input).
 *
 * <p>Install the binding and build the C ABI library first, then run from the
 * repo root:
 *
 * <pre>
 *   cargo build -p wickra-c --release
 *   mvn -q -f bindings/java install -DskipTests
 *   mvn -q -f bindings/java/benchmarks exec:exec -Dexec.mainClass=org.wickra.benchmarks.Throughput
 * </pre>
 */
public final class Throughput {
    private Throughput() {}

    public static void main(String[] args) {
        int bars = 200_000;
        for (int i = 0; i < args.length - 1; i++) {
            if (args[i].equals("--bars")) {
                try {
                    int n = Integer.parseInt(args[i + 1]);
                    if (n >= 1000) {
                        bars = n;
                    }
                } catch (NumberFormatException ignored) {
                    // keep default
                }
            }
        }

        // Deterministic synthetic OHLCV (no RNG, so runs are comparable).
        double[] open = new double[bars];
        double[] high = new double[bars];
        double[] low = new double[bars];
        double[] close = new double[bars];
        double[] volume = new double[bars];
        long[] timestamp = new long[bars];
        for (int i = 0; i < bars; i++) {
            double mid = 100 + Math.sin(i * 0.001) * 20 + i * 1e-4;
            double c = mid + Math.sin(i * 0.05) * 2;
            close[i] = c;
            open[i] = mid;
            high[i] = Math.max(c, mid) + 1.5;
            low[i] = Math.min(c, mid) - 1.5;
            volume[i] = 1000 + (i % 97) * 13;
            timestamp[i] = i;
        }
        final int n = bars;
        double[] out = new double[bars];

        try (Arena arena = Arena.ofConfined()) {
            // The same series in native memory, for the zero-copy segment forms.
            MemorySegment openSeg = arena.allocateFrom(JAVA_DOUBLE, open);
            MemorySegment highSeg = arena.allocateFrom(JAVA_DOUBLE, high);
            MemorySegment lowSeg = arena.allocateFrom(JAVA_DOUBLE, low);
            MemorySegment closeSeg = arena.allocateFrom(JAVA_DOUBLE, close);
            MemorySegment volumeSeg = arena.allocateFrom(JAVA_DOUBLE, volume);
            MemorySegment timestampSeg = arena.allocateFrom(JAVA_LONG, timestamp);
            MemorySegment outSeg = arena.allocate(JAVA_DOUBLE, n);
            double[] macdOut = new double[n * 3];
            MemorySegment macdOutSeg = arena.allocate(JAVA_DOUBLE, n * 3L);

            // SMA (scalar 1-in/1-out), ATR (multi-in/1-out), MACD (1-in/multi-out).
            Indicator[] indicators = {
                new Indicator("SMA(20)", new Runnable[] {
                    () -> {
                        try (Sma ind = new Sma(20)) {
                            for (int i = 0; i < n; i++) {
                                ind.update(close[i]);
                            }
                        }
                    },
                    () -> { try (Sma ind = new Sma(20)) { ind.batch(close); } },
                    () -> { try (Sma ind = new Sma(20)) { ind.batchFast(close); } },
                    () -> { try (Sma ind = new Sma(20)) { ind.batchInto(close, out); } },
                    () -> { try (Sma ind = new Sma(20)) { ind.batchInto(closeSeg, outSeg); } },
                    () -> { try (Sma ind = new Sma(20)) { ind.batchFastInto(closeSeg, outSeg); } },
                }),
                new Indicator("ATR(14)", new Runnable[] {
                    () -> {
                        try (Atr ind = new Atr(14)) {
                            for (int i = 0; i < n; i++) {
                                ind.update(open[i], high[i], low[i], close[i], volume[i], timestamp[i]);
                            }
                        }
                    },
                    () -> { try (Atr ind = new Atr(14)) { ind.batch(open, high, low, close, volume, timestamp); } },
                    () -> { try (Atr ind = new Atr(14)) { ind.batchFast(open, high, low, close, volume, timestamp); } },
                    () -> { try (Atr ind = new Atr(14)) { ind.batchInto(open, high, low, close, volume, timestamp, out); } },
                    () -> {
                        try (Atr ind = new Atr(14)) {
                            ind.batchInto(openSeg, highSeg, lowSeg, closeSeg, volumeSeg, timestampSeg, outSeg);
                        }
                    },
                    () -> {
                        try (Atr ind = new Atr(14)) {
                            ind.batchFastInto(openSeg, highSeg, lowSeg, closeSeg, volumeSeg, timestampSeg, outSeg);
                        }
                    },
                }),
                new Indicator("MACD(12,26,9)", new Runnable[] {
                    () -> {
                        try (MacdIndicator ind = new MacdIndicator(12, 26, 9)) {
                            for (int i = 0; i < n; i++) {
                                ind.update(close[i]);
                            }
                        }
                    },
                    () -> { try (MacdIndicator ind = new MacdIndicator(12, 26, 9)) { ind.batch(close); } },
                    () -> { try (MacdIndicator ind = new MacdIndicator(12, 26, 9)) { ind.batchFast(close); } },
                    () -> { try (MacdIndicator ind = new MacdIndicator(12, 26, 9)) { ind.batchInto(close, macdOut); } },
                    () -> { try (MacdIndicator ind = new MacdIndicator(12, 26, 9)) { ind.batchInto(closeSeg, macdOutSeg); } },
                    () -> { try (MacdIndicator ind = new MacdIndicator(12, 26, 9)) { ind.batchFastInto(closeSeg, macdOutSeg); } },
                }),
            };

            System.out.printf(Locale.ROOT, "Wickra Java throughput - %,d bars (median of 3 runs)%n%n", bars);
            String header = String.format(Locale.ROOT, "%-16s%11s%11s%11s%11s%11s%11s",
                "Indicator", "streaming", "batch", "fast", "into", "into seg", "fast seg");
            System.out.println(header);
            System.out.println("-".repeat(header.length()));
            for (Indicator ind : indicators) {
                StringBuilder row = new StringBuilder(String.format(Locale.ROOT, "%-16s", ind.name));
                for (Runnable run : ind.runs) {
                    String cell = run == null ? "-" : String.format(Locale.ROOT, "%.1f", mups(bars, timeNs(run)));
                    row.append(String.format(Locale.ROOT, "%11s", cell));
                }
                System.out.println(row);
            }
        }

        System.out.println(
            "\nMupd/s (million indicator updates per second). Streaming is the per-tick\n"
            + "update path crossing the Java FFM<->C-ABI boundary once per value; batch is\n"
            + "the bulk array path and fast the opt-in batchFast (SIMD kernels within a\n"
            + "few units in the last place of batch), both allocating their result; into\n"
            + "writes a reused array, and the seg columns hand native memory segments to\n"
            + "the C ABI without a copy. Higher is better. Numbers are machine-dependent -\n"
            + "use them for relative comparison, not as a speed claim.");
    }

    private static double mups(int bars, double ns) {
        return bars / (ns / 1e9) / 1e6;
    }

    // Median elapsed-ns over a few repetitions, after warmup passes enough for
    // the JIT to compile the loop being timed rather than interpret it.
    private static double timeNs(Runnable fn) {
        for (int w = 0; w < 5; w++) {
            fn.run(); // warmup (JIT + cache)
        }
        final int reps = 3;
        double[] samples = new double[reps];
        for (int r = 0; r < reps; r++) {
            long t0 = System.nanoTime();
            fn.run();
            samples[r] = System.nanoTime() - t0;
        }
        Arrays.sort(samples);
        return samples[reps / 2];
    }

    private record Indicator(String name, Runnable[] runs) {}
}
