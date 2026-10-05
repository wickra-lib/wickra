"""Streaming-vs-batch, shape and reference-value tests for the F1-F12 families.

Every indicator added since the original 25 is exercised here. The central
contract is the same as the rest of the suite: ``batch(...)`` must equal
repeated streaming ``update(...)`` across the whole warmup -> steady-state
transition, and batch shapes must match the input length.
"""

from __future__ import annotations

import math

import numpy as np
import pytest

import wickra as ta


def _to_np(x) -> np.ndarray:
    """Normalize a Wickra batch result to a float64 ndarray.

    Scalar batches return a stdlib ``array.array`` (1-D, wrapped zero-copy);
    multi-output batches return a ``Matrix`` exposing ``.shape``/``.tolist()``.
    """
    if isinstance(x, np.ndarray):
        return x.astype(np.float64)
    if hasattr(x, "tolist") and hasattr(x, "shape"):
        return np.asarray(x.tolist(), dtype=np.float64)
    return np.asarray(x, dtype=np.float64)


def _eq_nan(a, b, tol: float = 1e-9) -> bool:
    """Compare two float arrays treating NaN and matching-sign inf positions as equal."""
    a = _to_np(a)
    b = _to_np(b)
    if a.shape != b.shape:
        return False
    both_nan = np.isnan(a) & np.isnan(b)
    both_inf_same = np.isinf(a) & np.isinf(b) & (np.sign(a) == np.sign(b))
    skip = both_nan | both_inf_same
    with np.errstate(invalid="ignore"):
        diff = np.abs(a - b)
    return bool(np.all(np.where(skip, 0.0, diff) <= tol))


@pytest.fixture
def ohlcv() -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    """Synthetic high / low / close / volume series, 200 bars."""
    t = np.arange(200, dtype=np.float64)
    close = 100.0 + np.sin(t * 0.15) * 8.0 + np.cos(t * 0.32) * 3.0
    spread = 0.5 + np.abs(np.sin(t * 0.07))
    high = close + spread
    low = close - spread
    volume = 1000.0 + (t % 7) * 50.0
    return high, low, close, volume


# --- Scalar (f64 -> f64) indicators ---------------------------------------

SCALAR = [
    (ta.M2Measure, (20, 0.0, 0.02)),
    (ta.UpsidePotentialRatio, (20, 0.0)),
    (ta.GainToPainRatio, (12,)),
    (ta.CommonSenseRatio, (20,)),
    (ta.KRatio, (30,)),
    (ta.TailRatio, (20,)),
    (ta.MartinRatio, (14,)),
    (ta.BurkeRatio, (12,)),
    (ta.SterlingRatio, (12,)),
    (ta.AUTOCORRPGRAM, (10, 48)),
    (ta.EVENBETTERSINE, (40, 10)),
    (ta.BANDPASS, (20, 0.3)),
    (ta.UNIVERSALOSC, (20,)),
    (ta.ADAPTIVERSI, (14,)),
    (ta.CTI, (20,)),
    (ta.TRENDFLEX, (20,)),
    (ta.REFLEX, (20,)),
    (ta.HIGHPASS, (48,)),
    (ta.SAMPLEENT, (20, 2, 0.2)),
    (ta.SHANNONENT, (20, 8)),
    (ta.ROLLINGMINMAX, (20,)),
    (ta.JARQUEBERA, (20,)),
    (ta.BipowerVariation, (20,)),
    (ta.VolatilityOfVolatility, (20, 20)),
    (ta.Garch11, (0.000002, 0.1, 0.88)),
    (ta.EwmaVolatility, (0.94,)),
    (ta.PpoHistogram, (3, 6, 3)),
    (ta.MacdHistogram, (3, 6, 3)),
    (ta.TsfOscillator, (3,)),
    (ta.WAVE_PM, (32, 3)),
    (ta.POLARIZED_FRACTAL_EFFICIENCY, (10, 5)),
    (ta.TREND_STRENGTH_INDEX, (20,)),
    (ta.DerivativeOscillator, (14, 5, 3, 9)),
    (ta.RMI, (14, 5)),
    (ta.DynamicMomentumIndex, (14,)),
    (ta.RSX, (14,)),
    (ta.FisherRSI, (14,)),
    (ta.DisparityIndex, (14,)),
    (ta.HoltWinters, (0.2, 0.1)),
    (ta.GD, (5, 0.7)),
    (ta.AdaptiveLaguerre, (13,)),
    (ta.MedianMA, (14,)),
    (ta.EHMA, (9,)),
    (ta.GMA, (14,)),
    (ta.SWMA, (14,)),
    (ta.Expectancy, (20,)),
    (ta.WinRate, (20,)),
    (ta.RegimeLabel, (5, 20)),
    (ta.JumpIndicator, (20, 3.0)),
    (ta.TrendLabel, (10,)),
    (ta.RollingQuantile, (20, 0.5)),
    (ta.RollingPercentileRank, (14,)),
    (ta.RollingIqr, (14,)),
    (ta.RealizedVolatility, (20,)),
    (ta.LogReturn, (1,)),
    (ta.TSF, (14,)),
    (ta.LINEARREG_INTERCEPT, (14,)),
    (ta.ROCR100, (10,)),
    (ta.ROCR, (10,)),
    (ta.ROCP, (10,)),
    (ta.MIDPOINT, (14,)),
    (ta.SMMA, (14,)),
    (ta.TRIMA, (20,)),
    (ta.ZLEMA, (14,)),
    (ta.ALMA, (9, 0.85, 6.0)),
    (ta.McGinleyDynamic, (10,)),
    (ta.FRAMA, (16,)),
    (ta.VIDYA, (14, 9)),
    (ta.JMA, (14, 0.0, 2)),
    (ta.T3, (5, 0.7)),
    (ta.MOM, (10,)),
    (ta.CMO, (14,)),
    (ta.TSI, (25, 13)),
    (ta.PMO, (35, 20)),
    (ta.TII, (20, 10)),
    (ta.StochRSI, (14, 14)),
    (ta.PPO, (12, 26)),
    (ta.APO, (12, 26)),
    (ta.CFO, (14,)),
    (ta.ElderImpulse, (13, 12, 26, 9)),
    (ta.STC, (23, 50, 10, 0.5)),
    (ta.DPO, (20,)),
    (ta.Coppock, (14, 11, 10)),
    (ta.StdDev, (20,)),
    (ta.UlcerIndex, (14,)),
    (ta.HistoricalVolatility, (20, 252)),
    (ta.BollingerBandwidth, (20, 2.0)),
    (ta.PercentB, (20, 2.0)),
    (ta.LinearRegression, (14,)),
    (ta.LinRegSlope, (14,)),
    (ta.VerticalHorizontalFilter, (28,)),
    (ta.ZScore, (20,)),
    (ta.LinRegAngle, (14,)),
    (ta.PercentageTrailingStop, (5.0,)),
    (ta.StepTrailingStop, (1.0,)),
    (ta.RenkoTrailingStop, (1.0,)),
    (ta.LaguerreRSI, (0.5,)),
    (ta.ConnorsRSI, (3, 2, 100)),
    (ta.RVIVolatility, (10,)),
    # Family 10 — Ehlers / Cycle scalar indicators
    (ta.SuperSmoother, (10,)),
    (ta.FisherTransform, (10,)),
    (ta.InverseFisherTransform, (1.0,)),
    (ta.Decycler, (20,)),
    (ta.DecyclerOscillator, (10, 30)),
    (ta.RoofingFilter, (10, 48)),
    (ta.CenterOfGravity, (10,)),
    (ta.CyberneticCycle, (10,)),
    (ta.InstantaneousTrendline, (20,)),
    (ta.EhlersStochastic, (20,)),
    (ta.EmpiricalModeDecomposition, (20, 0.1)),
    (ta.HilbertDominantCycle, ()),
    (ta.HT_DCPHASE, ()),
    (ta.HT_TRENDMODE, ()),
    (ta.AdaptiveCycle, ()),
    (ta.SineWave, ()),
    (ta.FAMA, (0.5, 0.05)),
    # Family 12 — Statistik / Regression
    (ta.Variance, (20,)),
    (ta.CoefficientOfVariation, (20,)),
    (ta.Skewness, (20,)),
    (ta.Kurtosis, (20,)),
    (ta.StandardError, (14,)),
    (ta.DetrendedStdDev, (14,)),
    (ta.RSquared, (14,)),
    (ta.MedianAbsoluteDeviation, (20,)),
    (ta.Autocorrelation, (20, 1)),
    (ta.HurstExponent, (40, 4)),
    # Family 15 — Risk / Performance (scalar f64 input = period return or
    # equity sample).
    (ta.SharpeRatio, (20, 0.0)),
    (ta.SortinoRatio, (20, 0.0)),
    (ta.CalmarRatio, (20,)),
    (ta.OmegaRatio, (20, 0.0)),
    (ta.MaxDrawdown, (20,)),
    (ta.AverageDrawdown, (20,)),
    (ta.DrawdownDuration, ()),
    (ta.PainIndex, (20,)),
    (ta.ValueAtRisk, (20, 0.95)),
    (ta.ConditionalValueAtRisk, (20, 0.95)),
    (ta.ProfitFactor, (20,)),
    (ta.GainLossRatio, (20,)),
    (ta.RecoveryFactor, ()),
    (ta.KellyCriterion, (20,)),
]


# Family 05 band/channel indicators with scalar input and multi-output.
# `cols` is the expected number of band columns from `batch`.
SCALAR_MULTI = {
    "MedianChannel": (lambda: ta.MedianChannel(5, 2.0), 3),
    "BomarBands": (lambda: ta.BomarBands(4, 0.85), 3),
    "QuartileBands": (lambda: ta.QuartileBands(4), 3),
    "Qqe": (lambda: ta.QQE(14, 5, 4.236), 2),
    "MaEnvelope": (lambda: ta.MaEnvelope(20, 0.025), 3),
    "LinRegChannel": (lambda: ta.LinRegChannel(20, 2.0), 3),
    "StandardErrorBands": (lambda: ta.StandardErrorBands(21, 2.0), 3),
    "DoubleBollinger": (lambda: ta.DoubleBollinger(20, 1.0, 2.0), 5),
    "MacdFix": (lambda: ta.MACDFIX(9), 3),
    "MacdExt": (lambda: ta.MACDEXT(12, 0, 26, 0, 9, 0), 3),
    "HtPhasor": (lambda: ta.HT_PHASOR(), 2),
}


@pytest.mark.parametrize("cls, args", SCALAR, ids=[c.__name__ for c, _ in SCALAR])
def test_scalar_streaming_matches_batch(cls, args, sine_prices):
    batch = cls(*args).batch(sine_prices)
    assert len(batch) == len(sine_prices)
    assert batch.typecode == "d"

    streamer = cls(*args)
    streamed = []
    for p in sine_prices:
        v = streamer.update(float(p))
        streamed.append(math.nan if v is None else float(v))
    assert _eq_nan(batch, np.array(streamed, dtype=np.float64))


# --- Two-series (asset, benchmark) indicators -----------------------------

PAIR = [
    (ta.HasbrouckInformationShare, (6,)),
    (ta.KendallTau, (20,)),
    (ta.SpreadAr1Coefficient, (40,)),
    (ta.GrangerCausality, (60, 1)),
    (ta.VarianceRatio, (60, 2)),
    (ta.BetaNeutralSpread, (20,)),
    (ta.DistanceSsd, (20,)),
    (ta.SpreadHurst, (60,)),
    (ta.OuHalfLife, (60,)),
    (ta.RollingCovariance, (20,)),
    (ta.RollingCorrelation, (20,)),
    (ta.TreynorRatio, (20, 0.0)),
    (ta.InformationRatio, (20,)),
    (ta.Alpha, (20, 0.0)),
    (ta.PairwiseBeta, (20,)),
    (ta.PairSpreadZScore, (20, 20)),
]


@pytest.mark.parametrize("cls, args", PAIR, ids=[c.__name__ for c, _ in PAIR])
def test_pair_streaming_matches_batch(cls, args, sine_prices):
    asset = np.ascontiguousarray(sine_prices.astype(np.float64))
    bench = np.ascontiguousarray((sine_prices * 0.7 + 0.001).astype(np.float64))
    batch = cls(*args).batch(asset, bench)
    assert len(batch) == len(asset)
    assert batch.typecode == "d"

    streamer = cls(*args)
    streamed = []
    for a, b in zip(asset, bench):
        v = streamer.update(float(a), float(b))
        streamed.append(math.nan if v is None else float(v))
    assert _eq_nan(batch, np.array(streamed, dtype=np.float64))


def _ll_signal(t):
    return math.sin(t * 0.4) + 0.4 * math.sin(t * 1.1) + 0.2 * math.cos(t * 0.27)


def test_lead_lag_detects_lead():
    n = 60
    a = np.array([_ll_signal(t) for t in range(n)])
    # b is a delayed by 3 ⇒ a leads b ⇒ lag = +3, correlation ≈ 1.
    b = np.array([_ll_signal(t - 3) for t in range(n)])
    out = _to_np(ta.LeadLagCrossCorrelation(12, 5).batch(a, b))
    assert out.shape == (n, 2)
    assert int(out[-1, 0]) == 3
    assert out[-1, 1] > 0.99


def test_lead_lag_streaming_matches_batch():
    n = 60
    a = np.array([_ll_signal(t) for t in range(n)])
    b = np.array([_ll_signal(t - 2) for t in range(n)])
    ind = ta.LeadLagCrossCorrelation(12, 5)
    batch = ind.batch(a, b)
    streamer = ta.LeadLagCrossCorrelation(12, 5)
    for i in range(n):
        v = streamer.update(float(a[i]), float(b[i]))
        if v is None:
            assert math.isnan(batch[i, 0]) and math.isnan(batch[i, 1])
        else:
            lag, corr = v
            assert int(batch[i, 0]) == lag
            assert math.isclose(batch[i, 1], corr, rel_tol=1e-12, abs_tol=1e-12)


def test_cointegration_detects_mean_reverting_pair():
    n = 80
    b = np.array([50.0 + 0.5 * t for t in range(n)])
    # a tracks 2*b with a small mean-reverting wobble ⇒ cointegrated.
    a = 2.0 * b + 1.0 + 0.5 * np.sin(np.arange(n) * 0.6)
    out = _to_np(ta.Cointegration(40, 1).batch(a, b))
    assert out.shape == (n, 3)
    assert abs(out[-1, 0] - 2.0) < 0.1  # hedge ratio
    assert out[-1, 2] < -2.0  # ADF statistic: strongly mean-reverting


def test_cointegration_streaming_matches_batch():
    n = 70
    b = np.array([30.0 + 0.7 * t for t in range(n)])
    a = 1.8 * b + 2.0 + 0.5 * np.sin(np.arange(n) * 0.4)
    batch = ta.Cointegration(25, 2).batch(a, b)
    streamer = ta.Cointegration(25, 2)
    for i in range(n):
        v = streamer.update(float(a[i]), float(b[i]))
        if v is None:
            assert np.all(np.isnan(batch[i]))
        else:
            hr, sp, adf = v
            assert math.isclose(batch[i, 0], hr, rel_tol=1e-12, abs_tol=1e-12)
            assert math.isclose(batch[i, 1], sp, rel_tol=1e-12, abs_tol=1e-12)
            assert math.isclose(batch[i, 2], adf, rel_tol=1e-12, abs_tol=1e-12)


def test_kalman_hedge_ratio_converges_and_streaming_matches_batch():
    n = 500
    b = np.array([100.0 + 95.0 * math.sin(t * 0.5) for t in range(n)])
    a = 2.0 * b + 5.0  # a = 2*b + 5 with a wide-ranging b ⇒ identifiable
    batch = ta.KalmanHedgeRatio(1e-2, 1e-3).batch(a, b)
    assert batch.shape == (n, 3)
    assert abs(batch[-1, 0] - 2.0) < 0.05  # hedge ratio
    assert abs(batch[-1, 2]) < 0.05  # spread (forecast error)
    streamer = ta.KalmanHedgeRatio(1e-2, 1e-3)
    for i in range(n):
        hr, ic, sp = streamer.update(float(a[i]), float(b[i]))
        assert math.isclose(batch[i, 0], hr, rel_tol=1e-12, abs_tol=1e-12)
        assert math.isclose(batch[i, 1], ic, rel_tol=1e-12, abs_tol=1e-12)
        assert math.isclose(batch[i, 2], sp, rel_tol=1e-12, abs_tol=1e-12)


def test_spread_bollinger_bands_streaming_matches_batch():
    n = 60
    b = np.array([100.0 + t for t in range(n)])
    a = b + 3.0 * np.sin(np.arange(n) * 0.4)
    batch = ta.SpreadBollingerBands(20, 2.0).batch(a, b)
    assert batch.shape == (n, 4)
    streamer = ta.SpreadBollingerBands(20, 2.0)
    for i in range(n):
        v = streamer.update(float(a[i]), float(b[i]))
        if v is None:
            assert np.all(np.isnan(batch[i]))
        else:
            mid, up, lo, pct_b = v
            assert math.isclose(batch[i, 0], mid, rel_tol=1e-12, abs_tol=1e-12)
            assert math.isclose(batch[i, 1], up, rel_tol=1e-12, abs_tol=1e-12)
            assert math.isclose(batch[i, 2], lo, rel_tol=1e-12, abs_tol=1e-12)
            assert math.isclose(batch[i, 3], pct_b, rel_tol=1e-12, abs_tol=1e-12)
            assert lo <= mid <= up


def test_relative_strength_constant_ratio():
    n = 30
    a = np.full(n, 200.0)
    b = np.full(n, 100.0)  # ratio is a constant 2
    out = _to_np(ta.RelativeStrengthAB(5, 5).batch(a, b))
    assert out.shape == (n, 3)
    assert math.isclose(out[-1, 0], 2.0, abs_tol=1e-12)  # ratio
    assert math.isclose(out[-1, 1], 2.0, abs_tol=1e-12)  # ratio MA
    assert math.isclose(out[-1, 2], 50.0, abs_tol=1e-9)  # flat ratio ⇒ RSI 50


def test_relative_strength_streaming_matches_batch():
    n = 60
    tt = np.arange(n)
    a = 100.0 + 5.0 * np.sin(tt * 0.3)
    b = 100.0 + 2.0 * np.cos(tt * 0.2)
    batch = ta.RelativeStrengthAB(10, 14).batch(a, b)
    streamer = ta.RelativeStrengthAB(10, 14)
    for i in range(n):
        v = streamer.update(float(a[i]), float(b[i]))
        if v is None:
            assert np.all(np.isnan(batch[i]))
        else:
            ratio, ma, rsi = v
            assert math.isclose(batch[i, 0], ratio, rel_tol=1e-12, abs_tol=1e-12)
            assert math.isclose(batch[i, 1], ma, rel_tol=1e-12, abs_tol=1e-12)
            assert math.isclose(batch[i, 2], rsi, rel_tol=1e-12, abs_tol=1e-12)


# --- Candle-input, single-output indicators -------------------------------
#
# Each entry is (factory, batch-call). Streaming always feeds the full
# 6-tuple candle; the batch helper takes only the columns it needs.

