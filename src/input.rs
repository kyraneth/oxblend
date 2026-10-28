//! Explicit backing ownership. live raw FD or owned immutable bytes.
//! Linux compressed storage is an unnamed inode owned solely by its descriptor.
use crate::{Error, Result, checked};
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub(crate) struct Snapshot {
    pub metadata: std::fs::Metadata,
    pub reader: crate::BlendFile,
    // Content identity retained at capture: physical+decoded xxh64 over the
    // exact bytes the capture reads observed. The decoded body itself lives
    // only in the shared working-body slot behind `Backing::Cached`.
    pub identity: crate::body_cache::SourceDigest,
    // Inseparable from this exact reader: the saved seek verdict
    // (`Result<Option<SeekTable>>`) lives inside the proof, including a
    // readable-full-stream verdict that stays write-inadmissible. There is
    // no proof-less captured snapshot; selective readers are the separate
    // `StoredBlend::Inspection` variant, never pack-admissible.
    pub proof: crate::edit_container::capture::Proof,
}
fn signature(m: &std::fs::Metadata) -> (u64, u64, u64, i64, i64) {
    crate::fetch::signature(m)
}

/// L17 Phase 2 selective raw windows (ephemeral CLI pack/pack-sequence only).
///
/// Demand-filled 1 MiB aligned raw windows for uncompressed blends. Every
/// fetched window is immutable and retained for the SourceEntry lifetime;
/// later header/field reads reuse it across roots and PerFile walks, and
/// publication holes are consumed once in increasing offsets. A miss opens
/// and checks the source FD and path against the captured ctime-inclusive
/// signature before AND after the window fill; it is never a whole-source
/// materialization or digest replay. Compressed families never enter this
/// representation (they keep full decoded bodies). Long-lived Rust/Python
/// Sessions never construct it (F4); only the ephemeral CLI pack store
/// routes raw blends here, and no selective entry is promotable into a
/// general Session.
/// L19 borrowed view over retained selective windows.
///
/// Dense planning reads (pointer fields, ID headers) consult the same
/// retained window repeatedly. Copying the whole block per field
/// (`Cow::Owned` per range) costs O(fields x block) memcpy on top of the
/// retained bytes. When the requested range lies wholly inside one retained
/// window — the common case, since windows are 1 MiB aligned and fields are
/// bytes — this holds that window's `Arc` alive and slices it without
/// copying. Spanning ranges fall back to the guarded owned copy. Guards,
/// byte identity and the no-extra-source-bytes property are identical in
/// both paths: the same `fetch_window` fills, the same ctime-inclusive
/// signature checks and the same retained-byte accounting. No window is
/// evicted, but the `Arc` makes that a type property rather than a comment.
/// Hot-path reuse stays point-free (only the existing `raw.window.miss`
/// on fills and the aggregated hit counter), per the flattrace
/// hot-aggregation rule.
#[derive(Debug)]
pub(crate) enum WindowView<'a> {
    Slice(&'a [u8]),
    Window {
        window: Arc<Vec<u8>>,
        start: usize,
        len: usize,
    },
    Owned(Vec<u8>),
}

impl WindowView<'_> {
    pub(crate) fn as_slice(&self) -> &[u8] {
        match self {
            Self::Slice(bytes) => bytes,
            Self::Window { window, start, len } => &window[*start..*start + *len],
            Self::Owned(bytes) => &bytes[..],
        }
    }
}

#[derive(Debug)]
pub(crate) struct RawWindows {
    path: PathBuf,
    len: usize,
    signature: crate::fetch::CaptureSignature,
    quantum: usize,
    cache: Mutex<BTreeMap<u64, Arc<Vec<u8>>>>,
    windows_fetched: AtomicU64,
    window_hits: AtomicU64,
    bytes_fetched: AtomicU64,
}

impl RawWindows {
    fn window_start(&self, offset: u64) -> u64 {
        offset - offset % self.quantum as u64
    }

    /// Live-source identity check against the captured ctime-inclusive
    /// signature (FD and path). Shorter-than-captured is truncation;
    /// any other mismatch is a source change. Never reads body bytes.
    fn check_live(&self) -> Result<std::fs::Metadata> {
        let file = File::open(&self.path).map_err(io)?;
        let file_meta = file.metadata().map_err(io)?;
        let path_meta = std::fs::metadata(&self.path).map_err(io)?;
        if file_meta.len() < self.len as u64 || path_meta.len() < self.len as u64 {
            return Err(Error::new("truncated", "live backing truncated"));
        }
        if crate::fetch::capture_signature(&file_meta) != self.signature
            || crate::fetch::capture_signature(&path_meta) != self.signature
        {
            return Err(Error::new(
                "input_changed",
                "source signature changed since snapshot",
            ));
        }
        Ok(file_meta)
    }

