use super::{PointerLayout, pointer_layout};
use crate::checked::{add, mul, number};
use crate::edit_output::{AtomicOutput, Durability, io};
use crate::{BlendFile, Error, Result};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io::Write;
use std::path::Path;

#[path = "operations.rs"]
mod operations;

#[derive(Clone)]
pub(crate) struct Record {
    pub code: [u8; 4],
    pub sdna: usize,
    pub count: usize,
    pub data: Vec<u8>,
    pub address: u64,
    pub owner: usize,
    pub pointers: Vec<PointerLayout>,
    pub alive: bool,
}

pub(crate) struct CompositorRetarget {
    pub nodes: BTreeSet<usize>,
    pub clear_rlayers: bool,
    pub source_scene: usize,
    pub image: Option<usize>,
}
pub(crate) struct Graph<'a> {
    pub schema: &'a BlendFile,
    pub records: Vec<Record>,
    pub operations: Vec<serde_json::Value>,
    next: u64,
}
fn steps(names: &[&str]) -> Vec<crate::value::PathStep> {
    names
        .iter()
        .map(|s| crate::value::PathStep::Field(s.as_bytes().to_vec()))
        .collect()
}
pub(crate) fn text(f: &BlendFile, b: usize, p: &[&str]) -> Result<String> {
    Ok(f.get(b, 0, &steps(p), true)?.utf8()?.to_owned())
}
pub(crate) fn uint(f: &BlendFile, b: usize, p: &[&str]) -> Result<u64> {
    let v = f.get(b, 0, &steps(p), false)?;
    match v {
        crate::value::Value::Pointer(n) | crate::value::Value::Unsigned(n) => Ok(n),
        crate::value::Value::Signed(n) => Ok(n as u64),
        _ => Err(Error::new("type", "integer/pointer required")),
    }
}

pub(crate) fn discarded(f: &BlendFile, b: usize, p: &PointerLayout) -> Result<bool> {
    f.block(b)?;
    // The role owner qualifies zero-element array lifetimes before any
    // address lookup. An empty bake handle is not a persistent child.
    if f.header().version == 502 && super::roles::empty_allocation(f, b, p)? {
        return Ok(true);
    }
    if p.parent_type == b"bNode" && p.field_name == b"panel_states_array" {
        let si = f
            .schema()
            .structs
            .iter()
            .position(|s| f.schema().types[s.type_index] == b"bNode")
            .ok_or_else(|| Error::new("storage_profile", "node schema absent"))?;
        let count = f.schema().structs[si]
            .fields
            .iter()
            .find(|q| q.name == b"num_panel_states")
            .ok_or_else(|| Error::new("storage_profile", "panel count absent"))?;
        return Ok(number(
            &f.payload(b)?,
            add(p.parent_offset, count.offset)?,
            count.size,
            f.header().little_endian,
        )? == 0);
    }
    // Import/construction policy only: existing-file sparse editors preserve
    // original allocations and do not call this profile to clear their bytes.
    // These slots were observed in independently saved 5.2.1 files: their
    // runtime handles/cache allocations are absent from the saved DATA runs.
    let parent = p.parent_type.as_slice();
    let field = p.field_name.as_slice();
    // Independently reopened native 4.5.8 seed: key configurations point to
    // preferences/runtime objects absent from its saved DATA runs. Likewise
    // the timer list and Editing.seqbasep are rebuilt editor context, not
    // persistent child allocations to import.
    if p.list_role.as_ref().is_some_and(|(owner, list)| {
        owner == b"wmWindowManager" && matches!(list.as_slice(), b"keyconfigs" | b"timers")
    }) {
        return Ok(true);
    }
    // Blender 4.5 ViewLayer.stats is a SceneStats runtime handle:
    // original block6871 retained an address with no saved BHead. Reset only
    // when importing; unchanged original records retain their exact bytes.
    if f.header().version == 405
        && parent == b"ViewLayer"
        && field == b"stats"
        && p.declared_type == b"SceneStats"
        && p.pointer_depth == 1
    {
        return Ok(f
            .schema()
            .structs
            .iter()
            .find(|s| f.schema().types[s.type_index] == b"ViewLayer")
            .and_then(|s| s.fields.iter().find(|q| q.name == b"stats"))
            .is_some_and(|q| q.pointer && q.array_len == 1 && q.size == f.header().pointer_size));
    }
    // Blender 4.5 Cryptomatte saves these cache handles without layer DATA.
    // Its persistent `entries` list is distinct and must remain untouched.
    if p.list_role
        .as_ref()
        .is_some_and(|(owner, list)| owner == b"NodeCryptomatte_Runtime" && list == b"layers")
        && matches!(field, b"first" | b"last")
    {
        return Ok(true);
    }
    if field == b"runtime"
        && matches!(
            parent,
            b"ID"
                | b"Scene"
                | b"bNodeTree"
                | b"bNode"
                | b"bNodeSocket"
                | b"bNodeTreeInterface"
                | b"Image"
                | b"PreviewImage"
                | b"ViewLayer"
                | b"Paint"
                | b"Editing"
                | b"wmWindow"
                | b"WorkSpace"
                | b"SpaceButs"
                | b"SpaceOops"
                | b"SpaceNode"
                | b"SpaceSpreadsheet"
                | b"SpreadsheetColumn"
                | b"SpaceText"
                | b"SpaceFile"
                | b"wmXrData"
                | b"wmWindowManager"
                | b"ARegion"
                | b"Collection"
                | b"Object"
                | b"Mesh"
                | b"AttributeStorage"
        )
    {
        return Ok(true);
    }
    Ok(matches!(
        (parent, field),
        (b"bNodeTree" | b"bNode" | b"bNodeSocket", b"typeinfo")
            | (b"bNodeTree", b"owner_id")
            | (b"IDPropertyData", b"children_map")
            | (b"AttributeArray", b"sharing_info")
            | (b"ID", b"newid" | b"orig_id" | b"py_instance")
            | (b"bNodeStack", b"data")
            | (b"Image", b"rr" | b"cache" | b"gputexture")
            | (b"Editing", b"seqbasep" | b"displayed_channels")
            | (b"EditingRuntime" | b"Editing_Runtime", b"strip_lookup")
            | (b"ViewLayer", b"object_bases_hash")
            | (
                b"wmWindowManager",
                b"defaultconf" | b"addonconf" | b"userconf" | b"message_bus"
            )
            | (b"Scene", b"depsgraph_hash")
            | (b"wmWindowManager", b"autosavetimer")
            | (b"bScreen", b"context")
            | (b"CurveMap", b"table" | b"premultable")
            | (b"CurveProfile", b"table")
    ))
}
impl<'a> Graph<'a> {
    fn admit_raw(
        &mut self,
        f: &BlendFile,
        b: usize,
        p: &PointerLayout,
        target: usize,
    ) -> Result<()> {
        // The shared role validator is the owning boundary for an opaque
        // allocation's element/extent contract. Never reinterpret sdna=0 as a
        // pointer table merely because its size is pointer-aligned. This also
        // checks PreviewImage.rect's width*height*4 RGBA extent, string lengths,
        // packed bytes and qualified numeric arrays. Unknown void/raw roles
        // and pointer-to-pointer tables refuse there, before any allocation.
        if f.block(target)?.sdna_index != 0 {
            return Err(Error::new(
                "raw_profile",
                "raw admission needs an opaque allocation",
            ));
        }
        if p.pointer_depth != 1 {
            return Err(Error::new(
                "unprofiled_role",
                "opaque pointer tables are not qualified",
            ));
        }
        super::roles::write_qualify(f, b, p, target)?;
        // Role qualification remains enforced here; the owning import's
        // kept_closure_pointer_audit is the public receipt. Do not add a
        // second per-payload operation to the established JSON contract.
        Ok(())
    }

