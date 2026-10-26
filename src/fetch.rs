use crate::{Error, Result};
use std::cell::Cell;
use std::collections::VecDeque;
use std::fs::{File, Metadata};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};
#[derive(Debug)]
pub(crate) struct Raw {
    pub metadata: Metadata,
    pub bytes: Vec<u8>,
    pub proof: Option<crate::edit_container::capture::Proof>,
    /// Opaque assets retain only an identity digest, never their physical body.
    pub asset_hash: Option<u64>,
    /// Blend captures retain the physical+decoded content digest computed by
    /// the capture reads themselves; the decoded body is not retained here.
    /// Fast captures leave this as None; bounded captures fill it.
    pub digest: Option<crate::body_cache::SourceDigest>,
}
/// Whole-file snapshot buffer size; measured implementation choice, see
/// `read_into`.
pub(crate) const READ_CHUNK: usize = 1 << 20;
struct Job {
    path: PathBuf,
    blend: bool,
    reply: mpsc::Sender<Result<Raw>>,
    links: serde_json::Value,
}
#[derive(Default)]
struct Queue {
    critical: VecDeque<Job>,
    background: VecDeque<Job>,
    closed: bool,
}
#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    memory: crate::session::MemoryMode,
}
pub(crate) struct Fetcher {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}
pub(crate) fn signature(m: &Metadata) -> (u64, u64, u64, i64, i64) {
    (m.dev(), m.ino(), m.len(), m.mtime(), m.mtime_nsec())
}
/// Root-approved immutable capture proof also binds ctime, including nanoseconds.
pub(crate) type CaptureSignature = (u64, u64, u64, i64, i64, i64, i64);
pub(crate) fn capture_signature(m: &Metadata) -> CaptureSignature {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn io(e: std::io::Error) -> Error {
    Error::new(
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            "truncated"
        } else {
            "io"
        },
        e.to_string(),
    )
}
fn occupied_fds(limit: libc::rlim_t) -> Result<(usize, &'static str)> {
    if let Ok(entries) = std::fs::read_dir("/proc/self/fd") {
        // The enumeration itself owns one descriptor. Do not silently undercount
        // when reading a directory entry fails: fall back to descriptor probes.
        if let Ok(entries) = entries.collect::<std::io::Result<Vec<_>>>() {
            return Ok((entries.len().saturating_sub(1), "proc"));
        }
    }
    let mut occupied = 0;
    // fcntl takes a signed int descriptor; RLIMIT_NOFILE is the external bound
    // on allocatable descriptor numbers. High inherited FDs above a lowered
    // limit cannot consume allocatable slots below it.
    for fd in 0..limit.min(i32::MAX as libc::rlim_t) {
        if unsafe { libc::fcntl(fd as i32, libc::F_GETFD) } >= 0 {
            occupied += 1;
        } else {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() != Some(libc::EBADF) {
                return Err(Error::new(
                    "io",
                    format!("fcntl(F_GETFD) descriptor {fd} while counting pack slots: {e}"),
                ));
            }
        }
    }
    Ok((occupied, "fcntl"))
}
impl Fetcher {
    /// Historical default: fast retention, no inline content hashing.
    pub fn new(roots: usize) -> Result<Self> {
        Self::with_memory(roots, crate::session::MemoryMode::Fast)
    }
    pub fn with_memory(roots: usize, memory: crate::session::MemoryMode) -> Result<Self> {
        let mut limit = std::mem::MaybeUninit::<libc::rlimit>::uninit();
        if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, limit.as_mut_ptr()) } != 0 {
            return Err(io(std::io::Error::last_os_error()));
        }
        let limit = unsafe { limit.assume_init() }.rlim_cur;

        // /proc is an optimization for counting occupied descriptor slots, not
        // a packaging prerequisite. F_GETFD works in containers without procfs.
        let (occupied, fd_source) = occupied_fds(limit)?;

        // Actual simultaneously live publication descriptors: source (1),
        // AtomicOutput file + parent (2), and directory/sync helper (1).
        let publication_fds = 1 + 2 + 1;
        let worker_fds = 1; // one open source per fetch lane
        let slots = usize::try_from(limit)
            .unwrap_or(usize::MAX)
            .saturating_sub(occupied + publication_fds)
            / worker_fds;
        if slots == 0 {
            return Err(Error::new(
                "io",
                "RLIMIT_NOFILE leaves no private fetch lane after actual publication headroom",
            ));
        }
        let lanes = roots.min(slots);
        crate::diagnostics::point(
            "fetch.budget",
            serde_json::json!({"limit":limit,"occupied":occupied,"publication_fds":publication_fds,"worker_fds":worker_fds,"lanes":lanes,"source":fd_source}),
        );
        // Mode propagates to workers through the shared owner: every lane
        // reads the same retention policy without per-job plumbing.
        let shared = Arc::new(Shared {
            memory,
            ..Shared::default()
        });
        let threads = (0..lanes)
            .map(|_| {
                let shared = shared.clone();
                thread::spawn(move || {
                    loop {
                        let job = {
                            let mut queue = shared.queue.lock().unwrap();
                            loop {
                                if queue.closed {
                                    return;
                                }
                                if let Some(job) = queue
                                    .critical
                                    .pop_front()
                                    .or_else(|| queue.background.pop_front())
                                {
                                    break job;
                                }
                                queue = shared.wake.wait(queue).unwrap();
                            }
                        };
                        // Essential pack work stays visible at L1: worker fetch
                        // is the actual capture owner, not optional detail.
                        let mut span = crate::diagnostics::Span::new("fetch",
                            if crate::diagnostics::enabled() {
                                serde_json::json!({"path":job.path.to_string_lossy(),"blend":job.blend,"links":job.links})
                            } else {
                                serde_json::Value::Null
                            });
                        let result = fetch(&job.path, job.blend, &shared);
                        if result.is_err() && shared.queue.lock().unwrap().closed {
                            span.end("cancelled");
                        } else {
                            span.record_result(&result);
                        }
                        let _ = job.reply.send(result);
                    }
                })
            })
            .collect();
        Ok(Self { shared, threads })
    }
    pub fn promote(&self, path: &Path) {
        let mut queue = self.shared.queue.lock().unwrap();
        if let Some(index) = queue.background.iter().position(|job| job.path == path) {
            let mut job = queue.background.remove(index).unwrap();
            job.blend = true;
            queue.critical.push_front(job);
            self.shared.wake.notify_one();
        }
    }
    pub fn submit(&self, path: &Path, blend: bool) -> mpsc::Receiver<Result<Raw>> {
        let (reply, result) = mpsc::channel();
        let job = Job {
            path: path.to_path_buf(),
            blend,
            reply,
            links: crate::diagnostics::worker_links(),
        };
        let mut queue = self.shared.queue.lock().unwrap();
        if blend {
            queue.critical.push_back(job);
        } else {
            queue.background.push_back(job);
        }
        self.shared.wake.notify_one();
        result
    }
}
impl Drop for Fetcher {
    fn drop(&mut self) {
        {
            let mut queue = self.shared.queue.lock().unwrap();
            queue.closed = true;
            queue.critical.clear();
            queue.background.clear();
        }
        self.shared.wake.notify_all();
        for handle in self.threads.drain(..) {
            let _ = handle.join();
        }
    }
}
/// Baseline fast snapshot read: plain bytes, cancellation polled by the
/// caller. No content hashing on this path.
fn read_into(
    file: &mut impl Read,
    bytes: &mut Vec<u8>,
    cancelled: impl Fn() -> bool,
) -> Result<()> {
    let mut chunk = vec![0u8; READ_CHUNK];
    loop {
        if cancelled() {
            return Err(Error::new("io", "owning pack stopped private fetch"));
        }
        let n = match file.read(&mut chunk) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            r => r.map_err(io)?,
        };
        if n == 0 {
            return Ok(());
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
}
/// Bounded snapshot read: streams through the hashed decode output, so the
/// decoded-content digest is recorded at the single decode itself.
/// Cancellation is owned by the guarded reader/output wrappers; a stopped
/// batch fails at the first cancelled read or write here.
fn read_into_hashed(file: &mut impl Read, output: &mut impl Write) -> Result<()> {
    let mut chunk = vec![0u8; READ_CHUNK];
    loop {
        let n = match file.read(&mut chunk) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            r => r.map_err(io)?,
        };
        if n == 0 {
            return Ok(());
        }
        output.write_all(&chunk[..n]).map_err(io)?;
    }
}

