//! Verified-on-reuse working-body tests: digest mismatch under unchanged
//! metadata (injectable seam), latching, negative sources, shared-source
//! plans, nesting safety and honest zero-persistent-retention bounds.
use super::*;
use crate::session::{CaptureBatch, GuardPolicy, InputStore, OpCtx};
use std::io::Write;
use std::path::{Path, PathBuf};

fn harness() -> (InputStore, CaptureBatch, OpCtx) {
    let cwd = std::env::current_dir().unwrap();
    let temp_root = std::env::temp_dir();
    (
        InputStore::with_memory(crate::session::MemoryMode::Bounded),
        CaptureBatch::with_memory(crate::session::MemoryMode::Bounded),
        OpCtx::new(GuardPolicy::Session, None, cwd, temp_root),
    )
}

fn ensure_blend(
    store: &mut InputStore,
    batch: &mut CaptureBatch,
    op: &mut OpCtx,
    path: &Path,
) -> crate::session::SourceId {
    store.ensure(batch, path, true, false, op).unwrap()
}

fn encode(family: &str, body: &[u8]) -> Vec<u8> {
    if family == "raw" {
        return body.to_vec();
    }
    if family == "gzip" {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(body).unwrap();
        return encoder.finish().unwrap();
    }
    let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 0).unwrap();
    encoder.write_all(body).unwrap();
    encoder.finish().unwrap()
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let dir = crate::test_scratch::scratch_dir(&std::env::temp_dir());
        let _ = label;
        Self(dir)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn synthetic(work: &Work, name: &str, family: &str) -> (PathBuf, Vec<u8>) {
    let body = include_bytes!("../tests/fixtures/constraint-data/tar.blend").to_vec();
    let path = work.0.join(name);
    std::fs::write(&path, encode(family, &body)).unwrap();
    (path, body)
}

fn body_of(store: &InputStore, id: crate::session::SourceId) -> Vec<u8> {
    store.reader(id).unwrap().read_decompressed_bytes().unwrap()
}

/// Digest mismatch with completely unchanged metadata must refuse, latch,
/// and keep refusing without a second physical re-read. The injected
/// provider proves the metadata signature still matches, then returns the
/// mismatch target decoded with one flipped content byte and an honestly
/// recomputed digest — exactly the change class signature checks cannot
/// express and the retained digest exists to catch. Every other source
/// re-materializes honestly through the same seam, so an unrelated source
/// is still served (re-read and verified) while one source stands latched.
#[test]
fn digest_mismatch_with_unchanged_metadata_latches_and_refuses() {
    for family in ["raw", "gzip", "zstd"] {
        let work = Work::new("mismatch");
        let (path, body) = synthetic(&work, "input.blend", family);
        let (mut store, mut batch, mut op) = harness();
        let id = ensure_blend(&mut store, &mut batch, &mut op, &path);
        // Read once through the seeded body, then evict it with a second
        // source so the next read must re-materialize from the provider.
        let (other, _) = synthetic(&work, "other.blend", "raw");
        let other_id = ensure_blend(&mut store, &mut batch, &mut op, &other);
        store.bodies.set_recheck(|path, expected| {
            let metadata = std::fs::metadata(path).unwrap();
            assert_eq!(
                crate::fetch::capture_signature(&metadata),
                *expected,
                "metadata signature is unchanged at the recheck"
            );
            let mut bytes = std::fs::read(path).unwrap();
            // Only the mismatch target ("input.blend") diverges; any other
            // source re-materializing through this seam must be served its
            // honest bytes. The seam is a bare fn pointer, so the target is
            // named by file name rather than captured.
            if path.file_name() == Some(std::ffi::OsStr::new("input.blend")) {
                bytes[0] ^= 0xff;
            }
            let mut hash = crate::edit_xxh::Xxh64::new();
            hash.update(&bytes);
            let digest = SourceDigest {
                physical: hash.digest(),
                decoded: hash.digest(),
            };
            Ok((bytes, digest))
        });
        let error = store
            .reader(id)
            .unwrap()
            .read_decompressed_bytes()
            .unwrap_err();
        assert_eq!(error.kind, "input_changed", "{family}: {error:?}");
        assert!(
            error.message.contains("digest changed since capture"),
            "{family}: {error:?}"
        );
        let counters = store.bodies.counters(&path);
        assert_eq!(counters.physical_rereads, 1);
        assert_eq!(counters.digest_verifications, 1);
        // The injected provider materialized exactly the bytes it returned
        // (the physical file contents); the real provider would materialize
        // the decoded stream.
        assert_eq!(
            counters.decoded_bytes,
            std::fs::read(&path).unwrap().len() as u64
        );
        // The latch holds and never re-reads the source again.
        let again = store
            .reader(id)
            .unwrap()
            .read_decompressed_bytes()
            .unwrap_err();
        assert_eq!(again.kind, "input_changed");
        assert_eq!(store.bodies.counters(&path).physical_rereads, 1);
        // check_reuse announces the latched failure once, as a source check.
        op.record_touch(id);
        let refused = store.check_reuse(id, &mut op).unwrap_err();
        assert_eq!(refused.kind, "input_changed");
        assert!(
            matches!(
                op.diagnostics.last(),
                Some(crate::Diagnostic::SourceCheckFailure { source, error }) if source == &path
                    && error.kind == "input_changed"
            ),
            "{:?}",
            op.diagnostics
        );
        // Completion reports the latched source; unrelated sources read on —
        // including through re-materialization: eviction freed the slot at
        // the latched source's failed acquire, so this read re-reads,
        // verifies honestly, and is served despite the standing latch.
        let failures = store.check_all(id, &mut op);
        assert_eq!(failures[0].1.kind, "input_changed");
        assert_eq!(body_of(&store, other_id).len(), body.len());
        let other_counters = store.bodies.counters(&other);
        assert_eq!(other_counters.physical_rereads, 1);
        assert_eq!(other_counters.digest_verifications, 1);
        drop(batch);
    }
}

