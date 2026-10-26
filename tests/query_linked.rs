//! Native linked-subtype proof. Build a fresh fixture with the bundled builder;
//! OXBLEND_QUERY_LINKED_FIXTURE selects its root.blend. No studio input is used.
use oxblend::query::{self, LibraryMap, LibraryView};
use oxblend::{BlendFile, Error};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
fn fixture() -> Option<PathBuf> {
    let path = std::env::var_os("OXBLEND_QUERY_LINKED_FIXTURE").map(PathBuf::from);
    if path.is_none() {
        eprintln!(
            "native linked-subtype fixture not selected; set OXBLEND_QUERY_LINKED_FIXTURE after running its builder"
        );
    }
    path
}
#[test]
fn two_phase_owner_is_requested_once_per_library_and_keeps_local_first_selection() {
    let Some(source) = fixture() else { return };
    let root = BlendFile::open(&source).unwrap();
    let fields = ["compositor".into()];
    let mut owned: BTreeMap<PathBuf, BlendFile> = BTreeMap::new();
    let mut counts = HashMap::new();
    loop {
        let views: LibraryMap<'_> = owned
            .iter()
            .map(|(path, reader)| {
                (
                    path.clone(),
                    Ok(LibraryView {
                        reader,
                        provenance: json!({"owner":"counting-fixture-store"}),
                    }),
                )
            })
            .collect();
        let requests =
            query::library_requests(&root, Some(&source), "Scene", &fields, &views).unwrap();
        if requests.is_empty() {
            let report =
                query::query_with_libraries(&root, Some(&source), "Scene", &fields, &views)
                    .unwrap();
            assert_eq!(report["requested_coverage"]["complete"], true, "{report}");
            let observations = report["linked_subtype_observations"].as_array().unwrap();
            assert!(
                observations
                    .iter()
                    .any(|o| o["selected_from"] == "local_stub")
            );
            assert!(
                observations
                    .iter()
                    .any(|o| o["selected_from"] == "library_local"
                        && o["saved_reference"]["kind"] == "CompositorNodeTree")
            );
            assert!(
                !observations
                    .iter()
                    .any(|o| o["selected_from"] == "library_stub")
            );
            break;
        }
        drop(views); // A Vec-backed Session can now mutably capture without a root borrow.
        for request in requests {
            *counts.entry(request.key.clone()).or_insert(0) += 1;
            assert!(
                owned
                    .insert(request.key, BlendFile::open(request.path).unwrap())
                    .is_none()
            );
        }
    }
    assert_eq!(counts.len(), 1);
    assert!(counts.values().all(|count| *count == 1));
}
#[test]
fn no_source_and_missing_libraries_keep_partial_saved_graph_with_site_diagnostics() {
    let Some(source) = fixture() else { return };
    let file = BlendFile::open(&source).unwrap();
    let fields = ["compositor".into()];
    let report = query::query(&file, "Scene", &fields).unwrap();
    assert_eq!(report["requested_coverage"]["complete"], false);
    assert_eq!(
        report["linked_subtype_diagnostics"][0]["kind"],
        "unverified_semantics"
    );
    assert!(report["rows"][0]["values"]["compositor"]["partial_value"]["trees"].is_array());
    let directory = std::env::temp_dir().join(format!(
        "oxblend-missing-query-lib-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let copy = directory.join("root.blend");
    std::fs::copy(&source, &copy).unwrap();
    let missing = BlendFile::open(&copy).unwrap();
    let report = query::query_with_source(&missing, &copy, "Scene", &fields).unwrap();
    assert_eq!(report["requested_coverage"]["complete"], false);
    assert_eq!(
        report["linked_subtype_diagnostics"][0]["kind"],
        "missing_library"
    );
    assert!(report["rows"][0]["values"]["compositor"]["partial_value"]["trees"].is_array());
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn failed_capture_is_not_requested_again_and_changed_capture_discards_observation() {
    let Some(source) = fixture() else { return };
    let file = BlendFile::open(&source).unwrap();
    let fields = ["compositor".into()];
    let requests =
        query::library_requests(&file, Some(&source), "Scene", &fields, &LibraryMap::new())
            .unwrap();
    assert_eq!(requests.len(), 1);
    for kind in ["library_read", "input_changed"] {
        let mut libraries = LibraryMap::new();
        libraries.insert(
            requests[0].key.clone(),
            Err(Error::new(kind, "fixture capture failure")),
        );
        if kind == "input_changed" {
            assert_eq!(
                query::query_with_libraries(&file, Some(&source), "Scene", &fields, &libraries)
                    .unwrap_err()
                    .kind,
                "input_changed"
            );
        } else {
            assert!(
                query::library_requests(&file, Some(&source), "Scene", &fields, &libraries)
                    .unwrap()
                    .is_empty()
            );
            let report =
                query::query_with_libraries(&file, Some(&source), "Scene", &fields, &libraries)
                    .unwrap();
            assert_eq!(
                report["linked_subtype_diagnostics"][0]["kind"],
                "library_read"
            );
            assert_eq!(report["requested_coverage"]["complete"], false);
        }
    }
}