/// Stream opaque identity/copy through the existing measured read buffer.
/// The same operation hashes exactly the bytes sent to the publication stage.
pub(crate) fn opaque_copy(
    input: &mut impl Read,
    output: &mut impl Write,
    cancelled: impl Fn() -> bool,
) -> Result<(u64, u64)> {
    // Actual hash phase: identity digest over exactly the copied bytes.
    let mut span = crate::diagnostics::Span::new(
        "hash",
        if crate::diagnostics::enabled() {
            serde_json::json!({"algorithm":"xxh64_identity","role":"opaque_copy"})
        } else {
            serde_json::Value::Null
        },
    );
    let result = (|| {
        let mut hash = crate::edit_xxh::Xxh64::new();
        let mut chunk = vec![0; READ_CHUNK];
        let mut total = 0u64;
        loop {
            if cancelled() {
                return Err(Error::new("io", "owning pack stopped private fetch"));
            }
            let n = match input.read(&mut chunk) {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                result => result.map_err(io)?,
            };
            if n == 0 {
                if span.active() {
                    span.data(serde_json::json!({"bytes":total}));
                }
                return Ok((total, hash.digest()));
            }
            output.write_all(&chunk[..n]).map_err(io)?;
            hash.update(&chunk[..n]);
            total = total
                .checked_add(n as u64)
                .ok_or_else(|| Error::new("overflow", "opaque input length"))?;
        }
    })();
    span.result(result)
}

