//! Read-only SDNA field comparison. Identities, never allocator addresses.
use crate::file_ops::graph::{text, uint};
use crate::file_ops::{Census, pointer_census};
use crate::value::hex;
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

/// These exact serialized fields are process-local, not semantic file state.
/// Every observed exclusion is returned with its reason; identifier is NOT here.
fn ignored(ty: &[u8], field: &[u8]) -> Option<&'static str> {
    match (ty, field) {
        (
            b"ID",
            b"next" | b"prev" | b"newid" | b"orig_id" | b"py_instance" | b"runtime"
            | b"session_uid",
        ) => Some("ID process/list/runtime identity; roots matched by type+name+library"),
        (b"bNodeTreeInterface", b"runtime")
        | (b"Paint", b"runtime")
        | (b"Editing", b"runtime")
        | (b"WorkSpace", b"runtime")
        | (b"wmWindow", b"runtime")
        | (b"PreviewImage", b"runtime")
        | (b"SpaceSpreadsheet", b"runtime") => {
            Some("explicit C++ runtime handle, rebuilt by native read")
        }
        (b"CurveProfile", b"table" | b"segments") => Some("evaluated curve profile runtime cache"),
        (b"AssetShelf", b"type") => Some("asset-shelf runtime type registry"),
        (b"bNodeTree", b"runtime" | b"typeinfo") => Some("node tree runtime cache/type registry"),
        (b"bNode", b"runtime" | b"typeinfo") | (b"bNodeSocket", b"runtime" | b"typeinfo") => {
            Some("node/socket runtime cache/type registry")
        }
        (b"Image", b"runtime" | b"cache" | b"rr" | b"gputexture" | b"lastused") => {
            Some("image runtime GPU/decoded cache")
        }
        (
            b"Scene",
            b"runtime"
            | b"depsgraph_hash"
            | b"fps_info"
            | b"sound_scene"
            | b"playback_handle"
            | b"sound_scrub_handle"
            | b"speaker_handles",
        ) => Some("scene runtime dependency/audio state"),
        (b"ViewLayer", b"stats" | b"object_bases_hash" | b"runtime") => {
            Some("view-layer runtime cache")
        }
        (
            b"wmWindowManager",
            b"runtime" | b"undo_stack" | b"message_bus" | b"windrawable" | b"winactive"
            | b"autosavetimer",
        ) => Some("window-manager runtime process state"),
        (
            b"wmWindow",
            b"ghostwin"
            | b"gpuctx"
            | b"eventstate"
            | b"event_last_handled"
            | b"ime_data"
            | b"active_view_layer"
            | b"winid",
        ) => Some("native window/event/session state"),
        (b"ScrArea", b"runtime")
        | (b"ARegion", b"runtime" | b"type" | b"gizmo_map" | b"regiontimer" | b"headerstr") => {
            Some("editor runtime state")
        }
        (b"bScreen", b"context" | b"active_region" | b"animtimer" | b"tool_tip" | b"runtime") => {
            Some("screen runtime context")
        }
        (b"CurveMapping", b"table" | b"premultable") | (b"CurveMap", b"table" | b"premultable") => {
            Some("evaluated curve runtime cache")
        }
        (b"FileGlobal", b"winpos" | b"curscreen" | b"curarea") => {
            Some("file-open process UI selection")
        }
        _ => None,
    }
}
struct Inventory<'a> {
    f: &'a BlendFile,
    c: Census,
    keys: Vec<Option<String>>,
    by_key: BTreeMap<String, usize>,
    pointers: BTreeMap<(usize, usize), Option<(usize, usize)>>,
    unresolved: Vec<Json>,
    excluded: Vec<Json>,
    pointer_arrays: BTreeSet<usize>,
}
impl<'a> Inventory<'a> {
    fn new(f: &'a BlendFile) -> Result<Self> {
        let c = pointer_census(f)?;
        let pointers = c
            .sites
            .iter()
            .map(|s| ((s.block, s.offset), s.target))
            .collect();
        let mut me = Self {
            f,
            c,
            keys: vec![None; f.blocks().len()],
            by_key: BTreeMap::new(),
            pointers,
            unresolved: Vec::new(),
            excluded: Vec::new(),
            pointer_arrays: BTreeSet::new(),
        };
        me.expand_pointer_arrays()?;
        // All ID roots, including native Library placeholders, use the serialized ID name.
        for (i, b) in f.blocks().iter().enumerate() {
            if crate::typed_edit::owner_code(&b.code) {
                let name = text(f, i, &["id", "name"]).or_else(|_| text(f, i, &["name"]))?;
                let lib = uint(f, i, &["id", "lib"])
                    .or_else(|_| uint(f, i, &["lib"]))
                    .unwrap_or(0);
                let library = if lib == 0 {
                    String::new()
                } else {
                    let t = f
                        .dereference(lib)?
                        .ok_or_else(|| Error::new("diff_identity", "unresolved ID library"))?;
                    text(f, t, &["name"])?
                };
                me.assign(
                    i,
                    format!("ID/{}/{}@{}", name.get(..2).unwrap_or(""), name, library),
                )?;
            } else if b.code != *b"DATA" && b.code != *b"DNA1" {
                me.assign(i, format!("ROOT/{}/{}", hex(&b.code), me.ty(i)))?;
            }
        }
        // Tree-relative node names/types form the explicit node-identifier bijection.
        for (i, _) in f.blocks().iter().enumerate() {
            if me.ty(i) == "bNode"
                && let Some(owner) = me.c.owners[i]
                && let Some(root) = &me.keys[owner]
            {
                me.assign(
                    i,
                    format!(
                        "{root}/NODE/{}:{}",
                        text(f, i, &["name"])?,
                        text(f, i, &["idname"])?
                    ),
                )?;
            }
        }
        // Socket identity is anchored to a node + direction + serialized identifier.
        let nodes = (0..f.blocks().len())
            .filter(|i| me.ty(*i) == "bNode")
            .collect::<Vec<_>>();
        for node in nodes {
            let Some(root) = me.keys[node].clone() else {
                continue;
            };
            for direction in ["inputs", "outputs"] {
                for socket in me.list(node, &[direction])? {
                    me.assign(
                        socket,
                        format!("{root}/{direction}/{}", text(f, socket, &["identifier"])?),
                    )?;
                }
            }
        }
        // Link identity is its endpoint tuple; duplicate endpoint links are refused.
        for i in 0..f.blocks().len() {
            if me.ty(i) == "bNodeLink" {
                let endpoints = ["fromnode", "fromsock", "tonode", "tosock"]
                    .iter()
                    .map(|p| {
                        me.pointer(i, &[*p]).and_then(|t| {
                            t.and_then(|(b, _)| me.keys[b].clone()).ok_or_else(|| {
                                Error::new("diff_identity", "unanchored link endpoint")
                            })
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let owner = me.c.owners[i]
                    .and_then(|o| me.keys[o].clone())
                    .ok_or_else(|| Error::new("diff_identity", "link owner"))?;
                me.assign(i, format!("{owner}/LINK/{}", endpoints.join(" -> ")))?;
            }
        }
        // Assign remaining owned allocations through the actual persistent graph.
        let mut queue = (0..f.blocks().len())
            .filter(|i| me.keys[*i].is_some())
            .collect::<VecDeque<_>>();
        let mut visited = BTreeSet::new();
        while let Some(b) = queue.pop_front() {
            if !visited.insert(b) {
                continue;
            }
            let root = me.keys[b].clone().unwrap();
            let sites =
                me.c.sites
                    .iter()
                    .filter(|s| s.block == b)
                    .cloned()
                    .collect::<Vec<_>>();
            for site in sites {
                if site.address == 0 || me.path_ignored(b, &site.path)? {
                    continue;
                }
                let Some((target, _)) = site.target else {
                    continue;
                };
                if me.keys[target].is_some() {
                    continue;
                }
                if site.path.ends_with(".last")
                    || site.path == "[0].prev"
                    || site.path == "[0].next"
                {
                    continue;
                }
                let path = site.path.strip_prefix("[0].").unwrap_or(&site.path);
                if path.ends_with(".first") {
                    let mut current = Some(target);
                    let mut seen = BTreeSet::new();
                    let mut position = 0;
                    while let Some(t) = current {
                        if !seen.insert(t) {
                            return Err(Error::new("diff_identity", "cyclic intrusive list"));
                        }
                        if me.keys[t].is_none() {
                            let label = text(f, t, &["name"])
                                .ok()
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| format!("[{position}]"));
                            me.assign(t, format!("{root}/{path}/{}/{label}", me.ty(t)))?;
                            queue.push_back(t);
                        }
                        current = me.pointer(t, &["next"])?.map(|(b, _)| b);
                        position += 1;
                    }
                } else {
                    let label = if me.pointer_arrays.contains(&b) {
                        me.label(target).unwrap_or_else(|| path.to_owned())
                    } else {
                        path.to_owned()
                    };
                    me.assign(target, format!("{root}/{label}/{}", me.ty(target)))?;
                    queue.push_back(target);
                }
            }
        }
        // Native files also contain redundant detached embedded copies (e.g. AssetShelfSettings).
        // They have no incoming persistent pointers. Match their exact semantic leaf content
        // as a multiset, never by source block index; unknown-pointer records still refuse.
        let mut leaves = BTreeMap::<String, Vec<usize>>::new();
        for i in 0..f.blocks().len() {
            if me.keys[i].is_none()
                && f.blocks()[i].code != *b"DNA1"
                && me
                    .c
                    .sites
                    .iter()
                    .filter(|s| s.block == i)
                    .all(|s| s.address == 0)
            {
                let owner = me.c.owners[i]
                    .and_then(|o| me.keys[o].clone())
                    .unwrap_or_default();
                leaves
                    .entry(format!(
                        "{owner}/DETACHED/{}/{}",
                        me.ty(i),
                        hex(&f.payload(i)?)
                    ))
                    .or_default()
                    .push(i);
            }
        }
        for (key, blocks) in leaves {
            for (occurrence, b) in blocks.into_iter().enumerate() {
                me.assign(b, format!("{key}/indistinguishable[{occurrence}]"))?;
            }
        }
        // Disconnected allocations are not silently paired by numeric block order.
        for i in 0..f.blocks().len() {
            if me.keys[i].is_none() && f.blocks()[i].code != *b"DNA1" {
                me.unresolved.push(json!({"block":i,"type":me.ty(i),"owner":me.c.owners[i].and_then(|o|me.keys[o].clone()),"reason":"unreachable/unanchored allocation; no proven semantic identity","payload_hex":hex(&f.payload(i)?)}));
            }
        }
        Ok(me)
    }
    fn label(&self, b: usize) -> Option<String> {
        for name in ["identifier", "name"] {
            if let Ok(s) = text(self.f, b, &[name])
                && !s.is_empty()
            {
                return Some(s);
            }
            if let Ok(Some((p, _))) = self.pointer(b, &[name])
                && self.f.blocks()[p].sdna_index == 0
                && let Ok(data) = self.f.payload(p)
            {
                let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
                if let Ok(s) = std::str::from_utf8(&data[..end])
                    && !s.is_empty()
                {
                    return Some(s.to_owned());
                }
            }
        }
        None
    }
    fn expand_pointer_arrays(&mut self) -> Result<()> {
        let mut local = BTreeMap::<(Option<usize>, u64), Vec<usize>>::new();
        let mut ids = BTreeMap::<u64, Vec<usize>>::new();
        for (i, b) in self.f.blocks().iter().enumerate() {
            if b.code == *b"DATA" {
                local
                    .entry((self.c.owners[i], b.old_address))
                    .or_default()
                    .push(i);
            } else if crate::typed_edit::owner_code(&b.code) {
                ids.entry(b.old_address).or_default().push(i);
            }
        }
        let original = self.c.sites.clone();
        for site in original {
            if self.path_ignored(site.block, &site.path)? {
                continue;
            }
            let Some((target, _)) = site.target else {
                continue;
            };
            if self.f.blocks()[target].sdna_index != 0 {
                continue;
            }
            let b = self.f.block(site.block)?;
            let size = self.f.schema().lengths[self.f.schema().structs[b.sdna_index].type_index];
            let Some(field) = field_at(self.f, b.sdna_index, site.offset % size) else {
                continue;
            };
            if field.full_name.iter().filter(|c| **c == b'*').count() < 2 {
                continue;
            }
            if !self.pointer_arrays.insert(target) {
                continue;
            }
            let data = self.f.payload(target)?;
            let width = self.f.header().pointer_size;
            if data.len() % width != 0 {
                return Err(Error::new(
                    "diff_pointer_array",
                    "extent not pointer aligned",
                ));
            }
            for i in 0..data.len() / width {
                let address =
                    crate::checked::number(&data, i * width, width, self.f.header().little_endian)?;
                let candidates = local
                    .get(&(self.c.owners[target], address))
                    .or_else(|| ids.get(&address));
                let resolved = candidates.filter(|c| c.len() == 1).map(|c| (c[0], 0));
                self.pointers.insert((target, i * width), resolved);
                self.c.sites.push(crate::file_ops::PointerSite {
                    block: target,
                    owner: self.c.owners[target],
                    offset: i * width,
                    path: format!("[{i}]"),
                    declared_type: site.declared_type.clone(),
                    address,
                    target: resolved,
                    state: if address == 0 {
                        "null"
                    } else if resolved.is_some() {
                        "resolved"
                    } else {
                        "unresolved"
                    },
                });
            }
        }
        Ok(())
    }
    fn ty(&self, b: usize) -> String {
        let block = &self.f.blocks()[b];
        if block.sdna_index == 0 {
            return "opaque".to_owned();
        }
        String::from_utf8_lossy(
            &self.f.schema().types[self.f.schema().structs[block.sdna_index].type_index],
        )
        .into_owned()
    }
    fn assign(&mut self, b: usize, key: String) -> Result<()> {
        if let Some(old) = &self.keys[b] {
            if old != &key {
                return Err(Error::new(
                    "diff_identity",
                    "allocation has conflicting identities",
                ));
            }
            return Ok(());
        }
        if let Some(other) = self.by_key.get(&key)
            && *other != b
        {
            return Err(Error::new(
                "diff_identity",
                format!("nonbijective identity {key}"),
            ));
        }
        self.by_key.insert(key.clone(), b);
        self.keys[b] = Some(key);
        Ok(())
    }
    fn pointer(&self, b: usize, path: &[&str]) -> Result<Option<(usize, usize)>> {
        let steps = path
            .iter()
            .map(|s| crate::value::PathStep::Field(s.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        let location = match self.f.locate_field(b, 0, &steps) {
            Ok(location) => location,
            Err(e) if e.kind == "missing_field" && path == ["next"] => {
                let site = self
                    .c
                    .sites
                    .iter()
                    .find(|s| s.block == b && s.offset == 0 && s.path.ends_with(".next"));
                return site.map(|s| s.target).ok_or_else(|| {
                    Error::new(
                        "diff_identity",
                        format!(
                            "{} {}: no SDNA intrusive next at record start",
                            b,
                            self.ty(b)
                        ),
                    )
                });
            }
            Err(e) => {
                return Err(Error::new(
                    e.kind,
                    format!(
                        "block {} {}, field {}: {}",
                        b,
                        self.ty(b),
                        path.join("."),
                        e.message
                    ),
                ));
            }
        };
        Ok(self.pointers.get(&(b, location.offset)).copied().flatten())
    }
    fn list(&self, b: usize, path: &[&str]) -> Result<Vec<usize>> {
        let mut first = path.to_vec();
        first.push("first");
        let mut cur = self.pointer(b, &first)?.map(|(b, _)| b);
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        while let Some(b) = cur {
            if !seen.insert(b) {
                return Err(Error::new("diff_identity", "list cycle"));
            }
            out.push(b);
            cur = self.pointer(b, &["next"])?.map(|(b, _)| b);
        }
        Ok(out)
    }
    fn path_ignored(&self, b: usize, path: &str) -> Result<bool> {
        let block = self.f.block(b)?;
        if block.sdna_index == 0 {
            return Ok(false);
        }
        let mut si = block.sdna_index;
        for component in path.split('.').skip(1) {
            let name = component.split('[').next().unwrap();
            let st = &self.f.schema().structs[si];
            let Some(field) = st.fields.iter().find(|f| f.name == name.as_bytes()) else {
                return Ok(false);
            };
            let ty = &self.f.schema().types[st.type_index];
            if ignored(ty, &field.name).is_some() {
                return Ok(true);
            }
            if let Some(n) = self.f.schema().by_type.get(&field.type_index) {
                si = *n
            } else {
                return Ok(false);
            }
        }
        Ok(false)
    }
    fn fields(&mut self, b: usize) -> Result<BTreeMap<String, Json>> {
        let mut out = BTreeMap::new();
        let block = self.f.block(b)?;
        let data = self.f.payload(b)?.into_owned();
        if self.pointer_arrays.contains(&b) {
            for i in 0..data.len() / self.f.header().pointer_size {
                let offset = i * self.f.header().pointer_size;
                let target = self.pointers.get(&(b, offset)).copied().flatten();
                out.insert(format!("[{i}]"),json!({"type":"serialized_pointer_array","value":target.and_then(|(b,_)|self.keys[b].clone())}));
            }
            return Ok(out);
        }
        if block.sdna_index == 0 {
            out.insert("opaque_payload".to_owned(),json!({"kind":"opaque_bytes","hex":hex(&data),"pointer_semantics":"not interpreted"}));
            return Ok(out);
        }
        let si = block.sdna_index;
        let size = self.f.schema().lengths[self.f.schema().structs[si].type_index];
        for element in 0..block.count {
            self.walk_fields(
                b,
                si,
                element * size,
                &format!("[{element}]"),
                &data,
                &mut out,
            )?
        }
        Ok(out)
    }
    fn walk_fields(
        &mut self,
        b: usize,
        si: usize,
        base: usize,
        path: &str,
        data: &[u8],
        out: &mut BTreeMap<String, Json>,
    ) -> Result<()> {
        let st = &self.f.schema().structs[si];
        let ty = self.f.schema().types[st.type_index].clone();
        for fi in 0..st.fields.len() {
            let field = &self.f.schema().structs[si].fields[fi];
            let name = String::from_utf8_lossy(&field.name).into_owned();
            let p = format!("{path}.{name}");
            if let Some(reason) = ignored(&ty, &field.name) {
                self.excluded.push(json!({"identity":self.keys[b],"path":p,"container_type":String::from_utf8_lossy(&ty),"reason":reason}));
                continue;
            }
            let nested = self.f.schema().by_type.get(&field.type_index).copied();
            let offset = base + field.offset;
            let n = field.array_len;
            let width = field.element_size;
            let pointer = field.pointer;
            let declared = self.f.schema().types[field.type_index].clone();
            if !pointer && let Some(nested) = nested {
                for i in 0..n {
                    self.walk_fields(
                        b,
                        nested,
                        offset + i * width,
                        &if n == 1 {
                            p.clone()
                        } else {
                            format!("{p}[{i}]")
                        },
                        data,
                        out,
                    )?
                }
                continue;
            }
            for i in 0..if !pointer && matches!(declared.as_slice(), b"char" | b"uchar") && n > 1 {
                1
            } else {
                n
            } {
                let ep =
                    if n == 1 || (!pointer && matches!(declared.as_slice(), b"char" | b"uchar")) {
                        p.clone()
                    } else {
                        format!("{p}[{i}]")
                    };
                let off = offset + i * width;
                let value = if pointer {
                    let address =
                        crate::checked::number(data, off, width, self.f.header().little_endian)?;
                    if address == 0 {
                        Json::Null
                    } else if let Some(Some((target, byte_offset))) = self.pointers.get(&(b, off)) {
                        match &self.keys[*target] {
                            Some(k) => json!({"target":k,"byte_offset":byte_offset}),
                            None => {
                                json!({"unresolved_identity":true,"target_type":self.ty(*target)})
                            }
                        }
                    } else {
                        self.unresolved.push(json!({"identity":self.keys[b],"path":ep,"reason":"non-null persistent pointer unresolved/ambiguous"}));
                        json!({"unresolved_pointer":true,"declared_type":String::from_utf8_lossy(&declared)})
                    }
                } else {
                    let len = if matches!(declared.as_slice(), b"char" | b"uchar") && n > 1 {
                        n * width
                    } else {
                        width
                    };
                    let bytes = data
                        .get(off..off + len)
                        .ok_or_else(|| Error::new("diff_extent", "field beyond allocation"))?;
                    primitive(&declared, bytes, self.f.header().little_endian)
                };
                out.insert(
                    ep,
                    json!({"type":String::from_utf8_lossy(&declared),"value":value}),
                );
            }
        }
        Ok(())
    }
}
fn field_at(f: &BlendFile, si: usize, offset: usize) -> Option<&crate::schema::Field> {
    for field in &f.schema().structs[si].fields {
        if offset < field.offset || offset >= field.offset + field.size {
            continue;
        }
        if field.pointer {
            return Some(field);
        }
        if let Some(nested) = f.schema().by_type.get(&field.type_index) {
            return field_at(f, *nested, (offset - field.offset) % field.element_size);
        }
    }
    None
}
fn primitive(ty: &[u8], bytes: &[u8], little: bool) -> Json {
    let value = if ty == b"char" && bytes.len() > 1 {
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        std::str::from_utf8(&bytes[..end])
            .ok()
            .map(|s| json!({"text":s,"bytes_hex":hex(bytes)}))
    } else {
        None
    };
    if let Some(v) = value {
        return v;
    }
    let mut padded = [0u8; 8];
    if bytes.len() <= 8 {
        if little {
            padded[..bytes.len()].copy_from_slice(bytes)
        } else {
            padded[8 - bytes.len()..].copy_from_slice(bytes)
        }
        let bits = if little {
            u64::from_le_bytes(padded)
        } else {
            u64::from_be_bytes(padded)
        };
        if ty == b"float" && bytes.len() == 4 {
            return json!({"float":f32::from_bits(bits as u32),"bits_hex":hex(bytes)});
        }
        if ty == b"double" && bytes.len() == 8 {
            return json!({"double":f64::from_bits(bits),"bits_hex":hex(bytes)});
        }
        return json!({"unsigned_decimal":bits.to_string(),"bytes_hex":hex(bytes)});
    }
    json!({"bytes_hex":hex(bytes)})
}
/// Every differing field is returned. Nonbijective/unresolved identities prevent
/// completeness; opaque payload equality is byte equality, not a semantic waiver.
pub fn compare(a: &Path, b: &Path) -> Result<Json> {
    let left = BlendFile::open(a)?;
    let right = BlendFile::open(b)?;
    let mut report = json!({"schema":"oxblend.diff/v1","read_only":true,"left":a,"right":b,"matching":"ID type+name+library; nodes tree/name/type; sockets node/direction/identifier; links semantic endpoints; other allocations persistent owning field/list","matched_allocations":0,"node_identifier_bijection":[],"differences":[],"difference_count":0,"complete":true,"unresolved":{"left":[],"right":[]},"explicit_ignored_fields":{"left":[],"right":[]},"equal":true,"opaque_contract":"all anchored opaque allocations compared as bytes, no pointer/semantic interpretation","native_contract":"AMBER-3 requires actual saved FileGlobal hash68609be8e23b/version503.25; these are compared fields, never inferred from filenames"});
    // Physical equality proves equality even when semantic identities cannot be
    // reconstructed. Parse both inputs first: this is not a malformed-file bypass.
    if byte_equal(a, b)? {
        report["matching"] =
            json!("physical byte equality; no semantic identity reconstruction needed");
        report["matched_allocations"] =
            json!(left.blocks().iter().filter(|b| b.code != *b"DNA1").count());
        return Ok(report);
    }
    let mut l = Inventory::new(&left)?;
    let mut r = Inventory::new(&right)?;
    let keys = l
        .by_key
        .keys()
        .chain(r.by_key.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut differences = Vec::new();
    let mut nodes = Vec::new();
    let mut matched = 0;
    for key in keys {
        let li = l.by_key.get(&key).copied();
        let ri = r.by_key.get(&key).copied();
        let lf = li.map(|i| l.fields(i)).transpose()?;
        let rf = ri.map(|i| r.fields(i)).transpose()?;
        if let (Some(li), Some(ri)) = (li, ri) {
            matched += 1;
            if l.ty(li) == "bNode" && r.ty(ri) == "bNode" {
                nodes.push(json!({"identity":key,"old_identifier":uint(&left,li,&["identifier"])? as u32 as i32,"new_identifier":uint(&right,ri,&["identifier"])? as u32 as i32,"identity_basis":"matched tree ID type/name/library + unique node name/type","animation_path_basis":"all serialized FCurve RNA paths and DriverTarget fields compared without rewriting"}));
            }
        }
        let fields = lf
            .iter()
            .flat_map(|m| m.keys())
            .chain(rf.iter().flat_map(|m| m.keys()))
            .cloned()
            .collect::<BTreeSet<_>>();
        for field in fields {
            let old = lf.as_ref().and_then(|m| m.get(&field));
            let new = rf.as_ref().and_then(|m| m.get(&field));
            if old != new {
                differences.push(json!({"identity":key,"field":field,"old":old,"new":new,"old_present":old.is_some(),"new_present":new.is_some(),"impact":"UNASSESSED — receiving evidence required; not waived by pixel equality"}));
            }
        }
    }
    let mut by_tree = BTreeMap::<String, (BTreeSet<i64>, BTreeSet<i64>)>::new();
    for n in &nodes {
        let tree = n["identity"]
            .as_str()
            .unwrap()
            .split("/NODE/")
            .next()
            .unwrap()
            .to_owned();
        let (a, b) = by_tree.entry(tree).or_default();
        if !a.insert(n["old_identifier"].as_i64().unwrap())
            || !b.insert(n["new_identifier"].as_i64().unwrap())
        {
            return Err(Error::new(
                "diff_identity",
                "node identifier map is not bijective within its tree",
            ));
        }
    }
    let complete = l.unresolved.is_empty() && r.unresolved.is_empty();
    report["matched_allocations"] = json!(matched);
    report["node_identifier_bijection"] = json!(nodes);
    report["difference_count"] = json!(differences.len());
    report["equal"] = json!(if complete {
        Some(differences.is_empty())
    } else {
        None
    });
    report["differences"] = json!(differences);
    report["complete"] = json!(complete);
    report["unresolved"] = json!({"left":l.unresolved,"right":r.unresolved});
    report["explicit_ignored_fields"] = json!({"left":l.excluded,"right":r.excluded});
    Ok(report)
}
fn byte_equal(a: &Path, b: &Path) -> Result<bool> {
    use std::io::{BufRead, BufReader};
    let mut a = BufReader::new(std::fs::File::open(a).map_err(crate::edit_output::io)?);
    let mut b = BufReader::new(std::fs::File::open(b).map_err(crate::edit_output::io)?);
    loop {
        let left = a.fill_buf().map_err(crate::edit_output::io)?;
        let right = b.fill_buf().map_err(crate::edit_output::io)?;
        if left.is_empty() || right.is_empty() {
            return Ok(left.is_empty() && right.is_empty());
        }
        let n = left.len().min(right.len());
        if left[..n] != right[..n] {
            return Ok(false);
        }
        a.consume(n);
        b.consume(n);
    }
}
