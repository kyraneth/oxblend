//! Meaningful module tests. Native nested-fixture tests are selected with
//! `OXBLEND_COLLECTION_GRAPH_FIXTURE`; they never run Blender themselves.

use super::*;
use crate::query::{self, LibraryMap, LibraryView};
use crate::value::hex;
use crate::{BlendFile, Error};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};

fn fixture() -> Option<PathBuf> {
    let path = std::env::var_os("OXBLEND_COLLECTION_GRAPH_FIXTURE").map(PathBuf::from);
    if path.is_none() {
        eprintln!(
            "collection-graph fixture not selected; set OXBLEND_COLLECTION_GRAPH_FIXTURE after running its builder"
        );
    }
    path
}

fn run_query_type(source: &Path, type_name: &str) -> Json {
    let root = BlendFile::open(source).unwrap();
    let fields = ["collection.graph".to_string()];
    let mut owned: BTreeMap<PathBuf, BlendFile> = BTreeMap::new();
    loop {
        let views: LibraryMap<'_> = owned
            .iter()
            .map(|(path, reader)| {
                (
                    path.clone(),
                    Ok(LibraryView {
                        reader,
                        provenance: json!({"owner":"collection-graph-test-store"}),
                    }),
                )
            })
            .collect();
        let requests =
            query::library_requests(&root, Some(source), type_name, &fields, &views).unwrap();
        if requests.is_empty() {
            let report =
                query::query_with_libraries(&root, Some(source), type_name, &fields, &views)
                    .unwrap();
            return report;
        }
        drop(views);
        for request in requests {
            if let std::collections::btree_map::Entry::Vacant(entry) = owned.entry(request.key) {
                entry.insert(BlendFile::open(request.path).unwrap());
            }
        }
    }
}

// Mutate a NEW in-memory snapshot through proved SDNA offsets, never the fixture.
fn changed_pointer(file: &BlendFile, record: &Record, path: &str, address: u64) -> BlendFile {
    let field = file
        .locate_field(record.block, record.element, &record.at(path).prefix)
        .unwrap();
    assert!(field.field.pointer);
    assert_eq!(field.field.size, file.header().pointer_size);
    let start = file.block(record.block).unwrap().data_offset + field.offset;
    let mut bytes = file.read_decompressed_bytes().unwrap();
    let width = file.header().pointer_size;
    let encoded = if file.header().little_endian {
        address.to_le_bytes()
    } else {
        address.to_be_bytes()
    };
    let encoded = if file.header().little_endian {
        &encoded[..width]
    } else {
        &encoded[8 - width..]
    };
    bytes[start..start + width].copy_from_slice(encoded);
    BlendFile::from_bytes(bytes).unwrap()
}

