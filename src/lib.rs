//! Oxblend serialized reader and explicit new-output path editor.
//! No Blender evaluation, mmap, process caches, or incidental scene decoding.
#[cfg(feature = "native")]
mod arrays;
/// Unstable/internal Rust API: may change in minor releases. CLI/JSON contracts are separate.
#[cfg(feature = "native")]
pub mod bat_cli;
/// Verified-on-reuse working bodies for explicit bounded retention.
#[cfg(feature = "native")]
mod body_cache;
mod checked;
/// Unstable local diagnostic adapter; flattrace owns the format.
#[cfg(feature = "native")]
pub mod diagnostics;
#[cfg(feature = "native")]
pub mod diff;
/// Unstable/internal operations: may change in minor releases; CLI/JSON contracts are separate.
/// Exceptions stable in 1.x: EditOptions, EditReport, EditRange, EditFailure and Durability,
/// because these types are reachable from the stable packaging interface.
#[cfg(feature = "native")]
pub mod edit;
#[cfg(feature = "native")]
mod edit_container;
#[cfg(feature = "native")]
mod edit_decode;
#[cfg(feature = "native")]
mod edit_output;
#[cfg(feature = "native")]
mod edit_xxh;
#[cfg(feature = "native")]
pub mod error_help;
#[cfg(all(test, feature = "native"))]
#[path = "../newtests/exact_blocks.rs"]
mod exact_blocks_tests;
#[cfg(feature = "native")]
pub mod exr_header;
#[cfg(feature = "native")]
pub mod exr_pixels;
#[cfg(feature = "native")]
pub mod exr_scan;
/// Same-schema selected-ID partial writer.
#[cfg(feature = "native")]
pub mod extract;
#[cfg(feature = "native")]
pub mod facts;
#[cfg(feature = "native")]
pub mod failure_receipt;
#[cfg(feature = "native")]
mod fetch;
/// Unstable/internal Rust API: may change in minor releases. CLI/JSON contracts are separate.
#[cfg(feature = "native")]
pub mod file_ops;
#[cfg(test)]
#[cfg(feature = "native")]
mod file_ops_tests;
#[cfg(feature = "native")]
mod id_codes;
#[cfg(feature = "native")]
mod input;
#[cfg(feature = "native")]
mod kernel_copy;
#[cfg(feature = "native")]
mod library_compat;
/// Multi-root reporting, derived from the existing single-root traces.
#[cfg(feature = "native")]
pub mod multi_root;
/// Unstable expected-graph structural compositor editing.
#[cfg(feature = "native")]
pub mod node_tree_edit;
#[cfg(feature = "native")]
pub mod overrides;
#[cfg(test)]
#[cfg(feature = "native")]
mod owner_compat_tests;
#[cfg(feature = "native")]
pub mod pack;
#[cfg(feature = "native")]
mod pack_zip;
/// Native-byte path list convention shared by CLI commands.
#[cfg(feature = "native")]
pub mod path_list;
#[cfg(feature = "native")]
mod physical;
/// One-reader saved-record query and authored concept discovery.
#[cfg(feature = "native")]
pub mod query;
/// Unstable/internal single-open read batch: ordered inspect/get/pointer/payload plan.
#[cfg(feature = "native")]
pub mod read_batch;
/// Unstable/internal serialized report projections shared by CLI and bindings.
#[cfg(feature = "native")]
pub mod read_reports;
#[cfg(feature = "native")]
pub mod report;
#[cfg(feature = "native")]
mod report_input;
#[cfg(feature = "native")]
mod report_privacy;
/// Unstable/internal shared strict expected-old rewrite specification parser.
#[cfg(feature = "native")]
pub mod rewrite_spec;
pub mod schema;
#[cfg(feature = "native")]
mod seek_input;
#[cfg(feature = "native")]
pub mod session;
#[cfg(feature = "native")]
mod session_query;
#[cfg(feature = "native")]
mod stream_decode;
#[cfg(test)]
#[cfg(feature = "native")]
mod stream_decode_tests;
#[cfg(feature = "native")]
pub mod strict;
#[cfg(feature = "native")]
pub mod summary;
#[cfg(test)]
#[path = "../tests/support/scratch.rs"]
#[cfg(feature = "native")]
mod test_scratch;
#[cfg(feature = "native")]
pub mod trace;
#[cfg(feature = "native")]
mod trace_input;
#[cfg(test)]
#[cfg(feature = "native")]
mod trace_input_tests;
#[cfg(feature = "native")]
pub mod tracing;
/// Unstable/internal Rust API: may change in minor releases. CLI/JSON contracts are separate.
#[cfg(feature = "native")]
pub mod typed_edit;
/// Unstable/internal Rust API: may change in minor releases. CLI/JSON contracts are separate.
#[cfg(feature = "native")]
pub mod typed_spec;
#[cfg(feature = "native")]
pub mod value;
use checked::bytes;
#[cfg(feature = "native")]
use checked::{Cursor, number};
#[cfg(all(feature = "native", feature = "wasm-read-only"))]
compile_error!("native and wasm-read-only are mutually exclusive; use --no-default-features");
#[cfg(all(not(feature = "native"), not(feature = "wasm-read-only")))]
compile_error!("select native or wasm-read-only");
#[cfg(feature = "wasm-read-only")]
pub mod wasm_read;
#[cfg(feature = "native")]
use input::Backing;
#[cfg(feature = "native")]
use schema::Schema;
#[cfg(feature = "native")]
pub use session::{Diagnostic, Operation, Session};
#[cfg(feature = "native")]
use std::borrow::Cow;
#[cfg(feature = "native")]
use std::collections::HashMap;
#[cfg(feature = "native")]
use std::path::Path;
#[cfg(feature = "wasm-read-only")]
pub use wasm_read::BlendFile;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Error {
    pub kind: &'static str,
    pub message: String,
    #[cfg(feature = "native")]
    pub(crate) diagnostic: std::sync::OnceLock<std::sync::Arc<failure_receipt::DiagnosticRef>>,
    // First producer message, before optional Session-only presentation.
    // Shared canonical contexts (AtomicOutput create/publish, fetch, pack,
    // copy) are part of the CLI canonical message and are never recorded
    // here; only Session-only presentation (snapshot/index) records a
    // strippable base. Typed provenance: never recover it by parsing
    // formatted strings. The native binding restores the Session base at
    // its language boundary so Python sees exactly the CLI canonical
    // kind/message; Session presentation stays available through the typed
    // diagnostic/source channels, never the message.
    #[cfg(feature = "native")]
    pub(crate) context_message: Option<Box<str>>,
    // Owned typed source-diagnostic snapshot captured at instrumented owner
    // checks, populated whether or not a CLI invocation ledger exists. Cloned
    // errors share it; library callers keep their evidence without a receipt.
    #[cfg(feature = "native")]
    pub(crate) source_snapshot:
        std::sync::OnceLock<std::sync::Arc<std::sync::Mutex<failure_receipt::SourceSnapshot>>>,
}
impl Error {
    pub fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            #[cfg(feature = "native")]
            diagnostic: std::sync::OnceLock::new(),
            #[cfg(feature = "native")]
            context_message: None,
            #[cfg(feature = "native")]
            source_snapshot: std::sync::OnceLock::new(),
        }
    }
    /// Restore the typed Session-only base recorded by
    /// [`crate::edit_output::session_context`], dropping Session presentation
    /// (`fetch/read snapshot`, `snapshot/index` with no shared prefix
    /// outside) from the message while preserving every shared canonical
    /// context (AtomicOutput create/publish, pack) exactly as the CLI emits
    /// it. A shared operation prefix applied outside a Session layer drops
    /// the recorded base at [`crate::edit_output::context`], so it survives
    /// here as part of the canonical message. Kind is never touched by
    /// presentation, so this recovers exactly the CLI canonical error with
    /// no string parsing. Public so the native binding can canonicalize at
    /// its language boundary; the CLI read/trace paths already apply this
    /// internally for Session captures.
    #[cfg(feature = "native")]
    pub fn without_context(mut self) -> Self {
        if let Some(message) = self.context_message.take() {
            self.message = message.into_string();
        }
        self
    }
    #[cfg(feature = "native")]
    pub fn diagnostic_ref(&self) -> serde_json::Value {
        failure_receipt::reference_json(self.diagnostic.get())
    }
    /// Owned typed source-diagnostic snapshot for this error, captured at
    /// instrumented owner checks (`record_source_failure`) regardless of any
    /// CLI invocation ledger. First producer wins; later differing checks
    /// append typed propagations, mirroring the CLI ledger exactly. Cloned
    /// errors share one snapshot. `None` for errors that never reached an
    /// instrumented owner check.
    #[cfg(feature = "native")]
    pub fn source_snapshot(&self) -> Option<failure_receipt::SourceSnapshot> {
        self.source_snapshot
            .get()
            .and_then(|snapshot| snapshot.lock().ok().map(|guard| guard.clone()))
    }
    #[cfg(feature = "native")]
    pub(crate) fn reclassify(mut self, kind: &'static str) -> Self {
        self.kind = kind;
        self
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
#[non_exhaustive]
pub struct Header {
    pub version: u32,
    pub pointer_size: usize,
    pub little_endian: bool,
    pub file_format: u32,
    pub size: usize,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct Block {
    pub code: [u8; 4],
    pub old_address: u64,
    pub sdna_index: usize,
    pub count: usize,
    pub data_offset: usize,
    pub size: usize,
}
#[cfg(all(test, feature = "native"))]
thread_local! {
    pub(crate) static SCAN_RECORD_READS: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
#[derive(Debug)]
#[non_exhaustive]
#[cfg(feature = "native")]
pub struct BlendFile {
    header: Header,
    blocks: Box<[Block]>,
    schema: Schema,
    data: Backing,
    // Provenance captured at the real open/capture boundary, never reopened by export.
    source_path: Option<std::path::PathBuf>,
    // Raw, context-free API only; trace/overrides use the native owner index.
    by_address: std::cell::OnceCell<HashMap<u64, Option<usize>>>,
    // Derived from this parsed file, shared by all per-pack root contexts.
    pub(crate) name_index: std::cell::OnceCell<crate::trace::NameIndex>,
    owner_index: std::cell::OnceCell<crate::file_ops::index::Index>,
}
fn decimal(data: &[u8]) -> Result<u32> {
    if data.is_empty() || !data.iter().all(u8::is_ascii_digit) {
        return Err(Error::new("invalid_header", "decimal header field"));
    }
    std::str::from_utf8(data)
        .map_err(|_| Error::new("invalid_header", "header text"))?
        .parse()
        .map_err(|_| Error::new("invalid_header", "decimal header value"))
}
fn header(data: &[u8]) -> Result<Header> {
    if bytes(data, 0, 7)? != b"BLENDER" {
        return Err(Error::new("invalid_header", "BLENDER magic"));
    }
    let indicator = bytes(data, 7, 1)?[0];
    if indicator == b'_' || indicator == b'-' {
        let endian = bytes(data, 8, 1)?[0];
        if endian != b'v' && endian != b'V' {
            return Err(Error::new("invalid_header", "endian marker"));
        }
        Ok(Header {
            version: decimal(bytes(data, 9, 3)?)?,
            pointer_size: if indicator == b'_' { 4 } else { 8 },
            little_endian: endian == b'v',
            file_format: 0,
            size: 12,
        })
    } else {
        bytes(data, 0, 17)?;
        if bytes(data, 7, 6)? != b"17-01v" {
            return Err(Error::new("unsupported_format", "expected BLENDER17-01v"));
        }
        Ok(Header {
            version: decimal(bytes(data, 13, 4)?)?,
            pointer_size: 8,
            little_endian: true,
            file_format: 1,
            size: 17,
        })
    }
}
fn count(raw: u64, width: usize) -> Result<usize> {
    let max = if width == 4 {
        i32::MAX as u64
    } else {
        i64::MAX as u64
    };
    if raw > max {
        return Err(Error::new(
            "invalid_block",
            "negative signed length/count/index",
        ));
    }
    usize::try_from(raw).map_err(|_| Error::new("overflow", "block length/count"))
}
#[cfg(feature = "native")]
impl BlendFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        crate::diagnostics::opened(path, || {
            Self::from_backing(Backing::open(path)?).map(|reader| reader.with_source_path(path))
        })
    }
    /// Blocks inspection has an invocation-scoped streaming/frame reader;
    /// ordinary trace/list and kept-closure admission policies stay unchanged.
    /// Its streaming fallback stays silent by construction.
    pub(crate) fn open_blocks(
        path: &Path,
        diag: &crate::session::DiagSink,
        temp_root: &Path,
    ) -> Result<Self> {
        crate::diagnostics::opened(path, || {
            let reader = Self::from_backing(Backing::Trace(Box::new(
                crate::trace_input::Input::open_blocks(path, diag, temp_root)?,
            )))?
            .with_source_path(path);
            reader.check_source()?;
            Ok(reader)
        })
    }
    pub(crate) fn open_trace(
        path: &Path,
        diag: &crate::session::DiagSink,
        temp_root: &Path,
    ) -> Result<Self> {
        crate::diagnostics::opened(path, || {
            let reader = Self::from_backing(Backing::Trace(Box::new(
                crate::trace_input::Input::open_native(path, diag, temp_root)?,
            )))?
            .with_source_path(path);
            reader.check_source()?;
            Ok(reader)
        })
    }
    pub(crate) fn open_trace_low_memory(
        path: &Path,
        diag: &crate::session::DiagSink,
        temp_root: &Path,
    ) -> Result<Self> {
        crate::diagnostics::opened(path, || {
            let input = crate::trace_input::Input::open(path, diag, temp_root)?;
            let prefix = input.is_prefix();
            let signature = input.signature();
            let reader = match Self::from_backing(Backing::Trace(Box::new(input))) {
                Err(e) if prefix => {
                    let spill = crate::trace_input::Input::open_spill(
                        path,
                        e.to_string(),
                        diag,
                        temp_root,
                    )?;
                    if spill.signature() != signature {
                        return Err(Error::new(
                            "input_changed",
                            "trace source changed during seek fallback",
                        ));
                    }
                    Self::from_backing(Backing::Trace(Box::new(spill)))?
                }
                other => other?,
            };
            reader.data.check_source()?;
            Ok(reader.with_source_path(path))
        })
    }
    pub(crate) fn with_source_path(mut self, path: &Path) -> Self {
        self.source_path = Some(path.to_path_buf());
        self
    }
    pub(crate) fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let span = crate::diagnostics::Span::new(
            "memory.capture",
            if crate::diagnostics::enabled() {
                serde_json::json!({"bytes":data.len(),"identity_kind":"caller_owned_decoded_bytes","io":false})
            } else {
                serde_json::Value::Null
            },
        );
        span.result(Self::from_backing(Backing::Bytes(data)))
    }
    pub(crate) fn from_backing(data: Backing) -> Result<Self> {
        let mut span =
            crate::diagnostics::Span::new("index", serde_json::json!({"decoded_bytes":data.len()}));
        let result = (|| {
            let header = header(data.range(0, data.len().min(17))?.as_ref())?;
            let (blocks, dna) = data.scan(|reader| {
                let mut position = header.size;
                let mut blocks = Vec::new();
                let mut dna = None;
                // Fixed layout widths come from the supported BHead formats, not limits.
                let width = if header.file_format == 1 {
                    32
                } else {
                    16 + header.pointer_size
                };
                input::seek(reader, position)?;
                loop {
                    let mut encoded = [0u8; 32];
                    let next = checked::add(position, width)?;
                    if next > data.len() {
                        return Err(Error::new("truncated", "block header"));
                    }
                    input::read_exact(reader, &mut encoded[..width])?;
                    let mut cursor = Cursor {
                        data: &encoded[..width],
                        offset: 0,
                        little: header.little_endian,
                    };
                    let mut code = [0u8; 4];
                    code.copy_from_slice(cursor.take(4)?);
                    let (size, old_address, sdna_index, nr) = if header.file_format == 1 {
                        let sdna = count(cursor.uint(4)?, 4)?;
                        let old = cursor.uint(8)?;
                        let size = count(cursor.uint(8)?, 8)?;
                        let nr = count(cursor.uint(8)?, 8)?;
                        (size, old, sdna, nr)
                    } else {
                        let size = count(cursor.uint(4)?, 4)?;
                        let old = cursor.uint(header.pointer_size)?;
                        let sdna = count(cursor.uint(4)?, 4)?;
                        let nr = count(cursor.uint(4)?, 4)?;
                        (size, old, sdna, nr)
                    };
                    let offset = next;
                    position = checked::add(offset, size)?;
                    if position > data.len() {
                        return Err(Error::new("truncated", "block payload"));
                    }
                    if &code == b"ENDB" {
                        if size != 0 || nr != 0 {
                            return Err(Error::new("invalid_block", "ENDB payload/count"));
                        }
                        if position != data.len() {
                            return Err(Error::new("invalid_block", "bytes after ENDB"));
                        }
                        break;
                    }
                    if &code == b"DNA1" && dna.replace((offset, size)).is_some() {
                        return Err(Error::new("invalid_sdna", "multiple DNA1 blocks"));
                    }
                    blocks.push(Block {
                        code,
                        old_address,
                        sdna_index,
                        count: nr,
                        data_offset: offset,
                        size,
                    });
                    input::skip(reader, size)?;
                }
                Ok((blocks, dna))
            })?;
            let (offset, size) = dna.ok_or_else(|| Error::new("invalid_sdna", "missing DNA1"))?;
            let schema_span =
                crate::diagnostics::Span::new("schema", serde_json::json!({"bytes":size}));
            let schema = schema_span.result(Schema::parse(
                data.range(offset, size)?.as_ref(),
                header.little_endian,
                header.pointer_size,
            ))?;
            for block in &blocks {
                if &block.code != b"DNA1" && block.sdna_index >= schema.structs.len() {
                    return Err(Error::new("invalid_block", "SDNA index out of range"));
                }
            }
            let block_build_capacity = blocks.capacity();
            let blocks = blocks.into_boxed_slice();
            span.data(serde_json::json!({"blocks":blocks.len(),"types":schema.types.len(),"decoded_bytes":data.len(),"block_build_capacity":block_build_capacity,"block_build_storage_bytes":block_build_capacity * std::mem::size_of::<Block>(),"block_storage_bytes":std::mem::size_of_val(blocks.as_ref())}));
            Ok(Self {
                header,
                blocks,
                schema,
                data,
                source_path: None,
                by_address: std::cell::OnceCell::new(),
                name_index: std::cell::OnceCell::new(),
                owner_index: std::cell::OnceCell::new(),
            })
        })();
        span.result(result)
    }
    pub fn header(&self) -> &Header {
        &self.header
    }
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
    pub fn decompressed_len(&self) -> usize {
        self.data.len()
    }
    pub(crate) fn owner_index(&self) -> Result<&crate::file_ops::index::Index> {
        if self.owner_index.get().is_none() {
            let _ = self
                .owner_index
                .set(crate::file_ops::index::Index::new(self)?);
        }
        Ok(self.owner_index.get().unwrap())
    }
    pub(crate) fn check_source(&self) -> Result<()> {
        self.data.check_source()
    }
    pub(crate) fn serialized_header(&self) -> Result<Vec<u8>> {
        Ok(self.data.range(0, self.header.size)?.into_owned())
    }
    /// Explicit whole-body allocation. Facts queries never call this method.
    pub fn read_decompressed_bytes(&self) -> Result<Vec<u8>> {
        Ok(self.data.range(0, self.data.len())?.into_owned())
    }
    pub fn block(&self, index: usize) -> Result<&Block> {
        self.blocks
            .get(index)
            .ok_or_else(|| Error::new("bounds", "block index"))
    }
    pub fn payload(&self, index: usize) -> Result<Cow<'_, [u8]>> {
        let block = self.block(index)?;
        self.data.range(block.data_offset, block.size)
    }
    /// Referrer inspection reads record zero only, not giant opaque payloads.
    /// One reusable record buffer and one FD-identity guard at each scan boundary.
    pub(crate) fn scan_records(&self, visit: impl FnMut(usize, &[u8]) -> Result<()>) -> Result<()> {
        self.scan_records_where(|_| true, visit)
    }
    /// Filter record payload reads, not framing/extent or source admission.
    /// Callers whose query only concerns ID headers need not seek/read every
    /// unrelated DATA record (large geometry files contain millions of them).
    pub(crate) fn scan_records_where(
        &self,
        mut accepts: impl FnMut(usize) -> bool,
        mut visit: impl FnMut(usize, &[u8]) -> Result<()>,
    ) -> Result<()> {
        self.data.scan(|reader| {
            let mut record = Vec::new();
            for (i, b) in self.blocks.iter().enumerate() {
                if &b.code == b"DNA1" || b.count == 0 {
                    continue;
                }
                let size = self.schema.lengths[self.schema.structs[b.sdna_index].type_index];
                if size == 0 || checked::mul(b.count, size)? > b.size {
                    continue;
                }
                // Preserve the all-block checked extent arithmetic above,
                // even for records this query does not interpret.
                if !accepts(i) {
                    continue;
                }
                record.resize(size, 0);
                input::seek(reader, b.data_offset)?;
                input::read_exact(reader, &mut record)?;
                #[cfg(test)]
                SCAN_RECORD_READS.with(|n| {
                    if let Some(value) = n.get() {
                        n.set(Some(value + 1));
                    }
                });
                visit(i, &record)?;
            }
            Ok(())
        })
    }
    pub(crate) fn check_record_extent(&self, index: usize, len: usize) -> Result<()> {
        let block = self.block(index)?;
        if len > block.size {
            return Err(Error::new(
                "truncated",
                "declared records exceed block payload",
            ));
        }
        self.data.check_range(block.data_offset, len)
    }
    /// Borrowed payload view for dense planning loops: one window/`Arc`
    /// hold per block instead of one owned copy per field. Same bounds,
    /// guards and byte identity as `payload()`.
    pub(crate) fn payload_view(&self, index: usize) -> Result<input::WindowView<'_>> {
        let block = self.block(index)?;
        self.data.view(block.data_offset, block.size)
    }
    pub(crate) fn field_bytes(
        &self,
        index: usize,
        offset: usize,
        len: usize,
    ) -> Result<Cow<'_, [u8]>> {
        let block = self.block(index)?;
        if checked::add(offset, len)? > block.size {
            return Err(Error::new("truncated", "field outside block payload"));
        }
        self.data
            .range(checked::add(block.data_offset, offset)?, len)
    }
    pub fn dereference(&self, address: u64) -> Result<Option<usize>> {
        // Actual pointer-resolving owner in lib.rs: query.resolve phase.
        // Null stays a valid empty resolution; only real targets resolve.
        let mut span = crate::diagnostics::Span::new_lazy("query.resolve", || {
            if crate::diagnostics::enabled() {
                serde_json::json!({"address":format!("{address:#x}"),"role":"pointer_dereference"})
            } else {
                serde_json::Value::Null
            }
        });
        let result = (|| {
            if address == 0 {
                return Ok(None);
            }
            let addresses = self.by_address.get_or_init(|| {
                let mut map = HashMap::new();
                for (i, b) in self.blocks.iter().enumerate() {
                    if b.old_address != 0 {
                        map.entry(b.old_address)
                            .and_modify(|v| *v = None)
                            .or_insert(Some(i));
                    }
                }
                map
            });
            match addresses.get(&address) {
                Some(Some(index)) => Ok(Some(*index)),
                Some(None) => Err(Error::new(
                    "ambiguous_pointer",
                    format!("duplicate address {address:#x}"),
                )),
                None => Err(Error::new(
                    "dangling_pointer",
                    format!("address {address:#x}"),
                )),
            }
        })();
        // A null address is a successful empty resolution, not an error.
        // Record the resolved target only when enabled to avoid formatting.
        if let Err(error) = &result {
            let source = crate::failure_receipt::SourceContext {
                file: self.source_path.clone(),
                pointer: Some(address),
                namespace: crate::failure_receipt::Namespace::GlobalDiagnostic,
                resolution: if error.kind == "ambiguous_pointer" {
                    crate::failure_receipt::Resolution::Ambiguous
                } else {
                    crate::failure_receipt::Resolution::Missing
                },
                ..Default::default()
            };
            crate::failure_receipt::record_source_failure(
                error,
                if error.kind == "ambiguous_pointer" {
                    "pointer.namespace.lookup.ambiguous"
                } else {
                    "pointer.namespace.lookup.missing"
                },
                source,
                &[],
            );
        }
        if result.is_ok() && span.active() {
            span.data(serde_json::json!({"resolved":result.as_ref().ok().and_then(|v| *v)}));
        }
        span.result(result)
    }
    pub fn struct_index(&self, index: usize) -> Result<usize> {
        let block = self.block(index)?;
        if &block.code == b"DNA1" {
            return Err(Error::new("unsupported_type", "DNA1 is metadata"));
        }
        Ok(block.sdna_index)
    }
    pub(crate) fn primitive(&self, index: usize, offset: usize, width: usize) -> Result<u64> {
        number(
            self.field_bytes(index, offset, width)?.as_ref(),
            0,
            width,
            self.header.little_endian,
        )
    }
}
