//! Generate the language-neutral golden fixtures consumed by the per-binding
//! parity runners (`bindings/{csharp,go,java,r}`).
//!
//! Run from the repo root: `cargo run -p wickra-examples --bin gen_golden`.
//! It writes a deterministic OHLCV input series plus the reference outputs of a
//! curated set of indicators spanning the FFI archetypes (scalar, candle,
//! scalar multi-output, candle multi-output, pairwise), computed by the Rust
//! core. Each binding runner replays the same input through its own FFI and
//! checks it matches these values — catching wiring bugs (swapped params,
//! wrong multi-output index) that the math-only core tests cannot see.
//!
//! `nan` marks a warmup slot where the indicator returned `None`.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use wickra::{
    AdOscillator, Adx, Atr, AverageDrawdown, AwesomeOscillatorHistogram, Beta, Candle, Ema,
    Indicator, IntradayIntensity, MacdIndicator, Rsi, Sma, Tick,
};
use wickra_data::aggregator::{TickAggregator, Timeframe};
use wickra_data::resample::Resampler;

const N: usize = 80;

/// Deterministic OHLCV bar `i`: a varied, non-degenerate path so every
/// indicator gets real movement (no constant returns, no zero ranges).
fn bar(i: usize) -> (f64, f64, f64, f64, f64) {
    let t = i as f64;
    let close = 100.0 + 10.0 * (t * 0.3).sin() + 0.5 * t;
    let open = close - (t * 0.5).cos();
    let span = 1.0 + 0.5 * (t * 0.7).sin().abs();
    let high = close.max(open) + span;
    let low = close.min(open) - span;
    let volume = 1000.0 + 100.0 * (t * 0.2).sin().abs();
    (open, high, low, close, volume)
}

fn cell(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{x}"),
        None => "nan".to_owned(),
    }
}

fn write_csv(dir: &Path, name: &str, header: &str, rows: &[String]) {
    let mut out = String::new();
    let _ = writeln!(out, "{header}");
    for r in rows {
        let _ = writeln!(out, "{r}");
    }
    let path = dir.join(format!("{name}.csv"));
    fs::write(&path, out).expect("write fixture");
    println!("wrote {}", path.display());
}

#[allow(clippy::too_many_lines)]
fn main() {
    let dir = Path::new("testdata/golden");
    fs::create_dir_all(dir).expect("create testdata/golden");

    let candles: Vec<Candle> = (0..N)
        .map(|i| {
            let (open, high, low, close, volume) = bar(i);
            Candle::new(
                open,
                high,
                low,
                close,
                volume,
                i64::try_from(i).expect("golden row index fits i64"),
            )
            .expect("valid candle")
        })
        .collect();
    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();

    // input.csv — the single shared series every runner reads.
    let mut input = vec![String::from("open,high,low,close,volume")];
    for c in &candles {
        input.push(format!(
            "{},{},{},{},{}",
            c.open, c.high, c.low, c.close, c.volume
        ));
    }
    let input_header = input.remove(0);
    write_csv(dir, "input", &input_header, &input);

    // scalar (close-driven), single output. Each is a distinct type, so write
    // them out separately rather than from a heterogeneous collection.
    {
        let mut sma = Sma::new(14).unwrap();
        let rows: Vec<String> = closes.iter().map(|&c| cell(sma.update(c))).collect();
        write_csv(dir, "sma", "sma", &rows);
    }
    {
        let mut ema = Ema::new(14).unwrap();
        let rows: Vec<String> = closes.iter().map(|&c| cell(ema.update(c))).collect();
        write_csv(dir, "ema", "ema", &rows);
    }
    {
        let mut rsi = Rsi::new(14).unwrap();
        let rows: Vec<String> = closes.iter().map(|&c| cell(rsi.update(c))).collect();
        write_csv(dir, "rsi", "rsi", &rows);
    }

    // candle, single output.
    {
        let mut atr = Atr::new(14).unwrap();
        let rows: Vec<String> = candles.iter().map(|&c| cell(atr.update(c))).collect();
        write_csv(dir, "atr", "atr", &rows);
    }

    // scalar multi-output: MACD(12,26,9).
    {
        let mut macd = MacdIndicator::new(12, 26, 9).unwrap();
        let rows: Vec<String> = closes
            .iter()
            .map(|&c| match macd.update(c) {
                Some(o) => format!("{},{},{}", o.macd, o.signal, o.histogram),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "macd", "macd,signal,histogram", &rows);
    }

    // candle multi-output: ADX(14).
    {
        let mut adx = Adx::new(14).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match adx.update(c) {
                Some(o) => format!("{},{},{}", o.plus_di, o.minus_di, o.adx),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "adx", "plus_di,minus_di,adx", &rows);
    }

    // pairwise: Beta(20) over (close, open).
    {
        let mut beta = Beta::new(20).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(beta.update((c.close, c.open))))
            .collect();
        write_csv(dir, "beta", "beta", &rows);
    }

    // candle, single output: the de-duplicated indicators, pinned across every
    // binding so their corrected definitions stay identical to the Rust core.
    {
        let mut ad = AdOscillator::new(); // Williams A/D Oscillator
        let rows: Vec<String> = candles.iter().map(|&c| cell(ad.update(c))).collect();
        write_csv(dir, "ad_oscillator", "ad_oscillator", &rows);
    }
    {
        let mut ii = IntradayIntensity::new(); // per-bar Bostian intensity
        let rows: Vec<String> = candles.iter().map(|&c| cell(ii.update(c))).collect();
        write_csv(dir, "intraday_intensity", "intraday_intensity", &rows);
    }
    {
        let mut aoh = AwesomeOscillatorHistogram::new(5, 34, 1).unwrap(); // AO momentum
        let rows: Vec<String> = candles.iter().map(|&c| cell(aoh.update(c))).collect();
        write_csv(
            dir,
            "awesome_oscillator_histogram",
            "awesome_oscillator_histogram",
            &rows,
        );
    }
    // scalar (close-driven equity curve), single output.
    {
        let mut avg_dd = AverageDrawdown::new(20).unwrap(); // mean of distinct episodes
        let rows: Vec<String> = closes.iter().map(|&c| cell(avg_dd.update(c))).collect();
        write_csv(dir, "average_drawdown", "average_drawdown", &rows);
    }

    emit_scalar(dir, &candles, &closes);
    emit_multi(dir, &candles, &closes);
    emit_skips(dir, &candles, &closes);
    emit_missed(dir, &candles, &closes);
    emit_exotic(dir, &candles);
    emit_special(dir, &candles);
    emit_profiles(dir, &candles);
    emit_bars(dir, &candles);
    emit_data_layer(dir);
    emit_resampler(dir, &candles);
    emit_resampler_gap(dir, &candles);
    emit_candle_reader_csv(dir, &candles);
    println!("golden fixtures written to {}", dir.display());
}

/// Data layer: the CSV candle reader. Writes a source CSV in the reader's required
/// `timestamp,open,high,low,close,volume` layout, plus the reference candles the
/// reader parses out of it. Every binding parses the same bytes and checks the
/// candles match — pinning column mapping and numeric round-tripping across the
/// FFI.
fn emit_candle_reader_csv(dir: &Path, candles: &[Candle]) {
    let mut src = Vec::with_capacity(candles.len());
    for c in candles {
        src.push(format!(
            "{},{},{},{},{},{}",
            c.timestamp, c.open, c.high, c.low, c.close, c.volume
        ));
    }
    write_csv(
        dir,
        "data_csv",
        "timestamp,open,high,low,close,volume",
        &src,
    );

    // Reference parse: feed the same bytes back through the reader so the fixture
    // is exactly what wickra-data's parser yields, not just the input echoed.
    let mut bytes = String::from("timestamp,open,high,low,close,volume\n");
    for row in &src {
        bytes.push_str(row);
        bytes.push('\n');
    }
    let mut reader =
        wickra_data::csv::CandleReader::from_reader(bytes.as_bytes()).expect("valid candle reader");
    let parsed = reader.read_all().expect("valid csv parse");
    let rows: Vec<String> = parsed
        .iter()
        .map(|c| {
            format!(
                "{},{},{},{},{},{}",
                c.open, c.high, c.low, c.close, c.volume, c.timestamp
            )
        })
        .collect();
    write_csv(
        dir,
        "data_csv_candles",
        "open,high,low,close,volume,timestamp",
        &rows,
    );
}

/// Data layer: the resampler. Resamples the shared input candles (timestamp =
/// row index) into 5-unit buckets; the final partial bucket comes out of flush.
fn emit_resampler(dir: &Path, candles: &[Candle]) {
    let mut resampler = Resampler::new(Timeframe::new(5).unwrap());
    let mut rows = Vec::new();
    for &candle in candles {
        for out in resampler.push(candle).expect("valid resample push") {
            rows.push(format!(
                "{},{},{},{},{},{}",
                out.open, out.high, out.low, out.close, out.volume, out.timestamp
            ));
        }
    }
    if let Some(out) = resampler.flush().expect("valid resample flush") {
        rows.push(format!(
            "{},{},{},{},{},{}",
            out.open, out.high, out.low, out.close, out.volume, out.timestamp
        ));
    }
    write_csv(
        dir,
        "data_resampled",
        "open,high,low,close,volume,timestamp",
        &rows,
    );
}

/// The row range the gap-fill resampler fixture drops, opening a five-bucket
/// hole in otherwise contiguous input. Every binding replays the same subset.
const RESAMPLE_GAP: std::ops::RangeInclusive<usize> = 20..=44;

/// Data layer: the resampler with gap filling on. The input candles carry
/// `timestamp = row index` and no gaps of their own, so the fixture drops
/// `RESAMPLE_GAP` to create one; the resampler then emits a flat placeholder
/// candle for each skipped bucket instead of jumping the timestamp.
fn emit_resampler_gap(dir: &Path, candles: &[Candle]) {
    let mut resampler = Resampler::new(Timeframe::new(5).unwrap()).with_gap_fill(true);
    let mut rows = Vec::new();
    for (i, &candle) in candles.iter().enumerate() {
        if RESAMPLE_GAP.contains(&i) {
            continue;
        }
        for out in resampler.push(candle).expect("valid resample push") {
            rows.push(format!(
                "{},{},{},{},{},{}",
                out.open, out.high, out.low, out.close, out.volume, out.timestamp
            ));
        }
    }
    if let Some(out) = resampler.flush().expect("valid resample flush") {
        rows.push(format!(
            "{},{},{},{},{},{}",
            out.open, out.high, out.low, out.close, out.volume, out.timestamp
        ));
    }
    write_csv(
        dir,
        "data_resampled_gap",
        "open,high,low,close,volume,timestamp",
        &rows,
    );
}

/// Deterministic trade tick `i`: price on the shared varied path, a small
/// repeating size, and a timestamp that places roughly three ticks per
/// 1000-unit bucket. A deliberate jump at `i == 36` opens a multi-bucket gap so
/// the gap-fill fixture exercises several flat candles emitted from one push.
fn tick(i: usize) -> (f64, f64, i64) {
    let t = i as f64;
    let price = 100.0 + 10.0 * (t * 0.3).sin() + 0.5 * t;
    let size = 1.0 + (i % 5) as f64;
    let base = i64::try_from(i).expect("tick index fits i64") * 350;
    let ts = if i >= 36 { base + 5000 } else { base };
    (price, size, ts)
}

/// Data layer: the tick-to-candle aggregator. Writes the shared tick input plus
/// the reference candle streams with and without gap filling.
fn emit_data_layer(dir: &Path) {
    const N_TICKS: usize = 60;
    let ticks: Vec<(f64, f64, i64)> = (0..N_TICKS).map(tick).collect();

    let mut tin = Vec::with_capacity(N_TICKS);
    for &(price, size, ts) in &ticks {
        tin.push(format!("{price},{size},{ts}"));
    }
    write_csv(dir, "data_ticks", "price,size,timestamp", &tin);

    let header = "open,high,low,close,volume,timestamp";
    for (name, gap_fill) in [("data_candles", false), ("data_candles_gap", true)] {
        let mut agg = TickAggregator::new(Timeframe::new(1000).unwrap()).with_gap_fill(gap_fill);
        let mut rows = Vec::new();
        for &(price, size, ts) in &ticks {
            let tick = Tick::new(price, size, ts).expect("valid tick");
            for c in agg.push(tick).expect("valid push") {
                rows.push(format!(
                    "{},{},{},{},{},{}",
                    c.open, c.high, c.low, c.close, c.volume, c.timestamp
                ));
            }
        }
        write_csv(dir, name, header, &rows);
    }
}

