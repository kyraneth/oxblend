//! Saved view-layer projection: ordered layers with pass flags, the full
//! layer-collection tree, and collection/object own controls with memberships.
//!
//! Basis: saved controls only (`saved_controls_not_evaluated_visibility`).
//! This module never computes evaluated visibility, render presence, depsgraph
//! bases or ancestor OR summaries, and never names a field after them. An
//! object with several collection paths appears once with every membership;
//! excluded branches stay in the tree; disabled layers stay in `layers`.
//!
//! Qualified profile: files saved by Blender 5.2.1 (SDNA-MATRIX header version
//! 502, `FileGlobal.subversion` 44). The caller owns the version gate; this
//! module still refuses explicitly when a needed saved structure is absent
//! instead of guessing (`unsupported_type`, `invalid_saved_structure`).
//!
//! Flag-bit evidence. Bit meanings are pinned from the primary Blender source
//! (github.com/blender/blender, branch main, retrieved 2026-10-23) and were
//! verified against saved bytes of the synthetic 5.2.1 fixture
//! (`expected/synthetic-5.2.json`, fixture sha256 633017ea4bb2…) plus nine
//! further resaved-5.2 qualification files with zero comparator diffs
//! (`tools/model_check.py`, 12 scene projections). References:
//! - `DNA_layer_types.h`: `eViewLayer_Flag`, `eLayerCollection_Flag`,
//!   `eViewLayerCryptomatteFlags`, `eViewLayerGreasePencilFlags`
//! - `DNA_scene_enums.h`: `eSceneLayer_Flag` (`layflag`), `eScenePassType`
//!   (`passflag`)
//! - `DNA_collection_types.h`: `eCollection_Flag`
//! - `DNA_object_types.h`: `eObject_VisibilityFlag` — in this profile merged
//!   into the single `Object.restrictflag` word (the 5.2 struct has no
//!   separate `visibility_flag` member; fixture bytes confirm bits 0/2/8).
//!
//! These are authored semantic facts, not copied engine tables.

use crate::query::access::{self, Record};
use crate::{BlendFile, Error, Result};
use serde_json::{Map, Value as Json, json};
use std::collections::{HashMap, HashSet};

/// Saved SDNA structures this projection requires; absence is explicit.
const REQUIRED_TYPES: [&str; 6] = [
    "ViewLayer",
    "LayerCollection",
    "Collection",
    "CollectionChild",
    "CollectionObject",
    "Object",
];

