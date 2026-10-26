//! Version-pinned, expected-graph compositor transactions. Reuses the private
//! native-template graph operations; unchanged original allocations stay byte-exact.
use crate::edit::EditOptions;
use crate::edit_output::{AtomicOutput, io, sync_end};
use crate::file_ops::graph::{Graph, Record, text, uint};
use crate::input::Snapshot;
use crate::{BlendFile, Error, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const KIT: &[u8] = include_bytes!("../assets/template-5.2.1.blend");
const BUILD: &str = "9e2066aef7ef";
const SUPPORTED: &[&str] = &[
    "ShaderNodeMath",
    "ShaderNodeValue",
    "CompositorNodeRGB",
    "NodeReroute",
];

#[derive(Debug, Clone)]
struct Endpoint {
    node: String,
    socket: String,
    socket_type: String,
}
#[derive(Debug)]
enum Operation {
    AddNode {
        node_type: String,
        name: String,
    },
    RemoveNode {
        name: String,
        node_type: String,
    },
    AddLink {
        from: Endpoint,
        to: Endpoint,
    },
    RemoveLink {
        from: Endpoint,
        to: Endpoint,
    },
    Relink {
        from: Endpoint,
        to: Endpoint,
        replacement_from: Endpoint,
        replacement_to: Endpoint,
    },
}
#[derive(Debug)]
struct Spec {
    tree: usize,
    expected_graph: Value,
    operations: Vec<Operation>,
}
fn refusal(message: impl Into<String>) -> Error {
    Error::new("node_graph", message)
}
fn spec_error(message: &str) -> Error {
    Error::new("edit_spec", message)
}
fn keys(v: &Value, expected: &[&str]) -> Result<()> {
    let o = v.as_object().ok_or_else(|| spec_error("object required"))?;
    if o.len() != expected.len() || expected.iter().any(|k| !o.contains_key(*k)) {
        return Err(spec_error(
            "missing or unknown structural specification key",
        ));
    }
    Ok(())
}
fn string(v: &Value) -> Result<String> {
    v.as_str()
        .map(str::to_owned)
        .ok_or_else(|| spec_error("string required"))
}
fn parse_endpoint(v: &Value) -> Result<Endpoint> {
    keys(v, &["node", "socket", "socket_type"])?;
    Ok(Endpoint {
        node: string(&v["node"])?,
        socket: string(&v["socket"])?,
        socket_type: string(&v["socket_type"])?,
    })
}
fn parse(v: &Value) -> Result<Spec> {
    keys(v, &["schema", "tree", "expected_graph", "operations"])?;
    if v["schema"] != 1 {
        return Err(spec_error("node-tree schema must be 1"));
    }
    let tree = v["tree"]
        .as_u64()
        .and_then(|i| usize::try_from(i).ok())
        .ok_or_else(|| spec_error("tree block index required"))?;
    let operations = v["operations"]
        .as_array()
        .ok_or_else(|| spec_error("operations array required"))?
        .iter()
        .map(|o| {
            Ok(match o["op"].as_str() {
                Some("add_node") => {
                    keys(o, &["op", "node_type", "name"])?;
                    Operation::AddNode {
                        node_type: string(&o["node_type"])?,
                        name: string(&o["name"])?,
                    }
                }
                Some("remove_node") => {
                    keys(o, &["op", "node_type", "name"])?;
                    Operation::RemoveNode {
                        node_type: string(&o["node_type"])?,
                        name: string(&o["name"])?,
                    }
                }
                Some("add_link") => {
                    keys(o, &["op", "from", "to"])?;
                    Operation::AddLink {
                        from: parse_endpoint(&o["from"])?,
                        to: parse_endpoint(&o["to"])?,
                    }
                }
                Some("remove_link") => {
                    keys(o, &["op", "from", "to"])?;
                    Operation::RemoveLink {
                        from: parse_endpoint(&o["from"])?,
                        to: parse_endpoint(&o["to"])?,
                    }
                }
                Some("relink") => {
                    keys(
                        o,
                        &["op", "from", "to", "replacement_from", "replacement_to"],
                    )?;
                    Operation::Relink {
                        from: parse_endpoint(&o["from"])?,
                        to: parse_endpoint(&o["to"])?,
                        replacement_from: parse_endpoint(&o["replacement_from"])?,
                        replacement_to: parse_endpoint(&o["replacement_to"])?,
                    }
                }
                _ => return Err(spec_error("unknown structural operation")),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Spec {
        tree,
        expected_graph: v["expected_graph"].clone(),
        operations,
    })
}

/// FileGlobal's native build identity is required, not a caller's version claim.
fn version(f: &BlendFile) -> Result<()> {
    let check = (|| -> Result<()> {
        let globals = f
            .blocks()
            .iter()
            .enumerate()
            .filter(|(_, b)| b.code == *b"GLOB")
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if f.header().version != 502
            || !f.header().little_endian
            || f.header().pointer_size != 8
            || f.header().file_format != 1
            || globals.len() != 1
            || text(f, globals[0], &["build_hash"])? != BUILD
            || uint(f, globals[0], &["subversion"])? != 44
        {
            return Err(Error::new(
                "node_version",
                "structural profile requires native Blender 5.2.1 build 9e2066aef7ef, subversion 44, 64-bit little-endian format 1",
            ));
        }
        Ok(())
    })();
    check.map_err(|e| Error::new("node_version", e.message))
}
fn preserved(f: &BlendFile) -> Result<Graph<'_>> {
    let owners = crate::typed_edit::owner_runs(f)?;
    let mut g = Graph::new(f)?;
    // Graph helpers use exact addresses. Cross-owner reused addresses are safe for
    // the read API, but are not admitted by this deliberately narrower editor.
    let mut addresses = BTreeSet::new();
    for (i, b) in f.blocks().iter().enumerate() {
        if b.old_address != 0 && !addresses.insert(b.old_address) {
            return Err(refusal(
                "reused allocation addresses are not qualified for structural editing",
            ));
        }
        g.records.push(Record {
            code: b.code,
            sdna: b.sdna_index,
            count: b.count,
            data: f.payload(i)?.into_owned(),
            address: b.old_address,
            owner: owners[i].unwrap_or(i),
            pointers: crate::file_ops::pointer_layout(f, i)?.unwrap_or_default(),
            alive: true,
        });
    }
    g.reserve_source_addresses()?;
    Ok(g)
}
fn tree_check(g: &Graph<'_>, tree: usize) -> Result<()> {
    let r = g
        .records
        .get(tree)
        .ok_or_else(|| refusal("tree block not found"))?;
    if r.code != *b"NT\0\0"
        || g.ty(tree) != b"bNodeTree"
        || g.string(tree, &["idname"])? != "CompositorNodeTree"
        || g.value(tree, &["id", "lib"])? != 0
    {
        return Err(refusal(
            "only local, standalone compositor node-tree IDs are supported",
        ));
    }
    Ok(())
}
fn node(g: &Graph<'_>, tree: usize, name: &str) -> Result<usize> {
    let found = g
        .list(tree, &["nodes"])?
        .into_iter()
        .filter(|n| g.string(*n, &["name"]).ok().as_deref() == Some(name))
        .collect::<Vec<_>>();
    if found.len() != 1 {
        return Err(Error::new(
            "node_endpoint",
            format!("node {name:?} must identify exactly one tree member"),
        ));
    }
    Ok(found[0])
}
fn endpoint(g: &Graph<'_>, tree: usize, e: &Endpoint, direction: &str) -> Result<(usize, usize)> {
    let n = node(g, tree, &e.node)?;
    let sockets = g
        .list(n, &[direction])?
        .into_iter()
        .filter(|s| g.string(*s, &["identifier"]).ok().as_deref() == Some(e.socket.as_str()))
        .collect::<Vec<_>>();
    if sockets.len() != 1 {
        return Err(Error::new(
            "node_endpoint",
            "socket identifier/direction must identify exactly one member",
        ));
    }
    let s = sockets[0];
    if g.string(s, &["idname"])? != e.socket_type {
        return Err(Error::new(
            "node_socket_type",
            "endpoint socket type differs from exact expected type",
        ));
    }
    Ok((n, s))
}
fn member(g: &Graph<'_>, id: usize, tree: usize, ty: &[u8]) -> Result<()> {
    let r = &g.records[id];
    if r.code != *b"DATA" || r.count != 1 || r.owner != tree || g.ty(id) != ty {
        return Err(refusal(
            "graph member has wrong SDNA type, extent or tree owner",
        ));
    }
    Ok(())
}
fn sockets(g: &Graph<'_>, n: usize, direction: &str) -> Result<Value> {
    let items = g.list(n, &[direction])?;
    let mut identifiers = BTreeSet::new();
    for s in &items {
        member(g, *s, g.records[n].owner, b"bNodeSocket")?;
        if !identifiers.insert(g.string(*s, &["identifier"])?) {
            return Err(refusal("duplicate socket identifiers"));
        }
    }
    Ok(Value::Array(items.iter().map(|s| Ok(json!({"identifier":g.string(*s,&["identifier"])?,"socket_type":g.string(*s,&["idname"])?}))).collect::<Result<Vec<_>>>()?))
}
fn link_endpoint(g: &Graph<'_>, tree: usize, link: usize, direction: &str) -> Result<Endpoint> {
    let (nf, sf, sockets) = if direction == "from" {
        ("fromnode", "fromsock", "outputs")
    } else {
        ("tonode", "tosock", "inputs")
    };
    let n = g
        .target(link, &[nf])?
        .ok_or_else(|| Error::new("node_endpoint", "dangling/null link node"))?;
    let s = g
        .target(link, &[sf])?
        .ok_or_else(|| Error::new("node_endpoint", "dangling/null link socket"))?;
    if !g.list(tree, &["nodes"])?.contains(&n) || !g.list(n, &[sockets])?.contains(&s) {
        return Err(Error::new(
            "node_endpoint",
            "dangling link or wrong socket owner/direction",
        ));
    }
    Ok(Endpoint {
        node: g.string(n, &["name"])?,
        socket: g.string(s, &["identifier"])?,
        socket_type: g.string(s, &["idname"])?,
    })
}
fn endpoint_json(e: &Endpoint) -> Value {
    json!({"node":e.node,"socket":e.socket,"socket_type":e.socket_type})
}
fn describe(g: &Graph<'_>, tree: usize) -> Result<Value> {
    tree_check(g, tree)?;
    let mut nodes = BTreeMap::new();
    for n in g.list(tree, &["nodes"])? {
        member(g, n, tree, b"bNode")?;
        let name = g.string(n, &["name"])?;
        if nodes.insert(name.clone(), json!({"name":name,"node_type":g.string(n,&["idname"])?,"inputs":sockets(g,n,"inputs")?,"outputs":sockets(g,n,"outputs")?})).is_some() { return Err(refusal("duplicate node names")); }
    }
    let mut links = Vec::new();
    let mut edges = BTreeMap::<String, Vec<String>>::new();
    let mut incoming = nodes
        .keys()
        .map(|n| (n.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();
    let mut occupied = BTreeSet::new();
    for l in g.list(tree, &["links"])? {
        member(g, l, tree, b"bNodeLink")?;
        let from = link_endpoint(g, tree, l, "from")?;
        let to = link_endpoint(g, tree, l, "to")?;
        if from.socket_type != to.socket_type {
            return Err(Error::new(
                "node_socket_type",
                "implicit socket conversions are not supported",
            ));
        }
        let (_, socket) = endpoint(g, tree, &to, "inputs")?;
        if g.value(socket, &["flag"])? & 2048 != 0
            || !occupied.insert((to.node.clone(), to.socket.clone()))
        {
            return Err(refusal(
                "multi-input/occupied duplicate links are not supported",
            ));
        }
        edges
            .entry(from.node.clone())
            .or_default()
            .push(to.node.clone());
        *incoming
            .get_mut(&to.node)
            .ok_or_else(|| refusal("missing target"))? += 1;
        links.push(json!({"from":endpoint_json(&from),"to":endpoint_json(&to)}));
    }
    let mut ready = incoming
        .iter()
        .filter(|(_, v)| **v == 0)
        .map(|(n, _)| n.clone())
        .collect::<Vec<_>>();
    let mut visited = 0;
    while let Some(n) = ready.pop() {
        visited += 1;
        for target in edges.get(&n).into_iter().flatten() {
            let count = incoming
                .get_mut(target)
                .ok_or_else(|| refusal("missing cycle target"))?;
            *count -= 1;
            if *count == 0 {
                ready.push(target.clone());
            }
        }
    }
    if visited != nodes.len() {
        return Err(Error::new(
            "node_cycle",
            "compositor cycles are not supported",
        ));
    }
    links.sort_by_key(Value::to_string);
    Ok(
        json!({"name":g.string(tree,&["id","name"])?,"node_type":"CompositorNodeTree","nodes":nodes.into_values().collect::<Vec<_>>(),"links":links}),
    )
}
/// Read the exact graph used as the expected-old guard. Tree block indices belong
/// only to this input snapshot; they must never be reused after native resaving.
pub fn inspect(input: &Path) -> Result<Value> {
    let span = crate::diagnostics::Span::new(
        "node_tree_inspect",
        json!({"path":input.to_string_lossy(),"profile_build":BUILD}),
    );
    let result = (|| {
        let session = crate::Session::open(input).into_result()?;
        let captured = session.store.snapshot(session.root)?;
        version(&captured.reader)?;
        let g = preserved(&captured.reader)?;
        let mut trees = Vec::new();
        for (i, r) in g.records.iter().enumerate() {
            if r.code == *b"NT\0\0" && g.string(i, &["idname"])? == "CompositorNodeTree" {
                trees.push(json!({"block_index":i,"graph":describe(&g,i)?}));
            }
        }
        captured.check(input)?;
        Ok(
            json!({"schema":1,"operation":"node_tree_inspect","profile_build":BUILD,"trees":trees,"complete":true,"native_verified":false}),
        )
    })();
    span.result(result)
}
fn exact_link(g: &Graph<'_>, tree: usize, from: &Endpoint, to: &Endpoint) -> Result<usize> {
    let (n, s) = endpoint(g, tree, from, "outputs")?;
    let (t, u) = endpoint(g, tree, to, "inputs")?;
    for l in g.list(tree, &["links"])? {
        if g.target(l, &["fromnode"])? == Some(n)
            && g.target(l, &["fromsock"])? == Some(s)
            && g.target(l, &["tonode"])? == Some(t)
            && g.target(l, &["tosock"])? == Some(u)
        {
            return Ok(l);
        }
    }
    Err(Error::new("node_endpoint", "expected link not present"))
}
fn add_link(g: &mut Graph<'_>, tree: usize, from: &Endpoint, to: &Endpoint) -> Result<()> {
    let (n, s) = endpoint(g, tree, from, "outputs")?;
    let (t, u) = endpoint(g, tree, to, "inputs")?;
    if from.socket_type != to.socket_type {
        return Err(Error::new(
            "node_socket_type",
            "exact source/target socket types must match",
        ));
    }
    // Occupied-input replacement is explicit via relink/remove_link, never an
    // implicit side effect of the underlying primitive.
    if g.list(tree, &["links"])?
        .iter()
        .any(|l| g.target(*l, &["tosock"]).ok() == Some(Some(u)))
    {
        return Err(Error::new(
            "node_occupied",
            "remove/relink the guarded existing input link first",
        ));
    }
    g.link_new(tree, n, s, t, u)?;
    Ok(())
}
// Deliberately retain storage rather than inventing a generic node destructor.
// Expose its direct roots so a removal never hides the resulting orphan islands.
fn removal_retention(g: &Graph<'_>, tree: usize, op: &Operation) -> Result<Option<Value>> {
    let Operation::RemoveNode { name, .. } = op else {
        return Ok(None);
    };
    let n = node(g, tree, name)?;
    let mut roots = BTreeSet::new();
    for direction in ["inputs", "outputs"] {
        for socket in g.list(n, &[direction])? {
            roots.insert(socket);
            if let Some(default) = g.target(socket, &["default_value"])? {
                roots.insert(default);
            }
        }
    }
    for field in ["storage", "prop"] {
        if let Some(root) = g.target(n, &[field])? {
            roots.insert(root);
        }
    }
    let roots=roots.into_iter().map(|i|json!({"transaction_record":i,"saved_address_hex":format!("{:x}",g.records[i].address),"sdna_type":if g.records[i].sdna==0 {"opaque".to_owned()} else {String::from_utf8_lossy(g.ty(i)).into_owned()}})).collect::<Vec<_>>();
    Ok(Some(
        json!({"node":name,"retained_allocation_roots":roots,"coverage":"direct sockets, socket defaults, node storage/properties; their transitive children are retained too","disconnect_changes_in_changed_original_blocks":true}),
    ))
}
fn apply(g: &mut Graph<'_>, tree: usize, kit: &BlendFile, op: &Operation) -> Result<()> {
    match op {
        Operation::AddNode { node_type, name } => {
            if !SUPPORTED.contains(&node_type.as_str()) {
                return Err(Error::new(
                    "node_constructor",
                    format!(
                        "unsupported node type {node_type}; supported: {}",
                        SUPPORTED.join(", ")
                    ),
                ));
            }
            g.node_new(tree, kit, node_type, name)?;
        }
        Operation::RemoveNode { name, node_type } => {
            let n = node(g, tree, name)?;
            if g.string(n, &["idname"])? != *node_type {
                return Err(Error::new(
                    "node_expected_graph",
                    "remove node type differs",
                ));
            }
            if g.value(tree, &["adt"])? != 0 {
                return Err(refusal(
                    "node removal with tree animation is not qualified; no animation-orphan guessing",
                ));
            }
            g.node_remove(tree, n)?;
        }
        Operation::AddLink { from, to } => add_link(g, tree, from, to)?,
        Operation::RemoveLink { from, to } => {
            let l = exact_link(g, tree, from, to)?;
            g.link_remove(tree, l)?;
        }
        Operation::Relink {
            from,
            to,
            replacement_from,
            replacement_to,
        } => {
            let l = exact_link(g, tree, from, to)?;
            g.link_remove(tree, l)?;
            add_link(g, tree, replacement_from, replacement_to)?;
        }
    }
    describe(g, tree)?;
    Ok(())
}
fn emit(g: &Graph<'_>, original_count: usize, tree: usize) -> Result<Vec<u8>> {
    // Original runs, physical order and addresses stay unchanged. New DATA goes
    // into precisely the edited NT owner run, not after ENDB or another owner.
    let insert_after = (0..original_count)
        .filter(|i| g.records[*i].owner == tree)
        .max()
        .ok_or_else(|| refusal("missing tree run"))?;
    let mut out = g.schema.serialized_header()?;
    for i in 0..original_count {
        if g.records[i].alive {
            g.encode_block(&g.records[i], &mut out)?;
        }
        if i == insert_after {
            for r in &g.records[original_count..] {
                if r.alive {
                    if r.owner != tree {
                        return Err(refusal("new allocation escaped edited tree owner"));
                    }
                    g.encode_block(r, &mut out)?;
                }
            }
        }
    }
    g.encode_block(
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
fn publish(
    captured: &Snapshot,
    input: &Path,
    output: &Path,
    data: &[u8],
    options: &EditOptions,
    mut report: Value,
) -> Result<Value> {
    captured.check(input)?;
    let mut stage = AtomicOutput::create(&captured.metadata, output)?;
    stage.file.write_all(data).map_err(io)?;
    stage.file.seek(SeekFrom::Start(0)).map_err(io)?;
    let mut readback = Vec::new();
    stage.file.read_to_end(&mut readback).map_err(io)?;
    if readback != data {
        return Err(Error::new(
            "declared_diff",
            "staged structural output differs",
        ));
    }
    let verify_span =
        crate::diagnostics::Span::new("node_tree.verify", json!({"decoded_bytes":data.len()}));
    let verification = (|| {
        let check = BlendFile::from_bytes(readback)?;
        let g = preserved(&check)?;
        let tree = g
            .records
            .iter()
            .enumerate()
            .find(|(i, r)| {
                r.code == *b"NT\0\0"
                    && g.string(*i, &["id", "name"]).ok()
                        == report["after"]["name"].as_str().map(str::to_owned)
            })
            .map(|(i, _)| i)
            .ok_or_else(|| refusal("staged tree missing"))?;
        if describe(&g, tree)? != report["after"] {
            return Err(Error::new("declared_diff", "staged graph differs"));
        }
        Ok(())
    })();
    verify_span.result(verification)?;
    captured.check(input)?;
    let mut error = stage.publish(options.durability).err();
    if error.is_none() && options.durability == crate::edit::Durability::End {
        match sync_end(
            output
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        ) {
            Ok(()) => stage.durability_completed = true,
            Err(e) => error = Some(e),
        }
    }
    report["complete"] = json!(error.is_none());
    report["output_written"] = json!(stage.published);
    report["durability_completed"] = json!(stage.durability_completed);
    report["errors"] = json!(
        error
            .iter()
            .map(|e| json!({"kind":e.kind,"message":e.message}))
            .collect::<Vec<_>>()
    );
    Ok(report)
}
/// Schema-1 structural specification, separate from fixed-width schema-2 edits.
pub fn edit(input: &Path, output: &Path, value: &Value, options: &EditOptions) -> Result<Value> {
    let span = crate::diagnostics::Span::new(
        "node_tree_edit",
        json!({"profile_build":BUILD,"path":input.to_string_lossy()}),
    );
    let result = (|| {
        let plan_span = crate::diagnostics::Span::new("node_tree.plan", json!({"spec_schema":1}));
        let spec = plan_span.result(parse(value))?;
        let session = crate::Session::open(input).into_result()?;
        let captured = session.store.snapshot(session.root)?;
        let f = &captured.reader;
        version(f)?;
        for (i, _) in f
            .blocks()
            .iter()
            .enumerate()
            .filter(|(_, b)| b.code == *b"LI\0\0")
        {
            for key in ["packedfile", "archive_parent_library"] {
                if uint(f, i, &[key])? != 0 {
                    return Err(Error::new(
                        "profile_library",
                        "packed/archive libraries require their own profile",
                    ));
                }
            }
            let location =
                f.locate_field(i, 0, &[crate::value::PathStep::Field(b"name".to_vec())])?;
            let raw = f.field_bytes(i, location.offset, location.field.size)?;
            let end = raw.iter().position(|b| *b == 0).ok_or_else(|| {
                Error::new("string_termination", "Library path lacks bounded NUL")
            })?;
            crate::typed_edit::same_library_content(input, &raw[..end], &raw[..end], output)?;
        }
        let kit = BlendFile::from_bytes(zstd::stream::decode_all(KIT).map_err(io)?)?;
        let validation = crate::diagnostics::Span::new(
            "node_tree.validate",
            json!({"tree_block":spec.tree,"profile_build":BUILD}),
        );
        let (mut g, before) = validation.result((|| {
            version(f)?;
            version(&kit)?;
            let g = preserved(f)?;
            g.compatible(&kit)?;
            let before = describe(&g, spec.tree)?;
            if before != spec.expected_graph {
                return Err(Error::new(
                    "node_expected_graph",
                    "expected graph differs from saved input; inspect this exact file",
                ));
            }
            Ok((g, before))
        })())?;
        let original = g.records.len();
        let mut retention = Vec::new();
        for (index, op) in spec.operations.iter().enumerate() {
            let operation = crate::diagnostics::Span::new(
                "node_tree.apply",
                json!({"tree_block":spec.tree,"operation":value["operations"][index]["op"],"operation_index":index}),
            );
            let retained = operation.result((|| {
                let retained = removal_retention(&g, spec.tree, op)?;
                apply(&mut g, spec.tree, &kit, op)?;
                Ok(retained)
            })())?;
            if let Some(retained) = retained {
                retention.push(retained);
            }
            crate::diagnostics::point(
                "node_tree.operation",
                json!({"operation_index":index,"applied":true}),
            );
        }
        // Every untouched original record remains exact. Removal cannot leave a
        // saved typed referrer to the deleted allocation (including parent nodes).
        let removed = g
            .records
            .iter()
            .filter(|r| !r.alive)
            .map(|r| r.address)
            .collect::<BTreeSet<_>>();
        for r in g.records.iter().filter(|r| r.alive) {
            for p in &r.pointers {
                let a = crate::checked::number(&r.data, p.offset, f.header().pointer_size, true)?;
                if a != 0 && removed.contains(&a) {
                    return Err(refusal("node removal leaves a saved pointer referrer"));
                }
            }
        }
        let after = describe(&g, spec.tree)?;
        let mut changed = Vec::new();
        for i in 0..original {
            if !g.records[i].alive || g.records[i].data.as_slice() != f.payload(i)?.as_ref() {
                if g.records[i].owner != spec.tree
                    || !matches!(
                        g.ty(i),
                        b"bNodeTree" | b"bNode" | b"bNodeSocket" | b"bNodeLink"
                    )
                {
                    return Err(refusal("mutation escaped supported tree records"));
                }
                changed.push(i);
            }
        }
        let added = g.records[original..].iter().filter(|r| r.alive).count();
        let emit_span = crate::diagnostics::Span::new(
            "node_tree.emit",
            json!({"tree_block":spec.tree,"changed_original_blocks":changed.len(),"added_blocks":added}),
        );
        let data = emit_span.result(emit(&g, original, spec.tree))?;
        publish(
            captured,
            input,
            output,
            &data,
            options,
            json!({"schema":1,"operation":"node_tree_edit","profile_build":BUILD,"input_path_hex":crate::value::hex(input.as_os_str().as_encoded_bytes()),"output_path_hex":crate::value::hex(output.as_os_str().as_encoded_bytes()),"input_decoded_length":f.decompressed_len(),"output_decoded_length":data.len(),"before":before,"after":after,"changed_original_blocks":changed,"added_blocks":added,"removed_node_retention":retention,"removal_allocation_policy":"retain unreferenced socket/default/storage/property allocations; no garbage collection","operations":value["operations"],"output_written":false,"complete":false,"layout_verified":true,"graph_verified":true,"native_verified":false,"semantic_verified":false,"container_family":"raw","durability":options.durability.as_str(),"durability_completed":false,"animation_policy":"node removal refuses animated trees; no animation paths rewritten","unchanged_original_allocations_preserved":true}),
        )
    })();
    span.result(result)
}

#[cfg(test)]
#[path = "node_tree_edit_hostile_tests.rs"]
mod hostile_tests;
#[cfg(test)]
#[path = "node_tree_edit_tests.rs"]
mod tests;
