//! trusted fixture-spec to full synchronous API, never calls CLI dispatch.
//! Invalid fixture JSON is a harness failure, not a product parser qualification.
use oxblend::edit::{EditOptions, PathRewrite, rewrite_paths};
use oxblend::value::PathStep;
use std::path::Path;
fn hex(v: &serde_json::Value) -> Vec<u8> {
    let s = v.as_str().expect("fixture hex string");
    assert!(s.len().is_multiple_of(2) && s.bytes().all(|b| b.is_ascii_hexdigit()));
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| {
            let d = |b: u8| {
                if b.is_ascii_digit() {
                    b - b'0'
                } else {
                    b.to_ascii_lowercase() - b'a' + 10
                }
            };
            (d(p[0]) << 4) | d(p[1])
        })
        .collect()
}
fn main() {
    let args = std::env::args_os().collect::<Vec<_>>();
    assert!(
        args.len() == 4 || args.len() == 5,
        "INPUT NEW_OUTPUT TRUSTED_SPEC [none|end|each-file]"
    );
    let options = EditOptions::new(
        args.get(4)
            .map(|v| {
                v.to_str()
                    .expect("durability ASCII")
                    .parse()
                    .expect("durability level")
            })
            .unwrap_or_default(),
    );
    let spec: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&args[3]).expect("fixture read"))
            .expect("fixture JSON");
    let edits = spec["edits"]
        .as_array()
        .expect("fixture edits")
        .iter()
        .map(|e| PathRewrite {
            block_index: usize::try_from(e["block_index"].as_u64().expect("fixture block index"))
                .expect("usize"),
            element: usize::try_from(e["element"].as_u64().expect("fixture element"))
                .expect("usize"),
            steps: e["path"]
                .as_array()
                .expect("fixture path")
                .iter()
                .map(|s| {
                    if let Some(v) = s.get("field_hex") {
                        PathStep::Field(hex(v))
                    } else {
                        PathStep::Index(
                            usize::try_from(s["index"].as_u64().expect("fixture index"))
                                .expect("usize"),
                        )
                    }
                })
                .collect(),
            expected_path: hex(&e["expected_path_hex"]),
            replacement_path: hex(&e["replacement_path_hex"]),
        })
        .collect::<Vec<_>>();
    match rewrite_paths(Path::new(&args[1]), Path::new(&args[2]), &edits, &options) {
        Ok(report) => {
            println!("{}", report.json());
            if !report.complete {
                std::process::exit(2);
            }
        }
        Err(e) => {
            eprintln!(
                "{}",
                serde_json::json!({"error":{"kind":e.kind,"message":e.message}})
            );
            std::process::exit(2);
        }
    }
}
