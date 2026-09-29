// Cross-library benchmark for .NET: Wickra against QuanTAlib, TA-Lib
// (TALib.NETCore), Skender.Stock.Indicators and OoplesFinance.
//
// The setup is QuanTAlib's own (perf/Benchmark.cs in its repository): 500,000
// bars from its geometric-Brownian-motion feed (seed 42), period 220, the
// BenchmarkDotNet ShortRun job on .NET 10. On top of the row that repository
// times for Wickra -- the allocating Batch -- this measures Wickra the way the
// other libraries' fast rows are measured: into a buffer the caller keeps
// (Batch into a Span), the opt-in BatchFast into a Span, and per-tick Update.
// It also times Wickra on Correlation and Skew, which the original leaves out.
//
//   cargo build -p wickra-c --release
//   dotnet run -c Release --project bindings/csharp/cross-library                    # every benchmark
//   dotnet run -c Release --project bindings/csharp/cross-library -- --filter '*Sma*' # one group
//   dotnet run -c Release --project bindings/csharp/cross-library -- verify          # numerical cross-check
//
// `verify` checks that the libraries compute the same thing before their times
// are compared: Wickra against TA-Lib and against naive two-pass references (a
// failure exits non-zero), and QuanTAlib against both (reported only).

using BenchmarkDotNet.Attributes;
using BenchmarkDotNet.Columns;
using BenchmarkDotNet.Configs;
using BenchmarkDotNet.Environments;
using BenchmarkDotNet.Jobs;
using BenchmarkDotNet.Running;
using OoplesFinance.StockIndicators;
using OoplesFinance.StockIndicators.Enums;
using OoplesFinance.StockIndicators.Models;
using Skender.Stock.Indicators;
using TALib;

namespace Wickra.CrossLibrary;

public static class Program
{
    public static int Main(string[] args)
    {
        if (args.Length > 0 && args[0] == "verify")
        {
            return Verify.Run();
        }

        var config = ManualConfig.Create(DefaultConfig.Instance)
            .AddJob(Job.ShortRun.WithRuntime(CoreRuntime.Core10_0).WithId("NET10-JIT"))
            .AddColumn(StatisticColumn.Mean)
            .AddColumn(StatisticColumn.StdDev)
            .HideColumns(Column.Job, Column.Error, Column.RatioSD);

        if (args.Length == 0)
        {
            BenchmarkRunner.Run<IndicatorBenchmarks>(config);
        }
        else
        {
            BenchmarkSwitcher.FromAssembly(typeof(Program).Assembly).Run(args, config);
        }

        return 0;
    }
}

/// <summary>QuanTAlib's benchmark series: 500,000 GBM bars, seed 42.</summary>
public sealed class Series
{
    public const int Bars = 500_000;
    public const int Period = 220;

    public double[] Open = null!, High = null!, Low = null!, Close = null!, Volume = null!;
    public long[] Timestamps = null!;
    public QuanTAlib.TSeries CloseSeries = null!;
    public QuanTAlib.TBarSeries BarSeries = null!;

    public static Series Create()
    {
        var gbm = new QuanTAlib.GBM(startPrice: 100.0, mu: 0.05, sigma: 0.2, seed: 42);
        var bars = gbm.Fetch(Bars, DateTime.UtcNow.Ticks, TimeSpan.FromMinutes(1));
        return new Series
        {
            BarSeries = bars,
            Open = bars.Open.Values.ToArray(),
            High = bars.High.Values.ToArray(),
            Low = bars.Low.Values.ToArray(),
            Close = bars.Close.Values.ToArray(),
            Volume = bars.Volume.Values.ToArray(),
            CloseSeries = bars.Close,
            Timestamps = bars.Close.Times.ToArray(),
        };
    }
}

// OoplesFinance marks its Calculate* extensions obsolete in favour of a builder
// API; they are the calls QuanTAlib's benchmark times, so they are kept.
#pragma warning disable CS0618

[MemoryDiagnoser]
[MarkdownExporter]
[GroupBenchmarksBy(BenchmarkLogicalGroupRule.ByCategory)]
public class IndicatorBenchmarks
{
    private const int Period = Series.Period;

