"""Generate the TA-Lib reference fixtures under `testdata/talib/`.

Several Wickra indicators are documented as matching a TA-Lib function
(`ACCBANDS`, `MACDFIX`, the `HT_*` Hilbert family, `MAMA`, `SAR`, `SAREXT`,
`ADOSC`, and a set of candlestick patterns). This script computes those
functions with the real TA-Lib library on fixed input series and writes the
results as CSV, so `crates/wickra-core/tests/talib_reference.rs` can compare
Wickra against TA-Lib without TA-Lib being installed in CI.

Input series (written next to the outputs so the Rust test reads exactly the
values TA-Lib saw):

* `testdata/golden/input.csv` -- the 80-bar golden OHLCV series, reused as is.
* `input_long.csv` -- a 3000-bar deterministic OHLCV series that alternates
  trending and cycling regimes and contains wide reversal bars. The IIR state
  of the Hilbert family, MAMA and the EMA-seeded oscillators needs a few
  hundred bars to settle, which the golden series is too short for.
* `input_patterns.csv` -- a hand-built OHLC series in which every checked
  candlestick pattern fires several times in each direction, interleaved with
  near misses that must not fire.

Each output file is `<function>_<series>.csv`: a header row with one column per
TA-Lib output, then one row per input bar. Warmup rows are `nan`. Floats are
written with 17 significant digits so they round-trip exactly; candlestick
columns hold TA-Lib's raw integer codes (`±100`, and `±200` for the
`CDLHIKKAKEMOD` confirmation bar).

Requires `numpy` and the `TA-Lib` Python package. Run from the repository root:

    python scripts/gen_talib_reference.py

The CSVs are committed; rerun the script only when a series or the function
list changes.
"""

import csv
import os

import numpy as np
import talib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GOLDEN = os.path.join(ROOT, "testdata", "golden", "input.csv")
OUT = os.path.join(ROOT, "testdata", "talib")

LONG_BARS = 3000


def fmt(value):
    """Format a float so it parses back to the identical f64 (or `nan`)."""
    value = float(value)
    if np.isnan(value):
        return "nan"
    return repr(value)


def write_series(path, cols):
    names = list(cols)
    with open(path, "w", newline="") as fh:
        out = csv.writer(fh, lineterminator="\n")
        out.writerow(names)
        for row in zip(*(cols[n] for n in names)):
            out.writerow([fmt(v) for v in row])


def read_golden():
    with open(GOLDEN, newline="") as fh:
        rows = list(csv.DictReader(fh))
    return {k: np.array([float(r[k]) for r in rows]) for k in ("open", "high", "low", "close", "volume")}


