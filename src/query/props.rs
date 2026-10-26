//! Shared typed decoder for explicitly saved user and system IDProperties.
//! Minimal discriminants are authored from Blender v5.2.1 DNA_ID_enums.h and
//! DNA_ID.h; layout reads use the input's own checked SDNA, never C offsets.
//! Native semantic qualification uses saved/reopened Blender 5.2.1 fixtures.
//! Storage-subtype and older-version qualification remain separate gates.

use crate::query::access::{Record, get, hex_float, id_ref, list, pointer};
use crate::{BlendFile, Error, Result};
use serde_json::{Map, Value as Json, json};
use std::collections::HashSet;

// Minimal saved discriminants, see module docs (Blender v5.2.1 DNA_ID_enums.h).
const T_STRING: i64 = 0;
const T_INT: i64 = 1;
const T_FLOAT: i64 = 2;
const T_ARRAY: i64 = 5;
const T_GROUP: i64 = 6;
const T_ID: i64 = 7;
const T_DOUBLE: i64 = 8;
const T_IDPARRAY: i64 = 9;
const T_BOOL: i64 = 10;
const SUB_UTF8: i64 = 0;
const SUB_BYTE: i64 = 1;

fn prop_int(file: &BlendFile, prop: &Record, field: &str) -> Result<i64> {
    crate::query::access::int(file, prop, field).map_err(|e| {
        Error::new(
            e.kind,
            format!("IDProperty {}.{}: {}", prop.block, field, e.message),
        )
    })
}

fn prop_name(file: &BlendFile, prop: &Record) -> Result<String> {
    let raw = match get(file, prop, "name")? {
        crate::value::Value::Bytes(v) => v,
        _ => return Err(Error::new("type", "IDProperty.name is not bytes")),
    };
    // Empty saved keys remain keys; duplicate keys are checked by the group.
    String::from_utf8(raw.clone()).map_err(|_| {
        Error::new(
            "encoding",
            format!(
                "IDProperty block {} non-UTF8 key bytes {}",
                prop.block,
                crate::value::hex(&raw)
            ),
        )
    })
}

fn hex_bytes(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

fn read_f64_le(data: &[u8], little: bool) -> f64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&data[..8]);
    if !little {
        b.reverse();
    }
    f64::from_bits(u64::from_le_bytes(b))
}

fn read_f32_to_f64(data: &[u8], little: bool) -> f64 {
    let mut b = [0u8; 4];
    b.copy_from_slice(&data[..4]);
    if !little {
        b.reverse();
    }
    f64::from(f32::from_bits(u32::from_le_bytes(b)))
}

fn read_i32(data: &[u8], little: bool) -> i64 {
    let mut b = [0u8; 4];
    b.copy_from_slice(&data[..4]);
    if !little {
        b.reverse();
    }
    i32::from_le_bytes(b) as i64
}

fn payload_of(file: &BlendFile, target: &Record) -> Result<Vec<u8>> {
    Ok(file.payload(target.block)?.to_vec())
}

fn decode_one(
    file: &BlendFile,
    prop: &Record,
    active: &mut HashSet<(usize, usize)>,
) -> Result<Json> {
    if !active.insert((prop.block, prop.element)) {
        return Err(Error::new(
            "query_cycle",
            format!(
                "IDProperty cycle at block {} element {}",
                prop.block, prop.element
            ),
        ));
    }
    let result = decode_inner(file, prop, active);
    active.remove(&(prop.block, prop.element));
    result
}

