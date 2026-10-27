# Generated from bindings/c/include/wickra.h. DO NOT EDIT.
#
# Roxygen for the data-layer and Binance entry points lives in
# R_DOC_OVERRIDES in gen_r.py, not here -- editing this file loses it
# on the next regeneration.

# Internal: build an S3 wickra_indicator object holding the external
# pointer (handle, auto-freed by a C finalizer) and the C-symbol prefix.
.wk_obj <- function(prefix, ptr, cls, values_cap = NA_integer_) {
  structure(list(ptr = ptr, prefix = prefix, values_cap = values_cap),
            class = c(cls, "wickra_indicator"))
}

#' AbandonedBaby indicator
#' @keywords internal
#' @export
AbandonedBaby <- function() {
  ptr <- .Call("wk_abandoned_baby_new", PACKAGE = "wickra")
  .wk_obj("abandoned_baby", ptr, "AbandonedBaby")
}

#' Abcd indicator
#' @keywords internal
#' @export
Abcd <- function() {
  ptr <- .Call("wk_abcd_new", PACKAGE = "wickra")
  .wk_obj("abcd", ptr, "Abcd")
}

#' AbsoluteBreadthIndex indicator
#' @keywords internal
#' @export
AbsoluteBreadthIndex <- function() {
  ptr <- .Call("wk_absolute_breadth_index_new", PACKAGE = "wickra")
  .wk_obj("absolute_breadth_index", ptr, "AbsoluteBreadthIndex")
}

#' AccelerationBands indicator
#' @keywords internal
#' @export
AccelerationBands <- function(period, factor) {
  ptr <- .Call("wk_acceleration_bands_new", period, factor, PACKAGE = "wickra")
  .wk_obj("acceleration_bands", ptr, "AccelerationBands")
}

#' AcceleratorOscillator indicator
#' @keywords internal
#' @export
AcceleratorOscillator <- function(ao_fast, ao_slow, signal_period) {
  ptr <- .Call("wk_accelerator_oscillator_new", ao_fast, ao_slow, signal_period, PACKAGE = "wickra")
  .wk_obj("accelerator_oscillator", ptr, "AcceleratorOscillator")
}

#' AdOscillator indicator
#' @keywords internal
#' @export
AdOscillator <- function() {
  ptr <- .Call("wk_ad_oscillator_new", PACKAGE = "wickra")
  .wk_obj("ad_oscillator", ptr, "AdOscillator")
}

#' AdVolumeLine indicator
#' @keywords internal
#' @export
AdVolumeLine <- function() {
  ptr <- .Call("wk_ad_volume_line_new", PACKAGE = "wickra")
  .wk_obj("ad_volume_line", ptr, "AdVolumeLine")
}

#' AdaptiveCci indicator
#' @keywords internal
#' @export
AdaptiveCci <- function(period) {
  ptr <- .Call("wk_adaptive_cci_new", period, PACKAGE = "wickra")
  .wk_obj("adaptive_cci", ptr, "AdaptiveCci")
}

#' AdaptiveCycle indicator
#' @keywords internal
#' @export
AdaptiveCycle <- function() {
  ptr <- .Call("wk_adaptive_cycle_new", PACKAGE = "wickra")
  .wk_obj("adaptive_cycle", ptr, "AdaptiveCycle")
}

#' AdaptiveLaguerreFilter indicator
#' @keywords internal
#' @export
AdaptiveLaguerreFilter <- function(period) {
  ptr <- .Call("wk_adaptive_laguerre_filter_new", period, PACKAGE = "wickra")
  .wk_obj("adaptive_laguerre_filter", ptr, "AdaptiveLaguerreFilter")
}

#' AdaptiveRsi indicator
#' @keywords internal
#' @export
AdaptiveRsi <- function(period) {
  ptr <- .Call("wk_adaptive_rsi_new", period, PACKAGE = "wickra")
  .wk_obj("adaptive_rsi", ptr, "AdaptiveRsi")
}

#' Adl indicator
#' @keywords internal
#' @export
Adl <- function() {
  ptr <- .Call("wk_adl_new", PACKAGE = "wickra")
  .wk_obj("adl", ptr, "Adl")
}

#' AdvanceBlock indicator
#' @keywords internal
#' @export
AdvanceBlock <- function() {
  ptr <- .Call("wk_advance_block_new", PACKAGE = "wickra")
  .wk_obj("advance_block", ptr, "AdvanceBlock")
}

#' AdvanceDecline indicator
#' @keywords internal
#' @export
AdvanceDecline <- function() {
  ptr <- .Call("wk_advance_decline_new", PACKAGE = "wickra")
  .wk_obj("advance_decline", ptr, "AdvanceDecline")
}

#' AdvanceDeclineRatio indicator
#' @keywords internal
#' @export
AdvanceDeclineRatio <- function() {
  ptr <- .Call("wk_advance_decline_ratio_new", PACKAGE = "wickra")
  .wk_obj("advance_decline_ratio", ptr, "AdvanceDeclineRatio")
}

#' Adx indicator
#' @keywords internal
#' @export
Adx <- function(period) {
  ptr <- .Call("wk_adx_new", period, PACKAGE = "wickra")
  .wk_obj("adx", ptr, "Adx")
}

#' Adxr indicator
#' @keywords internal
#' @export
Adxr <- function(period) {
  ptr <- .Call("wk_adxr_new", period, PACKAGE = "wickra")
  .wk_obj("adxr", ptr, "Adxr")
}

#' Alligator indicator
#' @keywords internal
#' @export
Alligator <- function(jaw_period, teeth_period, lips_period) {
  ptr <- .Call("wk_alligator_new", jaw_period, teeth_period, lips_period, PACKAGE = "wickra")
  .wk_obj("alligator", ptr, "Alligator")
}

#' Alma indicator
#' @keywords internal
#' @export
Alma <- function(period, offset, sigma) {
  ptr <- .Call("wk_alma_new", period, offset, sigma, PACKAGE = "wickra")
  .wk_obj("alma", ptr, "Alma")
}

#' Alpha indicator
#' @keywords internal
#' @export
Alpha <- function(period, risk_free) {
  ptr <- .Call("wk_alpha_new", period, risk_free, PACKAGE = "wickra")
  .wk_obj("alpha", ptr, "Alpha")
}

#' AmihudIlliquidity indicator
#' @keywords internal
#' @export
AmihudIlliquidity <- function(period) {
  ptr <- .Call("wk_amihud_illiquidity_new", period, PACKAGE = "wickra")
  .wk_obj("amihud_illiquidity", ptr, "AmihudIlliquidity")
}

#' AnchoredRsi indicator
#' @keywords internal
#' @export
AnchoredRsi <- function() {
  ptr <- .Call("wk_anchored_rsi_new", PACKAGE = "wickra")
  .wk_obj("anchored_rsi", ptr, "AnchoredRsi")
}

#' AnchoredVwap indicator
#' @keywords internal
#' @export
AnchoredVwap <- function() {
  ptr <- .Call("wk_anchored_vwap_new", PACKAGE = "wickra")
  .wk_obj("anchored_vwap", ptr, "AnchoredVwap")
}

#' AndrewsPitchfork indicator
#' @keywords internal
#' @export
AndrewsPitchfork <- function(strength) {
  ptr <- .Call("wk_andrews_pitchfork_new", strength, PACKAGE = "wickra")
  .wk_obj("andrews_pitchfork", ptr, "AndrewsPitchfork")
}

#' Apo indicator
#' @keywords internal
#' @export
Apo <- function(fast, slow) {
  ptr <- .Call("wk_apo_new", fast, slow, PACKAGE = "wickra")
  .wk_obj("apo", ptr, "Apo")
}

#' Aroon indicator
#' @keywords internal
#' @export
Aroon <- function(period) {
  ptr <- .Call("wk_aroon_new", period, PACKAGE = "wickra")
  .wk_obj("aroon", ptr, "Aroon")
}

#' AroonOscillator indicator
#' @keywords internal
#' @export
AroonOscillator <- function(period) {
  ptr <- .Call("wk_aroon_oscillator_new", period, PACKAGE = "wickra")
  .wk_obj("aroon_oscillator", ptr, "AroonOscillator")
}

#' Atr indicator
#' @keywords internal
#' @export
Atr <- function(period) {
  ptr <- .Call("wk_atr_new", period, PACKAGE = "wickra")
  .wk_obj("atr", ptr, "Atr")
}

#' AtrBands indicator
#' @keywords internal
#' @export
AtrBands <- function(period, multiplier) {
  ptr <- .Call("wk_atr_bands_new", period, multiplier, PACKAGE = "wickra")
  .wk_obj("atr_bands", ptr, "AtrBands")
}

#' AtrRatchet indicator
#' @keywords internal
#' @export
AtrRatchet <- function(atr_period, start_mult, increment) {
  ptr <- .Call("wk_atr_ratchet_new", atr_period, start_mult, increment, PACKAGE = "wickra")
  .wk_obj("atr_ratchet", ptr, "AtrRatchet")
}

#' AtrTrailingStop indicator
#' @keywords internal
#' @export
AtrTrailingStop <- function(atr_period, multiplier) {
  ptr <- .Call("wk_atr_trailing_stop_new", atr_period, multiplier, PACKAGE = "wickra")
  .wk_obj("atr_trailing_stop", ptr, "AtrTrailingStop")
}

#' AutoFib indicator
#' @keywords internal
#' @export
AutoFib <- function() {
  ptr <- .Call("wk_auto_fib_new", PACKAGE = "wickra")
  .wk_obj("auto_fib", ptr, "AutoFib")
}

#' Autocorrelation indicator
#' @keywords internal
#' @export
Autocorrelation <- function(period, lag) {
  ptr <- .Call("wk_autocorrelation_new", period, lag, PACKAGE = "wickra")
  .wk_obj("autocorrelation", ptr, "Autocorrelation")
}

#' AutocorrelationPeriodogram indicator
#' @keywords internal
#' @export
AutocorrelationPeriodogram <- function(min_period, max_period) {
  ptr <- .Call("wk_autocorrelation_periodogram_new", min_period, max_period, PACKAGE = "wickra")
  .wk_obj("autocorrelation_periodogram", ptr, "AutocorrelationPeriodogram")
}

#' AverageDailyRange indicator
#' @keywords internal
#' @export
AverageDailyRange <- function(period, utc_offset_minutes) {
  ptr <- .Call("wk_average_daily_range_new", period, utc_offset_minutes, PACKAGE = "wickra")
  .wk_obj("average_daily_range", ptr, "AverageDailyRange")
}

#' AverageDrawdown indicator
#' @keywords internal
#' @export
AverageDrawdown <- function(period) {
  ptr <- .Call("wk_average_drawdown_new", period, PACKAGE = "wickra")
  .wk_obj("average_drawdown", ptr, "AverageDrawdown")
}

#' AvgPrice indicator
#' @keywords internal
#' @export
AvgPrice <- function() {
  ptr <- .Call("wk_avg_price_new", PACKAGE = "wickra")
  .wk_obj("avg_price", ptr, "AvgPrice")
}

#' AwesomeOscillator indicator
#' @keywords internal
#' @export
AwesomeOscillator <- function(fast, slow) {
  ptr <- .Call("wk_awesome_oscillator_new", fast, slow, PACKAGE = "wickra")
  .wk_obj("awesome_oscillator", ptr, "AwesomeOscillator")
}

#' AwesomeOscillatorHistogram indicator
#' @keywords internal
#' @export
AwesomeOscillatorHistogram <- function(fast, slow, lookback) {
  ptr <- .Call("wk_awesome_oscillator_histogram_new", fast, slow, lookback, PACKAGE = "wickra")
  .wk_obj("awesome_oscillator_histogram", ptr, "AwesomeOscillatorHistogram")
}

#' BalanceOfPower indicator
#' @keywords internal
#' @export
BalanceOfPower <- function() {
  ptr <- .Call("wk_balance_of_power_new", PACKAGE = "wickra")
  .wk_obj("balance_of_power", ptr, "BalanceOfPower")
}

#' BandpassFilter indicator
#' @keywords internal
#' @export
BandpassFilter <- function(period, bandwidth) {
  ptr <- .Call("wk_bandpass_filter_new", period, bandwidth, PACKAGE = "wickra")
  .wk_obj("bandpass_filter", ptr, "BandpassFilter")
}

#' Bat indicator
#' @keywords internal
#' @export
Bat <- function() {
  ptr <- .Call("wk_bat_new", PACKAGE = "wickra")
  .wk_obj("bat", ptr, "Bat")
}

#' BeltHold indicator
#' @keywords internal
#' @export
BeltHold <- function() {
  ptr <- .Call("wk_belt_hold_new", PACKAGE = "wickra")
  .wk_obj("belt_hold", ptr, "BeltHold")
}

#' Beta indicator
#' @keywords internal
#' @export
Beta <- function(period) {
  ptr <- .Call("wk_beta_new", period, PACKAGE = "wickra")
  .wk_obj("beta", ptr, "Beta")
}

#' BetaNeutralSpread indicator
#' @keywords internal
#' @export
BetaNeutralSpread <- function(period) {
  ptr <- .Call("wk_beta_neutral_spread_new", period, PACKAGE = "wickra")
  .wk_obj("beta_neutral_spread", ptr, "BetaNeutralSpread")
}

#' BetterVolume indicator
#' @keywords internal
#' @export
BetterVolume <- function(period) {
  ptr <- .Call("wk_better_volume_new", period, PACKAGE = "wickra")
  .wk_obj("better_volume", ptr, "BetterVolume")
}

#' BipowerVariation indicator
#' @keywords internal
#' @export
BipowerVariation <- function(period) {
  ptr <- .Call("wk_bipower_variation_new", period, PACKAGE = "wickra")
  .wk_obj("bipower_variation", ptr, "BipowerVariation")
}

#' BodySizePct indicator
#' @keywords internal
#' @export
BodySizePct <- function() {
  ptr <- .Call("wk_body_size_pct_new", PACKAGE = "wickra")
  .wk_obj("body_size_pct", ptr, "BodySizePct")
}

#' BollingerBands indicator
#' @keywords internal
#' @export
BollingerBands <- function(period, multiplier) {
  ptr <- .Call("wk_bollinger_bands_new", period, multiplier, PACKAGE = "wickra")
  .wk_obj("bollinger_bands", ptr, "BollingerBands")
}

#' BollingerBandwidth indicator
#' @keywords internal
#' @export
BollingerBandwidth <- function(period, multiplier) {
  ptr <- .Call("wk_bollinger_bandwidth_new", period, multiplier, PACKAGE = "wickra")
  .wk_obj("bollinger_bandwidth", ptr, "BollingerBandwidth")
}

#' BomarBands indicator
#' @keywords internal
#' @export
BomarBands <- function(period, coverage) {
  ptr <- .Call("wk_bomar_bands_new", period, coverage, PACKAGE = "wickra")
  .wk_obj("bomar_bands", ptr, "BomarBands")
}

#' BreadthThrust indicator
#' @keywords internal
#' @export
BreadthThrust <- function(period) {
  ptr <- .Call("wk_breadth_thrust_new", period, PACKAGE = "wickra")
  .wk_obj("breadth_thrust", ptr, "BreadthThrust")
}

#' Breakaway indicator
#' @keywords internal
#' @export
Breakaway <- function() {
  ptr <- .Call("wk_breakaway_new", PACKAGE = "wickra")
  .wk_obj("breakaway", ptr, "Breakaway")
}

#' BullishPercentIndex indicator
#' @keywords internal
#' @export
BullishPercentIndex <- function() {
  ptr <- .Call("wk_bullish_percent_index_new", PACKAGE = "wickra")
  .wk_obj("bullish_percent_index", ptr, "BullishPercentIndex")
}

#' BurkeRatio indicator
#' @keywords internal
#' @export
BurkeRatio <- function(period) {
  ptr <- .Call("wk_burke_ratio_new", period, PACKAGE = "wickra")
  .wk_obj("burke_ratio", ptr, "BurkeRatio")
}

#' Butterfly indicator
#' @keywords internal
#' @export
Butterfly <- function() {
  ptr <- .Call("wk_butterfly_new", PACKAGE = "wickra")
  .wk_obj("butterfly", ptr, "Butterfly")
}

#' CalendarSpread indicator
#' @keywords internal
#' @export
CalendarSpread <- function() {
  ptr <- .Call("wk_calendar_spread_new", PACKAGE = "wickra")
  .wk_obj("calendar_spread", ptr, "CalendarSpread")
}

#' CalmarRatio indicator
#' @keywords internal
#' @export
CalmarRatio <- function(period) {
  ptr <- .Call("wk_calmar_ratio_new", period, PACKAGE = "wickra")
  .wk_obj("calmar_ratio", ptr, "CalmarRatio")
}

#' Camarilla indicator
#' @keywords internal
#' @export
Camarilla <- function() {
  ptr <- .Call("wk_camarilla_new", PACKAGE = "wickra")
  .wk_obj("camarilla", ptr, "Camarilla")
}

#' Parse OHLCV candles from a CSV string
#'
#' Builds a reader over an in-memory CSV string with a header row and OHLCV
#' columns. Pass the result to [read()] to get every parsed candle.
#'
#' @param csv A length-one character string of CSV text (a header row followed
#'   by OHLCV data rows).
#' @return A `wickra_indicator` candle reader; read its candles with [read()].
#' @keywords internal
#' @export
CandleReader <- function(csv) {
  ptr <- .Call("wk_candle_reader_new", csv, PACKAGE = "wickra")
  .wk_obj("candle_reader", ptr, "CandleReader")
}

#' CandleVolume indicator
#' @keywords internal
#' @export
CandleVolume <- function(period) {
  ptr <- .Call("wk_candle_volume_new", period, PACKAGE = "wickra")
  .wk_obj("candle_volume", ptr, "CandleVolume")
}

#' Cci indicator
#' @keywords internal
#' @export
Cci <- function(period) {
  ptr <- .Call("wk_cci_new", period, PACKAGE = "wickra")
  .wk_obj("cci", ptr, "Cci")
}

#' CenterOfGravity indicator
#' @keywords internal
#' @export
CenterOfGravity <- function(period) {
  ptr <- .Call("wk_center_of_gravity_new", period, PACKAGE = "wickra")
  .wk_obj("center_of_gravity", ptr, "CenterOfGravity")
}

#' CentralPivotRange indicator
#' @keywords internal
#' @export
CentralPivotRange <- function() {
  ptr <- .Call("wk_central_pivot_range_new", PACKAGE = "wickra")
  .wk_obj("central_pivot_range", ptr, "CentralPivotRange")
}

