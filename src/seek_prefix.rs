//! Inspection: one decoder, each touched frame streamed once into an unnamed spill.
//! Each touched frame is decoded once; untouched frames remain unread.
use super::*;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
struct FrameSource {
    file: File,
    position: u64,
    remaining: u64,
}
impl Read for FrameSource {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = out
            .len()
            .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
        let n = self.file.read_at(&mut out[..n], self.position)?;
        self.position += n as u64;
        self.remaining -= n as u64;
        Ok(n)
    }
}
struct FrameSpill<'a> {
    file: &'a File,
    position: u64,
    hash: crate::edit_xxh::Xxh64,
}
impl Write for FrameSpill<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write_at(bytes, self.position)?;
        self.hash.update(&bytes[..n]);
        self.position += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Input {
    fn spill(&self) -> Result<&File> {
        if self.spill.get().is_none() {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .mode(0o600)
                .custom_flags(libc::O_TMPFILE)
                .open(std::env::temp_dir())
                .map_err(crate::edit_output::io)?;
            let _ = self.spill.set(file);
        }
        Ok(self.spill.get().unwrap())
    }
    pub(super) fn fill_frame(&self, i: usize) -> Result<()> {
        let frame = &self.frames[i];
        if frame.prefix_verified.get() {
            return Ok(());
        }
        let spill = self.spill()?;
        let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::fstatvfs(spill.as_raw_fd(), stats.as_mut_ptr()) } != 0 {
            return Err(crate::edit_output::io(std::io::Error::last_os_error()));
        }
        let stats = unsafe { stats.assume_init() };
        let available = stats
            .f_bavail
            .checked_mul(stats.f_frsize)
            .ok_or_else(|| Error::new("overflow", "spill filesystem available bytes"))?;
        crate::input::require_spill_space(frame.length as u64, available)?;
        let source = FrameSource {
            file: self.file.try_clone().map_err(crate::edit_output::io)?,
            position: frame.physical as u64,
            remaining: frame.compressed as u64,
        };
        let mut decoder = zstd::stream::read::Decoder::new(source)
            .map_err(|e| error(e.to_string()))?
            .single_frame();
        let mut writer = FrameSpill {
            file: spill,
            position: frame.decoded as u64,
            hash: crate::edit_xxh::Xxh64::new(),
        };
        let copied = std::io::copy(
            &mut (&mut decoder).take(checked::add(frame.length, 1)? as u64),
            &mut writer,
        )
        .map_err(|e| error(e.to_string()))? as usize;
        if copied != frame.length {
            return Err(error("frame decoded length differs from seek entry"));
        }
        let source = decoder.finish();
        if source.get_ref().remaining + source.buffer().len() as u64 != 0 {
            return Err(error(
                "entry boundary differs from independent zstd frame boundary",
            ));
        }
        if frame
            .checksum
            .is_some_and(|h| h != writer.hash.digest() as u32)
        {
            return Err(error("seek-entry XXH64 checksum mismatch"));
        }
        frame.prefix_verified.set(true);
        self.prefix_bytes
            .set(checked::add(self.prefix_bytes.get(), copied)?);
        Ok(())
    }
    pub fn range_ready(&self, offset: usize, length: usize) -> bool {
        let Some(end) = offset.checked_add(length) else {
            return false;
        };
        let first = self
            .frames
            .partition_point(|f| f.decoded + f.length <= offset);
        end <= self.length
            && self.frames[first..]
                .iter()
                .take_while(|f| f.decoded < end)
                .all(|f| f.prefix_verified.get())
    }
    pub fn range_prefix(&self, offset: usize, length: usize) -> Result<Cow<'_, [u8]>> {
        self.range_requests
            .set(checked::add(self.range_requests.get(), 1)?);
        let end = checked::add(offset, length)?;
        if end > self.length {
            return Err(Error::new("truncated", "captured seek range"));
        }
        let mut position = offset;
        while position < end {
            let i = self
                .frames
                .partition_point(|f| f.decoded + f.length <= position);
            let frame = self
                .frames
                .get(i)
                .ok_or_else(|| error("range has no frame"))?;
            self.fill_frame(i)?;
            position = end.min(frame.decoded + frame.length);
        }
        let mut out = vec![0; length];
        if length != 0 {
            self.spill_preads
                .set(checked::add(self.spill_preads.get(), 1)?);
            self.spill_read_bytes
                .set(checked::add(self.spill_read_bytes.get(), length)?);
            self.spill()?
                .read_exact_at(&mut out, offset as u64)
                .map_err(crate::edit_output::io)?;
        }
        Ok(Cow::Owned(out))
    }
    pub fn scan_prefix<T>(
        &self,
        f: impl FnOnce(&mut dyn crate::input::ReadSeek) -> Result<T>,
    ) -> Result<T> {
        let mut cursor = PrefixCursor {
            input: self,
            position: 0,
            error: None,
        };
        let result = f(&mut cursor);
        if let Some(e) = cursor.error {
            return Err(e);
        }
        result
    }
    pub fn prefix_stats(&self) -> serde_json::Value {
        let verified = self
            .frames
            .iter()
            .filter(|f| f.prefix_verified.get())
            .count();
        serde_json::json!({"mode":"seek_frame_spill_selective", "range_requests":self.range_requests.get(), "spill_preads":self.spill_preads.get(), "spill_read_bytes":self.spill_read_bytes.get(), "range_allocations":self.range_requests.get(), "source_frames_total":self.frames.len(), "verified_frames":verified, "skipped_frames":self.frames.len()-verified, "decode_work_bytes":self.prefix_bytes.get(), "spill_bytes_written":self.prefix_bytes.get(), "source_prefix_bytes":0, "source_full_frames_decoded":verified, "source_full_frame_bytes":self.prefix_bytes.get(), "full_source_decoder_verified":false})
    }
}
struct PrefixCursor<'a> {
    input: &'a Input,
    position: usize,
    error: Option<Error>,
}
impl Read for PrefixCursor<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = out
            .len()
            .min(self.input.len().saturating_sub(self.position));
        match self.input.range_prefix(self.position, n) {
            Ok(bytes) => {
                out[..n].copy_from_slice(&bytes);
                self.position += n;
                Ok(n)
            }
            Err(e) => {
                let message = e.message.clone();
                self.error = Some(e);
                Err(std::io::Error::other(message))
            }
        }
    }
}
impl Seek for PrefixCursor<'_> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let n = match to {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(n) => self.position as i128 + n as i128,
            SeekFrom::End(n) => self.input.len() as i128 + n as i128,
        };
        self.position =
            usize::try_from(n).map_err(|_| std::io::Error::other("seek position overflow"))?;
        Ok(self.position as u64)
    }
}