// AUTO-GENERATED scalar-output golden tranche (single f64 output).
#[allow(clippy::too_many_lines)]
fn emit_scalar(dir: &Path, candles: &[Candle], closes: &[f64]) {
    if let Ok(mut ind) = wickra::AdaptiveCci::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_AdaptiveCci", "AdaptiveCci", &rows);
    } else {
        eprintln!("gen_golden skip AdaptiveCci");
    }
    if let Ok(mut ind) = wickra::AdaptiveRsi::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_AdaptiveRsi", "AdaptiveRsi", &rows);
    } else {
        eprintln!("gen_golden skip AdaptiveRsi");
    }
    {
        let mut ind = wickra::Adl::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Adl", "Adl", &rows);
    }
    {
        let mut ind = wickra::AdOscillator::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_AdOscillator", "AdOscillator", &rows);
    }
    if let Ok(mut ind) = wickra::Adxr::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Adxr", "Adxr", &rows);
    } else {
        eprintln!("gen_golden skip Adxr");
    }
    if let Ok(mut ind) = wickra::Alma::new(14, 2.0, 0.5) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Alma", "Alma", &rows);
    } else {
        eprintln!("gen_golden skip Alma");
    }
    if let Ok(mut ind) = wickra::Apo::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Apo", "Apo", &rows);
    } else {
        eprintln!("gen_golden skip Apo");
    }
    if let Ok(mut ind) = wickra::Atr::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Atr", "Atr", &rows);
    } else {
        eprintln!("gen_golden skip Atr");
    }
    if let Ok(mut ind) = wickra::AutocorrelationPeriodogram::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_AutocorrelationPeriodogram",
            "AutocorrelationPeriodogram",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip AutocorrelationPeriodogram");
    }
    {
        let mut ind = wickra::AvgPrice::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_AvgPrice", "AvgPrice", &rows);
    }
    {
        let mut ind = wickra::AbandonedBaby::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_AbandonedBaby", "AbandonedBaby", &rows);
    }
    {
        let mut ind = wickra::Abcd::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Abcd", "Abcd", &rows);
    }
    if let Ok(mut ind) = wickra::AcceleratorOscillator::new(3, 7, 14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(
            dir,
            "g_AcceleratorOscillator",
            "AcceleratorOscillator",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip AcceleratorOscillator");
    }
    {
        let mut ind = wickra::AdaptiveCycle::new();
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_AdaptiveCycle", "AdaptiveCycle", &rows);
    }
    {
        let mut ind = wickra::AdvanceBlock::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_AdvanceBlock", "AdvanceBlock", &rows);
    }
    if let Ok(mut ind) = wickra::Alpha::new(14, 2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_Alpha", "Alpha", &rows);
    } else {
        eprintln!("gen_golden skip Alpha");
    }
    {
        let mut ind = wickra::AnchoredRsi::new();
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_AnchoredRsi", "AnchoredRsi", &rows);
    }
    {
        let mut ind = wickra::AnchoredVwap::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_AnchoredVwap", "AnchoredVwap", &rows);
    }
    if let Ok(mut ind) = wickra::AroonOscillator::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_AroonOscillator", "AroonOscillator", &rows);
    } else {
        eprintln!("gen_golden skip AroonOscillator");
    }
    if let Ok(mut ind) = wickra::AtrTrailingStop::new(14, 2.0) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_AtrTrailingStop", "AtrTrailingStop", &rows);
    } else {
        eprintln!("gen_golden skip AtrTrailingStop");
    }
    if let Ok(mut ind) = wickra::Autocorrelation::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Autocorrelation", "Autocorrelation", &rows);
    } else {
        eprintln!("gen_golden skip Autocorrelation");
    }
    if let Ok(mut ind) = wickra::AverageDrawdown::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_AverageDrawdown", "AverageDrawdown", &rows);
    } else {
        eprintln!("gen_golden skip AverageDrawdown");
    }
    if let Ok(mut ind) = wickra::AwesomeOscillator::new(3, 7) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_AwesomeOscillator", "AwesomeOscillator", &rows);
    } else {
        eprintln!("gen_golden skip AwesomeOscillator");
    }
    if let Ok(mut ind) = wickra::AwesomeOscillatorHistogram::new(3, 7, 14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(
            dir,
            "g_AwesomeOscillatorHistogram",
            "AwesomeOscillatorHistogram",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip AwesomeOscillatorHistogram");
    }
    if let Ok(mut ind) = wickra::BandpassFilter::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_BandpassFilter", "BandpassFilter", &rows);
    } else {
        eprintln!("gen_golden skip BandpassFilter");
    }
    {
        let mut ind = wickra::BalanceOfPower::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_BalanceOfPower", "BalanceOfPower", &rows);
    }
    {
        let mut ind = wickra::Bat::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Bat", "Bat", &rows);
    }
    {
        let mut ind = wickra::BeltHold::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_BeltHold", "BeltHold", &rows);
    }
    if let Ok(mut ind) = wickra::Beta::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_Beta", "Beta", &rows);
    } else {
        eprintln!("gen_golden skip Beta");
    }
    if let Ok(mut ind) = wickra::BetaNeutralSpread::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_BetaNeutralSpread", "BetaNeutralSpread", &rows);
    } else {
        eprintln!("gen_golden skip BetaNeutralSpread");
    }
    if let Ok(mut ind) = wickra::BetterVolume::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_BetterVolume", "BetterVolume", &rows);
    } else {
        eprintln!("gen_golden skip BetterVolume");
    }
    if let Ok(mut ind) = wickra::BipowerVariation::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_BipowerVariation", "BipowerVariation", &rows);
    } else {
        eprintln!("gen_golden skip BipowerVariation");
    }
    {
        let mut ind = wickra::BodySizePct::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_BodySizePct", "BodySizePct", &rows);
    }
    if let Ok(mut ind) = wickra::BollingerBandwidth::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_BollingerBandwidth", "BollingerBandwidth", &rows);
    } else {
        eprintln!("gen_golden skip BollingerBandwidth");
    }
    {
        let mut ind = wickra::Breakaway::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Breakaway", "Breakaway", &rows);
    }
    if let Ok(mut ind) = wickra::BurkeRatio::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_BurkeRatio", "BurkeRatio", &rows);
    } else {
        eprintln!("gen_golden skip BurkeRatio");
    }
    {
        let mut ind = wickra::Butterfly::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Butterfly", "Butterfly", &rows);
    }
    if let Ok(mut ind) = wickra::Cci::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Cci", "Cci", &rows);
    } else {
        eprintln!("gen_golden skip Cci");
    }
    if let Ok(mut ind) = wickra::Cfo::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Cfo", "Cfo", &rows);
    } else {
        eprintln!("gen_golden skip Cfo");
    }
    if let Ok(mut ind) = wickra::Cmo::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Cmo", "Cmo", &rows);
    } else {
        eprintln!("gen_golden skip Cmo");
    }
    if let Ok(mut ind) = wickra::CorrelationTrendIndicator::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_CorrelationTrendIndicator",
            "CorrelationTrendIndicator",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip CorrelationTrendIndicator");
    }
    if let Ok(mut ind) = wickra::CalmarRatio::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_CalmarRatio", "CalmarRatio", &rows);
    } else {
        eprintln!("gen_golden skip CalmarRatio");
    }
    if let Ok(mut ind) = wickra::CenterOfGravity::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_CenterOfGravity", "CenterOfGravity", &rows);
    } else {
        eprintln!("gen_golden skip CenterOfGravity");
    }
    if let Ok(mut ind) = wickra::ChaikinOscillator::new(3, 7) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ChaikinOscillator", "ChaikinOscillator", &rows);
    } else {
        eprintln!("gen_golden skip ChaikinOscillator");
    }
    if let Ok(mut ind) = wickra::ChaikinVolatility::new(3, 7) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ChaikinVolatility", "ChaikinVolatility", &rows);
    } else {
        eprintln!("gen_golden skip ChaikinVolatility");
    }
    if let Ok(mut ind) = wickra::ChoppinessIndex::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ChoppinessIndex", "ChoppinessIndex", &rows);
    } else {
        eprintln!("gen_golden skip ChoppinessIndex");
    }
    {
        let mut ind = wickra::CloseVsOpen::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_CloseVsOpen", "CloseVsOpen", &rows);
    }
    {
        let mut ind = wickra::ClosingMarubozu::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ClosingMarubozu", "ClosingMarubozu", &rows);
    }
    if let Ok(mut ind) = wickra::CoefficientOfVariation::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_CoefficientOfVariation",
            "CoefficientOfVariation",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip CoefficientOfVariation");
    }
    if let Ok(mut ind) = wickra::CommonSenseRatio::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_CommonSenseRatio", "CommonSenseRatio", &rows);
    } else {
        eprintln!("gen_golden skip CommonSenseRatio");
    }
    {
        let mut ind = wickra::ConcealingBabySwallow::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(
            dir,
            "g_ConcealingBabySwallow",
            "ConcealingBabySwallow",
            &rows,
        );
    }
    if let Ok(mut ind) = wickra::ConditionalValueAtRisk::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_ConditionalValueAtRisk",
            "ConditionalValueAtRisk",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip ConditionalValueAtRisk");
    }
    if let Ok(mut ind) = wickra::ConnorsRsi::new(3, 7, 14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_ConnorsRsi", "ConnorsRsi", &rows);
    } else {
        eprintln!("gen_golden skip ConnorsRsi");
    }
    if let Ok(mut ind) = wickra::Coppock::new(3, 7, 14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Coppock", "Coppock", &rows);
    } else {
        eprintln!("gen_golden skip Coppock");
    }
    {
        let mut ind = wickra::Counterattack::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Counterattack", "Counterattack", &rows);
    }
    {
        let mut ind = wickra::Crab::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Crab", "Crab", &rows);
    }
    {
        let mut ind = wickra::CupAndHandle::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_CupAndHandle", "CupAndHandle", &rows);
    }
    if let Ok(mut ind) = wickra::CyberneticCycle::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_CyberneticCycle", "CyberneticCycle", &rows);
    } else {
        eprintln!("gen_golden skip CyberneticCycle");
    }
    {
        let mut ind = wickra::Cypher::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Cypher", "Cypher", &rows);
    }
    if let Ok(mut ind) = wickra::Dema::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Dema", "Dema", &rows);
    } else {
        eprintln!("gen_golden skip Dema");
    }
    if let Ok(mut ind) = wickra::Dpo::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Dpo", "Dpo", &rows);
    } else {
        eprintln!("gen_golden skip Dpo");
    }
    if let Ok(mut ind) = wickra::Dx::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Dx", "Dx", &rows);
    } else {
        eprintln!("gen_golden skip Dx");
    }
    if let Ok(mut ind) = wickra::Decycler::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Decycler", "Decycler", &rows);
    } else {
        eprintln!("gen_golden skip Decycler");
    }
    if let Ok(mut ind) = wickra::DecyclerOscillator::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_DecyclerOscillator", "DecyclerOscillator", &rows);
    } else {
        eprintln!("gen_golden skip DecyclerOscillator");
    }
    if let Ok(mut ind) = wickra::DemandIndex::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_DemandIndex", "DemandIndex", &rows);
    } else {
        eprintln!("gen_golden skip DemandIndex");
    }
    if let Ok(mut ind) = wickra::DerivativeOscillator::new(3, 7, 14, 28) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_DerivativeOscillator", "DerivativeOscillator", &rows);
    } else {
        eprintln!("gen_golden skip DerivativeOscillator");
    }
    if let Ok(mut ind) = wickra::DetrendedStdDev::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_DetrendedStdDev", "DetrendedStdDev", &rows);
    } else {
        eprintln!("gen_golden skip DetrendedStdDev");
    }
    if let Ok(mut ind) = wickra::DisparityIndex::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_DisparityIndex", "DisparityIndex", &rows);
    } else {
        eprintln!("gen_golden skip DisparityIndex");
    }
    if let Ok(mut ind) = wickra::DistanceSsd::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_DistanceSsd", "DistanceSsd", &rows);
    } else {
        eprintln!("gen_golden skip DistanceSsd");
    }
    {
        let mut ind = wickra::Doji::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Doji", "Doji", &rows);
    }
    {
        let mut ind = wickra::DojiStar::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_DojiStar", "DojiStar", &rows);
    }
    {
        let mut ind = wickra::DoubleTopBottom::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_DoubleTopBottom", "DoubleTopBottom", &rows);
    }
    {
        let mut ind = wickra::DownsideGapThreeMethods::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(
            dir,
            "g_DownsideGapThreeMethods",
            "DownsideGapThreeMethods",
            &rows,
        );
    }
    {
        let mut ind = wickra::DragonflyDoji::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_DragonflyDoji", "DragonflyDoji", &rows);
    }
    if let Ok(mut ind) = wickra::DumplingTop::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_DumplingTop", "DumplingTop", &rows);
    } else {
        eprintln!("gen_golden skip DumplingTop");
    }
    if let Ok(mut ind) = wickra::DynamicMomentumIndex::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_DynamicMomentumIndex", "DynamicMomentumIndex", &rows);
    } else {
        eprintln!("gen_golden skip DynamicMomentumIndex");
    }
    if let Ok(mut ind) = wickra::Ehma::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Ehma", "Ehma", &rows);
    } else {
        eprintln!("gen_golden skip Ehma");
    }
    if let Ok(mut ind) = wickra::Ema::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Ema", "Ema", &rows);
    } else {
        eprintln!("gen_golden skip Ema");
    }
    if let Ok(mut ind) = wickra::Evwma::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Evwma", "Evwma", &rows);
    } else {
        eprintln!("gen_golden skip Evwma");
    }
    if let Ok(mut ind) = wickra::EaseOfMovement::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_EaseOfMovement", "EaseOfMovement", &rows);
    } else {
        eprintln!("gen_golden skip EaseOfMovement");
    }
    if let Ok(mut ind) = wickra::EhlersStochastic::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_EhlersStochastic", "EhlersStochastic", &rows);
    } else {
        eprintln!("gen_golden skip EhlersStochastic");
    }
    if let Ok(mut ind) = wickra::ElderImpulse::new(3, 7, 14, 28) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_ElderImpulse", "ElderImpulse", &rows);
    } else {
        eprintln!("gen_golden skip ElderImpulse");
    }
    if let Ok(mut ind) = wickra::EmpiricalModeDecomposition::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_EmpiricalModeDecomposition",
            "EmpiricalModeDecomposition",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip EmpiricalModeDecomposition");
    }
    {
        let mut ind = wickra::Engulfing::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Engulfing", "Engulfing", &rows);
    }
    {
        let mut ind = wickra::EveningDojiStar::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_EveningDojiStar", "EveningDojiStar", &rows);
    }
    if let Ok(mut ind) = wickra::EwmaVolatility::new(2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_EwmaVolatility", "EwmaVolatility", &rows);
    } else {
        eprintln!("gen_golden skip EwmaVolatility");
    }
    if let Ok(mut ind) = wickra::Expectancy::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Expectancy", "Expectancy", &rows);
    } else {
        eprintln!("gen_golden skip Expectancy");
    }
    if let Ok(mut ind) = wickra::Fama::new(2.0, 0.5) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Fama", "Fama", &rows);
    } else {
        eprintln!("gen_golden skip Fama");
    }
    if let Ok(mut ind) = wickra::Frama::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Frama", "Frama", &rows);
    } else {
        eprintln!("gen_golden skip Frama");
    }
    {
        let mut ind = wickra::FallingThreeMethods::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_FallingThreeMethods", "FallingThreeMethods", &rows);
    }
    if let Ok(mut ind) = wickra::FisherRsi::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_FisherRsi", "FisherRsi", &rows);
    } else {
        eprintln!("gen_golden skip FisherRsi");
    }
    if let Ok(mut ind) = wickra::FisherTransform::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_FisherTransform", "FisherTransform", &rows);
    } else {
        eprintln!("gen_golden skip FisherTransform");
    }
    {
        let mut ind = wickra::FlagPennant::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_FlagPennant", "FlagPennant", &rows);
    }
    if let Ok(mut ind) = wickra::ForceIndex::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ForceIndex", "ForceIndex", &rows);
    } else {
        eprintln!("gen_golden skip ForceIndex");
    }
    if let Ok(mut ind) = wickra::FryPanBottom::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_FryPanBottom", "FryPanBottom", &rows);
    } else {
        eprintln!("gen_golden skip FryPanBottom");
    }
    if let Ok(mut ind) = wickra::GeneralizedDema::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_GeneralizedDema", "GeneralizedDema", &rows);
    } else {
        eprintln!("gen_golden skip GeneralizedDema");
    }
    if let Ok(mut ind) = wickra::GeometricMa::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_GeometricMa", "GeometricMa", &rows);
    } else {
        eprintln!("gen_golden skip GeometricMa");
    }
    if let Ok(mut ind) = wickra::GainLossRatio::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_GainLossRatio", "GainLossRatio", &rows);
    } else {
        eprintln!("gen_golden skip GainLossRatio");
    }
    if let Ok(mut ind) = wickra::GainToPainRatio::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_GainToPainRatio", "GainToPainRatio", &rows);
    } else {
        eprintln!("gen_golden skip GainToPainRatio");
    }
    {
        let mut ind = wickra::GapSideBySideWhite::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_GapSideBySideWhite", "GapSideBySideWhite", &rows);
    }
    if let Ok(mut ind) = wickra::Garch11::new(2.0, 0.5, 0.5) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Garch11", "Garch11", &rows);
    } else {
        eprintln!("gen_golden skip Garch11");
    }
    {
        let mut ind = wickra::Gartley::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Gartley", "Gartley", &rows);
    }
    if let Ok(mut ind) = wickra::GrangerCausality::new(3, 7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_GrangerCausality", "GrangerCausality", &rows);
    } else {
        eprintln!("gen_golden skip GrangerCausality");
    }
    {
        let mut ind = wickra::GravestoneDoji::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_GravestoneDoji", "GravestoneDoji", &rows);
    }
    if let Ok(mut ind) = wickra::HighpassFilter::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_HighpassFilter", "HighpassFilter", &rows);
    } else {
        eprintln!("gen_golden skip HighpassFilter");
    }
    if let Ok(mut ind) = wickra::Hma::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Hma", "Hma", &rows);
    } else {
        eprintln!("gen_golden skip Hma");
    }
    {
        let mut ind = wickra::Hammer::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Hammer", "Hammer", &rows);
    }
    {
        let mut ind = wickra::HangingMan::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_HangingMan", "HangingMan", &rows);
    }
    {
        let mut ind = wickra::Harami::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Harami", "Harami", &rows);
    }
    {
        let mut ind = wickra::HaramiCross::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_HaramiCross", "HaramiCross", &rows);
    }
    if let Ok(mut ind) = wickra::HasbrouckInformationShare::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(
            dir,
            "g_HasbrouckInformationShare",
            "HasbrouckInformationShare",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip HasbrouckInformationShare");
    }
    {
        let mut ind = wickra::EstimatedLeverageRatio::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(
            dir,
            "g_EstimatedLeverageRatio",
            "EstimatedLeverageRatio",
            &rows,
        );
    }
    {
        let mut ind = wickra::HeadAndShoulders::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_HeadAndShoulders", "HeadAndShoulders", &rows);
    }
    if let Ok(mut ind) = wickra::HeikinAshiOscillator::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_HeikinAshiOscillator", "HeikinAshiOscillator", &rows);
    } else {
        eprintln!("gen_golden skip HeikinAshiOscillator");
    }
    {
        let mut ind = wickra::HighLowRange::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_HighLowRange", "HighLowRange", &rows);
    }
    {
        let mut ind = wickra::HighWave::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_HighWave", "HighWave", &rows);
    }
    {
        let mut ind = wickra::Hikkake::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Hikkake", "Hikkake", &rows);
    }
    {
        let mut ind = wickra::HikkakeModified::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_HikkakeModified", "HikkakeModified", &rows);
    }
    {
        let mut ind = wickra::HilbertDominantCycle::new();
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_HilbertDominantCycle", "HilbertDominantCycle", &rows);
    }
    if let Ok(mut ind) = wickra::HistoricalVolatility::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_HistoricalVolatility", "HistoricalVolatility", &rows);
    } else {
        eprintln!("gen_golden skip HistoricalVolatility");
    }
    if let Ok(mut ind) = wickra::HoltWinters::new(2.0, 0.5) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_HoltWinters", "HoltWinters", &rows);
    } else {
        eprintln!("gen_golden skip HoltWinters");
    }
    {
        let mut ind = wickra::HomingPigeon::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_HomingPigeon", "HomingPigeon", &rows);
    }
    if let Ok(mut ind) = wickra::HurstExponent::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_HurstExponent", "HurstExponent", &rows);
    } else {
        eprintln!("gen_golden skip HurstExponent");
    }
    if let Ok(mut ind) = wickra::IntradayMomentumIndex::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(
            dir,
            "g_IntradayMomentumIndex",
            "IntradayMomentumIndex",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip IntradayMomentumIndex");
    }
    {
        let mut ind = wickra::IdenticalThreeCrows::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_IdenticalThreeCrows", "IdenticalThreeCrows", &rows);
    }
    {
        let mut ind = wickra::InNeck::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_InNeck", "InNeck", &rows);
    }
    if let Ok(mut ind) = wickra::Inertia::new(3, 7) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Inertia", "Inertia", &rows);
    } else {
        eprintln!("gen_golden skip Inertia");
    }
    if let Ok(mut ind) = wickra::InformationRatio::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_InformationRatio", "InformationRatio", &rows);
    } else {
        eprintln!("gen_golden skip InformationRatio");
    }
    if let Ok(mut ind) = wickra::InstantaneousTrendline::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_InstantaneousTrendline",
            "InstantaneousTrendline",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip InstantaneousTrendline");
    }
    {
        let mut ind = wickra::IntradayIntensity::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_IntradayIntensity", "IntradayIntensity", &rows);
    }
    if let Ok(mut ind) = wickra::InverseFisherTransform::new(2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_InverseFisherTransform",
            "InverseFisherTransform",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip InverseFisherTransform");
    }
    {
        let mut ind = wickra::InvertedHammer::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_InvertedHammer", "InvertedHammer", &rows);
    }
    if let Ok(mut ind) = wickra::JarqueBera::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_JarqueBera", "JarqueBera", &rows);
    } else {
        eprintln!("gen_golden skip JarqueBera");
    }
    if let Ok(mut ind) = wickra::Jma::new(3, 2.0, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Jma", "Jma", &rows);
    } else {
        eprintln!("gen_golden skip Jma");
    }
    if let Ok(mut ind) = wickra::JumpIndicator::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_JumpIndicator", "JumpIndicator", &rows);
    } else {
        eprintln!("gen_golden skip JumpIndicator");
    }
    if let Ok(mut ind) = wickra::Kama::new(3, 7, 14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Kama", "Kama", &rows);
    } else {
        eprintln!("gen_golden skip Kama");
    }
    if let Ok(mut ind) = wickra::KRatio::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_KRatio", "KRatio", &rows);
    } else {
        eprintln!("gen_golden skip KRatio");
    }
    if let Ok(mut ind) = wickra::Kvo::new(3, 7) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Kvo", "Kvo", &rows);
    } else {
        eprintln!("gen_golden skip Kvo");
    }
    if let Ok(mut ind) = wickra::KellyCriterion::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_KellyCriterion", "KellyCriterion", &rows);
    } else {
        eprintln!("gen_golden skip KellyCriterion");
    }
    if let Ok(mut ind) = wickra::KendallTau::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_KendallTau", "KendallTau", &rows);
    } else {
        eprintln!("gen_golden skip KendallTau");
    }
    {
        let mut ind = wickra::Kicking::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Kicking", "Kicking", &rows);
    }
    {
        let mut ind = wickra::KickingByLength::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_KickingByLength", "KickingByLength", &rows);
    }
    if let Ok(mut ind) = wickra::Kurtosis::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Kurtosis", "Kurtosis", &rows);
    } else {
        eprintln!("gen_golden skip Kurtosis");
    }
    {
        let mut ind = wickra::LadderBottom::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_LadderBottom", "LadderBottom", &rows);
    }
    if let Ok(mut ind) = wickra::LaguerreRsi::new(2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_LaguerreRsi", "LaguerreRsi", &rows);
    } else {
        eprintln!("gen_golden skip LaguerreRsi");
    }
    if let Ok(mut ind) = wickra::LogReturn::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_LogReturn", "LogReturn", &rows);
    } else {
        eprintln!("gen_golden skip LogReturn");
    }
    {
        let mut ind = wickra::LongLeggedDoji::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_LongLeggedDoji", "LongLeggedDoji", &rows);
    }
    {
        let mut ind = wickra::LongLine::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_LongLine", "LongLine", &rows);
    }
    if let Ok(mut ind) = wickra::M2Measure::new(14, 2.0, 0.5) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_M2Measure", "M2Measure", &rows);
    } else {
        eprintln!("gen_golden skip M2Measure");
    }
    if let Ok(mut ind) = wickra::Mfi::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Mfi", "Mfi", &rows);
    } else {
        eprintln!("gen_golden skip Mfi");
    }
    if let Ok(mut ind) = wickra::MidPoint::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_MidPoint", "MidPoint", &rows);
    } else {
        eprintln!("gen_golden skip MidPoint");
    }
    if let Ok(mut ind) = wickra::MidPrice::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_MidPrice", "MidPrice", &rows);
    } else {
        eprintln!("gen_golden skip MidPrice");
    }
    if let Ok(mut ind) = wickra::MinusDi::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_MinusDi", "MinusDi", &rows);
    } else {
        eprintln!("gen_golden skip MinusDi");
    }
    if let Ok(mut ind) = wickra::MinusDm::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_MinusDm", "MinusDm", &rows);
    } else {
        eprintln!("gen_golden skip MinusDm");
    }
    if let Ok(mut ind) = wickra::Mom::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Mom", "Mom", &rows);
    } else {
        eprintln!("gen_golden skip Mom");
    }
    if let Ok(mut ind) = wickra::MacdHistogram::new(3, 7, 14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_MacdHistogram", "MacdHistogram", &rows);
    } else {
        eprintln!("gen_golden skip MacdHistogram");
    }
    {
        let mut ind = wickra::MarketFacilitationIndex::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(
            dir,
            "g_MarketFacilitationIndex",
            "MarketFacilitationIndex",
            &rows,
        );
    }
    if let Ok(mut ind) = wickra::MartinRatio::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_MartinRatio", "MartinRatio", &rows);
    } else {
        eprintln!("gen_golden skip MartinRatio");
    }
    {
        let mut ind = wickra::Marubozu::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Marubozu", "Marubozu", &rows);
    }
    if let Ok(mut ind) = wickra::MassIndex::new(3, 7) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_MassIndex", "MassIndex", &rows);
    } else {
        eprintln!("gen_golden skip MassIndex");
    }
    {
        let mut ind = wickra::MatHold::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_MatHold", "MatHold", &rows);
    }
    {
        let mut ind = wickra::MatchingLow::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_MatchingLow", "MatchingLow", &rows);
    }
    if let Ok(mut ind) = wickra::MaxDrawdown::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_MaxDrawdown", "MaxDrawdown", &rows);
    } else {
        eprintln!("gen_golden skip MaxDrawdown");
    }
    if let Ok(mut ind) = wickra::MedianAbsoluteDeviation::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_MedianAbsoluteDeviation",
            "MedianAbsoluteDeviation",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip MedianAbsoluteDeviation");
    }
    if let Ok(mut ind) = wickra::MedianMa::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_MedianMa", "MedianMa", &rows);
    } else {
        eprintln!("gen_golden skip MedianMa");
    }
    {
        let mut ind = wickra::MedianPrice::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_MedianPrice", "MedianPrice", &rows);
    }
    {
        let mut ind = wickra::MorningDojiStar::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_MorningDojiStar", "MorningDojiStar", &rows);
    }
    {
        let mut ind = wickra::MorningEveningStar::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_MorningEveningStar", "MorningEveningStar", &rows);
    }
    if let Ok(mut ind) = wickra::Natr::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Natr", "Natr", &rows);
    } else {
        eprintln!("gen_golden skip Natr");
    }
    {
        let mut ind = wickra::Nvi::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Nvi", "Nvi", &rows);
    }
    if let Ok(mut ind) = wickra::NakedPoc::new(3, 7) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_NakedPoc", "NakedPoc", &rows);
    } else {
        eprintln!("gen_golden skip NakedPoc");
    }
    if let Ok(mut ind) = wickra::NewPriceLines::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_NewPriceLines", "NewPriceLines", &rows);
    } else {
        eprintln!("gen_golden skip NewPriceLines");
    }
    {
        let mut ind = wickra::Obv::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Obv", "Obv", &rows);
    }
    if let Ok(mut ind) = wickra::OmegaRatio::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_OmegaRatio", "OmegaRatio", &rows);
    } else {
        eprintln!("gen_golden skip OmegaRatio");
    }
    {
        let mut ind = wickra::OnNeck::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_OnNeck", "OnNeck", &rows);
    }
    {
        let mut ind = wickra::OpeningMarubozu::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_OpeningMarubozu", "OpeningMarubozu", &rows);
    }
    if let Ok(mut ind) = wickra::OuHalfLife::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_OuHalfLife", "OuHalfLife", &rows);
    } else {
        eprintln!("gen_golden skip OuHalfLife");
    }
    if let Ok(mut ind) = wickra::Pgo::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Pgo", "Pgo", &rows);
    } else {
        eprintln!("gen_golden skip Pgo");
    }
    if let Ok(mut ind) = wickra::PlusDi::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_PlusDi", "PlusDi", &rows);
    } else {
        eprintln!("gen_golden skip PlusDi");
    }
    if let Ok(mut ind) = wickra::PlusDm::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_PlusDm", "PlusDm", &rows);
    } else {
        eprintln!("gen_golden skip PlusDm");
    }
    if let Ok(mut ind) = wickra::Pmo::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Pmo", "Pmo", &rows);
    } else {
        eprintln!("gen_golden skip Pmo");
    }
    if let Ok(mut ind) = wickra::Ppo::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Ppo", "Ppo", &rows);
    } else {
        eprintln!("gen_golden skip Ppo");
    }
    if let Ok(mut ind) = wickra::Psar::new(2.0, 0.5, 0.5) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Psar", "Psar", &rows);
    } else {
        eprintln!("gen_golden skip Psar");
    }
    {
        let mut ind = wickra::Pvi::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Pvi", "Pvi", &rows);
    }
    if let Ok(mut ind) = wickra::PainIndex::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_PainIndex", "PainIndex", &rows);
    } else {
        eprintln!("gen_golden skip PainIndex");
    }
    if let Ok(mut ind) = wickra::PairwiseBeta::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_PairwiseBeta", "PairwiseBeta", &rows);
    } else {
        eprintln!("gen_golden skip PairwiseBeta");
    }
    if let Ok(mut ind) = wickra::PearsonCorrelation::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_PearsonCorrelation", "PearsonCorrelation", &rows);
    } else {
        eprintln!("gen_golden skip PearsonCorrelation");
    }
    if let Ok(mut ind) = wickra::PercentB::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_PercentB", "PercentB", &rows);
    } else {
        eprintln!("gen_golden skip PercentB");
    }
    if let Ok(mut ind) = wickra::PercentageTrailingStop::new(2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_PercentageTrailingStop",
            "PercentageTrailingStop",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip PercentageTrailingStop");
    }
    {
        let mut ind = wickra::PiercingDarkCloud::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_PiercingDarkCloud", "PiercingDarkCloud", &rows);
    }
    if let Ok(mut ind) = wickra::PivotReversal::new(3, 7) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_PivotReversal", "PivotReversal", &rows);
    } else {
        eprintln!("gen_golden skip PivotReversal");
    }
    if let Ok(mut ind) = wickra::PpoHistogram::new(3, 7, 14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_PpoHistogram", "PpoHistogram", &rows);
    } else {
        eprintln!("gen_golden skip PpoHistogram");
    }
    if let Ok(mut ind) = wickra::ProfileShape::new(3, 7) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ProfileShape", "ProfileShape", &rows);
    } else {
        eprintln!("gen_golden skip ProfileShape");
    }
    if let Ok(mut ind) = wickra::ProfitFactor::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_ProfitFactor", "ProfitFactor", &rows);
    } else {
        eprintln!("gen_golden skip ProfitFactor");
    }
    if let Ok(mut ind) = wickra::ProjectionOscillator::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ProjectionOscillator", "ProjectionOscillator", &rows);
    } else {
        eprintln!("gen_golden skip ProjectionOscillator");
    }
    if let Ok(mut ind) = wickra::Qstick::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Qstick", "Qstick", &rows);
    } else {
        eprintln!("gen_golden skip Qstick");
    }
    if let Ok(mut ind) = wickra::Reflex::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Reflex", "Reflex", &rows);
    } else {
        eprintln!("gen_golden skip Reflex");
    }
    if let Ok(mut ind) = wickra::Rmi::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Rmi", "Rmi", &rows);
    } else {
        eprintln!("gen_golden skip Rmi");
    }
    if let Ok(mut ind) = wickra::Roc::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Roc", "Roc", &rows);
    } else {
        eprintln!("gen_golden skip Roc");
    }
    if let Ok(mut ind) = wickra::Rocp::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Rocp", "Rocp", &rows);
    } else {
        eprintln!("gen_golden skip Rocp");
    }
    if let Ok(mut ind) = wickra::Rocr::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Rocr", "Rocr", &rows);
    } else {
        eprintln!("gen_golden skip Rocr");
    }
    if let Ok(mut ind) = wickra::Rocr100::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Rocr100", "Rocr100", &rows);
    } else {
        eprintln!("gen_golden skip Rocr100");
    }
    if let Ok(mut ind) = wickra::RollingMinMaxScaler::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_RollingMinMaxScaler", "RollingMinMaxScaler", &rows);
    } else {
        eprintln!("gen_golden skip RollingMinMaxScaler");
    }
    if let Ok(mut ind) = wickra::Rsi::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Rsi", "Rsi", &rows);
    } else {
        eprintln!("gen_golden skip Rsi");
    }
    if let Ok(mut ind) = wickra::Rsx::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Rsx", "Rsx", &rows);
    } else {
        eprintln!("gen_golden skip Rsx");
    }
    if let Ok(mut ind) = wickra::RSquared::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_RSquared", "RSquared", &rows);
    } else {
        eprintln!("gen_golden skip RSquared");
    }
    if let Ok(mut ind) = wickra::Rvi::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Rvi", "Rvi", &rows);
    } else {
        eprintln!("gen_golden skip Rvi");
    }
    if let Ok(mut ind) = wickra::RviVolatility::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_RviVolatility", "RviVolatility", &rows);
    } else {
        eprintln!("gen_golden skip RviVolatility");
    }
    if let Ok(mut ind) = wickra::RealizedVolatility::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_RealizedVolatility", "RealizedVolatility", &rows);
    } else {
        eprintln!("gen_golden skip RealizedVolatility");
    }
    {
        let mut ind = wickra::RecoveryFactor::new();
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_RecoveryFactor", "RecoveryFactor", &rows);
    }
    {
        let mut ind = wickra::RectangleRange::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_RectangleRange", "RectangleRange", &rows);
    }
    if let Ok(mut ind) = wickra::RegimeLabel::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_RegimeLabel", "RegimeLabel", &rows);
    } else {
        eprintln!("gen_golden skip RegimeLabel");
    }
    if let Ok(mut ind) = wickra::RenkoTrailingStop::new(2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_RenkoTrailingStop", "RenkoTrailingStop", &rows);
    } else {
        eprintln!("gen_golden skip RenkoTrailingStop");
    }
    {
        let mut ind = wickra::RickshawMan::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_RickshawMan", "RickshawMan", &rows);
    }
    {
        let mut ind = wickra::RisingThreeMethods::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_RisingThreeMethods", "RisingThreeMethods", &rows);
    }
    if let Ok(mut ind) = wickra::RollingCorrelation::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_RollingCorrelation", "RollingCorrelation", &rows);
    } else {
        eprintln!("gen_golden skip RollingCorrelation");
    }
    if let Ok(mut ind) = wickra::RollingCovariance::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_RollingCovariance", "RollingCovariance", &rows);
    } else {
        eprintln!("gen_golden skip RollingCovariance");
    }
    if let Ok(mut ind) = wickra::RollingIqr::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_RollingIqr", "RollingIqr", &rows);
    } else {
        eprintln!("gen_golden skip RollingIqr");
    }
    if let Ok(mut ind) = wickra::RollingPercentileRank::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_RollingPercentileRank",
            "RollingPercentileRank",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip RollingPercentileRank");
    }
    if let Ok(mut ind) = wickra::RollingQuantile::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_RollingQuantile", "RollingQuantile", &rows);
    } else {
        eprintln!("gen_golden skip RollingQuantile");
    }
    if let Ok(mut ind) = wickra::RoofingFilter::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_RoofingFilter", "RoofingFilter", &rows);
    } else {
        eprintln!("gen_golden skip RoofingFilter");
    }
    if let Ok(mut ind) = wickra::SampleEntropy::new(3, 7, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_SampleEntropy", "SampleEntropy", &rows);
    } else {
        eprintln!("gen_golden skip SampleEntropy");
    }
    if let Ok(mut ind) = wickra::SarExt::new(2.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_SarExt", "SarExt", &rows);
    } else {
        eprintln!("gen_golden skip SarExt");
    }
    if let Ok(mut ind) = wickra::ShannonEntropy::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_ShannonEntropy", "ShannonEntropy", &rows);
    } else {
        eprintln!("gen_golden skip ShannonEntropy");
    }
    if let Ok(mut ind) = wickra::Sma::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Sma", "Sma", &rows);
    } else {
        eprintln!("gen_golden skip Sma");
    }
    if let Ok(mut ind) = wickra::Smi::new(3, 7, 14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Smi", "Smi", &rows);
    } else {
        eprintln!("gen_golden skip Smi");
    }
    if let Ok(mut ind) = wickra::Smma::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Smma", "Smma", &rows);
    } else {
        eprintln!("gen_golden skip Smma");
    }
    if let Ok(mut ind) = wickra::Stc::new(3, 7, 14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Stc", "Stc", &rows);
    } else {
        eprintln!("gen_golden skip Stc");
    }
    if let Ok(mut ind) = wickra::SineWeightedMa::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_SineWeightedMa", "SineWeightedMa", &rows);
    } else {
        eprintln!("gen_golden skip SineWeightedMa");
    }
    {
        let mut ind = wickra::SeasonalZScore::new(14);
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_SeasonalZScore", "SeasonalZScore", &rows);
    }
    {
        let mut ind = wickra::SeparatingLines::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_SeparatingLines", "SeparatingLines", &rows);
    }
    {
        let mut ind = wickra::SessionVwap::new(14);
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_SessionVwap", "SessionVwap", &rows);
    }
    {
        let mut ind = wickra::Shark::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Shark", "Shark", &rows);
    }
    if let Ok(mut ind) = wickra::SharpeRatio::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_SharpeRatio", "SharpeRatio", &rows);
    } else {
        eprintln!("gen_golden skip SharpeRatio");
    }
    {
        let mut ind = wickra::ShootingStar::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ShootingStar", "ShootingStar", &rows);
    }
    {
        let mut ind = wickra::ShortLine::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ShortLine", "ShortLine", &rows);
    }
    {
        let mut ind = wickra::SineWave::new();
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_SineWave", "SineWave", &rows);
    }
    if let Ok(mut ind) = wickra::SinglePrints::new(3, 7) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_SinglePrints", "SinglePrints", &rows);
    } else {
        eprintln!("gen_golden skip SinglePrints");
    }
    if let Ok(mut ind) = wickra::Skewness::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Skewness", "Skewness", &rows);
    } else {
        eprintln!("gen_golden skip Skewness");
    }
    if let Ok(mut ind) = wickra::SortinoRatio::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_SortinoRatio", "SortinoRatio", &rows);
    } else {
        eprintln!("gen_golden skip SortinoRatio");
    }
    if let Ok(mut ind) = wickra::SpearmanCorrelation::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_SpearmanCorrelation", "SpearmanCorrelation", &rows);
    } else {
        eprintln!("gen_golden skip SpearmanCorrelation");
    }
    {
        let mut ind = wickra::SpinningTop::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_SpinningTop", "SpinningTop", &rows);
    }
    if let Ok(mut ind) = wickra::SpreadAr1Coefficient::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_SpreadAr1Coefficient", "SpreadAr1Coefficient", &rows);
    } else {
        eprintln!("gen_golden skip SpreadAr1Coefficient");
    }
    if let Ok(mut ind) = wickra::SpreadHurst::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_SpreadHurst", "SpreadHurst", &rows);
    } else {
        eprintln!("gen_golden skip SpreadHurst");
    }
    {
        let mut ind = wickra::StalledPattern::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_StalledPattern", "StalledPattern", &rows);
    }
    if let Ok(mut ind) = wickra::StandardError::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_StandardError", "StandardError", &rows);
    } else {
        eprintln!("gen_golden skip StandardError");
    }
    if let Ok(mut ind) = wickra::StdDev::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_StdDev", "StdDev", &rows);
    } else {
        eprintln!("gen_golden skip StdDev");
    }
    if let Ok(mut ind) = wickra::StepTrailingStop::new(2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_StepTrailingStop", "StepTrailingStop", &rows);
    } else {
        eprintln!("gen_golden skip StepTrailingStop");
    }
    if let Ok(mut ind) = wickra::SterlingRatio::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_SterlingRatio", "SterlingRatio", &rows);
    } else {
        eprintln!("gen_golden skip SterlingRatio");
    }
    {
        let mut ind = wickra::StickSandwich::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_StickSandwich", "StickSandwich", &rows);
    }
    if let Ok(mut ind) = wickra::StochRsi::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_StochRsi", "StochRsi", &rows);
    } else {
        eprintln!("gen_golden skip StochRsi");
    }
    if let Ok(mut ind) = wickra::StochasticCci::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_StochasticCci", "StochasticCci", &rows);
    } else {
        eprintln!("gen_golden skip StochasticCci");
    }
    if let Ok(mut ind) = wickra::SuperSmoother::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_SuperSmoother", "SuperSmoother", &rows);
    } else {
        eprintln!("gen_golden skip SuperSmoother");
    }
    if let Ok(mut ind) = wickra::T3::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_T3", "T3", &rows);
    } else {
        eprintln!("gen_golden skip T3");
    }
    {
        let mut ind = wickra::TdCamouflage::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdCamouflage", "TdCamouflage", &rows);
    }
    {
        let mut ind = wickra::TdClop::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdClop", "TdClop", &rows);
    }
    {
        let mut ind = wickra::TdClopwin::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdClopwin", "TdClopwin", &rows);
    }
    if let Ok(mut ind) = wickra::TdCombo::new(3, 7, 14, 28) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdCombo", "TdCombo", &rows);
    } else {
        eprintln!("gen_golden skip TdCombo");
    }
    if let Ok(mut ind) = wickra::TdCountdown::new(3, 7, 14, 28) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdCountdown", "TdCountdown", &rows);
    } else {
        eprintln!("gen_golden skip TdCountdown");
    }
    {
        let mut ind = wickra::TdDifferential::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdDifferential", "TdDifferential", &rows);
    }
    {
        let mut ind = wickra::TdOpen::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdOpen", "TdOpen", &rows);
    }
    if let Ok(mut ind) = wickra::TdPressure::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdPressure", "TdPressure", &rows);
    } else {
        eprintln!("gen_golden skip TdPressure");
    }
    {
        let mut ind = wickra::TdPropulsion::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdPropulsion", "TdPropulsion", &rows);
    }
    if let Ok(mut ind) = wickra::TdRei::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdRei", "TdRei", &rows);
    } else {
        eprintln!("gen_golden skip TdRei");
    }
    if let Ok(mut ind) = wickra::TdSetup::new(3, 7) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdSetup", "TdSetup", &rows);
    } else {
        eprintln!("gen_golden skip TdSetup");
    }
    {
        let mut ind = wickra::TdTrap::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdTrap", "TdTrap", &rows);
    }
    if let Ok(mut ind) = wickra::Tema::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Tema", "Tema", &rows);
    } else {
        eprintln!("gen_golden skip Tema");
    }
    if let Ok(mut ind) = wickra::Tii::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Tii", "Tii", &rows);
    } else {
        eprintln!("gen_golden skip Tii");
    }
    if let Ok(mut ind) = wickra::Trendflex::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Trendflex", "Trendflex", &rows);
    } else {
        eprintln!("gen_golden skip Trendflex");
    }
    if let Ok(mut ind) = wickra::Trima::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Trima", "Trima", &rows);
    } else {
        eprintln!("gen_golden skip Trima");
    }
    if let Ok(mut ind) = wickra::Trix::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Trix", "Trix", &rows);
    } else {
        eprintln!("gen_golden skip Trix");
    }
    if let Ok(mut ind) = wickra::Tsf::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Tsf", "Tsf", &rows);
    } else {
        eprintln!("gen_golden skip Tsf");
    }
    if let Ok(mut ind) = wickra::Tsi::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Tsi", "Tsi", &rows);
    } else {
        eprintln!("gen_golden skip Tsi");
    }
    if let Ok(mut ind) = wickra::Tsv::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Tsv", "Tsv", &rows);
    } else {
        eprintln!("gen_golden skip Tsv");
    }
    if let Ok(mut ind) = wickra::TtmTrend::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TtmTrend", "TtmTrend", &rows);
    } else {
        eprintln!("gen_golden skip TtmTrend");
    }
    if let Ok(mut ind) = wickra::TailRatio::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_TailRatio", "TailRatio", &rows);
    } else {
        eprintln!("gen_golden skip TailRatio");
    }
    {
        let mut ind = wickra::Takuri::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Takuri", "Takuri", &rows);
    }
    {
        let mut ind = wickra::TasukiGap::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TasukiGap", "TasukiGap", &rows);
    }
    {
        let mut ind = wickra::ThreeDrives::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ThreeDrives", "ThreeDrives", &rows);
    }
    {
        let mut ind = wickra::ThreeInside::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ThreeInside", "ThreeInside", &rows);
    }
    if let Ok(mut ind) = wickra::ThreeLineBreak::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ThreeLineBreak", "ThreeLineBreak", &rows);
    } else {
        eprintln!("gen_golden skip ThreeLineBreak");
    }
    {
        let mut ind = wickra::ThreeLineStrike::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ThreeLineStrike", "ThreeLineStrike", &rows);
    }
    {
        let mut ind = wickra::ThreeOutside::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ThreeOutside", "ThreeOutside", &rows);
    }
    {
        let mut ind = wickra::ThreeSoldiersOrCrows::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ThreeSoldiersOrCrows", "ThreeSoldiersOrCrows", &rows);
    }
    {
        let mut ind = wickra::ThreeStarsInSouth::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ThreeStarsInSouth", "ThreeStarsInSouth", &rows);
    }
    {
        let mut ind = wickra::Thrusting::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Thrusting", "Thrusting", &rows);
    }
    if let Ok(mut ind) = wickra::TimeBasedStop::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TimeBasedStop", "TimeBasedStop", &rows);
    } else {
        eprintln!("gen_golden skip TimeBasedStop");
    }
    {
        let mut ind = wickra::TowerTopBottom::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TowerTopBottom", "TowerTopBottom", &rows);
    }
    if let Ok(mut ind) = wickra::TradeVolumeIndex::new(2.0) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TradeVolumeIndex", "TradeVolumeIndex", &rows);
    } else {
        eprintln!("gen_golden skip TradeVolumeIndex");
    }
    if let Ok(mut ind) = wickra::TrendLabel::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_TrendLabel", "TrendLabel", &rows);
    } else {
        eprintln!("gen_golden skip TrendLabel");
    }
    if let Ok(mut ind) = wickra::TreynorRatio::new(14, 2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_TreynorRatio", "TreynorRatio", &rows);
    } else {
        eprintln!("gen_golden skip TreynorRatio");
    }
    {
        let mut ind = wickra::Triangle::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Triangle", "Triangle", &rows);
    }
    {
        let mut ind = wickra::TripleTopBottom::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TripleTopBottom", "TripleTopBottom", &rows);
    }
    {
        let mut ind = wickra::Tristar::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Tristar", "Tristar", &rows);
    }
    {
        let mut ind = wickra::TrueRange::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TrueRange", "TrueRange", &rows);
    }
    if let Ok(mut ind) = wickra::TsfOscillator::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_TsfOscillator", "TsfOscillator", &rows);
    } else {
        eprintln!("gen_golden skip TsfOscillator");
    }
    {
        let mut ind = wickra::Tweezer::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Tweezer", "Tweezer", &rows);
    }
    if let Ok(mut ind) = wickra::TwiggsMoneyFlow::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TwiggsMoneyFlow", "TwiggsMoneyFlow", &rows);
    } else {
        eprintln!("gen_golden skip TwiggsMoneyFlow");
    }
    {
        let mut ind = wickra::TwoCrows::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TwoCrows", "TwoCrows", &rows);
    }
    {
        let mut ind = wickra::TypicalPrice::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TypicalPrice", "TypicalPrice", &rows);
    }
    if let Ok(mut ind) = wickra::UniversalOscillator::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_UniversalOscillator", "UniversalOscillator", &rows);
    } else {
        eprintln!("gen_golden skip UniversalOscillator");
    }
    if let Ok(mut ind) = wickra::UlcerIndex::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_UlcerIndex", "UlcerIndex", &rows);
    } else {
        eprintln!("gen_golden skip UlcerIndex");
    }
    if let Ok(mut ind) = wickra::UltimateOscillator::new(3, 7, 14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_UltimateOscillator", "UltimateOscillator", &rows);
    } else {
        eprintln!("gen_golden skip UltimateOscillator");
    }
    {
        let mut ind = wickra::UniqueThreeRiver::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_UniqueThreeRiver", "UniqueThreeRiver", &rows);
    }
    {
        let mut ind = wickra::UpsideGapThreeMethods::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(
            dir,
            "g_UpsideGapThreeMethods",
            "UpsideGapThreeMethods",
            &rows,
        );
    }
    {
        let mut ind = wickra::UpsideGapTwoCrows::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_UpsideGapTwoCrows", "UpsideGapTwoCrows", &rows);
    }
    if let Ok(mut ind) = wickra::UpsidePotentialRatio::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_UpsidePotentialRatio", "UpsidePotentialRatio", &rows);
    } else {
        eprintln!("gen_golden skip UpsidePotentialRatio");
    }
    if let Ok(mut ind) = wickra::Vidya::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Vidya", "Vidya", &rows);
    } else {
        eprintln!("gen_golden skip Vidya");
    }
    {
        let mut ind = wickra::Vwap::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Vwap", "Vwap", &rows);
    }
    if let Ok(mut ind) = wickra::Vwma::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Vwma", "Vwma", &rows);
    } else {
        eprintln!("gen_golden skip Vwma");
    }
    if let Ok(mut ind) = wickra::Vzo::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Vzo", "Vzo", &rows);
    } else {
        eprintln!("gen_golden skip Vzo");
    }
    if let Ok(mut ind) = wickra::ValueAtRisk::new(14, 2.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_ValueAtRisk", "ValueAtRisk", &rows);
    } else {
        eprintln!("gen_golden skip ValueAtRisk");
    }
    if let Ok(mut ind) = wickra::Variance::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Variance", "Variance", &rows);
    } else {
        eprintln!("gen_golden skip Variance");
    }
    if let Ok(mut ind) = wickra::VarianceRatio::new(3, 7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_VarianceRatio", "VarianceRatio", &rows);
    } else {
        eprintln!("gen_golden skip VarianceRatio");
    }
    if let Ok(mut ind) = wickra::VerticalHorizontalFilter::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_VerticalHorizontalFilter",
            "VerticalHorizontalFilter",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip VerticalHorizontalFilter");
    }
    if let Ok(mut ind) = wickra::VolatilityOfVolatility::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_VolatilityOfVolatility",
            "VolatilityOfVolatility",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip VolatilityOfVolatility");
    }
    if let Ok(mut ind) = wickra::VolatilityRatio::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_VolatilityRatio", "VolatilityRatio", &rows);
    } else {
        eprintln!("gen_golden skip VolatilityRatio");
    }
    if let Ok(mut ind) = wickra::VoltyStop::new(14, 2.0) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_VoltyStop", "VoltyStop", &rows);
    } else {
        eprintln!("gen_golden skip VoltyStop");
    }
    if let Ok(mut ind) = wickra::VolumeOscillator::new(3, 7) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_VolumeOscillator", "VolumeOscillator", &rows);
    } else {
        eprintln!("gen_golden skip VolumeOscillator");
    }
    if let Ok(mut ind) = wickra::VolumeRsi::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_VolumeRsi", "VolumeRsi", &rows);
    } else {
        eprintln!("gen_golden skip VolumeRsi");
    }
    if let Ok(mut ind) = wickra::WavePm::new(3, 7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_WavePm", "WavePm", &rows);
    } else {
        eprintln!("gen_golden skip WavePm");
    }
    if let Ok(mut ind) = wickra::Wma::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Wma", "Wma", &rows);
    } else {
        eprintln!("gen_golden skip Wma");
    }
    {
        let mut ind = wickra::Wad::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Wad", "Wad", &rows);
    }
    {
        let mut ind = wickra::Wedge::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Wedge", "Wedge", &rows);
    }
    {
        let mut ind = wickra::WeightedClose::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_WeightedClose", "WeightedClose", &rows);
    }
    {
        let mut ind = wickra::WickRatio::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_WickRatio", "WickRatio", &rows);
    }
    if let Ok(mut ind) = wickra::WilliamsR::new(14) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_WilliamsR", "WilliamsR", &rows);
    } else {
        eprintln!("gen_golden skip WilliamsR");
    }
    if let Ok(mut ind) = wickra::WinRate::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_WinRate", "WinRate", &rows);
    } else {
        eprintln!("gen_golden skip WinRate");
    }
    if let Ok(mut ind) = wickra::YoyoExit::new(14, 2.0) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_YoyoExit", "YoyoExit", &rows);
    } else {
        eprintln!("gen_golden skip YoyoExit");
    }
    if let Ok(mut ind) = wickra::Zlema::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Zlema", "Zlema", &rows);
    } else {
        eprintln!("gen_golden skip Zlema");
    }
    if let Ok(mut ind) = wickra::ZScore::new(14) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_ZScore", "ZScore", &rows);
    } else {
        eprintln!("gen_golden skip ZScore");
    }
}