    pub fn new(schema: &'a BlendFile) -> Result<Self> {
        if !schema.header().little_endian {
            return Err(Error::new("file_ops_profile", "little endian required"));
        }
        schema.owner_index()?.admit_write(schema)?;
        Ok(Self {
            schema,
            records: Vec::new(),
            operations: Vec::new(),
            next: 8,
        })
    }

    pub(crate) fn reserve_source_addresses(&mut self) -> Result<()> {
        let mut next = self.next;
        for b in self.schema.blocks() {
            if b.old_address != 0 {
                let end = b
                    .old_address
                    .checked_add(
                        u64::try_from(b.size.max(1))
                            .map_err(|_| Error::new("overflow", "source allocation extent"))?,
                    )
                    .ok_or_else(|| Error::new("overflow", "source allocation address"))?;
                next = next.max(end);
            }
        }
        self.next = next
            .checked_add(7)
            .map(|n| n & !7)
            .ok_or_else(|| Error::new("overflow", "source allocation alignment"))?;
        Ok(())
    }
    pub fn compatible(&self, f: &BlendFile) -> Result<()> {
        f.owner_index()?.admit_write(f)?;
        let dna = |g: &BlendFile| -> Result<Vec<u8>> {
            let i = g
                .blocks()
                .iter()
                .position(|b| b.code == *b"DNA1")
                .ok_or_else(|| Error::new("invalid_sdna", "DNA1"))?;
            Ok(g.payload(i)?.into_owned())
        };
        if dna(self.schema)? != dna(f)?
            || self.schema.header().pointer_size != f.header().pointer_size
            || self.schema.header().file_format != f.header().file_format
        {
            return Err(Error::new(
                "kit_dna",
                "source and constructor DNA/container mismatch",
            ));
        }
        Ok(())
    }
    pub(crate) fn address(&mut self, size: usize) -> Result<u64> {
        let a = self.next;
        let size = u64::try_from(size.max(1)).map_err(|_| Error::new("overflow", "allocation"))?;
        self.next = a
            .checked_add(size)
            .and_then(|v| v.checked_add(7))
            .map(|v| v & !7)
            .ok_or_else(|| Error::new("overflow", "address extent"))?;
        if self.schema.header().pointer_size == 4 && self.next > u32::MAX as u64 {
            return Err(Error::new("overflow", "32-bit address exhaustion"));
        }
        Ok(a)
    }
    pub fn allocate(
        &mut self,
        code: [u8; 4],
        sdna: usize,
        count: usize,
        data: Vec<u8>,
        owner: Option<usize>,
    ) -> Result<usize> {
        let id = self.records.len();
        let address = self.address(data.len())?;

        let mut pointers = Vec::new();
        if sdna != 0 {
            let size = self.schema.schema().lengths[self.schema.schema().structs[sdna].type_index];
            if mul(size, count)? != data.len() {
                return Err(Error::new("record_extent", "new count/size mismatch"));
            }
            super::walk(self.schema, sdna, 0, "[0]", &mut pointers)?;
            let base = pointers.clone();
            for i in 1..count {
                for p in &base {
                    let mut p = p.clone();
                    p.parent_offset = add(p.parent_offset, mul(i, size)?)?;
                    p.offset = add(p.offset, mul(i, size)?)?;
                    pointers.push(p);
                }
            }
        }
        self.records.push(Record {
            code,
            sdna,
            count,
            data,
            address,
            owner: owner.unwrap_or(id),
            pointers,
            alive: true,
        });
        Ok(id)
    }
    pub fn new_typed(&mut self, ty: &str, owner: Option<usize>) -> Result<usize> {
        let sdna = self
            .schema
            .schema()
            .structs
            .iter()
            .position(|s| self.schema.schema().types[s.type_index] == ty.as_bytes())
            .ok_or_else(|| Error::new("constructor_type", ty))?;
        let size = self.schema.schema().lengths[self.schema.schema().structs[sdna].type_index];
        self.allocate(*b"DATA", sdna, 1, vec![0; size], owner)
    }
    pub fn field(&self, id: usize, names: &[&str]) -> Result<(usize, usize)> {
        let r = &self.records[id];
        let mut si = r.sdna;
        let mut base = 0;
        for (i, n) in names.iter().enumerate() {
            let s = &self.schema.schema().structs[si];
            let field = s
                .fields
                .iter()
                .find(|f| f.name == n.as_bytes())
                .ok_or_else(|| {
                    Error::new(
                        "missing_field",
                        format!(
                            "{}.{n}",
                            String::from_utf8_lossy(&self.schema.schema().types[s.type_index])
                        ),
                    )
                })?;
            base = add(base, field.offset)?;
            if i + 1 == names.len() {
                return Ok((base, field.size));
            }
            if field.pointer {
                return Err(Error::new(
                    "pointer_path",
                    "explicit pointer traversal required",
                ));
            }
            si = *self
                .schema
                .schema()
                .by_type
                .get(&field.type_index)
                .ok_or_else(|| Error::new("field_type", "embedded struct required"))?;
        }
        Err(Error::new("path", "empty path"))
    }
    pub fn bytes(&self, id: usize, p: &[&str]) -> Result<&[u8]> {
        let (o, n) = self.field(id, p)?;
        Ok(&self.records[id].data[o..o + n])
    }
    pub fn value(&self, id: usize, p: &[&str]) -> Result<u64> {
        let (o, n) = self.field(id, p)?;
        number(&self.records[id].data, o, n, true)
    }
    pub fn string(&self, id: usize, p: &[&str]) -> Result<String> {
        let b = self.bytes(id, p)?;
        String::from_utf8(b.split(|c| *c == 0).next().unwrap_or_default().to_vec())
            .map_err(|_| Error::new("encoding", "UTF8 selector required"))
    }
    pub fn set_bytes(&mut self, id: usize, p: &[&str], bytes: &[u8]) -> Result<()> {
        let (o, n) = self.field(id, p)?;
        if n != bytes.len() {
            return Err(Error::new("typed_width", "no truncation"));
        }
        self.records[id].data[o..o + n].copy_from_slice(bytes);
        Ok(())
    }
    pub fn set(&mut self, id: usize, p: &[&str], value: u64) -> Result<()> {
        let (_, n) = self.field(id, p)?;
        if !matches!(n, 1 | 2 | 4 | 8) {
            return Err(Error::new("typed_width", "integer width"));
        }
        if n < 8 && value >= (1u64 << (n * 8)) {
            return Err(Error::new("typed_overflow", "integer width"));
        }
        self.set_bytes(id, p, &value.to_le_bytes()[..n])
    }
    pub fn cstring(&mut self, id: usize, p: &[&str], value: &str) -> Result<()> {
        let (_, n) = self.field(id, p)?;
        if value.len() >= n || value.contains('\0') {
            return Err(Error::new("cstring_width", "must fit including terminator"));
        }
        let mut b = vec![0; n];
        b[..value.len()].copy_from_slice(value.as_bytes());
        self.set_bytes(id, p, &b)
    }
    pub fn ptr(&mut self, id: usize, p: &[&str], target: Option<usize>) -> Result<()> {
        let v = target.map_or(0, |t| self.records[t].address);
        self.set(id, p, v)
    }
    pub fn target(&self, id: usize, p: &[&str]) -> Result<Option<usize>> {
        let a = self.value(id, p)?;
        if a == 0 {
            return Ok(None);
        }
        self.records
            .iter()
            .position(|r| r.alive && r.address == a)
            .map(Some)
            .ok_or_else(|| Error::new("dangling_pointer", format!("{id} {p:?} {a}")))
    }
    pub fn ty(&self, id: usize) -> &[u8] {
        let r = &self.records[id];
        &self.schema.schema().types[self.schema.schema().structs[r.sdna].type_index]
    }
    pub fn import(
        &mut self,
        f: &BlendFile,
        roots: &[usize],
        destination: Option<usize>,
        isolated_root: bool,
    ) -> Result<HashMap<usize, usize>> {
        self.import_with_compositor(f, roots, destination, isolated_root, None)
    }
    pub fn import_with_compositor(
        &mut self,
        f: &BlendFile,
        roots: &[usize],
        destination: Option<usize>,
        isolated_root: bool,
        crypto: Option<&CompositorRetarget>,
    ) -> Result<HashMap<usize, usize>> {
        self.compatible(f)?;
        let source_index = f.owner_index()?;
        let mut queue = VecDeque::from(roots.to_vec());
        let mut selected = BTreeSet::new();
        let mut links = HashMap::<usize, Vec<(usize, usize)>>::new();
        let mut clears = HashMap::<usize, Vec<usize>>::new();
        let rootset = roots.iter().copied().collect::<BTreeSet<_>>();
        let mut detached_sockets = BTreeSet::new();
        if isolated_root {
            for &root in roots {
                if f.dna_type_id(root)? == b"bNode" {
                    for direction in ["inputs", "outputs"] {
                        detached_sockets.extend(super::source_list(f, root, &[direction])?);
                    }
                }
            }
        }
        while let Some(b) = queue.pop_front() {
            if !selected.insert(b) {
                continue;
            }
            let block = f.block(b)?;
            let payload = f.payload(b)?;
            let Some(layout) = pointer_layout(f, b)? else {
                if !links.contains_key(&b) && !rootset.contains(&b) {
                    links.insert(b, Vec::new());
                }
                continue;
            };
            for p in layout {
                let a = number(&payload, p.offset, f.header().pointer_size, true)?;
                if a == 0 {
                    if !discarded(f, b, &p)? {
                        super::validate::write_site(f, b, &p, a)?;
                    }
                    continue;
                }
                let isolate = isolated_root
                    && ((rootset.contains(&b)
                        && (matches!(p.path.as_str(), "[0].next" | "[0].prev" | "[0].link")
                            || (f.schema().types
                                [f.schema().structs[block.sdna_index].type_index]
                                == b"bNode"
                                && matches!(p.path.as_str(), "[0].id" | "[0].parent"))))
                        || (detached_sockets.contains(&b) && p.path == "[0].link"));
                if isolate || discarded(f, b, &p)? {
                    clears.entry(b).or_default().push(p.offset);
                    continue;
                }
                let target = super::validate::write_site(f, b, &p, a)
                    .and_then(|target| {
                        target
                            .ok_or_else(|| Error::new("pointer_role", "nonnull target became null"))
                    })
                    .map_err(|e| {
                        Error::new(
                            e.kind,
                            format!(
                                "{} {}: {}",
                                String::from_utf8_lossy(
                                    &f.schema().types
                                        [f.schema().structs[block.sdna_index].type_index]
                                ),
                                p.path,
                                e.message
                            ),
                        )
                    })?;
                if let Some(plan) = crypto {
                    // Compositor-only policy: release qualified Render Layers
                    // Scene edges BEFORE traversal, not after importing Scenes.
                    if plan.clear_rlayers
                        && p.path == "[0].id"
                        && f.dna_type_id(b)? == b"bNode"
                        && text(f, b, &["idname"])? == "CompositorNodeRLayers"
                    {
                        if f.dna_type_id(target)? != b"Scene" {
                            return Err(Error::new(
                                "rlayers_source_id",
                                "planned Render Layers edge is not a Scene reference",
                            ));
                        }
                        clears.entry(b).or_default().push(p.offset);
                        continue;
                    }
                    if plan.nodes.contains(&b) && p.path == "[0].id" {
                        if target != plan.source_scene {
                            return Err(Error::new(
                                "crypto_source_id",
                                "Cryptomatte ID is not the source Tech Scene",
                            ));
                        }
                        clears.entry(b).or_default().push(p.offset);
                        continue;
                    }
                    if plan.image.is_some() && target == plan.source_scene {
                        return Err(Error::new(
                            "crypto_scene_kept_edge",
                            format!("block {b} {} still reaches Tech Scene", p.path),
                        ));
                    }
                }
                if f.block(target)?.sdna_index == 0 {
                    self.admit_raw(f, b, &p, target)?;
                }
                links.entry(b).or_default().push((p.offset, target));
                queue.push_back(target);
            }
        }
        let mut map = HashMap::new();
        for b in &selected {
            let s = f.block(*b)?;
            let id = self.allocate(
                s.code,
                s.sdna_index,
                s.count,
                f.payload(*b)?.into_owned(),
                destination,
            )?;
            map.insert(*b, id);
        }
        for b in &selected {
            let id = map[b];
            let original_owner = source_index.owner(f, *b);
            if self.records[id].code == *b"DATA" {
                self.records[id].owner = original_owner
                    .and_then(|o| map.get(&o).copied())
                    .or(destination)
                    .ok_or_else(|| {
                        Error::new(
                            "closure_owner",
                            format!("direct data block {b} lacks included/destination owner"),
                        )
                    })?;
            }
            for offset in clears.get(b).into_iter().flatten() {
                let n = f.header().pointer_size;
                self.records[id].data[*offset..*offset + n].fill(0);
            }
            for (offset, target) in links.get(b).into_iter().flatten() {
                let a = self.records[map[target]].address;
                let n = f.header().pointer_size;
                self.records[id].data[*offset..*offset + n].copy_from_slice(&a.to_le_bytes()[..n]);
            }
        }
        if let Some(plan) = crypto {
            for source in &plan.nodes {
                let image = plan.image.ok_or_else(|| {
                    Error::new("crypto_retarget", "planned Cryptomatte image missing")
                })?;
                let node = *map.get(source).ok_or_else(|| {
                    Error::new("crypto_retarget", "planned node is not in closure")
                })?;
                self.ptr(node, &["id"], Some(image))?;
                self.set(node, &["custom1"], 1)?;
                self.operations.push(serde_json::json!({"operation":"cryptomatte_scene_to_image_before_import_closure","source_node":source,"node":self.string(node,&["name"])?,"source":"IMAGE","id_image":self.string(image,&["id","name"])?,"original_scene_pointer_released":true,"other_kept_edges_to_original_scene":0,"applies_to":"caller-qualified graph import","new_image_record":image}));
            }
        }
        self.operations.push(serde_json::json!({"operation":"kept_closure_pointer_audit","whole_source_file":false,"source_blocks":f.blocks().len(),"kept_blocks":selected.len(),"remapped_persistent_sites":links.values().map(Vec::len).sum::<usize>(),"explicitly_cleared_sites":clears.values().map(Vec::len).sum::<usize>(),"unresolved_kept_persistent_sites":0,"opaque_policy":"inbound qualified byte/numeric/preview contracts; otherwise refused","source_roots":roots,"audit_before_emit":true}));
        Ok(map)
    }
    pub fn list(&self, owner: usize, p: &[&str]) -> Result<Vec<usize>> {
        let mut first = p.to_vec();
        first.push("first");
        let mut last = p.to_vec();
        last.push("last");
        let mut current = self.target(owner, &first)?;
        let end = self.target(owner, &last)?;
        let mut items = Vec::new();
        let mut seen = BTreeSet::new();
        let mut previous = None;
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(Error::new("list_cycle", "cycle"));
            }
            if self.target(id, &["prev"])? != previous {
                return Err(Error::new("list_symmetry", "prev mismatch"));
            }
            items.push(id);
            previous = current;
            current = self.target(id, &["next"])?;
        }
        if previous != end {
            return Err(Error::new("list_endpoint", "last mismatch"));
        }
        Ok(items)
    }
    pub fn set_list(&mut self, owner: usize, p: &[&str], items: &[usize]) -> Result<()> {
        let mut first = p.to_vec();
        first.push("first");
        let mut last = p.to_vec();
        last.push("last");
        self.ptr(owner, &first, items.first().copied())?;
        self.ptr(owner, &last, items.last().copied())?;
        for (i, id) in items.iter().enumerate() {
            self.ptr(*id, &["prev"], i.checked_sub(1).map(|j| items[j]))?;
            self.ptr(*id, &["next"], items.get(i + 1).copied())?;
        }
        self.list(owner, p)?;
        Ok(())
    }
    pub fn verify(&self) -> Result<serde_json::Value> {
        let addresses = self
            .records
            .iter()
            .filter(|r| r.alive)
            .map(|r| r.address)
            .collect::<BTreeSet<_>>();
        let mut n = 0;
        for (i, r) in self.records.iter().enumerate().filter(|(_, r)| r.alive) {
            if r.code == *b"DATA"
                && (!self.records[r.owner].alive || self.records[r.owner].code == *b"DATA")
            {
                return Err(Error::new("closure_owner", format!("record {i}")));
            }
            for p in &r.pointers {
                let a = number(&r.data, p.offset, self.schema.header().pointer_size, true)?;
                if a != 0 && !addresses.contains(&a) {
                    return Err(Error::new(
                        "dangling_pointer",
                        format!("record {i} {} -> {a:#x}", p.path),
                    ));
                }
                n += 1;
            }
        }
        let snapshot = self.serialized_snapshot()?;
        let check = BlendFile::from_bytes(snapshot)?;
        let census = super::checked_pointer_census(&check)?;
        census.require_closed()?;

        let by_address = check
            .blocks()
            .iter()
            .enumerate()
            .map(|(i, b)| (b.old_address, i))
            .collect::<BTreeMap<_, _>>();
        for r in self
            .records
            .iter()
            .filter(|r| r.alive && r.code == *b"DATA")
        {
            let read_index = *by_address
                .get(&r.address)
                .ok_or_else(|| Error::new("owner_projection", "missing DATA"))?;
            let expected = *by_address
                .get(&self.records[r.owner].address)
                .ok_or_else(|| Error::new("owner_projection", "missing owner"))?;
            if census.owners[read_index] != Some(expected) {
                return Err(Error::new(
                    "owner_projection",
                    "native namespace differs from structural transaction owner",
                ));
            }
        }
        Ok(
            serde_json::json!({"kept_records":self.records.iter().filter(|r|r.alive).count(),"kept_typed_pointer_sites":n,"no_dangling_persistent_pointers":true,"shared_type_role_list_validator":true,"native_owner_projection_verified":true}),
        )
    }
    pub(crate) fn encode_block(&self, r: &Record, out: &mut dyn Write) -> Result<()> {
        out.write_all(&r.code).map_err(io)?;
        let mut put = |v: usize, n: usize| -> Result<()> {
            let v = u64::try_from(v).map_err(|_| Error::new("overflow", "BHead"))?;
            if n == 4 && v > i32::MAX as u64 {
                return Err(Error::new("overflow", "legacy BHead signed width"));
            }
            out.write_all(&v.to_le_bytes()[..n]).map_err(io)
        };
        if self.schema.header().file_format == 1 {
            put(r.sdna, 4)?;
            out.write_all(&r.address.to_le_bytes()).map_err(io)?;
            out.write_all(&(r.data.len() as u64).to_le_bytes())
                .map_err(io)?;
            out.write_all(&(r.count as u64).to_le_bytes()).map_err(io)?;
        } else {
            put(r.data.len(), 4)?;
            out.write_all(&r.address.to_le_bytes()[..self.schema.header().pointer_size])
                .map_err(io)?;
            out.write_all(&(r.sdna as u32).to_le_bytes()).map_err(io)?;
            out.write_all(&(r.count as u32).to_le_bytes()).map_err(io)?;
        }
        out.write_all(&r.data).map_err(io)
    }
    pub fn decoded(&self) -> Result<Vec<u8>> {
        self.verify()?;
        self.serialized_snapshot()
    }
    fn serialized_snapshot(&self) -> Result<Vec<u8>> {
        let mut out = self.schema.serialized_header()?;
        let mut grouped = BTreeMap::<usize, Vec<usize>>::new();
        for (i, r) in self
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.alive && r.code == *b"DATA")
        {
            grouped.entry(r.owner).or_default().push(i);
        }
        for (i, r) in self
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.alive && r.code != *b"DATA")
        {
            self.encode_block(r, &mut out)?;
            for id in grouped.get(&i).into_iter().flatten() {
                self.encode_block(&self.records[*id], &mut out)?;
            }
        }
        let dna = self
            .schema
            .blocks()
            .iter()
            .position(|b| b.code == *b"DNA1")
            .ok_or_else(|| Error::new("invalid_sdna", "DNA"))?;
        let b = self.schema.block(dna)?;
        self.encode_block(
            &Record {
                code: b.code,
                sdna: b.sdna_index,
                count: b.count,
                data: self.schema.payload(dna)?.into_owned(),
                address: 0,
                owner: 0,
                pointers: Vec::new(),
                alive: true,
            },
            &mut out,
        )?;
        self.encode_block(
            &Record {
                code: *b"ENDB",
                sdna: 0,
                count: 0,
                data: Vec::new(),
                address: 0,
                owner: 0,
                pointers: Vec::new(),
                alive: true,
            },
            &mut out,
        )?;
        Ok(out)
    }
    pub fn write(
        &self,
        source: &Path,
        output: &Path,
        compressed: bool,
    ) -> Result<serde_json::Value> {
        let data = self.decoded()?;
        let encoded = if compressed {
            zstd::stream::encode_all(data.as_slice(), 0).map_err(io)?
        } else {
            data.clone()
        };
        let mut stage = AtomicOutput::create(&source.metadata().map_err(io)?, output)?;
        stage.file.write_all(&encoded).map_err(io)?;

        let check = BlendFile::from_bytes(if compressed {
            zstd::stream::decode_all(encoded.as_slice()).map_err(io)?
        } else {
            encoded.clone()
        })?;
        if check.read_decompressed_bytes()? != data {
            return Err(Error::new("decoded_proof", "emitted bytes changed"));
        }
        let census = super::checked_pointer_census(&check)?;
        if census.require_closed().is_err() {
            return Err(Error::new(
                "pointer_proof",
                format!(
                    "output unresolved/ambiguous: {:?}",
                    census
                        .sites
                        .iter()
                        .filter(|s| !matches!(s.state, "null" | "resolved"))
                        .map(|s| (s.block, &s.path, s.address))
                        .collect::<Vec<_>>()
                ),
            ));
        }
        stage.publish(Durability::None)?;
        Ok(
            serde_json::json!({"output":output,"output_written":true,"layout_verified":true,"container_verified":true,"structural_pointer_verified":true,"native_verified":false,"converted":false,"compression":if compressed{"zstd-reencoded"}else{"raw"},"decoded_bytes":data.len(),"encoded_bytes":encoded.len(),"proof":self.verify()?,"operations":self.operations}),
        )
    }
}

