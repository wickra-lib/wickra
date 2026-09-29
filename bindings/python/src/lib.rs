//! Python bindings for Wickra. Built with `PyO3` and exposed under the `wickra` package.
//!
//! This module is the thin glue between `wickra-core` and Python. Every indicator
//! has both a streaming class and a batch helper. Inputs accept any sequence or
//! buffer of numbers (`array.array`, `memoryview`, a `NumPy` array, or a list);
//! results are stdlib `array.array('d')` objects (and a buffer-protocol [`Matrix`]
//! for multi-output indicators), so the package has zero third-party dependencies.

#![allow(clippy::needless_pass_by_value)]
// Python `__repr__` is an instance method by protocol, so the `&self` parameter is
// mandatory even when its body does not read state (e.g. parameterless indicators
// like `TypicalPrice`). Clippy's `unused_self` triggers on those signatures.
#![allow(clippy::unused_self)]
// OHLCV batch helpers bind the conventional single-letter column names
// (o/h/l/c/v) that match the domain and the NumPy call sites.
#![allow(clippy::many_single_char_names)]

use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyBytes, PyDict, PyList, PyMemoryView, PyTuple, PyType};
use pyo3::Borrowed;
use wickra_core as wc;
use wickra_core::{BarBuilder, BatchExt, Indicator};

/// The element type a one-dimensional buffer carries, read from its
/// `memoryview` format string.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Elem {
    Float,
    Signed,
    Unsigned,
}

/// Classify a `memoryview` format string (`struct` syntax) as a native-order
/// number of `itemsize` bytes, or `None` for anything the fast path does not
/// read itself (booleans, half floats, complex, records, the other byte order).
fn native_elem(format: &str, itemsize: usize) -> Option<Elem> {
    let code = match format.as_bytes() {
        [c] | [b'@' | b'=', c] => *c,
        [b'<', c] if cfg!(target_endian = "little") => *c,
        [b'>' | b'!', c] if cfg!(target_endian = "big") => *c,
        _ => return None,
    };
    let elem = match code {
        b'f' | b'd' => Elem::Float,
        b'b' | b'h' | b'i' | b'l' | b'q' | b'n' => Elem::Signed,
        b'B' | b'H' | b'I' | b'L' | b'Q' | b'N' => Elem::Unsigned,
        _ => return None,
    };
    let width_ok = match elem {
        Elem::Float => itemsize == 4 || itemsize == 8,
        Elem::Signed | Elem::Unsigned => matches!(itemsize, 1 | 2 | 4 | 8),
    };
    width_ok.then_some(elem)
}

/// The raw bytes of a one-dimensional numeric buffer and how to read them, or
/// `None` when `obj` is a list/tuple, exposes no buffer, or is a buffer the
/// fast path leaves to the sequence protocol.
///
/// `tobytes()` is one C-level copy of the whole buffer (it also flattens a
/// strided view into order), which replaces converting every element through a
/// Python float object — the abi3 wheels cannot borrow the buffer in place,
/// since `PyObject_GetBuffer` only joined the limited API in Python 3.11.
fn buffer_bytes<'py>(obj: &Bound<'py, PyAny>) -> Option<(Bound<'py, PyBytes>, Elem, usize)> {
    if obj.is_instance_of::<PyList>() || obj.is_instance_of::<PyTuple>() {
        return None;
    }
    let view = PyMemoryView::from(obj).ok()?;
    if view.getattr("ndim").ok()?.extract::<usize>().ok()? != 1 {
        return None;
    }
    let itemsize = view.getattr("itemsize").ok()?.extract::<usize>().ok()?;
    let format = view.getattr("format").ok()?;
    let elem = native_elem(&format.extract::<String>().ok()?, itemsize)?;
    let bytes = view
        .call_method0("tobytes")
        .ok()?
        .cast_into::<PyBytes>()
        .ok()?;
    Some((bytes, elem, itemsize))
}

/// Read one native-order element of `width` bytes as `f64`, with the rounding
/// Python's own `float(x)` applies (integers round to nearest, ties to even).
fn elem_f64(chunk: &[u8], elem: Elem) -> f64 {
    macro_rules! read {
        ($t:ty) => {
            <$t>::from_ne_bytes(chunk.try_into().expect("chunk is exactly one element"))
        };
    }
    #[allow(clippy::cast_precision_loss)]
    match (elem, chunk.len()) {
        (Elem::Float, 4) => f64::from(read!(f32)),
        (Elem::Float, _) => read!(f64),
        (Elem::Signed, 1) => f64::from(read!(i8)),
        (Elem::Signed, 2) => f64::from(read!(i16)),
        (Elem::Signed, 4) => f64::from(read!(i32)),
        (Elem::Signed, _) => read!(i64) as f64,
        (Elem::Unsigned, 1) => f64::from(read!(u8)),
        (Elem::Unsigned, 2) => f64::from(read!(u16)),
        (Elem::Unsigned, 4) => f64::from(read!(u32)),
        (Elem::Unsigned, _) => read!(u64) as f64,
    }
}

/// A one-dimensional `f64` input.
///
/// Accepts `array.array`, `memoryview`, a `NumPy` `ndarray`, or any plain Python
/// sequence of numbers — the same set the previous `NumPy` `PyReadonlyArray1`
/// covered, without depending on `NumPy`. A native `float64` buffer is read with a
/// single copy and borrowed in place when that copy is 8-byte aligned (the
/// `bytes` object stays alive inside the value); other numeric buffers are
/// widened to `f64` in one pass; lists, tuples and anything else go through the
/// sequence protocol exactly as before.
struct Buf1 {
    /// A native `float64` buffer's single `tobytes()` copy, read in place when
    /// its data is 8-byte aligned. Python `bytes` are immutable, so the values
    /// cannot change while this value holds the reference.
    bytes: Option<Py<PyBytes>>,
    /// Values converted element by element (other dtypes, lists, a misaligned
    /// copy) — used when `bytes` is `None`.
    owned: Vec<f64>,
}

impl<'py> FromPyObject<'_, 'py> for Buf1 {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> PyResult<Self> {
        if let Some((bytes, elem, itemsize)) = buffer_bytes(&obj) {
            let raw = bytes.as_bytes();
            if elem == Elem::Float
                && itemsize == 8
                && bytemuck::try_cast_slice::<u8, f64>(raw).is_ok()
            {
                return Ok(Self {
                    bytes: Some(bytes.unbind()),
                    owned: Vec::new(),
                });
            }
            let owned = raw
                .chunks_exact(itemsize)
                .map(|c| elem_f64(c, elem))
                .collect();
            return Ok(Self { bytes: None, owned });
        }
        Vec::<f64>::extract(obj).map(|owned| Self { bytes: None, owned })
    }
}

impl Buf1 {
    /// Borrow the values as a slice.
    fn as_slice(&self) -> &[f64] {
        match &self.bytes {
            // `Py::as_bytes` ties the slice to `self`, not to the `Python` token,
            // and the alignment was checked when the value was built.
            Some(bytes) => Python::attach(|py| {
                bytemuck::try_cast_slice(bytes.as_bytes(py)).expect("checked aligned at extraction")
            }),
            None => &self.owned,
        }
    }
}

/// A one-dimensional `i64` input (e.g. millisecond timestamps for seasonality).
///
/// Mirrors [`Buf1`]: a native integer buffer is read with one copy and widened
/// in one pass (an unsigned 64-bit value above `i64::MAX` is left to the
/// sequence protocol, which raises the overflow error it always did); floats,
/// lists and everything else go through the sequence protocol as before.
struct BufI64 {
    data: Vec<i64>,
}

impl<'py> FromPyObject<'_, 'py> for BufI64 {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> PyResult<Self> {
        if let Some((bytes, elem, itemsize)) = buffer_bytes(&obj) {
            if elem != Elem::Float {
                let raw = bytes.as_bytes();
                let data: Option<Vec<i64>> = raw
                    .chunks_exact(itemsize)
                    .map(|c| elem_i64(c, elem))
                    .collect();
                if let Some(data) = data {
                    return Ok(Self { data });
                }
            }
        }
        Vec::<i64>::extract(obj).map(|data| Self { data })
    }
}

/// Read one native-order integer element as `i64`, or `None` for an unsigned
/// 64-bit value that does not fit.
fn elem_i64(chunk: &[u8], elem: Elem) -> Option<i64> {
    macro_rules! read {
        ($t:ty) => {
            <$t>::from_ne_bytes(chunk.try_into().expect("chunk is exactly one element"))
        };
    }
    Some(match (elem, chunk.len()) {
        (Elem::Signed, 1) => i64::from(read!(i8)),
        (Elem::Signed, 2) => i64::from(read!(i16)),
        (Elem::Signed, 4) => i64::from(read!(i32)),
        (Elem::Signed, _) => read!(i64),
        (Elem::Unsigned, 1) => i64::from(read!(u8)),
        (Elem::Unsigned, 2) => i64::from(read!(u16)),
        (Elem::Unsigned, 4) => i64::from(read!(u32)),
        (_, _) => i64::try_from(read!(u64)).ok()?,
    })
}

impl BufI64 {
    /// Borrow the values as a slice.
    fn as_slice(&self) -> &[i64] {
        &self.data
    }
}

/// `array.array`, imported once per interpreter.
static ARRAY_TYPE: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// Build a stdlib `array.array('d')` from a slice of `f64`s.
///
/// `array('d', x)` iterates any `x` that is not `bytes`/`list`/`array` element
/// by element, so the values go in as one `bytes` object, which `array` copies
/// with a single `memcpy`. `array.array` is a buffer-protocol object, so
/// `numpy.asarray(result)` wraps it zero-copy for callers who opt into `NumPy` —
/// but importing `NumPy` is never required.
fn f64_array<'py>(py: Python<'py>, data: &[f64]) -> PyResult<Bound<'py, PyAny>> {
    let bytes = PyBytes::new(py, bytemuck::cast_slice(data));
    ARRAY_TYPE.import(py, "array", "array")?.call1(("d", bytes))
}

/// Run a batch that writes one `f64` per input straight into the result's
/// storage and return it as `array.array('d')`.
///
/// `fill` writes into the `bytes` object that seeds the array, so the values
/// are copied once (into the array) instead of going through an intermediate
/// `Vec` first. `bytes` data is 16-byte aligned in `CPython`; should it ever not
/// be `f64`-aligned, the values are computed in a `Vec` and copied in.
fn f64_batch(
    py: Python<'_>,
    len: usize,
    fill: impl FnOnce(&mut [f64]),
) -> PyResult<Bound<'_, PyAny>> {
    let bytes = PyBytes::new_with(py, len * std::mem::size_of::<f64>(), |raw| {
        if let Ok(out) = bytemuck::try_cast_slice_mut::<u8, f64>(raw) {
            fill(out);
        } else {
            let mut values = vec![0.0; len];
            fill(&mut values);
            raw.copy_from_slice(bytemuck::cast_slice(&values));
        }
        Ok(())
    })?;
    ARRAY_TYPE.import(py, "array", "array")?.call1(("d", bytes))
}

/// A row-major, two-dimensional `f64` result returned by multi-output batch helpers.
///
/// Rows come back as flat, buffer-protocol `array.array('d')` values, so it
/// preserves the ergonomics of the former `NumPy` return type — `result.shape`,
/// integer row access and `result[i, j]` element access — without depending on
/// `NumPy`. Callers who want an `(nrows, ncols)` `NumPy` array build one with
/// `numpy.asarray(result.tolist())`; exposing the block through the buffer
/// protocol instead would let `numpy.asarray(result)` do it directly, but
/// `Py_buffer` is outside the limited API these `abi3` wheels are built
/// against.
#[pyclass(name = "Matrix", module = "wickra._wickra")]
struct Matrix {
    data: Vec<f64>,
    nrows: usize,
    ncols: usize,
}

impl Matrix {
    /// Resolve a possibly-negative index against `len`, mirroring Python semantics.
    fn resolve(index: isize, len: usize) -> PyResult<usize> {
        let idx = if index < 0 {
            let back = index.unsigned_abs();
            if back > len {
                return Err(PyIndexError::new_err("index out of range"));
            }
            len - back
        } else {
            index.unsigned_abs()
        };
        if idx >= len {
            return Err(PyIndexError::new_err("index out of range"));
        }
        Ok(idx)
    }
}

#[pymethods]
impl Matrix {
    #[getter]
    fn shape(&self) -> (usize, usize) {
        (self.nrows, self.ncols)
    }

    fn __len__(&self) -> usize {
        self.nrows
    }

    fn __getitem__<'py>(
        &self,
        py: Python<'py>,
        key: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if let Ok((row, col)) = key.extract::<(isize, isize)>() {
            let r = Self::resolve(row, self.nrows)?;
            let c = Self::resolve(col, self.ncols)?;
            return Ok(self.data[r * self.ncols + c].into_pyobject(py)?.into_any());
        }
        let row = Self::resolve(key.extract::<isize>()?, self.nrows)?;
        let start = row * self.ncols;
        f64_array(py, &self.data[start..start + self.ncols])
    }

    /// Return the matrix as a list of row lists.
    fn tolist(&self) -> Vec<Vec<f64>> {
        self.data.chunks(self.ncols).map(<[f64]>::to_vec).collect()
    }

    fn __repr__(&self) -> String {
        format!("Matrix(shape=({}, {}))", self.nrows, self.ncols)
    }
}

/// Build a [`Matrix`] from flat row-major data.
fn matrix(
    py: Python<'_>,
    data: Vec<f64>,
    nrows: usize,
    ncols: usize,
) -> PyResult<Bound<'_, PyAny>> {
    Ok(Bound::new(py, Matrix { data, nrows, ncols })?.into_any())
}

/// Convert an owned `f64` batch result into its Python representation
/// (a buffer-protocol `array.array('d')`), keeping the streaming batch call sites
/// uniform after the `NumPy` return type was dropped.
trait IntoPyData<'py> {
    fn into_pydata(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> IntoPyData<'py> for Vec<f64> {
    fn into_pydata(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        f64_array(py, &self)
    }
}

fn map_err(e: wc::Error) -> PyErr {
    // Every `wickra_core::Error` is a rejected argument, so they all map to the
    // same Python exception. Listing the variants added nothing but a match that
    // has to be revisited whenever the enum grows (it is `#[non_exhaustive]`).
    PyValueError::new_err(e.to_string())
}

/// Borrowed `(open, high, low, close)` columns for candle-driven bar builders.
type OhlcCols<'a> = (&'a [f64], &'a [f64], &'a [f64], &'a [f64]);
/// Borrowed `(open, high, low, close, volume)` columns for candle-driven bar builders.
type OhlcvCols<'a> = (&'a [f64], &'a [f64], &'a [f64], &'a [f64], &'a [f64]);

/// `(open, high, low, close, volume)` rows from Tick/Volume bar builders.
type OhlcvBarRows = Vec<(f64, f64, f64, f64, f64)>;
/// `(open, high, low, close, volume, dollar)` rows from the Dollar bar builder.
type DollarBarRows = Vec<(f64, f64, f64, f64, f64, f64)>;
/// `(open, high, low, close, imbalance, direction)` rows from the Imbalance bar builder.
type ImbalanceBarRows = Vec<(f64, f64, f64, f64, f64, i64)>;
/// `(open, high, low, close, length, direction)` rows from the Run bar builder.
type RunBarRows = Vec<(f64, f64, f64, f64, i64, i64)>;

/// Extract four OHLC slices, erroring if their lengths differ.
fn ohlc_slices<'a>(
    open: &'a Buf1,
    high: &'a Buf1,
    low: &'a Buf1,
    close: &'a Buf1,
) -> PyResult<OhlcCols<'a>> {
    let o = open.as_slice();
    let h = high.as_slice();
    let l = low.as_slice();
    let c = close.as_slice();
    if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
        return Err(PyValueError::new_err(
            "open, high, low, close must be equal length",
        ));
    }
    Ok((o, h, l, c))
}

/// Extract five OHLCV slices, erroring if their lengths differ.
fn ohlcv_slices<'a>(
    open: &'a Buf1,
    high: &'a Buf1,
    low: &'a Buf1,
    close: &'a Buf1,
    volume: &'a Buf1,
) -> PyResult<OhlcvCols<'a>> {
    let (o, h, l, c) = ohlc_slices(open, high, low, close)?;
    let v = volume.as_slice();
    if v.len() != o.len() {
        return Err(PyValueError::new_err(
            "open, high, low, close, volume must be equal length",
        ));
    }
    Ok((o, h, l, c, v))
}

/// `(pp, r1, r2, r3, s1, s2, s3)` pivot levels returned by Classic/Fibonacci pivots.
type PivotLevels = (f64, f64, f64, f64, f64, f64, f64);
/// The five Fibonacci-extension levels returned by `FibExtension`.
type FibExtLevels = (f64, f64, f64, f64, f64);
/// `(pp, r1, r2, s1, s2)` pivot levels returned by Woodie pivots.
type WoodieLevels = (f64, f64, f64, f64, f64);
/// `(current, min, median, max, percentile)` volatility-cone envelope.
type ConeBands = (f64, f64, f64, f64, f64);
/// `(tenkan, kijun, senkou_a, senkou_b, chikou)` Ichimoku lines, each optional during warmup.
type IchimokuLines = (
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
);

// ============================== SMA ==============================

#[pyclass(name = "SMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySma {
    inner: wc::Sma,
}

#[pymethods]
impl PySma {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Sma::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("SMA(period={})", self.inner.period())
    }
}

// ============================== EMA ==============================

#[pyclass(name = "EMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyEma {
    inner: wc::Ema,
}

#[pymethods]
impl PyEma {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Ema::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn alpha(&self) -> f64 {
        self.inner.alpha()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("EMA(period={})", self.inner.period())
    }
}

// ============================== WMA ==============================

#[pyclass(name = "WMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyWma {
    inner: wc::Wma,
}

#[pymethods]
impl PyWma {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Wma::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("WMA(period={})", self.inner.period())
    }
}

// ============================== RSI ==============================

#[pyclass(name = "RSI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRsi {
    inner: wc::Rsi,
}

#[pymethods]
impl PyRsi {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Rsi::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RSI(period={})", self.inner.period())
    }
}

// ============================== MACD ==============================

#[pyclass(name = "MACD", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMacd {
    inner: wc::MacdIndicator,
}

#[pymethods]
impl PyMacd {
    #[new]
    #[pyo3(signature = (fast=12, slow=26, signal=9))]
    fn new(fast: usize, slow: usize, signal: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MacdIndicator::new(fast, slow, signal).map_err(map_err)?,
        })
    }
    /// Returns `(macd, signal, histogram)` or `None` during warmup.
    fn update(&mut self, value: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update(value)
            .map(|o| (o.macd, o.signal, o.histogram))
    }
    /// Batch over a series of closes. Returns a 2D array of shape `(n, 3)`
    /// with columns `[macd, signal, histogram]`. Warmup rows are NaN.
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let out = self.inner.batch_macd(slice);
        matrix(py, out, n, 3)
    }
    #[getter]
    fn periods(&self) -> (usize, usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (f, s, sig) = self.inner.periods();
        format!("MACD(fast={f}, slow={s}, signal={sig})")
    }
}

// ============================== Bollinger Bands ==============================

#[pyclass(
    name = "BollingerBands",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyBb {
    inner: wc::BollingerBands,
}

#[pymethods]
impl PyBb {
    #[new]
    #[pyo3(signature = (period=20, multiplier=2.0))]
    fn new(period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::BollingerBands::new(period, multiplier).map_err(map_err)?,
        })
    }
    /// Returns `(upper, middle, lower, stddev)` or `None` during warmup.
    fn update(&mut self, value: f64) -> Option<(f64, f64, f64, f64)> {
        self.inner
            .update(value)
            .map(|o| (o.upper, o.middle, o.lower, o.stddev))
    }
    /// Batch returns shape `(n, 4)` columns `[upper, middle, lower, stddev]`.
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let out = self.inner.batch_bands(slice);
        matrix(py, out, n, 4)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn multiplier(&self) -> f64 {
        self.inner.multiplier()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "BollingerBands(period={}, multiplier={})",
            self.inner.period(),
            self.inner.multiplier()
        )
    }
}

// ============================== ATR ==============================

fn extract_candle(d: &Bound<'_, PyAny>) -> PyResult<wc::Candle> {
    // Accept either a dict-like with open/high/low/close/volume/timestamp,
    // or a tuple (open, high, low, close, volume, timestamp).
    if let Ok(tup) = d.extract::<(f64, f64, f64, f64, f64, i64)>() {
        return wc::Candle::new(tup.0, tup.1, tup.2, tup.3, tup.4, tup.5).map_err(map_err);
    }
    if let Ok(dict) = d.cast::<PyDict>() {
        let g = |k: &str| -> PyResult<f64> {
            dict.get_item(k)?
                .ok_or_else(|| PyValueError::new_err(format!("candle missing key '{k}'")))?
                .extract::<f64>()
        };
        let ts = dict
            .get_item("timestamp")?
            .map(|v| v.extract::<i64>())
            .transpose()?
            .unwrap_or(0);
        return wc::Candle::new(
            g("open")?,
            g("high")?,
            g("low")?,
            g("close")?,
            g("volume")?,
            ts,
        )
        .map_err(map_err);
    }
    Err(PyTypeError::new_err(
        "candle must be a 6-tuple (open, high, low, close, volume, timestamp) or a dict",
    ))
}

#[pyclass(name = "ATR", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAtr {
    inner: wc::Atr,
}

#[pymethods]
impl PyAtr {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Atr::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        // Validate the OHLC invariants once (the streaming path gets this from
        // `Candle::new`); ATR uses the close as open, so `high >= close >= low`.
        for i in 0..h.len() {
            if !(h[i].is_finite() && l[i].is_finite() && c[i].is_finite())
                || h[i] < l[i]
                || h[i] < c[i]
                || l[i] > c[i]
            {
                return Err(PyValueError::new_err(
                    "high, low, close must be finite with low <= close <= high",
                ));
            }
        }
        self.inner.batch_atr(h, l, c).into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ATR(period={})", self.inner.period())
    }
}

// ============================== Plus DM ==============================

#[pyclass(name = "PLUS_DM", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPlusDm {
    inner: wc::PlusDm,
}

#[pymethods]
impl PyPlusDm {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::PlusDm::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("PLUS_DM(period={})", self.inner.period())
    }
}

// ============================== Minus DM ==============================

#[pyclass(name = "MINUS_DM", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMinusDm {
    inner: wc::MinusDm,
}

#[pymethods]
impl PyMinusDm {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MinusDm::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("MINUS_DM(period={})", self.inner.period())
    }
}

// ============================== PlusDi ==============================

#[pyclass(name = "PLUS_DI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPlusDi {
    inner: wc::PlusDi,
}

#[pymethods]
impl PyPlusDi {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::PlusDi::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("PLUS_DI(period={})", self.inner.period())
    }
}

// ============================== MinusDi ==============================

#[pyclass(name = "MINUS_DI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMinusDi {
    inner: wc::MinusDi,
}

#[pymethods]
impl PyMinusDi {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MinusDi::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("MINUS_DI(period={})", self.inner.period())
    }
}

// ============================== Dx ==============================

#[pyclass(name = "DX", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyDx {
    inner: wc::Dx,
}

#[pymethods]
impl PyDx {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Dx::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("DX(period={})", self.inner.period())
    }
}

// ============================== MidPrice ==============================

#[pyclass(name = "MIDPRICE", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMidPrice {
    inner: wc::MidPrice,
}

#[pymethods]
impl PyMidPrice {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MidPrice::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("MIDPRICE(period={})", self.inner.period())
    }
}

// ============================== MidPoint ==============================

#[pyclass(name = "MIDPOINT", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMidPoint {
    inner: wc::MidPoint,
}

#[pymethods]
impl PyMidPoint {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MidPoint::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("MIDPOINT(period={})", self.inner.period())
    }
}

// ============================== Avg Price ==============================

#[pyclass(name = "AVGPRICE", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAvgPrice {
    inner: wc::AvgPrice,
}

#[pymethods]
impl PyAvgPrice {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::AvgPrice::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: open, high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if !(o.len() == h.len() && h.len() == l.len() && l.len() == c.len()) {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "AVGPRICE()".to_string()
    }
}

// ============================== Rocp ==============================

#[pyclass(name = "ROCP", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRocp {
    inner: wc::Rocp,
}

#[pymethods]
impl PyRocp {
    #[new]
    #[pyo3(signature = (period=10))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Rocp::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ROCP(period={})", self.inner.period())
    }
}

// ============================== Rocr ==============================

#[pyclass(name = "ROCR", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRocr {
    inner: wc::Rocr,
}

#[pymethods]
impl PyRocr {
    #[new]
    #[pyo3(signature = (period=10))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Rocr::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ROCR(period={})", self.inner.period())
    }
}

// ============================== Rocr100 ==============================

#[pyclass(name = "ROCR100", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRocr100 {
    inner: wc::Rocr100,
}

#[pymethods]
impl PyRocr100 {
    #[new]
    #[pyo3(signature = (period=10))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Rocr100::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ROCR100(period={})", self.inner.period())
    }
}

// ============================== LinRegIntercept ==============================

#[pyclass(
    name = "LINEARREG_INTERCEPT",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyLinRegIntercept {
    inner: wc::LinRegIntercept,
}

#[pymethods]
impl PyLinRegIntercept {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::LinRegIntercept::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("LINEARREG_INTERCEPT(period={})", self.inner.period())
    }
}

// ============================== Tsf ==============================

#[pyclass(name = "TSF", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTsf {
    inner: wc::Tsf,
}

#[pymethods]
impl PyTsf {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Tsf::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TSF(period={})", self.inner.period())
    }
}

// ============================== MACD Fix ==============================

#[pyclass(name = "MACDFIX", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMacdFix {
    inner: wc::MacdFix,
}

#[pymethods]
impl PyMacdFix {
    #[new]
    #[pyo3(signature = (signal=9))]
    fn new(signal: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MacdFix::new(signal).map_err(map_err)?,
        })
    }
    /// Returns `(macd, signal, histogram)` or `None` during warmup.
    fn update(&mut self, value: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update(value)
            .map(|o| (o.macd, o.signal, o.histogram))
    }
    /// Batch over a series of closes. Returns a 2D array of shape `(n, 3)`
    /// with columns `[macd, signal, histogram]`. Warmup rows are NaN.
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.macd;
                out[i * 3 + 1] = o.signal;
                out[i * 3 + 2] = o.histogram;
            }
        }
        matrix(py, out, n, 3)
    }
    #[getter]
    fn signal_period(&self) -> usize {
        self.inner.signal_period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("MACDFIX(signal={})", self.inner.signal_period())
    }
}

// ============================== SAR Extended ==============================

#[pyclass(name = "SAREXT", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySarExt {
    inner: wc::SarExt,
}

#[pymethods]
impl PySarExt {
    #[new]
    #[pyo3(signature = (
        start_value=0.0,
        offset_on_reverse=0.0,
        accel_init_long=0.02,
        accel_long=0.02,
        accel_max_long=0.2,
        accel_init_short=0.02,
        accel_short=0.02,
        accel_max_short=0.2,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        start_value: f64,
        offset_on_reverse: f64,
        accel_init_long: f64,
        accel_long: f64,
        accel_max_long: f64,
        accel_init_short: f64,
        accel_short: f64,
        accel_max_short: f64,
    ) -> PyResult<Self> {
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
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "SAREXT()".to_string()
    }
}

// ============================== MACD Extended ==============================

#[pyclass(name = "MACDEXT", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMacdExt {
    inner: wc::MacdExt,
}

#[pymethods]
impl PyMacdExt {
    /// Moving-average types are TA-Lib `MA_Type` codes `0..=5`
    /// (SMA, EMA, WMA, DEMA, TEMA, TRIMA).
    #[new]
    #[pyo3(signature = (
        fast=12,
        fast_matype=0,
        slow=26,
        slow_matype=0,
        signal=9,
        signal_matype=0,
    ))]
    fn new(
        fast: usize,
        fast_matype: u32,
        slow: usize,
        slow_matype: u32,
        signal: usize,
        signal_matype: u32,
    ) -> PyResult<Self> {
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
    /// Returns `(macd, signal, histogram)` or `None` during warmup.
    fn update(&mut self, value: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update(value)
            .map(|o| (o.macd, o.signal, o.histogram))
    }
    /// Batch over a series of closes. Returns a 2D array of shape `(n, 3)`
    /// with columns `[macd, signal, histogram]`. Warmup rows are NaN.
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.macd;
                out[i * 3 + 1] = o.signal;
                out[i * 3 + 2] = o.histogram;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "MACDEXT()".to_string()
    }
}

// ============================== HT Phasor ==============================

#[pyclass(name = "HT_PHASOR", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyHtPhasor {
    inner: wc::HtPhasor,
}

#[pymethods]
impl PyHtPhasor {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::HtPhasor::new(),
        }
    }
    /// Returns `(inphase, quadrature)` or `None` during warmup.
    fn update(&mut self, value: f64) -> Option<(f64, f64)> {
        self.inner.update(value).map(|o| (o.inphase, o.quadrature))
    }
    /// Batch over a series of closes. Returns a 2D array of shape `(n, 2)`
    /// with columns `[inphase, quadrature]`. Warmup rows are NaN.
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 2];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 2] = o.inphase;
                out[i * 2 + 1] = o.quadrature;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "HT_PHASOR()".to_string()
    }
}

// ============================== LogReturn ==============================

#[pyclass(name = "LogReturn", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyLogReturn {
    inner: wc::LogReturn,
}

#[pymethods]
impl PyLogReturn {
    #[new]
    #[pyo3(signature = (period=1))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::LogReturn::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("LogReturn(period={})", self.inner.period())
    }
}

// ============================== RealizedVolatility ==============================

#[pyclass(
    name = "RealizedVolatility",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyRealizedVolatility {
    inner: wc::RealizedVolatility,
}

#[pymethods]
impl PyRealizedVolatility {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RealizedVolatility::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RealizedVolatility(period={})", self.inner.period())
    }
}

// ============================== RollingIqr ==============================

#[pyclass(name = "RollingIqr", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRollingIqr {
    inner: wc::RollingIqr,
}

#[pymethods]
impl PyRollingIqr {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RollingIqr::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RollingIqr(period={})", self.inner.period())
    }
}

// ============================== RollingPercentileRank ==============================

#[pyclass(
    name = "RollingPercentileRank",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyRollingPercentileRank {
    inner: wc::RollingPercentileRank,
}

#[pymethods]
impl PyRollingPercentileRank {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RollingPercentileRank::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RollingPercentileRank(period={})", self.inner.period())
    }
}

// ============================== RollingQuantile ==============================

#[pyclass(
    name = "RollingQuantile",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyRollingQuantile {
    inner: wc::RollingQuantile,
}

#[pymethods]
impl PyRollingQuantile {
    #[new]
    #[pyo3(signature = (period=20, quantile=0.5))]
    fn new(period: usize, quantile: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RollingQuantile::new(period, quantile).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn quantile(&self) -> f64 {
        self.inner.quantile()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "RollingQuantile(period={}, quantile={})",
            self.inner.period(),
            self.inner.quantile()
        )
    }
}

// ============================== CloseVsOpen ==============================

#[pyclass(name = "CloseVsOpen", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyCloseVsOpen {
    inner: wc::CloseVsOpen,
}

#[pymethods]
impl PyCloseVsOpen {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::CloseVsOpen::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: open, high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(o.len());
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "CloseVsOpen()".to_string()
    }
}

// ============================== BodySizePct ==============================

#[pyclass(name = "BodySizePct", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyBodySizePct {
    inner: wc::BodySizePct,
}

#[pymethods]
impl PyBodySizePct {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::BodySizePct::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: open, high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(o.len());
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "BodySizePct()".to_string()
    }
}

// ============================== WickRatio ==============================

#[pyclass(name = "WickRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyWickRatio {
    inner: wc::WickRatio,
}

#[pymethods]
impl PyWickRatio {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::WickRatio::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: open, high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(o.len());
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "WickRatio()".to_string()
    }
}

// ============================== HighLowRange ==============================

#[pyclass(name = "HighLowRange", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyHighLowRange {
    inner: wc::HighLowRange,
}

#[pymethods]
impl PyHighLowRange {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::HighLowRange::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: open, high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(o.len());
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "HighLowRange()".to_string()
    }
}

// ============================== TrendLabel ==============================

#[pyclass(name = "TrendLabel", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTrendLabel {
    inner: wc::TrendLabel,
}

#[pymethods]
impl PyTrendLabel {
    #[new]
    #[pyo3(signature = (period=10))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TrendLabel::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TrendLabel(period={})", self.inner.period())
    }
}

// ============================== JumpIndicator ==============================

#[pyclass(name = "JumpIndicator", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyJumpIndicator {
    inner: wc::JumpIndicator,
}

#[pymethods]
impl PyJumpIndicator {
    #[new]
    #[pyo3(signature = (period=20, threshold=3.0))]
    fn new(period: usize, threshold: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::JumpIndicator::new(period, threshold).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.params().0
    }
    #[getter]
    fn threshold(&self) -> f64 {
        self.inner.params().1
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (period, threshold) = self.inner.params();
        format!("JumpIndicator(period={period}, threshold={threshold})")
    }
}

// ============================== RegimeLabel ==============================

#[pyclass(name = "RegimeLabel", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRegimeLabel {
    inner: wc::RegimeLabel,
}

#[pymethods]
impl PyRegimeLabel {
    #[new]
    #[pyo3(signature = (vol_period=5, lookback=20))]
    fn new(vol_period: usize, lookback: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RegimeLabel::new(vol_period, lookback).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn vol_period(&self) -> usize {
        self.inner.params().0
    }
    #[getter]
    fn lookback(&self) -> usize {
        self.inner.params().1
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (vol_period, lookback) = self.inner.params();
        format!("RegimeLabel(vol_period={vol_period}, lookback={lookback})")
    }
}

// ============================== WinRate ==============================

#[pyclass(name = "WinRate", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyWinRate {
    inner: wc::WinRate,
}

#[pymethods]
impl PyWinRate {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::WinRate::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("WinRate(period={})", self.inner.period())
    }
}

// ============================== Expectancy ==============================

#[pyclass(name = "Expectancy", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyExpectancy {
    inner: wc::Expectancy,
}

#[pymethods]
impl PyExpectancy {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Expectancy::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("Expectancy(period={})", self.inner.period())
    }
}

// ============================== SineWeightedMa ==============================

#[pyclass(name = "SWMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySineWeightedMa {
    inner: wc::SineWeightedMa,
}

#[pymethods]
impl PySineWeightedMa {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::SineWeightedMa::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("SWMA(period={})", self.inner.period())
    }
}

// ============================== GeometricMa ==============================

#[pyclass(name = "GMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyGeometricMa {
    inner: wc::GeometricMa,
}

#[pymethods]
impl PyGeometricMa {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::GeometricMa::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("GMA(period={})", self.inner.period())
    }
}

// ============================== Ehma ==============================

#[pyclass(name = "EHMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyEhma {
    inner: wc::Ehma,
}

#[pymethods]
impl PyEhma {
    #[new]
    #[pyo3(signature = (period=9))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Ehma::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("EHMA(period={})", self.inner.period())
    }
}

// ============================== MedianMa ==============================

#[pyclass(name = "MedianMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMedianMa {
    inner: wc::MedianMa,
}

#[pymethods]
impl PyMedianMa {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MedianMa::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("MedianMA(period={})", self.inner.period())
    }
}

// ============================== AdaptiveLaguerreFilter ==============================

#[pyclass(
    name = "AdaptiveLaguerre",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAdaptiveLaguerreFilter {
    inner: wc::AdaptiveLaguerreFilter,
}

#[pymethods]
impl PyAdaptiveLaguerreFilter {
    #[new]
    #[pyo3(signature = (period=13))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AdaptiveLaguerreFilter::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("AdaptiveLaguerre(period={})", self.inner.period())
    }
}

// ============================== DisparityIndex ==============================

#[pyclass(
    name = "DisparityIndex",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyDisparityIndex {
    inner: wc::DisparityIndex,
}

#[pymethods]
impl PyDisparityIndex {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::DisparityIndex::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("DisparityIndex(period={})", self.inner.period())
    }
}

// ============================== FisherRsi ==============================

#[pyclass(name = "FisherRSI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFisherRsi {
    inner: wc::FisherRsi,
}

#[pymethods]
impl PyFisherRsi {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::FisherRsi::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("FisherRSI(period={})", self.inner.period())
    }
}

// ============================== Rsx ==============================

#[pyclass(name = "RSX", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRsx {
    inner: wc::Rsx,
}

#[pymethods]
impl PyRsx {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Rsx::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn length(&self) -> usize {
        self.inner.length()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RSX(length={})", self.inner.length())
    }
}

// ============================== DynamicMomentumIndex ==============================

#[pyclass(
    name = "DynamicMomentumIndex",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyDynamicMomentumIndex {
    inner: wc::DynamicMomentumIndex,
}

#[pymethods]
impl PyDynamicMomentumIndex {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::DynamicMomentumIndex::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("DynamicMomentumIndex(period={})", self.inner.period())
    }
}

// ============================== StochasticCci ==============================

#[pyclass(name = "StochasticCCI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyStochasticCci {
    inner: wc::StochasticCci,
}

#[pymethods]
impl PyStochasticCci {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::StochasticCci::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("StochasticCCI(period={})", self.inner.period())
    }
}

// ============================== TtmTrend ==============================

#[pyclass(name = "TTM_TREND", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTtmTrend {
    inner: wc::TtmTrend,
}

#[pymethods]
impl PyTtmTrend {
    #[new]
    #[pyo3(signature = (period=6))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TtmTrend::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TTM_TREND(period={})", self.inner.period())
    }
}

// ============================== TrendStrengthIndex ==============================

#[pyclass(
    name = "TREND_STRENGTH_INDEX",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyTrendStrengthIndex {
    inner: wc::TrendStrengthIndex,
}

#[pymethods]
impl PyTrendStrengthIndex {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TrendStrengthIndex::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TREND_STRENGTH_INDEX(period={})", self.inner.period())
    }
}

// ============================== Qstick ==============================

#[pyclass(name = "Qstick", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyQstick {
    inner: wc::Qstick,
}

