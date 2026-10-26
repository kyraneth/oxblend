use serde_json::{Value, json};
use std::process::Command;

#[test]
fn tracing_child() {
    if std::env::var_os("OXBLEND_TRACING_CHILD").is_none() {
        return;
    }
    oxblend::tracing::initialize();
    let identity = oxblend::tracing::run_identity();
    if std::env::var("FLATTRACE_LEVEL").unwrap() == "0" {
        assert!(identity.is_none());
    } else {
        assert_eq!(
            identity.as_ref().unwrap()["identity_kind"],
            "trace_root_span"
        );
    }
    oxblend::tracing::root_summary(json!({"invocation":identity}));
    let mut span = oxblend::tracing::Span::new("api.fixture", json!({"input":"fixture"}));
    span.record_error("fixture_kind", "fixture message");
    oxblend::tracing::point(
        "decision",
        json!({"item":"fixture","kind":"skip","reason":"test"}),
    );
    // Root summary must target root, even with the call scope still active.
    oxblend::tracing::root_summary(json!({"caller":"integration_test"}));
    span.end(oxblend::tracing::Outcome::Refused);
}

#[test]
fn additive_api_records_on_exact_scope_and_disabled_is_noop() {
    let root = std::env::temp_dir().join(format!("oxblend-additive-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    for (arm, level) in [("off", "0"), ("on", "1")] {
        let sink = root.join(arm);
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tracing_child", "--nocapture"])
            .env("OXBLEND_TRACING_CHILD", "1")
            .env("FLATTRACE_DIR", &sink)
            .env("FLATTRACE_LEVEL", level)
            .env_remove("FLATTRACE_TRACEPARENT")
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        if level == "0" {
            assert!(!sink.exists());
            continue;
        }
        let trace = std::fs::read_dir(&sink)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let rows: Vec<Value> = std::fs::read_to_string(trace)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        let call = rows
            .iter()
            .find(|r| r["event"] == "api.fixture" && r["state"] == "start")
            .unwrap();
        let error = rows.iter().find(|r| r["event"] == "error").unwrap();
        assert_eq!(error["span"], call["span"]);
        assert_eq!(error["data"]["kind"], "fixture_kind");
        let end = rows
            .iter()
            .find(|r| r["event"] == "api.fixture" && r["state"] == "end")
            .unwrap();
        assert_eq!(end["outcome"], "refused");
        assert_eq!(end["error"]["kind"], "fixture_kind");
        for point in rows.iter().filter(|r| r["event"] == "root.summary") {
            assert_eq!(point["span"], rows[0]["span"]);
            assert_eq!(point["data"]["invocation"]["trace"], rows[0]["trace"]);
            assert_eq!(point["data"]["invocation"]["root_span"], rows[0]["span"]);
            if point["data"].get("caller").is_some() {
                assert_eq!(point["data"]["caller"], "integration_test");
            }
        }
        assert_eq!(rows.last().unwrap()["outcome"], "unknown");
    }
    std::fs::remove_dir_all(root).unwrap();
}
