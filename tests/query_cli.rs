//! Real CLI contract tests for one-open raw queries and discovery.
use serde_json::Value;
use std::process::Command;
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .args(args)
        .output()
        .unwrap()
}
fn seed() -> String {
    format!(
        "{}/tests/fixtures/file-ops/synthetic-empty-seed.blend",
        env!("CARGO_MANIFEST_DIR")
    )
}
#[test]
fn query_returns_raw_values_and_independent_unavailable_fields() {
    let file = seed();
    let out = run(&[
        "query",
        &file,
        "--type",
        "Scene",
        "--fields",
        "r.sfra,r.efra,r.frs_sec_base,no_such_field",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["schema"], "oxblend.query/1");
    assert_eq!(report["requested_coverage"]["complete"], false);
    assert_eq!(report["rows"][0]["values"]["r.sfra"]["status"], "available");
    assert_eq!(
        report["rows"][0]["values"]["r.frs_sec_base"]["value"]["kind"],
        "float32"
    );
    assert_eq!(
        report["rows"][0]["values"]["no_such_field"]["kind"],
        "missing_field"
    );
    assert_eq!(
        report["rows"][0]["values"]["no_such_field"]["next_action"]["command"],
        "explain"
    );
}
#[test]
fn successful_zero_matches_is_not_a_default_record() {
    let out = run(&[
        "query",
        &seed(),
        "--type",
        "NodeImageMultiFile",
        "--fields",
        "base_path",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["rows"], serde_json::json!([]));
    assert_eq!(report["requested_coverage"]["complete"], true);
}
#[test]
fn catalog_explain_and_help_are_real_entry_paths() {
    for command in ["query", "catalog", "explain"] {
        assert!(run(&[command, "--help"]).status.success());
    }
    let out = run(&["catalog", "--json"]);
    assert!(out.status.success());
    let c: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(c["catalog_revision"], "2");
    let names: Vec<_> = c["concepts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "scene.render",
            "scene.view_layers",
            "scene.compositor",
            "collection.graph",
            "id.properties"
        ]
    );
    let graph = c["concepts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "collection.graph")
        .unwrap();
    assert_eq!(graph["owners"], serde_json::json!(["Collection", "Scene"]));
    assert_eq!(graph["aliases"], serde_json::json!(["collection_graph"]));
    assert_eq!(c["read_operations"].as_array().unwrap().len(), 2);
    let explained = run(&["explain", "collection_graph", "--file", &seed(), "--json"]);
    assert!(explained.status.success());
    let graph_explain: Value = serde_json::from_slice(&explained.stdout).unwrap();
    assert_eq!(graph_explain["concept"], "collection.graph");
    assert!(
        graph_explain["sdna"]
            .as_array()
            .unwrap()
            .iter()
            .any(|ty| ty["type"] == "Collection")
    );
    let out = run(&["explain", "scene.compositor", "--file", &seed(), "--json"]);
    assert!(out.status.success());
    let e: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!e["sdna"].as_array().unwrap().is_empty());
    let out = run(&["explain", "not_a_concept", "--json"]);
    assert_eq!(out.status.code(), Some(2));
}
#[test]
fn duplicate_fields_and_unknown_options_are_usage_errors() {
    for args in [
        vec![
            "query",
            &seed(),
            "--type",
            "Scene",
            "--fields",
            "r.sfra,r.sfra",
        ],
        vec!["catalog", "--bad"],
    ] {
        assert_eq!(run(&args).status.code(), Some(2));
    }
}
