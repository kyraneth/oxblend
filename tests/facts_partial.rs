//! #25 regressions: value-taking blocks option and per-site partial fact closure.
#[path = "support/scratch.rs"]
mod scratch;
use oxblend::{BlendFile, value::PathStep};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn fixture(dir: &Path) -> PathBuf {
    // Independently authored saved layout: two Scene collection members, each
    // with a proved null parent. No native fixture assumptions or vacuous facts.
    let definitions = [
        ("ID", 64u16, vec![("char", "name[64]")]),
        ("ListBase", 16, vec![("void", "*first"), ("void", "*last")]),
        ("Object", 72, vec![("ID", "id"), ("Object", "*parent")]),
        (
            "Collection",
            96,
            vec![
                ("ID", "id"),
                ("ListBase", "children"),
                ("ListBase", "gobject"),
            ],
        ),
        (
            "CollectionObject",
            16,
            vec![("CollectionObject", "*next"), ("Object", "*ob")],
        ),
        (
            "Scene",
            72,
            vec![("ID", "id"), ("Collection", "*master_collection")],
        ),
        ("FileGlobal", 8, vec![("Scene", "*curscene")]),
    ];
    let mut types = vec!["char", "void"];
    types.extend(definitions.iter().map(|d| d.0));
    let names = definitions
        .iter()
        .flat_map(|d| d.2.iter().map(|f| f.1))
        .collect::<Vec<_>>();
    let mut dna = b"SDNANAME".to_vec();
    dna.extend((names.len() as u32).to_le_bytes());
    for name in &names {
        dna.extend(name.as_bytes());
        dna.push(0);
    }
    while !dna.len().is_multiple_of(4) {
        dna.push(0);
    }
    dna.extend(b"TYPE");
    dna.extend((types.len() as u32).to_le_bytes());
    for ty in &types {
        dna.extend(ty.as_bytes());
        dna.push(0);
    }
    while !dna.len().is_multiple_of(4) {
        dna.push(0);
    }
    dna.extend(b"TLEN");
    for size in [1u16, 0].into_iter().chain(definitions.iter().map(|d| d.1)) {
        dna.extend(size.to_le_bytes());
    }
    while !dna.len().is_multiple_of(4) {
        dna.push(0);
    }
    dna.extend(b"STRC");
    dna.extend((definitions.len() as u32).to_le_bytes());
    let mut name_index = 0u16;
    for (ty, _, fields) in &definitions {
        dna.extend((types.iter().position(|t| t == ty).unwrap() as u16).to_le_bytes());
        dna.extend((fields.len() as u16).to_le_bytes());
        for (ty, _) in fields {
            dna.extend((types.iter().position(|t| t == ty).unwrap() as u16).to_le_bytes());
            dna.extend(name_index.to_le_bytes());
            name_index += 1;
        }
    }
    let named = |name: &[u8], pointers: &[u64]| {
        let mut b = vec![0; 64];
        b[..name.len()].copy_from_slice(name);
        for p in pointers {
            b.extend(p.to_le_bytes());
        }
        b
    };
    let mut body = b"BLENDER-v405".to_vec();
    for (code, address, sdna, data) in [
        (*b"GLOB", 0u64, 6u32, 0x100u64.to_le_bytes().to_vec()),
        (*b"SC\0\0", 0x100, 5, named(b"SCScene", &[0x200])),
        (
            *b"DATA",
            0x200,
            3,
            named(b"GRCollection", &[0, 0, 0x300, 0x600]),
        ),
        (
            *b"DATA",
            0x300,
            4,
            [0x600u64.to_le_bytes(), 0x400u64.to_le_bytes()].concat(),
        ),
        (
            *b"DATA",
            0x600,
            4,
            [0u64.to_le_bytes(), 0x500u64.to_le_bytes()].concat(),
        ),
        (*b"OB\0\0", 0x400, 2, named(b"OBBroken", &[0])),
        (*b"OB\0\0", 0x500, 2, named(b"OBIndependent", &[0])),
        (*b"DNA1", 0, 0, dna),
        (*b"ENDB", 0, 0, vec![]),
    ] {
        body.extend(code);
        body.extend((data.len() as u32).to_le_bytes());
        body.extend(address.to_le_bytes());
        body.extend(sdna.to_le_bytes());
        body.extend((if code == *b"ENDB" { 0u32 } else { 1u32 }).to_le_bytes());
        body.extend(data);
    }
    let path = dir.join("facts-seed.blend");
    std::fs::write(&path, body).unwrap();
    path
}
#[test]
fn blocks_address_separate_and_equals_forms_have_identical_results() {
    let work = scratch::scratch_dir(&std::env::temp_dir());
    let input = fixture(&work);
    let file = BlendFile::open(&input).unwrap();
    let address = file
        .blocks()
        .iter()
        .find(|b| b.code == *b"OB\0\0")
        .unwrap()
        .old_address;
    let mut expected = None;
    for spelling in [address.to_string(), format!("0x{address:x}")] {
        for separate in [false, true] {
            let mut c = Command::new(env!("CARGO_BIN_EXE_oxblend"));
            c.arg("blocks").arg(&input);
            if separate {
                c.args(["--address", &spelling]);
            } else {
                c.arg(format!("--address={spelling}"));
            }
            let out = c.output().unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            if let Some(expected) = &expected {
                assert_eq!(&out.stdout, expected);
            } else {
                expected = Some(out.stdout);
            }
        }
    }
    for value in [None, Some("nonsense")] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        c.arg("blocks").arg(&input).arg("--address");
        if let Some(value) = value {
            c.arg(value);
        }
        let out = c.output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        let err: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(err["error"]["kind"], "usage");
    }
    std::fs::remove_dir_all(work).unwrap();
}
#[test]
fn dangling_parent_marks_its_site_and_keeps_independent_facts() {
    let work = scratch::scratch_dir(&std::env::temp_dir());
    let original = fixture(&work);
    let reader = BlendFile::open(&original).unwrap();
    let object = reader
        .blocks()
        .iter()
        .position(|b| b.code == *b"OB\0\0")
        .unwrap();
    let field = reader.schema().structs[reader.struct_index(object).unwrap()]
        .fields
        .iter()
        .find(|f| f.name == b"parent")
        .unwrap();
    assert!(field.pointer);
    let name = reader.id_name(object).unwrap().unwrap();
    let saved = 0x700e3a1a4408u64; // The independently reported #25 stale Object.parent value.
    assert!(reader.dereference(saved).is_err());
    let mut bytes = reader.read_decompressed_bytes().unwrap();
    let offset = reader.blocks()[object].data_offset + field.offset;
    bytes[offset..offset + 8].copy_from_slice(&saved.to_le_bytes());
    let input = work.join("dangling-parent.blend");
    std::fs::write(&input, bytes).unwrap();
    for command in ["source-facts", "library-facts"] {
        let invoke = |path: &Path| {
            Command::new(env!("CARGO_BIN_EXE_oxblend"))
                .arg(command)
                .arg(path)
                .output()
                .unwrap()
        };
        let baseline = invoke(&original);
        assert!(
            baseline.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&baseline.stderr)
        );
        let baseline: Value = serde_json::from_slice(&baseline.stdout).unwrap();
        let out = invoke(&input);
        assert_eq!(out.status.code(), Some(2));
        let report: Value =
            serde_json::from_slice(&out.stdout).expect("partial facts must reach stdout");
        assert_eq!(report["complete"], false);
        let errors = report["errors"].as_array().unwrap();
        assert_eq!(errors.len(), 1, "{report}");
        let e = &errors[0];
        assert_eq!(e["kind"], "dangling_pointer");
        assert_eq!(e["site"]["block_index"], object);
        assert_eq!(e["site"]["block_name_hex"], oxblend::value::hex(&name));
        assert_eq!(e["site"]["field_path"], serde_json::json!(["parent"]));
        assert_eq!(e["site"]["pointer_value_decimal"], saved.to_string());
        assert!(e["message"].as_str().unwrap().contains("Object.parent"));
        let address = reader.blocks()[object].old_address.to_string();
        assert_eq!(
            report["facts"]["blocks"][&address]["pointers"]["parent"]["status"],
            "unknown"
        );
        // Unknown ancestry cannot be reported as a proved root.
        assert!(report["facts"]["blocks"][&address].get("root").is_none());
        let before = baseline["facts"]["blocks"].as_object().unwrap();
        let after = report["facts"]["blocks"].as_object().unwrap();
        for (addr, b) in before {
            assert_eq!(after[addr]["name"], b["name"]);
            if addr != &address {
                assert_eq!(after[addr], *b, "independent fact changed: {addr}");
            }
        }
        assert_eq!(after.len(), before.len());
        let direct = if command == "source-facts" {
            oxblend::facts::source_facts(&BlendFile::open(&input).unwrap())
        } else {
            oxblend::facts::library_facts(&BlendFile::open(&input).unwrap())
        }
        .unwrap();
        assert_eq!(direct, report["facts"]);
        let err: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(err["error"]["kind"], "incomplete_facts");
        assert_eq!(err["error"]["next_command"]["argv"][1], "trace");
        // A valid edge to the broken ancestor does not prove the child's root.
        let child = object + 1;
        let mut descendant = std::fs::read(&input).unwrap();
        let offset = reader.blocks()[child].data_offset + field.offset;
        descendant[offset..offset + 8]
            .copy_from_slice(&reader.blocks()[object].old_address.to_le_bytes());
        let descendant_path = work.join("descendant.blend");
        std::fs::write(&descendant_path, descendant).unwrap();
        let out = invoke(&descendant_path);
        assert_eq!(out.status.code(), Some(2));
        let report: Value = serde_json::from_slice(&out.stdout).unwrap();
        let child_address = reader.blocks()[child].old_address.to_string();
        assert!(
            report["facts"]["blocks"][child_address]
                .get("root")
                .is_none()
        );
        assert_eq!(report["errors"].as_array().unwrap().len(), 1);
    }
    // The mutation is confined to the copied fixture, not an edit/repair API.
    let f = BlendFile::open(&original).unwrap();
    assert!(
        f.get_pointer(object, 0, &[PathStep::Field(b"parent".to_vec())])
            .is_ok()
    );
    std::fs::remove_dir_all(work).unwrap();
}
