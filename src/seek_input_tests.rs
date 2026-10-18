//! Narrow format/held-input contracts. Real CLI byte parity is separate receiving evidence.
use super::*;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
pub(super) fn fixture(checksum: bool) -> File {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_TMPFILE)
        .open(std::env::temp_dir())
        .unwrap();
    let bodies = [b"first".as_slice(), b"second".as_slice()];
    let mut entries = Vec::new();
    for body in bodies {
        let mut encoder =
            zstd::stream::write::Encoder::new(Vec::new(), zstd::DEFAULT_COMPRESSION_LEVEL).unwrap();
        encoder.include_checksum(true).unwrap();
        encoder.write_all(body).unwrap();
        let encoded = encoder.finish().unwrap();
        file.write_all(&encoded).unwrap();
        entries.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
        entries.extend_from_slice(&(body.len() as u32).to_le_bytes());
        if checksum {
            let mut h = crate::edit_xxh::Xxh64::new();
            h.update(body);
            entries.extend_from_slice(&(h.digest() as u32).to_le_bytes());
        }
    }
    entries.extend_from_slice(&(bodies.len() as u32).to_le_bytes());
    entries.push(if checksum { 0x80 } else { 0 });
    entries.extend_from_slice(&0x8f92eab1u32.to_le_bytes());
    file.write_all(&0x184d2a5eu32.to_le_bytes()).unwrap();
    file.write_all(&(entries.len() as u32).to_le_bytes())
        .unwrap();
    file.write_all(&entries).unwrap();
    file
}
#[test]
fn seek_cross_frame_extent_borrow_and_footer_checksum() {
    for checksum in [false, true] {
        let input = Input::open(fixture(checksum)).unwrap().unwrap();
        assert!(matches!(
            input.range(0, 5).unwrap(),
            Cow::Borrowed(b"first")
        ));
        assert_eq!(input.range(3, 6).unwrap().as_ref(), b"stseco");
        assert_eq!(input.range(10, 2).unwrap_err().kind, "truncated");
        let mut all = Vec::new();
        input
            .scan(|r| {
                r.read_to_end(&mut all).map_err(crate::edit_output::io)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(all, b"firstsecond");
    }
}
#[test]
fn blocks_scan_and_ranges_never_pin_frames() {
    let input = Input::open(fixture(true)).unwrap().unwrap();
    input
        .scan_uncached(|reader| {
            let mut bytes = Vec::new();
            reader
                .read_to_end(&mut bytes)
                .map_err(crate::edit_output::io)?;
            assert_eq!(bytes, b"firstsecond");
            reader
                .seek(SeekFrom::Start(3))
                .map_err(crate::edit_output::io)?;
            let mut cross = [0; 6];
            reader
                .read_exact(&mut cross)
                .map_err(crate::edit_output::io)?;
            assert_eq!(&cross, b"stseco");
            Ok(())
        })
        .unwrap();
    assert_eq!(input.range_uncached(3, 6).unwrap(), b"stseco");
    assert!(input.frames.iter().all(|frame| frame.data.get().is_none()));
    assert_eq!(input.stats()["source_frames_decoded"], 0);
    // Selective callers still retain only their explicitly touched frames.
    input.range(0, 5).unwrap();
    assert_eq!(input.stats()["source_frames_decoded"], 1);
}
#[test]
fn damaged_seek_footer_is_not_downgraded() {
    let file = fixture(false);
    let at = file.metadata().unwrap().len() - 1;
    let mut b = [0];
    file.read_exact_at(&mut b, at).unwrap();
    b[0] ^= 1;
    file.write_all_at(&b, at).unwrap();
    assert_eq!(Input::open(file).unwrap_err().kind, "seek_table");
}
#[test]
fn decoded_extent_and_optional_table_hash_are_checked() {
    for hash in [false, true] {
        let file = fixture(hash);
        let input = Input::open(file.try_clone().unwrap()).unwrap().unwrap();
        let table = input.frames.iter().map(|f| f.compressed).sum::<usize>();
        let at = table + if hash { 16 } else { 12 };
        let mut word = [0; 4];
        file.read_exact_at(&mut word, at as u64).unwrap();
        word[0] ^= 1;
        file.write_all_at(&word, at as u64).unwrap();
        let input = Input::open(file).unwrap().unwrap();
        assert_eq!(input.range(0, 5).unwrap_err().kind, "seek_table");
    }
}
#[test]
fn untouched_corruption_is_not_certified_but_touched_checksum_fails() {
    let file = fixture(false);
    let input = Input::open(file.try_clone().unwrap()).unwrap().unwrap();
    let second = &input.frames[1];
    let at = second.physical + second.compressed - 1;
    let mut b = [0];
    file.read_exact_at(&mut b, at as u64).unwrap();
    b[0] ^= 1;
    file.write_all_at(&b, at as u64).unwrap();
    let input = Input::open(file).unwrap().unwrap();
    assert_eq!(input.range(0, 5).unwrap().as_ref(), b"first");
    assert_eq!(input.range(5, 6).unwrap_err().kind, "seek_table");
    let error = input
        .scan(|r| {
            r.seek(SeekFrom::Start(5)).unwrap();
            r.read_exact(&mut [0; 6]).map_err(crate::edit_output::io)?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind, "seek_table");
}
