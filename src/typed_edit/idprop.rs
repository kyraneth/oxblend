//! Same-allocation UTF-8 IDProperty string value edits; never pointer edits.
use super::*;

pub struct IdpropString {
    pub block_index: usize,
    pub element: usize,
    pub name: Vec<u8>,
    pub expected: Vec<u8>,
    pub replacement: Vec<u8>,
}
fn path(names: &[&str]) -> Vec<PathStep> {
    names
        .iter()
        .map(|s| PathStep::Field(s.as_bytes().to_vec()))
        .collect()
}
fn integer(
    f: &BlendFile,
    b: usize,
    e: usize,
    name: &str,
    kind: &[u8],
    width: usize,
) -> Result<i64> {
    let p = path(&[name]);
    let loc = f.locate_field(b, e, &p)?;
    if loc.field.pointer
        || loc.field.array_len != 1
        || loc.field.size != width
        || f.schema().types[loc.field.type_index] != kind
    {
        return Err(Error::new(
            "idprop_string",
            "IDProperty scalar layout mismatch",
        ));
    }
    match f.get(b, e, &p, true)? {
        crate::value::Value::Signed(n) => Ok(n),
        crate::value::Value::Unsigned(n) => {
            i64::try_from(n).map_err(|_| Error::new("idprop_string", "scalar overflow"))
        }
        _ => Err(Error::new("idprop_string", "IDProperty scalar required")),
    }
}

