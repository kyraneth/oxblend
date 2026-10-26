//! Read explicitly selected local evidence. No producer rerun, recapture or upload.
use crate::{Error, Result, bat_cli::hash_bytes};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

fn usage(s: &str) -> Error {
    Error::new("usage", s)
}
pub(crate) struct Evidence {
    pub report: Value,
    pub receipt: Option<Value>,
    pub events: Vec<Value>,
    pub binding: &'static str,
    pub completion: &'static str,
    pub trace_status: &'static str,
    pub chronology: Option<String>,
    pub run_id: Option<String>,
    pub saved_kind: &'static str,
    pub trace_verified: bool,
    pub source_sha256: Option<String>,
    pub local_diagnostics: Vec<Value>,
    pub stderr_delivery_complete: Option<bool>,
    // Owned typed library evidence (no CLI invocation): ledger-shaped records
    // from Error source snapshots. Empty for every file-selected evidence.
    pub direct_sources: Vec<Value>,
}
/// Honest binding label for direct typed library evidence: no CLI invocation,
/// ledger or saved stream exists to bind, and none is fabricated.
pub(crate) const DIRECT_BINDING: &str = "library call; no CLI invocation binding";
/// Direct typed evidence from a raised engine error and its owned source
/// snapshot. No CLI envelope, receipt, ledger or saved report is fabricated;
/// the envelope states its own origin and the snapshot stays the only source
/// of structural failure records.
pub(crate) fn direct_error(
    kind: &str,
    message: &str,
    snapshot: Option<&crate::failure_receipt::SourceSnapshot>,
    diagnostics: &[Value],
    report: Option<Value>,
) -> Evidence {
    let mut envelope = json!({
        "schema":"oxblend.direct-evidence/v1",
        "origin":"raised_error",
        "error":{"kind":kind,"message":message},
    });
    if !diagnostics.is_empty() {
        envelope["operation_diagnostics"] = json!(diagnostics);
    }
    if let Some(report) = report {
        envelope["reports"] = json!([report]);
    }
    Evidence {
        report: envelope,
        receipt: None,
        events: Vec::new(),
        binding: DIRECT_BINDING,
        completion: "unknown",
        trace_status: "unavailable",
        chronology: None,
        run_id: None,
        saved_kind: "direct_error",
        trace_verified: false,
        source_sha256: None,
        local_diagnostics: Vec::new(),
        stderr_delivery_complete: None,
        direct_sources: snapshot.map(|s| vec![s.record_json()]).unwrap_or_default(),
    }
}
/// Direct typed evidence from an owned result report (for example one
/// carrying warnings/errors), projected through the same sanitizers as a
/// saved CLI report without claiming any invocation provenance.
pub(crate) fn direct_result(report: Value) -> Evidence {
    Evidence {
        report,
        receipt: None,
        events: Vec::new(),
        binding: DIRECT_BINDING,
        completion: "unknown",
        trace_status: "unavailable",
        chronology: None,
        run_id: None,
        saved_kind: "direct_result",
        trace_verified: false,
        source_sha256: None,
        local_diagnostics: Vec::new(),
        stderr_delivery_complete: None,
        direct_sources: Vec::new(),
    }
}
fn receipt_from(report: &Value) -> Option<Value> {
    if report["schema"] == "oxblend.failure-receipt/v1" {
        Some(report.clone())
    } else if report["diagnostic"]["schema"] == "oxblend.failure-receipt/v1" {
        Some(report["diagnostic"].clone())
    } else {
        None
    }
}
fn canonical(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).expect("JSON value")
}
pub(crate) fn validate_receipt(receipt: &Value) -> Result<()> {
    if receipt["schema"] != "oxblend.failure-receipt/v1" {
        return Err(usage("failure receipt schema required"));
    }
    let id = receipt["run_id"]
        .as_str()
        .ok_or_else(|| usage("receipt run ID missing"))?;
    if !valid_id(id) {
        return Err(usage("invalid receipt run ID"));
    }
    if receipt["command"]
        .as_str()
        .and_then(crate::bat_cli::builtin_command)
        .is_none()
    {
        return Err(usage("unrecognized receipt command"));
    }
    let identity = &receipt["trace_identity"];
    if !identity.is_null()
        && (identity["identity_kind"] != "trace_root_span"
            || !identity["trace"].as_str().is_some_and(valid_id)
            || !identity["root_span"].as_str().is_some_and(|s| {
                s.len() == 16
                    && s.bytes().all(|b| b.is_ascii_hexdigit())
                    && s.bytes().any(|b| b != b'0')
            }))
    {
        return Err(usage("invalid trace/root identity shape in receipt"));
    }
    if receipt["producer"]["name"] != "oxblend"
        || !receipt["producer"]["version"].is_string()
        || !matches!(
            receipt["source"]["status"].as_str(),
            Some("available" | "partial" | "unavailable")
        )
    {
        return Err(usage("receipt producer/source coverage missing or invalid"));
    }
    if receipt["binding"]["status"] != "invocation_bound" {
        return Err(usage("receipt is not invocation-bound"));
    }
    for field in ["stdout_sha256", "stderr_error_sha256"] {
        if !receipt["binding"][field]
            .as_str()
            .is_some_and(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(usage("receipt evidence digest missing/invalid"));
        }
    }
    let failures = receipt["failures"]
        .as_array()
        .ok_or_else(|| usage("receipt failures missing"))?;
    let terminal = failures
        .last()
        .ok_or_else(|| usage("failure receipt has no terminal failure"))?;
    let terminal_kind = terminal["kind"]
        .as_str()
        .ok_or_else(|| usage("terminal kind missing"))?;
    if receipt["outcome"] != crate::diagnostics::outcome_kind(terminal_kind) {
        return Err(usage("terminal kind and selected outcome disagree"));
    }
    let expected_exit = if terminal_kind == "pack_project" {
        5
    } else if matches!(terminal_kind, "pack_input" | "missing_input") {
        3
    } else {
        2
    };
    if receipt["exit_code"].as_i64() != Some(expected_exit)
        || !matches!(
            receipt["outcome"].as_str(),
            Some("error" | "partial" | "refused")
        )
        || !receipt["signal"].is_null()
        || receipt["binding"]["stdout_bytes"].as_u64().is_none()
        || !receipt["binding"]["stdout_delivery_complete"].is_boolean()
    {
        return Err(usage(
            "receipt final outcome/exit/delivery facts are inconsistent or missing",
        ));
    }
    let mut ids = BTreeSet::new();
    for failure in failures {
        if !failure["kind"].is_string() {
            return Err(usage("failure kind must be a string"));
        }
        let id = failure["id"]
            .as_str()
            .ok_or_else(|| usage("failure ID missing"))?;
        if ids.contains(id) {
            return Err(usage("duplicate failure ID"));
        }
        for cause in failure["caused_by"]
            .as_array()
            .ok_or_else(|| usage("failure causal edges missing"))?
        {
            // Producer hook only references already-recorded children. This
            // ordering proves acyclicity without a depth budget or recursion.
            if !cause.as_str().is_some_and(|c| ids.contains(c)) {
                return Err(usage("unknown/forward/cyclic failure edge"));
            }
        }
        ids.insert(id);
    }
    Ok(())
}
fn valid_id(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()) && s.bytes().any(|b| b != b'0')
}
// flattrace 0.1 wall() emits fixed-width UTC with nanoseconds (src/flattrace.rs).
// This is an external format width, not a workload limit. Reject an unorderable
// chronology for --last rather than using filesystem timestamps.
fn recorded_time(value: &Value) -> Option<String> {
    let s = value.as_str()?;
    let b = s.as_bytes();
    if b.len() != 30
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'.'
        || b[29] != b'Z'
    {
        return None;
    }
    if b.iter()
        .enumerate()
        .any(|(i, b)| ![4, 7, 10, 13, 16, 19, 29].contains(&i) && !b.is_ascii_digit())
    {
        return None;
    }
    let number = |a: usize, z: usize| s[a..z].parse::<u32>().ok();
    let (month, day, hour, minute, second) = (
        number(5, 7)?,
        number(8, 10)?,
        number(11, 13)?,
        number(14, 16)?,
        number(17, 19)?,
    );
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    Some(s.to_owned())
}
pub(crate) fn saved(path: &Path) -> Result<Evidence> {
    let report: Value =
        serde_json::from_slice(&crate::report::read_regular(path)?).map_err(|_| {
            usage("one complete JSON report/envelope required; mixed stderr is not JSON")
        })?;
    let accepted = (report["schema"] == 1
        || report["schema"] == 2
        || matches!(
            report["schema"].as_str(),
            Some("oxblend.diff/v1" | "oxblend.pointer-census/v2")
        ))
        && (report["error"].is_object()
            || report["errors"].is_array()
            || report["complete"].is_boolean()
            || report["semantic_verified"].is_boolean());
    let receipt = receipt_from(&report);
    let saved_kind = if report["schema"] == "oxblend.failure-receipt/v1" {
        "receipt"
    } else if receipt.is_some() && report["schema"] == 1 && report["error"].is_object() {
        "error_envelope"
    } else {
        "legacy"
    };
    if receipt.is_some() && saved_kind == "legacy" {
        return Err(usage(
            "a diagnostic receipt must be supplied as its exact schema-1 error envelope or standalone failure receipt, not attached to an unrelated report",
        ));
    }
    if !accepted && receipt.is_none() {
        return Err(usage("unsupported saved report family"));
    }
    if let Some(receipt) = &receipt {
        validate_receipt(receipt)?;
        if report.get("error").is_some() {
            if receipt["failures"]
                .as_array()
                .and_then(|a| a.last())
                .map(|v| &v["kind"])
                != Some(&report["error"]["kind"])
            {
                return Err(usage("saved error kind and terminal receipt kind differ"));
            }
            let mut base = report.clone();
            base.as_object_mut().unwrap().remove("diagnostic");
            let mut bytes = canonical(&base);
            bytes.push(b'\n');
            if receipt["binding"]["stderr_error_sha256"] != hash_bytes(&bytes)? {
                return Err(Error::new(
                    "input_changed",
                    "saved error envelope does not match its invocation receipt",
                ));
            }
        }
    }
    let run_id = receipt
        .as_ref()
        .and_then(|v| v["run_id"].as_str().map(str::to_owned));
    Ok(Evidence {
        report,
        receipt,
        events: Vec::new(),
        binding: "unverified",
        completion: "unknown",
        trace_status: "unavailable",
        chronology: None,
        run_id,
        saved_kind,
        trace_verified: false,
        source_sha256: None,
        local_diagnostics: Vec::new(),
        stderr_delivery_complete: None,
        direct_sources: Vec::new(),
    })
}
/// Envelope/ancestry validation is independent of L5's semantic command gate.
/// Interrupted traces stay explicit and require exact selection, never --last.
pub(crate) fn trace(path: &Path, supplied: Option<Evidence>) -> Result<Evidence> {
    let bytes = crate::report::read_regular(path)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| usage("trace must be UTF-8 JSONL"))?;
    let lines = text.lines().collect::<Vec<_>>();
    let mut events = Vec::new();
    let mut interrupted_tail = false;
    for (index, line) in lines.iter().enumerate() {
        match serde_json::from_str::<Value>(line) {
            Ok(event) => events.push(event),
            Err(_) if index + 1 == lines.len() && !text.ends_with('\n') => {
                interrupted_tail = true;
            }
            Err(_) => {
                return Err(usage(
                    "invalid trace JSONL record before the trailing boundary",
                ));
            }
        }
    }
    let first = events.first().ok_or_else(|| usage("empty trace"))?;
    let raw_run = first["run"]
        .as_str()
        .filter(|s| valid_id(s))
        .ok_or_else(|| usage("trace run ID invalid"))?;
    let raw_trace = first["trace"]
        .as_str()
        .filter(|s| valid_id(s))
        .ok_or_else(|| usage("trace identity invalid"))?;
    if first["v"] != 1
        || first["event"] != "run"
        || first["state"] != "start"
        || !first["parent"].is_null()
    {
        return Err(usage("trace root start required"));
    }
    let root_span = first["span"]
        .as_str()
        .ok_or_else(|| usage("root span missing"))?;
    let mut active = BTreeMap::<String, (Option<String>, String)>::new();
    let mut seen = BTreeSet::new();
    let mut receipt = None;
    let mut final_root = None;
    let mut invocation = None;
    for (i, event) in events.iter().enumerate() {
        if event["v"] != 1
            || event["seq"].as_u64() != Some(i as u64)
            || event["run"] != raw_run
            || event["trace"] != raw_trace
        {
            return Err(usage("trace identity/sequence mismatch"));
        }
        let span = event["span"]
            .as_str()
            .ok_or_else(|| usage("span identity missing"))?;
        let parent = event["parent"].as_str();
        match event["state"].as_str() {
            Some("start") => {
                if seen.contains(span)
                    || (i != 0 && !parent.is_some_and(|p| active.contains_key(p)))
                {
                    return Err(usage("duplicate span or inactive parent"));
                }
                seen.insert(span.to_owned());
                active.insert(
                    span.to_owned(),
                    (
                        parent.map(str::to_owned),
                        event["event"]
                            .as_str()
                            .ok_or_else(|| usage("event name missing"))?
                            .to_owned(),
                    ),
                );
            }
            Some("end") => {
                let Some(expected_parent) = active.get(span) else {
                    return Err(usage("end without active start"));
                };
                if expected_parent.0.as_deref() != parent
                    || event["event"] != expected_parent.1
                    || active.values().any(|p| p.0.as_deref() == Some(span))
                {
                    return Err(usage("trace parent mismatch/unfinished child"));
                }
                active.remove(span);
                if span == root_span {
                    if i + 1 != events.len() {
                        return Err(usage("records after root end"));
                    }
                    final_root = Some(event);
                }
            }
            Some("point") => {
                if active.get(span).is_none_or(|p| p.0.as_deref() != parent) {
                    return Err(usage("point outside active scope or wrong parent"));
                }
            }
            _ => return Err(usage("unknown trace state")),
        }
        if event["event"] == "diagnostic.invocation"
            && invocation.replace(event["data"].clone()).is_some()
        {
            return Err(usage("ambiguous invocation binding"));
        }
        if event["event"] == "diagnostic.receipt" {
            let candidate = event["data"]["receipt"].clone();
            validate_receipt(&candidate)?;
            if event["data"]["sha256"] != hash_bytes(&canonical(&candidate))? {
                return Err(Error::new("input_changed", "trace receipt digest mismatch"));
            }
            if receipt.replace(candidate).is_some() {
                return Err(usage("ambiguous diagnostic receipt"));
            }
        }
    }
    if interrupted_tail && final_root.is_some() {
        return Err(usage(
            "unparsed bytes after root completion; selected trace is not trustworthy",
        ));
    }
    let mut evidence = if let Some(evidence) = supplied {
        evidence
    } else {
        Evidence {
            report: json!({}),
            receipt: None,
            events: Vec::new(),
            binding: "unverified",
            completion: "unknown",
            trace_status: "available",
            chronology: None,
            run_id: None,
            saved_kind: "none",
            trace_verified: false,
            source_sha256: None,
            local_diagnostics: Vec::new(),
            stderr_delivery_complete: None,
            direct_sources: Vec::new(),
        }
    };
    if let (Some(trace_receipt), Some(saved_receipt)) = (&receipt, &evidence.receipt) {
        if trace_receipt != saved_receipt {
            return Err(Error::new(
                "input_changed",
                "trace and saved report belong to different invocation receipts",
            ));
        }
    } else if evidence.receipt.is_some() && receipt.is_none() {
        return Err(usage(
            "selected trace lacks the saved report's invocation receipt",
        ));
    }
    if let Some(r) = receipt {
        let inv = invocation
            .as_ref()
            .ok_or_else(|| usage("trace receipt lacks invocation start binding"))?;
        if r["trace_identity"]["trace"] != raw_trace
            || r["trace_identity"]["root_span"] != root_span
            || r["trace_identity"]["identity_kind"] != "trace_root_span"
            || inv["trace_identity"] != r["trace_identity"]
        {
            return Err(usage("receipt is not bound to this trace/root identity"));
        }
        if inv["run_id"] != r["run_id"]
            || inv["command"] != r["command"]
            || first["data"]["command"] != r["command"]
        {
            return Err(usage("trace command/invocation identity mismatch"));
        }
        if let Some(end) = final_root {
            if end["outcome"] != r["outcome"] {
                return Err(usage("receipt/final root outcome mismatch"));
            }
            evidence.trace_verified = true;
            if evidence.saved_kind != "legacy" {
                let delivered = r["binding"]["stdout_delivery_complete"] == true;
                evidence.binding = if delivered {
                    "verified_trace_receipt"
                } else {
                    "verified_partial_trace_receipt"
                };
                evidence.completion = if delivered { "complete" } else { "partial" };
            }
        }
        evidence.run_id = r["run_id"].as_str().map(str::to_owned);
        if evidence.report.as_object().is_none_or(|o| o.is_empty()) {
            evidence.report = r.clone();
        }
        evidence.receipt = Some(r);
    }
    if let Some(end) = final_root {
        evidence.chronology = recorded_time(&end["t"])
            .filter(|end| recorded_time(&first["t"]).is_some_and(|start| start <= *end));
    }
    evidence.trace_status = if final_root.is_some() {
        "available"
    } else {
        "partial"
    };
    // --run-id has one namespace: the product invocation ID. The flattrace
    // envelope run field is not an alias. Interrupted new runs retain the
    // invocation start ID; legacy traces require explicit --trace selection.
    if evidence.run_id.is_none() {
        evidence.run_id = invocation
            .as_ref()
            .and_then(|v| v["run_id"].as_str())
            .filter(|s| valid_id(s))
            .map(str::to_owned);
    }
    if interrupted_tail {
        evidence.local_diagnostics.push(local_diagnostic(
            path,
            "interrupted final JSONL record omitted; completion remains unknown",
        ));
    }
    evidence.stderr_delivery_complete = events
        .iter()
        .rev()
        .find(|e| e["event"] == "root.summary")
        .and_then(|e| e["data"]["diagnostic_stderr_delivered"].as_bool());
    if evidence.stderr_delivery_complete == Some(false)
        && evidence.trace_verified
        && evidence.saved_kind != "legacy"
    {
        evidence.binding = "verified_partial_trace_receipt";
        evidence.completion = "partial";
    }
    evidence.source_sha256 = Some(hash_bytes(&bytes)?);
    evidence.events = events;
    Ok(evidence)
}
fn claims_invocation(path: &Path, id: &str) -> bool {
    let Ok(bytes) = crate::report::read_regular(path) else {
        return false;
    };
    bytes
        .split(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .any(|e| {
            (e["event"] == "diagnostic.invocation" && e["data"]["run_id"] == id)
                || (e["event"] == "diagnostic.receipt" && e["data"]["receipt"]["run_id"] == id)
        })
}
fn local_diagnostic(path: &Path, reason: &str) -> Value {
    json!({"path":path.to_str(),"path_hex":crate::value::hex(path.as_os_str().as_encoded_bytes()),"reason":reason})
}
pub(crate) fn select(directory: &Path, id: Option<&str>, last: bool) -> Result<Evidence> {
    use std::os::unix::fs::MetadataExt;
    if let Some(id) = id
        && !valid_id(id)
    {
        return Err(usage(
            "--run-id requires the exact 32-hex product invocation ID from diagnostic.run_id; it is not the flattrace envelope run field",
        ));
    }
    let directory_meta =
        std::fs::symlink_metadata(directory).map_err(|e| Error::new("io", e.to_string()))?;
    if !directory_meta.is_dir() {
        return Err(usage(
            "--trace-dir must be an explicit regular directory, not a symlink",
        ));
    }
    let mut candidates = Vec::new();
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in std::fs::read_dir(directory).map_err(|e| Error::new("io", e.to_string()))? {
        let entry = entry.map_err(|e| Error::new("io", e.to_string()))?;
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|e| Error::new("io", e.to_string()))?;
        if !metadata.is_file() || metadata.uid() != unsafe { libc::geteuid() } {
            diagnostics.push(local_diagnostic(
                &path,
                "not an invoking-user-owned regular trace file; skipped without following links",
            ));
            continue;
        }
        let evidence = match trace(&path, None) {
            Ok(evidence) => evidence,
            Err(error) => {
                if id.is_some_and(|id| claims_invocation(&path, id)) {
                    return Err(Error::new(
                        "usage",
                        format!(
                            "selected invocation has an invalid/ambiguous artifact {}: {}",
                            path.display(),
                            error.message
                        ),
                    ));
                }
                diagnostics.push(local_diagnostic(
                    &path,
                    &format!("unvalidated non-candidate skipped: {}", error.message),
                ));
                continue;
            }
        };
        let hash = evidence
            .source_sha256
            .clone()
            .expect("trace captures its source hash");
        if !seen.insert(hash) {
            diagnostics.push(local_diagnostic(
                &path,
                "byte-identical trace duplicate ignored",
            ));
            continue;
        }
        let first = &evidence.events[0];
        if matches!(
            first["data"]["command"].as_str(),
            Some("report" | "help" | "skill" | "version")
        ) {
            continue;
        }
        if last && first["data"]["command"] == "invalid" {
            continue;
        }
        if let Some(id) = id {
            if evidence.run_id.as_deref() == Some(id) {
                candidates.push(evidence);
            }
        } else if last
            && evidence.trace_status == "available"
            && evidence.trace_verified
            && evidence.chronology.is_some()
            && evidence.receipt.as_ref().is_some_and(|r| {
                matches!(r["outcome"].as_str(), Some("error" | "partial" | "refused"))
            })
        {
            candidates.push(evidence);
        } else if last && (evidence.trace_status != "available" || evidence.chronology.is_none()) {
            diagnostics.push(local_diagnostic(&path,"interrupted/unorderable run is not eligible for --last; select its exact invocation ID or explicit trace"));
        }
    }
    let mut selected = if last {
        candidates.sort_by(|a, b| a.chronology.cmp(&b.chronology));
        if candidates.len() > 1
            && candidates[candidates.len() - 1].chronology
                == candidates[candidates.len() - 2].chronology
        {
            return Err(usage(
                "distinct runs have equal recorded chronology; select an exact --run-id",
            ));
        }
        candidates.pop().ok_or_else(||Error::new("usage",format!("no validated completed failed/partial product invocation in this directory ({} artifacts skipped); use explicit --trace or saved --report evidence",diagnostics.len())))?
    } else if candidates.len() == 1 {
        candidates.pop().unwrap()
    } else if candidates.is_empty() {
        return Err(Error::new(
            "usage",
            format!(
                "selected product invocation ID not found ({} artifacts skipped); no envelope-ID alias or fallback",
                diagnostics.len()
            ),
        ));
    } else {
        return Err(usage(
            "multiple nonidentical artifacts claim the selected invocation; choose an explicit --trace",
        ));
    };
    selected.local_diagnostics.extend(diagnostics);
    Ok(selected)
}