#[pymethods]
impl PyQstick {
    #[new]
    #[pyo3(signature = (period=10))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Qstick::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over open/close input columns (Qstick reads the body close-open).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let c = close.as_slice();
        if o.len() != c.len() {
            return Err(PyValueError::new_err("open, close must be equal length"));
        }
        let n = o.len();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let hi = o[i].max(c[i]);
            let lo = o[i].min(c[i]);
            let candle = wc::Candle::new(o[i], hi, lo, c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== PolarizedFractalEfficiency ==============================

#[pyclass(
    name = "POLARIZED_FRACTAL_EFFICIENCY",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyPolarizedFractalEfficiency {
    inner: wc::PolarizedFractalEfficiency,
}

#[pymethods]
impl PyPolarizedFractalEfficiency {
    #[new]
    #[pyo3(signature = (period=10, smoothing=5))]
    fn new(period: usize, smoothing: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::PolarizedFractalEfficiency::new(period, smoothing).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.periods().0
    }
    #[getter]
    fn smoothing(&self) -> usize {
        self.inner.periods().1
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== WavePm ==============================

#[pyclass(name = "WAVE_PM", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyWavePm {
    inner: wc::WavePm,
}

#[pymethods]
impl PyWavePm {
    #[new]
    #[pyo3(signature = (length=32, smoothing=3))]
    fn new(length: usize, smoothing: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::WavePm::new(length, smoothing).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn length(&self) -> usize {
        self.inner.periods().0
    }
    #[getter]
    fn smoothing(&self) -> usize {
        self.inner.periods().1
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== GatorOscillator ==============================

#[pyclass(
    name = "GatorOscillator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyGatorOscillator {
    inner: wc::GatorOscillator,
}

#[pymethods]
impl PyGatorOscillator {
    #[new]
    #[pyo3(signature = (jaw_period=13, teeth_period=8, lips_period=5))]
    fn new(jaw_period: usize, teeth_period: usize, lips_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::GatorOscillator::new(jaw_period, teeth_period, lips_period)
                .map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.upper, o.lower)))
    }
    /// Batch over high/low/close input columns. Returns shape `(n, 2)` for
    /// `[upper, lower]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.upper;
                out[i * 2 + 1] = o.lower;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== KasePermissionStochastic ==============================

#[pyclass(
    name = "KasePermissionStochastic",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyKasePermissionStochastic {
    inner: wc::KasePermissionStochastic,
}

#[pymethods]
impl PyKasePermissionStochastic {
    #[new]
    #[pyo3(signature = (length=9, smooth=3))]
    fn new(length: usize, smooth: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::KasePermissionStochastic::new(length, smooth).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.fast, o.slow)))
    }
    /// Batch over high/low/close input columns. Returns shape `(n, 2)` for
    /// `[fast, slow]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.fast;
                out[i * 2 + 1] = o.slow;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== TsfOscillator ==============================

#[pyclass(name = "TsfOscillator", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTsfOscillator {
    inner: wc::TsfOscillator,
}

#[pymethods]
impl PyTsfOscillator {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TsfOscillator::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TsfOscillator(period={})", self.inner.period())
    }
}

// ============================== MacdHistogram ==============================

#[pyclass(name = "MacdHistogram", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMacdHistogram {
    inner: wc::MacdHistogram,
}

#[pymethods]
impl PyMacdHistogram {
    #[new]
    #[pyo3(signature = (fast=12, slow=26, signal=9))]
    fn new(fast: usize, slow: usize, signal: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MacdHistogram::new(fast, slow, signal).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (fast, slow, signal) = self.inner.periods();
        format!("MacdHistogram(fast={fast}, slow={slow}, signal={signal})")
    }
}

// ============================== PpoHistogram ==============================

#[pyclass(name = "PpoHistogram", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPpoHistogram {
    inner: wc::PpoHistogram,
}

#[pymethods]
impl PyPpoHistogram {
    #[new]
    #[pyo3(signature = (fast=12, slow=26, signal=9))]
    fn new(fast: usize, slow: usize, signal: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::PpoHistogram::new(fast, slow, signal).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (fast, slow, signal) = self.inner.periods();
        format!("PpoHistogram(fast={fast}, slow={slow}, signal={signal})")
    }
}

// ============================== BipowerVariation ==============================

#[pyclass(
    name = "BipowerVariation",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyBipowerVariation {
    inner: wc::BipowerVariation,
}

#[pymethods]
impl PyBipowerVariation {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::BipowerVariation::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("BipowerVariation(period={})", self.inner.period())
    }
}

// ============================== VolatilityRatio ==============================

#[pyclass(
    name = "VolatilityRatio",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyVolatilityRatio {
    inner: wc::VolatilityRatio,
}

#[pymethods]
impl PyVolatilityRatio {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VolatilityRatio::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("VolatilityRatio(period={})", self.inner.period())
    }
}

// ============================== ProjectionOscillator ==============================

#[pyclass(
    name = "ProjectionOscillator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyProjectionOscillator {
    inner: wc::ProjectionOscillator,
}

#[pymethods]
impl PyProjectionOscillator {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ProjectionOscillator::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ProjectionOscillator(period={})", self.inner.period())
    }
}

// ============================== TimeBasedStop ==============================

#[pyclass(name = "TimeBasedStop", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTimeBasedStop {
    inner: wc::TimeBasedStop,
}

#[pymethods]
impl PyTimeBasedStop {
    #[new]
    #[pyo3(signature = (max_bars=5))]
    fn new(max_bars: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TimeBasedStop::new(max_bars).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    /// Ignores price; counts bars. Returns progress in `[0, 1]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn max_bars(&self) -> usize {
        self.inner.max_bars()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TimeBasedStop(max_bars={})", self.inner.max_bars())
    }
}

// ============================== JarqueBera ==============================

#[pyclass(name = "JARQUEBERA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyJarqueBera {
    inner: wc::JarqueBera,
}

#[pymethods]
impl PyJarqueBera {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::JarqueBera::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("JARQUEBERA(period={})", self.inner.period())
    }
}

// ============================== RollingMinMaxScaler ==============================

#[pyclass(name = "ROLLINGMINMAX", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRollingMinMaxScaler {
    inner: wc::RollingMinMaxScaler,
}

#[pymethods]
impl PyRollingMinMaxScaler {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RollingMinMaxScaler::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ROLLINGMINMAX(period={})", self.inner.period())
    }
}

// ============================== HighpassFilter ==============================

#[pyclass(name = "HIGHPASS", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyHighpassFilter {
    inner: wc::HighpassFilter,
}

#[pymethods]
impl PyHighpassFilter {
    #[new]
    #[pyo3(signature = (period=48))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::HighpassFilter::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("HIGHPASS(period={})", self.inner.period())
    }
}

// ============================== Reflex ==============================

#[pyclass(name = "REFLEX", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyReflex {
    inner: wc::Reflex,
}

#[pymethods]
impl PyReflex {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Reflex::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("REFLEX(period={})", self.inner.period())
    }
}

// ============================== Trendflex ==============================

#[pyclass(name = "TRENDFLEX", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTrendflex {
    inner: wc::Trendflex,
}

#[pymethods]
impl PyTrendflex {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Trendflex::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TRENDFLEX(period={})", self.inner.period())
    }
}

// ============================== CorrelationTrendIndicator ==============================

#[pyclass(name = "CTI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyCorrelationTrendIndicator {
    inner: wc::CorrelationTrendIndicator,
}

#[pymethods]
impl PyCorrelationTrendIndicator {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::CorrelationTrendIndicator::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("CTI(period={})", self.inner.period())
    }
}

// ============================== AdaptiveRsi ==============================

#[pyclass(name = "ADAPTIVERSI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAdaptiveRsi {
    inner: wc::AdaptiveRsi,
}

#[pymethods]
impl PyAdaptiveRsi {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AdaptiveRsi::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ADAPTIVERSI(period={})", self.inner.period())
    }
}

// ============================== UniversalOscillator ==============================

#[pyclass(name = "UNIVERSALOSC", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyUniversalOscillator {
    inner: wc::UniversalOscillator,
}

#[pymethods]
impl PyUniversalOscillator {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::UniversalOscillator::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("UNIVERSALOSC(period={})", self.inner.period())
    }
}

// ============================== AdaptiveCci ==============================

#[pyclass(name = "ADAPTIVECCI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAdaptiveCci {
    inner: wc::AdaptiveCci,
}

#[pymethods]
impl PyAdaptiveCci {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AdaptiveCci::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ADAPTIVECCI(period={})", self.inner.period())
    }
}

// ============================== SterlingRatio ==============================

#[pyclass(name = "SterlingRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySterlingRatio {
    inner: wc::SterlingRatio,
}

#[pymethods]
impl PySterlingRatio {
    #[new]
    #[pyo3(signature = (period=12))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::SterlingRatio::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("SterlingRatio(period={})", self.inner.period())
    }
}

// ============================== BurkeRatio ==============================

#[pyclass(name = "BurkeRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyBurkeRatio {
    inner: wc::BurkeRatio,
}

#[pymethods]
impl PyBurkeRatio {
    #[new]
    #[pyo3(signature = (period=12))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::BurkeRatio::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("BurkeRatio(period={})", self.inner.period())
    }
}

// ============================== MartinRatio ==============================

#[pyclass(name = "MartinRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMartinRatio {
    inner: wc::MartinRatio,
}

#[pymethods]
impl PyMartinRatio {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MartinRatio::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("MartinRatio(period={})", self.inner.period())
    }
}

// ============================== TailRatio ==============================

#[pyclass(name = "TailRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTailRatio {
    inner: wc::TailRatio,
}

#[pymethods]
impl PyTailRatio {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TailRatio::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TailRatio(period={})", self.inner.period())
    }
}

// ============================== KRatio ==============================

#[pyclass(name = "KRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyKRatio {
    inner: wc::KRatio,
}

#[pymethods]
impl PyKRatio {
    #[new]
    #[pyo3(signature = (period=30))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::KRatio::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("KRatio(period={})", self.inner.period())
    }
}

// ============================== CommonSenseRatio ==============================

#[pyclass(
    name = "CommonSenseRatio",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyCommonSenseRatio {
    inner: wc::CommonSenseRatio,
}

#[pymethods]
impl PyCommonSenseRatio {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::CommonSenseRatio::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("CommonSenseRatio(period={})", self.inner.period())
    }
}

// ============================== GainToPainRatio ==============================

#[pyclass(
    name = "GainToPainRatio",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyGainToPainRatio {
    inner: wc::GainToPainRatio,
}

#[pymethods]
impl PyGainToPainRatio {
    #[new]
    #[pyo3(signature = (period=12))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::GainToPainRatio::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("GainToPainRatio(period={})", self.inner.period())
    }
}

// ============================== Stochastic ==============================

#[pyclass(name = "IMI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyImi {
    inner: wc::IntradayMomentumIndex,
}

#[pymethods]
impl PyImi {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::IntradayMomentumIndex::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over open/high/low/close input columns (the IMI needs the open).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let n = o.len();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[pyclass(name = "QQE", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyQqe {
    inner: wc::Qqe,
}

#[pymethods]
impl PyQqe {
    #[new]
    #[pyo3(signature = (rsi_period=14, smoothing=5, factor=4.236))]
    fn new(rsi_period: usize, smoothing: usize, factor: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Qqe::new(rsi_period, smoothing, factor).map_err(map_err)?,
        })
    }
    /// Returns `(rsi_ma, trailing_line)` or `None` during warmup.
    fn update(&mut self, value: f64) -> Option<(f64, f64)> {
        self.inner
            .update(value)
            .map(|o| (o.rsi_ma, o.trailing_line))
    }
    /// Batch over a series of closes. Returns shape `(n, 2)` with columns
    /// `[rsi_ma, trailing_line]`. Warmup rows are NaN.
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 2];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 2] = o.rsi_ma;
                out[i * 2 + 1] = o.trailing_line;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn factor(&self) -> f64 {
        self.inner.factor()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[pyclass(name = "ElderRay", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyElderRay {
    inner: wc::ElderRay,
}

#[pymethods]
impl PyElderRay {
    #[new]
    #[pyo3(signature = (period=13))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ElderRay::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.bull_power, o.bear_power)))
    }
    /// Batch over high/low/close input columns. Returns shape `(n, 2)` for
    /// `[bull_power, bear_power]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.bull_power;
                out[i * 2 + 1] = o.bear_power;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

#[pyclass(name = "Stochastic", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyStoch {
    inner: wc::Stochastic,
}

#[pymethods]
impl PyStoch {
    #[new]
    #[pyo3(signature = (k_period=14, d_period=3))]
    fn new(k_period: usize, d_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Stochastic::new(k_period, d_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.k, o.d)))
    }
    /// Batch over high/low/close input columns. Returns shape `(n, 2)` for `[k, d]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.k;
                out[i * 2 + 1] = o.d;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (k, d) = self.inner.periods();
        format!("Stochastic(k_period={k}, d_period={d})")
    }
}

// ============================== OBV ==============================

#[pyclass(name = "OBV", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyObv {
    inner: wc::Obv,
}

#[pymethods]
impl PyObv {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::Obv::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over close + volume series.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let c = close.as_slice();
        let v = volume.as_slice();
        if c.len() != v.len() {
            return Err(PyValueError::new_err(
                "close and volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], c[i], c[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "OBV()".to_string()
    }
}

// ============================== DEMA ==============================

#[pyclass(name = "DEMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyDema {
    inner: wc::Dema,
}

#[pymethods]
impl PyDema {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Dema::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("DEMA(period={})", self.inner.period())
    }
}

// ============================== TEMA ==============================

#[pyclass(name = "TEMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTema {
    inner: wc::Tema,
}

#[pymethods]
impl PyTema {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Tema::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TEMA(period={})", self.inner.period())
    }
}

// ============================== HMA ==============================

#[pyclass(name = "HMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyHma {
    inner: wc::Hma,
}

#[pymethods]
impl PyHma {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Hma::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("HMA(period={})", self.inner.period())
    }
}

// ============================== KAMA ==============================

#[pyclass(name = "KAMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyKama {
    inner: wc::Kama,
}

#[pymethods]
impl PyKama {
    #[new]
    #[pyo3(signature = (er_period=10, fast=2, slow=30))]
    fn new(er_period: usize, fast: usize, slow: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Kama::new(er_period, fast, slow).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "KAMA".to_string()
    }
}

// ============================== Inertia ==============================

#[pyclass(name = "Inertia", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyInertia {
    inner: wc::Inertia,
}

#[pymethods]
impl PyInertia {
    #[new]
    #[pyo3(signature = (rvi_period=14, linreg_period=20))]
    fn new(rvi_period: usize, linreg_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Inertia::new(rvi_period, linreg_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if !(o.len() == h.len() && h.len() == l.len() && l.len() == c.len()) {
            return Err(PyValueError::new_err(
                "open, high, low and close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (r, l) = self.inner.periods();
        format!("Inertia(rvi_period={r}, linreg_period={l})")
    }
}

// ============================== Connors RSI ==============================

#[pyclass(name = "ConnorsRSI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyConnorsRsi {
    inner: wc::ConnorsRsi,
}

#[pymethods]
impl PyConnorsRsi {
    #[new]
    #[pyo3(signature = (period_rsi=3, period_streak=2, period_rank=100))]
    fn new(period_rsi: usize, period_streak: usize, period_rank: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ConnorsRsi::new(period_rsi, period_streak, period_rank).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (r, s, k) = self.inner.periods();
        format!("ConnorsRSI(period_rsi={r}, period_streak={s}, period_rank={k})")
    }
}

// ============================== Laguerre RSI ==============================

#[pyclass(name = "LaguerreRSI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyLaguerreRsi {
    inner: wc::LaguerreRsi,
}

#[pymethods]
impl PyLaguerreRsi {
    #[new]
    #[pyo3(signature = (gamma=0.5))]
    fn new(gamma: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::LaguerreRsi::new(gamma).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn gamma(&self) -> f64 {
        self.inner.gamma()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("LaguerreRSI(gamma={})", self.inner.gamma())
    }
}

// ============================== SMI ==============================

#[pyclass(name = "SMI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySmi {
    inner: wc::Smi,
}

#[pymethods]
impl PySmi {
    #[new]
    #[pyo3(signature = (period=5, d_period=3, d2_period=3))]
    fn new(period: usize, d_period: usize, d2_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Smi::new(period, d_period, d2_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if !(h.len() == l.len() && l.len() == c.len()) {
            return Err(PyValueError::new_err(
                "high, low and close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, d, d2) = self.inner.periods();
        format!("SMI(period={p}, d_period={d}, d2_period={d2})")
    }
}

// ============================== KST ==============================

#[pyclass(name = "KST", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyKst {
    inner: wc::Kst,
}

#[pymethods]
impl PyKst {
    #[new]
    #[pyo3(signature = (roc1=10, roc2=15, roc3=20, roc4=30, sma1=10, sma2=10, sma3=10, sma4=15, signal=9))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        roc1: usize,
        roc2: usize,
        roc3: usize,
        roc4: usize,
        sma1: usize,
        sma2: usize,
        sma3: usize,
        sma4: usize,
        signal: usize,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Kst::new(roc1, roc2, roc3, roc4, sma1, sma2, sma3, sma4, signal)
                .map_err(map_err)?,
        })
    }
    #[staticmethod]
    fn classic() -> Self {
        Self {
            inner: wc::Kst::classic(),
        }
    }
    fn update(&mut self, value: f64) -> Option<(f64, f64)> {
        self.inner.update(value).map(|o| (o.kst, o.signal))
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 2];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 2] = o.kst;
                out[i * 2 + 1] = o.signal;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "KST".to_string()
    }
}

// ============================== PGO ==============================

#[pyclass(name = "PGO", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPgo {
    inner: wc::Pgo,
}

#[pymethods]
impl PyPgo {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Pgo::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if !(h.len() == l.len() && l.len() == c.len()) {
            return Err(PyValueError::new_err(
                "high, low and close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("PGO(period={})", self.inner.period())
    }
}

// ============================== RVI ==============================

#[pyclass(name = "RVI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRvi {
    inner: wc::Rvi,
}

#[pymethods]
impl PyRvi {
    #[new]
    #[pyo3(signature = (period=10))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Rvi::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if !(o.len() == h.len() && h.len() == l.len() && l.len() == c.len()) {
            return Err(PyValueError::new_err(
                "open, high, low and close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RVI(period={})", self.inner.period())
    }
}

// ============================== FRAMA ==============================

#[pyclass(name = "FRAMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFrama {
    inner: wc::Frama,
}

#[pymethods]
impl PyFrama {
    #[new]
    #[pyo3(signature = (period=16))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Frama::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("FRAMA(period={})", self.inner.period())
    }
}

// ============================== EVWMA ==============================

#[pyclass(name = "EVWMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyEvwma {
    inner: wc::Evwma,
}

#[pymethods]
impl PyEvwma {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Evwma::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let c = close.as_slice();
        let v = volume.as_slice();
        if c.len() != v.len() {
            return Err(PyValueError::new_err(
                "close and volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], c[i], c[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("EVWMA(period={})", self.inner.period())
    }
}

// ============================== Alligator ==============================

#[pyclass(name = "Alligator", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAlligator {
    inner: wc::Alligator,
}

#[pymethods]
impl PyAlligator {
    #[new]
    #[pyo3(signature = (jaw=13, teeth=8, lips=5))]
    fn new(jaw: usize, teeth: usize, lips: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Alligator::new(jaw, teeth, lips).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.jaw, o.teeth, o.lips)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.jaw;
                out[i * 3 + 1] = o.teeth;
                out[i * 3 + 2] = o.lips;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (j, t, l) = self.inner.periods();
        format!("Alligator(jaw={j}, teeth={t}, lips={l})")
    }
}

// ============================== JMA ==============================

#[pyclass(name = "JMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyJma {
    inner: wc::Jma,
}

#[pymethods]
impl PyJma {
    #[new]
    #[pyo3(signature = (period=14, phase=0.0, power=2))]
    fn new(period: usize, phase: f64, power: u32) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Jma::new(period, phase, power).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, ph, pw) = self.inner.params();
        format!("JMA(period={p}, phase={ph}, power={pw})")
    }
}

// ============================== VIDYA ==============================

#[pyclass(name = "VIDYA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyVidya {
    inner: wc::Vidya,
}

#[pymethods]
impl PyVidya {
    #[new]
    #[pyo3(signature = (period=14, cmo_period=9))]
    fn new(period: usize, cmo_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Vidya::new(period, cmo_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, c) = self.inner.periods();
        format!("VIDYA(period={p}, cmo_period={c})")
    }
}

// ============================== McGinley Dynamic ==============================

#[pyclass(
    name = "McGinleyDynamic",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyMcGinleyDynamic {
    inner: wc::McGinleyDynamic,
}

#[pymethods]
impl PyMcGinleyDynamic {
    #[new]
    #[pyo3(signature = (period=10))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::McGinleyDynamic::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("McGinleyDynamic(period={})", self.inner.period())
    }
}

// ============================== ALMA ==============================

#[pyclass(name = "ALMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAlma {
    inner: wc::Alma,
}

#[pymethods]
impl PyAlma {
    #[new]
    #[pyo3(signature = (period=9, offset=0.85, sigma=6.0))]
    fn new(period: usize, offset: f64, sigma: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Alma::new(period, offset, sigma).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn offset(&self) -> f64 {
        self.inner.offset()
    }
    #[getter]
    fn sigma(&self) -> f64 {
        self.inner.sigma()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "ALMA(period={}, offset={}, sigma={})",
            self.inner.period(),
            self.inner.offset(),
            self.inner.sigma()
        )
    }
}

// ============================== AwesomeOscillatorHistogram ==============================

#[pyclass(
    name = "AwesomeOscillatorHistogram",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAoHist {
    inner: wc::AwesomeOscillatorHistogram,
}

#[pymethods]
impl PyAoHist {
    #[new]
    #[pyo3(signature = (fast=5, slow=34, sma_period=5))]
    fn new(fast: usize, slow: usize, sma_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AwesomeOscillatorHistogram::new(fast, slow, sma_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (f, s, k) = self.inner.periods();
        format!("AwesomeOscillatorHistogram(fast={f}, slow={s}, sma_period={k})")
    }
}

// ============================== STC ==============================

#[pyclass(name = "STC", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyStc {
    inner: wc::Stc,
}

#[pymethods]
impl PyStc {
    #[new]
    #[pyo3(signature = (fast=23, slow=50, schaff_period=10, factor=0.5))]
    fn new(fast: usize, slow: usize, schaff_period: usize, factor: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Stc::new(fast, slow, schaff_period, factor).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (f, s, p, k) = self.inner.params();
        format!("STC(fast={f}, slow={s}, schaff_period={p}, factor={k})")
    }
}

// ============================== ElderImpulse ==============================

#[pyclass(name = "ElderImpulse", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyElderImpulse {
    inner: wc::ElderImpulse,
}

#[pymethods]
impl PyElderImpulse {
    #[new]
    #[pyo3(signature = (ema_period=13, macd_fast=12, macd_slow=26, macd_signal=9))]
    fn new(
        ema_period: usize,
        macd_fast: usize,
        macd_slow: usize,
        macd_signal: usize,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ElderImpulse::new(ema_period, macd_fast, macd_slow, macd_signal)
                .map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (e, f, s, sig) = self.inner.periods();
        format!("ElderImpulse(ema_period={e}, macd_fast={f}, macd_slow={s}, macd_signal={sig})")
    }
}

// ============================== ZeroLagMACD ==============================

#[pyclass(name = "ZeroLagMACD", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyZeroLagMacd {
    inner: wc::ZeroLagMacd,
}

#[pymethods]
impl PyZeroLagMacd {
    #[new]
    #[pyo3(signature = (fast=12, slow=26, signal=9))]
    fn new(fast: usize, slow: usize, signal: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ZeroLagMacd::new(fast, slow, signal).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update(value)
            .map(|o| (o.macd, o.signal, o.histogram))
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.macd;
                out[i * 3 + 1] = o.signal;
                out[i * 3 + 2] = o.histogram;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (f, s, sig) = self.inner.periods();
        format!("ZeroLagMACD(fast={f}, slow={s}, signal={sig})")
    }
}

// ============================== CFO ==============================

#[pyclass(name = "CFO", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyCfo {
    inner: wc::Cfo,
}

#[pymethods]
impl PyCfo {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Cfo::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("CFO(period={})", self.inner.period())
    }
}

// ============================== APO ==============================

#[pyclass(name = "APO", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyApo {
    inner: wc::Apo,
}

#[pymethods]
impl PyApo {
    #[new]
    #[pyo3(signature = (fast=12, slow=26))]
    fn new(fast: usize, slow: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Apo::new(fast, slow).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (f, s) = self.inner.periods();
        format!("APO(fast={f}, slow={s})")
    }
}

// ============================== CCI ==============================

#[pyclass(name = "CCI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyCci {
    inner: wc::Cci,
}

#[pymethods]
impl PyCci {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Cci::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("CCI(period={})", self.inner.period())
    }
}

// ============================== ROC ==============================

#[pyclass(name = "ROC", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRoc {
    inner: wc::Roc,
}

#[pymethods]
impl PyRoc {
    #[new]
    #[pyo3(signature = (period=10))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Roc::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ROC(period={})", self.inner.period())
    }
}

// ============================== Williams %R ==============================

#[pyclass(name = "WilliamsR", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyWilliamsR {
    inner: wc::WilliamsR,
}

#[pymethods]
impl PyWilliamsR {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::WilliamsR::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== ADX ==============================

#[pyclass(name = "ADX", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAdx {
    inner: wc::Adx,
}

#[pymethods]
impl PyAdx {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Adx::new(period).map_err(map_err)?,
        })
    }
    /// Returns `(plus_di, minus_di, adx)` or None during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.plus_di, o.minus_di, o.adx)))
    }
    /// Batch returns shape `(n, 3)`: `[plus_di, minus_di, adx]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.plus_di;
                out[i * 3 + 1] = o.minus_di;
                out[i * 3 + 2] = o.adx;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== ADXR ==============================

#[pyclass(name = "ADXR", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAdxr {
    inner: wc::Adxr,
}

#[pymethods]
impl PyAdxr {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Adxr::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(v) = self.inner.update(candle) {
                out[i] = v;
            }
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ADXR(period={})", self.inner.period())
    }
}

// ============================== MFI ==============================

#[pyclass(name = "MFI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMfi {
    inner: wc::Mfi,
}

#[pymethods]
impl PyMfi {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Mfi::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== TRIX ==============================

#[pyclass(name = "TRIX", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTrix {
    inner: wc::Trix,
}

#[pymethods]
impl PyTrix {
    #[new]
    #[pyo3(signature = (period=30))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Trix::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== PSAR ==============================

#[pyclass(name = "PSAR", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPsar {
    inner: wc::Psar,
}

#[pymethods]
impl PyPsar {
    #[new]
    #[pyo3(signature = (af_start=0.02, af_step=0.02, af_max=0.20))]
    fn new(af_start: f64, af_step: f64, af_max: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Psar::new(af_start, af_step, af_max).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Keltner Channels ==============================

#[pyclass(name = "Keltner", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyKeltner {
    inner: wc::Keltner,
}

#[pymethods]
impl PyKeltner {
    #[new]
    #[pyo3(signature = (ema_period=20, atr_period=10, multiplier=2.0))]
    fn new(ema_period: usize, atr_period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Keltner::new(ema_period, atr_period, multiplier).map_err(map_err)?,
        })
    }
    /// Returns `(upper, middle, lower)` or None during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.upper, o.middle, o.lower)))
    }
    /// Returns shape `(n, 3)` for `[upper, middle, lower]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Donchian Channels ==============================

#[pyclass(name = "Donchian", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyDonchian {
    inner: wc::Donchian,
}

#[pymethods]
impl PyDonchian {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Donchian::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.upper, o.middle, o.lower)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== VWAP ==============================

#[pyclass(name = "VWAP", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyVwap {
    inner: wc::Vwap,
}

#[pymethods]
impl PyVwap {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::Vwap::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Rolling VWAP ==============================

#[pyclass(name = "RollingVWAP", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRollingVwap {
    inner: wc::RollingVwap,
}

#[pymethods]
impl PyRollingVwap {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RollingVwap::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RollingVWAP(period={})", self.inner.period())
    }
}

// ============================== Awesome Oscillator ==============================

#[pyclass(
    name = "AwesomeOscillator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAo {
    inner: wc::AwesomeOscillator,
}

#[pymethods]
impl PyAo {
    #[new]
    #[pyo3(signature = (fast=5, slow=34))]
    fn new(fast: usize, slow: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AwesomeOscillator::new(fast, slow).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Aroon ==============================

#[pyclass(name = "Aroon", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAroon {
    inner: wc::Aroon,
}

#[pymethods]
impl PyAroon {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Aroon::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.up, o.down)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.up;
                out[i * 2 + 1] = o.down;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== ADL ==============================

#[pyclass(name = "ADL", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAdl {
    inner: wc::Adl,
}

#[pymethods]
impl PyAdl {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::Adl::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close, volume (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "ADL()".to_string()
    }
}

// ============================== Volume-Price Trend ==============================

#[pyclass(
    name = "VolumePriceTrend",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyVolumePriceTrend {
    inner: wc::VolumePriceTrend,
}

#[pymethods]
impl PyVolumePriceTrend {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::VolumePriceTrend::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over close + volume series (both 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let c = close.as_slice();
        let v = volume.as_slice();
        if c.len() != v.len() {
            return Err(PyValueError::new_err(
                "close and volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], c[i], c[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "VolumePriceTrend()".to_string()
    }
}

// ============================== Bollinger Bandwidth ==============================

#[pyclass(
    name = "BollingerBandwidth",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyBollingerBandwidth {
    inner: wc::BollingerBandwidth,
}

#[pymethods]
impl PyBollingerBandwidth {
    #[new]
    #[pyo3(signature = (period=20, multiplier=2.0))]
    fn new(period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::BollingerBandwidth::new(period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn multiplier(&self) -> f64 {
        self.inner.multiplier()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "BollingerBandwidth(period={}, multiplier={})",
            self.inner.period(),
            self.inner.multiplier()
        )
    }
}

// ============================== Percent B ==============================

#[pyclass(name = "PercentB", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPercentB {
    inner: wc::PercentB,
}

#[pymethods]
impl PyPercentB {
    #[new]
    #[pyo3(signature = (period=20, multiplier=2.0))]
    fn new(period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::PercentB::new(period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn multiplier(&self) -> f64 {
        self.inner.multiplier()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "PercentB(period={}, multiplier={})",
            self.inner.period(),
            self.inner.multiplier()
        )
    }
}

// ============================== NATR ==============================

#[pyclass(name = "NATR", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyNatr {
    inner: wc::Natr,
}

#[pymethods]
impl PyNatr {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Natr::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("NATR(period={})", self.inner.period())
    }
}

// ============================== StdDev ==============================

#[pyclass(name = "StdDev", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyStdDev {
    inner: wc::StdDev,
}

#[pymethods]
impl PyStdDev {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::StdDev::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("StdDev(period={})", self.inner.period())
    }
}

// ============================== Ulcer Index ==============================

#[pyclass(name = "UlcerIndex", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyUlcerIndex {
    inner: wc::UlcerIndex,
}

#[pymethods]
impl PyUlcerIndex {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::UlcerIndex::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("UlcerIndex(period={})", self.inner.period())
    }
}

// ============================== Historical Volatility ==============================

#[pyclass(
    name = "HistoricalVolatility",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyHistoricalVolatility {
    inner: wc::HistoricalVolatility,
}

#[pymethods]
impl PyHistoricalVolatility {
    #[new]
    #[pyo3(signature = (period=20, trading_periods=252))]
    fn new(period: usize, trading_periods: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::HistoricalVolatility::new(period, trading_periods).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, t) = self.inner.periods();
        format!("HistoricalVolatility(period={p}, trading_periods={t})")
    }
}

// ============================== Aroon Oscillator ==============================

#[pyclass(
    name = "AroonOscillator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAroonOscillator {
    inner: wc::AroonOscillator,
}

#[pymethods]
impl PyAroonOscillator {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AroonOscillator::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high + low columns (both 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("AroonOscillator(period={})", self.inner.period())
    }
}

// ============================== Vortex ==============================

#[pyclass(name = "Vortex", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyVortex {
    inner: wc::Vortex,
}

#[pymethods]
impl PyVortex {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Vortex::new(period).map_err(map_err)?,
        })
    }
    /// Returns `(plus, minus)` or `None` during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.plus, o.minus)))
    }
    /// Batch over high/low/close input columns. Returns shape `(n, 2)` for `[plus, minus]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.plus;
                out[i * 2 + 1] = o.minus;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("Vortex(period={})", self.inner.period())
    }
}

// ============================== RWI ==============================

#[pyclass(name = "RWI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRwi {
    inner: wc::Rwi,
}

#[pymethods]
impl PyRwi {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Rwi::new(period).map_err(map_err)?,
        })
    }
    /// Returns `(high, low)` or `None` during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.high, o.low)))
    }
    /// Batch over high/low/close input columns. Returns shape `(n, 2)` for `[high, low]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.high;
                out[i * 2 + 1] = o.low;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RWI(period={})", self.inner.period())
    }
}

// ============================== WaveTrend ==============================

#[pyclass(name = "WaveTrend", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyWaveTrend {
    inner: wc::WaveTrend,
}

#[pymethods]
impl PyWaveTrend {
    #[new]
    #[pyo3(signature = (channel_period=10, average_period=21, signal_period=4))]
    fn new(channel_period: usize, average_period: usize, signal_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::WaveTrend::new(channel_period, average_period, signal_period)
                .map_err(map_err)?,
        })
    }
    #[staticmethod]
    fn classic() -> PyResult<Self> {
        Ok(Self {
            inner: wc::WaveTrend::classic().map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.wt1, o.wt2)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.wt1;
                out[i * 2 + 1] = o.wt2;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn periods(&self) -> (usize, usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (cp, ap, sp) = self.inner.periods();
        format!("WaveTrend(channel_period={cp}, average_period={ap}, signal_period={sp})")
    }
}

// ============================== Mass Index ==============================

#[pyclass(name = "MassIndex", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMassIndex {
    inner: wc::MassIndex,
}

#[pymethods]
impl PyMassIndex {
    #[new]
    #[pyo3(signature = (ema_period=9, sum_period=25))]
    fn new(ema_period: usize, sum_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MassIndex::new(ema_period, sum_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high + low columns (both 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (e, s) = self.inner.periods();
        format!("MassIndex(ema_period={e}, sum_period={s})")
    }
}

// ============================== PPO ==============================

#[pyclass(name = "PPO", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPpo {
    inner: wc::Ppo,
}

#[pymethods]
impl PyPpo {
    #[new]
    #[pyo3(signature = (fast=12, slow=26))]
    fn new(fast: usize, slow: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Ppo::new(fast, slow).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (f, s) = self.inner.periods();
        format!("PPO(fast={f}, slow={s})")
    }
}

// ============================== DPO ==============================

#[pyclass(name = "DPO", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyDpo {
    inner: wc::Dpo,
}

#[pymethods]
impl PyDpo {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Dpo::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn shift(&self) -> usize {
        self.inner.shift()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("DPO(period={})", self.inner.period())
    }
}

// ============================== Coppock ==============================

#[pyclass(name = "Coppock", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyCoppock {
    inner: wc::Coppock,
}

#[pymethods]
impl PyCoppock {
    #[new]
    #[pyo3(signature = (roc_long=14, roc_short=11, wma_period=10))]
    fn new(roc_long: usize, roc_short: usize, wma_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Coppock::new(roc_long, roc_short, wma_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn periods(&self) -> (usize, usize, usize) {
        self.inner.periods()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (l, s, w) = self.inner.periods();
        format!("Coppock(roc_long={l}, roc_short={s}, wma_period={w})")
    }
}

// ============================== StochRSI ==============================

#[pyclass(name = "StochRSI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyStochRsi {
    inner: wc::StochRsi,
}

#[pymethods]
impl PyStochRsi {
    #[new]
    #[pyo3(signature = (rsi_period=14, stoch_period=14))]
    fn new(rsi_period: usize, stoch_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::StochRsi::new(rsi_period, stoch_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (r, s) = self.inner.periods();
        format!("StochRSI(rsi_period={r}, stoch_period={s})")
    }
}

// ============================== Ultimate Oscillator ==============================

#[pyclass(
    name = "UltimateOscillator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyUltimateOscillator {
    inner: wc::UltimateOscillator,
}

#[pymethods]
impl PyUltimateOscillator {
    #[new]
    #[pyo3(signature = (short=7, mid=14, long=28))]
    fn new(short: usize, mid: usize, long: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::UltimateOscillator::new(short, mid, long).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn periods(&self) -> (usize, usize, usize) {
        self.inner.periods()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (s, m, l) = self.inner.periods();
        format!("UltimateOscillator(short={s}, mid={m}, long={l})")
    }
}

// ============================== MOM ==============================

#[pyclass(name = "MOM", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMom {
    inner: wc::Mom,
}

#[pymethods]
impl PyMom {
    #[new]
    #[pyo3(signature = (period=10))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Mom::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("MOM(period={})", self.inner.period())
    }
}

// ============================== CMO ==============================

#[pyclass(name = "CMO", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyCmo {
    inner: wc::Cmo,
}

#[pymethods]
impl PyCmo {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Cmo::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("CMO(period={})", self.inner.period())
    }
}

// ============================== TSI ==============================

#[pyclass(name = "TSI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTsi {
    inner: wc::Tsi,
}

#[pymethods]
impl PyTsi {
    #[new]
    #[pyo3(signature = (long=25, short=13))]
    fn new(long: usize, short: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Tsi::new(long, short).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (l, s) = self.inner.periods();
        format!("TSI(long={l}, short={s})")
    }
}

// ============================== PMO ==============================

#[pyclass(name = "PMO", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPmo {
    inner: wc::Pmo,
}

#[pymethods]
impl PyPmo {
    #[new]
    #[pyo3(signature = (smoothing1=35, smoothing2=20))]
    fn new(smoothing1: usize, smoothing2: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Pmo::new(smoothing1, smoothing2).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (s1, s2) = self.inner.periods();
        format!("PMO(smoothing1={s1}, smoothing2={s2})")
    }
}

// ============================== TII ==============================

#[pyclass(name = "TII", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTii {
    inner: wc::Tii,
}

#[pymethods]
impl PyTii {
    #[new]
    #[pyo3(signature = (sma_period=60, dev_period=30))]
    fn new(sma_period: usize, dev_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Tii::new(sma_period, dev_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (s, d) = self.inner.periods();
        format!("TII(sma_period={s}, dev_period={d})")
    }
}

// ============================== ZLEMA ==============================

#[pyclass(name = "ZLEMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyZlema {
    inner: wc::Zlema,
}

#[pymethods]
impl PyZlema {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Zlema::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn lag(&self) -> usize {
        self.inner.lag()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ZLEMA(period={})", self.inner.period())
    }
}

// ============================== T3 ==============================

#[pyclass(name = "T3", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyT3 {
    inner: wc::T3,
}

#[pymethods]
impl PyT3 {
    #[new]
    #[pyo3(signature = (period, v=0.7))]
    fn new(period: usize, v: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::T3::new(period, v).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn volume_factor(&self) -> f64 {
        self.inner.volume_factor()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "T3(period={}, v={})",
            self.inner.period(),
            self.inner.volume_factor()
        )
    }
}

// ============================== GD ==============================

#[pyclass(name = "GD", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyGeneralizedDema {
    inner: wc::GeneralizedDema,
}

#[pymethods]
impl PyGeneralizedDema {
    #[new]
    #[pyo3(signature = (period, v=0.7))]
    fn new(period: usize, v: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::GeneralizedDema::new(period, v).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn volume_factor(&self) -> f64 {
        self.inner.volume_factor()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "GD(period={}, v={})",
            self.inner.period(),
            self.inner.volume_factor()
        )
    }
}

// ============================== HoltWinters ==============================

#[pyclass(name = "HoltWinters", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyHoltWinters {
    inner: wc::HoltWinters,
}

#[pymethods]
impl PyHoltWinters {
    #[new]
    #[pyo3(signature = (alpha=0.2, beta=0.1))]
    fn new(alpha: f64, beta: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::HoltWinters::new(alpha, beta).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn alpha(&self) -> f64 {
        self.inner.alpha()
    }
    #[getter]
    fn beta(&self) -> f64 {
        self.inner.beta()
    }
    #[getter]
    fn level(&self) -> Option<f64> {
        self.inner.level()
    }
    #[getter]
    fn trend(&self) -> Option<f64> {
        self.inner.trend()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "HoltWinters(alpha={}, beta={})",
            self.inner.alpha(),
            self.inner.beta()
        )
    }
}

// ============================== RMI ==============================

#[pyclass(name = "RMI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRmi {
    inner: wc::Rmi,
}

