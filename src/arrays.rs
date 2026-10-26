//! Generic readonly saved-array readers: byte payloads and pointer tables.
//!
//! `byte_array` reads `count` raw bytes from the allocation one saved width-1
//! scalar pointer (`char*`, `uchar*`, `int8_t*`) selects. `pointer_table`
//! reads `count` slots from the allocation one saved `struct**` selects and
//! resolves each non-null entry to its owning block. Counts come from the
//! consumer (for example an actual saved `totcol`); these readers never
//! invent one. Before any allocation or read they prove the field's saved
//! pointer shape and the target allocation's actual saved extent.
//!
//! Ownership stays with the referencing record: the data/table allocation
//! resolves through `resolve_pointer` and every table entry through
//! `resolve_pointer_element` on the original block and element, so the
//! source-role namespaces (owner DATA run vs the ID lib-map) cannot rescue a
//! wrong-namespace address, and a same-address allocation in another owner's
//! DATA run stays unresolved. Each non-null entry is then qualified with the
//! shared `roles::qualify` on the scalarized pointer layout, the same proof
//! the native resolver applies to its checked sites. The existing trace
//! resolver is unchanged.
use crate::checked;
use crate::file_ops::PointerLayout;
use crate::value::{PathStep, display_bytes, display_path};
use crate::{BlendFile, Error, Result};
use serde_json::json;

/// Saved pointee declarations `byte_array` accepts: one-byte scalar types.
const BYTE_POINTERS: [&[u8]; 3] = [b"char", b"uchar", b"int8_t"];

fn pointed_field<'a>(
    file: &'a BlendFile,
    block: usize,
    element: usize,
    path: &[PathStep],
    depth: usize,
) -> Result<crate::value::FieldLocation<'a>> {
    let location = file.locate_field(block, element, path)?;
    let field = location.field;
    if !field.pointer || field.full_name.contains(&b'(') {
        return Err(Error::new(
            "type",
            "array read requires a simple saved pointer declarator, not an inline value or callable",
        ));
    }
    if !location.indexed && field.array_len > 1 {
        return Err(Error::new(
            "type",
            "array read requires one selected pointer element, not a whole pointer array",
        ));
    }
    if field.full_name.iter().filter(|b| **b == b'*').count() != depth {
        return Err(Error::new(
            "pointer_depth",
            format!("array read requires pointer depth {depth}"),
        ));
    }
    Ok(location)
}

impl BlendFile {
    /// Read `count` bytes from the allocation one saved width-1 scalar pointer
    /// (`char*`, `uchar*`, `int8_t*`) selects.
    ///
    /// The count is the consumer's (for example an actual saved `totcol`); it
    /// is refused when it exceeds the allocation's actual saved bytes, before
    /// any allocation or read. Embedded NUL/zero bytes are data and are
    /// preserved: this is a raw byte reader, not a string reader. A count of
    /// zero returns an empty vector after the field-type proof, without
    /// dereferencing the pointer.
    pub fn byte_array(
        &self,
        block_index: usize,
        element: usize,
        path: &[PathStep],
        count: usize,
    ) -> Result<Vec<u8>> {
        let mut span = crate::diagnostics::Span::new_lazy("read.byte_array", || {
            json!({
                "block_index": block_index,
                "element": element,
                "path": display_path(path),
                "count": count,
            })
        });
        let result = (|| {
            self.check_source()?;
            let location = pointed_field(self, block_index, element, path, 1)?;
            let field = location.field;
            let declared = self.schema().types[field.type_index].as_slice();
            if !BYTE_POINTERS.contains(&declared) {
                return Err(Error::new(
                    "type",
                    format!(
                        "byte_array requires a char/uchar/int8_t pointer, not '{}'",
                        display_bytes(declared)
                    ),
                ));
            }
            // Saved pointee width comes from SDNA TLEN: pointer fields carry
            // pointer_size as element_size, which is not the pointee width.
            if self.schema().lengths[field.type_index] != 1 {
                return Err(Error::new(
                    "type",
                    format!(
                        "byte_array pointee '{}' has saved width {}, not 1",
                        display_bytes(declared),
                        self.schema().lengths[field.type_index]
                    ),
                ));
            }
            if count == 0 {
                self.check_source()?;
                return Ok(Vec::new());
            }
            let width = self.header().pointer_size;
            let address = self.primitive(block_index, location.offset, width)?;
            if address == 0 {
                return Err(Error::new(
                    "required_pointer_null",
                    "nonzero byte count with a null byte-array pointer",
                ));
            }
            // The data allocation keeps the referencing record's owner context.
            let target = self
                .resolve_pointer(block_index, element, path, address)?
                .ok_or_else(|| Error::new("dangling_pointer", format!("{address:#x}")))?;
            let allocation = self.block(target)?;
            // The saved pointee width is 1, so count x width is count itself.
            if count > allocation.size {
                return Err(Error::new(
                    "pointer_extent",
                    format!(
                        "byte count {count} exceeds the {address:#x} allocation's {} saved bytes",
                        allocation.size
                    ),
                ));
            }
            let bytes = self.field_bytes(target, 0, count)?.into_owned();
            self.check_source()?;
            Ok(bytes)
        })();
        if crate::diagnostics::enabled()
            && let Ok(bytes) = &result
        {
            span.data(json!({"bytes":bytes.len()}));
        }
        span.result(result)
    }

