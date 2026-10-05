"""Reference-value tests that pin numerical behaviour from the Python side."""

from __future__ import annotations

import math

import numpy as np
import pytest

import wickra as ta


def _to_np(x) -> np.ndarray:
    """Normalize a Wickra batch result (``array.array`` or ``Matrix``) to an ndarray."""
    if isinstance(x, np.ndarray):
        return x.astype(np.float64)
    if hasattr(x, "tolist") and hasattr(x, "shape"):
        return np.asarray(x.tolist(), dtype=np.float64)
    return np.asarray(x, dtype=np.float64)


def test_sma_constant_series():
    out = _to_np(ta.SMA(5).batch(np.full(20, 42.0, dtype=np.float64)))
    # First 4 are warmup -> NaN; rest equal 42.
    assert np.all(np.isnan(out[:4]))
    assert np.allclose(out[4:], 42.0)


def test_sma_known_window():
    # SMA(3) of [2, 4, 6, 8, 10] -> [_, _, 4, 6, 8]
    out = _to_np(ta.SMA(3).batch(np.array([2.0, 4.0, 6.0, 8.0, 10.0])))
    assert math.isnan(out[0]) and math.isnan(out[1])
    np.testing.assert_allclose(out[2:], [4.0, 6.0, 8.0])


def test_ema_seed_equals_simple_mean_of_first_window():
    # EMA(5) seed = mean([10, 20, 30, 40, 50]) = 30
    out = _to_np(ta.EMA(5).batch(np.array([10.0, 20.0, 30.0, 40.0, 50.0])))
    assert math.isnan(out[0])
    assert math.isclose(out[4], 30.0, abs_tol=1e-12)


def test_wma_known_window():
    # WMA(4) of [1, 2, 3, 4] = (1*1 + 2*2 + 3*3 + 4*4)/10 = 3
    out = _to_np(ta.WMA(4).batch(np.array([1.0, 2.0, 3.0, 4.0])))
    assert math.isnan(out[0]) and math.isnan(out[1]) and math.isnan(out[2])
    assert math.isclose(out[3], 3.0, abs_tol=1e-12)


def test_rsi_pure_uptrend_is_100():
    out = _to_np(ta.RSI(14).batch(np.arange(1.0, 21.0, dtype=np.float64)))
    np.testing.assert_allclose(out[14:], 100.0)


def test_rsi_pure_downtrend_is_0():
    out = _to_np(ta.RSI(14).batch(np.arange(20.0, 0.0, -1.0)))
    np.testing.assert_allclose(out[14:], 0.0)


def test_rsi_flat_series_is_50():
    out = _to_np(ta.RSI(14).batch(np.full(30, 100.0)))
    np.testing.assert_allclose(out[14:], 50.0)


def test_rsi_wilder_textbook_first_value():
    """Wilder's original 14-period example, ~70.46 at the first emit."""
    prices = np.array(
        [
            44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08,
            45.89, 46.03, 45.61, 46.28, 46.28,
        ],
        dtype=np.float64,
    )
    out = _to_np(ta.RSI(14).batch(prices))
    assert math.isclose(out[14], 70.464, abs_tol=0.05)


def test_anchored_rsi_cumulative_reference():
    """Cumulative anchored RSI: 10 -> 11 (+1) -> 9 (-2) -> 12 (+3)."""
    out = _to_np(ta.AnchoredRSI().batch(np.array([10.0, 11.0, 9.0, 12.0])))
    assert math.isclose(out[1], 100.0, abs_tol=1e-9)
    assert math.isclose(out[2], 100.0 - 100.0 / 1.5, abs_tol=1e-6)
    assert math.isclose(out[3], 100.0 - 100.0 / 3.0, abs_tol=1e-6)


def test_inertia_constant_close_passes_through_linreg():
    # Every bar identical (open, high, low, close) = (10, 11, 9, 10.5): the
    # close never moves, so the Relative Volatility Index sits at its neutral
    # 50, and LinReg of a constant series equals that constant after warmup.
    n = 60
    out = _to_np(ta.Inertia(3, 4).batch(
        np.full(n, 10.0), np.full(n, 11.0), np.full(n, 9.0), np.full(n, 10.5)
    ))
    # warmup_period = (2·3 − 1) + 4 − 1 = 8.
    np.testing.assert_allclose(out[7:], 50.0, atol=1e-12)


def test_connors_rsi_output_is_bounded():
    # CRSI is the average of three [0, 100] components, so the aggregate must
    # also sit in [0, 100] after warmup.
    prices = 100.0 + 20.0 * np.sin(np.linspace(0, 30, 250))
    out = _to_np(ta.ConnorsRSI(3, 2, 100).batch(prices.astype(np.float64)))
    ready = out[~np.isnan(out)]
    assert ready.size > 0
    assert ready.min() >= 0.0
    assert ready.max() <= 100.0


def test_laguerre_rsi_constant_series_stays_at_mid_band():
    # All four Laguerre stages seed to the first input, so subsequent flat
    # inputs keep them equal and the up/down accumulator is 0 — Wickra maps
    # that to the neutral 50.
    out = _to_np(ta.LaguerreRSI(0.5).batch(np.full(40, 42.0, dtype=np.float64)))
    np.testing.assert_allclose(out, 50.0, atol=1e-12)


def test_smi_close_at_centre_yields_zero():
    # Close at the midpoint of a flat high/low range -> displacement is
    # always zero -> SMI converges to 0.
    n = 60
    out = _to_np(ta.SMI(5, 3, 3).batch(np.full(n, 11.0), np.full(n, 9.0), np.full(n, 10.0)))
    # warmup_period = 5 + 3 + 3 - 2 = 9.
    np.testing.assert_allclose(out[8:], 0.0, atol=1e-12)


def test_kst_constant_series_yields_zero():
    # ROC is zero on a flat input, so every RCMA is zero, so KST and its
    # signal SMA are both zero after warmup.
    kst = ta.KST(10, 15, 20, 30, 10, 10, 10, 15, 9)
    out = _to_np(kst.batch(np.full(80, 42.0, dtype=np.float64)))
    warmup = kst.warmup_period()
    # Use NaN-safe comparison on the post-warmup tail.
    tail = out[warmup - 1 :]
    assert np.all(np.isfinite(tail))
    np.testing.assert_allclose(tail, 0.0, atol=1e-12)


def test_pgo_flat_close_yields_zero():
    # On a constant close the numerator (close − SMA) is zero, so PGO emits 0
    # regardless of the TR-EMA in the denominator.
    n = 20
    high = np.full(n, 11.0)
    low = np.full(n, 9.0)
    close = np.full(n, 10.0)
    out = _to_np(ta.PGO(5).batch(high, low, close))
    assert np.all(np.isnan(out[:4]))
    np.testing.assert_allclose(out[4:], 0.0, atol=1e-12)


