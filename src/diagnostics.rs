//! Oxblend's best-effort flattrace adapter. Use `tracing` for the stable library API.
use crate::{Error, Result};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[path = "flattrace.rs"]
mod flattrace;
static WRITER: OnceLock<Mutex<Option<flattrace::Writer>>> = OnceLock::new();
static NEXT_SCOPE: AtomicU64 = AtomicU64::new(0);
static LEVEL: AtomicU64 = AtomicU64::new(0);
#[derive(Default)]
struct HotStats {
    count: u64,
    total_ns: u64,
    max_ns: u64,
    bytes: u64,
    input_bytes: u64,
    error_records: u64,
    outcomes: std::collections::BTreeMap<&'static str, u64>,
}
type HotMap = std::collections::BTreeMap<&'static str, HotStats>;
fn hot_event(event: &str) -> Option<&'static str> {
    match event {
        "query.resolve" => Some("query.resolve"),
        "query.project" => Some("query.project"),
        "read.byte_array" => Some("read.byte_array"),
        "read.pointer_table" => Some("read.pointer_table"),
        "decode.frame" => Some("decode.frame"),
        "decode.scan" => Some("decode.scan"),
        _ => None,
    }
}
fn emit_hot(hot: HotMap, owner: Option<&str>) {
    for (event, stats) in hot {
        with_writer(|w| {
            w.point("hot.summary", json!({"event":event,"count":stats.count,"total_ns":stats.total_ns,"max_ns":stats.max_ns,"bytes":stats.bytes,"input_bytes":stats.input_bytes,"error_records":stats.error_records,"outcomes":stats.outcomes}), owner, None)
        });
    }
}
static NEXT_CAUSE: AtomicU64 = AtomicU64::new(0);
static FACTS: OnceLock<Mutex<Facts>> = OnceLock::new();
#[derive(Default)]
struct Facts {
    inputs: Vec<Value>,
    outputs: Vec<Value>,
    errors: Vec<Value>,
    propagations: Vec<Value>,
    observations: Vec<Value>,
    degraded: bool,
    summary: Value,
    hot: HotMap,
}
/// True only after successful explicit opt-in initialization. Diagnostic-only
/// metadata/formatting must be behind this check; never initializes implicitly.
pub fn enabled() -> bool {
    LEVEL.load(Ordering::Relaxed) != 0
}
fn facts(f: impl FnOnce(&mut Facts)) {
    if let Some(slot) = FACTS.get()
        && let Ok(mut value) = slot.lock()
    {
        f(&mut value);
    }
}
/// Physical identity, not a content hash. Native path bytes survive non-UTF-8.
pub fn input_identity(path: &std::path::Path) -> Value {
    use std::os::unix::fs::MetadataExt;
    let signature = path.metadata().ok().map(|m| json!({"dev":m.dev(),"ino":m.ino(),"size_bytes":m.len(),"mtime_sec":m.mtime(),"mtime_nsec":m.mtime_nsec(),"ctime_sec":m.ctime(),"ctime_nsec":m.ctime_nsec()}));
    json!({"path":path.to_str(),"path_hex":crate::value::hex(path.as_os_str().as_encoded_bytes()),"identity_kind":"stat_signature_not_content_hash","signature":signature})
}
/// Observe an actual input-open attempt at its owner and return its identity
/// for the file.open payload. This is the sole capture-ledger recording path;
/// span attributes alone never imply that an input was observed. Disabled
/// tracing does no metadata read, path formatting or ledger mutation.
pub(crate) fn observe_input(path: &std::path::Path) -> Value {
    if !enabled() {
        return Value::Null;
    }
    let identity = input_identity(path);
    facts(|f| f.inputs.push(identity.clone()));
    identity
}
/// Record actual stdout delivery separately from product completeness.
pub fn stdout_delivery(bytes: u64, complete: bool) {
    if enabled() {
        facts(|f| {
            if let Some(output) = f.outputs.iter_mut().find(|v| v["role"] == "stdout") {
                output["bytes"] = json!(output["bytes"].as_u64().unwrap_or(0) + bytes);
                output["output_written"] = json!(output["output_written"] == true && complete);
            } else {
                f.outputs.push(json!({"role":"stdout","bytes":bytes,"output_written":complete,"durability_requested":"not_applicable","durability_completed":null}));
            }
        });
    }
}
fn resources() -> Value {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return json!({"scope":"process","source":"getrusage(RUSAGE_SELF)","value":null,"reason":std::io::Error::last_os_error().to_string()});
    }
    json!({"scope":"process_lifetime_including_trace_init","source":"getrusage(RUSAGE_SELF)","cpu_user_seconds":usage.ru_utime.tv_sec as f64 + usage.ru_utime.tv_usec as f64/1e6,"cpu_system_seconds":usage.ru_stime.tv_sec as f64 + usage.ru_stime.tv_usec as f64/1e6,"peak_rss_kib":usage.ru_maxrss,"input_blocks":usage.ru_inblock,"output_blocks":usage.ru_oublock,"block_unit":"OS block operations, not bytes","trace_bytes":null,"trace_bytes_unavailable_reason":"0.1 Writer does not expose sink path or byte count; measure retained file externally"})
}
struct Frame {
    token: u64,
    id: Option<String>,
    level: u64,
    hot: HotMap,
}
thread_local! { static STACK: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) }; }
fn with_writer<T>(f: impl FnOnce(&mut flattrace::Writer) -> std::io::Result<T>) -> Option<T> {
    let mut slot = WRITER.get()?.lock().ok()?;
    match f(slot.as_mut()?) {
        Ok(value) => Some(value),
        Err(error) => {
            // A bad instrumentation call is not loss of the process writer.
            // The writer itself owns unrecoverable sink degradation.
            facts(|f| f.degraded = true);
            flattrace::warn_text(&format!("instrumentation skipped: {error}"));
            None
        }
    }
}
extern "C" fn finish_process() {
    // atexit also runs for a nonzero process::exit; the library cannot infer
    // application success. CLI entry explicitly finishes its known outcome.
    finish("unknown");
}
/// Initialize once at the real entry; allowlisted facts only, no environment dump.
/// No environment mutation. Disabled initialization is also remembered.
pub fn init(args: &[std::ffi::OsString], command: &str) {
    WRITER.get_or_init(|| {
        if std::env::var_os("FLATTRACE_DIR").is_none_or(|v| v.is_empty())
            || flattrace::level_from_env().0 == 0
        {
            return Mutex::new(None);
        }
        let host = std::fs::read_to_string("/etc/hostname")
            .ok()
            .map(|s| s.trim().to_owned());
        let binary_hash = std::env::current_exe()
            .ok()
            .and_then(|p| crate::bat_cli::hash_file(&p).ok())
            .map(|(h, _)| format!("sha256:{h}"));
        // Package strictness is independent of read-only report collection.
        // Preserve intent too: compatibility mode flags on reports are no-ops.
        let requested = if matches!(command,"trace"|"list"|"overrides"|"pack"|"pack-sequence") {
            args.iter().skip(2).take_while(|s|*s != "--")
                .find_map(|s|match s.to_str() {
                    Some("--strict")=>Some("strict"),
                    Some("--permissive"|"--non-strict")=>Some("permissive"),
                    _=>None,
                })
        } else {None};
        let requested_mode=requested.unwrap_or("strict");
        let mode=if matches!(command,"pack"|"pack-sequence") {requested_mode} else {"report"};
        let paths: Vec<std::path::PathBuf> = match command {
            "pack"|"pack-sequence" => crate::bat_cli::pack_arguments(command, args.get(2..).unwrap_or(&[])).ok().map(|v|v.0).unwrap_or_default(),
            "diff" => args.iter().skip(2).take(2).map(std::path::PathBuf::from).collect(),
            "help"|"version"|"skill"|"catalog"|"invalid"|"library" => Vec::new(),
            "explain" => args.windows(2).find(|pair| pair[0] == "--file").map(|pair|std::path::PathBuf::from(&pair[1])).into_iter().collect(),
            _ => args.get(2).filter(|p| !p.as_encoded_bytes().starts_with(b"-")).map(std::path::PathBuf::from).into_iter().collect(),
        };
        let inputs: Vec<Value> = paths.iter().map(|p|input_identity(p)).collect();
        let operation_mode = if command == "edit" { args.windows(2).find(|v| v[0] == "--mode").and_then(|v| v[1].to_str()).filter(|v| matches!(*v,"format-only"|"comp")) } else {None};
        let data = json!({"command":command,"mode":mode,"requested_mode":requested_mode,"mode_source":if requested.is_some(){"explicit"}else{"default"},"operation_mode":operation_mode,"inputs":inputs,"inputs_coverage":"requested_at_entry; actual opens/captures in root.summary","privacy":{"activation":"explicit_FLATTRACE_DIR","paths":"full_local_opt_in","export_requires_review":true},"env":{
            "service.name":"oxblend", "service.version":env!("CARGO_PKG_VERSION"),
            "host.name":host, "host.arch":std::env::consts::ARCH,
            "os.type":std::env::consts::OS, "tool.binary.sha256":if command == "library" { None } else { binary_hash.clone() },
            "runtime.executable.sha256":if command == "library" { binary_hash } else { None },
            "tool.build.commit":env!("OXBLEND_BUILD_COMMIT")}});
        let writer = match flattrace::Writer::from_env(
            "oxblend",
            env!("CARGO_PKG_VERSION"),
            args.iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect(),
            host.as_deref().unwrap_or(""),
            data,
        ) {
            Ok(writer) => writer,
            Err(error) => {
                flattrace::warn(&error);
                None
            }
        };
        if let Some(w) = &writer {
            FACTS.get_or_init(|| Mutex::new(Facts::default()));
            LEVEL.store(w.level(), Ordering::Relaxed);
        }
        if writer.is_some() && unsafe { libc::atexit(finish_process) } != 0 {
            flattrace::warn(&std::io::Error::other(
                "cannot register process trace completion",
            ));
        }
        Mutex::new(writer)
    });
}
/// Emit a root-owned summary point using the existing 0.1 point API. Additive
/// caller facts are retained for process completion; arrays are caller-owned
/// snapshots, not inferred product effects. Nonobject attributes are wrapped.
pub fn root_summary(attributes: Value) {
    emit_root_summary(attributes, false);
}
fn emit_root_summary(attributes: Value, final_summary: bool) {
    if !enabled() {
        return;
    }
    facts(|f| {
        if !f.summary.is_object() {
            f.summary = json!({});
        }
        if let Some(object) = attributes.as_object() {
            for (key, value) in object {
                f.summary[key] = value.clone();
            }
        } else {
            f.summary["attributes"] = attributes;
        }
    });
    let mut data = json!({"outputs":[],"inputs":[],"errors":[],"resource":resources(),"diagnostics_complete":true});
    facts(|f| {
        data["outputs"] = json!(f.outputs);
        data["inputs"] = json!(f.inputs);
        data["errors"] = json!(f.errors);
        data["error_propagations"] = json!(f.propagations);
        data["report_observations"] = json!(f.observations);
        data["error_counting"] = json!(
            "primary records only; propagation/report observations are not additional incidents; causal links unavailable unless supplied"
        );
        data["diagnostics_complete"] = json!(!f.degraded);
        if let Some(object) = f.summary.as_object() {
            for (key, value) in object {
                if final_summary
                    && matches!(
                        key.as_str(),
                        "outputs"
                            | "inputs"
                            | "resource"
                            | "diagnostics_complete"
                            | "errors"
                            | "error_propagations"
                            | "report_observations"
                    )
                {
                    data["caller_summary"][key] = value.clone();
                    if matches!(key.as_str(), "inputs" | "outputs") && value.is_array() {
                        data[key]
                            .as_array_mut()
                            .unwrap()
                            .extend(value.as_array().unwrap().iter().cloned());
                    }
                    if key == "diagnostics_complete" && value == false {
                        data[key] = json!(false);
                    }
                } else {
                    data[key] = value.clone();
                }
            }
        }
    });
    data["summary_phase"] = json!(if final_summary { "final" } else { "checkpoint" });
    with_writer(|w| w.point("root.summary", data, None, None));
}
pub fn finish(outcome: &str) {
    // 0.1 finish cannot carry attributes; root-owned point precedes it.
    let mut hot = HotMap::new();
    facts(|f| hot = std::mem::take(&mut f.hot));
    emit_hot(hot, None);
    emit_root_summary(json!({}), true);
    LEVEL.store(0, Ordering::Relaxed);
    if let Some(slot) = WRITER.get()
        && let Some(writer) = slot.lock().ok().and_then(|mut w| w.take())
        && let Err(error) = writer.finish(outcome)
    {
        flattrace::warn(&error);
    }
}
/// Invocation identity available through the unchanged 0.1 API. The pair is
/// unique; it is not the inaccessible private envelope run string.
pub fn run_identity() -> Option<Value> {
    with_writer(|w| {
        Ok(
            json!({"trace":w.trace_id(),"root_span":w.root_span(),"identity_kind":"trace_root_span"}),
        )
    })
}
pub(crate) fn parent() -> Option<String> {
    STACK.with(|s| s.borrow().iter().rev().find_map(|f| f.id.clone()))
}
/// Links retain submission causality even after a submitting scope ends.
pub(crate) fn worker_links() -> Value {
    with_writer(|w| Ok(json!([{"trace":w.trace_id(),"span":parent().unwrap_or_else(|| w.root_span().to_owned()),"relationship":"submitted_by"}]))).unwrap_or_else(|| json!([]))
}
/// Non-Send: this guard belongs to its creating thread, not an async task.
pub struct Span {
    token: u64,
    id: Option<String>,
    // Like emitted spans, hot guards must end within their structural owner's
    // dynamic extent. They are synchronous/non-Send; never retain one past it.
    aggregate: Option<(Option<u64>, &'static str, std::time::Instant)>,
    outcome: &'static str,
    data: Value,
    error: Option<Value>,
    error_records: u64,
    event: Option<String>,
    start_data: Value,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl Span {
    pub fn new(event: &str, data: Value) -> Self {
        Self::new_lazy(event, || data)
    }
    /// Avoid constructing per-call identity payloads when only aggregate timing
    /// is requested. The structural owner supplies identity at level one.
    pub fn new_lazy(event: &str, data: impl FnOnce() -> Value) -> Self {
        let level = LEVEL.load(Ordering::Relaxed);
        if level == 1
            && let Some(event) = hot_event(event)
        {
            let owner = STACK.with(|s| {
                s.borrow()
                    .iter()
                    .rev()
                    .find(|f| f.id.is_some())
                    .map(|f| f.token)
            });
            return Self {
                token: u64::MAX,
                id: None,
                aggregate: Some((owner, event, std::time::Instant::now())),
                outcome: "unknown",
                data: Value::Null,
                error: None,
                error_records: 0,
                event: None,
                start_data: Value::Null,
                _thread: std::marker::PhantomData,
            };
        }
        Self::at_level(1, event, if level == 0 { Value::Null } else { data() })
    }
    pub(crate) fn at_level(level: u64, event: &str, data: Value) -> Self {
        if !enabled() {
            return Self {
                token: u64::MAX,
                id: None,
                aggregate: None,
                outcome: "unknown",
                data: Value::Null,
                error: None,
                error_records: 0,
                event: None,
                start_data: Value::Null,
                _thread: std::marker::PhantomData,
            };
        }
        let level = STACK.with(|s| s.borrow().last().map_or(level, |f| level.max(f.level)));
        let parent = parent();
        Self::under_level(level, event, data, parent.as_deref())
    }
    // Level-2 worker detail only: callers must check detail_enabled first,
    // so an inactive frame cannot suppress nested level-1 scopes.
    pub(crate) fn under(event: &str, data: Value, parent: Option<&str>) -> Self {
        Self::under_level(2, event, data, parent)
    }
    /// L19 worker-contract spans for inline selective capture. Worker-thread
    /// spans are structurally rooted because their STACK is empty; this
    /// inline path runs inside planning scopes, so it roots explicitly to
    /// honor the same fetch/file.open contract (structural parent == root;
    /// data links still carry the submitting scope). Same L1 visibility as
    /// `new`; not a hot path (once per capture), and fetch/file.open are
    /// not hot-aggregated events, so no aggregate branch is needed.
    pub(crate) fn rooted(event: &str, data: Value) -> Self {
        if !enabled() {
            return Self {
                token: u64::MAX,
                id: None,
                aggregate: None,
                outcome: "unknown",
                data: Value::Null,
                error: None,
                error_records: 0,
                event: None,
                start_data: Value::Null,
                _thread: std::marker::PhantomData,
            };
        }
        Self::under_level(1, event, data, None)
    }
    fn under_level(level: u64, event: &str, data: Value, parent: Option<&str>) -> Self {
        // Both ordinary and explicitly parented worker spans construct here.
        // Preserve attributes before Writer::start consumes the payload.
        let start_data = data.clone();
        let id = with_writer(|w| {
            if w.level() < level {
                Ok(None)
            } else {
                w.start(event, data, parent).map(Some)
            }
        })
        .flatten();
        let token = NEXT_SCOPE.fetch_add(1, Ordering::Relaxed);
        STACK.with(|s| {
            s.borrow_mut().push(Frame {
                token,
                id: id.clone(),
                level,
                hot: HotMap::new(),
            })
        });
        Self {
            token,
            id,
            aggregate: None,
            outcome: "unknown",
            data: json!({}),
            error: None,
            error_records: 0,
            event: Some(event.to_owned()),
            start_data,
            _thread: std::marker::PhantomData,
        }
    }
    fn diagnostic_owner(&self) -> Option<String> {
        self.id.clone().or_else(|| {
            let (owner, _, _) = self.aggregate.as_ref()?;
            STACK
                .with(|s| {
                    s.borrow()
                        .iter()
                        .find(|f| Some(f.token) == *owner)
                        .and_then(|f| f.id.clone())
                })
                .or_else(|| with_writer(|w| Ok(w.root_span().to_owned())))
        })
    }
    /// A cause record on this exact span, or the aggregate's structural owner.
    pub fn record_error(&mut self, kind: &str, message: &str) {
        let owner = self.diagnostic_owner();
        let Some(id) = owner.as_deref() else {
            return;
        };
        self.error_records += 1;
        let data = json!({"kind":kind,"message":message,"record_type":"primary","cause_id":format!("{id}:{}", NEXT_CAUSE.fetch_add(1, Ordering::Relaxed)),"hot_event":self.aggregate.as_ref().map(|a|a.1)});
        facts(|f| f.errors.push(data.clone()));
        with_writer(|w| w.point("error", data, Some(id), None));
        self.outcome = "error";
        self.error = Some(json!({"kind":kind,"message":message}));
    }
    pub fn active(&self) -> bool {
        self.id.is_some()
    }
    pub fn data(&mut self, data: Value) {
        self.data = data;
    }
    pub fn end(mut self, outcome: &'static str) {
        self.outcome = outcome;
    }
    pub fn result<T>(mut self, result: Result<T>) -> Result<T> {
        self.record_result(&result);
        result
    }
    pub(crate) fn record_result<T>(&mut self, result: &Result<T>) {
        if !self.active() && self.aggregate.is_none() {
            return;
        }
        match result {
            Ok(_) => self.outcome = "ok",
            Err(error) => {
                self.outcome = outcome(error);
                self.error_records += 1;
                self.error = Some(json!({"kind":error.kind,"message":error.message}));
                let owner = self.diagnostic_owner();
                if let Some(id) = owner.as_deref() {
                    with_writer(|w| {
                        w.point("error", json!({"kind":error.kind,"message":error.message,"record_type":"propagation","propagation":true,"cause_id":error.diagnostic.get().map(|r|r.failure_id.as_str()),"cause_run_id":error.diagnostic.get().map(|r|r.run_id.as_str()),"cause_coverage":if error.diagnostic.get().is_some(){"carried_exact"}else{"unattributed; no carried diagnostic reference at this propagation"},"source":"span_result","hot_event":self.aggregate.as_ref().map(|a|a.1)}), Some(id), None)
                    });
                }
            }
        }
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        let aggregate_owner = self.aggregate.as_ref().and_then(|_| {
            if self.error.is_some() {
                self.diagnostic_owner()
            } else {
                None
            }
        });
        if let Some((owner, event, started)) = self.aggregate.take() {
            if let Some(error) = &self.error {
                facts(|f| {
                    f.propagations.push(json!({"span":aggregate_owner,"event":event,"kind":error["kind"],"message":error["message"],"record_type":"propagation","propagation":true,"cause_id":null,"cause_coverage":"unattributed; stable Error carries no occurrence ID"}))
                });
            }
            let ns = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
            let update = |hot: &mut HotMap| {
                let stats = hot.entry(event).or_default();
                stats.count += 1;
                stats.total_ns = stats.total_ns.saturating_add(ns);
                stats.max_ns = stats.max_ns.max(ns);
                stats.bytes += self.data["bytes"].as_u64().unwrap_or(0);
                stats.input_bytes += self.data["input_bytes"].as_u64().unwrap_or(0);
                stats.error_records += self.error_records;
                *stats.outcomes.entry(self.outcome).or_default() += 1;
            };
            let owned = STACK.with(|s| {
                let mut stack = s.borrow_mut();
                if let Some(frame) = stack.iter_mut().find(|f| Some(f.token) == owner) {
                    update(&mut frame.hot);
                    true
                } else {
                    false
                }
            });
            if !owned {
                facts(|f| update(&mut f.hot));
            }
            return;
        }
        if self.token == u64::MAX {
            return;
        }
        let hot = STACK.with(|s| {
            let mut stack = s.borrow_mut();
            stack
                .iter()
                .position(|f| f.token == self.token)
                .map(|i| stack.remove(i).hot)
                .unwrap_or_default()
        });
        emit_hot(hot, self.id.as_deref());
        if let Some(id) = self.id.take() {
            if let Some(error) = &self.error {
                facts(|f| {
                    f.propagations.push(json!({"span":id,"kind":error["kind"],"message":error["message"],"record_type":"propagation","propagation":true,"cause_id":null,"cause_coverage":"unattributed; stable Error carries no occurrence ID"}))
                });
            }
            if self.event.as_deref() == Some("publish") {
                let mut effect = self.start_data.clone();
                if let Some(object) = self.data.as_object() {
                    for (k, v) in object {
                        effect[k] = v.clone();
                    }
                }
                effect["outcome"] = json!(self.outcome);
                effect["role"] = json!("atomic_publication");
                effect["scope"] = json!("publication_moment_not_package_completeness");
                facts(|f| f.outputs.push(effect));
            }
            with_writer(|w| w.end(&id, self.outcome, self.data.take(), self.error.take()));
        }
    }
}
pub fn outcome(error: &Error) -> &'static str {
    outcome_kind(error.kind)
}
pub(crate) fn outcome_kind(kind: &str) -> &'static str {
    if kind.starts_with("incomplete_") {
        "partial"
    } else if matches!(
        kind,
        "usage"
            | "unsupported_edit"
            | "unsupported_type"
            | "output_exists"
            | "same_input"
            | "pack_destination"
            | "pack_project"
            | "pack_input"
            | "pack_path"
            | "edit_spec"
            | "pointer_write"
            | "node_version"
            | "node_expected_graph"
            | "node_endpoint"
            | "node_socket_type"
            | "node_occupied"
            | "node_cycle"
            | "node_graph"
            | "node_constructor"
    ) {
        "refused"
    } else {
        "error"
    }
}
pub(crate) fn detail_enabled() -> bool {
    LEVEL.load(Ordering::Relaxed) >= 2
}
/// Emit command summaries on the still-active process root, not a child phase.
pub fn root_point(event: &str, data: Value) {
    with_writer(|w| {
        let root = w.root_span().to_owned();
        w.point(event, data, Some(&root), None)
    });
}
pub fn point(event: &str, mut data: Value) {
    if !enabled() {
        return;
    }
    if event == "error" {
        data["record_type"] = json!("primary");
        if data.get("cause_id").is_none() {
            data["cause_id"] = json!(format!(
                "cause:{}",
                NEXT_CAUSE.fetch_add(1, Ordering::Relaxed)
            ));
        }
        facts(|f| f.errors.push(data.clone()));
    } else if matches!(event, "report.diagnostic" | "decision.observation") {
        data["record_type"] = json!("report_observation");
        facts(|f| f.observations.push(data.clone()));
    }
    let level = STACK.with(|s| s.borrow().last().map_or(1, |f| f.level));
    with_writer(|w| {
        if w.level() < level {
            Ok(())
        } else {
            w.point(event, data, parent().as_deref(), None)
        }
    });
}
/// Observe structured returned report causes/skips independently of terminal
/// command errors. The JSON-pointer-like item is a stable site in that report;
/// the full record preserves owner identity without guessing its schema.
pub fn report(value: &Value) {
    if !enabled() {
        return;
    }
    fn visit(value: &Value, site: &str) {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    let path = format!("{site}/{key}");
                    if let Some(items) = child.as_array()
                        && matches!(
                            key.as_str(),
                            "errors" | "warnings" | "skipped_items" | "missing_links"
                        )
                    {
                        for (i, item) in items.iter().enumerate() {
                            let identity = format!("{path}/{i}");
                            if key == "skipped_items" {
                                point(
                                    "decision.observation",
                                    json!({"item":identity,"kind":item["kind"].as_str().unwrap_or("skipped"),"reason":item["reason"].as_str().or(item["message"].as_str()).unwrap_or("report_declared_skip"),"source":"product_report"}),
                                );
                            } else if let Some(kind) = item["kind"].as_str() {
                                point(
                                    if key == "warnings" {
                                        "warning"
                                    } else {
                                        "report.diagnostic"
                                    },
                                    json!({"kind":kind,"message":item["message"].as_str().unwrap_or("report-carried diagnostic; inspect product report"),"item":identity,"severity":key,"source":"product_report"}),
                                );
                            }
                        }
                    }
                    if child.is_object() || matches!(key.as_str(), "root_traces" | "rewrites") {
                        visit(child, &path);
                    }
                }
            }
            Value::Array(array) => {
                for (i, item) in array.iter().enumerate() {
                    visit(item, &format!("{site}/{i}"));
                }
            }
            _ => (),
        }
    }
    fn effect(value: Value) {
        facts(|f| f.outputs.push(value.clone()));
        point("output.effect", value);
    }
    if value["operation"] == "pack" {
        if let Some(files) = value["files"].as_array() {
            for file in files {
                effect(
                    json!({"role":"pack_member","path_hex":file["output_path_hex"],"archive_member":value["archive"],"output_written":file["output_written"],"durability_requested":value["durability"],"durability_completed":file["durability_completed"],"scope":"final_product_report"}),
                );
            }
        }
        if let Some(info) = value["pack_info"].as_object() {
            effect(
                json!({"role":"pack_info","path_hex":info.get("output_path_hex"),"archive_member":value["archive"],"output_written":info.get("output_written"),"durability_requested":value["durability"],"durability_completed":info.get("durability_completed"),"scope":"final_product_report"}),
            );
        }
        if value["archive"] == true {
            effect(
                json!({"role":"pack_archive","path_hex":value["directory_path_hex"],"output_written":value["archive_written"],"durability_requested":value["durability"],"durability_completed":value["durability_completed"],"scope":"final_product_report"}),
            );
        }
    } else if value.get("output_written").is_some() && value.get("output_path_hex").is_some() {
        effect(
            json!({"role":"edit_output","path_hex":value["output_path_hex"],"output_written":value["output_written"],"durability_requested":value["durability"],"durability_completed":value["durability_completed"],"bytes":value["output_physical_length"],"scope":"final_product_report"}),
        );
    }
    visit(value, "");
}
pub(crate) fn opened<T>(path: &std::path::Path, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let identity = observe_input(path);
    let mut span = Span::new(
        "file.open",
        if enabled() {
            json!({"path":path.to_string_lossy(),"input":identity})
        } else {
            Value::Null
        },
    );
    let result = f();
    if span.active() {
        span.data(json!({"size_bytes":path.metadata().ok().map(|m| m.len())}));
    }
    span.result(result)
}
