//! Captured engine Session: open/read/trace/pack over one retained arena.
//!
//! These tests run against real fixture blends through the public Session
//! API. Run the parse-evidence test with a flattrace directory selected:
//! `FLATTRACE_DIR=<dir> FLATTRACE_LEVEL=2 cargo test --test session`.
#[path = "support/scratch.rs"]
mod test_scratch;

use oxblend::trace::{PointerMode, TraceMode, TraceOptions};
use oxblend::{Operation, Session};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dependencies")
        .join(name)
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let dir = test_scratch::scratch_dir(&std::env::temp_dir());
        let _ = label;
        Self(dir)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Root plus its linked library as an isolated writable pair. The fixture
/// closure also declares `//texture.png`, so a synthetic member stands in
/// for the farm-provided image.
fn linked_pair(work: &Work, root_name: &str, library_name: &str) -> (PathBuf, PathBuf) {
    let root = work.0.join(root_name);
    let library = work.0.join(library_name);
    std::fs::copy(fixture(root_name), &root).unwrap();
    std::fs::copy(fixture(library_name), &library).unwrap();
    std::fs::write(work.0.join("texture.png"), b"synthetic image bytes").unwrap();
    (root, library)
}

fn native_query_pair(work: &Work, with_library: bool) -> (PathBuf, PathBuf) {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/query-linked-session");
    let root = work.0.join("root.blend");
    let library = work.0.join("group-library.blend");
    std::fs::copy(fixtures.join("root.blend"), &root).unwrap();
    if with_library {
        std::fs::copy(fixtures.join("group-library.blend"), &library).unwrap();
        std::fs::copy(
            fixtures.join("forwarded.blend"),
            work.0.join("forwarded.blend"),
        )
        .unwrap();
    }
    (root, library)
}

#[test]
fn first_library_capture_failure_matches_path_trace_report() {
    for malformed in [false, true] {
        let work = Work::new("library-report-parity");
        let root = work.0.join("mesh-material-root.blend");
        std::fs::copy(fixture("mesh-material-root.blend"), &root).unwrap();
        if malformed {
            std::fs::write(work.0.join("mesh-material-library.blend"), b"not a blend").unwrap();
        }
        let expected = oxblend::trace::trace(&root, TraceOptions::default())
            .unwrap()
            .json();
        let mut session = Session::open(&root).into_result().unwrap();
        let actual = oxblend::trace::trace_session(&mut session, TraceOptions::default())
            .into_result()
            .unwrap()
            .json();
        assert_eq!(
            actual, expected,
            "first-use failures are report facts, not capture-phase text"
        );
    }
}

fn read_blocks(session: &mut Session) -> Operation<usize> {
    session.read(|file| Ok(file.blocks().len()))
}

#[test]
fn open_read_roundtrip_owns_results_after_close() {
    let work = Work::new("open");
    let (root, _library) = linked_pair(
        &work,
        "mesh-material-root.blend",
        "mesh-material-library.blend",
    );
    let opened = Session::open(&root);
    assert!(opened.result.is_ok(), "open failed: {:?}", opened.result);
    assert!(
        opened.diagnostics.is_empty(),
        "fresh open must not emit diagnostics: {:?}",
        opened.diagnostics
    );
    let mut session = opened.result.unwrap();
    let blocks = read_blocks(&mut session);
    let count = blocks.into_result().unwrap();
    assert!(count > 0);
    let summary = session.read(oxblend::summary::summarize_typed);
    assert!(
        summary.result.is_ok(),
        "typed summary failed: {:?}",
        summary.result
    );
    let summary = summary.into_result().unwrap();
    assert!(!summary.objects.is_empty() || !summary.scenes.is_empty());
    // Owned results survive Session close: nothing borrows the arena.
    drop(session);
    assert!(count > 0);
    assert!(summary.block_codes.values().sum::<usize>() > 0);
}

#[test]
fn read_preserves_query_failure_identity() {
    let work = Work::new("query-err");
    let (root, _library) = linked_pair(
        &work,
        "mesh-material-root.blend",
        "mesh-material-library.blend",
    );
    let mut session = Session::open(&root).into_result().unwrap();
    let failed: Operation<usize> = session.read(|_| {
        Err(oxblend::Error::new(
            "missing_field",
            "no such field in test query",
        ))
    });
    let error = failed.result.unwrap_err();
    assert_eq!(error.kind, "missing_field");
    assert_eq!(error.message, "no such field in test query");
    // Diagnostics exist on failure too; a pure query miss adds no source
    // notice, but the channel is always drained.
    let _ = failed.diagnostics;
}

