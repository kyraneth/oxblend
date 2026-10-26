//! Exact saved-property representation evidence, separate from semantic values.
use super::access::{self, Record};
use crate::value::{Value, hex};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};

pub(super) fn properties(file: &BlendFile) -> Result<Vec<Json>> {
    let mut rows = Vec::new();
    for r in access::records(file, "IDProperty")? {
        let ty = access::int(file, &r, "type")?;
        let sub = access::int(file, &r, "subtype")?;
        let name = match access::get(file, &r, "name")? {
            Value::Bytes(v) => v,
            _ => return Err(Error::new("type", "IDProperty name must be bytes")),
        };
        let lo = access::int(file, &r, "data.val")? as i32 as u32;
        let hi = access::int(file, &r, "data.val2")? as i32 as u32;
        let bits = if file.header().little_endian {
            (u64::from(hi) << 32) | u64::from(lo)
        } else {
            (u64::from(lo) << 32) | u64::from(hi)
        };
        let scalar = match ty {
            1 => {
                let width = file
                    .locate_field(r.block, r.element, &access::steps("data.val"))?
                    .field
                    .element_size
                    * 8;
                json!({"integer_width_bits":width,"signed":true})
            }
            2 => json!({"float_width_bits":32,"bits_hex":format!("{lo:08x}")}),
            8 => json!({"float_width_bits":64,"bits_hex":format!("{bits:016x}")}),
            _ => Json::Null,
        };
        rows.push(json!({"source_local_ref":{"block_index":r.block,"element":r.element},"name_hex":hex(&name),"saved_type":ty,"saved_subtype":sub,"length":access::int(file,&r,"len")?,"scalar":scalar}));
    }
    Ok(rows)
}

/// Preserve authoritative names and exact source-local identity for every saved
/// ID owner; semantic display refs may contain null for invalid UTF-8.
pub(super) fn identities(file: &BlendFile) -> Result<Vec<Json>> {
    let mut out = Vec::new();
    for (block, b) in file.blocks().iter().enumerate() {
        if &b.code == b"DNA1" {
            continue;
        }
        let ty = &file.schema().types[file.schema().structs[b.sdna_index].type_index];
        let has_id = ty == b"ID"
            || file.schema().structs[b.sdna_index].fields.iter().any(|f| {
                f.name == b"id" && file.schema().types[f.type_index] == b"ID" && !f.pointer
            });
        if !has_id {
            continue;
        }
        for element in 0..b.count {
            let r = Record::new(block, element);
            let field = if ty == b"ID" { "name" } else { "id.name" };
            let name = match access::get(file, &r, field)? {
                Value::Bytes(v) => v,
                _ => return Err(Error::new("type", "saved ID name must be bytes")),
            };
            out.push(json!({"source_local_ref":{"block_index":block,"element":element},"name_hex":hex(&name),"library_pointer":access::get(file,&r,if ty==b"ID"{"lib"}else{"id.lib"})?.json()}));
        }
    }
    Ok(out)
}
