use crate::session::{BodyErr, BodyResult};
use crate::value::{PathStep, Value, hex};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

type NameMap = HashMap<Vec<u8>, HashMap<Vec<u8>, Vec<usize>>>;
#[derive(Debug, Default)]
pub(crate) struct NameIndex {
    local: NameMap,
    forwarded: NameMap,
}
impl NameIndex {
    fn new() -> Self {
        Self::default()
    }
}
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
    // A saved ID BHead is a forwarding stub, not an ID-family name. Index
    // its real prefix; selection still qualifies its native ID header and
    // checked Library pointer before following the existing recursive walk.
    let (index, code) = if code == b"ID" && name.len() >= 2 {
        (&mut index.forwarded, name[..2].to_vec())
    } else {
        (&mut index.local, code)
    };
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
    fn name_selection_reads_only_id_records_not_unrelated_data_payloads() {
        let reader = BlendFile::from_bytes(
            include_bytes!("../tests/fixtures/dependencies/spline-ik.blend").to_vec(),
        )
        .unwrap();
        assert!(reader.blocks().iter().any(|b| b.code == *b"DATA"));
        crate::SCAN_RECORD_READS.with(|n| n.set(Some(0)));
        let names = named_blocks(&reader, &[]).unwrap();
        let reads = crate::SCAN_RECORD_READS.with(|n| n.replace(None).unwrap());
        assert!(names.is_empty());
        let ids = reader
            .blocks()
            .iter()
            .filter(|b| code(&b.code).len() == 2)
            .count();
        assert_eq!(
            reads, ids,
            "name selection must not read record zero of unrelated geometry/DATA blocks"
        );
        assert!(reader.name_index.get().is_some());
        crate::SCAN_RECORD_READS.with(|n| n.set(Some(0)));
        named_blocks(&reader, &[]).unwrap();
        assert_eq!(
            crate::SCAN_RECORD_READS.with(|n| n.replace(None).unwrap()),
            0
        );
    }
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
            (b"ID", Some(b"OBtree")),
            (b"ID", Some(b"OBlinked")),
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
                            let local = records.iter().any(|(kind, id)| {
                                *kind != b"ID"
                                    && name.starts_with(kind)
                                    && id.is_some_and(|id| id == name.as_slice())
                            });
                            (if local {
                                *code != b"ID" && name.starts_with(code)
                            } else {
                                *code == b"ID" && name.len() >= 2
                            }) && stored.is_some_and(|id| id == name.as_slice())
                        })
                        .then_some(i)
                })
                .collect();
            assert_eq!(selected_blocks(&index, &names), expected);
        }
    }
}
/// Shared #30 local-first, forwarding-stub-fallback name selection.
/// Query's narrow linked-subtype reader uses the same resolver as tracing.
pub(crate) fn named_blocks(reader: &BlendFile, names: &[Vec<u8>]) -> Result<HashSet<usize>> {
    if reader.name_index.get().is_none() {
        let mut index = NameIndex::new();
        reader.scan_records_where(
            |block| code(&reader.blocks()[block].code).len() == 2,
            |block, record| {
                let code = code(&reader.blocks()[block].code);
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
            },
        )?;
        let _ = reader.name_index.set(index);
    }
    Ok(selected_blocks(reader.name_index.get().unwrap(), names))
}
fn selected_blocks(index: &NameIndex, names: &[Vec<u8>]) -> HashSet<usize> {
    let mut selected = HashSet::new();
    for name in names {
        // Blender name lookup prefers actual local IDs. A same-named stub
        // must not widen that selection. Direct pointer-reached stubs still
        // take the normal checked forwarding path below.
        let local = index
            .local
            .iter()
            .filter(|(code, _)| name.starts_with(code))
            .filter_map(|(_, entries)| entries.get(name))
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        if !local.is_empty() {
            selected.extend(local);
            continue;
        }
        for (code, entries) in &index.forwarded {
            if name.starts_with(code)
                && let Some(blocks) = entries.get(name)
            {
                selected.extend(blocks);
            }
        }
    }
    selected
}
fn tile_member(asset: &[u8], number: i64) -> Option<Vec<u8>> {
    let name_start = asset
        .iter()
        .rposition(|b| matches!(b, b'/' | b'\\'))
        .map_or(0, |n| n + 1);
    for (token, value) in [
        (b"<UDIM>".as_slice(), number.to_string()),
        (
            b"<UVTILE>".as_slice(),
            format!(
                "u{}_v{}",
                (number - 1001) % 10 + 1,
                (number - 1001) / 10 + 1
            ),
        ),
    ] {
        if let Some(at) = asset[name_start..]
            .windows(token.len())
            .position(|w| w == token)
        {
            let at = at + name_start;
            let mut path = asset[..at].to_vec();
            path.extend(value.as_bytes());
            path.extend(&asset[at + token.len()..]);
            return Some(path);
        }
    }
    None
}
/// Blender `BKE_image_ensure_tile_token_filename_only` (image.cc:3773):
/// auto-tokenize a concrete basename with Blender's own filename-only
/// regexes, UDIM first then UVTILE, returning the tokenized basename.
/// Neither regex matching means Blender keeps a non-UDIM path too.
/// Byte-exact port of the `std::regex_search` order: the UDIM group takes the
/// rightmost viable separator, the UVTILE group the rightmost viable `u`
/// start with greedy digit lengths, exactly like the C++ backtracking order.
fn auto_tile_token(name: &[u8]) -> Option<Vec<u8>> {
    // `(.*[._-])([12]\d{3})([._-].*)`: greedy group 1 ends at the rightmost
    // separator directly followed by `[12]\d{3}` plus a trailing separator.
    for (separator, byte) in name.iter().enumerate().rev() {
        if !matches!(byte, b'.' | b'_' | b'-') {
            continue;
        }
        if separator + 6 > name.len() {
            continue;
        }
        let digits = &name[separator + 1..separator + 5];
        if !matches!(digits[0], b'1' | b'2') || !digits[1..].iter().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if !matches!(name[separator + 5], b'.' | b'_' | b'-') {
            continue;
        }
        let mut tokenized = Vec::with_capacity(name.len() + 2);
        tokenized.extend_from_slice(&name[..separator + 1]);
        tokenized.extend_from_slice(b"<UDIM>");
        tokenized.extend_from_slice(&name[separator + 5..]);
        return Some(tokenized);
    }
    // `(.*)(u\d{1,2}_v\d{1,3})(\D.*)`: greedy group 1 ends at the rightmost
    // viable `u` start; digit lengths shrink exactly like regex backtracking.
    let mut start = name.len();
    while start > 0 {
        start -= 1;
        if name[start] != b'u' {
            continue;
        }
        for u_len in [2usize, 1] {
            let after_u = start + 1 + u_len;
            if after_u + 2 > name.len()
                || !name[start + 1..after_u].iter().all(|b| b.is_ascii_digit())
                || name[after_u] != b'_'
                || name[after_u + 1] != b'v'
            {
                continue;
            }
            for v_len in [3usize, 2, 1] {
                let group_end = after_u + 2 + v_len;
                if group_end >= name.len()
                    || !name[after_u + 2..group_end]
                        .iter()
                        .all(|b| b.is_ascii_digit())
                    || name[group_end].is_ascii_digit()
                {
                    continue;
                }
                let mut tokenized = Vec::with_capacity(name.len() + 4);
                tokenized.extend_from_slice(&name[..start]);
                tokenized.extend_from_slice(b"<UVTILE>");
                tokenized.extend_from_slice(&name[group_end..]);
                return Some(tokenized);
            }
        }
    }
    None
}
fn sequence_member(asset: &[u8], frame: i64) -> Option<Vec<u8>> {
    let start = asset
        .iter()
        .rposition(|b| matches!(b, b'/' | b'\\'))
        .map_or(0, |n| n + 1);
    let dot = asset[start..]
        .iter()
        .rposition(|b| *b == b'.')
        .map_or(asset.len(), |n| start + n);
    let width = asset[start..dot]
        .iter()
        .rev()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if width == 0 {
        return None;
    }
    let mut path = asset[..dot - width].to_vec();
    path.extend(format!("{frame:0width$}").as_bytes());
    path.extend(&asset[dot..]);
    Some(path)
}
fn filename_tile(data: &[u8]) -> bool {
    let name = data
        .rsplit(|b| matches!(b, b'/' | b'\\'))
        .next()
        .unwrap_or(data);
    [b"<UDIM>".as_slice(), b"<UVTILE>".as_slice()]
        .iter()
        .any(|token| name.windows(token.len()).any(|w| w == *token))
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
    /// Concrete saved pattern requirements (not a directory glob).
    pub expected_members: Vec<Vec<u8>>,
    /// Why the saved image's complete member set could not be proved.
    pub pattern_unknown: Option<String>,
    pattern_image: Option<View>,
    pub optional: Option<bool>,
    /// Saved Camera type for external custom shader declarations only.
    pub camera_type: Option<i64>,
    /// Active CUSTOM camera (type 3), distinct from a required saved declaration.
    pub active: Option<bool>,
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
    /// Source site for an unknown dependency subtree, never a guessed target.
    pub site: Option<Json>,
    pub message: String,
    pub(crate) diagnostic: Option<std::sync::Arc<crate::failure_receipt::DiagnosticRef>>,
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
    /// A qualified missing saved ID slot: native NULL or legacy reconstruction.
    LibraryNullPointer {
        failure: TraceFailure,
        pointer_address: u64,
        offset: usize,
        native_action: String,
        library_chain: Vec<Vec<u8>>,
    },
    UnknownAssetCode {
        owner_file: Vec<u8>,
        code: Vec<u8>,
    },
    /// Saved in-memory Text filepath is history, not a dependency declaration.
    EmbeddedTextFilepath {
        owner_file: Vec<u8>,
        block_address: u64,
        block_name: Vec<u8>,
        asset_path: Vec<u8>,
        flags: i64,
    },
    /// INTERNAL camera's filepath is history; its shader is the checked Text ID.
    EmbeddedCameraFilepath {
        owner_file: Vec<u8>,
        block_address: u64,
        block_name: Vec<u8>,
        asset_path: Vec<u8>,
    },
    /// C dependency-walker interpretation; native loader I/O is not proven.
    PathResolutionUnverified {
        owner_file: Vec<u8>,
        asset_path: Vec<u8>,
        resolved_path: Vec<u8>,
    },
}
impl TraceWarning {
    pub(crate) fn json(&self) -> Json {
        crate::value::with_text(match self {
            Self::MissingLibrary {
                owner_file,
                library_path,
            } => {
                json!({"kind":"missing_library","owner_file_hex":hex(owner_file),"library_path_hex":hex(library_path)})
            }
            Self::SilencedPointer { failure } => {
                json!({"kind":"silenced_pointer","failure":failure.json()})
            }
            Self::LibraryNullPointer {
                failure,
                pointer_address,
                offset,
                native_action,
                library_chain,
            } => {
                // This site has a proved native compatibility action, not an
                // unknown subtree. Preserve the established warning payload.
                let mut known = failure.json();
                if let Some(fields) = known.as_object_mut() {
                    fields.remove("status");
                    fields.remove("site");
                }
                json!({"kind":"library_null_pointer","failure":known,"pointer_address":pointer_address.to_string(),"offset":offset,"library_chain_hex":library_chain.iter().map(|p|hex(p)).collect::<Vec<_>>(),"native_action":native_action,"pack_action":"clear_to_null"})
            }
            Self::EmbeddedTextFilepath {
                owner_file,
                block_address,
                block_name,
                asset_path,
                flags,
            } => {
                json!({"kind":"embedded_text_filepath","status":"informational","owner_file_hex":hex(owner_file),"block_address":block_address.to_string(),"block_name_hex":hex(block_name),"asset_path_hex":hex(asset_path),"flags":flags,"reason":"saved TXT_ISMEM: content is embedded; filepath is historical, not a dependency"})
            }
            Self::EmbeddedCameraFilepath {
                owner_file,
                block_address,
                block_name,
                asset_path,
            } => {
                json!({"kind":"embedded_camera_filepath","status":"informational","owner_file_hex":hex(owner_file),"block_address":block_address.to_string(),"block_name_hex":hex(block_name),"asset_path_hex":hex(asset_path),"custom_mode":0,"reason":"saved INTERNAL custom camera: custom_filepath is historical; custom_shader Text owns shader content"})
            }
            Self::PathResolutionUnverified {
                owner_file,
                asset_path,
                resolved_path,
            } => {
                json!({"kind":"path_resolution_unverified","owner_file_hex":hex(owner_file),"asset_path_hex":hex(asset_path),"resolved_path_hex":hex(resolved_path),"reason":"foreign root follows native dependency semantics; portable relocation and loader-family I/O are not fully verified"})
            }
            Self::UnknownAssetCode { owner_file, code } => {
                json!({"kind":"unknown_asset_code","owner_file_hex":hex(owner_file),"code_hex":hex(code)})
            }
        })
    }
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
struct LibraryRequest {
    library: View,
    names: BTreeSet<Vec<u8>>,
    stubs: Vec<View>,
}
enum FileSelection {
    All,
    Names(Vec<Vec<u8>>),
    Endpoints(BTreeSet<View>),
}
struct ResolvedSelection {
    file: usize,
    endpoints: BTreeSet<View>,
    chain: Vec<Vec<u8>>,
}
struct File {
    path: PathBuf,
    chain: Vec<Vec<u8>>,
    // Store-local identity into the owned snapshot arena. File entries and
    // arena entries are admitted together and never removed; the borrow is
    // re-acquired by ID for each observation, never held across insertion.
    source: crate::session::SourceId,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibraryPolicy {
    ReportMissing,
    ExistingOnly,
}
struct Context<'a> {
    store: &'a mut crate::session::InputStore,
    batch: &'a mut crate::session::CaptureBatch,
    op: &'a mut crate::session::OpCtx,
    options: TraceOptions,
    library_policy: LibraryPolicy,
    preserve_capture_context: bool,
    files: Vec<File>,
    by_path: HashMap<PathBuf, usize>,
    report: TraceReport,
    visited: HashSet<View>,
    // Source-qualified forwarding identities active on the recursion stack.
    // Completed diamond routes may reuse visited blocks; active stubs may not.
    active_stubs: Vec<View>,
    declared_data: HashSet<View>,
    dedup: HashSet<Occurrence>,
    image_users: HashMap<usize, HashMap<u64, Vec<View>>>,
    linked_images: HashMap<(Vec<u8>, Vec<u8>), Vec<View>>,
    unknown: HashSet<Vec<u8>>,
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
/// Resolve serialized paths using Blender's native dependency semantics.
/// Native Image/Library/Sequencer I/O normalizes embedded backslashes.
/// Foreign roots use the C walker's mapping, flagged where portability is unproven.
pub(crate) fn resolve(owner: &Path, data: &[u8]) -> PathBuf {
    // Actual path-resolving owner: query.resolve phase at L1.
    // Resolution itself is infallible lexical mapping; uncertainty is a
    // separate PathResolutionUnverified warning at the emitting usage site.
    if !crate::diagnostics::enabled() {
        return resolve_blender(owner, data);
    }
    let mut span = crate::diagnostics::Span::new_lazy(
        "query.resolve",
        || serde_json::json!({"owner":owner.to_string_lossy()}),
    );
    let resolved = resolve_blender(owner, data);
    if span.active() {
        // Asset bytes are hex-encoded only when tracing; never logged otherwise.
        span.data(serde_json::json!({"resolved":resolved.to_string_lossy(),"uncertain":uncertain_path(data)}));
    }
    span.end("ok");
    // Emit an explicit decision for foreign-root paths whose loader I/O
    // cannot be safely relocated; the walk keeps the unknown site.
    if uncertain_path(data) {
        crate::diagnostics::point(
            "decision",
            serde_json::json!({"item":resolved.to_string_lossy(),"kind":"path_resolution_unverified","reason":"foreign Windows-root loader I/O cannot be safely relocated; retained as unknown"}),
        );
    }
    resolved
}
fn windows_drive(data: &[u8]) -> bool {
    data.len() >= 3
        && data[0].is_ascii_alphabetic()
        && data[1] == b':'
        && matches!(data[2], b'/' | b'\\')
}
fn windows_root(data: &[u8]) -> bool {
    windows_drive(data) || data.starts_with(b"\\\\")
}
pub(crate) fn uncertain_path(data: &[u8]) -> bool {
    windows_root(data)
}
pub(crate) fn resolve_usage_at(usage: &Usage, owner: &Path) -> PathBuf {
    resolve(owner, &usage.asset_path)
}
pub(crate) fn resolve_usage(usage: &Usage) -> PathBuf {
    resolve_usage_at(usage, &path(&usage.owner_file))
}
fn resolve_blender(owner: &Path, data: &[u8]) -> PathBuf {
    let unc = data.starts_with(b"\\\\");
    let data = normalize(data);
    if windows_drive(&data) {
        let mut mapped = vec![b'/', data[0].to_ascii_lowercase()];
        mapped.extend_from_slice(&data[2..]);
        return lexical(&path(&mapped));
    }
    if unc {
        let tail = lexical(&path(&data[2..]));
        let mut result = vec![b'\\', b'\\'];
        result.extend(raw(&tail));
        return path(&result);
    }
    let p = if absolute_bytes(&data) {
        path(&data)
    } else {
        owner
            .parent()
            .unwrap_or(Path::new("/"))
            .join(path(if data.starts_with(b"//") {
                &data[2 + data[2..].iter().take_while(|&&b| b == b'/').count()..]
            } else {
                &data
            }))
    };
    lexical(&p)
}
fn code(data: &[u8; 4]) -> Vec<u8> {
    data.iter().copied().take_while(|b| *b != 0).collect()
}
pub(crate) fn failure_site(
    reader: &BlendFile,
    block: usize,
    element: usize,
    fields: &[Vec<u8>],
) -> Json {
    let steps: Vec<_> = fields.iter().cloned().map(PathStep::Field).collect();
    let location = reader.locate_field(block, element, &steps).ok();
    let pointer = reader
        .get(block, element, &steps, false)
        .ok()
        .and_then(|v| {
            if let Value::Pointer(p) = v {
                Some(p.to_string())
            } else {
                None
            }
        });
    crate::value::with_text(json!({
        "block_index":block,"element":element,
        "block_code_hex":reader.blocks().get(block).map(|b|hex(&code(&b.code))),
        "block_code":reader.blocks().get(block).and_then(|b|String::from_utf8(code(&b.code)).ok()),
        "struct_hex":reader.dna_type_id(block).ok().map(hex),
        "struct":reader.dna_type_id(block).ok().and_then(|s|std::str::from_utf8(s).ok()),
        "field_path_hex":fields.iter().map(|p|hex(p)).collect::<Vec<_>>(),
        "pointer_value_decimal":pointer,
        "expected_target_type":location.as_ref().filter(|l|l.field.pointer).and_then(|l|std::str::from_utf8(&reader.schema().types[l.field.type_index]).ok()),
        "expected_target_type_hex":location.filter(|l|l.field.pointer).map(|l|hex(&reader.schema().types[l.field.type_index])),
        "scope":"subtree"
    }))
}
#[cfg(test)]
thread_local! {
    static PROBE_FAULTS: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
impl Context<'_> {
    /// Admit or reuse one source in the shared arena, then return this
    /// root-local file slot. Reader borrows end before this call; the next
    /// observation re-acquires its borrow by ID.
    fn open(&mut self, path: &Path, captured: bool, chain: Vec<Vec<u8>>) -> Result<usize> {
        if let Some(&fi) = self.by_path.get(path) {
            let source = self.files[fi].source;
            self.store.check_reuse(source, self.op)?;
            return Ok(fi);
        }
        let low_memory = self.options.low_memory;
        let first_session_capture = captured
            && self.op.policy == crate::session::GuardPolicy::Session
            && self.store.id_for_path(path).is_err();
        let source = self
            .store
            .ensure(self.batch, path, captured, low_memory, self.op)
            .map_err(|error| {
                // A fresh library's failure is a trace report fact, just as
                // with path-based inspection. Capture phase presentation must
                // not change that fact. Keep the Error's diagnostic identity.
                if first_session_capture && !self.preserve_capture_context {
                    error.without_context()
                } else {
                    error
                }
            })?;
        let fi = self.files.len();
        self.files.push(File {
            path: path.to_path_buf(),
            chain,
            source,
        });
        self.by_path.insert(path.to_path_buf(), fi);
        Ok(fi)
    }
    fn file(&self, v: View) -> &File {
        &self.files[v.file]
    }
    /// Short reader borrow for one observation. File slots and arena
    /// entries are admitted together and never removed, so the lookup is
    /// valid by construction; the error only guards against misuse.
    fn reader(&self, v: View) -> Result<&BlendFile> {
        self.store.reader(self.files[v.file].source)
    }
    fn reader_of(&self, fi: usize) -> Result<&BlendFile> {
        self.store.reader(self.files[fi].source)
    }
    fn fault(&self, v: View, p: &[&str], e: Error) -> Box<TraceFailure> {
        #[cfg(test)]
        PROBE_FAULTS.with(|n| {
            if let Some(value) = n.get() {
                n.set(Some(value + 1));
            }
        });
        let kind = match e.kind {
            "dangling_pointer" => FailureKind::DanglingPointer,
            "ambiguous_pointer" => FailureKind::AmbiguousPointer,
            _ => FailureKind::Reader(e.kind),
        };
        let reader = self.reader(v).ok();
        Box::new(TraceFailure {
            kind,
            owner_file: raw(&self.file(v).path),
            block_address: reader.map(|r| r.blocks()[v.block].old_address),
            field_path: p.iter().map(|s| s.as_bytes().to_vec()).collect(),
            library_path: None,
            missing_id: None,
            site: reader.map(|r| {
                failure_site(
                    r,
                    v.block,
                    v.element,
                    &p.iter().map(|s| s.as_bytes().to_vec()).collect::<Vec<_>>(),
                )
            }),
            diagnostic: e.diagnostic.get().cloned(),
            message: e.message,
        })
    }
    fn invalid(&self, v: View, p: &[&str], message: &str) -> Box<TraceFailure> {
        let mut e = self.fault(v, p, Error::new("invalid_graph", message));
        e.kind = FailureKind::InvalidGraph;
        e
    }
    fn get(&self, v: View, p: &[&str]) -> T<Value> {
        let reader = self.reader(v).map_err(|e| self.fault(v, p, e))?;
        reader
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
    // Presence is a schema question; no owned diagnostic is constructed.
    fn schema_field(&self, v: View, p: &[&str]) -> Result<&crate::schema::Field> {
        let reader = self.reader(v)?;
        let schema = reader.schema();
        let mut si = reader.struct_index(v.block)?;
        for (i, n) in p.iter().enumerate() {
            let st = &schema.structs[si];
            let field = st
                .by_name
                .get(n.as_bytes())
                .and_then(|i| st.fields.get(*i))
                .ok_or_else(|| Error::new("missing_field", *n))?;
            if i + 1 == p.len() {
                return Ok(field);
            }
            if field.pointer {
                return Err(Error::new(
                    "invalid_graph",
                    "pointer is not an embedded structure",
                ));
            }
            si = *schema
                .by_type
                .get(&field.type_index)
                .ok_or_else(|| Error::new("invalid_graph", "not embedded structure"))?;
        }
        Err(Error::new("invalid_graph", "empty field path"))
    }
    fn field(&self, v: View, p: &[&str]) -> T<FieldReference> {
        let field = self.schema_field(v, p).map_err(|e| {
            if e.kind == "invalid_graph" {
                self.invalid(v, p, &e.message)
            } else {
                self.fault(v, p, e)
            }
        })?;
        let f = self.file(v);
        Ok(FieldReference {
            owner_file: raw(&f.path),
            block_address: self.reader(v).map_err(|e| self.fault(v, p, e))?.blocks()[v.block]
                .old_address,
            element: v.element,
            path: p.iter().map(|s| s.as_bytes().to_vec()).collect(),
            name: field.name.clone(),
            full_name: field.full_name.clone(),
            offset: field.offset,
            size: field.size,
            dna_type: self
                .reader(v)
                .map_err(|e| self.fault(v, p, e))?
                .schema()
                .types[field.type_index]
                .clone(),
            pointer: field.pointer,
        })
    }
    fn has(&self, v: View, p: &[&str]) -> bool {
        self.schema_field(v, p).is_ok()
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
        let reader = self.reader(v).map_err(|e| self.fault(v, p, e))?;
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
            Ok(b) => {
                if let Some(block) = b
                    && reader.dna_type_id(v.block).is_ok_and(|ty| {
                        matches!(ty, b"NodesModifierData" | b"bTransLikeConstraint")
                    })
                {
                    let offset = reader
                        .locate_field(v.block, v.element, &steps)
                        .map_err(|e| self.fault(v, p, e))?
                        .offset;
                    crate::library_compat::validate_present(reader, v.block, offset, block)
                        .map_err(|e| self.fault(v, p, e))?;
                }
                Ok(b.map(|block| View {
                    file: v.file,
                    block,
                    element: 0,
                }))
            }
            Err(e) => {
                let mut failure = self.fault(v, p, e);
                if let Some(site) = failure.site.as_mut() {
                    site["pointer_value_decimal"] = json!(address.to_string());
                    if let Some(expected) = caller_type {
                        site["expected_target_type_hex"] = json!(hex(expected));
                        site["expected_target_type"] = json!(std::str::from_utf8(expected).ok());
                    }
                }
                if matches!(failure.kind, FailureKind::DanglingPointer) && v.file != 0 {
                    let offset = self
                        .reader(v)
                        .map_err(|e| self.fault(v, p, e))?
                        .locate_field(v.block, v.element, &steps)
                        .map_err(|e| self.fault(v, p, e))?
                        .offset;
                    if self.library_null(v, offset, &failure)? {
                        return Ok(None);
                    }
                }
                // Reports always keep the site unknown and continue only
                // independent provable edges; a missing target is never NULL.
                {
                    if crate::diagnostics::enabled() {
                        let item = failure
                            .missing_id
                            .as_ref()
                            .map(|m| crate::value::hex(&m.id_name))
                            .unwrap_or_else(|| {
                                format!(
                                    "{}:{}",
                                    crate::value::hex(&failure.owner_file),
                                    failure
                                        .block_address
                                        .map(|a| a.to_string())
                                        .unwrap_or_default()
                                )
                            });
                        crate::diagnostics::point(
                            "error",
                            serde_json::json!({"kind":failure.kind_str(),"message":failure.message,"item":item}),
                        );
                        crate::diagnostics::point(
                            "decision",
                            serde_json::json!({"item":item,"kind":"silenced_pointer","reason":"unknown subtree retained; independent provable edges continue"}),
                        );
                    }
                    self.report.complete = false;
                    self.report.errors.push((*failure).clone());
                    self.report
                        .warnings
                        .push(TraceWarning::SilencedPointer { failure: *failure });
                    Ok(None)
                }
            }
        }
    }
    fn library_null(&mut self, v: View, offset: usize, failure: &TraceFailure) -> T<bool> {
        let clear = crate::library_compat::missing_id(
            self.reader(v).map_err(|e| self.fault(v, &[], e))?,
            v.block,
            offset,
        )
        .map_err(|e| self.fault(v, &[], e))?;
        let Some(clear) = clear else { return Ok(false) };
        if self.report.warnings.iter().any(|warning| {
            matches!(warning,
            TraceWarning::LibraryNullPointer {failure: prior, offset, ..}
                if prior.owner_file == failure.owner_file && *offset == clear.offset)
        }) {
            return Ok(true);
        }
        self.report.warnings.push(TraceWarning::LibraryNullPointer {
            failure: failure.clone(),
            pointer_address: clear.address,
            offset: clear.offset,
            native_action: clear.native_action().into(),
            library_chain: self.file(v).chain.clone(),
        });
        if crate::diagnostics::enabled() {
            crate::diagnostics::point(
                "decision",
                serde_json::json!({"item":crate::value::hex(&failure.owner_file),"kind":"library_null_pointer","reason":clear.native_action()}),
            );
        }
        Ok(true)
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
        // The arena observation ends before the graph mutation below.
        let (code, address, offset) = {
            let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
            let b = &reader.blocks()[v.block];
            (code(&b.code), b.old_address, b.data_offset)
        };
        if self.options.mode == TraceMode::Recursive
            && !self.dedup.insert(Occurrence {
                view: v,
                name: name.clone(),
                sites: [&full, &dir, &base].map(|f| f.as_ref().map(|f| f.path.clone())),
            })
        {
            return Ok(());
        }
        // Keep saved bytes: normalizing here would erase UNC versus Blender //.
        let udim = seq && filename_tile(&asset);
        let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
        let si = reader
            .struct_index(v.block)
            .map_err(|e| self.fault(v, &[], e))?;
        let schema = reader.schema();
        let record_size = schema.lengths[schema.structs[si].type_index];
        let usage = Usage {
            owner_file: raw(&self.file(v).path),
            library_chain: self.file(v).chain.clone(),
            block_code: code,
            block_address: address,
            block_offset: offset + v.element * record_size,
            block_type: reader
                .dna_type_id(v.block)
                .map_err(|e| self.fault(v, &[], e))?
                .to_vec(),
            block_name: name,
            asset_path: asset,
            is_sequence: seq,
            is_udim: udim,
            expected_members: Vec::new(),
            pattern_unknown: None,
            pattern_image: None,
            optional: None,
            camera_type: None,
            active: None,
            path_full_field: full,
            path_dir_field: dir,
            path_base_field: base,
        };
        if uncertain_path(&usage.asset_path) {
            self.report
                .warnings
                .push(TraceWarning::PathResolutionUnverified {
                    owner_file: usage.owner_file.clone(),
                    asset_path: usage.asset_path.clone(),
                    resolved_path: raw(&resolve_usage(&usage)),
                });
            if crate::diagnostics::enabled() {
                crate::diagnostics::point(
                    "decision",
                    serde_json::json!({"item":crate::value::hex(&usage.asset_path),"kind":"path_resolution_unverified","reason":"foreign-root loader I/O cannot be safely relocated; retained as unknown"}),
                );
            }
        }
        self.report.usages.push(usage);
        Ok(())
    }
    fn full(&mut self, v: View, p: &[&str], seq: bool, name: Option<Vec<u8>>) -> T<()> {
        let asset = self.bytes(v, p)?;
        let field = self.field(v, p)?;
        self.emit(v, asset, seq, name, (Some(field), None, None))
    }
    fn checked_packed_pointer(&self, v: View, p: &[&str]) -> T<Option<View>> {
        let steps = p
            .iter()
            .map(|n| PathStep::Field(n.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        self.reader(v)
            .map_err(|e| self.fault(v, p, e))?
            .get_pointer(v.block, v.element, &steps)
            .map(|target| {
                target.map(|block| View {
                    block,
                    file: v.file,
                    element: 0,
                })
            })
            .map_err(|e| self.fault(v, p, e))
    }
    fn packed_bytes(&self, v: View) -> T<()> {
        let size = self.num(v, &["size"])?;
        if size < 0 {
            return Err(self.invalid(v, &["size"], "negative packed byte count"));
        }
        let target = self.checked_packed_pointer(v, &["data"])?;
        if size != 0 && target.is_none() {
            return Err(self.invalid(v, &["data"], "packed byte count without data"));
        }
        Ok(())
    }
    fn packed(&self, v: View) -> T<bool> {
        // Neither a populated PackedFile pointer nor a valid list head proves
        // the rest of the packed subtree. Check every link and the saved byte
        // allocation extent, without claiming image/content validity.
        let mut found = false;
        if self.has(v, &["packedfile"]) && self.address(v, &["packedfile"])? != 0 {
            let packed = self
                .checked_packed_pointer(v, &["packedfile"])?
                .ok_or_else(|| {
                    self.invalid(v, &["packedfile"], "nonzero packed pointer has no target")
                })?;
            self.packed_bytes(packed)?;
            found = true;
        }
        if self.has(v, &["packedfiles", "first"]) {
            let mut next = self.checked_packed_pointer(v, &["packedfiles", "first"])?;
            let mut seen = HashSet::new();
            while let Some(entry) = next {
                if !seen.insert(entry) {
                    let mut error = self.invalid(entry, &["next"], "cyclic packed-file list");
                    error.kind = FailureKind::Cycle;
                    return Err(error);
                }
                let packed = self
                    .checked_packed_pointer(entry, &["packedfile"])?
                    .ok_or_else(|| {
                        self.invalid(
                            entry,
                            &["packedfile"],
                            "packed-file list entry without bytes",
                        )
                    })?;
                self.packed_bytes(packed)?;
                found = true;
                next = self.checked_packed_pointer(entry, &["next"])?;
            }
        }
        Ok(found)
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
            let before = self.report.usages.len();
            self.full(v, &[p], matches!(source, 2 | 6), name)?;
            if self.report.usages.len() > before {
                if source == 2 {
                    // Consumers in other loaded files may be discovered later.
                    self.report.usages[before].pattern_image = Some(v);
                    self.report.usages[before].pattern_unknown =
                        Some("sequence ImageUser span has not been resolved".into());
                } else if source == 6 {
                    let asset = self.report.usages[before].asset_path.clone();
                    let (members, unknown, derived) = self.image_members(v, source, &asset)?;
                    self.report.usages[before].expected_members = members;
                    self.report.usages[before].pattern_unknown = unknown;
                    // Blender treats an auto-tokenized concrete basename as
                    // a UDIM set on load (image.cc:3597/3773).
                    if derived {
                        self.report.usages[before].is_udim = true;
                    }
                }
            }
        }
        Ok(())
    }
    fn image_users_for(&mut self, image: View) -> T<Vec<View>> {
        let address = self
            .reader(image)
            .map_err(|e| self.fault(image, &[], e))?
            .blocks()[image.block]
            .old_address;
        if !self.image_users.contains_key(&image.file) {
            let mut index = HashMap::<u64, Vec<View>>::new();
            let blocks = self
                .reader(image)
                .map_err(|e| self.fault(image, &[], e))?
                .blocks()
                .len();
            for block in 0..blocks {
                let b = &self
                    .reader(image)
                    .map_err(|e| self.fault(image, &[], e))?
                    .blocks()[block];
                if b.count == 0 || b.code == *b"DNA1" {
                    continue;
                }
                let ty = self
                    .reader(image)
                    .map_err(|e| self.fault(image, &[], e))?
                    .dna_type_id(block)
                    .map_err(|e| self.fault(image, &[], e))?
                    .to_vec();
                if !matches!(ty.as_slice(), b"bNode" | b"Tex") {
                    continue;
                }
                let count = self
                    .reader(image)
                    .map_err(|e| self.fault(image, &[], e))?
                    .blocks()[block]
                    .count;
                for element in 0..count {
                    let view = View {
                        file: image.file,
                        block,
                        element,
                    };
                    let pointer = if ty == b"Tex" { "ima" } else { "id" };
                    if self.has(view, &[pointer]) {
                        let target = self.address(view, &[pointer])?;
                        if target != 0 {
                            index.entry(target).or_default().push(view);
                        }
                    }
                }
            }
            self.image_users.insert(image.file, index);
        }
        Ok(self.image_users[&image.file]
            .get(&address)
            .cloned()
            .unwrap_or_default())
    }
    fn image_members(
        &mut self,
        image: View,
        source: i64,
        asset: &[u8],
    ) -> T<(Vec<Vec<u8>>, Option<String>, bool)> {
        let mut members = BTreeSet::new();
        if source == 6 {
            if !self.has(image, &["tiles", "first"]) {
                return Ok((
                    Vec::new(),
                    Some("saved Image tile list unavailable".into()),
                    false,
                ));
            }
            let first = self.optptr(image, &["tiles", "first"])?;
            let tiles = self.list(first, &["next"])?;
            if tiles.is_empty() {
                return Ok((
                    Vec::new(),
                    Some("saved Image tile list is empty".into()),
                    false,
                ));
            }
            // Blender load rule, not save validation: `BKE_image_get_tile_info`
            // (image.cc:3597) keeps the saved `Image.tiles` list and
            // auto-tokenizes a concrete basename with
            // `BKE_image_ensure_tile_token_filename_only` (image.cc:3773)
            // before expanding. The save-time token refusal
            // (`BKE_image_get_tile_strformat` returning
            // `UDIM_TILE_FORMAT_NONE`, image_save.cc:667) never applies here.
            let derived = if filename_tile(asset) {
                None
            } else {
                let name_start = asset
                    .iter()
                    .rposition(|b| matches!(b, b'/' | b'\\'))
                    .map_or(0, |n| n + 1);
                match auto_tile_token(&asset[name_start..]) {
                    Some(tokenized) => {
                        let mut pattern = asset[..name_start].to_vec();
                        pattern.extend_from_slice(&tokenized);
                        Some(pattern)
                    }
                    None => {
                        return Ok((
                            Vec::new(),
                            Some(
                                "tiled Image lacks a supported filename token (<UDIM>/<UVTILE>)"
                                    .into(),
                            ),
                            false,
                        ));
                    }
                }
            };
            let pattern = derived.as_deref().unwrap_or(asset);
            for tile in &tiles {
                let number = self.num(*tile, &["tile_number"])?;
                if number < 1001 {
                    return Ok((
                        Vec::new(),
                        Some("invalid saved ImageTile number".into()),
                        false,
                    ));
                }
                let Some(path) = tile_member(pattern, number) else {
                    return Ok((
                        Vec::new(),
                        Some(
                            "tiled Image lacks a supported filename token (<UDIM>/<UVTILE>)".into(),
                        ),
                        false,
                    ));
                };
                members.insert(path);
            }
            return Ok((members.into_iter().collect(), None, derived.is_some()));
        }
        // Associate ImageUser with its actual image pointer. A free-standing
        // ImageUser allocation cannot prove which image it uses. Enumerating
        // unrelated durations would invent requirements for the wrong image.
        let mut users = Vec::new();
        let mut pending = vec![image];
        let mut seen = HashSet::new();
        while let Some(view) = pending.pop() {
            if !seen.insert(view) {
                continue;
            }
            users.extend(self.image_users_for(view)?);
            let key = (raw(&self.file(view).path), self.name(view)?);
            if let Some(proxies) = self.linked_images.get(&key) {
                pending.extend(proxies.iter().copied());
            }
        }
        let mut found = false;
        let mut unknown = None;
        for view in users {
            let ty = self.ty(view)?;
            let (user, prefix) = if ty == b"Tex" {
                (view, vec!["iuser"])
            } else {
                let Some(storage) = self.node_storage(view)? else {
                    unknown = Some("image node has no saved ImageUser storage".into());
                    continue;
                };
                match self.ty(storage)?.as_slice() {
                    // Resolve the embedded ImageUser by the file's SDNA field path,
                    // never by a layout offset. Cryptomatte's render source points
                    // at a Scene (or null), so image_users_for cannot associate it
                    // with this Image; only its image source reaches this branch.
                    b"NodeTexImage" | b"NodeTexEnvironment" | b"NodeCryptomatte" => {
                        (storage, vec!["iuser"])
                    }
                    b"ImageUser" => (storage, vec![]),
                    _ => {
                        unknown = Some("image node has unsupported ImageUser storage".into());
                        continue;
                    }
                }
            };
            let mut frames_path = prefix.clone();
            frames_path.push("frames");
            let mut offset_path = prefix.clone();
            offset_path.push("offset");
            let mut flag_path = prefix.clone();
            flag_path.push("flag");
            let mut current_path = prefix;
            current_path.push("framenr");
            let frames = self.num(user, &frames_path)?;
            let offset = self.num(user, &offset_path)?;
            // Duration is the saved playback span; start frame and cyclic
            // playback change when members are used, not this declared span.
            // Negative offsets need separate native boundary qualification.
            if frames <= 0 || offset < 0 {
                unknown = Some(
                    "nonpositive ImageUser duration or negative offset is not qualified".into(),
                );
                continue;
            }
            let Some(end) = offset.checked_add(frames) else {
                unknown = Some("ImageUser frame span overflows".into());
                continue;
            };
            if !self.has(user, &flag_path) || !self.has(user, &current_path) {
                unknown = Some("saved ImageUser refresh flag/current frame unavailable".into());
                continue;
            }
            // Native Blender 5.2.1 paired saves: use_auto_refresh false/true
            // changes ImageUser.flag 1024 -> 1025. With bit0 clear, rendering
            // preserves saved framenr (including 0), not the playback span.
            if self.num(user, &flag_path)? & 1 == 0 {
                let current = self.num(user, &current_path)?;
                if current < 0 {
                    unknown =
                        Some("negative saved ImageUser current frame is not qualified".into());
                    continue;
                }
                if let Some(path) = sequence_member(asset, current) {
                    members.insert(path);
                } else {
                    unknown = Some("sequence Image filename has no numeric frame suffix".into());
                }
            }
            found = true;
            for frame in offset + 1..=end {
                if let Some(path) = sequence_member(asset, frame) {
                    members.insert(path);
                } else {
                    unknown = Some("sequence Image filename has no numeric frame suffix".into());
                    break;
                }
            }
        }
        if !found && unknown.is_none() {
            unknown = Some(
                "no associated saved ImageUser playback span proved for sequence Image".into(),
            );
        }
        Ok((members.into_iter().collect(), unknown, false))
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
        self.reader(v)
            .map_err(|e| self.fault(v, &[], e))?
            .dna_type_id(v.block)
            .map(Vec::from)
            .map_err(|e| self.fault(v, &[], e))
    }
    fn string_pointer(&mut self, v: View, p: &[&str]) -> T<Vec<u8>> {
        let Some(s) = self.optptr(v, p)? else {
            return Ok(Vec::new());
        };
        let bytes = self
            .reader(s)
            .map_err(|e| self.fault(v, p, e))?
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
        if count as usize
            > self
                .reader(bakes)
                .map_err(|e| self.fault(m, &["bakes"], e))?
                .blocks()[bakes.block]
                .count
        {
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
        let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
        let b = &reader.blocks()[v.block];
        if b.count == 0 || b.code == *b"DNA1" {
            return Ok(());
        }
        let block_code = code(&b.code);
        let ty = self.ty(v)?;
        if block_code.len() != 2 && ty != b"bNodeTree" {
            return Ok(());
        }
        // Recognizing CA must not admit an unrelated asset struct under that
        // header code. Preserve the unknown-family diagnostic, not Image/etc.
        // dispatch through a mislabelled Camera header.
        if block_code == b"CA" && ty != b"Camera" {
            self.unknown_asset(v, block_code);
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
                    | b"CA"
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
                // Path facts and pointer/type faults survive an unknown storage
                // classification. Do not let the flag proof mask either.
                let before_path = self.report.errors.len();
                let asset_path = match self.string_pointer(v, &[field]) {
                    Ok(path) if path.is_empty() && self.report.errors.len() == before_path => {
                        return Ok(());
                    } // proved no path obligation
                    Ok(path) if path.is_empty() => None, // deref reported UNKNOWN, not NULL
                    Ok(path) => Some(path),
                    Err(error) => {
                        self.capture(*error);
                        None
                    }
                };
                let flags = (|| -> T<i64> {
                    let role = self.field(v, &["flags"])?;
                    // Native SDNA Text.flags is a four-byte int scalar (2.25+).
                    if role.pointer
                        || role.dna_type != b"int"
                        || role.size != 4
                        || role.full_name != b"flags"
                    {
                        return Err(self.invalid(
                            v,
                            &["flags"],
                            "Text.flags requires its saved int scalar role",
                        ));
                    }
                    self.num(v, &["flags"])
                })();
                let flags = match flags {
                    Ok(flags) => Some(flags),
                    Err(mut error) => {
                        if let Some(site) = error.site.as_mut().and_then(Json::as_object_mut) {
                            site.insert("text_storage".into(), json!("unknown"));
                        }
                        self.capture(*error);
                        None
                    }
                };
                let Some(asset_path) = asset_path else {
                    return Ok(());
                };
                // Native Blender Text.is_in_memory is saved TXT_ISMEM (bit 2).
                // Dirty state and serialized cached lines are independent:
                // externally backed Texts can be dirty and still own a path.
                if let Some(flags) = flags.filter(|flags| flags & (1 << 2) != 0) {
                    self.report
                        .warnings
                        .push(TraceWarning::EmbeddedTextFilepath {
                            owner_file: raw(&self.file(v).path),
                            block_address: self
                                .reader(v)
                                .map_err(|e| self.fault(v, &[], e))?
                                .blocks()[v.block]
                                .old_address,
                            block_name: self.name(v)?,
                            asset_path: asset_path.clone(),
                            flags,
                        });
                    if crate::diagnostics::enabled() {
                        crate::diagnostics::point(
                            "decision",
                            serde_json::json!({"item":crate::value::hex(&asset_path),"kind":"embedded_text_filepath","reason":"saved TXT_ISMEM: content is embedded; filepath is historical, not a dependency"}),
                        );
                    }
                } else {
                    // For unknown flags this is a filepath FACT, not an external
                    // verdict: the correlated unknown error forbids strict pack.
                    let provenance = self.field(v, &[field])?;
                    self.emit(
                        v,
                        asset_path,
                        false,
                        Some(self.name(v)?),
                        (Some(provenance), None, None),
                    )?;
                }
            }
            b"Camera" => self.camera(v)?,
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
    fn camera(&mut self, v: View) -> T<()> {
        // Older Camera DNA has no custom shader path. Modern mode is proven
        // from its actual scalar role, never inferred from a version stamp.
        if !self.has(v, &["custom_filepath"]) {
            return Ok(());
        }
        let path_field = self.field(v, &["custom_filepath"])?;
        if path_field.pointer
            || path_field.dna_type != b"char"
            || !path_field.full_name.starts_with(b"custom_filepath[")
        {
            return Err(self.invalid(
                v,
                &["custom_filepath"],
                "Camera.custom_filepath requires a fixed char array",
            ));
        }
        let path = self.bytes(v, &["custom_filepath"])?;
        let mode = (|| -> T<i64> {
            let role = self.field(v, &["custom_mode"])?;
            if role.pointer
                || role.dna_type != b"int"
                || role.size != 4
                || role.full_name != b"custom_mode"
            {
                return Err(self.invalid(
                    v,
                    &["custom_mode"],
                    "Camera.custom_mode requires its saved int scalar role",
                ));
            }
            match self.num(v, &["custom_mode"])? {
                mode @ (0 | 1) => Ok(mode),
                _ => Err(self.invalid(
                    v,
                    &["custom_mode"],
                    "unknown Camera.custom_mode; expected INTERNAL=0 or EXTERNAL=1",
                )),
            }
        })();
        let mode = match mode {
            Ok(mode) => Some(mode),
            Err(mut failure) => {
                if let Some(site) = failure.site.as_mut() {
                    site["camera_storage"] = json!("unknown");
                }
                self.capture(*failure);
                None
            }
        };
        if mode == Some(0) {
            // All root Texts are already asset roots. In selected libraries,
            // id_fields follows this checked Text ID through normal traversal.
            self.optptr(v, &["custom_shader"])?;
            if !path.is_empty() {
                self.report
                    .warnings
                    .push(TraceWarning::EmbeddedCameraFilepath {
                        owner_file: raw(&self.file(v).path),
                        block_address: self.reader(v).map_err(|e| self.fault(v, &[], e))?.blocks()
                            [v.block]
                            .old_address,
                        block_name: self.name(v)?,
                        asset_path: path.clone(),
                    });
                if crate::diagnostics::enabled() {
                    crate::diagnostics::point(
                        "decision",
                        serde_json::json!({"item":crate::value::hex(&path),"kind":"embedded_camera_filepath","reason":"saved INTERNAL custom camera: custom_filepath is historical; custom_shader Text owns content"}),
                    );
                }
            }
            return Ok(());
        }
        // Unknown mode still retains the known path fact, like Text's unknown
        // storage classification; the correlated error forbids strict pack.
        let before = self.report.usages.len();
        self.emit(v, path, false, None, (Some(path_field), None, None))?;
        if mode == Some(1) && self.report.usages.len() != before {
            let role = self.field(v, &["type"])?;
            if role.pointer
                || role.dna_type != b"char"
                || role.size != 1
                || role.full_name != b"type"
            {
                return Err(self.invalid(
                    v,
                    &["type"],
                    "Camera.type requires its saved char scalar role",
                ));
            }
            let camera_type = self.num(v, &["type"])?;
            if !matches!(camera_type, 0..=3) {
                return Err(self.invalid(v, &["type"], "unknown Camera.type; CUSTOM=3"));
            }
            let usage = &mut self.report.usages[before];
            usage.camera_type = Some(camera_type);
            usage.active = Some(camera_type == 3);
        }
        Ok(())
    }
    fn unknown_asset(&mut self, v: View, code: Vec<u8>) {
        if code.len() == 2
            && !crate::id_codes::non_file_family(&code, &self.ty(v).unwrap_or_default())
            && !matches!(
                code.as_slice(),
                b"GR" | b"CO" | b"WO" | b"LA" | b"WM" | b"SN" | b"LS" | b"ID"
            )
            && self.unknown.insert(code.clone())
        {
            self.report.warnings.push(TraceWarning::UnknownAssetCode {
                owner_file: raw(&self.file(v).path),
                code: code.clone(),
            });
            if crate::diagnostics::enabled() {
                crate::diagnostics::point(
                    "decision",
                    serde_json::json!({"item":String::from_utf8_lossy(&code).into_owned(),"kind":"unknown_asset_code","reason":"unrecognized two-byte block family retains no asset edge; independent facts continue"}),
                );
            }
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
        let mut dir = self.bytes(strip, &["dir"])?;
        while dir.len() > 2 && dir.last() == Some(&b'/') {
            dir.pop();
        }
        if !dir.ends_with(b"/") {
            dir.push(b'/');
        }
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
        let width = self
            .reader(v)
            .map_err(|e| self.fault(v, &["mat"], e))?
            .header()
            .pointer_size;
        let size = self
            .reader(table)
            .map_err(|e| self.fault(v, &["mat"], e))?
            .blocks()[table.block]
            .size;
        if count as usize > size / width {
            return Err(self.invalid(v, &["mat"], "material table exceeds saved bytes"));
        }
        for i in 0..count as usize {
            let address = self
                .reader(table)
                .map_err(|e| self.fault(v, &["mat"], e))?
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
        let width = self
            .reader(v)
            .map_err(|e| self.fault(v, p, e))?
            .header()
            .pointer_size;
        let steps = p
            .iter()
            .map(|s| PathStep::Field(s.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        let (offset, count) = {
            let location = self
                .reader(v)
                .map_err(|e| self.fault(v, p, e))?
                .locate_field(v.block, v.element, &steps)
                .map_err(|e| self.fault(v, p, e))?;
            (location.offset, location.field.array_len)
        };
        if !field.pointer {
            return Err(self.invalid(v, p, "expected pointer array"));
        }
        for i in 0..count {
            let address = self
                .reader(v)
                .map_err(|e| self.fault(v, p, e))?
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
                        || count as usize
                            > self
                                .reader(array)
                                .map_err(|e| self.fault(v, &["len"], e))?
                                .blocks()[array.block]
                                .count
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
            let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
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
        // Each reader borrow below is a short observation: it ends before
        // any graph mutation or arena admission in the same iteration.
        let si = {
            let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
            reader
                .struct_index(v.block)
                .map_err(|e| self.fault(v, &[], e))?
        };
        let unhandled = {
            let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
            reader.schema().unhandled_path(si)
        };
        if unhandled {
            return Err(self.fault(
                v,
                &[],
                Error::new(
                    "unprofiled_role",
                    "declared DATA contains an unhandled path-like field",
                ),
            ));
        }
        // L19+s3: schema-owned structural program (compiled once per used
        // Schema/type, cached in OnceLock) plus one borrowed payload view
        // per block. Dense blocks hold a single retained-window `Arc`;
        // per-field work reads its pointer bytes from the slice. The
        // pre-pass iterates a fresh pointer_sites() iterator (cheap site
        // handles; the program itself is cached, and a second identical
        // iterator serves the resolution loop below, so no per-block Vec
        // of layouts is ever materialized and no whole-block copy happens
        // per field). Fill guards run on window misses exactly as before
        // (hits never re-checked in either design); address failures keep
        // the identical fault path and kind, fail-fast.
        let addresses: Vec<u64> = {
            let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
            let view = reader
                .payload_view(v.block)
                .map_err(|e| self.fault(v, &[], e))?;
            let bytes = view.as_slice();
            let pointer_size = reader.header().pointer_size;
            let little_endian = reader.header().little_endian;
            crate::file_ops::pointer_sites(reader, v.block)
                .map_err(|e| self.fault(v, &[], e))?
                .ok_or_else(|| self.invalid(v, &[], "declared DATA lacks typed pointer layout"))?
                .map(|site| {
                    crate::checked::number(bytes, site.offset, pointer_size, little_endian)
                        .map_err(|e| self.fault(v, &[&site.layout().path], e))
                })
                .collect::<std::result::Result<Vec<u64>, Box<TraceFailure>>>()?
        };
        let sites = {
            let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
            crate::file_ops::pointer_sites(reader, v.block)
                .map_err(|e| self.fault(v, &[], e))?
                .ok_or_else(|| self.invalid(v, &[], "declared DATA lacks typed pointer layout"))?
        };
        for (site, address) in sites.zip(addresses) {
            if address == 0 {
                continue;
            }
            let field = site.layout();
            let result = {
                let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
                (|| {
                    let target = reader
                        .owner_index()?
                        .resolve(reader, v.block, address, &field)?;
                    crate::file_ops::roles::qualify(reader, v.block, &field, target)?;
                    Ok::<_, Error>(target)
                })()
            };
            let result = match result {
                Err(error) if error.kind == "unprofiled_role" => {
                    let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
                    match crate::file_ops::roles::declared_intrusive_list_target(
                        reader, v.block, &field,
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
                    let mut failure = self.fault(v, &[&field.path], error);
                    if let Some(site) = failure.site.as_mut() {
                        site["pointer_value_decimal"] = json!(address.to_string());
                        site["expected_target_type_hex"] = json!(hex(&field.declared_type));
                        site["expected_target_type"] =
                            json!(std::str::from_utf8(&field.declared_type).ok());
                    }
                    if matches!(failure.kind, FailureKind::DanglingPointer)
                        && v.file != 0
                        && self.library_null(v, field.offset, &failure)?
                    {
                        continue;
                    }
                    {
                        self.report.complete = false;
                        self.report.errors.push((*failure).clone());
                        self.report
                            .warnings
                            .push(TraceWarning::SilencedPointer { failure: *failure });
                        continue;
                    }
                }
            };
            // A qualified opaque byte/numeric allocation is a leaf, not an
            // instance of struct zero (which may coincidentally be an ID).
            let (is_zero, is_data) = {
                let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
                (
                    reader.blocks()[block].sdna_index == 0,
                    reader.blocks()[block].code == *b"DATA",
                )
            };
            if is_zero {
                continue;
            }
            let target = View {
                file: v.file,
                block,
                element: 0,
            };
            if is_data {
                self.declared_data.insert(target);
            }
            out.push(target);
        }
        Ok(())
    }
    fn expand(&mut self, v: View) -> Vec<View> {
        let mut out = Vec::new();
        // A later structural fault cannot erase edges already proved. The
        // failing owner's remaining expansion is unknown; independent queued
        // records and libraries still run without guessing the damaged edge.
        let result = (|| -> T<()> {
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
                        if let Some(data) =
                            self.deref_hop(v, &["data"], address, false, Some(b"ID"))?
                        {
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
                b"ChannelDriver" => {
                    self.addlist(v, &["variables", "first"], &["next"], &mut out)?
                }
                b"DriverVar" => self.embedded_id_array(v, "targets", &mut out)?,
                b"ViewLayer" => self.addlist(v, &["object_bases", "first"], &["next"], &mut out)?,
                b"bPose" => self.addlist(v, &["chanbase", "first"], &["next"], &mut out)?,
                b"bPoseChannel" => {
                    self.addlist(v, &["constraints", "first"], &["next"], &mut out)?
                }
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
            Ok(())
        })();
        if let Err(error) = result {
            self.capture(*error);
        }
        // The sort key is a saved file-local offset, not a stable identity.
        // Arena identity is append-only, so this lookup is infallible by
        // construction; keep the historical order even in the impossible
        // case rather than inventing a key.
        let mut keys: Vec<(usize, usize, usize)> = Vec::with_capacity(out.len());
        for v in &out {
            let offset = self
                .reader(*v)
                .map(|r| r.blocks()[v.block].data_offset)
                .unwrap_or(0);
            keys.push((offset, v.element, v.block));
        }
        let mut indexed: Vec<_> = out.drain(..).zip(keys).collect();
        // Stable by the historical (offset, element) key only.
        indexed.sort_by_key(|(_, (offset, element, _))| (*offset, *element));
        out.extend(indexed.into_iter().map(|(v, _)| v));
        out.dedup();
        out
    }
    fn embedded_id_array(&mut self, v: View, field: &str, out: &mut Vec<View>) -> T<()> {
        if !self.has(v, &[field]) {
            return Ok(());
        }
        let steps = [PathStep::Field(field.as_bytes().to_vec())];
        let count = self
            .reader(v)
            .map_err(|e| self.fault(v, &[field], e))?
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
            let result = self
                .reader(v)
                .map_err(|e| self.fault(v, &[field], e))?
                .get_pointer(v.block, v.element, &steps);
            let target = match result {
                Ok(target) => target,
                Err(error) => {
                    let failure = self.fault(v, &[&format!("{field}[{i}].id")], error);
                    if matches!(failure.kind, FailureKind::DanglingPointer) && v.file != 0 {
                        let offset = self
                            .reader(v)
                            .map_err(|e| self.fault(v, &[field], e))?
                            .locate_field(v.block, v.element, &steps)
                            .map_err(|e| self.fault(v, &[field], e))?
                            .offset;
                        if self.library_null(v, offset, &failure)? {
                            continue;
                        }
                    }
                    return Err(failure);
                }
            };
            if let Some(block) = target {
                let reader = self.reader(v).map_err(|e| self.fault(v, &[field], e))?;
                let offset = reader
                    .locate_field(v.block, v.element, &steps)
                    .map_err(|e| self.fault(v, &[field], e))?
                    .offset;
                crate::library_compat::validate_present(reader, v.block, offset, block)
                    .map_err(|e| self.fault(v, &[field], e))?;
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
        // Report-carried error kinds reconcile by kind/item identity.
        // Never synthesize a success span for a captured unknown subtree.
        if crate::diagnostics::enabled() {
            let item = e
                .missing_id
                .as_ref()
                .map(|m| crate::value::hex(&m.id_name))
                .or_else(|| e.library_path.as_ref().map(|p| crate::value::hex(p)))
                .unwrap_or_else(|| {
                    format!(
                        "{}:{}",
                        crate::value::hex(&e.owner_file),
                        e.block_address.map(|a| a.to_string()).unwrap_or_default()
                    )
                });
            crate::diagnostics::point(
                "error",
                serde_json::json!({"kind":e.kind_str(),"message":e.message,"item":item,"owner_file_hex":crate::value::hex(&e.owner_file)}),
            );
        }
        self.report.complete = false;
        self.report.errors.push(e)
    }
    fn file_trace(&mut self, fi: usize, names: Option<Vec<Vec<u8>>>, captured: bool) -> T<()> {
        let mut resolved = VecDeque::new();
        let selection = names.map_or(FileSelection::All, FileSelection::Names);
        self.file_trace_chain(
            fi,
            selection,
            self.files[fi].chain.clone(),
            &mut resolved,
            captured,
        )?;
        self.expand_resolved(&mut resolved, captured);
        Ok(())
    }
    fn expand_resolved(&mut self, resolved: &mut VecDeque<ResolvedSelection>, captured: bool) {
        // Resolve saved selectors to local endpoints BEFORE following any of
        // their dependency edges. A dependency may reuse a completed selector
        // from the same batch; it is not an active forwarding-name loop.
        while let Some(ResolvedSelection {
            file,
            endpoints,
            chain,
        }) = resolved.pop_front()
        {
            debug_assert!(self.active_stubs.is_empty());
            if let Err(e) = self.file_trace_chain(
                file,
                FileSelection::Endpoints(endpoints),
                chain,
                resolved,
                captured,
            ) {
                self.capture(*e);
            }
        }
    }
    fn file_trace_chain(
        &mut self,
        fi: usize,
        selection: FileSelection,
        request_chain: Vec<Vec<u8>>,
        resolved: &mut VecDeque<ResolvedSelection>,
        captured: bool,
    ) -> T<()> {
        let phase = if matches!(&selection, FileSelection::Endpoints(_)) {
            "expand"
        } else {
            "resolve"
        };
        let mut span = crate::diagnostics::Span::new(
            "walk.file",
            if crate::diagnostics::enabled() {
                json!({"path":self.files[fi].path.to_string_lossy(),"selected":!matches!(&selection, FileSelection::All),"phase":phase})
            } else {
                serde_json::Value::Null
            },
        );
        let before = self.visited.len();
        let active = self.active_stubs.len();
        let result = self.file_trace_inner(fi, selection, request_chain, resolved, captured);
        self.active_stubs.truncate(active);
        span.data(json!({"visited":self.visited.len()-before,"usages":self.report.usages.len(),"complete":self.report.complete}));
        span.end(if result.is_err() {
            "error"
        } else if !self.report.complete {
            "partial"
        } else {
            "ok"
        });
        result
    }
    fn file_trace_inner(
        &mut self,
        fi: usize,
        selection_mode: FileSelection,
        request_chain: Vec<Vec<u8>>,
        resolved: &mut VecDeque<ResolvedSelection>,
        captured: bool,
    ) -> T<()> {
        let asset_roots = matches!(&selection_mode, FileSelection::All);
        let resolving_names = matches!(&selection_mode, FileSelection::Names(_));
        let names = match &selection_mode {
            FileSelection::Names(names) => Some(names),
            _ => None,
        };
        // Only ancestor passes are active recursion. Stubs completed earlier
        // in THIS selection pass may be re-reached by independent owners.
        let inherited_stubs = self.active_stubs.len();
        // Essential walk work stays visible at L1; source borrows remain short.
        let mut selection = crate::diagnostics::Span::new(
            "walk.select",
            if crate::diagnostics::enabled() {
                json!({"path":self.files[fi].path.to_string_lossy()})
            } else {
                serde_json::Value::Null
            },
        );
        let origin = View {
            file: fi,
            block: 0,
            element: 0,
        };
        // The arena borrow below is a short observation; it ends before any
        // graph mutation or nested admission in this file pass.
        let mut eligible = Vec::new();
        if asset_roots {
            let reader = self.reader_of(fi).map_err(|e| self.fault(origin, &[], e))?;
            eligible.extend(reader.blocks().iter().enumerate().filter_map(|(i, b)| {
                (b.count > 0
                    && (code(&b.code).len() == 2
                        || reader.dna_type_id(i).is_ok_and(|ty| ty == b"bNodeTree")))
                .then_some(i)
            }));
        } else if resolving_names {
            let reader = self.reader_of(fi).map_err(|e| self.fault(origin, &[], e))?;
            if reader.name_index.get().is_none() {
                // Shared local-first checked resolver; no arena borrow
                // survives into graph mutation or nested library capture.
                named_blocks(reader, &[]).map_err(|e| self.fault(origin, &[], e))?;
            }
        }
        if let Some(requested) = names {
            for name in requested {
                let (candidates, all_forwarding, targets) = {
                    let reader = self.reader_of(fi).map_err(|e| self.fault(origin, &[], e))?;
                    let candidates = selected_blocks(
                        reader.name_index.get().unwrap(),
                        std::slice::from_ref(name),
                    );
                    let all_forwarding = candidates.len() > 1
                        && candidates
                            .iter()
                            .all(|i| reader.blocks()[*i].code == *b"ID\0\0");
                    let mut targets = BTreeSet::new();
                    for block in &candidates {
                        if let Ok(Some(lib)) =
                            reader.get_pointer(*block, 0, &[PathStep::Field(b"lib".to_vec())])
                            && let Ok(Value::Bytes(path)) =
                                reader.get(lib, 0, &[PathStep::Field(b"name".to_vec())], true)
                        {
                            targets.insert(raw(&resolve(&self.files[fi].path, &path)));
                        }
                    }
                    (candidates, all_forwarding, targets)
                };
                if all_forwarding && targets.len() > 1 {
                    let first = *candidates.iter().min().unwrap();
                    let mut error=self.fault(View{block:first,..origin},&["name"],
                        Error::new("ambiguous_id","multiple forwarding libraries match this saved selector; all candidates retained, selected origin unknown"));
                    if let Some(site) = error.site.as_mut() {
                        site["id_name_hex"] = json!(hex(name));
                        site["candidate_library_paths_hex"] =
                            json!(targets.iter().map(|p| hex(p)).collect::<Vec<_>>());
                        site["referencing_owner_chain_hex"] =
                            json!(request_chain.iter().map(|p| hex(p)).collect::<Vec<_>>());
                    }
                    self.capture(*error);
                }
                if candidates.is_empty() {
                    // A corrupt candidate of this family is not absence. Its
                    // missing name must not rescue a malformed native header.
                    let qualified: Vec<usize> = {
                        let reader = self.reader_of(fi).map_err(|e| self.fault(origin, &[], e))?;
                        reader
                            .blocks()
                            .iter()
                            .enumerate()
                            .filter(|(_, block)| {
                                name.len() >= 2
                                    && block.code[..2] == name[..2]
                                    && crate::id_codes::expected(&block.code).is_some()
                            })
                            .map(|(i, _)| i)
                            .collect()
                    };
                    for i in qualified {
                        let reader = self.reader_of(fi).map_err(|e| self.fault(origin, &[], e))?;
                        crate::file_ops::qualify_read_id(reader, i)
                            .map_err(|e| self.fault(View { block: i, ..origin }, &[], e))?;
                    }
                    let owners = request_chain[..request_chain.len().saturating_sub(1)].to_vec();
                    let failure = TraceFailure {
                        diagnostic: None,
                        kind: FailureKind::MissingId,
                        owner_file: owners
                            .last()
                            .cloned()
                            .unwrap_or_else(|| raw(&self.files[fi].path)),
                        block_address: None,
                        field_path: vec![b"id".to_vec(), b"name".to_vec()],
                        library_path: Some(raw(&self.files[fi].path)),
                        site: None,
                        missing_id: Some(MissingId {
                            root_file: request_chain[0].clone(),
                            id_code: name.iter().copied().take(2).collect(),
                            id_name: name.clone(),
                            referencing_owner_chain: owners,
                        }),
                        message: format!(
                            "selected linked ID absent from source library: {}",
                            crate::value::display_bytes(name)
                        ),
                    };
                    self.capture(failure);
                }
            }
        }
        let mut queue: BTreeSet<View> = match selection_mode {
            FileSelection::Endpoints(endpoints) => endpoints,
            mode => {
                let seeds: BTreeSet<_> = match mode {
                    FileSelection::Names(names) => {
                        let reader = self.reader_of(fi).map_err(|e| self.fault(origin, &[], e))?;
                        selected_blocks(reader.name_index.get().unwrap(), &names)
                            .into_iter()
                            .collect()
                    }
                    _ => eligible.into_iter().collect(),
                };
                seeds
                    .into_iter()
                    .map(|block| View {
                        file: fi,
                        block,
                        element: 0,
                    })
                    .collect()
            }
        };
        if crate::diagnostics::enabled() {
            let file = &self.files[fi].path;
            for view in &queue {
                let block = view.block;
                crate::diagnostics::point(
                    "decision",
                    json!({"item":format!("{}:block:{block}", crate::value::hex(file.as_os_str().as_encoded_bytes())),"kind":"walk_seed_selected","reason":if asset_roots {"root asset census"} else if resolving_names {"saved library selector matched qualified candidate"} else {"resolved endpoint expansion"},"block_index":block}),
                );
            }
        }
        if selection.active() {
            selection.data(serde_json::json!({"seeds":queue.len()}));
        }
        selection.end(if !self.report.complete {
            "partial"
        } else {
            "ok"
        });
        let mut edges = crate::diagnostics::Span::new(
            "walk.edges",
            if crate::diagnostics::enabled() {
                serde_json::json!({"seeds":queue.len()})
            } else {
                serde_json::Value::Null
            },
        );
        let mut local = BTreeSet::new();
        let mut endpoints = BTreeSet::new();
        let mut libraries: BTreeMap<PathBuf, LibraryRequest> = BTreeMap::new();
        while let Some(v) = queue.pop_first() {
            if self.active_stubs[..inherited_stubs].contains(&v) {
                let mut failure = self.invalid(v, &[], "cyclic linked ID stub chain");
                failure.kind = FailureKind::Cycle;
                // The active forwarding ancestry check owns this failure.
                let error = Error::new("cycle", failure.message.clone());
                let mut source = crate::failure_receipt::SourceContext::block(
                    self.reader(v).map_err(|e| self.fault(v, &[], e))?,
                    v.block,
                );
                source.file = Some(self.files[v.file].path.clone());
                source.element = Some(v.element);
                source.namespace = crate::failure_receipt::Namespace::Id;
                source.resolution = crate::failure_receipt::Resolution::Unknown;
                crate::failure_receipt::record_source_failure(
                    &error,
                    "linked_id.forwarding.cycle",
                    source,
                    &[],
                );
                failure.diagnostic = error.diagnostic.get().cloned();
                self.capture(*failure);
                continue;
            }
            if self.visited.contains(&v) {
                continue;
            }
            let result = (|| -> T<()> {
                // These saved ID families have no dependency-tracing role.
                // In particular405 WindowManager carries process-only property
                // addresses without BHeads. Do not turn them into asset edges.
                // Short arena observations end before any mutation below.
                let file_code = {
                    let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
                    reader.blocks()[v.block].code
                };
                if matches!(code(&file_code).as_slice(), b"WM" | b"SN" | b"LS") {
                    self.visited.insert(v);
                    return Ok(());
                }
                // A linked name is an interpretation of the actual library ID,
                // not merely its placeholder in the parent. Share the same native
                // header/count/source-SDNA proof used by Object.data's read role.
                if !asset_roots && crate::id_codes::expected(&file_code).is_some() {
                    let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
                    crate::file_ops::qualify_read_id(reader, v.block)
                        .map_err(|e| self.fault(v, &[], e))?;
                }
                let library_path = if self.ty(v)? == b"ID" {
                    &["lib"][..]
                } else {
                    &["id", "lib"][..]
                };
                let linked = self.has(v, library_path) && self.address(v, library_path)? != 0;
                if linked || self.ty(v)? == b"ID" {
                    self.visited.insert(v);
                    let Some(lib) = self.optptr(v, library_path)? else {
                        return Err(self.invalid(
                            v,
                            library_path,
                            "linked identifier has no library",
                        ));
                    };
                    let target = self.bytes(lib, &["name"])?;
                    if target.is_empty() {
                        return Err(self.invalid(lib, &["name"], "linked library has empty path"));
                    }
                    if uncertain_path(&target) {
                        self.report
                            .warnings
                            .push(TraceWarning::PathResolutionUnverified {
                                owner_file: raw(&self.files[fi].path),
                                asset_path: target.clone(),
                                resolved_path: raw(&resolve(&self.files[fi].path, &target)),
                            });
                        if crate::diagnostics::enabled() {
                            crate::diagnostics::point(
                                "decision",
                                serde_json::json!({"item":crate::value::hex(&target),"kind":"path_resolution_unverified","reason":"foreign-root loader I/O cannot be safely relocated; retained as unknown"}),
                            );
                        }
                    }
                    let target = resolve(&self.files[fi].path, &target);
                    let name = self.name(v)?;
                    if name.starts_with(b"IM") {
                        self.linked_images
                            .entry((raw(&target), name.clone()))
                            .or_default()
                            .push(v);
                    }
                    let request = libraries.entry(target).or_insert_with(|| LibraryRequest {
                        library: lib,
                        names: BTreeSet::new(),
                        stubs: Vec::new(),
                    });
                    request.names.insert(name);
                    request.stubs.push(v);
                    local.insert(lib);
                    return Ok(());
                }
                if resolving_names {
                    endpoints.insert(v);
                    return Ok(());
                }
                self.visited.insert(v);
                local.insert(v);
                if !asset_roots {
                    if let Some(definition) = self.optptr(v, &["id", "override_library"])?
                        && let Some(reference) = self.optptr(definition, &["reference"])?
                    {
                        // Native Blender loads the original reference AND its
                        // dependencies, even when the local override removes an
                        // edge. Follow this exact ID through normal qualification
                        // and stub resolution, never census unrelated headers.
                        queue.insert(reference);
                    }
                    queue.extend(self.expand(v));
                }
                Ok(())
            })();
            if let Err(e) = result {
                // A malformed endpoint has no expansion work, but remains a
                // visited unknown just as before the phase separation.
                self.visited.insert(v);
                self.capture(*e);
            }
        }
        if edges.active() {
            edges.data(
                serde_json::json!({"visited":self.visited.len(),"libraries":libraries.len()}),
            );
        }
        edges.end(if !self.report.complete {
            "partial"
        } else {
            "ok"
        });
        if !endpoints.is_empty() {
            resolved.push_back(ResolvedSelection {
                file: fi,
                endpoints,
                chain: request_chain.clone(),
            });
        }
        let assets = crate::diagnostics::Span::new(
            "walk.assets",
            if crate::diagnostics::enabled() {
                serde_json::json!({"owners":local.len()})
            } else {
                serde_json::Value::Null
            },
        );
        for v in local {
            // DATA containers are traversed for references, never censused as
            // standalone assets. Embedded node trees are the sole tree owner.
            let file_code = {
                let reader = self.reader(v).map_err(|e| self.fault(v, &[], e))?;
                reader.blocks()[v.block].code
            };
            if (code(&file_code).len() == 2 || self.ty(v)? == b"bNodeTree")
                && let Err(e) = self.assets(v)
            {
                self.capture(*e);
            }
        }
        assets.end(if !self.report.complete {
            "partial"
        } else {
            "ok"
        });
        // Observable diagnostics follow the saved Library header order, not
        // lexical filesystem order (the latter reorders error/warning arrays).
        let libraries: Vec<_> = libraries.into_iter().collect();
        let mut order_keys: Vec<usize> = Vec::with_capacity(libraries.len());
        for (_, request) in &libraries {
            let offset = self
                .reader(request.library)
                .map_err(|e| self.fault(request.library, &[], e))?
                .blocks()[request.library.block]
                .data_offset;
            order_keys.push(offset);
        }
        let mut indexed: Vec<_> = libraries.into_iter().zip(order_keys).collect();
        // Stable by saved header order only, as before: equal offsets keep
        // their encounter order rather than re-sorting by block identity.
        indexed.sort_by_key(|(_, offset)| *offset);
        let libraries: Vec<_> = indexed.into_iter().map(|(entry, _)| entry).collect();
        for (
            target,
            LibraryRequest {
                library: lib,
                names,
                stubs,
            },
        ) in libraries
        {
            // Listing reports existing files, not missing-library diagnostics.
            // Existing but unreadable/malformed libraries still use the normal
            // reader failure path; default dependency tracing is unchanged.
            if self.library_policy == LibraryPolicy::ExistingOnly && !target.exists() {
                continue;
            }
            let mut child_chain = request_chain.clone();
            child_chain.push(raw(&target));
            // Under Session policy a changed linked source aborts this walk:
            // its latched identity reaches the completion finalizer instead
            // of disappearing inside a LibraryReader failure.
            let child = match self.open(&target, captured, child_chain.clone()) {
                Ok(child) => child,
                Err(error)
                    if self.op.policy == crate::session::GuardPolicy::Session
                        && error.kind == "input_changed" =>
                {
                    let (old_address, site) = match self.reader(lib) {
                        Ok(reader) => (
                            Some(reader.blocks()[lib.block].old_address),
                            Some(failure_site(
                                reader,
                                lib.block,
                                lib.element,
                                &[b"name".to_vec()],
                            )),
                        ),
                        Err(_) => (None, None),
                    };
                    return Err(Box::new(TraceFailure {
                        diagnostic: error.diagnostic.get().cloned(),
                        kind: FailureKind::Reader("input_changed"),
                        owner_file: raw(&self.files[fi].path),
                        block_address: old_address,
                        field_path: vec![b"name".to_vec()],
                        library_path: Some(raw(&target)),
                        missing_id: None,
                        site,
                        message: error.message,
                    }));
                }
                Err(error) => {
                    let missing = !target.exists();
                    let kind = if missing {
                        FailureKind::MissingLibrary
                    } else {
                        FailureKind::LibraryReader(error.kind)
                    };
                    let (old_address, site) = match self.reader(lib) {
                        Ok(reader) => (
                            Some(reader.blocks()[lib.block].old_address),
                            Some(failure_site(
                                reader,
                                lib.block,
                                lib.element,
                                &[b"name".to_vec()],
                            )),
                        ),
                        Err(_) => (None, None),
                    };
                    let failure = TraceFailure {
                        diagnostic: error.diagnostic.get().cloned(),
                        kind,
                        owner_file: raw(&self.files[fi].path),
                        block_address: old_address,
                        field_path: vec![b"name".to_vec()],
                        library_path: Some(raw(&target)),
                        missing_id: None,
                        site,
                        message: error.message,
                    };
                    if missing {
                        self.report.warnings.push(TraceWarning::MissingLibrary {
                            owner_file: failure.owner_file.clone(),
                            library_path: raw(&target),
                        });
                        if crate::diagnostics::enabled() {
                            crate::diagnostics::point(
                                "decision",
                                serde_json::json!({"item":target.to_string_lossy(),"kind":"missing_library","reason":failure.message}),
                            );
                        }
                    }
                    self.capture(failure);
                    continue;
                }
            };
            // Active forwarding belongs to this resolution branch only. A
            // completed sibling must not remain active for the whole file pass.
            let active = self.active_stubs.len();
            self.active_stubs.extend(stubs);
            let result = self.file_trace_chain(
                child,
                FileSelection::Names(names.into_iter().collect()),
                child_chain,
                resolved,
                captured,
            );
            self.active_stubs.truncate(active); // completion OR failure
            if let Err(e) = result {
                self.capture(*e);
            }
            // Preserve depth-first saved Library order where possible, but
            // never expand endpoints while any forwarding ancestry is active.
            if self.active_stubs.is_empty() {
                self.expand_resolved(resolved, captured);
            }
        }
        Ok(())
    }
}

/// Historical Result-only entry point. Routes through the operation core
/// with no observer, so pure library calls stay silent; use
/// [`trace_operation`] to retrieve the auxiliary diagnostics.
pub fn trace(root: &Path, options: TraceOptions) -> Result<TraceReport> {
    trace_operation(root, options).into_result()
}
/// Operation entry point: the guarded result plus every operation-local
/// diagnostic, on both success and failure.
pub fn trace_operation(
    root: &Path,
    options: TraceOptions,
) -> crate::session::Operation<TraceReport> {
    crate::tracing::initialize();
    let mut span =
        crate::diagnostics::Span::new("trace.operation", json!({"path":root.to_string_lossy()}));
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            let result = Err(crate::edit_output::io(error));
            span.record_result(&result);
            return crate::session::Operation::new(result, Vec::new());
        }
    };
    let temp_root = std::env::temp_dir();
    let mut op =
        crate::session::OpCtx::new(crate::session::GuardPolicy::Legacy, None, cwd, temp_root);
    let result = (|| {
        let root = crate::session::InputStore::resolve(&op, root);
        let mut store = crate::session::InputStore::default();
        let mut batch = crate::session::CaptureBatch::default();
        let (report, _) = trace_in_store(
            &mut store,
            &mut batch,
            &root,
            options,
            &mut op,
            LibraryPolicy::ReportMissing,
            false,
        )?;
        Ok(report)
    })();
    span.record_result(&result);
    op.finish(result)
}
/// CLI entry point: owns the explicit observer that renders the historical
/// stderr envelope synchronously at emission.
pub fn trace_report_cli(root: &Path, options: TraceOptions) -> Result<TraceReport> {
    crate::tracing::initialize();
    let cwd = std::env::current_dir().map_err(crate::edit_output::io)?;
    let temp_root = std::env::temp_dir();
    let mut op = crate::session::OpCtx::new(
        crate::session::GuardPolicy::Legacy,
        Some(crate::session::cli_observer),
        cwd,
        temp_root,
    );
    let root = crate::session::InputStore::resolve(&op, root);
    let mut store = crate::session::InputStore::default();
    let mut batch = crate::session::CaptureBatch::default();
    // Notices were already rendered by the observer; only the outcome
    // crosses the CLI boundary.
    let (report, _) = trace_in_store(
        &mut store,
        &mut batch,
        &root,
        options,
        &mut op,
        LibraryPolicy::ReportMissing,
        false,
    )?;
    Ok(report)
}
/// Session entry point: one guarded trace over the retained captured arena.
/// A full captured Session cannot honestly honour `low_memory`; that
/// request is refused as `usage` — but only after the root precheck, so a
/// changed capture is never masked by the refusal.
pub fn trace_session(
    session: &mut crate::session::Session,
    options: TraceOptions,
) -> crate::session::Operation<TraceReport> {
    if let Err(error) = session.require_single("trace") {
        return crate::session::Operation::new(Err(error), Vec::new());
    }
    crate::tracing::initialize();
    let mut span = crate::diagnostics::Span::new("session.trace", json!({}));
    let mut op = session.operation_context(None);
    let result = (|| {
        let root = session.root_path().to_path_buf();
        op.record_touch(session.root);
        // The precheck announces its own single diagnostic and never
        // enters the walk body.
        session.store.check_reuse(session.root, &mut op)?;
        if options.low_memory {
            let checks = session.store.check_all(session.root, &mut op);
            return crate::session::join_body::<TraceReport>(
                Err(Error::new(
                    "usage",
                    "captured Session trace cannot honour low_memory; use a path trace for spill/seek reads",
                ).into()),
                &checks,
                &mut op,
                "session.trace",
                Some(root),
            );
        }
        let mut batch = crate::session::CaptureBatch::with_memory(session.memory());
        let (report, _) = trace_in_store(
            &mut session.store,
            &mut batch,
            &root,
            options,
            &mut op,
            LibraryPolicy::ReportMissing,
            true,
        )?;
        Ok(report)
    })().map_err(Error::from);
    // Honest domain outcome: a returned incomplete report ends partial
    // with its error summary attached, never ok. Error results keep the
    // existing kind-to-outcome mapping; process outcome stays unknown.
    match &result {
        Ok(report) if !report.complete => {
            span.data(
                json!({"complete":false,"usages":report.usages.len(),"errors":report.errors.iter().map(|e|e.kind_str()).collect::<Vec<_>>()}),
            );
            span.end("partial");
        }
        _ => span.record_result(&result),
    }
    op.finish(result)
}
/// Legacy internal listing entry, kept for the existing walk-order tests.
/// Production listing (`bat_cli`) owns one operation across its walk and
/// its packed-resource second open instead of going through here.
#[cfg(test)]
pub(crate) fn listing_with_file_paths(
    root: &Path,
    options: TraceOptions,
) -> Result<(TraceReport, Vec<PathBuf>)> {
    crate::tracing::initialize();
    let cwd = std::env::current_dir().map_err(crate::edit_output::io)?;
    let temp_root = std::env::temp_dir();
    let mut op =
        crate::session::OpCtx::new(crate::session::GuardPolicy::Legacy, None, cwd, temp_root);
    let root = crate::session::InputStore::resolve(&op, root);
    let mut store = crate::session::InputStore::default();
    let mut batch = crate::session::CaptureBatch::default();
    // No inner closure: `?` applies directly since this entry returns
    // the walk result itself.
    trace_in_store(
        &mut store,
        &mut batch,
        &root,
        options,
        &mut op,
        LibraryPolicy::ExistingOnly,
        false,
    )
    .map_err(Error::from)
}
/// Shared traversal core: path preparation plus the supplied-store walk.
/// The store owns immutable parsed/index state; every call rebuilds its
/// root-specific traversal state on top.
#[allow(clippy::too_many_arguments)]
pub(crate) fn trace_in_store(
    store: &mut crate::session::InputStore,
    batch: &mut crate::session::CaptureBatch,
    root: &Path,
    options: TraceOptions,
    op: &mut crate::session::OpCtx,
    library_policy: LibraryPolicy,
    captured: bool,
) -> BodyResult<(TraceReport, Vec<PathBuf>)> {
    trace_in_store_policy(
        store,
        batch,
        root,
        options,
        op,
        library_policy,
        captured,
        false,
    )
}
/// Pack reports retain their owned capture-phase context; inspection reports
/// keep their established bare read-failure projection. One walker owns both.
#[allow(clippy::too_many_arguments)]
pub(crate) fn trace_in_store_for_pack(
    store: &mut crate::session::InputStore,
    batch: &mut crate::session::CaptureBatch,
    root: &Path,
    options: TraceOptions,
    op: &mut crate::session::OpCtx,
    library_policy: LibraryPolicy,
    captured: bool,
) -> BodyResult<(TraceReport, Vec<PathBuf>)> {
    trace_in_store_policy(
        store,
        batch,
        root,
        options,
        op,
        library_policy,
        captured,
        true,
    )
}
#[allow(clippy::too_many_arguments)]
fn trace_in_store_policy(
    store: &mut crate::session::InputStore,
    batch: &mut crate::session::CaptureBatch,
    root: &Path,
    options: TraceOptions,
    op: &mut crate::session::OpCtx,
    library_policy: LibraryPolicy,
    captured: bool,
    preserve_capture_context: bool,
) -> BodyResult<(TraceReport, Vec<PathBuf>)> {
    let session = op.policy == crate::session::GuardPolicy::Session;
    let retained = store.id_for_path(root).is_ok();
    let root_id = store
        .ensure(batch, root, captured, options.low_memory, op)
        .map_err(|error| {
            if session && retained {
                BodyErr::Guard {
                    source: root.to_path_buf(),
                    error,
                }
            } else {
                BodyErr::Independent(error)
            }
        })?;
    let mut paths = HashMap::new();
    paths.insert(root.to_path_buf(), 0);
    let mut ctx = Context {
        store,
        batch,
        op,
        options,
        library_policy,
        preserve_capture_context,
        files: vec![File {
            path: root.to_path_buf(),
            chain: vec![raw(root)],
            source: root_id,
        }],
        by_path: paths,
        report: TraceReport {
            usages: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            complete: true,
        },
        visited: HashSet::new(),
        active_stubs: Vec::new(),
        declared_data: HashSet::new(),
        dedup: HashSet::new(),
        image_users: HashMap::new(),
        linked_images: HashMap::new(),
        unknown: HashSet::new(),
    };
    // A Session walk aborted on a changed linked source propagates its
    // input_changed instead of capturing it as an ordinary trace failure:
    // the latched identity belongs to the completion finalizer.
    let mut aborted: Option<BodyErr> = None;
    if options.mode == TraceMode::PerFile {
        let blocks = ctx.reader_of(0).map(|r| r.blocks().len()).unwrap_or(0);
        for i in 0..blocks {
            let v = View {
                file: 0,
                block: i,
                element: 0,
            };
            if let Err(e) = ctx.assets(v) {
                ctx.capture(*e)
            }
        }
    } else if let Err(e) = ctx.file_trace(0, None, captured) {
        if session && matches!(e.kind, FailureKind::Reader("input_changed")) {
            let error = Error::new("input_changed", e.message.clone());
            aborted = Some(match &e.library_path {
                Some(source) => BodyErr::Guard {
                    source: path(source),
                    error,
                },
                None => BodyErr::Independent(error),
            });
        } else {
            ctx.capture(*e)
        }
    }

    // All checked linked-ID forwarding branches are now known. Union the saved
    // ImageUser spans in the image's own file and every referring file; a local
    // duration must not hide a longer root-file consumer of a linked image.
    for index in 0..ctx.report.usages.len() {
        if let Some(image) = ctx.report.usages[index].pattern_image {
            let asset = ctx.report.usages[index].asset_path.clone();
            match ctx.image_members(image, 2, &asset) {
                Ok((members, unknown, _)) => {
                    ctx.report.usages[index].expected_members = members;
                    ctx.report.usages[index].pattern_unknown = unknown;
                }
                Err(error) => ctx.capture(*error),
            }
        }
    }

    // Dependency reports include physical absence as incompleteness too.
    // The per-usage existence flag supplies the exact missing site.
    ctx.report.complete &= ctx.report.usages.iter().all(Usage::exists);
    // Release the context borrows before the completion checks.
    let Context {
        store,
        op,
        report,
        files,
        ..
    } = ctx;
    if !session {
        // Historical end guards with their error precedence: per-file
        // backing checks, reader stats, then the root Reader mapping.
        for file in &files {
            let reader = store.reader(file.source)?;
            reader.data.check_source()?;
            reader.data.report(&file.path);
        }
        for failure in &report.errors {
            if failure.owner_file == raw(root)
                && let FailureKind::Reader(kind @ ("io" | "truncated")) = failure.kind
            {
                return Err(Error::new(kind, failure.message.clone()).into());
            }
        }
        let paths = files.into_iter().map(|file| file.path).collect();
        return Ok((report, paths));
    }
    let checks = store.check_all(root_id, op);
    let paths = files.into_iter().map(|file| file.path).collect();
    let body: BodyResult<(TraceReport, Vec<PathBuf>)> = match aborted {
        Some(error) => Err(error),
        None => Ok((report, paths)),
    };
    crate::session::join_body(body, &checks, op, "trace.walk", Some(root.to_path_buf()))
}
#[cfg(test)]
mod serialized_vectors {
    use super::*;

    #[test]
    fn optional_field_probes_do_not_build_discarded_failure_sites() {
        let ctx =
            context(include_bytes!("../tests/fixtures/dependencies/group-root.blend").to_vec());
        for block in 0..ctx.reader_of(0).unwrap().blocks().len() {
            let v = View {
                file: 0,
                block,
                element: 0,
            };
            for path in [
                &[][..],
                &["not_a_saved_field"][..],
                &["id"][..],
                &["id", "name"][..],
                &["id", "not_a_saved_field"][..],
                &["lib", "name"][..],
            ] {
                let expected = ctx.field(v, path).is_ok();
                PROBE_FAULTS.with(|n| n.set(Some(0)));
                let actual = ctx.has(v, path);
                let faults = PROBE_FAULTS.with(|n| n.replace(None).unwrap());
                assert_eq!(actual, expected, "block {block}, {path:?}");
                assert_eq!(
                    faults, 0,
                    "boolean schema probes must not construct source-site diagnostics: {path:?}"
                );
            }
        }
    }

    // Independently authored wire-format vectors; no fixture-generator imports.
    type Structure<'a> = (&'a str, u16, Vec<(&'a str, &'a str)>);
    fn saved(structures: &[Structure<'_>], records: &[(&[u8], u64, usize, Vec<u8>)]) -> Vec<u8> {
        let mut types = vec!["char", "short", "int", "void", "float"];
        let mut lengths = vec![1u16, 2, 4, 0, 4];
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
        use crate::session::{CaptureBatch, GuardPolicy, InputStore, OpCtx};
        let reader = BlendFile::from_bytes(bytes).unwrap();
        let store: &'static mut InputStore = Box::leak(Box::new(InputStore::default()));
        let source = store.insert_inspection(PathBuf::from("/oxblend-test/test.blend"), reader);
        let batch: &'static mut CaptureBatch = Box::leak(Box::default());
        let op: &'static mut OpCtx = Box::leak(Box::new(OpCtx::new(
            GuardPolicy::Legacy,
            None,
            PathBuf::from("/oxblend-test"),
            std::env::temp_dir(),
        )));
        Context {
            store,
            batch,
            op,
            options: TraceOptions {
                mode: TraceMode::Recursive,
                pointers: PointerMode::Strict,
                low_memory: false,
            },
            library_policy: LibraryPolicy::ReportMissing,
            preserve_capture_context: false,
            files: vec![File {
                path: PathBuf::from("/oxblend-test/test.blend"),
                chain: vec![b"/oxblend-test/test.blend".to_vec()],
                source,
            }],
            by_path: HashMap::new(),
            report: TraceReport {
                usages: Vec::new(),
                errors: Vec::new(),
                warnings: Vec::new(),
                complete: true,
            },
            visited: HashSet::new(),
            active_stubs: Vec::new(),
            declared_data: HashSet::new(),
            dedup: HashSet::new(),
            image_users: HashMap::new(),
            linked_images: HashMap::new(),
            unknown: HashSet::new(),
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
        ctx.file_trace(0, None, false).unwrap();
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
    fn trace_and_listing_follow_selected_override_reference_without_censusing_unused_headers() {
        let structures = [
            (
                "ID",
                80,
                vec![
                    ("char", "name[64]"),
                    ("Library", "*lib"),
                    ("IDOverrideLibrary", "*override_library"),
                ],
            ),
            ("Object", 80, vec![("ID", "id")]),
            ("IDOverrideLibrary", 8, vec![("ID", "*reference")]),
            ("Library", 208, vec![("ID", "id"), ("char", "name[128]")]),
        ];
        let id = |name: &[u8], library: u64, definition: u64| {
            let mut data = fixed(name, 64);
            data.extend_from_slice(&library.to_le_bytes());
            data.extend_from_slice(&definition.to_le_bytes());
            data
        };
        let library = |name: &[u8], path: &[u8]| {
            let mut data = id(name, 0, 0);
            data.extend(fixed(path, 128));
            data
        };
        let bytes = saved(
            &structures,
            &[
                (b"OB", 1, 1, id(b"OBselected", 0, 2)),
                (b"DATA", 2, 2, 3u64.to_le_bytes().to_vec()),
                (b"ID", 3, 0, id(b"OBsource", 4, 0)),
                (b"LI", 4, 3, library(b"LIsource", b"//source.blend")),
                (b"LI", 5, 3, library(b"LIunused", b"//unused.blend")),
            ],
        );
        let mut selected_trace = context(bytes.clone());
        selected_trace
            .file_trace(0, Some(vec![b"OBselected".to_vec()]), false)
            .unwrap();
        assert_eq!(selected_trace.report.usages.len(), 1);
        assert_eq!(
            selected_trace.report.usages[0].asset_path,
            b"//source.blend"
        );
        assert_eq!(selected_trace.report.errors.len(), 1); // absent source, not unused header
        assert!(matches!(
            selected_trace.report.errors[0].kind,
            FailureKind::MissingLibrary
        ));
        let mut listing = context(bytes);
        listing.library_policy = LibraryPolicy::ExistingOnly;
        listing
            .file_trace(0, Some(vec![b"OBselected".to_vec()]), false)
            .unwrap();
        assert!(listing.report.errors.is_empty());
        assert_eq!(listing.report.usages.len(), 1);
        assert_eq!(listing.report.usages[0].asset_path, b"//source.blend");
        // Neither unrelated Library headers nor the reference ID's asset
        // graph are scanned; the physical source declaration is sufficient.
        assert_eq!(listing.files.len(), 1);
    }

    #[test]
    fn listing_retains_indirect_override_library_with_duplicate_filename() {
        // #32: follow an indirectly selected override's exact source identity,
        // even when another library has the same basename. Unrelated headers
        // still stay unselected.
        let structures = [
            (
                "ID",
                80,
                vec![
                    ("char", "name[64]"),
                    ("Library", "*lib"),
                    ("IDOverrideLibrary", "*override_library"),
                ],
            ),
            ("Object", 80, vec![("ID", "id")]),
            ("IDOverrideLibrary", 8, vec![("ID", "*reference")]),
            ("Library", 208, vec![("ID", "id"), ("char", "name[128]")]),
        ];
        let id = |name: &[u8], library: u64, definition: u64| {
            let mut data = fixed(name, 64);
            data.extend_from_slice(&library.to_le_bytes());
            data.extend_from_slice(&definition.to_le_bytes());
            data
        };
        let library = |name: &[u8], path: &[u8]| {
            let mut data = id(name, 0, 0);
            data.extend(fixed(path, 128));
            data
        };
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let work = Owned(std::env::temp_dir().join(format!(
            "oxblend-test-indirect-override-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )));
        std::fs::create_dir(&work.0).unwrap();
        std::fs::create_dir(work.0.join("env")).unwrap();
        std::fs::create_dir(work.0.join("props")).unwrap();
        let root = work.0.join("root.blend");
        let child = work.0.join("child.blend");
        let direct = work.0.join("env/trash.blend");
        let indirect = work.0.join("props/trash.blend");
        std::fs::write(
            &root,
            saved(
                &structures,
                &[
                    (b"ID", 1, 0, id(b"OBselected", 2, 0)),
                    (b"LI", 2, 3, library(b"LIchild", b"//child.blend")),
                    (b"ID", 3, 0, id(b"OBsource", 4, 0)),
                    (b"LI", 4, 3, library(b"LItrash.blend", b"//env/trash.blend")),
                ],
            ),
        )
        .unwrap();
        std::fs::write(
            &child,
            saved(
                &structures,
                &[
                    (b"OB", 1, 1, id(b"OBselected", 0, 2)),
                    (b"DATA", 2, 2, 3u64.to_le_bytes().to_vec()),
                    (b"ID", 3, 0, id(b"OBsource", 4, 0)),
                    (
                        b"LI",
                        4,
                        3,
                        library(b"LItrash.blend.001", b"//props/trash.blend"),
                    ),
                    (b"LI", 5, 3, library(b"LIunused", b"//unused.blend")),
                ],
            ),
        )
        .unwrap();
        std::fs::write(
            &direct,
            saved(&structures, &[(b"OB", 1, 1, id(b"OBsource", 0, 0))]),
        )
        .unwrap();
        // The source is now selected through the exact override reference ID;
        // its graph is loaded by Blender, not just its physical declaration.
        std::fs::write(
            &indirect,
            saved(&structures, &[(b"OB", 1, 1, id(b"OBsource", 0, 0))]),
        )
        .unwrap();
        std::fs::write(work.0.join("unused.blend"), b"not selected either").unwrap();

        let (listing, opened) = listing_with_file_paths(&root, TraceOptions::default()).unwrap();
        assert!(listing.complete, "{:?}", listing.errors);
        assert!(listing.errors.is_empty());
        assert_eq!(
            opened,
            vec![
                root.clone(),
                child.clone(),
                indirect.clone(),
                direct.clone()
            ]
        );
        assert_eq!(listing.usages.len(), 3);
        let usage = listing
            .usages
            .iter()
            .find(|usage| usage.asset_path == b"//props/trash.blend")
            .unwrap();
        assert_eq!(usage.owner_file, raw(&child));
        assert_eq!(usage.library_chain, vec![raw(&root), raw(&child)]);
        assert_eq!(usage.block_name, b"LItrash.blend.001");
        assert_eq!(resolve(&child, &usage.asset_path), indirect);
        assert!(listing.usages.iter().any(|usage| {
            usage.owner_file == raw(&root) && usage.asset_path == b"//env/trash.blend"
        }));
        assert!(
            listing
                .usages
                .iter()
                .all(|usage| usage.asset_path != b"//unused.blend")
        );

        let traced = trace(&root, TraceOptions::default()).unwrap();
        assert!(traced.complete, "{:?}", traced.errors);
        assert_eq!(traced.usages.len(), 3);
        assert!(
            traced
                .usages
                .iter()
                .any(|usage| usage.asset_path == b"//props/trash.blend")
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
    fn saved_pattern_member_names_preserve_bytes_and_padding() {
        assert_eq!(
            sequence_member(b"//\xff/shot.0001.png", 23),
            Some(b"//\xff/shot.0023.png".to_vec())
        );
        assert_eq!(sequence_member(b"//a12/still.png", 2), None);
        assert_eq!(
            tile_member(b"//\xff/tile.<UDIM>.exr", 1012),
            Some(b"//\xff/tile.1012.exr".to_vec())
        );
        assert_eq!(
            tile_member(b"//tile.<UVTILE>.exr", 1012),
            Some(b"//tile.u2_v2.exr".to_vec())
        );
    }
    #[test]
    fn tile_detection_is_filename_only_case_sensitive_and_byte_safe() {
        for (p, expected) in [
            (b"//tiles/<UDIM>/still.png".as_slice(), false),
            (b"//tiles/<UDIM>/tile.<UDIM>.exr".as_slice(), true),
            (b"C:\\tiles\\<UDIM>\\still.png".as_slice(), false),
            (b"tile.<udim>.exr".as_slice(), false),
            (b"\xff/tile.<UDIM>.exr".as_slice(), true),
            (b"tile.<UVTILE>.exr".as_slice(), true),
            (b"//tiles/<UVTILE>/still.png".as_slice(), false),
            (b"tile.<uvtile>.exr".as_slice(), false),
        ] {
            assert_eq!(filename_tile(p), expected, "{p:?}");
        }
    }
    /// Blender `BKE_image_ensure_tile_token_filename_only` order
    /// (image.cc:3773): UDIM regex first, then UVTILE, filename-only.
    /// Vectors cross-checked against Python `re` with Blender's patterns.
    #[test]
    fn concrete_tile_basenames_auto_tokenize_like_blender() {
        for (name, expected) in [
            ("monster_x_1005.png", Some("monster_x_<UDIM>.png")),
            ("monster_albedo.1001.png", Some("monster_albedo.<UDIM>.png")),
            ("pub_concrete2_1005.png", Some("pub_concrete2_<UDIM>.png")),
            (
                "monster-basecolor.1021.png",
                Some("monster-basecolor.<UDIM>.png"),
            ),
            ("tile.1002.png", Some("tile.<UDIM>.png")),
            ("tile.2001.png", Some("tile.<UDIM>.png")),
            ("tex.1001.2001.png", Some("tex.1001.<UDIM>.png")),
            ("a_u1_v1_b_u2_v2.png", Some("a_u1_v1_b_<UVTILE>.png")),
            ("still_1005.png", Some("still_<UDIM>.png")),
            ("_1001.png", Some("_<UDIM>.png")),
            ("x.1001.", Some("x.<UDIM>.")),
            ("a-1001-b.exr", Some("a-<UDIM>-b.exr")),
            ("tile.u1_v2.png", Some("tile.<UVTILE>.png")),
            ("tile.u12_v345.png", Some("tile.<UVTILE>.png")),
            ("tile.u2_v1.png", Some("tile.<UVTILE>.png")),
            ("u1_v2.png", Some("<UVTILE>.png")),
            ("monsteru1_v2.png", Some("monster<UVTILE>.png")),
            ("tile.<UDIM>.png", None),
            ("tile.<UVTILE>.png", None),
            ("still.png", None),
            ("shot.0001.png", None),
            ("tile.3001.png", None),
            ("tile.0999.png", None),
            ("a_1001", None),
            ("1001.png", None),
            ("u1_v2345x.png", None),
            ("tile.u123_v4.png", None),
            ("tile.u1_v2345.png", None),
            ("a_u1_v2", None),
            ("a1001b.png", None),
        ] {
            assert_eq!(
                auto_tile_token(name.as_bytes()),
                expected.map(|s| s.as_bytes().to_vec()),
                "{name}"
            );
        }
    }
    #[test]
    fn auto_tokenize_never_reads_the_directory_component() {
        assert_eq!(
            auto_tile_token(b"still_1005.png"),
            Some(b"still_<UDIM>.png".to_vec())
        );
        assert_eq!(auto_tile_token(b"still.png"), None);
    }
    #[test]
    fn census_and_selected_closure_preserve_physical_order_and_raw_names() {
        let bytes = images(&[
            (b"IM\xffsecond", b"//b.png", 1, 0),
            (b"IMfirst", b"//a.png", 1, 0),
        ]);
        let mut census = context(bytes.clone());
        census.file_trace(0, None, false).unwrap();
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
            .file_trace(
                0,
                Some(vec![b"IMfirst".to_vec(), b"IMfirst".to_vec()]),
                false,
            )
            .unwrap();
        selected
            .file_trace(0, Some(vec![b"IMfirst".to_vec()]), false)
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
        c.file_trace(0, None, false).unwrap();
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
        c.file_trace(0, None, false).unwrap();
        assert!(c.report.complete);
        assert!(c.report.errors.is_empty());
        assert_eq!(c.report.usages.len(), 1);
        assert_eq!(c.report.usages[0].asset_path, b"//visible.png");
        image[72..80].copy_from_slice(&u64::MAX.to_le_bytes());
        let mut c = context(saved(
            &structures,
            &[(b"IM".as_slice(), 4, 4, image.clone())],
        ));
        c.file_trace(0, None, false).unwrap();
        assert!(c.report.complete);
        assert!(c.report.errors.is_empty());
        assert_eq!(c.report.usages.len(), 1);
        // A genuine file dependency's library pointer remains checked. This
        // is a root-boundary change, never a general dangling-pointer ignore.
        image[64..72].copy_from_slice(&u64::MAX.to_le_bytes());
        let mut c = context(saved(&structures, &[(b"IM".as_slice(), 4, 4, image)]));
        c.file_trace(0, None, false).unwrap();
        assert!(!c.report.complete);
        assert!(c.report.errors.iter().any(|error| matches!(
            error.kind,
            FailureKind::DanglingPointer
        ) && error.field_path
            == vec![b"id".to_vec(), b"lib".to_vec()]));
    }
    #[test]
    fn known_non_file_families_do_not_warn_but_unknown_codes_still_do() {
        for (code, ty) in [
            ("CA", "Camera"),
            ("AC", "bAction"),
            ("AR", "bArmature"),
            ("CU", "Curve"),
            ("CV", "Curves"),
            ("GD", "bGPdata"),
            ("GP", "GreasePencil"),
            ("KE", "Key"),
            ("LP", "LightProbe"),
            ("LT", "Lattice"),
            ("MA", "Material"),
            ("MB", "MetaBall"),
            ("MS", "Mask"),
            ("PA", "ParticleSettings"),
            ("PC", "PaintCurve"),
            ("PL", "Palette"),
            ("PT", "PointCloud"),
            ("SK", "Speaker"),
            ("TE", "Tex"),
            ("WS", "WorkSpace"),
            ("ZZ", "Unknown"),
        ] {
            let structures = [
                ("ID", 72, vec![("char", "name[64]"), ("void", "*lib")]),
                (ty, 72, vec![("ID", "id")]),
            ];
            let mut data = fixed(format!("{code}Evidence").as_bytes(), 64);
            data.extend(0u64.to_le_bytes());
            let mut c = context(saved(&structures, &[(code.as_bytes(), 1, 1, data)]));
            c.file_trace(0, None, false).unwrap();
            assert_eq!(
                c.report
                    .warnings
                    .iter()
                    .any(|w| matches!(w, TraceWarning::UnknownAssetCode { .. })),
                code == "ZZ",
                "{code}: {:?}",
                c.report.warnings
            );
            assert!(c.report.usages.is_empty());
        }
    }
    #[test]
    fn camera_modes_keep_external_declarations_internal_text_and_unknown_facts() {
        for (label, mode, path, camera_type, malformed) in [
            ("empty", 0i32, b"".as_slice(), 0u8, false),
            ("external", 1, b"//camera.osl", 3, false),
            ("inactive", 1, b"//camera.osl", 0, false),
            ("internal", 0, b"//history.osl", 3, false),
            ("unknown", 9, b"//camera.osl", 3, false),
            ("malformed", 1, b"//camera.osl", 3, true),
        ] {
            let structures = [
                ("ID", 72, vec![("char", "name[64]"), ("void", "*lib")]),
                (
                    "Camera",
                    344,
                    vec![
                        ("ID", "id"),
                        ("char", "type"),
                        ("char", "_pad[3]"),
                        (if malformed { "float" } else { "int" }, "custom_mode"),
                        ("char", "custom_filepath[256]"),
                        ("Text", "*custom_shader"),
                    ],
                ),
                (
                    "Text",
                    84,
                    vec![("ID", "id"), ("char", "*name"), ("int", "flags")],
                ),
            ];
            let mut camera = fixed(b"CAEvidence", 64);
            camera.extend(0u64.to_le_bytes());
            camera.extend([camera_type, 0, 0, 0]);
            camera.extend(mode.to_le_bytes());
            camera.extend(fixed(path, 256));
            camera.extend((if label == "internal" { 2u64 } else { 0u64 }).to_le_bytes());
            let mut records = vec![(b"CA".as_slice(), 1, 1, camera)];
            if label == "internal" {
                let mut text = fixed(b"TXCameraShader", 64);
                text.extend(0u64.to_le_bytes());
                text.extend(3u64.to_le_bytes());
                text.extend(0i32.to_le_bytes());
                records.push((b"TX", 2, 2, text));
                records.push((b"DATA", 3, 0, b"//internal-source.osl\0".to_vec()));
            }
            let mut c = context(saved(&structures, &records));
            c.file_trace(0, None, false).unwrap();
            assert!(
                !c.report
                    .warnings
                    .iter()
                    .any(|w| matches!(w, TraceWarning::UnknownAssetCode { .. })),
                "{label}"
            );
            assert_eq!(
                c.report.errors.is_empty(),
                !matches!(label, "unknown" | "malformed"),
                "{label}: {:?}",
                c.report.errors
            );
            if label == "empty" {
                assert!(c.report.usages.is_empty());
                continue;
            }
            assert_eq!(c.report.usages.len(), 1, "{label}");
            let usage = c.report.usages[0].json();
            if label == "internal" {
                let mut selected = context(saved(&structures, &records));
                selected
                    .file_trace(0, Some(vec![b"CAEvidence".to_vec()]), false)
                    .unwrap();
                assert_eq!(
                    selected.report.usages.len(),
                    1,
                    "selected Camera must follow its Text: {:?}",
                    selected.report.errors
                );
                assert_eq!(
                    selected.report.usages[0].asset_path,
                    b"//internal-source.osl"
                );
                assert!(
                    selected.report.errors.is_empty(),
                    "{:?}",
                    selected.report.errors
                );
                assert_eq!(usage["asset_path"], "//internal-source.osl");
                assert!(
                    c.report
                        .warnings
                        .iter()
                        .any(|w| w.json()["kind"] == "embedded_camera_filepath")
                );
            } else {
                assert_eq!(usage["asset_path"], "//camera.osl");
                assert_eq!(
                    usage["field_provenance"]["full"]["field_path"],
                    json!(["custom_filepath"])
                );
                if matches!(label, "external" | "inactive") {
                    assert_eq!(usage["camera_type"], camera_type);
                    assert_eq!(usage["active"], camera_type == 3);
                }
            }
        }
    }
    #[test]
    fn camera_code_does_not_admit_another_asset_struct() {
        let mut bytes = images(&[(b"IMmislabelled", b"//not-a-camera.osl", 1, 0)]);
        bytes[12..16].copy_from_slice(b"CA\0\0");
        let mut c = context(bytes);
        c.file_trace(0, None, false).unwrap();
        assert!(
            c.report.usages.is_empty(),
            "CA must not dispatch an Image filepath"
        );
        assert!(
            c.report
                .warnings
                .iter()
                .any(|w| matches!(w, TraceWarning::UnknownAssetCode { code, .. } if code == b"CA"))
        );
    }
    #[test]
    fn unsupported_identifier_codes_do_not_turn_typed_strings_into_resources() {
        let mut bytes = images(&[
            (b"IMunknown", b"//not-a-resource.png", 1, 0),
            (b"IMknown", b"//known.png", 1, 0),
        ]);
        bytes[12..16].copy_from_slice(b"ZZ\0\0");
        let mut c = context(bytes);
        c.file_trace(0, None, false).unwrap();
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
            c.file_trace(0, None, false).unwrap();
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
        c.file_trace(0, None, false).unwrap();
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
        c.file_trace(0, Some(vec![b"OBselected".to_vec()]), false)
            .unwrap();
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
        c.file_trace(0, Some(vec![b"GRcycle".to_vec()]), false)
            .unwrap();
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
        c.file_trace(0, None, false).unwrap();
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
        crate::value::with_text(
            json!({"name_hex":hex(&self.name),"full_name_hex":hex(&self.full_name),"offset":self.offset,"size":self.size,"type_hex":hex(&self.dna_type),"pointer":self.pointer}),
        )
    }
    pub fn provenance_json(&self) -> Json {
        crate::value::with_text(
            json!({"owner_file_hex":hex(&self.owner_file),"block_address":self.block_address.to_string(),"element":self.element,"field_path_hex":self.path.iter().map(|p|hex(p)).collect::<Vec<_>>()}),
        )
    }
}
impl Usage {
    pub(crate) fn expected_paths(&self) -> Vec<PathBuf> {
        let owner = path(&self.owner_file);
        self.expected_members
            .iter()
            .map(|member| resolve(&owner, member))
            .collect()
    }
    pub(crate) fn missing_members(&self) -> Vec<Usage> {
        let owner = path(&self.owner_file);
        let mut template = self.clone();
        template.is_sequence = false;
        template.is_udim = false;
        template.expected_members.clear();
        template.pattern_unknown = None;
        template.pattern_image = None;
        self.expected_members
            .iter()
            .filter(|member| !resolve(&owner, member).is_file())
            .map(|member| {
                let mut usage = template.clone();
                usage.asset_path = member.clone();
                usage
            })
            .collect()
    }
    pub(crate) fn available_members(&self) -> Result<Vec<PathBuf>> {
        // Concrete-basename tiled Images resolve only their saved tile
        // numbers through Blender's auto-tokenized pattern (image.cc:3597
        // `BKE_image_get_tile_info` expands saved tiles, never a directory
        // glob): a resolved token already covers discovery via `expand`.
        let mut members = if self.is_udim && !filename_tile(&self.asset_path) {
            Vec::new()
        } else {
            crate::pack::expand(&resolve_usage(self), self.is_sequence).or_else(|e| {
                if e.kind == "missing_asset" && !self.expected_members.is_empty() {
                    Ok(Vec::new())
                } else {
                    Err(e)
                }
            })?
        };
        members.extend(self.expected_paths().into_iter().filter(|p| p.is_file()));
        members.sort();
        members.dedup();
        Ok(members)
    }
    pub(crate) fn exists(&self) -> bool {
        self.pattern_unknown.is_none()
            && self.expected_paths().iter().all(|p| p.is_file())
            && (resolve_usage(self).is_dir()
                || self
                    .available_members()
                    .is_ok_and(|members| !members.is_empty()))
    }
    pub fn json(&self) -> Json {
        let resolved = resolve_usage(self);
        let exists = self.exists();
        let mut value = json!({"owner_file":std::str::from_utf8(&self.owner_file).ok(),"owner_file_hex":hex(&self.owner_file),"library_chain_hex":self.library_chain.iter().map(|p|hex(p)).collect::<Vec<_>>(),"block_code_hex":hex(&self.block_code),"block_address":self.block_address.to_string(),"block_offset":self.block_offset,"block_type":std::str::from_utf8(&self.block_type).ok(),"block_name_hex":hex(&self.block_name),"asset_path_hex":hex(&self.asset_path),"is_sequence":self.is_sequence,"is_udim":self.is_udim,"optional":self.optional,"path_full_field":self.path_full_field.as_ref().map(FieldReference::json),"path_dir_field":self.path_dir_field.as_ref().map(FieldReference::json),"path_base_field":self.path_base_field.as_ref().map(FieldReference::json),"field_provenance":{"full":self.path_full_field.as_ref().map(FieldReference::provenance_json),"dir":self.path_dir_field.as_ref().map(FieldReference::provenance_json),"base":self.path_base_field.as_ref().map(FieldReference::provenance_json)}});
        value["resolved_path_hex"] = json!(hex(resolved.as_os_str().as_encoded_bytes()));
        value["exists"] = json!(exists);
        if !self.expected_members.is_empty() || self.pattern_unknown.is_some() {
            value["expected_members_hex"] = json!(
                self.expected_paths()
                    .iter()
                    .map(|p| hex(p.as_os_str().as_bytes()))
                    .collect::<Vec<_>>()
            );
            value["expected_members"] = json!(
                self.expected_paths()
                    .iter()
                    .map(|p| p.to_str())
                    .collect::<Vec<_>>()
            );
            value["missing_members_hex"] = json!(
                self.missing_members()
                    .iter()
                    .map(|u| hex(resolve_usage(u).as_os_str().as_bytes()))
                    .collect::<Vec<_>>()
            );
            value["missing_members"] = json!(
                self.missing_members()
                    .iter()
                    .map(|u| resolve_usage(u).to_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            );
            value["pattern_status"] = json!(if self.pattern_unknown.is_some() {
                "unknown"
            } else if exists {
                "available"
            } else {
                "missing"
            });
            value["pattern_unknown"] = json!(self.pattern_unknown);
        }
        if let Some(camera_type) = self.camera_type {
            value["camera_type"] = json!(camera_type);
            value["active"] = json!(self.active);
        }
        if let Some(reason) = &self.pattern_unknown {
            value["problem"] =
                json!({"kind":"pattern_members_unknown","status":"unknown","reason":reason});
        } else if !exists {
            value["problem"] = json!({"kind":if self.block_code==b"LI" {"missing_library"} else {"missing_asset"},"status":"missing","reason":"declared path has no available file/member"});
        }
        crate::value::with_text(value)
    }
}
impl TraceFailure {
    pub(crate) fn kind_str(&self) -> &'static str {
        match self.kind {
            FailureKind::Reader(k) => k,
            FailureKind::LibraryReader(_) => "library_read",
            FailureKind::DanglingPointer => "dangling_pointer",
            FailureKind::AmbiguousPointer => "ambiguous_pointer",
            FailureKind::MissingLibrary => "missing_library",
            FailureKind::MissingId => "missing_id",
            FailureKind::Cycle => "cycle",
            FailureKind::InvalidGraph => "invalid_graph",
        }
    }
    pub(crate) fn is_external_missing(&self) -> bool {
        matches!(self.kind, FailureKind::MissingLibrary)
            || (matches!(self.kind, FailureKind::MissingId) && self.missing_id.is_some())
    }
    pub fn json(&self) -> Json {
        let mut value = json!({"kind":match &self.kind{FailureKind::Reader(k)=>*k,FailureKind::LibraryReader(_)=>"library_read",FailureKind::DanglingPointer=>"dangling_pointer",FailureKind::AmbiguousPointer=>"ambiguous_pointer",FailureKind::MissingLibrary=>"missing_library",FailureKind::MissingId=>"missing_id",FailureKind::Cycle=>"cycle",FailureKind::InvalidGraph=>"invalid_graph"},"owner_file_hex":hex(&self.owner_file),"block_address":self.block_address.map(|n|n.to_string()),"field_path_hex":self.field_path.iter().map(|p|hex(p)).collect::<Vec<_>>(),"library_path_hex":self.library_path.as_ref().map(|p|hex(p)),"message":self.message});
        value["diagnostic_ref"] = crate::failure_receipt::reference_json(self.diagnostic.as_ref());
        if !self.is_external_missing() {
            value["status"] = json!("unknown");
            value["site"] = json!(self.site);
        }
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
        crate::value::with_text(value)
    }
}
impl TraceReport {
    pub fn json(&self) -> Json {
        let mut value = json!({"schema":1,"complete":self.complete,"usages":self.usages.iter().map(Usage::json).collect::<Vec<_>>(),"errors":self.errors.iter().map(TraceFailure::json).collect::<Vec<_>>(),"warnings":self.warnings.iter().map(TraceWarning::json).collect::<Vec<_>>()});
        value["missing_link_counts"] = json!({"missing_id":self.errors.iter().filter(|e|matches!(e.kind, FailureKind::MissingId)).count(),"missing_library":self.errors.iter().filter(|e|matches!(e.kind, FailureKind::MissingLibrary)).count()});
        value["missing_path_count"] = json!(
            value["usages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|u| u["exists"] == false)
                .count()
        );
        value["referenced_paths_exist"] = json!(value["missing_path_count"] == 0);
        let mut counts = BTreeMap::<String, usize>::new();
        for error in &value["errors"].as_array().unwrap()[..] {
            *counts
                .entry(error["kind"].as_str().unwrap().to_owned())
                .or_default() += 1;
        }
        for usage in &value["usages"].as_array().unwrap()[..] {
            if let Some(kind) = usage["problem"]["kind"].as_str() {
                if kind == "missing_library"
                    && self.errors.iter().any(|e| {
                        matches!(e.kind, FailureKind::MissingLibrary)
                            && e.library_path
                                .as_deref()
                                .is_some_and(|p| hex(p) == usage["resolved_path_hex"])
                    })
                {
                    continue;
                }
                *counts.entry(kind.to_owned()).or_default() += 1;
            }
        }
        value["problem_counts"] = json!(counts);
        crate::value::with_text(value)
    }
}
