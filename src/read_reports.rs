//! Shared serialized read-report projections used by CLI and bindings.
//!
//! One implementation per report shape: the CLI arms and the native binding
//! adapters both call these functions, so command JSON parity is by
//! construction rather than a second semantic reimplementation. Field names,
//! ordering of construction and error messages match the original CLI arms.

use crate::value::{PathStep, hex};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::path::Path;

/// Classify an I/O failure exactly as the CLI's delivery/error sites do:
/// a broken stdout pipe is its own kind, everything else is `io`.
pub fn io_kind_error(error: std::io::Error) -> Error {
    Error::new(
        if error.kind() == std::io::ErrorKind::BrokenPipe {
            "broken_pipe"
        } else {
            "io"
        },
        error.to_string(),
    )
}

/// Admit a root for direct-read semantics before a retained Session captures
/// it. Direct CLI reads report the operating system's directory error, whereas
/// pack capture deliberately retains its historical `pack_path` refusal.
/// Non-directory sources still go through normal capture admission (notably
/// FIFO/device refusal before opening); no formatted-error remapping occurs.
#[cfg(feature = "native")]
pub fn admit_read_root(path: &Path) -> Result<()> {
    if path.is_dir() {
        return Err(io_kind_error(std::io::Error::from_raw_os_error(
            libc::EISDIR,
        )));
    }
    Ok(())
}

/// Decode an already-parsed JSON field-path array with the exact shared
/// grammar: field strings and nonnegative flattened array indexes, native
/// bytes retained. Single source of truth for CLI text paths and batch
/// request arrays; messages stay identical, no second path semantics.
pub fn field_steps(value: &Json) -> Result<Vec<PathStep>> {
    let items = value
        .as_array()
        .ok_or_else(|| Error::new("path", "JSON array required"))?;
    items
        .iter()
        .map(|v| match v {
            Json::String(s) => Ok(PathStep::Field(s.as_bytes().to_vec())),
            Json::Number(n) => n
                .as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .map(PathStep::Index)
                .ok_or_else(|| Error::new("path", "index must be nonnegative and fit usize")),
            _ => Err(Error::new(
                "path",
                "only field strings and nonnegative integer indexes",
            )),
        })
        .collect()
}

/// Parse the JSON field-path argument shared by get/pointer: an array of
/// field strings and nonnegative flattened array indexes, native bytes
/// retained in each step.
pub fn field_path(text: &str) -> Result<Vec<PathStep>> {
    let value: Json = serde_json::from_str(text).map_err(|e| Error::new("path", e.to_string()))?;
    field_steps(&value)
}

/// Canonical input signature for per-file fact extraction: exact path bytes,
/// size and mtime as decimal strings. Computed before open and rechecked
/// after, so a source changed during extraction is a typed input_changed.
pub fn signature(path: &Path) -> Result<Json> {
    let metadata = path.metadata().map_err(io_kind_error)?;
    let modified = metadata
        .modified()
        .map_err(|e| Error::new("io", e.to_string()))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::new("io", e.to_string()))?;
    Ok(
        json!({"path_hex":hex(path.as_os_str().as_encoded_bytes()), "path":path.to_str(), "size_decimal":metadata.len().to_string(),
        "mtime_ns_decimal":modified.as_nanos().to_string()}),
    )
}

/// Every block in physical order plus the complete source SDNA.
/// `inspect.blocks` is ordered: its array index is the BLOCK argument.
pub fn inspect(file: &BlendFile) -> Json {
    let header = file.header();
    let mut report = json!({"schema":1, "header":{"version":header.version, "pointer_size":header.pointer_size,
        "little_endian":header.little_endian, "file_format":header.file_format},
        "sdna":{"names_hex":file.schema().names.iter().map(|v| hex(v)).collect::<Vec<_>>(),
            "types_hex":file.schema().types.iter().map(|v| hex(v)).collect::<Vec<_>>(), "lengths":file.schema().lengths,
            "structs":file.schema().structs.iter().map(|s| json!({"type_index":s.type_index,
                "fields":s.fields.iter().map(|f| json!({"name_hex":hex(&f.name), "full_name_hex":hex(&f.full_name),
                    "type_index":f.type_index, "offset":f.offset, "size":f.size, "array_len":f.array_len,
                    "element_size":f.element_size, "pointer":f.pointer})).collect::<Vec<_>>() })).collect::<Vec<_>>()}});
    // json!(Vec<Json>) serializes by reference and clones the entire tree.
    // Move this potentially millions-of-blocks array into the envelope instead.
    report["blocks"] = Json::Array(file.blocks().iter().map(|b| json!({
        "code_hex":hex(&b.code), "address_decimal":b.old_address.to_string(),
        "sdna_index":b.sdna_index, "count":b.count, "size":b.size, "data_offset":b.data_offset,
    })).collect());
    report
}

/// The block's entire decoded payload as hex (potentially large).
pub fn payload(file: &BlendFile, block: usize) -> Result<Json> {
    Ok(
        json!({"schema":1, "value":crate::value::Value::Bytes(file.payload(block)?.to_vec()).json()}),
    )
}

