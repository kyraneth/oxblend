use super::*;
use std::io::Write;

#[test]
fn raw_gzip_and_nonseek_zstd_equal_after_validation() {
    let dir = crate::test_scratch::scratch_dir(&std::env::temp_dir());
    let raw = include_bytes!("../../assets/template-5.2.1.blend");
    let source = dir.join("source.blend");
    std::fs::write(&source, raw).unwrap();
    // Decode the shipped template through the same backing owner; the asset's
    // container format is irrelevant to this independently recompressed control.
    let parsed = BlendFile::open(&source).unwrap();
    let bytes = parsed.data.range(0, parsed.data.len()).unwrap();
    let plain = dir.join("plain.blend");
    std::fs::write(&plain, &bytes).unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&bytes).unwrap();
    let gzip = dir.join("gzip.blend");
    std::fs::write(&gzip, gz.finish().unwrap()).unwrap();
    let zstd = dir.join("nonseek.blend");
    std::fs::write(&zstd, zstd::stream::encode_all(bytes.as_ref(), 0).unwrap()).unwrap();
    let opts = CompareOptions {
        temp_root: dir.clone(),
    };
    let multi = dir.join("multi-member.blend");
    let mut members = Vec::new();
    for part in [&bytes[..bytes.len() / 2], &bytes[bytes.len() / 2..]] {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(part).unwrap();
        members.extend(encoder.finish().unwrap());
    }
    std::fs::write(&multi, members).unwrap();
    for left in [&plain, &gzip, &zstd, &multi] {
        for right in [&plain, &gzip, &zstd, &multi] {
            let report = compare_with_options(left, right, &opts).unwrap();
            assert_eq!(report["complete"], true);
            assert_eq!(report["equal"], true);
            assert_eq!(report["difference_count"], 0);
            assert_eq!(report["differences"], json!([]));
        }
    }
    let bad = dir.join("invalid.blend");
    std::fs::write(&bad, b"not a blend").unwrap();
    assert!(compare_with_options(&bad, &bad, &opts).is_err());
    std::fs::write(&bad, &bytes[..bytes.len() / 2]).unwrap();
    assert!(compare_with_options(&bad, &bad, &opts).is_err());
    // Both parses are mandatory even when decoded streams would differ early.
    assert!(compare_with_options(&plain, &bad, &opts).is_err());
    assert!(compare_with_options(&bad, &plain, &opts).is_err());
    let missing = CompareOptions {
        temp_root: dir.join("missing"),
    };
    assert!(compare_with_options(&gzip, &plain, &missing).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn captured_descriptor_survives_path_replacement_but_rejects_in_place_mutation() {
    let dir = crate::test_scratch::scratch_dir(&std::env::temp_dir());
    let source = dir.join("source.blend");
    std::fs::write(&source, include_bytes!("../../assets/template-5.2.1.blend")).unwrap();
    let c = Capture::open(&source).unwrap();
    let options = CompareOptions {
        temp_root: dir.clone(),
    };
    let parsed = c.parse(&source, &options).unwrap();
    let replacement = dir.join("replacement");
    std::fs::write(&replacement, b"invalid replacement").unwrap();
    // rename changes ctime on the unlinked inode on Linux. Preserve the captured
    // file with a new path to show that reopening the original path is forbidden;
    // a conservative generation refusal is also permitted after namespace churn.
    std::fs::rename(&replacement, &source).unwrap();
    let expected =
        BlendFile::from_bytes(parsed.data.range(0, parsed.data.len()).unwrap().to_vec()).unwrap();
    assert!(decoded_equal(&parsed, &expected).unwrap());
    match c.check() {
        Ok(()) => assert!(decoded_equal(&c.parse(&source, &options).unwrap(), &expected).unwrap()),
        Err(e) => assert_eq!(e.kind, "input_changed"),
    }
    let current = Capture::open(&source).unwrap();
    assert!(current.parse(&source, &options).is_err());
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(&source)
        .unwrap();
    f.set_len(0).unwrap();
    assert_eq!(current.check().unwrap_err().kind, "input_changed");
    // Compressed-source guards must survive decoding into a different inode.
    let compressed = dir.join("compressed.blend");
    let bytes = parsed.data.range(0, parsed.data.len()).unwrap();
    std::fs::write(
        &compressed,
        zstd::stream::encode_all(bytes.as_ref(), 0).unwrap(),
    )
    .unwrap();
    let cap = Capture::open(&compressed).unwrap();
    let _decoded = cap.parse(&compressed, &options).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&compressed)
        .unwrap()
        .set_len(0)
        .unwrap();
    assert_eq!(cap.check().unwrap_err().kind, "input_changed");
    std::fs::remove_dir_all(dir).unwrap();
}