#[cfg(test)]
mod storage_vectors {
    use super::*;

    fn fixture() -> BlendFile {
        BlendFile::from_bytes(
            include_bytes!("../../tests/fixtures/file-ops/synthetic-control.blend").to_vec(),
        )
        .unwrap()
    }
    fn scene(g: &mut Graph<'_>) -> usize {
        let owner = g.new_typed("Scene", None).unwrap();
        g.records[owner].code = *b"SC\0\0";
        g.cstring(owner, &["id", "name"], "SCStorage").unwrap();
        owner
    }

    #[test]
    fn raw_rgba_preview_extent_is_preserved_not_traversed_as_pointers() {
        let f = fixture();
        let mut source = Graph::new(&f).unwrap();
        let owner = scene(&mut source);
        let preview = source.new_typed("PreviewImage", Some(owner)).unwrap();
        source.ptr(owner, &["preview"], Some(preview)).unwrap();
        source
            .set_bytes(
                preview,
                &["w"],
                &[2i32.to_le_bytes(), 0i32.to_le_bytes()].concat(),
            )
            .unwrap();
        source
            .set_bytes(
                preview,
                &["h"],
                &[3i32.to_le_bytes(), 0i32.to_le_bytes()].concat(),
            )
            .unwrap();
        // Pointer-aligned pixel bytes deliberately resemble relocated addresses.
        let pixels = [0xfeedbeefdeadbeefu64.to_le_bytes(); 3].concat();
        let raw = source
            .allocate(*b"DATA", 0, 1, pixels.clone(), Some(owner))
            .unwrap();
        source
            .set_bytes(
                preview,
                &["rect"],
                &[
                    source.records[raw].address.to_le_bytes(),
                    0u64.to_le_bytes(),
                ]
                .concat(),
            )
            .unwrap();
        let saved = BlendFile::from_bytes(source.decoded().unwrap()).unwrap();
        let root = saved
            .blocks()
            .iter()
            .position(|b| b.code == *b"SC\0\0")
            .unwrap();
        let mut copy = Graph::new(&saved).unwrap();
        let map = copy.import(&saved, &[root], None, false).unwrap();
        let preview = copy.target(map[&root], &["preview"]).unwrap().unwrap();
        let address = number(copy.bytes(preview, &["rect"]).unwrap(), 0, 8, true).unwrap();
        let raw = copy
            .records
            .iter()
            .position(|r| r.address == address)
            .unwrap();
        assert_eq!(copy.records[raw].data, pixels);
        assert!(copy.records[raw].pointers.is_empty());
        copy.verify().unwrap();
        copy.records[raw].data.push(0);
        assert_eq!(copy.verify().unwrap_err().kind, "pointer_extent");
    }

