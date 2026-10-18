//! Streaming capture decorates public zstd APIs; it does not implement a codec.
//! RFC8878 frame facts + existing seek admission facts. No physical byte capture.
use super::*;
use crate::fetch::{CaptureSignature, capture_signature};
use std::fs::Metadata;
use std::path::Path;
use zstd::stream::raw::{InBuffer, Operation, OutBuffer, WriteBuf};
#[derive(Debug)]
pub(crate) struct Proof {
    signature: CaptureSignature,
    pub(crate) family: &'static str,
    table: Result<Option<SeekTable>>,
}
impl Proof {
    pub(crate) fn new(
        metadata: &Metadata,
        family: &'static str,
        table: Result<Option<SeekTable>>,
    ) -> Self {
        Self {
            signature: capture_signature(metadata),
            family,
            table,
        }
    }
    pub(crate) fn check_source(&self, file: &File, path: &Path) -> Result<()> {
        if capture_signature(&file.metadata().map_err(io)?) != self.signature
            || capture_signature(&path.metadata().map_err(io)?) != self.signature
        {
            return Err(Error::new(
                "input_changed",
                "source signature changed since snapshot",
            ));
        }
        Ok(())
    }
    pub(crate) fn admit<'a>(&'a self, file: &File, path: &Path) -> Result<Option<&'a SeekTable>> {
        self.check_source(file, path)?;
        // Only Physical::captured(&Snapshot) consumes this verdict; that
        // immutable owner carries its decoded reader, so no external backing
        // can be substituted at admission.
        self.table
            .as_ref()
            .map(|table| table.as_ref())
            .map_err(Clone::clone)
    }
}
// Seek-format field buffers: 8-byte skippable header, 8/12-byte entry, 9-byte footer.
struct TableInput {
    payload: usize,
    width: usize,
    position: usize,
    entry: [u8; 12],
    have: usize,
    index: usize,
    footer: [u8; 9],
}
impl TableInput {
    fn new(payload: usize, frames: usize) -> Result<Self> {
        let width = if add(mul(frames, 8)?, 9)? == payload {
            8
        } else if add(mul(frames, 12)?, 9)? == payload {
            12
        } else {
            0
        };
        Ok(Self {
            payload,
            width,
            position: 0,
            entry: [0; 12],
            have: 0,
            index: 0,
            footer: [0; 9],
        })
    }
    fn consume(&mut self, bytes: &[u8], frames: &mut [Frame]) {
        let entries = self.payload.saturating_sub(9);
        for &byte in bytes {
            if self.position < entries && self.width != 0 {
                self.entry[self.have] = byte;
                self.have += 1;
                if self.have == self.width {
                    if let Some(frame) = frames.get_mut(self.index) {
                        frame.expected = Some(ExpectedEntry {
                            compressed: le(&self.entry[..4]),
                            decoded: le(&self.entry[4..8]),
                            checksum: if self.width == 12 {
                                Some(le(&self.entry[8..12]))
                            } else {
                                None
                            },
                        });
                    }
                    self.index += 1;
                    self.have = 0;
                }
            } else if self.position >= entries && self.position - entries < 9 {
                self.footer[self.position - entries] = byte;
            }
            self.position += 1;
        }
    }
}
struct Current {
    header: [u8; 8],
    have: usize,
    physical: usize,
    length: usize,
    hash: Xxh64,
    table: Option<TableInput>,
}
impl Default for Current {
    fn default() -> Self {
        Self {
            header: [0; 8],
            have: 0,
            physical: 0,
            length: 0,
            hash: Xxh64::new(),
            table: None,
        }
    }
}
struct Builder {
    physical_len: usize,
    physical: usize,
    decoded: usize,
    frames: Vec<Frame>,
    current: Current,
    descriptor: Option<u8>,
    verdict: Result<()>,
}
impl Builder {
    fn new(physical_len: usize) -> Self {
        Self {
            physical_len,
            physical: 0,
            decoded: 0,
            frames: Vec::new(),
            current: Current::default(),
            descriptor: None,
            verdict: Ok(()),
        }
    }
    fn input(&mut self, mut bytes: &[u8]) -> Result<()> {
        self.current.physical = add(self.current.physical, bytes.len())?;
        while !bytes.is_empty() {
            let needed = if self.current.have < 4 {
                4
            } else if le(&self.current.header[..4]) & 0xfffffff0 == 0x184d2a50 {
                8
            } else {
                5
            };
            if self.current.have >= 4 {
                let magic = le(&self.current.header[..4]);
                if magic != ZSTD_MAGIC && magic & 0xfffffff0 != 0x184d2a50 && self.verdict.is_ok() {
                    // libzstd can decode legacy frames; write admission cannot.
                    self.verdict = Err(table_error("not a complete zstd frame"));
                }
            }
            if self.current.have < needed {
                let n = (needed - self.current.have).min(bytes.len());
                self.current.header[self.current.have..self.current.have + n]
                    .copy_from_slice(&bytes[..n]);
                self.current.have += n;
                bytes = &bytes[n..];
                if self.current.have == 8 && le(&self.current.header[..4]) == SEEK_HEADER {
                    self.current.table = Some(TableInput::new(
                        le(&self.current.header[4..8]) as usize,
                        self.frames.len(),
                    )?);
                }
                continue;
            }
            if let Some(table) = &mut self.current.table {
                table.consume(bytes, &mut self.frames);
            }
            break;
        }
        Ok(())
    }
    fn output(&mut self, bytes: &[u8]) -> Result<()> {
        self.current.length = add(self.current.length, bytes.len())?;
        self.current.hash.update(bytes);
        Ok(())
    }
    fn finish_frame(&mut self) -> Result<()> {
        if self.current.physical == 0 {
            return Ok(());
        } // library's empty-input flush, not a frame
        let current = std::mem::take(&mut self.current);
        let end = add(self.physical, current.physical)?;
        let magic = le(&current.header[..4]);
        if magic == SEEK_HEADER {
            let table = current
                .table
                .ok_or_else(|| table_error("short skippable header"))?;
            match footer_descriptor(
                end,
                self.physical_len,
                table.payload,
                &table.footer,
                self.frames.len(),
            ) {
                Ok(descriptor) => self.descriptor = Some(descriptor),
                Err(error) if self.verdict.is_ok() => self.verdict = Err(error),
                Err(_) => {}
            }
        } else {
            let skippable = magic & 0xfffffff0 == 0x184d2a50;
            self.frames.push(Frame {
                physical: self.physical,
                compressed: current.physical,
                decoded: self.decoded,
                length: current.length,
                checksum: current.hash.digest() as u32,
                content_checksum: !skippable && current.header[4] & 4 != 0,
                content_size: !skippable && (current.header[4] & 0xe0 != 0),
                skippable,
                expected: None,
            });
        }
        self.physical = end;
        self.decoded = add(self.decoded, current.length)?;
        Ok(())
    }
    fn finish(mut self, decoded: usize) -> Result<Option<SeekTable>> {
        self.verdict?;
        if self.physical != self.physical_len {
            return Err(table_error("invalid physical frame boundary"));
        }
        if self.decoded != decoded {
            return Err(table_error(
                "frame decoded total differs from serialized body",
            ));
        }
        let Some(descriptor) = self.descriptor else {
            return Ok(None);
        };
        for frame in &mut self.frames {
            let expected = frame
                .expected
                .take()
                .ok_or_else(|| table_error("seek entry count/length does not match real frames"))?;
            check_entry(frame, &expected)?;
        }
        Ok(Some(SeekTable {
            frames: self.frames,
            descriptor,
        }))
    }
}
pub(crate) struct Recording {
    decoder: zstd::stream::raw::Decoder<'static>,
    builder: Builder,
}
impl Recording {
    pub(crate) fn new(length: usize) -> std::io::Result<Self> {
        Ok(Self {
            decoder: zstd::stream::raw::Decoder::new()?,
            builder: Builder::new(length),
        })
    }
    pub(crate) fn take_proof(&mut self, decoded: usize) -> Result<Option<SeekTable>> {
        std::mem::replace(&mut self.builder, Builder::new(0)).finish(decoded)
    }
    fn observe<C: WriteBuf + ?Sized>(
        &mut self,
        output: &mut OutBuffer<'_, C>,
        before: usize,
    ) -> std::io::Result<()> {
        let after = output.pos();
        debug_assert!(before <= after && after <= output.capacity());
        // Safety: zstd-safe WriteBuf::write_from requires its callback to return
        // the initialized byte count; OutBuffer positions expose that count.
        // The public OutBuffer position guarantees initialized bytes up
        // to `after` and <= capacity. The inner decoder has returned, so no writer
        // aliases this borrow; the hash consumes it immediately, retains no slice.
        let bytes =
            unsafe { std::slice::from_raw_parts(output.as_mut_ptr().add(before), after - before) };
        self.builder
            .output(bytes)
            .map_err(|e| std::io::Error::other(e.message))
    }
}
impl Operation for Recording {
    fn run<C: WriteBuf + ?Sized>(
        &mut self,
        input: &mut InBuffer<'_>,
        output: &mut OutBuffer<'_, C>,
    ) -> std::io::Result<usize> {
        let start = input.pos();
        let before = output.pos();
        let remaining = self.decoder.run(input, output)?;
        self.builder
            .input(&input.src[start..input.pos()])
            .map_err(|e| std::io::Error::other(e.message))?;
        self.observe(output, before)?;
        if remaining == 0 {
            self.builder
                .finish_frame()
                .map_err(|e| std::io::Error::other(e.message))?;
        }
        Ok(remaining)
    }
    fn reinit(&mut self) -> std::io::Result<()> {
        self.decoder.reinit()
    }
    fn flush<C: WriteBuf + ?Sized>(
        &mut self,
        output: &mut OutBuffer<'_, C>,
    ) -> std::io::Result<usize> {
        let before = output.pos();
        let n = self.decoder.flush(output)?;
        self.observe(output, before)?;
        Ok(n)
    }
    fn finish<C: WriteBuf + ?Sized>(
        &mut self,
        output: &mut OutBuffer<'_, C>,
        finished: bool,
    ) -> std::io::Result<usize> {
        let before = output.pos();
        let n = self.decoder.finish(output, finished)?;
        self.observe(output, before)?;
        Ok(n)
    }
}
