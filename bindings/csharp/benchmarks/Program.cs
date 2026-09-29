// Throughput benchmark for the Wickra C# binding.
//
// Measures how many indicator updates per second the binding sustains, per-tick
// (streaming Update), bulk (Batch) and bulk through the opt-in SIMD kernels
// (BatchFast), each allocating its result and into a reused Span, over a
// synthetic OHLCV series. It is the C# counterpart of the Node throughput.js
// and the Rust criterion benches: it benchmarks Wickra's own O(1) streaming
// engine across the managed<->C-ABI boundary (there is no comparable streaming
// TA library on NuGet to compare against), so the headline number is raw
// per-binding throughput / FFI overhead, not a cross-library ratio.
//
// Three indicators are timed, chosen by FFI call-signature archetype rather
// than algorithm: SMA (1-in -> 1-out), ATR (multi-in -> 1-out), and MACD
// (1-in -> multi-out, whose batches write MacdOutput records).
//
//   cargo build -p wickra-c --release
//   dotnet run -c Release --project bindings/csharp/benchmarks            # 200k bars
//   dotnet run -c Release --project bindings/csharp/benchmarks -- --bars 1000000

using System.Diagnostics;
using System.Globalization;
using Wickra;

// Deterministic, locale-independent number formatting for the report.
CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;

int bars = 200_000;
for (int i = 0; i < args.Length - 1; i++)
{
    if (args[i] == "--bars" && int.TryParse(args[i + 1], out var n) && n >= 1000)
    {
        bars = n;
    }
}

// Deterministic synthetic OHLCV (no RNG, so runs are comparable).
var open = new double[bars];
var high = new double[bars];
var low = new double[bars];
var close = new double[bars];
var volume = new double[bars];
var timestamp = new long[bars];
for (int i = 0; i < bars; i++)
{
    double mid = 100 + Math.Sin(i * 0.001) * 20 + i * 1e-4;
    double c = mid + Math.Sin(i * 0.05) * 2;
    close[i] = c;
    open[i] = mid;
    high[i] = Math.Max(c, mid) + 1.5;
    low[i] = Math.Min(c, mid) - 1.5;
    volume[i] = 1000 + (i % 97) * 13;
    timestamp[i] = i;
}
// The reused outputs of the Span forms.
var output = new double[bars];
var macdOutput = new MacdOutput[bars];

double Mups(double ns) => bars / (ns / 1e9) / 1e6;

// Median elapsed-ns over a few repetitions, after one warmup pass.
double TimeNs(Action fn, int reps = 3)
{
    fn(); // warmup (JIT + cache)
    var samples = new double[reps];
    for (int r = 0; r < reps; r++)
    {
        long t0 = Stopwatch.GetTimestamp();
        fn();
        samples[r] = (Stopwatch.GetTimestamp() - t0) * (1e9 / Stopwatch.Frequency);
    }
    Array.Sort(samples);
    return samples[reps / 2];
}

// SMA (scalar 1-in/1-out), ATR (multi-in/1-out), MACD (1-in/multi-out).
var indicators = new (string Name, Action Stream, Action Batch, Action Fast, Action? BatchSpan, Action? FastSpan)[]
{
    ("SMA(20)",
        () => { using var ind = new Sma(20); for (int i = 0; i < bars; i++) ind.Update(close[i]); },
        () => { using var ind = new Sma(20); ind.Batch(close); },
        () => { using var ind = new Sma(20); ind.BatchFast(close); },
        () => { using var ind = new Sma(20); ind.Batch(close, output); },
        () => { using var ind = new Sma(20); ind.BatchFast(close, output); }),
    ("ATR(14)",
        () => { using var ind = new Atr(14); for (int i = 0; i < bars; i++) ind.Update(open[i], high[i], low[i], close[i], volume[i], timestamp[i]); },
        () => { using var ind = new Atr(14); ind.Batch(open, high, low, close, volume, timestamp); },
        () => { using var ind = new Atr(14); ind.BatchFast(open, high, low, close, volume, timestamp); },
        () => { using var ind = new Atr(14); ind.Batch(open, high, low, close, volume, timestamp, output); },
        () => { using var ind = new Atr(14); ind.BatchFast(open, high, low, close, volume, timestamp, output); }),
    ("MACD(12,26,9)",
        () => { using var ind = new MacdIndicator(12, 26, 9); for (int i = 0; i < bars; i++) ind.Update(close[i]); },
        () => { using var ind = new MacdIndicator(12, 26, 9); ind.Batch(close); },
        () => { using var ind = new MacdIndicator(12, 26, 9); ind.BatchFast(close); },
        () => { using var ind = new MacdIndicator(12, 26, 9); ind.Batch(close, macdOutput); },
        () => { using var ind = new MacdIndicator(12, 26, 9); ind.BatchFast(close, macdOutput); }),
};

string Cell(Action? run) => run is null ? "-" : Mups(TimeNs(run)).ToString("F1");

Console.WriteLine($"Wickra C# throughput - {bars:N0} bars (median of 3 runs)\n");
string header = $"{"Indicator",-18}{"streaming",12}{"batch",12}{"fast",12}{"batch span",12}{"fast span",12}";
Console.WriteLine(header);
Console.WriteLine(new string('-', header.Length));

foreach (var (name, stream, batch, fast, batchSpan, fastSpan) in indicators)
{
    Console.WriteLine($"{name,-18}{Cell(stream),12}{Cell(batch),12}{Cell(fast),12}{Cell(batchSpan),12}{Cell(fastSpan),12}");
}

Console.WriteLine(
    "\nMupd/s (million indicator updates per second). Streaming is the per-tick\n" +
    "Update path crossing the managed<->C-ABI boundary once per value; batch is\n" +
    "the bulk path (one boundary crossing) and fast the opt-in BatchFast (SIMD\n" +
    "kernels within a few units in the last place of Batch), each allocating its\n" +
    "result or, in the span columns, writing into a reused buffer. Higher is\n" +
    "better. Numbers are machine-dependent - use them for relative comparison,\n" +
    "not as a speed claim.");
