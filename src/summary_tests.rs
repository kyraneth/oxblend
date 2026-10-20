use super::*;
use std::path::PathBuf;
#[test]
fn summary_bare_id_camera_uses_native_name_field() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/synthetic-empty-seed.blend");
    let mut bytes = std::fs::read(&path).unwrap();
    let f = BlendFile::open(&path).unwrap();
    let si = |ty: &[u8]| {
        f.schema()
            .structs
            .iter()
            .position(|s| f.schema().types[s.type_index] == ty)
            .unwrap()
    };
    let offset = |ty: &[u8], name: &[u8]| {
        f.schema().structs[si(ty)]
            .fields
            .iter()
            .find(|s| s.name == name)
            .unwrap()
            .offset
    };
    let size = |ty: &[u8]| f.schema().lengths[f.schema().structs[si(ty)].type_index];
    let proxy_address = f
        .blocks()
        .iter()
        .map(|b| b.old_address + b.size as u64)
        .max()
        .unwrap()
        + 8;
    let library_address = proxy_address + size(b"ID") as u64 + 8;
    let mut proxy = vec![0; size(b"ID")];
    let o = offset(b"ID", b"name");
    proxy[o..o + 13].copy_from_slice(b"OBProxyCamera");
    let o = offset(b"ID", b"lib");
    proxy[o..o + 8].copy_from_slice(&library_address.to_le_bytes());
    let mut library = vec![0; size(b"Library")];
    let o = offset(b"ID", b"name");
    library[o..o + 9].copy_from_slice(b"LIfixture");
    let mut extra = Vec::new();
    for (code, ty, address, data) in [
        (*b"ID\0\0", b"ID".as_slice(), proxy_address, proxy),
        (*b"LI\0\0", b"Library".as_slice(), library_address, library),
    ] {
        extra.extend_from_slice(&code);
        extra.extend_from_slice(&(data.len() as u32).to_le_bytes());
        extra.extend_from_slice(&address.to_le_bytes());
        extra.extend_from_slice(&(si(ty) as u32).to_le_bytes());
        extra.extend_from_slice(&1u32.to_le_bytes());
        extra.extend_from_slice(&data);
    }
    let scene = f.blocks().iter().find(|b| b.code == *b"SC\0\0").unwrap();
    let o = scene.data_offset + offset(b"Scene", b"camera");
    bytes[o..o + 8].copy_from_slice(&proxy_address.to_le_bytes());
    let dna = f.blocks().iter().find(|b| b.code == *b"DNA1").unwrap();
    bytes.splice(dna.data_offset - 24..dna.data_offset - 24, extra);
    let output = BlendFile::from_bytes(bytes).unwrap();
    let report = summarize(&output).unwrap();
    assert_eq!(report["scenes"][0]["camera"], "OBProxyCamera");
}

fn seed() -> BlendFile {
    BlendFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/file-ops/synthetic-empty-seed.blend"),
    )
    .unwrap()
}
fn field_offset(file: &BlendFile, ty: &[u8], name: &[u8]) -> usize {
    file.schema()
        .structs
        .iter()
        .find(|s| file.schema().types[s.type_index] == ty)
        .unwrap()
        .fields
        .iter()
        .find(|f| f.name == name)
        .unwrap()
        .offset
}
fn set_timing(
    file: &BlendFile,
    record: &mut [u8],
    start: i32,
    end: i32,
    step: i32,
    numerator: i16,
    base: u32,
) {
    let r = field_offset(file, b"Scene", b"r");
    for (field, value) in [
        (b"sfra".as_slice(), start),
        (b"efra", end),
        (b"frame_step", step),
    ] {
        let o = r + field_offset(file, b"RenderData", field);
        record[o..o + 4].copy_from_slice(&value.to_le_bytes());
    }
    let o = r + field_offset(file, b"RenderData", b"frs_sec");
    record[o..o + 2].copy_from_slice(&numerator.to_le_bytes());
    let o = r + field_offset(file, b"RenderData", b"frs_sec_base");
    record[o..o + 4].copy_from_slice(&base.to_le_bytes());
}

