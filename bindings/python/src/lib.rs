//! Thin adapters only. No parsing logic, callbacks while locked, or unsafe traits.
mod reads;
mod report_direct;
#[cfg(test)]
mod tests;
mod writers;
use oxblend::tracing::{Outcome, Span};
use oxblend::value::{PathStep, Value};
use oxblend::{BlendFile, Error, Session};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyTuple};
use serde_json::{Value as Json, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

static MODULE_IDENTITY: OnceLock<Json> = OnceLock::new();

create_exception!(_native, OxblendError, PyException);
create_exception!(_native, ClosedFileError, OxblendError);
create_exception!(_native, SourceChangedError, OxblendError);

#[derive(Debug)]
struct Failure {
    kind: &'static str,
    message: String,
    query: Option<usize>,
    diagnostics: Vec<oxblend::Diagnostic>,
    report: Option<Json>,
    native_error: Option<Box<Error>>,
}
type NativeResult<T> = Result<T, Failure>;
impl From<Error> for Failure {
    fn from(e: Error) -> Self {
        // Canonical negative-path parity: Session-only presentation
        // (fetch/read snapshot and snapshot/index via session_context)
        // must not change the semantic kind/message, while every shared
        // canonical context (AtomicOutput create/publish and pack via
        // edit_output::context) is preserved exactly as the CLI emits it.
        // without_context restores only the Session-recorded base, leaving
        // shared prefixes intact, so Python sees exactly the CLI canonical
        // error with no message parsing and no Python prose copies. Kind
        // is never touched by presentation.
        // The Session presentation itself stays available through the typed
        // channels below (operation, diagnostics, diagnostic_ref,
        // source_snapshot), never the message. Original Diagnostic, owned
        // Snapshot and cause chains are carried in native_error, never
        // reforged from public JSON.
        let e = e.without_context();
        Self {
            kind: e.kind,
            message: e.message.clone(),
            native_error: Some(Box::new(e)),
            query: None,
            diagnostics: Vec::new(),
            report: None,
        }
    }
}
fn fail(kind: &'static str, message: impl Into<String>) -> Failure {
    Failure {
        kind,
        message: message.into(),
        query: None,
        diagnostics: Vec::new(),
        report: None,
        native_error: None,
    }
}
fn exception(py: Python<'_>, op: &str, e: Failure) -> PyErr {
    let err = match e.kind {
        "closed_file" => ClosedFileError::new_err(e.message.clone()),
        "input_changed" => SourceChangedError::new_err(e.message.clone()),
        _ => OxblendError::new_err(e.message.clone()),
    };
    let v = err.value(py);
    // The exception's context is an immutable tuple of (key, value) pairs.
    let _ = v.setattr(
        "diagnostic_ref",
        json_value(
            py,
            e.native_error
                .as_deref()
                .map(Error::diagnostic_ref)
                .unwrap_or(Json::Null),
        )
        .unwrap_or_else(|_| py.None()),
    );
    let _ = v.setattr(
        "_source_snapshot",
        report_direct::source_diagnostic_value(py, e.native_error.as_deref())
            .unwrap_or_else(|| py.None()),
    );
    let _ = v.setattr(
        "source_snapshot",
        json_value(
            py,
            e.native_error
                .as_deref()
                .and_then(Error::source_snapshot)
                .map(|source| source.record_json())
                .unwrap_or(Json::Null),
        )
        .unwrap_or_else(|_| py.None()),
    );
    let _ = v.setattr("kind", e.kind);
    let _ = v.setattr("message", e.message);
    let _ = v.setattr("operation", op);
    let notices = e
        .report
        .as_ref()
        .and_then(|r| r.get("operation_diagnostics"));
    let diagnostic_value = if let Some(notices) = notices {
        json_value(py, notices.clone())
    } else {
        diagnostics(py, &e.diagnostics)
    };
    if let Ok(diagnostics) = diagnostic_value {
        let _ = v.setattr("diagnostics", diagnostics);
    }
    // Hard failures do not prove absence of effects. Pack reports, when
    // returned by the engine, are carried by IncompletePackError instead.
    let _ = v.setattr(
        "effects",
        if !matches!(
            op,
            "pack"
                | "pack_sequence"
                | "edit"
                | "rewrite"
                | "extract"
                | "exr_pixels"
                | "report_preview"
                | "report_create"
                | "report_discard"
                | "report_direct_error"
                | "report_direct_result"
        ) {
            "none"
        } else if e.report.is_some() {
            "reported"
        } else {
            "unknown"
        },
    );
    let _ = v.setattr(
        "report",
        e.report
            .map(|r| json_value(py, r))
            .transpose()
            .unwrap_or(None)
            .unwrap_or_else(|| py.None()),
    );
    if let Ok(context) = PyTuple::new(
        py,
        e.query
            .map(|i| vec![("query_index", i)])
            .unwrap_or_default(),
    ) {
        let _ = v.setattr("context", context);
    }
    err
}

// Construct this on the executing thread, inside detach. Engine spans share
// the same TLS parent; no !Send span is moved into a detached closure.
fn call<T>(op: &str, work: impl FnOnce() -> NativeResult<T>) -> NativeResult<T> {
    call_outcome(op, "report", || work().map(|value| (value, Outcome::Ok)))
}
fn call_outcome<T>(
    op: &str,
    policy: &str,
    work: impl FnOnce() -> NativeResult<(T, Outcome)>,
) -> NativeResult<T> {
    let argument_count = match op {
        "blocks" | "explain" | "inspect" | "source-facts" | "library-facts" | "node_tree"
        | "exr_header" => 1,
        "open" | "query" | "initialize" | "byte_array" | "pointer_table" | "trace" | "payload"
        | "pointer_census" | "diff" | "overrides" | "read_path_list" | "exr_scan"
        | "report_discard" => 2,
        "pointer" | "report_create" | "list_dependencies" => 3,
        "exr_pixels" | "block_report" => 4,
        "edit" => 5,
        "rewrite" => 4,
        "get" | "extract" => 7,
        "report_preview" => 9,
        "pack" => 10,
        "report_direct_error" | "report_direct_result" => 5,
        _ => 0,
    };
    let mut span = Span::new(
        "python.call",
        json!({
            "operation":op, "argument_count":argument_count, "effective_policy":policy,
            "native_module":MODULE_IDENTITY.get().cloned().unwrap_or(Json::Null),
            "binding_version":env!("CARGO_PKG_VERSION"),
            "engine_version":env!("OXBLEND_ENGINE_VERSION"),
            "build_commit":env!("OXBLEND_BINDING_BUILD_COMMIT"),
            "run_identity":oxblend::tracing::run_identity()
        }),
    );
    let result = work();
    let outcome = match &result {
        Ok((_, outcome)) => *outcome,
        Err(error) => {
            span.record_error(error.kind, &error.message);
            if matches!(
                error.kind,
                "usage" | "closed_file" | "foreign_block" | "input_changed"
            ) {
                Outcome::Refused
            } else {
                Outcome::Error
            }
        }
    };
    // L5 owns cumulative process outputs, errors and measured resources.
    // Never replace them with an empty read/getter inventory or guessed facts.
    if op == "initialize" {
        oxblend::tracing::root_summary(json!({"native_module":MODULE_IDENTITY.get()}));
    } else if matches!(op, "pack" | "pack_sequence")
        && let Err(error) = &result
        && error.report.is_none()
    {
        oxblend::tracing::root_summary(
            json!({"python_pack":{"effects":"unknown","error_kind":error.kind}}),
        );
    }
    span.end(outcome);
    result.map(|(value, _)| value)
}
fn operation<T>(op: oxblend::Operation<T>) -> NativeResult<T> {
    op.result.map_err(|e| {
        let mut failure = Failure::from(e);
        failure.diagnostics = op.diagnostics;
        failure
    })
}
fn configure_pack(
    mut options: oxblend::pack::PackOptions,
    roots: &[PathBuf],
    project: Option<PathBuf>,
    memory: Option<String>,
    default_memory: oxblend::session::MemoryMode,
) -> NativeResult<oxblend::pack::PackOptions> {
    // Project selection is the CLI-shared gate: absolute selection,
    // shared default projection and the canonical pack_project refusal.
    // Identical resolution keeps the refused display path and stored root
    // byte-identical to the binary.
    options.project_root = oxblend::pack::resolve_project_root(project, roots, roots.len() > 1)?;
    options.memory = memory
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(Failure::from)?
        .unwrap_or(default_memory);
    Ok(options)
}
fn dependency_result(
    op: oxblend::Operation<(Json, Vec<Json>, Option<Error>)>,
) -> NativeResult<(Json, Vec<Json>)> {
    let mut diagnostics = diagnostic_facts(&op.diagnostics);
    let (mut value, notices, error) = operation(op)?;
    diagnostics.extend(notices);
    if let Some(error) = error {
        let mut failure = Failure::from(error);
        if !value.is_null() {
            value["operation_diagnostics"] = json!(diagnostics);
            failure.report = Some(value);
        }
        return Err(failure);
    }
    Ok((value, diagnostics))
}
fn read_adapter<T>(
    session: &mut Session,
    op: &str,
    work: impl FnOnce(&BlendFile) -> NativeResult<T>,
) -> NativeResult<T> {
    let mut body_failure = None;
    let op_result = session.read(|f| {
        work(f).map_err(|failure| {
            let error = failure
                .native_error
                .as_deref()
                .cloned()
                .unwrap_or_else(|| Error::new(failure.kind, failure.message.clone()));
            body_failure = Some(failure);
            error
        })
    });
    let mut result = operation(op_result);
    if let (Err(error), Some(body)) = (&mut result, body_failure) {
        error.query = body.query.or_else(|| (op == "pointer").then_some(0));
        error.diagnostics.extend(body.diagnostics);
    }
    result
}
fn checked_pack_report(mut report: Json, notices: Vec<Json>) -> NativeResult<(Json, Vec<Json>)> {
    let changed = report
        .get("errors")
        .and_then(Json::as_array)
        .and_then(|errors| errors.iter().find(|error| error["kind"] == "input_changed"))
        .map(|error| {
            error["message"]
                .as_str()
                .unwrap_or("captured source changed during pack")
                .to_owned()
        });
    if let Some(message) = changed {
        oxblend::tracing::point(
            "python.report",
            json!({"operation":"pack","complete":false,
            "error_count":report.get("errors").and_then(Json::as_array).map(Vec::len),"diagnostic_count":notices.len()}),
        );
        report["operation_diagnostics"] = json!(notices);
        pack_summary(&report);
        let mut failure = fail("input_changed", message);
        failure.report = Some(report);
        return Err(failure);
    }
    Ok((report, notices))
}
// Opaque identity has no numeric counter, overflow, or cross-session reuse.
#[pyclass(frozen, module = "oxblend._native")]
#[derive(Clone)]
struct BlockInfo {
    index: usize,
    code: Vec<u8>,
    old_address: u64,
    sdna_index: usize,
    count: usize,
    data_offset: usize,
    size: usize,
    session: Arc<()>,
}
impl BlockInfo {
    fn new(file: &BlendFile, index: usize, session: &Arc<()>) -> Self {
        let b = &file.blocks()[index];
        Self {
            index,
            code: b.code.to_vec(),
            old_address: b.old_address,
            sdna_index: b.sdna_index,
            count: b.count,
            data_offset: b.data_offset,
            size: b.size,
            session: session.clone(),
        }
    }
}
#[pymethods]
impl BlockInfo {
    #[getter(code)]
    fn code_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        let _ = call("block.code", || Ok(()));
        PyBytes::new(py, &self.code)
    }
    #[getter]
    fn index(&self) -> usize {
        call("block.index", || Ok(self.index)).unwrap_or(self.index)
    }
    #[getter]
    fn old_address(&self) -> u64 {
        call("block.old_address", || Ok(self.old_address)).unwrap_or(self.old_address)
    }
    #[getter]
    fn sdna_index(&self) -> usize {
        call("block.sdna_index", || Ok(self.sdna_index)).unwrap_or(self.sdna_index)
    }
    #[getter]
    fn count(&self) -> usize {
        call("block.count", || Ok(self.count)).unwrap_or(self.count)
    }
    #[getter]
    fn data_offset(&self) -> usize {
        call("block.data_offset", || Ok(self.data_offset)).unwrap_or(self.data_offset)
    }
    #[getter]
    fn size(&self) -> usize {
        call("block.size", || Ok(self.size)).unwrap_or(self.size)
    }
}
struct Query {
    block: BlockInfo,
    element: usize,
    path: Vec<PathStep>,
    null_terminated: bool,
}
fn convert_queries(py: Python<'_>, op: &str, items: &Bound<'_, PyTuple>) -> PyResult<Vec<Query>> {
    queries(items).inspect_err(|_| {
        let _ = py.detach(|| call::<()>(op, || Err(fail("usage", "invalid query arguments"))));
    })
}
fn queries(items: &Bound<'_, PyTuple>) -> PyResult<Vec<Query>> {
    items
        .iter()
        .map(|q| {
            let (block, element, path, null_terminated): (
                PyRef<'_, BlockInfo>,
                usize,
                Bound<'_, PyTuple>,
                bool,
            ) = q.extract()?;
            let path = path
                .iter()
                .map(|step| {
                    if let Ok(b) = step.cast::<PyBytes>() {
                        Ok(PathStep::Field(b.as_bytes().to_vec()))
                    } else {
                        step.extract::<usize>().map(PathStep::Index)
                    }
                })
                .collect::<PyResult<Vec<_>>>()?;
            Ok(Query {
                block: block.clone(),
                element,
                path,
                null_terminated,
            })
        })
        .collect()
}
#[pyclass(frozen, module = "oxblend._native")]
struct File {
    state: Mutex<Option<Session>>,
    identity: Arc<()>,
    open_diagnostics: Vec<Json>,
    // Caller spelling is report provenance only; Session owns the frozen
    // lexical capture path and all source validity checks.
    source_path: Option<PathBuf>,
}
impl File {
    fn run<T: Send>(
        &self,
        py: Python<'_>,
        op: &str,
        work: impl FnOnce(&BlendFile) -> NativeResult<T> + Send,
    ) -> PyResult<T> {
        let result = py.detach(|| {
            call(op, || {
                let mut guard = self
                    .state
                    .lock()
                    .map_err(|_| fail("lock_poisoned", "reader mutex poisoned"))?;
                let s = guard
                    .as_mut()
                    .ok_or_else(|| fail("closed_file", "file is closed"))?;
                // Session must see an actual body Err so completion guards run
                // on failure and can retain it as SecondaryError. Keep adapter
                // query identity separately, not hidden in an Ok(inner Err).
                read_adapter(s, op, work)
            })
        });
        result.map_err(|e| exception(py, op, e))
    }
    fn report(
        &self,
        py: Python<'_>,
        op: &str,
        policy: &str,
        work: impl FnOnce(&mut Session) -> NativeResult<(Json, Vec<Json>)> + Send,
    ) -> PyResult<Py<PyAny>> {
        let (report, notices) = py
            .detach(|| {
                call_outcome(op, policy, || {
                    let mut guard = self
                        .state
                        .lock()
                        .map_err(|_| fail("lock_poisoned", "reader mutex poisoned"))?;
                    let session = guard.as_mut().ok_or_else(|| fail("closed_file", "file is closed"))?;
                    if !matches!(op, "trace" | "list_dependencies" | "pack_sequence") {
                        session.require_single(op)?;
                    }
                    let mut result = work(session)?;
                    if matches!(op, "pack" | "pack_sequence") { result = checked_pack_report(result.0, result.1)?; }
                    let partial = result.0.get("complete").and_then(Json::as_bool) == Some(false)
                        || result
                            .0
                            .pointer("/requested_coverage/complete")
                            .and_then(Json::as_bool)
                            == Some(false);
                    oxblend::tracing::point("python.report", json!({
                        "operation":op,
                        "complete":result.0.get("complete").or_else(|| result.0.pointer("/requested_coverage/complete")),
                        "error_count":result.0.get("errors").and_then(Json::as_array).map(Vec::len),
                        "diagnostic_count":result.1.len()
                    }));
                    Ok((
                        result,
                        if partial {
                            Outcome::Partial
                        } else {
                            Outcome::Ok
                        },
                    ))
                })
            })
            .map_err(|e| exception(py, op, e))?;
        if matches!(op, "pack" | "pack_sequence") {
            pack_summary(&report);
        }
        report_output(py, report, notices)
    }
    fn facts_report(&self, py: Python<'_>, command: &str) -> PyResult<Py<PyAny>> {
        self.report(py, command, "report", |s| {
            let root = s.root_path().to_path_buf();
            let op = s.read(|f| {
                let canonical =
                    std::fs::canonicalize(&root).map_err(|e| Error::new("io", e.to_string()))?;
                let signature = oxblend::read_reports::signature(&canonical)?;
                oxblend::read_reports::facts(
                    f,
                    command,
                    signature,
                    self.source_path.as_deref().unwrap_or(&root),
                )
            });
            let notices = diagnostic_facts(&op.diagnostics);
            Ok((operation(op)?, notices))
        })
    }
    fn validate(&self, q: &Query) -> NativeResult<()> {
        if Arc::ptr_eq(&self.identity, &q.block.session) {
            Ok(())
        } else {
            Err(fail("foreign_block", "block belongs to another session"))
        }
    }
}
#[pymethods]
impl File {
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| {
            call("close", || {
                self.state
                    .lock()
                    .map(|mut s| {
                        s.take();
                    })
                    .map_err(|_| fail("lock_poisoned", "reader mutex poisoned"))
            })
        })
        .map_err(|e| exception(py, "close", e))
    }
    #[getter]
    fn closed(&self, py: Python<'_>) -> PyResult<bool> {
        py.detach(|| {
            call("closed", || {
                self.state
                    .lock()
                    .map(|s| s.is_none())
                    .map_err(|_| fail("lock_poisoned", "reader mutex poisoned"))
            })
        })
        .map_err(|e| exception(py, "closed", e))
    }
    #[getter]
    fn source_spelling(&self) -> Option<PathBuf> {
        self.source_path.clone()
    }
    #[getter]
    fn root_paths(&self, py: Python<'_>) -> PyResult<Vec<PathBuf>> {
        py.detach(|| {
            call("root_paths", || {
                let guard = self
                    .state
                    .lock()
                    .map_err(|_| fail("lock_poisoned", "reader mutex poisoned"))?;
                let session = guard
                    .as_ref()
                    .ok_or_else(|| fail("closed_file", "file is closed"))?;
                Ok(session
                    .root_paths()
                    .into_iter()
                    .map(std::path::Path::to_path_buf)
                    .collect())
            })
        })
        .map_err(|e| exception(py, "root_paths", e))
    }
    /// Effective decoded-source retention policy, fixed when opened.
    #[getter]
    fn memory(&self, py: Python<'_>) -> PyResult<String> {
        py.detach(|| {
            call("memory", || {
                let guard = self
                    .state
                    .lock()
                    .map_err(|_| fail("lock_poisoned", "reader mutex poisoned"))?;
                guard
                    .as_ref()
                    .map(|s| s.memory().as_str().to_owned())
                    .ok_or_else(|| fail("closed_file", "file is closed"))
            })
        })
        .map_err(|e| exception(py, "memory", e))
    }
    fn validate_sources(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| {
            call("validate_sources", || {
                let mut guard = self
                    .state
                    .lock()
                    .map_err(|_| fail("lock_poisoned", "reader mutex poisoned"))?;
                let session = guard
                    .as_mut()
                    .ok_or_else(|| fail("closed_file", "file is closed"))?;
                operation(session.validate_sources())
            })
        })
        .map_err(|e| exception(py, "validate_sources", e))
    }
    fn header(&self, py: Python<'_>) -> PyResult<(u32, usize, bool, u32, usize)> {
        self.run(py, "header", |f| {
            let h = f.header();
            Ok((
                h.version,
                h.pointer_size,
                h.little_endian,
                h.file_format,
                h.size,
            ))
        })
    }
    #[pyo3(signature = (code=None))]
    fn blocks(&self, py: Python<'_>, code: Option<Vec<u8>>) -> PyResult<Py<PyTuple>> {
        let rows = self.run(py, "blocks", |f| {
            Ok(f.blocks()
                .iter()
                .enumerate()
                .filter_map(|(i, _)| {
                    let row = BlockInfo::new(f, i, &self.identity);
                    code.as_ref()
                        .is_none_or(|c| {
                            row.code.starts_with(c) && row.code[c.len()..].iter().all(|b| *b == 0)
                        })
                        .then_some(row)
                })
                .collect::<Vec<_>>())
        })?;
        let rows = rows
            .into_iter()
            .map(|r| Py::new(py, r))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(PyTuple::new(py, rows)?.unbind())
    }
    fn get(&self, py: Python<'_>, items: &Bound<'_, PyTuple>) -> PyResult<Py<PyTuple>> {
        let qs = convert_queries(py, "get", items)?;
        let vals = self.run(py, "get", |f| {
            qs.iter()
                .enumerate()
                .map(|(i, q)| {
                    self.validate(q)
                        .and_then(|_| {
                            f.get(q.block.index, q.element, &q.path, q.null_terminated)
                                .map(|v| {
                                    let projection = v.json();
                                    (v, projection)
                                })
                                .map_err(Into::into)
                        })
                        .map_err(|mut e: Failure| {
                            e.query = Some(i);
                            e
                        })
                })
                .collect::<NativeResult<Vec<_>>>()
        })?;
        let vals = vals
            .iter()
            .map(|(v, projection)| {
                let typed = value(py, v)?;
                let pair = typed.bind(py).cast::<PyTuple>()?;
                Ok(PyTuple::new(
                    py,
                    [
                        pair.get_item(0)?,
                        pair.get_item(1)?,
                        json_value(py, projection.clone())?.into_bound(py),
                    ],
                )?
                .unbind())
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(PyTuple::new(py, vals)?.unbind())
    }
    fn pointer(&self, py: Python<'_>, items: &Bound<'_, PyTuple>) -> PyResult<Option<BlockInfo>> {
        let mut qs = convert_queries(py, "pointer", items)?;
        if qs.len() != 1 {
            let _ = py.detach(|| {
                call::<()>("pointer", || {
                    Err(fail("usage", "one pointer query required"))
                })
            });
            return Err(exception(
                py,
                "pointer",
                fail("usage", "one pointer query required"),
            ));
        }
        let q = qs.remove(0);
        self.run(py, "pointer", |f| {
            self.validate(&q)?;
            Ok(f.get_pointer(q.block.index, q.element, &q.path)?
                .map(|i| BlockInfo::new(f, i, &self.identity)))
        })
    }
    fn byte_array(
        &self,
        py: Python<'_>,
        items: &Bound<'_, PyTuple>,
        count: usize,
    ) -> PyResult<Py<PyBytes>> {
        let qs = convert_queries(py, "byte_array", items)?;
        let bytes = self.run(py, "byte_array", |f| {
            if qs.len() != 1 {
                return Err(fail("usage", "one array query required"));
            }
            let q = &qs[0];
            self.validate(q)?;
            Ok(f.byte_array(q.block.index, q.element, &q.path, count)?)
        })?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }
    fn pointer_table(
        &self,
        py: Python<'_>,
        items: &Bound<'_, PyTuple>,
        count: usize,
    ) -> PyResult<Vec<Option<BlockInfo>>> {
        let qs = convert_queries(py, "pointer_table", items)?;
        self.run(py, "pointer_table", |f| {
            if qs.len() != 1 {
                return Err(fail("usage", "one array query required"));
            }
            let q = &qs[0];
            self.validate(q)?;
            Ok(f.pointer_table(q.block.index, q.element, &q.path, count)?
                .into_iter()
                .map(|index| index.map(|i| BlockInfo::new(f, i, &self.identity)))
                .collect())
        })
    }
    #[pyo3(signature = (recursive=true, low_memory=false))]
    fn trace(&self, py: Python<'_>, recursive: bool, low_memory: bool) -> PyResult<Py<PyAny>> {
        self.report(py, "trace", "strict", |s| {
            let mode = if recursive {
                oxblend::trace::TraceMode::Recursive
            } else {
                oxblend::trace::TraceMode::PerFile
            };
            let mut options =
                oxblend::trace::TraceOptions::new(mode, oxblend::trace::PointerMode::Strict);
            options.low_memory = low_memory;
            if s.root_paths().len() > 1 {
                dependency_result(oxblend::multi_root::dependency_session(
                    s, "trace", options, false, None,
                ))
            } else {
                let op = oxblend::trace::trace_session(s, options);
                let notices = diagnostic_facts(&op.diagnostics);
                Ok((operation(op)?.json(), notices))
            }
        })
    }
    #[pyo3(signature = (full_info=false, low_memory=false))]
    fn list_dependencies(
        &self,
        py: Python<'_>,
        full_info: bool,
        low_memory: bool,
    ) -> PyResult<Py<PyAny>> {
        self.report(py, "list_dependencies", "report", |s| {
            let mut options = oxblend::trace::TraceOptions::default();
            options.low_memory = low_memory;
            dependency_result(oxblend::multi_root::dependency_session(
                s,
                "list",
                options,
                full_info,
                self.source_path.as_deref(),
            ))
        })
    }
    #[pyo3(signature = (destination, project_root, *, dry_run=false, permissive=false, compress=false, keep_hierarchy=false, memory=None, exclude=Vec::new(), relative_only=false, durability="none"))]
    // Named Python options plus PyO3's self/token are the public boundary,
    // not an internal function that benefits from a second options wrapper.
    #[allow(clippy::too_many_arguments)]
    fn pack(
        &self,
        py: Python<'_>,
        destination: PathBuf,
        project_root: Option<PathBuf>,
        dry_run: bool,
        permissive: bool,
        compress: bool,
        keep_hierarchy: bool,
        memory: Option<String>,
        exclude: Vec<String>,
        relative_only: bool,
        durability: &str,
    ) -> PyResult<Py<PyAny>> {
        self.report(
            py,
            "pack",
            if permissive { "permissive" } else { "strict" },
            |s| {
                let mut options = oxblend::pack::PackOptions::default();
                // Project defaults are shared with CLI sequence selection.
                options.noop = dry_run;
                options.permissive = permissive;
                options.compress = compress;
                options.keep_hierarchy = keep_hierarchy;
                options.exclude = exclude;
                options.relative_only = relative_only;
                options.durability = durability.parse().map_err(Failure::from)?;
                // None keeps the Session's opened retention policy; an
                // explicit mode parses here, and the engine refuses a
                // mismatch against the Session policy as a usage error.
                let roots = s
                    .root_paths()
                    .into_iter()
                    .map(std::path::Path::to_path_buf)
                    .collect::<Vec<_>>();
                let options = configure_pack(options, &roots, project_root, memory, s.memory())?;
                let op = oxblend::pack::pack_session(s, &destination, &options);
                let notices = diagnostic_facts(&op.diagnostics);
                Ok((operation(op)?.json(), notices))
            },
        )
    }
    #[pyo3(signature = (destination, project_root, exclude=Vec::new(), dry_run=false, relative_only=false, compress=false, keep_hierarchy=true, permissive=false, durability="none", memory=None))]
    #[allow(clippy::too_many_arguments)]
    fn pack_sequence(
        &self,
        py: Python<'_>,
        destination: PathBuf,
        project_root: Option<PathBuf>,
        exclude: Vec<String>,
        dry_run: bool,
        relative_only: bool,
        compress: bool,
        keep_hierarchy: bool,
        permissive: bool,
        durability: &str,
        memory: Option<String>,
    ) -> PyResult<Py<PyAny>> {
        self.report(
            py,
            "pack_sequence",
            if permissive { "permissive" } else { "strict" },
            |s| {
                let mut options = oxblend::pack::PackOptions::default();
                options.exclude = exclude;
                options.noop = dry_run;
                options.relative_only = relative_only;
                options.compress = compress;
                options.keep_hierarchy = keep_hierarchy;
                options.permissive = permissive;
                options.durability = durability.parse().map_err(Failure::from)?;
                let roots = s
                    .root_paths()
                    .into_iter()
                    .map(std::path::Path::to_path_buf)
                    .collect::<Vec<_>>();
                let options = configure_pack(options, &roots, project_root, memory, s.memory())?;
                let op = oxblend::pack::pack_session(s, &destination, &options);
                let notices = diagnostic_facts(&op.diagnostics);
                Ok((operation(op)?.json(), notices))
            },
        )
    }
    #[pyo3(signature = (type_name, fields))]
    fn query(&self, py: Python<'_>, type_name: String, fields: Vec<String>) -> PyResult<Py<PyAny>> {
        self.report(py, "query", "report", |s| {
            let op = s.query(&type_name, &fields);
            let notices = diagnostic_facts(&op.diagnostics);
            let mut report = operation(op)?;
            // Match CLI projection: the shared query core has only a reader,
            // so each caller adds its owned source-path context.
            let source = self.source_path.as_deref().unwrap_or(s.root_path());
            report["source_observation"]["path"] = json!(source.to_str());
            report["source_observation"]["path_hex"] =
                json!(oxblend::value::hex(source.as_os_str().as_encoded_bytes()));
            Ok((report, notices))
        })
    }
    fn explain(&self, py: Python<'_>, concept: String) -> PyResult<Py<PyAny>> {
        self.report(py, "explain", "report", |s| {
            let op = s.read(|file| oxblend::query::explain(Some(file), &concept));
            let notices = diagnostic_facts(&op.diagnostics);
            Ok((operation(op)?, notices))
        })
    }
    #[getter]
    fn open_diagnostics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let facts = py
            .detach(|| call("open_diagnostics", || Ok(self.open_diagnostics.clone())))
            .map_err(|e| exception(py, "open_diagnostics", e))?;
        json_value(py, Json::Array(facts))
    }
    fn validate_block(&self, py: Python<'_>, block: PyRef<'_, BlockInfo>) -> PyResult<()> {
        let block = block.clone();
        self.run(py, "block.validate", |_| {
            if Arc::ptr_eq(&self.identity, &block.session) {
                Ok(())
            } else {
                Err(fail("foreign_block", "block belongs to another session"))
            }
        })
    }
    fn inspect(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let report = self.run(py, "inspect", |f| Ok(oxblend::read_reports::inspect(f)))?;
        json_value(py, report)
    }
    fn payload(&self, py: Python<'_>, block: usize) -> PyResult<Py<PyAny>> {
        let report = self.run(py, "payload", |f| {
            Ok(oxblend::read_reports::payload(f, block)?)
        })?;
        json_value(py, report)
    }
    #[pyo3(signature = (full=false))]
    fn pointer_census(&self, py: Python<'_>, full: bool) -> PyResult<Py<PyAny>> {
        let report = self.run(py, "pointer_census", |f| {
            Ok(oxblend::file_ops::pointer_census(f)?.json(full))
        })?;
        json_value(py, report)
    }
    fn source_facts(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.facts_report(py, "source-facts")
    }
    fn library_facts(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.facts_report(py, "library-facts")
    }
    #[pyo3(signature = (block, path, element=0, raw=false, byte_array=None, pointer_table=None))]
    #[allow(clippy::too_many_arguments)]
    fn get_json(
        &self,
        py: Python<'_>,
        block: usize,
        path: String,
        element: usize,
        raw: bool,
        byte_array: Option<usize>,
        pointer_table: Option<usize>,
    ) -> PyResult<Py<PyAny>> {
        let report = self.run(py, "get", |f| {
            let path = oxblend::read_reports::field_path(&path)?;
            Ok(oxblend::read_reports::get(
                f,
                block,
                element,
                &path,
                raw,
                byte_array,
                pointer_table,
            )?)
        })?;
        json_value(py, report)
    }
    #[pyo3(signature = (block, path, element=0))]
    fn pointer_json(
        &self,
        py: Python<'_>,
        block: usize,
        path: String,
        element: usize,
    ) -> PyResult<Py<PyAny>> {
        let report = self.run(py, "pointer", |f| {
            let path = oxblend::read_reports::field_path(&path)?;
            Ok(oxblend::read_reports::pointer(f, block, element, &path)?)
        })?;
        json_value(py, report)
    }
    fn summary(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let (s, projection) = self.run(py, "summary", |f| {
            let summary = oxblend::summary::summarize_typed(f)?;
            let projection = summary.json();
            Ok((summary, projection))
        })?;
        let d = PyDict::new(py);
        match projection {
            Ok(report) => d.set_item("_json", json_value(py, report)?)?,
            Err(error) => d.set_item(
                "_json_error",
                exception(py, "summary", error.into()).value(py),
            )?,
        }
        let h = &s.header;
        d.set_item(
            "header",
            (
                h.version,
                h.pointer_size,
                h.little_endian,
                h.file_format,
                h.size,
            ),
        )?;
        d.set_item(
            "block_codes",
            PyTuple::new(
                py,
                s.block_codes.iter().map(|(c, n)| (PyBytes::new(py, c), *n)),
            )?,
        )?;
        for (key, names) in [
            ("objects", s.objects),
            ("materials", s.materials),
            ("meshes", s.meshes),
            ("images", s.images),
            ("collections", s.collections),
        ] {
            d.set_item(
                key,
                PyTuple::new(py, names.iter().map(|b| PyBytes::new(py, b)))?,
            )?;
        }
        let scenes = s
            .scenes
            .into_iter()
            .map(|s| {
                (
                    s.name.map(|b| PyBytes::new(py, &b).unbind()),
                    s.frame_start,
                    s.frame_end,
                    s.frame_step,
                    s.camera.map(|b| PyBytes::new(py, &b).unbind()),
                    s.fps.numerator,
                    s.fps.base_bits,
                    s.fps.effective,
                )
            })
            .collect::<Vec<_>>();
        d.set_item("scenes", PyTuple::new(py, scenes)?)?;
        d.set_item(
            "warnings",
            PyTuple::new(
                py,
                s.warnings
                    .iter()
                    .map(|w| (&w.kind, w.block_index, w.element, &w.field, &w.message)),
            )?,
        )?;
        Ok(d.into_any().unbind())
    }
}
// Consume an owned engine tree progressively: each Rust subtree is released
// once its Python counterpart exists. Keys are shared only within this one
// conversion (no global interning or retained cache), reducing repeated schema
// string allocations without changing any key/value or public container shape.
fn json_value(py: Python<'_>, value: Json) -> PyResult<Py<PyAny>> {
    json_value_inner(py, value, &mut std::collections::HashMap::new())
}
fn json_value_inner(
    py: Python<'_>,
    value: Json,
    keys: &mut std::collections::HashMap<String, Py<pyo3::types::PyString>>,
) -> PyResult<Py<PyAny>> {
    Ok(match value {
        Json::Null => py.None(),
        Json::Bool(v) => v.into_pyobject(py)?.to_owned().into_any().unbind(),
        Json::Number(v) => {
            if let Some(v) = v.as_u64() {
                v.into_pyobject(py)?.into_any().unbind()
            } else if let Some(v) = v.as_i64() {
                v.into_pyobject(py)?.into_any().unbind()
            } else {
                v.as_f64().unwrap().into_pyobject(py)?.into_any().unbind()
            }
        }
        Json::String(v) => v.into_pyobject(py)?.into_any().unbind(),
        Json::Array(v) => PyTuple::new(
            py,
            v.into_iter()
                .map(|v| json_value_inner(py, v, keys))
                .collect::<PyResult<Vec<_>>>()?,
        )?
        .into_any()
        .unbind(),
        Json::Object(v) => {
            let d = PyDict::new(py);
            for (key, value) in v {
                let python_key = keys
                    .entry(key)
                    .or_insert_with_key(|key| pyo3::types::PyString::new(py, key).unbind())
                    .clone_ref(py);
                d.set_item(python_key, json_value_inner(py, value, keys)?)?;
            }
            d.into_any().unbind()
        }
    })
}
fn diagnostic_facts(values: &[oxblend::Diagnostic]) -> Vec<Json> {
    use oxblend::Diagnostic;
    values.iter().map(|d| match d {
        Diagnostic::SeekTableFallback { path, reason } => json!({"kind":"seek_table_fallback","source_hex":path.as_ref().map(|p|oxblend::value::hex(p.as_os_str().as_encoded_bytes())),"reason":reason}),
        Diagnostic::SourceCheckFailure { source, error } => json!({"kind":"source_check_failure","source_hex":oxblend::value::hex(source.as_os_str().as_encoded_bytes()),"error":{"kind":error.kind,"message":error.message,"diagnostic_ref":error.diagnostic_ref()}}),
        Diagnostic::SecondaryError { phase, source, error } => json!({"kind":"secondary_error","phase":phase,"source_hex":source.as_ref().map(|p|oxblend::value::hex(p.as_os_str().as_encoded_bytes())),"error":{"kind":error.kind,"message":error.message,"diagnostic_ref":error.diagnostic_ref()}}),
        _ => json!({"kind":d.kind(),"unrecognized_variant":true,"debug":format!("{d:?}")}),
    }).collect()
}
fn diagnostics(py: Python<'_>, values: &[oxblend::Diagnostic]) -> PyResult<Py<PyAny>> {
    json_value(py, Json::Array(diagnostic_facts(values)))
}
fn pack_summary(report: &Json) {
    oxblend::tracing::root_summary(json!({
        "python_pack":{"effects":"reported","files":report.get("files"),
            "archive_written":report.get("archive_written"), "info_written":report.get("info_written"),
            "directory_created":report.get("directory_created"), "complete":report.get("complete")}
    }));
}
fn report_output(py: Python<'_>, mut report: Json, notices: Vec<Json>) -> PyResult<Py<PyAny>> {
    report["operation_diagnostics"] = Json::Array(notices);
    json_value(py, report)
}
#[pyfunction]
fn catalog(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(|| call("catalog", || Ok(oxblend::query::catalog())))
        .map_err(|e| exception(py, "catalog", e))?;
    json_value(py, report)
}
#[pyfunction]
#[pyo3(signature = (path, recursive=true, low_memory=false))]
fn trace_path(
    py: Python<'_>,
    path: PathBuf,
    recursive: bool,
    low_memory: bool,
) -> PyResult<Py<PyAny>> {
    let (report, notices) = py
        .detach(|| {
            call("trace", || {
                let mode = if recursive {
                    oxblend::trace::TraceMode::Recursive
                } else {
                    oxblend::trace::TraceMode::PerFile
                };
                let mut options =
                    oxblend::trace::TraceOptions::new(mode, oxblend::trace::PointerMode::Strict);
                options.low_memory = low_memory;
                let op = oxblend::trace::trace_operation(&path, options);
                let notices = diagnostic_facts(&op.diagnostics);
                Ok((operation(op)?.json(), notices))
            })
        })
        .map_err(|e| exception(py, "trace", e))?;
    report_output(py, report, notices)
}
#[pyfunction]
#[pyo3(signature = (kind, command, source=None))]
fn next_command(
    py: Python<'_>,
    kind: String,
    command: String,
    source: Option<PathBuf>,
) -> PyResult<Py<PyAny>> {
    let mut args = vec![std::ffi::OsString::from("oxblend"), command.into()];
    if let Some(source) = source {
        args.push(source.into_os_string());
    }
    json_value(
        py,
        oxblend::error_help::suggestion(&kind, &args).unwrap_or(Json::Null),
    )
}
#[pyfunction]
fn error_catalog(py: Python<'_>) -> PyResult<Py<PyAny>> {
    json_value(py, oxblend::error_help::catalog())
}
#[pyfunction]
fn explain(py: Python<'_>, concept: String) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(|| call("explain", || Ok(oxblend::query::explain(None, &concept)?)))
        .map_err(|e| exception(py, "explain", e))?;
    json_value(py, report)
}
#[pyfunction]
fn initialize_identity(path: PathBuf, sha256: String) {
    MODULE_IDENTITY.get_or_init(|| json!({"path_hex":oxblend::value::hex(path.as_os_str().as_encoded_bytes()),"sha256":sha256,"crate_version":env!("CARGO_PKG_VERSION"),"build_commit":env!("OXBLEND_BINDING_BUILD_COMMIT")}));
    let _ = call("initialize", || Ok(()));
}
fn value(py: Python<'_>, v: &Value) -> PyResult<Py<PyAny>> {
    let (kind, item): (&str, Py<PyAny>) = match v {
        Value::Signed(n) => ("signed", n.into_pyobject(py)?.into_any().unbind()),
        Value::Unsigned(n) => ("unsigned", n.into_pyobject(py)?.into_any().unbind()),
        Value::Pointer(n) => ("pointer", n.into_pyobject(py)?.into_any().unbind()),
        Value::Float32Bits(n) => ("float32", n.into_pyobject(py)?.into_any().unbind()),
        Value::Bytes(b) => ("bytes", PyBytes::new(py, b).into_any().unbind()),
        Value::Array(v) => (
            "array",
            PyTuple::new(
                py,
                v.iter()
                    .map(|v| value(py, v))
                    .collect::<PyResult<Vec<_>>>()?,
            )?
            .into_any()
            .unbind(),
        ),
        _ => {
            return Err(exception(
                py,
                "get",
                fail("unsupported_type", "unrecognized engine value variant"),
            ));
        }
    };
    Ok((kind, item).into_pyobject(py)?.into_any().unbind())
}
#[pyfunction]
#[pyo3(signature = (path, *, memory=None))]
fn open(py: Python<'_>, path: PathBuf, memory: Option<String>) -> PyResult<File> {
    let source_path = Some(path.clone());
    let (s, open_diagnostics) = py
        .detach(|| {
            call("open", || {
                // None keeps the historical fast retention; unknown modes
                // stay usage errors from MemoryMode::from_str.
                let mode: oxblend::session::MemoryMode = memory
                    .as_deref()
                    .unwrap_or("fast")
                    .parse()
                    .map_err(Failure::from)?;
                oxblend::read_reports::admit_read_root(&path).map_err(Failure::from)?;
                let op = Session::open_with_memory(path, mode);
                let notices = diagnostic_facts(&op.diagnostics);
                Ok((operation(op)?, notices))
            })
        })
        .map_err(|e| exception(py, "open", e))?;
    Ok(File {
        state: Mutex::new(Some(s)),
        identity: Arc::new(()),
        open_diagnostics,
        source_path,
    })
}
#[pyfunction]
#[pyo3(signature = (paths, *, memory=None))]
fn open_many(py: Python<'_>, paths: Vec<PathBuf>, memory: Option<String>) -> PyResult<File> {
    let first_spelling = paths.first().cloned();
    let (session, open_diagnostics) = py
        .detach(|| {
            call("open_many", || {
                let mode = memory
                    .as_deref()
                    .unwrap_or("fast")
                    .parse()
                    .map_err(Failure::from)?;
                for path in &paths {
                    oxblend::read_reports::admit_read_root(path).map_err(Failure::from)?;
                }
                let op = Session::open_many(paths, mode);
                let notices = diagnostic_facts(&op.diagnostics);
                Ok((operation(op)?, notices))
            })
        })
        .map_err(|e| exception(py, "open_many", e))?;
    let source_path = if session.root_paths().len() == 1 {
        first_spelling
    } else {
        None
    };
    Ok(File {
        state: Mutex::new(Some(session)),
        identity: Arc::new(()),
        open_diagnostics,
        source_path,
    })
}
// Compiler-enforced ownership evidence. No unsafe Send/Sync assertions.
fn trait_proof() {
    fn send<T: Send>() {}
    fn sync<T: Sync>() {}
    send::<BlendFile>();
    send::<Session>();
    send::<Value>();
    sync::<Mutex<Option<Session>>>();
    send::<File>();
    sync::<File>();
}
#[pymodule(gil_used = false)]
fn _native(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    oxblend::tracing::initialize();
    trait_proof();
    reads::register(m)?;
    writers::register(m)?;
    report_direct::register(m)?;
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_function(wrap_pyfunction!(open_many, m)?)?;
    m.add_function(wrap_pyfunction!(catalog, m)?)?;
    m.add_function(wrap_pyfunction!(error_catalog, m)?)?;
    m.add_function(wrap_pyfunction!(next_command, m)?)?;
    m.add_function(wrap_pyfunction!(trace_path, m)?)?;
    m.add_function(wrap_pyfunction!(explain, m)?)?;
    m.add_function(wrap_pyfunction!(initialize_identity, m)?)?;
    m.add_class::<File>()?;
    m.add_class::<BlockInfo>()?;
    m.add("OxblendError", py.get_type::<OxblendError>())?;
    m.add("ClosedFileError", py.get_type::<ClosedFileError>())?;
    m.add("SourceChangedError", py.get_type::<SourceChangedError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("engine_version", env!("OXBLEND_ENGINE_VERSION"))?;
    m.add("__build_commit__", env!("OXBLEND_BINDING_BUILD_COMMIT"))?;
    Ok(())
}
