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
fn compressed_inner_decode_counts_reconcile_without_l1_span_pairs() {
    use serde_json::Value;
    let path =
        std::env::temp_dir().join(format!("oxblend-decode-structure-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let work = Work(path);
    let body = include_bytes!("fixtures/constraint-data/tar.blend");
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(body).unwrap();
    for (family, bytes, event, command) in [
        ("gzip", gzip.finish().unwrap(), "decode.scan", "blocks"),
        ("seek", seekable(body), "decode.frame", "blocks"),
    ] {
        let input = work.0.join(format!("{family}.blend"));
        std::fs::write(&input, bytes).unwrap();
        let mut counts = Vec::new();
        let mut outputs = Vec::new();
        for level in [1, 2] {
            let traces = work.0.join(format!("{family}-{level}"));
            let mut cli = Command::new(env!("CARGO_BIN_EXE_oxblend"));
            cli.arg(command)
                .arg(&input)
                .env("FLATTRACE_DIR", &traces)
                .env_remove("FLATTRACE_TRACEPARENT")
                .env_remove("FLATTRACE_LEVEL");
            if command == "trace" {
                cli.arg("--low-memory");
            }
            if level == 2 {
                cli.env("FLATTRACE_LEVEL", "2");
            }
            let output = cli.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            outputs.push(output.stdout);
            let files: Vec<_> = std::fs::read_dir(traces).unwrap().collect();
            assert_eq!(files.len(), 1);
            let rows: Vec<Value> = std::fs::read_to_string(files[0].as_ref().unwrap().path())
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            let calls = rows
                .iter()
                .filter(|r| r["event"] == event && r["state"] == "start")
                .count() as u64;
            if level == 1 {
                assert_eq!(calls, 0);
                let hot: Vec<_> = rows
                    .iter()
                    .filter(|r| r["event"] == "hot.summary" && r["data"]["event"] == event)
                    .collect();
                assert!(
                    !hot.is_empty(),
                    "missing {event} aggregate for {family}: {rows:?}"
                );
                assert!(hot.iter().all(|r| r["data"]["bytes"].as_u64().unwrap() > 0));
                if family == "seek" {
                    assert!(
                        hot.iter()
                            .all(|r| r["data"]["input_bytes"].as_u64().unwrap() > 0)
                    );
                }
                let keys: std::collections::HashSet<_> =
                    hot.iter().map(|r| r["span"].as_str().unwrap()).collect();
                assert_eq!(
                    keys.len(),
                    hot.len(),
                    "more than one aggregate per phase/owner"
                );
                counts.push(
                    hot.iter()
                        .map(|r| r["data"]["count"].as_u64().unwrap())
                        .sum::<u64>(),
                );
            } else {
                counts.push(calls);
            }
        }
        assert_eq!(outputs[0], outputs[1]);
        assert!(counts[0] > 0);
        assert_eq!(counts[0], counts[1], "{event} lost work in aggregate");
    }
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
