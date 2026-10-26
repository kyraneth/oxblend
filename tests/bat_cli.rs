//! real CLI entry tests on authored SDNA; native families are separate.
#[path = "support/scratch.rs"]
mod test_scratch;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let base = std::env::var_os("OXBLEND_TEST_RECEIPTS")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let p = test_scratch::scratch_dir(&base);
        Self(p)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::env::var_os("OXBLEND_TEST_RECEIPTS").is_none() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn u16s(b: &mut Vec<u8>, n: usize) {
    b.extend((n as u16).to_le_bytes())
}
fn u32s(b: &mut Vec<u8>, n: usize) {
    b.extend((n as u32).to_le_bytes())
}
fn head(b: &mut Vec<u8>, code: &[u8; 4], data: &[u8], address: u64, sdna: usize) {
    b.extend(code);
    u32s(b, data.len());
    b.extend(address.to_le_bytes());
    u32s(b, sdna);
    u32s(b, if code == b"ENDB" { 0 } else { 1 });
    b.extend(data);
}
fn fixture(dir: &Path, paths: &[(&[u8], i32)]) -> PathBuf {
    resource_fixture(dir, paths, &[], &[], &[], None)
}
// Authored metadata only, extending this file's original Image helper with
// observed Library/ID-proxy, MovieClip and Image.id.properties contracts.
fn resource_fixture(
    dir: &Path,
    paths: &[(&[u8], i32)],
    movies: &[&[u8]],
    libraries: &[&[u8]],
    proxies: &[(&[u8], usize)],
    properties: Option<u64>,
) -> PathBuf {
    let names = [
        "*next",
        "*prev",
        "name[64]",
        "*lib",
        "id",
        "name[256]",
        "*packedfile",
        "source",
        "_pad[4]",
        "*properties",
        "tiles",
        "*first",
        "*last",
        "tile_number",
    ];
    let types = [
        "char",
        "int",
        "ID",
        "Image",
        "Library",
        "PackedFile",
        "IDProperty",
        "MovieClip",
        "ListBase",
        "ImageTile",
        "void",
    ];
    let id_size = if properties.is_some() { 96 } else { 88 };
    let mut dna = b"SDNANAME".to_vec();
    u32s(&mut dna, names.len());
    for n in names {
        dna.extend(n.as_bytes());
        dna.push(0)
    }
    while !dna.len().is_multiple_of(4) {
        dna.push(0)
    }
    dna.extend(b"TYPE");
    u32s(&mut dna, types.len());
    for t in types {
        dna.extend(t.as_bytes());
        dna.push(0)
    }
    while !dna.len().is_multiple_of(4) {
        dna.push(0)
    }
    dna.extend(b"TLEN");
    for n in [
        1,
        4,
        id_size,
        id_size + 288,
        id_size + 256,
        0,
        0,
        id_size + 256,
        16,
        24,
        0,
    ] {
        u16s(&mut dna, n)
    }
    while !dna.len().is_multiple_of(4) {
        dna.push(0)
    }
    dna.extend(b"STRC");
    u32s(&mut dna, 6);
    let mut id_fields = vec![(2, 0), (2, 1), (0, 2), (4, 3)];
    if properties.is_some() {
        id_fields.push((6, 9));
    }
    for (ty, fields) in [
        (2, id_fields),
        (3, vec![(2, 4), (0, 5), (5, 6), (1, 7), (0, 8), (8, 10)]),
        (4, vec![(2, 4), (0, 5)]),
        (7, vec![(2, 4), (0, 5)]),
        (8, vec![(10, 11), (10, 12)]),
        (9, vec![(9, 0), (9, 1), (1, 13), (0, 8)]),
    ] {
        u16s(&mut dna, ty);
        u16s(&mut dna, fields.len());
        for (t, n) in fields {
            u16s(&mut dna, t);
            u16s(&mut dna, n)
        }
    }
    let mut out = b"BLENDER-v405".to_vec();
    let id = |name: &[u8], lib: u64, extra: usize| {
        let mut data = vec![0u8; id_size + extra];
        data[16..16 + name.len()].copy_from_slice(name);
        data[80..88].copy_from_slice(&lib.to_le_bytes());
        if let Some(address) = properties {
            data[88..96].copy_from_slice(&address.to_le_bytes());
        }
        data
    };
    for (i, (path, source)) in paths.iter().enumerate() {
        let mut data = id(format!("IMfixture{i}").as_bytes(), 0, 288);
        data[id_size..id_size + path.len()].copy_from_slice(path);
        data[id_size + 264..id_size + 268].copy_from_slice(&source.to_le_bytes());
        if *source == 6 {
            let first = 0xb000 + i as u64 * 0x100;
            data[id_size + 272..id_size + 280].copy_from_slice(&first.to_le_bytes());
            data[id_size + 280..id_size + 288].copy_from_slice(&(first + 0x40).to_le_bytes());
        }
        head(&mut out, b"IM\0\0", &data, 0x1000 + i as u64 * 0x100, 1);
        if *source == 6 {
            let first = 0xb000 + i as u64 * 0x100;
            for tile in 0..2u64 {
                let mut saved = vec![0u8; 24];
                if tile == 0 {
                    saved[..8].copy_from_slice(&(first + 0x40).to_le_bytes());
                } else {
                    saved[8..16].copy_from_slice(&first.to_le_bytes());
                }
                saved[16..20].copy_from_slice(&(1001i32 + tile as i32).to_le_bytes());
                head(&mut out, b"DATA", &saved, first + tile * 0x40, 5);
            }
        }
    }
    for (i, path) in libraries.iter().enumerate() {
        let mut data = id(format!("LIlib{i}").as_bytes(), 0, 256);
        data[id_size..id_size + path.len()].copy_from_slice(path);
        head(&mut out, b"LI\0\0", &data, 0x8000 + i as u64 * 0x100, 2);
    }
    for (i, (name, library)) in proxies.iter().enumerate() {
        let data = id(name, 0x8000 + *library as u64 * 0x100, 0);
        head(&mut out, b"ID\0\0", &data, 0x9000 + i as u64 * 0x100, 0);
    }
    for (i, path) in movies.iter().enumerate() {
        let mut data = id(format!("MCmovie{i}").as_bytes(), 0, 256);
        data[id_size..id_size + path.len()].copy_from_slice(path);
        head(&mut out, b"MC\0\0", &data, 0xa000 + i as u64 * 0x100, 3);
    }
    head(&mut out, b"DNA1", &dna, 0, 0);
    head(&mut out, b"ENDB", &[], 0, 0);
    let p = dir.join("source.blend");
    std::fs::write(&p, out).unwrap();
    p
}
fn cli(args: &[&str], paths: &[&Path]) -> std::process::Output {
    let mut c = Command::new(
        std::env::var_os("OXBLEND_TEST_BINARY")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_oxblend").into()),
    );
    c.args(args);
    for p in paths {
        c.arg(p);
    }
    c.output().unwrap()
}
fn parsed(r: &std::process::Output) -> Value {
    assert!(
        r.status.success(),
        "{} {}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr)
    );
    serde_json::from_slice(&r.stdout).unwrap()
}
#[test]
fn serialized_recursive_star_ancestors_use_full_glob_grammar() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//**/a*[x]?.png", 2)]);
    std::fs::create_dir(w.0.join("nested")).unwrap();
    let a = w.0.join("a001xZ.png");
    let b = w.0.join("nested/a002xZ.png");
    for p in [&a, &b, &w.0.join("a001[x]?.png")] {
        std::fs::write(p, []).unwrap();
    }
    let output = cli(&["list", "--json"], &[&src]);
    assert_eq!(output.status.code(), Some(2));
    let candidate: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(candidate["complete"], false);
    assert_eq!(candidate["errors"][0]["kind"], "pattern_members_unknown");
    assert_eq!(
        candidate["files"][src.to_str().unwrap()],
        serde_json::json!([{ "path":a, "missing":false }, { "path":b, "missing":false }])
    );
}
#[test]
fn serialized_repeated_stars_keep_broad_byte_safe_membership() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//a**.png", 2)]);
    let literal = w.0.join("a*literal.png");
    let numbered = w.0.join("a001.png");
    std::fs::write(&literal, []).unwrap();
    std::fs::write(&numbered, []).unwrap();
    std::fs::write(w.0.join("b001.png"), []).unwrap();
    let output = cli(&["list", "--json"], &[&src]);
    assert_eq!(output.status.code(), Some(2));
    let candidate: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(candidate["errors"][0]["kind"], "pattern_members_unknown");
    assert_eq!(
        candidate["files"][src.to_str().unwrap()],
        serde_json::json!([{ "path":literal, "missing":false }, { "path":numbered, "missing":false }])
    );
}
#[test]
fn serialized_saved_version_remains_readable_in_read_only_census() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//a.png", 1)]);
    let mut bytes = std::fs::read(&src).unwrap();
    bytes[9..12].copy_from_slice(b"404");
    std::fs::write(&src, bytes).unwrap();
    std::fs::write(w.0.join("a.png"), []).unwrap();
    parsed(&cli(&["list", "--json"], &[&src]));
    let observed = cli(&["pointer-census"], &[&src]);
    let census = parsed(&observed);
    assert_eq!(census["schema"], "oxblend.pointer-census/v2");
    assert_eq!(census["semantic_verified"], false);
}
#[test]
fn serialized_text_preserves_usage_order_json_sorts_dependency_members() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//z.png", 1), (b"//a.png", 1), (b"//z.png", 1)]);
    let z = w.0.join("z.png");
    let a = w.0.join("a.png");
    std::fs::write(&z, []).unwrap();
    std::fs::write(&a, []).unwrap();
    let output = cli(&["list"], &[&src]);
    assert!(output.status.success());
    let lines = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        lines.lines().skip(1).collect::<Vec<_>>(),
        vec![
            format!("    {}", z.display()),
            format!("    {}", a.display())
        ]
    );
    let value = parsed(&cli(&["list", "--json"], &[&src]));
    assert_eq!(value[src.to_str().unwrap()], serde_json::json!([a, z]));
}
#[test]
fn missing_declaration_inside_a_listed_directory_keeps_its_text_marker() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//assets", 1), (b"//assets/missing.png", 1)]);
    std::fs::create_dir(w.0.join("assets")).unwrap();
    let existing = w.0.join("assets/present.png");
    let missing = w.0.join("assets/missing.png");
    std::fs::write(&existing, []).unwrap();
    let text = cli(&["list"], &[&src]);
    assert_eq!(text.status.code(), Some(2));
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(
        text.contains(&format!("    {} [missing]", missing.display())),
        "{text}"
    );
    assert!(text.contains(&format!("    {}", existing.display())));
    let json = cli(&["list", "--json"], &[&src]);
    assert_eq!(json.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(report["complete"], false);
    assert_eq!(
        report["files"][src.to_str().unwrap()],
        serde_json::json!([
            {"path":missing,"missing":true}, {"path":existing,"missing":false}
        ])
    );
}
#[test]
fn list_groups_hash_and_options_before_input() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//asset.png", 1)]);
    let asset = w.0.join("asset.png");
    std::fs::write(&asset, []).unwrap();
    let r = parsed(&cli(&["list", "--json"], &[&src]));
    assert_eq!(r[src.to_str().unwrap()], serde_json::json!([asset]));
    let r = cli(&["list", "--sha256"], &[&src]);
    assert!(r.status.success());
    assert!(
        String::from_utf8(r.stdout)
            .unwrap()
            .contains("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
    );
    let r = cli(&["blocks", "--limit=0"], &[&src]);
    assert!(r.status.success());
    let text = String::from_utf8(r.stdout).unwrap();
    assert!(text.contains("Biggest"));
    assert!(!text.split("Biggest").next().unwrap().contains("Image-IM"));
}
#[test]
fn serialized_full_info_keeps_duplicate_usage_sites_and_byte_safe_paths() {
    use std::os::unix::ffi::OsStringExt;
    let w = Work::new();
    let src = fixture(
        &w.0,
        &[(b"//quote\"\xff.png", 1), (b"//quote\"\xff.png", 1)],
    );
    let asset =
        w.0.join(std::ffi::OsString::from_vec(b"quote\"\xff.png".to_vec()));
    std::fs::write(&asset, b"independently authored resource").unwrap();
    let report = parsed(&cli(&["list", "--json", "--full-info"], &[&src]));
    assert_eq!(report["usages"].as_array().unwrap().len(), 2);
    for usage in report["usages"].as_array().unwrap() {
        assert!(usage.get("asset_path").unwrap().is_null());
        assert!(usage.get("resolved_path").unwrap().is_null());
        assert_eq!(usage["exists"], true);
        assert!(usage["path_full_field"]["name"].is_string());
    }
    let paths = report["files"][src.to_str().unwrap()].as_array().unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(
        paths[0].as_str().unwrap(),
        format!(
            "bytes-hex:{}",
            asset
                .as_os_str()
                .as_encoded_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    );
}
#[test]
fn incomplete_pack_overview_lists_decoded_or_hex_missing_paths() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//texture.png", 1), (b"//bad\xff.png", 1)]);
    for sequence in [false, true] {
        let target = w.0.join(if sequence { "sequence" } else { "single" });
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        command
            .arg(if sequence { "pack-sequence" } else { "pack" })
            .arg(&src);
        if sequence {
            command.arg("-t");
        }
        let out = command.arg(&target).arg("--permissive").output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        let report: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(report["complete"], false);
        let info = std::fs::read_to_string(target.join("pack-info.txt")).unwrap();
        assert!(info.contains("INCOMPLETE"), "{info}");
        assert!(info.contains("Missing files: 2\n"), "{info}");
        for missing in report["missing"].as_array().unwrap() {
            let shown = missing["absolute_path"]
                .as_str()
                .map(|p| format!("{p:?}"))
                .unwrap_or_else(|| {
                    format!("hex:{}", missing["absolute_path_hex"].as_str().unwrap())
                });
            assert!(info.contains(&format!("Missing file: {shown}\n")), "{info}");
        }
    }
}

