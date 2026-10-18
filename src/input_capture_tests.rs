//! Capture ownership and actual guarded physical-copy contracts, for every container.
use super::*;
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
        let inputs = PackInputs::new(&[path.clone(), path.clone()]).unwrap();
        let snapshot = inputs.get(&path).unwrap();
        let reused = inputs.get(&path).unwrap();
        assert!(Rc::ptr_eq(&snapshot, &reused));
        assert_eq!(
            snapshot.reader.data.range(0, body.len()).unwrap().as_ref(),
            body.as_slice()
        );
        let checked = snapshot.open_checked(&path).unwrap();
        assert_eq!(
            crate::physical::Physical::new(&checked).family().unwrap(),
            if family == "seek" { "zstd" } else { family }
        );
        let copied_path = work.0.join("copied");
        let mut copied = File::create(&copied_path).unwrap();
        assert_eq!(
            inputs
                .copy_physical(&path, &snapshot.metadata, &mut copied)
                .unwrap(),
            physical.len() as u64
        );
        assert_eq!(std::fs::read(copied_path).unwrap(), physical);
        assert!(Rc::ptr_eq(&snapshot, &inputs.get(&path).unwrap()));
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
        let inputs = PackInputs::new(&[a.clone(), b.clone()]).unwrap();
        let first = inputs.get(&a).unwrap();
        let second = inputs.get(&b).unwrap();
        let proof = first.proof.as_ref().unwrap();
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
    let inputs = PackInputs::new(&[root]).unwrap();
    let captured = inputs.take(&path, false).unwrap();
    let Input::Opaque(raw) = &captured else {
        unreachable!()
    };
    let metadata = raw.metadata.clone();
    inputs.files.borrow_mut().insert(path.clone(), captured);
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
    let error = inputs
        .copy_physical(&path, &metadata, &mut stage)
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
        let inputs = PackInputs::new(std::slice::from_ref(&path)).unwrap();
        let snapshot = inputs.get(&path).unwrap();
        let mut output = File::create(work.0.join("stage")).unwrap();
        let mut fault_applied = 0;
        let error = inputs
            .copy_physical_using(&path, &snapshot.metadata, &mut output, |s, p, m, t, raw| {
                crate::kernel_copy::guarded(s, p, m, t, raw, |s, o, t, n| {
                    let copied = crate::kernel_copy::native(s, o, t, n)?;
                    wait_timestamp_tick(&path, &snapshot.metadata);
                    let f = OpenOptions::new().write(true).open(&path)?;
                    f.write_all_at(b"X", 0)?;
                    f.set_modified(snapshot.metadata.modified().unwrap())?;
                    assert_ctime_changed(&path, &snapshot.metadata);
                    fault_applied += 1;
                    Ok(copied)
                })
            })
            .unwrap_err();
        assert_eq!(fault_applied, 1);
        assert_eq!(error.kind, "input_changed");
        assert_eq!(error.message, "asset source changed during copy");
        assert_eq!(
            crate::fetch::signature(&path.metadata().unwrap()),
            crate::fetch::signature(&snapshot.metadata)
        );
        // Interpretation still comes from the same immutable decoded capture.
        assert_eq!(
            snapshot.reader.data.range(0, body.len()).unwrap().as_ref(),
            body.as_slice()
        );
        assert_eq!(inputs.get(&path).err().unwrap().kind, "input_changed");
    }
}
