//! Narrow public-constructor safety checks; CLI coverage also lives in malformed_input.rs.
use oxblend::BlendFile;

#[test]
fn incomplete_headers_return_errors() {
    for header in [b"BLENDER-v405".as_slice(), b"BLENDER17-01v0500".as_slice()] {
        for end in 0..header.len() {
            assert!(BlendFile::from_bytes(header[..end].to_vec()).is_err());
        }
        // Header alone is not a complete file, even when structurally legal.
        assert!(BlendFile::from_bytes(header.to_vec()).is_err());
    }
}

#[test]
fn unsupported_markers_are_not_guessed() {
    for input in [
        b"BLENDER-x405".as_slice(),
        b"BLENDER17-02v0500".as_slice(),
        b"BLENDER17-01V0500".as_slice(),
    ] {
        assert!(BlendFile::from_bytes(input.to_vec()).is_err());
    }
}