    #[test]
    fn imported_runtime_reset_does_not_mutate_untouched_original_bytes() {
        let base = fixture();
        let socket = base
            .blocks()
            .iter()
            .enumerate()
            .find(|(i, b)| b.sdna_index != 0 && base.dna_type_id(*i).unwrap() == b"bNodeSocket")
            .unwrap()
            .0;
        let field = base
            .locate_field(
                socket,
                0,
                &[crate::value::PathStep::Field(b"runtime".to_vec())],
            )
            .unwrap();
        let mut bytes = base.read_decompressed_bytes().unwrap();
        let offset = base.blocks()[socket].data_offset + field.offset;
        let sentinel = 0xabcdef0123456789u64;
        bytes[offset..offset + 8].copy_from_slice(&sentinel.to_le_bytes());
        let original = bytes.clone();
        let saved = BlendFile::from_bytes(bytes).unwrap();
        let mut copy = Graph::new(&saved).unwrap();
        let owner = scene(&mut copy);
        let map = copy.import(&saved, &[socket], Some(owner), true).unwrap();
        assert_eq!(copy.value(map[&socket], &["runtime"]).unwrap(), 0);
        assert_eq!(saved.read_decompressed_bytes().unwrap(), original);
        // A sparse original-record copy is a separate preservation contract.
        let untouched = copy
            .allocate(
                saved.blocks()[socket].code,
                saved.blocks()[socket].sdna_index,
                saved.blocks()[socket].count,
                saved.payload(socket).unwrap().into_owned(),
                Some(owner),
            )
            .unwrap();
        assert_eq!(copy.value(untouched, &["runtime"]).unwrap(), sentinel);
    }

