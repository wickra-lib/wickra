//! Node.js bindings for Wickra via napi-rs.
//!
//! Build with:
//! ```text
//! cd bindings/node && npm install && npm run build
//! ```
//!
//! Then `require("wickra")` from Node.

#![allow(clippy::needless_pass_by_value)]
#![allow(missing_debug_implementations)] // napi-derive auto-generates the Node-facing types.
#![allow(clippy::unused_self)]
#![allow(clippy::missing_const_for_fn)]

use napi::bindgen_prelude::{Float64Array, FromNapiValue, TypeName, ValidateNapiValue};
use napi::Error as NapiError;
use napi::Status;
use napi_derive::napi;
use wickra_core as wc;
use wickra_core::{BarBuilder, BatchNanExt, Indicator};

/// An integer argument arriving from JavaScript.
///
/// Every numeric parameter here used to be declared `u32`, and napi converts a
/// JS number to `u32` the way a bitwise operator would: `-1` arrives as
/// 4294967295, `1.5` as `1`, and `1e10` as its low 32 bits. All three reached
/// the Rust constructor as a plausible-looking period, so the validation there
/// had nothing to reject.
///
/// Taking the value as `f64` -- which is what a JS number is -- makes the
/// original visible, so it can be refused with a message naming what was
/// passed. The domain rules (a period of zero, the maximum window length) stay
/// where they were, in the Rust constructor; this only guarantees it sees the
/// number the caller wrote.
#[derive(Debug, Clone, Copy)]
pub struct Count(usize);

impl Count {
    /// The validated value.
    pub const fn get(self) -> usize {
        self.0
    }
}

impl napi::bindgen_prelude::FromNapiValue for Count {
    // The trait method is `unsafe` because it takes raw napi handles. The crate
    // denies rather than forbids `unsafe_code` exactly so a conversion like this
    // can opt in; the body only reads the value through napi's own safe f64
    // conversion and does no pointer work of its own.
    #[allow(unsafe_code)]
    unsafe fn from_napi_value(
        env: napi::sys::napi_env,
        value: napi::sys::napi_value,
    ) -> napi::Result<Self> {
        let raw = f64::from_napi_value(env, value)?;
        if !raw.is_finite() {
            return Err(NapiError::new(
                Status::InvalidArg,
                format!("expected a whole number, got {raw}"),
            ));
        }
        if raw.fract() != 0.0 {
            return Err(NapiError::new(
                Status::InvalidArg,
                format!("expected a whole number, got {raw}"),
            ));
        }
        if raw < 0.0 {
            return Err(NapiError::new(
                Status::InvalidArg,
                format!("expected a non-negative number, got {raw}"),
            ));
        }
        // Beyond 2^53 a JS number can no longer represent consecutive integers,
        // so a value this large is a mistake rather than an intent -- and every
        // parameter here is a window length that the core rejects long before
        // this bound.
        if raw > 9_007_199_254_740_992.0 {
            return Err(NapiError::new(
                Status::InvalidArg,
                format!("{raw} is too large to be an exact integer"),
            ));
        }
        Ok(Self(raw as usize))
    }
}

impl napi::bindgen_prelude::ValidateNapiValue for Count {}

/// Narrow a validated count to the `u32` a few core constructors take.
///
/// `Count` already refused anything that is not a whole non-negative number, so
/// the only way to arrive here out of range is a genuinely huge value -- which
/// gets a message naming the parameter rather than a silent wrap.
fn narrow(value: Count, what: &str) -> napi::Result<u32> {
    u32::try_from(value.get()).map_err(|_| {
        NapiError::new(
            Status::InvalidArg,
            format!("{what} is too large: {}", value.get()),
        )
    })
}

impl napi::bindgen_prelude::TypeName for Count {
    fn type_name() -> &'static str {
        "number"
    }

    fn value_type() -> napi::ValueType {
        napi::ValueType::Number
    }
}

/// A numeric input series from JavaScript.
///
/// A `Float64Array` is read in place, without a copy; any other array of
/// numbers is converted element by element, as every batch took it before. A
/// typed array of another element type is refused rather than reinterpreted.
pub struct Series(SeriesData);

enum SeriesData {
    Typed(Float64Array),
    Plain(Vec<f64>),
}

impl std::ops::Deref for Series {
    type Target = [f64];

    fn deref(&self) -> &[f64] {
        match &self.0 {
            SeriesData::Typed(values) => values,
            SeriesData::Plain(values) => values,
        }
    }
}

impl<'a> IntoIterator for &'a Series {
    type Item = &'a f64;
    type IntoIter = std::slice::Iter<'a, f64>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl FromNapiValue for Series {
    // The trait method is `unsafe` because it takes raw napi handles; the body
    // only chooses between napi's own typed-array and array conversions.
    #[allow(unsafe_code)]
    unsafe fn from_napi_value(
        env: napi::sys::napi_env,
        value: napi::sys::napi_value,
    ) -> napi::Result<Self> {
        let mut typed = false;
        napi::check_status!(napi::sys::napi_is_typedarray(env, value, &raw mut typed))?;
        let data = if typed {
            SeriesData::Typed(Float64Array::from_napi_value(env, value)?)
        } else {
            SeriesData::Plain(Vec::<f64>::from_napi_value(env, value)?)
        };
        Ok(Self(data))
    }
}

impl ValidateNapiValue for Series {}

impl TypeName for Series {
    fn type_name() -> &'static str {
        "Series"
    }

    fn value_type() -> napi::ValueType {
        napi::ValueType::Object
    }
}

/// A caller-owned `Float64Array` a batch writes its result into.
///
/// Writing into JavaScript memory is sound here because JavaScript runs on
/// this thread only and none runs during the synchronous call, and because two
/// things are refused up front: a buffer another thread could write at the same
/// time (a `SharedArrayBuffer`), and an output that overlaps one of the inputs.
pub struct OutSeries(Float64Array);

impl FromNapiValue for OutSeries {
    // The trait method is `unsafe` because it takes raw napi handles; the body
    // only queries the value through napi and then uses napi's own conversion.
    #[allow(unsafe_code)]
    unsafe fn from_napi_value(
        env: napi::sys::napi_env,
        value: napi::sys::napi_value,
    ) -> napi::Result<Self> {
        let mut typed = false;
        napi::check_status!(napi::sys::napi_is_typedarray(env, value, &raw mut typed))?;
        if !typed {
            return Err(NapiError::new(
                Status::InvalidArg,
                "the output must be a Float64Array".to_string(),
            ));
        }
        let mut buffer = std::ptr::null_mut();
        napi::check_status!(napi::sys::napi_get_typedarray_info(
            env,
            value,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut buffer,
            std::ptr::null_mut(),
        ))?;
        let mut plain = false;
        napi::check_status!(napi::sys::napi_is_arraybuffer(env, buffer, &raw mut plain))?;
        if !plain {
            return Err(NapiError::new(
                Status::InvalidArg,
                "the output Float64Array must not be backed by a SharedArrayBuffer".to_string(),
            ));
        }
        Ok(Self(Float64Array::from_napi_value(env, value)?))
    }
}

impl ValidateNapiValue for OutSeries {}

impl TypeName for OutSeries {
    fn type_name() -> &'static str {
        "Float64Array"
    }

    fn value_type() -> napi::ValueType {
        napi::ValueType::Object
    }
}

impl OutSeries {
    /// The output as a slice of `rows` values, after checking its length and
    /// that it shares no memory with any input.
    fn rows(&mut self, rows: usize, inputs: &[&[f64]]) -> napi::Result<&mut [f64]> {
        let out: &[f64] = &self.0;
        if out.len() != rows {
            return Err(NapiError::new(
                Status::InvalidArg,
                format!("the output must hold {rows} values, got {}", out.len()),
            ));
        }
        let span = out.as_ptr_range();
        let shares = inputs.iter().any(|input| {
            let other = input.as_ptr_range();
            !input.is_empty() && !out.is_empty() && other.start < span.end && span.start < other.end
        });
        if shares {
            return Err(NapiError::new(
                Status::InvalidArg,
                "the output must not share memory with an input".to_string(),
            ));
        }
        // SAFETY: JavaScript runs on this thread only and none runs until this
        // call returns; the buffer is not a SharedArrayBuffer (refused when the
        // value arrived) and overlaps no input (checked above), so this is the
        // only reference to the memory for as long as it lives.
        #[allow(unsafe_code)]
        let rows_out = unsafe { self.0.as_mut() };
        Ok(rows_out)
    }
}

/// Every input column of a batch must be as long as the first.
fn same_length(columns: &[&[f64]], what: &str) -> napi::Result<usize> {
    let n = columns.first().map_or(0, |c| c.len());
    if columns.iter().any(|c| c.len() != n) {
        return Err(NapiError::from_reason(format!(
            "{what} must be equal length"
        )));
    }
    Ok(n)
}

fn map_err(e: wc::Error) -> NapiError {
    NapiError::new(Status::InvalidArg, e.to_string())
}

/// Library version (matches the Rust crate version).
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ============================== Scalar indicators ==============================

macro_rules! node_scalar_indicator {
    ($wrapper:ident, $node_name:literal, $rust_ty:ty) => {
        #[napi(js_name = $node_name)]
        pub struct $wrapper {
            inner: $rust_ty,
        }

        #[napi]
        impl $wrapper {
            #[napi(constructor)]
            pub fn new(period: Count) -> napi::Result<Self> {
                Ok(Self {
                    inner: <$rust_ty>::new(period.get()).map_err(map_err)?,
                })
            }
            #[napi]
            pub fn update(&mut self, value: f64) -> Option<f64> {
                self.inner.update(value)
            }
            #[napi]
            pub fn batch(&mut self, prices: Series) -> Vec<f64> {
                self.inner.batch_nan(&prices)
            }
            #[napi(js_name = "batchFast")]
            pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
                Float64Array::new(self.inner.batch_fast(&prices))
            }
            #[napi(js_name = "batchInto")]
            pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
                let rows = out.rows(prices.len(), &[&prices])?;
                self.inner.batch_nan_into(&prices, rows);
                Ok(())
            }
            #[napi(js_name = "batchFastInto")]
            pub fn batch_fast_into(
                &mut self,
                prices: Series,
                mut out: OutSeries,
            ) -> napi::Result<()> {
                let rows = out.rows(prices.len(), &[&prices])?;
                self.inner.batch_fast_into(&prices, rows);
                Ok(())
            }
            #[napi]
            pub fn reset(&mut self) {
                self.inner.reset();
            }

            #[napi]
            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[napi(js_name = "isReady")]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[napi(js_name = "warmupPeriod")]
            pub fn warmup_period(&self) -> u32 {
                self.inner.warmup_period() as u32
            }
        }
    };
}

node_scalar_indicator!(SmaNode, "SMA", wc::Sma);
node_scalar_indicator!(EmaNode, "EMA", wc::Ema);
node_scalar_indicator!(WmaNode, "WMA", wc::Wma);
node_scalar_indicator!(RsiNode, "RSI", wc::Rsi);
node_scalar_indicator!(DemaNode, "DEMA", wc::Dema);
node_scalar_indicator!(TemaNode, "TEMA", wc::Tema);
node_scalar_indicator!(HmaNode, "HMA", wc::Hma);
node_scalar_indicator!(RocNode, "ROC", wc::Roc);
node_scalar_indicator!(TrixNode, "TRIX", wc::Trix);
node_scalar_indicator!(SmmaNode, "SMMA", wc::Smma);
node_scalar_indicator!(TrimaNode, "TRIMA", wc::Trima);
node_scalar_indicator!(ZlemaNode, "ZLEMA", wc::Zlema);
node_scalar_indicator!(MomNode, "MOM", wc::Mom);
node_scalar_indicator!(CmoNode, "CMO", wc::Cmo);
node_scalar_indicator!(DpoNode, "DPO", wc::Dpo);
node_scalar_indicator!(StdDevNode, "StdDev", wc::StdDev);
node_scalar_indicator!(UlcerIndexNode, "UlcerIndex", wc::UlcerIndex);
node_scalar_indicator!(
    VerticalHorizontalFilterNode,
    "VerticalHorizontalFilter",
    wc::VerticalHorizontalFilter
);
node_scalar_indicator!(ZScoreNode, "ZScore", wc::ZScore);
node_scalar_indicator!(McGinleyDynamicNode, "McGinleyDynamic", wc::McGinleyDynamic);
node_scalar_indicator!(FramaNode, "FRAMA", wc::Frama);

// Family 10 — Ehlers / Cycle: single-period scalars.
node_scalar_indicator!(SuperSmootherNode, "SuperSmoother", wc::SuperSmoother);
node_scalar_indicator!(FisherTransformNode, "FisherTransform", wc::FisherTransform);
node_scalar_indicator!(DecyclerNode, "Decycler", wc::Decycler);
node_scalar_indicator!(CenterOfGravityNode, "CenterOfGravity", wc::CenterOfGravity);
node_scalar_indicator!(CyberneticCycleNode, "CyberneticCycle", wc::CyberneticCycle);
node_scalar_indicator!(
    InstantaneousTrendlineNode,
    "InstantaneousTrendline",
    wc::InstantaneousTrendline
);
node_scalar_indicator!(
    EhlersStochasticNode,
    "EhlersStochastic",
    wc::EhlersStochastic
);

// RviVolatility (Relative Volatility Index, Donald Dorsey). Disambiguated
// from `RVI` = Relative Vigor Index in Family 02. Takes a single `period` and
// rejects both `period == 0` and `period == 1` (a 1-bar standard deviation is
// always zero). Hand-rolled, but behaves like `node_scalar_indicator!`: the
// fallible `new` propagates the core error and throws a JS error on bad period.
#[napi(js_name = "RVIVolatility")]
pub struct RviVolatilityNode {
    inner: wc::RviVolatility,
}

#[napi]
impl RviVolatilityNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RviVolatility::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

node_scalar_indicator!(VarianceNode, "Variance", wc::Variance);
node_scalar_indicator!(
    CoefficientOfVariationNode,
    "CoefficientOfVariation",
    wc::CoefficientOfVariation
);
node_scalar_indicator!(SkewnessNode, "Skewness", wc::Skewness);
node_scalar_indicator!(KurtosisNode, "Kurtosis", wc::Kurtosis);
node_scalar_indicator!(StandardErrorNode, "StandardError", wc::StandardError);
node_scalar_indicator!(DetrendedStdDevNode, "DetrendedStdDev", wc::DetrendedStdDev);
node_scalar_indicator!(RSquaredNode, "RSquared", wc::RSquared);
node_scalar_indicator!(
    MedianAbsoluteDeviationNode,
    "MedianAbsoluteDeviation",
    wc::MedianAbsoluteDeviation
);
node_scalar_indicator!(MidPointNode, "MIDPOINT", wc::MidPoint);
node_scalar_indicator!(RocpNode, "ROCP", wc::Rocp);
node_scalar_indicator!(RocrNode, "ROCR", wc::Rocr);
node_scalar_indicator!(Rocr100Node, "ROCR100", wc::Rocr100);
node_scalar_indicator!(
    LinRegInterceptNode,
    "LINEARREG_INTERCEPT",
    wc::LinRegIntercept
);
node_scalar_indicator!(TsfNode, "TSF", wc::Tsf);
node_scalar_indicator!(LogReturnNode, "LogReturn", wc::LogReturn);
node_scalar_indicator!(
    RealizedVolatilityNode,
    "RealizedVolatility",
    wc::RealizedVolatility
);
node_scalar_indicator!(RollingIqrNode, "RollingIqr", wc::RollingIqr);
node_scalar_indicator!(
    RollingPercentileRankNode,
    "RollingPercentileRank",
    wc::RollingPercentileRank
);
node_scalar_indicator!(TrendLabelNode, "TrendLabel", wc::TrendLabel);
node_scalar_indicator!(WinRateNode, "WinRate", wc::WinRate);
node_scalar_indicator!(ExpectancyNode, "Expectancy", wc::Expectancy);
node_scalar_indicator!(SineWeightedMaNode, "SWMA", wc::SineWeightedMa);
node_scalar_indicator!(GeometricMaNode, "GMA", wc::GeometricMa);
node_scalar_indicator!(EhmaNode, "EHMA", wc::Ehma);
node_scalar_indicator!(MedianMaNode, "MedianMA", wc::MedianMa);
node_scalar_indicator!(
    AdaptiveLaguerreFilterNode,
    "AdaptiveLaguerre",
    wc::AdaptiveLaguerreFilter
);
node_scalar_indicator!(DisparityIndexNode, "DisparityIndex", wc::DisparityIndex);
node_scalar_indicator!(FisherRsiNode, "FisherRSI", wc::FisherRsi);
node_scalar_indicator!(RsxNode, "RSX", wc::Rsx);
node_scalar_indicator!(
    DynamicMomentumIndexNode,
    "DynamicMomentumIndex",
    wc::DynamicMomentumIndex
);
node_scalar_indicator!(
    TrendStrengthIndexNode,
    "TREND_STRENGTH_INDEX",
    wc::TrendStrengthIndex
);
node_scalar_indicator!(TsfOscillatorNode, "TsfOscillator", wc::TsfOscillator);
node_scalar_indicator!(
    BipowerVariationNode,
    "BipowerVariation",
    wc::BipowerVariation
);
node_scalar_indicator!(JarqueBeraNode, "JARQUEBERA", wc::JarqueBera);
node_scalar_indicator!(
    RollingMinMaxScalerNode,
    "ROLLINGMINMAX",
    wc::RollingMinMaxScaler
);
node_scalar_indicator!(HighpassFilterNode, "HIGHPASS", wc::HighpassFilter);
node_scalar_indicator!(ReflexNode, "REFLEX", wc::Reflex);
node_scalar_indicator!(TrendflexNode, "TRENDFLEX", wc::Trendflex);
node_scalar_indicator!(
    CorrelationTrendIndicatorNode,
    "CTI",
    wc::CorrelationTrendIndicator
);
node_scalar_indicator!(AdaptiveRsiNode, "ADAPTIVERSI", wc::AdaptiveRsi);
node_scalar_indicator!(
    UniversalOscillatorNode,
    "UNIVERSALOSC",
    wc::UniversalOscillator
);
node_scalar_indicator!(SterlingRatioNode, "SterlingRatio", wc::SterlingRatio);
node_scalar_indicator!(BurkeRatioNode, "BurkeRatio", wc::BurkeRatio);
node_scalar_indicator!(MartinRatioNode, "MartinRatio", wc::MartinRatio);
node_scalar_indicator!(TailRatioNode, "TailRatio", wc::TailRatio);
node_scalar_indicator!(KRatioNode, "KRatio", wc::KRatio);
node_scalar_indicator!(
    CommonSenseRatioNode,
    "CommonSenseRatio",
    wc::CommonSenseRatio
);
node_scalar_indicator!(GainToPainRatioNode, "GainToPainRatio", wc::GainToPainRatio);

// Multi-arg Ehlers scalars: hand-written (node_scalar_indicator! is single-period).

#[napi(js_name = "UpsidePotentialRatio")]
pub struct UpsidePotentialRatioNode {
    inner: wc::UpsidePotentialRatio,
}

#[napi]
impl UpsidePotentialRatioNode {
    #[napi(constructor)]
    pub fn new(period: Count, mar: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::UpsidePotentialRatio::new(period.get(), mar).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "M2Measure")]
pub struct M2MeasureNode {
    inner: wc::M2Measure,
}

#[napi]
impl M2MeasureNode {
    #[napi(constructor)]
    pub fn new(period: Count, risk_free: f64, benchmark_stddev: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::M2Measure::new(period.get(), risk_free, benchmark_stddev)
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "BANDPASS")]
pub struct BandpassFilterNode {
    inner: wc::BandpassFilter,
}

#[napi]
impl BandpassFilterNode {
    #[napi(constructor)]
    pub fn new(period: Count, bandwidth: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::BandpassFilter::new(period.get(), bandwidth).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "EVENBETTERSINE")]
pub struct EvenBetterSinewaveNode {
    inner: wc::EvenBetterSinewave,
}

