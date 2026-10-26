//! Closed shared-output serializers. Treat every input string/key as untrusted.
use super::report::{hash, json_bytes, kind, project, token};
use crate::{Result, report_input::Evidence};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn identity(v: &Value, key: &[u8; 32], names: bool) -> Result<Value> {
    Ok(match v.as_str() {
        Some(v) => json!(if names { v.to_owned() } else { token(key, v)? }),
        None => Value::Null,
    })
}
fn selected(v: &Value, allowed: &[&str]) -> Value {
    v.as_str()
        .filter(|s| allowed.contains(s))
        .map_or(Value::Null, |s| json!(s))
}
fn command(v: &Value) -> Value {
    v.as_str()
        .and_then(crate::bat_cli::builtin_command)
        .map_or(Value::Null, |s| json!(s))
}
fn decimal(v: &Value) -> Value {
    v.as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .map_or(Value::Null, |n| json!(n.to_string()))
}
fn schema_identifier(v: &Value, key: &[u8; 32], names: bool, block: bool) -> Result<Value> {
    if let Some(hex) = v
        .as_str()
        .filter(|s| s.len().is_multiple_of(2) && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        let bytes = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        let trimmed = bytes.strip_suffix(&[0, 0]).unwrap_or(&bytes);
        let known = if block {
            bytes.len() == 4
                && (matches!(bytes.as_slice(), b"DATA" | b"DNA1" | b"ENDB")
                    || crate::id_codes::TYPES
                        .iter()
                        .any(|(code, _)| *code == trimmed))
        } else {
            [
                b"id".as_slice(),
                b"lib",
                b"nodes",
                b"links",
                b"first",
                b"last",
                b"next",
                b"prev",
                b"inputs",
                b"outputs",
                b"camera",
                b"r",
                b"pic",
            ]
            .contains(&bytes.as_slice())
        };
        if known {
            return Ok(json!(
                std::str::from_utf8(trimmed).expect("trusted built-in schema identifier")
            ));
        }
    }
    identity(v, key, names)
}
fn source(v: &Value, key: &[u8; 32], names: bool) -> Result<Value> {
    let mut out = project(v, key)?;
    out["file_token"] = identity(&v["file_hex"], key, false)?;
    out["file_path_hex"] = if names {
        v["file_hex"]
            .as_str()
            .filter(|s| s.len().is_multiple_of(2) && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .map_or(Value::Null, |s| json!(s))
    } else {
        Value::Null
    };
    for (input, token_name, path_name) in [
        ("asset_path_hex", "asset_token", "asset_path_hex"),
        (
            "resolved_path_hex",
            "resolved_path_token",
            "resolved_path_hex",
        ),
    ] {
        out[token_name] = identity(&v[input], key, false)?;
        out[path_name] = if names {
            v[input]
                .as_str()
                .filter(|s| s.len().is_multiple_of(2) && s.bytes().all(|b| b.is_ascii_hexdigit()))
                .map_or(Value::Null, |s| json!(s))
        } else {
            Value::Null
        };
    }
    out["block_code"] = schema_identifier(&v["block_code_hex"], key, names, true)?;
    out["old_address"] = decimal(&v["old_address"]);
    out["pointer_decimal"] = decimal(&v["pointer_decimal"]);
    out["namespace"] = selected(
        &v["namespace"],
        &[
            "id",
            "owner_data",
            "qualified_cross_data",
            "global_diagnostic",
            "unknown",
        ],
    );
    out["resolution"] = selected(
        &v["resolution"],
        &[
            "missing",
            "ambiguous",
            "wrong_type",
            "wrong_extent",
            "unprofiled",
            "unknown",
        ],
    );
    out["observed_target"] = Value::Null;
    out["candidates"] = Value::Null;
    out["neighbours"] = Value::Null;
    out["source_identity_verified"] = json!(false);
    let mut unavailable = vec![
        "observed_target",
        "candidates",
        "neighbours",
        "sdna_struct",
        "declared_type",
        "source_content_identity",
    ];
    if out["file_token"].is_null() {
        unavailable.push("file");
    }
    if out["block_index"].is_null() {
        unavailable.push("block_index");
    }
    if out["namespace"] == "unknown" || out["namespace"].is_null() {
        unavailable.push("namespace");
    }
    for field in [
        "offset_in_block",
        "decoded_file_offset",
        "width",
        "blend_version",
        "little_endian",
    ] {
        if out[field].is_null() {
            unavailable.push(field);
        }
    }
    if v["field_path_hex"].as_array().is_none_or(|a| a.is_empty()) {
        unavailable.push("field_path");
    }
    out["unavailable"] = json!(unavailable);
    out["field_path"] = Value::Array(
        v["field_path_hex"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|x| schema_identifier(x, key, names, false))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default(),
    );
    Ok(out)
}
fn checked_id(v: &Value, key: &[u8; 32]) -> Result<Value> {
    Ok(json!(
        v.as_str()
            .map(|s| {
                if [
                    "pointer.namespace.lookup.missing",
                    "pointer.namespace.lookup.ambiguous",
                    "pointer.namespace.lookup.wrong_namespace",
                    "pointer.namespace.lookup.forwarded",
                    "pointer.target.type",
                    "pointer.target.extent",
                    "pointer.role.unprofiled",
                    "pointer.resolve.forwarded",
                    "linked_id.forwarding.cycle",
                    "dependency.identity.observe",
                ]
                .contains(&s)
                {
                    Ok(s.to_owned())
                } else {
                    token(key, s)
                }
            })
            .transpose()?
    ))
}
/// Closed report-output policy for published warning identifiers. This is
/// deliberately not inferred from caller strings or an open-ended enum: new
/// kinds need an explicit privacy-policy review. These labels are the existing
/// TraceWarning wire identifiers, not new error-catalog entries.
pub(crate) fn warning_kind(v: &Value, key: &[u8; 32]) -> Result<Option<String>> {
    if let Some(name) = v.as_str().filter(|name| {
        matches!(
            *name,
            "missing_library"
                | "silenced_pointer"
                | "library_null_pointer"
                | "unknown_asset_code"
                | "embedded_text_filepath"
                | "embedded_camera_filepath"
                | "path_resolution_unverified"
        )
    }) {
        Ok(Some(name.to_owned()))
    } else {
        kind(v, key)
    }
}
fn failures(v: &Value, key: &[u8; 32], names: bool) -> Result<Vec<Value>> {
    diagnostic_records(v, key, names, false)
}
fn diagnostic_records(v: &Value, key: &[u8; 32], names: bool, warning: bool) -> Result<Vec<Value>> {
    v.as_array().map(|a|a.iter().map(|f| {
        let check=checked_id(&f["check_id"],key)?;
        let propagations=f["propagations"].as_array().map(|a|a.iter().map(|p|Ok(json!({"kind":kind(&p["kind"],key)?,"check_id":checked_id(&p["check_id"],key)?,"context":if p["context"].is_object(){source(&p["context"],key,names)?}else{Value::Null}}))).collect::<Result<Vec<_>>>()).transpose()?.unwrap_or_default();
        let projected_kind=if warning { warning_kind(&f["kind"],key)? } else { kind(&f["kind"],key)? };
        let catalogue=projected_kind.as_deref().and_then(crate::error_help::entry).unwrap_or(Value::Null);
        Ok(json!({"id":identity(&f["id"],key,false)?,"kind":projected_kind,"explanation":catalogue["explanation"].as_str().unwrap_or(if warning { "Producer-reported warning; raw message withheld." } else { "Unknown producer error; exact kind remains in local original evidence." }),"likely_causes":catalogue["likely_causes"],"next_steps":catalogue["next_steps"],"known_issue":catalogue["known_issue"],"check_id":check,"propagations":propagations,"context_origin" :selected(&f["context_origin"],&["owner_hook","producer_report","terminal","legacy_saved_report"]),"caused_by":f["caused_by"].as_array().map(|a|a.iter().map(|x|identity(x,key,false)).collect::<Result<Vec<_>>>()).transpose()?.unwrap_or_default(),"context":if f["context"].is_object(){source(&f["context"],key,names)?}else{Value::Null},"message_redacted":true}))
    }).collect()).transpose().map(Option::unwrap_or_default)
}
fn family(v: &Value) -> Value {
    if v == 1 || v == 2 {
        v.clone()
    } else {
        selected(
            v,
            &[
                "oxblend.diff/v1",
                "oxblend.pointer-census/v2",
                "oxblend.failure-receipt/v1",
            ],
        )
    }
}
fn safe_build(receipt: Option<&Value>) -> Value {
    let producer = receipt.map(|v| &v["producer"]);
    let revision = producer
        .and_then(|p| p["build_revision"].as_str())
        .filter(|s| [40, 64].contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit()));
    let version = producer.and_then(|p| p["version"].as_str()).filter(|s| {
        let v = s.strip_suffix("-dev").unwrap_or(s);
        let parts = v.split('.').collect::<Vec<_>>();
        parts.len() == 3
            && parts
                .iter()
                .all(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
    });
    let exporter_revision = env!("OXBLEND_BUILD_COMMIT");
    let exporter_revision = ([40, 64].contains(&exporter_revision.len())
        && exporter_revision.bytes().all(|b| b.is_ascii_hexdigit()))
    .then_some(exporter_revision);
    let producer_binary = producer
        .and_then(|p| p["binary_sha256"].as_str())
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()));
    let exporter_binary = crate::bat_cli::hash_file(std::path::Path::new("/proc/self/exe"))
        .ok()
        .map(|v| v.0);
    json!({"producer":{"name":producer.map(|_|"oxblend"),"version":version,"build_revision":revision,"binary_sha256":producer_binary},"exporter":{"name":"oxblend","version":env!("CARGO_PKG_VERSION"),"build_revision":exporter_revision,"binary_sha256":exporter_binary},"host":null})
}
fn trace(events: &[Value], key: &[u8; 32], names: bool) -> Result<Vec<u8>> {
    let catalog: Value = serde_json::from_str(include_str!("../flattrace-events.json"))
        .expect("compiled event catalog");
    let mut bytes = Vec::new();
    for event in events {
        let event_name = event["event"].as_str().unwrap_or("unknown");
        let event_name = if catalog["events"].get(event_name).is_some()
            || [
                "diagnostic.invocation",
                "diagnostic.source",
                "diagnostic.propagation",
                "diagnostic.receipt",
                "root.summary",
            ]
            .contains(&event_name)
        {
            event_name.to_owned()
        } else {
            token(key, event_name)?
        };
        let mut data = project(&event["data"], key)?;
        // Generic tracer data is never copied. These exact path identities are
        // tokens by default, even when an attacker supplies hex aliases.
        for field in [
            "path",
            "path_hex",
            "input",
            "output",
            "owner_file_hex",
            "member_id",
        ] {
            if event["data"].get(field).is_some() {
                data[field] = identity(&event["data"][field], key, names)?;
            }
        }
        if event["event"] == "diagnostic.invocation" {
            data = json!({"invocation_token":identity(&event["data"]["run_id"],key,false)?,"command":command(&event["data"]["command"])});
        }
        if event["event"] == "diagnostic.source" || event["event"] == "diagnostic.propagation" {
            data = json!({"failures":failures(&json!([event["data"].clone()]),key,names)?});
        }
        if event["event"] == "diagnostic.receipt" {
            data = json!({"receipt_bound":true,"failures":failures(&event["data"]["receipt"]["failures"],key,names)?});
        }
        let projected = json!({"seq":event["seq"].as_u64(),"event":event_name,"state":selected(&event["state"],&["start","end","point"]),"span":identity(&event["span"],key,false)?,"parent":identity(&event["parent"],key,false)?,"run":identity(&event["run"],key,false)?,"trace":identity(&event["trace"],key,false)?,"outcome":selected(&event["outcome"],&["ok","error","partial","refused","cancelled","unknown"]),"error":project(&event["error"],key)?,"took_ms":event["took_ms"].as_f64().filter(|v|v.is_finite() && *v>=0.0),"data":data});
        serde_json::to_writer(&mut bytes, &projected).expect("JSON value");
        bytes.push(b'\n');
    }
    Ok(bytes)
}
fn omissions(v: &Value, at: &str, key: &[u8; 32], out: &mut Vec<Value>) -> Result<()> {
    const COORDINATES: &[&str] = &[
        "schema",
        "error",
        "errors",
        "causes",
        "failures",
        "diagnostic",
        "root_traces",
        "operation_state",
        "reports",
        "source",
        "context",
        "binding",
        "producer",
        "data",
        "event",
        "state",
        "outcome",
        "kind",
        "message",
        "check_id",
        "id",
        "caused_by",
        "propagations",
        "diagnostic_ref",
        "failure_id",
        "run_id",
        "trace_identity",
        "trace",
        "root_span",
        "identity_kind",
        "command",
        "file_hex",
        "asset_path_hex",
        "resolved_path_hex",
        "block_index",
        "block_code_hex",
        "old_address",
        "sdna_index",
        "count",
        "payload_size",
        "data_offset",
        "element",
        "offset_in_block",
        "decoded_file_offset",
        "width",
        "blend_version",
        "little_endian",
        "field_path_hex",
        "pointer_decimal",
        "namespace",
        "resolution",
        "complete",
        "planning_complete",
        "output_written",
        "archive_written",
        "directory_created",
        "durability_completed",
        "output_decoder_verified",
        "argv",
        "env",
        "parent",
        "span",
        "seq",
        "t",
        "tool",
        "tool_version",
        "v",
        "run",
        "version",
        "build_revision",
        "binary_sha256",
        "stdout_sha256",
        "stdout_bytes",
        "stdout_delivery_complete",
        "stderr_error_sha256",
        "stderr_scope",
        "explanation",
        "likely_causes",
        "next_steps",
        "known_issue",
        "stdout_report",
        "stderr_report",
        "saved_report",
        "site",
    ];
    if let Some(object) = v.as_object() {
        for (name, child) in object {
            let recognized = COORDINATES.contains(&name.as_str())
                || (name == "warnings" && child.as_array().is_some_and(|a| !a.is_empty()));
            let safe = if recognized {
                name.to_owned()
            } else {
                token(key, name)?
            };
            let coordinate = format!("{at}/{safe}");
            if !recognized {
                out.push(json!({"event_or_report_pointer":coordinate,"reason":"unknown field omitted by closed serializer"}));
            } else {
                omissions(child, &coordinate, key, out)?;
            }
        }
    } else if let Some(array) = v.as_array() {
        for (i, child) in array.iter().enumerate() {
            omissions(child, &format!("{at}/{i}"), key, out)?;
        }
    } else if v.is_string() {
        out.push(json!({"event_or_report_pointer":at,"reason":"string revalidated, catalog-replaced, pseudonymized or omitted; never blindly forwarded"}));
    }
    Ok(())
}
/// One producer-report extractor for both error and warning records. Only
/// explicitly retained producer coordinates are used; no source is recaptured.
fn producer_record(v: &Value, at: &str, warning: bool) -> Value {
    let base = if warning && v["failure"].is_object() {
        &v["failure"]
    } else {
        v
    };
    let mut context = json!({"file_hex":base["owner_file_hex"],"block_index":base["block_index"],"old_address":base["block_address"],"field_path_hex":base["field_path_hex"],"asset_path_hex":base["asset_path_hex"],"resolved_path_hex":base["resolved_path_hex"]});
    if warning && v["kind"] == "library_null_pointer" {
        // This warning's established wire fields explicitly name the decoded
        // pointer-slot offset and saved pointer; do not infer these for others.
        context["decoded_file_offset"] = v["offset"].as_u64().map_or(Value::Null, |n| json!(n));
        context["pointer_decimal"] = v["pointer_address"].clone();
    }
    if warning
        && context
            .as_object()
            .is_some_and(|o| o.values().all(Value::is_null))
    {
        context = Value::Null;
    }
    json!({"id":format!("legacy:{at}"),"kind":v["kind"],"check_id":null,"context_origin":"legacy_saved_report","source_pointer":at,"input_binding":"unverified","caused_by":[],"context":context})
}
fn collect_report_sources(v: &Value, at: &str, warning: bool, out: &mut Vec<Value>) {
    if warning {
        if let Some(warnings) = v["warnings"].as_array() {
            for (index, item) in warnings.iter().enumerate() {
                if item["kind"].is_string() {
                    out.push(producer_record(
                        item,
                        &format!("{at}/warnings/{index}"),
                        true,
                    ));
                }
            }
        }
    } else if v["kind"].is_string()
        && (v["owner_file_hex"].is_string() || v["block_index"].is_u64())
    {
        out.push(producer_record(v, at, false));
    }
    if let Some(array) = v.as_array() {
        for (i, child) in array.iter().enumerate() {
            collect_report_sources(child, &format!("{at}/{i}"), warning, out);
        }
    } else if v.is_object() {
        for key in [
            "error",
            "errors",
            "causes",
            "trace",
            "root_traces",
            "files",
            "roots",
            "saved_report",
            "stdout_report",
        ] {
            if let Some(child) = v.get(key) {
                collect_report_sources(child, &format!("{at}/{key}"), warning, out);
            }
        }
        // Native aggregate errors retain their existing reports here. The
        // failure traversal remains unchanged; warnings use this same extractor.
        if warning && let Some(reports) = v.get("reports") {
            collect_report_sources(reports, &format!("{at}/reports"), true, out);
        }
    }
}
fn legacy_sources(v: &Value, at: &str, out: &mut Vec<Value>) {
    collect_report_sources(v, at, false, out);
}
/// Project fields already carried by a typed result through the same closed
/// legacy-report source extractor/sanitizer, without claiming a saved stream
/// or invocation ledger. Coordinates are relative to the actual caller input.
fn direct_report_sources(
    report: &Value,
    at: &str,
    key: &[u8; 32],
    names: bool,
    origin: &'static str,
) -> Result<Vec<Value>> {
    let mut records = Vec::new();
    legacy_sources(report, at, &mut records);
    for record in &mut records {
        record["context_origin"] = json!("producer_report");
    }
    let mut projected = failures(&json!(records), key, names)?;
    for (record, original) in projected.iter_mut().zip(&records) {
        record["source_pointer"] = original["source_pointer"].clone();
        record["input_binding"] = json!(crate::report_input::DIRECT_BINDING);
        record["input_origin"] = json!(origin);
    }
    Ok(projected)
}
pub(crate) struct Projected {
    pub files: BTreeMap<&'static str, Vec<u8>>,
    pub privacy: Value,
    pub warnings: Vec<&'static str>,
}
pub(crate) fn evidence(
    evidence: &Evidence,
    key: &[u8; 32],
    bundle_id: &str,
    names: bool,
    attachment: Option<Vec<u8>>,
) -> Result<Projected> {
    let receipt = evidence.receipt.as_ref();
    let legacy = evidence.saved_kind == "legacy";
    let partial_delivery = receipt
        .is_some_and(|r| r["binding"]["stdout_delivery_complete"] == false)
        || evidence.stderr_delivery_complete == Some(false);
    let trace_binding = if evidence.trace_verified {
        if partial_delivery {
            "verified_partial_trace_receipt"
        } else {
            "verified_trace_receipt"
        }
    } else {
        "unverified"
    };
    let mut sources = receipt
        .map(|r| failures(&r["failures"], key, names))
        .transpose()?
        .unwrap_or_default();
    for source in &mut sources {
        source["input_origin"] = json!(if evidence.saved_kind == "legacy" {
            "selected_trace_receipt"
        } else {
            "invocation_receipt"
        });
        source["input_binding"] = json!(if evidence.trace_verified {
            trace_binding
        } else {
            evidence.binding
        });
    }
    // Direct typed library evidence: the Error's owned snapshot (direct_error)
    // or the result report's own structural failures (direct_result) feed the
    // SAME failure sanitizers; provenance labels state the library origin.
    if evidence.saved_kind == "direct_error" {
        let records = json!(evidence.direct_sources);
        let mut projected = failures(&records, key, names)?;
        for record in &mut projected {
            record["input_origin"] = json!("engine_error_snapshot");
            record["input_binding"] = json!(crate::report_input::DIRECT_BINDING);
        }
        sources.extend(projected);
        // Aggregate errors can carry a partial typed report even when the
        // aggregate itself has no owner snapshot. Keep that existing evidence;
        // never recapture a source or invent source/ledger coordinates.
        sources.extend(direct_report_sources(
            &evidence.report["reports"],
            "/reports",
            key,
            names,
            "direct_error_report",
        )?);
    } else if evidence.saved_kind == "direct_result" {
        sources.extend(direct_report_sources(
            &evidence.report,
            "",
            key,
            names,
            "direct_result_report",
        )?);
    } else if receipt.is_none() || evidence.saved_kind == "legacy" {
        let mut legacy = Vec::new();
        legacy_sources(&evidence.report, "", &mut legacy);
        let mut projected = failures(&json!(legacy), key, names)?;
        for (record, original) in projected.iter_mut().zip(&legacy) {
            record["source_pointer"] = original["source_pointer"].clone();
            record["input_binding"] = json!("unverified");
            record["input_origin"] = json!("legacy_saved_report");
        }
        sources.extend(projected);
    }
    let direct = matches!(evidence.saved_kind, "direct_error" | "direct_result");
    let mut warning_records = Vec::new();
    collect_report_sources(&evidence.report, "", true, &mut warning_records);
    let (warning_origin, warning_binding, warning_context_origin) = if direct {
        (
            if evidence.saved_kind == "direct_error" {
                "direct_error_report"
            } else {
                "direct_result_report"
            },
            crate::report_input::DIRECT_BINDING,
            "producer_report",
        )
    } else if evidence.saved_kind == "legacy" || receipt.is_none() {
        ("legacy_saved_report", "unverified", "legacy_saved_report")
    } else {
        (
            "invocation_receipt",
            if evidence.trace_verified {
                trace_binding
            } else {
                evidence.binding
            },
            "producer_report",
        )
    };
    for record in &mut warning_records {
        record["context_origin"] = json!(warning_context_origin);
    }
    // Exactly the error sanitizer: catalog replacement, closed coordinates,
    // native-byte path opt-ins and bundle-local HMAC identities.
    let mut warning_sources = diagnostic_records(&json!(warning_records), key, names, true)?;
    for (record, original) in warning_sources.iter_mut().zip(&warning_records) {
        record["source_pointer"] = original["source_pointer"].clone();
        record["input_origin"] = json!(warning_origin);
        record["input_binding"] = json!(warning_binding);
    }
    let producer_source_status = receipt
        .map(|r| {
            selected(
                &r["source"]["status"],
                &["available", "partial", "unavailable"],
            )
        })
        .unwrap_or(Value::Null);
    let source_status = json!(if sources
        .iter()
        .chain(&warning_sources)
        .any(|f| f["context"].is_object())
    {
        "partial"
    } else {
        "unavailable"
    });
    let mut warnings = vec![
        "Review every exported member; no automatic upload or filing",
        "Receipt/trace hashes are content bindings, not producer authentication",
    ];
    if names {
        warnings.push("NAMES OPT-IN: typed source names/path hex and selected trace paths are disclosed; private strings can appear there");
    }
    if attachment.is_some() {
        warnings.push("WHOLE FILE OPT-IN: all user-supplied file bytes are disclosed. Attachment format and relation to the failed run are UNVERIFIED; preview shows size/hash, not a safe-content or scene-identity assertion");
    }
    if receipt.is_some_and(|r| r["binding"]["stdout_delivery_complete"] == false) {
        warnings.push("STDOUT DELIVERY INCOMPLETE: only accepted bytes were bound; report coverage is partial, and publication effects are independent. Never retry a write automatically.");
    }
    if evidence.stderr_delivery_complete == Some(false) {
        warnings.push("STDERR DELIVERY INCOMPLETE: the selected trace retained the receipt but no complete final envelope delivery is claimed; coverage is partial.");
    }
    if evidence.binding == "unverified" {
        warnings
            .push("Legacy or interrupted evidence: binding unverified, final completion unknown");
    }
    if direct {
        warnings.push("DIRECT LIBRARY EVIDENCE: typed in-process caller input; no CLI invocation, ledger ID, saved stream or trace binds it; provenance is the caller's assertion");
    }
    let mut dropped_fields = Vec::new();
    omissions(&evidence.report, "/report", key, &mut dropped_fields)?;
    for (i, event) in evidence.events.iter().enumerate() {
        omissions(event, &format!("/trace/{i}"), key, &mut dropped_fields)?;
    }
    let privacy = json!({"profile":"structural","include_names":names,"include_file":attachment.is_some(),"pseudonym_scheme":"HMAC-SHA-256/bundle-local","dropped_fields":dropped_fields,"limitations":["not a secret detector","saved structural coordinates/pointers disclose allocation topology","only instrumented resolver checks provide source context; missing facts are null","names/file opt-ins can disclose private data"]});
    let report_receipt = if legacy { None } else { receipt };
    let mut run_metadata = json!({"run_token":identity(&json!(evidence.run_id),key,false)?,"command":receipt.map(|r|command(&r["command"])),"binding":evidence.binding,"completion":evidence.completion,"exit_code":receipt.and_then(|r|r["exit_code"].as_i64()),"stdout_delivery_complete":receipt.and_then(|r|r["binding"]["stdout_delivery_complete"].as_bool()),"stderr_delivery_complete":evidence.stderr_delivery_complete,"stdout_bytes":receipt.and_then(|r|r["binding"]["stdout_bytes"].as_u64()),"outcome":receipt.map(|r|selected(&r["outcome"],&["ok","error","partial","refused","cancelled","unknown"]))});
    run_metadata["scope"] = json!(if direct {
        "direct typed library evidence; no CLI invocation"
    } else if legacy {
        "legacy_saved_report; producer invocation unknown"
    } else {
        "selected invocation receipt"
    });
    let selected_trace_run = if legacy && !evidence.events.is_empty() {
        let mut selected = run_metadata.clone();
        selected["scope"] = json!("selected trace only; NOT bound to legacy saved report");
        selected["binding"] = json!(trace_binding);
        selected["completion"] = json!(if evidence.trace_verified {
            if partial_delivery {
                "partial"
            } else {
                "complete"
            }
        } else {
            "unknown"
        });
        for key in [
            "run_token",
            "command",
            "exit_code",
            "stdout_delivery_complete",
            "stderr_delivery_complete",
            "stdout_bytes",
            "outcome",
        ] {
            run_metadata[key] = Value::Null;
        }
        warnings.push("Legacy saved report and selected trace are independent evidence; their metadata is separated and no same-invocation claim is made.");
        selected
    } else {
        Value::Null
    };
    let mut build = safe_build(receipt);
    if legacy && receipt.is_some() {
        build["selected_trace_producer"] = std::mem::replace(&mut build["producer"], Value::Null);
    }
    let mut files = BTreeMap::new();
    let mut report_member = json!({"schema":"oxblend.redacted-error-report/v1","source_schema":family(&evidence.report["schema"]),"stdout_schema":family(&evidence.report["stdout_report"]["schema"]),"saved_schema":family(&evidence.report["saved_report"]["schema"]),"saved_kind":evidence.saved_kind,"binding":evidence.binding,"completion":evidence.completion,"exit_code":report_receipt.and_then(|r|r["exit_code"].as_i64()),"stdout_delivery_complete":report_receipt.and_then(|r|r["binding"]["stdout_delivery_complete"].as_bool()),"stderr_delivery_complete":if legacy{None}else{evidence.stderr_delivery_complete},"stdout_bytes":report_receipt.and_then(|r|r["binding"]["stdout_bytes"].as_u64()),"operation_state":report_receipt.map(|r|project(&r["operation_state"],key)).transpose()?,"report":project(&evidence.report,key)?,"failures":sources});
    let mut source_member = json!({"schema":"oxblend.error-source/v1","status":source_status,"producer_status":producer_source_status,"coverage":"instrumented owner checks and retained typed producer-report fields; no recapture","failures":sources});
    // Optional additive v1 field: no-warning inputs retain their old shape.
    if !warning_sources.is_empty() {
        report_member["warnings"] = json!(warning_sources);
        source_member["warnings"] = json!(warning_sources);
    }
    files.insert("error-report.json", json_bytes(&report_member));
    files.insert("error-source.json", json_bytes(&source_member));
    files.insert("build.json", json_bytes(&build));
    files.insert("README.md",format!("Structural diagnostic derivative, not original evidence. Binding: {}. Trace: {}. Source coverage: instrumented checks only. Saved addresses/topology may be exposed. Names/file inclusion is declared in manifest; inspect every member. Nothing is uploaded. No scene validity, correctness of repair or renderability is certified.\n",evidence.binding,evidence.trace_status).into_bytes());
    files.insert("issue.md",format!("# Oxblend diagnostic report\n\nDescribe the needed outcome and actual result. Attach this reviewed ZIP manually. Binding: {}. Trace: {}. Source evidence is partial or unavailable outside instrumented checks. See manifest privacy/consent before sharing.\n",evidence.binding,evidence.trace_status).into_bytes());
    if !evidence.events.is_empty() {
        files.insert("trace/events.jsonl", trace(&evidence.events, key, names)?);
    }
    if let Some(bytes) = attachment {
        files.insert("attachments/user-supplied.bin", bytes);
    }
    let inventory=files.iter().map(|(name,bytes)|Ok(json!({"path":name,"bytes":bytes.len(),"sha256":hash(bytes)?,"role":match *name{"attachments/user-supplied.bin"=>"user_supplied_unverified_whole_file","trace/events.jsonl"=>"redacted_trace",_=>"structural_derivative"}}))).collect::<Result<Vec<_>>>()?;
    files.insert("manifest.json",json_bytes(&json!({"schema":"oxblend.bug-bundle/v1","bundle_id":bundle_id,"created_at":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d|d.as_secs()),"created_at_unit":"unix_seconds_utc","producer_run":run_metadata,"selected_trace_run":selected_trace_run,"trace":{"status":evidence.trace_status,"binding":trace_binding,"format":"flattrace/v1","coverage":if evidence.events.is_empty(){"unavailable"}else{"selected run only; closed redacted derivative"}},"evidence":{"catalog_version":env!("CARGO_PKG_VERSION"),"source_status":source_status,"report_binding":evidence.binding,"source_identity_verified":false,"attachment_binding":if files.contains_key("attachments/user-supplied.bin"){"user_supplied_unverified; not asserted to be the selected run input"}else{"not_included"}},"privacy":privacy,"consent":{"names_opt_in":names,"file_opt_in":files.contains_key("attachments/user-supplied.bin")},"warnings":warnings,"members":inventory})));
    Ok(Projected {
        files,
        privacy,
        warnings,
    })
}
