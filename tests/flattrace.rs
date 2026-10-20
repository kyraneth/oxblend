//! Real CLI activation, context and transparent sink failures.
use serde_json::Value;
use std::{path::PathBuf, process::Command};
struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("oxblend-flattrace-{}-{label}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/synthetic-empty-seed.blend")
}
fn cli() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_oxblend"));
    c.env_remove("FLATTRACE_DIR")
        .env_remove("FLATTRACE_TRACEPARENT");
    c
}
#[test]
fn opt_in_context_and_command_result_are_real() {
    let work = Work::new("parent");
    let plain = cli().arg("summary").arg(source()).output().unwrap();
    let traced = cli()
        .arg("summary")
        .arg(source())
        .env("FLATTRACE_DIR", &work.0)
        .env(
            "FLATTRACE_TRACEPARENT",
            "00-11111111111111111111111111111111-2222222222222222-01",
        )
        .output()
        .unwrap();
    assert_eq!(
        (plain.status.code(), &plain.stdout, &plain.stderr),
        (traced.status.code(), &traced.stdout, &traced.stderr)
    );
    let path = std::fs::read_dir(&work.0)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let rows: Vec<Value> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rows[0]["trace"], "11111111111111111111111111111111");
    assert_eq!(rows[0]["parent"], "2222222222222222");
    for (seq, row) in rows.iter().enumerate() {
        assert_eq!(row["seq"], seq);
    }
    for event in [
        "run",
        "summary",
        "file.open",
        "index",
        "schema",
        "serialize",
    ] {
        assert_eq!(
            rows.iter()
                .filter(|r| r["event"] == event && r["state"] == "start")
                .count(),
            1,
            "{event}"
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r["event"] == event && r["state"] == "end" && r["outcome"] == "ok")
                .count(),
            1,
            "{event}"
        );
    }
    assert_eq!(rows.last().unwrap()["event"], "run");
}
#[test]
fn unavailable_sink_and_bad_context_warn_once_without_product_failure() {
    let work = Work::new("sink");
    let blocked = work.0.join("blocked");
    std::fs::write(&blocked, b"not a directory").unwrap();
    let plain = cli().arg("summary").arg(source()).output().unwrap();
    for parent in [None, Some("bad")] {
        let mut c = cli();
        c.arg("summary").arg(source()).env(
            "FLATTRACE_DIR",
            if parent.is_none() { &blocked } else { &work.0 },
        );
        if let Some(parent) = parent {
            c.env("FLATTRACE_TRACEPARENT", parent);
        }
        let actual = c.output().unwrap();
        assert_eq!(actual.status.code(), plain.status.code());
        assert_eq!(actual.stdout, plain.stdout);
        assert_eq!(
            String::from_utf8_lossy(&actual.stderr)
                .matches("flattrace: tracing disabled:")
                .count(),
            1
        );
    }
}
#[test]
fn pack_io_failure_names_operation_and_actual_parent() {
    let work = Work::new("io");
    let output = work.0.join("absent-parent/package.zip");
    let actual = cli()
        .arg("pack")
        .arg(source())
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(actual.status.code(), Some(2));
    let text = String::from_utf8_lossy(&actual.stdout);
    assert!(text.contains("create output stage"), "{text}");
    assert!(text.contains(output.to_str().unwrap()), "{text}");
}