#' Cfo indicator
#' @keywords internal
#' @export
Cfo <- function(period) {
  ptr <- .Call("wk_cfo_new", period, PACKAGE = "wickra")
  .wk_obj("cfo", ptr, "Cfo")
}

#' ChaikinMoneyFlow indicator
#' @keywords internal
#' @export
ChaikinMoneyFlow <- function(period) {
  ptr <- .Call("wk_chaikin_money_flow_new", period, PACKAGE = "wickra")
  .wk_obj("chaikin_money_flow", ptr, "ChaikinMoneyFlow")
}

#' ChaikinOscillator indicator
#' @keywords internal
#' @export
ChaikinOscillator <- function(fast, slow) {
  ptr <- .Call("wk_chaikin_oscillator_new", fast, slow, PACKAGE = "wickra")
  .wk_obj("chaikin_oscillator", ptr, "ChaikinOscillator")
}

#' ChaikinVolatility indicator
#' @keywords internal
#' @export
ChaikinVolatility <- function(ema_period, roc_period) {
  ptr <- .Call("wk_chaikin_volatility_new", ema_period, roc_period, PACKAGE = "wickra")
  .wk_obj("chaikin_volatility", ptr, "ChaikinVolatility")
}

#' ChandeKrollStop indicator
#' @keywords internal
#' @export
ChandeKrollStop <- function(atr_period, atr_multiplier, stop_period) {
  ptr <- .Call("wk_chande_kroll_stop_new", atr_period, atr_multiplier, stop_period, PACKAGE = "wickra")
  .wk_obj("chande_kroll_stop", ptr, "ChandeKrollStop")
}

#' ChandelierExit indicator
#' @keywords internal
#' @export
ChandelierExit <- function(period, multiplier) {
  ptr <- .Call("wk_chandelier_exit_new", period, multiplier, PACKAGE = "wickra")
  .wk_obj("chandelier_exit", ptr, "ChandelierExit")
}

#' ChoppinessIndex indicator
#' @keywords internal
#' @export
ChoppinessIndex <- function(period) {
  ptr <- .Call("wk_choppiness_index_new", period, PACKAGE = "wickra")
  .wk_obj("choppiness_index", ptr, "ChoppinessIndex")
}

#' ClassicPivots indicator
#' @keywords internal
#' @export
ClassicPivots <- function() {
  ptr <- .Call("wk_classic_pivots_new", PACKAGE = "wickra")
  .wk_obj("classic_pivots", ptr, "ClassicPivots")
}

#' CloseVsOpen indicator
#' @keywords internal
#' @export
CloseVsOpen <- function() {
  ptr <- .Call("wk_close_vs_open_new", PACKAGE = "wickra")
  .wk_obj("close_vs_open", ptr, "CloseVsOpen")
}

#' ClosingMarubozu indicator
#' @keywords internal
#' @export
ClosingMarubozu <- function() {
  ptr <- .Call("wk_closing_marubozu_new", PACKAGE = "wickra")
  .wk_obj("closing_marubozu", ptr, "ClosingMarubozu")
}

#' Cmo indicator
#' @keywords internal
#' @export
Cmo <- function(period) {
  ptr <- .Call("wk_cmo_new", period, PACKAGE = "wickra")
  .wk_obj("cmo", ptr, "Cmo")
}

#' CoefficientOfVariation indicator
#' @keywords internal
#' @export
CoefficientOfVariation <- function(period) {
  ptr <- .Call("wk_coefficient_of_variation_new", period, PACKAGE = "wickra")
  .wk_obj("coefficient_of_variation", ptr, "CoefficientOfVariation")
}

#' Cointegration indicator
#' @keywords internal
#' @export
Cointegration <- function(period, adf_lags) {
  ptr <- .Call("wk_cointegration_new", period, adf_lags, PACKAGE = "wickra")
  .wk_obj("cointegration", ptr, "Cointegration")
}

#' CommonSenseRatio indicator
#' @keywords internal
#' @export
CommonSenseRatio <- function(period) {
  ptr <- .Call("wk_common_sense_ratio_new", period, PACKAGE = "wickra")
  .wk_obj("common_sense_ratio", ptr, "CommonSenseRatio")
}

#' CompositeProfile indicator
#' @keywords internal
#' @export
CompositeProfile <- function(period, bins, value_area_pct) {
  ptr <- .Call("wk_composite_profile_new", period, bins, value_area_pct, PACKAGE = "wickra")
  .wk_obj("composite_profile", ptr, "CompositeProfile")
}

#' ConcealingBabySwallow indicator
#' @keywords internal
#' @export
ConcealingBabySwallow <- function() {
  ptr <- .Call("wk_concealing_baby_swallow_new", PACKAGE = "wickra")
  .wk_obj("concealing_baby_swallow", ptr, "ConcealingBabySwallow")
}

#' ConditionalValueAtRisk indicator
#' @keywords internal
#' @export
ConditionalValueAtRisk <- function(period, confidence) {
  ptr <- .Call("wk_conditional_value_at_risk_new", period, confidence, PACKAGE = "wickra")
  .wk_obj("conditional_value_at_risk", ptr, "ConditionalValueAtRisk")
}

#' ConnorsRsi indicator
#' @keywords internal
#' @export
ConnorsRsi <- function(period_rsi, period_streak, period_rank) {
  ptr <- .Call("wk_connors_rsi_new", period_rsi, period_streak, period_rank, PACKAGE = "wickra")
  .wk_obj("connors_rsi", ptr, "ConnorsRsi")
}

#' Coppock indicator
#' @keywords internal
#' @export
Coppock <- function(roc_long_period, roc_short_period, wma_period) {
  ptr <- .Call("wk_coppock_new", roc_long_period, roc_short_period, wma_period, PACKAGE = "wickra")
  .wk_obj("coppock", ptr, "Coppock")
}

#' CorrelationTrendIndicator indicator
#' @keywords internal
#' @export
CorrelationTrendIndicator <- function(period) {
  ptr <- .Call("wk_correlation_trend_indicator_new", period, PACKAGE = "wickra")
  .wk_obj("correlation_trend_indicator", ptr, "CorrelationTrendIndicator")
}

#' Counterattack indicator
#' @keywords internal
#' @export
Counterattack <- function() {
  ptr <- .Call("wk_counterattack_new", PACKAGE = "wickra")
  .wk_obj("counterattack", ptr, "Counterattack")
}

#' Crab indicator
#' @keywords internal
#' @export
Crab <- function() {
  ptr <- .Call("wk_crab_new", PACKAGE = "wickra")
  .wk_obj("crab", ptr, "Crab")
}

#' CumulativeVolumeDelta indicator
#' @keywords internal
#' @export
CumulativeVolumeDelta <- function() {
  ptr <- .Call("wk_cumulative_volume_delta_new", PACKAGE = "wickra")
  .wk_obj("cumulative_volume_delta", ptr, "CumulativeVolumeDelta")
}

#' CumulativeVolumeIndex indicator
#' @keywords internal
#' @export
CumulativeVolumeIndex <- function() {
  ptr <- .Call("wk_cumulative_volume_index_new", PACKAGE = "wickra")
  .wk_obj("cumulative_volume_index", ptr, "CumulativeVolumeIndex")
}

#' CupAndHandle indicator
#' @keywords internal
#' @export
CupAndHandle <- function() {
  ptr <- .Call("wk_cup_and_handle_new", PACKAGE = "wickra")
  .wk_obj("cup_and_handle", ptr, "CupAndHandle")
}

#' CyberneticCycle indicator
#' @keywords internal
#' @export
CyberneticCycle <- function(period) {
  ptr <- .Call("wk_cybernetic_cycle_new", period, PACKAGE = "wickra")
  .wk_obj("cybernetic_cycle", ptr, "CyberneticCycle")
}

#' Cypher indicator
#' @keywords internal
#' @export
Cypher <- function() {
  ptr <- .Call("wk_cypher_new", PACKAGE = "wickra")
  .wk_obj("cypher", ptr, "Cypher")
}

#' DayOfWeekProfile indicator
#' @keywords internal
#' @export
DayOfWeekProfile <- function(utc_offset_minutes) {
  ptr <- .Call("wk_day_of_week_profile_new", utc_offset_minutes, PACKAGE = "wickra")
  .wk_obj("day_of_week_profile", ptr, "DayOfWeekProfile", values_cap = as.integer(4096L))
}

#' Decycler indicator
#' @keywords internal
#' @export
Decycler <- function(period) {
  ptr <- .Call("wk_decycler_new", period, PACKAGE = "wickra")
  .wk_obj("decycler", ptr, "Decycler")
}

#' DecyclerOscillator indicator
#' @keywords internal
#' @export
DecyclerOscillator <- function(fast, slow) {
  ptr <- .Call("wk_decycler_oscillator_new", fast, slow, PACKAGE = "wickra")
  .wk_obj("decycler_oscillator", ptr, "DecyclerOscillator")
}

#' Dema indicator
#' @keywords internal
#' @export
Dema <- function(period) {
  ptr <- .Call("wk_dema_new", period, PACKAGE = "wickra")
  .wk_obj("dema", ptr, "Dema")
}

#' DemandIndex indicator
#' @keywords internal
#' @export
DemandIndex <- function(period) {
  ptr <- .Call("wk_demand_index_new", period, PACKAGE = "wickra")
  .wk_obj("demand_index", ptr, "DemandIndex")
}

#' DemarkPivots indicator
#' @keywords internal
#' @export
DemarkPivots <- function() {
  ptr <- .Call("wk_demark_pivots_new", PACKAGE = "wickra")
  .wk_obj("demark_pivots", ptr, "DemarkPivots")
}

#' DepthSlope indicator
#' @keywords internal
#' @export
DepthSlope <- function() {
  ptr <- .Call("wk_depth_slope_new", PACKAGE = "wickra")
  .wk_obj("depth_slope", ptr, "DepthSlope")
}

#' DerivativeOscillator indicator
#' @keywords internal
#' @export
DerivativeOscillator <- function(rsi_period, smooth1, smooth2, signal_period) {
  ptr <- .Call("wk_derivative_oscillator_new", rsi_period, smooth1, smooth2, signal_period, PACKAGE = "wickra")
  .wk_obj("derivative_oscillator", ptr, "DerivativeOscillator")
}

#' DetrendedStdDev indicator
#' @keywords internal
#' @export
DetrendedStdDev <- function(period) {
  ptr <- .Call("wk_detrended_std_dev_new", period, PACKAGE = "wickra")
  .wk_obj("detrended_std_dev", ptr, "DetrendedStdDev")
}

#' DisparityIndex indicator
#' @keywords internal
#' @export
DisparityIndex <- function(period) {
  ptr <- .Call("wk_disparity_index_new", period, PACKAGE = "wickra")
  .wk_obj("disparity_index", ptr, "DisparityIndex")
}

#' DistanceSsd indicator
#' @keywords internal
#' @export
DistanceSsd <- function(period) {
  ptr <- .Call("wk_distance_ssd_new", period, PACKAGE = "wickra")
  .wk_obj("distance_ssd", ptr, "DistanceSsd")
}

#' Doji indicator
#' @keywords internal
#' @export
Doji <- function() {
  ptr <- .Call("wk_doji_new", PACKAGE = "wickra")
  .wk_obj("doji", ptr, "Doji")
}

#' DojiStar indicator
#' @keywords internal
#' @export
DojiStar <- function() {
  ptr <- .Call("wk_doji_star_new", PACKAGE = "wickra")
  .wk_obj("doji_star", ptr, "DojiStar")
}

#' DollarBars indicator
#' @keywords internal
#' @export
DollarBars <- function(dollar_per_bar) {
  ptr <- .Call("wk_dollar_bars_new", dollar_per_bar, PACKAGE = "wickra")
  .wk_obj("dollar_bars", ptr, "DollarBars")
}

#' Donchian indicator
#' @keywords internal
#' @export
Donchian <- function(period) {
  ptr <- .Call("wk_donchian_new", period, PACKAGE = "wickra")
  .wk_obj("donchian", ptr, "Donchian")
}

#' DonchianStop indicator
#' @keywords internal
#' @export
DonchianStop <- function(period) {
  ptr <- .Call("wk_donchian_stop_new", period, PACKAGE = "wickra")
  .wk_obj("donchian_stop", ptr, "DonchianStop")
}

#' DoubleBollinger indicator
#' @keywords internal
#' @export
DoubleBollinger <- function(period, k_inner, k_outer) {
  ptr <- .Call("wk_double_bollinger_new", period, k_inner, k_outer, PACKAGE = "wickra")
  .wk_obj("double_bollinger", ptr, "DoubleBollinger")
}

#' DoubleTopBottom indicator
#' @keywords internal
#' @export
DoubleTopBottom <- function() {
  ptr <- .Call("wk_double_top_bottom_new", PACKAGE = "wickra")
  .wk_obj("double_top_bottom", ptr, "DoubleTopBottom")
}

#' DownsideGapThreeMethods indicator
#' @keywords internal
#' @export
DownsideGapThreeMethods <- function() {
  ptr <- .Call("wk_downside_gap_three_methods_new", PACKAGE = "wickra")
  .wk_obj("downside_gap_three_methods", ptr, "DownsideGapThreeMethods")
}

#' Dpo indicator
#' @keywords internal
#' @export
Dpo <- function(period) {
  ptr <- .Call("wk_dpo_new", period, PACKAGE = "wickra")
  .wk_obj("dpo", ptr, "Dpo")
}

#' DragonflyDoji indicator
#' @keywords internal
#' @export
DragonflyDoji <- function() {
  ptr <- .Call("wk_dragonfly_doji_new", PACKAGE = "wickra")
  .wk_obj("dragonfly_doji", ptr, "DragonflyDoji")
}

#' DrawdownDuration indicator
#' @keywords internal
#' @export
DrawdownDuration <- function() {
  ptr <- .Call("wk_drawdown_duration_new", PACKAGE = "wickra")
  .wk_obj("drawdown_duration", ptr, "DrawdownDuration")
}

#' DumplingTop indicator
#' @keywords internal
#' @export
DumplingTop <- function(period) {
  ptr <- .Call("wk_dumpling_top_new", period, PACKAGE = "wickra")
  .wk_obj("dumpling_top", ptr, "DumplingTop")
}

#' Dx indicator
#' @keywords internal
#' @export
Dx <- function(period) {
  ptr <- .Call("wk_dx_new", period, PACKAGE = "wickra")
  .wk_obj("dx", ptr, "Dx")
}

#' DynamicMomentumIndex indicator
#' @keywords internal
#' @export
DynamicMomentumIndex <- function(period) {
  ptr <- .Call("wk_dynamic_momentum_index_new", period, PACKAGE = "wickra")
  .wk_obj("dynamic_momentum_index", ptr, "DynamicMomentumIndex")
}

#' EaseOfMovement indicator
#' @keywords internal
#' @export
EaseOfMovement <- function(period) {
  ptr <- .Call("wk_ease_of_movement_new", period, PACKAGE = "wickra")
  .wk_obj("ease_of_movement", ptr, "EaseOfMovement")
}

#' EffectiveSpread indicator
#' @keywords internal
#' @export
EffectiveSpread <- function() {
  ptr <- .Call("wk_effective_spread_new", PACKAGE = "wickra")
  .wk_obj("effective_spread", ptr, "EffectiveSpread")
}

#' EhlersStochastic indicator
#' @keywords internal
#' @export
EhlersStochastic <- function(period) {
  ptr <- .Call("wk_ehlers_stochastic_new", period, PACKAGE = "wickra")
  .wk_obj("ehlers_stochastic", ptr, "EhlersStochastic")
}

#' Ehma indicator
#' @keywords internal
#' @export
Ehma <- function(period) {
  ptr <- .Call("wk_ehma_new", period, PACKAGE = "wickra")
  .wk_obj("ehma", ptr, "Ehma")
}

#' ElderImpulse indicator
#' @keywords internal
#' @export
ElderImpulse <- function(ema_period, macd_fast, macd_slow, macd_signal) {
  ptr <- .Call("wk_elder_impulse_new", ema_period, macd_fast, macd_slow, macd_signal, PACKAGE = "wickra")
  .wk_obj("elder_impulse", ptr, "ElderImpulse")
}

#' ElderRay indicator
#' @keywords internal
#' @export
ElderRay <- function(period) {
  ptr <- .Call("wk_elder_ray_new", period, PACKAGE = "wickra")
  .wk_obj("elder_ray", ptr, "ElderRay")
}

#' ElderSafeZone indicator
#' @keywords internal
#' @export
ElderSafeZone <- function(period, coeff) {
  ptr <- .Call("wk_elder_safe_zone_new", period, coeff, PACKAGE = "wickra")
  .wk_obj("elder_safe_zone", ptr, "ElderSafeZone")
}

#' Ema indicator
#' @keywords internal
#' @export
Ema <- function(period) {
  ptr <- .Call("wk_ema_new", period, PACKAGE = "wickra")
  .wk_obj("ema", ptr, "Ema")
}

#' EmpiricalModeDecomposition indicator
#' @keywords internal
#' @export
EmpiricalModeDecomposition <- function(period, fraction) {
  ptr <- .Call("wk_empirical_mode_decomposition_new", period, fraction, PACKAGE = "wickra")
  .wk_obj("empirical_mode_decomposition", ptr, "EmpiricalModeDecomposition")
}

#' Engulfing indicator
#' @keywords internal
#' @export
Engulfing <- function() {
  ptr <- .Call("wk_engulfing_new", PACKAGE = "wickra")
  .wk_obj("engulfing", ptr, "Engulfing")
}

#' Equivolume indicator
#' @keywords internal
#' @export
Equivolume <- function(period) {
  ptr <- .Call("wk_equivolume_new", period, PACKAGE = "wickra")
  .wk_obj("equivolume", ptr, "Equivolume")
}

#' EstimatedLeverageRatio indicator
#' @keywords internal
#' @export
EstimatedLeverageRatio <- function() {
  ptr <- .Call("wk_estimated_leverage_ratio_new", PACKAGE = "wickra")
  .wk_obj("estimated_leverage_ratio", ptr, "EstimatedLeverageRatio")
}

#' EvenBetterSinewave indicator
#' @keywords internal
#' @export
EvenBetterSinewave <- function(hp_period, ssf_length) {
  ptr <- .Call("wk_even_better_sinewave_new", hp_period, ssf_length, PACKAGE = "wickra")
  .wk_obj("even_better_sinewave", ptr, "EvenBetterSinewave")
}

#' EveningDojiStar indicator
#' @keywords internal
#' @export
EveningDojiStar <- function() {
  ptr <- .Call("wk_evening_doji_star_new", PACKAGE = "wickra")
  .wk_obj("evening_doji_star", ptr, "EveningDojiStar")
}

