//! Header/BHead-first decompression. Limits come from the BLEND grammar only.
//! Do not inflate an arbitrary compressed tail before checking its structure.
use crate::{Error, Result, checked::Cursor};
use std::io::{Read, Write};
fn read(r: &mut impl Read, b: &mut [u8]) -> Result<()> {
    r.read_exact(b).map_err(|e| {
        Error::new(
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                "truncated"
            } else {
                "compression"
            },
            e.to_string(),
        )
    })
}
pub(crate) fn decode(r: &mut impl Read, w: &mut impl Write) -> Result<()> {
    decode_checked(r, w, |_, _| Ok(()))
}
// The spill owner supplies a live filesystem-space check, not a guessed size
// cap. Owned-memory callers retain their separate, uncapped memory contract.
pub(crate) fn decode_checked<W: Write>(
    r: &mut impl Read,
    w: &mut W,
    mut admit: impl FnMut(&mut W, u64) -> Result<()>,
) -> Result<()> {
    let mut prefix = vec![0; 12];
    read(r, &mut prefix)?;
    if prefix.get(7) == Some(&b'1') {
        prefix.resize(17, 0);
        read(r, &mut prefix[12..])?;
    }
    let header = crate::header(&prefix)?;
    admit(w, prefix.len() as u64)?;
    w.write_all(&prefix).map_err(crate::edit_output::io)?;
    let width = if header.file_format == 1 {
        32
    } else {
        16 + header.pointer_size
    };
    loop {
        let mut data = [0; 32];
        read(r, &mut data[..width])?;
        let mut c = Cursor {
            data: &data[..width],
            offset: 0,
            little: header.little_endian,
        };
        let code = c.take(4)?;
        if code.iter().all(|b| *b == 0) {
            return Err(Error::new(
                "invalid_block",
                "zero BHead code is not a native block",
            ));
        }
        let (size, count) = if header.file_format == 1 {
            crate::count(c.uint(4)?, 4)?;
            c.uint(8)?;
            (crate::count(c.uint(8)?, 8)?, crate::count(c.uint(8)?, 8)?)
        } else {
            let size = crate::count(c.uint(4)?, 4)?;
            c.uint(header.pointer_size)?;
            crate::count(c.uint(4)?, 4)?;
            (size, crate::count(c.uint(4)?, 4)?)
        };
        if code == b"ENDB" && (size != 0 || count != 0) {
            return Err(Error::new("invalid_block", "ENDB payload/count"));
        }
        admit(w, crate::checked::add(width, size)? as u64)?;
        w.write_all(&data[..width])
            .map_err(crate::edit_output::io)?;
        if code == b"ENDB" {
            let mut tail = [0];
            if r.read(&mut tail)
                .map_err(|e| Error::new("compression", e.to_string()))?
                != 0
            {
                return Err(Error::new("invalid_block", "decoded data after ENDB"));
            }
            return Ok(());
        }
        let copied = std::io::copy(&mut r.by_ref().take(size as u64), w)
            .map_err(|e| Error::new("compression", e.to_string()))?;
        if copied != size as u64 {
            return Err(Error::new("truncated", "BHead payload"));
        }
    }
}
