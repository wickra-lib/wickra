using System;
using System.Runtime.CompilerServices;
using Xunit;

namespace Wickra.Tests;

/// <summary>
/// Multi-output batches write their records in place: the public record is laid
/// out exactly like the native struct, so the native side fills the result array
/// directly. These pin that layout and the caller-buffer overload.
/// </summary>
public class MultiBatchSpanTests
{
    private const int N = 777;

    private static double[] Prices(int n)
    {
        var p = new double[n];
        for (var i = 0; i < n; i++)
        {
            p[i] = 100 + Math.Sin(i * 0.021) * 4 + Math.Cos(i * 0.29);
        }

        return p;
    }

    private static long Bits(double x) => BitConverter.DoubleToInt64Bits(x);

    [Fact]
    public void RecordIsLaidOutLikeTheNativeStruct()
    {
        // Three doubles, in declaration order, nothing else.
        Assert.Equal(3 * sizeof(double), Unsafe.SizeOf<MacdOutput>());
        var row = new MacdOutput(1.5, 2.5, 3.5);
        var fields = System.Runtime.InteropServices.MemoryMarshal.Cast<MacdOutput, double>(new[] { row });
        Assert.Equal(new[] { 1.5, 2.5, 3.5 }, fields.ToArray());
    }

    [Fact]
    public void BatchMatchesStreamingFieldForField()
    {
        var input = Prices(N);
        using var streaming = new MacdIndicator(12, 26, 9);
        using var batching = new MacdIndicator(12, 26, 9);
        var rows = batching.Batch(input);
        Assert.Equal(N, rows.Length);
        for (var i = 0; i < N; i++)
        {
            var want = streaming.Update(input[i]);
            if (want is null)
            {
                Assert.True(double.IsNaN(rows[i].Macd) && double.IsNaN(rows[i].Signal) && double.IsNaN(rows[i].Histogram));
                continue;
            }

            Assert.Equal(Bits(want.Value.Macd), Bits(rows[i].Macd));
            Assert.Equal(Bits(want.Value.Signal), Bits(rows[i].Signal));
            Assert.Equal(Bits(want.Value.Histogram), Bits(rows[i].Histogram));
        }
    }

    [Fact]
    public void SpanBatchIsTheAllocatingBatch()
    {
        var input = Prices(N);
        using var a = new BollingerBands(20, 2.0);
        using var b = new BollingerBands(20, 2.0);
        var want = a.Batch(input);
        var got = new BollingerOutput[N];
        Array.Fill(got, new BollingerOutput(-1, -1, -1, -1));
        b.Batch(input, got);
        Assert.Equal(want.Length, got.Length);
        for (var i = 0; i < N; i++)
        {
            Assert.Equal(Bits(want[i].Upper), Bits(got[i].Upper));
            Assert.Equal(Bits(want[i].Middle), Bits(got[i].Middle));
            Assert.Equal(Bits(want[i].Lower), Bits(got[i].Lower));
            Assert.Equal(Bits(want[i].Stddev), Bits(got[i].Stddev));
        }
    }

    [Fact]
    public void SpanBatchRejectsAMismatchedOutput()
    {
        using var macd = new MacdIndicator(12, 26, 9);
        Assert.Throws<ArgumentException>(() => macd.Batch(Prices(10), new MacdOutput[9]));
    }
}