#[test]
fn every_scene_element_keeps_saved_order_and_exact_fps_base() {
    let file = seed();
    assert_eq!(file.header().file_format, 0);
    assert_eq!(file.header().pointer_size, 8);
    let index = file
        .blocks()
        .iter()
        .position(|b| b.code == *b"SC\0\0")
        .unwrap();
    let block = &file.blocks()[index];
    assert_eq!(block.count, 1);
    let mut first = file.payload(index).unwrap().into_owned();
    let mut second = first.clone();
    set_timing(&file, &mut first, -12, 80, 3, 30, 1.001f32.to_bits());
    set_timing(&file, &mut second, 7, 15, 2, 48, 2.0f32.to_bits());
    let name = field_offset(&file, b"Scene", b"id") + field_offset(&file, b"ID", b"name");
    second[name..name + 9].copy_from_slice(b"SCSecond\0");
    let mut bytes = file.read_decompressed_bytes().unwrap();
    // This fixture uses legacy 64-bit BHeads: length at +4, count at +20.
    let header = block.data_offset - 24;
    bytes[header + 4..header + 8].copy_from_slice(&((first.len() * 2) as u32).to_le_bytes());
    bytes[header + 20..header + 24].copy_from_slice(&2u32.to_le_bytes());
    first.extend(second);
    bytes.splice(block.data_offset..block.data_offset + block.size, first);
    let report = summarize(&BlendFile::from_bytes(bytes).unwrap()).unwrap();
    let scenes = report["scenes"].as_array().unwrap();
    assert_eq!(scenes.len(), 2);
    assert_eq!(scenes[0]["frame_start"], -12);
    assert_eq!(scenes[0]["frame_end"], 80);
    assert_eq!(scenes[0]["frame_step"], 3);
    assert_eq!(scenes[0]["fps"]["numerator"], 30);
    assert_eq!(
        scenes[0]["fps"]["base_bits_hex"],
        format!("{:08x}", 1.001f32.to_bits())
    );
    assert_eq!(scenes[0]["fps"]["base"], f64::from(1.001f32));
    assert_eq!(scenes[0]["fps"]["effective"], 30.0 / f64::from(1.001f32));
    assert_eq!(scenes[1]["name"], "SCSecond");
    assert_eq!(scenes[1]["frame_start"], 7);
    assert_eq!(scenes[1]["frame_end"], 15);
    assert_eq!(scenes[1]["frame_step"], 2);
    assert_eq!(scenes[1]["fps"]["effective"], 24.0);
    assert!(report["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn invalid_fps_base_is_never_silent_null_or_infinite() {
    let file = seed();
    let index = file
        .blocks()
        .iter()
        .position(|b| b.code == *b"SC\0\0")
        .unwrap();
    let block = &file.blocks()[index];
    for base in [0.0f32, f32::NAN, f32::INFINITY] {
        let mut record = file.payload(index).unwrap().into_owned();
        set_timing(&file, &mut record, 1, 250, 1, 24, base.to_bits());
        let mut bytes = file.read_decompressed_bytes().unwrap();
        bytes[block.data_offset..block.data_offset + record.len()].copy_from_slice(&record);
        let report = summarize(&BlendFile::from_bytes(bytes).unwrap()).unwrap();
        let scene = &report["scenes"][0];
        assert!(scene["fps"]["effective"].is_null());
        assert_eq!(
            scene["fps"]["base_bits_hex"],
            format!("{:08x}", base.to_bits())
        );
        assert_eq!(report["warnings"][0]["kind"], "type");
        assert_eq!(report["warnings"][0]["field"], "r.frs_sec_base");
        if !base.is_finite() {
            assert!(scene["fps"]["base"].is_null());
        }
    }
}