#[test]
fn serialized_address_refusal_and_packed_reporting() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//packed.png", 1)]);
    let mut data = std::fs::read(&src).unwrap();
    // Authored fixture layout: 12-byte header + 24-byte BHead, then
    // 88-byte ID and 256-byte filename before the packedfile pointer.
    data[380..388].copy_from_slice(&0x1000u64.to_le_bytes());
    std::fs::write(&src, data).unwrap();
    // #31: 0x1000 is an Image ID address, not packed DATA. A nonzero
    // declaration must no longer masquerade as proven packed contents.
    let out = cli(&["list", "--json", "--full-info"], &[&src]);
    assert_eq!(out.status.code(), Some(2));
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["complete"], false);
    assert_eq!(report["problem_counts"]["pointer_target_type"], 1);
    assert_eq!(report["errors"][0]["status"], "unknown");
    assert_eq!(report["packed"][0]["packed"], Value::Null);
    assert!(report["files"].as_object().unwrap().is_empty());
    let absent = cli(&["blocks", "--address=0xdead"], &[&src]);
    assert!(!absent.status.success());
    assert!(!String::from_utf8_lossy(&absent.stdout).contains("Biggest"));
}
#[test]
fn no_op_and_exclusion_do_not_publish_or_rewrite_excluded_absolute() {
    let w = Work::new();
    let asset = w.0.join("asset.png");
    std::fs::write(&asset, b"payload").unwrap();
    let src = fixture(&w.0, &[(asset.as_os_str().as_encoded_bytes(), 1)]);
    let original = std::fs::read(&src).unwrap();
    for flag in ["--noop", "--dry-run"] {
        let target = w.0.join(flag);
        let r = parsed(&cli(&["pack", flag], &[&src, &target]));
        assert_eq!(r["complete"], true);
        assert_eq!(r["directory_created"], false);
        assert!(!target.exists());
        assert!(
            r["files"]
                .as_array()
                .unwrap()
                .iter()
                .all(|f| f["output_written"] == false)
        );
    }
    let target = w.0.join("excluded");
    let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("pack")
        .arg(&src)
        .arg(&target)
        .args(["--exclude", "*.png"])
        .output()
        .unwrap();
    let r = parsed(&r);
    assert_eq!(r["complete"], true);
    assert_eq!(
        std::fs::read(target.join("source.blend")).unwrap(),
        original
    );
    assert_eq!(std::fs::read_dir(target).unwrap().count(), 2);
}
#[test]
fn relative_only_preserves_absolute_and_zip_is_atomic_not_noop_output() {
    let w = Work::new();
    let asset = w.0.join("asset.png");
    std::fs::write(&asset, b"payload").unwrap();
    let src = fixture(&w.0, &[(asset.as_os_str().as_encoded_bytes(), 1)]);
    let original = std::fs::read(&src).unwrap();
    let target = w.0.join("relative");
    let r = parsed(&cli(&["pack", "--relative-only"], &[&src, &target]));
    assert_eq!(r["complete"], true);
    assert_eq!(
        std::fs::read(target.join("source.blend")).unwrap(),
        original
    );
    let target = w.0.join("bundle.zip");
    let r = parsed(&cli(&["pack", "--noop"], &[&src, &target]));
    assert_eq!(r["archive_written"], false);
    assert!(!target.exists());
    let r = parsed(&cli(&["pack"], &[&src, &target]));
    assert_eq!(r["archive_written"], true);
    assert!(std::fs::read(&target).unwrap().starts_with(b"PK\x03\x04"));
    let before = std::fs::read(&target).unwrap();
    assert!(!cli(&["pack"], &[&src, &target]).status.success());
    assert_eq!(std::fs::read(&target).unwrap(), before);
}
#[test]
fn compress_is_seekable_zstd_and_missing_input_is_three() {
    let w = Work::new();
    let src = fixture(&w.0, &[]);
    let target = w.0.join("compressed");
    let r = parsed(&cli(&["pack", "--compress"], &[&src, &target]));
    assert_eq!(r["complete"], true);
    let bytes = std::fs::read(target.join("source.blend")).unwrap();
    assert!(bytes.starts_with(b"\x28\xb5\x2f\xfd"));
    assert!(bytes.ends_with(b"\xb1\xea\x92\x8f"));
    let missing = w.0.join("absent.blend");
    assert_eq!(
        cli(&["pack"], &[&missing, &w.0.join("missing")])
            .status
            .code(),
        Some(3)
    );
    assert!(!w.0.join("missing").exists());
}
#[test]
fn absolute_literal_plus_relative_sequence_can_copy_same_source_twice() {
    let w = Work::new();
    let inputs = w.0.join("inputs");
    std::fs::create_dir(&inputs).unwrap();
    let asset = inputs.join("tile.1001.png");
    std::fs::write(&asset, b"payload").unwrap();
    std::fs::write(inputs.join("tile.1002.png"), b"payload2").unwrap();
    let src = fixture(
        &inputs,
        &[
            (asset.as_os_str().as_encoded_bytes(), 1),
            (b"//tile.<UDIM>.png", 6),
        ],
    );
    let target = w.0.join("aliases");
    let r = parsed(&cli(&["pack"], &[&src, &target]));
    assert_eq!(r["complete"], true);
    let matching = r["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| {
            f["input_path_hex"]
                == asset
                    .as_os_str()
                    .as_encoded_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
        })
        .count();
    assert_eq!(matching, 2);
    assert!(target.join("tile.1001.png").exists());
}
#[test]
fn generic_uri_pack_targets_refuse_as_usage_before_local_interpretation() {
    let w = Work::new();
    let src = fixture(&w.0, &[]);
    let absent = w.0.join("absent.blend");
    // Runs inside the scratch dir so a locally interpreted URI target can
    // only ever create its literal `scheme:` directory there, never in the
    // caller's working directory.
    let run = |args: &[&str], paths: &[&Path]| {
        let mut c = Command::new(
            std::env::var_os("OXBLEND_TEST_BINARY")
                .unwrap_or_else(|| env!("CARGO_BIN_EXE_oxblend").into()),
        );
        c.args(args);
        for p in paths {
            c.arg(p);
        }
        c.current_dir(&w.0).output().unwrap()
    };
    fn refused(output: &std::process::Output, scheme: &str) {
        assert_eq!(
            output.status.code(),
            Some(2),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        let report: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(report["error"]["kind"], "usage");
        let message = report["error"]["message"].as_str().unwrap();
        // The refusal must name its cause (the URI scheme) and give a hint.
        assert!(message.contains("URI"), "{message}");
        assert!(message.contains(scheme), "{message}");
        assert!(message.contains("filesystem"), "{message}");
    }
    // Generic valid schemes are refused for pack and pack-sequence, before
    // the target is interpreted as a local path.
    refused(&run(&["pack"], &[&src, Path::new("foo://farm/out")]), "foo");
    assert!(!w.0.join("foo:").exists());
    refused(
        &run(&["pack-sequence", "-t", "asset-store://farm"], &[&src]),
        "asset-store",
    );
    // Noop must not weaken the refusal: nothing is planned or written.
    refused(
        &run(
            &["pack", "--noop"],
            &[&src, Path::new("smb://server/share")],
        ),
        "smb",
    );
    // The refusal precedes local interpretation of the inputs: a missing
    // input would otherwise be reported as exit 3 after the target check.
    refused(
        &run(&["pack"], &[&absent, Path::new("file://tmp/out")]),
        "file",
    );
    // Known transports keep their own named refusal, excluded from the
    // generic URI message.
    for transport in ["s3", "https"] {
        let out = run(
            &["pack"],
            &[&src, Path::new(&format!("{transport}://bucket/key"))],
        );
        assert_eq!(out.status.code(), Some(2), "{transport}");
        let report: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(report["error"]["kind"], "usage");
        let message = report["error"]["message"].as_str().unwrap();
        assert!(message.contains("Shaman/S3 transport"), "{message}");
    }
    // A leading digit is not a valid scheme start, so this stays a local
    // (if unusual) relative path and must keep packing.
    let local = run(&["pack"], &[&src, Path::new("1x://farm/out")]);
    assert!(
        local.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&local.stderr)
    );
    let report: Value = serde_json::from_slice(&local.stdout).unwrap();
    assert_eq!(report["complete"], true);
    assert!(w.0.join("1x:/farm/out").join("source.blend").exists());
}
#[test]
fn uri_targets_are_refused_at_the_public_pack_api_boundary() {
    // The library boundary refuses URI targets itself, before inputs or the
    // target are read or absolutized, even when the CLI layer is bypassed.
    let error = oxblend::pack::pack(
        &[PathBuf::from("/nonexistent.blend")],
        Path::new("foo://farm/out"),
        &Default::default(),
    )
    .unwrap_err();
    assert_eq!(error.kind, "usage");
    assert!(error.message.contains("foo"), "{}", error.message);
    assert!(error.message.contains("filesystem"), "{}", error.message);
}
