use crate::trace::{self, PointerMode, TraceMode, TraceOptions};
use crate::value::PathStep;
use crate::{BlendFile, Error, Result};
use serde_json::Value as Json;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
/// Internal CLI vocabulary shared with receipt validation and safe exports.
pub const COMMANDS: &[&str] = &[
    "summary",
    "query",
    "catalog",
    "explain",
    "inspect",
    "get",
    "pointer",
    "payload",
    "read-batch",
    "source-facts",
    "library-facts",
    "trace",
    "list",
    "blocks",
    "overrides",
    "diff",
    "edit",
    "node-tree",
    "extract",
    "rewrite",
    "pack",
    "pack-sequence",
    "exr-header",
    "exr-scan",
    "exr-pixels",
    "pointer-census",
    "skill",
    "help",
    "report",
];
pub(crate) fn builtin_command(name: &str) -> Option<&'static str> {
    COMMANDS
        .iter()
        .copied()
        .find(|value| *value == name)
        .or(match name {
            "invalid" => Some("invalid"),
            "version" => Some("version"),
            _ => None,
        })
}
fn deliver_text(arguments: std::fmt::Arguments<'_>) -> Result<()> {
    let mut span = crate::diagnostics::Span::new(
        "serialize",
        serde_json::json!({"role":"stdout","scope":"text_fragment"}),
    );
    let mut bytes = 0;
    let result = {
        let mut out = crate::failure_receipt::stdout_counted(&mut bytes);
        out.write_fmt(arguments)
            .and_then(|_| out.flush())
            .map_err(crate::edit_output::io)
    };
    crate::diagnostics::stdout_delivery(bytes, result.is_ok());
    if span.active() {
        span.data(serde_json::json!({"bytes":bytes,"write_complete":result.is_ok(),"flush":"fragment_boundary","counter_source":"actual OS accepted writes"}));
    }
    span.result(result)
}
macro_rules! eprintln {
    ($($arg:tt)*) => { writeln!(std::io::stderr().lock(), $($arg)*).map_err(crate::edit_output::io)? };
}
fn usage(s: &str) -> Error {
    Error::new("usage", s)
}
/// Canonical missing-input refusal, shared by the CLI single-input gate,
/// the listing report seams and the blocks writer so every surface stays
/// byte-identical without duplicating the prose.
pub fn missing_input_error() -> Error {
    Error::new(
        "missing_input",
        "selected blend input missing or not a regular file",
    )
}
fn arg(s: &OsStr) -> Result<&str> {
    s.to_str().ok_or_else(|| usage("option must be UTF8"))
}
fn absolute(p: &Path) -> Result<PathBuf> {
    Ok(trace::lexical(
        &std::path::absolute(p).map_err(crate::edit_output::io)?,
    ))
}
pub fn pack_arguments(
    command: &str,
    args: &[OsString],
) -> Result<(Vec<PathBuf>, PathBuf, crate::pack::PackOptions)> {
    let sequence = command == "pack-sequence";
    let mut options = crate::pack::PackOptions {
        keep_hierarchy: sequence,
        ..Default::default()
    };
    let mut seen_mode = false;
    let (mut target, mut project) = (None, None);
    let mut positionals = Vec::new();
    let nul_delimited = crate::path_list::nul_delimited(args);
    let mut i = 0;
    while i < args.len() {
        if sequence && let Some(path) = crate::path_list::inline_path(&args[i]) {
            positionals.extend(crate::path_list::read(Path::new(path), nul_delimited)?);
            i += 1;
            continue;
        }
        let s = args[i].to_str();
        match s {
            Some("--") => {
                positionals.extend(args[i + 1..].iter().map(PathBuf::from));
                break;
            }
            Some("-0") if sequence => {}
            Some("--files-from") if sequence => {
                i += 1;
                let path = args
                    .get(i)
                    .ok_or_else(|| usage("--files-from needs a path"))?;
                positionals.extend(crate::path_list::read(Path::new(path), nul_delimited)?);
            }
            Some("-p" | "--project" | "-t" | "--target" | "--durability" | "--memory") => {
                let key = s.unwrap();
                i += 1;
                let v = args
                    .get(i)
                    .ok_or_else(|| usage("pack option needs a value"))?;
                match key {
                    "-p" | "--project" => project = Some(PathBuf::from(v)),
                    "-t" | "--target" => target = Some(PathBuf::from(v)),
                    "--durability" => options.durability = arg(v)?.parse()?,
                    // Unknown modes stay usage errors from MemoryMode::from_str.
                    _ => options.memory = arg(v)?.parse()?,
                }
            }
            Some("-e" | "--exclude") => {
                let before = options.exclude.len();
                while let Some(v) = args.get(i + 1) {
                    if v.to_str().is_some_and(|s| s.starts_with('-')) {
                        break;
                    }
                    options
                        .exclude
                        .extend(arg(v)?.split_whitespace().map(str::to_owned));
                    i += 1;
                }
                if options.exclude.len() == before {
                    return Err(usage("--exclude needs glob patterns"));
                }
            }
            Some(v) if v.starts_with("--exclude=") => {
                options
                    .exclude
                    .extend(v[10..].split_whitespace().map(str::to_owned));
                if v.len() == 10 {
                    return Err(usage("--exclude needs glob patterns"));
                }
            }
            Some(v) if v.starts_with("--project=") => project = Some(PathBuf::from(&v[10..])),
            Some(v) if v.starts_with("--target=") => target = Some(PathBuf::from(&v[9..])),
            Some(v) if v.starts_with("--durability=") => options.durability = v[13..].parse()?,
            Some(v) if v.starts_with("--memory=") => options.memory = v[9..].parse()?,
            Some(v)
                if v.starts_with('-')
                    && !v.starts_with("--")
                    && v.len() > 2
                    && v[1..].bytes().all(|c| b"nrc".contains(&c)) =>
            {
                options.noop |= v.contains('n');
                options.relative_only |= v.contains('r');
                options.compress |= v.contains('c');
            }
            Some("-n" | "--noop" | "--dry-run") => options.noop = true,
            Some("-r" | "--relative-only") => options.relative_only = true,
            Some("-c" | "--compress") => options.compress = true,
            Some("--keep-hierarchy") => options.keep_hierarchy = true,
            Some("--strict" | "--permissive" | "--non-strict") if !seen_mode => {
                options.permissive = s != Some("--strict");
                seen_mode = true;
            }
            Some(v) if v.starts_with('-') => return Err(usage("unknown pack option")),
            _ => positionals.push(PathBuf::from(&args[i])),
        }
        i += 1;
    }
    if !sequence {
        if positionals.len() != 2 || target.is_some() {
            return Err(usage("pack INPUT TARGET [-p PROJECT] [-e GLOBS...] [-nrc]"));
        }
        target = positionals.pop();
    }
    let target = target.ok_or_else(|| usage("pack-sequence requires -t TARGET"))?;
    if let Some(error) = crate::pack::uri_target_error(command, &target) {
        return Err(error);
    }
    if positionals.is_empty() {
        return Err(usage("pack requires a selected blend input"));
    }
    let inputs = positionals
        .iter()
        .map(|p| absolute(p))
        .collect::<Result<Vec<_>>>()?;
    for p in &inputs {
        if inputs.len() == 1 && !p.is_file() {
            return Err(crate::pack::missing_pack_input_error());
        }
    }
    options.project_root = crate::pack::resolve_project_root(project, &inputs, sequence)?;
    Ok((inputs, target, options))
}
fn shorten(p: &Path, cwd: &Path) -> String {
    p.strip_prefix(cwd).unwrap_or(p).display().to_string()
}
/// Trace entry for the binary: owns the explicit CLI observer that renders
/// the historical seek-fallback envelope synchronously at emission.
pub fn trace_cli(
    root: &Path,
    options: crate::trace::TraceOptions,
) -> Result<crate::trace::TraceReport> {
    crate::trace::trace_report_cli(root, options)
}
/// Pack entry for the binary: owns the explicit CLI observer, same contract.
pub fn pack_cli(
    inputs: &[PathBuf],
    new_directory: &Path,
    options: &crate::pack::PackOptions,
) -> Result<crate::pack::PackReport> {
    if options.memory == crate::session::MemoryMode::Bounded {
        crate::failure_receipt::bounded_pack();
    }
    crate::pack::pack_report_cli(inputs, new_directory, options)
}
pub fn run(command: &str, args: &[OsString]) -> Result<()> {
    let mut input = None;
    let mut options = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let v = &args[i];
        if v == "--" {
            if input.is_some() || args.len() != i + 2 {
                return Err(usage("one selected input required"));
            }
            input = Some(&args[i + 1]);
            break;
        }
        if v.to_str().is_some_and(|s| s.starts_with('-')) {
            if command == "blocks" && v == "--address" {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| usage("--address needs a value"))?;
                options.push(OsString::from(format!("--address={}", arg(value)?)));
            } else if command == "blocks" && (v == "-l" || v == "--limit") {
                options.push(v.clone());
                i += 1;
                options.push(
                    args.get(i)
                        .ok_or_else(|| usage("--limit needs a value"))?
                        .clone(),
                );
            } else if command == "blocks" && v.to_str().is_some_and(|s| s.starts_with("--limit=")) {
                options.push(OsString::from("--limit"));
                options.push(OsString::from(&arg(v)?[8..]));
            } else {
                options.push(v.clone());
            }
        } else if input.replace(v).is_some() {
            return Err(usage("one selected input required"));
        }
        i += 1;
    }
    let input = input.ok_or_else(|| usage("list|blocks INPUT [options]"))?;
    if !Path::new(input).is_file() {
        return Err(missing_input_error());
    }
    let result = match command {
        "list" => listing(Path::new(input), &options),
        "blocks" => blocks(Path::new(input), &options),
        _ => Err(usage("unknown command")),
    };
    let mut flush_span = crate::diagnostics::Span::new(
        "serialize",
        serde_json::json!({"role":"stdout","scope":"command_flush"}),
    );
    let flush = std::io::stdout()
        .lock()
        .flush()
        .map_err(crate::edit_output::io);
    crate::diagnostics::stdout_delivery(0, flush.is_ok());
    if flush_span.active() {
        flush_span.data(serde_json::json!({"bytes":0,"flush_complete":flush.is_ok()}));
    }
    flush_span.result(flush)?;
    result
}

