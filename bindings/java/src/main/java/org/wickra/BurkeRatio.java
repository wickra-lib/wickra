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

/** Streaming BurkeRatio indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class BurkeRatio implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_BURKE_RATIO_UPDATE;

    public BurkeRatio(int period) {
        if (period < 0) {
            throw new IllegalArgumentException("period must be non-negative");
        }
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_BURKE_RATIO_NEW.invokeExact((long) period);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid BurkeRatio parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_BURKE_RATIO_FREE);
    }

    /** Push one observation; returns the indicator value (NaN during warmup). */
    public double update(double value) {
        try {
            return (double) UPDATE.invokeExact(handle(), value);
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
    public double[] batch(double[] input) {
        double[] output = new double[input.length];
        batchInto(input, output);
        return output;
    }

    /**
     * Vectorized update over a whole series; NaN at warmup positions, bit for
     * bit what feeding the values one by one through {@code update} gives.
     * 
     * <p>Writes into {@code output}, which must be as long as the input.
     */
    public void batchInto(double[] input, double[] output) {
        int n = input.length;
        if (output.length != n) {
            throw new IllegalArgumentException("the output array must be as long as the input");
        }
        try {
            WickraNative.heapDowncall("wickra_burke_ratio_batch", NativeMethods.WICKRA_BURKE_RATIO_BATCH)
                    .invokeExact(handle(), MemorySegment.ofArray(input), MemorySegment.ofArray(output), (long) n);
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
    public void batchInto(MemorySegment input, MemorySegment output) {
        long n = input.byteSize() / JAVA_DOUBLE.byteSize();
        WickraNative.checkBatchSegment(input, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(output, JAVA_DOUBLE, n);
        try {
            NativeMethods.WICKRA_BURKE_RATIO_BATCH.invokeExact(handle(), input, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each
     * value agrees with {@code batch} to within a few units in the last place
     * rather than bit for bit; NaN placement and length are identical, and the
     * result is the same on every platform. Without a kernel it is exactly
     * {@code batch}.
     */
    public double[] batchFast(double[] input) {
        double[] output = new double[input.length];
        batchFastInto(input, output);
        return output;
    }

    /**
     * Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each
     * value agrees with {@code batch} to within a few units in the last place
     * rather than bit for bit; NaN placement and length are identical, and the
     * result is the same on every platform. Without a kernel it is exactly
     * {@code batch}.
     * 
     * <p>Writes into {@code output}, which must be as long as the input.
     */
    public void batchFastInto(double[] input, double[] output) {
        int n = input.length;
        if (output.length != n) {
            throw new IllegalArgumentException("the output array must be as long as the input");
        }
        try {
            WickraNative.heapDowncall("wickra_burke_ratio_batch_fast", NativeMethods.WICKRA_BURKE_RATIO_BATCH_FAST)
                    .invokeExact(handle(), MemorySegment.ofArray(input), MemorySegment.ofArray(output), (long) n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each
     * value agrees with {@code batch} to within a few units in the last place
     * rather than bit for bit; NaN placement and length are identical, and the
     * result is the same on every platform. Without a kernel it is exactly
     * {@code batch}.
     * 
     * <p>Zero-copy form over caller-owned native memory: every segment must be
     * off-heap, aligned for its element type, and hold the same number of
     * elements, {@code output} as many doubles. Nothing is copied or allocated.
     */
    public void batchFastInto(MemorySegment input, MemorySegment output) {
        long n = input.byteSize() / JAVA_DOUBLE.byteSize();
        WickraNative.checkBatchSegment(input, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(output, JAVA_DOUBLE, n);
        try {
            NativeMethods.WICKRA_BURKE_RATIO_BATCH_FAST.invokeExact(handle(), input, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_BURKE_RATIO_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_BURKE_RATIO_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_BURKE_RATIO_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_BURKE_RATIO_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("BurkeRatio has been closed");
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