/// Baseline fast physical-side capture read: cancellation and failure
/// latching only, no hashing.
struct CaptureRead<'a> {
    file: &'a mut File,
    shared: &'a Shared,
    failed: &'a Cell<bool>,
}
impl Read for CaptureRead<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.shared.queue.lock().unwrap().closed {
                self.failed.set(true);
                return Err(std::io::Error::other("owning pack stopped private fetch"));
            }
            match self.file.read(bytes) {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                result => {
                    if result.is_err() {
                        self.failed.set(true);
                    }
                    return result;
                }
            }
        }
    }
}
/// Baseline fast decoded-side capture output: cancellation and failure
/// latching only, no hashing.
struct CaptureOutput<'a> {
    bytes: Vec<u8>,
    shared: &'a Shared,
    failed: &'a Cell<bool>,
}
impl Write for CaptureOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.shared.queue.lock().unwrap().closed {
            self.failed.set(true);
            return Err(std::io::Error::other("owning pack stopped private fetch"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Bounded physical-side capture read: cancellation, failure latching and
/// the inline physical-content xxh64 (each live byte is hashed exactly
/// once, at its single read; no second pass).
struct HashedRead<'a, C: Fn() -> bool> {
    file: &'a mut File,
    cancelled: C,
    failed: &'a Cell<bool>,
    physical: &'a Cell<crate::edit_xxh::Xxh64>,
}
impl<C: Fn() -> bool> Read for HashedRead<'_, C> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if (self.cancelled)() {
                self.failed.set(true);
                return Err(std::io::Error::other("owning pack stopped private fetch"));
            }
            match self.file.read(bytes) {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                result => {
                    if result.is_err() {
                        self.failed.set(true);
                    } else if let Ok(n) = result
                        && n > 0
                    {
                        let mut hash = self.physical.get();
                        hash.update(&bytes[..n]);
                        self.physical.set(hash);
                    }
                    return result;
                }
            }
        }
    }
}
/// Bounded decoded-side capture output: cancellation, failure latching and
/// the inline decoded-content xxh64 (each decoded byte is hashed exactly
/// once as the decoder emits it).
struct HashedOutput<'a, C: Fn() -> bool> {
    bytes: Vec<u8>,
    cancelled: C,
    failed: &'a Cell<bool>,
    decoded: &'a Cell<crate::edit_xxh::Xxh64>,
}
impl<C: Fn() -> bool> Write for HashedOutput<'_, C> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if (self.cancelled)() {
            self.failed.set(true);
            return Err(std::io::Error::other("owning pack stopped private fetch"));
        }
        let mut hash = self.decoded.get();
        hash.update(bytes);
        self.decoded.set(hash);
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Baseline fast blend capture: exact historical decode with container
/// proof, no content hashing. Raw is a plain read; only compressed families
/// decode.
fn blend_capture(
    file: &mut File,
    metadata: &Metadata,
    shared: &Shared,
) -> Result<(Vec<u8>, crate::edit_container::capture::Proof)> {
    let failed = Cell::new(false);
    let mut input = BufReader::with_capacity(
        READ_CHUNK,
        CaptureRead {
            file,
            shared,
            failed: &failed,
        },
    );
    // Four magic bytes are a format field, not a read-size assumption. Accumulate
    // short reads and replay only this field from the existing buffer; no reread.
    let mut prefix = Vec::new();
    while prefix.len() < 4 {
        let available = input.fill_buf().map_err(io)?;
        if available.is_empty() {
            break;
        }
        let n = (4 - prefix.len()).min(available.len());
        prefix.extend_from_slice(&available[..n]);
        input.consume(n);
    }
    let family = if prefix.starts_with(&[0x1f, 0x8b]) {
        "gzip"
    } else if prefix
        .get(..4)
        .is_some_and(|b| le_magic(b) == 0xfd2fb528 || le_magic(b) & 0xfffffff0 == 0x184d2a50)
    {
        "zstd"
    } else {
        "raw"
    };
    let mut input = std::io::Cursor::new(prefix).chain(input);
    let mut output = CaptureOutput {
        bytes: if family == "raw" {
            Vec::with_capacity(
                usize::try_from(metadata.len())
                    .map_err(|_| Error::new("overflow", "physical input length"))?,
            )
        } else {
            Vec::new()
        },
        shared,
        failed: &failed,
    };
    let mut table = Ok(None);
    // True decode phase: only compressed families decode; raw is a plain read.
    let mut decode_span = if family != "raw" {
        Some(crate::diagnostics::Span::new(
            "decode",
            if crate::diagnostics::enabled() {
                serde_json::json!({"format":family,"compressed":true})
            } else {
                serde_json::Value::Null
            },
        ))
    } else {
        None
    };
    let result = match family {
        "raw" => read_into(&mut input, &mut output.bytes, || {
            shared.queue.lock().unwrap().closed
        }),
        "gzip" => {
            crate::stream_decode::decode(&mut flate2::read::MultiGzDecoder::new(input), &mut output)
        }
        _ => {
            let operation = crate::edit_container::capture::Recording::new(
                usize::try_from(metadata.len())
                    .map_err(|_| Error::new("overflow", "physical input length"))?,
            )
            .map_err(|e| Error::new("compression", e.to_string()))?;
            let mut decoder = zstd::stream::zio::Reader::new(input, operation);
            let result = crate::stream_decode::decode(&mut decoder, &mut output);
            if result.is_ok() {
                table = decoder.operation_mut().take_proof(output.bytes.len());
            }
            result
        }
    };
    let decode_result = result.map_err(|e| {
        if failed.get() {
            Error::new("io", e.message)
        } else {
            e
        }
    });
    if let Some(mut span) = decode_span.take() {
        if decode_result.is_ok() && span.active() {
            span.data(serde_json::json!({"decoded_bytes":output.bytes.len()}));
        }
        span.result(decode_result.map(|_| ()))?;
    } else {
        decode_result?;
    }
    let proof = crate::edit_container::capture::Proof::new(metadata, family, table);
    Ok((output.bytes, proof))
}

