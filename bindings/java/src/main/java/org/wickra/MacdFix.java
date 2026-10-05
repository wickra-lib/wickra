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

/** Streaming MacdFix indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class MacdFix implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_MACD_FIX_UPDATE;
    /** Where update receives its result, allocated once. */
    private final MemorySegment updateOut = Arena.ofAuto().allocate(24L);

    public MacdFix(int signal) {
        if (signal < 0) {
            throw new IllegalArgumentException("signal must be non-negative");
        }
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_MACD_FIX_NEW.invokeExact((long) signal);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid MacdFix parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_MACD_FIX_FREE);
    }

    /** Push one observation; returns the result, or null during warmup. */
    public MacdOutput update(double value) {
        try {
            MemorySegment out = updateOut;
            byte ok = (byte) UPDATE.invokeExact(handle(), value, out);
            if (ok == 0) {
                return null;
            }
            return new MacdOutput(
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
    public MacdOutput[] batch(double[] input) {
        int n = input.length;
        try (Arena a = Arena.ofConfined()) {
            MemorySegment inputSeg = a.allocateFrom(JAVA_DOUBLE, input);
            MemorySegment outSeg = a.allocate(24L * n);
            NativeMethods.WICKRA_MACD_FIX_BATCH.invokeExact(handle(), inputSeg, outSeg, (long) n);
            MacdOutput[] out = new MacdOutput[n];
            for (int i = 0; i < n; i++) {
                out[i] = new MacdOutput(
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
     * 3 doubles per input, one row per input in the order of {@link MacdOutput}'s
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
            WickraNative.heapDowncall("wickra_macd_fix_batch", NativeMethods.WICKRA_MACD_FIX_BATCH)
                    .invokeExact(handle(), MemorySegment.ofArray(input), MemorySegment.ofArray(output), (long) n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * The batch into a flat buffer of
     * 3 doubles per input, one row per input in the order of {@link MacdOutput}'s
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
            NativeMethods.WICKRA_MACD_FIX_BATCH.invokeExact(handle(), input, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Opt-in fast batch: the SIMD kernel reassociates the arithmetic, so each
     * field agrees with {@code batch} to within a few units in the last place
     * rather than bit for bit; warmup rows and length are identical, and the
     * result is the same on every platform.
     */
    public MacdOutput[] batchFast(double[] input) {
        int n = input.length;
        try (Arena a = Arena.ofConfined()) {
            MemorySegment inputSeg = a.allocateFrom(JAVA_DOUBLE, input);
            MemorySegment outSeg = a.allocate(24L * n);
            NativeMethods.WICKRA_MACD_FIX_BATCH_FAST.invokeExact(handle(), inputSeg, outSeg, (long) n);
            MacdOutput[] out = new MacdOutput[n];
            for (int i = 0; i < n; i++) {
                out[i] = new MacdOutput(
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
     * The opt-in fast batch into a flat buffer of
     * 3 doubles per input, one row per input in the order of {@link MacdOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>{@code output} must hold 3 values per input.
     */
    public void batchFastInto(double[] input, double[] output) {
        int n = input.length;
        if (output.length != (long) n * 3) {
            throw new IllegalArgumentException("the output array must hold 3 values per input");
        }
        try {
            WickraNative.heapDowncall("wickra_macd_fix_batch_fast", NativeMethods.WICKRA_MACD_FIX_BATCH_FAST)
                    .invokeExact(handle(), MemorySegment.ofArray(input), MemorySegment.ofArray(output), (long) n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * The opt-in fast batch into a flat buffer of
     * 3 doubles per input, one row per input in the order of {@link MacdOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>Zero-copy form over caller-owned native memory: every input segment must be
     * off-heap, aligned for its element type and hold the same number of elements,
     * {@code output} 3 doubles per input. Nothing is copied or allocated.
     */
    public void batchFastInto(MemorySegment input, MemorySegment output) {
        long n = input.byteSize() / JAVA_DOUBLE.byteSize();
        WickraNative.checkBatchSegment(input, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(output, JAVA_DOUBLE, n * 3);
        try {
            NativeMethods.WICKRA_MACD_FIX_BATCH_FAST.invokeExact(handle(), input, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_MACD_FIX_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_MACD_FIX_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_MACD_FIX_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_MACD_FIX_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("MacdFix has been closed");
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
