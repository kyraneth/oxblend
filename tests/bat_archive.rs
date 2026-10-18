//! archive ownership/publication negatives through the actual CLI.
#[path = "support/scratch.rs"]
mod test_scratch;
use serde_json::Value;
use std::process::Command;
#[test]
fn zip_durability_receipt_covers_members_and_compress_refuses_before_io() {
    let root = test_scratch::scratch_dir(&std::env::temp_dir());
    let original = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/trace-owner/cross-id.blend");
    let input = root.join("cross-id.blend");
    std::fs::copy(original, &input).unwrap();
    for name in ["A.pc2", "B.pc2"] {
        std::fs::write(root.join(name), b"authored receiving-boundary fixture").unwrap();
    }
    // Owner-scoped SDNA and actual copy entry paths; no native cache-content claim.
    let target = root.join("qualified.zip");
    let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("pack")
        .arg(&input)
        .arg(&target)
        .args(["--durability", "each-file"])
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "{} {}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr)
    );
    let j: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(j["archive_written"], true);
    assert_eq!(j["durability_completed"], true);
    assert_eq!(j["pack_info"]["durability_completed"], true);
    assert!(
        j["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["durability_completed"] == true)
    );
    let refused = root.join("refused.zip");
    let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("pack")
        .arg(&input)
        .arg(&refused)
        .arg("--compress")
        .output()
        .unwrap();
    assert!(!r.status.success());
    assert!(!refused.exists());
    std::fs::remove_dir_all(&root).unwrap();
}