CANDLE_SCALAR = {
    "ProfileShape": (
        lambda: ta.ProfileShape(20, 24),
        lambda ind, h, l, c, v: ind.batch(h, l, v),
    ),
    "SinglePrints": (
        lambda: ta.SinglePrints(20, 24),
        lambda ind, h, l, c, v: ind.batch(h, l),
    ),
    "NakedPoc": (
        lambda: ta.NakedPoc(20, 24),
        lambda ind, h, l, c, v: ind.batch(h, l, c, v),
    ),
    "FryPanBottom": (
        lambda: ta.FryPanBottom(9),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "NewPriceLines": (
        lambda: ta.NewPriceLines(5),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "DumplingTop": (
        lambda: ta.DumplingTop(9),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "TowerTopBottom": (
        lambda: ta.TowerTopBottom(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "HaramiCross": (
        lambda: ta.HaramiCross(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Tristar": (
        lambda: ta.Tristar(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "ThreeLineBreak": (
        lambda: ta.ThreeLineBreak(3),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "HeikinAshiOscillator": (
        lambda: ta.HeikinAshiOscillator(5),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "TDDWave": (
        lambda: ta.TDDWave(2),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "TDTrap": (
        lambda: ta.TDTrap(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "TDPropulsion": (
        lambda: ta.TDPropulsion(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "TDClopwin": (
        lambda: ta.TDClopwin(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "TDClop": (
        lambda: ta.TDClop(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "TDCamouflage": (
        lambda: ta.TDCamouflage(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "PivotReversal": (
        lambda: ta.PivotReversal(1, 1),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "ADAPTIVECCI": (lambda: ta.ADAPTIVECCI(20), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "BetterVolume": (
        lambda: ta.BetterVolume(14),
        # The streaming side builds candles with open == close; match it.
        lambda ind, h, l, c, v: ind.batch(c, h, l, c, v),
    ),
    "IntradayIntensity": (
        lambda: ta.IntradayIntensity(),
        lambda ind, h, l, c, v: ind.batch(h, l, c, v),
    ),
    "TradeVolumeIndex": (
        lambda: ta.TradeVolumeIndex(0.25),
        lambda ind, h, l, c, v: ind.batch(c, v),
    ),
    "TwiggsMoneyFlow": (
        lambda: ta.TwiggsMoneyFlow(21),
        lambda ind, h, l, c, v: ind.batch(h, l, c, v),
    ),
    "Wad": (
        lambda: ta.Wad(),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "VolumeRsi": (
        lambda: ta.VolumeRsi(14),
        lambda ind, h, l, c, v: ind.batch(c, v),
    ),
    "TimeBasedStop": (lambda: ta.TimeBasedStop(5), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "ProjectionOscillator": (lambda: ta.ProjectionOscillator(14), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "VolatilityRatio": (lambda: ta.VolatilityRatio(14), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "TTM_TREND": (lambda: ta.TTM_TREND(6), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "StochasticCCI": (lambda: ta.StochasticCCI(14), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    # Per-bar OHLC transforms (open matters). The streaming harness feeds
    # open == close, so batch passes the close column in for open to match.
    "HighLowRange": (lambda: ta.HighLowRange(), lambda ind, h, l, c, v: ind.batch(c, h, l, c)),
    "WickRatio": (lambda: ta.WickRatio(), lambda ind, h, l, c, v: ind.batch(c, h, l, c)),
    "BodySizePct": (lambda: ta.BodySizePct(), lambda ind, h, l, c, v: ind.batch(c, h, l, c)),
    "CloseVsOpen": (lambda: ta.CloseVsOpen(), lambda ind, h, l, c, v: ind.batch(c, h, l, c)),
    "ThreeDrives": (
        lambda: ta.ThreeDrives(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Cypher": (
        lambda: ta.Cypher(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Shark": (
        lambda: ta.Shark(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Crab": (
        lambda: ta.Crab(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Bat": (
        lambda: ta.Bat(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Butterfly": (
        lambda: ta.Butterfly(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Gartley": (
        lambda: ta.Gartley(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Abcd": (
        lambda: ta.Abcd(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "CupAndHandle": (
        lambda: ta.CupAndHandle(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "RectangleRange": (
        lambda: ta.RectangleRange(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "FlagPennant": (
        lambda: ta.FlagPennant(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Wedge": (
        lambda: ta.Wedge(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Triangle": (
        lambda: ta.Triangle(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "HeadAndShoulders": (
        lambda: ta.HeadAndShoulders(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "TripleTopBottom": (
        lambda: ta.TripleTopBottom(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "DoubleTopBottom": (
        lambda: ta.DoubleTopBottom(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "MIDPRICE": (lambda: ta.MIDPRICE(14), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "AVGPRICE": (lambda: ta.AVGPRICE(), lambda ind, h, l, c, v: ind.batch(c, h, l, c)),
    "DX": (lambda: ta.DX(14), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "MINUS_DI": (lambda: ta.MINUS_DI(14), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "PLUS_DI": (lambda: ta.PLUS_DI(14), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "VWMA": (lambda: ta.VWMA(20), lambda ind, h, l, c, v: ind.batch(c, v)),
    "SAREXT": (lambda: ta.SAREXT(), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "PLUS_DM": (lambda: ta.PLUS_DM(14), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "MINUS_DM": (lambda: ta.MINUS_DM(14), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "RVI": (
        # extract_candle pulls the open price from index 0 of the tuple; the
        # streaming test below already builds candles with open == close, so
        # match that here by passing close as the open column.
        lambda: ta.RVI(10),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Inertia": (
        lambda: ta.Inertia(14, 20),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "PGO": (lambda: ta.PGO(14), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "SMI": (lambda: ta.SMI(5, 3, 3), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "EVWMA": (lambda: ta.EVWMA(20), lambda ind, h, l, c, v: ind.batch(c, v)),
    "UltimateOscillator": (
        lambda: ta.UltimateOscillator(7, 14, 28),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "AroonOscillator": (
        lambda: ta.AroonOscillator(14),
        lambda ind, h, l, c, v: ind.batch(h, l),
    ),
    "NATR": (lambda: ta.NATR(14), lambda ind, h, l, c, v: ind.batch(h, l, c)),
    "MassIndex": (lambda: ta.MassIndex(9, 25), lambda ind, h, l, c, v: ind.batch(h, l)),
    "ADL": (lambda: ta.ADL(), lambda ind, h, l, c, v: ind.batch(h, l, c, v)),
    "VolumePriceTrend": (
        lambda: ta.VolumePriceTrend(),
        lambda ind, h, l, c, v: ind.batch(c, v),
    ),
    "ChaikinMoneyFlow": (
        lambda: ta.ChaikinMoneyFlow(20),
        lambda ind, h, l, c, v: ind.batch(h, l, c, v),
    ),
    "ChaikinOscillator": (
        lambda: ta.ChaikinOscillator(3, 10),
        lambda ind, h, l, c, v: ind.batch(h, l, c, v),
    ),
    "ForceIndex": (
        lambda: ta.ForceIndex(13),
        lambda ind, h, l, c, v: ind.batch(c, v),
    ),
    "EaseOfMovement": (
        lambda: ta.EaseOfMovement(14),
        lambda ind, h, l, c, v: ind.batch(h, l, v),
    ),
    "KVO": (
        lambda: ta.KVO(34, 55),
        lambda ind, h, l, c, v: ind.batch(h, l, c, v),
    ),
    "VolumeOscillator": (
        lambda: ta.VolumeOscillator(14, 28),
        lambda ind, h, l, c, v: ind.batch(v),
    ),
    "NVI": (
        lambda: ta.NVI(),
        lambda ind, h, l, c, v: ind.batch(c, v),
    ),
    "PVI": (
        lambda: ta.PVI(),
        lambda ind, h, l, c, v: ind.batch(c, v),
    ),
    "AdOscillator": (
        lambda: ta.AdOscillator(),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "AnchoredVWAP": (
        lambda: ta.AnchoredVWAP(),
        lambda ind, h, l, c, v: ind.batch(h, l, c, v),
    ),
    "DemandIndex": (
        lambda: ta.DemandIndex(10),
        lambda ind, h, l, c, v: ind.batch(h, l, c, v),
    ),
    "TSV": (
        lambda: ta.TSV(18),
        lambda ind, h, l, c, v: ind.batch(c, v),
    ),
    "VZO": (
        lambda: ta.VZO(14),
        lambda ind, h, l, c, v: ind.batch(c, v),
    ),
    "MarketFacilitationIndex": (
        lambda: ta.MarketFacilitationIndex(),
        lambda ind, h, l, c, v: ind.batch(h, l, v),
    ),
    "AtrTrailingStop": (
        lambda: ta.AtrTrailingStop(14, 3.0),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "HiLoActivator": (
        lambda: ta.HiLoActivator(3),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "VoltyStop": (
        lambda: ta.VoltyStop(14, 2.0),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "YoyoExit": (
        lambda: ta.YoyoExit(14, 2.0),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "TypicalPrice": (
        lambda: ta.TypicalPrice(),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "MedianPrice": (
        lambda: ta.MedianPrice(),
        lambda ind, h, l, c, v: ind.batch(h, l),
    ),
    "WeightedClose": (
        lambda: ta.WeightedClose(),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "AcceleratorOscillator": (
        lambda: ta.AcceleratorOscillator(5, 34, 5),
        lambda ind, h, l, c, v: ind.batch(h, l),
    ),
    "AwesomeOscillatorHistogram": (
        lambda: ta.AwesomeOscillatorHistogram(5, 34, 5),
        lambda ind, h, l, c, v: ind.batch(h, l),
    ),
    "BalanceOfPower": (
        # The streaming 6-tuple feeds open == close, so batch matches with
        # the close column standing in for open.
        lambda: ta.BalanceOfPower(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "ChoppinessIndex": (
        lambda: ta.ChoppinessIndex(14),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "TrueRange": (
        lambda: ta.TrueRange(),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "ChaikinVolatility": (
        lambda: ta.ChaikinVolatility(10, 10),
        lambda ind, h, l, c, v: ind.batch(h, l),
    ),
    "ADXR": (
        lambda: ta.ADXR(7),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "ParkinsonVolatility": (
        lambda: ta.ParkinsonVolatility(20, 252),
        lambda ind, h, l, c, v: ind.batch(h, l),
    ),
    "GarmanKlassVolatility": (
        # The streaming 6-tuple feeds open == close, so batch matches with
        # the close column standing in for open.
        lambda: ta.GarmanKlassVolatility(20, 252),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "RogersSatchellVolatility": (
        lambda: ta.RogersSatchellVolatility(20, 252),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "YangZhangVolatility": (
        lambda: ta.YangZhangVolatility(20, 252),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "TDSetup": (
        lambda: ta.TDSetup(4, 9),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "TDDeMarker": (
        lambda: ta.TDDeMarker(14),
        lambda ind, h, l, c, v: ind.batch(h, l),
    ),
    "TDREI": (
        lambda: ta.TDREI(5),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "TDCombo": (
        lambda: ta.TDCombo(4, 9, 2, 13),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "TDCountdown": (
        lambda: ta.TDCountdown(4, 9, 2, 13),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    "TDDifferential": (
        lambda: ta.TDDifferential(),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
    ),
    # Candlestick patterns -- batch takes (open, high, low, close) and the
    # streaming side feeds close as open (same convention as BalanceOfPower).
    "Doji": (
        lambda: ta.Doji(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Hammer": (
        lambda: ta.Hammer(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "InvertedHammer": (
        lambda: ta.InvertedHammer(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "HangingMan": (
        lambda: ta.HangingMan(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "ShootingStar": (
        lambda: ta.ShootingStar(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Engulfing": (
        lambda: ta.Engulfing(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Harami": (
        lambda: ta.Harami(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "MorningEveningStar": (
        lambda: ta.MorningEveningStar(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "ThreeSoldiersOrCrows": (
        lambda: ta.ThreeSoldiersOrCrows(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "PiercingDarkCloud": (
        lambda: ta.PiercingDarkCloud(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Marubozu": (
        lambda: ta.Marubozu(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Tweezer": (
        lambda: ta.Tweezer(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "SpinningTop": (
        lambda: ta.SpinningTop(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "ThreeInside": (
        lambda: ta.ThreeInside(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "ThreeOutside": (
        lambda: ta.ThreeOutside(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "TwoCrows": (
        lambda: ta.TwoCrows(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "UpsideGapTwoCrows": (
        lambda: ta.UpsideGapTwoCrows(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "IdenticalThreeCrows": (
        lambda: ta.IdenticalThreeCrows(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "ThreeLineStrike": (
        lambda: ta.ThreeLineStrike(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "ThreeStarsInSouth": (
        lambda: ta.ThreeStarsInSouth(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "AbandonedBaby": (
        lambda: ta.AbandonedBaby(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "AdvanceBlock": (
        lambda: ta.AdvanceBlock(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "BeltHold": (
        lambda: ta.BeltHold(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Breakaway": (
        lambda: ta.Breakaway(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Counterattack": (
        lambda: ta.Counterattack(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "DojiStar": (
        lambda: ta.DojiStar(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "DragonflyDoji": (
        lambda: ta.DragonflyDoji(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "GravestoneDoji": (
        lambda: ta.GravestoneDoji(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "LongLeggedDoji": (
        lambda: ta.LongLeggedDoji(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "RickshawMan": (
        lambda: ta.RickshawMan(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "EveningDojiStar": (
        lambda: ta.EveningDojiStar(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "MorningDojiStar": (
        lambda: ta.MorningDojiStar(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "GapSideBySideWhite": (
        lambda: ta.GapSideBySideWhite(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "HighWave": (
        lambda: ta.HighWave(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Hikkake": (
        lambda: ta.Hikkake(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "HikkakeModified": (
        lambda: ta.HikkakeModified(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "HomingPigeon": (
        lambda: ta.HomingPigeon(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "OnNeck": (
        lambda: ta.OnNeck(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "InNeck": (
        lambda: ta.InNeck(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Thrusting": (
        lambda: ta.Thrusting(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "SeparatingLines": (
        lambda: ta.SeparatingLines(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Kicking": (
        lambda: ta.Kicking(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "KickingByLength": (
        lambda: ta.KickingByLength(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "LadderBottom": (
        lambda: ta.LadderBottom(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "MatHold": (
        lambda: ta.MatHold(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "MatchingLow": (
        lambda: ta.MatchingLow(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "LongLine": (
        lambda: ta.LongLine(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "ShortLine": (
        lambda: ta.ShortLine(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "RisingThreeMethods": (
        lambda: ta.RisingThreeMethods(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "FallingThreeMethods": (
        lambda: ta.FallingThreeMethods(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "UpsideGapThreeMethods": (
        lambda: ta.UpsideGapThreeMethods(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "DownsideGapThreeMethods": (
        lambda: ta.DownsideGapThreeMethods(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "StalledPattern": (
        lambda: ta.StalledPattern(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "StickSandwich": (
        lambda: ta.StickSandwich(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "Takuri": (
        lambda: ta.Takuri(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "ClosingMarubozu": (
        lambda: ta.ClosingMarubozu(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "OpeningMarubozu": (
        lambda: ta.OpeningMarubozu(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "TasukiGap": (
        lambda: ta.TasukiGap(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "UniqueThreeRiver": (
        lambda: ta.UniqueThreeRiver(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
    "ConcealingBabySwallow": (
        lambda: ta.ConcealingBabySwallow(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
    ),
}


@pytest.mark.parametrize("name", list(CANDLE_SCALAR))
def test_candle_scalar_streaming_matches_batch(name, ohlcv):
    high, low, close, volume = ohlcv
    make, batch_call = CANDLE_SCALAR[name]

    batch = batch_call(make(), high, low, close, volume)
    assert len(batch) == len(close)

    streamer = make()
    streamed = []
    for i in range(close.size):
        candle = (
            float(close[i]),
            float(high[i]),
            float(low[i]),
            float(close[i]),
            float(volume[i]),
            i,
        )
        v = streamer.update(candle)
        streamed.append(math.nan if v is None else float(v))
    assert _eq_nan(batch, np.array(streamed, dtype=np.float64)), f"{name} mismatch"


# --- Candle-input, multi-output indicators --------------------------------

MULTI = {
    "CompositeProfile": (
        lambda: ta.CompositeProfile(20, 24, 0.7),
        lambda ind, h, l, c, v: ind.batch(h, l, v),
        3,
    ),
    "HighLowVolumeNodes": (
        lambda: ta.HighLowVolumeNodes(20, 24),
        lambda ind, h, l, c, v: ind.batch(h, l, v),
        2,
    ),
    "CandleVolume": (
        lambda: ta.CandleVolume(20),
        lambda ind, h, l, c, v: ind.batch(c, c, v),
        2,
    ),
    "Equivolume": (
        lambda: ta.Equivolume(20),
        lambda ind, h, l, c, v: ind.batch(h, l, v),
        2,
    ),
    "SmoothedHeikinAshi": (
        lambda: ta.SmoothedHeikinAshi(5),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
        4,
    ),
    "TDMovingAverage": (
        lambda: ta.TDMovingAverage(5, 13),
        lambda ind, h, l, c, v: ind.batch(h, l),
        2,
    ),
    "VolumeWeightedSr": (
        lambda: ta.VolumeWeightedSr(3),
        lambda ind, h, l, c, v: ind.batch(h, l, v),
        2,
    ),
    "AndrewsPitchfork": (
        lambda: ta.AndrewsPitchfork(2),
        lambda ind, h, l, c, v: ind.batch(h, l),
        3,
    ),
    "MurreyMathLines": (
        lambda: ta.MurreyMathLines(4),
        lambda ind, h, l, c, v: ind.batch(h, l),
        9,
    ),
    "CentralPivotRange": (
        lambda: ta.CentralPivotRange(),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        3,
    ),
    "VolumeWeightedMacd": (
        lambda: ta.VolumeWeightedMacd(12, 26, 9),
        lambda ind, h, l, c, v: ind.batch(c, v),
        3,
    ),
    "ModifiedMaStop": (
        lambda: ta.ModifiedMaStop(14),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "Nrtr": (
        lambda: ta.Nrtr(2.0),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "AtrRatchet": (
        lambda: ta.AtrRatchet(14, 4.0, 0.1),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "ElderSafeZone": (
        lambda: ta.ElderSafeZone(14, 2.0),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "KaseDevStop": (
        lambda: ta.KaseDevStop(3, 1.0),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "ProjectionBands": (
        lambda: ta.ProjectionBands(3),
        lambda ind, h, l, c, v: ind.batch(h, l),
        3,
    ),
    "VolatilityCone": (
        lambda: ta.VolatilityCone(20, 60),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        5,
    ),
    "KasePermissionStochastic": (
        lambda: ta.KasePermissionStochastic(9, 3),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "GatorOscillator": (
        lambda: ta.GatorOscillator(13, 8, 5),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "ElderRay": (
        lambda: ta.ElderRay(13),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "FibFan": (
        lambda: ta.FibFan(),
        lambda ind, h, l, c, v: ind.batch(h, l),
        3,
    ),
    "FibArcs": (
        lambda: ta.FibArcs(),
        lambda ind, h, l, c, v: ind.batch(h, l),
        3,
    ),
    "FibChannel": (
        lambda: ta.FibChannel(),
        lambda ind, h, l, c, v: ind.batch(h, l),
        4,
    ),
    "FibTimeZones": (
        lambda: ta.FibTimeZones(),
        lambda ind, h, l, c, v: ind.batch(h, l),
        2,
    ),
    "FibRetracement": (
        lambda: ta.FibRetracement(),
        lambda ind, h, l, c, v: ind.batch(h, l),
        7,
    ),
    "FibExtension": (
        lambda: ta.FibExtension(),
        lambda ind, h, l, c, v: ind.batch(h, l),
        5,
    ),
    "FibProjection": (
        lambda: ta.FibProjection(),
        lambda ind, h, l, c, v: ind.batch(h, l),
        4,
    ),
    "AutoFib": (
        lambda: ta.AutoFib(),
        lambda ind, h, l, c, v: ind.batch(h, l),
        7,
    ),
    "GoldenPocket": (
        lambda: ta.GoldenPocket(),
        lambda ind, h, l, c, v: ind.batch(h, l),
        3,
    ),
    "FibConfluence": (
        lambda: ta.FibConfluence(),
        lambda ind, h, l, c, v: ind.batch(h, l),
        2,
    ),
    "Vortex": (
        lambda: ta.Vortex(14),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "RWI": (
        lambda: ta.RWI(14),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "WaveTrend": (
        lambda: ta.WaveTrend.classic(),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "SuperTrend": (
        lambda: ta.SuperTrend(10, 3.0),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "ChandelierExit": (
        lambda: ta.ChandelierExit(22, 3.0),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "ChandeKrollStop": (
        lambda: ta.ChandeKrollStop(10, 1.0, 9),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "ClassicPivots": (
        lambda: ta.ClassicPivots(),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        7,
    ),
    "FibonacciPivots": (
        lambda: ta.FibonacciPivots(),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        7,
    ),
    "Camarilla": (
        lambda: ta.Camarilla(),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        9,
    ),
    "WoodiePivots": (
        lambda: ta.WoodiePivots(),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        5,
    ),
    "DemarkPivots": (
        # batch needs open; pass close in for open since the synthetic OHLCV
        # streaming feeds open=close as well.
        lambda: ta.DemarkPivots(),
        lambda ind, h, l, c, v: ind.batch(c, h, l, c),
        3,
    ),
    "WilliamsFractals": (
        lambda: ta.WilliamsFractals(),
        lambda ind, h, l, c, v: ind.batch(h, l),
        2,
    ),
    "ZigZag": (
        lambda: ta.ZigZag(0.02),
        lambda ind, h, l, c, v: ind.batch(h, l),
        2,
    ),
    "DonchianStop": (
        lambda: ta.DonchianStop(10),
        lambda ind, h, l, c, v: ind.batch(h, l),
        2,
    ),
    # Family 05 candle-input bands. Each entry is
    # `(factory, batch_call, output_arity)` where the third element is the
    # tuple shape returned by `update(...)`.
    "TtmSqueeze": (
        lambda: ta.TtmSqueeze(20, 2.0, 1.5),
        lambda ind, h, l, c, v: ind.batch(h, l, c),
        2,
    ),
    "FractalChaosBands": (
        lambda: ta.FractalChaosBands(2),
        lambda ind, h, l, c, v: ind.batch(h, l),
        2,
    ),
}


# Bands with 3 outputs upper/middle/lower from a candle (h, l, c).
HLC_BAND3 = {
    "AccelerationBands": lambda: ta.AccelerationBands(20, 4.0),
    "StarcBands": lambda: ta.StarcBands(6, 15, 2.0),
    "AtrBands": lambda: ta.AtrBands(14, 3.0),
    "HurstChannel": lambda: ta.HurstChannel(10, 0.5),
}

# --- Scalar-input, multi-output indicators --------------------------------
#
# Same shape contract as MULTI (batch returns (n, 2)) but streaming feeds a
# single float instead of a candle tuple.

MULTI_SCALAR_INPUT = {
    "KST": (
        lambda: ta.KST(10, 15, 20, 30, 10, 10, 10, 15, 9),
        lambda ind, c: ind.batch(c),
    ),
    "MAMA": (
        lambda: ta.MAMA(0.5, 0.05),
        lambda ind, c: ind.batch(c),
    ),
}


@pytest.mark.parametrize("name", list(MULTI))
def test_multi_streaming_matches_batch(name, ohlcv):
    high, low, close, volume = ohlcv
    make, batch_call, k = MULTI[name]

    batch = batch_call(make(), high, low, close, volume)
    assert batch.shape == (close.size, k)

    streamer = make()
    rows = []
    for i in range(close.size):
        candle = (
            float(close[i]),
            float(high[i]),
            float(low[i]),
            float(close[i]),
            float(volume[i]),
            i,
        )
        v = streamer.update(candle)
        if v is None:
            rows.append([math.nan] * k)
        else:
            # WilliamsFractals returns (Optional[float], Optional[float]); the
            # batch helper writes NaN for None. Normalise tuple/list entries
            # the same way before the equality check.
            rows.append([math.nan if x is None else float(x) for x in v])
    assert _eq_nan(batch, np.array(rows, dtype=np.float64)), f"{name} mismatch"


# --- Family 05: scalar-input multi-output band/channel indicators ----------


@pytest.mark.parametrize("name", list(SCALAR_MULTI))
def test_scalar_multi_streaming_matches_batch(name, sine_prices):
    make, cols = SCALAR_MULTI[name]
    batch = make().batch(sine_prices)
    assert batch.shape == (sine_prices.size, cols)

    streamer = make()
    rows = []
    for p in sine_prices:
        v = streamer.update(float(p))
        rows.append([math.nan] * cols if v is None else list(v))
    assert _eq_nan(batch, np.array(rows, dtype=np.float64)), f"{name} mismatch"


# --- Family 05: 3-band candle-input indicators ------------------------------


@pytest.mark.parametrize("name", list(HLC_BAND3))
def test_hlc_band3_streaming_matches_batch(name, ohlcv):
    high, low, close, _ = ohlcv
    make = HLC_BAND3[name]
    batch = make().batch(high, low, close)
    assert batch.shape == (close.size, 3)

    streamer = make()
    rows = []
    for i in range(close.size):
        candle = (
            float(close[i]),
            float(high[i]),
            float(low[i]),
            float(close[i]),
            1.0,
            i,
        )
        v = streamer.update(candle)
        rows.append([math.nan] * 3 if v is None else list(v))
    assert _eq_nan(batch, np.array(rows, dtype=np.float64)), f"{name} mismatch"


# --- VWAP StdDev Bands (4 outputs, needs volume) ----------------------------


def test_vwap_stddev_bands_streaming_matches_batch(ohlcv):
    high, low, close, volume = ohlcv
    batch = ta.VwapStdDevBands(2.0).batch(high, low, close, volume)
    assert batch.shape == (close.size, 4)

    streamer = ta.VwapStdDevBands(2.0)
    rows = []
    for i in range(close.size):
        candle = (
            float(close[i]),
            float(high[i]),
            float(low[i]),
            float(close[i]),
            float(volume[i]),
            i,
        )
        v = streamer.update(candle)
        rows.append([math.nan] * 4 if v is None else list(v))
    assert _eq_nan(batch, np.array(rows, dtype=np.float64))


@pytest.mark.parametrize("name", list(MULTI_SCALAR_INPUT))
def test_multi_scalar_streaming_matches_batch(name, ohlcv):
    _, _, close, _ = ohlcv
    make, batch_call = MULTI_SCALAR_INPUT[name]

    batch = batch_call(make(), close)
    assert batch.shape == (close.size, 2)

    streamer = make()
    rows = []
    for p in close:
        v = streamer.update(float(p))
        rows.append([math.nan, math.nan] if v is None else list(v))
    assert _eq_nan(batch, np.array(rows, dtype=np.float64)), f"{name} mismatch"


# --- Market Profile (multi-output with variable shapes) ------------------


def test_value_area_shape_and_streaming(ohlcv):
    high, low, _close, volume = ohlcv
    batch = ta.ValueArea(20, 50, 0.70).batch(high, low, volume)
    assert batch.shape == (high.size, 3)
    streamer = ta.ValueArea(20, 50, 0.70)
    rows = []
    for i in range(high.size):
        mid = float((high[i] + low[i]) / 2.0)
        candle = (mid, float(high[i]), float(low[i]), mid, float(volume[i]), i)
        v = streamer.update(candle)
        rows.append([math.nan, math.nan, math.nan] if v is None else list(v))
    assert _eq_nan(batch, np.array(rows, dtype=np.float64))


def test_initial_balance_shape_and_streaming(ohlcv):
    high, low, _close, _volume = ohlcv
    batch = ta.InitialBalance(12).batch(high, low)
    assert batch.shape == (high.size, 2)
    streamer = ta.InitialBalance(12)
    rows = []
    for i in range(high.size):
        mid = float((high[i] + low[i]) / 2.0)
        candle = (mid, float(high[i]), float(low[i]), mid, 0.0, i)
        v = streamer.update(candle)
        rows.append([math.nan, math.nan] if v is None else list(v))
    assert _eq_nan(batch, np.array(rows, dtype=np.float64))


def test_opening_range_shape_and_streaming(ohlcv):
    high, low, close, _volume = ohlcv
    batch = ta.OpeningRange(6).batch(high, low, close)
    assert batch.shape == (high.size, 3)
    streamer = ta.OpeningRange(6)
    rows = []
    for i in range(high.size):
        candle = (
            float(close[i]),
            float(high[i]),
            float(low[i]),
            float(close[i]),
            0.0,
            i,
        )
        v = streamer.update(candle)
        rows.append([math.nan, math.nan, math.nan] if v is None else list(v))
    assert _eq_nan(batch, np.array(rows, dtype=np.float64))


def test_volume_profile_reference():
    # bar0 single-print at 10 vol 100; bar1 spans 10..14 vol 80 over 4 bins.
    vp = ta.VolumeProfile(2, 4)
    assert vp.update((10.0, 10.0, 10.0, 10.0, 100.0, 0)) is None
    out = vp.update((10.0, 14.0, 10.0, 12.0, 80.0, 1))
    assert out is not None
    price_low, price_high, bins = out
    assert price_low == pytest.approx(10.0)
    assert price_high == pytest.approx(14.0)
    np.testing.assert_allclose(bins, [120.0, 20.0, 20.0, 20.0])


def test_volume_profile_streaming_matches_batch(ohlcv):
    high, low, close, volume = ohlcv
    batch = ta.VolumeProfile(10, 8).batch(high, low, volume)
    assert batch.shape == (high.size, 10)
    streamer = ta.VolumeProfile(10, 8)
    for i in range(high.size):
        mid = float((high[i] + low[i]) / 2)
        out = streamer.update((mid, float(high[i]), float(low[i]), mid, float(volume[i]), i))
        if out is None:
            assert np.isnan(batch[i]).all()
        else:
            pl, ph, bins = out
            assert pl == pytest.approx(batch[i][0])
            assert ph == pytest.approx(batch[i][1])
            np.testing.assert_allclose(bins, batch[i][2:])


def test_tpo_profile_reference():
    # bar0 spans 10..14 (+1 to all 4 bins); bar1 spans 11..12 (+1 to bins 1,2).
    tpo = ta.TpoProfile(2, 4)
    assert tpo.update((12.0, 14.0, 10.0, 12.0, 5.0, 0)) is None
    out = tpo.update((11.5, 12.0, 11.0, 11.5, 999.0, 1))
    assert out is not None
    price_low, price_high, counts = out
    assert price_low == pytest.approx(10.0)
    assert price_high == pytest.approx(14.0)
    np.testing.assert_allclose(counts, [1.0, 2.0, 2.0, 1.0])


def test_tpo_profile_streaming_matches_batch(ohlcv):
    high, low, close, volume = ohlcv
    batch = ta.TpoProfile(10, 8).batch(high, low)
    assert batch.shape == (high.size, 10)
    streamer = ta.TpoProfile(10, 8)
    for i in range(high.size):
        mid = float((high[i] + low[i]) / 2)
        out = streamer.update((mid, float(high[i]), float(low[i]), mid, 1.0, i))
        if out is None:
            assert np.isnan(batch[i]).all()
        else:
            pl, ph, counts = out
            assert pl == pytest.approx(batch[i][0])
            assert ph == pytest.approx(batch[i][1])
            np.testing.assert_allclose(counts, batch[i][2:])


# --- TD Pressure (OHLCV-input) -------------------------------------------


def test_td_pressure_streaming_matches_batch(ohlcv):
    high, low, close, volume = ohlcv
    open_ = close.copy()  # TD Pressure needs open; reuse close as the open column.
    batch = ta.TDPressure(5).batch(open_, high, low, close, volume)
    assert len(batch) == len(close)

    streamer = ta.TDPressure(5)
    streamed = []
    for i in range(close.size):
        candle = (
            float(open_[i]),
            float(high[i]),
            float(low[i]),
            float(close[i]),
            float(volume[i]),
            i,
        )
        v = streamer.update(candle)
        streamed.append(math.nan if v is None else float(v))
    assert _eq_nan(batch, np.array(streamed, dtype=np.float64))


# --- TD Sequential (3-column multi-output) ------------------------------


def test_td_sequential_streaming_matches_batch(ohlcv):
    high, low, close, volume = ohlcv
    batch = ta.TDSequential().batch(high, low, close)
    assert batch.shape == (close.size, 3)

    streamer = ta.TDSequential()
    rows = []
    for i in range(close.size):
        candle = (
            float(close[i]),
            float(high[i]),
            float(low[i]),
            float(close[i]),
            float(volume[i]),
            i,
        )
        v = streamer.update(candle)
        rows.append([math.nan, math.nan, math.nan] if v is None else list(v))
    assert _eq_nan(batch, np.array(rows, dtype=np.float64))


# --- ZeroLagMACD (scalar input, 3-tuple output: macd / signal / histogram) -


def test_zero_lag_macd_streaming_matches_batch(ohlcv):
    _, _, close, _ = ohlcv
    batch = ta.ZeroLagMACD(12, 26, 9).batch(close)
    assert batch.shape == (close.size, 3)

    streamer = ta.ZeroLagMACD(12, 26, 9)
    rows = []
    for p in close:
        v = streamer.update(float(p))
        rows.append([math.nan, math.nan, math.nan] if v is None else list(v))
    assert _eq_nan(batch, np.array(rows, dtype=np.float64)), "ZeroLagMACD mismatch"


# --- Alligator (3-tuple output) -------------------------------------------


def test_alligator_streaming_matches_batch(ohlcv):
    high, low, _, _ = ohlcv
    alligator = ta.Alligator(13, 8, 5)
    batch = alligator.batch(high, low)
    assert batch.shape == (high.size, 3)

    streamer = ta.Alligator(13, 8, 5)
    rows = []
    for i in range(high.size):
        candle = (float(low[i]), float(high[i]), float(low[i]), float(low[i]), 0.0, i)
        v = streamer.update(candle)
        rows.append([math.nan, math.nan, math.nan] if v is None else list(v))
    assert _eq_nan(batch, np.array(rows, dtype=np.float64)), "Alligator mismatch"


# --- Reference values -----------------------------------------------------


def test_typical_price_reference():
    # (high + low + close) / 3 = (12 + 6 + 9) / 3 = 9.
    assert ta.TypicalPrice().update((9.0, 12.0, 6.0, 9.0, 1.0, 0)) == pytest.approx(9.0)


def test_median_price_reference():
    # (high + low) / 2 = (12 + 8) / 2 = 10.
    assert ta.MedianPrice().update((10.0, 12.0, 8.0, 11.0, 1.0, 0)) == pytest.approx(10.0)


def test_weighted_close_reference():
    # (high + low + 2*close) / 4 = (12 + 8 + 22) / 4 = 10.5.
    assert ta.WeightedClose().update((10.0, 12.0, 8.0, 11.0, 1.0, 0)) == pytest.approx(
        10.5
    )


def test_plus_dm_reference():
    # Highs rise by 1 (up = +1) while lows rise by 0.5, so every raw +DM equals
    # the up-move (1.0). Period 3: seed = 3 * 1 = 3.0, then the Wilder step holds it.
    high = np.array([11.0, 12.0, 13.0, 14.0, 15.0])
    low = np.array([9.0, 9.5, 10.0, 10.5, 11.0])
    close = np.array([10.0, 11.0, 12.0, 13.0, 14.0])
    out = _to_np(ta.PLUS_DM(3).batch(high, low, close))
    assert math.isnan(out[0]) and math.isnan(out[2])
    assert out[3] == pytest.approx(3.0)
    assert out[4] == pytest.approx(3.0)


def test_minus_dm_reference():
    # Lows fall by 1 (down = +1) while highs fall by 0.5, so every raw -DM equals
    # the down-move (1.0). Period 3: seed = 3 * 1 = 3.0, then the Wilder step holds it.
    high = np.array([20.0, 19.5, 19.0, 18.5, 18.0])
    low = np.array([18.0, 17.0, 16.0, 15.0, 14.0])
    close = np.array([19.0, 18.0, 17.0, 16.0, 15.0])
    out = _to_np(ta.MINUS_DM(3).batch(high, low, close))
    assert math.isnan(out[0]) and math.isnan(out[2])
    assert out[3] == pytest.approx(3.0)
    assert out[4] == pytest.approx(3.0)


def test_plus_di_reference():
    # Strict uptrend -> +DI dominates and stays within (0, 100].
    high = np.array([101.0, 103.0, 105.0, 107.0, 109.0, 111.0])
    low = np.array([99.5, 101.5, 103.5, 105.5, 107.5, 109.5])
    close = np.array([100.5, 102.5, 104.5, 106.5, 108.5, 110.5])
    out = _to_np(ta.PLUS_DI(3).batch(high, low, close))
    assert 0.0 < out[-1] <= 100.0


def test_minus_di_reference():
    # Strict downtrend -> -DI dominates and stays within (0, 100].
    high = np.array([111.0, 109.0, 107.0, 105.0, 103.0, 101.0])
    low = np.array([109.5, 107.5, 105.5, 103.5, 101.5, 99.5])
    close = np.array([110.5, 108.5, 106.5, 104.5, 102.5, 100.5])
    out = _to_np(ta.MINUS_DI(3).batch(high, low, close))
    assert 0.0 < out[-1] <= 100.0


def test_dx_reference():
    # Strict trend -> one-sided directional movement -> DX is large, in (0, 100].
    high = np.array([101.0, 103.0, 105.0, 107.0, 109.0, 111.0])
    low = np.array([99.5, 101.5, 103.5, 105.5, 107.5, 109.5])
    close = np.array([100.5, 102.5, 104.5, 106.5, 108.5, 110.5])
    out = _to_np(ta.DX(3).batch(high, low, close))
    assert 50.0 < out[-1] <= 100.0


def test_mid_price_reference():
    # Window highs {12, 14, 16}, lows {8, 9, 10}: (16 + 8) / 2 = 12.
    high = np.array([12.0, 14.0, 16.0])
    low = np.array([8.0, 9.0, 10.0])
    close = np.array([10.0, 11.0, 12.0])
    out = _to_np(ta.MIDPRICE(3).batch(high, low, close))
    assert out[-1] == pytest.approx(12.0)


def test_mid_point_reference():
    # Window {8, 12, 10}: (12 + 8) / 2 = 10.
    out = _to_np(ta.MIDPOINT(3).batch(np.array([8.0, 12.0, 10.0])))
    assert out[-1] == pytest.approx(10.0)


def test_avg_price_reference():
    # (open + high + low + close) / 4 = (10 + 14 + 6 + 12) / 4 = 10.5.
    assert ta.AVGPRICE().update((10.0, 14.0, 6.0, 12.0, 1.0, 0)) == pytest.approx(10.5)


def test_roc_ratio_variants_reference():
    # period 1 over [10, 11]: ROCP = 0.1, ROCR = 1.1, ROCR100 = 110.
    assert ta.ROCP(1).batch(np.array([10.0, 11.0]))[-1] == pytest.approx(0.1)
    assert ta.ROCR(1).batch(np.array([10.0, 11.0]))[-1] == pytest.approx(1.1)
    assert ta.ROCR100(1).batch(np.array([10.0, 11.0]))[-1] == pytest.approx(110.0)


def test_linreg_intercept_and_tsf_reference():
    # period 3 over [1, 2, 9]: fit y = 0 + 4x. intercept = 0; forecast at x=3 = 12.
    data = np.array([1.0, 2.0, 9.0])
    assert ta.LINEARREG_INTERCEPT(3).batch(data)[-1] == pytest.approx(0.0, abs=1e-9)
    assert ta.TSF(3).batch(data)[-1] == pytest.approx(12.0)


def test_macdfix_uses_the_fixed_smoothing_constants():
    # MACDFIX(signal) is TA-Lib's MACDFIX: the 12/26 EMAs smooth with the fixed
    # constants 0.15 / 0.075 (each seeded with its simple mean).
    prices = 100.0 + np.sin(np.arange(80) * 0.3) * 5.0

    def ema(n, a):
        out = np.full(len(prices), np.nan)
        v = prices[:n].mean()
        out[n - 1] = v
        for i in range(n, len(prices)):
            v = a * prices[i] + (1.0 - a) * v
            out[i] = v
        return out

    macd_line = ema(12, 0.15) - ema(26, 0.075)
    fix = _to_np(ta.MACDFIX(9).batch(prices)).reshape(-1, 3)
    ready = ~np.isnan(fix[:, 0])
    assert ready.any()
    np.testing.assert_allclose(fix[ready, 0], macd_line[ready], atol=1e-9)
    classic = _to_np(ta.MACD(12, 26, 9).batch(prices)).reshape(-1, 3)
    assert not np.allclose(fix[ready, 0], classic[ready, 0])


def test_nvi_reference():
    # closes [10, 11], volumes [200, 100]: volume contracts -> NVI absorbs +10%.
    # 1000 * (1 + 0.1) = 1100.
    nvi = ta.NVI()
    out = _to_np(nvi.batch(np.array([10.0, 11.0]), np.array([200.0, 100.0])))
    assert out[0] == pytest.approx(1000.0)
    assert out[1] == pytest.approx(1100.0)


def test_pvi_reference():
    # closes [10, 11], volumes [100, 200]: volume expands -> PVI absorbs +10%.
    pvi = ta.PVI()
    out = _to_np(pvi.batch(np.array([10.0, 11.0]), np.array([100.0, 200.0])))
    assert out[0] == pytest.approx(1000.0)
    assert out[1] == pytest.approx(1100.0)


def test_volume_oscillator_reference():
    # fast=2, slow=4 over volumes [10, 20, 30, 40, 50]:
    # bar 4 -> fast=(30+40)/2=35, slow=(10+20+30+40)/4=25 -> VO = 100*(35-25)/25 = 40.
    vo = ta.VolumeOscillator(2, 4)
    out = _to_np(vo.batch(np.array([10.0, 20.0, 30.0, 40.0, 50.0])))
    assert math.isnan(out[2])
    assert out[3] == pytest.approx(40.0)
    assert out[4] == pytest.approx(1000.0 / 35.0)


def test_kvo_constant_series_is_zero():
    # A flat series produces dm with no sign change; vf collapses to 0 every
    # bar and both EMAs hold at 0, so the KVO line stays at 0.
    kvo = ta.KVO(3, 6)
    high = np.full(60, 10.0)
    low = np.full(60, 10.0)
    close = np.full(60, 10.0)
    volume = np.full(60, 100.0)
    out = _to_np(kvo.batch(high, low, close, volume))
    for v in out[~np.isnan(out)]:
        assert v == pytest.approx(0.0, abs=1e-12)


def test_wad_reference():
    # bar 0 seeds prev_close = 10.
    # bar 1: prev=10, today high=13, low=8, close=12 (up day).
    #   TR_l = min(10, 8) = 8 -> delta = 12 - 8 = 4. AD = 4.
    # bar 2: prev=12, today high=11, low=7, close=7 (down day).
    #   TR_h = max(12, 11) = 12 -> delta = 7 - 12 = -5. AD = 4 - 5 = -1.
    ad = ta.Wad()
    high = np.array([11.0, 13.0, 11.0])
    low = np.array([9.0, 8.0, 7.0])
    close = np.array([10.0, 12.0, 7.0])
    out = _to_np(ad.batch(high, low, close))
    assert math.isnan(out[0])
    assert out[1] == pytest.approx(4.0)
    assert out[2] == pytest.approx(-1.0)


def test_anchored_vwap_reference():
    # Three flat-OHLC bars: typical_price equals price.
    # 10@1, 20@1, 30@1 -> mean = 20.
    avwap = ta.AnchoredVWAP()
    high = np.array([10.0, 20.0, 30.0])
    low = np.array([10.0, 20.0, 30.0])
    close = np.array([10.0, 20.0, 30.0])
    volume = np.array([1.0, 1.0, 1.0])
    out = _to_np(avwap.batch(high, low, close, volume))
    assert out[2] == pytest.approx(20.0)


def test_anchored_vwap_set_anchor_clears_window():
    # Drive a few flat bars, re-anchor, then drive a high-priced bar:
    # the new running mean must equal the new bar's typical price.
    avwap = ta.AnchoredVWAP()
    for _ in range(3):
        avwap.update((10.0, 10.0, 10.0, 10.0, 1.0, 0))
    assert avwap.is_ready()
    avwap.set_anchor()
    v = avwap.update((100.0, 100.0, 100.0, 100.0, 5.0, 1))
    assert v == pytest.approx(100.0)


def test_anchored_rsi_reference():
    # prices 10 -> 11 (+1) -> 9 (-2) -> 12 (+3); cumulative anchored RSI.
    # bar2: sum_gain=1, sum_loss=2 -> rs=0.5 -> 100 - 100/1.5 = 33.3333
    # bar3: sum_gain=4, sum_loss=2 -> rs=2.0 -> 100 - 100/3   = 66.6667
    rsi = ta.AnchoredRSI()
    out = _to_np(rsi.batch(np.array([10.0, 11.0, 9.0, 12.0])))
    assert np.isnan(out[0])
    assert out[1] == pytest.approx(100.0)
    assert out[2] == pytest.approx(33.333333, abs=1e-4)
    assert out[3] == pytest.approx(66.666666, abs=1e-4)


def test_anchored_rsi_set_anchor_clears_window():
    # Downtrend reads 0; after re-anchor an uptrend must read a fresh 100.
    rsi = ta.AnchoredRSI()
    for p in (20.0, 19.0, 18.0, 17.0):
        rsi.update(p)
    assert rsi.is_ready()
    assert rsi.value == pytest.approx(0.0)
    rsi.set_anchor()
    assert rsi.update(50.0) is None
    assert rsi.update(51.0) == pytest.approx(100.0)


def test_anchored_rsi_streaming_matches_batch():
    prices = np.array([100.0 + np.sin(i * 0.4) * 8.0 for i in range(60)])
    batched = ta.AnchoredRSI().batch(prices)
    streamer = ta.AnchoredRSI()
    streamed = [streamer.update(float(p)) for p in prices]
    for b, s in zip(batched, streamed):
        if np.isnan(b):
            assert s is None
        else:
            assert s == pytest.approx(b)


def test_tsv_reference():
    # closes  = [10, 11, 13, 12, 14, 15]
    # volumes = [50, 100, 200, 150,  50, 200]
    # flows   = [None, 1*100=100, 2*200=400, -1*150=-150, 2*50=100, 1*200=200]
    # period=3: first emission at index 3.
    #   bar 3 window=[100,400,-150] -> 350
    #   bar 4 window=[400,-150,100] -> 350
    #   bar 5 window=[-150,100,200] -> 150
    tsv = ta.TSV(3)
    close = np.array([10.0, 11.0, 13.0, 12.0, 14.0, 15.0])
    volume = np.array([50.0, 100.0, 200.0, 150.0, 50.0, 200.0])
    out = _to_np(tsv.batch(close, volume))
    assert math.isnan(out[0]) and math.isnan(out[1]) and math.isnan(out[2])
    assert out[3] == pytest.approx(350.0)
    assert out[4] == pytest.approx(350.0)
    assert out[5] == pytest.approx(150.0)


def test_vzo_strictly_rising_saturates_to_plus_100():
    # Every bar is an up-day with identical volume -> signed_volume == volume,
    # so the smoothed signed-volume EMA equals the smoothed total-volume EMA,
    # giving a ratio of 1 -> VZO = +100.
    vzo = ta.VZO(5)
    close = np.array([10.0 + i for i in range(60)])
    volume = np.full(60, 100.0)
    out = _to_np(vzo.batch(close, volume))
    last = out[~np.isnan(out)][-1]
    assert last == pytest.approx(100.0)


def test_market_facilitation_index_reference():
    # (high - low) / volume = (12 - 8) / 200 = 0.02.
    mfi_bw = ta.MarketFacilitationIndex()
    high = np.array([12.0])
    low = np.array([8.0])
    volume = np.array([200.0])
    out = _to_np(mfi_bw.batch(high, low, volume))
    assert out[0] == pytest.approx(0.02)


def test_demand_index_constant_series_is_zero():
    # Flat close -> pressure = 0 every bar -> EMA stays at 0.
    di = ta.DemandIndex(5)
    high = np.full(60, 10.0)
    low = np.full(60, 10.0)
    close = np.full(60, 10.0)
    volume = np.full(60, 100.0)
    out = _to_np(di.batch(high, low, close, volume))
    for v in out[~np.isnan(out)]:
        assert v == pytest.approx(0.0, abs=1e-12)


def test_chaikin_money_flow_reference():
    cmf = ta.ChaikinMoneyFlow(2)
    assert cmf.update((8.0, 10.0, 8.0, 10.0, 100.0, 0)) is None
    assert cmf.update((10.0, 12.0, 8.0, 10.0, 100.0, 1)) == pytest.approx(0.5)


def test_linear_regression_reference():
    out = _to_np(ta.LinearRegression(3).batch(np.array([1.0, 2.0, 9.0])))
    assert math.isnan(out[0]) and math.isnan(out[1])
    assert out[2] == pytest.approx(8.0)


def test_linreg_slope_reference():
    out = _to_np(ta.LinRegSlope(3).batch(np.array([1.0, 2.0, 9.0])))
    assert math.isnan(out[0]) and math.isnan(out[1])
    assert out[2] == pytest.approx(4.0)


def test_balance_of_power_reference():
    # (close - open) / (high - low) = (12 - 10) / (14 - 10) = 0.5.
    bop = ta.BalanceOfPower()
    assert bop.update((10.0, 14.0, 10.0, 12.0, 1.0, 0)) == pytest.approx(0.5)


def test_true_range_reference():
    tr = ta.TrueRange()
    assert tr.update((11.0, 12.0, 8.0, 11.0, 1.0, 0)) == pytest.approx(4.0)
    assert tr.update((9.5, 10.0, 9.0, 9.5, 1.0, 1)) == pytest.approx(2.0)


def test_linreg_angle_reference():
    # A series rising by 1 per step has slope 1, and atan(1) = 45 degrees.
    out = _to_np(ta.LinRegAngle(5).batch(np.array([1.0, 2.0, 3.0, 4.0, 5.0, 6.0])))
    assert out[4] == pytest.approx(45.0)


def test_wave_trend_flat_market_yields_zero():
    # On a perfectly flat market the flat-tolerance guard keeps both lines
    # at exactly zero (otherwise the ratio ci = (ap - esa) / (0.015 * d)
    # would explode on the first esa ULP).
    out = _to_np(ta.WaveTrend.classic().batch(
        np.full(80, 10.0), np.full(80, 10.0), np.full(80, 10.0)
    ))
    last = out[~np.isnan(out[:, 0])][-1]
    assert last[0] == 0.0
    assert last[1] == 0.0


def test_kst_classic_constants_yield_zero():
    out = _to_np(ta.KST.classic().batch(np.full(120, 100.0)))
    last_row = out[~np.isnan(out[:, 0])][-1]
    assert last_row[0] == pytest.approx(0.0)
    assert last_row[1] == pytest.approx(0.0)


def test_tii_pure_uptrend_saturates_at_100():
    # On a strictly increasing series every close sits above the lagging
    # SMA, so every deviation is positive and TII reaches 100.
    prices = np.arange(80, dtype=np.float64) + 100.0
    out = _to_np(ta.TII(10, 5).batch(prices))
    last = out[~np.isnan(out)][-1]
    assert last == pytest.approx(100.0)


def test_tii_flat_market_yields_50():
    out = _to_np(ta.TII(5, 4).batch(np.full(30, 10.0)))
    last = out[~np.isnan(out)][-1]
    assert last == 50.0


def test_rwi_reference_uptrend_dominates_low_line():
    # In a pure linear uptrend RWI_High >> RWI_Low.
    n = 60
    base = np.arange(n, dtype=np.float64) * 2.0 + 100.0
    high = base + 1.0
    low = base - 0.5
    close = base + 0.5
    out = _to_np(ta.RWI(14).batch(high, low, close))
    last_row = out[~np.isnan(out[:, 0])][-1]
    assert last_row[0] > last_row[1], f"RWI_High {last_row[0]} must dominate RWI_Low {last_row[1]}"
    assert last_row[0] > 1.0


def test_adxr_reference_on_pure_uptrend():
    # On a pure linear uptrend ADX saturates at 100, so ADXR (average of two
    # saturated ADX values period-1 bars apart) also reads 100.
    n = 100
    base = np.arange(n, dtype=np.float64) * 2.0 + 100.0
    high = base + 1.0
    low = base - 0.5
    close = base + 0.5
    out = _to_np(ta.ADXR(5).batch(high, low, close))
    last = out[~np.isnan(out)][-1]
    assert last == pytest.approx(100.0)


def test_z_score_reference():
    # Window [1, 3]: mean 2, population stddev 1; latest 3 -> z = 1.
    out = _to_np(ta.ZScore(2).batch(np.array([1.0, 3.0])))
    assert math.isnan(out[0])
    assert out[1] == pytest.approx(1.0)


# --- Family 12: Statistik / Regression reference values ------------------


def test_variance_reference():
    # Variance(3) of [2, 4, 6]: mean 4, variance (4 + 0 + 4) / 3 = 8/3.
    out = _to_np(ta.Variance(3).batch(np.array([2.0, 4.0, 6.0])))
    assert math.isnan(out[1])
    assert out[2] == pytest.approx(8.0 / 3.0)


def test_coefficient_of_variation_reference():
    # CV(3) of [2, 4, 6]: sd / mean = sqrt(8/3) / 4.
    out = _to_np(ta.CoefficientOfVariation(3).batch(np.array([2.0, 4.0, 6.0])))
    assert out[2] == pytest.approx(math.sqrt(8.0 / 3.0) / 4.0)


def test_skewness_symmetric_window_is_zero():
    # Symmetric window has zero Pearson skewness.
    out = _to_np(ta.Skewness(5).batch(np.array([-2.0, -1.0, 0.0, 1.0, 2.0])))
    assert out[4] == pytest.approx(0.0, abs=1e-9)


def test_kurtosis_two_point_distribution_minimum():
    # Alternating {-1, 1} has m4/m2² = 1, so excess kurtosis = -2.
    out = _to_np(ta.Kurtosis(4).batch(np.array([-1.0, 1.0, -1.0, 1.0])))
    assert out[3] == pytest.approx(-2.0, abs=1e-9)


def test_standard_error_perfect_line_is_zero():
    # Residuals are zero on a perfectly linear series.
    out = _to_np(ta.StandardError(5).batch(np.linspace(1.0, 20.0, num=20, dtype=np.float64)))
    finite = out[~np.isnan(out)]
    assert np.allclose(finite, 0.0, atol=1e-9)


def test_detrended_std_dev_perfect_line_is_zero():
    out = _to_np(ta.DetrendedStdDev(5).batch(np.linspace(1.0, 20.0, num=20, dtype=np.float64)))
    finite = out[~np.isnan(out)]
    assert np.allclose(finite, 0.0, atol=1e-9)


def test_r_squared_perfect_line_is_one():
    out = _to_np(ta.RSquared(5).batch(np.linspace(1.0, 20.0, num=20, dtype=np.float64)))
    finite = out[~np.isnan(out)]
    assert np.allclose(finite, 1.0, atol=1e-9)


def test_median_absolute_deviation_ignores_single_outlier():
    # 9 equal values + 1 huge outlier: MAD is still 0 (more than half agree).
    prices = np.array([5.0] * 9 + [1000.0], dtype=np.float64)
    out = _to_np(ta.MedianAbsoluteDeviation(10).batch(prices))
    assert out[9] == pytest.approx(0.0, abs=1e-12)


def test_autocorrelation_alternating_series_negative():
    # ±1 alternating: lag-1 ACF must be strongly negative.
    prices = np.array([-1.0 if i % 2 == 0 else 1.0 for i in range(20)], dtype=np.float64)
    out = _to_np(ta.Autocorrelation(10, 1).batch(prices))
    assert out[-1] < -0.5


def test_hurst_exponent_trending_above_half():
    # A clean monotone ramp is the textbook persistent series.
    prices = np.arange(200, dtype=np.float64)
    out = _to_np(ta.HurstExponent(100, 4).batch(prices))
    assert out[-1] > 0.5


def test_pearson_correlation_perfect_positive_is_one():
    x = np.arange(10, dtype=np.float64)
    y = 2.0 * x + 3.0
    out = _to_np(ta.PearsonCorrelation(5).batch(x, y))
    assert out[-1] == pytest.approx(1.0, abs=1e-9)


def test_beta_perfect_two_to_one():
    benchmark = np.arange(10, dtype=np.float64)
    asset = 2.0 * benchmark
    out = _to_np(ta.Beta(5).batch(asset, benchmark))
    assert out[-1] == pytest.approx(2.0, abs=1e-9)


def test_spearman_correlation_monotone_nonlinear_is_one():
    # y = x^3 is monotone non-linear; Spearman = 1 (Pearson would not be).
    x = np.arange(1.0, 11.0, dtype=np.float64)
    y = x**3
    out = _to_np(ta.SpearmanCorrelation(5).batch(x, y))
    assert out[-1] == pytest.approx(1.0, abs=1e-9)


def test_pair_indicators_streaming_matches_batch():
    rng = np.linspace(0.0, 10.0, num=60, dtype=np.float64)
    x = np.sin(rng) + 0.1 * rng
    y = np.cos(rng * 0.3) + 0.05 * rng

    for cls, args in [(ta.PearsonCorrelation, (14,)), (ta.Beta, (14,)), (ta.SpearmanCorrelation, (14,))]:
        batch = cls(*args).batch(x, y)
        streamer = cls(*args)
        streamed = []
        for i in range(x.size):
            v = streamer.update(float(x[i]), float(y[i]))
            streamed.append(math.nan if v is None else float(v))
        assert _eq_nan(batch, np.array(streamed, dtype=np.float64)), f"{cls.__name__} mismatch"


# --- Family 10 — Ehlers / Cycle ---


def test_mama_batch_shape_and_streaming_equivalence(sine_prices):
    batch = ta.MAMA().batch(sine_prices)
    assert batch.shape == (sine_prices.size, 2)

    streamer = ta.MAMA()
    rows = []
    for p in sine_prices:
        v = streamer.update(float(p))
        rows.append([math.nan, math.nan] if v is None else list(v))
    streamed = np.array(rows, dtype=np.float64)
    assert _eq_nan(batch, streamed)


def test_inverse_fisher_transform_zero_input_yields_zero():
    out = _to_np(ta.InverseFisherTransform(1.0).batch(np.array([0.0, 0.0, 0.0])))
    np.testing.assert_allclose(out, [0.0, 0.0, 0.0], atol=1e-12)


def test_fisher_transform_flat_series_is_zero():
    # Zero range -> the normaliser yields 0, and tanh(0) chain stays at 0.
    out = _to_np(ta.FisherTransform(5).batch(np.full(20, 42.0)))
    ready = out[~np.isnan(out)]
    assert np.all(np.abs(ready) < 1e-6)


def test_decycler_flat_series_passes_through():
    # High-pass of a flat input is zero, so the decycler equals the input.
    out = _to_np(ta.Decycler(20).batch(np.full(30, 100.0)))
    ready = out[~np.isnan(out)]
    np.testing.assert_allclose(ready, 100.0, atol=1e-9)


def test_center_of_gravity_flat_series_is_zero():
    out = _to_np(ta.CenterOfGravity(5).batch(np.full(20, 7.0)))
    ready = out[~np.isnan(out)]
    np.testing.assert_allclose(ready, 0.0, atol=1e-12)


def test_super_smoother_first_two_outputs_equal_inputs():
    out = _to_np(ta.SuperSmoother(10).batch(np.array([100.0, 101.0, 102.0])))
    # The 2-pole filter is seeded with raw values for the first two bars.
    assert out[0] == pytest.approx(100.0)
    assert out[1] == pytest.approx(101.0)


def test_td_setup_pure_uptrend_reaches_minus_9():
    # Every close is strictly greater than four bars ago -> sell-setup -9.
    h = np.arange(2.0, 22.0)
    l = h - 1.0
    c = h - 0.5
    out = _to_np(ta.TDSetup(4, 9).batch(h, l, c))
    # Setup completes at index 12 (warmup is 5 -> first emit at index 4 with
    # value -1, increments to -9 at index 12).
    assert out[12] == pytest.approx(-9.0)


def test_td_demarker_uptrend_pegs_at_one():
    # Strictly higher highs, strictly higher lows -> DeMax > 0, DeMin == 0
    # -> indicator == 1 after warmup.
    h = np.arange(11.0, 31.0)
    l = h - 2.0
    out = _to_np(ta.TDDeMarker(5).batch(h, l))
    assert out[-1] == pytest.approx(1.0)


def test_td_demarker_flat_market_emits_05():
    # All highs and lows equal -> denominator is zero -> neutral fallback 0.5.
    h = np.full(20, 11.0)
    l = np.full(20, 9.0)
    out = _to_np(ta.TDDeMarker(5).batch(h, l))
    assert out[-1] == pytest.approx(0.5)


def test_td_pressure_pure_bullish_yields_100():
    # Every bar closes at its high (close == high, open == low) -> per-bar
    # pressure ratio is +1 -> indicator == 100.
    n = 20
    open_ = np.full(n, 9.0)
    high = np.full(n, 11.0)
    low = np.full(n, 9.0)
    close = np.full(n, 11.0)
    volume = np.full(n, 100.0)
    out = _to_np(ta.TDPressure(5).batch(open_, high, low, close, volume))
    assert out[-1] == pytest.approx(100.0)


def test_td_combo_uptrend_saturates_at_minus_13():
    # Strictly increasing closes -> sell setup completes; combo conditions
    # (close >= high[i-2], high[i] >= high[i-1], close > close[i-1]) all
    # hold so combo saturates at -13.
    n = 40
    high = np.arange(1.0, 1.0 + n) + 0.5
    low = high - 1.0
    close = high - 0.5
    out = _to_np(ta.TDCombo().batch(high, low, close))
    assert out[-1] == pytest.approx(-13.0)


def test_td_countdown_uptrend_saturates_at_minus_13():
    n = 40
    high = np.arange(1.0, 1.0 + n) + 0.5
    low = high - 1.0
    close = high - 0.5
    out = _to_np(ta.TDCountdown().batch(high, low, close))
    assert out[-1] == pytest.approx(-13.0)


def test_td_lines_uptrend_sets_support_at_first_run_low():
    # Strictly rising closes -> sell setup completes at idx 12; the
    # lowest low among the setup bars (idx 4..=12) is the low at idx 4.
    n = 20
    high = np.arange(1.0, 1.0 + n) + 0.5
    low = high - 1.0
    close = high - 0.5
    out = _to_np(ta.TDLines().batch(high, low, close))
    # support is column 1; resistance is NaN at -1.
    assert math.isnan(out[-1, 0])
    # low at idx 4 = 5 + 0.5 - 1.0 = 4.5.
    assert out[-1, 1] == pytest.approx(4.5)


def test_td_range_projection_bullish_bar_reference():
    # open=10, high=12, low=9, close=11 (close > open) ->
    # pivot_sum = 2*12 + 9 + 11 = 44; half = 22.
    # projHigh = 22 - 9 = 13; projLow = 22 - 12 = 10.
    out = _to_np(ta.TDRangeProjection().batch(
        np.array([10.0]), np.array([12.0]), np.array([9.0]), np.array([11.0])
    ))
    assert out[0, 0] == pytest.approx(13.0)
    assert out[0, 1] == pytest.approx(10.0)


def test_td_differential_buy_signal_reference():
    # Closes 10 -> 9 -> 8.5 (two lower closes). Bar 1 has buying 1 and
    # selling 1 against its true range; bar 2 has buying 1.5 > 1 and
    # selling 0.5 < 1 -> +1.
    td = ta.TDDifferential()
    assert td.update((10.0, 11.0, 9.0, 10.0, 1.0, 0)) is None
    assert td.update((9.0, 10.0, 8.0, 9.0, 1.0, 1)) is None
    assert td.update((8.5, 9.0, 7.0, 8.5, 1.0, 2)) == pytest.approx(1.0)


def test_td_open_buy_signal_reference():
    # Prev bar low=10. Curr open=9 < 10, curr high=11 > 10 -> +1.
    td = ta.TDOpen()
    assert td.update((10.0, 11.0, 10.0, 10.5, 1.0, 0)) is None
    assert td.update((9.0, 11.0, 8.5, 9.5, 1.0, 1)) == pytest.approx(1.0)


def test_td_risk_level_uptrend_sets_sell_risk():
    # Strictly rising closes -> sell setup completes at idx 12.
    # The highest high is at idx 12 (= 13.5) with true range 1.5 ->
    # sell_risk = 13.5 + 1.5 = 15.0. Subsequent setups re-ratchet the level
    # so we check the first emission at idx 12.
    n = 20
    high = np.arange(1.0, 1.0 + n) + 0.5
    low = high - 1.0
    close = high - 0.5
    out = _to_np(ta.TDRiskLevel().batch(high, low, close))
    # buy_risk is column 0; sell_risk is column 1.
    assert math.isnan(out[12, 0])
    assert out[12, 1] == pytest.approx(15.0)


def test_classic_pivots_reference():
    # H=110, L=90, C=105 -> PP = 305/3, R1 = 2·PP − L, S1 = 2·PP − H.
    cp = ta.ClassicPivots()
    pp, r1, r2, r3, s1, s2, s3 = cp.update((105.0, 110.0, 90.0, 105.0, 1.0, 0))
    expected_pp = 305.0 / 3.0
    assert pp == pytest.approx(expected_pp)
    assert r1 == pytest.approx(2 * expected_pp - 90.0)
    assert s1 == pytest.approx(2 * expected_pp - 110.0)
    assert r2 == pytest.approx(expected_pp + 20.0)
    assert s2 == pytest.approx(expected_pp - 20.0)
    assert r3 > r2 and s3 < s2


def test_fibonacci_pivots_reference():
    # H=110, L=90, C=100 -> PP=100, range=20, R1=PP+0.382·range, etc.
    fp = ta.FibonacciPivots()
    pp, r1, r2, r3, s1, s2, s3 = fp.update((100.0, 110.0, 90.0, 100.0, 1.0, 0))
    assert pp == pytest.approx(100.0)
    assert r1 == pytest.approx(100.0 + 0.382 * 20.0)
    assert r2 == pytest.approx(100.0 + 0.618 * 20.0)
    assert r3 == pytest.approx(100.0 + 20.0)
    assert s1 == pytest.approx(100.0 - 0.382 * 20.0)
    assert s3 == pytest.approx(100.0 - 20.0)


def test_camarilla_pivots_reference():
    # H=110, L=90, C=105 -> R4 = C + range · 1.1 / 2 = 105 + 11 = 116.
    cm = ta.Camarilla()
    pp, r1, r2, r3, r4, s1, s2, s3, s4 = cm.update((105.0, 110.0, 90.0, 105.0, 1.0, 0))
    range_ = 20.0
    assert r1 == pytest.approx(105.0 + range_ * 1.1 / 12.0)
    assert r4 == pytest.approx(105.0 + range_ * 1.1 / 2.0)
    assert s4 == pytest.approx(105.0 - range_ * 1.1 / 2.0)
    assert pp == pytest.approx(305.0 / 3.0)
    # Strict widening with index.
    assert r4 > r3 > r2 > r1
    assert s4 < s3 < s2 < s1


def test_woodie_pivots_reference():
    # H=110, L=90, C=108 -> PP = (110 + 90 + 216) / 4 = 104.
    wp = ta.WoodiePivots()
    pp, r1, r2, s1, s2 = wp.update((108.0, 110.0, 90.0, 108.0, 1.0, 0))
    assert pp == pytest.approx(104.0)
    assert r1 == pytest.approx(2 * 104.0 - 90.0)
    assert s1 == pytest.approx(2 * 104.0 - 110.0)
    assert r2 == pytest.approx(104.0 + 20.0)
    assert s2 == pytest.approx(104.0 - 20.0)


def test_demark_pivots_up_bar_reference():
    # Up bar: O=100, H=120, L=80, C=110 -> X = 2H + L + C = 430, PP = 107.5.
    dp = ta.DemarkPivots()
    pp, r1, s1 = dp.update((100.0, 120.0, 80.0, 110.0, 1.0, 0))
    assert pp == pytest.approx(107.5)
    assert r1 == pytest.approx(215.0 - 80.0)
    assert s1 == pytest.approx(215.0 - 120.0)


def test_williams_fractals_isolated_peak():
    # Highs 1, 2, 5, 2, 1; the centre is strictly above both neighbours.
    wf = ta.WilliamsFractals()
    last = None
    for i, h in enumerate([1.0, 2.0, 5.0, 2.0, 1.0]):
        last = wf.update((h, h, h - 0.5, h, 1.0, i))
    assert last is not None
    up, down = last
    assert up == pytest.approx(5.0)
    assert down is None


def test_zigzag_confirms_after_threshold_reversal():
    # 100 -> 120 (uptrend pivot) -> 100 (16.7% drop confirms swing high).
    zz = ta.ZigZag(0.10)
    assert zz.update((100.0, 100.5, 99.5, 100.0, 1.0, 0)) is None
    assert zz.update((120.0, 120.5, 119.5, 120.0, 1.0, 1)) is None
    confirmed = zz.update((100.0, 100.5, 99.5, 100.0, 1.0, 2))
    assert confirmed is not None
    swing, direction = confirmed
    assert swing == pytest.approx(120.5)
    assert direction == 1.0


# --- Family 05 reference values ---------------------------------------------


def test_ma_envelope_reference():
    # SMA([10, 20, 30]) = 20; with percent = 0.10: upper = 22, lower = 18.
    out = _to_np(ta.MaEnvelope(3, 0.10).batch(np.array([10.0, 20.0, 30.0])))
    assert math.isnan(out[0, 0]) and math.isnan(out[1, 0])
    assert out[2, 0] == pytest.approx(22.0)  # upper
    assert out[2, 1] == pytest.approx(20.0)  # middle
    assert out[2, 2] == pytest.approx(18.0)  # lower


def test_acceleration_bands_reference():
    # Single bar: high=12, low=8, close=10, factor=0.5, period=1.
    # ratio = 4/20 = 0.2; raw_up = 12·1.1 = 13.2; raw_lo = 8·0.9 = 7.2.
    v = ta.AccelerationBands(1, 0.5).update((10.0, 12.0, 8.0, 10.0, 1.0, 0))
    assert v == pytest.approx((13.2, 10.0, 7.2))


def test_atr_bands_reference():
    # Five identical bars (h=11, l=9, c=10) → ATR=2, close=10, mult=3:
    # upper=16, middle=10, lower=4.
    out = _to_np(ta.AtrBands(5, 3.0).batch(
        np.array([11.0] * 5), np.array([9.0] * 5), np.array([10.0] * 5)
    ))
    assert math.isnan(out[3, 0])
    assert out[4, 0] == pytest.approx(16.0)
    assert out[4, 1] == pytest.approx(10.0)
    assert out[4, 2] == pytest.approx(4.0)


def test_hurst_channel_reference():
    # Five identical (h=12, l=8, c=10): SMA(close)=10, range=4, mult=0.5.
    out = _to_np(ta.HurstChannel(5, 0.5).batch(
        np.array([12.0] * 5), np.array([8.0] * 5), np.array([10.0] * 5)
    ))
    assert out[4, 0] == pytest.approx(12.0)
    assert out[4, 1] == pytest.approx(10.0)
    assert out[4, 2] == pytest.approx(8.0)


def test_linreg_channel_reference():
    # period 3 over [1, 2, 9]: line y=4x, endpoint=8, residuals=[1, -2, 1],
    # population sigma=sqrt(2); mult=2 → upper=8+2√2, lower=8-2√2.
    out = _to_np(ta.LinRegChannel(3, 2.0).batch(np.array([1.0, 2.0, 9.0])))
    s = math.sqrt(2.0)
    assert out[2, 0] == pytest.approx(8.0 + 2.0 * s)
    assert out[2, 1] == pytest.approx(8.0)
    assert out[2, 2] == pytest.approx(8.0 - 2.0 * s)


def test_standard_error_bands_reference():
    # Same [1, 2, 9] with n=3: SSE=6, n-2=1, stderr=sqrt(6); mult=2 →
    # upper=8+2√6, lower=8-2√6.
    out = _to_np(ta.StandardErrorBands(3, 2.0).batch(np.array([1.0, 2.0, 9.0])))
    s = math.sqrt(6.0)
    assert out[2, 0] == pytest.approx(8.0 + 2.0 * s)
    assert out[2, 1] == pytest.approx(8.0)
    assert out[2, 2] == pytest.approx(8.0 - 2.0 * s)


def test_double_bollinger_orders_bands():
    # On a non-trivial dispersion, outer >= inner >= middle >= -inner >= -outer.
    out = _to_np(ta.DoubleBollinger(5, 1.0, 2.0).batch(
        np.array([1.0, 5.0, 2.0, 4.0, 3.0, 6.0])
    ))
    v = out[5]
    assert v[0] >= v[1] >= v[2] >= v[3] >= v[4]


def test_vwap_stddev_bands_reference():
    # Two equal-volume bars with tp=8, tp=12: vwap=10, σ=2, mult=1.5 →
    # upper=13, lower=7.
    v = ta.VwapStdDevBands(1.5)
    v.update((8.0, 8.0, 8.0, 8.0, 1.0, 0))
    out = v.update((12.0, 12.0, 12.0, 12.0, 1.0, 1))
    assert out[0] == pytest.approx(13.0)
    assert out[1] == pytest.approx(10.0)
    assert out[2] == pytest.approx(7.0)
    assert out[3] == pytest.approx(2.0)


def test_ttm_squeeze_flat_market():
    # Zero volatility: BB and KC both collapse to a point → squeeze=1.0,
    # momentum=0.0.
    candles_h = np.array([10.0] * 25)
    out = _to_np(ta.TtmSqueeze(20, 2.0, 1.5).batch(candles_h, candles_h, candles_h))
    assert out[24, 0] == pytest.approx(1.0)
    assert out[24, 1] == pytest.approx(0.0)


def test_fractal_chaos_bands_detects_peak_and_trough():
    # Sequence that creates one fractal high (i=2) and one low (i=3).
    h = np.array([1.0, 2.0, 5.0, 3.0, 2.0, 1.0, 2.0])
    l = np.array([1.0, 2.0, 3.0, 0.5, 2.0, 1.0, 2.0])
    out = _to_np(ta.FractalChaosBands(2).batch(h, l))
    # First bar with both bands set is index 5.
    assert math.isnan(out[4, 0])
    assert out[5, 0] == pytest.approx(5.0)
    assert out[5, 1] == pytest.approx(0.5)



def test_belt_hold_reference():
    t = ta.BeltHold()
    assert t.update((10.0, 12.0, 10.0, 11.5, 1.0, 0)) == pytest.approx(1.0)
    assert t.update((12.0, 12.0, 10.0, 10.5, 1.0, 1)) == pytest.approx(-1.0)


def test_breakaway_reference():
    t = ta.Breakaway()
    assert t.update((20.0, 20.2, 14.8, 15.0, 1.0, 0)) is None
    assert t.update((14.0, 14.1, 11.9, 12.0, 1.0, 1)) is None
    assert t.update((12.5, 13.0, 10.5, 11.0, 1.0, 2)) is None
    assert t.update((11.0, 11.5, 9.0, 9.5, 1.0, 3)) is None
    assert t.update((9.5, 14.7, 9.4, 14.5, 1.0, 4)) == pytest.approx(1.0)


def test_counterattack_reference():
    t = ta.Counterattack()
    assert t.update((20.0, 20.1, 14.9, 15.0, 1.0, 0)) is None
    assert t.update((10.0, 15.1, 9.9, 15.0, 1.0, 1)) == pytest.approx(1.0)


def test_doji_star_reference():
    t = ta.DojiStar()
    assert t.update((20.0, 20.2, 14.8, 15.0, 1.0, 0)) is None
    assert t.update((13.0, 13.1, 12.9, 13.0, 1.0, 1)) == pytest.approx(1.0)


def test_dragonfly_doji_reference():
    t = ta.DragonflyDoji()
    assert t.update((10.0, 10.05, 6.0, 10.0, 1.0, 0)) == pytest.approx(1.0)


def test_gravestone_doji_reference():
    t = ta.GravestoneDoji()
    assert t.update((10.0, 14.0, 9.95, 10.0, 1.0, 0)) == pytest.approx(-1.0)


def test_long_legged_doji_reference():
    t = ta.LongLeggedDoji()
    assert t.update((10.0, 12.0, 8.0, 10.05, 1.0, 0)) == pytest.approx(1.0)


def test_rickshaw_man_reference():
    t = ta.RickshawMan()
    assert t.update((10.0, 12.0, 8.0, 10.0, 1.0, 0)) == pytest.approx(1.0)


def test_evening_doji_star_reference():
    t = ta.EveningDojiStar()
    assert t.update((10.0, 15.1, 9.9, 15.0, 1.0, 0)) is None
    assert t.update((17.0, 17.1, 16.9, 17.0, 1.0, 1)) is None
    assert t.update((16.0, 16.1, 11.9, 12.0, 1.0, 2)) == pytest.approx(-1.0)


def test_morning_doji_star_reference():
    t = ta.MorningDojiStar()
    assert t.update((15.0, 15.1, 9.9, 10.0, 1.0, 0)) is None
    assert t.update((8.0, 8.1, 7.9, 8.0, 1.0, 1)) is None
    assert t.update((9.0, 13.1, 8.9, 13.0, 1.0, 2)) == pytest.approx(1.0)


def test_gap_side_by_side_white_reference():
    t = ta.GapSideBySideWhite()
    assert t.update((10.0, 11.1, 9.9, 11.0, 1.0, 0)) is None
    assert t.update((13.0, 14.1, 12.9, 14.0, 1.0, 1)) is None
    assert t.update((13.0, 14.1, 12.9, 14.0, 1.0, 2)) == pytest.approx(1.0)


def test_high_wave_reference():
    t = ta.HighWave()
    assert t.update((10.0, 12.0, 8.0, 10.3, 1.0, 0)) == pytest.approx(1.0)


def test_hikkake_reference():
    t = ta.Hikkake()
    assert t.update((10.0, 15.0, 5.0, 12.0, 1.0, 0)) is None
    assert t.update((11.0, 13.0, 8.0, 12.0, 1.0, 1)) is None
    assert t.update((9.0, 12.0, 6.0, 7.0, 1.0, 2)) == pytest.approx(1.0)


def test_hikkake_modified_reference():
    # Two nested inside bars, the first closing near its low, then a false
    # downside break -> +1.
    t = ta.HikkakeModified()
    assert t.update((10.0, 15.0, 5.0, 12.0, 1.0, 0)) is None
    assert t.update((11.0, 13.0, 7.0, 7.5, 1.0, 1)) is None
    assert t.update((9.0, 12.0, 8.0, 10.0, 1.0, 2)) is None
    assert t.update((9.0, 11.0, 6.0, 9.0, 1.0, 3)) == pytest.approx(1.0)


def test_homing_pigeon_reference():
    t = ta.HomingPigeon()
    assert t.update((15.0, 15.1, 9.9, 10.0, 1.0, 0)) is None
    assert t.update((14.0, 14.1, 10.9, 11.0, 1.0, 1)) == pytest.approx(1.0)


def test_on_neck_reference():
    t = ta.OnNeck()
    assert t.update((15.0, 15.1, 9.0, 10.0, 1.0, 0)) is None
    assert t.update((7.0, 9.1, 6.9, 9.0, 1.0, 1)) == pytest.approx(-1.0)


def test_in_neck_reference():
    t = ta.InNeck()
    assert t.update((15.0, 15.1, 9.0, 10.0, 1.0, 0)) is None
    assert t.update((7.0, 10.3, 6.9, 10.2, 1.0, 1)) == pytest.approx(-1.0)


def test_thrusting_reference():
    t = ta.Thrusting()
    assert t.update((15.0, 15.1, 9.0, 10.0, 1.0, 0)) is None
    assert t.update((7.0, 11.6, 6.9, 11.5, 1.0, 1)) == pytest.approx(-1.0)


def test_separating_lines_reference():
    t = ta.SeparatingLines()
    assert t.update((12.0, 12.1, 9.9, 10.0, 1.0, 0)) is None
    assert t.update((12.0, 14.1, 12.0, 14.0, 1.0, 1)) == pytest.approx(1.0)


def test_kicking_reference():
    t = ta.Kicking()
    assert t.update((12.0, 12.0, 10.0, 10.0, 1.0, 0)) is None
    assert t.update((14.0, 16.0, 14.0, 16.0, 1.0, 1)) == pytest.approx(1.0)


def test_kicking_by_length_reference():
    t = ta.KickingByLength()
    assert t.update((12.0, 12.0, 10.0, 10.0, 1.0, 0)) is None
    assert t.update((14.0, 20.0, 14.0, 20.0, 1.0, 1)) == pytest.approx(1.0)


def test_ladder_bottom_reference():
    t = ta.LadderBottom()
    assert t.update((20.0, 20.1, 17.9, 18.0, 1.0, 0)) is None
    assert t.update((18.0, 18.1, 15.9, 16.0, 1.0, 1)) is None
    assert t.update((16.0, 16.1, 13.9, 14.0, 1.0, 2)) is None
    assert t.update((14.0, 15.0, 12.4, 12.5, 1.0, 3)) is None
    assert t.update((15.0, 17.1, 14.9, 17.0, 1.0, 4)) == pytest.approx(1.0)


def test_mat_hold_reference():
    t = ta.MatHold()
    assert t.update((10.0, 15.1, 9.9, 15.0, 1.0, 0)) is None
    assert t.update((16.0, 16.1, 15.4, 15.5, 1.0, 1)) is None
    assert t.update((15.5, 15.6, 14.9, 15.0, 1.0, 2)) is None
    assert t.update((15.0, 15.1, 14.4, 14.5, 1.0, 3)) is None
    assert t.update((14.5, 17.1, 14.4, 17.0, 1.0, 4)) == pytest.approx(1.0)


def test_matching_low_reference():
    t = ta.MatchingLow()
    assert t.update((15.0, 15.1, 9.9, 10.0, 1.0, 0)) is None
    assert t.update((13.0, 13.1, 9.9, 10.0, 1.0, 1)) == pytest.approx(1.0)


def test_long_line_reference():
    t = ta.LongLine()
    # Five quiet bars fill the rolling range average, then a wide solid white bar.
    for ts in range(5):
        assert t.update((10.0, 10.5, 9.5, 10.2, 1.0, ts)) is None
    assert t.update((10.0, 13.0, 9.9, 12.9, 1.0, 5)) == pytest.approx(1.0)


def test_short_line_reference():
    t = ta.ShortLine()
    # Five wide bars fill the rolling range average, then a compact solid white bar.
    for ts in range(5):
        assert t.update((10.0, 13.0, 9.5, 12.9, 1.0, ts)) is None
    assert t.update((10.0, 11.0, 9.9, 10.9, 1.0, 5)) == pytest.approx(1.0)


def test_rising_three_methods_reference():
    t = ta.RisingThreeMethods()
    assert t.update((10.0, 15.1, 9.9, 15.0, 1.0, 0)) is None
    assert t.update((14.0, 14.1, 12.9, 13.0, 1.0, 1)) is None
    assert t.update((13.5, 13.6, 12.4, 12.5, 1.0, 2)) is None
    assert t.update((13.0, 13.1, 11.9, 12.0, 1.0, 3)) is None
    assert t.update((12.5, 16.1, 12.4, 16.0, 1.0, 4)) == pytest.approx(1.0)


def test_falling_three_methods_reference():
    t = ta.FallingThreeMethods()
    assert t.update((15.0, 15.1, 9.9, 10.0, 1.0, 0)) is None
    assert t.update((11.0, 12.1, 10.9, 12.0, 1.0, 1)) is None
    assert t.update((11.5, 12.6, 11.4, 12.5, 1.0, 2)) is None
    assert t.update((12.0, 13.1, 11.9, 13.0, 1.0, 3)) is None
    assert t.update((12.5, 12.6, 8.9, 9.0, 1.0, 4)) == pytest.approx(-1.0)


def test_upside_gap_three_methods_reference():
    t = ta.UpsideGapThreeMethods()
    assert t.update((10.0, 11.2, 9.8, 11.0, 1.0, 0)) is None
    assert t.update((12.0, 13.2, 11.9, 13.0, 1.0, 1)) is None
    assert t.update((12.5, 12.6, 10.4, 10.5, 1.0, 2)) == pytest.approx(1.0)


def test_downside_gap_three_methods_reference():
    t = ta.DownsideGapThreeMethods()
    assert t.update((13.0, 13.2, 11.8, 12.0, 1.0, 0)) is None
    assert t.update((11.0, 11.1, 9.8, 10.0, 1.0, 1)) is None
    assert t.update((10.5, 12.6, 10.4, 12.5, 1.0, 2)) == pytest.approx(-1.0)


def test_stalled_pattern_reference():
    t = ta.StalledPattern()
    assert t.update((10.0, 12.05, 9.9, 12.0, 1.0, 0)) is None
    assert t.update((11.0, 14.05, 10.9, 14.0, 1.0, 1)) is None
    assert t.update((14.0, 14.6, 13.95, 14.15, 1.0, 2)) == pytest.approx(-1.0)


def test_stick_sandwich_reference():
    t = ta.StickSandwich()
    assert t.update((12.0, 12.1, 9.9, 10.0, 1.0, 0)) is None
    assert t.update((10.5, 11.6, 10.4, 11.5, 1.0, 1)) is None
    assert t.update((11.5, 11.6, 9.9, 10.0, 1.0, 2)) == pytest.approx(1.0)


def test_takuri_reference():
    t = ta.Takuri()
    assert t.update((10.0, 10.05, 7.0, 10.0, 1.0, 0)) == pytest.approx(1.0)


def test_closing_marubozu_reference():
    t = ta.ClosingMarubozu()
    assert t.update((10.5, 15.0, 10.0, 15.0, 1.0, 0)) == pytest.approx(1.0)


def test_opening_marubozu_reference():
    t = ta.OpeningMarubozu()
    assert t.update((10.0, 15.0, 10.0, 14.5, 1.0, 0)) == pytest.approx(1.0)


def test_tasuki_gap_reference():
    t = ta.TasukiGap()
    assert t.update((10.0, 11.2, 9.8, 11.0, 1.0, 0)) is None
    assert t.update((12.0, 14.0, 11.9, 13.5, 1.0, 1)) is None
    assert t.update((13.0, 13.1, 11.4, 11.5, 1.0, 2)) == pytest.approx(1.0)


def test_unique_three_river_reference():
    t = ta.UniqueThreeRiver()
    assert t.update((15.0, 15.1, 10.0, 10.5, 1.0, 0)) is None
    assert t.update((14.0, 14.1, 9.0, 11.0, 1.0, 1)) is None
    assert t.update((10.2, 10.9, 9.5, 10.4, 1.0, 2)) == pytest.approx(1.0)


def test_concealing_baby_swallow_reference():
    t = ta.ConcealingBabySwallow()
    assert t.update((20.0, 20.1, 14.9, 15.0, 1.0, 0)) is None
    assert t.update((16.0, 16.1, 11.9, 12.0, 1.0, 1)) is None
    assert t.update((11.0, 13.0, 9.9, 10.0, 1.0, 2)) is None
    assert t.update((14.0, 14.1, 8.9, 9.0, 1.0, 3)) == pytest.approx(1.0)


def test_rolling_correlation_reference():
    t = ta.RollingCorrelation(20)
    assert t.update(1.0, 1.0) is None
    assert t.update(2.0, 1.5) is None


def test_rolling_covariance_reference():
    t = ta.RollingCovariance(20)
    assert t.update(1.0, 1.0) is None
    assert t.update(2.0, 1.5) is None


def test_ou_half_life_reference():
    t = ta.OuHalfLife(60)
    assert t.update(1.0, 1.0) is None
    assert t.update(2.0, 1.5) is None


def test_spread_hurst_reference():
    t = ta.SpreadHurst(60)
    assert t.update(1.0, 1.0) is None
    assert t.update(2.0, 1.5) is None


def test_distance_ssd_reference():
    t = ta.DistanceSsd(20)
    assert t.update(1.0, 1.0) is None
    assert t.update(2.0, 1.5) is None


def test_beta_neutral_spread_reference():
    t = ta.BetaNeutralSpread(20)
    assert t.update(1.0, 1.0) is None
    assert t.update(2.0, 1.5) is None


def test_variance_ratio_reference():
    t = ta.VarianceRatio(60, 2)
    assert t.update(1.0, 1.0) is None
    assert t.update(2.0, 1.5) is None


def test_granger_causality_reference():
    t = ta.GrangerCausality(60, 1)
    assert t.update(1.0, 1.0) is None
    assert t.update(2.0, 1.5) is None


def test_double_top_bottom_reference():
    t = ta.DoubleTopBottom()
    assert t.update((119.88, 120.0, 119.88, 119.88, 1.0, 0)) is None
    assert t.update((100.0, 118.8, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 120.0, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((108.0, 118.8, 108.0, 108.0, 1.0, 3)) == pytest.approx(-1.0)


def test_triple_top_bottom_reference():
    t = ta.TripleTopBottom()
    assert t.update((119.88, 120.0, 119.88, 119.88, 1.0, 0)) is None
    assert t.update((100.0, 118.8, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 121.0, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((99.0, 119.79, 99.0, 99.0, 1.0, 3)) is None
    assert t.update((99.99, 119.0, 99.99, 99.99, 1.0, 4)) is None
    assert t.update((107.1, 117.81, 107.1, 107.1, 1.0, 5)) == pytest.approx(-1.0)


def test_head_and_shoulders_reference():
    t = ta.HeadAndShoulders()
    assert t.update((99.9, 100.0, 99.9, 99.9, 1.0, 0)) is None
    assert t.update((90.0, 99.0, 90.0, 90.0, 1.0, 1)) is None
    assert t.update((90.9, 120.0, 90.9, 90.9, 1.0, 2)) is None
    assert t.update((92.0, 118.8, 92.0, 92.0, 1.0, 3)) is None
    assert t.update((92.92, 101.0, 92.92, 92.92, 1.0, 4)) is None
    assert t.update((90.9, 99.99, 90.9, 90.9, 1.0, 5)) == pytest.approx(-1.0)


def test_triangle_reference():
    t = ta.Triangle()
    assert t.update((129.87, 130.0, 129.87, 129.87, 1.0, 0)) is None
    assert t.update((100.0, 128.7, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 120.0, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((110.0, 118.8, 110.0, 110.0, 1.0, 3)) is None
    assert t.update((111.1, 120.0, 111.1, 111.1, 1.0, 4)) == pytest.approx(1.0)
    assert t.update((108.0, 118.8, 108.0, 108.0, 1.0, 5)) == pytest.approx(1.0)


def test_wedge_reference():
    t = ta.Wedge()
    assert t.update((109.89, 110.0, 109.89, 109.89, 1.0, 0)) is None
    assert t.update((90.0, 108.9, 90.0, 90.0, 1.0, 1)) is None
    assert t.update((90.9, 100.0, 90.9, 90.9, 1.0, 2)) is None
    assert t.update((94.0, 99.0, 94.0, 94.0, 1.0, 3)) is None
    assert t.update((94.94, 103.0, 94.94, 94.94, 1.0, 4)) == pytest.approx(0.0)
    assert t.update((92.7, 101.97, 92.7, 92.7, 1.0, 5)) == pytest.approx(-1.0)


def test_flag_pennant_reference():
    t = ta.FlagPennant()
    assert t.update((149.85, 150.0, 149.85, 149.85, 1.0, 0)) is None
    assert t.update((100.0, 148.5, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 140.0, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((130.0, 138.6, 130.0, 130.0, 1.0, 3)) == pytest.approx(0.0)
    assert t.update((131.3, 143.0, 131.3, 131.3, 1.0, 4)) == pytest.approx(1.0)


def test_rectangle_range_reference():
    t = ta.RectangleRange()
    assert t.update((119.88, 120.0, 119.88, 119.88, 1.0, 0)) is None
    assert t.update((100.0, 118.8, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 121.0, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((99.0, 119.79, 99.0, 99.0, 1.0, 3)) is None
    assert t.update((99.99, 108.9, 99.99, 99.99, 1.0, 4)) == pytest.approx(1.0)


def test_cup_and_handle_reference():
    t = ta.CupAndHandle()
    assert t.update((119.88, 120.0, 119.88, 119.88, 1.0, 0)) is None
    assert t.update((90.0, 118.8, 90.0, 90.0, 1.0, 1)) is None
    assert t.update((90.9, 121.0, 90.9, 90.9, 1.0, 2)) is None
    assert t.update((110.0, 119.79, 110.0, 110.0, 1.0, 3)) is None
    assert t.update((111.1, 121.0, 111.1, 111.1, 1.0, 4)) == pytest.approx(1.0)


def test_abcd_reference():
    t = ta.Abcd()
    assert t.update((139.86, 140.0, 139.86, 139.86, 1.0, 0)) is None
    assert t.update((100.0, 138.6, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 124.7, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((84.7, 123.453, 84.7, 84.7, 1.0, 3)) is None
    assert t.update((85.547, 93.17, 85.547, 85.547, 1.0, 4)) == pytest.approx(1.0)


def test_gartley_reference():
    t = ta.Gartley()
    assert t.update((149.85, 150.0, 149.85, 149.85, 1.0, 0)) is None
    assert t.update((100.0, 148.5, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 140.0, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((115.3, 138.6, 115.3, 115.3, 1.0, 3)) is None
    assert t.update((116.453, 127.65, 116.453, 116.453, 1.0, 4)) is None
    assert t.update((108.56, 126.3735, 108.56, 108.56, 1.0, 5)) == pytest.approx(0.0)
    assert t.update((109.6456, 119.416, 109.6456, 109.6456, 1.0, 6)) == pytest.approx(1.0)


def test_butterfly_reference():
    t = ta.Butterfly()
    assert t.update((149.85, 150.0, 149.85, 149.85, 1.0, 0)) is None
    assert t.update((100.0, 148.5, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 140.0, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((108.6, 138.6, 108.6, 108.6, 1.0, 3)) is None
    assert t.update((109.686, 128.0, 109.686, 109.686, 1.0, 4)) is None
    assert t.update((79.8, 126.72, 79.8, 79.8, 1.0, 5)) == pytest.approx(0.0)
    assert t.update((80.598, 87.78, 80.598, 80.598, 1.0, 6)) == pytest.approx(1.0)


def test_bat_reference():
    t = ta.Bat()
    assert t.update((149.85, 150.0, 149.85, 149.85, 1.0, 0)) is None
    assert t.update((100.0, 148.5, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 140.0, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((122.0, 138.6, 122.0, 122.0, 1.0, 3)) is None
    assert t.update((123.22, 137.0, 123.22, 123.22, 1.0, 4)) is None
    assert t.update((104.56, 135.63, 104.56, 104.56, 1.0, 5)) == pytest.approx(0.0)
    assert t.update((105.6056, 115.016, 105.6056, 105.6056, 1.0, 6)) == pytest.approx(1.0)


def test_crab_reference():
    t = ta.Crab()
    assert t.update((149.85, 150.0, 149.85, 149.85, 1.0, 0)) is None
    assert t.update((100.0, 148.5, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 140.0, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((120.0, 138.6, 120.0, 120.0, 1.0, 3)) is None
    assert t.update((121.2, 137.5, 121.2, 121.2, 1.0, 4)) is None
    assert t.update((75.3, 136.125, 75.3, 75.3, 1.0, 5)) == pytest.approx(0.0)
    assert t.update((76.053, 82.83, 76.053, 76.053, 1.0, 6)) == pytest.approx(1.0)


def test_shark_reference():
    t = ta.Shark()
    assert t.update((149.85, 150.0, 149.85, 149.85, 1.0, 0)) is None
    assert t.update((100.0, 148.5, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 140.0, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((88.0, 138.6, 88.0, 88.0, 1.0, 3)) is None
    assert t.update((88.88, 186.8, 88.88, 88.88, 1.0, 4)) is None
    assert t.update((100.0, 184.932, 100.0, 100.0, 1.0, 5)) == pytest.approx(0.0)
    assert t.update((101.0, 110.0, 101.0, 101.0, 1.0, 6)) == pytest.approx(1.0)


def test_cypher_reference():
    # X=100 A=140 B=120 C=152 D=111.128: AB/XA = 0.5, XC/XA = 1.3 (the X-to-C
    # projection, not BC), CD/XC = 0.786.
    t = ta.Cypher()
    assert t.update((149.85, 150.0, 149.85, 149.85, 1.0, 0)) is None
    assert t.update((100.0, 148.5, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 140.0, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((120.0, 138.6, 120.0, 120.0, 1.0, 3)) is None
    assert t.update((121.2, 152.0, 121.2, 121.2, 1.0, 4)) is None
    assert t.update((111.128, 150.48, 111.128, 111.128, 1.0, 5)) == pytest.approx(0.0)
    assert t.update(
        (112.23928000000001, 122.24080000000001, 112.23928000000001, 112.23928000000001, 1.0, 6)
    ) == pytest.approx(1.0)


def test_three_drives_reference():
    # Seven pivots spanning three rising drives (124, 128, 132), each extending
    # a 10-point retracement by 14. Two drives alone are not a match.
    t = ta.ThreeDrives()
    assert t.update((119.88, 120.0, 119.88, 119.88, 1.0, 0)) is None
    assert t.update((110.0, 118.8, 110.0, 110.0, 1.0, 1)) is None
    assert t.update((111.1, 124.0, 111.1, 111.1, 1.0, 2)) is None
    assert t.update((114.0, 122.76, 114.0, 114.0, 1.0, 3)) is None
    assert t.update((115.14, 128.0, 115.14, 115.14, 1.0, 4)) is None
    assert t.update((118.0, 126.72, 118.0, 118.0, 1.0, 5)) is None
    assert t.update((119.18, 132.0, 119.18, 119.18, 1.0, 6)) is None
    assert t.update((118.8, 130.68, 118.8, 118.8, 1.0, 7)) == pytest.approx(-1.0)


def test_fib_retracement_reference():
    t = ta.FibRetracement()
    assert t.update((199.8, 200.0, 199.8, 199.8, 1.0, 0)) is None
    assert t.update((100.0, 198.0, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 110.0, 101.0, 101.0, 1.0, 2)) == pytest.approx((100.0, 123.6, 138.2, 150.0, 161.8, 178.6, 200.0))


def test_fib_extension_reference():
    t = ta.FibExtension()
    assert t.update((199.8, 200.0, 199.8, 199.8, 1.0, 0)) is None
    assert t.update((100.0, 198.0, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 110.0, 101.0, 101.0, 1.0, 2)) == pytest.approx((72.8, 58.6, 38.2, 0.0, -61.8))


def test_fib_projection_reference():
    t = ta.FibProjection()
    assert t.update((199.8, 200.0, 199.8, 199.8, 1.0, 0)) is None
    assert t.update((160.0, 198.0, 160.0, 160.0, 1.0, 1)) is None
    assert t.update((161.6, 190.0, 161.6, 161.6, 1.0, 2)) is None
    assert t.update((171.0, 188.1, 171.0, 171.0, 1.0, 3)) == pytest.approx((165.28, 150.0, 125.28, 85.28))


def test_auto_fib_reference():
    t = ta.AutoFib()
    assert t.update((199.8, 200.0, 199.8, 199.8, 1.0, 0)) is None
    assert t.update((100.0, 198.0, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 110.0, 101.0, 101.0, 1.0, 2)) == pytest.approx((100.0, 123.6, 138.2, 150.0, 161.8, 178.6, 200.0))


def test_golden_pocket_reference():
    t = ta.GoldenPocket()
    assert t.update((199.8, 200.0, 199.8, 199.8, 1.0, 0)) is None
    assert t.update((100.0, 198.0, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 110.0, 101.0, 101.0, 1.0, 2)) == pytest.approx((161.8, 163.4, 165.0))


def test_fib_confluence_reference():
    t = ta.FibConfluence()
    assert t.update((199.8, 200.0, 199.8, 199.8, 1.0, 0)) is None
    assert t.update((100.0, 198.0, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((101.0, 160.0, 101.0, 101.0, 1.0, 2)) is None
    assert t.update((144.0, 158.4, 144.0, 144.0, 1.0, 3)) == pytest.approx((137.64, 2.0))


def test_fib_fan_reference():
    t = ta.FibFan()
    assert t.update((199.0, 200.0, 199.0, 199.0, 1.0, 0)) is None
    assert t.update((160.0, 190.0, 160.0, 160.0, 1.0, 1)) is None
    assert t.update((100.0, 150.0, 100.0, 100.0, 1.0, 2)) is None
    assert t.update((105.0, 110.0, 105.0, 105.0, 1.0, 3)) == pytest.approx((107.3, 125.0, 142.7))


def test_fib_arcs_reference():
    t = ta.FibArcs()
    assert t.update((199.0, 200.0, 199.0, 199.0, 1.0, 0)) is None
    assert t.update((160.0, 190.0, 160.0, 160.0, 1.0, 1)) is None
    assert t.update((100.0, 150.0, 100.0, 100.0, 1.0, 2)) is None
    assert t.update((105.0, 110.0, 105.0, 105.0, 1.0, 3)) == pytest.approx((133.082181, 143.30127, 153.52037))


def test_fib_channel_reference():
    t = ta.FibChannel()
    assert t.update((199.0, 200.0, 199.0, 199.0, 1.0, 0)) is None
    assert t.update((100.0, 190.0, 100.0, 100.0, 1.0, 1)) is None
    assert t.update((108.0, 110.0, 108.0, 108.0, 1.0, 2)) is None
    assert t.update((210.0, 220.0, 210.0, 210.0, 1.0, 3)) is None
    assert t.update((150.0, 200.0, 150.0, 150.0, 1.0, 4)) == pytest.approx((226.666667, 160.746667, 120.0, 54.08))


def test_fib_time_zones_reference():
    t = ta.FibTimeZones()
    assert t.update((199.0, 200.0, 199.0, 199.0, 1.0, 0)) is None
    assert t.update((150.0, 190.0, 150.0, 150.0, 1.0, 1)) == pytest.approx((1.0, 1.0))
    assert t.update((151.0, 155.0, 151.0, 151.0, 1.0, 2)) == pytest.approx((1.0, 1.0))
    assert t.update((151.0, 155.0, 151.0, 151.0, 1.0, 3)) == pytest.approx((1.0, 2.0))
    assert t.update((151.0, 155.0, 151.0, 151.0, 1.0, 4)) == pytest.approx((0.0, 1.0))
    assert t.update((151.0, 155.0, 151.0, 151.0, 1.0, 5)) == pytest.approx((1.0, 3.0))


def test_spread_ar1_coefficient_reference():
    t = ta.SpreadAr1Coefficient(20)
    assert t.update(1.0, 1.0) is None
    # Spread a - b grows by exactly 1 each bar (unit root) => rho == 1.
    a = np.array([2.0 * i for i in range(40)])
    b = np.array([float(i) for i in range(40)])
    out = _to_np(ta.SpreadAr1Coefficient(20).batch(a, b))
    assert math.isclose(out[-1], 1.0, abs_tol=1e-9)


def test_elder_ray_reference():
    er = ta.ElderRay(3)
    high = np.array([11.0, 13.0, 16.0])
    low = np.array([9.0, 11.0, 13.0])
    close = np.array([10.0, 12.0, 14.0])
    out = _to_np(er.batch(high, low, close))
    # EMA(3) seeds at the third bar with mean close 12; bar high 16 -> bull 4,
    # low 13 -> bear 1.
    assert out[2][0] == pytest.approx(4.0)
    assert out[2][1] == pytest.approx(1.0)


def test_imi_reference():
    imi = ta.IMI(3)
    open_ = np.array([10.0, 11.0, 10.0])
    high = np.array([12.0, 12.0, 13.0])
    low = np.array([9.0, 9.0, 9.0])
    close = np.array([11.0, 10.0, 12.0])
    out = _to_np(imi.batch(open_, high, low, close))
    # bodies +1, -1, +2 -> gain 3, loss 1 -> 100 * 3 / 4 = 75.
    assert math.isnan(out[0])
    assert math.isnan(out[1])
    assert out[2] == pytest.approx(75.0)


def test_qstick_reference():
    q = ta.Qstick(3)
    open_ = np.array([10.0, 10.0, 10.0])
    close = np.array([11.0, 11.0, 11.0])
    out = _to_np(q.batch(open_, close))
    # Each body is close - open = 1; SMA(3) of [1, 1, 1] = 1.
    assert math.isnan(out[0])
    assert math.isnan(out[1])
    assert out[2] == pytest.approx(1.0)


def test_ttm_trend_reference():
    t = ta.TTM_TREND(3)
    high = np.array([13.0, 13.0, 13.0])
    low = np.array([9.0, 9.0, 9.0])
    close = np.array([12.0, 12.0, 12.0])
    out = _to_np(t.batch(high, low, close))
    # Median (13 + 9) / 2 = 11; close 12 is above the SMA(3) reference -> +1.
    assert math.isnan(out[0])
    assert out[2] == pytest.approx(1.0)


def test_trend_strength_index_reference():
    tsi = ta.TREND_STRENGTH_INDEX(10)
    closes = np.arange(10, dtype=float)
    out = _to_np(tsi.batch(closes))
    # A clean ramp is a perfect uptrend -> signed r^2 = +1.
    assert math.isclose(out[-1], 1.0, abs_tol=1e-9)


def test_polarized_fractal_efficiency_reference():
    pfe = ta.POLARIZED_FRACTAL_EFFICIENCY(5, 3)
    closes = np.arange(20, dtype=float)
    out = _to_np(pfe.batch(closes))
    # On a straight ramp the path equals the diagonal -> efficiency 1 -> +100.
    assert math.isclose(out[-1], 100.0, abs_tol=1e-9)


def test_wave_pm_reference():
    wpm = ta.WAVE_PM(10, 3)
    closes = np.arange(60, dtype=float) * 5.0
    out = _to_np(wpm.batch(closes))
    # Constant-slope ramp: momentum equals its energy -> 100 * (1 - e^-0.5).
    baseline = 100.0 * (1.0 - math.exp(-0.5))
    assert math.isclose(out[-1], baseline, abs_tol=1e-9)


def test_gator_oscillator_reference():
    g = ta.GatorOscillator(13, 8, 5)
    n = 40
    high = np.full(n, 11.0)
    low = np.full(n, 9.0)
    close = np.full(n, 10.0)
    out = _to_np(g.batch(high, low, close))
    # Constant median collapses all three Alligator lines -> both bars zero.
    assert out[-1][0] == pytest.approx(0.0)
    assert out[-1][1] == pytest.approx(0.0)


def test_kase_permission_stochastic_reference():
    k = ta.KasePermissionStochastic(4, 2)
    n = 20
    flat = np.full(n, 10.0)
    out = _to_np(k.batch(flat, flat, flat))
    # HH == LL -> raw %K defaults to the neutral 50 -> both lines at 50.
    assert out[-1][0] == pytest.approx(50.0)
    assert out[-1][1] == pytest.approx(50.0)


def test_tsf_oscillator_reference():
    t = ta.TsfOscillator(3)
    assert t.update(1.0) is None
    assert t.update(2.0) is None
    assert t.update(9.0) == pytest.approx(-33.33333333333333)


def test_macd_histogram_reference():
    # On a constant-slope ramp the MACD line is flat once seeded, so the
    # signal EMA catches up and the histogram collapses to 0.
    t = ta.MacdHistogram(3, 6, 3)
    for i in range(7):
        assert t.update(100.0 + i * 2.0) is None
    assert t.update(100.0 + 7 * 2.0) == pytest.approx(0.0, abs=1e-9)


def test_ppo_histogram_reference():
    # PPO divides the EMA gap by the slow EMA, so on the same ramp the ratio
    # keeps drifting and the histogram stays non-zero.
    t = ta.PpoHistogram(3, 6, 3)
    for i in range(7):
        assert t.update(100.0 + i * 2.0) is None
    assert t.update(100.0 + 7 * 2.0) == pytest.approx(-0.052098, abs=1e-6)


def test_ewma_volatility_reference():
    t = ta.EwmaVolatility(0.94)
    assert t.update(100.0) is None
    assert t.update(110.0) == pytest.approx(0.09531017980432493)
    assert t.update(99.0) == pytest.approx(0.0959428936787596)


def test_garch11_reference():
    t = ta.Garch11(0.000002, 0.1, 0.88)
    assert t.update(100.0) is None
    assert t.update(110.0) == pytest.approx(0.009999999999999995)
    assert t.update(99.0) == pytest.approx(0.031597516317477786)


def test_volatility_cone_reference():
    t = ta.VolatilityCone(20, 60)


def test_quartile_bands_reference():
    t = ta.QuartileBands(4)
    assert t.update(40.0) is None
    assert t.update(30.0) is None
    assert t.update(20.0) is None
    assert t.update(10.0) == pytest.approx((32.5, 25.0, 17.5))


def test_bomar_bands_reference():
    t = ta.BomarBands(4, 0.85)
    assert t.update(100.0) is None
    assert t.update(102.0) is None
    assert t.update(98.0) is None
    assert t.update(104.0) == pytest.approx((104.0, 101.0, 98.0))


def test_median_channel_reference():
    t = ta.MedianChannel(5, 2.0)
    assert t.update(1.0) is None
    assert t.update(2.0) is None
    assert t.update(3.0) is None
    assert t.update(4.0) is None
    assert t.update(5.0) == pytest.approx((5.0, 3.0, 1.0))


def test_projection_bands_reference():
    t = ta.ProjectionBands(3)
    assert t.update((8.0, 10.0, 8.0, 9.0, 1.0, 0)) is None
    assert t.update((9.0, 12.0, 9.0, 11.0, 1.0, 1)) is None
    assert t.update((10.0, 11.0, 10.0, 11.0, 1.0, 2)) == pytest.approx((12.5, 11.25, 10.0))


def test_projection_oscillator_reference():
    # Same window as ProjectionBands: upper 12.5, lower 10; close 11 -> 40.
    t = ta.ProjectionOscillator(3)
    assert t.update((8.0, 10.0, 8.0, 9.0, 1.0, 0)) is None
    assert t.update((9.0, 12.0, 9.0, 11.0, 1.0, 1)) is None
    assert t.update((10.0, 11.0, 10.0, 11.0, 1.0, 2)) == pytest.approx(40.0)


def test_kase_devstop_reference():
    t = ta.KaseDevStop(3, 1.0)
    assert t.update((100.0, 101.0, 99.0, 100.0, 1.0, 0)) is None
    assert t.update((101.0, 102.0, 100.0, 101.0, 1.0, 1)) is None
    assert t.update((102.0, 103.0, 101.0, 102.0, 1.0, 2)) is None
    assert t.update((102.5, 104.0, 102.0, 103.0, 1.0, 3)) == pytest.approx((101.0, 1.0))


def _stop_candles(n):
    # Gently rising, valid OHLC: high >= open/close, low <= open/close.
    return [(100.0 + i, 101.5 + i, 98.5 + i, 100.5 + i, 1.0, i) for i in range(n)]


def test_elder_safezone_reference():
    t = ta.ElderSafeZone(14, 2.0)
    candles = _stop_candles(15)
    for c in candles[:14]:
        assert t.update(c) is None
    assert t.update(candles[14]) == pytest.approx((112.5, 1.0))


def test_atr_ratchet_reference():
    t = ta.AtrRatchet(14, 4.0, 0.1)
    candles = _stop_candles(14)
    for c in candles[:13]:
        assert t.update(c) is None
    assert t.update(candles[13]) == pytest.approx((101.5, 1.0))


def test_nrtr_reference():
    t = ta.Nrtr(2.0)
    assert t.update((100.0, 100.0, 100.0, 100.0, 1.0, 0)) == pytest.approx((98.0, 1.0))


def test_time_based_stop_reference():
    t = ta.TimeBasedStop(5)
    assert t.update((100.0, 101.0, 99.0, 100.0, 1.0, 0)) == pytest.approx(0.2)


def test_modified_ma_stop_reference():
    t = ta.ModifiedMaStop(14)
    candles = _stop_candles(14)
    for c in candles[:13]:
        assert t.update(c) is None
    assert t.update(candles[13]) == pytest.approx((107.0, 1.0))


def test_volume_rsi_reference():
    t = ta.VolumeRsi(14)


def test_twiggs_money_flow_reference():
    t = ta.TwiggsMoneyFlow(21)


def test_trade_volume_index_reference():
    t = ta.TradeVolumeIndex(0.25)


def test_intraday_intensity_reference():
    t = ta.IntradayIntensity()


def test_better_volume_reference():
    t = ta.BetterVolume(14)


def test_volume_weighted_macd_reference():
    t = ta.VolumeWeightedMacd(12, 26, 9)


def test_kendall_tau_reference():
    t = ta.KendallTau(20)


def test_central_pivot_range_reference():
    t = ta.CentralPivotRange()
    assert t.update((105.0, 110.0, 90.0, 105.0, 1.0, 0)) == pytest.approx((101.66666666666667, 103.33333333333334, 100.0))


def test_murrey_math_lines_reference():
    t = ta.MurreyMathLines(4)
    assert t.update((140.0, 180.0, 100.0, 140.0, 1.0, 0)) is None
    assert t.update((140.0, 180.0, 100.0, 140.0, 1.0, 1)) is None
    assert t.update((140.0, 180.0, 100.0, 140.0, 1.0, 2)) is None
    assert t.update((140.0, 180.0, 100.0, 140.0, 1.0, 3)) == pytest.approx((180.0, 170.0, 160.0, 150.0, 140.0, 130.0, 120.0, 110.0, 100.0))


def test_andrews_pitchfork_reference():
    t = ta.AndrewsPitchfork(2)
    # Warmup: no pitchfork until three alternating swing pivots are confirmed.
    assert t.update((100.0, 101.0, 99.0, 100.0, 1.0, 0)) is None


def test_volume_weighted_sr_reference():
    t = ta.VolumeWeightedSr(3)
    assert t.update((100.0, 102.0, 98.0, 100.0, 1.0, 0)) is None
    assert t.update((100.0, 104.0, 96.0, 100.0, 1.0, 1)) is None
    assert t.update((100.0, 106.0, 94.0, 100.0, 1.0, 2)) == pytest.approx((96.0, 104.0))


def test_pivot_reversal_reference():
    t = ta.PivotReversal(1, 1)
    assert t.update((9.5, 10.0, 9.0, 9.5, 1.0, 0)) is None
    assert t.update((11.5, 12.0, 11.0, 11.5, 1.0, 1)) is None
    # Pivot high = 12 confirmed; close 9.5 has not crossed it.
    assert t.update((9.5, 10.0, 9.0, 9.5, 1.0, 2)) == pytest.approx(0.0)
    assert t.update((9.0, 11.0, 9.0, 9.0, 1.0, 3)) == pytest.approx(0.0)
    # Close 13 > pivot high 12 with prev close 9 below it -> bullish reversal.
    assert t.update((13.0, 14.0, 12.5, 13.0, 1.0, 4)) == pytest.approx(1.0)



def test_td_camouflage_reference():
    t = ta.TDCamouflage()
    assert t.update((10.0, 11.0, 8.0, 10.0, 1.0, 0)) is None
    assert t.update((9.0, 10.0, 7.0, 9.5, 1.0, 1)) == pytest.approx(1.0)


def test_td_clop_reference():
    t = ta.TDClop()
    assert t.update((10.0, 12.0, 9.0, 11.0, 1.0, 0)) is None
    assert t.update((9.0, 13.0, 8.0, 12.0, 1.0, 1)) == pytest.approx(1.0)


def test_td_clopwin_reference():
    t = ta.TDClopwin()
    assert t.update((10.0, 15.0, 9.0, 14.0, 1.0, 0)) is None
    assert t.update((11.0, 14.0, 10.0, 13.0, 1.0, 1)) == pytest.approx(1.0)


def test_td_propulsion_reference():
    t = ta.TDPropulsion()
    assert t.update((9.5, 11.0, 9.0, 10.0, 1.0, 0)) is None
    assert t.update((10.5, 12.0, 10.0, 11.5, 1.0, 1)) == pytest.approx(1.0)


def test_td_trap_reference():
    t = ta.TDTrap()
    assert t.update((100.0, 110.0, 90.0, 100.0, 1.0, 0)) is None
    assert t.update((101.5, 108.0, 95.0, 102.0, 1.0, 1)) is None
    assert t.update((106.0, 112.0, 100.0, 109.0, 1.0, 2)) == pytest.approx(1.0)



def test_heikin_ashi_oscillator_reference():
    t = ta.HeikinAshiOscillator(5)


def test_three_line_break_reference():
    t = ta.ThreeLineBreak(3)


def test_smoothed_heikin_ashi_reference():
    t = ta.SmoothedHeikinAshi(5)


def test_equivolume_reference():
    t = ta.Equivolume(20)


def test_candle_volume_reference():
    t = ta.CandleVolume(20)


def test_tristar_reference():
    t = ta.Tristar()
    assert t.update((100.0, 101.0, 99.0, 100.02, 1.0, 0)) is None
    assert t.update((105.0, 106.0, 104.0, 105.02, 1.0, 1)) is None
    assert t.update((100.0, 101.0, 99.0, 100.02, 1.0, 2)) == pytest.approx(-1.0)


def test_harami_cross_reference():
    t = ta.HaramiCross()
    assert t.update((110.0, 110.2, 99.8, 100.0, 1.0, 0)) is None
    assert t.update((105.0, 106.0, 104.0, 105.02, 1.0, 1)) == pytest.approx(1.0)


def test_tower_top_bottom_reference():
    t = ta.TowerTopBottom()
    assert t.update((100.0, 110.1, 99.9, 110.0, 1.0, 0)) is None
    assert t.update((105.0, 107.0, 103.0, 105.1, 1.0, 1)) is None
    assert t.update((110.0, 110.1, 99.9, 100.0, 1.0, 2)) == pytest.approx(-1.0)



def test_hasbrouck_information_share_reference():
    # A flat pair has no error correction to measure -> the neutral 0.5.
    t = ta.HasbrouckInformationShare(6)
    for _ in range(7):
        assert t.update(7.0, 9.0) is None
    assert t.update(7.0, 9.0) == pytest.approx(0.5)


def test_naked_poc_reference():
    t = ta.NakedPoc(20, 24)


def test_single_prints_reference():
    t = ta.SinglePrints(20, 24)


def test_profile_shape_reference():
    t = ta.ProfileShape(20, 24)


def test_high_low_volume_nodes_reference():
    t = ta.HighLowVolumeNodes(20, 24)


def test_composite_profile_reference():
    t = ta.CompositeProfile(20, 24, 0.7)

# --- Lifecycle ------------------------------------------------------------


def test_new_indicators_expose_lifecycle():
    instances = [make() for make, _ in CANDLE_SCALAR.values()]
    instances += [t[0]() for t in MULTI.values()]
    instances += [make() for make, _ in MULTI_SCALAR_INPUT.values()]
    instances += [cls(*args) for cls, args in SCALAR]
    instances += [make() for make, _ in SCALAR_MULTI.values()]
    instances += [make() for make in HLC_BAND3.values()]
    instances += [ta.VwapStdDevBands(2.0)]
    instances.append(ta.Alligator(13, 8, 5))
    instances.append(ta.ZeroLagMACD(12, 26, 9))
    instances += [
        ta.TDPressure(5),
        ta.TDSequential(),
        ta.TDLines(),
        ta.TDRiskLevel(),
        ta.TDRangeProjection(),
        ta.TDOpen(),
    ]
    for ind in instances:
        assert ind.is_ready() is False
        assert ind.warmup_period() >= 1
        ind.reset()
        assert ind.is_ready() is False


# --- Ichimoku & Heikin-Ashi (Family 13) -----------------------------------


def test_ichimoku_batch_shape_and_warmup(ohlcv):
    high, low, close, _ = ohlcv
    ichi = ta.Ichimoku()
    out = _to_np(ichi.batch(high, low, close))
    assert out.shape == (close.size, 5)
    # A row is emitted from the first bar -- every component is optional and
    # they fill in as history allows -- so the declared warmup is 1. The 77 bars
    # the classic (9, 26, 52, 26) configuration needs are when the last
    # component arrives, which the per-column checks below cover.
    assert ichi.warmup_period() == 1
    # Tenkan emits from bar 9; before that, the entire column is NaN.
    assert np.all(np.isnan(out[:8, 0]))
    assert not math.isnan(out[8, 0])


def test_ichimoku_streaming_matches_batch(ohlcv):
    high, low, close, _ = ohlcv
    batch = ta.Ichimoku().batch(high, low, close)

    streamer = ta.Ichimoku()
    rows = []
    for i in range(close.size):
        candle = (
            float(close[i]),
            float(high[i]),
            float(low[i]),
            float(close[i]),
            0.0,
            i,
        )
        v = streamer.update(candle)
        if v is None:
            rows.append([math.nan] * 5)
        else:
            rows.append([math.nan if x is None else float(x) for x in v])
    assert _eq_nan(batch, np.array(rows, dtype=np.float64))


def test_ichimoku_chikou_is_close_displacement_back():
    # A constant series makes Chikou trivially equal to the close.
    n = 60
    close = np.full(n, 100.0)
    high = close + 1.0
    low = close - 1.0
    out = _to_np(ta.Ichimoku().batch(high, low, close))
    # Displacement = 26, so chikou is defined from bar 25 onwards.
    for i in range(25, n):
        assert out[i, 4] == pytest.approx(100.0)


def test_heikin_ashi_seed_and_recursion():
    ha = ta.HeikinAshi()
    # Bar 1: ha_open = (open + close) / 2, ha_close = (O+H+L+C)/4.
    first = ha.update((10.0, 12.0, 9.0, 11.0, 0.0, 0))
    assert first == pytest.approx(
        (
            (10.0 + 11.0) / 2.0,
            max(12.0, (10.0 + 11.0) / 2.0, (10.0 + 12.0 + 9.0 + 11.0) / 4.0),
            min(9.0, (10.0 + 11.0) / 2.0, (10.0 + 12.0 + 9.0 + 11.0) / 4.0),
            (10.0 + 12.0 + 9.0 + 11.0) / 4.0,
        )
    )
    # Bar 2: ha_open = (prev_ha_open + prev_ha_close) / 2.
    second = ha.update((11.5, 13.0, 10.5, 12.0, 0.0, 1))
    assert second[0] == pytest.approx((first[0] + first[3]) / 2.0)
    assert second[3] == pytest.approx((11.5 + 13.0 + 10.5 + 12.0) / 4.0)


def test_heikin_ashi_streaming_matches_batch(ohlcv):
    high, low, close, _ = ohlcv
    open_ = (high + low) / 2.0
    batch = ta.HeikinAshi().batch(open_, high, low, close)

    streamer = ta.HeikinAshi()
    rows = []
    for i in range(close.size):
        candle = (
            float(open_[i]),
            float(high[i]),
            float(low[i]),
            float(close[i]),
            0.0,
            i,
        )
        v = streamer.update(candle)
        rows.append([math.nan] * 4 if v is None else list(v))
    assert _eq_nan(batch, np.array(rows, dtype=np.float64))


def test_heikin_ashi_lifecycle_and_reset():
    ha = ta.HeikinAshi()
    assert ha.is_ready() is False
    assert ha.warmup_period() == 1
    ha.update((10.0, 11.0, 9.0, 10.5, 0.0, 0))
    assert ha.is_ready() is True
    ha.reset()
    assert ha.is_ready() is False
# --- Candlestick pattern reference values --------------------------------


def test_doji_reference():
    # body 0, range 2 -> doji.
    assert ta.Doji().update((10.0, 11.0, 9.0, 10.0, 1.0, 0)) == pytest.approx(1.0)
    # Marubozu shape -> not a doji.
    assert ta.Doji().update((10.0, 12.0, 10.0, 12.0, 1.0, 0)) == pytest.approx(0.0)


def test_hammer_reference():
    # body 0.5, lower shadow 5.0, upper 0.1.
    assert ta.Hammer().update((10.0, 10.6, 5.0, 10.5, 1.0, 0)) == pytest.approx(1.0)


def test_inverted_hammer_reference():
    assert ta.InvertedHammer().update(
        (10.0, 15.0, 9.9, 10.5, 1.0, 0)
    ) == pytest.approx(1.0)


def test_hanging_man_reference():
    assert ta.HangingMan().update((10.0, 10.6, 5.0, 10.5, 1.0, 0)) == pytest.approx(
        -1.0
    )


def test_shooting_star_reference():
    assert ta.ShootingStar().update(
        (10.0, 15.0, 9.9, 10.5, 1.0, 0)
    ) == pytest.approx(-1.0)


def test_engulfing_reference():
    e = ta.Engulfing()
    assert e.update((11.0, 11.2, 9.8, 10.0, 1.0, 0)) is None
    assert e.update((9.5, 12.0, 9.5, 11.5, 1.0, 1)) == pytest.approx(1.0)


def test_harami_reference():
    h = ta.Harami()
    assert h.update((12.0, 12.5, 9.5, 10.0, 1.0, 0)) is None
    assert h.update((10.5, 11.5, 10.4, 11.0, 1.0, 1)) == pytest.approx(1.0)


def test_morning_evening_star_reference():
    m = ta.MorningEveningStar()
    assert m.update((12.0, 12.2, 9.5, 10.0, 1.0, 0)) is None
    assert m.update((9.9, 10.1, 9.7, 9.95, 1.0, 1)) is None
    assert m.update((10.1, 12.0, 10.0, 11.8, 1.0, 2)) == pytest.approx(1.0)


def test_three_soldiers_reference():
    t = ta.ThreeSoldiersOrCrows()
    assert t.update((10.0, 11.5, 9.9, 11.0, 1.0, 0)) is None
    assert t.update((10.5, 12.5, 10.4, 12.0, 1.0, 1)) is None
    assert t.update((11.5, 13.5, 11.4, 13.0, 1.0, 2)) == pytest.approx(1.0)


def test_piercing_dark_cloud_reference():
    p = ta.PiercingDarkCloud()
    assert p.update((12.0, 12.5, 10.0, 10.0, 1.0, 0)) is None
    assert p.update((9.8, 11.8, 9.5, 11.5, 1.0, 1)) == pytest.approx(1.0)


def test_marubozu_reference():
    # Bullish marubozu: open == low, close == high.
    assert ta.Marubozu().update(
        (10.0, 12.0, 10.0, 12.0, 1.0, 0)
    ) == pytest.approx(1.0)
    assert ta.Marubozu().update(
        (12.0, 12.0, 10.0, 10.0, 1.0, 0)
    ) == pytest.approx(-1.0)


def test_tweezer_reference():
    t = ta.Tweezer()
    assert t.update((11.0, 12.0, 9.5, 9.6, 1.0, 0)) is None
    assert t.update((9.7, 10.5, 9.5, 10.2, 1.0, 1)) == pytest.approx(1.0)


def test_spinning_top_reference():
    # body 0.5, both shadows 3.0, range 6.5 -> body/range ~= 0.077.
    assert ta.SpinningTop().update(
        (10.0, 13.5, 7.0, 10.5, 1.0, 0)
    ) == pytest.approx(1.0)


def test_three_inside_reference():
    t = ta.ThreeInside()
    assert t.update((12.0, 12.5, 9.5, 10.0, 1.0, 0)) is None
    assert t.update((10.5, 11.5, 10.4, 11.0, 1.0, 1)) is None
    assert t.update((11.0, 13.0, 10.9, 12.5, 1.0, 2)) == pytest.approx(1.0)


def test_three_outside_reference():
    t = ta.ThreeOutside()
    assert t.update((11.0, 11.2, 9.8, 10.0, 1.0, 0)) is None
    assert t.update((9.5, 12.0, 9.5, 11.5, 1.0, 1)) is None
    assert t.update((11.5, 13.0, 11.4, 12.5, 1.0, 2)) == pytest.approx(1.0)


def test_two_crows_reference():
    t = ta.TwoCrows()
    assert t.update((10.0, 12.2, 9.9, 12.0, 1.0, 0)) is None
    assert t.update((14.0, 14.2, 12.9, 13.0, 1.0, 1)) is None
    assert t.update((13.5, 13.6, 10.9, 11.0, 1.0, 2)) == pytest.approx(-1.0)


def test_upside_gap_two_crows_reference():
    t = ta.UpsideGapTwoCrows()
    assert t.update((10.0, 12.2, 9.9, 12.0, 1.0, 0)) is None
    assert t.update((14.0, 14.2, 12.9, 13.0, 1.0, 1)) is None
    assert t.update((15.0, 15.2, 12.4, 12.5, 1.0, 2)) == pytest.approx(-1.0)


def test_identical_three_crows_reference():
    t = ta.IdenticalThreeCrows()
    assert t.update((13.0, 13.1, 11.9, 12.0, 1.0, 0)) is None
    assert t.update((12.0, 12.1, 10.9, 11.0, 1.0, 1)) is None
    assert t.update((11.0, 11.1, 9.9, 10.0, 1.0, 2)) == pytest.approx(-1.0)


def test_three_line_strike_reference():
    t = ta.ThreeLineStrike()
    assert t.update((10.0, 11.1, 9.9, 11.0, 1.0, 0)) is None
    assert t.update((10.5, 12.1, 10.4, 12.0, 1.0, 1)) is None
    assert t.update((11.5, 13.1, 11.4, 13.0, 1.0, 2)) is None
    assert t.update((13.5, 13.6, 9.4, 9.5, 1.0, 3)) == pytest.approx(1.0)


def test_three_stars_in_south_reference():
    t = ta.ThreeStarsInSouth()
    assert t.update((20.0, 20.1, 8.0, 15.0, 1.0, 0)) is None
    assert t.update((18.0, 18.1, 12.0, 16.0, 1.0, 1)) is None
    assert t.update((15.0, 15.0, 14.0, 14.0, 1.0, 2)) == pytest.approx(1.0)


def test_abandoned_baby_reference():
    t = ta.AbandonedBaby()
    assert t.update((20.0, 20.1, 14.9, 15.0, 1.0, 0)) is None
    assert t.update((13.0, 13.1, 12.9, 13.0, 1.0, 1)) is None
    assert t.update((16.0, 18.1, 15.9, 18.0, 1.0, 2)) == pytest.approx(1.0)


def test_advance_block_reference():
    t = ta.AdvanceBlock()
    assert t.update((10.0, 13.1, 9.9, 13.0, 1.0, 0)) is None
    assert t.update((12.0, 14.3, 11.9, 14.0, 1.0, 1)) is None
    assert t.update((13.5, 15.0, 13.4, 14.5, 1.0, 2)) == pytest.approx(-1.0)


# --- Lifecycle ------------------------------------------------------------


def test_new_indicators_expose_lifecycle():
    instances = [make() for make, _ in CANDLE_SCALAR.values()]
    instances += [make() for make, *_ in MULTI.values()]
    instances += [cls(*args) for cls, args in SCALAR]
    for ind in instances:
        assert ind.is_ready() is False
        assert ind.warmup_period() >= 1
        ind.reset()
        assert ind.is_ready() is False


def _orderbook_snapshots(n: int) -> list:
    """A deterministic varying sequence of order-book snapshots."""
    snaps = []
    for i in range(n):
        bid_sz = 1.0 + (i % 5)
        ask_sz = 1.0 + ((i + 2) % 4)
        snaps.append(
            (
                [100.0, 99.0],
                [bid_sz, 1.0],
                [101.0, 102.0],
                [ask_sz, 1.0],
            )
        )
    return snaps


def test_orderbook_indicators_streaming_equals_batch():
    snaps = _orderbook_snapshots(40)
    for make in (
        ta.OrderBookImbalanceTop1,
        lambda: ta.OrderBookImbalanceTopN(2),
        ta.OrderBookImbalanceFull,
        ta.Microprice,
        ta.QuotedSpread,
        ta.DepthSlope,
        lambda: ta.OrderFlowImbalance(10),
    ):
        batch = make().batch(snaps)
        streamer = make()
        streamed = np.array(
            [streamer.update(*snap) for snap in snaps], dtype=np.float64
        )
        assert len(batch) == len(snaps)
        assert _eq_nan(batch, streamed)


def test_tradeflow_indicators_streaming_equals_batch():
    n = 40
    price = np.full(n, 100.0)
    size = np.array([1.0 + (i % 5) for i in range(n)], dtype=np.float64)
    is_buy = [i % 2 == 0 for i in range(n)]
    for make in (
        ta.SignedVolume,
        ta.CumulativeVolumeDelta,
        lambda: ta.TradeImbalance(5),
        lambda: ta.Vpin(8.0, 5),
        lambda: ta.AmihudIlliquidity(14),
        lambda: ta.RollMeasure(14),
        lambda: ta.TradeSignAutocorrelation(10),
        lambda: ta.Pin(10),
    ):
        batch = make().batch(price, size, is_buy)
        streamer = make()
        streamed = np.array(
            [streamer.update(price[i], size[i], is_buy[i]) for i in range(n)],
            dtype=np.float64,
        )
        assert len(batch) == n
        assert _eq_nan(batch, streamed)


def test_trade_sign_autocorrelation_reference():
    # Perfectly alternating aggressor signs -> lag-1 autocorrelation -1.
    t = ta.TradeSignAutocorrelation(10)
    last = None
    for i in range(20):
        last = t.update(100.0, 1.0, i % 2 == 0)
    assert last == pytest.approx(-1.0)
    # All buys -> perfectly persistent flow -> +1.
    t2 = ta.TradeSignAutocorrelation(10)
    for _ in range(20):
        last2 = t2.update(100.0, 1.0, True)
    assert last2 == pytest.approx(1.0)


def test_pin_reference():
    # One-sided flow (all buys) -> maximally informed -> PIN 1.
    p = ta.Pin(10)
    last = None
    for _ in range(20):
        last = p.update(100.0, 1.0, True)
    assert last == pytest.approx(1.0)
    # Balanced flow -> uninformed -> PIN 0.
    p2 = ta.Pin(10)
    for i in range(20):
        last2 = p2.update(100.0, 1.0, i % 2 == 0)
    assert last2 == pytest.approx(0.0)


def test_price_impact_indicators_streaming_equals_batch():
    n = 40
    mid = np.array([100.0 + 0.5 * math.sin(i * 0.4) for i in range(n)], dtype=np.float64)
    is_buy = [i % 2 == 0 for i in range(n)]
    # Aggressive trades print across the mid in the aggressor's direction.
    price = np.array(
        [mid[i] + (0.02 if is_buy[i] else -0.02) for i in range(n)], dtype=np.float64
    )
    size = np.array([1.0 + (i % 5) for i in range(n)], dtype=np.float64)
    for make in (ta.EffectiveSpread, lambda: ta.RealizedSpread(4), lambda: ta.KylesLambda(5)):
        batch = make().batch(price, size, is_buy, mid)
        streamer = make()
        streamed = np.array(
            [streamer.update(price[i], size[i], is_buy[i], mid[i]) for i in range(n)],
            dtype=np.float64,
        )
        assert len(batch) == n
        assert _eq_nan(batch, streamed)


def test_footprint_streaming_equals_batch():
    n = 20
    price = [100.0 + (i % 5) * 0.3 for i in range(n)]
    size = [1.0 + (i % 3) for i in range(n)]
    is_buy = [i % 2 == 0 for i in range(n)]
    batch = ta.Footprint(1.0).batch(price, size, is_buy)
    streamer = ta.Footprint(1.0)
    assert len(batch) == n
    for i in range(n):
        streamed = streamer.update(price[i], size[i], is_buy[i])
        assert np.array_equal(_to_np(streamed), _to_np(batch[i]))


def test_funding_indicators_streaming_equals_batch():
    n = 40
    rate = np.array([0.0001 * math.sin(i * 0.3) for i in range(n)], dtype=np.float64)
    for make in (
        ta.FundingRate,
        lambda: ta.FundingRateMean(5),
        lambda: ta.FundingRateZScore(5),
    ):
        batch = make().batch(rate)
        streamer = make()
        streamed = np.array(
            [streamer.update(rate[i]) for i in range(n)], dtype=np.float64
        )
        assert len(batch) == n
        assert _eq_nan(batch, streamed)


def test_advance_decline_streaming_equals_batch():
    # Three ticks over a universe of four symbols; the sign of `change`
    # classifies each symbol as advancing / declining / unchanged.
    change = [
        [1.0, 0.5, 2.0, -1.0],  # 3 up, 1 down -> net +2
        [-1.0, -0.5, -2.0, 1.0],  # 1 up, 3 down -> net -2
        [0.0, 0.0, 1.0, -1.0],  # 1 up, 1 down -> net 0
    ]
    volume = [[10.0] * 4 for _ in range(3)]
    new_high = [[False] * 4 for _ in range(3)]
    new_low = [[False] * 4 for _ in range(3)]
    batch = ta.AdvanceDecline().batch(change, volume, new_high, new_low)
    streamer = ta.AdvanceDecline()
    streamed = np.array(
        [
            streamer.update(change[i], volume[i], new_high[i], new_low[i])
            for i in range(3)
        ],
        dtype=np.float64,
    )
    assert len(batch) == 3
    assert _eq_nan(batch, streamed)
    # Cumulative line: +2 -> 0 -> 0.
    assert list(batch) == [2.0, 0.0, 0.0]


def test_advance_decline_rejects_ragged_universe():
    ad = ta.AdvanceDecline()
    with pytest.raises(ValueError):
        ad.update([1.0, -1.0], [10.0], [False, False], [False, False])


def _breadth_streaming_equals_batch(indicator, change, volume, new_high, new_low):
    """Assert a 4-array breadth indicator's batch matches its streaming output."""
    batch = indicator().batch(change, volume, new_high, new_low)
    streamer = indicator()
    streamed = np.array(
        [
            streamer.update(change[i], volume[i], new_high[i], new_low[i])
            for i in range(len(change))
        ],
        dtype=np.float64,
    )
    assert len(batch) == len(change)
    assert _eq_nan(batch, streamed)
    return batch


def test_advance_decline_ratio_breadth():
    change = [[1.0, 1.0, 1.0, -1.0], [1.0, 0.0, 0.0, 0.0], [-1.0, -1.0, -1.0, -1.0]]
    volume = [[10.0] * 4 for _ in range(3)]
    flags = [[False] * 4 for _ in range(3)]
    batch = _breadth_streaming_equals_batch(ta.AdvanceDeclineRatio, change, volume, flags, flags)
    # 3/1 = 3 ; 1/max(0,1) = 1 ; 0/3 = 0.
    assert list(batch) == [3.0, 1.0, 0.0]


def test_ad_volume_line_breadth():
    change = [[1.0, -1.0], [1.0, -1.0], [1.0, 0.0]]
    volume = [[150.0, 50.0], [60.0, 60.0], [30.0, 0.0]]
    flags = [[False] * 2 for _ in range(3)]
    batch = _breadth_streaming_equals_batch(ta.AdVolumeLine, change, volume, flags, flags)
    # net +100 -> 100 ; net 0 -> 100 ; net +30 -> 130.
    assert list(batch) == [100.0, 100.0, 130.0]


def test_mcclellan_oscillator_breadth():
    change = [[1.0, 1.0, 1.0, -1.0], [-1.0, -1.0, -1.0, 1.0], [1.0, 1.0, -1.0, -1.0]]
    volume = [[10.0] * 4 for _ in range(3)]
    flags = [[False] * 4 for _ in range(3)]
    batch = _breadth_streaming_equals_batch(ta.McClellanOscillator, change, volume, flags, flags)
    # seed 0 ; -50 ; -67.5.
    assert abs(batch[0]) < 1e-9
    assert abs(batch[1] - (-50.0)) < 1e-9
    assert abs(batch[2] - (-67.5)) < 1e-9


def test_mcclellan_summation_index_breadth():
    change = [[1.0, 1.0, 1.0, -1.0], [-1.0, -1.0, -1.0, 1.0], [1.0, 1.0, -1.0, -1.0]]
    volume = [[10.0] * 4 for _ in range(3)]
    flags = [[False] * 4 for _ in range(3)]
    batch = _breadth_streaming_equals_batch(ta.McClellanSummationIndex, change, volume, flags, flags)
    # 0 ; -50 ; -117.5.
    assert abs(batch[0]) < 1e-9
    assert abs(batch[1] - (-50.0)) < 1e-9
    assert abs(batch[2] - (-117.5)) < 1e-9


def test_trin_breadth():
    change = [[1.0, 1.0, 1.0, -1.0], [1.0, 1.0, -1.0, -1.0]]
    volume = [[50.0, 50.0, 50.0, 50.0], [10.0, 10.0, 40.0, 40.0]]
    flags = [[False] * 4 for _ in range(2)]
    batch = _breadth_streaming_equals_batch(ta.Trin, change, volume, flags, flags)
    # (3/1)/(150/50) = 1 ; (2/2)/(20/80) = 4.
    assert abs(batch[0] - 1.0) < 1e-9
    assert abs(batch[1] - 4.0) < 1e-9


def test_breadth_thrust_breadth():
    change = [[1.0] * 8 + [-1.0] * 2, [1.0] * 6 + [-1.0] * 4]
    volume = [[10.0] * 10 for _ in range(2)]
    flags = [[False] * 10 for _ in range(2)]
    batch = ta.BreadthThrust(2).batch(change, volume, flags, flags)
    streamer = ta.BreadthThrust(2)
    streamed = np.array(
        [streamer.update(change[i], volume[i], flags[i], flags[i]) for i in range(2)],
        dtype=np.float64,
    )
    assert _eq_nan(batch, streamed)
    # 0.8 (warmup -> NaN) ; SMA(2) of [0.8, 0.6] = 0.7.
    assert math.isnan(batch[0])
    assert abs(batch[1] - 0.7) < 1e-9


def test_new_highs_new_lows_breadth():
    change = [[1.0, 1.0, -1.0], [1.0, -1.0, -1.0]]
    volume = [[10.0] * 3 for _ in range(2)]
    new_high = [[True, True, False], [True, False, False]]
    new_low = [[False, False, True], [False, True, True]]
    batch = _breadth_streaming_equals_batch(ta.NewHighsNewLows, change, volume, new_high, new_low)
    # 2 - 1 = 1 ; 1 - 2 = -1.
    assert list(batch) == [1.0, -1.0]


def test_high_low_index_breadth():
    change = [[1.0] * 10, [1.0] * 10]
    volume = [[10.0] * 10 for _ in range(2)]
    new_high = [[True] * 8 + [False] * 2, [True] * 6 + [False] * 4]
    new_low = [[False] * 8 + [True] * 2, [False] * 6 + [True] * 4]
    batch = ta.HighLowIndex(2).batch(change, volume, new_high, new_low)
    streamer = ta.HighLowIndex(2)
    streamed = np.array(
        [streamer.update(change[i], volume[i], new_high[i], new_low[i]) for i in range(2)],
        dtype=np.float64,
    )
    assert _eq_nan(batch, streamed)
    # 80% (warmup) ; SMA(2) of [80, 60] = 70.
    assert math.isnan(batch[0])
    assert abs(batch[1] - 70.0) < 1e-9


def test_percent_above_ma_breadth():
    change = [[1.0, 1.0, 1.0, -1.0], [1.0, 1.0, -1.0, -1.0]]
    volume = [[10.0] * 4 for _ in range(2)]
    flags = [[False] * 4 for _ in range(2)]
    above_ma = [[True, True, True, False], [True, False, False, False]]
    batch = ta.PercentAboveMa().batch(change, volume, flags, flags, above_ma)
    streamer = ta.PercentAboveMa()
    streamed = np.array(
        [streamer.update(change[i], volume[i], flags[i], flags[i], above_ma[i]) for i in range(2)],
        dtype=np.float64,
    )
    assert _eq_nan(batch, streamed)
    # 3/4 -> 75 ; 1/4 -> 25.
    assert list(batch) == [75.0, 25.0]


def test_up_down_volume_ratio_breadth():
    change = [[1.0, -1.0], [1.0, 0.0]]
    volume = [[150.0, 50.0], [100.0, 0.0]]
    flags = [[False] * 2 for _ in range(2)]
    batch = _breadth_streaming_equals_batch(ta.UpDownVolumeRatio, change, volume, flags, flags)
    # 150/50 = 3 ; 100/max(0,1) = 100.
    assert list(batch) == [3.0, 100.0]


def test_bullish_percent_index_breadth():
    change = [[1.0, 1.0, -1.0, -1.0], [1.0, 1.0, 1.0, 1.0]]
    volume = [[10.0] * 4 for _ in range(2)]
    flags = [[False] * 4 for _ in range(2)]
    on_buy = [[True, True, False, False], [True, True, True, True]]
    batch = ta.BullishPercentIndex().batch(change, volume, flags, flags, on_buy)
    streamer = ta.BullishPercentIndex()
    streamed = np.array(
        [streamer.update(change[i], volume[i], flags[i], flags[i], on_buy[i]) for i in range(2)],
        dtype=np.float64,
    )
    assert _eq_nan(batch, streamed)
    # 2/4 -> 50 ; 4/4 -> 100.
    assert list(batch) == [50.0, 100.0]


def test_cumulative_volume_index_breadth():
    change = [[1.0, -1.0], [1.0, -1.0], [0.0]]
    volume = [[150.0, 50.0], [60.0, 60.0], [0.0]]
    new_high = [[False, False], [False, False], [False]]
    new_low = [[False, False], [False, False], [False]]
    batch = ta.CumulativeVolumeIndex().batch(change, volume, new_high, new_low)
    streamer = ta.CumulativeVolumeIndex()
    streamed = np.array(
        [streamer.update(change[i], volume[i], new_high[i], new_low[i]) for i in range(3)],
        dtype=np.float64,
    )
    assert _eq_nan(batch, streamed)
    # 150 - 50 -> 100 ; net 0 -> 100 ; zero-volume tick -> 100.
    assert list(batch) == [100.0, 100.0, 100.0]


def test_absolute_breadth_index_breadth():
    change = [[1.0, 1.0, -1.0, -1.0, -1.0], [1.0, 1.0, 1.0, -1.0, -1.0]]
    volume = [[10.0] * 5 for _ in range(2)]
    flags = [[False] * 5 for _ in range(2)]
    batch = _breadth_streaming_equals_batch(ta.AbsoluteBreadthIndex, change, volume, flags, flags)
    # |2 - 3| = 1 ; |3 - 2| = 1.
    assert list(batch) == [1.0, 1.0]


def test_tick_index_breadth():
    change = [[1.0, 1.0, -1.0, -1.0, -1.0], [1.0, 1.0, 1.0, -1.0, -1.0]]
    volume = [[10.0] * 5 for _ in range(2)]
    flags = [[False] * 5 for _ in range(2)]
    batch = _breadth_streaming_equals_batch(ta.TickIndex, change, volume, flags, flags)
    # 2 - 3 = -1 ; 3 - 2 = 1.
    assert list(batch) == [-1.0, 1.0]


def test_funding_basis_streaming_equals_batch():
    n = 40
    index = np.array([100.0 + 0.5 * math.sin(i * 0.2) for i in range(n)], dtype=np.float64)
    mark = np.array(
        [index[i] + 0.1 * math.cos(i * 0.3) for i in range(n)], dtype=np.float64
    )
    batch = ta.FundingBasis().batch(mark, index)
    streamer = ta.FundingBasis()
    streamed = np.array(
        [streamer.update(mark[i], index[i]) for i in range(n)], dtype=np.float64
    )
    assert len(batch) == n
    assert _eq_nan(batch, streamed)


def test_open_interest_delta_streaming_equals_batch():
    n = 40
    oi = np.array([1000.0 + 50.0 * math.sin(i * 0.25) for i in range(n)], dtype=np.float64)
    batch = ta.OpenInterestDelta().batch(oi)
    streamer = ta.OpenInterestDelta()
    streamed = np.array([streamer.update(oi[i]) for i in range(n)], dtype=np.float64)
    assert len(batch) == n
    assert _eq_nan(batch, streamed)


def test_oi_flow_indicators_streaming_equals_batch():
    n = 40
    oi = np.array([1000.0 + 50.0 * math.sin(i * 0.2) for i in range(n)], dtype=np.float64)
    mark = np.array([100.0 + math.cos(i * 0.3) for i in range(n)], dtype=np.float64)
    long_sz = np.array([500.0 + 20.0 * math.sin(i * 0.25) for i in range(n)], dtype=np.float64)
    short_sz = np.array([400.0 + 20.0 * math.cos(i * 0.25) for i in range(n)], dtype=np.float64)

    # OIPriceDivergence carries a window; update(open_interest, mark_price).
    batch = ta.OIPriceDivergence(5).batch(oi, mark)
    streamer = ta.OIPriceDivergence(5)
    streamed = np.array(
        [streamer.update(oi[i], mark[i]) for i in range(n)], dtype=np.float64
    )
    assert len(batch) == n
    assert _eq_nan(batch, streamed)

    # OIWeighted; update(mark_price, open_interest).
    batch = ta.OIWeighted().batch(mark, oi)
    streamer = ta.OIWeighted()
    streamed = np.array(
        [streamer.update(mark[i], oi[i]) for i in range(n)], dtype=np.float64
    )
    assert _eq_nan(batch, streamed)

    # LongShortRatio; update(long_size, short_size).
    batch = ta.LongShortRatio().batch(long_sz, short_sz)
    streamer = ta.LongShortRatio()
    streamed = np.array(
        [streamer.update(long_sz[i], short_sz[i]) for i in range(n)], dtype=np.float64
    )
    assert _eq_nan(batch, streamed)

    # TakerBuySellRatio; update(taker_buy_volume, taker_sell_volume).
    batch = ta.TakerBuySellRatio().batch(long_sz, short_sz)
    streamer = ta.TakerBuySellRatio()
    streamed = np.array(
        [streamer.update(long_sz[i], short_sz[i]) for i in range(n)], dtype=np.float64
    )
    assert _eq_nan(batch, streamed)


def test_liquidation_features_streaming_equals_batch():
    n = 30
    long_liq = np.array([abs(50.0 * math.sin(i * 0.4)) for i in range(n)], dtype=np.float64)
    short_liq = np.array([abs(40.0 * math.cos(i * 0.3)) for i in range(n)], dtype=np.float64)
    batch = ta.LiquidationFeatures().batch(long_liq, short_liq)
    streamer = ta.LiquidationFeatures()
    assert batch.shape == (n, 5)
    for i in range(n):
        row = streamer.update(long_liq[i], short_liq[i])
        assert tuple(batch[i]) == pytest.approx(row)


def test_basis_indicators_streaming_equals_batch():
    n = 40
    index = np.array([100.0 + math.sin(i * 0.2) for i in range(n)], dtype=np.float64)
    mark = np.array([index[i] + 0.05 * math.cos(i * 0.3) for i in range(n)], dtype=np.float64)
    futures = np.array(
        [index[i] + 0.5 + 0.1 * math.sin(i * 0.25) for i in range(n)], dtype=np.float64
    )

    # TermStructureBasis; update(futures_price, index_price).
    batch = ta.TermStructureBasis().batch(futures, index)
    streamer = ta.TermStructureBasis()
    streamed = np.array(
        [streamer.update(futures[i], index[i]) for i in range(n)], dtype=np.float64
    )
    assert len(batch) == n
    assert _eq_nan(batch, streamed)

    # CalendarSpread; update(futures_price, mark_price).
    batch = ta.CalendarSpread().batch(futures, mark)
    streamer = ta.CalendarSpread()
    streamed = np.array(
        [streamer.update(futures[i], mark[i]) for i in range(n)], dtype=np.float64
    )
    assert _eq_nan(batch, streamed)


def test_b16_derivatives_reference():
    # Estimated leverage: oi / exchange reserve = 200 / 100 = 2.
    assert ta.EstimatedLeverageRatio().update(200.0, 100.0) == pytest.approx(2.0)
    # OI-to-volume: oi / (buy + sell) = 100 / 50 = 2.
    assert ta.OiToVolumeRatio().update(100.0, 30.0, 20.0) == pytest.approx(2.0)
    # Perpetual premium: (mark - index) / index = 0.5 / 100 = 0.005.
    assert ta.PerpetualPremiumIndex().update(100.5, 100.0) == pytest.approx(0.005)
    # Funding-implied APR: rate * intervals = 0.0001 * 1095 = 0.1095.
    assert ta.FundingImpliedApr(1095.0).update(0.0001) == pytest.approx(0.1095)
    # Open-interest momentum (period 2): warmup then ROC% = 100*(120-100)/100 = 20.
    oim = ta.OpenInterestMomentum(2)
    assert oim.update(100.0) is None
    assert oim.update(110.0) is None
    assert oim.update(120.0) == pytest.approx(20.0)


def test_b16_derivatives_streaming_equals_batch():
    n = 40
    oi = np.array([1000.0 + 50.0 * math.sin(i * 0.3) for i in range(n)], dtype=np.float64)
    long_sz = np.array([600.0 + 20.0 * math.cos(i * 0.2) for i in range(n)], dtype=np.float64)
    short_sz = np.array([400.0 + 15.0 * math.sin(i * 0.4) for i in range(n)], dtype=np.float64)
    buy = np.array([300.0 + 10.0 * math.sin(i * 0.5) for i in range(n)], dtype=np.float64)
    sell = np.array([250.0 + 12.0 * math.cos(i * 0.35) for i in range(n)], dtype=np.float64)
    index = np.array([100.0 + math.sin(i * 0.2) for i in range(n)], dtype=np.float64)
    mark = np.array([index[i] + 0.05 * math.cos(i * 0.3) for i in range(n)], dtype=np.float64)
    rate = np.array([0.0001 * math.sin(i * 0.3) for i in range(n)], dtype=np.float64)

    # EstimatedLeverageRatio; update(open_interest, exchange_reserve).
    reserve = long_sz + short_sz
    batch = ta.EstimatedLeverageRatio().batch(oi, reserve)
    streamer = ta.EstimatedLeverageRatio()
    streamed = np.array(
        [streamer.update(oi[i], reserve[i]) for i in range(n)], dtype=np.float64
    )
    assert len(batch) == n
    assert _eq_nan(batch, streamed)

    # OiToVolumeRatio; update(open_interest, taker_buy_volume, taker_sell_volume).
    batch = ta.OiToVolumeRatio().batch(oi, buy, sell)
    streamer = ta.OiToVolumeRatio()
    streamed = np.array(
        [streamer.update(oi[i], buy[i], sell[i]) for i in range(n)], dtype=np.float64
    )
    assert _eq_nan(batch, streamed)

    # PerpetualPremiumIndex; update(mark_price, index_price).
    batch = ta.PerpetualPremiumIndex().batch(mark, index)
    streamer = ta.PerpetualPremiumIndex()
    streamed = np.array(
        [streamer.update(mark[i], index[i]) for i in range(n)], dtype=np.float64
    )
    assert _eq_nan(batch, streamed)

    # FundingImpliedApr; update(funding_rate).
    batch = ta.FundingImpliedApr(1095.0).batch(rate)
    streamer = ta.FundingImpliedApr(1095.0)
    streamed = np.array([streamer.update(rate[i]) for i in range(n)], dtype=np.float64)
    assert _eq_nan(batch, streamed)

    # OpenInterestMomentum; update(open_interest).
    batch = ta.OpenInterestMomentum(10).batch(oi)
    streamer = ta.OpenInterestMomentum(10)
    streamed = np.array([streamer.update(oi[i]) for i in range(n)], dtype=np.float64)
    assert _eq_nan(batch, streamed)


# --- Alt-Chart Bars ------------------------------------------------------


def test_renko_bars_reference():
    r = ta.RenkoBars(1.0)
    assert r.update(10.0) == []  # seed
    assert r.update(13.0) == [(10.0, 11.0, 1), (11.0, 12.0, 1), (12.0, 13.0, 1)]
    assert r.update(10.0) == [(12.0, 11.0, -1), (11.0, 10.0, -1)]  # 2-box reversal


def test_renko_bars_batch_shape():
    r = ta.RenkoBars(1.0)
    out = _to_np(r.batch(np.array([10.0, 11.0, 12.0, 13.0])))
    assert out.shape == (3, 3)
    np.testing.assert_allclose(out[:, 2], [1.0, 1.0, 1.0])


def test_kagi_bars_reference():
    k = ta.KagiBars(2.0)
    assert k.update(10.0) == []  # seed
    assert k.update(11.0) == []  # establishes up
    assert k.update(15.0) == []  # extends
    assert k.update(12.0) == [(10.0, 15.0, 1)]  # reversal closes up segment


def test_point_and_figure_bars_reference():
    pnf = ta.PointAndFigureBars(1.0, 3)
    assert pnf.update(10.0) == []  # seed
    assert pnf.update(13.0) == []  # starts X column
    assert pnf.update(15.0) == []  # extends up
    assert pnf.update(12.0) == [(1, 15.0, 10.0)]  # 3-box reversal closes X column


def test_bar_builders_reset():
    r = ta.RenkoBars(1.0)
    r.update(10.0)
    r.update(15.0)
    r.reset()
    assert r.update(50.0) == []  # re-seeds after reset


def test_range_bars_reference():
    rb = ta.RangeBars(1.0)
    assert rb.update(10.0) == []  # seed
    assert rb.update(13.0) == [(10.0, 11.0, 1), (11.0, 12.0, 1), (12.0, 13.0, 1)]


def test_range_bars_batch_shape():
    rb = ta.RangeBars(1.0)
    out = _to_np(rb.batch(np.array([10.0, 11.0, 12.0, 13.0])))
    assert out.shape == (3, 3)
    np.testing.assert_allclose(out[:, 2], [1.0, 1.0, 1.0])


def test_tick_bars_reference():
    tb = ta.TickBars(2)
    assert tb.update(10.0, 11.0, 9.0, 10.5, 100.0) == []
    out = tb.update(10.5, 12.0, 10.0, 11.0, 150.0)
    assert len(out) == 1
    assert out[0] == (10.0, 12.0, 9.0, 11.0, 250.0)


def test_tick_bars_batch_shape():
    tb = ta.TickBars(2)
    col = np.array([10.0, 10.0, 10.0, 10.0])
    vol = np.array([1.0, 1.0, 1.0, 1.0])
    out = _to_np(tb.batch(col, col, col, col, vol))
    assert out.shape == (2, 5)


def test_volume_bars_reference():
    vb = ta.VolumeBars(100.0)
    assert vb.update(10.0, 10.0, 10.0, 10.0, 60.0) == []
    out = vb.update(10.5, 10.5, 10.5, 10.5, 60.0)
    assert len(out) == 1
    assert out[0][4] == 120.0  # accumulated volume


def test_dollar_bars_reference():
    db = ta.DollarBars(1000.0)
    assert db.update(10.0, 10.0, 10.0, 10.0, 60.0) == []  # 600
    out = db.update(10.0, 10.0, 10.0, 10.0, 60.0)  # 1200 >= 1000
    assert len(out) == 1
    assert out[0][4] == 120.0  # volume
    assert out[0][5] == 1200.0  # traded value


def test_imbalance_bars_reference():
    ib = ta.ImbalanceBars(3.0)
    assert ib.update(10.0, 10.0, 10.0, 10.0) == []  # seed
    ib.update(11.0, 11.0, 11.0, 11.0)  # +1
    ib.update(12.0, 12.0, 12.0, 12.0)  # +2
    out = ib.update(13.0, 13.0, 13.0, 13.0)  # +3 -> close
    assert len(out) == 1
    assert out[0][4] == 3.0  # imbalance
    assert out[0][5] == 1  # direction


def test_run_bars_reference():
    rb = ta.RunBars(3)
    assert rb.update(10.0, 10.0, 10.0, 10.0) == []  # seed
    rb.update(11.0, 11.0, 11.0, 11.0)  # run 1
    rb.update(12.0, 12.0, 12.0, 12.0)  # run 2
    out = rb.update(13.0, 13.0, 13.0, 13.0)  # run 3 -> close
    assert len(out) == 1
    assert out[0][4] == 3  # length
    assert out[0][5] == 1  # direction


def test_three_line_break_bars_reference():
    tlb = ta.ThreeLineBreakBars(3)
    assert tlb.update(10.0) == []  # seed
    assert tlb.update(11.0) == [(10.0, 11.0, 1)]


def test_three_line_break_bars_batch_shape():
    tlb = ta.ThreeLineBreakBars(3)
    out = _to_np(tlb.batch(np.array([10.0, 11.0, 12.0, 13.0])))
    assert out.shape[1] == 3
