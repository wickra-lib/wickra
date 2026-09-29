using TALib;

namespace Wickra.CrossLibrary;

/// <summary>
/// Whether the libraries compute the same thing, before their times are
/// compared. Every comparison starts at bar 2,000, past every warmup and past
/// the decay of the different EMA seeds. Wickra is held to TA-Lib, to its own
/// exact batch (the fast one) and to naive two-pass references; a failure makes
/// the run exit 1. QuanTAlib is compared to the same and only reported.
/// </summary>
public static class Verify
{
    private const int From = 2_000;
    private static bool _failed;

    public static int Run()
    {
        var s = Series.Create();
        int n = Series.Bars, p = Series.Period;

        Console.WriteLine($"{"check",-58}{"max |diff|",14}{"NaN mismatch",14}  verdict");

        // SMA
        {
            var w = Exact(() => new Wickra.Sma(p), (i, o) => i.Batch(s.Close, o));
            var f = Exact(() => new Wickra.Sma(p), (i, o) => i.BatchFast(s.Close, o));
            var t = TaLib(o => (Functions.Sma<double>(s.Close, 0..^0, o, out var r, p), r), out var off);
            var q = new double[n];
            QuanTAlib.Sma.Batch(s.Close, q, p);
            Check("SMA   Wickra exact vs TA-Lib", w, t, off, 1e-7);
            Check("SMA   Wickra fast vs Wickra exact", f, w, 0, 1e-8);
            Reference("SMA   Wickra exact vs two-pass mean", w, end => Mean(s.Close, end, p), 1e-9);
            Info("SMA   QuanTAlib Span vs TA-Lib", q, t, off);
        }
        // EMA
        {
            var w = Exact(() => new Wickra.Ema(p), (i, o) => i.Batch(s.Close, o));
            var f = Exact(() => new Wickra.Ema(p), (i, o) => i.BatchFast(s.Close, o));
            var t = TaLib(o => (Functions.Ema<double>(s.Close, 0..^0, o, out var r, p), r), out var off);
            var q = new double[n];
            QuanTAlib.Ema.Batch(s.Close, q, p);
            Check("EMA   Wickra exact vs TA-Lib", w, t, off, 1e-7);
            Check("EMA   Wickra fast vs Wickra exact", f, w, 0, 1e-8);
            Info("EMA   QuanTAlib Span vs TA-Lib", q, t, off);
        }
        // WMA
        {
            var w = Exact(() => new Wickra.Wma(p), (i, o) => i.Batch(s.Close, o));
            var f = Exact(() => new Wickra.Wma(p), (i, o) => i.BatchFast(s.Close, o));
            var t = TaLib(o => (Functions.Wma<double>(s.Close, 0..^0, o, out var r, p), r), out var off);
            var q = new double[n];
            QuanTAlib.Wma.Batch(s.Close, q, p);
            Check("WMA   Wickra exact vs TA-Lib", w, t, off, 1e-7);
            Check("WMA   Wickra fast vs Wickra exact", f, w, 0, 1e-8);
            Reference("WMA   Wickra exact vs two-pass weighted mean", w, end => Wma(k => s.Close[k], end, p), 1e-9);
            Info("WMA   QuanTAlib Span vs TA-Lib", q, t, off);
        }
        // HMA: WMA(2 * WMA(n/2) - WMA(n), sqrt n). Wickra rounds sqrt n to the
        // nearest integer (15 for 220); QuanTAlib, like TradingView and
        // pandas-ta, truncates it (14), so the two are different indicators.
        {
            var w = Exact(() => new Wickra.Hma(p), (i, o) => i.Batch(s.Close, o));
            var f = Exact(() => new Wickra.Hma(p), (i, o) => i.BatchFast(s.Close, o));
            var q = new double[n];
            QuanTAlib.Hma.Batch(s.Close, q, p);
            Check("HMA   Wickra fast vs Wickra exact", f, w, 0, 1e-8);
            Reference("HMA   Wickra exact vs two-pass, sqrt rounded (15)", w, end => Hma(s.Close, end, p, 15), 1e-9);
            InfoReference("HMA   QuanTAlib Span vs two-pass, sqrt truncated (14)", q, end => Hma(s.Close, end, p, 14));
        }
        // ADOSC (Chaikin oscillator 3, 10)
        {
            var w = Exact(() => new Wickra.ChaikinOscillator(3, 10), (i, o) => i.Batch(s.Open, s.High, s.Low, s.Close, s.Volume, s.Timestamps, o));
            var f = Exact(() => new Wickra.ChaikinOscillator(3, 10), (i, o) => i.BatchFast(s.Open, s.High, s.Low, s.Close, s.Volume, s.Timestamps, o));
            var t = TaLib(o => (Functions.AdOsc(s.High, s.Low, s.Close, s.Volume, 0..^0, o, out var r, 3, 10), r), out var off);
            var q = new double[n];
            QuanTAlib.Adosc.Batch(s.High, s.Low, s.Close, s.Volume, q, 3, 10);
            Check("ADOSC Wickra exact vs TA-Lib", w, t, off, 1e-7);
            Check("ADOSC Wickra fast vs Wickra exact", f, w, 0, 1e-8);
            Info("ADOSC QuanTAlib Span vs TA-Lib", q, t, off);
        }
        // Correlation of close and open.
        {
            var w = Exact(() => new Wickra.PearsonCorrelation(p), (i, o) => i.Batch(s.Close, s.Open, o));
            var f = Exact(() => new Wickra.PearsonCorrelation(p), (i, o) => i.BatchFast(s.Close, s.Open, o));
            var t = TaLib(o => (Functions.Correl<double>(s.Close, s.Open, 0..^0, o, out var r, p), r), out var off);
            var q = new double[n];
            QuanTAlib.Correl.Batch(s.Close, s.Open, q, p);
            var qs = new double[n];
            var streaming = new QuanTAlib.Correl(p);
            for (int i = 0; i < n; i++)
            {
                qs[i] = streaming.Update(s.Close[i], s.Open[i]).Value;
            }

            Check("CORR  Wickra exact vs TA-Lib", w, t, off, 1e-7);
            Check("CORR  Wickra fast vs Wickra exact", f, w, 0, 1e-8);
            Reference("CORR  Wickra exact vs two-pass Pearson", w, end => Pearson(s.Close, s.Open, end, p), 1e-10);
            Info("CORR  QuanTAlib Span vs TA-Lib", q, t, off);
            InfoReference("CORR  QuanTAlib Streaming vs two-pass Pearson", qs, end => Pearson(s.Close, s.Open, end, p));
        }
        // Skewness. Wickra's is the population skewness (divisor n).
        {
            var w = Exact(() => new Wickra.Skewness(p), (i, o) => i.Batch(s.Close, o));
            var f = Exact(() => new Wickra.Skewness(p), (i, o) => i.BatchFast(s.Close, o));
            var q = new double[n];
            QuanTAlib.Skew.Batch(s.Close, q, p, isPopulation: true);
            Check("SKEW  Wickra fast vs Wickra exact", f, w, 0, 1e-8);
            Reference("SKEW  Wickra exact vs two-pass population skewness", w, end => SkewPopulation(s.Close, end, p), 1e-9);
            InfoReference("SKEW  QuanTAlib Span (population) vs two-pass", q, end => SkewPopulation(s.Close, end, p));
        }

        Console.WriteLine(_failed ? "\nFAILED: Wickra disagrees with a reference." : "\nWickra agrees with every reference.");
        return _failed ? 1 : 0;
    }