    private Series _s = null!;
    private double[] _close = null!, _open = null!, _high = null!, _low = null!, _volume = null!;
    private long[] _timestamps = null!;
    private List<Quote> _quotes = null!;
    private List<TickerData> _ooples = null!;
    private double[] _talibOut = null!, _quantalibOut = null!, _wickraOut = null!;

    [GlobalSetup]
    public void Setup()
    {
        _s = Series.Create();
        (_close, _open, _high, _low, _volume, _timestamps) = (_s.Close, _s.Open, _s.High, _s.Low, _s.Volume, _s.Timestamps);
        _quotes = new List<Quote>(Series.Bars);
        _ooples = new List<TickerData>(Series.Bars);
        for (int i = 0; i < Series.Bars; i++)
        {
            var date = new DateTime(_s.CloseSeries.Times[i], DateTimeKind.Utc);
            _quotes.Add(new Quote
            {
                Date = date,
                Open = (decimal)_open[i],
                High = (decimal)_high[i],
                Low = (decimal)_low[i],
                Close = (decimal)_close[i],
                Volume = (decimal)_volume[i],
            });
            _ooples.Add(new TickerData
            {
                Date = date,
                Open = _open[i],
                High = _high[i],
                Low = _low[i],
                Close = _close[i],
                Volume = _volume[i],
            });
        }

        _talibOut = new double[Series.Bars];
        _quantalibOut = new double[Series.Bars];
        _wickraOut = new double[Series.Bars];
    }

    // ==================== SMA ====================
    [BenchmarkCategory("SMA"), Benchmark(Description = "QuanTAlib SMA (Span)")]
    public void QuanTAlib_Sma_Span() => QuanTAlib.Sma.Batch(_close.AsSpan(), _quantalibOut.AsSpan(), Period);

    [BenchmarkCategory("SMA"), Benchmark(Description = "QuanTAlib SMA (Batch)")]
    public QuanTAlib.TSeries QuanTAlib_Sma_TSeries() => QuanTAlib.Sma.Calculate(_s.CloseSeries, Period).Results;

    [BenchmarkCategory("SMA"), Benchmark(Description = "QuanTAlib SMA (Streaming)")]
    public void QuanTAlib_Sma_Streaming()
    {
        var sma = new QuanTAlib.Sma(Period);
        for (int i = 0; i < _close.Length; i++)
        {
            _quantalibOut[i] = sma.Update(new QuanTAlib.TValue(_s.CloseSeries.Times[i], _close[i])).Value;
        }
    }

    [BenchmarkCategory("SMA"), Benchmark(Description = "Wickra SMA (Batch, allocating)")]
    public double[] Wickra_Sma()
    {
        using var sma = new Wickra.Sma(Period);
        return sma.Batch(_close);
    }

    [BenchmarkCategory("SMA"), Benchmark(Description = "Wickra SMA (Span)")]
    public void Wickra_Sma_Span()
    {
        using var sma = new Wickra.Sma(Period);
        sma.Batch(_close, _wickraOut);
    }

    [BenchmarkCategory("SMA"), Benchmark(Description = "Wickra SMA (BatchFast Span)")]
    public void Wickra_Sma_Fast()
    {
        using var sma = new Wickra.Sma(Period);
        sma.BatchFast(_close, _wickraOut);
    }

    [BenchmarkCategory("SMA"), Benchmark(Description = "Wickra SMA (Streaming)")]
    public void Wickra_Sma_Streaming()
    {
        using var sma = new Wickra.Sma(Period);
        for (int i = 0; i < _close.Length; i++)
        {
            _wickraOut[i] = sma.Update(_close[i]);
        }
    }

    [BenchmarkCategory("SMA"), Benchmark(Description = "TALib SMA")]
    public Core.RetCode TALib_Sma() => Functions.Sma<double>(_close, 0..^0, _talibOut, out _, Period);

    [BenchmarkCategory("SMA"), Benchmark(Description = "Skender SMA")]
    public object Skender_Sma() => _quotes.GetSma(Period);

