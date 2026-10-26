//! Saved collection membership and ID-ownership graph.
//!
//! The projection starts from a saved Scene master collection or a saved
//! Collection and follows the serialized `Collection.children` and
//! `Collection.gobject` lists. A pointer target that is a bare saved `ID`
//! placeholder is resolved through the shared #30 name selector and the
//! two-phase linked-byte context; the actual local record is then read from
//! that library. Records are deduplicated by (lexical source, block, element),
//! never by ID name or old address. Edge ordinals are the saved list order.
//!
//! Traversal is iterative. Each record owns only the diagnostics found while
//! reading that record, and each edge owns only the diagnostics found while
//! resolving or scheduling that edge. A row's errors are derived by iterative
//! reachability through the cached edges and object-parent references, so a
//! shared subgraph does not copy descendant errors and a deep valid graph does
//! not consume call-stack or quadratic diagnostic state.
//!
//! This is a saved-edge graph, not evaluated visibility. It makes no render
//! presence, depsgraph, category, sky or material claim. Disabled branches and
//! object parents outside the seed membership remain saved relations.
//!
//! Keys in this graph are source-local ordinals (`s0`, `s1`, ... plus block and
//! element). They are not durable handles across queries, saves or files. A
//! source is a lexical file context; symlink aliases stay distinct, matching the
//! shared library request/ownership contract. Root sources are absolute lexical
//! paths whenever a source path was supplied.

use crate::query::access::{self, Record, list_partial as checked_list};
use crate::query::linked;
use crate::value::{Value, hex};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const GRAPH_REF: &str = "collection.graph";
const GRAPH_BASIS: &str = "saved_collection_edges_not_evaluated_visibility";
const OWNERSHIP_SCOPE: &str = "containing_saved_file";

type EdgeKey = (String, String, u64);

/// One seed's reference into the shared query graph.
pub(super) struct Projection {
    pub value: Json,
    pub errors: Vec<Error>,
}

/// State shared by every `collection.graph` row in one query. It owns JSON only;
/// no file or library borrow survives a call.
pub(super) struct Graph {
    sources: Vec<Source>,
    source_index: HashMap<PathBuf, usize>,
    root_ready: bool,
    records: Vec<Json>,
    record_index: HashMap<String, usize>,
    edges: Vec<Json>,
    edges_by_from: HashMap<String, Vec<usize>>,
    edge_index: HashMap<EdgeKey, usize>,
    diagnostics: Vec<Json>,
    diagnostic_errors: Vec<Error>,
    diagnostic_index: HashMap<String, usize>,
    record_diagnostics: HashMap<String, Vec<usize>>,
    edge_diagnostics: HashMap<usize, Vec<usize>>,
    expanded_collections: HashSet<String>,
    expanded_objects: HashSet<String>,
    node_info: HashMap<String, NodeInfo>,
}

struct Source {
    key: String,
    role: &'static str,
    path: Option<PathBuf>,
    provenance: Vec<Json>,
    provenance_keys: HashSet<String>,
    requests: Vec<Json>,
    request_keys: HashSet<String>,
}

