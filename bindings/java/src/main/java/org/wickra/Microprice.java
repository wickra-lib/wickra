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

/** Streaming Microprice indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class Microprice implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_MICROPRICE_UPDATE;

    public Microprice() {
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_MICROPRICE_NEW.invokeExact();
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid Microprice parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_MICROPRICE_FREE);
    }

    /** Push one observation; returns the indicator value (NaN during warmup). */
    public double update(double[] bidPrice, double[] bidSize, double[] askPrice, double[] askSize) {
        if (bidSize.length != bidPrice.length) {
            throw new IllegalArgumentException("input arrays in the same group must have equal length");
        }
        if (askSize.length != askPrice.length) {
            throw new IllegalArgumentException("input arrays in the same group must have equal length");
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment bidPriceSeg = a.allocateFrom(JAVA_DOUBLE, bidPrice);
            MemorySegment bidSizeSeg = a.allocateFrom(JAVA_DOUBLE, bidSize);
            MemorySegment askPriceSeg = a.allocateFrom(JAVA_DOUBLE, askPrice);
            MemorySegment askSizeSeg = a.allocateFrom(JAVA_DOUBLE, askSize);
            return (double) UPDATE.invokeExact(handle(), bidPriceSeg, bidSizeSeg, (long) bidPrice.length, askPriceSeg, askSizeSeg, (long) askPrice.length);
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
    public double[] batch(double[] bidPrice, double[] bidSize, int nBids, double[] askPrice, double[] askSize, int nAsks) {
        if (nBids <= 0) {
            throw new IllegalArgumentException("the per-bar width must be positive");
        }
        if (nAsks <= 0) {
            throw new IllegalArgumentException("the per-bar width must be positive");
        }
        int n = bidPrice.length / nBids;
        if (bidPrice.length != n * nBids) {
            throw new IllegalArgumentException("every input array must cover the whole series");
        }
        if (bidSize.length != n * nBids) {
            throw new IllegalArgumentException("every input array must cover the whole series");
        }
        if (askPrice.length != n * nAsks) {
            throw new IllegalArgumentException("every input array must cover the whole series");
        }
        if (askSize.length != n * nAsks) {
            throw new IllegalArgumentException("every input array must cover the whole series");
        }
        double[] out = new double[n];
        if (n == 0) {
            return out;
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment bidPriceSeg = a.allocateFrom(JAVA_DOUBLE, bidPrice);
            MemorySegment bidSizeSeg = a.allocateFrom(JAVA_DOUBLE, bidSize);
            MemorySegment askPriceSeg = a.allocateFrom(JAVA_DOUBLE, askPrice);
            MemorySegment askSizeSeg = a.allocateFrom(JAVA_DOUBLE, askSize);
            MemorySegment outSeg = a.allocate(JAVA_DOUBLE.byteSize() * n);
            NativeMethods.WICKRA_MICROPRICE_BATCH.invokeExact(handle(), bidPriceSeg, bidSizeSeg, (long) nBids, askPriceSeg, askSizeSeg, (long) nAsks, outSeg, (long) n);
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
            long n = (long) NativeMethods.WICKRA_MICROPRICE_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_MICROPRICE_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_MICROPRICE_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_MICROPRICE_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("Microprice has been closed");
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