#[test]
fn trace_session_repeated_walks_share_one_capture() {
    let work = Work::new("trace");
    let (root, _library) = linked_pair(
        &work,
        "mesh-material-root.blend",
        "mesh-material-library.blend",
    );
    let mut session = Session::open(&root).into_result().unwrap();
    let options = TraceOptions::new(TraceMode::Recursive, PointerMode::Strict);
    for _ in 0..2 {
        let walked = oxblend::trace::trace_session(&mut session, options);
        let report = walked.into_result().unwrap();
        assert!(report.complete, "fixture walk must be complete");
        assert!(!report.usages.is_empty());
    }
    // A summary read after retained linked capture still observes the root.
    let summary = session
        .read(oxblend::summary::summarize_typed)
        .into_result()
        .unwrap();
    assert!(!summary.block_codes.is_empty());
}

#[test]
fn trace_session_rejects_low_memory_before_traversal() {
    let work = Work::new("lowmem");
    let (root, _library) = linked_pair(
        &work,
        "mesh-material-root.blend",
        "mesh-material-library.blend",
    );
    let mut session = Session::open(&root).into_result().unwrap();
    let mut options = TraceOptions::new(TraceMode::Recursive, PointerMode::Strict);
    options.low_memory = true;
    let refused = oxblend::trace::trace_session(&mut session, options);
    let error = refused.result.unwrap_err();
    assert_eq!(error.kind, "usage");
    let _ = refused.diagnostics;
}

#[test]
fn pack_session_publishes_then_reuses_retained_capture() {
    let work = Work::new("pack");
    let (root, _library) = linked_pair(
        &work,
        "mesh-material-root.blend",
        "mesh-material-library.blend",
    );
    let mut session = Session::open(&root).into_result().unwrap();
    let target = work.0.join("package");
    let options = oxblend::pack::PackOptions::new(work.0.clone());
    let first = oxblend::pack::pack_session(&mut session, &target, &options);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    let report = first.into_result().unwrap();
    assert!(report.complete, "errors: {:?}", report.errors);
    assert!(target.is_dir());
    // A second pack over the same retained capture republishes facts.
    let target2 = work.0.join("package2");
    let second = oxblend::pack::pack_session(&mut session, &target2, &options);
    let report2 = second.into_result().unwrap();
    assert!(report2.complete, "errors: {:?}", report2.errors);
    assert!(target2.is_dir());
}

#[test]
fn changed_root_latches_later_calls() {
    let work = Work::new("latch");
    let (root, _library) = linked_pair(
        &work,
        "mesh-material-root.blend",
        "mesh-material-library.blend",
    );
    let mut session = Session::open(&root).into_result().unwrap();
    assert!(read_blocks(&mut session).result.is_ok());
    // Real mutation after capture: flip a content byte and force the
    // timestamps visibly away so the change detector must fire.
    {
        use std::os::unix::fs::FileExt;
        let file = std::fs::OpenOptions::new().write(true).open(&root).unwrap();
        file.write_all_at(b"X", 0).unwrap();
        file.set_modified(std::time::UNIX_EPOCH).unwrap();
    }
    let failed = read_blocks(&mut session);
    let error = failed.result.unwrap_err();
    assert_eq!(error.kind, "input_changed", "unexpected: {error:?}");
    assert_eq!(
        failed
            .diagnostics
            .iter()
            .filter(|d| matches!(
                d, oxblend::Diagnostic::SourceCheckFailure { source, .. } if source == &root
            ))
            .count(),
        1,
        "exactly one root check diagnostic: {:?}",
        failed.diagnostics
    );
    // The latch holds: a later call consulting the root still fails.
    let again = read_blocks(&mut session);
    assert_eq!(again.result.unwrap_err().kind, "input_changed");
    assert_eq!(
        again
            .diagnostics
            .iter()
            .filter(|d| matches!(
                d, oxblend::Diagnostic::SourceCheckFailure { source, .. } if source == &root
            ))
            .count(),
        1,
        "latched failure must be diagnosed exactly once per operation"
    );
    // And trace over the changed root refuses rather than trusting bytes.
    let options = TraceOptions::new(TraceMode::Recursive, PointerMode::Strict);
    let walked = oxblend::trace::trace_session(&mut session, options);
    assert_eq!(walked.result.unwrap_err().kind, "input_changed");
}