#[derive(Clone)]
struct NodeInfo {
    source: usize,
    record: Record,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    Collection,
    Object,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ObjectRelation {
    Membership,
    Parent,
}

struct Pending {
    key: String,
    kind: NodeKind,
}

enum Frame {
    Enter(Pending),
    Exit { key: String, kind: NodeKind },
}

#[derive(Clone, Copy)]
enum Reader<'r, 'l> {
    Root(&'r BlendFile),
    Library(&'l BlendFile),
}

struct Traversal<'r, 'l> {
    stack: Vec<Frame>,
    active_collections: HashSet<String>,
    active_objects: HashSet<String>,
    readers: HashMap<usize, Reader<'r, 'l>>,
}

struct Stub {
    name: Vec<u8>,
    library: Vec<u8>,
}

/// Outcome of following one saved pointer. `key` is present whenever the saved
/// target record is known, even when a cycle stopped expansion. `diagnostics`
/// are the direct IDs owned by the edge or parent relation that reached it.
struct Link {
    key: Option<String>,
    source: Option<usize>,
    status: &'static str,
    provenance: Vec<Json>,
    diagnostics: Vec<usize>,
}

impl Link {
    fn none(status: &'static str) -> Self {
        Self {
            key: None,
            source: None,
            status,
            provenance: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn none_with(status: &'static str, diagnostics: Vec<usize>) -> Self {
        Self {
            key: None,
            source: None,
            status,
            provenance: Vec::new(),
            diagnostics,
        }
    }
}

impl Graph {
    pub(super) fn new() -> Self {
        Self {
            sources: Vec::new(),
            source_index: HashMap::new(),
            root_ready: false,
            records: Vec::new(),
            record_index: HashMap::new(),
            edges: Vec::new(),
            edges_by_from: HashMap::new(),
            edge_index: HashMap::new(),
            diagnostics: Vec::new(),
            diagnostic_errors: Vec::new(),
            diagnostic_index: HashMap::new(),
            record_diagnostics: HashMap::new(),
            edge_diagnostics: HashMap::new(),
            expanded_collections: HashSet::new(),
            expanded_objects: HashSet::new(),
            node_info: HashMap::new(),
        }
    }

    /// Add one seed to the graph. Recoverable missing sites stay in `errors`
    /// and in the report-level graph diagnostics; only an observed input
    /// change aborts the batch.
    pub(super) fn project<'r, 'l>(
        &mut self,
        file: &'r BlendFile,
        seed: &Record,
        source: Option<&Path>,
        links: &mut linked::Context<'_, 'l>,
    ) -> Result<Projection> {
        let root_source = self.ensure_root(source)?;
        let mut traversal = Traversal {
            stack: Vec::new(),
            active_collections: HashSet::new(),
            active_objects: HashSet::new(),
            readers: HashMap::new(),
        };
        traversal.readers.insert(root_source, Reader::Root(file));
        let mut seed_diagnostics = Vec::new();
        let root = match self.seed(
            file,
            seed,
            root_source,
            links,
            &mut traversal,
            &mut seed_diagnostics,
        ) {
            Ok(root) => root,
            Err(error) if matches!(error.kind, "input_changed" | "source_changed") => {
                return Err(error);
            }
            Err(error) => {
                let id = self.note(root_source, Some(seed), "Seed", error)?;
                seed_diagnostics.push(id);
                None
            }
        };
        if root.is_some() {
            self.expand_all(links, &mut traversal)?;
        }
        let mut ids = seed_diagnostics;
        if let Some(root) = &root {
            ids.extend(self.reachable_diagnostic_ids(root));
        }
        Ok(Projection {
            value: json!({"graph_ref":GRAPH_REF, "root":root}),
            errors: self.errors_for_ids(&ids),
        })
    }

    /// The flat report-level graph. Records/edges/sources are stored once for
    /// the whole query, so selecting every Collection does not repeat bodies.
    pub(super) fn json(&self) -> Json {
        let sources = self
            .sources
            .iter()
            .map(|source| {
                json!({
                    "key": source.key,
                    "role": source.role,
                    "lexical_path": source.path.as_ref().and_then(|p| p.to_str()),
                    "path_hex": source.path.as_ref().map(|p| hex(p.as_os_str().as_encoded_bytes())),
                    "provenance": &source.provenance,
                    "requests": &source.requests,
                })
            })
            .collect::<Vec<_>>();
        let unresolved_edges = self
            .edges
            .iter()
            .filter(|edge| !matches!(edge["resolution"].as_str(), Some("local") | Some("library")))
            .count();
        json!({
            "schema": "oxblend.collection_graph/1",
            "graph_ref": GRAPH_REF,
            "basis": GRAPH_BASIS,
            "sources": sources,
            "records": &self.records,
            "edges": &self.edges,
            "diagnostics": &self.diagnostics,
            "coverage": {
                "record_count": self.records.len(),
                "edge_count": self.edges.len(),
                "unresolved_edge_count": unresolved_edges,
            },
            "complete": self.diagnostics.is_empty(),
            "limits": [
                "Saved collection child/object edges and saved ID ownership only; no evaluated visibility, render presence, category, sky or material result.",
                "Ownership is scoped to the containing saved file; a linked library body's local ID is not described as root-local.",
                "Absent optional fields are omitted; unavailable scalar/reference fields carry status and a diagnostic ID, not a saved-null substitute.",
                "Source-local keys are query ordinals, not durable handles across files, saves or queries.",
                "Object parent/type/restrictflag and override references are raw saved relations; membership is not a render claim.",
                "Row errors are derived by iterative saved-edge and object-parent reachability, not copied from descendant caches."
            ],
        })
    }

    fn ensure_root(&mut self, source: Option<&Path>) -> Result<usize> {
        if self.root_ready {
            return Ok(0);
        }
        let path = source.map(linked::absolute_lexical).transpose()?;
        let mut root = Source {
            key: "s0".into(),
            role: "root",
            path: path.clone(),
            provenance: Vec::new(),
            provenance_keys: HashSet::new(),
            requests: Vec::new(),
            request_keys: HashSet::new(),
        };
        let entry = json!({
            "kind": "root_source",
            "lexical_path": path.as_ref().and_then(|p| p.to_str()),
            "path_hex": path.as_ref().map(|p| hex(p.as_os_str().as_encoded_bytes())),
        });
        root.provenance_keys.insert(entry.to_string());
        root.provenance.push(entry);
        if let Some(path) = &path {
            self.source_index.insert(path.clone(), 0);
        }
        self.sources.push(root);
        self.root_ready = true;
        Ok(0)
    }

    /// One source per lexical library context. The actual local ID's path is
    /// the owner of later relative library references. Canonical/realpath
    /// identity stays in retained provenance and never collapses aliases.
    fn source_for_library(&mut self, path: &Path, provenance: &[Json], request: Json) -> usize {
        let path = crate::trace::lexical(path);
        let index = if let Some(&index) = self.source_index.get(&path) {
            index
        } else {
            let key = format!("s{}", self.sources.len());
            self.sources.push(Source {
                key,
                role: "library",
                path: Some(path.clone()),
                provenance: Vec::new(),
                provenance_keys: HashSet::new(),
                requests: Vec::new(),
                request_keys: HashSet::new(),
            });
            let index = self.sources.len() - 1;
            self.source_index.insert(path, index);
            index
        };
        let source = &mut self.sources[index];
        for entry in provenance {
            let key = entry.to_string();
            if source.provenance_keys.insert(key) {
                source.provenance.push(entry.clone());
            }
        }
        let key = request.to_string();
        if source.request_keys.insert(key) {
            source.requests.push(request);
        }
        index
    }

    fn note(
        &mut self,
        source: usize,
        record: Option<&Record>,
        field: &str,
        mut error: Error,
    ) -> Result<usize> {
        if matches!(error.kind, "input_changed" | "source_changed") {
            return Err(error);
        }
        let source_key = self
            .sources
            .get(source)
            .map(|s| s.key.clone())
            .unwrap_or_else(|| "s?".into());
        let lexical_path = self
            .sources
            .get(source)
            .and_then(|s| s.path.as_ref())
            .and_then(|p| p.to_str())
            .map(str::to_owned);
        let site = record.map(|r| json!({"block_index":r.block,"element":r.element}));
        let message = match record {
            Some(record) => format!(
                "{source_key} block {} element {} {field}: {}",
                record.block, record.element, error.message
            ),
            None => format!("{source_key} {field}: {}", error.message),
        };
        let diagnostic = json!({
            "status": "unavailable",
            "source": source_key,
            "lexical_path": lexical_path,
            "record": site,
            "field": field,
            "kind": error.kind,
            "message": &message,
        });
        let key = diagnostic.to_string();
        if let Some(&id) = self.diagnostic_index.get(&key) {
            return Ok(id);
        }
        let id = self.diagnostics.len();
        self.diagnostics.push(diagnostic);
        error.message = message;
        self.diagnostic_errors.push(error);
        self.diagnostic_index.insert(key, id);
        Ok(id)
    }

    fn attach_record(&mut self, key: &str, ids: Vec<usize>) {
        let target = self.record_diagnostics.entry(key.to_owned()).or_default();
        for id in ids {
            if !target.contains(&id) {
                target.push(id);
            }
        }
    }

    fn attach_edge(&mut self, edge: usize, ids: Vec<usize>) {
        let target = self.edge_diagnostics.entry(edge).or_default();
        for id in ids {
            if !target.contains(&id) {
                target.push(id);
            }
        }
    }

    fn insert_record(&mut self, key: String, record: Json) {
        if self.record_index.contains_key(&key) {
            return;
        }
        self.record_index.insert(key, self.records.len());
        self.records.push(record);
    }

    fn set_data(&mut self, key: &str, field: &str, value: Json) {
        if let Some(&index) = self.record_index.get(key)
            && let Some(data) = self.records[index]
                .get_mut("data")
                .and_then(Json::as_object_mut)
        {
            data.insert(field.into(), value);
        }
    }

    fn add_edge(&mut self, from: &str, kind: &str, ordinal: u64, edge: Json) -> usize {
        let key = (from.to_owned(), kind.to_owned(), ordinal);
        if let Some(&index) = self.edge_index.get(&key) {
            if edge["resolution"] == "cycle"
                && let Some(object) = self.edges[index].as_object_mut()
            {
                object.insert("resolution".into(), json!("cycle"));
            }
            return index;
        }
        let index = self.edges.len();
        self.edge_index.insert(key, index);
        self.edges_by_from
            .entry(from.to_owned())
            .or_default()
            .push(index);
        self.edges.push(edge);
        index
    }

    fn is_expanded(&self, kind: NodeKind, key: &str) -> bool {
        match kind {
            NodeKind::Collection => self.expanded_collections.contains(key),
            NodeKind::Object => self.expanded_objects.contains(key),
        }
    }

    fn mark_expanded(&mut self, kind: NodeKind, key: &str) {
        match kind {
            NodeKind::Collection => {
                self.expanded_collections.insert(key.to_owned());
            }
            NodeKind::Object => {
                self.expanded_objects.insert(key.to_owned());
            }
        }
    }

    fn seed<'f, 'r, 'l>(
        &mut self,
        file: &'f BlendFile,
        seed: &Record,
        source: usize,
        links: &mut linked::Context<'_, 'l>,
        traversal: &mut Traversal<'r, 'l>,
        diagnostics: &mut Vec<usize>,
    ) -> Result<Option<String>> {
        let saved_type = type_name(file, seed)?.to_vec();
        match saved_type.as_slice() {
            b"Scene" => self.visit_scene(file, seed, source, links, traversal, diagnostics),
            b"Collection" => {
                let link = self.visit_collection_target(
                    file,
                    seed,
                    source,
                    links,
                    traversal,
                    "Collection",
                )?;
                diagnostics.extend(link.diagnostics);
                Ok(link.key)
            }
            b"ID" => {
                let name = match id_name_bytes(file, seed) {
                    Ok(name) => name,
                    Err(error) => {
                        diagnostics.push(self.note(source, Some(seed), "Seed.id.name", error)?);
                        return Ok(None);
                    }
                };
                match name.as_slice() {
                    [b'S', b'C', ..] => {
                        self.visit_scene(file, seed, source, links, traversal, diagnostics)
                    }
                    [b'G', b'R', ..] => {
                        let link = self.visit_collection_target(
                            file,
                            seed,
                            source,
                            links,
                            traversal,
                            "Collection",
                        )?;
                        diagnostics.extend(link.diagnostics);
                        Ok(link.key)
                    }
                    _ => {
                        let error = Error::new(
                            "type",
                            "collection.graph seed placeholder is not a Scene or Collection ID",
                        );
                        diagnostics.push(self.note(source, Some(seed), "Seed.id.name", error)?);
                        Ok(None)
                    }
                }
            }
            _ => {
                let error = Error::new(
                    "type",
                    format!(
                        "collection.graph requires a Scene or Collection seed, found {}",
                        String::from_utf8_lossy(&saved_type)
                    ),
                );
                diagnostics.push(self.note(source, Some(seed), "Seed", error)?);
                Ok(None)
            }
        }
    }

    /// A Scene seed contributes its saved `master_collection` closure. A linked
    /// Scene placeholder is resolved first so the master collection is read
    /// from the actual local Scene bytes.
    fn visit_scene<'f, 'r, 'l>(
        &mut self,
        file: &'f BlendFile,
        scene: &Record,
        source: usize,
        links: &mut linked::Context<'_, 'l>,
        traversal: &mut Traversal<'r, 'l>,
        diagnostics: &mut Vec<usize>,
    ) -> Result<Option<String>> {
        if type_name(file, scene)? == b"ID" {
            let Some(stub) =
                self.stub_identity(file, scene, source, "Seed", Some(&b"SC"[..]), diagnostics)?
            else {
                return Ok(None);
            };
            let owner = self.sources[source].path.clone();
            let reason = "collection.graph scene seed";
            let request = self.request_json(
                source,
                scene,
                "Scene.master_collection",
                &stub.name,
                &stub.library,
                reason,
            );
            let resolved =
                match links.resolve_id(owner.as_deref(), &stub.library, &stub.name, reason) {
                    Ok(resolved) => resolved,
                    Err(error) => {
                        diagnostics.push(self.note(
                            source,
                            Some(scene),
                            "Scene.master_collection",
                            error,
                        )?);
                        return Ok(None);
                    }
                };
            let path = crate::trace::lexical(&resolved.path);
            let actual_source = self.source_for_library(&path, &resolved.provenance, request);
            traversal
                .readers
                .entry(actual_source)
                .or_insert(Reader::Library(resolved.reader));
            let actual_type = match type_name(resolved.reader, &resolved.record) {
                Ok(kind) => kind.to_vec(),
                Err(error) => {
                    diagnostics.push(self.note(
                        actual_source,
                        Some(&resolved.record),
                        "Scene",
                        error,
                    )?);
                    return Ok(None);
                }
            };
            if actual_type != b"Scene" {
                let error = Error::new(
                    "type",
                    "selected linked scene placeholder did not resolve to a saved Scene",
                );
                diagnostics.push(self.note(
                    actual_source,
                    Some(&resolved.record),
                    "Scene",
                    error,
                )?);
                return Ok(None);
            }
            return self.master_collection(
                resolved.reader,
                &resolved.record,
                actual_source,
                links,
                traversal,
                diagnostics,
            );
        }
        self.master_collection(file, scene, source, links, traversal, diagnostics)
    }

    fn master_collection<'f, 'r, 'l>(
        &mut self,
        file: &'f BlendFile,
        scene: &Record,
        source: usize,
        links: &mut linked::Context<'_, 'l>,
        traversal: &mut Traversal<'r, 'l>,
        diagnostics: &mut Vec<usize>,
    ) -> Result<Option<String>> {
        let target = match access::pointer(file, scene, "master_collection") {
            Ok(Some(target)) => target,
            Ok(None) => {
                let error = Error::new(
                    "invalid_saved_structure",
                    "Scene has no saved master_collection pointer",
                );
                diagnostics.push(self.note(
                    source,
                    Some(scene),
                    "Scene.master_collection",
                    error,
                )?);
                return Ok(None);
            }
            Err(error) => {
                diagnostics.push(self.note(
                    source,
                    Some(scene),
                    "Scene.master_collection",
                    error,
                )?);
                return Ok(None);
            }
        };
        let link = self.visit_collection_target(
            file,
            &target,
            source,
            links,
            traversal,
            "Scene.master_collection",
        )?;
        diagnostics.extend(link.diagnostics);
        Ok(link.key)
    }

    fn visit_collection_target<'f, 'r, 'l>(
        &mut self,
        file: &'f BlendFile,
        target: &Record,
        source: usize,
        links: &mut linked::Context<'_, 'l>,
        traversal: &mut Traversal<'r, 'l>,
        field: &str,
    ) -> Result<Link> {
        let mut diagnostics = Vec::new();
        let saved_type = match type_name(file, target) {
            Ok(kind) => kind.to_vec(),
            Err(error) => {
                diagnostics.push(self.note(source, Some(target), field, error)?);
                return Ok(Link::none_with("unavailable", diagnostics));
            }
        };
        if saved_type == b"ID" {
            let Some(stub) = self.stub_identity(
                file,
                target,
                source,
                field,
                Some(&b"GR"[..]),
                &mut diagnostics,
            )?
            else {
                return Ok(Link::none_with("unavailable", diagnostics));
            };
            let owner = self.sources[source].path.clone();
            let reason = match field {
                "Scene.master_collection" => "collection.graph scene seed",
                "Collection.children" => "collection.graph linked collection membership",
                _ => "collection.graph collection seed",
            };
            let request =
                self.request_json(source, target, field, &stub.name, &stub.library, reason);
            let resolved =
                match links.resolve_id(owner.as_deref(), &stub.library, &stub.name, reason) {
                    Ok(resolved) => resolved,
                    Err(error) => {
                        let status = if error.kind == "query_cycle" {
                            "cycle"
                        } else {
                            "unavailable"
                        };
                        diagnostics.push(self.note(source, Some(target), field, error)?);
                        return Ok(Link::none_with(status, diagnostics));
                    }
                };
            let path = crate::trace::lexical(&resolved.path);
            let provenance = resolved.provenance.clone();
            let target_source = self.source_for_library(&path, &provenance, request);
            traversal
                .readers
                .entry(target_source)
                .or_insert(Reader::Library(resolved.reader));
            let actual_type = match type_name(resolved.reader, &resolved.record) {
                Ok(kind) => kind.to_vec(),
                Err(error) => {
                    diagnostics.push(self.note(
                        target_source,
                        Some(&resolved.record),
                        field,
                        error,
                    )?);
                    return Ok(Link::none_with("unavailable", diagnostics));
                }
            };
            if actual_type == b"ID" {
                let error = Error::new(
                    "missing_id",
                    "linked collection placeholder did not resolve to local saved content",
                );
                diagnostics.push(self.note(target_source, Some(&resolved.record), field, error)?);
                return Ok(Link::none_with("unresolved", diagnostics));
            }
            if actual_type != b"Collection" {
                let error = Error::new(
                    "type",
                    format!(
                        "linked collection placeholder resolved to {}, not Collection",
                        String::from_utf8_lossy(&actual_type)
                    ),
                );
                diagnostics.push(self.note(target_source, Some(&resolved.record), field, error)?);
                return Ok(Link::none_with("unresolved", diagnostics));
            }
            let key =
                self.ensure_collection_record(resolved.reader, &resolved.record, target_source)?;
            return self.schedule_collection(
                Link {
                    key: Some(key),
                    source: Some(target_source),
                    status: "library",
                    provenance,
                    diagnostics,
                },
                traversal,
                field,
            );
        }

        if saved_type != b"Collection" {
            let error = Error::new(
                "type",
                format!(
                    "collection pointer targets {}, not Collection or ID placeholder",
                    String::from_utf8_lossy(&saved_type)
                ),
            );
            diagnostics.push(self.note(source, Some(target), field, error)?);
            return Ok(Link::none_with("unresolved", diagnostics));
        }
        let key = self.ensure_collection_record(file, target, source)?;
        self.schedule_collection(
            Link {
                key: Some(key),
                source: Some(source),
                status: "local",
                provenance: Vec::new(),
                diagnostics,
            },
            traversal,
            field,
        )
    }

