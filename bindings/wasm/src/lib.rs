//! WASM bindings for Wickra. Exposes every indicator with `Float64Array` I/O so
//! the API is essentially the same in the browser as it is in Python and Rust.
//!
//! Build with:
//! ```text
//! wasm-pack build bindings/wasm --target web --release
//! ```

#![allow(clippy::needless_pass_by_value)]
#![allow(missing_debug_implementations)] // wasm_bindgen wrappers expose JS objects, no need for Debug

use js_sys::{Array, Float64Array, Object, Reflect};
use wasm_bindgen::prelude::*;
use wickra_core as wc;
use wickra_core::{BarBuilder, BatchNanExt, Indicator};

/// A result-object key as a JS string, created once and then taken from
/// wasm-bindgen's intern cache: building a fresh string per key per `update`
/// (a UTF-8 decode across the boundary each) cost several times the update.
fn key(name: &'static str) -> JsValue {
    JsValue::from_str(wasm_bindgen::intern(name))
}

// A result object built in one call into JS: the keys are interned and cached
// per call site, the values cross as plain numbers. Building it field by field
// took two boundary crossings per field (one to box the number, one for
// `Reflect.set`) plus one for the object -- MACD's update spent most of its
// time there.
#[wasm_bindgen(
    inline_js = "export function obj2(k, v0, v1) { const o = {}; o[k[0]] = v0; o[k[1]] = v1; return o; }\nexport function obj3(k, v0, v1, v2) { const o = {}; o[k[0]] = v0; o[k[1]] = v1; o[k[2]] = v2; return o; }\nexport function obj4(k, v0, v1, v2, v3) { const o = {}; o[k[0]] = v0; o[k[1]] = v1; o[k[2]] = v2; o[k[3]] = v3; return o; }\nexport function obj5(k, v0, v1, v2, v3, v4) { const o = {}; o[k[0]] = v0; o[k[1]] = v1; o[k[2]] = v2; o[k[3]] = v3; o[k[4]] = v4; return o; }\nexport function obj6(k, v0, v1, v2, v3, v4, v5) { const o = {}; o[k[0]] = v0; o[k[1]] = v1; o[k[2]] = v2; o[k[3]] = v3; o[k[4]] = v4; o[k[5]] = v5; return o; }\nexport function obj7(k, v0, v1, v2, v3, v4, v5, v6) { const o = {}; o[k[0]] = v0; o[k[1]] = v1; o[k[2]] = v2; o[k[3]] = v3; o[k[4]] = v4; o[k[5]] = v5; o[k[6]] = v6; return o; }\nexport function obj9(k, v0, v1, v2, v3, v4, v5, v6, v7, v8) { const o = {}; o[k[0]] = v0; o[k[1]] = v1; o[k[2]] = v2; o[k[3]] = v3; o[k[4]] = v4; o[k[5]] = v5; o[k[6]] = v6; o[k[7]] = v7; o[k[8]] = v8; return o; }"
)]
extern "C" {
    fn obj2(keys: &Array, v0: f64, v1: f64) -> Object;
    fn obj3(keys: &Array, v0: f64, v1: f64, v2: f64) -> Object;
    fn obj4(keys: &Array, v0: f64, v1: f64, v2: f64, v3: f64) -> Object;
    fn obj5(keys: &Array, v0: f64, v1: f64, v2: f64, v3: f64, v4: f64) -> Object;
    fn obj6(keys: &Array, v0: f64, v1: f64, v2: f64, v3: f64, v4: f64, v5: f64) -> Object;
    fn obj7(keys: &Array, v0: f64, v1: f64, v2: f64, v3: f64, v4: f64, v5: f64, v6: f64) -> Object;
    fn obj9(
        keys: &Array,
        v0: f64,
        v1: f64,
        v2: f64,
        v3: f64,
        v4: f64,
        v5: f64,
        v6: f64,
        v7: f64,
        v8: f64,
    ) -> Object;
}

/// `js_object!("macd" => o.macd, "signal" => o.signal, ...)`: a plain JS object
/// with those number fields, built in one call (see `obj2` .. `obj9`).
macro_rules! js_object {
    ($k0:literal => $v0:expr, $k1:literal => $v1:expr) => {{
        thread_local! {
            static KEYS: Array = {
                let keys = Array::new();
                keys.push(&key($k0));
                keys.push(&key($k1));
                keys
            };
        }
        KEYS.with(|keys| obj2(keys, $v0, $v1))
    }};
    ($k0:literal => $v0:expr, $k1:literal => $v1:expr, $k2:literal => $v2:expr) => {{
        thread_local! {
            static KEYS: Array = {
                let keys = Array::new();
                keys.push(&key($k0));
                keys.push(&key($k1));
                keys.push(&key($k2));
                keys
            };
        }
        KEYS.with(|keys| obj3(keys, $v0, $v1, $v2))
    }};
    ($k0:literal => $v0:expr, $k1:literal => $v1:expr, $k2:literal => $v2:expr, $k3:literal => $v3:expr) => {{
        thread_local! {
            static KEYS: Array = {
                let keys = Array::new();
                keys.push(&key($k0));
                keys.push(&key($k1));
                keys.push(&key($k2));
                keys.push(&key($k3));
                keys
            };
        }
        KEYS.with(|keys| obj4(keys, $v0, $v1, $v2, $v3))
    }};
    ($k0:literal => $v0:expr, $k1:literal => $v1:expr, $k2:literal => $v2:expr, $k3:literal => $v3:expr, $k4:literal => $v4:expr) => {{
        thread_local! {
            static KEYS: Array = {
                let keys = Array::new();
                keys.push(&key($k0));
                keys.push(&key($k1));
                keys.push(&key($k2));
                keys.push(&key($k3));
                keys.push(&key($k4));
                keys
            };
        }
        KEYS.with(|keys| obj5(keys, $v0, $v1, $v2, $v3, $v4))
    }};
    ($k0:literal => $v0:expr, $k1:literal => $v1:expr, $k2:literal => $v2:expr, $k3:literal => $v3:expr, $k4:literal => $v4:expr, $k5:literal => $v5:expr) => {{
        thread_local! {
            static KEYS: Array = {
                let keys = Array::new();
                keys.push(&key($k0));
                keys.push(&key($k1));
                keys.push(&key($k2));
                keys.push(&key($k3));
                keys.push(&key($k4));
                keys.push(&key($k5));
                keys
            };
        }
        KEYS.with(|keys| obj6(keys, $v0, $v1, $v2, $v3, $v4, $v5))
    }};
    ($k0:literal => $v0:expr, $k1:literal => $v1:expr, $k2:literal => $v2:expr, $k3:literal => $v3:expr, $k4:literal => $v4:expr, $k5:literal => $v5:expr, $k6:literal => $v6:expr) => {{
        thread_local! {
            static KEYS: Array = {
                let keys = Array::new();
                keys.push(&key($k0));
                keys.push(&key($k1));
                keys.push(&key($k2));
                keys.push(&key($k3));
                keys.push(&key($k4));
                keys.push(&key($k5));
                keys.push(&key($k6));
                keys
            };
        }
        KEYS.with(|keys| obj7(keys, $v0, $v1, $v2, $v3, $v4, $v5, $v6))
    }};
    ($k0:literal => $v0:expr, $k1:literal => $v1:expr, $k2:literal => $v2:expr, $k3:literal => $v3:expr, $k4:literal => $v4:expr, $k5:literal => $v5:expr, $k6:literal => $v6:expr, $k7:literal => $v7:expr, $k8:literal => $v8:expr) => {{
        thread_local! {
            static KEYS: Array = {
                let keys = Array::new();
                keys.push(&key($k0));
                keys.push(&key($k1));
                keys.push(&key($k2));
                keys.push(&key($k3));
                keys.push(&key($k4));
                keys.push(&key($k5));
                keys.push(&key($k6));
                keys.push(&key($k7));
                keys.push(&key($k8));
                keys
            };
        }
        KEYS.with(|keys| obj9(keys, $v0, $v1, $v2, $v3, $v4, $v5, $v6, $v7, $v8))
    }};
}

fn map_err(e: wc::Error) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen]
extern "C" {
    /// A JavaScript array of booleans. `wasm_bindgen` has no slice type for
    /// `bool` the way it does for the numeric primitives, so the batch methods
    /// that take a trade side or a breadth flag take this and are typed
    /// `boolean[]` on the TypeScript side rather than `any`.
    #[wasm_bindgen(typescript_type = "boolean[]")]
    pub type BoolArray;
}

#[wasm_bindgen]
extern "C" {
    /// A run of candles from the data layer.
    #[wasm_bindgen(
        typescript_type = "{ open: number; high: number; low: number; close: number; volume: number; timestamp: number }[]"
    )]
    pub type WasmCandleArrayValue;

    /// The bars `DollarBars` completes.
    #[wasm_bindgen(
        typescript_type = "{ open: number; high: number; low: number; close: number; volume: number; dollar: number }[]"
    )]
    pub type WasmDollarBarsValue;

    /// Every `Footprint.batch` snapshot: one price-level array per trade.
    #[wasm_bindgen(typescript_type = "{ price: number; bidVol: number; askVol: number }[][]")]
    pub type WasmFootprintBatchValue;

    /// The bars `ImbalanceBars` completes.
    #[wasm_bindgen(
        typescript_type = "{ open: number; high: number; low: number; close: number; imbalance: number; direction: number }[]"
    )]
    pub type WasmImbalanceBarsValue;

    /// The bars `KagiBars` completes.
    #[wasm_bindgen(typescript_type = "{ start: number; end: number; direction: number }[]")]
    pub type WasmKagiBarsValue;

    /// The bars `PointAndFigureBars` completes.
    #[wasm_bindgen(typescript_type = "{ direction: number; high: number; low: number }[]")]
    pub type WasmPointAndFigureBarsValue;

    /// One bucket profile per bar, `undefined` while warming up.
    #[wasm_bindgen(typescript_type = "(Float64Array | undefined)[]")]
    pub type WasmProfileBatchValue;

    /// The bars `RangeBars` completes.
    #[wasm_bindgen(typescript_type = "{ open: number; close: number; direction: number }[]")]
    pub type WasmRangeBarsValue;

    /// The bars `RenkoBars` completes.
    #[wasm_bindgen(typescript_type = "{ open: number; close: number; direction: number }[]")]
    pub type WasmRenkoBarsValue;

    /// The bars `RunBars` completes.
    #[wasm_bindgen(
        typescript_type = "{ open: number; high: number; low: number; close: number; length: number; direction: number }[]"
    )]
    pub type WasmRunBarsValue;

    /// The bars `ThreeLineBreakBars` completes.
    #[wasm_bindgen(typescript_type = "{ open: number; close: number; direction: number }[]")]
    pub type WasmThreeLineBreakBarsValue;

    /// The bars `TickBars` completes.
    #[wasm_bindgen(
        typescript_type = "{ open: number; high: number; low: number; close: number; volume: number }[]"
    )]
    pub type WasmTickBarsValue;

    /// The bars `VolumeBars` completes.
    #[wasm_bindgen(
        typescript_type = "{ open: number; high: number; low: number; close: number; volume: number }[]"
    )]
    pub type WasmVolumeBarsValue;
}

#[wasm_bindgen]
extern "C" {
    /// One `Footprint.update` snapshot: the price levels seen so far.
    #[wasm_bindgen(typescript_type = "{ price: number; bidVol: number; askVol: number }[]")]
    pub type WasmFootprintValue;

    /// A candle handed back by the data layer.
    #[wasm_bindgen(
        typescript_type = "{ open: number; high: number; low: number; close: number; volume: number; timestamp: number }"
    )]
    pub type WasmCandleValue;
}

#[wasm_bindgen]
extern "C" {
    /// The object `AccelerationBands.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmAccelerationBandsValue;

    /// The object `ADX.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ plusDi: number; minusDi: number; adx: number }")]
    pub type WasmAdxValue;

    /// The object `Alligator.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ jaw: number; teeth: number; lips: number }")]
    pub type WasmAlligatorValue;

    /// The object `AndrewsPitchfork.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ median: number; upper: number; lower: number }")]
    pub type WasmAndrewsPitchforkValue;

    /// The object `Aroon.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ up: number; down: number }")]
    pub type WasmAroonValue;

    /// The object `AtrBands.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmAtrBandsValue;

    /// The object `AtrRatchet.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ value: number; direction: number }")]
    pub type WasmAtrRatchetValue;

    /// The object `AutoFib.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ level0: number; level236: number; level382: number; level500: number; level618: number; level786: number; level1000: number }"
    )]
    pub type WasmAutoFibValue;

    /// The object `BollingerBands.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ upper: number; middle: number; lower: number; stddev: number }"
    )]
    pub type WasmBbValue;

    /// The object `BomarBands.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmBomarBandsValue;

    /// The object `Camarilla.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ pp: number; r1: number; r2: number; r3: number; r4: number; s1: number; s2: number; s3: number; s4: number }"
    )]
    pub type WasmCamarillaValue;

    /// The object `CandleVolume.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ body: number; width: number }")]
    pub type WasmCandleVolumeValue;

    /// The object `CentralPivotRange.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ pivot: number; tc: number; bc: number }")]
    pub type WasmCentralPivotRangeValue;

    /// The object `ChandeKrollStop.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ stopLong: number; stopShort: number }")]
    pub type WasmChandeKrollStopValue;

    /// The object `ChandelierExit.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ longStop: number; shortStop: number }")]
    pub type WasmChandelierExitValue;

    /// The object `ClassicPivots.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ pp: number; r1: number; r2: number; r3: number; s1: number; s2: number; s3: number }"
    )]
    pub type WasmClassicPivotsValue;

    /// The object `Cointegration.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ hedgeRatio: number; spread: number; adfStat: number }")]
    pub type WasmCointegrationValue;

    /// The object `CompositeProfile.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ poc: number; vah: number; val: number }")]
    pub type WasmCompositeProfileValue;

    /// The object `DemarkPivots.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ pp: number; r1: number; s1: number }")]
    pub type WasmDemarkPivotsValue;

    /// The object `DonchianStop.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ stopLong: number; stopShort: number }")]
    pub type WasmDonchianStopValue;

    /// The object `Donchian.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmDonchianValue;

    /// The object `DoubleBollinger.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ upperOuter: number; upperInner: number; middle: number; lowerInner: number; lowerOuter: number }"
    )]
    pub type WasmDoubleBollingerValue;

    /// The object `ElderRay.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ bullPower: number; bearPower: number }")]
    pub type WasmElderRayValue;

    /// The object `ElderSafeZone.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ value: number; direction: number }")]
    pub type WasmElderSafeZoneValue;

    /// The object `Equivolume.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ height: number; width: number }")]
    pub type WasmEquivolumeValue;

    /// The object `FibArcs.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ arc382: number; arc500: number; arc618: number }")]
    pub type WasmFibArcsValue;

    /// The object `FibChannel.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ base: number; level618: number; level1000: number; level1618: number }"
    )]
    pub type WasmFibChannelValue;

    /// The object `FibConfluence.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ price: number; strength: number }")]
    pub type WasmFibConfluenceValue;

    /// The object `FibExtension.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ level1272: number; level1414: number; level1618: number; level2000: number; level2618: number }"
    )]
    pub type WasmFibExtensionValue;

    /// The object `FibFan.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ fan382: number; fan500: number; fan618: number }")]
    pub type WasmFibFanValue;

    /// The object `FibProjection.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ level618: number; level1000: number; level1618: number; level2618: number }"
    )]
    pub type WasmFibProjectionValue;

    /// The object `FibRetracement.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ level0: number; level236: number; level382: number; level500: number; level618: number; level786: number; level1000: number }"
    )]
    pub type WasmFibRetracementValue;

    /// The object `FibTimeZones.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ onZone: number; barsToNext: number }")]
    pub type WasmFibTimeZonesValue;

    /// The object `FibonacciPivots.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ pp: number; r1: number; r2: number; r3: number; s1: number; s2: number; s3: number }"
    )]
    pub type WasmFibonacciPivotsValue;

    /// The object `FractalChaosBands.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; lower: number }")]
    pub type WasmFractalChaosBandsValue;

    /// The object `GatorOscillator.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; lower: number }")]
    pub type WasmGatorOscillatorValue;

    /// The object `GoldenPocket.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ low: number; mid: number; high: number }")]
    pub type WasmGoldenPocketValue;

    /// The object `HeikinAshi.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ open: number; high: number; low: number; close: number }")]
    pub type WasmHeikinAshiValue;

    /// The object `HighLowVolumeNodes.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ hvn: number; lvn: number }")]
    pub type WasmHighLowVolumeNodesValue;

    /// The object `HT_PHASOR.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ inphase: number; quadrature: number }")]
    pub type WasmHtPhasorValue;

    /// The object `HurstChannel.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmHurstChannelValue;

    /// The object `Ichimoku.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ tenkan: number; kijun: number; senkouA: number; senkouB: number; chikou: number }"
    )]
    pub type WasmIchimokuValue;

    /// The object `InitialBalance.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ high: number; low: number }")]
    pub type WasmInitialBalanceValue;

    /// The object `KalmanHedgeRatio.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ hedgeRatio: number; intercept: number; spread: number }")]
    pub type WasmKalmanHedgeRatioValue;

    /// The object `KaseDevStop.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ value: number; direction: number }")]
    pub type WasmKaseDevStopValue;

    /// The object `KasePermissionStochastic.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ fast: number; slow: number }")]
    pub type WasmKasePermissionStochasticValue;

    /// The object `Keltner.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmKeltnerValue;

    /// The object `KST.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ kst: number; signal: number }")]
    pub type WasmKstValue;

    /// The object `LeadLagCrossCorrelation.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ lag: number; correlation: number }")]
    pub type WasmLeadLagCrossCorrelationValue;

    /// The object `LinRegChannel.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmLinRegChannelValue;

    /// The object `LiquidationFeatures.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ long: number; short: number; net: number; total: number; imbalance: number }"
    )]
    pub type WasmLiquidationFeaturesValue;

    /// The object `MaEnvelope.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmMaEnvelopeValue;

    /// The object `MACDEXT.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ macd: number; signal: number; histogram: number }")]
    pub type WasmMacdExtValue;

    /// The object `MACDFIX.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ macd: number; signal: number; histogram: number }")]
    pub type WasmMacdFixValue;

    /// The object `MACD.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ macd: number; signal: number; histogram: number }")]
    pub type WasmMacdValue;

    /// The object `MAMA.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ mama: number; fama: number }")]
    pub type WasmMamaValue;

    /// The object `MedianChannel.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmMedianChannelValue;

    /// The object `ModifiedMaStop.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ value: number; direction: number }")]
    pub type WasmModifiedMaStopValue;

    /// The object `MurreyMathLines.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ mm8_8: number; mm7_8: number; mm6_8: number; mm5_8: number; mm4_8: number; mm3_8: number; mm2_8: number; mm1_8: number; mm0_8: number }"
    )]
    pub type WasmMurreyMathLinesValue;

    /// The object `Nrtr.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ value: number; direction: number }")]
    pub type WasmNrtrValue;

    /// The object `OpeningRange.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ high: number; low: number; breakoutDistance: number }")]
    pub type WasmOpeningRangeValue;

    /// The object `OvernightIntradayReturn.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ overnight: number; intraday: number }")]
    pub type WasmOvernightIntradayReturnValue;

    /// The object `ProjectionBands.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmProjectionBandsValue;

    /// The object `QQE.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ rsiMa: number; trailingLine: number }")]
    pub type WasmQqeValue;

    /// The object `QuartileBands.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmQuartileBandsValue;

    /// The object `RelativeStrengthAB.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ ratio: number; ratioMa: number; ratioRsi: number }")]
    pub type WasmRelativeStrengthABValue;

    /// The object `RWI.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ high: number; low: number }")]
    pub type WasmRwiValue;

    /// The object `SessionHighLow.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ high: number; low: number }")]
    pub type WasmSessionHighLowValue;

    /// The object `SessionRange.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ asia: number; eu: number; us: number }")]
    pub type WasmSessionRangeValue;

    /// The object `SmoothedHeikinAshi.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ open: number; high: number; low: number; close: number }")]
    pub type WasmSmoothedHeikinAshiValue;

    /// The object `SpreadBollingerBands.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ middle: number; upper: number; lower: number; percentB: number }"
    )]
    pub type WasmSpreadBollingerBandsValue;

    /// The object `StandardErrorBands.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmStandardErrorBandsValue;

    /// The object `StarcBands.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ upper: number; middle: number; lower: number }")]
    pub type WasmStarcBandsValue;

    /// The object `Stochastic.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ k: number; d: number }")]
    pub type WasmStochValue;

    /// The object `SuperTrend.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ value: number; direction: number }")]
    pub type WasmSuperTrendValue;

    /// The object `TDLines.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ resistance: number; support: number }")]
    pub type WasmTdLinesValue;

    /// The object `TDMovingAverage.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ st1: number; st2: number }")]
    pub type WasmTdMovingAverageValue;

    /// The object `TDRangeProjection.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ high: number; low: number }")]
    pub type WasmTdRangeProjectionValue;

    /// The object `TDRiskLevel.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ buyRisk: number; sellRisk: number }")]
    pub type WasmTdRiskLevelValue;

    /// The object `TDSequential.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ setup: number; countdown: number; direction: number }")]
    pub type WasmTdSequentialValue;

    /// The object `TpoProfile.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ priceLow: number; priceHigh: number; counts: number }")]
    pub type WasmTpoProfileValue;

    /// The object `TtmSqueeze.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ squeeze: number; momentum: number }")]
    pub type WasmTtmSqueezeValue;

    /// The object `ValueArea.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ poc: number; vah: number; val: number }")]
    pub type WasmValueAreaValue;

    /// The object `VolatilityCone.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ current: number; min: number; median: number; max: number; percentile: number }"
    )]
    pub type WasmVolatilityConeValue;

    /// The object `VolumeProfile.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ priceLow: number; priceHigh: number; bins: number }")]
    pub type WasmVolumeProfileValue;

    /// The object `VolumeWeightedMacd.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ macd: number; signal: number; histogram: number }")]
    pub type WasmVolumeWeightedMacdValue;

    /// The object `VolumeWeightedSr.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ support: number; resistance: number }")]
    pub type WasmVolumeWeightedSrValue;

    /// The object `Vortex.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ plus: number; minus: number }")]
    pub type WasmVortexValue;

    /// The object `VwapStdDevBands.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ upper: number; middle: number; lower: number; stddev: number }"
    )]
    pub type WasmVwapStdDevBandsValue;

    /// The object `WaveTrend.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ wt1: number; wt2: number }")]
    pub type WasmWaveTrendValue;

    /// The object `WilliamsFractals.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ up: number; down: number }")]
    pub type WasmWilliamsFractalsValue;

    /// The object `WoodiePivots.update` returns once it is warm.
    #[wasm_bindgen(
        typescript_type = "{ pp: number; r1: number; r2: number; s1: number; s2: number }"
    )]
    pub type WasmWoodiePivotsValue;

    /// The object `ZeroLagMACD.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ macd: number; signal: number; histogram: number }")]
    pub type WasmZeroLagMacdValue;

    /// The object `ZigZag.update` returns once it is warm.
    #[wasm_bindgen(typescript_type = "{ swing: number; direction: number }")]
    pub type WasmZigZagValue;
}

/// Read a `boolean[]` into a `Vec<bool>`, rejecting anything else.
fn bool_series(flags: &BoolArray) -> Result<Vec<bool>, JsError> {
    Array::from(flags.as_ref())
        .iter()
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| JsError::new("expected an array of booleans"))
        })
        .collect()
}

/// Optional helper: install `console.error` panic hook in the browser.
#[wasm_bindgen(js_name = installPanicHook)]
pub fn install_panic_hook() {
    #[cfg(feature = "panic-hook")]
    console_error_panic_hook::set_once();
}

/// Library version (matches the Cargo package version).
#[wasm_bindgen(js_name = version)]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ---------- Scalar-input indicators ----------

macro_rules! wasm_scalar_indicator {
    ($name:ident, $py_name:literal, $rust_ty:ty, $($arg:ident: $arg_ty:ty),*) => {
        #[wasm_bindgen(js_name = $py_name)]
        pub struct $name {
            inner: $rust_ty,
        }

        #[wasm_bindgen(js_class = $py_name)]
        impl $name {
            #[wasm_bindgen(constructor)]
            pub fn new($($arg: $arg_ty),*) -> Result<$name, JsError> {
                Ok($name {
                    inner: <$rust_ty>::new($($arg),*).map_err(map_err)?,
                })
            }
            pub fn update(&mut self, value: f64) -> Option<f64> {
                self.inner.update(value)
            }
            pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
                Float64Array::from(self.inner.batch_nan(prices).as_slice())
            }
            /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
            /// agrees with `batch` to within a few units in the last place rather than bit for
            /// bit; NaN placement and length are identical, and the result is the same on
            /// every platform. Without a kernel it is exactly `batch`.
            #[wasm_bindgen(js_name = batchFast)]
            pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
                Float64Array::from(self.inner.batch_fast(prices).as_slice())
            }
            /// `batch` into a caller `Float64Array` exactly as long as the input.
            #[wasm_bindgen(js_name = batchInto)]
            pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
                check_rows(out, prices.len())?;
                self.inner.batch_nan_into(prices, out);
                Ok(())
            }
            /// `batchFast` into a caller `Float64Array` exactly as long as the input.
            #[wasm_bindgen(js_name = batchFastInto)]
            pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
                check_rows(out, prices.len())?;
                self.inner.batch_fast_into(prices, out);
                Ok(())
            }
            pub fn reset(&mut self) { self.inner.reset(); }
            pub fn name(&self) -> String { self.inner.name().to_string() }
            #[wasm_bindgen(js_name = isReady)] pub fn is_ready(&self) -> bool { self.inner.is_ready() }
            #[wasm_bindgen(js_name = warmupPeriod)] pub fn warmup_period(&self) -> usize { self.inner.warmup_period() }
        }
    };
}

wasm_scalar_indicator!(WasmSma, "SMA", wc::Sma, period: usize);
wasm_scalar_indicator!(WasmEma, "EMA", wc::Ema, period: usize);
wasm_scalar_indicator!(WasmWma, "WMA", wc::Wma, period: usize);
wasm_scalar_indicator!(WasmRsi, "RSI", wc::Rsi, period: usize);
wasm_scalar_indicator!(WasmDema, "DEMA", wc::Dema, period: usize);
wasm_scalar_indicator!(WasmTema, "TEMA", wc::Tema, period: usize);
wasm_scalar_indicator!(WasmHma, "HMA", wc::Hma, period: usize);
wasm_scalar_indicator!(WasmRoc, "ROC", wc::Roc, period: usize);
wasm_scalar_indicator!(WasmTrix, "TRIX", wc::Trix, period: usize);
wasm_scalar_indicator!(WasmSmma, "SMMA", wc::Smma, period: usize);
wasm_scalar_indicator!(WasmTrima, "TRIMA", wc::Trima, period: usize);
wasm_scalar_indicator!(WasmZlema, "ZLEMA", wc::Zlema, period: usize);
wasm_scalar_indicator!(WasmT3, "T3", wc::T3, period: usize, v: f64);
wasm_scalar_indicator!(WasmAlma, "ALMA", wc::Alma, period: usize, offset: f64, sigma: f64);
wasm_scalar_indicator!(
    WasmPolarizedFractalEfficiency,
    "POLARIZED_FRACTAL_EFFICIENCY",
    wc::PolarizedFractalEfficiency,
    period: usize,
    smoothing: usize
);
wasm_scalar_indicator!(WasmWavePm, "WAVE_PM", wc::WavePm, length: usize, smoothing: usize);
wasm_scalar_indicator!(WasmMcGinleyDynamic, "McGinleyDynamic", wc::McGinleyDynamic, period: usize);
wasm_scalar_indicator!(WasmFrama, "FRAMA", wc::Frama, period: usize);
wasm_scalar_indicator!(WasmVidya, "VIDYA", wc::Vidya, period: usize, cmo_period: usize);
wasm_scalar_indicator!(WasmJma, "JMA", wc::Jma, period: usize, phase: f64, power: u32);
wasm_scalar_indicator!(WasmMom, "MOM", wc::Mom, period: usize);
wasm_scalar_indicator!(WasmCmo, "CMO", wc::Cmo, period: usize);
wasm_scalar_indicator!(WasmTsi, "TSI", wc::Tsi, long: usize, short: usize);
wasm_scalar_indicator!(WasmPmo, "PMO", wc::Pmo, smoothing1: usize, smoothing2: usize);
wasm_scalar_indicator!(WasmTii, "TII", wc::Tii, sma_period: usize, dev_period: usize);

#[wasm_bindgen(js_name = KST)]
pub struct WasmKst {
    inner: wc::Kst,
}

#[wasm_bindgen(js_class = KST)]
impl WasmKst {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        roc1: usize,
        roc2: usize,
        roc3: usize,
        roc4: usize,
        sma1: usize,
        sma2: usize,
        sma3: usize,
        sma4: usize,
        signal_period: usize,
    ) -> Result<WasmKst, JsError> {
        Ok(Self {
            inner: wc::Kst::new(
                roc1,
                roc2,
                roc3,
                roc4,
                sma1,
                sma2,
                sma3,
                sma4,
                signal_period,
            )
            .map_err(map_err)?,
        })
    }
    pub fn classic() -> WasmKst {
        Self {
            inner: wc::Kst::classic(),
        }
    }
    pub fn update(&mut self, value: f64) -> Option<WasmKstValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj = js_object!("kst" => o.kst, "signal" => o.signal);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Returns `[kst0, signal0, kst1, signal1, ...]`, length `2 * n`. Warmup is NaN.
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 2];
        for (i, &p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(p) {
                out[i * 2] = o.kst;
                out[i * 2 + 1] = o.signal;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}
wasm_scalar_indicator!(WasmStochRsi, "StochRSI", wc::StochRsi, rsi_period: usize, stoch_period: usize);
wasm_scalar_indicator!(WasmDpo, "DPO", wc::Dpo, period: usize);
wasm_scalar_indicator!(WasmPpo, "PPO", wc::Ppo, fast: usize, slow: usize);
wasm_scalar_indicator!(WasmApo, "APO", wc::Apo, fast: usize, slow: usize);
wasm_scalar_indicator!(WasmCfo, "CFO", wc::Cfo, period: usize);
wasm_scalar_indicator!(WasmElderImpulse, "ElderImpulse", wc::ElderImpulse, ema_period: usize, macd_fast: usize, macd_slow: usize, macd_signal: usize);
wasm_scalar_indicator!(WasmStc, "STC", wc::Stc, fast: usize, slow: usize, schaff_period: usize, factor: f64);

#[wasm_bindgen(js_name = ZeroLagMACD)]
pub struct WasmZeroLagMacd {
    inner: wc::ZeroLagMacd,
}

#[wasm_bindgen(js_class = ZeroLagMACD)]
impl WasmZeroLagMacd {
    #[wasm_bindgen(constructor)]
    pub fn new(fast: usize, slow: usize, signal: usize) -> Result<WasmZeroLagMacd, JsError> {
        Ok(Self {
            inner: wc::ZeroLagMacd::new(fast, slow, signal).map_err(map_err)?,
        })
    }
    /// Returns `[macd0, signal0, histogram0, ...]`, length `3n`.
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.macd;
                out[i * 3 + 1] = o.signal;
                out[i * 3 + 2] = o.histogram;
            }
        }
        Float64Array::from(out.as_slice())
    }
    /// Returns `{ macd, signal, histogram }` once warm, else `undefined`.
    pub fn update(&mut self, value: f64) -> Option<WasmZeroLagMacdValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj =
                    js_object!("macd" => o.macd, "signal" => o.signal, "histogram" => o.histogram);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}
wasm_scalar_indicator!(WasmCoppock, "Coppock", wc::Coppock, roc_long: usize, roc_short: usize, wma_period: usize);
wasm_scalar_indicator!(WasmStdDev, "StdDev", wc::StdDev, period: usize);
wasm_scalar_indicator!(WasmUlcerIndex, "UlcerIndex", wc::UlcerIndex, period: usize);
wasm_scalar_indicator!(WasmHistoricalVolatility, "HistoricalVolatility", wc::HistoricalVolatility, period: usize, trading_periods: usize);
wasm_scalar_indicator!(WasmBollingerBandwidth, "BollingerBandwidth", wc::BollingerBandwidth, period: usize, multiplier: f64);
wasm_scalar_indicator!(WasmPercentB, "PercentB", wc::PercentB, period: usize, multiplier: f64);
wasm_scalar_indicator!(WasmLinearRegression, "LinearRegression", wc::LinearRegression, period: usize);
wasm_scalar_indicator!(WasmLinRegSlope, "LinRegSlope", wc::LinRegSlope, period: usize);
wasm_scalar_indicator!(WasmVerticalHorizontalFilter, "VerticalHorizontalFilter", wc::VerticalHorizontalFilter, period: usize);
wasm_scalar_indicator!(WasmZScore, "ZScore", wc::ZScore, period: usize);
wasm_scalar_indicator!(WasmLinRegAngle, "LinRegAngle", wc::LinRegAngle, period: usize);
wasm_scalar_indicator!(WasmVariance, "Variance", wc::Variance, period: usize);
wasm_scalar_indicator!(WasmCoefficientOfVariation, "CoefficientOfVariation", wc::CoefficientOfVariation, period: usize);
wasm_scalar_indicator!(WasmSkewness, "Skewness", wc::Skewness, period: usize);
wasm_scalar_indicator!(WasmKurtosis, "Kurtosis", wc::Kurtosis, period: usize);
wasm_scalar_indicator!(WasmStandardError, "StandardError", wc::StandardError, period: usize);
wasm_scalar_indicator!(WasmDetrendedStdDev, "DetrendedStdDev", wc::DetrendedStdDev, period: usize);
wasm_scalar_indicator!(WasmRSquared, "RSquared", wc::RSquared, period: usize);
wasm_scalar_indicator!(WasmMedianAbsoluteDeviation, "MedianAbsoluteDeviation", wc::MedianAbsoluteDeviation, period: usize);
wasm_scalar_indicator!(WasmAutocorrelation, "Autocorrelation", wc::Autocorrelation, period: usize, lag: usize);
wasm_scalar_indicator!(WasmHurstExponent, "HurstExponent", wc::HurstExponent, period: usize, chunks: usize);
wasm_scalar_indicator!(WasmRviVolatility, "RVIVolatility", wc::RviVolatility, period: usize);
wasm_scalar_indicator!(WasmLaguerreRsi, "LaguerreRSI", wc::LaguerreRsi, gamma: f64);
wasm_scalar_indicator!(WasmConnorsRsi, "ConnorsRSI", wc::ConnorsRsi, period_rsi: usize, period_streak: usize, period_rank: usize);

// ---------- Yang-Zhang Volatility (OHLC candle, 2 params) ----------

#[wasm_bindgen(js_name = YangZhangVolatility)]
pub struct WasmYangZhangVolatility {
    inner: wc::YangZhangVolatility,
}

