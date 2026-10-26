//! Destination-owned named exclusive staging and atomic no-replace hard-link publication.
//! Prefix `.oxblend-stage-`; errors/drop remove staging, a kill may leave that name.
//! Final names never expose partial bytes. One mechanism on every filesystem.
use crate::{Error, Result};
use std::collections::BTreeSet;
use std::ffi::CString;
use std::fs::{File, Metadata};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Explicit output persistence policy; complete atomic publication is independent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Durability {
    #[default]
    None,
    End,
    EachFile,
}
impl Durability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::End => "end",
            Self::EachFile => "each-file",
        }
    }
    pub fn mechanism(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::End => "syncfs-destination-filesystem",
            Self::EachFile => "file-and-directory-fsync",
        }
    }
}
impl std::str::FromStr for Durability {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "end" => Ok(Self::End),
            "each-file" => Ok(Self::EachFile),
            _ => Err(Error::new(
                "durability",
                "durability requires none|end|each-file",
            )),
        }
    }
}
pub(crate) fn sync_directories(directories: &BTreeSet<PathBuf>) -> Result<()> {
    // Actual durability phase at the directory-fsync owner.
    let span = crate::diagnostics::Span::new(
        "durability",
        if crate::diagnostics::enabled() {
            serde_json::json!({"mechanism":"file-and-directory-fsync","directories":directories.len()})
        } else {
            serde_json::Value::Null
        },
    );
    let result = (|| {
        let mut ordered = directories.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
        for directory in ordered {
            File::open(directory)
                .and_then(|f| f.sync_all())
                .map_err(|e| io_at("open/fsync directory", directory, e))?;
        }
        Ok(())
    })();
    span.result(result)
}
#[cfg(target_os = "linux")]
pub(crate) fn sync_end(directory: &Path) -> Result<()> {
    // Actual durability phase at the syncfs owner.
    let span = crate::diagnostics::Span::new(
        "durability",
        if crate::diagnostics::enabled() {
            serde_json::json!({"mechanism":"syncfs-destination-filesystem","path":directory.to_string_lossy()})
        } else {
            serde_json::Value::Null
        },
    );
    let result = (|| {
        let directory_path = directory;
        let directory = File::open(directory).map_err(|e| {
            Error::new(
                "durability",
                format!("open syncfs directory {}: {e}", directory_path.display()),
            )
        })?;
        let result = unsafe { libc::syncfs(directory.as_raw_fd()) };
        if result < 0 {
            return Err(Error::new(
                "durability",
                format!(
                    "syncfs directory {}: {}",
                    directory_path.display(),
                    std::io::Error::last_os_error()
                ),
            ));
        }
        Ok(())
    })();
    span.result(result)
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn sync_end(_directory: &Path) -> Result<()> {
    Err(Error::new(
        "durability",
        "End syncfs is only supported and qualified on Linux",
    ))
}
pub(crate) fn create_directories(path: &Path, durability: Durability) -> Result<()> {
    if durability != Durability::EachFile {
        return std::fs::create_dir_all(path).map_err(|e| io_at("create directories", path, e));
    }
    let new = path
        .ancestors()
        .take_while(|p| !p.exists())
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    std::fs::create_dir_all(path).map_err(|e| io_at("create directories", path, e))?;
    let mut affected = BTreeSet::new();
    for directory in new {
        affected.insert(directory.clone());
        if let Some(parent) = directory.parent() {
            affected.insert(parent.to_path_buf());
        }
    }
    if durability == Durability::EachFile {
        sync_directories(&affected)?;
    }
    Ok(())
}
const TEMP_PREFIX: &str = ".oxblend-stage-";
// Namespace generation only, not cached input state or a retry/resource bound.
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Shared canonical output context: part of the CLI canonical message,
/// never stripped as Session presentation. The binding preserves it exactly.
/// A shared operation prefix also closes any inner Session-only layer: the
/// full prefixed message from this owner outward IS the canonical CLI
/// message, so a later binding strip must not drop it. Clearing the
/// recorded Session base here is what lets e.g. `pack 'dest':` survive
/// `without_context` while pure read-path Session presentation
/// (`fetch/read snapshot`, `snapshot/index` with no shared prefix outside)
/// still restores exactly.
pub(crate) fn context(error: Error, operation: &str, path: &Path) -> Error {
    // Shared owner: do NOT record a strippable Session base. The full
    // prefixed message IS the canonical CLI message; Error::without_context
    // (binding Session strip) leaves it intact. Session-only presentation
    // uses session_context below so only that layer restores.
    // Operation-owner floor: a shared prefix applied outside an inner
    // Session layer (e.g. pack wrapping a snapshot/index capture error)
    // declares the whole message canonical, so the recorded Session base
    // is dropped and a later strip keeps the full CLI message. Read paths
    // apply no shared prefix, so their Session presentation still strips.
    let mut error = error;
    error.context_message = None;
    error.message = format!("{operation} '{}': {}", path.display(), error.message);
    error
}

/// Session-only presentation: recorded as strippable so the binding can
/// restore the shared canonical cause without parsing. Kind is never touched.
pub(crate) fn session_context(mut error: Error, operation: &str, path: &Path) -> Error {
    if error.context_message.is_none() {
        error.context_message = Some(error.message.clone().into_boxed_str());
    }
    error.message = format!("{operation} '{}': {}", path.display(), error.message);
    error
}
pub(crate) fn io_at(operation: &str, path: &Path, error: std::io::Error) -> Error {
    context(io(error), operation, path)
}
pub(crate) fn io(e: std::io::Error) -> Error {
    Error::new(
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            "broken_pipe"
        } else {
            "io"
        },
        e.to_string(),
    )
}
pub(crate) fn name(path: &Path) -> Result<CString> {
    if path.as_os_str().as_bytes().ends_with(b"/") {
        return Err(Error::new(
            "output_path",
            "file output has a trailing separator",
        ));
    }
    let value = path
        .file_name()
        .ok_or_else(|| Error::new("output_path", "output has no file name"))?;
    if value.as_bytes().starts_with(TEMP_PREFIX.as_bytes()) {
        return Err(Error::new(
            "output_path",
            "output basename uses reserved .oxblend-stage- prefix",
        ));
    }
    CString::new(value.as_bytes()).map_err(|_| Error::new("output_path", "NUL in output name"))
}
pub(crate) fn refuse_existing(source: &Metadata, output: &Path) -> Result<()> {
    match std::fs::symlink_metadata(output) {
        Ok(_) => {
            let same = output
                .metadata()
                .is_ok_and(|m| m.dev() == source.dev() && m.ino() == source.ino());
            Err(Error::new(
                if same { "same_input" } else { "output_exists" },
                "output path already exists; no replacement permitted",
            ))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io(e)),
    }
}
pub(crate) struct AtomicOutput {
    pub file: File,
    output: PathBuf,
    write_span: Option<crate::diagnostics::Span>,
    parent: File,
    name: CString,
    temp: Option<CString>,
    pub published: bool,
    pub durability_completed: bool,
}
impl AtomicOutput {
    pub fn create(source: &Metadata, output: &Path) -> Result<Self> {
        Self::create_inner(source, output).map_err(|e| context(e, "create output stage", output))
    }
    fn create_inner(source: &Metadata, output: &Path) -> Result<Self> {
        refuse_existing(source, output)?;
        let name = name(output)?;
        let parent_path = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let parent = File::open(parent_path).map_err(io)?;
        if !parent.metadata().map_err(io)?.is_dir() {
            return Err(Error::new(
                "output_path",
                "output parent is not a directory",
            ));
        }
        // O_EXCL owns exactly one named inode. An existing/crash-leftover name
        // is never opened/replaced; generate another name without a guessed cap.
        let (file, temp) = loop {
            let temp = CString::new(format!(
                "{TEMP_PREFIX}{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ))
            .map_err(|_| Error::new("output_path", "staging name"))?;
            let fd = unsafe {
                libc::openat(
                    parent.as_raw_fd(),
                    temp.as_ptr(),
                    libc::O_CREAT | libc::O_EXCL | libc::O_RDWR | libc::O_CLOEXEC,
                    0o666,
                )
            };
            if fd >= 0 {
                break (unsafe { File::from_raw_fd(fd) }, temp);
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(io(error));
            }
        };
        let stage = Self {
            file,
            output: output.to_path_buf(),
            write_span: Some(crate::diagnostics::Span::new(
                "write",
                if crate::diagnostics::enabled() {
                    serde_json::json!({"path":output.to_string_lossy()})
                } else {
                    serde_json::Value::Null
                },
            )),
            parent,
            name,
            temp: Some(temp),
            published: false,
            durability_completed: false,
        };
        // Preserve the source's ordinary permission bits independent of umask;
        // never propagate setuid/setgid/sticky bits to rewritten content.
        if unsafe { libc::fchmod(stage.file.as_raw_fd(), source.mode() & 0o777) } < 0 {
            return Err(io(std::io::Error::last_os_error()));
        }
        Ok(stage)
    }
    fn remove_temp(&mut self) -> Result<()> {
        if let Some(temp) = &self.temp {
            let result = unsafe { libc::unlinkat(self.parent.as_raw_fd(), temp.as_ptr(), 0) };
            if result < 0 {
                return Err(io(std::io::Error::last_os_error()));
            }
            self.temp = None;
        }
        Ok(())
    }
    pub fn publish(&mut self, durability: Durability) -> Result<()> {
        if let Some(mut span) = self.write_span.take() {
            if span.active() {
                span.data(serde_json::json!({"bytes":self.file.metadata().ok().map(|m| m.len())}));
            }
            span.end("ok");
        }
        // Publication with truthful output inventory: path, requested and
        // completed durability travel in start/end data for the root ledger.
        let mut span = crate::diagnostics::Span::new(
            "publish",
            if crate::diagnostics::enabled() {
                serde_json::json!({"path":self.output.to_string_lossy(),"path_hex":crate::value::hex(self.output.as_os_str().as_encoded_bytes()),"durability_requested":durability.as_str(),"durability_mechanism":durability.mechanism()})
            } else {
                serde_json::Value::Null
            },
        );
        let result = self
            .publish_inner(durability)
            .map_err(|e| context(e, "publish/sync output", &self.output));
        crate::failure_receipt::record_publication(
            &self.output,
            self.published,
            self.durability_completed,
        );
        if span.active() {
            // Actual staged bytes are known even when publication fails;
            // output_written/durability_completed record the true effect.
            span.data(serde_json::json!({"path":self.output.to_string_lossy(),"bytes":self.file.metadata().ok().map(|m| m.len()),"output_written":self.published,"durability_requested":durability.as_str(),"durability_completed":self.durability_completed,"durability_pending":durability == Durability::End,"effect_scope":"at_publication; end-sync effects follow in output.effect"}));
        }
        let result = span.result(result);
        if let Err(e) = &result {
            // Report-carried publication failures reconcile by kind/message.
            if crate::diagnostics::enabled() {
                crate::diagnostics::point(
                    "error",
                    serde_json::json!({"kind":e.kind,"message":e.message,"path":self.output.to_string_lossy(),"phase":"publish"}),
                );
            }
        }
        result
    }
    fn publish_inner(&mut self, durability: Durability) -> Result<()> {
        if durability == Durability::EachFile {
            // Actual per-file durability at the file-sync owner.
            let dspan = crate::diagnostics::Span::new(
                "durability",
                if crate::diagnostics::enabled() {
                    serde_json::json!({"mechanism":"file-and-directory-fsync","scope":"stage_file"})
                } else {
                    serde_json::Value::Null
                },
            );
            let r = self.file.sync_all().map_err(io);
            dspan.result(r)?;
        }
        // Normal named-inode link(2) semantics, directory-FD anchored via linkat.
        // No /proc FD source, AT_SYMLINK_FOLLOW, filesystem branch or fallback.
        // EEXIST includes a destination created after preflight: it always wins.
        let temp = self
            .temp
            .as_ref()
            .ok_or_else(|| Error::new("output_path", "stage already removed"))?;
        let result = unsafe {
            libc::linkat(
                self.parent.as_raw_fd(),
                temp.as_ptr(),
                self.parent.as_raw_fd(),
                self.name.as_ptr(),
                0,
            )
        };
        if result < 0 {
            let e = std::io::Error::last_os_error();
            return Err(if e.kind() == std::io::ErrorKind::AlreadyExists {
                Error::new(
                    "output_exists",
                    "destination appeared before publication; not replaced",
                )
            } else {
                io(e)
            });
        }
        // This is the actual publication boundary even if subsequent unlink IO
        // fails. Callers must retain output_written=true in that partial report.
        self.published = true;
        self.remove_temp()?;
        if durability == Durability::EachFile {
            // Actual per-file durability at the file-sync owner.
            let dspan2 = crate::diagnostics::Span::new(
                "durability",
                if crate::diagnostics::enabled() {
                    serde_json::json!({"mechanism":"file-and-directory-fsync","scope":"parent_directory"})
                } else {
                    serde_json::Value::Null
                },
            );
            let r = self.parent.sync_all().map_err(io);
            dspan2.result(r)?;
            self.durability_completed = true;
        }
        Ok(())
    }
}
impl Drop for AtomicOutput {
    fn drop(&mut self) {
        // All recoverable error/cancellation paths attempt cleanup; a filesystem
        // refusing unlink cannot be promised away. Never unlink the final name.
        let _ = self.remove_temp();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn shared_context_is_canonical_and_session_only_strips() {
        let error = Error::new("io", "synthetic producer message");
        error
            .diagnostic
            .set(std::sync::Arc::new(crate::failure_receipt::DiagnosticRef {
                run_id: "synthetic-unit-run".into(),
                failure_id: "synthetic-unit-failure".into(),
            }))
            .unwrap();
        let reference = error.diagnostic_ref();
        // Shared canonical contexts nest without recording a strippable base:
        // the full prefixed message IS the CLI canonical message.
        let shared = context(
            context(error, "fetch", Path::new("library.blend")),
            "snapshot",
            Path::new("library.blend"),
        );
        assert_eq!(
            shared.message,
            "snapshot 'library.blend': fetch 'library.blend': synthetic producer message"
        );
        // Shared-only: without_context preserves every shared prefix.
        let preserved = shared.clone().without_context();
        assert_eq!(preserved.message, shared.message);
        assert_eq!(preserved.kind, "io");
        assert_eq!(preserved.diagnostic_ref(), reference);
        // Session-only outer presentation records the shared-prefixed base so
        // the binding strips only the Session layer, preserving shared.
        let base = Error::new("io", "synthetic producer message");
        base.diagnostic
            .set(std::sync::Arc::new(crate::failure_receipt::DiagnosticRef {
                run_id: "synthetic-unit-run".into(),
                failure_id: "synthetic-unit-failure".into(),
            }))
            .unwrap();
        let presented = session_context(
            context(base, "fetch", Path::new("library.blend")),
            "snapshot/index",
            Path::new("library.blend"),
        );
        assert_eq!(
            presented.message,
            "snapshot/index 'library.blend': fetch 'library.blend': synthetic producer message"
        );
        let cause = presented.clone().without_context();
        assert_eq!(
            cause.message,
            "fetch 'library.blend': synthetic producer message"
        );
        assert_eq!(cause.kind, "io");
        assert_eq!(cause.diagnostic_ref(), reference);
        assert_eq!(presented.diagnostic_ref(), reference);
    }
    // R38: a shared operation prefix applied OUTSIDE an inner Session
    // layer is the canonical floor (e.g. pack wrapping a snapshot/index
    // capture error). The shared owner drops the recorded Session base,
    // so the binding strip keeps the full CLI message instead of
    // restoring the pre-operation base. Read paths apply no shared
    // prefix, so their Session presentation still strips (pinned above).
    #[test]
    fn shared_prefix_outside_session_presentation_survives_strip() {
        let base = Error::new("invalid_header", "BLENDER magic");
        let captured = session_context(base, "snapshot/index", Path::new("garbage.blend"));
        assert_eq!(
            captured.message,
            "snapshot/index 'garbage.blend': BLENDER magic"
        );
        let packed = context(captured, "pack", Path::new("out"));
        assert_eq!(
            packed.message,
            "pack 'out': snapshot/index 'garbage.blend': BLENDER magic"
        );
        assert_eq!(packed.kind, "invalid_header");
        let preserved = packed.clone().without_context();
        assert_eq!(preserved.message, packed.message);
        assert_eq!(preserved.kind, "invalid_header");
    }
    fn workspace(label: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("oxblend-output-{}-{label}", std::process::id()));
        std::fs::create_dir(&root).expect("new owned test directory");
        root
    }
    #[test]
    fn named_stage_and_raced_destination_is_not_replaced() {
        let root = workspace("race");
        let input = root.join("input");
        std::fs::write(&input, b"original").unwrap();
        let output = root.join("output");
        let mut staging = AtomicOutput::create(&input.metadata().unwrap(), &output).unwrap();
        staging.file.write_all(b"complete-stage").unwrap();
        assert!(!output.exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
        let temp = staging.temp.as_ref().unwrap();
        assert!(temp.to_bytes().starts_with(TEMP_PREFIX.as_bytes()));
        assert_eq!(
            staging.file.metadata().unwrap().mode() & 0o777,
            input.metadata().unwrap().mode() & 0o777
        );
        std::fs::write(&output, b"other-winner").unwrap();
        assert_eq!(
            staging.publish(Durability::None).unwrap_err().kind,
            "output_exists"
        );
        assert!(!staging.published);
        drop(staging);
        assert_eq!(std::fs::read(&output).unwrap(), b"other-winner");
        assert_eq!(std::fs::read(&input).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    }
    #[test]
    fn failed_stage_drops_without_final_or_temp_name() {
        let root = workspace("drop");
        let input = root.join("input");
        std::fs::write(&input, b"original").unwrap();
        let output = root.join("output");
        {
            let mut staging = AtomicOutput::create(&input.metadata().unwrap(), &output).unwrap();
            staging.file.write_all(b"partial-not-published").unwrap();
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
        }
        assert!(!output.exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        assert_eq!(std::fs::read(&input).unwrap(), b"original");
    }
    #[test]
    fn reserved_final_prefix_is_refused_without_creating_a_stage() {
        let root = workspace("prefix");
        let input = root.join("input");
        std::fs::write(&input, b"original").unwrap();
        let output = root.join(".oxblend-stage-real");
        assert!(
            matches!(AtomicOutput::create(&input.metadata().unwrap(), &output), Err(e) if e.kind == "output_path")
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    }
    #[test]
    fn existing_stage_collision_is_never_replaced_or_removed() {
        let root = workspace("collision");
        let input = root.join("input");
        std::fs::write(&input, b"original").unwrap();
        let foreign = root.join(format!(
            "{TEMP_PREFIX}{}-{}",
            std::process::id(),
            NEXT_TEMP.load(Ordering::Relaxed)
        ));
        std::fs::write(&foreign, b"foreign-owned-name").unwrap();
        let mut staging =
            AtomicOutput::create(&input.metadata().unwrap(), &root.join("output")).unwrap();
        staging.file.write_all(b"complete").unwrap();
        staging.publish(Durability::None).unwrap();
        drop(staging);
        assert_eq!(std::fs::read(&foreign).unwrap(), b"foreign-owned-name");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 3);
    }
    #[test]
    fn complete_publication_removes_temp_and_keeps_source() {
        let root = workspace("success");
        let input = root.join("input");
        std::fs::write(&input, b"original").unwrap();
        let output = root.join("output");
        let mut staging = AtomicOutput::create(&input.metadata().unwrap(), &output).unwrap();
        staging.file.write_all(b"complete").unwrap();
        staging.publish(Durability::None).unwrap();
        assert!(staging.published && staging.temp.is_none());
        drop(staging);
        assert_eq!(std::fs::read(&output).unwrap(), b"complete");
        assert_eq!(std::fs::read(&input).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    }
    // R35 permanent pin: trailing separator is a canonical output_path
    // refusal with the shared create-output-stage prefix, preserved by the
    // binding exactly as the CLI emits it. No stage is created.
    #[test]
    fn trailing_separator_is_canonical_output_path_with_stage_prefix() {
        let root = workspace("trailing");
        let input = root.join("input");
        std::fs::write(&input, b"original").unwrap();
        let output = root.join("out");
        let trailing = std::path::PathBuf::from(format!("{}/", output.display()));
        let error = match AtomicOutput::create(&input.metadata().unwrap(), &trailing) {
            Err(error) => error,
            Ok(_) => panic!("trailing separator must refuse"),
        };
        assert_eq!(error.kind, "output_path");
        assert_eq!(
            error.message,
            format!(
                "create output stage '{}': file output has a trailing separator",
                trailing.display()
            )
        );
        // Shared canonical: no strippable Session base recorded.
        assert!(error.context_message.is_none());
        let preserved = error.clone().without_context();
        assert_eq!(preserved.message, error.message);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    }
    // Other AtomicOutput canonical variants stay pinned with the same prefix.
    #[test]
    fn atomic_output_canonical_variants_keep_stage_prefix() {
        let root = workspace("variants");
        let input = root.join("input");
        std::fs::write(&input, b"original").unwrap();
        let meta = input.metadata().unwrap();
        // Empty output has no file name: shared stage prefix, never bare base.
        let empty = std::path::Path::new("");
        let error = match AtomicOutput::create(&meta, empty) {
            Err(error) => error,
            Ok(_) => panic!("empty output must refuse"),
        };
        assert_eq!(error.kind, "output_path");
        assert!(error.message.starts_with("create output stage '': "));
        assert!(error.context_message.is_none());
        let preserved = error.clone().without_context();
        assert_eq!(preserved.message, error.message);
        // Parent is not a directory: regular file as parent. The preflight
        // symlink check meets ENOTDIR first (io), still with the shared
        // stage prefix, never a bare base message.
        let not_dir = root.join("notdir-parent");
        std::fs::write(&not_dir, b"file").unwrap();
        let bad_child = not_dir.join("child");
        // open(parent) succeeds (file), is_dir check refuses with prefix.
        if let Err(error) = AtomicOutput::create(&meta, &bad_child) {
            assert_eq!(error.kind, "io");
            assert!(
                error
                    .message
                    .starts_with(&format!("create output stage '{}': ", bad_child.display()))
            );
            assert!(error.context_message.is_none());
        }
        assert_eq!(std::fs::read(&input).unwrap(), b"original");
    }
}
