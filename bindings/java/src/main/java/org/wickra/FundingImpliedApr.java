// Generated from bindings/c/include/wickra.h. Do not edit by hand.
package org.wickra;

import org.wickra.internal.NativeMethods;
import org.wickra.internal.WickraNative;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.ref.Cleaner;
import java.lang.ref.Reference;
import static java.lang.foreign.ValueLayout.*;

/** Streaming FundingImpliedApr indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class FundingImpliedApr implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;

    public FundingImpliedApr(double intervalsPerYear) {
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_FUNDING_IMPLIED_APR_NEW.invokeExact(intervalsPerYear);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid FundingImpliedApr parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_FUNDING_IMPLIED_APR_FREE);
    }

    /** Push one observation; returns the indicator value (NaN during warmup). */
    public double update(double fundingRate, double markPrice, double indexPrice, double futuresPrice, double openInterest, double longSize, double shortSize, double takerBuyVolume, double takerSellVolume, double longLiquidation, double shortLiquidation, long timestamp) {
        try {
            return (double) NativeMethods.WICKRA_FUNDING_IMPLIED_APR_UPDATE.invokeExact(handle(), fundingRate, markPrice, indexPrice, futuresPrice, openInterest, longSize, shortSize, takerBuyVolume, takerSellVolume, longLiquidation, shortLiquidation, timestamp);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Vectorized update over a whole series; NaN at warmup positions, bit for
     * bit what feeding the values one by one through {@code update} gives.
     */
    public double[] batch(double[] fundingRate, double[] markPrice, double[] indexPrice, double[] futuresPrice, double[] openInterest, double[] longSize, double[] shortSize, double[] takerBuyVolume, double[] takerSellVolume, double[] longLiquidation, double[] shortLiquidation, long[] timestamp) {
        double[] output = new double[fundingRate.length];
        batchInto(fundingRate, markPrice, indexPrice, futuresPrice, openInterest, longSize, shortSize, takerBuyVolume, takerSellVolume, longLiquidation, shortLiquidation, timestamp, output);
        return output;
    }

    /**
     * Vectorized update over a whole series; NaN at warmup positions, bit for
     * bit what feeding the values one by one through {@code update} gives.
     * 
     * <p>Writes into {@code output}, which must be as long as the input.
     */
    public void batchInto(double[] fundingRate, double[] markPrice, double[] indexPrice, double[] futuresPrice, double[] openInterest, double[] longSize, double[] shortSize, double[] takerBuyVolume, double[] takerSellVolume, double[] longLiquidation, double[] shortLiquidation, long[] timestamp, double[] output) {
        int n = fundingRate.length;
        if (markPrice.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (indexPrice.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (futuresPrice.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (openInterest.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (longSize.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (shortSize.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (takerBuyVolume.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (takerSellVolume.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (longLiquidation.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (shortLiquidation.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (timestamp.length != n) {
            throw new IllegalArgumentException("all input arrays must have the same length");
        }
        if (output.length != n) {
            throw new IllegalArgumentException("the output array must be as long as the input");
        }
        try (Arena a = Arena.ofConfined()) {
            MemorySegment fundingRateSeg = a.allocateFrom(JAVA_DOUBLE, fundingRate);
            MemorySegment markPriceSeg = a.allocateFrom(JAVA_DOUBLE, markPrice);
            MemorySegment indexPriceSeg = a.allocateFrom(JAVA_DOUBLE, indexPrice);
            MemorySegment futuresPriceSeg = a.allocateFrom(JAVA_DOUBLE, futuresPrice);
            MemorySegment openInterestSeg = a.allocateFrom(JAVA_DOUBLE, openInterest);
            MemorySegment longSizeSeg = a.allocateFrom(JAVA_DOUBLE, longSize);
            MemorySegment shortSizeSeg = a.allocateFrom(JAVA_DOUBLE, shortSize);
            MemorySegment takerBuyVolumeSeg = a.allocateFrom(JAVA_DOUBLE, takerBuyVolume);
            MemorySegment takerSellVolumeSeg = a.allocateFrom(JAVA_DOUBLE, takerSellVolume);
            MemorySegment longLiquidationSeg = a.allocateFrom(JAVA_DOUBLE, longLiquidation);
            MemorySegment shortLiquidationSeg = a.allocateFrom(JAVA_DOUBLE, shortLiquidation);
            MemorySegment timestampSeg = a.allocateFrom(JAVA_LONG, timestamp);
            MemorySegment outSeg = a.allocate(JAVA_DOUBLE, n);
            NativeMethods.WICKRA_FUNDING_IMPLIED_APR_BATCH.invokeExact(handle(), fundingRateSeg, markPriceSeg, indexPriceSeg, futuresPriceSeg, openInterestSeg, longSizeSeg, shortSizeSeg, takerBuyVolumeSeg, takerSellVolumeSeg, longLiquidationSeg, shortLiquidationSeg, timestampSeg, outSeg, (long) n);
            MemorySegment.copy(outSeg, JAVA_DOUBLE, 0L, output, 0, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Vectorized update over a whole series; NaN at warmup positions, bit for
     * bit what feeding the values one by one through {@code update} gives.
     * 
     * <p>Zero-copy form over caller-owned native memory: every segment must be
     * off-heap, aligned for its element type, and hold the same number of
     * elements, {@code output} as many doubles. Nothing is copied or allocated.
     */
    public void batchInto(MemorySegment fundingRate, MemorySegment markPrice, MemorySegment indexPrice, MemorySegment futuresPrice, MemorySegment openInterest, MemorySegment longSize, MemorySegment shortSize, MemorySegment takerBuyVolume, MemorySegment takerSellVolume, MemorySegment longLiquidation, MemorySegment shortLiquidation, MemorySegment timestamp, MemorySegment output) {
        long n = fundingRate.byteSize() / JAVA_DOUBLE.byteSize();
        WickraNative.checkBatchSegment(fundingRate, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(markPrice, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(indexPrice, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(futuresPrice, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(openInterest, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(longSize, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(shortSize, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(takerBuyVolume, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(takerSellVolume, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(longLiquidation, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(shortLiquidation, JAVA_DOUBLE, n);
        WickraNative.checkBatchSegment(timestamp, JAVA_LONG, n);
        WickraNative.checkBatchSegment(output, JAVA_DOUBLE, n);
        try {
            NativeMethods.WICKRA_FUNDING_IMPLIED_APR_BATCH.invokeExact(handle(), fundingRate, markPrice, indexPrice, futuresPrice, openInterest, longSize, shortSize, takerBuyVolume, takerSellVolume, longLiquidation, shortLiquidation, timestamp, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_FUNDING_IMPLIED_APR_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_FUNDING_IMPLIED_APR_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_FUNDING_IMPLIED_APR_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_FUNDING_IMPLIED_APR_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("FundingImpliedApr has been closed");
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