#[napi]
impl EvenBetterSinewaveNode {
    #[napi(constructor)]
    pub fn new(hp_period: Count, ssf_length: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::EvenBetterSinewave::new(hp_period.get(), ssf_length.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "AUTOCORRPGRAM")]
pub struct AutocorrelationPeriodogramNode {
    inner: wc::AutocorrelationPeriodogram,
}

#[napi]
impl AutocorrelationPeriodogramNode {
    #[napi(constructor)]
    pub fn new(min_period: Count, max_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AutocorrelationPeriodogram::new(min_period.get(), max_period.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// Shannon Entropy / Sample Entropy: multi-arg scalar ctors, hand-written
// (node_scalar_indicator! only generates a single-period constructor).

#[napi(js_name = "SHANNONENT")]
pub struct ShannonEntropyNode {
    inner: wc::ShannonEntropy,
}

#[napi]
impl ShannonEntropyNode {
    #[napi(constructor)]
    pub fn new(period: Count, bins: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ShannonEntropy::new(period.get(), bins.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "SAMPLEENT")]
pub struct SampleEntropyNode {
    inner: wc::SampleEntropy,
}

#[napi]
impl SampleEntropyNode {
    #[napi(constructor)]
    pub fn new(period: Count, m: Count, r_factor: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::SampleEntropy::new(period.get(), m.get(), r_factor).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "EwmaVolatility")]
pub struct EwmaVolatilityNode {
    inner: wc::EwmaVolatility,
}

#[napi]
impl EwmaVolatilityNode {
    #[napi(constructor)]
    pub fn new(lambda: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::EwmaVolatility::new(lambda).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "Garch11")]
pub struct Garch11Node {
    inner: wc::Garch11,
}

#[napi]
impl Garch11Node {
    #[napi(constructor)]
    pub fn new(omega: f64, alpha: f64, beta: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Garch11::new(omega, alpha, beta).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "VolatilityOfVolatility")]
pub struct VolatilityOfVolatilityNode {
    inner: wc::VolatilityOfVolatility,
}

#[napi]
impl VolatilityOfVolatilityNode {
    #[napi(constructor)]
    pub fn new(vol_window: Count, vov_window: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::VolatilityOfVolatility::new(vol_window.get(), vov_window.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

/// Volatility-cone result: current realized volatility and its lookback
/// envelope (min / median / max) plus the percentile rank of `current`.
#[napi(object)]
pub struct VolatilityConeValue {
    pub current: f64,
    pub min: f64,
    pub median: f64,
    pub max: f64,
    pub percentile: f64,
}

#[napi(js_name = "VolatilityCone")]
pub struct VolatilityConeNode {
    inner: wc::VolatilityCone,
}

#[napi]
impl VolatilityConeNode {
    #[napi(constructor)]
    pub fn new(window: Count, lookback: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::VolatilityCone::new(window.get(), lookback.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<VolatilityConeValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| VolatilityConeValue {
                current: o.current,
                min: o.min,
                median: o.median,
                max: o.max,
                percentile: o.percentile,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 5];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 5] = o.current;
                out[i * 5 + 1] = o.min;
                out[i * 5 + 2] = o.median;
                out[i * 5 + 3] = o.max;
                out[i * 5 + 4] = o.percentile;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "JumpIndicator")]
pub struct JumpIndicatorNode {
    inner: wc::JumpIndicator,
}

#[napi]
impl JumpIndicatorNode {
    #[napi(constructor)]
    pub fn new(period: Count, threshold: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::JumpIndicator::new(period.get(), threshold).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "RegimeLabel")]
pub struct RegimeLabelNode {
    inner: wc::RegimeLabel,
}

#[napi]
impl RegimeLabelNode {
    #[napi(constructor)]
    pub fn new(vol_period: Count, lookback: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RegimeLabel::new(vol_period.get(), lookback.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "RollingQuantile")]
pub struct RollingQuantileNode {
    inner: wc::RollingQuantile,
}

#[napi]
impl RollingQuantileNode {
    #[napi(constructor)]
    pub fn new(period: Count, quantile: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RollingQuantile::new(period.get(), quantile).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Autocorrelation (period + lag) ==============================

#[napi(js_name = "Autocorrelation")]
pub struct AutocorrelationNode {
    inner: wc::Autocorrelation,
}

#[napi]
impl AutocorrelationNode {
    #[napi(constructor)]
    pub fn new(period: Count, lag: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Autocorrelation::new(period.get(), lag.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== HurstExponent (period + chunks) ==============================

#[napi(js_name = "HurstExponent")]
pub struct HurstExponentNode {
    inner: wc::HurstExponent,
}

#[napi]
impl HurstExponentNode {
    #[napi(constructor)]
    pub fn new(period: Count, chunks: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::HurstExponent::new(period.get(), chunks.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Two-series indicators (Pearson / Beta / Spearman) ==============================

macro_rules! node_pair_indicator {
    ($wrapper:ident, $node_name:literal, $rust_ty:ty) => {
        #[napi(js_name = $node_name)]
        pub struct $wrapper {
            inner: $rust_ty,
        }

        #[napi]
        impl $wrapper {
            #[napi(constructor)]
            pub fn new(period: Count) -> napi::Result<Self> {
                Ok(Self {
                    inner: <$rust_ty>::new(period.get()).map_err(map_err)?,
                })
            }
            #[napi]
            pub fn update(&mut self, x: f64, y: f64) -> Option<f64> {
                self.inner.update((x, y))
            }
            /// Batch over two equally-sized arrays. Returns a length-`n` array
            /// with `NaN` for warmup positions.
            #[napi]
            pub fn batch(&mut self, x: Series, y: Series) -> napi::Result<Vec<f64>> {
                if x.len() != y.len() {
                    return Err(NapiError::new(
                        Status::InvalidArg,
                        "x and y must be equal length".to_string(),
                    ));
                }
                let mut out = Vec::with_capacity(x.len());
                for i in 0..x.len() {
                    out.push(self.inner.update((x[i], y[i])).unwrap_or(f64::NAN));
                }
                Ok(out)
            }
            #[napi]
            pub fn reset(&mut self) {
                self.inner.reset();
            }

            #[napi]
            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[napi(js_name = "isReady")]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[napi(js_name = "warmupPeriod")]
            pub fn warmup_period(&self) -> u32 {
                self.inner.warmup_period() as u32
            }
        }
    };
}

node_pair_indicator!(
    PearsonCorrelationNode,
    "PearsonCorrelation",
    wc::PearsonCorrelation
);

#[napi]
impl PearsonCorrelationNode {
    /// Opt-in fast batch over the pairs `(x[i], y[i])`.
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, x: Series, y: Series) -> napi::Result<Float64Array> {
        let n = same_length(&[&x, &y], "x and y")?;
        let mut out = vec![0.0; n];
        self.inner.batch_pairs_fast_into(&x, &y, &mut out);
        Ok(Float64Array::new(out))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, x: Series, y: Series, mut out: OutSeries) -> napi::Result<()> {
        let n = same_length(&[&x, &y], "x and y")?;
        let rows = out.rows(n, &[&x, &y])?;
        self.inner.batch_pairs_into(&x, &y, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(
        &mut self,
        x: Series,
        y: Series,
        mut out: OutSeries,
    ) -> napi::Result<()> {
        let n = same_length(&[&x, &y], "x and y")?;
        let rows = out.rows(n, &[&x, &y])?;
        self.inner.batch_pairs_fast_into(&x, &y, rows);
        Ok(())
    }
}
node_pair_indicator!(BetaNode, "Beta", wc::Beta);
node_pair_indicator!(PairwiseBetaNode, "PairwiseBeta", wc::PairwiseBeta);
node_pair_indicator!(
    SpreadAr1CoefficientNode,
    "SpreadAr1Coefficient",
    wc::SpreadAr1Coefficient
);
node_pair_indicator!(
    SpearmanCorrelationNode,
    "SpearmanCorrelation",
    wc::SpearmanCorrelation
);
node_pair_indicator!(
    RollingCorrelationNode,
    "RollingCorrelation",
    wc::RollingCorrelation
);
node_pair_indicator!(
    RollingCovarianceNode,
    "RollingCovariance",
    wc::RollingCovariance
);
node_pair_indicator!(OuHalfLifeNode, "OuHalfLife", wc::OuHalfLife);
node_pair_indicator!(SpreadHurstNode, "SpreadHurst", wc::SpreadHurst);
node_pair_indicator!(DistanceSsdNode, "DistanceSsd", wc::DistanceSsd);
node_pair_indicator!(KendallTauNode, "KendallTau", wc::KendallTau);
node_pair_indicator!(
    BetaNeutralSpreadNode,
    "BetaNeutralSpread",
    wc::BetaNeutralSpread
);
node_pair_indicator!(
    HasbrouckInformationShareNode,
    "HasbrouckInformationShare",
    wc::HasbrouckInformationShare
);

// ============================== PairSpreadZScore ==============================

/// Pair spread z-score: two ctor params (`betaPeriod`, `zPeriod`), one `(a, b)`
/// price pair per update, a single z-score out.
#[napi(js_name = "PairSpreadZScore")]
pub struct PairSpreadZScoreNode {
    inner: wc::PairSpreadZScore,
}

#[napi]
impl PairSpreadZScoreNode {
    #[napi(constructor)]
    pub fn new(beta_period: Count, z_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::PairSpreadZScore::new(beta_period.get(), z_period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized arrays of prices. Returns a length-`n`
    /// array with `NaN` for warmup positions.
    #[napi]
    pub fn batch(&mut self, a: Series, b: Series) -> napi::Result<Vec<f64>> {
        if a.len() != b.len() {
            return Err(NapiError::new(
                Status::InvalidArg,
                "a and b must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            out.push(self.inner.update((a[i], b[i])).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== LeadLagCrossCorrelation ==============================

/// Lead/lag result: the offset that maximises correlation, and that correlation.
#[napi(object)]
pub struct LeadLagValue {
    /// Offset that maximises `|corr(a, b shifted)|`. Positive ⇒ `a` leads `b`.
    pub lag: i32,
    /// Signed correlation at that lag, in `[-1, 1]`.
    pub correlation: f64,
}

#[napi(js_name = "LeadLagCrossCorrelation")]
pub struct LeadLagCrossCorrelationNode {
    inner: wc::LeadLagCrossCorrelation,
}

#[napi]
impl LeadLagCrossCorrelationNode {
    #[napi(constructor)]
    pub fn new(window: Count, max_lag: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::LeadLagCrossCorrelation::new(window.get(), max_lag.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, a: f64, b: f64) -> Option<LeadLagValue> {
        self.inner.update((a, b)).map(|o| LeadLagValue {
            lag: o.lag as i32,
            correlation: o.correlation,
        })
    }
    /// Batch over two equally-sized arrays. Returns a flat array of length
    /// `2 * n`, interleaved per row as `[lag0, corr0, lag1, corr1, ...]`. Read
    /// column `j` of row `i` as `result[i * 2 + j]`. Warmup rows are `NaN`.
    #[napi]
    pub fn batch(&mut self, a: Series, b: Series) -> napi::Result<Vec<f64>> {
        if a.len() != b.len() {
            return Err(NapiError::new(
                Status::InvalidArg,
                "a and b must be equal length".to_string(),
            ));
        }
        let mut out = vec![f64::NAN; a.len() * 2];
        for i in 0..a.len() {
            if let Some(o) = self.inner.update((a[i], b[i])) {
                out[i * 2] = o.lag as f64;
                out[i * 2 + 1] = o.correlation;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Cointegration ==============================

/// Cointegration result: hedge ratio, current spread, and the ADF statistic.
#[napi(object)]
pub struct CointegrationValue {
    /// Engle–Granger hedge ratio (OLS slope of `a` on `b`).
    pub hedge_ratio: f64,
    /// Current spread (regression residual) `a - (alpha + beta*b)`.
    pub spread: f64,
    /// Augmented Dickey–Fuller statistic on the spread; more negative ⇒ more
    /// strongly mean-reverting.
    pub adf_stat: f64,
}

#[napi(js_name = "Cointegration")]
pub struct CointegrationNode {
    inner: wc::Cointegration,
}

#[napi]
impl CointegrationNode {
    #[napi(constructor)]
    pub fn new(period: Count, adf_lags: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Cointegration::new(period.get(), adf_lags.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, a: f64, b: f64) -> Option<CointegrationValue> {
        self.inner.update((a, b)).map(|o| CointegrationValue {
            hedge_ratio: o.hedge_ratio,
            spread: o.spread,
            adf_stat: o.adf_stat,
        })
    }
    /// Batch over two equally-sized arrays. Returns a flat array of length
    /// `3 * n`, interleaved per row as `[hedgeRatio0, spread0, adfStat0, ...]`.
    /// Read column `j` of row `i` as `result[i * 3 + j]`. Warmup rows are `NaN`.
    #[napi]
    pub fn batch(&mut self, a: Series, b: Series) -> napi::Result<Vec<f64>> {
        if a.len() != b.len() {
            return Err(NapiError::new(
                Status::InvalidArg,
                "a and b must be equal length".to_string(),
            ));
        }
        let mut out = vec![f64::NAN; a.len() * 3];
        for i in 0..a.len() {
            if let Some(o) = self.inner.update((a[i], b[i])) {
                out[i * 3] = o.hedge_ratio;
                out[i * 3 + 1] = o.spread;
                out[i * 3 + 2] = o.adf_stat;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== RelativeStrengthAB ==============================

/// Relative-strength triple: the a/b ratio, its moving average, and its RSI.
#[napi(object)]
pub struct RelativeStrengthValue {
    /// Raw ratio `a / b`.
    pub ratio: f64,
    /// Moving average of the ratio.
    pub ratio_ma: f64,
    /// RSI of the ratio.
    pub ratio_rsi: f64,
}

#[napi(js_name = "RelativeStrengthAB")]
pub struct RelativeStrengthABNode {
    inner: wc::RelativeStrengthAB,
}

#[napi]
impl RelativeStrengthABNode {
    #[napi(constructor)]
    pub fn new(ma_period: Count, rsi_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RelativeStrengthAB::new(ma_period.get(), rsi_period.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, a: f64, b: f64) -> Option<RelativeStrengthValue> {
        self.inner.update((a, b)).map(|o| RelativeStrengthValue {
            ratio: o.ratio,
            ratio_ma: o.ratio_ma,
            ratio_rsi: o.ratio_rsi,
        })
    }
    /// Batch over two equally-sized arrays. Returns a flat array of length
    /// `3 * n`, interleaved per row as `[ratio0, ratioMa0, ratioRsi0, ...]`.
    /// Read column `j` of row `i` as `result[i * 3 + j]`. Warmup rows are `NaN`.
    #[napi]
    pub fn batch(&mut self, a: Series, b: Series) -> napi::Result<Vec<f64>> {
        if a.len() != b.len() {
            return Err(NapiError::new(
                Status::InvalidArg,
                "a and b must be equal length".to_string(),
            ));
        }
        let mut out = vec![f64::NAN; a.len() * 3];
        for i in 0..a.len() {
            if let Some(o) = self.inner.update((a[i], b[i])) {
                out[i * 3] = o.ratio;
                out[i * 3 + 1] = o.ratio_ma;
                out[i * 3 + 2] = o.ratio_rsi;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== VarianceRatio ==============================

/// Lo–MacKinlay variance ratio: two ctor params (`period`, `q`), one `(a, b)`
/// pair per update, a single ratio out.
#[napi(js_name = "VarianceRatio")]
pub struct VarianceRatioNode {
    inner: wc::VarianceRatio,
}

#[napi]
impl VarianceRatioNode {
    #[napi(constructor)]
    pub fn new(period: Count, q: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::VarianceRatio::new(period.get(), q.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized arrays. Returns a length-`n` array with
    /// `NaN` for warmup positions.
    #[napi]
    pub fn batch(&mut self, a: Series, b: Series) -> napi::Result<Vec<f64>> {
        if a.len() != b.len() {
            return Err(NapiError::new(
                Status::InvalidArg,
                "a and b must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            out.push(self.inner.update((a[i], b[i])).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== GrangerCausality ==============================

/// Granger causality F-statistic: two ctor params (`period`, `lag`), one
/// `(a, b)` pair per update, a single F-statistic out.
#[napi(js_name = "GrangerCausality")]
pub struct GrangerCausalityNode {
    inner: wc::GrangerCausality,
}

#[napi]
impl GrangerCausalityNode {
    #[napi(constructor)]
    pub fn new(period: Count, lag: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::GrangerCausality::new(period.get(), lag.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized arrays. Returns a length-`n` array with
    /// `NaN` for warmup positions.
    #[napi]
    pub fn batch(&mut self, a: Series, b: Series) -> napi::Result<Vec<f64>> {
        if a.len() != b.len() {
            return Err(NapiError::new(
                Status::InvalidArg,
                "a and b must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            out.push(self.inner.update((a[i], b[i])).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== KalmanHedgeRatio ==============================

/// Kalman hedge-ratio result: dynamic hedge ratio, intercept, and spread.
#[napi(object)]
pub struct KalmanHedgeRatioValue {
    /// Current hedge ratio (filtered slope of `a` on `b`).
    pub hedge_ratio: f64,
    /// Current intercept (filtered level offset).
    pub intercept: f64,
    /// Forecast error `a - (intercept + hedgeRatio*b)` — the spread signal.
    pub spread: f64,
}

#[napi(js_name = "KalmanHedgeRatio")]
pub struct KalmanHedgeRatioNode {
    inner: wc::KalmanHedgeRatio,
}

#[napi]
impl KalmanHedgeRatioNode {
    #[napi(constructor)]
    pub fn new(delta: f64, observation_var: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::KalmanHedgeRatio::new(delta, observation_var).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, a: f64, b: f64) -> Option<KalmanHedgeRatioValue> {
        self.inner.update((a, b)).map(|o| KalmanHedgeRatioValue {
            hedge_ratio: o.hedge_ratio,
            intercept: o.intercept,
            spread: o.spread,
        })
    }
    /// Batch over two equally-sized arrays. Returns a flat array of length
    /// `3 * n`, interleaved per row as `[hedgeRatio0, intercept0, spread0, ...]`.
    /// Read column `j` of row `i` as `result[i * 3 + j]`. Warmup rows are `NaN`.
    #[napi]
    pub fn batch(&mut self, a: Series, b: Series) -> napi::Result<Vec<f64>> {
        if a.len() != b.len() {
            return Err(NapiError::new(
                Status::InvalidArg,
                "a and b must be equal length".to_string(),
            ));
        }
        let mut out = vec![f64::NAN; a.len() * 3];
        for i in 0..a.len() {
            if let Some(o) = self.inner.update((a[i], b[i])) {
                out[i * 3] = o.hedge_ratio;
                out[i * 3 + 1] = o.intercept;
                out[i * 3 + 2] = o.spread;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== SpreadBollingerBands ==============================

/// Spread Bollinger-bands result: middle, upper and lower bands plus `%b`.
#[napi(object)]
pub struct SpreadBollingerBandsValue {
    /// Middle band: the rolling mean of the spread.
    pub middle: f64,
    /// Upper band.
    pub upper: f64,
    /// Lower band.
    pub lower: f64,
    /// `%b`: where the spread sits across the band (`0` lower, `1` upper).
    pub percent_b: f64,
}

#[napi(js_name = "SpreadBollingerBands")]
pub struct SpreadBollingerBandsNode {
    inner: wc::SpreadBollingerBands,
}

#[napi]
impl SpreadBollingerBandsNode {
    #[napi(constructor)]
    pub fn new(period: Count, num_std: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::SpreadBollingerBands::new(period.get(), num_std).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, a: f64, b: f64) -> Option<SpreadBollingerBandsValue> {
        self.inner
            .update((a, b))
            .map(|o| SpreadBollingerBandsValue {
                middle: o.middle,
                upper: o.upper,
                lower: o.lower,
                percent_b: o.percent_b,
            })
    }
    /// Batch over two equally-sized arrays. Returns a flat array of length
    /// `4 * n`, interleaved per row as `[middle0, upper0, lower0, percentB0, ...]`.
    /// Read column `j` of row `i` as `result[i * 4 + j]`. Warmup rows are `NaN`.
    #[napi]
    pub fn batch(&mut self, a: Series, b: Series) -> napi::Result<Vec<f64>> {
        if a.len() != b.len() {
            return Err(NapiError::new(
                Status::InvalidArg,
                "a and b must be equal length".to_string(),
            ));
        }
        let mut out = vec![f64::NAN; a.len() * 4];
        for i in 0..a.len() {
            if let Some(o) = self.inner.update((a[i], b[i])) {
                out[i * 4] = o.middle;
                out[i * 4 + 1] = o.upper;
                out[i * 4 + 2] = o.lower;
                out[i * 4 + 3] = o.percent_b;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== MACD ==============================

/// MACD triple: macd line, signal line, histogram.
#[napi(object)]
pub struct MacdValue {
    pub macd: f64,
    pub signal: f64,
    pub histogram: f64,
}

#[napi(js_name = "MACD")]
pub struct MacdNode {
    inner: wc::MacdIndicator,
}

#[napi]
impl MacdNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count, signal: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::MacdIndicator::new(fast.get(), slow.get(), signal.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<MacdValue> {
        self.inner.update(value).map(|o| MacdValue {
            macd: o.macd,
            signal: o.signal,
            histogram: o.histogram,
        })
    }
    /// Batch over a price array. Returns a flat array of length `3 * n`,
    /// interleaved per row as `[macd0, signal0, histogram0, macd1, ...]`.
    /// Read column `j` of row `i` as `result[i * 3 + j]`. Warmup rows are `NaN`.
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![0.0; prices.len() * 3];
        self.inner.batch_macd_into(&prices, &mut out);
        out
    }
    /// Opt-in fast batch, flat `[macd, signal, histogram]` rows like `batch`.
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        let mut out = vec![0.0; prices.len() * 3];
        self.inner.batch_macd_fast_into(&prices, &mut out);
        Float64Array::new(out)
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len() * 3, &[&prices])?;
        self.inner.batch_macd_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len() * 3, &[&prices])?;
        self.inner.batch_macd_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "MACDFIX")]
pub struct MacdFixNode {
    inner: wc::MacdFix,
}

#[napi]
impl MacdFixNode {
    #[napi(constructor)]
    pub fn new(signal: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::MacdFix::new(signal.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<MacdValue> {
        self.inner.update(value).map(|o| MacdValue {
            macd: o.macd,
            signal: o.signal,
            histogram: o.histogram,
        })
    }
    /// Batch over a price array. Returns a flat array of length `3 * n`,
    /// interleaved per row as `[macd0, signal0, histogram0, macd1, ...]`.
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![f64::NAN; prices.len() * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.macd;
                out[i * 3 + 1] = o.signal;
                out[i * 3 + 2] = o.histogram;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "MACDEXT")]
pub struct MacdExtNode {
    inner: wc::MacdExt,
}

#[napi]
impl MacdExtNode {
    /// Moving-average types are TA-Lib `MA_Type` codes `0..=5`
    /// (SMA, EMA, WMA, DEMA, TEMA, TRIMA).
    #[napi(constructor)]
    pub fn new(
        fast: Count,
        fast_matype: u32,
        slow: Count,
        slow_matype: u32,
        signal: Count,
        signal_matype: u32,
    ) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::MacdExt::new(
                fast.get(),
                wc::MaType::from_code(fast_matype).map_err(map_err)?,
                slow.get(),
                wc::MaType::from_code(slow_matype).map_err(map_err)?,
                signal.get(),
                wc::MaType::from_code(signal_matype).map_err(map_err)?,
            )
            .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<MacdValue> {
        self.inner.update(value).map(|o| MacdValue {
            macd: o.macd,
            signal: o.signal,
            histogram: o.histogram,
        })
    }
    /// Batch over a price array. Returns a flat array of length `3 * n`,
    /// interleaved per row as `[macd0, signal0, histogram0, macd1, ...]`.
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![f64::NAN; prices.len() * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.macd;
                out[i * 3 + 1] = o.signal;
                out[i * 3 + 2] = o.histogram;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Bollinger ==============================

#[napi(object)]
pub struct BollingerValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
    pub stddev: f64,
}

#[napi(js_name = "BollingerBands")]
pub struct BollingerNode {
    inner: wc::BollingerBands,
}

#[napi]
impl BollingerNode {
    #[napi(constructor)]
    pub fn new(period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::BollingerBands::new(period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<BollingerValue> {
        self.inner.update(value).map(|o| BollingerValue {
            upper: o.upper,
            middle: o.middle,
            lower: o.lower,
            stddev: o.stddev,
        })
    }
    /// Batch over a price array. Returns a flat array of length `4 * n`,
    /// interleaved per row as `[upper0, middle0, lower0, stddev0, upper1, ...]`.
    /// Read column `j` of row `i` as `result[i * 4 + j]`. Warmup rows are `NaN`.
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![0.0; prices.len() * 4];
        self.inner.batch_bands_into(&prices, &mut out);
        out
    }
    /// Opt-in fast batch, flat `[upper, middle, lower, stddev]` rows like `batch`.
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        let mut out = vec![0.0; prices.len() * 4];
        self.inner.batch_bands_fast_into(&prices, &mut out);
        Float64Array::new(out)
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len() * 4, &[&prices])?;
        self.inner.batch_bands_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len() * 4, &[&prices])?;
        self.inner.batch_bands_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Candle-input helpers ==============================

fn cnd(h: f64, l: f64, c: f64, v: f64) -> napi::Result<wc::Candle> {
    wc::Candle::new(c, h, l, c, v, 0).map_err(map_err)
}

#[napi(js_name = "ATR")]
pub struct AtrNode {
    inner: wc::Atr,
}

#[napi]
impl AtrNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Atr::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        let n = same_length(&[&high, &low, &close], "high, low, close")?;
        // Every bar is validated first, as `update` would build it, so a bad bar
        // is refused before the indicator consumes anything.
        // The vectorized column check first; the per-bar loop only runs to
        // name the bad bar.
        if !wc::Candle::all_valid_hlc(&high, &low, &close) {
            for i in 0..n {
                cnd(high[i], low[i], close[i], 0.0)?;
            }
        }
        let mut out = vec![0.0; n];
        self.inner.batch_atr_into(&high, &low, &close, &mut out);
        Ok(out)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Float64Array> {
        let n = same_length(&[&high, &low, &close], "high, low, close")?;
        // Every bar is validated first, as `update` would build it, so a bad bar
        // is refused before the indicator consumes anything.
        // The vectorized column check first; the per-bar loop only runs to
        // name the bad bar.
        if !wc::Candle::all_valid_hlc(&high, &low, &close) {
            for i in 0..n {
                cnd(high[i], low[i], close[i], 0.0)?;
            }
        }
        let mut out = vec![0.0; n];
        self.inner
            .batch_atr_fast_into(&high, &low, &close, &mut out);
        Ok(Float64Array::new(out))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        mut out: OutSeries,
    ) -> napi::Result<()> {
        let n = same_length(&[&high, &low, &close], "high, low, close")?;
        // Every bar is validated first, as `update` would build it, so a bad bar
        // is refused before the indicator consumes anything.
        // The vectorized column check first; the per-bar loop only runs to
        // name the bad bar.
        if !wc::Candle::all_valid_hlc(&high, &low, &close) {
            for i in 0..n {
                cnd(high[i], low[i], close[i], 0.0)?;
            }
        }
        let rows = out.rows(n, &[&high, &low, &close])?;
        self.inner.batch_atr_into(&high, &low, &close, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        mut out: OutSeries,
    ) -> napi::Result<()> {
        let n = same_length(&[&high, &low, &close], "high, low, close")?;
        // Every bar is validated first, as `update` would build it, so a bad bar
        // is refused before the indicator consumes anything.
        // The vectorized column check first; the per-bar loop only runs to
        // name the bad bar.
        if !wc::Candle::all_valid_hlc(&high, &low, &close) {
            for i in 0..n {
                cnd(high[i], low[i], close[i], 0.0)?;
            }
        }
        let rows = out.rows(n, &[&high, &low, &close])?;
        self.inner.batch_atr_fast_into(&high, &low, &close, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "PLUS_DM")]
pub struct PlusDmNode {
    inner: wc::PlusDm,
}

#[napi]
impl PlusDmNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::PlusDm::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "MINUS_DM")]
pub struct MinusDmNode {
    inner: wc::MinusDm,
}

#[napi]
impl MinusDmNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::MinusDm::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "PLUS_DI")]
pub struct PlusDiNode {
    inner: wc::PlusDi,
}

#[napi]
impl PlusDiNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::PlusDi::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "MINUS_DI")]
pub struct MinusDiNode {
    inner: wc::MinusDi,
}

#[napi]
impl MinusDiNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::MinusDi::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "DX")]
pub struct DxNode {
    inner: wc::Dx,
}

#[napi]
impl DxNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Dx::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "MIDPRICE")]
pub struct MidPriceNode {
    inner: wc::MidPrice,
}

#[napi]
impl MidPriceNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::MidPrice::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "AVGPRICE")]
pub struct AvgPriceNode {
    inner: wc::AvgPrice,
}

impl Default for AvgPriceNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl AvgPriceNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::AvgPrice::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd4(open, high, low, close)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if !(open.len() == high.len() && high.len() == low.len() && low.len() == close.len()) {
            return Err(NapiError::from_reason(
                "open, high, low and close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd4(open[i], high[i], low[i], close[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "SAREXT")]
pub struct SarExtNode {
    inner: wc::SarExt,
}

#[napi]
impl SarExtNode {
    #[napi(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        start_value: f64,
        offset_on_reverse: f64,
        accel_init_long: f64,
        accel_long: f64,
        accel_max_long: f64,
        accel_init_short: f64,
        accel_short: f64,
        accel_max_short: f64,
    ) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::SarExt::new(
                start_value,
                offset_on_reverse,
                accel_init_long,
                accel_long,
                accel_max_long,
                accel_init_short,
                accel_short,
                accel_max_short,
            )
            .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct HtPhasorValue {
    pub inphase: f64,
    pub quadrature: f64,
}

#[napi(js_name = "HT_PHASOR")]
pub struct HtPhasorNode {
    inner: wc::HtPhasor,
}

impl Default for HtPhasorNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl HtPhasorNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::HtPhasor::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<HtPhasorValue> {
        self.inner.update(value).map(|o| HtPhasorValue {
            inphase: o.inphase,
            quadrature: o.quadrature,
        })
    }
    /// Batch over a price array. Returns a flat array of length `2 * n`,
    /// interleaved per row as `[inphase0, quadrature0, inphase1, ...]`.
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![f64::NAN; prices.len() * 2];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 2] = o.inphase;
                out[i * 2 + 1] = o.quadrature;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "CloseVsOpen")]
pub struct CloseVsOpenNode {
    inner: wc::CloseVsOpen,
}

impl Default for CloseVsOpenNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl CloseVsOpenNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::CloseVsOpen::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "BodySizePct")]
pub struct BodySizePctNode {
    inner: wc::BodySizePct,
}

impl Default for BodySizePctNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl BodySizePctNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::BodySizePct::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "WickRatio")]
pub struct WickRatioNode {
    inner: wc::WickRatio,
}

impl Default for WickRatioNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl WickRatioNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::WickRatio::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "HighLowRange")]
pub struct HighLowRangeNode {
    inner: wc::HighLowRange,
}

impl Default for HighLowRangeNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl HighLowRangeNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::HighLowRange::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "StochasticCCI")]
pub struct StochasticCciNode {
    inner: wc::StochasticCci,
}

#[napi]
impl StochasticCciNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::StochasticCci::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "IMI")]
pub struct ImiNode {
    inner: wc::IntradayMomentumIndex,
}

#[napi]
impl ImiNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::IntradayMomentumIndex::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd4(open, high, low, close)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let n = open.len();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            out.push(
                self.inner
                    .update(cnd4(open[i], high[i], low[i], close[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct QqeValue {
    pub rsi_ma: f64,
    pub trailing_line: f64,
}

#[napi(js_name = "QQE")]
pub struct QqeNode {
    inner: wc::Qqe,
}

#[napi]
impl QqeNode {
    #[napi(constructor)]
    pub fn new(rsi_period: Count, smoothing: Count, factor: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Qqe::new(rsi_period.get(), smoothing.get(), factor).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<QqeValue> {
        self.inner.update(value).map(|o| QqeValue {
            rsi_ma: o.rsi_ma,
            trailing_line: o.trailing_line,
        })
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![f64::NAN; prices.len() * 2];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 2] = o.rsi_ma;
                out[i * 2 + 1] = o.trailing_line;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct ElderRayValue {
    pub bull_power: f64,
    pub bear_power: f64,
}

#[napi(js_name = "ElderRay")]
pub struct ElderRayNode {
    inner: wc::ElderRay,
}

#[napi]
impl ElderRayNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ElderRay::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<ElderRayValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| ElderRayValue {
                bull_power: o.bull_power,
                bear_power: o.bear_power,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.bull_power;
                out[i * 2 + 1] = o.bear_power;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "TTM_TREND")]
pub struct TtmTrendNode {
    inner: wc::TtmTrend,
}

#[napi]
impl TtmTrendNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TtmTrend::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "Qstick")]
pub struct QstickNode {
    inner: wc::Qstick,
}

#[napi]
impl QstickNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Qstick::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, open: f64, close: f64) -> napi::Result<Option<f64>> {
        let hi = open.max(close);
        let lo = open.min(close);
        Ok(self.inner.update(cnd4(open, hi, lo, close)?))
    }
    #[napi]
    pub fn batch(&mut self, open: Series, close: Series) -> napi::Result<Vec<f64>> {
        if open.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let hi = open[i].max(close[i]);
            let lo = open[i].min(close[i]);
            out.push(
                self.inner
                    .update(cnd4(open[i], hi, lo, close[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "POLARIZED_FRACTAL_EFFICIENCY")]
pub struct PolarizedFractalEfficiencyNode {
    inner: wc::PolarizedFractalEfficiency,
}

#[napi]
impl PolarizedFractalEfficiencyNode {
    #[napi(constructor)]
    pub fn new(period: Count, smoothing: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::PolarizedFractalEfficiency::new(period.get(), smoothing.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "WAVE_PM")]
pub struct WavePmNode {
    inner: wc::WavePm,
}

#[napi]
impl WavePmNode {
    #[napi(constructor)]
    pub fn new(length: Count, smoothing: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::WavePm::new(length.get(), smoothing.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct GatorOscillatorValue {
    pub upper: f64,
    pub lower: f64,
}

#[napi(js_name = "GatorOscillator")]
pub struct GatorOscillatorNode {
    inner: wc::GatorOscillator,
}

#[napi]
impl GatorOscillatorNode {
    #[napi(constructor)]
    pub fn new(jaw_period: Count, teeth_period: Count, lips_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::GatorOscillator::new(
                jaw_period.get(),
                teeth_period.get(),
                lips_period.get(),
            )
            .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<GatorOscillatorValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| GatorOscillatorValue {
                upper: o.upper,
                lower: o.lower,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.upper;
                out[i * 2 + 1] = o.lower;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct KasePermissionStochasticValue {
    pub fast: f64,
    pub slow: f64,
}

#[napi(js_name = "KasePermissionStochastic")]
pub struct KasePermissionStochasticNode {
    inner: wc::KasePermissionStochastic,
}

#[napi]
impl KasePermissionStochasticNode {
    #[napi(constructor)]
    pub fn new(length: Count, smooth: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::KasePermissionStochastic::new(length.get(), smooth.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<KasePermissionStochasticValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| KasePermissionStochasticValue {
                fast: o.fast,
                slow: o.slow,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.fast;
                out[i * 2 + 1] = o.slow;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "VolatilityRatio")]
pub struct VolatilityRatioNode {
    inner: wc::VolatilityRatio,
}

#[napi]
impl VolatilityRatioNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::VolatilityRatio::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "ProjectionOscillator")]
pub struct ProjectionOscillatorNode {
    inner: wc::ProjectionOscillator,
}

#[napi]
impl ProjectionOscillatorNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ProjectionOscillator::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "TimeBasedStop")]
pub struct TimeBasedStopNode {
    inner: wc::TimeBasedStop,
}

#[napi]
impl TimeBasedStopNode {
    #[napi(constructor)]
    pub fn new(max_bars: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TimeBasedStop::new(max_bars.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "ADAPTIVECCI")]
pub struct AdaptiveCciNode {
    inner: wc::AdaptiveCci,
}

#[napi]
impl AdaptiveCciNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AdaptiveCci::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct StochValue {
    pub k: f64,
    pub d: f64,
}

#[napi(js_name = "Stochastic")]
pub struct StochNode {
    inner: wc::Stochastic,
}

#[napi]
impl StochNode {
    #[napi(constructor)]
    pub fn new(k_period: Count, d_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Stochastic::new(k_period.get(), d_period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<StochValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| StochValue { k: o.k, d: o.d }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.k;
                out[i * 2 + 1] = o.d;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "OBV")]
pub struct ObvNode {
    inner: wc::Obv,
}

impl Default for ObvNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl ObvNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::Obv::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, close: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(close, close, close, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "close and volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(close[i], close[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct AdxValue {
    #[napi(js_name = "plusDi")]
    pub plus_di: f64,
    #[napi(js_name = "minusDi")]
    pub minus_di: f64,
    pub adx: f64,
}

#[napi(js_name = "ADX")]
pub struct AdxNode {
    inner: wc::Adx,
}

#[napi]
impl AdxNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Adx::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<AdxValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| AdxValue {
                plus_di: o.plus_di,
                minus_di: o.minus_di,
                adx: o.adx,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 3] = o.plus_di;
                out[i * 3 + 1] = o.minus_di;
                out[i * 3 + 2] = o.adx;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "ADXR")]
pub struct AdxrNode {
    inner: wc::Adxr,
}

#[napi]
impl AdxrNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Adxr::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n];
        for i in 0..n {
            if let Some(v) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i] = v;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "CCI")]
pub struct CciNode {
    inner: wc::Cci,
}
#[napi]
impl CciNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Cci::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
}

#[napi(js_name = "WilliamsR")]
pub struct WilliamsRNode {
    inner: wc::WilliamsR,
}
#[napi]
impl WilliamsRNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::WilliamsR::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
}

#[napi(js_name = "MFI")]
pub struct MfiNode {
    inner: wc::Mfi,
}
#[napi]
impl MfiNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Mfi::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
}

#[napi(js_name = "PSAR")]
pub struct PsarNode {
    inner: wc::Psar,
}
#[napi]
impl PsarNode {
    #[napi(constructor)]
    pub fn new(af_start: f64, af_step: f64, af_max: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Psar::new(af_start, af_step, af_max).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
}

#[napi(object)]
pub struct KeltnerValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "Keltner")]
pub struct KeltnerNode {
    inner: wc::Keltner,
}
#[napi]
impl KeltnerNode {
    #[napi(constructor)]
    pub fn new(ema_period: Count, atr_period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Keltner::new(ema_period.get(), atr_period.get(), multiplier)
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<KeltnerValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| KeltnerValue {
                upper: o.upper,
                middle: o.middle,
                lower: o.lower,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(out)
    }
}

#[napi(object)]
pub struct DonchianValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "Donchian")]
pub struct DonchianNode {
    inner: wc::Donchian,
}
#[napi]
impl DonchianNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Donchian::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<DonchianValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, low, 0.0)?)
            .map(|o| DonchianValue {
                upper: o.upper,
                middle: o.middle,
                lower: o.lower,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], low[i], 0.0)?) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(out)
    }
}

#[napi(js_name = "VWAP")]
pub struct VwapNode {
    inner: wc::Vwap,
}
impl Default for VwapNode {
    fn default() -> Self {
        Self::new()
    }
}
#[napi]
impl VwapNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::Vwap::new(),
        }
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
}

#[napi(js_name = "RollingVWAP")]
pub struct RollingVwapNode {
    inner: wc::RollingVwap,
}
#[napi]
impl RollingVwapNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RollingVwap::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi(getter)]
    pub fn period(&self) -> u32 {
        self.inner.period() as u32
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
}

#[napi(js_name = "AwesomeOscillator")]
pub struct AoNode {
    inner: wc::AwesomeOscillator,
}
#[napi]
impl AoNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AwesomeOscillator::new(fast.get(), slow.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, low, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], low[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
}

#[napi(object)]
pub struct AroonValue {
    pub up: f64,
    pub down: f64,
}

#[napi(js_name = "Aroon")]
pub struct AroonNode {
    inner: wc::Aroon,
}
#[napi]
impl AroonNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Aroon::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<AroonValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, low, 0.0)?)
            .map(|o| AroonValue {
                up: o.up,
                down: o.down,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], low[i], 0.0)?) {
                out[i * 2] = o.up;
                out[i * 2 + 1] = o.down;
            }
        }
        Ok(out)
    }
}

// Helper for OHLC-input indicators that need the open price (not provided
// by `cnd` which fakes open == close).
fn cnd4(open: f64, high: f64, low: f64, close: f64) -> napi::Result<wc::Candle> {
    wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)
}

#[napi(js_name = "Inertia")]
pub struct InertiaNode {
    inner: wc::Inertia,
}
#[napi]
impl InertiaNode {
    #[napi(constructor)]
    pub fn new(rvi_period: Count, linreg_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Inertia::new(rvi_period.get(), linreg_period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd4(open, high, low, close)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if !(open.len() == high.len() && high.len() == low.len() && low.len() == close.len()) {
            return Err(NapiError::from_reason(
                "open, high, low and close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd4(open[i], high[i], low[i], close[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "ConnorsRSI")]
pub struct ConnorsRsiNode {
    inner: wc::ConnorsRsi,
}
#[napi]
impl ConnorsRsiNode {
    #[napi(constructor)]
    pub fn new(period_rsi: Count, period_streak: Count, period_rank: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ConnorsRsi::new(period_rsi.get(), period_streak.get(), period_rank.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "LaguerreRSI")]
pub struct LaguerreRsiNode {
    inner: wc::LaguerreRsi,
}
#[napi]
impl LaguerreRsiNode {
    #[napi(constructor)]
    pub fn new(gamma: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::LaguerreRsi::new(gamma).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "SMI")]
pub struct SmiNode {
    inner: wc::Smi,
}
#[napi]
impl SmiNode {
    #[napi(constructor)]
    pub fn new(period: Count, d_period: Count, d2_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Smi::new(period.get(), d_period.get(), d2_period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if !(high.len() == low.len() && low.len() == close.len()) {
            return Err(NapiError::from_reason(
                "high, low and close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct KstValue {
    pub kst: f64,
    pub signal: f64,
}

#[napi(js_name = "KST")]
pub struct KstNode {
    inner: wc::Kst,
}
#[napi]
impl KstNode {
    #[napi(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        roc1: Count,
        roc2: Count,
        roc3: Count,
        roc4: Count,
        sma1: Count,
        sma2: Count,
        sma3: Count,
        sma4: Count,
        signal: Count,
    ) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Kst::new(
                roc1.get(),
                roc2.get(),
                roc3.get(),
                roc4.get(),
                sma1.get(),
                sma2.get(),
                sma3.get(),
                sma4.get(),
                signal.get(),
            )
            .map_err(map_err)?,
        })
    }
    #[napi(factory)]
    pub fn classic() -> Self {
        Self {
            inner: wc::Kst::classic(),
        }
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<KstValue> {
        self.inner.update(value).map(|o| KstValue {
            kst: o.kst,
            signal: o.signal,
        })
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 2];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 2] = o.kst;
                out[i * 2 + 1] = o.signal;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "PGO")]
pub struct PgoNode {
    inner: wc::Pgo,
}
#[napi]
impl PgoNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Pgo::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if !(high.len() == low.len() && low.len() == close.len()) {
            return Err(NapiError::from_reason(
                "high, low and close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "RVI")]
pub struct RviNode {
    inner: wc::Rvi,
}
#[napi]
impl RviNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Rvi::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd4(open, high, low, close)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if !(open.len() == high.len() && high.len() == low.len() && low.len() == close.len()) {
            return Err(NapiError::from_reason(
                "open, high, low and close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd4(open[i], high[i], low[i], close[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "AwesomeOscillatorHistogram")]
pub struct AwesomeOscillatorHistogramNode {
    inner: wc::AwesomeOscillatorHistogram,
}
#[napi]
impl AwesomeOscillatorHistogramNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count, sma_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AwesomeOscillatorHistogram::new(fast.get(), slow.get(), sma_period.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, low, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], low[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "STC")]
pub struct StcNode {
    inner: wc::Stc,
}
#[napi]
impl StcNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count, schaff_period: Count, factor: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Stc::new(fast.get(), slow.get(), schaff_period.get(), factor)
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "ElderImpulse")]
pub struct ElderImpulseNode {
    inner: wc::ElderImpulse,
}
#[napi]
impl ElderImpulseNode {
    #[napi(constructor)]
    pub fn new(
        ema_period: Count,
        macd_fast: Count,
        macd_slow: Count,
        macd_signal: Count,
    ) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ElderImpulse::new(
                ema_period.get(),
                macd_fast.get(),
                macd_slow.get(),
                macd_signal.get(),
            )
            .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct ZeroLagMacdValue {
    pub macd: f64,
    pub signal: f64,
    pub histogram: f64,
}

#[napi(js_name = "ZeroLagMACD")]
pub struct ZeroLagMacdNode {
    inner: wc::ZeroLagMacd,
}
#[napi]
impl ZeroLagMacdNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count, signal: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ZeroLagMacd::new(fast.get(), slow.get(), signal.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<ZeroLagMacdValue> {
        self.inner.update(value).map(|o| ZeroLagMacdValue {
            macd: o.macd,
            signal: o.signal,
            histogram: o.histogram,
        })
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.macd;
                out[i * 3 + 1] = o.signal;
                out[i * 3 + 2] = o.histogram;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "CFO")]
pub struct CfoNode {
    inner: wc::Cfo,
}
#[napi]
impl CfoNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Cfo::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "APO")]
pub struct ApoNode {
    inner: wc::Apo,
}
#[napi]
impl ApoNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Apo::new(fast.get(), slow.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "KAMA")]
pub struct KamaNode {
    inner: wc::Kama,
}
#[napi]
impl KamaNode {
    #[napi(constructor)]
    pub fn new(er_period: Count, fast: Count, slow: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Kama::new(er_period.get(), fast.get(), slow.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
}

// ============================== EVWMA ==============================

#[napi(js_name = "EVWMA")]
pub struct EvwmaNode {
    inner: wc::Evwma,
}
#[napi]
impl EvwmaNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Evwma::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, close: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(close, close, close, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "close and volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(close[i], close[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Alligator ==============================

#[napi(object)]
pub struct AlligatorValue {
    pub jaw: f64,
    pub teeth: f64,
    pub lips: f64,
}

#[napi(js_name = "Alligator")]
pub struct AlligatorNode {
    inner: wc::Alligator,
}
#[napi]
impl AlligatorNode {
    #[napi(constructor)]
    pub fn new(jaw: Count, teeth: Count, lips: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Alligator::new(jaw.get(), teeth.get(), lips.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<AlligatorValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, low, 0.0)?)
            .map(|o| AlligatorValue {
                jaw: o.jaw,
                teeth: o.teeth,
                lips: o.lips,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], low[i], 0.0)?) {
                out[i * 3] = o.jaw;
                out[i * 3 + 1] = o.teeth;
                out[i * 3 + 2] = o.lips;
            }
        }
        Ok(out)
    }
}

// ============================== JMA ==============================

#[napi(js_name = "JMA")]
pub struct JmaNode {
    inner: wc::Jma,
}
#[napi]
impl JmaNode {
    #[napi(constructor)]
    pub fn new(period: Count, phase: f64, power: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Jma::new(period.get(), phase, narrow(power, "power")?).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
}

// ============================== VIDYA ==============================

#[napi(js_name = "VIDYA")]
pub struct VidyaNode {
    inner: wc::Vidya,
}
#[napi]
impl VidyaNode {
    #[napi(constructor)]
    pub fn new(period: Count, cmo_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Vidya::new(period.get(), cmo_period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
}

// ============================== ALMA ==============================

#[napi(js_name = "ALMA")]
pub struct AlmaNode {
    inner: wc::Alma,
}
#[napi]
impl AlmaNode {
    #[napi(constructor)]
    pub fn new(period: Count, offset: f64, sigma: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Alma::new(period.get(), offset, sigma).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
}

// ============================== T3 ==============================

#[napi(js_name = "T3")]
pub struct T3Node {
    inner: wc::T3,
}

#[napi]
impl T3Node {
    #[napi(constructor)]
    pub fn new(period: Count, v: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::T3::new(period.get(), v).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== GD ==============================

#[napi(js_name = "GD")]
pub struct GeneralizedDemaNode {
    inner: wc::GeneralizedDema,
}

#[napi]
impl GeneralizedDemaNode {
    #[napi(constructor)]
    pub fn new(period: Count, v: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::GeneralizedDema::new(period.get(), v).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== HoltWinters ==============================

#[napi(js_name = "HoltWinters")]
pub struct HoltWintersNode {
    inner: wc::HoltWinters,
}

#[napi]
impl HoltWintersNode {
    #[napi(constructor)]
    pub fn new(alpha: f64, beta: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::HoltWinters::new(alpha, beta).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== RMI ==============================

#[napi(js_name = "RMI")]
pub struct RmiNode {
    inner: wc::Rmi,
}

#[napi]
impl RmiNode {
    #[napi(constructor)]
    pub fn new(period: Count, momentum: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Rmi::new(period.get(), momentum.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== DerivativeOscillator ==============================

#[napi(js_name = "DerivativeOscillator")]
pub struct DerivativeOscillatorNode {
    inner: wc::DerivativeOscillator,
}

#[napi]
impl DerivativeOscillatorNode {
    #[napi(constructor)]
    pub fn new(
        rsi_period: Count,
        smooth1: Count,
        smooth2: Count,
        signal_period: Count,
    ) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::DerivativeOscillator::new(
                rsi_period.get(),
                smooth1.get(),
                smooth2.get(),
                signal_period.get(),
            )
            .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== MacdHistogram ==============================

#[napi(js_name = "MacdHistogram")]
pub struct MacdHistogramNode {
    inner: wc::MacdHistogram,
}

#[napi]
impl MacdHistogramNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count, signal: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::MacdHistogram::new(fast.get(), slow.get(), signal.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== PpoHistogram ==============================

#[napi(js_name = "PpoHistogram")]
pub struct PpoHistogramNode {
    inner: wc::PpoHistogram,
}

#[napi]
impl PpoHistogramNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count, signal: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::PpoHistogram::new(fast.get(), slow.get(), signal.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TSI ==============================

#[napi(js_name = "TSI")]
pub struct TsiNode {
    inner: wc::Tsi,
}

#[napi]
impl TsiNode {
    #[napi(constructor)]
    pub fn new(long: Count, short: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Tsi::new(long.get(), short.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== PMO ==============================

#[napi(js_name = "PMO")]
pub struct PmoNode {
    inner: wc::Pmo,
}

#[napi]
impl PmoNode {
    #[napi(constructor)]
    pub fn new(smoothing1: Count, smoothing2: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Pmo::new(smoothing1.get(), smoothing2.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== VWMA ==============================

// ============================== TII ==============================

#[napi(js_name = "TII")]
pub struct TiiNode {
    inner: wc::Tii,
}

#[napi]
impl TiiNode {
    #[napi(constructor)]
    pub fn new(sma_period: Count, dev_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Tii::new(sma_period.get(), dev_period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== ADL ==============================

#[napi(js_name = "ADL")]
pub struct AdlNode {
    inner: wc::Adl,
}

impl Default for AdlNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl AdlNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::Adl::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Volume-Price Trend ==============================

#[napi(js_name = "VolumePriceTrend")]
pub struct VolumePriceTrendNode {
    inner: wc::VolumePriceTrend,
}

impl Default for VolumePriceTrendNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl VolumePriceTrendNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::VolumePriceTrend::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, close: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(close, close, close, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "close and volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(close[i], close[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Chaikin Money Flow ==============================

#[napi(js_name = "ChaikinMoneyFlow")]
pub struct ChaikinMoneyFlowNode {
    inner: wc::ChaikinMoneyFlow,
}

#[napi]
impl ChaikinMoneyFlowNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ChaikinMoneyFlow::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Chaikin Oscillator ==============================

#[napi(js_name = "ChaikinOscillator")]
pub struct ChaikinOscillatorNode {
    inner: wc::ChaikinOscillator,
}

#[napi]
impl ChaikinOscillatorNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ChaikinOscillator::new(fast.get(), slow.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        let n = same_length(&[&high, &low, &close, &volume], "high, low, close, volume")?;
        // Every bar is validated first, as `update` would build it, so a bad bar
        // is refused before the indicator consumes anything.
        // The vectorized column check first; the per-bar loop only runs to
        // name the bad bar.
        if !wc::Candle::all_valid(&close, &high, &low, &close, &volume) {
            for i in 0..n {
                cnd(high[i], low[i], close[i], volume[i])?;
            }
        }
        let mut out = vec![0.0; n];
        self.inner
            .batch_hlcv_into(&high, &low, &close, &volume, &mut out);
        Ok(out)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Float64Array> {
        let n = same_length(&[&high, &low, &close, &volume], "high, low, close, volume")?;
        // Every bar is validated first, as `update` would build it, so a bad bar
        // is refused before the indicator consumes anything.
        // The vectorized column check first; the per-bar loop only runs to
        // name the bad bar.
        if !wc::Candle::all_valid(&close, &high, &low, &close, &volume) {
            for i in 0..n {
                cnd(high[i], low[i], close[i], volume[i])?;
            }
        }
        let mut out = vec![0.0; n];
        self.inner
            .batch_hlcv_fast_into(&high, &low, &close, &volume, &mut out);
        Ok(Float64Array::new(out))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
        mut out: OutSeries,
    ) -> napi::Result<()> {
        let n = same_length(&[&high, &low, &close, &volume], "high, low, close, volume")?;
        // Every bar is validated first, as `update` would build it, so a bad bar
        // is refused before the indicator consumes anything.
        // The vectorized column check first; the per-bar loop only runs to
        // name the bad bar.
        if !wc::Candle::all_valid(&close, &high, &low, &close, &volume) {
            for i in 0..n {
                cnd(high[i], low[i], close[i], volume[i])?;
            }
        }
        let rows = out.rows(n, &[&high, &low, &close, &volume])?;
        self.inner
            .batch_hlcv_into(&high, &low, &close, &volume, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
        mut out: OutSeries,
    ) -> napi::Result<()> {
        let n = same_length(&[&high, &low, &close, &volume], "high, low, close, volume")?;
        // Every bar is validated first, as `update` would build it, so a bad bar
        // is refused before the indicator consumes anything.
        // The vectorized column check first; the per-bar loop only runs to
        // name the bad bar.
        if !wc::Candle::all_valid(&close, &high, &low, &close, &volume) {
            for i in 0..n {
                cnd(high[i], low[i], close[i], volume[i])?;
            }
        }
        let rows = out.rows(n, &[&high, &low, &close, &volume])?;
        self.inner
            .batch_hlcv_fast_into(&high, &low, &close, &volume, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Force Index ==============================

#[napi(js_name = "ForceIndex")]
pub struct ForceIndexNode {
    inner: wc::ForceIndex,
}

#[napi]
impl ForceIndexNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ForceIndex::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, close: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(close, close, close, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "close and volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(close[i], close[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Negative Volume Index ==============================

#[napi(js_name = "NVI")]
pub struct NviNode {
    inner: wc::Nvi,
}

#[napi]
impl NviNode {
    #[napi(constructor)]
    pub fn new(baseline: Option<f64>) -> Self {
        Self {
            inner: wc::Nvi::with_baseline(baseline.unwrap_or(1000.0)),
        }
    }
    #[napi]
    pub fn update(&mut self, close: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(close, close, close, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "close and volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(close[i], close[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Positive Volume Index ==============================

#[napi(js_name = "PVI")]
pub struct PviNode {
    inner: wc::Pvi,
}

#[napi]
impl PviNode {
    #[napi(constructor)]
    pub fn new(baseline: Option<f64>) -> Self {
        Self {
            inner: wc::Pvi::with_baseline(baseline.unwrap_or(1000.0)),
        }
    }
    #[napi]
    pub fn update(&mut self, close: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(close, close, close, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "close and volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(close[i], close[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Volume Oscillator ==============================

#[napi(js_name = "VolumeOscillator")]
pub struct VolumeOscillatorNode {
    inner: wc::VolumeOscillator,
}

#[napi]
impl VolumeOscillatorNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::VolumeOscillator::new(fast.get(), slow.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(10.0, 10.0, 10.0, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, volume: Series) -> napi::Result<Vec<f64>> {
        let mut out = Vec::with_capacity(volume.len());
        for &v in &volume {
            out.push(
                self.inner
                    .update(cnd(10.0, 10.0, 10.0, v)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Klinger Volume Oscillator ==============================

#[napi(js_name = "KVO")]
pub struct KvoNode {
    inner: wc::Kvo,
}

#[napi]
impl KvoNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Kvo::new(fast.get(), slow.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Williams A/D ==============================

#[napi(js_name = "ADOSC")]
pub struct AdOscillatorNode {
    inner: wc::AdOscillator,
}

#[napi]
impl AdOscillatorNode {
    #[napi(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            inner: wc::AdOscillator::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Anchored RSI ==============================

#[napi(js_name = "AnchoredRSI")]
pub struct AnchoredRsiNode {
    inner: wc::AnchoredRsi,
}

#[napi]
impl AnchoredRsiNode {
    #[napi(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            inner: wc::AnchoredRsi::new(),
        }
    }
    #[napi(js_name = "setAnchor")]
    pub fn set_anchor(&mut self) {
        self.inner.set_anchor();
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Anchored VWAP ==============================

#[napi(js_name = "AnchoredVWAP")]
pub struct AnchoredVwapNode {
    inner: wc::AnchoredVwap,
}

#[napi]
impl AnchoredVwapNode {
    #[napi(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            inner: wc::AnchoredVwap::new(),
        }
    }
    #[napi(js_name = "setAnchor")]
    pub fn set_anchor(&mut self) {
        self.inner.set_anchor();
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Demand Index ==============================

#[napi(js_name = "DemandIndex")]
pub struct DemandIndexNode {
    inner: wc::DemandIndex,
}

#[napi]
impl DemandIndexNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::DemandIndex::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Time Segmented Volume ==============================

#[napi(js_name = "TSV")]
pub struct TsvNode {
    inner: wc::Tsv,
}

#[napi]
impl TsvNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Tsv::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, close: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(close, close, close, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "close and volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(close[i], close[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Volume Zone Oscillator ==============================

#[napi(js_name = "VZO")]
pub struct VzoNode {
    inner: wc::Vzo,
}

#[napi]
impl VzoNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Vzo::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, close: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(close, close, close, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "close and volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(close[i], close[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Market Facilitation Index ==============================

#[napi(js_name = "MarketFacilitationIndex")]
pub struct MarketFacilitationIndexNode {
    inner: wc::MarketFacilitationIndex,
}

#[napi]
impl MarketFacilitationIndexNode {
    #[napi(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            inner: wc::MarketFacilitationIndex::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, low, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], low[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Ease of Movement ==============================

#[napi(js_name = "EaseOfMovement")]
pub struct EaseOfMovementNode {
    inner: wc::EaseOfMovement,
}

#[napi]
impl EaseOfMovementNode {
    #[napi(constructor)]
    pub fn new(period: Count, divisor: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::EaseOfMovement::with_divisor(period.get(), divisor).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, low, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], low[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== SuperTrend ==============================

#[napi(object)]
pub struct SuperTrendValue {
    pub value: f64,
    pub direction: f64,
}

#[napi(js_name = "SuperTrend")]
pub struct SuperTrendNode {
    inner: wc::SuperTrend,
}

#[napi]
impl SuperTrendNode {
    #[napi(constructor)]
    pub fn new(atr_period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::SuperTrend::new(atr_period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<SuperTrendValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| SuperTrendValue {
                value: o.value,
                direction: o.direction,
            }))
    }
    /// Returns `[value0, direction0, value1, direction1, ...]`, length `2 * n`.
    /// Warmup positions are `NaN`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Chandelier Exit ==============================

#[napi(object)]
pub struct ChandelierExitValue {
    pub long_stop: f64,
    pub short_stop: f64,
}

#[napi(js_name = "ChandelierExit")]
pub struct ChandelierExitNode {
    inner: wc::ChandelierExit,
}

#[napi]
impl ChandelierExitNode {
    #[napi(constructor)]
    pub fn new(period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ChandelierExit::new(period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<ChandelierExitValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| ChandelierExitValue {
                long_stop: o.long_stop,
                short_stop: o.short_stop,
            }))
    }
    /// Returns `[long0, short0, long1, short1, ...]`, length `2 * n`. Warmup
    /// positions are `NaN`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.long_stop;
                out[i * 2 + 1] = o.short_stop;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Chande Kroll Stop ==============================

#[napi(object)]
pub struct ChandeKrollStopValue {
    pub stop_long: f64,
    pub stop_short: f64,
}

#[napi(js_name = "ChandeKrollStop")]
pub struct ChandeKrollStopNode {
    inner: wc::ChandeKrollStop,
}

#[napi]
impl ChandeKrollStopNode {
    #[napi(constructor)]
    pub fn new(atr_period: Count, atr_multiplier: f64, stop_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ChandeKrollStop::new(atr_period.get(), atr_multiplier, stop_period.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<ChandeKrollStopValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| ChandeKrollStopValue {
                stop_long: o.stop_long,
                stop_short: o.stop_short,
            }))
    }
    /// Returns `[long0, short0, long1, short1, ...]`, length `2 * n`. Warmup
    /// positions are `NaN`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.stop_long;
                out[i * 2 + 1] = o.stop_short;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== ATR Trailing Stop ==============================

#[napi(js_name = "AtrTrailingStop")]
pub struct AtrTrailingStopNode {
    inner: wc::AtrTrailingStop,
}

#[napi]
impl AtrTrailingStopNode {
    #[napi(constructor)]
    pub fn new(atr_period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AtrTrailingStop::new(atr_period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== HiLo Activator ==============================

#[napi(js_name = "HiLoActivator")]
pub struct HiLoActivatorNode {
    inner: wc::HiLoActivator,
}

#[napi]
impl HiLoActivatorNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::HiLoActivator::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Volty Stop ==============================

#[napi(js_name = "VoltyStop")]
pub struct VoltyStopNode {
    inner: wc::VoltyStop,
}

#[napi]
impl VoltyStopNode {
    #[napi(constructor)]
    pub fn new(atr_period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::VoltyStop::new(atr_period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Yo-Yo Exit ==============================

#[napi(js_name = "YoyoExit")]
pub struct YoyoExitNode {
    inner: wc::YoyoExit,
}

#[napi]
impl YoyoExitNode {
    #[napi(constructor)]
    pub fn new(atr_period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::YoyoExit::new(atr_period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi(js_name = "inTrade")]
    pub fn in_trade(&self) -> bool {
        self.inner.in_trade()
    }
}

// ============================== Donchian Stop ==============================

#[napi(object)]
pub struct DonchianStopValue {
    pub stop_long: f64,
    pub stop_short: f64,
}

#[napi(js_name = "DonchianStop")]
pub struct DonchianStopNode {
    inner: wc::DonchianStop,
}

#[napi]
impl DonchianStopNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::DonchianStop::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<DonchianStopValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, low, 0.0)?)
            .map(|o| DonchianStopValue {
                stop_long: o.stop_long,
                stop_short: o.stop_short,
            }))
    }
    /// Returns `[long0, short0, long1, short1, ...]`, length `2 * n`. Warmup
    /// positions are `NaN`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], low[i], 0.0)?) {
                out[i * 2] = o.stop_long;
                out[i * 2 + 1] = o.stop_short;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Percentage Trailing Stop ==============================

#[napi(js_name = "PercentageTrailingStop")]
pub struct PercentageTrailingStopNode {
    inner: wc::PercentageTrailingStop,
}

#[napi]
impl PercentageTrailingStopNode {
    #[napi(constructor)]
    pub fn new(percent: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::PercentageTrailingStop::new(percent).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Step Trailing Stop ==============================

#[napi(js_name = "StepTrailingStop")]
pub struct StepTrailingStopNode {
    inner: wc::StepTrailingStop,
}

#[napi]
impl StepTrailingStopNode {
    #[napi(constructor)]
    pub fn new(step_size: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::StepTrailingStop::new(step_size).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Renko Trailing Stop ==============================

#[napi(js_name = "RenkoTrailingStop")]
pub struct RenkoTrailingStopNode {
    inner: wc::RenkoTrailingStop,
}

#[napi]
impl RenkoTrailingStopNode {
    #[napi(constructor)]
    pub fn new(block_size: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RenkoTrailingStop::new(block_size).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Kase DevStop ==============================

#[napi(object)]
pub struct KaseDevStopValue {
    pub value: f64,
    pub direction: f64,
}

#[napi(js_name = "KaseDevStop")]
pub struct KaseDevStopNode {
    inner: wc::KaseDevStop,
}

#[napi]
impl KaseDevStopNode {
    #[napi(constructor)]
    pub fn new(period: Count, dev: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::KaseDevStop::new(period.get(), dev).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<KaseDevStopValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| KaseDevStopValue {
                value: o.value,
                direction: o.direction,
            }))
    }
    /// Returns `[value0, direction0, value1, direction1, ...]`, length `2 * n`.
    /// Warmup positions are `NaN`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Elder SafeZone ==============================

#[napi(object)]
pub struct ElderSafeZoneValue {
    pub value: f64,
    pub direction: f64,
}

#[napi(js_name = "ElderSafeZone")]
pub struct ElderSafeZoneNode {
    inner: wc::ElderSafeZone,
}

#[napi]
impl ElderSafeZoneNode {
    #[napi(constructor)]
    pub fn new(period: Count, coeff: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ElderSafeZone::new(period.get(), coeff).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<ElderSafeZoneValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| ElderSafeZoneValue {
                value: o.value,
                direction: o.direction,
            }))
    }
    /// Returns `[value0, direction0, value1, direction1, ...]`, length `2 * n`.
    /// Warmup positions are `NaN`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== ATR Ratchet ==============================

#[napi(object)]
pub struct AtrRatchetValue {
    pub value: f64,
    pub direction: f64,
}

#[napi(js_name = "AtrRatchet")]
pub struct AtrRatchetNode {
    inner: wc::AtrRatchet,
}

#[napi]
impl AtrRatchetNode {
    #[napi(constructor)]
    pub fn new(atr_period: Count, start_mult: f64, increment: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AtrRatchet::new(atr_period.get(), start_mult, increment).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<AtrRatchetValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| AtrRatchetValue {
                value: o.value,
                direction: o.direction,
            }))
    }
    /// Returns `[value0, direction0, value1, direction1, ...]`, length `2 * n`.
    /// Warmup positions are `NaN`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== NRTR ==============================

#[napi(object)]
pub struct NrtrValue {
    pub value: f64,
    pub direction: f64,
}

#[napi(js_name = "Nrtr")]
pub struct NrtrNode {
    inner: wc::Nrtr,
}

#[napi]
impl NrtrNode {
    #[napi(constructor)]
    pub fn new(pct: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Nrtr::new(pct).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<NrtrValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| NrtrValue {
                value: o.value,
                direction: o.direction,
            }))
    }
    /// Returns `[value0, direction0, value1, direction1, ...]`, length `2 * n`.
    /// Warmup positions are `NaN`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Modified MA Stop ==============================

#[napi(object)]
pub struct ModifiedMaStopValue {
    pub value: f64,
    pub direction: f64,
}

#[napi(js_name = "ModifiedMaStop")]
pub struct ModifiedMaStopNode {
    inner: wc::ModifiedMaStop,
}

#[napi]
impl ModifiedMaStopNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ModifiedMaStop::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<ModifiedMaStopValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| ModifiedMaStopValue {
                value: o.value,
                direction: o.direction,
            }))
    }
    /// Returns `[value0, direction0, value1, direction1, ...]`, length `2 * n`.
    /// Warmup positions are `NaN`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Typical Price ==============================

#[napi(js_name = "TypicalPrice")]
pub struct TypicalPriceNode {
    inner: wc::TypicalPrice,
}

impl Default for TypicalPriceNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl TypicalPriceNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::TypicalPrice::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Median Price ==============================

#[napi(js_name = "MedianPrice")]
pub struct MedianPriceNode {
    inner: wc::MedianPrice,
}

impl Default for MedianPriceNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl MedianPriceNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::MedianPrice::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, low, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], low[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Weighted Close ==============================

#[napi(js_name = "WeightedClose")]
pub struct WeightedCloseNode {
    inner: wc::WeightedClose,
}

impl Default for WeightedCloseNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl WeightedCloseNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::WeightedClose::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Linear Regression ==============================

#[napi(js_name = "LinearRegression")]
pub struct LinearRegressionNode {
    inner: wc::LinearRegression,
}

#[napi]
impl LinearRegressionNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::LinearRegression::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Linear Regression Slope ==============================

#[napi(js_name = "LinRegSlope")]
pub struct LinRegSlopeNode {
    inner: wc::LinRegSlope,
}

#[napi]
impl LinRegSlopeNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::LinRegSlope::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Accelerator Oscillator ==============================

#[napi(js_name = "AcceleratorOscillator")]
pub struct AcceleratorOscillatorNode {
    inner: wc::AcceleratorOscillator,
}

#[napi]
impl AcceleratorOscillatorNode {
    #[napi(constructor)]
    pub fn new(ao_fast: Count, ao_slow: Count, signal_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AcceleratorOscillator::new(
                ao_fast.get(),
                ao_slow.get(),
                signal_period.get(),
            )
            .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, low, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], low[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Balance of Power ==============================

#[napi(js_name = "BalanceOfPower")]
pub struct BalanceOfPowerNode {
    inner: wc::BalanceOfPower,
}

impl Default for BalanceOfPowerNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl BalanceOfPowerNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::BalanceOfPower::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Choppiness Index ==============================

#[napi(js_name = "ChoppinessIndex")]
pub struct ChoppinessIndexNode {
    inner: wc::ChoppinessIndex,
}

#[napi]
impl ChoppinessIndexNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ChoppinessIndex::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== True Range ==============================

#[napi(js_name = "TrueRange")]
pub struct TrueRangeNode {
    inner: wc::TrueRange,
}

impl Default for TrueRangeNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl TrueRangeNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::TrueRange::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Chaikin Volatility ==============================

#[napi(js_name = "ChaikinVolatility")]
pub struct ChaikinVolatilityNode {
    inner: wc::ChaikinVolatility,
}

#[napi]
impl ChaikinVolatilityNode {
    #[napi(constructor)]
    pub fn new(ema_period: Count, roc_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ChaikinVolatility::new(ema_period.get(), roc_period.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, low, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], low[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Yang-Zhang Volatility ==============================

#[napi(js_name = "YangZhangVolatility")]
pub struct YangZhangVolatilityNode {
    inner: wc::YangZhangVolatility,
}

#[napi]
impl YangZhangVolatilityNode {
    #[napi(constructor)]
    pub fn new(period: Count, trading_periods: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::YangZhangVolatility::new(period.get(), trading_periods.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        let n = open.len();
        if high.len() != n || low.len() != n || close.len() != n {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Rogers-Satchell Volatility ==============================

#[napi(js_name = "RogersSatchellVolatility")]
pub struct RogersSatchellVolatilityNode {
    inner: wc::RogersSatchellVolatility,
}

#[napi]
impl RogersSatchellVolatilityNode {
    #[napi(constructor)]
    pub fn new(period: Count, trading_periods: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RogersSatchellVolatility::new(period.get(), trading_periods.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        let n = open.len();
        if high.len() != n || low.len() != n || close.len() != n {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Garman-Klass Volatility ==============================

#[napi(js_name = "GarmanKlassVolatility")]
pub struct GarmanKlassVolatilityNode {
    inner: wc::GarmanKlassVolatility,
}

#[napi]
impl GarmanKlassVolatilityNode {
    #[napi(constructor)]
    pub fn new(period: Count, trading_periods: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::GarmanKlassVolatility::new(period.get(), trading_periods.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        let n = open.len();
        if high.len() != n || low.len() != n || close.len() != n {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Parkinson Volatility ==============================

#[napi(js_name = "ParkinsonVolatility")]
pub struct ParkinsonVolatilityNode {
    inner: wc::ParkinsonVolatility,
}

#[napi]
impl ParkinsonVolatilityNode {
    #[napi(constructor)]
    pub fn new(period: Count, trading_periods: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ParkinsonVolatility::new(period.get(), trading_periods.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, low, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], low[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Linear Regression Angle ==============================

#[napi(js_name = "LinRegAngle")]
pub struct LinRegAngleNode {
    inner: wc::LinRegAngle,
}

#[napi]
impl LinRegAngleNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::LinRegAngle::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Bollinger Bandwidth ==============================

#[napi(js_name = "BollingerBandwidth")]
pub struct BollingerBandwidthNode {
    inner: wc::BollingerBandwidth,
}

#[napi]
impl BollingerBandwidthNode {
    #[napi(constructor)]
    pub fn new(period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::BollingerBandwidth::new(period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Percent B ==============================

#[napi(js_name = "PercentB")]
pub struct PercentBNode {
    inner: wc::PercentB,
}

#[napi]
impl PercentBNode {
    #[napi(constructor)]
    pub fn new(period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::PercentB::new(period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== NATR ==============================

#[napi(js_name = "NATR")]
pub struct NatrNode {
    inner: wc::Natr,
}

#[napi]
impl NatrNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Natr::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Historical Volatility ==============================

#[napi(js_name = "HistoricalVolatility")]
pub struct HistoricalVolatilityNode {
    inner: wc::HistoricalVolatility,
}

#[napi]
impl HistoricalVolatilityNode {
    #[napi(constructor)]
    pub fn new(period: Count, trading_periods: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::HistoricalVolatility::new(period.get(), trading_periods.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Aroon Oscillator ==============================

#[napi(js_name = "AroonOscillator")]
pub struct AroonOscillatorNode {
    inner: wc::AroonOscillator,
}

#[napi]
impl AroonOscillatorNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AroonOscillator::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, low, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], low[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Vortex ==============================

/// Vortex Indicator pair: `VI+` and `VI-`.
#[napi(object)]
pub struct VortexValue {
    pub plus: f64,
    pub minus: f64,
}

/// Random Walk Index pair: `RWI_High` and `RWI_Low`.
#[napi(object)]
pub struct RwiValue {
    pub high: f64,
    pub low: f64,
}

/// Wave Trend Oscillator pair: `wt1` (channel index) and `wt2` (signal SMA).
#[napi(object)]
pub struct WaveTrendValue {
    pub wt1: f64,
    pub wt2: f64,
}

#[napi(js_name = "WaveTrend")]
pub struct WaveTrendNode {
    inner: wc::WaveTrend,
}

#[napi]
impl WaveTrendNode {
    #[napi(constructor)]
    pub fn new(
        channel_period: Count,
        average_period: Count,
        signal_period: Count,
    ) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::WaveTrend::new(
                channel_period.get(),
                average_period.get(),
                signal_period.get(),
            )
            .map_err(map_err)?,
        })
    }
    #[napi(factory)]
    pub fn classic() -> napi::Result<Self> {
        Ok(Self {
            inner: wc::WaveTrend::classic().map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<WaveTrendValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| WaveTrendValue {
                wt1: o.wt1,
                wt2: o.wt2,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.wt1;
                out[i * 2 + 1] = o.wt2;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "RWI")]
pub struct RwiNode {
    inner: wc::Rwi,
}

#[napi]
impl RwiNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Rwi::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<RwiValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| RwiValue {
                high: o.high,
                low: o.low,
            }))
    }
    /// Returns `[high0, low0, high1, low1, ...]`, length `2 * n`. Warmup is NaN.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.high;
                out[i * 2 + 1] = o.low;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "Vortex")]
pub struct VortexNode {
    inner: wc::Vortex,
}

#[napi]
impl VortexNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Vortex::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<VortexValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| VortexValue {
                plus: o.plus,
                minus: o.minus,
            }))
    }
    /// Returns `[plus0, minus0, plus1, minus1, ...]`, length `2 * n`. Warmup is NaN.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.plus;
                out[i * 2 + 1] = o.minus;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Mass Index ==============================

#[napi(js_name = "MassIndex")]
pub struct MassIndexNode {
    inner: wc::MassIndex,
}

#[napi]
impl MassIndexNode {
    #[napi(constructor)]
    pub fn new(ema_period: Count, sum_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::MassIndex::new(ema_period.get(), sum_period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, low, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], low[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== StochRSI ==============================

#[napi(js_name = "StochRSI")]
pub struct StochRsiNode {
    inner: wc::StochRsi,
}

#[napi]
impl StochRsiNode {
    #[napi(constructor)]
    pub fn new(rsi_period: Count, stoch_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::StochRsi::new(rsi_period.get(), stoch_period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Ultimate Oscillator ==============================

#[napi(js_name = "UltimateOscillator")]
pub struct UltimateOscillatorNode {
    inner: wc::UltimateOscillator,
}

#[napi]
impl UltimateOscillatorNode {
    #[napi(constructor)]
    pub fn new(short: Count, mid: Count, long: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::UltimateOscillator::new(short.get(), mid.get(), long.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== PPO ==============================

#[napi(js_name = "PPO")]
pub struct PpoNode {
    inner: wc::Ppo,
}

#[napi]
impl PpoNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Ppo::new(fast.get(), slow.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Coppock ==============================

#[napi(js_name = "Coppock")]
pub struct CoppockNode {
    inner: wc::Coppock,
}

#[napi]
impl CoppockNode {
    #[napi(constructor)]
    pub fn new(roc_long: Count, roc_short: Count, wma_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Coppock::new(roc_long.get(), roc_short.get(), wma_period.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "VWMA")]
pub struct VwmaNode {
    inner: wc::Vwma,
}

#[napi]
impl VwmaNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Vwma::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, close: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(close, close, close, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "close and volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(close[i], close[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}
// ============================== Family 05: Bands & Channels ==============================

// ---------- MA Envelope ----------

#[napi(object)]
pub struct MaEnvelopeValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "MaEnvelope")]
pub struct MaEnvelopeNode {
    inner: wc::MaEnvelope,
}

#[napi]
impl MaEnvelopeNode {
    #[napi(constructor)]
    pub fn new(period: Count, percent: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::MaEnvelope::new(period.get(), percent).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<MaEnvelopeValue> {
        self.inner.update(value).map(|o| MaEnvelopeValue {
            upper: o.upper,
            middle: o.middle,
            lower: o.lower,
        })
    }
    /// Flat `[upper0, middle0, lower0, upper1, ...]`, length `3 * n`.
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![f64::NAN; prices.len() * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Acceleration Bands ----------

#[napi(object)]
pub struct AccelerationBandsValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "AccelerationBands")]
pub struct AccelerationBandsNode {
    inner: wc::AccelerationBands,
}

#[napi]
impl AccelerationBandsNode {
    #[napi(constructor)]
    pub fn new(period: Count, factor: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AccelerationBands::new(period.get(), factor).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<AccelerationBandsValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| AccelerationBandsValue {
                upper: o.upper,
                middle: o.middle,
                lower: o.lower,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- STARC Bands ----------

#[napi(object)]
pub struct StarcBandsValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "StarcBands")]
pub struct StarcBandsNode {
    inner: wc::StarcBands,
}

#[napi]
impl StarcBandsNode {
    #[napi(constructor)]
    pub fn new(sma_period: Count, atr_period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::StarcBands::new(sma_period.get(), atr_period.get(), multiplier)
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<StarcBandsValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| StarcBandsValue {
                upper: o.upper,
                middle: o.middle,
                lower: o.lower,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- ATR Bands ----------

#[napi(object)]
pub struct AtrBandsValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "AtrBands")]
pub struct AtrBandsNode {
    inner: wc::AtrBands,
}

#[napi]
impl AtrBandsNode {
    #[napi(constructor)]
    pub fn new(period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AtrBands::new(period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<AtrBandsValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| AtrBandsValue {
                upper: o.upper,
                middle: o.middle,
                lower: o.lower,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Hurst Channel ----------

#[napi(object)]
pub struct HurstChannelValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "HurstChannel")]
pub struct HurstChannelNode {
    inner: wc::HurstChannel,
}

#[napi]
impl HurstChannelNode {
    #[napi(constructor)]
    pub fn new(period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::HurstChannel::new(period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<HurstChannelValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| HurstChannelValue {
                upper: o.upper,
                middle: o.middle,
                lower: o.lower,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- LinReg Channel ----------

#[napi(object)]
pub struct LinRegChannelValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "LinRegChannel")]
pub struct LinRegChannelNode {
    inner: wc::LinRegChannel,
}

#[napi]
impl LinRegChannelNode {
    #[napi(constructor)]
    pub fn new(period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::LinRegChannel::new(period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<LinRegChannelValue> {
        self.inner.update(value).map(|o| LinRegChannelValue {
            upper: o.upper,
            middle: o.middle,
            lower: o.lower,
        })
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![f64::NAN; prices.len() * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Standard Error Bands ----------

#[napi(object)]
pub struct StandardErrorBandsValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "StandardErrorBands")]
pub struct StandardErrorBandsNode {
    inner: wc::StandardErrorBands,
}

#[napi]
impl StandardErrorBandsNode {
    #[napi(constructor)]
    pub fn new(period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::StandardErrorBands::new(period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<StandardErrorBandsValue> {
        self.inner.update(value).map(|o| StandardErrorBandsValue {
            upper: o.upper,
            middle: o.middle,
            lower: o.lower,
        })
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![f64::NAN; prices.len() * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Quartile Bands ----------

#[napi(object)]
pub struct QuartileBandsValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "QuartileBands")]
pub struct QuartileBandsNode {
    inner: wc::QuartileBands,
}

#[napi]
impl QuartileBandsNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::QuartileBands::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<QuartileBandsValue> {
        self.inner.update(value).map(|o| QuartileBandsValue {
            upper: o.upper,
            middle: o.middle,
            lower: o.lower,
        })
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![f64::NAN; prices.len() * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Bomar Bands ----------

#[napi(object)]
pub struct BomarBandsValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "BomarBands")]
pub struct BomarBandsNode {
    inner: wc::BomarBands,
}

#[napi]
impl BomarBandsNode {
    #[napi(constructor)]
    pub fn new(period: Count, coverage: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::BomarBands::new(period.get(), coverage).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<BomarBandsValue> {
        self.inner.update(value).map(|o| BomarBandsValue {
            upper: o.upper,
            middle: o.middle,
            lower: o.lower,
        })
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![f64::NAN; prices.len() * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Median Channel ----------

#[napi(object)]
pub struct MedianChannelValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "MedianChannel")]
pub struct MedianChannelNode {
    inner: wc::MedianChannel,
}

#[napi]
impl MedianChannelNode {
    #[napi(constructor)]
    pub fn new(period: Count, multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::MedianChannel::new(period.get(), multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<MedianChannelValue> {
        self.inner.update(value).map(|o| MedianChannelValue {
            upper: o.upper,
            middle: o.middle,
            lower: o.lower,
        })
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![f64::NAN; prices.len() * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Projection Bands ----------

#[napi(object)]
pub struct ProjectionBandsValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[napi(js_name = "ProjectionBands")]
pub struct ProjectionBandsNode {
    inner: wc::ProjectionBands,
}

#[napi]
impl ProjectionBandsNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ProjectionBands::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<ProjectionBandsValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, low, 0.0)?)
            .map(|o| ProjectionBandsValue {
                upper: o.upper,
                middle: o.middle,
                lower: o.lower,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], low[i], 0.0)?) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Central Pivot Range ----------

#[napi(object)]
pub struct CentralPivotRangeValue {
    pub pivot: f64,
    pub tc: f64,
    pub bc: f64,
}

#[napi(js_name = "CentralPivotRange")]
pub struct CentralPivotRangeNode {
    inner: wc::CentralPivotRange,
}

impl Default for CentralPivotRangeNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl CentralPivotRangeNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::CentralPivotRange::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<CentralPivotRangeValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| CentralPivotRangeValue {
                pivot: o.pivot,
                tc: o.tc,
                bc: o.bc,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 3] = o.pivot;
                out[i * 3 + 1] = o.tc;
                out[i * 3 + 2] = o.bc;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Murrey Math Lines ----------

#[napi(object)]
pub struct MurreyMathLinesValue {
    #[napi(js_name = "mm8_8")]
    pub mm8_8: f64,
    #[napi(js_name = "mm7_8")]
    pub mm7_8: f64,
    #[napi(js_name = "mm6_8")]
    pub mm6_8: f64,
    #[napi(js_name = "mm5_8")]
    pub mm5_8: f64,
    #[napi(js_name = "mm4_8")]
    pub mm4_8: f64,
    #[napi(js_name = "mm3_8")]
    pub mm3_8: f64,
    #[napi(js_name = "mm2_8")]
    pub mm2_8: f64,
    #[napi(js_name = "mm1_8")]
    pub mm1_8: f64,
    #[napi(js_name = "mm0_8")]
    pub mm0_8: f64,
}

#[napi(js_name = "MurreyMathLines")]
pub struct MurreyMathLinesNode {
    inner: wc::MurreyMathLines,
}

#[napi]
impl MurreyMathLinesNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::MurreyMathLines::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<MurreyMathLinesValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, low, 0.0)?)
            .map(|o| MurreyMathLinesValue {
                mm8_8: o.mm8_8,
                mm7_8: o.mm7_8,
                mm6_8: o.mm6_8,
                mm5_8: o.mm5_8,
                mm4_8: o.mm4_8,
                mm3_8: o.mm3_8,
                mm2_8: o.mm2_8,
                mm1_8: o.mm1_8,
                mm0_8: o.mm0_8,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 9];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], low[i], 0.0)?) {
                out[i * 9] = o.mm8_8;
                out[i * 9 + 1] = o.mm7_8;
                out[i * 9 + 2] = o.mm6_8;
                out[i * 9 + 3] = o.mm5_8;
                out[i * 9 + 4] = o.mm4_8;
                out[i * 9 + 5] = o.mm3_8;
                out[i * 9 + 6] = o.mm2_8;
                out[i * 9 + 7] = o.mm1_8;
                out[i * 9 + 8] = o.mm0_8;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Andrews Pitchfork ----------

#[napi(object)]
pub struct AndrewsPitchforkValue {
    pub median: f64,
    pub upper: f64,
    pub lower: f64,
}

#[napi(js_name = "AndrewsPitchfork")]
pub struct AndrewsPitchforkNode {
    inner: wc::AndrewsPitchfork,
}

#[napi]
impl AndrewsPitchforkNode {
    #[napi(constructor)]
    pub fn new(strength: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AndrewsPitchfork::new(strength.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<AndrewsPitchforkValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, low, 0.0)?)
            .map(|o| AndrewsPitchforkValue {
                median: o.median,
                upper: o.upper,
                lower: o.lower,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], low[i], 0.0)?) {
                out[i * 3] = o.median;
                out[i * 3 + 1] = o.upper;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Volume-Weighted S/R ----------

#[napi(object)]
pub struct VolumeWeightedSrValue {
    pub support: f64,
    pub resistance: f64,
}

#[napi(js_name = "VolumeWeightedSr")]
pub struct VolumeWeightedSrNode {
    inner: wc::VolumeWeightedSr,
}

#[napi]
impl VolumeWeightedSrNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::VolumeWeightedSr::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        volume: f64,
    ) -> napi::Result<Option<VolumeWeightedSrValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, low, volume)?)
            .map(|o| VolumeWeightedSrValue {
                support: o.support,
                resistance: o.resistance,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, volume must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], low[i], volume[i])?) {
                out[i * 2] = o.support;
                out[i * 2 + 1] = o.resistance;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Pivot Reversal ----------

#[napi(js_name = "PivotReversal")]
pub struct PivotReversalNode {
    inner: wc::PivotReversal,
}

#[napi]
impl PivotReversalNode {
    #[napi(constructor)]
    pub fn new(left: Count, right: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::PivotReversal::new(left.get(), right.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Double Bollinger ----------

#[napi(object)]
pub struct DoubleBollingerValue {
    #[napi(js_name = "upperOuter")]
    pub upper_outer: f64,
    #[napi(js_name = "upperInner")]
    pub upper_inner: f64,
    pub middle: f64,
    #[napi(js_name = "lowerInner")]
    pub lower_inner: f64,
    #[napi(js_name = "lowerOuter")]
    pub lower_outer: f64,
}

#[napi(js_name = "DoubleBollinger")]
pub struct DoubleBollingerNode {
    inner: wc::DoubleBollinger,
}

#[napi]
impl DoubleBollingerNode {
    #[napi(constructor)]
    pub fn new(period: Count, k_inner: f64, k_outer: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::DoubleBollinger::new(period.get(), k_inner, k_outer).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<DoubleBollingerValue> {
        self.inner.update(value).map(|o| DoubleBollingerValue {
            upper_outer: o.upper_outer,
            upper_inner: o.upper_inner,
            middle: o.middle,
            lower_inner: o.lower_inner,
            lower_outer: o.lower_outer,
        })
    }
    /// Flat `[u_o, u_i, m, l_i, l_o, ...]`, length `5 * n`.
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![f64::NAN; prices.len() * 5];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 5] = o.upper_outer;
                out[i * 5 + 1] = o.upper_inner;
                out[i * 5 + 2] = o.middle;
                out[i * 5 + 3] = o.lower_inner;
                out[i * 5 + 4] = o.lower_outer;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- TTM Squeeze ----------

#[napi(object)]
pub struct TtmSqueezeValue {
    pub squeeze: f64,
    pub momentum: f64,
}

#[napi(js_name = "TtmSqueeze")]
pub struct TtmSqueezeNode {
    inner: wc::TtmSqueeze,
}

#[napi]
impl TtmSqueezeNode {
    #[napi(constructor)]
    pub fn new(period: Count, bb_mult: f64, kc_mult: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TtmSqueeze::new(period.get(), bb_mult, kc_mult).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<TtmSqueezeValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| TtmSqueezeValue {
                squeeze: o.squeeze,
                momentum: o.momentum,
            }))
    }
    /// Flat `[sq0, mom0, sq1, mom1, ...]`, length `2 * n`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.squeeze;
                out[i * 2 + 1] = o.momentum;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Fractal Chaos Bands ----------

#[napi(object)]
pub struct FractalChaosBandsValue {
    pub upper: f64,
    pub lower: f64,
}

#[napi(js_name = "FractalChaosBands")]
pub struct FractalChaosBandsNode {
    inner: wc::FractalChaosBands,
}

#[napi]
impl FractalChaosBandsNode {
    #[napi(constructor)]
    pub fn new(k: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::FractalChaosBands::new(k.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<FractalChaosBandsValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, low, 0.0)?)
            .map(|o| FractalChaosBandsValue {
                upper: o.upper,
                lower: o.lower,
            }))
    }
    /// Flat `[u0, l0, u1, l1, ...]`, length `2 * n`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], low[i], 0.0)?) {
                out[i * 2] = o.upper;
                out[i * 2 + 1] = o.lower;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- VWAP StdDev Bands ----------

#[napi(object)]
pub struct VwapStdDevBandsValue {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
    pub stddev: f64,
}

#[napi(js_name = "VwapStdDevBands")]
pub struct VwapStdDevBandsNode {
    inner: wc::VwapStdDevBands,
}

#[napi]
impl VwapStdDevBandsNode {
    #[napi(constructor)]
    pub fn new(multiplier: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::VwapStdDevBands::new(multiplier).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<VwapStdDevBandsValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, volume)?)
            .map(|o| VwapStdDevBandsValue {
                upper: o.upper,
                middle: o.middle,
                lower: o.lower,
                stddev: o.stddev,
            }))
    }
    /// Flat `[u0, m0, l0, sd0, ...]`, length `4 * n`.
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        let n = high.len();
        if low.len() != n || close.len() != n || volume.len() != n {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            if let Some(o) = self
                .inner
                .update(cnd(high[i], low[i], close[i], volume[i])?)
            {
                out[i * 4] = o.upper;
                out[i * 4 + 1] = o.middle;
                out[i * 4 + 2] = o.lower;
                out[i * 4 + 3] = o.stddev;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}
// ============================== Pivots & S/R ==============================

#[napi(object)]
pub struct ClassicPivotsValue {
    pub pp: f64,
    pub r1: f64,
    pub r2: f64,
    pub r3: f64,
    pub s1: f64,
    pub s2: f64,
    pub s3: f64,
}

#[napi(js_name = "ClassicPivots")]
pub struct ClassicPivotsNode {
    inner: wc::ClassicPivots,
}

#[napi]
impl ClassicPivotsNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::ClassicPivots::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<ClassicPivotsValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| ClassicPivotsValue {
                pp: o.pp,
                r1: o.r1,
                r2: o.r2,
                r3: o.r3,
                s1: o.s1,
                s2: o.s2,
                s3: o.s3,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 7];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 7] = o.pp;
                out[i * 7 + 1] = o.r1;
                out[i * 7 + 2] = o.r2;
                out[i * 7 + 3] = o.r3;
                out[i * 7 + 4] = o.s1;
                out[i * 7 + 5] = o.s2;
                out[i * 7 + 6] = o.s3;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for ClassicPivotsNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi(object)]
pub struct FibonacciPivotsValue {
    pub pp: f64,
    pub r1: f64,
    pub r2: f64,
    pub r3: f64,
    pub s1: f64,
    pub s2: f64,
    pub s3: f64,
}

#[napi(js_name = "FibonacciPivots")]
pub struct FibonacciPivotsNode {
    inner: wc::FibonacciPivots,
}

#[napi]
impl FibonacciPivotsNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::FibonacciPivots::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<FibonacciPivotsValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| FibonacciPivotsValue {
                pp: o.pp,
                r1: o.r1,
                r2: o.r2,
                r3: o.r3,
                s1: o.s1,
                s2: o.s2,
                s3: o.s3,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 7];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 7] = o.pp;
                out[i * 7 + 1] = o.r1;
                out[i * 7 + 2] = o.r2;
                out[i * 7 + 3] = o.r3;
                out[i * 7 + 4] = o.s1;
                out[i * 7 + 5] = o.s2;
                out[i * 7 + 6] = o.s3;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for FibonacciPivotsNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi(object)]
pub struct CamarillaValue {
    pub pp: f64,
    pub r1: f64,
    pub r2: f64,
    pub r3: f64,
    pub r4: f64,
    pub s1: f64,
    pub s2: f64,
    pub s3: f64,
    pub s4: f64,
}

#[napi(js_name = "Camarilla")]
pub struct CamarillaNode {
    inner: wc::Camarilla,
}

#[napi]
impl CamarillaNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::Camarilla::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<CamarillaValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| CamarillaValue {
                pp: o.pp,
                r1: o.r1,
                r2: o.r2,
                r3: o.r3,
                r4: o.r4,
                s1: o.s1,
                s2: o.s2,
                s3: o.s3,
                s4: o.s4,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 9];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 9] = o.pp;
                out[i * 9 + 1] = o.r1;
                out[i * 9 + 2] = o.r2;
                out[i * 9 + 3] = o.r3;
                out[i * 9 + 4] = o.r4;
                out[i * 9 + 5] = o.s1;
                out[i * 9 + 6] = o.s2;
                out[i * 9 + 7] = o.s3;
                out[i * 9 + 8] = o.s4;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for CamarillaNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi(object)]
pub struct WoodiePivotsValue {
    pub pp: f64,
    pub r1: f64,
    pub r2: f64,
    pub s1: f64,
    pub s2: f64,
}

#[napi(js_name = "WoodiePivots")]
pub struct WoodiePivotsNode {
    inner: wc::WoodiePivots,
}

#[napi]
impl WoodiePivotsNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::WoodiePivots::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<WoodiePivotsValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| WoodiePivotsValue {
                pp: o.pp,
                r1: o.r1,
                r2: o.r2,
                s1: o.s1,
                s2: o.s2,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 5];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 5] = o.pp;
                out[i * 5 + 1] = o.r1;
                out[i * 5 + 2] = o.r2;
                out[i * 5 + 3] = o.s1;
                out[i * 5 + 4] = o.s2;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for WoodiePivotsNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi(object)]
pub struct DemarkPivotsValue {
    pub pp: f64,
    pub r1: f64,
    pub s1: f64,
}

#[napi(js_name = "DemarkPivots")]
pub struct DemarkPivotsNode {
    inner: wc::DemarkPivots,
}

#[napi]
impl DemarkPivotsNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::DemarkPivots::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<DemarkPivotsValue>> {
        let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle).map(|o| DemarkPivotsValue {
            pp: o.pp,
            r1: o.r1,
            s1: o.s1,
        }))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let n = open.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.pp;
                out[i * 3 + 1] = o.r1;
                out[i * 3 + 2] = o.s1;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for DemarkPivotsNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi(object)]
pub struct WilliamsFractalsValue {
    /// Up fractal price; NaN when no up fractal was confirmed on this bar.
    pub up: f64,
    /// Down fractal price; NaN when no down fractal was confirmed on this bar.
    pub down: f64,
}

#[napi(js_name = "WilliamsFractals")]
pub struct WilliamsFractalsNode {
    inner: wc::WilliamsFractals,
}

#[napi]
impl WilliamsFractalsNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::WilliamsFractals::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<WilliamsFractalsValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, low, 0.0)?)
            .map(|o| WilliamsFractalsValue {
                up: o.up.unwrap_or(f64::NAN),
                down: o.down.unwrap_or(f64::NAN),
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], low[i], 0.0)?) {
                if let Some(v) = o.up {
                    out[i * 2] = v;
                }
                if let Some(v) = o.down {
                    out[i * 2 + 1] = v;
                }
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for WilliamsFractalsNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi(object)]
pub struct ZigZagValue {
    pub swing: f64,
    pub direction: f64,
}

#[napi(js_name = "ZigZag")]
pub struct ZigZagNode {
    inner: wc::ZigZag,
}

#[napi]
impl ZigZagNode {
    #[napi(constructor)]
    pub fn new(threshold: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ZigZag::new(threshold).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<ZigZagValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, low, 0.0)?)
            .map(|o| ZigZagValue {
                swing: o.swing,
                direction: o.direction,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], low[i], 0.0)?) {
                out[i * 2] = o.swing;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(out)
    }
    #[napi(getter)]
    pub fn threshold(&self) -> f64 {
        self.inner.threshold()
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}
// ============================== TD Setup ==============================

#[napi(js_name = "TDSetup")]
pub struct TdSetupNode {
    inner: wc::TdSetup,
}

#[napi]
impl TdSetupNode {
    #[napi(constructor)]
    pub fn new(lookback: Count, target: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TdSetup::new(lookback.get(), target.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD Sequential ==============================

/// TD Sequential output triple: setup count, countdown count, direction.
#[napi(object)]
pub struct TdSequentialValue {
    pub setup: f64,
    pub countdown: f64,
    pub direction: f64,
}

#[napi(js_name = "TDSequential")]
pub struct TdSequentialNode {
    inner: wc::TdSequential,
}

#[napi]
impl TdSequentialNode {
    #[napi(constructor)]
    pub fn new(
        setup_lookback: Count,
        setup_target: Count,
        countdown_lookback: Count,
        countdown_target: Count,
    ) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TdSequential::new(
                setup_lookback.get(),
                setup_target.get(),
                countdown_lookback.get(),
                countdown_target.get(),
            )
            .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<TdSequentialValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| TdSequentialValue {
                setup: o.setup,
                countdown: o.countdown,
                direction: o.direction,
            }))
    }
    /// Batch returns a flat array `[setup0, countdown0, direction0, setup1, ...]`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 3] = o.setup;
                out[i * 3 + 1] = o.countdown;
                out[i * 3 + 2] = o.direction;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD DeMarker ==============================

#[napi(js_name = "TDDeMarker")]
pub struct TdDeMarkerNode {
    inner: wc::TdDeMarker,
}

#[napi]
impl TdDeMarkerNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TdDeMarker::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, low, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], low[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD REI ==============================

#[napi(js_name = "TDREI")]
pub struct TdReiNode {
    inner: wc::TdRei,
}

#[napi]
impl TdReiNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TdRei::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, low, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], low[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD Pressure ==============================

#[napi(js_name = "TDPressure")]
pub struct TdPressureNode {
    inner: wc::TdPressure,
}

#[napi]
impl TdPressureNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TdPressure::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        let candle = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        Ok(self.inner.update(candle))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len()
            || high.len() != low.len()
            || low.len() != close.len()
            || close.len() != volume.len()
        {
            return Err(NapiError::from_reason(
                "open, high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let candle = wc::Candle::new(open[i], high[i], low[i], close[i], volume[i], 0)
                .map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD Combo ==============================

#[napi(js_name = "TDCombo")]
pub struct TdComboNode {
    inner: wc::TdCombo,
}

#[napi]
impl TdComboNode {
    #[napi(constructor)]
    pub fn new(
        setup_lookback: Count,
        setup_target: Count,
        countdown_lookback: Count,
        countdown_target: Count,
    ) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TdCombo::new(
                setup_lookback.get(),
                setup_target.get(),
                countdown_lookback.get(),
                countdown_target.get(),
            )
            .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD D-Wave ==============================

#[napi(js_name = "TDDWave")]
pub struct TdDWaveNode {
    inner: wc::TdDWave,
}

#[napi]
impl TdDWaveNode {
    #[napi(constructor)]
    pub fn new(strength: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TdDWave::new(strength.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD Moving Averages ==============================

#[napi(object)]
pub struct TdMovingAverageValue {
    pub st1: f64,
    pub st2: f64,
}

#[napi(js_name = "TDMovingAverage")]
pub struct TdMovingAverageNode {
    inner: wc::TdMovingAverage,
}

#[napi]
impl TdMovingAverageNode {
    #[napi(constructor)]
    pub fn new(period_st1: Count, period_st2: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TdMovingAverage::new(period_st1.get(), period_st2.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<TdMovingAverageValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, low, 0.0)?)
            .map(|o| TdMovingAverageValue {
                st1: o.st1,
                st2: o.st2,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high, low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], low[i], 0.0)?) {
                out[i * 2] = o.st1;
                out[i * 2 + 1] = o.st2;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD Countdown ==============================

#[napi(js_name = "TDCountdown")]
pub struct TdCountdownNode {
    inner: wc::TdCountdown,
}

#[napi]
impl TdCountdownNode {
    #[napi(constructor)]
    pub fn new(
        setup_lookback: Count,
        setup_target: Count,
        countdown_lookback: Count,
        countdown_target: Count,
    ) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TdCountdown::new(
                setup_lookback.get(),
                setup_target.get(),
                countdown_lookback.get(),
                countdown_target.get(),
            )
            .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD Lines ==============================

/// TD Lines output pair: latest TDST resistance / support (NaN if unset).
#[napi(object)]
pub struct TdLinesValue {
    pub resistance: f64,
    pub support: f64,
}

#[napi(js_name = "TDLines")]
pub struct TdLinesNode {
    inner: wc::TdLines,
}

#[napi]
impl TdLinesNode {
    #[napi(constructor)]
    pub fn new(lookback: Count, target: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TdLines::new(lookback.get(), target.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<TdLinesValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| TdLinesValue {
                resistance: o.resistance,
                support: o.support,
            }))
    }
    /// Batch returns a flat array `[resistance0, support0, resistance1, ...]`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.resistance;
                out[i * 2 + 1] = o.support;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD Range Projection ==============================

/// TD Range Projection output pair: projected next-bar high / low.
#[napi(object)]
pub struct TdRangeProjectionValue {
    pub high: f64,
    pub low: f64,
}

#[napi(js_name = "TDRangeProjection")]
pub struct TdRangeProjectionNode {
    inner: wc::TdRangeProjection,
}

impl Default for TdRangeProjectionNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl TdRangeProjectionNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::TdRangeProjection::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<TdRangeProjectionValue>> {
        let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle).map(|o| TdRangeProjectionValue {
            high: o.high,
            low: o.low,
        }))
    }
    /// Batch returns a flat array `[high0, low0, high1, low1, ...]`.
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let n = open.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            if let Some(p) = self.inner.update(candle) {
                out[i * 2] = p.high;
                out[i * 2 + 1] = p.low;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD Differential ==============================

#[napi(js_name = "TDDifferential")]
pub struct TdDifferentialNode {
    inner: wc::TdDifferential,
}

impl Default for TdDifferentialNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl TdDifferentialNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::TdDifferential::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD Open ==============================

#[napi(js_name = "TDOpen")]
pub struct TdOpenNode {
    inner: wc::TdOpen,
}

impl Default for TdOpenNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl TdOpenNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::TdOpen::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== TD Risk Level ==============================

/// TD Risk Level output pair: buy-side / sell-side protective stop levels
/// (NaN if unset).
#[napi(object)]
pub struct TdRiskLevelValue {
    pub buy_risk: f64,
    pub sell_risk: f64,
}

#[napi(js_name = "TDRiskLevel")]
pub struct TdRiskLevelNode {
    inner: wc::TdRiskLevel,
}

#[napi]
impl TdRiskLevelNode {
    #[napi(constructor)]
    pub fn new(lookback: Count, target: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TdRiskLevel::new(lookback.get(), target.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<TdRiskLevelValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| TdRiskLevelValue {
                buy_risk: o.buy_risk,
                sell_risk: o.sell_risk,
            }))
    }
    /// Batch returns a flat array `[buyRisk0, sellRisk0, buyRisk1, ...]`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                out[i * 2] = o.buy_risk;
                out[i * 2 + 1] = o.sell_risk;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Family 10 — Ehlers / Cycle ==============================

#[napi(js_name = "InverseFisherTransform")]
pub struct InverseFisherTransformNode {
    inner: wc::InverseFisherTransform,
}

#[napi]
impl InverseFisherTransformNode {
    #[napi(constructor)]
    pub fn new(scale: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::InverseFisherTransform::new(scale).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "DecyclerOscillator")]
pub struct DecyclerOscillatorNode {
    inner: wc::DecyclerOscillator,
}

#[napi]
impl DecyclerOscillatorNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::DecyclerOscillator::new(fast.get(), slow.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "RoofingFilter")]
pub struct RoofingFilterNode {
    inner: wc::RoofingFilter,
}

#[napi]
impl RoofingFilterNode {
    #[napi(constructor)]
    pub fn new(lp_period: Count, hp_period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RoofingFilter::new(lp_period.get(), hp_period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "EmpiricalModeDecomposition")]
pub struct EmpiricalModeDecompositionNode {
    inner: wc::EmpiricalModeDecomposition,
}

#[napi]
impl EmpiricalModeDecompositionNode {
    #[napi(constructor)]
    pub fn new(period: Count, fraction: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::EmpiricalModeDecomposition::new(period.get(), fraction).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "HT_DCPHASE")]
pub struct HtDcPhaseNode {
    inner: wc::HtDcPhase,
}

impl Default for HtDcPhaseNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl HtDcPhaseNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::HtDcPhase::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "HT_TRENDMODE")]
pub struct HtTrendModeNode {
    inner: wc::HtTrendMode,
}

impl Default for HtTrendModeNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl HtTrendModeNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::HtTrendMode::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "HilbertDominantCycle")]
pub struct HilbertDominantCycleNode {
    inner: wc::HilbertDominantCycle,
}

impl Default for HilbertDominantCycleNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl HilbertDominantCycleNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::HilbertDominantCycle::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "AdaptiveCycle")]
pub struct AdaptiveCycleNode {
    inner: wc::AdaptiveCycle,
}

impl Default for AdaptiveCycleNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl AdaptiveCycleNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::AdaptiveCycle::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "SineWave")]
pub struct SineWaveNode {
    inner: wc::SineWave,
}

impl Default for SineWaveNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl SineWaveNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::SineWave::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn lead(&self) -> f64 {
        self.inner.lead()
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct MamaValue {
    pub mama: f64,
    pub fama: f64,
}

#[napi(js_name = "MAMA")]
pub struct MamaNode {
    inner: wc::Mama,
}

#[napi]
impl MamaNode {
    #[napi(constructor)]
    pub fn new(fast_limit: f64, slow_limit: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Mama::new(fast_limit, slow_limit).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<MamaValue> {
        self.inner.update(value).map(|o| MamaValue {
            mama: o.mama,
            fama: o.fama,
        })
    }
    /// Returns a flat array of length `2 * n`: `[mama0, fama0, mama1, fama1, ...]`.
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        let mut out = vec![f64::NAN; prices.len() * 2];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 2] = o.mama;
                out[i * 2 + 1] = o.fama;
            }
        }
        out
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "FAMA")]
pub struct FamaNode {
    inner: wc::Fama,
}

#[napi]
impl FamaNode {
    #[napi(constructor)]
    pub fn new(fast_limit: f64, slow_limit: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Fama::new(fast_limit, slow_limit).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Ichimoku ==============================

/// Ichimoku output: five lines, any of which may be `NaN` while the indicator
/// is still warming up.
#[napi(object)]
pub struct IchimokuValue {
    pub tenkan: f64,
    pub kijun: f64,
    #[napi(js_name = "senkouA")]
    pub senkou_a: f64,
    #[napi(js_name = "senkouB")]
    pub senkou_b: f64,
    pub chikou: f64,
}

#[napi(js_name = "Ichimoku")]
pub struct IchimokuNode {
    inner: wc::Ichimoku,
}

#[napi]
impl IchimokuNode {
    #[napi(constructor)]
    pub fn new(
        tenkan_period: Count,
        kijun_period: Count,
        senkou_b_period: Count,
        displacement: Count,
    ) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Ichimoku::new(
                tenkan_period.get(),
                kijun_period.get(),
                senkou_b_period.get(),
                displacement.get(),
            )
            .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<IchimokuValue>> {
        Ok(self
            .inner
            .update(cnd(high, low, close, 0.0)?)
            .map(|o| IchimokuValue {
                tenkan: o.tenkan.unwrap_or(f64::NAN),
                kijun: o.kijun.unwrap_or(f64::NAN),
                senkou_a: o.senkou_a.unwrap_or(f64::NAN),
                senkou_b: o.senkou_b.unwrap_or(f64::NAN),
                chikou: o.chikou.unwrap_or(f64::NAN),
            }))
    }
    /// Returns `[tenkan0, kijun0, senkouA0, senkouB0, chikou0, tenkan1, ...]`,
    /// length `5 * n`. Cells without a defined value are `NaN`.
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 5];
        for i in 0..n {
            if let Some(o) = self.inner.update(cnd(high[i], low[i], close[i], 0.0)?) {
                if let Some(v) = o.tenkan {
                    out[i * 5] = v;
                }
                if let Some(v) = o.kijun {
                    out[i * 5 + 1] = v;
                }
                if let Some(v) = o.senkou_a {
                    out[i * 5 + 2] = v;
                }
                if let Some(v) = o.senkou_b {
                    out[i * 5 + 3] = v;
                }
                if let Some(v) = o.chikou {
                    out[i * 5 + 4] = v;
                }
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Heikin-Ashi ==============================

#[napi(object)]
pub struct HeikinAshiValue {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

#[napi(js_name = "HeikinAshi")]
pub struct HeikinAshiNode {
    inner: wc::HeikinAshi,
}

impl Default for HeikinAshiNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl HeikinAshiNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::HeikinAshi::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<HeikinAshiValue>> {
        let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(c).map(|o| HeikinAshiValue {
            open: o.open,
            high: o.high,
            low: o.low,
            close: o.close,
        }))
    }
    /// Returns `[open0, high0, low0, close0, open1, ...]`, length `4 * n`.
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let n = open.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 4] = o.open;
                out[i * 4 + 1] = o.high;
                out[i * 4 + 2] = o.low;
                out[i * 4 + 3] = o.close;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Heikin-Ashi Oscillator ==============================

#[napi(js_name = "HeikinAshiOscillator")]
pub struct HeikinAshiOscillatorNode {
    inner: wc::HeikinAshiOscillator,
}

#[napi]
impl HeikinAshiOscillatorNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::HeikinAshiOscillator::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(c))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Three Line Break ==============================

#[napi(js_name = "ThreeLineBreak")]
pub struct ThreeLineBreakNode {
    inner: wc::ThreeLineBreak,
}

#[napi]
impl ThreeLineBreakNode {
    #[napi(constructor)]
    pub fn new(lines: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ThreeLineBreak::new(lines.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Smoothed Heikin-Ashi ==============================

#[napi(object)]
pub struct SmoothedHeikinAshiValue {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

#[napi(js_name = "SmoothedHeikinAshi")]
pub struct SmoothedHeikinAshiNode {
    inner: wc::SmoothedHeikinAshi,
}

#[napi]
impl SmoothedHeikinAshiNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::SmoothedHeikinAshi::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<SmoothedHeikinAshiValue>> {
        let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(c).map(|o| SmoothedHeikinAshiValue {
            open: o.open,
            high: o.high,
            low: o.low,
            close: o.close,
        }))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let n = open.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 4] = o.open;
                out[i * 4 + 1] = o.high;
                out[i * 4 + 2] = o.low;
                out[i * 4 + 3] = o.close;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Equivolume ==============================

#[napi(object)]
pub struct EquivolumeValue {
    pub height: f64,
    pub width: f64,
}

#[napi(js_name = "Equivolume")]
pub struct EquivolumeNode {
    inner: wc::Equivolume,
}

#[napi]
impl EquivolumeNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Equivolume::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        volume: f64,
    ) -> napi::Result<Option<EquivolumeValue>> {
        let c = wc::Candle::new(low, high, low, low, volume, 0).map_err(map_err)?;
        Ok(self.inner.update(c).map(|o| EquivolumeValue {
            height: o.height,
            width: o.width,
        }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, volume must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c =
                wc::Candle::new(low[i], high[i], low[i], low[i], volume[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.height;
                out[i * 2 + 1] = o.width;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== CandleVolume ==============================

#[napi(object)]
pub struct CandleVolumeValue {
    pub body: f64,
    pub width: f64,
}

#[napi(js_name = "CandleVolume")]
pub struct CandleVolumeNode {
    inner: wc::CandleVolume,
}

#[napi]
impl CandleVolumeNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::CandleVolume::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<CandleVolumeValue>> {
        let high = open.max(close);
        let low = open.min(close);
        let c = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        Ok(self.inner.update(c).map(|o| CandleVolumeValue {
            body: o.body,
            width: o.width,
        }))
    }
    #[napi]
    pub fn batch(&mut self, open: Series, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if open.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "open, close, volume must be equal length".to_string(),
            ));
        }
        let n = open.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let high = open[i].max(close[i]);
            let low = open[i].min(close[i]);
            let c = wc::Candle::new(open[i], high, low, close[i], volume[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.body;
                out[i * 2 + 1] = o.width;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Frying Pan Bottom ==============================

#[napi(js_name = "FryPanBottom")]
pub struct FryPanBottomNode {
    inner: wc::FryPanBottom,
}

#[napi]
impl FryPanBottomNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::FryPanBottom::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Dumpling Top ==============================

#[napi(js_name = "DumplingTop")]
pub struct DumplingTopNode {
    inner: wc::DumplingTop,
}

#[napi]
impl DumplingTopNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::DumplingTop::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== New Price Lines ==============================

#[napi(js_name = "NewPriceLines")]
pub struct NewPriceLinesNode {
    inner: wc::NewPriceLines,
}

#[napi]
impl NewPriceLinesNode {
    #[napi(constructor)]
    pub fn new(count: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::NewPriceLines::new(count.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== ValueArea ==============================

#[napi(object)]
pub struct ValueAreaValue {
    pub poc: f64,
    pub vah: f64,
    pub val: f64,
}

#[napi(js_name = "ValueArea")]
pub struct ValueAreaNode {
    inner: wc::ValueArea,
}
#[napi]
impl ValueAreaNode {
    #[napi(constructor)]
    pub fn new(period: Count, bin_count: Count, value_area_pct: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ValueArea::new(period.get(), bin_count.get(), value_area_pct)
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        volume: f64,
    ) -> napi::Result<Option<ValueAreaValue>> {
        let mid = f64::midpoint(high, low);
        let candle = wc::Candle::new(mid, high, low, mid, volume, 0).map_err(map_err)?;
        Ok(self.inner.update(candle).map(|o| ValueAreaValue {
            poc: o.poc,
            vah: o.vah,
            val: o.val,
        }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, volume must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let mid = f64::midpoint(high[i], low[i]);
            let candle =
                wc::Candle::new(mid, high[i], low[i], mid, volume[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.poc;
                out[i * 3 + 1] = o.vah;
                out[i * 3 + 2] = o.val;
            }
        }
        Ok(out)
    }
}

// ============================== VolumeProfile ==============================

#[napi(object)]
pub struct VolumeProfileValue {
    pub price_low: f64,
    pub price_high: f64,
    pub bins: Vec<f64>,
}

// Naked POC: most recent untouched point-of-control level (Candle -> f64).
#[napi(js_name = "NakedPoc")]
pub struct NakedPocNode {
    inner: wc::NakedPoc,
}

#[napi]
impl NakedPocNode {
    #[napi(constructor)]
    pub fn new(session_len: Count, bin_count: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::NakedPoc::new(session_len.get(), bin_count.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// Single Prints: count of single-print price levels (Candle -> f64).
#[napi(js_name = "SinglePrints")]
pub struct SinglePrintsNode {
    inner: wc::SinglePrints,
}

#[napi]
impl SinglePrintsNode {
    #[napi(constructor)]
    pub fn new(period: Count, bin_count: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::SinglePrints::new(period.get(), bin_count.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<f64>> {
        let mid = f64::midpoint(high, low);
        Ok(self
            .inner
            .update(wc::Candle::new(mid, high, low, mid, 0.0, 0).map_err(map_err)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let mid = f64::midpoint(high[i], low[i]);
            out.push(
                self.inner
                    .update(wc::Candle::new(mid, high[i], low[i], mid, 0.0, 0).map_err(map_err)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// Profile Shape: b/P/D classification as a numeric code (Candle -> f64).
#[napi(js_name = "ProfileShape")]
pub struct ProfileShapeNode {
    inner: wc::ProfileShape,
}

#[napi]
impl ProfileShapeNode {
    #[napi(constructor)]
    pub fn new(period: Count, bin_count: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ProfileShape::new(period.get(), bin_count.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, volume: f64) -> napi::Result<Option<f64>> {
        let mid = f64::midpoint(high, low);
        Ok(self
            .inner
            .update(wc::Candle::new(mid, high, low, mid, volume, 0).map_err(map_err)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let mid = f64::midpoint(high[i], low[i]);
            out.push(
                self.inner
                    .update(
                        wc::Candle::new(mid, high[i], low[i], mid, volume[i], 0)
                            .map_err(map_err)?,
                    )
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct HighLowVolumeNodesValue {
    pub hvn: f64,
    pub lvn: f64,
}

// High/Low Volume Nodes: highest- and lowest-volume price nodes (Candle -> struct).
#[napi(js_name = "HighLowVolumeNodes")]
pub struct HighLowVolumeNodesNode {
    inner: wc::HighLowVolumeNodes,
}

#[napi]
impl HighLowVolumeNodesNode {
    #[napi(constructor)]
    pub fn new(period: Count, bin_count: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::HighLowVolumeNodes::new(period.get(), bin_count.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        volume: f64,
    ) -> napi::Result<Option<HighLowVolumeNodesValue>> {
        let mid = f64::midpoint(high, low);
        let candle = wc::Candle::new(mid, high, low, mid, volume, 0).map_err(map_err)?;
        Ok(self.inner.update(candle).map(|o| HighLowVolumeNodesValue {
            hvn: o.hvn,
            lvn: o.lvn,
        }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, volume must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let mid = f64::midpoint(high[i], low[i]);
            let candle =
                wc::Candle::new(mid, high[i], low[i], mid, volume[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.hvn;
                out[i * 2 + 1] = o.lvn;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct CompositeProfileValue {
    pub poc: f64,
    pub vah: f64,
    pub val: f64,
}

// Composite Profile: multi-session composite volume profile (Candle -> struct).
#[napi(js_name = "CompositeProfile")]
pub struct CompositeProfileNode {
    inner: wc::CompositeProfile,
}

#[napi]
impl CompositeProfileNode {
    #[napi(constructor)]
    pub fn new(period: Count, bin_count: Count, value_area_pct: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::CompositeProfile::new(period.get(), bin_count.get(), value_area_pct)
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        volume: f64,
    ) -> napi::Result<Option<CompositeProfileValue>> {
        let mid = f64::midpoint(high, low);
        let candle = wc::Candle::new(mid, high, low, mid, volume, 0).map_err(map_err)?;
        Ok(self.inner.update(candle).map(|o| CompositeProfileValue {
            poc: o.poc,
            vah: o.vah,
            val: o.val,
        }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, volume must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let mid = f64::midpoint(high[i], low[i]);
            let candle =
                wc::Candle::new(mid, high[i], low[i], mid, volume[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.poc;
                out[i * 3 + 1] = o.vah;
                out[i * 3 + 2] = o.val;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "VolumeProfile")]
pub struct VolumeProfileNode {
    inner: wc::VolumeProfile,
}
#[napi]
impl VolumeProfileNode {
    #[napi(constructor)]
    pub fn new(period: Count, bin_count: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::VolumeProfile::new(period.get(), bin_count.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        volume: f64,
    ) -> napi::Result<Option<VolumeProfileValue>> {
        let mid = f64::midpoint(high, low);
        let candle = wc::Candle::new(mid, high, low, mid, volume, 0).map_err(map_err)?;
        Ok(self.inner.update(candle).map(|o| VolumeProfileValue {
            price_low: o.price_low,
            price_high: o.price_high,
            bins: o.bins,
        }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, volume must be equal length".to_string(),
            ));
        }
        let k = self.inner.params().1 + 2;
        let n = high.len();
        let mut out = vec![f64::NAN; n * k];
        for i in 0..n {
            let mid = f64::midpoint(high[i], low[i]);
            let candle =
                wc::Candle::new(mid, high[i], low[i], mid, volume[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * k] = o.price_low;
                out[i * k + 1] = o.price_high;
                for (j, b) in o.bins.iter().enumerate() {
                    out[i * k + 2 + j] = *b;
                }
            }
        }
        Ok(out)
    }
}

// ============================== TpoProfile ==============================

#[napi(object)]
pub struct TpoProfileValue {
    pub price_low: f64,
    pub price_high: f64,
    pub counts: Vec<f64>,
}

#[napi(js_name = "TpoProfile")]
pub struct TpoProfileNode {
    inner: wc::TpoProfile,
}
#[napi]
impl TpoProfileNode {
    #[napi(constructor)]
    pub fn new(period: Count, bin_count: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TpoProfile::new(period.get(), bin_count.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<TpoProfileValue>> {
        let mid = f64::midpoint(high, low);
        let candle = wc::Candle::new(mid, high, low, mid, 1.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle).map(|o| TpoProfileValue {
            price_low: o.price_low,
            price_high: o.price_high,
            counts: o.counts,
        }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high, low must be equal length".to_string(),
            ));
        }
        let k = self.inner.params().1 + 2;
        let n = high.len();
        let mut out = vec![f64::NAN; n * k];
        for i in 0..n {
            let mid = f64::midpoint(high[i], low[i]);
            let candle = wc::Candle::new(mid, high[i], low[i], mid, 1.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * k] = o.price_low;
                out[i * k + 1] = o.price_high;
                for (j, count) in o.counts.iter().enumerate() {
                    out[i * k + 2 + j] = *count;
                }
            }
        }
        Ok(out)
    }
}

// ============================== InitialBalance ==============================

#[napi(object)]
pub struct InitialBalanceValue {
    pub high: f64,
    pub low: f64,
}

#[napi(js_name = "InitialBalance")]
pub struct InitialBalanceNode {
    inner: wc::InitialBalance,
}
#[napi]
impl InitialBalanceNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::InitialBalance::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "isLocked")]
    pub fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<InitialBalanceValue>> {
        let mid = f64::midpoint(high, low);
        let candle = wc::Candle::new(mid, high, low, mid, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle).map(|o| InitialBalanceValue {
            high: o.high,
            low: o.low,
        }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let mid = f64::midpoint(high[i], low[i]);
            let candle = wc::Candle::new(mid, high[i], low[i], mid, 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.high;
                out[i * 2 + 1] = o.low;
            }
        }
        Ok(out)
    }
}

// ============================== OpeningRange ==============================

#[napi(object)]
pub struct OpeningRangeValue {
    pub high: f64,
    pub low: f64,
    pub breakout_distance: f64,
}

#[napi(js_name = "OpeningRange")]
pub struct OpeningRangeNode {
    inner: wc::OpeningRange,
}
#[napi]
impl OpeningRangeNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::OpeningRange::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "isLocked")]
    pub fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<OpeningRangeValue>> {
        let candle = wc::Candle::new(close, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle).map(|o| OpeningRangeValue {
            high: o.high,
            low: o.low,
            breakout_distance: o.breakout_distance,
        }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle =
                wc::Candle::new(close[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.high;
                out[i * 3 + 1] = o.low;
                out[i * 3 + 2] = o.breakout_distance;
            }
        }
        Ok(out)
    }
}
// ============================== Candlestick Patterns ==============================
//
// All 15 patterns take Candles (open, high, low, close) and emit a signed f64
// signal per bar: +1.0 bullish, -1.0 bearish, 0.0 no pattern. Doji is
// direction-less by default (0/+1); pass `signed = true` to its constructor for
// the dragonfly/gravestone signed +-1 encoding.

macro_rules! node_candle_pattern {
    ($node:ident, $inner:ty, $js:literal) => {
        #[napi(js_name = $js)]
        pub struct $node {
            inner: $inner,
        }

        impl Default for $node {
            fn default() -> Self {
                Self::new()
            }
        }

        #[napi]
        impl $node {
            #[napi(constructor)]
            pub fn new() -> Self {
                Self {
                    inner: <$inner>::new(),
                }
            }
            #[napi]
            pub fn update(
                &mut self,
                open: f64,
                high: f64,
                low: f64,
                close: f64,
            ) -> napi::Result<Option<f64>> {
                let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
                Ok(self.inner.update(candle))
            }
            #[napi]
            pub fn batch(
                &mut self,
                open: Series,
                high: Series,
                low: Series,
                close: Series,
            ) -> napi::Result<Vec<f64>> {
                if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
                    return Err(NapiError::from_reason(
                        "open, high, low, close must be equal length".to_string(),
                    ));
                }
                let mut out = Vec::with_capacity(open.len());
                for i in 0..open.len() {
                    let candle = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0)
                        .map_err(map_err)?;
                    out.push(self.inner.update(candle).unwrap_or(f64::NAN));
                }
                Ok(out)
            }
            #[napi]
            pub fn reset(&mut self) {
                self.inner.reset();
            }

            #[napi]
            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[napi(js_name = "isReady")]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[napi(js_name = "warmupPeriod")]
            pub fn warmup_period(&self) -> u32 {
                self.inner.warmup_period() as u32
            }
        }
    };
}

// Doji is the one pattern with an opt-in signed mode, so it is hand-written
// rather than generated by `node_candle_pattern!`.
#[napi(js_name = "Doji")]
pub struct DojiNode {
    inner: wc::Doji,
}

impl Default for DojiNode {
    fn default() -> Self {
        Self::new(None)
    }
}

#[napi]
impl DojiNode {
    #[napi(constructor)]
    pub fn new(signed: Option<bool>) -> Self {
        let inner = if signed.unwrap_or(false) {
            wc::Doji::new().signed()
        } else {
            wc::Doji::new()
        };
        Self { inner }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Option<f64>> {
        let candle = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(candle))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<f64>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
    #[napi(js_name = "isSigned")]
    pub fn is_signed(&self) -> bool {
        self.inner.is_signed()
    }
}

node_candle_pattern!(HammerNode, wc::Hammer, "Hammer");
node_candle_pattern!(InvertedHammerNode, wc::InvertedHammer, "InvertedHammer");
node_candle_pattern!(HangingManNode, wc::HangingMan, "HangingMan");
node_candle_pattern!(ShootingStarNode, wc::ShootingStar, "ShootingStar");
node_candle_pattern!(EngulfingNode, wc::Engulfing, "Engulfing");
node_candle_pattern!(HaramiNode, wc::Harami, "Harami");
node_candle_pattern!(
    MorningEveningStarNode,
    wc::MorningEveningStar,
    "MorningEveningStar"
);
node_candle_pattern!(
    ThreeSoldiersOrCrowsNode,
    wc::ThreeSoldiersOrCrows,
    "ThreeSoldiersOrCrows"
);
node_candle_pattern!(
    PiercingDarkCloudNode,
    wc::PiercingDarkCloud,
    "PiercingDarkCloud"
);
node_candle_pattern!(MarubozuNode, wc::Marubozu, "Marubozu");
node_candle_pattern!(TweezerNode, wc::Tweezer, "Tweezer");
node_candle_pattern!(SpinningTopNode, wc::SpinningTop, "SpinningTop");
node_candle_pattern!(ThreeInsideNode, wc::ThreeInside, "ThreeInside");
node_candle_pattern!(ThreeOutsideNode, wc::ThreeOutside, "ThreeOutside");
node_candle_pattern!(TwoCrowsNode, wc::TwoCrows, "TwoCrows");
node_candle_pattern!(
    UpsideGapTwoCrowsNode,
    wc::UpsideGapTwoCrows,
    "UpsideGapTwoCrows"
);
node_candle_pattern!(
    IdenticalThreeCrowsNode,
    wc::IdenticalThreeCrows,
    "IdenticalThreeCrows"
);
node_candle_pattern!(ThreeLineStrikeNode, wc::ThreeLineStrike, "ThreeLineStrike");
node_candle_pattern!(
    ThreeStarsInSouthNode,
    wc::ThreeStarsInSouth,
    "ThreeStarsInSouth"
);
node_candle_pattern!(AbandonedBabyNode, wc::AbandonedBaby, "AbandonedBaby");
node_candle_pattern!(AdvanceBlockNode, wc::AdvanceBlock, "AdvanceBlock");
node_candle_pattern!(BeltHoldNode, wc::BeltHold, "BeltHold");
node_candle_pattern!(BreakawayNode, wc::Breakaway, "Breakaway");
node_candle_pattern!(CounterattackNode, wc::Counterattack, "Counterattack");
node_candle_pattern!(DojiStarNode, wc::DojiStar, "DojiStar");
node_candle_pattern!(DragonflyDojiNode, wc::DragonflyDoji, "DragonflyDoji");
node_candle_pattern!(GravestoneDojiNode, wc::GravestoneDoji, "GravestoneDoji");
node_candle_pattern!(LongLeggedDojiNode, wc::LongLeggedDoji, "LongLeggedDoji");
node_candle_pattern!(RickshawManNode, wc::RickshawMan, "RickshawMan");
node_candle_pattern!(EveningDojiStarNode, wc::EveningDojiStar, "EveningDojiStar");
node_candle_pattern!(MorningDojiStarNode, wc::MorningDojiStar, "MorningDojiStar");
node_candle_pattern!(
    GapSideBySideWhiteNode,
    wc::GapSideBySideWhite,
    "GapSideBySideWhite"
);
node_candle_pattern!(HighWaveNode, wc::HighWave, "HighWave");
node_candle_pattern!(HikkakeNode, wc::Hikkake, "Hikkake");
node_candle_pattern!(HikkakeModifiedNode, wc::HikkakeModified, "HikkakeModified");
node_candle_pattern!(HomingPigeonNode, wc::HomingPigeon, "HomingPigeon");
node_candle_pattern!(OnNeckNode, wc::OnNeck, "OnNeck");
node_candle_pattern!(InNeckNode, wc::InNeck, "InNeck");
node_candle_pattern!(ThrustingNode, wc::Thrusting, "Thrusting");
node_candle_pattern!(SeparatingLinesNode, wc::SeparatingLines, "SeparatingLines");
node_candle_pattern!(KickingNode, wc::Kicking, "Kicking");
node_candle_pattern!(KickingByLengthNode, wc::KickingByLength, "KickingByLength");
node_candle_pattern!(LadderBottomNode, wc::LadderBottom, "LadderBottom");
node_candle_pattern!(MatHoldNode, wc::MatHold, "MatHold");
node_candle_pattern!(MatchingLowNode, wc::MatchingLow, "MatchingLow");
node_candle_pattern!(LongLineNode, wc::LongLine, "LongLine");
node_candle_pattern!(ShortLineNode, wc::ShortLine, "ShortLine");
node_candle_pattern!(
    RisingThreeMethodsNode,
    wc::RisingThreeMethods,
    "RisingThreeMethods"
);
node_candle_pattern!(
    FallingThreeMethodsNode,
    wc::FallingThreeMethods,
    "FallingThreeMethods"
);
node_candle_pattern!(
    UpsideGapThreeMethodsNode,
    wc::UpsideGapThreeMethods,
    "UpsideGapThreeMethods"
);
node_candle_pattern!(
    DownsideGapThreeMethodsNode,
    wc::DownsideGapThreeMethods,
    "DownsideGapThreeMethods"
);
node_candle_pattern!(StalledPatternNode, wc::StalledPattern, "StalledPattern");
node_candle_pattern!(StickSandwichNode, wc::StickSandwich, "StickSandwich");
node_candle_pattern!(TakuriNode, wc::Takuri, "Takuri");
node_candle_pattern!(ClosingMarubozuNode, wc::ClosingMarubozu, "ClosingMarubozu");
node_candle_pattern!(OpeningMarubozuNode, wc::OpeningMarubozu, "OpeningMarubozu");
node_candle_pattern!(TasukiGapNode, wc::TasukiGap, "TasukiGap");
node_candle_pattern!(
    UniqueThreeRiverNode,
    wc::UniqueThreeRiver,
    "UniqueThreeRiver"
);
node_candle_pattern!(
    ConcealingBabySwallowNode,
    wc::ConcealingBabySwallow,
    "ConcealingBabySwallow"
);
node_candle_pattern!(DoubleTopBottomNode, wc::DoubleTopBottom, "DoubleTopBottom");
node_candle_pattern!(TripleTopBottomNode, wc::TripleTopBottom, "TripleTopBottom");
node_candle_pattern!(
    HeadAndShouldersNode,
    wc::HeadAndShoulders,
    "HeadAndShoulders"
);
node_candle_pattern!(TriangleNode, wc::Triangle, "Triangle");
node_candle_pattern!(WedgeNode, wc::Wedge, "Wedge");
node_candle_pattern!(FlagPennantNode, wc::FlagPennant, "FlagPennant");
node_candle_pattern!(RectangleRangeNode, wc::RectangleRange, "RectangleRange");
node_candle_pattern!(CupAndHandleNode, wc::CupAndHandle, "CupAndHandle");
node_candle_pattern!(AbcdNode, wc::Abcd, "Abcd");
node_candle_pattern!(GartleyNode, wc::Gartley, "Gartley");
node_candle_pattern!(ButterflyNode, wc::Butterfly, "Butterfly");
node_candle_pattern!(BatNode, wc::Bat, "Bat");
node_candle_pattern!(CrabNode, wc::Crab, "Crab");
node_candle_pattern!(SharkNode, wc::Shark, "Shark");
node_candle_pattern!(CypherNode, wc::Cypher, "Cypher");
node_candle_pattern!(ThreeDrivesNode, wc::ThreeDrives, "ThreeDrives");
node_candle_pattern!(TdCamouflageNode, wc::TdCamouflage, "TDCamouflage");
node_candle_pattern!(TdClopNode, wc::TdClop, "TDClop");
node_candle_pattern!(TdClopwinNode, wc::TdClopwin, "TDClopwin");
node_candle_pattern!(TdPropulsionNode, wc::TdPropulsion, "TDPropulsion");
node_candle_pattern!(TdTrapNode, wc::TdTrap, "TDTrap");
node_candle_pattern!(TristarNode, wc::Tristar, "Tristar");
node_candle_pattern!(HaramiCrossNode, wc::HaramiCross, "HaramiCross");
node_candle_pattern!(TowerTopBottomNode, wc::TowerTopBottom, "TowerTopBottom");

// ============================== Microstructure: Order Book ==============================
//
// Order-book indicators consume a depth snapshot rather than OHLCV. Streaming
// `update(bidPx, bidSz, askPx, askSz)` takes four equal-length arrays for one
// snapshot (bids best-first = descending price, asks best-first = ascending
// price); `batch` takes an array of `{ bidPx, bidSz, askPx, askSz }` snapshots
// and returns one value per snapshot.

/// One order-book depth snapshot for batch evaluation.
#[napi(object)]
pub struct ObSnapshot {
    pub bid_px: Vec<f64>,
    pub bid_sz: Vec<f64>,
    pub ask_px: Vec<f64>,
    pub ask_sz: Vec<f64>,
}

fn build_order_book(
    bid_px: &[f64],
    bid_sz: &[f64],
    ask_px: &[f64],
    ask_sz: &[f64],
) -> napi::Result<wc::OrderBook> {
    if bid_px.len() != bid_sz.len() || ask_px.len() != ask_sz.len() {
        return Err(NapiError::from_reason(
            "bid/ask price and size arrays must be equal length".to_string(),
        ));
    }
    let bids = bid_px
        .iter()
        .zip(bid_sz)
        .map(|(&p, &s)| wc::Level::new_unchecked(p, s))
        .collect();
    let asks = ask_px
        .iter()
        .zip(ask_sz)
        .map(|(&p, &s)| wc::Level::new_unchecked(p, s))
        .collect();
    wc::OrderBook::new(bids, asks).map_err(map_err)
}

macro_rules! node_ob_indicator {
    ($node:ident, $inner:ty, $js:literal) => {
        #[napi(js_name = $js)]
        pub struct $node {
            inner: $inner,
        }

        impl Default for $node {
            fn default() -> Self {
                Self::new()
            }
        }

        #[napi]
        impl $node {
            #[napi(constructor)]
            pub fn new() -> Self {
                Self {
                    inner: <$inner>::new(),
                }
            }
            #[napi]
            pub fn update(
                &mut self,
                bid_px: Vec<f64>,
                bid_sz: Vec<f64>,
                ask_px: Vec<f64>,
                ask_sz: Vec<f64>,
            ) -> napi::Result<Option<f64>> {
                let book = build_order_book(&bid_px, &bid_sz, &ask_px, &ask_sz)?;
                Ok(self.inner.update(book))
            }
            #[napi]
            pub fn batch(&mut self, snapshots: Vec<ObSnapshot>) -> napi::Result<Vec<f64>> {
                let mut out = Vec::with_capacity(snapshots.len());
                for snap in &snapshots {
                    let book =
                        build_order_book(&snap.bid_px, &snap.bid_sz, &snap.ask_px, &snap.ask_sz)?;
                    out.push(self.inner.update(book).unwrap_or(f64::NAN));
                }
                Ok(out)
            }
            #[napi]
            pub fn reset(&mut self) {
                self.inner.reset();
            }

            #[napi]
            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[napi(js_name = "isReady")]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[napi(js_name = "warmupPeriod")]
            pub fn warmup_period(&self) -> u32 {
                self.inner.warmup_period() as u32
            }
        }
    };
}

node_ob_indicator!(
    OrderBookImbalanceTop1Node,
    wc::OrderBookImbalanceTop1,
    "OrderBookImbalanceTop1"
);
node_ob_indicator!(
    OrderBookImbalanceFullNode,
    wc::OrderBookImbalanceFull,
    "OrderBookImbalanceFull"
);
node_ob_indicator!(MicropriceNode, wc::Microprice, "Microprice");
node_ob_indicator!(QuotedSpreadNode, wc::QuotedSpread, "QuotedSpread");
node_ob_indicator!(DepthSlopeNode, wc::DepthSlope, "DepthSlope");

// Top-N imbalance carries a `levels` parameter, so it is hand-written.
#[napi(js_name = "OrderBookImbalanceTopN")]
pub struct OrderBookImbalanceTopNNode {
    inner: wc::OrderBookImbalanceTopN,
}

#[napi]
impl OrderBookImbalanceTopNNode {
    #[napi(constructor)]
    pub fn new(levels: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::OrderBookImbalanceTopN::new(levels.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        bid_px: Vec<f64>,
        bid_sz: Vec<f64>,
        ask_px: Vec<f64>,
        ask_sz: Vec<f64>,
    ) -> napi::Result<Option<f64>> {
        let book = build_order_book(&bid_px, &bid_sz, &ask_px, &ask_sz)?;
        Ok(self.inner.update(book))
    }
    #[napi]
    pub fn batch(&mut self, snapshots: Vec<ObSnapshot>) -> napi::Result<Vec<f64>> {
        let mut out = Vec::with_capacity(snapshots.len());
        for snap in &snapshots {
            let book = build_order_book(&snap.bid_px, &snap.bid_sz, &snap.ask_px, &snap.ask_sz)?;
            out.push(self.inner.update(book).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Microstructure: Trade Flow ==============================
//
// Trade-flow indicators consume a trade tape rather than OHLCV. Streaming
// `update(price, size, isBuy)` takes one trade (`isBuy=true` for a
// buyer-initiated trade); `batch` takes three equal-length arrays.

fn build_trade(price: f64, size: f64, is_buy: bool) -> napi::Result<wc::Trade> {
    let side = if is_buy {
        wc::Side::Buy
    } else {
        wc::Side::Sell
    };
    wc::Trade::new(price, size, side, 0).map_err(map_err)
}

macro_rules! node_trade_indicator {
    ($node:ident, $inner:ty, $js:literal) => {
        #[napi(js_name = $js)]
        pub struct $node {
            inner: $inner,
        }

        impl Default for $node {
            fn default() -> Self {
                Self::new()
            }
        }

        #[napi]
        impl $node {
            #[napi(constructor)]
            pub fn new() -> Self {
                Self {
                    inner: <$inner>::new(),
                }
            }
            #[napi]
            pub fn update(
                &mut self,
                price: f64,
                size: f64,
                is_buy: bool,
            ) -> napi::Result<Option<f64>> {
                Ok(self.inner.update(build_trade(price, size, is_buy)?))
            }
            #[napi]
            pub fn batch(
                &mut self,
                price: Series,
                size: Series,
                is_buy: Vec<bool>,
            ) -> napi::Result<Vec<f64>> {
                if price.len() != size.len() || size.len() != is_buy.len() {
                    return Err(NapiError::from_reason(
                        "price, size, is_buy must be equal length".to_string(),
                    ));
                }
                let mut out = Vec::with_capacity(price.len());
                for i in 0..price.len() {
                    let trade = build_trade(price[i], size[i], is_buy[i])?;
                    out.push(self.inner.update(trade).unwrap_or(f64::NAN));
                }
                Ok(out)
            }
            #[napi]
            pub fn reset(&mut self) {
                self.inner.reset();
            }

            #[napi]
            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[napi(js_name = "isReady")]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[napi(js_name = "warmupPeriod")]
            pub fn warmup_period(&self) -> u32 {
                self.inner.warmup_period() as u32
            }
        }
    };
}

node_trade_indicator!(SignedVolumeNode, wc::SignedVolume, "SignedVolume");
node_trade_indicator!(
    CumulativeVolumeDeltaNode,
    wc::CumulativeVolumeDelta,
    "CumulativeVolumeDelta"
);

// Trade imbalance carries a `window` parameter, so it is hand-written.
#[napi(js_name = "TradeImbalance")]
pub struct TradeImbalanceNode {
    inner: wc::TradeImbalance,
}

#[napi]
impl TradeImbalanceNode {
    #[napi(constructor)]
    pub fn new(window: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TradeImbalance::new(window.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, price: f64, size: f64, is_buy: bool) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        price: Series,
        size: Series,
        is_buy: Vec<bool>,
    ) -> napi::Result<Vec<f64>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(NapiError::from_reason(
                "price, size, is_buy must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let trade = build_trade(price[i], size[i], is_buy[i])?;
            out.push(self.inner.update(trade).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// Trade-sign autocorrelation carries a `period` parameter, so it is hand-written.
#[napi(js_name = "TradeSignAutocorrelation")]
pub struct TradeSignAutocorrelationNode {
    inner: wc::TradeSignAutocorrelation,
}

#[napi]
impl TradeSignAutocorrelationNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TradeSignAutocorrelation::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, price: f64, size: f64, is_buy: bool) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        price: Series,
        size: Series,
        is_buy: Vec<bool>,
    ) -> napi::Result<Vec<f64>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(NapiError::from_reason(
                "price, size, is_buy must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let trade = build_trade(price[i], size[i], is_buy[i])?;
            out.push(self.inner.update(trade).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// PIN carries a `window` parameter, so it is hand-written.
#[napi(js_name = "Pin")]
pub struct PinNode {
    inner: wc::Pin,
}

#[napi]
impl PinNode {
    #[napi(constructor)]
    pub fn new(window: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Pin::new(window.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, price: f64, size: f64, is_buy: bool) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        price: Series,
        size: Series,
        is_buy: Vec<bool>,
    ) -> napi::Result<Vec<f64>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(NapiError::from_reason(
                "price, size, is_buy must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let trade = build_trade(price[i], size[i], is_buy[i])?;
            out.push(self.inner.update(trade).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// Order Flow Imbalance: order-book input with a `period` parameter.
#[napi(js_name = "OrderFlowImbalance")]
pub struct OrderFlowImbalanceNode {
    inner: wc::OrderFlowImbalance,
}

#[napi]
impl OrderFlowImbalanceNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::OrderFlowImbalance::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        bid_px: Vec<f64>,
        bid_sz: Vec<f64>,
        ask_px: Vec<f64>,
        ask_sz: Vec<f64>,
    ) -> napi::Result<Option<f64>> {
        let book = build_order_book(&bid_px, &bid_sz, &ask_px, &ask_sz)?;
        Ok(self.inner.update(book))
    }
    #[napi]
    pub fn batch(&mut self, snapshots: Vec<ObSnapshot>) -> napi::Result<Vec<f64>> {
        let mut out = Vec::with_capacity(snapshots.len());
        for snap in &snapshots {
            let book = build_order_book(&snap.bid_px, &snap.bid_sz, &snap.ask_px, &snap.ask_sz)?;
            out.push(self.inner.update(book).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// VPIN: trade input, volume-bucketed `(bucket_volume, num_buckets)`.
#[napi(js_name = "Vpin")]
pub struct VpinNode {
    inner: wc::Vpin,
}

#[napi]
impl VpinNode {
    #[napi(constructor)]
    pub fn new(bucket_volume: f64, num_buckets: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Vpin::new(bucket_volume, num_buckets.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, price: f64, size: f64, is_buy: bool) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        price: Series,
        size: Series,
        is_buy: Vec<bool>,
    ) -> napi::Result<Vec<f64>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(NapiError::from_reason(
                "price, size, is_buy must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let trade = build_trade(price[i], size[i], is_buy[i])?;
            out.push(self.inner.update(trade).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// Amihud Illiquidity: trade input with a `period` parameter.
#[napi(js_name = "AmihudIlliquidity")]
pub struct AmihudIlliquidityNode {
    inner: wc::AmihudIlliquidity,
}

#[napi]
impl AmihudIlliquidityNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AmihudIlliquidity::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, price: f64, size: f64, is_buy: bool) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        price: Series,
        size: Series,
        is_buy: Vec<bool>,
    ) -> napi::Result<Vec<f64>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(NapiError::from_reason(
                "price, size, is_buy must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let trade = build_trade(price[i], size[i], is_buy[i])?;
            out.push(self.inner.update(trade).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// Roll Measure: trade input with a `period` parameter.
#[napi(js_name = "RollMeasure")]
pub struct RollMeasureNode {
    inner: wc::RollMeasure,
}

#[napi]
impl RollMeasureNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RollMeasure::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, price: f64, size: f64, is_buy: bool) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        price: Series,
        size: Series,
        is_buy: Vec<bool>,
    ) -> napi::Result<Vec<f64>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(NapiError::from_reason(
                "price, size, is_buy must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let trade = build_trade(price[i], size[i], is_buy[i])?;
            out.push(self.inner.update(trade).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Microstructure: Price Impact ==============================
//
// Price-impact indicators consume a trade paired with the mid prevailing at
// execution. Streaming `update(price, size, isBuy, mid)` takes one such
// trade-quote (`isBuy=true` for a buyer-initiated trade); `batch` takes four
// equal-length arrays.

fn build_trade_quote(
    price: f64,
    size: f64,
    is_buy: bool,
    mid: f64,
) -> napi::Result<wc::TradeQuote> {
    let trade = build_trade(price, size, is_buy)?;
    wc::TradeQuote::new(trade, mid).map_err(map_err)
}

macro_rules! node_trade_quote_indicator {
    ($node:ident, $inner:ty, $js:literal) => {
        #[napi(js_name = $js)]
        pub struct $node {
            inner: $inner,
        }

        impl Default for $node {
            fn default() -> Self {
                Self::new()
            }
        }

        #[napi]
        impl $node {
            #[napi(constructor)]
            pub fn new() -> Self {
                Self {
                    inner: <$inner>::new(),
                }
            }
            #[napi]
            pub fn update(
                &mut self,
                price: f64,
                size: f64,
                is_buy: bool,
                mid: f64,
            ) -> napi::Result<Option<f64>> {
                Ok(self
                    .inner
                    .update(build_trade_quote(price, size, is_buy, mid)?))
            }
            #[napi]
            pub fn batch(
                &mut self,
                price: Series,
                size: Series,
                is_buy: Vec<bool>,
                mid: Series,
            ) -> napi::Result<Vec<f64>> {
                if price.len() != size.len()
                    || size.len() != is_buy.len()
                    || is_buy.len() != mid.len()
                {
                    return Err(NapiError::from_reason(
                        "price, size, is_buy, mid must be equal length".to_string(),
                    ));
                }
                let mut out = Vec::with_capacity(price.len());
                for i in 0..price.len() {
                    let quote = build_trade_quote(price[i], size[i], is_buy[i], mid[i])?;
                    out.push(self.inner.update(quote).unwrap_or(f64::NAN));
                }
                Ok(out)
            }
            #[napi]
            pub fn reset(&mut self) {
                self.inner.reset();
            }

            #[napi]
            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[napi(js_name = "isReady")]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[napi(js_name = "warmupPeriod")]
            pub fn warmup_period(&self) -> u32 {
                self.inner.warmup_period() as u32
            }
        }
    };
}

node_trade_quote_indicator!(EffectiveSpreadNode, wc::EffectiveSpread, "EffectiveSpread");

// Realized spread carries a `horizon` parameter, so it is hand-written.
#[napi(js_name = "RealizedSpread")]
pub struct RealizedSpreadNode {
    inner: wc::RealizedSpread,
}

#[napi]
impl RealizedSpreadNode {
    #[napi(constructor)]
    pub fn new(horizon: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RealizedSpread::new(horizon.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        price: f64,
        size: f64,
        is_buy: bool,
        mid: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_trade_quote(price, size, is_buy, mid)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        price: Series,
        size: Series,
        is_buy: Vec<bool>,
        mid: Series,
    ) -> napi::Result<Vec<f64>> {
        if price.len() != size.len() || size.len() != is_buy.len() || is_buy.len() != mid.len() {
            return Err(NapiError::from_reason(
                "price, size, is_buy, mid must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let quote = build_trade_quote(price[i], size[i], is_buy[i], mid[i])?;
            out.push(self.inner.update(quote).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// Kyle's lambda carries a `window` parameter, so it is hand-written.
#[napi(js_name = "KylesLambda")]
pub struct KylesLambdaNode {
    inner: wc::KylesLambda,
}

#[napi]
impl KylesLambdaNode {
    #[napi(constructor)]
    pub fn new(window: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::KylesLambda::new(window.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        price: f64,
        size: f64,
        is_buy: bool,
        mid: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_trade_quote(price, size, is_buy, mid)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        price: Series,
        size: Series,
        is_buy: Vec<bool>,
        mid: Series,
    ) -> napi::Result<Vec<f64>> {
        if price.len() != size.len() || size.len() != is_buy.len() || is_buy.len() != mid.len() {
            return Err(NapiError::from_reason(
                "price, size, is_buy, mid must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let quote = build_trade_quote(price[i], size[i], is_buy[i], mid[i])?;
            out.push(self.inner.update(quote).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Microstructure: Footprint ==============================
//
// Footprint is a multi-output, variable-length indicator. Each `update(price,
// size, isBuy)` returns the full bar footprint accumulated since the last
// `reset()` as an array of `{ price, bidVol, askVol }` rows (sorted ascending
// by price); `batch` returns an array of such arrays, one per trade.

/// One price bucket of a footprint.
#[napi(object)]
pub struct FootprintLevelValue {
    pub price: f64,
    pub bid_vol: f64,
    pub ask_vol: f64,
}

fn footprint_levels(out: &wc::FootprintOutput) -> Vec<FootprintLevelValue> {
    out.levels
        .iter()
        .map(|level| FootprintLevelValue {
            price: level.price,
            bid_vol: level.bid_vol,
            ask_vol: level.ask_vol,
        })
        .collect()
}

#[napi(js_name = "Footprint")]
pub struct FootprintNode {
    inner: wc::Footprint,
}

#[napi]
impl FootprintNode {
    #[napi(constructor)]
    pub fn new(tick_size: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Footprint::new(tick_size).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        price: f64,
        size: f64,
        is_buy: bool,
    ) -> napi::Result<Vec<FootprintLevelValue>> {
        let out = self
            .inner
            .update(build_trade(price, size, is_buy)?)
            .expect("footprint emits on every trade");
        Ok(footprint_levels(&out))
    }
    #[napi]
    pub fn batch(
        &mut self,
        price: Series,
        size: Series,
        is_buy: Vec<bool>,
    ) -> napi::Result<Vec<Vec<FootprintLevelValue>>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(NapiError::from_reason(
                "price, size, is_buy must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let snapshot = self
                .inner
                .update(build_trade(price[i], size[i], is_buy[i])?)
                .expect("footprint emits on every trade");
            out.push(footprint_levels(&snapshot));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Derivatives ==============================
//
// Derivatives indicators consume a perpetual / futures tick rather than OHLCV.
// Each wrapper exposes only the tick fields its indicator reads; the helpers
// below build a fully-valid `DerivativesTick`, filling the unused fields with
// neutral defaults (prices `1.0`, sizes / rates `0.0`).

fn deriv_funding(funding_rate: f64) -> napi::Result<wc::DerivativesTick> {
    wc::DerivativesTick::new(
        funding_rate,
        1.0,
        1.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0,
    )
    .map_err(map_err)
}

fn deriv_basis(mark_price: f64, index_price: f64) -> napi::Result<wc::DerivativesTick> {
    wc::DerivativesTick::new(
        0.0,
        mark_price,
        index_price,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0,
    )
    .map_err(map_err)
}

fn deriv_oi(open_interest: f64) -> napi::Result<wc::DerivativesTick> {
    wc::DerivativesTick::new(
        0.0,
        1.0,
        1.0,
        1.0,
        open_interest,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0,
    )
    .map_err(map_err)
}

fn deriv_oi_mark(open_interest: f64, mark_price: f64) -> napi::Result<wc::DerivativesTick> {
    wc::DerivativesTick::new(
        0.0,
        mark_price,
        1.0,
        1.0,
        open_interest,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0,
    )
    .map_err(map_err)
}

fn deriv_long_short(long_size: f64, short_size: f64) -> napi::Result<wc::DerivativesTick> {
    wc::DerivativesTick::new(
        0.0, 1.0, 1.0, 1.0, 0.0, long_size, short_size, 0.0, 0.0, 0.0, 0.0, 0,
    )
    .map_err(map_err)
}

fn deriv_taker(taker_buy_volume: f64, taker_sell_volume: f64) -> napi::Result<wc::DerivativesTick> {
    wc::DerivativesTick::new(
        0.0,
        1.0,
        1.0,
        1.0,
        0.0,
        0.0,
        0.0,
        taker_buy_volume,
        taker_sell_volume,
        0.0,
        0.0,
        0,
    )
    .map_err(map_err)
}

fn deriv_oi_long_short(
    open_interest: f64,
    long_size: f64,
    short_size: f64,
) -> napi::Result<wc::DerivativesTick> {
    wc::DerivativesTick::new(
        0.0,
        1.0,
        1.0,
        1.0,
        open_interest,
        long_size,
        short_size,
        0.0,
        0.0,
        0.0,
        0.0,
        0,
    )
    .map_err(map_err)
}

fn deriv_oi_taker(
    open_interest: f64,
    taker_buy_volume: f64,
    taker_sell_volume: f64,
) -> napi::Result<wc::DerivativesTick> {
    wc::DerivativesTick::new(
        0.0,
        1.0,
        1.0,
        1.0,
        open_interest,
        0.0,
        0.0,
        taker_buy_volume,
        taker_sell_volume,
        0.0,
        0.0,
        0,
    )
    .map_err(map_err)
}

fn deriv_liquidation(
    long_liquidation: f64,
    short_liquidation: f64,
) -> napi::Result<wc::DerivativesTick> {
    wc::DerivativesTick::new(
        0.0,
        1.0,
        1.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        long_liquidation,
        short_liquidation,
        0,
    )
    .map_err(map_err)
}

fn deriv_futures_index(futures_price: f64, index_price: f64) -> napi::Result<wc::DerivativesTick> {
    wc::DerivativesTick::new(
        0.0,
        1.0,
        index_price,
        futures_price,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0,
    )
    .map_err(map_err)
}

fn deriv_futures_mark(futures_price: f64, mark_price: f64) -> napi::Result<wc::DerivativesTick> {
    wc::DerivativesTick::new(
        0.0,
        mark_price,
        1.0,
        futures_price,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0,
    )
    .map_err(map_err)
}

#[napi(js_name = "FundingRate")]
pub struct FundingRateNode {
    inner: wc::FundingRate,
}

impl Default for FundingRateNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl FundingRateNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::FundingRate::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, funding_rate: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(deriv_funding(funding_rate)?))
    }
    #[napi]
    pub fn batch(&mut self, funding_rate: Series) -> napi::Result<Vec<f64>> {
        let mut out = Vec::with_capacity(funding_rate.len());
        for &rate in &funding_rate {
            out.push(self.inner.update(deriv_funding(rate)?).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "FundingRateMean")]
pub struct FundingRateMeanNode {
    inner: wc::FundingRateMean,
}

#[napi]
impl FundingRateMeanNode {
    #[napi(constructor)]
    pub fn new(window: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::FundingRateMean::new(window.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, funding_rate: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(deriv_funding(funding_rate)?))
    }
    #[napi]
    pub fn batch(&mut self, funding_rate: Series) -> napi::Result<Vec<f64>> {
        let mut out = Vec::with_capacity(funding_rate.len());
        for &rate in &funding_rate {
            out.push(self.inner.update(deriv_funding(rate)?).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "FundingRateZScore")]
pub struct FundingRateZScoreNode {
    inner: wc::FundingRateZScore,
}

#[napi]
impl FundingRateZScoreNode {
    #[napi(constructor)]
    pub fn new(window: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::FundingRateZScore::new(window.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, funding_rate: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(deriv_funding(funding_rate)?))
    }
    #[napi]
    pub fn batch(&mut self, funding_rate: Series) -> napi::Result<Vec<f64>> {
        let mut out = Vec::with_capacity(funding_rate.len());
        for &rate in &funding_rate {
            out.push(self.inner.update(deriv_funding(rate)?).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "FundingBasis")]
pub struct FundingBasisNode {
    inner: wc::FundingBasis,
}

impl Default for FundingBasisNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl FundingBasisNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::FundingBasis::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, mark_price: f64, index_price: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(deriv_basis(mark_price, index_price)?))
    }
    #[napi]
    pub fn batch(&mut self, mark_price: Series, index_price: Series) -> napi::Result<Vec<f64>> {
        if mark_price.len() != index_price.len() {
            return Err(NapiError::from_reason(
                "mark_price and index_price must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(mark_price.len());
        for i in 0..mark_price.len() {
            out.push(
                self.inner
                    .update(deriv_basis(mark_price[i], index_price[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "OpenInterestDelta")]
pub struct OpenInterestDeltaNode {
    inner: wc::OpenInterestDelta,
}

impl Default for OpenInterestDeltaNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl OpenInterestDeltaNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::OpenInterestDelta::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, open_interest: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(deriv_oi(open_interest)?))
    }
    #[napi]
    pub fn batch(&mut self, open_interest: Series) -> napi::Result<Vec<f64>> {
        let mut out = Vec::with_capacity(open_interest.len());
        for &oi in &open_interest {
            out.push(self.inner.update(deriv_oi(oi)?).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "OIPriceDivergence")]
pub struct OIPriceDivergenceNode {
    inner: wc::OIPriceDivergence,
}

#[napi]
impl OIPriceDivergenceNode {
    #[napi(constructor)]
    pub fn new(window: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::OIPriceDivergence::new(window.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, open_interest: f64, mark_price: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(deriv_oi_mark(open_interest, mark_price)?))
    }
    #[napi]
    pub fn batch(&mut self, open_interest: Series, mark_price: Series) -> napi::Result<Vec<f64>> {
        if open_interest.len() != mark_price.len() {
            return Err(NapiError::from_reason(
                "open_interest and mark_price must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open_interest.len());
        for i in 0..open_interest.len() {
            out.push(
                self.inner
                    .update(deriv_oi_mark(open_interest[i], mark_price[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "OIWeighted")]
pub struct OIWeightedNode {
    inner: wc::OIWeighted,
}

impl Default for OIWeightedNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl OIWeightedNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::OIWeighted::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, mark_price: f64, open_interest: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(deriv_oi_mark(open_interest, mark_price)?))
    }
    #[napi]
    pub fn batch(&mut self, mark_price: Series, open_interest: Series) -> napi::Result<Vec<f64>> {
        if mark_price.len() != open_interest.len() {
            return Err(NapiError::from_reason(
                "mark_price and open_interest must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(mark_price.len());
        for i in 0..mark_price.len() {
            out.push(
                self.inner
                    .update(deriv_oi_mark(open_interest[i], mark_price[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "LongShortRatio")]
pub struct LongShortRatioNode {
    inner: wc::LongShortRatio,
}

impl Default for LongShortRatioNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl LongShortRatioNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::LongShortRatio::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, long_size: f64, short_size: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(deriv_long_short(long_size, short_size)?))
    }
    #[napi]
    pub fn batch(&mut self, long_size: Series, short_size: Series) -> napi::Result<Vec<f64>> {
        if long_size.len() != short_size.len() {
            return Err(NapiError::from_reason(
                "long_size and short_size must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(long_size.len());
        for i in 0..long_size.len() {
            out.push(
                self.inner
                    .update(deriv_long_short(long_size[i], short_size[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "TakerBuySellRatio")]
pub struct TakerBuySellRatioNode {
    inner: wc::TakerBuySellRatio,
}

impl Default for TakerBuySellRatioNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl TakerBuySellRatioNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::TakerBuySellRatio::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        taker_buy_volume: f64,
        taker_sell_volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(deriv_taker(taker_buy_volume, taker_sell_volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        taker_buy_volume: Series,
        taker_sell_volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if taker_buy_volume.len() != taker_sell_volume.len() {
            return Err(NapiError::from_reason(
                "taker_buy_volume and taker_sell_volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(taker_buy_volume.len());
        for i in 0..taker_buy_volume.len() {
            out.push(
                self.inner
                    .update(deriv_taker(taker_buy_volume[i], taker_sell_volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

/// The liquidation feature vector for one tick.
#[napi(object)]
pub struct LiquidationFeaturesValue {
    pub long: f64,
    pub short: f64,
    pub net: f64,
    pub total: f64,
    pub imbalance: f64,
}

#[napi(js_name = "LiquidationFeatures")]
pub struct LiquidationFeaturesNode {
    inner: wc::LiquidationFeatures,
}

impl Default for LiquidationFeaturesNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl LiquidationFeaturesNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::LiquidationFeatures::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        long_liquidation: f64,
        short_liquidation: f64,
    ) -> napi::Result<Option<LiquidationFeaturesValue>> {
        Ok(self
            .inner
            .update(deriv_liquidation(long_liquidation, short_liquidation)?)
            .map(|o| LiquidationFeaturesValue {
                long: o.long,
                short: o.short,
                net: o.net,
                total: o.total,
                imbalance: o.imbalance,
            }))
    }
    #[napi]
    pub fn batch(
        &mut self,
        long_liquidation: Series,
        short_liquidation: Series,
    ) -> napi::Result<Vec<f64>> {
        if long_liquidation.len() != short_liquidation.len() {
            return Err(NapiError::from_reason(
                "long_liquidation and short_liquidation must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(long_liquidation.len() * 5);
        for i in 0..long_liquidation.len() {
            let o = self
                .inner
                .update(deriv_liquidation(
                    long_liquidation[i],
                    short_liquidation[i],
                )?)
                .expect("liquidation features emit on every tick");
            out.push(o.long);
            out.push(o.short);
            out.push(o.net);
            out.push(o.total);
            out.push(o.imbalance);
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "TermStructureBasis")]
pub struct TermStructureBasisNode {
    inner: wc::TermStructureBasis,
}

impl Default for TermStructureBasisNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl TermStructureBasisNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::TermStructureBasis::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, futures_price: f64, index_price: f64) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(deriv_futures_index(futures_price, index_price)?))
    }
    #[napi]
    pub fn batch(&mut self, futures_price: Series, index_price: Series) -> napi::Result<Vec<f64>> {
        if futures_price.len() != index_price.len() {
            return Err(NapiError::from_reason(
                "futures_price and index_price must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(futures_price.len());
        for i in 0..futures_price.len() {
            out.push(
                self.inner
                    .update(deriv_futures_index(futures_price[i], index_price[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "CalendarSpread")]
pub struct CalendarSpreadNode {
    inner: wc::CalendarSpread,
}

impl Default for CalendarSpreadNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl CalendarSpreadNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::CalendarSpread::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, futures_price: f64, mark_price: f64) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(deriv_futures_mark(futures_price, mark_price)?))
    }
    #[napi]
    pub fn batch(&mut self, futures_price: Series, mark_price: Series) -> napi::Result<Vec<f64>> {
        if futures_price.len() != mark_price.len() {
            return Err(NapiError::from_reason(
                "futures_price and mark_price must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(futures_price.len());
        for i in 0..futures_price.len() {
            out.push(
                self.inner
                    .update(deriv_futures_mark(futures_price[i], mark_price[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// Estimated leverage ratio: open interest over aggregate long+short size.
#[napi(js_name = "EstimatedLeverageRatio")]
pub struct EstimatedLeverageRatioNode {
    inner: wc::EstimatedLeverageRatio,
}

impl Default for EstimatedLeverageRatioNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl EstimatedLeverageRatioNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::EstimatedLeverageRatio::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open_interest: f64,
        long_size: f64,
        short_size: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(deriv_oi_long_short(open_interest, long_size, short_size)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open_interest: Series,
        long_size: Series,
        short_size: Series,
    ) -> napi::Result<Vec<f64>> {
        if open_interest.len() != long_size.len() || long_size.len() != short_size.len() {
            return Err(NapiError::from_reason(
                "open_interest, long_size, short_size must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open_interest.len());
        for i in 0..open_interest.len() {
            out.push(
                self.inner
                    .update(deriv_oi_long_short(
                        open_interest[i],
                        long_size[i],
                        short_size[i],
                    )?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// OI-to-volume ratio: open interest over taker buy+sell volume.
#[napi(js_name = "OiToVolumeRatio")]
pub struct OiToVolumeRatioNode {
    inner: wc::OiToVolumeRatio,
}

impl Default for OiToVolumeRatioNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl OiToVolumeRatioNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::OiToVolumeRatio::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open_interest: f64,
        taker_buy_volume: f64,
        taker_sell_volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(deriv_oi_taker(
            open_interest,
            taker_buy_volume,
            taker_sell_volume,
        )?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open_interest: Series,
        taker_buy_volume: Series,
        taker_sell_volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if open_interest.len() != taker_buy_volume.len()
            || taker_buy_volume.len() != taker_sell_volume.len()
        {
            return Err(NapiError::from_reason(
                "open_interest, taker_buy_volume, taker_sell_volume must be equal length"
                    .to_string(),
            ));
        }
        let mut out = Vec::with_capacity(open_interest.len());
        for i in 0..open_interest.len() {
            out.push(
                self.inner
                    .update(deriv_oi_taker(
                        open_interest[i],
                        taker_buy_volume[i],
                        taker_sell_volume[i],
                    )?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// Perpetual premium index: relative premium of mark over index price.
#[napi(js_name = "PerpetualPremiumIndex")]
pub struct PerpetualPremiumIndexNode {
    inner: wc::PerpetualPremiumIndex,
}

impl Default for PerpetualPremiumIndexNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl PerpetualPremiumIndexNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::PerpetualPremiumIndex::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, mark_price: f64, index_price: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(deriv_basis(mark_price, index_price)?))
    }
    #[napi]
    pub fn batch(&mut self, mark_price: Series, index_price: Series) -> napi::Result<Vec<f64>> {
        if mark_price.len() != index_price.len() {
            return Err(NapiError::from_reason(
                "mark_price and index_price must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(mark_price.len());
        for i in 0..mark_price.len() {
            out.push(
                self.inner
                    .update(deriv_basis(mark_price[i], index_price[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// Funding-implied APR: per-interval funding annualised.
#[napi(js_name = "FundingImpliedApr")]
pub struct FundingImpliedAprNode {
    inner: wc::FundingImpliedApr,
}

#[napi]
impl FundingImpliedAprNode {
    #[napi(constructor)]
    pub fn new(intervals_per_year: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::FundingImpliedApr::new(intervals_per_year).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, funding_rate: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(deriv_funding(funding_rate)?))
    }
    #[napi]
    pub fn batch(&mut self, funding_rate: Series) -> napi::Result<Vec<f64>> {
        let mut out = Vec::with_capacity(funding_rate.len());
        for &r in &funding_rate {
            out.push(self.inner.update(deriv_funding(r)?).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// Open-interest momentum: rate-of-change of open interest over a window.
#[napi(js_name = "OpenInterestMomentum")]
pub struct OpenInterestMomentumNode {
    inner: wc::OpenInterestMomentum,
}

#[napi]
impl OpenInterestMomentumNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::OpenInterestMomentum::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, open_interest: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(deriv_oi(open_interest)?))
    }
    #[napi]
    pub fn batch(&mut self, open_interest: Series) -> napi::Result<Vec<f64>> {
        let mut out = Vec::with_capacity(open_interest.len());
        for &oi in &open_interest {
            out.push(self.inner.update(deriv_oi(oi)?).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ---------- Market Breadth (CrossSection input) ----------
//
// A breadth tick is the per-symbol state of the whole universe, passed as four
// equal-length parallel arrays (`change`, `volume`, `newHigh`, `newLow`).
// `batch` takes one such group of arrays per tick.

fn build_cross_section(
    change: &[f64],
    volume: &[f64],
    new_high: &[bool],
    new_low: &[bool],
) -> napi::Result<wc::CrossSection> {
    if change.len() != volume.len()
        || change.len() != new_high.len()
        || change.len() != new_low.len()
    {
        return Err(NapiError::from_reason(
            "change, volume, newHigh and newLow must be equal length".to_string(),
        ));
    }
    let members = (0..change.len())
        .map(|i| wc::Member::new(change[i], volume[i], new_high[i], new_low[i]))
        .collect();
    wc::CrossSection::new(members, 0).map_err(map_err)
}

#[napi(js_name = "AdvanceDecline")]
pub struct AdvanceDeclineNode {
    inner: wc::AdvanceDecline,
}

impl Default for AdvanceDeclineNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl AdvanceDeclineNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::AdvanceDecline::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

fn build_cross_section_above_ma(
    change: &[f64],
    volume: &[f64],
    new_high: &[bool],
    new_low: &[bool],
    above_ma: &[bool],
) -> napi::Result<wc::CrossSection> {
    if change.len() != volume.len()
        || change.len() != new_high.len()
        || change.len() != new_low.len()
        || change.len() != above_ma.len()
    {
        return Err(NapiError::from_reason(
            "change, volume, newHigh, newLow and aboveMa must be equal length".to_string(),
        ));
    }
    let members = (0..change.len())
        .map(|i| {
            wc::Member::with_signals(
                change[i],
                volume[i],
                new_high[i],
                new_low[i],
                above_ma[i],
                false,
            )
        })
        .collect();
    wc::CrossSection::new(members, 0).map_err(map_err)
}

fn build_cross_section_buy(
    change: &[f64],
    volume: &[f64],
    new_high: &[bool],
    new_low: &[bool],
    on_buy_signal: &[bool],
) -> napi::Result<wc::CrossSection> {
    if change.len() != volume.len()
        || change.len() != new_high.len()
        || change.len() != new_low.len()
        || change.len() != on_buy_signal.len()
    {
        return Err(NapiError::from_reason(
            "change, volume, newHigh, newLow and onBuySignal must be equal length".to_string(),
        ));
    }
    let members = (0..change.len())
        .map(|i| {
            wc::Member::with_signals(
                change[i],
                volume[i],
                new_high[i],
                new_low[i],
                false,
                on_buy_signal[i],
            )
        })
        .collect();
    wc::CrossSection::new(members, 0).map_err(map_err)
}

#[napi(js_name = "AdvanceDeclineRatio")]
pub struct AdvanceDeclineRatioNode {
    inner: wc::AdvanceDeclineRatio,
}

impl Default for AdvanceDeclineRatioNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl AdvanceDeclineRatioNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::AdvanceDeclineRatio::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "AdVolumeLine")]
pub struct AdVolumeLineNode {
    inner: wc::AdVolumeLine,
}

impl Default for AdVolumeLineNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl AdVolumeLineNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::AdVolumeLine::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "McClellanOscillator")]
pub struct McClellanOscillatorNode {
    inner: wc::McClellanOscillator,
}

impl Default for McClellanOscillatorNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl McClellanOscillatorNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::McClellanOscillator::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "McClellanSummationIndex")]
pub struct McClellanSummationIndexNode {
    inner: wc::McClellanSummationIndex,
}

impl Default for McClellanSummationIndexNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl McClellanSummationIndexNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::McClellanSummationIndex::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "Trin")]
pub struct TrinNode {
    inner: wc::Trin,
}

impl Default for TrinNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl TrinNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::Trin::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "BreadthThrust")]
pub struct BreadthThrustNode {
    inner: wc::BreadthThrust,
}

#[napi]
impl BreadthThrustNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::BreadthThrust::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "NewHighsNewLows")]
pub struct NewHighsNewLowsNode {
    inner: wc::NewHighsNewLows,
}

impl Default for NewHighsNewLowsNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl NewHighsNewLowsNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::NewHighsNewLows::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "HighLowIndex")]
pub struct HighLowIndexNode {
    inner: wc::HighLowIndex,
}

#[napi]
impl HighLowIndexNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::HighLowIndex::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "PercentAboveMa")]
pub struct PercentAboveMaNode {
    inner: wc::PercentAboveMa,
}

impl Default for PercentAboveMaNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl PercentAboveMaNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::PercentAboveMa::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
        above_ma: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(build_cross_section_above_ma(
            &change, &volume, &new_high, &new_low, &above_ma,
        )?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
        above_ma: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
            || change.len() != above_ma.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh, newLow and aboveMa must have the same number of ticks"
                    .to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section_above_ma(
                &change[i],
                &volume[i],
                &new_high[i],
                &new_low[i],
                &above_ma[i],
            )?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "UpDownVolumeRatio")]
pub struct UpDownVolumeRatioNode {
    inner: wc::UpDownVolumeRatio,
}

impl Default for UpDownVolumeRatioNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl UpDownVolumeRatioNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::UpDownVolumeRatio::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "BullishPercentIndex")]
pub struct BullishPercentIndexNode {
    inner: wc::BullishPercentIndex,
}

impl Default for BullishPercentIndexNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl BullishPercentIndexNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::BullishPercentIndex::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
        on_buy_signal: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(build_cross_section_buy(
            &change,
            &volume,
            &new_high,
            &new_low,
            &on_buy_signal,
        )?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
        on_buy_signal: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
            || change.len() != on_buy_signal.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh, newLow and onBuySignal must have the same number of ticks"
                    .to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section_buy(
                &change[i],
                &volume[i],
                &new_high[i],
                &new_low[i],
                &on_buy_signal[i],
            )?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "CumulativeVolumeIndex")]
pub struct CumulativeVolumeIndexNode {
    inner: wc::CumulativeVolumeIndex,
}

impl Default for CumulativeVolumeIndexNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl CumulativeVolumeIndexNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::CumulativeVolumeIndex::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "AbsoluteBreadthIndex")]
pub struct AbsoluteBreadthIndexNode {
    inner: wc::AbsoluteBreadthIndex,
}

impl Default for AbsoluteBreadthIndexNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl AbsoluteBreadthIndexNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::AbsoluteBreadthIndex::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "TickIndex")]
pub struct TickIndexNode {
    inner: wc::TickIndex,
}

impl Default for TickIndexNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl TickIndexNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::TickIndex::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> napi::Result<Vec<f64>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(NapiError::from_reason(
                "change, volume, newHigh and newLow must have the same number of ticks".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Family 15: Risk / Performance ==============================

// Risk metrics with fallible `new` (most need `period >= 2`), so each wrapper
// is written by hand rather than going through the `node_scalar_indicator!`
// macro above.

#[napi(js_name = "SharpeRatio")]
pub struct SharpeRatioNode {
    inner: wc::SharpeRatio,
}

#[napi]
impl SharpeRatioNode {
    #[napi(constructor)]
    pub fn new(period: Count, risk_free: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::SharpeRatio::new(period.get(), risk_free).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "SortinoRatio")]
pub struct SortinoRatioNode {
    inner: wc::SortinoRatio,
}

#[napi]
impl SortinoRatioNode {
    #[napi(constructor)]
    pub fn new(period: Count, mar: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::SortinoRatio::new(period.get(), mar).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "CalmarRatio")]
pub struct CalmarRatioNode {
    inner: wc::CalmarRatio,
}

#[napi]
impl CalmarRatioNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::CalmarRatio::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "OmegaRatio")]
pub struct OmegaRatioNode {
    inner: wc::OmegaRatio,
}

#[napi]
impl OmegaRatioNode {
    #[napi(constructor)]
    pub fn new(period: Count, threshold: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::OmegaRatio::new(period.get(), threshold).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "MaxDrawdown")]
pub struct MaxDrawdownNode {
    inner: wc::MaxDrawdown,
}

#[napi]
impl MaxDrawdownNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::MaxDrawdown::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "AverageDrawdown")]
pub struct AverageDrawdownNode {
    inner: wc::AverageDrawdown,
}

#[napi]
impl AverageDrawdownNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AverageDrawdown::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "DrawdownDuration")]
pub struct DrawdownDurationNode {
    inner: wc::DrawdownDuration,
}

impl Default for DrawdownDurationNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl DrawdownDurationNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::DrawdownDuration::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<u32> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        prices
            .iter()
            .map(|p| self.inner.update(*p).map_or(f64::NAN, f64::from))
            .collect()
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "PainIndex")]
pub struct PainIndexNode {
    inner: wc::PainIndex,
}

#[napi]
impl PainIndexNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::PainIndex::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "ValueAtRisk")]
pub struct ValueAtRiskNode {
    inner: wc::ValueAtRisk,
}

#[napi]
impl ValueAtRiskNode {
    #[napi(constructor)]
    pub fn new(period: Count, confidence: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ValueAtRisk::new(period.get(), confidence).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "ConditionalValueAtRisk")]
pub struct ConditionalValueAtRiskNode {
    inner: wc::ConditionalValueAtRisk,
}

#[napi]
impl ConditionalValueAtRiskNode {
    #[napi(constructor)]
    pub fn new(period: Count, confidence: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ConditionalValueAtRisk::new(period.get(), confidence).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "ProfitFactor")]
pub struct ProfitFactorNode {
    inner: wc::ProfitFactor,
}

#[napi]
impl ProfitFactorNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ProfitFactor::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "GainLossRatio")]
pub struct GainLossRatioNode {
    inner: wc::GainLossRatio,
}

#[napi]
impl GainLossRatioNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::GainLossRatio::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "RecoveryFactor")]
pub struct RecoveryFactorNode {
    inner: wc::RecoveryFactor,
}

impl Default for RecoveryFactorNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl RecoveryFactorNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::RecoveryFactor::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "KellyCriterion")]
pub struct KellyCriterionNode {
    inner: wc::KellyCriterion,
}

#[napi]
impl KellyCriterionNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::KellyCriterion::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    #[napi]
    pub fn batch(&mut self, prices: Series) -> Vec<f64> {
        self.inner.batch_nan(&prices)
    }
    #[napi(js_name = "batchFast")]
    pub fn batch_fast(&mut self, prices: Series) -> Float64Array {
        Float64Array::new(self.inner.batch_fast(&prices))
    }
    #[napi(js_name = "batchInto")]
    pub fn batch_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_nan_into(&prices, rows);
        Ok(())
    }
    #[napi(js_name = "batchFastInto")]
    pub fn batch_fast_into(&mut self, prices: Series, mut out: OutSeries) -> napi::Result<()> {
        let rows = out.rows(prices.len(), &[&prices])?;
        self.inner.batch_fast_into(&prices, rows);
        Ok(())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// --- Two-series (asset, benchmark) indicators ---
//
// Family 12 (statistik-regression, PR #51) introduces a
// `node_pair_indicator!` macro for Pearson / Beta / Spearman. Family 12 is
// not yet in main, so Family 15 inlines its pair wrappers below by hand.
// When PR #51 lands, the merge conflict on this file is resolved by keeping
// the macro from Family 12 and re-using it for Treynor / IR / Alpha.

#[napi(js_name = "TreynorRatio")]
pub struct TreynorRatioNode {
    inner: wc::TreynorRatio,
}

#[napi]
impl TreynorRatioNode {
    #[napi(constructor)]
    pub fn new(period: Count, risk_free: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TreynorRatio::new(period.get(), risk_free).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, asset: f64, benchmark: f64) -> Option<f64> {
        self.inner.update((asset, benchmark))
    }
    #[napi]
    pub fn batch(&mut self, asset: Series, benchmark: Series) -> napi::Result<Vec<f64>> {
        if asset.len() != benchmark.len() {
            return Err(NapiError::from_reason(
                "asset and benchmark must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(asset.len());
        for i in 0..asset.len() {
            out.push(
                self.inner
                    .update((asset[i], benchmark[i]))
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "InformationRatio")]
pub struct InformationRatioNode {
    inner: wc::InformationRatio,
}

#[napi]
impl InformationRatioNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::InformationRatio::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, asset: f64, benchmark: f64) -> Option<f64> {
        self.inner.update((asset, benchmark))
    }
    #[napi]
    pub fn batch(&mut self, asset: Series, benchmark: Series) -> napi::Result<Vec<f64>> {
        if asset.len() != benchmark.len() {
            return Err(NapiError::from_reason(
                "asset and benchmark must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(asset.len());
        for i in 0..asset.len() {
            out.push(
                self.inner
                    .update((asset[i], benchmark[i]))
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Alt-Chart Bars ==============================
//
// Bar builders consume close prices and emit a variable number of completed bars
// per input. `update(close)` returns the bars finished on that close; `batch`
// returns all completed bars concatenated.

#[napi(object)]
pub struct RenkoBrickValue {
    pub open: f64,
    pub close: f64,
    pub direction: i32,
}

#[napi(js_name = "RenkoBars")]
pub struct RenkoBarsNode {
    inner: wc::RenkoBars,
}
#[napi]
impl RenkoBarsNode {
    #[napi(constructor)]
    pub fn new(box_size: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RenkoBars::new(box_size).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, close: f64) -> napi::Result<Vec<RenkoBrickValue>> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| RenkoBrickValue {
                open: b.open,
                close: b.close,
                direction: i32::from(b.direction),
            })
            .collect())
    }
    #[napi]
    pub fn batch(&mut self, close: Series) -> napi::Result<Vec<RenkoBrickValue>> {
        let mut out = Vec::new();
        for &price in &close {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                out.push(RenkoBrickValue {
                    open: b.open,
                    close: b.close,
                    direction: i32::from(b.direction),
                });
            }
        }
        Ok(out)
    }
    #[napi(js_name = "boxSize")]
    pub fn box_size(&self) -> f64 {
        self.inner.box_size()
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[napi(object)]
pub struct KagiSegmentValue {
    pub start: f64,
    pub end: f64,
    pub direction: i32,
}

#[napi(js_name = "KagiBars")]
pub struct KagiBarsNode {
    inner: wc::KagiBars,
}
#[napi]
impl KagiBarsNode {
    #[napi(constructor)]
    pub fn new(reversal: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::KagiBars::new(reversal).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, close: f64) -> napi::Result<Vec<KagiSegmentValue>> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| KagiSegmentValue {
                start: b.start,
                end: b.end,
                direction: i32::from(b.direction),
            })
            .collect())
    }
    #[napi]
    pub fn batch(&mut self, close: Series) -> napi::Result<Vec<KagiSegmentValue>> {
        let mut out = Vec::new();
        for &price in &close {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                out.push(KagiSegmentValue {
                    start: b.start,
                    end: b.end,
                    direction: i32::from(b.direction),
                });
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reversal(&self) -> f64 {
        self.inner.reversal()
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[napi(object)]
pub struct PnfColumnValue {
    pub direction: i32,
    pub high: f64,
    pub low: f64,
}

#[napi(js_name = "PointAndFigureBars")]
pub struct PointAndFigureBarsNode {
    inner: wc::PointAndFigureBars,
}
#[napi]
impl PointAndFigureBarsNode {
    #[napi(constructor)]
    pub fn new(box_size: f64, reversal: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::PointAndFigureBars::new(box_size, reversal.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, close: f64) -> napi::Result<Vec<PnfColumnValue>> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|col| PnfColumnValue {
                direction: i32::from(col.direction),
                high: col.high,
                low: col.low,
            })
            .collect())
    }
    #[napi]
    pub fn batch(&mut self, close: Series) -> napi::Result<Vec<PnfColumnValue>> {
        let mut out = Vec::new();
        for &price in &close {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for col in self.inner.update(candle) {
                out.push(PnfColumnValue {
                    direction: i32::from(col.direction),
                    high: col.high,
                    low: col.low,
                });
            }
        }
        Ok(out)
    }
    #[napi(js_name = "boxSize")]
    pub fn box_size(&self) -> f64 {
        self.inner.box_size()
    }
    #[napi]
    pub fn reversal(&self) -> u32 {
        self.inner.reversal() as u32
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[napi(object)]
pub struct RangeBarValue {
    pub open: f64,
    pub close: f64,
    pub direction: i32,
}

#[napi(js_name = "RangeBars")]
pub struct RangeBarsNode {
    inner: wc::RangeBars,
}
#[napi]
impl RangeBarsNode {
    #[napi(constructor)]
    pub fn new(range: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RangeBars::new(range).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, close: f64) -> napi::Result<Vec<RangeBarValue>> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| RangeBarValue {
                open: b.open,
                close: b.close,
                direction: i32::from(b.direction),
            })
            .collect())
    }
    #[napi]
    pub fn batch(&mut self, close: Series) -> napi::Result<Vec<RangeBarValue>> {
        let mut out = Vec::new();
        for &price in &close {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                out.push(RangeBarValue {
                    open: b.open,
                    close: b.close,
                    direction: i32::from(b.direction),
                });
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn range(&self) -> f64 {
        self.inner.range()
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[napi(object)]
pub struct TickBarValue {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[napi(js_name = "TickBars")]
pub struct TickBarsNode {
    inner: wc::TickBars,
}
#[napi]
impl TickBarsNode {
    #[napi(constructor)]
    pub fn new(ticks: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TickBars::new(ticks.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Vec<TickBarValue>> {
        let candle = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| TickBarValue {
                open: b.open,
                high: b.high,
                low: b.low,
                close: b.close,
                volume: b.volume,
            })
            .collect())
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<TickBarValue>> {
        if open.len() != high.len()
            || high.len() != low.len()
            || low.len() != close.len()
            || close.len() != volume.len()
        {
            return Err(NapiError::from_reason(
                "open, high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::new();
        for i in 0..open.len() {
            let candle = wc::Candle::new(open[i], high[i], low[i], close[i], volume[i], 0)
                .map_err(map_err)?;
            for b in self.inner.update(candle) {
                out.push(TickBarValue {
                    open: b.open,
                    high: b.high,
                    low: b.low,
                    close: b.close,
                    volume: b.volume,
                });
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn ticks(&self) -> u32 {
        self.inner.ticks() as u32
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[napi(object)]
pub struct VolumeBarValue {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[napi(js_name = "VolumeBars")]
pub struct VolumeBarsNode {
    inner: wc::VolumeBars,
}
#[napi]
impl VolumeBarsNode {
    #[napi(constructor)]
    pub fn new(volume_per_bar: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::VolumeBars::new(volume_per_bar).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Vec<VolumeBarValue>> {
        let candle = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| VolumeBarValue {
                open: b.open,
                high: b.high,
                low: b.low,
                close: b.close,
                volume: b.volume,
            })
            .collect())
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<VolumeBarValue>> {
        if open.len() != high.len()
            || high.len() != low.len()
            || low.len() != close.len()
            || close.len() != volume.len()
        {
            return Err(NapiError::from_reason(
                "open, high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::new();
        for i in 0..open.len() {
            let candle = wc::Candle::new(open[i], high[i], low[i], close[i], volume[i], 0)
                .map_err(map_err)?;
            for b in self.inner.update(candle) {
                out.push(VolumeBarValue {
                    open: b.open,
                    high: b.high,
                    low: b.low,
                    close: b.close,
                    volume: b.volume,
                });
            }
        }
        Ok(out)
    }
    #[napi(js_name = "volumePerBar")]
    pub fn volume_per_bar(&self) -> f64 {
        self.inner.volume_per_bar()
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[napi(object)]
pub struct DollarBarValue {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub dollar: f64,
}

#[napi(js_name = "DollarBars")]
pub struct DollarBarsNode {
    inner: wc::DollarBars,
}
#[napi]
impl DollarBarsNode {
    #[napi(constructor)]
    pub fn new(dollar_per_bar: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::DollarBars::new(dollar_per_bar).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Vec<DollarBarValue>> {
        let candle = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| DollarBarValue {
                open: b.open,
                high: b.high,
                low: b.low,
                close: b.close,
                volume: b.volume,
                dollar: b.dollar,
            })
            .collect())
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<DollarBarValue>> {
        if open.len() != high.len()
            || high.len() != low.len()
            || low.len() != close.len()
            || close.len() != volume.len()
        {
            return Err(NapiError::from_reason(
                "open, high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::new();
        for i in 0..open.len() {
            let candle = wc::Candle::new(open[i], high[i], low[i], close[i], volume[i], 0)
                .map_err(map_err)?;
            for b in self.inner.update(candle) {
                out.push(DollarBarValue {
                    open: b.open,
                    high: b.high,
                    low: b.low,
                    close: b.close,
                    volume: b.volume,
                    dollar: b.dollar,
                });
            }
        }
        Ok(out)
    }
    #[napi(js_name = "dollarPerBar")]
    pub fn dollar_per_bar(&self) -> f64 {
        self.inner.dollar_per_bar()
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[napi(object)]
pub struct ImbalanceBarValue {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub imbalance: f64,
    pub direction: i32,
}

#[napi(js_name = "ImbalanceBars")]
pub struct ImbalanceBarsNode {
    inner: wc::ImbalanceBars,
}
#[napi]
impl ImbalanceBarsNode {
    #[napi(constructor)]
    pub fn new(threshold: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ImbalanceBars::new(threshold).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Vec<ImbalanceBarValue>> {
        let candle = wc::Candle::new(open, high, low, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| ImbalanceBarValue {
                open: b.open,
                high: b.high,
                low: b.low,
                close: b.close,
                imbalance: b.imbalance,
                direction: i32::from(b.direction),
            })
            .collect())
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<ImbalanceBarValue>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::new();
        for i in 0..open.len() {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                out.push(ImbalanceBarValue {
                    open: b.open,
                    high: b.high,
                    low: b.low,
                    close: b.close,
                    imbalance: b.imbalance,
                    direction: i32::from(b.direction),
                });
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn threshold(&self) -> f64 {
        self.inner.threshold()
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[napi(object)]
pub struct RunBarValue {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub length: u32,
    pub direction: i32,
}

#[napi(js_name = "RunBars")]
pub struct RunBarsNode {
    inner: wc::RunBars,
}
#[napi]
impl RunBarsNode {
    #[napi(constructor)]
    pub fn new(run_length: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::RunBars::new(run_length.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> napi::Result<Vec<RunBarValue>> {
        let candle = wc::Candle::new(open, high, low, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| RunBarValue {
                open: b.open,
                high: b.high,
                low: b.low,
                close: b.close,
                length: b.length as u32,
                direction: i32::from(b.direction),
            })
            .collect())
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
    ) -> napi::Result<Vec<RunBarValue>> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "open, high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::new();
        for i in 0..open.len() {
            let candle =
                wc::Candle::new(open[i], high[i], low[i], close[i], 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                out.push(RunBarValue {
                    open: b.open,
                    high: b.high,
                    low: b.low,
                    close: b.close,
                    length: b.length as u32,
                    direction: i32::from(b.direction),
                });
            }
        }
        Ok(out)
    }
    #[napi(js_name = "runLength")]
    pub fn run_length(&self) -> u32 {
        self.inner.run_length() as u32
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[napi(object)]
pub struct LineBreakBarValue {
    pub open: f64,
    pub close: f64,
    pub direction: i32,
}

#[napi(js_name = "ThreeLineBreakBars")]
pub struct ThreeLineBreakBarsNode {
    inner: wc::ThreeLineBreakBars,
}
#[napi]
impl ThreeLineBreakBarsNode {
    #[napi(constructor)]
    pub fn new(lines: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::ThreeLineBreakBars::new(lines.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, close: f64) -> napi::Result<Vec<LineBreakBarValue>> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| LineBreakBarValue {
                open: b.open,
                close: b.close,
                direction: i32::from(b.direction),
            })
            .collect())
    }
    #[napi]
    pub fn batch(&mut self, close: Series) -> napi::Result<Vec<LineBreakBarValue>> {
        let mut out = Vec::new();
        for &price in &close {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                out.push(LineBreakBarValue {
                    open: b.open,
                    close: b.close,
                    direction: i32::from(b.direction),
                });
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn lines(&self) -> u32 {
        self.inner.lines() as u32
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[napi(js_name = "Alpha")]
pub struct AlphaNode {
    inner: wc::Alpha,
}

#[napi]
impl AlphaNode {
    #[napi(constructor)]
    pub fn new(period: Count, risk_free: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::Alpha::new(period.get(), risk_free).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, asset: f64, benchmark: f64) -> Option<f64> {
        self.inner.update((asset, benchmark))
    }
    #[napi]
    pub fn batch(&mut self, asset: Series, benchmark: Series) -> napi::Result<Vec<f64>> {
        if asset.len() != benchmark.len() {
            return Err(NapiError::from_reason(
                "asset and benchmark must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(asset.len());
        for i in 0..asset.len() {
            out.push(
                self.inner
                    .update((asset[i], benchmark[i]))
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ====================== Seasonality & Session (full-candle) ======================
//
// These read the wall-clock fields of `Candle::timestamp`, so the bindings take
// the full candle (open, high, low, close, volume, timestamp) rather than the
// high/low/close slice used by the candle indicators above.

fn season_candles(
    open: &[f64],
    high: &[f64],
    low: &[f64],
    close: &[f64],
    volume: &[f64],
    timestamp: &[i64],
) -> napi::Result<Vec<wc::Candle>> {
    let n = open.len();
    if [
        high.len(),
        low.len(),
        close.len(),
        volume.len(),
        timestamp.len(),
    ]
    .iter()
    .any(|&x| x != n)
    {
        return Err(NapiError::from_reason(
            "open, high, low, close, volume, timestamp must be equal length".to_string(),
        ));
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(
            wc::Candle::new(open[i], high[i], low[i], close[i], volume[i], timestamp[i])
                .map_err(map_err)?,
        );
    }
    Ok(out)
}

macro_rules! node_seasonality_offset_scalar {
    ($wrapper:ident, $node_name:literal, $rust_ty:ty) => {
        #[napi(js_name = $node_name)]
        pub struct $wrapper {
            inner: $rust_ty,
        }
        #[napi]
        impl $wrapper {
            #[napi(constructor)]
            pub fn new(utc_offset_minutes: i32) -> Self {
                Self {
                    inner: <$rust_ty>::new(utc_offset_minutes),
                }
            }
            #[napi]
            pub fn update(
                &mut self,
                open: f64,
                high: f64,
                low: f64,
                close: f64,
                volume: f64,
                timestamp: i64,
            ) -> napi::Result<Option<f64>> {
                Ok(self.inner.update(
                    wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?,
                ))
            }
            #[napi]
            pub fn batch(
                &mut self,
                open: Series,
                high: Series,
                low: Series,
                close: Series,
                volume: Series,
                timestamp: Vec<i64>,
            ) -> napi::Result<Vec<f64>> {
                let candles = season_candles(&open, &high, &low, &close, &volume, &timestamp)?;
                Ok(candles
                    .into_iter()
                    .map(|c| self.inner.update(c).unwrap_or(f64::NAN))
                    .collect())
            }
            #[napi]
            pub fn reset(&mut self) {
                self.inner.reset();
            }

            #[napi]
            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[napi(js_name = "isReady")]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[napi(js_name = "warmupPeriod")]
            pub fn warmup_period(&self) -> u32 {
                self.inner.warmup_period() as u32
            }
            #[napi(js_name = "utcOffsetMinutes")]
            pub fn utc_offset_minutes(&self) -> i32 {
                self.inner.utc_offset_minutes()
            }
        }
    };
}

macro_rules! node_seasonality_bucket_profile {
    ($wrapper:ident, $node_name:literal, $rust_ty:ty) => {
        #[napi(js_name = $node_name)]
        pub struct $wrapper {
            inner: $rust_ty,
        }
        #[napi]
        impl $wrapper {
            #[napi(constructor)]
            pub fn new(buckets: Count, utc_offset_minutes: i32) -> napi::Result<Self> {
                Ok(Self {
                    inner: <$rust_ty>::new(buckets.get(), utc_offset_minutes).map_err(map_err)?,
                })
            }
            #[napi]
            pub fn update(
                &mut self,
                open: f64,
                high: f64,
                low: f64,
                close: f64,
                volume: f64,
                timestamp: i64,
            ) -> napi::Result<Option<Vec<f64>>> {
                Ok(self
                    .inner
                    .update(
                        wc::Candle::new(open, high, low, close, volume, timestamp)
                            .map_err(map_err)?,
                    )
                    .map(|o| o.bins))
            }
            #[napi]
            pub fn batch(
                &mut self,
                open: Series,
                high: Series,
                low: Series,
                close: Series,
                volume: Series,
                timestamp: Vec<i64>,
            ) -> napi::Result<Vec<f64>> {
                let candles = season_candles(&open, &high, &low, &close, &volume, &timestamp)?;
                let k = self.inner.params().0;
                let n = candles.len();
                let mut out = vec![f64::NAN; n * k];
                for (i, c) in candles.into_iter().enumerate() {
                    if let Some(o) = self.inner.update(c) {
                        for (j, b) in o.bins.iter().enumerate() {
                            out[i * k + j] = *b;
                        }
                    }
                }
                Ok(out)
            }
            #[napi]
            pub fn reset(&mut self) {
                self.inner.reset();
            }

            #[napi]
            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[napi(js_name = "isReady")]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[napi(js_name = "warmupPeriod")]
            pub fn warmup_period(&self) -> u32 {
                self.inner.warmup_period() as u32
            }
            #[napi(js_name = "buckets")]
            pub fn buckets(&self) -> u32 {
                self.inner.params().0 as u32
            }
            #[napi(js_name = "utcOffsetMinutes")]
            pub fn utc_offset_minutes(&self) -> i32 {
                self.inner.params().1
            }
        }
    };
}

macro_rules! node_seasonality_offset_profile {
    ($wrapper:ident, $node_name:literal, $rust_ty:ty, $k:expr) => {
        #[napi(js_name = $node_name)]
        pub struct $wrapper {
            inner: $rust_ty,
        }
        #[napi]
        impl $wrapper {
            #[napi(constructor)]
            pub fn new(utc_offset_minutes: i32) -> Self {
                Self {
                    inner: <$rust_ty>::new(utc_offset_minutes),
                }
            }
            #[napi]
            pub fn update(
                &mut self,
                open: f64,
                high: f64,
                low: f64,
                close: f64,
                volume: f64,
                timestamp: i64,
            ) -> napi::Result<Option<Vec<f64>>> {
                Ok(self
                    .inner
                    .update(
                        wc::Candle::new(open, high, low, close, volume, timestamp)
                            .map_err(map_err)?,
                    )
                    .map(|o| o.bins))
            }
            #[napi]
            pub fn batch(
                &mut self,
                open: Series,
                high: Series,
                low: Series,
                close: Series,
                volume: Series,
                timestamp: Vec<i64>,
            ) -> napi::Result<Vec<f64>> {
                let candles = season_candles(&open, &high, &low, &close, &volume, &timestamp)?;
                let k = $k;
                let n = candles.len();
                let mut out = vec![f64::NAN; n * k];
                for (i, c) in candles.into_iter().enumerate() {
                    if let Some(o) = self.inner.update(c) {
                        for (j, b) in o.bins.iter().enumerate() {
                            out[i * k + j] = *b;
                        }
                    }
                }
                Ok(out)
            }
            #[napi]
            pub fn reset(&mut self) {
                self.inner.reset();
            }

            #[napi]
            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[napi(js_name = "isReady")]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[napi(js_name = "warmupPeriod")]
            pub fn warmup_period(&self) -> u32 {
                self.inner.warmup_period() as u32
            }
            #[napi(js_name = "utcOffsetMinutes")]
            pub fn utc_offset_minutes(&self) -> i32 {
                self.inner.utc_offset_minutes()
            }
        }
    };
}

node_seasonality_offset_scalar!(SessionVwapNode, "SessionVwap", wc::SessionVwap);
node_seasonality_offset_scalar!(OvernightGapNode, "OvernightGap", wc::OvernightGap);
node_seasonality_offset_scalar!(SeasonalZScoreNode, "SeasonalZScore", wc::SeasonalZScore);
node_seasonality_bucket_profile!(
    TimeOfDayReturnProfileNode,
    "TimeOfDayReturnProfile",
    wc::TimeOfDayReturnProfile
);
node_seasonality_bucket_profile!(
    IntradayVolatilityProfileNode,
    "IntradayVolatilityProfile",
    wc::IntradayVolatilityProfile
);
node_seasonality_bucket_profile!(
    VolumeByTimeProfileNode,
    "VolumeByTimeProfile",
    wc::VolumeByTimeProfile
);
node_seasonality_offset_profile!(
    DayOfWeekProfileNode,
    "DayOfWeekProfile",
    wc::DayOfWeekProfile,
    7
);

#[napi(js_name = "AverageDailyRange")]
pub struct AverageDailyRangeNode {
    inner: wc::AverageDailyRange,
}
#[napi]
impl AverageDailyRangeNode {
    #[napi(constructor)]
    pub fn new(period: Count, utc_offset_minutes: i32) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::AverageDailyRange::new(period.get(), utc_offset_minutes).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
        timestamp: Vec<i64>,
    ) -> napi::Result<Vec<f64>> {
        let candles = season_candles(&open, &high, &low, &close, &volume, &timestamp)?;
        Ok(candles
            .into_iter()
            .map(|c| self.inner.update(c).unwrap_or(f64::NAN))
            .collect())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(js_name = "TurnOfMonth")]
pub struct TurnOfMonthNode {
    inner: wc::TurnOfMonth,
}
#[napi]
impl TurnOfMonthNode {
    #[napi(constructor)]
    pub fn new(n_first: Count, n_last: Count, utc_offset_minutes: i32) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TurnOfMonth::new(
                narrow(n_first, "n_first")?,
                narrow(n_last, "n_last")?,
                utc_offset_minutes,
            )
            .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> napi::Result<Option<f64>> {
        Ok(self
            .inner
            .update(wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
        timestamp: Vec<i64>,
    ) -> napi::Result<Vec<f64>> {
        let candles = season_candles(&open, &high, &low, &close, &volume, &timestamp)?;
        Ok(candles
            .into_iter()
            .map(|c| self.inner.update(c).unwrap_or(f64::NAN))
            .collect())
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct SessionHighLowValue {
    pub high: f64,
    pub low: f64,
}

#[napi(js_name = "SessionHighLow")]
pub struct SessionHighLowNode {
    inner: wc::SessionHighLow,
}
#[napi]
impl SessionHighLowNode {
    #[napi(constructor)]
    pub fn new(utc_offset_minutes: i32) -> Self {
        Self {
            inner: wc::SessionHighLow::new(utc_offset_minutes),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> napi::Result<Option<SessionHighLowValue>> {
        Ok(self
            .inner
            .update(wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?)
            .map(|o| SessionHighLowValue {
                high: o.high,
                low: o.low,
            }))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
        timestamp: Vec<i64>,
    ) -> napi::Result<Vec<f64>> {
        let candles = season_candles(&open, &high, &low, &close, &volume, &timestamp)?;
        let n = candles.len();
        let mut out = vec![f64::NAN; n * 2];
        for (i, c) in candles.into_iter().enumerate() {
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.high;
                out[i * 2 + 1] = o.low;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct SessionRangeValue {
    pub asia: f64,
    pub eu: f64,
    pub us: f64,
}

#[napi(js_name = "SessionRange")]
pub struct SessionRangeNode {
    inner: wc::SessionRange,
}
#[napi]
impl SessionRangeNode {
    #[napi(constructor)]
    pub fn new(utc_offset_minutes: i32) -> Self {
        Self {
            inner: wc::SessionRange::new(utc_offset_minutes),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> napi::Result<Option<SessionRangeValue>> {
        Ok(self
            .inner
            .update(wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?)
            .map(|o| SessionRangeValue {
                asia: o.asia,
                eu: o.eu,
                us: o.us,
            }))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
        timestamp: Vec<i64>,
    ) -> napi::Result<Vec<f64>> {
        let candles = season_candles(&open, &high, &low, &close, &volume, &timestamp)?;
        let n = candles.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, c) in candles.into_iter().enumerate() {
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.asia;
                out[i * 3 + 1] = o.eu;
                out[i * 3 + 2] = o.us;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct OvernightIntradayReturnValue {
    pub overnight: f64,
    pub intraday: f64,
}

#[napi(js_name = "OvernightIntradayReturn")]
pub struct OvernightIntradayReturnNode {
    inner: wc::OvernightIntradayReturn,
}
#[napi]
impl OvernightIntradayReturnNode {
    #[napi(constructor)]
    pub fn new(utc_offset_minutes: i32) -> Self {
        Self {
            inner: wc::OvernightIntradayReturn::new(utc_offset_minutes),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> napi::Result<Option<OvernightIntradayReturnValue>> {
        Ok(self
            .inner
            .update(wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?)
            .map(|o| OvernightIntradayReturnValue {
                overnight: o.overnight,
                intraday: o.intraday,
            }))
    }
    #[napi]
    pub fn batch(
        &mut self,
        open: Series,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
        timestamp: Vec<i64>,
    ) -> napi::Result<Vec<f64>> {
        let candles = season_candles(&open, &high, &low, &close, &volume, &timestamp)?;
        let n = candles.len();
        let mut out = vec![f64::NAN; n * 2];
        for (i, c) in candles.into_iter().enumerate() {
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.overnight;
                out[i * 2 + 1] = o.intraday;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Fibonacci ==============================

/// Build a candle for the swing-based Fibonacci tools from a `high`/`low` pair;
/// only the high and low drive the swing tracker, so open/close are the midpoint.
fn swing_cnd(high: f64, low: f64) -> napi::Result<wc::Candle> {
    cnd(high, low, f64::midpoint(high, low), 0.0)
}

#[napi(object)]
pub struct FibRetracementValue {
    pub level_0: f64,
    pub level_236: f64,
    pub level_382: f64,
    pub level_500: f64,
    pub level_618: f64,
    pub level_786: f64,
    pub level_1000: f64,
}

#[napi(js_name = "FibRetracement")]
pub struct FibRetracementNode {
    inner: wc::FibRetracement,
}

#[napi]
impl FibRetracementNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::FibRetracement::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<FibRetracementValue>> {
        Ok(self
            .inner
            .update(swing_cnd(high, low)?)
            .map(|o| FibRetracementValue {
                level_0: o.level_0,
                level_236: o.level_236,
                level_382: o.level_382,
                level_500: o.level_500,
                level_618: o.level_618,
                level_786: o.level_786,
                level_1000: o.level_1000,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 7];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_cnd(high[i], low[i])?) {
                out[i * 7] = o.level_0;
                out[i * 7 + 1] = o.level_236;
                out[i * 7 + 2] = o.level_382;
                out[i * 7 + 3] = o.level_500;
                out[i * 7 + 4] = o.level_618;
                out[i * 7 + 5] = o.level_786;
                out[i * 7 + 6] = o.level_1000;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct FibExtensionValue {
    pub level_1272: f64,
    pub level_1414: f64,
    pub level_1618: f64,
    pub level_2000: f64,
    pub level_2618: f64,
}

#[napi(js_name = "FibExtension")]
pub struct FibExtensionNode {
    inner: wc::FibExtension,
}

#[napi]
impl FibExtensionNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::FibExtension::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<FibExtensionValue>> {
        Ok(self
            .inner
            .update(swing_cnd(high, low)?)
            .map(|o| FibExtensionValue {
                level_1272: o.level_1272,
                level_1414: o.level_1414,
                level_1618: o.level_1618,
                level_2000: o.level_2000,
                level_2618: o.level_2618,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 5];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_cnd(high[i], low[i])?) {
                out[i * 5] = o.level_1272;
                out[i * 5 + 1] = o.level_1414;
                out[i * 5 + 2] = o.level_1618;
                out[i * 5 + 3] = o.level_2000;
                out[i * 5 + 4] = o.level_2618;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct FibProjectionValue {
    pub level_618: f64,
    pub level_1000: f64,
    pub level_1618: f64,
    pub level_2618: f64,
}

#[napi(js_name = "FibProjection")]
pub struct FibProjectionNode {
    inner: wc::FibProjection,
}

#[napi]
impl FibProjectionNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::FibProjection::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<FibProjectionValue>> {
        Ok(self
            .inner
            .update(swing_cnd(high, low)?)
            .map(|o| FibProjectionValue {
                level_618: o.level_618,
                level_1000: o.level_1000,
                level_1618: o.level_1618,
                level_2618: o.level_2618,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_cnd(high[i], low[i])?) {
                out[i * 4] = o.level_618;
                out[i * 4 + 1] = o.level_1000;
                out[i * 4 + 2] = o.level_1618;
                out[i * 4 + 3] = o.level_2618;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct AutoFibValue {
    pub level_0: f64,
    pub level_236: f64,
    pub level_382: f64,
    pub level_500: f64,
    pub level_618: f64,
    pub level_786: f64,
    pub level_1000: f64,
}

#[napi(js_name = "AutoFib")]
pub struct AutoFibNode {
    inner: wc::AutoFib,
}

#[napi]
impl AutoFibNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::AutoFib::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<AutoFibValue>> {
        Ok(self
            .inner
            .update(swing_cnd(high, low)?)
            .map(|o| AutoFibValue {
                level_0: o.level_0,
                level_236: o.level_236,
                level_382: o.level_382,
                level_500: o.level_500,
                level_618: o.level_618,
                level_786: o.level_786,
                level_1000: o.level_1000,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 7];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_cnd(high[i], low[i])?) {
                out[i * 7] = o.level_0;
                out[i * 7 + 1] = o.level_236;
                out[i * 7 + 2] = o.level_382;
                out[i * 7 + 3] = o.level_500;
                out[i * 7 + 4] = o.level_618;
                out[i * 7 + 5] = o.level_786;
                out[i * 7 + 6] = o.level_1000;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct GoldenPocketValue {
    pub low: f64,
    pub mid: f64,
    pub high: f64,
}

#[napi(js_name = "GoldenPocket")]
pub struct GoldenPocketNode {
    inner: wc::GoldenPocket,
}

#[napi]
impl GoldenPocketNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::GoldenPocket::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<GoldenPocketValue>> {
        Ok(self
            .inner
            .update(swing_cnd(high, low)?)
            .map(|o| GoldenPocketValue {
                low: o.low,
                mid: o.mid,
                high: o.high,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_cnd(high[i], low[i])?) {
                out[i * 3] = o.low;
                out[i * 3 + 1] = o.mid;
                out[i * 3 + 2] = o.high;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

#[napi(object)]
pub struct FibConfluenceValue {
    pub price: f64,
    pub strength: f64,
}

#[napi(js_name = "FibConfluence")]
pub struct FibConfluenceNode {
    inner: wc::FibConfluence,
}

#[napi]
impl FibConfluenceNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::FibConfluence::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<FibConfluenceValue>> {
        Ok(self
            .inner
            .update(swing_cnd(high, low)?)
            .map(|o| FibConfluenceValue {
                price: o.price,
                strength: o.strength,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_cnd(high[i], low[i])?) {
                out[i * 2] = o.price;
                out[i * 2 + 1] = o.strength;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for FibRetracementNode {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for FibExtensionNode {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for FibProjectionNode {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for AutoFibNode {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for GoldenPocketNode {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for FibConfluenceNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi(object)]
pub struct FibFanValue {
    pub fan_382: f64,
    pub fan_500: f64,
    pub fan_618: f64,
}

#[napi(js_name = "FibFan")]
pub struct FibFanNode {
    inner: wc::FibFan,
}

#[napi]
impl FibFanNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::FibFan::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<FibFanValue>> {
        Ok(self
            .inner
            .update(swing_cnd(high, low)?)
            .map(|o| FibFanValue {
                fan_382: o.fan_382,
                fan_500: o.fan_500,
                fan_618: o.fan_618,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_cnd(high[i], low[i])?) {
                out[i * 3] = o.fan_382;
                out[i * 3 + 1] = o.fan_500;
                out[i * 3 + 2] = o.fan_618;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for FibFanNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi(object)]
pub struct FibArcsValue {
    pub arc_382: f64,
    pub arc_500: f64,
    pub arc_618: f64,
}

#[napi(js_name = "FibArcs")]
pub struct FibArcsNode {
    inner: wc::FibArcs,
}

#[napi]
impl FibArcsNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::FibArcs::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<FibArcsValue>> {
        Ok(self
            .inner
            .update(swing_cnd(high, low)?)
            .map(|o| FibArcsValue {
                arc_382: o.arc_382,
                arc_500: o.arc_500,
                arc_618: o.arc_618,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_cnd(high[i], low[i])?) {
                out[i * 3] = o.arc_382;
                out[i * 3 + 1] = o.arc_500;
                out[i * 3 + 2] = o.arc_618;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for FibArcsNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi(object)]
pub struct FibChannelValue {
    pub base: f64,
    pub level_618: f64,
    pub level_1000: f64,
    pub level_1618: f64,
}

#[napi(js_name = "FibChannel")]
pub struct FibChannelNode {
    inner: wc::FibChannel,
}

#[napi]
impl FibChannelNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::FibChannel::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<FibChannelValue>> {
        Ok(self
            .inner
            .update(swing_cnd(high, low)?)
            .map(|o| FibChannelValue {
                base: o.base,
                level_618: o.level_618,
                level_1000: o.level_1000,
                level_1618: o.level_1618,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_cnd(high[i], low[i])?) {
                out[i * 4] = o.base;
                out[i * 4 + 1] = o.level_618;
                out[i * 4 + 2] = o.level_1000;
                out[i * 4 + 3] = o.level_1618;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for FibChannelNode {
    fn default() -> Self {
        Self::new()
    }
}

#[napi(object)]
pub struct FibTimeZonesValue {
    pub on_zone: f64,
    pub bars_to_next: f64,
}

#[napi(js_name = "FibTimeZones")]
pub struct FibTimeZonesNode {
    inner: wc::FibTimeZones,
}

#[napi]
impl FibTimeZonesNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::FibTimeZones::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64) -> napi::Result<Option<FibTimeZonesValue>> {
        Ok(self
            .inner
            .update(swing_cnd(high, low)?)
            .map(|o| FibTimeZonesValue {
                on_zone: o.on_zone,
                bars_to_next: o.bars_to_next,
            }))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() {
            return Err(NapiError::from_reason(
                "high and low must be equal length".to_string(),
            ));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_cnd(high[i], low[i])?) {
                out[i * 2] = o.on_zone;
                out[i * 2 + 1] = o.bars_to_next;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for FibTimeZonesNode {
    fn default() -> Self {
        Self::new()
    }
}

// ============================== Volume RSI ==============================

#[napi(js_name = "VolumeRsi")]
pub struct VolumeRsiNode {
    inner: wc::VolumeRsi,
}

#[napi]
impl VolumeRsiNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::VolumeRsi::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, close: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(close, close, close, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "close and volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(close[i], close[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Williams A/D ==============================

#[napi(js_name = "Wad")]
pub struct WadNode {
    inner: wc::Wad,
}

#[napi]
impl WadNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::Wad::new(),
        }
    }
    #[napi]
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, 0.0)?))
    }
    #[napi]
    pub fn batch(&mut self, high: Series, low: Series, close: Series) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(NapiError::from_reason(
                "high, low, close must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], 0.0)?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for WadNode {
    fn default() -> Self {
        Self::new()
    }
}

// ============================== Twiggs Money Flow ==============================

#[napi(js_name = "TwiggsMoneyFlow")]
pub struct TwiggsMoneyFlowNode {
    inner: wc::TwiggsMoneyFlow,
}

#[napi]
impl TwiggsMoneyFlowNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TwiggsMoneyFlow::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Trade Volume Index ==============================

#[napi(js_name = "TradeVolumeIndex")]
pub struct TradeVolumeIndexNode {
    inner: wc::TradeVolumeIndex,
}

#[napi]
impl TradeVolumeIndexNode {
    #[napi(constructor)]
    pub fn new(min_tick: f64) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::TradeVolumeIndex::new(min_tick).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(&mut self, close: f64, volume: f64) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(close, close, close, volume)?))
    }
    #[napi]
    pub fn batch(&mut self, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "close and volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(close[i], close[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Intraday Intensity ==============================

#[napi(js_name = "IntradayIntensity")]
pub struct IntradayIntensityNode {
    inner: wc::IntradayIntensity,
}

#[napi]
impl IntradayIntensityNode {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: wc::IntradayIntensity::new(),
        }
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

impl Default for IntradayIntensityNode {
    fn default() -> Self {
        Self::new()
    }
}

// ============================== Better Volume ==============================

#[napi(js_name = "BetterVolume")]
pub struct BetterVolumeNode {
    inner: wc::BetterVolume,
}

#[napi]
impl BetterVolumeNode {
    #[napi(constructor)]
    pub fn new(period: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::BetterVolume::new(period.get()).map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<f64>> {
        Ok(self.inner.update(cnd(high, low, close, volume)?))
    }
    #[napi]
    pub fn batch(
        &mut self,
        high: Series,
        low: Series,
        close: Series,
        volume: Series,
    ) -> napi::Result<Vec<f64>> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "high, low, close, volume must be equal length".to_string(),
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(cnd(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ============================== Volume-Weighted MACD ==============================

#[napi(object)]
pub struct VolumeWeightedMacdValue {
    pub macd: f64,
    pub signal: f64,
    pub histogram: f64,
}

#[napi(js_name = "VolumeWeightedMacd")]
pub struct VolumeWeightedMacdNode {
    inner: wc::VolumeWeightedMacd,
}

#[napi]
impl VolumeWeightedMacdNode {
    #[napi(constructor)]
    pub fn new(fast: Count, slow: Count, signal: Count) -> napi::Result<Self> {
        Ok(Self {
            inner: wc::VolumeWeightedMacd::new(fast.get(), slow.get(), signal.get())
                .map_err(map_err)?,
        })
    }
    #[napi]
    pub fn update(
        &mut self,
        close: f64,
        volume: f64,
    ) -> napi::Result<Option<VolumeWeightedMacdValue>> {
        Ok(self
            .inner
            .update(cnd(close, close, close, volume)?)
            .map(|o| VolumeWeightedMacdValue {
                macd: o.macd,
                signal: o.signal,
                histogram: o.histogram,
            }))
    }
    /// Returns `[macd0, signal0, histogram0, macd1, ...]`, length `3 * n`.
    /// Warmup positions are `NaN`.
    #[napi]
    pub fn batch(&mut self, close: Series, volume: Series) -> napi::Result<Vec<f64>> {
        if close.len() != volume.len() {
            return Err(NapiError::from_reason(
                "close and volume must be equal length".to_string(),
            ));
        }
        let mut out = vec![f64::NAN; close.len() * 3];
        for i in 0..close.len() {
            if let Some(o) = self
                .inner
                .update(cnd(close[i], close[i], close[i], volume[i])?)
            {
                out[i * 3] = o.macd;
                out[i * 3 + 1] = o.signal;
                out[i * 3 + 2] = o.histogram;
            }
        }
        Ok(out)
    }
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[napi]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[napi(js_name = "isReady")]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[napi(js_name = "warmupPeriod")]
    pub fn warmup_period(&self) -> u32 {
        self.inner.warmup_period() as u32
    }
}

// ===== Data layer: tick-to-candle aggregation =====

/// Convert a `wickra-data` error into a JS error.
fn map_data_err(e: wickra_data::Error) -> NapiError {
    NapiError::new(Status::InvalidArg, e.to_string())
}

/// One aggregated OHLCV candle emitted by [`TickAggregatorNode`].
#[napi(object)]
pub struct CandleValue {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub timestamp: f64,
}

/// One event from the live Binance feed.
#[napi(object)]
pub struct KlineEvent {
    pub symbol: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub open_time: f64,
    pub is_closed: bool,
}

/// Map a `u8` interval code (`0..=15`, the `Interval` declaration order) to the
/// feed interval.
fn binance_interval(code: u8) -> Option<wickra_data::live::binance::Interval> {
    use wickra_data::live::binance::Interval;
    Some(match code {
        0 => Interval::OneSecond,
        1 => Interval::OneMinute,
        2 => Interval::ThreeMinutes,
        3 => Interval::FiveMinutes,
        4 => Interval::FifteenMinutes,
        5 => Interval::ThirtyMinutes,
        6 => Interval::OneHour,
        7 => Interval::TwoHours,
        8 => Interval::FourHours,
        9 => Interval::SixHours,
        10 => Interval::EightHours,
        11 => Interval::TwelveHours,
        12 => Interval::OneDay,
        13 => Interval::ThreeDays,
        14 => Interval::OneWeek,
        15 => Interval::OneMonth,
        _ => return None,
    })
}

/// A live Binance kline feed. `next` is a blocking poll (it waits up to the
/// timeout on the calling thread) driving the mock-server-tested wickra-data
/// `BinanceKlineStream` on a single-thread tokio runtime — use a short timeout in
/// a loop, or run it on a Worker thread, to keep the event loop responsive.
#[napi(js_name = "BinanceFeed")]
pub struct BinanceFeedNode {
    runtime: tokio::runtime::Runtime,
    inner: wickra_data::live::binance::BinanceKlineStream,
}

#[napi]
impl BinanceFeedNode {
    /// Connect to Binance's live kline stream for the given comma-separated
    /// `symbols` (case-insensitive) at the given `interval` code (`0..=15`).
    /// `baseUrl` overrides the endpoint (omit for production; pass a `ws://` URL
    /// to target a test server).
    #[napi(constructor)]
    pub fn new(symbols: String, interval: u8, base_url: Option<String>) -> napi::Result<Self> {
        let iv = binance_interval(interval).ok_or_else(|| {
            NapiError::new(
                Status::InvalidArg,
                "unknown interval code (expected 0..=15)",
            )
        })?;
        let symbol_list: Vec<String> = symbols
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        if symbol_list.is_empty() {
            return Err(NapiError::new(
                Status::InvalidArg,
                "at least one symbol is required",
            ));
        }
        let mut config = wickra_data::live::binance::BinanceConfig::default();
        if let Some(url) = base_url {
            config.base_url = url;
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| NapiError::new(Status::GenericFailure, e.to_string()))?;
        let inner = runtime
            .block_on(
                wickra_data::live::binance::BinanceKlineStream::connect_with_config(
                    &symbol_list,
                    iv,
                    config,
                ),
            )
            .map_err(map_data_err)?;
        Ok(Self { runtime, inner })
    }

    /// Poll for the next kline event, waiting up to `timeoutMs` milliseconds.
    /// Returns the event, or `null` on timeout (call again). Throws once the
    /// stream is closed.
    #[napi]
    pub fn next(&mut self, timeout_ms: f64) -> napi::Result<Option<KlineEvent>> {
        let dur = std::time::Duration::from_millis(timeout_ms.max(0.0) as u64);
        let runtime = &self.runtime;
        let inner = &mut self.inner;
        let result = runtime.block_on(tokio::time::timeout(dur, inner.next_event()));
        match result {
            Ok(Ok(Some(ev))) => Ok(Some(KlineEvent {
                symbol: ev.symbol,
                open: ev.candle.open,
                high: ev.candle.high,
                low: ev.candle.low,
                close: ev.candle.close,
                volume: ev.candle.volume,
                #[allow(clippy::cast_precision_loss)]
                open_time: ev.candle.timestamp as f64,
                is_closed: ev.is_closed,
            })),
            Ok(Ok(None) | Err(_)) => Err(NapiError::new(
                Status::GenericFailure,
                "binance feed closed",
            )),
            Err(_) => Ok(None),
        }
    }

    /// Close the stream; subsequent `next` calls throw.
    #[napi]
    pub fn close(&mut self) {
        let runtime = &self.runtime;
        let inner = &mut self.inner;
        let _ = runtime.block_on(inner.close());
    }
}

/// Fetch historical klines from Binance's REST endpoint. `symbol` is the trading
/// pair (case-insensitive, e.g. `"BTCUSDT"`), `interval` the code `0..=15` (the
/// `Interval` declaration order), and `limit` the number of candles to request
/// (`1..=1000`). `startMs`/`endMs` are optional inclusive Unix-millisecond
/// bounds; `baseUrl` overrides the host (omit for production). This is a blocking
/// call — run it on a Worker thread to keep the event loop responsive.
#[napi(js_name = "fetchBinanceKlines")]
pub fn fetch_binance_klines(
    symbol: String,
    interval: u8,
    limit: Count,
    start_ms: Option<f64>,
    end_ms: Option<f64>,
    base_url: Option<String>,
) -> napi::Result<Vec<CandleValue>> {
    let iv = binance_interval(interval).ok_or_else(|| {
        NapiError::new(
            Status::InvalidArg,
            "unknown interval code (expected 0..=15)",
        )
    })?;
    let limit = u16::try_from(limit.get())
        .map_err(|_| NapiError::new(Status::InvalidArg, "limit must be in 1..=1000"))?;
    let start = start_ms.map(|v| v as i64);
    let end = end_ms.map(|v| v as i64);
    let candles = match base_url {
        Some(url) => {
            let config = wickra_data::live::binance_rest::BinanceRestConfig { base_url: url };
            wickra_data::live::binance_rest::fetch_klines_with_config(
                &symbol, iv, limit, start, end, &config,
            )
        }
        None => wickra_data::live::binance_rest::fetch_klines(&symbol, iv, limit, start, end),
    }
    .map_err(map_data_err)?;
    Ok(candles.into_iter().map(candle_to_value).collect())
}

/// Roll trade ticks up into fixed-timeframe OHLCV candles.
#[napi(js_name = "TickAggregator")]
pub struct TickAggregatorNode {
    inner: wickra_data::aggregator::TickAggregator,
}

#[napi]
impl TickAggregatorNode {
    /// Construct an aggregator with the given bucket size (in the same unit as
    /// the tick timestamps). Pass `gapFill = true` to emit a flat placeholder
    /// candle for every skipped bucket.
    #[napi(constructor)]
    pub fn new(bucket: f64, gap_fill: Option<bool>) -> napi::Result<Self> {
        let timeframe =
            wickra_data::aggregator::Timeframe::new(bucket as i64).map_err(map_data_err)?;
        let mut inner = wickra_data::aggregator::TickAggregator::new(timeframe);
        if gap_fill.unwrap_or(false) {
            inner = inner.with_gap_fill(true);
        }
        Ok(Self { inner })
    }

    /// Push one trade tick; returns every candle that closed as a result (none
    /// while the open bar keeps growing, one on a bucket boundary, plus one flat
    /// candle per skipped bucket when gap filling is enabled).
    #[napi]
    pub fn push(
        &mut self,
        price: f64,
        size: f64,
        timestamp: f64,
    ) -> napi::Result<Vec<CandleValue>> {
        let tick = wc::Tick::new(price, size, timestamp as i64).map_err(map_err)?;
        let candles = self.inner.push(tick).map_err(map_data_err)?;
        Ok(candles
            .into_iter()
            .map(|c| CandleValue {
                open: c.open,
                high: c.high,
                low: c.low,
                close: c.close,
                volume: c.volume,
                timestamp: c.timestamp as f64,
            })
            .collect())
    }

    /// Whether gap filling is enabled.
    #[napi(js_name = "fillsGaps")]
    pub fn fills_gaps(&self) -> bool {
        self.inner.fills_gaps()
    }
}

// ===== Data layer: resampling (candle -> higher-timeframe candle) =====

fn candle_to_value(c: wc::Candle) -> CandleValue {
    CandleValue {
        open: c.open,
        high: c.high,
        low: c.low,
        close: c.close,
        volume: c.volume,
        timestamp: c.timestamp as f64,
    }
}

/// Resample candles into a higher timeframe (e.g. 1m -> 5m).
#[napi(js_name = "Resampler")]
pub struct ResamplerNode {
    inner: wickra_data::resample::Resampler,
}

#[napi]
impl ResamplerNode {
    /// Construct a resampler that aggregates inputs into `timeframe`-sized
    /// candles (same unit as the candle timestamps).
    #[napi(constructor)]
    pub fn new(timeframe: f64, gap_fill: Option<bool>) -> napi::Result<Self> {
        let tf = wickra_data::aggregator::Timeframe::new(timeframe as i64).map_err(map_data_err)?;
        Ok(Self {
            inner: wickra_data::resample::Resampler::new(tf)
                .with_gap_fill(gap_fill.unwrap_or(false)),
        })
    }

    /// Whether the resampler emits a flat placeholder candle for skipped
    /// buckets.
    #[napi(js_name = "fillsGaps")]
    pub fn fills_gaps(&self) -> bool {
        self.inner.fills_gaps()
    }

    /// Push one candle; returns the higher-timeframe candles it completed.
    /// Normally that is none or one; with gap filling on, input that skips
    /// whole buckets completes several at once.
    #[napi]
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: f64,
    ) -> napi::Result<Vec<CandleValue>> {
        let candle =
            wc::Candle::new(open, high, low, close, volume, timestamp as i64).map_err(map_err)?;
        Ok(self
            .inner
            .push(candle)
            .map_err(map_data_err)?
            .into_iter()
            .map(candle_to_value)
            .collect())
    }

    /// Emit the final, still-open candle (or `null` if none is pending).
    #[napi]
    pub fn flush(&mut self) -> napi::Result<Option<CandleValue>> {
        Ok(self
            .inner
            .flush()
            .map_err(map_data_err)?
            .map(candle_to_value))
    }
}

// ===== Data layer: CSV candle reader =====

/// Parse OHLCV candles from a CSV string (header `timestamp,open,high,low,close,
/// volume`; a leading UTF-8 BOM is stripped).
#[napi(js_name = "CandleReader")]
pub struct CandleReaderNode {
    candles: Vec<wc::Candle>,
}

#[napi]
impl CandleReaderNode {
    /// Parse the whole CSV up front; throws on a malformed header or row.
    #[napi(constructor)]
    pub fn new(csv: String) -> napi::Result<Self> {
        let mut reader =
            wickra_data::csv::CandleReader::from_reader(csv.as_bytes()).map_err(map_data_err)?;
        let candles = reader.read_all().map_err(map_data_err)?;
        Ok(Self { candles })
    }

    /// Return every parsed candle as `{ open, high, low, close, volume, timestamp }`.
    #[napi]
    pub fn read(&self) -> Vec<CandleValue> {
        self.candles.iter().map(|&c| candle_to_value(c)).collect()
    }
}
