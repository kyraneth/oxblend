//! L17 Phase 2 slice-6 red/green families for selective raw windows.
//! Mechanics (hits/misses/counters), family routing, byte equivalence,
//! hostile mutations (mtime, ctime-only, truncate, disappear, permission),
//! scan-order equivalence and EXDEV-style streamed publication.
use super::*;
use std::sync::atomic::Ordering;

static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "oxblend-selective-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        // Restore permissions so removal cannot fail after a chmod negative.
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
        for entry in std::fs::read_dir(&self.0).into_iter().flatten().flatten() {
            let _ = std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o600));
        }
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
use std::os::unix::fs::PermissionsExt;

fn seed_blend() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/synthetic-empty-seed.blend")
}

/// RED→GREEN: a sparse raw blend keeps no full body yet parses and serves
/// the exact historical bytes; the second identical read is a counted hit,
/// not a new physical fill.
#[test]
fn selective_capture_parses_and_reuses_windows_without_reread() {
    let work = Work::new();
    let source = work.path("seed.blend");
    std::fs::copy(seed_blend(), &source).unwrap();
    let full = std::fs::read(&source).unwrap();

    let snapshot = capture_selective_raw(&source)
        .unwrap()
        .expect("raw seed must enter the selective path");
    assert_eq!(snapshot.proof.family, "raw");
    assert!(matches!(snapshot.reader.data, Backing::RawWindows(_)));
    // Byte equivalence with the historical full body for stable input.
    assert_eq!(snapshot.reader.read_decompressed_bytes().unwrap(), full);

    let stats_before = snapshot.reader.data.stats();
    assert_eq!(stats_before["mode"], "raw_selective_windows");
    let fetched_before = stats_before["bytes_fetched"].as_u64().unwrap();
    assert!(fetched_before > 0 && fetched_before <= full.len() as u64);
    // Repeat the same range: hits rise, physical fills do not.
    let end = full.len().min(64);
    let first = snapshot.reader.data.range(0, end).unwrap().into_owned();
    assert_eq!(first, full[..end]);
    let mid_stats = snapshot.reader.data.stats();
    let second = snapshot.reader.data.range(0, end).unwrap().into_owned();
    assert_eq!(second, full[..end]);
    let stats_after = snapshot.reader.data.stats();
    assert_eq!(
        stats_after["bytes_fetched"].as_u64().unwrap(),
        mid_stats["bytes_fetched"].as_u64().unwrap(),
        "identical reread must reuse resident windows"
    );
    assert!(
        stats_after["window_hits"].as_u64().unwrap() > mid_stats["window_hits"].as_u64().unwrap()
    );
    let _ = fetched_before;
}

/// Compressed families never enter the selective representation: the caller
/// falls back to the historical full decoded capture.
#[test]
fn selective_defers_gzip_and_zstd_to_full_capture() {
    let work = Work::new();
    let raw = std::fs::read(seed_blend()).unwrap();
    for (name, bytes) in [
        ("seed.gz", {
            let mut encoder = flate2::GzBuilder::new()
                .mtime(0)
                .operating_system(255)
                .write(Vec::new(), flate2::Compression::default());
            use std::io::Write as _;
            encoder.write_all(&raw).unwrap();
            encoder.finish().unwrap()
        }),
        (
            "seed.zst",
            zstd::stream::encode_all(raw.as_slice(), 0).unwrap(),
        ),
    ] {
        let path = work.path(name);
        std::fs::write(&path, &bytes).unwrap();
        assert!(
            capture_selective_raw(&path).unwrap().is_none(),
            "{name} must fall back to full capture"
        );
    }
}