#[pymethods]
impl PyRmi {
    #[new]
    #[pyo3(signature = (period, momentum))]
    fn new(period: usize, momentum: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Rmi::new(period, momentum).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn momentum(&self) -> usize {
        self.inner.momentum()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "RMI(period={}, momentum={})",
            self.inner.period(),
            self.inner.momentum()
        )
    }
}

// ============================== DerivativeOscillator ==============================

#[pyclass(
    name = "DerivativeOscillator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyDerivativeOscillator {
    inner: wc::DerivativeOscillator,
}

#[pymethods]
impl PyDerivativeOscillator {
    #[new]
    #[pyo3(signature = (rsi_period=14, smooth1=5, smooth2=3, signal_period=9))]
    fn new(
        rsi_period: usize,
        smooth1: usize,
        smooth2: usize,
        signal_period: usize,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: wc::DerivativeOscillator::new(rsi_period, smooth1, smooth2, signal_period)
                .map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== VWMA ==============================

#[pyclass(name = "VWMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyVwma {
    inner: wc::Vwma,
}

#[pymethods]
impl PyVwma {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Vwma::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over close + volume series (both 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let c = close.as_slice();
        let v = volume.as_slice();
        if c.len() != v.len() {
            return Err(PyValueError::new_err(
                "close and volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], c[i], c[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("VWMA(period={})", self.inner.period())
    }
}

// ============================== SMMA ==============================

#[pyclass(name = "SMMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySmma {
    inner: wc::Smma,
}

#[pymethods]
impl PySmma {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Smma::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("SMMA(period={})", self.inner.period())
    }
}

// ============================== TRIMA ==============================

#[pyclass(name = "TRIMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTrima {
    inner: wc::Trima,
}

#[pymethods]
impl PyTrima {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Trima::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TRIMA(period={})", self.inner.period())
    }
}

// ============================== Chaikin Money Flow ==============================

#[pyclass(
    name = "ChaikinMoneyFlow",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyChaikinMoneyFlow {
    inner: wc::ChaikinMoneyFlow,
}

#[pymethods]
impl PyChaikinMoneyFlow {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ChaikinMoneyFlow::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close, volume (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ChaikinMoneyFlow(period={})", self.inner.period())
    }
}

// ============================== Chaikin Oscillator ==============================

#[pyclass(
    name = "ChaikinOscillator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyChaikinOscillator {
    inner: wc::ChaikinOscillator,
}

#[pymethods]
impl PyChaikinOscillator {
    #[new]
    #[pyo3(signature = (fast=3, slow=10))]
    fn new(fast: usize, slow: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ChaikinOscillator::new(fast, slow).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close, volume (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (fast, slow) = self.inner.periods();
        format!("ChaikinOscillator(fast={fast}, slow={slow})")
    }
}

// ============================== Force Index ==============================

#[pyclass(name = "ForceIndex", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyForceIndex {
    inner: wc::ForceIndex,
}

#[pymethods]
impl PyForceIndex {
    #[new]
    #[pyo3(signature = (period=13))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ForceIndex::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over close + volume series (both 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let c = close.as_slice();
        let v = volume.as_slice();
        if c.len() != v.len() {
            return Err(PyValueError::new_err(
                "close and volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], c[i], c[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ForceIndex(period={})", self.inner.period())
    }
}

// ============================== Negative Volume Index ==============================

#[pyclass(name = "NVI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyNvi {
    inner: wc::Nvi,
}

#[pymethods]
impl PyNvi {
    #[new]
    #[pyo3(signature = (baseline=1000.0))]
    fn new(baseline: f64) -> Self {
        Self {
            inner: wc::Nvi::with_baseline(baseline),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over close + volume series.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let c = close.as_slice();
        let v = volume.as_slice();
        if c.len() != v.len() {
            return Err(PyValueError::new_err(
                "close and volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], c[i], c[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "NVI()".to_string()
    }
}

// ============================== Positive Volume Index ==============================

#[pyclass(name = "PVI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPvi {
    inner: wc::Pvi,
}

#[pymethods]
impl PyPvi {
    #[new]
    #[pyo3(signature = (baseline=1000.0))]
    fn new(baseline: f64) -> Self {
        Self {
            inner: wc::Pvi::with_baseline(baseline),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let c = close.as_slice();
        let v = volume.as_slice();
        if c.len() != v.len() {
            return Err(PyValueError::new_err(
                "close and volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], c[i], c[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "PVI()".to_string()
    }
}

// ============================== Volume Oscillator ==============================

#[pyclass(
    name = "VolumeOscillator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyVolumeOscillator {
    inner: wc::VolumeOscillator,
}

#[pymethods]
impl PyVolumeOscillator {
    #[new]
    #[pyo3(signature = (fast=14, slow=28))]
    fn new(fast: usize, slow: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VolumeOscillator::new(fast, slow).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over a 1-D volume series.
    fn batch<'py>(&mut self, py: Python<'py>, volume: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let v = volume.as_slice();
        let mut out = Vec::with_capacity(v.len());
        for &vol in v {
            let candle = wc::Candle::new(10.0, 10.0, 10.0, 10.0, vol, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (fast, slow) = self.inner.periods();
        format!("VolumeOscillator(fast={fast}, slow={slow})")
    }
}

// ============================== Klinger Volume Oscillator ==============================

#[pyclass(name = "KVO", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyKvo {
    inner: wc::Kvo,
}

#[pymethods]
impl PyKvo {
    #[new]
    #[pyo3(signature = (fast=34, slow=55))]
    fn new(fast: usize, slow: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Kvo::new(fast, slow).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high/low/close/volume input columns.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (fast, slow) = self.inner.periods();
        format!("KVO(fast={fast}, slow={slow})")
    }
}

// ============================== Williams A/D Oscillator ==============================

#[pyclass(name = "ADOSC", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAdOscillator {
    inner: wc::AdOscillator,
}

#[pymethods]
impl PyAdOscillator {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::AdOscillator::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high/low/close input columns.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "ADOSC()".to_string()
    }
}

// ============================== Anchored RSI ==============================

#[pyclass(name = "AnchoredRSI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAnchoredRsi {
    inner: wc::AnchoredRsi,
}

#[pymethods]
impl PyAnchoredRsi {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::AnchoredRsi::new(),
        }
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    /// Re-anchor the cumulative window at the next bar that arrives.
    fn set_anchor(&mut self) {
        self.inner.set_anchor();
    }
    /// Batch over a close-price column.
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "AnchoredRSI()".to_string()
    }
}

// ============================== Anchored VWAP ==============================

#[pyclass(name = "AnchoredVWAP", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAnchoredVwap {
    inner: wc::AnchoredVwap,
}

#[pymethods]
impl PyAnchoredVwap {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::AnchoredVwap::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Re-anchor the cumulative window at the next bar that arrives.
    fn set_anchor(&mut self) {
        self.inner.set_anchor();
    }
    /// Batch over high/low/close/volume input columns.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "AnchoredVWAP()".to_string()
    }
}

// ============================== Demand Index ==============================

#[pyclass(name = "DemandIndex", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyDemandIndex {
    inner: wc::DemandIndex,
}

#[pymethods]
impl PyDemandIndex {
    #[new]
    #[pyo3(signature = (period=10))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::DemandIndex::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high/low/close/volume input columns.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("DemandIndex(period={})", self.inner.period())
    }
}

// ============================== Time Segmented Volume ==============================

#[pyclass(name = "TSV", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTsv {
    inner: wc::Tsv,
}

#[pymethods]
impl PyTsv {
    #[new]
    #[pyo3(signature = (period=18))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Tsv::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over close + volume input columns.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let c = close.as_slice();
        let v = volume.as_slice();
        if c.len() != v.len() {
            return Err(PyValueError::new_err(
                "close and volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], c[i], c[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TSV(period={})", self.inner.period())
    }
}

// ============================== Volume Zone Oscillator ==============================

#[pyclass(name = "VZO", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyVzo {
    inner: wc::Vzo,
}

#[pymethods]
impl PyVzo {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Vzo::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over close + volume input columns.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let c = close.as_slice();
        let v = volume.as_slice();
        if c.len() != v.len() {
            return Err(PyValueError::new_err(
                "close and volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], c[i], c[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("VZO(period={})", self.inner.period())
    }
}

// ============================== Market Facilitation Index ==============================

#[pyclass(
    name = "MarketFacilitationIndex",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyMarketFacilitationIndex {
    inner: wc::MarketFacilitationIndex,
}

#[pymethods]
impl PyMarketFacilitationIndex {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::MarketFacilitationIndex::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high/low/volume input columns.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "MarketFacilitationIndex()".to_string()
    }
}

// ============================== Ease of Movement ==============================

#[pyclass(
    name = "EaseOfMovement",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyEaseOfMovement {
    inner: wc::EaseOfMovement,
}

#[pymethods]
impl PyEaseOfMovement {
    #[new]
    #[pyo3(signature = (period=14, divisor=100_000_000.0))]
    fn new(period: usize, divisor: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::EaseOfMovement::with_divisor(period, divisor).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, volume (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn divisor(&self) -> f64 {
        self.inner.divisor()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "EaseOfMovement(period={}, divisor={})",
            self.inner.period(),
            self.inner.divisor()
        )
    }
}

// ============================== SuperTrend ==============================

#[pyclass(name = "SuperTrend", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySuperTrend {
    inner: wc::SuperTrend,
}

#[pymethods]
impl PySuperTrend {
    #[new]
    #[pyo3(signature = (atr_period=10, multiplier=3.0))]
    fn new(atr_period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::SuperTrend::new(atr_period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.value, o.direction)))
    }
    /// Batch over input columns high, low, close. Returns shape `(n, 2)` with
    /// columns `[value, direction]`; warmup rows are `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn params(&self) -> (usize, f64) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (atr_period, multiplier) = self.inner.params();
        format!("SuperTrend(atr_period={atr_period}, multiplier={multiplier})")
    }
}

// ============================== Chandelier Exit ==============================

#[pyclass(
    name = "ChandelierExit",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyChandelierExit {
    inner: wc::ChandelierExit,
}

#[pymethods]
impl PyChandelierExit {
    #[new]
    #[pyo3(signature = (period=22, multiplier=3.0))]
    fn new(period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ChandelierExit::new(period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.long_stop, o.short_stop)))
    }
    /// Batch over input columns high, low, close. Returns shape `(n, 2)` with
    /// columns `[long_stop, short_stop]`; warmup rows are `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.long_stop;
                out[i * 2 + 1] = o.short_stop;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn params(&self) -> (usize, f64) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (period, multiplier) = self.inner.params();
        format!("ChandelierExit(period={period}, multiplier={multiplier})")
    }
}

// ============================== Chande Kroll Stop ==============================

#[pyclass(
    name = "ChandeKrollStop",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyChandeKrollStop {
    inner: wc::ChandeKrollStop,
}

#[pymethods]
impl PyChandeKrollStop {
    #[new]
    #[pyo3(signature = (atr_period=10, atr_multiplier=1.0, stop_period=9))]
    fn new(atr_period: usize, atr_multiplier: f64, stop_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ChandeKrollStop::new(atr_period, atr_multiplier, stop_period)
                .map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.stop_long, o.stop_short)))
    }
    /// Batch over input columns high, low, close. Returns shape `(n, 2)` with
    /// columns `[stop_long, stop_short]`; warmup rows are `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.stop_long;
                out[i * 2 + 1] = o.stop_short;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn params(&self) -> (usize, f64, usize) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (atr_period, atr_multiplier, stop_period) = self.inner.params();
        format!(
            "ChandeKrollStop(atr_period={atr_period}, atr_multiplier={atr_multiplier}, stop_period={stop_period})"
        )
    }
}

// ============================== ATR Trailing Stop ==============================

#[pyclass(
    name = "AtrTrailingStop",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAtrTrailingStop {
    inner: wc::AtrTrailingStop,
}

#[pymethods]
impl PyAtrTrailingStop {
    #[new]
    #[pyo3(signature = (atr_period=14, multiplier=3.0))]
    fn new(atr_period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AtrTrailingStop::new(atr_period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns high, low, close (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn params(&self) -> (usize, f64) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (atr_period, multiplier) = self.inner.params();
        format!("AtrTrailingStop(atr_period={atr_period}, multiplier={multiplier})")
    }
}

// ============================== HiLo Activator ==============================

#[pyclass(name = "HiLoActivator", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyHiLoActivator {
    inner: wc::HiLoActivator,
}

#[pymethods]
impl PyHiLoActivator {
    #[new]
    #[pyo3(signature = (period=3))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::HiLoActivator::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("HiLoActivator(period={})", self.inner.period())
    }
}

// ============================== Volty Stop ==============================

#[pyclass(name = "VoltyStop", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyVoltyStop {
    inner: wc::VoltyStop,
}

#[pymethods]
impl PyVoltyStop {
    #[new]
    #[pyo3(signature = (atr_period=14, multiplier=2.0))]
    fn new(atr_period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VoltyStop::new(atr_period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn params(&self) -> (usize, f64) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, m) = self.inner.params();
        format!("VoltyStop(atr_period={p}, multiplier={m})")
    }
}

// ============================== Yo-Yo Exit ==============================

#[pyclass(name = "YoyoExit", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyYoyoExit {
    inner: wc::YoyoExit,
}

#[pymethods]
impl PyYoyoExit {
    #[new]
    #[pyo3(signature = (atr_period=14, multiplier=2.0))]
    fn new(atr_period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::YoyoExit::new(atr_period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn params(&self) -> (usize, f64) {
        self.inner.params()
    }
    #[getter]
    fn in_trade(&self) -> bool {
        self.inner.in_trade()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, m) = self.inner.params();
        format!("YoyoExit(atr_period={p}, multiplier={m})")
    }
}

// ============================== Donchian Stop ==============================

#[pyclass(name = "DonchianStop", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyDonchianStop {
    inner: wc::DonchianStop,
}

#[pymethods]
impl PyDonchianStop {
    #[new]
    #[pyo3(signature = (period=10))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::DonchianStop::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.stop_long, o.stop_short)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.stop_long;
                out[i * 2 + 1] = o.stop_short;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("DonchianStop(period={})", self.inner.period())
    }
}

// ============================== Percentage Trailing Stop ==============================

#[pyclass(
    name = "PercentageTrailingStop",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyPercentageTrailingStop {
    inner: wc::PercentageTrailingStop,
}

#[pymethods]
impl PyPercentageTrailingStop {
    #[new]
    #[pyo3(signature = (percent=5.0))]
    fn new(percent: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::PercentageTrailingStop::new(percent).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn percent(&self) -> f64 {
        self.inner.percent()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("PercentageTrailingStop(percent={})", self.inner.percent())
    }
}

// ============================== Step Trailing Stop ==============================

#[pyclass(
    name = "StepTrailingStop",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyStepTrailingStop {
    inner: wc::StepTrailingStop,
}

#[pymethods]
impl PyStepTrailingStop {
    #[new]
    #[pyo3(signature = (step_size=1.0))]
    fn new(step_size: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::StepTrailingStop::new(step_size).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn step_size(&self) -> f64 {
        self.inner.step_size()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("StepTrailingStop(step_size={})", self.inner.step_size())
    }
}

// ============================== Renko Trailing Stop ==============================

#[pyclass(
    name = "RenkoTrailingStop",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyRenkoTrailingStop {
    inner: wc::RenkoTrailingStop,
}

#[pymethods]
impl PyRenkoTrailingStop {
    #[new]
    #[pyo3(signature = (block_size=1.0))]
    fn new(block_size: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RenkoTrailingStop::new(block_size).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn block_size(&self) -> f64 {
        self.inner.block_size()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RenkoTrailingStop(block_size={})", self.inner.block_size())
    }
}

// ============================== Kase DevStop ==============================

#[pyclass(name = "KaseDevStop", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyKaseDevStop {
    inner: wc::KaseDevStop,
}

#[pymethods]
impl PyKaseDevStop {
    #[new]
    #[pyo3(signature = (period=30, dev=1.0))]
    fn new(period: usize, dev: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::KaseDevStop::new(period, dev).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.value, o.direction)))
    }
    /// Batch over input columns high, low, close. Returns shape `(n, 2)` with
    /// columns `[value, direction]`; warmup rows are `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn params(&self) -> (usize, f64) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (period, dev) = self.inner.params();
        format!("KaseDevStop(period={period}, dev={dev})")
    }
}

// ============================== Elder SafeZone ==============================

#[pyclass(name = "ElderSafeZone", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyElderSafeZone {
    inner: wc::ElderSafeZone,
}

#[pymethods]
impl PyElderSafeZone {
    #[new]
    #[pyo3(signature = (period=14, coeff=2.0))]
    fn new(period: usize, coeff: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ElderSafeZone::new(period, coeff).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.value, o.direction)))
    }
    /// Batch over input columns high, low, close. Returns shape `(n, 2)` with
    /// columns `[value, direction]`; warmup rows are `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn params(&self) -> (usize, f64) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (period, coeff) = self.inner.params();
        format!("ElderSafeZone(period={period}, coeff={coeff})")
    }
}

// ============================== ATR Ratchet ==============================

#[pyclass(name = "AtrRatchet", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAtrRatchet {
    inner: wc::AtrRatchet,
}

#[pymethods]
impl PyAtrRatchet {
    #[new]
    #[pyo3(signature = (atr_period=14, start_mult=4.0, increment=0.1))]
    fn new(atr_period: usize, start_mult: f64, increment: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AtrRatchet::new(atr_period, start_mult, increment).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.value, o.direction)))
    }
    /// Batch over input columns high, low, close. Returns shape `(n, 2)` with
    /// columns `[value, direction]`; warmup rows are `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn params(&self) -> (usize, f64, f64) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (atr_period, start_mult, increment) = self.inner.params();
        format!(
            "AtrRatchet(atr_period={atr_period}, start_mult={start_mult}, increment={increment})"
        )
    }
}

// ============================== NRTR ==============================

#[pyclass(name = "Nrtr", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyNrtr {
    inner: wc::Nrtr,
}

#[pymethods]
impl PyNrtr {
    #[new]
    #[pyo3(signature = (pct=2.0))]
    fn new(pct: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Nrtr::new(pct).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.value, o.direction)))
    }
    /// Batch over input columns high, low, close. Returns shape `(n, 2)` with
    /// columns `[value, direction]`; warmup rows are `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn pct(&self) -> f64 {
        self.inner.pct()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("Nrtr(pct={})", self.inner.pct())
    }
}

// ============================== Modified MA Stop ==============================

#[pyclass(
    name = "ModifiedMaStop",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyModifiedMaStop {
    inner: wc::ModifiedMaStop,
}

#[pymethods]
impl PyModifiedMaStop {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ModifiedMaStop::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.value, o.direction)))
    }
    /// Batch over input columns high, low, close. Returns shape `(n, 2)` with
    /// columns `[value, direction]`; warmup rows are `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.value;
                out[i * 2 + 1] = o.direction;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ModifiedMaStop(period={})", self.inner.period())
    }
}

// ============================== Typical Price ==============================

#[pyclass(name = "TypicalPrice", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTypicalPrice {
    inner: wc::TypicalPrice,
}

#[pymethods]
impl PyTypicalPrice {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::TypicalPrice::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns high, low, close (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "TypicalPrice()".to_string()
    }
}

// ============================== Median Price ==============================

#[pyclass(name = "MedianPrice", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMedianPrice {
    inner: wc::MedianPrice,
}

#[pymethods]
impl PyMedianPrice {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::MedianPrice::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns high, low (both equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "MedianPrice()".to_string()
    }
}

// ============================== Weighted Close ==============================

#[pyclass(name = "WeightedClose", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyWeightedClose {
    inner: wc::WeightedClose,
}

#[pymethods]
impl PyWeightedClose {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::WeightedClose::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns high, low, close (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "WeightedClose()".to_string()
    }
}

// ============================== Linear Regression ==============================

#[pyclass(
    name = "LinearRegression",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyLinearRegression {
    inner: wc::LinearRegression,
}

#[pymethods]
impl PyLinearRegression {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::LinearRegression::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("LinearRegression(period={})", self.inner.period())
    }
}

// ============================== Linear Regression Slope ==============================

#[pyclass(name = "LinRegSlope", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyLinRegSlope {
    inner: wc::LinRegSlope,
}

#[pymethods]
impl PyLinRegSlope {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::LinRegSlope::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("LinRegSlope(period={})", self.inner.period())
    }
}

// ============================== Accelerator Oscillator ==============================

#[pyclass(
    name = "AcceleratorOscillator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAcceleratorOscillator {
    inner: wc::AcceleratorOscillator,
}

#[pymethods]
impl PyAcceleratorOscillator {
    #[new]
    #[pyo3(signature = (ao_fast=5, ao_slow=34, signal_period=5))]
    fn new(ao_fast: usize, ao_slow: usize, signal_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AcceleratorOscillator::new(ao_fast, ao_slow, signal_period)
                .map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns high, low (both equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn params(&self) -> (usize, usize, usize) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (f, s, sig) = self.inner.params();
        format!("AcceleratorOscillator(ao_fast={f}, ao_slow={s}, signal_period={sig})")
    }
}

// ============================== Balance of Power ==============================

#[pyclass(
    name = "BalanceOfPower",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyBalanceOfPower {
    inner: wc::BalanceOfPower,
}

#[pymethods]
impl PyBalanceOfPower {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::BalanceOfPower::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns open, high, low, close (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(o.len());
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "BalanceOfPower()".to_string()
    }
}

// ============================== Choppiness Index ==============================

#[pyclass(
    name = "ChoppinessIndex",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyChoppinessIndex {
    inner: wc::ChoppinessIndex,
}

#[pymethods]
impl PyChoppinessIndex {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ChoppinessIndex::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns high, low, close (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ChoppinessIndex(period={})", self.inner.period())
    }
}

// ============================== Vertical Horizontal Filter ==============================

#[pyclass(
    name = "VerticalHorizontalFilter",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyVerticalHorizontalFilter {
    inner: wc::VerticalHorizontalFilter,
}

#[pymethods]
impl PyVerticalHorizontalFilter {
    #[new]
    #[pyo3(signature = (period=28))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VerticalHorizontalFilter::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("VerticalHorizontalFilter(period={})", self.inner.period())
    }
}

// ============================== True Range ==============================

#[pyclass(name = "TrueRange", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTrueRange {
    inner: wc::TrueRange,
}

#[pymethods]
impl PyTrueRange {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::TrueRange::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns high, low, close (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "TrueRange()".to_string()
    }
}

// ============================== Chaikin Volatility ==============================

#[pyclass(
    name = "ChaikinVolatility",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyChaikinVolatility {
    inner: wc::ChaikinVolatility,
}

#[pymethods]
impl PyChaikinVolatility {
    #[new]
    #[pyo3(signature = (ema_period=10, roc_period=10))]
    fn new(ema_period: usize, roc_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ChaikinVolatility::new(ema_period, roc_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns high, low (both equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (ema, roc) = self.inner.periods();
        format!("ChaikinVolatility(ema_period={ema}, roc_period={roc})")
    }
}

// ============================== Z-Score ==============================

#[pyclass(name = "ZScore", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyZScore {
    inner: wc::ZScore,
}

#[pymethods]
impl PyZScore {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ZScore::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ZScore(period={})", self.inner.period())
    }
}

// ============================== Linear Regression Angle ==============================

#[pyclass(name = "LinRegAngle", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyLinRegAngle {
    inner: wc::LinRegAngle,
}

#[pymethods]
impl PyLinRegAngle {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::LinRegAngle::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("LinRegAngle(period={})", self.inner.period())
    }
}

#[pyclass(
    name = "YangZhangVolatility",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyYangZhangVolatility {
    inner: wc::YangZhangVolatility,
}

#[pymethods]
impl PyYangZhangVolatility {
    #[new]
    #[pyo3(signature = (period=20, trading_periods=252))]
    fn new(period: usize, trading_periods: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::YangZhangVolatility::new(period, trading_periods).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns open, high, low, close (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let cl = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != cl.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(o.len());
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], cl[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    #[getter]
    fn k(&self) -> f64 {
        self.inner.k()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, t) = self.inner.periods();
        format!("YangZhangVolatility(period={p}, trading_periods={t})")
    }
}

// ============================== Rogers-Satchell Volatility ==============================

#[pyclass(
    name = "RogersSatchellVolatility",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyRogersSatchellVolatility {
    inner: wc::RogersSatchellVolatility,
}

#[pymethods]
impl PyRogersSatchellVolatility {
    #[new]
    #[pyo3(signature = (period=20, trading_periods=252))]
    fn new(period: usize, trading_periods: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RogersSatchellVolatility::new(period, trading_periods).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns open, high, low, close (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let cl = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != cl.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(o.len());
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], cl[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, t) = self.inner.periods();
        format!("RogersSatchellVolatility(period={p}, trading_periods={t})")
    }
}

// ============================== Garman-Klass Volatility ==============================

#[pyclass(
    name = "GarmanKlassVolatility",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyGarmanKlassVolatility {
    inner: wc::GarmanKlassVolatility,
}

#[pymethods]
impl PyGarmanKlassVolatility {
    #[new]
    #[pyo3(signature = (period=20, trading_periods=252))]
    fn new(period: usize, trading_periods: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::GarmanKlassVolatility::new(period, trading_periods).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns open, high, low, close (all equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let cl = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != cl.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(o.len());
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], cl[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, t) = self.inner.periods();
        format!("GarmanKlassVolatility(period={p}, trading_periods={t})")
    }
}

// ============================== Parkinson Volatility ==============================

#[pyclass(
    name = "ParkinsonVolatility",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyParkinsonVolatility {
    inner: wc::ParkinsonVolatility,
}

#[pymethods]
impl PyParkinsonVolatility {
    #[new]
    #[pyo3(signature = (period=20, trading_periods=252))]
    fn new(period: usize, trading_periods: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ParkinsonVolatility::new(period, trading_periods).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns high, low (both equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, t) = self.inner.periods();
        format!("ParkinsonVolatility(period={p}, trading_periods={t})")
    }
}

// ============================== RVI (Volatility) ==============================
//
// Named `RVIVolatility` rather than plain `RVI` to disambiguate from
// Relative Vigor Index (a separate momentum indicator that lives in
// Family 02 with the shorter `RVI` name).

#[pyclass(name = "RVIVolatility", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRviVolatility {
    inner: wc::RviVolatility,
}

#[pymethods]
impl PyRviVolatility {
    #[new]
    #[pyo3(signature = (period=10))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RviVolatility::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RVIVolatility(period={})", self.inner.period())
    }
}

// ============================== MA Envelope ==============================

#[pyclass(name = "MaEnvelope", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMaEnvelope {
    inner: wc::MaEnvelope,
}

#[pymethods]
impl PyMaEnvelope {
    #[new]
    #[pyo3(signature = (period=20, percent=0.025))]
    fn new(period: usize, percent: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MaEnvelope::new(period, percent).map_err(map_err)?,
        })
    }
    /// Returns `(upper, middle, lower)` or `None` during warmup.
    fn update(&mut self, value: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update(value)
            .map(|o| (o.upper, o.middle, o.lower))
    }
    /// Batch returns shape `(n, 3)` columns `[upper, middle, lower]`.
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Acceleration Bands ==============================

#[pyclass(
    name = "AccelerationBands",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAccelerationBands {
    inner: wc::AccelerationBands,
}

#[pymethods]
impl PyAccelerationBands {
    #[new]
    #[pyo3(signature = (period=20, factor=0.001))]
    fn new(period: usize, factor: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AccelerationBands::new(period, factor).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.upper, o.middle, o.lower)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== STARC Bands ==============================

#[pyclass(name = "StarcBands", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyStarcBands {
    inner: wc::StarcBands,
}

#[pymethods]
impl PyStarcBands {
    #[new]
    #[pyo3(signature = (sma_period=6, atr_period=15, multiplier=2.0))]
    fn new(sma_period: usize, atr_period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::StarcBands::new(sma_period, atr_period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.upper, o.middle, o.lower)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== ATR Bands ==============================

#[pyclass(name = "AtrBands", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAtrBands {
    inner: wc::AtrBands,
}

#[pymethods]
impl PyAtrBands {
    #[new]
    #[pyo3(signature = (period=14, multiplier=3.0))]
    fn new(period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AtrBands::new(period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.upper, o.middle, o.lower)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Hurst Channel ==============================

#[pyclass(name = "HurstChannel", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyHurstChannel {
    inner: wc::HurstChannel,
}

#[pymethods]
impl PyHurstChannel {
    #[new]
    #[pyo3(signature = (period=10, multiplier=0.5))]
    fn new(period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::HurstChannel::new(period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.upper, o.middle, o.lower)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== LinReg Channel ==============================

#[pyclass(name = "LinRegChannel", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyLinRegChannel {
    inner: wc::LinRegChannel,
}

#[pymethods]
impl PyLinRegChannel {
    #[new]
    #[pyo3(signature = (period=20, multiplier=2.0))]
    fn new(period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::LinRegChannel::new(period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update(value)
            .map(|o| (o.upper, o.middle, o.lower))
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Standard Error Bands ==============================

#[pyclass(
    name = "StandardErrorBands",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyStandardErrorBands {
    inner: wc::StandardErrorBands,
}

#[pymethods]
impl PyStandardErrorBands {
    #[new]
    #[pyo3(signature = (period=21, multiplier=2.0))]
    fn new(period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::StandardErrorBands::new(period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update(value)
            .map(|o| (o.upper, o.middle, o.lower))
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Quartile Bands ==============================

#[pyclass(name = "QuartileBands", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyQuartileBands {
    inner: wc::QuartileBands,
}

#[pymethods]
impl PyQuartileBands {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::QuartileBands::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update(value)
            .map(|o| (o.upper, o.middle, o.lower))
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Bomar Bands ==============================

#[pyclass(name = "BomarBands", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyBomarBands {
    inner: wc::BomarBands,
}

#[pymethods]
impl PyBomarBands {
    #[new]
    #[pyo3(signature = (period=20, coverage=0.85))]
    fn new(period: usize, coverage: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::BomarBands::new(period, coverage).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update(value)
            .map(|o| (o.upper, o.middle, o.lower))
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Median Channel ==============================

#[pyclass(name = "MedianChannel", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMedianChannel {
    inner: wc::MedianChannel,
}

#[pymethods]
impl PyMedianChannel {
    #[new]
    #[pyo3(signature = (period=20, multiplier=2.0))]
    fn new(period: usize, multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MedianChannel::new(period, multiplier).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update(value)
            .map(|o| (o.upper, o.middle, o.lower))
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Projection Bands ==============================

#[pyclass(
    name = "ProjectionBands",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyProjectionBands {
    inner: wc::ProjectionBands,
}

#[pymethods]
impl PyProjectionBands {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ProjectionBands::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.upper, o.middle, o.lower)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(h[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.upper;
                out[i * 3 + 1] = o.middle;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Central Pivot Range ==============================

#[pyclass(
    name = "CentralPivotRange",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyCentralPivotRange {
    inner: wc::CentralPivotRange,
}

#[pymethods]
impl PyCentralPivotRange {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::CentralPivotRange::new(),
        }
    }
    /// Returns `(pivot, tc, bc)`.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.pivot, o.tc, o.bc)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.pivot;
                out[i * 3 + 1] = o.tc;
                out[i * 3 + 2] = o.bc;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Murrey Math Lines ==============================

#[pyclass(
    name = "MurreyMathLines",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyMurreyMathLines {
    inner: wc::MurreyMathLines,
}

#[pymethods]
impl PyMurreyMathLines {
    #[new]
    #[pyo3(signature = (period=64))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MurreyMathLines::new(period).map_err(map_err)?,
        })
    }
    /// Returns `(mm8_8, mm7_8, mm6_8, mm5_8, mm4_8, mm3_8, mm2_8, mm1_8, mm0_8)`.
    #[allow(clippy::type_complexity)]
    fn update(
        &mut self,
        candle: &Bound<'_, PyAny>,
    ) -> PyResult<Option<(f64, f64, f64, f64, f64, f64, f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| {
            (
                o.mm8_8, o.mm7_8, o.mm6_8, o.mm5_8, o.mm4_8, o.mm3_8, o.mm2_8, o.mm1_8, o.mm0_8,
            )
        }))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 9];
        for i in 0..n {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
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
        matrix(py, out, n, 9)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Andrews Pitchfork ==============================

#[pyclass(
    name = "AndrewsPitchfork",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAndrewsPitchfork {
    inner: wc::AndrewsPitchfork,
}

#[pymethods]
impl PyAndrewsPitchfork {
    #[new]
    #[pyo3(signature = (strength=2))]
    fn new(strength: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AndrewsPitchfork::new(strength).map_err(map_err)?,
        })
    }
    /// Returns `(median, upper, lower)`.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.median, o.upper, o.lower)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.median;
                out[i * 3 + 1] = o.upper;
                out[i * 3 + 2] = o.lower;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Volume-Weighted S/R ==============================

#[pyclass(
    name = "VolumeWeightedSr",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyVolumeWeightedSr {
    inner: wc::VolumeWeightedSr,
}

#[pymethods]
impl PyVolumeWeightedSr {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VolumeWeightedSr::new(period).map_err(map_err)?,
        })
    }
    /// Returns `(support, resistance)`.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.support, o.resistance)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, volume must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], v[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.support;
                out[i * 2 + 1] = o.resistance;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Pivot Reversal ==============================

#[pyclass(name = "PivotReversal", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPivotReversal {
    inner: wc::PivotReversal,
}

#[pymethods]
impl PyPivotReversal {
    #[new]
    #[pyo3(signature = (left=2, right=2))]
    fn new(left: usize, right: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::PivotReversal::new(left, right).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: high, low, close (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Double Bollinger ==============================

#[pyclass(
    name = "DoubleBollinger",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyDoubleBollinger {
    inner: wc::DoubleBollinger,
}

#[pymethods]
impl PyDoubleBollinger {
    #[new]
    #[pyo3(signature = (period=20, k_inner=1.0, k_outer=2.0))]
    fn new(period: usize, k_inner: f64, k_outer: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::DoubleBollinger::new(period, k_inner, k_outer).map_err(map_err)?,
        })
    }
    /// Returns `(upper_outer, upper_inner, middle, lower_inner, lower_outer)`.
    fn update(&mut self, value: f64) -> Option<(f64, f64, f64, f64, f64)> {
        self.inner.update(value).map(|o| {
            (
                o.upper_outer,
                o.upper_inner,
                o.middle,
                o.lower_inner,
                o.lower_outer,
            )
        })
    }
    /// Returns shape `(n, 5)` columns
    /// `[upper_outer, upper_inner, middle, lower_inner, lower_outer]`.
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 5];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 5] = o.upper_outer;
                out[i * 5 + 1] = o.upper_inner;
                out[i * 5 + 2] = o.middle;
                out[i * 5 + 3] = o.lower_inner;
                out[i * 5 + 4] = o.lower_outer;
            }
        }
        matrix(py, out, n, 5)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== TTM Squeeze ==============================

#[pyclass(name = "TtmSqueeze", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTtmSqueeze {
    inner: wc::TtmSqueeze,
}

#[pymethods]
impl PyTtmSqueeze {
    #[new]
    #[pyo3(signature = (period=20, bb_mult=2.0, kc_mult=1.5))]
    fn new(period: usize, bb_mult: f64, kc_mult: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TtmSqueeze::new(period, bb_mult, kc_mult).map_err(map_err)?,
        })
    }
    /// Returns `(squeeze, momentum)` or `None` during warmup. `squeeze` is
    /// `1.0` while BB ⊂ KC, `0.0` otherwise.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.squeeze, o.momentum)))
    }
    /// Returns shape `(n, 2)` columns `[squeeze, momentum]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.squeeze;
                out[i * 2 + 1] = o.momentum;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Fractal Chaos Bands ==============================

#[pyclass(
    name = "FractalChaosBands",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyFractalChaosBands {
    inner: wc::FractalChaosBands,
}

#[pymethods]
impl PyFractalChaosBands {
    #[new]
    #[pyo3(signature = (k=2))]
    fn new(k: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::FractalChaosBands::new(k).map_err(map_err)?,
        })
    }
    /// Returns `(upper, lower)` or `None` until both fractals have confirmed.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.upper, o.lower)))
    }
    /// Returns shape `(n, 2)` columns `[upper, lower]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.upper;
                out[i * 2 + 1] = o.lower;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== VWAP StdDev Bands ==============================

#[pyclass(
    name = "VwapStdDevBands",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyVwapStdDevBands {
    inner: wc::VwapStdDevBands,
}

#[pymethods]
impl PyVwapStdDevBands {
    #[new]
    #[pyo3(signature = (multiplier=2.0))]
    fn new(multiplier: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VwapStdDevBands::new(multiplier).map_err(map_err)?,
        })
    }
    /// Returns `(upper, middle, lower, stddev)` or `None` until volume is non-zero.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.upper, o.middle, o.lower, o.stddev)))
    }
    /// Returns shape `(n, 4)` columns `[upper, middle, lower, stddev]`.
    #[allow(clippy::many_single_char_names)]
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 4] = o.upper;
                out[i * 4 + 1] = o.middle;
                out[i * 4 + 2] = o.lower;
                out[i * 4 + 3] = o.stddev;
            }
        }
        matrix(py, out, n, 4)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Classic Pivots ==============================

#[pyclass(name = "ClassicPivots", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyClassicPivots {
    inner: wc::ClassicPivots,
}

#[pymethods]
impl PyClassicPivots {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::ClassicPivots::new(),
        }
    }
    /// Returns `(pp, r1, r2, r3, s1, s2, s3)` or None during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<PivotLevels>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.pp, o.r1, o.r2, o.r3, o.s1, o.s2, o.s3)))
    }
    /// Batch over input columns high, low, close. Returns shape `(n, 7)` for
    /// `[pp, r1, r2, r3, s1, s2, s3]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 7];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 7] = o.pp;
                out[i * 7 + 1] = o.r1;
                out[i * 7 + 2] = o.r2;
                out[i * 7 + 3] = o.r3;
                out[i * 7 + 4] = o.s1;
                out[i * 7 + 5] = o.s2;
                out[i * 7 + 6] = o.s3;
            }
        }
        matrix(py, out, n, 7)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Fibonacci Pivots ==============================

#[pyclass(
    name = "FibonacciPivots",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyFibonacciPivots {
    inner: wc::FibonacciPivots,
}

#[pymethods]
impl PyFibonacciPivots {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::FibonacciPivots::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<PivotLevels>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.pp, o.r1, o.r2, o.r3, o.s1, o.s2, o.s3)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 7];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 7] = o.pp;
                out[i * 7 + 1] = o.r1;
                out[i * 7 + 2] = o.r2;
                out[i * 7 + 3] = o.r3;
                out[i * 7 + 4] = o.s1;
                out[i * 7 + 5] = o.s2;
                out[i * 7 + 6] = o.s3;
            }
        }
        matrix(py, out, n, 7)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Camarilla Pivots ==============================

#[pyclass(name = "Camarilla", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyCamarilla {
    inner: wc::Camarilla,
}

#[pymethods]
impl PyCamarilla {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::Camarilla::new(),
        }
    }
    /// Returns `(pp, r1, r2, r3, r4, s1, s2, s3, s4)` or None during warmup.
    #[allow(clippy::type_complexity)]
    fn update(
        &mut self,
        candle: &Bound<'_, PyAny>,
    ) -> PyResult<Option<(f64, f64, f64, f64, f64, f64, f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.pp, o.r1, o.r2, o.r3, o.r4, o.s1, o.s2, o.s3, o.s4)))
    }
    /// Batch over input columns high, low, close. Returns shape `(n, 9)` for
    /// `[pp, r1, r2, r3, r4, s1, s2, s3, s4]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 9];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
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
        matrix(py, out, n, 9)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Woodie Pivots ==============================

