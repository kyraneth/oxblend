use oxblend::{
    BlendFile,
    trace::{PointerMode, TraceMode, TraceOptions, trace},
};
use std::{path::PathBuf, process::Command};

#[test]
fn empty_image_path_is_absent_but_populated_image_is_traced() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/native-4.5.8/build-kit/constructors.blend");
    let options = || TraceOptions::new(TraceMode::PerFile, PointerMode::Strict);
    let initial = trace(&source, options()).unwrap();
    assert!(initial.complete);
    assert!(
        !initial
            .usages
            .iter()
            .any(|u| u.block_code == b"IM" && u.asset_path.is_empty())
    );
    let f = BlendFile::open(&source).unwrap();
    let i = f
        .blocks()
        .iter()
        .position(|b| b.code == *b"IM\0\0")
        .unwrap();
    let field = f.schema().structs[f.struct_index(i).unwrap()]
        .fields
        .iter()
        .find(|p| p.name == b"filepath" || p.name == b"name")
        .unwrap();
    let mut body = f.read_decompressed_bytes().unwrap();
    let offset = f.blocks()[i].data_offset + field.offset;
    let value = b"//populated-image.png";
    body[offset..offset + field.size].fill(0);
    body[offset..offset + value.len()].copy_from_slice(value);
    let path = std::env::temp_dir().join(format!(
        "oxblend-populated-image-{}.blend",
        std::process::id()
    ));
    std::fs::write(&path, body).unwrap();
    let report = trace(&path, options()).unwrap();
    assert!(report.complete, "{:?}", report.errors);
    assert!(
        report
            .usages
            .iter()
            .any(|u| u.block_code == b"IM" && u.asset_path == value)
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn summary_missing_fields_are_typed_warnings_and_id_families_are_named() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dependencies");
    for name in ["mesh-material-root.blend", "mesh-material-library.blend"] {
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("summary")
            .arg(root.join(name))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let j: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        if name == "mesh-material-root.blend" {
            assert_eq!(j["warnings"][0]["kind"], "missing_field");
            assert_eq!(j["scenes"][0]["frame_end"], serde_json::Value::Null);
            assert_eq!(j["collections"], serde_json::json!(["GRlive"]));
        } else {
            assert_eq!(j["materials"], serde_json::json!(["MAmesh"]));
            assert_eq!(j["meshes"], serde_json::json!(["MEmesh"]));
            assert_eq!(j["images"], serde_json::json!(["IMmesh"]));
        }
    }
}

#[test]
fn missing_references_print_incomplete_list_but_do_not_redefine_trace_completeness() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dependencies");
    let work = std::env::temp_dir().join(format!("oxblend-missing-list-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    let orphan = work.join("root.blend");
    std::fs::copy(root.join("mesh-material-root.blend"), &orphan).unwrap();
    for (file, missing, kind) in [
        (
            root.join("text-filepath.blend"),
            "external.py",
            "missing_asset",
        ),
        (orphan, "mesh-material-library.blend", "missing_library"),
    ] {
        for flags in [vec!["--json"], vec!["--json", "--full-info"], vec![]] {
            let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
                .arg("list")
                .arg(&file)
                .args(&flags)
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(2));
            if flags.contains(&"--json") {
                let listing: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
                assert_eq!(listing["complete"], false);
                let members: Vec<_> = listing["files"]
                    .as_object()
                    .unwrap()
                    .values()
                    .flat_map(|v| v.as_array().unwrap())
                    .collect();
                assert!(members.iter().any(
                    |m| m["missing"] == true && m["path"].as_str().unwrap().ends_with(missing)
                ));
            } else {
                let text = String::from_utf8(out.stdout).unwrap();
                assert!(text.contains(&format!("{missing} [missing]")));
            }
            let j: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
            assert_eq!(j["error"]["kind"], kind);
            assert!(j["error"]["message"].as_str().unwrap().contains(missing));
        }
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("trace")
            .arg(&file)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(if kind == "missing_asset" { 0 } else { 2 })
        );
        let j: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(j["complete"], kind == "missing_asset");
        assert_eq!(j["missing_path_count"], 1);
        assert_eq!(j["referenced_paths_exist"], false);
        assert_eq!(j["usages"][0]["exists"], false);
        assert!(
            j["usages"][0]["resolved_path"]
                .as_str()
                .unwrap()
                .ends_with(missing)
        );
        assert!(
            j["usages"][0]["asset_path"]
                .as_str()
                .unwrap()
                .ends_with(missing)
        );
        if kind == "missing_library" {
            assert!(
                j["errors"][0]["library_path"]
                    .as_str()
                    .unwrap()
                    .ends_with(missing)
            );
            assert_eq!(j["errors"][0]["field_path"], serde_json::json!(["name"]));
        }
    }
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn missing_list_retains_existing_members_and_corrupt_libraries_still_refuse() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dependencies");
    let work = std::env::temp_dir().join(format!("oxblend-mixed-list-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    for name in ["mesh-material-root.blend", "mesh-material-library.blend"] {
        std::fs::copy(fixtures.join(name), work.join(name)).unwrap();
    }
    let source = work.join("mesh-material-root.blend");
    for flags in [
        vec![],
        vec!["--sha256"],
        vec!["--json"],
        vec!["--json", "--full-info"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("list")
            .arg(&source)
            .args(&flags)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        if flags.contains(&"--json") {
            let listing: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(listing["complete"], false);
            let members: Vec<_> = listing["files"]
                .as_object()
                .unwrap()
                .values()
                .flat_map(|v| v.as_array().unwrap())
                .collect();
            assert!(members.iter().any(|m| {
                m["path"]
                    .as_str()
                    .unwrap()
                    .ends_with("mesh-material-library.blend")
                    && m["missing"] == false
            }));
            assert!(
                members
                    .iter()
                    .any(|m| m["path"].as_str().unwrap().ends_with("texture.png")
                        && m["missing"] == true)
            );
        } else {
            let text = String::from_utf8(out.stdout).unwrap();
            assert!(text.contains("mesh-material-library.blend"));
            assert!(text.contains("texture.png [missing]"));
        }
    }
    std::fs::write(work.join("mesh-material-library.blend"), b"corrupt").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("list")
        .arg(&source)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn byte_identical_diff_proves_equality_without_semantic_anchoring() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dependencies");
    let copy =
        std::env::temp_dir().join(format!("oxblend-byte-equal-{}.blend", std::process::id()));
    for name in ["mesh-material-library.blend", "bake-old.blend"] {
        let source = root.join(name);
        std::fs::copy(&source, &copy).unwrap();
        for right in [&source, &copy] {
            let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
                .arg("diff")
                .arg(&source)
                .arg(right)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let j: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(j["equal"], true);
            assert_eq!(j["complete"], true);
            assert_eq!(j["difference_count"], 0);
            assert_eq!(j["differences"], serde_json::json!([]));
        }
    }
    std::fs::remove_file(copy).unwrap();
}

#[test]
fn diff_returned_errors_still_emit_an_inconclusive_v1_report() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dependencies");
    for right in ["mesh-material-root.blend", "absent.blend"] {
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("diff")
            .arg(root.join("mesh-material-library.blend"))
            .arg(root.join(right))
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let error: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(report["schema"], "oxblend.diff/v1");
        assert_eq!(report["equal"], serde_json::Value::Null);
        assert_eq!(report["complete"], false);
        assert_eq!(
            report["difference_count"],
            report["differences"].as_array().unwrap().len()
        );
        // CLI guidance is additive; the stable kind/message must still match
        // the incomplete report rather than requiring identical object keys.
        assert_eq!(report["errors"][0]["kind"], error["error"]["kind"]);
        assert_eq!(report["errors"][0]["message"], error["error"]["message"]);
        if right == "mesh-material-root.blend" {
            assert_eq!(report["errors"][0]["kind"], "missing_field");
        }
    }
}

