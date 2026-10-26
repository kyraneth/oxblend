//! Native Blender 5.2.1 saved ID stubs (root -> middle -> leaf).
use oxblend::trace::{FailureKind, TraceOptions, trace};
use std::path::PathBuf;
#[test]
fn native_local_id_wins_over_same_named_forwarding_stub() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stub-chain");
    let t = trace(&dir.join("pick.blend"), TraceOptions::default()).unwrap();
    assert!(t.complete, "{:?}", t.errors);
    assert_eq!(t.usages.len(), 1, "{:?}", t.usages);
    assert_eq!(t.usages[0].asset_path, b"//both.blend");
    let t = trace(&dir.join("direct.blend"), TraceOptions::default()).unwrap();
    assert!(t.complete, "{:?}", t.errors);
    assert!(
        t.usages
            .iter()
            .any(|u| u.owner_file.ends_with(b"/both.blend") && u.asset_path == b"//leaf.blend"),
        "direct pointer must retain its exact stub, not substitute the local same-name ID"
    );
}
#[test]
fn native_forwarded_stub_resolves_and_terminal_absence_extends_chain() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stub-chain");
    let root = std::env::temp_dir().join(format!("oxblend-stub-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    for name in ["root.blend", "midl.blend", "leaf.blend"] {
        std::fs::copy(fixtures.join(name), root.join(name)).unwrap();
    }
    let t = trace(&root.join("root.blend"), TraceOptions::default()).unwrap();
    assert!(t.complete, "{:?}", t.errors);
    assert!(t.errors.is_empty());
    assert!(
        t.usages
            .iter()
            .any(|u| u.owner_file.ends_with(b"/midl.blend") && u.asset_path == b"//leaf.blend")
    );
    // An existing terminal library that lacks the selected ID is not an
    // absent physical file. Its diagnostic retains the forwarding owner.
    std::fs::copy(fixtures.join("empty.blend"), root.join("leaf.blend")).unwrap();
    let t = trace(&root.join("root.blend"), TraceOptions::default()).unwrap();
    assert!(!t.complete);
    let e = t
        .errors
        .iter()
        .find(|e| matches!(e.kind, FailureKind::MissingId))
        .unwrap();
    assert!(e.library_path.as_ref().unwrap().ends_with(b"/leaf.blend"));
    let id = e.missing_id.as_ref().unwrap();
    assert_eq!(id.id_name, b"OBLeaf");
    assert!(
        id.referencing_owner_chain
            .last()
            .unwrap()
            .ends_with(b"/midl.blend")
    );
    assert!(
        id.referencing_owner_chain
            .first()
            .unwrap()
            .ends_with(b"/root.blend")
    );
    std::fs::copy(fixtures.join("leaf.blend"), root.join("leaf.blend")).unwrap();
    let mut bytes = std::fs::read(root.join("midl.blend")).unwrap();
    let at = bytes
        .windows(b"//leaf.blend\0".len())
        .position(|w| w == b"//leaf.blend\0")
        .unwrap();
    bytes[at..at + b"//midl.blend".len()].copy_from_slice(b"//midl.blend");
    std::fs::write(root.join("midl.blend"), bytes).unwrap();
    let t = trace(&root.join("root.blend"), TraceOptions::default()).unwrap();
    assert!(
        !t.complete,
        "forwarding cycle must not become a successful visited-cache hit"
    );
    assert!(
        t.errors
            .iter()
            .any(|e| matches!(e.kind, FailureKind::Cycle)),
        "{:?}",
        t.errors
    );
    // Fallback retains every qualified library identity, but reports that a
    // name alone cannot choose between two different forwarding sources.
    std::fs::copy(fixtures.join("midl.blend"), root.join("midl.blend")).unwrap();
    std::fs::copy(fixtures.join("leaf.blend"), root.join("else.blend")).unwrap();
    let path = root.join("midl.blend");
    let reader = oxblend::BlendFile::open(&path).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    let stub = reader
        .blocks()
        .iter()
        .position(|b| b.code == *b"ID\0\0")
        .unwrap();
    let library = reader
        .get_pointer(stub, 0, &[oxblend::value::PathStep::Field(b"lib".to_vec())])
        .unwrap()
        .unwrap();
    let lib = &reader.blocks()[library];
    let id = &reader.blocks()[stub];
    // BHead1 is 32 bytes; classic BHead is 16 + saved pointer width.
    let width = if reader.header().file_format == 1 {
        32
    } else {
        16 + reader.header().pointer_size
    };
    let address = reader
        .blocks()
        .iter()
        .map(|b| b.old_address)
        .max()
        .unwrap()
        .checked_add(1)
        .unwrap();
    let mut extra_lib = bytes[lib.data_offset - width..lib.data_offset + lib.size].to_vec();
    extra_lib[8..16].copy_from_slice(&address.to_le_bytes());
    let field = reader.schema().structs[reader.struct_index(library).unwrap()]
        .fields
        .iter()
        .find(|f| f.name == b"name")
        .unwrap();
    extra_lib[width + field.offset..width + field.offset + field.size].fill(0);
    extra_lib[width + field.offset..width + field.offset + b"//else.blend".len()]
        .copy_from_slice(b"//else.blend");
    let mut extra_id = bytes[id.data_offset - width..id.data_offset + id.size].to_vec();
    extra_id[8..16].copy_from_slice(&(address + 1).to_le_bytes());
    let field = reader.schema().structs[reader.struct_index(stub).unwrap()]
        .fields
        .iter()
        .find(|f| f.name == b"lib")
        .unwrap();
    extra_id[width + field.offset..width + field.offset + 8]
        .copy_from_slice(&address.to_le_bytes());
    extra_lib.extend(extra_id);
    let dna = reader
        .blocks()
        .iter()
        .find(|b| b.code == *b"DNA1")
        .unwrap()
        .data_offset
        - width;
    bytes.splice(dna..dna, extra_lib);
    drop(reader);
    std::fs::write(&path, bytes).unwrap();
    let t = trace(&root.join("root.blend"), TraceOptions::default()).unwrap();
    assert!(!t.complete);
    let json = t.json();
    let ambiguity = json["errors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "ambiguous_id")
        .unwrap_or_else(|| panic!("{json}"));
    assert_eq!(ambiguity["status"], "unknown");
    let actual = ambiguity["site"]["candidate_library_paths_hex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    let expected = [root.join("leaf.blend"), root.join("else.blend")]
        .iter()
        .map(|p| oxblend::value::hex(p.as_os_str().as_encoded_bytes()))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        actual, expected,
        "diagnostic paths must be real resolved paths, not NUL-padded SDNA fields"
    );
    for name in [b"//leaf.blend".as_slice(), b"//else.blend"] {
        assert!(t.usages.iter().any(|u| u.asset_path == name));
    }
    std::fs::remove_dir_all(root).unwrap();
}