#[pyclass(name = "WoodiePivots", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyWoodiePivots {
    inner: wc::WoodiePivots,
}

#[pymethods]
impl PyWoodiePivots {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::WoodiePivots::new(),
        }
    }
    /// Returns `(pp, r1, r2, s1, s2)` or None during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<WoodieLevels>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.pp, o.r1, o.r2, o.s1, o.s2)))
    }
    /// Batch over input columns high, low, close. Returns shape `(n, 5)` for
    /// `[pp, r1, r2, s1, s2]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 5];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 5] = o.pp;
                out[i * 5 + 1] = o.r1;
                out[i * 5 + 2] = o.r2;
                out[i * 5 + 3] = o.s1;
                out[i * 5 + 4] = o.s2;
            }
        }
        matrix(py, out, n, 5)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== DeMark Pivots ==============================

#[pyclass(name = "DemarkPivots", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyDemarkPivots {
    inner: wc::DemarkPivots,
}

#[pymethods]
impl PyDemarkPivots {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::DemarkPivots::new(),
        }
    }
    /// Returns `(pp, r1, s1)` or None during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.pp, o.r1, o.s1)))
    }
    /// Batch over input columns open, high, low, close. Returns shape `(n, 3)`
    /// for `[pp, r1, s1]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let n = o.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(v) = self.inner.update(candle) {
                out[i * 3] = v.pp;
                out[i * 3 + 1] = v.r1;
                out[i * 3 + 2] = v.s1;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Williams Fractals ==============================

#[pyclass(
    name = "WilliamsFractals",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyWilliamsFractals {
    inner: wc::WilliamsFractals,
}

#[pymethods]
impl PyWilliamsFractals {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::WilliamsFractals::new(),
        }
    }
    /// Returns `(up, down)` where each component is either the fractal price
    /// or `None` if no fractal was confirmed at the centre of the current
    /// 5-bar window. The outer `None` is returned during warmup (first 4 bars).
    fn update(
        &mut self,
        candle: &Bound<'_, PyAny>,
    ) -> PyResult<Option<(Option<f64>, Option<f64>)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.up, o.down)))
    }
    /// Batch over input columns high, low. Returns shape `(n, 2)` for
    /// `[up_fractal, down_fractal]`. Values are NaN both during warmup and on
    /// bars where no fractal was confirmed.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                if let Some(v) = o.up {
                    out[i * 2] = v;
                }
                if let Some(v) = o.down {
                    out[i * 2 + 1] = v;
                }
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== ZigZag ==============================

#[pyclass(name = "ZigZag", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyZigZag {
    inner: wc::ZigZag,
}

#[pymethods]
impl PyZigZag {
    #[new]
    #[pyo3(signature = (threshold=0.05))]
    fn new(threshold: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ZigZag::new(threshold).map_err(map_err)?,
        })
    }
    /// Returns `(swing, direction)` if a swing was confirmed on this bar,
    /// else `None`.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.swing, o.direction)))
    }
    /// Batch over input columns high, low. Returns shape `(n, 2)` for
    /// `[swing_price, direction]`. NaN on bars without a confirmed swing.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.swing;
                out[i * 2 + 1] = o.direction;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn threshold(&self) -> f64 {
        self.inner.threshold()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}
// ============================== TD Setup ==============================

#[pyclass(name = "TDSetup", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTdSetup {
    inner: wc::TdSetup,
}

#[pymethods]
impl PyTdSetup {
    #[new]
    #[pyo3(signature = (lookback=4, target=9))]
    fn new(lookback: usize, target: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TdSetup::new(lookback, target).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn __repr__(&self) -> String {
        let (lb, tg) = self.inner.params();
        format!("TDSetup(lookback={lb}, target={tg})")
    }
}

// ============================== TD Sequential ==============================

#[pyclass(name = "TDSequential", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTdSequential {
    inner: wc::TdSequential,
}

#[pymethods]
impl PyTdSequential {
    #[new]
    #[pyo3(signature = (setup_lookback=4, setup_target=9, countdown_lookback=2, countdown_target=13))]
    fn new(
        setup_lookback: usize,
        setup_target: usize,
        countdown_lookback: usize,
        countdown_target: usize,
    ) -> PyResult<Self> {
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
    /// Returns `(setup, countdown, direction)` or `None` during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.setup, o.countdown, o.direction)))
    }
    /// Batch returns shape `(n, 3)`: `[setup, countdown, direction]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.setup;
                out[i * 3 + 1] = o.countdown;
                out[i * 3 + 2] = o.direction;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== TD DeMarker ==============================

#[pyclass(name = "TDDeMarker", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTdDeMarker {
    inner: wc::TdDeMarker,
}

#[pymethods]
impl PyTdDeMarker {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TdDeMarker::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TDDeMarker(period={})", self.inner.period())
    }
}

// ============================== TD REI ==============================

#[pyclass(name = "TDREI", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTdRei {
    inner: wc::TdRei,
}

#[pymethods]
impl PyTdRei {
    #[new]
    #[pyo3(signature = (period=5))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TdRei::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TDREI(period={})", self.inner.period())
    }
}

// ============================== TD Pressure ==============================

#[pyclass(name = "TDPressure", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTdPressure {
    inner: wc::TdPressure,
}

#[pymethods]
impl PyTdPressure {
    #[new]
    #[pyo3(signature = (period=5))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TdPressure::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over input columns: open, high, low, close, volume.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let v = volume.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() || c.len() != v.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(o.len());
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TDPressure(period={})", self.inner.period())
    }
}

// ============================== TD Combo ==============================

#[pyclass(name = "TDCombo", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTdCombo {
    inner: wc::TdCombo,
}

#[pymethods]
impl PyTdCombo {
    #[new]
    #[pyo3(signature = (setup_lookback=4, setup_target=9, countdown_lookback=2, countdown_target=13))]
    fn new(
        setup_lookback: usize,
        setup_target: usize,
        countdown_lookback: usize,
        countdown_target: usize,
    ) -> PyResult<Self> {
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
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== TD D-Wave ==============================

#[pyclass(name = "TDDWave", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTdDWave {
    inner: wc::TdDWave,
}

#[pymethods]
impl PyTdDWave {
    #[new]
    #[pyo3(signature = (strength=2))]
    fn new(strength: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TdDWave::new(strength).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== TD Moving Averages ==============================

#[pyclass(
    name = "TDMovingAverage",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyTdMovingAverage {
    inner: wc::TdMovingAverage,
}

#[pymethods]
impl PyTdMovingAverage {
    #[new]
    #[pyo3(signature = (period_st1=5, period_st2=13))]
    fn new(period_st1: usize, period_st2: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TdMovingAverage::new(period_st1, period_st2).map_err(map_err)?,
        })
    }
    /// Returns `(st1, st2)`.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.st1, o.st2)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high, low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.st1;
                out[i * 2 + 1] = o.st2;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== TD Countdown ==============================

#[pyclass(name = "TDCountdown", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTdCountdown {
    inner: wc::TdCountdown,
}

#[pymethods]
impl PyTdCountdown {
    #[new]
    #[pyo3(signature = (setup_lookback=4, setup_target=9, countdown_lookback=2, countdown_target=13))]
    fn new(
        setup_lookback: usize,
        setup_target: usize,
        countdown_lookback: usize,
        countdown_target: usize,
    ) -> PyResult<Self> {
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
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== TD Lines ==============================

#[pyclass(name = "TDLines", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTdLines {
    inner: wc::TdLines,
}

#[pymethods]
impl PyTdLines {
    #[new]
    #[pyo3(signature = (lookback=4, target=9))]
    fn new(lookback: usize, target: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TdLines::new(lookback, target).map_err(map_err)?,
        })
    }
    /// Returns `(resistance, support)` (with `NaN` for unset levels) or
    /// `None` during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.resistance, o.support)))
    }
    /// Batch returns shape `(n, 2)`: `[resistance, support]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.resistance;
                out[i * 2 + 1] = o.support;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== TD Range Projection ==============================

#[pyclass(
    name = "TDRangeProjection",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone, Default)]
struct PyTdRangeProjection {
    inner: wc::TdRangeProjection,
}

#[pymethods]
impl PyTdRangeProjection {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::TdRangeProjection::new(),
        }
    }
    /// Returns `(projected_high, projected_low)` for the next bar.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.high, o.low)))
    }
    /// Batch returns shape `(n, 2)`: `[projected_high, projected_low]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let n = o.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(p) = self.inner.update(candle) {
                out[i * 2] = p.high;
                out[i * 2 + 1] = p.low;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== TD Differential ==============================

#[pyclass(
    name = "TDDifferential",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone, Default)]
struct PyTdDifferential {
    inner: wc::TdDifferential,
}

#[pymethods]
impl PyTdDifferential {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::TdDifferential::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== TD Open ==============================

#[pyclass(name = "TDOpen", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone, Default)]
struct PyTdOpen {
    inner: wc::TdOpen,
}

#[pymethods]
impl PyTdOpen {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::TdOpen::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(o.len());
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== TD Risk Level ==============================

#[pyclass(name = "TDRiskLevel", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTdRiskLevel {
    inner: wc::TdRiskLevel,
}

#[pymethods]
impl PyTdRiskLevel {
    #[new]
    #[pyo3(signature = (lookback=4, target=9))]
    fn new(lookback: usize, target: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TdRiskLevel::new(lookback, target).map_err(map_err)?,
        })
    }
    /// Returns `(buy_risk, sell_risk)` (with `NaN` for unset levels) or
    /// `None` during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.buy_risk, o.sell_risk)))
    }
    /// Batch returns shape `(n, 2)`: `[buy_risk, sell_risk]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.buy_risk;
                out[i * 2 + 1] = o.sell_risk;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Ehlers / Cycle (Family 10) ==============================

macro_rules! py_scalar_one_period {
    ($wrapper:ident, $py_name:literal, $rust_ty:ty) => {
        #[pyclass(name = $py_name, module = "wickra._wickra", skip_from_py_object)]
        #[derive(Clone)]
        struct $wrapper {
            inner: $rust_ty,
        }

        #[pymethods]
        impl $wrapper {
            #[new]
            fn new(period: usize) -> PyResult<Self> {
                Ok(Self {
                    inner: <$rust_ty>::new(period).map_err(map_err)?,
                })
            }
            fn update(&mut self, value: f64) -> Option<f64> {
                self.inner.update(value)
            }
            fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
                let slice = prices.as_slice();
                f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
            }
            #[getter]
            fn period(&self) -> usize {
                self.inner.period()
            }
            #[getter]
            fn value(&self) -> Option<f64> {
                self.inner.value()
            }
            fn reset(&mut self) {
                self.inner.reset();
            }

            fn name(&self) -> &'static str {
                self.inner.name()
            }
            fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
            fn __repr__(&self) -> String {
                format!("{}(period={})", $py_name, self.inner.period())
            }
        }
    };
}

py_scalar_one_period!(PySuperSmoother, "SuperSmoother", wc::SuperSmoother);
py_scalar_one_period!(PyFisherTransform, "FisherTransform", wc::FisherTransform);
py_scalar_one_period!(PyDecycler, "Decycler", wc::Decycler);
py_scalar_one_period!(PyCenterOfGravity, "CenterOfGravity", wc::CenterOfGravity);
py_scalar_one_period!(PyCyberneticCycle, "CyberneticCycle", wc::CyberneticCycle);
py_scalar_one_period!(
    PyInstantaneousTrendline,
    "InstantaneousTrendline",
    wc::InstantaneousTrendline
);
py_scalar_one_period!(PyEhlersStochastic, "EhlersStochastic", wc::EhlersStochastic);

// --- InverseFisherTransform: single f64 `scale` param ---

#[pyclass(
    name = "InverseFisherTransform",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyInverseFisherTransform {
    inner: wc::InverseFisherTransform,
}

#[pymethods]
impl PyInverseFisherTransform {
    #[new]
    #[pyo3(signature = (scale=1.0))]
    fn new(scale: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::InverseFisherTransform::new(scale).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn scale(&self) -> f64 {
        self.inner.scale()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("InverseFisherTransform(scale={})", self.inner.scale())
    }
}

// --- DecyclerOscillator: two-period ---

#[pyclass(
    name = "DecyclerOscillator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyDecyclerOscillator {
    inner: wc::DecyclerOscillator,
}

#[pymethods]
impl PyDecyclerOscillator {
    #[new]
    fn new(fast: usize, slow: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::DecyclerOscillator::new(fast, slow).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (f, s) = self.inner.periods();
        format!("DecyclerOscillator(fast={f}, slow={s})")
    }
}

// --- RoofingFilter: two-period (lp, hp) ---

#[pyclass(name = "RoofingFilter", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRoofingFilter {
    inner: wc::RoofingFilter,
}

#[pymethods]
impl PyRoofingFilter {
    #[new]
    #[pyo3(signature = (lp_period=10, hp_period=48))]
    fn new(lp_period: usize, hp_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RoofingFilter::new(lp_period, hp_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (lp, hp) = self.inner.periods();
        format!("RoofingFilter(lp_period={lp}, hp_period={hp})")
    }
}

// --- EmpiricalModeDecomposition: period + fraction ---

#[pyclass(
    name = "EmpiricalModeDecomposition",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyEmd {
    inner: wc::EmpiricalModeDecomposition,
}

#[pymethods]
impl PyEmd {
    #[new]
    #[pyo3(signature = (period=20, fraction=0.5))]
    fn new(period: usize, fraction: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::EmpiricalModeDecomposition::new(period, fraction).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn fraction(&self) -> f64 {
        self.inner.fraction()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "EmpiricalModeDecomposition(period={}, fraction={})",
            self.inner.period(),
            self.inner.fraction()
        )
    }
}

// --- HilbertDominantCycle / SineWave / AdaptiveCycle: parameterless ---

macro_rules! py_no_params_scalar {
    ($wrapper:ident, $py_name:literal, $rust_ty:ty) => {
        #[pyclass(name = $py_name, module = "wickra._wickra", skip_from_py_object)]
        #[derive(Clone)]
        struct $wrapper {
            inner: $rust_ty,
        }

        #[pymethods]
        impl $wrapper {
            #[new]
            fn new() -> Self {
                Self {
                    inner: <$rust_ty>::new(),
                }
            }
            fn update(&mut self, value: f64) -> Option<f64> {
                self.inner.update(value)
            }
            fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
                let slice = prices.as_slice();
                f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
            }
            #[getter]
            fn value(&self) -> Option<f64> {
                self.inner.value()
            }
            fn reset(&mut self) {
                self.inner.reset();
            }

            fn name(&self) -> &'static str {
                self.inner.name()
            }
            fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
            fn __repr__(&self) -> String {
                format!("{}()", $py_name)
            }
        }
    };
}

py_no_params_scalar!(
    PyHilbertDominantCycle,
    "HilbertDominantCycle",
    wc::HilbertDominantCycle
);
py_no_params_scalar!(PyAdaptiveCycle, "AdaptiveCycle", wc::AdaptiveCycle);
py_no_params_scalar!(PyHtDcPhase, "HT_DCPHASE", wc::HtDcPhase);
py_no_params_scalar!(PyHtTrendMode, "HT_TRENDMODE", wc::HtTrendMode);

// SineWave needs a `lead` accessor in addition to scalar value, but otherwise
// matches the parameterless surface.
#[pyclass(name = "SineWave", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySineWave {
    inner: wc::SineWave,
}

#[pymethods]
impl PySineWave {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::SineWave::new(),
        }
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    #[getter]
    fn lead(&self) -> f64 {
        self.inner.lead()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "SineWave()".to_string()
    }
}

// --- MAMA: multi-output (mama, fama), shape (n, 2) ---

#[pyclass(name = "MAMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMama {
    inner: wc::Mama,
}

#[pymethods]
impl PyMama {
    #[new]
    #[pyo3(signature = (fast_limit=0.5, slow_limit=0.05))]
    fn new(fast_limit: f64, slow_limit: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Mama::new(fast_limit, slow_limit).map_err(map_err)?,
        })
    }
    /// Returns `(mama, fama)` or `None` during warmup.
    fn update(&mut self, value: f64) -> Option<(f64, f64)> {
        self.inner.update(value).map(|o| (o.mama, o.fama))
    }
    /// Batch returns shape `(n, 2)` columns `[mama, fama]`. Warmup rows NaN.
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let n = slice.len();
        let mut out = vec![f64::NAN; n * 2];
        for (i, p) in slice.iter().enumerate() {
            if let Some(o) = self.inner.update(*p) {
                out[i * 2] = o.mama;
                out[i * 2 + 1] = o.fama;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn limits(&self) -> (f64, f64) {
        self.inner.limits()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (f, s) = self.inner.limits();
        format!("MAMA(fast_limit={f}, slow_limit={s})")
    }
}

// --- FAMA: scalar wrapper exposing only the fama line ---

#[pyclass(name = "FAMA", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFama {
    inner: wc::Fama,
}

#[pymethods]
impl PyFama {
    #[new]
    #[pyo3(signature = (fast_limit=0.5, slow_limit=0.05))]
    fn new(fast_limit: f64, slow_limit: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Fama::new(fast_limit, slow_limit).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn limits(&self) -> (f64, f64) {
        self.inner.limits()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (f, s) = self.inner.limits();
        format!("FAMA(fast_limit={f}, slow_limit={s})")
    }
}

// ============================== Ichimoku ==============================

#[pyclass(name = "Ichimoku", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyIchimoku {
    inner: wc::Ichimoku,
}

#[pymethods]
impl PyIchimoku {
    #[new]
    #[pyo3(signature = (tenkan_period=9, kijun_period=26, senkou_b_period=52, displacement=26))]
    fn new(
        tenkan_period: usize,
        kijun_period: usize,
        senkou_b_period: usize,
        displacement: usize,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Ichimoku::new(tenkan_period, kijun_period, senkou_b_period, displacement)
                .map_err(map_err)?,
        })
    }
    /// Returns `(tenkan, kijun, senkou_a, senkou_b, chikou)` as a 5-tuple
    /// where each element is `float` or `None`.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<IchimokuLines>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.tenkan, o.kijun, o.senkou_a, o.senkou_b, o.chikou)))
    }
    /// Batch over high/low/close input columns. Returns shape `(n, 5)` with
    /// columns `[tenkan, kijun, senkou_a, senkou_b, chikou]`. Any cell whose
    /// underlying line is undefined at that bar is `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 5];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
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
        matrix(py, out, n, 5)
    }
    #[getter]
    fn periods(&self) -> (usize, usize, usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (t, k, sb, d) = self.inner.periods();
        format!(
            "Ichimoku(tenkan_period={t}, kijun_period={k}, senkou_b_period={sb}, displacement={d})"
        )
    }
}

// ============================== Heikin-Ashi ==============================

#[pyclass(name = "HeikinAshi", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone, Default)]
struct PyHeikinAshi {
    inner: wc::HeikinAshi,
}

#[pymethods]
impl PyHeikinAshi {
    #[new]
    fn new() -> Self {
        Self::default()
    }
    /// Returns `(ha_open, ha_high, ha_low, ha_close)`.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.open, o.high, o.low, o.close)))
    }
    /// Batch over OHLC input columns. Returns shape `(n, 4)` with columns
    /// `[ha_open, ha_high, ha_low, ha_close]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let n = o.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(v) = self.inner.update(candle) {
                out[i * 4] = v.open;
                out[i * 4 + 1] = v.high;
                out[i * 4 + 2] = v.low;
                out[i * 4 + 3] = v.close;
            }
        }
        matrix(py, out, n, 4)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "HeikinAshi()".to_string()
    }
}

#[pyclass(name = "Variance", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyVariance {
    inner: wc::Variance,
}

#[pymethods]
impl PyVariance {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Variance::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("Variance(period={})", self.inner.period())
    }
}

// ============================== Heikin-Ashi Oscillator ==============================

#[pyclass(
    name = "HeikinAshiOscillator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyHeikinAshiOscillator {
    inner: wc::HeikinAshiOscillator,
}

#[pymethods]
impl PyHeikinAshiOscillator {
    #[new]
    #[pyo3(signature = (period=5))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::HeikinAshiOscillator::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(o.len());
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Three Line Break ==============================

#[pyclass(
    name = "ThreeLineBreak",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyThreeLineBreak {
    inner: wc::ThreeLineBreak,
}

#[pymethods]
impl PyThreeLineBreak {
    #[new]
    #[pyo3(signature = (lines=3))]
    fn new(lines: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ThreeLineBreak::new(lines).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Smoothed Heikin-Ashi ==============================

#[pyclass(
    name = "SmoothedHeikinAshi",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PySmoothedHeikinAshi {
    inner: wc::SmoothedHeikinAshi,
}

#[pymethods]
impl PySmoothedHeikinAshi {
    #[new]
    #[pyo3(signature = (period=5))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::SmoothedHeikinAshi::new(period).map_err(map_err)?,
        })
    }
    /// Returns `(open, high, low, close)`.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.open, o.high, o.low, o.close)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let n = o.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(v) = self.inner.update(candle) {
                out[i * 4] = v.open;
                out[i * 4 + 1] = v.high;
                out[i * 4 + 2] = v.low;
                out[i * 4 + 3] = v.close;
            }
        }
        matrix(py, out, n, 4)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Equivolume ==============================

#[pyclass(name = "Equivolume", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyEquivolume {
    inner: wc::Equivolume,
}

#[pymethods]
impl PyEquivolume {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Equivolume::new(period).map_err(map_err)?,
        })
    }
    /// Returns `(height, width)`.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.height, o.width)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let vol = volume.as_slice();
        if h.len() != l.len() || l.len() != vol.len() {
            return Err(PyValueError::new_err(
                "high, low, volume must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let candle = wc::Candle::new(l[i], h[i], l[i], l[i], vol[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.height;
                out[i * 2 + 1] = o.width;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== CandleVolume ==============================

#[pyclass(name = "CandleVolume", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyCandleVolume {
    inner: wc::CandleVolume,
}

#[pymethods]
impl PyCandleVolume {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::CandleVolume::new(period).map_err(map_err)?,
        })
    }
    /// Returns `(body, width)`.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.body, o.width)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let c = close.as_slice();
        let vol = volume.as_slice();
        if o.len() != c.len() || c.len() != vol.len() {
            return Err(PyValueError::new_err(
                "open, close, volume must be equal length",
            ));
        }
        let n = o.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let high = o[i].max(c[i]);
            let low = o[i].min(c[i]);
            let candle = wc::Candle::new(o[i], high, low, c[i], vol[i], 0).map_err(map_err)?;
            if let Some(v) = self.inner.update(candle) {
                out[i * 2] = v.body;
                out[i * 2 + 1] = v.width;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Frying Pan Bottom ==============================

#[pyclass(name = "FryPanBottom", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFryPanBottom {
    inner: wc::FryPanBottom,
}

#[pymethods]
impl PyFryPanBottom {
    #[new]
    #[pyo3(signature = (period=9))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::FryPanBottom::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Dumpling Top ==============================

#[pyclass(name = "DumplingTop", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyDumplingTop {
    inner: wc::DumplingTop,
}

#[pymethods]
impl PyDumplingTop {
    #[new]
    #[pyo3(signature = (period=9))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::DumplingTop::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== New Price Lines ==============================

#[pyclass(name = "NewPriceLines", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyNewPriceLines {
    inner: wc::NewPriceLines,
}

#[pymethods]
impl PyNewPriceLines {
    #[new]
    #[pyo3(signature = (count=5))]
    fn new(count: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::NewPriceLines::new(count).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== CoefficientOfVariation ==============================

#[pyclass(
    name = "CoefficientOfVariation",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyCoefficientOfVariation {
    inner: wc::CoefficientOfVariation,
}

#[pymethods]
impl PyCoefficientOfVariation {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::CoefficientOfVariation::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("CoefficientOfVariation(period={})", self.inner.period())
    }
}

// ============================== Skewness ==============================

#[pyclass(name = "Skewness", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySkewness {
    inner: wc::Skewness,
}

#[pymethods]
impl PySkewness {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Skewness::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("Skewness(period={})", self.inner.period())
    }
}

// ============================== Kurtosis ==============================

#[pyclass(name = "Kurtosis", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyKurtosis {
    inner: wc::Kurtosis,
}

#[pymethods]
impl PyKurtosis {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Kurtosis::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("Kurtosis(period={})", self.inner.period())
    }
}

// ============================== StandardError ==============================

#[pyclass(name = "StandardError", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyStandardError {
    inner: wc::StandardError,
}

#[pymethods]
impl PyStandardError {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::StandardError::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("StandardError(period={})", self.inner.period())
    }
}

// ============================== DetrendedStdDev ==============================

#[pyclass(
    name = "DetrendedStdDev",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyDetrendedStdDev {
    inner: wc::DetrendedStdDev,
}

#[pymethods]
impl PyDetrendedStdDev {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::DetrendedStdDev::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("DetrendedStdDev(period={})", self.inner.period())
    }
}

// ============================== RSquared ==============================

#[pyclass(name = "RSquared", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRSquared {
    inner: wc::RSquared,
}

#[pymethods]
impl PyRSquared {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RSquared::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RSquared(period={})", self.inner.period())
    }
}

// ============================== Autocorrelation ==============================

#[pyclass(
    name = "Autocorrelation",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAutocorrelation {
    inner: wc::Autocorrelation,
}

#[pymethods]
impl PyAutocorrelation {
    #[new]
    #[pyo3(signature = (period=20, lag=1))]
    fn new(period: usize, lag: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Autocorrelation::new(period, lag).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn lag(&self) -> usize {
        self.inner.lag()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "Autocorrelation(period={}, lag={})",
            self.inner.period(),
            self.inner.lag()
        )
    }
}

// ============================== MedianAbsoluteDeviation ==============================

#[pyclass(
    name = "MedianAbsoluteDeviation",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyMedianAbsoluteDeviation {
    inner: wc::MedianAbsoluteDeviation,
}

#[pymethods]
impl PyMedianAbsoluteDeviation {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MedianAbsoluteDeviation::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("MedianAbsoluteDeviation(period={})", self.inner.period())
    }
}

// ============================== HurstExponent ==============================

#[pyclass(name = "HurstExponent", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyHurstExponent {
    inner: wc::HurstExponent,
}

#[pymethods]
impl PyHurstExponent {
    #[new]
    #[pyo3(signature = (period=100, chunks=4))]
    fn new(period: usize, chunks: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::HurstExponent::new(period, chunks).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let s = prices.as_slice();
        f64_batch(py, s.len(), |out| self.inner.batch_nan_into(s, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn chunks(&self) -> usize {
        self.inner.chunks()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "HurstExponent(period={}, chunks={})",
            self.inner.period(),
            self.inner.chunks()
        )
    }
}

// ============================== PearsonCorrelation ==============================

#[pyclass(
    name = "PearsonCorrelation",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyPearsonCorrelation {
    inner: wc::PearsonCorrelation,
}

#[pymethods]
impl PyPearsonCorrelation {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::PearsonCorrelation::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, x: f64, y: f64) -> Option<f64> {
        self.inner.update((x, y))
    }
    /// Batch over two equally-sized series.
    fn batch<'py>(&mut self, py: Python<'py>, x: Buf1, y: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = x.as_slice();
        let ys = y.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("x and y must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("PearsonCorrelation(period={})", self.inner.period())
    }
}

// ============================== Beta ==============================

#[pyclass(name = "Beta", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyBeta {
    inner: wc::Beta,
}

#[pymethods]
impl PyBeta {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Beta::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, asset: f64, benchmark: f64) -> Option<f64> {
        self.inner.update((asset, benchmark))
    }
    /// Batch over two equally-sized series: asset and benchmark.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        asset: Buf1,
        benchmark: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let a = asset.as_slice();
        let b = benchmark.as_slice();
        if a.len() != b.len() {
            return Err(PyValueError::new_err(
                "asset and benchmark must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            out.push(self.inner.update((a[i], b[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("Beta(period={})", self.inner.period())
    }
}

// ============================== PairwiseBeta ==============================

#[pyclass(name = "PairwiseBeta", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPairwiseBeta {
    inner: wc::PairwiseBeta,
}

#[pymethods]
impl PyPairwiseBeta {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::PairwiseBeta::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized series of prices: `a` and `b`.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("PairwiseBeta(period={})", self.inner.period())
    }
}

// ============================== SpreadAr1Coefficient ==============================

#[pyclass(
    name = "SpreadAr1Coefficient",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PySpreadAr1Coefficient {
    inner: wc::SpreadAr1Coefficient,
}

#[pymethods]
impl PySpreadAr1Coefficient {
    #[new]
    #[pyo3(signature = (period=40))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::SpreadAr1Coefficient::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized series of prices: `a` and `b`.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("SpreadAr1Coefficient(period={})", self.inner.period())
    }
}

// ============================== PairSpreadZScore ==============================

#[pyclass(
    name = "PairSpreadZScore",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyPairSpreadZScore {
    inner: wc::PairSpreadZScore,
}

#[pymethods]
impl PyPairSpreadZScore {
    #[new]
    #[pyo3(signature = (beta_period=20, z_period=20))]
    fn new(beta_period: usize, z_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::PairSpreadZScore::new(beta_period, z_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized series of prices: `a` and `b`.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn beta_period(&self) -> usize {
        self.inner.beta_period()
    }
    #[getter]
    fn z_period(&self) -> usize {
        self.inner.z_period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "PairSpreadZScore(beta_period={}, z_period={})",
            self.inner.beta_period(),
            self.inner.z_period()
        )
    }
}

// ============================== LeadLagCrossCorrelation ==============================

#[pyclass(
    name = "LeadLagCrossCorrelation",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyLeadLagCrossCorrelation {
    inner: wc::LeadLagCrossCorrelation,
}

#[pymethods]
impl PyLeadLagCrossCorrelation {
    #[new]
    #[pyo3(signature = (window=20, max_lag=10))]
    fn new(window: usize, max_lag: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::LeadLagCrossCorrelation::new(window, max_lag).map_err(map_err)?,
        })
    }
    /// Returns `(lag, correlation)` or `None` during warmup. A positive lag
    /// means `a` leads `b`.
    fn update(&mut self, a: f64, b: f64) -> Option<(i64, f64)> {
        self.inner.update((a, b)).map(|o| (o.lag, o.correlation))
    }
    /// Batch over two equally-sized series. Returns a 2D array of shape
    /// `(n, 2)` with columns `[lag, correlation]`. Warmup rows are NaN.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let n = xs.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self.inner.update((xs[i], ys[i])) {
                out[i * 2] = o.lag as f64;
                out[i * 2 + 1] = o.correlation;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn window(&self) -> usize {
        self.inner.window()
    }
    #[getter]
    fn max_lag(&self) -> usize {
        self.inner.max_lag()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "LeadLagCrossCorrelation(window={}, max_lag={})",
            self.inner.window(),
            self.inner.max_lag()
        )
    }
}

// ============================== Cointegration ==============================

#[pyclass(name = "Cointegration", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyCointegration {
    inner: wc::Cointegration,
}

#[pymethods]
impl PyCointegration {
    #[new]
    #[pyo3(signature = (period=30, adf_lags=1))]
    fn new(period: usize, adf_lags: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Cointegration::new(period, adf_lags).map_err(map_err)?,
        })
    }
    /// Returns `(hedge_ratio, spread, adf_stat)` or `None` during warmup.
    fn update(&mut self, a: f64, b: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update((a, b))
            .map(|o| (o.hedge_ratio, o.spread, o.adf_stat))
    }
    /// Batch over two equally-sized series. Returns a 2D array of shape
    /// `(n, 3)` with columns `[hedge_ratio, spread, adf_stat]`. Warmup rows are
    /// NaN.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let n = xs.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update((xs[i], ys[i])) {
                out[i * 3] = o.hedge_ratio;
                out[i * 3 + 1] = o.spread;
                out[i * 3 + 2] = o.adf_stat;
            }
        }
        matrix(py, out, n, 3)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn adf_lags(&self) -> usize {
        self.inner.adf_lags()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "Cointegration(period={}, adf_lags={})",
            self.inner.period(),
            self.inner.adf_lags()
        )
    }
}

// ============================== RelativeStrengthAB ==============================

#[pyclass(
    name = "RelativeStrengthAB",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyRelativeStrengthAB {
    inner: wc::RelativeStrengthAB,
}

#[pymethods]
impl PyRelativeStrengthAB {
    #[new]
    #[pyo3(signature = (ma_period=20, rsi_period=14))]
    fn new(ma_period: usize, rsi_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RelativeStrengthAB::new(ma_period, rsi_period).map_err(map_err)?,
        })
    }
    /// Returns `(ratio, ratio_ma, ratio_rsi)` or `None` during warmup.
    fn update(&mut self, a: f64, b: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update((a, b))
            .map(|o| (o.ratio, o.ratio_ma, o.ratio_rsi))
    }
    /// Batch over two equally-sized series. Returns a 2D array of shape
    /// `(n, 3)` with columns `[ratio, ratio_ma, ratio_rsi]`. Warmup rows are
    /// NaN.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let n = xs.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update((xs[i], ys[i])) {
                out[i * 3] = o.ratio;
                out[i * 3 + 1] = o.ratio_ma;
                out[i * 3 + 2] = o.ratio_rsi;
            }
        }
        matrix(py, out, n, 3)
    }
    #[getter]
    fn ma_period(&self) -> usize {
        self.inner.ma_period()
    }
    #[getter]
    fn rsi_period(&self) -> usize {
        self.inner.rsi_period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "RelativeStrengthAB(ma_period={}, rsi_period={})",
            self.inner.ma_period(),
            self.inner.rsi_period()
        )
    }
}

// ============================== RollingCorrelation ==============================

#[pyclass(
    name = "RollingCorrelation",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyRollingCorrelation {
    inner: wc::RollingCorrelation,
}

#[pymethods]
impl PyRollingCorrelation {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RollingCorrelation::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized series: `a` and `b`.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RollingCorrelation(period={})", self.inner.period())
    }
}

// ========================= HasbrouckInformationShare =========================

#[pyclass(
    name = "HasbrouckInformationShare",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyHasbrouckInformationShare {
    inner: wc::HasbrouckInformationShare,
}

#[pymethods]
impl PyHasbrouckInformationShare {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::HasbrouckInformationShare::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized series: `a` and `b`.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("HasbrouckInformationShare(period={})", self.inner.period())
    }
}

// ============================== RollingCovariance ==============================

#[pyclass(
    name = "RollingCovariance",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyRollingCovariance {
    inner: wc::RollingCovariance,
}

#[pymethods]
impl PyRollingCovariance {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RollingCovariance::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized series: `a` and `b`.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RollingCovariance(period={})", self.inner.period())
    }
}

// ============================== OuHalfLife ==============================

#[pyclass(name = "OuHalfLife", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyOuHalfLife {
    inner: wc::OuHalfLife,
}

#[pymethods]
impl PyOuHalfLife {
    #[new]
    #[pyo3(signature = (period=60))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::OuHalfLife::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized series: `a` and `b`.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("OuHalfLife(period={})", self.inner.period())
    }
}

// ============================== SpreadHurst ==============================

#[pyclass(name = "SpreadHurst", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySpreadHurst {
    inner: wc::SpreadHurst,
}

#[pymethods]
impl PySpreadHurst {
    #[new]
    #[pyo3(signature = (period=60))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::SpreadHurst::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized series: `a` and `b`.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("SpreadHurst(period={})", self.inner.period())
    }
}

// ============================== DistanceSsd ==============================

#[pyclass(name = "DistanceSsd", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyDistanceSsd {
    inner: wc::DistanceSsd,
}

#[pymethods]
impl PyDistanceSsd {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::DistanceSsd::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized series: `a` and `b`.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("DistanceSsd(period={})", self.inner.period())
    }
}

// ============================== BetaNeutralSpread ==============================

#[pyclass(
    name = "BetaNeutralSpread",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyBetaNeutralSpread {
    inner: wc::BetaNeutralSpread,
}

#[pymethods]
impl PyBetaNeutralSpread {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::BetaNeutralSpread::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized series: `a` and `b`.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("BetaNeutralSpread(period={})", self.inner.period())
    }
}

// ============================== VarianceRatio ==============================

#[pyclass(name = "VarianceRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyVarianceRatio {
    inner: wc::VarianceRatio,
}

#[pymethods]
impl PyVarianceRatio {
    #[new]
    #[pyo3(signature = (period=60, q=2))]
    fn new(period: usize, q: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VarianceRatio::new(period, q).map_err(map_err)?,
        })
    }
    fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized series: `a` and `b`.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn q(&self) -> usize {
        self.inner.q()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "VarianceRatio(period={}, q={})",
            self.inner.period(),
            self.inner.q()
        )
    }
}

// ============================== GrangerCausality ==============================

#[pyclass(
    name = "GrangerCausality",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyGrangerCausality {
    inner: wc::GrangerCausality,
}

#[pymethods]
impl PyGrangerCausality {
    #[new]
    #[pyo3(signature = (period=60, lag=1))]
    fn new(period: usize, lag: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::GrangerCausality::new(period, lag).map_err(map_err)?,
        })
    }
    fn update(&mut self, a: f64, b: f64) -> Option<f64> {
        self.inner.update((a, b))
    }
    /// Batch over two equally-sized series: `a` and `b`.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn lag(&self) -> usize {
        self.inner.lag()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "GrangerCausality(period={}, lag={})",
            self.inner.period(),
            self.inner.lag()
        )
    }
}

// ============================== KalmanHedgeRatio ==============================

#[pyclass(
    name = "KalmanHedgeRatio",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyKalmanHedgeRatio {
    inner: wc::KalmanHedgeRatio,
}

#[pymethods]
impl PyKalmanHedgeRatio {
    #[new]
    #[pyo3(signature = (delta=1e-4, observation_var=1e-3))]
    fn new(delta: f64, observation_var: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::KalmanHedgeRatio::new(delta, observation_var).map_err(map_err)?,
        })
    }
    /// Returns `(hedge_ratio, intercept, spread)` or `None` during warmup.
    fn update(&mut self, a: f64, b: f64) -> Option<(f64, f64, f64)> {
        self.inner
            .update((a, b))
            .map(|o| (o.hedge_ratio, o.intercept, o.spread))
    }
    /// Batch over two equally-sized series. Returns a 2D array of shape
    /// `(n, 3)` with columns `[hedge_ratio, intercept, spread]`. Warmup rows are
    /// NaN.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let n = xs.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self.inner.update((xs[i], ys[i])) {
                out[i * 3] = o.hedge_ratio;
                out[i * 3 + 1] = o.intercept;
                out[i * 3 + 2] = o.spread;
            }
        }
        matrix(py, out, n, 3)
    }
    #[getter]
    fn delta(&self) -> f64 {
        self.inner.delta()
    }
    #[getter]
    fn observation_var(&self) -> f64 {
        self.inner.observation_var()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "KalmanHedgeRatio(delta={}, observation_var={})",
            self.inner.delta(),
            self.inner.observation_var()
        )
    }
}

// ============================== SpreadBollingerBands ==============================

#[pyclass(
    name = "SpreadBollingerBands",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PySpreadBollingerBands {
    inner: wc::SpreadBollingerBands,
}

