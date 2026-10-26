use super::*;
fn seed() -> BlendFile {
    BlendFile::open(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/file-ops/synthetic-empty-seed.blend"),
    )
    .unwrap()
}
#[test]
fn selects_every_element_in_a_multi_record_block() {
    let file = seed();
    let index = file
        .blocks()
        .iter()
        .position(|b| b.code == *b"SC\0\0")
        .unwrap();
    let b = &file.blocks()[index];
    let mut data = file.read_decompressed_bytes().unwrap();
    let record = file.payload(index).unwrap();
    let mut pair = record.to_vec();
    pair.extend_from_slice(&record);
    let header = b.data_offset - 24;
    data[header + 4..header + 8].copy_from_slice(&(pair.len() as u32).to_le_bytes());
    data[header + 20..header + 24].copy_from_slice(&2u32.to_le_bytes());
    data.splice(b.data_offset..b.data_offset + b.size, pair);
    let file = BlendFile::from_bytes(data).unwrap();
    let r = query(&file, "Scene", &["r.sfra".into()]).unwrap();
    assert_eq!(r["requested_coverage"]["complete"], true);
    let rows = r["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["source_local_ref"]["element"], 0);
    assert_eq!(rows[1]["source_local_ref"]["element"], 1);
}
#[test]
fn raw_signed_zero_is_bits_not_json_zero() {
    assert_eq!(access::hex_float(-0.0), "-0x0.0p+0");
    assert_eq!(access::hex_float(1.0), "0x1.0000000000000p+0");
    assert_eq!(
        access::hex_float(f64::from_bits(1)),
        "0x0.0000000000001p-1022"
    );
}
#[test]
fn malformed_auxiliary_record_counts_refuse_before_allocation() {
    for auxiliary in ["ID", "FileGlobal"] {
        let f = seed();
        let index = f
            .blocks()
            .iter()
            .position(|b| {
                if auxiliary == "ID" {
                    b.code == *b"SC\0\0"
                } else {
                    b.code == *b"GLOB"
                }
            })
            .unwrap();
        let block = &f.blocks()[index];
        let mut bytes = f.read_decompressed_bytes().unwrap();
        if auxiliary == "ID" {
            let si = f
                .schema()
                .structs
                .iter()
                .position(|s| f.schema().types[s.type_index] == b"ID")
                .unwrap();
            bytes[block.data_offset - 8..block.data_offset - 4]
                .copy_from_slice(&(si as u32).to_le_bytes());
            bytes[block.data_offset - 24..block.data_offset - 20].copy_from_slice(b"ID\0\0");
        }
        bytes[block.data_offset - 4..block.data_offset]
            .copy_from_slice(&(i32::MAX as u32).to_le_bytes());
        let file = BlendFile::from_bytes(bytes).unwrap();
        let error = query(&file, "Scene", &["r.sfra".into()]).unwrap_err();
        assert_eq!(error.kind, "truncated");
    }
}
#[test]
fn every_id_family_stub_is_selected_without_inventing_its_body() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/library-null/normal.blend");
    let f = BlendFile::open(path).unwrap();
    let stub = access::records(&f, "ID")
        .unwrap()
        .into_iter()
        .find(|r| matches!(access::get(&f,r,"name"),Ok(Value::Bytes(v)) if v.starts_with(b"IM")))
        .unwrap();
    let location = f
        .locate_field(stub.block, stub.element, &access::steps("name"))
        .unwrap()
        .offset;
    for (code, type_name, kind) in [
        (b"LT", "Lattice", "Lattice"),
        (b"NT", "bNodeTree", "NodeTree"),
    ] {
        let mut bytes = f.read_decompressed_bytes().unwrap();
        let offset = f.blocks()[stub.block].data_offset + location;
        bytes[offset..offset + 2].copy_from_slice(code);
        let file = BlendFile::from_bytes(bytes).unwrap();
        let report = query(
            &file,
            type_name,
            &["name".into(), "not_a_body_field".into()],
        )
        .unwrap();
        let row = report["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["content_status"] == "linked_reference")
            .unwrap();
        assert_eq!(row["id"]["kind"], kind);
        assert_eq!(row["values"]["name"]["status"], "available");
        assert_eq!(
            row["values"]["not_a_body_field"]["kind"],
            "linked_reference_only"
        );
    }
}
#[test]
fn missing_optional_subversion_preserves_raw_rows_and_reports_its_site() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dependencies/mesh-material-root.blend");
    let file = BlendFile::open(path).unwrap();
    let report = query(&file, "Scene", &["id.name".into()]).unwrap();
    assert!(!report["rows"].as_array().unwrap().is_empty());
    assert_eq!(report["requested_coverage"]["complete"], true);
    assert!(report["source_observation"]["subversion"].is_null());
    assert_eq!(report["source_observation"]["complete"], false);
    assert_eq!(
        report["source_observation"]["errors"][0]["field"],
        "FileGlobal.subversion"
    );
    assert_eq!(report["source_observation"]["errors"][0]["block_index"], 5);
    let partial = query(&file, "Scene", &["id.name".into(), "render".into()]).unwrap();
    assert_eq!(partial["requested_coverage"]["complete"], false);
    assert_eq!(
        partial["rows"][0]["values"]["id.name"]["status"],
        "available"
    );
}
#[test]
fn explain_never_fuzzily_executes_an_alias() {
    assert!(explain(None, "scene.rendr").is_err());
    assert_eq!(explain(None, "render").unwrap()["concept"], "scene.render");
}
