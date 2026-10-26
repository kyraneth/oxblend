//! Schema-1 fixed-width filepath rewrite specification parser.
//!
//! Extracted unchanged from the CLI so the Python binding and `main` accept
//! exactly the same bytes without a second parser or a subprocess.
use crate::edit::PathRewrite;
use crate::value::PathStep;
use crate::{Error, Result};
use serde_json::Value as Json;

fn unhex(value: &Json) -> Result<Vec<u8>> {
    let text = value
        .as_str()
        .ok_or_else(|| Error::new("edit_spec", "hex string required"))?;
    if !text.len().is_multiple_of(2) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::new("edit_spec", "even-length ASCII hex required"));
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let one = |v: u8| {
                if v.is_ascii_digit() {
                    v - b'0'
                } else {
                    v.to_ascii_lowercase() - b'a' + 10
                }
            };
            Ok((one(pair[0]) << 4) | one(pair[1]))
        })
        .collect()
}
fn spec_index(value: &Json) -> Result<usize> {
    value
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| Error::new("edit_spec", "index must be a nonnegative usize"))
}
fn spec_keys(value: &Json, keys: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| Error::new("edit_spec", "object required"))?;
    if object.keys().any(|key| !keys.contains(&key.as_str())) {
        return Err(Error::new("edit_spec", "unknown specification field"));
    }
    Ok(())
}
/// Parse the exact schema-1 `rewrite_paths` JSON object accepted by the CLI.
pub fn parse(value: &Json) -> Result<Vec<PathRewrite>> {
    spec_keys(value, &["schema", "operation", "edits"])?;
    if value["schema"] != 1 || value.get("operation").is_some_and(|v| v != "rewrite_paths") {
        return Err(Error::new(
            "edit_spec",
            "schema1 rewrite_paths specification required",
        ));
    }
    value["edits"]
        .as_array()
        .ok_or_else(|| Error::new("edit_spec", "edits array required"))?
        .iter()
        .map(|edit| {
            spec_keys(
                edit,
                &[
                    "block_index",
                    "element",
                    "path",
                    "expected_path_hex",
                    "replacement_path_hex",
                ],
            )?;
            let steps = edit["path"]
                .as_array()
                .ok_or_else(|| Error::new("edit_spec", "path component array required"))?
                .iter()
                .map(|step| {
                    spec_keys(step, &["field_hex", "index"])?;
                    if step.as_object().is_none_or(|v| v.len() != 1) {
                        return Err(Error::new("edit_spec", "one field_hex or index per step"));
                    }
                    if let Some(v) = step.get("field_hex") {
                        Ok(PathStep::Field(unhex(v)?))
                    } else {
                        Ok(PathStep::Index(spec_index(&step["index"])?))
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(PathRewrite {
                block_index: spec_index(&edit["block_index"])?,
                element: spec_index(&edit["element"])?,
                steps,
                expected_path: unhex(&edit["expected_path_hex"])?,
                replacement_path: unhex(&edit["replacement_path_hex"])?,
            })
        })
        .collect()
}