#[test]
fn saved_object_parent_cycle_is_diagnosed_without_repairing_membership() {
    let Some(source) = fixture() else { return };
    let file = BlendFile::open(&source).unwrap();
    let object = access::records(&file, "Object")
        .unwrap()
        .into_iter()
        .find(|r| access::text(&file, r, "id.name").unwrap() == "OBRootLocalObject")
        .unwrap();
    let changed = changed_pointer(
        &file,
        &object,
        "parent",
        file.block(object.block).unwrap().old_address,
    );
    let report =
        query::query_with_source(&changed, &source, "Scene", &["collection.graph".into()]).unwrap();
    let graph = graph(&report);
    assert_eq!(report["requested_coverage"]["complete"], false);
    assert!(
        graph["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["kind"] == "query_cycle" && d["field"] == "Object.parent")
    );
    let object = record_named(graph, "Object", "RootLocalObject");
    assert_eq!(object["data"]["parent"], object["key"]);
    assert_eq!(object["data"]["parent_status"], "cycle");
    record_named(graph, "Object", "NestedLocalObject");
    assert_eq!(
        edges(graph).len(),
        6,
        "independent membership edges survive"
    );
}
#[test]
fn saved_dangling_parent_retains_independent_nested_content() {
    let Some(source) = fixture() else { return };
    let file = BlendFile::open(&source).unwrap();
    let object = access::records(&file, "Object")
        .unwrap()
        .into_iter()
        .find(|r| access::text(&file, r, "id.name").unwrap() == "OBRootLocalObject")
        .unwrap();
    let changed = changed_pointer(&file, &object, "parent", u64::MAX);
    let report =
        query::query_with_source(&changed, &source, "Scene", &["collection.graph".into()]).unwrap();
    let graph = graph(&report);
    assert_eq!(report["requested_coverage"]["complete"], false);
    assert!(
        graph["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["field"] == "Object.parent" && d["kind"].is_string())
    );
    assert_eq!(
        record_named(graph, "Object", "RootLocalObject")["data"]["parent_status"],
        "unavailable"
    );
    record_named(graph, "Object", "NestedLocalObject");
    assert_eq!(edges(graph).len(), 6);
}
#[test]
fn saved_collection_child_cycle_is_not_a_shared_dag() {
    let Some(source) = gap_fixture() else { return };
    let file = BlendFile::open(&source).unwrap();
    let collection = access::records(&file, "Collection")
        .unwrap()
        .into_iter()
        .find(|r| access::text(&file, r, "id.name").unwrap() == "GRRootGapCollection")
        .unwrap();
    let child = access::pointer(&file, &collection, "children.first")
        .unwrap()
        .unwrap();
    let changed = changed_pointer(
        &file,
        &child,
        "collection",
        file.block(collection.block).unwrap().old_address,
    );
    let report = query::query_with_source(
        &changed,
        &source,
        "Collection",
        &["collection.graph".into()],
    )
    .unwrap();
    let graph = graph(&report);
    let collection = record_named(graph, "Collection", "RootGapCollection");
    let edge = edge(graph, key(collection), "collection_child", key(collection));
    assert_eq!(edge["resolution"], "cycle");
    assert_eq!(report["requested_coverage"]["complete"], false);
    assert!(
        graph["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["kind"] == "query_cycle" && d["field"] == "Collection.children")
    );
}
#[test]
fn unavailable_scalar_is_not_saved_null_and_source_change_aborts() {
    let Some(source) = fixture() else { return };
    let file = BlendFile::open(&source).unwrap();
    let record = access::records(&file, "Object").unwrap().remove(0);
    let mut graph = Graph::new();
    let source = graph.ensure_root(Some(&source)).unwrap();
    let mut errors = Vec::new();
    let value = graph
        .read_int(
            &file,
            &record,
            source,
            "Object.nonexistent_fixture_field",
            "nonexistent_fixture_field",
            &mut errors,
        )
        .unwrap();
    assert_eq!(value["status"], "unavailable");
    assert!(value["diagnostic"].is_number());
    assert!(!value.is_null());
    assert_eq!(errors.len(), 1);
    for kind in ["input_changed", "source_changed"] {
        assert_eq!(
            graph
                .note(
                    source,
                    Some(&record),
                    "Object.parent",
                    Error::new(kind, "fixture changed")
                )
                .unwrap_err()
                .kind,
            kind
        );
    }
    assert_eq!(
        graph.diagnostics.len(),
        1,
        "source changes are batch refusals, not row gaps"
    );
}

fn run_query(source: &Path) -> Json {
    run_query_type(source, "Scene")
}

fn gap_fixture() -> Option<PathBuf> {
    let path = std::env::var_os("OXBLEND_COLLECTION_GRAPH_GAP_FIXTURE").map(PathBuf::from);
    if path.is_none() {
        eprintln!(
            "collection-graph gap fixture not selected; set OXBLEND_COLLECTION_GRAPH_GAP_FIXTURE after running its builder"
        );
    }
    path
}

fn graph(report: &Json) -> &Json {
    &report["graphs"]["collection.graph"]
}

fn records(graph: &Json) -> &[Json] {
    graph["records"].as_array().unwrap()
}

fn edges(graph: &Json) -> &[Json] {
    graph["edges"].as_array().unwrap()
}

fn record_named<'a>(graph: &'a Json, kind: &str, name: &str) -> &'a Json {
    records(graph)
        .iter()
        .find(|record| record["kind"] == kind && record["id"]["name"] == name)
        .unwrap_or_else(|| panic!("missing {kind} {name} in {graph}"))
}

fn key(record: &Json) -> &str {
    record["key"].as_str().unwrap()
}

fn edge<'a>(graph: &'a Json, from: &str, kind: &str, to: &str) -> &'a Json {
    edges(graph)
        .iter()
        .find(|edge| {
            edge["from"] == from && edge["kind"] == kind && edge["to"].as_str() == Some(to)
        })
        .unwrap_or_else(|| panic!("missing {kind} edge {from} -> {to} in {graph}"))
}

