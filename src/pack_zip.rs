use crate::edit_output::io;
use crate::{Error, Result};
use flate2::{Compression, Crc, write::DeflateEncoder};
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(crate) struct Workspace(pub PathBuf);
impl Workspace {
    /// ZIP staging lives under the operation-frozen temp root, never a
    /// re-sampled process temp.
    pub(crate) fn create_in(temp_root: &Path) -> Result<Self> {
        loop {
            let path = temp_root.join(format!(
                "oxblend-zip-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(crate::edit_output::io_at("create ZIP workspace", &path, e)),
            }
        }
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Checksummed<R> {
    reader: R,
    crc: Crc,
}
impl<R: Read> Read for Checksummed<R> {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        let n = self.reader.read(b)?;
        self.crc.update(&b[..n]);
        Ok(n)
    }
}
fn u16v(o: &mut File, n: u16) -> Result<()> {
    o.write_all(&n.to_le_bytes()).map_err(io)
}
fn u32v(o: &mut File, n: u32) -> Result<()> {
    o.write_all(&n.to_le_bytes()).map_err(io)
}
fn u64v(o: &mut File, n: u64) -> Result<()> {
    o.write_all(&n.to_le_bytes()).map_err(io)
}
struct Entry {
    name: Vec<u8>,
    offset: u64,
    compressed: u64,
    size: u64,
    crc: u32,
    method: u16,
}

fn storage_extension(name: &str) -> &str {
    match name.rfind('.') {
        Some(at) => &name[at..],
        None => "",
    }
}
fn storage_method(relative: &Path, path: &Path) -> u16 {
    let _ = path;
    let name = relative
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if name == "bat-debug.log" {
        return 0;
    }
    let lowered = name.to_ascii_lowercase();
    let extension = storage_extension(&lowered);
    if matches!(extension, ".jpg" | ".jpeg" | ".exr") {
        return 0;
    }
    8
}

pub(crate) fn emit(directory: &Path, paths: &[PathBuf], output: &mut File) -> Result<()> {
    emit_inner(directory, paths, output)
        .map_err(|e| crate::edit_output::context(e, "emit ZIP from workspace", directory))
}
type Source<'a> = (Vec<u8>, u16, u64, Box<dyn Read + 'a>);
/// Serialize already-verified immutable bodies without reopening staged paths.
/// Reporting shares the native ZIP64 writer; there is no second archive format.
pub(crate) fn emit_bytes(members: &[(String, Vec<u8>)], output: &mut File) -> Result<()> {
    let sources = members.iter().map(|(name, bytes)| {
        let relative = Path::new(name);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(Error::new("zip_path", "unsafe ZIP member"));
        }
        Ok((
            name.as_bytes().to_vec(),
            storage_method(relative, relative),
            bytes.len() as u64,
            Box::new(std::io::Cursor::new(bytes.as_slice())) as Box<dyn Read + '_>,
        ))
    });
    emit_sources(sources, output)
}
fn emit_inner(directory: &Path, paths: &[PathBuf], output: &mut File) -> Result<()> {
    let sources = paths.iter().map(|path| {
        let relative = path
            .strip_prefix(directory)
            .map_err(|_| Error::new("zip_path", "member outside workspace"))?;
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(Error::new("zip_path", "unsafe ZIP member"));
        }
        let name = relative
            .to_str()
            .ok_or_else(|| Error::new("zip_encoding", "ZIP member must be UTF8"))?
            .as_bytes()
            .to_vec();
        let method = storage_method(relative, path);
        let file =
            File::open(path).map_err(|e| crate::edit_output::io_at("open ZIP member", path, e))?;
        let size = file
            .metadata()
            .map_err(|e| crate::edit_output::io_at("fstat ZIP member", path, e))?
            .len();
        Ok((name, method, size, Box::new(file) as Box<dyn Read>))
    });
    emit_sources(sources, output)
}
fn emit_sources<'a>(
    sources: impl Iterator<Item = Result<Source<'a>>>,
    output: &mut File,
) -> Result<()> {
    let mut entries = Vec::new();
    for source in sources {
        let (name, method, size, file) = source?;
        let name_len = u16::try_from(name.len())
            .map_err(|_| Error::new("zip_width", "member name exceeds ZIP u16 width"))?;
        let offset = output.stream_position().map_err(io)?;
        u32v(output, 0x04034b50)?;
        u16v(output, 45)?;
        u16v(output, 0x808)?;
        u16v(output, method)?;
        u16v(output, 0)?;
        u16v(output, 33)?;
        u32v(output, 0)?;
        u32v(output, u32::MAX)?;
        u32v(output, u32::MAX)?;
        u16v(output, name_len)?;
        u16v(output, 20)?;
        output.write_all(&name).map_err(io)?;
        u16v(output, 1)?;
        u16v(output, 16)?;
        u64v(output, size)?;
        u64v(output, 0)?;
        let start = output.stream_position().map_err(io)?;
        let mut input = Checksummed {
            reader: file,
            crc: Crc::new(),
        };
        let copied = if method == 0 {
            std::io::copy(&mut input, output).map_err(io)?
        } else {
            let mut encoder = DeflateEncoder::new(&mut *output, Compression::default());
            let n = std::io::copy(&mut input, &mut encoder).map_err(io)?;
            encoder.finish().map_err(io)?;
            n
        };
        if copied != size {
            return Err(Error::new(
                "input_changed",
                "ZIP staged member size changed",
            ));
        }
        let compressed = output.stream_position().map_err(io)? - start;
        let crc = input.crc.sum();
        u32v(output, 0x08074b50)?;
        u32v(output, crc)?;
        u64v(output, compressed)?;
        u64v(output, size)?;
        entries.push(Entry {
            name,
            offset,
            compressed,
            size,
            crc,
            method,
        });
    }
    let central = output.stream_position().map_err(io)?;
    for e in &entries {
        u32v(output, 0x02014b50)?;
        u16v(output, 0x32d)?;
        u16v(output, 45)?;
        u16v(output, 0x808)?;
        u16v(output, e.method)?;
        u16v(output, 0)?;
        u16v(output, 33)?;
        u32v(output, e.crc)?;
        u32v(output, u32::MAX)?;
        u32v(output, u32::MAX)?;
        u16v(output, e.name.len() as u16)?;
        u16v(output, 28)?;
        u16v(output, 0)?;
        u16v(output, 0)?;
        u16v(output, 0)?;
        u32v(output, 0o100644 << 16)?;
        u32v(output, u32::MAX)?;
        output.write_all(&e.name).map_err(io)?;
        u16v(output, 1)?;
        u16v(output, 24)?;
        u64v(output, e.size)?;
        u64v(output, e.compressed)?;
        u64v(output, e.offset)?;
    }
    let end = output.stream_position().map_err(io)?;
    let count = entries.len() as u64;
    u32v(output, 0x06064b50)?;
    u64v(output, 44)?;
    u16v(output, 45)?;
    u16v(output, 45)?;
    u32v(output, 0)?;
    u32v(output, 0)?;
    u64v(output, count)?;
    u64v(output, count)?;
    u64v(output, end - central)?;
    u64v(output, central)?;
    u32v(output, 0x07064b50)?;
    u32v(output, 0)?;
    u64v(output, end)?;
    u32v(output, 1)?;
    u32v(output, 0x06054b50)?;
    u16v(output, 0)?;
    u16v(output, 0)?;
    u16v(output, u16::MAX)?;
    u16v(output, u16::MAX)?;
    u32v(output, u32::MAX)?;
    u32v(output, u32::MAX)?;
    u16v(output, 0)
}