/// Negative sources: disappearance and signature divergence are established
/// changes that latch; a permission blip stays transient io and fails open.
#[test]
fn negative_sources_disappear_replace_and_permission_blip() {
    let work = Work::new("negative");
    let (path, _body) = synthetic(&work, "gone.blend", "raw");
    let (mut store, mut batch, mut op) = harness();
    let id = ensure_blend(&mut store, &mut batch, &mut op, &path);
    let (evict, _) = synthetic(&work, "evict.blend", "raw");
    let _evict_id = ensure_blend(&mut store, &mut batch, &mut op, &evict);
    // Disappearance latches with the established vocabulary.
    std::fs::remove_file(&path).unwrap();
    let error = store
        .reader(id)
        .unwrap()
        .read_decompressed_bytes()
        .unwrap_err();
    assert_eq!(error.kind, "input_changed");
    assert!(error.message.contains("disappeared"), "{error:?}");
    assert!(store.bodies.latched(&path).is_some());
    // Replacement with a diverging signature latches at the recheck guard.
    let (replaced, _) = synthetic(&work, "replaced.blend", "raw");
    let (mut store2, mut batch2, mut op2) = harness();
    let id2 = ensure_blend(&mut store2, &mut batch2, &mut op2, &replaced);
    let (evict2, _) = synthetic(&work, "evict2.blend", "raw");
    let _e2 = ensure_blend(&mut store2, &mut batch2, &mut op2, &evict2);
    let mut longer = std::fs::read(&replaced).unwrap();
    longer.extend_from_slice(b"appended-bytes");
    std::fs::write(&replaced, longer).unwrap();
    let error = store2
        .reader(id2)
        .unwrap()
        .read_decompressed_bytes()
        .unwrap_err();
    assert_eq!(error.kind, "input_changed");
    assert_eq!(error.message, "source signature changed since capture");
    assert!(store2.bodies.latched(&replaced).is_some());
    // A permission blip is transient io: no latch, and reads recover.
    if unsafe { libc::geteuid() } != 0 {
        let (locked_dir, _) = synthetic(&work, "locked", "raw");
        // synthetic writes a file; move it into a locked directory instead.
        let dir = work.0.join("locked-dir");
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join("input.blend");
        std::fs::rename(&locked_dir, &file).unwrap();
        let (mut store3, mut batch3, mut op3) = harness();
        let id3 = ensure_blend(&mut store3, &mut batch3, &mut op3, &file);
        let (evict3, _) = synthetic(&work, "evict3.blend", "raw");
        let _e3 = ensure_blend(&mut store3, &mut batch3, &mut op3, &evict3);
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
        let error = store3
            .reader(id3)
            .unwrap()
            .read_decompressed_bytes()
            .unwrap_err();
        assert_eq!(error.kind, "io", "permission blip must stay io: {error:?}");
        assert!(store3.bodies.latched(&file).is_none());
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            store3
                .reader(id3)
                .unwrap()
                .read_decompressed_bytes()
                .is_ok()
        );
        drop(op3);
    }
    drop((op, op2, batch, batch2));
}

