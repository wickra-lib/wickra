// Comprehensive tests for the Wickra Node bindings: streaming-vs-batch
// equivalence, reference values, and lifecycle methods across all 71
// indicators. Ported from the Python test_streaming_vs_batch / test_known_values
// suites.

const test = require('node:test');
const assert = require('node:assert/strict');
const wickra = require('..');

// Synthetic OHLCV series long enough to warm up every indicator.
const N = 120;
const close = Array.from({ length: N }, (_, i) => 100 + Math.sin(i * 0.2) * 10 + i * 0.1);
const high = close.map((c) => c + 1.5);
const low = close.map((c) => c - 1.5);
const volume = Array.from({ length: N }, (_, i) => 1000 + (i % 7) * 50);
const open = close.map((c) => c - 0.5);

function eq(a, b) {
  if (Number.isNaN(a)) return Number.isNaN(b);
  if (!Number.isFinite(a) || !Number.isFinite(b)) return a === b;
  return Math.abs(a - b) < 1e-9;
}

function num(v) {
  return v === null || v === undefined ? NaN : v;
}

// --- Scalar indicators: update(value) vs batch(prices) ---

const scalarFactories = {
  M2Measure: () => new wickra.M2Measure(20, 0.0, 0.02),
  UpsidePotentialRatio: () => new wickra.UpsidePotentialRatio(20, 0.0),
  GainToPainRatio: () => new wickra.GainToPainRatio(12),
  CommonSenseRatio: () => new wickra.CommonSenseRatio(20),
  KRatio: () => new wickra.KRatio(30),
  TailRatio: () => new wickra.TailRatio(20),
  MartinRatio: () => new wickra.MartinRatio(14),
  BurkeRatio: () => new wickra.BurkeRatio(12),
  SterlingRatio: () => new wickra.SterlingRatio(12),
  AUTOCORRPGRAM: () => new wickra.AUTOCORRPGRAM(10, 48),
  EVENBETTERSINE: () => new wickra.EVENBETTERSINE(40, 10),
  BANDPASS: () => new wickra.BANDPASS(20, 0.3),
  UNIVERSALOSC: () => new wickra.UNIVERSALOSC(20),
  ADAPTIVERSI: () => new wickra.ADAPTIVERSI(14),
  CTI: () => new wickra.CTI(20),
  TRENDFLEX: () => new wickra.TRENDFLEX(20),
  REFLEX: () => new wickra.REFLEX(20),
  HIGHPASS: () => new wickra.HIGHPASS(48),
  SAMPLEENT: () => new wickra.SAMPLEENT(20, 2, 0.2),
  SHANNONENT: () => new wickra.SHANNONENT(20, 8),
  ROLLINGMINMAX: () => new wickra.ROLLINGMINMAX(20),
  JARQUEBERA: () => new wickra.JARQUEBERA(20),
  BipowerVariation: () => new wickra.BipowerVariation(20),
  VolatilityOfVolatility: () => new wickra.VolatilityOfVolatility(20, 20),
  Garch11: () => new wickra.Garch11(0.000002, 0.1, 0.88),
  EwmaVolatility: () => new wickra.EwmaVolatility(0.94),
  PpoHistogram: () => new wickra.PpoHistogram(3, 6, 3),
  MacdHistogram: () => new wickra.MacdHistogram(3, 6, 3),
  TsfOscillator: () => new wickra.TsfOscillator(3),
  WAVE_PM: () => new wickra.WAVE_PM(32, 3),
  POLARIZED_FRACTAL_EFFICIENCY: () => new wickra.POLARIZED_FRACTAL_EFFICIENCY(10, 5),
  TREND_STRENGTH_INDEX: () => new wickra.TREND_STRENGTH_INDEX(20),
  DerivativeOscillator: () => new wickra.DerivativeOscillator(14, 5, 3, 9),
  RMI: () => new wickra.RMI(14, 5),
  DynamicMomentumIndex: () => new wickra.DynamicMomentumIndex(14),
  RSX: () => new wickra.RSX(14),
  FisherRSI: () => new wickra.FisherRSI(14),
  DisparityIndex: () => new wickra.DisparityIndex(14),
  HoltWinters: () => new wickra.HoltWinters(0.2, 0.1),
  GD: () => new wickra.GD(5, 0.7),
  AdaptiveLaguerre: () => new wickra.AdaptiveLaguerre(13),
  MedianMA: () => new wickra.MedianMA(14),
  EHMA: () => new wickra.EHMA(9),
  GMA: () => new wickra.GMA(14),
  SWMA: () => new wickra.SWMA(14),
  Expectancy: () => new wickra.Expectancy(20),
  WinRate: () => new wickra.WinRate(20),
  RegimeLabel: () => new wickra.RegimeLabel(5, 20),
  JumpIndicator: () => new wickra.JumpIndicator(20, 3.0),
  TrendLabel: () => new wickra.TrendLabel(10),
  RollingQuantile: () => new wickra.RollingQuantile(20, 0.5),
  RollingPercentileRank: () => new wickra.RollingPercentileRank(14),
  RollingIqr: () => new wickra.RollingIqr(14),
  RealizedVolatility: () => new wickra.RealizedVolatility(20),
  LogReturn: () => new wickra.LogReturn(1),
  TSF: () => new wickra.TSF(14),
  LINEARREG_INTERCEPT: () => new wickra.LINEARREG_INTERCEPT(14),
  ROCR100: () => new wickra.ROCR100(10),
  ROCR: () => new wickra.ROCR(10),
  ROCP: () => new wickra.ROCP(10),
  MIDPOINT: () => new wickra.MIDPOINT(14),
  SMA: () => new wickra.SMA(14),
  EMA: () => new wickra.EMA(14),
  WMA: () => new wickra.WMA(14),
  RSI: () => new wickra.RSI(14),
  AnchoredRSI: () => new wickra.AnchoredRSI(),
  DEMA: () => new wickra.DEMA(10),
  TEMA: () => new wickra.TEMA(10),
  HMA: () => new wickra.HMA(9),
  ROC: () => new wickra.ROC(12),
  TRIX: () => new wickra.TRIX(9),
  KAMA: () => new wickra.KAMA(10, 2, 30),
  ALMA: () => new wickra.ALMA(9, 0.85, 6.0),
  McGinleyDynamic: () => new wickra.McGinleyDynamic(10),
  FRAMA: () => new wickra.FRAMA(16),
  VIDYA: () => new wickra.VIDYA(14, 9),
  JMA: () => new wickra.JMA(14, 0, 2),
  SMMA: () => new wickra.SMMA(14),
  TRIMA: () => new wickra.TRIMA(20),
  ZLEMA: () => new wickra.ZLEMA(14),
  T3: () => new wickra.T3(5, 0.7),
  MOM: () => new wickra.MOM(10),
  CMO: () => new wickra.CMO(14),
  TSI: () => new wickra.TSI(25, 13),
  PMO: () => new wickra.PMO(35, 20),
  TII: () => new wickra.TII(20, 10),
  StochRSI: () => new wickra.StochRSI(14, 14),
  PPO: () => new wickra.PPO(12, 26),
  APO: () => new wickra.APO(12, 26),
  CFO: () => new wickra.CFO(14),
  ElderImpulse: () => new wickra.ElderImpulse(13, 12, 26, 9),
  STC: () => new wickra.STC(23, 50, 10, 0.5),
  DPO: () => new wickra.DPO(20),
  Coppock: () => new wickra.Coppock(14, 11, 10),
  StdDev: () => new wickra.StdDev(20),
  UlcerIndex: () => new wickra.UlcerIndex(14),
  HistoricalVolatility: () => new wickra.HistoricalVolatility(20, 252),
  BollingerBandwidth: () => new wickra.BollingerBandwidth(20, 2),
  PercentB: () => new wickra.PercentB(20, 2),
  LinearRegression: () => new wickra.LinearRegression(14),
  LinRegSlope: () => new wickra.LinRegSlope(14),
  VerticalHorizontalFilter: () => new wickra.VerticalHorizontalFilter(28),
  ZScore: () => new wickra.ZScore(20),
  LinRegAngle: () => new wickra.LinRegAngle(14),
  PercentageTrailingStop: () => new wickra.PercentageTrailingStop(5),
  StepTrailingStop: () => new wickra.StepTrailingStop(1),
  RenkoTrailingStop: () => new wickra.RenkoTrailingStop(1),
  LaguerreRSI: () => new wickra.LaguerreRSI(0.5),
  ConnorsRSI: () => new wickra.ConnorsRSI(3, 2, 100),
  RVIVolatility: () => new wickra.RVIVolatility(10),
  // Family 10 — Ehlers / Cycle
  SuperSmoother: () => new wickra.SuperSmoother(10),
  FisherTransform: () => new wickra.FisherTransform(10),
  InverseFisherTransform: () => new wickra.InverseFisherTransform(1.0),
  Decycler: () => new wickra.Decycler(20),
  DecyclerOscillator: () => new wickra.DecyclerOscillator(10, 30),
  RoofingFilter: () => new wickra.RoofingFilter(10, 48),
  CenterOfGravity: () => new wickra.CenterOfGravity(10),
  CyberneticCycle: () => new wickra.CyberneticCycle(10),
  InstantaneousTrendline: () => new wickra.InstantaneousTrendline(20),
  EhlersStochastic: () => new wickra.EhlersStochastic(20),
  EmpiricalModeDecomposition: () => new wickra.EmpiricalModeDecomposition(20, 0.1),
  HilbertDominantCycle: () => new wickra.HilbertDominantCycle(),
  HT_DCPHASE: () => new wickra.HT_DCPHASE(),
  HT_TRENDMODE: () => new wickra.HT_TRENDMODE(),
  AdaptiveCycle: () => new wickra.AdaptiveCycle(),
  SineWave: () => new wickra.SineWave(),
  FAMA: () => new wickra.FAMA(0.5, 0.05),
  // Family 12 — Statistik / Regression
  Variance: () => new wickra.Variance(20),
  CoefficientOfVariation: () => new wickra.CoefficientOfVariation(20),
  Skewness: () => new wickra.Skewness(20),
  Kurtosis: () => new wickra.Kurtosis(20),
  StandardError: () => new wickra.StandardError(14),
  DetrendedStdDev: () => new wickra.DetrendedStdDev(14),
  RSquared: () => new wickra.RSquared(14),
  MedianAbsoluteDeviation: () => new wickra.MedianAbsoluteDeviation(20),
  Autocorrelation: () => new wickra.Autocorrelation(20, 1),
  HurstExponent: () => new wickra.HurstExponent(40, 4),
  // Family 15 — Risk / Performance metrics (scalar f64 input).
  SharpeRatio: () => new wickra.SharpeRatio(20, 0),
  SortinoRatio: () => new wickra.SortinoRatio(20, 0),
  CalmarRatio: () => new wickra.CalmarRatio(20),
  OmegaRatio: () => new wickra.OmegaRatio(20, 0),
  MaxDrawdown: () => new wickra.MaxDrawdown(20),
  AverageDrawdown: () => new wickra.AverageDrawdown(20),
  DrawdownDuration: () => new wickra.DrawdownDuration(),
  PainIndex: () => new wickra.PainIndex(20),
  ValueAtRisk: () => new wickra.ValueAtRisk(20, 0.95),
  ConditionalValueAtRisk: () => new wickra.ConditionalValueAtRisk(20, 0.95),
  ProfitFactor: () => new wickra.ProfitFactor(20),
  GainLossRatio: () => new wickra.GainLossRatio(20),
  RecoveryFactor: () => new wickra.RecoveryFactor(),
  KellyCriterion: () => new wickra.KellyCriterion(20),
};

// --- Two-series (asset, benchmark) ratio indicators ---

const ratioPairFactories = {
  TreynorRatio: () => new wickra.TreynorRatio(20, 0),
  InformationRatio: () => new wickra.InformationRatio(20),
  Alpha: () => new wickra.Alpha(20, 0),
};

const asset = Array.from({ length: N }, (_, i) => 0.001 + Math.sin(i * 0.15) * 0.01);
const bench = Array.from({ length: N }, (_, i) => 0.001 + Math.sin(i * 0.15) * 0.007);

for (const [name, make] of Object.entries(ratioPairFactories)) {
  test(`${name}: streaming update matches batch (pair)`, () => {
    const batch = make().batch(asset, bench);
    const streaming = make();
    assert.equal(batch.length, N);
    for (let i = 0; i < N; i++) {
      const s = num(streaming.update(asset[i], bench[i]));
      assert.ok(eq(s, batch[i]), `${name} mismatch at ${i}: ${s} vs ${batch[i]}`);
    }
  });
}

for (const [name, make] of Object.entries(scalarFactories)) {
  test(`${name}: streaming update matches batch`, () => {
    const batch = make().batch(close);
    const streaming = make();
    assert.equal(batch.length, N);
    for (let i = 0; i < N; i++) {
      const s = num(streaming.update(close[i]));
      assert.ok(eq(s, batch[i]), `${name} mismatch at ${i}: ${s} vs ${batch[i]}`);
    }
  });
}

// --- Scalar-output candle indicators: update(...) vs batch(...) ---

