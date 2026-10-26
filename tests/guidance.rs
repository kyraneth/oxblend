//! Agent-facing guarantees are checked through the shipped CLI, not source text alone.
#[path = "support/scratch.rs"]
mod scratch;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}
fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .args(args)
        .output()
        .unwrap()
}
fn error(out: &Output) -> Value {
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    serde_json::from_slice::<Value>(&out.stderr).unwrap()["error"].clone()
}
#[test]
fn incomplete_messages_distinguish_report_plan_refusal_and_publication() {
    let work = scratch::scratch_dir(&std::env::temp_dir());
    let input = work.join("shot.blend");
    // A real missing external Text file, not an unknown pointer.
    std::fs::copy(fixture("dependencies/text-name.blend"), &input).unwrap();
    for command in ["pack", "pack-sequence"] {
        for (label, flags, expected) in [
            ("strict", vec![], "Package not published"),
            (
                "plan",
                vec!["--dry-run"],
                "Dry-run only; nothing was written",
            ),
            (
                "partial-plan",
                vec!["--permissive", "--dry-run"],
                "Dry-run only; nothing was written",
            ),
            (
                "partial",
                vec!["--permissive"],
                "INCOMPLETE package: output was published",
            ),
        ] {
            let target = work.join(format!("{command}-{label}"));
            let mut args = vec![command.as_ref(), input.as_os_str()];
            if command == "pack-sequence" {
                args.push("-t".as_ref());
            }
            args.push(target.as_os_str());
            args.extend(flags.iter().map(std::ffi::OsStr::new));
            let out = run(&args);
            let err = error(&out);
            assert_eq!(err["kind"], "incomplete_pack");
            let message = err["message"].as_str().unwrap();
            assert!(
                message.starts_with(expected),
                "{command}/{label}: {message}"
            );
            if label.contains("plan") {
                assert!(!message.contains("may remain"));
                assert!(!message.contains("was published"));
                assert!(!target.exists());
            } else if label == "strict" {
                assert!(message.contains("--permissive"));
                assert!(!target.exists());
            } else {
                assert!(message.contains("Never merge/resume"));
                assert!(target.join("pack-info.txt").is_file());
            }
            // Existing structured error envelope survives wording changes.
            assert!(err.get("next_command").is_some());
            let report: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(report["native_validation"], "not_performed");
        }
    }
    let invalid = fixture("file-ops/nodes-first-fileglobal.blend");
    let out = run(&["trace".as_ref(), invalid.as_os_str()]);
    let err = error(&out);
    assert_eq!(err["kind"], "incomplete_trace");
    assert!(
        err["message"]
            .as_str()
            .unwrap()
            .contains("Dependency report is INCOMPLETE")
    );
    assert!(!out.stdout.is_empty());
    std::fs::remove_dir_all(work).unwrap();
}
#[test]
fn successful_pack_and_help_do_not_claim_native_validation() {
    let work = scratch::scratch_dir(&std::env::temp_dir());
    let input = fixture("file-ops/synthetic-empty-seed.blend");
    let target = work.join("package");
    let out = run(&["pack".as_ref(), input.as_os_str(), target.as_os_str()]);
    assert!(out.status.success(), "{out:?}");
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["complete"], true);
    assert_eq!(report["native_validation"], "not_performed");
    let info = std::fs::read_to_string(target.join("pack-info.txt")).unwrap();
    assert!(info.contains("Blender open, evaluation and rendering have NOT been verified"));
    assert!(info.contains("selected serialized dependency closure"));
    for command in ["pack", "pack-sequence", "trace"] {
        let out = run(&[command.as_ref(), "--help".as_ref()]);
        assert!(out.status.success());
        let help = String::from_utf8(out.stdout).unwrap();
        assert!(help.contains("Diagnose does not mean repair"));
        if command != "trace" {
            assert!(help.contains("Blender open, evaluation and rendering have NOT been verified"));
        }
    }
    for command in ["inspect", "get"] {
        let out = run(&[command.as_ref(), "--help".as_ref()]);
        let help = String::from_utf8(out.stdout).unwrap();
        assert!(help.contains("address_decimal"));
        assert!(help.contains("53430000"));
    }
    std::fs::remove_dir_all(work).unwrap();
}
