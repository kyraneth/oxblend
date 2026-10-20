//! Recurrent saved-file overview, shared by CLI and in-process consumers.
//! No Blender evaluation or active-Scene selection is implied.
use crate::value::{PathStep, Value};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;

fn text(file: &BlendFile, block: usize, element: usize, names: &[&str]) -> Result<String> {
    let steps: Vec<_> = names
        .iter()
        .map(|s| PathStep::Field(s.as_bytes().to_vec()))
        .collect();
    Ok(file.get(block, element, &steps, true)?.utf8()?.to_owned())
}
fn id_name(file: &BlendFile, block: usize, element: usize) -> Result<String> {
    let si = file.struct_index(block)?;
    if file.schema().types[file.schema().structs[si].type_index] == b"ID" {
        text(file, block, element, &["name"])
    } else {
        text(file, block, element, &["id", "name"])
    }
}
fn partial<T>(
    result: Result<T>,
    warnings: &mut Vec<Json>,
    block: usize,
    element: usize,
    field: &str,
) -> Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind == "missing_field" => {
            warnings.push(json!({"kind":"missing_field", "block_index":block,
                "element":element, "field":field,
                "message":format!("summary block {block}, element {element}, field {field} unavailable: {}", error.message)}));
            Ok(None)
        }
        Err(error) => Err(error),
    }
}
// One checked evaluator for the timing fields; callers need not decode SDNA or floats.
fn render_value(file: &BlendFile, block: usize, element: usize, field: &str) -> Result<Value> {
    file.get(
        block,
        element,
        &[
            PathStep::Field(b"r".to_vec()),
            PathStep::Field(field.as_bytes().to_vec()),
        ],
        true,
    )
}
fn render_integer(file: &BlendFile, block: usize, element: usize, field: &str) -> Result<i64> {
    match render_value(file, block, element, field)? {
        Value::Signed(value) => Ok(value),
        _ => Err(Error::new(
            "type",
            format!("Scene.r.{field} not signed integer"),
        )),
    }
}
fn render_base(file: &BlendFile, block: usize, element: usize) -> Result<u32> {
    match render_value(file, block, element, "frs_sec_base")? {
        Value::Float32Bits(bits) => Ok(bits),
        _ => Err(Error::new("type", "Scene.r.frs_sec_base not float32")),
    }
}

/// Summarize one already-open reader. The owned schema-1 JSON outlives the reader.
///
/// Every Scene element is returned in saved block/element order. Missing fields
/// retain warning/null semantics; other read/type errors refuse. FPS is nominal
/// numerator divided by the exact saved float32 base, not an evaluated timebase.
/// No file is reopened and no dependency closure is traversed.
pub fn summarize(file: &BlendFile) -> Result<Json> {
    let mut codes = BTreeMap::<String, usize>::new();
    let mut objects = Vec::new();
    let mut materials = Vec::new();
    let mut meshes = Vec::new();
    let mut images = Vec::new();
    let mut collections = Vec::new();
    let mut warnings = Vec::new();
    let mut scenes = Vec::new();
    for (index, block) in file.blocks().iter().enumerate() {
        let code = block.code.split(|&b| b == 0).next().unwrap_or(&[]);
        let code =
            std::str::from_utf8(code).map_err(|_| Error::new("encoding", "block code UTF-8"))?;
        *codes.entry(code.to_owned()).or_default() += 1;
        let names = match code {
            "OB" => Some(&mut objects),
            "MA" => Some(&mut materials),
            "ME" => Some(&mut meshes),
            "IM" => Some(&mut images),
            "GR" => Some(&mut collections),
            _ => None,
        };
        if let Some(names) = names
            && let Some(name) =
                partial(id_name(file, index, 0), &mut warnings, index, 0, "id.name")?
        {
            names.push(name);
        }
        if code != "SC" {
            continue;
        }
        for element in 0..block.count {
            let mut integer = |field: &str| {
                partial(
                    render_integer(file, index, element, field),
                    &mut warnings,
                    index,
                    element,
                    &format!("r.{field}"),
                )
            };
            // Preserve existing frame_end warning ordering before added fields.
            let frame_end = integer("efra")?;
            let frame_start = integer("sfra")?;
            let frame_step = integer("frame_step")?;
            let numerator = integer("frs_sec")?;
            let bits = partial(
                render_base(file, index, element),
                &mut warnings,
                index,
                element,
                "r.frs_sec_base",
            )?;
            let base = bits.map(|bits| f64::from(f32::from_bits(bits)));
            let valid_base = base.filter(|base| base.is_finite() && *base != 0.0);
            if base.is_some() && valid_base.is_none() {
                warnings.push(json!({"kind":"type", "block_index":index, "element":element,
                    "field":"r.frs_sec_base", "message":"FPS base must be finite and nonzero; effective FPS unavailable"}));
            }
            let effective = numerator.zip(valid_base).map(|(n, b)| n as f64 / b);
            let camera = partial(
                file.get_pointer(index, element, &[PathStep::Field(b"camera".to_vec())])
                    .and_then(|target| target.map(|target| id_name(file, target, 0)).transpose()),
                &mut warnings,
                index,
                element,
                "camera.id.name",
            )?
            .flatten();
            let name = partial(
                id_name(file, index, element),
                &mut warnings,
                index,
                element,
                "id.name",
            )?;
            scenes.push(json!({"name":name, "frame_end":frame_end, "camera":camera,
                "frame_start":frame_start, "frame_step":frame_step,
                "fps":{"numerator":numerator, "base":base.filter(|v| v.is_finite()),
                    "effective":effective, "base_bits_hex":bits.map(|v| format!("{v:08x}"))}}));
        }
    }
    let header = file.header();
    Ok(
        json!({"schema":1, "header":{"version":header.version, "pointer_size":header.pointer_size,
        "little_endian":header.little_endian, "file_format":header.file_format},
        "block_codes":codes, "objects":objects, "scenes":scenes, "materials":materials,
        "meshes":meshes, "images":images, "collections":collections, "warnings":warnings}),
    )
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