#' Evwma indicator
#' @keywords internal
#' @export
Evwma <- function(period) {
  ptr <- .Call("wk_evwma_new", period, PACKAGE = "wickra")
  .wk_obj("evwma", ptr, "Evwma")
}

#' EwmaVolatility indicator
#' @keywords internal
#' @export
EwmaVolatility <- function(lambda) {
  ptr <- .Call("wk_ewma_volatility_new", lambda, PACKAGE = "wickra")
  .wk_obj("ewma_volatility", ptr, "EwmaVolatility")
}

#' Expectancy indicator
#' @keywords internal
#' @export
Expectancy <- function(period) {
  ptr <- .Call("wk_expectancy_new", period, PACKAGE = "wickra")
  .wk_obj("expectancy", ptr, "Expectancy")
}

#' FallingThreeMethods indicator
#' @keywords internal
#' @export
FallingThreeMethods <- function() {
  ptr <- .Call("wk_falling_three_methods_new", PACKAGE = "wickra")
  .wk_obj("falling_three_methods", ptr, "FallingThreeMethods")
}

#' Fama indicator
#' @keywords internal
#' @export
Fama <- function(fast_limit, slow_limit) {
  ptr <- .Call("wk_fama_new", fast_limit, slow_limit, PACKAGE = "wickra")
  .wk_obj("fama", ptr, "Fama")
}

#' FibArcs indicator
#' @keywords internal
#' @export
FibArcs <- function() {
  ptr <- .Call("wk_fib_arcs_new", PACKAGE = "wickra")
  .wk_obj("fib_arcs", ptr, "FibArcs")
}

#' FibChannel indicator
#' @keywords internal
#' @export
FibChannel <- function() {
  ptr <- .Call("wk_fib_channel_new", PACKAGE = "wickra")
  .wk_obj("fib_channel", ptr, "FibChannel")
}

#' FibConfluence indicator
#' @keywords internal
#' @export
FibConfluence <- function() {
  ptr <- .Call("wk_fib_confluence_new", PACKAGE = "wickra")
  .wk_obj("fib_confluence", ptr, "FibConfluence")
}

#' FibExtension indicator
#' @keywords internal
#' @export
FibExtension <- function() {
  ptr <- .Call("wk_fib_extension_new", PACKAGE = "wickra")
  .wk_obj("fib_extension", ptr, "FibExtension")
}

#' FibFan indicator
#' @keywords internal
#' @export
FibFan <- function() {
  ptr <- .Call("wk_fib_fan_new", PACKAGE = "wickra")
  .wk_obj("fib_fan", ptr, "FibFan")
}

#' FibProjection indicator
#' @keywords internal
#' @export
FibProjection <- function() {
  ptr <- .Call("wk_fib_projection_new", PACKAGE = "wickra")
  .wk_obj("fib_projection", ptr, "FibProjection")
}

#' FibRetracement indicator
#' @keywords internal
#' @export
FibRetracement <- function() {
  ptr <- .Call("wk_fib_retracement_new", PACKAGE = "wickra")
  .wk_obj("fib_retracement", ptr, "FibRetracement")
}

#' FibTimeZones indicator
#' @keywords internal
#' @export
FibTimeZones <- function() {
  ptr <- .Call("wk_fib_time_zones_new", PACKAGE = "wickra")
  .wk_obj("fib_time_zones", ptr, "FibTimeZones")
}

#' FibonacciPivots indicator
#' @keywords internal
#' @export
FibonacciPivots <- function() {
  ptr <- .Call("wk_fibonacci_pivots_new", PACKAGE = "wickra")
  .wk_obj("fibonacci_pivots", ptr, "FibonacciPivots")
}

#' FisherRsi indicator
#' @keywords internal
#' @export
FisherRsi <- function(period) {
  ptr <- .Call("wk_fisher_rsi_new", period, PACKAGE = "wickra")
  .wk_obj("fisher_rsi", ptr, "FisherRsi")
}

#' FisherTransform indicator
#' @keywords internal
#' @export
FisherTransform <- function(period) {
  ptr <- .Call("wk_fisher_transform_new", period, PACKAGE = "wickra")
  .wk_obj("fisher_transform", ptr, "FisherTransform")
}

#' FlagPennant indicator
#' @keywords internal
#' @export
FlagPennant <- function() {
  ptr <- .Call("wk_flag_pennant_new", PACKAGE = "wickra")
  .wk_obj("flag_pennant", ptr, "FlagPennant")
}

#' Footprint indicator
#' @keywords internal
#' @export
Footprint <- function(tick_size) {
  ptr <- .Call("wk_footprint_new", tick_size, PACKAGE = "wickra")
  .wk_obj("footprint", ptr, "Footprint")
}

#' ForceIndex indicator
#' @keywords internal
#' @export
ForceIndex <- function(period) {
  ptr <- .Call("wk_force_index_new", period, PACKAGE = "wickra")
  .wk_obj("force_index", ptr, "ForceIndex")
}

#' FractalChaosBands indicator
#' @keywords internal
#' @export
FractalChaosBands <- function(k) {
  ptr <- .Call("wk_fractal_chaos_bands_new", k, PACKAGE = "wickra")
  .wk_obj("fractal_chaos_bands", ptr, "FractalChaosBands")
}

#' Frama indicator
#' @keywords internal
#' @export
Frama <- function(period) {
  ptr <- .Call("wk_frama_new", period, PACKAGE = "wickra")
  .wk_obj("frama", ptr, "Frama")
}

#' FryPanBottom indicator
#' @keywords internal
#' @export
FryPanBottom <- function(period) {
  ptr <- .Call("wk_fry_pan_bottom_new", period, PACKAGE = "wickra")
  .wk_obj("fry_pan_bottom", ptr, "FryPanBottom")
}

#' FundingBasis indicator
#' @keywords internal
#' @export
FundingBasis <- function() {
  ptr <- .Call("wk_funding_basis_new", PACKAGE = "wickra")
  .wk_obj("funding_basis", ptr, "FundingBasis")
}

#' FundingImpliedApr indicator
#' @keywords internal
#' @export
FundingImpliedApr <- function(intervals_per_year) {
  ptr <- .Call("wk_funding_implied_apr_new", intervals_per_year, PACKAGE = "wickra")
  .wk_obj("funding_implied_apr", ptr, "FundingImpliedApr")
}

#' FundingRate indicator
#' @keywords internal
#' @export
FundingRate <- function() {
  ptr <- .Call("wk_funding_rate_new", PACKAGE = "wickra")
  .wk_obj("funding_rate", ptr, "FundingRate")
}

#' FundingRateMean indicator
#' @keywords internal
#' @export
FundingRateMean <- function(window) {
  ptr <- .Call("wk_funding_rate_mean_new", window, PACKAGE = "wickra")
  .wk_obj("funding_rate_mean", ptr, "FundingRateMean")
}

#' FundingRateZScore indicator
#' @keywords internal
#' @export
FundingRateZScore <- function(window) {
  ptr <- .Call("wk_funding_rate_z_score_new", window, PACKAGE = "wickra")
  .wk_obj("funding_rate_z_score", ptr, "FundingRateZScore")
}

#' GainLossRatio indicator
#' @keywords internal
#' @export
GainLossRatio <- function(period) {
  ptr <- .Call("wk_gain_loss_ratio_new", period, PACKAGE = "wickra")
  .wk_obj("gain_loss_ratio", ptr, "GainLossRatio")
}

#' GainToPainRatio indicator
#' @keywords internal
#' @export
GainToPainRatio <- function(period) {
  ptr <- .Call("wk_gain_to_pain_ratio_new", period, PACKAGE = "wickra")
  .wk_obj("gain_to_pain_ratio", ptr, "GainToPainRatio")
}

#' GapSideBySideWhite indicator
#' @keywords internal
#' @export
GapSideBySideWhite <- function() {
  ptr <- .Call("wk_gap_side_by_side_white_new", PACKAGE = "wickra")
  .wk_obj("gap_side_by_side_white", ptr, "GapSideBySideWhite")
}

#' Garch11 indicator
#' @keywords internal
#' @export
Garch11 <- function(omega, alpha, beta) {
  ptr <- .Call("wk_garch11_new", omega, alpha, beta, PACKAGE = "wickra")
  .wk_obj("garch11", ptr, "Garch11")
}

#' GarmanKlassVolatility indicator
#' @keywords internal
#' @export
GarmanKlassVolatility <- function(period, trading_periods) {
  ptr <- .Call("wk_garman_klass_volatility_new", period, trading_periods, PACKAGE = "wickra")
  .wk_obj("garman_klass_volatility", ptr, "GarmanKlassVolatility")
}

#' Gartley indicator
#' @keywords internal
#' @export
Gartley <- function() {
  ptr <- .Call("wk_gartley_new", PACKAGE = "wickra")
  .wk_obj("gartley", ptr, "Gartley")
}

#' GatorOscillator indicator
#' @keywords internal
#' @export
GatorOscillator <- function(jaw_period, teeth_period, lips_period) {
  ptr <- .Call("wk_gator_oscillator_new", jaw_period, teeth_period, lips_period, PACKAGE = "wickra")
  .wk_obj("gator_oscillator", ptr, "GatorOscillator")
}

#' GeneralizedDema indicator
#' @keywords internal
#' @export
GeneralizedDema <- function(period, v) {
  ptr <- .Call("wk_generalized_dema_new", period, v, PACKAGE = "wickra")
  .wk_obj("generalized_dema", ptr, "GeneralizedDema")
}

#' GeometricMa indicator
#' @keywords internal
#' @export
GeometricMa <- function(period) {
  ptr <- .Call("wk_geometric_ma_new", period, PACKAGE = "wickra")
  .wk_obj("geometric_ma", ptr, "GeometricMa")
}

#' GoldenPocket indicator
#' @keywords internal
#' @export
GoldenPocket <- function() {
  ptr <- .Call("wk_golden_pocket_new", PACKAGE = "wickra")
  .wk_obj("golden_pocket", ptr, "GoldenPocket")
}

#' GrangerCausality indicator
#' @keywords internal
#' @export
GrangerCausality <- function(period, lag) {
  ptr <- .Call("wk_granger_causality_new", period, lag, PACKAGE = "wickra")
  .wk_obj("granger_causality", ptr, "GrangerCausality")
}

#' GravestoneDoji indicator
#' @keywords internal
#' @export
GravestoneDoji <- function() {
  ptr <- .Call("wk_gravestone_doji_new", PACKAGE = "wickra")
  .wk_obj("gravestone_doji", ptr, "GravestoneDoji")
}

#' Hammer indicator
#' @keywords internal
#' @export
Hammer <- function() {
  ptr <- .Call("wk_hammer_new", PACKAGE = "wickra")
  .wk_obj("hammer", ptr, "Hammer")
}

#' HangingMan indicator
#' @keywords internal
#' @export
HangingMan <- function() {
  ptr <- .Call("wk_hanging_man_new", PACKAGE = "wickra")
  .wk_obj("hanging_man", ptr, "HangingMan")
}

#' Harami indicator
#' @keywords internal
#' @export
Harami <- function() {
  ptr <- .Call("wk_harami_new", PACKAGE = "wickra")
  .wk_obj("harami", ptr, "Harami")
}

#' HaramiCross indicator
#' @keywords internal
#' @export
HaramiCross <- function() {
  ptr <- .Call("wk_harami_cross_new", PACKAGE = "wickra")
  .wk_obj("harami_cross", ptr, "HaramiCross")
}

#' HasbrouckInformationShare indicator
#' @keywords internal
#' @export
HasbrouckInformationShare <- function(period) {
  ptr <- .Call("wk_hasbrouck_information_share_new", period, PACKAGE = "wickra")
  .wk_obj("hasbrouck_information_share", ptr, "HasbrouckInformationShare")
}

#' HeadAndShoulders indicator
#' @keywords internal
#' @export
HeadAndShoulders <- function() {
  ptr <- .Call("wk_head_and_shoulders_new", PACKAGE = "wickra")
  .wk_obj("head_and_shoulders", ptr, "HeadAndShoulders")
}

#' HeikinAshi indicator
#' @keywords internal
#' @export
HeikinAshi <- function() {
  ptr <- .Call("wk_heikin_ashi_new", PACKAGE = "wickra")
  .wk_obj("heikin_ashi", ptr, "HeikinAshi")
}

#' HeikinAshiOscillator indicator
#' @keywords internal
#' @export
HeikinAshiOscillator <- function(period) {
  ptr <- .Call("wk_heikin_ashi_oscillator_new", period, PACKAGE = "wickra")
  .wk_obj("heikin_ashi_oscillator", ptr, "HeikinAshiOscillator")
}

#' HiLoActivator indicator
#' @keywords internal
#' @export
HiLoActivator <- function(period) {
  ptr <- .Call("wk_hi_lo_activator_new", period, PACKAGE = "wickra")
  .wk_obj("hi_lo_activator", ptr, "HiLoActivator")
}

#' HighLowIndex indicator
#' @keywords internal
#' @export
HighLowIndex <- function(period) {
  ptr <- .Call("wk_high_low_index_new", period, PACKAGE = "wickra")
  .wk_obj("high_low_index", ptr, "HighLowIndex")
}

#' HighLowRange indicator
#' @keywords internal
#' @export
HighLowRange <- function() {
  ptr <- .Call("wk_high_low_range_new", PACKAGE = "wickra")
  .wk_obj("high_low_range", ptr, "HighLowRange")
}

#' HighLowVolumeNodes indicator
#' @keywords internal
#' @export
HighLowVolumeNodes <- function(period, bins) {
  ptr <- .Call("wk_high_low_volume_nodes_new", period, bins, PACKAGE = "wickra")
  .wk_obj("high_low_volume_nodes", ptr, "HighLowVolumeNodes")
}

#' HighWave indicator
#' @keywords internal
#' @export
HighWave <- function() {
  ptr <- .Call("wk_high_wave_new", PACKAGE = "wickra")
  .wk_obj("high_wave", ptr, "HighWave")
}

#' HighpassFilter indicator
#' @keywords internal
#' @export
HighpassFilter <- function(period) {
  ptr <- .Call("wk_highpass_filter_new", period, PACKAGE = "wickra")
  .wk_obj("highpass_filter", ptr, "HighpassFilter")
}

#' Hikkake indicator
#' @keywords internal
#' @export
Hikkake <- function() {
  ptr <- .Call("wk_hikkake_new", PACKAGE = "wickra")
  .wk_obj("hikkake", ptr, "Hikkake")
}

#' HikkakeModified indicator
#' @keywords internal
#' @export
HikkakeModified <- function() {
  ptr <- .Call("wk_hikkake_modified_new", PACKAGE = "wickra")
  .wk_obj("hikkake_modified", ptr, "HikkakeModified")
}

#' HilbertDominantCycle indicator
#' @keywords internal
#' @export
HilbertDominantCycle <- function() {
  ptr <- .Call("wk_hilbert_dominant_cycle_new", PACKAGE = "wickra")
  .wk_obj("hilbert_dominant_cycle", ptr, "HilbertDominantCycle")
}

#' HistoricalVolatility indicator
#' @keywords internal
#' @export
HistoricalVolatility <- function(period, trading_periods) {
  ptr <- .Call("wk_historical_volatility_new", period, trading_periods, PACKAGE = "wickra")
  .wk_obj("historical_volatility", ptr, "HistoricalVolatility")
}

#' Hma indicator
#' @keywords internal
#' @export
Hma <- function(period) {
  ptr <- .Call("wk_hma_new", period, PACKAGE = "wickra")
  .wk_obj("hma", ptr, "Hma")
}

#' HoltWinters indicator
#' @keywords internal
#' @export
HoltWinters <- function(alpha, beta) {
  ptr <- .Call("wk_holt_winters_new", alpha, beta, PACKAGE = "wickra")
  .wk_obj("holt_winters", ptr, "HoltWinters")
}

#' HomingPigeon indicator
#' @keywords internal
#' @export
HomingPigeon <- function() {
  ptr <- .Call("wk_homing_pigeon_new", PACKAGE = "wickra")
  .wk_obj("homing_pigeon", ptr, "HomingPigeon")
}

#' HtDcPhase indicator
#' @keywords internal
#' @export
HtDcPhase <- function() {
  ptr <- .Call("wk_ht_dc_phase_new", PACKAGE = "wickra")
  .wk_obj("ht_dc_phase", ptr, "HtDcPhase")
}

#' HtPhasor indicator
#' @keywords internal
#' @export
HtPhasor <- function() {
  ptr <- .Call("wk_ht_phasor_new", PACKAGE = "wickra")
  .wk_obj("ht_phasor", ptr, "HtPhasor")
}

#' HtTrendMode indicator
#' @keywords internal
#' @export
HtTrendMode <- function() {
  ptr <- .Call("wk_ht_trend_mode_new", PACKAGE = "wickra")
  .wk_obj("ht_trend_mode", ptr, "HtTrendMode")
}

#' HurstChannel indicator
#' @keywords internal
#' @export
HurstChannel <- function(period, multiplier) {
  ptr <- .Call("wk_hurst_channel_new", period, multiplier, PACKAGE = "wickra")
  .wk_obj("hurst_channel", ptr, "HurstChannel")
}

#' HurstExponent indicator
#' @keywords internal
#' @export
HurstExponent <- function(period, chunks) {
  ptr <- .Call("wk_hurst_exponent_new", period, chunks, PACKAGE = "wickra")
  .wk_obj("hurst_exponent", ptr, "HurstExponent")
}

#' Ichimoku indicator
#' @keywords internal
#' @export
Ichimoku <- function(tenkan_period, kijun_period, senkou_b_period, displacement) {
  ptr <- .Call("wk_ichimoku_new", tenkan_period, kijun_period, senkou_b_period, displacement, PACKAGE = "wickra")
  .wk_obj("ichimoku", ptr, "Ichimoku")
}

#' IdenticalThreeCrows indicator
#' @keywords internal
#' @export
IdenticalThreeCrows <- function() {
  ptr <- .Call("wk_identical_three_crows_new", PACKAGE = "wickra")
  .wk_obj("identical_three_crows", ptr, "IdenticalThreeCrows")
}

#' ImbalanceBars indicator
#' @keywords internal
#' @export
ImbalanceBars <- function(threshold) {
  ptr <- .Call("wk_imbalance_bars_new", threshold, PACKAGE = "wickra")
  .wk_obj("imbalance_bars", ptr, "ImbalanceBars")
}

#' InNeck indicator
#' @keywords internal
#' @export
InNeck <- function() {
  ptr <- .Call("wk_in_neck_new", PACKAGE = "wickra")
  .wk_obj("in_neck", ptr, "InNeck")
}

