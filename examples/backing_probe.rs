//! public API and live-FD ownership proof on caller-owned local fixtures.
use oxblend::BlendFile;
use std::borrow::Cow;
use std::io::Write;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let original = PathBuf::from(args.next().ok_or("fixture required")?);
    let replacement = PathBuf::from(args.next().ok_or("different replacement required")?);
    let dir = PathBuf::from(args.next().ok_or("owned scratch directory required")?);
    std::fs::create_dir(&dir)?;
    let path = dir.join("live.blend");
    let body = std::fs::read(original)?;
    let snapshot = BlendFile::from_bytes(body.clone())?;
    std::fs::write(&path, &body)?;
    let live = BlendFile::open(&path)?;
    assert_eq!(live.decompressed_len(), body.len());
    assert_eq!(snapshot.read_decompressed_bytes()?, body);
    assert_eq!(live.read_decompressed_bytes()?, body);
    for i in 0..live.blocks().len() {
        assert_eq!(live.payload(i)?, snapshot.payload(i)?);
        assert!(matches!(live.payload(i)?, Cow::Owned(_)));
        assert!(matches!(snapshot.payload(i)?, Cow::Borrowed(_)));
    }
    // An open raw descriptor pins its inode, not the future pathname target.
    let moved = dir.join("original-inode.blend");
    std::fs::rename(&path, &moved)?;
    std::fs::copy(replacement, &path)?;
    assert_eq!(live.read_decompressed_bytes()?, body);
    // In-place writes are visible, not claimed to be a snapshot. Use an opaque
    // payload byte so indexed framing/layout invariants themselves remain intact.
    let block = live
        .blocks()
        .iter()
        .position(|b| &b.code == b"JUNK")
        .ok_or("JUNK fixture required")?;
    let offset = live.block(block)?.data_offset;
    let mut writer = std::fs::OpenOptions::new().write(true).open(&moved)?;
    std::io::Seek::seek(&mut writer, std::io::SeekFrom::Start(offset.try_into()?))?;
    let changed = body[offset] ^ u8::MAX;
    writer.write_all(&[changed])?;
    assert_eq!(live.payload(block)?[0], changed);
    assert_eq!(snapshot.payload(block)?[0], body[offset]);
    let typed = live
        .blocks()
        .iter()
        .position(|b| &b.code[..2] == b"OB")
        .ok_or("object fixture required")?;
    // Keep the requested ID-name bytes, but remove the record's tail. Typed
    // access must validate the entire declared extent against the live inode.
    let ob = live.block(typed)?;
    writer.set_len((ob.data_offset + ob.size - 1).try_into()?)?;
    assert_eq!(live.id_name(typed).unwrap_err().kind, "truncated");
    writer.set_len(0)?;
    assert_eq!(live.payload(block).unwrap_err().kind, "truncated");
    assert_eq!(
        live.read_decompressed_bytes().unwrap_err().kind,
        "truncated"
    );
    assert_eq!(live.id_name(typed).unwrap_err().kind, "truncated");
    assert_eq!(snapshot.read_decompressed_bytes()?, body);
    println!(
        "{{\"status\":\"pass\",\"explicit_full_read\":true,\"cow_storage\":true,\"replacement_pins_inode\":true,\"in_place_write_is_live\":true,\"truncation_checked\":true,\"immutable_snapshot\":true}}"
    );
    Ok(())
}
