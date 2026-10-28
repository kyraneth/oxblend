//! Read-only SDNA field comparison. Identities, never allocator addresses.
use crate::file_ops::graph::{text, uint};
use crate::file_ops::{COMPACT_EXPANDED_TEMPLATE, CompactSite, collect_compact_base, compact_path};
use crate::value::hex;
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

/// Invocation-frozen scratch placement. No decoded cache survives the operation.
#[derive(Debug, Clone)]
pub struct CompareOptions {
    pub temp_root: PathBuf,
}
impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            temp_root: std::env::temp_dir(),
        }
    }
}

// Keep the original descriptor even when decoding produces an unnamed spill.
// Checking the spill alone cannot detect mutation of its compressed source.
struct Capture {
    file: std::fs::File,
    signature: crate::fetch::CaptureSignature,
}
impl Capture {
    fn open(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(crate::edit_output::io)?;
        let signature =
            crate::fetch::capture_signature(&file.metadata().map_err(crate::edit_output::io)?);
        Ok(Self { file, signature })
    }
    fn check(&self) -> Result<()> {
        if crate::fetch::capture_signature(&self.file.metadata().map_err(crate::edit_output::io)?)
            != self.signature
        {
            return Err(Error::new(
                "input_changed",
                "captured diff input changed during comparison",
            ));
        }
        Ok(())
    }
    fn parse(&self, path: &Path, options: &CompareOptions) -> Result<BlendFile> {
        self.check()?;
        let result = crate::diagnostics::opened(path, || {
            BlendFile::from_backing(crate::input::Backing::open_file(
                self.file.try_clone().map_err(crate::edit_output::io)?,
                &options.temp_root,
            )?)
            .map(|reader| reader.with_source_path(path))
        });
        self.check()?;
        result
    }
}

