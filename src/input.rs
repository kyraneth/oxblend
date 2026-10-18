//! Explicit backing ownership. live raw FD or owned immutable bytes.
//! Linux compressed storage is an unnamed inode owned solely by its descriptor.
use crate::{Error, Result, checked};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// One pack owns pending physical reads, opaque captured bytes or parsed reader.
enum Input {
    Pending(std::sync::mpsc::Receiver<Result<crate::fetch::Raw>>),
    Opaque(Box<crate::fetch::Raw>),
    Parsed(Rc<Snapshot>),
}
pub(crate) struct PackInputs {
    files: RefCell<BTreeMap<PathBuf, Input>>,
    fetcher: crate::fetch::Fetcher,
}
pub(crate) struct Snapshot {
    pub metadata: std::fs::Metadata,
    pub reader: Rc<crate::BlendFile>,
    pub proof: Option<crate::edit_container::capture::Proof>,
}
fn signature(m: &std::fs::Metadata) -> (u64, u64, u64, i64, i64) {
    crate::fetch::signature(m)
}
impl PackInputs {
    pub fn new(roots: &[PathBuf]) -> Result<Self> {
        let value = Self {
            files: RefCell::new(BTreeMap::new()),
            fetcher: crate::fetch::Fetcher::new(roots.len())?,
        };
        for path in roots {
            value.prefetch(path, true);
        }
        Ok(value)
    }
    pub fn prefetch(&self, path: &Path, blend: bool) {
        let mut files = self.files.borrow_mut();
        if !files.contains_key(path) {
            files.insert(
                path.to_path_buf(),
                Input::Pending(self.fetcher.submit(path, blend)),
            );
        } else if blend {
            self.fetcher.promote(path);
        }
    }
    fn take(&self, path: &Path, blend: bool) -> Result<Input> {
        self.prefetch(path, blend);
        let input = self.files.borrow_mut().remove(path).unwrap();
        match input {
            Input::Pending(result) => result
                .recv()
                .map_err(|_| Error::new("io", "private physical fetch ended without result"))?
                .map(|raw| Input::Opaque(Box::new(raw))),
            other => Ok(other),
        }
    }
    pub fn get(&self, path: &Path) -> Result<Rc<Snapshot>> {
        if let Some(Input::Parsed(value)) = self.files.borrow().get(path) {
            value.check(path)?;
            return Ok(value.clone());
        }
        let started = crate::phase::Start::new();
        let Input::Opaque(raw) = self.take(path, true)? else {
            unreachable!()
        };
        let raw = *raw;
        let raw = if raw.proof.is_some() {
            raw
        } else {
            // A background opaque job may already have started before promotion.
            // Release its physical bytes BEFORE checked blend capture in the same
            // worker lane. The original signature must still match; no new cache.
            let expected = crate::fetch::capture_signature(&raw.metadata);
            drop(raw);
            let captured =
                self.fetcher.submit(path, true).recv().map_err(|_| {
                    Error::new("io", "private physical fetch ended without result")
                })??;
            if crate::fetch::capture_signature(&captured.metadata) != expected {
                return Err(Error::new(
                    "input_changed",
                    "source signature changed since snapshot",
                ));
            }
            captured
        };
        let file = File::open(path).map_err(io)?;
        let metadata = raw.metadata;
        if crate::fetch::capture_signature(&file.metadata().map_err(io)?)
            != crate::fetch::capture_signature(&metadata)
            || crate::fetch::capture_signature(&path.metadata().map_err(io)?)
                != crate::fetch::capture_signature(&metadata)
        {
            return Err(Error::new(
                "input_changed",
                "source signature changed since snapshot",
            ));
        }
        let decoded = raw.bytes;
        let proof = raw.proof;
        // No pack-size-dependent retained descriptor: the checked source ends
        // here, immutable captured bytes own all later body reads.
        drop(file);
        let snapshot = Rc::new(Snapshot {
            metadata,
            reader: Rc::new(crate::BlendFile::from_bytes(decoded)?),
            proof,
        });
        snapshot.check(path)?;
        if std::env::var_os("OXBLEND_PHASES").is_some() {
            eprintln!(
                "OXBLEND_PHASE snapshot {:.9} {} {}",
                started.elapsed().as_secs_f64(),
                snapshot.reader.decompressed_len(),
                crate::value::hex(path.as_os_str().as_encoded_bytes())
            );
        }
        started.log_rx("snapshot");
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), Input::Parsed(snapshot.clone()));
        Ok(snapshot)
    }
    pub fn copy_physical(
        &self,
        path: &Path,
        expected: &std::fs::Metadata,
        output: &mut File,
    ) -> Result<u64> {
        self.copy_physical_using(path, expected, output, crate::kernel_copy::full)
    }
    fn copy_physical_using(
        &self,
        path: &Path,
        expected: &std::fs::Metadata,
        output: &mut File,
        transport: impl FnOnce(
            &File,
            &Path,
            &std::fs::Metadata,
            &mut File,
            Option<&[u8]>,
        ) -> Result<u64>,
    ) -> Result<u64> {
        let input = self.take(path, false)?;
        let result = (|| {
            let current = File::open(path).map_err(io)?;
            if crate::fetch::capture_signature(&current.metadata().map_err(io)?)
                != crate::fetch::capture_signature(expected)
            {
                return Err(Error::new(
                    "input_changed",
                    "asset source changed before captured copy",
                ));
            }
            let copied = match &input {
                Input::Opaque(raw) => {
                    if crate::fetch::capture_signature(&raw.metadata)
                        != crate::fetch::capture_signature(expected)
                    {
                        return Err(Error::new(
                            "input_changed",
                            "captured asset differs from planned signature",
                        ));
                    }
                    std::io::copy(&mut Cursor::new(&raw.bytes), output).map_err(io)?
                }
                Input::Parsed(snapshot) => {
                    snapshot.check(path)?;
                    if crate::fetch::capture_signature(&snapshot.metadata)
                        != crate::fetch::capture_signature(expected)
                    {
                        return Err(Error::new(
                            "input_changed",
                            "captured blend differs from planned signature",
                        ));
                    }
                    // The stage is a real File: use guarded kernel transport,
                    // not a dyn Write stream that re-reads physical bytes.
                    transport(&current, path, expected, output, snapshot.raw_capture()).map_err(
                        |error| {
                            if error.kind == "input_changed" {
                                Error::new("input_changed", "asset source changed during copy")
                            } else {
                                error
                            }
                        },
                    )?
                }
                Input::Pending(_) => unreachable!(),
            };
            if copied != expected.len()
                || crate::fetch::capture_signature(&current.metadata().map_err(io)?)
                    != crate::fetch::capture_signature(expected)
                || crate::fetch::capture_signature(&path.metadata().map_err(io)?)
                    != crate::fetch::capture_signature(expected)
            {
                return Err(Error::new(
                    "input_changed",
                    "asset source changed during copy",
                ));
            }
            Ok(copied)
        })();
        self.files.borrow_mut().insert(path.to_path_buf(), input);
        result
    }
}
impl Snapshot {
    pub(crate) fn raw_capture(&self) -> Option<&[u8]> {
        if self.proof.as_ref().is_some_and(|p| p.family == "raw")
            && let Backing::Bytes(bytes) = &self.reader.data
        {
            Some(bytes)
        } else {
            None
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
    let failed = Cell::new(false);
    let input = BufReader::new(Source {
        file,
        failed: &failed,
    });
    let mut decoded = Vec::new();
    let result = if gzip {
        crate::stream_decode::decode(&mut flate2::read::MultiGzDecoder::new(input), &mut decoded)
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
}

/// Spill filesystem figure used by blocks admission without materializing it.
pub(crate) fn temp_spill_available() -> Result<u64> {
    let path = std::ffi::CString::new(std::env::temp_dir().as_os_str().as_encoded_bytes().to_vec())
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
fn spill_space(destination: &mut Destination<'_>, required: u64) -> Result<()> {
    use std::os::fd::AsRawFd;
    let available = match destination.available {
        Some(value) => value,
        None => {
            let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
            // Sample the exact owned O_TMPFILE filesystem once, before writes.
            if unsafe { libc::fstatvfs(destination.file.as_raw_fd(), stats.as_mut_ptr()) } != 0 {
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

struct Destination<'a> {
    file: &'a mut File,
    failed: bool,
    available: Option<u64>,
    admitted: u64,
}
impl Write for Destination<'_> {
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
        self.file.flush()
    }
}

#[derive(Debug)]
pub(crate) enum Backing {
    Bytes(Vec<u8>),
    Trace(Box<crate::trace_input::Input>),
    File {
        file: File,
        len: usize,
        metadata: std::fs::Metadata,
    },
}
impl Backing {
    pub fn len(&self) -> usize {
        match self {
            Self::Bytes(data) => data.len(),
            Self::Trace(input) => input.len(),
            Self::File { len, .. } => *len,
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
        Ok(())
    }
    pub fn range(&self, offset: usize, len: usize) -> Result<Cow<'_, [u8]>> {
        self.check_range(offset, len)?;
        match self {
            Self::Bytes(data) => Ok(Cow::Borrowed(checked::bytes(data, offset, len)?)),
            Self::Trace(input) => input.range(offset, len),
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
    pub fn scan<T>(&self, f: impl FnOnce(&mut dyn ReadSeek) -> Result<T>) -> Result<T> {
        match self {
            Self::Bytes(bytes) => f(&mut Cursor::new(bytes)),
            Self::Trace(input) => input.scan(f),
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
        }
    }
    pub fn report(&self, path: &Path) {
        if std::env::var_os("OXBLEND_READ_REPORT").is_some() {
            eprintln!(
                "OXBLEND_READ {}",
                serde_json::json!({"path_hex":crate::value::hex(path.as_os_str().as_encoded_bytes()), "source":self.stats()})
            );
        }
    }
    pub fn stats(&self) -> serde_json::Value {
        match self {
            Self::Trace(input) => input.stats(),
            _ => serde_json::json!({"mode":"ordinary_backing"}),
        }
    }
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_file(File::open(path).map_err(io)?)
    }
    pub fn open_file(mut file: File) -> Result<Self> {
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
            // O_TMPFILE is a Linux filesystem contract, not a guessed naming scheme
            // or a persistent cache. Failure remains explicit IO; no unsafe mmap.
            let mut decoded = OpenOptions::new()
                .read(true)
                .write(true)
                .mode(0o600)
                .custom_flags(libc::O_TMPFILE)
                .open(std::env::temp_dir())
                .map_err(io)?;
            let source_failed = Cell::new(false);
            let input = BufReader::new(Source {
                file,
                failed: &source_failed,
            });
            let mut destination = Destination {
                file: &mut decoded,
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
                crate::stream_decode::decode_checked(&mut decoder, &mut destination, spill_space)
            };
            result.map_err(|e| {
                if destination.failed || source_failed.get() {
                    Error::new("io", e.message)
                } else {
                    e
                }
            })?;
            decoded
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