def test_rvi_reference_value_period_2():
    # Four bars (open, high, low, close) = (10, 11, 9, 10.5), then
    # (10.5, 11.5, 10, 11.5). With Ehlers' 1-2-2-1 weighting:
    #   RVI = (0.5 + 3.5/6) / (2 + 11.5/6) = 6.5 / 23.5.
    out = _to_np(ta.RVI(2).batch(
        np.array([10.0, 10.0, 10.0, 10.0, 10.5]),
        np.array([11.0, 11.0, 11.0, 11.0, 11.5]),
        np.array([9.0, 9.0, 9.0, 9.0, 10.0]),
        np.array([10.5, 10.5, 10.5, 10.5, 11.5]),
    ))
    assert all(math.isnan(x) for x in out[:4])
    assert math.isclose(out[4], 6.5 / 23.5, abs_tol=1e-12)


def test_alma_constant_series_yields_the_constant():
    # ALMA's Gaussian weights are normalised, so any constant series is
    # reproduced exactly after warmup.
    out = _to_np(ta.ALMA(9, 0.85, 6.0).batch(np.full(30, 42.0, dtype=np.float64)))
    assert np.all(np.isnan(out[:8]))
    np.testing.assert_allclose(out[8:], 42.0, atol=1e-12)


def test_alma_reference_value_period_3():
    # ALMA(period=3, offset=0.85, sigma=6) on [10, 20, 30].
    # m = 0.85 * 2 = 1.7; s = 3 / 6 = 0.5; 2*s^2 = 0.5.
    out = _to_np(ta.ALMA(3, 0.85, 6.0).batch(np.array([10.0, 20.0, 30.0])))
    assert math.isnan(out[0]) and math.isnan(out[1])
    # Independently compute the expected Gaussian-weighted sum.
    w = np.exp(-((np.arange(3, dtype=np.float64) - 1.7) ** 2) / 0.5)
    expected = float(np.dot([10.0, 20.0, 30.0], w) / w.sum())
    assert math.isclose(out[2], expected, abs_tol=1e-12)
    # Sanity: heavy offset toward the newest sample lifts the average above
    # the simple mean of 20.
    assert out[2] > 20.0


def test_mcginley_dynamic_constant_series_yields_the_constant():
    # ratio = 1, so the recurrence collapses to MD + 0 / divisor = MD.
    out = _to_np(ta.McGinleyDynamic(5).batch(np.full(30, 42.0, dtype=np.float64)))
    assert np.all(np.isnan(out[:4]))
    np.testing.assert_allclose(out[4:], 42.0, atol=1e-12)


def test_mcginley_dynamic_reference_value():
    # Period 3, seed = SMA([10, 20, 30]) = 20.0. Next price 40.0:
    # ratio   = 2; divisor = 0.6 * 3 * 16 = 28.8; next = 20 + 20/28.8.
    out = _to_np(ta.McGinleyDynamic(3).batch(np.array([10.0, 20.0, 30.0, 40.0])))
    assert math.isnan(out[0]) and math.isnan(out[1])
    assert math.isclose(out[2], 20.0, abs_tol=1e-12)
    expected = 20.0 + 20.0 / (0.6 * 3.0 * 16.0)
    assert math.isclose(out[3], expected, abs_tol=1e-12)


def test_frama_constant_series_yields_the_constant():
    # Flat input -> degenerate ranges -> alpha clamps to 0.01 and the EMA
    # recurrence holds the seed value.
    out = _to_np(ta.FRAMA(4).batch(np.full(20, 42.0, dtype=np.float64)))
    assert np.all(np.isnan(out[:3]))
    np.testing.assert_allclose(out[3:], 42.0, atol=1e-12)


def test_frama_pure_uptrend_hugs_latest():
    # Monotonic uptrend -> alpha pushed toward 1.0, FRAMA tracks close.
    out = _to_np(ta.FRAMA(4).batch(np.arange(1.0, 9.0, dtype=np.float64)))
    assert math.isclose(out[-1], 8.0, abs_tol=0.05)


def test_jma_constant_series_yields_the_constant():
    # JMA seeds e0 and the output to the first input, so a constant series
    # is reproduced exactly from the first sample.
    out = _to_np(ta.JMA(14, 0.0, 2).batch(np.full(30, 42.0, dtype=np.float64)))
    np.testing.assert_allclose(out, 42.0, atol=1e-12)


def test_evwma_reference_value_period_2():
    # EVWMA(2). Bars: (close, volume) = (10, 1), (20, 3), (30, 1).
    #   Bar 2: sum_v = 4, seeded prev = 20, EVWMA = (1*20 + 3*20)/4 = 20.
    #   Bar 3: sum_v = 4 (drops 1, gains 1), EVWMA = (3*20 + 1*30)/4 = 22.5.
    out = _to_np(ta.EVWMA(2).batch(np.array([10.0, 20.0, 30.0]), np.array([1.0, 3.0, 1.0])))
    assert math.isnan(out[0])
    assert math.isclose(out[1], 20.0, abs_tol=1e-12)
    assert math.isclose(out[2], 22.5, abs_tol=1e-12)


def test_alligator_constant_series_holds_at_median_price():
    # Median price = (11 + 9) / 2 = 10 on every candle, so all three SMMAs
    # seed at 10 and stay there.
    n = 30
    high = np.full(n, 11.0)
    low = np.full(n, 9.0)
    out = _to_np(ta.Alligator(13, 8, 5).batch(high, low))
    assert out.shape == (n, 3)
    for row in out[12:]:
        assert math.isclose(row[0], 10.0, abs_tol=1e-12)
        assert math.isclose(row[1], 10.0, abs_tol=1e-12)
        assert math.isclose(row[2], 10.0, abs_tol=1e-12)


def test_vidya_constant_series_holds_seed():
    # CMO = 0 on a flat series -> alpha = 0 -> VIDYA holds its seed value.
    out = _to_np(ta.VIDYA(14, 4).batch(np.full(20, 42.0, dtype=np.float64)))
    assert np.all(np.isnan(out[:4]))
    np.testing.assert_allclose(out[4:], 42.0, atol=1e-12)