/// Exact incomplete comparison projection shared by CLI and language bindings.
/// A returned engine error is not a completed comparison or an equality claim.
pub fn failure_report(a: &Path, b: &Path, error: &Error) -> Json {
    json!({"schema":"oxblend.diff/v1", "read_only":true,
        "left":a.to_str(), "right":b.to_str(),
        "left_path_hex":hex(a.as_os_str().as_encoded_bytes()),
        "right_path_hex":hex(b.as_os_str().as_encoded_bytes()),
        "equal":null, "complete":false, "difference_count":0, "differences":[],
        "errors":[{"kind":error.kind,"message":error.message}]})
}

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
// Stable source-local ranges into the compact edge store, built once after array
// expansion. Keeps base census order (and therefore v1 report/traversal order)
// unchanged. This plus `edges` is the single lookup authority; there is no
// separate BTreeMap<(block, offset), target>.
struct SourceEdges {
    offsets: Vec<usize>,
    edges: Vec<usize>,
}
impl SourceEdges {
    fn new(blocks: usize, sites: &[CompactSite]) -> Self {
        let mut offsets = vec![0; blocks + 1];
        for site in sites {
            offsets[site.block + 1] += 1;
        }
        for b in 0..blocks {
            offsets[b + 1] += offsets[b];
        }
        let mut next = offsets[..blocks].to_vec();
        let mut edges = vec![0; sites.len()];
        for (i, site) in sites.iter().enumerate() {
            edges[next[site.block]] = i;
            next[site.block] += 1;
        }
        Self { offsets, edges }
    }
    fn range(&self, block: usize) -> std::ops::Range<usize> {
        self.offsets[block]..self.offsets[block + 1]
    }
}
struct Inventory<'a> {
    f: &'a BlendFile,
    owners: Vec<Option<usize>>,
    edges: Vec<CompactSite>,
    adjacency: SourceEdges,
    keys: Vec<Option<String>>,
    by_key: BTreeMap<String, usize>,
    unresolved: Vec<Json>,
    excluded: Vec<Json>,
    pointer_arrays: BTreeSet<usize>,
}
impl<'a> Inventory<'a> {
    fn find_edge(&self, block: usize, offset: usize) -> Option<usize> {
        if block >= self.adjacency.offsets.len().saturating_sub(1) {
            return None;
        }
        let range = self.adjacency.range(block);
        let order = &self.adjacency.edges[range];
        // Lower bound on offset; per-block order is sorted by construction.
        let mut lo = 0usize;
        let mut hi = order.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.edges[order[mid]].offset < offset {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        // Last-wins among duplicates: scan forward to the final equal offset.
        let mut found = None;
        let mut i = lo;
        while i < order.len() && self.edges[order[i]].offset == offset {
            found = Some(order[i]);
            i += 1;
        }
        found
    }
    fn target_at(&self, block: usize, offset: usize) -> Option<(usize, usize)> {
        self.find_edge(block, offset)
            .and_then(|idx| self.edges[idx].target)
    }
    fn range_is_null(&self, block: usize) -> bool {
        if block >= self.adjacency.offsets.len().saturating_sub(1) {
            return true;
        }
        let range = self.adjacency.range(block);
        range
            .clone()
            .all(|pos| self.edges[self.adjacency.edges[pos]].address == 0)
    }
    fn expand_compact(&mut self, base_len: usize) -> Result<()> {
        let mut local = BTreeMap::<(Option<usize>, u64), Vec<usize>>::new();
        let mut ids = BTreeMap::<u64, Vec<usize>>::new();
        for (i, b) in self.f.blocks().iter().enumerate() {
            if b.code == *b"DATA" {
                local
                    .entry((self.owners[i], b.old_address))
                    .or_default()
                    .push(i);
            } else if crate::typed_edit::owner_code(&b.code) {
                ids.entry(b.old_address).or_default().push(i);
            }
        }
        for parent_idx in 0..base_len {
            let site = self.edges[parent_idx];
            let site_path = compact_path(self.f, &self.edges, parent_idx)?;
            if self.path_ignored(site.block, &site_path)? {
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
                    .get(&(self.owners[target], address))
                    .or_else(|| ids.get(&address));
                let resolved = candidates.filter(|cc| cc.len() == 1).map(|cc| (cc[0], 0));
                self.edges.push(CompactSite {
                    block: target,
                    offset: i * width,
                    address,
                    target: resolved,
                    state: if address == 0 {
                        "null"
                    } else if resolved.is_some() {
                        "resolved"
                    } else {
                        "unresolved"
                    },
                    template: COMPACT_EXPANDED_TEMPLATE,
                    record: i,
                    parent: parent_idx,
                });
            }
        }
        Ok(())
    }
    fn new(f: &'a BlendFile) -> Result<Self> {
        // Unified base edges come from file_ops and retain qualify_read.
        let (owners, base_edges, _opaque) = collect_compact_base(f)?;
        let base_len = base_edges.len();
        let mut me = Self {
            f,
            owners,
            edges: base_edges,
            adjacency: SourceEdges {
                offsets: vec![0; f.blocks().len() + 1],
                edges: Vec::new(),
            },
            keys: vec![None; f.blocks().len()],
            by_key: BTreeMap::new(),
            unresolved: Vec::new(),
            excluded: Vec::new(),
            pointer_arrays: BTreeSet::new(),
        };
        me.expand_compact(base_len)?;
        me.adjacency = SourceEdges::new(f.blocks().len(), &me.edges);
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
                && let Some(owner) = me.owners[i]
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
                let owner = me.owners[i]
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
            for pos in me.adjacency.range(b) {
                let edge_idx = me.adjacency.edges[pos];
                let site = me.edges[edge_idx];
                if site.address == 0 {
                    continue;
                }
                let site_path = compact_path(f, &me.edges, edge_idx)?;
                if me.path_ignored(b, &site_path)? {
                    continue;
                }
                let Some((target, _)) = site.target else {
                    continue;
                };
                if me.keys[target].is_some() {
                    continue;
                }
                if site_path.ends_with(".last")
                    || site_path == "[0].prev"
                    || site_path == "[0].next"
                {
                    continue;
                }
                let path_binding = site_path
                    .strip_prefix("[0].")
                    .unwrap_or(&site_path)
                    .to_owned();
                let path = path_binding.as_str();
                if path.ends_with(".first") {
                    // Release the census borrow before assigning list identities.
                    let list_root = format!("{root}/{path}");
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
                            me.assign(t, format!("{list_root}/{}/{label}", me.ty(t)))?;
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
            if me.keys[i].is_none() && f.blocks()[i].code != *b"DNA1" && me.range_is_null(i) {
                let owner = me.owners[i]
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
                me.unresolved.push(json!({"block":i,"type":me.ty(i),"owner":me.owners[i].and_then(|o|me.keys[o].clone()),"reason":"unreachable/unanchored allocation; no proven semantic identity","payload_hex":hex(&f.payload(i)?)}));
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
    // Legacy expand_pointer_arrays removed: expansion now lives in expand_compact
    // above, using the same compact edge form plus parent handles. No owned
    // path/type retained and no duplicate target map is built.
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
                let positions: Vec<usize> = self.adjacency.range(b).collect();
                let mut slot: Option<Option<(usize, usize)>> = None;
                for pos in positions {
                    let idx = self.adjacency.edges[pos];
                    if self.edges[idx].offset != 0 {
                        continue;
                    }
                    let p = compact_path(self.f, &self.edges, idx)?;
                    if p.ends_with(".next") {
                        slot = Some(self.edges[idx].target);
                        break;
                    }
                }
                return slot.ok_or_else(|| {
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
        Ok(self.target_at(b, location.offset))
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
                let target = self.target_at(b, offset);
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
                    } else if let Some((target, byte_offset)) = self.target_at(b, off) {
                        match &self.keys[target] {
                            Some(k) => json!({"target":k,"byte_offset":byte_offset}),
                            None => {
                                json!({"unresolved_identity":true,"target_type":self.ty(target)})
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
    compare_with_options(a, b, &CompareOptions::default())
}

/// Compare captured generations using one explicitly chosen temporary root.
pub fn compare_with_options(a: &Path, b: &Path, options: &CompareOptions) -> Result<Json> {
    let lc = Capture::open(a)?;
    let rc = Capture::open(b)?;
    let left = lc.parse(a, options)?;
    let right = rc.parse(b, options)?;
    let result = compare_captured(a, b, &left, &right);
    // These guards also wrap the inventory's range reads on the unequal path,
    // and take precedence over a report or a read error from a mixed generation.
    lc.check()?;
    rc.check()?;
    left.data.check_source()?;
    right.data.check_source()?;
    result
}

fn compare_captured(a: &Path, b: &Path, left: &BlendFile, right: &BlendFile) -> Result<Json> {
    let mut report = json!({"schema":"oxblend.diff/v1","read_only":true,"left":a,"right":b,"matching":"ID type+name+library; nodes tree/name/type; sockets node/direction/identifier; links semantic endpoints; other allocations persistent owning field/list","matched_allocations":0,"node_identifier_bijection":[],"differences":[],"difference_count":0,"complete":true,"unresolved":{"left":[],"right":[]},"explicit_ignored_fields":{"left":[],"right":[]},"equal":true,"opaque_contract":"all anchored opaque allocations compared as bytes, no pointer/semantic interpretation","native_contract":"AMBER-3 requires actual saved FileGlobal hash68609be8e23b/version503.25; these are compared fields, never inferred from filenames"});
    // Both complete parses precede equality, including for identical malformed
    // inputs. Compare the captured decoded streams, never reopen container paths.
    if decoded_equal(left, right)? {
        crate::diagnostics::point(
            "diff.work",
            json!({"inventories":0,"field_walked_allocations":0}),
        );
        report["matching"] =
            json!("decoded byte equality; no semantic identity reconstruction needed");
        report["matched_allocations"] =
            json!(left.blocks().iter().filter(|b| b.code != *b"DNA1").count());
        return Ok(report);
    }
    let mut l = Inventory::new(left)?;
    let mut r = Inventory::new(right)?;
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
                nodes.push(json!({"identity":key,"old_identifier":uint(left,li,&["identifier"])? as u32 as i32,"new_identifier":uint(right,ri,&["identifier"])? as u32 as i32,"identity_basis":"matched tree ID type/name/library + unique node name/type","animation_path_basis":"all serialized FCurve RNA paths and DriverTarget fields compared without rewriting"}));
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
#[cfg(test)]
mod captured_equality_tests;

#[cfg(test)]
mod source_edges_tests {
    use super::SourceEdges;
    use crate::file_ops::{COMPACT_BASE_PARENT, COMPACT_EXPANDED_TEMPLATE, CompactSite};

    #[test]
    fn stable_ranges_include_appended_array_edges_and_empty_blocks() {
        // Array expansion appends sites after the original census; source blocks
        // need not be contiguous. Preserve the original per-source iteration order.
        let sites = [2, 0, 2, 3, 0, 2].map(|block| CompactSite {
            block,
            offset: 0,
            address: 0,
            target: None,
            state: "null",
            template: COMPACT_EXPANDED_TEMPLATE,
            record: 0,
            parent: COMPACT_BASE_PARENT,
        });
        let index = SourceEdges::new(5, &sites);
        for block in 0..5 {
            let expected = sites
                .iter()
                .enumerate()
                .filter(|(_, site)| site.block == block)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            assert_eq!(&index.edges[index.range(block)], expected.as_slice());
        }
        assert_eq!(index.edges.len(), sites.len());
        assert!(SourceEdges::new(0, &[]).edges.is_empty());
        assert!(SourceEdges::new(3, &[]).range(2).is_empty());
    }
}

#[cfg(test)]
mod compact_tests {
    use super::{Inventory, SourceEdges};
    use crate::file_ops::{collect_compact_base, compact_declared, compact_path, pointer_census};
    use std::collections::{BTreeMap, BTreeSet};
    fn fixture(name: &str) -> crate::BlendFile {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/file-ops")
            .join(format!("{name}.blend"));
        crate::BlendFile::open(path).unwrap()
    }
    #[test]
    fn compact_matches_compatibility_counts_paths_types_states() {
        for name in [
            "synthetic-control",
            "synthetic-empty-seed",
            "duplicate-cross-ID-legal",
        ] {
            let f = fixture(name);
            let compat = pointer_census(&f).unwrap();
            assert!(!compat.sites.is_empty(), "{name} has pointer sites");
            let (owners, edges, _opaque) = collect_compact_base(&f).unwrap();
            assert_eq!(edges.len(), compat.sites.len(), "{name} site count");
            assert_eq!(owners, compat.owners, "{name} owners");
            let mut compat_paths = BTreeSet::new();
            let mut compact_paths = BTreeSet::new();
            let mut compat_types = BTreeSet::new();
            let mut compact_types = BTreeSet::new();
            let mut compat_path_len = 0usize;
            let mut compact_path_len = 0usize;
            let mut compat_type_len = 0usize;
            let mut compact_type_len = 0usize;
            let mut compat_states: BTreeMap<&str, usize> = BTreeMap::new();
            let mut compact_states: BTreeMap<&str, usize> = BTreeMap::new();
            for (i, s) in compat.sites.iter().enumerate() {
                compat_paths.insert(s.path.clone());
                compat_types.insert(s.declared_type.clone());
                compat_path_len += s.path.len();
                compat_type_len += s.declared_type.len();
                *compat_states.entry(s.state).or_default() += 1;
                assert_eq!(edges[i].block, s.block, "{name} block {i}");
                assert_eq!(edges[i].offset, s.offset, "{name} offset {i}");
                assert_eq!(edges[i].address, s.address, "{name} address {i}");
                assert_eq!(edges[i].target, s.target, "{name} target {i}");
                assert_eq!(edges[i].state, s.state, "{name} state {i}");
                let p = compact_path(&f, &edges, i).unwrap();
                let t = compact_declared(&f, &edges, i).unwrap();
                assert_eq!(p, s.path, "{name} path {i}");
                assert_eq!(t, s.declared_type.as_slice(), "{name} type {i}");
                compact_paths.insert(p.clone());
                compact_types.insert(t.to_vec());
                compact_path_len += p.len();
                compact_type_len += t.len();
                *compact_states.entry(edges[i].state).or_default() += 1;
            }
            assert_eq!(compact_paths, compat_paths, "{name} distinct paths");
            assert_eq!(compact_types, compat_types, "{name} distinct types");
            assert_eq!(compact_path_len, compat_path_len, "{name} path bytes");
            assert_eq!(compact_type_len, compat_type_len, "{name} type bytes");
            assert_eq!(compact_states, compat_states, "{name} states");
        }
    }
    #[test]
    fn compact_adjacency_sorted_last_wins_and_lookup() {
        for name in ["synthetic-control", "synthetic-empty-seed"] {
            let f = fixture(name);
            let inv = Inventory::new(&f).unwrap();
            for b in 0..f.blocks().len() {
                let range = inv.adjacency.range(b);
                let mut last = None;
                for pos in range.clone() {
                    let off = inv.edges[inv.adjacency.edges[pos]].offset;
                    if let Some(prev) = last {
                        assert!(prev <= off, "{name} block {b} unsorted");
                    }
                    last = Some(off);
                }
            }
            for (idx, e) in inv.edges.iter().enumerate() {
                let found = inv.find_edge(e.block, e.offset).expect("edge lookup");
                let expected = inv
                    .adjacency
                    .range(e.block)
                    .rev()
                    .find_map(|pos| {
                        let cand = inv.adjacency.edges[pos];
                        (inv.edges[cand].offset == e.offset).then_some(cand)
                    })
                    .unwrap();
                assert_eq!(found, expected, "{name} last-wins {idx}");
                assert_eq!(inv.target_at(e.block, e.offset), inv.edges[expected].target);
            }
            assert!(inv.find_edge(usize::MAX / 2, 0).is_none());
            // Inject a duplicate at the greatest offset of one source. This
            // exercises last-wins even when valid schemas have unique slots.
            let mut inv = inv;
            let b = (0..f.blocks().len())
                .find(|&b| !inv.adjacency.range(b).is_empty())
                .unwrap();
            let last = inv.adjacency.range(b).end - 1;
            let original = inv.adjacency.edges[last];
            inv.edges[original].target = None;
            let mut duplicate = inv.edges[original];
            duplicate.target = Some((b, 0));
            let index = inv.edges.len();
            inv.edges.push(duplicate);
            inv.adjacency = SourceEdges::new(f.blocks().len(), &inv.edges);
            assert_eq!(inv.find_edge(b, duplicate.offset), Some(index));
            assert_eq!(inv.target_at(b, duplicate.offset), Some((b, 0)));
        }
    }
    #[test]
    fn compact_expansion_uses_parent_handles() {
        let f = fixture("synthetic-control");
        let compat = pointer_census(&f).unwrap();
        let (_, mut edges, _) = collect_compact_base(&f).unwrap();
        assert!(!edges.is_empty());
        // A direct handle contract, deliberately independent of whether this
        // fixture has an admitted pointer array. Real expansion is exercised
        // through the packaged CLI qualification pair.
        let parent = edges[0];
        let index = edges.len();
        let record = 2;
        edges.push(crate::file_ops::CompactSite {
            block: parent.block,
            offset: record * f.header().pointer_size,
            address: 0,
            target: None,
            state: "null",
            template: crate::file_ops::COMPACT_EXPANDED_TEMPLATE,
            record,
            parent: 0,
        });
        assert_eq!(compact_path(&f, &edges, index).unwrap(), "[2]");
        assert_eq!(
            compact_declared(&f, &edges, index).unwrap(),
            compat.sites[0].declared_type
        );
    }
    // Explicit qualification input, not a shipped CLI option or a product limit.
    #[test]
    #[ignore = "requires OXBLEND_QUAL_INPUT on the qualification host"]
    fn compact_retention_accounting() {
        let name = std::env::var("OXBLEND_QUAL_INPUT").expect("qualification input path");
        {
            let f = crate::BlendFile::open(&name).unwrap();
            let compat = pointer_census(&f).unwrap();
            let (_owners, base, _opaque) = collect_compact_base(&f).unwrap();
            let inv = Inventory::new(&f).unwrap();
            let n = compat.sites.len();
            assert_eq!(base.len(), n, "{name} base count preserved");
            let sum_path: usize = compat.sites.iter().map(|s| s.path.len()).sum();
            let sum_type: usize = compat.sites.iter().map(|s| s.declared_type.len()).sum();
            let distinct_paths: BTreeSet<&str> =
                compat.sites.iter().map(|s| s.path.as_str()).collect();
            let distinct_types: BTreeSet<&[u8]> = compat
                .sites
                .iter()
                .map(|s| s.declared_type.as_slice())
                .collect();
            let inline_old = std::mem::size_of::<crate::file_ops::PointerSite>() * n;
            let inline_new_base = std::mem::size_of::<crate::file_ops::CompactSite>() * base.len();
            let inline_new_full =
                std::mem::size_of::<crate::file_ops::CompactSite>() * inv.edges.len();
            // Count complete initialized shared programs, not just path/type
            // contents. These already existed in the prefix's input schema;
            // charging them again here is conservative. Arc's allocation has
            // two usize reference counts in addition to its layout slice.
            let mut shared_programs = 0usize;
            let mut shared_templates = 0usize;
            let mut shared_bytes = 0usize;
            for cell in &f.schema().pointer_programs {
                if let Some(Ok(program)) = cell.get() {
                    shared_programs += 1;
                    shared_templates += program.len();
                    shared_bytes += 2 * std::mem::size_of::<usize>()
                        + program.len() * std::mem::size_of::<crate::file_ops::PointerLayout>();
                    for t in program.iter() {
                        shared_bytes += t.path.len()
                            + t.declared_type.len()
                            + t.parent_type.len()
                            + t.field_name.len();
                        if let Some((ty, field)) = &t.list_role {
                            shared_bytes += ty.len() + field.len();
                        }
                    }
                }
            }
            let mut states = BTreeMap::<&str, usize>::new();
            for (i, old) in compat.sites.iter().enumerate() {
                let new = base[i];
                assert_eq!(
                    (new.block, new.offset, new.address, new.target, new.state),
                    (old.block, old.offset, old.address, old.target, old.state)
                );
                assert_eq!(compact_path(&f, &base, i).unwrap(), old.path);
                assert_eq!(compact_declared(&f, &base, i).unwrap(), old.declared_type);
                *states.entry(new.state).or_default() += 1;
            }
            let adjacency_bytes = inv.adjacency.offsets.len() * std::mem::size_of::<usize>()
                + inv.adjacency.edges.len() * std::mem::size_of::<usize>();
            let total_new = inline_new_full + adjacency_bytes + shared_bytes;
            assert!(n > 0, "qualification input has base census sites");
            let per_site_new = total_new as f64 / n as f64;
            // VERIFICATION excluded Vec capacity slack. Report that same
            // logical account and, separately, the stronger allocated-capacity
            // account, both divided by the preserved base census count.
            let total_capacity = std::mem::size_of::<crate::file_ops::CompactSite>()
                * inv.edges.capacity()
                + adjacency_bytes
                + shared_bytes;
            let capacity_per_site = total_capacity as f64 / n as f64;
            let total_old_compat = inline_old + sum_path + sum_type;
            let per_site_old_compat = total_old_compat as f64 / n.max(1) as f64;
            println!(
                "S3_ACCOUNTING {}",
                serde_json::json!({
                    "input": name, "base_sites": n, "total_edges": inv.edges.len(),
                "expanded_sites": inv.edges.len() - base.len(),
                    "distinct_paths": distinct_paths.len(), "distinct_types": distinct_types.len(),
                    "path_bytes": sum_path, "type_bytes": sum_type, "states": states,
                    "inline_old": inline_old, "site_inline_bytes": std::mem::size_of::<crate::file_ops::CompactSite>(),
                    "inline_new_base": inline_new_base, "inline_new_full": inline_new_full,
                    "shared_programs": shared_programs, "shared_templates": shared_templates,
                    "shared_bytes": shared_bytes, "adjacency_bytes": adjacency_bytes,
                    "total_new": total_new, "bytes_per_site": per_site_new,
                "total_capacity": total_capacity, "capacity_bytes_per_site": capacity_per_site,
                    "old_bytes_per_site_without_map": per_site_old_compat,
                    "edge_capacity": inv.edges.capacity()
                })
            );
            assert!(
                per_site_new < 184.282,
                "{name} compact per-site {per_site_new:.3} must stay below VERIFICATION 184.282"
            );
            assert!(
                capacity_per_site < 184.282,
                "allocated compact capacity per base site"
            );
        }
    }
}

fn decoded_equal(a: &BlendFile, b: &BlendFile) -> Result<bool> {
    // Production callers parse two separate captures, even for the same path.
    // Never nest scans of the same File backing: try_clone shares its offset.
    use std::io::{BufRead, BufReader};
    a.data.scan(|a| {
        b.data.scan(|b| {
            let mut a = BufReader::new(a);
            let mut b = BufReader::new(b);
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
        })
    })
}
