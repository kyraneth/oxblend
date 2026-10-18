//! Independent physical framing, without decompression or trusting seek footer arithmetic.
//! Independently authored from the public RFC 8878 wire-format facts:
//! §3.1.1 (magic/checksum), §3.1.1.1 (frame header), §3.1.1.2 (block header),
//! §3.1.2 (skippable frames), https://www.rfc-editor.org/rfc/rfc8878.html.
//! Cross-checked against zstd-sys 1.5.7's ZSTD_frameHeaderSize_internal,
//! ZSTD_getFrameHeader_advanced and ZSTD_getcBlockSize (BSD-3-Clause).
//! No zstd code or RFC code component was copied; bit/width constants are format facts.
//! This is container metadata inspection; the one BLEND/SDNA parser is unchanged.
use super::*;
type PhysicalFrame = (usize, usize, u32);
fn framing(
    file: &File,
    mut visit: impl FnMut(PhysicalFrame) -> Result<()>,
) -> Result<Option<PhysicalFrame>> {
    let size = usize::try_from(file.metadata().map_err(crate::edit_output::io)?.len())
        .map_err(|_| error("physical size overflow"))?;
    let mut at = 0;
    let mut last = None;
    while at < size {
        let start = at;
        let magic = le(&bytes(file, at, 4)?);
        if magic & 0xfffffff0 == 0x184d2a50 {
            let length = le(&bytes(file, checked::add(at, 4)?, 4)?) as usize;
            at = checked::add(at, checked::add(8, length)?)?;
        } else if magic == 0xfd2fb528 {
            let flag = bytes(file, checked::add(at, 4)?, 1)?[0];
            if flag & 8 != 0 {
                return Err(error("reserved zstd frame header bit"));
            }
            let single = flag & 0x20 != 0;
            let dict = [0, 1, 2, 4][(flag & 3) as usize];
            let content = match flag >> 6 {
                0 => usize::from(single),
                1 => 2,
                2 => 4,
                _ => 8,
            };
            at = checked::add(at, checked::add(5 + usize::from(!single) + dict, content)?)?;
            loop {
                let header = bytes(file, at, 3)?;
                let word = u32::from_le_bytes([header[0], header[1], header[2], 0]);
                let kind = (word >> 1) & 3;
                if kind == 3 {
                    return Err(error("reserved zstd block type"));
                }
                let body = if kind == 1 { 1 } else { (word >> 3) as usize };
                at = checked::add(at, checked::add(3, body)?)?;
                if at > size {
                    return Err(error("physical zstd block outside source"));
                }
                if word & 1 != 0 {
                    break;
                }
            }
            if flag & 4 != 0 {
                at = checked::add(at, 4)?;
            }
        } else {
            return Err(error("physical frame magic"));
        }
        if at > size {
            return Err(error("physical frame outside source"));
        }
        let frame = (start, at - start, magic);
        visit(frame)?;
        last = Some(frame);
    }
    Ok(last)
}
impl Input {
    /// Inspection admission never lets zero decoded claims hide stream bytes.
    /// Existing kept-closure/write and blocks admission remain unchanged.
    pub fn open_inspection(file: File) -> Result<Option<Self>> {
        // Detection retains one physical frame, regardless of frame count.
        let last = framing(&file, |_| Ok(()))?;
        let advertised = last.is_some_and(|(_, _, magic)| magic == 0x184d2a5e);
        if advertised {
            let size = file.metadata().map_err(crate::edit_output::io)?.len() as usize;
            if size < 9 || le(&bytes(&file, size - 4, 4)?) != 0x8f92eab1 {
                return Err(error("damaged seek footer magic"));
            }
        }
        let input = Self::open(file)?;
        if let Some(input) = &input {
            // Compare incrementally against the one admitted seek table; no
            // second physical index, and no index at all for nonseekable data.
            let mut entries = input.frames.iter();
            framing(&input.file, |actual @ (at, size, _)| {
                if advertised && Some(actual) == last {
                    return Ok(());
                }
                let f = entries
                    .next()
                    .ok_or_else(|| error("physical frame missing from seek table"))?;
                if f.physical != at || f.compressed != size {
                    return Err(error(
                        "entry boundary differs from independent zstd frame boundary",
                    ));
                }
                Ok(())
            })?;
            if entries.next().is_some() {
                return Err(error(
                    "entry boundary differs from independent zstd frame boundary",
                ));
            }
            for (i, frame) in input.frames.iter().enumerate() {
                if frame.length == 0 && !frame.skippable {
                    input.fill_frame(i)?;
                }
            }
        }
        Ok(input)
    }
}