#[pymethods]
impl PySpreadBollingerBands {
    #[new]
    #[pyo3(signature = (period=20, num_std=2.0))]
    fn new(period: usize, num_std: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::SpreadBollingerBands::new(period, num_std).map_err(map_err)?,
        })
    }
    /// Returns `(middle, upper, lower, percent_b)` or `None` during warmup.
    fn update(&mut self, a: f64, b: f64) -> Option<(f64, f64, f64, f64)> {
        self.inner
            .update((a, b))
            .map(|o| (o.middle, o.upper, o.lower, o.percent_b))
    }
    /// Batch over two equally-sized series. Returns a 2D array of shape
    /// `(n, 4)` with columns `[middle, upper, lower, percent_b]`. Warmup rows are
    /// NaN.
    fn batch<'py>(&mut self, py: Python<'py>, a: Buf1, b: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = a.as_slice();
        let ys = b.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("a and b must be equal length"));
        }
        let n = xs.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            if let Some(o) = self.inner.update((xs[i], ys[i])) {
                out[i * 4] = o.middle;
                out[i * 4 + 1] = o.upper;
                out[i * 4 + 2] = o.lower;
                out[i * 4 + 3] = o.percent_b;
            }
        }
        matrix(py, out, n, 4)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn num_std(&self) -> f64 {
        self.inner.num_std()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "SpreadBollingerBands(period={}, num_std={})",
            self.inner.period(),
            self.inner.num_std()
        )
    }
}

// ============================== SpearmanCorrelation ==============================

#[pyclass(
    name = "SpearmanCorrelation",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PySpearmanCorrelation {
    inner: wc::SpearmanCorrelation,
}

#[pymethods]
impl PySpearmanCorrelation {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::SpearmanCorrelation::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, x: f64, y: f64) -> Option<f64> {
        self.inner.update((x, y))
    }
    /// Batch over two equally-sized series.
    fn batch<'py>(&mut self, py: Python<'py>, x: Buf1, y: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = x.as_slice();
        let ys = y.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("x and y must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("SpearmanCorrelation(period={})", self.inner.period())
    }
}

// ============================== ValueArea ==============================

#[pyclass(name = "ValueArea", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyValueArea {
    inner: wc::ValueArea,
}

#[pymethods]
impl PyValueArea {
    #[new]
    #[pyo3(signature = (period=20, bin_count=50, value_area_pct=0.70))]
    fn new(period: usize, bin_count: usize, value_area_pct: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ValueArea::new(period, bin_count, value_area_pct).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.poc, o.vah, o.val)))
    }
    /// Batch over input columns high, low, volume. Returns shape `(n, 3)`
    /// with columns `[poc, vah, val]`; warmup rows are `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, volume must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            // open / close pinned to the midpoint so the candle validates.
            let mid = f64::midpoint(h[i], l[i]);
            let candle = wc::Candle::new(mid, h[i], l[i], mid, v[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.poc;
                out[i * 3 + 1] = o.vah;
                out[i * 3 + 2] = o.val;
            }
        }
        matrix(py, out, n, 3)
    }
    #[getter]
    fn params(&self) -> (usize, usize, f64) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (period, bin_count, pct) = self.inner.params();
        format!("ValueArea(period={period}, bin_count={bin_count}, value_area_pct={pct})")
    }
}

// ============================== VolumeProfile ==============================

/// Streaming profile output: `(price_low, price_high, per_bin_values)`, or `None`
/// during warmup. Shared by `VolumeProfile` (volume bins) and `TpoProfile` (TPO
/// counts).
type ProfileHistogram<'py> = Option<(f64, f64, Bound<'py, PyAny>)>;

#[pyclass(name = "VolumeProfile", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyVolumeProfile {
    inner: wc::VolumeProfile,
}

#[pymethods]
impl PyVolumeProfile {
    #[new]
    #[pyo3(signature = (period=20, bin_count=50))]
    fn new(period: usize, bin_count: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VolumeProfile::new(period, bin_count).map_err(map_err)?,
        })
    }
    /// Streaming update. Returns `(price_low, price_high, bins)` once warm, else `None`.
    fn update<'py>(
        &mut self,
        py: Python<'py>,
        candle: &Bound<'_, PyAny>,
    ) -> PyResult<ProfileHistogram<'py>> {
        let c = extract_candle(candle)?;
        self.inner
            .update(c)
            .map(|o| PyResult::Ok((o.price_low, o.price_high, f64_array(py, &o.bins)?)))
            .transpose()
    }
    /// Batch over input columns high, low, volume. Returns shape `(n, bin_count + 2)`
    /// with columns `[price_low, price_high, bin_0, ..., bin_{k-1}]`; warmup rows are `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, volume must be equal length",
            ));
        }
        let k = self.inner.params().1 + 2;
        let n = h.len();
        let mut out = vec![f64::NAN; n * k];
        for i in 0..n {
            let mid = f64::midpoint(h[i], l[i]);
            let candle = wc::Candle::new(mid, h[i], l[i], mid, v[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * k] = o.price_low;
                out[i * k + 1] = o.price_high;
                for (j, b) in o.bins.iter().enumerate() {
                    out[i * k + 2 + j] = *b;
                }
            }
        }
        matrix(py, out, n, k)
    }
    #[getter]
    fn params(&self) -> (usize, usize) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (period, bin_count) = self.inner.params();
        format!("VolumeProfile(period={period}, bin_count={bin_count})")
    }
}

// ============================== TpoProfile ==============================

#[pyclass(name = "TpoProfile", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTpoProfile {
    inner: wc::TpoProfile,
}

#[pymethods]
impl PyTpoProfile {
    #[new]
    #[pyo3(signature = (period=30, bin_count=50))]
    fn new(period: usize, bin_count: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TpoProfile::new(period, bin_count).map_err(map_err)?,
        })
    }
    /// Streaming update. Returns `(price_low, price_high, counts)` once warm, else `None`.
    fn update<'py>(
        &mut self,
        py: Python<'py>,
        candle: &Bound<'_, PyAny>,
    ) -> PyResult<ProfileHistogram<'py>> {
        let c = extract_candle(candle)?;
        self.inner
            .update(c)
            .map(|o| PyResult::Ok((o.price_low, o.price_high, f64_array(py, &o.counts)?)))
            .transpose()
    }
    /// Batch over input columns high, low. Returns shape `(n, bin_count + 2)`
    /// with columns `[price_low, price_high, count_0, ..., count_{k-1}]`; warmup rows are `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high, low must be equal length"));
        }
        let k = self.inner.params().1 + 2;
        let n = h.len();
        let mut out = vec![f64::NAN; n * k];
        for i in 0..n {
            let mid = f64::midpoint(h[i], l[i]);
            let candle = wc::Candle::new(mid, h[i], l[i], mid, 1.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * k] = o.price_low;
                out[i * k + 1] = o.price_high;
                for (j, count) in o.counts.iter().enumerate() {
                    out[i * k + 2 + j] = *count;
                }
            }
        }
        matrix(py, out, n, k)
    }
    #[getter]
    fn params(&self) -> (usize, usize) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (period, bin_count) = self.inner.params();
        format!("TpoProfile(period={period}, bin_count={bin_count})")
    }
}

// ============================== InitialBalance ==============================

#[pyclass(
    name = "InitialBalance",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyInitialBalance {
    inner: wc::InitialBalance,
}

#[pymethods]
impl PyInitialBalance {
    #[new]
    #[pyo3(signature = (period=12))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::InitialBalance::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.high, o.low)))
    }
    /// Batch over input columns high, low. Returns shape `(n, 2)` with
    /// columns `[high, low]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let mid = f64::midpoint(h[i], l[i]);
            let candle = wc::Candle::new(mid, h[i], l[i], mid, 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.high;
                out[i * 2 + 1] = o.low;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("InitialBalance(period={})", self.inner.period())
    }
}

// ============================== OpeningRange ==============================

#[pyclass(name = "OpeningRange", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyOpeningRange {
    inner: wc::OpeningRange,
}

#[pymethods]
impl PyOpeningRange {
    #[new]
    #[pyo3(signature = (period=6))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::OpeningRange::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.high, o.low, o.breakout_distance)))
    }
    /// Batch over input columns high, low, close. Returns shape `(n, 3)`
    /// with columns `[high, low, breakout_distance]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.high;
                out[i * 3 + 1] = o.low;
                out[i * 3 + 2] = o.breakout_distance;
            }
        }
        matrix(py, out, n, 3)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("OpeningRange(period={})", self.inner.period())
    }
}

// Naked POC: most recent untouched point-of-control level (Candle -> f64).
#[pyclass(name = "NakedPoc", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyNakedPoc {
    inner: wc::NakedPoc,
}

#[pymethods]
impl PyNakedPoc {
    #[new]
    #[pyo3(signature = (session_len=20, bin_count=24))]
    fn new(session_len: usize, bin_count: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::NakedPoc::new(session_len, bin_count).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high, low, close, volume series (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (s, b) = self.inner.params();
        format!("NakedPoc(session_len={s}, bin_count={b})")
    }
}

// Single Prints: count of single-print price levels in the profile (Candle -> f64).
#[pyclass(name = "SinglePrints", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySinglePrints {
    inner: wc::SinglePrints,
}

#[pymethods]
impl PySinglePrints {
    #[new]
    #[pyo3(signature = (period=20, bin_count=24))]
    fn new(period: usize, bin_count: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::SinglePrints::new(period, bin_count).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high, low series (1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let mid = f64::midpoint(h[i], l[i]);
            let candle = wc::Candle::new(mid, h[i], l[i], mid, 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, b) = self.inner.params();
        format!("SinglePrints(period={p}, bin_count={b})")
    }
}

// Profile Shape: b/P/D shape classification as a numeric code (Candle -> f64).
#[pyclass(name = "ProfileShape", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyProfileShape {
    inner: wc::ProfileShape,
}

#[pymethods]
impl PyProfileShape {
    #[new]
    #[pyo3(signature = (period=20, bin_count=24))]
    fn new(period: usize, bin_count: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ProfileShape::new(period, bin_count).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high, low, volume series (1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(h.len());
        for i in 0..h.len() {
            let mid = f64::midpoint(h[i], l[i]);
            let candle = wc::Candle::new(mid, h[i], l[i], mid, v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, b) = self.inner.params();
        format!("ProfileShape(period={p}, bin_count={b})")
    }
}

// High/Low Volume Nodes: highest- and lowest-volume price nodes (Candle -> struct).
#[pyclass(
    name = "HighLowVolumeNodes",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyHighLowVolumeNodes {
    inner: wc::HighLowVolumeNodes,
}

#[pymethods]
impl PyHighLowVolumeNodes {
    #[new]
    #[pyo3(signature = (period=20, bin_count=24))]
    fn new(period: usize, bin_count: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::HighLowVolumeNodes::new(period, bin_count).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.hvn, o.lvn)))
    }
    /// Batch over high, low, volume columns. Returns shape `(n, 2)` `[hvn, lvn]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, volume must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            let mid = f64::midpoint(h[i], l[i]);
            let candle = wc::Candle::new(mid, h[i], l[i], mid, v[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 2] = o.hvn;
                out[i * 2 + 1] = o.lvn;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, b) = self.inner.params();
        format!("HighLowVolumeNodes(period={p}, bin_count={b})")
    }
}

// Composite Profile: multi-session composite volume profile (Candle -> struct).
#[pyclass(
    name = "CompositeProfile",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyCompositeProfile {
    inner: wc::CompositeProfile,
}

#[pymethods]
impl PyCompositeProfile {
    #[new]
    #[pyo3(signature = (period=20, bin_count=24, value_area_pct=0.70))]
    fn new(period: usize, bin_count: usize, value_area_pct: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::CompositeProfile::new(period, bin_count, value_area_pct).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.poc, o.vah, o.val)))
    }
    /// Batch over high, low, volume columns. Returns shape `(n, 3)` `[poc, vah, val]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let v = volume.as_slice();
        if h.len() != l.len() || l.len() != v.len() {
            return Err(PyValueError::new_err(
                "high, low, volume must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let mid = f64::midpoint(h[i], l[i]);
            let candle = wc::Candle::new(mid, h[i], l[i], mid, v[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.poc;
                out[i * 3 + 1] = o.vah;
                out[i * 3 + 2] = o.val;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (p, b, pct) = self.inner.params();
        format!("CompositeProfile(period={p}, bin_count={b}, value_area_pct={pct})")
    }
}

// ============================== Candlestick Patterns ==============================
//
// All 15 patterns take Candles and emit a signed f64 signal per bar:
//   +1.0 bullish, -1.0 bearish, 0.0 no pattern. Doji is direction-less by
// default (+1.0 / 0.0); construct it with `signed=True` for the
// dragonfly/gravestone signed +-1 encoding.

macro_rules! candle_pattern_no_param {
    ($name:ident, $inner:ty, $repr:expr) => {
        #[pyclass(name = $repr, module = "wickra._wickra", skip_from_py_object)]
        #[derive(Clone)]
        struct $name {
            inner: $inner,
        }

        #[pymethods]
        impl $name {
            #[new]
            fn new() -> Self {
                Self {
                    inner: <$inner>::new(),
                }
            }
            fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
                let c = extract_candle(candle)?;
                Ok(self.inner.update(c))
            }
            fn batch<'py>(
                &mut self,
                py: Python<'py>,
                open: Buf1,
                high: Buf1,
                low: Buf1,
                close: Buf1,
            ) -> PyResult<Bound<'py, PyAny>> {
                let o = open.as_slice();
                let h = high.as_slice();
                let l = low.as_slice();
                let c = close.as_slice();
                if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
                    return Err(PyValueError::new_err(
                        "open, high, low, close must be equal length",
                    ));
                }
                let mut out = Vec::with_capacity(o.len());
                for i in 0..o.len() {
                    let candle =
                        wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
                    out.push(self.inner.update(candle).unwrap_or(f64::NAN));
                }
                out.into_pydata(py)
            }
            fn reset(&mut self) {
                self.inner.reset();
            }

            fn name(&self) -> &'static str {
                self.inner.name()
            }
            fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
            fn __repr__(&self) -> String {
                format!("{}()", $repr)
            }
        }
    };
}

// Doji is the one pattern with an opt-in signed mode, so it is hand-written
// rather than generated by `candle_pattern_no_param!`.
#[pyclass(name = "Doji", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyDoji {
    inner: wc::Doji,
}

#[pymethods]
impl PyDoji {
    #[new]
    #[pyo3(signature = (signed = false))]
    fn new(signed: bool) -> Self {
        let inner = if signed {
            wc::Doji::new().signed()
        } else {
            wc::Doji::new()
        };
        Self { inner }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o = open.as_slice();
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if o.len() != h.len() || h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "open, high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(o.len());
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn is_signed(&self) -> bool {
        self.inner.is_signed()
    }
    fn __repr__(&self) -> String {
        format!(
            "Doji(signed={})",
            if self.inner.is_signed() {
                "True"
            } else {
                "False"
            }
        )
    }
}

candle_pattern_no_param!(PyHammer, wc::Hammer, "Hammer");
candle_pattern_no_param!(PyInvertedHammer, wc::InvertedHammer, "InvertedHammer");
candle_pattern_no_param!(PyHangingMan, wc::HangingMan, "HangingMan");
candle_pattern_no_param!(PyShootingStar, wc::ShootingStar, "ShootingStar");
candle_pattern_no_param!(PyEngulfing, wc::Engulfing, "Engulfing");
candle_pattern_no_param!(PyHarami, wc::Harami, "Harami");
candle_pattern_no_param!(
    PyMorningEveningStar,
    wc::MorningEveningStar,
    "MorningEveningStar"
);
candle_pattern_no_param!(
    PyThreeSoldiersOrCrows,
    wc::ThreeSoldiersOrCrows,
    "ThreeSoldiersOrCrows"
);
candle_pattern_no_param!(
    PyPiercingDarkCloud,
    wc::PiercingDarkCloud,
    "PiercingDarkCloud"
);
candle_pattern_no_param!(PyMarubozu, wc::Marubozu, "Marubozu");
candle_pattern_no_param!(PyTweezer, wc::Tweezer, "Tweezer");
candle_pattern_no_param!(PySpinningTop, wc::SpinningTop, "SpinningTop");
candle_pattern_no_param!(PyThreeInside, wc::ThreeInside, "ThreeInside");
candle_pattern_no_param!(PyThreeOutside, wc::ThreeOutside, "ThreeOutside");
candle_pattern_no_param!(PyTwoCrows, wc::TwoCrows, "TwoCrows");
candle_pattern_no_param!(
    PyUpsideGapTwoCrows,
    wc::UpsideGapTwoCrows,
    "UpsideGapTwoCrows"
);
candle_pattern_no_param!(
    PyIdenticalThreeCrows,
    wc::IdenticalThreeCrows,
    "IdenticalThreeCrows"
);
candle_pattern_no_param!(PyThreeLineStrike, wc::ThreeLineStrike, "ThreeLineStrike");
candle_pattern_no_param!(
    PyThreeStarsInSouth,
    wc::ThreeStarsInSouth,
    "ThreeStarsInSouth"
);
candle_pattern_no_param!(PyAbandonedBaby, wc::AbandonedBaby, "AbandonedBaby");
candle_pattern_no_param!(PyAdvanceBlock, wc::AdvanceBlock, "AdvanceBlock");

candle_pattern_no_param!(PyBeltHold, wc::BeltHold, "BeltHold");
candle_pattern_no_param!(PyBreakaway, wc::Breakaway, "Breakaway");
candle_pattern_no_param!(PyCounterattack, wc::Counterattack, "Counterattack");
candle_pattern_no_param!(PyDojiStar, wc::DojiStar, "DojiStar");
candle_pattern_no_param!(PyDragonflyDoji, wc::DragonflyDoji, "DragonflyDoji");
candle_pattern_no_param!(PyGravestoneDoji, wc::GravestoneDoji, "GravestoneDoji");
candle_pattern_no_param!(PyLongLeggedDoji, wc::LongLeggedDoji, "LongLeggedDoji");
candle_pattern_no_param!(PyRickshawMan, wc::RickshawMan, "RickshawMan");
candle_pattern_no_param!(PyEveningDojiStar, wc::EveningDojiStar, "EveningDojiStar");
candle_pattern_no_param!(PyMorningDojiStar, wc::MorningDojiStar, "MorningDojiStar");
candle_pattern_no_param!(
    PyGapSideBySideWhite,
    wc::GapSideBySideWhite,
    "GapSideBySideWhite"
);
candle_pattern_no_param!(PyHighWave, wc::HighWave, "HighWave");
candle_pattern_no_param!(PyHikkake, wc::Hikkake, "Hikkake");
candle_pattern_no_param!(PyHikkakeModified, wc::HikkakeModified, "HikkakeModified");
candle_pattern_no_param!(PyHomingPigeon, wc::HomingPigeon, "HomingPigeon");
candle_pattern_no_param!(PyOnNeck, wc::OnNeck, "OnNeck");
candle_pattern_no_param!(PyInNeck, wc::InNeck, "InNeck");
candle_pattern_no_param!(PyThrusting, wc::Thrusting, "Thrusting");
candle_pattern_no_param!(PySeparatingLines, wc::SeparatingLines, "SeparatingLines");
candle_pattern_no_param!(PyKicking, wc::Kicking, "Kicking");
candle_pattern_no_param!(PyKickingByLength, wc::KickingByLength, "KickingByLength");
candle_pattern_no_param!(PyLadderBottom, wc::LadderBottom, "LadderBottom");
candle_pattern_no_param!(PyMatHold, wc::MatHold, "MatHold");
candle_pattern_no_param!(PyMatchingLow, wc::MatchingLow, "MatchingLow");
candle_pattern_no_param!(PyLongLine, wc::LongLine, "LongLine");
candle_pattern_no_param!(PyShortLine, wc::ShortLine, "ShortLine");
candle_pattern_no_param!(
    PyRisingThreeMethods,
    wc::RisingThreeMethods,
    "RisingThreeMethods"
);
candle_pattern_no_param!(
    PyFallingThreeMethods,
    wc::FallingThreeMethods,
    "FallingThreeMethods"
);
candle_pattern_no_param!(
    PyUpsideGapThreeMethods,
    wc::UpsideGapThreeMethods,
    "UpsideGapThreeMethods"
);
candle_pattern_no_param!(
    PyDownsideGapThreeMethods,
    wc::DownsideGapThreeMethods,
    "DownsideGapThreeMethods"
);
candle_pattern_no_param!(PyStalledPattern, wc::StalledPattern, "StalledPattern");
candle_pattern_no_param!(PyStickSandwich, wc::StickSandwich, "StickSandwich");
candle_pattern_no_param!(PyTakuri, wc::Takuri, "Takuri");
candle_pattern_no_param!(PyClosingMarubozu, wc::ClosingMarubozu, "ClosingMarubozu");
candle_pattern_no_param!(PyOpeningMarubozu, wc::OpeningMarubozu, "OpeningMarubozu");
candle_pattern_no_param!(PyTasukiGap, wc::TasukiGap, "TasukiGap");
candle_pattern_no_param!(PyUniqueThreeRiver, wc::UniqueThreeRiver, "UniqueThreeRiver");
candle_pattern_no_param!(
    PyConcealingBabySwallow,
    wc::ConcealingBabySwallow,
    "ConcealingBabySwallow"
);
candle_pattern_no_param!(PyDoubleTopBottom, wc::DoubleTopBottom, "DoubleTopBottom");
candle_pattern_no_param!(PyTripleTopBottom, wc::TripleTopBottom, "TripleTopBottom");
candle_pattern_no_param!(PyHeadAndShoulders, wc::HeadAndShoulders, "HeadAndShoulders");
candle_pattern_no_param!(PyTriangle, wc::Triangle, "Triangle");
candle_pattern_no_param!(PyWedge, wc::Wedge, "Wedge");
candle_pattern_no_param!(PyFlagPennant, wc::FlagPennant, "FlagPennant");
candle_pattern_no_param!(PyRectangleRange, wc::RectangleRange, "RectangleRange");
candle_pattern_no_param!(PyCupAndHandle, wc::CupAndHandle, "CupAndHandle");
candle_pattern_no_param!(PyAbcd, wc::Abcd, "Abcd");
candle_pattern_no_param!(PyGartley, wc::Gartley, "Gartley");
candle_pattern_no_param!(PyButterfly, wc::Butterfly, "Butterfly");
candle_pattern_no_param!(PyBat, wc::Bat, "Bat");
candle_pattern_no_param!(PyCrab, wc::Crab, "Crab");
candle_pattern_no_param!(PyShark, wc::Shark, "Shark");
candle_pattern_no_param!(PyCypher, wc::Cypher, "Cypher");
candle_pattern_no_param!(PyThreeDrives, wc::ThreeDrives, "ThreeDrives");
candle_pattern_no_param!(PyTdCamouflage, wc::TdCamouflage, "TDCamouflage");
candle_pattern_no_param!(PyTdClop, wc::TdClop, "TDClop");
candle_pattern_no_param!(PyTdClopwin, wc::TdClopwin, "TDClopwin");
candle_pattern_no_param!(PyTdPropulsion, wc::TdPropulsion, "TDPropulsion");
candle_pattern_no_param!(PyTdTrap, wc::TdTrap, "TDTrap");
candle_pattern_no_param!(PyTristar, wc::Tristar, "Tristar");
candle_pattern_no_param!(PyHaramiCross, wc::HaramiCross, "HaramiCross");
candle_pattern_no_param!(PyTowerTopBottom, wc::TowerTopBottom, "TowerTopBottom");
// ============================== Microstructure: Order Book ==============================
//
// Order-book indicators consume a depth snapshot rather than OHLCV. Streaming
// `update(bid_px, bid_sz, ask_px, ask_sz)` takes four equal-length sequences
// describing one snapshot (bids best-first = descending price, asks best-first
// = ascending price); `batch` takes a list of such `(bid_px, bid_sz, ask_px,
// ask_sz)` tuples and returns one value per snapshot.

fn build_order_book(
    bid_px: &[f64],
    bid_sz: &[f64],
    ask_px: &[f64],
    ask_sz: &[f64],
) -> PyResult<wc::OrderBook> {
    if bid_px.len() != bid_sz.len() || ask_px.len() != ask_sz.len() {
        return Err(PyValueError::new_err(
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

macro_rules! py_ob_indicator {
    ($name:ident, $inner:ty, $repr:expr) => {
        #[pyclass(name = $repr, module = "wickra._wickra", skip_from_py_object)]
        #[derive(Clone)]
        struct $name {
            inner: $inner,
        }

        #[pymethods]
        impl $name {
            #[new]
            fn new() -> Self {
                Self {
                    inner: <$inner>::new(),
                }
            }
            fn update(
                &mut self,
                bid_px: Vec<f64>,
                bid_sz: Vec<f64>,
                ask_px: Vec<f64>,
                ask_sz: Vec<f64>,
            ) -> PyResult<Option<f64>> {
                let book = build_order_book(&bid_px, &bid_sz, &ask_px, &ask_sz)?;
                Ok(self.inner.update(book))
            }
            #[allow(clippy::type_complexity)]
            fn batch<'py>(
                &mut self,
                py: Python<'py>,
                snapshots: Vec<(Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>)>,
            ) -> PyResult<Bound<'py, PyAny>> {
                let mut out = Vec::with_capacity(snapshots.len());
                for (bid_px, bid_sz, ask_px, ask_sz) in &snapshots {
                    let book = build_order_book(bid_px, bid_sz, ask_px, ask_sz)?;
                    out.push(self.inner.update(book).unwrap_or(f64::NAN));
                }
                out.into_pydata(py)
            }
            fn reset(&mut self) {
                self.inner.reset();
            }

            fn name(&self) -> &'static str {
                self.inner.name()
            }
            fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
            fn __repr__(&self) -> String {
                format!("{}()", $repr)
            }
        }
    };
}

py_ob_indicator!(
    PyOrderBookImbalanceTop1,
    wc::OrderBookImbalanceTop1,
    "OrderBookImbalanceTop1"
);
py_ob_indicator!(
    PyOrderBookImbalanceFull,
    wc::OrderBookImbalanceFull,
    "OrderBookImbalanceFull"
);
py_ob_indicator!(PyMicroprice, wc::Microprice, "Microprice");
py_ob_indicator!(PyQuotedSpread, wc::QuotedSpread, "QuotedSpread");
py_ob_indicator!(PyDepthSlope, wc::DepthSlope, "DepthSlope");

// Top-N imbalance carries a `levels` parameter, so it is hand-written.
#[pyclass(
    name = "OrderBookImbalanceTopN",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyOrderBookImbalanceTopN {
    inner: wc::OrderBookImbalanceTopN,
}

#[pymethods]
impl PyOrderBookImbalanceTopN {
    #[new]
    fn new(levels: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::OrderBookImbalanceTopN::new(levels).map_err(map_err)?,
        })
    }
    fn update(
        &mut self,
        bid_px: Vec<f64>,
        bid_sz: Vec<f64>,
        ask_px: Vec<f64>,
        ask_sz: Vec<f64>,
    ) -> PyResult<Option<f64>> {
        let book = build_order_book(&bid_px, &bid_sz, &ask_px, &ask_sz)?;
        Ok(self.inner.update(book))
    }
    #[allow(clippy::type_complexity)]
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        snapshots: Vec<(Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>)>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut out = Vec::with_capacity(snapshots.len());
        for (bid_px, bid_sz, ask_px, ask_sz) in &snapshots {
            let book = build_order_book(bid_px, bid_sz, ask_px, ask_sz)?;
            out.push(self.inner.update(book).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("OrderBookImbalanceTopN(levels={})", self.inner.levels())
    }
}

// ============================== Microstructure: Trade Flow ==============================
//
// Trade-flow indicators consume a trade tape rather than OHLCV. Streaming
// `update(price, size, is_buy)` takes one trade (`is_buy=True` for a
// buyer-initiated trade); `batch` takes three equal-length arrays.

fn build_trade(price: f64, size: f64, is_buy: bool) -> PyResult<wc::Trade> {
    let side = if is_buy {
        wc::Side::Buy
    } else {
        wc::Side::Sell
    };
    wc::Trade::new(price, size, side, 0).map_err(map_err)
}

macro_rules! py_trade_indicator {
    ($name:ident, $inner:ty, $repr:expr) => {
        #[pyclass(name = $repr, module = "wickra._wickra", skip_from_py_object)]
        #[derive(Clone)]
        struct $name {
            inner: $inner,
        }

        #[pymethods]
        impl $name {
            #[new]
            fn new() -> Self {
                Self {
                    inner: <$inner>::new(),
                }
            }
            fn update(&mut self, price: f64, size: f64, is_buy: bool) -> PyResult<Option<f64>> {
                Ok(self.inner.update(build_trade(price, size, is_buy)?))
            }
            fn batch<'py>(
                &mut self,
                py: Python<'py>,
                price: Vec<f64>,
                size: Vec<f64>,
                is_buy: Vec<bool>,
            ) -> PyResult<Bound<'py, PyAny>> {
                if price.len() != size.len() || size.len() != is_buy.len() {
                    return Err(PyValueError::new_err(
                        "price, size, is_buy must be equal length",
                    ));
                }
                let mut out = Vec::with_capacity(price.len());
                for i in 0..price.len() {
                    let trade = build_trade(price[i], size[i], is_buy[i])?;
                    out.push(self.inner.update(trade).unwrap_or(f64::NAN));
                }
                out.into_pydata(py)
            }
            fn reset(&mut self) {
                self.inner.reset();
            }

            fn name(&self) -> &'static str {
                self.inner.name()
            }
            fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
            fn __repr__(&self) -> String {
                format!("{}()", $repr)
            }
        }
    };
}

py_trade_indicator!(PySignedVolume, wc::SignedVolume, "SignedVolume");
py_trade_indicator!(
    PyCumulativeVolumeDelta,
    wc::CumulativeVolumeDelta,
    "CumulativeVolumeDelta"
);

// Trade imbalance carries a `window` parameter, so it is hand-written.
#[pyclass(
    name = "TradeImbalance",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyTradeImbalance {
    inner: wc::TradeImbalance,
}

#[pymethods]
impl PyTradeImbalance {
    #[new]
    fn new(window: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TradeImbalance::new(window).map_err(map_err)?,
        })
    }
    fn update(&mut self, price: f64, size: f64, is_buy: bool) -> PyResult<Option<f64>> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        price: Vec<f64>,
        size: Vec<f64>,
        is_buy: Vec<bool>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(PyValueError::new_err(
                "price, size, is_buy must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let trade = build_trade(price[i], size[i], is_buy[i])?;
            out.push(self.inner.update(trade).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TradeImbalance(window={})", self.inner.window())
    }
}

// Trade-sign autocorrelation carries a `period` parameter, so it is hand-written.
#[pyclass(
    name = "TradeSignAutocorrelation",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyTradeSignAutocorrelation {
    inner: wc::TradeSignAutocorrelation,
}

#[pymethods]
impl PyTradeSignAutocorrelation {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TradeSignAutocorrelation::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, price: f64, size: f64, is_buy: bool) -> PyResult<Option<f64>> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        price: Vec<f64>,
        size: Vec<f64>,
        is_buy: Vec<bool>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(PyValueError::new_err(
                "price, size, is_buy must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let trade = build_trade(price[i], size[i], is_buy[i])?;
            out.push(self.inner.update(trade).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TradeSignAutocorrelation(period={})", self.inner.period())
    }
}

// PIN carries a `window` parameter, so it is hand-written.
#[pyclass(name = "Pin", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPin {
    inner: wc::Pin,
}

#[pymethods]
impl PyPin {
    #[new]
    fn new(window: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Pin::new(window).map_err(map_err)?,
        })
    }
    fn update(&mut self, price: f64, size: f64, is_buy: bool) -> PyResult<Option<f64>> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        price: Vec<f64>,
        size: Vec<f64>,
        is_buy: Vec<bool>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(PyValueError::new_err(
                "price, size, is_buy must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let trade = build_trade(price[i], size[i], is_buy[i])?;
            out.push(self.inner.update(trade).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("Pin(window={})", self.inner.window())
    }
}

// Order Flow Imbalance carries a `period` parameter and an order-book input,
// so it is hand-written.
#[pyclass(
    name = "OrderFlowImbalance",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyOrderFlowImbalance {
    inner: wc::OrderFlowImbalance,
}

#[pymethods]
impl PyOrderFlowImbalance {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::OrderFlowImbalance::new(period).map_err(map_err)?,
        })
    }
    fn update(
        &mut self,
        bid_px: Vec<f64>,
        bid_sz: Vec<f64>,
        ask_px: Vec<f64>,
        ask_sz: Vec<f64>,
    ) -> PyResult<Option<f64>> {
        let book = build_order_book(&bid_px, &bid_sz, &ask_px, &ask_sz)?;
        Ok(self.inner.update(book))
    }
    #[allow(clippy::type_complexity)]
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        snapshots: Vec<(Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>)>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut out = Vec::with_capacity(snapshots.len());
        for (bid_px, bid_sz, ask_px, ask_sz) in &snapshots {
            let book = build_order_book(bid_px, bid_sz, ask_px, ask_sz)?;
            out.push(self.inner.update(book).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("OrderFlowImbalance(period={})", self.inner.period())
    }
}

// VPIN buckets trades by volume; it carries `(bucket_volume, num_buckets)`.
#[pyclass(name = "Vpin", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyVpin {
    inner: wc::Vpin,
}

#[pymethods]
impl PyVpin {
    #[new]
    fn new(bucket_volume: f64, num_buckets: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Vpin::new(bucket_volume, num_buckets).map_err(map_err)?,
        })
    }
    fn update(&mut self, price: f64, size: f64, is_buy: bool) -> PyResult<Option<f64>> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        price: Vec<f64>,
        size: Vec<f64>,
        is_buy: Vec<bool>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(PyValueError::new_err(
                "price, size, is_buy must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let trade = build_trade(price[i], size[i], is_buy[i])?;
            out.push(self.inner.update(trade).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (bucket_volume, num_buckets) = self.inner.params();
        format!("Vpin(bucket_volume={bucket_volume}, num_buckets={num_buckets})")
    }
}

// Amihud illiquidity carries a `period` parameter and a trade input.
#[pyclass(
    name = "AmihudIlliquidity",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAmihudIlliquidity {
    inner: wc::AmihudIlliquidity,
}

#[pymethods]
impl PyAmihudIlliquidity {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AmihudIlliquidity::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, price: f64, size: f64, is_buy: bool) -> PyResult<Option<f64>> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        price: Vec<f64>,
        size: Vec<f64>,
        is_buy: Vec<bool>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(PyValueError::new_err(
                "price, size, is_buy must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let trade = build_trade(price[i], size[i], is_buy[i])?;
            out.push(self.inner.update(trade).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("AmihudIlliquidity(period={})", self.inner.period())
    }
}

// Roll measure carries a `period` parameter and a trade input.
#[pyclass(name = "RollMeasure", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRollMeasure {
    inner: wc::RollMeasure,
}

#[pymethods]
impl PyRollMeasure {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RollMeasure::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, price: f64, size: f64, is_buy: bool) -> PyResult<Option<f64>> {
        Ok(self.inner.update(build_trade(price, size, is_buy)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        price: Vec<f64>,
        size: Vec<f64>,
        is_buy: Vec<bool>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(PyValueError::new_err(
                "price, size, is_buy must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let trade = build_trade(price[i], size[i], is_buy[i])?;
            out.push(self.inner.update(trade).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RollMeasure(period={})", self.inner.period())
    }
}

// ============================== Microstructure: Price Impact ==============================
//
// Price-impact indicators consume a trade paired with the mid prevailing at
// execution. Streaming `update(price, size, is_buy, mid)` takes one such
// trade-quote (`is_buy=True` for a buyer-initiated trade); `batch` takes four
// equal-length arrays.

fn build_trade_quote(price: f64, size: f64, is_buy: bool, mid: f64) -> PyResult<wc::TradeQuote> {
    let trade = build_trade(price, size, is_buy)?;
    wc::TradeQuote::new(trade, mid).map_err(map_err)
}

macro_rules! py_trade_quote_indicator {
    ($name:ident, $inner:ty, $repr:expr) => {
        #[pyclass(name = $repr, module = "wickra._wickra", skip_from_py_object)]
        #[derive(Clone)]
        struct $name {
            inner: $inner,
        }

        #[pymethods]
        impl $name {
            #[new]
            fn new() -> Self {
                Self {
                    inner: <$inner>::new(),
                }
            }
            fn update(
                &mut self,
                price: f64,
                size: f64,
                is_buy: bool,
                mid: f64,
            ) -> PyResult<Option<f64>> {
                Ok(self
                    .inner
                    .update(build_trade_quote(price, size, is_buy, mid)?))
            }
            fn batch<'py>(
                &mut self,
                py: Python<'py>,
                price: Vec<f64>,
                size: Vec<f64>,
                is_buy: Vec<bool>,
                mid: Vec<f64>,
            ) -> PyResult<Bound<'py, PyAny>> {
                if price.len() != size.len()
                    || size.len() != is_buy.len()
                    || is_buy.len() != mid.len()
                {
                    return Err(PyValueError::new_err(
                        "price, size, is_buy, mid must be equal length",
                    ));
                }
                let mut out = Vec::with_capacity(price.len());
                for i in 0..price.len() {
                    let quote = build_trade_quote(price[i], size[i], is_buy[i], mid[i])?;
                    out.push(self.inner.update(quote).unwrap_or(f64::NAN));
                }
                out.into_pydata(py)
            }
            fn reset(&mut self) {
                self.inner.reset();
            }

            fn name(&self) -> &'static str {
                self.inner.name()
            }
            fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
            fn __repr__(&self) -> String {
                format!("{}()", $repr)
            }
        }
    };
}

py_trade_quote_indicator!(PyEffectiveSpread, wc::EffectiveSpread, "EffectiveSpread");

// Realized spread carries a `horizon` parameter, so it is hand-written.
#[pyclass(
    name = "RealizedSpread",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyRealizedSpread {
    inner: wc::RealizedSpread,
}

#[pymethods]
impl PyRealizedSpread {
    #[new]
    fn new(horizon: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RealizedSpread::new(horizon).map_err(map_err)?,
        })
    }
    fn update(&mut self, price: f64, size: f64, is_buy: bool, mid: f64) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_trade_quote(price, size, is_buy, mid)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        price: Vec<f64>,
        size: Vec<f64>,
        is_buy: Vec<bool>,
        mid: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if price.len() != size.len() || size.len() != is_buy.len() || is_buy.len() != mid.len() {
            return Err(PyValueError::new_err(
                "price, size, is_buy, mid must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let quote = build_trade_quote(price[i], size[i], is_buy[i], mid[i])?;
            out.push(self.inner.update(quote).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("RealizedSpread(horizon={})", self.inner.horizon())
    }
}

// Kyle's lambda carries a `window` parameter, so it is hand-written.
#[pyclass(name = "KylesLambda", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyKylesLambda {
    inner: wc::KylesLambda,
}

#[pymethods]
impl PyKylesLambda {
    #[new]
    fn new(window: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::KylesLambda::new(window).map_err(map_err)?,
        })
    }
    fn update(&mut self, price: f64, size: f64, is_buy: bool, mid: f64) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_trade_quote(price, size, is_buy, mid)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        price: Vec<f64>,
        size: Vec<f64>,
        is_buy: Vec<bool>,
        mid: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if price.len() != size.len() || size.len() != is_buy.len() || is_buy.len() != mid.len() {
            return Err(PyValueError::new_err(
                "price, size, is_buy, mid must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let quote = build_trade_quote(price[i], size[i], is_buy[i], mid[i])?;
            out.push(self.inner.update(quote).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("KylesLambda(window={})", self.inner.window())
    }
}

// ============================== Microstructure: Footprint ==============================
//
// Footprint is a multi-output, variable-length indicator: each `update(price,
// size, is_buy)` returns the full bar footprint accumulated since the last
// `reset()` as a `(k, 3)` array with columns `[price, bid_vol, ask_vol]`, one
// row per touched price bucket (sorted ascending by price). `batch` returns a
// list of such arrays, one per trade.

fn footprint_to_array<'py>(
    py: Python<'py>,
    out: &wc::FootprintOutput,
) -> PyResult<Bound<'py, PyAny>> {
    let rows = out.levels.len();
    let mut data = Vec::with_capacity(rows * 3);
    for level in &out.levels {
        data.push(level.price);
        data.push(level.bid_vol);
        data.push(level.ask_vol);
    }
    matrix(py, data, rows, 3)
}

#[pyclass(name = "Footprint", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFootprint {
    inner: wc::Footprint,
}

