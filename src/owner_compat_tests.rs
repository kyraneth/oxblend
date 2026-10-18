use crate::{
    BlendFile,
    file_ops::{graph::Graph, pointer_census},
};
use std::path::PathBuf;
#[test]
fn read_version_profile_does_not_expand_write_or_import_admission() {
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trace-owner/cross-id.blend");
    for version in [0, 400, 405, 406, 500, 502, 503, 600, 999, 9999] {
        let mut file = BlendFile::open(&source).unwrap();
        file.header.version = version;
        // Diagnostic owner/layout qualification works under the broader read
        // profile; it does not become writer/import authority.
        pointer_census(&file).unwrap();
        let admission = file.owner_index().unwrap().admit_write(&file);
        if matches!(version, 405 | 502) {
            admission.unwrap();
        } else {
            assert_eq!(admission.unwrap_err().kind, "owner_header");
            assert_eq!(Graph::new(&file).err().unwrap().kind, "owner_header");
        }
    }
}
fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trace-owner/read-compat")
}
#[test]
fn generated_id_table_qualifies_native_and_legacy_structs() {
    for (code, ty) in [
        (*b"PL\0\0", "Palette"),
        (*b"PA\0\0", "ParticleSettings"),
        (*b"SK\0\0", "Speaker"),
        (*b"GR\0\0", "Collection"),
    ] {
        let f = BlendFile::open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/file-ops/synthetic-empty-seed.blend"),
        )
        .unwrap();
        let mut g = Graph::new(&f).unwrap();
        let id = g.new_typed(ty, None).unwrap();
        g.records[id].code = code;
        g.cstring(
            id,
            &["id", "name"],
            &format!("{}contract", std::str::from_utf8(&code[..2]).unwrap()),
        )
        .unwrap();
        // Actual pinned native SDNA layout; zero fields, not a receiving claim.
        let file = BlendFile::from_bytes(g.decoded().unwrap()).unwrap();
        pointer_census(&file).unwrap().require_closed().unwrap();
    }
    assert_eq!(crate::id_codes::canonical(b"Group"), b"Collection");
    assert_eq!(crate::id_codes::canonical(b"Screen"), b"bScreen");
    assert_eq!(crate::id_codes::canonical(b"Lamp"), b"Light");
    assert_eq!(crate::id_codes::canonical_code(b"SN"), b"SR");
}
#[test]
fn read_ownership_does_not_admit_file_ops_writes() {
    use crate::value::PathStep;
    for (name, kind) in [
        ("unknown-id.blend", "owner_header"),
        ("data-before-id.blend", "stray_data_run"),
        ("data-after-control.blend", "stray_data_run"),
    ] {
        let file = dir().join(name);
        let f = BlendFile::open(&file).unwrap();
        let index = f.owner_index().unwrap();
        let t = crate::trace::trace(
            &file,
            crate::trace::TraceOptions {
                mode: crate::trace::TraceMode::PerFile,
                pointers: crate::trace::PointerMode::Strict,
                low_memory: false,
            },
        )
        .unwrap();
        assert!(t.complete, "{name}: {:?}", t.errors);
        let expected = if name == "unknown-id.blend" {
            // Trace does not enumerate the unknown ID as an Object root, but its
            // physical owner scope remains available to contextual getters.
            assert_eq!(
                f.get_pointer(
                    0,
                    0,
                    &[
                        PathStep::Field(b"modifiers".to_vec()),
                        PathStep::Field(b"first".to_vec())
                    ]
                )
                .unwrap(),
                Some(1)
            );
            vec![b"//B.pc2".as_slice()]
        } else {
            vec![b"//A.pc2".as_slice(), b"//B.pc2".as_slice()]
        };
        assert_eq!(
            t.usages
                .iter()
                .map(|u| u.asset_path.as_slice())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(index.admit_write(&f).unwrap_err().kind, kind);
        assert_eq!(pointer_census(&f).unwrap_err().kind, kind);
        assert!(matches!(Graph::new(&f), Err(e) if e.kind == kind));
        let known = BlendFile::open(dir().parent().unwrap().join("cross-id.blend")).unwrap();
        let g = Graph::new(&known).unwrap();
        assert_eq!(g.compatible(&f).unwrap_err().kind, kind);
        if name != "unknown-id.blend" {
            let ignored = f
                .blocks()
                .iter()
                .position(|b| b.old_address == 0x7000)
                .unwrap();
            assert_eq!(index.owner(ignored), None);
            assert_eq!(index.local(&f, ignored, 0x7000).unwrap(), None);
        }
    }
    for name in ["raw-linked-id.blend", "data-after-placeholder.blend"] {
        let f = BlendFile::open(dir().join(name)).unwrap();
        assert_eq!(f.blocks()[1].sdna_index, 0);
        assert_eq!(
            f.get_pointer(1, 0, &[PathStep::Field(b"lib".to_vec())])
                .unwrap(),
            Some(0)
        );
        let object = f
            .blocks()
            .iter()
            .position(|b| b.code == *b"OB\0\0")
            .unwrap();
        assert_eq!(
            f.get_pointer(object, 0, &[PathStep::Field(b"data".to_vec())])
                .unwrap(),
            Some(1)
        );
        if name == "data-after-placeholder.blend" {
            let index = f.owner_index().unwrap();
            assert_eq!(index.owner(2), None);
            assert_eq!(index.local(&f, 1, f.blocks()[2].old_address).unwrap(), None);
        }
        if name == "raw-linked-id.blend" {
            pointer_census(&f).unwrap();
        } else {
            // A valid typed ID0 placeholder does not own following DATA.
            assert_eq!(pointer_census(&f).unwrap_err().kind, "stray_data_run");
        }
        assert_eq!(
            f.owner_index().unwrap().admit_write(&f).unwrap_err().kind,
            "owner_header"
        );
        assert!(matches!(Graph::new(&f), Err(e) if e.kind == "owner_header"));
    }
    // USER is not in the ID libmap even when its payload looks like an ID.
    let mut bytes = std::fs::read(dir().join("raw-linked-id.blend")).unwrap();
    bytes[12..16].copy_from_slice(b"USER");
    let f = BlendFile::from_bytes(bytes).unwrap();
    assert_eq!(
        f.get_pointer(1, 0, &[PathStep::Field(b"lib".to_vec())])
            .unwrap_err()
            .kind,
        "dangling_pointer"
    );
}