    private static double[] Exact<T>(Func<T> make, Action<T, double[]> batch)
        where T : IDisposable
    {
        var out_ = new double[Series.Bars];
        using var ind = make();
        batch(ind, out_);
        return out_;
    }

    private static double[] TaLib(Func<double[], (Core.RetCode, Range)> call, out int offset)
    {
        var out_ = new double[Series.Bars];
        var (_, range) = call(out_);
        offset = range.Start.Value;
        return out_;
    }

    /// <summary>Largest |a[i] - b[i - offset]| from bar 2,000, and how many bars are NaN on one side only.</summary>
    private static (double Worst, int NanMismatch) Compare(double[] a, double[] b, int offset, double tolerance, out bool within)
    {
        double worst = 0;
        int nanMismatch = 0;
        within = true;
        for (int i = From; i < a.Length; i++)
        {
            double x = a[i], y = b[i - offset];
            if (double.IsNaN(x) != double.IsNaN(y))
            {
                nanMismatch++;
                continue;
            }

            if (double.IsNaN(x))
            {
                continue;
            }

            double diff = Math.Abs(x - y);
            worst = Math.Max(worst, diff);
            within &= diff <= tolerance * Math.Max(1.0, Math.Abs(y));
        }

        return (worst, nanMismatch);
    }