/// The arena retains parsed indexes only: no entry keeps decoded bytes, the
/// single working slot holds at most one body, and repeated reads of a
/// resident body cost no physical re-read.
#[test]
fn zero_persistent_entry_retention_and_single_working_body() {
    let work = Work::new("retention");
    let (a, body_a) = synthetic(&work, "a.blend", "raw");
    let (b, body_b) = synthetic(&work, "b.blend", "gzip");
    let (c, body_c) = synthetic(&work, "c.blend", "zstd");
    let (mut store, mut batch, mut op) = harness();
    let a_id = ensure_blend(&mut store, &mut batch, &mut op, &a);
    let b_id = ensure_blend(&mut store, &mut batch, &mut op, &b);
    let c_id = ensure_blend(&mut store, &mut batch, &mut op, &c);
    // Honest zero-retention assertion: every captured entry's backing is the
    // verified-on-reuse holder, never a private decoded body.
    assert!(
        store.captured_entries_retain_no_bodies(),
        "captured entries must not retain per-entry decoded bytes"
    );
    // The most recent capture (c) is the resident body: its read is served
    // without any physical re-read.
    assert_eq!(store.bodies.counters(&c).physical_rereads, 0);
    assert!(body_of(&store, c_id).len() == body_c.len());
    assert_eq!(store.bodies.counters(&c).physical_rereads, 0);
    // A was evicted by the later captures; re-read, verify, and reuse the
    // re-materialized body without a second re-read.
    assert_eq!(body_of(&store, a_id), body_a);
    let a_counters = store.bodies.counters(&a);
    assert_eq!(a_counters.physical_rereads, 1);
    assert_eq!(a_counters.digest_verifications, 1);
    assert_eq!(a_counters.decoded_bytes, body_a.len() as u64);
    assert_eq!(body_of(&store, a_id), body_a);
    assert_eq!(store.bodies.counters(&a).physical_rereads, 1);
    // Exactly one resident body, never more, bounded by the largest source.
    assert!(store.bodies.resident_bodies() <= 1);
    let largest = body_a.len().max(body_b.len()).max(body_c.len());
    assert!(store.bodies.resident_bytes() <= largest);
    assert!(body_of(&store, b_id).len() == body_b.len());
    assert!(store.bodies.resident_bodies() <= 1);
    assert!(store.bodies.resident_bytes() <= largest);
    drop(op);
}

/// Nested scans must not panic on the shared slot and must never be served
/// another source's bytes: an outer scan of A scans both B and A inside its
/// closure; each inner scan acquires its own verified body.
#[test]
fn nested_scans_never_panic_or_mix_sources() {
    let work = Work::new("nested");
    let (a, body_a) = synthetic(&work, "a.blend", "raw");
    let (b, body_b) = synthetic(&work, "b.blend", "raw");
    let (mut store, mut batch, mut op) = harness();
    let a_id = ensure_blend(&mut store, &mut batch, &mut op, &a);
    let b_id = ensure_blend(&mut store, &mut batch, &mut op, &b);
    let reader_a = store.reader(a_id).unwrap();
    let reader_b = store.reader(b_id).unwrap();
    let mut inner_b = 0;
    let mut inner_a = 0;
    let outer = reader_a.data.scan(|outer_reader| {
        // Read a byte through the outer cursor to prove it stays usable
        // across the nested acquisitions.
        let mut first = [0u8; 1];
        crate::input::read_exact(outer_reader, &mut first)?;
        assert_eq!(first[0], body_a[0]);
        reader_b
            .data
            .scan(|inner| {
                let mut head = vec![0; body_b.len().min(64)];
                crate::input::read_exact(inner, &mut head)?;
                assert_eq!(&head[..], &body_b[..head.len()]);
                inner_b += 1;
                Ok(())
            })
            .unwrap();
        reader_a
            .data
            .scan(|inner| {
                let mut head = vec![0; body_a.len().min(64)];
                crate::input::read_exact(inner, &mut head)?;
                assert_eq!(&head[..], &body_a[..head.len()]);
                inner_a += 1;
                Ok(())
            })
            .unwrap();
        Ok(())
    });
    assert!(outer.is_ok(), "{outer:?}");
    assert_eq!(inner_b, 1);
    assert_eq!(inner_a, 1);
    // The outer guard's body is released last; the slot ends with exactly
    // one resident body for whichever guard released into the empty slot.
    assert!(store.bodies.resident_bodies() <= 1);
    drop(op);
}