    /// Read `count` pointer slots from the allocation one saved `struct**`
    /// selects and resolve each non-null entry to its owning block index.
    ///
    /// The count is the consumer's (for example an actual saved `totcol`); it
    /// is refused when `count x pointer width` exceeds the table allocation's
    /// actual saved bytes, before any allocation or read, so a table can never
    /// be read past its block into adjacent DATA. The table allocation and
    /// every entry resolve in the ORIGINAL owner's context; null slots stay
    /// `None` in table order. Non-null entries are qualified against the
    /// declared struct type with the scalarized pointer layout. A count of
    /// zero returns an empty vector after the field-type proof, without
    /// dereferencing the table pointer.
    pub fn pointer_table(
        &self,
        block_index: usize,
        element: usize,
        path: &[PathStep],
        count: usize,
    ) -> Result<Vec<Option<usize>>> {
        let mut span = crate::diagnostics::Span::new_lazy("read.pointer_table", || {
            json!({
                "block_index": block_index,
                "element": element,
                "path": display_path(path),
                "count": count,
            })
        });
        let result = (|| {
            self.check_source()?;
            let location = pointed_field(self, block_index, element, path, 2)?;
            let field = location.field;
            if !self.schema().by_type.contains_key(&field.type_index) {
                return Err(Error::new(
                    "type",
                    format!(
                        "pointer_table requires a declared struct pointee, not '{}'",
                        display_bytes(&self.schema().types[field.type_index])
                    ),
                ));
            }
            if count == 0 {
                self.check_source()?;
                return Ok(Vec::new());
            }
            let width = self.header().pointer_size;
            let address = self.primitive(block_index, location.offset, width)?;
            if address == 0 {
                return Err(Error::new(
                    "required_pointer_null",
                    "nonzero count with a null pointer-table allocation",
                ));
            }
            // The table allocation keeps the referencing record's owner context.
            let table = self
                .resolve_pointer(block_index, element, path, address)?
                .ok_or_else(|| Error::new("dangling_pointer", format!("{address:#x}")))?;
            let allocation = self.block(table)?;
            let extent = checked::mul(count, width)?;
            if extent > allocation.size {
                return Err(Error::new(
                    "pointer_extent",
                    format!(
                        "pointer-table count {count} exceeds the {address:#x} allocation's {} saved bytes",
                        allocation.size
                    ),
                ));
            }
            // The scalarized layout non-null entries are qualified against:
            // the ** field with one level resolved, exactly what a saved
            // struct* site declares. Built once; roles::qualify is pure.
            let scalar = PointerLayout {
                offset: location.offset,
                path: display_path(path),
                declared_type: self.schema().types[field.type_index].clone(),
                parent_type: location.parent_type.to_vec(),
                parent_offset: location.parent_offset,
                field_name: field.name.clone(),
                pointer_depth: 1,
                list_role: location
                    .list_role
                    .map(|(ty, name)| (ty.to_vec(), name.to_vec())),
            };
            let mut resolved = Vec::with_capacity(count);
            for index in 0..count {
                let entry = self.primitive(table, checked::mul(index, width)?, width)?;
                let target = if entry == 0 {
                    None
                } else {
                    // Entries resolve in the ORIGINAL owner's context, never
                    // the table allocation's, so same-address allocations in
                    // another owner's DATA run cannot be confused with this one.
                    let target = self
                        .resolve_pointer_element(block_index, element, path, entry)?
                        .ok_or_else(|| Error::new("dangling_pointer", format!("{entry:#x}")))?;
                    crate::file_ops::roles::qualify(self, block_index, &scalar, target)?;
                    Some(target)
                };
                resolved.push(target);
            }
            self.check_source()?;
            Ok(resolved)
        })();
        if crate::diagnostics::enabled()
            && let Ok(slots) = &result
        {
            span.data(json!({"bytes":slots.len() * self.header().pointer_size}));
        }
        span.result(result)
    }
}
