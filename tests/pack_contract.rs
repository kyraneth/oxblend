use serde_json::Value;
use std::{path::PathBuf, process::Command};

#[test]
fn trustworthy_missing_assets_publish_with_errors_but_unreadable_libraries_do_not() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = std::env::temp_dir().join(format!("oxblend-pack-contract-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let source = root.join("tests/fixtures/trace-owner/pointer-kinds/id-only-data.blend");
    for suffix in ["directory", "archive.zip"] {
        let out = work.join(suffix);
        let result = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("pack")
            .arg(&source)
            .arg(&out)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        let report: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["planning_complete"], true);
        assert_eq!(report["errors"].as_array().unwrap().len(), 2);
        assert!(report["errors"].as_array().unwrap().iter().all(|e| {
            e["kind"] == "missing_asset"
                && e["message"]
                    .as_str()
                    .unwrap()
                    .contains("referenced by block")
        }));
        assert!(out.exists());
    }
    // Existing but unreadable linked library: known metadata is not enough to
    // claim a trustworthy dependency plan. Both directory and ZIP stay absent.
    let input = work.join("selected-material-root.blend");
    std::fs::copy(
        root.join("tests/fixtures/trace-owner/pointer-kinds/selected-material-root.blend"),
        &input,
    )
    .unwrap();
    std::fs::write(work.join("selected-material-child.blend"), b"not a blend").unwrap();
    for suffix in ["refused-directory", "refused.zip"] {
        let out = work.join(suffix);
        let result = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("pack")
            .arg(&input)
            .arg(&out)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        let report: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["planning_complete"], false);
        assert_eq!(report["errors"][0]["kind"], "incomplete_trace");
        assert!(!out.exists());
        assert!(report["files"].as_array().unwrap().is_empty());
    }
    std::fs::remove_dir_all(work).unwrap();
}