#[test]
fn changed_library_does_not_disable_unrelated_root_reads() {
    let work = Work::new("lib-latch");
    let (root, library) = linked_pair(
        &work,
        "mesh-material-root.blend",
        "mesh-material-library.blend",
    );
    let mut session = Session::open(&root).into_result().unwrap();
    let options = TraceOptions::new(TraceMode::Recursive, PointerMode::Strict);
    let walked = oxblend::trace::trace_session(&mut session, options);
    assert!(walked.into_result().unwrap().complete);
    // Mutate the linked library after it was captured.
    {
        use std::os::unix::fs::FileExt;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&library)
            .unwrap();
        file.write_all_at(b"X", 0).unwrap();
        file.set_modified(std::time::UNIX_EPOCH).unwrap();
    }
    // A walk consulting the library now refuses with typed diagnostics.
    let walked = oxblend::trace::trace_session(&mut session, options);
    let error = walked.result.unwrap_err();
    assert_eq!(error.kind, "input_changed");
    assert_eq!(
        walked
            .diagnostics
            .iter()
            .filter(|d| matches!(
                d, oxblend::Diagnostic::SourceCheckFailure { source, .. } if source == &library
            ))
            .count(),
        1,
        "one diagnostic per consulted failed library: {:?}",
        walked.diagnostics
    );
    assert_eq!(
        walked.diagnostics.len(),
        1,
        "a propagated guard is not an independent secondary: {:?}",
        walked.diagnostics
    );
    let target = work.0.join("refused-planning");
    let packed = oxblend::pack::pack_session(
        &mut session,
        &target,
        &oxblend::pack::PackOptions::new(work.0.clone()),
    );
    assert_eq!(packed.result.unwrap_err().kind, "input_changed");
    assert!(
        matches!(packed.diagnostics.as_slice(), [oxblend::Diagnostic::SourceCheckFailure { source, .. }] if source == &library),
        "nested trace/pack finalizers must not stack secondary copies: {:?}",
        packed.diagnostics
    );
    assert!(!target.exists());
    // A root-only read never consults the stale library and still succeeds.
    let blocks = read_blocks(&mut session);
    assert!(
        blocks.result.is_ok(),
        "root read failed: {:?}",
        blocks.result
    );
}

#[test]
fn session_moves_to_a_real_thread_for_read_trace_pack() {
    let work = Work::new("thread");
    let (root, _library) = linked_pair(
        &work,
        "mesh-material-root.blend",
        "mesh-material-library.blend",
    );
    let target = work.0.join("package-thread");
    let project = work.0.clone();
    let mut session = Session::open(&root).into_result().unwrap();
    let warmed = oxblend::trace::trace_session(&mut session, TraceOptions::default())
        .into_result()
        .unwrap();
    assert!(warmed.complete); // Move already-parsed root AND linked readers.
    let handle = std::thread::spawn(move || {
        let count = session
            .read(|file| Ok(file.blocks().len()))
            .into_result()
            .unwrap();
        let options = TraceOptions::new(TraceMode::Recursive, PointerMode::Strict);
        let report = oxblend::trace::trace_session(&mut session, options)
            .into_result()
            .unwrap();
        let pack_options = oxblend::pack::PackOptions::new(project);
        let packed = oxblend::pack::pack_session(&mut session, &target, &pack_options)
            .into_result()
            .unwrap();
        (count, report.usages.len(), packed.complete)
    });
    let (count, usages, complete) = handle.join().unwrap();
    assert!(count > 0 && usages > 0 && complete);
}

#[test]
fn two_handles_in_parallel_stay_independent() {
    let work = Work::new("parallel");
    let (root, _library) = linked_pair(
        &work,
        "mesh-material-root.blend",
        "mesh-material-library.blend",
    );
    let root2 = root.clone();
    let first = std::thread::spawn(move || {
        let mut session = Session::open(&root).into_result().unwrap();
        session.read(|file| Ok(file.blocks().len())).into_result()
    });
    let second = std::thread::spawn(move || {
        let mut session = Session::open(&root2).into_result().unwrap();
        session.read(|file| Ok(file.blocks().len())).into_result()
    });
    let (a, b) = (first.join().unwrap(), second.join().unwrap());
    assert_eq!(a.unwrap(), b.unwrap());
}