    [BenchmarkCategory("SMA"), Benchmark(Description = "Ooples SMA")]
    public object Ooples_Sma() => new StockData(_ooples).CalculateSimpleMovingAverage(Period);

    // ==================== EMA ====================
    [BenchmarkCategory("EMA"), Benchmark(Description = "QuanTAlib EMA (Span)")]
    public void QuanTAlib_Ema_Span() => QuanTAlib.Ema.Batch(_close.AsSpan(), _quantalibOut.AsSpan(), Period);

    [BenchmarkCategory("EMA"), Benchmark(Description = "QuanTAlib EMA (Batch)")]
    public QuanTAlib.TSeries QuanTAlib_Ema_TSeries() => QuanTAlib.Ema.Calculate(_s.CloseSeries, Period).Results;

    [BenchmarkCategory("EMA"), Benchmark(Description = "QuanTAlib EMA (Streaming)")]
    public void QuanTAlib_Ema_Streaming()
    {
        var ema = new QuanTAlib.Ema(Period);
        for (int i = 0; i < _close.Length; i++)
        {
            _quantalibOut[i] = ema.Update(new QuanTAlib.TValue(_s.CloseSeries.Times[i], _close[i])).Value;
        }
    }

    [BenchmarkCategory("EMA"), Benchmark(Description = "Wickra EMA (Batch, allocating)")]
    public double[] Wickra_Ema()
    {
        using var ema = new Wickra.Ema(Period);
        return ema.Batch(_close);
    }

    [BenchmarkCategory("EMA"), Benchmark(Description = "Wickra EMA (Span)")]
    public void Wickra_Ema_Span()
    {
        using var ema = new Wickra.Ema(Period);
        ema.Batch(_close, _wickraOut);
    }

    [BenchmarkCategory("EMA"), Benchmark(Description = "Wickra EMA (BatchFast Span)")]
    public void Wickra_Ema_Fast()
    {
        using var ema = new Wickra.Ema(Period);
        ema.BatchFast(_close, _wickraOut);
    }

    [BenchmarkCategory("EMA"), Benchmark(Description = "Wickra EMA (Streaming)")]
    public void Wickra_Ema_Streaming()
    {
        using var ema = new Wickra.Ema(Period);
        for (int i = 0; i < _close.Length; i++)
        {
            _wickraOut[i] = ema.Update(_close[i]);
        }
    }

    [BenchmarkCategory("EMA"), Benchmark(Description = "TALib EMA")]
    public Core.RetCode TALib_Ema() => Functions.Ema<double>(_close, 0..^0, _talibOut, out _, Period);

    [BenchmarkCategory("EMA"), Benchmark(Description = "Skender EMA")]
    public object Skender_Ema() => _quotes.GetEma(Period);

    [BenchmarkCategory("EMA"), Benchmark(Description = "Ooples EMA")]
    public object Ooples_Ema() => new StockData(_ooples).CalculateExponentialMovingAverage(Period);

    // ==================== WMA ====================
    [BenchmarkCategory("WMA"), Benchmark(Description = "QuanTAlib WMA (Span)")]
    public void QuanTAlib_Wma_Span() => QuanTAlib.Wma.Batch(_close.AsSpan(), _quantalibOut.AsSpan(), Period);

    [BenchmarkCategory("WMA"), Benchmark(Description = "QuanTAlib WMA (Batch)")]
    public QuanTAlib.TSeries QuanTAlib_Wma_TSeries() => QuanTAlib.Wma.Batch(_s.CloseSeries, Period);

    [BenchmarkCategory("WMA"), Benchmark(Description = "QuanTAlib WMA (Streaming)")]
    public void QuanTAlib_Wma_Streaming()
    {
        var wma = new QuanTAlib.Wma(Period);
        for (int i = 0; i < _close.Length; i++)
        {
            _quantalibOut[i] = wma.Update(new QuanTAlib.TValue(_s.CloseSeries.Times[i], _close[i])).Value;
        }
    }

