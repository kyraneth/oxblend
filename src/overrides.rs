//! Serialized library overrides: checked against Blender RNA, without evaluation.
//! Owned, synchronous, read-only. No Blender evaluation/resync/application.
use crate::trace::{PointerMode, resolve};
use crate::value::{PathStep, Value, hex};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::{HashMap, HashSet};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OverrideScope {
    RootFile,
    Recursive,
}
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct OverrideOptions {
    pub scope: OverrideScope,
    pub pointers: PointerMode,
}
impl OverrideOptions {
    pub fn new(scope: OverrideScope, pointers: PointerMode) -> Self {
        Self { scope, pointers }
    }
}
impl Default for OverrideOptions {
    fn default() -> Self {
        Self {
            scope: OverrideScope::Recursive,
            pointers: PointerMode::Strict,
        }
    }
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Provenance {
    pub owner_file: Vec<u8>,
    pub block_code: Vec<u8>,
    pub block_address: u64,
    pub block_offset: usize,
    pub block_size: usize,
    pub block_count: usize,
    pub block_type: Vec<u8>,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct IdIdentity {
    pub source: Provenance,
    pub name: Vec<u8>,
    pub library_address: u64,
    pub library_path: Option<Vec<u8>>,
    pub library_file: Option<Vec<u8>>,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PointerState<T> {
    Null,
    Resolved {
        address: u64,
        target: T,
    },
    Dangling {
        address: u64,
    },
    Ambiguous {
        address: u64,
        candidates: Vec<Provenance>,
    },
    InvalidLayout {
        address: u64,
        source: Option<Provenance>,
        message: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OverrideFailureKind {
    Reader(&'static str),
    DanglingPointer,
    AmbiguousPointer,
    InvalidLayout,
    MissingLibrary,
    LibraryReader(&'static str),
    MissingId,
    AmbiguousId,
    Cycle,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct OverrideFailure {
    pub kind: OverrideFailureKind,
    pub owner_file: Vec<u8>,
    pub block_address: Option<u64>,
    pub field_path: Vec<Vec<u8>>,
    pub pointer_address: Option<u64>,
    pub library_path: Option<Vec<u8>>,
    pub message: String,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum OverrideWarning {
    SilencedPointer { failure: OverrideFailure },
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LibraryLink {
    pub source: Provenance,
    pub path: Vec<u8>,
    pub resolved_file: Vec<u8>,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ReferenceHop {
    pub kind: ReferenceHopKind,
    pub from: IdIdentity,
    pub reference: Option<PointerState<IdIdentity>>,
    pub library: Option<LibraryLink>,
    pub resolved_reference: Option<IdIdentity>,
}
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum ReferenceHopKind {
    OverrideReference,
    LinkedLibrary,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct OverrideOperation {
    pub source: Provenance,
    pub operation: Option<Value>,
    pub flags: Option<Value>,
    pub tag: Option<Value>,
    pub subitem_reference_name: PointerState<Vec<u8>>,
    pub subitem_local_name: PointerState<Vec<u8>>,
    pub subitem_reference_index: Option<Value>,
    pub subitem_local_index: Option<Value>,
    pub subitem_reference_id: PointerState<IdIdentity>,
    pub subitem_local_id: PointerState<IdIdentity>,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct OverrideProperty {
    pub source: Provenance,
    pub rna_path: PointerState<Vec<u8>>,
    pub tag: Option<Value>,
    pub rna_prop_type: Option<Value>,
    pub operations: Vec<OverrideOperation>,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct OverrideRecord {
    pub id: IdIdentity,
    pub library_chain: Vec<Vec<u8>>,
    pub definition: PointerState<Provenance>,
    pub reference: Option<PointerState<IdIdentity>>,
    pub hierarchy_root: Option<PointerState<IdIdentity>>,
    pub flags: Option<Value>,
    pub is_system_override: Option<bool>,
    pub properties: Vec<OverrideProperty>,
    pub reference_chain: Vec<ReferenceHop>,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct OverrideReport {
    pub records: Vec<OverrideRecord>,
    pub errors: Vec<OverrideFailure>,
    pub warnings: Vec<OverrideWarning>,
    pub complete: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct View {
    file: usize,
    block: usize,
}
#[cfg(test)]
#[path = "override_tests.rs"]
mod tests;

#[derive(Default)]
struct IdCandidates {
    local: Vec<usize>,
    linked: Vec<usize>,
}
#[derive(Default)]
struct IdIndex {
    names: HashMap<Vec<u8>, IdCandidates>,
    invalid: Vec<(usize, String)>,
}
impl IdIndex {
    fn insert(&mut self, name: Vec<u8>, block: usize, local: bool) {
        let candidates = self.names.entry(name).or_default();
        if local {
            candidates.local.push(block);
        } else {
            candidates.linked.push(block);
        }
    }
    fn candidates(&self, name: &[u8]) -> &[usize] {
        match self.names.get(name) {
            Some(c) if !c.local.is_empty() => &c.local,
            Some(c) => &c.linked,
            None => &[],
        }
    }
}
struct File {
    path: PathBuf,
    chain: Vec<Vec<u8>>,
    reader: BlendFile,
    ids: Option<IdIndex>,
}
struct Context {
    options: OverrideOptions,
    files: Vec<File>,
    paths: HashMap<PathBuf, usize>,
    report: OverrideReport,
}
fn raw(p: &Path) -> Vec<u8> {
    p.as_os_str().as_bytes().to_vec()
}
fn code(b: &[u8; 4]) -> Vec<u8> {
    b.iter().copied().take_while(|&v| v != 0).collect()
}
fn steps(p: &[&str]) -> Vec<PathStep> {
    p.iter()
        .map(|p| PathStep::Field(p.as_bytes().to_vec()))
        .collect()
}
fn number(v: &Value) -> Option<u64> {
    match v {
        Value::Signed(v) => Some(*v as u64),
        Value::Unsigned(v) => Some(*v),
        _ => None,
    }
}
impl Context {
    // Decode once per opened file, but replay malformed ID diagnostics on each
    // lookup, preserving the original per-traversal diagnostic multiplicity.
    fn index_file(&mut self, file: usize) {
        let mut index = IdIndex::default();
        for block in 0..self.files[file].reader.blocks().len() {
            if code(&self.files[file].reader.blocks()[block].code).len() != 2 {
                continue;
            }
            match self.id(View { file, block }) {
                Ok(id) => index.insert(id.name, block, id.library_address == 0),
                Err(e) => index.invalid.push((block, e.to_string())),
            }
        }
        self.files[file].ids = Some(index);
    }
    fn reader(&self, v: View) -> &BlendFile {
        &self.files[v.file].reader
    }
    fn get(&self, v: View, p: &[&str]) -> Result<Value> {
        self.reader(v).get(v.block, 0, &steps(p), true)
    }
    fn prefix(&self, v: View) -> Result<Vec<&'static str>> {
        Ok(if self.reader(v).dna_type_id(v.block)? == b"ID" {
            vec![]
        } else {
            vec!["id"]
        })
    }
    fn id_value(&self, v: View, field: &str) -> Result<Value> {
        let mut p = self.prefix(v)?;
        p.push(field);
        self.get(v, &p)
    }
    fn provenance(&self, v: View) -> Result<Provenance> {
        let b = self.reader(v).block(v.block)?;
        Ok(Provenance {
            owner_file: raw(&self.files[v.file].path),
            block_code: code(&b.code),
            block_address: b.old_address,
            block_offset: b.data_offset,
            block_size: b.size,
            block_count: b.count,
            block_type: self.reader(v).dna_type_id(v.block)?.to_vec(),
        })
    }
    fn id(&self, v: View) -> Result<IdIdentity> {
        let source = self.provenance(v)?;
        if source.block_code.len() != 2 || source.block_count != 1 {
            return Err(Error::new(
                "invalid_layout",
                "ID target must have a single native two-byte ID BHead record, not an opaque DATA chunk or array",
            ));
        }
        let name = match self.id_value(v, "name")? {
            Value::Bytes(v) => v,
            _ => return Err(Error::new("invalid_layout", "ID name not bytes")),
        };
        if name.len() < 2 {
            return Err(Error::new("invalid_layout", "ID name lacks type prefix"));
        }
        let library_address = match self.id_value(v, "lib")? {
            Value::Pointer(v) => v,
            _ => return Err(Error::new("invalid_layout", "ID.lib not pointer")),
        };
        // Preserve declared library path even for a missing remote file. A corrupt
        // lib pointer remains explicit via library_address and traversal diagnostics.
        let mut library_field = self.prefix(v)?;
        library_field.push("lib");
        let library_path = self
            .reader(v)
            .get_pointer(v.block, 0, &steps(&library_field))
            .ok()
            .flatten()
            .and_then(|block| {
                self.get(
                    View {
                        file: v.file,
                        block,
                    },
                    &["name"],
                )
                .ok()
            })
            .and_then(|value| match value {
                Value::Bytes(v) => Some(v),
                _ => None,
            });
        let library_file = library_path
            .as_ref()
            .map(|p| raw(&resolve(&self.files[v.file].path, p)));
        Ok(IdIdentity {
            source,
            name,
            library_address,
            library_path,
            library_file,
        })
    }
    fn fault(
        &mut self,
        v: View,
        p: &[&str],
        kind: OverrideFailureKind,
        addr: Option<u64>,
        lib: Option<Vec<u8>>,
        message: impl Into<String>,
    ) {
        let failure = OverrideFailure {
            kind,
            owner_file: raw(&self.files[v.file].path),
            block_address: self.reader(v).block(v.block).ok().map(|b| b.old_address),
            field_path: p.iter().map(|s| s.as_bytes().to_vec()).collect(),
            pointer_address: addr,
            library_path: lib,
            message: message.into(),
        };
        if kind == OverrideFailureKind::DanglingPointer
            && self.options.pointers == PointerMode::NonStrict
        {
            self.report.warnings.push(OverrideWarning::SilencedPointer {
                failure: failure.clone(),
            });
        }
        self.report.complete = false;
        self.report.errors.push(failure);
    }
    fn scalar(&mut self, v: View, p: &[&str]) -> Option<Value> {
        match self.get(v, p) {
            Ok(value) => Some(value),
            Err(e) => {
                self.fault(
                    v,
                    p,
                    OverrideFailureKind::InvalidLayout,
                    None,
                    None,
                    e.to_string(),
                );
                None
            }
        }
    }
    fn optional_scalar(&mut self, v: View, p: &[&str]) -> Option<Value> {
        match self.get(v, p) {
            Ok(value) => Some(value),
            Err(e) if e.kind == "missing_field" => None,
            Err(e) => {
                self.fault(
                    v,
                    p,
                    OverrideFailureKind::InvalidLayout,
                    None,
                    None,
                    e.to_string(),
                );
                None
            }
        }
    }
    fn pointer(&mut self, v: View, p: &[&str]) -> std::result::Result<u64, String> {
        match self.get(v, p) {
            Ok(Value::Pointer(a)) => Ok(a),
            Ok(_) => {
                let m = "expected pointer field".to_string();
                self.fault(v, p, OverrideFailureKind::InvalidLayout, None, None, &m);
                Err(m)
            }
            Err(e) => {
                let m = e.to_string();
                self.fault(v, p, OverrideFailureKind::InvalidLayout, None, None, &m);
                Err(m)
            }
        }
    }
    fn lookup(&mut self, v: View, p: &[&str]) -> PointerState<Provenance> {
        let a = match self.pointer(v, p) {
            Ok(a) => a,
            Err(message) => {
                return PointerState::InvalidLayout {
                    address: 0,
                    source: None,
                    message,
                };
            }
        };
        if a == 0 {
            return PointerState::Null;
        }
        match self.reader(v).resolve_pointer(v.block, 0, &steps(p), a) {
            Ok(Some(block)) => match self.provenance(View {
                file: v.file,
                block,
            }) {
                Ok(target) => PointerState::Resolved { address: a, target },
                Err(e) => {
                    self.fault(
                        v,
                        p,
                        OverrideFailureKind::InvalidLayout,
                        Some(a),
                        None,
                        e.to_string(),
                    );
                    PointerState::InvalidLayout {
                        address: a,
                        source: None,
                        message: e.to_string(),
                    }
                }
            },
            Ok(None) => PointerState::Null,
            Err(e) if e.kind == "dangling_pointer" => {
                self.fault(
                    v,
                    p,
                    OverrideFailureKind::DanglingPointer,
                    Some(a),
                    None,
                    e.message,
                );
                PointerState::Dangling { address: a }
            }
            Err(e) if e.kind == "ambiguous_pointer" => {
                let candidates = self
                    .reader(v)
                    .blocks()
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.old_address == a)
                    .filter_map(|(block, _)| {
                        self.provenance(View {
                            file: v.file,
                            block,
                        })
                        .ok()
                    })
                    .collect();
                self.fault(
                    v,
                    p,
                    OverrideFailureKind::AmbiguousPointer,
                    Some(a),
                    None,
                    e.message,
                );
                PointerState::Ambiguous {
                    address: a,
                    candidates,
                }
            }
            Err(e) => {
                self.fault(
                    v,
                    p,
                    OverrideFailureKind::Reader(e.kind),
                    Some(a),
                    None,
                    &e.message,
                );
                PointerState::InvalidLayout {
                    address: a,
                    source: None,
                    message: e.message,
                }
            }
        }
    }
    fn target_view(&self, file: usize, source: &Provenance) -> Result<View> {
        let reader = &self.files[file].reader;
        let block = reader
            .blocks()
            .binary_search_by_key(&source.block_offset, |b| b.data_offset)
            .map_err(|_| Error::new("invalid_layout", "non-null target lost"))?;
        if reader.blocks()[block].old_address != source.block_address {
            return Err(Error::new("invalid_layout", "target provenance differs"));
        }
        Ok(View { file, block })
    }
    fn typed(&mut self, v: View, p: &[&str], expected: &[u8]) -> PointerState<Provenance> {
        let state = self.lookup(v, p);
        match state {
            PointerState::Resolved { address, target }
                if target.block_type != expected || target.block_count != 1 =>
            {
                let message = format!(
                    "expected {} single record",
                    String::from_utf8_lossy(expected)
                );
                self.fault(
                    v,
                    p,
                    OverrideFailureKind::InvalidLayout,
                    Some(address),
                    None,
                    &message,
                );
                PointerState::InvalidLayout {
                    address,
                    source: Some(target),
                    message,
                }
            }
            _ => state,
        }
    }
    fn id_pointer(&mut self, v: View, p: &[&str]) -> PointerState<IdIdentity> {
        match self.lookup(v, p) {
            PointerState::Resolved { address, target } => {
                match self.target_view(v.file, &target).and_then(|v| self.id(v)) {
                    Ok(id) => PointerState::Resolved {
                        address,
                        target: id,
                    },
                    Err(e) => {
                        self.fault(
                            v,
                            p,
                            OverrideFailureKind::InvalidLayout,
                            Some(address),
                            None,
                            e.to_string(),
                        );
                        PointerState::InvalidLayout {
                            address,
                            source: Some(target),
                            message: e.to_string(),
                        }
                    }
                }
            }
            PointerState::Null => PointerState::Null,
            PointerState::Dangling { address } => PointerState::Dangling { address },
            PointerState::Ambiguous {
                address,
                candidates,
            } => PointerState::Ambiguous {
                address,
                candidates,
            },
            PointerState::InvalidLayout {
                address,
                source,
                message,
            } => PointerState::InvalidLayout {
                address,
                source,
                message,
            },
        }
    }
    fn string_pointer(&mut self, v: View, p: &[&str]) -> PointerState<Vec<u8>> {
        match self.lookup(v, p) {
            PointerState::Resolved { address, target } => {
                let result = if target.block_code != b"DATA" {
                    Err(Error::new(
                        "invalid_layout",
                        "string target is not opaque DATA",
                    ))
                } else {
                    self.target_view(v.file, &target)
                        .and_then(|v| self.reader(v).payload(v.block).map(|b| b.to_vec()))
                }
                .and_then(|b| {
                    b.iter()
                        .position(|&b| b == 0)
                        .map(|end| b[..end].to_vec())
                        .ok_or_else(|| Error::new("invalid_layout", "unterminated string target"))
                });
                match result {
                    Ok(target) => PointerState::Resolved { address, target },
                    Err(e) => {
                        self.fault(
                            v,
                            p,
                            OverrideFailureKind::InvalidLayout,
                            Some(address),
                            None,
                            e.to_string(),
                        );
                        PointerState::InvalidLayout {
                            address,
                            source: Some(target),
                            message: e.to_string(),
                        }
                    }
                }
            }
            PointerState::Null => PointerState::Null,
            PointerState::Dangling { address } => PointerState::Dangling { address },
            PointerState::Ambiguous {
                address,
                candidates,
            } => PointerState::Ambiguous {
                address,
                candidates,
            },
            PointerState::InvalidLayout {
                address,
                source,
                message,
            } => PointerState::InvalidLayout {
                address,
                source,
                message,
            },
        }
    }
    fn list(&mut self, v: View, p: &[&str], expected: &[u8]) -> Vec<View> {
        let mut path = p.to_vec();
        path.push("first");
        let mut state = self.typed(v, &path, expected);
        let mut seen = HashSet::new();
        let mut views = Vec::new();
        let mut last_reached = 0;
        let mut cycle = false;
        while let PointerState::Resolved {
            address,
            ref target,
        } = state
        {
            if !seen.insert(address) {
                self.fault(
                    v,
                    &path,
                    OverrideFailureKind::Cycle,
                    Some(address),
                    None,
                    "cycle in serialized ListBase",
                );
                cycle = true;
                break;
            }
            let current = match self.target_view(v.file, target) {
                Ok(v) => v,
                Err(e) => {
                    self.fault(
                        v,
                        &path,
                        OverrideFailureKind::Reader(e.kind),
                        Some(address),
                        None,
                        e.message,
                    );
                    break;
                }
            };
            views.push(current);
            last_reached = address;
            state = self.typed(current, &["next"], expected);
        }
        // A clean NULL-terminated ListBase must agree with its declared tail.
        // After a pointer fault or cycle, the prefix cannot establish a tail:
        // retain the causal diagnostic, never add a redundant last mismatch.
        if !cycle && matches!(state, PointerState::Null) {
            let mut last_path = p.to_vec();
            last_path.push("last");
            if let Ok(declared) = self.pointer(v, &last_path)
                && declared != last_reached
            {
                self.fault(
                    v,
                    &last_path,
                    OverrideFailureKind::InvalidLayout,
                    Some(declared),
                    None,
                    "ListBase.last differs from final reachable node",
                );
            }
        }
        views
    }
    fn properties(&mut self, v: View) -> Vec<OverrideProperty> {
        let mut result = Vec::new();
        for prop in self.list(v, &["properties"], b"IDOverrideLibraryProperty") {
            let source = match self.provenance(prop) {
                Ok(v) => v,
                Err(e) => {
                    self.fault(
                        prop,
                        &[],
                        OverrideFailureKind::Reader(e.kind),
                        None,
                        None,
                        e.message,
                    );
                    continue;
                }
            };
            let rna_path = self.string_pointer(prop, &["rna_path"]);
            let tag = self.scalar(prop, &["tag"]);
            let rna_prop_type = self.optional_scalar(prop, &["rna_prop_type"]);
            let mut operations = Vec::new();
            for op in self.list(prop, &["operations"], b"IDOverrideLibraryPropertyOperation") {
                let source = match self.provenance(op) {
                    Ok(v) => v,
                    Err(e) => {
                        self.fault(
                            op,
                            &[],
                            OverrideFailureKind::Reader(e.kind),
                            None,
                            None,
                            e.message,
                        );
                        continue;
                    }
                };
                operations.push(OverrideOperation {
                    source,
                    operation: self.scalar(op, &["operation"]),
                    flags: self.scalar(op, &["flag"]),
                    tag: self.scalar(op, &["tag"]),
                    subitem_reference_name: self.string_pointer(op, &["subitem_reference_name"]),
                    subitem_local_name: self.string_pointer(op, &["subitem_local_name"]),
                    subitem_reference_index: self.scalar(op, &["subitem_reference_index"]),
                    subitem_local_index: self.scalar(op, &["subitem_local_index"]),
                    subitem_reference_id: self.optional_id_pointer(op, &["subitem_reference_id"]),
                    subitem_local_id: self.optional_id_pointer(op, &["subitem_local_id"]),
                });
            }
            result.push(OverrideProperty {
                source,
                rna_path,
                tag,
                rna_prop_type,
                operations,
            });
        }
        result
    }
    fn optional_id_pointer(&mut self, v: View, p: &[&str]) -> PointerState<IdIdentity> {
        if matches!(self.get(v,p),Err(e) if e.kind=="missing_field") {
            PointerState::Null
        } else {
            self.id_pointer(v, p)
        }
    }
    fn open_library(
        &mut self,
        v: View,
        li: View,
        chain: &[Vec<u8>],
    ) -> Option<(usize, LibraryLink)> {
        let source = match self.provenance(li) {
            Ok(s) => s,
            Err(e) => {
                self.fault(
                    v,
                    &["lib"],
                    OverrideFailureKind::InvalidLayout,
                    None,
                    None,
                    e.message,
                );
                return None;
            }
        };
        if source.block_code != b"LI" || source.block_type != b"Library" {
            self.fault(
                v,
                &["lib"],
                OverrideFailureKind::InvalidLayout,
                Some(source.block_address),
                None,
                "ID.lib target is not Library",
            );
            return None;
        }
        let data = match self.get(li, &["name"]) {
            Ok(Value::Bytes(b)) => b,
            _ => {
                self.fault(
                    li,
                    &["name"],
                    OverrideFailureKind::InvalidLayout,
                    None,
                    None,
                    "Library.name not bytes",
                );
                return None;
            }
        };
        let path = resolve(&self.files[li.file].path, &data);
        let link = LibraryLink {
            source,
            path: data,
            resolved_file: raw(&path),
        };
        if let Some(&file) = self.paths.get(&path) {
            return Some((file, link));
        }
        match BlendFile::open(&path) {
            Ok(reader) => {
                let file = self.files.len();
                let mut chain = chain.to_vec();
                chain.push(raw(&path));
                self.paths.insert(path.clone(), file);
                self.files.push(File {
                    path,
                    chain,
                    reader,
                    ids: None,
                });
                Some((file, link))
            }
            Err(e) => {
                let kind = if !path.exists() {
                    OverrideFailureKind::MissingLibrary
                } else {
                    OverrideFailureKind::LibraryReader(e.kind)
                };
                self.fault(
                    v,
                    &["lib"],
                    kind,
                    Some(link.source.block_address),
                    Some(link.resolved_file),
                    e.message,
                );
                None
            }
        }
    }
    fn declared_library(&self, v: View) -> Option<LibraryLink> {
        let lib = self
            .reader(v)
            .get_pointer(
                v.block,
                0,
                &steps(&{
                    let mut p = self.prefix(v).ok()?;
                    p.push("lib");
                    p
                }),
            )
            .ok()??;
        let li = View {
            file: v.file,
            block: lib,
        };
        let source = self.provenance(li).ok()?;
        if source.block_code != b"LI" || source.block_type != b"Library" {
            return None;
        }
        let path = match self.get(li, &["name"]).ok()? {
            Value::Bytes(p) => p,
            _ => return None,
        };
        let resolved_file = raw(&resolve(&self.files[v.file].path, &path));
        Some(LibraryLink {
            source,
            path,
            resolved_file,
        })
    }
    fn resolve_linked(
        &mut self,
        v: View,
        id: &IdIdentity,
        chain: &[Vec<u8>],
    ) -> Option<(View, LibraryLink)> {
        let state = self.typed(
            v,
            &self
                .prefix(v)
                .ok()
                .map(|mut p| {
                    p.push("lib");
                    p
                })
                .unwrap_or_else(|| vec!["lib"]),
            b"Library",
        );
        let li = match state {
            PointerState::Resolved { target, .. } => self.target_view(v.file, &target).ok()?,
            _ => return None,
        };
        let (file, link) = self.open_library(v, li, chain)?;
        // Files merely opened/visited incur no index work. Build once on the
        // first owning-ID lookup, with no diagnostic until its normal replay.
        if self.files[file].ids.is_none() {
            self.index_file(file);
        }
        for (block, message) in self.files[file].ids.as_ref().unwrap().invalid.clone() {
            self.fault(
                View { file, block },
                &["id", "name"],
                OverrideFailureKind::InvalidLayout,
                None,
                None,
                message,
            );
        }
        // Separate namespaces preserve local-wins and within-namespace ambiguity.
        match self.files[file].ids.as_ref().unwrap().candidates(&id.name) {
            [block] => Some((
                View {
                    file,
                    block: *block,
                },
                link,
            )),
            [] => {
                self.fault(
                    v,
                    &["lib"],
                    OverrideFailureKind::MissingId,
                    Some(id.source.block_address),
                    Some(link.resolved_file),
                    "raw ID name/type absent from library",
                );
                None
            }
            _ => {
                self.fault(
                    v,
                    &["lib"],
                    OverrideFailureKind::AmbiguousId,
                    Some(id.source.block_address),
                    Some(link.resolved_file),
                    "raw ID name/type ambiguous in library",
                );
                None
            }
        }
    }
    fn chain(&mut self, start: View) -> Vec<ReferenceHop> {
        let mut current = start;
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        let mut reaching = self.files[start.file].chain.clone();
        loop {
            if !seen.insert(current) {
                self.fault(
                    current,
                    &["override_library", "reference"],
                    OverrideFailureKind::Cycle,
                    None,
                    None,
                    "cycle in override/reference linked-ID chain",
                );
                break;
            }
            let from = match self.id(current) {
                Ok(id) => id,
                Err(e) => {
                    self.fault(
                        current,
                        &["id"],
                        OverrideFailureKind::InvalidLayout,
                        None,
                        None,
                        e.to_string(),
                    );
                    break;
                }
            };
            if from.library_address != 0 {
                // A linked-ID edge is NOT a fabricated IDOverrideLibrary.reference
                // pointer. Keep its declared LI path even if opening/matching fails.
                let mut hop = ReferenceHop {
                    kind: ReferenceHopKind::LinkedLibrary,
                    library: self.declared_library(current),
                    from: from.clone(),
                    reference: None,
                    resolved_reference: None,
                };
                if let Some((next, link)) = self.resolve_linked(current, &from, &reaching) {
                    reaching.push(link.resolved_file.clone());
                    hop.library = Some(link);
                    hop.resolved_reference = self.id(next).ok();
                    result.push(hop);
                    current = next;
                    continue;
                }
                result.push(hop);
                break;
            }
            let mut p = self.prefix(current).unwrap_or_default();
            p.push("override_library");
            let address = match self.get(current, &p) {
                Ok(Value::Pointer(a)) => a,
                _ => {
                    self.fault(
                        current,
                        &p,
                        OverrideFailureKind::InvalidLayout,
                        None,
                        None,
                        "override_library field unreadable",
                    );
                    break;
                }
            };
            if address == 0 {
                break;
            }
            let def = match self.typed(current, &p, b"IDOverrideLibrary") {
                PointerState::Resolved { target, .. } => {
                    match self.target_view(current.file, &target) {
                        Ok(v) => v,
                        Err(_) => break,
                    }
                }
                _ => break,
            };
            let reference = self.id_pointer(def, &["reference"]);
            let next = match &reference {
                PointerState::Resolved { target, .. } => {
                    self.target_view(current.file, &target.source).ok()
                }
                _ => None,
            };
            let resolved_reference = next.and_then(|v| self.id(v).ok());
            result.push(ReferenceHop {
                kind: ReferenceHopKind::OverrideReference,
                from,
                reference: Some(reference),
                library: None,
                resolved_reference,
            });
            if let Some(next) = next {
                current = next;
            } else {
                break;
            }
        }
        result
    }
    fn record(
        &mut self,
        v: View,
        id: IdIdentity,
        definition: PointerState<Provenance>,
    ) -> OverrideRecord {
        let mut record = OverrideRecord {
            id,
            library_chain: self.files[v.file].chain.clone(),
            definition,
            reference: None,
            hierarchy_root: None,
            flags: None,
            is_system_override: None,
            properties: Vec::new(),
            reference_chain: Vec::new(),
        };
        if let PointerState::Resolved { target, .. } = &record.definition
            && let Ok(def) = self.target_view(v.file, target)
        {
            record.reference = Some(self.id_pointer(def, &["reference"]));
            record.hierarchy_root = Some(self.id_pointer(def, &["hierarchy_root"]));
            record.flags = self.scalar(def, &["flag"]);
            // Native5.2.1 factory hierarchy False/True proved bit1=system override.
            // Other versions/bits remain raw; no unqualified named interpretation.
            if self.reader(v).header().version == 502 {
                record.is_system_override = record
                    .flags
                    .as_ref()
                    .and_then(number)
                    .map(|flags| flags & 2 != 0);
            }
            record.properties = self.properties(def);
            // First reference was already read/diagnosed; avoid duplicate diagnostics.
            if matches!(record.reference, Some(PointerState::Resolved { .. })) {
                record.reference_chain = self.chain(v);
            }
        }
        record
    }
    fn visit_file(&mut self, file: usize) {
        for block in 0..self.files[file].reader.blocks().len() {
            if code(&self.files[file].reader.blocks()[block].code).len() != 2 {
                continue;
            }
            let v = View { file, block };
            let id = match self.id(v) {
                Ok(id) => id,
                Err(e) => {
                    self.fault(
                        v,
                        &["id"],
                        OverrideFailureKind::InvalidLayout,
                        None,
                        None,
                        e.to_string(),
                    );
                    continue;
                }
            };
            if id.library_address != 0 {
                // Recursive definitions include relevant linked-ID owners, even if a root ID is not an override.
                if self.options.scope == OverrideScope::Recursive {
                    let reaching = self.files[file].chain.clone();
                    self.resolve_linked(v, &id, &reaching);
                }
                continue;
            }
            let mut p = self.prefix(v).unwrap_or_default();
            p.push("override_library");
            match self.get(v, &p) {
                Ok(Value::Pointer(0)) => (),
                Ok(Value::Pointer(_)) => {
                    let definition = self.typed(v, &p, b"IDOverrideLibrary");
                    let record = self.record(v, id, definition);
                    self.report.records.push(record);
                }
                Err(e) if e.kind == "missing_field" => (),
                _ => self.fault(
                    v,
                    &p,
                    OverrideFailureKind::InvalidLayout,
                    None,
                    None,
                    "override_library is not a pointer",
                ),
            }
        }
    }
}
pub fn trace_overrides(root: &Path, options: OverrideOptions) -> Result<OverrideReport> {
    let path = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| Error::new("io", e.to_string()))?
            .join(root)
    };
    let path = crate::trace::lexical(&path);
    let reader = BlendFile::open(&path)?;
    let mut ctx = Context {
        options,
        files: vec![File {
            chain: vec![raw(&path)],
            path: path.clone(),
            reader,
            ids: None,
        }],
        paths: HashMap::from([(path, 0)]),
        report: OverrideReport {
            records: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            complete: true,
        },
    };
    let mut file = 0;
    while file < ctx.files.len() {
        ctx.visit_file(file);
        file += 1;
        if options.scope == OverrideScope::RootFile {
            break;
        }
    }
    Ok(ctx.report)
}
impl Provenance {
    fn json(&self) -> Json {
        json!({"owner_file_hex":hex(&self.owner_file),"block_code_hex":hex(&self.block_code),"block_address_decimal":self.block_address.to_string(),"block_offset":self.block_offset,"block_size":self.block_size,"block_count":self.block_count,"block_type_hex":hex(&self.block_type)})
    }
}
impl IdIdentity {
    fn json(&self) -> Json {
        json!({"source":self.source.json(),"name_hex":hex(&self.name),"library_address_decimal":self.library_address.to_string(),"library_path_hex":self.library_path.as_ref().map(|p|hex(p)),"library_file_hex":self.library_file.as_ref().map(|p|hex(p))})
    }
}
impl<T> PointerState<T> {
    fn json(&self, encode: impl Fn(&T) -> Json) -> Json {
        match self {
            Self::Null => json!({"state":"null","address_decimal":"0"}),
            Self::Resolved { address, target } => {
                json!({"state":"resolved","address_decimal":address.to_string(),"target":encode(target)})
            }
            Self::Dangling { address } => {
                json!({"state":"dangling","address_decimal":address.to_string()})
            }
            Self::Ambiguous {
                address,
                candidates,
            } => {
                json!({"state":"ambiguous","address_decimal":address.to_string(),"candidates":candidates.iter().map(Provenance::json).collect::<Vec<_>>()})
            }
            Self::InvalidLayout {
                address,
                source,
                message,
            } => {
                json!({"state":"invalid_layout","address_decimal":address.to_string(),"source":source.as_ref().map(Provenance::json),"message":message})
            }
        }
    }
}
impl OverrideFailure {
    pub fn json(&self) -> Json {
        let kind = match self.kind {
            OverrideFailureKind::Reader(k) => k,
            OverrideFailureKind::LibraryReader(_) => "library_reader",
            OverrideFailureKind::DanglingPointer => "dangling_pointer",
            OverrideFailureKind::AmbiguousPointer => "ambiguous_pointer",
            OverrideFailureKind::InvalidLayout => "invalid_layout",
            OverrideFailureKind::MissingLibrary => "missing_library",
            OverrideFailureKind::MissingId => "missing_id",
            OverrideFailureKind::AmbiguousId => "ambiguous_id",
            OverrideFailureKind::Cycle => "cycle",
        };
        json!({"kind":kind,"reader_kind":match self.kind {OverrideFailureKind::LibraryReader(k)=>Some(k),_=>None},"owner_file_hex":hex(&self.owner_file),"block_address_decimal":self.block_address.map(|a|a.to_string()),"field_path_hex":self.field_path.iter().map(|s|hex(s)).collect::<Vec<_>>(),"pointer_address_decimal":self.pointer_address.map(|a|a.to_string()),"library_path_hex":self.library_path.as_ref().map(|p|hex(p)),"message":self.message})
    }
}
impl OverrideReport {
    pub fn json(&self) -> Json {
        json!({"schema":1,"semantic":"serialized-overrides-1","records":self.records.iter().map(|r|json!({"id":r.id.json(),"library_chain_hex":r.library_chain.iter().map(|p|hex(p)).collect::<Vec<_>>(),"definition":r.definition.json(Provenance::json),"reference":r.reference.as_ref().map(|p|p.json(IdIdentity::json)),"hierarchy_root":r.hierarchy_root.as_ref().map(|p|p.json(IdIdentity::json)),"flags":r.flags.as_ref().map(Value::json),"is_system_override":r.is_system_override,"properties":r.properties.iter().map(|p|json!({"source":p.source.json(),"rna_path":p.rna_path.json(|p|json!({"hex":hex(p)})),"tag":p.tag.as_ref().map(Value::json),"rna_prop_type":p.rna_prop_type.as_ref().map(Value::json),"operations":p.operations.iter().map(|o|json!({"source":o.source.json(),"operation":o.operation.as_ref().map(Value::json),"flags":o.flags.as_ref().map(Value::json),"tag":o.tag.as_ref().map(Value::json),"subitem_reference_name":o.subitem_reference_name.json(|n|json!({"hex":hex(n)})),"subitem_local_name":o.subitem_local_name.json(|n|json!({"hex":hex(n)})),"subitem_reference_index":o.subitem_reference_index.as_ref().map(Value::json),"subitem_local_index":o.subitem_local_index.as_ref().map(Value::json),"subitem_reference_id":o.subitem_reference_id.json(IdIdentity::json),"subitem_local_id":o.subitem_local_id.json(IdIdentity::json)})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"reference_chain":r.reference_chain.iter().map(|h|json!({"kind":match h.kind {ReferenceHopKind::OverrideReference=>"override_reference",ReferenceHopKind::LinkedLibrary=>"linked_library"},"from":h.from.json(),"reference":h.reference.as_ref().map(|p|p.json(IdIdentity::json)),"library":h.library.as_ref().map(|l|json!({"source":l.source.json(),"path_hex":hex(&l.path),"resolved_file_hex":hex(&l.resolved_file)})),"resolved_reference":h.resolved_reference.as_ref().map(IdIdentity::json)})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"errors":self.errors.iter().map(OverrideFailure::json).collect::<Vec<_>>(),"warnings":self.warnings.iter().map(|w|match w {OverrideWarning::SilencedPointer{failure}=>json!({"kind":"silenced_pointer","failure":failure.json()})}).collect::<Vec<_>>(),"complete":self.complete})
    }
}
