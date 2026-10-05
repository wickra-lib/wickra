//! Wickra versus TA-Lib on fixed reference series.
//!
//! `scripts/gen_talib_reference.py` runs the real TA-Lib library over the
//! series in `testdata/talib/` (plus the 80-bar golden input) and commits the
//! results as CSV. Every test here recomputes the same series with Wickra, both
//! streaming (`update`) and in one `batch` call, checks that the two paths are
//! bit-identical, and compares the result with TA-Lib's output.
//!
//! Tolerances:
//!
//! * Floating-point indicators match to `1e-9`, absolute below magnitude 1 and
//!   relative above it. Both sides run the same IEEE-754 recurrences, so any
//!   larger gap is a formula difference, not rounding.
//! * Candlestick patterns match exactly. Wickra emits `+1.0` / `-1.0` / `0.0`
//!   where TA-Lib emits `+100` / `-100` / `0`.
//!
//! Where an indicator's start-up differs from TA-Lib (seeding of an EMA or of
//! the Hilbert-transform state) the comparison begins at a documented settle
//! bar. The constants below record that bar for each indicator; the recursion
//! forgets its seed geometrically, and after the settle bar every value must
//! match to `1e-9` for the rest of the 3000-bar series.

use std::path::PathBuf;

use wickra_core::{
    AccelerationBands, BatchExt, Candle, ChaikinOscillator, FallingThreeMethods, HikkakeModified,
    HilbertDominantCycle, HtDcPhase, HtPhasor, HtTrendMode, Indicator, LadderBottom, MacdFix, Mama,
    MorningEveningStar, Psar, RisingThreeMethods, SarExt, SineWave, Tristar,
};

/// Read `testdata/<rel>` as a header plus columns of `f64` (`nan` allowed).
fn read_csv(rel: &str) -> (Vec<String>, Vec<Vec<f64>>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(rel);
    let text = std::fs::read_to_string(&path).unwrap();
    let mut lines = text.lines();
    let header: Vec<String> = lines
        .next()
        .unwrap()
        .split(',')
        .map(str::to_owned)
        .collect();
    let mut cols = vec![Vec::new(); header.len()];
    for line in lines {
        for (col, cell) in cols.iter_mut().zip(line.split(',')) {
            col.push(cell.parse::<f64>().unwrap());
        }
    }
    (header, cols)
}

/// One TA-Lib output column by name.
fn column(rel: &str, name: &str) -> Vec<f64> {
    let (header, mut cols) = read_csv(rel);
    let idx = header.iter().position(|h| h == name).unwrap();
    cols.swap_remove(idx)
}

/// The OHLCV input series `rel` as candles (volume 0 when the file has none).
fn candles(rel: &str) -> Vec<Candle> {
    let (header, cols) = read_csv(rel);
    let col = |name: &str| header.iter().position(|h| h == name);
    let (open, high, low, close) = (
        col("open").unwrap(),
        col("high").unwrap(),
        col("low").unwrap(),
        col("close").unwrap(),
    );
    let volume_col = col("volume");
    (0..cols[0].len())
        .map(|i| {
            let volume = volume_col.map_or(0.0, |v| cols[v][i]);
            Candle::new(
                cols[open][i],
                cols[high][i],
                cols[low][i],
                cols[close][i],
                volume,
                i64::try_from(i).unwrap(),
            )
            .unwrap()
        })
        .collect()
}

fn closes(rel: &str) -> Vec<f64> {
    candles(rel).iter().map(|c| c.close).collect()
}

/// Run `make()` streaming and in batch over `inputs`, assert the two agree
/// bit for bit, and return the per-bar outputs.
fn run<I: Indicator>(make: impl Fn() -> I, inputs: &[I::Input]) -> Vec<Option<I::Output>>
where
    I::Input: Clone,
    I::Output: PartialEq + std::fmt::Debug,
{
    let mut streaming = make();
    let streamed: Vec<Option<I::Output>> =
        inputs.iter().map(|x| streaming.update(x.clone())).collect();
    let batched = make().batch(inputs);
    assert_eq!(streamed, batched);
    streamed
}