/// `ViewLayer.flag` (short) layer switches.
const VIEW_LAYER_FLAG_BITS: [(&str, i64); 2] = [("use", 1 << 0), ("use_freestyle", 1 << 2)];
/// `ViewLayer.layflag` (int) render-category switches. Legacy bits 1..=6 and
/// the historical halo/edge/xray bits are unused by this profile and unmapped.
const VIEW_LAYER_LAYFLAG_BITS: [(&str, i64); 7] = [
    ("use_solid", 1 << 0),
    ("use_sky", 1 << 4),
    ("use_strand", 1 << 5),
    ("use_ao", 1 << 7),
    ("use_volumes", 1 << 8),
    ("use_motion_blur", 1 << 9),
    ("use_grease_pencil", 1 << 10),
];
/// `ViewLayer.passflag` (int) per-pass enables (`eScenePassType`).
const VIEW_LAYER_PASSFLAG_BITS: [(&str, i64); 25] = [
    ("use_pass_combined", 1 << 0),
    ("use_pass_z", 1 << 1),
    ("use_pass_shadow", 1 << 5),
    ("use_pass_ambient_occlusion", 1 << 6),
    ("use_pass_position", 1 << 7),
    ("use_pass_normal", 1 << 8),
    ("use_pass_vector", 1 << 9),
    ("use_pass_object_index", 1 << 11),
    ("use_pass_uv", 1 << 12),
    ("use_pass_mist", 1 << 14),
    ("use_pass_emit", 1 << 16),
    ("use_pass_environment", 1 << 17),
    ("use_pass_material_index", 1 << 18),
    ("use_pass_diffuse_direct", 1 << 19),
    ("use_pass_diffuse_indirect", 1 << 20),
    ("use_pass_diffuse_color", 1 << 21),
    ("use_pass_glossy_direct", 1 << 22),
    ("use_pass_glossy_indirect", 1 << 23),
    ("use_pass_glossy_color", 1 << 24),
    ("use_pass_transmission_direct", 1 << 25),
    ("use_pass_transmission_indirect", 1 << 26),
    ("use_pass_transmission_color", 1 << 27),
    ("use_pass_subsurface_direct", 1 << 28),
    ("use_pass_subsurface_indirect", 1 << 29),
    ("use_pass_subsurface_color", 1 << 30),
];
/// `ViewLayer.cryptomatte_flag` (short).
const VIEW_LAYER_CRYPTOMATTE_BITS: [(&str, i64); 4] = [
    ("use_pass_cryptomatte_object", 1 << 0),
    ("use_pass_cryptomatte_material", 1 << 1),
    ("use_pass_cryptomatte_asset", 1 << 2),
    ("use_pass_cryptomatte_accurate", 1 << 3),
];
/// `ViewLayer.grease_pencil_flags` (int): pass as separate output.
const VIEW_LAYER_GREASE_PENCIL_BITS: [(&str, i64); 1] = [("use_pass_grease_pencil", 1 << 0)];
/// `LayerCollection.flag` (short). Bits 0..=3 are deprecated and cleared by
/// current writers; bit 8 (`PREVIOUSLY_EXCLUDED`) is loader bookkeeping with
/// no RNA boolean and stays unmapped.
const LC_EXCLUDE: i64 = 1 << 4;
const LC_HOLDOUT: i64 = 1 << 5;
const LC_INDIRECT_ONLY: i64 = 1 << 6;
const LC_HIDE_VIEWPORT: i64 = 1 << 7;
/// `Collection.flag` (uchar). `HIDE_SELECT` (bit 1) has no place in this
/// projection; `IS_MASTER` (bit 5) marks the scene-embedded collection.
const COLL_HIDE_VIEWPORT: i64 = 1 << 0;
const COLL_HIDE_RENDER: i64 = 1 << 3;
/// `Object.restrictflag` (int): per-object hide and ray-visibility controls
/// in this merged profile. `HIDE_SELECT` (bit 1) and probe/pick bits (11..=15)
/// are unmapped here; the mapped booleans below are exact regardless.
const OB_HIDE_VIEWPORT: i64 = 1 << 0;
const OB_HIDE_RENDER: i64 = 1 << 2;
const OB_HIDE_CAMERA: i64 = 1 << 3;
const OB_HIDE_DIFFUSE: i64 = 1 << 4;
const OB_HIDE_GLOSSY: i64 = 1 << 5;
const OB_HIDE_TRANSMISSION: i64 = 1 << 6;
const OB_HIDE_VOLUME_SCATTER: i64 = 1 << 7;
const OB_HIDE_SHADOW: i64 = 1 << 8;
const OB_HOLDOUT: i64 = 1 << 9;
const OB_SHADOW_CATCHER: i64 = 1 << 10;

/// Project one saved Scene record into the view_layers semantic value.
///
/// The returned JSON has exactly the qualified payload keys: `layers` in saved
/// order, `collections` and `objects` as source-identity-deduplicated
/// presentation sets, and `visibility_basis`. Evidence stays in the query
/// envelope; nothing is added inside the semantic value.
pub fn project(file: &BlendFile, scene: &Record) -> Result<Json> {
    for name in REQUIRED_TYPES {
        if !file.schema().types.iter().any(|ty| ty == name.as_bytes()) {
            return Err(Error::new(
                "unsupported_type",
                format!(
                    "view_layers profile needs saved SDNA type {name}; use explain {name} --file INPUT --json"
                ),
            ));
        }
    }
    let layers = project_layers(file, scene)?;
    let (collections, objects) = project_membership(file, scene)?;
    Ok(json!({
        "layers": layers,
        "collections": collections,
        "objects": objects,
        "visibility_basis": "saved_controls_not_evaluated_visibility",
    }))
}