    #[test]
    fn legacy_editor_runtime_profiles_do_not_import_preference_handles() {
        let saved = BlendFile::from_bytes(
            include_bytes!("../../tests/fixtures/file-ops/native-4.5.8/empty-native-seed.blend")
                .to_vec(),
        )
        .unwrap();
        let original = saved.read_decompressed_bytes().unwrap();
        let wm = saved
            .blocks()
            .iter()
            .enumerate()
            .find(|(i, _)| saved.dna_type_id(*i).ok() == Some(b"wmWindowManager".as_slice()))
            .unwrap()
            .0;
        for p in pointer_layout(&saved, wm)
            .unwrap()
            .unwrap()
            .iter()
            .filter(|p| {
                p.path.starts_with("[0].keyconfigs.")
                    || p.path.starts_with("[0].timers.")
                    || matches!(
                        p.field_name.as_slice(),
                        b"defaultconf" | b"addonconf" | b"userconf" | b"message_bus"
                    )
            })
        {
            assert!(discarded(&saved, wm, p).unwrap(), "{}", p.path);
        }
        let editing = saved
            .blocks()
            .iter()
            .enumerate()
            .find(|(i, _)| saved.dna_type_id(*i).ok() == Some(b"Editing".as_slice()))
            .unwrap()
            .0;
        let mut copy = Graph::new(&saved).unwrap();
        let owner = scene(&mut copy);
        let map = copy.import(&saved, &[editing], Some(owner), true).unwrap();
        assert_eq!(copy.value(map[&editing], &["seqbasep"]).unwrap(), 0);
        assert_eq!(
            copy.value(map[&editing], &["displayed_channels"]).unwrap(),
            0
        );
        copy.ptr(owner, &["ed"], Some(map[&editing])).unwrap();
        copy.verify().unwrap();
        assert_eq!(saved.read_decompressed_bytes().unwrap(), original);
    }

