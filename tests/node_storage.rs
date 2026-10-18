//! Serialized structural admission, through both SDK and real public CLI.
use oxblend::trace::{TraceOptions, trace};
use std::path::PathBuf;
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/node-storage")
        .join(format!("{name}-root.blend"))
}
#[test]
fn numeric_and_nested_reference_free_storage_do_not_abort_dependency_scan() {
    for name in ["numeric", "nested", "socket"] {
        let input = fixture(name);
        let bytes = std::fs::read(&input).unwrap();
        let report = trace(&input, TraceOptions::default()).unwrap();
        assert!(report.complete, "{:?}", report.errors);
        assert!(report.errors.is_empty());
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args(["trace", input.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(json["complete"], true);
        if name == "socket" {
            assert!(
                report
                    .usages
                    .iter()
                    .any(|u| u.asset_path == b"//socket-texture.blend")
            );
            assert_eq!(report.usages.len(), 2);
            assert_eq!(json["usages"].as_array().unwrap().len(), 2);
        }
        assert_eq!(std::fs::read(input).unwrap(), bytes);
    }
}
#[test]
fn path_arrays_nested_paths_data_pointers_and_unknown_labels_still_refuse() {
    for name in [
        "path",
        "nested-path",
        "pointer",
        "label",
        "socket-extent",
        "socket-void",
        "socket-path",
    ] {
        let input = fixture(name);
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
    }
}