#[test]
fn open_missing_root_keeps_fetch_error_kind() {
    let work = Work::new("missing");
    let missing = work.0.join("no-such.blend");
    let opened = Session::open(&missing);
    let error = opened.result.unwrap_err();
    assert!(
        matches!(error.kind, "pack_path" | "io"),
        "unexpected kind: {error:?}"
    );
    assert!(
        opened.diagnostics.is_empty(),
        "an initial absent source was never captured; do not mislabel it as a failed reuse: {:?}",
        opened.diagnostics
    );
}

/// The L3 evaluator runs in the Session-owned two-phase operation: one
/// root capture, retained library views, guards before and after.
#[test]
fn query_evaluates_inside_retained_session_operation() {
    // The seed L3's own CLI contract uses: a successful evaluator run
    // inside one checked Session read, with guards applied around it.
    let seed = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/synthetic-empty-seed.blend");
    let mut session = Session::open(&seed).into_result().unwrap();
    let found = session.query("Scene", &["r.sfra".to_string(), "r.efra".to_string()]);
    assert!(found.result.is_ok(), "query failed: {:?}", found.result);
    assert!(found.diagnostics.is_empty(), "{:?}", found.diagnostics);
}

#[test]
fn identical_text_independent_body_error_is_not_mistaken_for_guard_propagation() {
    let work = Work::new("identical-body-error");
    let (root, _) = native_query_pair(&work, true);
    let mut session = Session::open(&root).into_result().unwrap();
    let mut changed = std::fs::read(&root).unwrap();
    changed.extend_from_slice(b"controlled root change inside independent body");
    let message = "source signature changed since snapshot";
    let failed: Operation<()> = session.read(|_| {
        std::fs::write(&root, &changed).unwrap();
        Err(oxblend::Error::new("input_changed", message))
    });
    let error = failed.result.unwrap_err();
    assert_eq!(error.kind, "input_changed");
    assert_eq!(
        error.message, message,
        "this control needs byte-identical errors, not merely the same kind"
    );
    assert!(
        matches!(failed.diagnostics.as_slice(),
        [oxblend::Diagnostic::SourceCheckFailure { source, .. }, oxblend::Diagnostic::SecondaryError { error, .. }]
        if source == &root && error.kind == "input_changed" && error.message == message),
        "independent same-text body evidence must survive: {:?}",
        failed.diagnostics
    );
}

#[test]
fn native_linked_query_reuses_captures_and_latches_only_consulted_sources() {
    let work = Work::new("native-query");
    let (root, library) = native_query_pair(&work, true);
    let original = std::fs::read(&library).unwrap();
    let mut session = Session::open(&root).into_result().unwrap();
    let root_fields = ["r.sfra".to_string()];
    let fields = ["compositor".to_string()];
    let independent = session.query("Scene", &root_fields).into_result().unwrap();
    let first = session.query("Scene", &fields).into_result().unwrap();
    assert_eq!(first["requested_coverage"]["complete"], true, "{first}");
    assert!(
        first["linked_subtype_observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["selected_from"] == "library_local"
                && entry["provider"]["owner"] == "session_retained_library_owner")
    );
    assert_eq!(
        session.query("Scene", &fields).into_result().unwrap(),
        first
    );
    let mut changed = original.clone();
    changed.extend_from_slice(b"controlled captured-library mutation");
    for content in [&changed, &original] {
        std::fs::write(&library, content).unwrap();
        let operation = session.query("Scene", &fields);
        assert_eq!(operation.result.unwrap_err().kind, "input_changed");
        assert!(
            matches!(operation.diagnostics.as_slice(),
            [oxblend::Diagnostic::SourceCheckFailure { source, error }]
                if source == &library && error.kind == "input_changed"),
            "one real check failure, not a secondary copy attributed to root: {:?}",
            operation.diagnostics
        );
        // Captured but unrelated stale libraries do not disable root-only reads.
        assert_eq!(
            session.query("Scene", &root_fields).into_result().unwrap(),
            independent
        );
    }
    drop(session);
    assert_eq!(first["requested_coverage"]["complete"], true);
}

