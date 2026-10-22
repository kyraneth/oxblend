use crate::edit::{
    Durability, EditOptions, EditReport, PathRewrite, prepared, rewrite_opened_cleared,
};
use crate::edit_output::{AtomicOutput, create_directories, io_at, sync_end};
use crate::input::PackInputs;
use crate::trace::{
    FailureKind, FieldReference, PointerMode, TraceMode, TraceOptions, TraceReport, Usage,
    trace_with_inputs,
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
    pub input: PathBuf,
    pub inputs: Vec<PathBuf>,
    pub root_traces: Vec<TraceReport>,
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
    pub fn json(&self) -> Json {
        let links = self.missing_links();
        let missing_ids = links.iter().filter(|v| v["kind"] == "missing_id").count();
        let missing_libraries = links
            .iter()
            .filter(|v| v["kind"] == "missing_library")
            .count();
        crate::value::with_text(
            json!({"schema":1,"operation":"pack","input_path_hex":raw_hex(&self.input),"legacy_input_trace_scope":"first-root-only","input_paths_hex":self.inputs.iter().map(|p|raw_hex(p)).collect::<Vec<_>>(),"root_traces":self.inputs.iter().zip(&self.root_traces).map(|(p,t)|json!({"input_path_hex":raw_hex(p),"trace":t.json()})).collect::<Vec<_>>(),"keep_hierarchy":self.keep_hierarchy,"directory_path_hex":raw_hex(&self.directory),"planning_complete":self.planning_complete,"directory_created":self.directory_created,"noop":self.noop,"compression_requested":self.compressed,"archive":self.archive,"archive_written":self.archive_written,"archive_entry_paths_not_filesystem_paths":self.archive,"complete":self.complete,"durability":self.durability.as_str(),"durability_mechanism":self.durability.mechanism(),"syncfs_shared_filesystem":self.durability == Durability::End,"durability_completed":self.durability_completed,"trace":self.trace.json(),"files":self.files.iter().map(|f|json!({"input_path_hex":raw_hex(&f.input),"output_path_hex":raw_hex(&f.output),"output_written":f.output_written,"durability_completed":f.durability_completed,"rewritten":f.rewritten})).collect::<Vec<_>>(),"rewrites":self.rewrites.iter().map(EditReport::json).collect::<Vec<_>>(),"library_pointer_clears":self.library_pointer_clears,"missing_links":links,"missing_link_counts":{"missing_id":missing_ids,"missing_library":missing_libraries},"skipped_absolute_paths":self.skipped_absolute_paths.iter().map(Usage::json).collect::<Vec<_>>(),"missing":self.missing.iter().map(|u| { let mut v=u.json(); v["absolute_path_hex"]=json!(raw_hex(&crate::trace::resolve_usage(u))); v }).collect::<Vec<_>>(),"warnings":self.unrelocated_unselected_paths.iter().map(|u| { let r=u.path_full_field.as_ref().or(u.path_dir_field.as_ref()).expect("unrelocated path has field provenance"); json!({"kind":"unrelocated_unselected_path","owner_file_hex":hex(&u.owner_file),"owner_file":std::str::from_utf8(&u.owner_file).ok(),"block_address":u.block_address.to_string(),"id_name_hex":hex(&u.block_name),"id_name":std::str::from_utf8(&u.block_name).ok(),"field_path_hex":r.path.iter().map(|p|hex(p)).collect::<Vec<_>>(),"field_path":r.path.iter().map(|p|std::str::from_utf8(p).ok()).collect::<Option<Vec<_>>>(),"asset_path_hex":hex(&u.asset_path),"asset_path":std::str::from_utf8(&u.asset_path).ok()}) }).collect::<Vec<_>>(),"pack_info":self.info_path.as_ref().map(|p|json!({"output_path_hex":raw_hex(p),"output_written":self.info_written,"durability_completed":self.info_durability_completed})),"errors":self.errors.iter().map(|e|json!({"kind":e.kind,"message":e.message})).collect::<Vec<_>>()}),
        )
    }
}
fn raw_hex(path: &Path) -> String {
    hex(path.as_os_str().as_bytes())
}
fn bytes_path(v: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_vec(v.to_vec()))
}
fn absolute(path: &Path) -> Result<PathBuf> {
    Ok(crate::trace::lexical(
        &std::path::absolute(path).map_err(|e| io_at("resolve absolute path", path, e))?,
    ))
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
    let mut text = b"This is an Oxblend pack (Blender Asset Tracer compatible).\n".to_vec();
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
    let missing = report
        .missing
        .iter()
        .map(crate::trace::resolve_usage)
        .collect::<BTreeSet<_>>();
    let links = report.missing_links();
    // Publication is not finished yet, so report.complete is not authoritative
    // here. These are the known closure failures at overview planning time.
    if !missing.is_empty() || !links.is_empty() || !report.errors.is_empty() {
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
    String::from_utf8(text.clone()).map_err(|_| unsafe_path("overview path is not UTF8"))?;
    Ok(text)
}

pub fn pack(inputs: &[PathBuf], new_directory: &Path, options: &PackOptions) -> Result<PackReport> {
    let mut span = crate::diagnostics::Span::new(
        "pack.operation",
        json!({"roots":inputs.len(),"dry_run":options.noop,"path":new_directory.to_string_lossy()}),
    );
    let mut result = pack_inner(inputs, new_directory, options)
        .map_err(|e| crate::edit_output::context(e, "pack", new_directory));
    if let Ok(report) = &mut result {
        for error in &mut report.errors {
            *error = crate::edit_output::context(error.clone(), "pack", new_directory);
        }
        span.data(json!({"complete":report.complete,"planning_complete":report.planning_complete,"missing":report.missing.len(),"files":report.files.len(),"archive_written":report.archive_written,"published_files":report.files.iter().filter(|f| f.output_written).count()}));
        if !report.complete {
            span.end(if report.planning_complete {
                "partial"
            } else {
                "refused"
            });
            return result;
        }
    }
    span.result(result)
}
fn pack_inner(
    inputs: &[PathBuf],
    new_directory: &Path,
    options: &PackOptions,
) -> Result<PackReport> {
    let planning = crate::diagnostics::Span::new("pack.plan", json!({}));
    if inputs.is_empty() {
        return Err(unsafe_path("pack needs at least one selected root"));
    }
    let mut selected = BTreeSet::new();
    let mut roots = Vec::new();
    for input in inputs {
        let input = absolute(input)?;
        if selected.insert(input.clone()) {
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
    for root in &roots {
        if !root.is_file() {
            return Err(Error::new(
                "pack_input",
                "selected blend input does not exist or is a directory",
            ));
        }
    }
    let project = absolute(&options.project_root)?;
    let directory = absolute(new_directory)?;
    let parsed = PackInputs::new(&roots)?;
    let mut root_traces = Vec::new();
    for root in &roots {
        if !root
            .metadata()
            .map_err(|e| io_at("stat selected root", root, e))?
            .is_file()
        {
            return Err(unsafe_path("pack input must be a regular blend file"));
        }
        root_traces.push(trace_with_inputs(
            root,
            TraceOptions {
                mode: TraceMode::Recursive,
                pointers: PointerMode::Strict,
                low_memory: false,
            },
            Some(&parsed),
        )?);
    }
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
        input: input.clone(),
        inputs: roots,
        root_traces,
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
    if report.root_traces.iter().any(|t| {
        t.errors.iter().any(|e| !e.is_external_missing()) || (!t.complete && t.errors.is_empty())
    }) {
        report.errors.push(Error::new(
            "incomplete_trace",
            "strict recursive trace is incomplete; no outputs",
        ));
        return Ok(report);
    }
    let mut unique_assets = 0;
    let planned = (|| {
        if archive {
            refuse_output_symlink(&directory)?;
            if directory.exists() {
                return Err(Error::new(
                    "output_exists",
                    "ZIP output must be new; never replace",
                ));
            }
        } else {
            destination_empty(&directory)?;
        }
        if !project.is_dir() {
            return Err(unsafe_path("project root is not a directory"));
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
            if options.relative_only && !data.starts_with(b"//") {
                report.skipped_absolute_paths.push(usage.clone());
                missing_indexes.insert(index);
                continue;
            }
            if excluded(&source, &options.exclude) {
                missing_indexes.insert(index);
                continue;
            }
            if crate::trace::uncertain_path(data) {
                return Err(Error::new(
                    "path_resolution_unverified",
                    "Foreign Windows-root loader I/O cannot be safely relocated; no outputs; inspect path_resolution_unverified trace warnings",
                ));
            }
            let members = match expand(&source, usage.is_sequence) {
                Ok(members) => members,
                Err(e) if e.kind == "missing_asset" => {
                    report.missing.push(usage.clone());
                    missing_indexes.insert(index);
                    continue;
                }
                Err(e) => return Err(e),
            };
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
                ));
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
            let snapshot = parsed.get(owner)?;
            let reader = &snapshot.reader;
            let clears = crate::library_compat::clears(reader)?;
            if selected.contains(owner) && !clears.is_empty() {
                return Err(Error::new(
                    "dangling_pointer",
                    "known invalid ID reference in a selected main file; no outputs",
                ));
            }
            for clear in &clears {
                report.library_pointer_clears.push(json!({"kind":"library_null_pointer","owner_file_hex":raw_hex(owner),"block_index":clear.block,"block_address":reader.blocks()[clear.block].old_address.to_string(),"steps":clear.steps.iter().map(|s| match s {PathStep::Field(n)=>json!({"field_hex":hex(n)}),PathStep::Index(i)=>json!({"index":i})}).collect::<Vec<_>>(),"offset":clear.offset,"width":reader.header().pointer_size,"expected_pointer":clear.address.to_string(),"replacement_pointer":"0","native_action":clear.native_action(),"action":"clear_to_null"}));
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
                    ));
                };
                if reference.owner_file != usage.owner_file {
                    return Err(Error::new(
                        "pack_origin",
                        "field owner differs from traced owner file",
                    ));
                }
                let mut edit = selector(reader, usage, reference, replacement.clone())?;
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
                let local = trace_with_inputs(
                    owner,
                    TraceOptions {
                        mode: TraceMode::PerFile,
                        pointers: PointerMode::Strict,
                        low_memory: false,
                    },
                    Some(&parsed),
                )?;
                if !local.complete {
                    return Err(Error::new(
                        "incomplete_trace",
                        "copied-library local path normalization is incomplete; no outputs",
                    ));
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
                    let key = (
                        origin_block(reader, usage, reference)?,
                        reference.element,
                        reference.offset,
                        reference.size,
                    );
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
                    let mut edit = selector(reader, usage, reference, replacement.clone())?;
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
            let (_, errors) = prepared(reader, &edits);
            if let Some(e) = errors.first() {
                return Err(Error::new(
                    e.kind,
                    format!("{}: {}", owner.display(), e.message),
                ));
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
                return Err(unsafe_path("output escapes destination directory"));
            }
            if let Some(prior) = outputs.insert(file.output.clone(), source.clone())
                && prior != *source
            {
                return Err(Error::new(
                    "pack_collision",
                    "different inputs map to same final file",
                ));
            }
        }
        let info_path = directory.join("pack-info.txt");
        if outputs.contains_key(&info_path) || outputs.keys().any(|p| p.starts_with(&info_path)) {
            return Err(Error::new(
                "pack_collision",
                "generated pack-info.txt collides with a required member",
            ));
        }
        if archive {
            let log = directory.join("bat-debug.log");
            if outputs.contains_key(&log) || outputs.keys().any(|p| p.starts_with(&log)) {
                return Err(Error::new(
                    "pack_collision",
                    "generated ZIP debug log collides with required member",
                ));
            }
        }
        report.info_path = Some(info_path);
        for output in outputs.keys() {
            for parent in output.ancestors().skip(1) {
                if outputs.contains_key(parent) {
                    return Err(Error::new(
                        "pack_collision",
                        "file/directory final path collision",
                    ));
                }
            }
        }
        Ok(files)
    })();
    let files = match planning.result(planned) {
        Ok(f) => f,
        Err(e) => {
            report.errors.push(e);
            return Ok(report);
        }
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

    for trace in &report.root_traces {
        for failure in &trace.errors {
            if matches!(failure.kind, FailureKind::MissingId) && failure.missing_id.is_some() {
                report
                    .errors
                    .push(Error::new("missing_id", failure.message.clone()));
            }
        }
    }
    if !options.noop {
        for file in files.values() {
            parsed.prefetch(&file.input, file.blend);
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
    let workspace = if archive {
        Some(crate::pack_zip::Workspace::create()?)
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
            let mut member_span = crate::diagnostics::Span::at_level(
                2,
                if file.blend {
                    "pack.repath"
                } else {
                    "pack.copy"
                },
                json!({"input":source.to_string_lossy(),"output":physical_output.to_string_lossy()}),
            );
            let member_result = (|| {
            if file.blend {
                let snapshot = parsed.get(source)?;
                let source_file = snapshot.open_checked(source)?;
                let edited = rewrite_opened_cleared(
                    source,
                    &physical_output,
                    &file.edits,
                    &EditOptions {
                        durability: entry_durability,
                    },
                    &crate::physical::Physical::captured(&source_file, &snapshot),
                    &snapshot.metadata,
                    &snapshot.reader,
                    options.compress,
                    &file.clears,
                )?;
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
                parsed.copy_physical(source, &file.metadata, &mut stage.file)?;
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
        Ok(()) => {
            report.complete =
                report.missing.is_empty() && report.root_traces.iter().all(|t| t.complete)
        }
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
            edit.output_path = directory
                .join(
                    staged
                        .strip_prefix(&workspace.as_ref().unwrap().0)
                        .map_err(|_| unsafe_path("ZIP proof escaped workspace"))?,
                )
                .as_os_str()
                .as_bytes()
                .to_vec();
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