    fn schedule_collection(
        &mut self,
        mut link: Link,
        traversal: &mut Traversal<'_, '_>,
        field: &str,
    ) -> Result<Link> {
        let (Some(key), Some(source)) = (link.key.as_ref(), link.source) else {
            return Ok(link);
        };
        if traversal.active_collections.contains(key) {
            let record = self.node_info.get(key).map(|info| info.record.clone());
            link.diagnostics.push(self.note(
                source,
                record.as_ref(),
                field,
                Error::new(
                    "query_cycle",
                    "collection child edge revisits a collection active on this path",
                ),
            )?);
            link.status = "cycle";
        } else if !self.expanded_collections.contains(key) {
            traversal.stack.push(Frame::Enter(Pending {
                key: key.clone(),
                kind: NodeKind::Collection,
            }));
        }
        Ok(link)
    }

    fn visit_object_target<'f, 'r, 'l>(
        &mut self,
        file: &'f BlendFile,
        target: &Record,
        source: usize,
        links: &mut linked::Context<'_, 'l>,
        traversal: &mut Traversal<'r, 'l>,
        relation: ObjectRelation,
    ) -> Result<Link> {
        let field = match relation {
            ObjectRelation::Membership => "Collection.gobject",
            ObjectRelation::Parent => "Object.parent",
        };
        let mut diagnostics = Vec::new();
        let saved_type = match type_name(file, target) {
            Ok(kind) => kind.to_vec(),
            Err(error) => {
                diagnostics.push(self.note(source, Some(target), field, error)?);
                return Ok(Link::none_with("unavailable", diagnostics));
            }
        };
        if saved_type == b"ID" {
            let Some(stub) = self.stub_identity(
                file,
                target,
                source,
                field,
                Some(&b"OB"[..]),
                &mut diagnostics,
            )?
            else {
                return Ok(Link::none_with("unavailable", diagnostics));
            };
            let owner = self.sources[source].path.clone();
            let reason = match relation {
                ObjectRelation::Parent => "collection.graph object parent",
                ObjectRelation::Membership => "collection.graph linked object membership",
            };
            let request =
                self.request_json(source, target, field, &stub.name, &stub.library, reason);
            let resolved =
                match links.resolve_id(owner.as_deref(), &stub.library, &stub.name, reason) {
                    Ok(resolved) => resolved,
                    Err(error) => {
                        let status = if error.kind == "query_cycle" {
                            "cycle"
                        } else {
                            "unavailable"
                        };
                        diagnostics.push(self.note(source, Some(target), field, error)?);
                        return Ok(Link::none_with(status, diagnostics));
                    }
                };
            let path = crate::trace::lexical(&resolved.path);
            let provenance = resolved.provenance.clone();
            let target_source = self.source_for_library(&path, &provenance, request);
            traversal
                .readers
                .entry(target_source)
                .or_insert(Reader::Library(resolved.reader));
            let actual_type = match type_name(resolved.reader, &resolved.record) {
                Ok(kind) => kind.to_vec(),
                Err(error) => {
                    diagnostics.push(self.note(
                        target_source,
                        Some(&resolved.record),
                        field,
                        error,
                    )?);
                    return Ok(Link::none_with("unavailable", diagnostics));
                }
            };
            if actual_type == b"ID" {
                let error = Error::new(
                    "missing_id",
                    "linked object placeholder did not resolve to local saved content",
                );
                diagnostics.push(self.note(target_source, Some(&resolved.record), field, error)?);
                return Ok(Link::none_with("unresolved", diagnostics));
            }
            if actual_type != b"Object" {
                let error = Error::new(
                    "type",
                    format!(
                        "linked object placeholder resolved to {}, not Object",
                        String::from_utf8_lossy(&actual_type)
                    ),
                );
                diagnostics.push(self.note(target_source, Some(&resolved.record), field, error)?);
                return Ok(Link::none_with("unresolved", diagnostics));
            }
            let key =
                self.ensure_object_record(resolved.reader, &resolved.record, target_source)?;
            return self.schedule_object(
                Link {
                    key: Some(key),
                    source: Some(target_source),
                    status: "library",
                    provenance,
                    diagnostics,
                },
                traversal,
                relation,
                field,
            );
        }

        if saved_type != b"Object" {
            let error = Error::new(
                "type",
                format!(
                    "object pointer targets {}, not Object or ID placeholder",
                    String::from_utf8_lossy(&saved_type)
                ),
            );
            diagnostics.push(self.note(source, Some(target), field, error)?);
            return Ok(Link::none_with("unresolved", diagnostics));
        }
        let key = self.ensure_object_record(file, target, source)?;
        self.schedule_object(
            Link {
                key: Some(key),
                source: Some(source),
                status: "local",
                provenance: Vec::new(),
                diagnostics,
            },
            traversal,
            relation,
            field,
        )
    }

