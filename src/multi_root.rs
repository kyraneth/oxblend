//! Cross-reference the existing single-root trace facts; no second discovery walk.
use crate::{
    Error, Result,
    trace::{self, TraceOptions, TraceReport},
    value::hex,
};
use serde_json::{Value as Json, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::Write,
    os::unix::ffi::OsStringExt,
    path::{Component, Path, PathBuf},
};

pub struct Selection {
    pub roots: Vec<PathBuf>,
    pub first_spelling: OsString,
    pub options: Vec<OsString>,
    pub summary: bool,
}
pub fn arguments(args: &[OsString]) -> Result<Selection> {
    let nul = crate::path_list::nul_delimited(args);
    let mut roots = Vec::new();
    let mut options = Vec::new();
    let mut summary = false;
    let mut i = 0;
    while i < args.len() {
        if let Some(path) = crate::path_list::inline_path(&args[i]) {
            roots.extend(crate::path_list::read(Path::new(path), nul)?);
            i += 1;
            continue;
        }
        match args[i].to_str() {
            Some("--") => {
                roots.extend(args[i + 1..].iter().map(PathBuf::from));
                break;
            }
            Some("-0") => {}
            Some("--files-from") => {
                i += 1;
                let p = args
                    .get(i)
                    .ok_or_else(|| Error::new("usage", "--files-from needs a path"))?;
                roots.extend(crate::path_list::read(Path::new(p), nul)?);
            }
            Some("--summary") if !summary => summary = true,
            Some(s) if s.starts_with('-') => options.push(args[i].clone()),
            _ => roots.push(PathBuf::from(&args[i])),
        }
        i += 1;
    }
    let first_spelling = roots
        .first()
        .ok_or_else(|| Error::new("usage", "at least one selected root is required"))?
        .as_os_str()
        .to_owned();
    let absolute = roots
        .into_iter()
        .map(|root| std::path::absolute(root).map_err(|e| Error::new("io", e.to_string())))
        .collect::<Result<Vec<_>>>()?;
    let selected = select_roots(absolute);
    Ok(Selection {
        roots: selected,
        first_spelling,
        options,
        summary,
    })
}

