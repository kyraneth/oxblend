//! Oxblend payload/lifecycle adapter. flattrace owns the wire format.
//! Explicit CLI initialization only; no trace I/O when disabled. Workers share
//! the process writer through exclusive access, with captured parent IDs.
use crate::{Error, Result};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::sync::{Mutex, OnceLock};

#[path = "flattrace.rs"]
mod flattrace;
static WRITER: OnceLock<Mutex<Option<flattrace::Writer>>> = OnceLock::new();
thread_local! { static STACK: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) }; }
fn with_writer<T>(f: impl FnOnce(&mut flattrace::Writer) -> std::io::Result<T>) -> Option<T> {
    let mut slot = WRITER.get()?.lock().ok()?;
    let result = f(slot.as_mut()?);
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            flattrace::warn(&error);
            *slot = None;
            None
        }
    }
}
/// Initialize once at the real CLI entry; allowlisted facts only, no env dump.
pub fn init(args: &[std::ffi::OsString], command: &str) {
    if std::env::var_os("FLATTRACE_DIR").is_none_or(|v| v.is_empty()) {
        return;
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
        "tool.build.commit":option_env!("OXBLEND_BUILD_COMMIT")}});
    match flattrace::Writer::from_env(
        "oxblend",
        env!("CARGO_PKG_VERSION"),
        args.iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect(),
        host.as_deref().unwrap_or(""),
        data,
    ) {
        Ok(writer) => {
            let _ = WRITER.set(Mutex::new(writer));
        }
        Err(error) => flattrace::warn(&error),
    }
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
    STACK.with(|s| s.borrow().last().cloned())
}
pub(crate) fn worker_parent() -> Option<String> {
    STACK.with(|s| s.borrow().first().cloned())
}
/// Non-Send: the scope belongs to its creating thread, not an async task.
pub struct Span {
    id: Option<String>,
    outcome: &'static str,
    data: Value,
    error: Option<Value>,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl Span {
    pub fn new(event: &str, data: Value) -> Self {
        Self::under(event, data, parent().as_deref())
    }
    pub(crate) fn under(event: &str, data: Value, parent: Option<&str>) -> Self {
        let id = with_writer(|w| w.start(event, data, parent));
        if let Some(id) = &id {
            STACK.with(|s| s.borrow_mut().push(id.clone()));
        }
        Self {
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
        match &result {
            Ok(_) => self.outcome = "ok",
            Err(error) => {
                self.outcome = outcome(error);
                self.error = Some(json!({"kind":error.kind,"message":error.message}));
            }
        }
        result
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            STACK.with(|s| s.borrow_mut().retain(|v| v != &id));
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
    with_writer(|w| w.point(event, data, parent().as_deref(), None));
}
pub(crate) fn opened<T>(path: &std::path::Path, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let mut span = Span::new("file.open", json!({"path":path.to_string_lossy()}));
    let result = f();
    // Metadata is diagnostic only; failure never changes the product result.
    if span.active() {
        span.data(json!({"size_bytes":path.metadata().ok().map(|m| m.len())}));
    }
    span.result(result)
}