#[pymethods]
impl PyFootprint {
    #[new]
    fn new(tick_size: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Footprint::new(tick_size).map_err(map_err)?,
        })
    }
    fn update<'py>(
        &mut self,
        py: Python<'py>,
        price: f64,
        size: f64,
        is_buy: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let out = self
            .inner
            .update(build_trade(price, size, is_buy)?)
            .expect("footprint emits on every trade");
        footprint_to_array(py, &out)
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        price: Vec<f64>,
        size: Vec<f64>,
        is_buy: Vec<bool>,
    ) -> PyResult<Vec<Bound<'py, PyAny>>> {
        if price.len() != size.len() || size.len() != is_buy.len() {
            return Err(PyValueError::new_err(
                "price, size, is_buy must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(price.len());
        for i in 0..price.len() {
            let snapshot = self
                .inner
                .update(build_trade(price[i], size[i], is_buy[i])?)
                .expect("footprint emits on every trade");
            out.push(footprint_to_array(py, &snapshot)?);
        }
        Ok(out)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("Footprint(tick_size={})", self.inner.tick_size())
    }
}

// ============================== Derivatives ==============================
//
// Derivatives indicators consume a perpetual / futures tick rather than OHLCV.
// Each wrapper exposes only the tick fields its indicator reads; the helpers
// below build a fully-valid `DerivativesTick`, filling the unused fields with
// neutral defaults (prices `1.0`, sizes / rates `0.0`).

fn deriv_funding(funding_rate: f64) -> PyResult<wc::DerivativesTick> {
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

fn deriv_basis(mark_price: f64, index_price: f64) -> PyResult<wc::DerivativesTick> {
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

fn deriv_oi(open_interest: f64) -> PyResult<wc::DerivativesTick> {
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

fn deriv_oi_mark(open_interest: f64, mark_price: f64) -> PyResult<wc::DerivativesTick> {
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

fn deriv_long_short(long_size: f64, short_size: f64) -> PyResult<wc::DerivativesTick> {
    wc::DerivativesTick::new(
        0.0, 1.0, 1.0, 1.0, 0.0, long_size, short_size, 0.0, 0.0, 0.0, 0.0, 0,
    )
    .map_err(map_err)
}

fn deriv_taker(taker_buy_volume: f64, taker_sell_volume: f64) -> PyResult<wc::DerivativesTick> {
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
) -> PyResult<wc::DerivativesTick> {
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
) -> PyResult<wc::DerivativesTick> {
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
) -> PyResult<wc::DerivativesTick> {
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

fn deriv_futures_index(futures_price: f64, index_price: f64) -> PyResult<wc::DerivativesTick> {
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

fn deriv_futures_mark(futures_price: f64, mark_price: f64) -> PyResult<wc::DerivativesTick> {
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

// FundingRate takes no parameters; streaming `update(funding_rate)`, `batch`
// over one funding-rate array.
#[pyclass(name = "FundingRate", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFundingRate {
    inner: wc::FundingRate,
}

#[pymethods]
impl PyFundingRate {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::FundingRate::new(),
        }
    }
    fn update(&mut self, funding_rate: f64) -> PyResult<Option<f64>> {
        Ok(self.inner.update(deriv_funding(funding_rate)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        funding_rate: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut out = Vec::with_capacity(funding_rate.len());
        for rate in funding_rate {
            out.push(self.inner.update(deriv_funding(rate)?).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "FundingRate()".to_string()
    }
}

// FundingRateMean carries a `window` parameter.
#[pyclass(
    name = "FundingRateMean",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyFundingRateMean {
    inner: wc::FundingRateMean,
}

#[pymethods]
impl PyFundingRateMean {
    #[new]
    fn new(window: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::FundingRateMean::new(window).map_err(map_err)?,
        })
    }
    fn update(&mut self, funding_rate: f64) -> PyResult<Option<f64>> {
        Ok(self.inner.update(deriv_funding(funding_rate)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        funding_rate: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut out = Vec::with_capacity(funding_rate.len());
        for rate in funding_rate {
            out.push(self.inner.update(deriv_funding(rate)?).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("FundingRateMean(window={})", self.inner.window())
    }
}

// FundingRateZScore carries a `window` parameter.
#[pyclass(
    name = "FundingRateZScore",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyFundingRateZScore {
    inner: wc::FundingRateZScore,
}

#[pymethods]
impl PyFundingRateZScore {
    #[new]
    fn new(window: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::FundingRateZScore::new(window).map_err(map_err)?,
        })
    }
    fn update(&mut self, funding_rate: f64) -> PyResult<Option<f64>> {
        Ok(self.inner.update(deriv_funding(funding_rate)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        funding_rate: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut out = Vec::with_capacity(funding_rate.len());
        for rate in funding_rate {
            out.push(self.inner.update(deriv_funding(rate)?).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("FundingRateZScore(window={})", self.inner.window())
    }
}

// FundingBasis takes no parameters; streaming `update(mark_price, index_price)`.
#[pyclass(name = "FundingBasis", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFundingBasis {
    inner: wc::FundingBasis,
}

#[pymethods]
impl PyFundingBasis {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::FundingBasis::new(),
        }
    }
    fn update(&mut self, mark_price: f64, index_price: f64) -> PyResult<Option<f64>> {
        Ok(self.inner.update(deriv_basis(mark_price, index_price)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        mark_price: Vec<f64>,
        index_price: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if mark_price.len() != index_price.len() {
            return Err(PyValueError::new_err(
                "mark_price and index_price must be equal length",
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
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "FundingBasis()".to_string()
    }
}

// OpenInterestDelta takes no parameters; streaming `update(open_interest)`.
#[pyclass(
    name = "OpenInterestDelta",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyOpenInterestDelta {
    inner: wc::OpenInterestDelta,
}

#[pymethods]
impl PyOpenInterestDelta {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::OpenInterestDelta::new(),
        }
    }
    fn update(&mut self, open_interest: f64) -> PyResult<Option<f64>> {
        Ok(self.inner.update(deriv_oi(open_interest)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open_interest: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut out = Vec::with_capacity(open_interest.len());
        for oi in open_interest {
            out.push(self.inner.update(deriv_oi(oi)?).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "OpenInterestDelta()".to_string()
    }
}

// OIPriceDivergence carries a `window` parameter; streaming
// `update(open_interest, mark_price)`.
#[pyclass(
    name = "OIPriceDivergence",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyOIPriceDivergence {
    inner: wc::OIPriceDivergence,
}

#[pymethods]
impl PyOIPriceDivergence {
    #[new]
    fn new(window: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::OIPriceDivergence::new(window).map_err(map_err)?,
        })
    }
    fn update(&mut self, open_interest: f64, mark_price: f64) -> PyResult<Option<f64>> {
        Ok(self.inner.update(deriv_oi_mark(open_interest, mark_price)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open_interest: Vec<f64>,
        mark_price: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if open_interest.len() != mark_price.len() {
            return Err(PyValueError::new_err(
                "open_interest and mark_price must be equal length",
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
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("OIPriceDivergence(window={})", self.inner.window())
    }
}

// OIWeighted takes no parameters; streaming `update(mark_price, open_interest)`.
#[pyclass(name = "OIWeighted", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyOIWeighted {
    inner: wc::OIWeighted,
}

#[pymethods]
impl PyOIWeighted {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::OIWeighted::new(),
        }
    }
    fn update(&mut self, mark_price: f64, open_interest: f64) -> PyResult<Option<f64>> {
        Ok(self.inner.update(deriv_oi_mark(open_interest, mark_price)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        mark_price: Vec<f64>,
        open_interest: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if mark_price.len() != open_interest.len() {
            return Err(PyValueError::new_err(
                "mark_price and open_interest must be equal length",
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
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "OIWeighted()".to_string()
    }
}

// LongShortRatio takes no parameters; streaming `update(long_size, short_size)`.
#[pyclass(
    name = "LongShortRatio",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyLongShortRatio {
    inner: wc::LongShortRatio,
}

#[pymethods]
impl PyLongShortRatio {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::LongShortRatio::new(),
        }
    }
    fn update(&mut self, long_size: f64, short_size: f64) -> PyResult<Option<f64>> {
        Ok(self.inner.update(deriv_long_short(long_size, short_size)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        long_size: Vec<f64>,
        short_size: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if long_size.len() != short_size.len() {
            return Err(PyValueError::new_err(
                "long_size and short_size must be equal length",
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
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "LongShortRatio()".to_string()
    }
}

// TakerBuySellRatio takes no parameters; streaming
// `update(taker_buy_volume, taker_sell_volume)`.
#[pyclass(
    name = "TakerBuySellRatio",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyTakerBuySellRatio {
    inner: wc::TakerBuySellRatio,
}

#[pymethods]
impl PyTakerBuySellRatio {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::TakerBuySellRatio::new(),
        }
    }
    fn update(&mut self, taker_buy_volume: f64, taker_sell_volume: f64) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(deriv_taker(taker_buy_volume, taker_sell_volume)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        taker_buy_volume: Vec<f64>,
        taker_sell_volume: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if taker_buy_volume.len() != taker_sell_volume.len() {
            return Err(PyValueError::new_err(
                "taker_buy_volume and taker_sell_volume must be equal length",
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
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "TakerBuySellRatio()".to_string()
    }
}

// LiquidationFeatures is a multi-output indicator: streaming
// `update(long_liquidation, short_liquidation)` returns a 5-tuple
// `(long, short, net, total, imbalance)`; `batch` returns an `(n, 5)` array.
#[pyclass(
    name = "LiquidationFeatures",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyLiquidationFeatures {
    inner: wc::LiquidationFeatures,
}

#[pymethods]
impl PyLiquidationFeatures {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::LiquidationFeatures::new(),
        }
    }
    /// Returns `(long, short, net, total, imbalance)` or None during warmup.
    #[allow(clippy::type_complexity)]
    fn update(
        &mut self,
        long_liquidation: f64,
        short_liquidation: f64,
    ) -> PyResult<Option<(f64, f64, f64, f64, f64)>> {
        Ok(self
            .inner
            .update(deriv_liquidation(long_liquidation, short_liquidation)?)
            .map(|o| (o.long, o.short, o.net, o.total, o.imbalance)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        long_liquidation: Vec<f64>,
        short_liquidation: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if long_liquidation.len() != short_liquidation.len() {
            return Err(PyValueError::new_err(
                "long_liquidation and short_liquidation must be equal length",
            ));
        }
        let rows = long_liquidation.len();
        let mut data = Vec::with_capacity(rows * 5);
        for i in 0..rows {
            let out = self
                .inner
                .update(deriv_liquidation(
                    long_liquidation[i],
                    short_liquidation[i],
                )?)
                .expect("liquidation features emit on every tick");
            data.push(out.long);
            data.push(out.short);
            data.push(out.net);
            data.push(out.total);
            data.push(out.imbalance);
        }
        matrix(py, data, rows, 5)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "LiquidationFeatures()".to_string()
    }
}

// TermStructureBasis takes no parameters; streaming
// `update(futures_price, index_price)`.
#[pyclass(
    name = "TermStructureBasis",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyTermStructureBasis {
    inner: wc::TermStructureBasis,
}

#[pymethods]
impl PyTermStructureBasis {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::TermStructureBasis::new(),
        }
    }
    fn update(&mut self, futures_price: f64, index_price: f64) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(deriv_futures_index(futures_price, index_price)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        futures_price: Vec<f64>,
        index_price: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if futures_price.len() != index_price.len() {
            return Err(PyValueError::new_err(
                "futures_price and index_price must be equal length",
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
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "TermStructureBasis()".to_string()
    }
}

// CalendarSpread takes no parameters; streaming `update(futures_price, mark_price)`.
#[pyclass(
    name = "CalendarSpread",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyCalendarSpread {
    inner: wc::CalendarSpread,
}

#[pymethods]
impl PyCalendarSpread {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::CalendarSpread::new(),
        }
    }
    fn update(&mut self, futures_price: f64, mark_price: f64) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(deriv_futures_mark(futures_price, mark_price)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        futures_price: Vec<f64>,
        mark_price: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if futures_price.len() != mark_price.len() {
            return Err(PyValueError::new_err(
                "futures_price and mark_price must be equal length",
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
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "CalendarSpread()".to_string()
    }
}

// Estimated leverage ratio: open interest over aggregate long+short size.
#[pyclass(
    name = "EstimatedLeverageRatio",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyEstimatedLeverageRatio {
    inner: wc::EstimatedLeverageRatio,
}

#[pymethods]
impl PyEstimatedLeverageRatio {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::EstimatedLeverageRatio::new(),
        }
    }
    fn update(
        &mut self,
        open_interest: f64,
        long_size: f64,
        short_size: f64,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(deriv_oi_long_short(open_interest, long_size, short_size)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open_interest: Vec<f64>,
        long_size: Vec<f64>,
        short_size: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if open_interest.len() != long_size.len() || long_size.len() != short_size.len() {
            return Err(PyValueError::new_err(
                "open_interest, long_size, short_size must be equal length",
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
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "EstimatedLeverageRatio()".to_string()
    }
}

// OI-to-volume ratio: open interest over taker buy+sell volume.
#[pyclass(
    name = "OiToVolumeRatio",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyOiToVolumeRatio {
    inner: wc::OiToVolumeRatio,
}

#[pymethods]
impl PyOiToVolumeRatio {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::OiToVolumeRatio::new(),
        }
    }
    fn update(
        &mut self,
        open_interest: f64,
        taker_buy_volume: f64,
        taker_sell_volume: f64,
    ) -> PyResult<Option<f64>> {
        Ok(self.inner.update(deriv_oi_taker(
            open_interest,
            taker_buy_volume,
            taker_sell_volume,
        )?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open_interest: Vec<f64>,
        taker_buy_volume: Vec<f64>,
        taker_sell_volume: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if open_interest.len() != taker_buy_volume.len()
            || taker_buy_volume.len() != taker_sell_volume.len()
        {
            return Err(PyValueError::new_err(
                "open_interest, taker_buy_volume, taker_sell_volume must be equal length",
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
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "OiToVolumeRatio()".to_string()
    }
}

// Perpetual premium index: relative premium of mark over index price.
#[pyclass(
    name = "PerpetualPremiumIndex",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyPerpetualPremiumIndex {
    inner: wc::PerpetualPremiumIndex,
}

#[pymethods]
impl PyPerpetualPremiumIndex {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::PerpetualPremiumIndex::new(),
        }
    }
    fn update(&mut self, mark_price: f64, index_price: f64) -> PyResult<Option<f64>> {
        Ok(self.inner.update(deriv_basis(mark_price, index_price)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        mark_price: Vec<f64>,
        index_price: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if mark_price.len() != index_price.len() {
            return Err(PyValueError::new_err(
                "mark_price and index_price must be equal length",
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
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "PerpetualPremiumIndex()".to_string()
    }
}

// Funding-implied APR: per-interval funding annualised.
#[pyclass(
    name = "FundingImpliedApr",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyFundingImpliedApr {
    inner: wc::FundingImpliedApr,
}

#[pymethods]
impl PyFundingImpliedApr {
    #[new]
    fn new(intervals_per_year: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::FundingImpliedApr::new(intervals_per_year).map_err(map_err)?,
        })
    }
    fn update(&mut self, funding_rate: f64) -> PyResult<Option<f64>> {
        Ok(self.inner.update(deriv_funding(funding_rate)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        funding_rate: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut out = Vec::with_capacity(funding_rate.len());
        for r in funding_rate {
            out.push(self.inner.update(deriv_funding(r)?).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "FundingImpliedApr(intervals_per_year={})",
            self.inner.intervals_per_year()
        )
    }
}

// Open-interest momentum: rate-of-change of open interest over a window.
#[pyclass(
    name = "OpenInterestMomentum",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyOpenInterestMomentum {
    inner: wc::OpenInterestMomentum,
}

#[pymethods]
impl PyOpenInterestMomentum {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::OpenInterestMomentum::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, open_interest: f64) -> PyResult<Option<f64>> {
        Ok(self.inner.update(deriv_oi(open_interest)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open_interest: Vec<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut out = Vec::with_capacity(open_interest.len());
        for oi in open_interest {
            out.push(self.inner.update(deriv_oi(oi)?).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("OpenInterestMomentum(period={})", self.inner.period())
    }
}

// ============================== Market Breadth ==============================
//
// Market-breadth indicators consume a `CrossSection`: one tick carrying the
// per-symbol state of the whole universe. The Python convention passes a tick as
// four equal-length parallel arrays (`change`, `volume`, `new_high`, `new_low`);
// `batch` takes one such group of arrays per tick.

fn build_cross_section(
    change: &[f64],
    volume: &[f64],
    new_high: &[bool],
    new_low: &[bool],
) -> PyResult<wc::CrossSection> {
    if change.len() != volume.len()
        || change.len() != new_high.len()
        || change.len() != new_low.len()
    {
        return Err(PyValueError::new_err(
            "change, volume, new_high and new_low must be equal length",
        ));
    }
    let members = (0..change.len())
        .map(|i| wc::Member::new(change[i], volume[i], new_high[i], new_low[i]))
        .collect();
    wc::CrossSection::new(members, 0).map_err(map_err)
}

// AdvanceDecline takes no parameters; streaming `update(change, volume, new_high,
// new_low)` over one universe, `batch` over one such array group per tick.
#[pyclass(
    name = "AdvanceDecline",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAdvanceDecline {
    inner: wc::AdvanceDecline,
}

#[pymethods]
impl PyAdvanceDecline {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::AdvanceDecline::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "AdvanceDecline()".to_string()
    }
}

fn build_cross_section_above_ma(
    change: &[f64],
    volume: &[f64],
    new_high: &[bool],
    new_low: &[bool],
    above_ma: &[bool],
) -> PyResult<wc::CrossSection> {
    if change.len() != volume.len()
        || change.len() != new_high.len()
        || change.len() != new_low.len()
        || change.len() != above_ma.len()
    {
        return Err(PyValueError::new_err(
            "change, volume, new_high, new_low and above_ma must be equal length",
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
) -> PyResult<wc::CrossSection> {
    if change.len() != volume.len()
        || change.len() != new_high.len()
        || change.len() != new_low.len()
        || change.len() != on_buy_signal.len()
    {
        return Err(PyValueError::new_err(
            "change, volume, new_high, new_low and on_buy_signal must be equal length",
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

#[pyclass(
    name = "AdvanceDeclineRatio",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAdvanceDeclineRatio {
    inner: wc::AdvanceDeclineRatio,
}

#[pymethods]
impl PyAdvanceDeclineRatio {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::AdvanceDeclineRatio::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "AdvanceDeclineRatio()".to_string()
    }
}

#[pyclass(name = "AdVolumeLine", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAdVolumeLine {
    inner: wc::AdVolumeLine,
}

#[pymethods]
impl PyAdVolumeLine {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::AdVolumeLine::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "AdVolumeLine()".to_string()
    }
}

#[pyclass(
    name = "McClellanOscillator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyMcClellanOscillator {
    inner: wc::McClellanOscillator,
}

#[pymethods]
impl PyMcClellanOscillator {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::McClellanOscillator::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "McClellanOscillator()".to_string()
    }
}

#[pyclass(
    name = "McClellanSummationIndex",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyMcClellanSummationIndex {
    inner: wc::McClellanSummationIndex,
}

#[pymethods]
impl PyMcClellanSummationIndex {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::McClellanSummationIndex::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "McClellanSummationIndex()".to_string()
    }
}

#[pyclass(name = "Trin", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTrin {
    inner: wc::Trin,
}

#[pymethods]
impl PyTrin {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::Trin::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "Trin()".to_string()
    }
}

#[pyclass(name = "BreadthThrust", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyBreadthThrust {
    inner: wc::BreadthThrust,
}

#[pymethods]
impl PyBreadthThrust {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::BreadthThrust::new(period).map_err(map_err)?,
        })
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("BreadthThrust(period={})", self.inner.period())
    }
}

#[pyclass(
    name = "NewHighsNewLows",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyNewHighsNewLows {
    inner: wc::NewHighsNewLows,
}

#[pymethods]
impl PyNewHighsNewLows {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::NewHighsNewLows::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "NewHighsNewLows()".to_string()
    }
}

#[pyclass(name = "HighLowIndex", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyHighLowIndex {
    inner: wc::HighLowIndex,
}

#[pymethods]
impl PyHighLowIndex {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::HighLowIndex::new(period).map_err(map_err)?,
        })
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("HighLowIndex(period={})", self.inner.period())
    }
}

#[pyclass(
    name = "PercentAboveMa",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyPercentAboveMa {
    inner: wc::PercentAboveMa,
}

#[pymethods]
impl PyPercentAboveMa {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::PercentAboveMa::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
        above_ma: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self.inner.update(build_cross_section_above_ma(
            &change, &volume, &new_high, &new_low, &above_ma,
        )?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
        above_ma: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
            || change.len() != above_ma.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high, new_low and above_ma must have the same number of ticks",
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
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "PercentAboveMa()".to_string()
    }
}

#[pyclass(
    name = "UpDownVolumeRatio",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyUpDownVolumeRatio {
    inner: wc::UpDownVolumeRatio,
}

#[pymethods]
impl PyUpDownVolumeRatio {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::UpDownVolumeRatio::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "UpDownVolumeRatio()".to_string()
    }
}

#[pyclass(
    name = "BullishPercentIndex",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyBullishPercentIndex {
    inner: wc::BullishPercentIndex,
}

#[pymethods]
impl PyBullishPercentIndex {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::BullishPercentIndex::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
        on_buy_signal: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self.inner.update(build_cross_section_buy(
            &change,
            &volume,
            &new_high,
            &new_low,
            &on_buy_signal,
        )?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
        on_buy_signal: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
            || change.len() != on_buy_signal.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high, new_low and on_buy_signal must have the same number of ticks",
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
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "BullishPercentIndex()".to_string()
    }
}

#[pyclass(
    name = "CumulativeVolumeIndex",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyCumulativeVolumeIndex {
    inner: wc::CumulativeVolumeIndex,
}

#[pymethods]
impl PyCumulativeVolumeIndex {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::CumulativeVolumeIndex::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "CumulativeVolumeIndex()".to_string()
    }
}

#[pyclass(
    name = "AbsoluteBreadthIndex",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAbsoluteBreadthIndex {
    inner: wc::AbsoluteBreadthIndex,
}

#[pymethods]
impl PyAbsoluteBreadthIndex {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::AbsoluteBreadthIndex::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "AbsoluteBreadthIndex()".to_string()
    }
}

#[pyclass(name = "TickIndex", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTickIndex {
    inner: wc::TickIndex,
}

#[pymethods]
impl PyTickIndex {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::TickIndex::new(),
        }
    }
    fn update(
        &mut self,
        change: Vec<f64>,
        volume: Vec<f64>,
        new_high: Vec<bool>,
        new_low: Vec<bool>,
    ) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .update(build_cross_section(&change, &volume, &new_high, &new_low)?))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        change: Vec<Vec<f64>>,
        volume: Vec<Vec<f64>>,
        new_high: Vec<Vec<bool>>,
        new_low: Vec<Vec<bool>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if change.len() != volume.len()
            || change.len() != new_high.len()
            || change.len() != new_low.len()
        {
            return Err(PyValueError::new_err(
                "change, volume, new_high and new_low must have the same number of ticks",
            ));
        }
        let mut out = Vec::with_capacity(change.len());
        for i in 0..change.len() {
            let section = build_cross_section(&change[i], &volume[i], &new_high[i], &new_low[i])?;
            out.push(self.inner.update(section).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "TickIndex()".to_string()
    }
}

// ============================== Family 15: Risk / Performance ==============================

#[pyclass(
    name = "UpsidePotentialRatio",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyUpsidePotentialRatio {
    inner: wc::UpsidePotentialRatio,
}

#[pymethods]
impl PyUpsidePotentialRatio {
    #[new]
    #[pyo3(signature = (period, mar=0.0))]
    fn new(period: usize, mar: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::UpsidePotentialRatio::new(period, mar).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn mar(&self) -> f64 {
        self.inner.mar()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "UpsidePotentialRatio(period={}, mar={})",
            self.inner.period(),
            self.inner.mar()
        )
    }
}

#[pyclass(name = "M2Measure", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyM2Measure {
    inner: wc::M2Measure,
}

#[pymethods]
impl PyM2Measure {
    #[new]
    #[pyo3(signature = (period, risk_free, benchmark_stddev))]
    fn new(period: usize, risk_free: f64, benchmark_stddev: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::M2Measure::new(period, risk_free, benchmark_stddev).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn risk_free(&self) -> f64 {
        self.inner.risk_free()
    }
    #[getter]
    fn benchmark_stddev(&self) -> f64 {
        self.inner.benchmark_stddev()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "M2Measure(period={}, risk_free={}, benchmark_stddev={})",
            self.inner.period(),
            self.inner.risk_free(),
            self.inner.benchmark_stddev()
        )
    }
}

#[pyclass(name = "SharpeRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySharpeRatio {
    inner: wc::SharpeRatio,
}

#[pymethods]
impl PySharpeRatio {
    #[new]
    #[pyo3(signature = (period, risk_free=0.0))]
    fn new(period: usize, risk_free: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::SharpeRatio::new(period, risk_free).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn risk_free(&self) -> f64 {
        self.inner.risk_free()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "SharpeRatio(period={}, risk_free={})",
            self.inner.period(),
            self.inner.risk_free()
        )
    }
}

#[pyclass(name = "SortinoRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySortinoRatio {
    inner: wc::SortinoRatio,
}

#[pymethods]
impl PySortinoRatio {
    #[new]
    #[pyo3(signature = (period, mar=0.0))]
    fn new(period: usize, mar: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::SortinoRatio::new(period, mar).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn mar(&self) -> f64 {
        self.inner.mar()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "SortinoRatio(period={}, mar={})",
            self.inner.period(),
            self.inner.mar()
        )
    }
}

#[pyclass(name = "CalmarRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyCalmarRatio {
    inner: wc::CalmarRatio,
}

#[pymethods]
impl PyCalmarRatio {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::CalmarRatio::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("CalmarRatio(period={})", self.inner.period())
    }
}

#[pyclass(name = "OmegaRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyOmegaRatio {
    inner: wc::OmegaRatio,
}

#[pymethods]
impl PyOmegaRatio {
    #[new]
    #[pyo3(signature = (period, threshold=0.0))]
    fn new(period: usize, threshold: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::OmegaRatio::new(period, threshold).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn threshold(&self) -> f64 {
        self.inner.threshold()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "OmegaRatio(period={}, threshold={})",
            self.inner.period(),
            self.inner.threshold()
        )
    }
}

#[pyclass(name = "MaxDrawdown", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyMaxDrawdown {
    inner: wc::MaxDrawdown,
}

#[pymethods]
impl PyMaxDrawdown {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::MaxDrawdown::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("MaxDrawdown(period={})", self.inner.period())
    }
}

#[pyclass(
    name = "AverageDrawdown",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAverageDrawdown {
    inner: wc::AverageDrawdown,
}

#[pymethods]
impl PyAverageDrawdown {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AverageDrawdown::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("AverageDrawdown(period={})", self.inner.period())
    }
}

#[pyclass(
    name = "DrawdownDuration",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyDrawdownDuration {
    inner: wc::DrawdownDuration,
}

#[pymethods]
impl PyDrawdownDuration {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::DrawdownDuration::new(),
        }
    }
    fn update(&mut self, value: f64) -> Option<u32> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        let out: Vec<f64> = self
            .inner
            .batch(slice)
            .into_iter()
            .map(|v| v.map_or(f64::NAN, f64::from))
            .collect();
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "DrawdownDuration()".to_string()
    }
}

#[pyclass(name = "PainIndex", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyPainIndex {
    inner: wc::PainIndex,
}

#[pymethods]
impl PyPainIndex {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::PainIndex::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("PainIndex(period={})", self.inner.period())
    }
}

#[pyclass(name = "ValueAtRisk", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyValueAtRisk {
    inner: wc::ValueAtRisk,
}

#[pymethods]
impl PyValueAtRisk {
    #[new]
    #[pyo3(signature = (period, confidence=0.95))]
    fn new(period: usize, confidence: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ValueAtRisk::new(period, confidence).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn confidence(&self) -> f64 {
        self.inner.confidence()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "ValueAtRisk(period={}, confidence={})",
            self.inner.period(),
            self.inner.confidence()
        )
    }
}

#[pyclass(
    name = "ConditionalValueAtRisk",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyConditionalValueAtRisk {
    inner: wc::ConditionalValueAtRisk,
}

#[pymethods]
impl PyConditionalValueAtRisk {
    #[new]
    #[pyo3(signature = (period, confidence=0.95))]
    fn new(period: usize, confidence: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ConditionalValueAtRisk::new(period, confidence).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn confidence(&self) -> f64 {
        self.inner.confidence()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "ConditionalValueAtRisk(period={}, confidence={})",
            self.inner.period(),
            self.inner.confidence()
        )
    }
}

#[pyclass(name = "ProfitFactor", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyProfitFactor {
    inner: wc::ProfitFactor,
}

#[pymethods]
impl PyProfitFactor {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ProfitFactor::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("ProfitFactor(period={})", self.inner.period())
    }
}

#[pyclass(name = "GainLossRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyGainLossRatio {
    inner: wc::GainLossRatio,
}

#[pymethods]
impl PyGainLossRatio {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::GainLossRatio::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("GainLossRatio(period={})", self.inner.period())
    }
}

#[pyclass(
    name = "RecoveryFactor",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyRecoveryFactor {
    inner: wc::RecoveryFactor,
}

#[pymethods]
impl PyRecoveryFactor {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::RecoveryFactor::new(),
        }
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "RecoveryFactor()".to_string()
    }
}

#[pyclass(
    name = "KellyCriterion",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyKellyCriterion {
    inner: wc::KellyCriterion,
}

#[pymethods]
impl PyKellyCriterion {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::KellyCriterion::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("KellyCriterion(period={})", self.inner.period())
    }
}

// --- Pair (asset, benchmark) indicators ---

#[pyclass(name = "TreynorRatio", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTreynorRatio {
    inner: wc::TreynorRatio,
}

#[pymethods]
impl PyTreynorRatio {
    #[new]
    #[pyo3(signature = (period, risk_free=0.0))]
    fn new(period: usize, risk_free: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TreynorRatio::new(period, risk_free).map_err(map_err)?,
        })
    }
    fn update(&mut self, asset: f64, benchmark: f64) -> Option<f64> {
        self.inner.update((asset, benchmark))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        asset: Buf1,
        benchmark: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let a = asset.as_slice();
        let b = benchmark.as_slice();
        if a.len() != b.len() {
            return Err(PyValueError::new_err(
                "asset and benchmark must have equal length",
            ));
        }
        let mut out = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            out.push(self.inner.update((a[i], b[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn risk_free(&self) -> f64 {
        self.inner.risk_free()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "TreynorRatio(period={}, risk_free={})",
            self.inner.period(),
            self.inner.risk_free()
        )
    }
}

#[pyclass(
    name = "InformationRatio",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyInformationRatio {
    inner: wc::InformationRatio,
}

#[pymethods]
impl PyInformationRatio {
    #[new]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::InformationRatio::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, asset: f64, benchmark: f64) -> Option<f64> {
        self.inner.update((asset, benchmark))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        asset: Buf1,
        benchmark: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let a = asset.as_slice();
        let b = benchmark.as_slice();
        if a.len() != b.len() {
            return Err(PyValueError::new_err(
                "asset and benchmark must have equal length",
            ));
        }
        let mut out = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            out.push(self.inner.update((a[i], b[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("InformationRatio(period={})", self.inner.period())
    }
}

#[pyclass(name = "Alpha", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAlpha {
    inner: wc::Alpha,
}

#[pymethods]
impl PyAlpha {
    #[new]
    #[pyo3(signature = (period, risk_free=0.0))]
    fn new(period: usize, risk_free: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Alpha::new(period, risk_free).map_err(map_err)?,
        })
    }
    fn update(&mut self, asset: f64, benchmark: f64) -> Option<f64> {
        self.inner.update((asset, benchmark))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        asset: Buf1,
        benchmark: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let a = asset.as_slice();
        let b = benchmark.as_slice();
        if a.len() != b.len() {
            return Err(PyValueError::new_err(
                "asset and benchmark must have equal length",
            ));
        }
        let mut out = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            out.push(self.inner.update((a[i], b[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn risk_free(&self) -> f64 {
        self.inner.risk_free()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "Alpha(period={}, risk_free={})",
            self.inner.period(),
            self.inner.risk_free()
        )
    }
}

// ============================== Alt-Chart Bars ==============================
//
// Bar builders consume close prices and emit a variable number of completed bars
// per input. `update(close)` returns the bars finished on that close; `batch`
// returns a `(k, 3)` array of all completed bars concatenated.

#[pyclass(name = "RenkoBars", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRenkoBars {
    inner: wc::RenkoBars,
}

#[pymethods]
impl PyRenkoBars {
    #[new]
    fn new(box_size: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RenkoBars::new(box_size).map_err(map_err)?,
        })
    }
    /// Feed one close; returns bricks completed on it as `(open, close, direction)`.
    fn update(&mut self, close: f64) -> PyResult<Vec<(f64, f64, i64)>> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| (b.open, b.close, i64::from(b.direction)))
            .collect())
    }
    /// Batch over a close column. Returns shape `(k, 3)` of `[open, close, direction]`.
    fn batch<'py>(&mut self, py: Python<'py>, close: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let prices = close.as_slice();
        let mut rows: Vec<f64> = Vec::new();
        let mut k = 0usize;
        for &price in prices {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                rows.push(b.open);
                rows.push(b.close);
                rows.push(f64::from(b.direction));
                k += 1;
            }
        }
        matrix(py, rows, k, 3)
    }
    #[getter]
    fn box_size(&self) -> f64 {
        self.inner.box_size()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn __repr__(&self) -> String {
        format!("RenkoBars(box_size={})", self.inner.box_size())
    }
}

#[pyclass(name = "KagiBars", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyKagiBars {
    inner: wc::KagiBars,
}

#[pymethods]
impl PyKagiBars {
    #[new]
    fn new(reversal: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::KagiBars::new(reversal).map_err(map_err)?,
        })
    }
    /// Feed one close; returns completed segments as `(start, end, direction)`.
    fn update(&mut self, close: f64) -> PyResult<Vec<(f64, f64, i64)>> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| (b.start, b.end, i64::from(b.direction)))
            .collect())
    }
    /// Batch over a close column. Returns shape `(k, 3)` of `[start, end, direction]`.
    fn batch<'py>(&mut self, py: Python<'py>, close: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let prices = close.as_slice();
        let mut rows: Vec<f64> = Vec::new();
        let mut k = 0usize;
        for &price in prices {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                rows.push(b.start);
                rows.push(b.end);
                rows.push(f64::from(b.direction));
                k += 1;
            }
        }
        matrix(py, rows, k, 3)
    }
    #[getter]
    fn reversal(&self) -> f64 {
        self.inner.reversal()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn __repr__(&self) -> String {
        format!("KagiBars(reversal={})", self.inner.reversal())
    }
}

#[pyclass(
    name = "PointAndFigureBars",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyPointAndFigureBars {
    inner: wc::PointAndFigureBars,
}

#[pymethods]
impl PyPointAndFigureBars {
    #[new]
    #[pyo3(signature = (box_size, reversal=3))]
    fn new(box_size: f64, reversal: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::PointAndFigureBars::new(box_size, reversal).map_err(map_err)?,
        })
    }
    /// Feed one close; returns completed columns as `(direction, high, low)`.
    fn update(&mut self, close: f64) -> PyResult<Vec<(i64, f64, f64)>> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|c| (i64::from(c.direction), c.high, c.low))
            .collect())
    }
    /// Batch over a close column. Returns shape `(k, 3)` of `[direction, high, low]`.
    fn batch<'py>(&mut self, py: Python<'py>, close: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let prices = close.as_slice();
        let mut rows: Vec<f64> = Vec::new();
        let mut k = 0usize;
        for &price in prices {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for col in self.inner.update(candle) {
                rows.push(f64::from(col.direction));
                rows.push(col.high);
                rows.push(col.low);
                k += 1;
            }
        }
        matrix(py, rows, k, 3)
    }
    #[getter]
    fn box_size(&self) -> f64 {
        self.inner.box_size()
    }
    #[getter]
    fn reversal(&self) -> usize {
        self.inner.reversal()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn __repr__(&self) -> String {
        format!(
            "PointAndFigureBars(box_size={}, reversal={})",
            self.inner.box_size(),
            self.inner.reversal()
        )
    }
}

#[pyclass(name = "RangeBars", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRangeBars {
    inner: wc::RangeBars,
}

#[pymethods]
impl PyRangeBars {
    #[new]
    fn new(range: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RangeBars::new(range).map_err(map_err)?,
        })
    }
    /// Feed one close; returns bars completed on it as `(open, close, direction)`.
    fn update(&mut self, close: f64) -> PyResult<Vec<(f64, f64, i64)>> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| (b.open, b.close, i64::from(b.direction)))
            .collect())
    }
    /// Batch over a close column. Returns shape `(k, 3)` of `[open, close, direction]`.
    fn batch<'py>(&mut self, py: Python<'py>, close: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let prices = close.as_slice();
        let mut rows: Vec<f64> = Vec::new();
        let mut k = 0usize;
        for &price in prices {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                rows.push(b.open);
                rows.push(b.close);
                rows.push(f64::from(b.direction));
                k += 1;
            }
        }
        matrix(py, rows, k, 3)
    }
    #[getter]
    fn range(&self) -> f64 {
        self.inner.range()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn __repr__(&self) -> String {
        format!("RangeBars(range={})", self.inner.range())
    }
}

#[pyclass(name = "TickBars", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTickBars {
    inner: wc::TickBars,
}

#[pymethods]
impl PyTickBars {
    #[new]
    fn new(ticks: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TickBars::new(ticks).map_err(map_err)?,
        })
    }
    /// Feed one candle; returns bars completed as `(open, high, low, close, volume)`.
    #[allow(clippy::too_many_arguments)]
    fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> PyResult<OhlcvBarRows> {
        let candle = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| (b.open, b.high, b.low, b.close, b.volume))
            .collect())
    }
    /// Batch over OHLCV columns. Returns shape `(k, 5)` of `[open, high, low, close, volume]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (o, h, l, c, v) = ohlcv_slices(&open, &high, &low, &close, &volume)?;
        let mut rows: Vec<f64> = Vec::new();
        let mut k = 0usize;
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                rows.extend_from_slice(&[b.open, b.high, b.low, b.close, b.volume]);
                k += 1;
            }
        }
        matrix(py, rows, k, 5)
    }
    #[getter]
    fn ticks(&self) -> usize {
        self.inner.ticks()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn __repr__(&self) -> String {
        format!("TickBars(ticks={})", self.inner.ticks())
    }
}