const candleScalar = {
  MIDPRICE: { make: () => new wickra.MIDPRICE(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  DX: { make: () => new wickra.DX(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  MINUS_DI: { make: () => new wickra.MINUS_DI(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  PLUS_DI: { make: () => new wickra.PLUS_DI(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  ATR: { make: () => new wickra.ATR(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  PLUS_DM: { make: () => new wickra.PLUS_DM(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  MINUS_DM: { make: () => new wickra.MINUS_DM(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  CCI: { make: () => new wickra.CCI(20), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  WilliamsR: { make: () => new wickra.WilliamsR(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  PSAR: { make: () => new wickra.PSAR(0.02, 0.02, 0.2), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  SAREXT: { make: () => new wickra.SAREXT(0, 0, 0.02, 0.02, 0.2, 0.02, 0.02, 0.2), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  MFI: { make: () => new wickra.MFI(14), step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  VWAP: { make: () => new wickra.VWAP(), step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  RollingVWAP: { make: () => new wickra.RollingVWAP(20), step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  AwesomeOscillator: { make: () => new wickra.AwesomeOscillator(5, 34), step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  OBV: { make: () => new wickra.OBV(), step: (ind, i) => ind.update(close[i], volume[i]), batch: (ind) => ind.batch(close, volume) },
  VWMA: { make: () => new wickra.VWMA(20), step: (ind, i) => ind.update(close[i], volume[i]), batch: (ind) => ind.batch(close, volume) },
  RVI: { make: () => new wickra.RVI(10), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  AVGPRICE: { make: () => new wickra.AVGPRICE(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Inertia: { make: () => new wickra.Inertia(14, 20), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  PGO: { make: () => new wickra.PGO(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  SMI: { make: () => new wickra.SMI(5, 3, 3), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  EVWMA: { make: () => new wickra.EVWMA(20), step: (ind, i) => ind.update(close[i], volume[i]), batch: (ind) => ind.batch(close, volume) },
  UltimateOscillator: { make: () => new wickra.UltimateOscillator(7, 14, 28), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  AroonOscillator: { make: () => new wickra.AroonOscillator(14), step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  NATR: { make: () => new wickra.NATR(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  MassIndex: { make: () => new wickra.MassIndex(9, 25), step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  ADL: { make: () => new wickra.ADL(), step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  VolumePriceTrend: { make: () => new wickra.VolumePriceTrend(), step: (ind, i) => ind.update(close[i], volume[i]), batch: (ind) => ind.batch(close, volume) },
  ChaikinMoneyFlow: { make: () => new wickra.ChaikinMoneyFlow(20), step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  ChaikinOscillator: { make: () => new wickra.ChaikinOscillator(3, 10), step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  ForceIndex: { make: () => new wickra.ForceIndex(13), step: (ind, i) => ind.update(close[i], volume[i]), batch: (ind) => ind.batch(close, volume) },
  EaseOfMovement: { make: () => new wickra.EaseOfMovement(14, 1e8), step: (ind, i) => ind.update(high[i], low[i], volume[i]), batch: (ind) => ind.batch(high, low, volume) },
  KVO: { make: () => new wickra.KVO(34, 55), step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  VolumeOscillator: { make: () => new wickra.VolumeOscillator(14, 28), step: (ind, i) => ind.update(volume[i]), batch: (ind) => ind.batch(volume) },
  NVI: { make: () => new wickra.NVI(), step: (ind, i) => ind.update(close[i], volume[i]), batch: (ind) => ind.batch(close, volume) },
  PVI: { make: () => new wickra.PVI(), step: (ind, i) => ind.update(close[i], volume[i]), batch: (ind) => ind.batch(close, volume) },
  AdOscillator: { make: () => new wickra.AdOscillator(), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  AnchoredVWAP: { make: () => new wickra.AnchoredVWAP(), step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  DemandIndex: { make: () => new wickra.DemandIndex(10), step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  TSV: { make: () => new wickra.TSV(18), step: (ind, i) => ind.update(close[i], volume[i]), batch: (ind) => ind.batch(close, volume) },
  VZO: { make: () => new wickra.VZO(14), step: (ind, i) => ind.update(close[i], volume[i]), batch: (ind) => ind.batch(close, volume) },
  MarketFacilitationIndex: { make: () => new wickra.MarketFacilitationIndex(), step: (ind, i) => ind.update(high[i], low[i], volume[i]), batch: (ind) => ind.batch(high, low, volume) },
  AtrTrailingStop: { make: () => new wickra.AtrTrailingStop(14, 3), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  HiLoActivator: { make: () => new wickra.HiLoActivator(3), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  VoltyStop: { make: () => new wickra.VoltyStop(14, 2), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  YoyoExit: { make: () => new wickra.YoyoExit(14, 2), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  TypicalPrice: { make: () => new wickra.TypicalPrice(), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  MedianPrice: { make: () => new wickra.MedianPrice(), step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  WeightedClose: { make: () => new wickra.WeightedClose(), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  AcceleratorOscillator: { make: () => new wickra.AcceleratorOscillator(5, 34, 5), step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  AwesomeOscillatorHistogram: { make: () => new wickra.AwesomeOscillatorHistogram(5, 34, 5), step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  BalanceOfPower: { make: () => new wickra.BalanceOfPower(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  ChoppinessIndex: { make: () => new wickra.ChoppinessIndex(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  TrueRange: { make: () => new wickra.TrueRange(), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  ChaikinVolatility: { make: () => new wickra.ChaikinVolatility(10, 10), step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  ADXR: { make: () => new wickra.ADXR(7), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  ParkinsonVolatility: { make: () => new wickra.ParkinsonVolatility(20, 252), step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  GarmanKlassVolatility: { make: () => new wickra.GarmanKlassVolatility(20, 252), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  RogersSatchellVolatility: { make: () => new wickra.RogersSatchellVolatility(20, 252), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  YangZhangVolatility: { make: () => new wickra.YangZhangVolatility(20, 252), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  TDSetup: { make: () => new wickra.TDSetup(4, 9), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  TDDeMarker: { make: () => new wickra.TDDeMarker(14), step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  TDREI: { make: () => new wickra.TDREI(5), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  TDPressure: { make: () => new wickra.TDPressure(5), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(open, high, low, close, volume) },
  TDCombo: { make: () => new wickra.TDCombo(4, 9, 2, 13), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  TDCountdown: { make: () => new wickra.TDCountdown(4, 9, 2, 13), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  TDDifferential: { make: () => new wickra.TDDifferential(), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  TDOpen: { make: () => new wickra.TDOpen(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  // Family 14 — Candlestick patterns
  Doji: { make: () => new wickra.Doji(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Hammer: { make: () => new wickra.Hammer(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  InvertedHammer: { make: () => new wickra.InvertedHammer(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  HangingMan: { make: () => new wickra.HangingMan(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  ShootingStar: { make: () => new wickra.ShootingStar(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Engulfing: { make: () => new wickra.Engulfing(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Harami: { make: () => new wickra.Harami(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  MorningEveningStar: { make: () => new wickra.MorningEveningStar(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  ThreeSoldiersOrCrows: { make: () => new wickra.ThreeSoldiersOrCrows(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  PiercingDarkCloud: { make: () => new wickra.PiercingDarkCloud(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Marubozu: { make: () => new wickra.Marubozu(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Tweezer: { make: () => new wickra.Tweezer(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  SpinningTop: { make: () => new wickra.SpinningTop(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  ThreeInside: { make: () => new wickra.ThreeInside(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  ThreeOutside: { make: () => new wickra.ThreeOutside(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  TwoCrows: { make: () => new wickra.TwoCrows(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  UpsideGapTwoCrows: { make: () => new wickra.UpsideGapTwoCrows(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  IdenticalThreeCrows: { make: () => new wickra.IdenticalThreeCrows(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  ThreeLineStrike: { make: () => new wickra.ThreeLineStrike(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  ThreeStarsInSouth: { make: () => new wickra.ThreeStarsInSouth(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  AbandonedBaby: { make: () => new wickra.AbandonedBaby(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  AdvanceBlock: { make: () => new wickra.AdvanceBlock(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  BeltHold: { make: () => new wickra.BeltHold(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Breakaway: { make: () => new wickra.Breakaway(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Counterattack: { make: () => new wickra.Counterattack(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  DojiStar: { make: () => new wickra.DojiStar(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  DragonflyDoji: { make: () => new wickra.DragonflyDoji(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  GravestoneDoji: { make: () => new wickra.GravestoneDoji(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  LongLeggedDoji: { make: () => new wickra.LongLeggedDoji(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  RickshawMan: { make: () => new wickra.RickshawMan(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  EveningDojiStar: { make: () => new wickra.EveningDojiStar(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  MorningDojiStar: { make: () => new wickra.MorningDojiStar(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  GapSideBySideWhite: { make: () => new wickra.GapSideBySideWhite(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  HighWave: { make: () => new wickra.HighWave(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Hikkake: { make: () => new wickra.Hikkake(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  HikkakeModified: { make: () => new wickra.HikkakeModified(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  HomingPigeon: { make: () => new wickra.HomingPigeon(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  OnNeck: { make: () => new wickra.OnNeck(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  InNeck: { make: () => new wickra.InNeck(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Thrusting: { make: () => new wickra.Thrusting(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  SeparatingLines: { make: () => new wickra.SeparatingLines(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Kicking: { make: () => new wickra.Kicking(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  KickingByLength: { make: () => new wickra.KickingByLength(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  LadderBottom: { make: () => new wickra.LadderBottom(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  MatHold: { make: () => new wickra.MatHold(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  MatchingLow: { make: () => new wickra.MatchingLow(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  LongLine: { make: () => new wickra.LongLine(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  ShortLine: { make: () => new wickra.ShortLine(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  RisingThreeMethods: { make: () => new wickra.RisingThreeMethods(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  FallingThreeMethods: { make: () => new wickra.FallingThreeMethods(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  UpsideGapThreeMethods: { make: () => new wickra.UpsideGapThreeMethods(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  DownsideGapThreeMethods: { make: () => new wickra.DownsideGapThreeMethods(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  StalledPattern: { make: () => new wickra.StalledPattern(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  StickSandwich: { make: () => new wickra.StickSandwich(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Takuri: { make: () => new wickra.Takuri(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  ClosingMarubozu: { make: () => new wickra.ClosingMarubozu(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  OpeningMarubozu: { make: () => new wickra.OpeningMarubozu(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  TasukiGap: { make: () => new wickra.TasukiGap(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  UniqueThreeRiver: { make: () => new wickra.UniqueThreeRiver(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  ConcealingBabySwallow: { make: () => new wickra.ConcealingBabySwallow(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  DoubleTopBottom: { make: () => new wickra.DoubleTopBottom(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  TripleTopBottom: { make: () => new wickra.TripleTopBottom(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  HeadAndShoulders: { make: () => new wickra.HeadAndShoulders(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Triangle: { make: () => new wickra.Triangle(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Wedge: { make: () => new wickra.Wedge(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  FlagPennant: { make: () => new wickra.FlagPennant(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  RectangleRange: { make: () => new wickra.RectangleRange(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  CupAndHandle: { make: () => new wickra.CupAndHandle(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Abcd: { make: () => new wickra.Abcd(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Gartley: { make: () => new wickra.Gartley(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Butterfly: { make: () => new wickra.Butterfly(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Bat: { make: () => new wickra.Bat(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Crab: { make: () => new wickra.Crab(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Shark: { make: () => new wickra.Shark(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Cypher: { make: () => new wickra.Cypher(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  ThreeDrives: { make: () => new wickra.ThreeDrives(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  CloseVsOpen: { make: () => new wickra.CloseVsOpen(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  BodySizePct: { make: () => new wickra.BodySizePct(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  WickRatio: { make: () => new wickra.WickRatio(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  HighLowRange: { make: () => new wickra.HighLowRange(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  StochasticCCI: { make: () => new wickra.StochasticCCI(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  IMI: { make: () => new wickra.IMI(14), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  TTM_TREND: { make: () => new wickra.TTM_TREND(6), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  Qstick: { make: () => new wickra.Qstick(10), step: (ind, i) => ind.update(open[i], close[i]), batch: (ind) => ind.batch(open, close) },
  VolatilityRatio: { make: () => new wickra.VolatilityRatio(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  ProjectionOscillator: { make: () => new wickra.ProjectionOscillator(14), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  TimeBasedStop: { make: () => new wickra.TimeBasedStop(5), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  VolumeRsi: { make: () => new wickra.VolumeRsi(14), step: (ind, i) => ind.update(close[i], volume[i]), batch: (ind) => ind.batch(close, volume) },
  Wad: { make: () => new wickra.Wad(), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  TwiggsMoneyFlow: { make: () => new wickra.TwiggsMoneyFlow(21), step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  TradeVolumeIndex: { make: () => new wickra.TradeVolumeIndex(0.25), step: (ind, i) => ind.update(close[i], volume[i]), batch: (ind) => ind.batch(close, volume) },
  IntradayIntensity: { make: () => new wickra.IntradayIntensity(), step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  BetterVolume: { make: () => new wickra.BetterVolume(14), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(open, high, low, close, volume) },
  ADAPTIVECCI: { make: () => new wickra.ADAPTIVECCI(20), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  PivotReversal: { make: () => new wickra.PivotReversal(1, 1), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  TDCamouflage: { make: () => new wickra.TDCamouflage(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  TDClop: { make: () => new wickra.TDClop(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  TDClopwin: { make: () => new wickra.TDClopwin(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  TDPropulsion: { make: () => new wickra.TDPropulsion(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  TDTrap: { make: () => new wickra.TDTrap(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  TDDWave: { make: () => new wickra.TDDWave(2), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  HeikinAshiOscillator: { make: () => new wickra.HeikinAshiOscillator(5), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  ThreeLineBreak: { make: () => new wickra.ThreeLineBreak(3), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  Tristar: { make: () => new wickra.Tristar(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  HaramiCross: { make: () => new wickra.HaramiCross(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  TowerTopBottom: { make: () => new wickra.TowerTopBottom(), step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  DumplingTop: { make: () => new wickra.DumplingTop(9), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  NewPriceLines: { make: () => new wickra.NewPriceLines(5), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  FryPanBottom: { make: () => new wickra.FryPanBottom(9), step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  NakedPoc: { make: () => new wickra.NakedPoc(20, 24), step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  SinglePrints: { make: () => new wickra.SinglePrints(20, 24), step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  ProfileShape: { make: () => new wickra.ProfileShape(20, 24), step: (ind, i) => ind.update(high[i], low[i], volume[i]), batch: (ind) => ind.batch(high, low, volume) },
};

for (const [name, d] of Object.entries(candleScalar)) {
  test(`${name}: streaming update matches batch`, () => {
    const batch = d.batch(d.make());
    const streaming = d.make();
    assert.equal(batch.length, N);
    for (let i = 0; i < N; i++) {
      const s = num(d.step(streaming, i));
      assert.ok(eq(s, batch[i]), `${name} mismatch at ${i}: ${s} vs ${batch[i]}`);
    }
  });
}

// --- Multi-output indicators: object update vs interleaved batch ---

const multi = {
  KST: { make: () => new wickra.KST(10, 15, 20, 30, 10, 10, 10, 15, 9), fields: ['kst', 'signal'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  Alligator: { make: () => new wickra.Alligator(13, 8, 5), fields: ['jaw', 'teeth', 'lips'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  ZeroLagMACD: { make: () => new wickra.ZeroLagMACD(12, 26, 9), fields: ['macd', 'signal', 'histogram'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  MACD: { make: () => new wickra.MACD(12, 26, 9), fields: ['macd', 'signal', 'histogram'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  HT_PHASOR: { make: () => new wickra.HT_PHASOR(), fields: ['inphase', 'quadrature'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  MACDFIX: { make: () => new wickra.MACDFIX(9), fields: ['macd', 'signal', 'histogram'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  MACDEXT: { make: () => new wickra.MACDEXT(12, 0, 26, 0, 9, 0), fields: ['macd', 'signal', 'histogram'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  KST: { make: () => wickra.KST.classic(), fields: ['kst', 'signal'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  BollingerBands: { make: () => new wickra.BollingerBands(20, 2), fields: ['upper', 'middle', 'lower', 'stddev'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  Stochastic: { make: () => new wickra.Stochastic(14, 3), fields: ['k', 'd'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  ADX: { make: () => new wickra.ADX(14), fields: ['plusDi', 'minusDi', 'adx'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  Keltner: { make: () => new wickra.Keltner(20, 10, 2), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  Donchian: { make: () => new wickra.Donchian(20), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  Aroon: { make: () => new wickra.Aroon(14), fields: ['up', 'down'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  Vortex: { make: () => new wickra.Vortex(14), fields: ['plus', 'minus'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  RWI: { make: () => new wickra.RWI(14), fields: ['high', 'low'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  WaveTrend: { make: () => wickra.WaveTrend.classic(), fields: ['wt1', 'wt2'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  SuperTrend: { make: () => new wickra.SuperTrend(10, 3), fields: ['value', 'direction'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  ChandelierExit: { make: () => new wickra.ChandelierExit(22, 3), fields: ['longStop', 'shortStop'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  ChandeKrollStop: { make: () => new wickra.ChandeKrollStop(10, 1, 9), fields: ['stopLong', 'stopShort'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  // Family 16: Market Profile
  ValueArea: { make: () => new wickra.ValueArea(20, 50, 0.70), fields: ['poc', 'vah', 'val'], step: (ind, i) => ind.update(high[i], low[i], volume[i]), batch: (ind) => ind.batch(high, low, volume) },
  InitialBalance: { make: () => new wickra.InitialBalance(12), fields: ['high', 'low'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  OpeningRange: { make: () => new wickra.OpeningRange(6), fields: ['high', 'low', 'breakoutDistance'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  DonchianStop: { make: () => new wickra.DonchianStop(10), fields: ['stopLong', 'stopShort'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  // Family 05: bands & channels
  MaEnvelope: { make: () => new wickra.MaEnvelope(20, 0.025), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  AccelerationBands: { make: () => new wickra.AccelerationBands(20, 4.0), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  StarcBands: { make: () => new wickra.StarcBands(6, 15, 2), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  AtrBands: { make: () => new wickra.AtrBands(14, 3), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  HurstChannel: { make: () => new wickra.HurstChannel(10, 0.5), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  LinRegChannel: { make: () => new wickra.LinRegChannel(20, 2), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  StandardErrorBands: { make: () => new wickra.StandardErrorBands(21, 2), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  DoubleBollinger: { make: () => new wickra.DoubleBollinger(20, 1, 2), fields: ['upperOuter', 'upperInner', 'middle', 'lowerInner', 'lowerOuter'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  TtmSqueeze: { make: () => new wickra.TtmSqueeze(20, 2, 1.5), fields: ['squeeze', 'momentum'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  FractalChaosBands: { make: () => new wickra.FractalChaosBands(2), fields: ['upper', 'lower'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  VwapStdDevBands: { make: () => new wickra.VwapStdDevBands(2), fields: ['upper', 'middle', 'lower', 'stddev'], step: (ind, i) => ind.update(high[i], low[i], close[i], volume[i]), batch: (ind) => ind.batch(high, low, close, volume) },
  // Family 08: Pivots & Support/Resistance
  ClassicPivots: { make: () => new wickra.ClassicPivots(), fields: ['pp', 'r1', 'r2', 'r3', 's1', 's2', 's3'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  FibonacciPivots: { make: () => new wickra.FibonacciPivots(), fields: ['pp', 'r1', 'r2', 'r3', 's1', 's2', 's3'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  Camarilla: { make: () => new wickra.Camarilla(), fields: ['pp', 'r1', 'r2', 'r3', 'r4', 's1', 's2', 's3', 's4'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  WoodiePivots: { make: () => new wickra.WoodiePivots(), fields: ['pp', 'r1', 'r2', 's1', 's2'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  DemarkPivots: { make: () => new wickra.DemarkPivots(), fields: ['pp', 'r1', 's1'], step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  WilliamsFractals: { make: () => new wickra.WilliamsFractals(), fields: ['up', 'down'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  ZigZag: { make: () => new wickra.ZigZag(0.02), fields: ['swing', 'direction'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  // Family 11: DeMark
  TDSequential: { make: () => new wickra.TDSequential(4, 9, 2, 13), fields: ['setup', 'countdown', 'direction'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  TDLines: { make: () => new wickra.TDLines(4, 9), fields: ['resistance', 'support'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  TDRangeProjection: { make: () => new wickra.TDRangeProjection(), fields: ['high', 'low'], step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  TDRiskLevel: { make: () => new wickra.TDRiskLevel(4, 9), fields: ['buyRisk', 'sellRisk'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  // Family 10: Ehlers / Cycle (multi-output)
  MAMA: { make: () => new wickra.MAMA(0.5, 0.05), fields: ['mama', 'fama'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  // Family 13: Ichimoku & alternative charts
  Ichimoku: { make: () => new wickra.Ichimoku(9, 26, 52, 26), fields: ['tenkan', 'kijun', 'senkouA', 'senkouB', 'chikou'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  HeikinAshi: { make: () => new wickra.HeikinAshi(), fields: ['open', 'high', 'low', 'close'], step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  FibRetracement: { make: () => new wickra.FibRetracement(), fields: ['level0', 'level236', 'level382', 'level500', 'level618', 'level786', 'level1000'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  FibExtension: { make: () => new wickra.FibExtension(), fields: ['level1272', 'level1414', 'level1618', 'level2000', 'level2618'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  FibProjection: { make: () => new wickra.FibProjection(), fields: ['level618', 'level1000', 'level1618', 'level2618'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  AutoFib: { make: () => new wickra.AutoFib(), fields: ['level0', 'level236', 'level382', 'level500', 'level618', 'level786', 'level1000'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  GoldenPocket: { make: () => new wickra.GoldenPocket(), fields: ['low', 'mid', 'high'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  FibConfluence: { make: () => new wickra.FibConfluence(), fields: ['price', 'strength'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  FibFan: { make: () => new wickra.FibFan(), fields: ['fan382', 'fan500', 'fan618'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  FibArcs: { make: () => new wickra.FibArcs(), fields: ['arc382', 'arc500', 'arc618'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  FibChannel: { make: () => new wickra.FibChannel(), fields: ['base', 'level618', 'level1000', 'level1618'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  FibTimeZones: { make: () => new wickra.FibTimeZones(), fields: ['onZone', 'barsToNext'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  ElderRay: { make: () => new wickra.ElderRay(13), fields: ['bullPower', 'bearPower'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  QQE: { make: () => new wickra.QQE(14, 5, 4.236), fields: ['rsiMa', 'trailingLine'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  GatorOscillator: { make: () => new wickra.GatorOscillator(13, 8, 5), fields: ['upper', 'lower'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  KasePermissionStochastic: { make: () => new wickra.KasePermissionStochastic(9, 3), fields: ['fast', 'slow'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  VolatilityCone: { make: () => new wickra.VolatilityCone(20, 60), fields: ['current', 'min', 'median', 'max', 'percentile'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  QuartileBands: { make: () => new wickra.QuartileBands(4), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  BomarBands: { make: () => new wickra.BomarBands(4, 0.85), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  MedianChannel: { make: () => new wickra.MedianChannel(5, 2.0), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(close[i]), batch: (ind) => ind.batch(close) },
  ProjectionBands: { make: () => new wickra.ProjectionBands(3), fields: ['upper', 'middle', 'lower'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  KaseDevStop: { make: () => new wickra.KaseDevStop(3, 1.0), fields: ['value', 'direction'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  ElderSafeZone: { make: () => new wickra.ElderSafeZone(14, 2.0), fields: ['value', 'direction'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  AtrRatchet: { make: () => new wickra.AtrRatchet(14, 4.0, 0.1), fields: ['value', 'direction'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  Nrtr: { make: () => new wickra.Nrtr(2.0), fields: ['value', 'direction'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  ModifiedMaStop: { make: () => new wickra.ModifiedMaStop(14), fields: ['value', 'direction'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  VolumeWeightedMacd: { make: () => new wickra.VolumeWeightedMacd(12, 26, 9), fields: ['macd', 'signal', 'histogram'], step: (ind, i) => ind.update(close[i], volume[i]), batch: (ind) => ind.batch(close, volume) },
  CentralPivotRange: { make: () => new wickra.CentralPivotRange(), fields: ['pivot', 'tc', 'bc'], step: (ind, i) => ind.update(high[i], low[i], close[i]), batch: (ind) => ind.batch(high, low, close) },
  MurreyMathLines: { make: () => new wickra.MurreyMathLines(4), fields: ['mm8_8', 'mm7_8', 'mm6_8', 'mm5_8', 'mm4_8', 'mm3_8', 'mm2_8', 'mm1_8', 'mm0_8'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  AndrewsPitchfork: { make: () => new wickra.AndrewsPitchfork(2), fields: ['median', 'upper', 'lower'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  VolumeWeightedSr: { make: () => new wickra.VolumeWeightedSr(3), fields: ['support', 'resistance'], step: (ind, i) => ind.update(high[i], low[i], volume[i]), batch: (ind) => ind.batch(high, low, volume) },
  TDMovingAverage: { make: () => new wickra.TDMovingAverage(5, 13), fields: ['st1', 'st2'], step: (ind, i) => ind.update(high[i], low[i]), batch: (ind) => ind.batch(high, low) },
  SmoothedHeikinAshi: { make: () => new wickra.SmoothedHeikinAshi(5), fields: ['open', 'high', 'low', 'close'], step: (ind, i) => ind.update(open[i], high[i], low[i], close[i]), batch: (ind) => ind.batch(open, high, low, close) },
  Equivolume: { make: () => new wickra.Equivolume(20), fields: ['height', 'width'], step: (ind, i) => ind.update(high[i], low[i], volume[i]), batch: (ind) => ind.batch(high, low, volume) },
  CandleVolume: { make: () => new wickra.CandleVolume(20), fields: ['body', 'width'], step: (ind, i) => ind.update(open[i], close[i], volume[i]), batch: (ind) => ind.batch(open, close, volume) },
  HighLowVolumeNodes: { make: () => new wickra.HighLowVolumeNodes(20, 24), fields: ['hvn', 'lvn'], step: (ind, i) => ind.update(high[i], low[i], volume[i]), batch: (ind) => ind.batch(high, low, volume) },
  CompositeProfile: { make: () => new wickra.CompositeProfile(20, 24, 0.7), fields: ['poc', 'vah', 'val'], step: (ind, i) => ind.update(high[i], low[i], volume[i]), batch: (ind) => ind.batch(high, low, volume) },
};

for (const [name, d] of Object.entries(multi)) {
  test(`${name}: streaming update matches interleaved batch`, () => {
    const k = d.fields.length;
    const batch = d.batch(d.make());
    const streaming = d.make();
    assert.equal(batch.length, N * k);
    for (let i = 0; i < N; i++) {
      const o = d.step(streaming, i);
      d.fields.forEach((field, j) => {
        const s = o === null || o === undefined ? NaN : o[field];
        assert.ok(eq(s, batch[i * k + j]), `${name}.${field} mismatch at ${i}`);
      });
    }
  });
}

// --- Lifecycle: every indicator exposes reset / isReady / warmupPeriod ---

test('every indicator exposes reset, isReady and warmupPeriod', () => {
  const all = [
    ...Object.values(scalarFactories).map((f) => f()),
    ...Object.values(candleScalar).map((d) => d.make()),
    ...Object.values(multi).map((d) => d.make()),
  ];
  for (const ind of all) {
    assert.equal(typeof ind.reset, 'function');
    assert.equal(typeof ind.isReady, 'function');
    assert.equal(typeof ind.warmupPeriod, 'function');
    assert.equal(ind.isReady(), false);
    assert.ok(ind.warmupPeriod() >= 1);
  }
});

test('reset returns an indicator to its un-warmed state', () => {
  const sma = new wickra.SMA(5);
  sma.batch([1, 2, 3, 4, 5]);
  assert.equal(sma.isReady(), true);
  sma.reset();
  assert.equal(sma.isReady(), false);
  assert.equal(sma.update(10), null);
});

// --- Reference values ---

test('SMA(3) reference values', () => {
  const out = new wickra.SMA(3).batch([2, 4, 6, 8, 10]);
  assert.ok(Number.isNaN(out[0]) && Number.isNaN(out[1]));
  assert.equal(out[2], 4);
  assert.equal(out[3], 6);
  assert.equal(out[4], 8);
});

test('MFI(2) reference value equals 1200/23', () => {
  // Candle 1 seeds; candle 2 (tp 12 > 10) +mf 1200; candle 3 (tp 11 < 12) -mf 1100.
  const mfi = new wickra.MFI(2);
  assert.equal(mfi.update(10, 10, 10, 100), null);
  assert.equal(mfi.update(12, 12, 12, 100), null);
  const v = mfi.update(11, 11, 11, 100);
  assert.ok(Math.abs(v - 1200 / 23) < 1e-9);
});

test('RSI pure uptrend yields 100', () => {
  const prices = Array.from({ length: 20 }, (_, i) => i + 1);
  const out = new wickra.RSI(14).batch(prices);
  for (let i = 14; i < out.length; i++) {
    assert.equal(out[i], 100);
  }
});

test('MACD histogram equals macd minus signal', () => {
  const macd = new wickra.MACD(12, 26, 9);
  let v = null;
  for (let i = 1; i <= 60; i++) v = macd.update(i);
  assert.ok(v);
  assert.ok(Math.abs(v.histogram - (v.macd - v.signal)) < 1e-9);
});

test('TypicalPrice reference value', () => {
  // (high + low + close) / 3 = (12 + 6 + 9) / 3 = 9.
  assert.equal(new wickra.TypicalPrice().update(12, 6, 9), 9);
});

test('ChaikinMoneyFlow(2) reference value equals 0.5', () => {
  // Bar 1 closes at the high (MFV +100); bar 2 closes mid-range (MFV 0).
  const cmf = new wickra.ChaikinMoneyFlow(2);
  assert.equal(cmf.update(10, 8, 10, 100), null);
  assert.ok(Math.abs(cmf.update(12, 8, 10, 100) - 0.5) < 1e-9);
});

test('LinearRegression(3) reference values', () => {
  // Least-squares line through [1, 2, 9] is y = 4x; endpoint 4·2 = 8.
  const out = new wickra.LinearRegression(3).batch([1, 2, 9]);
  assert.ok(Number.isNaN(out[0]) && Number.isNaN(out[1]));
  assert.ok(Math.abs(out[2] - 8) < 1e-9);
});

test('SuperTrend flat market holds the lower band and an uptrend', () => {
  // Flat candles: ATR 2, hl2 10, lower band 10 - 3·2 = 4.
  const n = 20;
  const out = new wickra.SuperTrend(5, 3).batch(
    Array(n).fill(11),
    Array(n).fill(9),
    Array(n).fill(10),
  );
  assert.ok(Math.abs(out[2 * n - 2] - 4) < 1e-9); // value
  assert.equal(out[2 * n - 1], 1); // direction
});

test('BalanceOfPower reference value', () => {
  // (close - open) / (high - low) = (12 - 10) / (14 - 10) = 0.5.
  assert.ok(Math.abs(new wickra.BalanceOfPower().update(10, 14, 10, 12) - 0.5) < 1e-9);
});

test('TrueRange reference values', () => {
  const tr = new wickra.TrueRange();
  assert.equal(tr.update(12, 8, 11), 4); // no prev close -> high - low
  assert.equal(tr.update(10, 9, 9.5), 2); // prev close 11 -> max(1, 1, 2)
});

test('LinRegAngle of a unit-slope series is 45 degrees', () => {
  const out = new wickra.LinRegAngle(5).batch([1, 2, 3, 4, 5, 6]);
  assert.ok(Math.abs(out[4] - 45) < 1e-9);
});

test('InitialBalance(2) locks after period and ignores subsequent bars', () => {
  const ib = new wickra.InitialBalance(2);
  let v = ib.update(102, 100);
  assert.equal(v.high, 102);
  assert.equal(v.low, 100);
  v = ib.update(103, 99);
  assert.equal(v.high, 103);
  assert.equal(v.low, 99);
  assert.equal(ib.isLocked(), true);
  // Extreme bar after lock must not modify the IB.
  v = ib.update(200, 50);
  assert.equal(v.high, 103);
  assert.equal(v.low, 99);
});

test('OpeningRange(2) breakout distance is signed close minus midpoint', () => {
  const or = new wickra.OpeningRange(2);
  or.update(102, 100, 101);
  or.update(103, 101, 102);
  // OR locked at high 103 / low 100 / mid 101.5. Close 105 -> +3.5.
  const v = or.update(110, 102, 105);
  assert.equal(v.high, 103);
  assert.equal(v.low, 100);
  assert.ok(Math.abs(v.breakoutDistance - 3.5) < 1e-9);
});

// --- Family 12: two-series indicators (Pearson / Beta / Spearman) ---

const pairFactories = {
  PearsonCorrelation: () => new wickra.PearsonCorrelation(14),
  Beta: () => new wickra.Beta(14),
  PairwiseBeta: () => new wickra.PairwiseBeta(14),
  PairSpreadZScore: () => new wickra.PairSpreadZScore(14, 14),
  SpearmanCorrelation: () => new wickra.SpearmanCorrelation(14),
  RollingCorrelation: () => new wickra.RollingCorrelation(20),
  RollingCovariance: () => new wickra.RollingCovariance(20),
  OuHalfLife: () => new wickra.OuHalfLife(60),
  SpreadHurst: () => new wickra.SpreadHurst(60),
  DistanceSsd: () => new wickra.DistanceSsd(20),
  BetaNeutralSpread: () => new wickra.BetaNeutralSpread(20),
  VarianceRatio: () => new wickra.VarianceRatio(60, 2),
  GrangerCausality: () => new wickra.GrangerCausality(60, 1),
  SpreadAr1Coefficient: () => new wickra.SpreadAr1Coefficient(40),
  KendallTau: () => new wickra.KendallTau(20),
  HasbrouckInformationShare: () => new wickra.HasbrouckInformationShare(6),
};

for (const [name, make] of Object.entries(pairFactories)) {
  test(`${name}: streaming update matches batch over a pair of series`, () => {
    const xs = Array.from({ length: N }, (_, i) => Math.sin(i * 0.2) + 0.05 * i);
    const ys = Array.from({ length: N }, (_, i) => Math.cos(i * 0.3) + 0.02 * i);
    const batch = make().batch(xs, ys);
    const streaming = make();
    assert.equal(batch.length, N);
    for (let i = 0; i < N; i++) {
      const s = num(streaming.update(xs[i], ys[i]));
      assert.ok(eq(s, batch[i]), `${name} mismatch at ${i}: ${s} vs ${batch[i]}`);
    }
  });
}

test('PearsonCorrelation perfect positive is 1', () => {
  const x = Array.from({ length: 10 }, (_, i) => i);
  const y = x.map((v) => 2 * v + 3);
  const out = new wickra.PearsonCorrelation(5).batch(x, y);
  assert.ok(Math.abs(out[out.length - 1] - 1) < 1e-9);
});

test('Beta perfect two-to-one', () => {
  const bench = Array.from({ length: 10 }, (_, i) => i);
  const asset = bench.map((v) => 2 * v);
  const out = new wickra.Beta(5).batch(asset, bench);
  assert.ok(Math.abs(out[out.length - 1] - 2) < 1e-9);
});

test('PairwiseBeta squared price is two', () => {
  // b needs varying returns; a = b² ⇒ a's log-returns are exactly 2× b's.
  const bench = Array.from({ length: 20 }, (_, i) => 100 + 10 * Math.sin(i * 0.5));
  const asset = bench.map((v) => v * v);
  const out = new wickra.PairwiseBeta(5).batch(asset, bench);
  assert.ok(Math.abs(out[out.length - 1] - 2) < 1e-9);
});

test('PairSpreadZScore flat benchmark is sign of last move', () => {
  // Flat b ⇒ hedge ratio 0 ⇒ spread = ln(a); z_period = 2 ⇒ z = sign of move.
  const a = [100, 100, 110, 105, 130];
  const b = [100, 100, 100, 100, 100];
  const out = new wickra.PairSpreadZScore(2, 2).batch(a, b);
  assert.ok(Math.abs(out[out.length - 1] - 1) < 1e-9);
  assert.ok(Math.abs(out[out.length - 2] + 1) < 1e-9);
});

const llSignal = (t) =>
  Math.sin(t * 0.4) + 0.4 * Math.sin(t * 1.1) + 0.2 * Math.cos(t * 0.27);

test('LeadLagCrossCorrelation detects positive lead (object output)', () => {
  const ll = new wickra.LeadLagCrossCorrelation(12, 5);
  let last = null;
  // b is a delayed by 3 ⇒ a leads b ⇒ lag = +3.
  for (let t = 0; t < 60; t++) last = ll.update(llSignal(t), llSignal(t - 3));
  assert.equal(last.lag, 3);
  assert.ok(last.correlation > 0.99);
});

test('LeadLagCrossCorrelation batch is flat 2*n with last row matching', () => {
  const n = 60;
  const a = Array.from({ length: n }, (_, t) => llSignal(t));
  const b = Array.from({ length: n }, (_, t) => llSignal(t - 3));
  const out = new wickra.LeadLagCrossCorrelation(12, 5).batch(a, b);
  assert.equal(out.length, 2 * n);
  assert.equal(out[2 * (n - 1)], 3);
  assert.ok(out[2 * (n - 1) + 1] > 0.99);
});

test('Cointegration detects mean-reverting pair (object output)', () => {
  const n = 80;
  const b = Array.from({ length: n }, (_, t) => 50 + 0.5 * t);
  const a = b.map((v, t) => 2 * v + 1 + 0.5 * Math.sin(t * 0.6));
  const co = new wickra.Cointegration(40, 1);
  let last = null;
  for (let i = 0; i < n; i++) last = co.update(a[i], b[i]);
  assert.ok(Math.abs(last.hedgeRatio - 2) < 0.1);
  assert.ok(last.adfStat < -2);
});

test('Cointegration batch is flat 3*n with last row matching', () => {
  const n = 80;
  const b = Array.from({ length: n }, (_, t) => 50 + 0.5 * t);
  const a = b.map((v, t) => 2 * v + 1 + 0.5 * Math.sin(t * 0.6));
  const out = new wickra.Cointegration(40, 1).batch(a, b);
  assert.equal(out.length, 3 * n);
  assert.ok(Math.abs(out[3 * (n - 1)] - 2) < 0.1);
  assert.ok(out[3 * (n - 1) + 2] < -2);
});

test('KalmanHedgeRatio converges to a static hedge ratio (object output)', () => {
  const n = 500;
  const b = Array.from({ length: n }, (_, t) => 100 + 95 * Math.sin(t * 0.5));
  const a = b.map((v) => 2 * v + 5);
  const k = new wickra.KalmanHedgeRatio(1e-2, 1e-3);
  let last = null;
  for (let i = 0; i < n; i++) last = k.update(a[i], b[i]);
  assert.ok(Math.abs(last.hedgeRatio - 2) < 0.05);
  assert.ok(Math.abs(last.spread) < 0.05);
});

test('KalmanHedgeRatio batch is flat 3*n with last row matching', () => {
  const n = 500;
  const b = Array.from({ length: n }, (_, t) => 100 + 95 * Math.sin(t * 0.5));
  const a = b.map((v) => 2 * v + 5);
  const out = new wickra.KalmanHedgeRatio(1e-2, 1e-3).batch(a, b);
  assert.equal(out.length, 3 * n);
  assert.ok(Math.abs(out[3 * (n - 1)] - 2) < 0.05);
  assert.ok(Math.abs(out[3 * (n - 1) + 2]) < 0.05);
});

test('SpreadBollingerBands bands are ordered (object output)', () => {
  const n = 60;
  const b = Array.from({ length: n }, (_, t) => 100 + t);
  const a = b.map((v, t) => v + 3 * Math.sin(t * 0.4));
  const bb = new wickra.SpreadBollingerBands(20, 2.0);
  let last = null;
  for (let i = 0; i < n; i++) last = bb.update(a[i], b[i]);
  assert.ok(last.lower <= last.middle && last.middle <= last.upper);
});

test('SpreadBollingerBands batch is flat 4*n with last row matching', () => {
  const n = 60;
  const b = Array.from({ length: n }, (_, t) => 100 + t);
  const a = b.map((v, t) => v + 3 * Math.sin(t * 0.4));
  const out = new wickra.SpreadBollingerBands(20, 2.0).batch(a, b);
  assert.equal(out.length, 4 * n);
  const base = 4 * (n - 1);
  assert.ok(out[base + 2] <= out[base] && out[base] <= out[base + 1]);
});

test('RelativeStrengthAB constant ratio is flat (object output)', () => {
  const rs = new wickra.RelativeStrengthAB(5, 5);
  let last = null;
  for (let i = 0; i < 30; i++) last = rs.update(200, 100); // ratio is a constant 2
  assert.ok(Math.abs(last.ratio - 2) < 1e-12);
  assert.ok(Math.abs(last.ratioMa - 2) < 1e-12);
  assert.ok(Math.abs(last.ratioRsi - 50) < 1e-9);
});

test('RelativeStrengthAB batch is flat 3*n with last row matching', () => {
  const n = 30;
  const a = Array.from({ length: n }, () => 200);
  const b = Array.from({ length: n }, () => 100);
  const out = new wickra.RelativeStrengthAB(5, 5).batch(a, b);
  assert.equal(out.length, 3 * n);
  assert.ok(Math.abs(out[3 * (n - 1)] - 2) < 1e-12);
  assert.ok(Math.abs(out[3 * (n - 1) + 2] - 50) < 1e-9);
});

test('SpearmanCorrelation monotone non-linear is 1', () => {
  const x = Array.from({ length: 10 }, (_, i) => i + 1);
  const y = x.map((v) => v ** 3);
  const out = new wickra.SpearmanCorrelation(5).batch(x, y);
  assert.ok(Math.abs(out[out.length - 1] - 1) < 1e-9);
});

test('Variance(3) of [2, 4, 6] equals 8/3', () => {
  const out = new wickra.Variance(3).batch([2, 4, 6]);
  assert.ok(Math.abs(out[2] - 8 / 3) < 1e-12);
});

test('RSquared on a perfect line is 1', () => {
  const xs = Array.from({ length: 20 }, (_, i) => 2 * i + 5);
  const out = new wickra.RSquared(5).batch(xs);
  for (let i = 5; i < out.length; i++) {
    assert.ok(Math.abs(out[i] - 1) < 1e-9);
  }
});

test('MedianAbsoluteDeviation ignores a single huge outlier', () => {
  const xs = Array(9).fill(5).concat([1000]);
  const out = new wickra.MedianAbsoluteDeviation(10).batch(xs);
  assert.ok(Math.abs(out[9]) < 1e-12);
});

test('Autocorrelation of an alternating series is strongly negative at lag 1', () => {
  const xs = Array.from({ length: 20 }, (_, i) => (i % 2 === 0 ? -1 : 1));
  const out = new wickra.Autocorrelation(10, 1).batch(xs);
  assert.ok(out[out.length - 1] < -0.5);
});

test('HurstExponent of a monotone ramp is above 0.5', () => {
  const xs = Array.from({ length: 200 }, (_, i) => i);
  const out = new wickra.HurstExponent(100, 4).batch(xs);
  assert.ok(out[out.length - 1] > 0.5);
});

test('Ichimoku emits from the first bar and tenkan fills in at bar 9', () => {
  const ichi = new wickra.Ichimoku(9, 26, 52, 26);
  // A row is emitted from the first bar -- every component is optional and
  // they fill in as history allows -- so the declared warmup is 1. The 77 bars
  // the classic (9, 26, 52, 26) configuration needs are when the LAST component
  // arrives, which is a different question from when a value first appears.
  assert.equal(ichi.warmupPeriod(), 1);
  const n = 30;
  const h = Array.from({ length: n }, (_, i) => 100 + i + 2);
  const l = Array.from({ length: n }, (_, i) => 100 + i - 2);
  const c = Array.from({ length: n }, (_, i) => 100 + i + 1);
  const out = ichi.batch(h, l, c);
  for (let i = 0; i < 8; i++) {
    assert.ok(Number.isNaN(out[i * 5]), `tenkan should be NaN at bar ${i}`);
  }
  assert.ok(!Number.isNaN(out[8 * 5]), 'tenkan should be defined at bar 9');
});

test('HeikinAshi first bar seeds from real open and close', () => {
  const ha = new wickra.HeikinAshi();
  const out = ha.update(10, 12, 9, 11);
  assert.ok(Math.abs(out.open - (10 + 11) / 2) < 1e-12);
  assert.ok(Math.abs(out.close - (10 + 12 + 9 + 11) / 4) < 1e-12);
});

test('PercentageTrailingStop seeds and ratchets', () => {
  const s = new wickra.PercentageTrailingStop(10);
  assert.ok(Math.abs(s.update(100) - 90) < 1e-9);
  assert.ok(Math.abs(s.update(110) - 99) < 1e-9);
});

test('RenkoTrailingStop only advances after a full block', () => {
  const s = new wickra.RenkoTrailingStop(1);
  assert.ok(Math.abs(s.update(100) - 99) < 1e-9);
  assert.ok(Math.abs(s.update(100.5) - 99) < 1e-9);
  assert.ok(Math.abs(s.update(101) - 100) < 1e-9);
});

test('DonchianStop window extremes', () => {
  const out = new wickra.DonchianStop(5).batch([1, 2, 3, 4, 5], [0, 1, 2, 3, 4]);
  // [long0, short0, long1, short1, ...]: idx 8 (=4*2) = long_5th, idx 9 = short_5th.
  assert.ok(Math.abs(out[8] - 0) < 1e-9);
  assert.ok(Math.abs(out[9] - 5) < 1e-9);
});

test('MaEnvelope reference values', () => {
  // SMA([10, 20, 30]) = 20; with percent 0.10: upper=22, lower=18.
  const out = new wickra.MaEnvelope(3, 0.10).batch([10, 20, 30]);
  assert.ok(Number.isNaN(out[0]) && Number.isNaN(out[3]));
  assert.ok(Math.abs(out[2 * 3 + 0] - 22) < 1e-9); // upper
  assert.ok(Math.abs(out[2 * 3 + 1] - 20) < 1e-9); // middle
  assert.ok(Math.abs(out[2 * 3 + 2] - 18) < 1e-9); // lower
});

test('AccelerationBands single-bar reference', () => {
  // high=12, low=8, close=10, factor=0.5, period=1.
  // ratio=0.2, raw_up=13.2, raw_lo=7.2.
  const v = new wickra.AccelerationBands(1, 0.5).update(12, 8, 10);
  assert.ok(Math.abs(v.upper - 13.2) < 1e-9);
  assert.ok(Math.abs(v.middle - 10) < 1e-9);
  assert.ok(Math.abs(v.lower - 7.2) < 1e-9);
});

test('LinRegChannel reference values for [1, 2, 9]', () => {
  // Line y=4x, endpoint=8, residuals=[1,-2,1], sigma=sqrt(2).
  const out = new wickra.LinRegChannel(3, 2).batch([1, 2, 9]);
  const s = Math.sqrt(2);
  const i = 2;
  assert.ok(Math.abs(out[i * 3 + 0] - (8 + 2 * s)) < 1e-9);
  assert.ok(Math.abs(out[i * 3 + 1] - 8) < 1e-9);
  assert.ok(Math.abs(out[i * 3 + 2] - (8 - 2 * s)) < 1e-9);
});

test('VwapStdDevBands two-bar reference', () => {
  const v = new wickra.VwapStdDevBands(1.5);
  v.update(8, 8, 8, 1);
  const o = v.update(12, 12, 12, 1);
  assert.ok(Math.abs(o.upper - 13) < 1e-9);
  assert.ok(Math.abs(o.middle - 10) < 1e-9);
  assert.ok(Math.abs(o.lower - 7) < 1e-9);
  assert.ok(Math.abs(o.stddev - 2) < 1e-9);
});

test('RVIVolatility pure uptrend saturates at 100', () => {
  const prices = Array.from({ length: 40 }, (_, i) => i + 1);
  const out = new wickra.RVIVolatility(5).batch(prices);
  for (let i = 9; i < out.length; i++) {
    assert.ok(Math.abs(out[i] - 100) < 1e-9, `RVIVolatility[${i}] = ${out[i]}`);
  }
});

test('ParkinsonVolatility zero-range bars yield zero', () => {
  const n = 30;
  const h = Array(n).fill(10);
  const l = Array(n).fill(10);
  const out = new wickra.ParkinsonVolatility(14, 252).batch(h, l);
  for (let i = 13; i < n; i++) {
    assert.ok(Math.abs(out[i]) < 1e-12, `Parkinson[${i}] = ${out[i]}`);
  }
});

test('GarmanKlassVolatility zero-movement bars yield zero', () => {
  const n = 30;
  const flat = Array(n).fill(10);
  const out = new wickra.GarmanKlassVolatility(14, 252).batch(flat, flat, flat, flat);
  for (let i = 13; i < n; i++) {
    assert.ok(Math.abs(out[i]) < 1e-12, `GK[${i}] = ${out[i]}`);
  }
});

test('RogersSatchellVolatility zero-movement bars yield zero', () => {
  const n = 30;
  const flat = Array(n).fill(10);
  const out = new wickra.RogersSatchellVolatility(14, 252).batch(flat, flat, flat, flat);
  for (let i = 13; i < n; i++) {
    assert.ok(Math.abs(out[i]) < 1e-12, `RS[${i}] = ${out[i]}`);
  }
});

test('YangZhangVolatility zero-movement bars yield zero', () => {
  const n = 30;
  const flat = Array(n).fill(10);
  const out = new wickra.YangZhangVolatility(14, 252).batch(flat, flat, flat, flat);
  for (let i = 14; i < n; i++) {
    assert.ok(Math.abs(out[i]) < 1e-12, `YZ[${i}] = ${out[i]}`);
  }
});

test('ZeroLagMACD on a flat series converges to zero', () => {
  const out = new wickra.ZeroLagMACD(3, 5, 3).batch(Array(60).fill(42));
  // Last interleaved row: macd, signal, histogram all 0.
  const n = 60;
  assert.ok(Math.abs(out[(n - 1) * 3]) < 1e-12);
  assert.ok(Math.abs(out[(n - 1) * 3 + 1]) < 1e-12);
  assert.ok(Math.abs(out[(n - 1) * 3 + 2]) < 1e-12);
});

test('AwesomeOscillatorHistogram on a flat median converges to zero', () => {
  const n = 50;
  const out = new wickra.AwesomeOscillatorHistogram(3, 5, 3).batch(
    Array(n).fill(11),
    Array(n).fill(9),
  );
  // AO momentum; warmup = slow + lookback = 5 + 3 = 8.
  for (let i = 7; i < n; i++) assert.ok(Math.abs(out[i]) < 1e-12);
});

test('STC on a flat series stays at zero', () => {
  const out = new wickra.STC(3, 5, 4, 0.5).batch(Array(60).fill(42));
  // Latest values must be exactly zero.
  for (let i = out.length - 5; i < out.length; i++) {
    if (Number.isNaN(out[i])) continue;
    assert.equal(out[i], 0);
  }
});

test('ElderImpulse on a flat series stays neutral (0)', () => {
  const out = new wickra.ElderImpulse(13, 12, 26, 9).batch(Array(120).fill(42));
  for (let i = 0; i < out.length; i++) {
    if (Number.isNaN(out[i])) continue;
    assert.equal(out[i], 0);
  }
});

test('CFO(5) on a perfectly linear series yields zero', () => {
  const prices = Array.from({ length: 20 }, (_, i) => (i + 1) * 2);
  const out = new wickra.CFO(5).batch(prices);
  for (let i = 4; i < 20; i++) assert.ok(Math.abs(out[i]) < 1e-9);
});

test('APO(3, 5) on a flat series converges to zero', () => {
  const out = new wickra.APO(3, 5).batch(Array(30).fill(42));
  for (let i = 0; i < 4; i++) assert.ok(Number.isNaN(out[i]));
  for (let i = 4; i < 30; i++) assert.ok(Math.abs(out[i]) < 1e-12);
});

test('Inertia(3, 4) on a constant close equals the neutral Relative Volatility Index', () => {
  const n = 60;
  // Every bar (open, high, low, close) = (10, 11, 9, 10.5): the close never
  // moves, so the Relative Volatility Index sits at its neutral 50.
  const out = new wickra.Inertia(3, 4).batch(
    Array(n).fill(10),
    Array(n).fill(11),
    Array(n).fill(9),
    Array(n).fill(10.5),
  );
  for (let i = 7; i < n; i++) assert.ok(Math.abs(out[i] - 50) < 1e-12);
});

test('ConnorsRSI stays bounded in [0, 100]', () => {
  const prices = Array.from({ length: 250 }, (_, i) => 100 + 20 * Math.sin(i * 0.12));
  const out = new wickra.ConnorsRSI(3, 2, 100).batch(prices);
  for (let i = 0; i < out.length; i++) {
    if (Number.isNaN(out[i])) continue;
    assert.ok(out[i] >= 0 && out[i] <= 100, `out[${i}] = ${out[i]}`);
  }
});

test('LaguerreRSI on a flat series stays at the neutral 50', () => {
  const out = new wickra.LaguerreRSI(0.5).batch(Array(40).fill(42));
  for (let i = 0; i < out.length; i++) assert.ok(Math.abs(out[i] - 50) < 1e-12);
});

test('SMI with close at range centre emits zero after warmup', () => {
  const n = 60;
  const out = new wickra.SMI(5, 3, 3).batch(Array(n).fill(11), Array(n).fill(9), Array(n).fill(10));
  // warmup_period = 5 + 3 + 3 - 2 = 9.
  for (let i = 8; i < n; i++) assert.ok(Math.abs(out[i]) < 1e-12);
});

test('KST on a flat series emits zero after warmup', () => {
  const kst = new wickra.KST(10, 15, 20, 30, 10, 10, 10, 15, 9);
  const n = 80;
  const out = kst.batch(Array(n).fill(42));
  const warmup = kst.warmupPeriod();
  for (let i = warmup - 1; i < n; i++) {
    assert.ok(Math.abs(out[i * 2]) < 1e-12, `kst[${i}] = ${out[i * 2]}`);
    assert.ok(Math.abs(out[i * 2 + 1]) < 1e-12, `signal[${i}] = ${out[i * 2 + 1]}`);
  }
});

test('PGO(5) on a flat close emits zero after warmup', () => {
  const n = 20;
  const out = new wickra.PGO(5).batch(Array(n).fill(11), Array(n).fill(9), Array(n).fill(10));
  for (let i = 0; i < 4; i++) assert.ok(Number.isNaN(out[i]));
  for (let i = 4; i < n; i++) assert.ok(Math.abs(out[i]) < 1e-12, `out[${i}] = ${out[i]}`);
});

test('RVI(2) reference value with the 1-2-2-1 weighting', () => {
  // Four bars (10, 11, 9, 10.5) then (10.5, 11.5, 10, 11.5):
  // RVI = (0.5 + 3.5/6) / (2 + 11.5/6) = 6.5 / 23.5.
  const out = new wickra.RVI(2).batch(
    [10, 10, 10, 10, 10.5],
    [11, 11, 11, 11, 11.5],
    [9, 9, 9, 9, 10],
    [10.5, 10.5, 10.5, 10.5, 11.5],
  );
  for (let i = 0; i < 4; i++) assert.ok(Number.isNaN(out[i]));
  assert.ok(Math.abs(out[4] - 6.5 / 23.5) < 1e-12);
});

test('EVWMA(2) reference values on [10, 20, 30] with volumes [1, 3, 1]', () => {
  const out = new wickra.EVWMA(2).batch([10, 20, 30], [1, 3, 1]);
  assert.ok(Number.isNaN(out[0]));
  assert.ok(Math.abs(out[1] - 20) < 1e-12);
  assert.ok(Math.abs(out[2] - 22.5) < 1e-12);
});

test('Alligator on a flat median price seeds to that median', () => {
  const n = 30;
  const out = new wickra.Alligator(13, 8, 5).batch(Array(n).fill(11), Array(n).fill(9));
  // All three SMMAs see median (11 + 9) / 2 = 10 every bar.
  for (let i = 12; i < n; i++) {
    assert.ok(Math.abs(out[i * 3] - 10) < 1e-12, `jaw at ${i}: ${out[i * 3]}`);
    assert.ok(Math.abs(out[i * 3 + 1] - 10) < 1e-12);
    assert.ok(Math.abs(out[i * 3 + 2] - 10) < 1e-12);
  }
});

test('JMA on a flat series reproduces the constant', () => {
  const out = new wickra.JMA(14, 0, 2).batch(Array(30).fill(42));
  for (let i = 0; i < 30; i++) assert.ok(Math.abs(out[i] - 42) < 1e-12);
});

test('VIDYA on a flat series holds the seed', () => {
  const out = new wickra.VIDYA(14, 4).batch(Array(20).fill(42));
  for (let i = 0; i < 4; i++) assert.ok(Number.isNaN(out[i]));
  for (let i = 4; i < 20; i++) assert.ok(Math.abs(out[i] - 42) < 1e-12);
});

test('FRAMA pure uptrend hugs the latest close', () => {
  const out = new wickra.FRAMA(4).batch([1, 2, 3, 4, 5, 6, 7, 8]);
  assert.ok(Math.abs(out[out.length - 1] - 8) < 0.05);
});

test('McGinleyDynamic(3) seeds with SMA and recurses on the next price', () => {
  // Seed = SMA([10, 20, 30]) = 20. On 40: ratio = 2, divisor = 0.6*3*16 = 28.8.
  const out = new wickra.McGinleyDynamic(3).batch([10, 20, 30, 40]);
  assert.ok(Number.isNaN(out[0]) && Number.isNaN(out[1]));
  assert.ok(Math.abs(out[2] - 20) < 1e-12);
  const expected = 20 + 20 / (0.6 * 3 * 16);
  assert.ok(Math.abs(out[3] - expected) < 1e-12);
});

test('ALMA(3, 0.85, 6) reference value on [10, 20, 30]', () => {
  // m = 0.85 * 2 = 1.7; s = 3 / 6 = 0.5; 2*s^2 = 0.5.
  const out = new wickra.ALMA(3, 0.85, 6).batch([10, 20, 30]);
  assert.ok(Number.isNaN(out[0]) && Number.isNaN(out[1]));
  const w = [0, 1, 2].map((i) => Math.exp(-Math.pow(i - 1.7, 2) / 0.5));
  const s = w[0] + w[1] + w[2];
  const expected = (10 * w[0] + 20 * w[1] + 30 * w[2]) / s;
  assert.ok(Math.abs(out[2] - expected) < 1e-12);
  // The heavy offset toward the newest sample lifts the average above the
  // simple mean of 20.
  assert.ok(out[2] > 20);
});

test('Doji signed mode encodes dragonfly/gravestone/neutral direction', () => {
  // Default: direction-less detection flag (+1 doji / 0 otherwise).
  const flag = new wickra.Doji();
  assert.equal(flag.isSigned(), false);
  assert.equal(flag.update(10, 11, 9, 10), 1); // body 0, range 2 -> doji
  assert.equal(flag.update(10, 12, 10, 12), 0); // body == range -> not a doji

  // Signed: classify a detected doji by its body position within the range.
  const d = new wickra.Doji(true);
  assert.equal(d.isSigned(), true);
  assert.equal(d.update(10, 10.05, 6, 10), 1); // dragonfly -> bullish +1
  assert.equal(d.update(10, 14, 9.95, 10), -1); // gravestone -> bearish -1
  assert.equal(d.update(10, 12, 8, 10), 0); // long-legged -> neutral 0
  assert.equal(d.update(10, 12, 10, 12), 0); // not a doji -> 0
});

test('order-book indicators reference values', () => {
  // Top-1: (3 - 1) / (3 + 1) = 0.5.
  assert.equal(new wickra.OrderBookImbalanceTop1().update([100], [3], [101], [1]), 0.5);
  // Top-2: bidDepth 3, askDepth 2 -> (3 - 2) / 5 = 0.2.
  assert.ok(
    Math.abs(new wickra.OrderBookImbalanceTopN(2).update([100, 99], [2, 1], [101, 102], [1, 1]) - 0.2) < 1e-12,
  );
  // Full: bidDepth 1, askDepth 3 -> -0.5.
  assert.equal(new wickra.OrderBookImbalanceFull().update([100], [1], [101, 102], [2, 1]), -0.5);
  // Microprice: (100*3 + 101*1) / 4 = 100.25.
  assert.equal(new wickra.Microprice().update([100], [1], [101], [3]), 100.25);
  // Quoted spread: 1 / 100.5 * 10000 ≈ 99.5025 bps.
  assert.ok(Math.abs(new wickra.QuotedSpread().update([100], [1], [101], [1]) - 99.50248756) < 1e-6);
  // Depth slope: each side distances 1,2 -> cumulative 1,3 -> OLS slope 2.
  assert.ok(Math.abs(new wickra.DepthSlope().update([99, 98], [1, 2], [101, 102], [1, 2]) - 2.0) < 1e-9);
  // Single level per side -> no slope -> 0.
  assert.equal(new wickra.DepthSlope().update([100], [1], [101], [1]), 0.0);
});

test('order-book streaming update matches batch', () => {
  const snaps = Array.from({ length: 30 }, (_, i) => ({
    bidPx: [100, 99],
    bidSz: [1 + (i % 5), 1],
    askPx: [101, 102],
    askSz: [1 + ((i + 1) % 3), 1],
  }));
  const batch = new wickra.Microprice().batch(snaps);
  const streamer = new wickra.Microprice();
  assert.equal(batch.length, snaps.length);
  for (let i = 0; i < snaps.length; i++) {
    const s = streamer.update(snaps[i].bidPx, snaps[i].bidSz, snaps[i].askPx, snaps[i].askSz);
    assert.ok(Math.abs(s - batch[i]) < 1e-12, `mismatch at ${i}: ${s} vs ${batch[i]}`);
  }
});

test('order-book TopN rejects zero levels', () => {
  assert.throws(() => new wickra.OrderBookImbalanceTopN(0));
});

test('order-book update rejects a crossed book', () => {
  assert.throws(() => new wickra.QuotedSpread().update([102], [1], [101], [1]));
});

test('trade-flow indicators reference values', () => {
  assert.equal(new wickra.SignedVolume().update(100, 2, true), 2);
  assert.equal(new wickra.SignedVolume().update(100, 3, false), -3);
  const cvd = new wickra.CumulativeVolumeDelta();
  assert.equal(cvd.update(100, 5, true), 5);
  assert.equal(cvd.update(100, 2, false), 3);
  const ti = new wickra.TradeImbalance(2);
  assert.equal(ti.update(100, 3, true), null); // warming up
  assert.equal(ti.update(100, 1, false), 0.5); // (3 - 1) / 4
});

test('trade-flow streaming update matches batch', () => {
  const n = 30;
  const price = Array.from({ length: n }, () => 100);
  const size = Array.from({ length: n }, (_, i) => 1 + (i % 4));
  const isBuy = Array.from({ length: n }, (_, i) => i % 3 !== 0);
  const batch = new wickra.CumulativeVolumeDelta().batch(price, size, isBuy);
  const streamer = new wickra.CumulativeVolumeDelta();
  assert.equal(batch.length, n);
  for (let i = 0; i < n; i++) {
    const s = streamer.update(price[i], size[i], isBuy[i]);
    assert.ok(Math.abs(s - batch[i]) < 1e-12, `mismatch at ${i}: ${s} vs ${batch[i]}`);
  }
});

test('trade-flow rejects bad input', () => {
  assert.throws(() => new wickra.TradeImbalance(0));
  assert.throws(() => new wickra.SignedVolume().update(100, -1, true));
});

test('order-flow imbalance reference + streaming matches batch', () => {
  // Rising bid (px up, size 6) with an unchanged ask -> +6 flow.
  const ofi = new wickra.OrderFlowImbalance(1);
  assert.equal(ofi.update([100], [5], [101], [4]), null); // seeds the reference
  assert.ok(Math.abs(ofi.update([100.5], [6], [101], [4]) - 6.0) < 1e-12);
  const snaps = Array.from({ length: 30 }, (_, i) => ({
    bidPx: [100 + Math.sin(i * 0.3)],
    bidSz: [5 + Math.abs(Math.cos(i * 0.5))],
    askPx: [101 + Math.sin(i * 0.3)],
    askSz: [4 + Math.abs(Math.sin(i * 0.4))],
  }));
  const batch = new wickra.OrderFlowImbalance(10).batch(snaps);
  const streamer = new wickra.OrderFlowImbalance(10);
  assert.equal(batch.length, snaps.length);
  for (let i = 0; i < snaps.length; i++) {
    const s = streamer.update(snaps[i].bidPx, snaps[i].bidSz, snaps[i].askPx, snaps[i].askSz);
    assert.ok((Number.isNaN(batch[i]) && s === null) || Math.abs(s - batch[i]) < 1e-9, `mismatch at ${i}`);
  }
});

test('vpin / amihud / roll reference + streaming matches batch', () => {
  // VPIN: two pure-buy buckets of size 10 -> imbalance == size -> 1.
  const v = new wickra.Vpin(10, 2);
  let last;
  for (let i = 0; i < 4; i++) last = v.update(100, 5, true);
  assert.equal(last, 1.0);
  // Amihud(1): |ln(101/100)| / (101 * 10).
  const a = new wickra.AmihudIlliquidity(1);
  assert.equal(a.update(100, 10, true), null);
  assert.ok(Math.abs(a.update(101, 10, true) - Math.abs(Math.log(101 / 100)) / (101 * 10)) < 1e-15);
  // Roll(6): a clean bid-ask bounce of ±1 implies a spread of 2.
  const r = new wickra.RollMeasure(6);
  let roll = null;
  for (let i = 0; i < 20; i++) roll = r.update(i % 2 === 0 ? 100 : 101, 1, true);
  assert.ok(Math.abs(roll - 2.0) < 1e-12);
  // Streaming-vs-batch for the three trade-input indicators.
  const n = 40;
  const price = Array.from({ length: n }, (_, i) => 100 + Math.sin(i * 0.25) * 4);
  const size = Array.from({ length: n }, (_, i) => 1 + (i % 5));
  const isBuy = Array.from({ length: n }, (_, i) => i % 2 === 0);
  for (const make of [() => new wickra.Vpin(8, 5), () => new wickra.AmihudIlliquidity(14), () => new wickra.RollMeasure(14), () => new wickra.TradeSignAutocorrelation(10), () => new wickra.Pin(10)]) {
    const batch = make().batch(price, size, isBuy);
    const streamer = make();
    assert.equal(batch.length, n);
    for (let i = 0; i < n; i++) {
      const s = streamer.update(price[i], size[i], isBuy[i]);
      assert.ok((Number.isNaN(batch[i]) && s === null) || Math.abs(s - batch[i]) < 1e-9, `mismatch at ${i}`);
    }
  }
  // Trade-sign autocorrelation: alternating signs -> -1, all buys -> +1.
  let tsac = null;
  const tsacInd = new wickra.TradeSignAutocorrelation(10);
  for (let i = 0; i < 20; i++) tsac = tsacInd.update(100, 1, i % 2 === 0);
  assert.ok(Math.abs(tsac - -1.0) < 1e-12);
  // PIN: one-sided flow -> 1, balanced flow -> 0.
  let pin = null;
  const pinInd = new wickra.Pin(10);
  for (let i = 0; i < 20; i++) pin = pinInd.update(100, 1, true);
  assert.ok(Math.abs(pin - 1.0) < 1e-12);
});

test('price-impact indicators reference values', () => {
  // Buy at 100.05 vs mid 100.0: 2 * (100.05 - 100) / 100 * 10000 = 10 bps.
  assert.ok(Math.abs(new wickra.EffectiveSpread().update(100.05, 1, true, 100.0) - 10.0) < 1e-9);
  // Sell at 99.95 vs mid 100.0: 2 * -1 * (99.95 - 100) / 100 * 10000 = 10 bps.
  assert.ok(Math.abs(new wickra.EffectiveSpread().update(99.95, 1, false, 100.0) - 10.0) < 1e-9);
  // A buy filled below the mid is price improvement -> negative.
  assert.ok(new wickra.EffectiveSpread().update(99.95, 1, true, 100.0) < 0.0);
});

test('price-impact streaming update matches batch', () => {
  const n = 30;
  const mid = Array.from({ length: n }, (_, i) => 100 + 0.25 * Math.sin(i * 0.5));
  const isBuy = Array.from({ length: n }, (_, i) => i % 3 !== 0);
  const price = Array.from({ length: n }, (_, i) => mid[i] + (isBuy[i] ? 0.03 : -0.03));
  const size = Array.from({ length: n }, (_, i) => 1 + (i % 4));
  const batch = new wickra.EffectiveSpread().batch(price, size, isBuy, mid);
  const streamer = new wickra.EffectiveSpread();
  assert.equal(batch.length, n);
  for (let i = 0; i < n; i++) {
    const s = streamer.update(price[i], size[i], isBuy[i], mid[i]);
    assert.ok(Math.abs(s - batch[i]) < 1e-9, `mismatch at ${i}: ${s} vs ${batch[i]}`);
  }
});

test('realized spread resolves against the future mid', () => {
  const rs = new wickra.RealizedSpread(1);
  assert.equal(rs.update(100.10, 1, true, 100.0), null); // buffered
  // 2 * (+1) * (100.10 - 100.20) / 100.0 * 10000 = -20 bps.
  assert.ok(Math.abs(rs.update(99.90, 1, false, 100.20) - -20.0) < 1e-9);
});

test('realized spread streaming update matches batch', () => {
  const n = 30;
  const mid = Array.from({ length: n }, (_, i) => 100 + 0.25 * Math.sin(i * 0.5));
  const isBuy = Array.from({ length: n }, (_, i) => i % 3 !== 0);
  const price = Array.from({ length: n }, (_, i) => mid[i] + (isBuy[i] ? 0.03 : -0.03));
  const size = Array.from({ length: n }, (_, i) => 1 + (i % 4));
  const batch = new wickra.RealizedSpread(4).batch(price, size, isBuy, mid);
  const streamer = new wickra.RealizedSpread(4);
  assert.equal(batch.length, n);
  for (let i = 0; i < n; i++) {
    const s = streamer.update(price[i], size[i], isBuy[i], mid[i]);
    const got = s === null ? NaN : s;
    assert.ok(
      (Number.isNaN(got) && Number.isNaN(batch[i])) || Math.abs(got - batch[i]) < 1e-9,
      `mismatch at ${i}: ${got} vs ${batch[i]}`,
    );
  }
});

test("kyle's lambda recovers a constant price-impact slope", () => {
  // Each trade moves the mid by exactly 0.5 per unit of signed volume.
  const impact = 0.5;
  let mid = 100;
  const price = [];
  const size = [];
  const isBuy = [];
  const mids = [];
  for (let i = 0; i < 20; i++) {
    const buy = i % 2 === 0;
    const sz = 1 + (i % 3);
    const signed = buy ? sz : -sz;
    mid += impact * signed;
    price.push(mid);
    size.push(sz);
    isBuy.push(buy);
    mids.push(mid);
  }
  const out = new wickra.KylesLambda(6).batch(price, size, isBuy, mids);
  assert.ok(Math.abs(out[out.length - 1] - 0.5) < 1e-9);
});

test('price-impact rejects bad input', () => {
  assert.throws(() => new wickra.EffectiveSpread().update(100, 1, true, 0));
  assert.throws(() => new wickra.RealizedSpread(0));
  assert.throws(() => new wickra.KylesLambda(1));
});

test('footprint buckets buy and sell volume per price level', () => {
  const fp = new wickra.Footprint(1.0);
  fp.update(100.2, 2, true); // bucket 100 -> ask 2
  fp.update(100.7, 3, false); // bucket 101 -> bid 3
  const out = fp.update(100.1, 1, true); // bucket 100 -> ask 3
  assert.equal(out.length, 2);
  assert.deepEqual(
    { price: out[0].price, bidVol: out[0].bidVol, askVol: out[0].askVol },
    { price: 100.0, bidVol: 0.0, askVol: 3.0 },
  );
  assert.deepEqual(
    { price: out[1].price, bidVol: out[1].bidVol, askVol: out[1].askVol },
    { price: 101.0, bidVol: 3.0, askVol: 0.0 },
  );
});

test('footprint streaming update matches batch and rejects bad tick', () => {
  const n = 12;
  const price = Array.from({ length: n }, (_, i) => 100 + (i % 5) * 0.3);
  const size = Array.from({ length: n }, (_, i) => 1 + (i % 3));
  const isBuy = Array.from({ length: n }, (_, i) => i % 2 === 0);
  const batch = new wickra.Footprint(1.0).batch(price, size, isBuy);
  const streamer = new wickra.Footprint(1.0);
  assert.equal(batch.length, n);
  for (let i = 0; i < n; i++) {
    const s = streamer.update(price[i], size[i], isBuy[i]);
    assert.deepEqual(s, batch[i], `mismatch at ${i}`);
  }
  assert.throws(() => new wickra.Footprint(0));
});

test('derivatives indicators reference values', () => {
  // Funding rate passes through (and may be negative).
  assert.equal(new wickra.FundingRate().update(0.0001), 0.0001);
  assert.equal(new wickra.FundingRate().update(-0.0003), -0.0003);
  // Rolling mean: window [0.001, 0.003] -> 0.002.
  const frm = new wickra.FundingRateMean(2);
  assert.equal(frm.update(0.001), null); // warming up
  assert.ok(Math.abs(frm.update(0.003) - 0.002) < 1e-12);
  // Z-score: window [0.001, 0.003] -> +1.
  const z = new wickra.FundingRateZScore(2);
  assert.equal(z.update(0.001), null); // warming up
  assert.ok(Math.abs(z.update(0.003) - 1.0) < 1e-9);
  // Basis: mark 100.5 vs index 100.0 -> 0.005.
  assert.ok(Math.abs(new wickra.FundingBasis().update(100.5, 100.0) - 0.005) < 1e-12);
  // OI delta: seeds then emits the change.
  const oid = new wickra.OpenInterestDelta();
  assert.equal(oid.update(1000), null);
  assert.equal(oid.update(1250), 250);
  assert.equal(oid.update(1100), -150);
});

test('derivatives streaming update matches batch', () => {
  const n = 30;
  const rate = Array.from({ length: n }, (_, i) => 0.0001 * Math.sin(i * 0.3));
  const batch = new wickra.FundingRateMean(5).batch(rate);
  const streamer = new wickra.FundingRateMean(5);
  assert.equal(batch.length, n);
  for (let i = 0; i < n; i++) {
    const s = streamer.update(rate[i]);
    assert.ok(
      (s === null && Number.isNaN(batch[i])) || Math.abs(s - batch[i]) < 1e-12,
      `mismatch at ${i}: ${s} vs ${batch[i]}`,
    );
  }
});

test('derivatives reject bad input', () => {
  assert.throws(() => new wickra.FundingRateMean(0));
  assert.throws(() => new wickra.FundingRateZScore(0));
  assert.throws(() => new wickra.FundingBasis().update(100, 0));
});

test('B16 derivatives reference values', () => {
  // Estimated leverage: oi / exchange reserve = 200 / 100 = 2.
  assert.ok(Math.abs(new wickra.EstimatedLeverageRatio().update(200, 100) - 2.0) < 1e-12);
  // OI-to-volume: oi / (buy + sell) = 100 / 50 = 2.
  assert.ok(Math.abs(new wickra.OiToVolumeRatio().update(100, 30, 20) - 2.0) < 1e-12);
  // Perpetual premium: (mark - index) / index = 0.5 / 100 = 0.005.
  assert.ok(Math.abs(new wickra.PerpetualPremiumIndex().update(100.5, 100.0) - 0.005) < 1e-12);
  // Funding-implied APR: rate * intervals = 0.0001 * 1095 = 0.1095.
  assert.ok(Math.abs(new wickra.FundingImpliedApr(1095).update(0.0001) - 0.1095) < 1e-12);
  // Open-interest momentum (period 2): warmup then ROC% = 100*(120 - 100)/100 = 20.
  const oim = new wickra.OpenInterestMomentum(2);
  assert.equal(oim.update(100), null);
  assert.equal(oim.update(110), null);
  assert.ok(Math.abs(oim.update(120) - 20.0) < 1e-12);
});

test('B16 derivatives streaming matches batch', () => {
  const n = 30;
  const oi = Array.from({ length: n }, (_, i) => 1000 + 50 * Math.sin(i * 0.3));
  const longSz = Array.from({ length: n }, (_, i) => 600 + 20 * Math.cos(i * 0.2));
  const shortSz = Array.from({ length: n }, (_, i) => 400 + 15 * Math.sin(i * 0.4));
  const buy = Array.from({ length: n }, (_, i) => 300 + 10 * Math.sin(i * 0.5));
  const sell = Array.from({ length: n }, (_, i) => 250 + 12 * Math.cos(i * 0.35));
  const index = Array.from({ length: n }, (_, i) => 100 + Math.sin(i * 0.2));
  const mark = Array.from({ length: n }, (_, i) => index[i] + 0.05 * Math.cos(i * 0.3));
  const rate = Array.from({ length: n }, (_, i) => 0.0001 * Math.sin(i * 0.3));
  const cmp = (batch, s, i) =>
    assert.ok((s === null && Number.isNaN(batch[i])) || Math.abs(s - batch[i]) < 1e-12, `mismatch at ${i}`);

  const reserve = longSz.map((l, i) => l + shortSz[i]);
  let b = new wickra.EstimatedLeverageRatio().batch(oi, reserve);
  let st = new wickra.EstimatedLeverageRatio();
  for (let i = 0; i < n; i++) cmp(b, st.update(oi[i], reserve[i]), i);

  b = new wickra.OiToVolumeRatio().batch(oi, buy, sell);
  st = new wickra.OiToVolumeRatio();
  for (let i = 0; i < n; i++) cmp(b, st.update(oi[i], buy[i], sell[i]), i);

  b = new wickra.PerpetualPremiumIndex().batch(mark, index);
  st = new wickra.PerpetualPremiumIndex();
  for (let i = 0; i < n; i++) cmp(b, st.update(mark[i], index[i]), i);

  b = new wickra.FundingImpliedApr(1095).batch(rate);
  st = new wickra.FundingImpliedApr(1095);
  for (let i = 0; i < n; i++) cmp(b, st.update(rate[i]), i);

  b = new wickra.OpenInterestMomentum(10).batch(oi);
  st = new wickra.OpenInterestMomentum(10);
  for (let i = 0; i < n; i++) cmp(b, st.update(oi[i]), i);
});

test('market breadth: AdvanceDecline reference values', () => {
  // A breadth tick is the universe as parallel arrays; the sign of `change`
  // classifies each symbol as advancing / declining / unchanged.
  const change = [
    [1.0, 0.5, 2.0, -1.0], // 3 up, 1 down -> net +2
    [-1.0, -0.5, -2.0, 1.0], // 1 up, 3 down -> net -2
    [0.0, 0.0, 1.0, -1.0], // 1 up, 1 down -> net 0
  ];
  const volume = change.map((row) => row.map(() => 10.0));
  const flags = change.map((row) => row.map(() => false));

  const ad = new wickra.AdvanceDecline();
  // Cumulative line: +2 -> 0 -> 0.
  assert.equal(ad.update(change[0], volume[0], flags[0], flags[0]), 2.0);
  assert.equal(ad.update(change[1], volume[1], flags[1], flags[1]), 0.0);
  assert.equal(ad.update(change[2], volume[2], flags[2], flags[2]), 0.0);

  // batch matches streaming.
  const batch = new wickra.AdvanceDecline().batch(change, volume, flags, flags);
  assert.deepEqual(Array.from(batch), [2.0, 0.0, 0.0]);
});

test('market breadth: AdvanceDecline rejects ragged universe', () => {
  assert.throws(() =>
    new wickra.AdvanceDecline().update(
      [1.0, -1.0],
      [10.0],
      [false, false],
      [false, false],
    ),
  );
});

test('market breadth: 14 indicators reference values + batch parity', () => {
  const flags4 = [false, false, false, false];

  // Advance/Decline Ratio: 3/1 = 3 ; 0 advancers -> 0.
  const adr = new wickra.AdvanceDeclineRatio();
  assert.equal(adr.update([1, 1, 1, -1], [10, 10, 10, 10], flags4, flags4), 3.0);
  assert.equal(adr.update([-1, -1, -1, -1], [10, 10, 10, 10], flags4, flags4), 0.0);
  assert.deepEqual(
    Array.from(
      new wickra.AdvanceDeclineRatio().batch(
        [[1, 1, 1, -1], [-1, -1, -1, -1]],
        [[10, 10, 10, 10], [10, 10, 10, 10]],
        [flags4, flags4],
        [flags4, flags4],
      ),
    ),
    [3.0, 0.0],
  );

  // AD Volume Line: cumulative net advancing volume.
  const adv = new wickra.AdVolumeLine();
  assert.equal(adv.update([1, -1], [150, 50], [false, false], [false, false]), 100.0);
  assert.equal(adv.update([1, -1], [60, 60], [false, false], [false, false]), 100.0);

  // McClellan Oscillator + Summation: seed 0, then -50.
  const osc = new wickra.McClellanOscillator();
  assert.ok(Math.abs(osc.update([1, 1, 1, -1], [10, 10, 10, 10], flags4, flags4)) < 1e-9);
  assert.ok(Math.abs(osc.update([-1, -1, -1, 1], [10, 10, 10, 10], flags4, flags4) - -50.0) < 1e-9);
  const msi = new wickra.McClellanSummationIndex();
  assert.ok(Math.abs(msi.update([1, 1, 1, -1], [10, 10, 10, 10], flags4, flags4)) < 1e-9);
  assert.ok(Math.abs(msi.update([-1, -1, -1, 1], [10, 10, 10, 10], flags4, flags4) - -50.0) < 1e-9);

  // TRIN: balanced breadth -> 1.
  assert.ok(
    Math.abs(new wickra.Trin().update([1, 1, 1, -1], [50, 50, 50, 50], flags4, flags4) - 1.0) < 1e-9,
  );

  // Breadth Thrust(2): warmup null, then SMA(2) of [0.8, 0.6] = 0.7.
  const bt = new wickra.BreadthThrust(2);
  const up10 = Array(10).fill(false);
  assert.equal(bt.update([...Array(8).fill(1), -1, -1], Array(10).fill(10), up10, up10), null);
  assert.ok(
    Math.abs(bt.update([...Array(6).fill(1), -1, -1, -1, -1], Array(10).fill(10), up10, up10) - 0.7) < 1e-9,
  );

  // New Highs - New Lows: 2 - 1 = 1.
  assert.equal(
    new wickra.NewHighsNewLows().update([1, 1, -1], [10, 10, 10], [true, true, false], [false, false, true]),
    1.0,
  );

  // High-Low Index(2): warmup null, then SMA(2) of [80, 60] = 70.
  const hli = new wickra.HighLowIndex(2);
  assert.equal(
    hli.update(Array(10).fill(1), Array(10).fill(10), [...Array(8).fill(true), false, false], [...Array(8).fill(false), true, true]),
    null,
  );
  assert.ok(
    Math.abs(
      hli.update(Array(10).fill(1), Array(10).fill(10), [...Array(6).fill(true), false, false, false, false], [...Array(6).fill(false), true, true, true, true]) - 70.0,
    ) < 1e-9,
  );

  // Percent Above MA: 3/4 -> 75 (5-array update with aboveMa).
  assert.equal(
    new wickra.PercentAboveMa().update([1, 1, 1, -1], [10, 10, 10, 10], flags4, flags4, [true, true, true, false]),
    75.0,
  );

  // Up/Down Volume Ratio: 150/50 = 3.
  assert.equal(
    new wickra.UpDownVolumeRatio().update([1, -1], [150, 50], [false, false], [false, false]),
    3.0,
  );

  // Bullish Percent Index: 2/4 -> 50 (5-array update with onBuySignal).
  assert.equal(
    new wickra.BullishPercentIndex().update([1, 1, -1, -1], [10, 10, 10, 10], flags4, flags4, [true, true, false, false]),
    50.0,
  );

  // Cumulative Volume Index: 150 - 50 -> 100.
  assert.ok(
    Math.abs(new wickra.CumulativeVolumeIndex().update([1, -1], [150, 50], [false, false], [false, false]) - 100) < 1e-9,
  );

  // Absolute Breadth Index: |2 - 3| = 1.
  assert.equal(
    new wickra.AbsoluteBreadthIndex().update([1, 1, -1, -1, -1], Array(5).fill(10), Array(5).fill(false), Array(5).fill(false)),
    1.0,
  );

  // TICK Index: 2 - 3 = -1.
  assert.equal(
    new wickra.TickIndex().update([1, 1, -1, -1, -1], Array(5).fill(10), Array(5).fill(false), Array(5).fill(false)),
    -1.0,
  );
});

test('market breadth: rejects ragged universe', () => {
  assert.throws(() => new wickra.Trin().update([1, -1], [10], [false, false], [false, false]));
  assert.throws(() =>
    new wickra.PercentAboveMa().update([1, -1], [10, 10], [false, false], [false, false], [true]),
  );
});

test('OI / flow / liquidation indicators reference values', () => {
  // OI +10% while price flat -> divergence +0.1.
  const div = new wickra.OIPriceDivergence(1);
  assert.equal(div.update(1000, 100), null); // warming up
  assert.ok(Math.abs(div.update(1100, 100) - 0.1) < 1e-12);
  // OI-weighted: (100·10 + 110·30) / 40 = 107.5.
  const oiw = new wickra.OIWeighted();
  assert.equal(oiw.update(100, 10), 100);
  assert.ok(Math.abs(oiw.update(110, 30) - 107.5) < 1e-12);
  // Long/short ratio.
  assert.ok(Math.abs(new wickra.LongShortRatio().update(600, 400) - 1.5) < 1e-12);
  assert.equal(new wickra.LongShortRatio().update(600, 0), 0);
  // Taker buy/sell ratio.
  assert.ok(Math.abs(new wickra.TakerBuySellRatio().update(60, 40) - 1.5) < 1e-12);
  assert.equal(new wickra.TakerBuySellRatio().update(60, 0), 0);
  // Liquidation features object.
  const liq = new wickra.LiquidationFeatures().update(30, 10);
  assert.equal(liq.net, 20);
  assert.equal(liq.total, 40);
  assert.equal(liq.imbalance, 0.5);
});

test('liquidation features batch is flat n*5', () => {
  const longLiq = [10, 0, 30];
  const shortLiq = [5, 20, 0];
  const batch = new wickra.LiquidationFeatures().batch(longLiq, shortLiq);
  assert.equal(batch.length, 15);
  // Row 0: long 10, short 5, net 5, total 15.
  assert.equal(batch[0], 10);
  assert.equal(batch[1], 5);
  assert.equal(batch[2], 5);
  assert.equal(batch[3], 15);
});

test('OI flow rejects bad input', () => {
  assert.throws(() => new wickra.OIPriceDivergence(0));
  assert.throws(() => new wickra.OIWeighted().update(0, 100));
});

test('basis & calendar-spread reference values', () => {
  // futures 102 vs index 100 -> 0.02 contango.
  assert.ok(Math.abs(new wickra.TermStructureBasis().update(102, 100) - 0.02) < 1e-12);
  assert.ok(Math.abs(new wickra.TermStructureBasis().update(98, 100) + 0.02) < 1e-12);
  // futures 101 vs perpetual mark 100 -> 0.01.
  assert.ok(Math.abs(new wickra.CalendarSpread().update(101, 100) - 0.01) < 1e-12);
});

test('basis streaming update matches batch', () => {
  const n = 20;
  const index = Array.from({ length: n }, (_, i) => 100 + Math.sin(i * 0.2));
  const futures = Array.from({ length: n }, (_, i) => index[i] + 0.5);
  const batch = new wickra.TermStructureBasis().batch(futures, index);
  const streamer = new wickra.TermStructureBasis();
  assert.equal(batch.length, n);
  for (let i = 0; i < n; i++) {
    assert.ok(Math.abs(streamer.update(futures[i], index[i]) - batch[i]) < 1e-12);
  }
});

test('basis rejects bad input', () => {
  assert.throws(() => new wickra.TermStructureBasis().update(100, 0));
  assert.throws(() => new wickra.CalendarSpread().update(100, 0));
});

test('VolumeProfile exposes the full histogram', () => {
  // bar0 single-print at 10 vol 100; bar1 spans 10..14 vol 80 over 4 bins.
  const vp = new wickra.VolumeProfile(2, 4);
  assert.equal(vp.update(10, 10, 100), null);
  const out = vp.update(14, 10, 80);
  assert.ok(out !== null);
  assert.ok(Math.abs(out.priceLow - 10) < 1e-9);
  assert.ok(Math.abs(out.priceHigh - 14) < 1e-9);
  assert.deepEqual(out.bins.length, 4);
  assert.ok(Math.abs(out.bins[0] - 120) < 1e-9);
  for (let i = 1; i < 4; i++) {
    assert.ok(Math.abs(out.bins[i] - 20) < 1e-9);
  }
});

test('TpoProfile counts time at price, volume-agnostic', () => {
  // bar0 spans 10..14 (+1 each bin); bar1 spans 11..12 (+1 bins 1,2).
  const tpo = new wickra.TpoProfile(2, 4);
  assert.equal(tpo.update(14, 10), null);
  const out = tpo.update(12, 11);
  assert.ok(out !== null);
  assert.ok(Math.abs(out.priceLow - 10) < 1e-9);
  assert.ok(Math.abs(out.priceHigh - 14) < 1e-9);
  assert.deepEqual(out.counts, [1, 2, 2, 1]);
});

test('RenkoBars prints aligned bricks and reverses on two boxes', () => {
  const r = new wickra.RenkoBars(1.0);
  assert.deepEqual(r.update(10), []); // seed
  const up = r.update(13);
  assert.equal(up.length, 3);
  assert.ok(Math.abs(up[0].open - 10) < 1e-9 && Math.abs(up[0].close - 11) < 1e-9);
  assert.ok(up.every((b) => b.direction === 1));
  const down = r.update(10);
  assert.equal(down.length, 2);
  assert.ok(down.every((b) => b.direction === -1));
});

test('KagiBars closes a segment on a reversal', () => {
  const k = new wickra.KagiBars(2.0);
  k.update(10);
  k.update(11);
  k.update(15);
  const seg = k.update(12);
  assert.equal(seg.length, 1);
  assert.equal(seg[0].direction, 1);
  assert.ok(Math.abs(seg[0].start - 10) < 1e-9 && Math.abs(seg[0].end - 15) < 1e-9);
});

test('PointAndFigureBars closes a column on a 3-box reversal', () => {
  const pnf = new wickra.PointAndFigureBars(1.0, 3);
  pnf.update(10);
  pnf.update(13);
  pnf.update(15);
  const col = pnf.update(12);
  assert.equal(col.length, 1);
  assert.equal(col[0].direction, 1);
  assert.ok(Math.abs(col[0].high - 15) < 1e-9 && Math.abs(col[0].low - 10) < 1e-9);
});

test('RangeBars prints aligned bars on an up move', () => {
  const rb = new wickra.RangeBars(1.0);
  assert.deepEqual(rb.update(10), []); // seed
  const up = rb.update(13);
  assert.equal(up.length, 3);
  assert.ok(Math.abs(up[0].open - 10) < 1e-9 && Math.abs(up[2].close - 13) < 1e-9);
  assert.ok(up.every((b) => b.direction === 1));
});

test('TickBars groups a fixed number of candles', () => {
  const tb = new wickra.TickBars(2);
  assert.deepEqual(tb.update(10, 11, 9, 10.5, 100), []);
  const out = tb.update(10.5, 12, 10, 11, 150);
  assert.equal(out.length, 1);
  assert.ok(Math.abs(out[0].open - 10) < 1e-9);
  assert.ok(Math.abs(out[0].high - 12) < 1e-9);
  assert.ok(Math.abs(out[0].low - 9) < 1e-9);
  assert.ok(Math.abs(out[0].close - 11) < 1e-9);
  assert.ok(Math.abs(out[0].volume - 250) < 1e-9);
});

test('VolumeBars closes when accumulated volume crosses the threshold', () => {
  const vb = new wickra.VolumeBars(100);
  assert.deepEqual(vb.update(10, 10, 10, 10, 60), []);
  const out = vb.update(10.5, 10.5, 10.5, 10.5, 60);
  assert.equal(out.length, 1);
  assert.ok(Math.abs(out[0].volume - 120) < 1e-9);
});

test('DollarBars closes when traded value crosses the threshold', () => {
  const db = new wickra.DollarBars(1000);
  assert.deepEqual(db.update(10, 10, 10, 10, 60), []);
  const out = db.update(10, 10, 10, 10, 60);
  assert.equal(out.length, 1);
  assert.ok(Math.abs(out[0].dollar - 1200) < 1e-9);
  assert.ok(Math.abs(out[0].volume - 120) < 1e-9);
});

test('ImbalanceBars closes a buy bar at the threshold', () => {
  const ib = new wickra.ImbalanceBars(3.0);
  ib.update(10, 10, 10, 10);
  ib.update(11, 11, 11, 11);
  ib.update(12, 12, 12, 12);
  const out = ib.update(13, 13, 13, 13);
  assert.equal(out.length, 1);
  assert.equal(out[0].direction, 1);
  assert.ok(Math.abs(out[0].imbalance - 3) < 1e-9);
});

test('RunBars closes a buy run at the run length', () => {
  const rb = new wickra.RunBars(3);
  rb.update(10, 10, 10, 10);
  rb.update(11, 11, 11, 11);
  rb.update(12, 12, 12, 12);
  const out = rb.update(13, 13, 13, 13);
  assert.equal(out.length, 1);
  assert.equal(out[0].direction, 1);
  assert.equal(out[0].length, 3);
});

test('ThreeLineBreakBars draws a rising line', () => {
  const tlb = new wickra.ThreeLineBreakBars(3);
  assert.deepEqual(tlb.update(10), []); // seed
  const out = tlb.update(11);
  assert.equal(out.length, 1);
  assert.equal(out[0].direction, 1);
  assert.ok(Math.abs(out[0].open - 10) < 1e-9 && Math.abs(out[0].close - 11) < 1e-9);
});
