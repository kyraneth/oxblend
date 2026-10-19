//! Real trace/override entries over small owner-scoped serialized contracts.
use oxblend::overrides::{OverrideOptions, OverrideScope, trace_overrides};
use oxblend::trace::{PointerMode, TraceMode, TraceOptions, trace};
use std::path::PathBuf;
fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/trace-owner")
        .join(name)
}
fn options() -> TraceOptions {
    TraceOptions::new(TraceMode::PerFile, PointerMode::Strict)
}
#[test]
fn cross_id_addresses_resolve_in_trace_and_overrides() {
    let f = path("cross-id.blend");
    let t = trace(&f, options()).unwrap();
    assert!(t.complete, "{:?}", t.errors);
    assert!(t.errors.is_empty());
    assert_eq!(
        t.usages
            .iter()
            .map(|u| u.asset_path.as_slice())
            .collect::<Vec<_>>(),
        [b"//A.pc2".as_slice(), b"//B.pc2".as_slice()]
    );
    let r = trace_overrides(
        &f,
        OverrideOptions::new(OverrideScope::RootFile, PointerMode::Strict),
    )
    .unwrap();
    assert!(r.complete, "{:?}", r.errors);
    assert!(r.errors.is_empty());
    assert_eq!(r.records.len(), 2);
    let j = r.json();
    assert_eq!(
        j["records"][0]["properties"][0]["rna_path"]["target"]["hex"],
        "70726f70657274795f41"
    );
    assert_eq!(
        j["records"][1]["properties"][0]["rna_path"]["target"]["hex"],
        "70726f70657274795f42"
    );
}
#[test]
fn pack_uses_owner_scoped_field_origins_and_distinct_rewrites() {
    // Real public CLI: copying, relocation, publication and re-trace.
    // The authored filepath field matches the native FILE_MAX=1024 width.
    // A compact unique workspace keeps source and replacement paths inside
    // that real schema capacity, including longer caller TMPDIR prefixes.
    let root = std::env::temp_dir().join(format!("rbo-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let project = root.join("source");
    std::fs::create_dir(&project).unwrap();
    let source = project.join("shot");
    std::fs::create_dir(&source).unwrap();
    let input = source.join("cross-id.blend");
    std::fs::copy(path("cross-id.blend"), &input).unwrap();
    for name in ["A.pc2", "B.pc2"] {
        std::fs::write(source.join(name), name.as_bytes()).unwrap();
    }
    // Absolute references force two actual relocation writes. Noncanonical
    // relative spellings that preserve layout correctly exercise zero edits.
    let reader = oxblend::BlendFile::open(&input).unwrap();
    let mut body = std::fs::read(&input).unwrap();
    for (i, block) in reader.blocks().iter().enumerate() {
        if !reader
            .dna_type_id(i)
            .is_ok_and(|ty| ty == b"MeshCacheModifierData")
        {
            continue;
        }
        let schema = reader.schema();
        let field = schema.structs[reader.struct_index(i).unwrap()]
            .fields
            .iter()
            .find(|f| f.name == b"filepath")
            .unwrap();
        let name = if i == 1 { "A.pc2" } else { "B.pc2" };
        let path = source.join(name);
        let bytes = path.as_os_str().as_encoded_bytes();
        assert!(bytes.len() < field.size);
        let start = block.data_offset + field.offset;
        body[start..start + field.size].fill(0);
        body[start..start + bytes.len()].copy_from_slice(bytes);
    }
    drop(reader);
    std::fs::write(&input, &body).unwrap();
    let output = root.join("packed");
    let r = std::process::Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("pack")
        .arg(&input)
        .arg(&output)
        .arg("--project")
        .arg(&project)
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(
        report["rewrites"][0]["edits"].as_array().unwrap().len(),
        2,
        "{report}"
    );
    assert_eq!(report["input_path"], input.to_str().unwrap());
    assert_eq!(report["input_paths"], serde_json::json!([input]));
    assert_eq!(report["directory_path"], output.to_str().unwrap());
    for edit in report["rewrites"][0]["edits"].as_array().unwrap() {
        assert!(edit["before_path"].as_str().unwrap().ends_with(".pc2"));
        assert!(edit["after_path"].as_str().unwrap().ends_with(".pc2"));
        assert_eq!(edit["steps"][0]["field"], "filepath");
    }
    assert_eq!(std::fs::read(&input).unwrap(), body);
    let packed = output.join("shot/cross-id.blend");
    let t = trace(&packed, options()).unwrap();
    assert!(t.complete);
    assert!(t.errors.is_empty());
    assert_eq!(t.usages.len(), 2);
    for u in &t.usages {
        assert!(u.asset_path.starts_with(b"//"));
        let target = packed
            .parent()
            .unwrap()
            .join(std::str::from_utf8(&u.asset_path[2..]).unwrap());
        assert_eq!(
            std::fs::read(&target).unwrap(),
            target.file_name().unwrap().as_encoded_bytes()
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn id_and_data_namespaces_do_not_shadow_or_rescue_each_other() {
    use oxblend::{BlendFile, value::PathStep};
    let field = |name: &str| vec![PathStep::Field(name.as_bytes().to_vec())];
    for name in [
        "id-data-alias.blend",
        "pointer-kinds/typed-id-data-alias.blend",
    ] {
        let f = BlendFile::open(path(name)).unwrap();
        let mesh = f.get_pointer(0, 0, &field("data")).unwrap().unwrap();
        assert_eq!(mesh, 10);
        assert_eq!(f.blocks()[mesh].code, *b"ME\0\0");
        if name.contains("typed") {
            assert_eq!(f.get_pointer(0, 0, &field("mesh")).unwrap(), Some(mesh));
        }
        // Actual public CLI must not return local MeshCache DATA (block 1).
        let r = std::process::Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args(["pointer", path(name).to_str().unwrap(), "0", "[\"data\"]"])
            .output()
            .unwrap();
        assert!(r.status.success(), "{:?}", r);
        let j: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(j["block_index"], 10);
    }
    for name in [
        "pointer-kinds/id-only-data.blend",
        "pointer-kinds/typed-id-only-data.blend",
    ] {
        let f = BlendFile::open(path(name)).unwrap();
        assert_eq!(
            f.get_pointer(0, 0, &field("data")).unwrap_err().kind,
            "dangling_pointer"
        );
        if name.contains("typed") {
            assert_eq!(
                f.get_pointer(0, 0, &field("mesh")).unwrap_err().kind,
                "dangling_pointer"
            );
        }
    }
    for name in [
        "pointer-kinds/data-only-id.blend",
        "pointer-kinds/typed-data-only-id.blend",
    ] {
        let f = BlendFile::open(path(name)).unwrap();
        let fields = vec![
            PathStep::Field(b"id".to_vec()),
            PathStep::Field(b"override_library".to_vec()),
        ];
        assert_eq!(
            f.get_pointer(0, 0, &fields).unwrap_err().kind,
            "pointer_target_type"
        );
        if name.contains("typed") {
            assert_eq!(
                f.get_pointer(0, 0, &field("data_ptr")).unwrap_err().kind,
                "pointer_target_type"
            );
        }
        let t = trace(&path(name), options()).unwrap();
        assert!(!t.complete);
        assert!(!t.errors.is_empty());
        let r = trace_overrides(
            &path(name),
            OverrideOptions::new(OverrideScope::RootFile, PointerMode::Strict),
        )
        .unwrap();
        assert!(!r.complete);
        assert!(!r.errors.is_empty());
        let root = std::env::temp_dir().join(format!(
            "oxblend-namespace-mirror-{}-{}",
            std::process::id(),
            name.replace('/', "-")
        ));
        std::fs::create_dir(&root).unwrap();
        let input = root.join("mirror.blend");
        std::fs::copy(path(name), &input).unwrap();
        for asset in ["A.pc2", "B.pc2"] {
            std::fs::write(root.join(asset), asset).unwrap();
        }
        let output = root.join("packed");
        let pack = std::process::Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args([
                "pack",
                input.to_str().unwrap(),
                output.to_str().unwrap(),
                "--project",
                root.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!pack.status.success());
        assert!(!output.join("mirror.blend").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn material_pointer_table_uses_data_then_id_without_fallback() {
    use oxblend::{BlendFile, value::PathStep};
    let input = path("pointer-kinds/material-pointer-table.blend");
    let f = BlendFile::open(&input).unwrap();
    let table = f
        .get_pointer(0, 0, &[PathStep::Field(b"mat".to_vec())])
        .unwrap()
        .unwrap();
    assert_eq!(f.blocks()[table].code, *b"DATA");
    assert_eq!(f.blocks()[table].old_address, 0x950);
    let t = trace(
        &input,
        TraceOptions::new(TraceMode::Recursive, PointerMode::Strict),
    )
    .unwrap();
    assert!(t.complete, "{:?}", t.errors);
    assert!(t.errors.is_empty());
    assert_eq!(t.usages.len(), 2);
    let selected = trace(
        &path("pointer-kinds/selected-material-root.blend"),
        TraceOptions::new(TraceMode::Recursive, PointerMode::Strict),
    )
    .unwrap();
    assert!(!selected.complete);
    assert_eq!(selected.errors.len(), 1, "{:?}", selected.errors);
    assert!(matches!(
        selected.errors[0].kind,
        oxblend::trace::FailureKind::MissingLibrary
    ));
    assert!(
        selected.errors[0]
            .library_path
            .as_ref()
            .unwrap()
            .ends_with(b"missing-material-library.blend")
    );
}
#[test]
fn embedded_nodetree_resolves_only_to_its_owner_data() {
    use oxblend::{BlendFile, value::PathStep};
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/native-4.5.8/crypto-image.blend");
    let f = BlendFile::open(source).unwrap();
    let scene = f
        .blocks()
        .iter()
        .position(|b| b.code == *b"SC\0\0")
        .unwrap();
    let tree = f
        .get_pointer(scene, 0, &[PathStep::Field(b"nodetree".to_vec())])
        .unwrap()
        .unwrap();
    assert_eq!(f.blocks()[tree].code, *b"DATA");
    assert_eq!(f.dna_type_id(tree).unwrap(), b"bNodeTree");
    let master = f
        .get_pointer(scene, 0, &[PathStep::Field(b"master_collection".to_vec())])
        .unwrap()
        .unwrap();
    let mut master_sites = 0;
    for i in 0..f.blocks().len() {
        if !f.dna_type_id(i).is_ok_and(|ty| ty == b"LayerCollection") {
            continue;
        }
        let target = f
            .get_pointer(i, 0, &[PathStep::Field(b"collection".to_vec())])
            .unwrap()
            .unwrap();
        if target == master {
            master_sites += 1;
            assert_eq!(f.blocks()[target].code, *b"DATA");
        } else {
            assert_ne!(f.blocks()[target].code, *b"DATA");
        }
    }
    assert!(master_sites > 0);
}
#[test]
fn same_id_duplicate_refuses_and_foreign_data_is_not_fallback() {
    for (file, expected) in [
        ("same-id.blend", "duplicate_data_address"),
        ("foreign-only.blend", "dangling_pointer"),
    ] {
        let t = trace(&path(file), options()).unwrap();
        assert!(!t.complete);
        let j = t.json();
        assert!(
            j["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == expected),
            "{j}"
        );
    }
}