#[pyclass(name = "VolumeBars", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyVolumeBars {
    inner: wc::VolumeBars,
}

#[pymethods]
impl PyVolumeBars {
    #[new]
    fn new(volume_per_bar: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VolumeBars::new(volume_per_bar).map_err(map_err)?,
        })
    }
    /// Feed one candle; returns bars completed as `(open, high, low, close, volume)`.
    #[allow(clippy::too_many_arguments)]
    fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> PyResult<OhlcvBarRows> {
        let candle = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| (b.open, b.high, b.low, b.close, b.volume))
            .collect())
    }
    /// Batch over OHLCV columns. Returns shape `(k, 5)` of `[open, high, low, close, volume]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (o, h, l, c, v) = ohlcv_slices(&open, &high, &low, &close, &volume)?;
        let mut rows: Vec<f64> = Vec::new();
        let mut k = 0usize;
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                rows.extend_from_slice(&[b.open, b.high, b.low, b.close, b.volume]);
                k += 1;
            }
        }
        matrix(py, rows, k, 5)
    }
    #[getter]
    fn volume_per_bar(&self) -> f64 {
        self.inner.volume_per_bar()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn __repr__(&self) -> String {
        format!("VolumeBars(volume_per_bar={})", self.inner.volume_per_bar())
    }
}

#[pyclass(name = "DollarBars", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyDollarBars {
    inner: wc::DollarBars,
}

#[pymethods]
impl PyDollarBars {
    #[new]
    fn new(dollar_per_bar: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::DollarBars::new(dollar_per_bar).map_err(map_err)?,
        })
    }
    /// Feed one candle; returns bars completed as `(open, high, low, close, volume, dollar)`.
    #[allow(clippy::too_many_arguments)]
    fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> PyResult<DollarBarRows> {
        let candle = wc::Candle::new(open, high, low, close, volume, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| (b.open, b.high, b.low, b.close, b.volume, b.dollar))
            .collect())
    }
    /// Batch over OHLCV columns. Returns shape `(k, 6)` of `[open, high, low, close, volume, dollar]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (o, h, l, c, v) = ohlcv_slices(&open, &high, &low, &close, &volume)?;
        let mut rows: Vec<f64> = Vec::new();
        let mut k = 0usize;
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], v[i], 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                rows.extend_from_slice(&[b.open, b.high, b.low, b.close, b.volume, b.dollar]);
                k += 1;
            }
        }
        matrix(py, rows, k, 6)
    }
    #[getter]
    fn dollar_per_bar(&self) -> f64 {
        self.inner.dollar_per_bar()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn __repr__(&self) -> String {
        format!("DollarBars(dollar_per_bar={})", self.inner.dollar_per_bar())
    }
}

#[pyclass(name = "ImbalanceBars", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyImbalanceBars {
    inner: wc::ImbalanceBars,
}

#[pymethods]
impl PyImbalanceBars {
    #[new]
    fn new(threshold: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ImbalanceBars::new(threshold).map_err(map_err)?,
        })
    }
    /// Feed one candle; returns bars completed as `(open, high, low, close, imbalance, direction)`.
    fn update(&mut self, open: f64, high: f64, low: f64, close: f64) -> PyResult<ImbalanceBarRows> {
        let candle = wc::Candle::new(open, high, low, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| {
                (
                    b.open,
                    b.high,
                    b.low,
                    b.close,
                    b.imbalance,
                    i64::from(b.direction),
                )
            })
            .collect())
    }
    /// Batch over OHLC columns. Returns shape `(k, 6)` of `[open, high, low, close, imbalance, direction]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (o, h, l, c) = ohlc_slices(&open, &high, &low, &close)?;
        let mut rows: Vec<f64> = Vec::new();
        let mut k = 0usize;
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                rows.extend_from_slice(&[
                    b.open,
                    b.high,
                    b.low,
                    b.close,
                    b.imbalance,
                    f64::from(b.direction),
                ]);
                k += 1;
            }
        }
        matrix(py, rows, k, 6)
    }
    #[getter]
    fn threshold(&self) -> f64 {
        self.inner.threshold()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn __repr__(&self) -> String {
        format!("ImbalanceBars(threshold={})", self.inner.threshold())
    }
}

#[pyclass(name = "RunBars", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyRunBars {
    inner: wc::RunBars,
}

#[pymethods]
impl PyRunBars {
    #[new]
    fn new(run_length: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::RunBars::new(run_length).map_err(map_err)?,
        })
    }
    /// Feed one candle; returns bars completed as `(open, high, low, close, length, direction)`.
    fn update(&mut self, open: f64, high: f64, low: f64, close: f64) -> PyResult<RunBarRows> {
        let candle = wc::Candle::new(open, high, low, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| {
                (
                    b.open,
                    b.high,
                    b.low,
                    b.close,
                    i64::try_from(b.length).unwrap_or(i64::MAX),
                    i64::from(b.direction),
                )
            })
            .collect())
    }
    /// Batch over OHLC columns. Returns shape `(k, 6)` of `[open, high, low, close, length, direction]`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (o, h, l, c) = ohlc_slices(&open, &high, &low, &close)?;
        let mut rows: Vec<f64> = Vec::new();
        let mut k = 0usize;
        for i in 0..o.len() {
            let candle = wc::Candle::new(o[i], h[i], l[i], c[i], 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                #[allow(clippy::cast_precision_loss)]
                rows.extend_from_slice(&[
                    b.open,
                    b.high,
                    b.low,
                    b.close,
                    b.length as f64,
                    f64::from(b.direction),
                ]);
                k += 1;
            }
        }
        matrix(py, rows, k, 6)
    }
    #[getter]
    fn run_length(&self) -> usize {
        self.inner.run_length()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn __repr__(&self) -> String {
        format!("RunBars(run_length={})", self.inner.run_length())
    }
}

#[pyclass(
    name = "ThreeLineBreakBars",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyThreeLineBreakBars {
    inner: wc::ThreeLineBreakBars,
}

#[pymethods]
impl PyThreeLineBreakBars {
    #[new]
    #[pyo3(signature = (lines=3))]
    fn new(lines: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ThreeLineBreakBars::new(lines).map_err(map_err)?,
        })
    }
    /// Feed one close; returns bars completed on it as `(open, close, direction)`.
    fn update(&mut self, close: f64) -> PyResult<Vec<(f64, f64, i64)>> {
        let candle = wc::Candle::new(close, close, close, close, 1.0, 0).map_err(map_err)?;
        Ok(self
            .inner
            .update(candle)
            .into_iter()
            .map(|b| (b.open, b.close, i64::from(b.direction)))
            .collect())
    }
    /// Batch over a close column. Returns shape `(k, 3)` of `[open, close, direction]`.
    fn batch<'py>(&mut self, py: Python<'py>, close: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let prices = close.as_slice();
        let mut rows: Vec<f64> = Vec::new();
        let mut k = 0usize;
        for &price in prices {
            let candle = wc::Candle::new(price, price, price, price, 1.0, 0).map_err(map_err)?;
            for b in self.inner.update(candle) {
                rows.push(b.open);
                rows.push(b.close);
                rows.push(f64::from(b.direction));
                k += 1;
            }
        }
        matrix(py, rows, k, 3)
    }
    #[getter]
    fn lines(&self) -> usize {
        self.inner.lines()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn __repr__(&self) -> String {
        format!("ThreeLineBreakBars(lines={})", self.inner.lines())
    }
}

// ============================== Module ==============================

// ====================== Seasonality & Session (full-candle) ======================
//
// These indicators read the wall-clock fields of `Candle::timestamp`, so the
// bindings consume the FULL candle (open, high, low, close, volume, timestamp)
// — unlike the high/low/close candle indicators above.

fn build_seasonality_candles(
    open: &Buf1,
    high: &Buf1,
    low: &Buf1,
    close: &Buf1,
    volume: &Buf1,
    timestamp: &BufI64,
) -> PyResult<Vec<wc::Candle>> {
    let o = open.as_slice();
    let h = high.as_slice();
    let l = low.as_slice();
    let c = close.as_slice();
    let v = volume.as_slice();
    let t = timestamp.as_slice();
    let n = o.len();
    if [h.len(), l.len(), c.len(), v.len(), t.len()]
        .iter()
        .any(|&x| x != n)
    {
        return Err(PyValueError::new_err(
            "open, high, low, close, volume, timestamp must be equal length",
        ));
    }
    let mut candles = Vec::with_capacity(n);
    for i in 0..n {
        candles.push(wc::Candle::new(o[i], h[i], l[i], c[i], v[i], t[i]).map_err(map_err)?);
    }
    Ok(candles)
}

macro_rules! py_seasonality_offset_scalar {
    ($pytype:ident, $name:literal, $rust:ident) => {
        #[pyclass(name = $name, module = "wickra._wickra", skip_from_py_object)]
        #[derive(Clone)]
        struct $pytype {
            inner: wc::$rust,
        }
        #[pymethods]
        impl $pytype {
            #[new]
            #[pyo3(signature = (utc_offset_minutes = 0))]
            fn new(utc_offset_minutes: i32) -> Self {
                Self {
                    inner: wc::$rust::new(utc_offset_minutes),
                }
            }
            fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
                Ok(self.inner.update(extract_candle(candle)?))
            }
            #[allow(clippy::too_many_arguments)]
            fn batch<'py>(
                &mut self,
                py: Python<'py>,
                open: Buf1,
                high: Buf1,
                low: Buf1,
                close: Buf1,
                volume: Buf1,
                timestamp: BufI64,
            ) -> PyResult<Bound<'py, PyAny>> {
                let candles =
                    build_seasonality_candles(&open, &high, &low, &close, &volume, &timestamp)?;
                let out: Vec<f64> = candles
                    .into_iter()
                    .map(|c| self.inner.update(c).unwrap_or(f64::NAN))
                    .collect();
                out.into_pydata(py)
            }
            #[getter]
            fn utc_offset_minutes(&self) -> i32 {
                self.inner.utc_offset_minutes()
            }
            fn reset(&mut self) {
                self.inner.reset();
            }

            fn name(&self) -> &'static str {
                self.inner.name()
            }
            fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
            fn __repr__(&self) -> String {
                format!(
                    "{}(utc_offset_minutes={})",
                    $name,
                    self.inner.utc_offset_minutes()
                )
            }
        }
    };
}

macro_rules! py_seasonality_bucket_profile {
    ($pytype:ident, $name:literal, $rust:ident) => {
        #[pyclass(name = $name, module = "wickra._wickra", skip_from_py_object)]
        #[derive(Clone)]
        struct $pytype {
            inner: wc::$rust,
        }
        #[pymethods]
        impl $pytype {
            #[new]
            #[pyo3(signature = (buckets = 24, utc_offset_minutes = 0))]
            fn new(buckets: usize, utc_offset_minutes: i32) -> PyResult<Self> {
                Ok(Self {
                    inner: wc::$rust::new(buckets, utc_offset_minutes).map_err(map_err)?,
                })
            }
            fn update<'py>(
                &mut self,
                py: Python<'py>,
                candle: &Bound<'_, PyAny>,
            ) -> PyResult<Option<Bound<'py, PyAny>>> {
                let c = extract_candle(candle)?;
                self.inner
                    .update(c)
                    .map(|o| f64_array(py, &o.bins))
                    .transpose()
            }
            #[allow(clippy::too_many_arguments)]
            fn batch<'py>(
                &mut self,
                py: Python<'py>,
                open: Buf1,
                high: Buf1,
                low: Buf1,
                close: Buf1,
                volume: Buf1,
                timestamp: BufI64,
            ) -> PyResult<Bound<'py, PyAny>> {
                let candles =
                    build_seasonality_candles(&open, &high, &low, &close, &volume, &timestamp)?;
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
                matrix(py, out, n, k)
            }
            #[getter]
            fn params(&self) -> (usize, i32) {
                self.inner.params()
            }
            fn reset(&mut self) {
                self.inner.reset();
            }

            fn name(&self) -> &'static str {
                self.inner.name()
            }
            fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
            fn __repr__(&self) -> String {
                let (buckets, offset) = self.inner.params();
                format!("{}(buckets={buckets}, utc_offset_minutes={offset})", $name)
            }
        }
    };
}

macro_rules! py_seasonality_offset_profile {
    ($pytype:ident, $name:literal, $rust:ident, $k:expr) => {
        #[pyclass(name = $name, module = "wickra._wickra", skip_from_py_object)]
        #[derive(Clone)]
        struct $pytype {
            inner: wc::$rust,
        }
        #[pymethods]
        impl $pytype {
            #[new]
            #[pyo3(signature = (utc_offset_minutes = 0))]
            fn new(utc_offset_minutes: i32) -> Self {
                Self {
                    inner: wc::$rust::new(utc_offset_minutes),
                }
            }
            fn update<'py>(
                &mut self,
                py: Python<'py>,
                candle: &Bound<'_, PyAny>,
            ) -> PyResult<Option<Bound<'py, PyAny>>> {
                let c = extract_candle(candle)?;
                self.inner
                    .update(c)
                    .map(|o| f64_array(py, &o.bins))
                    .transpose()
            }
            #[allow(clippy::too_many_arguments)]
            fn batch<'py>(
                &mut self,
                py: Python<'py>,
                open: Buf1,
                high: Buf1,
                low: Buf1,
                close: Buf1,
                volume: Buf1,
                timestamp: BufI64,
            ) -> PyResult<Bound<'py, PyAny>> {
                let candles =
                    build_seasonality_candles(&open, &high, &low, &close, &volume, &timestamp)?;
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
                matrix(py, out, n, k)
            }
            #[getter]
            fn utc_offset_minutes(&self) -> i32 {
                self.inner.utc_offset_minutes()
            }
            fn reset(&mut self) {
                self.inner.reset();
            }

            fn name(&self) -> &'static str {
                self.inner.name()
            }
            fn is_ready(&self) -> bool {
                self.inner.is_ready()
            }
            fn warmup_period(&self) -> usize {
                self.inner.warmup_period()
            }
            fn __repr__(&self) -> String {
                format!(
                    "{}(utc_offset_minutes={})",
                    $name,
                    self.inner.utc_offset_minutes()
                )
            }
        }
    };
}

py_seasonality_offset_scalar!(PySessionVwap, "SessionVwap", SessionVwap);
py_seasonality_offset_scalar!(PyOvernightGap, "OvernightGap", OvernightGap);
py_seasonality_offset_scalar!(PySeasonalZScore, "SeasonalZScore", SeasonalZScore);
py_seasonality_bucket_profile!(
    PyTimeOfDayReturnProfile,
    "TimeOfDayReturnProfile",
    TimeOfDayReturnProfile
);
py_seasonality_bucket_profile!(
    PyIntradayVolatilityProfile,
    "IntradayVolatilityProfile",
    IntradayVolatilityProfile
);
py_seasonality_bucket_profile!(
    PyVolumeByTimeProfile,
    "VolumeByTimeProfile",
    VolumeByTimeProfile
);
py_seasonality_offset_profile!(PyDayOfWeekProfile, "DayOfWeekProfile", DayOfWeekProfile, 7);

#[pyclass(
    name = "AverageDailyRange",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyAverageDailyRange {
    inner: wc::AverageDailyRange,
}
#[pymethods]
impl PyAverageDailyRange {
    #[new]
    #[pyo3(signature = (period = 14, utc_offset_minutes = 0))]
    fn new(period: usize, utc_offset_minutes: i32) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AverageDailyRange::new(period, utc_offset_minutes).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        Ok(self.inner.update(extract_candle(candle)?))
    }
    #[allow(clippy::too_many_arguments)]
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
        timestamp: BufI64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let candles = build_seasonality_candles(&open, &high, &low, &close, &volume, &timestamp)?;
        let out: Vec<f64> = candles
            .into_iter()
            .map(|c| self.inner.update(c).unwrap_or(f64::NAN))
            .collect();
        out.into_pydata(py)
    }
    #[getter]
    fn params(&self) -> (usize, i32) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (period, offset) = self.inner.params();
        format!("AverageDailyRange(period={period}, utc_offset_minutes={offset})")
    }
}

#[pyclass(name = "TurnOfMonth", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyTurnOfMonth {
    inner: wc::TurnOfMonth,
}
#[pymethods]
impl PyTurnOfMonth {
    #[new]
    #[pyo3(signature = (n_first = 3, n_last = 1, utc_offset_minutes = 0))]
    fn new(n_first: u32, n_last: u32, utc_offset_minutes: i32) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TurnOfMonth::new(n_first, n_last, utc_offset_minutes).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        Ok(self.inner.update(extract_candle(candle)?))
    }
    #[allow(clippy::too_many_arguments)]
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
        timestamp: BufI64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let candles = build_seasonality_candles(&open, &high, &low, &close, &volume, &timestamp)?;
        let out: Vec<f64> = candles
            .into_iter()
            .map(|c| self.inner.update(c).unwrap_or(f64::NAN))
            .collect();
        out.into_pydata(py)
    }
    #[getter]
    fn params(&self) -> (u32, u32, i32) {
        self.inner.params()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (n_first, n_last, offset) = self.inner.params();
        format!("TurnOfMonth(n_first={n_first}, n_last={n_last}, utc_offset_minutes={offset})")
    }
}

#[pyclass(
    name = "SessionHighLow",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PySessionHighLow {
    inner: wc::SessionHighLow,
}
#[pymethods]
impl PySessionHighLow {
    #[new]
    #[pyo3(signature = (utc_offset_minutes = 0))]
    fn new(utc_offset_minutes: i32) -> Self {
        Self {
            inner: wc::SessionHighLow::new(utc_offset_minutes),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.high, o.low)))
    }
    #[allow(clippy::too_many_arguments)]
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
        timestamp: BufI64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let candles = build_seasonality_candles(&open, &high, &low, &close, &volume, &timestamp)?;
        let n = candles.len();
        let mut out = vec![f64::NAN; n * 2];
        for (i, c) in candles.into_iter().enumerate() {
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.high;
                out[i * 2 + 1] = o.low;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn utc_offset_minutes(&self) -> i32 {
        self.inner.utc_offset_minutes()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "SessionHighLow(utc_offset_minutes={})",
            self.inner.utc_offset_minutes()
        )
    }
}

#[pyclass(name = "SessionRange", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySessionRange {
    inner: wc::SessionRange,
}
#[pymethods]
impl PySessionRange {
    #[new]
    #[pyo3(signature = (utc_offset_minutes = 0))]
    fn new(utc_offset_minutes: i32) -> Self {
        Self {
            inner: wc::SessionRange::new(utc_offset_minutes),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.asia, o.eu, o.us)))
    }
    #[allow(clippy::too_many_arguments)]
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
        timestamp: BufI64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let candles = build_seasonality_candles(&open, &high, &low, &close, &volume, &timestamp)?;
        let n = candles.len();
        let mut out = vec![f64::NAN; n * 3];
        for (i, c) in candles.into_iter().enumerate() {
            if let Some(o) = self.inner.update(c) {
                out[i * 3] = o.asia;
                out[i * 3 + 1] = o.eu;
                out[i * 3 + 2] = o.us;
            }
        }
        matrix(py, out, n, 3)
    }
    #[getter]
    fn utc_offset_minutes(&self) -> i32 {
        self.inner.utc_offset_minutes()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "SessionRange(utc_offset_minutes={})",
            self.inner.utc_offset_minutes()
        )
    }
}

#[pyclass(
    name = "OvernightIntradayReturn",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyOvernightIntradayReturn {
    inner: wc::OvernightIntradayReturn,
}
#[pymethods]
impl PyOvernightIntradayReturn {
    #[new]
    #[pyo3(signature = (utc_offset_minutes = 0))]
    fn new(utc_offset_minutes: i32) -> Self {
        Self {
            inner: wc::OvernightIntradayReturn::new(utc_offset_minutes),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.overnight, o.intraday)))
    }
    #[allow(clippy::too_many_arguments)]
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        open: Buf1,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
        timestamp: BufI64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let candles = build_seasonality_candles(&open, &high, &low, &close, &volume, &timestamp)?;
        let n = candles.len();
        let mut out = vec![f64::NAN; n * 2];
        for (i, c) in candles.into_iter().enumerate() {
            if let Some(o) = self.inner.update(c) {
                out[i * 2] = o.overnight;
                out[i * 2 + 1] = o.intraday;
            }
        }
        matrix(py, out, n, 2)
    }
    #[getter]
    fn utc_offset_minutes(&self) -> i32 {
        self.inner.utc_offset_minutes()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!(
            "OvernightIntradayReturn(utc_offset_minutes={})",
            self.inner.utc_offset_minutes()
        )
    }
}

// ============================== Fibonacci ==============================

/// Build a candle for the swing-based Fibonacci tools from a `high`/`low` pair.
/// Only the high and low drive the swing tracker, so open and close are pinned
/// to the midpoint to keep the OHLC invariants valid.
fn swing_candle(high: f64, low: f64) -> Result<wc::Candle, wc::Error> {
    let mid = f64::midpoint(high, low);
    wc::Candle::new(mid, high, low, mid, 0.0, 0)
}

#[pyclass(
    name = "FibRetracement",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyFibRetracement {
    inner: wc::FibRetracement,
}

#[pymethods]
impl PyFibRetracement {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::FibRetracement::new(),
        }
    }
    /// Returns `(level_0, …, level_1000)` (seven levels) or None during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<PivotLevels>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| {
            (
                o.level_0,
                o.level_236,
                o.level_382,
                o.level_500,
                o.level_618,
                o.level_786,
                o.level_1000,
            )
        }))
    }
    /// Batch over input columns high, low. Returns shape `(n, 7)`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 7];
        for i in 0..n {
            if let Some(o) = self
                .inner
                .update(swing_candle(h[i], l[i]).map_err(map_err)?)
            {
                out[i * 7] = o.level_0;
                out[i * 7 + 1] = o.level_236;
                out[i * 7 + 2] = o.level_382;
                out[i * 7 + 3] = o.level_500;
                out[i * 7 + 4] = o.level_618;
                out[i * 7 + 5] = o.level_786;
                out[i * 7 + 6] = o.level_1000;
            }
        }
        matrix(py, out, n, 7)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "FibRetracement()".to_string()
    }
}

#[pyclass(name = "FibExtension", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFibExtension {
    inner: wc::FibExtension,
}

#[pymethods]
impl PyFibExtension {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::FibExtension::new(),
        }
    }
    /// Returns `(level_1272, level_1414, level_1618, level_2000, level_2618)` or None.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<FibExtLevels>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| {
            (
                o.level_1272,
                o.level_1414,
                o.level_1618,
                o.level_2000,
                o.level_2618,
            )
        }))
    }
    /// Batch over input columns high, low. Returns shape `(n, 5)`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 5];
        for i in 0..n {
            if let Some(o) = self
                .inner
                .update(swing_candle(h[i], l[i]).map_err(map_err)?)
            {
                out[i * 5] = o.level_1272;
                out[i * 5 + 1] = o.level_1414;
                out[i * 5 + 2] = o.level_1618;
                out[i * 5 + 3] = o.level_2000;
                out[i * 5 + 4] = o.level_2618;
            }
        }
        matrix(py, out, n, 5)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "FibExtension()".to_string()
    }
}

#[pyclass(name = "FibProjection", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFibProjection {
    inner: wc::FibProjection,
}

#[pymethods]
impl PyFibProjection {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::FibProjection::new(),
        }
    }
    /// Returns `(level_618, level_1000, level_1618, level_2618)` or None during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.level_618, o.level_1000, o.level_1618, o.level_2618)))
    }
    /// Batch over input columns high, low. Returns shape `(n, 4)`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            if let Some(o) = self
                .inner
                .update(swing_candle(h[i], l[i]).map_err(map_err)?)
            {
                out[i * 4] = o.level_618;
                out[i * 4 + 1] = o.level_1000;
                out[i * 4 + 2] = o.level_1618;
                out[i * 4 + 3] = o.level_2618;
            }
        }
        matrix(py, out, n, 4)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "FibProjection()".to_string()
    }
}

#[pyclass(name = "AutoFib", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAutoFib {
    inner: wc::AutoFib,
}

#[pymethods]
impl PyAutoFib {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::AutoFib::new(),
        }
    }
    /// Returns `(level_0, …, level_1000)` for the dominant leg, or None during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<PivotLevels>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| {
            (
                o.level_0,
                o.level_236,
                o.level_382,
                o.level_500,
                o.level_618,
                o.level_786,
                o.level_1000,
            )
        }))
    }
    /// Batch over input columns high, low. Returns shape `(n, 7)`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 7];
        for i in 0..n {
            if let Some(o) = self
                .inner
                .update(swing_candle(h[i], l[i]).map_err(map_err)?)
            {
                out[i * 7] = o.level_0;
                out[i * 7 + 1] = o.level_236;
                out[i * 7 + 2] = o.level_382;
                out[i * 7 + 3] = o.level_500;
                out[i * 7 + 4] = o.level_618;
                out[i * 7 + 5] = o.level_786;
                out[i * 7 + 6] = o.level_1000;
            }
        }
        matrix(py, out, n, 7)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "AutoFib()".to_string()
    }
}

#[pyclass(name = "GoldenPocket", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyGoldenPocket {
    inner: wc::GoldenPocket,
}

#[pymethods]
impl PyGoldenPocket {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::GoldenPocket::new(),
        }
    }
    /// Returns `(low, mid, high)` of the golden-pocket band, or None during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.low, o.mid, o.high)))
    }
    /// Batch over input columns high, low. Returns shape `(n, 3)`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self
                .inner
                .update(swing_candle(h[i], l[i]).map_err(map_err)?)
            {
                out[i * 3] = o.low;
                out[i * 3 + 1] = o.mid;
                out[i * 3 + 2] = o.high;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "GoldenPocket()".to_string()
    }
}

#[pyclass(name = "FibConfluence", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFibConfluence {
    inner: wc::FibConfluence,
}

#[pymethods]
impl PyFibConfluence {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::FibConfluence::new(),
        }
    }
    /// Returns `(price, strength)` of the densest cluster, or None during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.price, o.strength)))
    }
    /// Batch over input columns high, low. Returns shape `(n, 2)`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self
                .inner
                .update(swing_candle(h[i], l[i]).map_err(map_err)?)
            {
                out[i * 2] = o.price;
                out[i * 2 + 1] = o.strength;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "FibConfluence()".to_string()
    }
}

#[pyclass(name = "FibFan", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFibFan {
    inner: wc::FibFan,
}

#[pymethods]
impl PyFibFan {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::FibFan::new(),
        }
    }
    /// Returns `(fan_382, fan_500, fan_618)` at the current bar, or None.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.fan_382, o.fan_500, o.fan_618)))
    }
    /// Batch over input columns high, low. Returns shape `(n, 3)`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self
                .inner
                .update(swing_candle(h[i], l[i]).map_err(map_err)?)
            {
                out[i * 3] = o.fan_382;
                out[i * 3 + 1] = o.fan_500;
                out[i * 3 + 2] = o.fan_618;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "FibFan()".to_string()
    }
}

#[pyclass(name = "FibArcs", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFibArcs {
    inner: wc::FibArcs,
}

#[pymethods]
impl PyFibArcs {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::FibArcs::new(),
        }
    }
    /// Returns `(arc_382, arc_500, arc_618)` at the current bar, or None.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.arc_382, o.arc_500, o.arc_618)))
    }
    /// Batch over input columns high, low. Returns shape `(n, 3)`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            if let Some(o) = self
                .inner
                .update(swing_candle(h[i], l[i]).map_err(map_err)?)
            {
                out[i * 3] = o.arc_382;
                out[i * 3 + 1] = o.arc_500;
                out[i * 3 + 2] = o.arc_618;
            }
        }
        matrix(py, out, n, 3)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "FibArcs()".to_string()
    }
}

#[pyclass(name = "FibChannel", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFibChannel {
    inner: wc::FibChannel,
}

#[pymethods]
impl PyFibChannel {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::FibChannel::new(),
        }
    }
    /// Returns `(base, level_618, level_1000, level_1618)` at the current bar, or None.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.base, o.level_618, o.level_1000, o.level_1618)))
    }
    /// Batch over input columns high, low. Returns shape `(n, 4)`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 4];
        for i in 0..n {
            if let Some(o) = self
                .inner
                .update(swing_candle(h[i], l[i]).map_err(map_err)?)
            {
                out[i * 4] = o.base;
                out[i * 4 + 1] = o.level_618;
                out[i * 4 + 2] = o.level_1000;
                out[i * 4 + 3] = o.level_1618;
            }
        }
        matrix(py, out, n, 4)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "FibChannel()".to_string()
    }
}

#[pyclass(name = "FibTimeZones", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyFibTimeZones {
    inner: wc::FibTimeZones,
}

#[pymethods]
impl PyFibTimeZones {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::FibTimeZones::new(),
        }
    }
    /// Returns `(on_zone, bars_to_next)` at the current bar, or None during warmup.
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c).map(|o| (o.on_zone, o.bars_to_next)))
    }
    /// Batch over input columns high, low. Returns shape `(n, 2)`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        if h.len() != l.len() {
            return Err(PyValueError::new_err("high and low must be equal length"));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 2];
        for i in 0..n {
            if let Some(o) = self
                .inner
                .update(swing_candle(h[i], l[i]).map_err(map_err)?)
            {
                out[i * 2] = o.on_zone;
                out[i * 2 + 1] = o.bars_to_next;
            }
        }
        matrix(py, out, n, 2)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "FibTimeZones()".to_string()
    }
}

// ============================== EWMA Volatility ==============================

#[pyclass(
    name = "EwmaVolatility",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyEwmaVolatility {
    inner: wc::EwmaVolatility,
}

#[pymethods]
impl PyEwmaVolatility {
    #[new]
    #[pyo3(signature = (lambda_=0.94))]
    fn new(lambda_: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::EwmaVolatility::new(lambda_).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn lambda_(&self) -> f64 {
        self.inner.lambda()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== GARCH(1,1) ==============================

#[pyclass(name = "Garch11", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyGarch11 {
    inner: wc::Garch11,
}

#[pymethods]
impl PyGarch11 {
    #[new]
    #[pyo3(signature = (omega=0.000_002, alpha=0.1, beta=0.88))]
    fn new(omega: f64, alpha: f64, beta: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::Garch11::new(omega, alpha, beta).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn params(&self) -> (f64, f64, f64) {
        self.inner.params()
    }
    #[getter]
    fn unconditional_variance(&self) -> f64 {
        self.inner.unconditional_variance()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Volatility of Volatility ==============================

#[pyclass(
    name = "VolatilityOfVolatility",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyVolatilityOfVolatility {
    inner: wc::VolatilityOfVolatility,
}

#[pymethods]
impl PyVolatilityOfVolatility {
    #[new]
    #[pyo3(signature = (vol_window=20, vov_window=20))]
    fn new(vol_window: usize, vov_window: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VolatilityOfVolatility::new(vol_window, vov_window).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn windows(&self) -> (usize, usize) {
        self.inner.windows()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Volatility Cone ==============================

#[pyclass(
    name = "VolatilityCone",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyVolatilityCone {
    inner: wc::VolatilityCone,
}

#[pymethods]
impl PyVolatilityCone {
    #[new]
    #[pyo3(signature = (window=20, lookback=60))]
    fn new(window: usize, lookback: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VolatilityCone::new(window, lookback).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<ConeBands>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.current, o.min, o.median, o.max, o.percentile)))
    }
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let n = h.len();
        let mut out = vec![f64::NAN; n * 5];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 5] = o.current;
                out[i * 5 + 1] = o.min;
                out[i * 5 + 2] = o.median;
                out[i * 5 + 3] = o.max;
                out[i * 5 + 4] = o.percentile;
            }
        }
        matrix(py, out, n, 5)
    }
    #[getter]
    fn windows(&self) -> (usize, usize) {
        self.inner.windows()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
}

// ============================== Volume RSI ==============================

#[pyclass(name = "VolumeRsi", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyVolumeRsi {
    inner: wc::VolumeRsi,
}

#[pymethods]
impl PyVolumeRsi {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VolumeRsi::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over close + volume series (both 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let c = close.as_slice();
        let v = volume.as_slice();
        if c.len() != v.len() {
            return Err(PyValueError::new_err(
                "close and volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], c[i], c[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("VolumeRsi(period={})", self.inner.period())
    }
}

// ============================== Williams A/D ==============================

#[pyclass(name = "Wad", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyWad {
    inner: wc::Wad,
}

#[pymethods]
impl PyWad {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::Wad::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high, low, close series (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        if h.len() != l.len() || l.len() != c.len() {
            return Err(PyValueError::new_err(
                "high, low, close must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], 0.0, 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "Wad()".to_string()
    }
}

// ============================== Twiggs Money Flow ==============================

#[pyclass(
    name = "TwiggsMoneyFlow",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyTwiggsMoneyFlow {
    inner: wc::TwiggsMoneyFlow,
}

#[pymethods]
impl PyTwiggsMoneyFlow {
    #[new]
    #[pyo3(signature = (period=21))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TwiggsMoneyFlow::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high, low, close, volume series (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let vol = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != vol.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], vol[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TwiggsMoneyFlow(period={})", self.inner.period())
    }
}

// ============================== Trade Volume Index ==============================

#[pyclass(
    name = "TradeVolumeIndex",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyTradeVolumeIndex {
    inner: wc::TradeVolumeIndex,
}

#[pymethods]
impl PyTradeVolumeIndex {
    #[new]
    #[pyo3(signature = (min_tick=0.25))]
    fn new(min_tick: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::TradeVolumeIndex::new(min_tick).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over close + volume series (both 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let c = close.as_slice();
        let v = volume.as_slice();
        if c.len() != v.len() {
            return Err(PyValueError::new_err(
                "close and volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], c[i], c[i], c[i], v[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn min_tick(&self) -> f64 {
        self.inner.min_tick()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("TradeVolumeIndex(min_tick={})", self.inner.min_tick())
    }
}

// ============================== Intraday Intensity ==============================

#[pyclass(
    name = "IntradayIntensity",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyIntradayIntensity {
    inner: wc::IntradayIntensity,
}

#[pymethods]
impl PyIntradayIntensity {
    #[new]
    fn new() -> Self {
        Self {
            inner: wc::IntradayIntensity::new(),
        }
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high, low, close, volume series (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let vol = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != vol.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], vol[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        "IntradayIntensity()".to_string()
    }
}

// ============================== Better Volume ==============================

#[pyclass(name = "BetterVolume", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyBetterVolume {
    inner: wc::BetterVolume,
}

#[pymethods]
impl PyBetterVolume {
    #[new]
    #[pyo3(signature = (period=14))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::BetterVolume::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<f64>> {
        let c = extract_candle(candle)?;
        Ok(self.inner.update(c))
    }
    /// Batch over high, low, close, volume series (all 1-D, equal length).
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        high: Buf1,
        low: Buf1,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = high.as_slice();
        let l = low.as_slice();
        let c = close.as_slice();
        let vol = volume.as_slice();
        if h.len() != l.len() || l.len() != c.len() || c.len() != vol.len() {
            return Err(PyValueError::new_err(
                "high, low, close, volume must be equal length",
            ));
        }
        let mut out = Vec::with_capacity(c.len());
        for i in 0..c.len() {
            let candle = wc::Candle::new(c[i], h[i], l[i], c[i], vol[i], 0).map_err(map_err)?;
            out.push(self.inner.update(candle).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("BetterVolume(period={})", self.inner.period())
    }
}

// ============================== Volume-Weighted MACD ==============================

#[pyclass(
    name = "VolumeWeightedMacd",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyVolumeWeightedMacd {
    inner: wc::VolumeWeightedMacd,
}

#[pymethods]
impl PyVolumeWeightedMacd {
    #[new]
    #[pyo3(signature = (fast=12, slow=26, signal=9))]
    fn new(fast: usize, slow: usize, signal: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::VolumeWeightedMacd::new(fast, slow, signal).map_err(map_err)?,
        })
    }
    fn update(&mut self, candle: &Bound<'_, PyAny>) -> PyResult<Option<(f64, f64, f64)>> {
        let c = extract_candle(candle)?;
        Ok(self
            .inner
            .update(c)
            .map(|o| (o.macd, o.signal, o.histogram)))
    }
    /// Batch over close + volume series. Returns shape `(n, 3)` with
    /// columns `[macd, signal, histogram]`; warmup rows are `NaN`.
    fn batch<'py>(
        &mut self,
        py: Python<'py>,
        close: Buf1,
        volume: Buf1,
    ) -> PyResult<Bound<'py, PyAny>> {
        let c = close.as_slice();
        let v = volume.as_slice();
        if c.len() != v.len() {
            return Err(PyValueError::new_err(
                "close and volume must be equal length",
            ));
        }
        let n = c.len();
        let mut out = vec![f64::NAN; n * 3];
        for i in 0..n {
            let candle = wc::Candle::new(c[i], c[i], c[i], c[i], v[i], 0).map_err(map_err)?;
            if let Some(o) = self.inner.update(candle) {
                out[i * 3] = o.macd;
                out[i * 3 + 1] = o.signal;
                out[i * 3 + 2] = o.histogram;
            }
        }
        matrix(py, out, n, 3)
    }
    #[getter]
    fn periods(&self) -> (usize, usize, usize) {
        self.inner.periods()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (fast, slow, signal) = self.inner.periods();
        format!("VolumeWeightedMacd(fast={fast}, slow={slow}, signal={signal})")
    }
}

// ============================== Shannon Entropy ==============================

#[pyclass(name = "SHANNONENT", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyShannonEntropy {
    inner: wc::ShannonEntropy,
}

#[pymethods]
impl PyShannonEntropy {
    #[new]
    #[pyo3(signature = (period=20, bins=8))]
    fn new(period: usize, bins: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::ShannonEntropy::new(period, bins).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn params(&self) -> (usize, usize) {
        self.inner.params()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (period, bins) = self.inner.params();
        format!("SHANNONENT(period={period}, bins={bins})")
    }
}

// ============================== Sample Entropy ==============================

#[pyclass(name = "SAMPLEENT", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PySampleEntropy {
    inner: wc::SampleEntropy,
}

#[pymethods]
impl PySampleEntropy {
    #[new]
    #[pyo3(signature = (period=20, m=2, r_factor=0.2))]
    fn new(period: usize, m: usize, r_factor: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::SampleEntropy::new(period, m, r_factor).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn params(&self) -> (usize, usize, f64) {
        self.inner.params()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (period, m, r_factor) = self.inner.params();
        format!("SAMPLEENT(period={period}, m={m}, r_factor={r_factor})")
    }
}

// ============================== Kendall Tau ==============================

#[pyclass(name = "KendallTau", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyKendallTau {
    inner: wc::KendallTau,
}

#[pymethods]
impl PyKendallTau {
    #[new]
    #[pyo3(signature = (period=20))]
    fn new(period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::KendallTau::new(period).map_err(map_err)?,
        })
    }
    fn update(&mut self, x: f64, y: f64) -> Option<f64> {
        self.inner.update((x, y))
    }
    /// Batch over two equally-sized series.
    fn batch<'py>(&mut self, py: Python<'py>, x: Buf1, y: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let xs = x.as_slice();
        let ys = y.as_slice();
        if xs.len() != ys.len() {
            return Err(PyValueError::new_err("x and y must be equal length"));
        }
        let mut out = Vec::with_capacity(xs.len());
        for i in 0..xs.len() {
            out.push(self.inner.update((xs[i], ys[i])).unwrap_or(f64::NAN));
        }
        out.into_pydata(py)
    }
    #[getter]
    fn period(&self) -> usize {
        self.inner.period()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        format!("KendallTau(period={})", self.inner.period())
    }
}

// ============================== Bandpass Filter ==============================

#[pyclass(name = "BANDPASS", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyBandpassFilter {
    inner: wc::BandpassFilter,
}

