//! Direct bug-report adapters: typed engine evidence, never a CLI invocation.
//!
//! A raised oxblend exception (with its owned source-diagnostic snapshot) or
//! an owned result report feeds the SAME engine preview core and
//! report_privacy sanitizers as `oxblend report --preview`. No ledger ID,
//! receipt or fake saved CLI envelope is fabricated; the produced report
//! states `library call; no CLI invocation binding` honestly. Python inputs
//! are converted to owned typed values first; only engine work runs detached.
use super::{NativeResult, call, exception, fail, json_value};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use serde_json::Value as Json;
use std::path::PathBuf;

/// Typed owned engine source-diagnostic snapshot as carried by exceptions.
/// Attributes document the first producer; `to_json()` returns the ledger
/// record shape without an id (no invocation ledger exists to name one).
#[pyclass(frozen, module = "oxblend._native")]
struct SourceDiagnostic {
    snapshot: oxblend::failure_receipt::SourceSnapshot,
}
impl SourceDiagnostic {
    fn new(snapshot: oxblend::failure_receipt::SourceSnapshot) -> Self {
        Self { snapshot }
    }
}
#[pymethods]
impl SourceDiagnostic {
    /// Error kind at first capture; later reclassifications propagate.
    #[getter]
    fn kind(&self) -> &str {
        self.snapshot.kind
    }
    /// Instrumented owner check that first captured this evidence.
    #[getter]
    fn check_id(&self) -> &str {
        self.snapshot.check_id
    }
    /// Owned JSON-compatible projection: ledger record shape, no id.
    fn to_json<'py>(&self, py: Python<'py>) -> PyResult<Py<PyAny>> {
        json_value(py, self.snapshot.record_json())
    }
    fn __repr__(&self) -> String {
        format!(
            "SourceDiagnostic(kind={}, check_id={})",
            self.snapshot.kind, self.snapshot.check_id
        )
    }
}
/// Parent exception-helper hook: the typed attribute value for a raised
/// engine error, or `None` when no instrumented owner check captured it.
pub(super) fn source_diagnostic_value(
    py: Python<'_>,
    error: Option<&oxblend::Error>,
) -> Option<Py<PyAny>> {
    let snapshot = error.and_then(oxblend::Error::source_snapshot)?;
    Py::new(py, SourceDiagnostic::new(snapshot))
        .ok()
        .map(Py::into_any)
}
/// Owned JSON-compatible Python value -> engine JSON. Accepts exactly the
/// scalar/container shapes `json_value` produces (and typed report
/// `.to_json()` results); any other object is a usage error, never coerced.
fn py_to_json(value: &Bound<'_, PyAny>) -> NativeResult<Json> {
    fn convert(
        value: &Bound<'_, PyAny>,
        active: &mut std::collections::BTreeSet<usize>,
    ) -> NativeResult<Json> {
        if value.is_none() {
            return Ok(Json::Null);
        }
        if let Ok(boolean) = value.cast::<PyBool>() {
            return Ok(Json::Bool(boolean.is_true()));
        }
        if let Ok(int) = value.cast::<PyInt>() {
            if let Ok(signed) = int.extract::<i64>() {
                return Ok(Json::Number(signed.into()));
            }
            let unsigned = int.extract::<u64>().map_err(|_| {
                fail(
                    "usage",
                    "result integer is outside the engine JSON value range",
                )
            })?;
            return Ok(Json::Number(unsigned.into()));
        }
        if let Ok(float) = value.cast::<PyFloat>() {
            let number = float
                .extract::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .ok_or_else(|| fail("usage", "result float is not a finite JSON number"))?;
            return Ok(Json::Number(number));
        }
        if let Ok(text) = value.cast::<PyString>() {
            return Ok(Json::String(text.extract::<String>().map_err(|_| {
                fail("usage", "result string is not valid Unicode")
            })?));
        }
        let identity = value.as_ptr() as usize;
        if !active.insert(identity) {
            return Err(fail("usage", "cyclic direct report evidence is not JSON"));
        }
        let result = (|| {
            if let Ok(list) = value.cast::<PyList>() {
                return Ok(Json::Array(
                    list.iter()
                        .map(|item| convert(&item, active))
                        .collect::<NativeResult<Vec<_>>>()?,
                ));
            }
            if let Ok(tuple) = value.cast::<PyTuple>() {
                return Ok(Json::Array(
                    tuple
                        .iter()
                        .map(|item| convert(&item, active))
                        .collect::<NativeResult<Vec<_>>>()?,
                ));
            }
            if let Ok(dict) = value.cast::<PyDict>() {
                let mut object = serde_json::Map::new();
                for (key, item) in dict.iter() {
                    let key = key
                        .extract::<String>()
                        .map_err(|_| fail("usage", "result mapping keys must be strings"))?;
                    object.insert(key, convert(&item, active)?);
                }
                return Ok(Json::Object(object));
            }
            // Immutable public Records deliberately expose their owned JSON
            // projection rather than their private MappingProxyType storage.
            if let Ok(method) = value.getattr("to_json") {
                let projected = method
                    .call0()
                    .map_err(|_| fail("usage", "typed evidence to_json() failed"))?;
                return convert(&projected, active);
            }
            Err(fail(
                "usage",
                "result must be JSON-compatible or a typed report with to_json()",
            ))
        })();
        active.remove(&identity);
        result
    }
    convert(value, &mut std::collections::BTreeSet::new())
}
fn string_attribute(object: &Bound<'_, PyAny>, name: &str) -> Option<String> {
    object
        .getattr(name)
        .ok()?
        .cast::<PyString>()
        .ok()?
        .extract::<String>()
        .ok()
}
fn json_attribute(object: &Bound<'_, PyAny>, name: &str) -> NativeResult<Json> {
    match object.getattr(name) {
        Ok(value) if !value.is_none() => py_to_json(&value),
        _ => Ok(Json::Null),
    }
}
/// Extract typed direct-evidence inputs from a raised exception: required
/// kind/message, optional diagnostics/report, and the owned engine snapshot
/// attached to the raised exception by the native adapter.
fn error_inputs(error: &Bound<'_, PyAny>) -> NativeResult<oxblend::report::DirectErrorEvidence> {
    let Some(kind) = string_attribute(error, "kind") else {
        return Err(fail(
            "usage",
            "error must be a raised oxblend exception carrying kind and message",
        ));
    };
    let Some(message) = string_attribute(error, "message") else {
        return Err(fail(
            "usage",
            "error must be a raised oxblend exception carrying kind and message",
        ));
    };
    let snapshot = ["_source_snapshot", "source_snapshot", "source_diagnostic"]
        .iter()
        .find_map(|name| {
            error
                .getattr(*name)
                .ok()
                .and_then(|value| value.extract::<PyRef<'_, SourceDiagnostic>>().ok())
        })
        .map(|diagnostic| diagnostic.snapshot.clone());
    let diagnostics = match json_attribute(error, "diagnostics")? {
        Json::Null => Vec::new(),
        Json::Array(items) => items,
        single => vec![single],
    };
    let report = match error.getattr("report") {
        Ok(value) if !value.is_none() => Some(py_to_json(&value)?),
        _ => None,
    };
    Ok(oxblend::report::DirectErrorEvidence {
        kind,
        message,
        source_snapshot: snapshot,
        diagnostics,
        report,
    })
}
#[pyfunction]
#[pyo3(signature = (output, error=None, result=None, *, include_names=false, include_file=None))]
fn report_direct_preview(
    py: Python<'_>,
    output: PathBuf,
    error: Option<Bound<'_, PyAny>>,
    result: Option<Bound<'_, PyAny>>,
    include_names: bool,
    include_file: Option<PathBuf>,
) -> PyResult<Py<PyAny>> {
    let op = if error.is_some() {
        "report_direct_error"
    } else {
        "report_direct_result"
    };
    // One call span owns both boundary conversion and engine work. The span
    // stays on this thread's stack; only owned Send inputs enter detach.
    let preview = call(op, || {
        if error.is_some() == result.is_some() {
            return Err(fail(
                "usage",
                "pass exactly one raised oxblend error or result report",
            ));
        }
        if let Some(error) = error {
            let evidence = error_inputs(&error)?;
            py.detach(move || {
                Ok(oxblend::report::direct_error_preview(
                    &output,
                    evidence,
                    include_names,
                    include_file.as_deref(),
                )?)
            })
        } else {
            let report = py_to_json(&result.expect("evidence arity checked"))?;
            py.detach(move || {
                Ok(oxblend::report::direct_result_preview(
                    &output,
                    report,
                    include_names,
                    include_file.as_deref(),
                )?)
            })
        }
    })
    .map_err(|e| exception(py, op, e))?;
    json_value(py, preview)
}
pub(super) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(report_direct_preview, m)?)?;
    m.add_class::<SourceDiagnostic>()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_compatible_values_convert_losslessly() {
        Python::initialize();
        Python::attach(|py| {
            let value = py
                .eval(
                    c"{'a': [1, -2, 3.5, True, False, None, 'x'], 'b': ('t',), 'c': {}}",
                    None,
                    None,
                )
                .unwrap();
            let converted = py_to_json(&value).unwrap();
            assert_eq!(
                converted,
                serde_json::json!({"a":[1,-2,3.5,true,false,null,"x"],"b":["t"],"c":{}})
            );
            let objects = py.eval(c"object()", None, None).unwrap();
            assert_eq!(py_to_json(&objects).unwrap_err().kind, "usage");
            let bytes = py.eval(c"b'ab'", None, None).unwrap();
            assert_eq!(py_to_json(&bytes).unwrap_err().kind, "usage");
        });
    }
    #[test]
    fn direct_preview_requires_exactly_one_typed_evidence() {
        Python::initialize();
        Python::attach(|py| {
            let directory =
                std::env::temp_dir().join(format!("oxblend-direct-native-{}", std::process::id()));
            std::fs::create_dir_all(&directory).unwrap();
            let error = py
                .eval(
                    c"type('E', (), {'kind': 'dangling_pointer', 'message': 'm'})()",
                    None,
                    None,
                )
                .unwrap();
            let both = report_direct_preview(
                py,
                directory.join("both.zip"),
                Some(error.clone()),
                Some(error.clone()),
                false,
                None,
            );
            assert_eq!(
                both.unwrap_err()
                    .value(py)
                    .getattr("kind")
                    .unwrap()
                    .extract::<&str>()
                    .unwrap(),
                "usage"
            );
            let neither: Option<Bound<'_, PyAny>> = None;
            let neither = report_direct_preview(
                py,
                directory.join("none.zip"),
                neither.clone(),
                neither,
                false,
                None,
            );
            assert_eq!(
                neither
                    .unwrap_err()
                    .value(py)
                    .getattr("kind")
                    .unwrap()
                    .extract::<&str>()
                    .unwrap(),
                "usage"
            );
            // A raised-error-shaped object without a snapshot still produces
            // an honest direct preview: envelope facts, no fabricated sources.
            let preview = report_direct_preview(
                py,
                directory.join("direct.zip"),
                Some(error),
                None,
                false,
                None,
            )
            .unwrap();
            let preview = preview.bind(py).cast::<PyDict>().unwrap();
            assert_eq!(
                preview
                    .get_item("schema")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "oxblend.report-preview/v1"
            );
            // The parent exception hook: an engine error whose instrumented
            // owner check captured evidence yields the typed attribute even
            // with no CLI ledger (snapshot populated regardless of state).
            let error = oxblend::Error::new("unresolved_persistent", "test site");
            let source = oxblend::failure_receipt::SourceContext::default();
            oxblend::failure_receipt::record_source_failure(
                &error,
                "pointer.namespace.lookup.missing",
                source,
                &[],
            );
            let attribute = source_diagnostic_value(py, Some(&error)).unwrap();
            let diagnostic = attribute.bind(py);
            assert_eq!(
                diagnostic
                    .getattr("kind")
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "unresolved_persistent"
            );
            assert_eq!(
                diagnostic
                    .getattr("check_id")
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "pointer.namespace.lookup.missing"
            );
            let uninstrumented = oxblend::Error::new("usage", "plain");
            assert!(source_diagnostic_value(py, Some(&uninstrumented)).is_none());
            std::fs::remove_dir_all(&directory).unwrap();
        });
    }
}
