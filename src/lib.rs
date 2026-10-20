//! Oxblend serialized reader and explicit new-output path editor.
//! No Blender evaluation, mmap, process caches, or incidental scene decoding.
/// Unstable/internal Rust API: may change in minor releases. CLI/JSON contracts are separate.
pub mod bat_cli;
mod checked;
pub mod diff;
/// Unstable/internal operations: may change in minor releases; CLI/JSON contracts are separate.
/// Exceptions stable in 1.x: EditOptions, EditReport, EditRange, EditFailure and Durability,
/// because these types are reachable from the stable packaging interface.
pub mod edit;
mod edit_container;
mod edit_decode;
mod edit_output;
mod edit_xxh;
pub mod exr_header;
pub mod facts;
mod fetch;
/// Unstable/internal Rust API: may change in minor releases. CLI/JSON contracts are separate.
pub mod file_ops;
#[cfg(test)]
mod file_ops_tests;
mod id_codes;
mod input;
mod kernel_copy;
pub mod overrides;
#[cfg(test)]
mod owner_compat_tests;
pub mod pack;
mod pack_zip;
mod phase;
mod physical;
pub mod schema;
mod seek_input;
mod stream_decode;
#[cfg(test)]
mod stream_decode_tests;
pub mod summary;
#[cfg(test)]
#[path = "../tests/support/scratch.rs"]
mod test_scratch;
pub mod trace;
mod trace_input;
#[cfg(test)]
mod trace_input_tests;
/// Unstable/internal Rust API: may change in minor releases. CLI/JSON contracts are separate.
pub mod typed_edit;
/// Unstable/internal Rust API: may change in minor releases. CLI/JSON contracts are separate.
pub mod typed_spec;
pub mod value;
use checked::{Cursor, bytes, number};
use input::Backing;
use schema::Schema;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Error {
    pub kind: &'static str,
    pub message: String,
}
impl Error {
    pub fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
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
#[derive(Debug)]
#[non_exhaustive]
pub struct BlendFile {
    header: Header,
    blocks: Vec<Block>,
    schema: Schema,
    data: Backing,
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
impl BlendFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_backing(Backing::open(path.as_ref())?)
    }
    /// Blocks inspection has an invocation-scoped streaming/frame reader;
    /// ordinary trace/list and kept-closure admission policies stay unchanged.
    pub(crate) fn open_blocks(path: &Path) -> Result<Self> {
        let reader = Self::from_backing(Backing::Trace(Box::new(
            crate::trace_input::Input::open_blocks(path)?,
        )))?;
        reader.check_source()?;
        Ok(reader)
    }
    pub(crate) fn open_trace(path: &Path) -> Result<Self> {
        let reader = Self::from_backing(Backing::Trace(Box::new(
            crate::trace_input::Input::open_native(path)?,
        )))?;
        reader.check_source()?;
        Ok(reader)
    }
    pub(crate) fn open_trace_low_memory(path: &Path) -> Result<Self> {
        let input = crate::trace_input::Input::open(path)?;
        let prefix = input.is_prefix();
        let signature = input.signature();
        let reader = match Self::from_backing(Backing::Trace(Box::new(input))) {
            Err(e) if prefix => {
                let spill = crate::trace_input::Input::open_spill(path, e.to_string())?;
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
        Ok(reader)
    }
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        Self::from_backing(Backing::Bytes(data))
    }
    pub(crate) fn from_backing(data: Backing) -> Result<Self> {
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
        let schema = Schema::parse(
            data.range(offset, size)?.as_ref(),
            header.little_endian,
            header.pointer_size,
        )?;
        for block in &blocks {
            if &block.code != b"DNA1" && block.sdna_index >= schema.structs.len() {
                return Err(Error::new("invalid_block", "SDNA index out of range"));
            }
        }
        Ok(Self {
            header,
            blocks,
            schema,
            data,
            by_address: std::cell::OnceCell::new(),
            name_index: std::cell::OnceCell::new(),
            owner_index: std::cell::OnceCell::new(),
        })
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
    pub(crate) fn scan_records(
        &self,
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
                record.resize(size, 0);
                input::seek(reader, b.data_offset)?;
                input::read_exact(reader, &mut record)?;
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