    fn read_at_exact(file: &File, mut buf: &mut [u8], mut offset: u64) -> Result<()> {
        while !buf.is_empty() {
            match file.read_at(buf, offset) {
                Ok(0) => {
                    return Err(Error::new("truncated", "live backing truncated"));
                }
                Ok(n) => {
                    buf = &mut buf[n..];
                    offset += n as u64;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(io(e)),
            }
        }
        Ok(())
    }

    /// Fetch (or reuse) the single aligned window containing `offset`.
    /// Misses check the live source before and after the fill and retain
    /// the window; hits only bump the aggregated counter (hot path stays
    /// point-free, per the flattrace hot-aggregation rule). Fill failures
    /// carry the worker-identical `fetch/read snapshot` context, exactly
    /// like a failed worker read, including its `input_changed` kind for
    /// short/truncated fills (the worker maps short reads to signature
    /// failure, never to a backing-level `truncated`, which stays reserved
    /// for scan boundaries mirroring `File`); the parse layer above stays
    /// unwrapped.
    fn fetch_window(&self, offset: u64) -> Result<Arc<Vec<u8>>> {
        let start = self.window_start(offset);
        if let Some(hit) = self.cache.lock().unwrap().get(&start).cloned() {
            self.window_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(hit);
        }
        self.fill_check()?;
        let want = (self.len as u64 - start).min(self.quantum as u64) as usize;
        let file = File::open(&self.path)
            .map_err(io)
            .map_err(|e| self.fill_error(e))?;
        let mut window = vec![0u8; want];
        Self::read_at_exact(&file, &mut window, start).map_err(|e| self.fill_error(e))?;
        self.fill_check()?;
        let window = Arc::new(window);
        self.cache.lock().unwrap().insert(start, window.clone());
        self.windows_fetched.fetch_add(1, Ordering::Relaxed);
        self.bytes_fetched.fetch_add(want as u64, Ordering::Relaxed);
        crate::diagnostics::point(
            "raw.window.miss",
            serde_json::json!({"path": self.path.to_string_lossy(), "offset": start, "length": want}),
        );
        Ok(window)
    }

    /// Borrowed range view: zero-copy when the range lies wholly inside
    /// one retained window, guarded owned copy otherwise. Bounds, guards
    /// and byte identity match `range_owned` exactly.
    fn view(&self, offset: usize, len: usize) -> Result<WindowView<'_>> {
        let end = checked::add(offset, len)?;
        if end > self.len {
            return Err(Error::new("truncated", "range outside retained windows"));
        }
        if len == 0 {
            return Ok(WindowView::Owned(Vec::new()));
        }
        let first = self.window_start(offset as u64);
        let last = self.window_start((end - 1) as u64);
        if first == last {
            let window = self.fetch_window(offset as u64)?;
            let inner = offset - first as usize;
            // `fetch_window` retains exactly `min(quantum, len - start)`
            // bytes, so `inner + len` is in-bounds whenever `end <= len`.
            if inner + len <= window.len() {
                return Ok(WindowView::Window {
                    window,
                    start: inner,
                    len,
                });
            }
        }
        Ok(WindowView::Owned(self.range_owned(offset, len)?))
    }

    fn range_owned(&self, offset: usize, len: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        let mut remaining = len;
        let mut cursor = offset as u64;
        while remaining > 0 {
            let start = self.window_start(cursor);
            let window = self.fetch_window(cursor)?;
            let inner = (cursor - start) as usize;
            let take = remaining.min(window.len() - inner);
            out.extend_from_slice(&window[inner..inner + take]);
            cursor += take as u64;
            remaining -= take;
        }
        Ok(out)
    }

    fn check_source(&self) -> Result<()> {
        self.check_live().map(drop)
    }

    /// Worker-read analogue for fills: fetch/read context plus the worker's
    /// `input_changed` kind for short/truncated sources. Scan boundaries
    /// keep the backing-level kinds via `check_source`, mirroring `File`.
    fn fill_check(&self) -> Result<()> {
        self.check_live().map(drop).map_err(|e| self.fill_error(e))
    }

    fn fill_error(&self, error: Error) -> Error {
        let error = crate::edit_output::session_context(error, "fetch/read snapshot", &self.path);
        if error.kind == "truncated" {
            return error.reclassify("input_changed");
        }
        error
    }

    fn retained_bytes(&self) -> u64 {
        self.cache
            .lock()
            .unwrap()
            .values()
            .map(|w| w.len() as u64)
            .sum()
    }

    pub(crate) fn stats_value(&self) -> serde_json::Value {
        serde_json::json!({
            "mode": "raw_selective_windows",
            "decoded_len": self.len,
            "quantum": self.quantum,
            "windows_cached": self.cache.lock().unwrap().len(),
            "bytes_retained": self.retained_bytes(),
            "bytes_fetched": self.bytes_fetched.load(Ordering::Relaxed),
            "window_hits": self.window_hits.load(Ordering::Relaxed),
            "windows_fetched": self.windows_fetched.load(Ordering::Relaxed),
        })
    }

    fn scan_with<T>(&self, f: impl FnOnce(&mut dyn ReadSeek) -> Result<T>) -> Result<T> {
        // Mirrors `Backing::File::scan`: the post-closure source check runs
        // even when the closure failed, and its error wins — a mutation
        // racing the stream is reported at the boundary, not masked by a
        // lossy inner read error. The typed side channel only preserves a
        // fill failure the inner error mapping would otherwise lose.
        self.check_source()?;
        let mut scan = WindowScan {
            windows: self,
            pos: 0,
            typed: std::cell::RefCell::new(None),
        };
        let result = f(&mut scan);
        self.check_source()?;
        if let Some(typed) = scan.typed.borrow_mut().take() {
            return Err(typed);
        }
        result
    }
}

/// Sequential windowed reader for `Backing::scan`. Reads serve cached
/// windows and fetch holes once in increasing offsets; seeks only move the
/// logical cursor. A failed window fill records its typed error
/// (`input_changed`/`io`) at the side so `scan_with` can return it instead
/// of the lossy `io`-mapped read error.
struct WindowScan<'a> {
    windows: &'a RawWindows,
    pos: u64,
    typed: std::cell::RefCell<Option<Error>>,
}

