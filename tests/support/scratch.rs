//! Shared test-only scratch ownership; each executable includes this module once.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

pub fn scratch_dir(base: &Path) -> PathBuf {
    std::fs::create_dir_all(base).unwrap();
    loop {
        let path = base.join(format!(
            "oxblend-test-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => return path,
            // A recycled pid may encounter a directory retained by an earlier run.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create scratch directory {}: {error}", path.display()),
        }
    }
}