def long_series():
    """3000 bars: a random walk with alternating cycle and trend regimes.

    Every 40th bar is a wide-range bar so the Parabolic SAR meets reversal bars
    whose range covers both the stop and the prior extreme.
    """
    rng = np.random.default_rng(20261004)
    n = LONG_BARS
    t = np.arange(n)
    regime = (t // 250) % 2  # 0 = cycle, 1 = trend
    drift = np.where(regime == 1, 0.15 * np.sign(np.sin(t / 500.0) + 1e-9), 0.0)
    cycle = np.where(regime == 0, 6.0 * np.sin(2.0 * np.pi * t / 20.0), 0.0)
    walk = np.cumsum(rng.normal(0.0, 0.6, n) + drift)
    close = 200.0 + walk + cycle
    open_ = np.empty(n)
    open_[0] = close[0]
    open_[1:] = close[:-1] + rng.normal(0.0, 0.25, n - 1)
    spread = np.abs(rng.normal(0.0, 0.7, n)) + 0.05
    spread[::40] += 6.0
    high = np.maximum(open_, close) + spread * rng.uniform(0.2, 1.0, n)
    low = np.minimum(open_, close) - spread * rng.uniform(0.2, 1.0, n)
    volume = 1000.0 + np.abs(rng.normal(0.0, 400.0, n))
    return {"open": open_, "high": high, "low": low, "close": close, "volume": volume}


class Builder:
    """Append OHLC bars by (open, close, upper shadow, lower shadow)."""

    def __init__(self):
        self.bars = []
        self.level = 100.0

    def bar(self, open_, close, upper=0.5, lower=0.5):
        self.bars.append((open_, max(open_, close) + upper, min(open_, close) - lower, close))
        self.level = close

    def neutral(self, count):
        # Alternating white/black bars with body 1 and range 2: they set the
        # TA-Lib 10-bar averages (body 1.0, range 2.0) the patterns are sized
        # against, and never form a pattern on their own.
        for i in range(count):
            o = self.level
            c = o + (1.0 if i % 2 == 0 else -1.0)
            self.bar(o, c, 0.5, 0.5)
        # Return to a fixed level so every segment starts in the same place.
        self.level = 100.0

    # --- CDLMORNINGSTAR / CDLEVENINGSTAR (penetration 0.3) -------------------
    def morning_star(self, third_close_frac=0.6, gap=True):
        base = self.level
        self.bar(base + 4.0, base, 0.3, 0.3)  # long black
        star_top = base - 0.6 if gap else base + 0.2
        self.bar(star_top, star_top - 0.2, 0.3, 0.3)  # short star
        self.bar(star_top + 0.1, base + 4.0 * third_close_frac, 0.3, 0.3)  # white

    def evening_star(self, third_close_frac=0.6, gap=True):
        base = self.level
        self.bar(base, base + 4.0, 0.3, 0.3)  # long white
        star_bottom = base + 4.6 if gap else base + 3.8
        self.bar(star_bottom, star_bottom + 0.2, 0.3, 0.3)  # short star
        self.bar(star_bottom - 0.1, base + 4.0 - 4.0 * third_close_frac, 0.3, 0.3)  # black

    # --- CDLRISEFALL3METHODS --------------------------------------------------
    def rising_three(self, fifth_close_above_first=True):
        b = self.level
        self.bar(b, b + 5.0, 0.3, 0.3)  # long white
        self.bar(b + 4.0, b + 3.6, 0.2, 0.2)  # small black, drifting down
        self.bar(b + 3.6, b + 3.2, 0.2, 0.2)
        self.bar(b + 3.2, b + 2.8, 0.2, 0.2)
        top = b + 5.6 if fifth_close_above_first else b + 4.6
        self.bar(b + 3.0, top, 0.3, 0.3)  # long white

    def falling_three(self, fifth_close_below_first=True):
        b = self.level
        self.bar(b, b - 5.0, 0.3, 0.3)  # long black
        self.bar(b - 4.0, b - 3.6, 0.2, 0.2)  # small white, drifting up
        self.bar(b - 3.6, b - 3.2, 0.2, 0.2)
        self.bar(b - 3.2, b - 2.8, 0.2, 0.2)
        bottom = b - 5.6 if fifth_close_below_first else b - 4.6
        self.bar(b - 3.0, bottom, 0.3, 0.3)  # long black

    # --- CDLLADDERBOTTOM ------------------------------------------------------
    def ladder_bottom(self, close_above_fourth_high=True):
        b = self.level
        self.bar(b, b - 2.0, 0.2, 0.2)
        self.bar(b - 1.5, b - 3.5, 0.2, 0.2)
        self.bar(b - 3.0, b - 5.0, 0.2, 0.2)
        self.bar(b - 4.8, b - 5.6, 1.5, 0.2)  # black with a long upper shadow
        close = b - 2.5 if close_above_fourth_high else b - 3.6
        self.bar(b - 4.5, close, 0.2, 0.2)  # white

    # --- CDLTRISTAR -----------------------------------------------------------
    def tristar(self, bearish=True, gap=True):
        b = self.level
        self.bar(b, b + 0.05, 1.0, 1.0)
        if bearish:
            mid = b + 1.0 if gap else b
            self.bar(mid, mid + 0.05, 1.0, 1.0)
            self.bar(b + 0.5, b + 0.55, 1.0, 1.0)
        else:
            mid = b - 1.0 if gap else b
            self.bar(mid, mid + 0.05, 1.0, 1.0)
            self.bar(b - 0.5, b - 0.45, 1.0, 1.0)

    # --- CDLHIKKAKEMOD --------------------------------------------------------
    def hikkake_mod(self, bullish=True, near=True, confirm=True):
        b = self.level
        self.bar(b, b + 0.5, 1.5, 1.5)  # bar1: wide, range [b-1.5, b+2.0]
        if bullish:
            close2 = b - 0.9 if near else b - 0.5
            self.bar(b - 0.5, close2, 0.5, 0.2)  # bar2: inside bar1, closes near its low
            self.bar(b - 0.6, b - 0.4, 0.2, 0.2)  # bar3: inside bar2
            self.bar(b - 0.5, b - 0.9, 0.2, 0.2)  # bar4: lower high and lower low
            if confirm:
                self.bar(b - 0.8, b + 0.5, 0.2, 0.2)  # closes above bar3's high
        else:
            close2 = b + 1.4 if near else b + 0.9
            self.bar(b + 1.0, close2, 0.2, 0.5)  # bar2: inside bar1, closes near its high
            self.bar(b + 1.0, b + 1.2, 0.2, 0.2)  # bar3: inside bar2
            self.bar(b + 1.3, b + 1.5, 0.2, 0.2)  # bar4: higher high and higher low
            if confirm:
                self.bar(b + 1.2, b + 0.3, 0.2, 0.2)  # closes below bar3's low


def pattern_series():
    b = Builder()
    segments = [
        lambda: b.morning_star(),
        lambda: b.evening_star(),
        lambda: b.morning_star(third_close_frac=0.45),
        lambda: b.evening_star(third_close_frac=0.45),
        lambda: b.morning_star(third_close_frac=0.2),  # near miss: penetration < 0.3
        lambda: b.evening_star(third_close_frac=0.2),  # near miss: penetration < 0.3
        lambda: b.morning_star(gap=False),  # near miss: star body does not gap
        lambda: b.evening_star(gap=False),  # near miss: star body does not gap
        lambda: b.rising_three(),
        lambda: b.falling_three(),
        lambda: b.rising_three(),
        lambda: b.falling_three(),
        lambda: b.rising_three(fifth_close_above_first=False),  # near miss
        lambda: b.falling_three(fifth_close_below_first=False),  # near miss
        lambda: b.ladder_bottom(),
        lambda: b.ladder_bottom(),
        lambda: b.ladder_bottom(close_above_fourth_high=False),  # near miss
        lambda: b.tristar(bearish=True),
        lambda: b.tristar(bearish=False),
        lambda: b.tristar(bearish=True),
        lambda: b.tristar(bearish=False),
        lambda: b.tristar(bearish=True, gap=False),  # near miss
        lambda: b.tristar(bearish=False, gap=False),  # near miss
        lambda: b.hikkake_mod(bullish=True),
        lambda: b.hikkake_mod(bullish=False),
        lambda: b.hikkake_mod(bullish=True, confirm=False),
        lambda: b.hikkake_mod(bullish=False, confirm=False),
        lambda: b.hikkake_mod(bullish=True, near=False),  # near miss: bar2 not near its low
        lambda: b.hikkake_mod(bullish=False, near=False),  # near miss: bar2 not near its high
    ]
    b.neutral(14)
    for seg in segments:
        seg()
        b.neutral(14)
    o, h, l, c = (np.array(col) for col in zip(*b.bars))
    return {"open": o, "high": h, "low": l, "close": c}


def main():
    os.makedirs(OUT, exist_ok=True)
    golden = read_golden()
    long = long_series()
    patterns = pattern_series()
    write_series(os.path.join(OUT, "input_long.csv"), long)
    write_series(os.path.join(OUT, "input_patterns.csv"), patterns)

    outputs = {}
    for name, s in (("golden", golden), ("long", long)):
        o, h, l, c, v = s["open"], s["high"], s["low"], s["close"], s["volume"]
        upper, middle, lower = talib.ACCBANDS(h, l, c, timeperiod=20)
        outputs[f"accbands_{name}"] = {"upper": upper, "middle": middle, "lower": lower}
        outputs[f"sar_{name}"] = {"sar": talib.SAR(h, l, acceleration=0.02, maximum=0.2)}
        outputs[f"sarext_{name}"] = {"sarext": talib.SAREXT(h, l)}
    o, h, l, c, v = (long[k] for k in ("open", "high", "low", "close", "volume"))
    macd, signal, hist = talib.MACDFIX(c, signalperiod=9)
    outputs["macdfix_long"] = {"macd": macd, "signal": signal, "histogram": hist}
    outputs["adosc_long"] = {"adosc": talib.ADOSC(h, l, c, v, fastperiod=3, slowperiod=10)}
    outputs["ht_dcperiod_long"] = {"dcperiod": talib.HT_DCPERIOD(c)}
    outputs["ht_dcphase_long"] = {"dcphase": talib.HT_DCPHASE(c)}
    inphase, quadrature = talib.HT_PHASOR(c)
    outputs["ht_phasor_long"] = {"inphase": inphase, "quadrature": quadrature}
    outputs["ht_trendmode_long"] = {"trendmode": talib.HT_TRENDMODE(c).astype(float)}
    sine, lead = talib.HT_SINE(c)
    outputs["ht_sine_long"] = {"sine": sine, "leadsine": lead}
    mama, fama = talib.MAMA(c, fastlimit=0.5, slowlimit=0.05)
    outputs["mama_long"] = {"mama": mama, "fama": fama}

    o, h, l, c = (patterns[k] for k in ("open", "high", "low", "close"))
    outputs["cdlmorningstar_patterns"] = {"cdl": talib.CDLMORNINGSTAR(o, h, l, c, penetration=0.3).astype(float)}
    outputs["cdleveningstar_patterns"] = {"cdl": talib.CDLEVENINGSTAR(o, h, l, c, penetration=0.3).astype(float)}
    outputs["cdlrisefall3methods_patterns"] = {"cdl": talib.CDLRISEFALL3METHODS(o, h, l, c).astype(float)}
    outputs["cdlladderbottom_patterns"] = {"cdl": talib.CDLLADDERBOTTOM(o, h, l, c).astype(float)}
    outputs["cdltristar_patterns"] = {"cdl": talib.CDLTRISTAR(o, h, l, c).astype(float)}
    outputs["cdlhikkakemod_patterns"] = {"cdl": talib.CDLHIKKAKEMOD(o, h, l, c).astype(float)}

    for name, cols in outputs.items():
        write_series(os.path.join(OUT, f"{name}.csv"), cols)
    print(f"wrote {len(outputs) + 2} files to {os.path.relpath(OUT, ROOT)} (TA-Lib {talib.__ta_version__.decode()})")


if __name__ == "__main__":
    main()