fn full_name_hex(code: &str, name: &str) -> String {
    hex(format!("{code}{name}").as_bytes())
}

#[test]
fn nested_library_chain_keeps_ordered_edges_parent_chain_and_shared_records() {
    let Some(source) = fixture() else { return };
    let report = run_query(&source);
    assert_eq!(report["requested_coverage"]["complete"], true, "{report}");
    let graph = graph(&report);
    assert_eq!(graph["complete"], true, "{graph}");
    assert_eq!(records(graph).len(), 7, "{graph}");
    assert_eq!(edges(graph).len(), 6, "{graph}");
    let sources = graph["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 3, "{sources:?}");
    assert_eq!(sources[0]["role"], "root");
    assert!(
        sources[0]["lexical_path"]
            .as_str()
            .is_some_and(|path| Path::new(path).is_absolute()),
        "root source must be absolute lexical: {sources:?}"
    );
    assert!(sources.iter().any(|source| {
        source["lexical_path"]
            .as_str()
            .is_some_and(|path| path.ends_with("set-library.blend"))
    }));
    assert!(sources.iter().any(|source| {
        source["lexical_path"]
            .as_str()
            .is_some_and(|path| path.ends_with("nested-library.blend"))
    }));

    let set = record_named(graph, "Collection", "SetCollection");
    let nested = record_named(graph, "Collection", "NestedCollection");
    let set_object = record_named(graph, "Object", "SetLocalObject");
    let nested_object = record_named(graph, "Object", "NestedLocalObject");
    let root_object = record_named(graph, "Object", "RootLocalObject");
    let parent = record_named(graph, "Object", "RootParentObject");

    let scene_record = &report["rows"][0]["values"]["collection.graph"];
    assert_eq!(scene_record["status"], "available", "{scene_record}");
    let root_key = scene_record["value"]["root"].as_str().unwrap();
    let root_collection = records(graph)
        .iter()
        .find(|record| record["key"] == root_key)
        .unwrap();
    assert_eq!(root_collection["kind"], "Collection");

    edge(graph, root_key, "collection_child", key(set));
    edge(graph, key(set), "collection_child", key(nested));
    edge(graph, key(nested), "collection_object", key(nested_object));
    edge(graph, key(set), "collection_object", key(set_object));
    edge(graph, root_key, "collection_object", key(root_object));
    // The library-A object is linked into both the root scene and SetCollection:
    // both edges point at one deduplicated actual record.
    edge(graph, root_key, "collection_object", key(set_object));

    let parent_key = root_object["data"]["parent"].as_str().unwrap();
    assert_eq!(parent_key, key(parent));
    assert_eq!(root_object["data"]["parent_status"], json!("local"));
    assert!(
        !edges(graph)
            .iter()
            .any(|edge| edge["to"].as_str() == Some(parent_key)),
        "parent-only object must remain outside membership edges"
    );
    assert_eq!(parent["ownership"], "local");

    let mut seen = HashSet::new();
    for record in records(graph) {
        assert!(seen.insert(key(record)), "duplicate key {}", key(record));
        assert_eq!(record["ownership_scope"], json!("containing_saved_file"));
        assert!(
            record["data"].get("parent_type").is_none(),
            "parent_type is not a saved Object field in this profile: {record}"
        );
        assert!(
            record["data"].get("proxy").is_none(),
            "unasked legacy proxy field must not be synthesized: {record}"
        );
        assert!(
            record["data"].get("proxy_group").is_none(),
            "unasked legacy proxy_group field must not be synthesized: {record}"
        );
    }
    for record in [&set, &nested, &set_object, &nested_object, &parent] {
        assert_eq!(record["content_status"], "local");
    }
    assert_eq!(
        set["id"]["name_hex"],
        json!(full_name_hex("GR", "SetCollection"))
    );
    assert_eq!(
        set_object["id"]["name_hex"],
        json!(full_name_hex("OB", "SetLocalObject"))
    );
    assert_eq!(set["id"]["library"], Json::Null);
    assert_eq!(nested["id"]["library"], Json::Null);
    assert_eq!(set_object["ownership"], "local");

    for edge in edges(graph) {
        assert!(edge["ordinal"].as_u64().is_some());
        assert!(edge["resolution"].is_string());
        assert!(
            edge["target_reference"]["name_hex"]
                .as_str()
                .is_some_and(|name| !name.is_empty()),
            "every saved edge target keeps the full name bytes: {edge}"
        );
        if let Some(target) = edge["to"].as_str() {
            let target = records(graph)
                .iter()
                .find(|record| record["key"] == target)
                .unwrap();
            assert_eq!(
                edge["target_source"], target["source"],
                "edge target source must match the target record source"
            );
        }
    }
    let root_edges = edges(graph)
        .iter()
        .filter(|edge| edge["from"] == root_key)
        .collect::<Vec<_>>();
    assert!(root_edges.iter().any(|edge| edge["ordinal"] == 0));
    assert!(root_edges.iter().any(|edge| edge["ordinal"] == 1));
}

#[test]
fn missing_library_bytes_keep_known_edges_and_report_the_request_site() {
    let Some(source) = fixture() else { return };
    let root = BlendFile::open(&source).unwrap();
    let report = query::query_with_libraries(
        &root,
        Some(&source),
        "Scene",
        &["collection.graph".into()],
        &LibraryMap::new(),
    )
    .unwrap();
    assert_eq!(report["requested_coverage"]["complete"], false);
    let graph = graph(&report);
    assert_eq!(graph["complete"], false);
    assert!(!records(graph).is_empty());
    assert!(graph["diagnostics"].as_array().unwrap().iter().any(|row| {
        row["kind"] == "library_read"
            && row["field"] == "Collection.children"
            && row["record"]["block_index"].is_number()
    }));
    let row = &report["rows"][0]["values"]["collection.graph"];
    assert_eq!(row["status"], "unavailable", "{row}");
    assert_eq!(row["partial_value"]["graph_ref"], "collection.graph");
    let root_key = row["partial_value"]["root"].as_str().unwrap();
    let known = edges(graph).iter().any(|edge| {
        edge["from"] == root_key
            && edge["target_reference"]["name"] == "SetCollection"
            && edge["to"].is_null()
            && edge["resolution"] == "unavailable"
    });
    assert!(known, "{graph}");
}

#[test]
fn an_object_seed_is_rejected_without_fabricating_a_collection_root() {
    let Some(source) = fixture() else { return };
    let report = run_query_type(&source, "Object");
    let graph = graph(&report);
    assert_eq!(graph["complete"], false, "{graph}");
    assert!(records(graph).is_empty(), "{graph}");
    assert!(graph["diagnostics"].as_array().unwrap().iter().any(|row| {
        row["kind"] == "type"
            && row["field"] == "Seed"
            && row["message"]
                .as_str()
                .is_some_and(|message| message.contains("requires a Scene or Collection seed"))
    }));
    let row = &report["rows"][0]["values"]["collection.graph"];
    assert_eq!(row["status"], "unavailable", "{row}");
    assert!(row["partial_value"]["root"].is_null(), "{row}");
}

#[test]
fn cached_shared_subgraph_gap_reaches_every_affected_collection_row() {
    let Some(source) = gap_fixture() else { return };
    let root = BlendFile::open(&source).unwrap();
    let fields = ["collection.graph".to_string()];
    let requests = query::library_requests(
        &root,
        Some(&source),
        "Collection",
        &fields,
        &LibraryMap::new(),
    )
    .unwrap();
    assert!(
        !requests.is_empty(),
        "gap fixture must retain a library request"
    );
    let mut failed: LibraryMap<'_> = BTreeMap::new();
    for request in requests {
        failed.insert(
            request.key,
            Err(Error::new(
                "missing_library",
                "fixture library was deleted after the root was saved",
            )),
        );
    }
    let report =
        query::query_with_libraries(&root, Some(&source), "Collection", &fields, &failed).unwrap();
    assert_eq!(report["requested_coverage"]["complete"], false);
    let graph = graph(&report);
    assert_eq!(graph["complete"], false);
    assert!(graph["diagnostics"].as_array().unwrap().iter().any(|row| {
        row["kind"] == "missing_library"
            && row["field"] == "Collection.children"
            && row["record"]["block_index"].is_number()
    }));
    assert!(
        !graph["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["kind"] == "query_cycle"),
        "shared saved DAG is not a cycle"
    );
    let rows = report["rows"].as_array().unwrap();
    let mut affected = 0;
    for name in ["RootGapCollection", "SharedGapCollection"] {
        let row = rows
            .iter()
            .find(|row| row["id"]["name"] == name)
            .unwrap_or_else(|| panic!("missing row {name} in {report}"));
        let value = &row["values"]["collection.graph"];
        assert_eq!(value["status"], "unavailable", "{value}");
        assert_eq!(value["partial_value"]["graph_ref"], "collection.graph");
        assert!(
            value["partial_value"]["root"].is_string(),
            "cached row must still carry its root reference: {value}"
        );
        affected += 1;
    }
    assert_eq!(affected, 2, "{report}");
}

#[test]
fn library_path_and_id_names_are_read_from_saved_bytes_for_a_stub() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/library-null/normal.blend");
    let file = BlendFile::open(path).unwrap();
    let stub = access::records(&file, "ID")
        .unwrap()
        .into_iter()
        .find(|record| {
            linked::library_bytes(&file, record)
                .ok()
                .flatten()
                .is_some_and(|path| !path.is_empty())
        })
        .expect("fixture has a linked ID placeholder");
    let name = id_name_bytes(&file, &stub).unwrap();
    assert!(name.len() >= 2);
    let library = linked::library_bytes(&file, &stub).unwrap().unwrap();
    assert!(!library.is_empty());
    let reference = access::id_ref(&file, &stub).unwrap();
    assert_eq!(
        reference["library"],
        json!(std::str::from_utf8(&library).unwrap())
    );
}

#[test]
fn source_keys_deduplicate_lexical_variants_and_preserve_requests() {
    let mut graph = Graph::new();
    graph.ensure_root(None).unwrap();
    let first = graph.source_for_library(
        Path::new("/tmp/collection-graph/./nested-library.blend"),
        &[],
        json!({"request": 1}),
    );
    let same = graph.source_for_library(
        Path::new("/tmp/collection-graph/nested-library.blend"),
        &[],
        json!({"request": 2}),
    );
    let other = graph.source_for_library(
        Path::new("/tmp/collection-graph/../collection-graph/nested-library.blend"),
        &[],
        json!({"request": 3}),
    );
    assert_eq!(first, same);
    assert_eq!(first, other);
    assert_eq!(graph.sources.len(), 2);
    assert_eq!(graph.sources[first].requests.len(), 3);
}

#[test]
fn empty_graph_json_keeps_the_shared_contract() {
    let mut graph = Graph::new();
    graph
        .ensure_root(Some(Path::new("/tmp/collection-graph-root.blend")))
        .unwrap();
    let json = graph.json();
    assert_eq!(json["schema"], "oxblend.collection_graph/1");
    assert_eq!(json["graph_ref"], "collection.graph");
    assert_eq!(
        json["basis"],
        "saved_collection_edges_not_evaluated_visibility"
    );
    assert_eq!(json["complete"], true);
    assert_eq!(json["sources"][0]["role"], "root");
    assert_eq!(json["records"], json!([]));
    assert_eq!(json["edges"], json!([]));
    assert_eq!(json["diagnostics"], json!([]));
    assert_eq!(json["coverage"]["record_count"], 0);
    assert_eq!(json["coverage"]["edge_count"], 0);
    assert_eq!(json["coverage"]["unresolved_edge_count"], 0);
}

#[test]
fn root_source_is_absolute_lexical() {
    let mut graph = Graph::new();
    graph
        .ensure_root(Some(Path::new("relative-root.blend")))
        .unwrap();
    let path = graph.sources[0].path.as_ref().unwrap();
    assert!(path.is_absolute(), "{path:?}");
    assert!(path.ends_with("relative-root.blend"), "{path:?}");
}

#[test]
fn ownership_never_turns_an_unknown_override_or_library_state_into_local() {
    assert_eq!(
        ownership_for(&json!({"present":true}), Some(false)),
        "override"
    );
    assert_eq!(
        ownership_for(&json!({"present":false}), Some(true)),
        "linked"
    );
    assert_eq!(
        ownership_for(&json!({"present":false}), Some(false)),
        "local"
    );
    assert_eq!(
        ownership_for(
            &json!({"present":null,"status":"not_saved_in_layout"}),
            Some(false)
        ),
        "unknown"
    );
    assert_eq!(
        ownership_for(
            &json!({"present":true,"status":"unexpected_type"}),
            Some(false)
        ),
        "unknown"
    );
    assert_eq!(ownership_for(&json!({"present":false}), None), "unknown");
}

#[test]
fn deep_graph_reachability_is_iterative_and_keeps_direct_ownership() {
    let mut graph = Graph::new();
    graph.ensure_root(None).unwrap();
    let mut previous: Option<String> = None;
    for index in 0..20_000 {
        let key = format!("s0:1:{index}");
        graph.insert_record(key.clone(), json!({"key":key,"data":{"parent":Json::Null}}));
        if let Some(previous) = &previous {
            graph.add_edge(
                previous,
                "collection_child",
                0,
                json!({"from":previous,"kind":"collection_child","ordinal":0,"to":key}),
            );
        }
        previous = Some(key);
    }
    let tail = previous.unwrap();
    let diagnostic = graph
        .note(
            0,
            None,
            "Deep.chain",
            Error::new("missing_library", "deep shared child"),
        )
        .unwrap();
    graph.attach_record(&tail, vec![diagnostic]);
    let ids = graph.reachable_diagnostic_ids("s0:1:0");
    let errors = graph.errors_for_ids(&ids);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].kind, "missing_library");
    assert_eq!(graph.reachable_diagnostic_ids(&tail).len(), 1);
}

