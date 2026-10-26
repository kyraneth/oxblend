//! Stable, best-effort local tracing for in-process consumers.
use serde_json::Value;
/// Initialize from FLATTRACE_DIR/FLATTRACE_LEVEL/FLATTRACE_TRACEPARENT once.
/// Unset/empty directory or level zero is a remembered no-op. Does not mutate
/// the environment. A normally exiting process closes its single run.
pub fn initialize() {
    crate::diagnostics::init(&[], "library");
}
/// Explicit call outcome. Partial means a report was returned with incomplete
/// coverage; it is not proof of successful publication or usable output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    Ok,
    Error,
    Refused,
    Partial,
    Cancelled,
    Unknown,
}
impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Refused => "refused",
            Self::Partial => "partial",
            Self::Cancelled => "cancelled",
            Self::Unknown => "unknown",
        }
    }
}
/// One synchronous scope, nested under the current thread's scope.
/// Reserved hot phase names (`query.resolve`, `query.project`, `read.byte_array`,
/// `read.pointer_table`, `decode.frame`, `decode.scan`) aggregate on their structural owner at level one;
/// explicit level two retains per-call spans. Other names remain per-call.
/// Not Send/Sync; unknown on drop unless end supplies the actual outcome.
pub struct Span(crate::diagnostics::Span);
/// Emit a point on the current thread's active scope (or process root).
/// Does not initialize tracing and is a no-op while disabled.
pub fn point(event: &str, attributes: Value) {
    crate::diagnostics::point(event, attributes);
}
/// Return the invocation identity as {trace, root_span,
/// identity_kind:"trace_root_span"}, or None when tracing is disabled. The pair
/// binds this invocation; it is not flattrace's private envelope run string.
pub fn run_identity() -> Option<Value> {
    crate::diagnostics::run_identity()
}
/// Emit a point on the process root, retaining additive caller summary facts
/// for process completion. Does not initialize tracing. Disabled is a no-op.
pub fn root_summary(attributes: Value) {
    crate::diagnostics::root_summary(attributes);
}
impl Span {
    /// Initialize idempotently and begin a scope. Attributes should be an object;
    /// non-object values are wrapped without failing the consumer.
    pub fn new(name: &str, attributes: Value) -> Self {
        initialize();
        Self(crate::diagnostics::Span::new(name, attributes))
    }
    /// Attach end attributes (replaces previously attached end attributes).
    pub fn data(&mut self, attributes: Value) {
        self.0.data(attributes);
    }
    /// Record a stable error kind/message on this exact call span. It also
    /// sets the default terminal outcome to error; end may supply a more precise
    /// observed outcome. No effect when local tracing is disabled.
    pub fn record_error(&mut self, kind: &str, message: &str) {
        self.0.record_error(kind, message);
    }
    /// End this call with its observed outcome. Dropping also closes the span.
    pub fn end(self, outcome: Outcome) {
        self.0.end(outcome.as_str());
    }
}