/// All saved layers of this scene in list order, including disabled ones.
fn project_layers(file: &BlendFile, scene: &Record) -> Result<Json> {
    let mut layers = Vec::new();
    for layer in access::list(file, scene, "view_layers")? {
        let mut flags = Map::new();
        for (field, bits) in [
            ("flag", &VIEW_LAYER_FLAG_BITS[..]),
            ("layflag", &VIEW_LAYER_LAYFLAG_BITS[..]),
            ("passflag", &VIEW_LAYER_PASSFLAG_BITS[..]),
            ("cryptomatte_flag", &VIEW_LAYER_CRYPTOMATTE_BITS[..]),
            ("grease_pencil_flags", &VIEW_LAYER_GREASE_PENCIL_BITS[..]),
        ] {
            let raw = access::int(file, &layer, field)?;
            for (name, bit) in bits {
                flags.insert((*name).to_string(), Json::Bool(raw & bit != 0));
            }
        }
        // One saved root per layer is the qualified structure; anything else
        // is reported instead of guessing which subtree is the tree.
        let roots = access::list(file, &layer, "layer_collections")?;
        if roots.len() != 1 {
            return Err(Error::new(
                "invalid_saved_structure",
                format!(
                    "view layer {:?} has {} root layer collections, expected exactly 1",
                    access::text(file, &layer, "name").unwrap_or_default(),
                    roots.len()
                ),
            ));
        }
        let mut seen = HashSet::new();
        let tree = layer_tree(file, &roots[0], &mut Vec::new(), &mut seen)?;
        layers.push(json!({
            "name": access::text(file, &layer, "name")?,
            "flags": Json::Object(flags),
            "pass_settings": {
                "samples": access::int(file, &layer, "samples")?,
                "pass_alpha_threshold": access::float(file, &layer, "pass_alpha_threshold")?,
                "pass_cryptomatte_depth": access::int(file, &layer, "cryptomatte_levels")?,
            },
            "layer_collection": tree,
        }));
    }
    Ok(Json::Array(layers))
}

/// Recursive saved layer-collection subtree with ordinal paths. The RNA
/// `LayerCollection.name` is the referenced collection's name, so the tree
/// reads the name through the pointer target. Cycles are diagnosed by visited
/// source identity, never by an invented depth bound.
fn layer_tree(
    file: &BlendFile,
    node: &Record,
    path: &mut Vec<u64>,
    seen: &mut HashSet<(usize, usize)>,
) -> Result<Json> {
    if !seen.insert((node.block, node.element)) {
        return Err(Error::new(
            "query_cycle",
            format!(
                "layer collection tree revisits block {} element {}",
                node.block, node.element
            ),
        ));
    }
    let target = access::pointer(file, node, "collection")?.ok_or_else(|| {
        Error::new(
            "invalid_saved_structure",
            "layer collection without a collection pointer",
        )
    })?;
    let reference = collection_ref(file, &target)?;
    let raw = access::int(file, node, "flag")?;
    let mut children = Vec::new();
    for (index, child) in access::list(file, node, "layer_collections")?
        .into_iter()
        .enumerate()
    {
        path.push(index as u64);
        children.push(layer_tree(file, &child, path, seen)?);
        path.pop();
    }
    seen.remove(&(node.block, node.element));
    Ok(json!({
        "path": path,
        "name": reference["name"],
        "collection": reference,
        "exclude": raw & LC_EXCLUDE != 0,
        "holdout": raw & LC_HOLDOUT != 0,
        "indirect_only": raw & LC_INDIRECT_ONLY != 0,
        "hide_viewport": raw & LC_HIDE_VIEWPORT != 0,
        "children": children,
    }))
}