    fn schedule_object(
        &mut self,
        mut link: Link,
        traversal: &mut Traversal<'_, '_>,
        relation: ObjectRelation,
        field: &str,
    ) -> Result<Link> {
        let (Some(key), Some(source)) = (link.key.as_ref(), link.source) else {
            return Ok(link);
        };
        if relation == ObjectRelation::Parent && traversal.active_objects.contains(key) {
            let record = self.node_info.get(key).map(|info| info.record.clone());
            link.diagnostics.push(self.note(
                source,
                record.as_ref(),
                field,
                Error::new(
                    "query_cycle",
                    "object parent chain revisits an object active on this path",
                ),
            )?);
            link.status = "cycle";
        } else if !(self.expanded_objects.contains(key)
            || relation == ObjectRelation::Membership && traversal.active_objects.contains(key))
        {
            traversal.stack.push(Frame::Enter(Pending {
                key: key.clone(),
                kind: NodeKind::Object,
            }));
        }
        Ok(link)
    }

    fn expand_all<'r, 'l>(
        &mut self,
        links: &mut linked::Context<'_, 'l>,
        traversal: &mut Traversal<'r, 'l>,
    ) -> Result<()> {
        while let Some(frame) = traversal.stack.pop() {
            match frame {
                Frame::Exit { key, kind } => match kind {
                    NodeKind::Collection => {
                        traversal.active_collections.remove(&key);
                    }
                    NodeKind::Object => {
                        traversal.active_objects.remove(&key);
                    }
                },
                Frame::Enter(pending) => {
                    if self.is_expanded(pending.kind, &pending.key) {
                        continue;
                    }
                    let Some(info) = self.node_info.get(&pending.key).cloned() else {
                        continue;
                    };
                    let Some(reader) = traversal.readers.get(&info.source).copied() else {
                        let id = self.note(
                            info.source,
                            Some(&info.record),
                            "graph.reader",
                            Error::new(
                                "library_read",
                                "scheduled collection-graph node has no retained reader",
                            ),
                        )?;
                        self.attach_record(&pending.key, vec![id]);
                        self.mark_expanded(pending.kind, &pending.key);
                        continue;
                    };
                    self.mark_expanded(pending.kind, &pending.key);
                    match pending.kind {
                        NodeKind::Collection => {
                            traversal.active_collections.insert(pending.key.clone());
                        }
                        NodeKind::Object => {
                            traversal.active_objects.insert(pending.key.clone());
                        }
                    }
                    traversal.stack.push(Frame::Exit {
                        key: pending.key.clone(),
                        kind: pending.kind,
                    });
                    let file = match reader {
                        Reader::Root(file) => file,
                        Reader::Library(file) => file,
                    };
                    match pending.kind {
                        NodeKind::Collection => self.expand_collection_direct(
                            file,
                            &info.record,
                            info.source,
                            &pending.key,
                            links,
                            traversal,
                        )?,
                        NodeKind::Object => self.expand_object_direct(
                            file,
                            &info.record,
                            info.source,
                            &pending.key,
                            links,
                            traversal,
                        )?,
                    }
                }
            }
        }
        Ok(())
    }

