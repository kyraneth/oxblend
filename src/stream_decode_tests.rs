use crate::stream_decode::decode;
use std::io::Cursor;
fn blend() -> Vec<u8> {
    let mut b = b"BLENDER-v405ENDB".to_vec();
    b.extend_from_slice(&[0; 20]);
    b
}
#[test]
fn header_and_end_are_read_before_tail() {
    let b = blend();
    let mut out = Vec::new();
    decode(&mut Cursor::new(&b), &mut out).unwrap();
    assert_eq!(out, b);
    let mut garbage = b"BLENDER-v405".to_vec();
    garbage.extend_from_slice(&[0; 24]);
    let mut source = Cursor::new(garbage);
    let e = decode(&mut source, &mut Vec::new()).unwrap_err();
    assert_eq!(e.kind, "invalid_block");
    assert_eq!(source.position(), 36);
}
#[test]
fn decompressed_trailing_garbage_refuses() {
    let mut b = blend();
    b.push(1);
    assert_eq!(
        decode(&mut Cursor::new(b), &mut Vec::new())
            .unwrap_err()
            .kind,
        "invalid_block"
    );
}
#[test]
fn truncated_structure_refuses() {
    let b = blend();
    for end in 0..b.len() {
        assert!(decode(&mut Cursor::new(&b[..end]), &mut Vec::new()).is_err());
    }
}
#[test]
fn signed_negative_extent_refuses_before_payload() {
    let mut b = blend();
    b[12..16].copy_from_slice(b"DATA");
    b[16..20].copy_from_slice(&(-1i32).to_le_bytes());
    assert_eq!(
        decode(&mut Cursor::new(b), &mut Vec::new())
            .unwrap_err()
            .kind,
        "invalid_block"
    );
}
#[test]
fn compressed_end_still_requires_checksum() {
    let b = blend();
    let mut z = zstd::stream::Encoder::new(Vec::new(), 0).unwrap();
    z.include_checksum(true).unwrap();
    std::io::Write::write_all(&mut z, &b).unwrap();
    let bytes = z.finish().unwrap();
    for end in bytes.len() - 4..bytes.len() {
        let mut r = zstd::stream::Decoder::new(Cursor::new(&bytes[..end])).unwrap();
        assert_eq!(
            decode(&mut r, &mut Vec::new()).unwrap_err().kind,
            "compression"
        );
    }
    let mut corrupt = bytes;
    let i = corrupt.len() - 1;
    corrupt[i] ^= 1;
    let mut r = zstd::stream::Decoder::new(Cursor::new(corrupt)).unwrap();
    assert_eq!(
        decode(&mut r, &mut Vec::new()).unwrap_err().kind,
        "compression"
    );
}
#[test]
fn gzip_trailing_data_is_not_silently_ignored() {
    use std::io::Write;
    let mut g = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    g.write_all(&blend()).unwrap();
    let mut bytes = g.finish().unwrap();
    bytes.extend_from_slice(b"garbage");
    let mut r = flate2::read::MultiGzDecoder::new(Cursor::new(bytes));
    assert!(decode(&mut r, &mut Vec::new()).is_err());
}
