// Generated from bindings/c/include/wickra.h. Do not edit by hand.
package org.wickra;

import org.wickra.internal.NativeMethods;
import org.wickra.internal.WickraNative;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.invoke.MethodHandle;
import java.lang.ref.Cleaner;
import java.lang.ref.Reference;
import static java.lang.foreign.ValueLayout.*;

/** Streaming VarianceRatio indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class VarianceRatio implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_VARIANCE_RATIO_UPDATE;

    public VarianceRatio(int period, int q) {
        if (period < 0) {
            throw new IllegalArgumentException("period must be non-negative");
        }
        if (q < 0) {
            throw new IllegalArgumentException("q must be non-negative");
        }
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_VARIANCE_RATIO_NEW.invokeExact((long) period, (long) q);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid VarianceRatio parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_VARIANCE_RATIO_FREE);
    }

    /** Push one observation; returns the indicator value (NaN during warmup). */
    public double update(double x, double y) {
        try {
            return (double) UPDATE.invokeExact(handle(), x, y);
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
    public double[] batch(double[] x, double[] y) {
        double[] output = new double[x.length];
        batchInto(x, y, output);
        return output;
    }

    /**
     * Vectorized update over a whole series; NaN at warmup positions, bit for
     * bit what feeding the values one by one through {@code update} gives.
     * 
     * <p>Writes into {@code output}, which must be as long as the input.
     */
    public void batchInto(double[] x, double[] y, double[] output) {
        int n = x.length;
        if (y.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (output.length != n) {
            throw new IllegalArgumentException("the output array must be as long as the input");
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment xSeg = a.allocateFrom(JAVA_DOUBLE, x);
            MemorySegment ySeg = a.allocateFrom(JAVA_DOUBLE, y);
            MemorySegment outSeg = a.allocate(JAVA_DOUBLE, n);
            NativeMethods.WICKRA_VARIANCE_RATIO_BATCH.invokeExact(handle(), xSeg, ySeg, outSeg, (long) n);
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
    public void batchInto(MemorySegment x, MemorySegment y, MemorySegment output) {
        long n = x.byteSize() / JAVA_DOUBLE.byteSize();
        WickraNative.checkBatchSegment(x, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(y, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(output, JAVA_DOUBLE, n);
        try {
            NativeMethods.WICKRA_VARIANCE_RATIO_BATCH.invokeExact(handle(), x, y, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_VARIANCE_RATIO_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_VARIANCE_RATIO_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_VARIANCE_RATIO_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_VARIANCE_RATIO_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("VarianceRatio has been closed");
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
