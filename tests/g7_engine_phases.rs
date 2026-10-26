//! G7 engine-phase instrumentation: focused CLI trace assertions.
//! Uses the real binary with FLATTRACE_DIR opt-in; asserts L1 visibility,
//! true decode/hash/copy/durability/publication phases, query.resolve,
//! reader reuse, and structured decision/error points with reconciled identities.
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn binary() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_oxblend"));
    command
        .env_remove("FLATTRACE_DIR")
        .env_remove("FLATTRACE_LEVEL")
        .env_remove("FLATTRACE_TRACEPARENT");
    command
}

fn seed() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/synthetic-empty-seed.blend")
}

fn traced(args: &[String], level: &str) -> (std::process::Output, Vec<Value>) {
    let work = std::env::temp_dir().join(format!(
        "oxblend-g7-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&work).unwrap();
    let traces = work.join("traces");
    std::fs::create_dir_all(&traces).unwrap();
    let mut command = binary();
    for arg in args {
        command.arg(arg);
    }
    command.env("FLATTRACE_DIR", &traces);
    if !level.is_empty() {
        command.env("FLATTRACE_LEVEL", level);
    }
    let output = command.output().unwrap();
    let files: Vec<_> = std::fs::read_dir(&traces).unwrap().collect();
    assert_eq!(files.len(), 1, "expected one trace file");
    let text = std::fs::read_to_string(files.into_iter().next().unwrap().unwrap().path()).unwrap();
    let rows: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    std::fs::remove_dir_all(&work).unwrap();
    (output, rows)
}

fn starts(rows: &[Value], event: &str) -> usize {
    rows.iter()
        .filter(|row| row["event"] == event && row["state"] == "start")
        .count()
}

fn points<'a>(rows: &'a [Value], event: &str) -> Vec<&'a Value> {
    rows.iter()
        .filter(|row| row["event"] == event && row["state"] == "point")
        .collect()
}

fn has_run_anchored_start(rows: &[Value], event: &str) -> bool {
    let by_span: std::collections::HashMap<String, &Value> = rows
        .iter()
        .filter(|row| row["state"] == "start")
        .map(|row| (row["span"].as_str().unwrap().to_owned(), row))
        .collect();
    rows.iter()
        .filter(|row| row["event"] == event && row["state"] == "start")
        .any(|row| {
            let mut parent = row["parent"].as_str().map(str::to_owned);
            let mut seen_run = row["event"] == "run";
            while let Some(span) = parent {
                if let Some(record) = by_span.get(&span) {
                    if record["event"] == "run" {
                        seen_run = true;
                        break;
                    }
                    parent = record["parent"].as_str().map(str::to_owned);
                } else {
                    break;
                }
            }
            seen_run
        })
}

#[test]
fn summary_phases_visible_at_l1() {
    let args = vec!["summary".to_string(), seed().to_string_lossy().to_string()];
    let (output, rows) = traced(&args, "1");
    assert!(output.status.success());
    for event in ["run", "summary", "file.open", "index", "schema"] {
        assert_eq!(starts(&rows, event), 1, "missing L1 start: {event}");
        assert!(
            has_run_anchored_start(&rows, event),
            "no run-anchored path for {event}"
        );
    }
}

#[test]
fn trace_walk_phases_visible_at_l1_with_truthful_outcomes() {
    let args = vec!["trace".to_string(), seed().to_string_lossy().to_string()];
    let (output, rows) = traced(&args, "1");
    assert!(output.status.success());
    for event in ["walk.file", "walk.select", "walk.edges", "walk.assets"] {
        assert!(starts(&rows, event) >= 1, "missing L1 start: {event}");
    }
    for event in ["walk.file", "walk.select", "walk.edges", "walk.assets"] {
        let ends: Vec<_> = rows
            .iter()
            .filter(|row| row["event"] == event && row["state"] == "end")
            .collect();
        assert_eq!(ends.len(), starts(&rows, event), "unpaired {event}");
        for end in ends {
            assert!(
                ["ok", "partial", "error", "refused"].contains(&end["outcome"].as_str().unwrap()),
                "unexpected outcome for {event}: {end}"
            );
        }
    }
}

