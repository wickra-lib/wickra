package org.wickra;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import org.junit.jupiter.api.Test;

import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The caller-buffer {@code batchInto} forms (array and zero-copy segment) and the
 * opt-in {@code batchFast}. {@code batchInto} is bit for bit the allocating
 * {@code batch}; {@code batchFast} keeps NaN placement and length and agrees
 * within a documented tolerance.
 */
class BatchFastTest {

    private static final int N = 1003;

    /** A wandering path long enough for every kernel's vector loop and tail. */
    private static double[] prices(int n) {
        double[] p = new double[n];
        for (int i = 0; i < n; i++) {
            p[i] = 100 + Math.sin(i * 0.0137) * 5 + Math.cos(i * 0.37);
        }
        return p;
    }

    private static void assertWithin(double[] exact, double[] fast, double tol) {
        assertEquals(exact.length, fast.length);
        for (int i = 0; i < exact.length; i++) {
            assertEquals(Double.isNaN(exact[i]), Double.isNaN(fast[i]), "NaN mismatch at " + i);
            if (!Double.isNaN(exact[i])) {
                double bound = tol * Math.max(1, Math.abs(exact[i]));
                assertTrue(Math.abs(exact[i] - fast[i]) <= bound, "at " + i);
            }
        }
    }

    @Test
    void batchIntoIsTheAllocatingBatchBitForBit() {
        double[] input = prices(N);
        try (Ema a = new Ema(20); Ema b = new Ema(20)) {
            double[] want = a.batch(input);
            double[] got = new double[N];
            b.batchInto(input, got);
            assertArrayEquals(want, got);
            assertEquals(a.update(101.0), b.update(101.0));
        }
    }

    @Test
    void segmentBatchIntoIsTheArrayFormBitForBit() {
        double[] input = prices(N);
        try (Sma a = new Sma(20); Sma b = new Sma(20); Arena arena = Arena.ofConfined()) {
            MemorySegment in = arena.allocateFrom(JAVA_DOUBLE, input);
            MemorySegment out = arena.allocate(JAVA_DOUBLE, N);
            b.batchInto(in, out);
            assertArrayEquals(a.batch(input), out.toArray(JAVA_DOUBLE));
        }
    }

    @Test
    void scalarBatchFastAgreesWithBatch() {
        double[] input = prices(N);
        try (Sma exact = new Sma(20); Sma fast = new Sma(20)) {
            assertWithin(exact.batch(input), fast.batchFast(input), 1e-12);
            assertEquals(exact.update(101.0), fast.update(101.0), 1e-12);
        }
        try (Rsi exact = new Rsi(14); Rsi fast = new Rsi(14); Arena arena = Arena.ofConfined()) {
            MemorySegment in = arena.allocateFrom(JAVA_DOUBLE, input);
            MemorySegment out = arena.allocate(JAVA_DOUBLE, N);
            fast.batchFastInto(in, out);
            assertWithin(exact.batch(input), out.toArray(JAVA_DOUBLE), 1e-12);
        }
    }

    @Test
    void batchFastWithoutAKernelIsTheExactBatch() {
        double[] input = prices(N);
        try (Roc exact = new Roc(10); Roc fast = new Roc(10)) {
            assertArrayEquals(exact.batch(input), fast.batchFast(input));
        }
    }

