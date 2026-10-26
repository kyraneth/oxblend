//! Native writer adapters: existing engine operations only, no CLI subprocess.
//!
//! The typed Python facade encodes specifications as JSON text; this layer
//! parses that text with the same engine parsers the CLI uses. Every parse and
//! long filesystem operation runs inside `Python::detach` + `call`, so the GIL
//! is released and argument faults are traced like other native operations.
use super::{Failure, NativeResult, call, exception, fail, json_value};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use serde_json::Value as Json;
use std::ffi::OsString;
use std::path::PathBuf;

fn parse_spec(text: &str) -> NativeResult<Json> {
    serde_json::from_str(text).map_err(|e| fail("edit_spec", e.to_string()))
}
/// CLI edit/rewrite option policy maps a bad durability to `usage`.
fn usage_durability(value: &str) -> NativeResult<oxblend::edit::Durability> {
    value
        .parse()
        .map_err(|e: oxblend::Error| fail("usage", e.message))
}
/// CLI extract parses durability directly, retaining the engine `durability` kind.
fn durability(value: &str) -> NativeResult<oxblend::edit::Durability> {
    value.parse().map_err(Failure::from)
}
fn edit_mode(value: &str) -> Option<oxblend::typed_edit::EditMode> {
    match value {
        "format-only" => Some(oxblend::typed_edit::EditMode::FormatOnly),
        "comp" => Some(oxblend::typed_edit::EditMode::Comp),
        _ => None,
    }
}

#[pyfunction]
fn diff(py: Python<'_>, a: PathBuf, b: PathBuf) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("diff", move || {
                match oxblend::diff::compare(&a, &b) {
                    Ok(report) => Ok(report),
                    Err(error) => {
                        // Shared CLI projection retained on the typed failure
                        // so Python can admit an incomplete diff.
                        let report = oxblend::diff::failure_report(&a, &b, &error);
                        let mut failure = Failure::from(error);
                        failure.report = Some(report);
                        Err(failure)
                    }
                }
            })
        })
        .map_err(|e| exception(py, "diff", e))?;
    json_value(py, report)
}

#[pyfunction]
#[pyo3(signature = (input, output, spec, mode, durability=String::from("none")))]
fn edit(
    py: Python<'_>,
    input: PathBuf,
    output: PathBuf,
    spec: String,
    mode: String,
    durability: String,
) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("edit", move || -> NativeResult<Json> {
                // CLI validates the explicit mode before reading the spec.
                if mode != "node-tree" && edit_mode(&mode).is_none() {
                    return Err(fail(
                        "usage",
                        "edit mode requires format-only|comp|node-tree",
                    ));
                }
                let value = parse_spec(&spec)?;
                if mode == "node-tree" {
                    let durability = usage_durability(&durability)?;
                    return oxblend::node_tree_edit::edit(
                        &input,
                        &output,
                        &value,
                        &oxblend::edit::EditOptions::new(durability),
                    )
                    .map_err(Into::into);
                }
                let selected = edit_mode(&mode).expect("mode validated above");
                let snapshot = oxblend::typed_edit::snapshot(&input)?;
                let plan = oxblend::typed_spec::plan(&snapshot, &value, selected)?;
                let durability = usage_durability(&durability)?;
                let report = oxblend::typed_edit::write_plan(
                    &snapshot,
                    &plan,
                    &output,
                    &oxblend::edit::EditOptions::new(durability),
                )?;
                Ok(report.json())
            })
        })
        .map_err(|e| exception(py, "edit", e))?;
    json_value(py, report)
}

#[pyfunction]
fn node_tree(py: Python<'_>, path: PathBuf) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("node_tree", move || {
                oxblend::node_tree_edit::inspect(&path).map_err(Into::into)
            })
        })
        .map_err(|e| exception(py, "node_tree", e))?;
    json_value(py, report)
}

