//! L18: TILED Images whose saved filepath is a concrete tile file.
//!
//! Blender (image.cc:3597 `BKE_image_get_tile_info` + image.cc:3773
//! `BKE_image_ensure_tile_token_filename_only`) requires the `<UDIM>`/`<UVTILE>`
//! token only to SAVE (image_save.cc:667). On load it keeps the saved
//! `Image.tiles` list and auto-tokenizes a concrete basename with its own
//! filename-only regexes before expanding. Oxblend must expand ONLY the saved
//! tile numbers through that derived pattern: never glob the directory to
//! invent members, never treat a directory-only token as UDIM, and never drop
//! the existing UNKNOWN reasons for unavailable/empty/invalid tile lists or
//! untokenizable names.
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
/// Authored SDNA mirror of the layout used by `bat_cli::resource_fixture`:
/// Image carries `name[256]` + `source` + `tiles` ListBase; ImageTile carries
/// `next`/`prev` + `tile_number`. Tile lists are explicit per image.
fn fixture(dir: &Path, images: &[(&[u8], i32, &[i32])]) -> PathBuf {
    let id_size = 88usize;
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
    let id_fields = vec![(2, 0), (2, 1), (0, 2), (4, 3)];
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
        data
    };
    for (i, (path, source, tiles)) in images.iter().enumerate() {
        let mut data = id(format!("IMfixture{i}").as_bytes(), 0, 288);
        data[id_size..id_size + path.len()].copy_from_slice(path);
        data[id_size + 264..id_size + 268].copy_from_slice(&source.to_le_bytes());
        let first = 0xb000 + i as u64 * 0x1000;
        if *source == 6 {
            data[id_size + 272..id_size + 280].copy_from_slice(&first.to_le_bytes());
            let last = first + (tiles.len().saturating_sub(1)) as u64 * 0x40;
            data[id_size + 280..id_size + 288].copy_from_slice(&last.to_le_bytes());
        }
        head(&mut out, b"IM\0\0", &data, 0x1000 + i as u64 * 0x100, 1);
        if *source == 6 {
            for (tile, number) in tiles.iter().enumerate() {
                let mut saved = vec![0u8; 24];
                let address = first + tile as u64 * 0x40;
                // ImageTile layout mirrors `bat_cli::resource_fixture`:
                // `next` first, then `prev`, then `tile_number`.
                if tile + 1 < tiles.len() {
                    saved[..8].copy_from_slice(&(address + 0x40).to_le_bytes());
                }
                if tile > 0 {
                    saved[8..16].copy_from_slice(&(address - 0x40).to_le_bytes());
                }
                saved[16..20].copy_from_slice(&number.to_le_bytes());
                head(&mut out, b"DATA", &saved, address, 5);
            }
        }
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
fn usage_by_asset<'a>(report: &'a Value, suffix: &str) -> &'a Value {
    report["usages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| {
            u["asset_path"]
                .as_str()
                .is_some_and(|p| p.ends_with(suffix))
        })
        .unwrap_or_else(|| panic!("no usage for {suffix}: {}", report["usages"]))
}
fn resolved_names(usage: &Value, key: &str) -> Vec<String> {
    usage[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            PathBuf::from(v.as_str().unwrap())
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}
#[test]
fn blender_saved_concrete_tiles_expand_saved_numbers() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("tests/fixtures/concrete-tiles");
    let w = Work::new();
    let inputs = w.0.join("inputs");
    std::fs::create_dir(&inputs).unwrap();
    for name in [
        "concrete.blend",
        "conc_udim_1005.png",
        "conc_uv_u1_v1.png",
        "conc_uv_u2_v1.png",
        "conc_udim_1001_supplied.png",
    ] {
        std::fs::copy(fixture.join(name), inputs.join(name)).unwrap();
    }
    let src = inputs.join("concrete.blend");
    // conc_udim_1001.png is deliberately absent: missing, not UNKNOWN.
    let output = cli(&["trace"], &[&src]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["complete"], false);
    let udim = usage_by_asset(&report, "conc_udim_1005.png");
    assert_eq!(udim["is_udim"], true);
    assert_eq!(udim["pattern_status"], "missing");
    assert_eq!(
        resolved_names(udim, "expected_members"),
        ["conc_udim_1001.png", "conc_udim_1005.png"]
    );
    assert_eq!(
        resolved_names(udim, "missing_members"),
        ["conc_udim_1001.png"]
    );
    let uv = usage_by_asset(&report, "conc_uv_u2_v1.png");
    assert_eq!(uv["is_udim"], true);
    assert_eq!(uv["pattern_status"], "available");
    assert_eq!(
        resolved_names(uv, "expected_members"),
        ["conc_uv_u1_v1.png", "conc_uv_u2_v1.png"]
    );
    // Supplying the saved tile completes the closure; the strict pack
    // carries exactly the saved tiles.
    std::fs::copy(
        inputs.join("conc_udim_1001_supplied.png"),
        inputs.join("conc_udim_1001.png"),
    )
    .unwrap();
    let output = cli(&["trace"], &[&src]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let target = w.0.join("package");
    let output = cli(&["pack"], &[&src, &target]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["complete"], true);
    for name in [
        "conc_udim_1001.png",
        "conc_udim_1005.png",
        "conc_uv_u1_v1.png",
        "conc_uv_u2_v1.png",
    ] {
        assert!(target.join(name).is_file(), "{name}");
    }
    // Untokenizable basenames and directory-only tokens stay UNKNOWN.
    let w = Work::new();
    std::fs::copy(
        fixture.join("unknown-concrete.blend"),
        w.0.join("unknown-concrete.blend"),
    )
    .unwrap();
    let src = w.0.join("unknown-concrete.blend");
    let output = cli(&["trace"], &[&src]);
    assert_eq!(output.status.code(), Some(2));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["complete"], false);
    for suffix in ["plain.png", "still.png"] {
        let usage = usage_by_asset(&report, suffix);
        assert_eq!(usage["is_udim"], false, "{suffix}");
        assert_eq!(
            usage["pattern_unknown"],
            "tiled Image lacks a supported filename token (<UDIM>/<UVTILE>)",
            "{suffix}"
        );
    }
}
#[test]
fn concrete_udim_path_expands_saved_tiles_with_concrete_missing_list() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//conc_1005.png", 6, &[1001, 1005])]);
    std::fs::write(w.0.join("conc_1005.png"), b"tile").unwrap();
    let output = cli(&["trace"], &[&src]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["complete"], false);
    let usage = usage_by_asset(&report, "conc_1005.png");
    assert_eq!(usage["is_sequence"], true);
    assert_eq!(usage["is_udim"], true);
    assert_eq!(usage["pattern_status"], "missing");
    assert_eq!(
        resolved_names(usage, "expected_members"),
        ["conc_1001.png", "conc_1005.png"]
    );
    assert_eq!(resolved_names(usage, "missing_members"), ["conc_1001.png"]);
    assert!(usage["pattern_unknown"].is_null());
}
#[test]
fn concrete_uvtile_path_expands_saved_tiles() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//tile_u2_v1.png", 6, &[1001, 1002])]);
    std::fs::write(w.0.join("tile_u1_v1.png"), b"one").unwrap();
    std::fs::write(w.0.join("tile_u2_v1.png"), b"two").unwrap();
    let output = cli(&["trace"], &[&src]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["complete"], true);
    let usage = usage_by_asset(&report, "tile_u2_v1.png");
    assert_eq!(usage["is_udim"], true);
    assert_eq!(usage["pattern_status"], "available");
    assert_eq!(
        resolved_names(usage, "expected_members"),
        ["tile_u1_v1.png", "tile_u2_v1.png"]
    );
    assert_eq!(
        resolved_names(usage, "missing_members"),
        Vec::<String>::new()
    );
}
#[test]
fn untokenizable_concrete_name_stays_unknown() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//plain.png", 6, &[1001, 1002])]);
    for command in ["trace", "list", "pack"] {
        let target = w.0.join(format!("plain-{command}"));
        let mut c = Command::new(
            std::env::var_os("OXBLEND_TEST_BINARY")
                .unwrap_or_else(|| env!("CARGO_BIN_EXE_oxblend").into()),
        );
        c.arg(command).arg(&src);
        if command == "list" {
            c.arg("--json");
        }
        if command == "pack" {
            c.arg(&target).arg("--permissive");
        }
        let output = c.output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{command}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("pattern_members_unknown"),
            "{command}"
        );
    }
}
#[test]
fn directory_only_token_stays_non_udim() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//<UDIM>/still.png", 6, &[1001, 1002])]);
    let output = cli(&["trace"], &[&src]);
    assert_eq!(output.status.code(), Some(2));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let usage = usage_by_asset(&report, "still.png");
    assert_eq!(usage["is_udim"], false);
    assert_eq!(
        usage["pattern_unknown"],
        "tiled Image lacks a supported filename token (<UDIM>/<UVTILE>)"
    );
}
#[test]
fn invalid_saved_tile_number_stays_unknown() {
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//conc_1005.png", 6, &[1000, 1005])]);
    std::fs::write(w.0.join("conc_1005.png"), b"tile").unwrap();
    let output = cli(&["trace"], &[&src]);
    assert_eq!(output.status.code(), Some(2));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("invalid saved ImageTile number"), "{text}");
}
#[test]
fn untokenizable_name_reports_token_reason_before_tile_numbers() {
    // Blender tokenizes the basename before touching tiles (image.cc:3605-3606):
    // an untokenizable name keeps the token reason even when a saved tile
    // number is also invalid. Both stay pattern_members_unknown, exit 2.
    let w = Work::new();
    let src = fixture(&w.0, &[(b"//plain.png", 6, &[1000, 1002])]);
    let output = cli(&["trace"], &[&src]);
    assert_eq!(output.status.code(), Some(2));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let usage = usage_by_asset(&report, "plain.png");
    assert_eq!(
        usage["pattern_unknown"],
        "tiled Image lacks a supported filename token (<UDIM>/<UVTILE>)"
    );
}
#[test]
fn strict_pack_of_concrete_tiles_is_incomplete_until_supplied() {
    let w = Work::new();
    // Pack targets must sit outside the source namespace.
    let inputs = w.0.join("inputs");
    std::fs::create_dir(&inputs).unwrap();
    let src = fixture(&inputs, &[(b"//conc_1005.png", 6, &[1001, 1005])]);
    std::fs::write(inputs.join("conc_1005.png"), b"tile").unwrap();
    // A decoy sharing the stem glob must never be invented as a member.
    std::fs::write(inputs.join("conc_decoy.png"), b"decoy").unwrap();
    let target = w.0.join("missing");
    let output = cli(&["pack"], &[&src, &target]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("conc_1001.png"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!target.exists());
    std::fs::write(inputs.join("conc_1001.png"), b"tile").unwrap();
    let target = w.0.join("complete");
    let output = cli(&["pack"], &[&src, &target]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["complete"], true);
    assert!(target.join("conc_1001.png").is_file());
    assert!(target.join("conc_1005.png").is_file());
    assert!(!target.join("conc_decoy.png").exists());
}
