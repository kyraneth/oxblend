//! Structure only, derived from OpenEXR File Layout (openexr.com).
//! Offset tables and fixed chunk headers are read; compressed samples are not.
use super::*;
use serde_json::{Value, json};
use std::os::unix::fs::FileExt;

fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b)
        .ok_or_else(|| err("EXR structure size overflow"))
}
fn mul(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b)
        .ok_or_else(|| err("EXR structure size overflow"))
}
fn attr<'a>(p: &'a Part, name: &[u8], kind: &[u8]) -> Result<&'a [u8]> {
    let a = p
        .attributes
        .iter()
        .find(|a| a.name == name && a.kind == kind)
        .ok_or_else(|| {
            err(&format!(
                "expected {} attribute",
                crate::value::display_bytes(name)
            ))
        })?;
    p.values
        .get(&a.name)
        .map(Vec::as_slice)
        .ok_or_else(|| err("missing attribute payload"))
}
fn integer(p: &Part, name: &[u8]) -> Result<i32> {
    i32le(attr(p, name, b"int")?)
}
fn window(p: &Part) -> Result<[i32; 4]> {
    let b = attr(p, b"dataWindow", b"box2i")?;
    if b.len() != 16 {
        return Err(err("invalid data window length"));
    }
    Ok([
        i32le(&b[..4])?,
        i32le(&b[4..8])?,
        i32le(&b[8..12])?,
        i32le(&b[12..])?,
    ])
}
pub(super) fn deep(p: &Part) -> bool {
    p.values
        .get(b"type".as_slice())
        .is_some_and(|v| matches!(v.as_slice(), b"deepscanline" | b"deeptile"))
}
pub(super) fn tiled(p: &Part, version: u32) -> bool {
    version & 0x200 != 0
        || p.values
            .get(b"type".as_slice())
            .is_some_and(|v| matches!(v.as_slice(), b"tiledimage" | b"deeptile"))
}
pub(super) fn placeholder(p: &Part) -> bool {
    window(p).is_ok_and(|w| w[0] == w[2] && w[1] == w[3])
}
#[derive(Debug)]
struct Layout {
    count: u64,
    window: [i32; 4],
    lines: u64,
    // (level x, level y, tiles across, tiles down).
    levels: Vec<(i32, i32, u64, u64)>,
}
// Technical Introduction, "Level numbers, level size and rounding mode":
// max(1, rf(width / 2^level)), similarly for height, independent of origin.
fn sizes(mut n: u64, up: bool) -> Vec<u64> {
    let mut out = vec![n];
    while n > 1 {
        n = if up { n.div_ceil(2) } else { n / 2 };
        out.push(n);
    }
    out
}
fn layout(p: &Part, version: u32) -> Result<Layout> {
    let multipart = version & 0x1000 != 0;
    if multipart {
        if attr(p, b"name", b"string")?.is_empty() {
            return Err(err("multipart part name must not be empty"));
        }
        attr(p, b"type", b"string")?;
        if version & 0x200 != 0 {
            return Err(err(
                "multipart files must not set the single-part tile flag",
            ));
        }
    }
    if p.values.contains_key(b"type".as_slice()) {
        let kind = attr(p, b"type", b"string")?;
        if !matches!(kind, b"scanlineimage" | b"tiledimage") {
            return Err(Error::new("exr_unsupported", "unsupported EXR part type"));
        }
        if !multipart && (kind == b"tiledimage") != (version & 0x200 != 0) {
            return Err(err("part type disagrees with single-part tile flag"));
        }
    }
    let w = window(p)?;
    let width = i64::from(w[2]) - i64::from(w[0]) + 1;
    let height = i64::from(w[3]) - i64::from(w[1]) + 1;
    if width <= 0 || height <= 0 {
        return Err(err("data window has nonpositive dimensions"));
    }
    let compression = attr(p, b"compression", b"compression")?;
    // OpenEXR File Layout: scan lines per block. Technical Introduction also
    // specifies LJ2K=256 and ZSTD=1; codes 12/13 match official OpenEXR 3.5.2.
    let lines = *compression
        .first()
        .and_then(|c| [1, 1, 1, 16, 32, 16, 32, 32, 32, 256, 256, 32, 256, 1].get(*c as usize))
        .ok_or_else(|| Error::new("exr_unsupported", "unknown EXR compression code"))?;
    let mut l = Layout {
        count: 0,
        window: w,
        lines,
        levels: Vec::new(),
    };
    if tiled(p, version) {
        let b = attr(p, b"tiles", b"tiledesc")?;
        if b.len() != 9 {
            return Err(err("tile description must contain nine bytes"));
        }
        let tx = u32::from_le_bytes(b[..4].try_into().unwrap()) as u64;
        let ty = u32::from_le_bytes(b[4..8].try_into().unwrap()) as u64;
        let mode = b[8] & 15;
        if tx == 0 || ty == 0 || mode > 2 || b[8] >> 4 > 1 {
            return Err(err("invalid tile dimensions or level mode"));
        }
        let xs = sizes(width as u64, b[8] >> 4 == 1);
        let ys = sizes(height as u64, b[8] >> 4 == 1);
        let mut level = |x: usize, y: usize| -> Result<()> {
            let nx = xs.get(x).copied().unwrap_or(1).div_ceil(tx);
            let ny = ys.get(y).copied().unwrap_or(1).div_ceil(ty);
            l.count = add(l.count, mul(nx, ny)?)?;
            l.levels.push((x as i32, y as i32, nx, ny));
            Ok(())
        };
        match mode {
            0 => level(0, 0)?,
            1 => {
                for i in 0..xs.len().max(ys.len()) {
                    level(i, i)?;
                }
            }
            2 => {
                for y in 0..ys.len() {
                    for x in 0..xs.len() {
                        level(x, y)?;
                    }
                }
            }
            _ => unreachable!(),
        }
    } else {
        l.count = (height as u64).div_ceil(lines);
    }
    if version & 0x1000 != 0 || p.values.contains_key(b"chunkCount".as_slice()) {
        let declared = integer(p, b"chunkCount")?;
        if declared < 0 || declared as u64 != l.count {
            return Err(err(
                "chunkCount disagrees with data window/compression/tile geometry",
            ));
        }
    }
    Ok(l)
}

