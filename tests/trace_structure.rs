//! #62: amplify real saved-pointer calls without adding files/edges/phases.
use serde_json::{Value, json};
use std::process::Command;
#[path = "support/pointed_arrays.rs"]
mod arrays_fixture;

#[test]
fn traced_calls_child() {
    let Some(count) = std::env::var_os("OXBLEND_STRUCTURE_CALLS") else {
        return;
    };
    let count: usize = count.to_str().unwrap().parse().unwrap();
    oxblend::tracing::initialize();
    let file = oxblend::BlendFile::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/file-ops/synthetic-empty-seed.blend"
    ))
    .unwrap();
    let arrays = oxblend::BlendFile::from_bytes(arrays_fixture::synthetic(true, 8)).unwrap();
    let widget = arrays
        .blocks()
        .iter()
        .position(|b| b.old_address == 0x1000)
        .unwrap();
    let owner = oxblend::tracing::Span::new("query.select", json!({"type":"structural-test"}));
    for _ in 0..count {
        assert_eq!(file.dereference(0).unwrap(), None);
        // Actual fixture bytes: eight label bytes and three 64-bit slots.
        arrays
            .byte_array(
                widget,
                0,
                &[oxblend::value::PathStep::Field(b"label".to_vec())],
                8,
            )
            .unwrap();
        arrays
            .pointer_table(
                widget,
                0,
                &[oxblend::value::PathStep::Field(b"parts".to_vec())],
                3,
            )
            .unwrap();
    }
    owner.end(oxblend::tracing::Outcome::Ok);
    oxblend::diagnostics::finish("ok");
}

#[test]
fn default_records_are_independent_of_pointer_call_count() {
    let file = oxblend::BlendFile::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/file-ops/synthetic-empty-seed.blend"
    ))
    .unwrap();
    // Amplification derives from this input's structure, not a trace byte limit.
    let calls = file.blocks().len() + 1;
    assert!(calls > 1);
    let work = std::env::temp_dir().join(format!("oxblend-structure-{}", std::process::id()));
    let mut sizes = Vec::new();
    for count in [1, calls] {
        let directory = work.join(count.to_string());
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "traced_calls_child", "--nocapture"])
            .env("OXBLEND_STRUCTURE_CALLS", count.to_string())
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
        assert_eq!(rows[0]["data"]["level"], 1);
        let summaries: Vec<_> = rows
            .iter()
            .filter(|r| r["event"] == "hot.summary")
            .collect();
        assert_eq!(summaries.len(), 3);
        for (event, bytes) in [
            ("query.resolve", 0),
            ("read.byte_array", 8),
            ("read.pointer_table", 24),
        ] {
            let summary = summaries
                .iter()
                .find(|r| r["data"]["event"] == event)
                .unwrap();
            assert_eq!(summary["data"]["count"], count);
            assert_eq!(summary["data"]["outcomes"]["ok"], count);
            assert_eq!(summary["data"]["bytes"], count * bytes);
        }
        assert!(!rows.iter().any(|r| r["event"] == "query.resolve"));
        sizes.push(rows.len());
    }
    assert_eq!(
        sizes[0], sizes[1],
        "records grew with inner-loop calls, without any new structural owner"
    );
    std::fs::remove_dir_all(work).unwrap();
}