    @Test
    void mismatchedBuffersAreRejected() {
        double[] input = prices(16);
        try (Sma sma = new Sma(3); PearsonCorrelation pearson = new PearsonCorrelation(5);
                Arena arena = Arena.ofConfined()) {
            assertThrows(IllegalArgumentException.class, () -> sma.batchInto(input, new double[15]));
            assertThrows(IllegalArgumentException.class, () -> sma.batchFastInto(input, new double[17]));
            assertThrows(IllegalArgumentException.class, () -> pearson.batchFast(input, new double[15]));
            MemorySegment in = arena.allocateFrom(JAVA_DOUBLE, input);
            // A heap segment, a short output, and a misaligned input.
            assertThrows(IllegalArgumentException.class,
                    () -> sma.batchInto(MemorySegment.ofArray(input), arena.allocate(JAVA_DOUBLE, 16)));
            assertThrows(IllegalArgumentException.class,
                    () -> sma.batchInto(in, arena.allocate(JAVA_DOUBLE, 15)));
            MemorySegment skewed = arena.allocate(8L * 17 + 1, 8).asSlice(1, 8L * 16);
            assertThrows(IllegalArgumentException.class,
                    () -> sma.batchInto(skewed, arena.allocate(JAVA_DOUBLE, 16)));
        }
    }

    @Test
    void emptyInputsGiveEmptyOutputs() {
        try (Ema ema = new Ema(5); MacdIndicator macd = new MacdIndicator(12, 26, 9);
                Arena arena = Arena.ofConfined()) {
            assertEquals(0, ema.batch(new double[0]).length);
            assertEquals(0, ema.batchFast(new double[0]).length);
            ema.batchFastInto(arena.allocate(0), arena.allocate(0));
            assertEquals(0, macd.batchFast(new double[0]).length);
        }
    }

    @Test
    void candleBatchFastAgreesWithBatch() {
        double[] close = prices(N);
        double[] open = close.clone();
        double[] high = new double[N];
        double[] low = new double[N];
        double[] volume = new double[N];
        long[] stamps = new long[N];
        for (int i = 0; i < N; i++) {
            high[i] = close[i] + 1;
            low[i] = close[i] - 1;
            volume[i] = 1000 + i % 7;
            stamps[i] = i;
        }
        try (Atr exact = new Atr(14); Atr fast = new Atr(14)) {
            assertWithin(exact.batch(open, high, low, close, volume, stamps),
                    fast.batchFast(open, high, low, close, volume, stamps), 1e-12);
        }
        try (ChaikinOscillator exact = new ChaikinOscillator(3, 10);
                ChaikinOscillator fast = new ChaikinOscillator(3, 10)) {
            assertWithin(exact.batch(open, high, low, close, volume, stamps),
                    fast.batchFast(open, high, low, close, volume, stamps), 1e-9);
        }
    }

    @Test
    void pairBatchFastAgreesWithBatch() {
        double[] x = prices(N);
        double[] y = new double[N];
        for (int i = 0; i < N; i++) {
            y[i] = x[N - 1 - i] * 0.5 + 3;
        }
        try (PearsonCorrelation exact = new PearsonCorrelation(20);
                PearsonCorrelation fast = new PearsonCorrelation(20)) {
            assertWithin(exact.batch(x, y), fast.batchFast(x, y), 1e-9);
        }
    }

    @Test
    void multiOutputBatchFastAgreesWithBatch() {
        double[] input = prices(N);
        try (MacdIndicator exact = new MacdIndicator(12, 26, 9);
                MacdIndicator fast = new MacdIndicator(12, 26, 9)) {
            MacdOutput[] want = exact.batch(input);
            MacdOutput[] got = fast.batchFast(input);
            assertEquals(want.length, got.length);
            for (int i = 0; i < N; i++) {
                assertWithin(new double[] {want[i].macd(), want[i].signal(), want[i].histogram()},
                        new double[] {got[i].macd(), got[i].signal(), got[i].histogram()}, 1e-12);
            }
        }
        try (BollingerBands exact = new BollingerBands(20, 2.0);
                BollingerBands fast = new BollingerBands(20, 2.0)) {
            BollingerOutput[] want = exact.batch(input);
            BollingerOutput[] got = fast.batchFast(input);
            for (int i = 0; i < N; i++) {
                assertWithin(
                        new double[] {want[i].upper(), want[i].middle(), want[i].lower(), want[i].stddev()},
                        new double[] {got[i].upper(), got[i].middle(), got[i].lower(), got[i].stddev()},
                        1e-10);
            }
        }
    }
}
