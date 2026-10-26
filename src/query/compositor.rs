//! Saved compositor projection for one qualified Scene record.
//!
//! The parent query layer owns profile admission (currently Blender 5.2 saved
//! bytes, header 502 / subversion 44). This module projects only what the saved
//! SDNA and checked accessors actually contain: no graph evaluation, no RNA
//! defaults, no external library traversal and no produced-file claims.
//!
//! Representation precedence is by field presence, never by a fallback from a
//! null modern pointer to the legacy field:
//!   Scene.compositing_node_group (5.2) -> Scene.nodetree (legacy).
//!
//! File Output uses the 5.2 item model when the saved storage has the `items`
//! array. A legacy `NodeImageMultiFile` without that array stays an explicit
//! `unverified_semantics` gap instead of being translated into modern slots.

use crate::query::access::{self, Record};
use crate::query::props;
use crate::value::Value;
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::{HashMap, HashSet};

// Saved flag bits, from DNA_node_types.h (3.6/4.2/5.2 agree).
const NODE_MUTED: i64 = 1 << 9;
const NODE_LINK_MUTED: i64 = 1 << 4;
const SOCK_HIDDEN: i64 = 1 << 1;
const SOCK_MULTI_INPUT: i64 = 1 << 11;
/// ID_FLAG_EMBEDDED_DATA (DNA_ID.h, 5.2.1). bpy.data collections exclude
/// embedded IDs; a Scene's deprecated `nodetree` compatibility copy is one.
const ID_FLAG_EMBEDDED_DATA: i64 = 1 << 10;

/// Project `values.compositor` for a Scene record: `{scene_tree, trees}`.
pub fn project(file: &BlendFile, scene: &Record) -> Result<Json> {
    let root = scene_root(file, scene)?;
    let root_ref = match &root {
        Some(root) => Some(tree_target_ref(file, root, "Scene compositor root")?),
        None => None,
    };

    // File-global local compositor inventory. This intentionally includes
    // unused local trees; the projection is an inventory, not a reachability
    // result. Linked IDs are references and never traversed.
    let mut inventory: Vec<Record> = Vec::new();
    let mut seen: HashSet<(usize, usize)> = HashSet::new();

    // bpy.data collections expose non-embedded local IDs. A Scene's deprecated
    // `nodetree` compatibility copy is embedded and therefore not a data-block
    // (the synthetic fixture stores two different trees with the same name).
    // Scene-owned roots and group dependencies are added below even if their
    // saved ID flag says embedded.
    for tree in access::records(file, "bNodeTree")? {
        if !is_local(file, &tree)? {
            continue;
        }
        if access::int(file, &tree, "id.flag")? & ID_FLAG_EMBEDDED_DATA != 0 {
            continue;
        }
        if access::text(file, &tree, "idname")? != "CompositorNodeTree" {
            continue;
        }
        push_tree(&mut inventory, &mut seen, tree);
    }

    // The oracle also admits every local Scene's root even if it was not found
    // through bpy.data.node_groups. Keep that saved, file-global inventory.
    for owner in access::records(file, "Scene")? {
        if !is_local(file, &owner)? {
            continue;
        }
        if let Some(owned) = scene_root(file, &owner)? {
            ensure_tree_target(file, &owned, "Scene compositor root")?;
            if is_local(file, &owned)? {
                push_tree(&mut inventory, &mut seen, owned);
            }
        }
    }
    if let Some(root) = &root
        && is_local(file, root)?
    {
        push_tree(&mut inventory, &mut seen, root.clone());
    }

    // Local group dependencies of every local tree, including unused trees.
    let mut cursor = 0;
    while cursor < inventory.len() {
        let tree = inventory[cursor].clone();
        for node in access::list(file, &tree, "nodes")? {
            if !is_group_node(file, &node)? {
                continue;
            }
            let Some(target) = access::pointer(file, &node, "id")? else {
                continue;
            };
            ensure_tree_target(file, &target, "CompositorNodeGroup id")?;
            if is_local(file, &target)? {
                push_tree(&mut inventory, &mut seen, target);
            }
        }
        cursor += 1;
    }

    let mut trees = Vec::with_capacity(inventory.len());
    for tree in &inventory {
        trees.push(tree_record(file, tree)?);
    }
    // The native oracle sorts complete tree records by canonical JSON
    // (json.dumps(..., sort_keys=True, ensure_ascii=True)). Match that order
    // rather than imposing a nicer but incompatible one.
    trees.sort_by_cached_key(py_sort_key);

    Ok(json!({"scene_tree": root_ref, "trees": trees}))
}

