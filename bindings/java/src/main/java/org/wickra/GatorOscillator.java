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

/** Streaming GatorOscillator indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class GatorOscillator implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_GATOR_OSCILLATOR_UPDATE;
    /** Where update receives its result, allocated once. */
    private final MemorySegment updateOut = Arena.ofAuto().allocate(16L);

    public GatorOscillator(int jawPeriod, int teethPeriod, int lipsPeriod) {
        if (jawPeriod < 0) {
            throw new IllegalArgumentException("jawPeriod must be non-negative");
        }
        if (teethPeriod < 0) {
            throw new IllegalArgumentException("teethPeriod must be non-negative");
        }
        if (lipsPeriod < 0) {
            throw new IllegalArgumentException("lipsPeriod must be non-negative");
        }
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_GATOR_OSCILLATOR_NEW.invokeExact((long) jawPeriod, (long) teethPeriod, (long) lipsPeriod);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid GatorOscillator parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_GATOR_OSCILLATOR_FREE);
    }

    /** Push one observation; returns the result, or null during warmup. */
    public GatorOscillatorOutput update(double open, double high, double low, double close, double volume, long timestamp) {
        try {
            MemorySegment out = updateOut;
            byte ok = (byte) UPDATE.invokeExact(handle(), open, high, low, close, volume, timestamp, out);
            if (ok == 0) {
                return null;
            }
            return new GatorOscillatorOutput(
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
    public GatorOscillatorOutput[] batch(double[] open, double[] high, double[] low, double[] close, double[] volume, long[] timestamp) {
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
            NativeMethods.WICKRA_GATOR_OSCILLATOR_BATCH.invokeExact(handle(), openSeg, highSeg, lowSeg, closeSeg, volumeSeg, timestampSeg, outSeg, (long) n);
            GatorOscillatorOutput[] out = new GatorOscillatorOutput[n];
            for (int i = 0; i < n; i++) {
                out[i] = new GatorOscillatorOutput(
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
     * 2 doubles per input, one row per input in the order of {@link GatorOscillatorOutput}'s
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
            NativeMethods.WICKRA_GATOR_OSCILLATOR_BATCH.invokeExact(handle(), openSeg, highSeg, lowSeg, closeSeg, volumeSeg, timestampSeg, outSeg, (long) n);
            MemorySegment.copy(outSeg, JAVA_DOUBLE, 0L, output, 0, output.length);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * The batch into a flat buffer of
     * 2 doubles per input, one row per input in the order of {@link GatorOscillatorOutput}'s
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
            NativeMethods.WICKRA_GATOR_OSCILLATOR_BATCH.invokeExact(handle(), open, high, low, close, volume, timestamp, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_GATOR_OSCILLATOR_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_GATOR_OSCILLATOR_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_GATOR_OSCILLATOR_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_GATOR_OSCILLATOR_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("GatorOscillator has been closed");
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