#[pyfunction]
#[pyo3(signature = (input, output, ids=Vec::new(), blocks=Vec::new(), clear=Vec::new(), durability=String::from("none")))]
fn extract(
    py: Python<'_>,
    input: PathBuf,
    output: PathBuf,
    ids: Vec<String>,
    blocks: Vec<usize>,
    clear: Vec<String>,
    durability: String,
) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("extract", move || {
                let options = oxblend::extract::ExtractOptions {
                    ids,
                    blocks,
                    clear: clear.into_iter().collect(),
                    edit: oxblend::edit::EditOptions::new(self::durability(&durability)?),
                };
                oxblend::extract::extract(&input, &output, &options).map_err(Into::into)
            })
        })
        .map_err(|e| exception(py, "extract", e))?;
    json_value(py, report)
}

#[pyfunction]
#[pyo3(signature = (input, output, edits, durability=String::from("none")))]
fn rewrite(
    py: Python<'_>,
    input: PathBuf,
    output: PathBuf,
    edits: String,
    durability: String,
) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("rewrite", move || {
                let value = parse_spec(&edits)?;
                let edits = oxblend::rewrite_spec::parse(&value)?;
                let durability = usage_durability(&durability)?;
                let report = oxblend::edit::rewrite_paths(
                    &input,
                    &output,
                    &edits,
                    &oxblend::edit::EditOptions::new(durability),
                )?;
                Ok(report.json())
            })
        })
        .map_err(|e| exception(py, "rewrite", e))?;
    json_value(py, report)
}

#[pyfunction]
#[pyo3(signature = (path, recursive=true))]
fn overrides(py: Python<'_>, path: PathBuf, recursive: bool) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("overrides", move || {
                let mut options = oxblend::overrides::OverrideOptions::default();
                options.scope = if recursive {
                    oxblend::overrides::OverrideScope::Recursive
                } else {
                    oxblend::overrides::OverrideScope::RootFile
                };
                Ok(oxblend::overrides::trace_overrides(&path, options)?.json())
            })
        })
        .map_err(|e| exception(py, "overrides", e))?;
    json_value(py, report)
}

#[pyfunction]
#[pyo3(signature = (path, nul=false))]
fn read_path_list(py: Python<'_>, path: PathBuf, nul: bool) -> PyResult<Py<PyTuple>> {
    let paths = py
        .detach(move || {
            call("read_path_list", move || {
                Ok(oxblend::path_list::read(&path, nul)?)
            })
        })
        .map_err(|e| exception(py, "read_path_list", e))?;
    Ok(PyTuple::new(py, paths)?.unbind())
}

fn push_option(args: &mut Vec<OsString>, flag: &str, value: Option<PathBuf>) {
    if let Some(value) = value {
        args.push(flag.into());
        args.push(value.into_os_string());
    }
}

#[pyfunction]
#[pyo3(signature = (output, report=None, stdout=None, trace=None, trace_dir=None, run_id=None, last=false, include_names=false, include_file=None))]
#[allow(clippy::too_many_arguments)]
fn report_preview(
    py: Python<'_>,
    output: PathBuf,
    report: Option<PathBuf>,
    stdout: Option<PathBuf>,
    trace: Option<PathBuf>,
    trace_dir: Option<PathBuf>,
    run_id: Option<String>,
    last: bool,
    include_names: bool,
    include_file: Option<PathBuf>,
) -> PyResult<Py<PyAny>> {
    let mut args = Vec::new();
    push_option(&mut args, "--report", report);
    push_option(&mut args, "--stdout", stdout);
    push_option(&mut args, "--trace", trace);
    push_option(&mut args, "--trace-dir", trace_dir);
    if let Some(run_id) = run_id {
        args.push("--run-id".into());
        args.push(run_id.into());
    }
    if last {
        args.push("--last".into());
    }
    if include_names {
        args.push("--include-names".into());
    }
    push_option(&mut args, "--include-file", include_file);
    args.push("--output".into());
    args.push(output.into_os_string());
    args.push("--preview".into());
    let report = py
        .detach(move || call("report_preview", move || Ok(oxblend::report::run(&args)?)))
        .map_err(|e| exception(py, "report_preview", e))?;
    json_value(py, report)
}

#[pyfunction]
fn report_create(
    py: Python<'_>,
    preview: PathBuf,
    preview_sha256: String,
    output: PathBuf,
) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("report_create", move || {
                let args = vec![
                    "--create".into(),
                    preview.into_os_string(),
                    "--preview-sha256".into(),
                    preview_sha256.into(),
                    "--output".into(),
                    output.into_os_string(),
                ];
                Ok(oxblend::report::run(&args)?)
            })
        })
        .map_err(|e| exception(py, "report_create", e))?;
    json_value(py, report)
}