#[test]
fn shared_dag_reachability_does_not_duplicate_a_descendant_error() {
    let mut graph = Graph::new();
    graph.ensure_root(None).unwrap();
    let root = "s0:1:0";
    let left = "s0:1:1";
    let right = "s0:1:2";
    let shared = "s0:1:3";
    for key in [root, left, right, shared] {
        graph.insert_record(
            key.to_owned(),
            json!({"key":key,"data":{"parent":Json::Null}}),
        );
    }
    for (from, to) in [(root, left), (root, right), (left, shared), (right, shared)] {
        graph.add_edge(
            from,
            "collection_child",
            0,
            json!({"from":from,"kind":"collection_child","ordinal":0,"to":to}),
        );
    }
    let diagnostic = graph
        .note(
            0,
            None,
            "Shared.record",
            Error::new("missing_library", "shared child"),
        )
        .unwrap();
    graph.attach_record(shared, vec![diagnostic]);
    let errors = graph.errors_for_ids(&graph.reachable_diagnostic_ids(root));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].kind, "missing_library");
}

#[test]
fn a_revisited_edge_keeps_the_cycle_resolution() {
    let mut graph = Graph::new();
    graph.ensure_root(None).unwrap();
    graph.add_edge(
        "s0:1:0",
        "collection_child",
        0,
        json!({"from":"s0:1:0","kind":"collection_child","ordinal":0,"to":"s0:1:1","resolution":"local"}),
    );
    graph.add_edge(
        "s0:1:0",
        "collection_child",
        0,
        json!({"from":"s0:1:0","kind":"collection_child","ordinal":0,"to":"s0:1:1","resolution":"cycle"}),
    );
    assert_eq!(graph.edges.len(), 1);
    assert_eq!(graph.edges[0]["resolution"], "cycle");
}

