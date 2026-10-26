use crate::edit::{
    Durability, EditOptions, EditReport, PathRewrite, prepared, rewrite_opened_cleared,
};
use crate::edit_output::{AtomicOutput, create_directories, io_at, sync_end};
use crate::session::{
    BodyErr, BodyResult, CaptureBatch, GuardPolicy, InputStore, OpCtx, Operation, Session,
};
use crate::trace::{
    FailureKind, FieldReference, PointerMode, TraceMode, TraceOptions, TraceReport, Usage,
    trace_in_store_for_pack as trace_in_store,
};
use crate::value::{PathStep, Value, hex};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::{File, Metadata};
use std::io::Write;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct PackOptions {
    pub project_root: PathBuf,
    pub durability: Durability,

    pub keep_hierarchy: bool,
    pub exclude: Vec<String>,
    pub noop: bool,
    pub relative_only: bool,
    pub compress: bool,
    /// Opt in to publishing only provable members, explicitly INCOMPLETE.
    pub permissive: bool,
    /// Pack retention policy: fast defaults to selective raw windows for CLI packs;
    /// Sessions retain captures, while bounded re-reads verified working bodies.
    pub memory: crate::session::MemoryMode,
}
impl PackOptions {
    /// Programmatic entry (Session, bindings): names the project root
    /// explicitly. The CLI adapter keeps its input-parent defaulting.
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            ..Default::default()
        }
    }
}
/// Canonical single-root selection refusal, shared by the CLI argument
/// gate and the engine prepare gate so both surfaces stay byte-identical.
pub fn missing_pack_input_error() -> Error {
    Error::new("pack_input", "selected input missing or a directory")
}
/// Canonical post-report refusal for an unreadable selected pack root.
/// Shared by the CLI delivery check and the native binding so the kind,
/// message and exit stay identical on both sides.
pub fn missing_pack_root_error() -> Error {
    Error::new(
        "pack_input",
        "selected input missing or not a regular file; independent root diagnostics retained; no publication",
    )
}
/// Canonical missing-project refusal. The displayed path is the resolved
/// absolute selection the CLI gates on; the binding resolves the same way.
pub fn missing_pack_project_error(project: &Path) -> Error {
    Error::new(
        "pack_project",
        format!(
            "Project directory {} does not exist; nothing was written",
            project.display()
        ),
    )
}
/// CLI/native shared project selection: absolute selection, shared default
/// projection, existence gate with the canonical pack_project error, then
/// the historical file-parent handling. Both surfaces resolve identically so
/// the refused display path and the stored root stay byte-identical.
pub fn resolve_project_root(
    explicit: Option<PathBuf>,
    inputs: &[PathBuf],
    sequence: bool,
) -> Result<PathBuf> {
    let project = match explicit {
        Some(path) => {
            crate::trace::lexical(&std::path::absolute(path).map_err(crate::edit_output::io)?)
        }
        None => default_project(inputs, sequence)?,
    };
    if !project.exists() {
        return Err(missing_pack_project_error(&project));
    }
    Ok(if project.is_file() {
        project.parent().unwrap().to_path_buf()
    } else {
        project
    })
}
/// CLI/native shared post-report gate: a selected root that failed with a
/// retained per-root error and is not a readable file is a hard pack_input
/// refusal on both surfaces, with the report retained beside it.
pub fn missing_root_error(report: &PackReport) -> Option<Error> {
    report
        .inputs
        .iter()
        .zip(&report.root_errors)
        .any(|(path, error)| error.is_some() && !path.is_file())
        .then(missing_pack_root_error)
}
/// CLI/native default project selection, pure over absolute selected roots.
/// Explicit project existence/file handling remains at the owning boundary.
pub fn default_project(inputs: &[PathBuf], sequence: bool) -> Result<PathBuf> {
    let normalized = inputs
        .iter()
        .map(|path| crate::trace::lexical(path))
        .collect::<Vec<_>>();
    let inputs = normalized.as_slice();
    let first = inputs
        .first()
        .ok_or_else(|| Error::new("usage", "pack requires a selected blend input"))?;
    let mut project = first
        .parent()
        .ok_or_else(|| Error::new("usage", "input has no parent"))?
        .to_path_buf();
    for input in &inputs[1..] {
        let parent = input
            .parent()
            .ok_or_else(|| Error::new("usage", "input has no parent"))?;
        while !parent.starts_with(&project) {
            project = project
                .parent()
                .ok_or_else(|| Error::new("usage", "no common project"))?
                .to_path_buf();
        }
    }
    if sequence && inputs.len() > 1 && project.parent().is_none() {
        return Err(Error::new(
            "usage",
            "common project is filesystem root; provide -p",
        ));
    }
    Ok(project)
}
#[derive(Debug)]
#[non_exhaustive]
pub struct PackFile {
    pub input: PathBuf,
    pub output: PathBuf,
    pub output_written: bool,
    pub durability_completed: bool,
    pub rewritten: bool,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct PackReport {
    pub memory: crate::session::MemoryMode,
    pub input: PathBuf,
    pub inputs: Vec<PathBuf>,
    pub root_traces: Vec<TraceReport>,
    /// Aligned hard root failures; these roots have no trace JSON, not a fake success.
    pub root_errors: Vec<Option<Error>>,
    pub root_trace_available: Vec<bool>,
    pub cross_reference: Option<Json>,
    pub permissive: bool,
    pub skipped_items: Vec<Json>,
    pub keep_hierarchy: bool,
    pub directory: PathBuf,
    pub noop: bool,
    pub compressed: bool,
    pub archive: bool,
    pub archive_written: bool,
    pub files: Vec<PackFile>,
    pub rewrites: Vec<EditReport>,
    pub missing: Vec<Usage>,
    /// Absolute declarations intentionally left unchanged by --relative-only.
    pub skipped_absolute_paths: Vec<Usage>,
    /// Unselected copied-library paths retained verbatim, never write admission.
    pub unrelocated_unselected_paths: Vec<Usage>,
    /// Guarded pointer-clear plan; publication is reported by each owning file.
    pub library_pointer_clears: Vec<Json>,
    pub info_path: Option<PathBuf>,
    pub info_written: bool,
    pub info_durability_completed: bool,
    pub durability: Durability,
    pub durability_completed: bool,
    pub trace: TraceReport,
    pub errors: Vec<Error>,
    pub planning_complete: bool,
    pub directory_created: bool,
    pub complete: bool,
}
impl PackReport {
    fn missing_links(&self) -> Vec<Json> {
        self.inputs
            .iter()
            .zip(&self.root_traces)
            .flat_map(|(root, trace)| {
                trace
                    .errors
                    .iter()
                    .filter(|e| e.is_external_missing())
                    .map(move |e| {
                        let mut value = e.json();
                        value["root_file_hex"] = json!(raw_hex(root));
                        value["root_file"] = json!(root.to_str());
                        value["owner_file"] = json!(std::str::from_utf8(&e.owner_file).ok());
                        value["library_path"] = json!(
                            e.library_path
                                .as_ref()
                                .and_then(|p| std::str::from_utf8(p).ok())
                        );
                        value
                    })
            })
            .collect()
    }
    fn excluded_count(&self) -> usize {
        self.skipped_items
            .iter()
            .filter(|item| item["status"] == "excluded" && item.get("usage").is_some())
            .count()
    }
    pub fn json(&self) -> Json {
        let links = self.missing_links();
        let missing_ids = links.iter().filter(|v| v["kind"] == "missing_id").count();
        let missing_libraries = links
            .iter()
            .filter(|v| v["kind"] == "missing_library")
            .count();
        let mut value = crate::value::with_text(
            json!({"schema":1,"operation":"pack","input_path_hex":raw_hex(&self.input),"legacy_input_trace_scope":"first-root-only","input_paths_hex":self.inputs.iter().map(|p|raw_hex(p)).collect::<Vec<_>>(),"root_traces":self.inputs.iter().zip(&self.root_traces).map(|(p,t)|json!({"input_path_hex":raw_hex(p),"trace":t.json()})).collect::<Vec<_>>(),"keep_hierarchy":self.keep_hierarchy,"directory_path_hex":raw_hex(&self.directory),"planning_complete":self.planning_complete,"directory_created":self.directory_created,"noop":self.noop,"compression_requested":self.compressed,"archive":self.archive,"archive_written":self.archive_written,"archive_entry_paths_not_filesystem_paths":self.archive,"complete":self.complete,"durability":self.durability.as_str(),"durability_mechanism":self.durability.mechanism(),"syncfs_shared_filesystem":self.durability == Durability::End,"durability_completed":self.durability_completed,"trace":self.trace.json(),"files":self.files.iter().map(|f|json!({"input_path_hex":raw_hex(&f.input),"output_path_hex":raw_hex(&f.output),"output_written":f.output_written,"durability_completed":f.durability_completed,"rewritten":f.rewritten})).collect::<Vec<_>>(),"rewrites":self.rewrites.iter().map(EditReport::json).collect::<Vec<_>>(),"library_pointer_clears":self.library_pointer_clears,"missing_links":links,"missing_link_counts":{"missing_id":missing_ids,"missing_library":missing_libraries},"skipped_absolute_paths":self.skipped_absolute_paths.iter().map(Usage::json).collect::<Vec<_>>(),"missing":self.missing.iter().map(|u| { let mut v=u.json(); v["absolute_path_hex"]=json!(raw_hex(&crate::trace::resolve_usage(u))); v }).collect::<Vec<_>>(),"warnings":self.unrelocated_unselected_paths.iter().map(|u| { let r=u.path_full_field.as_ref().or(u.path_dir_field.as_ref()).expect("unrelocated path has field provenance"); json!({"kind":"unrelocated_unselected_path","owner_file_hex":hex(&u.owner_file),"owner_file":std::str::from_utf8(&u.owner_file).ok(),"block_address":u.block_address.to_string(),"id_name_hex":hex(&u.block_name),"id_name":std::str::from_utf8(&u.block_name).ok(),"field_path_hex":r.path.iter().map(|p|hex(p)).collect::<Vec<_>>(),"field_path":r.path.iter().map(|p|std::str::from_utf8(p).ok()).collect::<Option<Vec<_>>>(),"asset_path_hex":hex(&u.asset_path),"asset_path":std::str::from_utf8(&u.asset_path).ok()}) }).collect::<Vec<_>>(),"pack_info":self.info_path.as_ref().map(|p|json!({"output_path_hex":raw_hex(p),"output_written":self.info_written,"durability_completed":self.info_durability_completed})),"errors":self.errors.iter().map(|e|json!({"kind":e.kind,"message":e.message,"catalogue_entry":format!("error-{}",e.kind),"hint":partial_hint(e.kind)})).collect::<Vec<_>>()}),
        );
        if self.memory == crate::session::MemoryMode::Bounded {
            value["memory"] = json!(self.memory.as_str());
        }
        if let Some(cross_reference) = &self.cross_reference {
            value["cross_reference"] = cross_reference.clone();
        }
        if self.root_errors.iter().any(Option::is_some) {
            value["root_traces"] = json!(
                self.inputs
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let error = self.root_errors[i].as_ref();
                        crate::multi_root::root_json(
                            p,
                            self.root_trace_available[i].then_some(&self.root_traces[i]),
                            error,
                        )
                    })
                    .collect::<Vec<_>>()
            );
            if !self.root_trace_available[0] {
                value["trace"] = Json::Null;
            }
        }
        value["native_validation"] = json!("not_performed");
        value["mode"] = json!(if self.permissive {
            "permissive"
        } else {
            "strict"
        });
        value["status"] = json!(if self.complete {
            "COMPLETE"
        } else {
            "INCOMPLETE"
        });
        value["skipped_items"] = json!(self.skipped_items);
        value["excluded_count"] = json!(self.excluded_count());
        value["self_contained"] = json!(self.complete && self.excluded_count() == 0);
        value
    }
}
fn partial_hint(kind: &str) -> String {
    let cause = match kind {
        "missing_id" => {
            "Restore the intended library revision or repair its saved ID link in Blender."
        }
        "missing_asset" | "missing_library" => {
            "Restore or repath the declared dependency using its owner file."
        }
        _ => {
            "Inspect the typed owner/field/pointer site and compare native behavior on a copy; never guess or zero arbitrary pointers."
        }
    };
    format!(
        "{cause} Use --permissive only for an INCOMPLETE partial package, not a farm-ready project."
    )
}
fn raw_hex(path: &Path) -> String {
    hex(path.as_os_str().as_bytes())
}
fn bytes_path(v: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_vec(v.to_vec()))
}
fn unsafe_path(message: &str) -> Error {
    Error::new("pack_path", message)
}
fn refuse_output_symlink(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Err(unsafe_path("symlink pack destination refused")),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_at("stat destination symlink", path, e)),
    }
}
fn signature(m: &Metadata) -> (u64, u64, u64, i64, i64) {
    (m.dev(), m.ino(), m.len(), m.mtime(), m.mtime_nsec())
}
fn destination_empty(root: &Path) -> Result<bool> {
    refuse_output_symlink(root)?;
    match std::fs::symlink_metadata(root) {
        Ok(m)
            if m.is_dir()
                && std::fs::read_dir(root)
                    .map_err(|e| io_at("read destination directory", root, e))?
                    .next()
                    .is_none() =>
        {
            Ok(false)
        }
        Ok(_) => Err(Error::new(
            "pack_destination",
            "pack directory must be new or empty; never merge/resume",
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(io_at("stat destination directory", root, e)),
    }
}
fn strip_root(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        if let Component::Normal(n) = component {
            let bytes = n.as_bytes();
            if result.as_os_str().is_empty()
                && bytes.len() == 2
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
            {
                result.push(OsString::from_vec(bytes[..1].to_vec()));
            } else {
                result.push(n);
            }
        }
    }
    result
}
fn relative(owner: &Path, asset: &Path) -> Result<Vec<u8>> {
    let parent = owner
        .parent()
        .ok_or_else(|| unsafe_path("final owner has no parent"))?;
    let from = parent.components().collect::<Vec<_>>();
    let to = asset.components().collect::<Vec<_>>();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return Err(unsafe_path("final paths do not share a local root"));
    }
    let mut p = PathBuf::new();
    for _ in common..from.len() {
        p.push("..");
    }
    for c in &to[common..] {
        p.push(c.as_os_str());
    }
    let mut result = b"//".to_vec();
    result.extend(p.as_os_str().as_bytes());
    Ok(result)
}