impl Read for WindowScan<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let len = self.windows.len as u64;
        if self.pos >= len {
            return Ok(0);
        }
        let mut total = 0;
        while total < buf.len() && self.pos < len {
            let window = match self.windows.fetch_window(self.pos) {
                Ok(w) => w,
                Err(e) => {
                    self.typed.borrow_mut().replace(e);
                    return Err(std::io::Error::other("selective window fetch failed"));
                }
            };
            let start = self.windows.window_start(self.pos);
            let inner = (self.pos - start) as usize;
            let avail = window.len().saturating_sub(inner);
            if avail == 0 {
                break;
            }
            let want = (buf.len() - total)
                .min(avail)
                .min((len - self.pos) as usize);
            buf[total..total + want].copy_from_slice(&window[inner..inner + want]);
            self.pos += want as u64;
            total += want;
        }
        Ok(total)
    }
}

impl Seek for WindowScan<'_> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next: i64 = match position {
            SeekFrom::Start(n) => i64::try_from(n).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "scan offset overflow")
            })?,
            SeekFrom::End(n) => {
                i64::try_from(self.windows.len as u64).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "scan offset overflow")
                })? + n
            }
            SeekFrom::Current(n) => {
                i64::try_from(self.pos).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "scan offset overflow")
                })? + n
            }
        };
        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "scan seek before start",
            ));
        }
        self.pos = next as u64;
        Ok(self.pos)
    }
}

impl Snapshot {
    pub(crate) fn raw_capture(&self) -> Option<&Backing> {
        // Raw-family captures ARE their physical container. Byte-backed and
        // selective-window holders qualify: a private captured body streams
        // its bytes, a verified working body streams its
        // exclusively-guarded verified bytes, and selective windows stream
        // resident windows plus checked holes in increasing offsets — never
        // a reread of the live source outside the signature guard.
        // Compressed families and inspection backings keep None (live-FD
        // fallback with its guards).
        if self.proof.family != "raw" {
            return None;
        }
        match &self.reader.data {
            Backing::Bytes(_) | Backing::Cached(_) | Backing::RawWindows(_) => {
                Some(&self.reader.data)
            }
            _ => None,
        }
    }
    /// The shared verified working-body holder of this captured snapshot,
    /// present once the arena adopted the capture decode into the slot.
    /// Phase boundaries (reuse checks, admission, publication) use it to
    /// force a digest proof; ordinary field reads keep the cheap resident
    /// path through the backing itself.
    pub(crate) fn verified_body(&self) -> Option<&std::sync::Arc<crate::body_cache::VerifiedBody>> {
        match &self.reader.data {
            Backing::Cached(verified) => Some(verified),
            _ => None,
        }
    }
    pub fn open_checked(&self, path: &Path) -> Result<File> {
        let file = File::open(path).map_err(io)?;
        if crate::fetch::capture_signature(&file.metadata().map_err(io)?)
            != crate::fetch::capture_signature(&self.metadata)
            || crate::fetch::capture_signature(&path.metadata().map_err(io)?)
                != crate::fetch::capture_signature(&self.metadata)
        {
            return Err(Error::new(
                "input_changed",
                "source signature changed since snapshot",
            ));
        }
        Ok(file)
    }
    pub fn check(&self, path: &Path) -> Result<()> {
        self.open_checked(path).map(drop)
    }
}

fn io(error: std::io::Error) -> Error {
    Error::new(
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            "truncated"
        } else {
            "io"
        },
        error.to_string(),
    )
}

pub(crate) struct Source<'a> {
    pub file: File,
    pub failed: &'a Cell<bool>,
}
impl Read for Source<'_> {
    fn read(&mut self, data: &mut [u8]) -> std::io::Result<usize> {
        let result = self.file.read(data);
        if result
            .as_ref()
            .is_err_and(|e| e.kind() != std::io::ErrorKind::Interrupted)
        {
            self.failed.set(true);
        }
        result
    }
}

impl Seek for Source<'_> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let result = self.file.seek(position);
        if result
            .as_ref()
            .is_err_and(|e| e.kind() != std::io::ErrorKind::Interrupted)
        {
            self.failed.set(true);
        }
        result
    }
}

/// Same full-stream decoders as the live reader, with private owned output.
pub(crate) fn decode_owned(file: File, gzip: bool) -> Result<Vec<u8>> {
    let mut span = crate::diagnostics::Span::new(
        "decode",
        if crate::diagnostics::enabled() {
            serde_json::json!({"format":if gzip {"gzip"} else {"zstd"},"compressed":true})
        } else {
            serde_json::Value::Null
        },
    );
    let result = (|| {
        let failed = Cell::new(false);
        let input = BufReader::new(Source {
            file,
            failed: &failed,
        });
        let mut decoded = Vec::new();
        let result = if gzip {
            crate::stream_decode::decode(
                &mut flate2::read::MultiGzDecoder::new(input),
                &mut decoded,
            )
        } else {
            let mut decoder = zstd::stream::read::Decoder::new(input).map_err(|e| {
                Error::new(
                    if failed.get() { "io" } else { "compression" },
                    e.to_string(),
                )
            })?;
            crate::stream_decode::decode(&mut decoder, &mut decoded)
        };
        result.map_err(|e| {
            if failed.get() {
                Error::new("io", e.message)
            } else {
                e
            }
        })?;
        if !decoded.starts_with(b"BLENDER") {
            return Err(Error::new("invalid_header", "decoded BLENDER magic"));
        }
        Ok(decoded)
    })();
    if let Ok(decoded) = &result
        && span.active()
    {
        span.data(serde_json::json!({"decoded_bytes":decoded.len()}));
    }
    span.result(result)
}