#[pyfunction]
fn report_discard(py: Python<'_>, preview: PathBuf, preview_sha256: String) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            call("report_discard", move || {
                let args = vec![
                    "--discard-preview".into(),
                    preview.into_os_string(),
                    "--preview-sha256".into(),
                    preview_sha256.into(),
                ];
                Ok(oxblend::report::run(&args)?)
            })
        })
        .map_err(|e| exception(py, "report_discard", e))?;
    json_value(py, report)
}

#[pyfunction]
#[pyo3(signature = (paths, destination, project_root=None, exclude=Vec::new(), dry_run=false, relative_only=false, compress=false, keep_hierarchy=true, permissive=false, durability="none", memory="fast"))]
#[allow(clippy::too_many_arguments)]
fn pack_sequence(
    py: Python<'_>,
    paths: Vec<PathBuf>,
    destination: PathBuf,
    project_root: Option<PathBuf>,
    exclude: Vec<String>,
    dry_run: bool,
    relative_only: bool,
    compress: bool,
    keep_hierarchy: bool,
    permissive: bool,
    durability: &str,
    memory: &str,
) -> PyResult<Py<PyAny>> {
    let (report, notices) = py
        .detach(|| {
            call("pack_sequence", || {
                let roots = paths
                    .iter()
                    .map(|path| {
                        std::path::absolute(path)
                            .map_err(|e| oxblend::Error::new("io", e.to_string()))
                    })
                    .collect::<oxblend::Result<Vec<_>>>()?;
                // CLI argument-gate order: the single-root selection refusal
                // fires before project resolution on both surfaces.
                if roots.len() == 1 && !roots[0].is_file() {
                    return Err(Failure::from(oxblend::pack::missing_pack_input_error()));
                }
                let mut options = oxblend::pack::PackOptions::default();
                options.exclude = exclude;
                options.noop = dry_run;
                options.relative_only = relative_only;
                options.compress = compress;
                options.keep_hierarchy = keep_hierarchy;
                options.permissive = permissive;
                options.durability = durability.parse().map_err(Failure::from)?;
                let options = super::configure_pack(
                    options,
                    &roots,
                    project_root,
                    Some(memory.to_owned()),
                    oxblend::session::MemoryMode::Fast,
                )?;
                let op = oxblend::pack::pack_operation(&roots, &destination, &options);
                let notices = super::diagnostic_facts(&op.diagnostics);
                // CLI-shared post-report gate: an unreadable selected root
                // with a retained per-root error is the canonical pack_input
                // refusal (with the report retained), never incomplete_pack.
                let report = super::operation(op)?;
                if let Some(error) = oxblend::pack::missing_root_error(&report) {
                    let mut failure = Failure::from(error);
                    failure.report = Some(report.json());
                    return Err(failure);
                }
                let result = super::checked_pack_report(report.json(), notices)?;
                super::pack_summary(&result.0);
                Ok(result)
            })
        })
        .map_err(|e| exception(py, "pack_sequence", e))?;
    super::report_output(py, report, notices)
}