/// A shared-source plan (two selected roots referencing one library) keeps
/// one arena identity and one counter key per source, completes, and
/// verifies on reuse across plan and publication.
#[test]
fn shared_source_plan_completes_with_single_counter_key() {
    let work = Work::new("shared-plan");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dependencies");
    let root1 = work.0.join("root1.blend");
    let root2 = work.0.join("root2.blend");
    let library = work.0.join("mesh-material-library.blend");
    std::fs::copy(fixtures.join("mesh-material-root.blend"), &root1).unwrap();
    std::fs::copy(fixtures.join("mesh-material-root.blend"), &root2).unwrap();
    std::fs::copy(fixtures.join("mesh-material-library.blend"), &library).unwrap();
    std::fs::write(work.0.join("texture.png"), b"synthetic image bytes").unwrap();
    let mut options = crate::pack::PackOptions::new(work.0.clone());
    options.memory = crate::session::MemoryMode::Bounded;
    let target = work.0.join("package");
    let report = crate::pack::pack_operation(&[root1.clone(), root2.clone()], &target, &options)
        .into_result()
        .unwrap();
    assert!(report.complete, "errors: {:?}", report.errors);
    // The report's own store is gone; re-open a Session over the same pair
    // to assert the shared arena identity and reuse counters.
    let mut session = crate::Session::open_with_memory(&root1, crate::session::MemoryMode::Bounded)
        .into_result()
        .unwrap();
    let traced = crate::trace::trace_session(
        &mut session,
        crate::trace::TraceOptions::new(
            crate::trace::TraceMode::Recursive,
            crate::trace::PointerMode::Strict,
        ),
    )
    .into_result()
    .unwrap();
    assert!(traced.complete);
    assert!(
        session.store.id_for_path(&library).is_ok(),
        "the shared library must be one arena identity"
    );
    assert!(session.store.bodies.resident_bodies() <= 1);
    // Publication reuse over the same retained arena completes, and the
    // root's body was evicted long before execution: its payload copy
    // re-reads the source and verifies the retained digest again.
    let before = session.store.bodies.counters(session.root_path());
    let target2 = work.0.join("package2");
    let second = crate::pack::pack_session(&mut session, &target2, &options)
        .into_result()
        .unwrap();
    assert!(second.complete, "errors: {:?}", second.errors);
    let after = session.store.bodies.counters(session.root_path());
    assert!(
        after.digest_verifications > before.digest_verifications,
        "publication reuse must re-read and verify the root: {after:?}"
    );
    assert!(session.store.bodies.resident_bodies() <= 1);
}

/// Re-materialized bytes are byte-identical to the capture decode, and the
/// digest rides the existing capture reads (the seeded read performs no
/// physical re-read at all).
#[test]
fn re_materialized_bytes_match_capture_and_digest_rides_capture_reads() {
    for family in ["raw", "gzip", "zstd"] {
        let work = Work::new("rematerialize");
        let (path, body) = synthetic(&work, "input.blend", family);
        let (mut store, mut batch, mut op) = harness();
        let id = ensure_blend(&mut store, &mut batch, &mut op, &path);
        // Seed: the capture decode's own bytes serve the first read.
        assert_eq!(body_of(&store, id), body);
        assert_eq!(store.bodies.counters(&path).physical_rereads, 0);
        // Evict via a second capture, then re-materialize through the real
        // provider: same physical+decoded digests, byte-identical body.
        let (other, _) = synthetic(&work, "other.blend", "raw");
        let _other = ensure_blend(&mut store, &mut batch, &mut op, &other);
        assert_eq!(body_of(&store, id), body);
        let counters = store.bodies.counters(&path);
        assert_eq!(counters.physical_rereads, 1);
        assert_eq!(counters.digest_verifications, 1);
        drop(op);
    }
}