/// One root-selection policy: callers supply absolute paths (Session callers
/// resolve against their frozen cwd); lexical identity is not realpath identity.
pub(crate) fn select_roots(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    paths
        .into_iter()
        .map(|path| trace::lexical(&path))
        .filter(|path| seen.insert(path.clone()))
        .collect()
}
pub(crate) fn error_json(e: &Error) -> Json {
    json!({"kind":e.kind,"message":e.message})
}
pub(crate) fn root_json(root: &Path, report: Option<&TraceReport>, error: Option<&Error>) -> Json {
    json!({"input_path_hex":hex(root.as_os_str().as_encoded_bytes()),"input_path":root.to_str(),"trace":report.map(TraceReport::json),"error":error.map(error_json)})
}
fn from_bytes(p: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_vec(p.to_vec()))
}
/// POSIX realpath identity, including absent suffixes. Follow symlinks before
/// applying '..'; hard links are deliberately distinct path identities.
fn canonical(path: &Path) -> Result<PathBuf> {
    fn walk(path: &Path, mut result: PathBuf, active: &mut BTreeSet<PathBuf>) -> Result<PathBuf> {
        for component in path.components() {
            match component {
                Component::RootDir => result = PathBuf::from("/"),
                Component::CurDir => {}
                Component::ParentDir => {
                    result.pop();
                }
                Component::Normal(part) => {
                    result.push(part);
                    match std::fs::symlink_metadata(&result) {
                        Ok(m) if m.file_type().is_symlink() => {
                            if !active.insert(result.clone()) {
                                return Err(Error::new(
                                    "io",
                                    "symlink cycle in dependency identity",
                                ));
                            }
                            let link = result.clone();
                            let target = std::fs::read_link(&link)
                                .map_err(|e| Error::new("io", e.to_string()))?;
                            result.pop();
                            result = walk(&target, result, active)?;
                            active.remove(&link);
                        }
                        Ok(_) => {}
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                            ) => {}
                        Err(e) => {
                            return Err(Error::new(
                                "io",
                                format!("dependency identity '{}': {e}", result.display()),
                            ));
                        }
                    }
                }
                Component::Prefix(_) => {
                    return Err(Error::new("io", "unsupported native path prefix"));
                }
            }
        }
        Ok(result)
    }
    walk(path, PathBuf::from("/"), &mut BTreeSet::new())
}
fn patterned(path: &Path, sequence: bool) -> bool {
    if !sequence {
        return false;
    }
    let bytes = path.as_os_str().as_encoded_bytes();
    if path.is_dir() || bytes.ends_with(b"/") {
        return true;
    }
    if bytes.iter().any(|b| b"*?[#".contains(b))
        || bytes.windows(6).any(|w| w == b"<UDIM>")
        || bytes.windows(8).any(|w| w == b"<UVTILE>")
    {
        return true;
    }
    let stem = path
        .file_stem()
        .map(|p| p.as_encoded_bytes())
        .unwrap_or_default();
    stem.last().is_some_and(u8::is_ascii_digit)
}
struct Asset {
    identity: PathBuf,
    paths: BTreeSet<PathBuf>,
    unexpanded: bool,
    exists: bool,
    bytes: Option<u64>,
    uses: BTreeMap<(String, Vec<String>), BTreeSet<usize>>,
}
/// `reports` and `errors` align with the lexical selected roots. Hard failures
/// carry no successful trace; independently readable roots remain available.
pub(crate) fn cross_reference(
    roots: &[PathBuf],
    reports: &[Option<&TraceReport>],
    errors: &[Option<&Error>],
) -> Result<Json> {
    let span = crate::diagnostics::Span::new("cross_reference", json!({"roots":roots.len()}));
    let result = (|| {
        let mut assets: BTreeMap<String, Asset> = BTreeMap::new();
        let mut observation_errors = vec![None; roots.len()];
        let mut observation_sites = vec![Vec::new(); roots.len()];
        for (n, root) in roots.iter().enumerate() {
            let rid = hex(root.as_os_str().as_encoded_bytes());
            let Some(report) = reports[n] else { continue };
            for (index, usage) in report.usages.iter().enumerate() {
                let observation = (|| {
                    let resolved = trace::resolve_usage(usage);
                    let (members, unexpanded) = if resolved.is_dir() && !usage.is_sequence {
                        (vec![resolved.clone()], false)
                    } else {
                        match usage.available_members() {
                            Ok(mut m) => {
                                m.extend(usage.expected_paths());
                                m.sort();
                                m.dedup();
                                (m, usage.pattern_unknown.is_some())
                            }
                            Err(e) if e.kind == "missing_asset" => (
                                vec![resolved.clone()],
                                patterned(&resolved, usage.is_sequence),
                            ),
                            Err(e) => return Err(e),
                        }
                    };
                    let mut chain = usage
                        .library_chain
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            if i == 0 {
                                Ok(root.clone())
                            } else {
                                canonical(&from_bytes(p))
                            }
                        })
                        .collect::<Result<Vec<_>>>()?;
                    if chain.is_empty() {
                        chain.push(root.clone());
                    }
                    let chain_hex = chain
                        .iter()
                        .map(|p| hex(p.as_os_str().as_encoded_bytes()))
                        .collect::<Vec<_>>();
                    for member in members {
                        let identity = canonical(&member)?;
                        let aid = hex(identity.as_os_str().as_encoded_bytes());
                        let metadata = match std::fs::metadata(&member) {
                            Ok(m) => Some(m),
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    std::io::ErrorKind::NotFound
                                        | std::io::ErrorKind::NotADirectory
                                ) =>
                            {
                                None
                            }
                            Err(e) => {
                                return Err(Error::new(
                                    "io",
                                    format!("dependency stat '{}': {e}", member.display()),
                                ));
                            }
                        };
                        let exists = metadata.is_some() && !unexpanded;
                        let bytes = metadata
                            .as_ref()
                            .filter(|m| m.is_file() && exists)
                            .map(|m| m.len());
                        let a = assets.entry(aid).or_insert_with(|| Asset {
                            identity,
                            paths: BTreeSet::new(),
                            unexpanded,
                            exists,
                            bytes,
                            uses: BTreeMap::new(),
                        });
                        if a.exists != exists || a.bytes != bytes {
                            return Err(Error::new(
                                "input_changed",
                                "dependency availability changed while aggregating roots",
                            ));
                        }
                        a.paths.insert(member);
                        a.uses
                            .entry((rid.clone(), chain_hex.clone()))
                            .or_default()
                            .insert(index);
                    }
                    Ok(())
                })();
                if let Err(error) = observation {
                    record_observation_failure(&error, usage, index);
                    let mut site = error_json(&error);
                    site["usage_index"] = json!(index);
                    site["resolved_path_hex"] = json!(hex(trace::resolve_usage(usage)
                        .as_os_str()
                        .as_encoded_bytes()));
                    observation_sites[n].push(site);
                    observation_errors[n].get_or_insert(error);
                }
            }
        }
        let shared = assets
            .iter()
            .filter(|(_, a)| a.uses.keys().map(|(r, _)| r).collect::<BTreeSet<_>>().len() > 1)
            .map(|(id, _)| id.clone())
            .collect::<BTreeSet<_>>();
        let mut rows = Vec::new();
        for (i, root) in roots.iter().enumerate() {
            let rid = hex(root.as_os_str().as_encoded_bytes());
            let used = assets
                .iter()
                .filter(|(_, a)| a.uses.keys().any(|(r, _)| r == &rid))
                .collect::<Vec<_>>();
            let missing = used
                .iter()
                .filter(|(_, a)| !a.exists)
                .map(|(id, _)| *id)
                .collect::<Vec<_>>();
            let missing_links = reports[i]
                .map(|r| {
                    r.errors
                        .iter()
                        .filter(|e| {
                            matches!(
                                e.kind,
                                trace::FailureKind::MissingId | trace::FailureKind::MissingLibrary
                            )
                        })
                        .map(|e| e.json())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let error = errors[i].or(observation_errors[i].as_ref());
            let complete = reports[i].is_some_and(|r| r.complete)
                && missing.is_empty()
                && missing_links.is_empty()
                && error.is_none();
            let mut row = json!({"root_id":rid,"input_path_hex":rid,"input_path":root.to_str(),
            "traversal_complete":reports[i].is_some_and(|r|r.complete),"complete":complete,
            "shared":used.iter().filter(|(id,_)|shared.contains(*id)).map(|(id,_)|*id).collect::<Vec<_>>(),
            "unique":used.iter().filter(|(id,_)|!shared.contains(*id)).map(|(id,_)|*id).collect::<Vec<_>>(),
            "missing_files":missing,"missing_links":missing_links,"error":error.map(error_json)});
            if !observation_sites[i].is_empty() {
                row["observation_errors"] = json!(observation_sites[i]);
            }
            rows.push(row);
        }
        let existing = assets.values().filter(|a| a.exists).count();
        let bytes = assets
            .values()
            .filter_map(|a| a.bytes)
            .try_fold(0u64, |total, n| {
                total
                    .checked_add(n)
                    .ok_or_else(|| Error::new("io", "dependency byte total overflow"))
            })?;
        let values=assets.iter().map(|(id,a)|{
        let mut paths=a.paths.iter().collect::<Vec<_>>();
        paths.sort_by(|a,b|a.as_os_str().as_encoded_bytes().cmp(b.as_os_str().as_encoded_bytes()));
        let paths_hex=paths.iter().map(|p|hex(p.as_os_str().as_encoded_bytes())).collect::<Vec<_>>();
        let uses=a.uses.iter().map(|((rid,chain),indices)|json!({"root_id":rid,"chain_hex":chain,
            "chain":chain.iter().map(|p|decode_hex_path(p).and_then(|p|p.to_str().map(str::to_owned))).collect::<Vec<_>>(),
            "direct":chain.len()==1,"usage_indices":indices})).collect::<Vec<_>>();
        json!({"asset_id":id,"identity_path_hex":id,"identity_path":a.identity.to_str(),"resolved_paths_hex":paths_hex,
            "resolved_paths":paths.iter().map(|p|p.to_str()).collect::<Vec<_>>(),"unexpanded":a.unexpanded,"exists":a.exists,"bytes":a.bytes,"shared":shared.contains(id),"used_by":uses})
    }).collect::<Vec<_>>();
        Ok(
            json!({"schema":"oxblend.cross-reference/v1","complete":rows.iter().all(|r|r["complete"]==true),"assets":values,"roots":rows,"shared":shared,
        "totals":{"assets":assets.len(),"existing_assets":existing,"missing_assets":assets.len()-existing,"shared_assets":shared.len(),"unique_assets":assets.len()-shared.len(),"bytes":bytes,"unknown_size_assets":assets.values().filter(|a|a.bytes.is_none()).count()}}),
        )
    })();
    span.result(result)
}
// Filesystem observation, not a pointer namespace or guessed causal ID.
fn record_observation_failure(error: &Error, usage: &trace::Usage, _index: usize) {
    let field = usage
        .path_full_field
        .as_ref()
        .or(usage.path_dir_field.as_ref())
        .or(usage.path_base_field.as_ref());
    crate::failure_receipt::record_source_failure(
        error,
        "dependency.identity.observe",
        crate::failure_receipt::SourceContext {
            file: Some(from_bytes(&usage.owner_file)),
            old_address: Some(usage.block_address),
            element: field.map(|f| f.element),
            field_path: field.map(|f| f.path.clone()).unwrap_or_default(),
            ..Default::default()
        },
        &[],
    );
}
fn decode_hex_path(s: &str) -> Option<PathBuf> {
    let bytes = s
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
        })
        .collect::<Option<Vec<_>>>()?;
    Some(from_bytes(&bytes))
}
pub fn run(command: &str, selection: &Selection) -> Result<()> {
    let started = std::time::Instant::now();
    let has = |flag: &str| selection.options.iter().any(|f| f == flag);
    let human = selection.summary || (command == "list" && !has("--json"));
    let mut presentation_error = None;
    let mut opts = TraceOptions::default();
    let mut seen_mode = false;
    let mut seen_pointers = false;
    let mut list_flags = Vec::new();
    for flag in &selection.options {
        match flag.to_str() {
            Some("--low-memory") if !opts.low_memory => opts.low_memory = true,
            Some("--per-file" | "--recursive") if command == "trace" && !seen_mode => {
                opts.mode = if flag == "--per-file" {
                    trace::TraceMode::PerFile
                } else {
                    trace::TraceMode::Recursive
                };
                seen_mode = true;
            }
            Some("--strict" | "--permissive" | "--non-strict") if !seen_pointers => {
                seen_pointers = true
            }
            Some("--json" | "--full-info" | "--sha256" | "--timing") if command == "list" => {}
            _ => {
                return Err(Error::new(
                    "usage",
                    "unknown or duplicate dependency option",
                ));
            }
        }
        if command == "list" && flag != "--sha256" && flag != "--timing" {
            list_flags.push(flag.clone());
        }
    }
    if command == "list" && !list_flags.iter().any(|f| f == "--json") {
        list_flags.push("--json".into());
    }
    let mut reports = Vec::new();
    let mut errors = Vec::new();
    for root in &selection.roots {
        let span = crate::diagnostics::Span::new(
            "trace.root",
            json!({"input_path_hex":hex(root.as_os_str().as_encoded_bytes())}),
        );
        match span.result(crate::bat_cli::trace_cli(root, opts)) {
            Ok(r) => {
                reports.push(Some(r));
                errors.push(None);
            }
            Err(e) => {
                reports.push(None);
                errors.push(Some(e));
            }
        }
    }
    let mut value = dependency_value(command, &selection.roots, &reports, &errors)?;
    let mut complete = value["complete"] == true;
    if command == "list" && !human {
        if has("--sha256") || has("--timing") {
            writeln!(
                std::io::stderr().lock(),
                "--sha256/--timing cannot be used with --json; reporting JSON"
            )
            .map_err(crate::edit_output::io)?;
        }
        let mut listings = Vec::new();
        for root in &selection.roots {
            let (listing, error) = crate::bat_cli::capture_listing(root, &list_flags)?;
            complete &= error.is_none();
            listings.push(root_list(root, &listing, error.as_ref()));
        }
        value["root_lists"] = json!(listings);
        value["complete"] = json!(complete);
    }
    crate::diagnostics::point(
        "query.stats",
        json!({"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"complete":complete,"roots":selection.roots.len()}),
    );
    let serialization = crate::diagnostics::Span::new(
        "serialize",
        json!({"role":"stdout","operation":command,"multi_root":true}),
    );
    let text = if human {
        let mut text = summary(&value["cross_reference"]);
        for (i, root) in value["root_traces"]
            .as_array()
            .expect("root reports")
            .iter()
            .enumerate()
        {
            let fields = if command == "list" && has("--full-info") {
                vec!["errors", "warnings", "usages"]
            } else {
                vec!["errors", "warnings"]
            };
            for field in fields {
                for diagnostic in root["trace"][field].as_array().into_iter().flatten() {
                    text += &format!("R{} {field}: {diagnostic}\n", i + 1);
                }
            }
        }
        if command == "list" && has("--sha256") {
            for path in selection.roots.iter().cloned().chain(
                value["cross_reference"]["assets"]
                    .as_array()
                    .expect("assets")
                    .iter()
                    .filter(|a| a["bytes"].is_u64())
                    .filter_map(|a| a["identity_path_hex"].as_str().and_then(decode_hex_path)),
            ) {
                match crate::bat_cli::hash_file(&path) {
                    Ok((digest, _)) => {
                        text += &format!(
                            "SHA256 {digest} {}\n",
                            crate::value::display_bytes(path.as_os_str().as_encoded_bytes())
                        )
                    }
                    Err(error) => {
                        text += &format!("[hash unavailable] {}\n", error_json(&error));
                        presentation_error.get_or_insert(error);
                    }
                }
            }
        }
        if command == "list" && has("--timing") {
            text += &format!(
                "Spent {:.2} seconds on producing this listing\n",
                started.elapsed().as_secs_f64()
            );
        }
        text
    } else {
        serde_json::to_string(&value).map_err(|e| Error::new("serialization", e.to_string()))?
    };
    let mut bytes = 0;
    let delivered = {
        let mut out = crate::failure_receipt::stdout_counted(&mut bytes);
        writeln!(out, "{text}")
            .and_then(|_| out.flush())
            .map_err(crate::edit_output::io)
    };
    crate::diagnostics::stdout_delivery(bytes, delivered.is_ok());
    serialization.result(delivered)?;
    if let Some(error) = presentation_error {
        return Err(error);
    }
    if !complete {
        return Err(crate::strict::multi_root_refusal());
    }
    Ok(())
}
fn dependency_value(
    command: &str,
    roots: &[PathBuf],
    reports: &[Option<TraceReport>],
    errors: &[Option<Error>],
) -> Result<Json> {
    let xref = cross_reference(
        roots,
        &reports.iter().map(Option::as_ref).collect::<Vec<_>>(),
        &errors.iter().map(Option::as_ref).collect::<Vec<_>>(),
    )?;
    Ok(
        json!({"schema":"oxblend.multi-root/v1","operation":command,"complete":xref["complete"],
        "root_traces":roots.iter().enumerate().map(|(i,p)|root_json(p,reports[i].as_ref(),errors[i].as_ref())).collect::<Vec<_>>(),"cross_reference":xref}),
    )
}
fn root_list(root: &Path, listing: &Json, error: Option<&Error>) -> Json {
    json!({"input_path_hex":hex(root.as_os_str().as_encoded_bytes()),"input_path":root.to_str(),"list":listing,"error":error.map(error_json)})
}
/// Same traversal/projector as the CLI, over one caller-owned arena. Per-root
/// independent read failures remain rows; source guards retain their owner and
/// refuse the whole observation, never masquerading as missing dependencies.
#[allow(clippy::too_many_arguments)]
fn dependency_in_store(
    roots: &[PathBuf],
    spelling: &Path,
    command: &str,
    options: TraceOptions,
    full_info: bool,
    store: &mut crate::session::InputStore,
    batch: &mut crate::session::CaptureBatch,
    op: &mut crate::session::OpCtx,
    captured: bool,
) -> crate::session::BodyResult<(Json, Vec<Json>, Option<Error>)> {
    use crate::session::BodyErr;
    let mut flags = vec![OsString::from("--json")];
    if full_info {
        flags.push("--full-info".into());
    }
    if options.low_memory {
        flags.push("--low-memory".into());
    }
    if let [root] = roots {
        if command == "list" {
            return crate::bat_cli::listing_in_store(
                spelling, root, store, batch, op, &flags, captured,
            );
        }
        let (report, _) = trace::trace_in_store(
            store,
            batch,
            root,
            options,
            op,
            trace::LibraryPolicy::ReportMissing,
            captured,
        )?;
        return Ok((report.json(), Vec::new(), None));
    }
    let mut reports = Vec::new();
    let mut errors = Vec::new();
    for root in roots {
        let mut span = crate::diagnostics::Span::new(
            "trace.root",
            json!({"input_path_hex":hex(root.as_os_str().as_encoded_bytes())}),
        );
        let observed = trace::trace_in_store(
            store,
            batch,
            root,
            options,
            op,
            trace::LibraryPolicy::ReportMissing,
            captured,
        );
        span.record_result(
            &observed
                .as_ref()
                .map(|_| ())
                .map_err(|e| e.as_error().clone()),
        );
        match observed {
            Ok((report, _)) => {
                reports.push(Some(report));
                errors.push(None);
            }
            Err(guard @ BodyErr::Guard { .. }) => return Err(guard),
            Err(BodyErr::Independent(error)) => {
                reports.push(None);
                errors.push(Some(error));
            }
        }
    }
    let mut value = dependency_value(command, roots, &reports, &errors)?;
    let mut notices = Vec::new();
    if command == "list" {
        let mut listings = Vec::new();
        for root in roots {
            let (listing, warnings, error) = match crate::bat_cli::listing_in_store(
                root, root, store, batch, op, &flags, captured,
            ) {
                Ok(result) => result,
                Err(guard @ BodyErr::Guard { .. }) => return Err(guard),
                Err(BodyErr::Independent(error)) => (Json::Null, Vec::new(), Some(error)),
            };
            if error.is_some() {
                value["complete"] = json!(false);
            }
            notices.extend(warnings);
            listings.push(root_list(root, &listing, error.as_ref()));
        }
        value["root_lists"] = json!(listings);
    }
    Ok((value, notices, None))
}
/// Path-based in-process dependency operation. Roots are captured independently
/// into one arena so one unreadable input does not hide other root facts.
pub fn dependency_operation(
    inputs: &[PathBuf],
    command: &str,
    options: TraceOptions,
    full_info: bool,
) -> crate::session::Operation<(Json, Vec<Json>, Option<Error>)> {
    use crate::session::{CaptureBatch, GuardPolicy, InputStore, OpCtx};
    let result = (|| {
        if !matches!(command, "trace" | "list") {
            return Err(Error::new(
                "usage",
                "dependency operation requires trace or list",
            ));
        }
        let cwd = std::env::current_dir().map_err(crate::edit_output::io)?;
        Ok(OpCtx::new(
            GuardPolicy::Legacy,
            None,
            cwd,
            std::env::temp_dir(),
        ))
    })();
    let mut op = match result {
        Ok(op) => op,
        Err(error) => return crate::session::Operation::new(Err(error), Vec::new()),
    };
    let roots = select_roots(inputs.iter().map(|p| InputStore::resolve(&op, p)));
    let result = if roots.is_empty() {
        Err(Error::new(
            "usage",
            "at least one selected root is required",
        ))
    } else {
        let mut store = InputStore::default();
        let mut batch = CaptureBatch::default();
        dependency_in_store(
            &roots, &inputs[0], command, options, full_info, &mut store, &mut batch, &mut op, false,
        )
        .map_err(Error::from)
    };
    op.finish(result)
}
/// Captured dependency operation: no root/library is reopened into another
/// arena, and every selected root participates in completion source checks.
pub fn dependency_session(
    session: &mut crate::Session,
    command: &str,
    options: TraceOptions,
    full_info: bool,
    spelling: Option<&Path>,
) -> crate::session::Operation<(Json, Vec<Json>, Option<Error>)> {
    let mut op = session.operation_context(None);
    let roots = session
        .root_paths()
        .into_iter()
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    let body = (|| {
        if !matches!(command, "trace" | "list") {
            return Err(Error::new("usage", "dependency operation requires trace or list").into());
        }
        for (id, root) in session.roots.iter().zip(&roots) {
            op.record_touch(*id);
            session.store.check_reuse(*id, &mut op).map_err(|error| {
                crate::session::BodyErr::Guard {
                    source: root.clone(),
                    error,
                }
            })?;
        }
        if options.low_memory {
            return Err(Error::new("usage", "captured Session dependency operation cannot honour low_memory; use a path operation").into());
        }
        let mut batch = crate::session::CaptureBatch::with_memory(session.memory());
        dependency_in_store(
            &roots,
            spelling.unwrap_or(&roots[0]),
            command,
            options,
            full_info,
            &mut session.store,
            &mut batch,
            &mut op,
            true,
        )
    })();
    let checks = session.store.check_all_roots(&session.roots, &mut op);
    let result = crate::session::join_body(body, &checks, &mut op, "session.dependencies", None)
        .map_err(Error::from);
    op.finish(result)
}
fn label(v: &Json) -> String {
    v["identity_path"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("hex:{}", v["identity_path_hex"].as_str().unwrap_or("")))
}
fn summary(x: &Json) -> String {
    let mut s = String::from("ROOTS\n");
    let roots = x["roots"].as_array().expect("roots");
    let assets = x["assets"].as_array().expect("assets");
    for (i, r) in roots.iter().enumerate() {
        s += &format!(
            "R{}={}\n",
            i + 1,
            r["input_path"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("hex:{}", r["input_path_hex"].as_str().unwrap_or("")))
        );
    }
    s += "SHARED ASSET";
    for i in 0..roots.len() {
        s += &format!("\tR{}", i + 1);
    }
    s += "\tBYTES\n";
    for a in assets.iter().filter(|a| a["shared"] == true) {
        s += &label(a);
        for r in roots {
            let uses = a["used_by"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|u| u["root_id"] == r["root_id"])
                .collect::<Vec<_>>();
            let direct = uses.iter().any(|u| u["direct"] == true);
            let linked = uses.iter().any(|u| u["direct"] == false);
            let marker = match (direct, linked) {
                (true, true) => "D+L",
                (true, false) => "D",
                (false, true) => "L",
                _ => "-",
            };
            s += &format!(
                "\t{marker}{}",
                if !uses.is_empty() && a["exists"] == false {
                    "!"
                } else {
                    ""
                }
            );
        }
        s += &format!(
            "\t{}\n",
            a["bytes"]
                .as_u64()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "unknown".into())
        );
        for u in a["used_by"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|u| u["direct"] == false)
        {
            s += &format!("  chain {}\n", u);
        }
    }
    s += "ROOT\tSHARED\tUNIQUE\tMISSING FILES\tMISSING IDs\tKNOWN BYTES\tSTATUS\n";
    for (i, r) in roots.iter().enumerate() {
        let count = |key: &str| r[key].as_array().unwrap().len();
        let bytes: u64 = assets
            .iter()
            .filter(|a| {
                a["used_by"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|u| u["root_id"] == r["root_id"])
            })
            .filter_map(|a| a["bytes"].as_u64())
            .sum();
        let ids = r["missing_links"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "missing_id")
            .count();
        s += &format!(
            "R{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            i + 1,
            count("shared"),
            count("unique"),
            count("missing_files"),
            ids,
            bytes,
            if r["complete"] == true {
                "COMPLETE"
            } else {
                "INCOMPLETE"
            }
        );
        for aid in r["missing_files"].as_array().unwrap() {
            if let Some(a) = assets.iter().find(|a| a["asset_id"] == *aid) {
                s += &format!("  missing file: {}\n", label(a));
            }
        }
        for e in r["missing_links"].as_array().unwrap() {
            s += &format!("  missing link: {e}\n");
        }
        if !r["error"].is_null() {
            s += &format!("  error: {}\n", r["error"]);
        }
        for site in r["observation_errors"].as_array().into_iter().flatten() {
            s += &format!("  observation error: {site}\n");
        }
    }
    s += &format!(
        "TOTAL: {}\nD=direct, L=via library, !=absent; byte sizes are observations, not render validation.\n",
        x["totals"]
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn udim_member_rows_empty_expansion_and_nonregular_size() {
        let work = crate::test_scratch::scratch_dir(&std::env::temp_dir());
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/trace-owner/pointer-kinds/id-only-data.blend");
        let roots = [work.join("one.blend"), work.join("two.blend")];
        let mut reports = Vec::new();
        for root in &roots {
            let mut report = trace::trace(&source, TraceOptions::default()).unwrap();
            report.usages.truncate(1);
            report.complete = true;
            let usage = &mut report.usages[0];
            usage.owner_file = root.as_os_str().as_encoded_bytes().to_vec();
            usage.library_chain = vec![usage.owner_file.clone()];
            usage.asset_path = b"//tile.<UDIM>.exr".to_vec();
            usage.is_sequence = true;
            usage.is_udim = true;
            reports.push(report);
        }
        for tile in ["tile.1001.exr", "tile.1003.exr"] {
            std::fs::write(work.join(tile), b"tile").unwrap();
        }
        let x = cross_reference(
            &roots,
            &reports.iter().map(Some).collect::<Vec<_>>(),
            &[None, None],
        )
        .unwrap();
        assert_eq!(x["totals"]["assets"], 2);
        assert_eq!(x["totals"]["shared_assets"], 2);
        assert_eq!(x["totals"]["bytes"], 8);
        for tile in ["tile.1001.exr", "tile.1003.exr"] {
            std::fs::remove_file(work.join(tile)).unwrap();
        }
        let x = cross_reference(
            &roots,
            &reports.iter().map(Some).collect::<Vec<_>>(),
            &[None, None],
        )
        .unwrap();
        assert_eq!(x["totals"]["assets"], 1);
        assert_eq!(x["assets"][0]["unexpanded"], true);
        assert_eq!(x["assets"][0]["exists"], false);
        assert_eq!(x["complete"], false);
        assert_eq!(x["totals"]["unknown_size_assets"], 1);
        let directory = work.join("directory");
        std::fs::create_dir(&directory).unwrap();
        for report in &mut reports {
            let u = &mut report.usages[0];
            u.asset_path = b"//directory".to_vec();
            u.is_sequence = false;
            u.is_udim = false;
        }
        let x = cross_reference(
            &roots,
            &reports.iter().map(Some).collect::<Vec<_>>(),
            &[None, None],
        )
        .unwrap();
        assert_eq!(x["assets"][0]["exists"], true);
        assert!(x["assets"][0]["bytes"].is_null());
        assert_eq!(x["totals"]["unknown_size_assets"], 1);
        std::fs::remove_dir_all(work).unwrap();
    }
}