def test_zero_lag_macd_constant_series_converges_to_zero():
    # Each inner ZLEMA reproduces a constant, so macd, signal and histogram
    # are all 0 once the slowest branch warms up.
    out = _to_np(ta.ZeroLagMACD(3, 5, 3).batch(np.full(60, 42.0, dtype=np.float64)))
    # Take the last row and verify all three columns are 0.
    last = out[-1]
    assert math.isclose(last[0], 0.0, abs_tol=1e-12)
    assert math.isclose(last[1], 0.0, abs_tol=1e-12)
    assert math.isclose(last[2], 0.0, abs_tol=1e-12)


def test_awesome_oscillator_histogram_flat_series_converges_to_zero():
    # Flat median price -> AO = 0 -> SMA(AO) = 0 -> AOHist = 0.
    n = 50
    high = np.full(n, 11.0)
    low = np.full(n, 9.0)
    out = ta.AwesomeOscillatorHistogram(3, 5, 3).batch(high, low)  # AO momentum
    # warmup = slow + lookback = 5 + 3 = 8.
    np.testing.assert_allclose(out[7:], 0.0, atol=1e-12)


def test_stc_constant_series_yields_zero():
    # Flat input collapses both stochastic stages to zero -> STC stays at 0.
    out = _to_np(ta.STC(3, 5, 4, 0.5).batch(np.full(60, 42.0, dtype=np.float64)))
    ready = out[~np.isnan(out)]
    assert ready.size > 0
    np.testing.assert_array_equal(ready[-5:], np.zeros(5))


def test_elder_impulse_constant_series_is_neutral():
    # Flat input -> neither EMA nor MACD histogram moves -> Impulse stays at 0.
    out = _to_np(ta.ElderImpulse(13, 12, 26, 9).batch(np.full(120, 42.0, dtype=np.float64)))
    ready = out[~np.isnan(out)]
    assert ready.size > 0
    np.testing.assert_array_equal(ready, np.zeros_like(ready))


def test_cfo_perfect_linear_series_yields_zero():
    # LinReg of a perfectly linear series fits exactly, so CFO = 0 after warmup.
    out = _to_np(ta.CFO(5).batch(np.arange(1.0, 21.0, dtype=np.float64) * 2.0))
    np.testing.assert_allclose(out[4:], 0.0, atol=1e-9)


def test_apo_constant_series_converges_to_zero():
    # Both EMAs reproduce a constant exactly, so APO = 0 after warmup.
    out = _to_np(ta.APO(3, 5).batch(np.full(30, 42.0, dtype=np.float64)))
    assert np.all(np.isnan(out[:4]))
    np.testing.assert_allclose(out[4:], 0.0, atol=1e-12)


def test_macd_constant_series_converges_to_zero():
    out = _to_np(ta.MACD().batch(np.full(200, 100.0)))
    # Last row's MACD and signal must be ~0.
    last = out[-1]
    assert math.isclose(last[0], 0.0, abs_tol=1e-9)
    assert math.isclose(last[1], 0.0, abs_tol=1e-9)
    assert math.isclose(last[2], 0.0, abs_tol=1e-9)


def test_bollinger_constant_series_zero_width():
    out = _to_np(ta.BollingerBands(20, 2.0).batch(np.full(50, 100.0)))
    row = out[-1]
    np.testing.assert_allclose(row, [100.0, 100.0, 100.0, 0.0], atol=1e-12)


def test_bollinger_upper_middle_lower_ordering():
    out = _to_np(ta.BollingerBands(20, 2.0).batch(np.linspace(50.0, 150.0, 100)))
    ready = out[~np.isnan(out[:, 0])]
    assert np.all(ready[:, 0] >= ready[:, 1])
    assert np.all(ready[:, 1] >= ready[:, 2])
    assert np.all(ready[:, 3] >= 0.0)


def test_atr_constant_range_constant_output():
    high = np.full(30, 11.0)
    low = np.full(30, 9.0)
    close = np.full(30, 10.0)
    out = _to_np(ta.ATR(14).batch(high, low, close))
    # Once seeded, ATR equals the constant TR of 2.
    np.testing.assert_allclose(out[13:], 2.0, atol=1e-12)


def test_stochastic_extremes():
    # Close at the top of a 3-period range -> %K = 100.
    high = np.array([10.0, 11.0, 12.0])
    low = np.array([8.0, 9.0, 10.0])
    close = np.array([9.0, 10.0, 12.0])
    out = _to_np(ta.Stochastic(3, 1).batch(high, low, close))
    assert math.isclose(out[2, 0], 100.0, abs_tol=1e-12)


def test_obv_cumulative_known_sequence():
    close = np.array([10.0, 11.0, 10.5, 10.5, 12.0])
    volume = np.array([100.0, 20.0, 30.0, 40.0, 10.0])
    out = _to_np(ta.OBV().batch(close, volume))
    np.testing.assert_allclose(out, [0.0, 20.0, -10.0, -10.0, 0.0])


# --- Family 15: Risk / Performance ---------------------------------------


def test_sharpe_ratio_known_window():
    # returns [0.01, 0.02, 0.03, 0.04], rf = 0; mean = 0.025;
    # sample-var = 0.000166...; Sharpe = 0.025 / sqrt(var).
    out = _to_np(ta.SharpeRatio(4, 0.0).batch(np.array([0.01, 0.02, 0.03, 0.04])))
    expected = 0.025 / math.sqrt(0.000_166_666_666_666_666_67)
    assert math.isclose(out[3], expected, rel_tol=1e-9)


def test_sortino_ratio_known_window():
    # returns [-0.02, 0.01, -0.01, 0.03], mar = 0; mean = 0.0025;
    # downside_sq = 0.0005; dd = sqrt(0.0005/4); Sortino = 0.0025/dd.
    out = _to_np(ta.SortinoRatio(4, 0.0).batch(np.array([-0.02, 0.01, -0.01, 0.03])))
    expected = 0.0025 / math.sqrt(0.000_125)
    assert math.isclose(out[3], expected, rel_tol=1e-9)


def test_max_drawdown_known_window():
    # window [100, 120, 90] -> peak 120, trough 90 -> 25% drawdown.
    out = _to_np(ta.MaxDrawdown(3).batch(np.array([100.0, 120.0, 90.0])))
    assert math.isclose(out[2], 0.25, abs_tol=1e-12)


def test_pain_index_known_window():
    # dd[0..2] = 0, 0, 0.25; mean = 0.25/3.
    out = _to_np(ta.PainIndex(3).batch(np.array([100.0, 120.0, 90.0])))
    assert math.isclose(out[2], 0.25 / 3.0, abs_tol=1e-12)


def test_profit_factor_known_window():
    # gains 0.05, losses 0.03 -> PF = 5/3.
    out = _to_np(ta.ProfitFactor(4).batch(np.array([0.02, -0.01, 0.03, -0.02])))
    assert math.isclose(out[3], 5.0 / 3.0, rel_tol=1e-9)


