//! Re-reaching a processed stub in one selection pass is not recursion.
use serde_json::Value;
use std::{path::Path, process::Command};
#[test]
fn override_reference_can_reach_an_already_processed_stub_without_a_false_cycle() {
    let work = std::env::temp_dir().join(format!("oxblend-stub-requeue-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stub-requeue");
    for name in ["root.blend", "child.blend", "nested.blend"] {
        std::fs::copy(fixture.join(name), work.join(name)).unwrap();
    }
    let root = work.join("root.blend");
    let trace = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("trace")
        .arg(&root)
        .output()
        .unwrap();
    let j: Value = serde_json::from_slice(&trace.stdout).unwrap();
    assert!(trace.status.success(), "{j}");
    assert_eq!(j["complete"], true);
    assert!(j["errors"].as_array().unwrap().is_empty());
    let out = work.join("package");
    let pack = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("pack")
        .arg(&root)
        .arg(&out)
        .arg("-p")
        .arg(&work)
        .output()
        .unwrap();
    assert!(
        pack.status.success(),
        "{}",
        String::from_utf8_lossy(&pack.stdout)
    );
    for name in ["root.blend", "child.blend", "nested.blend"] {
        assert!(out.join(name).is_file());
    }
    std::fs::remove_dir_all(work).unwrap();
}
