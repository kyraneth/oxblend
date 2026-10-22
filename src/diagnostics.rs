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
struct Frame {
    token: u64,
    id: Option<String>,
    level: u64,
}
thread_local! { static STACK: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) }; }
fn with_writer<T>(f: impl FnOnce(&mut flattrace::Writer) -> std::io::Result<T>) -> Option<T> {
    let mut slot = WRITER.get()?.lock().ok()?;
    match f(slot.as_mut()?) {
        Ok(value) => Some(value),
        Err(error) => {
            // A bad instrumentation call is not loss of the process writer.
            // The writer itself owns unrecoverable sink degradation.
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
        let data = json!({"command":command,"env":{
            "service.name":"oxblend", "service.version":env!("CARGO_PKG_VERSION"),
            "host.name":host, "host.arch":std::env::consts::ARCH,
            "os.type":std::env::consts::OS, "tool.binary.sha256":binary_hash,
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
        if writer.is_some() && unsafe { libc::atexit(finish_process) } != 0 {
            flattrace::warn(&std::io::Error::other(
                "cannot register process trace completion",
            ));
        }
        Mutex::new(writer)
    });
}
pub fn finish(outcome: &str) {
    if let Some(slot) = WRITER.get()
        && let Some(writer) = slot.lock().ok().and_then(|mut w| w.take())
        && let Err(error) = writer.finish(outcome)
    {
        flattrace::warn(&error);
    }
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
    outcome: &'static str,
    data: Value,
    error: Option<Value>,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl Span {
    pub fn new(event: &str, data: Value) -> Self {
        Self::at_level(1, event, data)
    }
    pub(crate) fn at_level(level: u64, event: &str, data: Value) -> Self {
        let level = STACK.with(|s| s.borrow().last().map_or(level, |f| level.max(f.level)));
        let parent = parent();
        let id = with_writer(|w| {
            if w.level() < level {
                Ok(None)
            } else {
                w.start(event, data, parent.as_deref()).map(Some)
            }
        })
        .flatten();
        let token = NEXT_SCOPE.fetch_add(1, Ordering::Relaxed);
        STACK.with(|s| {
            s.borrow_mut().push(Frame {
                token,
                id: id.clone(),
                level,
            })
        });
        Self {
            token,
            id,
            outcome: "unknown",
            data: json!({}),
            error: None,
            _thread: std::marker::PhantomData,
        }
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
        match result {
            Ok(_) => self.outcome = "ok",
            Err(error) => {
                self.outcome = outcome(error);
                self.error = Some(json!({"kind":error.kind,"message":error.message}));
            }
        }
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        STACK.with(|s| s.borrow_mut().retain(|f| f.token != self.token));
        if let Some(id) = self.id.take() {
            with_writer(|w| w.end(&id, self.outcome, self.data.take(), self.error.take()));
        }
    }
}
pub fn outcome(error: &Error) -> &'static str {
    if error.kind.starts_with("incomplete_") {
        "partial"
    } else if matches!(
        error.kind,
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
    ) {
        "refused"
    } else {
        "error"
    }
}
pub fn point(event: &str, data: Value) {
    let level = STACK.with(|s| s.borrow().last().map_or(1, |f| f.level));
    with_writer(|w| {
        if w.level() < level {
            Ok(())
        } else {
            w.point(event, data, parent().as_deref(), None)
        }
    });
}
pub(crate) fn opened<T>(path: &std::path::Path, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let mut span = Span::new("file.open", json!({"path":path.to_string_lossy()}));
    let result = f();
    if span.active() {
        span.data(json!({"size_bytes":path.metadata().ok().map(|m| m.len())}));
    }
    span.result(result)
}
