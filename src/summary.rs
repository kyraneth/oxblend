//! Recurrent saved-file overview, shared by CLI and in-process consumers.
//! Typed results retain bytes and float bits. JSON rendering preserves schema 1.
use crate::value::{PathStep, Value};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;

#[derive(Debug)]
#[non_exhaustive]
pub struct SummaryHeader {
    pub version: u32,
    pub pointer_size: usize,
    pub little_endian: bool,
    pub file_format: u32,
    pub size: usize,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct Warning {
    pub kind: String,
    pub block_index: usize,
    pub element: usize,
    pub field: String,
    pub message: String,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct Fps {
    pub numerator: Option<i64>,
    pub base_bits: Option<u32>,
    pub effective: Option<f64>,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct SceneSummary {
    pub name: Option<Vec<u8>>,
    pub frame_start: Option<i64>,
    pub frame_end: Option<i64>,
    pub frame_step: Option<i64>,
    pub camera: Option<Vec<u8>>,
    pub fps: Fps,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct Summary {
    pub header: SummaryHeader,
    pub block_codes: BTreeMap<Vec<u8>, usize>,
    pub objects: Vec<Vec<u8>>,
    pub scenes: Vec<SceneSummary>,
    pub materials: Vec<Vec<u8>>,
    pub meshes: Vec<Vec<u8>>,
    pub images: Vec<Vec<u8>>,
    pub collections: Vec<Vec<u8>>,
    pub warnings: Vec<Warning>,
}
fn text(file: &BlendFile, block: usize, element: usize, names: &[&str]) -> Result<Vec<u8>> {
    let steps = names
        .iter()
        .map(|s| PathStep::Field(s.as_bytes().to_vec()))
        .collect::<Vec<_>>();
    match file.get(block, element, &steps, true)? {
        Value::Bytes(b) => Ok(b),
        _ => Err(Error::new("type", "not bytes")),
    }
}
fn id_name(file: &BlendFile, block: usize, element: usize) -> Result<Vec<u8>> {
    let si = file.struct_index(block)?;
    if file.schema().types[file.schema().structs[si].type_index] == b"ID" {
        text(file, block, element, &["name"])
    } else {
        text(file, block, element, &["id", "name"])
    }
}
fn partial<T>(
    result: Result<T>,
    warnings: &mut Vec<Warning>,
    block: usize,
    element: usize,
    field: &str,
) -> Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind == "missing_field" => {
            warnings.push(Warning {
                kind: "missing_field".into(),
                block_index: block,
                element,
                field: field.into(),
                message: format!(
                    "summary block {block}, element {element}, field {field} unavailable: {}",
                    error.message
                ),
            });
            Ok(None)
        }
        Err(error) => Err(error),
    }
}
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
        Value::Signed(v) => Ok(v),
        _ => Err(Error::new(
            "type",
            format!("Scene.r.{field} not signed integer"),
        )),
    }
}
fn render_base(file: &BlendFile, block: usize, element: usize) -> Result<u32> {
    match render_value(file, block, element, "frs_sec_base")? {
        Value::Float32Bits(v) => Ok(v),
        _ => Err(Error::new("type", "Scene.r.frs_sec_base not float32")),
    }
}
/// One traversal of an already-open reader. Every result owns its data.
/// Missing fields retain warning/null semantics. Other read/type errors refuse.
pub fn summarize_typed(file: &BlendFile) -> Result<Summary> {
    let h = file.header();
    let mut s = Summary {
        header: SummaryHeader {
            version: h.version,
            pointer_size: h.pointer_size,
            little_endian: h.little_endian,
            file_format: h.file_format,
            size: h.size,
        },
        block_codes: BTreeMap::new(),
        objects: Vec::new(),
        scenes: Vec::new(),
        materials: Vec::new(),
        meshes: Vec::new(),
        images: Vec::new(),
        collections: Vec::new(),
        warnings: Vec::new(),
    };
    for (index, block) in file.blocks().iter().enumerate() {
        let code = block.code.split(|&b| b == 0).next().unwrap_or(&[]);
        *s.block_codes.entry(code.to_vec()).or_default() += 1;
        let names = match code {
            b"OB" => Some(&mut s.objects),
            b"MA" => Some(&mut s.materials),
            b"ME" => Some(&mut s.meshes),
            b"IM" => Some(&mut s.images),
            b"GR" => Some(&mut s.collections),
            _ => None,
        };
        if let Some(names) = names
            && let Some(name) = partial(
                id_name(file, index, 0),
                &mut s.warnings,
                index,
                0,
                "id.name",
            )?
        {
            names.push(name);
        }
        if code != b"SC" {
            continue;
        }
        for element in 0..block.count {
            let mut integer = |field: &str| {
                partial(
                    render_integer(file, index, element, field),
                    &mut s.warnings,
                    index,
                    element,
                    &format!("r.{field}"),
                )
            };
            let frame_end = integer("efra")?;
            let frame_start = integer("sfra")?;
            let frame_step = integer("frame_step")?;
            let numerator = integer("frs_sec")?;
            let bits = partial(
                render_base(file, index, element),
                &mut s.warnings,
                index,
                element,
                "r.frs_sec_base",
            )?;
            let base = bits.map(|b| f64::from(f32::from_bits(b)));
            let valid = base.filter(|b| b.is_finite() && *b != 0.0);
            if base.is_some() && valid.is_none() {
                s.warnings.push(Warning {
                    kind: "type".into(),
                    block_index: index,
                    element,
                    field: "r.frs_sec_base".into(),
                    message: "FPS base must be finite and nonzero; effective FPS unavailable"
                        .into(),
                });
            }
            let effective = numerator.zip(valid).map(|(n, b)| n as f64 / b);
            let camera = partial(
                file.get_pointer(index, element, &[PathStep::Field(b"camera".to_vec())])
                    .and_then(|t| t.map(|t| id_name(file, t, 0)).transpose()),
                &mut s.warnings,
                index,
                element,
                "camera.id.name",
            )?
            .flatten();
            let name = partial(
                id_name(file, index, element),
                &mut s.warnings,
                index,
                element,
                "id.name",
            )?;
            s.scenes.push(SceneSummary {
                name,
                frame_start,
                frame_end,
                frame_step,
                camera,
                fps: Fps {
                    numerator,
                    base_bits: bits,
                    effective,
                },
            });
        }
    }
    Ok(s)
}
impl Summary {
    /// Legacy schema-1 rendering remains strict UTF-8, unlike the typed result.
    pub fn json(&self) -> Result<Json> {
        fn text(b: &[u8]) -> Result<&str> {
            std::str::from_utf8(b).map_err(|_| Error::new("encoding", "invalid UTF-8"))
        }
        fn names(v: &[Vec<u8>]) -> Result<Vec<&str>> {
            v.iter().map(|b| text(b)).collect()
        }
        let codes = self
            .block_codes
            .iter()
            .map(|(b, n)| {
                std::str::from_utf8(b)
                    .map(|b| (b, *n))
                    .map_err(|_| Error::new("encoding", "block code UTF-8"))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let scenes=self.scenes.iter().map(|s| {
            let base=s.fps.base_bits.map(|b| f64::from(f32::from_bits(b))).filter(|b| b.is_finite());
            Ok(json!({"name":s.name.as_deref().map(text).transpose()?,"frame_end":s.frame_end,"camera":s.camera.as_deref().map(text).transpose()?,"frame_start":s.frame_start,"frame_step":s.frame_step,"fps":{"numerator":s.fps.numerator,"base":base,"effective":s.fps.effective,"base_bits_hex":s.fps.base_bits.map(|b| format!("{b:08x}"))}}))
        }).collect::<Result<Vec<_>>>()?;
        let h = &self.header;
        Ok(
            json!({"schema":1,"header":{"version":h.version,"pointer_size":h.pointer_size,"little_endian":h.little_endian,"file_format":h.file_format},"block_codes":codes,"objects":names(&self.objects)?,"scenes":scenes,"materials":names(&self.materials)?,"meshes":names(&self.meshes)?,"images":names(&self.images)?,"collections":names(&self.collections)?,"warnings":self.warnings.iter().map(|w| json!({"kind":w.kind,"block_index":w.block_index,"element":w.element,"field":w.field,"message":w.message})).collect::<Vec<_>>() }),
        )
    }
}
/// Backwards-compatible Rust/CLI JSON entry point.
pub fn summarize(file: &BlendFile) -> Result<Json> {
    summarize_typed(file)?.json()
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
