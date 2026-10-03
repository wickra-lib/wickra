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

/** Streaming LiquidationFeatures indicator over the Wickra C ABI. Not thread-safe; close when done. */
public final class LiquidationFeatures implements AutoCloseable {
    private final MemorySegment handle;
    private final Cleaner.Cleanable cleanable;
    private boolean closed;
    private static final MethodHandle UPDATE = NativeMethods.WICKRA_LIQUIDATION_FEATURES_UPDATE;
    /** Where update receives its result, allocated once. */
    private final MemorySegment updateOut = Arena.ofAuto().allocate(40L);

    public LiquidationFeatures() {
        MemorySegment h;
        try {
            h = (MemorySegment) NativeMethods.WICKRA_LIQUIDATION_FEATURES_NEW.invokeExact();
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        }
        if (h.address() == 0L) {
            throw new IllegalArgumentException("invalid LiquidationFeatures parameters");
        }
        this.handle = h;
        this.cleanable = WickraNative.register(this, h, NativeMethods.WICKRA_LIQUIDATION_FEATURES_FREE);
    }

    /** Push one observation; returns the result, or null during warmup. */
    public LiquidationFeaturesOutput update(double fundingRate, double markPrice, double indexPrice, double futuresPrice, double openInterest, double longSize, double shortSize, double takerBuyVolume, double takerSellVolume, double longLiquidation, double shortLiquidation, long timestamp) {
        try {
            MemorySegment out = updateOut;
            byte ok = (byte) UPDATE.invokeExact(handle(), fundingRate, markPrice, indexPrice, futuresPrice, openInterest, longSize, shortSize, takerBuyVolume, takerSellVolume, longLiquidation, shortLiquidation, timestamp, out);
            if (ok == 0) {
                return null;
            }
            return new LiquidationFeaturesOutput(
                out.get(JAVA_DOUBLE, 0L),
                out.get(JAVA_DOUBLE, 8L),
                out.get(JAVA_DOUBLE, 16L),
                out.get(JAVA_DOUBLE, 24L),
                out.get(JAVA_DOUBLE, 32L));
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
    public LiquidationFeaturesOutput[] batch(double[] fundingRate, double[] markPrice, double[] indexPrice, double[] futuresPrice, double[] openInterest, double[] longSize, double[] shortSize, double[] takerBuyVolume, double[] takerSellVolume, double[] longLiquidation, double[] shortLiquidation, long[] timestamp) {
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
            MemorySegment outSeg = a.allocate(40L * n);
            NativeMethods.WICKRA_LIQUIDATION_FEATURES_BATCH.invokeExact(handle(), fundingRateSeg, markPriceSeg, indexPriceSeg, futuresPriceSeg, openInterestSeg, longSizeSeg, shortSizeSeg, takerBuyVolumeSeg, takerSellVolumeSeg, longLiquidationSeg, shortLiquidationSeg, timestampSeg, outSeg, (long) n);
            LiquidationFeaturesOutput[] out = new LiquidationFeaturesOutput[n];
            for (int i = 0; i < n; i++) {
                out[i] = new LiquidationFeaturesOutput(
                        outSeg.get(JAVA_DOUBLE, i * 40L + 0L),
                        outSeg.get(JAVA_DOUBLE, i * 40L + 8L),
                        outSeg.get(JAVA_DOUBLE, i * 40L + 16L),
                        outSeg.get(JAVA_DOUBLE, i * 40L + 24L),
                        outSeg.get(JAVA_DOUBLE, i * 40L + 32L));
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
     * 5 doubles per input, one row per input in the order of {@link LiquidationFeaturesOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>{@code output} must hold 5 values per input.
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
        if (output.length != (long) n * 5) {
            throw new IllegalArgumentException("the output array must hold 5 values per input");
        }
        try {
            WickraNative.heapDowncall("wickra_liquidation_features_batch", NativeMethods.WICKRA_LIQUIDATION_FEATURES_BATCH)
                    .invokeExact(handle(), MemorySegment.ofArray(fundingRate), MemorySegment.ofArray(markPrice), MemorySegment.ofArray(indexPrice), MemorySegment.ofArray(futuresPrice), MemorySegment.ofArray(openInterest), MemorySegment.ofArray(longSize), MemorySegment.ofArray(shortSize), MemorySegment.ofArray(takerBuyVolume), MemorySegment.ofArray(takerSellVolume), MemorySegment.ofArray(longLiquidation), MemorySegment.ofArray(shortLiquidation), MemorySegment.ofArray(timestamp), MemorySegment.ofArray(output), (long) n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * The batch into a flat buffer of
     * 5 doubles per input, one row per input in the order of {@link LiquidationFeaturesOutput}'s
     * components (NaN rows during warmup), without allocating the records.
     * 
     * <p>Zero-copy form over caller-owned native memory: every input segment must be
     * off-heap, aligned for its element type and hold the same number of elements,
     * {@code output} 5 doubles per input. Nothing is copied or allocated.
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
        WickraNative.checkBatchSegment(output, JAVA_DOUBLE, n * 5);
        try {
            NativeMethods.WICKRA_LIQUIDATION_FEATURES_BATCH.invokeExact(handle(), fundingRate, markPrice, indexPrice, futuresPrice, openInterest, longSize, shortSize, takerBuyVolume, takerSellVolume, longLiquidation, shortLiquidation, timestamp, output, n);
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Number of updates required before update() yields a value. */
    public int warmupPeriod() {
        try {
            long n = (long) NativeMethods.WICKRA_LIQUIDATION_FEATURES_WARMUP_PERIOD.invokeExact(handle());
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
            byte r = (byte) NativeMethods.WICKRA_LIQUIDATION_FEATURES_IS_READY.invokeExact(handle());
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
            MemorySegment s = (MemorySegment) NativeMethods.WICKRA_LIQUIDATION_FEATURES_NAME.invokeExact(handle());
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
            NativeMethods.WICKRA_LIQUIDATION_FEATURES_RESET.invokeExact(handle());
        } catch (Throwable t) {
            throw WickraNative.rethrow(t);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** The native handle, refusing to hand out one that has been released. */
    private MemorySegment handle() {
        if (closed) {
            throw new IllegalStateException("LiquidationFeatures has been closed");
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
