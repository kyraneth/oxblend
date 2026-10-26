use crate::edit_output::io;
use crate::{Error, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub(crate) fn verify_output(file: &File, family: &str, decoded_length: usize) -> Result<()> {
    let span = crate::diagnostics::Span::new(
        "decode",
        if crate::diagnostics::enabled() {
            serde_json::json!({"format":family,"scope":"output_decoder_verification","decoded_bytes":decoded_length})
        } else {
            serde_json::Value::Null
        },
    );
    let result = (|| {
        let mut clone = file.try_clone().map_err(io)?;
        clone.seek(SeekFrom::Start(0)).map_err(io)?;
        let failed = std::cell::Cell::new(false);
        let source = crate::input::Source {
            file: clone,
            failed: &failed,
        };
        let decode_error = |e: std::io::Error| {
            Error::new(
                if failed.get() {
                    "io"
                } else {
                    "output_compression"
                },
                e.to_string(),
            )
        };
        let mut decoded: Box<dyn Read + '_> = match family {
            "gzip" => Box::new(flate2::read::MultiGzDecoder::new(source)),
            "zstd" => Box::new(zstd::stream::read::Decoder::new(source).map_err(decode_error)?),
            _ => {
                return Err(Error::new(
                    "output_compression",
                    "decoder check is for compressed output",
                ));
            }
        };
        let total = std::io::copy(&mut decoded, &mut std::io::sink()).map_err(decode_error)?;
        if total
            != u64::try_from(decoded_length)
                .map_err(|_| Error::new("overflow", "decoded output length"))?
        {
            return Err(Error::new(
                "output_decode_length",
                "decoder output length differs from immutable decoded body",
            ));
        }
        Ok(())
    })();
    span.result(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::{FileExt, OpenOptionsExt};
    fn staged_file() -> File {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_TMPFILE)
            .open(std::env::temp_dir())
            .unwrap()
    }
    #[test]
    fn upstream_zstd_decoder_rejects_corrupt_frame_checksum_not_our_hash() {
        let mut file = staged_file();
        let body = b"independent upstream decoder checksum proof";
        {
            let mut encoder =
                zstd::stream::write::Encoder::new(&mut file, zstd::DEFAULT_COMPRESSION_LEVEL)
                    .unwrap();
            encoder.include_checksum(true).unwrap();
            encoder.write_all(body).unwrap();
            encoder.finish().unwrap();
        }
        verify_output(&file, "zstd", body.len()).unwrap();
        assert_eq!(
            verify_output(&file, "zstd", body.len() + 1)
                .unwrap_err()
                .kind,
            "output_decode_length"
        );
        let at = file.metadata().unwrap().len() - 1;
        let mut byte = [0];
        file.read_exact_at(&mut byte, at).unwrap();
        byte[0] ^= 1;
        file.write_all_at(&byte, at).unwrap();
        assert_eq!(
            verify_output(&file, "zstd", body.len()).unwrap_err().kind,
            "output_compression"
        );
    }
    #[test]
    fn upstream_gzip_decoder_rejects_corrupt_crc() {
        let mut file = staged_file();
        let body = b"gzip crc is also verified before publication";
        {
            let mut encoder =
                flate2::write::GzEncoder::new(&mut file, flate2::Compression::default());
            encoder.write_all(body).unwrap();
            encoder.finish().unwrap();
        }
        verify_output(&file, "gzip", body.len()).unwrap();
        let at = file.metadata().unwrap().len() - 8;
        let mut byte = [0];
        file.read_exact_at(&mut byte, at).unwrap();
        byte[0] ^= 1;
        file.write_all_at(&byte, at).unwrap();
        assert_eq!(
            verify_output(&file, "gzip", body.len()).unwrap_err().kind,
            "output_compression"
        );
    }
}