fn decode_inner(
    file: &BlendFile,
    prop: &Record,
    active: &mut HashSet<(usize, usize)>,
) -> Result<Json> {
    let ty = prop_int(file, prop, "type")?;
    let sub = prop_int(file, prop, "subtype")?;
    let len = prop_int(file, prop, "len")?;
    if len < 0 {
        return Err(Error::new(
            "type",
            format!("IDProperty block {} negative len", prop.block),
        ));
    }
    let len_us = len as usize;
    match ty {
        T_STRING => {
            if sub != SUB_UTF8 && sub != SUB_BYTE {
                return Err(Error::new(
                    "unverified_semantics",
                    format!("IDProperty string subtype {sub} unqualified"),
                ));
            }
            if len_us == 0 {
                // BYTE empty is legal (zero-length raw); UTF8 len 0 would miss
                // its NUL terminator, so only BYTE may be empty here.
                if sub != SUB_BYTE {
                    return Err(Error::new(
                        "pointer_extent",
                        "UTF8 string len 0 misses NUL terminator",
                    ));
                }
                return Ok(json!({"kind":"bytes","hex":""}));
            }
            let target = pointer(file, prop, "data.pointer")?
                .ok_or_else(|| Error::new("dangling_pointer", "string data pointer is null"))?;
            let bytes = payload_of(file, &target)?;
            if bytes.len() != len_us {
                return Err(Error::new(
                    "pointer_extent",
                    format!("string allocation {} != len {len}", bytes.len()),
                ));
            }
            if sub == SUB_BYTE {
                return Ok(json!({"kind":"bytes","hex":hex_bytes(&bytes)}));
            }
            // UTF8: len includes NUL; hex excludes terminator (oracle shape).
            if bytes.last() != Some(&0) {
                return Err(Error::new(
                    "pointer_extent",
                    "UTF8 string lacks NUL terminator",
                ));
            }
            if bytes[..len_us - 1].contains(&0) {
                return Err(Error::new("pointer_extent", "UTF8 string interior NUL"));
            }
            let body = &bytes[..len_us - 1];
            let hex = hex_bytes(body);
            match String::from_utf8(body.to_vec()) {
                Ok(s) => Ok(json!({"kind":"string","utf8":s,"hex":hex})),
                Err(_) => Ok(json!({"kind":"string","utf8":null,"hex":hex})),
            }
        }
        T_INT => {
            if sub != 0 {
                return Err(Error::new(
                    "unverified_semantics",
                    format!("IDP_INT subtype {sub} unqualified"),
                ));
            }
            let v = prop_int(file, prop, "data.val")?;
            // i32 range is guaranteed by SDNA (int field); keep exact decimal.
            Ok(json!({"kind":"integer","decimal":v.to_string()}))
        }
        T_FLOAT => {
            if sub != 0 {
                return Err(Error::new(
                    "unverified_semantics",
                    format!("IDP_FLOAT subtype {sub} unqualified"),
                ));
            }
            let bits = prop_int(file, prop, "data.val")? as i32 as u32;
            let f = f64::from(f32::from_bits(bits));
            Ok(json!({"kind":"real","hex":hex_float(f)}))
        }
        T_DOUBLE => {
            if sub != 0 {
                return Err(Error::new(
                    "unverified_semantics",
                    format!("IDP_DOUBLE subtype {sub} unqualified"),
                ));
            }
            let lo = prop_int(file, prop, "data.val")? as i32 as u32 as u64;
            let hi = prop_int(file, prop, "data.val2")? as i32 as u32 as u64;
            let bits = (hi << 32) | lo;
            let f = f64::from_bits(bits);
            Ok(json!({"kind":"real","hex":hex_float(f)}))
        }
        T_BOOL => {
            if sub != 0 {
                return Err(Error::new(
                    "unverified_semantics",
                    format!("IDP_BOOLEAN subtype {sub} unqualified"),
                ));
            }
            let v = prop_int(file, prop, "data.val")?;
            if v != 0 && v != 1 {
                return Err(Error::new("type", format!("boolean value {v} is not 0/1")));
            }
            Ok(json!({"kind":"boolean","value":v == 1}))
        }
        T_ID => {
            if sub != 0 {
                return Err(Error::new(
                    "unverified_semantics",
                    format!("IDP_ID subtype {sub} unqualified"),
                ));
            }
            match pointer(file, prop, "data.pointer")? {
                None => Ok(json!({"kind":"null_id","target":null})),
                Some(target) => {
                    let r = id_ref(file, &target)?;
                    Ok(json!({"kind":"id","target":r}))
                }
            }
        }
        T_GROUP => {
            if sub != 0 {
                return Err(Error::new(
                    "unverified_semantics",
                    format!("IDP_GROUP subtype {sub} unqualified"),
                ));
            }
            let children = list(file, prop, "data.group")?;
            let mut entries = Map::new();
            for child in children {
                let key = prop_name(file, &child)?;
                if key == "_RNA_UI" {
                    continue; // UI metadata is never a user value.
                }
                if entries.contains_key(&key) {
                    return Err(Error::new(
                        "property_collision",
                        format!("duplicate IDProperty key {key:?}"),
                    ));
                }
                let value = decode_one(file, &child, active)?;
                entries.insert(key, value);
            }
            Ok(json!({"kind":"group","entries":entries}))
        }
        T_ARRAY => {
            let (width, typecode) = match sub {
                1 => (4usize, "i"),
                2 => (4usize, "f"),
                8 => (8usize, "d"),
                10 => (1usize, "b"),
                6 => {
                    return Err(Error::new(
                        "unprofiled_role",
                        "pointer-bearing property table requires relocation profile",
                    ));
                }
                _ => {
                    return Err(Error::new(
                        "unverified_semantics",
                        format!("IDP_ARRAY subtype {sub} unqualified"),
                    ));
                }
            };
            if len_us == 0 {
                return Ok(json!({"kind":"array","typecode":typecode,"items":[]}));
            }
            let target = pointer(file, prop, "data.pointer")?
                .ok_or_else(|| Error::new("dangling_pointer", "array data pointer is null"))?;
            let bytes = payload_of(file, &target)?;
            let expect = len_us
                .checked_mul(width)
                .ok_or_else(|| Error::new("overflow", "array extent"))?;
            if bytes.len() != expect {
                return Err(Error::new(
                    "pointer_extent",
                    format!("array allocation {} != {expect}", bytes.len()),
                ));
            }
            let little = file.header().little_endian;
            let mut items = Vec::with_capacity(len_us);
            for i in 0..len_us {
                let off = i * width;
                let item = match sub {
                    1 => {
                        let v = read_i32(&bytes[off..off + 4], little);
                        json!({"kind":"integer","decimal":v.to_string()})
                    }
                    2 => {
                        let f = read_f32_to_f64(&bytes[off..off + 4], little);
                        json!({"kind":"real","hex":hex_float(f)})
                    }
                    8 => {
                        let f = read_f64_le(&bytes[off..off + 8], little);
                        // Preserve signed zero/nonfinite bit-exactness via hex.
                        json!({"kind":"real","hex":hex_float(f)})
                    }
                    10 => {
                        let b = bytes[off];
                        if b != 0 && b != 1 {
                            return Err(Error::new(
                                "type",
                                format!("boolean array element {b} is not 0/1"),
                            ));
                        }
                        json!({"kind":"boolean","value":b == 1})
                    }
                    _ => unreachable!(),
                };
                items.push(item);
            }
            Ok(json!({"kind":"array","typecode":typecode,"items":items}))
        }
        T_IDPARRAY => {
            if sub != 0 {
                return Err(Error::new(
                    "unverified_semantics",
                    format!("IDP_IDPARRAY subtype {sub} unqualified"),
                ));
            }
            if len_us == 0 {
                return Ok(json!({"kind":"idproperty_array","items":[]}));
            }
            let target = pointer(file, prop, "data.pointer")?
                .ok_or_else(|| Error::new("dangling_pointer", "IDPARRAY data pointer is null"))?;
            // Target must be IDProperty records; count must cover len.
            let si = file.struct_index(target.block)?;
            let ty = &file.schema().types[file.schema().structs[si].type_index];
            if ty != b"IDProperty" {
                return Err(Error::new("type", "IDPARRAY target is not IDProperty"));
            }
            let count = file.block(target.block)?.count;
            if len_us > count {
                return Err(Error::new(
                    "pointer_extent",
                    "IDPARRAY len exceeds saved records",
                ));
            }
            let mut items = Vec::with_capacity(len_us);
            for i in 0..len_us {
                let elem = Record::new(target.block, target.element + i);
                // Element bounds are proven by count above; locate via decode.
                items.push(decode_one(file, &elem, active)?);
            }
            Ok(json!({"kind":"idproperty_array","items":items}))
        }
        _ => Err(Error::new(
            "unverified_semantics",
            format!("IDProperty type {ty} subtype {sub} unqualified"),
        )),
    }
}

