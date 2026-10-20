//! Both actual CLI entry paths and API options preserve output; seek damage recovers explicitly.
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
struct Work(PathBuf);
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn seekable(body: &[u8]) -> Vec<u8> {
    let parts = body
        .chunks(body.len().div_ceil(2))
        .map(|b| (b, b.len() as u32))
        .collect::<Vec<_>>();
    framed(&parts)
}
fn framed(parts: &[(&[u8], u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut table = Vec::new();
    let mut count = 0u32;
    // Authored frame divisions are fixtures, not a processing bound.
    for (bytes, claimed) in parts {
        let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 0).unwrap();
        encoder.include_checksum(true).unwrap();
        encoder.write_all(bytes).unwrap();
        let frame = encoder.finish().unwrap();
        table.extend((frame.len() as u32).to_le_bytes());
        table.extend(claimed.to_le_bytes());
        out.extend(frame);
        count += 1;
    }
    table.extend(count.to_le_bytes());
    table.push(0);
    table.extend(0x8f92eab1u32.to_le_bytes());
    out.extend(0x184d2a5eu32.to_le_bytes());
    out.extend((table.len() as u32).to_le_bytes());
    out.extend(table);
    out
}
#[test]
fn cli_and_api_modes_match_and_damaged_seek_metadata_reports_fallback() {
    let dir = std::env::temp_dir().join(format!("oxblend-low-memory-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let work = Work(dir);
    let input = work.0.join("input.blend");
    let body = include_bytes!("fixtures/constraint-data/tar.blend");
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(body).unwrap();
    let seek = seekable(body);
    let mut damaged = seek.clone();
    *damaged.last_mut().unwrap() ^= 1;
    for (family, bytes) in [
        ("raw", body.to_vec()),
        ("gzip", gzip.finish().unwrap()),
        (
            "zstd",
            zstd::stream::encode_all(body.as_slice(), 0).unwrap(),
        ),
        ("seek", seek),
        ("damaged", damaged),
    ] {
        std::fs::write(&input, bytes).unwrap();
        for command in ["trace", "list"] {
            let mut fast = Command::new(env!("CARGO_BIN_EXE_oxblend"));
            fast.arg(command).arg(&input);
            if command == "list" {
                fast.arg("--json");
            }
            let baseline = fast.output().unwrap();
            let low = fast
                .arg("--low-memory")
                .env("OXBLEND_READ_REPORT", "1")
                .output()
                .unwrap();
            assert_eq!(
                baseline.status.code(),
                low.status.code(),
                "{family} {command}"
            );
            assert_eq!(baseline.stdout, low.stdout, "{family} {command}");
            if family == "damaged" {
                let plain = fast.env_remove("OXBLEND_READ_REPORT").output().unwrap();
                let warning: serde_json::Value =
                    serde_json::from_slice(plain.stderr.split(|b| *b == b'\n').next().unwrap())
                        .unwrap();
                assert_eq!(warning["warning"]["kind"], "seek_table_fallback");
                assert_eq!(
                    warning["warning"]["message"],
                    "untrusted seek table; using checked_full_streaming_spill"
                );
                assert_eq!(warning["warning"]["reason"], "damaged seek footer magic");
                assert_eq!(baseline.stdout, plain.stdout);
                // Retired read-report controls add no stderr; the structured
                // fallback warning survives with exactly the same content.
                assert_eq!(low.stderr, plain.stderr);
                assert!(String::from_utf8_lossy(&low.stderr).contains("damaged seek footer magic"));
            }
        }
        let mut options = oxblend::trace::TraceOptions::default();
        let fast = oxblend::trace::trace(&input, options).unwrap();
        options.low_memory = true;
        let low = oxblend::trace::trace(&input, options).unwrap();
        assert_eq!(fast.json(), low.json(), "{family}");
    }
    let bad = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .args(["trace"])
        .arg(&input)
        .args(["--low-memory", "--low-memory"])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    assert!(bad.stdout.is_empty());
}
#[test]
fn contradictory_zero_frames_and_multifield_footer_damage_never_succeed_silently() {
    let dir = std::env::temp_dir().join(format!(
        "oxblend-low-memory-adversarial-{}",
        std::process::id()
    ));
    std::fs::create_dir(&dir).unwrap();
    let work = Work(dir);
    let input = work.0.join("input.blend");
    let body = include_bytes!("fixtures/constraint-data/tar.blend").as_slice();
    let normal = seekable(body);
    let mut count = normal.clone();
    let end = count.len();
    count[end - 1] ^= 1;
    count[end - 9] ^= 1;
    let mut descriptor = normal.clone();
    let end = descriptor.len();
    descriptor[end - 1] ^= 1;
    descriptor[end - 5] ^= 0x80;
    let mut broken = normal.clone();
    let end = broken.len();
    let table_start = end - (2 * 8 + 9 + 8); // authored two-entry table, spec widths
    let first = u32::from_le_bytes(
        broken[table_start + 8..table_start + 12]
            .try_into()
            .unwrap(),
    ) as usize;
    broken[first - 1] ^= 1;
    broken[end - 1] ^= 1;
    for (name, bytes, valid) in [
        (
            "zero-head",
            framed(&[(b"GARBAGE", 0), (body, body.len() as u32)]),
            false,
        ),
        (
            "zero-tail",
            framed(&[(body, body.len() as u32), (b"GARBAGE", 0)]),
            false,
        ),
        ("magic-count", count, true),
        ("magic-descriptor", descriptor, true),
        ("index-checksum", broken, false),
    ] {
        std::fs::write(&input, bytes).unwrap();
        for command in ["trace", "list"] {
            let fast = Command::new(env!("CARGO_BIN_EXE_oxblend"))
                .arg(command)
                .arg(&input)
                .env_remove("OXBLEND_READ_REPORT")
                .output()
                .unwrap();
            let low = Command::new(env!("CARGO_BIN_EXE_oxblend"))
                .arg(command)
                .arg(&input)
                .arg("--low-memory")
                .env_remove("OXBLEND_READ_REPORT")
                .output()
                .unwrap();
            assert_eq!(fast.status.code(), low.status.code(), "{name} {command}");
            assert_eq!(fast.stdout, low.stdout, "{name} {command}");
            if !valid {
                assert_eq!(low.status.code(), Some(2));
                assert!(low.stdout.is_empty());
            }
            let warning: serde_json::Value =
                serde_json::from_slice(low.stderr.split(|b| *b == b'\n').next().unwrap()).unwrap();
            assert_eq!(
                warning["warning"]["kind"], "seek_table_fallback",
                "{name} {command}"
            );
        }
    }
}