    [BenchmarkCategory("WMA"), Benchmark(Description = "Wickra WMA (Batch, allocating)")]
    public double[] Wickra_Wma()
    {
        using var wma = new Wickra.Wma(Period);
        return wma.Batch(_close);
    }

    [BenchmarkCategory("WMA"), Benchmark(Description = "Wickra WMA (Span)")]
    public void Wickra_Wma_Span()
    {
        using var wma = new Wickra.Wma(Period);
        wma.Batch(_close, _wickraOut);
    }

    [BenchmarkCategory("WMA"), Benchmark(Description = "Wickra WMA (BatchFast Span)")]
    public void Wickra_Wma_Fast()
    {
        using var wma = new Wickra.Wma(Period);
        wma.BatchFast(_close, _wickraOut);
    }

    [BenchmarkCategory("WMA"), Benchmark(Description = "Wickra WMA (Streaming)")]
    public void Wickra_Wma_Streaming()
    {
        using var wma = new Wickra.Wma(Period);
        for (int i = 0; i < _close.Length; i++)
        {
            _wickraOut[i] = wma.Update(_close[i]);
        }
    }

    [BenchmarkCategory("WMA"), Benchmark(Description = "TALib WMA")]
    public Core.RetCode TALib_Wma() => Functions.Wma<double>(_close, 0..^0, _talibOut, out _, Period);

    [BenchmarkCategory("WMA"), Benchmark(Description = "Skender WMA")]
    public object Skender_Wma() => _quotes.GetWma(Period);

    [BenchmarkCategory("WMA"), Benchmark(Description = "Ooples WMA")]
    public object Ooples_Wma() => new StockData(_ooples).CalculateWeightedMovingAverage(Period);

    // ==================== HMA ====================
    [BenchmarkCategory("HMA"), Benchmark(Description = "QuanTAlib HMA (Span)")]
    public void QuanTAlib_Hma_Span() => QuanTAlib.Hma.Batch(_close.AsSpan(), _quantalibOut.AsSpan(), Period);

    [BenchmarkCategory("HMA"), Benchmark(Description = "QuanTAlib HMA (Batch)")]
    public QuanTAlib.TSeries QuanTAlib_Hma_TSeries() => QuanTAlib.Hma.Batch(_s.CloseSeries, Period);

    [BenchmarkCategory("HMA"), Benchmark(Description = "QuanTAlib HMA (Streaming)")]
    public void QuanTAlib_Hma_Streaming()
    {
        var hma = new QuanTAlib.Hma(Period);
        for (int i = 0; i < _close.Length; i++)
        {
            _quantalibOut[i] = hma.Update(new QuanTAlib.TValue(_s.CloseSeries.Times[i], _close[i])).Value;
        }
    }

    [BenchmarkCategory("HMA"), Benchmark(Description = "Wickra HMA (Batch, allocating)")]
    public double[] Wickra_Hma()
    {
        using var hma = new Wickra.Hma(Period);
        return hma.Batch(_close);
    }

    [BenchmarkCategory("HMA"), Benchmark(Description = "Wickra HMA (Span)")]
    public void Wickra_Hma_Span()
    {
        using var hma = new Wickra.Hma(Period);
        hma.Batch(_close, _wickraOut);
    }

    [BenchmarkCategory("HMA"), Benchmark(Description = "Wickra HMA (BatchFast Span)")]
    public void Wickra_Hma_Fast()
    {
        using var hma = new Wickra.Hma(Period);
        hma.BatchFast(_close, _wickraOut);
    }

    [BenchmarkCategory("HMA"), Benchmark(Description = "Wickra HMA (Streaming)")]
    public void Wickra_Hma_Streaming()
    {
        using var hma = new Wickra.Hma(Period);
        for (int i = 0; i < _close.Length; i++)
        {
            _wickraOut[i] = hma.Update(_close[i]);
        }
    }

    [BenchmarkCategory("HMA"), Benchmark(Description = "Skender HMA")]
    public object Skender_Hma() => _quotes.GetHma(Period);

    [BenchmarkCategory("HMA"), Benchmark(Description = "Ooples HMA")]
    public object Ooples_Hma() => new StockData(_ooples).CalculateHullMovingAverage(MovingAvgType.WeightedMovingAverage, Period);

