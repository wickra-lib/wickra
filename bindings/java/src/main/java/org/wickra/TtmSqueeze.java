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

/** Streaming TtmSqueeze indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class TtmSqueeze implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_TTM_SQUEEZE_UPDATE;
    /** Where update receives its result, allocated once. */
    private final MemorySegment updateOut = Arena.ofAuto().allocate(16L);

    public TtmSqueeze(int period, double bbMult, double kcMult) {
        if (period < 0) {
            throw new IllegalArgumentException("period must be non-negative");
        }
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_TTM_SQUEEZE_NEW.invokeExact((long) period, bbMult, kcMult);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid TtmSqueeze parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_TTM_SQUEEZE_FREE);
    }

    /** Push one observation; returns the result, or null during warmup. */
    public TtmSqueezeOutput update(double open, double high, double low, double close, double volume, long timestamp) {
        try {
            MemorySegment out = updateOut;
            byte ok = (byte) UPDATE.invokeExact(handle(), open, high, low, close, volume, timestamp, out);
            if (ok == 0) {
                return null;
            }
            return new TtmSqueezeOutput(
                out.get(JAVA_DOUBLE, 0L),
                out.get(JAVA_DOUBLE, 8L));
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
    public TtmSqueezeOutput[] batch(double[] open, double[] high, double[] low, double[] close, double[] volume, long[] timestamp) {
        int n = open.length;
        if (high.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (low.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (close.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (volume.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (timestamp.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment openSeg = a.allocateFrom(JAVA_DOUBLE, open);
            MemorySegment highSeg = a.allocateFrom(JAVA_DOUBLE, high);
            MemorySegment lowSeg = a.allocateFrom(JAVA_DOUBLE, low);
            MemorySegment closeSeg = a.allocateFrom(JAVA_DOUBLE, close);
            MemorySegment volumeSeg = a.allocateFrom(JAVA_DOUBLE, volume);
            MemorySegment timestampSeg = a.allocateFrom(JAVA_LONG, timestamp);
            MemorySegment outSeg = a.allocate(16L * n);
            NativeMethods.WICKRA_TTM_SQUEEZE_BATCH.invokeExact(handle(), openSeg, highSeg, lowSeg, closeSeg, volumeSeg, timestampSeg, outSeg, (long) n);
            TtmSqueezeOutput[] out = new TtmSqueezeOutput[n];
            for (int i = 0; i < n; i++) {
                out[i] = new TtmSqueezeOutput(
                        outSeg.get(JAVA_DOUBLE, i * 16L + 0L),
                        outSeg.get(JAVA_DOUBLE, i * 16L + 8L));
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
     * 2 doubles per input, one row per input in the order of {@link TtmSqueezeOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>{@code output} must hold 2 values per input.
     */
    public void batchInto(double[] open, double[] high, double[] low, double[] close, double[] volume, long[] timestamp, double[] output) {
        int n = open.length;
        if (high.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (low.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (close.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (volume.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (timestamp.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (output.length != (long) n * 2) {
            throw new IllegalArgumentException("the output array must hold 2 values per input");
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment openSeg = a.allocateFrom(JAVA_DOUBLE, open);
            MemorySegment highSeg = a.allocateFrom(JAVA_DOUBLE, high);
            MemorySegment lowSeg = a.allocateFrom(JAVA_DOUBLE, low);
            MemorySegment closeSeg = a.allocateFrom(JAVA_DOUBLE, close);
            MemorySegment volumeSeg = a.allocateFrom(JAVA_DOUBLE, volume);
            MemorySegment timestampSeg = a.allocateFrom(JAVA_LONG, timestamp);
            MemorySegment outSeg = a.allocate(JAVA_DOUBLE, output.length);
            NativeMethods.WICKRA_TTM_SQUEEZE_BATCH.invokeExact(handle(), openSeg, highSeg, lowSeg, closeSeg, volumeSeg, timestampSeg, outSeg, (long) n);
            MemorySegment.copy(outSeg, JAVA_DOUBLE, 0L, output, 0, output.length);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * The batch into a flat buffer of
     * 2 doubles per input, one row per input in the order of {@link TtmSqueezeOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>Zero-copy form over caller-owned native memory: every input segment must be
     * off-heap, aligned for its element type and hold the same number of elements,
     * {@code output} 2 doubles per input. Nothing is copied or allocated.
     */
    public void batchInto(MemorySegment open, MemorySegment high, MemorySegment low, MemorySegment close, MemorySegment volume, MemorySegment timestamp, MemorySegment output) {
        long n = open.byteSize() / JAVA_DOUBLE.byteSize();
        WickraNative.checkBatchSegment(open, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(high, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(low, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(close, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(volume, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(timestamp, JAVA_LONG, n);
        WickraNative.checkBatchSegment(output, JAVA_DOUBLE, n * 2);
        try {
            NativeMethods.WICKRA_TTM_SQUEEZE_BATCH.invokeExact(handle(), open, high, low, close, volume, timestamp, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_TTM_SQUEEZE_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_TTM_SQUEEZE_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_TTM_SQUEEZE_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_TTM_SQUEEZE_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("TtmSqueeze has been closed");
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
