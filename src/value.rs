//! Byte-safe primitive access. No implicit pointer traversal or UTF-8 replacement.
use crate::checked::{add, mul};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Value {
    Signed(i64),
    Unsigned(u64),
    Pointer(u64),
    Float32Bits(u32),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PathStep {
    Field(Vec<u8>),
    Index(usize),
}
pub fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}
impl Value {
    pub fn json(&self) -> Json {
        match self {
            Self::Signed(v) => json!({"kind":"signed", "decimal":v.to_string()}),
            Self::Unsigned(v) => json!({"kind":"unsigned", "decimal":v.to_string()}),
            Self::Pointer(v) => json!({"kind":"pointer", "decimal":v.to_string()}),
            Self::Float32Bits(v) => json!({"kind":"float32", "bits_hex":format!("{v:08x}")}),
            Self::Bytes(v) => json!({"kind":"bytes", "hex":hex(v)}),
            Self::Array(v) => {
                json!({"kind":"array", "items":v.iter().map(Value::json).collect::<Vec<_>>()})
            }
        }
    }
    pub fn utf8(&self) -> Result<&str> {
        match self {
            Self::Bytes(v) => {
                std::str::from_utf8(v).map_err(|_| Error::new("encoding", "invalid UTF-8"))
            }
            _ => Err(Error::new("type", "not bytes")),
        }
    }
}
pub(crate) struct FieldLocation<'a> {
    pub field: &'a crate::schema::Field,
    pub parent_type: &'a [u8],
    pub parent_offset: usize,
    pub list_role: Option<(&'a [u8], &'a [u8])>,
    pub offset: usize,
    pub indexed: bool,
}
impl BlendFile {
    /// Shared checked embedded-field resolution; never traverses a pointer.
    pub(crate) fn locate_field(
        &self,
        block_index: usize,
        element: usize,
        path: &[PathStep],
    ) -> Result<FieldLocation<'_>> {
        let block = self.block(block_index)?;
        let mut si = self.struct_index(block_index)?;
        let mut ty = self
            .schema
            .structs
            .get(si)
            .ok_or_else(|| Error::new("invalid_sdna", "struct index"))?
            .type_index;
        let record_size = self.schema.lengths[ty];
        if element >= block.count || record_size == 0 {
            return Err(Error::new("bounds", "record element"));
        }
        // Opaque blocks may have arbitrary count/type metadata; typed access must
        // prove the entire declared record extent, not just its first element.
        self.check_record_extent(block_index, mul(block.count, record_size)?)?;
        let mut base = mul(element, record_size)?;
        if add(base, record_size)? > block.size {
            return Err(Error::new("truncated", "record exceeds block payload"));
        }
        if path.is_empty() {
            return Err(Error::new("path", "empty field path"));
        }
        let mut pos = 0;
        let mut list_role = None;
        loop {
            let name = match path.get(pos) {
                Some(PathStep::Field(v)) => v,
                _ => return Err(Error::new("path", "expected field name")),
            };
            let structure = &self.schema.structs[si];
            let field_index = *structure
                .by_name
                .get(name.as_slice())
                .ok_or_else(|| Error::new("missing_field", hex(name)))?;
            let field = &structure.fields[field_index];
            let parent_offset = base;
            base = add(base, field.offset)?;
            pos += 1;
            let indexed = if let Some(PathStep::Index(index)) = path.get(pos) {
                if *index >= field.array_len {
                    return Err(Error::new("bounds", "array index"));
                }
                base = add(base, mul(*index, field.element_size)?)?;
                pos += 1;
                true
            } else {
                false
            };
            if field.array_len == 0 {
                return Err(Error::new("bounds", "zero-length array"));
            }
            ty = field.type_index;
            if pos < path.len() {
                if field.pointer {
                    return Err(Error::new(
                        "pointer_path",
                        "dereference pointer explicitly before traversing",
                    ));
                }
                if self.schema.types[ty] == b"ListBase" {
                    list_role = Some((
                        self.schema.types[structure.type_index].as_slice(),
                        field.name.as_slice(),
                    ));
                }
                si = *self
                    .schema
                    .by_type
                    .get(&ty)
                    .ok_or_else(|| Error::new("unsupported_type", "not embedded struct"))?;
                continue;
            }
            return Ok(FieldLocation {
                field,
                parent_type: &self.schema.types[structure.type_index],
                parent_offset,
                list_role,
                offset: base,
                indexed,
            });
        }
    }
    /// Resolve serialized embedded-field paths; pointer dereference is a separate call.
    /// Indexes flatten SDNA dimensions. Negative or out-of-bounds paths never read.
    pub fn get(
        &self,
        block_index: usize,
        element: usize,
        path: &[PathStep],
        null_terminated: bool,
    ) -> Result<Value> {
        let location = self.locate_field(block_index, element, path)?;
        let field = location.field;
        let base = location.offset;
        let indexed = location.indexed;
        if field.pointer {
            return Ok(Value::Pointer(self.primitive(
                block_index,
                base,
                self.header.pointer_size,
            )?));
        }
        let kind = self.schema.types[field.type_index].as_slice();
        let count = if indexed { 1 } else { field.array_len };
        if kind == b"char" && field.element_size != 1 {
            return Err(Error::new("invalid_sdna", "char width/TLEN mismatch"));
        }
        if kind == b"char" && count > 1 {
            let slice = self.field_bytes(block_index, base, count)?;
            let end = if null_terminated {
                slice.iter().position(|&b| b == 0).unwrap_or(slice.len())
            } else {
                slice.len()
            };
            return Ok(Value::Bytes(slice[..end].to_vec()));
        }
        let width = match kind {
            b"char" | b"uchar" | b"int8_t" => 1,
            b"short" => 2,
            b"int" | b"float" => 4,
            b"uint64_t" => 8,
            _ => return Err(Error::new("unsupported_type", hex(kind))),
        };
        if field.element_size != width {
            return Err(Error::new("invalid_sdna", "primitive width/TLEN mismatch"));
        }
        let one = |offset| -> Result<Value> {
            let bits = self.primitive(block_index, offset, width)?;
            Ok(match kind {
                b"int8_t" => Value::Signed(bits as i8 as i64),
                b"short" => Value::Signed(bits as i16 as i64),
                b"int" => Value::Signed(bits as i32 as i64),
                b"float" => Value::Float32Bits(bits as u32),
                _ => Value::Unsigned(bits),
            })
        };
        if count == 1 {
            return one(base);
        }
        let mut values = Vec::new();
        for index in 0..count {
            values.push(one(add(base, mul(index, width)?)?)?);
        }
        Ok(Value::Array(values))
    }
    /// Native datamap/ID resolution, retaining the referencing record's owner.
    /// Uses the same qualified owner validator and lazy scopes as FILE-OPS.
    pub(crate) fn resolve_pointer(
        &self,
        block: usize,
        element: usize,
        path: &[PathStep],
        address: u64,
    ) -> Result<Option<usize>> {
        self.resolve_pointer_hop(block, element, path, address, false)
    }
    /// A checked caller has read one pointer from a serialized DATA table.
    pub(crate) fn resolve_pointer_element(
        &self,
        block: usize,
        element: usize,
        path: &[PathStep],
        address: u64,
    ) -> Result<Option<usize>> {
        self.resolve_pointer_hop(block, element, path, address, true)
    }
    fn resolve_pointer_hop(
        &self,
        block: usize,
        element: usize,
        path: &[PathStep],
        address: u64,
        table_entry: bool,
    ) -> Result<Option<usize>> {
        if address == 0 {
            return Ok(None);
        }
        let location = self.locate_field(block, element, path)?;
        if !location.field.pointer {
            return Err(Error::new("type", "not a pointer field"));
        }
        let mut pointer = crate::file_ops::PointerLayout {
            offset: location.offset,
            path: format!("{path:?}"),
            declared_type: self.schema().types[location.field.type_index].clone(),
            parent_type: location.parent_type.to_vec(),
            parent_offset: location.parent_offset,
            field_name: location.field.name.clone(),
            pointer_depth: location
                .field
                .full_name
                .iter()
                .filter(|b| **b == b'*')
                .count(),
            list_role: location.list_role.map(|(t, n)| (t.to_vec(), n.to_vec())),
        };
        if table_entry {
            if pointer.pointer_depth < 2 {
                return Err(Error::new(
                    "pointer_depth",
                    "table entry requires a pointer-to-pointer field",
                ));
            }
            pointer.pointer_depth -= 1;
        }
        let target = self
            .owner_index()?
            .resolve(self, block, address, &pointer)
            .map_err(|e| {
                if e.kind == "unresolved_persistent" {
                    Error::new("dangling_pointer", e.message)
                } else {
                    e
                }
            })?;
        if crate::file_ops::roles::checked_source_role(&pointer) {
            // A saved dependency role admits a checked target, not arbitrary
            // bytes at its address. Keep material tables and unrelated source
            // paths on their existing contracts, not a new generic admission.
            crate::file_ops::roles::qualify(self, block, &pointer, target)?;
            if pointer.parent_type == b"ListBase" {
                crate::file_ops::roles::validate_list(self, block, &pointer)?;
            }
        }
        Ok(Some(target))
    }
    pub fn get_pointer(
        &self,
        block: usize,
        element: usize,
        path: &[PathStep],
    ) -> Result<Option<usize>> {
        match self.get(block, element, path, true)? {
            Value::Pointer(address) => self.resolve_pointer(block, element, path, address),
            _ => Err(Error::new("type", "not a pointer field")),
        }
    }
}