    fn expand_collection_direct<'f, 'r, 'l>(
        &mut self,
        file: &'f BlendFile,
        collection: &Record,
        source: usize,
        key: &str,
        links: &mut linked::Context<'_, 'l>,
        traversal: &mut Traversal<'r, 'l>,
    ) -> Result<()> {
        let mut node_diagnostics = Vec::new();
        let (children, children_error) = checked_list(file, collection, "children");
        if let Some(error) = children_error {
            node_diagnostics.push(self.note(
                source,
                Some(collection),
                "Collection.children",
                error,
            )?);
        }
        for (ordinal, entry) in children.iter().enumerate() {
            let mut edge_diagnostics = Vec::new();
            let target = match access::pointer(file, entry, "collection") {
                Ok(target) => target,
                Err(error) => {
                    edge_diagnostics.push(self.note(
                        source,
                        Some(entry),
                        "Collection.children.collection",
                        error,
                    )?);
                    None
                }
            };
            let reference = match target.as_ref() {
                Some(target) => match typed_ref(file, target, "Collection") {
                    Ok(reference) => Some(reference),
                    Err(error) => {
                        edge_diagnostics.push(self.note(
                            source,
                            Some(target),
                            "Collection.children.collection.reference",
                            error,
                        )?);
                        None
                    }
                },
                None => None,
            };
            let link = match target {
                Some(target) => {
                    let link = self.visit_collection_target(
                        file,
                        &target,
                        source,
                        links,
                        traversal,
                        "Collection.children",
                    )?;
                    edge_diagnostics.extend(link.diagnostics.iter().copied());
                    link
                }
                None => {
                    if reference.is_none() {
                        let error = Error::new(
                            "invalid_saved_structure",
                            "collection child entry has no target pointer",
                        );
                        edge_diagnostics.push(self.note(
                            source,
                            Some(entry),
                            "Collection.children.collection",
                            error,
                        )?);
                    }
                    Link::none("missing_pointer")
                }
            };
            let Link {
                key: to,
                source: target_source,
                status,
                provenance,
                ..
            } = link;
            let target_source = target_source.map(|index| self.sources[index].key.clone());
            let edge = json!({
                "from": key,
                "kind": "collection_child",
                "ordinal": ordinal,
                "saved_field": "Collection.children",
                "to": to,
                "target_source": target_source,
                "target_reference": reference,
                "resolution": status,
                "provenance": provenance,
            });
            let index = self.add_edge(key, "collection_child", ordinal as u64, edge);
            self.attach_edge(index, edge_diagnostics);
        }

        let (objects, objects_error) = checked_list(file, collection, "gobject");
        if let Some(error) = objects_error {
            node_diagnostics.push(self.note(
                source,
                Some(collection),
                "Collection.gobject",
                error,
            )?);
        }
        for (ordinal, entry) in objects.iter().enumerate() {
            let mut edge_diagnostics = Vec::new();
            let target = match access::pointer(file, entry, "ob") {
                Ok(target) => target,
                Err(error) => {
                    edge_diagnostics.push(self.note(
                        source,
                        Some(entry),
                        "Collection.gobject.ob",
                        error,
                    )?);
                    None
                }
            };
            let reference = match target.as_ref() {
                Some(target) => match typed_ref(file, target, "Object") {
                    Ok(reference) => Some(reference),
                    Err(error) => {
                        edge_diagnostics.push(self.note(
                            source,
                            Some(target),
                            "Collection.gobject.ob.reference",
                            error,
                        )?);
                        None
                    }
                },
                None => None,
            };
            let link = match target {
                Some(target) => {
                    let link = self.visit_object_target(
                        file,
                        &target,
                        source,
                        links,
                        traversal,
                        ObjectRelation::Membership,
                    )?;
                    edge_diagnostics.extend(link.diagnostics.iter().copied());
                    link
                }
                None => {
                    if reference.is_none() {
                        let error = Error::new(
                            "invalid_saved_structure",
                            "collection object entry has no target pointer",
                        );
                        edge_diagnostics.push(self.note(
                            source,
                            Some(entry),
                            "Collection.gobject.ob",
                            error,
                        )?);
                    }
                    Link::none("missing_pointer")
                }
            };
            let Link {
                key: to,
                source: target_source,
                status,
                provenance,
                ..
            } = link;
            let target_source = target_source.map(|index| self.sources[index].key.clone());
            let edge = json!({
                "from": key,
                "kind": "collection_object",
                "ordinal": ordinal,
                "saved_field": "Collection.gobject",
                "to": to,
                "target_source": target_source,
                "target_reference": reference,
                "resolution": status,
                "provenance": provenance,
            });
            let index = self.add_edge(key, "collection_object", ordinal as u64, edge);
            self.attach_edge(index, edge_diagnostics);
        }
        self.attach_record(key, node_diagnostics);
        Ok(())
    }

    fn expand_object_direct<'f, 'r, 'l>(
        &mut self,
        file: &'f BlendFile,
        object: &Record,
        source: usize,
        key: &str,
        links: &mut linked::Context<'_, 'l>,
        traversal: &mut Traversal<'r, 'l>,
    ) -> Result<()> {
        let mut node_diagnostics = Vec::new();
        match access::pointer(file, object, "parent") {
            Ok(Some(parent)) => {
                let reference = match typed_ref(file, &parent, "Object") {
                    Ok(reference) => Some(reference),
                    Err(error) => {
                        node_diagnostics.push(self.note(
                            source,
                            Some(&parent),
                            "Object.parent.reference",
                            error,
                        )?);
                        None
                    }
                };
                let link = self.visit_object_target(
                    file,
                    &parent,
                    source,
                    links,
                    traversal,
                    ObjectRelation::Parent,
                )?;
                node_diagnostics.extend(link.diagnostics.iter().copied());
                let Link {
                    key: parent_key,
                    source: parent_source,
                    status,
                    provenance,
                    ..
                } = link;
                let parent_source = parent_source.map(|index| self.sources[index].key.clone());
                self.set_data(key, "parent", json!(parent_key));
                self.set_data(key, "parent_status", json!(status));
                self.set_data(key, "parent_source", json!(parent_source));
                self.set_data(key, "parent_reference", json!(reference));
                self.set_data(key, "parent_provenance", json!(provenance));
            }
            Ok(None) => {
                self.set_data(key, "parent", Json::Null);
                self.set_data(key, "parent_status", json!("none"));
                self.set_data(key, "parent_source", Json::Null);
                self.set_data(key, "parent_reference", Json::Null);
                self.set_data(key, "parent_provenance", json!([]));
            }
            Err(error) => {
                node_diagnostics.push(self.note(source, Some(object), "Object.parent", error)?);
                self.set_data(key, "parent", Json::Null);
                self.set_data(key, "parent_status", json!("unavailable"));
                self.set_data(key, "parent_source", Json::Null);
                self.set_data(key, "parent_reference", Json::Null);
                self.set_data(key, "parent_provenance", json!([]));
            }
        }
        self.attach_record(key, node_diagnostics);
        Ok(())
    }

    fn reachable_diagnostic_ids(&self, root: &str) -> Vec<usize> {
        let mut records = HashSet::new();
        let mut edges = HashSet::new();
        let mut ids = Vec::new();
        let mut stack = vec![root.to_owned()];
        while let Some(key) = stack.pop() {
            if !records.insert(key.clone()) {
                continue;
            }
            if let Some(direct) = self.record_diagnostics.get(&key) {
                ids.extend(direct.iter().copied());
            }
            if let Some(&index) = self.record_index.get(&key)
                && let Some(parent) = self.records[index]
                    .get("data")
                    .and_then(|data| data.get("parent"))
                    .and_then(Json::as_str)
            {
                stack.push(parent.to_owned());
            }
            if let Some(outgoing) = self.edges_by_from.get(&key) {
                for &edge in outgoing {
                    if !edges.insert(edge) {
                        continue;
                    }
                    if let Some(direct) = self.edge_diagnostics.get(&edge) {
                        ids.extend(direct.iter().copied());
                    }
                    if let Some(to) = self.edges[edge]["to"].as_str() {
                        stack.push(to.to_owned());
                    }
                }
            }
        }
        ids
    }

    fn errors_for_ids(&self, ids: &[usize]) -> Vec<Error> {
        let mut seen_ids = HashSet::new();
        let mut seen_errors = HashSet::new();
        let mut errors = Vec::new();
        for &id in ids {
            if !seen_ids.insert(id) {
                continue;
            }
            let Some(error) = self.diagnostic_errors.get(id) else {
                continue;
            };
            if seen_errors.insert((error.kind, error.message.clone())) {
                errors.push(error.clone());
            }
        }
        errors
    }

    fn stub_identity(
        &mut self,
        file: &BlendFile,
        record: &Record,
        source: usize,
        field: &str,
        expected_code: Option<&[u8]>,
        diagnostics: &mut Vec<usize>,
    ) -> Result<Option<Stub>> {
        let name = match id_name_bytes(file, record) {
            Ok(name) => name,
            Err(error) => {
                diagnostics.push(self.note(source, Some(record), field, error)?);
                return Ok(None);
            }
        };
        if name.len() < 2 {
            let error = Error::new("type", "linked ID placeholder name lacks a two-byte code");
            diagnostics.push(self.note(source, Some(record), field, error)?);
            return Ok(None);
        }
        if let Some(code) = expected_code
            && !name.starts_with(code)
        {
            let error = Error::new(
                "type",
                format!(
                    "linked ID placeholder has unexpected saved code {:?}",
                    String::from_utf8_lossy(&name[..2])
                ),
            );
            diagnostics.push(self.note(source, Some(record), field, error)?);
            return Ok(None);
        }
        let library = match linked::library_bytes(file, record) {
            Ok(Some(library)) => library,
            Ok(None) => {
                let error = Error::new(
                    "missing_library",
                    "linked ID placeholder has no saved Library pointer",
                );
                diagnostics.push(self.note(source, Some(record), field, error)?);
                return Ok(None);
            }
            Err(error) => {
                diagnostics.push(self.note(source, Some(record), field, error)?);
                return Ok(None);
            }
        };
        Ok(Some(Stub { name, library }))
    }

    fn request_json(
        &self,
        source: usize,
        record: &Record,
        field: &str,
        name: &[u8],
        library: &[u8],
        reason: &str,
    ) -> Json {
        json!({
            "from": self.sources[source].key,
            "owner_local_ref": {"block_index":record.block,"element":record.element},
            "field": field,
            "reason": reason,
            "library": std::str::from_utf8(library).ok(),
            "library_hex": hex(library),
            "id_name": std::str::from_utf8(name).ok(),
            "id_name_hex": hex(name),
        })
    }

    fn ensure_collection_record(
        &mut self,
        file: &BlendFile,
        record: &Record,
        source: usize,
    ) -> Result<String> {
        let key = record_key(&self.sources[source], record);
        if self.record_index.contains_key(&key) {
            return Ok(key);
        }
        let mut record_errors = Vec::new();
        let saved_type = match type_name(file, record) {
            Ok(name) => std::str::from_utf8(name).ok().map(str::to_owned),
            Err(error) => {
                record_errors.push(self.note(
                    source,
                    Some(record),
                    "Collection.saved_type",
                    error,
                )?);
                None
            }
        };
        let reference = match typed_ref(file, record, "Collection") {
            Ok(reference) => Some(reference),
            Err(error) => {
                record_errors.push(self.note(source, Some(record), "Collection.id", error)?);
                None
            }
        };
        let flag = self.read_int(
            file,
            record,
            source,
            "Collection.flag",
            "flag",
            &mut record_errors,
        )?;
        let owner =
            self.optional_reference(file, record, source, "owner_id", true, &mut record_errors)?;
        let override_info = self.override_info(file, record, source, &mut record_errors)?;
        let library_owned = reference
            .as_ref()
            .and_then(|reference| reference.get("library"))
            .map(|library| !library.is_null());
        let ownership = ownership_for(&override_info, library_owned);
        let mut data = json!({"flag": flag, "override": override_info});
        if let Some(owner) = owner {
            data["owner"] = owner;
        }
        let row = json!({
            "key": key,
            "source": self.sources[source].key,
            "source_local_ref": {"block_index":record.block,"element":record.element},
            "saved_type": saved_type,
            "kind": "Collection",
            "id": reference,
            "content_status": "local",
            "ownership": ownership,
            "ownership_scope": OWNERSHIP_SCOPE,
            "library_owned": library_owned,
            "data": data,
        });
        self.insert_record(key.clone(), row);
        self.node_info.insert(
            key.clone(),
            NodeInfo {
                source,
                record: record.clone(),
            },
        );
        self.attach_record(&key, record_errors);
        Ok(key)
    }

    fn ensure_object_record(
        &mut self,
        file: &BlendFile,
        record: &Record,
        source: usize,
    ) -> Result<String> {
        let key = record_key(&self.sources[source], record);
        if self.record_index.contains_key(&key) {
            return Ok(key);
        }
        let mut record_errors = Vec::new();
        let saved_type = match type_name(file, record) {
            Ok(name) => std::str::from_utf8(name).ok().map(str::to_owned),
            Err(error) => {
                record_errors.push(self.note(source, Some(record), "Object.saved_type", error)?);
                None
            }
        };
        let reference = match typed_ref(file, record, "Object") {
            Ok(reference) => Some(reference),
            Err(error) => {
                record_errors.push(self.note(source, Some(record), "Object.id", error)?);
                None
            }
        };
        let object_type = self.read_int(
            file,
            record,
            source,
            "Object.type",
            "type",
            &mut record_errors,
        )?;
        let partype = self.read_int(
            file,
            record,
            source,
            "Object.partype",
            "partype",
            &mut record_errors,
        )?;
        let restrictflag = self.read_int(
            file,
            record,
            source,
            "Object.restrictflag",
            "restrictflag",
            &mut record_errors,
        )?;
        let override_info = self.override_info(file, record, source, &mut record_errors)?;
        let library_owned = reference
            .as_ref()
            .and_then(|reference| reference.get("library"))
            .map(|library| !library.is_null());
        let ownership = ownership_for(&override_info, library_owned);
        let row = json!({
            "key": key,
            "source": self.sources[source].key,
            "source_local_ref": {"block_index":record.block,"element":record.element},
            "saved_type": saved_type,
            "kind": "Object",
            "id": reference,
            "content_status": "local",
            "ownership": ownership,
            "ownership_scope": OWNERSHIP_SCOPE,
            "library_owned": library_owned,
            "data": {
                "type": object_type,
                "partype": partype,
                "restrictflag": restrictflag,
                "parent": Json::Null,
                "parent_status": "unvisited",
                "parent_source": Json::Null,
                "parent_reference": Json::Null,
                "parent_provenance": [],
                "override": override_info,
            },
        });
        self.insert_record(key.clone(), row);
        self.node_info.insert(
            key.clone(),
            NodeInfo {
                source,
                record: record.clone(),
            },
        );
        self.attach_record(&key, record_errors);
        Ok(key)
    }

    fn read_int(
        &mut self,
        file: &BlendFile,
        record: &Record,
        source: usize,
        field: &str,
        path: &str,
        errors: &mut Vec<usize>,
    ) -> Result<Json> {
        match access::int(file, record, path) {
            Ok(value) => Ok(json!(value)),
            Err(error) => {
                let id = self.note(source, Some(record), field, error)?;
                errors.push(id);
                Ok(json!({"status":"unavailable","diagnostic":id}))
            }
        }
    }

    /// Optional pointer. A missing field returns None so the caller can omit it
    /// when the saved layout does not define it; a present null is Some(null).
    fn optional_reference(
        &mut self,
        file: &BlendFile,
        record: &Record,
        source: usize,
        path: &str,
        optional_in_layout: bool,
        errors: &mut Vec<usize>,
    ) -> Result<Option<Json>> {
        let field = format!(
            "{}.{path}",
            String::from_utf8_lossy(type_name(file, record)?)
        );
        let field = field.as_str();
        match access::pointer(file, record, path) {
            Ok(Some(target)) => match access::id_ref(file, &target) {
                Ok(mut reference) => {
                    if let Err(error) = reference_name_hex(file, &target, &mut reference) {
                        errors.push(self.note(source, Some(record), field, error)?);
                    }
                    Ok(Some(reference))
                }
                Err(error) => {
                    let id = self.note(source, Some(record), field, error)?;
                    errors.push(id);
                    Ok(Some(json!({"status":"unavailable","diagnostic":id})))
                }
            },
            Ok(None) => Ok(Some(Json::Null)),
            Err(error) if optional_in_layout && error.kind == "missing_field" => Ok(None),
            Err(error) => {
                let id = self.note(source, Some(record), field, error)?;
                errors.push(id);
                Ok(Some(json!({"status":"unavailable","diagnostic":id})))
            }
        }
    }

    fn override_info(
        &mut self,
        file: &BlendFile,
        record: &Record,
        source: usize,
        errors: &mut Vec<usize>,
    ) -> Result<Json> {
        let prefix = match id_prefix(file, record) {
            Ok(prefix) => prefix,
            Err(error) => {
                errors.push(self.note(source, Some(record), "ID.override_library", error)?);
                return Ok(json!({"present":null,"status":"unavailable"}));
            }
        };
        let field = format!("{prefix}override_library");
        let target = match access::pointer(file, record, &field) {
            Ok(target) => target,
            Err(error) if error.kind == "missing_field" => {
                return Ok(json!({"present":null,"status":"not_saved_in_layout"}));
            }
            Err(error) => {
                errors.push(self.note(source, Some(record), "ID.override_library", error)?);
                return Ok(json!({"present":null,"status":"unavailable"}));
            }
        };
        let Some(target) = target else {
            return Ok(json!({"present":false}));
        };
        let saved_type = type_name(file, &target)
            .map(|name| std::str::from_utf8(name).ok().map(str::to_owned))
            .unwrap_or(None);
        match type_name(file, &target) {
            Ok(b"IDOverrideLibrary") => {}
            Ok(_) => {
                let error = Error::new(
                    "type",
                    "ID.override_library does not target IDOverrideLibrary",
                );
                errors.push(self.note(source, Some(&target), "ID.override_library", error)?);
                return Ok(json!({
                    "present": true,
                    "saved_type": saved_type,
                    "status": "unexpected_type",
                }));
            }
            Err(error) => {
                errors.push(self.note(source, Some(&target), "ID.override_library", error)?);
                return Ok(json!({
                    "present": true,
                    "saved_type": saved_type,
                    "status": "unavailable",
                }));
            }
        }
        let flag = self.read_int(
            file,
            &target,
            source,
            "IDOverrideLibrary.flag",
            "flag",
            errors,
        )?;
        let reference =
            self.optional_reference(file, &target, source, "reference", false, errors)?;
        let hierarchy_root =
            self.optional_reference(file, &target, source, "hierarchy_root", true, errors)?;
        let mut info = json!({
            "present": true,
            "saved_type": saved_type,
            "flag": flag,
            "reference": reference,
        });
        if let Some(hierarchy_root) = hierarchy_root {
            info["hierarchy_root"] = hierarchy_root;
        }
        Ok(info)
    }
}

