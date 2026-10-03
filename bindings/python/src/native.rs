//! Native `float64` memory shared with Python instead of copied.
//!
//! The abi3 wheels target Python 3.9, whose limited API has no buffer protocol,
//! so a series used to cross into Rust as one `tobytes()` copy and a result to
//! leave as a `bytes` object copied into an `array.array`. Two objects tell
//! their address without the buffer protocol: a `NumPy` array through
//! `__array_interface__` and an `array.array` through `buffer_info()`. This
//! module reads and writes through those addresses -- the binding's only
//! `unsafe` -- under the conditions each block states.

#![allow(unsafe_code)]

use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyDict, PyTuple, PyType};

/// `gc.isenabled`, `gc.disable` and `gc.enable`.
static GC: PyOnceLock<(Py<PyAny>, Py<PyAny>, Py<PyAny>)> = PyOnceLock::new();

/// The cyclic garbage collector held off while a [`SharedF64`] may be read.
///
/// The collector is the only way Python code -- a finalizer -- can run in the
/// middle of the binding's own C-level calls: they never release the GIL and
/// never call back into Python, but any allocation of a tracked object may
/// start a collection. With it off, nothing outside the binding touches a
/// shared series between taking its slice and the slice's last use. Restores
/// the collector on drop only if this pause turned it off.
#[derive(Debug)]
struct GcPause {
    resume: bool,
}

impl GcPause {
    fn new(py: Python<'_>) -> PyResult<Self> {
        let (enabled, disable, _) = GC.get_or_try_init(py, || -> PyResult<_> {
            let gc = py.import("gc")?;
            Ok((
                gc.getattr("isenabled")?.unbind(),
                gc.getattr("disable")?.unbind(),
                gc.getattr("enable")?.unbind(),
            ))
        })?;
        let resume = enabled.bind(py).call0()?.is_truthy()?;
        if resume {
            disable.bind(py).call0()?;
        }
        Ok(Self { resume })
    }
}

impl Drop for GcPause {
    fn drop(&mut self) {
        if self.resume {
            Python::attach(|py| {
                if let Some((_, _, enable)) = GC.get(py) {
                    // `gc.enable()` takes no argument and cannot fail.
                    let _ = enable.bind(py).call0();
                }
            });
        }
    }
}

/// A `NumPy` `float64` array or an `array.array('d')` whose values Rust reads in
/// place.
///
/// Only the exact types qualify -- a subclass could compute its interface in
/// Python -- and only a one-dimensional, contiguous, native-order, aligned
/// series of at least [`MIN_SHARED`] values; below that one copy costs less
/// than asking the object where its values are. Taking a value checks the type
/// and the length only; the address is read once, when the slice is taken,
/// after every argument has been converted, and the collector stays off for as
/// long as this value lives, so no Python code runs while the slice can be
/// read.
#[derive(Debug)]
pub(crate) struct SharedF64 {
    obj: Py<PyAny>,
    _gc: GcPause,
}

impl SharedF64 {
    /// `obj` if its values can be read in place, `None` for everything else.
    pub(crate) fn new(obj: &Bound<'_, PyAny>) -> Option<Self> {
        let py = obj.py();
        let ty = obj.get_type();
        let candidate = if ty.is(array_type(py)?) {
            obj.getattr("typecode").ok()?.extract::<String>().ok()? == "d"
        } else {
            is_ndarray(py, &ty)
        };
        if !candidate || obj.len().ok()? < MIN_SHARED {
            return None;
        }
        let gc = GcPause::new(py).ok()?;
        Some(Self {
            obj: obj.clone().unbind(),
            _gc: gc,
        })
    }

    /// The object whose values are shared.
    pub(crate) fn object(&self) -> &Py<PyAny> {
        &self.obj
    }

    /// The values in place, or `None` if the object no longer qualifies (a
    /// conversion of a later argument ran Python code that changed it).
    pub(crate) fn as_slice(&self) -> Option<&[f64]> {
        Python::attach(|py| {
            let (addr, len) = location(self.obj.bind(py))?;
            if len == 0 {
                return Some(&[][..]);
            }
            // SAFETY: `location` just read `addr` and `len` from the object
            // itself: `len` aligned, initialised `f64`s at `addr`, owned by an
            // object `self` keeps alive. Nothing can move, free or write them
            // while the slice is in use: the GIL is held and never released,
            // the binding calls no Python code of the caller's, and `GcPause`
            // keeps a collection -- and any finalizer -- from running until
            // `self` is dropped, which the slice cannot outlive.
            Some(unsafe { std::slice::from_raw_parts(addr as *const f64, len) })
        })
    }
}

/// Series shorter than this are copied: below it, the copy costs less than
/// the calls that find a shared series' values.
const MIN_SHARED: usize = 8192;