pub fn plan_strings<'a>(
    s: &'a EditSnapshot,
    mut plan: EditPlan<'a>,
    edits: &[IdpropString],
) -> Result<EditPlan<'a>> {
    if !std::ptr::eq(s, plan.snapshot) {
        return Err(Error::new(
            "snapshot_mismatch",
            "plan belongs to another snapshot",
        ));
    }
    let f = s.reader();
    for w in edits {
        if !matches!(f.header().version, 502 | 503)
            || record_type(f, w.block_index)? != b"IDProperty"
        {
            return Err(Error::new(
                "idprop_string",
                "requires Blender 502/503 IDProperty layout",
            ));
        }
        for bytes in [&w.name, &w.expected, &w.replacement] {
            if bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
                return Err(Error::new(
                    "idprop_string",
                    "name and values require UTF-8 without embedded NUL",
                ));
            }
        }
        let field = s.field(w.block_index, w.element, path(&["data", "pointer"]))?;
        local(s, &field)?;
        let loc = f.locate_field(w.block_index, w.element, &field.steps)?;
        if loc.parent_type != b"IDPropertyData"
            || !loc.field.pointer
            || loc.field.array_len != 1
            || loc.field.full_name != b"*pointer"
            || loc.field.size != f.header().pointer_size
            || f.schema().types[loc.field.type_index] != b"void"
        {
            return Err(Error::new(
                "idprop_string",
                "IDProperty data.pointer layout mismatch",
            ));
        }
        if integer(f, w.block_index, w.element, "type", b"char", 1)? != 0
            || integer(f, w.block_index, w.element, "subtype", b"char", 1)? != 0
        {
            return Err(Error::new(
                "idprop_string",
                "requires existing string type and UTF-8 subtype",
            ));
        }
        let name_path = path(&["name"]);
        let name = f.locate_field(w.block_index, w.element, &name_path)?;
        if name.field.pointer
            || f.schema().types[name.field.type_index] != b"char"
            || name.field.element_size != 1
            || name.indexed
            || name.field.array_len < 2
        {
            return Err(Error::new(
                "idprop_string",
                "IDProperty name layout mismatch",
            ));
        }
        let raw_name = f.field_bytes(w.block_index, name.offset, name.field.size)?;
        let end = raw_name
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| Error::new("idprop_string", "unterminated property name"))?;
        if raw_name[..end] != w.name {
            return Err(Error::new("expected_old", "IDProperty name guard differs"));
        }
        let len = integer(f, w.block_index, w.element, "len", b"int", 4)?;
        let capacity = integer(f, w.block_index, w.element, "totallen", b"int", 4)?;
        let crate::value::Value::Pointer(address) =
            f.get(w.block_index, w.element, &field.steps, true)?
        else {
            return Err(Error::new("idprop_string", "string pointer required"));
        };
        let target = f
            .resolve_pointer(w.block_index, w.element, &field.steps, address)?
            .ok_or_else(|| Error::new("idprop_string", "null string allocation"))?;
        let b = f.block(target)?;
        if b.code != *b"DATA"
            || b.sdna_index != 0
            || b.old_address != address
            || s.owners[target] != Some(field.owner)
            || len < 1
            || capacity < len
            || usize::try_from(capacity).ok() != Some(b.size)
        {
            return Err(Error::new(
                "idprop_string",
                "requires exact same-owner raw allocation and 1 <= len <= totallen == allocation size",
            ));
        }
        let old = f.field_bytes(target, 0, b.size)?;
        let len = usize::try_from(len)
            .map_err(|_| Error::new("idprop_string", "invalid string length"))?;
        if old[..len].last() != Some(&0)
            || old[..len - 1].contains(&0)
            || old[..len - 1] != w.expected
        {
            return Err(Error::new(
                "expected_old",
                "IDProperty string value/length guard differs",
            ));
        }
        let new_len = add(w.replacement.len(), 1)?;
        if new_len > b.size {
            return Err(Error::new(
                "idprop_string",
                format!(
                    "replacement needs {new_len} bytes including NUL; existing allocation limit is {} bytes",
                    b.size
                ),
            ));
        }
        // Audit every SDNA pointer, including interior references, independent of
        // semantic profiles. Opaque DATA is scanned conservatively at every byte
        // for pointer-table aliases; false positives refuse rather than mutate.
        let pointer_offset = add(f.block(w.block_index)?.data_offset, loc.offset)?;
        let limit = address
            .checked_add(b.size as u64)
            .ok_or_else(|| Error::new("idprop_string", "allocation address overflow"))?;
        let mut references = 0;
        for (i, block) in f.blocks().iter().enumerate() {
            if let Some(sites) = crate::file_ops::pointer_sites(f, i)? {
                for site in sites {
                    let bytes = f.field_bytes(i, site.offset, f.header().pointer_size)?;
                    let mut raw = [0u8; 8];
                    raw[..bytes.len()].copy_from_slice(&bytes);
                    let a = u64::from_le_bytes(raw);
                    if a >= address && a < limit {
                        if add(block.data_offset, site.offset)? != pointer_offset || a != address {
                            return Err(Error::new(
                                "idprop_string",
                                "string allocation has another or interior pointer alias",
                            ));
                        }
                        references += 1;
                    }
                }
            } else if block.code == *b"DATA" {
                let bytes = f.field_bytes(i, 0, block.size)?;
                for chunk in bytes.windows(f.header().pointer_size) {
                    let mut raw = [0u8; 8];
                    raw[..chunk.len()].copy_from_slice(chunk);
                    let a = u64::from_le_bytes(raw);
                    if a >= address && a < limit {
                        return Err(Error::new(
                            "idprop_string",
                            "opaque allocation may alias string payload",
                        ));
                    }
                }
            }
        }
        if references != 1 {
            return Err(Error::new(
                "idprop_string",
                "requires exactly one string payload reference",
            ));
        }
        let len_field = s.field(w.block_index, w.element, path(&["len"]))?;
        let (length_span, _, length_decl) = candidate(
            s,
            &len_field,
            b"int",
            4,
            &TypedValue::Signed(len as i64),
            &TypedValue::Signed(
                i64::try_from(new_len)
                    .map_err(|_| Error::new("typed_overflow", "IDProperty length exceeds int"))?,
            ),
        )?;
        let mut bytes = w.replacement.clone();
        bytes.push(0);
        let payload_span = Span {
            offset: b.data_offset,
            bytes: if old[..new_len] == bytes {
                Vec::new()
            } else {
                bytes.clone()
            },
        };
        let payload_decl = json!({"operation":"idprop-string","block_index":w.block_index,"element":w.element,
            "path":["data","pointer"],"name_hex":hex(&w.name),"payload_block_index":target,
            "offset":b.data_offset,"extent":b.size,"before_hex":hex(&old),"replacement_hex":hex(&bytes),
            "allocation_preserved":true,"pointer_preserved":true,"totallen_preserved":true});
        plan.declarations.extend([length_decl, payload_decl]);
        plan.spans.extend([length_span, payload_span]);
    }
    // Include full declared extents, even no-op writes, in conflict admission.
    let mut extents = plan
        .declarations
        .iter()
        .map(|d| {
            (
                d["offset"].as_u64().unwrap() as usize,
                d["extent"].as_u64().unwrap() as usize,
            )
        })
        .collect::<Vec<_>>();
    extents.sort_unstable();
    for pair in extents.windows(2) {
        if add(pair[0].0, pair[0].1)? > pair[1].0 {
            return Err(Error::new(
                "edit_conflict",
                "overlapping declared value extents",
            ));
        }
    }
    plan.spans.retain(|span| !span.bytes.is_empty());
    plan.spans.sort_by_key(|span| span.offset);
    Ok(plan)
}
