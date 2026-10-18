use crate::input::PackInputs;
use crate::value::{PathStep, Value, hex};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

pub(crate) type NameIndex = HashMap<Vec<u8>, HashMap<Vec<u8>, Vec<usize>>>;
pub(crate) fn geometry_import(idname: &[u8]) -> bool {
    matches!(
        idname,
        b"GeometryNodeImportOBJ"
            | b"GeometryNodeImportPLY"
            | b"GeometryNodeImportSTL"
            | b"GeometryNodeImportText"
            | b"GeometryNodeImportCSV"
            | b"GeometryNodeImportVDB"
    )
}
fn index_name(index: &mut NameIndex, code: Vec<u8>, name: Vec<u8>, block: usize) {
    if name.starts_with(&code) {
        index
            .entry(code)
            .or_default()
            .entry(name)
            .or_default()
            .push(block);
    }
}
#[cfg(test)]
mod selection_contract {
    use super::*;
    #[test]
    fn index_retains_literal_selector_duplicates_mismatches_and_raw_names() {
        let records: Vec<(&[u8], Option<&[u8]>)> = vec![
            (b"OB", Some(b"OBtree")),
            (b"OB", Some(b"OBtree")),
            (b"MA", Some(b"MAmat")),
            (b"OB", Some(b"MAmat")),
            (b"", Some(b"-unnamed-")),
            (b"XYZ", Some(b"XYZid")),
            (b"ABCD", Some(b"ABCDlong")),
            (b"ID", Some(b"OBlinked")),
            (b"OB", None),
            (b"OB", Some(b"OB\xffraw")),
        ];
        let mut index = NameIndex::new();
        for (i, (code, name)) in records.iter().enumerate() {
            if let Some(name) = name {
                index_name(&mut index, code.to_vec(), name.to_vec(), i);
            }
        }
        for names in [
            vec![],
            vec![b"OBtree".to_vec()],
            vec![b"MAmat".to_vec(), b"OBtree".to_vec(), b"OBtree".to_vec()],
            vec![
                b"-unnamed-".to_vec(),
                b"XYZid".to_vec(),
                b"ABCDlong".to_vec(),
                b"OBlinked".to_vec(),
                b"OB\xffraw".to_vec(),
            ],
        ] {
            let expected: HashSet<_> = records
                .iter()
                .enumerate()
                .filter_map(|(i, (code, stored))| {
                    names
                        .iter()
                        .any(|name| {
                            name.starts_with(code) && stored.is_some_and(|id| id == name.as_slice())
                        })
                        .then_some(i)
                })
                .collect();
            assert_eq!(selected_blocks(&index, &names), expected);
        }
    }
}
fn selected_blocks(index: &NameIndex, names: &[Vec<u8>]) -> HashSet<usize> {
    let mut selected = HashSet::new();
    for name in names {
        for (code, entries) in index {
            if name.starts_with(code)
                && let Some(blocks) = entries.get(name)
            {
                selected.extend(blocks);
            }
        }
    }
    selected
}
fn filename_tile(data: &[u8]) -> bool {
    data.rsplit(|b| matches!(b, b'/' | b'\\'))
        .next()
        .unwrap_or(data)
        .windows(b"<UDIM>".len())
        .any(|w| w == b"<UDIM>")
}
fn suffix_name(mut name: Vec<u8>, suffix: &str, index: usize) -> Vec<u8> {
    name.extend_from_slice(format!(".{suffix}[{index}]").as_bytes());
    name
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TraceMode {
    PerFile,
    Recursive,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointerMode {
    Strict,
    NonStrict,
}
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct TraceOptions {
    pub mode: TraceMode,
    pub pointers: PointerMode,
    /// Opt-in reduced compressed-input RSS, with spill read-at overhead.
    pub low_memory: bool,
}
impl TraceOptions {
    pub fn new(mode: TraceMode, pointers: PointerMode) -> Self {
        Self {
            mode,
            pointers,
            low_memory: false,
        }
    }
}
impl Default for TraceOptions {
    fn default() -> Self {
        Self::new(TraceMode::Recursive, PointerMode::Strict)
    }
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FieldReference {
    pub owner_file: Vec<u8>,
    pub block_address: u64,
    pub element: usize,
    pub path: Vec<Vec<u8>>,
    pub name: Vec<u8>,
    pub full_name: Vec<u8>,
    pub offset: usize,
    pub size: usize,
    pub dna_type: Vec<u8>,
    pub pointer: bool,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Usage {
    pub owner_file: Vec<u8>,
    pub library_chain: Vec<Vec<u8>>,
    pub block_code: Vec<u8>,
    pub block_address: u64,
    pub block_offset: usize,
    pub block_type: Vec<u8>,
    pub block_name: Vec<u8>,
    pub asset_path: Vec<u8>,
    pub is_sequence: bool,
    pub is_udim: bool,
    pub optional: Option<bool>,
    pub path_full_field: Option<FieldReference>,
    pub path_dir_field: Option<FieldReference>,
    pub path_base_field: Option<FieldReference>,
}
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum FailureKind {
    Reader(&'static str),
    LibraryReader(&'static str),
    DanglingPointer,
    AmbiguousPointer,
    MissingLibrary,
    MissingId,
    Cycle,
    InvalidGraph,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MissingId {
    pub root_file: Vec<u8>,
    pub id_code: Vec<u8>,
    /// Exact saved ID name, including its two-byte kind prefix.
    pub id_name: Vec<u8>,
    pub referencing_owner_chain: Vec<Vec<u8>>,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TraceFailure {
    pub kind: FailureKind,
    pub owner_file: Vec<u8>,
    pub block_address: Option<u64>,
    pub field_path: Vec<Vec<u8>>,
    pub library_path: Option<Vec<u8>>,
    pub missing_id: Option<MissingId>,
    pub message: String,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TraceWarning {
    MissingLibrary {
        owner_file: Vec<u8>,
        library_path: Vec<u8>,
    },
    SilencedPointer {
        failure: TraceFailure,
    },
    UnknownAssetCode {
        owner_file: Vec<u8>,
        code: Vec<u8>,
    },
}
#[derive(Debug)]
#[non_exhaustive]
pub struct TraceReport {
    pub usages: Vec<Usage>,
    pub errors: Vec<TraceFailure>,
    pub warnings: Vec<TraceWarning>,
    pub complete: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct View {
    file: usize,
    block: usize,
    element: usize,
}
#[derive(PartialEq, Eq, Hash)]
struct Occurrence {
    view: View,
    name: Vec<u8>,
    sites: [Option<Vec<Vec<u8>>>; 3],
}
struct File {
    path: PathBuf,
    chain: Vec<Vec<u8>>,
    reader: Rc<BlendFile>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum LibraryPolicy {
    ReportMissing,
    ExistingOnly,
}
struct Context<'a> {
    inputs: Option<&'a PackInputs>,
    options: TraceOptions,
    library_policy: LibraryPolicy,
    files: Vec<File>,
    by_path: HashMap<PathBuf, usize>,
    report: TraceReport,
    visited: HashSet<View>,
    declared_data: HashSet<View>,
    dedup: HashSet<Occurrence>,
    unknown: HashSet<Vec<u8>>,
    stopped: bool,
}
type T<T> = std::result::Result<T, Box<TraceFailure>>;
fn raw(path: &Path) -> Vec<u8> {
    path.as_os_str().as_bytes().to_vec()
}
fn path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_vec(bytes.to_vec()))
}
pub(crate) fn lexical(p: &Path) -> PathBuf {
    let data = p.as_os_str().as_bytes();
    if data.len() >= 3 && data[0].is_ascii_alphabetic() && data[1] == b':' && data[2] == b'/' {
        let tail = lexical(&path(&data[2..]));
        let mut joined = data[..2].to_vec();
        joined.extend(raw(&tail));
        return path(&joined);
    }
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => (),
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(c.as_os_str()),
        }
    }
    out
}
fn normalize(data: &[u8]) -> Vec<u8> {
    data.iter()
        .map(|&b| if b == b'\\' { b'/' } else { b })
        .collect()
}
fn absolute_bytes(data: &[u8]) -> bool {
    (!data.starts_with(b"//") && data.starts_with(b"/"))
        || (data.len() >= 3 && data[0].is_ascii_alphabetic() && data[1] == b':' && data[2] == b'/')
}
pub(crate) fn resolve(owner: &Path, data: &[u8]) -> PathBuf {
    let data = normalize(data);
    let p = if absolute_bytes(&data) {
        path(&data)
    } else {
        owner
            .parent()
            .unwrap_or(Path::new("/"))
            .join(path(if data.starts_with(b"//") {
                &data[2..]
            } else {
                &data
            }))
    };
    lexical(&p)
}
fn code(data: &[u8; 4]) -> Vec<u8> {
    data.iter().copied().take_while(|b| *b != 0).collect()
}
impl Context<'_> {
    fn open(&self, path: &Path) -> Result<Rc<BlendFile>> {
        match self.inputs {
            Some(inputs) => Ok(inputs.get(path)?.reader.clone()),
            None => Ok(Rc::new(if self.options.low_memory {
                BlendFile::open_trace_low_memory(path)?
            } else {
                BlendFile::open_trace(path)?
            })),
        }
    }
    fn file(&self, v: View) -> &File {
        &self.files[v.file]
    }
    fn fault(&self, v: View, p: &[&str], e: Error) -> Box<TraceFailure> {
        let kind = match e.kind {
            "dangling_pointer" => FailureKind::DanglingPointer,
            "ambiguous_pointer" => FailureKind::AmbiguousPointer,
            _ => FailureKind::Reader(e.kind),
        };
        Box::new(TraceFailure {
            kind,
            owner_file: raw(&self.file(v).path),
            block_address: Some(self.file(v).reader.blocks()[v.block].old_address),
            field_path: p.iter().map(|s| s.as_bytes().to_vec()).collect(),
            library_path: None,
            missing_id: None,
            message: e.message,
        })
    }
    fn invalid(&self, v: View, p: &[&str], message: &str) -> Box<TraceFailure> {
        let mut e = self.fault(v, p, Error::new("invalid_graph", message));
        e.kind = FailureKind::InvalidGraph;
        e
    }
    fn get(&self, v: View, p: &[&str]) -> T<Value> {
        self.file(v)
            .reader
            .get(
                v.block,
                v.element,
                &p.iter()
                    .map(|s| PathStep::Field(s.as_bytes().to_vec()))
                    .collect::<Vec<_>>(),
                true,
            )
            .map_err(|e| self.fault(v, p, e))
    }
    fn field(&self, v: View, p: &[&str]) -> T<FieldReference> {
        let f = self.file(v);
        let schema = f.reader.schema();
        let mut si = f
            .reader
            .struct_index(v.block)
            .map_err(|e| self.fault(v, p, e))?;
        for (i, n) in p.iter().enumerate() {
            let st = &schema.structs[si];
            let field = st
                .by_name
                .get(n.as_bytes())
                .and_then(|i| st.fields.get(*i))
                .ok_or_else(|| self.fault(v, p, Error::new("missing_field", *n)))?;
            if i + 1 == p.len() {
                return Ok(FieldReference {
                    owner_file: raw(&f.path),
                    block_address: f.reader.blocks()[v.block].old_address,
                    element: v.element,
                    path: p.iter().map(|s| s.as_bytes().to_vec()).collect(),
                    name: field.name.clone(),
                    full_name: field.full_name.clone(),
                    offset: field.offset,
                    size: field.size,
                    dna_type: schema.types[field.type_index].clone(),
                    pointer: field.pointer,
                });
            }
            if field.pointer {
                return Err(self.invalid(v, p, "pointer is not an embedded structure"));
            }
            si = *schema
                .by_type
                .get(&field.type_index)
                .ok_or_else(|| self.invalid(v, p, "not embedded structure"))?;
        }
        Err(self.invalid(v, p, "empty field path"))
    }
    fn has(&self, v: View, p: &[&str]) -> bool {
        self.field(v, p).is_ok()
    }
    fn bytes(&self, v: View, p: &[&str]) -> T<Vec<u8>> {
        match self.get(v, p)? {
            Value::Bytes(b) => Ok(b),
            _ => Err(self.invalid(v, p, "expected byte field")),
        }
    }
    fn num(&self, v: View, p: &[&str]) -> T<i64> {
        match self.get(v, p)? {
            Value::Signed(n) => Ok(n),
            Value::Unsigned(n) | Value::Pointer(n) => {
                i64::try_from(n).map_err(|_| self.invalid(v, p, "number exceeds i64"))
            }
            _ => Err(self.invalid(v, p, "expected integer")),
        }
    }
    fn address(&self, v: View, p: &[&str]) -> T<u64> {
        match self.get(v, p)? {
            Value::Pointer(n) => Ok(n),
            _ => Err(self.invalid(v, p, "expected pointer")),
        }
    }
    fn deref(&mut self, v: View, p: &[&str], address: u64) -> T<Option<View>> {
        self.deref_hop(v, p, address, false, None)
    }
    fn deref_hop(
        &mut self,
        v: View,
        p: &[&str],
        address: u64,
        table_entry: bool,
        caller_type: Option<&[u8]>,
    ) -> T<Option<View>> {
        let steps = p
            .iter()
            .map(|n| PathStep::Field(n.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        let reader = &self.file(v).reader;
        let resolved = if let Some(expected) = caller_type {
            // Caller qualification reuses the owning role: node storage is
            // owner-local DATA; Object.data is an ID family discriminated by
            // Object.type. Both prove count and file-local SDNA extent here.
            // This is not write admission or a generic void-pointer fallback.
            (|| {
                if address == 0 {
                    return Ok(None);
                }
                let loc = reader.locate_field(v.block, v.element, &steps)?;
                let declared = &reader.schema().types[loc.field.type_index];
                let q = crate::file_ops::PointerLayout {
                    offset: loc.offset,
                    path: format!("Trace {p:?}"),
                    declared_type: if declared == b"void" {
                        expected.to_vec()
                    } else {
                        declared.clone()
                    },
                    parent_type: loc.parent_type.to_vec(),
                    parent_offset: loc.parent_offset,
                    field_name: loc.field.name.clone(),
                    pointer_depth: loc.field.full_name.iter().filter(|&&c| c == b'*').count(),
                    list_role: loc.list_role.map(|(ty, name)| (ty.to_vec(), name.to_vec())),
                };
                let target = reader
                    .owner_index()?
                    .resolve(reader, v.block, address, &q)?;
                crate::file_ops::roles::qualify(reader, v.block, &q, target)?;
                // ID means the role's discriminated ID family, not a literal
                // struct named ID. qualify above owns that family/header proof
                // (including linked placeholders); concrete storage stays exact.
                if expected != b"ID" && reader.dna_type_id(target)? != expected {
                    return Err(Error::new(
                        "pointer_target_type",
                        "node storage differs from caller-qualified type",
                    ));
                }
                Ok(Some(target))
            })()
            .map_err(|error| {
                if error.kind == "unresolved_persistent" {
                    Error::new("dangling_pointer", error.message)
                } else {
                    error
                }
            })
        } else if table_entry {
            reader.resolve_pointer_element(v.block, v.element, &steps, address)
        } else {
            reader.resolve_pointer(v.block, v.element, &steps, address)
        };
        // Legacy Group membership is a read-only dependency edge. Reuse the
        // declared intrusive-list proof, without admitting it to writes/census.
        let resolved = match resolved {
            Ok(None)
            | Err(Error {
                kind: "unprofiled_role",
                ..
            }) if matches!(p, ["gobject", "first"] | ["gobject", "last"])
                && !table_entry
                && reader.dna_type_id(v.block).is_ok_and(|ty| ty == b"Group") =>
            {
                (|| {
                    // The intrusive helper proves non-null sites. Empty lists
                    // are valid only when both serialized endpoints are null.
                    if address == 0 {
                        let peer = if p[1] == "first" { "last" } else { "first" };
                        if matches!(
                            reader.get(
                                v.block,
                                v.element,
                                &[
                                    PathStep::Field(b"gobject".to_vec()),
                                    PathStep::Field(peer.as_bytes().to_vec())
                                ],
                                true
                            )?,
                            Value::Pointer(0)
                        ) {
                            return Ok(None);
                        }
                    }
                    let path = format!("[{}].gobject.{}", v.element, p[1]);
                    let layout = crate::file_ops::pointer_layout(reader, v.block)?
                        .and_then(|fields| fields.into_iter().find(|field| field.path == path))
                        .ok_or_else(|| Error::new("unprofiled_role", "Group list layout absent"))?;
                    let target = crate::file_ops::roles::declared_intrusive_list_target(
                        reader, v.block, &layout,
                    )
                    .map_err(|error| {
                        if error.kind == "unresolved_persistent" {
                            Error::new("dangling_pointer", error.message)
                        } else {
                            error
                        }
                    })?;
                    if let Some(target) = target
                        && reader.dna_type_id(target)? != b"GroupObject"
                    {
                        return Err(Error::new(
                            "pointer_target_type",
                            "Group membership requires GroupObject",
                        ));
                    }
                    Ok(target)
                })()
            }
            result => result,
        };
        match resolved {
            Ok(b) => Ok(b.map(|block| View {
                file: v.file,
                block,
                element: 0,
            })),
            Err(e) => {
                let failure = self.fault(v, p, e);
                if matches!(failure.kind, FailureKind::DanglingPointer)
                    && self.options.pointers == PointerMode::NonStrict
                {
                    self.report.complete = false;
                    self.report.errors.push((*failure).clone());
                    self.report
                        .warnings
                        .push(TraceWarning::SilencedPointer { failure: *failure });
                    Ok(None)
                } else {
                    Err(failure)
                }
            }
        }
    }
    fn ptr(&mut self, v: View, p: &[&str]) -> T<Option<View>> {
        let a = self.address(v, p)?;
        self.deref(v, p, a)
    }
    fn optptr(&mut self, v: View, p: &[&str]) -> T<Option<View>> {
        if !self.has(v, p) {
            return Ok(None);
        }
        self.ptr(v, p)
    }
    fn list(&mut self, start: Option<View>, next: &[&str]) -> T<Vec<View>> {
        let mut rows = Vec::new();
        let mut seen = HashSet::new();
        let mut current = start;
        while let Some(v) = current {
            if !seen.insert(v) {
                let mut f = self.invalid(v, next, "cyclic ListBase");
                f.kind = FailureKind::Cycle;
                return Err(f);
            }
            rows.push(v);
            let next = if next == ["modifier", "next"] && !self.has(v, next) {
                &["next"][..]
            } else {
                next
            };
            current = self.ptr(v, next)?;
        }
        Ok(rows)
    }
    fn name(&self, v: View) -> T<Vec<u8>> {
        if self.has(v, &["id", "name"]) {
            self.bytes(v, &["id", "name"])
        } else if self.has(v, &["name"]) {
            self.bytes(v, &["name"])
        } else {
            Ok(b"-unnamed-".to_vec())
        }
    }
    fn modifiers(&mut self, v: View) -> T<Vec<View>> {
        let first = self.optptr(v, &["modifiers", "first"])?;
        self.list(first, &["modifier", "next"])
    }
    fn sequences(&mut self, ed: View) -> T<Vec<(View, i64)>> {
        let start = self.optptr(ed, &["seqbase", "first"])?;
        let mut out = Vec::new();
        self.seq_list(start, &mut out, &mut HashSet::new())?;
        Ok(out)
    }
    fn seq_list(
        &mut self,
        start: Option<View>,
        out: &mut Vec<(View, i64)>,
        active: &mut HashSet<View>,
    ) -> T<()> {
        for v in self.list(start, &["next"])? {
            if !active.insert(v) {
                return Err(self.invalid(v, &["seqbase"], "cyclic nested sequencer list"));
            }
            out.push((v, self.num(v, &["type"])?));
            let child = self.optptr(v, &["seqbase", "first"])?;
            self.seq_list(child, out, active)?;
            active.remove(&v);
        }
        Ok(())
    }
    fn emit(
        &mut self,
        v: View,
        asset: Vec<u8>,
        seq: bool,
        name: Option<Vec<u8>>,
        fields: (
            Option<FieldReference>,
            Option<FieldReference>,
            Option<FieldReference>,
        ),
    ) -> T<()> {
        if asset.is_empty() {
            return Ok(());
        }
        let (full, dir, base) = fields;
        let name = match name {
            Some(n) => n,
            None => self.name(v)?,
        };
        let b = &self.file(v).reader.blocks()[v.block];
        let code = code(&b.code);
        let address = b.old_address;
        let offset = b.data_offset;
        if self.options.mode == TraceMode::Recursive
            && !self.dedup.insert(Occurrence {
                view: v,
                name: name.clone(),
                sites: [&full, &dir, &base].map(|f| f.as_ref().map(|f| f.path.clone())),
            })
        {
            return Ok(());
        }
        let asset = normalize(&asset);
        let udim = seq && filename_tile(&asset);
        let si = self
            .file(v)
            .reader
            .struct_index(v.block)
            .map_err(|e| self.fault(v, &[], e))?;
        let schema = self.file(v).reader.schema();
        let record_size = schema.lengths[schema.structs[si].type_index];
        let usage = Usage {
            owner_file: raw(&self.file(v).path),
            library_chain: self.file(v).chain.clone(),
            block_code: code,
            block_address: address,
            block_offset: offset + v.element * record_size,
            block_type: self
                .file(v)
                .reader
                .dna_type_id(v.block)
                .map_err(|e| self.fault(v, &[], e))?
                .to_vec(),
            block_name: name,
            asset_path: asset,
            is_sequence: seq,
            is_udim: udim,
            optional: None,
            path_full_field: full,
            path_dir_field: dir,
            path_base_field: base,
        };
        self.report.usages.push(usage);
        Ok(())
    }
    fn full(&mut self, v: View, p: &[&str], seq: bool, name: Option<Vec<u8>>) -> T<()> {
        let asset = self.bytes(v, p)?;
        let field = self.field(v, p)?;
        self.emit(v, asset, seq, name, (Some(field), None, None))
    }
    fn packed(&self, v: View) -> T<bool> {
        for p in [&["packedfile"][..], &["packedfiles", "first"][..]] {
            if self.has(v, p) && self.address(v, p)? != 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn image_ref(&mut self, v: Option<View>, p: &[&str], name: Vec<u8>) -> T<()> {
        if let Some(v) = v {
            let image = self.optptr(v, p)?;
            if let Some(image) = image {
                self.image_asset(image, Some(name))?;
            }
        }
        Ok(())
    }
    fn image_asset(&mut self, v: View, name: Option<Vec<u8>>) -> T<()> {
        let source = self.num(v, &["source"])?;
        if matches!(source, 1 | 2 | 3 | 6) && !self.packed(v)? {
            let p = if self.has(v, &["filepath"]) {
                "filepath"
            } else {
                "name"
            };
            self.full(v, &[p], matches!(source, 2 | 6), name)?;
        }
        Ok(())
    }
    fn pointcache(&mut self, owner: View, pc: View, name: Vec<u8>, ext: &[u8]) -> T<()> {
        // Flags qualified by separately saved memory/disk/external Blender files.
        let flag = self.num(pc, &["flag"])?;
        if flag & 512 != 0 {
            return self.full(pc, &["path"], true, Some(name));
        }
        if flag & 64 == 0 {
            return Ok(());
        }
        let mut cache_name = self.bytes(pc, &["name"])?;
        if cache_name.is_empty() {
            let owner_name = self.name(owner)?;
            cache_name = hex(owner_name.get(2..).unwrap_or(&owner_name))
                .to_uppercase()
                .into_bytes();
        }
        let stem = self
            .file(owner)
            .path
            .file_stem()
            .map_or(&[][..], OsStrExt::as_bytes);
        let mut asset = b"//blendcache_".to_vec();
        asset.extend_from_slice(stem);
        asset.push(b'/');
        asset.extend(cache_name);
        asset.extend_from_slice(b"_*");
        asset.extend_from_slice(ext);
        let field = self.field(pc, &["name"])?;
        self.emit(pc, asset, true, Some(name), (Some(field), None, None))
    }
    fn ty(&self, v: View) -> T<Vec<u8>> {
        self.file(v)
            .reader
            .dna_type_id(v.block)
            .map(Vec::from)
            .map_err(|e| self.fault(v, &[], e))
    }
    fn string_pointer(&mut self, v: View, p: &[&str]) -> T<Vec<u8>> {
        let Some(s) = self.optptr(v, p)? else {
            return Ok(Vec::new());
        };
        let bytes = self
            .file(s)
            .reader
            .payload(s.block)
            .map_err(|e| self.fault(v, p, e))?;
        Ok(bytes.iter().copied().take_while(|b| *b != 0).collect())
    }
    fn pointer_path(&mut self, v: View, p: &[&str], seq: bool, name: Vec<u8>) -> T<()> {
        let asset = self.string_pointer(v, p)?;
        let field = self.field(v, p)?;
        self.emit(v, asset, seq, Some(name), (Some(field), None, None))
    }
    fn modifier(&mut self, owner: View, m: View, name: Vec<u8>) -> T<()> {
        let ty = self.ty(m)?;
        let discriminator = match ty.as_slice() {
            b"MeshCacheModifierData" => 46,
            b"MeshSeqCacheModifierData" => 52,
            b"OceanModifierData" => 39,
            b"DisplaceModifierData" => 14,
            b"WaveModifierData" => 7,
            b"UVProjectModifierData" => 15,
            b"ClothModifierData" => 22,
            b"ParticleSystemModifierData" => 19,
            b"FluidModifierData" => 56,
            b"SmokeModifierData" => 31,
            b"DynamicPaintModifierData" => 40,
            b"NodesModifierData" => 57,
            _ => return Ok(()),
        };
        if self.num(m, &["modifier", "type"])? != discriminator {
            return Ok(());
        }
        match ty.as_slice() {
            b"MeshCacheModifierData" => self.full(m, &["filepath"], false, Some(name))?,
            b"MeshSeqCacheModifierData" => {
                // The modifier is a reference site, but CacheFile owns the
                // physical filepath/provenance. Enumerate it here, before the
                // later standalone CF BHead. Recursive emit deduplicates that
                // same write site; per-file trace retains each reference site.
                if let Some(cache) = self.optptr(m, &["cache_file"])? {
                    let seq = self.num(cache, &["is_sequence"])? != 0;
                    self.full(cache, &["filepath"], seq, None)?;
                }
            }
            b"OceanModifierData" => {
                if self.num(m, &["cached"])? != 0 {
                    self.full(m, &["cachepath"], true, Some(name))?;
                }
            }
            b"DisplaceModifierData" | b"WaveModifierData" => {
                let tex = self.optptr(m, &["texture"])?;
                self.image_ref(tex, &["ima"], name)?;
            }
            b"UVProjectModifierData" => self.image_ref(Some(m), &["image"], name)?,
            b"ClothModifierData" => {
                if let Some(pc) = self.optptr(m, &["point_cache"])? {
                    self.pointcache(owner, pc, name, b".bphys")?;
                }
            }
            b"ParticleSystemModifierData" => {
                if let Some(ps) = self.optptr(m, &["psys"])? {
                    if let Some(pc) = self.optptr(ps, &["pointcache"])? {
                        self.pointcache(owner, pc, name.clone(), b".bphys")?;
                    }
                    if let Some(cloth) = self.optptr(ps, &["clmd"])?
                        && let Some(pc) = self.optptr(cloth, &["point_cache"])?
                    {
                        self.pointcache(owner, pc, name, b".bphys")?;
                    }
                }
            }
            b"FluidModifierData" => {
                if let Some(domain) = self.optptr(m, &["domain"])? {
                    self.full(domain, &["cache_directory"], true, Some(name))?;
                }
            }
            b"SmokeModifierData" => {
                if let Some(domain) = self.optptr(m, &["domain"])? {
                    let format = if self.has(domain, &["cache_file_format"]) {
                        self.num(domain, &["cache_file_format"])?
                    } else {
                        0
                    };
                    let ext = match format {
                        0 => b".bphys".as_slice(),
                        1 => b".vdb".as_slice(),
                        _ => {
                            return Err(self.invalid(
                                domain,
                                &["cache_file_format"],
                                "unknown smoke cache format",
                            ));
                        }
                    };
                    if let Some(pc) = self.optptr(domain, &["point_cache"])? {
                        self.pointcache(owner, pc, name, ext)?;
                    }
                }
            }
            b"DynamicPaintModifierData" => {
                if let Some(canvas) = self.optptr(m, &["canvas"])? {
                    let start = self.optptr(canvas, &["surfaces", "first"])?;
                    for (i, surface) in self.list(start, &["next"])?.into_iter().enumerate() {
                        if let Some(pc) = self.optptr(surface, &["pointcache"])? {
                            let mut surface_name = name.clone();
                            surface_name.extend_from_slice(b".canvas_settings");
                            self.pointcache(
                                owner,
                                pc,
                                suffix_name(surface_name, "surfaces", i),
                                b".bphys",
                            )?;
                        }
                    }
                }
            }
            b"NodesModifierData" => self.bakes(m, name)?,
            _ => (),
        }
        Ok(())
    }
    fn bakes(&mut self, m: View, name: Vec<u8>) -> T<()> {
        if !self.has(m, &["bakes_num"]) {
            return Ok(());
        }
        let count = self.num(m, &["bakes_num"])?;
        if count < 0 {
            return Err(self.invalid(m, &["bakes_num"], "negative bake count"));
        }
        // Native GN-import files retain an unsaved handle when the logical
        // array is empty. Zero elements means no pointer is dereferenced.
        if count == 0 {
            return Ok(());
        }
        let Some(bakes) = self.optptr(m, &["bakes"])? else {
            return Ok(());
        };
        if count as usize > self.file(bakes).reader.blocks()[bakes.block].count {
            return Err(self.invalid(m, &["bakes"], "bake count exceeds saved array"));
        }
        for i in 0..count as usize {
            let bake = View {
                element: i,
                ..bakes
            };
            if self.has(bake, &["packed"]) && self.address(bake, &["packed"])? != 0 {
                continue;
            }
            // Older saved layouts have disk directories but no target enum.
            // Never interpret their padding as a modern bake_target byte.
            let target = if self.has(bake, &["bake_target"]) {
                self.num(bake, &["bake_target"])?
            } else {
                2
            };
            let target = if target == 0 {
                if self.has(m, &["bake_target"]) {
                    self.num(m, &["bake_target"])?
                } else {
                    2
                }
            } else {
                target
            };
            if target != 2 {
                continue;
            }
            let usage_name = suffix_name(name.clone(), "bakes", i);
            if self.num(bake, &["flag"])? & 2 != 0 {
                self.pointer_path(bake, &["directory"], true, usage_name)?;
            } else {
                self.pointer_path(m, &["simulation_bake_directory"], true, usage_name)?;
            }
        }
        Ok(())
    }
    fn assets(&mut self, v: View) -> T<()> {
        let b = &self.file(v).reader.blocks()[v.block];
        if b.count == 0 || b.code == *b"DNA1" {
            return Ok(());
        }
        let block_code = code(&b.code);
        let ty = self.ty(v)?;
        if block_code.len() != 2 && ty != b"bNodeTree" {
            return Ok(());
        }
        if block_code.len() == 2
            && !matches!(
                block_code.as_slice(),
                b"IM"
                    | b"VF"
                    | b"LI"
                    | b"SO"
                    | b"MC"
                    | b"VO"
                    | b"CF"
                    | b"ME"
                    | b"OB"
                    | b"SC"
                    | b"NT"
                    | b"ID"
                    | b"TX"
                    | b"PY"
            )
        {
            self.unknown_asset(v, block_code);
            return Ok(());
        }
        // Linked placeholders are not physical assets in the referencing file.
        if ty == b"ID" {
            return Ok(());
        }
        if self.has(v, &["id", "lib"]) && self.address(v, &["id", "lib"])? != 0 {
            return Ok(());
        }
        match ty.as_slice() {
            b"Image" => self.image_asset(v, None)?,
            b"VFont" => {
                if !self.packed(v)? && self.bytes(v, &["name"])? != b"<builtin>" {
                    self.full(v, &["name"], false, None)?;
                }
            }
            b"Library" | b"bSound" | b"MovieClip" => {
                if !self.packed(v)? {
                    self.full(v, &["name"], false, None)?;
                }
            }
            b"Text" => {
                let field = if self.has(v, &["filepath"]) {
                    "filepath"
                } else {
                    "name"
                };
                self.pointer_path(v, &[field], false, self.name(v)?)?;
            }
            b"Script" => self.full(v, &["scriptname"], false, None)?,
            b"Volume" | b"CacheFile" => {
                if !self.packed(v)? {
                    let seq = self.has(v, &["is_sequence"]) && self.num(v, &["is_sequence"])? != 0;
                    self.full(v, &["filepath"], seq, None)?;
                }
            }
            b"Mesh" => {
                for field in ["vdata", "edata", "fdata", "pdata", "ldata"] {
                    if let Some(external) = self.optptr(v, &[field, "external"])? {
                        self.full(external, &["filename"], false, Some(self.name(v)?))?;
                    }
                }
            }
            b"Object" => {
                let name = self.name(v)?;
                if let Some(soft) = self.optptr(v, &["soft"])?
                    && let Some(pc) = self.optptr(soft, &["pointcache"])?
                {
                    let mut cache_name = name.clone();
                    cache_name.extend_from_slice(b".soft_body");
                    self.pointcache(v, pc, cache_name, b".bphys")?;
                }
                for (i, m) in self.modifiers(v)?.into_iter().enumerate() {
                    self.modifier(v, m, suffix_name(name.clone(), "modifiers", i))?;
                }
            }
            b"Scene" => {
                if let Some(ed) = self.optptr(v, &["ed"])? {
                    let start = self.optptr(ed, &["seqbase", "first"])?;
                    self.seq_assets(start, &mut HashSet::new())?;
                }
            }
            b"bNodeTree" => self.import_assets(v)?,
            _ => self.unknown_asset(v, block_code),
        }
        Ok(())
    }
    fn unknown_asset(&mut self, v: View, code: Vec<u8>) {
        if code.len() == 2
            && !matches!(
                code.as_slice(),
                b"GR" | b"CO" | b"WO" | b"LA" | b"WM" | b"SN" | b"LS" | b"ID"
            )
            && self.unknown.insert(code.clone())
        {
            self.report.warnings.push(TraceWarning::UnknownAssetCode {
                owner_file: raw(&self.file(v).path),
                code,
            });
        }
    }
    // Both root asset enumeration and selected-library DATA expansion use this
    // interpretation. The saved declaration stays void*: only this known caller
    // supplies the concrete type; deref_hop proves owner-local DATA/type/count
    // and the target extent against this file's SDNA. No write admission.
    fn node_storage(&mut self, node: View) -> T<Option<View>> {
        if self.bytes(node, &["idname"])? == b"ShaderNodeScript" {
            if self.num(node, &["type"])? != 171 {
                return Err(self.fault(
                    node,
                    &["storage"],
                    Error::new("unprofiled_role", "script node discriminators disagree"),
                ));
            }
            let address = self.address(node, &["storage"])?;
            self.deref_hop(
                node,
                &["storage"],
                address,
                false,
                Some(b"NodeShaderScript"),
            )
        } else {
            self.optptr(node, &["storage"])
        }
    }
    fn import_assets(&mut self, v: View) -> T<()> {
        let first = self.optptr(v, &["nodes", "first"])?;
        for node in self.list(first, &["next"])? {
            let kind = self.bytes(node, &["idname"])?;
            if kind == b"ShaderNodeScript" {
                if let Some(storage) = self.node_storage(node)? {
                    match self.num(storage, &["mode"])? {
                        0 => (), // Internal Text (node.id), not the saved filepath.
                        1 => {
                            let mut name = self.name(v)?;
                            name.extend_from_slice(b".script.");
                            name.extend(self.bytes(node, &["name"])?);
                            self.full(storage, &["filepath"], false, Some(name))?;
                        }
                        _ => {
                            return Err(self.fault(
                                storage,
                                &["mode"],
                                Error::new("unprofiled_role", "unknown script storage mode"),
                            ));
                        }
                    }
                }
                continue;
            }
            if !geometry_import(&kind) {
                continue;
            }
            let first = self.optptr(node, &["inputs", "first"])?;
            for socket in self.list(first, &["next"])? {
                if self.bytes(socket, &["name"])? != b"Path" {
                    continue;
                }
                if let Some(value) = self.optptr(socket, &["default_value"])? {
                    let mut name = self.name(v)?;
                    name.extend_from_slice(b".import.");
                    name.extend(self.bytes(node, &["name"])?);
                    self.full(value, &["value"], false, Some(name))?;
                }
            }
        }
        Ok(())
    }
    fn seq_assets(&mut self, start: Option<View>, active: &mut HashSet<View>) -> T<()> {
        let mut sequences = Vec::new();
        self.seq_list(start, &mut sequences, active)?;
        for (s, ty) in sequences {
            self.seq_asset(s, ty)?;
        }
        Ok(())
    }
    fn seq_asset(&mut self, s: View, ty: i64) -> T<()> {
        // Saved image, movie and sound strips have a split physical write site.
        if !matches!(ty, 0 | 3 | 4) {
            return Ok(());
        }
        let Some(strip) = self.optptr(s, &["strip"])? else {
            return Ok(());
        };
        let Some(elem) = self.optptr(strip, &["stripdata"])? else {
            return Ok(());
        };
        let mut dir = normalize(&self.bytes(strip, &["dir"])?);
        while dir.last() == Some(&b'/') {
            dir.pop();
        }
        dir.push(b'/');
        dir.extend(self.bytes(elem, &["name"])?);
        let dir_field = self.field(strip, &["dir"])?;
        let base_field = self.field(elem, &["name"])?;
        self.emit(
            strip,
            dir,
            ty == 0,
            None,
            (None, Some(dir_field), Some(base_field)),
        )
    }
    fn addptr(&mut self, v: View, p: &[&str], out: &mut Vec<View>, optional: bool) -> T<()> {
        let next = if optional {
            self.optptr(v, p)?
        } else {
            self.ptr(v, p)?
        };
        if let Some(n) = next {
            out.push(n)
        }
        Ok(())
    }
    fn animation(&mut self, v: View, out: &mut Vec<View>) -> T<()> {
        self.addptr(v, &["adt"], out, true)
    }
    fn materials(&mut self, v: View, out: &mut Vec<View>) -> T<()> {
        if !self.has(v, &["mat"]) || !self.has(v, &["totcol"]) {
            return Ok(());
        }
        let count = self.num(v, &["totcol"])?;
        if count < 0 {
            return Err(self.invalid(v, &["totcol"], "negative material count"));
        }
        let Some(table) = self.optptr(v, &["mat"])? else {
            if count != 0 {
                return Err(self.invalid(v, &["mat"], "material count without pointer table"));
            }
            return Ok(());
        };
        let width = self.file(v).reader.header().pointer_size;
        let size = self.file(table).reader.blocks()[table.block].size;
        if count as usize > size / width {
            return Err(self.invalid(v, &["mat"], "material table exceeds saved bytes"));
        }
        for i in 0..count as usize {
            let address = self
                .file(table)
                .reader
                .primitive(table.block, i * width, width)
                .map_err(|e| self.fault(v, &["mat"], e))?;
            if let Some(target) = self.deref_hop(v, &["mat"], address, true, None)? {
                out.push(target);
            }
        }
        Ok(())
    }
    fn mtex(&mut self, v: View, out: &mut Vec<View>) -> T<()> {
        if !self.has(v, &["mtex"]) {
            return Ok(());
        }
        self.pointer_array(v, &["mtex"], out)
    }
    fn pointer_array(&mut self, v: View, p: &[&str], out: &mut Vec<View>) -> T<()> {
        let field = self.field(v, p)?;
        let width = self.file(v).reader.header().pointer_size;
        let steps = p
            .iter()
            .map(|s| PathStep::Field(s.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        let (offset, count) = {
            let location = self
                .file(v)
                .reader
                .locate_field(v.block, v.element, &steps)
                .map_err(|e| self.fault(v, p, e))?;
            (location.offset, location.field.array_len)
        };
        if !field.pointer {
            return Err(self.invalid(v, p, "expected pointer array"));
        }
        for i in 0..count {
            let address = self
                .file(v)
                .reader
                .primitive(v.block, offset + i * width, width)
                .map_err(|e| self.fault(v, p, e))?;
            if let Some(target) = self.deref(v, p, address)? {
                out.push(target);
            }
        }
        Ok(())
    }
    fn node_tree(&mut self, v: View, out: &mut Vec<View>) -> T<()> {
        let start = self.optptr(v, &["nodes", "first"])?;
        out.extend(self.list(start, &["next"])?);
        Ok(())
    }
    fn node_owner(&mut self, v: View, out: &mut Vec<View>) -> T<()> {
        for p in ["nodetree", "compositing_node_group"] {
            self.addptr(v, &[p], out, true)?;
        }
        Ok(())
    }
    fn group(&mut self, v: View, out: &mut Vec<View>, active: &mut HashSet<View>) -> T<()> {
        if !active.insert(v) {
            return Ok(());
        }
        match self.num(v, &["type"])? {
            7 => self.addptr(v, &["data", "pointer"], out, false)?,
            6 => {
                let first = self.optptr(v, &["data", "group", "first"])?;
                for child in self.list(first, &["next"])? {
                    self.group(child, out, active)?;
                }
            }
            9 => {
                if let Some(array) = self.optptr(v, &["data", "pointer"])? {
                    let count = self.num(v, &["len"])?;
                    if count < 0
                        || count as usize > self.file(array).reader.blocks()[array.block].count
                    {
                        return Err(self.invalid(
                            v,
                            &["len"],
                            "property array exceeds saved records",
                        ));
                    }
                    for i in 0..count as usize {
                        self.group(
                            View {
                                element: i,
                                ..array
                            },
                            out,
                            active,
                        )?;
                    }
                }
            }
            _ => (),
        }
        active.remove(&v);
        Ok(())
    }
    fn addlist(&mut self, v: View, p: &[&str], next: &[&str], out: &mut Vec<View>) -> T<()> {
        let first = self.optptr(v, p)?;
        out.extend(self.list(first, next)?);
        Ok(())
    }
    fn id_fields(&mut self, v: View, out: &mut Vec<View>) -> T<()> {
        let fields = {
            let reader = &self.file(v).reader;
            let schema = reader.schema();
            let si = reader
                .struct_index(v.block)
                .map_err(|e| self.fault(v, &[], e))?;
            let ty = schema.types[schema.structs[si].type_index].as_slice();
            schema.structs[si]
                .fields
                .iter()
                .filter(|f| {
                    // These images are emitted at modifier write sites. Adding
                    // an independent texture/image census would invent a second
                    // occurrence in selected libraries (black-box-qualified).
                    if (ty == b"Object" && f.name == b"parent")
                        || (ty.ends_with(b"ModifierData")
                            && matches!(schema.types[f.type_index].as_slice(), b"Tex" | b"Image"))
                    {
                        return false;
                    }
                    if !f.pointer
                        || f.full_name.iter().filter(|b| **b == b'*').count() != 1
                        || f.name == b"lib"
                        || f.name == b"next"
                        || f.name == b"prev"
                        || f.name == b"owner_id"
                    {
                        return false;
                    }
                    schema.types[f.type_index] == b"ID"
                        || schema.by_type.get(&f.type_index).is_some_and(|i| {
                            schema.structs[*i].fields.first().is_some_and(|head| {
                                !head.pointer && schema.types[head.type_index] == b"ID"
                            })
                        })
                })
                .map(|f| f.name.clone())
                .collect::<Vec<_>>()
        };
        for field in fields {
            let name = std::str::from_utf8(&field)
                .map_err(|_| self.invalid(v, &[], "non-UTF8 schema field role"))?;
            self.pointer_array(v, &[name], out)?;
        }
        Ok(())
    }
    fn declared_data_edges(&mut self, v: View, out: &mut Vec<View>) -> T<()> {
        fn unhandled_path(f: &BlendFile, ti: usize, visiting: &mut HashSet<usize>) -> bool {
            if !visiting.insert(ti) {
                return true;
            }
            let result = f.schema().by_type.get(&ti).is_some_and(|si| {
                f.schema().structs[*si].fields.iter().any(|field| {
                    if field.pointer {
                        return false;
                    }
                    let byte = matches!(
                        f.schema().types[field.type_index].as_slice(),
                        b"char" | b"uchar" | b"int8_t" | b"uint8_t"
                    );
                    let path = [b"path".as_slice(), b"file", b"dir"].iter().any(|token| {
                        field
                            .name
                            .windows(token.len())
                            .any(|part| part.eq_ignore_ascii_case(token))
                    });
                    (byte && field.full_name.contains(&b'[') && path)
                        || unhandled_path(f, field.type_index, visiting)
                })
            });
            visiting.remove(&ti);
            result
        }
        let reader = self.file(v).reader.clone();
        let si = reader
            .struct_index(v.block)
            .map_err(|e| self.fault(v, &[], e))?;
        if unhandled_path(
            &reader,
            reader.schema().structs[si].type_index,
            &mut HashSet::new(),
        ) {
            return Err(self.fault(
                v,
                &[],
                Error::new(
                    "unprofiled_role",
                    "declared DATA contains an unhandled path-like field",
                ),
            ));
        }
        let layout = crate::file_ops::pointer_layout(&reader, v.block)
            .map_err(|e| self.fault(v, &[], e))?
            .ok_or_else(|| self.invalid(v, &[], "declared DATA lacks typed pointer layout"))?;
        for field in layout {
            let address = crate::checked::number(
                &reader.payload(v.block).map_err(|e| self.fault(v, &[], e))?,
                field.offset,
                reader.header().pointer_size,
                reader.header().little_endian,
            )
            .map_err(|e| self.fault(v, &[&field.path], e))?;
            if address == 0 {
                continue;
            }
            let result = (|| {
                let target = reader
                    .owner_index()?
                    .resolve(&reader, v.block, address, &field)?;
                crate::file_ops::roles::qualify(&reader, v.block, &field, target)?;
                Ok::<_, Error>(target)
            })();
            let result = match result {
                Err(error) if error.kind == "unprofiled_role" => {
                    match crate::file_ops::roles::declared_intrusive_list_target(
                        &reader, v.block, &field,
                    ) {
                        Ok(Some(target)) => Ok(target),
                        Ok(None) => Err(error),
                        Err(error) => Err(error),
                    }
                }
                result => result,
            };
            let block = match result {
                Ok(block) => block,
                Err(error) => {
                    // Owner lookup exposes its lower-level SDK classification;
                    // trace keeps the same strict/non-strict dangling policy as
                    // resolve_pointer, without changing SDK error kinds.
                    let error = if error.kind == "unresolved_persistent" {
                        Error::new("dangling_pointer", error.message)
                    } else {
                        error
                    };
                    let failure = self.fault(v, &[&field.path], error);
                    if matches!(failure.kind, FailureKind::DanglingPointer)
                        && self.options.pointers == PointerMode::NonStrict
                    {
                        self.report.complete = false;
                        self.report.errors.push((*failure).clone());
                        self.report
                            .warnings
                            .push(TraceWarning::SilencedPointer { failure: *failure });
                        continue;
                    }
                    return Err(failure);
                }
            };
            // A qualified opaque byte/numeric allocation is a leaf, not an
            // instance of struct zero (which may coincidentally be an ID).
            if reader.blocks()[block].sdna_index == 0 {
                continue;
            }
            let target = View {
                file: v.file,
                block,
                element: 0,
            };
            if reader.blocks()[block].code == *b"DATA" {
                self.declared_data.insert(target);
            }
            out.push(target);
        }
        Ok(())
    }
    fn expand(&mut self, v: View) -> T<Vec<View>> {
        let mut out = Vec::new();
        if self.declared_data.contains(&v) {
            self.declared_data_edges(v, &mut out)?;
        }
        self.id_fields(v, &mut out)?;
        self.animation(v, &mut out)?;
        self.materials(v, &mut out)?;
        self.mtex(v, &mut out)?;
        self.node_owner(v, &mut out)?;
        for p in [
            &["id", "properties"][..],
            &["id", "system_properties"][..],
            &["prop"][..],
            &["system_properties"][..],
        ] {
            if let Some(prop) = self.optptr(v, p)? {
                self.group(prop, &mut out, &mut HashSet::new())?;
            }
        }
        match self.ty(v)?.as_slice() {
            b"Object" => {
                // Object.data is a registered ID union, although SDNA declares
                // void*. Its read role owns type/family/count/extent checks.
                if self.has(v, &["data"]) {
                    let address = self.address(v, &["data"])?;
                    if let Some(data) = self.deref_hop(v, &["data"], address, false, Some(b"ID"))? {
                        out.push(data);
                    }
                }
                out.extend(self.modifiers(v)?);
                self.addlist(v, &["particlesystem", "first"], &["next"], &mut out)?;
                self.addlist(v, &["constraints", "first"], &["next"], &mut out)?;
                for p in ["pose", "soft", "pd"] {
                    self.addptr(v, &[p], &mut out, true)?;
                }
            }
            b"Scene" => {
                self.addlist(v, &["base", "first"], &["next"], &mut out)?;
                self.addlist(v, &["view_layers", "first"], &["next"], &mut out)?;
                if let Some(ed) = self.optptr(v, &["ed"])? {
                    out.extend(self.sequences(ed)?.into_iter().map(|(s, _)| s));
                }
            }
            b"Collection" | b"Group" => {
                self.addlist(v, &["gobject", "first"], &["next"], &mut out)?;
                self.addlist(v, &["children", "first"], &["next"], &mut out)?;
            }
            b"bNodeTree" => self.node_tree(v, &mut out)?,
            b"bNode" => {
                for p in ["inputs", "outputs"] {
                    self.addlist(v, &[p, "first"], &["next"], &mut out)?;
                }
                if let Some(storage) = self.node_storage(v)? {
                    out.push(storage);
                }
            }
            b"bNodeSocket" => {
                if let Some(value) = self.optptr(v, &["default_value"])? {
                    let kind = self.bytes(v, &["idname"])?;
                    if crate::file_ops::roles::socket_value(&String::from_utf8_lossy(&kind))
                        .is_none()
                    {
                        self.declared_data.insert(value);
                    }
                    out.push(value);
                }
            }
            b"AnimData" => {
                self.addlist(v, &["nla_tracks", "first"], &["next"], &mut out)?;
                self.addlist(v, &["drivers", "first"], &["next"], &mut out)?;
            }
            b"NlaTrack" | b"NlaStrip" => {
                self.addlist(v, &["strips", "first"], &["next"], &mut out)?
            }
            b"FCurve" => self.addptr(v, &["driver"], &mut out, true)?,
            b"ChannelDriver" => self.addlist(v, &["variables", "first"], &["next"], &mut out)?,
            b"DriverVar" => self.embedded_id_array(v, "targets", &mut out)?,
            b"ViewLayer" => self.addlist(v, &["object_bases", "first"], &["next"], &mut out)?,
            b"bPose" => self.addlist(v, &["chanbase", "first"], &["next"], &mut out)?,
            b"bPoseChannel" => self.addlist(v, &["constraints", "first"], &["next"], &mut out)?,
            b"bConstraint" => {
                if let Some(data) = self.optptr(v, &["data"])? {
                    // Existing specialized cache-constraint semantics remain;
                    // other read-qualified DATA gets generic declared edges.
                    if self.num(v, &["type"])? != 29 {
                        self.declared_data.insert(data);
                    }
                    out.push(data);
                }
            }
            b"NodesModifierData" => {
                if let Some(prop) = self.optptr(v, &["settings", "properties"])? {
                    self.group(prop, &mut out, &mut HashSet::new())?;
                }
            }
            b"ParticleSystem" => self.addlist(v, &["targets", "first"], &["next"], &mut out)?,
            b"IDProperty" => self.group(v, &mut out, &mut HashSet::new())?,
            _ => (),
        }
        out.sort_by_key(|v| {
            (
                self.file(*v).reader.blocks()[v.block].data_offset,
                v.element,
            )
        });
        out.dedup();
        Ok(out)
    }
    fn embedded_id_array(&mut self, v: View, field: &str, out: &mut Vec<View>) -> T<()> {
        if !self.has(v, &[field]) {
            return Ok(());
        }
        let reader = &self.file(v).reader;
        let steps = [PathStep::Field(field.as_bytes().to_vec())];
        let count = reader
            .locate_field(v.block, v.element, &steps)
            .map_err(|e| self.fault(v, &[field], e))?
            .field
            .array_len;
        for i in 0..count {
            let steps = [
                PathStep::Field(field.as_bytes().to_vec()),
                PathStep::Index(i),
                PathStep::Field(b"id".to_vec()),
            ];
            let reader = &self.file(v).reader;
            let target = reader
                .get_pointer(v.block, v.element, &steps)
                .map_err(|e| self.fault(v, &[field, "id"], e))?;
            if let Some(block) = target {
                out.push(View {
                    block,
                    file: v.file,
                    element: 0,
                });
            }
        }
        Ok(())
    }
    fn capture(&mut self, e: TraceFailure) {
        if (self.options.mode == TraceMode::Recursive && !e.is_external_missing())
            || (self.options.pointers == PointerMode::Strict
                && matches!(
                    e.kind,
                    FailureKind::DanglingPointer | FailureKind::AmbiguousPointer
                ))
        {
            self.stopped = true
        }
        self.report.complete = false;
        self.report.errors.push(e)
    }
    fn file_trace(&mut self, fi: usize, names: Option<Vec<Vec<u8>>>) -> T<()> {
        self.file_trace_chain(fi, names, self.files[fi].chain.clone())
    }
    fn file_trace_chain(
        &mut self,
        fi: usize,
        names: Option<Vec<Vec<u8>>>,
        request_chain: Vec<Vec<u8>>,
    ) -> T<()> {
        let reader = self.files[fi].reader.clone();
        let origin = View {
            file: fi,
            block: 0,
            element: 0,
        };
        let mut eligible = Vec::new();
        let asset_roots = names.is_none();
        if asset_roots {
            eligible.extend(reader.blocks().iter().enumerate().filter_map(|(i, b)| {
                (b.count > 0
                    && (code(&b.code).len() == 2
                        || reader.dna_type_id(i).is_ok_and(|ty| ty == b"bNodeTree")))
                .then_some(i)
            }));
        } else if reader.name_index.get().is_none() {
            let mut index = NameIndex::new();
            reader
                .scan_records(|block, record| {
                    let b = &reader.blocks()[block];
                    let code = code(&b.code);
                    if code.len() != 2 {
                        return Ok(());
                    }
                    eligible.push(block);
                    let path = if reader.dna_type_id(block)? == b"ID" {
                        vec![PathStep::Field(b"name".to_vec())]
                    } else {
                        vec![
                            PathStep::Field(b"id".to_vec()),
                            PathStep::Field(b"name".to_vec()),
                        ]
                    };
                    if let Ok(location) = reader.locate_field(block, 0, &path) {
                        let bytes =
                            crate::checked::bytes(record, location.offset, location.field.size)?;
                        let name = bytes.iter().copied().take_while(|b| *b != 0).collect();
                        index_name(&mut index, code, name, block);
                    }
                    Ok(())
                })
                .map_err(|e| self.fault(origin, &[], e))?;
            let _ = reader.name_index.set(index);
        } else {
            eligible.extend(
                reader
                    .blocks()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, b)| (code(&b.code).len() == 2 && b.count > 0).then_some(i)),
            );
        }
        if let Some(requested) = &names {
            for name in requested {
                if selected_blocks(reader.name_index.get().unwrap(), std::slice::from_ref(name))
                    .is_empty()
                {
                    // A corrupt candidate of this family is not absence. Its
                    // missing name must not rescue a malformed native header.
                    for (i, block) in reader.blocks().iter().enumerate() {
                        if name.len() >= 2
                            && block.code[..2] == name[..2]
                            && crate::id_codes::expected(&block.code).is_some()
                        {
                            crate::file_ops::qualify_read_id(&reader, i)
                                .map_err(|e| self.fault(View { block: i, ..origin }, &[], e))?;
                        }
                    }
                    let owners = request_chain[..request_chain.len().saturating_sub(1)].to_vec();
                    let failure = TraceFailure {
                        kind: FailureKind::MissingId,
                        owner_file: owners
                            .last()
                            .cloned()
                            .unwrap_or_else(|| raw(&self.files[fi].path)),
                        block_address: None,
                        field_path: vec![b"id".to_vec(), b"name".to_vec()],
                        library_path: Some(raw(&self.files[fi].path)),
                        missing_id: Some(MissingId {
                            root_file: request_chain[0].clone(),
                            id_code: name.iter().copied().take(2).collect(),
                            id_name: name.clone(),
                            referencing_owner_chain: owners,
                        }),
                        message: format!(
                            "selected linked ID absent from source library: {}",
                            hex(name)
                        ),
                    };
                    self.capture(failure);
                }
            }
        }
        let seeds: BTreeSet<_> = match names {
            Some(names) => selected_blocks(reader.name_index.get().unwrap(), &names)
                .into_iter()
                .collect(),
            None => eligible.into_iter().collect(),
        };
        let mut queue: BTreeSet<View> = seeds
            .into_iter()
            .map(|block| View {
                file: fi,
                block,
                element: 0,
            })
            .collect();
        let mut local = BTreeSet::new();
        let mut libraries: BTreeMap<PathBuf, (View, BTreeSet<Vec<u8>>)> = BTreeMap::new();
        while let Some(v) = queue.pop_first() {
            if self.stopped {
                break;
            }
            if !self.visited.insert(v) {
                continue;
            }
            // These saved ID families have no dependency-tracing role.
            // In particular405 WindowManager carries process-only property
            // addresses without BHeads. Do not turn them into asset edges.
            if matches!(
                code(&reader.blocks()[v.block].code).as_slice(),
                b"WM" | b"SN" | b"LS"
            ) {
                continue;
            }
            // A linked name is an interpretation of the actual library ID,
            // not merely its placeholder in the parent. Share the same native
            // header/count/source-SDNA proof used by Object.data's read role.
            if !asset_roots && crate::id_codes::expected(&reader.blocks()[v.block].code).is_some() {
                crate::file_ops::qualify_read_id(&reader, v.block)
                    .map_err(|e| self.fault(v, &[], e))?;
            }
            let library_path = if self.ty(v)? == b"ID" {
                &["lib"][..]
            } else {
                &["id", "lib"][..]
            };
            let linked = self.has(v, library_path) && self.address(v, library_path)? != 0;
            if linked || self.ty(v)? == b"ID" {
                let Some(lib) = self.optptr(v, library_path)? else {
                    return Err(self.invalid(v, library_path, "linked identifier has no library"));
                };
                let target = self.bytes(lib, &["name"])?;
                if target.is_empty() {
                    return Err(self.invalid(lib, &["name"], "linked library has empty path"));
                }
                let target = resolve(&self.files[fi].path, &target);
                libraries
                    .entry(target)
                    .or_insert_with(|| (lib, BTreeSet::new()))
                    .1
                    .insert(self.name(v)?);
                local.insert(lib);
                continue;
            }
            local.insert(v);
            if !asset_roots {
                match self.expand(v) {
                    Ok(next) => queue.extend(next),
                    Err(e) => self.capture(*e),
                }
            }
        }
        for v in local {
            if self.stopped {
                break;
            }
            // DATA containers are traversed for references, never censused as
            // standalone assets. Embedded node trees are the sole tree owner.
            if (code(&reader.blocks()[v.block].code).len() == 2 || self.ty(v)? == b"bNodeTree")
                && let Err(e) = self.assets(v)
            {
                self.capture(*e);
            }
        }
        // Observable diagnostics follow the saved Library header order, not
        // lexical filesystem order (the latter reorders error/warning arrays).
        let mut libraries: Vec<_> = libraries.into_iter().collect();
        libraries
            .sort_by_key(|(_, (lib, _))| self.file(*lib).reader.blocks()[lib.block].data_offset);
        for (target, (lib, names)) in libraries {
            if self.stopped {
                break;
            }
            // Listing reports existing files, not missing-library diagnostics.
            // Existing but unreadable/malformed libraries still use the normal
            // reader failure path; default dependency tracing is unchanged.
            if self.library_policy == LibraryPolicy::ExistingOnly && !target.exists() {
                continue;
            }
            let mut child_chain = request_chain.clone();
            child_chain.push(raw(&target));
            let child = if let Some(&child) = self.by_path.get(&target) {
                child
            } else {
                let reader = match self.open(&target) {
                    Ok(reader) => reader,
                    Err(error) => {
                        let missing = !target.exists();
                        let kind = if missing {
                            FailureKind::MissingLibrary
                        } else {
                            FailureKind::LibraryReader(error.kind)
                        };
                        let failure = TraceFailure {
                            kind,
                            owner_file: raw(&self.files[fi].path),
                            block_address: Some(
                                self.file(lib).reader.blocks()[lib.block].old_address,
                            ),
                            field_path: vec![b"name".to_vec()],
                            library_path: Some(raw(&target)),
                            missing_id: None,
                            message: error.message,
                        };
                        if missing {
                            self.report.warnings.push(TraceWarning::MissingLibrary {
                                owner_file: failure.owner_file.clone(),
                                library_path: raw(&target),
                            });
                        }
                        self.capture(failure);
                        continue;
                    }
                };
                let child = self.files.len();
                self.files.push(File {
                    path: target.clone(),
                    chain: child_chain.clone(),
                    reader,
                });
                self.by_path.insert(target, child);
                child
            };
            self.file_trace_chain(child, Some(names.into_iter().collect()), child_chain)?;
        }
        Ok(())
    }
}

pub fn trace(root: &Path, options: TraceOptions) -> Result<TraceReport> {
    trace_with_inputs(root, options, None)
}
pub(crate) fn trace_with_inputs(
    root: &Path,
    options: TraceOptions,
    inputs: Option<&PackInputs>,
) -> Result<TraceReport> {
    trace_core(root, options, inputs, LibraryPolicy::ReportMissing).map(|(report, _)| report)
}
pub(crate) fn listing_with_file_paths(
    root: &Path,
    options: TraceOptions,
) -> Result<(TraceReport, Vec<PathBuf>)> {
    trace_core(root, options, None, LibraryPolicy::ExistingOnly)
}
fn trace_core(
    root: &Path,
    options: TraceOptions,
    inputs: Option<&PackInputs>,
    library_policy: LibraryPolicy,
) -> Result<(TraceReport, Vec<PathBuf>)> {
    let root = if root.is_absolute() {
        lexical(root)
    } else {
        lexical(
            &std::env::current_dir()
                .map_err(|e| Error::new("io", e.to_string()))?
                .join(root),
        )
    };
    let reader = match inputs {
        Some(inputs) => inputs.get(&root)?.reader.clone(),
        None => Rc::new(if options.low_memory {
            BlendFile::open_trace_low_memory(&root)?
        } else {
            BlendFile::open_trace(&root)?
        }),
    };
    let mut paths = HashMap::new();
    paths.insert(root.clone(), 0);
    let mut ctx = Context {
        inputs,
        options,
        library_policy,
        files: vec![File {
            path: root.clone(),
            chain: vec![raw(&root)],
            reader,
        }],
        by_path: paths,
        report: TraceReport {
            usages: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            complete: true,
        },
        visited: HashSet::new(),
        declared_data: HashSet::new(),
        dedup: HashSet::new(),
        unknown: HashSet::new(),
        stopped: false,
    };
    if options.mode == TraceMode::PerFile {
        for i in 0..ctx.files[0].reader.blocks().len() {
            if ctx.stopped {
                break;
            }
            let v = View {
                file: 0,
                block: i,
                element: 0,
            };
            if let Err(e) = ctx.assets(v) {
                ctx.capture(*e)
            }
        }
    } else if let Err(e) = ctx.file_trace(0, None) {
        ctx.capture(*e)
    }

    for file in &ctx.files {
        file.reader.data.check_source()?;
        file.reader.data.report(&file.path);
    }
    for failure in &ctx.report.errors {
        if failure.owner_file == raw(&root)
            && let FailureKind::Reader(kind @ ("io" | "truncated")) = failure.kind
        {
            return Err(Error::new(kind, failure.message.clone()));
        }
    }
    let paths = ctx.files.into_iter().map(|file| file.path).collect();
    Ok((ctx.report, paths))
}
#[cfg(test)]
mod serialized_vectors {
    use super::*;

    // Independently authored wire-format vectors; no fixture-generator imports.
    type Structure<'a> = (&'a str, u16, Vec<(&'a str, &'a str)>);
    fn saved(structures: &[Structure<'_>], records: &[(&[u8], u64, usize, Vec<u8>)]) -> Vec<u8> {
        let mut types = vec!["char", "short", "int", "void"];
        let mut lengths = vec![1u16, 2, 4, 0];
        let mut names = Vec::new();
        for (ty, len, fields) in structures {
            types.push(ty);
            lengths.push(*len);
            for (_, name) in fields {
                if !names.contains(name) {
                    names.push(*name);
                }
            }
        }
        let mut dna = b"SDNANAME".to_vec();
        dna.extend_from_slice(&(names.len() as u32).to_le_bytes());
        for name in &names {
            dna.extend_from_slice(name.as_bytes());
            dna.push(0);
        }
        fn align(bytes: &mut Vec<u8>) {
            while !bytes.len().is_multiple_of(4) {
                bytes.push(0);
            }
        }
        align(&mut dna);
        dna.extend_from_slice(b"TYPE");
        dna.extend_from_slice(&(types.len() as u32).to_le_bytes());
        for ty in &types {
            dna.extend_from_slice(ty.as_bytes());
            dna.push(0);
        }
        align(&mut dna);
        dna.extend_from_slice(b"TLEN");
        for length in lengths {
            dna.extend_from_slice(&length.to_le_bytes());
        }
        align(&mut dna);
        dna.extend_from_slice(b"STRC");
        dna.extend_from_slice(&(structures.len() as u32).to_le_bytes());
        for (ty, _, fields) in structures {
            dna.extend_from_slice(
                &(types.iter().position(|t| t == ty).unwrap() as u16).to_le_bytes(),
            );
            dna.extend_from_slice(&(fields.len() as u16).to_le_bytes());
            for (ty, name) in fields {
                dna.extend_from_slice(
                    &(types.iter().position(|t| t == ty).unwrap() as u16).to_le_bytes(),
                );
                dna.extend_from_slice(
                    &(names.iter().position(|n| n == name).unwrap() as u16).to_le_bytes(),
                );
            }
        }
        fn block(
            out: &mut Vec<u8>,
            code: &[u8],
            address: u64,
            si: usize,
            payload: &[u8],
            count: u32,
        ) {
            let mut encoded = [0; 4];
            encoded[..code.len()].copy_from_slice(code);
            out.extend_from_slice(&encoded);
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&address.to_le_bytes());
            out.extend_from_slice(&(si as u32).to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(payload);
        }
        let mut out = b"BLENDER-v405".to_vec();
        for (code, address, si, payload) in records {
            block(&mut out, code, *address, *si, payload, 1);
        }
        block(&mut out, b"DNA1", 0, 0, &dna, 1);
        block(&mut out, b"ENDB", 0, 0, &[], 0);
        out
    }
    fn fixed(data: &[u8], size: usize) -> Vec<u8> {
        let mut out = vec![0; size];
        out[..data.len()].copy_from_slice(data);
        out
    }
    fn images(records: &[(&[u8], &[u8], i16, u64)]) -> Vec<u8> {
        let structures = [
            ("ID", 72, vec![("char", "name[64]"), ("void", "*lib")]),
            ("ListBase", 16, vec![("void", "*first"), ("void", "*last")]),
            (
                "Image",
                354,
                vec![
                    ("ID", "id"),
                    ("char", "name[256]"),
                    ("short", "source"),
                    ("void", "*packedfile"),
                    ("ListBase", "packedfiles"),
                ],
            ),
        ];
        let records = records
            .iter()
            .enumerate()
            .map(|(i, (name, path, source, packed))| {
                let mut payload = fixed(name, 64);
                payload.extend_from_slice(&0u64.to_le_bytes());
                payload.extend(fixed(path, 256));
                payload.extend_from_slice(&source.to_le_bytes());
                payload.extend_from_slice(&packed.to_le_bytes());
                payload.extend_from_slice(&[0; 16]);
                (b"IM".as_slice(), (i + 1) as u64, 2, payload)
            })
            .collect::<Vec<_>>();
        saved(&structures, &records)
    }
    fn context(bytes: Vec<u8>) -> Context<'static> {
        Context {
            inputs: None,
            options: TraceOptions {
                mode: TraceMode::Recursive,
                pointers: PointerMode::Strict,
                low_memory: false,
            },
            library_policy: LibraryPolicy::ReportMissing,
            files: vec![File {
                path: PathBuf::from("/oxblend-test/test.blend"),
                chain: vec![b"/oxblend-test/test.blend".to_vec()],
                reader: Rc::new(BlendFile::from_bytes(bytes).unwrap()),
            }],
            by_path: HashMap::new(),
            report: TraceReport {
                usages: Vec::new(),
                errors: Vec::new(),
                warnings: Vec::new(),
                complete: true,
            },
            visited: HashSet::new(),
            declared_data: HashSet::new(),
            dedup: HashSet::new(),
            unknown: HashSet::new(),
            stopped: false,
        }
    }
    #[test]
    fn missing_library_diagnostics_follow_saved_header_order() {
        let structures = [
            ("ID", 72, vec![("char", "name[64]"), ("Library", "*lib")]),
            ("Library", 200, vec![("ID", "id"), ("char", "name[128]")]),
        ];
        let id = |name: &[u8], lib: u64| {
            let mut data = fixed(name, 64);
            data.extend_from_slice(&lib.to_le_bytes());
            data
        };
        let library = |name: &[u8], path: &[u8]| {
            let mut data = id(name, 0);
            data.extend(fixed(path, 128));
            data
        };
        let mut ctx = context(saved(
            &structures,
            &[
                (b"OB", 1, 0, id(b"OBz", 3)),
                (b"OB", 2, 0, id(b"OBa", 4)),
                (
                    b"LI",
                    3,
                    1,
                    library(b"LIz", b"/oxblend-test/missing-z.blend"),
                ),
                (
                    b"LI",
                    4,
                    1,
                    library(b"LIa", b"/oxblend-test/missing-a.blend"),
                ),
            ],
        ));
        ctx.file_trace(0, None).unwrap();
        assert_eq!(
            ctx.report
                .errors
                .iter()
                .map(|e| e.block_address)
                .collect::<Vec<_>>(),
            vec![Some(3), Some(4)]
        );
        let json = ctx.report.json();
        let warnings = json["warnings"].as_array().unwrap();
        assert_eq!(
            warnings[0]["library_path_hex"],
            hex(b"/oxblend-test/missing-z.blend")
        );
        assert_eq!(
            warnings[1]["library_path_hex"],
            hex(b"/oxblend-test/missing-a.blend")
        );
    }

    #[test]
    fn file_path_handoff_preserves_report_and_returns_loaded_root() {
        use std::io::Write;
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "oxblend-test-trace-paths-{}-{}.blend",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let owned = Owned(path);
        let bytes = images(&[(b"IMstill", b"//still.png", 1, 0)]);
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&owned.0)
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        let options = TraceOptions {
            mode: TraceMode::Recursive,
            pointers: PointerMode::Strict,
            low_memory: false,
        };
        let (report, paths) = listing_with_file_paths(&owned.0, options).unwrap();
        assert_eq!(paths.as_slice(), std::slice::from_ref(&owned.0));
        assert_eq!(report.json(), trace(&owned.0, options).unwrap().json());
    }
    #[test]
    fn tile_detection_is_filename_only_case_sensitive_and_byte_safe() {
        for (p, expected) in [
            (b"//tiles/<UDIM>/still.png".as_slice(), false),
            (b"//tiles/<UDIM>/tile.<UDIM>.exr".as_slice(), true),
            (b"C:\\tiles\\<UDIM>\\still.png".as_slice(), false),
            (b"tile.<udim>.exr".as_slice(), false),
            (b"\xff/tile.<UDIM>.exr".as_slice(), true),
            (b"tile.<UVTILE>.exr".as_slice(), false),
        ] {
            assert_eq!(filename_tile(p), expected, "{p:?}");
        }
    }
    #[test]
    fn census_and_selected_closure_preserve_physical_order_and_raw_names() {
        let bytes = images(&[
            (b"IM\xffsecond", b"//b.png", 1, 0),
            (b"IMfirst", b"//a.png", 1, 0),
        ]);
        let mut census = context(bytes.clone());
        census.file_trace(0, None).unwrap();
        assert_eq!(
            census
                .report
                .usages
                .iter()
                .map(|u| u.asset_path.as_slice())
                .collect::<Vec<_>>(),
            [b"//b.png", b"//a.png"]
        );
        let mut selected = context(bytes);
        selected
            .file_trace(0, Some(vec![b"IMfirst".to_vec(), b"IMfirst".to_vec()]))
            .unwrap();
        selected
            .file_trace(0, Some(vec![b"IMfirst".to_vec()]))
            .unwrap();
        assert_eq!(selected.report.usages.len(), 1);
        assert_eq!(selected.report.usages[0].asset_path, b"//a.png");
    }
    #[test]
    fn image_sources_packing_empty_paths_and_tile_provenance() {
        let bytes = images(&[
            (b"IMfile", b"//tex/a.png", 1, 0),
            (b"IMsequence", b"//tex/a001.png", 2, 0),
            (b"IMmovie", b"//tex/movie.mov", 3, 0),
            (b"IMgenerated", b"//not-resource.png", 4, 0),
            (b"IMviewer", b"//not-resource.png", 5, 0),
            (b"IMtile", b"//<UDIM>/tile.<UDIM>.exr", 6, 0),
            (b"IMancestor", b"//<UDIM>/still.png", 1, 0),
            (b"IMpacked", b"//packed.png", 1, 99),
            (b"IMempty", b"", 1, 0),
        ]);
        let mut c = context(bytes);
        c.file_trace(0, None).unwrap();
        assert_eq!(c.report.usages.len(), 5);
        assert_eq!(
            c.report
                .usages
                .iter()
                .map(|u| (u.is_sequence, u.is_udim))
                .collect::<Vec<_>>(),
            [
                (false, false),
                (true, false),
                (false, false),
                (true, true),
                (false, false)
            ]
        );
        let field = c.report.usages[0].path_full_field.as_ref().unwrap();
        assert_eq!(field.path, [b"name".to_vec()]);
        assert_eq!(field.offset, 72);
        assert_eq!(field.size, 256);
    }
    #[test]
    fn distinct_occurrence_sites_are_not_collapsed() {
        let mut c = context(images(&[(b"IMimage", b"//same.png", 1, 0)]));
        let v = View {
            file: 0,
            block: 0,
            element: 0,
        };
        let field = c.field(v, &["name"]).unwrap();
        c.emit(
            v,
            b"//same.png".to_vec(),
            false,
            Some(b"ownerA".to_vec()),
            (Some(field.clone()), None, None),
        )
        .unwrap();
        c.emit(
            v,
            b"//same.png".to_vec(),
            false,
            Some(b"ownerB".to_vec()),
            (Some(field.clone()), None, None),
        )
        .unwrap();
        c.emit(
            v,
            b"//same.png".to_vec(),
            false,
            Some(b"ownerA".to_vec()),
            (Some(field.clone()), None, None),
        )
        .unwrap();
        let mut alternate = field;
        alternate.path = vec![b"alternate".to_vec()];
        c.emit(
            v,
            b"//same.png".to_vec(),
            false,
            Some(b"ownerA".to_vec()),
            (Some(alternate), None, None),
        )
        .unwrap();
        assert_eq!(c.report.usages.len(), 3);
    }
    #[test]
    fn ignored_metadata_ids_do_not_dereference_process_properties() {
        let structures = [
            (
                "ID",
                80,
                vec![
                    ("char", "name[64]"),
                    ("void", "*lib"),
                    ("IDProperty", "*properties"),
                ],
            ),
            ("ListBase", 16, vec![("void", "*first"), ("void", "*last")]),
            ("IDProperty", 8, vec![("void", "*next")]),
            ("WindowManager", 80, vec![("ID", "id")]),
            (
                "Image",
                362,
                vec![
                    ("ID", "id"),
                    ("char", "name[256]"),
                    ("short", "source"),
                    ("void", "*packedfile"),
                    ("ListBase", "packedfiles"),
                ],
            ),
        ];
        let mut image = fixed(b"IMvisible", 64);
        image.extend_from_slice(&[0; 16]);
        image.extend(fixed(b"//visible.png", 256));
        image.extend_from_slice(&1i16.to_le_bytes());
        image.extend_from_slice(&[0; 24]);
        let mut records = Vec::new();
        for (i, code) in [b"WM".as_slice(), b"SN", b"LS"].into_iter().enumerate() {
            let mut payload = fixed(code, 64);
            payload.extend_from_slice(&0u64.to_le_bytes());
            payload.extend_from_slice(&u64::MAX.to_le_bytes());
            records.push((code, (i + 1) as u64, 3, payload));
        }
        records.push((b"IM", 4, 4, image.clone()));
        let mut c = context(saved(&structures, &records));
        c.file_trace(0, None).unwrap();
        assert!(c.report.complete);
        assert!(c.report.errors.is_empty());
        assert_eq!(c.report.usages.len(), 1);
        assert_eq!(c.report.usages[0].asset_path, b"//visible.png");
        image[72..80].copy_from_slice(&u64::MAX.to_le_bytes());
        let mut c = context(saved(
            &structures,
            &[(b"IM".as_slice(), 4, 4, image.clone())],
        ));
        c.file_trace(0, None).unwrap();
        assert!(c.report.complete);
        assert!(c.report.errors.is_empty());
        assert_eq!(c.report.usages.len(), 1);
        // A genuine file dependency's library pointer remains checked. This
        // is a root-boundary change, never a general dangling-pointer ignore.
        image[64..72].copy_from_slice(&u64::MAX.to_le_bytes());
        let mut c = context(saved(&structures, &[(b"IM".as_slice(), 4, 4, image)]));
        let error = c.file_trace(0, None).unwrap_err();
        assert!(matches!(error.kind, FailureKind::DanglingPointer));
        assert_eq!(error.field_path, vec![b"id".to_vec(), b"lib".to_vec()]);
    }
    #[test]
    fn unsupported_identifier_codes_do_not_turn_typed_strings_into_resources() {
        let mut bytes = images(&[
            (b"IMunknown", b"//not-a-resource.png", 1, 0),
            (b"IMknown", b"//known.png", 1, 0),
        ]);
        bytes[12..16].copy_from_slice(b"ZZ\0\0");
        let mut c = context(bytes);
        c.file_trace(0, None).unwrap();
        assert_eq!(c.report.usages.len(), 1);
        assert_eq!(c.report.usages[0].asset_path, b"//known.png");
        assert!(
            matches!(&c.report.warnings[0], TraceWarning::UnknownAssetCode { code, .. } if code == b"ZZ")
        );
    }
    #[test]
    fn schema_presence_never_implicitly_crosses_a_pointer() {
        let structures = [
            ("ID", 72, vec![("char", "name[64]"), ("void", "*lib")]),
            ("bNode", 8, vec![("ID", "*id")]),
        ];
        let c = context(saved(
            &structures,
            &[(b"DATA".as_slice(), 0x100, 1, vec![0; 8])],
        ));
        assert!(!c.has(
            View {
                file: 0,
                block: 0,
                element: 0
            },
            &["id", "lib"]
        ));
    }
    #[test]
    fn point_cache_memory_disk_external_and_raw_derived_name_vectors() {
        let structures = [
            ("ID", 72, vec![("char", "name[64]"), ("void", "*lib")]),
            ("Object", 72, vec![("ID", "id")]),
            (
                "PointCache",
                328,
                vec![
                    ("int", "flag"),
                    ("int", "index"),
                    ("char", "name[64]"),
                    ("char", "path[256]"),
                ],
            ),
        ];
        let mut owner = fixed(b"OB\xffcache", 64);
        owner.extend_from_slice(&0u64.to_le_bytes());
        for (flag, cache_name, cache_path, expected) in [
            (0i32, b"named".as_slice(), b"//external".as_slice(), None),
            (
                64,
                b"named".as_slice(),
                b"//external".as_slice(),
                Some(b"//blendcache_test/named_*.bphys".as_slice()),
            ),
            (
                64,
                b"".as_slice(),
                b"".as_slice(),
                Some(b"//blendcache_test/FF6361636865_*.bphys".as_slice()),
            ),
            (
                512,
                b"named".as_slice(),
                b"//external".as_slice(),
                Some(b"//external".as_slice()),
            ),
            (512, b"named".as_slice(), b"".as_slice(), None),
        ] {
            let mut cache = flag.to_le_bytes().to_vec();
            cache.extend_from_slice(&0i32.to_le_bytes());
            cache.extend(fixed(cache_name, 64));
            cache.extend(fixed(cache_path, 256));
            let records = [
                (b"OB".as_slice(), 0x100, 1, owner.clone()),
                (b"DATA".as_slice(), 0x200, 2, cache),
            ];
            let mut c = context(saved(&structures, &records));
            c.pointcache(
                View {
                    file: 0,
                    block: 0,
                    element: 0,
                },
                View {
                    file: 0,
                    block: 1,
                    element: 0,
                },
                b"OBcache.modifiers[0]".to_vec(),
                b".bphys",
            )
            .unwrap();
            assert_eq!(
                c.report.usages.first().map(|u| u.asset_path.as_slice()),
                expected
            );
            if let Some(usage) = c.report.usages.first() {
                assert!(usage.is_sequence);
                assert_eq!(
                    usage.path_full_field.as_ref().unwrap().path,
                    [if flag == 512 {
                        b"path".to_vec()
                    } else {
                        b"name".to_vec()
                    }]
                );
            }
        }
    }
    #[test]
    fn serialized_zero_bakes_never_dereferences_unsaved_handle() {
        let structures = [
            ("ID", 72, vec![("char", "name[64]"), ("void", "*lib")]),
            (
                "ListBase",
                16,
                vec![("ModifierData", "*first"), ("ModifierData", "*last")],
            ),
            ("Object", 88, vec![("ID", "id"), ("ListBase", "modifiers")]),
            (
                "ModifierData",
                88,
                vec![
                    ("ModifierData", "*next"),
                    ("ModifierData", "*prev"),
                    ("int", "type"),
                    ("int", "_pad"),
                    ("char", "name[64]"),
                ],
            ),
            (
                "NodesModifierData",
                100,
                vec![
                    ("ModifierData", "modifier"),
                    ("int", "bakes_num"),
                    ("void", "*bakes"),
                ],
            ),
        ];
        for count in [0i32, 1] {
            let mut object = fixed(b"OBbakes", 64);
            object.extend(0u64.to_le_bytes());
            object.extend(0x200u64.to_le_bytes());
            object.extend(0x200u64.to_le_bytes());
            let mut modifier = vec![0; 16];
            modifier.extend(57i32.to_le_bytes());
            modifier.extend(0i32.to_le_bytes());
            modifier.extend(fixed(b"GN", 64));
            modifier.extend(count.to_le_bytes());
            modifier.extend(0xbadf00du64.to_le_bytes());
            let mut c = context(saved(
                &structures,
                &[(b"OB", 0x100, 2, object), (b"DATA", 0x200, 4, modifier)],
            ));
            c.file_trace(0, None).unwrap();
            assert_eq!(c.report.complete, count == 0, "{:?}", c.report.errors);
            assert!(c.report.usages.is_empty());
        }
    }

    #[test]
    fn owner_scoped_reused_data_addresses_keep_both_modifier_sites() {
        let structures = [
            ("ID", 72, vec![("char", "name[64]"), ("void", "*lib")]),
            (
                "ListBase",
                16,
                vec![("ModifierData", "*first"), ("ModifierData", "*last")],
            ),
            ("Object", 88, vec![("ID", "id"), ("ListBase", "modifiers")]),
            (
                "ModifierData",
                88,
                vec![
                    ("ModifierData", "*next"),
                    ("ModifierData", "*prev"),
                    ("int", "type"),
                    ("int", "_pad"),
                    ("char", "name[64]"),
                ],
            ),
            (
                "MeshCacheModifierData",
                344,
                vec![("ModifierData", "modifier"), ("char", "filepath[256]")],
            ),
        ];
        let mut records = Vec::new();
        for (i, name, asset) in [
            (0, b"OBalpha".as_slice(), b"//a.pc2".as_slice()),
            (1, b"OBbeta".as_slice(), b"//b.pc2".as_slice()),
        ] {
            let mut object = fixed(name, 64);
            object.extend_from_slice(&0u64.to_le_bytes());
            object.extend_from_slice(&0x200u64.to_le_bytes());
            object.extend_from_slice(&0x200u64.to_le_bytes());
            records.push((b"OB".as_slice(), 0x100 + i, 2, object));
            let mut modifier = vec![0; 16];
            modifier.extend_from_slice(&46i32.to_le_bytes());
            modifier.extend_from_slice(&0i32.to_le_bytes());
            modifier.extend(fixed(b"cache", 64));
            modifier.extend(fixed(asset, 256));
            records.push((b"DATA".as_slice(), 0x200, 4, modifier));
        }
        let mut c = context(saved(&structures, &records));
        c.file_trace(0, None).unwrap();
        assert!(c.report.complete, "{:?}", c.report.errors);
        assert_eq!(
            c.report
                .usages
                .iter()
                .map(|u| u.asset_path.as_slice())
                .collect::<Vec<_>>(),
            [b"//a.pc2", b"//b.pc2"]
        );
        assert_ne!(
            c.report.usages[0]
                .path_full_field
                .as_ref()
                .unwrap()
                .owner_file,
            Vec::<u8>::new()
        );
        assert_ne!(
            c.report.usages[0].block_offset,
            c.report.usages[1].block_offset
        );
    }
    #[test]
    fn selected_object_dependency_closure_excludes_unselected_image() {
        let structures = [
            ("ID", 72, vec![("char", "name[64]"), ("void", "*lib")]),
            (
                "Object",
                90,
                vec![
                    ("ID", "id"),
                    ("ID", "*data"),
                    ("Object", "*parent"),
                    ("short", "type"),
                ],
            ),
            (
                "Image",
                338,
                vec![
                    ("ID", "id"),
                    ("char", "name[256]"),
                    ("short", "source"),
                    ("void", "*packedfile"),
                ],
            ),
        ];
        let mut object = fixed(b"OBselected", 64);
        object.extend_from_slice(&0u64.to_le_bytes());
        object.extend_from_slice(&0x200u64.to_le_bytes());
        object.extend_from_slice(&0x100u64.to_le_bytes());
        // Empty-image Object: the data union is Image, not an unqualified ID.
        object.extend_from_slice(&0i16.to_le_bytes());
        let image = |name: &[u8], asset: &[u8]| {
            let mut b = fixed(name, 64);
            b.extend_from_slice(&0u64.to_le_bytes());
            b.extend(fixed(asset, 256));
            b.extend_from_slice(&1i16.to_le_bytes());
            b.extend_from_slice(&0u64.to_le_bytes());
            b
        };
        let records = [
            (b"OB".as_slice(), 0x100, 1, object),
            (
                b"IM".as_slice(),
                0x200,
                2,
                image(b"IMneeded", b"//needed.png"),
            ),
            (
                b"IM".as_slice(),
                0x300,
                2,
                image(b"IMunselected", b"//excluded.png"),
            ),
        ];
        let mut c = context(saved(&structures, &records));
        c.file_trace(0, Some(vec![b"OBselected".to_vec()])).unwrap();
        assert!(c.report.complete, "{:?}", c.report.errors);
        assert_eq!(c.report.usages.len(), 1);
        assert_eq!(c.report.usages[0].asset_path, b"//needed.png");
    }
    #[test]
    fn selected_collection_self_reference_terminates_at_owner_scoped_identity() {
        let structures = [
            ("ID", 72, vec![("char", "name[64]"), ("void", "*lib")]),
            (
                "ListBase",
                16,
                vec![("CollectionChild", "*first"), ("CollectionChild", "*last")],
            ),
            (
                "Collection",
                88,
                vec![("ID", "id"), ("ListBase", "children")],
            ),
            (
                "CollectionChild",
                24,
                vec![
                    ("CollectionChild", "*next"),
                    ("CollectionChild", "*prev"),
                    ("Collection", "*collection"),
                ],
            ),
        ];
        let mut collection = fixed(b"GRcycle", 64);
        collection.extend_from_slice(&0u64.to_le_bytes());
        collection.extend_from_slice(&0x200u64.to_le_bytes());
        collection.extend_from_slice(&0x200u64.to_le_bytes());
        let mut child = vec![0; 16];
        child.extend_from_slice(&0x100u64.to_le_bytes());
        let records = [
            (b"GR".as_slice(), 0x100, 2, collection),
            (b"DATA".as_slice(), 0x200, 3, child),
        ];
        let mut c = context(saved(&structures, &records));
        c.file_trace(0, Some(vec![b"GRcycle".to_vec()])).unwrap();
        assert!(c.report.complete, "{:?}", c.report.errors);
        assert_eq!(c.visited.len(), 2);
        assert!(c.report.usages.is_empty());
    }
    #[test]
    fn missing_library_is_an_incomplete_dependency_not_success() {
        let structures = [
            ("ID", 72, vec![("char", "name[64]"), ("Library", "*lib")]),
            ("Library", 328, vec![("ID", "id"), ("char", "name[256]")]),
        ];
        let mut library = fixed(b"LIabsent", 64);
        library.extend_from_slice(&0u64.to_le_bytes());
        library.extend(fixed(
            b"//__oxblend_serialized_missing_library__.blend",
            256,
        ));
        let mut placeholder = fixed(b"OBselected", 64);
        placeholder.extend_from_slice(&0x100u64.to_le_bytes());
        let records = [
            (b"LI".as_slice(), 0x100, 1, library),
            (b"ID".as_slice(), 0x200, 0, placeholder),
        ];
        let mut c = context(saved(&structures, &records));
        c.file_trace(0, None).unwrap();
        assert!(!c.report.complete);
        assert_eq!(c.report.errors.len(), 1);
        assert!(matches!(
            c.report.errors[0].kind,
            FailureKind::MissingLibrary
        ));
        assert_eq!(c.report.usages.len(), 1);
    }
    #[test]
    fn import_node_admission_excludes_arbitrary_string_nodes() {
        for name in [
            b"GeometryNodeImportOBJ".as_slice(),
            b"GeometryNodeImportPLY",
            b"GeometryNodeImportSTL",
            b"GeometryNodeImportText",
            b"GeometryNodeImportCSV",
            b"GeometryNodeImportVDB",
        ] {
            assert!(geometry_import(name));
        }
        assert!(!geometry_import(b"FunctionNodeInputString"));
        assert!(!geometry_import(b"GeometryNodeImportUnknown"));
    }
}
impl FieldReference {
    pub fn json(&self) -> Json {
        json!({"name_hex":hex(&self.name),"full_name_hex":hex(&self.full_name),"offset":self.offset,"size":self.size,"type_hex":hex(&self.dna_type),"pointer":self.pointer})
    }
    pub fn provenance_json(&self) -> Json {
        json!({"owner_file_hex":hex(&self.owner_file),"block_address":self.block_address.to_string(),"element":self.element,"field_path_hex":self.path.iter().map(|p|hex(p)).collect::<Vec<_>>()})
    }
}
impl Usage {
    pub fn json(&self) -> Json {
        json!({"owner_file":std::str::from_utf8(&self.owner_file).ok(),"owner_file_hex":hex(&self.owner_file),"library_chain_hex":self.library_chain.iter().map(|p|hex(p)).collect::<Vec<_>>(),"block_code_hex":hex(&self.block_code),"block_address":self.block_address.to_string(),"block_offset":self.block_offset,"block_type":std::str::from_utf8(&self.block_type).ok(),"block_name_hex":hex(&self.block_name),"asset_path_hex":hex(&self.asset_path),"is_sequence":self.is_sequence,"is_udim":self.is_udim,"optional":self.optional,"path_full_field":self.path_full_field.as_ref().map(FieldReference::json),"path_dir_field":self.path_dir_field.as_ref().map(FieldReference::json),"path_base_field":self.path_base_field.as_ref().map(FieldReference::json),"field_provenance":{"full":self.path_full_field.as_ref().map(FieldReference::provenance_json),"dir":self.path_dir_field.as_ref().map(FieldReference::provenance_json),"base":self.path_base_field.as_ref().map(FieldReference::provenance_json)}})
    }
}
impl TraceFailure {
    pub(crate) fn is_external_missing(&self) -> bool {
        matches!(self.kind, FailureKind::MissingLibrary)
            || (matches!(self.kind, FailureKind::MissingId) && self.missing_id.is_some())
    }
    pub fn json(&self) -> Json {
        let mut value = json!({"kind":match &self.kind{FailureKind::Reader(k)=>*k,FailureKind::LibraryReader(_)=>"library_read",FailureKind::DanglingPointer=>"dangling_pointer",FailureKind::AmbiguousPointer=>"ambiguous_pointer",FailureKind::MissingLibrary=>"missing_library",FailureKind::MissingId=>"missing_id",FailureKind::Cycle=>"cycle",FailureKind::InvalidGraph=>"invalid_graph"},"owner_file_hex":hex(&self.owner_file),"block_address":self.block_address.map(|n|n.to_string()),"field_path_hex":self.field_path.iter().map(|p|hex(p)).collect::<Vec<_>>(),"library_path_hex":self.library_path.as_ref().map(|p|hex(p)),"message":self.message});
        if let Some(id) = &self.missing_id {
            value["root_file_hex"] = json!(hex(&id.root_file));
            value["root_file"] = json!(std::str::from_utf8(&id.root_file).ok());
            value["owner_file"] = json!(std::str::from_utf8(&self.owner_file).ok());
            value["library_path"] = json!(
                self.library_path
                    .as_ref()
                    .and_then(|p| std::str::from_utf8(p).ok())
            );
            value["id_code_hex"] = json!(hex(&id.id_code));
            value["id_code"] = json!(std::str::from_utf8(&id.id_code).ok());
            value["id_name_hex"] = json!(hex(&id.id_name));
            value["id_name"] = json!(std::str::from_utf8(&id.id_name).ok());
            value["referencing_owner_chain_hex"] = json!(
                id.referencing_owner_chain
                    .iter()
                    .map(|p| hex(p))
                    .collect::<Vec<_>>()
            );
            value["referencing_owner_chain"] = json!(
                id.referencing_owner_chain
                    .iter()
                    .map(|p| std::str::from_utf8(p).ok())
                    .collect::<Option<Vec<_>>>()
            );
        }
        value
    }
}
impl TraceReport {
    pub fn json(&self) -> Json {
        let mut value = json!({"schema":1,"complete":self.complete,"usages":self.usages.iter().map(Usage::json).collect::<Vec<_>>(),"errors":self.errors.iter().map(TraceFailure::json).collect::<Vec<_>>(),"warnings":self.warnings.iter().map(|w|match w{TraceWarning::MissingLibrary{owner_file,library_path}=>json!({"kind":"missing_library","owner_file_hex":hex(owner_file),"library_path_hex":hex(library_path)}),TraceWarning::SilencedPointer{failure}=>json!({"kind":"silenced_pointer","failure":failure.json()}),TraceWarning::UnknownAssetCode{owner_file,code}=>json!({"kind":"unknown_asset_code","owner_file_hex":hex(owner_file),"code_hex":hex(code)})}).collect::<Vec<_>>()});
        value["missing_link_counts"] = json!({"missing_id":self.errors.iter().filter(|e|matches!(e.kind, FailureKind::MissingId)).count(),"missing_library":self.errors.iter().filter(|e|matches!(e.kind, FailureKind::MissingLibrary)).count()});
        value
    }
}