fn characters(data: &[u8]) -> Vec<u32> {
    match std::str::from_utf8(data) {
        Ok(s) => s.chars().map(u32::from).collect(),
        Err(_) => data.iter().map(|b| u32::from(*b)).collect(),
    }
}
fn token(pattern: &[u32], at: usize, value: u32) -> (bool, usize) {
    let current = pattern[at];
    if current == u32::from(b'?') {
        return (true, at + 1);
    }
    if current != u32::from(b'[') {
        return (current == value, at + 1);
    }
    let mut start = at + 1;
    let negative = pattern.get(start) == Some(&u32::from(b'!'));
    if negative {
        start += 1;
    }
    let mut end = start;
    if pattern.get(end) == Some(&u32::from(b']')) {
        end += 1;
    }
    while end < pattern.len() && pattern[end] != u32::from(b']') {
        end += 1;
    }
    if end == pattern.len() || end == start {
        return (current == value, at + 1);
    }
    let (mut i, mut matched) = (start, false);
    while i < end {
        if i + 2 < end && pattern[i + 1] == u32::from(b'-') {
            matched |= pattern[i] <= value && value <= pattern[i + 2];
            i += 3;
        } else {
            matched |= pattern[i] == value;
            i += 1;
        }
    }
    (matched != negative, end + 1)
}
fn star_match(pattern: &[u8], name: &[u8]) -> bool {
    let pattern = characters(pattern);
    let name = characters(name);
    let (mut p, mut n, mut star, mut retry) = (0, 0, None, 0);
    while n < name.len() {
        if p < pattern.len() && pattern[p] == u32::from(b'*') {
            star = Some(p);
            p += 1;
            retry = n;
            continue;
        }
        if p < pattern.len() {
            let (matches, next) = token(&pattern, p, name[n]);
            if matches {
                p = next;
                n += 1;
                continue;
            }
        }
        if let Some(s) = star {
            retry += 1;
            n = retry;
            p = s + 1;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == u32::from(b'*') {
        p += 1;
    }
    p == pattern.len()
}

fn excluded(path: &Path, globs: &[String]) -> bool {
    let parts = path
        .components()
        .map(|c| c.as_os_str().as_bytes())
        .collect::<Vec<_>>();
    globs.iter().any(|glob| {
        let p = Path::new(glob)
            .components()
            .map(|c| c.as_os_str().as_bytes())
            .collect::<Vec<_>>();
        !p.is_empty()
            && p.len() <= parts.len()
            && (!Path::new(glob).is_absolute() || p.len() == parts.len())
            && p.iter()
                .rev()
                .zip(parts.iter().rev())
                .all(|(a, b)| star_match(a, b))
    })
}
fn scope_exclusion(usage: &Usage, options: &PackOptions) -> Option<String> {
    if options.relative_only && !usage.asset_path.starts_with(b"//") {
        return Some("--relative-only: absolute declaration".to_owned());
    }
    let source = crate::trace::resolve_usage(usage);
    options
        .exclude
        .iter()
        .find(|pattern| excluded(&source, std::slice::from_ref(pattern)))
        .map(|pattern| format!("--exclude: {pattern}"))
}
fn magic(pattern: &[u8]) -> bool {
    pattern.iter().any(|b| matches!(b, b'*' | b'?' | b'['))
}
fn directory_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = std::fs::read_dir(directory)
        .map_err(|e| io_at("read asset directory", directory, e))?
        .map(|e| {
            e.map(|x| x.path())
                .map_err(|e| io_at("read asset entry", directory, e))
        })
        .collect::<Result<Vec<_>>>()?;
    entries.sort();
    for p in entries {
        let m = std::fs::symlink_metadata(&p).map_err(|e| io_at("stat sequence member", &p, e))?;
        if m.is_dir() {
            directory_files(&p, files)?;
        } else if p.is_file() {
            files.push(p);
        } else if m.file_type().is_symlink() && p.is_dir() {
            continue;
        } else {
            return Err(unsafe_path("nonregular sequence member"));
        }
    }
    Ok(())
}
fn asset_metadata(path: &Path) -> Result<Metadata> {
    path.metadata().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::new(
                "missing_asset",
                format!("required asset missing: '{}'", path.display()),
            )
        } else {
            io_at("stat asset", path, e)
        }
    })
}
fn udim_pattern(name: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut position = None;
    let mut i = 0;
    while i + 6 <= name.len() {
        if &name[i..i + 6] == b"<UDIM>" {
            position = Some(i);
            break;
        }
        i += 1;
    }
    position.map(|at| (name[..at].to_vec(), name[at + 6..].to_vec()))
}
fn stem_pattern(name: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let stem = match name.iter().rposition(|b| *b == b'.') {
        Some(dot) if dot != 0 => &name[..dot],
        _ => name,
    };
    let extension = &name[stem.len()..];
    let digits = stem.iter().rev().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    Some((stem[..stem.len() - digits].to_vec(), extension.to_vec()))
}
fn star_pattern(name: &[u8]) -> bool {
    name.contains(&b'*')
}
fn wildcard_members(path: &Path) -> Result<Vec<PathBuf>> {
    let parts = path
        .components()
        .filter(|c| !matches!(c, Component::RootDir))
        .map(|c| c.as_os_str().to_os_string())
        .collect::<Vec<_>>();
    let root = PathBuf::from(if path.is_absolute() { "/" } else { "." });
    let mut pending = vec![(root, 0, BTreeSet::<PathBuf>::new())];
    let mut files = BTreeSet::new();
    while let Some((base, index, mut ancestors)) = pending.pop() {
        if index == parts.len() {
            let mut found = Vec::new();
            push_member(
                base.parent().unwrap_or(Path::new(".")),
                base.file_name().unwrap_or_default().as_bytes(),
                &mut found,
            )?;
            files.extend(found);
            continue;
        }
        let pattern = parts[index].as_bytes();
        if pattern == b"**" && index + 1 < parts.len() {
            let canonical = match base.canonicalize() {
                Ok(p) => p,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(io_at("canonicalize wildcard base", &base, e)),
            };
            if !ancestors.insert(canonical) {
                continue;
            }
            pending.push((base.clone(), index + 1, BTreeSet::new()));
            for name in scan_directory(&base)? {
                if !name.as_bytes().starts_with(b".") && base.join(&name).is_dir() {
                    pending.push((base.join(name), index, ancestors.clone()));
                }
            }
        } else if magic(pattern) && !base.join(&parts[index]).exists() {
            for name in scan_directory(&base)? {
                if name.as_bytes().starts_with(b".") && !pattern.starts_with(b".") {
                    continue;
                }
                if star_match(pattern, name.as_bytes()) {
                    let target = base.join(name);
                    if index + 1 == parts.len() || target.is_dir() {
                        pending.push((target, index + 1, ancestors.clone()));
                    }
                }
            }
        } else {
            let target = base.join(&parts[index]);
            if target.exists() {
                pending.push((target, index + 1, ancestors));
            }
        }
    }
    if files.is_empty() {
        return Err(Error::new(
            "missing_asset",
            format!("required asset missing: '{}'", path.display()),
        ));
    }
    Ok(files.into_iter().collect())
}
fn scan_directory(directory: &Path) -> Result<Vec<OsString>> {
    let mut names = Vec::new();
    let entries = match std::fs::read_dir(directory) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(names),
        Err(e) => return Err(io_at("read sequence directory", directory, e)),
    };
    for entry in entries {
        let entry = entry.map_err(|e| io_at("read sequence entry", directory, e))?;
        names.push(entry.file_name());
    }
    names.sort();
    Ok(names)
}
fn push_member(directory: &Path, name: &[u8], files: &mut Vec<PathBuf>) -> Result<()> {
    let candidate = directory.join(bytes_path(name));
    match std::fs::symlink_metadata(&candidate) {
        Ok(m) if m.is_dir() => {
            return Err(unsafe_path("glob matched a directory/nonregular member"));
        }
        Ok(m) if m.file_type().is_symlink() && candidate.is_dir() => {
            return Err(unsafe_path("glob matched a directory/nonregular member"));
        }
        Ok(_) => {}
        Err(e) => return Err(io_at("stat member symlink", &candidate, e)),
    }
    match std::fs::metadata(&candidate) {
        Ok(m) if m.is_file() => files.push(candidate),
        Ok(_) => {
            return Err(unsafe_path("glob matched a directory/nonregular member"));
        }
        Err(e) => return Err(io_at("stat expanded member", &candidate, e)),
    }
    Ok(())
}
pub(crate) fn expand(path: &Path, sequence: bool) -> Result<Vec<PathBuf>> {
    let raw = path.as_os_str().as_bytes();
    let is_directory = path.is_dir();
    if !sequence {
        if is_directory || raw.ends_with(b"/") {
            return Err(Error::new("pack_path", "nonregular asset"));
        }
        return match asset_metadata(path) {
            Ok(m) if m.is_file() => Ok(vec![path.to_path_buf()]),
            Ok(_) => Err(unsafe_path("pack source is not regular file")),
            Err(e) => Err(e),
        };
    }
    if is_directory || raw.ends_with(b"/") {
        let directory = if is_directory {
            path.to_path_buf()
        } else {
            bytes_path(&raw[..raw.len() - 1])
        };
        let mut files = Vec::new();
        directory_files(&directory, &mut files)?;
        files.sort();
        if files.is_empty() {
            return Err(Error::new(
                "missing_asset",
                format!("required asset missing: '{}'", path.display()),
            ));
        }
        return Ok(files);
    }
    let parent = path.parent().unwrap_or(Path::new("/")).to_path_buf();
    let filename = path
        .file_name()
        .map(|n| n.as_bytes().to_vec())
        .unwrap_or_default();
    if filename.is_empty() {
        return Err(Error::new(
            "missing_asset",
            format!("required asset missing: '{}'", path.display()),
        ));
    }
    if let Some((prefix, suffix)) = udim_pattern(&filename) {
        let mut pattern = prefix;
        pattern.push(b'*');
        pattern.extend(suffix);
        return wildcard_members(&parent.join(bytes_path(&pattern)));
    }
    let concrete = asset_metadata(path).ok().filter(|m| m.is_file()).is_some();
    if star_pattern(raw) && !concrete {
        return wildcard_members(path);
    }
    match stem_pattern(&filename) {
        Some((prefix, suffix)) if concrete => {
            let mut pattern = prefix;
            pattern.push(b'*');
            pattern.extend(suffix);
            wildcard_members(&parent.join(bytes_path(&pattern)))
        }
        _ => match asset_metadata(path) {
            Ok(m) if m.is_file() => Ok(vec![path.to_path_buf()]),
            Ok(_) => Err(unsafe_path("pack source is not regular file")),
            Err(e) => Err(e),
        },
    }
}
fn member_base(path: &Path) -> PathBuf {
    let mut literal_prefix = PathBuf::new();
    for part in path.components() {
        if magic(part.as_os_str().as_bytes()) && !literal_prefix.join(part.as_os_str()).exists() {
            return literal_prefix;
        }
        literal_prefix.push(part.as_os_str());
    }
    if path.is_dir() {
        return path.to_path_buf();
    }
    if let Some(name) = path.file_name()
        && (name.as_bytes().windows(6).any(|v| v == b"<UDIM>") || magic(name.as_bytes()))
    {
        return path.parent().unwrap_or(Path::new("/")).to_path_buf();
    }
    path.parent().unwrap_or(Path::new("/")).to_path_buf()
}
struct Asset {
    members: Vec<PathBuf>,
    sequence: bool,
    outside: bool,
    target: PathBuf,
}
struct Planned {
    input: PathBuf,
    output: PathBuf,
    edits: Vec<PathRewrite>,
    clears: Vec<crate::library_compat::PointerClear>,
    metadata: Metadata,
    blend: bool,
}
fn origin_block(reader: &BlendFile, usage: &Usage, reference: &FieldReference) -> Result<usize> {
    let anchor = reader
        .blocks()
        .partition_point(|b| b.data_offset <= usage.block_offset)
        .checked_sub(1)
        .ok_or_else(|| Error::new("pack_origin", "trace origin has no record"))?;
    let b = &reader.blocks()[anchor];
    if b.old_address != usage.block_address || usage.block_offset - b.data_offset >= b.size {
        return Err(Error::new(
            "pack_origin",
            "trace origin differs from checked record",
        ));
    }
    reader
        .owner_index()?
        .resolve_origin(reader, anchor, reference.block_address)
}
fn selector(
    reader: &BlendFile,
    usage: &Usage,
    reference: &FieldReference,
    replacement: Vec<u8>,
) -> Result<PathRewrite> {
    let index = origin_block(reader, usage, reference)?;
    let steps = reference
        .path
        .iter()
        .cloned()
        .map(PathStep::Field)
        .collect::<Vec<_>>();
    let location = reader.locate_field(index, reference.element, &steps)?;
    if location.offset != reference.offset
        || location.field.size != reference.size
        || location.field.pointer != reference.pointer
        || reader.schema().types[location.field.type_index] != reference.dna_type
        || location.field.name != reference.name
    {
        return Err(Error::new(
            "pack_origin",
            "trace field origin differs from per-file checked SDNA",
        ));
    }
    if location.field.pointer {
        return Err(Error::new(
            "pointer_write",
            "required pack relocation cannot rewrite a pointer field",
        ));
    }
    let old = match reader.get(index, reference.element, &steps, true)? {
        Value::Bytes(v) => v,
        _ => return Err(Error::new("pack_origin", "path origin is not bytes")),
    };
    Ok(PathRewrite {
        block_index: index,
        element: reference.element,
        steps,
        expected_path: old,
        replacement_path: replacement,
    })
}
fn add_file(
    files: &mut BTreeMap<PathBuf, Planned>,
    input: &Path,
    output: &Path,
    blend: bool,
) -> Result<()> {
    let file = File::open(input).map_err(|e| io_at("open planned input", input, e))?;
    let metadata = file
        .metadata()
        .map_err(|e| io_at("fstat planned input", input, e))?;
    if !metadata.is_file() {
        return Err(unsafe_path("pack source is not regular file"));
    }
    if let Some(prior) = files.get_mut(output) {
        if prior.input != input {
            return Err(Error::new(
                "pack_collision",
                "different sources map to the same final destination",
            ));
        }
        prior.blend |= blend;
        return Ok(());
    }
    files.insert(
        output.to_path_buf(),
        Planned {
            input: input.to_path_buf(),
            output: output.to_path_buf(),
            edits: Vec::new(),
            clears: Vec::new(),
            metadata,
            blend,
        },
    );
    Ok(())
}
fn coalesce(
    edits: &mut Vec<PathRewrite>,
    keys: &mut BTreeMap<(usize, usize, usize, usize), usize>,
    field: &FieldReference,
    edit: PathRewrite,
) -> Result<()> {
    let key = (edit.block_index, field.element, field.offset, field.size);
    if let Some(&index) = keys.get(&key) {
        let prior = &edits[index];
        if prior.expected_path != edit.expected_path
            || prior.replacement_path != edit.replacement_path
        {
            return Err(Error::new(
                "conflicting_edits",
                "shared owning field has conflicting rewrite requests",
            ));
        }
    } else {
        keys.insert(key, edits.len());
        edits.push(edit);
    }
    Ok(())
}