/// Spill filesystem figure used by blocks admission without materializing it.
/// The directory is the operation-frozen temp root, never a re-sampled
/// process temp: capacity is measured where the spill will actually land.
pub(crate) fn temp_spill_available(temp_root: &Path) -> Result<u64> {
    let path = std::ffi::CString::new(temp_root.as_os_str().as_encoded_bytes().to_vec())
        .map_err(|_| Error::new("io", "temporary directory path contains NUL"))?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(io(std::io::Error::last_os_error()));
    }
    let stats = unsafe { stats.assume_init() };
    stats
        .f_bavail
        .checked_mul(stats.f_frsize)
        .ok_or_else(|| Error::new("overflow", "spill filesystem available bytes"))
}
pub(crate) fn require_spill_space(required: u64, available: u64) -> Result<()> {
    if required > available {
        return Err(Error::new(
            "spill_space",
            format!(
                "decoded BHead spill requires {required} bytes; available {available} bytes; shortfall {} bytes",
                required - available
            ),
        ));
    }
    Ok(())
}
fn spill_space(destination: &mut Destination<&mut File>, required: u64) -> Result<()> {
    use std::os::fd::AsRawFd;
    let available = match destination.available {
        Some(value) => value,
        None => {
            let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
            // Sample the exact owned O_TMPFILE filesystem once, before writes.
            if unsafe { libc::fstatvfs(destination.file.get_ref().as_raw_fd(), stats.as_mut_ptr()) }
                != 0
            {
                return Err(io(std::io::Error::last_os_error()));
            }
            let stats = unsafe { stats.assume_init() };
            let value = stats
                .f_bavail
                .checked_mul(stats.f_frsize)
                .ok_or_else(|| Error::new("overflow", "spill filesystem available bytes"))?;
            destination.available = Some(value);
            value
        }
    };
    admit_spill_extent(&mut destination.admitted, required, available)
}

#[cfg(test)]
#[path = "input_capture_tests.rs"]
mod capture_tests;

#[cfg(test)]
#[path = "input_selective_tests.rs"]
mod selective_tests;

#[cfg(test)]
mod spill_tests {
    #[test]
    fn cumulative_extents_and_overflow_are_checked_without_consuming_failed_extent() {
        let mut total = 0;
        super::admit_spill_extent(&mut total, 12, 44).unwrap();
        super::admit_spill_extent(&mut total, 24, 44).unwrap();
        let e = super::admit_spill_extent(&mut total, 24, 44).unwrap_err();
        assert_eq!(e.kind, "spill_space");
        assert!(e.message.contains("requires 60 bytes"));
        assert_eq!(total, 36);
        let mut total = u64::MAX;
        assert_eq!(
            super::admit_spill_extent(&mut total, 1, u64::MAX)
                .unwrap_err()
                .kind,
            "overflow"
        );
    }
    #[test]
    fn shortfall_is_typed_and_not_an_invented_cap() {
        super::require_spill_space(7, 7).unwrap();
        super::require_spill_space(8, 100).unwrap();
        let e = super::require_spill_space(8, 7).unwrap_err();
        assert_eq!(e.kind, "spill_space");
        assert!(e.message.contains("shortfall 1 bytes"));
    }
    #[test]
    fn live_spill_entry_refuses_format_max_before_inflating_payload() {
        // The signed 64-bit BHead extent is imposed by native format 1,
        // not a decoder cap. Missing payload is deliberate: space refusal must
        // happen before attempting to read it, through the real spill entry.
        let mut b = b"BLENDER17-01v0503DATA".to_vec();
        b.extend_from_slice(&0u32.to_le_bytes()); // SDNA
        b.extend_from_slice(&0u64.to_le_bytes()); // old address
        b.extend_from_slice(&(i64::MAX as u64).to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes()); // count
        let dir = crate::test_scratch::scratch_dir(&std::env::temp_dir());
        let path = dir.join("extent.zst");
        std::fs::write(&path, zstd::stream::encode_all(b.as_slice(), 0).unwrap()).unwrap();
        let e = super::Backing::open(&path).unwrap_err();
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!(e.kind, "spill_space");
        assert!(e.message.contains("shortfall"));
    }
    #[test]
    fn valid_bhead_shortfall_refuses_before_payload_or_spill() {
        let mut b = b"BLENDER-v405DATA".to_vec();
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(&[0; 16]);
        b.extend_from_slice(&[0; 8]);
        let mut source = std::io::Cursor::new(b);
        let mut output = Vec::new();
        let e = crate::stream_decode::decode_checked(&mut source, &mut output, |_, required| {
            super::require_spill_space(required, 31)
        })
        .unwrap_err();
        assert_eq!(e.kind, "spill_space");
        assert!(
            e.message
                .contains("requires 32 bytes; available 31 bytes; shortfall 1 bytes")
        );
        assert_eq!(source.position(), 36); // Header+BHead, no decoded payload.
        assert_eq!(output, b"BLENDER-v405"); // BHead/payload not written.
    }
}

pub(crate) fn admit_spill_extent(total: &mut u64, extent: u64, available: u64) -> Result<()> {
    let required = total
        .checked_add(extent)
        .ok_or_else(|| Error::new("overflow", "cumulative decoded spill extent"))?;
    require_spill_space(required, available)?;
    *total = required;
    Ok(())
}