#[wasm_bindgen(js_class = YangZhangVolatility)]
impl WasmYangZhangVolatility {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, trading_periods: usize) -> Result<WasmYangZhangVolatility, JsError> {
        Ok(Self {
            inner: wc::YangZhangVolatility::new(period, trading_periods).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = open.len();
        if high.len() != n || low.len() != n || close.len() != n {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Rogers-Satchell Volatility (OHLC candle, 2 params) ----------

#[wasm_bindgen(js_name = RogersSatchellVolatility)]
pub struct WasmRogersSatchellVolatility {
    inner: wc::RogersSatchellVolatility,
}

#[wasm_bindgen(js_class = RogersSatchellVolatility)]
impl WasmRogersSatchellVolatility {
    #[wasm_bindgen(constructor)]
    pub fn new(
        period: usize,
        trading_periods: usize,
    ) -> Result<WasmRogersSatchellVolatility, JsError> {
        Ok(Self {
            inner: wc::RogersSatchellVolatility::new(period, trading_periods).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = open.len();
        if high.len() != n || low.len() != n || close.len() != n {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Garman-Klass Volatility (OHLC candle, 2 params) ----------

#[wasm_bindgen(js_name = GarmanKlassVolatility)]
pub struct WasmGarmanKlassVolatility {
    inner: wc::GarmanKlassVolatility,
}

#[wasm_bindgen(js_class = GarmanKlassVolatility)]
impl WasmGarmanKlassVolatility {
    #[wasm_bindgen(constructor)]
    pub fn new(
        period: usize,
        trading_periods: usize,
    ) -> Result<WasmGarmanKlassVolatility, JsError> {
        Ok(Self {
            inner: wc::GarmanKlassVolatility::new(period, trading_periods).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = open.len();
        if high.len() != n || low.len() != n || close.len() != n {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Parkinson Volatility (high-low candle, 2 params) ----------

#[wasm_bindgen(js_name = ParkinsonVolatility)]
pub struct WasmParkinsonVolatility {
    inner: wc::ParkinsonVolatility,
}

#[wasm_bindgen(js_class = ParkinsonVolatility)]
impl WasmParkinsonVolatility {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, trading_periods: usize) -> Result<WasmParkinsonVolatility, JsError> {
        Ok(Self {
            inner: wc::ParkinsonVolatility::new(period, trading_periods).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// Family 10 — Ehlers / Cycle scalars
wasm_scalar_indicator!(WasmSuperSmoother, "SuperSmoother", wc::SuperSmoother, period: usize);
wasm_scalar_indicator!(WasmFisherTransform, "FisherTransform", wc::FisherTransform, period: usize);
wasm_scalar_indicator!(WasmInverseFisherTransform, "InverseFisherTransform", wc::InverseFisherTransform, scale: f64);
wasm_scalar_indicator!(WasmDecycler, "Decycler", wc::Decycler, period: usize);
wasm_scalar_indicator!(WasmDecyclerOscillator, "DecyclerOscillator", wc::DecyclerOscillator, fast: usize, slow: usize);
wasm_scalar_indicator!(WasmRoofingFilter, "RoofingFilter", wc::RoofingFilter, lp_period: usize, hp_period: usize);
wasm_scalar_indicator!(WasmCenterOfGravity, "CenterOfGravity", wc::CenterOfGravity, period: usize);
wasm_scalar_indicator!(WasmCyberneticCycle, "CyberneticCycle", wc::CyberneticCycle, period: usize);
wasm_scalar_indicator!(WasmInstantaneousTrendline, "InstantaneousTrendline", wc::InstantaneousTrendline, period: usize);
wasm_scalar_indicator!(WasmEhlersStochastic, "EhlersStochastic", wc::EhlersStochastic, period: usize);
wasm_scalar_indicator!(WasmEmpiricalModeDecomposition, "EmpiricalModeDecomposition", wc::EmpiricalModeDecomposition, period: usize, fraction: f64);
wasm_scalar_indicator!(WasmFama, "FAMA", wc::Fama, fast_limit: f64, slow_limit: f64);

// ---------- Family 12: Two-series indicators (Pearson / Beta / Spearman) ----------

macro_rules! wasm_pair_indicator {
    ($name:ident, $js_name:literal, $rust_ty:ty) => {
        #[wasm_bindgen(js_name = $js_name)]
        pub struct $name {
            inner: $rust_ty,
        }

        #[wasm_bindgen(js_class = $js_name)]
        impl $name {
            #[wasm_bindgen(constructor)]
            pub fn new(period: usize) -> Result<$name, JsError> {
                Ok($name {
                    inner: <$rust_ty>::new(period).map_err(map_err)?,
                })
            }
            pub fn update(&mut self, x: f64, y: f64) -> Option<f64> {
                self.inner.update((x, y))
            }
            /// Batch over two equally-sized arrays. Returns one `f64` per
            /// input position (`NaN` during warmup).
            pub fn batch(&mut self, x: &[f64], y: &[f64]) -> Result<Float64Array, JsError> {
                if x.len() != y.len() {
                    return Err(JsError::new("x and y must be equal length"));
                }
                let mut out = Vec::with_capacity(x.len());
                for i in 0..x.len() {
                    out.push(self.inner.update((x[i], y[i])).unwrap_or(f64::NAN));
                }
                Ok(Float64Array::from(out.as_slice()))
            }
            pub fn reset(&mut self) {
                self.inner.reset();
            }

            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[wasm_bindgen(js_name = isReady)]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[wasm_bindgen(js_name = warmupPeriod)]
            pub fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
        }
    };
}

wasm_pair_indicator!(
    WasmPearsonCorrelation,
    "PearsonCorrelation",
    wc::PearsonCorrelation
);

#[wasm_bindgen(js_class = PearsonCorrelation)]
impl WasmPearsonCorrelation {
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, x: &[f64], y: &[f64]) -> Result<Float64Array, JsError> {
        if x.len() != y.len() {
            return Err(JsError::new("x and y must be equal length"));
        }
        let mut out = vec![0.0; x.len()];
        self.inner.batch_pairs_fast_into(x, y, &mut out);
        Ok(Float64Array::from(out.as_slice()))
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, x: &[f64], y: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        if x.len() != y.len() {
            return Err(JsError::new("x and y must be equal length"));
        }
        check_rows(out, x.len())?;
        self.inner.batch_pairs_into(x, y, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(
        &mut self,
        x: &[f64],
        y: &[f64],
        out: &mut [f64],
    ) -> Result<(), JsError> {
        if x.len() != y.len() {
            return Err(JsError::new("x and y must be equal length"));
        }
        check_rows(out, x.len())?;
        self.inner.batch_pairs_fast_into(x, y, out);
        Ok(())
    }
}
wasm_pair_indicator!(WasmBeta, "Beta", wc::Beta);
wasm_pair_indicator!(WasmPairwiseBeta, "PairwiseBeta", wc::PairwiseBeta);
wasm_pair_indicator!(
    WasmSpreadAr1Coefficient,
    "SpreadAr1Coefficient",
    wc::SpreadAr1Coefficient
);
wasm_pair_indicator!(
    WasmSpearmanCorrelation,
    "SpearmanCorrelation",
    wc::SpearmanCorrelation
);
wasm_pair_indicator!(
    WasmRollingCorrelation,
    "RollingCorrelation",
    wc::RollingCorrelation
);
wasm_pair_indicator!(
    WasmRollingCovariance,
    "RollingCovariance",
    wc::RollingCovariance
);
wasm_pair_indicator!(WasmOuHalfLife, "OuHalfLife", wc::OuHalfLife);
wasm_pair_indicator!(WasmSpreadHurst, "SpreadHurst", wc::SpreadHurst);
wasm_pair_indicator!(WasmDistanceSsd, "DistanceSsd", wc::DistanceSsd);
wasm_pair_indicator!(WasmKendallTau, "KendallTau", wc::KendallTau);
wasm_pair_indicator!(
    WasmBetaNeutralSpread,
    "BetaNeutralSpread",
    wc::BetaNeutralSpread
);
wasm_pair_indicator!(
    WasmHasbrouckInformationShare,
    "HasbrouckInformationShare",
    wc::HasbrouckInformationShare
);

// ---------- PairSpreadZScore (two params) ----------

#[wasm_bindgen(js_name = "PairSpreadZScore")]
pub struct WasmPairSpreadZScore {
    inner: wc::PairSpreadZScore,
}

#[wasm_bindgen(js_class = "PairSpreadZScore")]
impl WasmPairSpreadZScore {
    #[wasm_bindgen(constructor)]
    pub fn new(beta_period: usize, z_period: usize) -> Result<WasmPairSpreadZScore, JsError> {
        Ok(Self {
            inner: wc::PairSpreadZScore::new(beta_period, z_period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized arrays of prices. Returns one `f64` per
    /// input position (`NaN` during warmup).
    pub fn batch(&mut self, a: &[f64], b: &[f64]) -> Result<Float64Array, JsError> {
        if a.len() != b.len() {
            return Err(JsError::new("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            out.push(self.inner.update((a[i], b[i])).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- LeadLagCrossCorrelation (two params, object output) ----------

#[wasm_bindgen(js_name = "LeadLagCrossCorrelation")]
pub struct WasmLeadLagCrossCorrelation {
    inner: wc::LeadLagCrossCorrelation,
}

#[wasm_bindgen(js_class = "LeadLagCrossCorrelation")]
impl WasmLeadLagCrossCorrelation {
    #[wasm_bindgen(constructor)]
    pub fn new(window: usize, max_lag: usize) -> Result<WasmLeadLagCrossCorrelation, JsError> {
        Ok(Self {
            inner: wc::LeadLagCrossCorrelation::new(window, max_lag).map_err(map_err)?,
        })
    }
    /// Returns `{ lag, correlation }`, or `undefined` during warmup. Positive lag
    /// means `a` leads `b`.
    pub fn update(&mut self, a: f64, b: f64) -> Option<WasmLeadLagCrossCorrelationValue> {
        match self.inner.update((a, b)) {
            Some(o) => {
                let obj = js_object!("lag" => o.lag as f64, "correlation" => o.correlation);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Flat `Float64Array` of length `2 * n`: `[lag0, corr0, lag1, corr1, ...]`.
    /// Warmup positions are NaN.
    pub fn batch(&mut self, a: &[f64], b: &[f64]) -> Result<Float64Array, JsError> {
        if a.len() != b.len() {
            return Err(JsError::new("a and b must be equal length"));
        }
        let n = a.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update((a[i], b[i])) {
                out[i * 2] = o.lag as f64;
                out[i * 2 + 1] = o.correlation;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Cointegration (two params, object output) ----------

#[wasm_bindgen(js_name = "Cointegration")]
pub struct WasmCointegration {
    inner: wc::Cointegration,
}

#[wasm_bindgen(js_class = "Cointegration")]
impl WasmCointegration {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, adf_lags: usize) -> Result<WasmCointegration, JsError> {
        Ok(Self {
            inner: wc::Cointegration::new(period, adf_lags).map_err(map_err)?,
        })
    }
    /// Returns `{ hedgeRatio, spread, adfStat }`, or `undefined` during warmup.
    pub fn update(&mut self, a: f64, b: f64) -> Option<WasmCointegrationValue> {
        match self.inner.update((a, b)) {
            Some(o) => {
                let obj = js_object!("hedgeRatio" => o.hedge_ratio, "spread" => o.spread, "adfStat" => o.adf_stat);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Flat `Float64Array` of length `3 * n`:
    /// `[hedgeRatio0, spread0, adfStat0, hedgeRatio1, ...]`. Warmup rows are NaN.
    pub fn batch(&mut self, a: &[f64], b: &[f64]) -> Result<Float64Array, JsError> {
        if a.len() != b.len() {
            return Err(JsError::new("a and b must be equal length"));
        }
        let n = a.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update((a[i], b[i])) {
                out[i * 3] = o.hedge_ratio;
                out[i * 3 + 1] = o.spread;
                out[i * 3 + 2] = o.adf_stat;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- RelativeStrengthAB (two params, object output) ----------

#[wasm_bindgen(js_name = "RelativeStrengthAB")]
pub struct WasmRelativeStrengthAB {
    inner: wc::RelativeStrengthAB,
}

#[wasm_bindgen(js_class = "RelativeStrengthAB")]
impl WasmRelativeStrengthAB {
    #[wasm_bindgen(constructor)]
    pub fn new(ma_period: usize, rsi_period: usize) -> Result<WasmRelativeStrengthAB, JsError> {
        Ok(Self {
            inner: wc::RelativeStrengthAB::new(ma_period, rsi_period).map_err(map_err)?,
        })
    }
    /// Returns `{ ratio, ratioMa, ratioRsi }`, or `undefined` during warmup.
    pub fn update(&mut self, a: f64, b: f64) -> Option<WasmRelativeStrengthABValue> {
        match self.inner.update((a, b)) {
            Some(o) => {
                let obj = js_object!("ratio" => o.ratio, "ratioMa" => o.ratio_ma, "ratioRsi" => o.ratio_rsi);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Flat `Float64Array` of length `3 * n`:
    /// `[ratio0, ratioMa0, ratioRsi0, ratio1, ...]`. Warmup rows are NaN.
    pub fn batch(&mut self, a: &[f64], b: &[f64]) -> Result<Float64Array, JsError> {
        if a.len() != b.len() {
            return Err(JsError::new("a and b must be equal length"));
        }
        let n = a.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update((a[i], b[i])) {
                out[i * 3] = o.ratio;
                out[i * 3 + 1] = o.ratio_ma;
                out[i * 3 + 2] = o.ratio_rsi;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- VarianceRatio (two params) ----------

#[wasm_bindgen(js_name = "VarianceRatio")]
pub struct WasmVarianceRatio {
    inner: wc::VarianceRatio,
}

#[wasm_bindgen(js_class = "VarianceRatio")]
impl WasmVarianceRatio {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, q: usize) -> Result<WasmVarianceRatio, JsError> {
        Ok(Self {
            inner: wc::VarianceRatio::new(period, q).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized arrays of prices. Returns one `f64` per
    /// input position (`NaN` during warmup).
    pub fn batch(&mut self, a: &[f64], b: &[f64]) -> Result<Float64Array, JsError> {
        if a.len() != b.len() {
            return Err(JsError::new("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            out.push(self.inner.update((a[i], b[i])).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- GrangerCausality (two params) ----------

#[wasm_bindgen(js_name = "GrangerCausality")]
pub struct WasmGrangerCausality {
    inner: wc::GrangerCausality,
}

#[wasm_bindgen(js_class = "GrangerCausality")]
impl WasmGrangerCausality {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, lag: usize) -> Result<WasmGrangerCausality, JsError> {
        Ok(Self {
            inner: wc::GrangerCausality::new(period, lag).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized arrays of prices. Returns one `f64` per
    /// input position (`NaN` during warmup).
    pub fn batch(&mut self, a: &[f64], b: &[f64]) -> Result<Float64Array, JsError> {
        if a.len() != b.len() {
            return Err(JsError::new("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            out.push(self.inner.update((a[i], b[i])).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- KalmanHedgeRatio (two params, object output) ----------

#[wasm_bindgen(js_name = "KalmanHedgeRatio")]
pub struct WasmKalmanHedgeRatio {
    inner: wc::KalmanHedgeRatio,
}

#[wasm_bindgen(js_class = "KalmanHedgeRatio")]
impl WasmKalmanHedgeRatio {
    #[wasm_bindgen(constructor)]
    pub fn new(delta: f64, observation_var: f64) -> Result<WasmKalmanHedgeRatio, JsError> {
        Ok(Self {
            inner: wc::KalmanHedgeRatio::new(delta, observation_var).map_err(map_err)?,
        })
    }
    /// Returns `{ hedgeRatio, intercept, spread }`, or `undefined` during warmup.
    pub fn update(&mut self, a: f64, b: f64) -> Option<WasmKalmanHedgeRatioValue> {
        match self.inner.update((a, b)) {
            Some(o) => {
                let obj = js_object!("hedgeRatio" => o.hedge_ratio, "intercept" => o.intercept, "spread" => o.spread);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Flat `Float64Array` of length `3 * n`:
    /// `[hedgeRatio0, intercept0, spread0, hedgeRatio1, ...]`. Warmup rows are NaN.
    pub fn batch(&mut self, a: &[f64], b: &[f64]) -> Result<Float64Array, JsError> {
        if a.len() != b.len() {
            return Err(JsError::new("a and b must be equal length"));
        }
        let n = a.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update((a[i], b[i])) {
                out[i * 3] = o.hedge_ratio;
                out[i * 3 + 1] = o.intercept;
                out[i * 3 + 2] = o.spread;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- SpreadBollingerBands (two params, object output) ----------

#[wasm_bindgen(js_name = "SpreadBollingerBands")]
pub struct WasmSpreadBollingerBands {
    inner: wc::SpreadBollingerBands,
}

#[wasm_bindgen(js_class = "SpreadBollingerBands")]
impl WasmSpreadBollingerBands {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, num_std: f64) -> Result<WasmSpreadBollingerBands, JsError> {
        Ok(Self {
            inner: wc::SpreadBollingerBands::new(period, num_std).map_err(map_err)?,
        })
    }
    /// Returns `{ middle, upper, lower, percentB }`, or `undefined` during warmup.
    pub fn update(&mut self, a: f64, b: f64) -> Option<WasmSpreadBollingerBandsValue> {
        match self.inner.update((a, b)) {
            Some(o) => {
                let obj = js_object!("middle" => o.middle, "upper" => o.upper, "lower" => o.lower, "percentB" => o.percent_b);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Flat `Float64Array` of length `4 * n`:
    /// `[middle0, upper0, lower0, percentB0, middle1, ...]`. Warmup rows are NaN.
    pub fn batch(&mut self, a: &[f64], b: &[f64]) -> Result<Float64Array, JsError> {
        if a.len() != b.len() {
            return Err(JsError::new("a and b must be equal length"));
        }
        let n = a.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            if let Some(o) = self.inner.update((a[i], b[i])) {
                out[i * 4] = o.middle;
                out[i * 4 + 1] = o.upper;
                out[i * 4 + 2] = o.lower;
                out[i * 4 + 3] = o.percent_b;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- KAMA (three params) ----------

#[wasm_bindgen(js_name = KAMA)]
pub struct WasmKama {
    inner: wc::Kama,
}

#[wasm_bindgen(js_class = KAMA)]
impl WasmKama {
    #[wasm_bindgen(constructor)]
    pub fn new(er_period: usize, fast: usize, slow: usize) -> Result<WasmKama, JsError> {
        Ok(Self {
            inner: wc::Kama::new(er_period, fast, slow).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_nan(prices).as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_fast(prices).as_slice())
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_nan_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_fast_into(prices, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- MACD ----------

#[wasm_bindgen(js_name = MACD)]
pub struct WasmMacd {
    inner: wc::MacdIndicator,
}

#[wasm_bindgen(js_class = MACD)]
impl WasmMacd {
    #[wasm_bindgen(constructor)]
    pub fn new(fast: usize, slow: usize, signal: usize) -> Result<WasmMacd, JsError> {
        Ok(Self {
            inner: wc::MacdIndicator::new(fast, slow, signal).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<WasmMacdValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj =
                    js_object!("macd" => o.macd, "signal" => o.signal, "histogram" => o.histogram);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Returns a flat `Float64Array` of length `3 * n`: `[macd0, sig0, hist0, macd1, sig1, hist1, ...]`.
    /// Use `result[3*i + 0/1/2]` to read each column. Warmup positions are NaN.
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let mut out = vec![0.0; prices.len() * 3];
        self.inner.batch_macd_into(prices, &mut out);
        Float64Array::from(out.as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each field
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        let mut out = vec![0.0; prices.len() * 3];
        self.inner.batch_macd_fast_into(prices, &mut out);
        Float64Array::from(out.as_slice())
    }
    /// `batch` into a caller `Float64Array` of `3 * n` values.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len() * 3)?;
        self.inner.batch_macd_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` of `3 * n` values.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len() * 3)?;
        self.inner.batch_macd_fast_into(prices, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Bollinger ----------

#[wasm_bindgen(js_name = BollingerBands)]
pub struct WasmBb {
    inner: wc::BollingerBands,
}

#[wasm_bindgen(js_class = BollingerBands)]
impl WasmBb {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, multiplier: f64) -> Result<WasmBb, JsError> {
        Ok(Self {
            inner: wc::BollingerBands::new(period, multiplier).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<WasmBbValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower, "stddev" => o.stddev);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Returns `[u0, m0, l0, sd0, u1, m1, l1, sd1, ...]`, length `4 * n`.
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let mut out = vec![0.0; prices.len() * 4];
        self.inner.batch_bands_into(prices, &mut out);
        Float64Array::from(out.as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each field
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        let mut out = vec![0.0; prices.len() * 4];
        self.inner.batch_bands_fast_into(prices, &mut out);
        Float64Array::from(out.as_slice())
    }
    /// `batch` into a caller `Float64Array` of `4 * n` values.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len() * 4)?;
        self.inner.batch_bands_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` of `4 * n` values.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len() * 4)?;
        self.inner.batch_bands_fast_into(prices, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Candle-input indicators ----------

/// A caller output buffer must hold exactly `rows` values.
fn check_rows(out: &[f64], rows: usize) -> Result<(), JsError> {
    if out.len() != rows {
        return Err(JsError::new(&format!(
            "the output must hold {rows} values, got {}",
            out.len()
        )));
    }
    Ok(())
}

fn make_candle(h: f64, l: f64, c: f64, v: f64) -> Result<wc::Candle, JsError> {
    wc::Candle::new(c, h, l, c, v, 0).map_err(map_err)
}

/// Helper for OHLC-input indicators where `open` matters (`RVI`, `BalanceOfPower`).
fn make_candle_ohlc(o: f64, h: f64, l: f64, c: f64) -> Result<wc::Candle, JsError> {
    wc::Candle::new(o, h, l, c, 0.0, 0).map_err(map_err)
}

#[wasm_bindgen(js_name = SMI)]
pub struct WasmSmi {
    inner: wc::Smi,
}

#[wasm_bindgen(js_class = SMI)]
impl WasmSmi {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, d_period: usize, d2_period: usize) -> Result<WasmSmi, JsError> {
        Ok(Self {
            inner: wc::Smi::new(period, d_period, d2_period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if !(high.len() == low.len() && low.len() == close.len()) {
            return Err(JsError::new("high, low and close must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = PGO)]
pub struct WasmPgo {
    inner: wc::Pgo,
}

#[wasm_bindgen(js_class = PGO)]
impl WasmPgo {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmPgo, JsError> {
        Ok(Self {
            inner: wc::Pgo::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if !(high.len() == low.len() && low.len() == close.len()) {
            return Err(JsError::new("high, low and close must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = Inertia)]
pub struct WasmInertia {
    inner: wc::Inertia,
}

#[wasm_bindgen(js_class = Inertia)]
impl WasmInertia {
    #[wasm_bindgen(constructor)]
    pub fn new(rvi_period: usize, linreg_period: usize) -> Result<WasmInertia, JsError> {
        Ok(Self {
            inner: wc::Inertia::new(rvi_period, linreg_period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle_ohlc(open, high, low, close)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if !(open.len() == high.len() && high.len() == low.len() && low.len() == close.len()) {
            return Err(JsError::new(
                "open, high, low and close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle_ohlc(open[i], high[i], low[i], close[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = RVI)]
pub struct WasmRvi {
    inner: wc::Rvi,
}

#[wasm_bindgen(js_class = RVI)]
impl WasmRvi {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmRvi, JsError> {
        Ok(Self {
            inner: wc::Rvi::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle_ohlc(open, high, low, close)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if !(open.len() == high.len() && high.len() == low.len() && low.len() == close.len()) {
            return Err(JsError::new(
                "open, high, low and close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle_ohlc(open[i], high[i], low[i], close[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ATR)]
pub struct WasmAtr {
    inner: wc::Atr,
}

#[wasm_bindgen(js_class = ATR)]
impl WasmAtr {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmAtr, JsError> {
        Ok(Self {
            inner: wc::Atr::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        // Every bar is validated first, as `update` would build it, so a bad
        // bar is refused before the indicator consumes anything.
        for i in 0..n {
            make_candle(high[i], low[i], close[i], 0.0)?;
        }
        let mut out = vec![0.0; n];
        self.inner.batch_atr_into(high, low, close, &mut out);
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        // Every bar is validated first, as `update` would build it, so a bad
        // bar is refused before the indicator consumes anything.
        for i in 0..n {
            make_candle(high[i], low[i], close[i], 0.0)?;
        }
        let mut out = vec![0.0; n];
        self.inner.batch_atr_fast_into(high, low, close, &mut out);
        Ok(Float64Array::from(out.as_slice()))
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        out: &mut [f64],
    ) -> Result<(), JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        // Every bar is validated first, as `update` would build it, so a bad
        // bar is refused before the indicator consumes anything.
        for i in 0..n {
            make_candle(high[i], low[i], close[i], 0.0)?;
        }
        check_rows(out, n)?;
        self.inner.batch_atr_into(high, low, close, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        out: &mut [f64],
    ) -> Result<(), JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        // Every bar is validated first, as `update` would build it, so a bad
        // bar is refused before the indicator consumes anything.
        for i in 0..n {
            make_candle(high[i], low[i], close[i], 0.0)?;
        }
        check_rows(out, n)?;
        self.inner.batch_atr_fast_into(high, low, close, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = PLUS_DM)]
pub struct WasmPlusDm {
    inner: wc::PlusDm,
}

#[wasm_bindgen(js_class = PLUS_DM)]
impl WasmPlusDm {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmPlusDm, JsError> {
        Ok(Self {
            inner: wc::PlusDm::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = MINUS_DM)]
pub struct WasmMinusDm {
    inner: wc::MinusDm,
}

#[wasm_bindgen(js_class = MINUS_DM)]
impl WasmMinusDm {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmMinusDm, JsError> {
        Ok(Self {
            inner: wc::MinusDm::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = PLUS_DI)]
pub struct WasmPlusDi {
    inner: wc::PlusDi,
}

#[wasm_bindgen(js_class = PLUS_DI)]
impl WasmPlusDi {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmPlusDi, JsError> {
        Ok(Self {
            inner: wc::PlusDi::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = MINUS_DI)]
pub struct WasmMinusDi {
    inner: wc::MinusDi,
}

#[wasm_bindgen(js_class = MINUS_DI)]
impl WasmMinusDi {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmMinusDi, JsError> {
        Ok(Self {
            inner: wc::MinusDi::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = DX)]
pub struct WasmDx {
    inner: wc::Dx,
}

#[wasm_bindgen(js_class = DX)]
impl WasmDx {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmDx, JsError> {
        Ok(Self {
            inner: wc::Dx::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = MIDPRICE)]
pub struct WasmMidPrice {
    inner: wc::MidPrice,
}

#[wasm_bindgen(js_class = MIDPRICE)]
impl WasmMidPrice {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmMidPrice, JsError> {
        Ok(Self {
            inner: wc::MidPrice::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = AVGPRICE)]
pub struct WasmAvgPrice {
    inner: wc::AvgPrice,
}

impl Default for WasmAvgPrice {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = AVGPRICE)]
impl WasmAvgPrice {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmAvgPrice {
        Self {
            inner: wc::AvgPrice::new(),
        }
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle_ohlc(open, high, low, close)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if !(open.len() == high.len() && high.len() == low.len() && low.len() == close.len()) {
            return Err(JsError::new(
                "open, high, low and close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle_ohlc(open[i], high[i], low[i], close[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = MACDEXT)]
pub struct WasmMacdExt {
    inner: wc::MacdExt,
}

#[wasm_bindgen(js_class = MACDEXT)]
impl WasmMacdExt {
    /// Moving-average types are TA-Lib `MA_Type` codes `0..=5`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        fast: usize,
        fast_matype: u32,
        slow: usize,
        slow_matype: u32,
        signal: usize,
        signal_matype: u32,
    ) -> Result<WasmMacdExt, JsError> {
        Ok(Self {
            inner: wc::MacdExt::new(
                fast,
                wc::MaType::from_code(fast_matype).map_err(map_err)?,
                slow,
                wc::MaType::from_code(slow_matype).map_err(map_err)?,
                signal,
                wc::MaType::from_code(signal_matype).map_err(map_err)?,
            )
            .map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<WasmMacdExtValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj =
                    js_object!("macd" => o.macd, "signal" => o.signal, "histogram" => o.histogram);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Returns a flat `Float64Array` of length `3 * n`: `[macd0, sig0, hist0, ...]`.
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.macd;
                out[i * 3 + 1] = o.signal;
                out[i * 3 + 2] = o.histogram;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = MACDFIX)]
pub struct WasmMacdFix {
    inner: wc::MacdFix,
}

#[wasm_bindgen(js_class = MACDFIX)]
impl WasmMacdFix {
    #[wasm_bindgen(constructor)]
    pub fn new(signal: usize) -> Result<WasmMacdFix, JsError> {
        Ok(Self {
            inner: wc::MacdFix::new(signal).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<WasmMacdFixValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj =
                    js_object!("macd" => o.macd, "signal" => o.signal, "histogram" => o.histogram);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Returns a flat `Float64Array` of length `3 * n`: `[macd0, sig0, hist0, ...]`.
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.macd;
                out[i * 3 + 1] = o.signal;
                out[i * 3 + 2] = o.histogram;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = SAREXT)]
pub struct WasmSarExt {
    inner: wc::SarExt,
}

#[wasm_bindgen(js_class = SAREXT)]
impl WasmSarExt {
    #[wasm_bindgen(constructor)]
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
    ) -> Result<WasmSarExt, JsError> {
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
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = HT_PHASOR)]
pub struct WasmHtPhasor {
    inner: wc::HtPhasor,
}

impl Default for WasmHtPhasor {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = HT_PHASOR)]
impl WasmHtPhasor {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmHtPhasor {
        Self {
            inner: wc::HtPhasor::new(),
        }
    }
    pub fn update(&mut self, value: f64) -> Option<WasmHtPhasorValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj = js_object!("inphase" => o.inphase, "quadrature" => o.quadrature);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Returns a flat `Float64Array` of length `2 * n`: `[inphase0, quad0, ...]`.
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 2];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 2] = o.inphase;
                out[i * 2 + 1] = o.quadrature;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = CloseVsOpen)]
pub struct WasmCloseVsOpen {
    inner: wc::CloseVsOpen,
}

impl Default for WasmCloseVsOpen {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = CloseVsOpen)]
impl WasmCloseVsOpen {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmCloseVsOpen {
        Self {
            inner: wc::CloseVsOpen::new(),
        }
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle_ohlc(open, high, low, close)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let c = make_candle_ohlc(open[i], high[i], low[i], close[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = BodySizePct)]
pub struct WasmBodySizePct {
    inner: wc::BodySizePct,
}

impl Default for WasmBodySizePct {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = BodySizePct)]
impl WasmBodySizePct {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmBodySizePct {
        Self {
            inner: wc::BodySizePct::new(),
        }
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle_ohlc(open, high, low, close)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let c = make_candle_ohlc(open[i], high[i], low[i], close[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = WickRatio)]
pub struct WasmWickRatio {
    inner: wc::WickRatio,
}

impl Default for WasmWickRatio {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = WickRatio)]
impl WasmWickRatio {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmWickRatio {
        Self {
            inner: wc::WickRatio::new(),
        }
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle_ohlc(open, high, low, close)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let c = make_candle_ohlc(open[i], high[i], low[i], close[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = HighLowRange)]
pub struct WasmHighLowRange {
    inner: wc::HighLowRange,
}

impl Default for WasmHighLowRange {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = HighLowRange)]
impl WasmHighLowRange {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmHighLowRange {
        Self {
            inner: wc::HighLowRange::new(),
        }
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle_ohlc(open, high, low, close)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let c = make_candle_ohlc(open[i], high[i], low[i], close[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = StochasticCCI)]
pub struct WasmStochasticCci {
    inner: wc::StochasticCci,
}

#[wasm_bindgen(js_class = StochasticCCI)]
impl WasmStochasticCci {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmStochasticCci, JsError> {
        Ok(Self {
            inner: wc::StochasticCci::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = IMI)]
pub struct WasmImi {
    inner: wc::IntradayMomentumIndex,
}

#[wasm_bindgen(js_class = IMI)]
impl WasmImi {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmImi, JsError> {
        Ok(Self {
            inner: wc::IntradayMomentumIndex::new(period).map_err(map_err)?,
        })
    }
    /// Batch over open/high/low/close arrays; `NaN` during warmup.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = open.len();
        if high.len() != n || low.len() != n || close.len() != n {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n];
        for i in 0..n {
            let c = make_candle_ohlc(open[i], high[i], low[i], close[i])?;
            if let Some(v) = self.inner.update(c) {
                out[i] = v;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Streaming update over one candle's open/high/low/close.
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle_ohlc(open, high, low, close)?;
        Ok(self.inner.update(c))
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = QQE)]
pub struct WasmQqe {
    inner: wc::Qqe,
}

#[wasm_bindgen(js_class = QQE)]
impl WasmQqe {
    #[wasm_bindgen(constructor)]
    pub fn new(rsi_period: usize, smoothing: usize, factor: f64) -> Result<WasmQqe, JsError> {
        Ok(Self {
            inner: wc::Qqe::new(rsi_period, smoothing, factor).map_err(map_err)?,
        })
    }
    /// Returns `[rsiMa0, trailing0, rsiMa1, trailing1, ...]`, length `2 * n`.
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let mut out = vec![f64::NAN; prices.len() * 2];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 2] = o.rsi_ma;
                out[i * 2 + 1] = o.trailing_line;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Streaming update. Returns `{ rsiMa, trailingLine }` once warm, else `undefined`.
    pub fn update(&mut self, value: f64) -> Option<WasmQqeValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj = js_object!("rsiMa" => o.rsi_ma, "trailingLine" => o.trailing_line);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ElderRay)]
pub struct WasmElderRay {
    inner: wc::ElderRay,
}

#[wasm_bindgen(js_class = ElderRay)]
impl WasmElderRay {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmElderRay, JsError> {
        Ok(Self {
            inner: wc::ElderRay::new(period).map_err(map_err)?,
        })
    }
    /// Returns `[bull0, bear0, bull1, bear1, ...]`, length `2 * n`.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.bull_power;
                out[i * 2 + 1] = o.bear_power;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Streaming update. Returns `{ bullPower, bearPower }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmElderRayValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("bullPower" => o.bull_power, "bearPower" => o.bear_power);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = TTM_TREND)]
pub struct WasmTtmTrend {
    inner: wc::TtmTrend,
}

#[wasm_bindgen(js_class = TTM_TREND)]
impl WasmTtmTrend {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmTtmTrend, JsError> {
        Ok(Self {
            inner: wc::TtmTrend::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = Qstick)]
pub struct WasmQstick {
    inner: wc::Qstick,
}

#[wasm_bindgen(js_class = Qstick)]
impl WasmQstick {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmQstick, JsError> {
        Ok(Self {
            inner: wc::Qstick::new(period).map_err(map_err)?,
        })
    }
    /// Batch over open/close arrays; `NaN` during warmup.
    pub fn batch(&mut self, open: &[f64], close: &[f64]) -> Result<Float64Array, JsError> {
        let n = open.len();
        if close.len() != n {
            return Err(JsError::new("open, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n];
        for i in 0..n {
            let hi = open[i].max(close[i]);
            let lo = open[i].min(close[i]);
            let c = make_candle_ohlc(open[i], hi, lo, close[i])?;
            if let Some(v) = self.inner.update(c) {
                out[i] = v;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Streaming update over one candle's open and close.
    pub fn update(&mut self, open: f64, close: f64) -> Result<Option<f64>, JsError> {
        let hi = open.max(close);
        let lo = open.min(close);
        let c = make_candle_ohlc(open, hi, lo, close)?;
        Ok(self.inner.update(c))
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = GatorOscillator)]
pub struct WasmGatorOscillator {
    inner: wc::GatorOscillator,
}

#[wasm_bindgen(js_class = GatorOscillator)]
impl WasmGatorOscillator {
    #[wasm_bindgen(constructor)]
    pub fn new(
        jaw_period: usize,
        teeth_period: usize,
        lips_period: usize,
    ) -> Result<WasmGatorOscillator, JsError> {
        Ok(Self {
            inner: wc::GatorOscillator::new(jaw_period, teeth_period, lips_period)
                .map_err(map_err)?,
        })
    }
    /// Returns `[upper0, lower0, upper1, lower1, ...]`, length `2 * n`.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.upper;
                out[i * 2 + 1] = o.lower;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Streaming update. Returns `{ upper, lower }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmGatorOscillatorValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = KasePermissionStochastic)]
pub struct WasmKasePermissionStochastic {
    inner: wc::KasePermissionStochastic,
}

#[wasm_bindgen(js_class = KasePermissionStochastic)]
impl WasmKasePermissionStochastic {
    #[wasm_bindgen(constructor)]
    pub fn new(length: usize, smooth: usize) -> Result<WasmKasePermissionStochastic, JsError> {
        Ok(Self {
            inner: wc::KasePermissionStochastic::new(length, smooth).map_err(map_err)?,
        })
    }
    /// Returns `[fast0, slow0, fast1, slow1, ...]`, length `2 * n`.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.fast;
                out[i * 2 + 1] = o.slow;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Streaming update. Returns `{ fast, slow }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmKasePermissionStochasticValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("fast" => o.fast, "slow" => o.slow);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = VolatilityRatio)]
pub struct WasmVolatilityRatio {
    inner: wc::VolatilityRatio,
}

#[wasm_bindgen(js_class = VolatilityRatio)]
impl WasmVolatilityRatio {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmVolatilityRatio, JsError> {
        Ok(Self {
            inner: wc::VolatilityRatio::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ProjectionOscillator)]
pub struct WasmProjectionOscillator {
    inner: wc::ProjectionOscillator,
}

#[wasm_bindgen(js_class = ProjectionOscillator)]
impl WasmProjectionOscillator {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmProjectionOscillator, JsError> {
        Ok(Self {
            inner: wc::ProjectionOscillator::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = TimeBasedStop)]
pub struct WasmTimeBasedStop {
    inner: wc::TimeBasedStop,
}

#[wasm_bindgen(js_class = TimeBasedStop)]
impl WasmTimeBasedStop {
    #[wasm_bindgen(constructor)]
    pub fn new(max_bars: usize) -> Result<WasmTimeBasedStop, JsError> {
        Ok(Self {
            inner: wc::TimeBasedStop::new(max_bars).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ADAPTIVECCI)]
pub struct WasmAdaptiveCci {
    inner: wc::AdaptiveCci,
}

#[wasm_bindgen(js_class = ADAPTIVECCI)]
impl WasmAdaptiveCci {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmAdaptiveCci, JsError> {
        Ok(Self {
            inner: wc::AdaptiveCci::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = Stochastic)]
pub struct WasmStoch {
    inner: wc::Stochastic,
}

#[wasm_bindgen(js_class = Stochastic)]
impl WasmStoch {
    #[wasm_bindgen(constructor)]
    pub fn new(k_period: usize, d_period: usize) -> Result<WasmStoch, JsError> {
        Ok(Self {
            inner: wc::Stochastic::new(k_period, d_period).map_err(map_err)?,
        })
    }
    /// Returns `[k0, d0, k1, d1, ...]`, length `2 * n`.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.k;
                out[i * 2 + 1] = o.d;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Streaming update. Returns `{ k, d }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmStochValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("k" => o.k, "d" => o.d);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = OBV)]
pub struct WasmObv {
    inner: wc::Obv,
}

impl Default for WasmObv {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = OBV)]
impl WasmObv {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmObv {
        Self {
            inner: wc::Obv::new(),
        }
    }
    pub fn batch(&mut self, close: &[f64], volume: &[f64]) -> Result<Float64Array, JsError> {
        if close.len() != volume.len() {
            return Err(JsError::new("close and volume must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(close[i], close[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    pub fn update(&mut self, close: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(close, close, close, volume)?;
        Ok(self.inner.update(c))
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = UltimateOscillator)]
pub struct WasmUltimateOscillator {
    inner: wc::UltimateOscillator,
}

#[wasm_bindgen(js_class = UltimateOscillator)]
impl WasmUltimateOscillator {
    #[wasm_bindgen(constructor)]
    pub fn new(short: usize, mid: usize, long: usize) -> Result<WasmUltimateOscillator, JsError> {
        Ok(Self {
            inner: wc::UltimateOscillator::new(short, mid, long).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ADL)]
pub struct WasmAdl {
    inner: wc::Adl,
}

impl Default for WasmAdl {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = ADL)]
impl WasmAdl {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmAdl {
        Self {
            inner: wc::Adl::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n || volume.len() != n {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = VolumePriceTrend)]
pub struct WasmVolumePriceTrend {
    inner: wc::VolumePriceTrend,
}

impl Default for WasmVolumePriceTrend {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = VolumePriceTrend)]
impl WasmVolumePriceTrend {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmVolumePriceTrend {
        Self {
            inner: wc::VolumePriceTrend::new(),
        }
    }
    pub fn update(&mut self, close: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(close, close, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, close: &[f64], volume: &[f64]) -> Result<Float64Array, JsError> {
        if close.len() != volume.len() {
            return Err(JsError::new("close and volume must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(close[i], close[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ChaikinMoneyFlow)]
pub struct WasmChaikinMoneyFlow {
    inner: wc::ChaikinMoneyFlow,
}

#[wasm_bindgen(js_class = ChaikinMoneyFlow)]
impl WasmChaikinMoneyFlow {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmChaikinMoneyFlow, JsError> {
        Ok(Self {
            inner: wc::ChaikinMoneyFlow::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n || volume.len() != n {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ChaikinOscillator)]
pub struct WasmChaikinOscillator {
    inner: wc::ChaikinOscillator,
}

#[wasm_bindgen(js_class = ChaikinOscillator)]
impl WasmChaikinOscillator {
    #[wasm_bindgen(constructor)]
    pub fn new(fast: usize, slow: usize) -> Result<WasmChaikinOscillator, JsError> {
        Ok(Self {
            inner: wc::ChaikinOscillator::new(fast, slow).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n || volume.len() != n {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        // Every bar is validated first, as `update` would build it, so a bad
        // bar is refused before the indicator consumes anything.
        for i in 0..n {
            make_candle(high[i], low[i], close[i], volume[i])?;
        }
        let mut out = vec![0.0; n];
        self.inner
            .batch_hlcv_into(high, low, close, volume, &mut out);
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n || volume.len() != n {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        // Every bar is validated first, as `update` would build it, so a bad
        // bar is refused before the indicator consumes anything.
        for i in 0..n {
            make_candle(high[i], low[i], close[i], volume[i])?;
        }
        let mut out = vec![0.0; n];
        self.inner
            .batch_hlcv_fast_into(high, low, close, volume, &mut out);
        Ok(Float64Array::from(out.as_slice()))
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        out: &mut [f64],
    ) -> Result<(), JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n || volume.len() != n {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        // Every bar is validated first, as `update` would build it, so a bad
        // bar is refused before the indicator consumes anything.
        for i in 0..n {
            make_candle(high[i], low[i], close[i], volume[i])?;
        }
        check_rows(out, n)?;
        self.inner.batch_hlcv_into(high, low, close, volume, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        out: &mut [f64],
    ) -> Result<(), JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n || volume.len() != n {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        // Every bar is validated first, as `update` would build it, so a bad
        // bar is refused before the indicator consumes anything.
        for i in 0..n {
            make_candle(high[i], low[i], close[i], volume[i])?;
        }
        check_rows(out, n)?;
        self.inner
            .batch_hlcv_fast_into(high, low, close, volume, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ForceIndex)]
pub struct WasmForceIndex {
    inner: wc::ForceIndex,
}

#[wasm_bindgen(js_class = ForceIndex)]
impl WasmForceIndex {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmForceIndex, JsError> {
        Ok(Self {
            inner: wc::ForceIndex::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, close: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(close, close, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, close: &[f64], volume: &[f64]) -> Result<Float64Array, JsError> {
        if close.len() != volume.len() {
            return Err(JsError::new("close and volume must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(close[i], close[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = VolumeOscillator)]
pub struct WasmVolumeOscillator {
    inner: wc::VolumeOscillator,
}

#[wasm_bindgen(js_class = VolumeOscillator)]
impl WasmVolumeOscillator {
    #[wasm_bindgen(constructor)]
    pub fn new(fast: usize, slow: usize) -> Result<WasmVolumeOscillator, JsError> {
        Ok(Self {
            inner: wc::VolumeOscillator::new(fast, slow).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(10.0, 10.0, 10.0, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, volume: &[f64]) -> Result<Float64Array, JsError> {
        let mut out = Vec::with_capacity(volume.len());
        for &v in volume {
            let c = make_candle(10.0, 10.0, 10.0, v)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = NVI)]
pub struct WasmNvi {
    inner: wc::Nvi,
}

#[wasm_bindgen(js_class = NVI)]
impl WasmNvi {
    #[wasm_bindgen(constructor)]
    pub fn new(baseline: Option<f64>) -> WasmNvi {
        Self {
            inner: wc::Nvi::with_baseline(baseline.unwrap_or(1000.0)),
        }
    }
    pub fn update(&mut self, close: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(close, close, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, close: &[f64], volume: &[f64]) -> Result<Float64Array, JsError> {
        if close.len() != volume.len() {
            return Err(JsError::new("close and volume must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(close[i], close[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = PVI)]
pub struct WasmPvi {
    inner: wc::Pvi,
}

#[wasm_bindgen(js_class = PVI)]
impl WasmPvi {
    #[wasm_bindgen(constructor)]
    pub fn new(baseline: Option<f64>) -> WasmPvi {
        Self {
            inner: wc::Pvi::with_baseline(baseline.unwrap_or(1000.0)),
        }
    }
    pub fn update(&mut self, close: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(close, close, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, close: &[f64], volume: &[f64]) -> Result<Float64Array, JsError> {
        if close.len() != volume.len() {
            return Err(JsError::new("close and volume must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(close[i], close[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = KVO)]
pub struct WasmKvo {
    inner: wc::Kvo,
}

#[wasm_bindgen(js_class = KVO)]
impl WasmKvo {
    #[wasm_bindgen(constructor)]
    pub fn new(fast: usize, slow: usize) -> Result<WasmKvo, JsError> {
        Ok(Self {
            inner: wc::Kvo::new(fast, slow).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n || volume.len() != n {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ADOSC)]
pub struct WasmAdOscillator {
    inner: wc::AdOscillator,
}

#[wasm_bindgen(js_class = ADOSC)]
impl WasmAdOscillator {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmAdOscillator {
        Self {
            inner: wc::AdOscillator::new(),
        }
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = AnchoredRSI)]
pub struct WasmAnchoredRsi {
    inner: wc::AnchoredRsi,
}

#[wasm_bindgen(js_class = AnchoredRSI)]
impl WasmAnchoredRsi {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmAnchoredRsi {
        Self {
            inner: wc::AnchoredRsi::new(),
        }
    }
    #[wasm_bindgen(js_name = setAnchor)]
    pub fn set_anchor(&mut self) {
        self.inner.set_anchor();
    }
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_nan(prices).as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_fast(prices).as_slice())
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_nan_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_fast_into(prices, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = AnchoredVWAP)]
pub struct WasmAnchoredVwap {
    inner: wc::AnchoredVwap,
}

#[wasm_bindgen(js_class = AnchoredVWAP)]
impl WasmAnchoredVwap {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmAnchoredVwap {
        Self {
            inner: wc::AnchoredVwap::new(),
        }
    }
    #[wasm_bindgen(js_name = setAnchor)]
    pub fn set_anchor(&mut self) {
        self.inner.set_anchor();
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n || volume.len() != n {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = DemandIndex)]
pub struct WasmDemandIndex {
    inner: wc::DemandIndex,
}

#[wasm_bindgen(js_class = DemandIndex)]
impl WasmDemandIndex {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmDemandIndex, JsError> {
        Ok(Self {
            inner: wc::DemandIndex::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n || volume.len() != n {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = TSV)]
pub struct WasmTsv {
    inner: wc::Tsv,
}

#[wasm_bindgen(js_class = TSV)]
impl WasmTsv {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmTsv, JsError> {
        Ok(Self {
            inner: wc::Tsv::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, close: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(close, close, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, close: &[f64], volume: &[f64]) -> Result<Float64Array, JsError> {
        if close.len() != volume.len() {
            return Err(JsError::new("close and volume must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(close[i], close[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = VZO)]
pub struct WasmVzo {
    inner: wc::Vzo,
}

#[wasm_bindgen(js_class = VZO)]
impl WasmVzo {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmVzo, JsError> {
        Ok(Self {
            inner: wc::Vzo::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, close: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(close, close, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, close: &[f64], volume: &[f64]) -> Result<Float64Array, JsError> {
        if close.len() != volume.len() {
            return Err(JsError::new("close and volume must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(close[i], close[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = MarketFacilitationIndex)]
pub struct WasmMarketFacilitationIndex {
    inner: wc::MarketFacilitationIndex,
}

#[wasm_bindgen(js_class = MarketFacilitationIndex)]
impl WasmMarketFacilitationIndex {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmMarketFacilitationIndex {
        Self {
            inner: wc::MarketFacilitationIndex::new(),
        }
    }
    pub fn update(&mut self, high: f64, low: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, low, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || volume.len() != n {
            return Err(JsError::new("high, low, volume must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], low[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = EaseOfMovement)]
pub struct WasmEaseOfMovement {
    inner: wc::EaseOfMovement,
}

#[wasm_bindgen(js_class = EaseOfMovement)]
impl WasmEaseOfMovement {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, divisor: f64) -> Result<WasmEaseOfMovement, JsError> {
        Ok(Self {
            inner: wc::EaseOfMovement::with_divisor(period, divisor).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, low, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || volume.len() != n {
            return Err(JsError::new("high, low, volume must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], low[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = SuperTrend)]
pub struct WasmSuperTrend {
    inner: wc::SuperTrend,
}

#[wasm_bindgen(js_class = SuperTrend)]
impl WasmSuperTrend {
    #[wasm_bindgen(constructor)]
    pub fn new(atr_period: usize, multiplier: f64) -> Result<WasmSuperTrend, JsError> {
        Ok(Self {
            inner: wc::SuperTrend::new(atr_period, multiplier).map_err(map_err)?,
        })
    }
    /// Returns `{ value, direction }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmSuperTrendValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("value" => o.value, "direction" => o.direction);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[value0, direction0, value1, direction1, ...]`, length `2 * n`.
    /// Warmup positions are NaN.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ChandelierExit)]
pub struct WasmChandelierExit {
    inner: wc::ChandelierExit,
}

#[wasm_bindgen(js_class = ChandelierExit)]
impl WasmChandelierExit {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, multiplier: f64) -> Result<WasmChandelierExit, JsError> {
        Ok(Self {
            inner: wc::ChandelierExit::new(period, multiplier).map_err(map_err)?,
        })
    }
    /// Returns `{ longStop, shortStop }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmChandelierExitValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("longStop" => o.long_stop, "shortStop" => o.short_stop);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[long0, short0, long1, short1, ...]`, length `2 * n`. Warmup is NaN.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.long_stop;
                out[i * 2 + 1] = o.short_stop;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ChandeKrollStop)]
pub struct WasmChandeKrollStop {
    inner: wc::ChandeKrollStop,
}

#[wasm_bindgen(js_class = ChandeKrollStop)]
impl WasmChandeKrollStop {
    #[wasm_bindgen(constructor)]
    pub fn new(
        atr_period: usize,
        atr_multiplier: f64,
        stop_period: usize,
    ) -> Result<WasmChandeKrollStop, JsError> {
        Ok(Self {
            inner: wc::ChandeKrollStop::new(atr_period, atr_multiplier, stop_period)
                .map_err(map_err)?,
        })
    }
    /// Returns `{ stopLong, stopShort }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmChandeKrollStopValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("stopLong" => o.stop_long, "stopShort" => o.stop_short);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[long0, short0, long1, short1, ...]`, length `2 * n`. Warmup is NaN.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.stop_long;
                out[i * 2 + 1] = o.stop_short;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = AtrTrailingStop)]
pub struct WasmAtrTrailingStop {
    inner: wc::AtrTrailingStop,
}

#[wasm_bindgen(js_class = AtrTrailingStop)]
impl WasmAtrTrailingStop {
    #[wasm_bindgen(constructor)]
    pub fn new(atr_period: usize, multiplier: f64) -> Result<WasmAtrTrailingStop, JsError> {
        Ok(Self {
            inner: wc::AtrTrailingStop::new(atr_period, multiplier).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Trailing Stops (family 09) ----------

#[wasm_bindgen(js_name = HiLoActivator)]
pub struct WasmHiLoActivator {
    inner: wc::HiLoActivator,
}

#[wasm_bindgen(js_class = HiLoActivator)]
impl WasmHiLoActivator {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmHiLoActivator, JsError> {
        Ok(Self {
            inner: wc::HiLoActivator::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = VoltyStop)]
pub struct WasmVoltyStop {
    inner: wc::VoltyStop,
}

#[wasm_bindgen(js_class = VoltyStop)]
impl WasmVoltyStop {
    #[wasm_bindgen(constructor)]
    pub fn new(atr_period: usize, multiplier: f64) -> Result<WasmVoltyStop, JsError> {
        Ok(Self {
            inner: wc::VoltyStop::new(atr_period, multiplier).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = YoyoExit)]
pub struct WasmYoyoExit {
    inner: wc::YoyoExit,
}

#[wasm_bindgen(js_class = YoyoExit)]
impl WasmYoyoExit {
    #[wasm_bindgen(constructor)]
    pub fn new(atr_period: usize, multiplier: f64) -> Result<WasmYoyoExit, JsError> {
        Ok(Self {
            inner: wc::YoyoExit::new(atr_period, multiplier).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = inTrade)]
    pub fn in_trade(&self) -> bool {
        self.inner.in_trade()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = DonchianStop)]
pub struct WasmDonchianStop {
    inner: wc::DonchianStop,
}

#[wasm_bindgen(js_class = DonchianStop)]
impl WasmDonchianStop {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmDonchianStop, JsError> {
        Ok(Self {
            inner: wc::DonchianStop::new(period).map_err(map_err)?,
        })
    }
    /// Returns `{ stopLong, stopShort }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmDonchianStopValue>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("stopLong" => o.stop_long, "stopShort" => o.stop_short);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[long0, short0, long1, short1, ...]`, length `2 * n`. Warmup is NaN.
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n {
            return Err(JsError::new("high and low must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.stop_long;
                out[i * 2 + 1] = o.stop_short;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = PercentageTrailingStop)]
pub struct WasmPercentageTrailingStop {
    inner: wc::PercentageTrailingStop,
}

#[wasm_bindgen(js_class = PercentageTrailingStop)]
impl WasmPercentageTrailingStop {
    #[wasm_bindgen(constructor)]
    pub fn new(percent: f64) -> Result<WasmPercentageTrailingStop, JsError> {
        Ok(Self {
            inner: wc::PercentageTrailingStop::new(percent).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_nan(prices).as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_fast(prices).as_slice())
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_nan_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_fast_into(prices, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = StepTrailingStop)]
pub struct WasmStepTrailingStop {
    inner: wc::StepTrailingStop,
}

#[wasm_bindgen(js_class = StepTrailingStop)]
impl WasmStepTrailingStop {
    #[wasm_bindgen(constructor)]
    pub fn new(step_size: f64) -> Result<WasmStepTrailingStop, JsError> {
        Ok(Self {
            inner: wc::StepTrailingStop::new(step_size).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_nan(prices).as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_fast(prices).as_slice())
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_nan_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_fast_into(prices, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = RenkoTrailingStop)]
pub struct WasmRenkoTrailingStop {
    inner: wc::RenkoTrailingStop,
}

#[wasm_bindgen(js_class = RenkoTrailingStop)]
impl WasmRenkoTrailingStop {
    #[wasm_bindgen(constructor)]
    pub fn new(block_size: f64) -> Result<WasmRenkoTrailingStop, JsError> {
        Ok(Self {
            inner: wc::RenkoTrailingStop::new(block_size).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_nan(prices).as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_fast(prices).as_slice())
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_nan_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_fast_into(prices, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = KaseDevStop)]
pub struct WasmKaseDevStop {
    inner: wc::KaseDevStop,
}

#[wasm_bindgen(js_class = KaseDevStop)]
impl WasmKaseDevStop {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, dev: f64) -> Result<WasmKaseDevStop, JsError> {
        Ok(Self {
            inner: wc::KaseDevStop::new(period, dev).map_err(map_err)?,
        })
    }
    /// Returns `{ value, direction }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmKaseDevStopValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("value" => o.value, "direction" => o.direction);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[value0, direction0, value1, direction1, ...]`, length `2 * n`.
    /// Warmup is NaN.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ElderSafeZone)]
pub struct WasmElderSafeZone {
    inner: wc::ElderSafeZone,
}

#[wasm_bindgen(js_class = ElderSafeZone)]
impl WasmElderSafeZone {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, coeff: f64) -> Result<WasmElderSafeZone, JsError> {
        Ok(Self {
            inner: wc::ElderSafeZone::new(period, coeff).map_err(map_err)?,
        })
    }
    /// Returns `{ value, direction }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmElderSafeZoneValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("value" => o.value, "direction" => o.direction);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[value0, direction0, value1, direction1, ...]`, length `2 * n`.
    /// Warmup is NaN.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = AtrRatchet)]
pub struct WasmAtrRatchet {
    inner: wc::AtrRatchet,
}

#[wasm_bindgen(js_class = AtrRatchet)]
impl WasmAtrRatchet {
    #[wasm_bindgen(constructor)]
    pub fn new(
        atr_period: usize,
        start_mult: f64,
        increment: f64,
    ) -> Result<WasmAtrRatchet, JsError> {
        Ok(Self {
            inner: wc::AtrRatchet::new(atr_period, start_mult, increment).map_err(map_err)?,
        })
    }
    /// Returns `{ value, direction }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmAtrRatchetValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("value" => o.value, "direction" => o.direction);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[value0, direction0, value1, direction1, ...]`, length `2 * n`.
    /// Warmup is NaN.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = Nrtr)]
pub struct WasmNrtr {
    inner: wc::Nrtr,
}

#[wasm_bindgen(js_class = Nrtr)]
impl WasmNrtr {
    #[wasm_bindgen(constructor)]
    pub fn new(pct: f64) -> Result<WasmNrtr, JsError> {
        Ok(Self {
            inner: wc::Nrtr::new(pct).map_err(map_err)?,
        })
    }
    /// Returns `{ value, direction }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmNrtrValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("value" => o.value, "direction" => o.direction);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[value0, direction0, value1, direction1, ...]`, length `2 * n`.
    /// Warmup is NaN.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ModifiedMaStop)]
pub struct WasmModifiedMaStop {
    inner: wc::ModifiedMaStop,
}

#[wasm_bindgen(js_class = ModifiedMaStop)]
impl WasmModifiedMaStop {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmModifiedMaStop, JsError> {
        Ok(Self {
            inner: wc::ModifiedMaStop::new(period).map_err(map_err)?,
        })
    }
    /// Returns `{ value, direction }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmModifiedMaStopValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("value" => o.value, "direction" => o.direction);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[value0, direction0, value1, direction1, ...]`, length `2 * n`.
    /// Warmup is NaN.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = TypicalPrice)]
pub struct WasmTypicalPrice {
    inner: wc::TypicalPrice,
}

impl Default for WasmTypicalPrice {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = TypicalPrice)]
impl WasmTypicalPrice {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmTypicalPrice {
        Self {
            inner: wc::TypicalPrice::new(),
        }
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = MedianPrice)]
pub struct WasmMedianPrice {
    inner: wc::MedianPrice,
}

impl Default for WasmMedianPrice {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = MedianPrice)]
impl WasmMedianPrice {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmMedianPrice {
        Self {
            inner: wc::MedianPrice::new(),
        }
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = WeightedClose)]
pub struct WasmWeightedClose {
    inner: wc::WeightedClose,
}

impl Default for WasmWeightedClose {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = WeightedClose)]
impl WasmWeightedClose {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmWeightedClose {
        Self {
            inner: wc::WeightedClose::new(),
        }
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = AcceleratorOscillator)]
pub struct WasmAcceleratorOscillator {
    inner: wc::AcceleratorOscillator,
}

#[wasm_bindgen(js_class = AcceleratorOscillator)]
impl WasmAcceleratorOscillator {
    #[wasm_bindgen(constructor)]
    pub fn new(
        ao_fast: usize,
        ao_slow: usize,
        signal_period: usize,
    ) -> Result<WasmAcceleratorOscillator, JsError> {
        Ok(Self {
            inner: wc::AcceleratorOscillator::new(ao_fast, ao_slow, signal_period)
                .map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = BalanceOfPower)]
pub struct WasmBalanceOfPower {
    inner: wc::BalanceOfPower,
}

impl Default for WasmBalanceOfPower {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = BalanceOfPower)]
impl WasmBalanceOfPower {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmBalanceOfPower {
        Self {
            inner: wc::BalanceOfPower::new(),
        }
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = open.len();
        if high.len() != n || low.len() != n || close.len() != n {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ChoppinessIndex)]
pub struct WasmChoppinessIndex {
    inner: wc::ChoppinessIndex,
}

#[wasm_bindgen(js_class = ChoppinessIndex)]
impl WasmChoppinessIndex {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmChoppinessIndex, JsError> {
        Ok(Self {
            inner: wc::ChoppinessIndex::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = TrueRange)]
pub struct WasmTrueRange {
    inner: wc::TrueRange,
}

impl Default for WasmTrueRange {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = TrueRange)]
impl WasmTrueRange {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmTrueRange {
        Self {
            inner: wc::TrueRange::new(),
        }
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ChaikinVolatility)]
pub struct WasmChaikinVolatility {
    inner: wc::ChaikinVolatility,
}

#[wasm_bindgen(js_class = ChaikinVolatility)]
impl WasmChaikinVolatility {
    #[wasm_bindgen(constructor)]
    pub fn new(ema_period: usize, roc_period: usize) -> Result<WasmChaikinVolatility, JsError> {
        Ok(Self {
            inner: wc::ChaikinVolatility::new(ema_period, roc_period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = NATR)]
pub struct WasmNatr {
    inner: wc::Natr,
}

#[wasm_bindgen(js_class = NATR)]
impl WasmNatr {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmNatr, JsError> {
        Ok(Self {
            inner: wc::Natr::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = AroonOscillator)]
pub struct WasmAroonOscillator {
    inner: wc::AroonOscillator,
}

#[wasm_bindgen(js_class = AroonOscillator)]
impl WasmAroonOscillator {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmAroonOscillator, JsError> {
        Ok(Self {
            inner: wc::AroonOscillator::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = Vortex)]
pub struct WasmVortex {
    inner: wc::Vortex,
}

#[wasm_bindgen(js_class = Vortex)]
impl WasmVortex {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmVortex, JsError> {
        Ok(Self {
            inner: wc::Vortex::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmVortexValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("plus" => o.plus, "minus" => o.minus);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[plus0, minus0, plus1, minus1, ...]`, length `2 * n`. Warmup is NaN.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.plus;
                out[i * 2 + 1] = o.minus;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = WaveTrend)]
pub struct WasmWaveTrend {
    inner: wc::WaveTrend,
}

#[wasm_bindgen(js_class = WaveTrend)]
impl WasmWaveTrend {
    #[wasm_bindgen(constructor)]
    pub fn new(
        channel_period: usize,
        average_period: usize,
        signal_period: usize,
    ) -> Result<WasmWaveTrend, JsError> {
        Ok(Self {
            inner: wc::WaveTrend::new(channel_period, average_period, signal_period)
                .map_err(map_err)?,
        })
    }
    pub fn classic() -> Result<WasmWaveTrend, JsError> {
        Ok(Self {
            inner: wc::WaveTrend::classic().map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmWaveTrendValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("wt1" => o.wt1, "wt2" => o.wt2);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.wt1;
                out[i * 2 + 1] = o.wt2;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = RWI)]
pub struct WasmRwi {
    inner: wc::Rwi,
}

#[wasm_bindgen(js_class = RWI)]
impl WasmRwi {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmRwi, JsError> {
        Ok(Self {
            inner: wc::Rwi::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmRwiValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("high" => o.high, "low" => o.low);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[high0, low0, high1, low1, ...]`, length `2 * n`. Warmup is NaN.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.high;
                out[i * 2 + 1] = o.low;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = MassIndex)]
pub struct WasmMassIndex {
    inner: wc::MassIndex,
}

#[wasm_bindgen(js_class = MassIndex)]
impl WasmMassIndex {
    #[wasm_bindgen(constructor)]
    pub fn new(ema_period: usize, sum_period: usize) -> Result<WasmMassIndex, JsError> {
        Ok(Self {
            inner: wc::MassIndex::new(ema_period, sum_period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = EVWMA)]
pub struct WasmEvwma {
    inner: wc::Evwma,
}

#[wasm_bindgen(js_class = EVWMA)]
impl WasmEvwma {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmEvwma, JsError> {
        Ok(Self {
            inner: wc::Evwma::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, close: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(close, close, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, close: &[f64], volume: &[f64]) -> Result<Float64Array, JsError> {
        if close.len() != volume.len() {
            return Err(JsError::new("close and volume must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(close[i], close[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = VWMA)]
pub struct WasmVwma {
    inner: wc::Vwma,
}

#[wasm_bindgen(js_class = VWMA)]
impl WasmVwma {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmVwma, JsError> {
        Ok(Self {
            inner: wc::Vwma::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, close: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(close, close, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, close: &[f64], volume: &[f64]) -> Result<Float64Array, JsError> {
        if close.len() != volume.len() {
            return Err(JsError::new("close and volume must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(close[i], close[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Whether enough input has arrived for `update` to produce a value.
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    /// How many inputs `update` needs before it first produces a value.
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ADX)]
pub struct WasmAdx {
    inner: wc::Adx,
}

#[wasm_bindgen(js_class = ADX)]
impl WasmAdx {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmAdx, JsError> {
        Ok(Self {
            inner: wc::Adx::new(period).map_err(map_err)?,
        })
    }
    /// Returns `[plusDi, minusDi, adx]` × n, length `3n`.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.plus_di;
                out[i * 3 + 1] = o.minus_di;
                out[i * 3 + 2] = o.adx;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Streaming update. Returns `{ plusDi, minusDi, adx }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmAdxValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj =
                    js_object!("plusDi" => o.plus_di, "minusDi" => o.minus_di, "adx" => o.adx);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ADXR)]
pub struct WasmAdxr {
    inner: wc::Adxr,
}

#[wasm_bindgen(js_class = ADXR)]
impl WasmAdxr {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmAdxr, JsError> {
        Ok(Self {
            inner: wc::Adxr::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = WilliamsR)]
pub struct WasmWilliamsR {
    inner: wc::WilliamsR,
}

#[wasm_bindgen(js_class = WilliamsR)]
impl WasmWilliamsR {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmWilliamsR, JsError> {
        Ok(Self {
            inner: wc::WilliamsR::new(period).map_err(map_err)?,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = CCI)]
pub struct WasmCci {
    inner: wc::Cci,
}

#[wasm_bindgen(js_class = CCI)]
impl WasmCci {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmCci, JsError> {
        Ok(Self {
            inner: wc::Cci::new(period).map_err(map_err)?,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = MFI)]
pub struct WasmMfi {
    inner: wc::Mfi,
}

#[wasm_bindgen(js_class = MFI)]
impl WasmMfi {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmMfi, JsError> {
        Ok(Self {
            inner: wc::Mfi::new(period).map_err(map_err)?,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = PSAR)]
pub struct WasmPsar {
    inner: wc::Psar,
}

#[wasm_bindgen(js_class = PSAR)]
impl WasmPsar {
    #[wasm_bindgen(constructor)]
    pub fn new(af_start: f64, af_step: f64, af_max: f64) -> Result<WasmPsar, JsError> {
        Ok(Self {
            inner: wc::Psar::new(af_start, af_step, af_max).map_err(map_err)?,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = Keltner)]
pub struct WasmKeltner {
    inner: wc::Keltner,
}

#[wasm_bindgen(js_class = Keltner)]
impl WasmKeltner {
    #[wasm_bindgen(constructor)]
    pub fn new(
        ema_period: usize,
        atr_period: usize,
        multiplier: f64,
    ) -> Result<WasmKeltner, JsError> {
        Ok(Self {
            inner: wc::Keltner::new(ema_period, atr_period, multiplier).map_err(map_err)?,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Streaming update. Returns `{ upper, middle, lower }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmKeltnerValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = Donchian)]
pub struct WasmDonchian {
    inner: wc::Donchian,
}

#[wasm_bindgen(js_class = Donchian)]
impl WasmDonchian {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmDonchian, JsError> {
        Ok(Self {
            inner: wc::Donchian::new(period).map_err(map_err)?,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Streaming update. Returns `{ upper, middle, lower }` once warm, else `undefined`.
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<WasmDonchianValue>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = VWAP)]
pub struct WasmVwap {
    inner: wc::Vwap,
}

impl Default for WasmVwap {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = VWAP)]
impl WasmVwap {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmVwap {
        Self {
            inner: wc::Vwap::new(),
        }
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = RollingVWAP)]
pub struct WasmRollingVwap {
    inner: wc::RollingVwap,
}

#[wasm_bindgen(js_class = RollingVWAP)]
impl WasmRollingVwap {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmRollingVwap, JsError> {
        Ok(Self {
            inner: wc::RollingVwap::new(period).map_err(map_err)?,
        })
    }
    #[wasm_bindgen(getter)]
    pub fn period(&self) -> usize {
        self.inner.period()
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = AwesomeOscillatorHistogram)]
pub struct WasmAoHist {
    inner: wc::AwesomeOscillatorHistogram,
}

#[wasm_bindgen(js_class = AwesomeOscillatorHistogram)]
impl WasmAoHist {
    #[wasm_bindgen(constructor)]
    pub fn new(fast: usize, slow: usize, sma_period: usize) -> Result<WasmAoHist, JsError> {
        Ok(Self {
            inner: wc::AwesomeOscillatorHistogram::new(fast, slow, sma_period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = AwesomeOscillator)]
pub struct WasmAo {
    inner: wc::AwesomeOscillator,
}

#[wasm_bindgen(js_class = AwesomeOscillator)]
impl WasmAo {
    #[wasm_bindgen(constructor)]
    pub fn new(fast: usize, slow: usize) -> Result<WasmAo, JsError> {
        Ok(Self {
            inner: wc::AwesomeOscillator::new(fast, slow).map_err(map_err)?,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = Alligator)]
pub struct WasmAlligator {
    inner: wc::Alligator,
}

#[wasm_bindgen(js_class = Alligator)]
impl WasmAlligator {
    #[wasm_bindgen(constructor)]
    pub fn new(jaw: usize, teeth: usize, lips: usize) -> Result<WasmAlligator, JsError> {
        Ok(Self {
            inner: wc::Alligator::new(jaw, teeth, lips).map_err(map_err)?,
        })
    }
    /// Returns `[jaw0, teeth0, lips0, jaw1, teeth1, lips1, ...]`, length `3n`.
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.jaw;
                out[i * 3 + 1] = o.teeth;
                out[i * 3 + 2] = o.lips;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Streaming update. Returns `{ jaw, teeth, lips }` once warm, else `undefined`.
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<WasmAlligatorValue>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("jaw" => o.jaw, "teeth" => o.teeth, "lips" => o.lips);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = Aroon)]
pub struct WasmAroon {
    inner: wc::Aroon,
}

#[wasm_bindgen(js_class = Aroon)]
impl WasmAroon {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmAroon, JsError> {
        Ok(Self {
            inner: wc::Aroon::new(period).map_err(map_err)?,
        })
    }
    /// Returns `[up0, down0, up1, down1, ...]`, length `2n`.
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.up;
                out[i * 2 + 1] = o.down;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Streaming update. Returns `{ up, down }` once warm, else `undefined`.
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<WasmAroonValue>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("up" => o.up, "down" => o.down);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Family 10: parameterless / multi-output ==============================

#[wasm_bindgen(js_name = HT_DCPHASE)]
pub struct WasmHtDcPhase {
    inner: wc::HtDcPhase,
}

#[wasm_bindgen(js_class = HT_DCPHASE)]
impl WasmHtDcPhase {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmHtDcPhase {
        Self {
            inner: wc::HtDcPhase::new(),
        }
    }
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_nan(prices).as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_fast(prices).as_slice())
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_nan_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_fast_into(prices, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = HT_TRENDMODE)]
pub struct WasmHtTrendMode {
    inner: wc::HtTrendMode,
}

#[wasm_bindgen(js_class = HT_TRENDMODE)]
impl WasmHtTrendMode {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmHtTrendMode {
        Self {
            inner: wc::HtTrendMode::new(),
        }
    }
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_nan(prices).as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_fast(prices).as_slice())
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_nan_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_fast_into(prices, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = HilbertDominantCycle)]
pub struct WasmHilbertDominantCycle {
    inner: wc::HilbertDominantCycle,
}

#[wasm_bindgen(js_class = HilbertDominantCycle)]
impl WasmHilbertDominantCycle {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmHilbertDominantCycle {
        Self {
            inner: wc::HilbertDominantCycle::new(),
        }
    }
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_nan(prices).as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_fast(prices).as_slice())
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_nan_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_fast_into(prices, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = AdaptiveCycle)]
pub struct WasmAdaptiveCycle {
    inner: wc::AdaptiveCycle,
}

#[wasm_bindgen(js_class = AdaptiveCycle)]
impl WasmAdaptiveCycle {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmAdaptiveCycle {
        Self {
            inner: wc::AdaptiveCycle::new(),
        }
    }
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_nan(prices).as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_fast(prices).as_slice())
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_nan_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_fast_into(prices, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = SineWave)]
pub struct WasmSineWave {
    inner: wc::SineWave,
}

#[wasm_bindgen(js_class = SineWave)]
impl WasmSineWave {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmSineWave {
        Self {
            inner: wc::SineWave::new(),
        }
    }
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_nan(prices).as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_fast(prices).as_slice())
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_nan_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_fast_into(prices, out);
        Ok(())
    }
    pub fn lead(&self) -> f64 {
        self.inner.lead()
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = MAMA)]
pub struct WasmMama {
    inner: wc::Mama,
}

#[wasm_bindgen(js_class = MAMA)]
impl WasmMama {
    #[wasm_bindgen(constructor)]
    pub fn new(fast_limit: f64, slow_limit: f64) -> Result<WasmMama, JsError> {
        Ok(Self {
            inner: wc::Mama::new(fast_limit, slow_limit).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<WasmMamaValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj = js_object!("mama" => o.mama, "fama" => o.fama);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Returns a flat `Float64Array` of length `2 * n`: `[mama0, fama0, mama1, fama1, ...]`.
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 2];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 2] = o.mama;
                out[i * 2 + 1] = o.fama;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Family 05: Bands & Channels ==============================

// Every indicator below is multi-output (2-5 bands), so they bypass the
// `wasm_scalar_indicator!` macro and follow the hand-rolled pattern used by
// Bollinger Bands, Keltner Channels, etc. above: `update` returns a JS object
// via `Object::new` + `Reflect::set`; `batch` returns a flat interleaved
// `Float64Array`.

// ---------- MA Envelope (scalar input, 3 outputs) ----------

#[wasm_bindgen(js_name = MaEnvelope)]
pub struct WasmMaEnvelope {
    inner: wc::MaEnvelope,
}

#[wasm_bindgen(js_class = MaEnvelope)]
impl WasmMaEnvelope {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, percent: f64) -> Result<WasmMaEnvelope, JsError> {
        Ok(Self {
            inner: wc::MaEnvelope::new(period, percent).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<WasmMaEnvelopeValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Flat `[upper0, middle0, lower0, upper1, ...]`, length `3 * n`.
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Acceleration Bands ----------

#[wasm_bindgen(js_name = AccelerationBands)]
pub struct WasmAccelerationBands {
    inner: wc::AccelerationBands,
}

#[wasm_bindgen(js_class = AccelerationBands)]
impl WasmAccelerationBands {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, factor: f64) -> Result<WasmAccelerationBands, JsError> {
        Ok(Self {
            inner: wc::AccelerationBands::new(period, factor).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmAccelerationBandsValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[u0, m0, l0, u1, m1, l1, ...]`, length `3 * n`.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- STARC Bands ----------

#[wasm_bindgen(js_name = StarcBands)]
pub struct WasmStarcBands {
    inner: wc::StarcBands,
}

#[wasm_bindgen(js_class = StarcBands)]
impl WasmStarcBands {
    #[wasm_bindgen(constructor)]
    pub fn new(
        sma_period: usize,
        atr_period: usize,
        multiplier: f64,
    ) -> Result<WasmStarcBands, JsError> {
        Ok(Self {
            inner: wc::StarcBands::new(sma_period, atr_period, multiplier).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmStarcBandsValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- ATR Bands ----------

#[wasm_bindgen(js_name = AtrBands)]
pub struct WasmAtrBands {
    inner: wc::AtrBands,
}

#[wasm_bindgen(js_class = AtrBands)]
impl WasmAtrBands {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, multiplier: f64) -> Result<WasmAtrBands, JsError> {
        Ok(Self {
            inner: wc::AtrBands::new(period, multiplier).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmAtrBandsValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Hurst Channel ----------

#[wasm_bindgen(js_name = HurstChannel)]
pub struct WasmHurstChannel {
    inner: wc::HurstChannel,
}

#[wasm_bindgen(js_class = HurstChannel)]
impl WasmHurstChannel {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, multiplier: f64) -> Result<WasmHurstChannel, JsError> {
        Ok(Self {
            inner: wc::HurstChannel::new(period, multiplier).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmHurstChannelValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- LinReg Channel (scalar input) ----------

#[wasm_bindgen(js_name = LinRegChannel)]
pub struct WasmLinRegChannel {
    inner: wc::LinRegChannel,
}

#[wasm_bindgen(js_class = LinRegChannel)]
impl WasmLinRegChannel {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, multiplier: f64) -> Result<WasmLinRegChannel, JsError> {
        Ok(Self {
            inner: wc::LinRegChannel::new(period, multiplier).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<WasmLinRegChannelValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Standard Error Bands (scalar input) ----------

#[wasm_bindgen(js_name = StandardErrorBands)]
pub struct WasmStandardErrorBands {
    inner: wc::StandardErrorBands,
}

#[wasm_bindgen(js_class = StandardErrorBands)]
impl WasmStandardErrorBands {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, multiplier: f64) -> Result<WasmStandardErrorBands, JsError> {
        Ok(Self {
            inner: wc::StandardErrorBands::new(period, multiplier).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<WasmStandardErrorBandsValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Quartile Bands (scalar input, 3 outputs) ----------

#[wasm_bindgen(js_name = QuartileBands)]
pub struct WasmQuartileBands {
    inner: wc::QuartileBands,
}

#[wasm_bindgen(js_class = QuartileBands)]
impl WasmQuartileBands {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmQuartileBands, JsError> {
        Ok(Self {
            inner: wc::QuartileBands::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<WasmQuartileBandsValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Bomar Bands (scalar input, 3 outputs) ----------

#[wasm_bindgen(js_name = BomarBands)]
pub struct WasmBomarBands {
    inner: wc::BomarBands,
}

#[wasm_bindgen(js_class = BomarBands)]
impl WasmBomarBands {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, coverage: f64) -> Result<WasmBomarBands, JsError> {
        Ok(Self {
            inner: wc::BomarBands::new(period, coverage).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<WasmBomarBandsValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Median Channel (scalar input, 3 outputs) ----------

#[wasm_bindgen(js_name = MedianChannel)]
pub struct WasmMedianChannel {
    inner: wc::MedianChannel,
}

#[wasm_bindgen(js_class = MedianChannel)]
impl WasmMedianChannel {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, multiplier: f64) -> Result<WasmMedianChannel, JsError> {
        Ok(Self {
            inner: wc::MedianChannel::new(period, multiplier).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<WasmMedianChannelValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Projection Bands (high/low input, 3 outputs) ----------

#[wasm_bindgen(js_name = ProjectionBands)]
pub struct WasmProjectionBands {
    inner: wc::ProjectionBands,
}

#[wasm_bindgen(js_class = ProjectionBands)]
impl WasmProjectionBands {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmProjectionBands, JsError> {
        Ok(Self {
            inner: wc::ProjectionBands::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmProjectionBandsValue>, JsError> {
        let candle = make_candle(high, low, low, 0.0)?;
        match self.inner.update(candle) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower);
                Ok(Some(obj.unchecked_into()))
            }
            None => Ok(None),
        }
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = make_candle(high[i], low[i], low[i], 0.0)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Central Pivot Range (high/low/close input, 3 outputs) ----------

#[wasm_bindgen(js_name = CentralPivotRange)]
pub struct WasmCentralPivotRange {
    inner: wc::CentralPivotRange,
}

impl Default for WasmCentralPivotRange {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = CentralPivotRange)]
impl WasmCentralPivotRange {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmCentralPivotRange {
        Self {
            inner: wc::CentralPivotRange::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmCentralPivotRangeValue>, JsError> {
        let candle = make_candle(high, low, close, 0.0)?;
        match self.inner.update(candle) {
            Some(o) => {
                let obj = js_object!("pivot" => o.pivot, "tc" => o.tc, "bc" => o.bc);
                Ok(Some(obj.unchecked_into()))
            }
            None => Ok(None),
        }
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.pivot;
                out[i * 3 + 1] = o.tc;
                out[i * 3 + 2] = o.bc;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Murrey Math Lines (high/low input, 9 outputs) ----------

#[wasm_bindgen(js_name = MurreyMathLines)]
pub struct WasmMurreyMathLines {
    inner: wc::MurreyMathLines,
}

#[wasm_bindgen(js_class = MurreyMathLines)]
impl WasmMurreyMathLines {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmMurreyMathLines, JsError> {
        Ok(Self {
            inner: wc::MurreyMathLines::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmMurreyMathLinesValue>, JsError> {
        let candle = make_candle(high, low, low, 0.0)?;
        match self.inner.update(candle) {
            Some(o) => {
                let obj = js_object!("mm8_8" => o.mm8_8, "mm7_8" => o.mm7_8, "mm6_8" => o.mm6_8, "mm5_8" => o.mm5_8, "mm4_8" => o.mm4_8, "mm3_8" => o.mm3_8, "mm2_8" => o.mm2_8, "mm1_8" => o.mm1_8, "mm0_8" => o.mm0_8);
                Ok(Some(obj.unchecked_into()))
            }
            None => Ok(None),
        }
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 9];
        for i in 0..n {
            let candle = make_candle(high[i], low[i], low[i], 0.0)?;
            if let Some(o) = self.inner.update(candle) {
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
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Andrews Pitchfork (high/low input, 3 outputs) ----------

#[wasm_bindgen(js_name = AndrewsPitchfork)]
pub struct WasmAndrewsPitchfork {
    inner: wc::AndrewsPitchfork,
}

#[wasm_bindgen(js_class = AndrewsPitchfork)]
impl WasmAndrewsPitchfork {
    #[wasm_bindgen(constructor)]
    pub fn new(strength: usize) -> Result<WasmAndrewsPitchfork, JsError> {
        Ok(Self {
            inner: wc::AndrewsPitchfork::new(strength).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmAndrewsPitchforkValue>, JsError> {
        let candle = make_candle(high, low, low, 0.0)?;
        match self.inner.update(candle) {
            Some(o) => {
                let obj = js_object!("median" => o.median, "upper" => o.upper, "lower" => o.lower);
                Ok(Some(obj.unchecked_into()))
            }
            None => Ok(None),
        }
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = make_candle(high[i], low[i], low[i], 0.0)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.median;
                out[i * 3 + 1] = o.upper;
                out[i * 3 + 2] = o.lower;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Volume-Weighted S/R (high/low/volume input, 2 outputs) ----------

#[wasm_bindgen(js_name = VolumeWeightedSr)]
pub struct WasmVolumeWeightedSr {
    inner: wc::VolumeWeightedSr,
}

#[wasm_bindgen(js_class = VolumeWeightedSr)]
impl WasmVolumeWeightedSr {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmVolumeWeightedSr, JsError> {
        Ok(Self {
            inner: wc::VolumeWeightedSr::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        volume: f64,
    ) -> Result<Option<WasmVolumeWeightedSrValue>, JsError> {
        let candle = make_candle(high, low, low, volume)?;
        match self.inner.update(candle) {
            Some(o) => {
                let obj = js_object!("support" => o.support, "resistance" => o.resistance);
                Ok(Some(obj.unchecked_into()))
            }
            None => Ok(None),
        }
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(JsError::new("high, low, volume must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = make_candle(high[i], low[i], low[i], volume[i])?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.support;
                out[i * 2 + 1] = o.resistance;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Pivot Reversal (high/low/close input, scalar signal) ----------

#[wasm_bindgen(js_name = PivotReversal)]
pub struct WasmPivotReversal {
    inner: wc::PivotReversal,
}

#[wasm_bindgen(js_class = PivotReversal)]
impl WasmPivotReversal {
    #[wasm_bindgen(constructor)]
    pub fn new(left: usize, right: usize) -> Result<WasmPivotReversal, JsError> {
        Ok(Self {
            inner: wc::PivotReversal::new(left, right).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let candle = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(candle))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let candle = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Double Bollinger (scalar input, 5 outputs) ----------

#[wasm_bindgen(js_name = DoubleBollinger)]
pub struct WasmDoubleBollinger {
    inner: wc::DoubleBollinger,
}

#[wasm_bindgen(js_class = DoubleBollinger)]
impl WasmDoubleBollinger {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, k_inner: f64, k_outer: f64) -> Result<WasmDoubleBollinger, JsError> {
        Ok(Self {
            inner: wc::DoubleBollinger::new(period, k_inner, k_outer).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, value: f64) -> Option<WasmDoubleBollingerValue> {
        match self.inner.update(value) {
            Some(o) => {
                let obj = js_object!("upperOuter" => o.upper_outer, "upperInner" => o.upper_inner, "middle" => o.middle, "lowerInner" => o.lower_inner, "lowerOuter" => o.lower_outer);
                Some(obj.unchecked_into())
            }
            None => None,
        }
    }
    /// Flat `[u_o, u_i, m, l_i, l_o, ...]`, length `5 * n`.
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let n = prices.len();
        let mut out = vec![f64::NAN; n * 5];
        for (i, p) in prices.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 5] = o.upper_outer;
                out[i * 5 + 1] = o.upper_inner;
                out[i * 5 + 2] = o.middle;
                out[i * 5 + 3] = o.lower_inner;
                out[i * 5 + 4] = o.lower_outer;
            }
        }
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TTM Squeeze ----------

#[wasm_bindgen(js_name = TtmSqueeze)]
pub struct WasmTtmSqueeze {
    inner: wc::TtmSqueeze,
}

#[wasm_bindgen(js_class = TtmSqueeze)]
impl WasmTtmSqueeze {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, bb_mult: f64, kc_mult: f64) -> Result<WasmTtmSqueeze, JsError> {
        Ok(Self {
            inner: wc::TtmSqueeze::new(period, bb_mult, kc_mult).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmTtmSqueezeValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("squeeze" => o.squeeze, "momentum" => o.momentum);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Flat `[sq0, mom0, sq1, mom1, ...]`, length `2 * n`.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.squeeze;
                out[i * 2 + 1] = o.momentum;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Fractal Chaos Bands ----------

#[wasm_bindgen(js_name = FractalChaosBands)]
pub struct WasmFractalChaosBands {
    inner: wc::FractalChaosBands,
}

#[wasm_bindgen(js_class = FractalChaosBands)]
impl WasmFractalChaosBands {
    #[wasm_bindgen(constructor)]
    pub fn new(k: usize) -> Result<WasmFractalChaosBands, JsError> {
        Ok(Self {
            inner: wc::FractalChaosBands::new(k).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmFractalChaosBandsValue>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "lower" => o.lower);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Flat `[u0, l0, u1, l1, ...]`, length `2 * n`.
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n {
            return Err(JsError::new("high and low must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.upper;
                out[i * 2 + 1] = o.lower;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- VWAP StdDev Bands ----------

#[wasm_bindgen(js_name = VwapStdDevBands)]
pub struct WasmVwapStdDevBands {
    inner: wc::VwapStdDevBands,
}

#[wasm_bindgen(js_class = VwapStdDevBands)]
impl WasmVwapStdDevBands {
    #[wasm_bindgen(constructor)]
    pub fn new(multiplier: f64) -> Result<WasmVwapStdDevBands, JsError> {
        Ok(Self {
            inner: wc::VwapStdDevBands::new(multiplier).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<WasmVwapStdDevBandsValue>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("upper" => o.upper, "middle" => o.middle, "lower" => o.lower, "stddev" => o.stddev);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Flat `[u0, m0, l0, sd0, ...]`, length `4 * n`.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n || volume.len() != n {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], volume[i])?;
            if let Some(o) = self.inner.update(c) {
                out[i * 4] = o.upper;
                out[i * 4 + 1] = o.middle;
                out[i * 4 + 2] = o.lower;
                out[i * 4 + 3] = o.stddev;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Pivots & S/R ==============================

#[wasm_bindgen(js_name = ClassicPivots)]
pub struct WasmClassicPivots {
    inner: wc::ClassicPivots,
}

impl Default for WasmClassicPivots {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = ClassicPivots)]
impl WasmClassicPivots {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmClassicPivots {
        Self {
            inner: wc::ClassicPivots::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmClassicPivotsValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("pp" => o.pp, "r1" => o.r1, "r2" => o.r2, "r3" => o.r3, "s1" => o.s1, "s2" => o.s2, "s3" => o.s3);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 7];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 7] = o.pp;
                out[i * 7 + 1] = o.r1;
                out[i * 7 + 2] = o.r2;
                out[i * 7 + 3] = o.r3;
                out[i * 7 + 4] = o.s1;
                out[i * 7 + 5] = o.s2;
                out[i * 7 + 6] = o.s3;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = FibonacciPivots)]
pub struct WasmFibonacciPivots {
    inner: wc::FibonacciPivots,
}

impl Default for WasmFibonacciPivots {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = FibonacciPivots)]
impl WasmFibonacciPivots {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmFibonacciPivots {
        Self {
            inner: wc::FibonacciPivots::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmFibonacciPivotsValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("pp" => o.pp, "r1" => o.r1, "r2" => o.r2, "r3" => o.r3, "s1" => o.s1, "s2" => o.s2, "s3" => o.s3);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 7];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 7] = o.pp;
                out[i * 7 + 1] = o.r1;
                out[i * 7 + 2] = o.r2;
                out[i * 7 + 3] = o.r3;
                out[i * 7 + 4] = o.s1;
                out[i * 7 + 5] = o.s2;
                out[i * 7 + 6] = o.s3;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = Camarilla)]
pub struct WasmCamarilla {
    inner: wc::Camarilla,
}

impl Default for WasmCamarilla {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = Camarilla)]
impl WasmCamarilla {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmCamarilla {
        Self {
            inner: wc::Camarilla::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmCamarillaValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("pp" => o.pp, "r1" => o.r1, "r2" => o.r2, "r3" => o.r3, "r4" => o.r4, "s1" => o.s1, "s2" => o.s2, "s3" => o.s3, "s4" => o.s4);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 9];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
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
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = WoodiePivots)]
pub struct WasmWoodiePivots {
    inner: wc::WoodiePivots,
}

impl Default for WasmWoodiePivots {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = WoodiePivots)]
impl WasmWoodiePivots {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmWoodiePivots {
        Self {
            inner: wc::WoodiePivots::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmWoodiePivotsValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("pp" => o.pp, "r1" => o.r1, "r2" => o.r2, "s1" => o.s1, "s2" => o.s2);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 5];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 5] = o.pp;
                out[i * 5 + 1] = o.r1;
                out[i * 5 + 2] = o.r2;
                out[i * 5 + 3] = o.s1;
                out[i * 5 + 4] = o.s2;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = DemarkPivots)]
pub struct WasmDemarkPivots {
    inner: wc::DemarkPivots,
}

impl Default for WasmDemarkPivots {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = DemarkPivots)]
impl WasmDemarkPivots {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmDemarkPivots {
        Self {
            inner: wc::DemarkPivots::new(),
        }
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmDemarkPivotsValue>, JsError> {
        let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("pp" => o.pp, "r1" => o.r1, "s1" => o.s1);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let n = open.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.pp;
                out[i * 3 + 1] = o.r1;
                out[i * 3 + 2] = o.s1;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = WilliamsFractals)]
pub struct WasmWilliamsFractals {
    inner: wc::WilliamsFractals,
}

impl Default for WasmWilliamsFractals {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = WilliamsFractals)]
impl WasmWilliamsFractals {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmWilliamsFractals {
        Self {
            inner: wc::WilliamsFractals::new(),
        }
    }
    /// Returns `{ up, down }` where each is the fractal price or `NaN` when no
    /// fractal was confirmed at the centre of the most recent 5-bar window.
    /// Returns `undefined` during the four-bar warmup.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmWilliamsFractalsValue>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("up" => o.up.unwrap_or(f64::NAN), "down" => o.down.unwrap_or(f64::NAN));
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                if let Some(v) = o.up {
                    out[i * 2] = v;
                }
                if let Some(v) = o.down {
                    out[i * 2 + 1] = v;
                }
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ZigZag)]
pub struct WasmZigZag {
    inner: wc::ZigZag,
}

#[wasm_bindgen(js_class = ZigZag)]
impl WasmZigZag {
    #[wasm_bindgen(constructor)]
    pub fn new(threshold: f64) -> Result<WasmZigZag, JsError> {
        Ok(Self {
            inner: wc::ZigZag::new(threshold).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<WasmZigZagValue>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("swing" => o.swing, "direction" => o.direction);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.swing;
                out[i * 2 + 1] = o.direction;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    #[wasm_bindgen(js_name = threshold)]
    pub fn threshold(&self) -> f64 {
        self.inner.threshold()
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}
// ---------- TD Setup ----------

#[wasm_bindgen(js_name = TDSetup)]
pub struct WasmTdSetup {
    inner: wc::TdSetup,
}

#[wasm_bindgen(js_class = TDSetup)]
impl WasmTdSetup {
    #[wasm_bindgen(constructor)]
    pub fn new(lookback: usize, target: usize) -> Result<WasmTdSetup, JsError> {
        Ok(Self {
            inner: wc::TdSetup::new(lookback, target).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD Sequential ----------

#[wasm_bindgen(js_name = TDSequential)]
pub struct WasmTdSequential {
    inner: wc::TdSequential,
}

#[wasm_bindgen(js_class = TDSequential)]
impl WasmTdSequential {
    #[wasm_bindgen(constructor)]
    pub fn new(
        setup_lookback: usize,
        setup_target: usize,
        countdown_lookback: usize,
        countdown_target: usize,
    ) -> Result<WasmTdSequential, JsError> {
        Ok(Self {
            inner: wc::TdSequential::new(
                setup_lookback,
                setup_target,
                countdown_lookback,
                countdown_target,
            )
            .map_err(map_err)?,
        })
    }
    /// Streaming update. Returns `{ setup, countdown, direction }` once warm,
    /// else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmTdSequentialValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("setup" => o.setup, "countdown" => o.countdown, "direction" => o.direction);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Batch returns a flat `Float64Array` `[setup0, countdown0, direction0, ...]`.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.setup;
                out[i * 3 + 1] = o.countdown;
                out[i * 3 + 2] = o.direction;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD DeMarker ----------

#[wasm_bindgen(js_name = TDDeMarker)]
pub struct WasmTdDeMarker {
    inner: wc::TdDeMarker,
}

#[wasm_bindgen(js_class = TDDeMarker)]
impl WasmTdDeMarker {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmTdDeMarker, JsError> {
        Ok(Self {
            inner: wc::TdDeMarker::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD REI ----------

#[wasm_bindgen(js_name = TDREI)]
pub struct WasmTdRei {
    inner: wc::TdRei,
}

#[wasm_bindgen(js_class = TDREI)]
impl WasmTdRei {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmTdRei, JsError> {
        Ok(Self {
            inner: wc::TdRei::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, low, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], low[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD Pressure ----------

#[wasm_bindgen(js_name = TDPressure)]
pub struct WasmTdPressure {
    inner: wc::TdPressure,
}

#[wasm_bindgen(js_class = TDPressure)]
impl WasmTdPressure {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmTdPressure, JsError> {
        Ok(Self {
            inner: wc::TdPressure::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open.len() != high.len()
            || high.len() != low.len()
            || low.len() != close.len()
            || close.len() != volume.len()
        {
            return Err(JsError::new(
                "open, high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], volume[i], 0)
                .map_err(map_err)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD Combo ----------

#[wasm_bindgen(js_name = TDCombo)]
pub struct WasmTdCombo {
    inner: wc::TdCombo,
}

#[wasm_bindgen(js_class = TDCombo)]
impl WasmTdCombo {
    #[wasm_bindgen(constructor)]
    pub fn new(
        setup_lookback: usize,
        setup_target: usize,
        countdown_lookback: usize,
        countdown_target: usize,
    ) -> Result<WasmTdCombo, JsError> {
        Ok(Self {
            inner: wc::TdCombo::new(
                setup_lookback,
                setup_target,
                countdown_lookback,
                countdown_target,
            )
            .map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD D-Wave ----------

#[wasm_bindgen(js_name = TDDWave)]
pub struct WasmTdDWave {
    inner: wc::TdDWave,
}

#[wasm_bindgen(js_class = TDDWave)]
impl WasmTdDWave {
    #[wasm_bindgen(constructor)]
    pub fn new(strength: usize) -> Result<WasmTdDWave, JsError> {
        Ok(Self {
            inner: wc::TdDWave::new(strength).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD Moving Averages ----------

#[wasm_bindgen(js_name = TDMovingAverage)]
pub struct WasmTdMovingAverage {
    inner: wc::TdMovingAverage,
}

#[wasm_bindgen(js_class = TDMovingAverage)]
impl WasmTdMovingAverage {
    #[wasm_bindgen(constructor)]
    pub fn new(period_st1: usize, period_st2: usize) -> Result<WasmTdMovingAverage, JsError> {
        Ok(Self {
            inner: wc::TdMovingAverage::new(period_st1, period_st2).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmTdMovingAverageValue>, JsError> {
        let candle = make_candle(high, low, low, 0.0)?;
        match self.inner.update(candle) {
            Some(o) => {
                let obj = js_object!("st1" => o.st1, "st2" => o.st2);
                Ok(Some(obj.unchecked_into()))
            }
            None => Ok(None),
        }
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high, low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = make_candle(high[i], low[i], low[i], 0.0)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.st1;
                out[i * 2 + 1] = o.st2;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD Countdown ----------

#[wasm_bindgen(js_name = TDCountdown)]
pub struct WasmTdCountdown {
    inner: wc::TdCountdown,
}

#[wasm_bindgen(js_class = TDCountdown)]
impl WasmTdCountdown {
    #[wasm_bindgen(constructor)]
    pub fn new(
        setup_lookback: usize,
        setup_target: usize,
        countdown_lookback: usize,
        countdown_target: usize,
    ) -> Result<WasmTdCountdown, JsError> {
        Ok(Self {
            inner: wc::TdCountdown::new(
                setup_lookback,
                setup_target,
                countdown_lookback,
                countdown_target,
            )
            .map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD Lines ----------

#[wasm_bindgen(js_name = TDLines)]
pub struct WasmTdLines {
    inner: wc::TdLines,
}

#[wasm_bindgen(js_class = TDLines)]
impl WasmTdLines {
    #[wasm_bindgen(constructor)]
    pub fn new(lookback: usize, target: usize) -> Result<WasmTdLines, JsError> {
        Ok(Self {
            inner: wc::TdLines::new(lookback, target).map_err(map_err)?,
        })
    }
    /// Streaming update. Returns `{ resistance, support }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmTdLinesValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("resistance" => o.resistance, "support" => o.support);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Batch returns a flat `Float64Array` `[resistance0, support0, ...]`.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.resistance;
                out[i * 2 + 1] = o.support;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD Range Projection ----------

#[wasm_bindgen(js_name = TDRangeProjection)]
pub struct WasmTdRangeProjection {
    inner: wc::TdRangeProjection,
}

#[wasm_bindgen(js_class = TDRangeProjection)]
impl WasmTdRangeProjection {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmTdRangeProjection {
        Self {
            inner: wc::TdRangeProjection::new(),
        }
    }
    /// Streaming update. Returns `{ high, low }` projected for the next bar.
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmTdRangeProjectionValue>, JsError> {
        let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("high" => o.high, "low" => o.low);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Batch returns a flat `Float64Array` `[projHigh0, projLow0, ...]`.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let n = open.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            if let Some(p) = self.inner.update(c) {
                out[i * 2] = p.high;
                out[i * 2 + 1] = p.low;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD Differential ----------

#[wasm_bindgen(js_name = TDDifferential)]
pub struct WasmTdDifferential {
    inner: wc::TdDifferential,
}

#[wasm_bindgen(js_class = TDDifferential)]
impl WasmTdDifferential {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmTdDifferential {
        Self {
            inner: wc::TdDifferential::new(),
        }
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD Open ----------

#[wasm_bindgen(js_name = TDOpen)]
pub struct WasmTdOpen {
    inner: wc::TdOpen,
}

#[wasm_bindgen(js_class = TDOpen)]
impl WasmTdOpen {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WasmTdOpen {
        Self {
            inner: wc::TdOpen::new(),
        }
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- TD Risk Level ----------

#[wasm_bindgen(js_name = TDRiskLevel)]
pub struct WasmTdRiskLevel {
    inner: wc::TdRiskLevel,
}

#[wasm_bindgen(js_class = TDRiskLevel)]
impl WasmTdRiskLevel {
    #[wasm_bindgen(constructor)]
    pub fn new(lookback: usize, target: usize) -> Result<WasmTdRiskLevel, JsError> {
        Ok(Self {
            inner: wc::TdRiskLevel::new(lookback, target).map_err(map_err)?,
        })
    }
    /// Streaming update. Returns `{ buyRisk, sellRisk }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmTdRiskLevelValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("buyRisk" => o.buy_risk, "sellRisk" => o.sell_risk);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Batch returns a flat `Float64Array` `[buyRisk0, sellRisk0, ...]`.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.buy_risk;
                out[i * 2 + 1] = o.sell_risk;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Ichimoku ----------

#[wasm_bindgen(js_name = Ichimoku)]
pub struct WasmIchimoku {
    inner: wc::Ichimoku,
}

#[wasm_bindgen(js_class = Ichimoku)]
impl WasmIchimoku {
    #[wasm_bindgen(constructor)]
    pub fn new(
        tenkan_period: usize,
        kijun_period: usize,
        senkou_b_period: usize,
        displacement: usize,
    ) -> Result<WasmIchimoku, JsError> {
        Ok(Self {
            inner: wc::Ichimoku::new(tenkan_period, kijun_period, senkou_b_period, displacement)
                .map_err(map_err)?,
        })
    }
    /// Streaming update. Returns `{ tenkan, kijun, senkouA, senkouB, chikou }`
    /// with `NaN` for any line that is not yet defined.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmIchimokuValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("tenkan" => o.tenkan.unwrap_or(f64::NAN), "kijun" => o.kijun.unwrap_or(f64::NAN), "senkouA" => o.senkou_a.unwrap_or(f64::NAN), "senkouB" => o.senkou_b.unwrap_or(f64::NAN), "chikou" => o.chikou.unwrap_or(f64::NAN));
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[tenkan0, kijun0, senkouA0, senkouB0, chikou0, tenkan1, ...]`,
    /// length `5 * n`. Cells without a defined value are `NaN`.
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 5];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
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
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Heikin-Ashi ----------

#[wasm_bindgen(js_name = HeikinAshi)]
pub struct WasmHeikinAshi {
    inner: wc::HeikinAshi,
}

impl Default for WasmHeikinAshi {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = HeikinAshi)]
impl WasmHeikinAshi {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmHeikinAshi {
        Self {
            inner: wc::HeikinAshi::new(),
        }
    }
    /// Streaming update. Returns `{ open, high, low, close }` of the
    /// Heikin-Ashi candle.
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmHeikinAshiValue>, JsError> {
        let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("open" => o.open, "high" => o.high, "low" => o.low, "close" => o.close);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[open0, high0, low0, close0, open1, ...]`, length `4 * n`.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("open, high, low, close must be equal length"));
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
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = ValueArea)]
pub struct WasmValueArea {
    inner: wc::ValueArea,
}

#[wasm_bindgen(js_class = ValueArea)]
impl WasmValueArea {
    #[wasm_bindgen(constructor)]
    pub fn new(
        period: usize,
        bin_count: usize,
        value_area_pct: f64,
    ) -> Result<WasmValueArea, JsError> {
        Ok(Self {
            inner: wc::ValueArea::new(period, bin_count, value_area_pct).map_err(map_err)?,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(JsError::new("high, low, volume must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let mid = f64::midpoint(high[i], low[i]);
            let c = wc::Candle::new(mid, high[i], low[i], mid, volume[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.poc;
                out[i * 3 + 1] = o.vah;
                out[i * 3 + 2] = o.val;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Streaming update. Returns `{ poc, vah, val }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        volume: f64,
    ) -> Result<Option<WasmValueAreaValue>, JsError> {
        let mid = f64::midpoint(high, low);
        let c = wc::Candle::new(mid, high, low, mid, volume, 0).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("poc" => o.poc, "vah" => o.vah, "val" => o.val);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// Naked POC: most recent untouched point-of-control level (Candle -> f64).
#[wasm_bindgen(js_name = NakedPoc)]
pub struct WasmNakedPoc {
    inner: wc::NakedPoc,
}

#[wasm_bindgen(js_class = NakedPoc)]
impl WasmNakedPoc {
    #[wasm_bindgen(constructor)]
    pub fn new(session_len: usize, bin_count: usize) -> Result<WasmNakedPoc, JsError> {
        Ok(Self {
            inner: wc::NakedPoc::new(session_len, bin_count).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(make_candle(high, low, close, volume)?))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            out.push(
                self.inner
                    .update(make_candle(high[i], low[i], close[i], volume[i])?)
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// Single Prints: count of single-print price levels (Candle -> f64).
#[wasm_bindgen(js_name = SinglePrints)]
pub struct WasmSinglePrints {
    inner: wc::SinglePrints,
}

#[wasm_bindgen(js_class = SinglePrints)]
impl WasmSinglePrints {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, bin_count: usize) -> Result<WasmSinglePrints, JsError> {
        Ok(Self {
            inner: wc::SinglePrints::new(period, bin_count).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<f64>, JsError> {
        let mid = f64::midpoint(high, low);
        Ok(self
            .inner
            .update(wc::Candle::new(mid, high, low, mid, 0.0, 0).map_err(map_err)?))
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
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
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// Profile Shape: b/P/D classification as a numeric code (Candle -> f64).
#[wasm_bindgen(js_name = ProfileShape)]
pub struct WasmProfileShape {
    inner: wc::ProfileShape,
}

#[wasm_bindgen(js_class = ProfileShape)]
impl WasmProfileShape {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, bin_count: usize) -> Result<WasmProfileShape, JsError> {
        Ok(Self {
            inner: wc::ProfileShape::new(period, bin_count).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let mid = f64::midpoint(high, low);
        Ok(self
            .inner
            .update(wc::Candle::new(mid, high, low, mid, volume, 0).map_err(map_err)?))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(JsError::new("high, low, volume must be equal length"));
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
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// High/Low Volume Nodes: highest- and lowest-volume price nodes (Candle -> struct).
#[wasm_bindgen(js_name = HighLowVolumeNodes)]
pub struct WasmHighLowVolumeNodes {
    inner: wc::HighLowVolumeNodes,
}

#[wasm_bindgen(js_class = HighLowVolumeNodes)]
impl WasmHighLowVolumeNodes {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, bin_count: usize) -> Result<WasmHighLowVolumeNodes, JsError> {
        Ok(Self {
            inner: wc::HighLowVolumeNodes::new(period, bin_count).map_err(map_err)?,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(JsError::new("high, low, volume must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let mid = f64::midpoint(high[i], low[i]);
            let c = wc::Candle::new(mid, high[i], low[i], mid, volume[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.hvn;
                out[i * 2 + 1] = o.lvn;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Streaming update. Returns `{ hvn, lvn }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        volume: f64,
    ) -> Result<Option<WasmHighLowVolumeNodesValue>, JsError> {
        let mid = f64::midpoint(high, low);
        let c = wc::Candle::new(mid, high, low, mid, volume, 0).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("hvn" => o.hvn, "lvn" => o.lvn);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// Composite Profile: multi-session composite volume profile (Candle -> struct).
#[wasm_bindgen(js_name = CompositeProfile)]
pub struct WasmCompositeProfile {
    inner: wc::CompositeProfile,
}

#[wasm_bindgen(js_class = CompositeProfile)]
impl WasmCompositeProfile {
    #[wasm_bindgen(constructor)]
    pub fn new(
        period: usize,
        bin_count: usize,
        value_area_pct: f64,
    ) -> Result<WasmCompositeProfile, JsError> {
        Ok(Self {
            inner: wc::CompositeProfile::new(period, bin_count, value_area_pct).map_err(map_err)?,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(JsError::new("high, low, volume must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let mid = f64::midpoint(high[i], low[i]);
            let c = wc::Candle::new(mid, high[i], low[i], mid, volume[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.poc;
                out[i * 3 + 1] = o.vah;
                out[i * 3 + 2] = o.val;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Streaming update. Returns `{ poc, vah, val }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        volume: f64,
    ) -> Result<Option<WasmCompositeProfileValue>, JsError> {
        let mid = f64::midpoint(high, low);
        let c = wc::Candle::new(mid, high, low, mid, volume, 0).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("poc" => o.poc, "vah" => o.vah, "val" => o.val);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = VolumeProfile)]
pub struct WasmVolumeProfile {
    inner: wc::VolumeProfile,
}

#[wasm_bindgen(js_class = VolumeProfile)]
impl WasmVolumeProfile {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, bin_count: usize) -> Result<WasmVolumeProfile, JsError> {
        Ok(Self {
            inner: wc::VolumeProfile::new(period, bin_count).map_err(map_err)?,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(JsError::new("high, low, volume must be equal length"));
        }
        let k = self.inner.params().1 + 2;
        let n = high.len();
        let mut out = vec![f64::NAN; n * k];
        for i in 0..n {
            let mid = f64::midpoint(high[i], low[i]);
            let c = wc::Candle::new(mid, high[i], low[i], mid, volume[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * k] = o.price_low;
                out[i * k + 1] = o.price_high;
                for (j, b) in o.bins.iter().enumerate() {
                    out[i * k + 2 + j] = *b;
                }
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Streaming update. Returns `{ priceLow, priceHigh, bins }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        volume: f64,
    ) -> Result<Option<WasmVolumeProfileValue>, JsError> {
        let mid = f64::midpoint(high, low);
        let c = wc::Candle::new(mid, high, low, mid, volume, 0).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("priceLow" => o.price_low, "priceHigh" => o.price_high);
                let bins = Float64Array::from(o.bins.as_slice());
                Reflect::set(&obj, &key("bins"), &bins).ok();
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = TpoProfile)]
pub struct WasmTpoProfile {
    inner: wc::TpoProfile,
}

#[wasm_bindgen(js_class = TpoProfile)]
impl WasmTpoProfile {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, bin_count: usize) -> Result<WasmTpoProfile, JsError> {
        Ok(Self {
            inner: wc::TpoProfile::new(period, bin_count).map_err(map_err)?,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high, low must be equal length"));
        }
        let k = self.inner.params().1 + 2;
        let n = high.len();
        let mut out = vec![f64::NAN; n * k];
        for i in 0..n {
            let mid = f64::midpoint(high[i], low[i]);
            let c = wc::Candle::new(mid, high[i], low[i], mid, 1.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * k] = o.price_low;
                out[i * k + 1] = o.price_high;
                for (j, count) in o.counts.iter().enumerate() {
                    out[i * k + 2 + j] = *count;
                }
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Streaming update. Returns `{ priceLow, priceHigh, counts }` once warm, else `undefined`.
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<WasmTpoProfileValue>, JsError> {
        let mid = f64::midpoint(high, low);
        let c = wc::Candle::new(mid, high, low, mid, 1.0, 0).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("priceLow" => o.price_low, "priceHigh" => o.price_high);
                let counts = Float64Array::from(o.counts.as_slice());
                Reflect::set(&obj, &key("counts"), &counts).ok();
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = InitialBalance)]
pub struct WasmInitialBalance {
    inner: wc::InitialBalance,
}

#[wasm_bindgen(js_class = InitialBalance)]
impl WasmInitialBalance {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmInitialBalance, JsError> {
        Ok(Self {
            inner: wc::InitialBalance::new(period).map_err(map_err)?,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let mid = f64::midpoint(high[i], low[i]);
            let c = wc::Candle::new(mid, high[i], low[i], mid, 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.high;
                out[i * 2 + 1] = o.low;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Streaming update. Returns `{ high, low }`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmInitialBalanceValue>, JsError> {
        let mid = f64::midpoint(high, low);
        let c = wc::Candle::new(mid, high, low, mid, 0.0, 0).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("high" => o.high, "low" => o.low);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = isLocked)]
    pub fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = OpeningRange)]
pub struct WasmOpeningRange {
    inner: wc::OpeningRange,
}

#[wasm_bindgen(js_class = OpeningRange)]
impl WasmOpeningRange {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmOpeningRange, JsError> {
        Ok(Self {
            inner: wc::OpeningRange::new(period).map_err(map_err)?,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let c =
                wc::Candle::new(close[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.high;
                out[i * 3 + 1] = o.low;
                out[i * 3 + 2] = o.breakout_distance;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    /// Streaming update. Returns `{ high, low, breakoutDistance }`.
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmOpeningRangeValue>, JsError> {
        let c = wc::Candle::new(close, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("high" => o.high, "low" => o.low, "breakoutDistance" => o.breakout_distance);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = isLocked)]
    pub fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Candlestick Patterns ==============================
//
// All 15 patterns take Candles (open, high, low, close) and emit a signed f64
// signal per bar: +1.0 bullish, -1.0 bearish, 0.0 no pattern. Doji is
// direction-less by default (0/+1); pass `signed = true` to its constructor for
// the dragonfly/gravestone signed +-1 encoding.

macro_rules! wasm_candle_pattern {
    ($wasm:ident, $inner:ty, $js:ident) => {
        #[wasm_bindgen(js_name = $js)]
        pub struct $wasm {
            inner: $inner,
        }

        impl Default for $wasm {
            fn default() -> Self {
                Self::new()
            }
        }

        #[wasm_bindgen(js_class = $js)]
        impl $wasm {
            #[wasm_bindgen(constructor)]
            pub fn new() -> $wasm {
                Self {
                    inner: <$inner>::new(),
                }
            }
            pub fn update(
                &mut self,
                open: f64,
                high: f64,
                low: f64,
                close: f64,
            ) -> Result<Option<f64>, JsError> {
                let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
                Ok(self.inner.update(c))
            }
            pub fn batch(
                &mut self,
                open: &[f64],
                high: &[f64],
                low: &[f64],
                close: &[f64],
            ) -> Result<Float64Array, JsError> {
                let n = open.len();
                if high.len() != n || low.len() != n || close.len() != n {
                    return Err(JsError::new("open, high, low, close must be equal length"));
                }
                let mut out = Vec::with_capacity(n);
                for i in 0..n {
                    let c = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0)
                        .map_err(map_err)?;
                    out.push(self.inner.update(c).unwrap_or(f64::NAN));
                }
                Ok(Float64Array::from(out.as_slice()))
            }
            pub fn reset(&mut self) {
                self.inner.reset();
            }

            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[wasm_bindgen(js_name = isReady)]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[wasm_bindgen(js_name = warmupPeriod)]
            pub fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
        }
    };
}

// Doji is the one pattern with an opt-in signed mode, so it is hand-written
// rather than generated by `wasm_candle_pattern!`.
#[wasm_bindgen(js_name = Doji)]
pub struct WasmDoji {
    inner: wc::Doji,
}

impl Default for WasmDoji {
    fn default() -> Self {
        Self::new(None)
    }
}

#[wasm_bindgen(js_class = Doji)]
impl WasmDoji {
    #[wasm_bindgen(constructor)]
    pub fn new(signed: Option<bool>) -> WasmDoji {
        let inner = if signed.unwrap_or(false) {
            wc::Doji::new().signed()
        } else {
            wc::Doji::new()
        };
        Self { inner }
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = wc::Candle::new(open, high, low, close, 0.0, 0).map_err(map_err)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = open.len();
        if high.len() != n || low.len() != n || close.len() != n {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    #[wasm_bindgen(js_name = isSigned)]
    pub fn is_signed(&self) -> bool {
        self.inner.is_signed()
    }
}

wasm_candle_pattern!(WasmHammer, wc::Hammer, Hammer);
wasm_candle_pattern!(WasmInvertedHammer, wc::InvertedHammer, InvertedHammer);
wasm_candle_pattern!(WasmHangingMan, wc::HangingMan, HangingMan);
wasm_candle_pattern!(WasmShootingStar, wc::ShootingStar, ShootingStar);
wasm_candle_pattern!(WasmEngulfing, wc::Engulfing, Engulfing);
wasm_candle_pattern!(WasmHarami, wc::Harami, Harami);
wasm_candle_pattern!(
    WasmMorningEveningStar,
    wc::MorningEveningStar,
    MorningEveningStar
);
wasm_candle_pattern!(
    WasmThreeSoldiersOrCrows,
    wc::ThreeSoldiersOrCrows,
    ThreeSoldiersOrCrows
);
wasm_candle_pattern!(
    WasmPiercingDarkCloud,
    wc::PiercingDarkCloud,
    PiercingDarkCloud
);
wasm_candle_pattern!(WasmMarubozu, wc::Marubozu, Marubozu);
wasm_candle_pattern!(WasmTweezer, wc::Tweezer, Tweezer);
wasm_candle_pattern!(WasmSpinningTop, wc::SpinningTop, SpinningTop);
wasm_candle_pattern!(WasmThreeInside, wc::ThreeInside, ThreeInside);
wasm_candle_pattern!(WasmThreeOutside, wc::ThreeOutside, ThreeOutside);
wasm_candle_pattern!(WasmTwoCrows, wc::TwoCrows, TwoCrows);
wasm_candle_pattern!(
    WasmUpsideGapTwoCrows,
    wc::UpsideGapTwoCrows,
    UpsideGapTwoCrows
);
wasm_candle_pattern!(
    WasmIdenticalThreeCrows,
    wc::IdenticalThreeCrows,
    IdenticalThreeCrows
);
wasm_candle_pattern!(WasmThreeLineStrike, wc::ThreeLineStrike, ThreeLineStrike);
wasm_candle_pattern!(
    WasmThreeStarsInSouth,
    wc::ThreeStarsInSouth,
    ThreeStarsInSouth
);
wasm_candle_pattern!(WasmAbandonedBaby, wc::AbandonedBaby, AbandonedBaby);
wasm_candle_pattern!(WasmAdvanceBlock, wc::AdvanceBlock, AdvanceBlock);
wasm_candle_pattern!(WasmBeltHold, wc::BeltHold, BeltHold);
wasm_candle_pattern!(WasmBreakaway, wc::Breakaway, Breakaway);
wasm_candle_pattern!(WasmCounterattack, wc::Counterattack, Counterattack);
wasm_candle_pattern!(WasmDojiStar, wc::DojiStar, DojiStar);
wasm_candle_pattern!(WasmDragonflyDoji, wc::DragonflyDoji, DragonflyDoji);
wasm_candle_pattern!(WasmGravestoneDoji, wc::GravestoneDoji, GravestoneDoji);
wasm_candle_pattern!(WasmLongLeggedDoji, wc::LongLeggedDoji, LongLeggedDoji);
wasm_candle_pattern!(WasmRickshawMan, wc::RickshawMan, RickshawMan);
wasm_candle_pattern!(WasmEveningDojiStar, wc::EveningDojiStar, EveningDojiStar);
wasm_candle_pattern!(WasmMorningDojiStar, wc::MorningDojiStar, MorningDojiStar);
wasm_candle_pattern!(
    WasmGapSideBySideWhite,
    wc::GapSideBySideWhite,
    GapSideBySideWhite
);
wasm_candle_pattern!(WasmHighWave, wc::HighWave, HighWave);
wasm_candle_pattern!(WasmHikkake, wc::Hikkake, Hikkake);
wasm_candle_pattern!(WasmHikkakeModified, wc::HikkakeModified, HikkakeModified);
wasm_candle_pattern!(WasmHomingPigeon, wc::HomingPigeon, HomingPigeon);
wasm_candle_pattern!(WasmOnNeck, wc::OnNeck, OnNeck);
wasm_candle_pattern!(WasmInNeck, wc::InNeck, InNeck);
wasm_candle_pattern!(WasmThrusting, wc::Thrusting, Thrusting);
wasm_candle_pattern!(WasmSeparatingLines, wc::SeparatingLines, SeparatingLines);
wasm_candle_pattern!(WasmKicking, wc::Kicking, Kicking);
wasm_candle_pattern!(WasmKickingByLength, wc::KickingByLength, KickingByLength);
wasm_candle_pattern!(WasmLadderBottom, wc::LadderBottom, LadderBottom);
wasm_candle_pattern!(WasmMatHold, wc::MatHold, MatHold);
wasm_candle_pattern!(WasmMatchingLow, wc::MatchingLow, MatchingLow);
wasm_candle_pattern!(WasmLongLine, wc::LongLine, LongLine);
wasm_candle_pattern!(WasmShortLine, wc::ShortLine, ShortLine);
wasm_candle_pattern!(
    WasmRisingThreeMethods,
    wc::RisingThreeMethods,
    RisingThreeMethods
);
wasm_candle_pattern!(
    WasmFallingThreeMethods,
    wc::FallingThreeMethods,
    FallingThreeMethods
);
wasm_candle_pattern!(
    WasmUpsideGapThreeMethods,
    wc::UpsideGapThreeMethods,
    UpsideGapThreeMethods
);
wasm_candle_pattern!(
    WasmDownsideGapThreeMethods,
    wc::DownsideGapThreeMethods,
    DownsideGapThreeMethods
);
wasm_candle_pattern!(WasmStalledPattern, wc::StalledPattern, StalledPattern);
wasm_candle_pattern!(WasmStickSandwich, wc::StickSandwich, StickSandwich);
wasm_candle_pattern!(WasmTakuri, wc::Takuri, Takuri);
wasm_candle_pattern!(WasmClosingMarubozu, wc::ClosingMarubozu, ClosingMarubozu);
wasm_candle_pattern!(WasmOpeningMarubozu, wc::OpeningMarubozu, OpeningMarubozu);
wasm_candle_pattern!(WasmTasukiGap, wc::TasukiGap, TasukiGap);
wasm_candle_pattern!(WasmUniqueThreeRiver, wc::UniqueThreeRiver, UniqueThreeRiver);
wasm_candle_pattern!(
    WasmConcealingBabySwallow,
    wc::ConcealingBabySwallow,
    ConcealingBabySwallow
);
wasm_candle_pattern!(WasmDoubleTopBottom, wc::DoubleTopBottom, DoubleTopBottom);
wasm_candle_pattern!(WasmTripleTopBottom, wc::TripleTopBottom, TripleTopBottom);
wasm_candle_pattern!(WasmHeadAndShoulders, wc::HeadAndShoulders, HeadAndShoulders);
wasm_candle_pattern!(WasmTriangle, wc::Triangle, Triangle);
wasm_candle_pattern!(WasmWedge, wc::Wedge, Wedge);
wasm_candle_pattern!(WasmFlagPennant, wc::FlagPennant, FlagPennant);
wasm_candle_pattern!(WasmRectangleRange, wc::RectangleRange, RectangleRange);
wasm_candle_pattern!(WasmCupAndHandle, wc::CupAndHandle, CupAndHandle);
wasm_candle_pattern!(WasmAbcd, wc::Abcd, Abcd);
wasm_candle_pattern!(WasmGartley, wc::Gartley, Gartley);
wasm_candle_pattern!(WasmButterfly, wc::Butterfly, Butterfly);
wasm_candle_pattern!(WasmBat, wc::Bat, Bat);
wasm_candle_pattern!(WasmCrab, wc::Crab, Crab);
wasm_candle_pattern!(WasmShark, wc::Shark, Shark);
wasm_candle_pattern!(WasmCypher, wc::Cypher, Cypher);
wasm_candle_pattern!(WasmThreeDrives, wc::ThreeDrives, ThreeDrives);
wasm_candle_pattern!(WasmTdCamouflage, wc::TdCamouflage, TDCamouflage);
wasm_candle_pattern!(WasmTdClop, wc::TdClop, TDClop);
wasm_candle_pattern!(WasmTdClopwin, wc::TdClopwin, TDClopwin);
wasm_candle_pattern!(WasmTdPropulsion, wc::TdPropulsion, TDPropulsion);
wasm_candle_pattern!(WasmTdTrap, wc::TdTrap, TDTrap);
wasm_candle_pattern!(WasmTristar, wc::Tristar, Tristar);
wasm_candle_pattern!(WasmHaramiCross, wc::HaramiCross, HaramiCross);
wasm_candle_pattern!(WasmTowerTopBottom, wc::TowerTopBottom, TowerTopBottom);

// ============================== Microstructure: Order Book ==============================
//
// Order-book indicators consume a depth snapshot rather than OHLCV. Each
// `update(bidPx, bidSz, askPx, askSz)` takes four equal-length typed arrays for
// one snapshot (bids best-first = descending price, asks best-first = ascending
// price) — the streaming model that fits a live browser book feed. Batch over a
// ragged depth history is provided by the Python and Node bindings.

fn build_order_book(
    bid_px: &[f64],
    bid_sz: &[f64],
    ask_px: &[f64],
    ask_sz: &[f64],
) -> Result<wc::OrderBook, JsError> {
    if bid_px.len() != bid_sz.len() || ask_px.len() != ask_sz.len() {
        return Err(JsError::new(
            "bid/ask price and size arrays must be equal length",
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

macro_rules! wasm_ob_indicator {
    ($wasm:ident, $inner:ty, $js:ident) => {
        #[wasm_bindgen(js_name = $js)]
        pub struct $wasm {
            inner: $inner,
        }

        impl Default for $wasm {
            fn default() -> Self {
                Self::new()
            }
        }

        #[wasm_bindgen(js_class = $js)]
        impl $wasm {
            #[wasm_bindgen(constructor)]
            pub fn new() -> $wasm {
                Self {
                    inner: <$inner>::new(),
                }
            }
            pub fn update(
                &mut self,
                bid_px: &[f64],
                bid_sz: &[f64],
                ask_px: &[f64],
                ask_sz: &[f64],
            ) -> Result<Option<f64>, JsError> {
                let book = build_order_book(bid_px, bid_sz, ask_px, ask_sz)?;
                Ok(self.inner.update(book))
            }
            /// Batch over the same inputs as `update`, one element per bar.
            /// Warmup positions come back as `NaN`, so the output length
            /// matches the input.
            pub fn batch(
                &mut self,
                bid_px: Vec<Float64Array>,
                bid_sz: Vec<Float64Array>,
                ask_px: Vec<Float64Array>,
                ask_sz: Vec<Float64Array>,
            ) -> Result<Float64Array, JsError> {
                if bid_sz.len() != bid_px.len()
                    || ask_px.len() != bid_px.len()
                    || ask_sz.len() != bid_px.len()
                {
                    return Err(JsError::new(
                        "bid_px, bid_sz, ask_px, ask_sz must be equal length",
                    ));
                }
                let mut out = Vec::with_capacity(bid_px.len());
                for i in 0..bid_px.len() {
                    out.push(
                        self.update(
                            &bid_px[i].to_vec(),
                            &bid_sz[i].to_vec(),
                            &ask_px[i].to_vec(),
                            &ask_sz[i].to_vec(),
                        )?
                        .unwrap_or(f64::NAN),
                    );
                }
                Ok(Float64Array::from(out.as_slice()))
            }

            pub fn reset(&mut self) {
                self.inner.reset();
            }

            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[wasm_bindgen(js_name = isReady)]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[wasm_bindgen(js_name = warmupPeriod)]
            pub fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
        }
    };
}

wasm_ob_indicator!(
    WasmOrderBookImbalanceTop1,
    wc::OrderBookImbalanceTop1,
    OrderBookImbalanceTop1
);
wasm_ob_indicator!(
    WasmOrderBookImbalanceFull,
    wc::OrderBookImbalanceFull,
    OrderBookImbalanceFull
);
wasm_ob_indicator!(WasmMicroprice, wc::Microprice, Microprice);
wasm_ob_indicator!(WasmQuotedSpread, wc::QuotedSpread, QuotedSpread);
wasm_ob_indicator!(WasmDepthSlope, wc::DepthSlope, DepthSlope);

// Top-N imbalance carries a `levels` parameter, so it is hand-written.
#[wasm_bindgen(js_name = OrderBookImbalanceTopN)]
pub struct WasmOrderBookImbalanceTopN {
    inner: wc::OrderBookImbalanceTopN,
}

#[wasm_bindgen(js_class = OrderBookImbalanceTopN)]
impl WasmOrderBookImbalanceTopN {
    #[wasm_bindgen(constructor)]
    pub fn new(levels: usize) -> Result<WasmOrderBookImbalanceTopN, JsError> {
        Ok(Self {
            inner: wc::OrderBookImbalanceTopN::new(levels).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        bid_px: &[f64],
        bid_sz: &[f64],
        ask_px: &[f64],
        ask_sz: &[f64],
    ) -> Result<Option<f64>, JsError> {
        let book = build_order_book(bid_px, bid_sz, ask_px, ask_sz)?;
        Ok(self.inner.update(book))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        bid_px: Vec<Float64Array>,
        bid_sz: Vec<Float64Array>,
        ask_px: Vec<Float64Array>,
        ask_sz: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if bid_sz.len() != bid_px.len()
            || ask_px.len() != bid_px.len()
            || ask_sz.len() != bid_px.len()
        {
            return Err(JsError::new(
                "bid_px, bid_sz, ask_px, ask_sz must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(bid_px.len());
        for i in 0..bid_px.len() {
            out.push(
                self.update(
                    &bid_px[i].to_vec(),
                    &bid_sz[i].to_vec(),
                    &ask_px[i].to_vec(),
                    &ask_sz[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// ============================== Microstructure: Trade Flow ==============================
//
// Trade-flow indicators consume a trade tape rather than OHLCV. Each
// `update(price, size, isBuy)` takes one trade (`isBuy=true` for a
// buyer-initiated trade) — the streaming model for a live browser trade feed.

fn build_trade(price: f64, size: f64, is_buy: bool) -> Result<wc::Trade, JsError> {
    let side = if is_buy {
        wc::Side::Buy
    } else {
        wc::Side::Sell
    };
    wc::Trade::new(price, size, side, 0).map_err(map_err)
}

macro_rules! wasm_trade_indicator {
    ($wasm:ident, $inner:ty, $js:ident) => {
        #[wasm_bindgen(js_name = $js)]
        pub struct $wasm {
            inner: $inner,
        }

        impl Default for $wasm {
            fn default() -> Self {
                Self::new()
            }
        }

        #[wasm_bindgen(js_class = $js)]
        impl $wasm {
            #[wasm_bindgen(constructor)]
            pub fn new() -> $wasm {
                Self {
                    inner: <$inner>::new(),
                }
            }
            pub fn update(
                &mut self,
                price: f64,
                size: f64,
                is_buy: bool,
            ) -> Result<Option<f64>, JsError> {
                Ok(self.inner.update(build_trade(price, size, is_buy)?))
            }
            /// Batch over the same inputs as `update`, one element per bar.
            /// Warmup positions come back as `NaN`, so the output length
            /// matches the input.
            pub fn batch(
                &mut self,
                price: &[f64],
                size: &[f64],
                is_buy: &BoolArray,
            ) -> Result<Float64Array, JsError> {
                let is_buy = bool_series(is_buy)?;
                if size.len() != price.len() || is_buy.len() != price.len() {
                    return Err(JsError::new("price, size, is_buy must be equal length"));
                }
                let mut out = Vec::with_capacity(price.len());
                for i in 0..price.len() {
                    out.push(
                        self.update(price[i], size[i], is_buy[i])?
                            .unwrap_or(f64::NAN),
                    );
                }
                Ok(Float64Array::from(out.as_slice()))
            }

            pub fn reset(&mut self) {
                self.inner.reset();
            }

            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[wasm_bindgen(js_name = isReady)]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[wasm_bindgen(js_name = warmupPeriod)]
            pub fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
        }
    };
}

wasm_trade_indicator!(WasmSignedVolume, wc::SignedVolume, SignedVolume);
wasm_trade_indicator!(
    WasmCumulativeVolumeDelta,
    wc::CumulativeVolumeDelta,
    CumulativeVolumeDelta
);

// Trade imbalance carries a `window` parameter, so it is hand-written.
#[wasm_bindgen(js_name = TradeImbalance)]
pub struct WasmTradeImbalance {
    inner: wc::TradeImbalance,
}

#[wasm_bindgen(js_class = TradeImbalance)]
impl WasmTradeImbalance {
    #[wasm_bindgen(constructor)]
    pub fn new(window: usize) -> Result<WasmTradeImbalance, JsError> {
        Ok(Self {
            inner: wc::TradeImbalance::new(window).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, price: f64, size: f64, is_buy: bool) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        price: &[f64],
        size: &[f64],
        is_buy: &BoolArray,
    ) -> Result<Float64Array, JsError> {
        let is_buy = bool_series(is_buy)?;
        if size.len() != price.len() || is_buy.len() != price.len() {
            return Err(JsError::new("price, size, is_buy must be equal length"));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            out.push(
                self.update(price[i], size[i], is_buy[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// Trade-sign autocorrelation carries a `period` parameter, so it is hand-written.
#[wasm_bindgen(js_name = TradeSignAutocorrelation)]
pub struct WasmTradeSignAutocorrelation {
    inner: wc::TradeSignAutocorrelation,
}

#[wasm_bindgen(js_class = TradeSignAutocorrelation)]
impl WasmTradeSignAutocorrelation {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmTradeSignAutocorrelation, JsError> {
        Ok(Self {
            inner: wc::TradeSignAutocorrelation::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, price: f64, size: f64, is_buy: bool) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        price: &[f64],
        size: &[f64],
        is_buy: &BoolArray,
    ) -> Result<Float64Array, JsError> {
        let is_buy = bool_series(is_buy)?;
        if size.len() != price.len() || is_buy.len() != price.len() {
            return Err(JsError::new("price, size, is_buy must be equal length"));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            out.push(
                self.update(price[i], size[i], is_buy[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// PIN carries a `window` parameter, so it is hand-written.
#[wasm_bindgen(js_name = Pin)]
pub struct WasmPin {
    inner: wc::Pin,
}

#[wasm_bindgen(js_class = Pin)]
impl WasmPin {
    #[wasm_bindgen(constructor)]
    pub fn new(window: usize) -> Result<WasmPin, JsError> {
        Ok(Self {
            inner: wc::Pin::new(window).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, price: f64, size: f64, is_buy: bool) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        price: &[f64],
        size: &[f64],
        is_buy: &BoolArray,
    ) -> Result<Float64Array, JsError> {
        let is_buy = bool_series(is_buy)?;
        if size.len() != price.len() || is_buy.len() != price.len() {
            return Err(JsError::new("price, size, is_buy must be equal length"));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            out.push(
                self.update(price[i], size[i], is_buy[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// Order Flow Imbalance: order-book input with a `period` parameter.
#[wasm_bindgen(js_name = OrderFlowImbalance)]
pub struct WasmOrderFlowImbalance {
    inner: wc::OrderFlowImbalance,
}

#[wasm_bindgen(js_class = OrderFlowImbalance)]
impl WasmOrderFlowImbalance {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmOrderFlowImbalance, JsError> {
        Ok(Self {
            inner: wc::OrderFlowImbalance::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        bid_px: &[f64],
        bid_sz: &[f64],
        ask_px: &[f64],
        ask_sz: &[f64],
    ) -> Result<Option<f64>, JsError> {
        let book = build_order_book(bid_px, bid_sz, ask_px, ask_sz)?;
        Ok(self.inner.update(book))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        bid_px: Vec<Float64Array>,
        bid_sz: Vec<Float64Array>,
        ask_px: Vec<Float64Array>,
        ask_sz: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if bid_sz.len() != bid_px.len()
            || ask_px.len() != bid_px.len()
            || ask_sz.len() != bid_px.len()
        {
            return Err(JsError::new(
                "bid_px, bid_sz, ask_px, ask_sz must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(bid_px.len());
        for i in 0..bid_px.len() {
            out.push(
                self.update(
                    &bid_px[i].to_vec(),
                    &bid_sz[i].to_vec(),
                    &ask_px[i].to_vec(),
                    &ask_sz[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// VPIN: trade input, volume-bucketed `(bucket_volume, num_buckets)`.
#[wasm_bindgen(js_name = Vpin)]
pub struct WasmVpin {
    inner: wc::Vpin,
}

#[wasm_bindgen(js_class = Vpin)]
impl WasmVpin {
    #[wasm_bindgen(constructor)]
    pub fn new(bucket_volume: f64, num_buckets: usize) -> Result<WasmVpin, JsError> {
        Ok(Self {
            inner: wc::Vpin::new(bucket_volume, num_buckets).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, price: f64, size: f64, is_buy: bool) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        price: &[f64],
        size: &[f64],
        is_buy: &BoolArray,
    ) -> Result<Float64Array, JsError> {
        let is_buy = bool_series(is_buy)?;
        if size.len() != price.len() || is_buy.len() != price.len() {
            return Err(JsError::new("price, size, is_buy must be equal length"));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            out.push(
                self.update(price[i], size[i], is_buy[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// Amihud Illiquidity: trade input with a `period` parameter.
#[wasm_bindgen(js_name = AmihudIlliquidity)]
pub struct WasmAmihudIlliquidity {
    inner: wc::AmihudIlliquidity,
}

#[wasm_bindgen(js_class = AmihudIlliquidity)]
impl WasmAmihudIlliquidity {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmAmihudIlliquidity, JsError> {
        Ok(Self {
            inner: wc::AmihudIlliquidity::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, price: f64, size: f64, is_buy: bool) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        price: &[f64],
        size: &[f64],
        is_buy: &BoolArray,
    ) -> Result<Float64Array, JsError> {
        let is_buy = bool_series(is_buy)?;
        if size.len() != price.len() || is_buy.len() != price.len() {
            return Err(JsError::new("price, size, is_buy must be equal length"));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            out.push(
                self.update(price[i], size[i], is_buy[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// Roll Measure: trade input with a `period` parameter.
#[wasm_bindgen(js_name = RollMeasure)]
pub struct WasmRollMeasure {
    inner: wc::RollMeasure,
}

#[wasm_bindgen(js_class = RollMeasure)]
impl WasmRollMeasure {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmRollMeasure, JsError> {
        Ok(Self {
            inner: wc::RollMeasure::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, price: f64, size: f64, is_buy: bool) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        price: &[f64],
        size: &[f64],
        is_buy: &BoolArray,
    ) -> Result<Float64Array, JsError> {
        let is_buy = bool_series(is_buy)?;
        if size.len() != price.len() || is_buy.len() != price.len() {
            return Err(JsError::new("price, size, is_buy must be equal length"));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            out.push(
                self.update(price[i], size[i], is_buy[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// ============================== Microstructure: Price Impact ==============================
//
// Price-impact indicators consume a trade paired with the mid prevailing at
// execution. Each `update(price, size, isBuy, mid)` takes one such trade-quote
// (`isBuy=true` for a buyer-initiated trade) — the streaming model for a live
// browser trade feed. Batch over a tape is provided by the Python and Node
// bindings.

fn build_trade_quote(
    price: f64,
    size: f64,
    is_buy: bool,
    mid: f64,
) -> Result<wc::TradeQuote, JsError> {
    let trade = build_trade(price, size, is_buy)?;
    wc::TradeQuote::new(trade, mid).map_err(map_err)
}

macro_rules! wasm_trade_quote_indicator {
    ($wasm:ident, $inner:ty, $js:ident) => {
        #[wasm_bindgen(js_name = $js)]
        pub struct $wasm {
            inner: $inner,
        }

        impl Default for $wasm {
            fn default() -> Self {
                Self::new()
            }
        }

        #[wasm_bindgen(js_class = $js)]
        impl $wasm {
            #[wasm_bindgen(constructor)]
            pub fn new() -> $wasm {
                Self {
                    inner: <$inner>::new(),
                }
            }
            pub fn update(
                &mut self,
                price: f64,
                size: f64,
                is_buy: bool,
                mid: f64,
            ) -> Result<Option<f64>, JsError> {
                Ok(self
                    .inner
                    .update(build_trade_quote(price, size, is_buy, mid)?))
            }
            /// Batch over the same inputs as `update`, one element per bar.
            /// Warmup positions come back as `NaN`, so the output length
            /// matches the input.
            pub fn batch(
                &mut self,
                price: &[f64],
                size: &[f64],
                is_buy: &BoolArray,
                mid: &[f64],
            ) -> Result<Float64Array, JsError> {
                let is_buy = bool_series(is_buy)?;
                if size.len() != price.len()
                    || is_buy.len() != price.len()
                    || mid.len() != price.len()
                {
                    return Err(JsError::new(
                        "price, size, is_buy, mid must be equal length",
                    ));
                }
                let mut out = Vec::with_capacity(price.len());
                for i in 0..price.len() {
                    out.push(
                        self.update(price[i], size[i], is_buy[i], mid[i])?
                            .unwrap_or(f64::NAN),
                    );
                }
                Ok(Float64Array::from(out.as_slice()))
            }

            pub fn reset(&mut self) {
                self.inner.reset();
            }

            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[wasm_bindgen(js_name = isReady)]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[wasm_bindgen(js_name = warmupPeriod)]
            pub fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
        }
    };
}

wasm_trade_quote_indicator!(WasmEffectiveSpread, wc::EffectiveSpread, EffectiveSpread);

// Realized spread carries a `horizon` parameter, so it is hand-written.
#[wasm_bindgen(js_name = RealizedSpread)]
pub struct WasmRealizedSpread {
    inner: wc::RealizedSpread,
}

#[wasm_bindgen(js_class = RealizedSpread)]
impl WasmRealizedSpread {
    #[wasm_bindgen(constructor)]
    pub fn new(horizon: usize) -> Result<WasmRealizedSpread, JsError> {
        Ok(Self {
            inner: wc::RealizedSpread::new(horizon).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        price: f64,
        size: f64,
        is_buy: bool,
        mid: f64,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_trade_quote(price, size, is_buy, mid)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        price: &[f64],
        size: &[f64],
        is_buy: &BoolArray,
        mid: &[f64],
    ) -> Result<Float64Array, JsError> {
        let is_buy = bool_series(is_buy)?;
        if size.len() != price.len() || is_buy.len() != price.len() || mid.len() != price.len() {
            return Err(JsError::new(
                "price, size, is_buy, mid must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            out.push(
                self.update(price[i], size[i], is_buy[i], mid[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// Kyle's lambda carries a `window` parameter, so it is hand-written.
#[wasm_bindgen(js_name = KylesLambda)]
pub struct WasmKylesLambda {
    inner: wc::KylesLambda,
}

#[wasm_bindgen(js_class = KylesLambda)]
impl WasmKylesLambda {
    #[wasm_bindgen(constructor)]
    pub fn new(window: usize) -> Result<WasmKylesLambda, JsError> {
        Ok(Self {
            inner: wc::KylesLambda::new(window).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        price: f64,
        size: f64,
        is_buy: bool,
        mid: f64,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_trade_quote(price, size, is_buy, mid)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        price: &[f64],
        size: &[f64],
        is_buy: &BoolArray,
        mid: &[f64],
    ) -> Result<Float64Array, JsError> {
        let is_buy = bool_series(is_buy)?;
        if size.len() != price.len() || is_buy.len() != price.len() || mid.len() != price.len() {
            return Err(JsError::new(
                "price, size, is_buy, mid must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            out.push(
                self.update(price[i], size[i], is_buy[i], mid[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// ============================== Microstructure: Footprint ==============================
//
// Footprint is a multi-output, variable-length indicator. Each `update(price,
// size, isBuy)` returns the full bar footprint accumulated since the last
// `reset()` as an array of `{ price, bidVol, askVol }` objects (sorted ascending
// by price) — the streaming model for a live browser trade feed.

#[wasm_bindgen(js_name = Footprint)]
pub struct WasmFootprint {
    inner: wc::Footprint,
}

#[wasm_bindgen(js_class = Footprint)]
impl WasmFootprint {
    #[wasm_bindgen(constructor)]
    pub fn new(tick_size: f64) -> Result<WasmFootprint, JsError> {
        Ok(Self {
            inner: wc::Footprint::new(tick_size).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        price: f64,
        size: f64,
        is_buy: bool,
    ) -> Result<WasmFootprintValue, JsError> {
        let out = self
            .inner
            .update(build_trade(price, size, is_buy)?)
            .expect("footprint emits on every trade");
        let levels = Array::new();
        for level in &out.levels {
            let obj = js_object!("price" => level.price, "bidVol" => level.bid_vol, "askVol" => level.ask_vol);
            levels.push(&obj);
        }
        Ok(levels.unchecked_into())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`. Returns one array of
    /// `{ price, bidVol, askVol }` levels per trade, since the level count
    /// varies with the trades seen so far.
    pub fn batch(
        &mut self,
        price: &[f64],
        size: &[f64],
        is_buy: &BoolArray,
    ) -> Result<WasmFootprintBatchValue, JsError> {
        let is_buy = bool_series(is_buy)?;
        if size.len() != price.len() || is_buy.len() != price.len() {
            return Err(JsError::new("price, size, is_buy must be equal length"));
        }
        let out = Array::new();
        for i in 0..price.len() {
            out.push(self.update(price[i], size[i], is_buy[i])?.as_ref());
        }
        Ok(out.unchecked_into())
    }
}

// ============================== Derivatives ==============================
//
// Derivatives indicators consume a perpetual / futures tick rather than OHLCV.
// Each `update(...)` takes only the tick fields its indicator reads — the
// streaming model for a live browser derivatives feed. Batch over a tape is
// provided by the Python and Node bindings. The helpers build a fully-valid
// `DerivativesTick`, filling unused fields with neutral defaults.

fn deriv_funding(funding_rate: f64) -> Result<wc::DerivativesTick, JsError> {
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

fn deriv_basis(mark_price: f64, index_price: f64) -> Result<wc::DerivativesTick, JsError> {
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

fn deriv_oi(open_interest: f64) -> Result<wc::DerivativesTick, JsError> {
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

#[wasm_bindgen(js_name = FundingRate)]
pub struct WasmFundingRate {
    inner: wc::FundingRate,
}

impl Default for WasmFundingRate {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = FundingRate)]
impl WasmFundingRate {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmFundingRate {
        Self {
            inner: wc::FundingRate::new(),
        }
    }
    pub fn update(&mut self, funding_rate: f64) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(deriv_funding(funding_rate)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(&mut self, funding_rate: &[f64]) -> Result<Float64Array, JsError> {
        let mut out = Vec::with_capacity(funding_rate.len());
        for &value in funding_rate {
            out.push(self.update(value)?.unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = FundingRateMean)]
pub struct WasmFundingRateMean {
    inner: wc::FundingRateMean,
}

#[wasm_bindgen(js_class = FundingRateMean)]
impl WasmFundingRateMean {
    #[wasm_bindgen(constructor)]
    pub fn new(window: usize) -> Result<WasmFundingRateMean, JsError> {
        Ok(Self {
            inner: wc::FundingRateMean::new(window).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, funding_rate: f64) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(deriv_funding(funding_rate)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(&mut self, funding_rate: &[f64]) -> Result<Float64Array, JsError> {
        let mut out = Vec::with_capacity(funding_rate.len());
        for &value in funding_rate {
            out.push(self.update(value)?.unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = FundingRateZScore)]
pub struct WasmFundingRateZScore {
    inner: wc::FundingRateZScore,
}

#[wasm_bindgen(js_class = FundingRateZScore)]
impl WasmFundingRateZScore {
    #[wasm_bindgen(constructor)]
    pub fn new(window: usize) -> Result<WasmFundingRateZScore, JsError> {
        Ok(Self {
            inner: wc::FundingRateZScore::new(window).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, funding_rate: f64) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(deriv_funding(funding_rate)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(&mut self, funding_rate: &[f64]) -> Result<Float64Array, JsError> {
        let mut out = Vec::with_capacity(funding_rate.len());
        for &value in funding_rate {
            out.push(self.update(value)?.unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = FundingBasis)]
pub struct WasmFundingBasis {
    inner: wc::FundingBasis,
}

impl Default for WasmFundingBasis {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = FundingBasis)]
impl WasmFundingBasis {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmFundingBasis {
        Self {
            inner: wc::FundingBasis::new(),
        }
    }
    pub fn update(&mut self, mark_price: f64, index_price: f64) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(deriv_basis(mark_price, index_price)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        mark_price: &[f64],
        index_price: &[f64],
    ) -> Result<Float64Array, JsError> {
        if index_price.len() != mark_price.len() {
            return Err(JsError::new("mark_price, index_price must be equal length"));
        }
        let mut out = Vec::with_capacity(mark_price.len());
        for i in 0..mark_price.len() {
            out.push(
                self.update(mark_price[i], index_price[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = OpenInterestDelta)]
pub struct WasmOpenInterestDelta {
    inner: wc::OpenInterestDelta,
}

impl Default for WasmOpenInterestDelta {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = OpenInterestDelta)]
impl WasmOpenInterestDelta {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmOpenInterestDelta {
        Self {
            inner: wc::OpenInterestDelta::new(),
        }
    }
    pub fn update(&mut self, open_interest: f64) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(deriv_oi(open_interest)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(&mut self, open_interest: &[f64]) -> Result<Float64Array, JsError> {
        let mut out = Vec::with_capacity(open_interest.len());
        for &value in open_interest {
            out.push(self.update(value)?.unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

fn deriv_oi_mark(open_interest: f64, mark_price: f64) -> Result<wc::DerivativesTick, JsError> {
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

fn deriv_long_short(long_size: f64, short_size: f64) -> Result<wc::DerivativesTick, JsError> {
    wc::DerivativesTick::new(
        0.0, 1.0, 1.0, 1.0, 0.0, long_size, short_size, 0.0, 0.0, 0.0, 0.0, 0,
    )
    .map_err(map_err)
}

fn deriv_taker(
    taker_buy_volume: f64,
    taker_sell_volume: f64,
) -> Result<wc::DerivativesTick, JsError> {
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
) -> Result<wc::DerivativesTick, JsError> {
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
) -> Result<wc::DerivativesTick, JsError> {
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
) -> Result<wc::DerivativesTick, JsError> {
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

#[wasm_bindgen(js_name = OIPriceDivergence)]
pub struct WasmOIPriceDivergence {
    inner: wc::OIPriceDivergence,
}

#[wasm_bindgen(js_class = OIPriceDivergence)]
impl WasmOIPriceDivergence {
    #[wasm_bindgen(constructor)]
    pub fn new(window: usize) -> Result<WasmOIPriceDivergence, JsError> {
        Ok(Self {
            inner: wc::OIPriceDivergence::new(window).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, open_interest: f64, mark_price: f64) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(deriv_oi_mark(open_interest, mark_price)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        open_interest: &[f64],
        mark_price: &[f64],
    ) -> Result<Float64Array, JsError> {
        if mark_price.len() != open_interest.len() {
            return Err(JsError::new(
                "open_interest, mark_price must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(open_interest.len());
        for i in 0..open_interest.len() {
            out.push(
                self.update(open_interest[i], mark_price[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = OIWeighted)]
pub struct WasmOIWeighted {
    inner: wc::OIWeighted,
}

impl Default for WasmOIWeighted {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = OIWeighted)]
impl WasmOIWeighted {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmOIWeighted {
        Self {
            inner: wc::OIWeighted::new(),
        }
    }
    pub fn update(&mut self, mark_price: f64, open_interest: f64) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(deriv_oi_mark(open_interest, mark_price)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        mark_price: &[f64],
        open_interest: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open_interest.len() != mark_price.len() {
            return Err(JsError::new(
                "mark_price, open_interest must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(mark_price.len());
        for i in 0..mark_price.len() {
            out.push(
                self.update(mark_price[i], open_interest[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = LongShortRatio)]
pub struct WasmLongShortRatio {
    inner: wc::LongShortRatio,
}

impl Default for WasmLongShortRatio {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = LongShortRatio)]
impl WasmLongShortRatio {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmLongShortRatio {
        Self {
            inner: wc::LongShortRatio::new(),
        }
    }
    pub fn update(&mut self, long_size: f64, short_size: f64) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(deriv_long_short(long_size, short_size)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        long_size: &[f64],
        short_size: &[f64],
    ) -> Result<Float64Array, JsError> {
        if short_size.len() != long_size.len() {
            return Err(JsError::new("long_size, short_size must be equal length"));
        }
        let mut out = Vec::with_capacity(long_size.len());
        for i in 0..long_size.len() {
            out.push(
                self.update(long_size[i], short_size[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = TakerBuySellRatio)]
pub struct WasmTakerBuySellRatio {
    inner: wc::TakerBuySellRatio,
}

impl Default for WasmTakerBuySellRatio {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = TakerBuySellRatio)]
impl WasmTakerBuySellRatio {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmTakerBuySellRatio {
        Self {
            inner: wc::TakerBuySellRatio::new(),
        }
    }
    pub fn update(
        &mut self,
        taker_buy_volume: f64,
        taker_sell_volume: f64,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(deriv_taker(taker_buy_volume, taker_sell_volume)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        taker_buy_volume: &[f64],
        taker_sell_volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if taker_sell_volume.len() != taker_buy_volume.len() {
            return Err(JsError::new(
                "taker_buy_volume, taker_sell_volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(taker_buy_volume.len());
        for i in 0..taker_buy_volume.len() {
            out.push(
                self.update(taker_buy_volume[i], taker_sell_volume[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = LiquidationFeatures)]
pub struct WasmLiquidationFeatures {
    inner: wc::LiquidationFeatures,
}

impl Default for WasmLiquidationFeatures {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = LiquidationFeatures)]
impl WasmLiquidationFeatures {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmLiquidationFeatures {
        Self {
            inner: wc::LiquidationFeatures::new(),
        }
    }
    pub fn update(
        &mut self,
        long_liquidation: f64,
        short_liquidation: f64,
    ) -> Result<WasmLiquidationFeaturesValue, JsError> {
        let out = self
            .inner
            .update(deriv_liquidation(long_liquidation, short_liquidation)?)
            .expect("liquidation features emit on every tick");
        let obj = js_object!("long" => out.long, "short" => out.short, "net" => out.net, "total" => out.total, "imbalance" => out.imbalance);
        Ok(obj.unchecked_into())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`. Returns a flat array of
    /// `n * 5` values, `[long, short, net, total, imbalance]` per tick.
    pub fn batch(
        &mut self,
        long_liquidation: &[f64],
        short_liquidation: &[f64],
    ) -> Result<Float64Array, JsError> {
        if short_liquidation.len() != long_liquidation.len() {
            return Err(JsError::new(
                "long_liquidation, short_liquidation must be equal length",
            ));
        }
        let mut out = vec![f64::NAN; long_liquidation.len() * 5];
        for i in 0..long_liquidation.len() {
            let o = self
                .inner
                .update(deriv_liquidation(
                    long_liquidation[i],
                    short_liquidation[i],
                )?)
                .expect("liquidation features emit on every tick");
            out[i * 5] = o.long;
            out[i * 5 + 1] = o.short;
            out[i * 5 + 2] = o.net;
            out[i * 5 + 3] = o.total;
            out[i * 5 + 4] = o.imbalance;
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

fn deriv_futures_index(
    futures_price: f64,
    index_price: f64,
) -> Result<wc::DerivativesTick, JsError> {
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

fn deriv_futures_mark(futures_price: f64, mark_price: f64) -> Result<wc::DerivativesTick, JsError> {
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

#[wasm_bindgen(js_name = TermStructureBasis)]
pub struct WasmTermStructureBasis {
    inner: wc::TermStructureBasis,
}

impl Default for WasmTermStructureBasis {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = TermStructureBasis)]
impl WasmTermStructureBasis {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmTermStructureBasis {
        Self {
            inner: wc::TermStructureBasis::new(),
        }
    }
    pub fn update(&mut self, futures_price: f64, index_price: f64) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(deriv_futures_index(futures_price, index_price)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        futures_price: &[f64],
        index_price: &[f64],
    ) -> Result<Float64Array, JsError> {
        if index_price.len() != futures_price.len() {
            return Err(JsError::new(
                "futures_price, index_price must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(futures_price.len());
        for i in 0..futures_price.len() {
            out.push(
                self.update(futures_price[i], index_price[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = CalendarSpread)]
pub struct WasmCalendarSpread {
    inner: wc::CalendarSpread,
}

impl Default for WasmCalendarSpread {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = CalendarSpread)]
impl WasmCalendarSpread {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmCalendarSpread {
        Self {
            inner: wc::CalendarSpread::new(),
        }
    }
    pub fn update(&mut self, futures_price: f64, mark_price: f64) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(deriv_futures_mark(futures_price, mark_price)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        futures_price: &[f64],
        mark_price: &[f64],
    ) -> Result<Float64Array, JsError> {
        if mark_price.len() != futures_price.len() {
            return Err(JsError::new(
                "futures_price, mark_price must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(futures_price.len());
        for i in 0..futures_price.len() {
            out.push(
                self.update(futures_price[i], mark_price[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// ---------- Estimated Leverage Ratio ----------

#[wasm_bindgen(js_name = EstimatedLeverageRatio)]
pub struct WasmEstimatedLeverageRatio {
    inner: wc::EstimatedLeverageRatio,
}

impl Default for WasmEstimatedLeverageRatio {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = EstimatedLeverageRatio)]
impl WasmEstimatedLeverageRatio {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmEstimatedLeverageRatio {
        Self {
            inner: wc::EstimatedLeverageRatio::new(),
        }
    }
    pub fn update(
        &mut self,
        open_interest: f64,
        long_size: f64,
        short_size: f64,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(deriv_oi_long_short(open_interest, long_size, short_size)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        open_interest: &[f64],
        long_size: &[f64],
        short_size: &[f64],
    ) -> Result<Float64Array, JsError> {
        if long_size.len() != open_interest.len() || short_size.len() != open_interest.len() {
            return Err(JsError::new(
                "open_interest, long_size, short_size must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(open_interest.len());
        for i in 0..open_interest.len() {
            out.push(
                self.update(open_interest[i], long_size[i], short_size[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// ---------- OI-to-Volume Ratio ----------

#[wasm_bindgen(js_name = OiToVolumeRatio)]
pub struct WasmOiToVolumeRatio {
    inner: wc::OiToVolumeRatio,
}

impl Default for WasmOiToVolumeRatio {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = OiToVolumeRatio)]
impl WasmOiToVolumeRatio {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmOiToVolumeRatio {
        Self {
            inner: wc::OiToVolumeRatio::new(),
        }
    }
    pub fn update(
        &mut self,
        open_interest: f64,
        taker_buy_volume: f64,
        taker_sell_volume: f64,
    ) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(deriv_oi_taker(
            open_interest,
            taker_buy_volume,
            taker_sell_volume,
        )?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        open_interest: &[f64],
        taker_buy_volume: &[f64],
        taker_sell_volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if taker_buy_volume.len() != open_interest.len()
            || taker_sell_volume.len() != open_interest.len()
        {
            return Err(JsError::new(
                "open_interest, taker_buy_volume, taker_sell_volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(open_interest.len());
        for i in 0..open_interest.len() {
            out.push(
                self.update(open_interest[i], taker_buy_volume[i], taker_sell_volume[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// ---------- Perpetual Premium Index ----------

#[wasm_bindgen(js_name = PerpetualPremiumIndex)]
pub struct WasmPerpetualPremiumIndex {
    inner: wc::PerpetualPremiumIndex,
}

impl Default for WasmPerpetualPremiumIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = PerpetualPremiumIndex)]
impl WasmPerpetualPremiumIndex {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmPerpetualPremiumIndex {
        Self {
            inner: wc::PerpetualPremiumIndex::new(),
        }
    }
    pub fn update(&mut self, mark_price: f64, index_price: f64) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(deriv_basis(mark_price, index_price)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        mark_price: &[f64],
        index_price: &[f64],
    ) -> Result<Float64Array, JsError> {
        if index_price.len() != mark_price.len() {
            return Err(JsError::new("mark_price, index_price must be equal length"));
        }
        let mut out = Vec::with_capacity(mark_price.len());
        for i in 0..mark_price.len() {
            out.push(
                self.update(mark_price[i], index_price[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// ---------- Funding-Implied APR ----------

#[wasm_bindgen(js_name = FundingImpliedApr)]
pub struct WasmFundingImpliedApr {
    inner: wc::FundingImpliedApr,
}

#[wasm_bindgen(js_class = FundingImpliedApr)]
impl WasmFundingImpliedApr {
    #[wasm_bindgen(constructor)]
    pub fn new(intervals_per_year: f64) -> Result<WasmFundingImpliedApr, JsError> {
        Ok(Self {
            inner: wc::FundingImpliedApr::new(intervals_per_year).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, funding_rate: f64) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(deriv_funding(funding_rate)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(&mut self, funding_rate: &[f64]) -> Result<Float64Array, JsError> {
        let mut out = Vec::with_capacity(funding_rate.len());
        for &value in funding_rate {
            out.push(self.update(value)?.unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// ---------- Open-Interest Momentum ----------

#[wasm_bindgen(js_name = OpenInterestMomentum)]
pub struct WasmOpenInterestMomentum {
    inner: wc::OpenInterestMomentum,
}

#[wasm_bindgen(js_class = OpenInterestMomentum)]
impl WasmOpenInterestMomentum {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmOpenInterestMomentum, JsError> {
        Ok(Self {
            inner: wc::OpenInterestMomentum::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, open_interest: f64) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(deriv_oi(open_interest)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(&mut self, open_interest: &[f64]) -> Result<Float64Array, JsError> {
        let mut out = Vec::with_capacity(open_interest.len());
        for &value in open_interest {
            out.push(self.update(value)?.unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// ---------- Heikin-Ashi Oscillator ----------

#[wasm_bindgen(js_name = HeikinAshiOscillator)]
pub struct WasmHeikinAshiOscillator {
    inner: wc::HeikinAshiOscillator,
}

#[wasm_bindgen(js_class = HeikinAshiOscillator)]
impl WasmHeikinAshiOscillator {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmHeikinAshiOscillator, JsError> {
        Ok(Self {
            inner: wc::HeikinAshiOscillator::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle_ohlc(open, high, low, close)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            let c = make_candle_ohlc(open[i], high[i], low[i], close[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Three Line Break ----------

#[wasm_bindgen(js_name = ThreeLineBreak)]
pub struct WasmThreeLineBreak {
    inner: wc::ThreeLineBreak,
}

#[wasm_bindgen(js_class = ThreeLineBreak)]
impl WasmThreeLineBreak {
    #[wasm_bindgen(constructor)]
    pub fn new(lines: usize) -> Result<WasmThreeLineBreak, JsError> {
        Ok(Self {
            inner: wc::ThreeLineBreak::new(lines).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Smoothed Heikin-Ashi ----------

#[wasm_bindgen(js_name = SmoothedHeikinAshi)]
pub struct WasmSmoothedHeikinAshi {
    inner: wc::SmoothedHeikinAshi,
}

#[wasm_bindgen(js_class = SmoothedHeikinAshi)]
impl WasmSmoothedHeikinAshi {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmSmoothedHeikinAshi, JsError> {
        Ok(Self {
            inner: wc::SmoothedHeikinAshi::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmSmoothedHeikinAshiValue>, JsError> {
        let candle = make_candle_ohlc(open, high, low, close)?;
        match self.inner.update(candle) {
            Some(o) => {
                let obj = js_object!("open" => o.open, "high" => o.high, "low" => o.low, "close" => o.close);
                Ok(Some(obj.unchecked_into()))
            }
            None => Ok(None),
        }
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open.len() != high.len() || high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let n = open.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            let candle = make_candle_ohlc(open[i], high[i], low[i], close[i])?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 4] = o.open;
                out[i * 4 + 1] = o.high;
                out[i * 4 + 2] = o.low;
                out[i * 4 + 3] = o.close;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Equivolume ----------

#[wasm_bindgen(js_name = Equivolume)]
pub struct WasmEquivolume {
    inner: wc::Equivolume,
}

#[wasm_bindgen(js_class = Equivolume)]
impl WasmEquivolume {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmEquivolume, JsError> {
        Ok(Self {
            inner: wc::Equivolume::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        volume: f64,
    ) -> Result<Option<WasmEquivolumeValue>, JsError> {
        let candle = wc::Candle::new(low, high, low, low, volume, 0).map_err(map_err)?;
        match self.inner.update(candle) {
            Some(o) => {
                let obj = js_object!("height" => o.height, "width" => o.width);
                Ok(Some(obj.unchecked_into()))
            }
            None => Ok(None),
        }
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != volume.len() {
            return Err(JsError::new("high, low, volume must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle =
                wc::Candle::new(low[i], high[i], low[i], low[i], volume[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.height;
                out[i * 2 + 1] = o.width;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- CandleVolume ----------

#[wasm_bindgen(js_name = CandleVolume)]
pub struct WasmCandleVolume {
    inner: wc::CandleVolume,
}

#[wasm_bindgen(js_class = CandleVolume)]
impl WasmCandleVolume {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmCandleVolume, JsError> {
        Ok(Self {
            inner: wc::CandleVolume::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        open: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<WasmCandleVolumeValue>, JsError> {
        let high = open.max(close);
        let low = open.min(close);
        let candle = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        match self.inner.update(candle) {
            Some(o) => {
                let obj = js_object!("body" => o.body, "width" => o.width);
                Ok(Some(obj.unchecked_into()))
            }
            None => Ok(None),
        }
    }
    pub fn batch(
        &mut self,
        open: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if open.len() != close.len() || close.len() != volume.len() {
            return Err(JsError::new("open, close, volume must be equal length"));
        }
        let n = open.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let high = open[i].max(close[i]);
            let low = open[i].min(close[i]);
            let candle =
                wc::Candle::new(open[i], high, low, close[i], volume[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.body;
                out[i * 2 + 1] = o.width;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Frying Pan Bottom ----------

#[wasm_bindgen(js_name = FryPanBottom)]
pub struct WasmFryPanBottom {
    inner: wc::FryPanBottom,
}

#[wasm_bindgen(js_class = FryPanBottom)]
impl WasmFryPanBottom {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmFryPanBottom, JsError> {
        Ok(Self {
            inner: wc::FryPanBottom::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Dumpling Top ----------

#[wasm_bindgen(js_name = DumplingTop)]
pub struct WasmDumplingTop {
    inner: wc::DumplingTop,
}

#[wasm_bindgen(js_class = DumplingTop)]
impl WasmDumplingTop {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmDumplingTop, JsError> {
        Ok(Self {
            inner: wc::DumplingTop::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- New Price Lines ----------

#[wasm_bindgen(js_name = NewPriceLines)]
pub struct WasmNewPriceLines {
    inner: wc::NewPriceLines,
}

#[wasm_bindgen(js_class = NewPriceLines)]
impl WasmNewPriceLines {
    #[wasm_bindgen(constructor)]
    pub fn new(count: usize) -> Result<WasmNewPriceLines, JsError> {
        Ok(Self {
            inner: wc::NewPriceLines::new(count).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(high.len());
        for i in 0..high.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ---------- Market Breadth (CrossSection input) ----------
//
// A breadth tick is the per-symbol state of the whole universe, passed as four
// equal-length parallel arrays (`change`, `volume`, `newHigh`, `newLow`). The
// high/low flag arrays are numeric (non-zero is true) so the whole tick crosses
// the wasm boundary as `Float64Array`s. The universe is ragged across ticks, so
// only `update` is exposed (no `batch`), matching the other multi-input wasm
// indicators.

fn build_cross_section(
    change: &[f64],
    volume: &[f64],
    new_high: &[f64],
    new_low: &[f64],
) -> Result<wc::CrossSection, JsError> {
    if change.len() != volume.len()
        || change.len() != new_high.len()
        || change.len() != new_low.len()
    {
        return Err(JsError::new(
            "change, volume, newHigh and newLow must be equal length",
        ));
    }
    let members = (0..change.len())
        .map(|i| wc::Member::new(change[i], volume[i], new_high[i] != 0.0, new_low[i] != 0.0))
        .collect();
    wc::CrossSection::new(members, 0).map_err(map_err)
}

#[wasm_bindgen(js_name = AdvanceDecline)]
pub struct WasmAdvanceDecline {
    inner: wc::AdvanceDecline,
}

impl Default for WasmAdvanceDecline {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = AdvanceDecline)]
impl WasmAdvanceDecline {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmAdvanceDecline {
        Self {
            inner: wc::AdvanceDecline::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

fn build_cross_section_above_ma(
    change: &[f64],
    volume: &[f64],
    new_high: &[f64],
    new_low: &[f64],
    above_ma: &[f64],
) -> Result<wc::CrossSection, JsError> {
    if change.len() != volume.len()
        || change.len() != new_high.len()
        || change.len() != new_low.len()
        || change.len() != above_ma.len()
    {
        return Err(JsError::new(
            "change, volume, newHigh, newLow and aboveMa must be equal length",
        ));
    }
    let members = (0..change.len())
        .map(|i| {
            wc::Member::with_signals(
                change[i],
                volume[i],
                new_high[i] != 0.0,
                new_low[i] != 0.0,
                above_ma[i] != 0.0,
                false,
            )
        })
        .collect();
    wc::CrossSection::new(members, 0).map_err(map_err)
}

fn build_cross_section_buy(
    change: &[f64],
    volume: &[f64],
    new_high: &[f64],
    new_low: &[f64],
    on_buy_signal: &[f64],
) -> Result<wc::CrossSection, JsError> {
    if change.len() != volume.len()
        || change.len() != new_high.len()
        || change.len() != new_low.len()
        || change.len() != on_buy_signal.len()
    {
        return Err(JsError::new(
            "change, volume, newHigh, newLow and onBuySignal must be equal length",
        ));
    }
    let members = (0..change.len())
        .map(|i| {
            wc::Member::with_signals(
                change[i],
                volume[i],
                new_high[i] != 0.0,
                new_low[i] != 0.0,
                false,
                on_buy_signal[i] != 0.0,
            )
        })
        .collect();
    wc::CrossSection::new(members, 0).map_err(map_err)
}

#[wasm_bindgen(js_name = AdvanceDeclineRatio)]
pub struct WasmAdvanceDeclineRatio {
    inner: wc::AdvanceDeclineRatio,
}

impl Default for WasmAdvanceDeclineRatio {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = AdvanceDeclineRatio)]
impl WasmAdvanceDeclineRatio {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmAdvanceDeclineRatio {
        Self {
            inner: wc::AdvanceDeclineRatio::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = AdVolumeLine)]
pub struct WasmAdVolumeLine {
    inner: wc::AdVolumeLine,
}

impl Default for WasmAdVolumeLine {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = AdVolumeLine)]
impl WasmAdVolumeLine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmAdVolumeLine {
        Self {
            inner: wc::AdVolumeLine::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = McClellanOscillator)]
pub struct WasmMcClellanOscillator {
    inner: wc::McClellanOscillator,
}

impl Default for WasmMcClellanOscillator {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = McClellanOscillator)]
impl WasmMcClellanOscillator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmMcClellanOscillator {
        Self {
            inner: wc::McClellanOscillator::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = McClellanSummationIndex)]
pub struct WasmMcClellanSummationIndex {
    inner: wc::McClellanSummationIndex,
}

impl Default for WasmMcClellanSummationIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = McClellanSummationIndex)]
impl WasmMcClellanSummationIndex {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmMcClellanSummationIndex {
        Self {
            inner: wc::McClellanSummationIndex::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = Trin)]
pub struct WasmTrin {
    inner: wc::Trin,
}

impl Default for WasmTrin {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = Trin)]
impl WasmTrin {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmTrin {
        Self {
            inner: wc::Trin::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = BreadthThrust)]
pub struct WasmBreadthThrust {
    inner: wc::BreadthThrust,
}

#[wasm_bindgen(js_class = BreadthThrust)]
impl WasmBreadthThrust {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmBreadthThrust, JsError> {
        Ok(WasmBreadthThrust {
            inner: wc::BreadthThrust::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = NewHighsNewLows)]
pub struct WasmNewHighsNewLows {
    inner: wc::NewHighsNewLows,
}

impl Default for WasmNewHighsNewLows {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = NewHighsNewLows)]
impl WasmNewHighsNewLows {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmNewHighsNewLows {
        Self {
            inner: wc::NewHighsNewLows::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = HighLowIndex)]
pub struct WasmHighLowIndex {
    inner: wc::HighLowIndex,
}

#[wasm_bindgen(js_class = HighLowIndex)]
impl WasmHighLowIndex {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmHighLowIndex, JsError> {
        Ok(WasmHighLowIndex {
            inner: wc::HighLowIndex::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = PercentAboveMa)]
pub struct WasmPercentAboveMa {
    inner: wc::PercentAboveMa,
}

impl Default for WasmPercentAboveMa {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = PercentAboveMa)]
impl WasmPercentAboveMa {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmPercentAboveMa {
        Self {
            inner: wc::PercentAboveMa::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
        above_ma: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(build_cross_section_above_ma(
            &change, &volume, &new_high, &new_low, &above_ma,
        )?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
        above_ma: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
            || above_ma.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low, above_ma must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                    above_ma[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = UpDownVolumeRatio)]
pub struct WasmUpDownVolumeRatio {
    inner: wc::UpDownVolumeRatio,
}

impl Default for WasmUpDownVolumeRatio {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = UpDownVolumeRatio)]
impl WasmUpDownVolumeRatio {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmUpDownVolumeRatio {
        Self {
            inner: wc::UpDownVolumeRatio::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = BullishPercentIndex)]
pub struct WasmBullishPercentIndex {
    inner: wc::BullishPercentIndex,
}

impl Default for WasmBullishPercentIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = BullishPercentIndex)]
impl WasmBullishPercentIndex {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmBullishPercentIndex {
        Self {
            inner: wc::BullishPercentIndex::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
        on_buy_signal: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self.inner.update(build_cross_section_buy(
            &change,
            &volume,
            &new_high,
            &new_low,
            &on_buy_signal,
        )?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
        on_buy_signal: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
            || on_buy_signal.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low, on_buy_signal must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                    on_buy_signal[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = CumulativeVolumeIndex)]
pub struct WasmCumulativeVolumeIndex {
    inner: wc::CumulativeVolumeIndex,
}

impl Default for WasmCumulativeVolumeIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = CumulativeVolumeIndex)]
impl WasmCumulativeVolumeIndex {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmCumulativeVolumeIndex {
        Self {
            inner: wc::CumulativeVolumeIndex::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = AbsoluteBreadthIndex)]
pub struct WasmAbsoluteBreadthIndex {
    inner: wc::AbsoluteBreadthIndex,
}

impl Default for WasmAbsoluteBreadthIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = AbsoluteBreadthIndex)]
impl WasmAbsoluteBreadthIndex {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmAbsoluteBreadthIndex {
        Self {
            inner: wc::AbsoluteBreadthIndex::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = TickIndex)]
pub struct WasmTickIndex {
    inner: wc::TickIndex,
}

impl Default for WasmTickIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = TickIndex)]
impl WasmTickIndex {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmTickIndex {
        Self {
            inner: wc::TickIndex::new(),
        }
    }
    pub fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<f64>,
        new_low: Vec<f64>,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        change: Vec<Float64Array>,
        volume: Vec<Float64Array>,
        new_high: Vec<Float64Array>,
        new_low: Vec<Float64Array>,
    ) -> Result<Float64Array, JsError> {
        if volume.len() != change.len()
            || new_high.len() != change.len()
            || new_low.len() != change.len()
        {
            return Err(JsError::new(
                "change, volume, new_high, new_low must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            out.push(
                self.update(
                    change[i].to_vec(),
                    volume[i].to_vec(),
                    new_high[i].to_vec(),
                    new_low[i].to_vec(),
                )?
                .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn sma_batch_reference_values() {
        // SMA(3) of [2, 4, 6, 8, 10] -> [NaN, NaN, 4, 6, 8].
        let mut sma = WasmSma::new(3).expect("valid period");
        let out = sma.batch(&[2.0, 4.0, 6.0, 8.0, 10.0]);
        assert!(out.get_index(0).is_nan());
        assert!(out.get_index(1).is_nan());
        assert_eq!(out.get_index(2), 4.0);
        assert_eq!(out.get_index(3), 6.0);
        assert_eq!(out.get_index(4), 8.0);
    }

    #[wasm_bindgen_test]
    fn ema_batch_equals_streaming() {
        let prices: Vec<f64> = (1..=60)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let batch = WasmEma::new(14).expect("valid").batch(&prices);
        let mut ema = WasmEma::new(14).expect("valid");
        for (i, &p) in prices.iter().enumerate() {
            let b = batch.get_index(i as u32);
            match ema.update(p) {
                Some(v) => assert!((v - b).abs() < 1e-9, "streaming != batch at {i}"),
                None => assert!(b.is_nan(), "expected NaN during warmup at {i}"),
            }
        }
    }

    #[wasm_bindgen_test]
    fn rsi_pure_uptrend_yields_100() {
        let prices: Vec<f64> = (1..=20).map(f64::from).collect();
        let out = WasmRsi::new(14).expect("valid").batch(&prices);
        for i in 14..prices.len() {
            assert_eq!(out.get_index(i as u32), 100.0);
        }
    }

    #[wasm_bindgen_test]
    fn invalid_constructors_return_err() {
        assert!(WasmSma::new(0).is_err());
        assert!(WasmMacd::new(0, 0, 0).is_err());
        assert!(WasmMacd::new(26, 12, 9).is_err());
        assert!(WasmBb::new(20, -1.0).is_err());
        assert!(WasmPsar::new(0.30, 0.02, 0.20).is_err());
    }

    #[wasm_bindgen_test]
    fn batch_rejects_unequal_lengths() {
        let mut atr = WasmAtr::new(14).expect("valid");
        assert!(atr.batch(&[1.0, 2.0], &[1.0], &[1.0, 2.0]).is_err());
        let mut stoch = WasmStoch::new(14, 3).expect("valid");
        assert!(stoch
            .batch(&[1.0, 2.0, 3.0], &[1.0], &[1.0, 2.0, 3.0])
            .is_err());
        let mut mfi = WasmMfi::new(14).expect("valid");
        assert!(mfi
            .batch(&[1.0, 2.0], &[1.0, 2.0], &[1.0, 2.0], &[1.0])
            .is_err());
    }

    // ---------- Streaming-API coverage for the candle-input indicators
    // (Adx, WilliamsR, Cci, Mfi, Psar, Keltner, Donchian, Vwap, AwesomeOscillator,
    //  Aroon, Stochastic, Obv). Verifies that the per-tick `update` produces
    // exactly the same per-row values as `batch` and that the lifecycle methods
    // (`reset`, `isReady`, `warmupPeriod`) honour the same contract as Python /
    // Node and as the other already-wired WASM classes.

    /// Deterministic OHLCV stream long enough to clear every indicator's warmup
    /// (the longest in this group is `Adx(14)` at `2*period = 28` bars).
    fn synthetic_ohlcv(n: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
        let mut high = Vec::with_capacity(n);
        let mut low = Vec::with_capacity(n);
        let mut close = Vec::with_capacity(n);
        let mut volume = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f64;
            let mid = 100.0 + (t * 0.17).sin() * 5.0 + t * 0.05;
            let spread = 1.0 + (t * 0.31).cos().abs();
            high.push(mid + spread);
            low.push(mid - spread);
            close.push(mid + (t * 0.07).sin() * 0.5);
            volume.push(1_000.0 + (t * 0.21).sin().abs() * 500.0);
        }
        (high, low, close, volume)
    }

    fn close_enough(a: f64, b: f64) -> bool {
        if a.is_nan() {
            b.is_nan()
        } else if a == b {
            // Exact equality, including matching infinities (e.g. ProfitFactor
            // with no losing trades is +inf in both the streaming and batch
            // passes). `(inf - inf).abs()` is NaN, so the tolerance check below
            // would otherwise reject two equal infinities.
            true
        } else {
            (a - b).abs() < 1e-9
        }
    }

    // Single test deliberately exercises every newly wired class so the lifecycle
    // contract (`update == batch` + `is_ready` + `warmup_period` + `reset`) lives
    // in one auditable place. `h`/`l`/`c`/`v` mirror the OHLCV column names used
    // by the production API and the rest of the test suite.
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)]
    #[wasm_bindgen_test]
    fn candle_input_streaming_matches_batch_and_lifecycle_is_consistent() {
        let (h, l, c, v) = synthetic_ohlcv(40);

        // --- ADX (multi: { plusDi, minusDi, adx }) ---
        let batch = WasmAdx::new(7)
            .expect("valid")
            .batch(&h, &l, &c)
            .expect("valid");
        let mut adx = WasmAdx::new(7).expect("valid");
        assert!(!adx.is_ready());
        assert_eq!(
            adx.warmup_period(),
            wc::Adx::new(7).expect("valid").warmup_period()
        );
        for i in 0..h.len() {
            let stream = adx.update(h[i], l[i], c[i]).expect("valid");
            let bp = batch.get_index((i * 3) as u32);
            let bm = batch.get_index((i * 3 + 1) as u32);
            let ba = batch.get_index((i * 3 + 2) as u32);
            if let Some(value) = &stream {
                let obj: &Object = value.unchecked_ref();
                let p = Reflect::get(obj, &key("plusDi")).unwrap().as_f64().unwrap();
                let m = Reflect::get(obj, &key("minusDi"))
                    .unwrap()
                    .as_f64()
                    .unwrap();
                let a = Reflect::get(obj, &key("adx")).unwrap().as_f64().unwrap();
                assert!(close_enough(p, bp), "ADX plusDi diverges at {i}");
                assert!(close_enough(m, bm), "ADX minusDi diverges at {i}");
                assert!(close_enough(a, ba), "ADX value diverges at {i}");
            } else {
                assert!(bp.is_nan() && bm.is_nan() && ba.is_nan(), "row {i}");
            }
        }
        assert!(adx.is_ready());
        adx.reset();
        assert!(!adx.is_ready());

        // --- WilliamsR (single) ---
        let batch = WasmWilliamsR::new(14)
            .expect("valid")
            .batch(&h, &l, &c)
            .expect("valid");
        let mut wr = WasmWilliamsR::new(14).expect("valid");
        for i in 0..h.len() {
            let s = wr
                .update(h[i], l[i], c[i])
                .expect("valid")
                .unwrap_or(f64::NAN);
            assert!(
                close_enough(s, batch.get_index(i as u32)),
                "WilliamsR at {i}"
            );
        }
        assert!(wr.is_ready());
        assert_eq!(
            wr.warmup_period(),
            wc::WilliamsR::new(14).expect("valid").warmup_period()
        );
        wr.reset();
        assert!(!wr.is_ready());

        // --- CCI (single) ---
        let batch = WasmCci::new(14)
            .expect("valid")
            .batch(&h, &l, &c)
            .expect("valid");
        let mut cci = WasmCci::new(14).expect("valid");
        for i in 0..h.len() {
            let s = cci
                .update(h[i], l[i], c[i])
                .expect("valid")
                .unwrap_or(f64::NAN);
            assert!(close_enough(s, batch.get_index(i as u32)), "CCI at {i}");
        }

        // --- MFI (single, with volume) ---
        let batch = WasmMfi::new(14)
            .expect("valid")
            .batch(&h, &l, &c, &v)
            .expect("valid");
        let mut mfi = WasmMfi::new(14).expect("valid");
        for i in 0..h.len() {
            let s = mfi
                .update(h[i], l[i], c[i], v[i])
                .expect("valid")
                .unwrap_or(f64::NAN);
            assert!(close_enough(s, batch.get_index(i as u32)), "MFI at {i}");
        }

        // --- PSAR (single) ---
        let batch = WasmPsar::new(0.02, 0.02, 0.2)
            .expect("valid")
            .batch(&h, &l, &c)
            .expect("valid");
        let mut psar = WasmPsar::new(0.02, 0.02, 0.2).expect("valid");
        for i in 0..h.len() {
            let s = psar
                .update(h[i], l[i], c[i])
                .expect("valid")
                .unwrap_or(f64::NAN);
            assert!(close_enough(s, batch.get_index(i as u32)), "PSAR at {i}");
        }

        // --- Keltner (multi: { upper, middle, lower }) ---
        let batch = WasmKeltner::new(10, 10, 2.0)
            .expect("valid")
            .batch(&h, &l, &c)
            .expect("valid");
        let mut kc = WasmKeltner::new(10, 10, 2.0).expect("valid");
        for i in 0..h.len() {
            let stream = kc.update(h[i], l[i], c[i]).expect("valid");
            let bu = batch.get_index((i * 3) as u32);
            let bm = batch.get_index((i * 3 + 1) as u32);
            let bl = batch.get_index((i * 3 + 2) as u32);
            if let Some(value) = &stream {
                let obj: &Object = value.unchecked_ref();
                let u = Reflect::get(obj, &key("upper")).unwrap().as_f64().unwrap();
                let m = Reflect::get(obj, &key("middle")).unwrap().as_f64().unwrap();
                let lo = Reflect::get(obj, &key("lower")).unwrap().as_f64().unwrap();
                assert!(close_enough(u, bu), "Keltner upper at {i}");
                assert!(close_enough(m, bm), "Keltner middle at {i}");
                assert!(close_enough(lo, bl), "Keltner lower at {i}");
            } else {
                assert!(bu.is_nan() && bm.is_nan() && bl.is_nan(), "Keltner row {i}");
            }
        }

        // --- Donchian (multi: { upper, middle, lower }) ---
        let batch = WasmDonchian::new(10)
            .expect("valid")
            .batch(&h, &l)
            .expect("valid");
        let mut dc = WasmDonchian::new(10).expect("valid");
        for i in 0..h.len() {
            let stream = dc.update(h[i], l[i]).expect("valid");
            let bu = batch.get_index((i * 3) as u32);
            let bm = batch.get_index((i * 3 + 1) as u32);
            let bl = batch.get_index((i * 3 + 2) as u32);
            if let Some(value) = &stream {
                let obj: &Object = value.unchecked_ref();
                let u = Reflect::get(obj, &key("upper")).unwrap().as_f64().unwrap();
                let m = Reflect::get(obj, &key("middle")).unwrap().as_f64().unwrap();
                let lo = Reflect::get(obj, &key("lower")).unwrap().as_f64().unwrap();
                assert!(close_enough(u, bu), "Donchian upper at {i}");
                assert!(close_enough(m, bm), "Donchian middle at {i}");
                assert!(close_enough(lo, bl), "Donchian lower at {i}");
            } else {
                assert!(
                    bu.is_nan() && bm.is_nan() && bl.is_nan(),
                    "Donchian row {i}"
                );
            }
        }

        // --- VWAP (single, cumulative, with volume) ---
        let batch = WasmVwap::new().batch(&h, &l, &c, &v).expect("valid");
        let mut vwap = WasmVwap::new();
        for i in 0..h.len() {
            let s = vwap
                .update(h[i], l[i], c[i], v[i])
                .expect("valid")
                .unwrap_or(f64::NAN);
            assert!(close_enough(s, batch.get_index(i as u32)), "VWAP at {i}");
        }

        // --- AwesomeOscillator (single) ---
        let batch = WasmAo::new(5, 34)
            .expect("valid")
            .batch(&h, &l)
            .expect("valid");
        let mut ao = WasmAo::new(5, 34).expect("valid");
        for i in 0..h.len() {
            let s = ao.update(h[i], l[i]).expect("valid").unwrap_or(f64::NAN);
            assert!(close_enough(s, batch.get_index(i as u32)), "AO at {i}");
        }

        // --- Aroon (multi: { up, down }) ---
        let batch = WasmAroon::new(14)
            .expect("valid")
            .batch(&h, &l)
            .expect("valid");
        let mut aroon = WasmAroon::new(14).expect("valid");
        for i in 0..h.len() {
            let stream = aroon.update(h[i], l[i]).expect("valid");
            let bu = batch.get_index((i * 2) as u32);
            let bd = batch.get_index((i * 2 + 1) as u32);
            if let Some(value) = &stream {
                let obj: &Object = value.unchecked_ref();
                let u = Reflect::get(obj, &key("up")).unwrap().as_f64().unwrap();
                let d = Reflect::get(obj, &key("down")).unwrap().as_f64().unwrap();
                assert!(close_enough(u, bu), "Aroon up at {i}");
                assert!(close_enough(d, bd), "Aroon down at {i}");
            } else {
                assert!(bu.is_nan() && bd.is_nan(), "Aroon row {i}");
            }
        }

        // --- Stochastic (multi: { k, d }) ---
        let batch = WasmStoch::new(14, 3)
            .expect("valid")
            .batch(&h, &l, &c)
            .expect("valid");
        let mut st = WasmStoch::new(14, 3).expect("valid");
        for i in 0..h.len() {
            let stream = st.update(h[i], l[i], c[i]).expect("valid");
            let bk = batch.get_index((i * 2) as u32);
            let bd = batch.get_index((i * 2 + 1) as u32);
            if let Some(value) = &stream {
                let obj: &Object = value.unchecked_ref();
                let k = Reflect::get(obj, &key("k")).unwrap().as_f64().unwrap();
                let d = Reflect::get(obj, &key("d")).unwrap().as_f64().unwrap();
                assert!(close_enough(k, bk), "Stoch k at {i}");
                assert!(close_enough(d, bd), "Stoch d at {i}");
            } else {
                assert!(bk.is_nan() && bd.is_nan(), "Stoch row {i}");
            }
        }

        // --- OBV (single, with volume; no warmup) ---
        let batch = WasmObv::new().batch(&c, &v).expect("valid");
        let mut obv = WasmObv::new();
        for i in 0..c.len() {
            let s = obv.update(c[i], v[i]).expect("valid").unwrap_or(f64::NAN);
            assert!(close_enough(s, batch.get_index(i as u32)), "OBV at {i}");
        }
    }

    #[allow(clippy::many_single_char_names)]
    #[wasm_bindgen_test]
    fn rolling_vwap_streaming_matches_batch_and_lifecycle() {
        // R4: RollingVwap is now exposed in WASM (previously Rust-only despite
        // the README listing it as a cross-language indicator).
        let (h, l, c, v) = synthetic_ohlcv(50);
        let batch = WasmRollingVwap::new(10)
            .expect("valid")
            .batch(&h, &l, &c, &v)
            .expect("valid");
        let mut rv = WasmRollingVwap::new(10).expect("valid");
        assert_eq!(rv.warmup_period(), 10);
        assert_eq!(rv.period(), 10);
        assert!(!rv.is_ready());
        for i in 0..h.len() {
            let s = rv
                .update(h[i], l[i], c[i], v[i])
                .expect("valid")
                .unwrap_or(f64::NAN);
            assert!(
                close_enough(s, batch.get_index(i as u32)),
                "RollingVWAP at {i}"
            );
        }
        assert!(rv.is_ready());
        rv.reset();
        assert!(!rv.is_ready());
        assert!(WasmRollingVwap::new(0).is_err());
    }

    #[wasm_bindgen_test]
    fn kama_exposes_warmup_period() {
        // R8: the KAMA wrapper was missing `warmupPeriod`; it now matches the
        // core indicator's value (which is `er_period + 1`).
        let kama = WasmKama::new(10, 2, 30).expect("valid");
        assert_eq!(
            kama.warmup_period(),
            wc::Kama::new(10, 2, 30).expect("valid").warmup_period()
        );
    }

    // === Extended coverage: one wasm_bindgen_test per family ===========
    //
    // The bulk lifecycle test above proves the candle-input contract on a
    // representative set; the cases below add a per-family spot-check so a
    // regression that only manifests for a specific indicator (output shape,
    // multi-output unwrap, etc.) cannot slip through.

    #[wasm_bindgen_test]
    fn macd_multi_output_batch_shape() {
        // Family 4 — Price Oscillators. Macd emits a {macd, signal, histogram}
        // record; batch returns a Float64Array packing all three components
        // back-to-back. Length must be 3 * series_len.
        let prices: Vec<f64> = (0..60).map(|i| 100.0 + f64::from(i) * 0.1).collect();
        let batch = WasmMacd::new(12, 26, 9).expect("valid").batch(&prices);
        assert_eq!(batch.length() as usize, 3 * prices.len());
    }

    #[wasm_bindgen_test]
    fn bollinger_batch_orders_upper_mid_lower() {
        // Family 5 — Bollinger emits {upper, middle, lower} per bar. On any
        // ready output, upper >= middle >= lower must hold.
        let prices: Vec<f64> = (0..60)
            .map(|i| 100.0 + (f64::from(i) * 0.3).sin() * 5.0)
            .collect();
        let batch = WasmBb::new(20, 2.0).expect("valid").batch(&prices);
        // Batch layout is `[u0, m0, l0, sd0, u1, m1, l1, sd1, ...]` — four
        // floats per bar (upper / middle / lower / stddev). First ready bar
        // with period=20 is index 19.
        let start = 19 * 4;
        let upper = batch.get_index(start as u32);
        let middle = batch.get_index((start + 1) as u32);
        let lower = batch.get_index((start + 2) as u32);
        assert!(upper.is_finite() && middle.is_finite() && lower.is_finite());
        assert!(
            upper >= middle,
            "upper {upper} should be >= middle {middle}"
        );
        assert!(
            middle >= lower,
            "middle {middle} should be >= lower {lower}"
        );
    }

    #[wasm_bindgen_test]
    fn fisher_transform_streaming_roundtrip() {
        // Family 10 — Ehlers / Cycle. FisherTransform compresses bounded inputs
        // and is highly recursive — verify streaming runs cleanly across a sine
        // wave without panic or NaN explosion after warmup.
        let prices: Vec<f64> = (0..100)
            .map(|i| 100.0 + (f64::from(i) * 0.2).sin() * 5.0)
            .collect();
        let mut ind = WasmFisherTransform::new(10).expect("valid");
        let mut produced = 0usize;
        for &p in &prices {
            if let Some(v) = ind.update(p) {
                assert!(v.is_finite(), "FisherTransform produced non-finite output");
                produced += 1;
            }
        }
        assert!(
            produced > 50,
            "FisherTransform should emit on most bars after warmup"
        );
    }

    #[wasm_bindgen_test]
    fn sharpe_ratio_reset_clears_state() {
        // Family 16 — Risk / Performance. Confirms reset semantics survive the
        // wasm-bindgen boundary on a metric whose state is the rolling-return
        // window plus running sum / sum-of-squares.
        let mut sr = WasmSharpeRatio::new(10, 0.0).expect("valid");
        for i in 0..15 {
            sr.update(f64::from(i) * 0.001);
        }
        assert!(sr.is_ready(), "SharpeRatio should be ready after warmup");
        sr.reset();
        assert!(
            !sr.is_ready(),
            "SharpeRatio should not be ready after reset"
        );
        assert!(
            sr.update(0.001).is_none(),
            "post-reset update should warmup again"
        );
    }

    #[wasm_bindgen_test]
    fn max_drawdown_monotone_uptrend_yields_zero_after_warmup() {
        // Family 16 — Risk / Performance. Strictly-increasing equity curve has
        // zero drawdown; the bounded-window MaxDrawdown should reflect that.
        let mut md = WasmMaxDrawdown::new(10).expect("valid");
        let mut last = None;
        for i in 1..=20 {
            last = md.update(f64::from(i));
        }
        let final_dd = last.expect("ready after 20 inputs");
        assert!(
            (final_dd).abs() < 1e-12,
            "monotone uptrend should yield max-drawdown == 0, got {final_dd}"
        );
    }

    #[wasm_bindgen_test]
    fn value_at_risk_constructor_rejects_invalid() {
        // Family 16 — Risk / Performance. VaR's confidence must be in (0, 1)
        // and period >= 2; the constructor must surface those as JsError
        // across the binding boundary.
        assert!(
            WasmValueAtRisk::new(1, 0.95).is_err(),
            "period < 2 should reject"
        );
        assert!(
            WasmValueAtRisk::new(20, 0.0).is_err(),
            "confidence == 0 should reject"
        );
        assert!(
            WasmValueAtRisk::new(20, 1.0).is_err(),
            "confidence == 1 should reject"
        );
        assert!(
            WasmValueAtRisk::new(20, 0.95).is_ok(),
            "valid params should construct"
        );
    }

    #[wasm_bindgen_test]
    fn reset_returns_indicator_to_warmup() {
        // Sanity across two macro-generated scalar indicators. Each must
        // report `is_ready() == false` immediately after `reset()`. The
        // hand-coded candle wrappers (ATR, Stoch, etc.) do not expose
        // `is_ready` on the JS surface; their lifecycle is covered by
        // the bulk `candle_input_streaming_matches_batch_and_lifecycle`
        // test above instead.
        let mut sma = WasmSma::new(5).expect("valid");
        for i in 1..=10 {
            sma.update(f64::from(i));
        }
        assert!(sma.is_ready());
        sma.reset();
        assert!(!sma.is_ready());

        let mut ema = WasmEma::new(14).expect("valid");
        for i in 1..=20 {
            ema.update(f64::from(i));
        }
        assert!(ema.is_ready());
        ema.reset();
        assert!(!ema.is_ready());
    }

    #[wasm_bindgen_test]
    fn warmup_period_matches_core_for_baseline_scalar() {
        // The binding must report the same warmup as the underlying wickra-core
        // indicator. Drift here would silently break "wait for first non-NaN"
        // user code.
        let sma = WasmSma::new(20).expect("valid");
        assert_eq!(
            sma.warmup_period(),
            wc::Sma::new(20).expect("valid").warmup_period()
        );
        let ema = WasmEma::new(14).expect("valid");
        assert_eq!(
            ema.warmup_period(),
            wc::Ema::new(14).expect("valid").warmup_period()
        );
        let rsi = WasmRsi::new(14).expect("valid");
        assert_eq!(
            rsi.warmup_period(),
            wc::Rsi::new(14).expect("valid").warmup_period()
        );
    }

    #[wasm_bindgen_test]
    fn nan_input_is_rejected_without_state_mutation() {
        // Non-finite input must be ignored — calling update with NaN must not
        // advance the warmup counter, otherwise streaming code that fed in a
        // spurious NaN would prematurely flip to ready.
        let mut sma = WasmSma::new(5).expect("valid");
        for _ in 0..4 {
            sma.update(f64::NAN);
        }
        assert!(!sma.is_ready(), "NaN inputs must not advance warmup");
        for i in 1..=5 {
            sma.update(f64::from(i));
        }
        assert!(
            sma.is_ready(),
            "ready after 5 finite inputs even with prior NaNs"
        );
    }

    // Streaming `update` must reproduce `batch` value-for-value for every scalar
    // indicator — the core O(1) state-machine invariant. Each entry builds a
    // fresh instance for the batch pass and another for the streaming pass. The
    // constructor arguments mirror the (CI-passing) Node `indicators.test.js`
    // factories, so they are known-valid.
    macro_rules! assert_scalar_stream_eq {
        ($ctor:expr, $prices:expr) => {{
            let prices: &[f64] = $prices;
            let batch = { $ctor }.batch(prices);
            let mut streaming = { $ctor };
            for (i, &p) in prices.iter().enumerate() {
                let b = batch.get_index(i as u32);
                match streaming.update(p) {
                    Some(v) => assert!(
                        close_enough(v, b),
                        "{} streaming != batch at {i}: {v} vs {b}",
                        stringify!($ctor)
                    ),
                    None => assert!(
                        b.is_nan(),
                        "{} expected NaN warmup at {i}",
                        stringify!($ctor)
                    ),
                }
            }
        }};
    }

    #[allow(clippy::too_many_lines)]
    #[wasm_bindgen_test]
    fn scalar_streaming_matches_batch_broad() {
        let prices: Vec<f64> = (0..120)
            .map(|i| {
                let t = f64::from(i);
                100.0 + (t * 0.2).sin() * 10.0 + t * 0.1
            })
            .collect();
        let p = prices.as_slice();

        // Moving averages.
        assert_scalar_stream_eq!(WasmSma::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmWma::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmDema::new(10).expect("valid"), p);
        assert_scalar_stream_eq!(WasmTema::new(10).expect("valid"), p);
        assert_scalar_stream_eq!(WasmHma::new(9).expect("valid"), p);
        assert_scalar_stream_eq!(WasmSmma::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmTrima::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmZlema::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmT3::new(5, 0.7).expect("valid"), p);
        assert_scalar_stream_eq!(WasmAlma::new(9, 0.85, 6.0).expect("valid"), p);
        assert_scalar_stream_eq!(WasmMcGinleyDynamic::new(10).expect("valid"), p);
        assert_scalar_stream_eq!(WasmFrama::new(16).expect("valid"), p);
        assert_scalar_stream_eq!(WasmVidya::new(14, 9).expect("valid"), p);
        assert_scalar_stream_eq!(WasmJma::new(14, 0.0, 2).expect("valid"), p);

        // Momentum / oscillators.
        assert_scalar_stream_eq!(WasmRsi::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmRoc::new(12).expect("valid"), p);
        assert_scalar_stream_eq!(WasmTrix::new(9).expect("valid"), p);
        assert_scalar_stream_eq!(WasmMom::new(10).expect("valid"), p);
        assert_scalar_stream_eq!(WasmCmo::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmTsi::new(25, 13).expect("valid"), p);
        assert_scalar_stream_eq!(WasmPmo::new(35, 20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmTii::new(20, 10).expect("valid"), p);
        assert_scalar_stream_eq!(WasmStochRsi::new(14, 14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmDpo::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmPpo::new(12, 26).expect("valid"), p);
        assert_scalar_stream_eq!(WasmApo::new(12, 26).expect("valid"), p);
        assert_scalar_stream_eq!(WasmCfo::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmStc::new(23, 50, 10, 0.5).expect("valid"), p);
        assert_scalar_stream_eq!(WasmCoppock::new(14, 11, 10).expect("valid"), p);
        assert_scalar_stream_eq!(WasmLaguerreRsi::new(0.5).expect("valid"), p);
        assert_scalar_stream_eq!(WasmConnorsRsi::new(3, 2, 100).expect("valid"), p);

        // Volatility / statistics / regression.
        assert_scalar_stream_eq!(WasmStdDev::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmUlcerIndex::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmHistoricalVolatility::new(20, 252).expect("valid"), p);
        assert_scalar_stream_eq!(WasmBollingerBandwidth::new(20, 2.0).expect("valid"), p);
        assert_scalar_stream_eq!(WasmPercentB::new(20, 2.0).expect("valid"), p);
        assert_scalar_stream_eq!(WasmLinearRegression::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmLinRegSlope::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmLinRegAngle::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmVerticalHorizontalFilter::new(28).expect("valid"), p);
        assert_scalar_stream_eq!(WasmZScore::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmVariance::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmCoefficientOfVariation::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmSkewness::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmKurtosis::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmStandardError::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmDetrendedStdDev::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmRSquared::new(14).expect("valid"), p);
        assert_scalar_stream_eq!(WasmMedianAbsoluteDeviation::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmAutocorrelation::new(20, 1).expect("valid"), p);
        assert_scalar_stream_eq!(WasmHurstExponent::new(40, 4).expect("valid"), p);
        assert_scalar_stream_eq!(WasmRviVolatility::new(10).expect("valid"), p);

        // Ehlers / cycle.
        assert_scalar_stream_eq!(WasmSuperSmoother::new(10).expect("valid"), p);
        assert_scalar_stream_eq!(WasmFisherTransform::new(10).expect("valid"), p);
        assert_scalar_stream_eq!(WasmInverseFisherTransform::new(1.0).expect("valid"), p);
        assert_scalar_stream_eq!(WasmDecycler::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmDecyclerOscillator::new(10, 30).expect("valid"), p);
        assert_scalar_stream_eq!(WasmRoofingFilter::new(10, 48).expect("valid"), p);
        assert_scalar_stream_eq!(WasmCenterOfGravity::new(10).expect("valid"), p);
        assert_scalar_stream_eq!(WasmCyberneticCycle::new(10).expect("valid"), p);
        assert_scalar_stream_eq!(WasmInstantaneousTrendline::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmEhlersStochastic::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(
            WasmEmpiricalModeDecomposition::new(20, 0.5).expect("valid"),
            p
        );
        assert_scalar_stream_eq!(WasmFama::new(0.5, 0.05).expect("valid"), p);

        // Risk / performance (scalar f64 input).
        assert_scalar_stream_eq!(WasmCalmarRatio::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmMaxDrawdown::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmAverageDrawdown::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmPainIndex::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmProfitFactor::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmGainLossRatio::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmKellyCriterion::new(20).expect("valid"), p);
        assert_scalar_stream_eq!(WasmSharpeRatio::new(20, 0.0).expect("valid"), p);
        assert_scalar_stream_eq!(WasmSortinoRatio::new(20, 0.0).expect("valid"), p);
        assert_scalar_stream_eq!(WasmOmegaRatio::new(20, 0.0).expect("valid"), p);
        assert_scalar_stream_eq!(WasmValueAtRisk::new(20, 0.95).expect("valid"), p);
        assert_scalar_stream_eq!(WasmConditionalValueAtRisk::new(20, 0.95).expect("valid"), p);
    }

    // Additional invalid-constructor coverage. These wrap the same fallible core
    // `new` as the Python / Node bindings, where the equivalent calls are
    // confirmed to error.
    #[wasm_bindgen_test]
    fn additional_invalid_constructors_are_rejected() {
        assert!(WasmDecyclerOscillator::new(30, 10).is_err()); // short cutoff >= long
        assert!(WasmRoofingFilter::new(48, 10).is_err()); // lowpass >= highpass
        assert!(WasmInverseFisherTransform::new(0.0).is_err()); // zero scale
        assert!(WasmEmpiricalModeDecomposition::new(20, 0.0).is_err()); // zero fraction
    }
}
// ============================== Family 15: Risk / Performance ==============================

// Most metrics need fallible `new` (period >= 2), so they're written by hand
// rather than going through `wasm_scalar_indicator!`. Single-parameter helpers
// reuse the same patterns as the rest of the file.

wasm_scalar_indicator!(WasmCalmarRatio, "CalmarRatio", wc::CalmarRatio, period: usize);
wasm_scalar_indicator!(WasmMaxDrawdown, "MaxDrawdown", wc::MaxDrawdown, period: usize);
wasm_scalar_indicator!(WasmAverageDrawdown, "AverageDrawdown", wc::AverageDrawdown, period: usize);
wasm_scalar_indicator!(WasmPainIndex, "PainIndex", wc::PainIndex, period: usize);
wasm_scalar_indicator!(WasmProfitFactor, "ProfitFactor", wc::ProfitFactor, period: usize);
wasm_scalar_indicator!(WasmGainLossRatio, "GainLossRatio", wc::GainLossRatio, period: usize);
wasm_scalar_indicator!(WasmKellyCriterion, "KellyCriterion", wc::KellyCriterion, period: usize);
wasm_scalar_indicator!(WasmSharpeRatio, "SharpeRatio", wc::SharpeRatio, period: usize, risk_free: f64);
wasm_scalar_indicator!(WasmSortinoRatio, "SortinoRatio", wc::SortinoRatio, period: usize, mar: f64);
wasm_scalar_indicator!(WasmOmegaRatio, "OmegaRatio", wc::OmegaRatio, period: usize, threshold: f64);
wasm_scalar_indicator!(WasmValueAtRisk, "ValueAtRisk", wc::ValueAtRisk, period: usize, confidence: f64);
wasm_scalar_indicator!(WasmConditionalValueAtRisk, "ConditionalValueAtRisk", wc::ConditionalValueAtRisk, period: usize, confidence: f64);
wasm_scalar_indicator!(WasmMidPoint, "MIDPOINT", wc::MidPoint, period: usize);
wasm_scalar_indicator!(WasmRocp, "ROCP", wc::Rocp, period: usize);
wasm_scalar_indicator!(WasmRocr, "ROCR", wc::Rocr, period: usize);
wasm_scalar_indicator!(WasmRocr100, "ROCR100", wc::Rocr100, period: usize);
wasm_scalar_indicator!(WasmLinRegIntercept, "LINEARREG_INTERCEPT", wc::LinRegIntercept, period: usize);
wasm_scalar_indicator!(WasmTsf, "TSF", wc::Tsf, period: usize);
wasm_scalar_indicator!(WasmLogReturn, "LogReturn", wc::LogReturn, period: usize);
wasm_scalar_indicator!(WasmRealizedVolatility, "RealizedVolatility", wc::RealizedVolatility, period: usize);
wasm_scalar_indicator!(WasmRollingIqr, "RollingIqr", wc::RollingIqr, period: usize);
wasm_scalar_indicator!(WasmRollingPercentileRank, "RollingPercentileRank", wc::RollingPercentileRank, period: usize);
wasm_scalar_indicator!(WasmRollingQuantile, "RollingQuantile", wc::RollingQuantile, period: usize, quantile: f64);
wasm_scalar_indicator!(WasmTrendLabel, "TrendLabel", wc::TrendLabel, period: usize);
wasm_scalar_indicator!(WasmJumpIndicator, "JumpIndicator", wc::JumpIndicator, period: usize, threshold: f64);
wasm_scalar_indicator!(WasmRegimeLabel, "RegimeLabel", wc::RegimeLabel, vol_period: usize, lookback: usize);
wasm_scalar_indicator!(WasmWinRate, "WinRate", wc::WinRate, period: usize);
wasm_scalar_indicator!(WasmExpectancy, "Expectancy", wc::Expectancy, period: usize);
wasm_scalar_indicator!(WasmSineWeightedMa, "SWMA", wc::SineWeightedMa, period: usize);
wasm_scalar_indicator!(WasmGeometricMa, "GMA", wc::GeometricMa, period: usize);
wasm_scalar_indicator!(WasmEhma, "EHMA", wc::Ehma, period: usize);
wasm_scalar_indicator!(WasmMedianMa, "MedianMA", wc::MedianMa, period: usize);
wasm_scalar_indicator!(WasmAdaptiveLaguerreFilter, "AdaptiveLaguerre", wc::AdaptiveLaguerreFilter, period: usize);
wasm_scalar_indicator!(WasmGeneralizedDema, "GD", wc::GeneralizedDema, period: usize, v: f64);
wasm_scalar_indicator!(WasmHoltWinters, "HoltWinters", wc::HoltWinters, alpha: f64, beta: f64);
wasm_scalar_indicator!(WasmDisparityIndex, "DisparityIndex", wc::DisparityIndex, period: usize);
wasm_scalar_indicator!(WasmFisherRsi, "FisherRSI", wc::FisherRsi, period: usize);
wasm_scalar_indicator!(WasmRsx, "RSX", wc::Rsx, period: usize);
wasm_scalar_indicator!(WasmDynamicMomentumIndex, "DynamicMomentumIndex", wc::DynamicMomentumIndex, period: usize);
wasm_scalar_indicator!(WasmRmi, "RMI", wc::Rmi, period: usize, momentum: usize);
wasm_scalar_indicator!(WasmDerivativeOscillator, "DerivativeOscillator", wc::DerivativeOscillator, rsi_period: usize, smooth1: usize, smooth2: usize, signal_period: usize);
wasm_scalar_indicator!(WasmTrendStrengthIndex, "TREND_STRENGTH_INDEX", wc::TrendStrengthIndex, period: usize);
wasm_scalar_indicator!(WasmTsfOscillator, "TsfOscillator", wc::TsfOscillator, period: usize);
wasm_scalar_indicator!(WasmMacdHistogram, "MacdHistogram", wc::MacdHistogram, fast: usize, slow: usize, signal: usize);
wasm_scalar_indicator!(WasmPpoHistogram, "PpoHistogram", wc::PpoHistogram, fast: usize, slow: usize, signal: usize);
wasm_scalar_indicator!(WasmBipowerVariation, "BipowerVariation", wc::BipowerVariation, period: usize);
wasm_scalar_indicator!(WasmEwmaVolatility, "EwmaVolatility", wc::EwmaVolatility, lambda: f64);
wasm_scalar_indicator!(WasmGarch11, "Garch11", wc::Garch11, omega: f64, alpha: f64, beta: f64);
wasm_scalar_indicator!(WasmVolatilityOfVolatility, "VolatilityOfVolatility", wc::VolatilityOfVolatility, vol_window: usize, vov_window: usize);
wasm_scalar_indicator!(WasmJarqueBera, "JARQUEBERA", wc::JarqueBera, period: usize);
wasm_scalar_indicator!(WasmRollingMinMaxScaler, "ROLLINGMINMAX", wc::RollingMinMaxScaler, period: usize);
wasm_scalar_indicator!(WasmShannonEntropy, "SHANNONENT", wc::ShannonEntropy, period: usize, bins: usize);
wasm_scalar_indicator!(WasmSampleEntropy, "SAMPLEENT", wc::SampleEntropy, period: usize, m: usize, r_factor: f64);
wasm_scalar_indicator!(WasmHighpassFilter, "HIGHPASS", wc::HighpassFilter, period: usize);
wasm_scalar_indicator!(WasmReflex, "REFLEX", wc::Reflex, period: usize);
wasm_scalar_indicator!(WasmTrendflex, "TRENDFLEX", wc::Trendflex, period: usize);
wasm_scalar_indicator!(WasmCorrelationTrendIndicator, "CTI", wc::CorrelationTrendIndicator, period: usize);
wasm_scalar_indicator!(WasmAdaptiveRsi, "ADAPTIVERSI", wc::AdaptiveRsi, period: usize);
wasm_scalar_indicator!(WasmUniversalOscillator, "UNIVERSALOSC", wc::UniversalOscillator, period: usize);
wasm_scalar_indicator!(WasmBandpassFilter, "BANDPASS", wc::BandpassFilter, period: usize, bandwidth: f64);
wasm_scalar_indicator!(WasmEvenBetterSinewave, "EVENBETTERSINE", wc::EvenBetterSinewave, hp_period: usize, ssf_length: usize);
wasm_scalar_indicator!(WasmAutocorrelationPeriodogram, "AUTOCORRPGRAM", wc::AutocorrelationPeriodogram, min_period: usize, max_period: usize);
wasm_scalar_indicator!(WasmSterlingRatio, "SterlingRatio", wc::SterlingRatio, period: usize);
wasm_scalar_indicator!(WasmBurkeRatio, "BurkeRatio", wc::BurkeRatio, period: usize);
wasm_scalar_indicator!(WasmMartinRatio, "MartinRatio", wc::MartinRatio, period: usize);
wasm_scalar_indicator!(WasmTailRatio, "TailRatio", wc::TailRatio, period: usize);
wasm_scalar_indicator!(WasmKRatio, "KRatio", wc::KRatio, period: usize);
wasm_scalar_indicator!(WasmCommonSenseRatio, "CommonSenseRatio", wc::CommonSenseRatio, period: usize);
wasm_scalar_indicator!(WasmGainToPainRatio, "GainToPainRatio", wc::GainToPainRatio, period: usize);
wasm_scalar_indicator!(WasmUpsidePotentialRatio, "UpsidePotentialRatio", wc::UpsidePotentialRatio, period: usize, mar: f64);
wasm_scalar_indicator!(WasmM2Measure, "M2Measure", wc::M2Measure, period: usize, risk_free: f64, benchmark_stddev: f64);

// --- VolatilityCone: Candle in, struct out (current/min/median/max/percentile) ---

#[wasm_bindgen(js_name = VolatilityCone)]
pub struct WasmVolatilityCone {
    inner: wc::VolatilityCone,
}

#[wasm_bindgen(js_class = VolatilityCone)]
impl WasmVolatilityCone {
    #[wasm_bindgen(constructor)]
    pub fn new(window: usize, lookback: usize) -> Result<WasmVolatilityCone, JsError> {
        Ok(Self {
            inner: wc::VolatilityCone::new(window, lookback).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<Option<WasmVolatilityConeValue>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("current" => o.current, "min" => o.min, "median" => o.median, "max" => o.max, "percentile" => o.percentile);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        let n = high.len();
        if low.len() != n || close.len() != n {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = vec![f64::NAN; n * 5];
        for i in 0..n {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 5] = o.current;
                out[i * 5 + 1] = o.min;
                out[i * 5 + 2] = o.median;
                out[i * 5 + 3] = o.max;
                out[i * 5 + 4] = o.percentile;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// --- DrawdownDuration: u32 output, no constructor args ---

#[wasm_bindgen(js_name = DrawdownDuration)]
pub struct WasmDrawdownDuration {
    inner: wc::DrawdownDuration,
}

impl Default for WasmDrawdownDuration {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = DrawdownDuration)]
impl WasmDrawdownDuration {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmDrawdownDuration {
        Self {
            inner: wc::DrawdownDuration::new(),
        }
    }
    pub fn update(&mut self, value: f64) -> Option<u32> {
        self.inner.update(value)
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        let out: Vec<f64> = prices
            .iter()
            .map(|p| self.inner.update(*p).map_or(f64::NAN, f64::from))
            .collect();
        Float64Array::from(out.as_slice())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// --- RecoveryFactor: no constructor args ---

#[wasm_bindgen(js_name = RecoveryFactor)]
pub struct WasmRecoveryFactor {
    inner: wc::RecoveryFactor,
}

impl Default for WasmRecoveryFactor {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = RecoveryFactor)]
impl WasmRecoveryFactor {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmRecoveryFactor {
        Self {
            inner: wc::RecoveryFactor::new(),
        }
    }
    pub fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    pub fn batch(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_nan(prices).as_slice())
    }
    /// Opt-in fast batch: a SIMD kernel may reassociate the arithmetic, so each value
    /// agrees with `batch` to within a few units in the last place rather than bit for
    /// bit; NaN placement and length are identical, and the result is the same on
    /// every platform. Without a kernel it is exactly `batch`.
    #[wasm_bindgen(js_name = batchFast)]
    pub fn batch_fast(&mut self, prices: &[f64]) -> Float64Array {
        Float64Array::from(self.inner.batch_fast(prices).as_slice())
    }
    /// `batch` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchInto)]
    pub fn batch_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_nan_into(prices, out);
        Ok(())
    }
    /// `batchFast` into a caller `Float64Array` exactly as long as the input.
    #[wasm_bindgen(js_name = batchFastInto)]
    pub fn batch_fast_into(&mut self, prices: &[f64], out: &mut [f64]) -> Result<(), JsError> {
        check_rows(out, prices.len())?;
        self.inner.batch_fast_into(prices, out);
        Ok(())
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// --- Two-series (asset, benchmark) indicators ---
//
// Family 12 (PR #51) introduces `wasm_pair_indicator!` for Pearson / Beta /
// Spearman. Family 12 is not in main, so Family 15 writes its three pair
// wrappers by hand here; merge with PR #51 keeps the macro and re-uses it.

#[wasm_bindgen(js_name = TreynorRatio)]
pub struct WasmTreynorRatio {
    inner: wc::TreynorRatio,
}

#[wasm_bindgen(js_class = TreynorRatio)]
impl WasmTreynorRatio {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, risk_free: f64) -> Result<WasmTreynorRatio, JsError> {
        Ok(Self {
            inner: wc::TreynorRatio::new(period, risk_free).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, asset: f64, benchmark: f64) -> Option<f64> {
        self.inner.update((asset, benchmark))
    }
    pub fn batch(&mut self, asset: &[f64], benchmark: &[f64]) -> Result<Float64Array, JsError> {
        if asset.len() != benchmark.len() {
            return Err(JsError::new("asset and benchmark must be equal length"));
        }
        let mut out = Vec::with_capacity(asset.len());
        for i in 0..asset.len() {
            out.push(
                self.inner
                    .update((asset[i], benchmark[i]))
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = InformationRatio)]
pub struct WasmInformationRatio {
    inner: wc::InformationRatio,
}

#[wasm_bindgen(js_class = InformationRatio)]
impl WasmInformationRatio {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmInformationRatio, JsError> {
        Ok(Self {
            inner: wc::InformationRatio::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, asset: f64, benchmark: f64) -> Option<f64> {
        self.inner.update((asset, benchmark))
    }
    pub fn batch(&mut self, asset: &[f64], benchmark: &[f64]) -> Result<Float64Array, JsError> {
        if asset.len() != benchmark.len() {
            return Err(JsError::new("asset and benchmark must be equal length"));
        }
        let mut out = Vec::with_capacity(asset.len());
        for i in 0..asset.len() {
            out.push(
                self.inner
                    .update((asset[i], benchmark[i]))
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Alt-Chart Bars ==============================
//
// Bar builders consume close prices and emit a variable number of completed bars
// per input. `update(close)` returns a JS array of the bars finished on that
// close; `batch` returns all completed bars concatenated.

#[wasm_bindgen(js_name = RenkoBars)]
pub struct WasmRenkoBars {
    inner: wc::RenkoBars,
}

#[wasm_bindgen(js_class = RenkoBars)]
impl WasmRenkoBars {
    #[wasm_bindgen(constructor)]
    pub fn new(box_size: f64) -> Result<WasmRenkoBars, JsError> {
        Ok(Self {
            inner: wc::RenkoBars::new(box_size).map_err(map_err)?,
        })
    }
    /// Returns an array of `{ open, close, direction }` bricks completed on this close.
    pub fn update(&mut self, close: f64) -> Result<WasmRenkoBarsValue, JsError> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        let arr = Array::new();
        for b in self.inner.update(candle) {
            let obj = js_object!("open" => b.open, "close" => b.close, "direction" => f64::from(b.direction));
            arr.push(&obj);
        }
        Ok(arr.unchecked_into())
    }
    pub fn batch(&mut self, close: &[f64]) -> Result<WasmRenkoBarsValue, JsError> {
        let arr = Array::new();
        for &price in close {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                let obj = js_object!("open" => b.open, "close" => b.close, "direction" => f64::from(b.direction));
                arr.push(&obj);
            }
        }
        Ok(arr.unchecked_into())
    }
    #[wasm_bindgen(js_name = boxSize)]
    pub fn box_size(&self) -> f64 {
        self.inner.box_size()
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[wasm_bindgen(js_name = KagiBars)]
pub struct WasmKagiBars {
    inner: wc::KagiBars,
}

#[wasm_bindgen(js_class = KagiBars)]
impl WasmKagiBars {
    #[wasm_bindgen(constructor)]
    pub fn new(reversal: f64) -> Result<WasmKagiBars, JsError> {
        Ok(Self {
            inner: wc::KagiBars::new(reversal).map_err(map_err)?,
        })
    }
    /// Returns an array of `{ start, end, direction }` segments completed on this close.
    pub fn update(&mut self, close: f64) -> Result<WasmKagiBarsValue, JsError> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        let arr = Array::new();
        for b in self.inner.update(candle) {
            let obj = js_object!("start" => b.start, "end" => b.end, "direction" => f64::from(b.direction));
            arr.push(&obj);
        }
        Ok(arr.unchecked_into())
    }
    pub fn batch(&mut self, close: &[f64]) -> Result<WasmKagiBarsValue, JsError> {
        let arr = Array::new();
        for &price in close {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                let obj = js_object!("start" => b.start, "end" => b.end, "direction" => f64::from(b.direction));
                arr.push(&obj);
            }
        }
        Ok(arr.unchecked_into())
    }
    pub fn reversal(&self) -> f64 {
        self.inner.reversal()
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[wasm_bindgen(js_name = PointAndFigureBars)]
pub struct WasmPointAndFigureBars {
    inner: wc::PointAndFigureBars,
}

#[wasm_bindgen(js_class = PointAndFigureBars)]
impl WasmPointAndFigureBars {
    #[wasm_bindgen(constructor)]
    pub fn new(box_size: f64, reversal: usize) -> Result<WasmPointAndFigureBars, JsError> {
        Ok(Self {
            inner: wc::PointAndFigureBars::new(box_size, reversal).map_err(map_err)?,
        })
    }
    /// Returns an array of `{ direction, high, low }` columns completed on this close.
    pub fn update(&mut self, close: f64) -> Result<WasmPointAndFigureBarsValue, JsError> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        let arr = Array::new();
        for col in self.inner.update(candle) {
            let obj = js_object!("direction" => f64::from(col.direction), "high" => col.high, "low" => col.low);
            arr.push(&obj);
        }
        Ok(arr.unchecked_into())
    }
    pub fn batch(&mut self, close: &[f64]) -> Result<WasmPointAndFigureBarsValue, JsError> {
        let arr = Array::new();
        for &price in close {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for col in self.inner.update(candle) {
                let obj = js_object!("direction" => f64::from(col.direction), "high" => col.high, "low" => col.low);
                arr.push(&obj);
            }
        }
        Ok(arr.unchecked_into())
    }
    #[wasm_bindgen(js_name = boxSize)]
    pub fn box_size(&self) -> f64 {
        self.inner.box_size()
    }
    pub fn reversal(&self) -> usize {
        self.inner.reversal()
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[wasm_bindgen(js_name = RangeBars)]
pub struct WasmRangeBars {
    inner: wc::RangeBars,
}

#[wasm_bindgen(js_class = RangeBars)]
impl WasmRangeBars {
    #[wasm_bindgen(constructor)]
    pub fn new(range: f64) -> Result<WasmRangeBars, JsError> {
        Ok(Self {
            inner: wc::RangeBars::new(range).map_err(map_err)?,
        })
    }
    /// Returns an array of `{ open, close, direction }` bars completed on this close.
    pub fn update(&mut self, close: f64) -> Result<WasmRangeBarsValue, JsError> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        let arr = Array::new();
        for b in self.inner.update(candle) {
            let obj = js_object!("open" => b.open, "close" => b.close, "direction" => f64::from(b.direction));
            arr.push(&obj);
        }
        Ok(arr.unchecked_into())
    }
    pub fn batch(&mut self, close: &[f64]) -> Result<WasmRangeBarsValue, JsError> {
        let arr = Array::new();
        for &price in close {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                let obj = js_object!("open" => b.open, "close" => b.close, "direction" => f64::from(b.direction));
                arr.push(&obj);
            }
        }
        Ok(arr.unchecked_into())
    }
    pub fn range(&self) -> f64 {
        self.inner.range()
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[wasm_bindgen(js_name = TickBars)]
pub struct WasmTickBars {
    inner: wc::TickBars,
}

#[wasm_bindgen(js_class = TickBars)]
impl WasmTickBars {
    #[wasm_bindgen(constructor)]
    pub fn new(ticks: usize) -> Result<WasmTickBars, JsError> {
        Ok(Self {
            inner: wc::TickBars::new(ticks).map_err(map_err)?,
        })
    }
    /// Returns an array of `{ open, high, low, close, volume }` bars completed on this candle.
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<WasmTickBarsValue, JsError> {
        let candle = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        let arr = Array::new();
        for b in self.inner.update(candle) {
            let obj = js_object!("open" => b.open, "high" => b.high, "low" => b.low, "close" => b.close, "volume" => b.volume);
            arr.push(&obj);
        }
        Ok(arr.unchecked_into())
    }
    pub fn ticks(&self) -> usize {
        self.inner.ticks()
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Batch over the same inputs as `update`, concatenating the bars each
    /// candle completed. The output length is data-dependent, not `n`.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<WasmTickBarsValue, JsError> {
        if high.len() != open.len()
            || low.len() != open.len()
            || close.len() != open.len()
            || volume.len() != open.len()
        {
            return Err(JsError::new(
                "open, high, low, close, volume must be equal length",
            ));
        }
        let out = Array::new();
        for i in 0..open.len() {
            for bar in self
                .update(open[i], high[i], low[i], close[i], volume[i])?
                .unchecked_into::<Array>()
                .iter()
            {
                out.push(&bar);
            }
        }
        Ok(out.unchecked_into())
    }
}

#[wasm_bindgen(js_name = VolumeBars)]
pub struct WasmVolumeBars {
    inner: wc::VolumeBars,
}

#[wasm_bindgen(js_class = VolumeBars)]
impl WasmVolumeBars {
    #[wasm_bindgen(constructor)]
    pub fn new(volume_per_bar: f64) -> Result<WasmVolumeBars, JsError> {
        Ok(Self {
            inner: wc::VolumeBars::new(volume_per_bar).map_err(map_err)?,
        })
    }
    /// Returns an array of `{ open, high, low, close, volume }` bars completed on this candle.
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<WasmVolumeBarsValue, JsError> {
        let candle = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        let arr = Array::new();
        for b in self.inner.update(candle) {
            let obj = js_object!("open" => b.open, "high" => b.high, "low" => b.low, "close" => b.close, "volume" => b.volume);
            arr.push(&obj);
        }
        Ok(arr.unchecked_into())
    }
    #[wasm_bindgen(js_name = volumePerBar)]
    pub fn volume_per_bar(&self) -> f64 {
        self.inner.volume_per_bar()
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Batch over the same inputs as `update`, concatenating the bars each
    /// candle completed. The output length is data-dependent, not `n`.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<WasmVolumeBarsValue, JsError> {
        if high.len() != open.len()
            || low.len() != open.len()
            || close.len() != open.len()
            || volume.len() != open.len()
        {
            return Err(JsError::new(
                "open, high, low, close, volume must be equal length",
            ));
        }
        let out = Array::new();
        for i in 0..open.len() {
            for bar in self
                .update(open[i], high[i], low[i], close[i], volume[i])?
                .unchecked_into::<Array>()
                .iter()
            {
                out.push(&bar);
            }
        }
        Ok(out.unchecked_into())
    }
}

#[wasm_bindgen(js_name = DollarBars)]
pub struct WasmDollarBars {
    inner: wc::DollarBars,
}

#[wasm_bindgen(js_class = DollarBars)]
impl WasmDollarBars {
    #[wasm_bindgen(constructor)]
    pub fn new(dollar_per_bar: f64) -> Result<WasmDollarBars, JsError> {
        Ok(Self {
            inner: wc::DollarBars::new(dollar_per_bar).map_err(map_err)?,
        })
    }
    /// Returns an array of `{ open, high, low, close, volume, dollar }` bars completed on this candle.
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<WasmDollarBarsValue, JsError> {
        let candle = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        let arr = Array::new();
        for b in self.inner.update(candle) {
            let obj = js_object!("open" => b.open, "high" => b.high, "low" => b.low, "close" => b.close, "volume" => b.volume, "dollar" => b.dollar);
            arr.push(&obj);
        }
        Ok(arr.unchecked_into())
    }
    #[wasm_bindgen(js_name = dollarPerBar)]
    pub fn dollar_per_bar(&self) -> f64 {
        self.inner.dollar_per_bar()
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Batch over the same inputs as `update`, concatenating the bars each
    /// candle completed. The output length is data-dependent, not `n`.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<WasmDollarBarsValue, JsError> {
        if high.len() != open.len()
            || low.len() != open.len()
            || close.len() != open.len()
            || volume.len() != open.len()
        {
            return Err(JsError::new(
                "open, high, low, close, volume must be equal length",
            ));
        }
        let out = Array::new();
        for i in 0..open.len() {
            for bar in self
                .update(open[i], high[i], low[i], close[i], volume[i])?
                .unchecked_into::<Array>()
                .iter()
            {
                out.push(&bar);
            }
        }
        Ok(out.unchecked_into())
    }
}

#[wasm_bindgen(js_name = ImbalanceBars)]
pub struct WasmImbalanceBars {
    inner: wc::ImbalanceBars,
}

#[wasm_bindgen(js_class = ImbalanceBars)]
impl WasmImbalanceBars {
    #[wasm_bindgen(constructor)]
    pub fn new(threshold: f64) -> Result<WasmImbalanceBars, JsError> {
        Ok(Self {
            inner: wc::ImbalanceBars::new(threshold).map_err(map_err)?,
        })
    }
    /// Returns an array of `{ open, high, low, close, imbalance, direction }` bars completed on this candle.
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<WasmImbalanceBarsValue, JsError> {
        let candle = wc::Candle::new(open, high, low, close, 1.0, 0).map_err(map_err)?;
        let arr = Array::new();
        for b in self.inner.update(candle) {
            let obj = js_object!("open" => b.open, "high" => b.high, "low" => b.low, "close" => b.close, "imbalance" => b.imbalance, "direction" => f64::from(b.direction));
            arr.push(&obj);
        }
        Ok(arr.unchecked_into())
    }
    pub fn threshold(&self) -> f64 {
        self.inner.threshold()
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Batch over the same inputs as `update`, concatenating the bars each
    /// candle completed. The output length is data-dependent, not `n`.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<WasmImbalanceBarsValue, JsError> {
        if high.len() != open.len() || low.len() != open.len() || close.len() != open.len() {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let out = Array::new();
        for i in 0..open.len() {
            for bar in self
                .update(open[i], high[i], low[i], close[i])?
                .unchecked_into::<Array>()
                .iter()
            {
                out.push(&bar);
            }
        }
        Ok(out.unchecked_into())
    }
}

#[wasm_bindgen(js_name = RunBars)]
pub struct WasmRunBars {
    inner: wc::RunBars,
}

#[wasm_bindgen(js_class = RunBars)]
impl WasmRunBars {
    #[wasm_bindgen(constructor)]
    pub fn new(run_length: usize) -> Result<WasmRunBars, JsError> {
        Ok(Self {
            inner: wc::RunBars::new(run_length).map_err(map_err)?,
        })
    }
    /// Returns an array of `{ open, high, low, close, length, direction }` bars completed on this candle.
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Result<WasmRunBarsValue, JsError> {
        let candle = wc::Candle::new(open, high, low, close, 1.0, 0).map_err(map_err)?;
        let arr = Array::new();
        for b in self.inner.update(candle) {
            let obj =
                js_object!("open" => b.open, "high" => b.high, "low" => b.low, "close" => b.close);
            #[allow(clippy::cast_precision_loss)]
            Reflect::set(&obj, &key("length"), &(b.length as f64).into()).ok();
            Reflect::set(&obj, &key("direction"), &f64::from(b.direction).into()).ok();
            arr.push(&obj);
        }
        Ok(arr.unchecked_into())
    }
    #[wasm_bindgen(js_name = runLength)]
    pub fn run_length(&self) -> usize {
        self.inner.run_length()
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    /// Batch over the same inputs as `update`, concatenating the bars each
    /// candle completed. The output length is data-dependent, not `n`.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<WasmRunBarsValue, JsError> {
        if high.len() != open.len() || low.len() != open.len() || close.len() != open.len() {
            return Err(JsError::new("open, high, low, close must be equal length"));
        }
        let out = Array::new();
        for i in 0..open.len() {
            for bar in self
                .update(open[i], high[i], low[i], close[i])?
                .unchecked_into::<Array>()
                .iter()
            {
                out.push(&bar);
            }
        }
        Ok(out.unchecked_into())
    }
}

#[wasm_bindgen(js_name = ThreeLineBreakBars)]
pub struct WasmThreeLineBreakBars {
    inner: wc::ThreeLineBreakBars,
}

#[wasm_bindgen(js_class = ThreeLineBreakBars)]
impl WasmThreeLineBreakBars {
    #[wasm_bindgen(constructor)]
    pub fn new(lines: usize) -> Result<WasmThreeLineBreakBars, JsError> {
        Ok(Self {
            inner: wc::ThreeLineBreakBars::new(lines).map_err(map_err)?,
        })
    }
    /// Returns an array of `{ open, close, direction }` bars completed on this close.
    pub fn update(&mut self, close: f64) -> Result<WasmThreeLineBreakBarsValue, JsError> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        let arr = Array::new();
        for b in self.inner.update(candle) {
            let obj = js_object!("open" => b.open, "close" => b.close, "direction" => f64::from(b.direction));
            arr.push(&obj);
        }
        Ok(arr.unchecked_into())
    }
    pub fn batch(&mut self, close: &[f64]) -> Result<WasmThreeLineBreakBarsValue, JsError> {
        let arr = Array::new();
        for &price in close {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                let obj = js_object!("open" => b.open, "close" => b.close, "direction" => f64::from(b.direction));
                arr.push(&obj);
            }
        }
        Ok(arr.unchecked_into())
    }
    pub fn lines(&self) -> usize {
        self.inner.lines()
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
}

#[wasm_bindgen(js_name = Alpha)]
pub struct WasmAlpha {
    inner: wc::Alpha,
}

#[wasm_bindgen(js_class = Alpha)]
impl WasmAlpha {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, risk_free: f64) -> Result<WasmAlpha, JsError> {
        Ok(Self {
            inner: wc::Alpha::new(period, risk_free).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, asset: f64, benchmark: f64) -> Option<f64> {
        self.inner.update((asset, benchmark))
    }
    pub fn batch(&mut self, asset: &[f64], benchmark: &[f64]) -> Result<Float64Array, JsError> {
        if asset.len() != benchmark.len() {
            return Err(JsError::new("asset and benchmark must be equal length"));
        }
        let mut out = Vec::with_capacity(asset.len());
        for i in 0..asset.len() {
            out.push(
                self.inner
                    .update((asset[i], benchmark[i]))
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ====================== Seasonality & Session (full-candle) ======================
//
// These read the wall-clock fields of `Candle::timestamp`. JS passes `timestamp`
// as a BigInt (epoch milliseconds). Following the multi-input precedent
// (microstructure / derivatives), WASM exposes streaming `update` only — no
// batch over ragged multi-arrays.

macro_rules! wasm_seasonality_offset_scalar {
    ($wrapper:ident, $js:ident, $rust:ty) => {
        #[wasm_bindgen(js_name = $js)]
        pub struct $wrapper {
            inner: $rust,
        }
        #[wasm_bindgen(js_class = $js)]
        impl $wrapper {
            #[wasm_bindgen(constructor)]
            pub fn new(utc_offset_minutes: i32) -> $wrapper {
                Self {
                    inner: <$rust>::new(utc_offset_minutes),
                }
            }
            pub fn update(
                &mut self,
                open: f64,
                high: f64,
                low: f64,
                close: f64,
                volume: f64,
                timestamp: i64,
            ) -> Result<Option<f64>, JsError> {
                Ok(self.inner.update(
                    wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?,
                ))
            }
            /// Batch over the same inputs as `update`, one element per bar.
            /// Warmup positions come back as `NaN`, so the output length
            /// matches the input.
            pub fn batch(
                &mut self,
                open: &[f64],
                high: &[f64],
                low: &[f64],
                close: &[f64],
                volume: &[f64],
                timestamp: &[i64],
            ) -> Result<Float64Array, JsError> {
                if high.len() != open.len()
                    || low.len() != open.len()
                    || close.len() != open.len()
                    || volume.len() != open.len()
                    || timestamp.len() != open.len()
                {
                    return Err(JsError::new(
                        "open, high, low, close, volume, timestamp must be equal length",
                    ));
                }
                let mut out = Vec::with_capacity(open.len());
                for i in 0..open.len() {
                    out.push(
                        self.update(open[i], high[i], low[i], close[i], volume[i], timestamp[i])?
                            .unwrap_or(f64::NAN),
                    );
                }
                Ok(Float64Array::from(out.as_slice()))
            }

            pub fn reset(&mut self) {
                self.inner.reset();
            }

            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[wasm_bindgen(js_name = isReady)]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[wasm_bindgen(js_name = warmupPeriod)]
            pub fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
            #[wasm_bindgen(js_name = utcOffsetMinutes)]
            pub fn utc_offset_minutes(&self) -> i32 {
                self.inner.utc_offset_minutes()
            }
        }
    };
}

macro_rules! wasm_seasonality_bucket_profile {
    ($wrapper:ident, $js:ident, $rust:ty) => {
        #[wasm_bindgen(js_name = $js)]
        pub struct $wrapper {
            inner: $rust,
        }
        #[wasm_bindgen(js_class = $js)]
        impl $wrapper {
            #[wasm_bindgen(constructor)]
            pub fn new(buckets: usize, utc_offset_minutes: i32) -> Result<$wrapper, JsError> {
                Ok(Self {
                    inner: <$rust>::new(buckets, utc_offset_minutes).map_err(map_err)?,
                })
            }
            pub fn update(
                &mut self,
                open: f64,
                high: f64,
                low: f64,
                close: f64,
                volume: f64,
                timestamp: i64,
            ) -> Result<Option<Float64Array>, JsError> {
                let c =
                    wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?;
                Ok(self
                    .inner
                    .update(c)
                    .map(|o| Float64Array::from(o.bins.as_slice())))
            }
            /// Batch over the same inputs as `update`. Returns one entry per
            /// bar: the bucket profile as a `Float64Array`, or `undefined` while
            /// warming up. The bucket count is fixed by the constructor, so a
            /// flat array would carry no more information.
            pub fn batch(
                &mut self,
                open: &[f64],
                high: &[f64],
                low: &[f64],
                close: &[f64],
                volume: &[f64],
                timestamp: &[i64],
            ) -> Result<WasmProfileBatchValue, JsError> {
                if high.len() != open.len()
                    || low.len() != open.len()
                    || close.len() != open.len()
                    || volume.len() != open.len()
                    || timestamp.len() != open.len()
                {
                    return Err(JsError::new(
                        "open, high, low, close, volume, timestamp must be equal length",
                    ));
                }
                let out = Array::new();
                for i in 0..open.len() {
                    // `null` for a warmup bar; the array carries one entry per bar either way.
                    let bins =
                        self.update(open[i], high[i], low[i], close[i], volume[i], timestamp[i])?;
                    out.push(&bins.map_or(JsValue::UNDEFINED, JsValue::from));
                }
                Ok(out.unchecked_into())
            }

            pub fn reset(&mut self) {
                self.inner.reset();
            }

            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[wasm_bindgen(js_name = isReady)]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[wasm_bindgen(js_name = warmupPeriod)]
            pub fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
            #[wasm_bindgen(js_name = utcOffsetMinutes)]
            pub fn utc_offset_minutes(&self) -> i32 {
                self.inner.params().1
            }
        }
    };
}

macro_rules! wasm_seasonality_offset_profile {
    ($wrapper:ident, $js:ident, $rust:ty) => {
        #[wasm_bindgen(js_name = $js)]
        pub struct $wrapper {
            inner: $rust,
        }
        #[wasm_bindgen(js_class = $js)]
        impl $wrapper {
            #[wasm_bindgen(constructor)]
            pub fn new(utc_offset_minutes: i32) -> $wrapper {
                Self {
                    inner: <$rust>::new(utc_offset_minutes),
                }
            }
            pub fn update(
                &mut self,
                open: f64,
                high: f64,
                low: f64,
                close: f64,
                volume: f64,
                timestamp: i64,
            ) -> Result<Option<Float64Array>, JsError> {
                let c =
                    wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?;
                Ok(self
                    .inner
                    .update(c)
                    .map(|o| Float64Array::from(o.bins.as_slice())))
            }
            /// Batch over the same inputs as `update`. Returns one entry per
            /// bar: the bucket profile as a `Float64Array`, or `undefined` while
            /// warming up. The bucket count is fixed by the constructor, so a
            /// flat array would carry no more information.
            pub fn batch(
                &mut self,
                open: &[f64],
                high: &[f64],
                low: &[f64],
                close: &[f64],
                volume: &[f64],
                timestamp: &[i64],
            ) -> Result<WasmProfileBatchValue, JsError> {
                if high.len() != open.len()
                    || low.len() != open.len()
                    || close.len() != open.len()
                    || volume.len() != open.len()
                    || timestamp.len() != open.len()
                {
                    return Err(JsError::new(
                        "open, high, low, close, volume, timestamp must be equal length",
                    ));
                }
                let out = Array::new();
                for i in 0..open.len() {
                    // `null` for a warmup bar; the array carries one entry per bar either way.
                    let bins =
                        self.update(open[i], high[i], low[i], close[i], volume[i], timestamp[i])?;
                    out.push(&bins.map_or(JsValue::UNDEFINED, JsValue::from));
                }
                Ok(out.unchecked_into())
            }

            pub fn reset(&mut self) {
                self.inner.reset();
            }

            pub fn name(&self) -> String {
                self.inner.name().to_string()
            }
            #[wasm_bindgen(js_name = isReady)]
            pub fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            #[wasm_bindgen(js_name = warmupPeriod)]
            pub fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
            #[wasm_bindgen(js_name = utcOffsetMinutes)]
            pub fn utc_offset_minutes(&self) -> i32 {
                self.inner.utc_offset_minutes()
            }
        }
    };
}

wasm_seasonality_offset_scalar!(WasmSessionVwap, SessionVwap, wc::SessionVwap);
wasm_seasonality_offset_scalar!(WasmOvernightGap, OvernightGap, wc::OvernightGap);
wasm_seasonality_offset_scalar!(WasmSeasonalZScore, SeasonalZScore, wc::SeasonalZScore);
wasm_seasonality_bucket_profile!(
    WasmTimeOfDayReturnProfile,
    TimeOfDayReturnProfile,
    wc::TimeOfDayReturnProfile
);
wasm_seasonality_bucket_profile!(
    WasmIntradayVolatilityProfile,
    IntradayVolatilityProfile,
    wc::IntradayVolatilityProfile
);
wasm_seasonality_bucket_profile!(
    WasmVolumeByTimeProfile,
    VolumeByTimeProfile,
    wc::VolumeByTimeProfile
);
wasm_seasonality_offset_profile!(WasmDayOfWeekProfile, DayOfWeekProfile, wc::DayOfWeekProfile);

#[wasm_bindgen(js_name = AverageDailyRange)]
pub struct WasmAverageDailyRange {
    inner: wc::AverageDailyRange,
}
#[wasm_bindgen(js_class = AverageDailyRange)]
impl WasmAverageDailyRange {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize, utc_offset_minutes: i32) -> Result<WasmAverageDailyRange, JsError> {
        Ok(Self {
            inner: wc::AverageDailyRange::new(period, utc_offset_minutes).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        timestamp: &[i64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != open.len()
            || low.len() != open.len()
            || close.len() != open.len()
            || volume.len() != open.len()
            || timestamp.len() != open.len()
        {
            return Err(JsError::new(
                "open, high, low, close, volume, timestamp must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            out.push(
                self.update(open[i], high[i], low[i], close[i], volume[i], timestamp[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = TurnOfMonth)]
pub struct WasmTurnOfMonth {
    inner: wc::TurnOfMonth,
}
#[wasm_bindgen(js_class = TurnOfMonth)]
impl WasmTurnOfMonth {
    #[wasm_bindgen(constructor)]
    pub fn new(
        n_first: u32,
        n_last: u32,
        utc_offset_minutes: i32,
    ) -> Result<WasmTurnOfMonth, JsError> {
        Ok(Self {
            inner: wc::TurnOfMonth::new(n_first, n_last, utc_offset_minutes).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> Result<Option<f64>, JsError> {
        Ok(self
            .inner
            .update(wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`, one element per bar.
    /// Warmup positions come back as `NaN`, so the output length matches
    /// the input.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        timestamp: &[i64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != open.len()
            || low.len() != open.len()
            || close.len() != open.len()
            || volume.len() != open.len()
            || timestamp.len() != open.len()
        {
            return Err(JsError::new(
                "open, high, low, close, volume, timestamp must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(open.len());
        for i in 0..open.len() {
            out.push(
                self.update(open[i], high[i], low[i], close[i], volume[i], timestamp[i])?
                    .unwrap_or(f64::NAN),
            );
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = SessionHighLow)]
pub struct WasmSessionHighLow {
    inner: wc::SessionHighLow,
}
#[wasm_bindgen(js_class = SessionHighLow)]
impl WasmSessionHighLow {
    #[wasm_bindgen(constructor)]
    pub fn new(utc_offset_minutes: i32) -> WasmSessionHighLow {
        Self {
            inner: wc::SessionHighLow::new(utc_offset_minutes),
        }
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> Result<Option<WasmSessionHighLowValue>, JsError> {
        let c = wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("high" => o.high, "low" => o.low);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`. Returns a flat array of
    /// `n * 2` values, `[high, low]` per bar, `NaN` while warming up.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        timestamp: &[i64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != open.len()
            || low.len() != open.len()
            || close.len() != open.len()
            || volume.len() != open.len()
            || timestamp.len() != open.len()
        {
            return Err(JsError::new(
                "open, high, low, close, volume, timestamp must be equal length",
            ));
        }
        let mut out = vec![f64::NAN; open.len() * 2];
        for i in 0..open.len() {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], volume[i], timestamp[i])
                .map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.high;
                out[i * 2 + 1] = o.low;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = SessionRange)]
pub struct WasmSessionRange {
    inner: wc::SessionRange,
}
#[wasm_bindgen(js_class = SessionRange)]
impl WasmSessionRange {
    #[wasm_bindgen(constructor)]
    pub fn new(utc_offset_minutes: i32) -> WasmSessionRange {
        Self {
            inner: wc::SessionRange::new(utc_offset_minutes),
        }
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> Result<Option<WasmSessionRangeValue>, JsError> {
        let c = wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("asia" => o.asia, "eu" => o.eu, "us" => o.us);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`. Returns a flat array of
    /// `n * 3` values, `[asia, eu, us]` per bar, `NaN` while warming up.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        timestamp: &[i64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != open.len()
            || low.len() != open.len()
            || close.len() != open.len()
            || volume.len() != open.len()
            || timestamp.len() != open.len()
        {
            return Err(JsError::new(
                "open, high, low, close, volume, timestamp must be equal length",
            ));
        }
        let mut out = vec![f64::NAN; open.len() * 3];
        for i in 0..open.len() {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], volume[i], timestamp[i])
                .map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.asia;
                out[i * 3 + 1] = o.eu;
                out[i * 3 + 2] = o.us;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

#[wasm_bindgen(js_name = OvernightIntradayReturn)]
pub struct WasmOvernightIntradayReturn {
    inner: wc::OvernightIntradayReturn,
}
#[wasm_bindgen(js_class = OvernightIntradayReturn)]
impl WasmOvernightIntradayReturn {
    #[wasm_bindgen(constructor)]
    pub fn new(utc_offset_minutes: i32) -> WasmOvernightIntradayReturn {
        Self {
            inner: wc::OvernightIntradayReturn::new(utc_offset_minutes),
        }
    }
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> Result<Option<WasmOvernightIntradayReturnValue>, JsError> {
        let c = wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("overnight" => o.overnight, "intraday" => o.intraday);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    /// Batch over the same inputs as `update`. Returns a flat array of
    /// `n * 2` values, `[overnight, intraday]` per bar, `NaN` while warming up.
    pub fn batch(
        &mut self,
        open: &[f64],
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
        timestamp: &[i64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != open.len()
            || low.len() != open.len()
            || close.len() != open.len()
            || volume.len() != open.len()
            || timestamp.len() != open.len()
        {
            return Err(JsError::new(
                "open, high, low, close, volume, timestamp must be equal length",
            ));
        }
        let mut out = vec![f64::NAN; open.len() * 2];
        for i in 0..open.len() {
            let c = wc::Candle::new(open[i], high[i], low[i], close[i], volume[i], timestamp[i])
                .map_err(map_err)?;
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.overnight;
                out[i * 2 + 1] = o.intraday;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
}

// ============================== Fibonacci ==============================

/// Candle for the swing-based Fibonacci tools: only high/low drive the tracker,
/// so open/close are pinned to the midpoint.
fn swing_make_candle(high: f64, low: f64) -> Result<wc::Candle, JsError> {
    make_candle(high, low, f64::midpoint(high, low), 0.0)
}

#[wasm_bindgen(js_name = FibRetracement)]
pub struct WasmFibRetracement {
    inner: wc::FibRetracement,
}

impl Default for WasmFibRetracement {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = FibRetracement)]
impl WasmFibRetracement {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmFibRetracement {
        Self {
            inner: wc::FibRetracement::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmFibRetracementValue>, JsError> {
        let c = swing_make_candle(high, low)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("level0" => o.level_0, "level236" => o.level_236, "level382" => o.level_382, "level500" => o.level_500, "level618" => o.level_618, "level786" => o.level_786, "level1000" => o.level_1000);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 7];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_make_candle(high[i], low[i])?) {
                out[i * 7] = o.level_0;
                out[i * 7 + 1] = o.level_236;
                out[i * 7 + 2] = o.level_382;
                out[i * 7 + 3] = o.level_500;
                out[i * 7 + 4] = o.level_618;
                out[i * 7 + 5] = o.level_786;
                out[i * 7 + 6] = o.level_1000;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = FibExtension)]
pub struct WasmFibExtension {
    inner: wc::FibExtension,
}

impl Default for WasmFibExtension {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = FibExtension)]
impl WasmFibExtension {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmFibExtension {
        Self {
            inner: wc::FibExtension::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmFibExtensionValue>, JsError> {
        let c = swing_make_candle(high, low)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("level1272" => o.level_1272, "level1414" => o.level_1414, "level1618" => o.level_1618, "level2000" => o.level_2000, "level2618" => o.level_2618);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 5];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_make_candle(high[i], low[i])?) {
                out[i * 5] = o.level_1272;
                out[i * 5 + 1] = o.level_1414;
                out[i * 5 + 2] = o.level_1618;
                out[i * 5 + 3] = o.level_2000;
                out[i * 5 + 4] = o.level_2618;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = FibProjection)]
pub struct WasmFibProjection {
    inner: wc::FibProjection,
}

impl Default for WasmFibProjection {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = FibProjection)]
impl WasmFibProjection {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmFibProjection {
        Self {
            inner: wc::FibProjection::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmFibProjectionValue>, JsError> {
        let c = swing_make_candle(high, low)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("level618" => o.level_618, "level1000" => o.level_1000, "level1618" => o.level_1618, "level2618" => o.level_2618);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_make_candle(high[i], low[i])?) {
                out[i * 4] = o.level_618;
                out[i * 4 + 1] = o.level_1000;
                out[i * 4 + 2] = o.level_1618;
                out[i * 4 + 3] = o.level_2618;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = AutoFib)]
pub struct WasmAutoFib {
    inner: wc::AutoFib,
}

impl Default for WasmAutoFib {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = AutoFib)]
impl WasmAutoFib {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmAutoFib {
        Self {
            inner: wc::AutoFib::new(),
        }
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<WasmAutoFibValue>, JsError> {
        let c = swing_make_candle(high, low)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("level0" => o.level_0, "level236" => o.level_236, "level382" => o.level_382, "level500" => o.level_500, "level618" => o.level_618, "level786" => o.level_786, "level1000" => o.level_1000);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 7];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_make_candle(high[i], low[i])?) {
                out[i * 7] = o.level_0;
                out[i * 7 + 1] = o.level_236;
                out[i * 7 + 2] = o.level_382;
                out[i * 7 + 3] = o.level_500;
                out[i * 7 + 4] = o.level_618;
                out[i * 7 + 5] = o.level_786;
                out[i * 7 + 6] = o.level_1000;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = GoldenPocket)]
pub struct WasmGoldenPocket {
    inner: wc::GoldenPocket,
}

impl Default for WasmGoldenPocket {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = GoldenPocket)]
impl WasmGoldenPocket {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmGoldenPocket {
        Self {
            inner: wc::GoldenPocket::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmGoldenPocketValue>, JsError> {
        let c = swing_make_candle(high, low)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("low" => o.low, "mid" => o.mid, "high" => o.high);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_make_candle(high[i], low[i])?) {
                out[i * 3] = o.low;
                out[i * 3 + 1] = o.mid;
                out[i * 3 + 2] = o.high;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = FibConfluence)]
pub struct WasmFibConfluence {
    inner: wc::FibConfluence,
}

impl Default for WasmFibConfluence {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = FibConfluence)]
impl WasmFibConfluence {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmFibConfluence {
        Self {
            inner: wc::FibConfluence::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmFibConfluenceValue>, JsError> {
        let c = swing_make_candle(high, low)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("price" => o.price, "strength" => o.strength);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_make_candle(high[i], low[i])?) {
                out[i * 2] = o.price;
                out[i * 2 + 1] = o.strength;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = FibFan)]
pub struct WasmFibFan {
    inner: wc::FibFan,
}

impl Default for WasmFibFan {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = FibFan)]
impl WasmFibFan {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmFibFan {
        Self {
            inner: wc::FibFan::new(),
        }
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<WasmFibFanValue>, JsError> {
        let c = swing_make_candle(high, low)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj =
                    js_object!("fan382" => o.fan_382, "fan500" => o.fan_500, "fan618" => o.fan_618);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_make_candle(high[i], low[i])?) {
                out[i * 3] = o.fan_382;
                out[i * 3 + 1] = o.fan_500;
                out[i * 3 + 2] = o.fan_618;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = FibArcs)]
pub struct WasmFibArcs {
    inner: wc::FibArcs,
}

impl Default for WasmFibArcs {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = FibArcs)]
impl WasmFibArcs {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmFibArcs {
        Self {
            inner: wc::FibArcs::new(),
        }
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<WasmFibArcsValue>, JsError> {
        let c = swing_make_candle(high, low)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj =
                    js_object!("arc382" => o.arc_382, "arc500" => o.arc_500, "arc618" => o.arc_618);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_make_candle(high[i], low[i])?) {
                out[i * 3] = o.arc_382;
                out[i * 3 + 1] = o.arc_500;
                out[i * 3 + 2] = o.arc_618;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = FibChannel)]
pub struct WasmFibChannel {
    inner: wc::FibChannel,
}

impl Default for WasmFibChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = FibChannel)]
impl WasmFibChannel {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmFibChannel {
        Self {
            inner: wc::FibChannel::new(),
        }
    }
    pub fn update(&mut self, high: f64, low: f64) -> Result<Option<WasmFibChannelValue>, JsError> {
        let c = swing_make_candle(high, low)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("base" => o.base, "level618" => o.level_618, "level1000" => o.level_1000, "level1618" => o.level_1618);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_make_candle(high[i], low[i])?) {
                out[i * 4] = o.base;
                out[i * 4 + 1] = o.level_618;
                out[i * 4 + 2] = o.level_1000;
                out[i * 4 + 3] = o.level_1618;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[wasm_bindgen(js_name = FibTimeZones)]
pub struct WasmFibTimeZones {
    inner: wc::FibTimeZones,
}

impl Default for WasmFibTimeZones {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen(js_class = FibTimeZones)]
impl WasmFibTimeZones {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmFibTimeZones {
        Self {
            inner: wc::FibTimeZones::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
    ) -> Result<Option<WasmFibTimeZonesValue>, JsError> {
        let c = swing_make_candle(high, low)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj = js_object!("onZone" => o.on_zone, "barsToNext" => o.bars_to_next);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    pub fn batch(&mut self, high: &[f64], low: &[f64]) -> Result<Float64Array, JsError> {
        if high.len() != low.len() {
            return Err(JsError::new("high and low must be equal length"));
        }
        let n = high.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update(swing_make_candle(high[i], low[i])?) {
                out[i * 2] = o.on_zone;
                out[i * 2 + 1] = o.bars_to_next;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Volume RSI ==============================

#[wasm_bindgen(js_name = VolumeRsi)]
pub struct WasmVolumeRsi {
    inner: wc::VolumeRsi,
}

#[wasm_bindgen(js_class = VolumeRsi)]
impl WasmVolumeRsi {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmVolumeRsi, JsError> {
        Ok(Self {
            inner: wc::VolumeRsi::new(period).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, close: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(close, close, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, close: &[f64], volume: &[f64]) -> Result<Float64Array, JsError> {
        if close.len() != volume.len() {
            return Err(JsError::new("close and volume must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(close[i], close[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Williams A/D ==============================

#[wasm_bindgen(js_name = Wad)]
pub struct WasmWad {
    inner: wc::Wad,
}

#[wasm_bindgen(js_class = Wad)]
impl WasmWad {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmWad {
        Self {
            inner: wc::Wad::new(),
        }
    }
    pub fn update(&mut self, high: f64, low: f64, close: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, 0.0)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() {
            return Err(JsError::new("high, low, close must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(high[i], low[i], close[i], 0.0)?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Twiggs Money Flow ==============================

#[wasm_bindgen(js_name = TwiggsMoneyFlow)]
pub struct WasmTwiggsMoneyFlow {
    inner: wc::TwiggsMoneyFlow,
}

#[wasm_bindgen(js_class = TwiggsMoneyFlow)]
impl WasmTwiggsMoneyFlow {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmTwiggsMoneyFlow, JsError> {
        Ok(Self {
            inner: wc::TwiggsMoneyFlow::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(high[i], low[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Trade Volume Index ==============================

#[wasm_bindgen(js_name = TradeVolumeIndex)]
pub struct WasmTradeVolumeIndex {
    inner: wc::TradeVolumeIndex,
}

#[wasm_bindgen(js_class = TradeVolumeIndex)]
impl WasmTradeVolumeIndex {
    #[wasm_bindgen(constructor)]
    pub fn new(min_tick: f64) -> Result<WasmTradeVolumeIndex, JsError> {
        Ok(Self {
            inner: wc::TradeVolumeIndex::new(min_tick).map_err(map_err)?,
        })
    }
    pub fn update(&mut self, close: f64, volume: f64) -> Result<Option<f64>, JsError> {
        let c = make_candle(close, close, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(&mut self, close: &[f64], volume: &[f64]) -> Result<Float64Array, JsError> {
        if close.len() != volume.len() {
            return Err(JsError::new("close and volume must be equal length"));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(close[i], close[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Intraday Intensity ==============================

#[wasm_bindgen(js_name = IntradayIntensity)]
pub struct WasmIntradayIntensity {
    inner: wc::IntradayIntensity,
}

#[wasm_bindgen(js_class = IntradayIntensity)]
impl WasmIntradayIntensity {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmIntradayIntensity {
        Self {
            inner: wc::IntradayIntensity::new(),
        }
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(high[i], low[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Better Volume ==============================

#[wasm_bindgen(js_name = BetterVolume)]
pub struct WasmBetterVolume {
    inner: wc::BetterVolume,
}

#[wasm_bindgen(js_class = BetterVolume)]
impl WasmBetterVolume {
    #[wasm_bindgen(constructor)]
    pub fn new(period: usize) -> Result<WasmBetterVolume, JsError> {
        Ok(Self {
            inner: wc::BetterVolume::new(period).map_err(map_err)?,
        })
    }
    pub fn update(
        &mut self,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Option<f64>, JsError> {
        let c = make_candle(high, low, close, volume)?;
        Ok(self.inner.update(c))
    }
    pub fn batch(
        &mut self,
        high: &[f64],
        low: &[f64],
        close: &[f64],
        volume: &[f64],
    ) -> Result<Float64Array, JsError> {
        if high.len() != low.len() || low.len() != close.len() || close.len() != volume.len() {
            return Err(JsError::new(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(close.len());
        for i in 0..close.len() {
            let c = make_candle(high[i], low[i], close[i], volume[i])?;
            out.push(self.inner.update(c).unwrap_or(f64::NAN));
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Volume-Weighted MACD ==============================

#[wasm_bindgen(js_name = VolumeWeightedMacd)]
pub struct WasmVolumeWeightedMacd {
    inner: wc::VolumeWeightedMacd,
}

#[wasm_bindgen(js_class = VolumeWeightedMacd)]
impl WasmVolumeWeightedMacd {
    #[wasm_bindgen(constructor)]
    pub fn new(fast: usize, slow: usize, signal: usize) -> Result<WasmVolumeWeightedMacd, JsError> {
        Ok(Self {
            inner: wc::VolumeWeightedMacd::new(fast, slow, signal).map_err(map_err)?,
        })
    }
    /// Returns `{ macd, signal, histogram }` once warm, else `undefined`.
    pub fn update(
        &mut self,
        close: f64,
        volume: f64,
    ) -> Result<Option<WasmVolumeWeightedMacdValue>, JsError> {
        let c = make_candle(close, close, close, volume)?;
        Ok(match self.inner.update(c) {
            Some(o) => {
                let obj =
                    js_object!("macd" => o.macd, "signal" => o.signal, "histogram" => o.histogram);
                Some(obj.unchecked_into())
            }
            None => None,
        })
    }
    /// Returns `[macd0, signal0, histogram0, macd1, ...]`, length `3 * n`.
    /// Warmup is NaN.
    pub fn batch(&mut self, close: &[f64], volume: &[f64]) -> Result<Float64Array, JsError> {
        if close.len() != volume.len() {
            return Err(JsError::new("close and volume must be equal length"));
        }
        let mut out = vec![f64::NAN; close.len() * 3];
        for i in 0..close.len() {
            let c = make_candle(close[i], close[i], close[i], volume[i])?;
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.macd;
                out[i * 3 + 1] = o.signal;
                out[i * 3 + 2] = o.histogram;
            }
        }
        Ok(Float64Array::from(out.as_slice()))
    }
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }
    #[wasm_bindgen(js_name = isReady)]
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    #[wasm_bindgen(js_name = warmupPeriod)]
    pub fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

impl Default for WasmWad {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for WasmIntradayIntensity {
    fn default() -> Self {
        Self::new()
    }
}

// ===== Data layer: tick-to-candle aggregation =====

/// Convert a `wickra-data` error into a JS error.
fn map_data_err(e: wickra_data::Error) -> JsError {
    JsError::new(&e.to_string())
}

/// Roll trade ticks up into fixed-timeframe OHLCV candles.
#[wasm_bindgen(js_name = TickAggregator)]
pub struct WasmTickAggregator {
    inner: wickra_data::aggregator::TickAggregator,
}

#[wasm_bindgen(js_class = TickAggregator)]
impl WasmTickAggregator {
    /// Construct an aggregator with the given bucket size (same unit as the tick
    /// timestamps). Pass `gapFill = true` to emit a flat placeholder candle for
    /// every skipped bucket.
    #[wasm_bindgen(constructor)]
    pub fn new(bucket: f64, gap_fill: Option<bool>) -> Result<WasmTickAggregator, JsError> {
        let timeframe =
            wickra_data::aggregator::Timeframe::new(bucket as i64).map_err(map_data_err)?;
        let mut inner = wickra_data::aggregator::TickAggregator::new(timeframe);
        if gap_fill.unwrap_or(false) {
            inner = inner.with_gap_fill(true);
        }
        Ok(Self { inner })
    }

    /// Push one trade tick; returns an array of `{ open, high, low, close,
    /// volume, timestamp }` candles closed as a result.
    pub fn push(
        &mut self,
        price: f64,
        size: f64,
        timestamp: f64,
    ) -> Result<WasmCandleArrayValue, JsError> {
        let tick = wc::Tick::new(price, size, timestamp as i64).map_err(map_err)?;
        let arr = Array::new();
        for c in self.inner.push(tick).map_err(map_data_err)? {
            let obj = js_object!("open" => c.open, "high" => c.high, "low" => c.low, "close" => c.close, "volume" => c.volume, "timestamp" => c.timestamp as f64);
            arr.push(&obj);
        }
        Ok(arr.unchecked_into())
    }

    /// Whether gap filling is enabled.
    #[wasm_bindgen(js_name = fillsGaps)]
    pub fn fills_gaps(&self) -> bool {
        self.inner.fills_gaps()
    }
}

// ===== Data layer: resampling (candle -> higher-timeframe candle) =====

fn candle_object(c: wc::Candle) -> Object {
    js_object!("open" => c.open, "high" => c.high, "low" => c.low, "close" => c.close, "volume" => c.volume, "timestamp" => c.timestamp as f64)
}

/// Resample candles into a higher timeframe (e.g. 1m -> 5m).
#[wasm_bindgen(js_name = Resampler)]
pub struct WasmResampler {
    inner: wickra_data::resample::Resampler,
}

#[wasm_bindgen(js_class = Resampler)]
impl WasmResampler {
    /// Construct a resampler aggregating inputs into `timeframe`-sized candles.
    /// Pass `gapFill = true` to emit a flat placeholder candle for every skipped
    /// bucket.
    #[wasm_bindgen(constructor)]
    pub fn new(timeframe: f64, gap_fill: Option<bool>) -> Result<WasmResampler, JsError> {
        let tf = wickra_data::aggregator::Timeframe::new(timeframe as i64).map_err(map_data_err)?;
        Ok(Self {
            inner: wickra_data::resample::Resampler::new(tf)
                .with_gap_fill(gap_fill.unwrap_or(false)),
        })
    }

    /// Whether the resampler emits a flat placeholder candle for skipped
    /// buckets.
    #[wasm_bindgen(js_name = fillsGaps)]
    pub fn fills_gaps(&self) -> bool {
        self.inner.fills_gaps()
    }

    /// Push one candle; returns an array of the higher-timeframe candles it
    /// completed, each `{ open, high, low, close, volume, timestamp }`. Normally
    /// that is empty or one element; with gap filling on, input that skips whole
    /// buckets completes several at once.
    pub fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: f64,
    ) -> Result<WasmCandleArrayValue, JsError> {
        let candle =
            wc::Candle::new(open, high, low, close, volume, timestamp as i64).map_err(map_err)?;
        let arr = Array::new();
        for c in self.inner.push(candle).map_err(map_data_err)? {
            arr.push(&candle_object(c).into());
        }
        Ok(arr.unchecked_into())
    }

    /// Emit the final, still-open candle (or `undefined` if none is pending).
    pub fn flush(&mut self) -> Result<Option<WasmCandleValue>, JsError> {
        Ok(self
            .inner
            .flush()
            .map_err(map_data_err)?
            .map(|c| candle_object(c).unchecked_into()))
    }
}

// ===== Data layer: CSV candle reader =====

/// Parse OHLCV candles from a CSV string (header `timestamp,open,high,low,close,
/// volume`; a leading UTF-8 BOM is stripped).
#[wasm_bindgen(js_name = CandleReader)]
pub struct WasmCandleReader {
    candles: Vec<wc::Candle>,
}

#[wasm_bindgen(js_class = CandleReader)]
impl WasmCandleReader {
    /// Parse the whole CSV up front; throws on a malformed header or row.
    #[wasm_bindgen(constructor)]
    pub fn new(csv: &str) -> Result<WasmCandleReader, JsError> {
        let mut reader =
            wickra_data::csv::CandleReader::from_reader(csv.as_bytes()).map_err(map_data_err)?;
        let candles = reader.read_all().map_err(map_data_err)?;
        Ok(Self { candles })
    }

    /// Return every parsed candle as a `{ open, high, low, close, volume,
    /// timestamp }` object.
    pub fn read(&self) -> WasmCandleArrayValue {
        let arr = Array::new();
        for &c in &self.candles {
            arr.push(&candle_object(c));
        }
        arr.unchecked_into()
    }
}