fn overview(
    report: &PackReport,
    files: &BTreeMap<PathBuf, Planned>,
    directory: &Path,
    unique_assets: usize,
) -> Result<Vec<u8>> {
    let mut roots = Vec::new();
    for root in &report.inputs {
        let planned = files
            .values()
            .find(|f| f.input == *root)
            .ok_or_else(|| unsafe_path("pack overview misses a planned root blend"))?;
        let shown = planned
            .output
            .strip_prefix(directory)
            .map_err(|_| unsafe_path("root output escapes pack directory"))?;
        if shown.as_os_str().is_empty() {
            return Err(unsafe_path("root output is the pack directory"));
        }
        roots.push(shown.to_path_buf());
    }
    let mut text = b"This is an Oxblend pack (Blender Asset Tracer compatible).\nComplete concerns the selected serialized dependency closure only.\nBlender open, evaluation and rendering have NOT been verified by this command.\nBefore farm delivery, open the isolated package in compatible Blender with original project sources unavailable; check missing data and run required render checks.\nA repaired or byte-edited input needs its own native validation.\n".to_vec();
    if roots.len() == 1 {
        text.extend(b"Start by opening the following blend file:\n    ");
        text.extend(roots[0].as_os_str().as_bytes());
        text.push(b'\n');
    } else {
        text.extend(format!("This pack contains {} blend files:\n", roots.len()).as_bytes());
        for root in &roots {
            text.extend(b"    ");
            text.extend(root.as_os_str().as_bytes());
            text.push(b'\n');
        }
        text.extend(format!("Total unique assets: {unique_assets}\n").as_bytes());
    }
    text.extend(
        format!(
            "Complete (requested scope): {}\nSelf-contained: {}\nExcluded items: {}\n",
            report.errors.is_empty() && report.missing.is_empty(),
            report.errors.is_empty() && report.missing.is_empty() && report.excluded_count() == 0,
            report.excluded_count()
        )
        .as_bytes(),
    );
    let missing = report
        .missing
        .iter()
        .map(crate::trace::resolve_usage)
        .collect::<BTreeSet<_>>();
    let links = report.missing_links();
    // Publication is not finished yet, so report.complete is not authoritative
    // here. These are the known closure failures at overview planning time.
    if !missing.is_empty() || !report.errors.is_empty() {
        text.extend(b"INCOMPLETE: do not use this pack as a complete project.\n");
    }
    if !missing.is_empty() {
        text.extend(format!("Missing files: {}\n", missing.len()).as_bytes());
        for path in missing {
            let shown = path
                .to_str()
                .map(|s| format!("{s:?}"))
                .unwrap_or_else(|| format!("hex:{}", raw_hex(&path)));
            text.extend(format!("Missing file: {shown}\n").as_bytes());
        }
    }
    let ids = links.iter().filter(|v| v["kind"] == "missing_id").count();
    let libraries = links
        .iter()
        .filter(|v| v["kind"] == "missing_library")
        .count();
    if libraries != 0 {
        text.extend(format!("Missing libraries: {libraries}\n").as_bytes());
    }
    if ids != 0 {
        text.extend(format!("Missing linked IDs: {ids}\n").as_bytes());
    }
    for link in links {
        text.extend(
            format!(
                "{}: {link}\n",
                link["kind"].as_str().unwrap_or("missing_link")
            )
            .as_bytes(),
        );
    }
    for item in &report.skipped_items {
        text.extend(format!("Skipped item: {item}\n").as_bytes());
    }
    for usage in &report.skipped_absolute_paths {
        text.extend(
            format!(
                "Unrelocated absolute path (--relative-only): {}\n",
                usage.json()
            )
            .as_bytes(),
        );
    }
    if !report.library_pointer_clears.is_empty() {
        text.extend(
            format!(
                "Library ID pointers cleared: {}\n",
                report.library_pointer_clears.len()
            )
            .as_bytes(),
        );
        for clear in &report.library_pointer_clears {
            text.extend(format!("library_null_pointer: {clear}\n").as_bytes());
        }
    }
    if !report.unrelocated_unselected_paths.is_empty() {
        text.extend(
            format!(
                "Unrelocated unselected paths: {}\n",
                report.unrelocated_unselected_paths.len()
            )
            .as_bytes(),
        );
    }
    for trace in &report.root_traces {
        for usage in trace.usages.iter().filter(|u| u.camera_type.is_some()) {
            text.extend(
                format!(
                    "Camera shader declaration (required even when active:false): {}\n",
                    usage.json()
                )
                .as_bytes(),
            );
        }
        for warning in &trace.warnings {
            if matches!(
                warning,
                crate::trace::TraceWarning::EmbeddedCameraFilepath { .. }
            ) {
                text.extend(
                    format!(
                        "Historical camera filepath (not a dependency): {}\n",
                        warning.json()
                    )
                    .as_bytes(),
                );
            }
        }
    }
    String::from_utf8(text.clone()).map_err(|_| unsafe_path("overview path is not UTF8"))?;
    Ok(text)
}