struct Destination<W: Write> {
    // Stdlib's transport buffer coalesces BHead/payload writes; no input cap.
    file: std::io::BufWriter<W>,
    failed: bool,
    available: Option<u64>,
    admitted: u64,
}
impl<W: Write> Write for Destination<W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let result = self.file.write(data);
        if matches!(result, Ok(0)) && !data.is_empty()
            || result
                .as_ref()
                .is_err_and(|e| e.kind() != std::io::ErrorKind::Interrupted)
        {
            self.failed = true;
        }
        result
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let result = self.file.flush();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}

#[cfg(test)]
mod buffered_spill_tests {
    use super::*;

    fn open_encoded(raw: &[u8], gzip: bool) -> Result<Backing> {
        let encoded = if gzip {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(raw).unwrap();
            encoder.finish().unwrap()
        } else {
            zstd::stream::encode_all(raw, 0).unwrap()
        };
        let temp = std::env::temp_dir();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_TMPFILE)
            .open(&temp)
            .unwrap();
        file.write_all(&encoded).unwrap();
        Backing::open_file(file, &temp)
    }

    #[test]
    fn buffered_spill_open_flushes_tail_before_indexing_both_codecs() {
        let mut raw = b"BLENDER-v405ENDB".to_vec();
        raw.extend_from_slice(&[0; 20]);
        for gzip in [true, false] {
            let backing = open_encoded(&raw, gzip).unwrap();
            assert_eq!(backing.len(), raw.len());
            assert_eq!(backing.range(0, raw.len()).unwrap().as_ref(), raw);
        }
    }

    #[test]
    fn buffered_spill_open_preserves_structural_error_after_buffered_header() {
        let mut raw = b"BLENDER-v405".to_vec();
        raw.extend_from_slice(&[0; 24]);
        for gzip in [true, false] {
            let error = open_encoded(&raw, gzip).unwrap_err();
            assert_eq!(error.kind, "invalid_block");
            assert_eq!(error.message, "zero BHead code is not a native block");
        }
    }

    struct FailedSink;
    impl Write for FailedSink {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from_raw_os_error(libc::ENOSPC))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn buffered_spill_tail_error_is_io_and_not_lost_on_drop() {
        let mut destination = Destination {
            file: std::io::BufWriter::new(FailedSink),
            failed: false,
            available: None,
            admitted: 0,
        };
        destination.write_all(b"BLENDER-v405").unwrap();
        assert!(!destination.failed);
        let error = destination.flush().map_err(io).unwrap_err();
        assert_eq!(error.kind, "io");
        assert_eq!(
            error.message,
            std::io::Error::from_raw_os_error(libc::ENOSPC).to_string()
        );
        assert!(destination.failed);
        // Taking the parts never retries the failed tail.
        let (_, pending) = destination.file.into_parts();
        assert_eq!(pending.unwrap(), b"BLENDER-v405");
    }

    #[test]
    fn buffered_spill_large_write_marks_destination_failure() {
        let mut destination = Destination {
            file: std::io::BufWriter::new(FailedSink),
            failed: false,
            available: None,
            admitted: 0,
        };
        let data = vec![0; destination.file.capacity()];
        assert!(destination.write_all(&data).is_err());
        assert!(destination.failed);
        let _ = destination.file.into_parts();
    }
}