/// Undefined status belongs to saved evidence, not a guessed load-time class.
pub(super) fn undefined_nodes(value: &Json) -> Vec<Json> {
    let mut rows = Vec::new();
    if let Some(trees) = value["trees"].as_array() {
        for tree in trees {
            if let Some(nodes) = tree["nodes"].as_array() {
                for (index, node) in nodes.iter().enumerate() {
                    if node["type"].as_str().is_some_and(|name| {
                        name == "NodeUndefined" || name.starts_with("Undefined[")
                    }) {
                        rows.push(json!({"tree":tree["ref"],"node_index":index,"name":node["name"],"saved_type":node["type"],"undefined_in_saved_file":true}));
                    }
                }
            }
        }
    }
    rows
}
fn push_tree(inventory: &mut Vec<Record>, seen: &mut HashSet<(usize, usize)>, tree: Record) {
    if seen.insert((tree.block, tree.element)) {
        inventory.push(tree);
    }
}

/// Modern authoritative root if its field exists, including an explicitly null
/// pointer. Only a genuinely legacy Scene reads `nodetree`.
fn scene_root(file: &BlendFile, scene: &Record) -> Result<Option<Record>> {
    if field_present(file, scene, "compositing_node_group")? {
        return access::pointer(file, scene, "compositing_node_group");
    }
    if field_present(file, scene, "nodetree")? {
        return access::pointer(file, scene, "nodetree");
    }
    Ok(None)
}

fn field_present(file: &BlendFile, record: &Record, path: &str) -> Result<bool> {
    match access::get(file, record, path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind == "missing_field" => Ok(false),
        Err(error) => Err(error),
    }
}

fn record_type<'a>(file: &'a BlendFile, record: &Record) -> Result<&'a [u8]> {
    let block = file.block(record.block)?;
    let structure = file
        .schema()
        .structs
        .get(block.sdna_index)
        .ok_or_else(|| Error::new("unsupported_type", "block has no SDNA structure"))?;
    file.schema()
        .types
        .get(structure.type_index)
        .map(Vec::as_slice)
        .ok_or_else(|| Error::new("unsupported_type", "SDNA structure type index is absent"))
}

