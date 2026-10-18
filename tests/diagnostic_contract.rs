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