/// A valid RFC 3986 scheme followed by an authority component
/// (`scheme://...`) marks the target as a URI rather than a local path.
fn uri_scheme(candidate: &[u8]) -> Option<&str> {
    let colon = candidate.iter().position(|b| *b == b':')?;
    let (scheme, rest) = candidate.split_at(colon);
    let mut bytes = scheme.iter().copied();
    if !bytes.next().is_some_and(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    if !bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')) {
        return None;
    }
    if !rest.starts_with(b"://") {
        return None;
    }
    // Only the ASCII scheme must be UTF-8, not the URI authority/path bytes.
    std::str::from_utf8(scheme).ok()
}
/// Every pack boundary refuses URI targets with the same named cause and
/// hint, before the target is interpreted as a local path (even for a noop);
/// known transports keep their own named refusal.
pub(crate) fn uri_target_error(command: &str, target: &Path) -> Option<Error> {
    let scheme = uri_scheme(target.as_os_str().as_bytes())?;
    let message = if matches!(
        scheme.to_ascii_lowercase().as_str(),
        "s3" | "http" | "https"
    ) {
        "Shaman/S3 transport is not implemented; use a filesystem target".to_string()
    } else {
        format!("{command} target is a URI (scheme \"{scheme}\"); use a filesystem target")
    };
    Some(Error::new("usage", message))
}
/// Historical Result-only entry point. Routes through the operation core
/// with no observer, so pure library calls stay silent; use
/// [`pack_operation`] to retrieve the auxiliary diagnostics.
pub fn pack(inputs: &[PathBuf], new_directory: &Path, options: &PackOptions) -> Result<PackReport> {
    pack_operation(inputs, new_directory, options).into_result()
}
/// Operation entry point: the guarded result plus every operation-local
/// diagnostic, on both success and failure.
pub fn pack_operation(
    inputs: &[PathBuf],
    new_directory: &Path,
    options: &PackOptions,
) -> Operation<PackReport> {
    pack_operation_with_observer(inputs, new_directory, options, None)
}
/// One owner for pack error presentation on both path and captured entries.
/// Context is attached to the original Error, preserving its source evidence.
fn contextualize_pack(result: Result<PackReport>, destination: &Path) -> Result<PackReport> {
    let mut result =
        result.map_err(|error| crate::edit_output::context(error, "pack", destination));
    if let Ok(report) = &mut result {
        for error in &mut report.errors {
            *error = crate::edit_output::context(error.clone(), "pack", destination);
        }
    }
    result
}
/// Canonical strict refusal for an incomplete pack report, shared by the CLI
/// delivery check and the native binding. Prose lives once in
/// [`crate::strict`]; this stays a thin typed-report wrapper.
pub fn strict_pack_refusal(report: &PackReport) -> Error {
    crate::strict::pack_refusal(
        report.noop,
        report.directory_created
            || report.archive_written
            || report.info_written
            || report.files.iter().any(|f| f.output_written),
    )
}
/// Native-binding entry to the same canonical refusal, read over the
/// serialized report the facade hands back. Delegates to the single
/// [`crate::strict`] owner; absent flags fail loudly as usage.
pub fn strict_pack_refusal_json(report: &serde_json::Value) -> Result<Error> {
    crate::strict::pack_refusal_json(report)
}
fn pack_operation_with_observer(
    inputs: &[PathBuf],
    new_directory: &Path,
    options: &PackOptions,
    observer: Option<fn(&crate::Diagnostic)>,
) -> Operation<PackReport> {
    crate::tracing::initialize();

    let mut span = crate::diagnostics::Span::new(
        "pack.operation",
        if crate::diagnostics::enabled() {
            json!({"roots":inputs.len(),"dry_run":options.noop,"path":new_directory.to_string_lossy()})
        } else {
            serde_json::Value::Null
        },
    );
    // Sampled once at this operation's entry: later spill, backing and
    // workspace creation stage under these roots instead of re-sampling.
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            let result = Err(crate::edit_output::io(error));
            span.record_result(&result);
            return Operation::new(result, Vec::new());
        }
    };
    let temp_root = std::env::temp_dir();
    let mut op = OpCtx::new(GuardPolicy::Legacy, observer, cwd, temp_root);
    let mut result = contextualize_pack(
        pack_with_op(inputs, new_directory, options, &mut op),
        new_directory,
    );
    if let Ok(report) = &mut result {
        span.data(json!({"complete":report.complete,"planning_complete":report.planning_complete,"missing":report.missing.len(),"files":report.files.len(),"archive_written":report.archive_written,"published_files":report.files.iter().filter(|f| f.output_written).count()}));
        if crate::diagnostics::enabled() {
            crate::diagnostics::report(&report.json());
            if options.noop {
                crate::diagnostics::point(
                    "decision",
                    json!({"item":new_directory.to_string_lossy(),"kind":"dry_run","reason":"explicit dry-run selects planning only; no execution/publication"}),
                );
            }
            if !report.complete {
                crate::diagnostics::point(
                    "decision",
                    json!({"item":new_directory.to_string_lossy(),"kind":if report.planning_complete {"partial"} else {"refused"},"reason":"product report is incomplete; inspect structured errors and skipped_items","planning_complete":report.planning_complete}),
                );
            }
        }
        if !report.complete {
            span.end(if report.planning_complete {
                "partial"
            } else {
                "refused"
            });
            return op.finish(result);
        }
    }
    span.record_result(&result);
    op.finish(result)
}
/// CLI entry point: owns the explicit observer that renders the historical
/// stderr envelope synchronously at emission.
pub fn pack_report_cli(
    inputs: &[PathBuf],
    new_directory: &Path,
    options: &PackOptions,
) -> Result<PackReport> {
    pack_operation_with_observer(
        inputs,
        new_directory,
        options,
        Some(crate::session::cli_observer),
    )
    .into_result()
}
/// Session entry point: one guarded pack over the retained captured arena.
/// Uses all selected Session roots; completion checks latch changed sources
/// and preserve published effects.
pub fn pack_session(
    session: &mut Session,
    new_directory: &Path,
    options: &PackOptions,
) -> Operation<PackReport> {
    crate::tracing::initialize();
    let mut span =
        crate::diagnostics::Span::new("session.pack", json!({"memory":options.memory.as_str()}));
    // Design §4.1: project/output paths resolve against the cwd sampled at
    // this pack call's entry, not the Session open-time cwd. The captured
    // root/library identities stay frozen; only the destination context is
    // fresh. Temp stays Session-frozen per the memory contract.
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            let result = Err(crate::edit_output::io(error));
            span.record_result(&result);
            return Operation::new(result, Vec::new());
        }
    };
    let mut op = OpCtx::new(
        GuardPolicy::Session,
        None,
        cwd,
        session.defaults.temp_root.clone(),
    );
    let result = (|| {
        let roots = session.root_paths().into_iter().map(Path::to_path_buf).collect::<Vec<_>>();
        let root = (roots.len() == 1).then(|| roots[0].clone());
        // Check every selected capture before any planning or effects.
        for id in &session.roots {
            op.record_touch(*id);
            session.store.check_reuse(*id, &mut op)?;
        }
        // Resolve the destination against the pack-call cwd, then run the
        // shared planner/emitter over the retained arena. Setup errors
        // route through the finalizer like body errors: a source change
        // racing setup is latched and wins with body-error retention.
        let setup = match pack_prepare(&roots, new_directory, options, &op) {
            Ok(setup) => setup,
            Err(error) => {
                let checks = session.store.check_all_roots(&session.roots, &mut op);
                return crate::session::join_body::<PackReport>(
                    Err(BodyErr::Independent(error)),
                    &checks,
                    &mut op,
                    "session.pack",
                    root.clone(),
                );
            }
        };
        if session.memory() != options.memory {
            return Err(Error::new("usage", format!("pack memory mode {} differs from Session mode {}; retention is fixed at Session open", options.memory.as_str(), session.memory().as_str())).into());
        }
        let mut batch = CaptureBatch::with_memory(options.memory);
        match pack_in_store(&mut session.store, &mut batch, &mut op, &setup, options) {
            Ok(mut report) => {
                // Route every completion failure through the preserved
                // report: publication and effect facts survive, every body
                // error stays, and only the outcome flips to incomplete.
                // check_all visits each touched source once. Do not dedupe
                // generic kind/message pairs: different sources can fail with
                // the same message, and body failures must remain as well.
                let failures = session.store.check_all_roots(&session.roots, &mut op);
                for (source, error) in &failures {
                    report.errors.push(crate::edit_output::context(
                        error.clone(),
                        "pack completion source",
                        source,
                    ));
                }
                if !failures.is_empty() {
                    report.complete = false;
                }
                Ok(report)
            }
            Err(body_error) => {
                // No report or effects exist yet: join the body refusal
                // with the completion checks so a competing change wins
                // and the body error is retained as a SecondaryError.
                let checks = session.store.check_all_roots(&session.roots, &mut op);
                crate::session::join_body::<PackReport>(
                    Err(body_error),
                    &checks,
                    &mut op,
                    "session.pack",
                    root,
                )
            }
        }
    })()
    .map_err(Error::from);
    let result = contextualize_pack(result, new_directory);
    // Honest domain outcome: a returned incomplete report ends partial
    // (refused when nothing was planned) with its error/publication
    // summary attached, never ok. A post-effect source change keeps its
    // error-kind propagation point while the report stays Ok. Error
    // results keep the existing kind-to-outcome mapping; process outcome
    // stays unknown.
    match &result {
        Ok(report) if !report.complete => {
            span.data(json!({"complete":false,"planning_complete":report.planning_complete,"missing":report.missing.len(),"files":report.files.iter().filter(|f| f.output_written).count(),"archive_written":report.archive_written,"errors":report.errors.iter().map(|e|e.kind).collect::<Vec<_>>() }));
            for error in &report.errors {
                if error.kind == "input_changed" {
                    span.record_error(error.kind, &error.message);
                }
            }
            span.end(if report.planning_complete {
                "partial"
            } else {
                "refused"
            });
        }
        _ => span.record_result(&result),
    }
    op.finish(result)
}
/// Split from the historical pack_inner at its PackInputs admission
/// boundary: destination/project checks and precedence first, then the
/// shared planner/emitter over the supplied store.
struct PreparedPack {
    input: PathBuf,
    roots: Vec<PathBuf>,
    project: PathBuf,
    directory: PathBuf,
    archive: bool,
}
/// Legacy/shared core: argument preparation against the frozen operation
/// cwd, then the supplied-store pack. Pure of policy; completion checks
/// belong to the entry points.
fn pack_with_op(
    inputs: &[PathBuf],
    new_directory: &Path,
    options: &PackOptions,
    op: &mut OpCtx,
) -> Result<PackReport> {
    let prepared = pack_prepare(inputs, new_directory, options, op)?;
    let mut store = InputStore::with_memory(options.memory);
    let mut batch = CaptureBatch::with_memory(options.memory);
    // L17 Phase 2: the ephemeral CLI pack store routes raw blends to
    // signature-bound selective windows. Compressed blends keep full
    // decoded bodies; Sessions (pack_session) keep full captures (F4).
    // Bounded mode is unchanged: selective applies to fast only.
    if options.memory == crate::session::MemoryMode::Fast {
        store.selective_raw_ephemeral = true;
        batch.selective_raw = true;
    }
    pack_in_store(&mut store, &mut batch, op, &prepared, options).map_err(Error::from)
}
fn pack_prepare(
    inputs: &[PathBuf],
    new_directory: &Path,
    options: &PackOptions,
    op: &OpCtx,
) -> Result<PreparedPack> {
    if let Some(error) = uri_target_error("pack", new_directory) {
        return Err(error);
    }
    if inputs.is_empty() {
        return Err(unsafe_path("pack needs at least one selected root"));
    }
    let resolve = |path: &Path| -> PathBuf {
        // Frozen operation cwd, not a re-sampled process directory.
        crate::trace::lexical(&if path.is_absolute() {
            path.to_path_buf()
        } else {
            op.cwd.join(path)
        })
    };
    let mut selected = BTreeSet::new();
    let mut roots = Vec::new();
    for input in inputs {
        let input = resolve(input);
        let inserted = selected.insert(input.clone());
        if crate::diagnostics::enabled() {
            crate::diagnostics::point(
                "decision",
                json!({"item":input.to_string_lossy(),"kind":if inserted {"root_selected"} else {"duplicate_root_skipped"},"reason":if inserted {"explicit selected root"} else {"same absolute input already selected"}}),
            );
        }
        if inserted {
            roots.push(input);
        }
    }
    let input = roots[0].clone();
    let archive = new_directory
        .extension()
        .is_some_and(|s| s.as_encoded_bytes().eq_ignore_ascii_case(b"zip"));
    if archive && options.compress {
        return Err(Error::new(
            "usage",
            "ZIP rejects --compress; ZIP entries use stored/deflate",
        ));
    }
    if roots.len() == 1 && !roots[0].is_file() {
        return Err(missing_pack_input_error());
    }
    let project = resolve(&options.project_root);
    let directory = resolve(new_directory);
    Ok(PreparedPack {
        input,
        roots,
        project,
        directory,
        archive,
    })
}
/// Planning has not published anything. Independent failures remain the
/// historical incomplete planning report; a nested source guard must reach
/// the enclosing finalizer with its origin intact, never as a report copy.
fn finish_planning<T>(
    planned: BodyResult<T>,
    mut phase: crate::diagnostics::Span,
    errors: &mut Vec<Error>,
) -> BodyResult<Option<T>> {
    match planned {
        Ok(files) => {
            phase.end("ok");
            Ok(Some(files))
        }
        Err(error) => {
            phase.record_result(&Err::<(), _>(error.as_error().clone()));
            match error {
                guard @ BodyErr::Guard { .. } => Err(guard),
                BodyErr::Independent(error) => {
                    errors.push(error);
                    Ok(None)
                }
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn pack_in_store(
    store: &mut InputStore,
    batch: &mut CaptureBatch,
    op: &mut OpCtx,
    setup: &PreparedPack,
    options: &PackOptions,
) -> BodyResult<PackReport> {
    // L5 plan-outcome guard: the pack.plan phase owns the whole store body.
    // The normal planning path consumes the span via `finish_planning`;
    // any earlier exit (trace refusal, permissive gate, execution return)
    // still terminates the phase here instead of dropping it silently:
    // errors keep their kind mapping, returned reports end ok/refused
    // from their actual completeness facts.
    let mut planning = Some(crate::diagnostics::Span::new(
        "pack.plan",
        json!({"memory":options.memory.as_str()}),
    ));
    let result = pack_in_store_body(store, batch, op, setup, options, &mut planning);
    if let Some(mut phase) = planning {
        match &result {
            Err(_) => {
                phase.record_result(&result.as_ref().map_err(|error| error.as_error().clone()))
            }
            Ok(report) => {
                if phase.active() {
                    phase.data(json!({"complete":report.complete,"planning_complete":report.planning_complete}));
                }
                phase.end(if report.complete { "ok" } else { "refused" });
            }
        }
    }
    result
}

#[allow(clippy::too_many_lines)]
fn pack_in_store_body(
    store: &mut InputStore,
    batch: &mut CaptureBatch,
    op: &mut OpCtx,
    setup: &PreparedPack,
    options: &PackOptions,
    planning: &mut Option<crate::diagnostics::Span>,
) -> BodyResult<PackReport> {
    let PreparedPack {
        input,
        roots,
        project,
        directory,
        archive,
    } = setup;
    let input = input.clone();
    let roots = roots.clone();
    let project = project.clone();
    let directory = directory.clone();
    let archive = *archive;
    let selected: BTreeSet<PathBuf> = roots.iter().cloned().collect();
    // Preserve fast mode's upfront root scheduling. Bounded mode declines
    // blend prefetch at the CaptureBatch owner and captures on demand.
    batch.prime(roots.len())?;
    for root in &roots {
        batch.prefetch(root, true)?;
    }
    let mut root_traces = Vec::new();
    let mut root_errors = Vec::new();
    for root in &roots {
        let traced = trace_in_store(
            store,
            batch,
            root,
            TraceOptions {
                mode: TraceMode::Recursive,
                pointers: PointerMode::Strict,
                low_memory: false,
            },
            op,
            crate::trace::LibraryPolicy::ReportMissing,
            true,
        );
        match traced {
            Ok((trace, _)) => {
                root_traces.push(trace);
                root_errors.push(None);
            }
            Err(error @ BodyErr::Guard { .. }) => return Err(error),
            Err(error) if roots.len() == 1 => return Err(error),
            Err(BodyErr::Independent(error)) => {
                // Internal aligned placeholder only. JSON exposes trace:null
                // and the actual typed hard error; no fabricated discovery facts.
                root_traces.push(TraceReport {
                    usages: Vec::new(),
                    errors: Vec::new(),
                    warnings: Vec::new(),
                    complete: false,
                });
                root_errors.push(Some(error));
            }
        }
    }
    let root_trace_available = root_errors.iter().map(Option::is_none).collect::<Vec<_>>();
    // Derive the union from these exact captured readers. Reopening roots here
    // would evade the Session source guards and could mix different saves.
    let cross_reference = if roots.len() > 1 {
        Some(crate::multi_root::cross_reference(
            &roots,
            &root_traces
                .iter()
                .enumerate()
                .map(|(i, t)| root_trace_available[i].then_some(t))
                .collect::<Vec<_>>(),
            &root_errors.iter().map(Option::as_ref).collect::<Vec<_>>(),
        )?)
    } else {
        None
    };
    let usages = root_traces
        .iter()
        .flat_map(|t| t.usages.iter().cloned())
        .collect::<Vec<_>>();
    let first = &root_traces[0];
    let traced = TraceReport {
        usages: first.usages.clone(),
        errors: first.errors.clone(),
        warnings: first.warnings.clone(),
        complete: first.complete,
    };
    let mut report = PackReport {
        memory: options.memory,
        input: input.clone(),
        inputs: roots,
        root_traces,
        root_errors,
        root_trace_available,
        cross_reference,
        permissive: options.permissive,
        skipped_items: Vec::new(),
        keep_hierarchy: options.keep_hierarchy,
        directory: directory.clone(),
        noop: options.noop,
        compressed: options.compress,
        archive,
        archive_written: false,
        files: Vec::new(),
        rewrites: Vec::new(),
        missing: Vec::new(),
        skipped_absolute_paths: Vec::new(),
        unrelocated_unselected_paths: Vec::new(),
        library_pointer_clears: Vec::new(),
        info_path: None,
        info_written: false,
        info_durability_completed: false,
        durability: options.durability,
        durability_completed: false,
        trace: traced,
        errors: Vec::new(),
        planning_complete: false,
        directory_created: false,
        complete: false,
    };
    for (i, error) in report.root_errors.iter().enumerate() {
        if let Some(error) = error {
            crate::diagnostics::point(
                "decision",
                json!({"item":raw_hex(&report.inputs[i]),"kind":error.kind,"reason":error.message}),
            );
            report.errors.push(error.clone());
        }
    }
    if report.root_errors.iter().any(Option::is_some)
        || report.cross_reference.as_ref().is_some_and(|x| {
            x["roots"]
                .as_array()
                .is_some_and(|roots| roots.iter().any(|r| !r["error"].is_null()))
        })
    {
        report.errors.push(Error::new(
            "incomplete_trace",
            "hard root failures; all independent diagnostics retained; no planning or publication",
        ));
        return Ok(report);
    }
    for trace in &report.root_traces {
        for failure in &trace.errors {
            let mut item = failure.json();
            item["reason"] = json!(failure.message);
            let scoped_out = failure.is_external_missing()
                && failure.library_path.as_ref().is_some_and(|path| {
                    let matching = usages
                        .iter()
                        .filter(|u| crate::trace::resolve_usage(u) == bytes_path(path))
                        .collect::<Vec<_>>();
                    !matching.is_empty()
                        && matching
                            .iter()
                            .all(|u| scope_exclusion(u, options).is_some())
                });
            if failure.is_external_missing() {
                item["status"] = json!(if scoped_out { "excluded" } else { "missing" });
                item["missing"] = json!(true);
                item["in_scope"] = json!(!scoped_out);
            }
            report.skipped_items.push(item.clone());
            // Trace-failure decision at the pack planning owner; the item
            // identity reconciles with the independently parsed trace report.
            if crate::diagnostics::enabled() {
                let (kind, reason) = (
                    item["kind"].as_str().unwrap_or("unknown"),
                    failure.message.as_str(),
                );
                let decision_kind = if item["status"] == "excluded" {
                    "excluded"
                } else {
                    kind
                };
                crate::diagnostics::point(
                    "decision",
                    json!({"item":item["library_path"].as_str().or(item["root_file"].as_str()).unwrap_or(kind),"kind":decision_kind,"reason":reason}),
                );
            }
            if !scoped_out
                && (!matches!(failure.kind, FailureKind::MissingLibrary) || !options.permissive)
            {
                report
                    .errors
                    .push(Error::new(failure.kind_str(), failure.message.clone()));
            }
        }
    }
    if !options.permissive && !report.errors.is_empty() {
        report.errors.push(Error::new("incomplete_trace",
            "unprovable dependency sites; no outputs; inspect causes and use --permissive only for an INCOMPLETE partial package"));
        return Ok(report);
    }
    let mut unique_assets = 0;
    let planned = (|| -> BodyResult<BTreeMap<PathBuf, Planned>> {
        if archive {
            refuse_output_symlink(&directory)?;
            if directory.exists() {
                return Err(
                    Error::new("output_exists", "ZIP output must be new; never replace").into(),
                );
            }
        } else {
            destination_empty(&directory)?;
        }
        if !project.is_dir() {
            return Err(unsafe_path("project root is not a directory").into());
        }
        for root in &report.inputs {
            root.strip_prefix(&project)
                .map_err(|_| unsafe_path("every root blend must be inside project root"))?;
        }
        let root_output = |root: &Path| -> Result<PathBuf> {
            Ok(directory.join(if options.keep_hierarchy {
                strip_root(root)
            } else {
                root.strip_prefix(&project)
                    .map_err(|_| unsafe_path("root outside project"))?
                    .to_path_buf()
            }))
        };
        let mut assets: BTreeMap<PathBuf, Asset> = BTreeMap::new();
        let mut missing_indexes = BTreeSet::new();
        for (index, usage) in usages.iter().enumerate() {
            let data = &usage.asset_path;
            let source = crate::trace::resolve_usage(usage);
            if let Some(reason) = scope_exclusion(usage, options) {
                report.skipped_items.push(json!({"kind":"excluded","status":"excluded","reason":reason,"missing":!usage.exists(),"usage":usage.json()}));
                // Exclusion decision at the planning owner with reconciled identity.
                if crate::diagnostics::enabled() {
                    crate::diagnostics::point(
                        "decision",
                        json!({"item":crate::trace::resolve_usage(usage).to_string_lossy(),"kind":"excluded","reason":reason}),
                    );
                }
                if options.relative_only && !data.starts_with(b"//") {
                    report.skipped_absolute_paths.push(usage.clone());
                }
                missing_indexes.insert(index);
                continue;
            }
            if report.root_traces.iter().any(|t| {
                t.errors.iter().any(|e| {
                    matches!(e.kind, FailureKind::LibraryReader(_))
                        && e.library_path
                            .as_deref()
                            .is_some_and(|p| bytes_path(p) == source)
                })
            }) {
                // Existing but unreadable library bytes cannot prove members.
                // Keep the original declaration and the exact unknown cause.
                missing_indexes.insert(index);
                continue;
            }
            if crate::trace::uncertain_path(data) {
                if options.permissive {
                    report.skipped_items.push(json!({"kind":"path_resolution_unverified","status":"unknown","reason":"foreign-root loader I/O cannot be safely relocated","usage":usage.json()}));
                    if crate::diagnostics::enabled() {
                        crate::diagnostics::point(
                            "decision",
                            json!({"item":crate::trace::resolve_usage(usage).to_string_lossy(),"kind":"path_resolution_unverified","reason":"foreign-root loader I/O cannot be safely relocated; retained as unknown"}),
                        );
                    }
                    report.errors.push(Error::new(
                        "path_resolution_unverified",
                        "foreign-root dependency left unknown; no guessed relocation",
                    ));
                    missing_indexes.insert(index);
                    continue;
                }
                return Err(Error::new(
                    "path_resolution_unverified",
                    "Foreign Windows-root loader I/O cannot be safely relocated; no outputs; inspect path_resolution_unverified trace warnings",
                ).into());
            }
            if let Some(reason) = &usage.pattern_unknown {
                report.skipped_items.push(json!({"kind":"pattern_members_unknown","status":"unknown","reason":reason,"usage":usage.json()}));
                report
                    .errors
                    .push(Error::new("pattern_members_unknown", reason.clone()));
            }
            for missing in usage.missing_members() {
                report.skipped_items.push(json!({"kind":"missing_asset","status":"missing","reason":"saved pattern member missing","usage":missing.json()}));
                report.missing.push(missing);
            }
            let members = match usage.available_members() {
                Ok(members) => members,
                Err(e) if e.kind == "missing_asset" => {
                    report.skipped_items.push(json!({"kind":if usage.block_code==b"LI" {"missing_library"} else {"missing_asset"},"status":"missing","reason":e.message,"usage":usage.json()}));
                    if crate::diagnostics::enabled() {
                        crate::diagnostics::point(
                            "decision",
                            serde_json::json!({"item":source.to_string_lossy(),"kind":if usage.block_code==b"LI" {"missing_library"} else {"missing_asset"},"reason":e.message}),
                        );
                    }
                    report.missing.push(usage.clone());
                    missing_indexes.insert(index);
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            if members.is_empty() {
                missing_indexes.insert(index);
                continue;
            }
            let outside = !data.starts_with(b"//") || !members[0].starts_with(&project);
            let asset = assets.entry(source).or_insert_with(|| Asset {
                members: Vec::new(),
                sequence: false,
                outside: false,
                target: PathBuf::new(),
            });

            asset.members.extend(members);
            asset.members.sort();
            asset.members.dedup();
            asset.sequence |= usage.is_sequence;
            asset.outside |= outside;
        }
        for (source, asset) in &mut assets {
            if asset.sequence && directory.starts_with(member_base(source)) {
                return Err(Error::new(
                    "pack_destination",
                    "destination would alter a required source sequence/directory namespace",
                )
                .into());
            }
            asset.target = if selected.contains(source) {
                root_output(source)?
            } else if options.keep_hierarchy {
                directory.join(strip_root(source))
            } else if asset.outside {
                directory.join("_outside_project").join(strip_root(source))
            } else {
                directory.join(
                    source
                        .strip_prefix(&project)
                        .map_err(|_| unsafe_path("asset mapping escaped project"))?,
                )
            };
        }
        let mut owners = selected.clone();
        for owner in usages.iter().map(|u| bytes_path(&u.owner_file)) {
            if assets.contains_key(&owner) {
                owners.insert(owner);
            }
        }
        unique_assets = assets
            .keys()
            .chain(owners.iter())
            .collect::<BTreeSet<_>>()
            .len();
        let mut files = BTreeMap::new();
        for owner in &owners {
            let output = if selected.contains(owner) {
                root_output(owner)?
            } else if options.keep_hierarchy {
                directory.join(strip_root(owner))
            } else if let Some(a) = assets.get(owner) {
                a.target.clone()
            } else if let Ok(p) = owner.strip_prefix(&project) {
                directory.join(p)
            } else {
                directory.join("_outside_project").join(strip_root(owner))
            };
            add_file(&mut files, owner, &output, true)?;
        }
        for (source, asset) in &assets {
            let base = if asset.sequence {
                member_base(source)
            } else {
                source.clone()
            };
            let target_base = if base == *source {
                asset.target.clone()
            } else if options.keep_hierarchy {
                directory.join(strip_root(&base))
            } else if asset.outside {
                directory.join("_outside_project").join(strip_root(&base))
            } else {
                directory.join(
                    base.strip_prefix(&project)
                        .map_err(|_| unsafe_path("sequence base outside project"))?,
                )
            };
            for member in &asset.members {
                let output = if selected.contains(member) {
                    root_output(member)?
                } else if !asset.sequence && member == source {
                    asset.target.clone()
                } else {
                    target_base.join(
                        member
                            .strip_prefix(&base)
                            .map_err(|_| unsafe_path("sequence member escaped base"))?,
                    )
                };
                add_file(&mut files, member, &output, owners.contains(member))?;
            }
        }

        let copies = files
            .values()
            .filter(|f| f.blend)
            .map(|f| (f.input.clone(), f.output.clone()))
            .collect::<Vec<_>>();
        for (owner, final_owner) in &copies {
            // Only the arena identity plus owned clears/edits/keys cross
            // the local PerFile normalization walk; the selected-path
            // reader borrow is released before that walk and re-acquired
            // for the final preparation.
            let owner_id = store.id_for_path(owner)?;
            let (mut clears, unknown) = {
                let reader = store.reader(owner_id)?;
                crate::library_compat::clears(reader)
            };
            let has_unknown = !unknown.is_empty();
            for mut item in unknown {
                item["owner_file_hex"] = json!(raw_hex(owner));
                item["owner_file"] = json!(owner.to_str());
                report.errors.push(Error::new(
                    "incomplete_trace",
                    item["reason"].as_str().unwrap(),
                ));
                report.skipped_items.push(item);
            }
            if has_unknown && !options.permissive {
                return Err(Error::new(
                    "incomplete_trace",
                    "unqualified copied-library ID slots; no outputs",
                )
                .into());
            }
            if selected.contains(owner) && !clears.is_empty() {
                if !options.permissive {
                    return Err(Error::new(
                        "dangling_pointer",
                        "known invalid ID reference in a selected main file; no outputs",
                    )
                    .into());
                }
                for clear in &clears {
                    report.skipped_items.push(json!({"kind":"dangling_pointer","status":"unknown","owner_file_hex":raw_hex(owner),"block_index":clear.block,"pointer_value_decimal":clear.address.to_string(),"reason":"narrow copied-library repair does not authorize clearing a selected main-file pointer"}));
                }
                report.errors.push(Error::new(
                    "dangling_pointer",
                    "unqualified main-file pointer repair skipped",
                ));
                clears.clear();
            }
            for clear in &clears {
                let (block_address, width) = {
                    let reader = store.reader(owner_id)?;
                    (
                        reader.blocks()[clear.block].old_address,
                        reader.header().pointer_size,
                    )
                };
                report.library_pointer_clears.push(json!({"kind":"library_null_pointer","owner_file_hex":raw_hex(owner),"block_index":clear.block,"block_address":block_address.to_string(),"steps":clear.steps.iter().map(|s| match s {PathStep::Field(n)=>json!({"field_hex":hex(n)}),PathStep::Index(i)=>json!({"index":i})}).collect::<Vec<_>>(),"offset":clear.offset,"width":width,"expected_pointer":clear.address.to_string(),"replacement_pointer":"0","native_action":clear.native_action(),"action":"clear_to_null"}));
            }
            files
                .get_mut(final_owner)
                .ok_or_else(|| unsafe_path("owner plan missing"))?
                .clears = clears;
            let mut edits: Vec<PathRewrite> = Vec::new();
            let mut edit_keys: BTreeMap<(usize, usize, usize, usize), usize> = BTreeMap::new();
            for (_, usage) in usages.iter().enumerate().filter(|(i, u)| {
                !missing_indexes.contains(i) && bytes_path(&u.owner_file) == *owner
            }) {
                let source = crate::trace::resolve_usage(usage);
                let asset = assets
                    .get(&source)
                    .ok_or_else(|| unsafe_path("usage asset missing plan"))?;
                if crate::trace::resolve_usage_at(usage, final_owner) == asset.target {
                    continue;
                }
                let (reference, mut replacement) = if let Some(f) = &usage.path_full_field {
                    (f, relative(final_owner, &asset.target)?)
                } else if let Some(f) = &usage.path_dir_field {
                    (
                        f,
                        relative(
                            final_owner,
                            asset
                                .target
                                .parent()
                                .ok_or_else(|| unsafe_path("directory-only target"))?,
                        )?,
                    )
                } else {
                    return Err(Error::new(
                        "pack_origin",
                        "required relocation has no full/directory field origin",
                    )
                    .into());
                };
                if reference.owner_file != usage.owner_file {
                    return Err(Error::new(
                        "pack_origin",
                        "field owner differs from traced owner file",
                    )
                    .into());
                }
                let mut edit = match (|| {
                    let reader = store.reader(owner_id)?;
                    selector(reader, usage, reference, replacement.clone())
                })() {
                    Ok(edit) => edit,
                    Err(error)
                        if options.permissive
                            && !matches!(error.kind, "io" | "truncated" | "input_changed") =>
                    {
                        report.skipped_items.push(json!({"kind":error.kind,"status":"unknown","reason":error.message,"usage":usage.json()}));
                        report.errors.push(error);
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                if usage.path_full_field.is_none()
                    && edit.expected_path.ends_with(b"/")
                    && !replacement.ends_with(b"/")
                {
                    replacement.push(b'/');
                    edit.replacement_path = replacement;
                }
                coalesce(&mut edits, &mut edit_keys, reference, edit)?;
            }

            if !options.relative_only && !selected.contains(owner) {
                // The selected-path borrow is released across this walk;
                // the nested walk inherits this operation's policy.
                let (local, _) = trace_in_store(
                    store,
                    batch,
                    owner,
                    TraceOptions {
                        mode: TraceMode::PerFile,
                        pointers: PointerMode::Strict,
                        low_memory: false,
                    },
                    op,
                    crate::trace::LibraryPolicy::ReportMissing,
                    true,
                )?;
                if !local.errors.is_empty() {
                    if !options.permissive {
                        return Err(Error::new(
                            "incomplete_trace",
                            "copied-library local path normalization has unknown sites; no outputs",
                        )
                        .into());
                    }
                    for error in &local.errors {
                        let mut item = error.json();
                        item["reason"] = json!(error.message);
                        if !report.skipped_items.contains(&item) {
                            report.skipped_items.push(item);
                        }
                        report
                            .errors
                            .push(Error::new("incomplete_trace", error.message.clone()));
                    }
                }
                for usage in &local.usages {
                    if usage.asset_path.starts_with(b"//") {
                        continue;
                    }
                    let reference = usage
                        .path_full_field
                        .as_ref()
                        .or(usage.path_dir_field.as_ref())
                        .ok_or_else(|| {
                            Error::new("pack_origin", "local path has no fixed field origin")
                        })?;
                    let origin = {
                        let reader = store.reader(owner_id)?;
                        origin_block(reader, usage, reference)?
                    };
                    let key = (origin, reference.element, reference.offset, reference.size);
                    if edit_keys.contains_key(&key)
                        || usages.iter().enumerate().any(|(i, u)| {
                            missing_indexes.contains(&i)
                                && u.owner_file == usage.owner_file
                                && u.block_address == usage.block_address
                                && u.asset_path == usage.asset_path
                        })
                    {
                        continue;
                    }
                    let source = crate::trace::resolve_usage(usage);
                    if excluded(&source, &options.exclude) {
                        continue;
                    }
                    let target = if options.keep_hierarchy {
                        directory.join(strip_root(&source))
                    } else if let Ok(p) = source.strip_prefix(&project) {
                        directory.join(p)
                    } else {
                        directory.join("_outside_project").join(strip_root(&source))
                    };
                    let target = if usage.path_full_field.is_none() {
                        target
                            .parent()
                            .ok_or_else(|| unsafe_path("local directory target"))?
                    } else {
                        &target
                    };
                    if reference.pointer {
                        // Required selected fields already passed selector above
                        // (and still refuse pointer writes). This extra local
                        // normalization must not widen that selected closure.
                        // Preserve the unselected allocation/pointer bytes and
                        // disclose the retained path in both report and overview.
                        report.unrelocated_unselected_paths.push(usage.clone());
                        continue;
                    }
                    let mut replacement = relative(final_owner, target)?;
                    let mut edit = {
                        let reader = store.reader(owner_id)?;
                        selector(reader, usage, reference, replacement.clone())
                    }?;
                    if usage.path_full_field.is_none()
                        && edit.expected_path.ends_with(b"/")
                        && !replacement.ends_with(b"/")
                    {
                        replacement.push(b'/');
                        edit.replacement_path = replacement;
                    }
                    coalesce(&mut edits, &mut edit_keys, reference, edit)?;
                }
            }
            let (_, errors) = {
                let reader = store.reader(owner_id)?;
                prepared(reader, &edits)
            };
            if let Some(e) = errors.first() {
                return Err(
                    Error::new(e.kind, format!("{}: {}", owner.display(), e.message)).into(),
                );
            }
            files
                .get_mut(final_owner)
                .ok_or_else(|| unsafe_path("owner plan missing"))?
                .edits = edits;
        }
        let mut outputs: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
        for file in files.values() {
            let source = &file.input;
            crate::edit_output::name(&file.output)?;
            if !file.output.starts_with(&directory) || file.output == directory {
                return Err(unsafe_path("output escapes destination directory").into());
            }
            if let Some(prior) = outputs.insert(file.output.clone(), source.clone())
                && prior != *source
            {
                return Err(Error::new(
                    "pack_collision",
                    "different inputs map to same final file",
                )
                .into());
            }
        }
        let info_path = directory.join("pack-info.txt");
        if outputs.contains_key(&info_path) || outputs.keys().any(|p| p.starts_with(&info_path)) {
            return Err(Error::new(
                "pack_collision",
                "generated pack-info.txt collides with a required member",
            )
            .into());
        }
        if archive {
            let log = directory.join("bat-debug.log");
            if outputs.contains_key(&log) || outputs.keys().any(|p| p.starts_with(&log)) {
                return Err(Error::new(
                    "pack_collision",
                    "generated ZIP debug log collides with required member",
                )
                .into());
            }
        }
        report.info_path = Some(info_path);
        for output in outputs.keys() {
            for parent in output.ancestors().skip(1) {
                if outputs.contains_key(parent) {
                    return Err(Error::new(
                        "pack_collision",
                        "file/directory final path collision",
                    )
                    .into());
                }
            }
        }
        Ok(files)
    })();
    let files = match finish_planning(planned, planning.take().unwrap(), &mut report.errors)? {
        Some(files) => files,
        None => return Ok(report),
    };
    report.planning_complete = true;
    // Missing on-disk dependencies do not invalidate a completed plan. Publish
    // the available members, but never report a failing pack with no errors.
    // Deduplicate by resolved path; retain the first referencing block.
    let mut missing_paths = BTreeSet::new();
    for usage in &report.missing {
        let path = crate::trace::resolve_usage(usage);
        if missing_paths.insert(path.clone()) {
            report.errors.push(Error::new(
                if usage.block_code == b"LI" {
                    "missing_library"
                } else {
                    "missing_asset"
                },
                format!(
                    "missing path {} referenced by block {} in {}",
                    path.display(),
                    usage.block_address,
                    crate::value::display_bytes(&usage.owner_file)
                ),
            ));
        }
    }

    // Publication policy has exactly one owning boundary: after all preflight
    // checks, before directory/ZIP/member/info creation or any prefetch writes.
    if !options.permissive && !report.errors.is_empty() {
        return Ok(report);
    }
    if !options.noop {
        for file in files.values().filter(|file| !file.blend) {
            // Blend publication reuses its checked store snapshot. Refetching
            // it would leave a second decoded body in an unconsumed reply.
            batch.prefetch(&file.input, false)?;
        }
    }
    report.files = files
        .values()
        .map(|f| PackFile {
            input: f.input.clone(),
            output: f.output.clone(),
            output_written: false,
            durability_completed: false,
            rewritten: !f.edits.is_empty() || !f.clears.is_empty(),
        })
        .collect();
    if options.noop {
        report.complete = report.missing.is_empty() && report.errors.is_empty();
        return Ok(report);
    }
    // Workspace creation precedes every member/info/ZIP effect, so a
    // staging failure keeps the legacy direct refusal (no report yet to
    // preserve). This is deliberately distinct from post-publication
    // bookkeeping below, which must never discard a published report.
    let workspace = if archive {
        Some(crate::pack_zip::Workspace::create_in(&op.temp_root)?)
    } else {
        None
    };
    let physical = |output: &Path| -> Result<PathBuf> {
        if let Some(w) = &workspace {
            Ok(w.0.join(
                output
                    .strip_prefix(&directory)
                    .map_err(|_| unsafe_path("ZIP member escaped namespace"))?,
            ))
        } else {
            Ok(output.to_path_buf())
        }
    };
    let entry_durability = if !archive && options.durability == Durability::EachFile {
        Durability::EachFile
    } else {
        Durability::None
    };
    let mut execution_span =
        crate::diagnostics::Span::new("pack.execute", json!({"files":report.files.len()}));
    let execution = (|| {
        if !archive && destination_empty(&directory)? {
            let creation = create_directories(&directory, options.durability);
            report.directory_created = directory.is_dir();
            creation?;
        }
        for (file, result) in files.values().zip(&mut report.files) {
            let source = &file.input;
            if signature(
                &source
                    .metadata()
                    .map_err(|e| io_at("stat source before execution", source, e))?,
            ) != signature(&file.metadata)
            {
                return Err(Error::new(
                    "input_changed",
                    "pack input changed after complete planning",
                ));
            }
            let physical_output = physical(&file.output)?;
            let parent = physical_output
                .parent()
                .ok_or_else(|| unsafe_path("file destination parent"))?;
            create_directories(parent, options.durability)?;
            // Essential member publication stays visible at L1; per-member
            // detail is not optional noise that can hide all pack reads.
            let mut member_span = crate::diagnostics::Span::new(
                if file.blend {
                    "pack.repath"
                } else {
                    "pack.copy"
                },
                if crate::diagnostics::enabled() {
                    json!({"input":source.to_string_lossy(),"output":physical_output.to_string_lossy()})
                } else {
                    serde_json::Value::Null
                },
            );
            let member_result = (|| {
            if file.blend {
                let owner_id = store.id_for_path(source)?;
                // Admission is a phase boundary: prove the live source still
                // matches the retained digest before this member's first
                // output byte exists. The physical container below is the
                // live source file; metadata checks cannot express a
                // same-signature content change, and a mismatch must latch
                // and refuse rather than bless new bytes with an old proof.
                store.revalidate_working_body(owner_id)?;
                // The snapshot borrow ends with this block; guarded kernel
                // transport below borrows the store mutably instead.
                let edited = {
                    let snapshot = store.snapshot(owner_id)?;
                    let reuse = crate::diagnostics::Span::new(
                        "reader.reuse",
                        if crate::diagnostics::enabled() {
                            json!({"path":source.to_string_lossy(),"captured":true,"reason":"blend rewrite reuses retained parsed capture"})
                        } else { serde_json::Value::Null },
                    );
                    let source_file = reuse.result(snapshot.open_checked(source))?;
                    // L17 sealed per-source plan record: the union coalesced
                    // edits/clears exactly once before any publication; the
                    // emitter below streams original raw bytes in increasing
                    // offsets replacing exactly these spans. Keys carry block
                    // and length identity only, never replacement content.
                    // Byte identity itself is proven by member-hash parity
                    // against baseline, not by this record. Emitted only for
                    // selective raw-window captures; compressed/bounded
                    // members keep the historical transport and its points.
                    if crate::diagnostics::enabled()
                        && matches!(
                            snapshot.reader.data,
                            crate::input::Backing::RawWindows(_)
                        ) {
                        crate::diagnostics::point(
                            "raw.publish.plan",
                            json!({
                                "path": source.to_string_lossy(),
                                "output": physical_output.to_string_lossy(),
                                "edits": file.edits.len(),
                                "clears": file.clears.len(),
                                "edit_keys": file.edits.iter().map(|e| json!({"block": e.block_index, "element": e.element, "expected_len": e.expected_path.len(), "replacement_len": e.replacement_path.len()})).collect::<Vec<_>>(),
                                "clear_keys": file.clears.iter().map(|c| json!({"block": c.block, "offset": c.offset, "address": c.address})).collect::<Vec<_>>(),
                                "reader": snapshot.reader.data.stats(),
                            }),
                        );
                    }
                    rewrite_opened_cleared(
                        source,
                        &physical_output,
                        &file.edits,
                        &EditOptions {
                            durability: entry_durability,
                        },
                        &crate::physical::Physical::captured(&source_file, snapshot),
                        &snapshot.metadata,
                        &snapshot.reader,
                        options.compress,
                        &file.clears,
                    )?
                };
                let complete = edited.complete;
                result.output_written = edited.output_written;
                result.durability_completed = edited.durability_completed;
                report.rewrites.push(edited);
                if !complete {
                    return Err(Error::new(
                        "incomplete_edit",
                        "shared rewrite engine failed; inspect rewrites for actual partial state",
                    ));
                }
            } else {
                let mut stage = AtomicOutput::create(&file.metadata, &physical_output)?;
                crate::session::copy_physical(
                    store,
                    batch,
                    source,
                    &file.metadata,
                    &mut stage.file,
                )?;
                let publication = stage.publish(entry_durability);
                result.output_written = stage.published;
                result.durability_completed = stage.durability_completed;
                publication?;
            }
            Ok(())
            })().map_err(|e| crate::edit_output::context(e, "copy/repath member", &physical_output));
            member_span.data(json!({"output_written":result.output_written,"durability_completed":result.durability_completed}));
            member_span.result(member_result)?;
        }
        let info_path = report
            .info_path
            .as_ref()
            .ok_or_else(|| unsafe_path("pack info plan missing"))?;
        let info = overview(&report, &files, &directory, unique_assets)?;
        let mut staging = AtomicOutput::create(
            &input
                .metadata()
                .map_err(|e| io_at("stat overview source", &input, e))?,
            &physical(info_path)?,
        )?;
        staging
            .file
            .write_all(&info)
            .map_err(|e| io_at("write pack-info", info_path, e))?;
        let publication = staging.publish(entry_durability);
        report.info_written = staging.published;
        report.info_durability_completed = staging.durability_completed;
        publication?;
        if let Some(workspace) = &workspace {
            let debug = workspace.0.join("bat-debug.log");
            std::fs::write(&debug, b"Produced by Oxblend. Validated dependency plan; archive publish follows completed ZIP footer.\n").map_err(|e| io_at("write ZIP debug member", &debug, e))?;
            let mut members = files
                .values()
                .map(|f| physical(&f.output))
                .collect::<Result<Vec<_>>>()?;
            members.push(physical(info_path)?);
            members.push(debug);
            members.sort();
            let mut zip = AtomicOutput::create(
                &input
                    .metadata()
                    .map_err(|e| io_at("stat ZIP source", &input, e))?,
                &directory,
            )?;
            crate::pack_zip::emit(&workspace.0, &members, &mut zip.file)?;
            let publication = zip.publish(options.durability);
            report.archive_written = zip.published;
            publication?;
        }
        if options.durability == Durability::End {
            sync_end(if archive {
                directory
                    .parent()
                    .ok_or_else(|| unsafe_path("ZIP parent missing"))?
            } else {
                &directory
            })?;
            for file in &mut report.files {
                file.durability_completed = true;
            }
            report.info_durability_completed = true;
        }
        report.durability_completed = options.durability != Durability::None;
        Ok(())
    })();
    execution_span.data(json!({"files":report.files.len(),"published_files":report.files.iter().filter(|f| f.output_written).count(),"archive_written":report.archive_written}));
    match execution_span.result(execution) {
        Ok(()) => report.complete = report.missing.is_empty() && report.errors.is_empty(),
        Err(e) => report.errors.push(e),
    }
    if archive {
        for file in &mut report.files {
            file.output_written &= report.archive_written;
            file.durability_completed = report.archive_written && report.durability_completed;
        }
        report.info_written &= report.archive_written;
        report.info_durability_completed = report.archive_written && report.durability_completed;
        for edit in &mut report.rewrites {
            let staged = bytes_path(&edit.output_path);
            // Post-publication bookkeeping must not discard the published
            // report: an unmappable staged path is recorded as a report
            // error with the owning edit marked failed, effects retained.
            match staged.strip_prefix(&workspace.as_ref().unwrap().0) {
                Ok(rel) => {
                    edit.output_path = directory.join(rel).as_os_str().as_bytes().to_vec();
                }
                Err(_) => {
                    report
                        .errors
                        .push(unsafe_path("ZIP proof escaped workspace"));
                    report.complete = false;
                    edit.output_written = false;
                    edit.complete = false;
                    continue;
                }
            }
            edit.output_written &= report.archive_written;
            edit.complete &= report.archive_written;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_library_local_walk_guard_reaches_pack_finalizer_without_report_copy() {
        let work = crate::test_scratch::scratch_dir(&std::env::temp_dir());
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dependencies");
        let root = work.join("mesh-material-root.blend");
        let library = work.join("mesh-material-library.blend");
        std::fs::copy(fixtures.join("mesh-material-root.blend"), &root).unwrap();
        std::fs::copy(fixtures.join("mesh-material-library.blend"), &library).unwrap();
        std::fs::write(work.join("texture.png"), b"synthetic image bytes").unwrap();
        let mut session = Session::open(&root).into_result().unwrap();
        assert!(
            crate::trace::trace_session(&mut session, TraceOptions::default())
                .into_result()
                .unwrap()
                .complete
        );
        let mut op = session.operation_context(None);
        op.record_touch(session.root);
        session.store.check_reuse(session.root, &mut op).unwrap();
        // The selected root was consulted by the earlier recursive stage.
        // Change it before the non-selected owner's PerFile normalization:
        // that local walk's completion check (not its library precheck)
        // discovers the change. No race, sleep or runtime test hook.
        let mut changed = std::fs::read(&root).unwrap();
        changed.push(b'X');
        std::fs::write(&root, changed).unwrap();
        let mut batch = CaptureBatch::default();
        let local = trace_in_store(
            &mut session.store,
            &mut batch,
            &library,
            TraceOptions::new(TraceMode::PerFile, PointerMode::Strict),
            &mut op,
            crate::trace::LibraryPolicy::ReportMissing,
            true,
        );
        assert!(matches!(&local, Err(BodyErr::Guard { source, .. }) if source == &root));
        let mut errors = Vec::new();
        let planned = finish_planning(
            local,
            crate::diagnostics::Span::new("pack.plan", json!({})),
            &mut errors,
        );
        assert!(
            errors.is_empty(),
            "a guard is not an independent report error"
        );
        let checks = session.store.check_all(session.root, &mut op);
        let final_result = crate::session::join_body(
            planned,
            &checks,
            &mut op,
            "session.pack",
            Some(root.clone()),
        );
        assert_eq!(Error::from(final_result.unwrap_err()).kind, "input_changed");
        assert!(
            matches!(op.diagnostics.as_slice(), [crate::Diagnostic::SourceCheckFailure { source, .. }] if source == &root),
            "{:?}",
            op.diagnostics
        );
        drop(batch);
        drop(session);
        std::fs::remove_dir_all(work).unwrap();
    }
    /// Deterministic cwd-change fixture for §4.1: the same relative pack
    /// request prepared under two operation cwds resolves project and
    /// destination against the call-entry cwd, never a stale Session cwd.
    /// No process-global directory mutation is involved.
    #[test]
    fn pack_prepare_resolves_against_call_entry_cwd() {
        let work = crate::test_scratch::scratch_dir(&std::env::temp_dir());
        let root = work.join("root.blend");
        std::fs::write(&root, b"synthetic root").unwrap();
        let options = PackOptions::new(work.clone());
        let call_a = work.join("call-a");
        let call_b = work.join("call-b");
        std::fs::create_dir(&call_a).unwrap();
        std::fs::create_dir(&call_b).unwrap();
        let op_a = OpCtx::new(
            GuardPolicy::Session,
            None,
            call_a.clone(),
            PathBuf::from("/tmp"),
        );
        let op_b = OpCtx::new(
            GuardPolicy::Session,
            None,
            call_b.clone(),
            PathBuf::from("/tmp"),
        );
        let prepared_a = pack_prepare(
            std::slice::from_ref(&root),
            Path::new("out"),
            &options,
            &op_a,
        )
        .unwrap();
        let prepared_b = pack_prepare(
            std::slice::from_ref(&root),
            Path::new("out"),
            &options,
            &op_b,
        )
        .unwrap();
        // The absolute captured root identity is frozen in both.
        assert_eq!(prepared_a.roots, vec![root.clone()]);
        assert_eq!(prepared_b.roots, vec![root]);
        // Destination follows the entry cwd; the explicit absolute project
        // root does not move with it.
        assert_eq!(prepared_a.directory, call_a.join("out"));
        assert_eq!(prepared_b.directory, call_b.join("out"));
        assert_eq!(prepared_a.project, work);
        assert_eq!(prepared_b.project, work);
        std::fs::remove_dir_all(&work).unwrap();
    }
    #[test]
    fn star_sequences_preserve_broad_membership_and_literal_star_names() {
        for name in [
            b"frame001.png".as_slice(),
            b"frameWHATEVER.png",
            b"frame*literal.png",
            b"frame\xff.png",
        ] {
            assert!(star_match(b"frame*.png", name));
        }
        assert!(!star_match(b"frame*.png", b"frame001.exr"));
        assert!(star_match(b"*", b"*literal"));
        assert!(star_match(b"*a*b", b"xaayb"));
        assert!(!star_match(b"*a*b", b"xaayc"));
        assert!(star_match(b"tile[0-9]*.png", b"tile1abc.png"));
        assert!(!star_match(b"tile[!a-c]*.png", b"tileb.png"));
        assert!(star_match(b"tile[!a-c]*.png", b"tiled.png"));
        assert!(star_match(b"literal[*].png", b"literal*.png"));
        assert!(star_match(b"caf?.png", "café.png".as_bytes()));
        assert!(star_match(b"a[.png", b"a[.png"));
    }
    // Error-parity pins: the CLI canonical kind/message for every shared
    // refusal, so the binary and the native binding cannot drift apart.
    #[test]
    fn shared_pack_refusals_match_cli_canonical() {
        let single = missing_pack_input_error();
        assert_eq!(single.kind, "pack_input");
        assert_eq!(single.message, "selected input missing or a directory");
        let root = missing_pack_root_error();
        assert_eq!(root.kind, "pack_input");
        assert_eq!(
            root.message,
            "selected input missing or not a regular file; independent root diagnostics retained; no publication"
        );
        let project = missing_pack_project_error(Path::new("/work/no-project"));
        assert_eq!(project.kind, "pack_project");
        assert_eq!(
            project.message,
            "Project directory /work/no-project does not exist; nothing was written"
        );
    }
    #[test]
    fn resolve_project_root_gates_missing_project_canonically() {
        let work = std::env::temp_dir().join(format!(
            "oxblend-pack-project-{}-{}",
            std::process::id(),
            "canonical"
        ));
        std::fs::create_dir_all(&work).unwrap();
        let missing = work.join("no-project");
        let error = resolve_project_root(Some(missing.clone()), &[], false).unwrap_err();
        assert_eq!(error.kind, "pack_project");
        assert_eq!(
            error.message,
            format!(
                "Project directory {} does not exist; nothing was written",
                missing.display()
            )
        );
        std::fs::remove_dir_all(&work).unwrap();
    }
    #[test]
    fn strict_pack_refusal_selects_cli_variant() {
        // Unpublished, non-dry-run: the exact CLI literal, single owner.
        let unpublished = crate::strict::pack_refusal(false, false);
        assert!(unpublished.message.starts_with("Package not published:"));
        let dry_run = crate::strict::pack_refusal(true, false);
        assert!(
            dry_run
                .message
                .starts_with("Dry-run only; nothing was written.")
        );
        let published = crate::strict::pack_refusal(false, true);
        assert!(
            published
                .message
                .starts_with("INCOMPLETE package: output was published")
        );
        // Serialized-report entry reads the same JSON shape the facade
        // hands back, including the nested pack_info output flag.
        let report = serde_json::json!({
            "noop": false,
            "directory_created": false,
            "archive_written": false,
            "pack_info": {"output_written": true},
            "files": [{"output_written": false}],
        });
        let error = strict_pack_refusal_json(&report).unwrap();
        assert_eq!(error.kind, "incomplete_pack");
        assert_eq!(
            error.message,
            crate::strict::pack_refusal(false, true).message
        );
    }
    #[test]
    fn final_owner_relative_path_uses_final_not_staging_location() {
        assert_eq!(
            relative(
                Path::new("/destination/sub/library.blend"),
                Path::new("/destination/assets/a.png")
            )
            .unwrap(),
            b"//../assets/a.png"
        );
        assert_eq!(
            relative(
                Path::new("/destination/root.blend"),
                Path::new("/destination/_outside_project/source/a.png")
            )
            .unwrap(),
            b"//_outside_project/source/a.png"
        );
    }
}