fn directory_listing(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(directory)
        .map_err(crate::edit_output::io)?
        .map(|e| e.map(|e| e.path()).map_err(crate::edit_output::io))
        .collect::<Result<Vec<_>>>()?;
    for p in &entries {
        if p.is_file() {
            files.push(p.clone());
        }
    }
    for p in entries {
        if p.is_dir() && !p.is_symlink() {
            directory_listing(&p, files)?;
        }
    }
    Ok(())
}
pub(crate) fn capture_listing(
    input: &Path,
    args: &[OsString],
) -> Result<(serde_json::Value, Option<Error>)> {
    if !input.is_file() {
        return Ok((serde_json::Value::Null, Some(missing_input_error())));
    }
    let mut output = Vec::new();
    let error = listing_output(input, args, &mut output).err();
    let value = if output.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&output).map_err(|e| Error::new("serialization", e.to_string()))?
    };
    // An incomplete report is still the legacy listing value, with its returned
    // typed error retained beside it. Never turn an empty failed listing into {}.
    Ok((value, error))
}
fn listing(input: &Path, args: &[OsString]) -> Result<()> {
    let mut span = crate::diagnostics::Span::new(
        "serialize",
        serde_json::json!({"role":"stdout","scope":"listing"}),
    );
    let mut bytes = 0;
    let (result, flush) = {
        let mut out = crate::failure_receipt::stdout_counted(&mut bytes);
        let result = listing_output(input, args, &mut out);
        let flush = out.flush().map_err(crate::edit_output::io);
        (result, flush)
    };
    crate::diagnostics::stdout_delivery(bytes, flush.is_ok());
    if span.active() {
        span.data(serde_json::json!({"bytes":bytes,"flush_complete":flush.is_ok(),"counter_source":"actual OS accepted writes"}));
    }
    span.result(flush)?;
    result
}
/// CLI text/JSON delivery for `list`: the shared listing core writing to the
/// caller's stdout-counted writer with the historical live stderr warnings.
/// No collector; serialization is exact.
pub fn listing_output(input: &Path, args: &[OsString], output: &mut dyn Write) -> Result<()> {
    listing_core(input, args, None, None, Some(output))
}

/// Native report-value seam for `list`: run the same listing core with no
/// stdout delivery and collect the exact JSON report value the CLI --json
/// projection would serialize, gathered before serialization. Warning
/// notices (trace warnings and seek-fallback envelopes) come back out-of-band
/// instead of the CLI's live stderr rendering. Returns that value beside the
/// notices and the typed error actually returned by the engine
/// (missing_asset, missing_library, pattern_members_unknown or
/// incomplete_trace); partial rows are retained, never discarded.
pub fn listing_report(input: &Path, args: &[OsString]) -> Result<(Json, Vec<Json>, Option<Error>)> {
    if !input.is_file() {
        return Ok((Json::Null, Vec::new(), Some(missing_input_error())));
    }
    let mut collected = None;
    let mut emit = |value: &Json| collected = Some(value.clone());
    let mut notices = Vec::new();
    let error = listing_core(input, args, Some(&mut emit), Some(&mut notices), None).err();
    Ok((collected.unwrap_or(Json::Null), notices, error))
}