// AUTO-GENERATED multi-output golden tranche.
#[allow(clippy::too_many_lines)]
fn emit_multi(dir: &Path, candles: &[Candle], closes: &[f64]) {
    if let Ok(mut ind) = wickra::Adx::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.plus_di, o.minus_di, o.adx),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Adx", "plus_di,minus_di,adx", &rows);
    } else {
        eprintln!("gen_golden skip Adx");
    }
    if let Ok(mut ind) = wickra::AccelerationBands::new(14, 2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_AccelerationBands", "upper,middle,lower", &rows);
    } else {
        eprintln!("gen_golden skip AccelerationBands");
    }
    if let Ok(mut ind) = wickra::Alligator::new(3, 7, 14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.jaw, o.teeth, o.lips),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Alligator", "jaw,teeth,lips", &rows);
    } else {
        eprintln!("gen_golden skip Alligator");
    }
    if let Ok(mut ind) = wickra::AndrewsPitchfork::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.median, o.upper, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_AndrewsPitchfork", "median,upper,lower", &rows);
    } else {
        eprintln!("gen_golden skip AndrewsPitchfork");
    }
    if let Ok(mut ind) = wickra::Aroon::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.up, o.down),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Aroon", "up,down", &rows);
    } else {
        eprintln!("gen_golden skip Aroon");
    }
    if let Ok(mut ind) = wickra::AtrBands::new(14, 2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_AtrBands", "upper,middle,lower", &rows);
    } else {
        eprintln!("gen_golden skip AtrBands");
    }
    if let Ok(mut ind) = wickra::AtrRatchet::new(14, 2.0, 0.5) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.value, o.direction),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_AtrRatchet", "value,direction", &rows);
    } else {
        eprintln!("gen_golden skip AtrRatchet");
    }
    {
        let mut ind = wickra::AutoFib::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!(
                    "{},{},{},{},{},{},{}",
                    o.level_0,
                    o.level_236,
                    o.level_382,
                    o.level_500,
                    o.level_618,
                    o.level_786,
                    o.level_1000
                ),
                None => "nan,nan,nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_AutoFib",
            "level_0,level_236,level_382,level_500,level_618,level_786,level_1000",
            &rows,
        );
    }
    if let Ok(mut ind) = wickra::BomarBands::new(14, 2.0) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_BomarBands", "upper,middle,lower", &rows);
    } else {
        eprintln!("gen_golden skip BomarBands");
    }
    if let Ok(mut ind) = wickra::CandleVolume::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.body, o.width),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_CandleVolume", "body,width", &rows);
    } else {
        eprintln!("gen_golden skip CandleVolume");
    }
    {
        let mut ind = wickra::CentralPivotRange::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.pivot, o.tc, o.bc),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_CentralPivotRange", "pivot,tc,bc", &rows);
    }
    if let Ok(mut ind) = wickra::ChandeKrollStop::new(3, 2.0, 7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.stop_long, o.stop_short),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_ChandeKrollStop", "stop_long,stop_short", &rows);
    } else {
        eprintln!("gen_golden skip ChandeKrollStop");
    }
    if let Ok(mut ind) = wickra::ChandelierExit::new(14, 2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.long_stop, o.short_stop),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_ChandelierExit", "long_stop,short_stop", &rows);
    } else {
        eprintln!("gen_golden skip ChandelierExit");
    }
    {
        let mut ind = wickra::ClassicPivots::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!(
                    "{},{},{},{},{},{},{}",
                    o.pp, o.r1, o.r2, o.r3, o.s1, o.s2, o.s3
                ),
                None => "nan,nan,nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_ClassicPivots", "pp,r1,r2,r3,s1,s2,s3", &rows);
    }
    if let Ok(mut ind) = wickra::Cointegration::new(3, 7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update((c.close, c.open)) {
                Some(o) => format!("{},{},{}", o.hedge_ratio, o.spread, o.adf_stat),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Cointegration", "hedge_ratio,spread,adf_stat", &rows);
    } else {
        eprintln!("gen_golden skip Cointegration");
    }
    if let Ok(mut ind) = wickra::CompositeProfile::new(3, 7, 2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.poc, o.vah, o.val),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_CompositeProfile", "poc,vah,val", &rows);
    } else {
        eprintln!("gen_golden skip CompositeProfile");
    }
    {
        let mut ind = wickra::DemarkPivots::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.pp, o.r1, o.s1),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_DemarkPivots", "pp,r1,s1", &rows);
    }
    if let Ok(mut ind) = wickra::Donchian::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Donchian", "upper,middle,lower", &rows);
    } else {
        eprintln!("gen_golden skip Donchian");
    }
    if let Ok(mut ind) = wickra::DonchianStop::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.stop_long, o.stop_short),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_DonchianStop", "stop_long,stop_short", &rows);
    } else {
        eprintln!("gen_golden skip DonchianStop");
    }
    if let Ok(mut ind) = wickra::DoubleBollinger::new(14, 2.0, 0.5) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!(
                    "{},{},{},{},{}",
                    o.upper_outer, o.upper_inner, o.middle, o.lower_inner, o.lower_outer
                ),
                None => "nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_DoubleBollinger",
            "upper_outer,upper_inner,middle,lower_inner,lower_outer",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip DoubleBollinger");
    }
    if let Ok(mut ind) = wickra::ElderRay::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.bull_power, o.bear_power),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_ElderRay", "bull_power,bear_power", &rows);
    } else {
        eprintln!("gen_golden skip ElderRay");
    }
    if let Ok(mut ind) = wickra::Equivolume::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.height, o.width),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Equivolume", "height,width", &rows);
    } else {
        eprintln!("gen_golden skip Equivolume");
    }
    {
        let mut ind = wickra::FibArcs::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.arc_382, o.arc_500, o.arc_618),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_FibArcs", "arc_382,arc_500,arc_618", &rows);
    }
    {
        let mut ind = wickra::FibChannel::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!(
                    "{},{},{},{}",
                    o.base, o.level_618, o.level_1000, o.level_1618
                ),
                None => "nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_FibChannel",
            "base,level_618,level_1000,level_1618",
            &rows,
        );
    }
    {
        let mut ind = wickra::FibConfluence::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.price, o.strength),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_FibConfluence", "price,strength", &rows);
    }
    {
        let mut ind = wickra::FibExtension::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!(
                    "{},{},{},{},{}",
                    o.level_1272, o.level_1414, o.level_1618, o.level_2000, o.level_2618
                ),
                None => "nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_FibExtension",
            "level_1272,level_1414,level_1618,level_2000,level_2618",
            &rows,
        );
    }
    {
        let mut ind = wickra::FibFan::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.fan_382, o.fan_500, o.fan_618),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_FibFan", "fan_382,fan_500,fan_618", &rows);
    }
    {
        let mut ind = wickra::FibProjection::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!(
                    "{},{},{},{}",
                    o.level_618, o.level_1000, o.level_1618, o.level_2618
                ),
                None => "nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_FibProjection",
            "level_618,level_1000,level_1618,level_2618",
            &rows,
        );
    }
    {
        let mut ind = wickra::FibRetracement::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!(
                    "{},{},{},{},{},{},{}",
                    o.level_0,
                    o.level_236,
                    o.level_382,
                    o.level_500,
                    o.level_618,
                    o.level_786,
                    o.level_1000
                ),
                None => "nan,nan,nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_FibRetracement",
            "level_0,level_236,level_382,level_500,level_618,level_786,level_1000",
            &rows,
        );
    }
    {
        let mut ind = wickra::FibTimeZones::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.on_zone, o.bars_to_next),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_FibTimeZones", "on_zone,bars_to_next", &rows);
    }
    {
        let mut ind = wickra::FibonacciPivots::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!(
                    "{},{},{},{},{},{},{}",
                    o.pp, o.r1, o.r2, o.r3, o.s1, o.s2, o.s3
                ),
                None => "nan,nan,nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_FibonacciPivots", "pp,r1,r2,r3,s1,s2,s3", &rows);
    }
    if let Ok(mut ind) = wickra::FractalChaosBands::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.upper, o.lower),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_FractalChaosBands", "upper,lower", &rows);
    } else {
        eprintln!("gen_golden skip FractalChaosBands");
    }
    if let Ok(mut ind) = wickra::GatorOscillator::new(3, 7, 14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.upper, o.lower),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_GatorOscillator", "upper,lower", &rows);
    } else {
        eprintln!("gen_golden skip GatorOscillator");
    }
    {
        let mut ind = wickra::GoldenPocket::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.low, o.mid, o.high),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_GoldenPocket", "low,mid,high", &rows);
    }
    {
        let mut ind = wickra::HtPhasor::new();
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!("{},{}", o.inphase, o.quadrature),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_HtPhasor", "inphase,quadrature", &rows);
    }
    {
        let mut ind = wickra::HeikinAshi::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{},{}", o.open, o.high, o.low, o.close),
                None => "nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_HeikinAshi", "open,high,low,close", &rows);
    }
    if let Ok(mut ind) = wickra::HighLowVolumeNodes::new(3, 7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.hvn, o.lvn),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_HighLowVolumeNodes", "hvn,lvn", &rows);
    } else {
        eprintln!("gen_golden skip HighLowVolumeNodes");
    }
    if let Ok(mut ind) = wickra::HurstChannel::new(14, 2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_HurstChannel", "upper,middle,lower", &rows);
    } else {
        eprintln!("gen_golden skip HurstChannel");
    }
    if let Ok(mut ind) = wickra::InitialBalance::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.high, o.low),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_InitialBalance", "high,low", &rows);
    } else {
        eprintln!("gen_golden skip InitialBalance");
    }
    if let Ok(mut ind) = wickra::Kst::new(3, 7, 14, 28, 35, 42, 56, 63, 70) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!("{},{}", o.kst, o.signal),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Kst", "kst,signal", &rows);
    } else {
        eprintln!("gen_golden skip Kst");
    }
    if let Ok(mut ind) = wickra::KalmanHedgeRatio::new(2.0, 0.5) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update((c.close, c.open)) {
                Some(o) => format!("{},{},{}", o.hedge_ratio, o.intercept, o.spread),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_KalmanHedgeRatio",
            "hedge_ratio,intercept,spread",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip KalmanHedgeRatio");
    }
    if let Ok(mut ind) = wickra::KasePermissionStochastic::new(3, 7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.fast, o.slow),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_KasePermissionStochastic", "fast,slow", &rows);
    } else {
        eprintln!("gen_golden skip KasePermissionStochastic");
    }
    if let Ok(mut ind) = wickra::Keltner::new(3, 7, 2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Keltner", "upper,middle,lower", &rows);
    } else {
        eprintln!("gen_golden skip Keltner");
    }
    if let Ok(mut ind) = wickra::Mama::new(2.0, 0.5) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!("{},{}", o.mama, o.fama),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Mama", "mama,fama", &rows);
    } else {
        eprintln!("gen_golden skip Mama");
    }
    if let Ok(mut ind) = wickra::MaEnvelope::new(14, 2.0) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_MaEnvelope", "upper,middle,lower", &rows);
    } else {
        eprintln!("gen_golden skip MaEnvelope");
    }
    if let Ok(mut ind) = wickra::MedianChannel::new(14, 2.0) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_MedianChannel", "upper,middle,lower", &rows);
    } else {
        eprintln!("gen_golden skip MedianChannel");
    }
    if let Ok(mut ind) = wickra::ModifiedMaStop::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.value, o.direction),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_ModifiedMaStop", "value,direction", &rows);
    } else {
        eprintln!("gen_golden skip ModifiedMaStop");
    }
    if let Ok(mut ind) = wickra::MurreyMathLines::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!(
                    "{},{},{},{},{},{},{},{},{}",
                    o.mm8_8, o.mm7_8, o.mm6_8, o.mm5_8, o.mm4_8, o.mm3_8, o.mm2_8, o.mm1_8, o.mm0_8
                ),
                None => "nan,nan,nan,nan,nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_MurreyMathLines",
            "mm8_8,mm7_8,mm6_8,mm5_8,mm4_8,mm3_8,mm2_8,mm1_8,mm0_8",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip MurreyMathLines");
    }
    if let Ok(mut ind) = wickra::Nrtr::new(2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.value, o.direction),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Nrtr", "value,direction", &rows);
    } else {
        eprintln!("gen_golden skip Nrtr");
    }
    if let Ok(mut ind) = wickra::OpeningRange::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.high, o.low, o.breakout_distance),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_OpeningRange", "high,low,breakout_distance", &rows);
    } else {
        eprintln!("gen_golden skip OpeningRange");
    }
    {
        let mut ind = wickra::OvernightIntradayReturn::new(14);
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.overnight, o.intraday),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_OvernightIntradayReturn",
            "overnight,intraday",
            &rows,
        );
    }
    if let Ok(mut ind) = wickra::ProjectionBands::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_ProjectionBands", "upper,middle,lower", &rows);
    } else {
        eprintln!("gen_golden skip ProjectionBands");
    }
    if let Ok(mut ind) = wickra::Qqe::new(3, 7, 2.0) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!("{},{}", o.rsi_ma, o.trailing_line),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Qqe", "rsi_ma,trailing_line", &rows);
    } else {
        eprintln!("gen_golden skip Qqe");
    }
    if let Ok(mut ind) = wickra::QuartileBands::new(14) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_QuartileBands", "upper,middle,lower", &rows);
    } else {
        eprintln!("gen_golden skip QuartileBands");
    }
    if let Ok(mut ind) = wickra::Rwi::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.high, o.low),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Rwi", "high,low", &rows);
    } else {
        eprintln!("gen_golden skip Rwi");
    }
    {
        let mut ind = wickra::SessionHighLow::new(14);
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.high, o.low),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_SessionHighLow", "high,low", &rows);
    }
    {
        let mut ind = wickra::SessionRange::new(14);
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.asia, o.eu, o.us),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_SessionRange", "asia,eu,us", &rows);
    }
    if let Ok(mut ind) = wickra::SmoothedHeikinAshi::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{},{}", o.open, o.high, o.low, o.close),
                None => "nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_SmoothedHeikinAshi", "open,high,low,close", &rows);
    } else {
        eprintln!("gen_golden skip SmoothedHeikinAshi");
    }
    if let Ok(mut ind) = wickra::SpreadBollingerBands::new(14, 2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update((c.close, c.open)) {
                Some(o) => format!("{},{},{},{}", o.middle, o.upper, o.lower, o.percent_b),
                None => "nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_SpreadBollingerBands",
            "middle,upper,lower,percent_b",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip SpreadBollingerBands");
    }
    if let Ok(mut ind) = wickra::StandardErrorBands::new(14, 2.0) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_StandardErrorBands", "upper,middle,lower", &rows);
    } else {
        eprintln!("gen_golden skip StandardErrorBands");
    }
    if let Ok(mut ind) = wickra::StarcBands::new(3, 7, 2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_StarcBands", "upper,middle,lower", &rows);
    } else {
        eprintln!("gen_golden skip StarcBands");
    }
    if let Ok(mut ind) = wickra::Stochastic::new(3, 7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.k, o.d),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Stochastic", "k,d", &rows);
    } else {
        eprintln!("gen_golden skip Stochastic");
    }
    if let Ok(mut ind) = wickra::SuperTrend::new(14, 2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.value, o.direction),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_SuperTrend", "value,direction", &rows);
    } else {
        eprintln!("gen_golden skip SuperTrend");
    }
    if let Ok(mut ind) = wickra::TdLines::new(3, 7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.resistance, o.support),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_TdLines", "resistance,support", &rows);
    } else {
        eprintln!("gen_golden skip TdLines");
    }
    if let Ok(mut ind) = wickra::TdMovingAverage::new(3, 7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.st1, o.st2),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_TdMovingAverage", "st1,st2", &rows);
    } else {
        eprintln!("gen_golden skip TdMovingAverage");
    }
    {
        let mut ind = wickra::TdRangeProjection::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.high, o.low),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_TdRangeProjection", "high,low", &rows);
    }
    if let Ok(mut ind) = wickra::TdRiskLevel::new(3, 7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.buy_risk, o.sell_risk),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_TdRiskLevel", "buy_risk,sell_risk", &rows);
    } else {
        eprintln!("gen_golden skip TdRiskLevel");
    }
    if let Ok(mut ind) = wickra::TdSequential::new(3, 7, 14, 28) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.setup, o.countdown, o.direction),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_TdSequential", "setup,countdown,direction", &rows);
    } else {
        eprintln!("gen_golden skip TdSequential");
    }
    if let Ok(mut ind) = wickra::TtmSqueeze::new(14, 2.0, 0.5) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.squeeze, o.momentum),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_TtmSqueeze", "squeeze,momentum", &rows);
    } else {
        eprintln!("gen_golden skip TtmSqueeze");
    }
    if let Ok(mut ind) = wickra::ValueArea::new(3, 7, 2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.poc, o.vah, o.val),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_ValueArea", "poc,vah,val", &rows);
    } else {
        eprintln!("gen_golden skip ValueArea");
    }
    if let Ok(mut ind) = wickra::VolatilityCone::new(3, 7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!(
                    "{},{},{},{},{}",
                    o.current, o.min, o.median, o.max, o.percentile
                ),
                None => "nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_VolatilityCone",
            "current,min,median,max,percentile",
            &rows,
        );
    } else {
        eprintln!("gen_golden skip VolatilityCone");
    }
    if let Ok(mut ind) = wickra::VolumeWeightedMacd::new(3, 7, 14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.macd, o.signal, o.histogram),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_VolumeWeightedMacd", "macd,signal,histogram", &rows);
    } else {
        eprintln!("gen_golden skip VolumeWeightedMacd");
    }
    if let Ok(mut ind) = wickra::VolumeWeightedSr::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.support, o.resistance),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_VolumeWeightedSr", "support,resistance", &rows);
    } else {
        eprintln!("gen_golden skip VolumeWeightedSr");
    }
    if let Ok(mut ind) = wickra::Vortex::new(14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.plus, o.minus),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Vortex", "plus,minus", &rows);
    } else {
        eprintln!("gen_golden skip Vortex");
    }
    if let Ok(mut ind) = wickra::WaveTrend::new(3, 7, 14) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.wt1, o.wt2),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_WaveTrend", "wt1,wt2", &rows);
    } else {
        eprintln!("gen_golden skip WaveTrend");
    }
    {
        let mut ind = wickra::WoodiePivots::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{},{},{}", o.pp, o.r1, o.r2, o.s1, o.s2),
                None => "nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_WoodiePivots", "pp,r1,r2,s1,s2", &rows);
    }
    if let Ok(mut ind) = wickra::ZeroLagMacd::new(3, 7, 14) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!("{},{},{}", o.macd, o.signal, o.histogram),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_ZeroLagMacd", "macd,signal,histogram", &rows);
    } else {
        eprintln!("gen_golden skip ZeroLagMacd");
    }
    if let Ok(mut ind) = wickra::ZigZag::new(2.0) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.swing, o.direction),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_ZigZag", "swing,direction", &rows);
    } else {
        eprintln!("gen_golden skip ZigZag");
    }
}

