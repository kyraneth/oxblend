use super::{Graph, text};
use crate::{BlendFile, Error, Result};
use std::collections::{BTreeSet, VecDeque};
fn link_flags() -> (u64, u64) {
    // Independently saved Blender 5.2.1 ordinary links have flag 2. The
    // Join Geometry multi-input differs from a normal input by bit 2048.
    (2048, 2)
}

fn image_pass_index(saved_version: u32, index: usize) -> Result<u64> {
    match saved_version {
        405 => i32::try_from(index)
            .map(|v| v as u64)
            .map_err(|_| Error::new("typed_overflow", "NodeImageLayer.pass_index signed32")),
        502 => Ok(0),
        _ => Err(Error::new(
            "socket_constructor",
            "unqualified saved pass metadata version",
        )),
    }
}

impl Graph<'_> {
    pub fn node_new(
        &mut self,
        tree: usize,
        kit: &BlendFile,
        kind: &str,
        name: &str,
    ) -> Result<usize> {
        let prototype = kit
            .blocks()
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                b.code == *b"DATA"
                    && kit.schema().types[kit.schema().structs[b.sdna_index].type_index] == b"bNode"
            })
            .find_map(|(i, _)| {
                (text(kit, i, &["idname"]).ok().as_deref() == Some(kind)).then_some(i)
            })
            .ok_or_else(|| Error::new("node_constructor", kind))?;
        let mut nodes = self.list(tree, &["nodes"])?;
        if nodes
            .iter()
            .any(|n| self.string(*n, &["name"]).ok().as_deref() == Some(name))
        {
            return Err(Error::new("name_collision", name));
        }
        let identifier = nodes
            .iter()
            .map(|n| self.value(*n, &["identifier"]))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| Error::new("identifier_overflow", "node identifier"))?;
        if identifier > i32::MAX as u64 {
            return Err(Error::new(
                "identifier_overflow",
                "positive signed node identifier",
            ));
        }
        let loc = kit.locate_field(
            prototype,
            0,
            &[crate::value::PathStep::Field(b"name".to_vec())],
        )?;
        if name.contains('\0') || name.len() >= loc.field.size {
            return Err(Error::new(
                "cstring_width",
                "node name must fit including terminator",
            ));
        }
        let owner = self.records[tree].owner;
        let map = self.import(kit, &[prototype], Some(owner), true)?;
        let node = map[&prototype];
        self.set(node, &["identifier"], identifier)?;
        self.cstring(node, &["name"], name)?;
        self.ptr(node, &["id"], None)?;
        self.ptr(node, &["parent"], None)?;
        for direction in ["inputs", "outputs"] {
            for socket in self.list(node, &[direction])? {
                self.ptr(socket, &["link"], None)?;
                self.set(socket, &["flag"], self.value(socket, &["flag"])? & !4)?;
            }
        }
        nodes.push(node);
        self.set_list(tree, &["nodes"], &nodes)?;
        self.operations.push(serde_json::json!({"operation":"node_new","node":name,"kind":kind,"identifier":identifier,"list_verified":true}));
        Ok(node)
    }
    pub fn link_remove(&mut self, tree: usize, link: usize) -> Result<()> {
        let links = self.list(tree, &["links"])?;
        if !links.contains(&link) {
            return Err(Error::new("link_owner", "link not in tree"));
        }
        let to = self.target(link, &["tosock"])?;
        let from = self.target(link, &["fromsock"])?;
        let keep = links.into_iter().filter(|n| *n != link).collect::<Vec<_>>();
        if let Some(to) = to {
            let mut incoming = None;
            for &other in &keep {
                if self.target(other, &["tosock"])? == Some(to) {
                    incoming = Some(other);
                }
            }
            if self.target(to, &["link"])? == Some(link) {
                self.ptr(to, &["link"], incoming)?;
            }
            if incoming.is_none() {
                self.set(to, &["flag"], self.value(to, &["flag"])? & !4)?;
            }
        }
        if let Some(from) = from {
            let mut outgoing = false;
            for &other in &keep {
                outgoing |= self.target(other, &["fromsock"])? == Some(from);
            }
            if !outgoing {
                self.set(from, &["flag"], self.value(from, &["flag"])? & !4)?;
            }
        }
        self.set_list(tree, &["links"], &keep)?;
        self.records[link].alive = false;
        Ok(())
    }
    pub fn link_new(
        &mut self,
        tree: usize,
        fromnode: usize,
        from: usize,
        tonode: usize,
        to: usize,
    ) -> Result<usize> {
        let nodes = self.list(tree, &["nodes"])?;
        if !nodes.contains(&fromnode)
            || !nodes.contains(&tonode)
            || !self.list(fromnode, &["outputs"])?.contains(&from)
            || !self.list(tonode, &["inputs"])?.contains(&to)
        {
            return Err(Error::new(
                "link_owner",
                "socket directions or tree ownership",
            ));
        }

        let (multi_input_flag, valid_link_flag) = link_flags();
        if self.value(to, &["flag"])? & multi_input_flag != 0 {
            return Err(Error::new("link_profile", "multi-input not qualified"));
        }
        for link in self.list(tree, &["links"])? {
            if self.target(link, &["tosock"])? == Some(to) {
                self.link_remove(tree, link)?;
            }
        }
        let link = self.new_typed("bNodeLink", Some(self.records[tree].owner))?;
        self.ptr(link, &["fromnode"], Some(fromnode))?;
        self.ptr(link, &["fromsock"], Some(from))?;
        self.ptr(link, &["tonode"], Some(tonode))?;
        self.ptr(link, &["tosock"], Some(to))?;
        self.set(link, &["flag"], valid_link_flag)?;
        let mut links = self.list(tree, &["links"])?;
        links.push(link);
        self.set_list(tree, &["links"], &links)?;
        self.ptr(to, &["link"], Some(link))?;
        self.set(to, &["flag"], self.value(to, &["flag"])? | 4)?;
        // Blender 4.5 Image/RLayers pass outputs keep their observed zero flag,
        // including linked outputs;502 keeps native connection-state flags.
        if !(self.schema.header().version == 405
            && matches!(
                self.string(fromnode, &["idname"])?.as_str(),
                "CompositorNodeImage" | "CompositorNodeRLayers"
            ))
        {
            self.set(from, &["flag"], self.value(from, &["flag"])? | 4)?;
        }
        self.operations.push(serde_json::json!({"operation":"link_new","from_node":self.string(fromnode,&["name"])? ,"from_socket":self.string(from,&["identifier"])? ,"to_node":self.string(tonode,&["name"])? ,"to_socket":self.string(to,&["identifier"])? ,"list_verified":true}));
        Ok(link)
    }
    pub fn socket_remove(
        &mut self,
        tree: usize,
        node: usize,
        socket: usize,
        direction: &str,
    ) -> Result<()> {
        let sockets = self.list(node, &[direction])?;
        if !sockets.contains(&socket) {
            return Err(Error::new("socket_owner", "socket not in node/direction"));
        }
        for link in self.list(tree, &["links"])? {
            if self.target(link, &["fromsock"])? == Some(socket)
                || self.target(link, &["tosock"])? == Some(socket)
            {
                self.link_remove(tree, link)?;
            }
        }
        self.set_list(
            node,
            &[direction],
            &sockets
                .into_iter()
                .filter(|n| *n != socket)
                .collect::<Vec<_>>(),
        )?;
        self.records[socket].alive = false;
        Ok(())
    }
    pub fn node_remove(&mut self, tree: usize, node: usize) -> Result<()> {
        let name = self.string(node, &["name"])?;
        for link in self.list(tree, &["links"])? {
            if self.target(link, &["fromnode"])? == Some(node)
                || self.target(link, &["tonode"])? == Some(node)
            {
                self.link_remove(tree, link)?;
            }
        }
        let nodes = self.list(tree, &["nodes"])?;
        if !nodes.contains(&node) {
            return Err(Error::new("node_owner", "not in tree"));
        }
        self.set_list(
            tree,
            &["nodes"],
            &nodes.into_iter().filter(|n| *n != node).collect::<Vec<_>>(),
        )?;
        self.records[node].alive = false;

        self.operations.push(serde_json::json!({"operation":"node_remove","node":name,"animation_orphans_preserved":true,"incident_links_removed":true}));
        Ok(())
    }
    #[allow(dead_code)] // Retained constructor API for callers without explicit pass metadata.
    pub fn socket_new(
        &mut self,
        node: usize,
        kit: &BlendFile,
        kind: &str,
        name: &str,
        index: usize,
    ) -> Result<usize> {
        // Compatibility entrypoint: an Alpha immediately following Combined is
        // the synthetic output. Callers with explicit EXR identities use the
        // non-ambiguous socket_new_pass entrypoint instead.
        let outputs = self.list(node, &["outputs"])?;
        let source_pass = if name == "Alpha"
            && outputs
                .last()
                .is_some_and(|s| self.string(*s, &["name"]).ok().as_deref() == Some("Combined"))
        {
            "Combined"
        } else {
            name
        };
        self.socket_new_pass(node, kit, kind, name, index, source_pass)
    }

    pub fn socket_new_pass(
        &mut self,
        node: usize,
        kit: &BlendFile,
        kind: &str,
        name: &str,
        index: usize,
        source_pass: &str,
    ) -> Result<usize> {
        self.compatible(kit)?;
        if self.ty(node) != b"bNode" {
            return Err(Error::new("socket_owner", "owner must be a node"));
        }
        if !matches!(
            kind,
            "NodeSocketFloat" | "NodeSocketColor" | "NodeSocketVector" | "NodeSocketVector4D"
        ) {
            return Err(Error::new("socket_constructor", "unqualified output type"));
        }
        let mut outputs = self.list(node, &["outputs"])?;
        for &socket in &outputs {
            if self.string(socket, &["name"])? == name
                || self.string(socket, &["identifier"])? == name
            {
                return Err(Error::new("name_collision", name));
            }
        }
        let prototype = kit
            .blocks()
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                if b.sdna_index == 0
                    || kit.schema().types[kit.schema().structs[b.sdna_index].type_index]
                        != b"bNodeSocket"
                {
                    return None;
                }
                let actual = text(kit, i, &["idname"]).ok()?;
                let direction = super::uint(kit, i, &["in_out"]).ok()?;
                // Prefer exact output, then exact input. Direction is an
                // independently observed three-dimensional subtype, not a
                // substitute for a four-dimensional vector.
                let family = if actual == kind {
                    0
                } else if kind == "NodeSocketVector"
                    && actual == "NodeSocketVectorDirection"
                    && matches!(kit.header().version, 405 | 502)
                {
                    2
                } else {
                    return None;
                };
                match direction {
                    2 => Some((family, i)),
                    1 => Some((family + 1, i)),
                    _ => None,
                }
            })
            .min_by_key(|&(priority, _)| priority)
            .map(|(_, i)| i)
            .ok_or_else(|| {
                Error::new(
                    "socket_constructor",
                    format!("qualified native prototype missing: {kind}"),
                )
            })?;
        let prototype_kind = text(kit, prototype, &["idname"])?;
        let prototype_direction = super::uint(kit, prototype, &["in_out"])?;
        let layout = super::pointer_layout(kit, prototype)?
            .ok_or_else(|| Error::new("socket_constructor", "socket pointer layout absent"))?;
        let value_slot = layout
            .iter()
            .find(|p| p.path == "[0].default_value")
            .ok_or_else(|| Error::new("socket_constructor", "socket default slot absent"))?;
        let default = crate::file_ops::validate::site(
            kit,
            prototype,
            value_slot,
            super::uint(kit, prototype, &["default_value"])?,
        )?
        .ok_or_else(|| Error::new("socket_constructor", "native socket default missing"))?;
        if matches!(kind, "NodeSocketVector" | "NodeSocketVector4D") {
            let st = &kit.schema().structs[kit.block(default)?.sdna_index];
            let value = st
                .fields
                .iter()
                .find(|f| f.name == b"value")
                .ok_or_else(|| Error::new("socket_constructor", "vector value absent"))?;
            let dimensions = if st.fields.iter().any(|f| f.name == b"dimensions") {
                super::uint(kit, default, &["dimensions"])?
            } else if value.array_len == 3 {
                // The Blender 4.5 legacy schema stores float[3].
                3
            } else {
                return Err(Error::new(
                    "socket_constructor",
                    "vector dimensions unqualified",
                ));
            };
            let wanted = if kind == "NodeSocketVector" { 3 } else { 4 };
            if kit.schema().types[value.type_index] != b"float"
                || value.element_size != 4
                || !matches!(value.array_len, 3 | 4)
                || value.array_len < wanted
                || dimensions != wanted as u64
            {
                return Err(Error::new(
                    "socket_constructor",
                    format!("{prototype_kind} default dimensions {dimensions}, requested {wanted}"),
                ));
            }
        }
        // Check strings before importing any allocation. Names must fit both
        // display and persistent identifier fields, including their terminator.
        for field in ["name", "identifier"] {
            let loc = kit.locate_field(
                prototype,
                0,
                &[crate::value::PathStep::Field(field.as_bytes().to_vec())],
            )?;
            if name.contains('\0') || name.len() >= loc.field.size {
                return Err(Error::new(
                    "cstring_width",
                    "socket name must fit including terminator",
                ));
            }
        }
        let image_pass = matches!(
            self.string(node, &["idname"])?.as_str(),
            "CompositorNodeImage" | "CompositorNodeRLayers"
        );
        let legacy_image_pass = image_pass && self.schema.header().version == 405;
        let pass_index = if image_pass {
            image_pass_index(self.schema.header().version, index)?
        } else {
            0
        };
        if image_pass {
            let si = kit
                .schema()
                .structs
                .iter()
                .position(|s| kit.schema().types[s.type_index] == b"NodeImageLayer")
                .ok_or_else(|| Error::new("socket_constructor", "pass storage schema absent"))?;
            let index_field = kit.schema().structs[si]
                .fields
                .iter()
                .find(|f| f.name == b"pass_index")
                .ok_or_else(|| Error::new("socket_constructor", "pass_index absent"))?;
            if kit.schema().types[index_field.type_index] != b"int"
                || index_field.size != 4
                || index_field.array_len != 1
                || index_field.pointer
            {
                return Err(Error::new(
                    "socket_constructor",
                    "pass_index is not observed signed32 storage",
                ));
            }
            let width = kit.schema().structs[si]
                .fields
                .iter()
                .find(|f| f.name == b"pass_name")
                .ok_or_else(|| Error::new("socket_constructor", "pass_name absent"))?
                .size;
            if source_pass.contains('\0') || source_pass.len() >= width {
                return Err(Error::new(
                    "cstring_width",
                    "pass identity must fit including terminator",
                ));
            }
        }
        let owner = self.records[node].owner;
        let map = self.import(kit, &[prototype], Some(owner), true)?;
        let socket = map[&prototype];
        self.cstring(socket, &["name"], name)?;
        self.cstring(socket, &["identifier"], name)?;
        self.cstring(socket, &["idname"], kind)?;
        self.set(socket, &["in_out"], 2)?;
        // Independently saved Blender 4.5/5.2 outputs have link capacity4095;
        // inputs have capacity1. Promotion must not inherit input semantics.
        self.set(socket, &["limit"], 4095)?;
        if prototype_direction == 1 {
            // Native unlinked outputs have flag64, not input-only UI flags.
            self.set(socket, &["flag"], 64)?;
        }
        if !legacy_image_pass
            && kind == "NodeSocketVector"
            && (prototype_kind != kind || prototype_direction == 1)
        {
            let defaults = self.target(socket, &["default_value"])?.ok_or_else(|| {
                Error::new("socket_constructor", "imported vector default absent")
            })?;
            // Observed generic Vector outputs (including native Image.Normal)
            // use subtype0, zero value and +/-FLT_MAX, unlike Direction's
            // subtype22, unit-Z default and [-1,1] range. Keep dimensions3.
            self.set(defaults, &["subtype"], 0)?;
            let width = self.bytes(defaults, &["value"])?.len();
            self.set_bytes(defaults, &["value"], &vec![0; width])?;
            self.set(defaults, &["min"], u64::from((-f32::MAX).to_bits()))?;
            self.set(defaults, &["max"], u64::from(f32::MAX.to_bits()))?;
        }
        // A prototype's connection/visibility flags are not the new socket's
        // connection state. Retain its independently qualified type/defaults.
        self.set(
            socket,
            &["flag"],
            self.value(socket, &["flag"])? & !(4 | 8 | 2048),
        )?;
        if legacy_image_pass {
            self.set(socket, &["flag"], 0)?;
            self.set(socket, &["own_index"], pass_index)?;
        }
        self.ptr(socket, &["storage"], None)?;
        if image_pass {
            let storage = self.new_typed("NodeImageLayer", Some(owner))?;
            // The405 caller supplies its legacy ordered ordinal;502 uses
            // native5.2.1's all-zero index and explicit source-pass identity.
            self.set(storage, &["pass_index"], pass_index)?;
            self.cstring(storage, &["pass_name"], source_pass)?;
            self.ptr(socket, &["storage"], Some(storage))?;
        }
        outputs.push(socket);
        self.set_list(node, &["outputs"], &outputs)?;
        self.operations.push(serde_json::json!({"operation":"socket_new","node":self.string(node,&["name"])?,"identifier":name,"kind":kind,"list_verified":true}));
        Ok(socket)
    }

    fn property_check_name(&self, group: Option<usize>, name: &str) -> Result<()> {
        let si = self
            .schema
            .schema()
            .structs
            .iter()
            .position(|s| self.schema.schema().types[s.type_index] == b"IDProperty")
            .ok_or_else(|| Error::new("constructor_type", "IDProperty"))?;
        let width = self.schema.schema().structs[si]
            .fields
            .iter()
            .find(|f| f.name == b"name")
            .ok_or_else(|| Error::new("missing_field", "IDProperty.name"))?
            .size;
        if name.contains('\0') || name.len() >= width {
            return Err(Error::new(
                "cstring_width",
                "property name must fit including terminator",
            ));
        }
        if let Some(group) = group {
            if self.ty(group) != b"IDProperty" || self.value(group, &["type"])? != 6 {
                return Err(Error::new("property_type", "group required"));
            }
            for p in self.list(group, &["data", "group"])? {
                if self.string(p, &["name"])? == name {
                    return Err(Error::new(
                        "property_collision",
                        crate::value::hex(name.as_bytes()),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn property_group(&mut self, owner: usize, name: &str) -> Result<usize> {
        self.property_check_name(None, name)?;
        let p = self.new_typed("IDProperty", Some(self.records[owner].owner))?;
        self.set(p, &["type"], 6)?;
        self.cstring(p, &["name"], name)?;
        Ok(p)
    }
    pub fn property_int(&mut self, group: usize, name: &str, value: i32) -> Result<usize> {
        self.property_check_name(Some(group), name)?;
        let p = self.new_typed("IDProperty", Some(self.records[group].owner))?;
        self.set(p, &["type"], 1)?;
        self.cstring(p, &["name"], name)?;
        self.set_bytes(p, &["data", "val"], &value.to_le_bytes())?;
        self.property_insert(group, p)?;
        Ok(p)
    }

    #[allow(dead_code)] // Exact float32 constructor, independently exercised by native vectors.
    pub(crate) fn property_float(&mut self, group: usize, name: &str, bits: u32) -> Result<usize> {
        self.property_check_name(Some(group), name)?;
        let p = self.new_typed("IDProperty", Some(self.records[group].owner))?;
        self.set(p, &["type"], 2)?;
        self.cstring(p, &["name"], name)?;
        self.set_bytes(p, &["data", "val"], &bits.to_le_bytes())?;
        self.property_insert(group, p)?;
        Ok(p)
    }
    pub fn property_string(&mut self, group: usize, name: &str, value: &str) -> Result<usize> {
        self.property_check_name(Some(group), name)?;
        if value.contains('\0') {
            return Err(Error::new(
                "cstring_width",
                "property string contains an interior terminator",
            ));
        }
        let length = value
            .len()
            .checked_add(1)
            .ok_or_else(|| Error::new("overflow", "string terminator"))?;
        // Saved IDProperty.len is signed int. Do not truncate a larger backing
        // allocation or mistake byte-string storage for a terminated string.
        let length = i32::try_from(length)
            .map_err(|_| Error::new("typed_overflow", "IDProperty string length"))?;
        let owner = self.records[group].owner;
        let p = self.new_typed("IDProperty", Some(owner))?;
        self.cstring(p, &["name"], name)?;
        self.set(p, &["type"], 0)?;
        self.set(p, &["subtype"], 0)?;
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        let raw = self.allocate(*b"DATA", 0, 1, bytes, Some(owner))?;
        self.ptr(p, &["data", "pointer"], Some(raw))?;
        self.set(p, &["len"], length as u64)?;
        self.set(p, &["totallen"], length as u64)?;
        self.property_insert(group, p)?;
        Ok(p)
    }
    pub fn property_insert(&mut self, group: usize, p: usize) -> Result<()> {
        if self.ty(group) != b"IDProperty"
            || self.value(group, &["type"])? != 6
            || self.ty(p) != b"IDProperty"
            || self.records[group].owner != self.records[p].owner
            || self.target(p, &["next"])?.is_some()
            || self.target(p, &["prev"])?.is_some()
        {
            return Err(Error::new(
                "property_owner",
                "detached owner-local property and group required",
            ));
        }
        let name = self
            .bytes(p, &["name"])?
            .split(|&b| b == 0)
            .next()
            .unwrap_or_default()
            .to_vec();
        let mut properties = self.list(group, &["data", "group"])?;
        for &id in &properties {
            if self
                .bytes(id, &["name"])?
                .split(|&b| b == 0)
                .next()
                .unwrap_or_default()
                == name
            {
                return Err(Error::new("property_collision", crate::value::hex(&name)));
            }
        }
        properties.push(p);
        self.set_list(group, &["data", "group"], &properties)?;
        self.set(group, &["len"], properties.len() as u64)?;
        Ok(())
    }
    pub fn property_remove(&mut self, group: usize, name: &str) -> Result<()> {
        let properties = self.list(group, &["data", "group"])?;
        let mut selected = Vec::new();
        for &p in &properties {
            if self
                .bytes(p, &["name"])?
                .split(|&b| b == 0)
                .next()
                .unwrap_or_default()
                == name.as_bytes()
            {
                selected.push(p);
            }
        }
        if selected.len() != 1 {
            return Err(Error::new("property_selection", "requires one property"));
        }
        let keep = properties
            .into_iter()
            .filter(|p| *p != selected[0])
            .collect::<Vec<_>>();
        self.set_list(group, &["data", "group"], &keep)?;
        self.set(group, &["len"], keep.len() as u64)?;
        self.records[selected[0]].alive = false;
        Ok(())
    }
    pub fn text_new(&mut self, kit: &BlendFile, name: &str, content: &str) -> Result<usize> {
        let source = kit
            .blocks()
            .iter()
            .position(|b| b.code == *b"TX\0\0")
            .ok_or_else(|| Error::new("text_constructor", "Text prototype absent"))?;
        let map = self.import(kit, &[source], None, false)?;
        let text = map[&source];
        self.cstring(text, &["id", "name"], &format!("TX{name}"))?;
        let old = self.list(text, &["lines"])?;
        self.set_list(text, &["lines"], &[])?;
        self.ptr(text, &["curl"], None)?;
        self.ptr(text, &["sell"], None)?;
        for id in old {
            self.records[id].alive = false;
        }
        let mut lines = Vec::new();
        for line in content.split('\n') {
            let id = self.new_typed("TextLine", Some(text))?;
            let mut bytes = line.as_bytes().to_vec();
            bytes.push(0);
            let data = self.allocate(*b"DATA", 0, 1, bytes, Some(text))?;
            self.ptr(id, &["line"], Some(data))?;
            self.set(id, &["len"], line.len() as u64)?;
            lines.push(id);
        }
        self.set_list(text, &["lines"], &lines)?;
        self.ptr(text, &["curl"], lines.last().copied())?;
        self.ptr(text, &["sell"], lines.last().copied())?;
        let column = content.rsplit('\n').next().unwrap_or_default().len();
        self.set(text, &["curc"], column as u64)?;
        self.set(text, &["selc"], column as u64)?;
        let flags = self.value(text, &["flags"])?;
        self.set(text, &["flags"], flags | 16)?;
        self.operations.push(serde_json::json!({"operation":"text_new_write","text":name,"bytes":content.len(),"lines":lines.len(),"module":true,"list_verified":true}));
        Ok(text)
    }
    pub fn prune_owned(&mut self) -> Result<()> {
        let mut queue = self
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.alive && r.code != *b"DATA")
            .map(|(i, _)| i)
            .collect::<VecDeque<_>>();
        let mut keep = BTreeSet::new();
        let addresses = self
            .records
            .iter()
            .enumerate()
            .map(|(i, r)| (r.address, i))
            .collect::<std::collections::HashMap<_, _>>();
        while let Some(id) = queue.pop_front() {
            if !keep.insert(id) {
                continue;
            }
            let r = &self.records[id];
            if !r.alive {
                return Err(Error::new(
                    "retained_removed",
                    "kept pointer refers to removed record",
                ));
            }
            for p in &r.pointers {
                let a = crate::checked::number(
                    &r.data,
                    p.offset,
                    self.schema.header().pointer_size,
                    true,
                )?;
                if a != 0 {
                    queue.push_back(
                        *addresses
                            .get(&a)
                            .ok_or_else(|| Error::new("dangling_pointer", "prune"))?,
                    );
                }
            }
        }
        for (i, r) in self.records.iter_mut().enumerate() {
            if r.code == *b"DATA" && !keep.contains(&i) {
                r.alive = false;
            }
        }
        self.verify()?;
        Ok(())
    }
}

#[cfg(test)]
mod constructor_vectors {
    use super::*;

    fn schema() -> BlendFile {
        BlendFile::from_bytes(
            include_bytes!("../../tests/fixtures/file-ops/synthetic-control.blend").to_vec(),
        )
        .unwrap()
    }
    fn scene(g: &mut Graph<'_>, name: &str) -> usize {
        let id = g.new_typed("Scene", None).unwrap();
        g.records[id].code = *b"SC\0\0";
        g.cstring(id, &["id", "name"], &format!("SC{name}"))
            .unwrap();
        id
    }
    fn kit(schema: &BlendFile) -> BlendFile {
        let mut g = Graph::new(schema).unwrap();
        let owner = scene(&mut g, "Vectors");
        let tree = g.new_typed("bNodeTree", Some(owner)).unwrap();
        g.cstring(tree, &["id", "name"], "NTVectors").unwrap();
        g.ptr(owner, &["nodetree"], Some(tree)).unwrap();
        let node = g.new_typed("bNode", Some(owner)).unwrap();
        g.cstring(node, &["name"], "Prototype").unwrap();
        g.cstring(node, &["idname"], "NodeReroute").unwrap();
        g.set(node, &["identifier"], 1).unwrap();
        g.set_list(tree, &["nodes"], &[node]).unwrap();
        let mut sockets = Vec::new();
        // Independently observed native output representations, not retained
        // generation tables: float=0, vector=1, color=2; output direction=2.
        for (kind, value_type, numeric_type) in [
            ("NodeSocketFloat", "bNodeSocketValueFloat", 0),
            ("NodeSocketVector", "bNodeSocketValueVector", 1),
            ("NodeSocketColor", "bNodeSocketValueRGBA", 2),
        ] {
            let socket = g.new_typed("bNodeSocket", Some(owner)).unwrap();
            g.cstring(socket, &["name"], kind).unwrap();
            g.cstring(socket, &["identifier"], kind).unwrap();
            g.cstring(socket, &["idname"], kind).unwrap();
            g.set(socket, &["type"], numeric_type).unwrap();
            g.set(socket, &["in_out"], 2).unwrap();
            // Native short link capacity observed in saved outputs.
            g.set(socket, &["limit"], 4095).unwrap();
            let defaults = g.new_typed(value_type, Some(owner)).unwrap();
            if kind == "NodeSocketVector" && g.bytes(defaults, &["dimensions"]).is_ok() {
                g.set(defaults, &["dimensions"], 3).unwrap();
            }
            g.ptr(socket, &["default_value"], Some(defaults)).unwrap();
            sockets.push(socket);
        }
        g.set_list(node, &["outputs"], &sockets).unwrap();
        BlendFile::from_bytes(g.decoded().unwrap()).unwrap()
    }

    #[test]
    fn typed_properties_roundtrip_and_refuse_without_new_allocations() {
        let f = schema();
        let mut g = Graph::new(&f).unwrap();
        let owner = scene(&mut g, "Properties");
        let root = g.property_group(owner, "").unwrap();
        g.ptr(owner, &["id", "properties"], Some(root)).unwrap();
        let child = g.property_group(owner, "group").unwrap();
        g.property_insert(root, child).unwrap();
        let integer = g.property_int(child, "integer", i32::MIN).unwrap();
        let float = g
            .property_float(child, "float", (-1.25f32).to_bits())
            .unwrap();
        let string = g.property_string(child, "bytes", "héllo").unwrap();
        assert_eq!(
            g.value(integer, &["data", "val"]).unwrap(),
            i32::MIN as u32 as u64
        );
        assert_eq!(
            g.value(float, &["data", "val"]).unwrap(),
            (-1.25f32).to_bits() as u64
        );
        let raw = g.target(string, &["data", "pointer"]).unwrap().unwrap();
        assert_eq!(g.records[raw].owner, owner);
        assert_eq!(g.records[raw].data, b"h\xc3\xa9llo\0");
        assert_eq!(g.value(string, &["len"]).unwrap(), 7);
        assert_eq!(
            g.list(child, &["data", "group"]).unwrap(),
            [integer, float, string]
        );
        let before = g.records.len();
        assert_eq!(
            g.property_int(child, "integer", 4).unwrap_err().kind,
            "property_collision"
        );
        assert_eq!(
            g.property_string(child, "bad", "a\0b").unwrap_err().kind,
            "cstring_width"
        );
        assert_eq!(
            g.property_group(owner, &"n".repeat(64)).unwrap_err().kind,
            "cstring_width"
        );
        assert_eq!(
            g.set(integer, &["data", "val"], 1u64 << 32)
                .unwrap_err()
                .kind,
            "typed_overflow"
        );
        assert_eq!(g.records.len(), before);
        let receiving = BlendFile::from_bytes(g.decoded().unwrap()).unwrap();
        let mut copy = Graph::new(&receiving).unwrap();
        let source_scene = receiving
            .blocks()
            .iter()
            .position(|b| b.code == *b"SC\0\0")
            .unwrap();
        let map = copy
            .import(&receiving, &[source_scene], None, false)
            .unwrap();
        let root2 = copy
            .target(map[&source_scene], &["id", "properties"])
            .unwrap()
            .unwrap();
        let child2 = copy.list(root2, &["data", "group"]).unwrap()[0];
        let leaves = copy.list(child2, &["data", "group"]).unwrap();
        assert_eq!(copy.value(leaves[0], &["type"]).unwrap(), 1);
        assert_eq!(copy.value(leaves[1], &["type"]).unwrap(), 2);
        assert_eq!(
            copy.value(leaves[1], &["data", "val"]).unwrap(),
            (-1.25f32).to_bits() as u64
        );
        let raw2 = copy
            .target(leaves[2], &["data", "pointer"])
            .unwrap()
            .unwrap();
        assert_eq!(copy.records[raw2].data, b"h\xc3\xa9llo\0");
        copy.verify().unwrap();
    }

    #[test]
    fn saved_pass_index_version_split_and_signed_storage_bounds() {
        assert_eq!(image_pass_index(405, 0).unwrap(), 0);
        assert_eq!(
            image_pass_index(405, i32::MAX as usize).unwrap(),
            i32::MAX as u64
        );
        assert_eq!(
            image_pass_index(405, i32::MAX as usize + 1)
                .unwrap_err()
                .kind,
            "typed_overflow"
        );
        assert_eq!(image_pass_index(502, usize::MAX).unwrap(), 0);
        assert_eq!(
            image_pass_index(503, 0).unwrap_err().kind,
            "socket_constructor"
        );
        let f = schema();
        let k = kit(&f);
        let mut g = Graph::new(&f).unwrap();
        let owner = scene(&mut g, "PassBounds");
        let node = g.new_typed("bNode", Some(owner)).unwrap();
        g.cstring(node, &["idname"], "CompositorNodeImage").unwrap();
        let before = g.records.len();
        if f.header().version == 405 {
            assert_eq!(
                g.socket_new_pass(
                    node,
                    &k,
                    "NodeSocketFloat",
                    "Overflow",
                    i32::MAX as usize + 1,
                    "Depth"
                )
                .unwrap_err()
                .kind,
                "typed_overflow"
            );
            assert_eq!(g.records.len(), before);
            assert!(g.list(node, &["outputs"]).unwrap().is_empty());
        }
    }

    #[test]
    fn pass_outputs_keep_order_and_explicit_shared_identity() {
        let f = schema();
        let k = kit(&f);
        let mut g = Graph::new(&f).unwrap();
        let owner = scene(&mut g, "Passes");
        let node = g.new_typed("bNode", Some(owner)).unwrap();
        g.cstring(node, &["idname"], "CompositorNodeImage").unwrap();
        let mut expected = Vec::new();
        for (index, (kind, name, source)) in [
            ("NodeSocketFloat", "Depth", "Depth"),
            ("NodeSocketColor", "Combined", "Combined"),
            ("NodeSocketFloat", "Alpha", "Combined"),
            ("NodeSocketVector", "Normal", "Normal"),
            ("NodeSocketColor", "Custom", "Custom"),
        ]
        .into_iter()
        .enumerate()
        {
            let socket = g
                .socket_new_pass(node, &k, kind, name, index, source)
                .unwrap();
            let storage = g.target(socket, &["storage"]).unwrap().unwrap();
            assert_eq!(g.string(storage, &["pass_name"]).unwrap(), source);
            assert_eq!(
                g.value(storage, &["pass_index"]).unwrap(),
                image_pass_index(f.header().version, index).unwrap()
            );
            assert!(g.target(socket, &["default_value"]).unwrap().is_some());
            expected.push(socket);
        }
        assert_eq!(g.list(node, &["outputs"]).unwrap(), expected);
        let before = g.records.len();
        assert_eq!(
            g.socket_new_pass(node, &k, "NodeSocketFloat", "Alpha", 5, "Alpha")
                .unwrap_err()
                .kind,
            "name_collision"
        );
        assert_eq!(
            g.socket_new(node, &k, "NodeSocketInt", "Unsupported", 5)
                .unwrap_err()
                .kind,
            "socket_constructor"
        );
        assert_eq!(g.records.len(), before);
        let separate = g.new_typed("bNode", Some(owner)).unwrap();
        g.cstring(separate, &["idname"], "CompositorNodeImage")
            .unwrap();
        g.socket_new_pass(separate, &k, "NodeSocketColor", "Combined", 0, "Combined")
            .unwrap();
        let alpha = g
            .socket_new_pass(separate, &k, "NodeSocketFloat", "Alpha", 1, "Alpha")
            .unwrap();
        let storage = g.target(alpha, &["storage"]).unwrap().unwrap();
        assert_eq!(g.string(storage, &["pass_name"]).unwrap(), "Alpha");
    }

    #[test]
    fn direction_input_prototype_builds_generic_three_dimensional_output() {
        let f = schema();
        let k = kit(&f);
        let root = k
            .blocks()
            .iter()
            .position(|b| b.code == *b"SC\0\0")
            .unwrap();
        let mut input = Graph::new(&k).unwrap();
        let owner = input.import(&k, &[root], None, false).unwrap()[&root];
        let tree = input.target(owner, &["nodetree"]).unwrap().unwrap();
        let prototype_node = input.list(tree, &["nodes"]).unwrap()[0];
        let output_list = input.list(prototype_node, &["outputs"]).unwrap();
        let vector = *output_list
            .iter()
            .find(|s| input.string(**s, &["idname"]).unwrap() == "NodeSocketVector")
            .unwrap();
        input
            .set_list(
                prototype_node,
                &["outputs"],
                &output_list
                    .into_iter()
                    .filter(|s| *s != vector)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        input
            .set_list(prototype_node, &["inputs"], &[vector])
            .unwrap();
        input
            .cstring(vector, &["idname"], "NodeSocketVectorDirection")
            .unwrap();
        input.set(vector, &["in_out"], 1).unwrap();
        input.set(vector, &["limit"], 1).unwrap();
        input.set(vector, &["flag"], 192).unwrap();
        let defaults = input.target(vector, &["default_value"]).unwrap().unwrap();
        input.set(defaults, &["subtype"], 22).unwrap();
        let mut direction_value = vec![0; input.bytes(defaults, &["value"]).unwrap().len()];
        direction_value[8..12].copy_from_slice(&1f32.to_le_bytes());
        input
            .set_bytes(defaults, &["value"], &direction_value)
            .unwrap();
        input
            .set(defaults, &["min"], u64::from((-1f32).to_bits()))
            .unwrap();
        input
            .set(defaults, &["max"], u64::from(1f32.to_bits()))
            .unwrap();
        let prototype = BlendFile::from_bytes(input.decoded().unwrap()).unwrap();
        let untouched = prototype.read_decompressed_bytes().unwrap();
        let mut g = Graph::new(&prototype).unwrap();
        let owner = scene(&mut g, "VectorPromotion");
        let image = g.new_typed("bNode", Some(owner)).unwrap();
        g.cstring(image, &["idname"], "CompositorNodeImage")
            .unwrap();
        let socket = g
            .socket_new_pass(image, &prototype, "NodeSocketVector", "Normal", 0, "Normal")
            .unwrap();
        assert_eq!(g.string(socket, &["idname"]).unwrap(), "NodeSocketVector");
        assert_eq!(g.value(socket, &["type"]).unwrap(), 1);
        assert_eq!(g.value(socket, &["in_out"]).unwrap(), 2);
        assert_eq!(g.value(socket, &["limit"]).unwrap(), 4095);
        let legacy = g.schema.header().version == 405;
        assert_eq!(
            g.value(socket, &["flag"]).unwrap(),
            if legacy { 0 } else { 64 }
        );
        let defaults = g.target(socket, &["default_value"]).unwrap().unwrap();
        assert_eq!(
            g.value(defaults, &["subtype"]).unwrap(),
            if legacy { 22 } else { 0 }
        );
        let values = g.bytes(defaults, &["value"]).unwrap();
        if legacy {
            assert_eq!(values, direction_value);
        } else {
            assert!(values.iter().all(|b| *b == 0));
        }
        if g.bytes(defaults, &["dimensions"]).is_ok() {
            assert_eq!(g.value(defaults, &["dimensions"]).unwrap(), 3);
        }
        assert_eq!(
            g.value(defaults, &["min"]).unwrap(),
            u64::from((if legacy { -1f32 } else { -f32::MAX }).to_bits())
        );
        let storage = g.target(socket, &["storage"]).unwrap().unwrap();
        assert_eq!(g.string(storage, &["pass_name"]).unwrap(), "Normal");
        g.verify().unwrap();
        assert_eq!(prototype.read_decompressed_bytes().unwrap(), untouched);
        // A three-dimensional subtype cannot supply a requested four-vector.
        let before = g.records.len();
        assert_eq!(
            g.socket_new_pass(
                image,
                &prototype,
                "NodeSocketVector4D",
                "Vector",
                1,
                "Vector"
            )
            .unwrap_err()
            .kind,
            "socket_constructor"
        );
        assert_eq!(g.records.len(), before);
        let input_defaults = input.target(vector, &["default_value"]).unwrap().unwrap();
        if input.bytes(input_defaults, &["dimensions"]).is_ok() {
            input.set(input_defaults, &["dimensions"], 4).unwrap();
            let wrong_dimensions = BlendFile::from_bytes(input.decoded().unwrap()).unwrap();
            assert_eq!(
                g.socket_new_pass(
                    image,
                    &wrong_dimensions,
                    "NodeSocketVector",
                    "WrongDimensions",
                    1,
                    "Normal"
                )
                .unwrap_err()
                .kind,
                "socket_constructor"
            );
            assert_eq!(g.records.len(), before);
            input.set(input_defaults, &["dimensions"], 3).unwrap();
        }
        // Nor may a four-vector's identity be used as a three-vector fallback.
        input
            .cstring(vector, &["idname"], "NodeSocketVector4D")
            .unwrap();
        let only_four_identity = BlendFile::from_bytes(input.decoded().unwrap()).unwrap();
        assert_eq!(
            g.socket_new_pass(
                image,
                &only_four_identity,
                "NodeSocketVector",
                "AnotherNormal",
                1,
                "Normal"
            )
            .unwrap_err()
            .kind,
            "socket_constructor"
        );
        assert_eq!(g.records.len(), before);
    }

    #[test]
    fn links_replace_one_input_preserve_others_and_refuse_wrong_owner() {
        let f = schema();
        let mut g = Graph::new(&f).unwrap();
        let roots = f
            .blocks()
            .iter()
            .enumerate()
            .filter(|(_, b)| b.code != *b"DATA" && b.code != *b"DNA1" && b.code != *b"REND")
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        g.import(&f, &roots, None, false).unwrap();
        let tree = (0..g.records.len())
            .find(|i| g.ty(*i) == b"bNodeTree")
            .unwrap();
        let nodes = g.list(tree, &["nodes"]).unwrap();
        let from = g.list(nodes[0], &["outputs"]).unwrap()[0];
        let to = g.list(nodes[1], &["inputs"]).unwrap()[0];
        let third = g.node_new(tree, &f, "NodeReroute", "Third").unwrap();
        let prototype = g.records[to].clone();
        let other_input = g
            .allocate(
                prototype.code,
                prototype.sdna,
                prototype.count,
                prototype.data,
                Some(prototype.owner),
            )
            .unwrap();
        g.ptr(other_input, &["next"], None).unwrap();
        g.ptr(other_input, &["prev"], None).unwrap();
        g.ptr(other_input, &["link"], None).unwrap();
        g.set_list(third, &["inputs"], &[other_input]).unwrap();
        let other = g
            .link_new(tree, nodes[0], from, third, other_input)
            .unwrap();
        let new = g.link_new(tree, nodes[0], from, nodes[1], to).unwrap();
        assert_eq!(g.list(tree, &["links"]).unwrap(), [other, new]);
        assert_eq!(g.target(to, &["link"]).unwrap(), Some(new));
        assert_eq!(g.value(new, &["flag"]).unwrap(), 2);
        assert_eq!(
            g.link_new(tree, nodes[0], from, nodes[0], from)
                .unwrap_err()
                .kind,
            "link_owner"
        );
        let foreign = g.new_typed("bNode", Some(g.records[tree].owner)).unwrap();
        assert_eq!(
            g.link_new(tree, foreign, from, nodes[1], to)
                .unwrap_err()
                .kind,
            "link_owner"
        );
        g.set(to, &["flag"], 2048).unwrap();
        assert_eq!(
            g.link_new(tree, nodes[0], from, nodes[1], to)
                .unwrap_err()
                .kind,
            "link_profile"
        );
        assert_eq!(g.list(tree, &["links"]).unwrap(), [other, new]);
        g.set(to, &["flag"], 4).unwrap();
        g.link_remove(tree, new).unwrap();
        assert_eq!(g.target(to, &["link"]).unwrap(), None);
        assert_eq!(g.value(to, &["flag"]).unwrap() & 4, 0);
        assert_ne!(g.value(from, &["flag"]).unwrap() & 4, 0);
        g.verify().unwrap();
    }
}
