//! Benchmark probe, not a production CLI/API choice.
//! Measure the complete trace invocation and its reported dependency fields.
//! Retain input and binary hashes separately when comparing measurements.
use oxblend::trace::{PointerMode, TraceMode, TraceOptions, trace};
use serde_json::json;
fn main() {
    let a: Vec<_> = std::env::args_os().collect();
    let mode = if a[2] == "per-file" {
        TraceMode::PerFile
    } else {
        TraceMode::Recursive
    };
    let pointers = if a[3] == "strict" {
        PointerMode::Strict
    } else {
        PointerMode::NonStrict
    };
    let report = match trace(
        std::path::Path::new(&a[1]),
        TraceOptions::new(mode, pointers),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", json!({"error":{"kind":e.kind,"message":e.message}}));
            std::process::exit(2)
        }
    };
    // Report the full dependency identity, not a reduced asset-path query.
    let keys = [
        "owner_file",
        "block_code_hex",
        "block_address",
        "block_offset",
        "block_type",
        "block_name_hex",
        "asset_path_hex",
        "is_sequence",
        "path_full_field",
        "path_dir_field",
        "path_base_field",
    ];
    let usages: Vec<_> = report
        .usages
        .iter()
        .map(|u| {
            let rich = u.json();
            let mut result = serde_json::Map::new();
            for key in keys {
                result.insert(key.to_owned(), rich[key].clone());
            }
            serde_json::Value::Object(result)
        })
        .collect();
    let value = json!({"schema":1,"complete":report.complete,"mode":a[2].to_str(),"pointer_mode":if pointers==PointerMode::Strict{"strict-library"}else{"non-strict"},"corrections":true,"input":a[1].to_str(),"input_sha256":null,"usages":usages,"errors":report.errors.iter().map(|e|e.json()).collect::<Vec<_>>()});
    use std::io::Write;
    let encoded = serde_json::to_vec(&value).unwrap();
    let mut out = std::io::stdout().lock();
    out.write_all(&encoded)
        .and_then(|_| out.write_all(b"\n"))
        .and_then(|_| out.flush())
        .unwrap();
    if !report.complete {
        std::process::exit(2)
    }
}
