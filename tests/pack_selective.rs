//! L17 Phase 2 slice-6 integration families (public API only):
//! ephemeral selective pack emits byte-identical members to the retained
//! full-capture Session pack; Sessions keep serving repeated packs from
//! their arena (F4); compressed blends still pack through the fallback.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn workdir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oxblend-pack-selective-{}-{}-{label}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn member_bytes(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut map = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let key = path
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                map.insert(key, std::fs::read(&path).unwrap());
            }
        }
    }
    map
}

/// Ephemeral CLI-equivalent pack (selective raw windows) emits exactly the
/// same member bytes, rewrite count and completion as the long-lived
/// Session pack (full retained captures) for stable inputs.
#[test]
fn selective_ephemeral_pack_matches_session_full_pack() {
    let work = workdir("parity");
    let project = work.join("project");
    std::fs::create_dir(&project).unwrap();
    for name in ["mesh-material-root.blend", "mesh-material-library.blend"] {
        let from = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/dependencies")
            .join(name);
        std::fs::copy(&from, project.join(name)).unwrap();
    }
    // A standalone seed exercises the no-library path as well.
    let seed_from = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/synthetic-empty-seed.blend");
    std::fs::copy(&seed_from, project.join("seed.blend")).unwrap();

    for root_name in ["mesh-material-root.blend", "seed.blend"] {
        let root = project.join(root_name);
        let mut options = oxblend::pack::PackOptions::new(project.clone());
        options.permissive = true;

        let selective_out = work.join(format!("selective-{root_name}"));
        let selective = oxblend::pack::pack(std::slice::from_ref(&root), &selective_out, &options)
            .expect("selective pack must succeed");
        let selective_members = member_bytes(&selective_out);

        let mut session = oxblend::Session::open(&root).into_result().unwrap();
        let session_out = work.join(format!("session-{root_name}"));
        let retained = oxblend::pack::pack_session(&mut session, &session_out, &options)
            .into_result()
            .expect("session pack must succeed");
        let retained_members = member_bytes(&session_out);

        assert_eq!(
            selective_members, retained_members,
            "selective vs full member bytes differ for {root_name}"
        );
        assert_eq!(selective.complete, retained.complete);
        assert_eq!(selective.rewrites.len(), retained.rewrites.len());
        assert_eq!(selective.files.len(), retained.files.len());
        // The retained arena keeps serving: a second session pack reuses the
        // same captures and stays byte-identical (F4 long-lived behavior).
        let session_out2 = work.join(format!("session2-{root_name}"));
        let retained2 = oxblend::pack::pack_session(&mut session, &session_out2, &options)
            .into_result()
            .expect("second session pack must succeed");
        assert_eq!(member_bytes(&session_out2), retained_members);
        assert_eq!(retained2.complete, retained.complete);
    }
    std::fs::remove_dir_all(&work).unwrap();
}

/// Compressed blends never enter the selective representation but still pack
/// through the historical full-body fallback with identical members.
#[test]
fn selective_router_falls_back_for_compressed_blends() {
    let work = workdir("compressed");
    let project = work.join("project");
    std::fs::create_dir(&project).unwrap();
    let raw = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/file-ops/synthetic-empty-seed.blend"),
    )
    .unwrap();
    let mut options = oxblend::pack::PackOptions::new(project.clone());
    options.permissive = true;
    for (label, bytes) in [
        ("gzip", {
            let mut encoder = flate2::GzBuilder::new()
                .mtime(0)
                .operating_system(255)
                .write(Vec::new(), flate2::Compression::default());
            use std::io::Write as _;
            encoder.write_all(&raw).unwrap();
            encoder.finish().unwrap()
        }),
        ("zstd", zstd::stream::encode_all(raw.as_slice(), 0).unwrap()),
    ] {
        // Store the compressed stream under a .blend name: the pack router
        // admits it by content family, and the selective probe must defer
        // to full bodies for both gzip and zstd.
        let blended = project.join(format!("seed-{label}.blend"));
        std::fs::write(&blended, &bytes).unwrap();
        let out = work.join(format!("out-{label}"));
        let selective = oxblend::pack::pack(std::slice::from_ref(&blended), &out, &options)
            .unwrap_or_else(|e| panic!("{label} pack must succeed: {e:?}"));
        assert!(selective.complete, "{label} pack must be complete");
        assert_eq!(selective.files.len(), 1);
        assert!(out.join(blended.file_name().unwrap()).is_file());

        let mut session = oxblend::Session::open(&blended).into_result().unwrap();
        let out2 = work.join(format!("out2-{label}"));
        let retained = oxblend::pack::pack_session(&mut session, &out2, &options)
            .into_result()
            .unwrap_or_else(|e| panic!("{label} session pack must succeed: {e:?}"));
        assert_eq!(member_bytes(&out), member_bytes(&out2));
        assert_eq!(selective.complete, retained.complete);
    }
    std::fs::remove_dir_all(&work).unwrap();
}
