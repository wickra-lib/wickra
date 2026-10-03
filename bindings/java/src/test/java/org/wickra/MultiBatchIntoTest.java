package org.wickra;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import org.junit.jupiter.api.Test;

import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

/**
 * The flat caller-buffer forms of a multi-output batch: one row of the record's
 * components per input, bit for bit the allocating batch, in an array or a
 * native segment.
 */
class MultiBatchIntoTest {

    private static final int N = 777;

    private static double[] prices(int n) {
        double[] p = new double[n];
        for (int i = 0; i < n; i++) {
            p[i] = 100 + Math.sin(i * 0.021) * 4 + Math.cos(i * 0.29);
        }
        return p;
    }

    /** The allocating batch's records laid out flat, row by row. */
    private static double[] flatten(MacdOutput[] rows) {
        double[] flat = new double[rows.length * 3];
        for (int i = 0; i < rows.length; i++) {
            flat[i * 3] = rows[i].macd();
            flat[i * 3 + 1] = rows[i].signal();
            flat[i * 3 + 2] = rows[i].histogram();
        }
        return flat;
    }

    @Test
    void arrayFormIsTheAllocatingBatchBitForBit() {
        double[] input = prices(N);
        try (MacdIndicator a = new MacdIndicator(12, 26, 9); MacdIndicator b = new MacdIndicator(12, 26, 9)) {
            double[] want = flatten(a.batch(input));
            double[] got = new double[N * 3];
            b.batchInto(input, got);
            assertArrayEquals(want, got);
            // Either form leaves the same state behind.
            assertEquals(a.update(101.0), b.update(101.0));
        }
    }

    @Test
    void segmentFormIsTheArrayFormBitForBit() {
        double[] input = prices(N);
        try (MacdIndicator a = new MacdIndicator(12, 26, 9);
             MacdIndicator b = new MacdIndicator(12, 26, 9);
             Arena arena = Arena.ofConfined()) {
            double[] want = new double[N * 3];
            a.batchFastInto(input, want);
            MemorySegment in = arena.allocateFrom(JAVA_DOUBLE, input);
            MemorySegment out = arena.allocate(JAVA_DOUBLE, N * 3L);
            b.batchFastInto(in, out);
            assertArrayEquals(want, out.toArray(JAVA_DOUBLE));
        }
    }

    @Test
    void rejectsAnOutputOfTheWrongWidth() {
        try (MacdIndicator macd = new MacdIndicator(12, 26, 9); Arena arena = Arena.ofConfined()) {
            assertThrows(IllegalArgumentException.class, () -> macd.batchInto(prices(10), new double[10]));
            MemorySegment in = arena.allocateFrom(JAVA_DOUBLE, prices(10));
            assertThrows(IllegalArgumentException.class,
                    () -> macd.batchInto(in, arena.allocate(JAVA_DOUBLE, 10)));
        }
    }
}
