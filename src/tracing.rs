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
/// One synchronous per-call scope, nested under the current thread's scope.
/// Not Send/Sync; unknown on drop unless end supplies the actual outcome.
pub struct Span(crate::diagnostics::Span);
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
    /// End this call with its observed outcome. Dropping also closes the span.
    pub fn end(self, outcome: Outcome) {
        self.0.end(outcome.as_str());
    }
}