#[derive(Debug)]
pub(crate) enum Backing {
    Bytes(Vec<u8>),
    Trace(Box<crate::trace_input::Input>),
    /// L17 Phase 2 selective raw windows for ephemeral CLI pack stores.
    /// Parsed BHead directory/DNA plus demand-filled immutable 1 MiB
    /// windows; compressed families and Sessions never use this variant.
    RawWindows(Box<RawWindows>),
    File {
        file: File,
        len: usize,
        metadata: std::fs::Metadata,
    },
    /// Captured verified-on-reuse body: parsed index retained, decoded body
    /// re-materialized through the shared working-body slot with digest
    /// verification on every re-read. Range data is returned owned because
    /// the slot is exclusive interior state, never a borrowed view.
    Cached(std::sync::Arc<crate::body_cache::VerifiedBody>),
}
impl Backing {
    pub fn len(&self) -> usize {
        match self {
            Self::Bytes(data) => data.len(),
            Self::Trace(input) => input.len(),
            Self::RawWindows(windows) => windows.len,
            Self::File { len, .. } => *len,
            Self::Cached(verified) => verified.len,
        }
    }
    pub fn check_range(&self, offset: usize, len: usize) -> Result<()> {
        let end = checked::add(offset, len)?;
        if end > self.len() {
            return Err(Error::new("truncated", "range outside indexed backing"));
        }
        if let Self::File { file, .. } = self {
            let live_len = file.metadata().map_err(io)?.len();
            if u64::try_from(end).map_err(|_| Error::new("overflow", "range end"))? > live_len {
                return Err(Error::new("truncated", "live backing truncated"));
            }
        }
        if let Self::RawWindows(windows) = self {
            // Stored-length bound only; liveness is proven per window fill
            // and at scan boundaries under the ctime-inclusive signature.
            let _ = windows.len;
        }
        Ok(())
    }
    pub fn range(&self, offset: usize, len: usize) -> Result<Cow<'_, [u8]>> {
        self.check_range(offset, len)?;
        match self {
            Self::Bytes(data) => Ok(Cow::Borrowed(checked::bytes(data, offset, len)?)),
            Self::Trace(input) => input.range(offset, len),
            Self::RawWindows(windows) => Ok(Cow::Owned(windows.range_owned(offset, len)?)),
            Self::Cached(verified) => {
                // Owned range data: the exclusive working-body guard outlives
                // this call only as a copy, so no borrowed view of shared
                // interior state can escape or alias a later eviction.
                let body = verified.acquire()?;
                let slice = checked::bytes(&body, offset, len)?;
                Ok(Cow::Owned(slice.to_vec()))
            }
            Self::File { file, .. } => {
                let mut value = vec![0; len];
                file.read_exact_at(
                    &mut value,
                    u64::try_from(offset).map_err(|_| Error::new("overflow", "read offset"))?,
                )
                .map_err(io)?;
                Ok(Cow::Owned(value))
            }
        }
    }
    /// Borrowed range view: zero-copy for in-memory bodies, for trace
    /// ranges the trace layer already serves borrowed, and for selective
    /// windows wholly inside one retained window; guarded owned copy
    /// otherwise. Same bounds, guards and byte identity as `range()`.
    pub fn view(&self, offset: usize, len: usize) -> Result<WindowView<'_>> {
        self.check_range(offset, len)?;
        match self {
            Self::Bytes(data) => Ok(WindowView::Slice(checked::bytes(data, offset, len)?)),
            Self::Trace(input) => match input.range(offset, len)? {
                Cow::Borrowed(bytes) => Ok(WindowView::Slice(bytes)),
                Cow::Owned(bytes) => Ok(WindowView::Owned(bytes)),
            },
            Self::RawWindows(windows) => windows.view(offset, len),
            Self::Cached(verified) => {
                // Owned range data: the exclusive working-body guard outlives
                // this call only as a copy, so no borrowed view of shared
                // interior state can escape or alias a later eviction.
                let body = verified.acquire()?;
                let slice = checked::bytes(&body, offset, len)?;
                Ok(WindowView::Owned(slice.to_vec()))
            }
            Self::File { file, .. } => {
                let mut value = vec![0; len];
                file.read_exact_at(
                    &mut value,
                    u64::try_from(offset).map_err(|_| Error::new("overflow", "read offset"))?,
                )
                .map_err(io)?;
                Ok(WindowView::Owned(value))
            }
        }
    }
    pub fn scan<T>(&self, f: impl FnOnce(&mut dyn ReadSeek) -> Result<T>) -> Result<T> {
        match self {
            Self::Bytes(bytes) => f(&mut Cursor::new(bytes)),
            Self::Cached(verified) => {
                // The guard exclusively owns this source's verified bytes for
                // the whole scan: a nested scan of any source (including this
                // one) acquires its own body and can neither panic on the
                // slot nor be served these bytes.
                let body = verified.acquire()?;
                let result = f(&mut Cursor::new(&body[..]));
                drop(body);
                result
            }
            Self::Trace(input) => input.scan(f),
            Self::RawWindows(windows) => windows.scan_with(f),
            Self::File { file, .. } => {
                // The entire buffered scan is one guarded read transaction.
                self.check_source()?;
                let mut reader = BufReader::new(file.try_clone().map_err(io)?);
                reader.seek(SeekFrom::Start(0)).map_err(io)?;
                let result = f(&mut reader);
                self.check_source()?;
                result
            }
        }
    }
    pub fn check_source(&self) -> Result<()> {
        match self {
            Self::Trace(input) => input.check(),
            Self::RawWindows(windows) => windows.check_source(),
            Self::File { file, metadata, .. } => {
                let current = file.metadata().map_err(io)?;
                if current.len() < metadata.len() {
                    return Err(Error::new(
                        "truncated",
                        "live backing truncated during scan",
                    ));
                }
                if signature(&current) != signature(metadata) {
                    return Err(Error::new(
                        "source_changed",
                        "live backing identity/content changed during scan",
                    ));
                }
                Ok(())
            }
            Self::Bytes(_) => Ok(()),
            // Backing-level check for a verified body is the established
            // latch only: live signature guards stay owned by the Session
            // check sites, and content identity is verified at every
            // re-materialization instead of sampled here.
            Self::Cached(verified) => verified.check(),
        }
    }
    pub fn report(&self, path: &Path) {
        crate::diagnostics::point(
            "reader.stats",
            serde_json::json!({"path":path.to_string_lossy(),"source":self.stats()}),
        );
    }
    pub fn stats(&self) -> serde_json::Value {
        match self {
            Self::Trace(input) => input.stats(),
            Self::RawWindows(windows) => windows.stats_value(),
            Self::Cached(verified) => {
                let counters = verified.counters();
                serde_json::json!({"mode":"verified_working_body","physical_rereads":counters.physical_rereads,"decoded_bytes":counters.decoded_bytes,"digest_verifications":counters.digest_verifications})
            }
            _ => serde_json::json!({"mode":"ordinary_backing"}),
        }
    }
    /// Stable read path (inspect/get/pointer/payload CLI, file-owned reads):
    /// keeps sampling the process temp dir, exactly as before. Operation
    /// paths pass their frozen root explicitly via `open_file`.
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_file(File::open(path).map_err(io)?, &std::env::temp_dir())
    }
    pub fn open_file(mut file: File, temp: &Path) -> Result<Self> {
        file.seek(SeekFrom::Start(0)).map_err(io)?;
        let mut magic = [0; 4];
        let mut n = 0;
        while n < magic.len() {
            match file.read_at(&mut magic[n..], n as u64) {
                Ok(0) => break,
                Ok(read) => n += read,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(io(e)),
            }
        }
        let gzip = n >= 2 && magic[..2] == [0x1f, 0x8b];
        let zstd = n == 4
            && (magic == [0x28, 0xb5, 0x2f, 0xfd]
                || (u32::from_le_bytes(magic) & 0xfffffff0) == 0x184d2a50);
        let file = if gzip || zstd {
            let decode_span = crate::diagnostics::Span::new(
                "decode",
                if crate::diagnostics::enabled() {
                    serde_json::json!({"format":if gzip {"gzip"} else {"zstd"},"compressed":true})
                } else {
                    serde_json::Value::Null
                },
            );
            let decoded_result = (|| {
                // O_TMPFILE is a Linux filesystem contract, not a guessed naming scheme
                // or a persistent cache. Failure remains explicit IO; no unsafe mmap.
                // The unnamed inode lives on the operation-frozen temp filesystem.
                let mut decoded = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .mode(0o600)
                    .custom_flags(libc::O_TMPFILE)
                    .open(temp)
                    .map_err(io)?;
                let source_failed = Cell::new(false);
                let input = BufReader::new(Source {
                    file,
                    failed: &source_failed,
                });
                {
                    let mut destination = Destination {
                        file: std::io::BufWriter::new(&mut decoded),
                        failed: false,
                        available: None,
                        admitted: 0,
                    };
                    let result = if gzip {
                        crate::stream_decode::decode_checked(
                            &mut flate2::read::MultiGzDecoder::new(input),
                            &mut destination,
                            spill_space,
                        )
                    } else {
                        let mut decoder = zstd::stream::read::Decoder::new(input).map_err(|e| {
                            Error::new(
                                if source_failed.get() {
                                    "io"
                                } else {
                                    "compression"
                                },
                                e.to_string(),
                            )
                        })?;
                        crate::stream_decode::decode_checked(
                            &mut decoder,
                            &mut destination,
                            spill_space,
                        )
                    };
                    // A successful decode must deliver its buffered tail before
                    // the backing is indexed. Never let Drop hide a write failure
                    // or retry buffered writes after a refused/failed decode.
                    let result = result.and_then(|()| destination.flush().map_err(io));
                    let result = result.map_err(|e| {
                        if destination.failed || source_failed.get() {
                            Error::new("io", e.message)
                        } else {
                            e
                        }
                    });
                    let _ = destination.file.into_parts();
                    result?;
                }
                Ok(decoded)
            })();
            decode_span.result(decoded_result)?
        } else {
            file
        };
        let len = usize::try_from(file.metadata().map_err(io)?.len())
            .map_err(|_| Error::new("overflow", "backing length"))?;
        let metadata = file.metadata().map_err(io)?;
        let backing = Self::File {
            file,
            len,
            metadata,
        };
        if (gzip || zstd) && backing.range(0, len.min(7))?.as_ref() != b"BLENDER" {
            return Err(Error::new("invalid_header", "decoded BLENDER magic"));
        }
        Ok(backing)
    }
}