fn ownership_for(override_info: &Json, library_owned: Option<bool>) -> &'static str {
    let present = override_info.get("present").and_then(Json::as_bool);
    let status = override_info.get("status");
    match (present, status) {
        (Some(true), None) => "override",
        (Some(false), None) => match library_owned {
            Some(true) => "linked",
            Some(false) => "local",
            None => "unknown",
        },
        _ => "unknown",
    }
}

fn record_key(source: &Source, record: &Record) -> String {
    format!("{}:{}:{}", source.key, record.block, record.element)
}

fn type_name<'a>(file: &'a BlendFile, record: &Record) -> Result<&'a [u8]> {
    let block = file.block(record.block)?;
    let structure = file
        .schema()
        .structs
        .get(block.sdna_index)
        .ok_or_else(|| Error::new("invalid_sdna", "saved block has no structure entry"))?;
    file.schema()
        .types
        .get(structure.type_index)
        .map(Vec::as_slice)
        .ok_or_else(|| Error::new("invalid_sdna", "saved structure has no type name"))
}

fn id_prefix(file: &BlendFile, record: &Record) -> Result<&'static str> {
    match access::get(file, record, "id.name") {
        Ok(_) => Ok("id."),
        Err(error) if error.kind == "missing_field" => Ok(""),
        Err(error) => Err(error),
    }
}