#[test]
fn native_query_failed_first_capture_does_not_poison_later_explicit_call() {
    for malformed in [false, true] {
        let work = Work::new("native-query-first-failure");
        let (root, library) = native_query_pair(&work, false);
        if malformed {
            std::fs::write(&library, b"not a blend").unwrap();
        }
        let mut session = Session::open(&root).into_result().unwrap();
        let fields = ["compositor".to_string()];
        let unavailable = session.query("Scene", &fields);
        assert!(
            unavailable.diagnostics.is_empty(),
            "initial capture is not failed reuse: {:?}",
            unavailable.diagnostics
        );
        let report = unavailable.into_result().unwrap();
        assert_eq!(report["requested_coverage"]["complete"], false, "{report}");
        let expected = if malformed {
            "library_read"
        } else {
            "missing_library"
        };
        assert!(
            report["linked_subtype_diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|error| error["kind"] == expected),
            "{report}"
        );
        assert!(report["rows"][0]["values"]["compositor"]["partial_value"]["trees"].is_array());
        let fixtures =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/query-linked-session");
        std::fs::copy(fixtures.join("group-library.blend"), &library).unwrap();
        let available = session.query("Scene", &fields).into_result().unwrap();
        assert_eq!(
            available["requested_coverage"]["complete"], true,
            "{available}"
        );
    }
}

/// Parse evidence through the real tracing spans: one successful snapshot
/// sequence per distinct captured source, repeated walks without reparse.
/// A fresh exact-test child enables level-2 tracing explicitly; this proof
/// cannot silently pass when its caller forgot a trace environment.
#[test]
fn traced_parse_evidence_one_snapshot_per_source() {
    if std::env::var_os("OXBLEND_SESSION_TRACE_CHILD").is_none() {
        let work = Work::new("trace-child");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "traced_parse_evidence_one_snapshot_per_source",
                "--nocapture",
            ])
            .env("OXBLEND_SESSION_TRACE_CHILD", "1")
            .env("FLATTRACE_DIR", work.0.join("traces"))
            .env("FLATTRACE_LEVEL", "2")
            .env_remove("FLATTRACE_TRACEPARENT")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let dir = PathBuf::from(
        std::env::var_os("FLATTRACE_DIR").expect("trace child requires its explicit sink"),
    );
    let work = Work::new("spans");
    let (root, library) = linked_pair(
        &work,
        "mesh-material-root.blend",
        "mesh-material-library.blend",
    );
    let root_key = root.to_string_lossy().into_owned();
    let mut session = Session::open(&root).into_result().unwrap();
    let options = TraceOptions::new(TraceMode::Recursive, PointerMode::Strict);
    assert!(
        oxblend::trace::trace_session(&mut session, options)
            .into_result()
            .unwrap()
            .complete
    );
    let target = work.0.join("package-spans");
    let pack_options = oxblend::pack::PackOptions::new(work.0.clone());
    assert!(
        oxblend::pack::pack_session(&mut session, &target, &pack_options)
            .into_result()
            .unwrap()
            .complete
    );
    assert!(
        oxblend::trace::trace_session(&mut session, options)
            .into_result()
            .unwrap()
            .complete
    );
    drop(session);
    // Drain the writer: process exit flushes, but within the test the run
    // file already holds every completed span (writes flush per record).
    let mut snapshots = std::collections::BTreeMap::<String, usize>::new();
    let mut walk_starts = 0;
    let mut reuse_starts = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_file() {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        for line in text.lines() {
            let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if row["state"] != "start" {
                continue;
            }
            if row["event"] == "snapshot"
                && let Some(path) = row["data"]["path"].as_str()
                && (path == root_key || path == library.to_string_lossy())
            {
                *snapshots.entry(path.to_owned()).or_default() += 1;
            }
            if row["event"] == "reader.reuse" {
                reuse_starts += 1;
            }
            if row["event"] == "walk.file"
                && row["data"]["path"].as_str().is_some_and(|p| p == root_key)
            {
                walk_starts += 1;
            }
        }
    }
    assert_eq!(
        snapshots.get(&root_key),
        Some(&1),
        "root must parse once: {snapshots:?}"
    );
    assert_eq!(
        snapshots.get(library.to_string_lossy().as_ref()),
        Some(&1),
        "linked source must parse once: {snapshots:?}"
    );
    assert!(
        reuse_starts > 0,
        "retained operations must expose reader.reuse, not only absence of another parse"
    );
    assert!(
        walk_starts >= 2,
        "repeated walks must revisit without reparsing (walk.file starts: {walk_starts})"
    );
}