def test_gain_loss_ratio_known_window():
    # avg_win 0.03, avg_loss 0.02 -> GLR = 1.5.
    out = _to_np(ta.GainLossRatio(4).batch(np.array([0.02, -0.01, 0.04, -0.03])))
    assert math.isclose(out[3], 1.5, rel_tol=1e-9)


def test_omega_ratio_known_window():
    # gains 0.04, losses 0.03 -> Omega = 4/3.
    out = _to_np(ta.OmegaRatio(4, 0.0).batch(np.array([-0.02, 0.01, -0.01, 0.03])))
    assert math.isclose(out[3], 4.0 / 3.0, rel_tol=1e-9)


def test_kelly_criterion_known_window():
    # n_win=n_loss=2, payoff=2 -> Kelly = 0.5 - 0.5/2 = 0.25.
    out = _to_np(ta.KellyCriterion(4).batch(np.array([0.02, 0.04, -0.01, -0.02])))
    assert math.isclose(out[3], 0.25, rel_tol=1e-9)


def test_drawdown_duration_under_water_counter():
    out = _to_np(ta.DrawdownDuration().batch(np.array([100.0, 95.0, 90.0, 85.0])))
    np.testing.assert_allclose(out, [0.0, 1.0, 2.0, 3.0])


def test_recovery_factor_known_path():
    # Start 100, peak 110, trough 88 -> max_dd = 0.20; end 130 ->
    # net_return = 0.30 -> Recovery = 1.5.
    prices = np.array([100.0, 110.0, 105.0, 95.0, 88.0, 100.0, 120.0, 130.0])
    out = _to_np(ta.RecoveryFactor().batch(prices))
    assert math.isclose(out[-1], 1.5, rel_tol=1e-9)


def test_alpha_perfect_capm_fit_yields_zero():
    bench = np.array([0.01 * i for i in range(1, 21)])
    asset = 2.0 * bench
    out = _to_np(ta.Alpha(20, 0.0).batch(asset, bench))
    assert math.isclose(out[-1], 0.0, abs_tol=1e-12)


def test_alpha_additive_offset_recovered():
    bench = np.array([0.01 * i for i in range(1, 21)])
    asset = bench + 0.005
    out = _to_np(ta.Alpha(20, 0.0).batch(asset, bench))
    assert math.isclose(out[-1], 0.005, rel_tol=1e-9)


def test_treynor_ratio_known_window():
    bench = np.array([0.01 * i for i in range(1, 21)])
    asset = 2.0 * bench
    out = _to_np(ta.TreynorRatio(20, 0.0).batch(asset, bench))
    assert math.isclose(out[-1], bench.mean(), rel_tol=1e-9)


def test_information_ratio_known_window():
    asset = np.array([0.02, 0.04, 0.06, 0.08])
    bench = np.array([0.01, 0.02, 0.03, 0.04])
    out = _to_np(ta.InformationRatio(4).batch(asset, bench))
    expected = 0.025 / math.sqrt(0.000_166_666_666_666_666_67)
    assert math.isclose(out[-1], expected, rel_tol=1e-9)


def test_pairwise_beta_squared_price_is_two():
    # a = b² ⇒ a's log-returns are exactly 2× b's ⇒ pairwise beta = 2.
    # b must have *varying* returns (a constant-return path has zero variance
    # and an undefined slope, which the indicator reports as 0).
    b = np.array([100.0 + 10.0 * math.sin(i * 0.5) for i in range(20)])
    a = b**2
    out = _to_np(ta.PairwiseBeta(5).batch(a, b))
    assert math.isclose(out[-1], 2.0, rel_tol=1e-9)


def test_pairwise_beta_inverse_price_is_minus_one():
    # a = 1/b ⇒ a's log-returns are −1× b's ⇒ pairwise beta = −1.
    b = np.array([100.0 + 10.0 * math.sin(i * 0.5) for i in range(20)])
    a = 1.0 / b
    out = _to_np(ta.PairwiseBeta(5).batch(a, b))
    assert math.isclose(out[-1], -1.0, rel_tol=1e-9)


def test_pair_spread_zscore_flat_benchmark_sign():
    # Flat b ⇒ hedge ratio 0 ⇒ spread = ln(a). With z_period = 2 the z-score
    # collapses to the sign of the last move: rising a ⇒ +1, falling a ⇒ −1.
    a = np.array([100.0, 100.0, 110.0, 105.0, 130.0])
    b = np.full_like(a, 100.0)
    out = _to_np(ta.PairSpreadZScore(2, 2).batch(a, b))
    assert math.isclose(out[-1], 1.0, abs_tol=1e-9)
    assert math.isclose(out[-2], -1.0, abs_tol=1e-9)


def test_lead_lag_cross_correlation_negative_lead():
    # a is a delayed copy of b ⇒ b leads a ⇒ lag = −2, correlation ≈ 1.
    def sig(t):
        return math.sin(t * 0.4) + 0.4 * math.sin(t * 1.1) + 0.2 * math.cos(t * 0.27)

    n = 60
    a = np.array([sig(t - 2) for t in range(n)])
    b = np.array([sig(t) for t in range(n)])
    out = _to_np(ta.LeadLagCrossCorrelation(12, 5).batch(a, b))
    assert int(out[-1, 0]) == -2
    assert out[-1, 1] > 0.99


def test_cointegration_perfect_pair():
    # a = 2*b + 5 exactly ⇒ hedge ratio 2, zero spread, degenerate ADF ⇒ 0.
    b = np.array([100.0 + t for t in range(40)])
    a = 2.0 * b + 5.0
    out = _to_np(ta.Cointegration(20, 1).batch(a, b))
    assert math.isclose(out[-1, 0], 2.0, rel_tol=1e-9)
    assert math.isclose(out[-1, 1], 0.0, abs_tol=1e-6)
    assert math.isclose(out[-1, 2], 0.0, abs_tol=1e-12)


def test_relative_strength_rising_ratio_is_overbought():
    # a rises while b is flat ⇒ ratio strictly increases ⇒ RSI saturates at 100.
    n = 20
    a = np.array([100.0 + 2.0 * t for t in range(n)])
    b = np.full(n, 100.0)
    out = _to_np(ta.RelativeStrengthAB(5, 5).batch(a, b))
    assert out[-1, 0] > 1.0
    assert math.isclose(out[-1, 2], 100.0, abs_tol=1e-9)