/// Shared `list` flag parser: one parser policy for the CLI engine and the
/// retained captured core. No duplicate usage strings.
struct ListFlags {
    json: bool,
    sha: bool,
    timing: bool,
    full: bool,
    low_memory: bool,
}
fn parse_list_flags(args: &[OsString]) -> Result<ListFlags> {
    let (mut json, mut sha, mut timing, mut full, mut low_memory) =
        (false, false, false, false, false);
    let mut seen_mode = false;
    for a in args {
        match arg(a)? {
            "--strict" | "--permissive" | "--non-strict" if !seen_mode => seen_mode = true,
            "--json" => json = true,
            "--sha256" => sha = true,
            "--timing" => timing = true,
            "--full-info" => full = true,
            "--low-memory" if !low_memory => low_memory = true,
            _ => {
                return Err(usage(
                    "list INPUT [--json] [--sha256] [--timing] [--full-info] [--low-memory]",
                ));
            }
        }
    }
    Ok(ListFlags {
        json,
        sha,
        timing,
        full,
        low_memory,
    })
}
/// Exact historical seek-fallback envelope the CLI observer renders live.
/// Shared by the CLI stderr path and the retained captured notices so the
/// shape never drifts between them.
fn seek_fallback_notice(path: &Option<PathBuf>, reason: &str) -> Json {
    serde_json::json!({"warning":{"kind":"seek_table_fallback", "message":"untrusted seek table; using checked_full_streaming_spill", "reason":reason}, "path_hex":path.as_ref().map(|p|crate::value::hex(p.as_os_str().as_encoded_bytes()))})
}
/// Seek-fallback envelopes collected from the given diagnostics, in emission
/// order with no source dedup: every emission keeps its own multiplicity.
fn seek_fallback_notices_in(diagnostics: &[crate::session::Diagnostic]) -> Vec<Json> {
    diagnostics
        .iter()
        .filter_map(|diagnostic| {
            if let crate::session::Diagnostic::SeekTableFallback { path, reason } = diagnostic {
                Some(seek_fallback_notice(path, reason))
            } else {
                None
            }
        })
        .collect()
}
/// Seek-fallback envelopes collected from an operation after its drain, in
/// emission order. Same multiplicity and timing as the live observer.
fn seek_fallback_notices(op: &crate::session::OpCtx) -> Vec<Json> {
    seek_fallback_notices_in(&op.diagnostics)
}
/// Exact warning envelope shared by the CLI stderr rendering and the
/// retained captured out-of-band notices.
fn warning_notice(warning: &trace::TraceWarning) -> Json {
    serde_json::json!({"schema":1,"warning":warning.json()})
}
/// Shared listing computation: one group/member/missing policy for the CLI
/// engine and the retained captured core. Missing-kind precedence is exactly
/// the historical one: a non-empty report error set keeps `incomplete_trace`
/// first; `missing_asset`/`missing_library` and `pattern_members_unknown`
/// only fill an otherwise-healthy listing.
struct ListingGroups {
    groups: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    text_members: BTreeMap<PathBuf, Vec<PathBuf>>,
    directories: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    missing: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    missing_links: Vec<Json>,
    unknown: Vec<Json>,
    missing_error: Option<Error>,
}
fn collect_listing_groups(report: &trace::TraceReport) -> Result<ListingGroups> {
    let mut groups: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    let mut text_members: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    let mut directories: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    let mut missing: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    let missing_links: Vec<_> = report
        .errors
        .iter()
        .filter(|e| e.is_external_missing())
        .map(trace::TraceFailure::json)
        .collect();
    let unknown: Vec<_> = report
        .errors
        .iter()
        .filter(|e| !e.is_external_missing())
        .map(trace::TraceFailure::json)
        .collect();
    let mut missing_error =
        (!report.errors.is_empty()).then(crate::strict::listing_incomplete_trace);
    for u in &report.usages {
        let owner = PathBuf::from(OsString::from_vec(u.owner_file.clone()));
        let resource = trace::resolve_usage(u);
        for member in u.missing_members() {
            let path = trace::resolve_usage(&member);
            missing
                .entry(owner.clone())
                .or_default()
                .insert(path.clone());
            groups
                .entry(owner.clone())
                .or_default()
                .insert(path.clone());
            text_members.entry(owner.clone()).or_default().push(path);
            missing_error.get_or_insert_with(crate::strict::listing_missing_asset);
        }
        if let Some(reason) = &u.pattern_unknown {
            missing_error
                .get_or_insert_with(|| crate::strict::listing_pattern_unknown(reason.clone()));
        }
        let members = if resource.is_dir() {
            directories
                .entry(owner.clone())
                .or_default()
                .insert(resource.clone());
            let mut members = Vec::new();
            directory_listing(&resource, &mut members)?;
            members
        } else {
            match u.available_members() {
                Ok(members) => members,
                Err(mut error) if error.kind == "missing_asset" => {
                    if u.block_code == b"LI" {
                        error.kind = "missing_library";
                    }
                    error.message = format!(
                        "{}; referenced by block {} in '{}'; use trace for per-usage availability",
                        error.message,
                        u.block_address,
                        crate::value::display_bytes(&u.owner_file)
                    );
                    // Missing-member decision at the listing owner with
                    // reconciled usage identity; never a silent omission.
                    if crate::diagnostics::enabled() {
                        crate::diagnostics::point(
                            "decision",
                            serde_json::json!({"item":resource.to_string_lossy(),"kind":error.kind,"reason":error.message}),
                        );
                    }
                    missing_error.get_or_insert(error);
                    missing
                        .entry(owner.clone())
                        .or_default()
                        .insert(resource.clone());
                    vec![resource]
                }
                Err(error) => return Err(error),
            }
        };
        text_members
            .entry(owner.clone())
            .or_default()
            .extend(members.iter().cloned());
        if !members.is_empty() {
            groups.entry(owner).or_default().extend(members);
        }
    }
    Ok(ListingGroups {
        groups,
        text_members,
        directories,
        missing,
        missing_links,
        unknown,
        missing_error,
    })
}
/// Shared structured-link reconciliation: the report JSON owns the
/// errors/warnings identities beside the dependency map.
fn add_listing_links(
    value: &mut Json,
    report: &trace::TraceReport,
    missing_links: &[Json],
    unknown: &[Json],
) {
    if !unknown.is_empty() {
        value["errors"] = serde_json::json!(unknown);
        value["problem_counts"] = report.json()["problem_counts"].clone();
    }
    let pattern_unknowns = report.usages.iter().filter_map(|u| u.pattern_unknown.as_ref().map(|reason| serde_json::json!({"kind":"pattern_members_unknown","status":"unknown","reason":reason,"usage":u.json()}))).collect::<Vec<_>>();
    if !pattern_unknowns.is_empty() {
        if !value["errors"].is_array() {
            value["errors"] = serde_json::json!([]);
        }
        value["errors"]
            .as_array_mut()
            .unwrap()
            .extend(pattern_unknowns);
        value["problem_counts"] = report.json()["problem_counts"].clone();
    }
    if !missing_links.is_empty() {
        value["missing_links"] = serde_json::json!(missing_links);
        value["missing_link_counts"] = serde_json::json!({
            "missing_id":missing_links.iter().filter(|e|e["kind"]=="missing_id").count(),
            "missing_library":missing_links.iter().filter(|e|e["kind"]=="missing_library").count(),
        });
    }
}
/// Shared JSON projection for `list --json`: the dependency map, the
/// `--full-info` expansion, and the reconciled links. Both the CLI engine
/// and the retained captured core serialize this same value.
fn build_listing_json(
    data: &ListingGroups,
    report: &trace::TraceReport,
    full: bool,
    packed: &[Json],
) -> Json {
    let mut files = serde_json::Map::new();
    for (owner, paths) in &data.groups {
        files.insert(
            json_path(owner),
            serde_json::Value::Array(
                paths
                    .iter()
                    .map(|p| {
                        if data.missing_error.is_some() {
                            serde_json::json!({"path":json_path(p),"missing":data.missing.get(owner).is_some_and(|paths| paths.contains(p))})
                        } else {
                            serde_json::Value::String(json_path(p))
                        }
                    })
                    .collect(),
            ),
        );
    }
    if full {
        let mut value = serde_json::json!({"files":files,"usages":report.usages.iter().map(trace::Usage::json).collect::<Vec<_>>(),"packed":packed,"warnings":report.warnings.iter().map(trace::TraceWarning::json).collect::<Vec<_>>()});
        if data.missing_error.is_some() {
            value["complete"] = serde_json::json!(false);
        }
        add_listing_links(&mut value, report, &data.missing_links, &data.unknown);
        value
    } else {
        let mut value = if data.missing_error.is_some() {
            serde_json::json!({"complete":false,"files":files})
        } else {
            serde_json::Value::Object(files)
        };
        add_listing_links(&mut value, report, &data.missing_links, &data.unknown);
        value
    }
}
/// Retained-listing shared core: the same parser, `ExistingOnly` trace,
/// group policy, notice shapes and JSON projection as the CLI engine, but on
/// caller-owned store/op plumbing. The trace runs on the supplied store,
/// batch and op with the frozen `resolved_root` (never re-resolved); the
/// retained captured path reuses the supplied store reader for packed
/// resources with no new capture or reopen, and stays silent (no eprintln)
/// with warning/seek envelopes returned out-of-band. Trace-level failures
/// propagate as `BodyErr` with their Guard/Independent provenance intact,
/// never flattened into report rows; listing-level missing dependencies come
/// back as the partial JSON value beside the same typed domain error the
/// native report seam returns. Final Session completion guarding stays with
/// the parent owner.
#[allow(clippy::too_many_arguments)]
pub(crate) fn listing_in_store(
    input_spelling: &Path,
    resolved_root: &Path,
    store: &mut crate::session::InputStore,
    batch: &mut crate::session::CaptureBatch,
    op: &mut crate::session::OpCtx,
    args: &[OsString],
    captured: bool,
) -> crate::session::BodyResult<(Json, Vec<Json>, Option<Error>)> {
    let flags = parse_list_flags(args)?;
    if !flags.json {
        return Err(usage("native listing report requires --json").into());
    }
    // The supplied op may already carry earlier roots' diagnostics (one op is
    // threaded through roots): drain pending channel items, then snapshot the
    // boundary so only this call's newly emitted envelopes are returned.
    op.drain();
    let prior = op.diagnostics.len();
    let (report, _file_paths) = trace::trace_in_store(
        store,
        batch,
        resolved_root,
        TraceOptions {
            mode: TraceMode::Recursive,
            pointers: PointerMode::NonStrict,
            low_memory: flags.low_memory,
        },
        op,
        trace::LibraryPolicy::ExistingOnly,
        captured,
    )?;
    // Reconcile report-carried causes at the listing owner, as the CLI does.
    if crate::diagnostics::enabled() {
        crate::diagnostics::report(&report.json());
    }
    // --sha256/--timing stay JSON-reporting without the CLI's stderr hint,
    // keeping retained notices exactly warnings plus seek envelopes.
    let mut notices: Vec<Json> = report.warnings.iter().map(warning_notice).collect();
    op.drain();
    notices.extend(seek_fallback_notices_in(&op.diagnostics[prior..]));
    let data = collect_listing_groups(&report)?;
    // Healthy member-only listings need no duplicate full trace JSON merely
    // to prepare a failure receipt. Capture typed causes before output only
    // when this listing is already known to be incomplete.
    let report_causes = if data.missing_error.is_some() {
        crate::failure_receipt::record_report(&report.json())
    } else {
        Vec::new()
    };
    let packed = if flags.full {
        let id = store.id_for_path(resolved_root)?;
        let file = store.reader(id)?;
        packed_resources_from_reader(file, input_spelling, resolved_root, &report)?
    } else {
        Vec::new()
    };
    let value = build_listing_json(&data, &report, flags.full, &packed);
    if let Some(error) = &data.missing_error
        && error.kind.starts_with("incomplete_")
    {
        crate::failure_receipt::record_terminal(error, &report_causes);
    }
    Ok((value, notices, data.missing_error))
}
fn listing_core(
    input: &Path,
    args: &[OsString],
    collector: Option<&mut dyn FnMut(&Json)>,
    mut notices: Option<&mut Vec<Json>>,
    mut output: Option<&mut dyn Write>,
) -> Result<()> {
    macro_rules! print {
        ($($arg:tt)*) => {
            if let Some(out) = output.as_deref_mut() {
                write!(out, $($arg)*).map_err(crate::edit_output::io)?;
            }
        };
    }
    macro_rules! println {
        () => { print!("\n") };
        ($($arg:tt)*) => { print!("{}\n", format_args!($($arg)*)) };
    }
    let started = std::time::Instant::now();
    let ListFlags {
        json,
        sha,
        timing,
        full,
        low_memory,
    } = parse_list_flags(args)?;
    // One operation owns this listing's diagnostics end to end: the observer
    // renders the historical envelope synchronously, and the second
    // packed-resource open below runs on the same emitter, cwd, temp root
    // and channel — same multiplicity/timing, no per-open resampling.
    // Native collection (no writer) stays silent instead: notices are
    // returned out-of-band rather than rendered to the process stderr.
    let native = output.is_none();
    crate::tracing::initialize();
    let cwd = std::env::current_dir().map_err(crate::edit_output::io)?;
    let temp_root = std::env::temp_dir();
    let mut op = crate::session::OpCtx::new(
        crate::session::GuardPolicy::Legacy,
        if native {
            None
        } else {
            Some(crate::session::cli_observer)
        },
        cwd,
        temp_root,
    );
    let mut store = crate::session::InputStore::default();
    let mut batch = crate::session::CaptureBatch::default();
    let resolved = crate::session::InputStore::resolve(&op, input);
    // Notices were already rendered by the observer; the collected vector
    // is intentionally never printed a second time.
    let (report, file_paths) = trace::trace_in_store(
        &mut store,
        &mut batch,
        &resolved,
        TraceOptions {
            mode: TraceMode::Recursive,
            pointers: PointerMode::NonStrict,
            low_memory,
        },
        &mut op,
        trace::LibraryPolicy::ExistingOnly,
        false,
    )?;
    // Reconcile report-carried causes at the listing owner: the structured
    // report JSON owns errors/warnings identities; text mode has no JSON
    // stdout for the parent ledger, so emit here while the command is active.
    // Native collection keeps the same envelope objects out-of-band.
    if crate::diagnostics::enabled() {
        crate::diagnostics::report(&report.json());
    }
    let mut seek_notices = Vec::new();
    if native {
        op.drain();
        // The exact envelopes the CLI observer renders live.
        seek_notices = seek_fallback_notices(&op);
    }
    for warning in &report.warnings {
        let envelope = warning_notice(warning);
        match notices.as_deref_mut() {
            Some(sink) => sink.push(envelope),
            None => eprintln!("{envelope}"),
        }
    }
    if let Some(sink) = notices {
        sink.extend(seek_notices);
    }
    let cwd = op.cwd.clone();
    // Groups, members, directories, missing sets and the missing-kind error
    // come from the one shared listing policy; the CLI only delivers them.
    let data = collect_listing_groups(&report)?;
    // Healthy member-only listings need no duplicate full trace JSON merely
    // to prepare a failure receipt. Capture typed causes before output only
    // when this listing is already known to be incomplete.
    let report_causes = if data.missing_error.is_some() {
        crate::failure_receipt::record_report(&report.json())
    } else {
        Vec::new()
    };
    if json {
        if sha || timing {
            eprintln!("--sha256/--timing cannot be used with --json; reporting JSON");
        }
        // The independent second open stays at this exact late full-info
        // point on the listing's own emitter, frozen cwd and temp root.
        if full {
            let packed = packed_resources(input, low_memory, &report, &mut op)?;
            let value = build_listing_json(&data, &report, true, &packed);
            if let Some(emit) = collector {
                emit(&value);
            }
            println!("{value}");
        } else {
            let value = build_listing_json(&data, &report, false, &[]);
            if let Some(emit) = collector {
                emit(&value);
            }
            let pretty = serde_json::to_string_pretty(&value)
                .map_err(|e| Error::new("json", e.to_string()))?;
            // serde_json owns escaping/layout. Double only its structural
            // leading indentation; JSON string whitespace is already escaped.
            let encoded = pretty
                .lines()
                .map(|line| {
                    let indentation = line.bytes().take_while(|&byte| byte == b' ').count();
                    format!("{}{line}", " ".repeat(indentation))
                })
                .collect::<Vec<_>>()
                .join("\n");
            // All four ordinary JSON flag combinations retain UTF-8, use
            // four-space indentation and omit the terminal newline.
            print!("{encoded}");
        }
    } else {
        if output.is_none() {
            return Err(usage("native listing report requires --json"));
        }
        // Compatibility text follows the root-first selected traversal, not
        // the lexical order of the JSON dependency-map keys.
        // Text has a heading for each owner with an external usage, even
        // when every declared member is missing. JSON has only nonempty groups.
        let mut hash_duration = std::time::Duration::ZERO;
        let mut timed_hash = |path: &Path| -> Result<String> {
            let hash_started = std::time::Instant::now();
            let digest = hash_file(path)?.0;
            hash_duration += hash_started.elapsed();
            Ok(digest)
        };
        for owner in file_paths
            .iter()
            .filter(|p| data.text_members.contains_key(*p))
        {
            print!("{}", shorten(owner, &cwd));
            if sha {
                print!(" {}", timed_hash(owner)?);
            }
            println!();
            let mut displayed = BTreeSet::new();
            let mut ordered = Vec::new();
            for p in data.text_members.get(owner).into_iter().flatten() {
                // Directory expansion can supply existing members, never an
                // absent declaration beneath it. Keep that declaration visible.
                if data
                    .missing
                    .get(owner)
                    .is_some_and(|paths| paths.contains(p))
                    || !data
                        .directories
                        .get(owner)
                        .is_some_and(|dirs| dirs.iter().any(|d| p.starts_with(d)))
                {
                    ordered.push(p.clone());
                }
            }
            if let Some(dirs) = data.directories.get(owner) {
                for d in dirs {
                    directory_listing(d, &mut ordered)?;
                }
            }
            for p in ordered {
                if !displayed.insert(p.clone()) {
                    continue;
                }
                print!("    {}", shorten(&p, &cwd));
                if data
                    .missing
                    .get(owner)
                    .is_some_and(|paths| paths.contains(&p))
                {
                    print!(" [missing]");
                }
                if sha && p.is_file() {
                    print!(" {}", timed_hash(&p)?);
                }
                println!();
            }
        }
        for e in &report.errors {
            if !e.is_external_missing() {
                println!("    [unknown] {}", e.json());
            }
            if e.missing_id.is_some() {
                // The existing record carries every identity/chain field and
                // escapes control bytes onto one line; hex never loses a name.
                println!("    [missing linked ID] {}", e.json());
            }
        }
        for u in &report.usages {
            if let Some(reason) = &u.pattern_unknown {
                println!(
                    "    [unknown pattern_members_unknown] {} at {:#x}: {} ({reason})",
                    crate::value::display_bytes(&u.owner_file),
                    u.block_address,
                    crate::value::display_bytes(&u.asset_path)
                );
            }
        }
        if full {
            for u in &report.usages {
                println!(
                    "    usage {} at {:#x}: {}",
                    crate::value::display_bytes(&u.owner_file),
                    u.block_address,
                    crate::value::display_bytes(&u.asset_path)
                );
            }
        }
        if timing {
            let total = started.elapsed().as_secs_f64();
            println!("Spent {total:.2} seconds on producing this listing");
            if sha {
                let hash_seconds = hash_duration.as_secs_f64();
                println!("Spent {hash_seconds:.2} seconds on calculating SHA sums");
                let percent = if total == 0.0 {
                    0.0
                } else {
                    100.0 * hash_seconds / total
                };
                println!("  (that is {percent:.0}% of the total time");
            }
        }
    }
    match data.missing_error {
        Some(error) => {
            if error.kind.starts_with("incomplete_") {
                crate::failure_receipt::record_terminal(&error, &report_causes);
            }
            Err(error)
        }
        None => Ok(()),
    }
}
fn packed_resources(
    input: &Path,
    low_memory: bool,
    report: &trace::TraceReport,
    op: &mut crate::session::OpCtx,
) -> Result<Vec<serde_json::Value>> {
    // Independent second open on the listing's own emitter, frozen cwd and
    // temp root: notices keep their original multiplicity and timing
    // rather than deduplicating by source, with no per-open resampling.
    let resolved = crate::session::InputStore::resolve(op, input);
    let mut store = crate::session::InputStore::default();
    let mut batch = crate::session::CaptureBatch::default();
    let id = store.ensure(&mut batch, &resolved, false, low_memory, op)?;
    let file = store.reader(id)?;
    let out = packed_resources_from_reader(file, input, &resolved, report)?;
    file.check_source()?;
    file.data.report(input);
    Ok(out)
}
/// Shared packed-resource scan over one already-open reader: the same field
/// probe, unknown-marking and record shape for the CLI second open and the
/// retained captured core. No capture, resolve or completion checks here;
/// the caller owns the reader and any end guards.
fn packed_resources_from_reader(
    file: &BlendFile,
    input_spelling: &Path,
    resolved: &Path,
    report: &trace::TraceReport,
) -> Result<Vec<serde_json::Value>> {
    let mut out = Vec::new();
    for (i, block) in file.blocks().iter().enumerate() {
        if &block.code == b"DNA1" {
            continue;
        }
        for field in ["packedfile", "packedfiles"] {
            let path = if field == "packedfiles" {
                vec![
                    PathStep::Field(field.as_bytes().to_vec()),
                    PathStep::Field(b"first".to_vec()),
                ]
            } else {
                vec![PathStep::Field(field.as_bytes().to_vec())]
            };
            if let Ok(crate::value::Value::Pointer(address)) = file.get(i, 0, &path, true)
                && address != 0
            {
                let mut record = serde_json::json!({"owner_file":json_path(input_spelling),"block_address":block.old_address.to_string(),"packed":true,"field":field});
                let unknown = report.errors.iter().any(|error| {
                    error.owner_file == resolved.as_os_str().as_encoded_bytes()
                        && error
                            .site
                            .as_ref()
                            .and_then(|s| s["block_index"].as_u64())
                            .and_then(|n| usize::try_from(n).ok())
                            .is_some_and(|block| {
                                block == i
                                    || file.owner_index().ok().is_some_and(|index| {
                                        index.owner(file, block)
                                            == Some(index.owner(file, i).unwrap_or(i))
                                    })
                            })
                });
                if unknown || file.get_pointer(i, 0, &path).is_err() {
                    record["packed"] = serde_json::Value::Null;
                    record["status"] = serde_json::json!("unknown");
                }
                out.push(record);
            }
        }
    }
    Ok(out)
}
// Valid UTF-8 keeps the compatibility map shape. Invalid native paths use an
// explicit hex tag, with the tag itself escaped to keep decoding reversible.
fn json_path(path: &Path) -> String {
    let bytes = path.as_os_str().as_encoded_bytes();
    match std::str::from_utf8(bytes) {
        Ok(s) if !s.starts_with("bytes-hex:") && !s.starts_with("utf8:") => s.to_owned(),
        Ok(s) => format!("utf8:{s}"),
        Err(_) => format!("bytes-hex:{}", crate::value::hex(bytes)),
    }
}
fn human(n: usize) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64;
    for unit in ["kB", "MB", "GB", "TB", "PB", "EB"] {
        value /= 1024.0;
        if value < 1024.0 || unit == "EB" {
            return format!("{value:.1} {unit}");
        }
    }
    unreachable!()
}
fn block_name(file: &BlendFile, i: usize) -> Result<String> {
    let block = file.block(i)?;
    let ty = String::from_utf8_lossy(file.dna_type_id(i)?);
    let end = block.code.iter().position(|&b| b == 0).unwrap_or(4);
    Ok(format!(
        "{ty}-{}",
        String::from_utf8_lossy(&block.code[..end])
    ))
}
fn byte_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'\"') {
        b'\"'
    } else {
        b'\''
    };
    let mut out = format!("b{}", quote as char);
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            b if b == quote => {
                out.push('\\');
                out.push(b as char);
            }
            32..=126 => out.push(b as char),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push(quote as char);
    out
}
fn path_repr(path: &[PathStep]) -> String {
    let entries = path
        .iter()
        .map(|s| match s {
            PathStep::Field(v) => byte_repr(v),
            PathStep::Index(v) => v.to_string(),
        })
        .collect::<Vec<_>>();
    if entries.len() == 1 {
        entries[0].clone()
    } else {
        format!("({})", entries.join(", "))
    }
}
struct ReferenceOutput<'a> {
    writer: &'a mut dyn Write,
    found: bool,
}
fn references<'a>(
    file: &'a BlendFile,
    block: usize,
    si: usize,
    source: (usize, &[u8]),
    prefix: &mut Vec<&'a [u8]>,
    address: u64,
    output: &mut ReferenceOutput<'_>,
) -> Result<()> {
    let (base, bytes) = source;
    for field in &file.schema().structs[si].fields {
        let offset = crate::checked::add(base, field.offset)?;
        prefix.push(&field.name);
        for element in 0..field.array_len {
            let at =
                crate::checked::add(offset, crate::checked::mul(element, field.element_size)?)?;
            if field.pointer {
                let end = crate::checked::add(at, file.header().pointer_size)?;
                let raw = bytes
                    .get(at..end)
                    .ok_or_else(|| Error::new("truncated", "reference field outside record"))?;
                let value = raw.iter().enumerate().fold(0u64, |n, (i, &b)| {
                    let shift = if file.header().little_endian {
                        i
                    } else {
                        raw.len() - 1 - i
                    } * 8;
                    n | (u64::from(b) << shift)
                });
                if value == address {
                    let b = file.block(block)?;
                    let ty = String::from_utf8_lossy(file.dna_type_id(block)?);
                    let end = b.code.iter().position(|&b| b == 0).unwrap_or(4);
                    let path = prefix
                        .iter()
                        .map(|s| PathStep::Field(s.to_vec()))
                        .collect::<Vec<_>>();
                    writeln!(
                        output.writer,
                        "     <BlendFileBlock.{ty} ({}), size={} at {:#x}> {}",
                        String::from_utf8_lossy(&b.code[..end]),
                        b.size,
                        b.old_address,
                        path_repr(&path)
                    )
                    .map_err(crate::edit_output::io)?;
                    output.found = true;
                }
            } else if let Some(&child) = file.schema().by_type.get(&field.type_index) {
                references(file, block, child, (at, bytes), prefix, address, output)?;
            }
        }
        prefix.pop();
    }
    Ok(())
}
fn blocks(input: &Path, args: &[OsString]) -> Result<()> {
    // The ten-row default is the observed public `blocks` presentation;
    // explicit --limit controls display only, never the record scan.
    let (mut dump, mut limit, mut requested) = (false, 10i64, None);
    let mut i = 0;
    while i < args.len() {
        match arg(&args[i])? {
            "--dump" => dump = true,
            "--limit" => {
                i += 1;
                let n = arg(args
                    .get(i)
                    .ok_or_else(|| usage("--limit requires an integer"))?)?
                .parse::<i64>()
                .map_err(|_| usage("invalid block limit"))?;
                limit = n;
            }
            value if value.starts_with("--address=") => {
                let value = &value[10..];
                requested = Some(
                    if let Some(hex) = value.strip_prefix("0x") {
                        u64::from_str_radix(hex, 16)
                    } else {
                        value.parse()
                    }
                    .map_err(|_| usage("invalid block address"))?,
                );
            }
            _ => {
                return Err(usage(
                    "blocks INPUT [--dump] [--limit N] [--address=ADDRESS]",
                ));
            }
        }
        i += 1;
    }
    blocks_to_writer(input, limit, requested, dump, &mut FragmentOut)
}

