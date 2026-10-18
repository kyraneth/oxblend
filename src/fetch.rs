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
}
/// Whole-file snapshot buffer size; measured implementation choice, see
/// `read_into`.
pub(crate) const READ_CHUNK: usize = 1 << 20;
struct Job {
    path: PathBuf,
    blend: bool,
    reply: mpsc::Sender<Result<Raw>>,
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
impl Fetcher {
    pub fn new(roots: usize) -> Result<Self> {
        let mut limit = std::mem::MaybeUninit::<libc::rlimit>::uninit();
        if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, limit.as_mut_ptr()) } != 0 {
            return Err(io(std::io::Error::last_os_error()));
        }
        let limit = unsafe { limit.assume_init() }.rlim_cur;

        let occupied = std::fs::read_dir("/proc/self/fd").map_err(io)?.count() - 1;

        let publication_fds = 1 + 2 + 1;

        let worker_fds = 1 + usize::from(std::env::var_os("OXBLEND_RX_INTERFACE").is_some());
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
        if std::env::var_os("OXBLEND_PHASES").is_some() {
            eprintln!(
                "OXBLEND_FD_BUDGET {limit} {occupied} {publication_fds} {worker_fds} {lanes}"
            );
        }
        let shared = Arc::new(Shared::default());
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
                        let result = fetch(&job.path, job.blend, &shared);
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
    result.map_err(|e| {
        if failed.get() {
            Error::new("io", e.message)
        } else {
            e
        }
    })?;
    let proof = crate::edit_container::capture::Proof::new(metadata, family, table);
    Ok((output.bytes, proof))
}
fn le_magic(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("four-byte physical magic"))
}
fn fetch(path: &Path, blend: bool, shared: &Shared) -> Result<Raw> {
    let started = crate::phase::Start::new();
    if !path.metadata().map_err(io)?.is_file() {
        return Err(Error::new("pack_path", "pack source is not regular file"));
    }
    let mut file = File::open(path).map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    if !metadata.is_file() {
        return Err(Error::new("pack_path", "pack source is not regular file"));
    }
    let captured = if blend {
        blend_capture(&mut file, &metadata, shared).map(|(bytes, proof)| (bytes, Some(proof)))
    } else {
        let capacity = usize::try_from(metadata.len())
            .map_err(|_| Error::new("overflow", "physical input length"))?;
        let mut bytes = Vec::with_capacity(capacity);
        read_into(&mut file, &mut bytes, || {
            shared.queue.lock().unwrap().closed
        })?;
        Ok((bytes, None))
    };
    if (blend
        && (capture_signature(&file.metadata().map_err(io)?) != capture_signature(&metadata)
            || capture_signature(&path.metadata().map_err(io)?) != capture_signature(&metadata)))
        || (!blend
            && (signature(&file.metadata().map_err(io)?) != signature(&metadata)
                || signature(&path.metadata().map_err(io)?) != signature(&metadata)))
    {
        return Err(Error::new(
            "input_changed",
            "source signature changed during snapshot",
        ));
    }
    let (bytes, proof) = captured?;
    if proof.as_ref().is_none_or(|p| p.family == "raw") && bytes.len() as u64 != metadata.len() {
        return Err(Error::new(
            "input_changed",
            "source signature changed during snapshot",
        ));
    }
    if std::env::var_os("OXBLEND_PHASES").is_some() {
        eprintln!(
            "OXBLEND_FETCH {:.9} {} {} {}",
            started.elapsed().as_secs_f64(),
            metadata.len(),
            if blend { "blend" } else { "opaque" },
            crate::value::hex(path.as_os_str().as_encoded_bytes())
        );
    }
    started.log_rx("fetch");

    drop(file);
    Ok(Raw {
        metadata,
        bytes,
        proof,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

    /// input-capture.md: cancellation during capture stops without a
    /// fabricated snapshot. The queue is observed before every chunk.
    #[test]
    fn cancelled_capture_stops_without_a_snapshot() {
        let file = Owned::new(&vec![7u8; 4 * READ_CHUNK]);
        let shared = Shared::default();
        shared.queue.lock().unwrap().closed = true;
        let error = fetch(&file.0, false, &shared).unwrap_err();
        assert_eq!(error.kind, "io");
        assert_eq!(error.message, "owning pack stopped private fetch");
    }

    /// input-capture.md: a source that changes during capture fails
    /// explicitly. A concurrent writer appends to a file large enough that
    /// the read is still in progress; the signature guard then refuses.
    #[test]
    fn source_change_during_capture_fails_explicitly() {
        let path = std::env::temp_dir().join(format!(
            "oxblend-member8-fetch-change-{}",
            std::process::id()
        ));
        {
            let block = vec![0u8; 1 << 20];
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .unwrap();
            for _ in 0..512 {
                std::io::Write::write_all(&mut file, &block).unwrap();
            }
        }
        struct Guard(PathBuf);
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _guard = Guard(path.clone());
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let writer = {
            let path = path.clone();
            let stop = stop.clone();
            thread::spawn(move || {
                let mut appender = std::fs::OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .unwrap();
                while !stop.load(Ordering::Relaxed) {
                    std::io::Write::write_all(&mut appender, b"x").unwrap();
                }
            })
        };
        let shared = Shared::default();
        let result = fetch(&path, false, &shared);
        stop.store(true, Ordering::Relaxed);
        writer.join().unwrap();
        let error = result.unwrap_err();
        assert_eq!(error.kind, "input_changed");
    }

    /// The public queue contract: a submitted job is answered by a worker,
    /// and closing the Fetcher stops workers without deadlocking.
    #[test]
    fn fetcher_answers_and_stops() {
        let fetcher = Fetcher::new(1).unwrap();
        let file = Owned::new(b"payload");
        let result = fetcher.submit(&file.0, true).recv().unwrap().unwrap();
        assert_eq!(result.bytes, b"payload");
        drop(fetcher);
        let fetcher = Fetcher::new(1).unwrap();
        let file = Owned::new(b"late");
        let pending = fetcher.submit(&file.0, false);
        drop(fetcher);
        // The job may complete before the close lands; either a result or a
        // channel close is acceptable, never a fabricated snapshot.
        match pending.recv() {
            Ok(Ok(raw)) => assert_eq!(raw.bytes, b"late"),
            Ok(Err(error)) => assert_eq!(error.kind, "io"),
            Err(_) => {}
        }
    }
}
