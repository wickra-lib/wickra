"""Input-validation tests: malformed NumPy inputs raise ValueError, not panics."""

from __future__ import annotations

import numpy as np
import pytest

import wickra as ta


def test_non_contiguous_array_is_accepted():
    # Inputs are read through the buffer/sequence protocol, so a strided (non
    # C-contiguous) view is accepted and yields the same result as a dense copy.
    base = np.linspace(1.0, 100.0, 60)
    non_contiguous = base[::2]
    assert not non_contiguous.flags["C_CONTIGUOUS"]
    strided = np.asarray(ta.SMA(5).batch(non_contiguous))
    dense = np.asarray(ta.SMA(5).batch(np.ascontiguousarray(non_contiguous)))
    np.testing.assert_array_equal(strided, dense)


def test_dense_array_batches():
    base = np.linspace(1.0, 100.0, 60)
    fixed = np.ascontiguousarray(base[::2])
    out = ta.SMA(5).batch(fixed)
    assert len(out) == len(fixed)


def test_unequal_length_candle_batch_raises(ohlc_series):
    high, low, close = ohlc_series
    short = low[:-1]
    with pytest.raises(ValueError):
        ta.ATR(14).batch(high, short, close)
    with pytest.raises(ValueError):
        ta.WilliamsR(14).batch(high, short, close)
    with pytest.raises(ValueError):
        ta.Aroon(14).batch(high, short)


def test_pairwise_beta_rejects_bad_period():
    with pytest.raises(ValueError):
        ta.PairwiseBeta(0)
    with pytest.raises(ValueError):
        ta.PairwiseBeta(1)


def test_unequal_length_pair_batch_raises(sine_prices):
    a = np.ascontiguousarray((sine_prices + 100.0).astype(np.float64))
    b = a[:-1]
    with pytest.raises(ValueError):
        ta.PairwiseBeta(20).batch(a, b)
    with pytest.raises(ValueError):
        ta.PairSpreadZScore(20, 20).batch(a, b)


def test_pair_spread_zscore_rejects_bad_periods():
    with pytest.raises(ValueError):
        ta.PairSpreadZScore(1, 20)
    with pytest.raises(ValueError):
        ta.PairSpreadZScore(20, 1)


def test_lead_lag_rejects_bad_params():
    with pytest.raises(ValueError):
        ta.LeadLagCrossCorrelation(1, 5)
    with pytest.raises(ValueError):
        ta.LeadLagCrossCorrelation(10, 0)


def test_lead_lag_unequal_length_batch_raises(sine_prices):
    a = np.ascontiguousarray((sine_prices + 100.0).astype(np.float64))
    b = a[:-1]
    with pytest.raises(ValueError):
        ta.LeadLagCrossCorrelation(12, 5).batch(a, b)


def test_cointegration_rejects_too_small_period():
    # period must be >= 2*adf_lags + 4.
    with pytest.raises(ValueError):
        ta.Cointegration(3, 0)
    with pytest.raises(ValueError):
        ta.Cointegration(5, 1)


def test_cointegration_unequal_length_batch_raises(sine_prices):
    a = np.ascontiguousarray((sine_prices + 100.0).astype(np.float64))
    b = a[:-1]
    with pytest.raises(ValueError):
        ta.Cointegration(20, 1).batch(a, b)


def test_relative_strength_rejects_zero_periods():
    with pytest.raises(ValueError):
        ta.RelativeStrengthAB(0, 14)
    with pytest.raises(ValueError):
        ta.RelativeStrengthAB(20, 0)


def test_relative_strength_unequal_length_batch_raises(sine_prices):
    a = np.ascontiguousarray((sine_prices + 100.0).astype(np.float64))
    b = a[:-1]
    with pytest.raises(ValueError):
        ta.RelativeStrengthAB(10, 14).batch(a, b)


def test_roc_and_trix_have_default_periods():
    # ROC/TRIX gained constructor defaults matching the TA-Lib convention.
    assert ta.ROC().period == 10
    assert ta.TRIX() is not None


def test_value_area_rejects_zero_period():
    with pytest.raises(ValueError):
        ta.ValueArea(0, 50, 0.7)
    with pytest.raises(ValueError):
        ta.ValueArea(20, 0, 0.7)


def test_value_area_rejects_invalid_pct():
    with pytest.raises(ValueError):
        ta.ValueArea(20, 50, 0.0)
    with pytest.raises(ValueError):
        ta.ValueArea(20, 50, 1.5)


def test_initial_balance_rejects_zero_period():
    with pytest.raises(ValueError):
        ta.InitialBalance(0)


def test_opening_range_rejects_zero_period():
    with pytest.raises(ValueError):
        ta.OpeningRange(0)


def test_value_area_unequal_length_raises():
    high = np.array([1.0, 2.0, 3.0])
    low = np.array([0.5, 1.5])
    volume = np.array([10.0, 10.0, 10.0])
    with pytest.raises(ValueError):
        ta.ValueArea(2, 10, 0.7).batch(high, low, volume)