/// Shared checked blend capture: the fetch worker's decode and the
/// verified-on-reuse re-materialization run the exact same reads, container
/// proof construction and inline physical+decoded digest computation.
pub(crate) fn capture_body(
    file: &mut File,
    metadata: &Metadata,
    cancelled: impl Fn() -> bool + Copy,
) -> Result<(
    Vec<u8>,
    crate::edit_container::capture::Proof,
    crate::body_cache::SourceDigest,
)> {
    let failed = Cell::new(false);
    let physical = Cell::new(crate::edit_xxh::Xxh64::new());
    let decoded = Cell::new(crate::edit_xxh::Xxh64::new());
    let mut input = BufReader::with_capacity(
        READ_CHUNK,
        HashedRead {
            file,
            cancelled,
            failed: &failed,
            physical: &physical,
        },
    );
    // Four magic bytes are a format field, not a read-size assumption. Accumulate
    // short reads and replay only this field from the existing buffer; no reread.
    let mut prefix = Vec::new();
    while prefix.len() < 4 {
        let available = input.fill_buf().map_err(io)?;
        if available.is_empty() {
            break;
        }
        let n = (4 - prefix.len()).min(available.len());
        prefix.extend_from_slice(&available[..n]);
        input.consume(n);
    }
    let family = if prefix.starts_with(&[0x1f, 0x8b]) {
        "gzip"
    } else if prefix
        .get(..4)
        .is_some_and(|b| le_magic(b) == 0xfd2fb528 || le_magic(b) & 0xfffffff0 == 0x184d2a50)
    {
        "zstd"
    } else {
        "raw"
    };
    let mut input = std::io::Cursor::new(prefix).chain(input);
    let mut output = HashedOutput {
        bytes: if family == "raw" {
            Vec::with_capacity(
                usize::try_from(metadata.len())
                    .map_err(|_| Error::new("overflow", "physical input length"))?,
            )
        } else {
            Vec::new()
        },
        cancelled,
        failed: &failed,
        decoded: &decoded,
    };
    let mut table = Ok(None);
    // True decode phase: only compressed families decode; raw is a plain read.
    let mut decode_span = if family != "raw" {
        Some(crate::diagnostics::Span::new(
            "decode",
            if crate::diagnostics::enabled() {
                serde_json::json!({"format":family,"compressed":true})
            } else {
                serde_json::Value::Null
            },
        ))
    } else {
        None
    };
    let result = match family {
        // Raw streams through the same hashed output wrapper, so every
        // family records its decoded-content digest at the decode itself.
        "raw" => read_into_hashed(&mut input, &mut output),
        "gzip" => {
            crate::stream_decode::decode(&mut flate2::read::MultiGzDecoder::new(input), &mut output)
        }
        _ => {
            let operation = crate::edit_container::capture::Recording::new(
                usize::try_from(metadata.len())
                    .map_err(|_| Error::new("overflow", "physical input length"))?,
            )
            .map_err(|e| Error::new("compression", e.to_string()))?;
            let mut decoder = zstd::stream::zio::Reader::new(input, operation);
            let result = crate::stream_decode::decode(&mut decoder, &mut output);
            if result.is_ok() {
                table = decoder.operation_mut().take_proof(output.bytes.len());
            }
            result
        }
    };
    let decode_result = result.map_err(|e| {
        if failed.get() {
            Error::new("io", e.message)
        } else {
            e
        }
    });
    if let Some(mut span) = decode_span.take() {
        if decode_result.is_ok() && span.active() {
            span.data(serde_json::json!({"decoded_bytes":output.bytes.len()}));
        }
        span.result(decode_result.map(|_| ()))?;
    } else {
        decode_result?;
    }
    let proof = crate::edit_container::capture::Proof::new(metadata, family, table);
    let digest = crate::body_cache::SourceDigest {
        physical: physical.get().digest(),
        decoded: decoded.get().digest(),
    };
    Ok((output.bytes, proof, digest))
}
fn le_magic(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("four-byte physical magic"))
}
/// Snapshot capture preserves its historical nonregular-source refusal,
/// including directory library paths during fast and bounded packs.
/// Read-root admission has a separate shared owner in read_reports: changing
/// this capture boundary would change existing CLI pack diagnostics.
/// Stat-before-open remains essential for FIFO/socket/device sources.
fn admit_capture_source(metadata: &Metadata) -> Result<()> {
    if !metadata.is_file() {
        return Err(Error::new("pack_path", "pack source is not regular file"));
    }
    Ok(())
}
fn check_capture_signature(path: &Path, file: &File, admitted: &Metadata) -> Result<()> {
    if capture_signature(&file.metadata().map_err(io)?) != capture_signature(admitted)
        || capture_signature(&path.metadata().map_err(io)?) != capture_signature(admitted)
    {
        return Err(Error::new(
            "input_changed",
            "source signature changed during snapshot",
        ));
    }
    Ok(())
}

