#[path = "support/scratch.rs"]
mod test_scratch;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let dir = test_scratch::scratch_dir(&std::env::temp_dir());
        Self(dir)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn n16(out: &mut Vec<u8>, n: usize) {
    out.extend((n as u16).to_le_bytes());
}
fn n32(out: &mut Vec<u8>, n: usize) {
    out.extend((n as u32).to_le_bytes());
}
fn align(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}
fn block(code: &[u8; 4], sdna: usize, address: u64, data: &[u8]) -> Vec<u8> {
    let mut out = code.to_vec();
    n32(&mut out, sdna);
    out.extend(address.to_le_bytes());
    out.extend((data.len() as u64).to_le_bytes());
    out.extend(if code == b"ENDB" { 0u64 } else { 1u64 }.to_le_bytes());
    out.extend(data);
    out
}
fn fixture(dir: &Path, sites: usize, assets_first: bool, cache_pointer: u64) -> PathBuf {
    let names = [
        "*next",
        "*prev",
        "name[64]",
        "*lib",
        "*first",
        "*last",
        "id",
        "*adt",
        "*data",
        "*parent",
        "totcol",
        "transflag",
        "_pad[2]",
        "**mat",
        "*proxy",
        "*proxy_group",
        "*proxy_from",
        "*poselib",
        "*pose",
        "*soft",
        "*pd",
        "*dup_group",
        "particlesystem",
        "modifiers",
        "constraints",
        "type",
        "_pad[4]",
        "modifier",
        "*cache_file",
        "filepath[256]",
        "is_sequence",
    ];
    let types = [
        "char",
        "int",
        "short",
        "void",
        "ID",
        "Library",
        "ListBase",
        "Object",
        "ModifierData",
        "MeshSeqCacheModifierData",
        "CacheFile",
        "Volume",
    ];
    let specs = [
        (4, vec![(4, 0), (4, 1), (0, 2), (5, 3)]),
        (6, vec![(3, 4), (3, 5)]),
        (
            7,
            vec![
                (4, 6),
                (3, 7),
                (4, 8),
                (7, 9),
                (1, 10),
                (2, 11),
                (0, 12),
                (4, 13),
                (7, 14),
                (7, 15),
                (7, 16),
                (4, 17),
                (3, 18),
                (3, 19),
                (3, 20),
                (4, 21),
                (6, 22),
                (6, 23),
                (6, 24),
            ],
        ),
        (8, vec![(8, 0), (8, 1), (1, 25), (0, 2), (0, 26)]),
        (9, vec![(8, 27), (10, 28)]),
        (10, vec![(4, 6), (0, 29), (1, 30), (0, 26)]),
        (11, vec![(4, 6), (0, 29), (1, 30), (0, 26)]),
    ];
    let lengths = [1, 4, 2, 0, 88, 0, 16, 240, 88, 96, 352, 352];
    let mut dna = b"SDNANAME".to_vec();
    n32(&mut dna, names.len());
    for name in names {
        dna.extend(name.as_bytes());
        dna.push(0);
    }
    align(&mut dna);
    dna.extend(b"TYPE");
    n32(&mut dna, types.len());
    for ty in types {
        dna.extend(ty.as_bytes());
        dna.push(0);
    }
    align(&mut dna);
    dna.extend(b"TLEN");
    for length in lengths {
        n16(&mut dna, length);
    }
    align(&mut dna);
    dna.extend(b"STRC");
    n32(&mut dna, specs.len());
    for (ty, fields) in specs {
        n16(&mut dna, ty);
        n16(&mut dna, fields.len());
        for (ty, name) in fields {
            n16(&mut dna, ty);
            n16(&mut dna, name);
        }
    }
    let ident = |name: &[u8], size: usize| {
        let mut data = vec![0; size];
        data[16..16 + name.len()].copy_from_slice(name);
        data
    };
    let mut object = ident(b"OBsites", 240);
    if sites > 0 {
        object[208..216].copy_from_slice(&0x2000u64.to_le_bytes());
        object[216..224].copy_from_slice(&(0x2000 + (sites as u64 - 1) * 0x100).to_le_bytes());
    }
    let mut objects = block(b"OB\0\0", 2, 0x1000, &object);
    for i in 0..sites {
        let mut modifier = vec![0; 96];
        let next = if i + 1 == sites {
            0
        } else {
            0x2000 + (i as u64 + 1) * 0x100
        };
        modifier[..8].copy_from_slice(&next.to_le_bytes());
        modifier[16..20].copy_from_slice(&52i32.to_le_bytes());
        modifier[88..96].copy_from_slice(&cache_pointer.to_le_bytes());
        objects.extend(block(b"DATA", 4, 0x2000 + i as u64 * 0x100, &modifier));
    }
    let mut volume = ident(b"VOvolume", 352);
    volume[88..101].copy_from_slice(b"//density.vdb");
    let mut cache = ident(b"CFcache", 352);
    cache[88..98].copy_from_slice(b"//anim.abc");
    let mut assets = block(b"VO\0\0", 6, 0x8000, &volume);
    assets.extend(block(b"CF\0\0", 5, 0x9000, &cache));
    let mut out = b"BLENDER17-01v0502".to_vec();
    if assets_first {
        out.extend(assets);
        out.extend(objects);
    } else {
        out.extend(objects);
        out.extend(assets);
    }
    out.extend(block(b"DNA1", 0, 0, &dna));
    out.extend(block(b"ENDB", 0, 0, &[]));
    let path = dir.join("sites.blend");
    std::fs::write(&path, out).unwrap();
    std::fs::write(dir.join("anim.abc"), b"abc").unwrap();
    std::fs::write(dir.join("density.vdb"), b"vdb").unwrap();
    path
}
fn candidate_binary() -> std::ffi::OsString {
    std::env::var_os("OXBLEND_TEST_BINARY").unwrap_or_else(|| env!("CARGO_BIN_EXE_oxblend").into())
}
fn cli(args: &[&std::ffi::OsStr]) -> std::process::Output {
    Command::new(candidate_binary())
        .args(args)
        .output()
        .unwrap()
}
fn trace(path: &Path, perfile: bool) -> (std::process::ExitStatus, Value) {
    let mut args = vec![std::ffi::OsStr::new("trace"), path.as_os_str()];
    if perfile {
        args.push(std::ffi::OsStr::new("--per-file"));
    }
    let r = cli(&args);
    (r.status, serde_json::from_slice(&r.stdout).unwrap())
}
#[test]
fn physical_write_owner_and_reference_site_order_are_not_a_usage_sort() {
    for assets_first in [false, true] {
        let work = Work::new();
        let path = fixture(&work.0, 2, assets_first, 0x9000);
        let (status, recursive) = trace(&path, false);
        assert!(status.success(), "{recursive}");
        let (status, perfile) = trace(&path, true);
        assert!(status.success(), "{perfile}");
        let codes = |j: &Value| {
            j["usages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|u| u["block_type"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        if assets_first {
            assert_eq!(codes(&recursive), ["Volume", "CacheFile"]);
            assert_eq!(
                codes(&perfile),
                ["Volume", "CacheFile", "CacheFile", "CacheFile"]
            );
        } else {
            assert_eq!(codes(&recursive), ["CacheFile", "Volume"]);
            assert_eq!(
                codes(&perfile),
                ["CacheFile", "CacheFile", "Volume", "CacheFile"]
            );
        }
        let caches: Vec<_> = perfile["usages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|u| u["block_type"] == "CacheFile")
            .collect();
        assert_eq!(caches.len(), 3);
        assert!(caches.iter().all(|u| *u == caches[0]));
        assert_eq!(
            caches[0]["field_provenance"]["full"]["block_address"],
            "36864"
        );
        assert_eq!(caches[0]["block_name_hex"], "43466361636865");
        let r = cli(&["list".as_ref(), path.as_os_str()]);
        assert!(r.status.success());
        let listing = String::from_utf8(r.stdout).unwrap();
        let abc = listing.find("anim.abc").unwrap();
        let vdb = listing.find("density.vdb").unwrap();
        assert_eq!(abc < vdb, !assets_first);
        let target = work.0.join("packed");
        let r = cli(&[
            "pack".as_ref(),
            path.as_os_str(),
            target.as_os_str(),
            "-p".as_ref(),
            work.0.as_os_str(),
        ]);
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stdout));
        let packed: Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(packed["root_traces"][0]["trace"], recursive);
        assert_eq!(std::fs::read(target.join("anim.abc")).unwrap(), b"abc");
        assert_eq!(std::fs::read(target.join("density.vdb")).unwrap(), b"vdb");
    }
}
#[test]
fn dangling_cache_asset_pointer_is_not_ignored() {
    let work = Work::new();
    let path = fixture(&work.0, 1, false, 0xdead);
    for perfile in [false, true] {
        let (status, j) = trace(&path, perfile);
        assert!(!status.success());
        assert_eq!(j["complete"], false);
        assert_eq!(j["errors"][0]["kind"], "dangling_pointer");
        assert_eq!(
            j["errors"][0]["field_path_hex"],
            serde_json::json!(["63616368655f66696c65"])
        );
        assert_eq!(j["errors"][0]["status"], "unknown");
        assert_eq!(j["usages"].as_array().unwrap().len(), 2);
    }
}
#[test]
fn cache_site_reads_its_path_not_recursive_asset_dispatch() {
    let work = Work::new();
    let path = fixture(&work.0, 1, false, 0x1000);
    for perfile in [false, true] {
        let (status, j) = trace(&path, perfile);
        assert!(!status.success());
        assert_eq!(j["errors"][0]["kind"], "missing_field");
        assert_eq!(
            j["errors"][0]["field_path_hex"],
            serde_json::json!(["69735f73657175656e6365"])
        );
        assert_eq!(j["usages"].as_array().unwrap().len(), 2);
    }
}