def test_value_at_risk_known_window():
    # returns -5..4 *0.01; q=0.05*9=0.45 -> -0.0455; VaR = 0.0455.
    returns = np.array([i * 0.01 for i in range(-5, 5)])
    out = _to_np(ta.ValueAtRisk(10, 0.95).batch(returns))
    assert math.isclose(out[-1], 0.0455, rel_tol=1e-9)


def test_conditional_value_at_risk_known_window():
    # tail = {-0.10}; CVaR = 0.10.
    returns = np.array([i * 0.01 for i in range(-10, 10)])
    out = _to_np(ta.ConditionalValueAtRisk(20, 0.95).batch(returns))
    assert math.isclose(out[-1], 0.10, rel_tol=1e-9)


def test_calmar_ratio_known_path():
    # returns [0.10, -0.20, 0.05]; equity 1.0->1.10->0.88->0.924;
    # mdd = 0.20; mean = -0.01666...; Calmar = mean / 0.20.
    out = _to_np(ta.CalmarRatio(3).batch(np.array([0.10, -0.20, 0.05])))
    expected = ((0.10 - 0.20 + 0.05) / 3.0) / 0.20
    assert math.isclose(out[-1], expected, rel_tol=1e-9)


def test_average_drawdown_known_window():
    # window [100, 120, 90, 110]: one drawdown episode (peak 120, trough 90),
    # never recovering -> depth (120-90)/120 = 0.25; one episode -> AvgDD = 0.25.
    out = _to_np(ta.AverageDrawdown(4).batch(np.array([100.0, 120.0, 90.0, 110.0])))
    expected = 0.25
    assert math.isclose(out[-1], expected, rel_tol=1e-12)


def test_value_area_concentrated_volume_locates_poc():
    # Bars 0..3 sit at price 100 with low volume; bar 4 dumps massive volume
    # at price 110. POC must fall inside the high-volume bar's [low, high]
    # range; ties resolve to the lowest-index bin, so the POC may sit on the
    # left edge of bar 4's range rather than at its midpoint.
    high = np.array([100.5, 100.5, 100.5, 100.5, 110.5])
    low = np.array([99.5, 99.5, 99.5, 99.5, 109.5])
    volume = np.array([1.0, 1.0, 1.0, 1.0, 1000.0])
    out = _to_np(ta.ValueArea(5, 50, 0.70).batch(high, low, volume))
    poc = out[-1, 0]
    assert 109.5 <= poc <= 110.5
    # VAH >= POC >= VAL.
    assert out[-1, 1] >= poc >= out[-1, 2]


def test_initial_balance_locks_after_period():
    # First two bars set IB = [99, 103]. Third bar (extreme) must be ignored.
    high = np.array([102.0, 103.0, 200.0])
    low = np.array([100.0, 99.0, 50.0])
    out = _to_np(ta.InitialBalance(2).batch(high, low))
    # Bar 0: IB = [100, 102]; Bar 1: IB locked at [99, 103]; Bar 2: unchanged.
    np.testing.assert_allclose(out[0], [102.0, 100.0])
    np.testing.assert_allclose(out[1], [103.0, 99.0])
    np.testing.assert_allclose(out[2], [103.0, 99.0])


def test_opening_range_breakout_distance_signed():
    # OR locks after 2 bars at high 103 / low 100; mid 101.5. Third bar
    # closes at 105 -> breakout +3.5; fourth bar closes at 95 -> -6.5.
    high = np.array([102.0, 103.0, 110.0, 110.0])
    low = np.array([100.0, 101.0, 102.0, 90.0])
    close = np.array([101.0, 102.0, 105.0, 95.0])
    out = _to_np(ta.OpeningRange(2).batch(high, low, close))
    assert math.isclose(out[2, 0], 103.0)
    assert math.isclose(out[2, 1], 100.0)
    assert math.isclose(out[2, 2], 105.0 - 101.5)
    assert math.isclose(out[3, 2], 95.0 - 101.5)


# --- Family 10 — Ehlers / Cycle reference values ---


def test_inverse_fisher_saturates_for_large_input():
    # tanh(10) ~ 0.99999996; very close to +1 without exceeding.
    v = ta.InverseFisherTransform(1.0).batch(np.array([10.0]))[0]
    assert v < 1.0
    assert v > 0.999


def test_super_smoother_constant_input_is_constant():
    out = _to_np(ta.SuperSmoother(20).batch(np.full(200, 50.0)))
    # Steady-state gain is 1, so a flat input stays flat.
    np.testing.assert_allclose(out[-50:], 50.0, atol=1e-9)


def test_decycler_oscillator_flat_series_is_zero():
    out = _to_np(ta.DecyclerOscillator(10, 30).batch(np.full(80, 42.0)))
    ready = out[~np.isnan(out)]
    np.testing.assert_allclose(ready, 0.0, atol=1e-9)


def test_mama_constant_series_both_lines_converge_to_price():
    out = _to_np(ta.MAMA().batch(np.full(200, 100.0)))
    last = out[-1]
    # MAMA and FAMA both track price closely on a flat series.
    assert abs(last[0] - 100.0) < 1.0
    assert abs(last[1] - 100.0) < 1.0


# --- DeMark family ---------------------------------------------------------


def test_td_setup_buy_setup_completes_at_minus_9_uptrend():
    # Strictly rising closes -> every bar has close > close[-4] (sell setup);
    # the streak hits -9 at index 12 and caps there.
    h = np.arange(2.0, 22.0)
    l = h - 1.0
    c = h - 0.5
    out = _to_np(ta.TDSetup(4, 9).batch(h, l, c))
    assert out[12] == pytest.approx(-9.0)
    assert out[-1] == pytest.approx(-9.0)


def test_td_demarker_downtrend_pegs_at_zero():
    n = 20
    h = np.arange(30.0, 30.0 - n, -1.0)
    l = h - 2.0
    out = _to_np(ta.TDDeMarker(5).batch(h, l))
    assert out[-1] == pytest.approx(0.0)


def test_td_pressure_pure_bearish_yields_minus_100():
    n = 20
    open_ = np.full(n, 11.0)
    high = np.full(n, 11.0)
    low = np.full(n, 9.0)
    close = np.full(n, 9.0)
    volume = np.full(n, 100.0)
    out = _to_np(ta.TDPressure(5).batch(open_, high, low, close, volume))
    assert out[-1] == pytest.approx(-100.0)


def test_td_combo_uptrend_completes_to_minus_13():
    # Pure uptrend -> setup completes, then combo conditions (close>=high[-2],
    # high>=prev.high, close>prev.close) all hold for every subsequent bar
    # -> sell combo saturates at -13.
    n = 40
    high = np.arange(1.0, 1.0 + n) + 0.5
    low = high - 1.0
    close = high - 0.5
    out = _to_np(ta.TDCombo().batch(high, low, close))
    assert out[-1] == pytest.approx(-13.0)