#' Inertia indicator
#' @keywords internal
#' @export
Inertia <- function(rvi_period, linreg_period) {
  ptr <- .Call("wk_inertia_new", rvi_period, linreg_period, PACKAGE = "wickra")
  .wk_obj("inertia", ptr, "Inertia")
}

#' InformationRatio indicator
#' @keywords internal
#' @export
InformationRatio <- function(period) {
  ptr <- .Call("wk_information_ratio_new", period, PACKAGE = "wickra")
  .wk_obj("information_ratio", ptr, "InformationRatio")
}

#' InitialBalance indicator
#' @keywords internal
#' @export
InitialBalance <- function(period) {
  ptr <- .Call("wk_initial_balance_new", period, PACKAGE = "wickra")
  .wk_obj("initial_balance", ptr, "InitialBalance")
}

#' InstantaneousTrendline indicator
#' @keywords internal
#' @export
InstantaneousTrendline <- function(period) {
  ptr <- .Call("wk_instantaneous_trendline_new", period, PACKAGE = "wickra")
  .wk_obj("instantaneous_trendline", ptr, "InstantaneousTrendline")
}

#' IntradayIntensity indicator
#' @keywords internal
#' @export
IntradayIntensity <- function() {
  ptr <- .Call("wk_intraday_intensity_new", PACKAGE = "wickra")
  .wk_obj("intraday_intensity", ptr, "IntradayIntensity")
}

#' IntradayMomentumIndex indicator
#' @keywords internal
#' @export
IntradayMomentumIndex <- function(period) {
  ptr <- .Call("wk_intraday_momentum_index_new", period, PACKAGE = "wickra")
  .wk_obj("intraday_momentum_index", ptr, "IntradayMomentumIndex")
}

#' IntradayVolatilityProfile indicator
#' @keywords internal
#' @export
IntradayVolatilityProfile <- function(buckets, utc_offset_minutes) {
  ptr <- .Call("wk_intraday_volatility_profile_new", buckets, utc_offset_minutes, PACKAGE = "wickra")
  .wk_obj("intraday_volatility_profile", ptr, "IntradayVolatilityProfile", values_cap = as.integer(buckets))
}

#' InverseFisherTransform indicator
#' @keywords internal
#' @export
InverseFisherTransform <- function(scale) {
  ptr <- .Call("wk_inverse_fisher_transform_new", scale, PACKAGE = "wickra")
  .wk_obj("inverse_fisher_transform", ptr, "InverseFisherTransform")
}

#' InvertedHammer indicator
#' @keywords internal
#' @export
InvertedHammer <- function() {
  ptr <- .Call("wk_inverted_hammer_new", PACKAGE = "wickra")
  .wk_obj("inverted_hammer", ptr, "InvertedHammer")
}

#' JarqueBera indicator
#' @keywords internal
#' @export
JarqueBera <- function(period) {
  ptr <- .Call("wk_jarque_bera_new", period, PACKAGE = "wickra")
  .wk_obj("jarque_bera", ptr, "JarqueBera")
}

#' Jma indicator
#' @keywords internal
#' @export
Jma <- function(period, phase, power) {
  ptr <- .Call("wk_jma_new", period, phase, power, PACKAGE = "wickra")
  .wk_obj("jma", ptr, "Jma")
}

#' JumpIndicator indicator
#' @keywords internal
#' @export
JumpIndicator <- function(period, threshold) {
  ptr <- .Call("wk_jump_indicator_new", period, threshold, PACKAGE = "wickra")
  .wk_obj("jump_indicator", ptr, "JumpIndicator")
}

#' KRatio indicator
#' @keywords internal
#' @export
KRatio <- function(period) {
  ptr <- .Call("wk_k_ratio_new", period, PACKAGE = "wickra")
  .wk_obj("k_ratio", ptr, "KRatio")
}

#' KagiBars indicator
#' @keywords internal
#' @export
KagiBars <- function(reversal) {
  ptr <- .Call("wk_kagi_bars_new", reversal, PACKAGE = "wickra")
  .wk_obj("kagi_bars", ptr, "KagiBars")
}

#' KalmanHedgeRatio indicator
#' @keywords internal
#' @export
KalmanHedgeRatio <- function(delta, observation_var) {
  ptr <- .Call("wk_kalman_hedge_ratio_new", delta, observation_var, PACKAGE = "wickra")
  .wk_obj("kalman_hedge_ratio", ptr, "KalmanHedgeRatio")
}

#' Kama indicator
#' @keywords internal
#' @export
Kama <- function(er_period, fast, slow) {
  ptr <- .Call("wk_kama_new", er_period, fast, slow, PACKAGE = "wickra")
  .wk_obj("kama", ptr, "Kama")
}

#' KaseDevStop indicator
#' @keywords internal
#' @export
KaseDevStop <- function(period, dev) {
  ptr <- .Call("wk_kase_dev_stop_new", period, dev, PACKAGE = "wickra")
  .wk_obj("kase_dev_stop", ptr, "KaseDevStop")
}

#' KasePermissionStochastic indicator
#' @keywords internal
#' @export
KasePermissionStochastic <- function(length, smooth) {
  ptr <- .Call("wk_kase_permission_stochastic_new", length, smooth, PACKAGE = "wickra")
  .wk_obj("kase_permission_stochastic", ptr, "KasePermissionStochastic")
}

#' KellyCriterion indicator
#' @keywords internal
#' @export
KellyCriterion <- function(period) {
  ptr <- .Call("wk_kelly_criterion_new", period, PACKAGE = "wickra")
  .wk_obj("kelly_criterion", ptr, "KellyCriterion")
}

#' Keltner indicator
#' @keywords internal
#' @export
Keltner <- function(ema_period, atr_period, multiplier) {
  ptr <- .Call("wk_keltner_new", ema_period, atr_period, multiplier, PACKAGE = "wickra")
  .wk_obj("keltner", ptr, "Keltner")
}

#' KendallTau indicator
#' @keywords internal
#' @export
KendallTau <- function(period) {
  ptr <- .Call("wk_kendall_tau_new", period, PACKAGE = "wickra")
  .wk_obj("kendall_tau", ptr, "KendallTau")
}

#' Kicking indicator
#' @keywords internal
#' @export
Kicking <- function() {
  ptr <- .Call("wk_kicking_new", PACKAGE = "wickra")
  .wk_obj("kicking", ptr, "Kicking")
}

#' KickingByLength indicator
#' @keywords internal
#' @export
KickingByLength <- function() {
  ptr <- .Call("wk_kicking_by_length_new", PACKAGE = "wickra")
  .wk_obj("kicking_by_length", ptr, "KickingByLength")
}

#' Kst indicator
#' @keywords internal
#' @export
Kst <- function(roc1, roc2, roc3, roc4, sma1, sma2, sma3, sma4, signal) {
  ptr <- .Call("wk_kst_new", roc1, roc2, roc3, roc4, sma1, sma2, sma3, sma4, signal, PACKAGE = "wickra")
  .wk_obj("kst", ptr, "Kst")
}

#' Kurtosis indicator
#' @keywords internal
#' @export
Kurtosis <- function(period) {
  ptr <- .Call("wk_kurtosis_new", period, PACKAGE = "wickra")
  .wk_obj("kurtosis", ptr, "Kurtosis")
}

#' Kvo indicator
#' @keywords internal
#' @export
Kvo <- function(fast, slow) {
  ptr <- .Call("wk_kvo_new", fast, slow, PACKAGE = "wickra")
  .wk_obj("kvo", ptr, "Kvo")
}

#' KylesLambda indicator
#' @keywords internal
#' @export
KylesLambda <- function(window) {
  ptr <- .Call("wk_kyles_lambda_new", window, PACKAGE = "wickra")
  .wk_obj("kyles_lambda", ptr, "KylesLambda")
}

#' LadderBottom indicator
#' @keywords internal
#' @export
LadderBottom <- function() {
  ptr <- .Call("wk_ladder_bottom_new", PACKAGE = "wickra")
  .wk_obj("ladder_bottom", ptr, "LadderBottom")
}

#' LaguerreRsi indicator
#' @keywords internal
#' @export
LaguerreRsi <- function(gamma) {
  ptr <- .Call("wk_laguerre_rsi_new", gamma, PACKAGE = "wickra")
  .wk_obj("laguerre_rsi", ptr, "LaguerreRsi")
}

#' LeadLagCrossCorrelation indicator
#' @keywords internal
#' @export
LeadLagCrossCorrelation <- function(window, max_lag) {
  ptr <- .Call("wk_lead_lag_cross_correlation_new", window, max_lag, PACKAGE = "wickra")
  .wk_obj("lead_lag_cross_correlation", ptr, "LeadLagCrossCorrelation")
}

#' LinRegAngle indicator
#' @keywords internal
#' @export
LinRegAngle <- function(period) {
  ptr <- .Call("wk_lin_reg_angle_new", period, PACKAGE = "wickra")
  .wk_obj("lin_reg_angle", ptr, "LinRegAngle")
}

#' LinRegChannel indicator
#' @keywords internal
#' @export
LinRegChannel <- function(period, multiplier) {
  ptr <- .Call("wk_lin_reg_channel_new", period, multiplier, PACKAGE = "wickra")
  .wk_obj("lin_reg_channel", ptr, "LinRegChannel")
}

#' LinRegIntercept indicator
#' @keywords internal
#' @export
LinRegIntercept <- function(period) {
  ptr <- .Call("wk_lin_reg_intercept_new", period, PACKAGE = "wickra")
  .wk_obj("lin_reg_intercept", ptr, "LinRegIntercept")
}

#' LinRegSlope indicator
#' @keywords internal
#' @export
LinRegSlope <- function(period) {
  ptr <- .Call("wk_lin_reg_slope_new", period, PACKAGE = "wickra")
  .wk_obj("lin_reg_slope", ptr, "LinRegSlope")
}

#' LinearRegression indicator
#' @keywords internal
#' @export
LinearRegression <- function(period) {
  ptr <- .Call("wk_linear_regression_new", period, PACKAGE = "wickra")
  .wk_obj("linear_regression", ptr, "LinearRegression")
}

#' LiquidationFeatures indicator
#' @keywords internal
#' @export
LiquidationFeatures <- function() {
  ptr <- .Call("wk_liquidation_features_new", PACKAGE = "wickra")
  .wk_obj("liquidation_features", ptr, "LiquidationFeatures")
}

#' LogReturn indicator
#' @keywords internal
#' @export
LogReturn <- function(period) {
  ptr <- .Call("wk_log_return_new", period, PACKAGE = "wickra")
  .wk_obj("log_return", ptr, "LogReturn")
}

#' LongLeggedDoji indicator
#' @keywords internal
#' @export
LongLeggedDoji <- function() {
  ptr <- .Call("wk_long_legged_doji_new", PACKAGE = "wickra")
  .wk_obj("long_legged_doji", ptr, "LongLeggedDoji")
}

#' LongLine indicator
#' @keywords internal
#' @export
LongLine <- function() {
  ptr <- .Call("wk_long_line_new", PACKAGE = "wickra")
  .wk_obj("long_line", ptr, "LongLine")
}

#' LongShortRatio indicator
#' @keywords internal
#' @export
LongShortRatio <- function() {
  ptr <- .Call("wk_long_short_ratio_new", PACKAGE = "wickra")
  .wk_obj("long_short_ratio", ptr, "LongShortRatio")
}

#' M2Measure indicator
#' @keywords internal
#' @export
M2Measure <- function(period, risk_free, benchmark_stddev) {
  ptr <- .Call("wk_m2_measure_new", period, risk_free, benchmark_stddev, PACKAGE = "wickra")
  .wk_obj("m2_measure", ptr, "M2Measure")
}

#' MaEnvelope indicator
#' @keywords internal
#' @export
MaEnvelope <- function(period, percent) {
  ptr <- .Call("wk_ma_envelope_new", period, percent, PACKAGE = "wickra")
  .wk_obj("ma_envelope", ptr, "MaEnvelope")
}

#' MacdExt indicator
#' @keywords internal
#' @export
MacdExt <- function(fast, fast_type, slow, slow_type, signal, signal_type) {
  ptr <- .Call("wk_macd_ext_new", fast, fast_type, slow, slow_type, signal, signal_type, PACKAGE = "wickra")
  .wk_obj("macd_ext", ptr, "MacdExt")
}

#' MacdFix indicator
#' @keywords internal
#' @export
MacdFix <- function(signal) {
  ptr <- .Call("wk_macd_fix_new", signal, PACKAGE = "wickra")
  .wk_obj("macd_fix", ptr, "MacdFix")
}

#' MacdHistogram indicator
#' @keywords internal
#' @export
MacdHistogram <- function(fast, slow, signal) {
  ptr <- .Call("wk_macd_histogram_new", fast, slow, signal, PACKAGE = "wickra")
  .wk_obj("macd_histogram", ptr, "MacdHistogram")
}

#' MacdIndicator indicator
#' @keywords internal
#' @export
MacdIndicator <- function(fast, slow, signal) {
  ptr <- .Call("wk_macd_indicator_new", fast, slow, signal, PACKAGE = "wickra")
  .wk_obj("macd_indicator", ptr, "MacdIndicator")
}

#' Mama indicator
#' @keywords internal
#' @export
Mama <- function(fast_limit, slow_limit) {
  ptr <- .Call("wk_mama_new", fast_limit, slow_limit, PACKAGE = "wickra")
  .wk_obj("mama", ptr, "Mama")
}

#' MarketFacilitationIndex indicator
#' @keywords internal
#' @export
MarketFacilitationIndex <- function() {
  ptr <- .Call("wk_market_facilitation_index_new", PACKAGE = "wickra")
  .wk_obj("market_facilitation_index", ptr, "MarketFacilitationIndex")
}

#' MartinRatio indicator
#' @keywords internal
#' @export
MartinRatio <- function(period) {
  ptr <- .Call("wk_martin_ratio_new", period, PACKAGE = "wickra")
  .wk_obj("martin_ratio", ptr, "MartinRatio")
}

#' Marubozu indicator
#' @keywords internal
#' @export
Marubozu <- function() {
  ptr <- .Call("wk_marubozu_new", PACKAGE = "wickra")
  .wk_obj("marubozu", ptr, "Marubozu")
}

#' MassIndex indicator
#' @keywords internal
#' @export
MassIndex <- function(ema_period, sum_period) {
  ptr <- .Call("wk_mass_index_new", ema_period, sum_period, PACKAGE = "wickra")
  .wk_obj("mass_index", ptr, "MassIndex")
}

#' MatHold indicator
#' @keywords internal
#' @export
MatHold <- function() {
  ptr <- .Call("wk_mat_hold_new", PACKAGE = "wickra")
  .wk_obj("mat_hold", ptr, "MatHold")
}

#' MatchingLow indicator
#' @keywords internal
#' @export
MatchingLow <- function() {
  ptr <- .Call("wk_matching_low_new", PACKAGE = "wickra")
  .wk_obj("matching_low", ptr, "MatchingLow")
}

#' MaxDrawdown indicator
#' @keywords internal
#' @export
MaxDrawdown <- function(period) {
  ptr <- .Call("wk_max_drawdown_new", period, PACKAGE = "wickra")
  .wk_obj("max_drawdown", ptr, "MaxDrawdown")
}

#' McClellanOscillator indicator
#' @keywords internal
#' @export
McClellanOscillator <- function() {
  ptr <- .Call("wk_mc_clellan_oscillator_new", PACKAGE = "wickra")
  .wk_obj("mc_clellan_oscillator", ptr, "McClellanOscillator")
}

#' McClellanSummationIndex indicator
#' @keywords internal
#' @export
McClellanSummationIndex <- function() {
  ptr <- .Call("wk_mc_clellan_summation_index_new", PACKAGE = "wickra")
  .wk_obj("mc_clellan_summation_index", ptr, "McClellanSummationIndex")
}

#' McGinleyDynamic indicator
#' @keywords internal
#' @export
McGinleyDynamic <- function(period) {
  ptr <- .Call("wk_mc_ginley_dynamic_new", period, PACKAGE = "wickra")
  .wk_obj("mc_ginley_dynamic", ptr, "McGinleyDynamic")
}

#' MedianAbsoluteDeviation indicator
#' @keywords internal
#' @export
MedianAbsoluteDeviation <- function(period) {
  ptr <- .Call("wk_median_absolute_deviation_new", period, PACKAGE = "wickra")
  .wk_obj("median_absolute_deviation", ptr, "MedianAbsoluteDeviation")
}

#' MedianChannel indicator
#' @keywords internal
#' @export
MedianChannel <- function(period, multiplier) {
  ptr <- .Call("wk_median_channel_new", period, multiplier, PACKAGE = "wickra")
  .wk_obj("median_channel", ptr, "MedianChannel")
}

#' MedianMa indicator
#' @keywords internal
#' @export
MedianMa <- function(period) {
  ptr <- .Call("wk_median_ma_new", period, PACKAGE = "wickra")
  .wk_obj("median_ma", ptr, "MedianMa")
}

#' MedianPrice indicator
#' @keywords internal
#' @export
MedianPrice <- function() {
  ptr <- .Call("wk_median_price_new", PACKAGE = "wickra")
  .wk_obj("median_price", ptr, "MedianPrice")
}

#' Mfi indicator
#' @keywords internal
#' @export
Mfi <- function(period) {
  ptr <- .Call("wk_mfi_new", period, PACKAGE = "wickra")
  .wk_obj("mfi", ptr, "Mfi")
}

#' Microprice indicator
#' @keywords internal
#' @export
Microprice <- function() {
  ptr <- .Call("wk_microprice_new", PACKAGE = "wickra")
  .wk_obj("microprice", ptr, "Microprice")
}

#' MidPoint indicator
#' @keywords internal
#' @export
MidPoint <- function(period) {
  ptr <- .Call("wk_mid_point_new", period, PACKAGE = "wickra")
  .wk_obj("mid_point", ptr, "MidPoint")
}

#' MidPrice indicator
#' @keywords internal
#' @export
MidPrice <- function(period) {
  ptr <- .Call("wk_mid_price_new", period, PACKAGE = "wickra")
  .wk_obj("mid_price", ptr, "MidPrice")
}

#' MinusDi indicator
#' @keywords internal
#' @export
MinusDi <- function(period) {
  ptr <- .Call("wk_minus_di_new", period, PACKAGE = "wickra")
  .wk_obj("minus_di", ptr, "MinusDi")
}

#' MinusDm indicator
#' @keywords internal
#' @export
MinusDm <- function(period) {
  ptr <- .Call("wk_minus_dm_new", period, PACKAGE = "wickra")
  .wk_obj("minus_dm", ptr, "MinusDm")
}

