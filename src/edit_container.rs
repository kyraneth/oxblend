//! Container-aware stream emitter. No whole decoded/compressed arena.
use crate::checked::{add, mul};
use crate::edit::{Span, write_spans};
use crate::edit_output::io;
use crate::edit_xxh::Xxh64;
use crate::physical::Physical;
use crate::{BlendFile, Error, Result};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
const SEEK_MAGIC: u32 = 0x8f92eab1;
const SEEK_HEADER: u32 = 0x184d2a5e;
const ZSTD_MAGIC: u32 = 0xfd2fb528;
fn table_error(message: &str) -> Error {
    Error::new("seek_table", message)
}
fn read_at<const N: usize>(source: &Physical<'_>, offset: usize) -> Result<[u8; N]> {
    let mut a = [0; N];
    source
        .read_exact_at(
            &mut a,
            u64::try_from(offset).map_err(|_| table_error("offset overflow"))?,
        )
        .map_err(io)?;
    Ok(a)
}
#[path = "container_capture.rs"]
pub(crate) mod capture;
fn le(data: &[u8]) -> u32 {
    let mut a = [0; 4];
    a.copy_from_slice(&data[..4]);
    u32::from_le_bytes(a)
}
// Both live inspection and streaming capture use these exact admission rulings.
fn footer_descriptor(
    end: usize,
    physical_len: usize,
    payload: usize,
    footer: &[u8; 9],
    frames: usize,
) -> Result<u8> {
    if end != physical_len || payload < 9 {
        return Err(table_error("seek table is not a complete terminal frame"));
    }
    let descriptor = footer[4];
    if le(&footer[5..]) != SEEK_MAGIC || descriptor & 0x7c != 0 {
        return Err(table_error("bad seek footer magic or reserved bits"));
    }
    let width = if descriptor & 0x80 != 0 { 12 } else { 8 };
    if add(mul(le(&footer[..4]) as usize, width)?, 9)? != payload
        || le(&footer[..4]) as usize != frames
    {
        return Err(table_error(
            "seek entry count/length does not match real frames",
        ));
    }
    Ok(descriptor)
}
fn check_entry(frame: &Frame, entry: &ExpectedEntry) -> Result<()> {
    if entry.compressed as usize != frame.compressed || entry.decoded as usize != frame.length {
        return Err(table_error(
            "seek entry sizes do not match independent frame decode",
        ));
    }
    if entry
        .checksum
        .is_some_and(|checksum| checksum != frame.checksum)
    {
        return Err(table_error("seek entry XXH64 checksum mismatch"));
    }
    Ok(())
}
#[derive(Debug)]
struct ExpectedEntry {
    compressed: u32,
    decoded: u32,
    checksum: Option<u32>,
}
#[derive(Debug)]
pub(crate) struct Frame {
    physical: usize,
    compressed: usize,
    decoded: usize,
    length: usize,
    checksum: u32,
    content_checksum: bool,
    content_size: bool,
    skippable: bool,
    expected: Option<ExpectedEntry>,
}
#[derive(Debug)]
pub(crate) struct SeekTable {
    frames: Vec<Frame>,
    descriptor: u8,
}
struct Verify<'a> {
    reader: &'a BlendFile,
    position: usize,
    hash: Xxh64,
    error: Option<Error>,
}
impl Write for Verify<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let result = (|| {
            if self.reader.data.range(self.position, data.len())?.as_ref() != data {
                return Err(Error::new(
                    "input_changed",
                    "frame decoded bytes differ from indexed source",
                ));
            }
            self.position = add(self.position, data.len())?;
            Ok(())
        })();
        if let Err(e) = result {
            let message = e.message.clone();
            self.error = Some(e);
            return Err(std::io::Error::other(message));
        }
        self.hash.update(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Walk physical frames (including skippables), then reconcile every seek entry.
/// This also detects a damaged seek footer rather than downgrading it to nonseek.
/// Streaming decode proves real boundaries/counts; backing comparison proves the
/// same snapshot; hash is streamed rather than allocating a frame-sized arena.
pub(crate) fn inspect(source: &Physical<'_>, reader: &BlendFile) -> Result<Option<SeekTable>> {
    let physical_len = usize::try_from(source.len().map_err(io)?)
        .map_err(|_| table_error("physical length overflow"))?;
    let (mut physical, mut decoded) = (0, 0);
    let mut frames: Vec<Frame> = Vec::new();
    let mut seek = None;
    while physical < physical_len {
        if physical_len - physical < 4 {
            return Err(table_error("trailing incomplete frame magic"));
        }
        let magic = le(&read_at::<4>(source, physical)?);
        if (magic & 0xfffffff0) == 0x184d2a50 {
            if physical_len - physical < 8 {
                return Err(table_error("short skippable header"));
            }
            let payload = le(&read_at::<4>(source, add(physical, 4)?)?) as usize;
            let compressed = add(8, payload)?;
            let end = add(physical, compressed)?;
            if end > physical_len {
                return Err(table_error("skippable frame exceeds file"));
            }
            if magic == SEEK_HEADER {
                if end != physical_len || payload < 9 {
                    return Err(table_error("seek table is not a complete terminal frame"));
                }
                let footer = read_at::<9>(source, end - 9)?;
                let descriptor =
                    footer_descriptor(end, physical_len, payload, &footer, frames.len())?;
                let width = if descriptor & 0x80 != 0 { 12 } else { 8 };
                for (i, frame) in frames.iter().enumerate() {
                    let at = add(add(physical, 8)?, mul(i, width)?)?;
                    let entry = read_at::<8>(source, at)?;
                    let mut expected = ExpectedEntry {
                        compressed: le(&entry[..4]),
                        decoded: le(&entry[4..]),
                        checksum: None,
                    };
                    check_entry(frame, &expected)?;
                    if width == 12 {
                        expected.checksum = Some(le(&read_at::<4>(source, add(at, 8)?)?));
                        check_entry(frame, &expected)?;
                    }
                }
                seek = Some(descriptor);
                physical = end;
                continue;
            }
            let mut hash = Xxh64::new();
            hash.update(&[]);
            frames.push(Frame {
                physical,
                compressed,
                decoded,
                length: 0,
                checksum: hash.digest() as u32,
                content_checksum: false,
                content_size: false,
                skippable: true,
                expected: None,
            });
            physical = end;
            continue;
        }
        if magic != ZSTD_MAGIC || physical_len - physical < 5 {
            return Err(table_error("not a complete zstd frame"));
        }
        let descriptor = read_at::<1>(source, add(physical, 4)?)?[0];
        let failed = std::cell::Cell::new(false);
        let tracked = source.reader(physical as u64, &failed).map_err(io)?;
        let compression_error = |e: std::io::Error| {
            Error::new(
                if failed.get() { "io" } else { "compression" },
                e.to_string(),
            )
        };
        let mut decoder = zstd::stream::read::Decoder::with_buffer(BufReader::new(tracked))
            .map_err(compression_error)?
            .single_frame();
        let mut verify = Verify {
            reader,
            position: decoded,
            hash: Xxh64::new(),
            error: None,
        };
        let result = std::io::copy(&mut decoder, &mut verify);
        if let Some(e) = verify.error {
            return Err(e);
        }
        result.map_err(compression_error)?;
        let end = usize::try_from(decoder.finish().stream_position().map_err(io)?)
            .map_err(|_| table_error("frame end overflow"))?;
        if end <= physical || end > physical_len {
            return Err(table_error("invalid physical frame boundary"));
        }
        let length = verify
            .position
            .checked_sub(decoded)
            .ok_or_else(|| table_error("decoded position"))?;
        frames.push(Frame {
            physical,
            compressed: end - physical,
            decoded,
            length,
            checksum: verify.hash.digest() as u32,
            content_checksum: descriptor & 4 != 0,
            content_size: descriptor & 0xc0 != 0 || descriptor & 0x20 != 0,
            skippable: false,
            expected: None,
        });
        physical = end;
        decoded = verify.position;
    }
    if decoded != reader.decompressed_len() {
        return Err(table_error(
            "frame decoded total differs from serialized body",
        ));
    }
    Ok(seek.map(|descriptor| SeekTable { frames, descriptor }))
}
struct HashWriter<'a, W: Write> {
    writer: &'a mut W,
    hash: Xxh64,
}
impl<W: Write> Write for HashWriter<'_, W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let n = self.writer.write(data)?;
        self.hash.update(&data[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
fn copy_exact(source: &mut dyn Read, output: &mut dyn Write, length: usize) -> Result<()> {
    let n = std::io::copy(&mut source.take(length as u64), output).map_err(io)?;
    if n != length as u64 {
        return Err(Error::new("truncated", "frame source truncated"));
    }
    Ok(())
}
fn frame_body(
    reader: &BlendFile,
    frame: &Frame,
    edits: &[Span],
    output: &mut dyn Write,
) -> Result<()> {
    let end = add(frame.decoded, frame.length)?;
    reader.data.scan(|source| {
        source
            .seek(SeekFrom::Start(frame.decoded as u64))
            .map_err(io)?;
        let mut position = frame.decoded;
        for edit in edits {
            let from = frame.decoded.max(edit.offset);
            let until = end.min(add(edit.offset, edit.bytes.len())?);
            if from >= until {
                continue;
            }
            copy_exact(source, output, from - position)?;
            source
                .seek(SeekFrom::Current(
                    i64::try_from(until - from).map_err(|_| table_error("patch seek overflow"))?,
                ))
                .map_err(io)?;
            output
                .write_all(&edit.bytes[from - edit.offset..until - edit.offset])
                .map_err(io)?;
            position = until;
        }
        copy_exact(source, output, end - position)
    })
}
fn emit_seek(
    source: &Physical<'_>,
    reader: &BlendFile,
    table: &SeekTable,
    edits: &[Span],
    output: &mut File,
    fresh: bool,
) -> Result<()> {
    let mut entries = Vec::new();
    for frame in &table.frames {
        let end = add(frame.decoded, frame.length)?;
        let modified = edits
            .iter()
            .any(|e| e.offset < end && frame.decoded < e.offset.saturating_add(e.bytes.len()));
        let (compressed, checksum) = if (!fresh && !modified) || frame.skippable {
            let failed = std::cell::Cell::new(false);
            let mut input = source.reader(frame.physical as u64, &failed).map_err(io)?;
            copy_exact(&mut input, output, frame.compressed)?;
            (frame.compressed, frame.checksum)
        } else {
            let start = output.stream_position().map_err(io)?;
            let mut encoder =
                zstd::stream::write::Encoder::new(&mut *output, zstd::DEFAULT_COMPRESSION_LEVEL)
                    .map_err(io)?;
            encoder
                .include_checksum(frame.content_checksum)
                .map_err(io)?;
            encoder
                .include_contentsize(frame.content_size)
                .map_err(io)?;
            encoder
                .set_pledged_src_size(Some(frame.length as u64))
                .map_err(io)?;
            let checksum = {
                let mut hashing = HashWriter {
                    writer: &mut encoder,
                    hash: Xxh64::new(),
                };
                frame_body(reader, frame, edits, &mut hashing)?;
                hashing.hash.digest() as u32
            };
            encoder.finish().map_err(io)?;
            (
                usize::try_from(output.stream_position().map_err(io)? - start)
                    .map_err(|_| table_error("encoded frame overflow"))?,
                checksum,
            )
        };
        entries.push((
            u32::try_from(compressed)
                .map_err(|_| table_error("compressed frame exceeds seek entry width"))?,
            u32::try_from(frame.length)
                .map_err(|_| table_error("decoded frame exceeds seek entry width"))?,
            checksum,
        ));
    }
    let width = if table.descriptor & 0x80 != 0 { 12 } else { 8 };
    let payload = u32::try_from(add(mul(entries.len(), width)?, 9)?)
        .map_err(|_| table_error("seek table exceeds format width"))?;
    output
        .write_all(&SEEK_HEADER.to_le_bytes())
        .and_then(|_| output.write_all(&payload.to_le_bytes()))
        .map_err(io)?;
    for (compressed, decoded, checksum) in &entries {
        output
            .write_all(&compressed.to_le_bytes())
            .and_then(|_| output.write_all(&decoded.to_le_bytes()))
            .map_err(io)?;
        if width == 12 {
            output.write_all(&checksum.to_le_bytes()).map_err(io)?;
        }
    }
    output
        .write_all(
            &u32::try_from(entries.len())
                .map_err(|_| table_error("frame count overflow"))?
                .to_le_bytes(),
        )
        .and_then(|_| output.write_all(&[table.descriptor]))
        .and_then(|_| output.write_all(&SEEK_MAGIC.to_le_bytes()))
        .map_err(io)
}
/// Explicit pack compression uses the same seek emitter, not a gzip wrapper.
pub(crate) fn compress_seek(
    source: &Physical<'_>,
    reader: &BlendFile,
    edits: &[Span],
    output: &mut File,
) -> Result<()> {
    let quantum = 1 << 20;
    let frames = (0..reader.decompressed_len())
        .step_by(quantum)
        .map(|decoded| Frame {
            physical: 0,
            compressed: 0,
            decoded,
            length: quantum.min(reader.decompressed_len() - decoded),
            checksum: 0,
            content_checksum: true,
            content_size: true,
            skippable: false,
            expected: None,
        })
        .collect();
    emit_seek(
        source,
        reader,
        &SeekTable {
            frames,
            descriptor: 0x80,
        },
        edits,
        output,
        true,
    )
}
pub(crate) fn emit_spans(
    source: &Physical<'_>,
    reader: &BlendFile,
    family: &str,
    seek: Option<&SeekTable>,
    edits: &[Span],
    output: &mut File,
) -> Result<()> {
    match family {
        "raw" => write_spans(reader, edits, output),
        "gzip" => {
            // Deterministic RFC1952 header: mtime0, OS255, no name/comment/extra; level6.
            let mut encoder = flate2::GzBuilder::new()
                .mtime(0)
                .operating_system(255)
                .write(output, flate2::Compression::default());
            write_spans(reader, edits, &mut encoder)?;
            encoder.finish().map_err(io)?;
            Ok(())
        }
        "zstd" => {
            if let Some(table) = seek {
                emit_seek(source, reader, table, edits, output, false)
            } else {
                let mut encoder =
                    zstd::stream::write::Encoder::new(output, zstd::DEFAULT_COMPRESSION_LEVEL)
                        .map_err(io)?;
                // A newly encoded nonseekable body carries a content checksum;
                // no-op streams remain physically exact. Seekable frames retain
                // their original flags in emit_seek instead of changing them.
                encoder.include_checksum(true).map_err(io)?;
                encoder
                    .set_pledged_src_size(Some(reader.decompressed_len() as u64))
                    .map_err(io)?;
                write_spans(reader, edits, &mut encoder)?;
                encoder.finish().map_err(io)?;
                Ok(())
            }
        }
        _ => Err(Error::new("compression", "unsupported container family")),
    }
}