fn id_name_bytes(file: &BlendFile, record: &Record) -> Result<Vec<u8>> {
    let path = if id_prefix(file, record)?.is_empty() {
        "name"
    } else {
        "id.name"
    };
    match access::get(file, record, path)? {
        Value::Bytes(name) => Ok(name),
        _ => Err(Error::new("type", "ID name must be bytes")),
    }
}

/// Context-typed ID reference. Collection master records can carry the saved
/// Scene naming prefix, so the walked context, not the name prefix, supplies
/// the kind. The full saved name is retained as hex even when its UTF-8 tail is
/// not representable; the graph-only body has no storage inventory to recover it.
fn typed_ref(file: &BlendFile, record: &Record, kind: &str) -> Result<Json> {
    let mut reference = access::id_ref(file, record)?;
    reference_name_hex(file, record, &mut reference)?;
    let saved_kind = reference["kind"].clone();
    reference["saved_kind"] = saved_kind;
    reference["kind"] = json!(kind);
    Ok(reference)
}

/// Add the full saved ID name bytes to any ID reference. Python-style `name`
/// drops invalid UTF-8; the graph-only body has no storage inventory to recover
/// those bytes, so every owner/override/parent/target reference keeps them.
fn reference_name_hex(file: &BlendFile, record: &Record, reference: &mut Json) -> Result<()> {
    let full_name = id_name_bytes(file, record)?;
    reference["name_hex"] = json!(hex(&full_name));
    Ok(())
}

#[cfg(test)]
#[path = "collection_graph_tests.rs"]
mod tests;