fn decode_store_entries(
    file: &BlendFile,
    store: Option<Record>,
    active: &mut HashSet<(usize, usize)>,
) -> Result<Map<String, Json>> {
    let Some(group) = store else {
        return Ok(Map::new());
    };
    let ty = prop_int(file, &group, "type")?;
    if ty != T_GROUP {
        return Err(Error::new(
            "type",
            format!("property store type {ty} is not GROUP"),
        ));
    }
    let children = list(file, &group, "data.group")?;
    let mut entries = Map::new();
    for child in children {
        let key = prop_name(file, &child)?;
        if key == "_RNA_UI" {
            continue;
        }
        if entries.contains_key(&key) {
            return Err(Error::new(
                "property_collision",
                format!("duplicate IDProperty key {key:?}"),
            ));
        }
        entries.insert(key, decode_one(file, &child, active)?);
    }
    Ok(entries)
}

/// Decode one owner's two checked stores.
///
/// `user_path` is e.g. `id.properties` (Scene/Object/Collection) or
/// `id_properties` (ViewLayer); `system_path` is the optional second store
/// (`id.system_properties` / `system_properties`). Missing system field is
/// empty (pre-5.x owners); missing user store with no system support is
/// `owner_not_idproperty_capable`. A present non-group root is malformed and
/// refuses; unknown future types refuse with owner/path/type, never coerce.
pub fn decode_owner(
    file: &BlendFile,
    record: &Record,
    user_path: &str,
    system_path: Option<&str>,
) -> Result<Json> {
    let user = match pointer(file, record, user_path) {
        Ok(v) => v,
        Err(e) if e.kind == "missing_field" => {
            // No user store field: only capable if a system store exists.
            let sys = match system_path {
                Some(sp) => match pointer(file, record, sp) {
                    Ok(v) => v,
                    Err(se) if se.kind == "missing_field" => None,
                    Err(se) => return Err(se),
                },
                None => None,
            };
            if sys.is_none() {
                return Ok(json!({"status":"unavailable","kind":"owner_not_idproperty_capable"}));
            }
            None
        }
        Err(e) => return Err(e),
    };
    let system = match system_path {
        Some(sp) => match pointer(file, record, sp) {
            Ok(v) => v,
            Err(e) if e.kind == "missing_field" => None, // pre-5.x: no second store.
            Err(e) => return Err(e),
        },
        None => None,
    };
    // Both stores absent (null pointers) is available-empty, not missing.
    let mut active = HashSet::new();
    let entries = decode_store_entries(file, user, &mut active)?;
    let mut active2 = HashSet::new();
    let system_entries = decode_store_entries(file, system, &mut active2)?;
    Ok(json!({"status":"available","entries":entries,"system_entries":system_entries}))
}