def test_td_countdown_uptrend_completes_to_minus_13():
    n = 40
    high = np.arange(1.0, 1.0 + n) + 0.5
    low = high - 1.0
    close = high - 0.5
    out = _to_np(ta.TDCountdown().batch(high, low, close))
    assert out[-1] == pytest.approx(-13.0)


def test_td_range_projection_doji_reference():
    # open=close=10, high=12, low=9 -> doji branch.
    # pivot_sum = 12 + 9 + 2*10 = 41; half = 20.5.
    # projHigh = 20.5 - 9 = 11.5; projLow = 20.5 - 12 = 8.5.
    out = _to_np(ta.TDRangeProjection().batch(
        np.array([10.0]), np.array([12.0]), np.array([9.0]), np.array([10.0])
    ))
    assert out[0, 0] == pytest.approx(11.5)
    assert out[0, 1] == pytest.approx(8.5)


def test_td_open_sell_signal_reference():
    # Prev high=12. Curr open=13 > 12, curr low=11 < 12 -> -1.
    td = ta.TDOpen()
    assert td.update((10.0, 12.0, 9.0, 11.0, 1.0, 0)) is None
    assert td.update((13.0, 13.5, 11.0, 11.5, 1.0, 1)) == pytest.approx(-1.0)


def test_td_differential_sell_signal_reference():
    # Closes 8 -> 9 -> 9.8 (two higher closes).
    # Bar 1: TrueLow = min(8, 8) = 8 -> buying 1; TrueHigh = max(10, 8) = 10 -> selling 1.
    # Bar 2: TrueLow = min(9.5, 9) = 9 -> buying 0.8 < 1; TrueHigh = 11.5 -> selling 1.7 > 1.
    td = ta.TDDifferential()
    assert td.update((8.0, 9.0, 7.0, 8.0, 1.0, 0)) is None
    assert td.update((9.0, 10.0, 8.0, 9.0, 1.0, 1)) is None
    assert td.update((9.8, 11.5, 9.5, 9.8, 1.0, 2)) == pytest.approx(-1.0)


def test_td_lines_uptrend_support_reference():
    # Strictly rising series -> sell setup completes at idx 12, the
    # lowest low across bars 4..=12 is the low at idx 4 = 4.5.
    n = 20
    high = np.arange(1.0, 1.0 + n) + 0.5
    low = high - 1.0
    close = high - 0.5
    out = _to_np(ta.TDLines().batch(high, low, close))
    assert math.isnan(out[-1, 0])
    assert out[-1, 1] == pytest.approx(4.5)


def test_td_risk_level_uptrend_sell_risk_reference():
    # Strictly rising series -> sell setup completes at idx 12 with high
    # 13.5 and true range 1.5 -> sell_risk = 13.5 + 1.5 = 15.0.
    # Subsequent setups re-ratchet the level, so we check the first emission
    # at idx 12 rather than the latest value.
    n = 20
    high = np.arange(1.0, 1.0 + n) + 0.5
    low = high - 1.0
    close = high - 0.5
    out = _to_np(ta.TDRiskLevel().batch(high, low, close))
    assert math.isnan(out[12, 0])
    assert out[12, 1] == pytest.approx(15.0)


def test_percentage_trailing_stop_seed_and_ratchet():
    # 10% trail: first close 100 -> stop 90; next 110 -> stop max(90, 99) = 99.
    s = ta.PercentageTrailingStop(10.0)
    assert math.isclose(s.update(100.0), 90.0, abs_tol=1e-12)
    assert math.isclose(s.update(110.0), 99.0, abs_tol=1e-12)


def test_step_trailing_stop_snaps_below_close():
    # step 1: floor((100.4 - 1) / 1) = 99.
    s = ta.StepTrailingStop(1.0)
    assert math.isclose(s.update(100.4), 99.0, abs_tol=1e-12)


def test_renko_trailing_stop_holds_until_full_block():
    # block 1: seed 100 -> stop 99; 100.5 still 99; 101 -> stop 100.
    s = ta.RenkoTrailingStop(1.0)
    assert math.isclose(s.update(100.0), 99.0, abs_tol=1e-12)
    assert math.isclose(s.update(100.5), 99.0, abs_tol=1e-12)
    assert math.isclose(s.update(101.0), 100.0, abs_tol=1e-12)


def test_donchian_stop_window_extremes():
    # 5-bar window of highs 1..5 and lows 0..4.
    high = np.array([1.0, 2.0, 3.0, 4.0, 5.0])
    low = np.array([0.0, 1.0, 2.0, 3.0, 4.0])
    out = _to_np(ta.DonchianStop(5).batch(high, low))
    # First 4 rows NaN, fifth row: stop_long = 0, stop_short = 5.
    for i in range(4):
        assert math.isnan(out[i, 0])
        assert math.isnan(out[i, 1])
    assert math.isclose(out[4, 0], 0.0, abs_tol=1e-12)
    assert math.isclose(out[4, 1], 5.0, abs_tol=1e-12)


def test_hilo_activator_flat_market_holds_low_sma():
    # Flat candles H=11, L=9, C=10 -> close (10) sits between bands, so the
    # initial long seed is preserved: emitted stop = lo_sma = 9.
    h = np.full(15, 11.0)
    l = np.full(15, 9.0)
    c = np.full(15, 10.0)
    out = _to_np(ta.HiLoActivator(3).batch(h, l, c))
    # warmup_period == period + 1 == 4, so indices 0..2 are NaN; index 3 onwards is 9.
    for i in range(3):
        assert math.isnan(out[i])
    for i in range(3, 15):
        assert math.isclose(out[i], 9.0, abs_tol=1e-12)


def test_volty_stop_flat_market_constant_level():
    # ATR=2, mult=2 -> band 4; anchor stays at close 10 -> stop = 10 - 4 = 6.
    h = np.full(20, 11.0)
    l = np.full(20, 9.0)
    c = np.full(20, 10.0)
    out = _to_np(ta.VoltyStop(5, 2.0).batch(h, l, c))
    for i in range(4):
        assert math.isnan(out[i])
    for i in range(4, 20):
        assert math.isclose(out[i], 6.0, abs_tol=1e-12)


def test_yoyo_exit_flat_market_constant_level():
    # ATR=2, mult=2 -> band 4; trail = close - band = 10 - 4 = 6 and holds.
    h = np.full(20, 11.0)
    l = np.full(20, 9.0)
    c = np.full(20, 10.0)
    out = _to_np(ta.YoyoExit(5, 2.0).batch(h, l, c))
    for i in range(4):
        assert math.isnan(out[i])
    for i in range(4, 20):
        assert math.isclose(out[i], 6.0, abs_tol=1e-12)


