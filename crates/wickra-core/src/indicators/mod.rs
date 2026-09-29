//! Built-in indicators. Every indicator implements [`crate::Indicator`].
//!
//! Modules are listed alphabetically; the canonical family taxonomy lives in
//! [`FAMILIES`]. Every public name is re-exported flat from this module and
//! from the crate root for convenience.

// Internal shared building block for the chart- and harmonic-pattern detectors.
// Declared `pub(crate)` (not `mod`) so it is excluded from the public-catalogue
// counter (`grep -c '^mod '`) and re-exported nowhere.
pub(crate) mod pattern_swing;

// Internal shared building block for every rolling variance/dispersion
// indicator. Declared `pub(crate)` (not `mod`) for the same reason as
// `pattern_swing`: it is not a catalogue entry.
pub(crate) mod dc_phasor;
pub(crate) mod rolling_moments;
pub(crate) mod sorted_window;

mod abandoned_baby;
mod abcd;
mod absolute_breadth_index;
mod acceleration_bands;
mod accelerator_oscillator;
mod ad_oscillator;
mod ad_volume_line;
mod adaptive_cci;
mod adaptive_cycle;
mod adaptive_laguerre_filter;
mod adaptive_rsi;
mod adl;
mod advance_block;
mod advance_decline;
mod advance_decline_ratio;
mod adx;
mod adxr;
mod alligator;
mod alma;
mod alpha;
mod amihud_illiquidity;
mod anchored_rsi;
mod anchored_vwap;
mod andrews_pitchfork;
mod apo;
mod aroon;
mod aroon_oscillator;
mod atr;
mod atr_bands;
mod atr_ratchet;
mod atr_trailing_stop;
mod auto_fib;
mod autocorrelation;
mod autocorrelation_periodogram;
mod average_daily_range;
mod average_drawdown;
mod avg_price;
mod awesome_oscillator;
mod awesome_oscillator_histogram;
mod balance_of_power;
mod bandpass_filter;
mod bat;
mod belt_hold;
mod beta;
mod beta_neutral_spread;
mod better_volume;
mod bipower_variation;
mod body_size_pct;
mod bollinger;
mod bollinger_bandwidth;
mod bomar_bands;
mod breadth_thrust;
mod breakaway;
mod bullish_percent_index;
mod burke_ratio;
mod butterfly;
mod calendar_spread;
mod calmar_ratio;
mod camarilla_pivots;
mod candle_volume;
mod cci;
mod center_of_gravity;
mod central_pivot_range;
mod cfo;
mod chaikin_oscillator;
mod chaikin_volatility;
mod chande_kroll_stop;
mod chandelier_exit;
mod choppiness_index;
mod classic_pivots;
mod close_vs_open;
mod closing_marubozu;
mod cmf;
mod cmo;
mod coefficient_of_variation;
mod cointegration;
mod common_sense_ratio;
mod composite_profile;
mod concealing_baby_swallow;
mod conditional_value_at_risk;
mod connors_rsi;
mod coppock;
mod correlation_trend_indicator;
mod counterattack;
mod crab;
mod cumulative_volume_index;
mod cup_and_handle;
mod cvd;
mod cybernetic_cycle;
mod cypher;
mod day_of_week_profile;
mod decycler;
mod decycler_oscillator;
mod dema;
mod demand_index;
mod demark_pivots;
mod depth_slope;
mod derivative_oscillator;
mod detrended_std_dev;
mod disparity_index;
mod distance_ssd;
mod doji;
mod doji_star;
mod dollar_bars;
mod donchian;
mod donchian_stop;
mod double_bollinger;
mod double_top_bottom;
mod downside_gap_three_methods;
mod dpo;
mod dragonfly_doji;
mod drawdown_duration;
mod dumpling_top;
mod dx;
mod dynamic_momentum_index;
mod ease_of_movement;
mod effective_spread;
mod ehlers_stochastic;
mod ehma;
mod elder_impulse;
mod elder_ray;
mod elder_safezone;
mod ema;
mod empirical_mode_decomposition;
mod engulfing;
mod equivolume;
mod estimated_leverage_ratio;
mod even_better_sinewave;
mod evening_doji_star;
mod evwma;
mod ewma_volatility;
mod expectancy;
mod falling_three_methods;
mod fama;
mod fib_arcs;
mod fib_channel;
mod fib_confluence;
mod fib_extension;
mod fib_fan;
mod fib_projection;
mod fib_retracement;
mod fib_time_zones;
mod fibonacci_pivots;
mod fisher_rsi;
mod fisher_transform;
mod flag_pennant;
mod footprint;
mod force_index;
mod fractal_chaos_bands;
mod frama;
mod fry_pan_bottom;
mod funding_basis;
mod funding_implied_apr;
mod funding_rate;
mod funding_rate_mean;
mod funding_rate_zscore;
mod gain_loss_ratio;
mod gain_to_pain_ratio;
mod gap_side_by_side_white;
mod garch11;
mod garman_klass;
mod gartley;
mod gator_oscillator;
mod generalized_dema;
mod geometric_ma;
mod golden_pocket;
mod granger_causality;
mod gravestone_doji;
mod hammer;
mod hanging_man;
mod harami;
mod harami_cross;
mod hasbrouck_information_share;
mod head_and_shoulders;
mod heikin_ashi;
mod heikin_ashi_oscillator;
mod high_low_index;
mod high_low_range;
mod high_low_volume_nodes;
mod high_wave;
mod highpass_filter;
mod hikkake;
mod hikkake_modified;
mod hilbert_dominant_cycle;
mod hilo_activator;
mod historical_volatility;
mod hma;
mod holt_winters;
mod homing_pigeon;
mod ht_dcphase;
mod ht_phasor;
mod ht_trendmode;
mod hurst_channel;
mod hurst_exponent;
mod ichimoku;
mod identical_three_crows;
mod imbalance_bars;
mod in_neck;
mod inertia;
mod information_ratio;
mod initial_balance;
mod instantaneous_trendline;
mod intraday_intensity;
mod intraday_momentum_index;
mod intraday_volatility_profile;
mod inverse_fisher_transform;
mod inverted_hammer;
mod jarque_bera;
mod jma;
mod jump_indicator;
mod k_ratio;
mod kagi_bars;
mod kalman_hedge_ratio;
mod kama;
mod kase_devstop;
mod kase_permission_stochastic;
mod kelly_criterion;
mod keltner;
mod kendall_tau;
mod kicking;
mod kicking_by_length;
mod kst;
mod kurtosis;
mod kvo;
mod kyles_lambda;
mod ladder_bottom;
mod laguerre_rsi;
mod lead_lag_cross_correlation;
mod linreg;
mod linreg_angle;
mod linreg_channel;
mod linreg_intercept;
mod linreg_slope;
mod liquidation_features;
mod log_return;
mod long_legged_doji;
mod long_line;
mod long_short_ratio;
mod m2_measure;
mod ma_envelope;
mod macd;
mod macd_ext;
mod macd_fix;
mod macd_histogram;
mod mama;
mod market_facilitation_index;
mod martin_ratio;
mod marubozu;
mod mass_index;
mod mat_hold;
mod matching_low;
mod max_drawdown;
mod mcclellan_oscillator;
mod mcclellan_summation_index;
mod mcginley_dynamic;
mod median_absolute_deviation;
mod median_channel;
mod median_ma;
mod median_price;
mod mfi;
mod microprice;
mod mid_point;
mod mid_price;
mod minus_di;
mod minus_dm;
mod modified_ma_stop;
mod mom;
mod morning_doji_star;
mod morning_evening_star;
mod murrey_math_lines;
mod naked_poc;
mod natr;
mod new_highs_new_lows;
mod new_price_lines;
mod nrtr;
mod nvi;
mod ob_imbalance_full;
mod ob_imbalance_top1;
mod ob_imbalance_topn;
mod obv;
mod oi_delta;
mod oi_price_divergence;
mod oi_to_volume_ratio;
mod oi_weighted;
mod omega_ratio;
mod on_neck;
mod open_interest_momentum;
mod opening_marubozu;
mod opening_range;
mod order_flow_imbalance;
mod ou_half_life;
mod overnight_gap;
mod overnight_intraday_return;
mod pain_index;
mod pair_spread_zscore;
mod pairwise_beta;
mod parkinson;
mod pearson_correlation;
mod percent_above_ma;
mod percent_b;
mod percentage_trailing_stop;
mod perpetual_premium_index;
mod pgo;
mod piercing_dark_cloud;
mod pin;
mod pivot_reversal;
mod plus_di;
mod plus_dm;
mod pmo;
mod point_and_figure_bars;
mod polarized_fractal_efficiency;
mod ppo;
mod ppo_histogram;
mod profile_shape;
mod profit_factor;
mod projection_bands;
mod projection_oscillator;
mod psar;
mod pvi;
mod qqe;
mod qstick;
mod quartile_bands;
mod quoted_spread;
mod r_squared;
mod range_bars;
mod realized_spread;
mod realized_volatility;
mod recovery_factor;
mod rectangle_range;
mod reflex;
mod regime_label;
mod relative_strength_ab;
mod renko_bars;
mod renko_trailing_stop;
mod rickshaw_man;
mod rising_three_methods;
mod rmi;
mod roc;
mod rocp;
mod rocr;
mod rocr100;
mod rogers_satchell;
mod roll_measure;
mod rolling_correlation;
mod rolling_covariance;
mod rolling_iqr;
mod rolling_min_max_scaler;
mod rolling_percentile_rank;
mod rolling_quantile;
mod roofing_filter;
mod rsi;
mod rsx;
mod run_bars;
mod rvi;
mod rvi_volatility;
mod rwi;
mod sample_entropy;
mod sar_ext;
mod seasonal_z_score;
mod separating_lines;
mod session_high_low;
mod session_range;
mod session_vwap;
mod shannon_entropy;
mod shark;
mod sharpe_ratio;
mod shooting_star;
mod short_line;
mod signed_volume;
mod sine_wave;
mod sine_weighted_ma;
mod single_prints;
mod skewness;
mod sma;
mod smi;
mod smma;
mod smoothed_heikin_ashi;
mod sortino_ratio;
mod spearman_correlation;
mod spinning_top;
mod spread_ar1_coefficient;
mod spread_bollinger_bands;
mod spread_hurst;
mod stalled_pattern;
mod standard_error;
mod standard_error_bands;
mod starc_bands;
mod stc;
mod std_dev;
mod step_trailing_stop;
mod sterling_ratio;
mod stick_sandwich;
mod stoch_rsi;
mod stochastic;
mod stochastic_cci;
mod super_smoother;
mod super_trend;
mod t3;
mod tail_ratio;
mod taker_buy_sell_ratio;
mod takuri;
mod tasuki_gap;
mod td_camouflage;
mod td_clop;
mod td_clopwin;
mod td_combo;
mod td_countdown;
mod td_demarker;
mod td_differential;
mod td_dwave;
mod td_lines;
mod td_moving_average;
mod td_open;
mod td_pressure;
mod td_propulsion;
mod td_range_projection;
mod td_rei;
mod td_risk_level;
mod td_sequential;
mod td_setup;
mod td_trap;
mod tema;
mod term_structure_basis;
mod three_drives;
mod three_inside;
mod three_line_break;
mod three_line_break_bars;
mod three_line_strike;
mod three_outside;
mod three_soldiers_or_crows;
mod three_stars_in_south;
mod thrusting;
mod tick_bars;
mod tick_index;
mod tii;
mod time_based_stop;
mod time_of_day_return_profile;
mod tower_top_bottom;
mod tpo_profile;
mod trade_imbalance;
mod trade_sign_autocorrelation;
mod trade_volume_index;
mod trend_label;
mod trend_strength_index;
mod trendflex;
mod treynor_ratio;
mod triangle;
mod trima;
mod trin;
mod triple_top_bottom;
mod tristar;
mod trix;
mod true_range;
mod tsf;
mod tsf_oscillator;
mod tsi;
mod tsv;
mod ttm_squeeze;
mod ttm_trend;
mod turn_of_month;
mod tweezer;
mod twiggs_money_flow;
mod two_crows;
mod typical_price;
mod ulcer_index;
mod ultimate_oscillator;
mod unique_three_river;
mod universal_oscillator;
mod up_down_volume_ratio;
mod upside_gap_three_methods;
mod upside_gap_two_crows;
mod upside_potential_ratio;
mod value_area;
mod value_at_risk;
mod variance;
mod variance_ratio;
mod vertical_horizontal_filter;
mod vidya;
mod volatility_cone;
mod volatility_of_volatility;
mod volatility_ratio;
mod volty_stop;
mod volume_bars;
mod volume_by_time_profile;
mod volume_oscillator;
mod volume_profile;
mod volume_rsi;
mod volume_weighted_macd;
mod volume_weighted_sr;
mod vortex;
mod vpin;
mod vpt;
mod vwap;
mod vwap_stddev_bands;
mod vwma;
mod vzo;
mod wad;
mod wave_pm;
mod wave_trend;
mod wedge;
mod weighted_close;
mod wick_ratio;
mod williams_fractals;
mod williams_r;
mod win_rate;
mod wma;
mod woodie_pivots;
mod yang_zhang;
mod yoyo_exit;
mod z_score;
mod zero_lag_macd;
mod zig_zag;
mod zlema;

