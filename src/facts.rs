//! Per-file serialized facts only. Callers own scene traversal and linked resolution.
//! Per-file collection, object and library facts from the input schema.
use crate::value::{PathStep, Value, hex};
use crate::{BlendFile, Error, Result};
use serde_json::{Map, Value as Json, json};
use std::collections::{HashMap, HashSet};

impl BlendFile {
    pub fn dna_type_id(&self, block: usize) -> Result<&[u8]> {
        let si = self.struct_index(block)?;
        let structure = self
            .schema()
            .structs
            .get(si)
            .ok_or_else(|| Error::new("invalid_sdna", "structure index"))?;
        Ok(&self.schema().types[structure.type_index])
    }
    pub fn id_name(&self, block: usize) -> Result<Option<Vec<u8>>> {
        match self.bytes_field(block, &["id", "name"]) {
            Ok(name) => Ok(Some(name)),
            Err(e) if e.kind == "missing_field" => Ok(None),
            Err(e) => Err(e),
        }
    }
    pub fn bytes_field(&self, block: usize, names: &[&str]) -> Result<Vec<u8>> {
        match self.get(block, 0, &steps(names), true)? {
            Value::Bytes(v) => Ok(v),
            _ => Err(Error::new("type", "expected char array")),
        }
    }
    pub fn pointer_field(&self, block: usize, names: &[&str]) -> Result<Option<usize>> {
        self.get_pointer(block, 0, &steps(names))
    }
}
fn steps(names: &[&str]) -> Vec<PathStep> {
    names
        .iter()
        .map(|n| PathStep::Field(n.as_bytes().to_vec()))
        .collect()
}
fn text(data: &[u8]) -> Result<String> {
    std::str::from_utf8(data)
        .map(str::to_owned)
        .map_err(|_| Error::new("encoding", "fact code/type is not UTF-8"))
}
fn code(data: &[u8; 4]) -> &[u8] {
    let end = data.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    &data[..end]
}
struct Closure<'a> {
    file: &'a BlendFile,
    source: bool,
    blocks: Map<String, Json>,
    pending: Vec<usize>,
    addresses: HashMap<usize, String>,
}
impl<'a> Closure<'a> {
    fn visit(&mut self, index: Option<usize>) -> Result<Json> {
        let Some(index) = index else {
            return Ok(Json::Null);
        };
        if let Some(address) = self.addresses.get(&index) {
            return Ok(json!(address));
        }
        let b = self.file.block(index)?;
        // Facts are address-keyed: never collapse an ambiguity, even for indexed IDs.
        if b.old_address == 0 || self.file.dereference(b.old_address)? != Some(index) {
            return Err(Error::new(
                "ambiguous_pointer",
                "fact block has no unique nonzero address",
            ));
        }
        let address = b.old_address.to_string();
        let block_code = code(&b.code);
        let name = if block_code == b"ID" {
            Some(self.file.bytes_field(index, &["name"])?)
        } else {
            self.file.id_name(index)?
        };
        let mut value = json!({"code":text(block_code)?, "dna":text(self.file.dna_type_id(index)?)?,
            "name":name.map_or_else(String::new, |v| hex(&v)), "pointers":{}});
        if self.source {
            value["fields"] = json!({});
        }
        self.blocks.insert(address.clone(), value);
        self.addresses.insert(index, address.clone());
        self.pending.push(index);
        Ok(json!(address))
    }
    fn finish(&mut self) -> Result<()> {
        while let Some(index) = self.pending.pop() {
            let block_code = code(&self.file.block(index)?.code);
            let kind = self.file.dna_type_id(index)?;
            let fields: &[&[&str]] = match (block_code, kind, self.source) {
                (b"ID", _, false) => &[],
                (b"ID", _, true) => &[&["lib"]],
                (_, b"Collection", _) => &[&["children", "first"], &["gobject", "first"]],
                (_, b"Object", _) => &[&["parent"]],
                (_, b"CollectionChild", _) => &[&["next"], &["collection"]],
                (_, b"CollectionObject", _) => &[&["next"], &["ob"]],
                (_, b"Scene", true) => &[&["master_collection"]],
                (b"LI", b"Library", true) => &[], // bpy Library.parent is not a required serialized field.
                _ => {
                    return Err(Error::new(
                        "unsupported_type",
                        "outside per-file collection facts closure",
                    ));
                }
            };
            let address = self
                .addresses
                .get(&index)
                .ok_or_else(|| Error::new("invalid_block", "closure address"))?
                .clone();
            for path in fields {
                let target = self.file.pointer_field(index, path)?;
                let reference = self.visit(target)?;
                self.blocks
                    .get_mut(&address)
                    .ok_or_else(|| Error::new("invalid_block", "closure block"))?["pointers"]
                    [path.join(".")] = reference;
            }
            if self.source && block_code == b"LI" {
                let path = self.file.bytes_field(index, &["name"])?;
                self.blocks
                    .get_mut(&address)
                    .ok_or_else(|| Error::new("invalid_block", "library block"))?["fields"]["name"] =
                    json!({"kind":"bytes", "hex":hex(&path)});
            }
        }
        Ok(())
    }
    fn roots(&mut self) -> Result<()> {
        let objects: Vec<String> = self
            .blocks
            .iter()
            .filter(|(_, v)| v["dna"] == "Object")
            .map(|(k, _)| k.clone())
            .collect();
        for address in objects {
            let mut current = address.clone();
            let mut seen = HashSet::new();
            while seen.insert(current.clone()) {
                let value = self
                    .blocks
                    .get(&current)
                    .ok_or_else(|| Error::new("dangling_pointer", "root closure"))?;
                let parent = value["pointers"]["parent"].as_str();
                if parent.is_none() || value["code"] == "ID" {
                    break;
                }
                current = parent
                    .ok_or_else(|| Error::new("type", "parent address"))?
                    .to_owned();
            }
            let root = self
                .blocks
                .get(&current)
                .ok_or_else(|| Error::new("dangling_pointer", "root block"))?;
            if root["code"] != "ID" {
                let root_address = current
                    .parse::<u64>()
                    .map_err(|_| Error::new("type", "root address"))?;
                let index = self
                    .file
                    .dereference(root_address)?
                    .ok_or_else(|| Error::new("invalid_block", "null root index"))?;
                let name = self
                    .file
                    .id_name(index)?
                    .ok_or_else(|| Error::new("missing_field", "root ID name"))?;
                let body = name.get(2..).unwrap_or_default();
                self.blocks
                    .get_mut(&address)
                    .ok_or_else(|| Error::new("invalid_block", "object block"))?["root"] =
                    json!(String::from_utf8_lossy(body));
            }
        }
        Ok(())
    }
}
/// Exact existing per-file library-facts semantics, no cross-file IO.
pub fn library_facts(file: &BlendFile) -> Result<Json> {
    extract(file, false)
}
/// Current GLOB/scene/master closure with byte-safe LI paths and ID.lib references.
/// Python still owns ordered traversal and cross-file resolution.
pub fn source_facts(file: &BlendFile) -> Result<Json> {
    extract(file, true)
}
fn extract(file: &BlendFile, source: bool) -> Result<Json> {
    let mut closure = Closure {
        file,
        source,
        blocks: Map::new(),
        pending: Vec::new(),
        addresses: HashMap::new(),
    };
    let mut indexes = Map::new();
    if !source {
        for wanted in [b"GR".as_slice(), b"OB".as_slice()] {
            let mut index = Map::new();
            for (i, _) in file
                .blocks()
                .iter()
                .enumerate()
                .filter(|(_, b)| code(&b.code) == wanted)
            {
                let name = file
                    .id_name(i)?
                    .ok_or_else(|| Error::new("missing_field", "indexed ID name"))?;
                index.insert(hex(&name), closure.visit(Some(i))?);
            }
            indexes.insert(text(wanted)?, Json::Object(index));
        }
    }
    let mut entry = None;
    if source {
        // GLOB is selected by file order, not by dereferencing its metadata address.
        // Keep it separate: metadata addresses can be zero or reused. Scene/list/ID
        // references still use strict unique old-address lookup at the reader boundary.
        let glob = file
            .blocks()
            .iter()
            .position(|b| code(&b.code) == b"GLOB")
            .ok_or_else(|| Error::new("missing_field", "GLOB current scene"))?;
        if file.dna_type_id(glob)? != b"FileGlobal" {
            return Err(Error::new("unsupported_type", "GLOB is not FileGlobal"));
        }
        let scene = file.pointer_field(glob, &["curscene"])?;
        entry = Some(
            json!({"address":file.block(glob)?.old_address.to_string(), "block_index":glob,
            "value":{"code":"GLOB", "dna":"FileGlobal", "name":"", "fields":{}, "pointers":{"curscene":closure.visit(scene)?}}}),
        );
    }
    closure.finish()?;
    if !source {
        closure.roots()?;
    }
    let mut result = json!({"blocks":closure.blocks, "index":indexes});
    if let Some(entry) = entry {
        result["entry"] = entry;
    }
    Ok(result)
}