/// The address and length of `obj`'s `float64` values if they can be shared.
fn location(obj: &Bound<'_, PyAny>) -> Option<(usize, usize)> {
    let ty = obj.get_type();
    let (addr, len) = if ty.is(array_type(obj.py())?) {
        if obj.getattr("typecode").ok()?.extract::<String>().ok()? != "d" {
            return None;
        }
        obj.call_method0("buffer_info")
            .ok()?
            .extract::<(usize, usize)>()
            .ok()?
    } else if is_ndarray(obj.py(), &ty) {
        ndarray_location(obj)?
    } else {
        return None;
    };
    (len == 0 || addr % std::mem::align_of::<f64>() == 0).then_some((addr, len))
}

/// `numpy.ndarray`, remembered the first time one is seen.
static NDARRAY: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// Whether `ty` is exactly `numpy.ndarray`. The first match is found by name,
/// so `NumPy` is never imported, and remembered; every later check compares
/// the type object.
fn is_ndarray(py: Python<'_>, ty: &Bound<'_, PyType>) -> bool {
    if let Some(ndarray) = NDARRAY.get(py) {
        return ty.is(ndarray.bind(py));
    }
    let name = |attr: &str| {
        ty.getattr(attr)
            .ok()
            .and_then(|v| v.extract::<String>().ok())
    };
    let found = name("__module__").as_deref() == Some("numpy")
        && name("__qualname__").as_deref() == Some("ndarray");
    if found {
        NDARRAY.get_or_init(py, || ty.clone().unbind());
    }
    found
}

/// A one-dimensional, C-contiguous, native little-endian `float64` array's
/// address and length.
fn ndarray_location(obj: &Bound<'_, PyAny>) -> Option<(usize, usize)> {
    if !cfg!(target_endian = "little") {
        return None;
    }
    let interface = obj.getattr("__array_interface__").ok()?;
    let interface = interface.cast::<PyDict>().ok()?;
    let item = |key: &str| interface.get_item(key).ok().flatten();
    if item("typestr")?.extract::<String>().ok()? != "<f8" {
        return None;
    }
    let shape = item("shape")?;
    let shape = shape.cast::<PyTuple>().ok()?;
    if shape.len() != 1 {
        return None;
    }
    let len = shape.get_item(0).ok()?.extract::<usize>().ok()?;
    let strides = item("strides")?;
    if !strides.is_none() && strides.extract::<(usize,)>().ok()? != (std::mem::size_of::<f64>(),) {
        return None;
    }
    let (addr, _readonly) = item("data")?.extract::<(usize, bool)>().ok()?;
    Some((addr, len))
}

/// `array.array`, imported once per interpreter.
static ARRAY: PyOnceLock<Py<PyType>> = PyOnceLock::new();

fn array_type(py: Python<'_>) -> Option<&Bound<'_, PyType>> {
    ARRAY.import(py, "array", "array").ok()
}

/// `array.array('d', [0.0])`, the seed a result array is repeated from.
static ZERO: PyOnceLock<Py<PyAny>> = PyOnceLock::new();

/// Whether `array * n` fills the result in large copies: from Python 3.11 on
/// `CPython` repeats an array by doubling the filled part, before that one
/// element at a time.
static FAST_REPEAT: PyOnceLock<bool> = PyOnceLock::new();

/// A fresh `array.array('d')` of `len` values that Rust writes in place.
#[derive(Debug)]
pub(crate) struct OutArray<'py> {
    array: Bound<'py, PyAny>,
    addr: usize,
    len: usize,
}

impl<'py> OutArray<'py> {
    /// A zeroed array of `len` values, or `None` where building one is not
    /// cheaper than the copy it would replace.
    pub(crate) fn new(py: Python<'py>, len: usize) -> PyResult<Option<Self>> {
        if !*FAST_REPEAT.get_or_init(py, || py.version_info() >= (3, 11)) {
            return Ok(None);
        }
        let zero = ZERO.get_or_try_init(py, || -> PyResult<_> {
            let array = ARRAY.import(py, "array", "array")?;
            Ok(array.call1(("d", [0.0_f64]))?.unbind())
        })?;
        let array = zero.bind(py).mul(len)?;
        let (addr, filled) = array
            .call_method0("buffer_info")?
            .extract::<(usize, usize)>()?;
        if filled != len || (len > 0 && addr % std::mem::align_of::<f64>() != 0) {
            return Ok(None);
        }
        Ok(Some(Self { array, addr, len }))
    }

    /// Let `fill` write every value, then hand the array to Python.
    pub(crate) fn fill(self, fill: impl FnOnce(&mut [f64])) -> Bound<'py, PyAny> {
        let out: &mut [f64] = if self.len == 0 {
            &mut []
        } else {
            // SAFETY: `buffer_info()` gave the address of the array's `len`
            // initialised, aligned `f64`s. The array was just created and is
            // referenced only by `self`, exports no buffer and cannot be
            // resized or read by anything else while `fill` runs, and it
            // outlives the slice.
            unsafe { std::slice::from_raw_parts_mut(self.addr as *mut f64, self.len) }
        };
        fill(out);
        self.array
    }
}