#[test]
fn reachable_cycle_terminates_and_keeps_the_cycle_edge_diagnostic() {
    let mut graph = Graph::new();
    graph.ensure_root(None).unwrap();
    for key in ["s0:1:0", "s0:1:1"] {
        graph.insert_record(
            key.to_owned(),
            json!({"key":key,"data":{"parent":Json::Null}}),
        );
    }
    graph.add_edge(
        "s0:1:0",
        "collection_child",
        0,
        json!({"from":"s0:1:0","kind":"collection_child","ordinal":0,"to":"s0:1:1","resolution":"local"}),
    );
    let edge = graph.add_edge(
        "s0:1:1",
        "collection_child",
        0,
        json!({"from":"s0:1:1","kind":"collection_child","ordinal":0,"to":"s0:1:0","resolution":"cycle"}),
    );
    let diagnostic = graph
        .note(
            0,
            None,
            "Collection.children",
            Error::new("query_cycle", "collection child cycle"),
        )
        .unwrap();
    graph.attach_edge(edge, vec![diagnostic]);
    let errors = graph.errors_for_ids(&graph.reachable_diagnostic_ids("s0:1:0"));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].kind, "query_cycle");
    assert_eq!(graph.edges[edge]["resolution"], "cycle");
}

#[test]
fn records_and_edges_are_deduplicated_by_key_and_saved_ordinal() {
    let mut graph = Graph::new();
    graph.ensure_root(None).unwrap();
    let key = record_key(&graph.sources[0], &Record::new(7, 2));
    graph.insert_record(key.clone(), json!({"key":key}));
    graph.insert_record(key.clone(), json!({"key":key,"duplicate":true}));
    assert_eq!(graph.records.len(), 1);
    graph.add_edge(&key, "collection_child", 0, json!({"ordinal":0}));
    graph.add_edge(
        &key,
        "collection_child",
        0,
        json!({"ordinal":0,"duplicate":true}),
    );
    graph.add_edge(&key, "collection_child", 1, json!({"ordinal":1}));
    assert_eq!(graph.edges.len(), 2);
    assert_eq!(graph.edges[0]["ordinal"], 0);
    assert_eq!(graph.edges[1]["ordinal"], 1);
}

