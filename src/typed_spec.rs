//! Strict schema-2 JSON transport for typed edits. No persisted offset patches.
use crate::typed_edit::*;
use crate::value::PathStep;
use crate::{Error, Result};
use serde_json::Value as J;
fn bad(message: &str) -> Error {
    Error::new("edit_spec", message)
}
fn keys(j: &J, allowed: &[&str]) -> Result<()> {
    let o = j.as_object().ok_or_else(|| bad("object required"))?;
    if o.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(bad("unknown spec key"));
    }
    Ok(())
}
fn text(j: &J) -> Result<&str> {
    j.as_str().ok_or_else(|| bad("string required"))
}
fn unhex(j: &J) -> Result<Vec<u8>> {
    let s = text(j)?;
    if !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad("even ASCII hex required"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| bad("invalid hex")))
        .collect()
}
fn uint(j: &J) -> Result<u64> {
    if let Some(v) = j.as_u64() {
        Ok(v)
    } else {
        text(j)?
            .parse()
            .map_err(|_| bad("unsigned decimal required"))
    }
}
fn index(j: &J) -> Result<usize> {
    usize::try_from(uint(j)?).map_err(|_| bad("usize overflow"))
}
fn value(j: &J) -> Result<TypedValue> {
    keys(j, &["kind", "value", "hex", "bits_hex", "items"])?;
    let k = text(&j["kind"])?;
    // Each tagged variant admits exactly its required payload, not mixed fields.
    let payload = match k {
        "signed" | "unsigned" => "value",
        "f32" | "f64" => "bits_hex",
        "cstring" | "bytes" => "hex",
        "array" => "items",
        _ => return Err(bad("unknown typed value kind")),
    };
    if j.as_object().unwrap().len() != 2 || j.get(payload).is_none() {
        return Err(bad("typed value requires exactly kind and its payload"));
    }
    Ok(match k {
        "signed" => TypedValue::Signed(if let Some(v) = j["value"].as_i64() {
            v
        } else {
            text(&j["value"])?
                .parse()
                .map_err(|_| bad("signed decimal required"))?
        }),
        "unsigned" => TypedValue::Unsigned(uint(&j["value"])?),
        "f32" => {
            let s = text(&j["bits_hex"])?;
            if s.len() != 8 {
                return Err(bad("f32 needs exactly 8 hex digits"));
            }
            TypedValue::F32Bits(u32::from_str_radix(s, 16).map_err(|_| bad("invalid float bits"))?)
        }
        "f64" => {
            let s = text(&j["bits_hex"])?;
            if s.len() != 16 {
                return Err(bad("f64 needs exactly 16 hex digits"));
            }
            TypedValue::F64Bits(u64::from_str_radix(s, 16).map_err(|_| bad("invalid float bits"))?)
        }
        "cstring" => TypedValue::CString(unhex(&j["hex"])?),
        "bytes" => TypedValue::FixedBytes(unhex(&j["hex"])?),
        "array" => TypedValue::Array(
            j["items"]
                .as_array()
                .ok_or_else(|| bad("items array required"))?
                .iter()
                .map(value)
                .collect::<Result<_>>()?,
        ),
        _ => unreachable!(),
    })
}
fn field<'a>(s: &'a EditSnapshot, j: &J) -> Result<FieldRef<'a>> {
    let steps = j["path"]
        .as_array()
        .ok_or_else(|| bad("path array required"))?
        .iter()
        .map(|p| {
            if let Some(v) = p.as_str() {
                Ok(PathStep::Field(v.as_bytes().to_vec()))
            } else if p.is_number() {
                Ok(PathStep::Index(index(p)?))
            } else {
                keys(p, &["field_hex", "index"])?;
                if p.as_object().unwrap().len() != 1 {
                    return Err(bad("one path step key required"));
                }
                if let Some(v) = p.get("field_hex") {
                    Ok(PathStep::Field(unhex(v)?))
                } else {
                    Ok(PathStep::Index(index(&p["index"])?))
                }
            }
        })
        .collect::<Result<_>>()?;
    s.field(index(&j["block_index"])?, index(&j["element"])?, steps)
}
pub fn plan<'a>(s: &'a EditSnapshot, j: &J, mode: EditMode) -> Result<EditPlan<'a>> {
    keys(j, &["schema", "writes", "bits"])?;
    if j["schema"] != 2 {
        return Err(bad("schema 2 required"));
    }
    let writes = j["writes"]
        .as_array()
        .ok_or_else(|| bad("writes array required"))?
        .iter()
        .map(|w| {
            keys(
                w,
                &[
                    "block_index",
                    "element",
                    "path",
                    "sdna_type",
                    "width",
                    "expected",
                    "replacement",
                ],
            )?;
            Ok(TypedWrite {
                field: field(s, w)?,
                sdna_type: text(&w["sdna_type"])?.as_bytes().to_vec(),
                width: index(&w["width"])?,
                expected: value(&w["expected"])?,
                replacement: value(&w["replacement"])?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let bits = j["bits"]
        .as_array()
        .ok_or_else(|| bad("bits array required"))?
        .iter()
        .map(|w| {
            keys(
                w,
                &[
                    "block_index",
                    "element",
                    "path",
                    "sdna_type",
                    "width",
                    "expected",
                    "clear",
                    "set",
                ],
            )?;
            Ok(BitWrite {
                field: field(s, w)?,
                sdna_type: text(&w["sdna_type"])?.as_bytes().to_vec(),
                width: index(&w["width"])?,
                expected: value(&w["expected"])?,
                clear: uint(&w["clear"])?,
                set: uint(&w["set"])?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    plan_fields(s, &writes, &bits, mode)
}
