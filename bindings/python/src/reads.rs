//! Native read adapters: path-based engine operations over engine-owned
//! captures, and EXR operations through the shared record/value seams.
//! Long engine work releases the GIL through `Python::detach`; report
//! values are exact CLI-projection JSON owned by the engine.
use super::{Failure, NativeResult, call, exception, json_value, operation, read_adapter};
use oxblend::Error;
use pyo3::prelude::*;
use serde_json::{Value as Json, json};
use std::ffi::OsString;
use std::path::PathBuf;

/// One transient engine-owned capture for a path-based read. The capture is
/// opened with the engine's default retention and dropped after the read;
/// captured File methods never come through here. Open diagnostics are
/// preserved out-of-band: attached as the binding-only
/// `operation_diagnostics` root key on success (stripped by the facade's
/// exact CLI projection) and prepended to body diagnostics on failure.
fn with_capture(
    op: &'static str,
    path: &PathBuf,
    work: impl FnOnce(&oxblend::BlendFile) -> NativeResult<Json> + Send,
) -> NativeResult<Json> {
    oxblend::read_reports::admit_read_root(path).map_err(Failure::from)?;
    let session_op = oxblend::Session::open(path);
    let open_notices = session_op.diagnostics.clone();
    let mut session = operation(session_op)?;
    match read_adapter(&mut session, op, work) {
        Ok(mut report) => {
            report["operation_diagnostics"] = Json::Array(super::diagnostic_facts(&open_notices));
            Ok(report)
        }
        Err(mut failure) => {
            let mut notices = open_notices;
            notices.extend(std::mem::take(&mut failure.diagnostics));
            failure.diagnostics = notices;
            Err(failure)
        }
    }
}

/// Per-file fact extraction over one path: canonicalize, capture the input
/// signature before open, extract, then recheck the signature exactly as the
/// CLI does. A source changed during extraction is a typed input_changed.
fn facts_from_path(op: &'static str, path: &PathBuf) -> NativeResult<Json> {
    let canonical = std::fs::canonicalize(path).map_err(oxblend::read_reports::io_kind_error)?;
    let before = oxblend::read_reports::signature(&canonical)?;
    let report = with_capture(op, &canonical, |f| {
        Ok(oxblend::read_reports::facts(f, op, before.clone(), path)?)
    })?;
    if oxblend::read_reports::signature(&canonical)? != before {
        return Err(Failure::from(Error::new(
            "input_changed",
            "source changed during per-file fact extraction",
        )));
    }
    Ok(report)
}

#[pyfunction]
fn inspect(py: Python<'_>, path: PathBuf) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("inspect", move || {
                with_capture("inspect", &path, |f| Ok(oxblend::read_reports::inspect(f)))
            })
        })
        .map_err(|e| exception(py, "inspect", e))?;
    json_value(py, report)
}

#[pyfunction]
fn payload(py: Python<'_>, path: PathBuf, block: usize) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("payload", move || {
                with_capture("payload", &path, |f| {
                    Ok(oxblend::read_reports::payload(f, block)?)
                })
            })
        })
        .map_err(|e| exception(py, "payload", e))?;
    json_value(py, report)
}

#[pyfunction]
fn source_facts(py: Python<'_>, path: PathBuf) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("source-facts", move || {
                facts_from_path("source-facts", &path)
            })
        })
        .map_err(|e| exception(py, "source-facts", e))?;
    json_value(py, report)
}

#[pyfunction]
fn library_facts(py: Python<'_>, path: PathBuf) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("library-facts", move || {
                facts_from_path("library-facts", &path)
            })
        })
        .map_err(|e| exception(py, "library-facts", e))?;
    json_value(py, report)
}

#[pyfunction]
#[pyo3(signature = (path, full=false))]
fn pointer_census(py: Python<'_>, path: PathBuf, full: bool) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("pointer_census", move || {
                with_capture("pointer_census", &path, |f| {
                    Ok(oxblend::file_ops::pointer_census(f)?.json(full))
                })
            })
        })
        .map_err(|e| exception(py, "pointer_census", e))?;
    json_value(py, report)
}

#[pyfunction]
fn exr_header(py: Python<'_>, path: PathBuf) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("exr_header", move || {
                Ok(oxblend::exr_header::read(&path)?.json())
            })
        })
        .map_err(|e| exception(py, "exr_header", e))?;
    json_value(py, report)
}

#[pyfunction]
#[pyo3(signature = (inputs, jobs=None))]
fn exr_scan(py: Python<'_>, inputs: Vec<PathBuf>, jobs: Option<usize>) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("exr_scan", move || {
                // None means available core parallelism, exactly the CLI
                // default; no other limit is invented here.
                let jobs = match jobs {
                    Some(jobs) => jobs,
                    None => std::thread::available_parallelism()
                        .map_err(oxblend::read_reports::io_kind_error)?
                        .get(),
                };
                let mut records = Vec::new();
                let outcome = oxblend::exr_scan::collect(&inputs, jobs, &mut |record| {
                    records.push(record);
                    Ok(Vec::new())
                });
                match outcome {
                    // One sequence in both outcomes: file records in arrival
                    // order, then the summary row — exactly the CLI JSONL.
                    // collect already emitted the summary through the
                    // collector, so records ends with it; the returned copy
                    // is the same row and must not be appended again.
                    Ok(_summary) => Ok(Json::Array(records)),
                    Err(error) => {
                        // Partial rows (and the summary row) stay attached to
                        // the returned typed failure as the exact JSONL
                        // sequence; nothing collected is discarded.
                        let mut failure = Failure::from(error);
                        failure.report = Some(Json::Array(records));
                        Err(failure)
                    }
                }
            })
        })
        .map_err(|e| exception(py, "exr_scan", e))?;
    json_value(py, report)
}

