//! Checked saved-record access shared by the query projections.
use crate::value::{PathStep, Value};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub block: usize,
    pub element: usize,
    pub prefix: Vec<PathStep>,
}
impl Record {
    pub fn new(block: usize, element: usize) -> Self {
        Self {
            block,
            element,
            prefix: Vec::new(),
        }
    }
    pub fn at(&self, path: &str) -> Self {
        let mut out = self.clone();
        out.prefix.extend(steps(path));
        out
    }
}
pub(crate) fn steps(path: &str) -> Vec<PathStep> {
    if path.is_empty() {
        return Vec::new();
    }
    path.split('.')
        .map(|p| {
            p.parse::<usize>()
                .map(PathStep::Index)
                .unwrap_or_else(|_| PathStep::Field(p.as_bytes().to_vec()))
        })
        .collect()
}
fn full(record: &Record, path: &str) -> Vec<PathStep> {
    let mut out = record.prefix.clone();
    out.extend(steps(path));
    out
}
pub(crate) fn get(file: &BlendFile, record: &Record, path: &str) -> Result<Value> {
    file.get(record.block, record.element, &full(record, path), true)
}
pub(crate) fn int(file: &BlendFile, record: &Record, path: &str) -> Result<i64> {
    match get(file, record, path)? {
        Value::Signed(v) => Ok(v),
        Value::Unsigned(v) => i64::try_from(v)
            .map_err(|_| Error::new("overflow", "unsigned integer does not fit i64")),
        _ => Err(Error::new("type", format!("{path}: expected integer"))),
    }
}
pub(crate) fn float(file: &BlendFile, record: &Record, path: &str) -> Result<f64> {
    match get(file, record, path)? {
        Value::Float32Bits(v) => Ok(f64::from(f32::from_bits(v))),
        _ => Err(Error::new("type", format!("{path}: expected float32"))),
    }
}
pub(crate) fn text(file: &BlendFile, record: &Record, path: &str) -> Result<String> {
    Ok(get(file, record, path)?.utf8()?.to_owned())
}
pub(crate) fn pointer(file: &BlendFile, record: &Record, path: &str) -> Result<Option<Record>> {
    let span = crate::diagnostics::Span::new_lazy(
        "query.resolve",
        || json!({"block_index":record.block,"element":record.element,"field":path}),
    );
    span.result(
        file.get_pointer(record.block, record.element, &full(record, path))
            .map(|v| v.map(|block| Record::new(block, 0))),
    )
}
/// One checked list reader; ordinary projections require a complete list.
pub(crate) fn list(file: &BlendFile, record: &Record, path: &str) -> Result<Vec<Record>> {
    let (rows, error) = list_partial(file, record, path);
    match error {
        Some(error) => Err(error),
        None => Ok(rows),
    }
}
/// Retain proved prefix entries and the terminal refusal for partial graph views.
pub(crate) fn list_partial(
    file: &BlendFile,
    record: &Record,
    path: &str,
) -> (Vec<Record>, Option<Error>) {
    let mut rows = Vec::new();
    let outcome = (|| -> Result<()> {
        let base = record.at(path);
        let first = match get(file, &base, "first") {
            Ok(_) => "first",
            Err(e) if e.kind == "missing_field" => "first_",
            Err(e) => return Err(e),
        };
        let last_name = if first == "first" { "last" } else { "last_" };
        let mut current = pointer(file, &base, first)?;
        let (last, terminal) = match pointer(file, &base, last_name) {
            Ok(last) => (last, None),
            Err(error) if matches!(error.kind, "input_changed" | "source_changed") => {
                return Err(error);
            }
            Err(error) => (None, Some(error)),
        };
        let mut previous = None;
        let mut seen = HashSet::new();
        while let Some(row) = current {
            if !seen.insert((row.block, row.element)) {
                return Err(Error::new("query_cycle", format!("cycle in {path}")));
            }
            let prev = pointer(file, &row, "prev")?;
            if prev.as_ref().map(|r| (r.block, r.element)) != previous {
                return Err(Error::new(
                    "query_list",
                    format!("inconsistent previous link in {path}"),
                ));
            }
            previous = Some((row.block, row.element));
            rows.push(row.clone());
            current = pointer(file, &row, "next")?;
        }
        if let Some(error) = terminal {
            return Err(error);
        }
        if last.as_ref().map(|r| (r.block, r.element)) != previous {
            return Err(Error::new(
                "query_list",
                format!("inconsistent last link in {path}"),
            ));
        }
        Ok(())
    })();
    (rows, outcome.err())
}
pub(crate) fn records(file: &BlendFile, type_name: &str) -> Result<Vec<Record>> {
    let mut rows = Vec::new();
    for (index, block) in file.blocks().iter().enumerate() {
        if &block.code == b"DNA1" {
            continue;
        }
        let ti = file.schema().structs[block.sdna_index].type_index;
        if file.schema().types[ti] != type_name.as_bytes() {
            continue;
        }
        let width = file.schema().lengths[ti];
        let extent = block
            .count
            .checked_mul(width)
            .ok_or_else(|| Error::new("overflow", "query record extent"))?;
        if width == 0 || extent > block.size {
            return Err(Error::new(
                "truncated",
                format!("block {index} cannot contain its declared {type_name} records"),
            ));
        }
        for element in 0..block.count {
            rows.push(Record::new(index, element));
        }
    }
    Ok(rows)
}
pub(crate) fn id_ref(file: &BlendFile, record: &Record) -> Result<Json> {
    let id = match get(file, record, "id.name") {
        Ok(_) => record.at("id"),
        Err(e) if e.kind == "missing_field" => record.clone(),
        Err(e) => return Err(e),
    };
    let name = match get(file, &id, "name")? {
        Value::Bytes(v) => v,
        _ => return Err(Error::new("type", "ID name must be bytes")),
    };
    if name.len() < 2 {
        return Err(Error::new("type", "ID name lacks two-byte kind"));
    }
    let kind = match &name[..2] {
        b"SC" => "Scene",
        b"OB" => "Object",
        b"GR" => "Collection",
        b"MA" => "Material",
        b"ME" => "Mesh",
        b"IM" => "Image",
        b"CA" => "Camera",
        b"WO" => "World",
        b"TX" => "Text",
        b"LT" => "Lattice",
        b"LI" => "Library",
        b"NT" => match text(file, record, "idname") {
            Ok(kind) => match kind.as_str() {
                "CompositorNodeTree" => "CompositorNodeTree",
                "ShaderNodeTree" => "ShaderNodeTree",
                "GeometryNodeTree" => "GeometryNodeTree",
                _ => "NodeTree",
            },
            Err(e) if e.kind == "missing_field" => "NodeTree",
            Err(e) => return Err(e),
        },
        _ => "ID",
    };
    let library = pointer(file, &id, "lib")?
        .map(|r| {
            let raw = get(file, &r, "filepath").or_else(|e| {
                if e.kind == "missing_field" {
                    get(file, &r, "name")
                } else {
                    Err(e)
                }
            })?;
            match raw {
                Value::Bytes(bytes) => Ok(match String::from_utf8(bytes.clone()) {
                    Ok(path) => json!(path),
                    Err(_) => json!({"kind":"bytes","hex":crate::value::hex(&bytes),"utf8":null}),
                }),
                _ => Err(Error::new("type", "Library saved filepath must be bytes")),
            }
        })
        .transpose()?;
    Ok(json!({"kind":kind,"name":std::str::from_utf8(&name[2..]).ok(),"library":library}))
}
/// Python-compatible exact hexadecimal semantic binary64 value, including signed zero.
pub(crate) fn hex_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value == f64::INFINITY {
        return "inf".into();
    }
    if value == f64::NEG_INFINITY {
        return "-inf".into();
    }
    let bits = value.to_bits();
    let sign = if bits >> 63 != 0 { "-" } else { "" };
    let exp = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & 0x000f_ffff_ffff_ffff;
    if exp == 0 && frac == 0 {
        return format!("{sign}0x0.0p+0");
    }
    let e = if exp == 0 { -1022 } else { exp - 1023 };
    format!("{sign}0x{}.{frac:013x}p{e:+}", if exp == 0 { 0 } else { 1 })
}