/// L17 selective raw capture for ephemeral CLI pack stores.
///
/// Returns `Ok(None)` for compressed families (caller falls back to the
/// historical full decoded capture, which retains every decoded byte).
/// The family probe is span-free and ledger-free: four magic bytes only,
/// never a window fill or `observe_input`, so a compressed fallback owns
/// exactly the worker's single fetch/file.open pair and ledger entry.
/// Probe/read-site failures carry the worker-identical `fetch/read
/// snapshot` context, so a missing/unreadable source refuses exactly like
/// the worker's failed open/read. Returns `Ok(Some(_))` for raw blends
/// with a validated parsed directory and a ctime-inclusive proof but no
/// retained full body. The raw path mirrors the worker fetch/file.open
/// span contract exactly once (same fields, same `observe_input` ledger
/// recording), so L1 fetch/file.open presence and once-only capture
/// accounting are unchanged. Parse failures stay single-context, exactly
/// like the full path's post-take parse. No snapshot check here: the
/// caller applies it under the standard snapshot/index context.
pub(crate) fn capture_selective_raw(path: &Path) -> Result<Option<Snapshot>> {
    // Read-site context owner: probe/open failures refuse exactly like the
    // worker's failed open/read (fetch/read snapshot + snapshot/index).
    // Compressed detection returns None with no spans and no ledger entry.
    // Too-small or non-magic inputs fall through to the raw path so the
    // parse below produces the historical single-context error, exactly
    // like the full path's post-take parse.
    let probe = probe_selective_family(path)
        .map_err(|e| crate::edit_output::session_context(e, "fetch/read snapshot", path))?;
    let Some((metadata, len)) = probe else {
        return Ok(None);
    };
    // Raw: the worker-identical fetch/file.open span pair, exactly once.
    // Straight-line span recording (no immediately-invoked closures): the
    // inner build stays a plain helper so error contexts keep their exact
    // worker-identical layering.
    let mut fetch_span = crate::diagnostics::Span::rooted(
        "fetch",
        if crate::diagnostics::enabled() {
            serde_json::json!({"path": path.to_string_lossy(), "blend": true, "links": crate::diagnostics::worker_links()})
        } else {
            serde_json::Value::Null
        },
    );
    // Nested under the rooted fetch span above, exactly like the worker
    // pair (worker file.open spans are children of their fetch span, not
    // siblings). The flattrace reference requires file.open under a
    // linked fetch; the fetch span itself stays structurally rooted.
    let open_span = crate::diagnostics::Span::new(
        "file.open",
        if crate::diagnostics::enabled() {
            serde_json::json!({"path": path.to_string_lossy(), "input": crate::diagnostics::observe_input(path)})
        } else {
            serde_json::Value::Null
        },
    );
    let result = open_span.result(capture_selective_inner(path, metadata, len).map(Some));
    fetch_span.record_result(&result);
    result
}