fn is_local(file: &BlendFile, record: &Record) -> Result<bool> {
    Ok(id_ref(file, record)?["library"].is_null())
}

/// Whole `id_properties` column for one Scene row.
///
/// Owns the Scene properties, all root-local Object/Collection owner records
/// (including unlinked IDs; linked stubs excluded, never assigned), and
/// scene-scoped ViewLayer properties. Node/tree properties colocate with the
/// compositor owner and are not duplicated here.
pub fn scene(file: &BlendFile, record: &Record) -> Result<Json> {
    let scene_props = decode_owner(file, record, "id.properties", Some("id.system_properties"))?;
    if scene_props.get("status") != Some(&json!("available")) {
        return Err(Error::new(
            "unverified_semantics",
            "Scene IDProperty store unavailable",
        ));
    }
    // Root-local owners: every local record, sorted by display ref for stable
    // output (acceptance also normalizes presentation sets by ref).
    let mut objects = Vec::new();
    for r in crate::query::access::records(file, "Object")? {
        if !is_local(file, &r)? {
            continue;
        }
        let owner = id_ref(file, &r)?;
        let properties = decode_owner(file, &r, "id.properties", Some("id.system_properties"))?;
        objects.push(json!({"owner":owner,"properties":properties}));
    }
    objects.sort_by(|a, b| {
        serde_json::to_string(&a["owner"])
            .unwrap_or_default()
            .cmp(&serde_json::to_string(&b["owner"]).unwrap_or_default())
    });
    // Embedded master collections are not in the file-global collection ID
    // inventory. Include this Scene's master, never another Scene's master.
    let own_master = pointer(file, record, "master_collection")?.map(|r| (r.block, r.element));
    let mut foreign_masters = HashSet::new();
    for scene in crate::query::access::records(file, "Scene")? {
        if let Some(master) = pointer(file, &scene, "master_collection")? {
            let identity = (master.block, master.element);
            if Some(identity) != own_master {
                foreign_masters.insert(identity);
            }
        }
    }
    let mut collections = Vec::new();
    for r in crate::query::access::records(file, "Collection")? {
        if foreign_masters.contains(&(r.block, r.element)) || !is_local(file, &r)? {
            continue;
        }
        let owner = id_ref(file, &r)?;
        let properties = decode_owner(file, &r, "id.properties", Some("id.system_properties"))?;
        collections.push(json!({"owner":owner,"properties":properties}));
    }
    collections.sort_by(|a, b| {
        serde_json::to_string(&a["owner"])
            .unwrap_or_default()
            .cmp(&serde_json::to_string(&b["owner"]).unwrap_or_default())
    });
    let mut layers = Vec::new();
    for v in list(file, record, "view_layers")? {
        let name = match get(file, &v, "name")? {
            crate::value::Value::Bytes(v) => {
                String::from_utf8(v).map_err(|_| Error::new("encoding", "ViewLayer name"))?
            }
            _ => return Err(Error::new("type", "ViewLayer name")),
        };
        let properties = decode_owner(file, &v, "id_properties", Some("system_properties"))?;
        layers.push(json!({"name":name,"properties":properties}));
    }
    Ok(
        json!({"scene":scene_props,"objects":objects,"collections":collections,"view_layers":layers}),
    )
}
