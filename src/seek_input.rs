//! Captured lazy read of an EXISTING zstd seek table. No index sidecars or new format.
//! Footer sizes bound allocations; each touched frame is independently decoded,
//! extent checked, and its present frame/table checksum verified. Untouched pixel/
//! geometry frames are deliberately not certified by the kept-closure build.
#[cfg(test)]
#[path = "seek_prefix_tests.rs"]
mod prefix_tests;
#[cfg(test)]
#[path = "seek_input_tests.rs"]
mod tests;
use crate::{Error, Result, checked};
use std::borrow::Cow;
use std::cell::{Cell, OnceCell};
#[path = "seek_framing.rs"]
mod framing;
#[path = "seek_prefix.rs"]
mod prefix;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::FileExt;
#[derive(Debug)]
pub(crate) struct Frame {
    pub physical: usize,
    pub compressed: usize,
    pub decoded: usize,
    pub length: usize,
    pub checksum: Option<u32>,
    pub skippable: bool,
    data: OnceCell<Vec<u8>>,
    prefix_verified: Cell<bool>,
}
#[derive(Debug)]
pub(crate) struct Input {
    pub file: File,
    pub frames: Vec<Frame>,
    length: usize,
    prefix_bytes: Cell<usize>,
    spill: OnceCell<File>,
    range_requests: Cell<usize>,
    spill_preads: Cell<usize>,
    spill_read_bytes: Cell<usize>,
}
fn error(s: impl Into<String>) -> Error {
    Error::new("seek_table", s)
}
fn le(d: &[u8]) -> u32 {
    u32::from_le_bytes(d.try_into().unwrap())
}
fn bytes(file: &File, offset: usize, length: usize) -> Result<Vec<u8>> {
    let mut out = vec![0; length];
    file.read_exact_at(&mut out, offset as u64)
        .map_err(crate::edit_output::io)?;
    Ok(out)
}
impl Input {
    pub fn open(file: File) -> Result<Option<Self>> {
        let size = usize::try_from(file.metadata().map_err(crate::edit_output::io)?.len())
            .map_err(|_| error("physical size overflow"))?;
        if size < 9 {
            return Ok(None);
        }
        let footer = bytes(&file, size - 9, 9)?;
        if le(&footer[5..9]) != 0x8f92eab1 {
            // Detect a damaged advertised seek footer rather than silently falling back.
            let width = if footer[4] & 0x80 != 0 { 12 } else { 8 };
            let size_claim = checked::add(checked::mul(le(&footer[..4]) as usize, width)?, 17)?;
            if let Some(start) = size.checked_sub(size_claim)
                && le(&bytes(&file, start, 4)?) == 0x184d2a5e
            {
                return Err(error("damaged seek footer magic"));
            }
            return Ok(None);
        }
        let descriptor = footer[4];
        if descriptor & 0x7c != 0 {
            return Err(error("reserved footer bits"));
        }
        let count = le(&footer[..4]) as usize;
        let width = if descriptor & 0x80 != 0 { 12 } else { 8 };
        let payload = checked::add(checked::mul(count, width)?, 9)?;
        let table_size = checked::add(payload, 8)?;
        let start = size
            .checked_sub(table_size)
            .ok_or_else(|| error("table extent beyond source"))?;
        let table = bytes(&file, start, table_size)?;
        if le(&table[..4]) != 0x184d2a5e || le(&table[4..8]) as usize != payload {
            return Err(error("seek table header/extent mismatch"));
        }
        let mut physical = 0;
        let mut decoded = 0;
        let mut frames = Vec::with_capacity(count);
        for i in 0..count {
            let entry = &table[8 + i * width..8 + (i + 1) * width];
            let compressed = le(&entry[..4]) as usize;
            let length = le(&entry[4..8]) as usize;
            let end = checked::add(physical, compressed)?;
            if end > start || compressed < 4 {
                return Err(error("frame extent outside source body"));
            }
            let magic = le(&bytes(&file, physical, 4)?);
            if magic != 0xfd2fb528 && magic & 0xfffffff0 != 0x184d2a50 {
                return Err(error("entry is not a zstd/skippable frame"));
            }
            let skippable = magic & 0xfffffff0 == 0x184d2a50;
            let (_content_checksum, _content_size) = if skippable {
                if compressed < 8
                    || length != 0
                    || le(&bytes(&file, physical + 4, 4)?) as usize != compressed - 8
                {
                    return Err(error("skippable entry extent/decoded length mismatch"));
                }
                (false, false)
            } else {
                if compressed < 5 {
                    return Err(error("short zstd frame header"));
                }
                let flag = bytes(&file, physical + 4, 1)?[0];
                (flag & 4 != 0, flag & 0xe0 != 0)
            };
            frames.push(Frame {
                physical,
                compressed,
                decoded,
                length,
                checksum: (width == 12).then(|| le(&entry[8..12])),
                skippable,
                data: OnceCell::new(),
                prefix_verified: Cell::new(false),
            });
            physical = end;
            decoded = checked::add(decoded, length)?;
        }
        if physical != start {
            return Err(error("entries do not cover exact physical body"));
        }
        Ok(Some(Self {
            file,
            frames,
            length: decoded,
            prefix_bytes: Cell::new(0),
            spill: OnceCell::new(),
            range_requests: Cell::new(0),
            spill_preads: Cell::new(0),
            spill_read_bytes: Cell::new(0),
        }))
    }
    pub fn len(&self) -> usize {
        self.length
    }
    pub fn stats(&self) -> serde_json::Value {
        let decoded = self.frames.iter().filter(|f| f.data.get().is_some());
        serde_json::json!({"mode":"seek_frame_selective", "source_frames_total":self.frames.len(), "source_frames_decoded":decoded.clone().count(), "source_decoded_bytes":decoded.map(|f|f.length).sum::<usize>(), "full_source_decoder_verified":false})
    }
    pub fn frame(&self, i: usize) -> Result<&[u8]> {
        let f = &self.frames[i];
        if let Some(data) = f.data.get() {
            return Ok(data);
        }
        let data = self.decode_frame(i)?;
        let _ = f.data.set(data);
        Ok(f.data.get().unwrap())
    }
    /// Full-proof callers release each frame immediately; do not populate the
    /// selective reader's retained touched-frame cells with the entire file.
    pub fn decode_frame(&self, i: usize) -> Result<Vec<u8>> {
        let mut span = crate::diagnostics::Span::new_lazy("decode.frame", || {
            if crate::diagnostics::enabled() {
                serde_json::json!({"format":"zstd_seekable","frame":i,"scope":"actual_frame"})
            } else {
                serde_json::Value::Null
            }
        });
        let result = (|| {
            let f = &self.frames[i];
            let compressed = bytes(&self.file, f.physical, f.compressed)?;
            let size = zstd::zstd_safe::find_frame_compressed_size(&compressed)
                .map_err(|e| error(format!("invalid frame boundary {e}")))?;
            if size != f.compressed {
                return Err(error(
                    "entry boundary differs from independent zstd frame boundary",
                ));
            }
            let decoder = zstd::stream::read::Decoder::new(compressed.as_slice())
                .map_err(|e| error(e.to_string()))?;
            let bound = checked::add(f.length, 1)?; // External decoded length + one byte detects an oversized frame.
            let mut data = Vec::with_capacity(f.length);
            decoder
                .take(bound as u64)
                .read_to_end(&mut data)
                .map_err(|e| error(e.to_string()))?;
            if data.len() != f.length {
                return Err(error("frame decoded length differs from seek entry"));
            }
            if let Some(checksum) = f.checksum {
                let mut hash = crate::edit_xxh::Xxh64::new();
                hash.update(&data);
                if hash.digest() as u32 != checksum {
                    return Err(error("seek-entry XXH64 checksum mismatch"));
                }
            }
            if crate::diagnostics::enabled() {
                span.data(
                    serde_json::json!({"bytes":data.len(),"input_bytes":f.compressed,"decoded_bytes":data.len(),"physical_bytes":f.compressed}),
                );
            }
            Ok(data)
        })();
        span.result(result)
    }
    pub fn range(&self, offset: usize, length: usize) -> Result<Cow<'_, [u8]>> {
        let end = checked::add(offset, length)?;
        if end > self.length {
            return Err(Error::new("truncated", "captured seek range"));
        }
        if length == 0 {
            return Ok(Cow::Borrowed(&[]));
        }
        let first = self
            .frames
            .partition_point(|f| f.decoded + f.length <= offset);
        let f = self
            .frames
            .get(first)
            .ok_or_else(|| error("range has no frame"))?;
        if end <= f.decoded + f.length {
            return Ok(Cow::Borrowed(
                &self.frame(first)?[offset - f.decoded..end - f.decoded],
            ));
        }
        let mut out = Vec::with_capacity(length);
        let mut position = offset;
        let mut i = first;
        while position < end {
            let f = self.frames.get(i).ok_or_else(|| error("range frame gap"))?;
            let n = (end - position).min(f.decoded + f.length - position);
            out.extend_from_slice(&self.frame(i)?[position - f.decoded..position - f.decoded + n]);
            position += n;
            i += 1;
        }
        Ok(Cow::Owned(out))
    }
    /// A blocks scan owns just its current frame. Unlike selective ranges,
    /// it never fills the invocation's retained touched-frame cells.
    pub fn scan_uncached<T>(
        &self,
        f: impl FnOnce(&mut dyn crate::input::ReadSeek) -> Result<T>,
    ) -> Result<T> {
        let mut cursor = UncachedCursor {
            input: self,
            position: 0,
            frame: None,
            error: None,
        };
        let result = f(&mut cursor);
        if let Some(error) = cursor.error {
            return Err(error);
        }
        result
    }
    pub fn range_uncached(&self, offset: usize, length: usize) -> Result<Vec<u8>> {
        if checked::add(offset, length)? > self.len() {
            return Err(Error::new("truncated", "captured seek range"));
        }
        self.scan_uncached(|reader| {
            crate::input::seek(reader, offset)?;
            let mut bytes = vec![0; length];
            crate::input::read_exact(reader, &mut bytes)?;
            Ok(bytes)
        })
    }
    pub fn scan<T>(
        &self,
        f: impl FnOnce(&mut dyn crate::input::ReadSeek) -> Result<T>,
    ) -> Result<T> {
        let mut cursor = Cursor {
            input: self,
            position: 0,
            error: None,
        };
        let result = f(&mut cursor);
        if let Some(error) = cursor.error {
            return Err(error);
        }
        result
    }
}
struct Cursor<'a> {
    input: &'a Input,
    position: usize,
    error: Option<Error>,
}
impl Read for Cursor<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = out
            .len()
            .min(self.input.length.saturating_sub(self.position));
        let data = match self.input.range(self.position, n) {
            Ok(data) => data,
            Err(error) => {
                let message = error.message.clone();
                self.error = Some(error);
                return Err(std::io::Error::other(message));
            }
        };
        out[..n].copy_from_slice(&data);
        self.position += n;
        Ok(n)
    }
}
/// Position-based frame selection also handles empty/skippable entries and
/// record headers straddling frames. Seeking does not pin skipped frames.
struct UncachedCursor<'a> {
    input: &'a Input,
    position: usize,
    frame: Option<(usize, Vec<u8>)>,
    error: Option<Error>,
}
impl Read for UncachedCursor<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if out.is_empty() || self.position >= self.input.length {
            return Ok(0);
        }
        let index = self
            .input
            .frames
            .partition_point(|f| f.decoded + f.length <= self.position);
        if self.frame.as_ref().is_none_or(|(i, _)| *i != index) {
            // Release before decoding the next frame, not after allocating it.
            self.frame = None;
            match self.input.decode_frame(index) {
                Ok(bytes) => self.frame = Some((index, bytes)),
                Err(error) => {
                    let message = error.message.clone();
                    self.error = Some(error);
                    return Err(std::io::Error::other(message));
                }
            }
        }
        let (_, bytes) = self.frame.as_ref().unwrap();
        let at = self.position - self.input.frames[index].decoded;
        let n = out.len().min(bytes.len() - at);
        out[..n].copy_from_slice(&bytes[at..at + n]);
        self.position += n;
        Ok(n)
    }
}
impl Seek for UncachedCursor<'_> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let position = match to {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(n) => self.position as i128 + n as i128,
            SeekFrom::End(n) => self.input.length as i128 + n as i128,
        };
        self.position = usize::try_from(position)
            .map_err(|_| std::io::Error::other("seek position overflow"))?;
        Ok(self.position as u64)
    }
}

impl Seek for Cursor<'_> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let position = match to {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(n) => self.position as i128 + n as i128,
            SeekFrom::End(n) => self.input.length as i128 + n as i128,
        };
        self.position = usize::try_from(position)
            .map_err(|_| std::io::Error::other("seek position overflow"))?;
        Ok(self.position as u64)
    }
}