/// [`run`] for a scalar indicator, additionally checking that the exact batch
/// (`batch_nan_into`, which fused kernels override) is bit-identical to
/// streaming. Returns the series with `NaN` for warmup bars.
fn run_scalar<I>(make: impl Fn() -> I, inputs: &[I::Input]) -> Vec<f64>
where
    I: Indicator<Output = f64>,
    I::Input: Copy,
{
    let streamed = project(&run(&make, inputs), |v| *v);
    let mut exact = vec![0.0; inputs.len()];
    make().batch_nan_into(inputs, &mut exact);
    assert_bits_eq("exact batch vs streaming", &exact, &streamed);
    streamed
}

/// Assert two series are bit-identical, reporting the first differing bar.
fn assert_bits_eq(what: &str, a: &[f64], b: &[f64]) {
    assert_eq!(a.len(), b.len(), "{what}: length");
    let first = a
        .iter()
        .zip(b)
        .position(|(x, y)| x.to_bits() != y.to_bits());
    assert_eq!(first, None, "{what}: first differing bar");
}

/// `None` → `NaN`, so a Wickra series lines up with TA-Lib's columns.
fn project<T>(out: &[Option<T>], f: impl Fn(&T) -> f64) -> Vec<f64> {
    out.iter()
        .map(|o| o.as_ref().map_or(f64::NAN, &f))
        .collect()
}

fn first_valid(xs: &[f64]) -> usize {
    xs.iter().position(|x| !x.is_nan()).unwrap()
}

