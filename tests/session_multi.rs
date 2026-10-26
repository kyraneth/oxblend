//! Selected roots share one captured Session; scalar reads never pick root zero.
use oxblend::{Session, session::MemoryMode};
use std::{io::Write, path::PathBuf};
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "oxblend-session-multi-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/library-null/normal-root.blend");
        for name in ["a.blend", "b.blend"] {
            std::fs::copy(&fixture, root.join(name)).unwrap();
        }
        Self(root)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn selected_order_dedup_empty_and_scalar_refusal() {
    let w = Work::new();
    let a = w.0.join("a.blend");
    let b = w.0.join("b.blend");
    let mut session = Session::open_many([&b, &a, &b], MemoryMode::Fast)
        .into_result()
        .unwrap();
    assert_eq!(session.root_paths(), vec![b.as_path(), a.as_path()]);
    assert!(session.validate_sources().is_ok());
    let error = session
        .read(|file| Ok(file.header().version))
        .into_result()
        .unwrap_err();
    assert_eq!(error.kind, "usage");
    assert!(error.message.contains("open(path)"));
    assert!(
        session
            .require_single("pack")
            .unwrap_err()
            .message
            .contains("pack_sequence")
    );
    let mut one = Session::open_many([&a, &a], MemoryMode::Fast)
        .into_result()
        .unwrap();
    assert_eq!(one.root_paths(), vec![a.as_path()]);
    assert!(one.read(|file| Ok(file.header().version)).is_ok());
    assert_eq!(
        Session::open_many(Vec::<PathBuf>::new(), MemoryMode::Fast)
            .into_result()
            .unwrap_err()
            .kind,
        "usage"
    );
}
#[test]
fn validation_checks_nonzero_root_and_keeps_changed_latch() {
    for memory in [MemoryMode::Fast, MemoryMode::Bounded] {
        let w = Work::new();
        let a = w.0.join("a.blend");
        let b = w.0.join("b.blend");
        let original = std::fs::read(&b).unwrap();
        let mut session = Session::open_many([&a, &b], memory).into_result().unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&b)
            .unwrap()
            .write_all(b"changed source")
            .unwrap();
        let operation = session.validate_sources();
        assert_eq!(operation.result.unwrap_err().kind, "input_changed");
        assert!(operation.diagnostics.iter().any(|d| matches!(d, oxblend::Diagnostic::SourceCheckFailure{source,error} if source==&b && error.kind=="input_changed")));
        std::fs::write(&b, original).unwrap();
        assert_eq!(
            session.validate_sources().into_result().unwrap_err().kind,
            "input_changed"
        );
    }
}