/// Phase-boundary verification on a RESIDENT body: unchanged metadata plus
/// changed content must refuse at logical reuse (check_reuse), not only on
/// an evicted cache miss. The metadata signature still matches, so only the
/// forced re-read+digest comparison can catch it.
#[test]
fn resident_change_with_unchanged_metadata_refuses_at_logical_reuse() {
    for family in ["raw", "gzip", "zstd"] {
        let work = Work::new("resident-reuse");
        let (path, body) = synthetic(&work, "reuse.blend", family);
        let (mut store, mut batch, mut op) = harness();
        let id = ensure_blend(&mut store, &mut batch, &mut op, &path);
        // The body stays RESIDENT: no later capture evicts the slot, and the
        // resident read costs no physical re-read.
        assert_eq!(store.bodies.resident_bodies(), 1);
        assert_eq!(body_of(&store, id), body);
        assert_eq!(store.bodies.counters(&path).physical_rereads, 0);
        store.bodies.set_recheck(|path, expected| {
            let metadata = std::fs::metadata(path).unwrap();
            assert_eq!(
                crate::fetch::capture_signature(&metadata),
                *expected,
                "metadata signature is unchanged at the reuse boundary"
            );
            let mut bytes = std::fs::read(path).unwrap();
            if path.file_name() == Some(std::ffi::OsStr::new("reuse.blend")) {
                bytes[0] ^= 0xff;
            }
            let mut hash = crate::edit_xxh::Xxh64::new();
            hash.update(&bytes);
            let digest = SourceDigest {
                physical: hash.digest(),
                decoded: hash.digest(),
            };
            Ok((bytes, digest))
        });
        op.record_touch(id);
        let refused = store.check_reuse(id, &mut op).unwrap_err();
        assert_eq!(refused.kind, "input_changed", "{family}: {refused:?}");
        assert!(
            refused.message.contains("digest changed since capture"),
            "{family}: {refused:?}"
        );
        let counters = store.bodies.counters(&path);
        assert_eq!(counters.physical_rereads, 1);
        assert_eq!(counters.digest_verifications, 1);
        assert!(store.bodies.latched(&path).is_some());
        // The latch short-circuits later checks without another re-read.
        let again = store.check_reuse(id, &mut op).unwrap_err();
        assert_eq!(again.kind, "input_changed");
        assert_eq!(store.bodies.counters(&path).physical_rereads, 1);
        assert!(
            matches!(
                op.diagnostics.last(),
                Some(crate::Diagnostic::SourceCheckFailure { source, error }) if source == &path
                    && error.kind == "input_changed"
            ),
            "{:?}",
            op.diagnostics
        );
        drop((batch, op));
    }
}

/// Phase-boundary verification at physical-container admission: a RESIDENT
/// body whose content changed under unchanged metadata must refuse BEFORE
/// any transport runs, so publication can never copy a changed container
/// under an old cached proof.
#[test]
fn resident_change_with_unchanged_metadata_refuses_at_publication_admission() {
    let work = Work::new("resident-publish");
    let (path, _body) = synthetic(&work, "publish.blend", "raw");
    let (mut store, mut batch, mut op) = harness();
    let _id = ensure_blend(&mut store, &mut batch, &mut op, &path);
    let expected = std::fs::metadata(&path).unwrap();
    assert_eq!(store.bodies.resident_bodies(), 1);
    store.bodies.set_recheck(|path, expected_signature| {
        let metadata = std::fs::metadata(path).unwrap();
        assert_eq!(
            crate::fetch::capture_signature(&metadata),
            *expected_signature,
            "metadata signature is unchanged at the admission boundary"
        );
        let mut bytes = std::fs::read(path).unwrap();
        if path.file_name() == Some(std::ffi::OsStr::new("publish.blend")) {
            bytes[0] ^= 0xff;
        }
        let mut hash = crate::edit_xxh::Xxh64::new();
        hash.update(&bytes);
        let digest = SourceDigest {
            physical: hash.digest(),
            decoded: hash.digest(),
        };
        Ok((bytes, digest))
    });
    let out = work.0.join("out.blend");
    let mut output = std::fs::File::create(&out).unwrap();
    let mut transported = false;
    let error = crate::session::copy_physical_using(
        &mut store,
        &mut batch,
        &path,
        &expected,
        &mut output,
        |_live, _path, _metadata, _output, _raw| {
            transported = true;
            Ok(0)
        },
    )
    .unwrap_err();
    assert!(!transported, "a changed source must never reach transport");
    assert_eq!(error.kind, "input_changed", "{error:?}");
    assert!(
        error.message.contains("digest changed since capture"),
        "{error:?}"
    );
    assert!(store.bodies.latched(&path).is_some());
    let counters = store.bodies.counters(&path);
    assert_eq!(counters.physical_rereads, 1);
    assert_eq!(counters.digest_verifications, 1);
    assert_eq!(std::fs::metadata(&out).unwrap().len(), 0);
    drop((op, batch, output));
}

