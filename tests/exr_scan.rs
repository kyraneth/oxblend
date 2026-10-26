//! Independent wire fixtures: OpenEXR File Layout facts, no decoder code.
use serde_json::Value;
use std::path::Path;
use std::process::Command;
mod support {
    pub mod scratch;
}
fn attr(b: &mut Vec<u8>, name: &str, kind: &str, v: &[u8]) {
    b.extend(name.bytes());
    b.push(0);
    b.extend(kind.bytes());
    b.push(0);
    b.extend((v.len() as i32).to_le_bytes());
    b.extend(v);
}
fn header(compression: u8, tiled: bool, extra: &[(&str, &str, Vec<u8>)]) -> Vec<u8> {
    let mut b = Vec::new();
    let mut channel = b"R\0".to_vec();
    channel.extend(1i32.to_le_bytes());
    channel.extend([1, 0, 0, 0]);
    channel.extend(1i32.to_le_bytes());
    channel.extend(1i32.to_le_bytes());
    channel.push(0);
    attr(&mut b, "channels", "chlist", &channel);
    attr(&mut b, "compression", "compression", &[compression]);
    for name in ["dataWindow", "displayWindow"] {
        attr(
            &mut b,
            name,
            "box2i",
            &[0i32, 0, 0, 0]
                .into_iter()
                .flat_map(i32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    }
    attr(&mut b, "lineOrder", "lineOrder", &[0]);
    attr(&mut b, "pixelAspectRatio", "float", &1f32.to_le_bytes());
    attr(&mut b, "screenWindowCenter", "v2f", &[0; 8]);
    attr(&mut b, "screenWindowWidth", "float", &1f32.to_le_bytes());
    if tiled {
        let mut t = 1u32.to_le_bytes().to_vec();
        t.extend(1u32.to_le_bytes());
        t.push(0);
        attr(&mut b, "tiles", "tiledesc", &t);
    }
    for (n, k, v) in extra {
        attr(&mut b, n, k, v);
    }
    b.push(0);
    b
}
fn image(comp: u8, tiled: bool, multipart: bool) -> Vec<u8> {
    let mut b = 20000630u32.to_le_bytes().to_vec();
    b.extend(
        (2u32
            | if multipart {
                0x1000
            } else if tiled {
                0x200
            } else {
                0
            })
        .to_le_bytes(),
    );
    let parts: i32 = if multipart { 2 } else { 1 };
    for part in 0..parts {
        let extra = if multipart {
            vec![
                ("name", "string", format!("part{part}").into_bytes()),
                (
                    "type",
                    "string",
                    if tiled {
                        b"tiledimage".to_vec()
                    } else {
                        b"scanlineimage".to_vec()
                    },
                ),
                ("chunkCount", "int", 1i32.to_le_bytes().to_vec()),
            ]
        } else {
            vec![]
        };
        b.extend(header(comp, tiled, &extra));
    }
    if multipart {
        b.push(0);
    }
    let start = b.len() as u64 + 8 * parts as u64;
    let head = if tiled { 20 } else { 8 } + if multipart { 4 } else { 0 };
    for part in 0..parts {
        b.extend((start + part as u64 * (head + 1)).to_le_bytes());
    }
    for part in 0..parts {
        if multipart {
            b.extend(part.to_le_bytes());
        }
        b.extend(vec![0; if tiled { 16 } else { 4 }]);
        b.extend(1i32.to_le_bytes());
        b.push(0);
    }
    b
}
fn scan(paths: &[&Path]) -> (std::process::Output, Vec<Value>) {
    let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("exr-scan")
        .args(paths)
        .args(["--jobs", "2"])
        .output()
        .unwrap();
    let rows = String::from_utf8(out.stdout.clone())
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    (out, rows)
}
#[test]
fn every_compression_scanline_tiled_and_multipart_structure() {
    let dir = support::scratch::scratch_dir(&std::env::temp_dir());
    for comp in 0..14 {
        for tiled in [false, true] {
            for multi in [false, true] {
                let path = dir.join(format!("{comp}-{tiled}-{multi}.exr"));
                std::fs::write(&path, image(comp, tiled, multi)).unwrap();
            }
        }
    }
    let (out, rows) = scan(&[&dir]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(rows.len(), 57);
    for row in &rows[..56] {
        assert_eq!(row["status"], "ok", "{row}");
        assert_eq!(row["placeholder_1x1"], true);
        assert_eq!(row["truncated"], false);
        assert_eq!(row["pixel_integrity_verified"], false);
        assert_eq!(row["extent"], row["header_end"]);
        assert!(row["header_end"].as_u64().unwrap() < row["file_size"].as_u64().unwrap());
        assert_eq!(
            row["chunks_declared"],
            row["parts"].as_array().unwrap().len()
        );
        assert_eq!(row["chunks_present"], row["chunks_declared"]);
        assert_eq!(row["parts"][0]["channels"][0]["pixel_type_name"], "half");
        assert_eq!(row["parts"][0]["channels"][0]["pLinear"], true);
    }
    assert_eq!(rows[56]["counts_by_status"]["ok"], 56);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn distinct_statuses_and_group_counts_via_streamed_cli() {
    let dir = support::scratch::scratch_dir(&std::env::temp_dir());
    let family = dir.join("render/v001/beauty");
    std::fs::create_dir_all(&family).unwrap();
    let full = image(3, false, false);
    let valid = family.join("ok.exr");
    std::fs::write(&valid, &full).unwrap();
    let h = oxblend::exr_header::read(&valid).unwrap().header_end as usize;
    let mut deep = 20000630u32.to_le_bytes().to_vec();
    deep.extend(0x802u32.to_le_bytes());
    deep.extend(header(
        0,
        false,
        &[
            ("type", "string", b"deepscanline".to_vec()),
            ("chunkCount", "int", 1i32.to_le_bytes().to_vec()),
        ],
    ));
    for (name, bytes) in [
        ("zero.exr", vec![]),
        ("foreign.txt", b"not an EXR".to_vec()),
        ("short.exr", full[..30].to_vec()),
        ("header-only.exr", full[..h].to_vec()),
        ("table-only.exr", full[..h + 8].to_vec()),
        ("chunk-short.exr", full[..full.len() - 1].to_vec()),
        ("deep.exr", deep),
    ] {
        std::fs::write(family.join(name), bytes).unwrap();
    }
    let (out, rows) = scan(&[&dir]);
    assert!(out.status.success());
    let summary = rows.last().unwrap();
    assert_eq!(summary["record_type"], "summary");
    assert_eq!(summary["files"], 8);
    for (s, n) in [
        ("zero_byte", 1),
        ("not_exr", 1),
        ("header_truncated", 1),
        ("header_only_no_chunks", 2),
        ("chunks_truncated", 1),
        ("deep_unsupported", 1),
        ("ok", 1),
    ] {
        assert_eq!(summary["counts_by_status"][s], n, "{summary}");
    }
    assert_eq!(summary["truncated"], 4);
    assert_eq!(summary["groups"].as_array().unwrap().len(), 1);
    assert_eq!(summary["groups"][0]["family"], "beauty");
    let truncated = rows
        .iter()
        .find(|r| r["status"] == "chunks_truncated")
        .unwrap();
    assert_eq!(truncated["chunks_present"], 1);
    assert!(
        truncated["bytes_expected"].as_u64().unwrap() > truncated["file_size"].as_u64().unwrap()
    );
    assert_eq!(
        rows.iter()
            .find(|r| r["status"] == "deep_unsupported")
            .unwrap()["truncated"],
        Value::Null
    );
    let (duplicate, dups) = scan(&[&valid, &valid]);
    assert!(duplicate.status.success());
    assert_eq!(dups.last().unwrap()["files"], 1);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn files_from_and_stdin_preserve_literal_native_paths() {
    use std::io::Write;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::process::Stdio;
    let dir = support::scratch::scratch_dir(&std::env::temp_dir());
    let odd_dir = dir.join(std::ffi::OsString::from_vec(b"native-\xff".to_vec()));
    std::fs::create_dir(&odd_dir).unwrap();
    let file = odd_dir.join("line\nbreak.exr");
    std::fs::write(&file, image(0, false, false)).unwrap();
    let mut list = file.as_os_str().as_bytes().to_vec();
    list.extend([0, 0]);
    let source = dir.join("files.nul");
    std::fs::write(&source, &list).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("exr-scan")
        .args(["--jobs", "1", "-0", "--files-from"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows[0]["absolute_path"], Value::Null);
    assert_eq!(rows[0]["status"], "ok");
    assert_eq!(rows[1]["files"], 1);
    assert_eq!(rows[1]["relative_root"], Value::Null);
    assert!(rows[1]["relative_root_hex"].is_string());
    let ordinary = dir.join(" space name.exr");
    std::fs::write(&ordinary, image(0, false, false)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .args(["exr-scan", "--files-from=-", "--jobs=1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "\n{}\n", ordinary.display()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let rows = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.last().unwrap()["files"], 1);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn invalid_jobs_missing_inputs_and_broken_stdout_are_not_success() {
    for args in [
        vec!["exr-scan"],
        vec!["exr-scan", ".", "--jobs", "0"],
        vec!["exr-scan", ".", "--jobs"],
        vec!["exr-scan", ".", "--jobs", "x"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
    let dir = support::scratch::scratch_dir(&std::env::temp_dir());
    let missing = dir.join("missing");
    let (out, rows) = scan(&[&missing]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(rows.last().unwrap()["complete"], false);
    for i in 0..48 {
        std::fs::write(dir.join(format!("{i}.exr")), image(0, false, false)).unwrap();
    }
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("exr-scan")
        .arg(&dir)
        .args(["--jobs", "2"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    std::fs::remove_dir_all(dir).unwrap();
}
