//! Direct typed-evidence bug reports versus the real CLI: identical closed
//! sanitizers and structural diagnostics, honest provenance differences, and
//! no disclosure beyond the equivalent CLI projection. This process never
//! initializes a CLI invocation ledger; the CLI side runs as a subprocess.
use oxblend::{BlendFile, Error, value::PathStep};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .args(args)
        .env_remove("FLATTRACE_DIR")
        .output()
        .unwrap()
}
struct Work(PathBuf);
impl Work {
    fn new(tag: &str) -> Self {
        let work = std::env::temp_dir().join(format!(
            "oxblend-direct-report-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&work).unwrap();
        Self(work)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}
/// Synthetic stale saved pointer: block 0 Text.name points at an
/// address no block owns. The original fixture resolves the same field.
fn invalid_dangling_pointer(target: &Path) {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dependencies/text-name.blend");
    let f = BlendFile::open(&source).unwrap();
    let mut body = std::fs::read(&source).unwrap();
    let b = &f.blocks()[0];
    let si = f.struct_index(0).unwrap();
    let field = f.schema().structs[si]
        .fields
        .iter()
        .find(|f| f.name == b"name")
        .unwrap();
    assert!(field.pointer);
    body[b.data_offset + field.offset..b.data_offset + field.offset + 8]
        .copy_from_slice(&9999u64.to_le_bytes());
    assert!(
        f.get_pointer(0, 0, &[PathStep::Field(b"name".to_vec())])
            .unwrap()
            .is_some()
    );
    std::fs::write(target, body).unwrap();
}
fn member_text(preview: &Value, name: &str) -> Value {
    let text = preview["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["path"] == name)
        .unwrap_or_else(|| panic!("member {name} missing"))["preview_text"]
        .as_str()
        .unwrap_or_else(|| panic!("member {name} is not text"));
    serde_json::from_str(text).expect("member JSON")
}
/// Alpha-rename only the engine's `kind:<SHA-256 hex>` HMAC protocol,
/// preserving repeated-token identity. Raw hashes and addresses stay exact.
fn normalize(value: &Value) -> Value {
    fn walk(value: &Value, ids: &mut std::collections::BTreeMap<String, usize>) -> Value {
        match value {
            Value::String(text) => {
                let mut result = String::new();
                let mut rest = text.as_str();
                while let Some(at) = rest.find("kind:") {
                    result.push_str(&rest[..at]);
                    let candidate = &rest[at + "kind:".len()..];
                    // SHA-256 is 32 bytes / 64 hexadecimal digits.
                    if candidate
                        .as_bytes()
                        .get(..64)
                        .is_some_and(|v| v.iter().all(u8::is_ascii_hexdigit))
                    {
                        let token = &rest[at..at + "kind:".len() + 64];
                        let next = ids.len();
                        let id = *ids.entry(token.to_owned()).or_insert(next);
                        result.push_str(&format!("BUNDLE_TOKEN_{id}"));
                        rest = &candidate[64..];
                    } else {
                        result.push_str("kind:");
                        rest = candidate;
                    }
                }
                result.push_str(rest);
                Value::String(result)
            }
            Value::Array(items) => Value::Array(items.iter().map(|v| walk(v, ids)).collect()),
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), walk(v, ids)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
    walk(value, &mut Default::default())
}
fn strip(record: &Value, keys: &[&str]) -> Value {
    let mut record = record.as_object().unwrap().clone();
    for key in keys {
        record.remove(*key);
    }
    Value::Object(record)
}
fn assert_no_path_disclosure(preview: &Value, source: &Path) {
    let raw = source.as_os_str().as_encoded_bytes();
    let hex = oxblend::value::hex(raw);
    let display = source.display().to_string();
    for member in preview["members"].as_array().unwrap() {
        let text = member["preview_text"].as_str().unwrap_or("");
        assert!(
            !text.contains(&display),
            "path disclosed in {}",
            member["path"]
        );
        assert!(
            !text.contains(&hex),
            "path hex disclosed in {}",
            member["path"]
        );
    }
}
#[test]
fn comparison_preserves_source_coordinates_hashes_and_token_aliases() {
    let token = |c: char| format!("kind:{}", c.to_string().repeat(64));
    let a = json!({"a":token('a'),"b":token('a')});
    let same = json!({"a":token('b'),"b":token('b')});
    let broken_alias = json!({"a":token('b'),"b":token('c')});
    assert_eq!(normalize(&a), normalize(&same));
    assert_ne!(normalize(&a), normalize(&broken_alias));
    let source = json!({"source":{"context":{"block_index":0,"pointer_decimal":"9999"}}});
    let mut changed = source.clone();
    changed["source"]["context"]["block_index"] = json!(1);
    assert_ne!(normalize(&source), normalize(&changed));
    let raw_hash = json!("a".repeat(64)); // Plain SHA-256 is not a HMAC token.
    assert_eq!(normalize(&raw_hash), raw_hash);
}
#[test]
fn direct_error_report_matches_cli_structural_diagnostics_exactly() {
    let work = Work::new("error");
    let input = work.0.join("invalid.blend");
    invalid_dangling_pointer(&input);
    // CLI side: real binary, real invocation ledger, saved error envelope.
    let get = run(&["pointer", input.to_str().unwrap(), "0", "[\"name\"]"]);
    assert_eq!(get.status.code(), Some(2), "{:?}", get);
    let envelope: Value = serde_json::from_slice(&get.stderr).unwrap();
    assert_eq!(envelope["error"]["kind"], "dangling_pointer");
    assert_eq!(
        envelope["diagnostic"]["schema"],
        "oxblend.failure-receipt/v1"
    );
    let saved = work.0.join("saved.json");
    std::fs::write(&saved, &get.stderr).unwrap();
    let cli = run(&[
        "report",
        "--report",
        saved.to_str().unwrap(),
        "--output",
        work.0.join("cli.zip").to_str().unwrap(),
        "--preview",
    ]);
    assert!(cli.status.success(), "{:?}", cli);
    let cli_preview: Value = serde_json::from_slice(&cli.stdout).unwrap();
    // Direct side: in-process engine error, owned snapshot, no ledger at all.
    let file = BlendFile::open(&input).unwrap();
    let error: Error = file
        .get_pointer(0, 0, &[PathStep::Field(b"name".to_vec())])
        .unwrap_err();
    assert_eq!(error.kind, "dangling_pointer");
    assert!(
        error.diagnostic_ref().is_null(),
        "no CLI ledger exists in this process"
    );
    let snapshot = error
        .source_snapshot()
        .expect("owned snapshot populated without any CLI ledger");
    assert!(!snapshot.propagations.is_empty());
    let raw_snapshot = snapshot.record_json();
    let raw_owner = envelope["diagnostic"]["failures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["context_origin"] == "owner_hook")
        .unwrap();
    for (field, value) in raw_snapshot.as_object().unwrap() {
        assert_eq!(&raw_owner[field], value, "raw source field {field}");
    }
    let direct_preview = oxblend::report::direct_error_preview(
        &work.0.join("direct.zip"),
        oxblend::report::DirectErrorEvidence {
            kind: error.kind.to_owned(),
            message: error.message.clone(),
            source_snapshot: Some(snapshot),
            diagnostics: Vec::new(),
            report: None,
        },
        false,
        None,
    )
    .unwrap();
    // Structural diagnostics compare exactly after key-derived tokens are
    // normalized; the ONLY differences are the declared provenance fields.
    let cli_source = member_text(&cli_preview, "error-source.json");
    let direct_source = member_text(&direct_preview, "error-source.json");
    let cli_owner_hook = cli_source["failures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["context_origin"] == "owner_hook")
        .cloned()
        .expect("CLI owner-hook failure");
    let direct_record = direct_source["failures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["context_origin"] == "owner_hook")
        .cloned()
        .expect("direct snapshot failure");
    assert_eq!(
        normalize(&strip(
            &cli_owner_hook,
            &["id", "input_origin", "input_binding"]
        )),
        normalize(&strip(
            &direct_record,
            &["id", "input_origin", "input_binding"]
        )),
        "structural diagnostics must match exactly"
    );
    // Expected, explicit differences.
    let cli_id = cli_owner_hook["id"]
        .as_str()
        .unwrap()
        .strip_prefix("kind:")
        .unwrap();
    assert_eq!(cli_id.len(), 64); // SHA-256 hex, not an arbitrary identity width.
    assert!(cli_id.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(direct_record["id"].is_null(), "no fabricated ledger id");
    assert_eq!(cli_owner_hook["input_origin"], "invocation_receipt");
    assert_eq!(direct_record["input_origin"], "engine_error_snapshot");
    assert_eq!(cli_owner_hook["input_binding"], "unverified");
    assert_eq!(
        direct_record["input_binding"],
        "library call; no CLI invocation binding"
    );
    assert!(
        cli_source["failures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["context_origin"] == "terminal"),
        "CLI receipt carries its terminal record"
    );
    assert_eq!(
        direct_source["failures"].as_array().unwrap().len(),
        1,
        "direct evidence has exactly the snapshot record"
    );
    // Honest binding labels in every provenance surface.
    let direct_manifest = member_text(&direct_preview, "manifest.json");
    assert_eq!(
        direct_manifest["producer_run"]["binding"],
        "library call; no CLI invocation binding"
    );
    assert!(direct_manifest["producer_run"]["run_token"].is_null());
    assert!(direct_manifest["producer_run"]["command"].is_null());
    assert_eq!(
        direct_manifest["producer_run"]["scope"],
        "direct typed library evidence; no CLI invocation"
    );
    assert!(
        direct_manifest["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().starts_with("DIRECT LIBRARY EVIDENCE"))
    );
    let cli_manifest = member_text(&cli_preview, "manifest.json");
    assert_ne!(
        cli_manifest["producer_run"]["binding"],
        "library call; no CLI invocation binding"
    );
    assert!(cli_manifest["producer_run"]["run_token"].is_string());
    // Same sanitizer path: direct reveals nothing the CLI would not.
    assert_no_path_disclosure(&cli_preview, &input);
    assert_no_path_disclosure(&direct_preview, &input);
    let cli_report_member = member_text(&cli_preview, "error-report.json");
    let direct_report_member = member_text(&direct_preview, "error-report.json");
    assert_eq!(cli_report_member["saved_kind"], "error_envelope");
    assert_eq!(direct_report_member["saved_kind"], "direct_error");
    assert_eq!(
        direct_report_member["binding"],
        "library call; no CLI invocation binding"
    );
    assert_ne!(
        cli_report_member["binding"],
        direct_report_member["binding"]
    );
    // A direct preview is a first-class preview: the reviewed destination is
    // part of consent, not a new arbitrary create-time target.
    let wrong = oxblend::report::run(&[
        "--create".into(),
        direct_preview["receipt_path"].as_str().unwrap().into(),
        "--preview-sha256".into(),
        direct_preview["receipt_sha256"].as_str().unwrap().into(),
        "--output".into(),
        work.0.join("wrong.zip").into_os_string(),
    ])
    .unwrap_err();
    assert_eq!(wrong.kind, "usage");
    assert!(!work.0.join("wrong.zip").exists());
    assert!(Path::new(direct_preview["receipt_path"].as_str().unwrap()).exists());
    let created = oxblend::report::run(&[
        "--create".into(),
        direct_preview["receipt_path"].as_str().unwrap().into(),
        "--preview-sha256".into(),
        direct_preview["receipt_sha256"].as_str().unwrap().into(),
        "--output".into(),
        work.0.join("direct.zip").into_os_string(),
    ])
    .unwrap();
    assert_eq!(created["schema"], "oxblend.report-created/v1");
    assert_eq!(
        &std::fs::read(work.0.join("direct.zip")).unwrap()[..4],
        b"PK\x03\x04"
    );
}
#[test]
fn direct_result_report_uses_the_same_closed_sanitizers_as_saved_cli_reports() {
    let work = Work::new("result");
    // Identical report JSON through both entry paths; only honest binding and
    // origin labels may differ.
    let report = json!({"schema":1,"error":{"kind":"dangling_pointer","message":"SECRET /home/alice/project.blend","block_index":2,"other_SECRET":{"token":"SECRET"},"causes":[{"kind":"SECRET_private_kind","message":"SECRET"}]},"complete":false,"SECRET":{"path_hex":"534543524554"}});
    let saved = work.0.join("saved.json");
    std::fs::write(&saved, serde_json::to_vec(&report).unwrap()).unwrap();
    let cli = run(&[
        "report",
        "--report",
        saved.to_str().unwrap(),
        "--output",
        work.0.join("cli.zip").to_str().unwrap(),
        "--preview",
    ]);
    assert!(cli.status.success(), "{:?}", cli);
    let cli_preview: Value = serde_json::from_slice(&cli.stdout).unwrap();
    let direct_preview =
        oxblend::report::direct_result_preview(&work.0.join("direct.zip"), report, false, None)
            .unwrap();
    for name in ["error-report.json", "error-source.json"] {
        let cli_member = member_text(&cli_preview, name);
        let direct_member = member_text(&direct_preview, name);
        let differing = ["input_origin", "input_binding", "context_origin"];
        for (cli, direct) in cli_member["failures"]
            .as_array()
            .unwrap()
            .iter()
            .zip(direct_member["failures"].as_array().unwrap())
        {
            assert_eq!(cli["input_origin"], "legacy_saved_report");
            assert_eq!(direct["input_origin"], "direct_result_report");
            assert_eq!(cli["input_binding"], "unverified");
            assert_eq!(
                direct["input_binding"],
                "library call; no CLI invocation binding"
            );
            assert_eq!(cli["context_origin"], "legacy_saved_report");
            assert_eq!(direct["context_origin"], "producer_report");
        }
        let cli_failures: Vec<Value> = cli_member["failures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| strip(f, &differing))
            .collect();
        let direct_failures: Vec<Value> = direct_member["failures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| strip(f, &differing))
            .collect();
        assert_eq!(
            normalize(&Value::Array(cli_failures)),
            normalize(&Value::Array(direct_failures)),
            "{name}: identical structural sanitization"
        );
        if name == "error-report.json" {
            assert_eq!(cli_member["binding"], "unverified");
            assert_eq!(
                direct_member["binding"],
                "library call; no CLI invocation binding"
            );
            assert_eq!(cli_member["saved_kind"], "legacy");
            assert_eq!(direct_member["saved_kind"], "direct_result");
            let report_projection = "report";
            assert_eq!(
                normalize(&cli_member[report_projection]),
                normalize(&direct_member[report_projection]),
                "closed report projection must be identical"
            );
        }
        let text = serde_json::to_string(&direct_member).unwrap();
        assert!(!text.contains("SECRET"));
        assert!(!text.contains("/home/alice"));
    }
    let direct_manifest = member_text(&direct_preview, "manifest.json");
    let cli_manifest = member_text(&cli_preview, "manifest.json");
    assert_eq!(
        normalize(&cli_manifest["privacy"]),
        normalize(&direct_manifest["privacy"]),
        "privacy profile and dropped-field policy are one implementation"
    );
    assert_eq!(
        direct_manifest["evidence"]["report_binding"],
        "library call; no CLI invocation binding"
    );
    for member in direct_preview["members"].as_array().unwrap() {
        let text = member["preview_text"].as_str().unwrap_or("");
        assert!(!text.contains("SECRET"));
        assert!(!text.contains("/home/alice"));
    }
    // Nothing a saved-report CLI preview would withhold appears here either:
    // the raw report stays local; only the sanitized derivative is produced.
    assert!(!work.0.join("direct.zip").exists());
}