/// No decoded blend body may ever wait in a queue: a blend prefetch is a
/// structural no-op, while the on-demand take still produces the checked
/// blend capture with its proof and content digest.
#[test]
fn blend_captures_are_never_queued_ahead() {
    let work = Work::new("no-blend-queue");
    let (path, _) = synthetic(&work, "queued.blend", "raw");
    let mut batch = CaptureBatch::with_memory(crate::session::MemoryMode::Bounded);
    batch.prefetch(&path, true).unwrap();
    assert_eq!(batch.pending_len(), 0);
    let raw = batch.take_raw(&path, true).unwrap();
    assert!(raw.proof.is_some());
    assert!(raw.digest.is_some());
    drop(batch);
    // Fast retains the original scheduled-reply path, not bounded's no-op.
    let mut fast = CaptureBatch::default();
    fast.prefetch(&path, true).unwrap();
    assert_eq!(fast.pending_len(), 1);
    let raw = fast.take_raw(&path, true).unwrap();
    assert!(raw.proof.is_some());
    assert!(raw.digest.is_none());
    assert_eq!(fast.pending_len(), 0);
}

/// Cross-filesystem fallback (copy_file_range -> EXDEV) must stream a
/// verified working body, never the live source: a write-only source FD
/// cannot satisfy a live reread, and the injected provider would refuse any
/// re-materialization. The resident verified bytes alone can carry it.
#[test]
fn cached_raw_fallback_streams_verified_body_without_live_read() {
    let work = Work::new("cached-fallback");
    let (path, body) = synthetic(&work, "fallback.blend", "raw");
    let (mut store, mut batch, mut op) = harness();
    let id = ensure_blend(&mut store, &mut batch, &mut op, &path);
    assert_eq!(store.bodies.resident_bodies(), 1);
    // Any attempt to re-materialize refuses loudly; only the resident
    // verified body may serve the fallback.
    store.bodies.set_recheck(|_, _| {
        Err(crate::Error::new(
            "io",
            "provider must not run for a resident body",
        ))
    });
    let write_only = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    let expected = std::fs::metadata(&path).unwrap();
    let out = work.0.join("out.blend");
    let mut output = std::fs::File::create(&out).unwrap();
    {
        let snapshot = store.snapshot(id).unwrap();
        let raw = snapshot.raw_capture().unwrap();
        assert!(matches!(raw, crate::input::Backing::Cached(_)));
        let copied = crate::kernel_copy::guarded(
            &write_only,
            &path,
            &expected,
            &mut output,
            Some(raw),
            |_, _, _, _| Err(std::io::Error::from_raw_os_error(libc::EXDEV)),
        )
        .unwrap();
        assert_eq!(copied, body.len() as u64);
    }
    assert_eq!(std::fs::read(&out).unwrap(), body);
    // No re-materialization happened: zero physical re-reads, and the body
    // is still resident for cheap later reads.
    assert_eq!(store.bodies.counters(&path).physical_rereads, 0);
    assert_eq!(store.bodies.resident_bodies(), 1);
    drop((batch, op, output));
}

