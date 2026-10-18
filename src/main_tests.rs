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
    let report = summary(&output).unwrap();
    assert_eq!(report["scenes"][0]["camera"], "OBProxyCamera");
}