#' ModifiedMaStop indicator
#' @keywords internal
#' @export
ModifiedMaStop <- function(period) {
  ptr <- .Call("wk_modified_ma_stop_new", period, PACKAGE = "wickra")
  .wk_obj("modified_ma_stop", ptr, "ModifiedMaStop")
}

#' Mom indicator
#' @keywords internal
#' @export
Mom <- function(period) {
  ptr <- .Call("wk_mom_new", period, PACKAGE = "wickra")
  .wk_obj("mom", ptr, "Mom")
}

#' MorningDojiStar indicator
#' @keywords internal
#' @export
MorningDojiStar <- function() {
  ptr <- .Call("wk_morning_doji_star_new", PACKAGE = "wickra")
  .wk_obj("morning_doji_star", ptr, "MorningDojiStar")
}

#' MorningEveningStar indicator
#' @keywords internal
#' @export
MorningEveningStar <- function() {
  ptr <- .Call("wk_morning_evening_star_new", PACKAGE = "wickra")
  .wk_obj("morning_evening_star", ptr, "MorningEveningStar")
}

#' MurreyMathLines indicator
#' @keywords internal
#' @export
MurreyMathLines <- function(period) {
  ptr <- .Call("wk_murrey_math_lines_new", period, PACKAGE = "wickra")
  .wk_obj("murrey_math_lines", ptr, "MurreyMathLines")
}

#' NakedPoc indicator
#' @keywords internal
#' @export
NakedPoc <- function(session_len, bins) {
  ptr <- .Call("wk_naked_poc_new", session_len, bins, PACKAGE = "wickra")
  .wk_obj("naked_poc", ptr, "NakedPoc")
}

#' Natr indicator
#' @keywords internal
#' @export
Natr <- function(period) {
  ptr <- .Call("wk_natr_new", period, PACKAGE = "wickra")
  .wk_obj("natr", ptr, "Natr")
}

#' NewHighsNewLows indicator
#' @keywords internal
#' @export
NewHighsNewLows <- function() {
  ptr <- .Call("wk_new_highs_new_lows_new", PACKAGE = "wickra")
  .wk_obj("new_highs_new_lows", ptr, "NewHighsNewLows")
}

#' NewPriceLines indicator
#' @keywords internal
#' @export
NewPriceLines <- function(count) {
  ptr <- .Call("wk_new_price_lines_new", count, PACKAGE = "wickra")
  .wk_obj("new_price_lines", ptr, "NewPriceLines")
}

#' Nrtr indicator
#' @keywords internal
#' @export
Nrtr <- function(pct) {
  ptr <- .Call("wk_nrtr_new", pct, PACKAGE = "wickra")
  .wk_obj("nrtr", ptr, "Nrtr")
}

#' Nvi indicator
#' @keywords internal
#' @export
Nvi <- function() {
  ptr <- .Call("wk_nvi_new", PACKAGE = "wickra")
  .wk_obj("nvi", ptr, "Nvi")
}

#' Obv indicator
#' @keywords internal
#' @export
Obv <- function() {
  ptr <- .Call("wk_obv_new", PACKAGE = "wickra")
  .wk_obj("obv", ptr, "Obv")
}

#' OIPriceDivergence indicator
#' @keywords internal
#' @export
OIPriceDivergence <- function(window) {
  ptr <- .Call("wk_oi_price_divergence_new", window, PACKAGE = "wickra")
  .wk_obj("oi_price_divergence", ptr, "OIPriceDivergence")
}

#' OiToVolumeRatio indicator
#' @keywords internal
#' @export
OiToVolumeRatio <- function() {
  ptr <- .Call("wk_oi_to_volume_ratio_new", PACKAGE = "wickra")
  .wk_obj("oi_to_volume_ratio", ptr, "OiToVolumeRatio")
}

#' OIWeighted indicator
#' @keywords internal
#' @export
OIWeighted <- function() {
  ptr <- .Call("wk_oi_weighted_new", PACKAGE = "wickra")
  .wk_obj("oi_weighted", ptr, "OIWeighted")
}

#' OmegaRatio indicator
#' @keywords internal
#' @export
OmegaRatio <- function(period, threshold) {
  ptr <- .Call("wk_omega_ratio_new", period, threshold, PACKAGE = "wickra")
  .wk_obj("omega_ratio", ptr, "OmegaRatio")
}

#' OnNeck indicator
#' @keywords internal
#' @export
OnNeck <- function() {
  ptr <- .Call("wk_on_neck_new", PACKAGE = "wickra")
  .wk_obj("on_neck", ptr, "OnNeck")
}

#' OpenInterestDelta indicator
#' @keywords internal
#' @export
OpenInterestDelta <- function() {
  ptr <- .Call("wk_open_interest_delta_new", PACKAGE = "wickra")
  .wk_obj("open_interest_delta", ptr, "OpenInterestDelta")
}

#' OpenInterestMomentum indicator
#' @keywords internal
#' @export
OpenInterestMomentum <- function(period) {
  ptr <- .Call("wk_open_interest_momentum_new", period, PACKAGE = "wickra")
  .wk_obj("open_interest_momentum", ptr, "OpenInterestMomentum")
}

#' OpeningMarubozu indicator
#' @keywords internal
#' @export
OpeningMarubozu <- function() {
  ptr <- .Call("wk_opening_marubozu_new", PACKAGE = "wickra")
  .wk_obj("opening_marubozu", ptr, "OpeningMarubozu")
}

#' OpeningRange indicator
#' @keywords internal
#' @export
OpeningRange <- function(period) {
  ptr <- .Call("wk_opening_range_new", period, PACKAGE = "wickra")
  .wk_obj("opening_range", ptr, "OpeningRange")
}

#' OrderBookImbalanceFull indicator
#' @keywords internal
#' @export
OrderBookImbalanceFull <- function() {
  ptr <- .Call("wk_order_book_imbalance_full_new", PACKAGE = "wickra")
  .wk_obj("order_book_imbalance_full", ptr, "OrderBookImbalanceFull")
}

#' OrderBookImbalanceTop1 indicator
#' @keywords internal
#' @export
OrderBookImbalanceTop1 <- function() {
  ptr <- .Call("wk_order_book_imbalance_top1_new", PACKAGE = "wickra")
  .wk_obj("order_book_imbalance_top1", ptr, "OrderBookImbalanceTop1")
}

#' OrderBookImbalanceTopN indicator
#' @keywords internal
#' @export
OrderBookImbalanceTopN <- function(levels) {
  ptr <- .Call("wk_order_book_imbalance_top_n_new", levels, PACKAGE = "wickra")
  .wk_obj("order_book_imbalance_top_n", ptr, "OrderBookImbalanceTopN")
}

#' OrderFlowImbalance indicator
#' @keywords internal
#' @export
OrderFlowImbalance <- function(period) {
  ptr <- .Call("wk_order_flow_imbalance_new", period, PACKAGE = "wickra")
  .wk_obj("order_flow_imbalance", ptr, "OrderFlowImbalance")
}

#' OuHalfLife indicator
#' @keywords internal
#' @export
OuHalfLife <- function(period) {
  ptr <- .Call("wk_ou_half_life_new", period, PACKAGE = "wickra")
  .wk_obj("ou_half_life", ptr, "OuHalfLife")
}

#' OvernightGap indicator
#' @keywords internal
#' @export
OvernightGap <- function(utc_offset_minutes) {
  ptr <- .Call("wk_overnight_gap_new", utc_offset_minutes, PACKAGE = "wickra")
  .wk_obj("overnight_gap", ptr, "OvernightGap")
}

#' OvernightIntradayReturn indicator
#' @keywords internal
#' @export
OvernightIntradayReturn <- function(utc_offset_minutes) {
  ptr <- .Call("wk_overnight_intraday_return_new", utc_offset_minutes, PACKAGE = "wickra")
  .wk_obj("overnight_intraday_return", ptr, "OvernightIntradayReturn")
}

#' PainIndex indicator
#' @keywords internal
#' @export
PainIndex <- function(period) {
  ptr <- .Call("wk_pain_index_new", period, PACKAGE = "wickra")
  .wk_obj("pain_index", ptr, "PainIndex")
}

#' PairSpreadZScore indicator
#' @keywords internal
#' @export
PairSpreadZScore <- function(beta_period, z_period) {
  ptr <- .Call("wk_pair_spread_z_score_new", beta_period, z_period, PACKAGE = "wickra")
  .wk_obj("pair_spread_z_score", ptr, "PairSpreadZScore")
}

#' PairwiseBeta indicator
#' @keywords internal
#' @export
PairwiseBeta <- function(period) {
  ptr <- .Call("wk_pairwise_beta_new", period, PACKAGE = "wickra")
  .wk_obj("pairwise_beta", ptr, "PairwiseBeta")
}

#' ParkinsonVolatility indicator
#' @keywords internal
#' @export
ParkinsonVolatility <- function(period, trading_periods) {
  ptr <- .Call("wk_parkinson_volatility_new", period, trading_periods, PACKAGE = "wickra")
  .wk_obj("parkinson_volatility", ptr, "ParkinsonVolatility")
}

#' PearsonCorrelation indicator
#' @keywords internal
#' @export
PearsonCorrelation <- function(period) {
  ptr <- .Call("wk_pearson_correlation_new", period, PACKAGE = "wickra")
  .wk_obj("pearson_correlation", ptr, "PearsonCorrelation")
}

#' PercentAboveMa indicator
#' @keywords internal
#' @export
PercentAboveMa <- function() {
  ptr <- .Call("wk_percent_above_ma_new", PACKAGE = "wickra")
  .wk_obj("percent_above_ma", ptr, "PercentAboveMa")
}

#' PercentB indicator
#' @keywords internal
#' @export
PercentB <- function(period, multiplier) {
  ptr <- .Call("wk_percent_b_new", period, multiplier, PACKAGE = "wickra")
  .wk_obj("percent_b", ptr, "PercentB")
}

#' PercentageTrailingStop indicator
#' @keywords internal
#' @export
PercentageTrailingStop <- function(percent) {
  ptr <- .Call("wk_percentage_trailing_stop_new", percent, PACKAGE = "wickra")
  .wk_obj("percentage_trailing_stop", ptr, "PercentageTrailingStop")
}

#' PerpetualPremiumIndex indicator
#' @keywords internal
#' @export
PerpetualPremiumIndex <- function() {
  ptr <- .Call("wk_perpetual_premium_index_new", PACKAGE = "wickra")
  .wk_obj("perpetual_premium_index", ptr, "PerpetualPremiumIndex")
}

#' Pgo indicator
#' @keywords internal
#' @export
Pgo <- function(period) {
  ptr <- .Call("wk_pgo_new", period, PACKAGE = "wickra")
  .wk_obj("pgo", ptr, "Pgo")
}

#' PiercingDarkCloud indicator
#' @keywords internal
#' @export
PiercingDarkCloud <- function() {
  ptr <- .Call("wk_piercing_dark_cloud_new", PACKAGE = "wickra")
  .wk_obj("piercing_dark_cloud", ptr, "PiercingDarkCloud")
}

#' Pin indicator
#' @keywords internal
#' @export
Pin <- function(window) {
  ptr <- .Call("wk_pin_new", window, PACKAGE = "wickra")
  .wk_obj("pin", ptr, "Pin")
}

#' PivotReversal indicator
#' @keywords internal
#' @export
PivotReversal <- function(left, right) {
  ptr <- .Call("wk_pivot_reversal_new", left, right, PACKAGE = "wickra")
  .wk_obj("pivot_reversal", ptr, "PivotReversal")
}

#' PlusDi indicator
#' @keywords internal
#' @export
PlusDi <- function(period) {
  ptr <- .Call("wk_plus_di_new", period, PACKAGE = "wickra")
  .wk_obj("plus_di", ptr, "PlusDi")
}

#' PlusDm indicator
#' @keywords internal
#' @export
PlusDm <- function(period) {
  ptr <- .Call("wk_plus_dm_new", period, PACKAGE = "wickra")
  .wk_obj("plus_dm", ptr, "PlusDm")
}

#' Pmo indicator
#' @keywords internal
#' @export
Pmo <- function(smoothing1, smoothing2) {
  ptr <- .Call("wk_pmo_new", smoothing1, smoothing2, PACKAGE = "wickra")
  .wk_obj("pmo", ptr, "Pmo")
}

#' PointAndFigureBars indicator
#' @keywords internal
#' @export
PointAndFigureBars <- function(box_size, reversal) {
  ptr <- .Call("wk_point_and_figure_bars_new", box_size, reversal, PACKAGE = "wickra")
  .wk_obj("point_and_figure_bars", ptr, "PointAndFigureBars")
}

#' PolarizedFractalEfficiency indicator
#' @keywords internal
#' @export
PolarizedFractalEfficiency <- function(period, smoothing) {
  ptr <- .Call("wk_polarized_fractal_efficiency_new", period, smoothing, PACKAGE = "wickra")
  .wk_obj("polarized_fractal_efficiency", ptr, "PolarizedFractalEfficiency")
}

#' Ppo indicator
#' @keywords internal
#' @export
Ppo <- function(fast, slow) {
  ptr <- .Call("wk_ppo_new", fast, slow, PACKAGE = "wickra")
  .wk_obj("ppo", ptr, "Ppo")
}

#' PpoHistogram indicator
#' @keywords internal
#' @export
PpoHistogram <- function(fast, slow, signal) {
  ptr <- .Call("wk_ppo_histogram_new", fast, slow, signal, PACKAGE = "wickra")
  .wk_obj("ppo_histogram", ptr, "PpoHistogram")
}

#' ProfileShape indicator
#' @keywords internal
#' @export
ProfileShape <- function(period, bins) {
  ptr <- .Call("wk_profile_shape_new", period, bins, PACKAGE = "wickra")
  .wk_obj("profile_shape", ptr, "ProfileShape")
}

#' ProfitFactor indicator
#' @keywords internal
#' @export
ProfitFactor <- function(period) {
  ptr <- .Call("wk_profit_factor_new", period, PACKAGE = "wickra")
  .wk_obj("profit_factor", ptr, "ProfitFactor")
}

#' ProjectionBands indicator
#' @keywords internal
#' @export
ProjectionBands <- function(period) {
  ptr <- .Call("wk_projection_bands_new", period, PACKAGE = "wickra")
  .wk_obj("projection_bands", ptr, "ProjectionBands")
}

#' ProjectionOscillator indicator
#' @keywords internal
#' @export
ProjectionOscillator <- function(period) {
  ptr <- .Call("wk_projection_oscillator_new", period, PACKAGE = "wickra")
  .wk_obj("projection_oscillator", ptr, "ProjectionOscillator")
}

#' Psar indicator
#' @keywords internal
#' @export
Psar <- function(af_start, af_step, af_max) {
  ptr <- .Call("wk_psar_new", af_start, af_step, af_max, PACKAGE = "wickra")
  .wk_obj("psar", ptr, "Psar")
}

#' Pvi indicator
#' @keywords internal
#' @export
Pvi <- function() {
  ptr <- .Call("wk_pvi_new", PACKAGE = "wickra")
  .wk_obj("pvi", ptr, "Pvi")
}

#' Qqe indicator
#' @keywords internal
#' @export
Qqe <- function(rsi_period, smoothing, factor) {
  ptr <- .Call("wk_qqe_new", rsi_period, smoothing, factor, PACKAGE = "wickra")
  .wk_obj("qqe", ptr, "Qqe")
}

#' Qstick indicator
#' @keywords internal
#' @export
Qstick <- function(period) {
  ptr <- .Call("wk_qstick_new", period, PACKAGE = "wickra")
  .wk_obj("qstick", ptr, "Qstick")
}

#' QuartileBands indicator
#' @keywords internal
#' @export
QuartileBands <- function(period) {
  ptr <- .Call("wk_quartile_bands_new", period, PACKAGE = "wickra")
  .wk_obj("quartile_bands", ptr, "QuartileBands")
}

#' QuotedSpread indicator
#' @keywords internal
#' @export
QuotedSpread <- function() {
  ptr <- .Call("wk_quoted_spread_new", PACKAGE = "wickra")
  .wk_obj("quoted_spread", ptr, "QuotedSpread")
}

#' RSquared indicator
#' @keywords internal
#' @export
RSquared <- function(period) {
  ptr <- .Call("wk_r_squared_new", period, PACKAGE = "wickra")
  .wk_obj("r_squared", ptr, "RSquared")
}

#' RangeBars indicator
#' @keywords internal
#' @export
RangeBars <- function(range) {
  ptr <- .Call("wk_range_bars_new", range, PACKAGE = "wickra")
  .wk_obj("range_bars", ptr, "RangeBars")
}

#' RealizedSpread indicator
#' @keywords internal
#' @export
RealizedSpread <- function(horizon) {
  ptr <- .Call("wk_realized_spread_new", horizon, PACKAGE = "wickra")
  .wk_obj("realized_spread", ptr, "RealizedSpread")
}

#' RealizedVolatility indicator
#' @keywords internal
#' @export
RealizedVolatility <- function(period) {
  ptr <- .Call("wk_realized_volatility_new", period, PACKAGE = "wickra")
  .wk_obj("realized_volatility", ptr, "RealizedVolatility")
}

#' RecoveryFactor indicator
#' @keywords internal
#' @export
RecoveryFactor <- function() {
  ptr <- .Call("wk_recovery_factor_new", PACKAGE = "wickra")
  .wk_obj("recovery_factor", ptr, "RecoveryFactor")
}

#' RectangleRange indicator
#' @keywords internal
#' @export
RectangleRange <- function() {
  ptr <- .Call("wk_rectangle_range_new", PACKAGE = "wickra")
  .wk_obj("rectangle_range", ptr, "RectangleRange")
}

#' Reflex indicator
#' @keywords internal
#' @export
Reflex <- function(period) {
  ptr <- .Call("wk_reflex_new", period, PACKAGE = "wickra")
  .wk_obj("reflex", ptr, "Reflex")
}

#' RegimeLabel indicator
#' @keywords internal
#' @export
RegimeLabel <- function(vol_period, lookback) {
  ptr <- .Call("wk_regime_label_new", vol_period, lookback, PACKAGE = "wickra")
  .wk_obj("regime_label", ptr, "RegimeLabel")
}

#' RelativeStrengthAB indicator
#' @keywords internal
#' @export
RelativeStrengthAB <- function(ma_period, rsi_period) {
  ptr <- .Call("wk_relative_strength_ab_new", ma_period, rsi_period, PACKAGE = "wickra")
  .wk_obj("relative_strength_ab", ptr, "RelativeStrengthAB")
}

#' RenkoBars indicator
#' @keywords internal
#' @export
RenkoBars <- function(box_size) {
  ptr <- .Call("wk_renko_bars_new", box_size, PACKAGE = "wickra")
  .wk_obj("renko_bars", ptr, "RenkoBars")
}

