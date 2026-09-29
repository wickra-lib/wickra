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

/** Streaming PercentAboveMa indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class PercentAboveMa implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_PERCENT_ABOVE_MA_UPDATE;

    public PercentAboveMa() {
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_PERCENT_ABOVE_MA_NEW.invokeExact();
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid PercentAboveMa parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_PERCENT_ABOVE_MA_FREE);
    }

    /** Push one observation; returns the indicator value (NaN during warmup). */
    public double update(double[] change, double[] volume, boolean[] newHigh, boolean[] newLow, boolean[] aboveMa, boolean[] onBuySignal, long timestamp) {
        if (volume.length != change.length) {
            throw new IllegalArgumentException("input arrays in the same group must have equal length");
        }
        if (newHigh.length != change.length) {
            throw new IllegalArgumentException("input arrays in the same group must have equal length");
        }
        if (newLow.length != change.length) {
            throw new IllegalArgumentException("input arrays in the same group must have equal length");
        }
        if (aboveMa.length != change.length) {
            throw new IllegalArgumentException("input arrays in the same group must have equal length");
        }
        if (onBuySignal.length != change.length) {
            throw new IllegalArgumentException("input arrays in the same group must have equal length");
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment changeSeg = a.allocateFrom(JAVA_DOUBLE, change);
            MemorySegment volumeSeg = a.allocateFrom(JAVA_DOUBLE, volume);
            MemorySegment newHighSeg = WickraNative.boolSegment(a, newHigh);
            MemorySegment newLowSeg = WickraNative.boolSegment(a, newLow);
            MemorySegment aboveMaSeg = WickraNative.boolSegment(a, aboveMa);
            MemorySegment onBuySignalSeg = WickraNative.boolSegment(a, onBuySignal);
            return (double) UPDATE.invokeExact(handle(), changeSeg, volumeSeg, newHighSeg, newLowSeg, aboveMaSeg, onBuySignalSeg, (long) change.length, timestamp);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Feeds a whole series in one native call and returns the per-bar output.
     * Every snapshot carries the same width, so the per-member arrays are
     * flat: bar i occupies elements [i*width, (i+1)*width).
     */
    public double[] batch(double[] change, double[] volume, boolean[] newHigh, boolean[] newLow, boolean[] aboveMa, boolean[] onBuySignal, int members, long[] timestamp) {
        if (members <= 0) {
            throw new IllegalArgumentException("the per-bar width must be positive");
        }
        int n = timestamp.length;
        if (change.length != n * members) {
            throw new IllegalArgumentException("every input array must cover the whole series");
        }
        if (volume.length != n * members) {
            throw new IllegalArgumentException("every input array must cover the whole series");
        }
        if (newHigh.length != n * members) {
            throw new IllegalArgumentException("every input array must cover the whole series");
        }
        if (newLow.length != n * members) {
            throw new IllegalArgumentException("every input array must cover the whole series");
        }
        if (aboveMa.length != n * members) {
            throw new IllegalArgumentException("every input array must cover the whole series");
        }
        if (onBuySignal.length != n * members) {
            throw new IllegalArgumentException("every input array must cover the whole series");
        }
        if (timestamp.length != n) {
            throw new IllegalArgumentException("every input array must cover the whole series");
        }
        double[] out = new double[n];
        if (n == 0) {
            return out;
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment changeSeg = a.allocateFrom(JAVA_DOUBLE, change);
            MemorySegment volumeSeg = a.allocateFrom(JAVA_DOUBLE, volume);
            MemorySegment newHighSeg = WickraNative.boolSegment(a, newHigh);
            MemorySegment newLowSeg = WickraNative.boolSegment(a, newLow);
            MemorySegment aboveMaSeg = WickraNative.boolSegment(a, aboveMa);
            MemorySegment onBuySignalSeg = WickraNative.boolSegment(a, onBuySignal);
            MemorySegment timestampSeg = a.allocateFrom(JAVA_LONG, timestamp);
            MemorySegment outSeg = a.allocate(JAVA_DOUBLE.byteSize() * n);
            NativeMethods.WICKRA_PERCENT_ABOVE_MA_BATCH.invokeExact(handle(), changeSeg, volumeSeg, newHighSeg, newLowSeg, aboveMaSeg, onBuySignalSeg, (long) members, timestampSeg, outSeg, (long) n);
            MemorySegment.copy(outSeg, JAVA_DOUBLE, 0L, out, 0, n);
            return out;
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_PERCENT_ABOVE_MA_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_PERCENT_ABOVE_MA_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_PERCENT_ABOVE_MA_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_PERCENT_ABOVE_MA_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("PercentAboveMa has been closed");
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