    // ==================== ADOSC ====================
    [BenchmarkCategory("ADOSC"), Benchmark(Description = "QuanTAlib ADOSC (Span)")]
    public void QuanTAlib_Adosc_Span() => QuanTAlib.Adosc.Batch(_high, _low, _close, _volume, _quantalibOut, 3, 10);

    [BenchmarkCategory("ADOSC"), Benchmark(Description = "QuanTAlib ADOSC (Batch)")]
    public QuanTAlib.TSeries QuanTAlib_Adosc_TSeries() => QuanTAlib.Adosc.Batch(_s.BarSeries, 3, 10);

    [BenchmarkCategory("ADOSC"), Benchmark(Description = "QuanTAlib ADOSC (Streaming)")]
    public void QuanTAlib_Adosc_Streaming()
    {
        var adosc = new QuanTAlib.Adosc(3, 10);
        for (int i = 0; i < _s.BarSeries.Count; i++)
        {
            _quantalibOut[i] = adosc.Update(_s.BarSeries[i]).Value;
        }
    }

    [BenchmarkCategory("ADOSC"), Benchmark(Description = "Wickra ADOSC (Batch, allocating)")]
    public double[] Wickra_Adosc()
    {
        using var adosc = new Wickra.ChaikinOscillator(3, 10);
        return adosc.Batch(_open, _high, _low, _close, _volume, _timestamps);
    }

    [BenchmarkCategory("ADOSC"), Benchmark(Description = "Wickra ADOSC (Span)")]
    public void Wickra_Adosc_Span()
    {
        using var adosc = new Wickra.ChaikinOscillator(3, 10);
        adosc.Batch(_open, _high, _low, _close, _volume, _timestamps, _wickraOut);
    }

    [BenchmarkCategory("ADOSC"), Benchmark(Description = "Wickra ADOSC (BatchFast Span)")]
    public void Wickra_Adosc_Fast()
    {
        using var adosc = new Wickra.ChaikinOscillator(3, 10);
        adosc.BatchFast(_open, _high, _low, _close, _volume, _timestamps, _wickraOut);
    }

    [BenchmarkCategory("ADOSC"), Benchmark(Description = "Wickra ADOSC (Streaming)")]
    public void Wickra_Adosc_Streaming()
    {
        using var adosc = new Wickra.ChaikinOscillator(3, 10);
        for (int i = 0; i < _close.Length; i++)
        {
            _wickraOut[i] = adosc.Update(_open[i], _high[i], _low[i], _close[i], _volume[i], _timestamps[i]);
        }
    }

    [BenchmarkCategory("ADOSC"), Benchmark(Description = "TALib ADOSC")]
    public Core.RetCode TALib_Adosc() => Functions.AdOsc(_high, _low, _close, _volume, 0..^0, _talibOut, out _, 3, 10);

    [BenchmarkCategory("ADOSC"), Benchmark(Description = "Skender ADOSC")]
    public object Skender_Adosc() => _quotes.GetChaikinOsc(3, 10);

    [BenchmarkCategory("ADOSC"), Benchmark(Description = "Ooples ADOSC")]
    public object Ooples_Adosc() => new StockData(_ooples).CalculateChaikinOscillator(MovingAvgType.ExponentialMovingAverage, 3, 10);

    // ==================== CORRELATION ====================
    [BenchmarkCategory("CORRELATION"), Benchmark(Description = "QuanTAlib Correlation (Span)")]
    public void QuanTAlib_Correlation_Span() => QuanTAlib.Correl.Batch(_close.AsSpan(), _open.AsSpan(), _quantalibOut.AsSpan(), Period);

    [BenchmarkCategory("CORRELATION"), Benchmark(Description = "QuanTAlib Correlation (Streaming)")]
    public void QuanTAlib_Correlation_Streaming()
    {
        var corr = new QuanTAlib.Correl(Period);
        for (int i = 0; i < _close.Length; i++)
        {
            _quantalibOut[i] = corr.Update(_close[i], _open[i]).Value;
        }
    }

