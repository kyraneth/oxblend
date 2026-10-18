#[path = "support/scratch.rs"]
mod test_scratch;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
fn candidate_binary() -> PathBuf {
    std::env::var_os("OXBLEND_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_oxblend")))
}
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let base = std::env::var_os("OXBLEND_BLOCKS_RECEIPTS")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = test_scratch::scratch_dir(&base);
        Self(path)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::env::var_os("OXBLEND_BLOCKS_RECEIPTS").is_none() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn head(out: &mut Vec<u8>, code: &[u8; 4], address: u64, data: &[u8], count: u32) {
    out.extend(code);
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(address.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(data);
}
fn align(dna: &mut Vec<u8>) {
    while !dna.len().is_multiple_of(4) {
        dna.push(0);
    }
}
fn fixture() -> Vec<u8> {
    let mut dna = b"SDNANAME".to_vec();
    dna.extend(3u32.to_le_bytes());
    dna.extend(b"value\0pad\0*next\0");
    align(&mut dna);
    dna.extend(b"TYPE");
    dna.extend(2u32.to_le_bytes());
    dna.extend(b"int\0Node\0");
    align(&mut dna);
    dna.extend(b"TLEN");
    dna.extend(4u16.to_le_bytes());
    dna.extend(16u16.to_le_bytes());
    dna.extend(b"STRC");
    dna.extend(1u32.to_le_bytes());
    for n in [1u16, 3, 0, 0, 0, 1, 1, 2] {
        dna.extend(n.to_le_bytes());
    }
    let mut out = b"BLENDER-v405".to_vec();
    let record = |value: u32, next: u64| {
        let mut r = value.to_le_bytes().to_vec();
        r.extend(0u32.to_le_bytes());
        r.extend(next.to_le_bytes());
        r
    };
    // Same type with different codes, tied sizes, upper-median vectors,
    // opaque excess payload, and a pointer in record 1 that is NOT inspected.
    head(&mut out, b"AA\0\0", 0x1000, &record(1, 0x2000), 1);
    let mut two = record(2, 0);
    two.extend(record(3, 0x1000));
    head(&mut out, b"BB\0\0", 0x2000, &two, 2);
    head(&mut out, b"AA\0\0", 0x3000, &record(4, 0), 1);
    head(&mut out, b"CC\0\0", 0x4000, &[0; 32], 0);
    head(&mut out, b"DNA1", 0, &dna, 1);
    head(&mut out, b"ENDB", 0, &[], 0);
    out
}
fn seekable(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut table = Vec::new();
    let mut count = 0u32;
    // Authored frame boundaries deliberately split BLEND/BHead/record/DNA.
    for chunk in bytes.chunks(19) {
        let mut encoder = zstd::stream::Encoder::new(Vec::new(), 0).unwrap();
        encoder.include_checksum(true).unwrap();
        encoder.write_all(chunk).unwrap();
        let compressed = encoder.finish().unwrap();
        table.extend((compressed.len() as u32).to_le_bytes());
        table.extend((chunk.len() as u32).to_le_bytes());
        out.extend(compressed);
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
fn families() -> Vec<(&'static str, Vec<u8>)> {
    families_of(fixture())
}
fn families_of(raw: Vec<u8>) -> Vec<(&'static str, Vec<u8>)> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&raw).unwrap();
    vec![
        ("raw", raw.clone()),
        ("gzip", encoder.finish().unwrap()),
        ("zstd", zstd::stream::encode_all(raw.as_slice(), 0).unwrap()),
        ("seek-zstd", seekable(&raw)),
    ]
}
fn run(binary: &Path, path: &Path, flags: &[&str]) -> Output {
    Command::new(binary)
        .arg("blocks")
        .arg(path)
        .args(flags)
        .output()
        .unwrap()
}
fn normalized(output: &[u8], path: &Path) -> String {
    String::from_utf8_lossy(output).replace(path.to_str().unwrap(), "<INPUT>")
}
#[test]
fn all_containers_keep_default_limit_address_dump_and_record_zero_semantics() {
    let work = Work::new();
    let binary_path = candidate_binary();
    let binary = binary_path.as_path();
    for flags in [
        vec![],
        vec!["--limit", "0"],
        vec!["--limit", "-1"],
        vec!["--limit", "99"],
        vec!["--address=0x2000"],
        vec!["--address=4096", "--dump"],
        vec!["--address=0xdead"],
    ] {
        let mut expected = None;
        for (family, bytes) in families() {
            let path = work.0.join(format!("{family}.blend"));
            std::fs::write(&path, bytes).unwrap();
            let result = run(binary, &path, &flags);
            let actual = (
                result.status.code(),
                normalized(&result.stdout, &path),
                normalized(&result.stderr, &path),
            );
            if let Some(expected) = &expected {
                assert_eq!(&actual, expected, "{family} {flags:?}");
            } else {
                expected = Some(actual);
            }
            if !flags.contains(&"--address=0xdead") {
                assert!(result.status.success());
            }
            if flags.contains(&"--address=0x2000") {
                assert!(String::from_utf8_lossy(&result.stdout).contains("b'next'"));
            }
            if flags.contains(&"--address=4096") {
                assert!(
                    String::from_utf8_lossy(&result.stdout)
                        .contains("Nothing points there\nHexdump:")
                );
            }
        }
    }
}
#[test]
fn advertised_zero_seek_extent_recovers_full_stream() {
    let work = Work::new();
    let raw = fixture();
    let mut bytes = seekable(&raw);
    let table = bytes.len() - 17 - raw.len().div_ceil(19) * 8;
    bytes[table + 12..table + 16].fill(0);
    let plain = work.0.join("plain.blend");
    let damaged = work.0.join("advertised-zero.blend");
    std::fs::write(&plain, raw).unwrap();
    std::fs::write(&damaged, bytes).unwrap();
    let binary = candidate_binary();
    for flags in [vec![], vec!["--dump"], vec!["--limit", "1"]] {
        let expected = run(&binary, &plain, &flags);
        let actual = run(&binary, &damaged, &flags);
        assert!(expected.status.success());
        assert!(actual.status.success(), "{:?}", actual);
        assert_eq!(
            String::from_utf8(actual.stdout)
                .unwrap()
                .replace(damaged.to_str().unwrap(), "<INPUT>"),
            String::from_utf8(expected.stdout)
                .unwrap()
                .replace(plain.to_str().unwrap(), "<INPUT>")
        );
        assert_eq!(actual.stderr, expected.stderr);
    }
}
