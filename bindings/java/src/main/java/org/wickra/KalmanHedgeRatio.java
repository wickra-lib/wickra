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

/** Streaming KalmanHedgeRatio indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class KalmanHedgeRatio implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_KALMAN_HEDGE_RATIO_UPDATE;
    /** Where update receives its result, allocated once. */
    private final MemorySegment updateOut = Arena.ofAuto().allocate(24L);

    public KalmanHedgeRatio(double delta, double observationVar) {
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_KALMAN_HEDGE_RATIO_NEW.invokeExact(delta, observationVar);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid KalmanHedgeRatio parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_KALMAN_HEDGE_RATIO_FREE);
    }

    /** Push one observation; returns the result, or null during warmup. */
    public KalmanHedgeRatioOutput update(double x, double y) {
        try {
            MemorySegment out = updateOut;
            byte ok = (byte) UPDATE.invokeExact(handle(), x, y, out);
            if (ok == 0) {
                return null;
            }
            return new KalmanHedgeRatioOutput(
                out.get(JAVA_DOUBLE, 0L),
                out.get(JAVA_DOUBLE, 8L),
                out.get(JAVA_DOUBLE, 16L));
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Vectorized update over whole series, one output per input. A row the
     * indicator did not produce -- warmup, or an input it rejected -- carries
     * NaN in every floating-point field.
     */
    public KalmanHedgeRatioOutput[] batch(double[] x, double[] y) {
        int n = x.length;
        if (y.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment xSeg = a.allocateFrom(JAVA_DOUBLE, x);
            MemorySegment ySeg = a.allocateFrom(JAVA_DOUBLE, y);
            MemorySegment outSeg = a.allocate(24L * n);
            NativeMethods.WICKRA_KALMAN_HEDGE_RATIO_BATCH.invokeExact(handle(), xSeg, ySeg, outSeg, (long) n);
            KalmanHedgeRatioOutput[] out = new KalmanHedgeRatioOutput[n];
            for (int i = 0; i < n; i++) {
                out[i] = new KalmanHedgeRatioOutput(
                        outSeg.get(JAVA_DOUBLE, i * 24L + 0L),
                        outSeg.get(JAVA_DOUBLE, i * 24L + 8L),
                        outSeg.get(JAVA_DOUBLE, i * 24L + 16L));
            }
            return out;
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * The batch into a flat buffer of
     * 3 doubles per input, one row per input in the order of {@link KalmanHedgeRatioOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>{@code output} must hold 3 values per input.
     */
    public void batchInto(double[] x, double[] y, double[] output) {
        int n = x.length;
        if (y.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (output.length != (long) n * 3) {
            throw new IllegalArgumentException("the output array must hold 3 values per input");
        }
        try {
            WickraNative.heapDowncall("wickra_kalman_hedge_ratio_batch", NativeMethods.WICKRA_KALMAN_HEDGE_RATIO_BATCH)
                    .invokeExact(handle(), MemorySegment.ofArray(x), MemorySegment.ofArray(y), MemorySegment.ofArray(output), (long) n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * The batch into a flat buffer of
     * 3 doubles per input, one row per input in the order of {@link KalmanHedgeRatioOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>Zero-copy form over caller-owned native memory: every input segment must be
     * off-heap, aligned for its element type and hold the same number of elements,
     * {@code output} 3 doubles per input. Nothing is copied or allocated.
     */
    public void batchInto(MemorySegment x, MemorySegment y, MemorySegment output) {
        long n = x.byteSize() / JAVA_DOUBLE.byteSize();
        WickraNative.checkBatchSegment(x, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(y, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(output, JAVA_DOUBLE, n * 3);
        try {
            NativeMethods.WICKRA_KALMAN_HEDGE_RATIO_BATCH.invokeExact(handle(), x, y, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_KALMAN_HEDGE_RATIO_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_KALMAN_HEDGE_RATIO_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_KALMAN_HEDGE_RATIO_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_KALMAN_HEDGE_RATIO_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("KalmanHedgeRatio has been closed");
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