#[pymethods]
impl PyBandpassFilter {
    #[new]
    #[pyo3(signature = (period=20, bandwidth=0.3))]
    fn new(period: usize, bandwidth: f64) -> PyResult<Self> {
        Ok(Self {
            inner: wc::BandpassFilter::new(period, bandwidth).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn params(&self) -> (usize, f64) {
        self.inner.params()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (period, bandwidth) = self.inner.params();
        format!("BANDPASS(period={period}, bandwidth={bandwidth})")
    }
}

// ============================== Even Better Sinewave ==============================

#[pyclass(
    name = "EVENBETTERSINE",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyEvenBetterSinewave {
    inner: wc::EvenBetterSinewave,
}

#[pymethods]
impl PyEvenBetterSinewave {
    #[new]
    #[pyo3(signature = (hp_period=40, ssf_length=10))]
    fn new(hp_period: usize, ssf_length: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::EvenBetterSinewave::new(hp_period, ssf_length).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn params(&self) -> (usize, usize) {
        self.inner.params()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (hp_period, ssf_length) = self.inner.params();
        format!("EVENBETTERSINE(hp_period={hp_period}, ssf_length={ssf_length})")
    }
}

// ============================== Autocorrelation Periodogram ==============================

#[pyclass(name = "AUTOCORRPGRAM", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyAutocorrelationPeriodogram {
    inner: wc::AutocorrelationPeriodogram,
}

#[pymethods]
impl PyAutocorrelationPeriodogram {
    #[new]
    #[pyo3(signature = (min_period=10, max_period=48))]
    fn new(min_period: usize, max_period: usize) -> PyResult<Self> {
        Ok(Self {
            inner: wc::AutocorrelationPeriodogram::new(min_period, max_period).map_err(map_err)?,
        })
    }
    fn update(&mut self, value: f64) -> Option<f64> {
        self.inner.update(value)
    }
    fn batch<'py>(&mut self, py: Python<'py>, prices: Buf1) -> PyResult<Bound<'py, PyAny>> {
        let slice = prices.as_slice();
        f64_batch(py, slice.len(), |out| self.inner.batch_nan_into(slice, out))
    }
    #[getter]
    fn periods(&self) -> (usize, usize) {
        self.inner.periods()
    }
    #[getter]
    fn value(&self) -> Option<f64> {
        self.inner.value()
    }
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn __repr__(&self) -> String {
        let (min_period, max_period) = self.inner.periods();
        format!("AUTOCORRPGRAM(min_period={min_period}, max_period={max_period})")
    }
}

#[pymodule]
#[allow(clippy::too_many_lines)]
fn _wickra(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    // The result type of every multi-output `batch`. It was documented but
    // never registered, so callers could neither import it nor name it in a
    // type annotation or an `isinstance` check.
    m.add_class::<Matrix>()?;
    m.add_class::<PySma>()?;
    m.add_class::<PyEma>()?;
    m.add_class::<PyWma>()?;
    m.add_class::<PyRsi>()?;
    m.add_class::<PyMacd>()?;
    m.add_class::<PyMacdFix>()?;
    m.add_class::<PyMacdExt>()?;
    m.add_class::<PyHtPhasor>()?;
    m.add_class::<PyBb>()?;
    m.add_class::<PyAtr>()?;
    m.add_class::<PyImi>()?;
    m.add_class::<PyQqe>()?;
    m.add_class::<PyElderRay>()?;
    m.add_class::<PyStoch>()?;
    m.add_class::<PyObv>()?;
    m.add_class::<PyDema>()?;
    m.add_class::<PyTema>()?;
    m.add_class::<PyHma>()?;
    m.add_class::<PyKama>()?;
    m.add_class::<PyRvi>()?;
    m.add_class::<PyPgo>()?;
    m.add_class::<PyKst>()?;
    m.add_class::<PySmi>()?;
    m.add_class::<PyLaguerreRsi>()?;
    m.add_class::<PyConnorsRsi>()?;
    m.add_class::<PyInertia>()?;
    m.add_class::<PyCci>()?;
    m.add_class::<PyRoc>()?;
    m.add_class::<PyWilliamsR>()?;
    m.add_class::<PyAdx>()?;
    m.add_class::<PyAdxr>()?;
    m.add_class::<PyPlusDm>()?;
    m.add_class::<PyMinusDm>()?;
    m.add_class::<PyMfi>()?;
    m.add_class::<PyTrix>()?;
    m.add_class::<PyPsar>()?;
    m.add_class::<PySarExt>()?;
    m.add_class::<PyKeltner>()?;
    m.add_class::<PyDonchian>()?;
    m.add_class::<PyVwap>()?;
    m.add_class::<PyRollingVwap>()?;
    m.add_class::<PyAo>()?;
    m.add_class::<PyAroon>()?;
    m.add_class::<PySmma>()?;
    m.add_class::<PyTrima>()?;
    m.add_class::<PyZlema>()?;
    m.add_class::<PyT3>()?;
    m.add_class::<PyGeneralizedDema>()?;
    m.add_class::<PyHoltWinters>()?;
    m.add_class::<PyRmi>()?;
    m.add_class::<PyDerivativeOscillator>()?;
    m.add_class::<PyVwma>()?;
    m.add_class::<PyMom>()?;
    m.add_class::<PyCmo>()?;
    m.add_class::<PyTsi>()?;
    m.add_class::<PyPmo>()?;
    m.add_class::<PyTii>()?;
    m.add_class::<PyKst>()?;
    m.add_class::<PyStochRsi>()?;
    m.add_class::<PyUltimateOscillator>()?;
    m.add_class::<PyPpo>()?;
    m.add_class::<PyDpo>()?;
    m.add_class::<PyCoppock>()?;
    m.add_class::<PyAroonOscillator>()?;
    m.add_class::<PyVortex>()?;
    m.add_class::<PyRwi>()?;
    m.add_class::<PyWaveTrend>()?;
    m.add_class::<PyMassIndex>()?;
    m.add_class::<PyNatr>()?;
    m.add_class::<PyStdDev>()?;
    m.add_class::<PyUlcerIndex>()?;
    m.add_class::<PyHistoricalVolatility>()?;
    m.add_class::<PyBollingerBandwidth>()?;
    m.add_class::<PyPercentB>()?;
    m.add_class::<PyAdl>()?;
    m.add_class::<PyVolumePriceTrend>()?;
    m.add_class::<PyChaikinMoneyFlow>()?;
    m.add_class::<PyChaikinOscillator>()?;
    m.add_class::<PyForceIndex>()?;
    m.add_class::<PyKvo>()?;
    m.add_class::<PyVolumeOscillator>()?;
    m.add_class::<PyNvi>()?;
    m.add_class::<PyPvi>()?;
    m.add_class::<PyAdOscillator>()?;
    m.add_class::<PyAnchoredRsi>()?;
    m.add_class::<PyAnchoredVwap>()?;
    m.add_class::<PyDemandIndex>()?;
    m.add_class::<PyTsv>()?;
    m.add_class::<PyVzo>()?;
    m.add_class::<PyMarketFacilitationIndex>()?;
    m.add_class::<PyEaseOfMovement>()?;
    m.add_class::<PySuperTrend>()?;
    m.add_class::<PyChandelierExit>()?;
    m.add_class::<PyChandeKrollStop>()?;
    m.add_class::<PyAtrTrailingStop>()?;
    m.add_class::<PyHiLoActivator>()?;
    m.add_class::<PyVoltyStop>()?;
    m.add_class::<PyYoyoExit>()?;
    m.add_class::<PyDonchianStop>()?;
    m.add_class::<PyPercentageTrailingStop>()?;
    m.add_class::<PyStepTrailingStop>()?;
    m.add_class::<PyRenkoTrailingStop>()?;
    m.add_class::<PyKaseDevStop>()?;
    m.add_class::<PyElderSafeZone>()?;
    m.add_class::<PyAtrRatchet>()?;
    m.add_class::<PyNrtr>()?;
    m.add_class::<PyModifiedMaStop>()?;
    m.add_class::<PyTypicalPrice>()?;
    m.add_class::<PyMedianPrice>()?;
    m.add_class::<PyWeightedClose>()?;
    m.add_class::<PyLinearRegression>()?;
    m.add_class::<PyLinRegSlope>()?;
    m.add_class::<PyAcceleratorOscillator>()?;
    m.add_class::<PyBalanceOfPower>()?;
    m.add_class::<PyChoppinessIndex>()?;
    m.add_class::<PyVerticalHorizontalFilter>()?;
    m.add_class::<PyTrueRange>()?;
    m.add_class::<PyChaikinVolatility>()?;
    m.add_class::<PyZScore>()?;
    m.add_class::<PyLinRegAngle>()?;
    m.add_class::<PyAlma>()?;
    m.add_class::<PyFrama>()?;
    m.add_class::<PyMcGinleyDynamic>()?;
    m.add_class::<PyVidya>()?;
    m.add_class::<PyJma>()?;
    m.add_class::<PyAlligator>()?;
    m.add_class::<PyEvwma>()?;
    m.add_class::<PyApo>()?;
    m.add_class::<PyAoHist>()?;
    m.add_class::<PyCfo>()?;
    m.add_class::<PyZeroLagMacd>()?;
    m.add_class::<PyElderImpulse>()?;
    m.add_class::<PyStc>()?;
    m.add_class::<PyRviVolatility>()?;
    m.add_class::<PyParkinsonVolatility>()?;
    m.add_class::<PyGarmanKlassVolatility>()?;
    m.add_class::<PyRogersSatchellVolatility>()?;
    m.add_class::<PyYangZhangVolatility>()?;
    m.add_class::<PyMaEnvelope>()?;
    m.add_class::<PyAccelerationBands>()?;
    m.add_class::<PyStarcBands>()?;
    m.add_class::<PyAtrBands>()?;
    m.add_class::<PyHurstChannel>()?;
    m.add_class::<PyLinRegChannel>()?;
    m.add_class::<PyStandardErrorBands>()?;
    m.add_class::<PyQuartileBands>()?;
    m.add_class::<PyBomarBands>()?;
    m.add_class::<PyMedianChannel>()?;
    m.add_class::<PyProjectionBands>()?;
    m.add_class::<PyDoubleBollinger>()?;
    m.add_class::<PyTtmSqueeze>()?;
    m.add_class::<PyFractalChaosBands>()?;
    m.add_class::<PyVwapStdDevBands>()?;
    m.add_class::<PyClassicPivots>()?;
    m.add_class::<PyFibonacciPivots>()?;
    m.add_class::<PyCamarilla>()?;
    m.add_class::<PyWoodiePivots>()?;
    m.add_class::<PyDemarkPivots>()?;
    m.add_class::<PyWilliamsFractals>()?;
    m.add_class::<PyZigZag>()?;
    m.add_class::<PyCentralPivotRange>()?;
    m.add_class::<PyMurreyMathLines>()?;
    m.add_class::<PyAndrewsPitchfork>()?;
    m.add_class::<PyVolumeWeightedSr>()?;
    m.add_class::<PyPivotReversal>()?;
    m.add_class::<PyTdSetup>()?;
    m.add_class::<PyTdSequential>()?;
    m.add_class::<PyTdDeMarker>()?;
    m.add_class::<PyTdRei>()?;
    m.add_class::<PyTdPressure>()?;
    m.add_class::<PyTdCombo>()?;
    m.add_class::<PyTdDWave>()?;
    m.add_class::<PyTdMovingAverage>()?;
    m.add_class::<PyTdCountdown>()?;
    m.add_class::<PyTdLines>()?;
    m.add_class::<PyTdRangeProjection>()?;
    m.add_class::<PyTdDifferential>()?;
    m.add_class::<PyTdOpen>()?;
    m.add_class::<PyTdRiskLevel>()?;
    // Family 10 — Ehlers / Cycle
    m.add_class::<PySuperSmoother>()?;
    m.add_class::<PyFisherTransform>()?;
    m.add_class::<PyInverseFisherTransform>()?;
    m.add_class::<PyDecycler>()?;
    m.add_class::<PyDecyclerOscillator>()?;
    m.add_class::<PyRoofingFilter>()?;
    m.add_class::<PyCenterOfGravity>()?;
    m.add_class::<PyCyberneticCycle>()?;
    m.add_class::<PyInstantaneousTrendline>()?;
    m.add_class::<PyEhlersStochastic>()?;
    m.add_class::<PyEmd>()?;
    m.add_class::<PyHilbertDominantCycle>()?;
    m.add_class::<PyHtDcPhase>()?;
    m.add_class::<PyHtTrendMode>()?;
    m.add_class::<PyAdaptiveCycle>()?;
    m.add_class::<PySineWave>()?;
    m.add_class::<PyMama>()?;
    m.add_class::<PyFama>()?;
    // Family 13 — Ichimoku & alternative charts
    m.add_class::<PyIchimoku>()?;
    m.add_class::<PyHeikinAshi>()?;
    m.add_class::<PySmoothedHeikinAshi>()?;
    m.add_class::<PyHeikinAshiOscillator>()?;
    m.add_class::<PyThreeLineBreak>()?;
    m.add_class::<PyEquivolume>()?;
    m.add_class::<PyCandleVolume>()?;
    m.add_class::<PyVariance>()?;
    m.add_class::<PyCoefficientOfVariation>()?;
    m.add_class::<PySkewness>()?;
    m.add_class::<PyKurtosis>()?;
    m.add_class::<PyStandardError>()?;
    m.add_class::<PyDetrendedStdDev>()?;
    m.add_class::<PyRSquared>()?;
    m.add_class::<PyAutocorrelation>()?;
    m.add_class::<PyMedianAbsoluteDeviation>()?;
    m.add_class::<PyHurstExponent>()?;
    m.add_class::<PyPearsonCorrelation>()?;
    m.add_class::<PyBeta>()?;
    m.add_class::<PyPairwiseBeta>()?;
    m.add_class::<PySpreadAr1Coefficient>()?;
    m.add_class::<PyPairSpreadZScore>()?;
    m.add_class::<PyLeadLagCrossCorrelation>()?;
    m.add_class::<PyCointegration>()?;
    m.add_class::<PyRelativeStrengthAB>()?;
    m.add_class::<PyRollingCorrelation>()?;
    m.add_class::<PyHasbrouckInformationShare>()?;
    m.add_class::<PyRollingCovariance>()?;
    m.add_class::<PyOuHalfLife>()?;
    m.add_class::<PySpreadHurst>()?;
    m.add_class::<PyDistanceSsd>()?;
    m.add_class::<PyBetaNeutralSpread>()?;
    m.add_class::<PyVarianceRatio>()?;
    m.add_class::<PyGrangerCausality>()?;
    m.add_class::<PyKalmanHedgeRatio>()?;
    m.add_class::<PySpreadBollingerBands>()?;
    m.add_class::<PySpearmanCorrelation>()?;
    m.add_class::<PyValueArea>()?;
    m.add_class::<PyVolumeProfile>()?;
    m.add_class::<PyTpoProfile>()?;
    m.add_class::<PyRenkoBars>()?;
    m.add_class::<PyKagiBars>()?;
    m.add_class::<PyPointAndFigureBars>()?;
    m.add_class::<PyRangeBars>()?;
    m.add_class::<PyTickBars>()?;
    m.add_class::<PyVolumeBars>()?;
    m.add_class::<PyDollarBars>()?;
    m.add_class::<PyImbalanceBars>()?;
    m.add_class::<PyRunBars>()?;
    m.add_class::<PyThreeLineBreakBars>()?;
    m.add_class::<PyInitialBalance>()?;
    m.add_class::<PyOpeningRange>()?;
    m.add_class::<PyNakedPoc>()?;
    m.add_class::<PySinglePrints>()?;
    m.add_class::<PyProfileShape>()?;
    m.add_class::<PyHighLowVolumeNodes>()?;
    m.add_class::<PyCompositeProfile>()?;
    // Data layer.
    m.add_class::<PyTickAggregator>()?;
    m.add_class::<PyResampler>()?;
    m.add_class::<PyCandleReader>()?;
    m.add_class::<PyBinanceFeed>()?;
    m.add_function(wrap_pyfunction!(fetch_binance_klines, m)?)?;
    // Candlestick patterns.
    m.add_class::<PyDoji>()?;
    m.add_class::<PyHammer>()?;
    m.add_class::<PyInvertedHammer>()?;
    m.add_class::<PyHangingMan>()?;
    m.add_class::<PyShootingStar>()?;
    m.add_class::<PyEngulfing>()?;
    m.add_class::<PyHarami>()?;
    m.add_class::<PyMorningEveningStar>()?;
    m.add_class::<PyThreeSoldiersOrCrows>()?;
    m.add_class::<PyPiercingDarkCloud>()?;
    m.add_class::<PyMarubozu>()?;
    m.add_class::<PyTweezer>()?;
    m.add_class::<PySpinningTop>()?;
    m.add_class::<PyThreeInside>()?;
    m.add_class::<PyThreeOutside>()?;
    m.add_class::<PyTwoCrows>()?;
    m.add_class::<PyUpsideGapTwoCrows>()?;
    m.add_class::<PyIdenticalThreeCrows>()?;
    m.add_class::<PyThreeLineStrike>()?;
    m.add_class::<PyThreeStarsInSouth>()?;
    m.add_class::<PyAbandonedBaby>()?;
    m.add_class::<PyAdvanceBlock>()?;
    m.add_class::<PyBeltHold>()?;
    m.add_class::<PyBreakaway>()?;
    m.add_class::<PyCounterattack>()?;
    m.add_class::<PyDojiStar>()?;
    m.add_class::<PyDragonflyDoji>()?;
    m.add_class::<PyGravestoneDoji>()?;
    m.add_class::<PyLongLeggedDoji>()?;
    m.add_class::<PyRickshawMan>()?;
    m.add_class::<PyEveningDojiStar>()?;
    m.add_class::<PyMorningDojiStar>()?;
    m.add_class::<PyGapSideBySideWhite>()?;
    m.add_class::<PyHighWave>()?;
    m.add_class::<PyHikkake>()?;
    m.add_class::<PyHikkakeModified>()?;
    m.add_class::<PyHomingPigeon>()?;
    m.add_class::<PyOnNeck>()?;
    m.add_class::<PyInNeck>()?;
    m.add_class::<PyThrusting>()?;
    m.add_class::<PySeparatingLines>()?;
    m.add_class::<PyKicking>()?;
    m.add_class::<PyKickingByLength>()?;
    m.add_class::<PyLadderBottom>()?;
    m.add_class::<PyMatHold>()?;
    m.add_class::<PyMatchingLow>()?;
    m.add_class::<PyLongLine>()?;
    m.add_class::<PyShortLine>()?;
    m.add_class::<PyRisingThreeMethods>()?;
    m.add_class::<PyFallingThreeMethods>()?;
    m.add_class::<PyUpsideGapThreeMethods>()?;
    m.add_class::<PyDownsideGapThreeMethods>()?;
    m.add_class::<PyStalledPattern>()?;
    m.add_class::<PyStickSandwich>()?;
    m.add_class::<PyTakuri>()?;
    m.add_class::<PyClosingMarubozu>()?;
    m.add_class::<PyOpeningMarubozu>()?;
    m.add_class::<PyTasukiGap>()?;
    m.add_class::<PyUniqueThreeRiver>()?;
    m.add_class::<PyConcealingBabySwallow>()?;
    // Microstructure: order book.
    m.add_class::<PyOrderBookImbalanceTop1>()?;
    m.add_class::<PyOrderBookImbalanceTopN>()?;
    m.add_class::<PyOrderBookImbalanceFull>()?;
    m.add_class::<PyMicroprice>()?;
    m.add_class::<PyQuotedSpread>()?;
    m.add_class::<PyDepthSlope>()?;
    // Microstructure: trade flow.
    m.add_class::<PySignedVolume>()?;
    m.add_class::<PyCumulativeVolumeDelta>()?;
    m.add_class::<PyTradeImbalance>()?;
    m.add_class::<PyTradeSignAutocorrelation>()?;
    m.add_class::<PyPin>()?;
    m.add_class::<PyOrderFlowImbalance>()?;
    m.add_class::<PyVpin>()?;
    m.add_class::<PyAmihudIlliquidity>()?;
    m.add_class::<PyRollMeasure>()?;
    // Microstructure: price impact.
    m.add_class::<PyEffectiveSpread>()?;
    m.add_class::<PyRealizedSpread>()?;
    m.add_class::<PyKylesLambda>()?;
    // Microstructure: footprint.
    m.add_class::<PyFootprint>()?;
    // Derivatives.
    m.add_class::<PyFundingRate>()?;
    m.add_class::<PyFundingRateMean>()?;
    m.add_class::<PyFundingRateZScore>()?;
    m.add_class::<PyFundingBasis>()?;
    m.add_class::<PyOpenInterestDelta>()?;
    m.add_class::<PyOIPriceDivergence>()?;
    m.add_class::<PyOIWeighted>()?;
    m.add_class::<PyLongShortRatio>()?;
    m.add_class::<PyTakerBuySellRatio>()?;
    m.add_class::<PyLiquidationFeatures>()?;
    m.add_class::<PyTermStructureBasis>()?;
    m.add_class::<PyCalendarSpread>()?;
    m.add_class::<PyEstimatedLeverageRatio>()?;
    m.add_class::<PyOiToVolumeRatio>()?;
    m.add_class::<PyPerpetualPremiumIndex>()?;
    m.add_class::<PyFundingImpliedApr>()?;
    m.add_class::<PyOpenInterestMomentum>()?;
    m.add_class::<PyAdvanceDecline>()?;
    m.add_class::<PyAdvanceDeclineRatio>()?;
    m.add_class::<PyAdVolumeLine>()?;
    m.add_class::<PyMcClellanOscillator>()?;
    m.add_class::<PyMcClellanSummationIndex>()?;
    m.add_class::<PyTrin>()?;
    m.add_class::<PyBreadthThrust>()?;
    m.add_class::<PyNewHighsNewLows>()?;
    m.add_class::<PyHighLowIndex>()?;
    m.add_class::<PyPercentAboveMa>()?;
    m.add_class::<PyUpDownVolumeRatio>()?;
    m.add_class::<PyBullishPercentIndex>()?;
    m.add_class::<PyCumulativeVolumeIndex>()?;
    m.add_class::<PyAbsoluteBreadthIndex>()?;
    m.add_class::<PyTickIndex>()?;
    // Family 15: Risk / Performance metrics.
    m.add_class::<PySharpeRatio>()?;
    m.add_class::<PySortinoRatio>()?;
    m.add_class::<PyCalmarRatio>()?;
    m.add_class::<PyOmegaRatio>()?;
    m.add_class::<PyMaxDrawdown>()?;
    m.add_class::<PyAverageDrawdown>()?;
    m.add_class::<PyDrawdownDuration>()?;
    m.add_class::<PyPainIndex>()?;
    m.add_class::<PyValueAtRisk>()?;
    m.add_class::<PyConditionalValueAtRisk>()?;
    m.add_class::<PyProfitFactor>()?;
    m.add_class::<PyGainLossRatio>()?;
    m.add_class::<PyRecoveryFactor>()?;
    m.add_class::<PyKellyCriterion>()?;
    m.add_class::<PyTreynorRatio>()?;
    m.add_class::<PyInformationRatio>()?;
    m.add_class::<PyAlpha>()?;
    m.add_class::<PyPlusDi>()?;
    m.add_class::<PyMinusDi>()?;
    m.add_class::<PyDx>()?;
    m.add_class::<PyMidPrice>()?;
    m.add_class::<PyMidPoint>()?;
    m.add_class::<PyAvgPrice>()?;
    m.add_class::<PyRocp>()?;
    m.add_class::<PyRocr>()?;
    m.add_class::<PyRocr100>()?;
    m.add_class::<PyLinRegIntercept>()?;
    m.add_class::<PyTsf>()?;
    // Family 16: Seasonality & Session.
    m.add_class::<PySessionVwap>()?;
    m.add_class::<PySessionHighLow>()?;
    m.add_class::<PySessionRange>()?;
    m.add_class::<PyAverageDailyRange>()?;
    m.add_class::<PyOvernightGap>()?;
    m.add_class::<PyOvernightIntradayReturn>()?;
    m.add_class::<PyTurnOfMonth>()?;
    m.add_class::<PySeasonalZScore>()?;
    m.add_class::<PyTimeOfDayReturnProfile>()?;
    m.add_class::<PyDayOfWeekProfile>()?;
    m.add_class::<PyIntradayVolatilityProfile>()?;
    m.add_class::<PyVolumeByTimeProfile>()?;
    m.add_class::<PyDoubleTopBottom>()?;
    m.add_class::<PyTripleTopBottom>()?;
    m.add_class::<PyHeadAndShoulders>()?;
    m.add_class::<PyTriangle>()?;
    m.add_class::<PyWedge>()?;
    m.add_class::<PyFlagPennant>()?;
    m.add_class::<PyRectangleRange>()?;
    m.add_class::<PyCupAndHandle>()?;
    m.add_class::<PyAbcd>()?;
    m.add_class::<PyGartley>()?;
    m.add_class::<PyButterfly>()?;
    m.add_class::<PyBat>()?;
    m.add_class::<PyCrab>()?;
    m.add_class::<PyShark>()?;
    m.add_class::<PyCypher>()?;
    m.add_class::<PyThreeDrives>()?;
    // Fibonacci.
    m.add_class::<PyFibRetracement>()?;
    m.add_class::<PyFibExtension>()?;
    m.add_class::<PyFibProjection>()?;
    m.add_class::<PyAutoFib>()?;
    m.add_class::<PyGoldenPocket>()?;
    m.add_class::<PyFibConfluence>()?;
    m.add_class::<PyFibFan>()?;
    m.add_class::<PyFibArcs>()?;
    m.add_class::<PyFibChannel>()?;
    m.add_class::<PyFibTimeZones>()?;
    m.add_class::<PyLogReturn>()?;
    m.add_class::<PyRealizedVolatility>()?;
    m.add_class::<PyRollingIqr>()?;
    m.add_class::<PyRollingPercentileRank>()?;
    m.add_class::<PyRollingQuantile>()?;
    m.add_class::<PyCloseVsOpen>()?;
    m.add_class::<PyBodySizePct>()?;
    m.add_class::<PyWickRatio>()?;
    m.add_class::<PyHighLowRange>()?;
    m.add_class::<PyTrendLabel>()?;
    m.add_class::<PyJumpIndicator>()?;
    m.add_class::<PyRegimeLabel>()?;
    m.add_class::<PyWinRate>()?;
    m.add_class::<PyExpectancy>()?;
    m.add_class::<PySineWeightedMa>()?;
    m.add_class::<PyGeometricMa>()?;
    m.add_class::<PyEhma>()?;
    m.add_class::<PyMedianMa>()?;
    m.add_class::<PyAdaptiveLaguerreFilter>()?;
    m.add_class::<PyDisparityIndex>()?;
    m.add_class::<PyFisherRsi>()?;
    m.add_class::<PyRsx>()?;
    m.add_class::<PyDynamicMomentumIndex>()?;
    m.add_class::<PyStochasticCci>()?;
    m.add_class::<PyTtmTrend>()?;
    m.add_class::<PyTrendStrengthIndex>()?;
    m.add_class::<PyQstick>()?;
    m.add_class::<PyPolarizedFractalEfficiency>()?;
    m.add_class::<PyWavePm>()?;
    m.add_class::<PyGatorOscillator>()?;
    m.add_class::<PyKasePermissionStochastic>()?;
    m.add_class::<PyTsfOscillator>()?;
    m.add_class::<PyMacdHistogram>()?;
    m.add_class::<PyPpoHistogram>()?;
    m.add_class::<PyBipowerVariation>()?;
    m.add_class::<PyVolatilityRatio>()?;
    m.add_class::<PyEwmaVolatility>()?;
    m.add_class::<PyGarch11>()?;
    m.add_class::<PyVolatilityOfVolatility>()?;
    m.add_class::<PyVolatilityCone>()?;
    m.add_class::<PyProjectionOscillator>()?;
    m.add_class::<PyTimeBasedStop>()?;
    m.add_class::<PyVolumeRsi>()?;
    m.add_class::<PyWad>()?;
    m.add_class::<PyTwiggsMoneyFlow>()?;
    m.add_class::<PyTradeVolumeIndex>()?;
    m.add_class::<PyIntradayIntensity>()?;
    m.add_class::<PyBetterVolume>()?;
    m.add_class::<PyVolumeWeightedMacd>()?;
    m.add_class::<PyShannonEntropy>()?;
    m.add_class::<PySampleEntropy>()?;
    m.add_class::<PyKendallTau>()?;
    m.add_class::<PyBandpassFilter>()?;
    m.add_class::<PyEvenBetterSinewave>()?;
    m.add_class::<PyAutocorrelationPeriodogram>()?;
    m.add_class::<PyJarqueBera>()?;
    m.add_class::<PyRollingMinMaxScaler>()?;
    m.add_class::<PyHighpassFilter>()?;
    m.add_class::<PyReflex>()?;
    m.add_class::<PyTrendflex>()?;
    m.add_class::<PyCorrelationTrendIndicator>()?;
    m.add_class::<PyAdaptiveRsi>()?;
    m.add_class::<PyUniversalOscillator>()?;
    m.add_class::<PyAdaptiveCci>()?;
    m.add_class::<PyTdCamouflage>()?;
    m.add_class::<PyTdClop>()?;
    m.add_class::<PyTdClopwin>()?;
    m.add_class::<PyTdPropulsion>()?;
    m.add_class::<PyTdTrap>()?;
    m.add_class::<PyTristar>()?;
    m.add_class::<PyHaramiCross>()?;
    m.add_class::<PyTowerTopBottom>()?;
    m.add_class::<PyFryPanBottom>()?;
    m.add_class::<PyDumplingTop>()?;
    m.add_class::<PyNewPriceLines>()?;
    m.add_class::<PySterlingRatio>()?;
    m.add_class::<PyBurkeRatio>()?;
    m.add_class::<PyMartinRatio>()?;
    m.add_class::<PyTailRatio>()?;
    m.add_class::<PyKRatio>()?;
    m.add_class::<PyCommonSenseRatio>()?;
    m.add_class::<PyGainToPainRatio>()?;
    m.add_class::<PyUpsidePotentialRatio>()?;
    m.add_class::<PyM2Measure>()?;
    Ok(())
}

// ===== Data layer: tick-to-candle aggregation =====

/// One aggregated candle as `(open, high, low, close, volume, timestamp)`.
type CandleTuple = (f64, f64, f64, f64, f64, i64);

/// Convert a `wickra-data` error into a Python `ValueError`.
fn map_data_err(e: wickra_data::Error) -> PyErr {
    PyValueError::new_err(e.to_string())
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

/// `(symbol, open, high, low, close, volume, open_time, is_closed)`.
type KlineTuple = (String, f64, f64, f64, f64, f64, i64, bool);

/// A live Binance kline feed (blocking poll). The connect / read / reconnect
/// pipeline is the mock-server-tested wickra-data `BinanceKlineStream`, driven on
/// a single-thread tokio runtime; `next` releases the GIL while it waits.
#[pyclass(name = "BinanceFeed", module = "wickra._wickra", skip_from_py_object)]
struct PyBinanceFeed {
    runtime: tokio::runtime::Runtime,
    inner: wickra_data::live::binance::BinanceKlineStream,
}

#[pymethods]
impl PyBinanceFeed {
    #[new]
    #[pyo3(signature = (symbols, interval, base_url = None))]
    fn new(symbols: &str, interval: u8, base_url: Option<&str>) -> PyResult<Self> {
        let iv = binance_interval(interval)
            .ok_or_else(|| PyValueError::new_err("unknown interval code (expected 0..=15)"))?;
        let symbol_list: Vec<String> = symbols
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        if symbol_list.is_empty() {
            return Err(PyValueError::new_err("at least one symbol is required"));
        }
        let mut config = wickra_data::live::binance::BinanceConfig::default();
        if let Some(url) = base_url {
            url.clone_into(&mut config.base_url);
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
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

    /// Poll for the next kline event, waiting up to `timeout_ms`. Returns a
    /// `(symbol, open, high, low, close, volume, open_time, is_closed)` tuple, or
    /// `None` on timeout (call again). Raises once the stream is closed.
    #[pyo3(signature = (timeout_ms = 1000.0))]
    fn next(&mut self, py: Python<'_>, timeout_ms: f64) -> PyResult<Option<KlineTuple>> {
        let dur = std::time::Duration::from_millis(timeout_ms.max(0.0) as u64);
        let runtime = &self.runtime;
        let inner = &mut self.inner;
        let result = py.detach(|| runtime.block_on(tokio::time::timeout(dur, inner.next_event())));
        match result {
            Ok(Ok(Some(ev))) => Ok(Some((
                ev.symbol,
                ev.candle.open,
                ev.candle.high,
                ev.candle.low,
                ev.candle.close,
                ev.candle.volume,
                ev.candle.timestamp,
                ev.is_closed,
            ))),
            Ok(Ok(None) | Err(_)) => Err(PyRuntimeError::new_err("binance feed closed")),
            Err(_) => Ok(None),
        }
    }

    /// Close the stream; subsequent `next` calls raise.
    fn close(&mut self) {
        let runtime = &self.runtime;
        let inner = &mut self.inner;
        let _ = runtime.block_on(inner.close());
    }
}

/// Fetch historical klines from Binance's REST endpoint. `symbol` is the trading
/// pair (case-insensitive, e.g. `"BTCUSDT"`), `interval` the code `0..=15` (the
/// `Interval` declaration order), and `limit` the number of candles to request
/// (`1..=1000`). `start_ms`/`end_ms` are optional inclusive Unix-millisecond
/// bounds; `base_url` overrides the host (omit for production). Returns a list of
/// `(open, high, low, close, volume, timestamp)` tuples. This blocks until the
/// HTTP response arrives, releasing the GIL while it waits.
#[pyfunction]
#[pyo3(signature = (symbol, interval, limit, start_ms = None, end_ms = None, base_url = None))]
fn fetch_binance_klines(
    py: Python<'_>,
    symbol: &str,
    interval: u8,
    limit: u32,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
    base_url: Option<&str>,
) -> PyResult<Vec<CandleTuple>> {
    let iv = binance_interval(interval)
        .ok_or_else(|| PyValueError::new_err("unknown interval code (expected 0..=15)"))?;
    let limit =
        u16::try_from(limit).map_err(|_| PyValueError::new_err("limit must be in 1..=1000"))?;
    let symbol = symbol.to_owned();
    let base_url = base_url.map(str::to_owned);
    let candles = py
        .detach(move || match base_url {
            Some(url) => {
                let config = wickra_data::live::binance_rest::BinanceRestConfig { base_url: url };
                wickra_data::live::binance_rest::fetch_klines_with_config(
                    &symbol, iv, limit, start_ms, end_ms, &config,
                )
            }
            None => {
                wickra_data::live::binance_rest::fetch_klines(&symbol, iv, limit, start_ms, end_ms)
            }
        })
        .map_err(map_data_err)?;
    Ok(candles
        .into_iter()
        .map(|c| (c.open, c.high, c.low, c.close, c.volume, c.timestamp))
        .collect())
}

/// Roll trade ticks up into fixed-timeframe OHLCV candles.
#[pyclass(
    name = "TickAggregator",
    module = "wickra._wickra",
    skip_from_py_object
)]
#[derive(Clone)]
struct PyTickAggregator {
    inner: wickra_data::aggregator::TickAggregator,
}

#[pymethods]
impl PyTickAggregator {
    #[new]
    #[pyo3(signature = (bucket, gap_fill = false))]
    fn new(bucket: i64, gap_fill: bool) -> PyResult<Self> {
        let timeframe = wickra_data::aggregator::Timeframe::new(bucket).map_err(map_data_err)?;
        let mut inner = wickra_data::aggregator::TickAggregator::new(timeframe);
        if gap_fill {
            inner = inner.with_gap_fill(true);
        }
        Ok(Self { inner })
    }

    /// Push one trade tick; returns the candles closed as a result, each a
    /// `(open, high, low, close, volume, timestamp)` tuple.
    fn push(&mut self, price: f64, size: f64, timestamp: i64) -> PyResult<Vec<CandleTuple>> {
        let tick = wc::Tick::new(price, size, timestamp).map_err(map_err)?;
        Ok(self
            .inner
            .push(tick)
            .map_err(map_data_err)?
            .into_iter()
            .map(|c| (c.open, c.high, c.low, c.close, c.volume, c.timestamp))
            .collect())
    }

    #[getter]
    fn fills_gaps(&self) -> bool {
        self.inner.fills_gaps()
    }

    fn __repr__(&self) -> String {
        format!("TickAggregator(fills_gaps={})", self.inner.fills_gaps())
    }
}

// ===== Data layer: resampling (candle -> higher-timeframe candle) =====

/// Resample candles into a higher timeframe (e.g. 1m -> 5m).
#[pyclass(name = "Resampler", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyResampler {
    inner: wickra_data::resample::Resampler,
}

#[pymethods]
impl PyResampler {
    #[new]
    #[pyo3(signature = (timeframe, gap_fill = false))]
    fn new(timeframe: i64, gap_fill: bool) -> PyResult<Self> {
        let tf = wickra_data::aggregator::Timeframe::new(timeframe).map_err(map_data_err)?;
        Ok(Self {
            inner: wickra_data::resample::Resampler::new(tf).with_gap_fill(gap_fill),
        })
    }

    /// Whether the resampler emits a flat placeholder candle for skipped
    /// buckets.
    #[getter]
    fn fills_gaps(&self) -> bool {
        self.inner.fills_gaps()
    }

    /// Push one candle; returns the higher-timeframe candles it completed, each
    /// `(open, high, low, close, volume, timestamp)`. Normally that is an empty
    /// list or one element; with gap filling on, input that skips whole buckets
    /// completes several at once.
    fn update(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: i64,
    ) -> PyResult<Vec<CandleTuple>> {
        let candle = wc::Candle::new(open, high, low, close, volume, timestamp).map_err(map_err)?;
        Ok(self
            .inner
            .push(candle)
            .map_err(map_data_err)?
            .into_iter()
            .map(|c| (c.open, c.high, c.low, c.close, c.volume, c.timestamp))
            .collect())
    }

    /// Emit the final, still-open candle (or `None` if none is pending).
    fn flush(&mut self) -> PyResult<Option<CandleTuple>> {
        Ok(self
            .inner
            .flush()
            .map_err(map_data_err)?
            .map(|c| (c.open, c.high, c.low, c.close, c.volume, c.timestamp)))
    }
}

// ===== Data layer: CSV candle reader =====

/// Parse OHLCV candles from a CSV string (header `timestamp,open,high,low,close,
/// volume`; a leading UTF-8 BOM is stripped).
#[pyclass(name = "CandleReader", module = "wickra._wickra", skip_from_py_object)]
#[derive(Clone)]
struct PyCandleReader {
    candles: Vec<wc::Candle>,
}

#[pymethods]
impl PyCandleReader {
    #[new]
    fn new(csv: &str) -> PyResult<Self> {
        let mut reader =
            wickra_data::csv::CandleReader::from_reader(csv.as_bytes()).map_err(map_data_err)?;
        let candles = reader.read_all().map_err(map_data_err)?;
        Ok(Self { candles })
    }

    /// Return every parsed candle as `(open, high, low, close, volume, timestamp)`.
    fn read(&self) -> Vec<CandleTuple> {
        self.candles
            .iter()
            .map(|c| (c.open, c.high, c.low, c.close, c.volume, c.timestamp))
            .collect()
    }
}