#[test]
fn facts_failures_name_the_file_and_field_in_words() {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dependencies/mesh-material-root.blend");
    for (command, field) in [
        ("source-facts", "master_collection"),
        ("library-facts", "children"),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg(command)
            .arg(&file)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        let j: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
        let message = j["error"]["message"].as_str().unwrap();
        assert!(
            message.contains(field) && message.contains("mesh-material-root.blend"),
            "{message}"
        );
        assert!(!message.contains("Field(["));
        assert!(message.contains("use trace for cross-file dependencies"));
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn id_name_rewrite_refusal_is_worded_and_path_rewrite_aliases_are_readable() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dependencies/mesh-material-library.blend");
    let work = std::env::temp_dir().join(format!("oxblend-rewrite-text-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    for (label, block, path, expected, replacement) in [
        (
            "rename",
            0,
            serde_json::json!([{"field_hex":"6964"},{"field_hex":"6e616d65"}]),
            "4f426d657368",
            "4f426865726f",
        ),
        (
            "filepath",
            7,
            serde_json::json!([{"field_hex":"6e616d65"}]),
            "2f2f746578747572652e706e67",
            "2f2f6e65772e706e67",
        ),
    ] {
        let spec = work.join(format!("{label}.json"));
        let target = work.join(format!("{label}.blend"));
        std::fs::write(&spec, serde_json::to_vec(&serde_json::json!({"schema":1,"edits":[{"block_index":block,"element":0,"path":path,"expected_path_hex":expected,"replacement_path_hex":replacement}]})).unwrap()).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("rewrite")
            .arg(&source)
            .arg("--output")
            .arg(&target)
            .arg("--edits")
            .arg(spec)
            .output()
            .unwrap();
        let j: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        if label == "rename" {
            assert_eq!(out.status.code(), Some(2));
            assert_eq!(j["errors"][0]["kind"], "unsupported_edit");
            assert!(
                j["errors"][0]["message"]
                    .as_str()
                    .unwrap()
                    .contains("ID name fields are not editable")
            );
            assert!(!target.exists());
        } else {
            assert!(out.status.success());
            assert_eq!(j["input_path"], source.to_str().unwrap());
            assert_eq!(j["output_path"], target.to_str().unwrap());
            assert_eq!(j["edits"][0]["before_path"], "//texture.png");
            assert_eq!(j["edits"][0]["after_path"], "//new.png");
            assert_eq!(j["edits"][0]["steps"][0]["field"], "name");
        }
    }
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn zero_element_by_value_layout_refuses_before_asset_walk() {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audit/zero-by-value.blend");
    let result = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("trace")
        .arg(file)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    let err: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(err["error"]["kind"], "invalid_sdna");
    assert!(result.stdout.is_empty());
}
