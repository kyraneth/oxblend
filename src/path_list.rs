//! Native-byte list input convention shared by dependency and EXR commands.
//! LF by default, NUL with -0; empty entries ignored, all other bytes literal.
use crate::{Error, Result};
use std::{
    ffi::{OsStr, OsString},
    io::Read,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

pub fn inline_path(arg: &OsStr) -> Option<&OsStr> {
    arg.as_bytes()
        .strip_prefix(b"--files-from=")
        .map(OsStr::from_bytes)
}

pub fn read(path: &Path, nul: bool) -> Result<Vec<PathBuf>> {
    let mut span = crate::diagnostics::Span::new(
        "input.paths",
        serde_json::json!({"path_hex":crate::value::hex(path.as_os_str().as_encoded_bytes()),"nul_delimited":nul}),
    );
    let result = (|| {
        let mut data = Vec::new();
        if path == Path::new("-") {
            std::io::stdin().lock().read_to_end(&mut data)
        } else {
            std::fs::File::open(path).and_then(|mut f| f.read_to_end(&mut data))
        }
        .map_err(|e| Error::new("io", format!("read path list '{}': {e}", path.display())))?;
        decode(&data, nul)
    })();
    if let Ok(paths) = &result {
        span.data(serde_json::json!({"entries":paths.len()}));
    }
    span.result(result)
}
pub fn decode(data: &[u8], nul: bool) -> Result<Vec<PathBuf>> {
    if !nul && data.contains(&0) {
        return Err(Error::new(
            "usage",
            "NUL in LF-delimited path list; use -0 for NUL delimiters",
        ));
    }
    Ok(data
        .split(|b| *b == if nul { 0 } else { b'\n' })
        .filter(|p| !p.is_empty())
        .map(|p| PathBuf::from(OsString::from_vec(p.to_vec())))
        .collect())
}
/// Detect a global delimiter flag, but never interpret tokens after `--`.
pub fn nul_delimited(args: &[OsString]) -> bool {
    args.iter().take_while(|a| *a != "--").any(|a| a == "-0")
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;
    #[test]
    fn literal_native_bytes_and_delimiters() {
        let p = decode(b"\n a #\r\n\xff\n", false).unwrap();
        assert_eq!(p[0].as_os_str().as_bytes(), b" a #\r");
        assert_eq!(p[1].as_os_str().as_bytes(), b"\xff");
        assert!(decode(b"a\0b", false).is_err());
        let p = decode(b"\0a\nb\0", true).unwrap();
        assert_eq!(p[0].as_os_str().as_bytes(), b"a\nb");
    }
}
