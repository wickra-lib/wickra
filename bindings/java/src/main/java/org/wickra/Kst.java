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

/** Streaming Kst indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class Kst implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_KST_UPDATE;
    /** Where update receives its result, allocated once. */
    private final MemorySegment updateOut = Arena.ofAuto().allocate(16L);

    public Kst(int roc1, int roc2, int roc3, int roc4, int sma1, int sma2, int sma3, int sma4, int signal) {
        if (roc1 < 0) {
            throw new IllegalArgumentException("roc1 must be non-negative");
        }
        if (roc2 < 0) {
            throw new IllegalArgumentException("roc2 must be non-negative");
        }
        if (roc3 < 0) {
            throw new IllegalArgumentException("roc3 must be non-negative");
        }
        if (roc4 < 0) {
            throw new IllegalArgumentException("roc4 must be non-negative");
        }
        if (sma1 < 0) {
            throw new IllegalArgumentException("sma1 must be non-negative");
        }
        if (sma2 < 0) {
            throw new IllegalArgumentException("sma2 must be non-negative");
        }
        if (sma3 < 0) {
            throw new IllegalArgumentException("sma3 must be non-negative");
        }
        if (sma4 < 0) {
            throw new IllegalArgumentException("sma4 must be non-negative");
        }
        if (signal < 0) {
            throw new IllegalArgumentException("signal must be non-negative");
        }
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_KST_NEW.invokeExact((long) roc1, (long) roc2, (long) roc3, (long) roc4, (long) sma1, (long) sma2, (long) sma3, (long) sma4, (long) signal);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid Kst parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_KST_FREE);
    }

    /** Push one observation; returns the result, or null during warmup. */
    public KstOutput update(double value) {
        try {
            MemorySegment out = updateOut;
            byte ok = (byte) UPDATE.invokeExact(handle(), value, out);
            if (ok == 0) {
                return null;
            }
            return new KstOutput(
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
    public KstOutput[] batch(double[] input) {
        int n = input.length;
        try (Arena a = Arena.ofConfined()) {
            MemorySegment inputSeg = a.allocateFrom(JAVA_DOUBLE, input);
            MemorySegment outSeg = a.allocate(16L * n);
            NativeMethods.WICKRA_KST_BATCH.invokeExact(handle(), inputSeg, outSeg, (long) n);
            KstOutput[] out = new KstOutput[n];
            for (int i = 0; i < n; i++) {
                out[i] = new KstOutput(
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
     * 2 doubles per input, one row per input in the order of {@link KstOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>{@code output} must hold 2 values per input.
     */
    public void batchInto(double[] input, double[] output) {
        int n = input.length;
        if (output.length != (long) n * 2) {
            throw new IllegalArgumentException("the output array must hold 2 values per input");
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment inputSeg = a.allocateFrom(JAVA_DOUBLE, input);
            MemorySegment outSeg = a.allocate(JAVA_DOUBLE, output.length);
            NativeMethods.WICKRA_KST_BATCH.invokeExact(handle(), inputSeg, outSeg, (long) n);
            MemorySegment.copy(outSeg, JAVA_DOUBLE, 0L, output, 0, output.length);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * The batch into a flat buffer of
     * 2 doubles per input, one row per input in the order of {@link KstOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>Zero-copy form over caller-owned native memory: every input segment must be
     * off-heap, aligned for its element type and hold the same number of elements,
     * {@code output} 2 doubles per input. Nothing is copied or allocated.
     */
    public void batchInto(MemorySegment input, MemorySegment output) {
        long n = input.byteSize() / JAVA_DOUBLE.byteSize();
        WickraNative.checkBatchSegment(input, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(output, JAVA_DOUBLE, n * 2);
        try {
            NativeMethods.WICKRA_KST_BATCH.invokeExact(handle(), input, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_KST_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_KST_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_KST_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_KST_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("Kst has been closed");
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
