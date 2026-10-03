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

/** Streaming SpreadBollingerBands indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class SpreadBollingerBands implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_SPREAD_BOLLINGER_BANDS_UPDATE;
    /** Where update receives its result, allocated once. */
    private final MemorySegment updateOut = Arena.ofAuto().allocate(32L);

    public SpreadBollingerBands(int period, double numStd) {
        if (period < 0) {
            throw new IllegalArgumentException("period must be non-negative");
        }
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_SPREAD_BOLLINGER_BANDS_NEW.invokeExact((long) period, numStd);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid SpreadBollingerBands parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_SPREAD_BOLLINGER_BANDS_FREE);
    }

    /** Push one observation; returns the result, or null during warmup. */
    public SpreadBollingerBandsOutput update(double x, double y) {
        try {
            MemorySegment out = updateOut;
            byte ok = (byte) UPDATE.invokeExact(handle(), x, y, out);
            if (ok == 0) {
                return null;
            }
            return new SpreadBollingerBandsOutput(
                out.get(JAVA_DOUBLE, 0L),
                out.get(JAVA_DOUBLE, 8L),
                out.get(JAVA_DOUBLE, 16L),
                out.get(JAVA_DOUBLE, 24L));
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
    public SpreadBollingerBandsOutput[] batch(double[] x, double[] y) {
        int n = x.length;
        if (y.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment xSeg = a.allocateFrom(JAVA_DOUBLE, x);
            MemorySegment ySeg = a.allocateFrom(JAVA_DOUBLE, y);
            MemorySegment outSeg = a.allocate(32L * n);
            NativeMethods.WICKRA_SPREAD_BOLLINGER_BANDS_BATCH.invokeExact(handle(), xSeg, ySeg, outSeg, (long) n);
            SpreadBollingerBandsOutput[] out = new SpreadBollingerBandsOutput[n];
            for (int i = 0; i < n; i++) {
                out[i] = new SpreadBollingerBandsOutput(
                        outSeg.get(JAVA_DOUBLE, i * 32L + 0L),
                        outSeg.get(JAVA_DOUBLE, i * 32L + 8L),
                        outSeg.get(JAVA_DOUBLE, i * 32L + 16L),
                        outSeg.get(JAVA_DOUBLE, i * 32L + 24L));
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
     * 4 doubles per input, one row per input in the order of {@link SpreadBollingerBandsOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>{@code output} must hold 4 values per input.
     */
    public void batchInto(double[] x, double[] y, double[] output) {
        int n = x.length;
        if (y.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (output.length != (long) n * 4) {
            throw new IllegalArgumentException("the output array must hold 4 values per input");
        }
        try {
            WickraNative.heapDowncall("wickra_spread_bollinger_bands_batch", NativeMethods.WICKRA_SPREAD_BOLLINGER_BANDS_BATCH)
                    .invokeExact(handle(), MemorySegment.ofArray(x), MemorySegment.ofArray(y), MemorySegment.ofArray(output), (long) n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * The batch into a flat buffer of
     * 4 doubles per input, one row per input in the order of {@link SpreadBollingerBandsOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>Zero-copy form over caller-owned native memory: every input segment must be
     * off-heap, aligned for its element type and hold the same number of elements,
     * {@code output} 4 doubles per input. Nothing is copied or allocated.
     */
    public void batchInto(MemorySegment x, MemorySegment y, MemorySegment output) {
        long n = x.byteSize() / JAVA_DOUBLE.byteSize();
        WickraNative.checkBatchSegment(x, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(y, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(output, JAVA_DOUBLE, n * 4);
        try {
            NativeMethods.WICKRA_SPREAD_BOLLINGER_BANDS_BATCH.invokeExact(handle(), x, y, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_SPREAD_BOLLINGER_BANDS_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_SPREAD_BOLLINGER_BANDS_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_SPREAD_BOLLINGER_BANDS_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_SPREAD_BOLLINGER_BANDS_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("SpreadBollingerBands has been closed");
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