/// Multi-window mechanics on a synthetic 3 MiB raw file: ranges spanning
/// window boundaries serve exact bytes, each window fills once, holes are
/// consumed in increasing offsets through scan.
#[test]
fn selective_windows_span_boundaries_and_fetch_once() {
    let work = Work::new();
    let path = work.path("big.raw");
    let mut body = vec![0u8; 3 * 1024 * 1024 + 123];
    for (i, b) in body.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    std::fs::write(&path, &body).unwrap();
    let metadata = std::fs::metadata(&path).unwrap();
    let signature = crate::fetch::capture_signature(&metadata);
    let windows = RawWindows {
        path: path.clone(),
        len: body.len(),
        signature,
        quantum: crate::fetch::READ_CHUNK,
        cache: Mutex::new(BTreeMap::new()),
        windows_fetched: AtomicU64::new(0),
        window_hits: AtomicU64::new(0),
        bytes_fetched: AtomicU64::new(0),
    };
    // Straddle the first window boundary and the EOF tail.
    let span = windows
        .range_owned(crate::fetch::READ_CHUNK - 7, 31)
        .unwrap();
    assert_eq!(
        span,
        body[crate::fetch::READ_CHUNK - 7..crate::fetch::READ_CHUNK + 24]
    );
    assert_eq!(windows.windows_fetched.load(Ordering::Relaxed), 2);
    let tail = body.len() - 100;
    assert_eq!(windows.range_owned(tail, 100).unwrap(), body[tail..]);
    // Windows are strictly 1 MiB aligned: the span touched windows 0-1 and
    // the tail touched only the final partial window [3 MiB, len). The
    // middle window [2 MiB, 3 MiB) stays a hole until the sequential scan.
    assert_eq!(
        windows.bytes_fetched.load(Ordering::Relaxed) as usize,
        2 * crate::fetch::READ_CHUNK + (body.len() - 3 * crate::fetch::READ_CHUNK)
    );
    // Sequential scan consumes every byte exactly once in order.
    let mut seen = Vec::new();
    windows
        .scan_with(|reader| {
            std::io::copy(reader, &mut seen).map_err(|e| Error::new("io", e.to_string()))?;
            Ok(())
        })
        .unwrap();
    assert_eq!(seen, body);
    // Full coverage retains at most the physical length, never more.
    assert!(windows.bytes_fetched.load(Ordering::Relaxed) as usize <= body.len());
}

/// Non-regular files refuse at the probe without ever opening: a FIFO
/// with no writer would block `File::open` forever, so the worker order
/// (stat, admit regular files, then open) is load-bearing. The watchdog
/// fails the test instead of hanging the suite on regression.
#[test]
fn selective_fifo_probe_refuses_without_opening() {
    let work = Work::new();
    let fifo = work.path("lib.fifo");
    let cpath = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = capture_selective_raw(&fifo);
        let _ = done_tx.send(result.map(|_| ()));
    });
    match done_rx.recv_timeout(std::time::Duration::from_secs(10)) {
        Ok(Err(e)) => assert_eq!(e.kind, "pack_path"),
        Ok(Ok(())) => panic!("FIFO must not enter the selective path"),
        Err(_) => panic!("selective probe hung on a FIFO with no writer"),
    }
}

/// Fill-path truncation refuses as `input_changed` (worker-read analogue),
/// while scan boundaries keep the backing-level `truncated` kind.
#[test]
fn selective_fill_truncation_is_input_changed() {
    let work = Work::new();
    let path = work.path("big.raw");
    let body = vec![0xA5u8; 3 * crate::fetch::READ_CHUNK + 123];
    std::fs::write(&path, &body).unwrap();
    let metadata = std::fs::metadata(&path).unwrap();
    let signature = crate::fetch::capture_signature(&metadata);
    let windows = RawWindows {
        path: path.clone(),
        len: body.len(),
        signature,
        quantum: crate::fetch::READ_CHUNK,
        cache: Mutex::new(BTreeMap::new()),
        windows_fetched: AtomicU64::new(0),
        window_hits: AtomicU64::new(0),
        bytes_fetched: AtomicU64::new(0),
    };
    // Fill one window, then truncate the live source below it.
    assert_eq!(windows.range_owned(0, 8).unwrap(), body[..8]);
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(crate::fetch::READ_CHUNK as u64).unwrap();
    drop(file);
    // A fill reaching past the live end refuses like a failed worker read.
    let error = windows
        .range_owned(2 * crate::fetch::READ_CHUNK, 8)
        .unwrap_err();
    assert_eq!(error.kind, "input_changed");
    // The scan boundary itself keeps the backing-level kind.
    let error = windows.check_source().unwrap_err();
    assert_eq!(error.kind, "truncated");
}