/// Span-free family probe: four magic bytes only. Compressed returns
/// `None` (caller falls back to the full worker capture); raw-assumed
/// inputs return their metadata and length for the guarded build.
fn probe_selective_family(path: &Path) -> Result<Option<(std::fs::Metadata, usize)>> {
    // Worker-identical order: stat and admit regular files BEFORE any
    // open. Opening first would block forever on a FIFO with no writer.
    let metadata = std::fs::metadata(path).map_err(io)?;
    if !metadata.is_file() {
        return Err(Error::new("pack_path", "pack source is not regular file"));
    }
    let probe_file = File::open(path).map_err(io)?;
    let metadata = probe_file.metadata().map_err(io)?;
    let len = usize::try_from(metadata.len())
        .map_err(|_| Error::new("overflow", "physical input length"))?;
    let mut magic = [0u8; 4];
    let mut have = 0;
    while have < 4 {
        match std::os::unix::fs::FileExt::read_at(&probe_file, &mut magic[have..], have as u64) {
            Ok(0) => break,
            Ok(n) => have += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(io(e)),
        }
    }
    drop(probe_file);
    if have >= 2 && magic[..2] == [0x1f, 0x8b] {
        return Ok(None);
    }
    if have == 4 {
        let word = u32::from_le_bytes(magic);
        if word == 0xfd2fb528 || (word & 0xfffffff0) == 0x184d2a50 {
            return Ok(None);
        }
    }
    Ok(Some((metadata, len)))
}

/// Raw selective build: guarded windows, validated parse, proof, snapshot.
/// Fill failures already carry the worker-identical `fetch/read snapshot`
/// context from the window layer; parse failures stay single-context like
/// the full path's post-take parse. No snapshot check here: the caller
/// applies it under the standard snapshot/index context.
fn capture_selective_inner(
    path: &Path,
    metadata: std::fs::Metadata,
    len: usize,
) -> Result<Snapshot> {
    let signature = crate::fetch::capture_signature(&metadata);
    let windows = RawWindows {
        path: path.to_path_buf(),
        len,
        signature,
        quantum: crate::fetch::READ_CHUNK,
        cache: Mutex::new(BTreeMap::new()),
        windows_fetched: AtomicU64::new(0),
        window_hits: AtomicU64::new(0),
        bytes_fetched: AtomicU64::new(0),
    };
    // Window 0 is genuinely retained and reused by the parse below.
    let _ = windows.fetch_window(0)?;
    let reader = crate::BlendFile::from_backing(Backing::RawWindows(Box::new(windows)))?
        .with_source_path(path);
    let proof = crate::edit_container::capture::Proof::new(&metadata, "raw", Ok(None));
    let snapshot = Snapshot {
        metadata,
        reader,
        identity: crate::body_cache::SourceDigest {
            physical: 0,
            decoded: 0,
        },
        proof,
    };
    crate::diagnostics::point(
        "raw.capture.selective",
        serde_json::json!({
            "path": path.to_string_lossy(),
            "decoded_len": snapshot.reader.decompressed_len(),
            "blocks": snapshot.reader.blocks().len(),
        }),
    );
    Ok(snapshot)
}

#[cfg(test)]
mod scan_guard_tests {
    use super::*;
    #[test]
    fn file_scan_checks_start_and_end_identity_and_truncation() {
        for when in ["start", "end"] {
            for truncate in [false, true] {
                let path = std::env::temp_dir().join(format!(
                    "oxblend-scan-{}-{when}-{truncate}",
                    std::process::id()
                ));
                let mut writer = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .unwrap();
                writer.write_all(b"original scan bytes").unwrap();
                let backing = Backing::open(&path).unwrap();
                let mut change = || {
                    if truncate {
                        writer.set_len(1).unwrap();
                    } else {
                        writer.seek(SeekFrom::Start(0)).unwrap();
                        writer.write_all(b"X").unwrap();
                        writer.flush().unwrap();
                        // Filesystem timestamp granularity can hide two writes
                        // in one tick; explicitly make the metadata change visible.
                        writer.set_modified(std::time::UNIX_EPOCH).unwrap();
                    }
                };
                if when == "start" {
                    change();
                }
                let result = backing.scan(|_| {
                    if when == "end" {
                        change();
                    }
                    Ok(())
                });
                assert_eq!(
                    result.unwrap_err().kind,
                    if truncate {
                        "truncated"
                    } else {
                        "source_changed"
                    }
                );
                std::fs::remove_file(path).unwrap();
            }
        }
    }
}

pub(crate) trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

pub(crate) fn read_exact(reader: &mut dyn ReadSeek, data: &mut [u8]) -> Result<()> {
    reader.read_exact(data).map_err(io)
}
pub(crate) fn skip(reader: &mut dyn ReadSeek, len: usize) -> Result<()> {
    reader
        .seek_relative(i64::try_from(len).map_err(|_| Error::new("overflow", "skip length"))?)
        .map_err(io)
}
pub(crate) fn seek(reader: &mut dyn ReadSeek, offset: usize) -> Result<()> {
    reader
        .seek(SeekFrom::Start(
            u64::try_from(offset).map_err(|_| Error::new("overflow", "scan offset"))?,
        ))
        .map_err(io)?;
    Ok(())
}
