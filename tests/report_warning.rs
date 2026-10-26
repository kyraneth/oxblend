//! Real warning-bearing producer input, shared CLI/library report privacy.
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oxblend-report-warning-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        for sub in ["project", "assets"] {
            std::fs::create_dir(path.join(sub)).unwrap();
        }
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/library-null");
        for name in ["normal-root.blend", "normal.blend"] {
            std::fs::copy(fixtures.join(name), path.join("project").join(name)).unwrap();
        }
        std::fs::copy(
            fixtures.join("image.blend"),
            path.join("assets/image.blend"),
        )
        .unwrap();
        let png = "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c63f8cfc0f01f00050001ff89993d1d0000000049454e44ae426082";
        let bytes = (0..png.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&png[i..i + 2], 16).unwrap())
            .collect::<Vec<_>>();
        std::fs::write(path.join("assets/pixel.png"), bytes).unwrap();
        Self(path)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .args(args)
        .env_remove("FLATTRACE_DIR")
        .output()
        .unwrap()
}
fn member(preview: &Value, name: &str) -> Value {
    let text = preview["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["path"] == name)
        .unwrap()["preview_text"]
        .as_str()
        .unwrap();
    serde_json::from_str(text).unwrap()
}
#[test]
fn three_library_null_warnings_survive_cli_and_library_report_privacy() {
    let work = Work::new();
    let input = work.0.join("project/normal-root.blend");
    let produced = cli(&["trace", input.to_str().unwrap()]);
    assert!(produced.status.success(), "{produced:?}");
    let mut raw: Value = serde_json::from_slice(&produced.stdout).unwrap();
    assert_eq!(raw["complete"], true);
    assert_eq!(raw["warnings"].as_array().unwrap().len(), 3);
    for warning in raw["warnings"].as_array().unwrap() {
        assert_eq!(warning["kind"], "library_null_pointer");
    }
    // Warning-bearing input must be tested for privacy, not just acceptance.
    raw["warnings"][0]["message"] = json!("PRIVATE_WARNING_MESSAGE /home/private/source.blend");
    raw["warnings"][0]["PRIVATE_WARNING_KEY"] = json!({"password":"PRIVATE_WARNING_VALUE"});
    let saved = work.0.join("saved.json");
    std::fs::write(&saved, serde_json::to_vec(&raw).unwrap()).unwrap();
    let result = cli(&[
        "report",
        "--report",
        saved.to_str().unwrap(),
        "--output",
        work.0.join("cli.zip").to_str().unwrap(),
        "--preview",
    ]);
    assert!(result.status.success(), "{result:?}");
    let cli_preview: Value = serde_json::from_slice(&result.stdout).unwrap();
    let native = oxblend::report::direct_result_preview(
        &work.0.join("native.zip"),
        raw.clone(),
        false,
        None,
    )
    .unwrap();
    for preview in [&cli_preview, &native] {
        let source = member(preview, "error-source.json");
        let warnings = source["warnings"]
            .as_array()
            .expect("sanitized warning records");
        assert_eq!(warnings.len(), 3);
        assert_eq!(source["failures"], json!([]));
        assert_eq!(source["status"], "partial");
        for (index, (warning, original)) in warnings
            .iter()
            .zip(raw["warnings"].as_array().unwrap())
            .enumerate()
        {
            assert_eq!(warning["kind"], "library_null_pointer");
            assert_eq!(warning["message_redacted"], true);
            assert_eq!(warning["source_pointer"], format!("/warnings/{index}"));
            assert_eq!(
                warning["context"]["decoded_file_offset"],
                original["offset"]
            );
            assert_eq!(
                warning["context"]["pointer_decimal"],
                original["pointer_address"]
            );
            assert!(warning["context"]["file_path_hex"].is_null());
        }
        let projected = member(preview, "error-report.json");
        assert_eq!(projected["warnings"].as_array().unwrap().len(), 3);
        assert_eq!(projected["report"]["warnings"].as_array().unwrap().len(), 3);
        for item in preview["members"].as_array().unwrap() {
            let text = item["preview_text"].as_str().unwrap_or("");
            assert!(!text.contains("PRIVATE_WARNING"), "{}", item["path"]);
            assert!(!text.contains("/home/private"), "{}", item["path"]);
            assert!(!text.contains(input.to_str().unwrap()), "{}", item["path"]);
            assert!(
                !text.contains(&oxblend::value::hex(input.as_os_str().as_encoded_bytes())),
                "{}",
                item["path"]
            );
        }
    }
    // Empty warnings must not add fields to the old non-warning derivative.
    raw["warnings"] = json!([]);
    let empty = oxblend::report::direct_result_preview(&work.0.join("empty.zip"), raw, false, None)
        .unwrap();
    assert!(
        member(&empty, "error-source.json")
            .get("warnings")
            .is_none()
    );
    assert!(
        member(&empty, "error-report.json")
            .get("warnings")
            .is_none()
    );
    assert!(
        member(&empty, "error-report.json")["report"]
            .get("warnings")
            .is_none()
    );
}