fn is_local(file: &BlendFile, record: &Record) -> Result<bool> {
    match access::pointer(file, record, "id.lib") {
        Ok(library) => Ok(library.is_none()),
        Err(error) if error.kind == "missing_field" => {
            // A root-file linked-ID stub is a bare ID record.
            match access::pointer(file, record, "lib") {
                Ok(library) => Ok(library.is_none()),
                Err(error) if error.kind == "missing_field" => Err(Error::new(
                    "type",
                    "record has neither an embedded ID library nor a bare-ID library field",
                )),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

fn is_group_node(file: &BlendFile, node: &Record) -> Result<bool> {
    Ok(access::text(file, node, "idname")?.ends_with("NodeGroup"))
}

/// A root-file linked-ID stub is a bare `ID` record. Its two-byte name prefix
/// is the only saved type discriminator; the subtype `idname` lives on a full
/// bNodeTree record and is absent here.
fn bare_id_prefix(file: &BlendFile, record: &Record) -> Result<Option<[u8; 2]>> {
    if record_type(file, record)? != b"ID" {
        return Ok(None);
    }
    let name = match access::get(file, record, "name")? {
        Value::Bytes(name) => name,
        _ => return Err(Error::new("type", "ID name must be bytes")),
    };
    if name.len() < 2 {
        return Err(Error::new("type", "ID name lacks two-byte kind"));
    }
    Ok(Some([name[0], name[1]]))
}

fn ensure_tree_target(file: &BlendFile, record: &Record, context: &str) -> Result<()> {
    if record_type(file, record)? == b"bNodeTree" {
        return Ok(());
    }
    if bare_id_prefix(file, record)?.as_ref() == Some(b"NT") {
        return Ok(());
    }
    Err(Error::new(
        "type",
        format!("{context} does not target a saved bNodeTree or an NT ID stub"),
    ))
}

fn tree_target_ref(file: &BlendFile, record: &Record, context: &str) -> Result<Json> {
    ensure_tree_target(file, record, context)?;
    tree_ref(file, record)
}

/// Reference for a local full bNodeTree or a bare linked-ID stub. The shared
/// id_ref helper already falls back to `NodeTree` when the subtype `idname` is
/// not stored on a bare stub.
fn tree_ref(file: &BlendFile, record: &Record) -> Result<Json> {
    access::id_ref(file, record)
}

fn tree_record(file: &BlendFile, tree: &Record) -> Result<Json> {
    let node_records = access::list(file, tree, "nodes")?;
    let mut nodes = Vec::with_capacity(node_records.len());
    let mut views: Vec<NodeView> = Vec::with_capacity(node_records.len());
    let mut by_id: HashMap<(usize, usize), usize> = HashMap::new();

    for node in &node_records {
        let inputs = access::list(file, node, "inputs")?;
        let outputs = access::list(file, node, "outputs")?;
        let name = access::text(file, node, "name")?;
        let value = node_record(file, node, &inputs, &outputs)?;
        by_id.insert((node.block, node.element), views.len());
        views.push(NodeView {
            name,
            inputs: identities(&inputs),
            outputs: identities(&outputs),
        });
        nodes.push(value);
    }

    let mut links = Vec::new();
    for link in access::list(file, tree, "links")? {
        links.push(link_record(file, &link, &views, &by_id)?);
    }

    let system = if field_present(file, tree, "id.system_properties")? {
        Some("id.system_properties")
    } else {
        None
    };
    let id_properties = props::decode_owner(file, tree, "id.properties", system)?;

    Ok(json!({
        "ref": tree_ref(file, tree)?,
        "nodes": nodes,
        "links": links,
        "id_properties": id_properties
    }))
}

struct NodeView {
    name: String,
    inputs: Vec<(usize, usize)>,
    outputs: Vec<(usize, usize)>,
}

fn identities(records: &[Record]) -> Vec<(usize, usize)> {
    records.iter().map(|r| (r.block, r.element)).collect()
}

fn node_record(
    file: &BlendFile,
    node: &Record,
    inputs: &[Record],
    outputs: &[Record],
) -> Result<Json> {
    let name = access::text(file, node, "name")?;
    let idname = access::text(file, node, "idname")?;
    let label = access::text(file, node, "label")?;
    let flag = access::int(file, node, "flag")?;
    let system = if field_present(file, node, "system_properties")? {
        Some("system_properties")
    } else {
        None
    };
    let id_properties = props::decode_owner(file, node, "prop", system)?;

    let mut value = json!({
        "name": name,
        "type": idname.clone(),
        "label": label,
        "mute": flag & NODE_MUTED != 0,
        "inputs": socket_list(file, inputs)?,
        "outputs": socket_list(file, outputs)?,
        "id_properties": id_properties
    });

    // The native oracle uses hasattr(node, "node_tree"): a group node emits the
    // key even when its saved id pointer is null.
    if idname.ends_with("NodeGroup") {
        let group_tree = match access::pointer(file, node, "id")? {
            Some(target) => tree_target_ref(file, &target, "CompositorNodeGroup id")?,
            None => Json::Null,
        };
        value["group_tree"] = group_tree;
    }

    if idname == "CompositorNodeOutputFile" {
        value["file_output"] = file_output(file, node)?;
    }

    if idname == "CompositorNodeRLayers" {
        value["render_layers"] = render_layers(file, node)?;
    }

    Ok(value)
}

fn socket_list(file: &BlendFile, sockets: &[Record]) -> Result<Json> {
    let mut values = Vec::with_capacity(sockets.len());
    for (index, socket) in sockets.iter().enumerate() {
        let flag = access::int(file, socket, "flag")?;
        values.push(json!({
            "index": index,
            "identifier": access::text(file, socket, "identifier")?,
            "name": access::text(file, socket, "name")?,
            "type": access::text(file, socket, "idname")?,
            "hide": flag & SOCK_HIDDEN != 0,
            "is_multi_input": flag & SOCK_MULTI_INPUT != 0
        }));
    }
    Ok(Json::Array(values))
}

#[derive(Clone, Copy)]
enum Direction {
    Input,
    Output,
}

fn link_record(
    file: &BlendFile,
    link: &Record,
    views: &[NodeView],
    by_id: &HashMap<(usize, usize), usize>,
) -> Result<Json> {
    let flag = access::int(file, link, "flag")?;
    Ok(json!({
        "from": endpoint(file, link, "fromnode", "fromsock", Direction::Output, views, by_id)?,
        "to": endpoint(file, link, "tonode", "tosock", Direction::Input, views, by_id)?,
        "is_muted": flag & NODE_LINK_MUTED != 0
    }))
}

fn endpoint(
    file: &BlendFile,
    link: &Record,
    node_field: &str,
    socket_field: &str,
    direction: Direction,
    views: &[NodeView],
    by_id: &HashMap<(usize, usize), usize>,
) -> Result<Json> {
    let node = access::pointer(file, link, node_field)?.ok_or_else(|| {
        Error::new(
            "query_list",
            format!("saved link has a null {node_field} pointer"),
        )
    })?;
    let socket = access::pointer(file, link, socket_field)?.ok_or_else(|| {
        Error::new(
            "query_list",
            format!("saved link has a null {socket_field} pointer"),
        )
    })?;
    let view_index = *by_id.get(&(node.block, node.element)).ok_or_else(|| {
        Error::new(
            "query_list",
            format!("saved link {node_field} is not a member of the tree's saved node list"),
        )
    })?;
    let view = &views[view_index];
    let sockets = match direction {
        Direction::Input => &view.inputs,
        Direction::Output => &view.outputs,
    };
    let socket_index = sockets
        .iter()
        .position(|identity| *identity == (socket.block, socket.element))
        .ok_or_else(|| {
            Error::new(
                "query_list",
                format!("saved link {socket_field} is not a socket of its saved node list"),
            )
        })?;
    Ok(json!({
        "node": view.name.clone(),
        "socket_index": socket_index,
        "socket_identifier": access::text(file, &socket, "identifier")?
    }))
}

fn file_output(file: &BlendFile, node: &Record) -> Result<Json> {
    let storage = access::pointer(file, node, "storage")?.ok_or_else(|| {
        Error::new(
            "unverified_semantics",
            "CompositorNodeOutputFile has a null saved storage pointer",
        )
    })?;
    // The synthetic 5.2 schema names the modern item container
    // NodeImageMultiFile; upstream v5.2.1 names the same representation
    // NodeCompositorFileOutput. Both are the item model when `items` exists.
    if !matches!(
        record_type(file, &storage)?,
        b"NodeImageMultiFile" | b"NodeCompositorFileOutput"
    ) {
        return Err(Error::new(
            "type",
            "CompositorNodeOutputFile storage is neither a saved NodeImageMultiFile nor NodeCompositorFileOutput",
        ));
    }

    // Representation discriminator is the saved items array itself, not a
    // header version or an RNA default.
    if !field_present(file, &storage, "items")? {
        return Err(Error::new(
            "unverified_semantics",
            "legacy File Output slot model has no saved items array; slot/format inheritance is not qualified here",
        ));
    }

    let count = access::int(file, &storage, "items_count")?;
    let count = usize::try_from(count)
        .map_err(|_| Error::new("overflow", "File Output items_count is negative"))?;
    let items = access::pointer(file, &storage, "items")?;

    // Do not preallocate from the saved count before its block extent is
    // checked; a corrupt count must not become an allocation request.
    let mut slots = Vec::new();
    if count > 0 {
        let items = items.ok_or_else(|| {
            Error::new(
                "type",
                "File Output items_count is nonzero while the items pointer is null",
            )
        })?;
        if record_type(file, &items)? != b"NodeCompositorFileOutputItem" {
            return Err(Error::new(
                "type",
                "File Output items pointer does not target NodeCompositorFileOutputItem records",
            ));
        }
        let block = file.block(items.block)?;
        let width = file.schema().lengths[file.schema().structs[block.sdna_index].type_index];
        let extent = count
            .checked_mul(width)
            .ok_or_else(|| Error::new("overflow", "File Output items extent"))?;
        if width == 0 || count > block.count || extent > block.size {
            return Err(Error::new(
                "bounds",
                "File Output items_count exceeds the saved NodeCompositorFileOutputItem block extent",
            ));
        }
        for index in 0..count {
            slots.push(slot_record(file, &Record::new(items.block, index), index)?);
        }
    }

    let file_name = optional_c_string(file, &storage, "file_name")?;
    Ok(json!({
        "model": "file_output_items",
        "directory": text_field(file, &storage, &["directory", "base_path"])?,
        "file_name": file_name,
        "format": image_format(file, &storage.at("format"))?,
        "save_as_render": access::int(file, &storage, "save_as_render")? != 0,
        "use_file_extension": access::int(file, &storage, "use_file_extension")? != 0,
        "slots": slots
    }))
}

fn slot_record(file: &BlendFile, item: &Record, index: usize) -> Result<Json> {
    Ok(json!({
        "index": index,
        "name": optional_c_string(file, item, "name")?,
        "socket_type": socket_type_name(access::int(file, item, "socket_type")?)?,
        "override_node_format": access::int(file, item, "override_node_format")? != 0,
        "save_as_render": access::int(file, item, "save_as_render")? != 0,
        "format": image_format(file, &item.at("format"))?
    }))
}

fn optional_c_string(file: &BlendFile, record: &Record, path: &str) -> Result<String> {
    match access::pointer(file, record, path)? {
        Some(target) => c_string(file, &target),
        None => Ok(String::new()),
    }
}

/// A saved `char *` points at a separate raw-data block. It is not an embedded
/// SDNA field, so read that block's payload and require valid UTF-8.
fn c_string(file: &BlendFile, target: &Record) -> Result<String> {
    if target.element != 0 {
        return Err(Error::new(
            "type",
            "string pointer targets a non-zero saved element",
        ));
    }
    let payload = file.payload(target.block)?;
    let end = payload.iter().position(|byte| *byte == 0).ok_or_else(|| {
        Error::new(
            "string_termination",
            "saved compositor string lacks its NUL terminator",
        )
    })?;
    std::str::from_utf8(&payload[..end])
        .map(str::to_owned)
        .map_err(|_| Error::new("encoding", "saved string is not valid UTF-8"))
}

fn image_format(file: &BlendFile, format: &Record) -> Result<Json> {
    super::render::image_format(file, format, "")
}

fn text_field(file: &BlendFile, record: &Record, names: &[&str]) -> Result<String> {
    for name in names {
        match access::get(file, record, name) {
            Ok(_) => return access::text(file, record, name),
            Err(error) if error.kind == "missing_field" => continue,
            Err(error) => return Err(error),
        }
    }
    Err(Error::new(
        "missing_field",
        format!("none of the saved fields {} is present", names.join("/")),
    ))
}

fn mapped(value: i64, field: &str, table: &[(i64, &'static str)]) -> Result<&'static str> {
    table
        .iter()
        .find_map(|(candidate, name)| (*candidate == value).then_some(*name))
        .ok_or_else(|| {
            Error::new(
                "unverified_semantics",
                format!("saved {field} value {value} has no qualified semantic name"),
            )
        })
}

fn socket_type_name(value: i64) -> Result<&'static str> {
    // eNodeSocketDatatype and the RNA identifiers from rna_nodetree.cc.
    mapped(
        value,
        "NodeCompositorFileOutputItem.socket_type",
        &[
            (0, "FLOAT"),
            (1, "VECTOR"),
            (2, "RGBA"),
            (3, "SHADER"),
            (4, "BOOLEAN"),
            (6, "INT"),
            (7, "STRING"),
            (8, "OBJECT"),
            (9, "IMAGE"),
            (10, "GEOMETRY"),
            (11, "COLLECTION"),
            (12, "TEXTURE"),
            (13, "MATERIAL"),
            (14, "ROTATION"),
            (15, "MENU"),
            (16, "MATRIX"),
            (17, "BUNDLE"),
            (18, "CLOSURE"),
            (19, "FONT"),
            (20, "SCENE"),
            (21, "TEXT"),
            (22, "MASK"),
            (23, "SOUND"),
            (24, "INT_VECTOR"),
        ],
    )
}

fn render_layers(file: &BlendFile, node: &Record) -> Result<Json> {
    let Some(scene) = access::pointer(file, node, "id")? else {
        return Ok(json!({
            "scene": null,
            "scene_binding": "null_scene_pointer",
            "layer": ""
        }));
    };
    let scene_type = record_type(file, &scene)?;
    if scene_type == b"ID" && bare_id_prefix(file, &scene)?.as_ref() == Some(b"SC") {
        return Err(Error::new(
            "unverified_semantics",
            "CompositorNodeRLayers points at a root-file Scene stub; its ViewLayers are not saved in this file",
        ));
    }
    if scene_type != b"Scene" {
        return Err(Error::new(
            "type",
            "CompositorNodeRLayers id pointer does not target a saved Scene",
        ));
    }
    let scene_reference = access::id_ref(file, &scene)?;
    let layer_index = access::int(file, node, "custom1")?;
    let layer_index = usize::try_from(layer_index).map_err(|_| {
        Error::new(
            "type",
            "CompositorNodeRLayers custom1 layer index is negative",
        )
    })?;
    let layers = match access::list(file, &scene, "view_layers") {
        Ok(layers) => layers,
        Err(error) if error.kind == "missing_field" => {
            return Err(Error::new(
                "unverified_semantics",
                "CompositorNodeRLayers Scene has no saved ViewLayers in this file",
            ));
        }
        Err(error) => return Err(error),
    };
    let layer = layers.get(layer_index).ok_or_else(|| {
        Error::new(
            "type",
            "CompositorNodeRLayers custom1 layer index is outside its explicit Scene.view_layers",
        )
    })?;
    Ok(json!({
        "scene": scene_reference,
        "scene_binding": "explicit",
        "layer": access::text(file, layer, "name")?
    }))
}

/// Python `json.dumps(value, sort_keys=True, ensure_ascii=True)` ordering key.
/// The projected tree has only JSON scalars, arrays and objects; numbers are
/// integers, so this encoder does not need Python's float formatting rules.
fn py_sort_key(value: &Json) -> String {
    let mut out = String::new();
    write_py_json(value, &mut out);
    out
}

fn write_py_json(value: &Json, out: &mut String) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Json::Number(value) => out.push_str(&value.to_string()),
        Json::String(value) => write_py_string(value, out),
        Json::Array(values) => {
            out.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    out.push_str(", ");
                }
                write_py_json(value, out);
            }
            out.push(']');
        }
        Json::Object(values) => {
            out.push('{');
            let mut keys: Vec<&String> = values.keys().collect();
            keys.sort();
            for (index, key) in keys.iter().enumerate() {
                if index != 0 {
                    out.push_str(", ");
                }
                write_py_string(key.as_str(), out);
                out.push_str(": ");
                if let Some(value) = values.get(key.as_str()) {
                    write_py_json(value, out);
                }
            }
            out.push('}');
        }
    }
}

fn write_py_string(value: &str, out: &mut String) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            character if (0x20..=0x7e).contains(&(character as u32)) => out.push(character),
            character => {
                let codepoint = character as u32;
                if codepoint <= 0xffff {
                    out.push_str(&format!("\\u{codepoint:04x}"));
                } else {
                    let codepoint = codepoint - 0x10000;
                    let high = 0xd800 + (codepoint >> 10);
                    let low = 0xdc00 + (codepoint & 0x03ff);
                    out.push_str(&format!("\\u{high:04x}\\u{low:04x}"));
                }
            }
        }
    }
    out.push('"');
}
