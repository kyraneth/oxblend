//! Authored serialized dependency paths through the real public CLI.
#[path = "support/scratch.rs"]
mod test_scratch;
use oxblend::{BlendFile, value::PathStep};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dependencies")
        .join(name)
}
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        Self(test_scratch::scratch_dir(&std::env::temp_dir()))
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn cli(source: &Path) -> (Output, serde_json::Value) {
    let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("trace")
        .arg(source)
        .output()
        .unwrap();
    let j = serde_json::from_slice(&r.stdout).unwrap();
    (r, j)
}
fn paths(j: &serde_json::Value) -> Vec<String> {
    j["usages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["asset_path_hex"].as_str().unwrap().to_owned())
        .collect()
}
fn field(f: &BlendFile, block: usize, names: &[&str]) -> usize {
    let mut si = f.struct_index(block).unwrap();
    let mut offset = f.blocks()[block].data_offset;
    for (index, name) in names.iter().enumerate() {
        let member = f.schema().structs[si]
            .fields
            .iter()
            .find(|f| f.name == name.as_bytes())
            .unwrap();
        offset += member.offset;
        if index + 1 < names.len() {
            si = f
                .schema()
                .structs
                .iter()
                .position(|s| s.type_index == member.type_index)
                .unwrap();
        }
    }
    offset
}
#[test]
fn external_text_and_script_paths_have_physical_field_provenance() {
    for (name, path, field_name) in [
        ("text-name.blend", "//external.py", "name"),
        ("text-filepath.blend", "//external.py", "filepath"),
        ("script.blend", "//external.py", "scriptname"),
        ("shader-script.blend", "//shader.osl", "filepath"),
    ] {
        let (r, j) = cli(&fixture(name));
        assert_eq!(r.status.code(), Some(0), "{name}: {j}");
        assert_eq!(j["complete"], true);
        assert_eq!(paths(&j), [oxblend::value::hex(path.as_bytes())]);
        let u = &j["usages"][0];
        assert_eq!(u["is_sequence"], false);
        assert_eq!(
            u["path_full_field"]["name_hex"],
            serde_json::json!(oxblend::value::hex(field_name.as_bytes()))
        );
        assert_eq!(
            u["field_provenance"]["full"]["field_path_hex"],
            serde_json::json!([oxblend::value::hex(field_name.as_bytes())])
        );
    }
}
#[test]
fn script_internal_mode_and_empty_text_are_not_external_assets() {
    let work = Work::new();
    for (name, block, member, replacement) in [
        ("text-name.blend", 0, "name", 0u64.to_le_bytes().to_vec()),
        (
            "shader-script.blend",
            2,
            "mode",
            0i32.to_le_bytes().to_vec(),
        ),
    ] {
        let f = BlendFile::open(fixture(name)).unwrap();
        let mut bytes = std::fs::read(fixture(name)).unwrap();
        let start = field(&f, block, &[member]);
        bytes[start..start + replacement.len()].copy_from_slice(&replacement);
        let source = work.0.join(name);
        std::fs::write(&source, bytes).unwrap();
        let (r, j) = cli(&source);
        assert_eq!(r.status.code(), Some(0), "{j}");
        assert!(paths(&j).is_empty());
    }
}
#[test]
fn selected_library_material_uses_the_same_script_storage_contract_as_root() {
    let work = Work::new();
    let material = fixture("linked-script-material.blend");
    let original = std::fs::read(&material).unwrap();
    let f = BlendFile::open(&material).unwrap();
    std::fs::copy(
        fixture("linked-script-root.blend"),
        work.0.join("root.blend"),
    )
    .unwrap();
    let child = work.0.join("linked-script-material.blend");
    // The caller-qualified trace does not widen the generic SDK/write role.
    assert_eq!(
        f.get_pointer(2, 0, &[PathStep::Field(b"storage".to_vec())])
            .unwrap_err()
            .kind,
        "unprofiled_role"
    );
    for (mode, path, dependency) in [
        (0i32, b"C:\\".as_slice(), false),
        (1, b"//shader.osl".as_slice(), true),
        (1, b"".as_slice(), false),
    ] {
        let mut bytes = original.clone();
        let offset = field(&f, 3, &["mode"]);
        bytes[offset..offset + 4].copy_from_slice(&mode.to_le_bytes());
        let offset = field(&f, 3, &["filepath"]);
        bytes[offset..offset + 1024].fill(0);
        bytes[offset..offset + path.len()].copy_from_slice(path);
        std::fs::write(&child, bytes).unwrap();
        for root in [&child, &work.0.join("root.blend")] {
            let (r, j) = cli(root);
            assert_eq!(r.status.code(), Some(0), "{mode}: {j}");
            assert_eq!(j["complete"], true);
            assert!(j["errors"].as_array().unwrap().is_empty());
            let scripts: Vec<_> = j["usages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|u| u["block_type"] == "NodeShaderScript")
                .collect();
            assert_eq!(scripts.len(), usize::from(dependency), "{j}");
            if dependency {
                assert_eq!(scripts[0]["asset_path_hex"], oxblend::value::hex(path));
            }
        }
    }
    assert_eq!(std::fs::read(&material).unwrap(), original);
}
#[test]
fn selected_script_storage_rejects_wrong_layout_discriminator_and_mode() {
    let work = Work::new();
    let material = fixture("linked-script-material.blend");
    let f = BlendFile::open(&material).unwrap();
    let original = std::fs::read(&material).unwrap();
    std::fs::copy(
        fixture("linked-script-root.blend"),
        work.0.join("root.blend"),
    )
    .unwrap();
    for case in [
        "target-type",
        "target-size",
        "node-kind",
        "node-type",
        "unknown-mode",
    ] {
        let mut bytes = original.clone();
        let expected = match case {
            "target-type" => {
                // A valid bNode allocation, not a malformed typed extent.
                let offset = field(&f, 2, &["storage"]);
                bytes[offset..offset + 8].copy_from_slice(&300u64.to_le_bytes());
                "pointer_target_type"
            }
            "target-size" => {
                let offset = f.blocks()[3].data_offset - 20;
                bytes[offset..offset + 4].copy_from_slice(&1105u32.to_le_bytes());
                bytes.insert(f.blocks()[3].data_offset + 1104, 0);
                "pointer_extent"
            }
            "node-kind" => {
                let offset = field(&f, 2, &["idname"]);
                bytes[offset..offset + 64].fill(0);
                bytes[offset..offset + 14].copy_from_slice(b"ShaderNodeMath");
                "unprofiled_role"
            }
            "node-type" => {
                let offset = field(&f, 2, &["type"]);
                bytes[offset..offset + 2].copy_from_slice(&1i16.to_le_bytes());
                "unprofiled_role"
            }
            _ => {
                let offset = field(&f, 3, &["mode"]);
                bytes[offset..offset + 4].copy_from_slice(&2i32.to_le_bytes());
                "unprofiled_role"
            }
        };
        let child = work.0.join("linked-script-material.blend");
        std::fs::write(&child, bytes).unwrap();
        let (r, j) = cli(&work.0.join("root.blend"));
        assert_eq!(r.status.code(), Some(2), "{case}: {j}");
        assert_eq!(j["complete"], false, "{case}: {j}");
        assert_eq!(j["errors"][0]["kind"], expected, "{case}: {j}");
        // Unknown modes must also refuse through standalone root enumeration.
        if case == "unknown-mode" {
            let (r, j) = cli(&child);
            assert_eq!(r.status.code(), Some(2), "{j}");
            assert_eq!(j["errors"][0]["kind"], "unprofiled_role", "{j}");
        }
    }
}
#[test]
fn spline_ik_raw_points_are_a_caller_qualified_source_sdna_float_extent() {
    let work = Work::new();
    let source = fixture("spline-ik.blend");
    let f = BlendFile::open(&source).unwrap();
    let original = std::fs::read(&source).unwrap();
    std::fs::copy(fixture("spline-ik-root.blend"), work.0.join("root.blend")).unwrap();
    for case in ["valid", "size", "zero", "caller", "typed", "count", "code"] {
        let mut bytes = original.clone();
        match case {
            "size" => {
                let header = f.blocks()[3].data_offset - 20;
                bytes[header..header + 4].copy_from_slice(&124u32.to_le_bytes());
                let end = f.blocks()[3].data_offset + 128;
                bytes.drain(end - 4..end);
            }
            "zero" => {
                let start = field(&f, 2, &["numpoints"]);
                bytes[start..start + 2].copy_from_slice(&0i16.to_le_bytes());
            }
            "caller" => {
                let start = field(&f, 1, &["type"]);
                bytes[start..start + 2].copy_from_slice(&12i16.to_le_bytes());
            }
            "typed" => {
                // Same 128 bytes and count=1, valid typed FloatPoints layout:
                // refusal must be for non-raw storage, not a size mismatch.
                let start = f.blocks()[3].data_offset - 8;
                bytes[start..start + 4].copy_from_slice(&5u32.to_le_bytes());
            }
            "count" => {
                let start = f.blocks()[3].data_offset - 4;
                bytes[start..start + 4].copy_from_slice(&2u32.to_le_bytes());
            }
            "code" => {
                let start = f.blocks()[3].data_offset - 24;
                bytes[start..start + 4].copy_from_slice(b"TEST");
            }
            _ => (),
        }
        std::fs::write(work.0.join("spline-ik.blend"), bytes).unwrap();
        let (r, j) = cli(&work.0.join("root.blend"));
        assert_eq!(
            r.status.code(),
            Some(if case == "valid" { 0 } else { 2 }),
            "{case}: {j}"
        );
        assert_eq!(j["complete"], case == "valid", "{case}: {j}");
        if case != "valid" {
            assert!(!j["errors"].as_array().unwrap().is_empty(), "{case}: {j}");
            let kind = match case {
                "size" | "zero" | "count" => Some("pointer_extent"),
                "caller" => Some("unprofiled_role"),
                "typed" => Some("pointer_target_type"),
                _ => None,
            };
            if let Some(kind) = kind {
                assert_eq!(j["errors"][0]["kind"], kind, "{case}: {j}");
            }
        }
    }
}
#[test]
fn copied_library_unselected_pointer_paths_are_retained_and_disclosed_but_selected_refuse() {
    let work = Work::new();
    let root = work.0.join("root.blend");
    std::fs::copy(fixture("pack-unselected-root.blend"), &root).unwrap();
    let library = work.0.join("pack-unselected-library.blend");
    let f = BlendFile::open(fixture("pack-unselected-library.blend")).unwrap();
    let mut original = std::fs::read(fixture("pack-unselected-library.blend")).unwrap();
    let texture = work.0.join("texture.png");
    let script = work.0.join("external.py");
    std::fs::write(&texture, b"synthetic asset").unwrap();
    std::fs::write(&script, b"synthetic unused source").unwrap();
    // Real absolute selected path must still relocate; raw Text name is a
    // separate allocation whose pointer AND bytes must remain unchanged.
    let start = field(&f, 1, &["name"]);
    original[start..start + 1024].fill(0);
    let path = texture.to_str().unwrap().as_bytes();
    original[start..start + path.len()].copy_from_slice(path);
    let raw_start = f.blocks()[3].data_offset;
    let mut raw_path = script.to_str().unwrap().as_bytes().to_vec();
    raw_path.push(0);
    original[raw_start - 20..raw_start - 16]
        .copy_from_slice(&(raw_path.len() as u32).to_le_bytes());
    original.splice(raw_start..raw_start + f.blocks()[3].size, raw_path.clone());
    std::fs::write(&library, &original).unwrap();
    let target = work.0.join("pack");
    let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("pack")
        .arg(&root)
        .arg(&target)
        .arg("--project")
        .arg(&work.0)
        .output()
        .unwrap();
    let j: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(
        r.status.code(),
        Some(0),
        "{j} {}",
        String::from_utf8_lossy(&r.stderr)
    );
    assert_eq!(j["complete"], true);
    let warnings = j["warnings"].as_array().unwrap();
    assert_eq!(
        warnings.len(),
        1,
        "removing the explicit warning must fail: {j}"
    );
    assert_eq!(warnings[0]["kind"], "unrelocated_unselected_path");
    assert_eq!(warnings[0]["id_name"], "TXunused");
    assert_eq!(warnings[0]["field_path"], serde_json::json!(["name"]));
    assert_eq!(warnings[0]["asset_path"], script.to_str().unwrap());
    assert_eq!(
        warnings[0]["asset_path_hex"],
        oxblend::value::hex(script.to_str().unwrap().as_bytes())
    );
    assert!(
        std::fs::read_to_string(target.join("pack-info.txt"))
            .unwrap()
            .contains("Unrelocated unselected paths: 1\n")
    );
    let output = target.join("pack-unselected-library.blend");
    let after = std::fs::read(&output).unwrap();
    let ptr = field(&f, 2, &["name"]);
    assert_eq!(&after[ptr..ptr + 8], &original[ptr..ptr + 8]);
    assert_eq!(&after[raw_start..raw_start + raw_path.len()], raw_path);
    let image = field(&f, 1, &["name"]);
    assert_eq!(&after[image..image + 2], b"//");
    assert_ne!(&after[image..image + 1024], &original[image..image + 1024]);
    // Every other byte of this library is unchanged, including dormant Text.
    let mut expected = original.clone();
    expected[image..image + 1024].copy_from_slice(&after[image..image + 1024]);
    assert_eq!(after, expected);
    assert_eq!(std::fs::read(&library).unwrap(), original);
    // Select the same Text ID: the active dependency must still fail closed.
    let rf = BlendFile::open(&root).unwrap();
    let mut bytes = std::fs::read(&root).unwrap();
    let start = field(&rf, 0, &["name"]);
    bytes[start..start + 64].fill(0);
    bytes[start..start + 8].copy_from_slice(b"TXunused");
    std::fs::write(&root, bytes).unwrap();
    let selected_target = work.0.join("selected-pack");
    let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("pack")
        .arg(&root)
        .arg(&selected_target)
        .arg("--project")
        .arg(&work.0)
        .output()
        .unwrap();
    let j: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(r.status.code(), Some(2), "{j}");
    assert_eq!(j["errors"][0]["kind"], "pointer_write", "{j}");
    assert!(!selected_target.exists());
}
#[test]
fn live_linked_object_reaches_mesh_owned_material_images_and_packs_them() {
    let work = Work::new();
    let root = work.0.join("root.blend");
    std::fs::copy(fixture("mesh-material-root.blend"), &root).unwrap();
    std::fs::copy(
        fixture("mesh-material-library.blend"),
        work.0.join("mesh-material-library.blend"),
    )
    .unwrap();
    std::fs::write(work.0.join("texture.png"), b"synthetic image bytes").unwrap();
    let (r, j) = cli(&root);
    assert_eq!(r.status.code(), Some(0), "{j}");
    assert_eq!(j["complete"], true);
    let images: Vec<_> = j["usages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|u| u["block_type"] == "Image")
        .collect();
    assert_eq!(images.len(), 1, "{j}");
    assert_eq!(
        images[0]["asset_path_hex"],
        oxblend::value::hex(b"//texture.png")
    );
    let target = work.0.join("pack");
    let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("pack")
        .arg(&root)
        .arg(&target)
        .arg("--project")
        .arg(&work.0)
        .output()
        .unwrap();
    let j: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(r.status.code(), Some(0), "{j}");
    assert_eq!(j["complete"], true);
    assert_eq!(j["files"].as_array().unwrap().len(), 3, "{j}");
    assert_eq!(
        std::fs::read(target.join("texture.png")).unwrap(),
        b"synthetic image bytes"
    );
}
#[test]
fn object_data_requires_its_discriminated_id_family_count_and_sdna_extent() {
    let work = Work::new();
    std::fs::copy(
        fixture("mesh-material-root.blend"),
        work.0.join("root.blend"),
    )
    .unwrap();
    let f = BlendFile::open(fixture("mesh-material-library.blend")).unwrap();
    let original = std::fs::read(fixture("mesh-material-library.blend")).unwrap();
    for case in [
        "raw",
        "non-id",
        "mismatch",
        "count",
        "linked-count",
        "linked-extent",
        "linked-mismatch",
        "linked-missing",
        "linked-valid",
        "extent",
        "unknown",
        "null",
    ] {
        let mut bytes = original.clone();
        let data = field(&f, 0, &["data"]);
        match case {
            "raw" => bytes[data..data + 8].copy_from_slice(&400u64.to_le_bytes()),
            "non-id" => {
                // A native-looking ME code is not enough: its valid source
                // layout here is NonID, with no embedded ID at all.
                bytes[data..data + 8].copy_from_slice(&350u64.to_le_bytes());
                let header = f.blocks()[1].data_offset - 24;
                bytes[header..header + 4].copy_from_slice(b"ME\0\0");
            }
            "mismatch" => bytes[data..data + 8].copy_from_slice(&900u64.to_le_bytes()),
            "count" | "linked-count" | "extent" | "linked-extent" => {
                let payload = f.blocks()[2].data_offset;
                let length = f.blocks()[2].size;
                let is_count = matches!(case, "count" | "linked-count");
                let added = if is_count { length } else { 1 };
                bytes[payload - 20..payload - 16]
                    .copy_from_slice(&((length + added) as u32).to_le_bytes());
                if is_count {
                    bytes[payload - 4..payload].copy_from_slice(&2u32.to_le_bytes());
                }
                bytes.splice(payload + length..payload + length, vec![0; added]);
            }
            "linked-mismatch" => {
                let payload = f.blocks()[2].data_offset;
                let image = f.blocks()[7].data_offset;
                let mut body = original[image..image + f.blocks()[7].size].to_vec();
                body[..64].fill(0);
                body[..6].copy_from_slice(b"MEmesh");
                bytes[payload - 20..payload - 16]
                    .copy_from_slice(&(body.len() as u32).to_le_bytes());
                bytes[payload - 8..payload - 4].copy_from_slice(&8u32.to_le_bytes());
                bytes.splice(payload..payload + f.blocks()[2].size, body);
            }
            "linked-missing" => {
                let start = field(&f, 2, &["id", "name"]);
                bytes[start..start + 64].fill(0);
                bytes[start..start + 11].copy_from_slice(b"MEdifferent");
            }
            "linked-valid" => (),
            "unknown" => {
                let start = field(&f, 0, &["type"]);
                bytes[start..start + 2].copy_from_slice(&99i16.to_le_bytes());
            }
            _ => bytes[data..data + 8].fill(0),
        }
        if case.starts_with("linked-") {
            std::fs::write(work.0.join("mesh-data.blend"), bytes).unwrap();
            std::fs::copy(
                fixture("mesh-material-indirect.blend"),
                work.0.join("mesh-material-library.blend"),
            )
            .unwrap();
        } else {
            std::fs::write(work.0.join("mesh-material-library.blend"), bytes).unwrap();
        }
        let (r, j) = cli(&work.0.join("root.blend"));
        assert_eq!(
            r.status.code(),
            Some(if matches!(case, "null" | "linked-valid") {
                0
            } else {
                2
            }),
            "{case}: {j}"
        );
        assert_eq!(
            j["complete"],
            matches!(case, "null" | "linked-valid"),
            "{case}: {j}"
        );
        if case == "null" {
            assert!(
                !j["usages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|u| u["block_type"] == "Image")
            );
        } else if case == "linked-valid" {
            assert_eq!(
                j["usages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|u| u["block_type"] == "Image")
                    .count(),
                1,
                "{j}"
            );
        } else {
            assert!(!j["errors"].as_array().unwrap().is_empty(), "{case}: {j}");
        }
        if case == "linked-mismatch" {
            let output = work.0.join("hard-refusal");
            let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
                .arg("pack")
                .arg(work.0.join("root.blend"))
                .arg(&output)
                .arg("-p")
                .arg(&work.0)
                .output()
                .unwrap();
            let j: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
            assert_eq!(r.status.code(), Some(2));
            assert_eq!(j["planning_complete"], false);
            assert!(j["files"].as_array().unwrap().is_empty());
            assert!(!output.exists());
        }
    }
}
#[test]
fn missing_linked_ids_are_typed_incomplete_and_publish_other_existing_members() {
    let work = Work::new();
    let root = work.0.join("root.blend");
    let f = BlendFile::open(fixture("mesh-material-root.blend")).unwrap();
    let mut root_bytes = std::fs::read(fixture("mesh-material-root.blend")).unwrap();
    let fixed = |value: &[u8], size: usize| {
        let mut b = value.to_vec();
        b.resize(size, 0);
        b
    };
    let block = |code: &[u8; 4], address: u64, sdna: u32, body: Vec<u8>| {
        let mut b = code.to_vec();
        b.extend_from_slice(&(body.len() as u32).to_le_bytes());
        b.extend_from_slice(&address.to_le_bytes());
        b.extend_from_slice(&sdna.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend(body);
        b
    };
    let mut library = fixed(b"LIdata", 64);
    library.extend_from_slice(&0u64.to_le_bytes());
    library.extend(fixed(b"//mesh-data.blend", 1024));
    let mut extra = block(b"LI\0\0", 700, 3, library);
    for (address, name) in [(800, b"IMmesh".as_slice()), (850, b"IMmissing".as_slice())] {
        let mut id = fixed(name, 64);
        id.extend_from_slice(&700u64.to_le_bytes());
        extra.extend(block(b"ID\0\0", address, 1, id));
    }
    let at = f
        .blocks()
        .iter()
        .find(|b| b.code == *b"DNA1")
        .unwrap()
        .data_offset
        - 24;
    root_bytes.splice(at..at, extra);
    std::fs::write(&root, root_bytes).unwrap();
    std::fs::copy(
        fixture("mesh-material-indirect.blend"),
        work.0.join("mesh-material-library.blend"),
    )
    .unwrap();
    let f = BlendFile::open(fixture("mesh-material-library.blend")).unwrap();
    let mut bytes = std::fs::read(fixture("mesh-material-library.blend")).unwrap();
    let at = field(&f, 2, &["id", "name"]);
    bytes[at..at + 64].fill(0);
    bytes[at..at + 11].copy_from_slice(b"MEdifferent");
    std::fs::write(work.0.join("mesh-data.blend"), bytes).unwrap();
    std::fs::write(work.0.join("texture.png"), b"selected existing image").unwrap();
    let (r, j) = cli(&root);
    assert_eq!(r.status.code(), Some(2), "{j}");
    assert_eq!(j["complete"], false);
    let errors = j["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2, "{j}");
    for (name, code, owners) in [
        (
            "MEmesh",
            "ME",
            vec![root.clone(), work.0.join("mesh-material-library.blend")],
        ),
        ("IMmissing", "IM", vec![root.clone()]),
    ] {
        let e = errors.iter().find(|e| e["id_name"] == name).unwrap();
        assert_eq!(e["kind"], "missing_id");
        assert!(e["block_address"].is_null());
        assert_eq!(
            e["root_file_hex"],
            oxblend::value::hex(root.as_os_str().as_encoded_bytes())
        );
        assert_eq!(e["id_code"], code);
        assert_eq!(e["id_name_hex"], oxblend::value::hex(name.as_bytes()));
        assert_eq!(
            e["library_path"],
            work.0.join("mesh-data.blend").to_str().unwrap()
        );
        assert_eq!(
            e["referencing_owner_chain_hex"],
            serde_json::json!(
                owners
                    .iter()
                    .map(|p| oxblend::value::hex(p.as_os_str().as_encoded_bytes()))
                    .collect::<Vec<_>>()
            )
        );
    }
    assert!(
        j["usages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["block_type"] == "Image"),
        "{j}"
    );
    // List deliberately has no partial-list protocol in 1.0: fail closed.
    let listing = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("list")
        .arg(&root)
        .output()
        .unwrap();
    assert_eq!(listing.status.code(), Some(2));
    assert!(listing.stdout.is_empty());
    let listing_error: serde_json::Value = serde_json::from_slice(&listing.stderr).unwrap();
    assert_eq!(listing_error["error"]["kind"], "incomplete_trace");
    let noop_output = work.0.join("noop-pack");
    let noop = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("pack")
        .arg(&root)
        .arg(&noop_output)
        .arg("-p")
        .arg(&work.0)
        .arg("--noop")
        .output()
        .unwrap();
    let noop_report: serde_json::Value = serde_json::from_slice(&noop.stdout).unwrap();
    assert_eq!(noop.status.code(), Some(2), "{noop_report}");
    assert_eq!(noop_report["complete"], false);
    assert_eq!(noop_report["missing_link_counts"]["missing_id"], 2);
    assert_eq!(noop_report["missing_links"].as_array().unwrap().len(), 2);
    assert!(!noop_output.exists());
    let output = work.0.join("pack");
    let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("pack")
        .arg(&root)
        .arg(&output)
        .arg("-p")
        .arg(&work.0)
        .output()
        .unwrap();
    let j: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(r.status.code(), Some(2), "{j}");
    assert_eq!(j["complete"], false);
    assert_eq!(j["planning_complete"], true);
    assert_eq!(j["missing_link_counts"]["missing_id"], 2);
    assert_eq!(j["missing_link_counts"]["missing_library"], 0);
    assert_eq!(j["missing_links"].as_array().unwrap().len(), 2);
    assert_eq!(j["files"].as_array().unwrap().len(), 4, "{j}");
    assert_eq!(
        std::fs::read(output.join("texture.png")).unwrap(),
        b"selected existing image"
    );
    assert!(
        std::fs::read_to_string(output.join("pack-info.txt"))
            .unwrap()
            .contains("Missing linked IDs: 2\n")
    );
    assert_eq!(
        j["errors"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "missing_id")
            .count(),
        2
    );
}
#[test]
fn external_text_and_script_storage_dangling_pointers_refuse() {
    let work = Work::new();
    for (name, block, member) in [
        ("text-name.blend", 0, "name"),
        ("shader-script.blend", 1, "storage"),
    ] {
        let f = BlendFile::open(fixture(name)).unwrap();
        let mut bytes = std::fs::read(fixture(name)).unwrap();
        let start = field(&f, block, &[member]);
        bytes[start..start + 8].copy_from_slice(&9999u64.to_le_bytes());
        let source = work.0.join(name);
        std::fs::write(&source, bytes).unwrap();
        let (r, j) = cli(&source);
        assert_eq!(r.status.code(), Some(2));
        assert_eq!(j["complete"], false);
        assert_eq!(j["errors"][0]["kind"], "dangling_pointer", "{j}");
    }
}
#[test]
fn external_text_and_script_storage_wrong_types_refuse() {
    let work = Work::new();
    for (name, block, member, address) in [
        ("text-name.blend", 0, "name", 100u64),
        ("shader-script.blend", 1, "storage", 200),
    ] {
        let f = BlendFile::open(fixture(name)).unwrap();
        let mut bytes = std::fs::read(fixture(name)).unwrap();
        let start = field(&f, block, &[member]);
        bytes[start..start + 8].copy_from_slice(&address.to_le_bytes());
        let source = work.0.join(name);
        std::fs::write(&source, bytes).unwrap();
        let (r, j) = cli(&source);
        assert_eq!(r.status.code(), Some(2));
        assert_eq!(j["complete"], false);
        assert_eq!(j["errors"][0]["kind"], "pointer_target_type", "{j}");
    }
}
#[test]
fn bake_target_is_version_local_and_modern_targets_still_control_disk_paths() {
    for name in ["bake-old.blend", "bake-modern.blend"] {
        let (r, j) = cli(&fixture(name));
        assert_eq!(r.status.code(), Some(0), "{name}: {j}");
        assert_eq!(paths(&j), [oxblend::value::hex(b"//bake-cache")]);
        assert_eq!(j["usages"][0]["is_sequence"], true);
    }
    let work = Work::new();
    let f = BlendFile::open(fixture("bake-modern.blend")).unwrap();
    for (block, value, expected) in [(1, 1i16, false), (2, 1, false), (2, 2, true)] {
        let mut bytes = std::fs::read(fixture("bake-modern.blend")).unwrap();
        let start = field(&f, block, &["bake_target"]);
        bytes[start..start + 2].copy_from_slice(&value.to_le_bytes());
        let source = work.0.join("bake.blend");
        std::fs::write(&source, bytes).unwrap();
        let (r, j) = cli(&source);
        assert_eq!(r.status.code(), Some(0), "{j}");
        assert_eq!(!paths(&j).is_empty(), expected);
    }
}
#[test]
fn legacy_group_membership_has_structural_proof_but_no_new_sdk_admission() {
    let (r, j) = cli(&fixture("group-root.blend"));
    assert_eq!(r.status.code(), Some(0), "{j}");
    assert_eq!(j["complete"], true);
    let f = BlendFile::open(fixture("group.blend")).unwrap();
    let steps = [
        PathStep::Field(b"gobject".to_vec()),
        PathStep::Field(b"first".to_vec()),
    ];
    assert_eq!(
        f.get_pointer(0, 0, &steps).unwrap_err().kind,
        "unprofiled_role"
    );
    let work = Work::new();
    std::fs::copy(fixture("group-root.blend"), work.0.join("root.blend")).unwrap();
    let original = std::fs::read(fixture("group.blend")).unwrap();
    for (block, member, value, expected) in [
        (0, vec!["gobject", "last"], 200u64, "list_endpoints"),
        (0, vec!["gobject", "first"], 0, "list_endpoints"),
        (2, vec!["prev"], 0, "list_reciprocity"),
        (2, vec!["next"], 200, "list_cycle"),
        (1, vec!["next"], 9999, "dangling_pointer"),
        (0, vec!["gobject", "first"], 400, "pointer_target_type"),
    ] {
        let mut bytes = original.clone();
        let start = field(&f, block, &member);
        bytes[start..start + 8].copy_from_slice(&value.to_le_bytes());
        std::fs::write(work.0.join("group.blend"), bytes).unwrap();
        let (r, j) = cli(&work.0.join("root.blend"));
        assert_eq!(r.status.code(), Some(2), "{j}");
        assert_eq!(j["complete"], false);
        assert_eq!(j["errors"][0]["kind"], expected, "{member:?}: {j}");
    }
    // A structurally valid intrusive list still requires GroupObject membership.
    std::fs::copy(
        fixture("group-wrong-type.blend"),
        work.0.join("group.blend"),
    )
    .unwrap();
    let (r, j) = cli(&work.0.join("root.blend"));
    assert_eq!(r.status.code(), Some(2));
    assert_eq!(j["errors"][0]["kind"], "pointer_target_type", "{j}");
    // A typed DATA record with an inconsistent count/extent cannot enter the list.
    let mut bytes = original.clone();
    let count_offset = f.blocks()[1].data_offset - 4;
    bytes[count_offset..count_offset + 4].copy_from_slice(&2u32.to_le_bytes());
    std::fs::write(work.0.join("group.blend"), bytes).unwrap();
    let (r, j) = cli(&work.0.join("root.blend"));
    assert_eq!(r.status.code(), Some(2));
    assert_eq!(j["errors"][0]["kind"], "pointer_extent", "{j}");
}
#[test]
fn pack_reports_new_external_script_obligations_as_incomplete_when_missing() {
    let work = Work::new();
    for name in ["text-name.blend", "shader-script.blend"] {
        let source = fixture(name);
        let before = std::fs::read(&source).unwrap();
        let target = work.0.join(name);
        let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args(["pack"])
            .arg(&source)
            .arg(&target)
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(2));
        let j: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
        let error: serde_json::Value = serde_json::from_slice(&r.stderr).unwrap();
        assert_eq!(error["error"]["kind"], "incomplete_pack");
        assert_eq!(j["complete"], false);
        assert_eq!(j["missing"].as_array().unwrap().len(), 1);
        assert_eq!(
            j["missing"][0]["block_type"],
            if name.starts_with("text") {
                "Text"
            } else {
                "NodeShaderScript"
            }
        );
        assert_eq!(std::fs::read(&source).unwrap(), before);
    }
}
#[test]
fn real_library_headers_emit_at_root_but_unselected_child_headers_do_not() {
    let (r, j) = cli(&fixture("unused-library-root.blend"));
    assert_eq!(r.status.code(), Some(0), "{j}");
    assert_eq!(paths(&j), [oxblend::value::hex(b"//unused.blend")]);
    let work = Work::new();
    std::fs::copy(fixture("group-root.blend"), work.0.join("root.blend")).unwrap();
    std::fs::copy(
        fixture("unused-library-child.blend"),
        work.0.join("group.blend"),
    )
    .unwrap();
    let (r, j) = cli(&work.0.join("root.blend"));
    assert_eq!(r.status.code(), Some(0), "{j}");
    assert_eq!(j["complete"], true);
    assert_eq!(paths(&j), [oxblend::value::hex(b"//group.blend")]);
}
#[test]
fn append_origin_weak_references_are_metadata_not_dependencies() {
    for name in ["weak-drive.blend", "weak-copybuffer.blend"] {
        let (r, j) = cli(&fixture(name));
        assert_eq!(r.status.code(), Some(0), "{j}");
        assert_eq!(j["complete"], true);
        assert!(paths(&j).is_empty());
        let f = BlendFile::open(fixture(name)).unwrap();
        let p = [
            PathStep::Field(b"id".to_vec()),
            PathStep::Field(b"library_weak_reference".to_vec()),
        ];
        assert_eq!(f.get_pointer(0, 0, &p).unwrap(), Some(1));
    }
}
