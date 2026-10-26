//! Aggregate errors retain their captured structural owner, not current scope.
use serde_json::{Value, json};
use std::process::Command;

#[test]
fn error_child() {
    if std::env::var_os("OXBLEND_AGGREGATE_ERRORS").is_none() {
        return;
    }
    oxblend::tracing::initialize();
    let outer = oxblend::tracing::Span::new("query.select", json!({"type":"owner"}));
    let mut hot = oxblend::tracing::Span::new("query.resolve", json!({}));
    let inner = oxblend::tracing::Span::new("serialize", json!({}));
    hot.record_error(
        "dangling_pointer",
        "retained while another scope is current",
    );
    inner.end(oxblend::tracing::Outcome::Ok);
    hot.end(oxblend::tracing::Outcome::Error);
    outer.end(oxblend::tracing::Outcome::Ok);
    // No thread-local structural owner: flush this aggregate on the root.
    let mut orphan = oxblend::tracing::Span::new("query.resolve", json!({}));
    orphan.record_error("dangling_pointer", "retained on root");
    orphan.end(oxblend::tracing::Outcome::Error);
    oxblend::diagnostics::finish("ok");
}

#[test]
fn errors_reconcile_on_exact_aggregate_owner_and_root() {
    let directory =
        std::env::temp_dir().join(format!("oxblend-aggregate-errors-{}", std::process::id()));
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "error_child", "--nocapture"])
        .env("OXBLEND_AGGREGATE_ERRORS", "1")
        .env("FLATTRACE_DIR", &directory)
        .env_remove("FLATTRACE_LEVEL")
        .env_remove("FLATTRACE_TRACEPARENT")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let files: Vec<_> = std::fs::read_dir(&directory).unwrap().collect();
    assert_eq!(files.len(), 1);
    let rows: Vec<Value> = std::fs::read_to_string(files[0].as_ref().unwrap().path())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let summaries: Vec<_> = rows
        .iter()
        .filter(|r| r["event"] == "hot.summary")
        .collect();
    assert_eq!(summaries.len(), 2);
    for summary in summaries {
        assert_eq!(summary["data"]["count"], 1);
        assert_eq!(summary["data"]["error_records"], 1);
        assert_eq!(summary["data"]["outcomes"]["error"], 1);
        let errors: Vec<_> = rows
            .iter()
            .filter(|r| {
                r["event"] == "error"
                    && r["span"] == summary["span"]
                    && r["data"]["hot_event"] == "query.resolve"
            })
            .collect();
        assert_eq!(errors.len(), 1);
        let owner = rows
            .iter()
            .find(|r| r["state"] == "start" && r["span"] == summary["span"])
            .unwrap();
        assert!(["run", "query.select"].contains(&owner["event"].as_str().unwrap()));
    }
    let root = rows.iter().find(|r| r["event"] == "root.summary").unwrap();
    assert_eq!(root["data"]["errors"].as_array().unwrap().len(), 2);
    assert_eq!(
        root["data"]["error_propagations"].as_array().unwrap().len(),
        2
    );
    std::fs::remove_dir_all(directory).unwrap();
}