#[pyfunction]
fn exr_pixels(
    py: Python<'_>,
    input: PathBuf,
    output: PathBuf,
    part: usize,
    pass_name: &str,
) -> PyResult<Py<PyAny>> {
    let pass_name = pass_name.to_owned();
    let report = py
        .detach(move || {
            call("exr_pixels", move || {
                oxblend::exr_pixels::extract(&input, part, &pass_name, &output)
                    .map_err(Failure::from)
            })
        })
        .map_err(|e| exception(py, "exr_pixels", e))?;
    json_value(py, report)
}

#[pyfunction]
#[pyo3(signature = (path, limit=10, address=None, dump=false))]
fn block_report(
    py: Python<'_>,
    path: PathBuf,
    limit: i64,
    address: Option<u64>,
    dump: bool,
) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("block_report", move || {
                let mut text = Vec::new();
                oxblend::bat_cli::blocks_to_writer(&path, limit, address, dump, &mut text)?;
                Ok(json!({"text": String::from_utf8_lossy(&text).into_owned()}))
            })
        })
        .map_err(|e| exception(py, "block_report", e))?;
    json_value(py, report)
}

#[pyfunction]
#[pyo3(signature = (path, full_info=false, low_memory=false))]
fn list_dependencies(
    py: Python<'_>,
    path: PathBuf,
    full_info: bool,
    low_memory: bool,
) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("list_dependencies", move || {
                // The exact CLI --json projection: flags map one to one, and
                // strictness policy switches stay engine defaults.
                let mut args: Vec<OsString> = vec!["--json".into()];
                if full_info {
                    args.push("--full-info".into());
                }
                if low_memory {
                    args.push("--low-memory".into());
                }
                let (mut value, notices, error) = oxblend::bat_cli::listing_report(&path, &args)?;
                if !notices.is_empty() {
                    value["operation_diagnostics"] = Json::Array(notices);
                }
                match error {
                    // The engine's actual kind (missing_asset,
                    // missing_library, pattern_members_unknown,
                    // incomplete_trace, missing_input) is preserved with the
                    // full report attached; the facade never guesses.
                    Some(error) => {
                        let mut failure = Failure::from(error);
                        if !value.is_null() {
                            failure.report = Some(value);
                        }
                        Err(failure)
                    }
                    None => Ok(value),
                }
            })
        })
        .map_err(|e| exception(py, "list_dependencies", e))?;
    json_value(py, report)
}

#[pyfunction]
#[pyo3(signature = (paths, recursive=true, low_memory=false))]
fn trace_many(
    py: Python<'_>,
    paths: Vec<PathBuf>,
    recursive: bool,
    low_memory: bool,
) -> PyResult<Py<PyAny>> {
    let (report, notices) = py
        .detach(move || {
            call("trace_many", move || {
                let mut options = oxblend::trace::TraceOptions::default();
                options.mode = if recursive {
                    oxblend::trace::TraceMode::Recursive
                } else {
                    oxblend::trace::TraceMode::PerFile
                };
                options.low_memory = low_memory;
                super::dependency_result(oxblend::multi_root::dependency_operation(
                    &paths, "trace", options, false,
                ))
            })
        })
        .map_err(|e| exception(py, "trace", e))?;
    super::report_output(py, report, notices)
}
#[pyfunction]
#[pyo3(signature = (paths, full_info=false, low_memory=false))]
fn list_many(
    py: Python<'_>,
    paths: Vec<PathBuf>,
    full_info: bool,
    low_memory: bool,
) -> PyResult<Py<PyAny>> {
    let (report, notices) = py
        .detach(move || {
            call("list_many", move || {
                let mut options = oxblend::trace::TraceOptions::default();
                options.low_memory = low_memory;
                super::dependency_result(oxblend::multi_root::dependency_operation(
                    &paths, "list", options, full_info,
                ))
            })
        })
        .map_err(|e| exception(py, "list_dependencies", e))?;
    super::report_output(py, report, notices)
}
pub(super) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(trace_many, m)?)?;
    m.add_function(wrap_pyfunction!(list_many, m)?)?;
    m.add_function(wrap_pyfunction!(inspect, m)?)?;
    m.add_function(wrap_pyfunction!(payload, m)?)?;
    m.add_function(wrap_pyfunction!(source_facts, m)?)?;
    m.add_function(wrap_pyfunction!(library_facts, m)?)?;
    m.add_function(wrap_pyfunction!(pointer_census, m)?)?;
    m.add_function(wrap_pyfunction!(exr_header, m)?)?;
    m.add_function(wrap_pyfunction!(exr_scan, m)?)?;
    m.add_function(wrap_pyfunction!(exr_pixels, m)?)?;
    m.add_function(wrap_pyfunction!(block_report, m)?)?;
    m.add_function(wrap_pyfunction!(list_dependencies, m)?)?;
    Ok(())
}
