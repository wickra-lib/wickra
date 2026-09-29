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

/** Streaming IntradayVolatilityProfile indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class IntradayVolatilityProfile implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_INTRADAY_VOLATILITY_PROFILE_UPDATE;
    private final int valuesCapacity;

    public IntradayVolatilityProfile(int buckets, int utcOffsetMinutes) {
        if (buckets < 0) {
            throw new IllegalArgumentException("buckets must be non-negative");
        }
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_INTRADAY_VOLATILITY_PROFILE_NEW.invokeExact((long) buckets, utcOffsetMinutes);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid IntradayVolatilityProfile parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_INTRADAY_VOLATILITY_PROFILE_FREE);
        try {
            this.valuesCapacity = (int) (long) NativeMethods.WICKRA_INTRADAY_VOLATILITY_PROFILE_WIDTH.invokeExact(h);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
    }

    /** Push one observation; returns the profile values, or null during warmup. */
    public double[] update(double open, double high, double low, double close, double volume, long timestamp) {
        long cap = valuesCapacity;
        try (Arena a = Arena.ofConfined()) {
            MemorySegment values = a.allocate(JAVA_DOUBLE.byteSize() * cap);
            long len = (long) UPDATE.invokeExact(handle(), open, high, low, close, volume, timestamp, values, cap);
            if (len < 0) {
                return null;
            }
            int count = (int) Math.min(len, cap);
            double[] v = new double[count];
            MemorySegment.copy(values, JAVA_DOUBLE, 0L, v, 0, count);
            return v;
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Feeds a whole series in one native call and returns one profile per
     * input. A row the indicator did not produce carries NaN.
     */
    public double[][] batch(double[] open, double[] high, double[] low, double[] close, double[] volume, long[] timestamp) {
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
        if (n == 0) {
            return new double[0][];
        }
        int width = (int) valuesCapacity;
        try (Arena a = Arena.ofConfined()) {
            MemorySegment openSeg = a.allocateFrom(JAVA_DOUBLE, open);
            MemorySegment highSeg = a.allocateFrom(JAVA_DOUBLE, high);
            MemorySegment lowSeg = a.allocateFrom(JAVA_DOUBLE, low);
            MemorySegment closeSeg = a.allocateFrom(JAVA_DOUBLE, close);
            MemorySegment volumeSeg = a.allocateFrom(JAVA_DOUBLE, volume);
            MemorySegment timestampSeg = a.allocateFrom(JAVA_LONG, timestamp);
            MemorySegment flat = a.allocate(JAVA_DOUBLE.byteSize() * width * n);
            NativeMethods.WICKRA_INTRADAY_VOLATILITY_PROFILE_BATCH.invokeExact(handle(), openSeg, highSeg, lowSeg, closeSeg, volumeSeg, timestampSeg, flat, (long) width, (long) n);
            double[][] result = new double[n][];
            for (int i = 0; i < n; i++) {
                double[] row = new double[width];
                MemorySegment.copy(flat, JAVA_DOUBLE, (long) i * width * JAVA_DOUBLE.byteSize(), row, 0, width);
                result[i] = row;
            }
            return result;
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_INTRADAY_VOLATILITY_PROFILE_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_INTRADAY_VOLATILITY_PROFILE_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_INTRADAY_VOLATILITY_PROFILE_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_INTRADAY_VOLATILITY_PROFILE_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("IntradayVolatilityProfile has been closed");
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