// AUTO-GENERATED constraint-tuned tranche.
#[allow(clippy::too_many_lines)]
fn emit_skips(dir: &Path, candles: &[Candle], closes: &[f64]) {
    if let Ok(mut ind) = wickra::Alma::new(9, 0.85, 6.0) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Alma", "Alma", &rows);
    } else {
        eprintln!("skip2 Alma");
    }
    if let Ok(mut ind) = wickra::AutocorrelationPeriodogram::new(10, 48) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_AutocorrelationPeriodogram",
            "AutocorrelationPeriodogram",
            &rows,
        );
    } else {
        eprintln!("skip2 AutocorrelationPeriodogram");
    }
    if let Ok(mut ind) = wickra::Autocorrelation::new(10, 1) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Autocorrelation", "Autocorrelation", &rows);
    } else {
        eprintln!("skip2 Autocorrelation");
    }
    if let Ok(mut ind) = wickra::BandpassFilter::new(20, 0.3) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_BandpassFilter", "BandpassFilter", &rows);
    } else {
        eprintln!("skip2 BandpassFilter");
    }
    if let Ok(mut ind) = wickra::ConditionalValueAtRisk::new(20, 0.95) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_ConditionalValueAtRisk",
            "ConditionalValueAtRisk",
            &rows,
        );
    } else {
        eprintln!("skip2 ConditionalValueAtRisk");
    }
    if let Ok(mut ind) = wickra::EmpiricalModeDecomposition::new(20, 0.1) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(
            dir,
            "g_EmpiricalModeDecomposition",
            "EmpiricalModeDecomposition",
            &rows,
        );
    } else {
        eprintln!("skip2 EmpiricalModeDecomposition");
    }
    if let Ok(mut ind) = wickra::EwmaVolatility::new(0.94) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_EwmaVolatility", "EwmaVolatility", &rows);
    } else {
        eprintln!("skip2 EwmaVolatility");
    }
    if let Ok(mut ind) = wickra::Fama::new(0.5, 0.05) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Fama", "Fama", &rows);
    } else {
        eprintln!("skip2 Fama");
    }
    if let Ok(mut ind) = wickra::GeneralizedDema::new(5, 0.7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_GeneralizedDema", "GeneralizedDema", &rows);
    } else {
        eprintln!("skip2 GeneralizedDema");
    }
    if let Ok(mut ind) = wickra::Garch11::new(2e-06, 0.1, 0.88) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Garch11", "Garch11", &rows);
    } else {
        eprintln!("skip2 Garch11");
    }
    if let Ok(mut ind) = wickra::GrangerCausality::new(60, 1) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_GrangerCausality", "GrangerCausality", &rows);
    } else {
        eprintln!("skip2 GrangerCausality");
    }
    if let Ok(mut ind) = wickra::HoltWinters::new(0.5, 0.1) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_HoltWinters", "HoltWinters", &rows);
    } else {
        eprintln!("skip2 HoltWinters");
    }
    if let Ok(mut ind) = wickra::HurstExponent::new(100, 4) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_HurstExponent", "HurstExponent", &rows);
    } else {
        eprintln!("skip2 HurstExponent");
    }
    if let Ok(mut ind) = wickra::Jma::new(7, 0.0, 2) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Jma", "Jma", &rows);
    } else {
        eprintln!("skip2 Jma");
    }
    if let Ok(mut ind) = wickra::LaguerreRsi::new(0.5) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_LaguerreRsi", "LaguerreRsi", &rows);
    } else {
        eprintln!("skip2 LaguerreRsi");
    }
    if let Ok(mut ind) = wickra::Psar::new(0.02, 0.02, 0.2) {
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_Psar", "Psar", &rows);
    } else {
        eprintln!("skip2 Psar");
    }
    if let Ok(mut ind) = wickra::RollingQuantile::new(20, 0.5) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_RollingQuantile", "RollingQuantile", &rows);
    } else {
        eprintln!("skip2 RollingQuantile");
    }
    if let Ok(mut ind) = wickra::SampleEntropy::new(20, 2, 0.2) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_SampleEntropy", "SampleEntropy", &rows);
    } else {
        eprintln!("skip2 SampleEntropy");
    }
    if let Ok(mut ind) = wickra::Stc::new(10, 23, 10, 0.5) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_Stc", "Stc", &rows);
    } else {
        eprintln!("skip2 Stc");
    }
    if let Ok(mut ind) = wickra::T3::new(5, 0.7) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_T3", "T3", &rows);
    } else {
        eprintln!("skip2 T3");
    }
    if let Ok(mut ind) = wickra::ValueAtRisk::new(20, 0.95) {
        let rows: Vec<String> = closes.iter().map(|&x| cell(ind.update(x))).collect();
        write_csv(dir, "g_ValueAtRisk", "ValueAtRisk", &rows);
    } else {
        eprintln!("skip2 ValueAtRisk");
    }
    if let Ok(mut ind) = wickra::VarianceRatio::new(60, 2) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_VarianceRatio", "VarianceRatio", &rows);
    } else {
        eprintln!("skip2 VarianceRatio");
    }
    if let Ok(mut ind) = wickra::Mama::new(0.5, 0.05) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!("{},{}", o.mama, o.fama),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Mama", "mama,fama", &rows);
    } else {
        eprintln!("skip2 Mama");
    }
    if let Ok(mut ind) = wickra::DoubleBollinger::new(20, 1.0, 2.0) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!(
                    "{},{},{},{},{}",
                    o.upper_outer, o.upper_inner, o.middle, o.lower_inner, o.lower_outer
                ),
                None => "nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_DoubleBollinger",
            "upper_outer,upper_inner,middle,lower_inner,lower_outer",
            &rows,
        );
    } else {
        eprintln!("skip2 DoubleBollinger");
    }
    if let Ok(mut ind) = wickra::BomarBands::new(4, 0.85) {
        let rows: Vec<String> = closes
            .iter()
            .map(|&x| match ind.update(x) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_BomarBands", "upper,middle,lower", &rows);
    } else {
        eprintln!("skip2 BomarBands");
    }
    if let Ok(mut ind) = wickra::Cointegration::new(40, 1) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update((c.close, c.open)) {
                Some(o) => format!("{},{},{}", o.hedge_ratio, o.spread, o.adf_stat),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Cointegration", "hedge_ratio,spread,adf_stat", &rows);
    } else {
        eprintln!("skip2 Cointegration");
    }
    if let Ok(mut ind) = wickra::CompositeProfile::new(20, 24, 0.7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.poc, o.vah, o.val),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_CompositeProfile", "poc,vah,val", &rows);
    } else {
        eprintln!("skip2 CompositeProfile");
    }
    if let Ok(mut ind) = wickra::KalmanHedgeRatio::new(0.01, 0.001) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update((c.close, c.open)) {
                Some(o) => format!("{},{},{}", o.hedge_ratio, o.intercept, o.spread),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_KalmanHedgeRatio",
            "hedge_ratio,intercept,spread",
            &rows,
        );
    } else {
        eprintln!("skip2 KalmanHedgeRatio");
    }
    if let Ok(mut ind) = wickra::ValueArea::new(20, 50, 0.7) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.poc, o.vah, o.val),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_ValueArea", "poc,vah,val", &rows);
    } else {
        eprintln!("skip2 ValueArea");
    }
    if let Ok(mut ind) = wickra::ZigZag::new(0.02) {
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.swing, o.direction),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_ZigZag", "swing,direction", &rows);
    } else {
        eprintln!("skip2 ZigZag");
    }
}