    #[test]
    fn viewlayer_stats_version_405_reset_only_on_import() {
        let native = BlendFile::from_bytes(
            include_bytes!("../../tests/fixtures/file-ops/native-4.5.8/crypto-scene.blend")
                .to_vec(),
        )
        .unwrap();
        let mut input = Graph::new(&native).unwrap();
        let owner = scene(&mut input);
        let layer = input.new_typed("ViewLayer", Some(owner)).unwrap();
        input.cstring(layer, &["name"], "IndependentLayer").unwrap();
        input.set(layer, &["stats"], u64::MAX).unwrap();
        let saved = BlendFile::from_bytes(input.serialized_snapshot().unwrap()).unwrap();
        let original = saved.read_decompressed_bytes().unwrap();
        let source = saved
            .blocks()
            .iter()
            .enumerate()
            .find(|(i, _)| {
                saved.dna_type_id(*i).ok() == Some(b"ViewLayer".as_slice())
                    && text(&saved, *i, &["name"]).ok().as_deref() == Some("IndependentLayer")
            })
            .unwrap()
            .0;
        assert_eq!(uint(&saved, source, &["stats"]).unwrap(), u64::MAX);
        let layout = pointer_layout(&saved, source).unwrap().unwrap();
        let stats = layout.iter().find(|p| p.field_name == b"stats").unwrap();
        assert!(discarded(&saved, source, stats).unwrap());
        let mut wrong_type = stats.clone();
        wrong_type.declared_type = b"Object".to_vec();
        assert!(!discarded(&saved, source, &wrong_type).unwrap());
        let mut copy = Graph::new(&saved).unwrap();
        let destination = scene(&mut copy);
        let map = copy
            .import(&saved, &[source], Some(destination), true)
            .unwrap();
        assert_eq!(copy.value(map[&source], &["stats"]).unwrap(), 0);
        copy.verify().unwrap();
        assert_eq!(uint(&saved, source, &["stats"]).unwrap(), u64::MAX);
        assert_eq!(saved.read_decompressed_bytes().unwrap(), original);
    }
    #[test]
    fn cryptomatte_runtime_layer_handles_reset_only_on_import() {
        let native = BlendFile::from_bytes(
            include_bytes!("../../tests/fixtures/file-ops/native-4.5.8/crypto-scene.blend")
                .to_vec(),
        )
        .unwrap();
        let native_bytes = native.read_decompressed_bytes().unwrap();
        let native_storage = native
            .blocks()
            .iter()
            .enumerate()
            .find(|(i, _)| native.dna_type_id(*i).ok() == Some(b"NodeCryptomatte".as_slice()))
            .unwrap()
            .0;
        // The retained native fixture has empty persistent entries. Add one
        // structural entry so this test would detect clearing the wrong list.
        let mut input = Graph::new(&native).unwrap();
        let owner = scene(&mut input);
        let storage = input
            .allocate(
                native.blocks()[native_storage].code,
                native.blocks()[native_storage].sdna_index,
                native.blocks()[native_storage].count,
                native.payload(native_storage).unwrap().into_owned(),
                Some(owner),
            )
            .unwrap();
        let entry = input.new_typed("CryptomatteEntry", Some(owner)).unwrap();
        input.cstring(entry, &["name"], "IndependentEntry").unwrap();
        input
            .set(entry, &["encoded_hash"], u64::from(1.25f32.to_bits()))
            .unwrap();
        input.set_list(storage, &["entries"], &[entry]).unwrap();
        let saved = BlendFile::from_bytes(input.serialized_snapshot().unwrap()).unwrap();
        assert_eq!(native.read_decompressed_bytes().unwrap(), native_bytes);
        let original = saved.read_decompressed_bytes().unwrap();
        let storage = saved
            .blocks()
            .iter()
            .enumerate()
            .find(|(i, _)| saved.dna_type_id(*i).ok() == Some(b"NodeCryptomatte".as_slice()))
            .unwrap()
            .0;
        let first = uint(&saved, storage, &["runtime", "layers", "first"]).unwrap();
        let last = uint(&saved, storage, &["runtime", "layers", "last"]).unwrap();
        assert_ne!(first, 0);
        assert_ne!(last, 0);
        let mut copy = Graph::new(&saved).unwrap();
        let owner = scene(&mut copy);
        let map = copy.import(&saved, &[storage], Some(owner), false).unwrap();
        assert_eq!(
            copy.value(map[&storage], &["runtime", "layers", "first"])
                .unwrap(),
            0
        );
        assert_eq!(
            copy.value(map[&storage], &["runtime", "layers", "last"])
                .unwrap(),
            0
        );
        let old_entries = super::super::source_list(&saved, storage, &["entries"]).unwrap();
        let new_entries = copy.list(map[&storage], &["entries"]).unwrap();
        assert_eq!(old_entries.len(), 1);
        assert_eq!(new_entries.len(), old_entries.len());
        for (old, new) in old_entries.into_iter().zip(new_entries) {
            for field in ["encoded_hash", "name"] {
                let loc = saved.locate_field(old, 0, &steps(&[field])).unwrap();
                let payload = saved.payload(old).unwrap();
                assert_eq!(
                    copy.bytes(new, &[field]).unwrap(),
                    &payload[loc.offset..loc.offset + loc.field.size]
                );
            }
        }
        copy.verify().unwrap();
        let untouched = copy
            .allocate(
                saved.blocks()[storage].code,
                saved.blocks()[storage].sdna_index,
                saved.blocks()[storage].count,
                saved.payload(storage).unwrap().into_owned(),
                Some(owner),
            )
            .unwrap();
        assert_eq!(
            copy.value(untouched, &["runtime", "layers", "first"])
                .unwrap(),
            first
        );
        assert_eq!(
            copy.value(untouched, &["runtime", "layers", "last"])
                .unwrap(),
            last
        );
        assert_eq!(saved.read_decompressed_bytes().unwrap(), original);
    }

