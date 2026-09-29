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

/** Streaming ThreeLineBreakBars indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class ThreeLineBreakBars implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_THREE_LINE_BREAK_BARS_UPDATE;

    public ThreeLineBreakBars(int lines) {
        if (lines < 0) {
            throw new IllegalArgumentException("lines must be non-negative");
        }
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_THREE_LINE_BREAK_BARS_NEW.invokeExact((long) lines);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid ThreeLineBreakBars parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_THREE_LINE_BREAK_BARS_FREE);
    }

    /** Push one observation; returns the bars completed by it (possibly empty). */
    public LineBreakBar[] update(double open, double high, double low, double close, double volume, long timestamp) {
        final long cap = 64L;
        try (Arena a = Arena.ofConfined()) {
            MemorySegment out = a.allocate(24L * cap);
            long n = (long) UPDATE.invokeExact(handle(), open, high, low, close, volume, timestamp, out, cap);
            if (n <= 0) {
                return new LineBreakBar[0];
            }
            LineBreakBar[] result = new LineBreakBar[(int) n];
            long written = Math.min(n, cap);
            for (int i = 0; i < written; i++) {
                long b = (long) i * 24L;
                result[i] = new LineBreakBar(
                    out.get(JAVA_DOUBLE, b + 0L),
                    out.get(JAVA_DOUBLE, b + 8L),
                    (double) out.get(JAVA_BYTE, b + 16L));
            }
            if (n > cap) {
                // One input produced more elements than the buffer holds;
                // the surplus waits on the handle rather than being dropped.
                MemorySegment more = a.allocate(24L * (n - cap));
                long drained = (long) NativeMethods.WICKRA_THREE_LINE_BREAK_BARS_DRAIN.invokeExact(handle(), more, n - cap);
                for (int i = 0; i < drained; i++) {
                    long b = (long) i * 24L;
                    result[(int) cap + i] = new LineBreakBar(
                    more.get(JAVA_DOUBLE, b + 0L),
                    more.get(JAVA_DOUBLE, b + 8L),
                    (double) more.get(JAVA_BYTE, b + 16L));
                }
            }
            return result;
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Feeds a whole series in one native call and returns every bar it
     * completed. The count depends on the data, not on the input length.
     */
    public LineBreakBar[] batch(double[] open, double[] high, double[] low, double[] close, double[] volume, long[] timestamp) {
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
            return new LineBreakBar[0];
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment openSeg = a.allocateFrom(JAVA_DOUBLE, open);
            MemorySegment highSeg = a.allocateFrom(JAVA_DOUBLE, high);
            MemorySegment lowSeg = a.allocateFrom(JAVA_DOUBLE, low);
            MemorySegment closeSeg = a.allocateFrom(JAVA_DOUBLE, close);
            MemorySegment volumeSeg = a.allocateFrom(JAVA_DOUBLE, volume);
            MemorySegment timestampSeg = a.allocateFrom(JAVA_LONG, timestamp);
            long total = (long) NativeMethods.WICKRA_THREE_LINE_BREAK_BARS_BATCH.invokeExact(handle(), openSeg, highSeg, lowSeg, closeSeg, volumeSeg, timestampSeg, (long) n);
            if (total <= 0) {
                return new LineBreakBar[0];
            }
            MemorySegment buf = a.allocate(24L * total);
            long drained = (long) NativeMethods.WICKRA_THREE_LINE_BREAK_BARS_DRAIN.invokeExact(handle(), buf, total);
            LineBreakBar[] result = new LineBreakBar[(int) drained];
            for (int i = 0; i < drained; i++) {
                long b = (long) i * 24L;
                result[i] = new LineBreakBar(
                    buf.get(JAVA_DOUBLE, b + 0L),
                    buf.get(JAVA_DOUBLE, b + 8L),
                    (double) buf.get(JAVA_BYTE, b + 16L));
            }
            return result;
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The indicator's canonical name. */
    public String name() {
        try {
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_THREE_LINE_BREAK_BARS_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_THREE_LINE_BREAK_BARS_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("ThreeLineBreakBars has been closed");
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