pub use abandoned_baby::AbandonedBaby;
pub use abcd::Abcd;
pub use absolute_breadth_index::AbsoluteBreadthIndex;
pub use acceleration_bands::{AccelerationBands, AccelerationBandsOutput};
pub use accelerator_oscillator::AcceleratorOscillator;
pub use ad_oscillator::AdOscillator;
pub use ad_volume_line::AdVolumeLine;
pub use adaptive_cci::AdaptiveCci;
pub use adaptive_cycle::AdaptiveCycle;
pub use adaptive_laguerre_filter::AdaptiveLaguerreFilter;
pub use adaptive_rsi::AdaptiveRsi;
pub use adl::Adl;
pub use advance_block::AdvanceBlock;
pub use advance_decline::AdvanceDecline;
pub use advance_decline_ratio::AdvanceDeclineRatio;
pub use adx::{Adx, AdxOutput};
pub use adxr::Adxr;
pub use alligator::{Alligator, AlligatorOutput};
pub use alma::Alma;
pub use alpha::Alpha;
pub use amihud_illiquidity::AmihudIlliquidity;
pub use anchored_rsi::AnchoredRsi;
pub use anchored_vwap::AnchoredVwap;
pub use andrews_pitchfork::{AndrewsPitchfork, AndrewsPitchforkOutput};
pub use apo::Apo;
pub use aroon::{Aroon, AroonOutput};
pub use aroon_oscillator::AroonOscillator;
pub use atr::Atr;
pub use atr_bands::{AtrBands, AtrBandsOutput};
pub use atr_ratchet::{AtrRatchet, AtrRatchetOutput};
pub use atr_trailing_stop::AtrTrailingStop;
pub use auto_fib::{AutoFib, AutoFibOutput};
pub use autocorrelation::Autocorrelation;
pub use autocorrelation_periodogram::AutocorrelationPeriodogram;
pub use average_daily_range::AverageDailyRange;
pub use average_drawdown::AverageDrawdown;
pub use avg_price::AvgPrice;
pub use awesome_oscillator::AwesomeOscillator;
pub use awesome_oscillator_histogram::AwesomeOscillatorHistogram;
pub use balance_of_power::BalanceOfPower;
pub use bandpass_filter::BandpassFilter;
pub use bat::Bat;
pub use belt_hold::BeltHold;
pub use beta::Beta;
pub use beta_neutral_spread::BetaNeutralSpread;
pub use better_volume::BetterVolume;
pub use bipower_variation::BipowerVariation;
pub use body_size_pct::BodySizePct;
pub use bollinger::{BollingerBands, BollingerOutput};
pub use bollinger_bandwidth::BollingerBandwidth;
pub use bomar_bands::{BomarBands, BomarBandsOutput};
pub use breadth_thrust::BreadthThrust;
pub use breakaway::Breakaway;
pub use bullish_percent_index::BullishPercentIndex;
pub use burke_ratio::BurkeRatio;
pub use butterfly::Butterfly;
pub use calendar_spread::CalendarSpread;
pub use calmar_ratio::CalmarRatio;
pub use camarilla_pivots::{Camarilla, CamarillaPivotsOutput};
pub use candle_volume::{CandleVolume, CandleVolumeOutput};
pub use cci::Cci;
pub use center_of_gravity::CenterOfGravity;
pub use central_pivot_range::{CentralPivotRange, CentralPivotRangeOutput};
pub use cfo::Cfo;
pub use chaikin_oscillator::ChaikinOscillator;
pub use chaikin_volatility::ChaikinVolatility;
pub use chande_kroll_stop::{ChandeKrollStop, ChandeKrollStopOutput};
pub use chandelier_exit::{ChandelierExit, ChandelierExitOutput};
pub use choppiness_index::ChoppinessIndex;
pub use classic_pivots::{ClassicPivots, ClassicPivotsOutput};
pub use close_vs_open::CloseVsOpen;
pub use closing_marubozu::ClosingMarubozu;
pub use cmf::ChaikinMoneyFlow;
pub use cmo::Cmo;
pub use coefficient_of_variation::CoefficientOfVariation;
pub use cointegration::{Cointegration, CointegrationOutput};
pub use common_sense_ratio::CommonSenseRatio;
pub use composite_profile::{CompositeProfile, CompositeProfileOutput};
pub use concealing_baby_swallow::ConcealingBabySwallow;
pub use conditional_value_at_risk::ConditionalValueAtRisk;
pub use connors_rsi::ConnorsRsi;
pub use coppock::Coppock;
pub use correlation_trend_indicator::CorrelationTrendIndicator;
pub use counterattack::Counterattack;
pub use crab::Crab;
pub use cumulative_volume_index::CumulativeVolumeIndex;
pub use cup_and_handle::CupAndHandle;
pub use cvd::CumulativeVolumeDelta;
pub use cybernetic_cycle::CyberneticCycle;
pub use cypher::Cypher;
pub use day_of_week_profile::{DayOfWeekProfile, DayOfWeekProfileOutput};
pub use decycler::Decycler;
pub use decycler_oscillator::DecyclerOscillator;
pub use dema::Dema;
pub use demand_index::DemandIndex;
pub use demark_pivots::{DemarkPivots, DemarkPivotsOutput};
pub use depth_slope::DepthSlope;
pub use derivative_oscillator::DerivativeOscillator;
pub use detrended_std_dev::DetrendedStdDev;
pub use disparity_index::DisparityIndex;
pub use distance_ssd::DistanceSsd;
pub use doji::Doji;
pub use doji_star::DojiStar;
pub use dollar_bars::{DollarBar, DollarBars};
pub use donchian::{Donchian, DonchianOutput};
pub use donchian_stop::{DonchianStop, DonchianStopOutput};
pub use double_bollinger::{DoubleBollinger, DoubleBollingerOutput};
pub use double_top_bottom::DoubleTopBottom;
pub use downside_gap_three_methods::DownsideGapThreeMethods;
pub use dpo::Dpo;
pub use dragonfly_doji::DragonflyDoji;
pub use drawdown_duration::DrawdownDuration;
pub use dumpling_top::DumplingTop;
pub use dx::Dx;
pub use dynamic_momentum_index::DynamicMomentumIndex;
pub use ease_of_movement::EaseOfMovement;
pub use effective_spread::EffectiveSpread;
pub use ehlers_stochastic::EhlersStochastic;
pub use ehma::Ehma;
pub use elder_impulse::ElderImpulse;
pub use elder_ray::{ElderRay, ElderRayOutput};
pub use elder_safezone::{ElderSafeZone, ElderSafeZoneOutput};
pub use ema::Ema;
pub use empirical_mode_decomposition::EmpiricalModeDecomposition;
pub use engulfing::Engulfing;
pub use equivolume::{Equivolume, EquivolumeOutput};
pub use estimated_leverage_ratio::EstimatedLeverageRatio;
pub use even_better_sinewave::EvenBetterSinewave;
pub use evening_doji_star::EveningDojiStar;
pub use evwma::Evwma;
pub use ewma_volatility::EwmaVolatility;
pub use expectancy::Expectancy;
pub use falling_three_methods::FallingThreeMethods;
pub use fama::Fama;
pub use fib_arcs::{FibArcs, FibArcsOutput};
pub use fib_channel::{FibChannel, FibChannelOutput};
pub use fib_confluence::{FibConfluence, FibConfluenceOutput};
pub use fib_extension::{FibExtension, FibExtensionOutput};
pub use fib_fan::{FibFan, FibFanOutput};
pub use fib_projection::{FibProjection, FibProjectionOutput};
pub use fib_retracement::{FibRetracement, FibRetracementOutput};
pub use fib_time_zones::{FibTimeZones, FibTimeZonesOutput};
pub use fibonacci_pivots::{FibonacciPivots, FibonacciPivotsOutput};
pub use fisher_rsi::FisherRsi;
pub use fisher_transform::FisherTransform;
pub use flag_pennant::FlagPennant;
pub use footprint::{Footprint, FootprintLevel, FootprintOutput};
pub use force_index::ForceIndex;
pub use fractal_chaos_bands::{FractalChaosBands, FractalChaosBandsOutput};
pub use frama::Frama;
pub use fry_pan_bottom::FryPanBottom;
pub use funding_basis::FundingBasis;
pub use funding_implied_apr::FundingImpliedApr;
pub use funding_rate::FundingRate;
pub use funding_rate_mean::FundingRateMean;
pub use funding_rate_zscore::FundingRateZScore;
pub use gain_loss_ratio::GainLossRatio;
pub use gain_to_pain_ratio::GainToPainRatio;
pub use gap_side_by_side_white::GapSideBySideWhite;
pub use garch11::Garch11;
pub use garman_klass::GarmanKlassVolatility;
pub use gartley::Gartley;
pub use gator_oscillator::{GatorOscillator, GatorOscillatorOutput};
pub use generalized_dema::GeneralizedDema;
pub use geometric_ma::GeometricMa;
pub use golden_pocket::{GoldenPocket, GoldenPocketOutput};
pub use granger_causality::GrangerCausality;
pub use gravestone_doji::GravestoneDoji;
pub use hammer::Hammer;
pub use hanging_man::HangingMan;
pub use harami::Harami;
pub use harami_cross::HaramiCross;
pub use hasbrouck_information_share::HasbrouckInformationShare;
pub use head_and_shoulders::HeadAndShoulders;
pub use heikin_ashi::{HeikinAshi, HeikinAshiOutput};
pub use heikin_ashi_oscillator::HeikinAshiOscillator;
pub use high_low_index::HighLowIndex;
pub use high_low_range::HighLowRange;
pub use high_low_volume_nodes::{HighLowVolumeNodes, HighLowVolumeNodesOutput};
pub use high_wave::HighWave;
pub use highpass_filter::HighpassFilter;
pub use hikkake::Hikkake;
pub use hikkake_modified::HikkakeModified;
pub use hilbert_dominant_cycle::HilbertDominantCycle;
pub use hilo_activator::HiLoActivator;
pub use historical_volatility::HistoricalVolatility;
pub use hma::Hma;
pub use holt_winters::HoltWinters;
pub use homing_pigeon::HomingPigeon;
pub use ht_dcphase::HtDcPhase;
pub use ht_phasor::{HtPhasor, HtPhasorOutput};
pub use ht_trendmode::HtTrendMode;
pub use hurst_channel::{HurstChannel, HurstChannelOutput};
pub use hurst_exponent::HurstExponent;
pub use ichimoku::{Ichimoku, IchimokuOutput};
pub use identical_three_crows::IdenticalThreeCrows;
pub use imbalance_bars::{ImbalanceBar, ImbalanceBars};
pub use in_neck::InNeck;
pub use inertia::Inertia;
pub use information_ratio::InformationRatio;
pub use initial_balance::{InitialBalance, InitialBalanceOutput};
pub use instantaneous_trendline::InstantaneousTrendline;
pub use intraday_intensity::IntradayIntensity;
pub use intraday_momentum_index::IntradayMomentumIndex;
pub use intraday_volatility_profile::{IntradayVolatilityProfile, IntradayVolatilityProfileOutput};
pub use inverse_fisher_transform::InverseFisherTransform;
pub use inverted_hammer::InvertedHammer;
pub use jarque_bera::JarqueBera;
pub use jma::Jma;
pub use jump_indicator::JumpIndicator;
pub use k_ratio::KRatio;
pub use kagi_bars::{KagiBar, KagiBars};
pub use kalman_hedge_ratio::{KalmanHedgeRatio, KalmanHedgeRatioOutput};
pub use kama::Kama;
pub use kase_devstop::{KaseDevStop, KaseDevStopOutput};
pub use kase_permission_stochastic::{KasePermissionStochastic, KasePermissionStochasticOutput};
pub use kelly_criterion::KellyCriterion;
pub use keltner::{Keltner, KeltnerOutput};
pub use kendall_tau::KendallTau;
pub use kicking::Kicking;
pub use kicking_by_length::KickingByLength;
pub use kst::{Kst, KstOutput};
pub use kurtosis::Kurtosis;
pub use kvo::Kvo;
pub use kyles_lambda::KylesLambda;
pub use ladder_bottom::LadderBottom;
pub use laguerre_rsi::LaguerreRsi;
pub use lead_lag_cross_correlation::{LeadLagCrossCorrelation, LeadLagCrossCorrelationOutput};
pub use linreg::LinearRegression;
pub use linreg_angle::LinRegAngle;
pub use linreg_channel::{LinRegChannel, LinRegChannelOutput};
pub use linreg_intercept::LinRegIntercept;
pub use linreg_slope::LinRegSlope;
pub use liquidation_features::{LiquidationFeatures, LiquidationFeaturesOutput};
pub use log_return::LogReturn;
pub use long_legged_doji::LongLeggedDoji;
pub use long_line::LongLine;
pub use long_short_ratio::LongShortRatio;
pub use m2_measure::M2Measure;
pub use ma_envelope::{MaEnvelope, MaEnvelopeOutput};
pub use macd::{MacdIndicator, MacdOutput};
pub use macd_ext::{MaType, MacdExt};
pub use macd_fix::MacdFix;
pub use macd_histogram::MacdHistogram;
pub use mama::{Mama, MamaOutput};
pub use market_facilitation_index::MarketFacilitationIndex;
pub use martin_ratio::MartinRatio;
pub use marubozu::Marubozu;
pub use mass_index::MassIndex;
pub use mat_hold::MatHold;
pub use matching_low::MatchingLow;
pub use max_drawdown::MaxDrawdown;
pub use mcclellan_oscillator::McClellanOscillator;
pub use mcclellan_summation_index::McClellanSummationIndex;
pub use mcginley_dynamic::McGinleyDynamic;
pub use median_absolute_deviation::MedianAbsoluteDeviation;
pub use median_channel::{MedianChannel, MedianChannelOutput};
pub use median_ma::MedianMa;
pub use median_price::MedianPrice;
pub use mfi::Mfi;
pub use microprice::Microprice;
pub use mid_point::MidPoint;
pub use mid_price::MidPrice;
pub use minus_di::MinusDi;
pub use minus_dm::MinusDm;
pub use modified_ma_stop::{ModifiedMaStop, ModifiedMaStopOutput};
pub use mom::Mom;
pub use morning_doji_star::MorningDojiStar;
pub use morning_evening_star::MorningEveningStar;
pub use murrey_math_lines::{MurreyMathLines, MurreyMathLinesOutput};
pub use naked_poc::NakedPoc;
pub use natr::Natr;
pub use new_highs_new_lows::NewHighsNewLows;
pub use new_price_lines::NewPriceLines;
pub use nrtr::{Nrtr, NrtrOutput};
pub use nvi::Nvi;
pub use ob_imbalance_full::OrderBookImbalanceFull;
pub use ob_imbalance_top1::OrderBookImbalanceTop1;
pub use ob_imbalance_topn::OrderBookImbalanceTopN;
pub use obv::Obv;
pub use oi_delta::OpenInterestDelta;
pub use oi_price_divergence::OIPriceDivergence;
pub use oi_to_volume_ratio::OiToVolumeRatio;
pub use oi_weighted::OIWeighted;
pub use omega_ratio::OmegaRatio;
pub use on_neck::OnNeck;
pub use open_interest_momentum::OpenInterestMomentum;
pub use opening_marubozu::OpeningMarubozu;
pub use opening_range::{OpeningRange, OpeningRangeOutput};
pub use order_flow_imbalance::OrderFlowImbalance;
pub use ou_half_life::OuHalfLife;
pub use overnight_gap::OvernightGap;
pub use overnight_intraday_return::{OvernightIntradayReturn, OvernightIntradayReturnOutput};
pub use pain_index::PainIndex;
pub use pair_spread_zscore::PairSpreadZScore;
pub use pairwise_beta::PairwiseBeta;
pub use parkinson::ParkinsonVolatility;
pub use pearson_correlation::PearsonCorrelation;
pub use percent_above_ma::PercentAboveMa;
pub use percent_b::PercentB;
pub use percentage_trailing_stop::PercentageTrailingStop;
pub use perpetual_premium_index::PerpetualPremiumIndex;
pub use pgo::Pgo;
pub use piercing_dark_cloud::PiercingDarkCloud;
pub use pin::Pin;
pub use pivot_reversal::PivotReversal;
pub use plus_di::PlusDi;
pub use plus_dm::PlusDm;
pub use pmo::Pmo;
pub use point_and_figure_bars::{PnfColumn, PointAndFigureBars};
pub use polarized_fractal_efficiency::PolarizedFractalEfficiency;
pub use ppo::Ppo;
pub use ppo_histogram::PpoHistogram;
pub use profile_shape::ProfileShape;
pub use profit_factor::ProfitFactor;
pub use projection_bands::{ProjectionBands, ProjectionBandsOutput};
pub use projection_oscillator::ProjectionOscillator;
pub use psar::Psar;
pub use pvi::Pvi;
pub use qqe::{Qqe, QqeOutput};
pub use qstick::Qstick;
pub use quartile_bands::{QuartileBands, QuartileBandsOutput};
pub use quoted_spread::QuotedSpread;
pub use r_squared::RSquared;
pub use range_bars::{RangeBar, RangeBars};
pub use realized_spread::RealizedSpread;
pub use realized_volatility::RealizedVolatility;
pub use recovery_factor::RecoveryFactor;
pub use rectangle_range::RectangleRange;
pub use reflex::Reflex;
pub use regime_label::RegimeLabel;
pub use relative_strength_ab::{RelativeStrengthAB, RelativeStrengthOutput};
pub use renko_bars::{RenkoBars, RenkoBrick};
pub use renko_trailing_stop::RenkoTrailingStop;
pub use rickshaw_man::RickshawMan;
pub use rising_three_methods::RisingThreeMethods;
pub use rmi::Rmi;
pub use roc::Roc;
pub use rocp::Rocp;
pub use rocr::Rocr;
pub use rocr100::Rocr100;
pub use rogers_satchell::RogersSatchellVolatility;
pub use roll_measure::RollMeasure;
pub use rolling_correlation::RollingCorrelation;
pub use rolling_covariance::RollingCovariance;
pub use rolling_iqr::RollingIqr;
pub use rolling_min_max_scaler::RollingMinMaxScaler;
pub use rolling_percentile_rank::RollingPercentileRank;
pub use rolling_quantile::RollingQuantile;
pub use roofing_filter::RoofingFilter;
pub use rsi::Rsi;
pub use rsx::Rsx;
pub use run_bars::{RunBar, RunBars};
pub use rvi::Rvi;
pub use rvi_volatility::RviVolatility;
pub use rwi::{Rwi, RwiOutput};
pub use sample_entropy::SampleEntropy;
pub use sar_ext::SarExt;
pub use seasonal_z_score::SeasonalZScore;
pub use separating_lines::SeparatingLines;
pub use session_high_low::{SessionHighLow, SessionHighLowOutput};
pub use session_range::{SessionRange, SessionRangeOutput};
pub use session_vwap::SessionVwap;
pub use shannon_entropy::ShannonEntropy;
pub use shark::Shark;
pub use sharpe_ratio::SharpeRatio;
pub use shooting_star::ShootingStar;
pub use short_line::ShortLine;
pub use signed_volume::SignedVolume;
pub use sine_wave::SineWave;
pub use sine_weighted_ma::SineWeightedMa;
pub use single_prints::SinglePrints;
pub use skewness::Skewness;
pub use sma::Sma;
pub use smi::Smi;
pub use smma::Smma;
pub use smoothed_heikin_ashi::{SmoothedHeikinAshi, SmoothedHeikinAshiOutput};
pub use sortino_ratio::SortinoRatio;
pub use spearman_correlation::SpearmanCorrelation;
pub use spinning_top::SpinningTop;
pub use spread_ar1_coefficient::SpreadAr1Coefficient;
pub use spread_bollinger_bands::{SpreadBollingerBands, SpreadBollingerBandsOutput};
pub use spread_hurst::SpreadHurst;
pub use stalled_pattern::StalledPattern;
pub use standard_error::StandardError;
pub use standard_error_bands::{StandardErrorBands, StandardErrorBandsOutput};
pub use starc_bands::{StarcBands, StarcBandsOutput};
pub use stc::Stc;
pub use std_dev::StdDev;
pub use step_trailing_stop::StepTrailingStop;
pub use sterling_ratio::SterlingRatio;
pub use stick_sandwich::StickSandwich;
pub use stoch_rsi::StochRsi;
pub use stochastic::{Stochastic, StochasticOutput};
pub use stochastic_cci::StochasticCci;
pub use super_smoother::SuperSmoother;
pub use super_trend::{SuperTrend, SuperTrendOutput};
pub use t3::T3;
pub use tail_ratio::TailRatio;
pub use taker_buy_sell_ratio::TakerBuySellRatio;
pub use takuri::Takuri;
pub use tasuki_gap::TasukiGap;
pub use td_camouflage::TdCamouflage;
pub use td_clop::TdClop;
pub use td_clopwin::TdClopwin;
pub use td_combo::TdCombo;
pub use td_countdown::TdCountdown;
pub use td_demarker::TdDeMarker;
pub use td_differential::TdDifferential;
pub use td_dwave::TdDWave;
pub use td_lines::{TdLines, TdLinesOutput};
pub use td_moving_average::{TdMovingAverage, TdMovingAverageOutput};
pub use td_open::TdOpen;
pub use td_pressure::TdPressure;
pub use td_propulsion::TdPropulsion;
pub use td_range_projection::{TdRangeProjection, TdRangeProjectionOutput};
pub use td_rei::TdRei;
pub use td_risk_level::{TdRiskLevel, TdRiskLevelOutput};
pub use td_sequential::{TdSequential, TdSequentialOutput};
pub use td_setup::TdSetup;
pub use td_trap::TdTrap;
pub use tema::Tema;
pub use term_structure_basis::TermStructureBasis;
pub use three_drives::ThreeDrives;
pub use three_inside::ThreeInside;
pub use three_line_break::ThreeLineBreak;
pub use three_line_break_bars::{LineBreakBar, ThreeLineBreakBars};
pub use three_line_strike::ThreeLineStrike;
pub use three_outside::ThreeOutside;
pub use three_soldiers_or_crows::ThreeSoldiersOrCrows;
pub use three_stars_in_south::ThreeStarsInSouth;
pub use thrusting::Thrusting;
pub use tick_bars::{TickBar, TickBars};
pub use tick_index::TickIndex;
pub use tii::Tii;
pub use time_based_stop::TimeBasedStop;
pub use time_of_day_return_profile::{TimeOfDayReturnProfile, TimeOfDayReturnProfileOutput};
pub use tower_top_bottom::TowerTopBottom;
pub use tpo_profile::{TpoProfile, TpoProfileOutput};
pub use trade_imbalance::TradeImbalance;
pub use trade_sign_autocorrelation::TradeSignAutocorrelation;
pub use trade_volume_index::TradeVolumeIndex;
pub use trend_label::TrendLabel;
pub use trend_strength_index::TrendStrengthIndex;
pub use trendflex::Trendflex;
pub use treynor_ratio::TreynorRatio;
pub use triangle::Triangle;
pub use trima::Trima;
pub use trin::Trin;
pub use triple_top_bottom::TripleTopBottom;
pub use tristar::Tristar;
pub use trix::Trix;
pub use true_range::TrueRange;
pub use tsf::Tsf;
pub use tsf_oscillator::TsfOscillator;
pub use tsi::Tsi;
pub use tsv::Tsv;
pub use ttm_squeeze::{TtmSqueeze, TtmSqueezeOutput};
pub use ttm_trend::TtmTrend;
pub use turn_of_month::TurnOfMonth;
pub use tweezer::Tweezer;
pub use twiggs_money_flow::TwiggsMoneyFlow;
pub use two_crows::TwoCrows;
pub use typical_price::TypicalPrice;
pub use ulcer_index::UlcerIndex;
pub use ultimate_oscillator::UltimateOscillator;
pub use unique_three_river::UniqueThreeRiver;
pub use universal_oscillator::UniversalOscillator;
pub use up_down_volume_ratio::UpDownVolumeRatio;
pub use upside_gap_three_methods::UpsideGapThreeMethods;
pub use upside_gap_two_crows::UpsideGapTwoCrows;
pub use upside_potential_ratio::UpsidePotentialRatio;
pub use value_area::{ValueArea, ValueAreaOutput};
pub use value_at_risk::ValueAtRisk;
pub use variance::Variance;
pub use variance_ratio::VarianceRatio;
pub use vertical_horizontal_filter::VerticalHorizontalFilter;
pub use vidya::Vidya;
pub use volatility_cone::{VolatilityCone, VolatilityConeOutput};
pub use volatility_of_volatility::VolatilityOfVolatility;
pub use volatility_ratio::VolatilityRatio;
pub use volty_stop::VoltyStop;
pub use volume_bars::{VolumeBar, VolumeBars};
pub use volume_by_time_profile::{VolumeByTimeProfile, VolumeByTimeProfileOutput};
pub use volume_oscillator::VolumeOscillator;
pub use volume_profile::{VolumeProfile, VolumeProfileOutput};
pub use volume_rsi::VolumeRsi;
pub use volume_weighted_macd::{VolumeWeightedMacd, VolumeWeightedMacdOutput};
pub use volume_weighted_sr::{VolumeWeightedSr, VolumeWeightedSrOutput};
pub use vortex::{Vortex, VortexOutput};
pub use vpin::Vpin;
pub use vpt::VolumePriceTrend;
pub use vwap::{RollingVwap, Vwap};
pub use vwap_stddev_bands::{VwapStdDevBands, VwapStdDevBandsOutput};
pub use vwma::Vwma;
pub use vzo::Vzo;
pub use wad::Wad;
pub use wave_pm::WavePm;
pub use wave_trend::{WaveTrend, WaveTrendOutput};
pub use wedge::Wedge;
pub use weighted_close::WeightedClose;
pub use wick_ratio::WickRatio;
pub use williams_fractals::{WilliamsFractals, WilliamsFractalsOutput};
pub use williams_r::WilliamsR;
pub use win_rate::WinRate;
pub use wma::Wma;
pub use woodie_pivots::{WoodiePivots, WoodiePivotsOutput};
pub use yang_zhang::YangZhangVolatility;
pub use yoyo_exit::YoyoExit;
pub use z_score::ZScore;
pub use zero_lag_macd::{ZeroLagMacd, ZeroLagMacdOutput};
pub use zig_zag::{ZigZag, ZigZagOutput};
pub use zlema::Zlema;

