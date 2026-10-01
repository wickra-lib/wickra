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

/** Streaming ZeroLagMacd indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class ZeroLagMacd implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_ZERO_LAG_MACD_UPDATE;
    /** Where update receives its result, allocated once. */
    private final MemorySegment updateOut = Arena.ofAuto().allocate(24L);

    public ZeroLagMacd(int fast, int slow, int signal) {
        if (fast < 0) {
            throw new IllegalArgumentException("fast must be non-negative");
        }
        if (slow < 0) {
            throw new IllegalArgumentException("slow must be non-negative");
        }
        if (signal < 0) {
            throw new IllegalArgumentException("signal must be non-negative");
        }
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_ZERO_LAG_MACD_NEW.invokeExact((long) fast, (long) slow, (long) signal);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid ZeroLagMacd parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_ZERO_LAG_MACD_FREE);
    }

    /** Push one observation; returns the result, or null during warmup. */
    public ZeroLagMacdOutput update(double value) {
        try {
            MemorySegment out = updateOut;
            byte ok = (byte) UPDATE.invokeExact(handle(), value, out);
            if (ok == 0) {
                return null;
            }
            return new ZeroLagMacdOutput(
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
    public ZeroLagMacdOutput[] batch(double[] input) {
        int n = input.length;
        try (Arena a = Arena.ofConfined()) {
            MemorySegment inputSeg = a.allocateFrom(JAVA_DOUBLE, input);
            MemorySegment outSeg = a.allocate(24L * n);
            NativeMethods.WICKRA_ZERO_LAG_MACD_BATCH.invokeExact(handle(), inputSeg, outSeg, (long) n);
            ZeroLagMacdOutput[] out = new ZeroLagMacdOutput[n];
            for (int i = 0; i < n; i++) {
                out[i] = new ZeroLagMacdOutput(
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
     * 3 doubles per input, one row per input in the order of {@link ZeroLagMacdOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>{@code output} must hold 3 values per input.
     */
    public void batchInto(double[] input, double[] output) {
        int n = input.length;
        if (output.length != (long) n * 3) {
            throw new IllegalArgumentException("the output array must hold 3 values per input");
        }
        try {
            WickraNative.heapDowncall("wickra_zero_lag_macd_batch", NativeMethods.WICKRA_ZERO_LAG_MACD_BATCH)
                    .invokeExact(handle(), MemorySegment.ofArray(input), MemorySegment.ofArray(output), (long) n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * The batch into a flat buffer of
     * 3 doubles per input, one row per input in the order of {@link ZeroLagMacdOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>Zero-copy form over caller-owned native memory: every input segment must be
     * off-heap, aligned for its element type and hold the same number of elements,
     * {@code output} 3 doubles per input. Nothing is copied or allocated.
     */
    public void batchInto(MemorySegment input, MemorySegment output) {
        long n = input.byteSize() / JAVA_DOUBLE.byteSize();
        WickraNative.checkBatchSegment(input, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(output, JAVA_DOUBLE, n * 3);
        try {
            NativeMethods.WICKRA_ZERO_LAG_MACD_BATCH.invokeExact(handle(), input, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_ZERO_LAG_MACD_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_ZERO_LAG_MACD_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_ZERO_LAG_MACD_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_ZERO_LAG_MACD_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("ZeroLagMacd has been closed");
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
