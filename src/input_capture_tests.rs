//! Capture ownership and actual guarded physical-copy contracts, for every container.
use super::*;
use crate::session::{CaptureBatch, GuardPolicy, InputStore, OpCtx};
use std::path::PathBuf;
fn harness() -> (InputStore, CaptureBatch, OpCtx) {
    let cwd = std::env::current_dir().unwrap();
    let temp_root = std::env::temp_dir();
    (
        InputStore::default(),
        CaptureBatch::default(),
        OpCtx::new(GuardPolicy::Legacy, None, cwd, temp_root),
    )
}
fn ensure_blend(
    store: &mut InputStore,
    batch: &mut CaptureBatch,
    op: &mut OpCtx,
    path: &std::path::Path,
) -> crate::session::SourceId {
    store.ensure(batch, path, true, false, op).unwrap()
}
fn encode(family: &str, body: &[u8]) -> Vec<u8> {
    if family == "raw" {
        return body.to_vec();
    }
    if family == "gzip" {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(body).unwrap();
        return e.finish().unwrap();
    }
    let mut e = zstd::stream::write::Encoder::new(Vec::new(), 0).unwrap();
    e.include_checksum(true).unwrap();
    e.write_all(body).unwrap();
    let mut out = e.finish().unwrap();
    if family == "seek" {
        let mut table = Vec::new();
        table.extend((out.len() as u32).to_le_bytes());
        table.extend((body.len() as u32).to_le_bytes());
        table.extend(1u32.to_le_bytes());
        table.push(0);
        table.extend(0x8f92eab1u32.to_le_bytes());
        out.extend(0x184d2a5eu32.to_le_bytes());
        out.extend((table.len() as u32).to_le_bytes());
        out.extend(table);
    }
    out
}
struct Work(PathBuf);
impl Work {
    fn new(label: &str, family: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "oxblend-capture-{}-{label}-{family}",
            std::process::id()
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
// External platform constraint: Linux filesystem ctime can use a coarse clock
// tick. Observe a same-filesystem sibling's clock advancing before mutation;
// otherwise restoring mtime within the capture tick cannot seed this fault.
// No sleep, timeout or retry count: the observed timestamp is the condition.
fn wait_timestamp_tick(path: &std::path::Path, captured: &std::fs::Metadata) {
    use std::os::unix::fs::MetadataExt;
    let clock = File::create(path.with_extension("ctime-clock")).unwrap();
    let mode = clock.metadata().unwrap().permissions();
    loop {
        clock.set_permissions(mode.clone()).unwrap();
        let now = clock.metadata().unwrap();
        if (now.ctime(), now.ctime_nsec()) > (captured.ctime(), captured.ctime_nsec()) {
            break;
        }
    }
}
fn assert_ctime_changed(path: &std::path::Path, captured: &std::fs::Metadata) {
    use std::os::unix::fs::MetadataExt;
    let after = path.metadata().unwrap();
    assert_ne!(
        (after.ctime(), after.ctime_nsec()),
        (captured.ctime(), captured.ctime_nsec()),
        "setup error: ctime mutation did not advance the filesystem timestamp"
    );
}
#[test]
fn decoded_capture_reuse_and_reopened_physical_identity_all_families() {
    let body = include_bytes!("../tests/fixtures/constraint-data/tar.blend");
    for family in ["raw", "gzip", "zstd", "seek"] {
        let work = Work::new("identity", family);
        let path = work.0.join("input.blend");
        let physical = encode(family, body);
        std::fs::write(&path, &physical).unwrap();
        let (mut store, mut batch, mut op) = harness();
        let id = ensure_blend(&mut store, &mut batch, &mut op, &path);
        let reused = ensure_blend(&mut store, &mut batch, &mut op, &path);
        // The same lexical source shares one arena identity: no second
        // capture or parse. Parse-count evidence additionally comes from
        // the session span tests.
        assert_eq!(id, reused);
        let metadata = store.snapshot(id).unwrap().metadata.clone();
        // Short arena observations: each borrow ends before the next call.
        assert_eq!(
            store
                .snapshot(id)
                .unwrap()
                .reader
                .data
                .range(0, body.len())
                .unwrap()
                .as_ref(),
            body.as_slice()
        );
        let checked = store.snapshot(id).unwrap().open_checked(&path).unwrap();
        assert_eq!(
            crate::physical::Physical::new(&checked).family().unwrap(),
            if family == "seek" { "zstd" } else { family }
        );
        let copied_path = work.0.join("copied");
        let mut copied = File::create(&copied_path).unwrap();
        assert_eq!(
            crate::session::copy_physical_using(
                &mut store,
                &mut batch,
                &path,
                &metadata,
                &mut copied,
                crate::kernel_copy::full,
            )
            .unwrap(),
            physical.len() as u64
        );
        assert_eq!(std::fs::read(copied_path).unwrap(), physical);
        assert_eq!(ensure_blend(&mut store, &mut batch, &mut op, &path), id);
    }
}
#[test]
fn capture_proof_rejects_wrong_source_and_ctime_mutation_all_families() {
    let body = include_bytes!("../tests/fixtures/constraint-data/tar.blend");
    for family in ["raw", "gzip", "zstd", "seek"] {
        let work = Work::new("proof", family);
        let a = work.0.join("a.blend");
        let b = work.0.join("b.blend");
        let physical = encode(family, body);
        std::fs::write(&a, &physical).unwrap();
        std::fs::write(&b, &physical).unwrap();
        let (mut store, mut batch, mut op) = harness();
        let first_id = ensure_blend(&mut store, &mut batch, &mut op, &a);
        let second_id = ensure_blend(&mut store, &mut batch, &mut op, &b);
        assert_ne!(first_id, second_id);
        let first = store.snapshot(first_id).unwrap();
        let second = store.snapshot(second_id).unwrap();
        let proof = &first.proof;
        let wrong = second.open_checked(&b).unwrap();
        let error = proof.admit(&wrong, &b).unwrap_err();
        assert_eq!(error.kind, "input_changed");
        assert_eq!(error.message, "source signature changed since snapshot");
        let current = first.open_checked(&a).unwrap();
        // Retired wrong-allocation negative: Physical::captured now takes
        // &Snapshot, not an independently swappable reader/proof pair. Its
        // immutable decoded owner is the admission backing by construction.
        assert!(proof.admit(&current, &a).is_ok());
        // Restore mtime after a real write; only ctime exposes this mutation.
        wait_timestamp_tick(&a, &first.metadata);
        let writer = OpenOptions::new().write(true).open(&a).unwrap();
        writer.write_all_at(b"X", 0).unwrap();
        writer
            .set_modified(first.metadata.modified().unwrap())
            .unwrap();
        assert_eq!(
            crate::fetch::signature(&writer.metadata().unwrap()),
            crate::fetch::signature(&first.metadata)
        );
        assert_ctime_changed(&a, &first.metadata);
        assert_eq!(first.check(&a).unwrap_err().kind, "input_changed");
        eprintln!(
            "OXBLEND_CAPTURE_PROOF_TEST {}",
            serde_json::json!({"test":"capture_proof_rejects_wrong_source_and_ctime_mutation_all_families","family":family,"wrong_inode_signature":1,"wrong_allocation_retired":"Snapshot-owned reader by construction","restored_mtime_ctime_mutation":1,"positive_same_owner":1,"negative_cases":2,"verdict":"pass"})
        );
    }
}
#[test]
fn opaque_capture_and_completed_copy_retain_no_asset_body() {
    let work = Work::new("opaque-storage", "opaque");
    let root = work.0.join("root.blend");
    std::fs::write(
        &root,
        include_bytes!("../tests/fixtures/constraint-data/tar.blend"),
    )
    .unwrap();
    let path = work.0.join("asset");
    let body = vec![7; 3 * crate::fetch::READ_CHUNK + 17];
    std::fs::write(&path, &body).unwrap();
    let (mut store, mut batch, mut op) = harness();
    let _root_id = ensure_blend(&mut store, &mut batch, &mut op, &root);
    let raw = batch.take_raw(&path, false).unwrap();
    assert!(
        raw.bytes.is_empty(),
        "opaque prefetch must retain proof, not asset bytes"
    );
    let metadata = raw.metadata.clone();
    batch.stage_raw(path.clone(), raw);
    let mut stage = File::create(work.0.join("stage")).unwrap();
    assert_eq!(
        crate::session::copy_physical(&mut store, &mut batch, &path, &metadata, &mut stage)
            .unwrap(),
        body.len() as u64
    );
    assert_eq!(std::fs::read(work.0.join("stage")).unwrap(), body);
    assert!(batch.opaque_bodies_empty());
}
#[test]
fn opaque_copy_checks_captured_hash() {
    let work = Work::new("opaque-hash", "opaque");
    let root = work.0.join("root.blend");
    std::fs::write(
        &root,
        include_bytes!("../tests/fixtures/constraint-data/tar.blend"),
    )
    .unwrap();
    let path = work.0.join("asset");
    std::fs::write(&path, b"opaque physical data").unwrap();
    let (mut store, mut batch, mut op) = harness();
    let _root_id = ensure_blend(&mut store, &mut batch, &mut op, &root);
    let mut raw = batch.take_raw(&path, false).unwrap();
    let metadata = raw.metadata.clone();
    // Seed an actually mismatching proof: copying the live source must not
    // accept its own digest instead of comparing with the captured identity.
    raw.asset_hash = Some(raw.asset_hash.unwrap() ^ 1);
    batch.stage_raw(path.clone(), raw);
    let mut stage = File::create(work.0.join("stage")).unwrap();
    let error = crate::session::copy_physical(&mut store, &mut batch, &path, &metadata, &mut stage)
        .unwrap_err();
    assert_eq!(error.kind, "input_changed");
    assert_eq!(error.message, "asset bytes changed since capture");
}
#[test]
fn opaque_captured_copy_refuses_restored_mtime_mutation() {
    let work = Work::new("opaque-ctime", "opaque");
    let root = work.0.join("root.blend");
    std::fs::write(
        &root,
        include_bytes!("../tests/fixtures/constraint-data/tar.blend"),
    )
    .unwrap();
    let path = work.0.join("asset");
    std::fs::write(&path, b"opaque physical data").unwrap();
    let (mut store, mut batch, mut op) = harness();
    let _root_id = ensure_blend(&mut store, &mut batch, &mut op, &root);
    let raw = batch.take_raw(&path, false).unwrap();
    let metadata = raw.metadata.clone();
    batch.stage_raw(path.clone(), raw);
    wait_timestamp_tick(&path, &metadata);
    let writer = OpenOptions::new().write(true).open(&path).unwrap();
    writer.write_all_at(b"X", 0).unwrap();
    writer.set_modified(metadata.modified().unwrap()).unwrap();
    assert_eq!(
        crate::fetch::signature(&path.metadata().unwrap()),
        crate::fetch::signature(&metadata)
    );
    assert_ctime_changed(&path, &metadata);
    let mut stage = File::create(work.0.join("stage")).unwrap();
    let error = crate::session::copy_physical(&mut store, &mut batch, &path, &metadata, &mut stage)
        .unwrap_err();
    assert_eq!(error.kind, "input_changed");
    assert_eq!(error.message, "asset source changed before captured copy");
    assert_eq!(stage.metadata().unwrap().len(), 0);
}
#[test]
fn reopened_physical_copy_refuses_real_midcopy_mutation_all_families() {
    let body = include_bytes!("../tests/fixtures/constraint-data/tar.blend");
    for family in ["raw", "gzip", "zstd", "seek"] {
        let work = Work::new("mutation", family);
        let path = work.0.join("input.blend");
        std::fs::write(&path, encode(family, body)).unwrap();
        let (mut store, mut batch, mut op) = harness();
        let id = ensure_blend(&mut store, &mut batch, &mut op, &path);
        let metadata = store.snapshot(id).unwrap().metadata.clone();
        let mut output = File::create(work.0.join("stage")).unwrap();
        let mut fault_applied = 0;
        let error = crate::session::copy_physical_using(
            &mut store,
            &mut batch,
            &path,
            &metadata,
            &mut output,
            |s, p, m, t, raw| {
                crate::kernel_copy::guarded(s, p, m, t, raw, |s, o, t, n| {
                    let copied = crate::kernel_copy::native(s, o, t, n)?;
                    wait_timestamp_tick(&path, &metadata);
                    let f = OpenOptions::new().write(true).open(&path)?;
                    f.write_all_at(b"X", 0)?;
                    f.set_modified(metadata.modified().unwrap())?;
                    assert_ctime_changed(&path, &metadata);
                    fault_applied += 1;
                    Ok(copied)
                })
            },
        )
        .unwrap_err();
        assert_eq!(fault_applied, 1);
        assert_eq!(error.kind, "input_changed");
        assert_eq!(error.message, "asset source changed during copy");
        assert_eq!(
            crate::fetch::signature(&path.metadata().unwrap()),
            crate::fetch::signature(&metadata)
        );
        // Interpretation still comes from the same immutable decoded capture.
        assert_eq!(
            store
                .snapshot(id)
                .unwrap()
                .reader
                .data
                .range(0, body.len())
                .unwrap()
                .as_ref(),
            body.as_slice()
        );
        assert_eq!(
            store
                .ensure(&mut batch, &path, true, false, &mut op)
                .unwrap_err()
                .kind,
            "input_changed"
        );
    }
}
/// A directory permission blip is not a source change: stat fails with a
/// device/permission error that stays `io`, never latches `input_changed`,
/// and the next check after restore succeeds.
#[test]
fn directory_permission_blip_never_latches_input_changed() {
    use std::os::unix::fs::PermissionsExt;
    struct Restore<'a> {
        path: &'a std::path::Path,
    }
    impl Drop for Restore<'_> {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(self.path, std::fs::Permissions::from_mode(0o700));
        }
    }
    let body = include_bytes!("../tests/fixtures/constraint-data/tar.blend");
    let work = Work::new("perm", "blip");
    let dir = work.0.join("locked");
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("input.blend");
    std::fs::write(&path, body).unwrap();
    let (mut store, mut batch, mut op) = harness();
    op.policy = GuardPolicy::Session; // Exercise Session latching, not legacy's no-latch policy.
    let id = ensure_blend(&mut store, &mut batch, &mut op, &path);
    let before = op.diagnostics.len();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
    let _restore = Restore { path: &dir };
    let error = store
        .ensure(&mut batch, &path, true, false, &mut op)
        .unwrap_err();
    assert_eq!(error.kind, "io", "permission blip must stay io: {error:?}");
    // Exactly one typed notice for the failed check, no latch behind it.
    assert_eq!(op.diagnostics.len(), before + 1);
    let [crate::session::Diagnostic::SourceCheckFailure { error: noticed, .. }] =
        &op.diagnostics[before..]
    else {
        panic!(
            "expected one SourceCheckFailure, got {:?}",
            &op.diagnostics[before..]
        );
    };
    assert_eq!(noticed.kind, "io");
    drop(_restore);
    // Restore proves the entry never latched: reuse succeeds immediately.
    assert_eq!(
        store
            .ensure(&mut batch, &path, true, false, &mut op)
            .unwrap(),
        id
    );
    // A different completion failure on that SAME source in the SAME
    // operation is new evidence, not a duplicate to suppress by source ID.
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"changed")
        .unwrap();
    let failures = store.check_all(id, &mut op);
    assert_eq!(failures[0].1.kind, "input_changed");
    let kinds = op.diagnostics[before..]
        .iter()
        .filter_map(|notice| match notice {
            crate::session::Diagnostic::SourceCheckFailure { error, .. } => Some(error.kind),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        ["io", "input_changed"],
        "distinct failed checks must both remain in diagnostics"
    );
}
