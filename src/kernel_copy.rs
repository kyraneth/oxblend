//! Physical copy transport, not publication. The caller owns a fresh AtomicOutput.
//! Root-approved: only unavailable-before-progress may fall back. No restart.
use crate::edit_output::io;
use crate::{Error, Result};
use std::fs::{File, Metadata};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::path::Path;
#[derive(Debug, PartialEq)]
struct Outcome {
    bytes: u64,
    mechanism: &'static str,
    fallback_errno: Option<i32>,
    fallback_reason: Option<&'static str>,
}
impl Outcome {
    fn report(&self, path: &Path) {
        crate::diagnostics::point(
            "copy.stats",
            serde_json::json!({"path":path.to_string_lossy(),"bytes":self.bytes,"mechanism":self.mechanism,"fallback_errno":self.fallback_errno,"fallback_reason":self.fallback_reason}),
        );
    }
}
pub(crate) fn native(
    source: &File,
    offset: &mut libc::loff_t,
    target: &File,
    count: usize,
) -> std::io::Result<usize> {
    // Linux API: explicit source offset, current target offset, flags must be zero.
    let n = unsafe {
        libc::copy_file_range(
            source.as_raw_fd(),
            offset,
            target.as_raw_fd(),
            std::ptr::null_mut(),
            count,
            0,
        )
    };
    if n < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(n as usize)
    }
}
fn request_max() -> Result<usize> {
    // External Linux MAX_RW_COUNT: signed INT_MAX rounded down to a page.
    // Linux include/linux/fs.h; this is a syscall constraint, not a workload cap.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page <= 0 {
        return Err(io(std::io::Error::last_os_error()));
    }
    let page = page as usize;
    Ok((i32::MAX as usize / page) * page)
}
fn transfer(
    source: &File,
    target: &mut File,
    length: u64,
    raw: Option<&[u8]>,
    mut call: impl FnMut(&File, &mut libc::loff_t, &File, usize) -> std::io::Result<usize>,
) -> Result<Outcome> {
    let mut offset = 0;
    let mut copied = 0u64;
    let maximum = request_max()?;
    let (errno, reason) = loop {
        if copied == length {
            return Ok(Outcome {
                bytes: copied,
                mechanism: "copy_file_range",
                fallback_errno: None,
                fallback_reason: None,
            });
        }
        let count = usize::try_from((length - copied).min(maximum as u64))
            .map_err(|_| Error::new("overflow", "physical copy request"))?;
        match call(source, &mut offset, target, count) {
            Ok(0) if copied == 0 => break (None, Some("zero_return")),
            // Linux permits short positive progress (chunking, signals, MAX_RW_COUNT).
            Ok(n) if n > 0 && n <= count => copied += n as u64,
            Ok(_) => {
                return Err(Error::new(
                    "truncated",
                    "copy_file_range incomplete physical copy",
                ));
            }
            Err(e)
                if copied == 0
                    && matches!(
                        e.raw_os_error(),
                        Some(libc::EXDEV | libc::ENOSYS | libc::EOPNOTSUPP)
                    ) =>
            {
                break (e.raw_os_error(), Some("errno"));
            }
            // EINTR also refuses: this crate installs no signal handlers;
            // root permits only the three unavailable-before-progress errnos.
            Err(e) => return Err(io(e)),
        }
    };
    // No kernel byte reached the fresh stage. Use a genuinely user-space stream
    // (dyn Read prevents std::io::copy's implicit second kernel-copy attempt).
    let copied = if let Some(bytes) = raw {
        // Immutable raw capture is already physical data, source-signature bound.
        std::io::copy(&mut std::io::Cursor::new(bytes), target).map_err(io)?
    } else {
        let mut input = source.try_clone().map_err(io)?;
        input.seek(SeekFrom::Start(0)).map_err(io)?;
        let reader: &mut dyn Read = &mut input;
        let mut buffered = BufReader::with_capacity(crate::fetch::READ_CHUNK, reader);
        std::io::copy(&mut buffered, target).map_err(io)?
    };
    if copied != length {
        return Err(Error::new(
            "truncated",
            "physical source changed during copy",
        ));
    }
    Ok(Outcome {
        bytes: copied,
        mechanism: "streamed",
        fallback_errno: errno,
        fallback_reason: reason,
    })
}
pub(crate) fn full(
    source: &File,
    path: &Path,
    metadata: &Metadata,
    target: &mut File,
    raw: Option<&[u8]>,
) -> Result<u64> {
    guarded(source, path, metadata, target, raw, native)
}
pub(crate) fn guarded(
    source: &File,
    path: &Path,
    metadata: &Metadata,
    target: &mut File,
    raw: Option<&[u8]>,
    call: impl FnMut(&File, &mut libc::loff_t, &File, usize) -> std::io::Result<usize>,
) -> Result<u64> {
    check_source(source, path, metadata)?;
    let copied = transfer(source, target, metadata.len(), raw, call);
    if let Ok(outcome) = &copied {
        outcome.report(path);
    }
    // Changed-source dominates a partial transport result, as the existing guard
    // does. Never publish an output whose source changed during any mechanism.
    check_source(source, path, metadata)?;
    copied.map(|outcome| outcome.bytes)
}
pub(crate) fn check_source(source: &File, path: &Path, metadata: &Metadata) -> Result<()> {
    let expected = crate::fetch::capture_signature(metadata);
    if crate::fetch::capture_signature(&source.metadata().map_err(io)?) != expected
        || crate::fetch::capture_signature(&path.metadata().map_err(io)?) != expected
    {
        return Err(Error::new(
            "input_changed",
            "source signature changed before publication",
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::FileExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Work(std::path::PathBuf);
    impl Work {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "oxblend-kernel-copy-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
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
    fn files(work: &Work) -> (File, File, Vec<u8>) {
        let body = include_bytes!("../tests/fixtures/constraint-data/tar.blend").to_vec();
        std::fs::write(work.0.join("source"), &body).unwrap();
        let source = File::open(work.0.join("source")).unwrap();
        let target = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(work.0.join("target"))
            .unwrap();
        (source, target, body)
    }
    #[test]
    fn actual_kernel_copy_and_each_unavailable_first_call_preserve_bytes() {
        for errno in [
            None,
            Some(libc::EXDEV),
            Some(libc::ENOSYS),
            Some(libc::EOPNOTSUPP),
            Some(0),
        ] {
            let work = Work::new();
            let (source, mut target, body) = files(&work);
            let mut calls = 0;
            let outcome = transfer(
                &source,
                &mut target,
                body.len() as u64,
                None,
                |s, o, t, n| {
                    calls += 1;
                    match errno {
                        None => native(s, o, t, n),
                        Some(0) => Ok(0),
                        Some(e) => Err(std::io::Error::from_raw_os_error(e)),
                    }
                },
            )
            .unwrap();
            assert_eq!(calls, 1);
            assert_eq!(outcome.bytes, body.len() as u64);
            assert_eq!(std::fs::read(work.0.join("target")).unwrap(), body);
            assert_eq!(
                outcome.mechanism,
                if errno.is_none() {
                    "copy_file_range"
                } else {
                    "streamed"
                }
            );
            assert_eq!(outcome.fallback_errno, errno.filter(|e| *e != 0));
            assert_eq!(
                outcome.fallback_reason,
                match errno {
                    None => None,
                    Some(0) => Some("zero_return"),
                    Some(_) => Some("errno"),
                }
            );
        }
    }
    #[test]
    fn nonfallback_errno_and_zero_after_progress_refuse_without_stream_restart() {
        for errno in [Some(libc::EINVAL), None] {
            let work = Work::new();
            let (source, mut target, body) = files(&work);
            let mut calls = 0;
            let error = transfer(
                &source,
                &mut target,
                body.len() as u64,
                None,
                |s, o, t, _| {
                    calls += 1;
                    if let Some(e) = errno {
                        Err(std::io::Error::from_raw_os_error(e))
                    } else if calls == 1 {
                        native(s, o, t, 1)
                    } else {
                        Ok(0)
                    }
                },
            )
            .unwrap_err();
            assert_eq!(calls, if errno.is_some() { 1 } else { 2 });
            assert_eq!(error.kind, if errno.is_some() { "io" } else { "truncated" });
            assert_eq!(
                target.metadata().unwrap().len(),
                if errno.is_some() { 0 } else { 1 }
            );
        }
    }
    #[test]
    fn short_positive_kernel_progress_loops_and_preserves_bytes() {
        let work = Work::new();
        let (source, mut target, body) = files(&work);
        let mut calls = 0;
        let outcome = transfer(
            &source,
            &mut target,
            body.len() as u64,
            None,
            |s, o, t, n| {
                calls += 1;
                native(s, o, t, if calls == 1 { 1 } else { n })
            },
        )
        .unwrap();
        assert!(calls >= 2);
        assert_eq!(outcome.mechanism, "copy_file_range");
        assert_eq!(outcome.fallback_errno, None);
        assert_eq!(outcome.bytes, body.len() as u64);
        assert_eq!(std::fs::read(work.0.join("target")).unwrap(), body);
    }
    #[test]
    fn errno_after_progress_never_restarts_stream() {
        for errno in [libc::EXDEV, libc::ENOSYS, libc::EOPNOTSUPP, libc::EINVAL] {
            let work = Work::new();
            let (source, mut target, body) = files(&work);
            let mut calls = 0;
            let error = transfer(
                &source,
                &mut target,
                body.len() as u64,
                None,
                |s, o, t, _| {
                    calls += 1;
                    if calls == 1 {
                        native(s, o, t, 1)
                    } else {
                        Err(std::io::Error::from_raw_os_error(errno))
                    }
                },
            )
            .unwrap_err();
            assert_eq!(calls, 2);
            assert_eq!(error.kind, "io");
            assert_eq!(target.metadata().unwrap().len(), 1);
        }
    }
    #[test]
    fn unavailable_raw_copy_uses_capture_without_source_read() {
        let work = Work::new();
        let (_, mut target, body) = files(&work);
        // A write-only source cannot satisfy a userspace reread. Metadata and
        // immutable capture are valid; forced EXDEV must still preserve bytes.
        let source = std::fs::OpenOptions::new()
            .write(true)
            .open(work.0.join("source"))
            .unwrap();
        let outcome = transfer(
            &source,
            &mut target,
            body.len() as u64,
            Some(&body),
            |_, _, _, _| Err(std::io::Error::from_raw_os_error(libc::EXDEV)),
        )
        .unwrap();
        assert_eq!(outcome.mechanism, "streamed");
        assert_eq!(std::fs::read(work.0.join("target")).unwrap(), body);
    }
    #[test]
    fn fallback_short_capture_refuses_length_mismatch() {
        let work = Work::new();
        let (source, mut target, body) = files(&work);
        let error = transfer(
            &source,
            &mut target,
            body.len() as u64,
            Some(&body[..1]),
            |_, _, _, _| Err(std::io::Error::from_raw_os_error(libc::EXDEV)),
        )
        .unwrap_err();
        assert_eq!(error.kind, "truncated");
        assert_eq!(error.message, "physical source changed during copy");
    }
    #[test]
    fn real_mutation_during_kernel_copy_refuses_and_fault_must_apply() {
        let work = Work::new();
        let (source, mut target, _) = files(&work);
        let path = work.0.join("source");
        let metadata = source.metadata().unwrap();
        let mut fault_applied = 0;
        let error = guarded(
            &source,
            &path,
            &metadata,
            &mut target,
            None,
            |s, o, t, n| {
                let copied = native(s, o, t, n)?;
                let mut writer = std::fs::OpenOptions::new().write(true).open(&path)?;
                writer.write_all_at(b"X", 0)?;
                writer.flush()?;
                writer.set_modified(std::time::UNIX_EPOCH)?;
                fault_applied += 1;
                Ok(copied)
            },
        )
        .unwrap_err();
        assert_eq!(fault_applied, 1);
        assert_eq!(error.kind, "input_changed");
        assert_eq!(error.message, "source signature changed before publication");
    }
}