#' RenkoTrailingStop indicator
#' @keywords internal
#' @export
RenkoTrailingStop <- function(block_size) {
  ptr <- .Call("wk_renko_trailing_stop_new", block_size, PACKAGE = "wickra")
  .wk_obj("renko_trailing_stop", ptr, "RenkoTrailingStop")
}

#' Resample candles to a higher timeframe
#'
#' Aggregates a stream of candles into higher-timeframe bars. Feed candles with
#' [update()] (it returns every bar that closed as a result, which is none while
#' the current bar is still open) and emit the final, still-open bar with
#' [flush()].
#'
#' @param timeframe Integer number of input candles per higher-timeframe bar.
#' @param gap_fill Logical. When `TRUE`, a flat placeholder bar
#'   (`open == high == low == close`, zero volume, carrying the close before the
#'   gap) is emitted for every output bucket no input candle fell into, so the
#'   series stays evenly spaced. Defaults to `FALSE`, which leaves holes.
#' @return A `wickra_indicator` resampler.
#' @keywords internal
#' @export
Resampler <- function(timeframe, gap_fill = FALSE) {
  ptr <- .Call("wk_resampler_new", timeframe, gap_fill, PACKAGE = "wickra")
  .wk_obj("resampler", ptr, "Resampler")
}

#' RickshawMan indicator
#' @keywords internal
#' @export
RickshawMan <- function() {
  ptr <- .Call("wk_rickshaw_man_new", PACKAGE = "wickra")
  .wk_obj("rickshaw_man", ptr, "RickshawMan")
}

#' RisingThreeMethods indicator
#' @keywords internal
#' @export
RisingThreeMethods <- function() {
  ptr <- .Call("wk_rising_three_methods_new", PACKAGE = "wickra")
  .wk_obj("rising_three_methods", ptr, "RisingThreeMethods")
}

#' Rmi indicator
#' @keywords internal
#' @export
Rmi <- function(period, momentum) {
  ptr <- .Call("wk_rmi_new", period, momentum, PACKAGE = "wickra")
  .wk_obj("rmi", ptr, "Rmi")
}

#' Roc indicator
#' @keywords internal
#' @export
Roc <- function(period) {
  ptr <- .Call("wk_roc_new", period, PACKAGE = "wickra")
  .wk_obj("roc", ptr, "Roc")
}

#' Rocp indicator
#' @keywords internal
#' @export
Rocp <- function(period) {
  ptr <- .Call("wk_rocp_new", period, PACKAGE = "wickra")
  .wk_obj("rocp", ptr, "Rocp")
}

#' Rocr indicator
#' @keywords internal
#' @export
Rocr <- function(period) {
  ptr <- .Call("wk_rocr_new", period, PACKAGE = "wickra")
  .wk_obj("rocr", ptr, "Rocr")
}

#' Rocr100 indicator
#' @keywords internal
#' @export
Rocr100 <- function(period) {
  ptr <- .Call("wk_rocr100_new", period, PACKAGE = "wickra")
  .wk_obj("rocr100", ptr, "Rocr100")
}

#' RogersSatchellVolatility indicator
#' @keywords internal
#' @export
RogersSatchellVolatility <- function(period, trading_periods) {
  ptr <- .Call("wk_rogers_satchell_volatility_new", period, trading_periods, PACKAGE = "wickra")
  .wk_obj("rogers_satchell_volatility", ptr, "RogersSatchellVolatility")
}

#' RollMeasure indicator
#' @keywords internal
#' @export
RollMeasure <- function(period) {
  ptr <- .Call("wk_roll_measure_new", period, PACKAGE = "wickra")
  .wk_obj("roll_measure", ptr, "RollMeasure")
}

#' RollingCorrelation indicator
#' @keywords internal
#' @export
RollingCorrelation <- function(period) {
  ptr <- .Call("wk_rolling_correlation_new", period, PACKAGE = "wickra")
  .wk_obj("rolling_correlation", ptr, "RollingCorrelation")
}

#' RollingCovariance indicator
#' @keywords internal
#' @export
RollingCovariance <- function(period) {
  ptr <- .Call("wk_rolling_covariance_new", period, PACKAGE = "wickra")
  .wk_obj("rolling_covariance", ptr, "RollingCovariance")
}

#' RollingIqr indicator
#' @keywords internal
#' @export
RollingIqr <- function(period) {
  ptr <- .Call("wk_rolling_iqr_new", period, PACKAGE = "wickra")
  .wk_obj("rolling_iqr", ptr, "RollingIqr")
}

#' RollingMinMaxScaler indicator
#' @keywords internal
#' @export
RollingMinMaxScaler <- function(period) {
  ptr <- .Call("wk_rolling_min_max_scaler_new", period, PACKAGE = "wickra")
  .wk_obj("rolling_min_max_scaler", ptr, "RollingMinMaxScaler")
}

#' RollingPercentileRank indicator
#' @keywords internal
#' @export
RollingPercentileRank <- function(period) {
  ptr <- .Call("wk_rolling_percentile_rank_new", period, PACKAGE = "wickra")
  .wk_obj("rolling_percentile_rank", ptr, "RollingPercentileRank")
}

#' RollingQuantile indicator
#' @keywords internal
#' @export
RollingQuantile <- function(period, quantile) {
  ptr <- .Call("wk_rolling_quantile_new", period, quantile, PACKAGE = "wickra")
  .wk_obj("rolling_quantile", ptr, "RollingQuantile")
}

#' RollingVwap indicator
#' @keywords internal
#' @export
RollingVwap <- function(period) {
  ptr <- .Call("wk_rolling_vwap_new", period, PACKAGE = "wickra")
  .wk_obj("rolling_vwap", ptr, "RollingVwap")
}

#' RoofingFilter indicator
#' @keywords internal
#' @export
RoofingFilter <- function(lp_period, hp_period) {
  ptr <- .Call("wk_roofing_filter_new", lp_period, hp_period, PACKAGE = "wickra")
  .wk_obj("roofing_filter", ptr, "RoofingFilter")
}

#' Rsi indicator
#' @keywords internal
#' @export
Rsi <- function(period) {
  ptr <- .Call("wk_rsi_new", period, PACKAGE = "wickra")
  .wk_obj("rsi", ptr, "Rsi")
}

#' Rsx indicator
#' @keywords internal
#' @export
Rsx <- function(length) {
  ptr <- .Call("wk_rsx_new", length, PACKAGE = "wickra")
  .wk_obj("rsx", ptr, "Rsx")
}

#' RunBars indicator
#' @keywords internal
#' @export
RunBars <- function(run_length) {
  ptr <- .Call("wk_run_bars_new", run_length, PACKAGE = "wickra")
  .wk_obj("run_bars", ptr, "RunBars")
}

#' RunningDrawdown indicator
#' @keywords internal
#' @export
RunningDrawdown <- function() {
  ptr <- .Call("wk_running_drawdown_new", PACKAGE = "wickra")
  .wk_obj("running_drawdown", ptr, "RunningDrawdown")
}

#' Rvi indicator
#' @keywords internal
#' @export
Rvi <- function(period) {
  ptr <- .Call("wk_rvi_new", period, PACKAGE = "wickra")
  .wk_obj("rvi", ptr, "Rvi")
}

#' RviVolatility indicator
#' @keywords internal
#' @export
RviVolatility <- function(period) {
  ptr <- .Call("wk_rvi_volatility_new", period, PACKAGE = "wickra")
  .wk_obj("rvi_volatility", ptr, "RviVolatility")
}

#' Rwi indicator
#' @keywords internal
#' @export
Rwi <- function(period) {
  ptr <- .Call("wk_rwi_new", period, PACKAGE = "wickra")
  .wk_obj("rwi", ptr, "Rwi")
}

#' SampleEntropy indicator
#' @keywords internal
#' @export
SampleEntropy <- function(period, m, r_factor) {
  ptr <- .Call("wk_sample_entropy_new", period, m, r_factor, PACKAGE = "wickra")
  .wk_obj("sample_entropy", ptr, "SampleEntropy")
}

#' SarExt indicator
#' @keywords internal
#' @export
SarExt <- function(start_value, offset_on_reverse, accel_init_long, accel_long, accel_max_long, accel_init_short, accel_short, accel_max_short) {
  ptr <- .Call("wk_sar_ext_new", start_value, offset_on_reverse, accel_init_long, accel_long, accel_max_long, accel_init_short, accel_short, accel_max_short, PACKAGE = "wickra")
  .wk_obj("sar_ext", ptr, "SarExt")
}

#' SeasonalZScore indicator
#' @keywords internal
#' @export
SeasonalZScore <- function(utc_offset_minutes) {
  ptr <- .Call("wk_seasonal_z_score_new", utc_offset_minutes, PACKAGE = "wickra")
  .wk_obj("seasonal_z_score", ptr, "SeasonalZScore")
}

#' SeparatingLines indicator
#' @keywords internal
#' @export
SeparatingLines <- function() {
  ptr <- .Call("wk_separating_lines_new", PACKAGE = "wickra")
  .wk_obj("separating_lines", ptr, "SeparatingLines")
}

#' SessionHighLow indicator
#' @keywords internal
#' @export
SessionHighLow <- function(utc_offset_minutes) {
  ptr <- .Call("wk_session_high_low_new", utc_offset_minutes, PACKAGE = "wickra")
  .wk_obj("session_high_low", ptr, "SessionHighLow")
}

#' SessionRange indicator
#' @keywords internal
#' @export
SessionRange <- function(utc_offset_minutes) {
  ptr <- .Call("wk_session_range_new", utc_offset_minutes, PACKAGE = "wickra")
  .wk_obj("session_range", ptr, "SessionRange")
}

#' SessionVwap indicator
#' @keywords internal
#' @export
SessionVwap <- function(utc_offset_minutes) {
  ptr <- .Call("wk_session_vwap_new", utc_offset_minutes, PACKAGE = "wickra")
  .wk_obj("session_vwap", ptr, "SessionVwap")
}

#' ShannonEntropy indicator
#' @keywords internal
#' @export
ShannonEntropy <- function(period, bins) {
  ptr <- .Call("wk_shannon_entropy_new", period, bins, PACKAGE = "wickra")
  .wk_obj("shannon_entropy", ptr, "ShannonEntropy")
}

#' Shark indicator
#' @keywords internal
#' @export
Shark <- function() {
  ptr <- .Call("wk_shark_new", PACKAGE = "wickra")
  .wk_obj("shark", ptr, "Shark")
}

#' SharpeRatio indicator
#' @keywords internal
#' @export
SharpeRatio <- function(period, risk_free) {
  ptr <- .Call("wk_sharpe_ratio_new", period, risk_free, PACKAGE = "wickra")
  .wk_obj("sharpe_ratio", ptr, "SharpeRatio")
}

#' ShootingStar indicator
#' @keywords internal
#' @export
ShootingStar <- function() {
  ptr <- .Call("wk_shooting_star_new", PACKAGE = "wickra")
  .wk_obj("shooting_star", ptr, "ShootingStar")
}

#' ShortLine indicator
#' @keywords internal
#' @export
ShortLine <- function() {
  ptr <- .Call("wk_short_line_new", PACKAGE = "wickra")
  .wk_obj("short_line", ptr, "ShortLine")
}

#' SignedVolume indicator
#' @keywords internal
#' @export
SignedVolume <- function() {
  ptr <- .Call("wk_signed_volume_new", PACKAGE = "wickra")
  .wk_obj("signed_volume", ptr, "SignedVolume")
}

#' SineWave indicator
#' @keywords internal
#' @export
SineWave <- function() {
  ptr <- .Call("wk_sine_wave_new", PACKAGE = "wickra")
  .wk_obj("sine_wave", ptr, "SineWave")
}

#' SineWeightedMa indicator
#' @keywords internal
#' @export
SineWeightedMa <- function(period) {
  ptr <- .Call("wk_sine_weighted_ma_new", period, PACKAGE = "wickra")
  .wk_obj("sine_weighted_ma", ptr, "SineWeightedMa")
}

#' SinglePrints indicator
#' @keywords internal
#' @export
SinglePrints <- function(period, bins) {
  ptr <- .Call("wk_single_prints_new", period, bins, PACKAGE = "wickra")
  .wk_obj("single_prints", ptr, "SinglePrints")
}

#' Skewness indicator
#' @keywords internal
#' @export
Skewness <- function(period) {
  ptr <- .Call("wk_skewness_new", period, PACKAGE = "wickra")
  .wk_obj("skewness", ptr, "Skewness")
}

#' Sma indicator
#' @keywords internal
#' @export
Sma <- function(period) {
  ptr <- .Call("wk_sma_new", period, PACKAGE = "wickra")
  .wk_obj("sma", ptr, "Sma")
}

#' Smi indicator
#' @keywords internal
#' @export
Smi <- function(period, d_period, d2_period) {
  ptr <- .Call("wk_smi_new", period, d_period, d2_period, PACKAGE = "wickra")
  .wk_obj("smi", ptr, "Smi")
}

#' Smma indicator
#' @keywords internal
#' @export
Smma <- function(period) {
  ptr <- .Call("wk_smma_new", period, PACKAGE = "wickra")
  .wk_obj("smma", ptr, "Smma")
}

#' SmoothedHeikinAshi indicator
#' @keywords internal
#' @export
SmoothedHeikinAshi <- function(period) {
  ptr <- .Call("wk_smoothed_heikin_ashi_new", period, PACKAGE = "wickra")
  .wk_obj("smoothed_heikin_ashi", ptr, "SmoothedHeikinAshi")
}

#' SortinoRatio indicator
#' @keywords internal
#' @export
SortinoRatio <- function(period, mar) {
  ptr <- .Call("wk_sortino_ratio_new", period, mar, PACKAGE = "wickra")
  .wk_obj("sortino_ratio", ptr, "SortinoRatio")
}

#' SpearmanCorrelation indicator
#' @keywords internal
#' @export
SpearmanCorrelation <- function(period) {
  ptr <- .Call("wk_spearman_correlation_new", period, PACKAGE = "wickra")
  .wk_obj("spearman_correlation", ptr, "SpearmanCorrelation")
}

#' SpinningTop indicator
#' @keywords internal
#' @export
SpinningTop <- function() {
  ptr <- .Call("wk_spinning_top_new", PACKAGE = "wickra")
  .wk_obj("spinning_top", ptr, "SpinningTop")
}

#' SpreadAr1Coefficient indicator
#' @keywords internal
#' @export
SpreadAr1Coefficient <- function(period) {
  ptr <- .Call("wk_spread_ar1_coefficient_new", period, PACKAGE = "wickra")
  .wk_obj("spread_ar1_coefficient", ptr, "SpreadAr1Coefficient")
}

#' SpreadBollingerBands indicator
#' @keywords internal
#' @export
SpreadBollingerBands <- function(period, num_std) {
  ptr <- .Call("wk_spread_bollinger_bands_new", period, num_std, PACKAGE = "wickra")
  .wk_obj("spread_bollinger_bands", ptr, "SpreadBollingerBands")
}

#' SpreadHurst indicator
#' @keywords internal
#' @export
SpreadHurst <- function(period) {
  ptr <- .Call("wk_spread_hurst_new", period, PACKAGE = "wickra")
  .wk_obj("spread_hurst", ptr, "SpreadHurst")
}

#' StalledPattern indicator
#' @keywords internal
#' @export
StalledPattern <- function() {
  ptr <- .Call("wk_stalled_pattern_new", PACKAGE = "wickra")
  .wk_obj("stalled_pattern", ptr, "StalledPattern")
}

#' StandardError indicator
#' @keywords internal
#' @export
StandardError <- function(period) {
  ptr <- .Call("wk_standard_error_new", period, PACKAGE = "wickra")
  .wk_obj("standard_error", ptr, "StandardError")
}

#' StandardErrorBands indicator
#' @keywords internal
#' @export
StandardErrorBands <- function(period, multiplier) {
  ptr <- .Call("wk_standard_error_bands_new", period, multiplier, PACKAGE = "wickra")
  .wk_obj("standard_error_bands", ptr, "StandardErrorBands")
}

#' StarcBands indicator
#' @keywords internal
#' @export
StarcBands <- function(sma_period, atr_period, multiplier) {
  ptr <- .Call("wk_starc_bands_new", sma_period, atr_period, multiplier, PACKAGE = "wickra")
  .wk_obj("starc_bands", ptr, "StarcBands")
}

#' Stc indicator
#' @keywords internal
#' @export
Stc <- function(fast, slow, schaff_period, factor) {
  ptr <- .Call("wk_stc_new", fast, slow, schaff_period, factor, PACKAGE = "wickra")
  .wk_obj("stc", ptr, "Stc")
}

#' StdDev indicator
#' @keywords internal
#' @export
StdDev <- function(period) {
  ptr <- .Call("wk_std_dev_new", period, PACKAGE = "wickra")
  .wk_obj("std_dev", ptr, "StdDev")
}

#' StepTrailingStop indicator
#' @keywords internal
#' @export
StepTrailingStop <- function(step_size) {
  ptr <- .Call("wk_step_trailing_stop_new", step_size, PACKAGE = "wickra")
  .wk_obj("step_trailing_stop", ptr, "StepTrailingStop")
}

#' SterlingRatio indicator
#' @keywords internal
#' @export
SterlingRatio <- function(period) {
  ptr <- .Call("wk_sterling_ratio_new", period, PACKAGE = "wickra")
  .wk_obj("sterling_ratio", ptr, "SterlingRatio")
}

#' StickSandwich indicator
#' @keywords internal
#' @export
StickSandwich <- function() {
  ptr <- .Call("wk_stick_sandwich_new", PACKAGE = "wickra")
  .wk_obj("stick_sandwich", ptr, "StickSandwich")
}

#' StochRsi indicator
#' @keywords internal
#' @export
StochRsi <- function(rsi_period, stoch_period) {
  ptr <- .Call("wk_stoch_rsi_new", rsi_period, stoch_period, PACKAGE = "wickra")
  .wk_obj("stoch_rsi", ptr, "StochRsi")
}

#' Stochastic indicator
#' @keywords internal
#' @export
Stochastic <- function(k_period, d_period) {
  ptr <- .Call("wk_stochastic_new", k_period, d_period, PACKAGE = "wickra")
  .wk_obj("stochastic", ptr, "Stochastic")
}

#' StochasticCci indicator
#' @keywords internal
#' @export
StochasticCci <- function(period) {
  ptr <- .Call("wk_stochastic_cci_new", period, PACKAGE = "wickra")
  .wk_obj("stochastic_cci", ptr, "StochasticCci")
}