/// Collection closure from the scene master plus the object set, both as
/// presentation sets. Locally reachable collection records report their own
/// controls and direct links; linked placeholders stay references.
fn project_membership(file: &BlendFile, scene: &Record) -> Result<(Json, Json)> {
    let master = access::pointer(file, scene, "master_collection")?.ok_or_else(|| {
        Error::new(
            "invalid_saved_structure",
            "scene without a master collection pointer",
        )
    })?;
    let mut collections: Vec<Record> = Vec::new();
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut objects: HashMap<(usize, usize), Record> = HashMap::new();
    let mut stack = vec![master];
    while let Some(record) = stack.pop() {
        if !seen.insert((record.block, record.element)) {
            continue;
        }
        let reference = collection_ref(file, &record)?;
        let local = reference["library"].is_null();
        if local {
            for child in access::list(file, &record, "children")? {
                if let Some(target) = access::pointer(file, &child, "collection")? {
                    stack.push(target);
                } else {
                    return Err(Error::new(
                        "invalid_saved_structure",
                        "collection child entry without a collection pointer",
                    ));
                }
            }
            for item in access::list(file, &record, "gobject")? {
                if let Some(target) = access::pointer(file, &item, "ob")? {
                    objects.insert((target.block, target.element), target);
                } else {
                    return Err(Error::new(
                        "invalid_saved_structure",
                        "collection object entry without an object pointer",
                    ));
                }
            }
        }
        collections.push(record);
    }
    let mut collection_rows: Vec<(String, Json)> = Vec::new();
    for record in &collections {
        let reference = collection_ref(file, record)?;
        let mut row = Map::new();
        row.insert("ref".into(), reference.clone());
        if !reference["library"].is_null() {
            row.insert("content_status".into(), json!("linked_reference"));
        } else {
            let raw = access::int(file, record, "flag")?;
            let mut linked_objects = Vec::new();
            for item in access::list(file, record, "gobject")? {
                if let Some(target) = access::pointer(file, &item, "ob")? {
                    linked_objects.push(object_ref(file, &target)?);
                } else {
                    return Err(Error::new(
                        "invalid_saved_structure",
                        "collection object entry without an object pointer",
                    ));
                }
            }
            let mut children = Vec::new();
            for child in access::list(file, record, "children")? {
                if let Some(target) = access::pointer(file, &child, "collection")? {
                    children.push(collection_ref(file, &target)?);
                } else {
                    return Err(Error::new(
                        "invalid_saved_structure",
                        "collection child entry without a collection pointer",
                    ));
                }
            }
            row.insert("content_status".into(), json!("local"));
            row.insert(
                "hide_render".into(),
                Json::Bool(raw & COLL_HIDE_RENDER != 0),
            );
            row.insert(
                "hide_viewport".into(),
                Json::Bool(raw & COLL_HIDE_VIEWPORT != 0),
            );
            row.insert("objects".into(), sort_refs(linked_objects));
            row.insert("children".into(), Json::Array(children));
        }
        collection_rows.push((python_ref_key(&reference), Json::Object(row)));
    }
    collection_rows.sort_by(|a, b| a.0.cmp(&b.0));
    let memberships = collection_memberships(file)?;
    let mut object_rows: Vec<(String, Json)> = Vec::new();
    for (key, record) in &objects {
        let reference = object_ref(file, record)?;
        let mut row = Map::new();
        row.insert("ref".into(), reference.clone());
        if !reference["library"].is_null() {
            row.insert("content_status".into(), json!("linked_reference"));
        } else {
            let raw = access::int(file, record, "restrictflag")?;
            row.insert("content_status".into(), json!("local"));
            row.insert("hide_render".into(), Json::Bool(raw & OB_HIDE_RENDER != 0));
            row.insert(
                "hide_viewport".into(),
                Json::Bool(raw & OB_HIDE_VIEWPORT != 0),
            );
            row.insert(
                "visible_camera".into(),
                Json::Bool(raw & OB_HIDE_CAMERA == 0),
            );
            row.insert(
                "visible_diffuse".into(),
                Json::Bool(raw & OB_HIDE_DIFFUSE == 0),
            );
            row.insert(
                "visible_glossy".into(),
                Json::Bool(raw & OB_HIDE_GLOSSY == 0),
            );
            row.insert(
                "visible_transmission".into(),
                Json::Bool(raw & OB_HIDE_TRANSMISSION == 0),
            );
            row.insert(
                "visible_volume_scatter".into(),
                Json::Bool(raw & OB_HIDE_VOLUME_SCATTER == 0),
            );
            row.insert(
                "visible_shadow".into(),
                Json::Bool(raw & OB_HIDE_SHADOW == 0),
            );
            row.insert("is_holdout".into(), Json::Bool(raw & OB_HOLDOUT != 0));
            row.insert(
                "is_shadow_catcher".into(),
                Json::Bool(raw & OB_SHADOW_CATCHER != 0),
            );
            row.insert(
                "collections".into(),
                sort_refs(memberships.get(key).cloned().unwrap_or_default()),
            );
        }
        object_rows.push((python_ref_key(&reference), Json::Object(row)));
    }
    object_rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((
        Json::Array(collection_rows.into_iter().map(|(_, row)| row).collect()),
        Json::Array(object_rows.into_iter().map(|(_, row)| row).collect()),
    ))
}

