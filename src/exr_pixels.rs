//! Explicit named-pass pixels. Header/structure facts remain owned by our reader;
//! the `exr` crate alone owns compression. No first-pass or RGB fallback.
use crate::{Error, Result};
use exr::block::reader::ChunksReader;
use exr::prelude::{SampleType, f16};
use serde_json::{Value, json};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

const CODEC: &str = "exr-1.74.2";
fn validate_block_selection(index: &exr::block::BlockIndex, part: usize) -> exr::error::UnitResult {
    if index.layer != part || index.level != exr::math::Vec2(0, 0) {
        return Err(exr::error::Error::Invalid(
            "decoded block differs from selected part/base resolution".into(),
        ));
    }
    Ok(())
}
fn decode_error(message: impl Into<String>) -> Error {
    Error::new("exr_decode", message)
}
fn codec_error(error: exr::error::Error) -> Error {
    match error {
        exr::error::Error::NotSupported(message) => {
            Error::new("exr_unsupported", message.into_owned())
        }
        exr::error::Error::Io(error) => crate::edit_output::io(error),
        other => decode_error(other.to_string()),
    }
}
/// Base-resolution, full-sampled floating channels, in on-disk channel order.
/// Pixels are interleaved, x increasing then y increasing from data_window min.
/// Finite half values widen exactly to f32; signaling half NaNs may quiet.
/// Existing f32 bits (including signaling NaNs) are retained.
#[derive(Debug)]
#[non_exhaustive]
pub struct Pixels {
    pub width: usize,
    pub height: usize,
    pub data_window: [i32; 4],
    pub channels: Vec<String>,
    pub samples: Vec<f32>,
}
/// Read exactly the requested part and pass prefix (e.g. `ViewLayer.Combined`).
/// Empty pass explicitly selects unprefixed channels. No colour conversion,
/// alpha synthesis, display-window padding, deep flattening or integer casting.
/// Decompression is serial; memory includes selected pixels, byte coverage,
/// codec blocks, headers and offset tables (including unselected parts).
pub fn decode(path: &Path, part: usize, pass: &str) -> Result<Pixels> {
    let span = crate::diagnostics::Span::new(
        "exr.pixels",
        json!({"path":path.to_string_lossy(),"part":part,"pass":pass,"codec":CODEC,"parallel":false}),
    );
    span.result(decode_inner(path, part, pass))
}
fn decode_inner(path: &Path, part: usize, pass: &str) -> Result<Pixels> {
    let file = File::open(path).map_err(crate::edit_output::io)?;
    let before = file.metadata().map_err(crate::edit_output::io)?;
    let own = crate::exr_header::read(path)?;
    let select = crate::diagnostics::Span::new("exr.select", json!({"part":part,"pass":pass}));
    let selection = (|| {
        let header = own
            .parts
            .get(part)
            .ok_or_else(|| Error::new("exr_selection", "part index is absent"))?;
        let prefix = format!("{pass}.");
        let channels=header.channels.iter().enumerate().filter(|(_,c)| {
            if pass.is_empty() { !c.name.contains(&b'.') } else { c.name.starts_with(prefix.as_bytes()) }
        }).map(|(i,c)| {
            if c.sampling != [1,1] || c.pixel_type == 0 {
                return Err(Error::new("exr_unsupported","pixel output requires full-sampled float/half channels; integer conversion is not lossless"));
            }
            let name=String::from_utf8(c.name.clone()).map_err(|_|Error::new("exr_selection","selected channel name is not UTF-8"))?;
            Ok((i,name))
        }).collect::<Result<Vec<_>>>()?;
        if channels.is_empty() {
            return Err(Error::new(
                "exr_selection",
                "named pass has no channels; no fallback was attempted",
            ));
        }
        // All channels in a selected compressed block must be full-sampled: the
        // codec's raw line iterator explicitly does not implement subsampling.
        if header.channels.iter().any(|c| c.sampling != [1, 1]) {
            return Err(Error::new(
                "exr_unsupported",
                "selected part has subsampled channels",
            ));
        }
        let facts = own.json();
        if own.version & 0x800 != 0 || facts["parts"][part]["deep"] == true {
            return Err(Error::new(
                "exr_unsupported",
                "deep pixel data is not supported",
            ));
        }
        if facts["truncated"] == true {
            return Err(decode_error(
                "header reader found truncated chunk structure",
            ));
        }
        Ok(channels)
    })();
    let channels = select.result(selection)?;
    let reader = exr::block::read(BufReader::new(&file), true).map_err(codec_error)?;
    let header = reader
        .headers()
        .get(part)
        .ok_or_else(|| decode_error("codec/header part disagreement"))?;
    let width = header.layer_size.width();
    let height = header.layer_size.height();
    let origin = header.own_attributes.layer_position;
    let xmax = i64::from(origin.x()) + width as i64 - 1;
    let ymax = i64::from(origin.y()) + height as i64 - 1;
    let data_window = [
        origin.x(),
        origin.y(),
        i32::try_from(xmax).map_err(|_| decode_error("data window overflow"))?,
        i32::try_from(ymax).map_err(|_| decode_error("data window overflow"))?,
    ];
    let count = width
        .checked_mul(height)
        .and_then(|v| v.checked_mul(channels.len()))
        .ok_or_else(|| decode_error("pixel sample count overflow"))?;
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(count)
        .map_err(|e| decode_error(e.to_string()))?;
    samples.resize(count, 0f32);
    let mut seen = Vec::new();
    seen.try_reserve_exact(count)
        .map_err(|e| decode_error(e.to_string()))?;
    seen.resize(count, false);
    let decompress = crate::diagnostics::Span::new(
        "exr.decode",
        json!({"part":part,"width":width,"height":height,"channels":channels.iter().map(|(_,n)|n).collect::<Vec<_>>(),"samples_expected":count}),
    );
    let result = (|| {
        reader
            .filter_chunks(true, |_, tile, block| {
                block.layer == part && tile.is_largest_resolution_level()
            })
            .map_err(codec_error)?
            .decompress_sequential(true, |meta, block| {
                validate_block_selection(&block.index, part)?;
                let header = &meta.headers[part];
                // The crate owns block geometry validation and native sample reads.
                for line in block.lines(&header.channels) {
                    let Some(c) = channels
                        .iter()
                        .position(|(index, _)| *index == line.location.channel)
                    else {
                        continue;
                    };
                    let position = line.location.position;
                    let values: Vec<f32> = match header.channels.list[line.location.channel]
                        .sample_type
                    {
                        SampleType::F16 => line
                            .read_samples::<f16>()
                            .map(|v| v.map(f16::to_f32))
                            .collect::<exr::error::Result<_>>()?,
                        SampleType::F32 => line
                            .read_samples::<f32>()
                            .collect::<exr::error::Result<_>>()?,
                        SampleType::U32 => {
                            return Err(exr::error::Error::NotSupported("integer output".into()));
                        }
                    };
                    for (x, value) in values.into_iter().enumerate() {
                        if position.y() >= height
                            || position.x().checked_add(x).is_none_or(|x| x >= width)
                        {
                            return Err(exr::error::Error::Invalid(
                                "decoded sample lies outside data window".into(),
                            ));
                        }
                        let index =
                            ((position.y() * width + position.x() + x) * channels.len()) + c;
                        if seen[index] {
                            return Err(exr::error::Error::Invalid(
                                "duplicate decoded sample".into(),
                            ));
                        }
                        seen[index] = true;
                        samples[index] = value;
                    }
                }
                Ok(())
            })
            .map_err(codec_error)
    })();
    decompress.result(result)?;
    let verify = crate::diagnostics::Span::new("exr.verify", json!({"samples_expected":count}));
    verify.result((|| {
        if seen.iter().any(|present| !present) {
            return Err(decode_error("missing decoded samples"));
        }
        crate::kernel_copy::check_source(&file, path, &before)?;
        Ok(())
    })())?;
    Ok(Pixels {
        width,
        height,
        data_window,
        channels: channels.into_iter().map(|(_, n)| n).collect(),
        samples,
    })
}
/// Decode completely before staging and atomically publishing a new f32le file.
/// Existing outputs are never replaced; returned report names the exact layout.
pub fn extract(path: &Path, part: usize, pass: &str, output: &Path) -> Result<Value> {
    let pixels = decode(path, part, pass)?;
    let metadata = path.metadata().map_err(crate::edit_output::io)?;
    let mut stage = crate::edit_output::AtomicOutput::create(&metadata, output)?;
    {
        let mut writer = BufWriter::new(&mut stage.file);
        for sample in &pixels.samples {
            writer
                .write_all(&sample.to_le_bytes())
                .map_err(crate::edit_output::io)?;
        }
        writer.flush().map_err(crate::edit_output::io)?;
    }
    stage.publish(crate::edit_output::Durability::None)?;
    Ok(
        json!({"schema":1,"complete":true,"decode_complete":true,"codec":CODEC,"part":part,"pass":pass,"channels":pixels.channels,"width":pixels.width,"height":pixels.height,"data_window":pixels.data_window,"samples":pixels.samples.len(),"format":"f32le-interleaved","order":"increasing-y-then-x-from-data-window-min","output":output.to_string_lossy(),"output_written":true,"bytes":pixels.samples.len()*4,"durability":"none"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codec_block_must_match_selected_part_and_base_level() {
        let mut index = exr::block::BlockIndex {
            layer: 1,
            pixel_position: exr::math::Vec2(0, 0),
            pixel_size: exr::math::Vec2(1, 1),
            level: exr::math::Vec2(0, 0),
        };
        assert!(
            validate_block_selection(&index, 0).is_err(),
            "must reject part substitution before slicing any pixel bytes"
        );
        index.layer = 0;
        index.level = exr::math::Vec2(1, 0);
        assert!(
            validate_block_selection(&index, 0).is_err(),
            "must not substitute a lower resolution level"
        );
        index.level = exr::math::Vec2(0, 0);
        assert!(validate_block_selection(&index, 0).is_ok());
    }
    fn codec_matches_lockfile(lockfile: &str) -> bool {
        let mut packages = lockfile
            .split("[[package]]")
            .filter(|package| package.lines().any(|line| line == "name = \"exr\""));
        let Some(package) = packages.next() else {
            return false;
        };
        let version = format!("version = \"{}\"", CODEC.strip_prefix("exr-").unwrap());
        packages.next().is_none() && package.lines().any(|line| line == version)
    }

    #[test]
    fn reported_codec_matches_lockfile_pin() {
        assert!(codec_matches_lockfile(include_str!("../Cargo.lock")));
    }

    #[test]
    fn mismatched_lockfile_version_is_rejected() {
        let lockfile = include_str!("../Cargo.lock");
        let version = CODEC.strip_prefix("exr-").unwrap();
        let entry = format!("name = \"exr\"\nversion = \"{version}\"");
        assert_eq!(lockfile.matches(&entry).count(), 1);
        let mismatched =
            lockfile.replace(&entry, "name = \"exr\"\nversion = \"mismatched-version\"");
        assert!(!codec_matches_lockfile(&mismatched));
    }
}
