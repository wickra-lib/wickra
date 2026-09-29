// Generated from bindings/c/include/wickra.h. Do not edit by hand.
package org.wickra;

import org.wickra.internal.NativeMethods;
import org.wickra.internal.WickraNative;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.ref.Cleaner;
import java.lang.ref.Reference;
import static java.lang.foreign.ValueLayout.*;

/** Streaming RollMeasure indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class RollMeasure implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;

    public RollMeasure(int period) {
        if (period < 0) {
            throw new IllegalArgumentException("period must be non-negative");
        }
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_ROLL_MEASURE_NEW.invokeExact((long) period);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid RollMeasure parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_ROLL_MEASURE_FREE);
    }

    /** Push one observation; returns the indicator value (NaN during warmup). */
    public double update(double price, double size, boolean isBuy, long timestamp) {
        try {
            return (double) NativeMethods.WICKRA_ROLL_MEASURE_UPDATE.invokeExact(handle(), price, size, (byte) (isBuy ? 1 : 0), timestamp);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Vectorized update over a whole series; NaN at warmup positions, bit for
     * bit what feeding the values one by one through {@code update} gives.
     */
    public double[] batch(double[] price, double[] size, boolean[] isBuy, long[] timestamp) {
        double[] output = new double[price.length];
        batchInto(price, size, isBuy, timestamp, output);
        return output;
    }

    /**
     * Vectorized update over a whole series; NaN at warmup positions, bit for
     * bit what feeding the values one by one through {@code update} gives.
     * 
     * <p>Writes into {@code output}, which must be as long as the input.
     */
    public void batchInto(double[] price, double[] size, boolean[] isBuy, long[] timestamp, double[] output) {
        int n = price.length;
        if (size.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (isBuy.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (timestamp.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (output.length != n) {
            throw new IllegalArgumentException("the output array must be as long as the input");
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment priceSeg = a.allocateFrom(JAVA_DOUBLE, price);
            MemorySegment sizeSeg = a.allocateFrom(JAVA_DOUBLE, size);
            MemorySegment isBuySeg = WickraNative.boolSegment(a, isBuy);
            MemorySegment timestampSeg = a.allocateFrom(JAVA_LONG, timestamp);
            MemorySegment outSeg = a.allocate(JAVA_DOUBLE, n);
            NativeMethods.WICKRA_ROLL_MEASURE_BATCH.invokeExact(handle(), priceSeg, sizeSeg, isBuySeg, timestampSeg, outSeg, (long) n);
            MemorySegment.copy(outSeg, JAVA_DOUBLE, 0L, output, 0, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Vectorized update over a whole series; NaN at warmup positions, bit for
     * bit what feeding the values one by one through {@code update} gives.
     * 
     * <p>Zero-copy form over caller-owned native memory: every segment must be
     * off-heap, aligned for its element type, and hold the same number of
     * elements, {@code output} as many doubles. Nothing is copied or allocated.
     */
    public void batchInto(MemorySegment price, MemorySegment size, MemorySegment isBuy, MemorySegment timestamp, MemorySegment output) {
        long n = price.byteSize() / JAVA_DOUBLE.byteSize();
        WickraNative.checkBatchSegment(price, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(size, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(isBuy, JAVA_BYTE, n);
        WickraNative.checkBatchSegment(timestamp, JAVA_LONG, n);
        WickraNative.checkBatchSegment(output, JAVA_DOUBLE, n);
        try {
            NativeMethods.WICKRA_ROLL_MEASURE_BATCH.invokeExact(handle(), price, size, isBuy, timestamp, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_ROLL_MEASURE_WARMUP_PERIOD.invokeExact(handle());
            return (int) n;
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Whether the indicator has consumed enough input to emit a value. */
    public boolean isReady() {
        try {
            byte r = (byte) NativeMethods.WICKRA_ROLL_MEASURE_IS_READY.invokeExact(handle());
            return r != 0;
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The indicator's canonical name. */
    public String name() {
        try {
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_ROLL_MEASURE_NAME.invokeExact(handle());
            return s.address() == 0 ? "" : s.reinterpret(Long.MAX_VALUE).getString(0);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Reset to the just-constructed state. */
    public void reset() {
        try {
            NativeMethods.WICKRA_ROLL_MEASURE_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("RollMeasure has been closed");
        }
        return handle;
    }

    @Override public void close() {
        if (closed) {
            return;
        }
        closed = true;
        cleanable.clean();
    }
}
