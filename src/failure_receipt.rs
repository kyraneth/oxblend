//! Invocation-bound local failure evidence; never uploads or reopens scene files.
use crate::{
    Error,
    bat_cli::{Sha256, hash_bytes},
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

#[derive(Debug, Clone)]
pub(crate) struct DiagnosticRef {
    pub run_id: String,
    pub failure_id: String,
}
/// Typed owned source-diagnostic snapshot: the same producer data the CLI
/// invocation ledger retains, held by the `Error` itself so library callers
/// without `begin` keep identical evidence. Local error metadata, never a
/// fabricated receipt or scoped/reentrant diagnostic owner.
#[derive(Debug, Clone)]
pub struct SourceSnapshot {
    /// Error kind at first capture; later reclassifications propagate.
    pub kind: &'static str,
    /// First producer message, before presentation/reclassification.
    pub message: String,
    /// Instrumented owner check that first captured this evidence.
    pub check_id: &'static str,
    /// Typed structural source context (paths stay local; export sanitizes).
    pub context: SourceContext,
    pub caused_by: Vec<String>,
    pub propagations: Vec<SourcePropagation>,
}
#[derive(Debug, Clone)]
pub struct SourcePropagation {
    pub kind: &'static str,
    pub check_id: &'static str,
    pub context: SourceContext,
}
impl SourceSnapshot {
    /// Ledger-shaped failure record for the shared report sanitizers. It has
    /// no `id`: no invocation ledger exists to name one, and none is invented.
    pub fn record_json(&self) -> Value {
        json!({"kind":self.kind,"message":self.message,"check_id":self.check_id,"context_origin":"owner_hook","caused_by":self.caused_by,"context":self.context.json(),"propagations":self.propagations.iter().map(|p|json!({"kind":p.kind,"check_id":p.check_id,"context":p.context.json()})).collect::<Vec<_>>()})
    }
}
/// Mirror the ledger's carried-reference update on the owned snapshot: same
/// exact site enriches missing typed context; a differing kind/check_id
/// appends a typed propagation. Runs regardless of any CLI ledger state.
pub(crate) fn capture_snapshot(
    error: &Error,
    check_id: &'static str,
    source: &SourceContext,
    caused_by: &[String],
) {
    if let Some(existing) = error.source_snapshot.get() {
        let Ok(mut snapshot) = existing.lock() else {
            return;
        };
        let same_site = snapshot.context.file.is_some()
            && snapshot.context.file == source.file
            && snapshot.context.block_index == source.block_index
            && snapshot.context.pointer == source.pointer;
        if same_site {
            enrich_context(&mut snapshot.context, source);
        }
        if snapshot.kind != error.kind || snapshot.check_id != check_id {
            snapshot.propagations.push(SourcePropagation {
                kind: error.kind,
                check_id,
                context: source.clone(),
            });
        }
        return;
    }
    let _ = error
        .source_snapshot
        .set(std::sync::Arc::new(std::sync::Mutex::new(SourceSnapshot {
            kind: error.kind,
            message: error.message.clone(),
            check_id,
            context: source.clone(),
            caused_by: caused_by.to_vec(),
            propagations: Vec::new(),
        })));
}
fn enrich_context(into: &mut SourceContext, from: &SourceContext) {
    fn take<T: Clone>(slot: &mut Option<T>, value: &Option<T>) {
        if slot.is_none() && value.is_some() {
            *slot = value.clone();
        }
    }
    take(&mut into.block_index, &from.block_index);
    take(&mut into.block_code, &from.block_code);
    take(&mut into.old_address, &from.old_address);
    take(&mut into.sdna_index, &from.sdna_index);
    take(&mut into.count, &from.count);
    take(&mut into.payload_size, &from.payload_size);
    take(&mut into.data_offset, &from.data_offset);
    take(&mut into.element, &from.element);
    take(&mut into.offset_in_block, &from.offset_in_block);
    take(&mut into.decoded_file_offset, &from.decoded_file_offset);
    take(&mut into.pointer_width, &from.pointer_width);
    take(&mut into.blend_version, &from.blend_version);
    take(&mut into.little_endian, &from.little_endian);
    take(&mut into.pointer, &from.pointer);
    if into.field_path.is_empty() {
        into.field_path = from.field_path.clone();
    }
    if into.namespace == Namespace::Unknown && from.namespace != Namespace::Unknown {
        into.namespace = from.namespace;
    }
    if into.resolution == Resolution::Unknown && from.resolution != Resolution::Unknown {
        into.resolution = from.resolution;
    }
}
pub(crate) fn reference_json(reference: Option<&std::sync::Arc<DiagnosticRef>>) -> Value {
    reference.map_or(
        Value::Null,
        |r| json!({"run_id":r.run_id,"failure_id":r.failure_id}),
    )
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Namespace {
    Id,
    OwnerData,
    QualifiedCrossData,
    GlobalDiagnostic,
    #[default]
    Unknown,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Resolution {
    Missing,
    Ambiguous,
    WrongType,
    WrongExtent,
    Unprofiled,
    #[default]
    Unknown,
}
#[derive(Debug, Clone, Default)]
pub struct SourceContext {
    pub file: Option<PathBuf>,
    pub block_index: Option<usize>,
    pub block_code: Option<[u8; 4]>,
    pub old_address: Option<u64>,
    pub sdna_index: Option<usize>,
    pub count: Option<usize>,
    pub payload_size: Option<usize>,
    pub data_offset: Option<usize>,
    pub element: Option<usize>,
    pub offset_in_block: Option<usize>,
    pub decoded_file_offset: Option<usize>,
    pub pointer_width: Option<usize>,
    pub blend_version: Option<u32>,
    pub little_endian: Option<bool>,
    pub field_path: Vec<Vec<u8>>,
    pub pointer: Option<u64>,
    pub namespace: Namespace,
    pub resolution: Resolution,
}
impl SourceContext {
    pub fn block(file: &crate::BlendFile, index: usize) -> Self {
        let Some(b) = file.blocks().get(index) else {
            return Self::default();
        };
        Self {
            file: file.source_path().map(std::path::Path::to_path_buf),
            block_index: Some(index),
            block_code: Some(b.code),
            old_address: Some(b.old_address),
            sdna_index: Some(b.sdna_index),
            count: Some(b.count),
            payload_size: Some(b.size),
            data_offset: Some(b.data_offset),
            pointer_width: Some(file.header().pointer_size),
            blend_version: Some(file.header().version),
            little_endian: Some(file.header().little_endian),
            ..Self::default()
        }
    }
    fn json(&self) -> Value {
        json!({"file_hex":self.file.as_ref().map(|p|crate::value::hex(p.as_os_str().as_encoded_bytes())),"block_index":self.block_index,"block_code_hex":self.block_code.map(|v|crate::value::hex(&v)),"old_address":self.old_address.map(|v|v.to_string()),"sdna_index":self.sdna_index,"count":self.count,"payload_size":self.payload_size,"data_offset":self.data_offset,"element":self.element,"offset_in_block":self.offset_in_block,"decoded_file_offset":self.decoded_file_offset,"width":self.pointer_width,"blend_version":self.blend_version,"little_endian":self.little_endian,"field_path_hex":self.field_path.iter().map(|v|crate::value::hex(v)).collect::<Vec<_>>(),"pointer_decimal":self.pointer.map(|v|v.to_string()),"namespace":match self.namespace {Namespace::Id=>"id",Namespace::OwnerData=>"owner_data",Namespace::QualifiedCrossData=>"qualified_cross_data",Namespace::GlobalDiagnostic=>"global_diagnostic",Namespace::Unknown=>"unknown"},"resolution":match self.resolution {Resolution::Missing=>"missing",Resolution::Ambiguous=>"ambiguous",Resolution::WrongType=>"wrong_type",Resolution::WrongExtent=>"wrong_extent",Resolution::Unprofiled=>"unprofiled",Resolution::Unknown=>"unknown"}})
    }
}
fn decorate(record: &mut Value) {
    if let Some(entry) = record["kind"].as_str().and_then(crate::error_help::entry) {
        for field in ["explanation", "likely_causes", "next_steps", "known_issue"] {
            record[field] = entry[field].clone();
        }
    }
}
struct State {
    run_id: String,
    command: String,
    stdout: Sha256,
    stdout_bytes: u64,
    stdout_complete: bool,
    failures: Vec<Value>,
    reports: Vec<Value>,
    report_failures: std::collections::BTreeMap<String, String>,
    terminal_relation: Option<(&'static str, String, Vec<String>)>,
    bounded_pack: bool,
}
static STATE: OnceLock<Mutex<Option<State>>> = OnceLock::new();
/// Called only for an explicitly bounded CLI pack; fast receipts stay unchanged.
pub(crate) fn bounded_pack() {
    state(|s| s.bounded_pack = true);
}
fn state<T>(f: impl FnOnce(&mut State) -> T) -> Option<T> {
    let mut guard = STATE.get()?.lock().ok()?;
    Some(f(guard.as_mut()?))
}
/// CLI initialization only. Entropy failure leaves evidence unavailable and does
/// not change the product result; no global last-failure database is created.
pub fn begin(command: &str) {
    let id = (|| {
        let mut bytes = [0u8; 16];
        std::fs::File::open("/dev/urandom")
            .ok()?
            .read_exact(&mut bytes)
            .ok()?;
        Some(crate::value::hex(&bytes))
    })();
    let Some(run_id) = id else {
        return;
    };
    crate::tracing::point(
        "diagnostic.invocation",
        json!({"run_id":run_id,"command":command,"trace_identity":crate::tracing::run_identity()}),
    );
    if let Ok(mut slot) = STATE.get_or_init(|| Mutex::new(None)).lock() {
        *slot = Some(State {
            run_id,
            command: command.into(),
            stdout: Sha256::new(),
            stdout_bytes: 0,
            stdout_complete: true,
            failures: Vec::new(),
            reports: Vec::new(),
            report_failures: std::collections::BTreeMap::new(),
            terminal_relation: None,
            bounded_pack: false,
        });
    }
}
/// Exact-write adapter: hash only bytes accepted by stdout, not a proposed full
/// write. A failed flush/write marks delivery incomplete even after publication.
struct KernelStdout<'a> {
    _guard: std::io::StdoutLock<'static>,
    accepted: Option<&'a mut u64>,
}
pub fn stdout() -> impl Write {
    // Stdlib buffering is a transport buffer, not a workload limit. Hash at
    // the raw syscall boundary, never bytes merely accepted by a line buffer.
    std::io::BufWriter::new(KernelStdout {
        _guard: std::io::stdout().lock(),
        accepted: None,
    })
}
/// L5's stdout measurements share this same actual OS write owner.
pub fn stdout_counted(accepted: &mut u64) -> impl Write + '_ {
    std::io::BufWriter::new(KernelStdout {
        _guard: std::io::stdout().lock(),
        accepted: Some(accepted),
    })
}
impl Write for KernelStdout<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let written =
            unsafe { libc::write(libc::STDOUT_FILENO, bytes.as_ptr().cast(), bytes.len()) };
        if written < 0 {
            state(|s| s.stdout_complete = false);
            return Err(std::io::Error::last_os_error());
        }
        let n = written as usize;
        if let Some(accepted) = self.accepted.as_deref_mut() {
            *accepted = accepted.saturating_add(n as u64);
        }
        state(|s| {
            if s.stdout.update(&bytes[..n]).is_err() {
                s.stdout_complete = false;
            }
            s.stdout_bytes = s.stdout_bytes.saturating_add(n as u64);
        });
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Small additive owner hook. A local record can contain paths; export must use
/// its closed projector. Invalid causal references are not silently repaired.
pub fn record_source_failure(
    error: &Error,
    check_id: &'static str,
    source: SourceContext,
    caused_by: &[String],
) -> String {
    // Owned engine evidence first: independent of any CLI invocation ledger,
    // so direct library callers retain the same source data the CLI records.
    capture_snapshot(error, check_id, &source, caused_by);
    let record = state(|s| {
        let context = source.json();
        if let Some(reference) = error.diagnostic.get()
            && reference.run_id == s.run_id
            && let Some(record) = s
                .failures
                .iter_mut()
                .find(|v| v["id"] == reference.failure_id)
        {
            // Exact carried identity, never nearest/last/message matching.
            if record["context"]["file_hex"].is_string()
                && record["context"]["file_hex"] == context["file_hex"]
                && record["context"]["block_index"] == context["block_index"]
                && record["context"]["pointer_decimal"] == context["pointer_decimal"]
                && let Some(fields) = context.as_object()
            {
                for (key, value) in fields {
                    let missing = record["context"][key].is_null()
                        || record["context"][key].as_array().is_some_and(Vec::is_empty)
                        || record["context"][key] == "unknown";
                    if missing && !value.is_null() && value != "unknown" {
                        record["context"][key] = value.clone();
                    }
                }
            }
            if record["kind"] != error.kind || record["check_id"] != check_id {
                if !record["propagations"].is_array() {
                    record["propagations"] = json!([]);
                }
                record["propagations"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"kind":error.kind,"check_id":check_id,"context":context}));
            }
            return (record.clone(), true);
        }
        let id = format!("f{}", s.failures.len());
        let mut v = json!({"id":id,"kind":error.kind,"message":error.message,"check_id":check_id,"context_origin":"owner_hook","caused_by":caused_by,"context":context,"propagations":[]});
        decorate(&mut v);
        let _ = error.diagnostic.set(std::sync::Arc::new(DiagnosticRef {
            run_id: s.run_id.clone(),
            failure_id: id,
        }));
        s.failures.push(v.clone());
        (v, false)
    });
    if let Some((record, propagation)) = record {
        crate::tracing::point(
            if propagation {
                "diagnostic.propagation"
            } else {
                "diagnostic.source"
            },
            record.clone(),
        );
        record["id"].as_str().unwrap().to_owned()
    } else {
        String::new()
    }
}
fn known_reference(s: &State, reference: &Value, kind: &str) -> Option<String> {
    if reference["run_id"] != s.run_id {
        return None;
    }
    let id = reference["failure_id"].as_str()?;
    let record = s.failures.iter().find(|v| v["id"] == id)?;
    (record["kind"] == kind
        || record["propagations"]
            .as_array()
            .is_some_and(|a| a.iter().any(|p| p["kind"] == kind)))
    .then(|| id.to_owned())
}
/// The command owner explicitly associates its aggregate with the exact report
/// it emitted. Never infer edges from the order of unrelated failures.
pub fn record_terminal(error: &Error, causes: &[String]) {
    state(|s| s.terminal_relation = Some((error.kind, error.message.clone(), causes.to_vec())));
}
pub(crate) fn record_publication(
    path: &std::path::Path,
    output_written: bool,
    durability_completed: bool,
) {
    state(|s| {
        s.reports.push(json!({"origin":"native_atomic_publish","output_path_hex":crate::value::hex(path.as_os_str().as_encoded_bytes()),"output_written":output_written,"durability_completed":durability_completed,"scope":"observed atomic file step; may be staging, not final package certification"}))
    });
}
pub fn record_report(report: &Value) -> Vec<String> {
    // Native pack owns its one L5 report observation; other callers delegate here.
    if report["operation"] != "pack" {
        crate::diagnostics::report(report);
    }
    // Retain publication/completeness and all explicit causal errors, not a
    // full report duplicate. Source hooks add exact checked contexts; report
    // fields retain only context already present, never reopen a file.
    fn collect(v: &Value, at: &str, out: &mut Vec<Value>) {
        if let Some(object) = v.as_object() {
            let mut facts = serde_json::Map::new();
            for k in [
                "complete",
                "planning_complete",
                "output_written",
                "archive_written",
                "directory_created",
                "durability_completed",
                "durability_requested",
                "output_decoder_verified",
            ] {
                if let Some(b) = object.get(k).and_then(Value::as_bool) {
                    facts.insert(k.into(), json!(b));
                }
            }
            if !facts.is_empty() {
                if facts.contains_key("output_written") || facts.contains_key("file_written") {
                    for name in ["output_path_hex", "path_hex"] {
                        if v[name].is_string() {
                            facts.insert(name.into(), v[name].clone());
                        }
                    }
                    for name in ["output", "path", "destination"] {
                        if let Some(path) = v[name].as_str() {
                            let key = format!("{name}_path_hex");
                            if !facts.contains_key(&key) {
                                facts.insert(key, json!(crate::value::hex(path.as_bytes())));
                            }
                        }
                    }
                }
                facts.insert("site".into(), json!(at));
                out.push(Value::Object(facts));
            }
            for (key, child) in object {
                if matches!(
                    key.as_str(),
                    "files" | "roots" | "root_traces" | "trace" | "rewrites" | "rewrite"
                ) {
                    collect(child, &format!("{at}/{key}"), out);
                }
            }
        } else if let Some(array) = v.as_array() {
            for (i, child) in array.iter().enumerate() {
                collect(child, &format!("{at}/{i}"), out);
            }
        }
    }
    fn failure(v: &Value, s: &mut State) -> Option<String> {
        let kind = v["kind"].as_str()?;
        let digest = hash_bytes(&serde_json::to_vec(v).ok()?).ok()?;
        if let Some(id) = s.report_failures.get(&digest) {
            return Some(id.clone());
        }
        if let Some(id) = known_reference(s, &v["diagnostic_ref"], kind) {
            s.report_failures.insert(digest, id.clone());
            return Some(id);
        }
        let causes = v["causes"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| failure(v, s)).collect::<Vec<_>>())
            .unwrap_or_default();
        let id = format!("f{}", s.failures.len());
        let context = if v["owner_file_hex"].is_string() || v["block_index"].is_u64() {
            json!({"file_hex":v["owner_file_hex"],"block_index":v["block_index"],"old_address":v["block_address"],"field_path_hex":v["field_path_hex"],"asset_path_hex":v["asset_path_hex"],"resolved_path_hex":v["resolved_path_hex"]})
        } else {
            Value::Null
        };
        let mut record = json!({"id":id,"kind":kind,"message":v["message"].as_str().unwrap_or("report-carried failure"),"check_id":null,"context_origin":"producer_report","caused_by":causes,"context":context});
        decorate(&mut record);
        s.failures.push(record);
        s.report_failures.insert(digest, id.clone());
        Some(id)
    }
    fn problem(usage: &Value) -> Value {
        let mut value = usage["problem"].clone();
        if let Some(fields) = value.as_object_mut() {
            for key in [
                "owner_file_hex",
                "block_address",
                "asset_path_hex",
                "resolved_path_hex",
            ] {
                fields.insert(key.to_owned(), usage[key].clone());
            }
            let provenance = &usage["field_provenance"];
            let field = if !provenance["full"].is_null() {
                &provenance["full"]
            } else {
                &provenance["dir"]
            };
            fields.insert("field_path_hex".to_owned(), field["field_path_hex"].clone());
        }
        value
    }
    fn contained_error(parent: &Value) -> Value {
        let mut error = parent["error"].clone();
        if error.is_object() && parent["absolute_path_hex"].is_string() {
            error["owner_file_hex"] = parent["absolute_path_hex"].clone();
        }
        error
    }
    fn errors(v: &Value, s: &mut State) {
        if let Some(object) = v.as_object() {
            for (key, child) in object {
                if matches!(key.as_str(), "errors" | "causes")
                    && let Some(a) = child.as_array()
                {
                    for error in a {
                        failure(error, s);
                    }
                }
                if key == "problem" {
                    failure(&problem(v), s);
                }
                if key == "error" {
                    failure(&contained_error(v), s);
                }
                errors(child, s);
            }
        } else if let Some(a) = v.as_array() {
            for child in a {
                errors(child, s);
            }
        }
    }
    state(|s| {
        collect(report, "", &mut s.reports);
        errors(report, s);
        let mut causes = report["errors"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| failure(v, s)).collect::<Vec<_>>())
            .unwrap_or_default();
        if let Some(id) = failure(&contained_error(report), s) {
            causes.push(id);
        }
        if report["operation"] != "pack"
            && let Some(usages) = report["usages"].as_array()
        {
            for usage in usages {
                if let Some(id) = failure(&problem(usage), s) {
                    causes.push(id);
                }
            }
        }
        causes
    })
    .unwrap_or_default()
}
/// Report actual stdout totals once on the root, after its delivery attempts.
pub fn summarize_stdout() {
    if let Some((bytes, complete)) = state(|s| (s.stdout_bytes, s.stdout_complete)) {
        crate::diagnostics::stdout_delivery(bytes, complete);
    }
}
/// Canonical error-envelope digest excludes `diagnostic` to avoid self-reference.
/// It is the original envelope's compact JSON plus newline, not mixed stderr.
pub fn finish(error: &Error, base_error: &Value, exit_code: i32) -> Option<Value> {
    summarize_stdout();
    let stderr_bytes = format!("{base_error}\n");
    let receipt = state(|s| {
        let causes = s
            .terminal_relation
            .as_ref()
            .filter(|(kind, message, _)| *kind == error.kind && *message == error.message)
            .map(|(_, _, ids)| ids.clone())
            .unwrap_or_else(|| {
                known_reference(s, &reference_json(error.diagnostic.get()), error.kind)
                    .into_iter()
                    .collect()
            });
        let id = format!("f{}", s.failures.len());
        let mut terminal = json!({"id":id,"kind":error.kind,"message":error.message,"check_id":null,"context_origin":"terminal","caused_by":causes,"context":null});
        decorate(&mut terminal);
        s.failures.push(terminal);
        let stdout_sha = s.stdout.clone().finish().ok();
        let binary_sha256 = crate::bat_cli::hash_file(std::path::Path::new("/proc/self/exe"))
            .ok()
            .map(|v| v.0);
        let mut receipt = json!({"schema":"oxblend.failure-receipt/v1","producer":{"name":"oxblend","version":env!("CARGO_PKG_VERSION"),"build_revision":env!("OXBLEND_BUILD_COMMIT"),"binary_sha256":binary_sha256},"run_id":s.run_id,"trace_identity":crate::tracing::run_identity(),"command":s.command,"outcome":crate::diagnostics::outcome(error),"exit_code":exit_code,"signal":null,"binding":{"status":"invocation_bound","stdout_sha256":stdout_sha,"stdout_bytes":s.stdout_bytes,"stdout_delivery_complete":s.stdout_complete,"stderr_error_sha256":hash_bytes(stderr_bytes.as_bytes()).ok(),"stderr_scope":"canonical original schema-1 envelope without diagnostic plus newline; excludes ancillary diagnostics"},"operation_state":{"reports":s.reports,"files":s.reports.iter().filter(|r|r["origin"]=="native_atomic_publish").collect::<Vec<_>>()},"failures":s.failures,"source":{"schema":"oxblend.error-source/v1","status":if s.failures.iter().any(|f|f["context"].is_object()){"partial"}else{"unavailable"},"coverage":"instrumented owner checks and retained typed producer-report fields; no recapture"}});
        if s.bounded_pack {
            receipt["memory"] = json!("bounded");
        }
        receipt
    })?;
    let sha = hash_bytes(&serde_json::to_vec(&receipt).ok()?).ok()?;
    crate::tracing::point(
        "diagnostic.receipt",
        json!({"receipt":receipt,"sha256":sha}),
    );
    crate::tracing::root_summary(
        json!({"diagnostic":{"receipt_sha256":sha,"run_id":receipt["run_id"],"source_status":receipt["source"]["status"]}}),
    );
    Some(receipt)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn carried_reference_rejects_seeded_foreign_run_id_and_kind() {
        let run_id = "11111111111111111111111111111111".to_owned();
        let s = State {
            run_id: run_id.clone(),
            command: "trace".into(),
            stdout: Sha256::new(),
            stdout_bytes: 0,
            stdout_complete: true,
            failures: vec![
                json!({"id":"f0","kind":"unresolved_persistent","propagations":[{"kind":"dangling_pointer"}]}),
            ],
            reports: vec![],
            report_failures: Default::default(),
            terminal_relation: None,
            bounded_pack: false,
        };
        let mut reference = json!({"run_id":run_id,"failure_id":"f0"});
        assert_eq!(
            known_reference(&s, &reference, "dangling_pointer"),
            Some("f0".into())
        );
        reference["run_id"] = json!("22222222222222222222222222222222");
        assert_eq!(known_reference(&s, &reference, "dangling_pointer"), None);
        reference["run_id"] = json!(s.run_id);
        reference["failure_id"] = json!("not-retained");
        assert_eq!(known_reference(&s, &reference, "dangling_pointer"), None);
        reference["failure_id"] = json!("f0");
        assert_eq!(known_reference(&s, &reference, "broken_pipe"), None);
        let error = Error::new("unresolved_persistent", "same message");
        error
            .diagnostic
            .set(std::sync::Arc::new(DiagnosticRef {
                run_id: s.run_id.clone(),
                failure_id: "f0".into(),
            }))
            .unwrap();
        let cloned = error.clone().reclassify("dangling_pointer");
        assert!(std::sync::Arc::ptr_eq(
            error.diagnostic.get().unwrap(),
            cloned.diagnostic.get().unwrap()
        ));
        assert_eq!(cloned.to_string(), "dangling_pointer: same message");
    }
    #[test]
    fn source_snapshot_is_owned_without_any_ledger_and_mirrors_ledger_updates() {
        let error = Error::new("unresolved_persistent", "site message");
        let mut source = SourceContext {
            file: Some(PathBuf::from("/local/private/source.blend")),
            block_index: Some(3),
            ..Default::default()
        };
        source.pointer = Some(9999);
        record_source_failure(
            &error,
            "pointer.namespace.lookup.missing",
            source.clone(),
            &[],
        );
        assert!(error.diagnostic_ref().is_null());
        let snapshot = error
            .source_snapshot()
            .expect("snapshot without any ledger");
        assert_eq!(snapshot.kind, "unresolved_persistent");
        assert_eq!(snapshot.check_id, "pointer.namespace.lookup.missing");
        assert_eq!(snapshot.message, "site message");
        assert_eq!(snapshot.context.block_index, Some(3));
        assert_eq!(snapshot.context.pointer, Some(9999));
        // Same site and check: typed context enrichment, no propagation.
        let mut second = source.clone();
        second.element = Some(1);
        second.namespace = Namespace::Id;
        record_source_failure(&error, "pointer.namespace.lookup.missing", second, &[]);
        let snapshot = error.source_snapshot().unwrap();
        assert!(snapshot.propagations.is_empty());
        assert_eq!(snapshot.context.element, Some(1));
        assert_eq!(snapshot.context.namespace, Namespace::Id);
        // A differing check propagates, and clones share the same snapshot.
        let cloned = error.clone();
        let third = SourceContext {
            block_index: Some(3),
            ..Default::default()
        };
        record_source_failure(&cloned, "pointer.resolve.forwarded", third, &[]);
        let snapshot = error.source_snapshot().unwrap();
        assert_eq!(snapshot.propagations.len(), 1);
        assert_eq!(
            snapshot.propagations[0].check_id,
            "pointer.resolve.forwarded"
        );
        assert_eq!(snapshot.propagations[0].kind, "unresolved_persistent");
        // Ledger-shaped record without any invented id.
        let record = snapshot.record_json();
        assert!(record.get("id").is_none());
        assert_eq!(record["context_origin"], "owner_hook");
        assert_eq!(record["kind"], "unresolved_persistent");
        assert_eq!(
            record["propagations"][0]["check_id"],
            "pointer.resolve.forwarded"
        );
    }
    // CLI ledger co-population is exercised through the real CLI/report
    // entry path. Mutating its global owner inside parallel library unit
    // tests would cross-contaminate unrelated source checks.
}
