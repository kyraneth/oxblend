//! Single-open serialized read batch: one `inspect`/`get`/`pointer`/`payload`
//! plan executed in order with JSONL streaming and a terminal summary.
//!
//! Unstable/internal Rust API: may change in minor releases. CLI/JSON
//! contracts are separate. The engine projections are exactly the shared
//! `read_reports::{get,pointer,payload,inspect}` functions, so per-item
//! report bytes equal the corresponding single-command stdout bytes and
//! per-item public error objects equal the single CLI error objects
//! (diagnostic references remain invocation-local and are validated
//! separately, never fabricated or normalized).

use crate::read_reports::{self, io_kind_error};
use crate::value::PathStep;
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::ffi::{OsStr, OsString};
use std::path::Path;

/// Strict top-level requests document: `{"schema":1,"requests":[...]}`.
/// Empty arrays are valid (zero observations, input check still runs).
/// Unknown top-level keys refuse the whole batch; per-item problems are
/// retained as independent item errors, never a whole-batch abort.
pub fn load_requests(spec: &Path) -> Result<Vec<Json>> {
    let data = std::fs::read(spec).map_err(|e| Error::new("io", e.to_string()))?;
    let mut value: Json =
        serde_json::from_slice(&data).map_err(|e| Error::new("edit_spec", e.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| Error::new("edit_spec", "object required"))?;
    if object.keys().any(|k| k != "schema" && k != "requests") {
        return Err(Error::new("edit_spec", "unknown specification field"));
    }
    if value["schema"] != 1 {
        return Err(Error::new(
            "edit_spec",
            "schema1 read-batch requests document required",
        ));
    }
    match value
        .get_mut("requests")
        .map(|v| std::mem::replace(v, Json::Null))
    {
        Some(Json::Array(requests)) => Ok(requests),
        _ => Err(Error::new("edit_spec", "requests array required")),
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ValidRequest {
    Inspect,
    Payload {
        block: usize,
    },
    Pointer {
        block: usize,
        element: usize,
        path: Vec<PathStep>,
    },
    Get {
        block: usize,
        element: usize,
        path: Vec<PathStep>,
        raw: bool,
        byte_array: Option<usize>,
        pointer_table: Option<usize>,
    },
}

fn usage(message: &str) -> Error {
    Error::new("usage", message)
}

fn parse_index(value: &Json) -> Result<usize> {
    value
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| usage("nonnegative integer required"))
}

fn parse_path(value: &Json) -> Result<Vec<PathStep>> {
    // Single shared grammar/messages with the CLI text path decoder.
    read_reports::field_steps(value)
}

fn allowed(value: &Json, keys: &[&str]) -> Result<()> {
    let object = value.as_object().ok_or_else(|| usage("object required"))?;
    if object.keys().any(|k| !keys.contains(&k.as_str())) {
        return Err(usage("unknown request field"));
    }
    Ok(())
}

/// Parse one planned request. Structural problems are independent per-item
/// errors (usage/path), never a whole-batch abort and never a guard stop.
pub(crate) fn parse_item(value: &Json) -> Result<(String, ValidRequest)> {
    let object = value.as_object().ok_or_else(|| usage("object required"))?;
    let op = object
        .get("op")
        .and_then(|v| v.as_str())
        .ok_or_else(|| usage("unknown op; expected inspect|payload|get|pointer"))?;
    match op {
        "inspect" => {
            allowed(value, &["op"])?;
            Ok((op.to_owned(), ValidRequest::Inspect))
        }
        "payload" => {
            allowed(value, &["op", "block"])?;
            let block = value
                .get("block")
                .map(parse_index)
                .transpose()?
                .ok_or_else(|| usage("missing required field: block"))?;
            Ok((op.to_owned(), ValidRequest::Payload { block }))
        }
        "pointer" => {
            allowed(value, &["op", "block", "element", "path"])?;
            let block = value
                .get("block")
                .map(parse_index)
                .transpose()?
                .ok_or_else(|| usage("missing required field: block"))?;
            let element = value
                .get("element")
                .map(parse_index)
                .transpose()?
                .unwrap_or(0);
            let path = value
                .get("path")
                .map(parse_path)
                .transpose()?
                .ok_or_else(|| usage("missing required field: path"))?;
            Ok((
                op.to_owned(),
                ValidRequest::Pointer {
                    block,
                    element,
                    path,
                },
            ))
        }
        "get" => {
            allowed(
                value,
                &[
                    "op",
                    "block",
                    "element",
                    "path",
                    "raw",
                    "byte_array",
                    "pointer_table",
                ],
            )?;
            let block = value
                .get("block")
                .map(parse_index)
                .transpose()?
                .ok_or_else(|| usage("missing required field: block"))?;
            let element = value
                .get("element")
                .map(parse_index)
                .transpose()?
                .unwrap_or(0);
            let path = value
                .get("path")
                .map(parse_path)
                .transpose()?
                .ok_or_else(|| usage("missing required field: path"))?;
            let raw = match value.get("raw") {
                None => false,
                Some(Json::Bool(b)) => *b,
                Some(_) => return Err(usage("raw must be boolean")),
            };
            let byte_array = value.get("byte_array").map(parse_index).transpose()?;
            let pointer_table = value.get("pointer_table").map(parse_index).transpose()?;
            if usize::from(raw)
                + usize::from(byte_array.is_some())
                + usize::from(pointer_table.is_some())
                > 1
            {
                return Err(Error::new(
                    "usage",
                    "duplicate, incompatible or unknown get/pointer option",
                ));
            }
            Ok((
                op.to_owned(),
                ValidRequest::Get {
                    block,
                    element,
                    path,
                    raw,
                    byte_array,
                    pointer_table,
                },
            ))
        }
        _ => Err(usage("unknown op; expected inspect|payload|get|pointer")),
    }
}

fn single_argv(op: &str, file: &OsStr) -> Vec<OsString> {
    vec![
        OsString::from("oxblend"),
        OsString::from(op),
        OsString::from(file),
    ]
}

/// Canonical public error object for one failed item via the single owned
/// helper: exactly the CLI envelope shape (kind, message, next_command
/// plus catalog prose, no diagnostic_ref inside, no raw snapshots).
/// The row sibling `diagnostic_ref` is added at the producer while the
/// original `Error` is alive (verbatim null or bound pair), never by
/// reconstruction or SourceSnapshot export.
/// `single_argv` must yield the identical observed help context as the
/// corresponding single command; parity tests prove the equality.
pub(crate) fn public_error(error: &Error, op: &str, file: &OsStr) -> Json {
    let command = if crate::bat_cli::COMMANDS.contains(&op) {
        op
    } else {
        "read-batch"
    };
    crate::error_help::public_error(error, &single_argv(command, file))
}

fn emit_line(out: &mut dyn std::io::Write, value: &Json) -> Result<Vec<String>> {
    let causes = crate::failure_receipt::record_report(value);
    let encoded =
        serde_json::to_vec(value).map_err(|e| Error::new("serialization", e.to_string()))?;
    let span = crate::diagnostics::Span::new(
        "serialize",
        json!({"role":"report_jsonl","scope":"caller_writer"}),
    );
    let result = (|| {
        out.write_all(&encoded).map_err(io_kind_error)?;
        out.write_all(b"\n").map_err(io_kind_error)?;
        out.flush().map_err(io_kind_error)?;
        Ok(())
    })();
    span.result(result).map(|_| causes)
}

fn is_guard_kind(kind: &str) -> bool {
    matches!(kind, "input_changed" | "source_changed")
}

/// Execute one opened batch: ordered JSONL items plus a terminal summary.
/// Independent item errors are retained; a known guard failure stops the
/// remaining requests (counted as unobserved) and its original kind
/// outranks the `incomplete_read_batch` aggregate. A mandatory final
/// `check_source` runs even for empty/all-parse-error/guard batches;
/// metadata signature alone is insufficient against the Reader guard.
/// First guard wins; a dropped secondary is not retained as a separate
/// ledger cause (its producer `check_source` carries no source record),
/// only the winning guard plus summary `input`/`input_recheck`/`input_changed`
/// stay honest, never replacing the known root guard.
/// Large reports are MOVED into their wrapper (`line["report"]=report`),
/// never cloned into a second 8GB tree; `emit_line` buffers one item only.
pub fn execute(
    file: &BlendFile,
    input: &Path,
    input_os: &OsStr,
    requests: Vec<Json>,
    input_before: Json,
    out: &mut dyn std::io::Write,
) -> Result<Json> {
    let total = requests.len();
    let mut observed = 0usize;
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut aggregate_causes: Vec<String> = Vec::new();
    let mut guard: Option<Error> = None;

    for (index, raw) in requests.iter().enumerate() {
        if let Err(error) = file.check_source() {
            guard = Some(error);
            break;
        }
        let (op, request) = match parse_item(raw) {
            Ok(parsed) => parsed,
            Err(error) => {
                let op = raw
                    .get("op")
                    .and_then(|v| v.as_str())
                    .unwrap_or("read-batch")
                    .to_owned();
                let mut line = json!({"index":index,"op":op,"ok":false});
                line["error"] = public_error(&error, &op, input_os);
                line["diagnostic_ref"] = error.diagnostic_ref();
                aggregate_causes.extend(emit_line(out, &line)?);
                observed += 1;
                failed += 1;
                continue;
            }
        };
        let report = match request {
            ValidRequest::Inspect => Ok(read_reports::inspect(file)),
            ValidRequest::Payload { block } => read_reports::payload(file, block),
            ValidRequest::Pointer {
                block,
                element,
                path,
            } => read_reports::pointer(file, block, element, &path),
            ValidRequest::Get {
                block,
                element,
                path,
                raw,
                byte_array,
                pointer_table,
            } => read_reports::get(file, block, element, &path, raw, byte_array, pointer_table),
        };
        match report {
            Ok(report) => {
                let mut line = json!({"index":index,"op":op,"ok":true});
                line["report"] = report;
                aggregate_causes.extend(emit_line(out, &line)?);
                observed += 1;
                succeeded += 1;
            }
            Err(error) => {
                if is_guard_kind(error.kind) {
                    let mut line = json!({"index":index,"op":op,"ok":false});
                    line["error"] = public_error(&error, &op, input_os);
                    line["diagnostic_ref"] = error.diagnostic_ref();
                    aggregate_causes.extend(emit_line(out, &line)?);
                    observed += 1;
                    failed += 1;
                    guard = Some(error);
                    break;
                }
                let mut line = json!({"index":index,"op":op,"ok":false});
                line["error"] = public_error(&error, &op, input_os);
                line["diagnostic_ref"] = error.diagnostic_ref();
                aggregate_causes.extend(emit_line(out, &line)?);
                observed += 1;
                failed += 1;
            }
        }
        if let Err(error) = file.check_source() {
            guard = Some(error);
            break;
        }
    }

    // Mandatory final Reader guard, even for empty/all-parse-error/guard
    // batches. First guard wins; a dropped secondary is not retained
    // (its `check_source` producer carries no source record), only the
    // winning guard plus summary input fields stay honest.
    let final_check = file.check_source().err();
    let unobserved = total.saturating_sub(observed);
    let (input_recheck, input_changed, terminal_guard) = match read_reports::signature(input) {
        Ok(after) => {
            let changed = after != input_before;
            let sig_guard = if changed {
                Some(Error::new(
                    "input_changed",
                    "source changed during read-batch",
                ))
            } else {
                None
            };
            match (guard, final_check, sig_guard) {
                (Some(mid), _, _) => (after, true, Some(mid)),
                (None, Some(check), _) => (after, true, Some(check)),
                (None, None, Some(end)) => (after, true, Some(end)),
                (None, None, None) => (after, changed, None),
            }
        }
        Err(error) => (
            Json::Null,
            true,
            Some(guard.or(final_check).unwrap_or(error)),
        ),
    };

    let complete = terminal_guard.is_none() && failed == 0;
    let summary = json!({"schema":1,"record_type":"summary","complete":complete,
        "counts":{"total":total,"observed":observed,"succeeded":succeeded,"failed":failed,"unobserved":unobserved},
        "input":input_before,"input_recheck":input_recheck,"input_changed":input_changed});
    // The summary is itself a retained report observation.
    let summary_causes = emit_line(out, &summary)?;
    let _ = summary_causes;

    if let Some(error) = terminal_guard {
        crate::failure_receipt::record_terminal(&error, &aggregate_causes);
        return Err(error);
    }
    if failed > 0 {
        let error = crate::strict::read_batch_refusal();
        crate::failure_receipt::record_terminal(&error, &aggregate_causes);
        return Err(error);
    }
    Ok(summary)
}

/// Open once, run the whole plan, enforce the end source check.
/// Missing/nonregular inputs keep the single-read `io`/exit-2 semantics:
/// signature/open failures return before any JSONL.
pub fn run(
    input: &Path,
    input_os: &OsStr,
    spec: &Path,
    out: &mut dyn std::io::Write,
) -> Result<Json> {
    let requests = load_requests(spec)?;
    let before = read_reports::signature(input)?;
    let file = BlendFile::open(input)?;
    execute(&file, input, input_os, requests, before, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const EMPTY: &[u8] = include_bytes!("../tests/fixtures/stub-chain/empty.blend");

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
    fn strict_document_accepts_empty_and_refuses_unknown_top_level() {
        let scratch = Scratch::new();
        let spec = scratch.write("empty.json", b"{\"schema\":1,\"requests\":[]}");
        assert!(load_requests(&spec).unwrap().is_empty());
        let bad = scratch.write("bad.json", b"{\"schema\":1,\"requests\":[],\"extra\":1}");
        assert_eq!(load_requests(&bad).unwrap_err().kind, "edit_spec");
        let wrong = scratch.write("wrong.json", b"{\"schema\":2,\"requests\":[]}");
        assert_eq!(load_requests(&wrong).unwrap_err().kind, "edit_spec");
        let missing = scratch.write("missing.json", b"{\"schema\":1}");
        assert_eq!(load_requests(&missing).unwrap_err().kind, "edit_spec");
    }

    #[test]
    fn item_shapes_accept_valid_and_refuse_unknown_keys() {
        let (_, request) = parse_item(&json!({"op":"inspect"})).unwrap();
        assert!(matches!(request, ValidRequest::Inspect));
        assert!(parse_item(&json!({"op":"inspect","block":0})).is_err());
        assert!(parse_item(&json!({"op":"payload"})).is_err());
        let (_, request) = parse_item(&json!({"op":"payload","block":0})).unwrap();
        assert!(matches!(request, ValidRequest::Payload { block: 0 }));
        assert!(parse_item(&json!({"op":"get","block":0})).is_err());
        assert_eq!(
            parse_item(&json!({"op":"get","block":0,"path":[],"raw":true,"byte_array":1}))
                .unwrap_err()
                .kind,
            "usage"
        );
        assert_eq!(parse_item(&json!({"op":"nope"})).unwrap_err().kind, "usage");
        assert_eq!(
            parse_item(&json!({"op":"get","block":0,"path":["a"],"raw":"yes"}))
                .unwrap_err()
                .kind,
            "usage"
        );
    }

    #[test]
    fn batch_streams_items_then_summary_and_keeps_input_check() {
        let scratch = Scratch::new();
        let input = scratch.write("empty.blend", EMPTY);
        let spec = scratch.write(
            "requests.json",
            b"{\"schema\":1,\"requests\":[{\"op\":\"inspect\"},{\"op\":\"payload\",\"block\":99999},{\"op\":\"inspect\",\"extra\":1}]}",
        );
        let requests = load_requests(&spec).unwrap();
        assert_eq!(requests.len(), 3);
        let before = read_reports::signature(&input).unwrap();
        let file = BlendFile::open(&input).unwrap();
        let mut out = Vec::new();
        let error =
            execute(&file, &input, input.as_os_str(), requests, before, &mut out).unwrap_err();
        assert_eq!(error.kind, "incomplete_read_batch");
        let lines: Vec<Json> = out
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0]["ok"], true);
        assert_eq!(lines[0]["report"]["schema"], 1);
        assert_eq!(lines[1]["ok"], false);
        assert_eq!(lines[1]["error"]["kind"], "bounds");
        // Batch error object has no diagnostic_ref inside; the verbatim
        // invocation-local reference is the row sibling.
        assert!(lines[1]["error"].get("diagnostic_ref").is_none());
        assert!(lines[1].get("diagnostic_ref").is_some());
        assert_eq!(lines[2]["ok"], false);
        assert_eq!(lines[2]["error"]["kind"], "usage");
        assert!(lines[2]["error"].get("diagnostic_ref").is_none());
        assert!(lines[2].get("diagnostic_ref").is_some());
        assert_eq!(lines[3]["record_type"], "summary");
        assert_eq!(lines[3]["complete"], false);
        assert_eq!(lines[3]["counts"]["total"], 3);
        assert_eq!(lines[3]["counts"]["observed"], 3);
        assert_eq!(lines[3]["counts"]["unobserved"], 0);
        assert_eq!(lines[3]["input_changed"], false);
    }
}
