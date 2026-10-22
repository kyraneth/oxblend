//! Library entry: one malformed instrumentation call must not kill tracing.
use serde_json::{Value, json};
use std::process::Command;
#[test]
fn writer_error_skips_only_the_bad_call() {
    let dir = std::env::temp_dir().join(format!("oxblend-trace-recover-{}", std::process::id()));
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "writer_error_child", "--nocapture"])
        .env("OXBLEND_TRACE_RECOVERY_CHILD", "1")
        .env("FLATTRACE_DIR", &dir)
        .env_remove("FLATTRACE_TRACEPARENT")
        .output()
        .unwrap();
    assert!(child.status.success(), "{child:?}");
    let files = std::fs::read_dir(&dir)
        .unwrap()
        .collect::<std::io::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(files.len(), 1);
    let rows: Vec<Value> = std::fs::read_to_string(files[0].path())
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(
        rows.iter()
            .any(|r| r["event"] == "after_bad_call" && r["state"] == "end")
    );
    assert_eq!(rows.last().unwrap()["event"], "run");
    assert_eq!(rows.last().unwrap()["state"], "end");
}
#[test]
fn writer_error_child() {
    if std::env::var_os("OXBLEND_TRACE_RECOVERY_CHILD").is_none() {
        return;
    }
    oxblend::diagnostics::init(&[], "library_test");
    oxblend::diagnostics::Span::new("run", json!({})).end("ok");
    oxblend::diagnostics::Span::new("after_bad_call", json!({})).end("ok");
    oxblend::diagnostics::finish("ok");
}
