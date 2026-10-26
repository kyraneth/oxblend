use serde_json::Value;
use std::process::Command;
#[path = "support/scratch.rs"]
mod scratch;

#[test]
fn extract_cli_reports_policies_and_refuses_without_them() {
    let root = scratch::scratch_dir(&std::env::temp_dir());
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/extract/native-5.2.1.blend");
    let output = root.join("library.blend");
    let run = |clear: bool| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        c.arg("extract")
            .arg(&source)
            .args(["--id", "NTClosureProbe", "--output"])
            .arg(&output)
            .arg("--json");
        if clear {
            c.args(["--clear", "CompositorNodeRLayers.scene"]);
        }
        c.output().unwrap()
    };
    let refused = run(false);
    assert_eq!(refused.status.code(), Some(2));
    assert!(!output.exists());
    let r: Value = serde_json::from_slice(&refused.stdout).unwrap();
    assert_eq!(r["complete"], false);
    let success = run(true);
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stderr)
    );
    let r: Value = serde_json::from_slice(&success.stdout).unwrap();
    assert_eq!(r["complete"], true);
    assert_eq!(r["output_written"], true);
    assert!(
        r["applied_policies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["struct_field"] == "bNode.id")
    );
    let before = std::fs::read(&output).unwrap();
    let existing = run(true);
    assert_eq!(existing.status.code(), Some(2));
    assert_eq!(std::fs::read(output).unwrap(), before);
    for args in [["extract", "--help"], ["help", "error"]] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        c.args(args);
        if args[0] == "help" {
            c.arg("extract_refusal");
        }
        assert!(c.output().unwrap().status.success());
    }
}