fn fetch(path: &Path, blend: bool, shared: &Shared) -> Result<Raw> {
    let mut span = crate::diagnostics::Span::new(
        "file.open",
        if crate::diagnostics::enabled() {
            serde_json::json!({"path":path.to_string_lossy(),"input":crate::diagnostics::observe_input(path)})
        } else {
            serde_json::Value::Null
        },
    );
    let result = (|| {
    admit_capture_source(&path.metadata().map_err(io)?)?;
    let mut file = File::open(path).map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    admit_capture_source(&metadata)?;
    let captured = if blend {
        match shared.memory {
            crate::session::MemoryMode::Fast => blend_capture(&mut file, &metadata, shared)
                .map(|(bytes, proof)| (bytes, Some(proof), None, None)),
            crate::session::MemoryMode::Bounded => {
                let closed = || shared.queue.lock().unwrap().closed;
                capture_body(&mut file, &metadata, closed)
                    .map(|(bytes, proof, digest)| (bytes, Some(proof), None, Some(digest)))
            }
        }
    } else {
        let (length, hash) = opaque_copy(&mut file, &mut std::io::sink(), || {
            shared.queue.lock().unwrap().closed
        })?;
        if length != metadata.len() {
            return Err(Error::new("input_changed", "source signature changed during snapshot"));
        }
        Ok((Vec::new(), None, Some(hash), None))
    };
    check_capture_signature(path, &file, &metadata)?;
    let (bytes, proof, asset_hash, digest) = captured?;
    if blend && proof.as_ref().is_none_or(|p| p.family == "raw") && bytes.len() as u64 != metadata.len() {
        return Err(Error::new(
            "input_changed",
            "source signature changed during snapshot",
        ));
    }
    span.data(serde_json::json!({"size_bytes":metadata.len(),"decoded_bytes":bytes.len(),"blend":blend}));
    drop(file);
    Ok(Raw {
        metadata,
        bytes,
        proof,
        asset_hash,
        digest,
    })
    })().map_err(|e| crate::edit_output::session_context(e, "fetch/read snapshot", path));
    span.result(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Owned(PathBuf);
    impl Owned {
        fn new(bytes: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "oxblend-member8-fetch-{}-{}",
                std::process::id(),
                crate::fetch::tests::NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::write(&path, bytes).unwrap();
            Self(path)
        }
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    /// Pack capture must retain its pre-R38 refusal, independent of scalar
    /// read-root admission. Both blend and opaque lanes refuse before open.
    #[test]
    fn directory_capture_preserves_pack_path() {
        let dir = std::env::temp_dir().join(format!(
            "oxblend-member8-fetch-dir-{}-{}",
            std::process::id(),
            crate::fetch::tests::NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        struct Guard(PathBuf);
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir(&self.0);
            }
        }
        let _guard = Guard(dir.clone());
        let shared = Shared::default();
        for blend in [true, false] {
            let error = fetch(&dir, blend, &shared).unwrap_err();
            assert_eq!(error.kind, "pack_path");
            assert_eq!(
                error.message,
                format!(
                    "fetch/read snapshot '{}': pack source is not regular file",
                    dir.display()
                )
            );
        }
    }

    /// A reader that fails once with Interrupted before producing the payload
    /// in short reads smaller than any buffer (input-capture.md: interrupted
    /// read then successful bytes yields the same complete snapshot).
    struct InterruptedShort {
        data: Vec<u8>,
        position: usize,
        interrupt_pending: bool,
        max_read: usize,
    }
    impl Read for InterruptedShort {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.interrupt_pending {
                self.interrupt_pending = false;
                return Err(std::io::Error::new(ErrorKind::Interrupted, "simulated"));
            }
            let remaining = self.data.len() - self.position;
            if remaining == 0 {
                return Ok(0);
            }
            let take = buf.len().min(self.max_read).min(remaining);
            buf[..take].copy_from_slice(&self.data[self.position..self.position + take]);
            self.position += take;
            Ok(take)
        }
    }

    #[test]
    fn interrupted_then_short_reads_capture_the_whole_snapshot() {
        let data: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        let mut reader = InterruptedShort {
            data: data.clone(),
            position: 0,
            interrupt_pending: true,
            max_read: 3,
        };
        let mut bytes = Vec::new();
        read_into(&mut reader, &mut bytes, || false).unwrap();
        assert_eq!(bytes, data);
    }

    #[test]
    fn empty_input_is_an_empty_snapshot() {
        let file = Owned::new(b"");
        let shared = Shared::default();
        let raw = fetch(&file.0, false, &shared).unwrap();
        assert!(raw.bytes.is_empty());
        assert_eq!(raw.metadata.len(), 0);
    }

    #[test]
    fn multi_chunk_file_snapshot_matches_bytes_and_identity() {
        // Spans several read chunks plus a partial final chunk.
        let mut data = vec![0u8; 5 * READ_CHUNK + 123];
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }
        let file = Owned::new(&data);
        let shared = Shared::default();
        let raw = fetch(&file.0, true, &shared).unwrap();
        assert_eq!(raw.bytes, data);
        assert_eq!(
            signature(&raw.metadata),
            signature(&std::fs::metadata(&file.0).unwrap())
        );
    }

    #[test]
    fn fast_blend_capture_carries_no_digest() {
        let file = Owned::new(b"payload");
        let shared = Shared::default();
        assert_eq!(shared.memory, crate::session::MemoryMode::Fast);
        let raw = fetch(&file.0, true, &shared).unwrap();
        assert_eq!(raw.bytes, b"payload");
        assert!(raw.proof.is_some());
        assert!(raw.digest.is_none());
    }

    #[test]
    fn bounded_blend_capture_hashes_inline() {
        let file = Owned::new(b"payload");
        let shared = Shared {
            memory: crate::session::MemoryMode::Bounded,
            ..Shared::default()
        };
        let raw = fetch(&file.0, true, &shared).unwrap();
        assert_eq!(raw.bytes, b"payload");
        assert!(raw.proof.is_some());
        let digest = raw.digest.expect("bounded blend carries a content digest");
        let mut physical = crate::edit_xxh::Xxh64::new();
        physical.update(b"payload");
        let mut decoded = crate::edit_xxh::Xxh64::new();
        decoded.update(b"payload");
        assert_eq!(digest.physical, physical.digest());
        assert_eq!(digest.decoded, decoded.digest());
    }

    /// input-capture.md: cancellation during capture stops without a
    /// fabricated snapshot. The queue is observed before every chunk.
    #[test]
    fn cancelled_capture_stops_without_a_snapshot() {
        let file = Owned::new(&vec![7u8; 4 * READ_CHUNK]);
        let shared = Shared::default();
        shared.queue.lock().unwrap().closed = true;
        let error = fetch(&file.0, false, &shared).unwrap_err();
        assert_eq!(error.kind, "io");
        assert_eq!(
            error.message,
            format!(
                "fetch/read snapshot '{}': owning pack stopped private fetch",
                file.0.display()
            )
        );
    }

    /// A mutation after capture admission must fail the actual completion
    /// guard. Synchronize by performing the append between the two boundaries,
    /// not by hoping a writer thread runs during a large read under host load.
    #[test]
    fn source_change_after_capture_admission_fails_explicitly() {
        let source = Owned::new(b"captured source");
        let file = File::open(&source.0).unwrap();
        let admitted = file.metadata().unwrap();
        check_capture_signature(&source.0, &file, &admitted).unwrap();
        let mut appender = std::fs::OpenOptions::new()
            .append(true)
            .open(&source.0)
            .unwrap();
        std::io::Write::write_all(&mut appender, b"changed").unwrap();
        let error = check_capture_signature(&source.0, &file, &admitted).unwrap_err();
        assert_eq!(error.kind, "input_changed");
        assert_eq!(error.message, "source signature changed during snapshot");
    }

    /// The public queue contract: a submitted job is answered by a worker,
    /// and closing the Fetcher stops workers without deadlocking.
    #[test]
    fn fetcher_answers_and_stops() {
        let fetcher = Fetcher::new(1).unwrap();
        let file = Owned::new(b"payload");
        let result = fetcher.submit(&file.0, true).recv().unwrap().unwrap();
        assert_eq!(result.bytes, b"payload");
        assert!(result.digest.is_none());
        drop(fetcher);
        let fetcher = Fetcher::new(1).unwrap();
        let file = Owned::new(b"late");
        let pending = fetcher.submit(&file.0, false);
        drop(fetcher);
        // The job may complete before the close lands; either a result or a
        // channel close is acceptable, never a fabricated snapshot.
        match pending.recv() {
            Ok(Ok(raw)) => {
                assert!(raw.bytes.is_empty());
                let mut expected = crate::edit_xxh::Xxh64::new();
                expected.update(b"late");
                assert_eq!(raw.asset_hash, Some(expected.digest()));
            }
            Ok(Err(error)) => assert_eq!(error.kind, "io"),
            Err(_) => {}
        }
    }

    #[test]
    fn with_memory_bounded_worker_carries_digest() {
        let fetcher = Fetcher::with_memory(1, crate::session::MemoryMode::Bounded).unwrap();
        let file = Owned::new(b"payload");
        let raw = fetcher.submit(&file.0, true).recv().unwrap().unwrap();
        assert_eq!(raw.bytes, b"payload");
        assert!(raw.digest.is_some());
    }
}