pub(super) fn inspect(file: &File, header: &ExrHeader) -> Result<Value> {
    if header.version & 0x800 != 0 || header.parts.iter().any(deep) {
        // Do not use flat-image size rules or claim truncation for deep storage.
        let declared = header
            .parts
            .iter()
            .map(|p| {
                integer(p, b"chunkCount")
                    .ok()
                    .filter(|n| *n >= 0)
                    .map(|n| n as u64)
            })
            .collect::<Option<Vec<_>>>();
        return Ok(
            json!({"status":"deep_unsupported","chunks_declared":declared.and_then(|v|v.into_iter().try_fold(0u64,|a,b|a.checked_add(b))),"chunks_present":null,"bytes_expected":null,"truncated":null}),
        );
    }
    let layouts = header
        .parts
        .iter()
        .map(|p| layout(p, header.version))
        .collect::<Result<Vec<_>>>()?;
    let count = layouts.iter().try_fold(0, |a, l| add(a, l.count))?;
    let table_end = add(header.header_end, mul(count, 8)?)?;
    // Lower bound: the complete table plus each mandatory fixed chunk header.
    // Packed payload sizes strengthen it when readable; missing payload sizes
    // cannot be inferred from compression or uncompressed dimensions.
    let mut expected = layouts.iter().try_fold(table_end, |n, l| {
        add(
            n,
            mul(
                l.count,
                (if l.levels.is_empty() { 8 } else { 20 })
                    + if header.version & 0x1000 != 0 { 4 } else { 0 },
            )?,
        )
    })?;
    if table_end > header.file_size {
        // No offset can point after this incomplete table and still lie inside
        // the file. Do not allocate/read the remaining file as a guessed table.
        return Ok(
            json!({"status":"header_only_no_chunks","chunks_declared":count,
            "chunks_present":0,"bytes_expected":expected,"truncated":true}),
        );
    }
    let mut table = header.header_end;
    let multipart = header.version & 0x1000 != 0;
    let mut present = 0u64;
    let mut complete = table_end <= header.file_size;
    let mut ranges = Vec::new();
    for (part, l) in layouts.iter().enumerate() {
        let tiled = !l.levels.is_empty();
        let head_size = (if tiled { 20 } else { 8 }) + if multipart { 4 } else { 0 };
        let mut source = file;
        source.seek(SeekFrom::Start(table)).map_err(io)?;
        // Take bounds standard buffered read-ahead to this table, not pixels.
        // Chunk headers use positional IO, so they do not disturb this cursor.
        let mut offsets = BufReader::new(source.take(mul(l.count, 8)?));
        let mut coordinates = BTreeSet::new();
        for index in 0..l.count {
            let mut b = [0; 8];
            offsets.read_exact(&mut b).map_err(io)?;
            let offset = u64::from_le_bytes(b);
            if offset < table_end || add(offset, head_size)? > header.file_size {
                complete = false;
                continue;
            }
            let mut chunk = [0u8; 24];
            file.read_exact_at(&mut chunk[..head_size as usize], offset)
                .map_err(io)?;
            let skip = if multipart { 4 } else { 0 };
            if multipart && i32le(&chunk[..4])? != part as i32 {
                complete = false;
                continue;
            }
            let c = &chunk[skip..head_size as usize];
            let valid = if tiled {
                let x = i32le(&c[..4])?;
                let y = i32le(&c[4..8])?;
                let lx = i32le(&c[8..12])?;
                let ly = i32le(&c[12..16])?;
                coordinates.insert((x, y, lx, ly))
                    && l.levels.iter().any(|&(a, b, nx, ny)| {
                        a == lx && b == ly && x >= 0 && y >= 0 && (x as u64) < nx && (y as u64) < ny
                    })
            } else {
                i64::from(i32le(&c[..4])?)
                    == i64::from(l.window[1])
                        + i64::try_from(mul(index, l.lines)?)
                            .map_err(|_| err("scanline coordinate overflow"))?
            };
            let packed = i32le(&c[c.len() - 4..])?;
            if !valid || packed < 0 {
                complete = false;
                continue;
            }
            present += 1;
            let end = add(add(offset, head_size)?, packed as u64)?;
            expected = expected.max(end);
            if end > header.file_size {
                complete = false;
            }
            ranges.push((offset, end));
        }
        table = add(table, mul(l.count, 8)?)?;
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|w| w[0].1 > w[1].0) {
        complete = false;
    }
    let status = if complete && present == count {
        "ok"
    } else if present == 0 {
        "header_only_no_chunks"
    } else {
        "chunks_truncated"
    };
    Ok(
        json!({"status":status,"chunks_declared":count,"chunks_present":present,"bytes_expected":expected,"truncated":!complete}),
    )
}