    private static void Check(string what, double[] a, double[] b, int offset, double tolerance)
    {
        var (worst, nan) = Compare(a, b, offset, tolerance, out bool within);
        bool ok = within && nan == 0;
        _failed |= !ok;
        Console.WriteLine($"{what,-58}{worst,14:E2}{nan,14}  {(ok ? "ok" : "FAIL")}");
    }

    private static void Info(string what, double[] a, double[] b, int offset)
    {
        var (worst, nan) = Compare(a, b, offset, double.PositiveInfinity, out _);
        Console.WriteLine($"{what,-58}{worst,14:E2}{nan,14}  (reported only)");
    }

    /// <summary>Against a reference computed on a sample of windows, every 997th bar.</summary>
    private static (double Worst, int NanMismatch) Sampled(double[] a, Func<int, double> reference)
    {
        double worst = 0;
        int nan = 0;
        for (int end = 3_000; end < a.Length; end += 997)
        {
            if (double.IsNaN(a[end]))
            {
                nan++;
                continue;
            }

            worst = Math.Max(worst, Math.Abs(a[end] - reference(end)));
        }

        return (worst, nan);
    }

    private static void Reference(string what, double[] a, Func<int, double> reference, double tolerance)
    {
        var (worst, nan) = Sampled(a, reference);
        bool ok = worst <= tolerance && nan == 0;
        _failed |= !ok;
        Console.WriteLine($"{what,-58}{worst,14:E2}{nan,14}  {(ok ? "ok" : "FAIL")}");
    }

    private static void InfoReference(string what, double[] a, Func<int, double> reference)
    {
        var (worst, nan) = Sampled(a, reference);
        Console.WriteLine($"{what,-58}{worst,14:E2}{nan,14}  (reported only)");
    }

    private static double Mean(double[] x, int end, int n)
    {
        double sum = 0;
        for (int k = end - n + 1; k <= end; k++)
        {
            sum += x[k];
        }

        return sum / n;
    }

    private static double Wma(Func<int, double> x, int end, int n)
    {
        double num = 0, den = 0;
        for (int k = 0; k < n; k++)
        {
            double weight = n - k;
            num += weight * x(end - k);
            den += weight;
        }

        return num / den;
    }

    private static double Hma(double[] x, int end, int n, int smooth)
    {
        double Raw(int i) => 2 * Wma(k => x[k], i, n / 2) - Wma(k => x[k], i, n);
        return Wma(Raw, end, smooth);
    }

    private static double Pearson(double[] a, double[] b, int end, int n)
    {
        double ma = 0, mb = 0;
        for (int k = end - n + 1; k <= end; k++)
        {
            ma += a[k];
            mb += b[k];
        }

        ma /= n;
        mb /= n;
        double sab = 0, saa = 0, sbb = 0;
        for (int k = end - n + 1; k <= end; k++)
        {
            double da = a[k] - ma, db = b[k] - mb;
            sab += da * db;
            saa += da * da;
            sbb += db * db;
        }

        return sab / Math.Sqrt(saa * sbb);
    }

    private static double SkewPopulation(double[] x, int end, int n)
    {
        double mean = Mean(x, end, n), m2 = 0, m3 = 0;
        for (int k = end - n + 1; k <= end; k++)
        {
            double d = x[k] - mean;
            m2 += d * d;
            m3 += d * d * d;
        }

        m2 /= n;
        m3 /= n;
        return m3 / (m2 * Math.Sqrt(m2));
    }
}
