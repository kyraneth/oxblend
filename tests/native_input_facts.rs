//! In-process receiving proof for both real input-open owners. The child calls
//! the Rust library directly, never the CLI; its library root has no argv inputs.
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;

#[test]
fn native_input_child() {
    let Some(source) = std::env::var_os("OXBLEND_NATIVE_INPUT_PATH") else {
        return;
    };
    let source = PathBuf::from(source);
    let route = std::env::var("OXBLEND_NATIVE_INPUT_ROUTE").unwrap();
    oxblend::tracing::initialize();
    let scope = oxblend::tracing::Span::new("api.open", json!({"route":route}));
    let result = match route.as_str() {
        "opened" => {
            let file = oxblend::BlendFile::open(&source).unwrap();
            json!({"version":file.header().version,"blocks":file.blocks().len()})
        }
        "fetch" => {
            // PackInputs and native Session share this physical-fetch owner.
            // A dry-run needs the capture/index but must publish nothing.
            let target = source.parent().unwrap().join("never-published");
            let mut options = oxblend::pack::PackOptions::default();
            options.project_root = source.parent().unwrap().to_owned();
            options.noop = true;
            let report =
                oxblend::pack::pack(std::slice::from_ref(&source), &target, &options).unwrap();
            assert!(report.complete && report.planning_complete);
            assert!(!target.exists());
            json!({"complete":report.complete,"planning_complete":report.planning_complete})
        }
        _ => panic!("unknown test route"),
    };
    scope.end(oxblend::tracing::Outcome::Ok);
    println!("NATIVE_INPUT_RESULT {result}");
}

#[test]
fn native_open_and_fetch_observe_the_same_input_once_and_off_is_noop() {
    let retained = std::env::var_os("OXBLEND_NATIVE_INPUT_EVIDENCE");
    let base = retained
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let work = base.join(format!("native-input-facts-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let source = work.join("source.blend");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/file-ops/synthetic-empty-seed.blend"),
        &source,
    )
    .unwrap();
    let mut identities = Vec::new();
    for route in ["opened", "fetch"] {
        let mut values = Vec::new();
        for (arm, level) in [("on", "1"), ("off", "0")] {
            let sink = work.join(format!("{route}-{arm}"));
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "native_input_child", "--nocapture"])
                .env("OXBLEND_NATIVE_INPUT_PATH", &source)
                .env("OXBLEND_NATIVE_INPUT_ROUTE", route)
                .env("FLATTRACE_DIR", &sink)
                .env("FLATTRACE_LEVEL", level)
                .env_remove("FLATTRACE_TRACEPARENT")
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let stdout = String::from_utf8(output.stdout).unwrap();
            let product = stdout
                .lines()
                .find_map(|line| line.split_once("NATIVE_INPUT_RESULT "))
                .map(|(_, value)| serde_json::from_str::<Value>(value).unwrap())
                .unwrap();
            values.push(product);
            if level == "0" {
                assert!(!sink.exists(), "disabled tracing must not create a sink");
                continue;
            }
            let files: Vec<_> = std::fs::read_dir(&sink)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            assert_eq!(files.len(), 1);
            let trace = &files[0];
            let rows: Vec<Value> = std::fs::read_to_string(trace)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(rows[0]["data"]["command"], "library");
            assert_eq!(rows[0]["data"]["argv"], json!([]));
            assert_eq!(rows[0]["data"]["inputs"], json!([]));
            if route == "fetch" {
                assert!(
                    rows.iter()
                        .any(|row| row["event"] == "fetch" && row["state"] == "start")
                );
            }
            let selection_path = work.join(format!("{route}.selection.json"));
            std::fs::write(
                &selection_path,
                serde_json::to_vec_pretty(&json!({"command":"library","mode":"report",
                    "scenario":"success","level":1,"calls":["open"],
                    "inputs":[source.to_str().unwrap()],"conditions":{"dry_run":true}}))
                .unwrap(),
            )
            .unwrap();
            // Optional end-to-end Python semantic gate, enabled by qualification.
            // The native ledger assertions below always run, including cargo test
            // on hosts that do not have the external reference-checker checkout.
            if std::env::var_os("OXBLEND_NATIVE_INPUT_GATE").is_some() {
                let checked = Command::new("python3")
                    .arg(
                        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("scripts/check-flattrace.py"),
                    )
                    .arg(trace)
                    .arg("--selection")
                    .arg(&selection_path)
                    .env("PYTHONDONTWRITEBYTECODE", "1")
                    .output()
                    .unwrap();
                assert!(
                    checked.status.success(),
                    "native {route} semantic gate failed: {}{}",
                    String::from_utf8_lossy(&checked.stdout),
                    String::from_utf8_lossy(&checked.stderr)
                );
            }
            let summary = rows
                .iter()
                .find(|row| {
                    row["event"] == "root.summary" && row["data"]["summary_phase"] == "final"
                })
                .unwrap();
            let inputs = summary["data"]["inputs"].as_array().unwrap();
            assert_eq!(
                inputs.len(),
                1,
                "native {route} must record exactly one capture"
            );
            assert_eq!(inputs[0]["path"], source.to_str().unwrap());
            assert_eq!(
                inputs[0]["identity_kind"],
                "stat_signature_not_content_hash"
            );
            assert!(inputs[0]["signature"].is_object());
            let opens: Vec<_> = rows
                .iter()
                .filter(|row| row["event"] == "file.open" && row["state"] == "start")
                .collect();
            assert_eq!(opens.len(), 1);
            assert_eq!(opens[0]["data"]["input"], inputs[0]);
            identities.push(inputs[0].clone());
        }
        assert_eq!(
            values[0], values[1],
            "tracing must not change native results"
        );
    }
    assert_eq!(
        identities[0], identities[1],
        "both owners use one identity format"
    );
    if retained.is_none() {
        std::fs::remove_dir_all(work).unwrap();
    }
}
