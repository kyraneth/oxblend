//! Private trace-scoped captured input. no live-reader policy changes.
use crate::input::ReadSeek;
use crate::{Error, Result, checked};
use std::borrow::Cow;
use std::cell::{OnceCell, RefCell};
use std::collections::BTreeMap;
use std::fs::{File, Metadata};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

type Windows = BTreeMap<usize, Vec<u8>>;
#[derive(Debug)]
enum Storage {
    Bytes(Vec<u8>),
    Seekable(crate::seek_input::Input),
    Prefix {
        input: crate::seek_input::Input,
        fallback: OnceCell<Box<crate::input::Backing>>,
        reason: RefCell<Option<String>>,
    },
    BlocksSeekable(crate::seek_input::Input),
    BlocksStream {
        file: File,
        gzip: bool,
        len: usize,
    },
    BlocksRaw {
        file: File,
        len: usize,
    },
    Spill {
        source: File,
        backing: Box<crate::input::Backing>,
        fallback_reason: Option<String>,
    },
    Windows {
        file: File,
        len: usize,
        index: OnceCell<Windows>,
        misses: RefCell<Windows>,
    },
}
#[derive(Debug)]
pub(crate) struct Input {
    path: Option<PathBuf>,
    metadata: Metadata,
    storage: Storage,
}
fn fallback_warning(path: Option<&Path>, reason: &str) {
    eprintln!(
        "{}",
        serde_json::json!({"warning":{"kind":"seek_table_fallback", "message":"untrusted seek table; using checked_full_streaming_spill", "reason":reason}, "path_hex":path.map(|p|crate::value::hex(p.as_os_str().as_encoded_bytes()))})
    );
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
fn insert(windows: &mut Windows, offset: usize, bytes: &[u8]) {
    if let Some(mut last) = windows.last_entry() {
        let end = *last.key() + last.get().len();
        if offset >= *last.key() && offset <= end {
            let overlap = end - offset;
            if overlap < bytes.len() {
                last.get_mut().extend_from_slice(&bytes[overlap..]);
            }
            return;
        }
    }
    windows.insert(offset, bytes.to_vec());
}
fn containing(windows: &Windows, offset: usize) -> Option<(usize, &[u8])> {
    let (&start, bytes) = windows.range(..=offset).next_back()?;
    (offset - start < bytes.len()).then_some((start, bytes))
}
impl Input {
    pub fn open_blocks(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(io)?;
        let metadata = file.metadata().map_err(io)?;
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
                || u32::from_le_bytes(magic) & 0xfffffff0 == 0x184d2a50);
        let storage = if zstd {
            match crate::seek_input::Input::open(file.try_clone().map_err(io)?) {
                // A single data frame could be the whole decoded file. Stream
                // that container instead of materializing it as a frame buffer.
                Ok(Some(input)) if input.frames.iter().filter(|f| f.length != 0).count() > 1 => {
                    Storage::BlocksSeekable(input)
                }
                Ok(_) => Storage::BlocksStream { file, gzip, len: 0 },
                // Blocks historically validated the zstd stream, not seek-table
                // metadata in its skippable tail. Never trust a damaged index:
                // fall back to the same full decoder/error contract, not a
                // weaker selective read. Kept-closure admission stays strict.
                Err(e) if e.kind == "seek_table" => Storage::BlocksStream { file, gzip, len: 0 },
                Err(e) => return Err(e),
            }
        } else if gzip {
            Storage::BlocksStream { file, gzip, len: 0 }
        } else {
            Storage::BlocksRaw {
                len: usize::try_from(metadata.len())
                    .map_err(|_| Error::new("overflow", "backing length"))?,
                file,
            }
        };
        let mut input = Self {
            path: Some(path.to_path_buf()),
            metadata,
            storage,
        };
        input.check()?;
        // Preserve full compressed admission (grammar, extent, trailer and
        // checksums), but discard payloads instead of retaining/spilling them.
        let mut fallback = None;
        let admission = match &mut input.storage {
            Storage::BlocksSeekable(source) => {
                let proof = (|| {
                    // A falsely advertised empty first frame would otherwise
                    // hide the BLENDER header before the scan can validate it.
                    // Validate skipped frames first; malformed optional indexes
                    // then take the same full-stream fallback as other damage.
                    for (i, frame) in source.frames.iter().enumerate() {
                        if frame.length == 0 {
                            source.decode_frame(i)?;
                        }
                    }
                    source.scan_uncached(|mut reader| {
                        spill_checked_decode(&mut reader, &mut std::io::sink())
                    })
                })();
                if proof.as_ref().is_err_and(|e| e.kind == "seek_table") {
                    // An inconsistent frame extent/table hash must not replace
                    // the established full-decoder compression diagnostics.
                    fallback = Some(source.file.try_clone().map_err(io)?);
                    Ok(())
                } else {
                    proof
                }
            }
            Storage::BlocksStream { file, gzip, len } => {
                decoded_length(file, *gzip).map(|length| *len = length)
            }
            _ => Ok(()),
        };
        input.check()?;
        admission?;
        if let Some(file) = fallback {
            let length = decoded_length(&file, false);
            input.check()?;
            let len = length?;
            input.storage = Storage::BlocksStream {
                file,
                gzip: false,
                len,
            };
            input.check()?;
        }
        Ok(input)
    }
    pub fn open_spill(path: &Path, reason: String) -> Result<Self> {
        let source = File::open(path).map_err(io)?;
        let metadata = source.metadata().map_err(io)?;
        fallback_warning(Some(path), &reason);
        let backing = Box::new(crate::input::Backing::open_file(
            source.try_clone().map_err(io)?,
        )?);
        let value = Self {
            path: Some(path.to_path_buf()),
            metadata,
            storage: Storage::Spill {
                source,
                backing,
                fallback_reason: Some(reason),
            },
        };
        value.check()?;
        Ok(value)
    }
    pub fn signature(&self) -> (u64, u64, u64, i64, i64) {
        crate::fetch::signature(&self.metadata)
    }
    pub fn is_prefix(&self) -> bool {
        matches!(self.storage, Storage::Prefix { .. })
    }
    pub fn open_native(path: &Path) -> Result<Self> {
        Self::from_file(
            File::open(path).map_err(io)?,
            Some(path.to_path_buf()),
            false,
            true,
        )
    }
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_profile(path, false)
    }
    fn open_profile(path: &Path, kept_closure: bool) -> Result<Self> {
        Self::from_file(
            File::open(path).map_err(io)?,
            Some(path.to_path_buf()),
            kept_closure,
            false,
        )
    }
    fn from_file(
        file: File,
        path: Option<PathBuf>,
        kept_closure: bool,
        native: bool,
    ) -> Result<Self> {
        let metadata = file.metadata().map_err(io)?;
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
                || u32::from_le_bytes(magic) & 0xfffffff0 == 0x184d2a50);
        let signature = crate::fetch::signature(&metadata);
        let check = || -> Result<()> {
            if crate::fetch::signature(&file.metadata().map_err(io)?) != signature
                || path.as_ref().is_some_and(|p| {
                    p.metadata()
                        .map(|m| crate::fetch::signature(&m) != signature)
                        .unwrap_or(true)
                })
            {
                return Err(Error::new(
                    "input_changed",
                    "trace source signature changed",
                ));
            }
            Ok(())
        };
        check()?;
        let seekable = if zstd && !native {
            let source = file.try_clone().map_err(io)?;
            if kept_closure {
                crate::seek_input::Input::open(source)
            } else {
                crate::seek_input::Input::open_inspection(source)
            }
        } else {
            Ok(None)
        };
        let (seekable, fallback_reason) = match seekable {
            Ok(v) => (v, None),
            Err(e) if !kept_closure && e.kind == "seek_table" => (None, Some(e.message)),
            Err(e) => return Err(e),
        };
        let storage = if native && (gzip || zstd) {
            let bytes = crate::input::decode_owned(file.try_clone().map_err(io)?, gzip)?;
            check()?;
            Storage::Bytes(bytes)
        } else if let Some(input) = seekable {
            if kept_closure {
                Storage::Seekable(input)
            } else {
                Storage::Prefix {
                    input,
                    fallback: OnceCell::new(),
                    reason: RefCell::new(None),
                }
            }
        } else if gzip || zstd {
            if let Some(reason) = &fallback_reason {
                fallback_warning(path.as_deref(), reason);
            }
            let backing = Box::new(crate::input::Backing::open_file(
                file.try_clone().map_err(io)?,
            )?);
            check()?;
            Storage::Spill {
                source: file,
                backing,
                fallback_reason,
            }
        } else {
            Storage::Windows {
                len: usize::try_from(metadata.len())
                    .map_err(|_| Error::new("overflow", "backing length"))?,
                file,
                index: OnceCell::new(),
                misses: RefCell::new(BTreeMap::new()),
            }
        };
        let input = Self {
            path,
            metadata,
            storage,
        };
        input.check()?;
        Ok(input)
    }
    pub fn len(&self) -> usize {
        match &self.storage {
            Storage::Bytes(bytes) => bytes.len(),
            Storage::Seekable(input) | Storage::BlocksSeekable(input) => input.len(),
            Storage::BlocksStream { len, .. } | Storage::BlocksRaw { len, .. } => *len,
            Storage::Prefix {
                input, fallback, ..
            } => fallback.get().map_or_else(|| input.len(), |b| b.len()),
            Storage::Spill { backing, .. } => backing.len(),
            Storage::Windows { len, .. } => *len,
        }
    }
    pub fn stats(&self) -> serde_json::Value {
        match &self.storage {
            Storage::Bytes(bytes) => {
                serde_json::json!({"mode":"native_trace_full_ram", "source_decoded_bytes":bytes.len(), "full_source_decoder_verified":true})
            }
            Storage::Seekable(input) => input.stats(),
            Storage::BlocksSeekable(input) => {
                serde_json::json!({"mode":"blocks_seek_stream", "source_frames_total":input.frames.len(), "source_decoded_bytes":input.len(), "full_source_decoder_verified":true})
            }
            Storage::BlocksStream { len, .. } => {
                serde_json::json!({"mode":"blocks_full_stream", "source_decoded_bytes":len, "full_source_decoder_verified":true})
            }
            Storage::BlocksRaw { .. } => serde_json::json!({"mode":"blocks_raw"}),

            Storage::Prefix {
                input,
                fallback,
                reason,
            } => {
                if let Some(backing) = fallback.get() {
                    serde_json::json!({"mode":"checked_full_streaming_spill", "seek_table":"untrusted → checked_full_streaming_spill", "seek_table_fallback_reason":*reason.borrow(), "source_decoded_bytes":backing.len(), "full_source_decoder_verified":true})
                } else {
                    input.prefix_stats()
                }
            }
            Storage::Spill {
                backing,
                fallback_reason,
                ..
            } => {
                serde_json::json!({"mode":"checked_full_streaming_spill", "seek_table":fallback_reason.as_ref().map(|_| "untrusted → checked_full_streaming_spill"), "seek_table_fallback_reason":fallback_reason, "source_frames_decoded":null, "source_decoded_bytes":backing.len(), "full_source_decoder_verified":true})
            }
            Storage::Windows { .. } => {
                serde_json::json!({"mode":"raw_selective", "source_frames_decoded":0, "source_decoded_bytes":0})
            }
        }
    }
    pub fn check(&self) -> Result<()> {
        if let Some(path) = &self.path {
            let current = path.metadata().map_err(io)?;
            if current.len() < self.metadata.len() {
                return Err(Error::new("truncated", "trace source truncated"));
            }
            if crate::fetch::signature(&current) != crate::fetch::signature(&self.metadata) {
                return Err(Error::new(
                    "input_changed",
                    "trace source signature changed",
                ));
            }
        }
        let file = match &self.storage {
            Storage::Bytes(_) => None,
            Storage::Windows { file, .. }
            | Storage::BlocksRaw { file, .. }
            | Storage::BlocksStream { file, .. } => Some(file),

            Storage::Seekable(input)
            | Storage::BlocksSeekable(input)
            | Storage::Prefix { input, .. } => Some(&input.file),
            Storage::Spill { source, .. } => Some(source),
        };
        if let Some(file) = file {
            let current = file.metadata().map_err(io)?;
            if current.len() < self.metadata.len() {
                return Err(Error::new("truncated", "trace source truncated"));
            }
            if crate::fetch::signature(&current) != crate::fetch::signature(&self.metadata) {
                return Err(Error::new(
                    "input_changed",
                    "trace source descriptor changed",
                ));
            }
        }
        Ok(())
    }
    pub fn range(&self, offset: usize, len: usize) -> Result<Cow<'_, [u8]>> {
        let end = checked::add(offset, len)?;
        if end > self.len() {
            return Err(Error::new("truncated", "range outside captured backing"));
        }
        match &self.storage {
            Storage::Bytes(bytes) => Ok(Cow::Borrowed(checked::bytes(bytes, offset, len)?)),
            Storage::Seekable(input) => {
                self.check()?;
                let result = input.range(offset, len);
                self.check()?;
                result
            }
            Storage::Prefix {
                input,
                fallback,
                reason,
            } => {
                let miss = fallback.get().is_none() && !input.range_ready(offset, len);
                if miss {
                    self.check()?;
                }
                let result = if let Some(backing) = fallback.get() {
                    backing.range(offset, len)
                } else {
                    match input.range_prefix(offset, len) {
                        Err(e) if e.kind == "seek_table" => {
                            fallback_warning(self.path.as_deref(), &e.message);
                            let backing = Box::new(crate::input::Backing::open_file(
                                input.file.try_clone().map_err(io)?,
                            )?);
                            self.check()?;
                            let _ = fallback.set(backing);
                            *reason.borrow_mut() = Some(e.message);
                            fallback.get().unwrap().range(offset, len)
                        }
                        other => other,
                    }
                };
                if miss {
                    self.check()?;
                }
                result
            }
            Storage::BlocksSeekable(input) => {
                self.check()?;
                let result = input.range_uncached(offset, len);
                self.check()?;
                result.map(Cow::Owned)
            }
            Storage::BlocksRaw { file, .. } => {
                self.check()?;
                let mut bytes = vec![0; len];
                file.read_exact_at(&mut bytes, offset as u64).map_err(io)?;
                self.check()?;
                Ok(Cow::Owned(bytes))
            }
            Storage::BlocksStream { .. } => self.scan(|reader| {
                crate::input::seek(reader, offset)?;
                let mut bytes = vec![0; len];
                crate::input::read_exact(reader, &mut bytes)?;
                Ok(Cow::Owned(bytes))
            }),
            Storage::Spill { backing, .. } => backing.range(offset, len),
            Storage::Windows {
                file,
                index,
                misses,
                ..
            } => {
                let empty = BTreeMap::new();
                let windows = index.get().unwrap_or(&empty);
                if let Some((start, bytes)) = index.get().and_then(|w| containing(w, offset))
                    && end - start <= bytes.len()
                {
                    return Ok(Cow::Borrowed(&bytes[offset - start..end - start]));
                }
                let mut captured = misses.borrow_mut();
                let mut value = Vec::with_capacity(len);
                let mut position = offset;
                while position < end {
                    if let Some((start, bytes)) =
                        containing(windows, position).or_else(|| containing(&captured, position))
                    {
                        let take = (end - position).min(bytes.len() - (position - start));
                        value.extend_from_slice(&bytes[position - start..position - start + take]);
                        position += take;
                    } else {
                        let next = windows
                            .range(position..)
                            .next()
                            .map(|(&start, _)| start)
                            .into_iter()
                            .chain(captured.range(position..).next().map(|(&start, _)| start))
                            .min()
                            .unwrap_or(end)
                            .min(end);
                        self.check()?;
                        let mut bytes = vec![0; next - position];
                        file.read_exact_at(
                            &mut bytes,
                            u64::try_from(position)
                                .map_err(|_| Error::new("overflow", "read offset"))?,
                        )
                        .map_err(io)?;
                        self.check()?;
                        value.extend_from_slice(&bytes);
                        // Miss ranges are captured exactly; no guessed coalescing gap.
                        captured.insert(position, bytes);
                        position = next;
                    }
                }
                Ok(Cow::Owned(value))
            }
        }
    }
    pub fn scan<T>(&self, f: impl FnOnce(&mut dyn ReadSeek) -> Result<T>) -> Result<T> {
        match &self.storage {
            Storage::Bytes(bytes) => f(&mut std::io::Cursor::new(bytes)),
            Storage::BlocksRaw { file, .. } => {
                self.check()?;
                let mut reader = BufReader::new(file.try_clone().map_err(io)?);
                reader.seek(SeekFrom::Start(0)).map_err(io)?;
                let result = f(&mut reader);
                self.check()?;
                result
            }
            Storage::BlocksSeekable(input) => {
                self.check()?;
                let result = input.scan_uncached(f);
                self.check()?;
                result
            }
            Storage::BlocksStream { file, gzip, len } => {
                self.check()?;
                let result = stream(file, *gzip, |reader| {
                    f(&mut Forward {
                        reader,
                        position: 0,
                        length: *len,
                    })
                });
                self.check()?;
                result
            }
            Storage::Spill { backing, .. } => {
                self.check()?;
                let result = backing.scan(f);
                self.check()?;
                result
            }
            Storage::Seekable(input) => {
                self.check()?;
                let result = input.scan(f);
                self.check()?;
                result
            }
            Storage::Prefix {
                input, fallback, ..
            } => {
                self.check()?;
                let result = if let Some(backing) = fallback.get() {
                    backing.scan(f)
                } else {
                    input.scan_prefix(f)
                };
                self.check()?;
                result
            }
            Storage::Windows { file, index, .. } => {
                self.check()?;
                let mut reader = BufReader::new(file.try_clone().map_err(io)?);
                reader.seek(SeekFrom::Start(0)).map_err(io)?;
                let mut windows = BTreeMap::new();
                let result = f(&mut Scan {
                    reader,
                    position: 0,
                    windows: &mut windows,
                });
                self.check()?;
                if index.get().is_none() {
                    let _ = index.set(windows);
                }
                result
            }
        }
    }
}
// Full streaming fallback for containers without a seek table. Each scan
// restarts the decoder and forward seeks discard bytes. No decoded tempfile,
// retained windows, or compressed/decoded whole-file buffers are introduced.
fn stream<T>(file: &File, gzip: bool, f: impl FnOnce(&mut dyn Read) -> Result<T>) -> Result<T> {
    let failed = std::cell::Cell::new(false);
    let mut source = file.try_clone().map_err(io)?;
    source.seek(SeekFrom::Start(0)).map_err(io)?;
    let source = BufReader::new(crate::input::Source {
        file: source,
        failed: &failed,
    });
    let result = if gzip {
        f(&mut flate2::read::MultiGzDecoder::new(source))
    } else {
        let mut decoder = zstd::stream::read::Decoder::new(source).map_err(|e| {
            Error::new(
                if failed.get() { "io" } else { "compression" },
                e.to_string(),
            )
        })?;
        f(&mut decoder)
    };
    result.map_err(|e| {
        if failed.get() {
            Error::new("io", e.message)
        } else {
            e
        }
    })
}
/// Blocks admission keeps the spill-space precedence of the materializing
/// reader: a decoded BHead extent that cannot fit the spill filesystem is
/// refused as `spill_space` before any payload copy can report truncation.
/// The decoded stream itself is still discarded byte-by-byte (Count/sink),
/// never retained or spilled, so blocks stays streaming. The available
/// figure is sampled once per admission from the spill filesystem; cumulative
/// decoded extents, including the header, must fit that sampled availability.
fn spill_checked_decode(
    reader: &mut impl std::io::Read,
    writer: &mut impl std::io::Write,
) -> Result<()> {
    let mut available: Option<u64> = None;
    let mut admitted = 0;
    crate::stream_decode::decode_checked(reader, writer, |_, required| {
        let sampled = match available {
            Some(value) => value,
            None => {
                let value = crate::input::temp_spill_available()?;
                available = Some(value);
                value
            }
        };
        crate::input::admit_spill_extent(&mut admitted, required, sampled)
    })
}
fn decoded_length(file: &File, gzip: bool) -> Result<usize> {
    let mut count = Count(0);
    stream(file, gzip, |mut reader| {
        spill_checked_decode(&mut reader, &mut count)
    })?;
    Ok(count.0)
}
struct Count(usize);
impl std::io::Write for Count {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("decoded length overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct Forward<'a> {
    reader: &'a mut dyn Read,
    position: usize,
    length: usize,
}
impl Read for Forward<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = self.reader.read(out)?;
        self.position += n;
        Ok(n)
    }
}
impl Seek for Forward<'_> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let at = match to {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(n) => self.position as i128 + n as i128,
            SeekFrom::End(n) => self.length as i128 + n as i128,
        };
        let at =
            usize::try_from(at).map_err(|_| std::io::Error::other("scan position overflow"))?;
        let skip = at
            .checked_sub(self.position)
            .ok_or_else(|| std::io::Error::other("streaming scan cannot seek backwards"))?;
        let copied = std::io::copy(
            &mut (&mut self.reader).take(skip as u64),
            &mut std::io::sink(),
        )?;
        self.position += copied as usize;
        if copied != skip as u64 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "streaming seek payload",
            ));
        }
        Ok(at as u64)
    }
}
struct Scan<'a> {
    reader: BufReader<File>,
    position: usize,
    windows: &'a mut Windows,
}
impl Read for Scan<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let bytes = self.reader.fill_buf()?;
        insert(self.windows, self.position, bytes);
        let n = output.len().min(bytes.len());
        output[..n].copy_from_slice(&bytes[..n]);
        self.reader.consume(n);
        self.position += n;
        Ok(n)
    }
}
impl Seek for Scan<'_> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let result = self.reader.seek(position)?;
        self.position =
            usize::try_from(result).map_err(|_| std::io::Error::other("scan position overflow"))?;
        Ok(result)
    }
    fn seek_relative(&mut self, offset: i64) -> std::io::Result<()> {
        let position = (self.position as i128) + i128::from(offset);
        let position = usize::try_from(position)
            .map_err(|_| std::io::Error::other("scan position overflow"))?;
        self.reader.seek_relative(offset)?;
        self.position = position;
        Ok(())
    }
}