#[test]
fn compressed_input_emits_true_decode() {
    let work = std::env::temp_dir().join(format!(
        "oxblend-g7-zstd-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&work).unwrap();
    let raw = std::fs::read(seed()).unwrap();
    let compressed = zstd::stream::encode_all(raw.as_slice(), 0).unwrap();
    assert!(compressed != raw);
    let input = work.join("compressed.blend");
    std::fs::write(&input, &compressed).unwrap();
    let args = vec!["summary".to_string(), input.to_string_lossy().to_string()];
    let (output, rows) = traced(&args, "1");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        starts(&rows, "decode") >= 1,
        "compressed input must emit a true decode phase"
    );
    std::fs::remove_dir_all(&work).unwrap();
}

#[test]
fn list_sha256_emits_hash_and_query_resolve() {
    let work = std::env::temp_dir().join(format!("oxblend-g7-hash-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    let input = work.join("image.blend");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/library-null/image.blend"),
        &input,
    )
    .unwrap();
    std::fs::write(work.join("pixel.png"), b"opaque-copy-fixture\n").unwrap();
    // --sha256 is a text-list option; JSON deliberately ignores it. Use a
    // real physical asset so a requested hash actually executes.
    let args = vec![
        "list".to_string(),
        input.to_string_lossy().to_string(),
        "--sha256".to_string(),
    ];
    let (output, rows) = traced(&args, "1");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        starts(&rows, "hash") >= 1,
        "requested hashing must emit a hash phase"
    );
    assert!(
        points(&rows, "hot.summary")
            .iter()
            .any(|row| row["data"]["event"] == "query.resolve"
                && row["data"]["count"].as_u64().unwrap() > 0),
        "listing must aggregate query.resolve"
    );
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn pack_dry_run_forbids_execution_and_publication() {
    let work = std::env::temp_dir().join(format!(
        "oxblend-g7-dry-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&work).unwrap();
    let target = work.join("dry-target");
    let args = vec![
        "pack".to_string(),
        seed().to_string_lossy().to_string(),
        target.to_string_lossy().to_string(),
        "--dry-run".to_string(),
    ];
    let (output, rows) = traced(&args, "1");
    assert!(!target.exists(), "dry-run must write nothing");
    assert!(starts(&rows, "pack.operation") >= 1);
    assert!(starts(&rows, "pack.plan") >= 1);
    assert_eq!(
        starts(&rows, "pack.execute"),
        0,
        "dry-run forbids pack.execute"
    );
    assert_eq!(starts(&rows, "publish"), 0, "dry-run forbids publish");
    assert_eq!(starts(&rows, "write"), 0, "dry-run forbids write");
    assert_eq!(starts(&rows, "durability"), 0, "dry-run forbids durability");
    drop(output);
    std::fs::remove_dir_all(&work).unwrap();
}

#[test]
fn pack_real_directory_covers_copy_publish_and_reuse() {
    let work = std::env::temp_dir().join(format!(
        "oxblend-g7-pack-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&work).unwrap();
    let project = work.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let root = project.join("root.blend");
    std::fs::copy(seed(), &root).unwrap();
    let target = work.join("package");
    let args = vec![
        "pack".to_string(),
        root.to_string_lossy().to_string(),
        target.to_string_lossy().to_string(),
    ];
    let (output, rows) = traced(&args, "1");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(target.is_dir(), "pack must publish a directory");
    for event in [
        "pack.operation",
        "pack.plan",
        "pack.execute",
        "publish",
        "snapshot",
        "fetch",
    ] {
        assert!(starts(&rows, event) >= 1, "pack must emit L1 {event}");
    }
    assert!(
        starts(&rows, "copy") >= 1
            || starts(&rows, "pack.repath") >= 1
            || starts(&rows, "pack.copy") >= 1,
        "pack must emit a copy/member phase"
    );
    assert!(
        starts(&rows, "reader.reuse") >= 1,
        "pack execution must emit retained reader.reuse"
    );
    std::fs::remove_dir_all(&work).unwrap();
}

#[test]
fn decisions_and_errors_emit_structured_points_with_identities() {
    let work = std::env::temp_dir().join(format!(
        "oxblend-g7-missing-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&work).unwrap();
    let project = work.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let root = project.join("root.blend");
    std::fs::copy(seed(), &root).unwrap();
    let target = work.join("package");
    let args = vec![
        "pack".to_string(),
        root.to_string_lossy().to_string(),
        target.to_string_lossy().to_string(),
        "--permissive".to_string(),
        "--exclude".to_string(),
        "*.blend".to_string(),
    ];
    let (_output, rows) = traced(&args, "1");
    let decisions = points(&rows, "decision");
    assert!(
        !decisions.is_empty(),
        "skipped/excluded reasons must emit decision points"
    );
    for point in &decisions {
        let data = &point["data"];
        assert!(
            data["item"].is_string(),
            "decision needs item identity: {point}"
        );
        assert!(data["kind"].is_string(), "decision needs kind: {point}");
        assert!(data["reason"].is_string(), "decision needs reason: {point}");
    }
    for point in points(&rows, "error") {
        let data = &point["data"];
        assert!(data["kind"].is_string(), "error needs kind: {point}");
        assert!(data["message"].is_string(), "error needs message: {point}");
    }
    std::fs::remove_dir_all(&work).unwrap();
}

#[test]
fn strict_refusal_forbids_publication_and_keeps_decision() {
    let work = std::env::temp_dir().join(format!(
        "oxblend-g7-refuse-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&work).unwrap();
    let target = work.join("package");
    let args = vec![
        "pack".to_string(),
        seed().to_string_lossy().to_string(),
        target.to_string_lossy().to_string(),
        "-p".to_string(),
        work.join("absent-project").to_string_lossy().to_string(),
    ];
    let (output, rows) = traced(&args, "1");
    assert!(!output.status.success(), "refusal must fail the command");
    assert!(!target.exists(), "strict refusal publishes nothing");
    assert_eq!(starts(&rows, "pack.execute"), 0);
    assert_eq!(starts(&rows, "publish"), 0);
    assert!(
        !points(&rows, "error").is_empty() || !points(&rows, "decision").is_empty(),
        "refusal must keep a structured cause"
    );
    std::fs::remove_dir_all(&work).unwrap();
}

#[test]
fn durability_phase_at_actual_sync_owner() {
    let work = std::env::temp_dir().join(format!(
        "oxblend-g7-dur-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&work).unwrap();
    let project = work.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let root = project.join("root.blend");
    std::fs::copy(seed(), &root).unwrap();
    let target = work.join("package-each");
    let args = vec![
        "pack".to_string(),
        root.to_string_lossy().to_string(),
        target.to_string_lossy().to_string(),
        "--durability".to_string(),
        "each-file".to_string(),
    ];
    let (output, rows) = traced(&args, "1");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        starts(&rows, "durability") >= 1,
        "each-file durability must emit a durability phase"
    );
    assert!(!points(&rows, "decision").is_empty() || starts(&rows, "publish") >= 1);
    std::fs::remove_dir_all(&work).unwrap();
}
