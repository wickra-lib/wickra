using System;
using Xunit;

namespace Wickra.Tests;

/// <summary>
/// The caller-buffer <c>Batch</c> overloads and the opt-in <c>BatchFast</c>.
/// <c>Batch</c> into a span is bit for bit the allocating form; <c>BatchFast</c>
/// keeps NaN placement and length and agrees within a documented tolerance.
/// </summary>
public class BatchFastTests
{
    private const int N = 1003;

    /// <summary>A wandering path long enough for every kernel's vector loop and tail.</summary>
    private static double[] Prices(int n)
    {
        var p = new double[n];
        for (var i = 0; i < n; i++)
        {
            p[i] = 100 + Math.Sin(i * 0.0137) * 5 + Math.Cos(i * 0.37);
        }

        return p;
    }

    private static void AssertWithin(double[] exact, double[] fast, double tol)
    {
        Assert.Equal(exact.Length, fast.Length);
        for (var i = 0; i < exact.Length; i++)
        {
            Assert.Equal(double.IsNaN(exact[i]), double.IsNaN(fast[i]));
            if (!double.IsNaN(exact[i]))
            {
                var bound = tol * Math.Max(1, Math.Abs(exact[i]));
                Assert.True(Math.Abs(exact[i] - fast[i]) <= bound, $"at {i}: {exact[i]} vs {fast[i]}");
            }
        }
    }

    private static void AssertBits(double[] a, double[] b)
    {
        Assert.Equal(a.Length, b.Length);
        for (var i = 0; i < a.Length; i++)
        {
            Assert.Equal(BitConverter.DoubleToInt64Bits(a[i]), BitConverter.DoubleToInt64Bits(b[i]));
        }
    }

    [Fact]
    public void SpanBatchIsTheAllocatingBatchBitForBit()
    {
        var input = Prices(N);
        using var a = new Ema(20);
        using var b = new Ema(20);
        var want = a.Batch(input);
        var got = new double[N];
        b.Batch(input, got);
        AssertBits(want, got);
        // The state carries on identically after either form.
        Assert.Equal(a.Update(101.0), b.Update(101.0));
    }

    [Fact]
    public void ScalarBatchFastAgreesWithBatch()
    {
        var input = Prices(N);
        using (var exact = new Sma(20))
        using (var fast = new Sma(20))
        {
            AssertWithin(exact.Batch(input), fast.BatchFast(input), 1e-12);
            Assert.Equal(exact.Update(101.0), fast.Update(101.0), 1e-12);
        }

        using (var exact = new Rsi(14))
        using (var fast = new Rsi(14))
        {
            var reused = new double[N];
            fast.BatchFast(input, reused);
            AssertWithin(exact.Batch(input), reused, 1e-12);
        }
    }

    [Fact]
    public void BatchFastWithoutAKernelIsTheExactBatch()
    {
        var input = Prices(N);
        using var exact = new Roc(10);
        using var fast = new Roc(10);
        AssertBits(exact.Batch(input), fast.BatchFast(input));
    }

    [Fact]
    public void SpanOverloadsRejectAMismatchedOutput()
    {
        var input = Prices(16);
        using var sma = new Sma(3);
        Assert.Throws<ArgumentException>(() => sma.Batch(input, new double[15]));
        Assert.Throws<ArgumentException>(() => sma.BatchFast(input, new double[17]));
        using var pearson = new PearsonCorrelation(5);
        Assert.Throws<ArgumentException>(() => pearson.BatchFast(input, new double[15], new double[16]));
    }

    [Fact]
    public void EmptyInputsGiveEmptyOutputs()
    {
        using var ema = new Ema(5);
        Assert.Empty(ema.Batch(ReadOnlySpan<double>.Empty));
        Assert.Empty(ema.BatchFast(ReadOnlySpan<double>.Empty));
        using var macd = new MacdIndicator(12, 26, 9);
        Assert.Empty(macd.BatchFast(ReadOnlySpan<double>.Empty));
    }

    [Fact]
    public void CandleBatchFastAgreesWithBatch()
    {
        var close = Prices(N);
        var open = (double[])close.Clone();
        var high = new double[N];
        var low = new double[N];
        var volume = new double[N];
        var stamps = new long[N];
        for (var i = 0; i < N; i++)
        {
            high[i] = close[i] + 1;
            low[i] = close[i] - 1;
            volume[i] = 1000 + i % 7;
            stamps[i] = i;
        }

        using (var exact = new Atr(14))
        using (var fast = new Atr(14))
        {
            AssertWithin(
                exact.Batch(open, high, low, close, volume, stamps),
                fast.BatchFast(open, high, low, close, volume, stamps),
                1e-12);
        }

        using (var exact = new ChaikinOscillator(3, 10))
        using (var fast = new ChaikinOscillator(3, 10))
        {
            AssertWithin(
                exact.Batch(open, high, low, close, volume, stamps),
                fast.BatchFast(open, high, low, close, volume, stamps),
                1e-9);
        }
    }

    [Fact]
    public void PairBatchFastAgreesWithBatch()
    {
        var x = Prices(N);
        var y = new double[N];
        for (var i = 0; i < N; i++)
        {
            y[i] = x[N - 1 - i] * 0.5 + 3;
        }

        using var exact = new PearsonCorrelation(20);
        using var fast = new PearsonCorrelation(20);
        AssertWithin(exact.Batch(x, y), fast.BatchFast(x, y), 1e-9);
    }

    [Fact]
    public void MultiOutputBatchFastAgreesWithBatch()
    {
        var input = Prices(N);
        using (var exact = new MacdIndicator(12, 26, 9))
        using (var fast = new MacdIndicator(12, 26, 9))
        {
            var want = exact.Batch(input);
            var got = fast.BatchFast(input);
            Assert.Equal(want.Length, got.Length);
            for (var i = 0; i < N; i++)
            {
                Assert.Equal(double.IsNaN(want[i].Macd), double.IsNaN(got[i].Macd));
                if (!double.IsNaN(want[i].Macd))
                {
                    Assert.Equal(want[i].Macd, got[i].Macd, 1e-12);
                    Assert.Equal(want[i].Signal, got[i].Signal, 1e-12);
                    Assert.Equal(want[i].Histogram, got[i].Histogram, 1e-12);
                }
            }
        }

        using (var exact = new BollingerBands(20, 2.0))
        using (var fast = new BollingerBands(20, 2.0))
        {
            var want = exact.Batch(input);
            var got = fast.BatchFast(input);
            for (var i = 0; i < N; i++)
            {
                Assert.Equal(double.IsNaN(want[i].Middle), double.IsNaN(got[i].Middle));
                if (!double.IsNaN(want[i].Middle))
                {
                    Assert.Equal(want[i].Upper, got[i].Upper, 1e-10);
                    Assert.Equal(want[i].Middle, got[i].Middle, 1e-10);
                    Assert.Equal(want[i].Lower, got[i].Lower, 1e-10);
                    Assert.Equal(want[i].Stddev, got[i].Stddev, 1e-10);
                }
            }
        }
    }
}