#' SuperSmoother indicator
#' @keywords internal
#' @export
SuperSmoother <- function(period) {
  ptr <- .Call("wk_super_smoother_new", period, PACKAGE = "wickra")
  .wk_obj("super_smoother", ptr, "SuperSmoother")
}

#' SuperTrend indicator
#' @keywords internal
#' @export
SuperTrend <- function(atr_period, multiplier) {
  ptr <- .Call("wk_super_trend_new", atr_period, multiplier, PACKAGE = "wickra")
  .wk_obj("super_trend", ptr, "SuperTrend")
}

#' T3 indicator
#' @keywords internal
#' @export
T3 <- function(period, v) {
  ptr <- .Call("wk_t3_new", period, v, PACKAGE = "wickra")
  .wk_obj("t3", ptr, "T3")
}

#' TailRatio indicator
#' @keywords internal
#' @export
TailRatio <- function(period) {
  ptr <- .Call("wk_tail_ratio_new", period, PACKAGE = "wickra")
  .wk_obj("tail_ratio", ptr, "TailRatio")
}

#' TakerBuySellRatio indicator
#' @keywords internal
#' @export
TakerBuySellRatio <- function() {
  ptr <- .Call("wk_taker_buy_sell_ratio_new", PACKAGE = "wickra")
  .wk_obj("taker_buy_sell_ratio", ptr, "TakerBuySellRatio")
}

#' Takuri indicator
#' @keywords internal
#' @export
Takuri <- function() {
  ptr <- .Call("wk_takuri_new", PACKAGE = "wickra")
  .wk_obj("takuri", ptr, "Takuri")
}

#' TasukiGap indicator
#' @keywords internal
#' @export
TasukiGap <- function() {
  ptr <- .Call("wk_tasuki_gap_new", PACKAGE = "wickra")
  .wk_obj("tasuki_gap", ptr, "TasukiGap")
}

#' TdCamouflage indicator
#' @keywords internal
#' @export
TdCamouflage <- function() {
  ptr <- .Call("wk_td_camouflage_new", PACKAGE = "wickra")
  .wk_obj("td_camouflage", ptr, "TdCamouflage")
}

#' TdClop indicator
#' @keywords internal
#' @export
TdClop <- function() {
  ptr <- .Call("wk_td_clop_new", PACKAGE = "wickra")
  .wk_obj("td_clop", ptr, "TdClop")
}

#' TdClopwin indicator
#' @keywords internal
#' @export
TdClopwin <- function() {
  ptr <- .Call("wk_td_clopwin_new", PACKAGE = "wickra")
  .wk_obj("td_clopwin", ptr, "TdClopwin")
}

#' TdCombo indicator
#' @keywords internal
#' @export
TdCombo <- function(setup_lookback, setup_target, countdown_lookback, countdown_target) {
  ptr <- .Call("wk_td_combo_new", setup_lookback, setup_target, countdown_lookback, countdown_target, PACKAGE = "wickra")
  .wk_obj("td_combo", ptr, "TdCombo")
}

#' TdCountdown indicator
#' @keywords internal
#' @export
TdCountdown <- function(setup_lookback, setup_target, countdown_lookback, countdown_target) {
  ptr <- .Call("wk_td_countdown_new", setup_lookback, setup_target, countdown_lookback, countdown_target, PACKAGE = "wickra")
  .wk_obj("td_countdown", ptr, "TdCountdown")
}

#' TdDWave indicator
#' @keywords internal
#' @export
TdDWave <- function(strength) {
  ptr <- .Call("wk_td_d_wave_new", strength, PACKAGE = "wickra")
  .wk_obj("td_d_wave", ptr, "TdDWave")
}

#' TdDeMarker indicator
#' @keywords internal
#' @export
TdDeMarker <- function(period) {
  ptr <- .Call("wk_td_de_marker_new", period, PACKAGE = "wickra")
  .wk_obj("td_de_marker", ptr, "TdDeMarker")
}

#' TdDifferential indicator
#' @keywords internal
#' @export
TdDifferential <- function() {
  ptr <- .Call("wk_td_differential_new", PACKAGE = "wickra")
  .wk_obj("td_differential", ptr, "TdDifferential")
}

#' TdLines indicator
#' @keywords internal
#' @export
TdLines <- function(lookback, target) {
  ptr <- .Call("wk_td_lines_new", lookback, target, PACKAGE = "wickra")
  .wk_obj("td_lines", ptr, "TdLines")
}

#' TdMovingAverage indicator
#' @keywords internal
#' @export
TdMovingAverage <- function(period_st1, period_st2) {
  ptr <- .Call("wk_td_moving_average_new", period_st1, period_st2, PACKAGE = "wickra")
  .wk_obj("td_moving_average", ptr, "TdMovingAverage")
}

#' TdOpen indicator
#' @keywords internal
#' @export
TdOpen <- function() {
  ptr <- .Call("wk_td_open_new", PACKAGE = "wickra")
  .wk_obj("td_open", ptr, "TdOpen")
}

#' TdPressure indicator
#' @keywords internal
#' @export
TdPressure <- function(period) {
  ptr <- .Call("wk_td_pressure_new", period, PACKAGE = "wickra")
  .wk_obj("td_pressure", ptr, "TdPressure")
}

#' TdPropulsion indicator
#' @keywords internal
#' @export
TdPropulsion <- function() {
  ptr <- .Call("wk_td_propulsion_new", PACKAGE = "wickra")
  .wk_obj("td_propulsion", ptr, "TdPropulsion")
}

#' TdRangeProjection indicator
#' @keywords internal
#' @export
TdRangeProjection <- function() {
  ptr <- .Call("wk_td_range_projection_new", PACKAGE = "wickra")
  .wk_obj("td_range_projection", ptr, "TdRangeProjection")
}

#' TdRei indicator
#' @keywords internal
#' @export
TdRei <- function(period) {
  ptr <- .Call("wk_td_rei_new", period, PACKAGE = "wickra")
  .wk_obj("td_rei", ptr, "TdRei")
}

#' TdRiskLevel indicator
#' @keywords internal
#' @export
TdRiskLevel <- function(lookback, target) {
  ptr <- .Call("wk_td_risk_level_new", lookback, target, PACKAGE = "wickra")
  .wk_obj("td_risk_level", ptr, "TdRiskLevel")
}

#' TdSequential indicator
#' @keywords internal
#' @export
TdSequential <- function(setup_lookback, setup_target, countdown_lookback, countdown_target) {
  ptr <- .Call("wk_td_sequential_new", setup_lookback, setup_target, countdown_lookback, countdown_target, PACKAGE = "wickra")
  .wk_obj("td_sequential", ptr, "TdSequential")
}

#' TdSetup indicator
#' @keywords internal
#' @export
TdSetup <- function(lookback, target) {
  ptr <- .Call("wk_td_setup_new", lookback, target, PACKAGE = "wickra")
  .wk_obj("td_setup", ptr, "TdSetup")
}

#' TdTrap indicator
#' @keywords internal
#' @export
TdTrap <- function() {
  ptr <- .Call("wk_td_trap_new", PACKAGE = "wickra")
  .wk_obj("td_trap", ptr, "TdTrap")
}

#' Tema indicator
#' @keywords internal
#' @export
Tema <- function(period) {
  ptr <- .Call("wk_tema_new", period, PACKAGE = "wickra")
  .wk_obj("tema", ptr, "Tema")
}

#' TermStructureBasis indicator
#' @keywords internal
#' @export
TermStructureBasis <- function() {
  ptr <- .Call("wk_term_structure_basis_new", PACKAGE = "wickra")
  .wk_obj("term_structure_basis", ptr, "TermStructureBasis")
}

#' ThreeDrives indicator
#' @keywords internal
#' @export
ThreeDrives <- function() {
  ptr <- .Call("wk_three_drives_new", PACKAGE = "wickra")
  .wk_obj("three_drives", ptr, "ThreeDrives")
}

#' ThreeInside indicator
#' @keywords internal
#' @export
ThreeInside <- function() {
  ptr <- .Call("wk_three_inside_new", PACKAGE = "wickra")
  .wk_obj("three_inside", ptr, "ThreeInside")
}

#' ThreeLineBreak indicator
#' @keywords internal
#' @export
ThreeLineBreak <- function(lines) {
  ptr <- .Call("wk_three_line_break_new", lines, PACKAGE = "wickra")
  .wk_obj("three_line_break", ptr, "ThreeLineBreak")
}

#' ThreeLineBreakBars indicator
#' @keywords internal
#' @export
ThreeLineBreakBars <- function(lines) {
  ptr <- .Call("wk_three_line_break_bars_new", lines, PACKAGE = "wickra")
  .wk_obj("three_line_break_bars", ptr, "ThreeLineBreakBars")
}

#' ThreeLineStrike indicator
#' @keywords internal
#' @export
ThreeLineStrike <- function() {
  ptr <- .Call("wk_three_line_strike_new", PACKAGE = "wickra")
  .wk_obj("three_line_strike", ptr, "ThreeLineStrike")
}

#' ThreeOutside indicator
#' @keywords internal
#' @export
ThreeOutside <- function() {
  ptr <- .Call("wk_three_outside_new", PACKAGE = "wickra")
  .wk_obj("three_outside", ptr, "ThreeOutside")
}

#' ThreeSoldiersOrCrows indicator
#' @keywords internal
#' @export
ThreeSoldiersOrCrows <- function() {
  ptr <- .Call("wk_three_soldiers_or_crows_new", PACKAGE = "wickra")
  .wk_obj("three_soldiers_or_crows", ptr, "ThreeSoldiersOrCrows")
}

#' ThreeStarsInSouth indicator
#' @keywords internal
#' @export
ThreeStarsInSouth <- function() {
  ptr <- .Call("wk_three_stars_in_south_new", PACKAGE = "wickra")
  .wk_obj("three_stars_in_south", ptr, "ThreeStarsInSouth")
}

#' Thrusting indicator
#' @keywords internal
#' @export
Thrusting <- function() {
  ptr <- .Call("wk_thrusting_new", PACKAGE = "wickra")
  .wk_obj("thrusting", ptr, "Thrusting")
}

#' Aggregate trade ticks into time-bucketed candles
#'
#' Builds OHLCV candles from a stream of trade ticks. Feed ticks with [push()],
#' which returns the candles closed by that tick.
#'
#' @param bucket Time-bucket width, in the same unit as the tick timestamps.
#' @param gap_fill Logical; if `TRUE`, empty buckets with no trades are still
#'   emitted (carried forward) instead of skipped.
#' @return A `wickra_indicator` tick aggregator; feed it ticks with [push()].
#' @keywords internal
#' @export
TickAggregator <- function(bucket, gap_fill) {
  ptr <- .Call("wk_tick_aggregator_new", bucket, gap_fill, PACKAGE = "wickra")
  .wk_obj("tick_aggregator", ptr, "TickAggregator")
}

#' TickBars indicator
#' @keywords internal
#' @export
TickBars <- function(ticks) {
  ptr <- .Call("wk_tick_bars_new", ticks, PACKAGE = "wickra")
  .wk_obj("tick_bars", ptr, "TickBars")
}

#' TickIndex indicator
#' @keywords internal
#' @export
TickIndex <- function() {
  ptr <- .Call("wk_tick_index_new", PACKAGE = "wickra")
  .wk_obj("tick_index", ptr, "TickIndex")
}

#' Tii indicator
#' @keywords internal
#' @export
Tii <- function(sma_period, dev_period) {
  ptr <- .Call("wk_tii_new", sma_period, dev_period, PACKAGE = "wickra")
  .wk_obj("tii", ptr, "Tii")
}

#' TimeBasedStop indicator
#' @keywords internal
#' @export
TimeBasedStop <- function(max_bars) {
  ptr <- .Call("wk_time_based_stop_new", max_bars, PACKAGE = "wickra")
  .wk_obj("time_based_stop", ptr, "TimeBasedStop")
}

#' TimeOfDayReturnProfile indicator
#' @keywords internal
#' @export
TimeOfDayReturnProfile <- function(buckets, utc_offset_minutes) {
  ptr <- .Call("wk_time_of_day_return_profile_new", buckets, utc_offset_minutes, PACKAGE = "wickra")
  .wk_obj("time_of_day_return_profile", ptr, "TimeOfDayReturnProfile", values_cap = as.integer(buckets))
}

#' TowerTopBottom indicator
#' @keywords internal
#' @export
TowerTopBottom <- function() {
  ptr <- .Call("wk_tower_top_bottom_new", PACKAGE = "wickra")
  .wk_obj("tower_top_bottom", ptr, "TowerTopBottom")
}

#' TpoProfile indicator
#' @keywords internal
#' @export
TpoProfile <- function(period, bin_count) {
  ptr <- .Call("wk_tpo_profile_new", period, bin_count, PACKAGE = "wickra")
  .wk_obj("tpo_profile", ptr, "TpoProfile", values_cap = as.integer(bin_count))
}

#' TradeImbalance indicator
#' @keywords internal
#' @export
TradeImbalance <- function(window) {
  ptr <- .Call("wk_trade_imbalance_new", window, PACKAGE = "wickra")
  .wk_obj("trade_imbalance", ptr, "TradeImbalance")
}

#' TradeSignAutocorrelation indicator
#' @keywords internal
#' @export
TradeSignAutocorrelation <- function(period) {
  ptr <- .Call("wk_trade_sign_autocorrelation_new", period, PACKAGE = "wickra")
  .wk_obj("trade_sign_autocorrelation", ptr, "TradeSignAutocorrelation")
}

#' TradeVolumeIndex indicator
#' @keywords internal
#' @export
TradeVolumeIndex <- function(min_tick) {
  ptr <- .Call("wk_trade_volume_index_new", min_tick, PACKAGE = "wickra")
  .wk_obj("trade_volume_index", ptr, "TradeVolumeIndex")
}

#' TrendLabel indicator
#' @keywords internal
#' @export
TrendLabel <- function(period) {
  ptr <- .Call("wk_trend_label_new", period, PACKAGE = "wickra")
  .wk_obj("trend_label", ptr, "TrendLabel")
}

#' TrendStrengthIndex indicator
#' @keywords internal
#' @export
TrendStrengthIndex <- function(period) {
  ptr <- .Call("wk_trend_strength_index_new", period, PACKAGE = "wickra")
  .wk_obj("trend_strength_index", ptr, "TrendStrengthIndex")
}

#' Trendflex indicator
#' @keywords internal
#' @export
Trendflex <- function(period) {
  ptr <- .Call("wk_trendflex_new", period, PACKAGE = "wickra")
  .wk_obj("trendflex", ptr, "Trendflex")
}

#' TreynorRatio indicator
#' @keywords internal
#' @export
TreynorRatio <- function(period, risk_free) {
  ptr <- .Call("wk_treynor_ratio_new", period, risk_free, PACKAGE = "wickra")
  .wk_obj("treynor_ratio", ptr, "TreynorRatio")
}

#' Triangle indicator
#' @keywords internal
#' @export
Triangle <- function() {
  ptr <- .Call("wk_triangle_new", PACKAGE = "wickra")
  .wk_obj("triangle", ptr, "Triangle")
}

#' Trima indicator
#' @keywords internal
#' @export
Trima <- function(period) {
  ptr <- .Call("wk_trima_new", period, PACKAGE = "wickra")
  .wk_obj("trima", ptr, "Trima")
}

#' Trin indicator
#' @keywords internal
#' @export
Trin <- function() {
  ptr <- .Call("wk_trin_new", PACKAGE = "wickra")
  .wk_obj("trin", ptr, "Trin")
}

#' TripleTopBottom indicator
#' @keywords internal
#' @export
TripleTopBottom <- function() {
  ptr <- .Call("wk_triple_top_bottom_new", PACKAGE = "wickra")
  .wk_obj("triple_top_bottom", ptr, "TripleTopBottom")
}

#' Tristar indicator
#' @keywords internal
#' @export
Tristar <- function() {
  ptr <- .Call("wk_tristar_new", PACKAGE = "wickra")
  .wk_obj("tristar", ptr, "Tristar")
}

#' Trix indicator
#' @keywords internal
#' @export
Trix <- function(period) {
  ptr <- .Call("wk_trix_new", period, PACKAGE = "wickra")
  .wk_obj("trix", ptr, "Trix")
}

#' TrueRange indicator
#' @keywords internal
#' @export
TrueRange <- function() {
  ptr <- .Call("wk_true_range_new", PACKAGE = "wickra")
  .wk_obj("true_range", ptr, "TrueRange")
}

#' Tsf indicator
#' @keywords internal
#' @export
Tsf <- function(period) {
  ptr <- .Call("wk_tsf_new", period, PACKAGE = "wickra")
  .wk_obj("tsf", ptr, "Tsf")
}

#' TsfOscillator indicator
#' @keywords internal
#' @export
TsfOscillator <- function(period) {
  ptr <- .Call("wk_tsf_oscillator_new", period, PACKAGE = "wickra")
  .wk_obj("tsf_oscillator", ptr, "TsfOscillator")
}

#' Tsi indicator
#' @keywords internal
#' @export
Tsi <- function(long_, short_) {
  ptr <- .Call("wk_tsi_new", long_, short_, PACKAGE = "wickra")
  .wk_obj("tsi", ptr, "Tsi")
}

#' Tsv indicator
#' @keywords internal
#' @export
Tsv <- function(period) {
  ptr <- .Call("wk_tsv_new", period, PACKAGE = "wickra")
  .wk_obj("tsv", ptr, "Tsv")
}

#' TtmSqueeze indicator
#' @keywords internal
#' @export
TtmSqueeze <- function(period, bb_mult, kc_mult) {
  ptr <- .Call("wk_ttm_squeeze_new", period, bb_mult, kc_mult, PACKAGE = "wickra")
  .wk_obj("ttm_squeeze", ptr, "TtmSqueeze")
}

#' TtmTrend indicator
#' @keywords internal
#' @export
TtmTrend <- function(period) {
  ptr <- .Call("wk_ttm_trend_new", period, PACKAGE = "wickra")
  .wk_obj("ttm_trend", ptr, "TtmTrend")
}

#' TurnOfMonth indicator
#' @keywords internal
#' @export
TurnOfMonth <- function(n_first, n_last, utc_offset_minutes) {
  ptr <- .Call("wk_turn_of_month_new", n_first, n_last, utc_offset_minutes, PACKAGE = "wickra")
  .wk_obj("turn_of_month", ptr, "TurnOfMonth")
}

#' Tweezer indicator
#' @keywords internal
#' @export
Tweezer <- function() {
  ptr <- .Call("wk_tweezer_new", PACKAGE = "wickra")
  .wk_obj("tweezer", ptr, "Tweezer")
}