/// Hostile mtime/content mutation between capture and reuse refuses closed.
#[test]
fn selective_mtime_mutation_refuses_before_reuse() {
    let work = Work::new();
    let source = work.path("seed.blend");
    std::fs::copy(seed_blend(), &source).unwrap();
    let snapshot = capture_selective_raw(&source)
        .unwrap()
        .expect("raw seed must enter the selective path");
    // Same-length hostile replacement with a forced visible timestamp.
    let mut body = std::fs::read(&source).unwrap();
    body[0] ^= 0xFF;
    std::fs::write(&source, &body).unwrap();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&source)
        .unwrap();
    file.set_modified(std::time::UNIX_EPOCH).unwrap();
    drop(file);
    // Hostile mutation between capture and reuse refuses at the guard
    // boundary (the Session latch sites: reuse checks, scan guards,
    // publication checks). Resident-window reads themselves reuse immutable
    // bytes exactly like the historical full-body backing; staleness is
    // never promoted because the guards fail first.
    let error = snapshot.reader.data.check_source().unwrap_err();
    assert_eq!(error.kind, "input_changed");
    let streamed = snapshot.reader.data.scan(|reader| {
        let mut out = Vec::new();
        std::io::copy(reader, &mut out).map_err(io)?;
        Ok(out)
    });
    assert_eq!(streamed.unwrap_err().kind, "input_changed");
}

/// Ctime-only mutation (permission change, same bytes/length/mtime) still
/// refuses: the guard is ctime-inclusive, not mtime-only.
#[test]
fn selective_ctime_only_mutation_refuses() {
    let work = Work::new();
    let source = work.path("seed.blend");
    std::fs::copy(seed_blend(), &source).unwrap();
    let snapshot = capture_selective_raw(&source)
        .unwrap()
        .expect("raw seed must enter the selective path");
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o600)).unwrap();
    let error = snapshot.reader.data.check_source().unwrap_err();
    assert_eq!(error.kind, "input_changed");
}

/// Truncation and disappearance fail closed with typed errors.
#[test]
fn selective_truncation_and_disappearance_refuse() {
    let work = Work::new();
    // Truncation.
    let truncated = work.path("truncated.blend");
    std::fs::copy(seed_blend(), &truncated).unwrap();
    let snapshot = capture_selective_raw(&truncated)
        .unwrap()
        .expect("raw seed must enter the selective path");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&truncated)
        .unwrap();
    file.set_len(16).unwrap();
    drop(file);
    // Guard-boundary refusal (see mtime test): the latched entry never
    // serves its resident windows again.
    let error = snapshot.reader.data.check_source().unwrap_err();
    assert_eq!(error.kind, "truncated");

    // Disappearance.
    let gone = work.path("gone.blend");
    std::fs::copy(seed_blend(), &gone).unwrap();
    let snapshot = capture_selective_raw(&gone)
        .unwrap()
        .expect("raw seed must enter the selective path");
    std::fs::remove_file(&gone).unwrap();
    let error = snapshot.reader.data.check_source().unwrap_err();
    assert!(error.kind == "io" || error.kind == "input_changed");
}

/// Unreadable source fails as IO, never as a fabricated latch.
#[test]
fn selective_unreadable_source_is_io_not_a_latch() {
    let work = Work::new();
    let source = work.path("seed.blend");
    std::fs::copy(seed_blend(), &source).unwrap();
    let snapshot = capture_selective_raw(&source)
        .unwrap()
        .expect("raw seed must enter the selective path");
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o000)).unwrap();
    let error = snapshot.reader.data.check_source().unwrap_err();
    assert_eq!(error.kind, "io");
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o600)).unwrap();
}