/// Per-file source/library fact report. `command` is exactly
/// "source-facts" or "library-facts"; `input_signature` is the canonical
/// `signature` captured before open and `display_path` the caller's
/// original spelling for error messages.
pub fn facts(
    file: &BlendFile,
    command: &str,
    input_signature: Json,
    display_path: &Path,
) -> Result<Json> {
    let facts = if command == "source-facts" {
        crate::facts::source_facts(file)
    } else {
        crate::facts::library_facts(file)
    }
    .map_err(|mut error| {
        error.message = format!(
            "{command} in file '{}': {}; use trace for cross-file dependencies",
            display_path.display(),
            error.message
        );
        error
    })?;
    let mut report = json!({"schema":1, "query":command, "semantic":if command == "source-facts" {"oxblend-source-1"} else {"oxblend-library-1"},
        "reader":{"name":"oxblend", "version":env!("CARGO_PKG_VERSION"), "python_reference":"BAT-1.23"},
        "input":input_signature, "stats":{"decompressed_bytes":file.decompressed_len(),
            "indexed_blocks":file.blocks().len(), "fact_blocks":facts["blocks"].as_object().map_or(0, |b| b.len())}, "facts":facts});
    if report["facts"]["complete"] == false {
        report["complete"] = json!(false);
        report["errors"] = report["facts"]["errors"].clone();
    }
    Ok(report)
}

/// Typed get projection for one field query. Array modes are mutually
/// exclusive: one raw trimmed read, one declared byte-pointer read or one
/// declared struct** pointer-table read.
pub fn get(
    file: &BlendFile,
    block: usize,
    element: usize,
    path: &[PathStep],
    raw: bool,
    byte_array: Option<usize>,
    pointer_table: Option<usize>,
) -> Result<Json> {
    if usize::from(raw) + usize::from(byte_array.is_some()) + usize::from(pointer_table.is_some())
        > 1
    {
        return Err(Error::new(
            "usage",
            "duplicate, incompatible or unknown get/pointer option",
        ));
    }
    let value = match (byte_array, pointer_table) {
        (Some(count), _) => {
            crate::value::Value::Bytes(file.byte_array(block, element, path, count)?).json()
        }
        (_, Some(count)) => json!({"kind":"pointer_table","block_indices":file.pointer_table(
            block, element, path, count,
        )?}),
        _ => file.get(block, element, path, !raw)?.json(),
    };
    Ok(json!({"schema":1,"value":value}))
}

/// Checked single pointer traversal projection: the target block index.
pub fn pointer(file: &BlendFile, block: usize, element: usize, path: &[PathStep]) -> Result<Json> {
    Ok(json!({"schema":1, "block_index":file.get_pointer(block, element, path)?}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const EMPTY: &[u8] = include_bytes!("../tests/fixtures/stub-chain/empty.blend");

    /// One unique scratch directory per test from the shared test support;
    /// files stay on disk until the guard drops, after every reader does.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            Self(crate::test_scratch::scratch_dir(&std::env::temp_dir()))
        }
        fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn field_path_accepts_fields_and_indexes_and_refuses_other_json() {
        let steps = field_path(r#"["name",2,0]"#).unwrap();
        assert_eq!(steps.len(), 3);
        assert!(field_path("[]").unwrap().is_empty());
        assert_eq!(field_path("{}").unwrap_err().kind, "path");
        assert_eq!(field_path(r#"[-1]"#).unwrap_err().kind, "path");
        assert_eq!(field_path(r#"[true]"#).unwrap_err().kind, "path");
    }

    #[test]
    fn signature_records_path_bytes_size_and_mtime_as_decimals() {
        let scratch = Scratch::new();
        let path = scratch.write("signature.blend", EMPTY);
        let value = signature(&path).unwrap();
        assert_eq!(value["path"].as_str(), path.to_str());
        assert_eq!(value["path_hex"], hex(path.as_os_str().as_encoded_bytes()));
        assert_eq!(value["size_decimal"], EMPTY.len().to_string());
        value["mtime_ns_decimal"].as_str().unwrap();
        assert_eq!(
            signature(&scratch.0.join("absent.blend")).unwrap_err().kind,
            "io"
        );
    }

    #[test]
    fn inspect_payload_and_facts_match_published_cli_shapes() {
        let scratch = Scratch::new();
        let path = scratch.write("empty.blend", EMPTY);
        let file = BlendFile::open(&path).unwrap();
        let report = inspect(&file);
        assert_eq!(report["schema"], 1);
        assert!(report["blocks"].is_array());
        assert!(report["sdna"]["structs"].is_array());
        assert_eq!(report["header"]["version"], file.header().version);

        let payload_report = payload(&file, 0).unwrap();
        assert_eq!(payload_report["schema"], 1);
        assert_eq!(payload_report["value"]["kind"], "bytes");
        assert!(payload_report["value"]["hex"].is_string());
        assert_eq!(payload(&file, 9_999).unwrap_err().kind, "bounds");

        for command in ["source-facts", "library-facts"] {
            let report = facts(
                &file,
                command,
                signature(&path).unwrap(),
                Path::new("empty.blend"),
            )
            .unwrap();
            assert_eq!(report["schema"], 1);
            assert_eq!(report["query"], command);
            assert_eq!(
                report["semantic"],
                if command == "source-facts" {
                    "oxblend-source-1"
                } else {
                    "oxblend-library-1"
                }
            );
            assert!(report["facts"]["blocks"].is_object());
        }
    }

    #[test]
    fn get_and_pointer_projections_keep_exact_cli_envelopes() {
        let scratch = Scratch::new();
        let path = scratch.write("empty.blend", EMPTY);
        let file = BlendFile::open(&path).unwrap();
        let report = get(&file, 0, 0, &[], true, None, None);
        // An empty-path raw read is refused by the engine; the envelope is
        // the caller's, so only the exclusivity guard is proven here.
        assert!(report.is_err());
        assert_eq!(
            get(&file, 0, 0, &[], true, Some(1), None).unwrap_err().kind,
            "usage"
        );
        assert_eq!(
            get(&file, 0, 0, &[], false, Some(1), Some(1))
                .unwrap_err()
                .kind,
            "usage"
        );
        assert!(pointer(&file, 9_999, 0, &[]).is_err());
    }
}