def test_rvi_volatility_pure_uptrend_saturates_at_one_hundred():
    # Strictly rising closes -> every stddev sample classified as "up" ->
    # RVIVolatility saturates at 100. Renamed from the original ta.RVI in
    # PR 42 to disambiguate from Family 02's Relative Vigor Index, which
    # now owns the short ta.RVI name (candle input).
    out = _to_np(ta.RVIVolatility(5).batch(np.arange(1.0, 41.0, dtype=np.float64)))
    ready = out[~np.isnan(out)]
    assert ready.size > 0
    np.testing.assert_allclose(ready[-10:], 100.0, atol=1e-9)


def test_parkinson_volatility_zero_range_yields_zero():
    # H == L every bar -> ln(H/L) = 0 -> Parkinson sigma is zero.
    h = np.full(30, 10.0)
    l = np.full(30, 10.0)
    out = _to_np(ta.ParkinsonVolatility(14, 252).batch(h, l))
    ready = out[~np.isnan(out)]
    assert ready.size > 0
    np.testing.assert_allclose(ready, 0.0, atol=1e-12)


def test_garman_klass_zero_movement_yields_zero():
    # O == H == L == C every bar -> both log terms are zero -> sigma is zero.
    o = np.full(30, 10.0)
    h = np.full(30, 10.0)
    l = np.full(30, 10.0)
    c = np.full(30, 10.0)
    out = _to_np(ta.GarmanKlassVolatility(14, 252).batch(o, h, l, c))
    ready = out[~np.isnan(out)]
    assert ready.size > 0
    np.testing.assert_allclose(ready, 0.0, atol=1e-12)


def test_rogers_satchell_zero_movement_yields_zero():
    o = np.full(30, 10.0)
    h = np.full(30, 10.0)
    l = np.full(30, 10.0)
    c = np.full(30, 10.0)
    out = _to_np(ta.RogersSatchellVolatility(14, 252).batch(o, h, l, c))
    ready = out[~np.isnan(out)]
    assert ready.size > 0
    np.testing.assert_allclose(ready, 0.0, atol=1e-12)


def test_yang_zhang_zero_movement_yields_zero():
    # O == H == L == C and constant across bars -> every sub-component is
    # zero -> Yang-Zhang sigma is zero.
    o = np.full(30, 10.0)
    h = np.full(30, 10.0)
    l = np.full(30, 10.0)
    c = np.full(30, 10.0)
    out = _to_np(ta.YangZhangVolatility(14, 252).batch(o, h, l, c))
    ready = out[~np.isnan(out)]
    assert ready.size > 0
    np.testing.assert_allclose(ready, 0.0, atol=1e-12)


def test_doji_default_is_directionless_flag():
    # Default Doji is a direction-less detection flag: +1 on a doji, 0 else.
    d = ta.Doji()
    assert d.is_signed() is False
    # body 0, range 2 -> doji.
    assert d.update((10.0, 11.0, 9.0, 10.0, 1.0, 0)) == pytest.approx(1.0)
    # body 2 == range -> not a doji.
    assert d.update((10.0, 12.0, 10.0, 12.0, 1.0, 1)) == pytest.approx(0.0)


def test_doji_signed_dragonfly_gravestone_neutral():
    # Signed Doji classifies by body position within the range.
    d = ta.Doji(signed=True)
    assert d.is_signed() is True
    # Dragonfly: body at the top, long lower shadow -> bullish +1.
    assert d.update((10.0, 10.05, 6.0, 10.0, 1.0, 0)) == pytest.approx(1.0)
    # Gravestone: body at the bottom, long upper shadow -> bearish -1.
    assert d.update((10.0, 14.0, 9.95, 10.0, 1.0, 1)) == pytest.approx(-1.0)
    # Long-legged: body centred, symmetric shadows -> neutral 0.
    assert d.update((10.0, 12.0, 8.0, 10.0, 1.0, 2)) == pytest.approx(0.0)
    # A large body is not a doji at all -> 0 regardless of position.
    assert d.update((10.0, 12.0, 10.0, 12.0, 1.0, 3)) == pytest.approx(0.0)


def test_orderbook_imbalance_reference_values():
    # Top-1: (3 - 1) / (3 + 1) = 0.5.
    assert ta.OrderBookImbalanceTop1().update([100.0], [3.0], [101.0], [1.0]) == pytest.approx(0.5)
    # Top-2: bidDepth 3, askDepth 2 -> (3 - 2) / 5 = 0.2.
    topn = ta.OrderBookImbalanceTopN(2)
    assert topn.update([100.0, 99.0], [2.0, 1.0], [101.0, 102.0], [1.0, 1.0]) == pytest.approx(0.2)
    # Full: bidDepth 1, askDepth 3 -> (1 - 3) / 4 = -0.5.
    full = ta.OrderBookImbalanceFull()
    assert full.update([100.0], [1.0], [101.0, 102.0], [2.0, 1.0]) == pytest.approx(-0.5)


def test_microprice_reference_value():
    # (100*3 + 101*1) / (1 + 3) = 401 / 4 = 100.25 — heavy ask pulls toward bid.
    mp = ta.Microprice()
    assert mp.update([100.0], [1.0], [101.0], [3.0]) == pytest.approx(100.25)


def test_quoted_spread_reference_value():
    # spread 1.0, mid 100.5 -> 1 / 100.5 * 10_000 ≈ 99.5025 bps.
    qs = ta.QuotedSpread()
    assert qs.update([100.0], [1.0], [101.0], [1.0]) == pytest.approx(99.50248756, abs=1e-6)


def test_depth_slope_reference_value():
    # Symmetric book, each side distances 1, 2 with cumulative sizes 1, 3.
    # OLS slope of (1->1, 2->3) = 2; mean of two equal sides = 2.
    ds = ta.DepthSlope()
    out = ds.update([99.0, 98.0], [1.0, 2.0], [101.0, 102.0], [1.0, 2.0])
    assert out == pytest.approx(2.0, abs=1e-9)
    # A book with a single level per side has no slope -> 0.
    assert ta.DepthSlope().update([100.0], [1.0], [101.0], [1.0]) == pytest.approx(0.0)


def test_footprint_buckets_buy_and_sell_volume():
    fp = ta.Footprint(1.0)
    fp.update(100.2, 2.0, True)  # bucket 100 -> ask 2
    fp.update(100.7, 3.0, False)  # bucket 101 -> bid 3
    out = fp.update(100.1, 1.0, True)  # bucket 100 -> ask 3
    # Columns are [price, bid_vol, ask_vol], rows sorted ascending by price.
    assert out.shape == (2, 3)
    assert list(out[0]) == [100.0, 0.0, 3.0]
    assert list(out[1]) == [101.0, 3.0, 0.0]


