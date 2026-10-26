//! Narrow held-input contracts; actual CLI mutation and real-shot proofs are separate.
use crate::trace_input::Input;
use std::borrow::Cow;
use std::fs::OpenOptions;
use std::io::{SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Owned(PathBuf);
impl Owned {
    fn new(bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "oxblend-v5-contract-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap()
            .write_all(bytes)
            .unwrap();
        Self(path)
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
#[test]
fn index_window_is_borrowed_and_truncation_still_fails_boundary() {
    let file = Owned::new(b"abcdefghijklmnopqrstuvwxyz");
    let input = Input::open(
        &file.0,
        &crate::session::DiagSink::test_sink(),
        &std::env::temp_dir(),
    )
    .unwrap();
    input
        .scan(|reader| {
            let mut first = [0; 3];
            reader.read_exact(&mut first).unwrap();
            reader.seek_relative(4).unwrap();
            let mut second = [0; 2];
            reader.read_exact(&mut second).unwrap();
            assert_eq!(first, *b"abc");
            assert_eq!(second, *b"hi");
            Ok(())
        })
        .unwrap();
    assert!(matches!(input.range(15, 3).unwrap(), Cow::Borrowed(b"pqr")));
    OpenOptions::new()
        .write(true)
        .open(&file.0)
        .unwrap()
        .set_len(0)
        .unwrap();
    // Held bytes remain immutable. Returning a report still requires the guard.
    assert_eq!(input.range(15, 3).unwrap().as_ref(), b"pqr");
    assert_eq!(input.check().unwrap_err().kind, "truncated");
}
#[test]
fn exact_miss_capture_and_path_replacement_guard() {
    let file = Owned::new(b"abcdefghijklmnopqrstuvwxyz");
    let input = Input::open(
        &file.0,
        &crate::session::DiagSink::test_sink(),
        &std::env::temp_dir(),
    )
    .unwrap();
    input
        .scan(|reader| {
            reader.seek(SeekFrom::Start(20)).unwrap();
            let mut bytes = [0; 2];
            reader.read_exact(&mut bytes).unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(input.range(10, 13).unwrap().as_ref(), b"klmnopqrstuvw");
    std::fs::remove_file(&file.0).unwrap();
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file.0)
        .unwrap()
        .write_all(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ")
        .unwrap();
    assert_eq!(input.check().unwrap_err().kind, "input_changed");
}
#[test]
fn blocks_scans_ranges_and_final_boundary_guard_every_source_kind() {
    let mut blend = b"BLENDER-v405ENDB".to_vec();
    blend.extend_from_slice(&[0; 20]);
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&blend).unwrap();
    let mut seek = Vec::new();
    let mut table = Vec::new();
    // Two authored equal frames deliberately divide the ENDB BHead.
    for chunk in blend.chunks(blend.len() / 2) {
        let encoded = zstd::stream::encode_all(chunk, 0).unwrap();
        table.extend((encoded.len() as u32).to_le_bytes());
        table.extend((chunk.len() as u32).to_le_bytes());
        seek.extend(encoded);
    }
    table.extend(2u32.to_le_bytes());
    table.push(0);
    table.extend(0x8f92eab1u32.to_le_bytes());
    seek.extend(0x184d2a5eu32.to_le_bytes());
    seek.extend((table.len() as u32).to_le_bytes());
    seek.extend(table);
    for bytes in [
        blend.clone(),
        gzip.finish().unwrap(),
        zstd::stream::encode_all(blend.as_slice(), 0).unwrap(),
        seek,
    ] {
        for when in ["start", "end"] {
            for truncate in [false, true] {
                let file = Owned::new(&bytes);
                let input = Input::open_blocks(
                    &file.0,
                    &crate::session::DiagSink::test_sink(),
                    &std::env::temp_dir(),
                )
                .unwrap();
                assert_eq!(input.len(), blend.len());
                assert_eq!(input.range(0, 7).unwrap().as_ref(), b"BLENDER");
                let change = || {
                    let mut writer = OpenOptions::new().write(true).open(&file.0).unwrap();
                    if truncate {
                        writer.set_len(1).unwrap();
                    } else {
                        writer.write_all(b"X").unwrap();
                        writer.set_modified(std::time::UNIX_EPOCH).unwrap();
                    }
                };
                if when == "start" {
                    change();
                }
                let error = input
                    .scan(|reader| {
                        let mut prefix = [0; 7];
                        reader.read_exact(&mut prefix).unwrap();
                        if when == "end" {
                            change();
                        }
                        Ok(())
                    })
                    .unwrap_err();
                assert_eq!(
                    error.kind,
                    if truncate {
                        "truncated"
                    } else {
                        "input_changed"
                    }
                );
                assert_eq!(input.range(0, 7).unwrap_err().kind, error.kind);
                assert_eq!(input.check().unwrap_err().kind, error.kind);
            }
        }
    }
}
#[test]
fn blocks_stream_admission_checks_checksum_tail_and_structure() {
    let mut blend = b"BLENDER-v405ENDB".to_vec();
    blend.extend_from_slice(&[0; 20]);
    for tail in [false, true] {
        let mut source = blend.clone();
        if tail {
            source.push(1);
        }
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(&source).unwrap();
        let bytes = gzip.finish().unwrap();
        if tail {
            assert_eq!(
                Input::open_blocks(
                    &Owned::new(&bytes).0,
                    &crate::session::DiagSink::test_sink(),
                    &std::env::temp_dir()
                )
                .unwrap_err()
                .kind,
                "invalid_block"
            );
        } else {
            let mut bad = bytes.clone();
            let end = bad.len();
            bad[end - 8] ^= 1;
            assert_eq!(
                Input::open_blocks(
                    &Owned::new(&bad).0,
                    &crate::session::DiagSink::test_sink(),
                    &std::env::temp_dir()
                )
                .unwrap_err()
                .kind,
                "compression"
            );
            assert_eq!(
                Input::open_blocks(
                    &Owned::new(&bytes[..end - 1]).0,
                    &crate::session::DiagSink::test_sink(),
                    &std::env::temp_dir()
                )
                .unwrap_err()
                .kind,
                "compression"
            );
        }
    }
}
#[test]
fn damaged_seek_table_falls_back_explicitly_to_checked_spill() {
    let mut blend = b"BLENDER-v405ENDB".to_vec();
    blend.extend_from_slice(&[0; 20]);
    let mut seek = Vec::new();
    let mut table = Vec::new();
    for chunk in blend.chunks(blend.len() / 2) {
        let encoded = zstd::stream::encode_all(chunk, 0).unwrap();
        table.extend((encoded.len() as u32).to_le_bytes());
        table.extend((chunk.len() as u32).to_le_bytes());
        seek.extend(encoded);
    }
    table.extend(2u32.to_le_bytes());
    table.push(0);
    table.extend(0x8f92eab1u32.to_le_bytes());
    seek.extend(0x184d2a5eu32.to_le_bytes());
    seek.extend((table.len() as u32).to_le_bytes());
    seek.extend(table);
    *seek.last_mut().unwrap() ^= 1;
    let source = Owned::new(&seek);
    let input = Input::open(
        &source.0,
        &crate::session::DiagSink::test_sink(),
        &std::env::temp_dir(),
    )
    .unwrap();
    assert_eq!(input.stats()["mode"], "checked_full_streaming_spill");
    assert_eq!(
        input.stats()["seek_table"],
        "untrusted → checked_full_streaming_spill"
    );
    assert_eq!(
        input.stats()["seek_table_fallback_reason"],
        "damaged seek footer magic"
    );
    assert_eq!(input.range(0, blend.len()).unwrap().as_ref(), blend);
}
#[test]
fn compressed_spill_requires_complete_gzip_checksum() {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut blend = b"BLENDER-v405ENDB".to_vec();
    blend.extend_from_slice(&[0; 20]);
    encoder.write_all(&blend).unwrap();
    let bytes = encoder.finish().unwrap();
    let file = Owned::new(&bytes);
    let input = Input::open(
        &file.0,
        &crate::session::DiagSink::test_sink(),
        &std::env::temp_dir(),
    )
    .unwrap();
    assert_eq!(input.range(0, 7).unwrap().as_ref(), b"BLENDER");
    assert_eq!(input.stats()["mode"], "checked_full_streaming_spill");
    let mut broken = bytes.clone();
    let end = broken.len();
    broken[end - 8] ^= 1;
    let file = Owned::new(&broken);
    assert_eq!(
        Input::open(
            &file.0,
            &crate::session::DiagSink::test_sink(),
            &std::env::temp_dir()
        )
        .unwrap_err()
        .kind,
        "compression"
    );
    let file = Owned::new(&bytes[..bytes.len() - 1]);
    assert_eq!(
        Input::open(
            &file.0,
            &crate::session::DiagSink::test_sink(),
            &std::env::temp_dir()
        )
        .unwrap_err()
        .kind,
        "compression"
    );
}