/// Family classification of every built-in indicator. The (family,
/// indicators) list is the single source of truth used by `family_tests`
/// below; README and Wiki taxonomy tables should be kept in sync with it.
///
/// Each indicator appears in exactly one family. Names are the public
/// struct identifiers re-exported from this module (and the crate root).
pub const FAMILIES: &[(&str, &[&str])] = &[
    (
        "Moving Averages",
        &[
            "Sma",
            "Ema",
            "Wma",
            "Dema",
            "Tema",
            "Hma",
            "Kama",
            "Smma",
            "Trima",
            "Zlema",
            "T3",
            "Vwma",
            "Alma",
            "McGinleyDynamic",
            "Frama",
            "Vidya",
            "Jma",
            "Alligator",
            "Evwma",
            "SineWeightedMa",
            "GeometricMa",
            "Ehma",
            "MedianMa",
            "AdaptiveLaguerreFilter",
            "GeneralizedDema",
            "HoltWinters",
        ],
    ),
    (
        "Momentum Oscillators",
        &[
            "Rsi",
            "AnchoredRsi",
            "Stochastic",
            "Cci",
            "Roc",
            "WilliamsR",
            "Mfi",
            "AwesomeOscillator",
            "Mom",
            "Cmo",
            "Tsi",
            "Pmo",
            "StochRsi",
            "UltimateOscillator",
            "Rvi",
            "Pgo",
            "Kst",
            "Smi",
            "LaguerreRsi",
            "ConnorsRsi",
            "Inertia",
            "Rocp",
            "Rocr",
            "Rocr100",
            "DisparityIndex",
            "FisherRsi",
            "Rsx",
            "DynamicMomentumIndex",
            "StochasticCci",
            "Rmi",
            "DerivativeOscillator",
            "ElderRay",
            "IntradayMomentumIndex",
            "Qqe",
        ],
    ),
    (
        "Trend & Directional",
        &[
            "MacdIndicator",
            "MacdFix",
            "MacdExt",
            "Adx",
            "Adxr",
            "Aroon",
            "Trix",
            "AroonOscillator",
            "Vortex",
            "Rwi",
            "Tii",
            "WaveTrend",
            "MassIndex",
            "ChoppinessIndex",
            "VerticalHorizontalFilter",
            "PlusDm",
            "MinusDm",
            "PlusDi",
            "MinusDi",
            "Dx",
            "TrendLabel",
            "TtmTrend",
            "TrendStrengthIndex",
            "Qstick",
            "PolarizedFractalEfficiency",
            "WavePm",
            "GatorOscillator",
            "KasePermissionStochastic",
        ],
    ),
    (
        "Price Oscillators",
        &[
            "Ppo",
            "Dpo",
            "Coppock",
            "AcceleratorOscillator",
            "BalanceOfPower",
            "Apo",
            "AwesomeOscillatorHistogram",
            "Cfo",
            "ZeroLagMacd",
            "ElderImpulse",
            "Stc",
            "TsfOscillator",
            "MacdHistogram",
            "PpoHistogram",
        ],
    ),
    (
        "Volatility & Bands",
        &[
            "Atr",
            "BollingerBands",
            "Keltner",
            "Donchian",
            "Natr",
            "StdDev",
            "UlcerIndex",
            "HistoricalVolatility",
            "BollingerBandwidth",
            "PercentB",
            "TrueRange",
            "ChaikinVolatility",
            "RviVolatility",
            "ParkinsonVolatility",
            "GarmanKlassVolatility",
            "RogersSatchellVolatility",
            "YangZhangVolatility",
            "JumpIndicator",
            "RegimeLabel",
            "EwmaVolatility",
            "Garch11",
            "VolatilityOfVolatility",
            "BipowerVariation",
            "VolatilityRatio",
            "VolatilityCone",
        ],
    ),
    (
        "Bands & Channels",
        &[
            "MaEnvelope",
            "AccelerationBands",
            "StarcBands",
            "AtrBands",
            "HurstChannel",
            "LinRegChannel",
            "StandardErrorBands",
            "DoubleBollinger",
            "TtmSqueeze",
            "FractalChaosBands",
            "VwapStdDevBands",
            "QuartileBands",
            "BomarBands",
            "MedianChannel",
            "ProjectionBands",
            "ProjectionOscillator",
        ],
    ),
    (
        "Trailing Stops",
        &[
            "Psar",
            "SuperTrend",
            "ChandelierExit",
            "ChandeKrollStop",
            "AtrTrailingStop",
            "HiLoActivator",
            "VoltyStop",
            "YoyoExit",
            "DonchianStop",
            "PercentageTrailingStop",
            "StepTrailingStop",
            "RenkoTrailingStop",
            "SarExt",
            "KaseDevStop",
            "ElderSafeZone",
            "AtrRatchet",
            "Nrtr",
            "TimeBasedStop",
            "ModifiedMaStop",
        ],
    ),
    (
        "Volume",
        &[
            "Obv",
            "Vwap",
            "RollingVwap",
            "Adl",
            "VolumePriceTrend",
            "ChaikinMoneyFlow",
            "ChaikinOscillator",
            "ForceIndex",
            "EaseOfMovement",
            "Kvo",
            "VolumeOscillator",
            "Nvi",
            "Pvi",
            "AdOscillator",
            "AnchoredVwap",
            "DemandIndex",
            "Tsv",
            "Vzo",
            "MarketFacilitationIndex",
            "VolumeRsi",
            "Wad",
            "TwiggsMoneyFlow",
            "TradeVolumeIndex",
            "IntradayIntensity",
            "BetterVolume",
            "VolumeWeightedMacd",
        ],
    ),
    (
        "Price Statistics",
        &[
            "TypicalPrice",
            "MedianPrice",
            "WeightedClose",
            "LinearRegression",
            "LinRegSlope",
            "ZScore",
            "LinRegAngle",
            "Variance",
            "CoefficientOfVariation",
            "Skewness",
            "Kurtosis",
            "StandardError",
            "DetrendedStdDev",
            "RSquared",
            "MedianAbsoluteDeviation",
            "Autocorrelation",
            "HurstExponent",
            "PearsonCorrelation",
            "Beta",
            "SpearmanCorrelation",
            "Cointegration",
            "LeadLagCrossCorrelation",
            "PairSpreadZScore",
            "PairwiseBeta",
            "RelativeStrengthAB",
            "MidPrice",
            "MidPoint",
            "AvgPrice",
            "LinRegIntercept",
            "Tsf",
            "RollingCorrelation",
            "RollingCovariance",
            "OuHalfLife",
            "SpreadHurst",
            "DistanceSsd",
            "BetaNeutralSpread",
            "VarianceRatio",
            "GrangerCausality",
            "KalmanHedgeRatio",
            "SpreadBollingerBands",
            "LogReturn",
            "RealizedVolatility",
            "RollingIqr",
            "RollingPercentileRank",
            "RollingQuantile",
            "SpreadAr1Coefficient",
            "CloseVsOpen",
            "BodySizePct",
            "WickRatio",
            "HighLowRange",
            "JarqueBera",
            "RollingMinMaxScaler",
            "ShannonEntropy",
            "SampleEntropy",
            "KendallTau",
        ],
    ),
    (
        "Ehlers / Cycle (DSP)",
        &[
            "Mama",
            "Fama",
            "FisherTransform",
            "InverseFisherTransform",
            "SuperSmoother",
            "HilbertDominantCycle",
            "HtDcPhase",
            "HtPhasor",
            "HtTrendMode",
            "SineWave",
            "Decycler",
            "DecyclerOscillator",
            "RoofingFilter",
            "CenterOfGravity",
            "CyberneticCycle",
            "AdaptiveCycle",
            "EmpiricalModeDecomposition",
            "EhlersStochastic",
            "InstantaneousTrendline",
            "HighpassFilter",
            "Reflex",
            "Trendflex",
            "CorrelationTrendIndicator",
            "AdaptiveRsi",
            "UniversalOscillator",
            "AdaptiveCci",
            "BandpassFilter",
            "EvenBetterSinewave",
            "AutocorrelationPeriodogram",
        ],
    ),
    (
        "Pivots & S/R",
        &[
            "ClassicPivots",
            "FibonacciPivots",
            "Camarilla",
            "WoodiePivots",
            "DemarkPivots",
            "WilliamsFractals",
            "ZigZag",
            "CentralPivotRange",
            "MurreyMathLines",
            "AndrewsPitchfork",
            "VolumeWeightedSr",
            "PivotReversal",
        ],
    ),
    (
        "DeMark",
        &[
            "TdSetup",
            "TdSequential",
            "TdDeMarker",
            "TdRei",
            "TdPressure",
            "TdCombo",
            "TdCountdown",
            "TdLines",
            "TdRangeProjection",
            "TdDifferential",
            "TdOpen",
            "TdRiskLevel",
            "TdCamouflage",
            "TdClop",
            "TdClopwin",
            "TdPropulsion",
            "TdTrap",
            "TdDWave",
            "TdMovingAverage",
        ],
    ),
    (
        "Ichimoku & Charts",
        &[
            "Ichimoku",
            "HeikinAshi",
            "HeikinAshiOscillator",
            "ThreeLineBreak",
            "SmoothedHeikinAshi",
            "Equivolume",
            "CandleVolume",
        ],
    ),
    (
        "Candlestick Patterns",
        &[
            "Doji",
            "Hammer",
            "InvertedHammer",
            "HangingMan",
            "ShootingStar",
            "Engulfing",
            "Harami",
            "MorningEveningStar",
            "ThreeSoldiersOrCrows",
            "PiercingDarkCloud",
            "Marubozu",
            "Tweezer",
            "SpinningTop",
            "ThreeInside",
            "ThreeOutside",
            "TwoCrows",
            "UpsideGapTwoCrows",
            "IdenticalThreeCrows",
            "ThreeLineStrike",
            "ThreeStarsInSouth",
            "AbandonedBaby",
            "AdvanceBlock",
            "BeltHold",
            "Breakaway",
            "Counterattack",
            "DojiStar",
            "DragonflyDoji",
            "GravestoneDoji",
            "LongLeggedDoji",
            "RickshawMan",
            "EveningDojiStar",
            "MorningDojiStar",
            "GapSideBySideWhite",
            "HighWave",
            "Hikkake",
            "HikkakeModified",
            "HomingPigeon",
            "OnNeck",
            "InNeck",
            "Thrusting",
            "SeparatingLines",
            "Kicking",
            "KickingByLength",
            "LadderBottom",
            "MatHold",
            "MatchingLow",
            "LongLine",
            "ShortLine",
            "RisingThreeMethods",
            "FallingThreeMethods",
            "UpsideGapThreeMethods",
            "DownsideGapThreeMethods",
            "StalledPattern",
            "StickSandwich",
            "Takuri",
            "ClosingMarubozu",
            "OpeningMarubozu",
            "TasukiGap",
            "UniqueThreeRiver",
            "ConcealingBabySwallow",
            "Tristar",
            "HaramiCross",
            "TowerTopBottom",
            "FryPanBottom",
            "DumplingTop",
            "NewPriceLines",
        ],
    ),
    (
        "Microstructure",
        &[
            "OrderBookImbalanceTop1",
            "OrderBookImbalanceTopN",
            "OrderBookImbalanceFull",
            "Microprice",
            "QuotedSpread",
            "DepthSlope",
            "SignedVolume",
            "CumulativeVolumeDelta",
            "TradeImbalance",
            "EffectiveSpread",
            "RealizedSpread",
            "KylesLambda",
            "Footprint",
            "OrderFlowImbalance",
            "Vpin",
            "AmihudIlliquidity",
            "RollMeasure",
            "TradeSignAutocorrelation",
            "Pin",
            "HasbrouckInformationShare",
        ],
    ),
    (
        "Derivatives",
        &[
            "FundingRate",
            "FundingRateMean",
            "FundingRateZScore",
            "FundingBasis",
            "OpenInterestDelta",
            "OIPriceDivergence",
            "OIWeighted",
            "LongShortRatio",
            "TakerBuySellRatio",
            "LiquidationFeatures",
            "TermStructureBasis",
            "CalendarSpread",
            "EstimatedLeverageRatio",
            "OiToVolumeRatio",
            "PerpetualPremiumIndex",
            "FundingImpliedApr",
            "OpenInterestMomentum",
        ],
    ),
    (
        "Market Profile",
        &[
            "ValueArea",
            "InitialBalance",
            "OpeningRange",
            "VolumeProfile",
            "TpoProfile",
            "NakedPoc",
            "SinglePrints",
            "ProfileShape",
            "HighLowVolumeNodes",
            "CompositeProfile",
        ],
    ),
    (
        "Risk / Performance",
        &[
            "SharpeRatio",
            "SortinoRatio",
            "CalmarRatio",
            "OmegaRatio",
            "MaxDrawdown",
            "AverageDrawdown",
            "DrawdownDuration",
            "PainIndex",
            "ValueAtRisk",
            "ConditionalValueAtRisk",
            "ProfitFactor",
            "GainLossRatio",
            "RecoveryFactor",
            "KellyCriterion",
            "TreynorRatio",
            "InformationRatio",
            "Alpha",
            "WinRate",
            "Expectancy",
            "SterlingRatio",
            "BurkeRatio",
            "MartinRatio",
            "TailRatio",
            "KRatio",
            "CommonSenseRatio",
            "GainToPainRatio",
            "UpsidePotentialRatio",
            "M2Measure",
        ],
    ),
    (
        "Alt-Chart Bars",
        &[
            "RenkoBars",
            "KagiBars",
            "PointAndFigureBars",
            "RangeBars",
            "TickBars",
            "VolumeBars",
            "DollarBars",
            "ImbalanceBars",
            "RunBars",
            "ThreeLineBreakBars",
        ],
    ),
    (
        "Market Breadth",
        &[
            "AdvanceDecline",
            "AdvanceDeclineRatio",
            "AdVolumeLine",
            "McClellanOscillator",
            "McClellanSummationIndex",
            "Trin",
            "BreadthThrust",
            "NewHighsNewLows",
            "HighLowIndex",
            "PercentAboveMa",
            "UpDownVolumeRatio",
            "BullishPercentIndex",
            "CumulativeVolumeIndex",
            "AbsoluteBreadthIndex",
            "TickIndex",
        ],
    ),
    (
        "Seasonality & Session",
        &[
            "SessionVwap",
            "SessionHighLow",
            "SessionRange",
            "AverageDailyRange",
            "OvernightGap",
            "OvernightIntradayReturn",
            "TurnOfMonth",
            "SeasonalZScore",
            "TimeOfDayReturnProfile",
            "DayOfWeekProfile",
            "IntradayVolatilityProfile",
            "VolumeByTimeProfile",
        ],
    ),
    (
        "Chart Patterns",
        &[
            "DoubleTopBottom",
            "TripleTopBottom",
            "HeadAndShoulders",
            "Triangle",
            "Wedge",
            "FlagPennant",
            "RectangleRange",
            "CupAndHandle",
        ],
    ),
    (
        "Harmonic Patterns",
        &[
            "Abcd",
            "Gartley",
            "Butterfly",
            "Bat",
            "Crab",
            "Shark",
            "Cypher",
            "ThreeDrives",
        ],
    ),
    (
        "Fibonacci",
        &[
            "FibRetracement",
            "FibExtension",
            "FibProjection",
            "AutoFib",
            "GoldenPocket",
            "FibConfluence",
            "FibFan",
            "FibArcs",
            "FibChannel",
            "FibTimeZones",
        ],
    ),
];

#[cfg(test)]
mod family_tests {
    use super::FAMILIES;

    #[test]
    fn no_duplicates_across_families() {
        let mut names: Vec<&str> = FAMILIES
            .iter()
            .flat_map(|(_, ns)| ns.iter().copied())
            .collect();
        let len_before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            len_before,
            "duplicate indicator across families"
        );
    }

    #[test]
    fn total_count_matches_expected() {
        // Bump together with new indicators. Drift between this number and
        // the actual indicator count is the early-warning signal that an
        // indicator was added without being assigned a family.
        let total: usize = FAMILIES.iter().map(|(_, ns)| ns.len()).sum();
        assert_eq!(total, 514, "FAMILIES total drifted from indicator count");
    }
}