// AUTO-GENERATED missed scalar/multi tranche (single + multi f64 output).
#[allow(clippy::too_many_lines)]
fn emit_missed(dir: &Path, candles: &[Candle], closes: &[f64]) {
    {
        let mut ind = wickra::AdaptiveLaguerreFilter::new(20).unwrap();
        let rows: Vec<String> = closes.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(
            dir,
            "g_AdaptiveLaguerreFilter",
            "AdaptiveLaguerreFilter",
            &rows,
        );
    }
    {
        let mut ind = wickra::EvenBetterSinewave::new(40, 10).unwrap();
        let rows: Vec<String> = closes.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_EvenBetterSinewave", "EvenBetterSinewave", &rows);
    }
    {
        let mut ind = wickra::HtDcPhase::new();
        let rows: Vec<String> = closes.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_HtDcPhase", "HtDcPhase", &rows);
    }
    {
        let mut ind = wickra::HtTrendMode::new();
        let rows: Vec<String> = closes.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_HtTrendMode", "HtTrendMode", &rows);
    }
    {
        let mut ind = wickra::LinearRegression::new(14).unwrap();
        let rows: Vec<String> = closes.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_LinearRegression", "LinearRegression", &rows);
    }
    {
        let mut ind = wickra::LinRegAngle::new(14).unwrap();
        let rows: Vec<String> = closes.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_LinRegAngle", "LinRegAngle", &rows);
    }
    {
        let mut ind = wickra::LinRegIntercept::new(14).unwrap();
        let rows: Vec<String> = closes.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_LinRegIntercept", "LinRegIntercept", &rows);
    }
    {
        let mut ind = wickra::LinRegSlope::new(14).unwrap();
        let rows: Vec<String> = closes.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_LinRegSlope", "LinRegSlope", &rows);
    }
    {
        let mut ind = wickra::McGinleyDynamic::new(14).unwrap();
        let rows: Vec<String> = closes.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_McGinleyDynamic", "McGinleyDynamic", &rows);
    }
    {
        let mut ind = wickra::PolarizedFractalEfficiency::new(10, 5).unwrap();
        let rows: Vec<String> = closes.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(
            dir,
            "g_PolarizedFractalEfficiency",
            "PolarizedFractalEfficiency",
            &rows,
        );
    }
    {
        let mut ind = wickra::TrendStrengthIndex::new(14).unwrap();
        let rows: Vec<String> = closes.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TrendStrengthIndex", "TrendStrengthIndex", &rows);
    }
    {
        let mut ind = wickra::PairSpreadZScore::new(20, 20).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| cell(ind.update((c.close, c.open))))
            .collect();
        write_csv(dir, "g_PairSpreadZScore", "PairSpreadZScore", &rows);
    }
    {
        let mut ind = wickra::AverageDailyRange::new(14, 0).unwrap();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_AverageDailyRange", "AverageDailyRange", &rows);
    }
    {
        let mut ind = wickra::ChaikinMoneyFlow::new(20).unwrap();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ChaikinMoneyFlow", "ChaikinMoneyFlow", &rows);
    }
    {
        let mut ind = wickra::GarmanKlassVolatility::new(20, 252).unwrap();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(
            dir,
            "g_GarmanKlassVolatility",
            "GarmanKlassVolatility",
            &rows,
        );
    }
    {
        let mut ind = wickra::HiLoActivator::new(14).unwrap();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_HiLoActivator", "HiLoActivator", &rows);
    }
    {
        let mut ind = wickra::OvernightGap::new(0);
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_OvernightGap", "OvernightGap", &rows);
    }
    {
        let mut ind = wickra::ParkinsonVolatility::new(20, 252).unwrap();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_ParkinsonVolatility", "ParkinsonVolatility", &rows);
    }
    {
        let mut ind = wickra::RogersSatchellVolatility::new(20, 252).unwrap();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(
            dir,
            "g_RogersSatchellVolatility",
            "RogersSatchellVolatility",
            &rows,
        );
    }
    {
        let mut ind = wickra::TdDeMarker::new(14).unwrap();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdDeMarker", "TdDeMarker", &rows);
    }
    {
        let mut ind = wickra::TdDWave::new(2).unwrap();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TdDWave", "TdDWave", &rows);
    }
    {
        let mut ind = wickra::TurnOfMonth::new(3, 3, 0).unwrap();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_TurnOfMonth", "TurnOfMonth", &rows);
    }
    {
        let mut ind = wickra::VolumePriceTrend::new();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_VolumePriceTrend", "VolumePriceTrend", &rows);
    }
    {
        let mut ind = wickra::RollingVwap::new(14).unwrap();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_RollingVwap", "RollingVwap", &rows);
    }
    {
        let mut ind = wickra::YangZhangVolatility::new(20, 252).unwrap();
        let rows: Vec<String> = candles.iter().map(|&c| cell(ind.update(c))).collect();
        write_csv(dir, "g_YangZhangVolatility", "YangZhangVolatility", &rows);
    }
    {
        let mut ind = wickra::DrawdownDuration::new();
        let rows: Vec<String> = closes
            .iter()
            .map(|&c| cell(ind.update(c).map(f64::from)))
            .collect();
        write_csv(dir, "g_DrawdownDuration", "DrawdownDuration", &rows);
    }
    {
        let mut ind = wickra::BollingerBands::new(20, 2.0).unwrap();
        let rows: Vec<String> = closes
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{},{}", o.upper, o.middle, o.lower, o.stddev),
                None => "nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_BollingerBands", "upper,middle,lower,stddev", &rows);
    }
    {
        let mut ind = wickra::LinRegChannel::new(14, 2.0).unwrap();
        let rows: Vec<String> = closes
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.upper, o.middle, o.lower),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_LinRegChannel", "upper,middle,lower", &rows);
    }
    {
        let mut ind = wickra::MacdIndicator::new(12, 26, 9).unwrap();
        let rows: Vec<String> = closes
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.macd, o.signal, o.histogram),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_MacdIndicator", "macd,signal,histogram", &rows);
    }
    {
        let mut ind = wickra::MacdExt::new(
            12,
            wickra::MaType::Sma,
            26,
            wickra::MaType::Sma,
            9,
            wickra::MaType::Sma,
        )
        .unwrap();
        let rows: Vec<String> = closes
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.macd, o.signal, o.histogram),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_MacdExt", "macd,signal,histogram", &rows);
    }
    {
        let mut ind = wickra::MacdFix::new(9).unwrap();
        let rows: Vec<String> = closes
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{}", o.macd, o.signal, o.histogram),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_MacdFix", "macd,signal,histogram", &rows);
    }
    {
        let mut ind = wickra::RelativeStrengthAB::new(14, 14).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update((c.close, c.open)) {
                Some(o) => format!("{},{},{}", o.ratio, o.ratio_ma, o.ratio_rsi),
                None => "nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_RelativeStrengthAB",
            "ratio,ratio_ma,ratio_rsi",
            &rows,
        );
    }
    {
        let mut ind = wickra::Camarilla::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!(
                    "{},{},{},{},{},{},{},{},{}",
                    o.pp, o.r1, o.r2, o.r3, o.r4, o.s1, o.s2, o.s3, o.s4
                ),
                None => "nan,nan,nan,nan,nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_Camarilla", "pp,r1,r2,r3,r4,s1,s2,s3,s4", &rows);
    }
    {
        let mut ind = wickra::ElderSafeZone::new(10, 2.0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.value, o.direction),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_ElderSafeZone", "value,direction", &rows);
    }
    {
        let mut ind = wickra::KaseDevStop::new(14, 2.0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", o.value, o.direction),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_KaseDevStop", "value,direction", &rows);
    }
    {
        let mut ind = wickra::VwapStdDevBands::new(2.0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{},{},{}", o.upper, o.middle, o.lower, o.stddev),
                None => "nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_VwapStdDevBands", "upper,middle,lower,stddev", &rows);
    }
}

