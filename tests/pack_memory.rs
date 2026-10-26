use std::process::Command;

#[test]
fn memory_help_explains_the_opt_in_tradeoff_on_both_commands() {
    for command in ["pack", "pack-sequence"] {
        let output = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        for fact in [
            "--memory fast|bounded",
            "fast (default) uses signature-bound selective raw windows",
            "reading only needed raw ranges and retaining no whole decoded sources on that path",
            "Compressed blends fall back to full decoded capture",
            "Python Sessions retain captured sources as before",
            "largest single source plus indexes",
            "not a hard RAM ceiling",
            "re-reading sources",
            "more IO, slower",
            "Selected workload measurements",
            "fast heap 27-53% lower",
            "not a universal memory or timing guarantee",
            "Same output bytes and refusals",
            "large sequences on shared workstations",
        ] {
            assert!(help.contains(fact), "{command} help omits {fact}");
        }
    }
}

#[test]
fn bounded_pack_failure_receipt_records_mode_and_fast_stays_unchanged() {
    let root = std::env::temp_dir().join(format!(
        "oxblend-pack-memory-receipt-{}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    let target = root.join("exists.zip");
    std::fs::write(&target, b"untouched sentinel").unwrap();
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/synthetic-empty-seed.blend");
    for mode in [None, Some("fast"), Some("bounded")] {
        // An existing target refuses inside pack, after the policy is selected.
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        command.arg("pack").arg(&input).arg(&target);
        if let Some(mode) = mode {
            command.args(["--memory", mode]);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        let envelope = output
            .stderr
            .split(|b| *b == b'\n')
            .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
            .find(|v| v.get("diagnostic").is_some())
            .expect("bound failure receipt");
        assert_eq!(envelope["error"]["kind"], "incomplete_pack");
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            report["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "output_exists")
        );
        if mode == Some("bounded") {
            assert_eq!(envelope["diagnostic"]["memory"], "bounded");
        } else {
            assert!(envelope["diagnostic"].get("memory").is_none());
        }
    }
    assert_eq!(std::fs::read(&target).unwrap(), b"untouched sentinel");
    std::fs::remove_file(target).unwrap();
    std::fs::remove_dir(root).unwrap();
}