/// File-global collection-to-object membership, the saved counterpart of the
/// object's users_collection view: every saved local collection record counts,
/// including scene-embedded masters, because a directly linked object belongs
/// to its master collection just like to any other collection. Linked-library
/// placeholders carry no membership links of their own and never appear here.
fn collection_memberships(file: &BlendFile) -> Result<HashMap<(usize, usize), Vec<Json>>> {
    let mut memberships: HashMap<(usize, usize), Vec<Json>> = HashMap::new();
    let mut linked: HashSet<(usize, usize, usize, usize)> = HashSet::new();
    for record in access::records(file, "Collection")? {
        let collection = collection_ref(file, &record)?;
        if !collection["library"].is_null() {
            continue;
        }
        for item in access::list(file, &record, "gobject")? {
            let Some(target) = access::pointer(file, &item, "ob")? else {
                return Err(Error::new(
                    "invalid_saved_structure",
                    "collection object entry without an object pointer",
                ));
            };
            let key = (target.block, target.element);
            if linked.insert((record.block, record.element, key.0, key.1)) {
                memberships.entry(key).or_default().push(collection.clone());
            }
        }
    }
    Ok(memberships)
}

/// Reference for a record reached from a collection context. The referenced
/// record is either a full owner struct (`id` member) or a saved
/// linked-library placeholder (bare `ID` block); `access::id_ref` reads both
/// shapes. The kind is forced from the walked context because the master
/// collection carries the scene-kind `SC` name prefix by Blender's naming
/// convention, which would otherwise mislabel it.
fn collection_ref(file: &BlendFile, record: &Record) -> Result<Json> {
    let mut reference = access::id_ref(file, record)?;
    reference["kind"] = json!("Collection");
    Ok(reference)
}

/// Same as [`collection_ref`] for object contexts (`OB` prefixes agree with
/// the context, so this is a uniform context-typed reference).
fn object_ref(file: &BlendFile, record: &Record) -> Result<Json> {
    let mut reference = access::id_ref(file, record)?;
    reference["kind"] = json!("Object");
    Ok(reference)
}

/// Sort references by the exact native presentation key:
/// `json.dumps(ref, sort_keys=True, ensure_ascii=True)` byte order.
fn sort_refs(mut references: Vec<Json>) -> Json {
    let mut keyed: Vec<(String, Json)> = references
        .drain(..)
        .map(|r| (python_ref_key(&r), r))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    Json::Array(keyed.into_iter().map(|(_, r)| r).collect())
}

/// Reproduce Python `json.dumps` with `sort_keys=True, ensure_ascii=True` and
/// default separators for the three-key reference objects. Key order is
/// kind, library, name; non-ASCII text becomes lowercase `\uXXXX` escapes
/// (surrogate pairs beyond the BMP), so ordering matches the native oracle
/// even for names outside ASCII.
fn python_ref_key(reference: &Json) -> String {
    let kind = reference["kind"].as_str().unwrap_or("");
    let library = match reference.get("library") {
        Some(Json::String(text)) => python_escaped(text),
        Some(Json::Null) | None => "null".to_string(),
        Some(other) => other.to_string(),
    };
    let name = match reference.get("name") {
        Some(Json::String(text)) => python_escaped(text),
        _ => "null".to_string(),
    };
    format!(
        "{{\"kind\": {}, \"library\": {}, \"name\": {}}}",
        python_escaped(kind),
        library,
        name
    )
}

/// One Python `ensure_ascii` JSON string, escaping exactly as `json.dumps`.
fn python_escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let value = c as u32;
                if value > 0xffff {
                    let rest = value - 0x10000;
                    let high = 0xd800 + (rest >> 10);
                    let low = 0xdc00 + (rest & 0x3ff);
                    out.push_str(&format!("\\u{high:04x}\\u{low:04x}"));
                } else {
                    out.push_str(&format!("\\u{value:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