// AUTO-GENERATED exotic-input tranche (DerivativesTick / CrossSection / Trade / TradeQuote / OrderBook).
#[allow(clippy::too_many_lines)]
fn emit_exotic(dir: &Path, candles: &[Candle]) {
    use wickra::{
        CrossSection, DerivativesTick, Level, Member, OrderBook, Side, Trade, TradeQuote,
    };
    let ticks: Vec<DerivativesTick> = candles
        .iter()
        .map(|c| {
            let funding_rate = (c.close - c.open) / c.close * 0.01;
            DerivativesTick::new(
                funding_rate,
                c.close,
                c.close - 0.5,
                c.close + 1.0,
                c.volume * 10.0,
                c.volume * 0.6,
                c.volume * 0.4,
                c.volume * 0.55,
                c.volume * 0.45,
                c.high - c.close,
                c.close - c.low,
                c.timestamp,
            )
            .unwrap()
        })
        .collect();
    let sections: Vec<CrossSection> = candles
        .iter()
        .map(|c| {
            let members: Vec<Member> = (0..5)
                .map(|j| {
                    let jf = f64::from(j);
                    Member::with_signals(
                        (c.close - c.open) + jf,
                        c.volume + jf * 10.0,
                        j % 2 == 0,
                        j % 3 == 0,
                        j % 2 == 0,
                        j % 3 == 0,
                    )
                })
                .collect();
            CrossSection::new(members, c.timestamp).unwrap()
        })
        .collect();
    let trades: Vec<Trade> = candles
        .iter()
        .map(|c| {
            let side = if c.close >= c.open {
                Side::Buy
            } else {
                Side::Sell
            };
            Trade::new(c.close, c.volume, side, c.timestamp).unwrap()
        })
        .collect();
    let quotes: Vec<TradeQuote> = candles
        .iter()
        .map(|c| {
            let side = if c.close >= c.open {
                Side::Buy
            } else {
                Side::Sell
            };
            let trade = Trade::new(c.close, c.volume, side, c.timestamp).unwrap();
            TradeQuote::new(trade, c.high.midpoint(c.low)).unwrap()
        })
        .collect();
    let books: Vec<OrderBook> = candles
        .iter()
        .map(|c| {
            let bids: Vec<Level> = (0..5)
                .map(|k| {
                    let kf = f64::from(k + 1);
                    Level::new(c.close - 0.1 * kf, c.volume / kf).unwrap()
                })
                .collect();
            let asks: Vec<Level> = (0..5)
                .map(|k| {
                    let kf = f64::from(k + 1);
                    Level::new(c.close + 0.1 * kf, c.volume * 0.9 / kf).unwrap()
                })
                .collect();
            OrderBook::new(bids, asks).unwrap()
        })
        .collect();
    {
        let mut ind = wickra::CalendarSpread::new();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_CalendarSpread", "CalendarSpread", &rows);
    }
    {
        let mut ind = wickra::FundingBasis::new();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_FundingBasis", "FundingBasis", &rows);
    }
    {
        let mut ind = wickra::FundingImpliedApr::new(1095.0).unwrap();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_FundingImpliedApr", "FundingImpliedApr", &rows);
    }
    {
        let mut ind = wickra::FundingRate::new();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_FundingRate", "FundingRate", &rows);
    }
    {
        let mut ind = wickra::FundingRateMean::new(20).unwrap();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_FundingRateMean", "FundingRateMean", &rows);
    }
    {
        let mut ind = wickra::FundingRateZScore::new(20).unwrap();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_FundingRateZScore", "FundingRateZScore", &rows);
    }
    {
        let mut ind = wickra::LongShortRatio::new();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_LongShortRatio", "LongShortRatio", &rows);
    }
    {
        let mut ind = wickra::OpenInterestDelta::new();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_OpenInterestDelta", "OpenInterestDelta", &rows);
    }
    {
        let mut ind = wickra::OIPriceDivergence::new(20).unwrap();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_OIPriceDivergence", "OIPriceDivergence", &rows);
    }
    {
        let mut ind = wickra::OiToVolumeRatio::new();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_OiToVolumeRatio", "OiToVolumeRatio", &rows);
    }
    {
        let mut ind = wickra::OIWeighted::new();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_OIWeighted", "OIWeighted", &rows);
    }
    {
        let mut ind = wickra::OpenInterestMomentum::new(10).unwrap();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_OpenInterestMomentum", "OpenInterestMomentum", &rows);
    }
    {
        let mut ind = wickra::PerpetualPremiumIndex::new();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(
            dir,
            "g_PerpetualPremiumIndex",
            "PerpetualPremiumIndex",
            &rows,
        );
    }
    {
        let mut ind = wickra::TakerBuySellRatio::new();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_TakerBuySellRatio", "TakerBuySellRatio", &rows);
    }
    {
        let mut ind = wickra::TermStructureBasis::new();
        let rows: Vec<String> = ticks.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_TermStructureBasis", "TermStructureBasis", &rows);
    }
    {
        let mut ind = wickra::LiquidationFeatures::new();
        let rows: Vec<String> = ticks
            .iter()
            .map(|&t| match ind.update(t) {
                Some(o) => format!(
                    "{},{},{},{},{}",
                    o.long, o.short, o.net, o.total, o.imbalance
                ),
                None => "nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_LiquidationFeatures",
            "long,short,net,total,imbalance",
            &rows,
        );
    }
    {
        let mut ind = wickra::AbsoluteBreadthIndex::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_AbsoluteBreadthIndex", "AbsoluteBreadthIndex", &rows);
    }
    {
        let mut ind = wickra::AdvanceDecline::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_AdvanceDecline", "AdvanceDecline", &rows);
    }
    {
        let mut ind = wickra::AdvanceDeclineRatio::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_AdvanceDeclineRatio", "AdvanceDeclineRatio", &rows);
    }
    {
        let mut ind = wickra::AdVolumeLine::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_AdVolumeLine", "AdVolumeLine", &rows);
    }
    {
        let mut ind = wickra::BreadthThrust::new(10).unwrap();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_BreadthThrust", "BreadthThrust", &rows);
    }
    {
        let mut ind = wickra::BullishPercentIndex::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_BullishPercentIndex", "BullishPercentIndex", &rows);
    }
    {
        let mut ind = wickra::CumulativeVolumeIndex::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(
            dir,
            "g_CumulativeVolumeIndex",
            "CumulativeVolumeIndex",
            &rows,
        );
    }
    {
        let mut ind = wickra::HighLowIndex::new(10).unwrap();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_HighLowIndex", "HighLowIndex", &rows);
    }
    {
        let mut ind = wickra::McClellanOscillator::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_McClellanOscillator", "McClellanOscillator", &rows);
    }
    {
        let mut ind = wickra::McClellanSummationIndex::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(
            dir,
            "g_McClellanSummationIndex",
            "McClellanSummationIndex",
            &rows,
        );
    }
    {
        let mut ind = wickra::NewHighsNewLows::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_NewHighsNewLows", "NewHighsNewLows", &rows);
    }
    {
        let mut ind = wickra::PercentAboveMa::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_PercentAboveMa", "PercentAboveMa", &rows);
    }
    {
        let mut ind = wickra::TickIndex::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_TickIndex", "TickIndex", &rows);
    }
    {
        let mut ind = wickra::Trin::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_Trin", "Trin", &rows);
    }
    {
        let mut ind = wickra::UpDownVolumeRatio::new();
        let rows: Vec<String> = sections
            .iter()
            .map(|s| cell(ind.update(s.clone())))
            .collect();
        write_csv(dir, "g_UpDownVolumeRatio", "UpDownVolumeRatio", &rows);
    }
    {
        let mut ind = wickra::AmihudIlliquidity::new(20).unwrap();
        let rows: Vec<String> = trades.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_AmihudIlliquidity", "AmihudIlliquidity", &rows);
    }
    {
        let mut ind = wickra::CumulativeVolumeDelta::new();
        let rows: Vec<String> = trades.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(
            dir,
            "g_CumulativeVolumeDelta",
            "CumulativeVolumeDelta",
            &rows,
        );
    }
    {
        let mut ind = wickra::Pin::new(20).unwrap();
        let rows: Vec<String> = trades.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_Pin", "Pin", &rows);
    }
    {
        let mut ind = wickra::RollMeasure::new(20).unwrap();
        let rows: Vec<String> = trades.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_RollMeasure", "RollMeasure", &rows);
    }
    {
        let mut ind = wickra::SignedVolume::new();
        let rows: Vec<String> = trades.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_SignedVolume", "SignedVolume", &rows);
    }
    {
        let mut ind = wickra::TradeImbalance::new(20).unwrap();
        let rows: Vec<String> = trades.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_TradeImbalance", "TradeImbalance", &rows);
    }
    {
        let mut ind = wickra::TradeSignAutocorrelation::new(20).unwrap();
        let rows: Vec<String> = trades.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(
            dir,
            "g_TradeSignAutocorrelation",
            "TradeSignAutocorrelation",
            &rows,
        );
    }
    {
        let mut ind = wickra::Vpin::new(5000.0, 10).unwrap();
        let rows: Vec<String> = trades.iter().map(|&t| cell(ind.update(t))).collect();
        write_csv(dir, "g_Vpin", "Vpin", &rows);
    }
    {
        let mut ind = wickra::KylesLambda::new(20).unwrap();
        let rows: Vec<String> = quotes.iter().map(|&q| cell(ind.update(q))).collect();
        write_csv(dir, "g_KylesLambda", "KylesLambda", &rows);
    }
    {
        let mut ind = wickra::RealizedSpread::new(20).unwrap();
        let rows: Vec<String> = quotes.iter().map(|&q| cell(ind.update(q))).collect();
        write_csv(dir, "g_RealizedSpread", "RealizedSpread", &rows);
    }
    {
        let mut ind = wickra::EffectiveSpread::new();
        let rows: Vec<String> = quotes.iter().map(|&q| cell(ind.update(q))).collect();
        write_csv(dir, "g_EffectiveSpread", "EffectiveSpread", &rows);
    }
    {
        let mut ind = wickra::DepthSlope::new();
        let rows: Vec<String> = books.iter().map(|b| cell(ind.update(b.clone()))).collect();
        write_csv(dir, "g_DepthSlope", "DepthSlope", &rows);
    }
    {
        let mut ind = wickra::Microprice::new();
        let rows: Vec<String> = books.iter().map(|b| cell(ind.update(b.clone()))).collect();
        write_csv(dir, "g_Microprice", "Microprice", &rows);
    }
    {
        let mut ind = wickra::OrderBookImbalanceFull::new();
        let rows: Vec<String> = books.iter().map(|b| cell(ind.update(b.clone()))).collect();
        write_csv(
            dir,
            "g_OrderBookImbalanceFull",
            "OrderBookImbalanceFull",
            &rows,
        );
    }
    {
        let mut ind = wickra::OrderBookImbalanceTop1::new();
        let rows: Vec<String> = books.iter().map(|b| cell(ind.update(b.clone()))).collect();
        write_csv(
            dir,
            "g_OrderBookImbalanceTop1",
            "OrderBookImbalanceTop1",
            &rows,
        );
    }
    {
        let mut ind = wickra::OrderBookImbalanceTopN::new(5).unwrap();
        let rows: Vec<String> = books.iter().map(|b| cell(ind.update(b.clone()))).collect();
        write_csv(
            dir,
            "g_OrderBookImbalanceTopN",
            "OrderBookImbalanceTopN",
            &rows,
        );
    }
    {
        let mut ind = wickra::OrderFlowImbalance::new(20).unwrap();
        let rows: Vec<String> = books.iter().map(|b| cell(ind.update(b.clone()))).collect();
        write_csv(dir, "g_OrderFlowImbalance", "OrderFlowImbalance", &rows);
    }
    {
        let mut ind = wickra::QuotedSpread::new();
        let rows: Vec<String> = books.iter().map(|b| cell(ind.update(b.clone()))).collect();
        write_csv(dir, "g_QuotedSpread", "QuotedSpread", &rows);
    }
}

// AUTO-GENERATED special multi-output tranche (Option / integer fields).
fn emit_special(dir: &Path, candles: &[Candle]) {
    {
        let mut ind = wickra::Ichimoku::new(9, 26, 52, 26).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!(
                    "{},{},{},{},{}",
                    cell(o.tenkan),
                    cell(o.kijun),
                    cell(o.senkou_a),
                    cell(o.senkou_b),
                    cell(o.chikou)
                ),
                None => "nan,nan,nan,nan,nan".to_owned(),
            })
            .collect();
        write_csv(
            dir,
            "g_Ichimoku",
            "tenkan,kijun,senkou_a,senkou_b,chikou",
            &rows,
        );
    }
    {
        let mut ind = wickra::WilliamsFractals::new();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update(c) {
                Some(o) => format!("{},{}", cell(o.up), cell(o.down)),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_WilliamsFractals", "up,down", &rows);
    }
    {
        let mut ind = wickra::LeadLagCrossCorrelation::new(20, 10).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| match ind.update((c.close, c.open)) {
                Some(o) => format!("{},{}", o.lag, o.correlation),
                None => "nan,nan".to_owned(),
            })
            .collect();
        write_csv(dir, "g_LeadLagCrossCorrelation", "lag,correlation", &rows);
    }
}

