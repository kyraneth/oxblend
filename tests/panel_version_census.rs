#[path = "support/scratch.rs"]
mod test_scratch;
use oxblend::BlendFile;
use oxblend::trace::{PointerMode, TraceMode, TraceOptions, trace};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/panel-version-census")
}
fn temp(_tag: &str) -> PathBuf {
    test_scratch::scratch_dir(&std::env::temp_dir())
}
fn options() -> TraceOptions {
    TraceOptions::new(TraceMode::PerFile, PointerMode::Strict)
}
fn modern(legacy: &[u8]) -> Vec<u8> {
    let mut out = b"BLENDER17-01v0405".to_vec();
    let mut pos = 12;
    while pos < legacy.len() {
        let code = &legacy[pos..pos + 4];
        let size = u32::from_le_bytes(legacy[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let address = &legacy[pos + 8..pos + 16];
        let sdna = &legacy[pos + 16..pos + 20];
        let count = u32::from_le_bytes(legacy[pos + 20..pos + 24].try_into().unwrap());
        out.extend_from_slice(code);
        out.extend_from_slice(sdna);
        out.extend_from_slice(address);
        out.extend_from_slice(&(size as u64).to_le_bytes());
        out.extend_from_slice(&(count as u64).to_le_bytes());
        pos += 24;
        out.extend_from_slice(&legacy[pos..pos + size]);
        pos += size;
    }
    out
}
#[test]
fn every_decimal_header_value_keeps_checked_asset_reads() {
    let root = temp("versions");
    let path = root.join("same.blend");
    let legacy = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trace-owner/cross-id.blend"),
    )
    .unwrap();
    let dangling = std::fs::read(fixtures().join("assets/dangling-asset.blend")).unwrap();
    for (source, offset, width, negative) in [
        (&legacy, 9, 3, false),
        (&modern(&legacy), 13, 4, false),
        (&dangling, 9, 3, true),
        (&modern(&dangling), 13, 4, true),
    ] {
        // Header's decimal field widths impose these ranges; no sampled versions.
        for version in 0..10u32.pow(width as u32) {
            let mut data = source.clone();
            data[offset..offset + width].copy_from_slice(format!("{version:0width$}").as_bytes());
            assert_eq!(
                BlendFile::from_bytes(data.clone())
                    .unwrap()
                    .header()
                    .version,
                version
            );
            std::fs::write(&path, data).unwrap();
            let t = trace(&path, options()).unwrap();
            if negative {
                assert!(!t.complete, "version {version} silenced the asset pointer");
                let report = t.json();
                assert!(
                    report["errors"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|e| e["kind"] == "dangling_pointer"
                            && e["field_path_hex"] == serde_json::json!(["696d616765"])),
                    "version {version}: {report}"
                );
                continue;
            }
            // #31 counts the two absent physical cache paths as incomplete.
            assert!(!t.complete, "version {version}");
            assert_eq!(t.json()["missing_path_count"], 2);
            assert!(t.errors.is_empty());
            assert_eq!(
                t.usages
                    .iter()
                    .map(|u| u.asset_path.as_slice())
                    .collect::<Vec<_>>(),
                [b"//A.pc2".as_slice(), b"//B.pc2".as_slice()],
                "version {version}"
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