    [BenchmarkCategory("CORRELATION"), Benchmark(Description = "Wickra Correlation (Batch, allocating)")]
    public double[] Wickra_Correlation()
    {
        using var corr = new Wickra.PearsonCorrelation(Period);
        return corr.Batch(_close, _open);
    }

    [BenchmarkCategory("CORRELATION"), Benchmark(Description = "Wickra Correlation (Span)")]
    public void Wickra_Correlation_Span()
    {
        using var corr = new Wickra.PearsonCorrelation(Period);
        corr.Batch(_close, _open, _wickraOut);
    }

    [BenchmarkCategory("CORRELATION"), Benchmark(Description = "Wickra Correlation (BatchFast Span)")]
    public void Wickra_Correlation_Fast()
    {
        using var corr = new Wickra.PearsonCorrelation(Period);
        corr.BatchFast(_close, _open, _wickraOut);
    }

    [BenchmarkCategory("CORRELATION"), Benchmark(Description = "Wickra Correlation (Streaming)")]
    public void Wickra_Correlation_Streaming()
    {
        using var corr = new Wickra.PearsonCorrelation(Period);
        for (int i = 0; i < _close.Length; i++)
        {
            _wickraOut[i] = corr.Update(_close[i], _open[i]);
        }
    }

    [BenchmarkCategory("CORRELATION"), Benchmark(Description = "TALib Correlation")]
    public Core.RetCode TALib_Correlation() => Functions.Correl<double>(_close, _open, 0..^0, _talibOut, out _, Period);

    [BenchmarkCategory("CORRELATION"), Benchmark(Description = "Skender Correlation")]
    public object Skender_Correlation() => _quotes.GetCorrelation(_quotes, Period);

    // ==================== SKEW ====================
    // Wickra's Skewness is the population skewness (divisor n); QuanTAlib's
    // default is the sample skewness, so it is timed both ways.
    [BenchmarkCategory("SKEW"), Benchmark(Description = "QuanTAlib Skew (Span)")]
    public void QuanTAlib_Skew_Span() => QuanTAlib.Skew.Batch(_close.AsSpan(), _quantalibOut.AsSpan(), Period);

    [BenchmarkCategory("SKEW"), Benchmark(Description = "QuanTAlib Skew (Span, population)")]
    public void QuanTAlib_Skew_Span_Population() => QuanTAlib.Skew.Batch(_close.AsSpan(), _quantalibOut.AsSpan(), Period, isPopulation: true);

    [BenchmarkCategory("SKEW"), Benchmark(Description = "QuanTAlib Skew (Streaming)")]
    public void QuanTAlib_Skew_Streaming()
    {
        var skew = new QuanTAlib.Skew(Period);
        for (int i = 0; i < _close.Length; i++)
        {
            _quantalibOut[i] = skew.Update(new QuanTAlib.TValue(_s.CloseSeries.Times[i], _close[i])).Value;
        }
    }

    [BenchmarkCategory("SKEW"), Benchmark(Description = "Wickra Skew (Batch, allocating)")]
    public double[] Wickra_Skew()
    {
        using var skew = new Wickra.Skewness(Period);
        return skew.Batch(_close);
    }

    [BenchmarkCategory("SKEW"), Benchmark(Description = "Wickra Skew (Span)")]
    public void Wickra_Skew_Span()
    {
        using var skew = new Wickra.Skewness(Period);
        skew.Batch(_close, _wickraOut);
    }

    [BenchmarkCategory("SKEW"), Benchmark(Description = "Wickra Skew (BatchFast Span)")]
    public void Wickra_Skew_Fast()
    {
        using var skew = new Wickra.Skewness(Period);
        skew.BatchFast(_close, _wickraOut);
    }

    [BenchmarkCategory("SKEW"), Benchmark(Description = "Wickra Skew (Streaming)")]
    public void Wickra_Skew_Streaming()
    {
        using var skew = new Wickra.Skewness(Period);
        for (int i = 0; i < _close.Length; i++)
        {
            _wickraOut[i] = skew.Update(_close[i]);
        }
    }
}
