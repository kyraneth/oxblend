//! Authored declared-structure closure and integrity refusal proofs.
use oxblend::trace::{TraceOptions, trace};
use std::path::PathBuf;
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/constraint-data")
        .join(format!("{name}-root.blend"))
}
#[test]
fn constraint_tar_and_action_are_library_edges_and_source_sdna_extent_is_used() {
    let input = fixture("normal");
    let original = std::fs::read(&input).unwrap();
    let report = trace(&input, TraceOptions::default()).unwrap();
    assert!(report.complete, "{:?}", report.errors);
    assert!(report.errors.is_empty());
    let paths: Vec<_> = report
        .usages
        .iter()
        .map(|u| u.asset_path.as_slice())
        .collect();
    assert_eq!(
        paths,
        vec![b"//normal.blend".as_slice(), b"//tar.blend", b"//act.blend"]
    );
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .args(["trace", input.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(result.status.success());
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["complete"], true);
    assert_eq!(json["usages"].as_array().unwrap().len(), 3);
    assert_eq!(std::fs::read(input).unwrap(), original);
}
#[test]
fn constraint_and_driver_extent_void_path_and_intrusive_integrity_seeds_refuse() {
    for name in [
        "extent",
        "driver-extent",
        "void",
        "path",
        "reciprocity",
        "last",
        "cycle",
        "sibling",
        "dangling",
    ] {
        let input = fixture(name);
        let original = std::fs::read(&input).unwrap();
        let report = trace(&input, TraceOptions::default()).unwrap();
        assert!(!report.complete, "{name}");
        assert!(!report.errors.is_empty(), "{name}");
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args(["trace", input.to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2), "{name}");
        let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(json["complete"], false, "{name}");
        assert_eq!(std::fs::read(input).unwrap(), original);
    }
    let mut options = TraceOptions::default();
    options.pointers = oxblend::trace::PointerMode::NonStrict;
    let partial = trace(&fixture("dangling"), options).unwrap();
    assert!(!partial.complete);
    assert!(
        partial
            .warnings
            .iter()
            .any(|w| matches!(w, oxblend::trace::TraceWarning::SilencedPointer { .. }))
    );
    assert!(
        partial
            .usages
            .iter()
            .any(|u| u.asset_path == b"//act.blend")
    );
}