// AUTO-GENERATED profile tranche (variable-length histogram output, fixed width
// per configuration). The CSV header is a single placeholder token; each data
// row holds the flattened output (`bins` for time/volume profiles, prefixed by
// `price_low,price_high` for the price-binned TPO/volume profiles).
fn emit_profiles(dir: &Path, candles: &[Candle]) {
    fn bins_row(bins: &[f64]) -> String {
        bins.iter()
            .map(|b| format!("{b}"))
            .collect::<Vec<_>>()
            .join(",")
    }
    fn price_bins_row(price_low: f64, price_high: f64, bins: &[f64]) -> String {
        format!("{price_low},{price_high},{}", bins_row(bins))
    }
    fn nan_row(width: usize) -> String {
        vec!["nan"; width].join(",")
    }
    {
        let mut ind = wickra::TimeOfDayReturnProfile::new(24, 0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| {
                ind.update(c)
                    .map_or_else(|| nan_row(24), |o| bins_row(&o.bins))
            })
            .collect();
        write_csv(dir, "g_TimeOfDayReturnProfile", "profile", &rows);
    }
    {
        let mut ind = wickra::IntradayVolatilityProfile::new(24, 0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| {
                ind.update(c)
                    .map_or_else(|| nan_row(24), |o| bins_row(&o.bins))
            })
            .collect();
        write_csv(dir, "g_IntradayVolatilityProfile", "profile", &rows);
    }
    {
        let mut ind = wickra::VolumeByTimeProfile::new(24, 0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| {
                ind.update(c)
                    .map_or_else(|| nan_row(24), |o| bins_row(&o.bins))
            })
            .collect();
        write_csv(dir, "g_VolumeByTimeProfile", "profile", &rows);
    }
    {
        let mut ind = wickra::DayOfWeekProfile::new(0);
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| {
                ind.update(c)
                    .map_or_else(|| nan_row(7), |o| bins_row(&o.bins))
            })
            .collect();
        write_csv(dir, "g_DayOfWeekProfile", "profile", &rows);
    }
    {
        let mut ind = wickra::TpoProfile::new(30, 50).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| {
                ind.update(c).map_or_else(
                    || nan_row(52),
                    |o| price_bins_row(o.price_low, o.price_high, &o.counts),
                )
            })
            .collect();
        write_csv(dir, "g_TpoProfile", "profile", &rows);
    }
    {
        let mut ind = wickra::VolumeProfile::new(20, 50).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|&c| {
                ind.update(c).map_or_else(
                    || nan_row(52),
                    |o| price_bins_row(o.price_low, o.price_high, &o.bins),
                )
            })
            .collect();
        write_csv(dir, "g_VolumeProfile", "profile", &rows);
    }
}