def test_signed_volume_reference_values():
    assert ta.SignedVolume().update(100.0, 2.0, True) == pytest.approx(2.0)
    assert ta.SignedVolume().update(100.0, 3.0, False) == pytest.approx(-3.0)


def test_cumulative_volume_delta_reference_values():
    cvd = ta.CumulativeVolumeDelta()
    assert cvd.update(100.0, 5.0, True) == pytest.approx(5.0)
    assert cvd.update(100.0, 2.0, False) == pytest.approx(3.0)
    assert cvd.update(100.0, 4.0, False) == pytest.approx(-1.0)


def test_trade_imbalance_reference_value():
    ti = ta.TradeImbalance(2)
    assert ti.update(100.0, 3.0, True) is None  # warming up
    # Window full: buyVol 3, sellVol 1 -> (3 - 1) / 4 = 0.5.
    assert ti.update(100.0, 1.0, False) == pytest.approx(0.5)


def test_effective_spread_reference_values():
    # Buy at 100.05 vs mid 100.0: 2 * (100.05 - 100) / 100 * 10000 = 10 bps.
    assert ta.EffectiveSpread().update(100.05, 1.0, True, 100.0) == pytest.approx(10.0)
    # Sell at 99.95 vs mid 100.0: 2 * -1 * (99.95 - 100) / 100 * 10000 = 10 bps.
    assert ta.EffectiveSpread().update(99.95, 1.0, False, 100.0) == pytest.approx(10.0)
    # A buy filled below the mid is price improvement -> negative.
    assert ta.EffectiveSpread().update(99.95, 1.0, True, 100.0) < 0.0


def test_realized_spread_reference_value():
    rs = ta.RealizedSpread(1)
    assert rs.update(100.10, 1.0, True, 100.0) is None  # buffered
    # Resolved against mid 100.20 one trade later:
    # 2 * (+1) * (100.10 - 100.20) / 100.0 * 10000 = -20 bps (adverse selection).
    assert rs.update(99.90, 1.0, False, 100.20) == pytest.approx(-20.0)


def test_kyles_lambda_recovers_constant_impact():
    # Build a tape where each trade moves the mid by exactly 0.5 per unit of
    # signed volume -> the rolling OLS slope is 0.5.
    impact = 0.5
    mid = 100.0
    price, size, is_buy, mids = [], [], [], []
    for i in range(20):
        buy = i % 2 == 0
        sz = 1.0 + (i % 3)
        signed = sz if buy else -sz
        mid += impact * signed
        price.append(mid)
        size.append(sz)
        is_buy.append(buy)
        mids.append(mid)
    out = _to_np(ta.KylesLambda(6).batch(price, size, is_buy, mids))
    assert out[-1] == pytest.approx(0.5, abs=1e-9)


def test_funding_rate_reference_values():
    assert ta.FundingRate().update(0.0001) == pytest.approx(0.0001)
    assert ta.FundingRate().update(-0.0003) == pytest.approx(-0.0003)


def test_funding_rate_mean_reference_value():
    frm = ta.FundingRateMean(2)
    assert frm.update(0.001) is None  # warming up
    # Window [0.001, 0.003] -> mean 0.002.
    assert frm.update(0.003) == pytest.approx(0.002)


def test_funding_rate_zscore_reference_value():
    z = ta.FundingRateZScore(2)
    assert z.update(0.001) is None  # warming up
    # Window [0.001, 0.003]: mean 0.002, population stddev 0.001 -> +1.
    assert z.update(0.003) == pytest.approx(1.0, abs=1e-9)


def test_funding_basis_reference_value():
    # mark 100.5 vs index 100.0 -> (100.5 - 100.0) / 100.0 = 0.005.
    assert ta.FundingBasis().update(100.5, 100.0) == pytest.approx(0.005)
    # A discount reads negative.
    assert ta.FundingBasis().update(99.5, 100.0) == pytest.approx(-0.005)


def test_open_interest_delta_reference_value():
    oid = ta.OpenInterestDelta()
    assert oid.update(1000.0) is None  # seeds the previous OI
    assert oid.update(1250.0) == pytest.approx(250.0)
    assert oid.update(1100.0) == pytest.approx(-150.0)


def test_oi_price_divergence_reference_value():
    div = ta.OIPriceDivergence(1)
    assert div.update(1000.0, 100.0) is None  # warming up
    # OI +10% while price flat -> divergence +0.1.
    assert div.update(1100.0, 100.0) == pytest.approx(0.1)


def test_oi_weighted_reference_value():
    oiw = ta.OIWeighted()
    assert oiw.update(100.0, 10.0) == pytest.approx(100.0)
    # (100·10 + 110·30) / 40 = 107.5.
    assert oiw.update(110.0, 30.0) == pytest.approx(107.5)


def test_long_short_ratio_reference_value():
    # 600 longs vs 400 shorts -> 1.5.
    assert ta.LongShortRatio().update(600.0, 400.0) == pytest.approx(1.5)
    # No short side -> 0.0.
    assert ta.LongShortRatio().update(600.0, 0.0) == pytest.approx(0.0)


def test_taker_buy_sell_ratio_reference_value():
    # 60 taker buys vs 40 taker sells -> 1.5.
    assert ta.TakerBuySellRatio().update(60.0, 40.0) == pytest.approx(1.5)
    # No taker sell volume -> 0.0.
    assert ta.TakerBuySellRatio().update(60.0, 0.0) == pytest.approx(0.0)


def test_liquidation_features_reference_value():
    # 30 long vs 10 short: (long, short, net, total, imbalance).
    out = ta.LiquidationFeatures().update(30.0, 10.0)
    assert out == pytest.approx((30.0, 10.0, 20.0, 40.0, 0.5))


def test_term_structure_basis_reference_value():
    # futures 102 vs index 100 -> 0.02 (contango).
    assert ta.TermStructureBasis().update(102.0, 100.0) == pytest.approx(0.02)
    # Backwardation reads negative.
    assert ta.TermStructureBasis().update(98.0, 100.0) == pytest.approx(-0.02)


def test_calendar_spread_reference_value():
    # futures 101 vs perpetual mark 100 -> 0.01.
    assert ta.CalendarSpread().update(101.0, 100.0) == pytest.approx(0.01)
    assert ta.CalendarSpread().update(99.0, 100.0) == pytest.approx(-0.01)