#' TwiggsMoneyFlow indicator
#' @keywords internal
#' @export
TwiggsMoneyFlow <- function(period) {
  ptr <- .Call("wk_twiggs_money_flow_new", period, PACKAGE = "wickra")
  .wk_obj("twiggs_money_flow", ptr, "TwiggsMoneyFlow")
}

#' TwoCrows indicator
#' @keywords internal
#' @export
TwoCrows <- function() {
  ptr <- .Call("wk_two_crows_new", PACKAGE = "wickra")
  .wk_obj("two_crows", ptr, "TwoCrows")
}

#' TypicalPrice indicator
#' @keywords internal
#' @export
TypicalPrice <- function() {
  ptr <- .Call("wk_typical_price_new", PACKAGE = "wickra")
  .wk_obj("typical_price", ptr, "TypicalPrice")
}

#' UlcerIndex indicator
#' @keywords internal
#' @export
UlcerIndex <- function(period) {
  ptr <- .Call("wk_ulcer_index_new", period, PACKAGE = "wickra")
  .wk_obj("ulcer_index", ptr, "UlcerIndex")
}

#' UltimateOscillator indicator
#' @keywords internal
#' @export
UltimateOscillator <- function(short_, mid, long_) {
  ptr <- .Call("wk_ultimate_oscillator_new", short_, mid, long_, PACKAGE = "wickra")
  .wk_obj("ultimate_oscillator", ptr, "UltimateOscillator")
}

#' UniqueThreeRiver indicator
#' @keywords internal
#' @export
UniqueThreeRiver <- function() {
  ptr <- .Call("wk_unique_three_river_new", PACKAGE = "wickra")
  .wk_obj("unique_three_river", ptr, "UniqueThreeRiver")
}

#' UniversalOscillator indicator
#' @keywords internal
#' @export
UniversalOscillator <- function(period) {
  ptr <- .Call("wk_universal_oscillator_new", period, PACKAGE = "wickra")
  .wk_obj("universal_oscillator", ptr, "UniversalOscillator")
}

#' UpDownVolumeRatio indicator
#' @keywords internal
#' @export
UpDownVolumeRatio <- function() {
  ptr <- .Call("wk_up_down_volume_ratio_new", PACKAGE = "wickra")
  .wk_obj("up_down_volume_ratio", ptr, "UpDownVolumeRatio")
}

#' UpsideGapThreeMethods indicator
#' @keywords internal
#' @export
UpsideGapThreeMethods <- function() {
  ptr <- .Call("wk_upside_gap_three_methods_new", PACKAGE = "wickra")
  .wk_obj("upside_gap_three_methods", ptr, "UpsideGapThreeMethods")
}

#' UpsideGapTwoCrows indicator
#' @keywords internal
#' @export
UpsideGapTwoCrows <- function() {
  ptr <- .Call("wk_upside_gap_two_crows_new", PACKAGE = "wickra")
  .wk_obj("upside_gap_two_crows", ptr, "UpsideGapTwoCrows")
}

#' UpsidePotentialRatio indicator
#' @keywords internal
#' @export
UpsidePotentialRatio <- function(period, mar) {
  ptr <- .Call("wk_upside_potential_ratio_new", period, mar, PACKAGE = "wickra")
  .wk_obj("upside_potential_ratio", ptr, "UpsidePotentialRatio")
}

#' ValueArea indicator
#' @keywords internal
#' @export
ValueArea <- function(period, bin_count, value_area_pct) {
  ptr <- .Call("wk_value_area_new", period, bin_count, value_area_pct, PACKAGE = "wickra")
  .wk_obj("value_area", ptr, "ValueArea")
}

#' ValueAtRisk indicator
#' @keywords internal
#' @export
ValueAtRisk <- function(period, confidence) {
  ptr <- .Call("wk_value_at_risk_new", period, confidence, PACKAGE = "wickra")
  .wk_obj("value_at_risk", ptr, "ValueAtRisk")
}

#' Variance indicator
#' @keywords internal
#' @export
Variance <- function(period) {
  ptr <- .Call("wk_variance_new", period, PACKAGE = "wickra")
  .wk_obj("variance", ptr, "Variance")
}

#' VarianceRatio indicator
#' @keywords internal
#' @export
VarianceRatio <- function(period, q) {
  ptr <- .Call("wk_variance_ratio_new", period, q, PACKAGE = "wickra")
  .wk_obj("variance_ratio", ptr, "VarianceRatio")
}

#' VerticalHorizontalFilter indicator
#' @keywords internal
#' @export
VerticalHorizontalFilter <- function(period) {
  ptr <- .Call("wk_vertical_horizontal_filter_new", period, PACKAGE = "wickra")
  .wk_obj("vertical_horizontal_filter", ptr, "VerticalHorizontalFilter")
}

#' Vidya indicator
#' @keywords internal
#' @export
Vidya <- function(period, cmo_period) {
  ptr <- .Call("wk_vidya_new", period, cmo_period, PACKAGE = "wickra")
  .wk_obj("vidya", ptr, "Vidya")
}

#' VolatilityCone indicator
#' @keywords internal
#' @export
VolatilityCone <- function(window, lookback) {
  ptr <- .Call("wk_volatility_cone_new", window, lookback, PACKAGE = "wickra")
  .wk_obj("volatility_cone", ptr, "VolatilityCone")
}

#' VolatilityOfVolatility indicator
#' @keywords internal
#' @export
VolatilityOfVolatility <- function(vol_window, vov_window) {
  ptr <- .Call("wk_volatility_of_volatility_new", vol_window, vov_window, PACKAGE = "wickra")
  .wk_obj("volatility_of_volatility", ptr, "VolatilityOfVolatility")
}

#' VolatilityRatio indicator
#' @keywords internal
#' @export
VolatilityRatio <- function(period) {
  ptr <- .Call("wk_volatility_ratio_new", period, PACKAGE = "wickra")
  .wk_obj("volatility_ratio", ptr, "VolatilityRatio")
}

#' VoltyStop indicator
#' @keywords internal
#' @export
VoltyStop <- function(atr_period, multiplier) {
  ptr <- .Call("wk_volty_stop_new", atr_period, multiplier, PACKAGE = "wickra")
  .wk_obj("volty_stop", ptr, "VoltyStop")
}

#' VolumeBars indicator
#' @keywords internal
#' @export
VolumeBars <- function(volume_per_bar) {
  ptr <- .Call("wk_volume_bars_new", volume_per_bar, PACKAGE = "wickra")
  .wk_obj("volume_bars", ptr, "VolumeBars")
}

#' VolumeByTimeProfile indicator
#' @keywords internal
#' @export
VolumeByTimeProfile <- function(buckets, utc_offset_minutes) {
  ptr <- .Call("wk_volume_by_time_profile_new", buckets, utc_offset_minutes, PACKAGE = "wickra")
  .wk_obj("volume_by_time_profile", ptr, "VolumeByTimeProfile", values_cap = as.integer(buckets))
}

#' VolumeOscillator indicator
#' @keywords internal
#' @export
VolumeOscillator <- function(fast, slow) {
  ptr <- .Call("wk_volume_oscillator_new", fast, slow, PACKAGE = "wickra")
  .wk_obj("volume_oscillator", ptr, "VolumeOscillator")
}

#' VolumePriceTrend indicator
#' @keywords internal
#' @export
VolumePriceTrend <- function() {
  ptr <- .Call("wk_volume_price_trend_new", PACKAGE = "wickra")
  .wk_obj("volume_price_trend", ptr, "VolumePriceTrend")
}

#' VolumeProfile indicator
#' @keywords internal
#' @export
VolumeProfile <- function(period, bin_count) {
  ptr <- .Call("wk_volume_profile_new", period, bin_count, PACKAGE = "wickra")
  .wk_obj("volume_profile", ptr, "VolumeProfile", values_cap = as.integer(bin_count))
}

#' VolumeRsi indicator
#' @keywords internal
#' @export
VolumeRsi <- function(period) {
  ptr <- .Call("wk_volume_rsi_new", period, PACKAGE = "wickra")
  .wk_obj("volume_rsi", ptr, "VolumeRsi")
}

#' VolumeWeightedMacd indicator
#' @keywords internal
#' @export
VolumeWeightedMacd <- function(fast, slow, signal) {
  ptr <- .Call("wk_volume_weighted_macd_new", fast, slow, signal, PACKAGE = "wickra")
  .wk_obj("volume_weighted_macd", ptr, "VolumeWeightedMacd")
}

#' VolumeWeightedSr indicator
#' @keywords internal
#' @export
VolumeWeightedSr <- function(period) {
  ptr <- .Call("wk_volume_weighted_sr_new", period, PACKAGE = "wickra")
  .wk_obj("volume_weighted_sr", ptr, "VolumeWeightedSr")
}

#' Vortex indicator
#' @keywords internal
#' @export
Vortex <- function(period) {
  ptr <- .Call("wk_vortex_new", period, PACKAGE = "wickra")
  .wk_obj("vortex", ptr, "Vortex")
}

#' Vpin indicator
#' @keywords internal
#' @export
Vpin <- function(bucket_volume, num_buckets) {
  ptr <- .Call("wk_vpin_new", bucket_volume, num_buckets, PACKAGE = "wickra")
  .wk_obj("vpin", ptr, "Vpin")
}

#' Vwap indicator
#' @keywords internal
#' @export
Vwap <- function() {
  ptr <- .Call("wk_vwap_new", PACKAGE = "wickra")
  .wk_obj("vwap", ptr, "Vwap")
}

#' VwapStdDevBands indicator
#' @keywords internal
#' @export
VwapStdDevBands <- function(multiplier) {
  ptr <- .Call("wk_vwap_std_dev_bands_new", multiplier, PACKAGE = "wickra")
  .wk_obj("vwap_std_dev_bands", ptr, "VwapStdDevBands")
}

#' Vwma indicator
#' @keywords internal
#' @export
Vwma <- function(period) {
  ptr <- .Call("wk_vwma_new", period, PACKAGE = "wickra")
  .wk_obj("vwma", ptr, "Vwma")
}

#' Vzo indicator
#' @keywords internal
#' @export
Vzo <- function(period) {
  ptr <- .Call("wk_vzo_new", period, PACKAGE = "wickra")
  .wk_obj("vzo", ptr, "Vzo")
}

#' Wad indicator
#' @keywords internal
#' @export
Wad <- function() {
  ptr <- .Call("wk_wad_new", PACKAGE = "wickra")
  .wk_obj("wad", ptr, "Wad")
}

#' WavePm indicator
#' @keywords internal
#' @export
WavePm <- function(length, smoothing) {
  ptr <- .Call("wk_wave_pm_new", length, smoothing, PACKAGE = "wickra")
  .wk_obj("wave_pm", ptr, "WavePm")
}

#' WaveTrend indicator
#' @keywords internal
#' @export
WaveTrend <- function(channel_period, average_period, signal_period) {
  ptr <- .Call("wk_wave_trend_new", channel_period, average_period, signal_period, PACKAGE = "wickra")
  .wk_obj("wave_trend", ptr, "WaveTrend")
}

#' Wedge indicator
#' @keywords internal
#' @export
Wedge <- function() {
  ptr <- .Call("wk_wedge_new", PACKAGE = "wickra")
  .wk_obj("wedge", ptr, "Wedge")
}

#' WeightedClose indicator
#' @keywords internal
#' @export
WeightedClose <- function() {
  ptr <- .Call("wk_weighted_close_new", PACKAGE = "wickra")
  .wk_obj("weighted_close", ptr, "WeightedClose")
}

#' WickRatio indicator
#' @keywords internal
#' @export
WickRatio <- function() {
  ptr <- .Call("wk_wick_ratio_new", PACKAGE = "wickra")
  .wk_obj("wick_ratio", ptr, "WickRatio")
}

#' WilliamsFractals indicator
#' @keywords internal
#' @export
WilliamsFractals <- function() {
  ptr <- .Call("wk_williams_fractals_new", PACKAGE = "wickra")
  .wk_obj("williams_fractals", ptr, "WilliamsFractals")
}

#' WilliamsR indicator
#' @keywords internal
#' @export
WilliamsR <- function(period) {
  ptr <- .Call("wk_williams_r_new", period, PACKAGE = "wickra")
  .wk_obj("williams_r", ptr, "WilliamsR")
}

#' WinRate indicator
#' @keywords internal
#' @export
WinRate <- function(period) {
  ptr <- .Call("wk_win_rate_new", period, PACKAGE = "wickra")
  .wk_obj("win_rate", ptr, "WinRate")
}

#' Wma indicator
#' @keywords internal
#' @export
Wma <- function(period) {
  ptr <- .Call("wk_wma_new", period, PACKAGE = "wickra")
  .wk_obj("wma", ptr, "Wma")
}

#' WoodiePivots indicator
#' @keywords internal
#' @export
WoodiePivots <- function() {
  ptr <- .Call("wk_woodie_pivots_new", PACKAGE = "wickra")
  .wk_obj("woodie_pivots", ptr, "WoodiePivots")
}

#' YangZhangVolatility indicator
#' @keywords internal
#' @export
YangZhangVolatility <- function(period, trading_periods) {
  ptr <- .Call("wk_yang_zhang_volatility_new", period, trading_periods, PACKAGE = "wickra")
  .wk_obj("yang_zhang_volatility", ptr, "YangZhangVolatility")
}

#' YoyoExit indicator
#' @keywords internal
#' @export
YoyoExit <- function(atr_period, multiplier) {
  ptr <- .Call("wk_yoyo_exit_new", atr_period, multiplier, PACKAGE = "wickra")
  .wk_obj("yoyo_exit", ptr, "YoyoExit")
}

#' ZScore indicator
#' @keywords internal
#' @export
ZScore <- function(period) {
  ptr <- .Call("wk_z_score_new", period, PACKAGE = "wickra")
  .wk_obj("z_score", ptr, "ZScore")
}

#' ZeroLagMacd indicator
#' @keywords internal
#' @export
ZeroLagMacd <- function(fast, slow, signal) {
  ptr <- .Call("wk_zero_lag_macd_new", fast, slow, signal, PACKAGE = "wickra")
  .wk_obj("zero_lag_macd", ptr, "ZeroLagMacd")
}

#' ZigZag indicator
#' @keywords internal
#' @export
ZigZag <- function(threshold) {
  ptr <- .Call("wk_zig_zag_new", threshold, PACKAGE = "wickra")
  .wk_obj("zig_zag", ptr, "ZigZag")
}

#' Zlema indicator
#' @keywords internal
#' @export
Zlema <- function(period) {
  ptr <- .Call("wk_zlema_new", period, PACKAGE = "wickra")
  .wk_obj("zlema", ptr, "Zlema")
}

#' Connect to a live Binance kline feed
#'
#' Opens a live Binance kline stream for one or more comma-separated `symbols`
#' (case-insensitive) at the given `interval` code (an integer `0:15`, the same
#' order as the other bindings: 0 = 1s, 1 = 1m, ... 12 = 1d, 13 = 3d, 14 = 1w,
#' 15 = 1M). `base_url` overrides the endpoint (`NULL` = production). Not
#' available in the wasm (r-universe/webR) build, which has no raw sockets.
#'
#' @param symbols Comma-separated trading symbols (case-insensitive), e.g.
#'   `"btcusdt,ethusdt"`.
#' @param interval Integer interval code `0:15` (`0` = 1s, `1` = 1m, ...,
#'   `12` = 1d, `13` = 3d, `14` = 1w, `15` = 1M).
#' @param base_url Optional endpoint override; `NULL` uses production.
#' @return A `wickra_binance_feed`; poll it with [binance_next()] and close it
#'   with [binance_close()].
#' @keywords internal
#' @export
BinanceFeed <- function(symbols, interval, base_url = NULL) {
  ptr <- .Call("wk_binance_connect", symbols, as.integer(interval), base_url,
               PACKAGE = "wickra")
  structure(list(ptr = ptr), class = "wickra_binance_feed")
}

#' Poll a Binance feed for the next kline event
#'
#' Waits up to `timeout_ms` milliseconds. Returns a named list (`symbol`, `open`,
#' `high`, `low`, `close`, `volume`, `open_time`, `is_closed`) when an event
#' arrives, or `NULL` on timeout. Errors once the stream is closed.
#'
#' @param feed A `wickra_binance_feed` created by [BinanceFeed()].
#' @param timeout_ms Poll timeout in milliseconds.
#' @return A named list (`symbol`, `open`, `high`, `low`, `close`, `volume`,
#'   `open_time`, `is_closed`) for an event, or `NULL` on timeout.
#' @keywords internal
#' @export
binance_next <- function(feed, timeout_ms = 1000) {
  .Call("wk_binance_next", feed$ptr, as.numeric(timeout_ms), PACKAGE = "wickra")
}

#' Close a Binance feed
#'
#' @param feed A `wickra_binance_feed` created by [BinanceFeed()].
#' @return `NULL`, invisibly.
#' @keywords internal
#' @export
binance_close <- function(feed) {
  .Call("wk_binance_close", feed$ptr, PACKAGE = "wickra")
  invisible(NULL)
}

#' Fetch historical Binance klines over REST
#'
#' Downloads up to `limit` (`1:1000`) historical klines for `symbol` at the given
#' `interval` code (an integer `0:15`, the same order as the other bindings).
#' `start_ms`/`end_ms` are optional inclusive Unix-millisecond bounds (a negative
#' value means unset); `base_url` overrides the host (`NULL` = production). Returns
#' an `n x 6` numeric matrix with columns `open`, `high`, `low`, `close`,
#' `volume`, `timestamp`. Blocks until the response arrives. Not available in the
#' wasm (r-universe/webR) build, which has no raw sockets.
#'
#' @param symbol Trading symbol (case-insensitive), e.g. `"btcusdt"`.
#' @param interval Integer interval code `0:15` (see [BinanceFeed()]).
#' @param limit Number of klines to fetch (`1:1000`).
#' @param start_ms,end_ms Optional inclusive Unix-millisecond bounds; a negative
#'   value means unset.
#' @param base_url Optional host override; `NULL` uses production.
#' @return An `n x 6` numeric matrix with columns `open`, `high`, `low`,
#'   `close`, `volume`, `timestamp`.
#' @keywords internal
#' @export
fetch_binance_klines <- function(symbol, interval, limit, start_ms = -1,
                                 end_ms = -1, base_url = NULL) {
  m <- .Call("wk_binance_fetch_klines", symbol, as.integer(interval),
             as.integer(limit), as.numeric(start_ms), as.numeric(end_ms),
             base_url, PACKAGE = "wickra")
  colnames(m) <- c("open", "high", "low", "close", "volume", "timestamp")
  m
}

