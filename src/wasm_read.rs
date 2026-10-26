//! Read-only, owned-byte boundary for wasm32. Raw BLENDER containers only.
//! No filesystem, tracing, worker threads, compression, pointer traversal or writers.
use crate::{
    Block, Error, Header, Result,
    checked::{self, Cursor},
    schema::Schema,
};

#[derive(Debug)]
#[non_exhaustive]
pub struct BlendFile {
    header: Header,
    blocks: Vec<Block>,
    schema: Schema,
    bytes: Vec<u8>,
}
impl BlendFile {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let header = crate::header(&bytes)?;
        let mut offset = header.size;
        let mut blocks = Vec::new();
        let mut dna = None;
        loop {
            let width = if header.file_format == 1 {
                32
            } else {
                16 + header.pointer_size
            };
            let raw = checked::bytes(&bytes, offset, width)?;
            let mut cursor = Cursor {
                data: raw,
                offset: 0,
                little: header.little_endian,
            };
            let mut code = [0u8; 4];
            code.copy_from_slice(cursor.take(4)?);
            let (size, address, sdna_index, count) = if header.file_format == 1 {
                let sdna = crate::count(cursor.uint(4)?, 4)?;
                let address = cursor.uint(8)?;
                (
                    crate::count(cursor.uint(8)?, 8)?,
                    address,
                    sdna,
                    crate::count(cursor.uint(8)?, 8)?,
                )
            } else {
                (
                    crate::count(cursor.uint(4)?, 4)?,
                    cursor.uint(header.pointer_size)?,
                    crate::count(cursor.uint(4)?, 4)?,
                    crate::count(cursor.uint(4)?, 4)?,
                )
            };
            let data_offset = checked::add(offset, width)?;
            offset = checked::add(data_offset, size)?;
            checked::bytes(&bytes, data_offset, size)?;
            if &code == b"ENDB" {
                if size != 0 || count != 0 || offset != bytes.len() {
                    return Err(Error::new(
                        "invalid_block",
                        "ENDB extent/count or trailing bytes",
                    ));
                }
                break;
            }
            if &code == b"DNA1" && dna.replace((data_offset, size)).is_some() {
                return Err(Error::new("invalid_sdna", "multiple DNA1 blocks"));
            }
            blocks.push(Block {
                code,
                old_address: address,
                sdna_index,
                count,
                data_offset,
                size,
            });
        }
        let (start, len) = dna.ok_or_else(|| Error::new("invalid_sdna", "missing DNA1"))?;
        let schema = Schema::parse(
            checked::bytes(&bytes, start, len)?,
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
            bytes,
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
    pub fn payload(&self, index: usize) -> Result<&[u8]> {
        let b = self
            .blocks
            .get(index)
            .ok_or_else(|| Error::new("bounds", "block index"))?;
        checked::bytes(&self.bytes, b.data_offset, b.size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_raw_fixture_and_negative_boundaries() {
        let bytes = include_bytes!("../tests/fixtures/file-ops/synthetic-empty-seed.blend");
        let file = BlendFile::from_bytes(bytes.to_vec()).unwrap();
        assert!(!file.blocks().is_empty());
        assert!(!file.schema().structs.is_empty());
        assert!(file.payload(file.blocks().len()).is_err());
        assert!(BlendFile::from_bytes(bytes[..bytes.len() - 1].to_vec()).is_err());
        assert!(BlendFile::from_bytes(b"not a blend".to_vec()).is_err());
    }
}