/// A mutation landing between plan (capture/parse) and publication also
/// refuses inside the streaming hole: the scan guard checks the live
/// source before and after the sequential stream.
#[test]
fn selective_publication_hole_mutation_refuses() {
    let work = Work::new();
    let source = work.path("seed.blend");
    std::fs::copy(seed_blend(), &source).unwrap();
    let snapshot = capture_selective_raw(&source)
        .unwrap()
        .expect("raw seed must enter the selective path");
    // Plan-time parse succeeds; mutate before the publication stream.
    let mut body = std::fs::read(&source).unwrap();
    body[64] ^= 0xFF;
    std::fs::write(&source, &body).unwrap();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&source)
        .unwrap();
    file.set_modified(std::time::UNIX_EPOCH).unwrap();
    drop(file);
    let streamed = snapshot.reader.data.scan(|reader| {
        let mut out = Vec::new();
        std::io::copy(reader, &mut out).map_err(io)?;
        Ok(out)
    });
    assert_eq!(streamed.unwrap_err().kind, "input_changed");
}

/// EXDEV-style fallback streams resident windows plus checked holes with no
/// restart: the publication hole is fetched once and the bytes are exact.
#[test]
fn selective_exdev_fallback_streams_windows_and_holes_exactly() {
    let work = Work::new();
    let source = work.path("source.blend");
    std::fs::copy(seed_blend(), &source).unwrap();
    let body = std::fs::read(&source).unwrap();
    let snapshot = capture_selective_raw(&source)
        .unwrap()
        .expect("raw seed must enter the selective path");
    // Write-only live FD: a userspace reread of the source is impossible, so
    // the transport can only succeed through the selective backing.
    let target_path = work.path("target.blend");
    let output = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&target_path)
        .unwrap();
    let live = std::fs::OpenOptions::new()
        .write(true)
        .open(&source)
        .unwrap();
    let mut target = output;
    let outcome = crate::kernel_copy::guarded(
        &live,
        &source,
        &snapshot.metadata,
        &mut target,
        snapshot.raw_capture(),
        |_, _, _, _| Err(std::io::Error::from_raw_os_error(libc::EXDEV)),
    )
    .unwrap();
    assert_eq!(outcome, body.len() as u64);
    drop(target);
    assert_eq!(std::fs::read(&target_path).unwrap(), body);
    // Publication consumed holes through windows: physical fills stay within
    // the source length (no whole-source reread beyond the body itself).
    let stats = snapshot.reader.data.stats();
    assert!(stats["bytes_fetched"].as_u64().unwrap() <= body.len() as u64);
}

/// A short/zero return after positive kernel progress never restarts the
/// stream: the partial result refuses and the target keeps one byte.
#[test]
fn selective_partial_progress_never_restarts() {
    let work = Work::new();
    let source = work.path("source.blend");
    std::fs::copy(seed_blend(), &source).unwrap();
    let body = std::fs::read(&source).unwrap();
    let snapshot = capture_selective_raw(&source)
        .unwrap()
        .expect("raw seed must enter the selective path");
    let target = work.path("target.blend");
    let mut target = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&target)
        .unwrap();
    let live = File::open(&source).unwrap();
    let mut calls = 0;
    let error = crate::kernel_copy::guarded(
        &live,
        &source,
        &snapshot.metadata,
        &mut target,
        snapshot.raw_capture(),
        |s, o, t, _| {
            calls += 1;
            if calls == 1 {
                crate::kernel_copy::native(s, o, t, 1)
            } else {
                Ok(0)
            }
        },
    )
    .unwrap_err();
    assert_eq!(calls, 2);
    assert_eq!(error.kind, "truncated");
    assert_eq!(target.metadata().unwrap().len(), 1);
    let _ = body;
}