#[pyfunction]
#[pyo3(signature = (path, destination, project_root=None, exclude=Vec::new(), dry_run=false, relative_only=false, compress=false, keep_hierarchy=false, permissive=false, durability="none", memory="fast"))]
#[allow(clippy::too_many_arguments)]
fn pack(
    py: Python<'_>,
    path: PathBuf,
    destination: PathBuf,
    project_root: Option<PathBuf>,
    exclude: Vec<String>,
    dry_run: bool,
    relative_only: bool,
    compress: bool,
    keep_hierarchy: bool,
    permissive: bool,
    durability: &str,
    memory: &str,
) -> PyResult<Py<PyAny>> {
    // Single-root CLI pack owned by the engine: the Session open happens
    // INSIDE this pack call, so an invalid selected blend refuses with the
    // exact CLI pack path (`pack 'dest': snapshot/index 'src': ...`)
    // instead of a bare Session-open base. Argument-gate order (single
    // non-file refusal, then shared project resolution) mirrors
    // `pack pack_arguments` exactly; the facade never synthesizes prose.
    let (report, notices) = py
        .detach(|| {
            call("pack", || {
                let root = std::path::absolute(&path)
                    .map_err(|e| oxblend::Error::new("io", e.to_string()))?;
                // CLI argument-gate order: the single-root selection refusal
                // fires before project resolution on both surfaces.
                if !root.is_file() {
                    return Err(Failure::from(oxblend::pack::missing_pack_input_error()));
                }
                let roots = vec![root];
                let mut options = oxblend::pack::PackOptions::default();
                options.exclude = exclude;
                options.noop = dry_run;
                options.relative_only = relative_only;
                options.compress = compress;
                options.keep_hierarchy = keep_hierarchy;
                options.permissive = permissive;
                options.durability = durability.parse().map_err(Failure::from)?;
                let options = super::configure_pack(
                    options,
                    &roots,
                    project_root,
                    Some(memory.to_owned()),
                    oxblend::session::MemoryMode::Fast,
                )?;
                let op = oxblend::pack::pack_operation(&roots, &destination, &options);
                let notices = super::diagnostic_facts(&op.diagnostics);
                // CLI-shared post-report gate: an unreadable selected root
                // with a retained per-root error is the canonical pack_input
                // refusal (with the report retained), never incomplete_pack.
                let report = super::operation(op)?;
                if let Some(error) = oxblend::pack::missing_root_error(&report) {
                    let mut failure = Failure::from(error);
                    failure.report = Some(report.json());
                    return Err(failure);
                }
                let result = super::checked_pack_report(report.json(), notices)?;
                super::pack_summary(&result.0);
                Ok(result)
            })
        })
        .map_err(|e| exception(py, "pack", e))?;
    super::report_output(py, report, notices)
}
/// Canonical single-root selection refusal for the facade pack pre-check:
/// the prose lives once in the engine, never synthesized in Python.
#[pyfunction]
fn pack_input_error(py: Python<'_>) -> PyResult<Py<PyAny>> {
    Err(exception(
        py,
        "pack_input_error",
        Failure::from(oxblend::pack::missing_pack_input_error()),
    ))
}
/// One coherent strict/refusal seam for every `_admit` operation. The
/// engine selects the identical canonical kind/message the CLI delivery
/// check emits, over the serialized incomplete report; no caller synthesizes
/// prose or falls back to a formatted string. Unknown operations fail loudly
/// as usage.
#[pyfunction]
#[pyo3(signature = (operation, report_json, command=None, input_display=None))]
fn strict_refusal(
    py: Python<'_>,
    operation: String,
    report_json: String,
    command: Option<String>,
    input_display: Option<String>,
) -> PyResult<(String, String)> {
    let report: Json = serde_json::from_str(&report_json)
        .map_err(|e| exception(py, "strict_refusal", fail("usage", e.to_string())))?;
    let error = oxblend::strict::strict_refusal(
        &operation,
        command.as_deref(),
        &report,
        input_display.as_deref(),
    )
    .map_err(|e| exception(py, "strict_refusal", Failure::from(e)))?;
    Ok((error.kind.to_owned(), error.message))
}
pub(super) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(pack, m)?)?;
    m.add_function(wrap_pyfunction!(pack_sequence, m)?)?;
    m.add_function(wrap_pyfunction!(pack_input_error, m)?)?;
    m.add_function(wrap_pyfunction!(strict_refusal, m)?)?;
    m.add_function(wrap_pyfunction!(diff, m)?)?;
    m.add_function(wrap_pyfunction!(edit, m)?)?;
    m.add_function(wrap_pyfunction!(node_tree, m)?)?;
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    m.add_function(wrap_pyfunction!(rewrite, m)?)?;
    m.add_function(wrap_pyfunction!(overrides, m)?)?;
    m.add_function(wrap_pyfunction!(read_path_list, m)?)?;
    m.add_function(wrap_pyfunction!(report_preview, m)?)?;
    m.add_function(wrap_pyfunction!(report_create, m)?)?;
    m.add_function(wrap_pyfunction!(report_discard, m)?)?;
    Ok(())
}