/// When the source has latched a digest change, the cross-filesystem
/// fallback refuses with the backing's own typed latched error — exact kind
/// and message preserved through the transport seam, not rewritten by an
/// inferred kind/message match, and no duplicate independent cause is
/// invented.
#[test]
fn cached_raw_fallback_refuses_latched_source_with_typed_error() {
    let work = Work::new("cached-fallback-latched");
    let (path, body) = synthetic(&work, "latched.blend", "raw");
    let (mut store, mut batch, mut op) = harness();
    let id = ensure_blend(&mut store, &mut batch, &mut op, &path);
    // Evict the resident body, then read it: the flip provider produces a
    // digest mismatch that latches the source.
    let (other, _) = synthetic(&work, "other.blend", "raw");
    let other_id = ensure_blend(&mut store, &mut batch, &mut op, &other);
    store.bodies.set_recheck(|path, expected| {
        let metadata = std::fs::metadata(path).unwrap();
        assert_eq!(
            crate::fetch::capture_signature(&metadata),
            *expected,
            "metadata signature is unchanged at the fallback boundary"
        );
        let mut bytes = std::fs::read(path).unwrap();
        if path.file_name() == Some(std::ffi::OsStr::new("latched.blend")) {
            bytes[0] ^= 0xff;
        }
        let mut hash = crate::edit_xxh::Xxh64::new();
        hash.update(&bytes);
        let digest = SourceDigest {
            physical: hash.digest(),
            decoded: hash.digest(),
        };
        Ok((bytes, digest))
    });
    let refused = store
        .reader(id)
        .unwrap()
        .read_decompressed_bytes()
        .unwrap_err();
    assert_eq!(refused.kind, "input_changed");
    assert!(store.bodies.latched(&path).is_some());
    let write_only = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    let expected = std::fs::metadata(&path).unwrap();
    let out = work.0.join("out.blend");
    let mut output = std::fs::File::create(&out).unwrap();
    {
        let snapshot = store.snapshot(id).unwrap();
        let raw = snapshot.raw_capture().unwrap();
        let error = crate::kernel_copy::guarded(
            &write_only,
            &path,
            &expected,
            &mut output,
            Some(raw),
            |_, _, _, _| Err(std::io::Error::from_raw_os_error(libc::EXDEV)),
        )
        .unwrap_err();
        assert_eq!(error.kind, "input_changed");
        assert!(
            error.message.contains("digest changed since capture"),
            "{error:?}"
        );
    }
    // Nothing was copied, and the latch short-circuited without another
    // physical re-read at the transport seam.
    assert_eq!(std::fs::metadata(&out).unwrap().len(), 0);
    let counters = store.bodies.counters(&path);
    assert_eq!(counters.physical_rereads, 1);
    // The latched error is reachable once, with its original message, from
    // the store's own latch: the transport propagated that exact object.
    let latched = store.bodies.latched(&path).unwrap();
    assert_eq!(latched.kind, "input_changed");
    assert!(latched.message.contains("digest changed since capture"));
    // Later logical reuse consults the same latch (no duplicate cause).
    op.record_touch(id);
    let again = store.check_reuse(id, &mut op).unwrap_err();
    assert_eq!(again.kind, "input_changed");
    assert!(again.message.contains("digest changed since capture"));
    assert_eq!(store.bodies.counters(&path).physical_rereads, 1);
    let _ = (other_id, body);
    drop((batch, op, output));
}

#[test]
fn memory_mode_is_closed_default_fast_and_session_mismatch_refuses() {
    use crate::session::MemoryMode;
    assert_eq!(MemoryMode::default(), MemoryMode::Fast);
    assert_eq!(
        "bounded".parse::<MemoryMode>().unwrap(),
        MemoryMode::Bounded
    );
    let error = "automatic".parse::<MemoryMode>().unwrap_err();
    assert_eq!(error.kind, "usage");
    assert!(error.message.contains("automatic"));
    let work = Work::new("memory-mismatch");
    let (root, _) = synthetic(&work, "root.blend", "raw");
    for (opened, requested) in [
        (MemoryMode::Fast, MemoryMode::Bounded),
        (MemoryMode::Bounded, MemoryMode::Fast),
    ] {
        let mut session = crate::Session::open_with_memory(&root, opened)
            .into_result()
            .unwrap();
        let mut options = crate::pack::PackOptions::new(work.0.clone());
        options.memory = requested;
        let target = work.0.join(format!("mismatch-{}", opened.as_str()));
        let error = crate::pack::pack_session(&mut session, &target, &options)
            .into_result()
            .unwrap_err();
        assert_eq!(error.kind, "usage");
        assert!(error.message.contains(opened.as_str()));
        assert!(error.message.contains(requested.as_str()));
        assert!(error.message.contains("fixed at Session open"));
        assert!(!target.exists());
    }
}