/// L19 borrowed views: a range wholly inside one retained window serves
/// the exact historical bytes with no copy and no new source fill; the
/// second identical view is a counted hit, not a physical reread.
#[test]
fn window_view_borrows_single_window_without_copy_or_reread() {
    let work = Work::new();
    let source = work.path("seed.blend");
    std::fs::copy(seed_blend(), &source).unwrap();
    let full = std::fs::read(&source).unwrap();
    let snapshot = capture_selective_raw(&source)
        .unwrap()
        .expect("raw seed must enter the selective path");
    let end = full.len().min(64);
    let view = snapshot.reader.data.view(0, end).unwrap();
    assert!(
        matches!(view, WindowView::Window { .. }),
        "small in-window range must borrow, not copy"
    );
    assert_eq!(view.as_slice(), &full[..end]);
    assert_eq!(
        view.as_slice(),
        snapshot.reader.data.range(0, end).unwrap().as_ref()
    );
    let mid_stats = snapshot.reader.data.stats();
    let second = snapshot.reader.data.view(0, end).unwrap();
    assert_eq!(second.as_slice(), &full[..end]);
    let stats_after = snapshot.reader.data.stats();
    assert_eq!(
        stats_after["bytes_fetched"].as_u64().unwrap(),
        mid_stats["bytes_fetched"].as_u64().unwrap(),
        "identical review must reuse resident windows"
    );
    assert!(
        stats_after["window_hits"].as_u64().unwrap() > mid_stats["window_hits"].as_u64().unwrap()
    );
}

/// L19 borrowed views: a range straddling a window boundary falls back to
/// the guarded owned copy with byte identity, and out-of-range views
/// refuse exactly like `range`.
#[test]
fn window_view_spanning_falls_back_to_identical_owned() {
    let work = Work::new();
    let path = work.path("big.raw");
    let mut body = vec![0u8; 3 * 1024 * 1024 + 123];
    for (i, b) in body.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    std::fs::write(&path, &body).unwrap();
    let metadata = std::fs::metadata(&path).unwrap();
    let signature = crate::fetch::capture_signature(&metadata);
    let windows = RawWindows {
        path: path.clone(),
        len: body.len(),
        signature,
        quantum: crate::fetch::READ_CHUNK,
        cache: Mutex::new(BTreeMap::new()),
        windows_fetched: AtomicU64::new(0),
        window_hits: AtomicU64::new(0),
        bytes_fetched: AtomicU64::new(0),
    };
    let at = crate::fetch::READ_CHUNK - 7;
    let view = windows.view(at, 31).unwrap();
    assert!(
        matches!(view, WindowView::Owned(_)),
        "boundary-straddling range must take the owned fallback"
    );
    assert_eq!(view.as_slice(), &body[at..at + 31]);
    assert_eq!(view.as_slice(), windows.range_owned(at, 31).unwrap());
    assert!(windows.view(body.len() - 50, 51).is_err());
    assert_eq!(
        windows.view(body.len() - 50, 51).unwrap_err().kind,
        "truncated"
    );
}

/// L19 `payload_view`/`field_view`: byte identity with `payload` and
/// `field_bytes` on a selective capture, zero-copy where retained.
#[test]
fn payload_and_field_views_match_payload_bytes() {
    let work = Work::new();
    let source = work.path("seed.blend");
    std::fs::copy(seed_blend(), &source).unwrap();
    let snapshot = capture_selective_raw(&source)
        .unwrap()
        .expect("raw seed must enter the selective path");
    let reader = &snapshot.reader;
    for i in 0..reader.blocks().len().min(8) {
        let owned = reader.payload(i).unwrap();
        let view = reader.payload_view(i).unwrap();
        assert_eq!(view.as_slice(), owned.as_ref(), "block {i}");
        if reader.block(i).unwrap().size >= 8 {
            // Same 8-byte field through both paths: the checked owned API
            // and the borrowed window view (single-window borrow here).
            let field = reader.field_bytes(i, 0, 8).unwrap();
            let block = reader.block(i).unwrap();
            let fview = reader.data.view(block.data_offset, 8).unwrap();
            assert_eq!(fview.as_slice(), field.as_ref(), "block {i} field");
        }
    }
}