/// CLI stdout delivery for the shared blocks writer: every formatted
/// fragment goes through deliver_text with its span and actual-write
/// counter, exactly as before the extraction. Fragment-formatted only;
/// direct byte writes are refused, not silently dropped.
struct FragmentOut;
impl std::io::Write for FragmentOut {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other(
            "blocks text is delivered only through formatted fragments",
        ))
    }
    fn write_fmt(&mut self, args: std::fmt::Arguments<'_>) -> std::io::Result<()> {
        deliver_text(args).map_err(|e| {
            // Preserve the engine kind through the writer boundary so the
            // shared core's io reclassification stays exact (broken_pipe).
            std::io::Error::new(
                if e.kind == "broken_pipe" {
                    std::io::ErrorKind::BrokenPipe
                } else {
                    std::io::ErrorKind::Other
                },
                e.message,
            )
        })
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Shared blocks text producer: one opened capture, type statistics,
/// largest-block referrer scan and optional dump, written to any
/// caller-owned writer. The CLI passes fragment-delivering stdout; native
/// callers collect the exact CLI text.
pub fn blocks_to_writer(
    input: &Path,
    limit: i64,
    address: Option<u64>,
    dump: bool,
    output: &mut dyn Write,
) -> Result<()> {
    // The native binding enters here directly; the CLI single-input gate
    // above fires first for the binary. One shared owner either way.
    if !input.is_file() {
        return Err(missing_input_error());
    }
    macro_rules! println {
        () => { writeln!(output).map_err(crate::edit_output::io)? };
        ($($arg:tt)*) => { writeln!(output, $($arg)*).map_err(crate::edit_output::io)? };
    }
    println!("Opening {}", input.display());
    // Invocation-scoped streaming reads run under their own silent
    // operation: frozen cwd/temp plus a drain-nowhere sink. Blocks stays
    // silent by construction — the sink can never fire here.
    let mut op = crate::session::OpCtx::new(
        crate::session::GuardPolicy::Legacy,
        None,
        std::env::current_dir().map_err(crate::edit_output::io)?,
        std::env::temp_dir(),
    );
    // Shared borrows end with the call; the drain below only clears the
    // (never-filled) channel.
    let file = {
        let (sink, temp) = (&op.sink, &op.temp_root);
        BlendFile::open_blocks(input, sink, temp)?
    };
    op.drain();
    println!("Inspecting {}", input.display());
    let mut groups: Vec<(String, usize, Vec<usize>)> = Vec::new();
    let mut indices = BTreeMap::new();
    for (index, b) in file.blocks().iter().enumerate() {
        if &b.code == b"DNA1" {
            continue;
        }
        let name = block_name(&file, index)?;
        let entry = *indices.entry(name.clone()).or_insert_with(|| {
            groups.push((name, 0, Vec::new()));
            groups.len() - 1
        });
        groups[entry].1 = crate::checked::add(groups[entry].1, b.size)?;
        groups[entry].2.push(index);
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.1));
    println!("Block type                          Total Size Num blocks   Avg Size     Median");
    println!("----------------------------------- ---------- ---------- ---------- ----------");
    // A negative display limit uses the observed compatibility slice semantics:
    // -1 omits the final group; it is not an unlimited scan or processing bound.
    let displayed = if limit < 0 {
        groups
            .len()
            .saturating_sub(usize::try_from(limit.unsigned_abs()).unwrap_or(usize::MAX))
    } else {
        usize::try_from(limit).unwrap_or(usize::MAX)
    };
    for (name, total, members) in groups.iter().take(displayed) {
        let mut sizes = members
            .iter()
            .map(|&i| file.blocks()[i].size)
            .collect::<Vec<_>>();
        sizes.sort_unstable();
        let middle = sizes.len() / 2;
        // Black-box even-length vectors [54,55], [55,56], [1,2], [2,3]
        // qualify the compatibility upper median (not an averaged median).
        let median = sizes[middle];
        println!(
            "{name:<35} {:>10} {:>10} {:>10} {:>10}",
            human(*total),
            members.len(),
            human(total / members.len()),
            human(median)
        );
    }
    println!("----------------------------------------------------------------------");
    let selected =
        if let Some(address) = address {
            Some(file.dereference(address)?.ok_or_else(|| {
                Error::new("missing_address", "selected address is null or absent")
            })?)
        } else {
            groups.first().and_then(|g| {
                g.2.iter().copied().max_by(|&a, &b| {
                    file.blocks()[a]
                        .size
                        .cmp(&file.blocks()[b].size)
                        .then_with(|| b.cmp(&a))
                })
            })
        };
    if let Some(index) = selected {
        let b = file.block(index)?;
        println!(
            "Biggest {} block is {} at address {:#x}",
            block_name(&file, index)?,
            human(b.size),
            b.old_address
        );
        println!("Finding what points there");
        let found = {
            let mut references_output = ReferenceOutput {
                writer: output,
                found: false,
            };
            file.scan_records(|other, bytes| {
                references(
                    &file,
                    other,
                    file.blocks()[other].sdna_index,
                    (0, bytes),
                    &mut Vec::new(),
                    b.old_address,
                    &mut references_output,
                )
            })?;
            references_output.found
        };
        if !found {
            println!("Nothing points there");
        }
        if dump {
            println!("Hexdump:");
            // Compatibility hexdump line width is an observed presentation
            // format, not a processing limit; every payload byte is emitted.
            let payload = file.payload(index)?;
            for (line, chunk) in payload.chunks(32).enumerate() {
                println!(
                    "{:>6} - {}",
                    line * 32,
                    chunk
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
        }
    }
    file.check_source()?;
    Ok(())
}

pub(crate) fn hash_file(path: &Path) -> Result<(String, f64)> {
    let mut span = crate::diagnostics::Span::new(
        "hash",
        if crate::diagnostics::enabled() {
            serde_json::json!({"path":path.to_string_lossy(),"algorithm":"sha256"})
        } else {
            serde_json::Value::Null
        },
    );
    let result = (|| {
        let start = std::time::Instant::now();
        use std::os::unix::fs::MetadataExt;
        let file = std::fs::File::open(path).map_err(crate::edit_output::io)?;
        let metadata = file.metadata().map_err(crate::edit_output::io)?;
        let capacity = usize::try_from(metadata.blksize())
            .map_err(|_| Error::new("overflow", "filesystem I/O block size"))?;
        let mut source = std::io::BufReader::with_capacity(capacity, file);
        let digest = hash_reader(&mut source)?;
        let elapsed = start.elapsed().as_secs_f64();
        if span.active() {
            span.data(serde_json::json!({"path":path.to_string_lossy(),"algorithm":"sha256","bytes":metadata.len(),"elapsed_ms":elapsed*1000.0}));
        }
        Ok((digest, elapsed))
    })();
    span.result(result)
}
pub(crate) fn hash_bytes(bytes: &[u8]) -> Result<String> {
    hash_reader(&mut std::io::Cursor::new(bytes))
}
pub(crate) fn hash_reader(source: &mut impl std::io::Read) -> Result<String> {
    let mut state = Sha256::new();
    // SHA-256's block width, not a workload ceiling.
    let mut block = [0u8; 64];
    loop {
        let n = source.read(&mut block).map_err(crate::edit_output::io)?;
        if n == 0 {
            break;
        }
        state.update(&block[..n])?;
    }
    state.finish()
}
#[derive(Clone)]
pub(crate) struct Sha256 {
    h: [u32; 8],
    block: [u8; 64],
    used: usize,
    length: u64,
    compress: fn(&mut [u32; 8], &[u8; 64]),
}
impl Sha256 {
    pub(crate) fn new() -> Self {
        Self {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0; 64],
            used: 0,
            length: 0,
            compress: sha_compressor(),
        }
    }
    pub(crate) fn update(&mut self, mut bytes: &[u8]) -> Result<()> {
        self.length = self
            .length
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| Error::new("sha256_width", "message length overflow"))?;
        while !bytes.is_empty() {
            let n = (64 - self.used).min(bytes.len());
            self.block[self.used..self.used + n].copy_from_slice(&bytes[..n]);
            self.used += n;
            bytes = &bytes[n..];
            if self.used == 64 {
                (self.compress)(&mut self.h, &self.block);
                self.used = 0;
            }
        }
        Ok(())
    }
    pub(crate) fn finish(mut self) -> Result<String> {
        self.block[self.used] = 0x80;
        self.block[self.used + 1..].fill(0);
        if self.used >= 56 {
            (self.compress)(&mut self.h, &self.block);
            self.block.fill(0);
        }
        self.block[56..].copy_from_slice(
            &self
                .length
                .checked_mul(8)
                .ok_or_else(|| Error::new("sha256_width", "bit length overflow"))?
                .to_be_bytes(),
        );
        (self.compress)(&mut self.h, &self.block);
        Ok(self.h.iter().map(|v| format!("{v:08x}")).collect())
    }
}
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];
fn sha_block(h: &mut [u32; 8], b: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (i, c) in b.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes(c.try_into().unwrap());
    }
    for i in 16..64 {
        let x = w[i - 15];
        let y = w[i - 2];
        w[i] = w[i - 16]
            .wrapping_add(x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3))
            .wrapping_add(w[i - 7])
            .wrapping_add(y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10));
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for i in 0..64 {
        let t = hh
            .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
            .wrapping_add((e & f) ^ (!e & g))
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
            .wrapping_add((a & b) ^ (a & c) ^ (b & c));
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t);
        d = c;
        c = b;
        b = a;
        a = t.wrapping_add(t2);
    }
    for (x, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
        *x = x.wrapping_add(v);
    }
}
fn sha_compressor() -> fn(&mut [u32; 8], &[u8; 64]) {
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("sha")
        && std::arch::is_x86_feature_detected!("ssse3")
        && std::arch::is_x86_feature_detected!("sse4.1")
    {
        return |h, b| unsafe { sha_block_x86(h, b) };
    }
    sha_block
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sha,ssse3,sse4.1")]
unsafe fn sha_block_x86(h: &mut [u32; 8], b: &[u8; 64]) {
    use std::arch::x86_64::*;

    unsafe {
        let tmp = _mm_shuffle_epi32::<0xb1>(_mm_loadu_si128(h.as_ptr().cast()));
        let mut s1 = _mm_shuffle_epi32::<0x1b>(_mm_loadu_si128(h.as_ptr().add(4).cast()));
        let mut s0 = _mm_alignr_epi8::<8>(tmp, s1);
        s1 = _mm_blend_epi16::<0xf0>(s1, tmp);
        let saved0 = s0;
        let saved1 = s1;
        let mask = _mm_set_epi64x(0x0c0d0e0f08090a0b, 0x0405060700010203);
        let mut words = [_mm_setzero_si128(); 4];
        for (i, word) in words.iter_mut().enumerate() {
            *word = _mm_shuffle_epi8(_mm_loadu_si128(b.as_ptr().add(i * 16).cast()), mask);
        }
        for group in 0..16 {
            let j = group % 4;
            if group >= 4 {
                let first = _mm_sha256msg1_epu32(words[j], words[(j + 1) % 4]);
                let cross = _mm_alignr_epi8::<4>(words[(j + 3) % 4], words[(j + 2) % 4]);
                words[j] = _mm_sha256msg2_epu32(_mm_add_epi32(first, cross), words[(j + 3) % 4]);
            }
            let msg = _mm_add_epi32(words[j], _mm_loadu_si128(K.as_ptr().add(group * 4).cast()));
            s1 = _mm_sha256rnds2_epu32(s1, s0, msg);
            s0 = _mm_sha256rnds2_epu32(s0, s1, _mm_shuffle_epi32::<0x0e>(msg));
        }
        s0 = _mm_add_epi32(s0, saved0);
        s1 = _mm_add_epi32(s1, saved1);
        let tmp = _mm_shuffle_epi32::<0x1b>(s0);
        s1 = _mm_shuffle_epi32::<0xb1>(s1);
        _mm_storeu_si128(h.as_mut_ptr().cast(), _mm_blend_epi16::<0xf0>(tmp, s1));
        _mm_storeu_si128(h.as_mut_ptr().add(4).cast(), _mm_alignr_epi8::<8>(s1, tmp));
    }
}
#[cfg(test)]
mod hash_tests {
    #[test]
    fn native_path_json_encoding_is_reversible_and_tags_do_not_collide() {
        use std::os::unix::ffi::OsStringExt;
        let raw = std::path::PathBuf::from(std::ffi::OsString::from_vec(b"a\"\xff".to_vec()));
        assert_eq!(super::json_path(&raw), "bytes-hex:6122ff");
        assert_eq!(
            super::json_path(std::path::Path::new("bytes-hex:6122ff")),
            "utf8:bytes-hex:6122ff"
        );
        assert_eq!(
            super::json_path(std::path::Path::new("utf8:hello")),
            "utf8:utf8:hello"
        );
        let json = serde_json::to_string(&super::json_path(&raw)).unwrap();
        assert_eq!(
            serde_json::from_str::<String>(&json).unwrap(),
            "bytes-hex:6122ff"
        );
    }
    #[test]
    fn human_and_field_path_public_presentation_vectors() {
        use crate::value::PathStep;
        assert_eq!(super::human(19), "19 B");
        assert_eq!(super::human(1024), "1.0 kB");
        assert_eq!(
            super::path_repr(&[PathStep::Field(b"data".to_vec())]),
            "b'data'"
        );
        assert_eq!(
            super::path_repr(&[
                PathStep::Field(b"regionbase".to_vec()),
                PathStep::Field(b"first".to_vec())
            ]),
            "(b'regionbase', b'first')"
        );
        assert_eq!(super::byte_repr(b"a\xff"), "b'a\\xff'");
    }
    #[test]
    fn accelerated_rounds_match_scalar_for_every_byte_seed() {
        for seed in 0..=u8::MAX {
            let block = std::array::from_fn(|i| seed.wrapping_add(i as u8));
            let mut scalar = std::array::from_fn(|i| (seed as u32).wrapping_mul(i as u32 + 1));
            let mut actual = scalar;
            super::sha_block(&mut scalar, &block);
            super::sha_compressor()(&mut actual, &block);
            assert_eq!(actual, scalar);
        }
    }
}
#[cfg(test)]
mod memory_tests {
    use super::pack_arguments;
    use std::ffi::OsString;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn workdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "oxblend-memory-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("scene.blend"), b"BLENDER").unwrap();
        dir
    }
    fn args(dir: &std::path::Path, extra: &[&str]) -> Vec<OsString> {
        let mut out = vec![
            OsString::from(dir.join("scene.blend")),
            OsString::from(dir.join("package-new")),
        ];
        out.extend(extra.iter().map(OsString::from));
        out
    }
    #[test]
    fn memory_defaults_to_fast_and_parses_both_spellings() {
        let dir = workdir();
        let (_, _, options) = pack_arguments("pack", &args(&dir, &[])).unwrap();
        assert_eq!(options.memory, crate::session::MemoryMode::Fast);
        for command in ["pack", "pack-sequence"] {
            for extra in [vec!["--memory", "bounded"], vec!["--memory=bounded"]] {
                let mut argv = args(&dir, &extra);
                if command == "pack-sequence" {
                    argv.insert(1, OsString::from("-t"));
                }
                let (_, _, options) = pack_arguments(command, &argv).unwrap();
                assert_eq!(options.memory, crate::session::MemoryMode::Bounded);
            }
            let mut argv = args(&dir, &["--memory=fast"]);
            if command == "pack-sequence" {
                argv.insert(1, OsString::from("-t"));
            }
            let (_, _, options) = pack_arguments(command, &argv).unwrap();
            assert_eq!(options.memory, crate::session::MemoryMode::Fast);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn unknown_memory_is_a_usage_refusal() {
        let dir = workdir();
        let error = pack_arguments("pack", &args(&dir, &["--memory", "huge"]))
            .err()
            .unwrap();
        assert_eq!(error.kind, "usage");
        let error = pack_arguments("pack", &args(&dir, &["--memory=huge"]))
            .err()
            .unwrap();
        assert_eq!(error.kind, "usage");
        // A missing value keeps the existing pack arity refusal, not a mode guess.
        let error = pack_arguments("pack", &args(&dir, &["--memory"]))
            .err()
            .unwrap();
        assert_eq!(error.kind, "usage");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
/// Focused proof for the retained-listing seam: the caller-owned
/// `listing_in_store` core (Session policy, captured, supplied-store packed
/// reader) must project the same JSON value, notices and typed domain error
/// as the native report seam on the same fixture and flags.
#[cfg(test)]
mod listing_core_tests {
    use super::*;
    // Error-parity pin: one shared missing-input owner for the CLI gate,
    // the listing seams and the blocks writer.
    #[test]
    fn shared_missing_input_matches_cli_canonical() {
        let error = missing_input_error();
        assert_eq!(error.kind, "missing_input");
        assert_eq!(
            error.message,
            "selected blend input missing or not a regular file"
        );
    }
    fn flags(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }
    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/dependencies")
            .join(name)
    }
    fn retained(
        input: &Path,
        args: &[OsString],
    ) -> (Json, Vec<Json>, Option<Error>, crate::session::InputStore) {
        let mut op = crate::session::OpCtx::new(
            crate::session::GuardPolicy::Session,
            None,
            std::env::current_dir().unwrap(),
            std::env::temp_dir(),
        );
        let mut store = crate::session::InputStore::default();
        let mut batch = crate::session::CaptureBatch::default();
        let resolved = crate::session::InputStore::resolve(&op, input);
        let (value, notices, error) = listing_in_store(
            input, &resolved, &mut store, &mut batch, &mut op, args, true,
        )
        .unwrap();
        // The retained root stays in the supplied arena: no reopen needed.
        assert!(store.id_for_path(&resolved).is_ok());
        (value, notices, error, store)
    }
    fn error_key(error: &Option<Error>) -> Option<(&'static str, String)> {
        error.as_ref().map(|e| (e.kind, e.message.clone()))
    }
    #[test]
    fn retained_core_matches_native_report_json_full_info() {
        for name in ["group-root.blend", "mesh-material-root.blend"] {
            let input = fixture(name);
            assert!(input.is_file(), "missing fixture {name}");
            let args = flags(&["--json", "--full-info"]);
            let (expected, expected_notices, expected_error) =
                listing_report(&input, &args).unwrap();
            let (value, notices, error, _store) = retained(&input, &args);
            assert_eq!(value, expected, "JSON projection drift for {name}");
            assert_eq!(notices, expected_notices, "notice drift for {name}");
            assert_eq!(error_key(&error), error_key(&expected_error));
        }
    }
    #[test]
    fn retained_core_matches_native_report_json_members_only() {
        let input = fixture("group-root.blend");
        let args = flags(&["--json"]);
        let (expected, expected_notices, expected_error) = listing_report(&input, &args).unwrap();
        let (value, notices, error, _store) = retained(&input, &args);
        assert_eq!(value, expected);
        assert_eq!(notices, expected_notices);
        assert_eq!(error_key(&error), error_key(&expected_error));
    }
    #[test]
    fn retained_core_rejects_non_json_like_native() {
        let input = fixture("group-root.blend");
        let error = listing_report(&input, &flags(&[]))
            .unwrap()
            .2
            .expect("native text without writer must refuse");
        assert_eq!(error.kind, "usage");
        let mut op = crate::session::OpCtx::new(
            crate::session::GuardPolicy::Session,
            None,
            std::env::current_dir().unwrap(),
            std::env::temp_dir(),
        );
        let mut store = crate::session::InputStore::default();
        let mut batch = crate::session::CaptureBatch::default();
        let resolved = crate::session::InputStore::resolve(&op, &input);
        let error = match listing_in_store(
            &input,
            &resolved,
            &mut store,
            &mut batch,
            &mut op,
            &[],
            true,
        ) {
            Err(error) => error,
            Ok(_) => panic!("retained text must refuse without a writer"),
        };
        assert_eq!(error.as_error().kind, "usage");
    }
}