def test_ichimoku_rejects_zero_and_non_increasing_periods():
    with pytest.raises(ValueError):
        ta.Ichimoku(0, 26, 52, 26)
    with pytest.raises(ValueError):
        ta.Ichimoku(9, 26, 52, 0)
    # Periods must satisfy tenkan < kijun < senkou_b.
    with pytest.raises(ValueError):
        ta.Ichimoku(26, 9, 52, 26)
    with pytest.raises(ValueError):
        ta.Ichimoku(9, 52, 52, 26)


def test_family_10_ehlers_rejects_invalid_parameters():
    with pytest.raises(ValueError):
        ta.SuperSmoother(0)
    with pytest.raises(ValueError):
        ta.FisherTransform(0)
    with pytest.raises(ValueError):
        ta.InverseFisherTransform(0.0)
    with pytest.raises(ValueError):
        ta.DecyclerOscillator(30, 10)
    with pytest.raises(ValueError):
        ta.RoofingFilter(48, 10)
    with pytest.raises(ValueError):
        ta.MAMA(0.05, 0.5)
    with pytest.raises(ValueError):
        ta.EmpiricalModeDecomposition(20, 0.0)
    # The VECM behind the information share needs at least six observations.
    with pytest.raises(ValueError):
        ta.HasbrouckInformationShare(5)


def test_orderbook_topn_zero_levels_raises():
    with pytest.raises(ValueError):
        ta.OrderBookImbalanceTopN(0)


def test_orderbook_unequal_price_size_lengths_raise():
    # bid_px has 2 entries but bid_sz has 1 -> mismatched -> ValueError.
    with pytest.raises(ValueError):
        ta.OrderBookImbalanceTop1().update([100.0, 99.0], [1.0], [101.0], [1.0])
    with pytest.raises(ValueError):
        ta.Microprice().update([100.0], [1.0], [101.0, 102.0], [1.0])


def test_orderbook_crossed_book_raises():
    # best_bid (102) >= best_ask (101) is a crossed book -> rejected.
    with pytest.raises(ValueError):
        ta.QuotedSpread().update([102.0], [1.0], [101.0], [1.0])


def test_orderbook_misordered_levels_raise():
    # Bids must be strictly descending in price.
    with pytest.raises(ValueError):
        ta.OrderBookImbalanceFull().update([99.0, 100.0], [1.0, 1.0], [101.0], [1.0])


def test_trade_imbalance_zero_window_raises():
    with pytest.raises(ValueError):
        ta.TradeImbalance(0)


def test_trade_negative_size_raises():
    with pytest.raises(ValueError):
        ta.SignedVolume().update(100.0, -1.0, True)


def test_trade_non_positive_price_raises():
    with pytest.raises(ValueError):
        ta.CumulativeVolumeDelta().update(0.0, 1.0, True)


def test_trade_batch_unequal_lengths_raise():
    with pytest.raises(ValueError):
        ta.SignedVolume().batch([100.0, 100.0], [1.0], [True, False])


def test_effective_spread_non_positive_mid_raises():
    with pytest.raises(ValueError):
        ta.EffectiveSpread().update(100.0, 1.0, True, 0.0)


def test_effective_spread_batch_unequal_lengths_raise():
    with pytest.raises(ValueError):
        ta.EffectiveSpread().batch([100.0, 100.0], [1.0, 1.0], [True, False], [100.0])


def test_realized_spread_zero_horizon_raises():
    with pytest.raises(ValueError):
        ta.RealizedSpread(0)


def test_kyles_lambda_window_below_two_raises():
    with pytest.raises(ValueError):
        ta.KylesLambda(1)


def test_footprint_non_positive_tick_raises():
    with pytest.raises(ValueError):
        ta.Footprint(0.0)
    with pytest.raises(ValueError):
        ta.Footprint(-1.0)


def test_funding_rate_mean_zero_window_raises():
    with pytest.raises(ValueError):
        ta.FundingRateMean(0)


def test_funding_rate_zscore_zero_window_raises():
    with pytest.raises(ValueError):
        ta.FundingRateZScore(0)


def test_funding_basis_non_positive_index_raises():
    with pytest.raises(ValueError):
        ta.FundingBasis().update(100.0, 0.0)


def test_funding_rate_non_finite_raises():
    with pytest.raises(ValueError):
        ta.FundingRate().update(float("nan"))


def test_oi_price_divergence_zero_window_raises():
    with pytest.raises(ValueError):
        ta.OIPriceDivergence(0)


def test_oi_weighted_non_positive_mark_raises():
    with pytest.raises(ValueError):
        ta.OIWeighted().update(0.0, 100.0)


def test_term_structure_basis_non_positive_index_raises():
    with pytest.raises(ValueError):
        ta.TermStructureBasis().update(100.0, 0.0)


def test_calendar_spread_non_positive_mark_raises():
    with pytest.raises(ValueError):
        ta.CalendarSpread().update(100.0, 0.0)