fn close_enough(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

/// Assert `wickra` matches `talib` on every bar from `from` on, and that both
/// are defined there.
fn assert_matches(what: &str, wickra: &[f64], talib: &[f64], from: usize) {
    assert_eq!(wickra.len(), talib.len(), "{what}: length");
    for i in from..talib.len() {
        assert!(
            close_enough(wickra[i], talib[i]),
            "{what}: bar {i}: wickra {} vs TA-Lib {}",
            wickra[i],
            talib[i]
        );
    }
}

// ---------------------------------------------------------------------------
// Overlap / bands
// ---------------------------------------------------------------------------

/// ACCBANDS(timeperiod = 20) ↔ `AccelerationBands::new(20, 4.0)`. Exact from
/// the first bar both emit.
#[test]
fn accbands_matches_talib() {
    for series in ["golden", "long"] {
        let input = if series == "golden" {
            candles("golden/input.csv")
        } else {
            candles("talib/input_long.csv")
        };
        let out = run(|| AccelerationBands::new(20, 4.0).unwrap(), &input);
        let file = format!("talib/accbands_{series}.csv");
        let upper = column(&file, "upper");
        assert_eq!(
            first_valid(&project(&out, |o| o.upper)),
            first_valid(&upper)
        );
        assert_matches("ACCBANDS upper", &project(&out, |o| o.upper), &upper, 19);
        assert_matches(
            "ACCBANDS middle",
            &project(&out, |o| o.middle),
            &column(&file, "middle"),
            19,
        );
        assert_matches(
            "ACCBANDS lower",
            &project(&out, |o| o.lower),
            &column(&file, "lower"),
            19,
        );
    }
}

// ---------------------------------------------------------------------------
// Parabolic SAR
// ---------------------------------------------------------------------------

/// SAR(acceleration = 0.02, maximum = 0.2) ↔ `Psar::new(0.02, 0.02, 0.2)`.
/// Exact from the first output bar: the seed direction, the first extreme
/// point and the reversal clamp all follow TA-Lib.
#[test]
fn sar_matches_talib() {
    for series in ["golden", "long"] {
        let input = if series == "golden" {
            candles("golden/input.csv")
        } else {
            candles("talib/input_long.csv")
        };
        let out = run_scalar(|| Psar::new(0.02, 0.02, 0.2).unwrap(), &input);
        let talib = column(&format!("talib/sar_{series}.csv"), "sar");
        assert_eq!(first_valid(&out), first_valid(&talib));
        assert_matches("SAR", &out, &talib, 1);
    }
}

/// SAREXT with TA-Lib's defaults (start 0 = auto, offset 0, all accelerations
/// 0.02 / 0.02 / 0.2) ↔ `SarExt::classic()`. TA-Lib signs short-side values
/// negative, as Wickra does.
#[test]
fn sarext_matches_talib() {
    for series in ["golden", "long"] {
        let input = if series == "golden" {
            candles("golden/input.csv")
        } else {
            candles("talib/input_long.csv")
        };
        let out = run_scalar(SarExt::classic, &input);
        let talib = column(&format!("talib/sarext_{series}.csv"), "sarext");
        assert_eq!(first_valid(&out), first_valid(&talib));
        assert_matches("SAREXT", &out, &talib, 1);
    }
}

// ---------------------------------------------------------------------------
// EMA-seeded oscillators
// ---------------------------------------------------------------------------

/// MACDFIX(signalperiod = 9) ↔ `MacdFix::new(9)`.
///
/// Same smoothing constants (0.15 / 0.075) and the same first output bar (33).
/// The seeds differ: Wickra seeds each EMA with the SMA of its own first
/// window (the fast EMA from bars 0..=11), TA-Lib delays the fast EMA's seed so
/// it ends on the slow EMA's first bar (bars 14..=25). The gap decays by
/// 0.85 per bar and is below 1e-9 from `MACDFIX_SETTLE` on.
const MACDFIX_SETTLE: usize = 164;

#[test]
fn macdfix_matches_talib_after_seed_decay() {
    let input = closes("talib/input_long.csv");
    let out = run(|| MacdFix::new(9).unwrap(), &input);
    // The fused exact batch (`[macd, signal, histogram]` per row) is
    // bit-identical to streaming, warmup rows included.
    let fused = MacdFix::new(9).unwrap().batch_macd(&input);
    for (k, field) in ["macd", "signal", "histogram"].into_iter().enumerate() {
        let column: Vec<f64> = fused.iter().skip(k).step_by(3).copied().collect();
        let streamed = project(&out, |o| [o.macd, o.signal, o.histogram][k]);
        assert_bits_eq(field, &column, &streamed);
    }
    let file = "talib/macdfix_long.csv";
    let macd = column(file, "macd");
    assert_eq!(first_valid(&project(&out, |o| o.macd)), first_valid(&macd));
    assert_matches(
        "MACDFIX macd",
        &project(&out, |o| o.macd),
        &macd,
        MACDFIX_SETTLE,
    );
    assert_matches(
        "MACDFIX signal",
        &project(&out, |o| o.signal),
        &column(file, "signal"),
        MACDFIX_SETTLE,
    );
    assert_matches(
        "MACDFIX histogram",
        &project(&out, |o| o.histogram),
        &column(file, "histogram"),
        MACDFIX_SETTLE,
    );
}

/// ADOSC(fastperiod = 3, slowperiod = 10) ↔ `ChaikinOscillator::new(3, 10)`.
/// TA-Lib's `ADOSC` is the Chaikin oscillator; Wickra's `AdOscillator` is the
/// unrelated Williams A/D oscillator.
///
/// Both EMAs use `2 / (n + 1)`. Wickra seeds each EMA with the SMA of its first
/// window, TA-Lib seeds both with the first A/D value; the difference decays
/// with the slow EMA (factor 9/11 per bar) and is below 1e-9 from
/// `ADOSC_SETTLE` on.
const ADOSC_SETTLE: usize = 120;

#[test]
fn adosc_matches_talib_after_seed_decay() {
    let input = candles("talib/input_long.csv");
    let out = run_scalar(|| ChaikinOscillator::new(3, 10).unwrap(), &input);
    let talib = column("talib/adosc_long.csv", "adosc");
    assert_eq!(first_valid(&out), first_valid(&talib));
    assert_matches("ADOSC", &out, &talib, ADOSC_SETTLE);
}

// ---------------------------------------------------------------------------
// Hilbert transform family and MAMA
// ---------------------------------------------------------------------------
//
// TA-Lib primes its Hilbert state with zeros after a nine-bar WMA burn-in and
// starts emitting at its lookback (32, or 63 for the phase-based functions);
// Wickra waits for every tap buffer to fill and emits at its own warmup. The
// two start from different states and then run the same recursion, so the
// outputs converge. The settle bars below are where the gap has fallen under
// 1e-9 on `input_long.csv`; they are a property of the start-up, not of the
// series length.

const HT_DCPERIOD_SETTLE: usize = 186;
const HT_DCPHASE_SETTLE: usize = 202;
const HT_PHASOR_SETTLE: usize = 189;
const HT_SINE_SETTLE: usize = 193;
const HT_TRENDMODE_SETTLE: usize = 75;
const MAMA_SETTLE: usize = 152;
const FAMA_SETTLE: usize = 316;

/// `HT_DCPERIOD` ↔ `HilbertDominantCycle::new()`.
#[test]
fn ht_dcperiod_matches_talib_after_settle() {
    let input = closes("talib/input_long.csv");
    let out = run_scalar(HilbertDominantCycle::new, &input);
    let talib = column("talib/ht_dcperiod_long.csv", "dcperiod");
    assert_eq!(first_valid(&out), 49);
    assert_matches("HT_DCPERIOD", &out, &talib, HT_DCPERIOD_SETTLE);
}

/// `HT_DCPHASE` ↔ `HtDcPhase::new()`.
#[test]
fn ht_dcphase_matches_talib_after_settle() {
    let input = closes("talib/input_long.csv");
    let out = run_scalar(HtDcPhase::new, &input);
    let talib = column("talib/ht_dcphase_long.csv", "dcphase");
    assert_eq!(first_valid(&out), 49);
    assert_matches("HT_DCPHASE", &out, &talib, HT_DCPHASE_SETTLE);
}

/// `HT_PHASOR` ↔ `HtPhasor::new()`.
#[test]
fn ht_phasor_matches_talib_after_settle() {
    let input = closes("talib/input_long.csv");
    let out = run(HtPhasor::new, &input);
    let file = "talib/ht_phasor_long.csv";
    assert_eq!(first_valid(&project(&out, |o| o.inphase)), 21);
    assert_matches(
        "HT_PHASOR inphase",
        &project(&out, |o| o.inphase),
        &column(file, "inphase"),
        HT_PHASOR_SETTLE,
    );
    assert_matches(
        "HT_PHASOR quadrature",
        &project(&out, |o| o.quadrature),
        &column(file, "quadrature"),
        HT_PHASOR_SETTLE,
    );
}

/// `HT_SINE` ↔ `SineWave::new()` (`sine` from `update`, `leadsine` from `lead()`).
#[test]
fn ht_sine_matches_talib_after_settle() {
    let input = closes("talib/input_long.csv");
    let mut streaming = SineWave::new();
    let mut sine = Vec::with_capacity(input.len());
    let mut lead = Vec::with_capacity(input.len());
    for &x in &input {
        let out = streaming.update(x);
        sine.push(out.unwrap_or(f64::NAN));
        lead.push(out.map_or(f64::NAN, |_| streaming.lead()));
    }
    assert_bits_eq("HT_SINE batch", &run_scalar(SineWave::new, &input), &sine);
    let file = "talib/ht_sine_long.csv";
    assert_eq!(first_valid(&sine), 49);
    assert_matches("HT_SINE sine", &sine, &column(file, "sine"), HT_SINE_SETTLE);
    assert_matches(
        "HT_SINE leadsine",
        &lead,
        &column(file, "leadsine"),
        HT_SINE_SETTLE,
    );
}

/// `HT_TRENDMODE` ↔ `HtTrendMode::new()` (both emit 1 = trend, 0 = cycle).
#[test]
fn ht_trendmode_matches_talib_after_settle() {
    let input = closes("talib/input_long.csv");
    let out = run_scalar(HtTrendMode::new, &input);
    let talib = column("talib/ht_trendmode_long.csv", "trendmode");
    assert_eq!(first_valid(&out), 49);
    assert_matches("HT_TRENDMODE", &out, &talib, HT_TRENDMODE_SETTLE);
}

/// MAMA(fastlimit = 0.5, slowlimit = 0.05) ↔ `Mama::new(0.5, 0.05)`.
#[test]
fn mama_matches_talib_after_settle() {
    let input = closes("talib/input_long.csv");
    let out = run(|| Mama::new(0.5, 0.05).unwrap(), &input);
    let file = "talib/mama_long.csv";
    let mama = project(&out, |o| o.mama);
    assert_eq!(first_valid(&mama), first_valid(&column(file, "mama")));
    assert_matches("MAMA", &mama, &column(file, "mama"), MAMA_SETTLE);
    assert_matches(
        "FAMA",
        &project(&out, |o| o.fama),
        &column(file, "fama"),
        FAMA_SETTLE,
    );
}

/// The settle bars are tight: the bar just before each one is still outside
/// the tolerance, so a start-up change shows up here first.
#[test]
fn settle_bars_are_tight() {
    let input = closes("talib/input_long.csv");
    let candles = candles("talib/input_long.csv");
    let macd = run(|| MacdFix::new(9).unwrap(), &input);
    let phasor = run(HtPhasor::new, &input);
    let mama = run(|| Mama::new(0.5, 0.05).unwrap(), &input);
    let cases: [(&str, Vec<f64>, Vec<f64>, usize); 8] = [
        (
            "MACDFIX signal",
            project(&macd, |o| o.signal),
            column("talib/macdfix_long.csv", "signal"),
            MACDFIX_SETTLE,
        ),
        (
            "ADOSC",
            run_scalar(|| ChaikinOscillator::new(3, 10).unwrap(), &candles),
            column("talib/adosc_long.csv", "adosc"),
            ADOSC_SETTLE,
        ),
        (
            "HT_DCPERIOD",
            run_scalar(HilbertDominantCycle::new, &input),
            column("talib/ht_dcperiod_long.csv", "dcperiod"),
            HT_DCPERIOD_SETTLE,
        ),
        (
            "HT_DCPHASE",
            run_scalar(HtDcPhase::new, &input),
            column("talib/ht_dcphase_long.csv", "dcphase"),
            HT_DCPHASE_SETTLE,
        ),
        (
            "HT_PHASOR quadrature",
            project(&phasor, |o| o.quadrature),
            column("talib/ht_phasor_long.csv", "quadrature"),
            HT_PHASOR_SETTLE,
        ),
        (
            "HT_TRENDMODE",
            run_scalar(HtTrendMode::new, &input),
            column("talib/ht_trendmode_long.csv", "trendmode"),
            HT_TRENDMODE_SETTLE,
        ),
        (
            "MAMA",
            project(&mama, |o| o.mama),
            column("talib/mama_long.csv", "mama"),
            MAMA_SETTLE,
        ),
        (
            "FAMA",
            project(&mama, |o| o.fama),
            column("talib/mama_long.csv", "fama"),
            FAMA_SETTLE,
        ),
    ];
    for (what, wickra, talib, settle) in cases {
        assert!(
            !close_enough(wickra[settle - 1], talib[settle - 1]),
            "{what}: settle bar {settle} is not tight"
        );
    }
}

// ---------------------------------------------------------------------------
// Candlestick patterns (exact; ±1.0 ↔ ±100)
// ---------------------------------------------------------------------------
//
// TA-Lib sizes candle bodies and shadows against rolling averages of the
// previous 5 or 10 bars ("candle settings"); Wickra's pattern family sizes them
// against the pattern's own bars (for example a doji is a body of at most a
// tenth of the bar's range). The pattern rules -- colours, gaps, penetration,
// nesting, closes -- are the same. `input_patterns.csv` is built from neutral
// bars with constant body and range, so both sizing schemes agree on which
// bodies are long, short or doji, and every difference left is a rule
// difference. Each pattern fires at least twice in each direction there, and
// the near misses next to them (penetration below 0.3, no star gap, fifth bar
// closing short of the first, no close above the fourth high, no tristar gap,
// second hikkake bar not near its extreme) fire in neither library.

fn talib_signal(file: &str) -> Vec<f64> {
    column(file, "cdl").iter().map(|v| v / 100.0).collect()
}

fn pattern_output<I: Indicator<Input = Candle, Output = f64>>(make: impl Fn() -> I) -> Vec<f64> {
    let input = candles("talib/input_patterns.csv");
    run_scalar(make, &input)
        .iter()
        .map(|v| if v.is_nan() { 0.0 } else { *v })
        .collect()
}

fn assert_signals(what: &str, wickra: &[f64], talib: &[f64]) {
    assert!(
        talib.iter().filter(|v| **v > 0.0).count() >= 2
            || talib.iter().filter(|v| **v < 0.0).count() >= 2,
        "{what}: the series must trigger the pattern"
    );
    assert_eq!(wickra, talib, "{what}");
}

/// CDLMORNINGSTAR / CDLEVENINGSTAR(penetration = 0.3) ↔ `MorningEveningStar`
/// (`+1` morning, `-1` evening).
#[test]
fn morning_evening_star_matches_talib() {
    let morning = talib_signal("talib/cdlmorningstar_patterns.csv");
    let evening = talib_signal("talib/cdleveningstar_patterns.csv");
    let talib: Vec<f64> = morning.iter().zip(&evening).map(|(m, e)| m + e).collect();
    assert_signals(
        "CDLMORNINGSTAR/CDLEVENINGSTAR",
        &pattern_output(MorningEveningStar::new),
        &talib,
    );
}

/// CDLRISEFALL3METHODS ↔ `RisingThreeMethods` (`+1`) and
/// `FallingThreeMethods` (`-1`).
#[test]
fn rise_fall_three_methods_matches_talib() {
    let rising = pattern_output(RisingThreeMethods::new);
    let falling = pattern_output(FallingThreeMethods::new);
    let wickra: Vec<f64> = rising.iter().zip(&falling).map(|(r, f)| r + f).collect();
    assert_signals(
        "CDLRISEFALL3METHODS",
        &wickra,
        &talib_signal("talib/cdlrisefall3methods_patterns.csv"),
    );
}

/// CDLLADDERBOTTOM ↔ `LadderBottom` (bullish only).
#[test]
fn ladder_bottom_matches_talib() {
    assert_signals(
        "CDLLADDERBOTTOM",
        &pattern_output(LadderBottom::new),
        &talib_signal("talib/cdlladderbottom_patterns.csv"),
    );
}

/// CDLTRISTAR ↔ `Tristar`.
#[test]
fn tristar_matches_talib() {
    assert_signals(
        "CDLTRISTAR",
        &pattern_output(Tristar::new),
        &talib_signal("talib/cdltristar_patterns.csv"),
    );
}

/// CDLHIKKAKEMOD ↔ `HikkakeModified`. TA-Lib flags the setup bar with `±100`
/// and a later confirmation bar with `±200`; Wickra flags the setup bar only,
/// so the confirmation codes map to `0`.
#[test]
fn hikkake_modified_matches_talib() {
    let talib: Vec<f64> = column("talib/cdlhikkakemod_patterns.csv", "cdl")
        .iter()
        .map(|v| if v.abs() == 100.0 { v / 100.0 } else { 0.0 })
        .collect();
    assert_signals(
        "CDLHIKKAKEMOD",
        &pattern_output(HikkakeModified::new),
        &talib,
    );
}