#[test]
fn graph_names_are_covered_by_the_native_reopen_oracle() {
    let Some(source) = fixture() else { return };
    let Some(oracle_path) = std::env::var_os("OXBLEND_COLLECTION_GRAPH_ORACLE").map(PathBuf::from)
    else {
        eprintln!("native collection-graph oracle not selected");
        return;
    };
    let report = run_query(&source);
    let graph = graph(&report);
    let oracle: Json = serde_json::from_slice(&std::fs::read(oracle_path).unwrap()).unwrap();
    let oracle_collections = oracle["collections"].as_array().unwrap();
    let oracle_objects = oracle["objects"].as_array().unwrap();
    assert_eq!(report["requested_coverage"]["complete"], true, "{report}");
    assert_eq!(
        records(graph).len(),
        oracle_collections.len() + oracle_objects.len(),
        "native inventory coverage mismatch"
    );
    assert_eq!(
        edges(graph).len(),
        oracle["membership_edges"].as_array().unwrap().len(),
        "native membership coverage mismatch"
    );
    let native_collection = |name: &str| {
        oracle_collections
            .iter()
            .find(|row| row["name"] == name)
            .unwrap_or_else(|| panic!("native reopen lacks collection {name}"))
    };
    let native_object = |name: &str| {
        oracle_objects
            .iter()
            .find(|row| row["name"] == name)
            .unwrap_or_else(|| panic!("native reopen lacks object {name}"))
    };
    for record in records(graph) {
        let name = record["id"]["name"].as_str().unwrap();
        let native = match record["kind"].as_str().unwrap() {
            "Collection" => native_collection(name),
            "Object" => native_object(name),
            other => panic!("unexpected graph record kind {other}"),
        };
        let full_name = record["id"]["name_hex"].as_str().unwrap();
        assert!(
            full_name.ends_with(&hex(name.as_bytes())),
            "full saved ID bytes lost the name tail for {name}: {full_name}"
        );
        assert!(full_name.len() >= 4, "saved ID code was lost for {name}");
        let source = graph["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|source| source["key"] == record["source"])
            .unwrap();
        let native_library = native["library"]["filepath"].as_str();
        let expected = record["id"]["library"].as_str();
        if source["role"] == "root" {
            match expected {
                Some(expected) => assert!(
                    native_library.is_some_and(|path| path.ends_with(
                        Path::new(expected)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap()
                    )),
                    "native reopen lost root library {expected} for {name}"
                ),
                None => assert!(
                    native_library.is_none(),
                    "native reopen records {name} in a library but graph records root-local"
                ),
            }
        } else {
            let lexical = source["lexical_path"].as_str().unwrap();
            assert!(
                native_library.is_some_and(|path| path.ends_with(
                    Path::new(lexical)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap()
                )),
                "native reopen does not record {name} from source {lexical}"
            );
        }
        assert_eq!(record["ownership_scope"], json!("containing_saved_file"));
        assert_eq!(
            record["data"]["override"]["present"], native["override"]["present"],
            "native override presence changed for {name}"
        );
        assert!(record["data"].get("parent_type").is_none());
        assert!(record["data"].get("proxy").is_none());
        assert!(record["data"].get("proxy_group").is_none());
        let raw_fields: &[&str] = if record["kind"] == "Collection" {
            &["flag"]
        } else {
            &["type", "partype", "restrictflag"]
        };
        for field in raw_fields {
            let value = &record["data"][*field];
            assert!(
                value.as_i64().is_some()
                    || (value["status"] == "unavailable" && value["diagnostic"].is_number()),
                "raw saved {field} must be an integer or explicit refusal, never null"
            );
            if native[*field].as_i64().is_none() {
                let key = format!("{}.{}", record["kind"].as_str().unwrap(), field);
                assert!(
                    oracle["coverage"]["uncompared_saved_fields"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|x| x == &key),
                    "uncompared saved integer {key} must be declared"
                );
            }
        }
        if record["kind"] == "Collection" {
            if let Some(flag) = native["flag"].as_i64() {
                assert_eq!(
                    record["data"]["flag"],
                    json!(flag),
                    "collection flag {name}"
                );
            }
            if let Some(native_owner) = native.get("owner") {
                assert_eq!(
                    record["data"]["owner"]["name"], native_owner["name"],
                    "collection owner {name}"
                );
            } else {
                assert!(
                    oracle["coverage"]["unexposed_rna_attributes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|x| x == "Collection.owner_id"),
                    "missing owner oracle coverage must be declared"
                );
            }
        } else {
            for field in ["type", "partype", "restrictflag"] {
                if let Some(value) = native[field].as_i64() {
                    assert_eq!(
                        record["data"][field],
                        json!(value),
                        "object {field} of {name}"
                    );
                }
            }
        }
    }
    for edge in edges(graph) {
        let Some(to) = edge["to"].as_str() else {
            continue;
        };
        let from = edge["from"].as_str().unwrap();
        let from_name = records(graph)
            .iter()
            .find(|record| record["key"] == from)
            .and_then(|record| record["id"]["name"].as_str())
            .unwrap();
        let to_name = records(graph)
            .iter()
            .find(|record| record["key"] == to)
            .and_then(|record| record["id"]["name"].as_str())
            .unwrap();
        let ordinal = edge["ordinal"].as_u64().unwrap() as usize;
        let native_members = match edge["kind"].as_str().unwrap() {
            "collection_child" => native_collection(from_name)["children"].clone(),
            "collection_object" => native_collection(from_name)["objects"].clone(),
            other => panic!("unexpected edge kind {other}"),
        };
        assert_eq!(
            native_members[ordinal],
            json!(to_name),
            "native ordinal {ordinal} of {from_name} does not match {to_name}"
        );
    }
    for record in records(graph) {
        if record["kind"] != "Object" {
            continue;
        }
        let name = record["id"]["name"].as_str().unwrap();
        let native = native_object(name);
        if let Some(parent) = record["data"]["parent"].as_str() {
            let parent_name = records(graph)
                .iter()
                .find(|candidate| candidate["key"] == parent)
                .and_then(|candidate| candidate["id"]["name"].as_str())
                .unwrap();
            assert_eq!(
                native["parent"],
                json!(parent_name),
                "native parent of {name} does not match the graph"
            );
        } else {
            assert!(native["parent"].is_null(), "graph lost parent of {name}");
        }
    }
}
