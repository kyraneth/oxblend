//! An external Rust consumer: one process run, one scope per synchronous call.
use oxblend::tracing::{Outcome, Span, initialize};
use serde_json::{Value, json};
use std::process::Command;
#[test]
fn library_once_and_per_call_scopes() {
    let dir = std::env::temp_dir().join(format!("oxblend-library-trace-{}", std::process::id()));
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "library_child", "--nocapture"])
        .env("OXBLEND_LIBRARY_TRACE_CHILD", "1")
        .env("FLATTRACE_DIR", &dir)
        .env("FLATTRACE_LEVEL", "2")
        .env(
            "FLATTRACE_TRACEPARENT",
            "00-11111111111111111111111111111111-2222222222222222-01",
        )
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    assert!(result.stderr.is_empty(), "{result:?}");
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
    assert_eq!(
        rows.iter()
            .filter(|r| r["event"] == "run" && r["state"] == "start")
            .count(),
        1
    );
    assert_eq!(rows[0]["trace"], "11111111111111111111111111111111");
    assert_eq!(rows[0]["parent"], "2222222222222222");
    assert!(
        rows[0]["data"]["env"]["tool.build.commit"]
            .as_str()
            .is_some()
    );
    for (call, outcome) in [(0, "ok"), (1, "cancelled")] {
        let starts: Vec<_> = rows
            .iter()
            .filter(|r| {
                r["event"] == "consumer.call" && r["state"] == "start" && r["data"]["call"] == call
            })
            .collect();
        assert_eq!(starts.len(), 1);
        let ends: Vec<_> = rows
            .iter()
            .filter(|r| r["span"] == starts[0]["span"] && r["state"] == "end")
            .collect();
        assert_eq!(ends.len(), 1);
        assert_eq!(ends[0]["outcome"], outcome);
        assert_eq!(ends[0]["data"]["call"], call);
    }
    assert_eq!(rows.last().unwrap()["event"], "run");
    assert_eq!(rows.last().unwrap()["state"], "end");
}
#[test]
fn library_run_does_not_invent_success_on_process_exit() {
    let dir = std::env::temp_dir().join(format!("oxblend-library-exit-{}", std::process::id()));
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "library_child", "--nocapture"])
        .env("OXBLEND_LIBRARY_TRACE_CHILD", "1")
        .env("OXBLEND_LIBRARY_TRACE_EXIT", "1")
        .env("FLATTRACE_DIR", &dir)
        .env_remove("FLATTRACE_TRACEPARENT")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(7));
    let file = std::fs::read_dir(&dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let rows: Vec<Value> = std::fs::read_to_string(file)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(rows.last().unwrap()["event"], "run");
    assert_eq!(rows.last().unwrap()["state"], "end");
    assert_eq!(rows.last().unwrap()["outcome"], "unknown");
}
#[test]
fn library_child() {
    if std::env::var_os("OXBLEND_LIBRARY_TRACE_CHILD").is_none() {
        return;
    }
    let parent = std::env::var_os("FLATTRACE_TRACEPARENT");
    let threads: Vec<_> = [0, 1]
        .into_iter()
        .map(|call| {
            std::thread::spawn(move || {
                initialize();
                initialize();
                let mut span = Span::new("consumer.call", json!({"call":call}));
                span.data(json!({"call":call}));
                span.end(if call == 0 {
                    Outcome::Ok
                } else {
                    Outcome::Cancelled
                });
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(std::env::var_os("FLATTRACE_TRACEPARENT"), parent);
    if std::env::var_os("OXBLEND_LIBRARY_TRACE_EXIT").is_some() {
        std::process::exit(7);
    }
    // Do not call an internal finish: normal process exit closes the run.
}