// AUTO-GENERATED alt-chart-bars + footprint tranche (variable bar count per
// candle). Each row holds every bar completed on that candle, flattened; an
// empty row means no bar closed. Close-driven builders see a flat
// `Candle(close, close, close, close, 1.0, 0)`, mirroring the binding feed.
#[allow(clippy::too_many_lines)]
fn emit_bars(dir: &Path, candles: &[Candle]) {
    use wickra::BarBuilder;
    let flat = |c: &Candle| Candle::new(c.close, c.close, c.close, c.close, 1.0, 0).unwrap();
    let novol = |c: &Candle| Candle::new(c.open, c.high, c.low, c.close, 1.0, 0).unwrap();
    let withvol = |c: &Candle| Candle::new(c.open, c.high, c.low, c.close, c.volume, 0).unwrap();
    {
        let mut b = wickra::RenkoBars::new(2.0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|c| {
                b.update(flat(c))
                    .iter()
                    .map(|x| format!("{},{},{}", x.open, x.close, i64::from(x.direction)))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        write_csv(dir, "g_RenkoBars", "bars", &rows);
    }
    {
        let mut b = wickra::KagiBars::new(2.0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|c| {
                b.update(flat(c))
                    .iter()
                    .map(|x| format!("{},{},{}", x.start, x.end, i64::from(x.direction)))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        write_csv(dir, "g_KagiBars", "bars", &rows);
    }
    {
        let mut b = wickra::PointAndFigureBars::new(2.0, 3).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|c| {
                b.update(flat(c))
                    .iter()
                    .map(|x| format!("{},{},{}", i64::from(x.direction), x.high, x.low))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        write_csv(dir, "g_PointAndFigureBars", "bars", &rows);
    }
    {
        let mut b = wickra::RangeBars::new(2.0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|c| {
                b.update(flat(c))
                    .iter()
                    .map(|x| format!("{},{},{}", x.open, x.close, i64::from(x.direction)))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        write_csv(dir, "g_RangeBars", "bars", &rows);
    }
    {
        let mut b = wickra::ThreeLineBreakBars::new(3).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|c| {
                b.update(flat(c))
                    .iter()
                    .map(|x| format!("{},{},{}", x.open, x.close, i64::from(x.direction)))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        write_csv(dir, "g_ThreeLineBreakBars", "bars", &rows);
    }
    {
        let mut b = wickra::ImbalanceBars::new(5.0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|c| {
                b.update(novol(c))
                    .iter()
                    .map(|x| {
                        format!(
                            "{},{},{},{},{},{}",
                            x.open,
                            x.high,
                            x.low,
                            x.close,
                            x.imbalance,
                            i64::from(x.direction)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        write_csv(dir, "g_ImbalanceBars", "bars", &rows);
    }
    {
        let mut b = wickra::RunBars::new(3).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|c| {
                b.update(novol(c))
                    .iter()
                    .map(|x| {
                        format!(
                            "{},{},{},{},{},{}",
                            x.open,
                            x.high,
                            x.low,
                            x.close,
                            x.length,
                            i64::from(x.direction)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        write_csv(dir, "g_RunBars", "bars", &rows);
    }
    {
        let mut b = wickra::DollarBars::new(50000.0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|c| {
                b.update(withvol(c))
                    .iter()
                    .map(|x| {
                        format!(
                            "{},{},{},{},{},{}",
                            x.open, x.high, x.low, x.close, x.volume, x.dollar
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        write_csv(dir, "g_DollarBars", "bars", &rows);
    }
    {
        let mut b = wickra::TickBars::new(2).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|c| {
                b.update(withvol(c))
                    .iter()
                    .map(|x| format!("{},{},{},{},{}", x.open, x.high, x.low, x.close, x.volume))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        write_csv(dir, "g_TickBars", "bars", &rows);
    }
    {
        let mut b = wickra::VolumeBars::new(500.0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|c| {
                b.update(withvol(c))
                    .iter()
                    .map(|x| format!("{},{},{},{},{}", x.open, x.high, x.low, x.close, x.volume))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        write_csv(dir, "g_VolumeBars", "bars", &rows);
    }
    {
        use wickra::{Indicator, Side, Trade};
        let mut fp = wickra::Footprint::new(1.0).unwrap();
        let rows: Vec<String> = candles
            .iter()
            .map(|c| {
                let side = if c.close >= c.open {
                    Side::Buy
                } else {
                    Side::Sell
                };
                let trade = Trade::new(c.close, c.volume, side, c.timestamp).unwrap();
                fp.update(trade).map_or_else(String::new, |o| {
                    o.levels
                        .iter()
                        .map(|x| format!("{},{},{}", x.price, x.bid_vol, x.ask_vol))
                        .collect::<Vec<_>>()
                        .join(",")
                })
            })
            .collect();
        write_csv(dir, "g_Footprint", "footprint", &rows);
    }
}