    #[test]
    fn empty_bake_handles_reset_but_nonempty_arrays_stay_persistent() {
        // Actual Blender 5.2 schema, not a relabelled legacy fixture. The root
        // owns retaining portable native fixtures; no private path is baked in.
        let Some(path) = std::env::var_os("OXBLEND_NATIVE_STORAGE_KIT") else {
            eprintln!(
                "bake storage receiving profile requires independently native-saved OXBLEND_NATIVE_STORAGE_KIT"
            );
            return;
        };
        let base = BlendFile::open(path).unwrap();
        let mut source = Graph::new(&base).unwrap();
        let source_owner = scene(&mut source);
        let modifier = source
            .new_typed("NodesModifierData", Some(source_owner))
            .unwrap();
        let sentinel = 0xabcdef0123456789u64;
        source.set(modifier, &["bakes"], sentinel).unwrap();
        let saved = BlendFile::from_bytes(source.serialized_snapshot().unwrap()).unwrap();
        let modifier_block = saved
            .blocks()
            .iter()
            .enumerate()
            .find(|(i, _)| saved.dna_type_id(*i).ok() == Some(b"NodesModifierData".as_slice()))
            .unwrap()
            .0;
        let original = saved.read_decompressed_bytes().unwrap();
        let mut copy = Graph::new(&saved).unwrap();
        let destination = scene(&mut copy);
        let map = copy
            .import(&saved, &[modifier_block], Some(destination), false)
            .unwrap();
        assert_eq!(copy.value(map[&modifier_block], &["bakes"]).unwrap(), 0);
        copy.verify().unwrap();
        let untouched = copy
            .allocate(
                saved.blocks()[modifier_block].code,
                saved.blocks()[modifier_block].sdna_index,
                saved.blocks()[modifier_block].count,
                saved.payload(modifier_block).unwrap().into_owned(),
                Some(destination),
            )
            .unwrap();
        assert_eq!(copy.value(untouched, &["bakes"]).unwrap(), sentinel);
        assert_eq!(saved.read_decompressed_bytes().unwrap(), original);
        // A positive count must resolve and relocate a concrete bake, not clear.
        let bake = source
            .new_typed("NodesModifierBake", Some(source_owner))
            .unwrap();
        source.set(modifier, &["bakes_num"], 1).unwrap();
        source.ptr(modifier, &["bakes"], Some(bake)).unwrap();
        let saved = BlendFile::from_bytes(source.decoded().unwrap()).unwrap();
        let modifier_block = saved
            .blocks()
            .iter()
            .enumerate()
            .find(|(i, _)| saved.dna_type_id(*i).ok() == Some(b"NodesModifierData".as_slice()))
            .unwrap()
            .0;
        let mut copy = Graph::new(&saved).unwrap();
        let destination = scene(&mut copy);
        let map = copy
            .import(&saved, &[modifier_block], Some(destination), false)
            .unwrap();
        let bake = copy
            .target(map[&modifier_block], &["bakes"])
            .unwrap()
            .unwrap();
        assert_eq!(copy.ty(bake), b"NodesModifierBake");
        assert_eq!(copy.value(map[&modifier_block], &["bakes_num"]).unwrap(), 1);
        copy.verify().unwrap();
        // Missing positive-count storage still refuses before allocation.
        source.set(modifier, &["bakes"], sentinel).unwrap();
        let saved = BlendFile::from_bytes(source.serialized_snapshot().unwrap()).unwrap();
        let mut copy = Graph::new(&saved).unwrap();
        let destination = scene(&mut copy);
        let before = copy.records.len();
        assert!(
            copy.import(&saved, &[modifier_block], Some(destination), false)
                .is_err()
        );
        assert_eq!(copy.records.len(), before);
    }

    #[test]
    fn unqualified_void_raw_storage_refuses_before_allocation() {
        let base = fixture();
        let mut source = Graph::new(&base).unwrap();
        let owner = scene(&mut source);
        let node = source.new_typed("bNode", Some(owner)).unwrap();
        source
            .cstring(node, &["idname"], "NotAnObservedStorageProfile")
            .unwrap();
        let raw = source
            .allocate(*b"DATA", 0, 1, vec![0; 8], Some(owner))
            .unwrap();
        source.ptr(node, &["storage"], Some(raw)).unwrap();
        let saved = BlendFile::from_bytes(source.serialized_snapshot().unwrap()).unwrap();
        let node = saved
            .blocks()
            .iter()
            .enumerate()
            .find(|(i, _)| saved.dna_type_id(*i).ok() == Some(b"bNode".as_slice()))
            .unwrap()
            .0;
        let mut copy = Graph::new(&saved).unwrap();
        let destination = scene(&mut copy);
        let before = copy.records.len();
        assert!(
            copy.import(&saved, &[node], Some(destination), true)
                .is_err()
        );
        assert_eq!(copy.records.len(), before);
    }
}
