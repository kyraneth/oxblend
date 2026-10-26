//! Captured engine Session: one parsed root plus lazily captured linked
//! blends. Fast (default) retains immutable decoded bytes and proofs.
//! Bounded opt-in retains indexes, proofs and digests, re-materializing
//! decoded bodies through a shared single-slot verified working cache.
//!
//! A `Session` owns its arena by value. Parsed readers are never
//! shared by reference-counted pointer, never moved across threads while
//! borrowed, and never refreshed after a detected source change: a latched
//! entry keeps failing until the caller opens a new `Session`. The crate
//! makes no claim of an atomic filesystem snapshot; signatures are change
//! detectors with the documented same-size/same-tick limit, and the working
//! body adds a content digest verified at every re-materialization.
use crate::input::{Backing, Snapshot};
use crate::{BlendFile, Error, Result};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Source retention policy. Fast Sessions retain decoded captures; ephemeral
/// CLI packs use selective raw windows, with full capture for compressed blends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum MemoryMode {
    #[default]
    Fast,
    Bounded,
}
impl MemoryMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Bounded => "bounded",
        }
    }
}
impl std::str::FromStr for MemoryMode {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "fast" => Ok(Self::Fast),
            "bounded" => Ok(Self::Bounded),
            _ => Err(Error::new(
                "usage",
                format!("unknown memory mode {value:?}; expected fast or bounded"),
            )),
        }
    }
}

/// Owned result of one engine call: the outcome plus every operation-local
/// diagnostic, on both success and failure.
#[derive(Debug)]
#[non_exhaustive]
pub struct Operation<T> {
    pub result: Result<T>,
    pub diagnostics: Vec<Diagnostic>,
}

impl<T> Operation<T> {
    pub(crate) fn new(result: Result<T>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            result,
            diagnostics,
        }
    }

    /// Convenience for callers that only need the outcome.
    pub fn into_result(self) -> Result<T> {
        self.result
    }

    pub fn is_ok(&self) -> bool {
        self.result.is_ok()
    }
}

/// Operation-local reader/fallback/check notices. These never imply complete
/// coverage or publication; domain warnings stay in their own reports.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Diagnostic {
    /// An untrusted seek table fell back to checked full streaming spill.
    /// Emitted at the transition before recovery, even when recovery fails.
    SeekTableFallback {
        path: Option<PathBuf>,
        reason: String,
    },
    /// A previously captured source failed its reuse/completion check. The
    /// entry is latched: later calls consulting it fail until a new Session.
    SourceCheckFailure { source: PathBuf, error: Error },
    /// A body error displaced by a competing completion-check error. The
    /// returned error is the check error; the body error is retained here.
    SecondaryError {
        phase: &'static str,
        source: Option<PathBuf>,
        error: Error,
    },
}

impl Diagnostic {
    /// Stable machine-readable kind for binding mapping.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SeekTableFallback { .. } => "seek_table_fallback",
            Self::SourceCheckFailure { .. } => "source_check_failure",
            Self::SecondaryError { .. } => "secondary_error",
        }
    }
}

/// Which source-guard contract an operation runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuardPolicy {
    /// Historical path-call behavior: by-path-hit short-circuit for
    /// inspection walks, snapshot checks for captured pack inputs, and the
    /// existing end guards with their error precedence.
    Legacy,
    /// Long-lived Session behavior: every consulted source is recorded as
    /// touched before its check can fail, reused entries are revalidated per
    /// encounter, changed entries latch, and all touched entries are
    /// revalidated at completion.
    Session,
}

/// Explicit per-operation diagnostic emitter. The channel carries notices to
/// the operation drain; the optional plain-function observer renders the
/// legacy CLI envelope synchronously at emission. Library entry points use
/// no observer and stay silent.
#[derive(Clone)]
pub(crate) struct DiagSink {
    tx: mpsc::Sender<Diagnostic>,
    pub observer: Option<fn(&Diagnostic)>,
}

impl DiagSink {
    /// Test-only discard sink: sends never block and are dropped, so
    /// direct-constructed readers stay silent without a legacy envelope.
    #[cfg(test)]
    pub(crate) fn test_sink() -> Self {
        let (tx, _rx) = mpsc::channel::<Diagnostic>();
        Self { tx, observer: None }
    }
    pub fn emit(&self, diagnostic: &Diagnostic) {
        if let Some(observe) = self.observer {
            observe(diagnostic);
        }
        let _ = self.tx.send(diagnostic.clone());
    }
}

/// Render the exact historical seek-fallback stderr envelope. The CLI passes
/// this as its observer; collected notices are not printed a second time.
pub(crate) fn cli_observer(diagnostic: &Diagnostic) {
    if let Diagnostic::SeekTableFallback { path, reason } = diagnostic {
        eprintln!(
            "{}",
            serde_json::json!({"warning":{"kind":"seek_table_fallback", "message":"untrusted seek table; using checked_full_streaming_spill", "reason":reason}, "path_hex":path.as_ref().map(|p|crate::value::hex(p.as_os_str().as_encoded_bytes()))})
        );
    }
}

/// Per-operation mutable state: guard policy, touched set, diagnostics,
/// frozen path configuration and the diagnostic channel pair.
pub(crate) struct OpCtx {
    pub policy: GuardPolicy,
    touched: Vec<SourceId>,
    seen: HashSet<usize>,
    pub diagnostics: Vec<Diagnostic>,
    rx: mpsc::Receiver<Diagnostic>,
    pub sink: DiagSink,
    pub cwd: PathBuf,
    // Frozen at the operation boundary: every lazy spill, backing decode
    // and workspace in this operation stages under this directory instead
    // of re-sampling the process environment mid-operation.
    pub temp_root: PathBuf,
}

impl OpCtx {
    pub fn new(
        policy: GuardPolicy,
        observer: Option<fn(&Diagnostic)>,
        cwd: PathBuf,
        temp_root: PathBuf,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            policy,
            touched: Vec::new(),
            seen: HashSet::new(),
            diagnostics: Vec::new(),
            rx,
            sink: DiagSink { tx, observer },
            cwd,
            temp_root,
        }
    }

    /// The check owner emits its failure once. Distinct failed checks remain
    /// evidence even when they concern the same source; callers never re-emit.
    pub fn announce(&mut self, source: PathBuf, error: Error) {
        self.emit(Diagnostic::SourceCheckFailure { source, error });
    }

    /// Record a consulted source in first-consultation order, before its
    /// check can fail. Session policy only; legacy walks track nothing.
    pub fn record_touch(&mut self, id: SourceId) {
        if self.policy != GuardPolicy::Session {
            return;
        }
        if self.seen.insert(id.0) {
            self.touched.push(id);
        }
    }

    pub fn touched(&self) -> &[SourceId] {
        &self.touched
    }

    pub fn emit(&mut self, diagnostic: Diagnostic) {
        self.sink.emit(&diagnostic);
        self.drain();
    }

    /// Move reader/emitted notices into the collected diagnostics, preserving
    /// emission order. Called after joins on every return path.
    pub fn drain(&mut self) {
        for diagnostic in self.rx.try_iter() {
            self.diagnostics.push(diagnostic);
        }
    }

    pub fn finish<T>(&mut self, result: Result<T>) -> Operation<T> {
        self.drain();
        Operation::new(result, std::mem::take(&mut self.diagnostics))
    }
}

/// Store-local stable identity for one parsed source. Append-only: entries
/// are never deleted, reused or referenced from another entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SourceId(pub usize);

/// One owned parsed source plus its validity latch.
/// Both variants hold whole readers by value; entries live in the arena
/// `Vec`, so no extra boxing indirection is added for the size gap.
#[allow(clippy::large_enum_variant)]
pub(crate) enum StoredBlend {
    /// Full checked capture: the parsed index with the inseparable original
    /// container proof and physical+decoded content digest. The decoded
    /// body lives only in the shared verified working slot, re-read and
    /// digest-compared on every re-materialization. The only pack-admissible
    /// variant.
    Captured(Snapshot),
    /// Selective inspection reader (legacy default/low-memory trace and
    /// list policies). Never pack-admissible.
    Inspection(BlendFile),
}

pub(crate) struct SourceEntry {
    pub source_path: PathBuf,
    pub changed: Option<Error>,
    pub blend: StoredBlend,
}

impl SourceEntry {
    fn reader(&self) -> Result<&BlendFile> {
        match &self.blend {
            StoredBlend::Captured(snapshot) => Ok(&snapshot.reader),
            StoredBlend::Inspection(reader) => Ok(reader),
        }
    }
}

/// Owned snapshot arena: absolute lexical path to append-only entry.
/// In bounded mode the shared working-body cache keeps at most one decoded
/// body resident; entries retain indexes, digests and proofs. Fast entries
/// retain their original immutable decoded bytes.
#[derive(Default)]
pub(crate) struct InputStore {
    blends: Vec<SourceEntry>,
    by_path: HashMap<PathBuf, SourceId>,
    pub(crate) bodies: std::sync::Arc<crate::body_cache::BodyCache>,
    pub(crate) memory: MemoryMode,
    /// L17 Phase 2: ephemeral CLI pack/pack-sequence routes raw blends to
    /// selective windows. Long-lived Sessions never set this (F4);
    /// pack_session's retained arena keeps full captures.
    pub(crate) selective_raw_ephemeral: bool,
}

impl InputStore {
    pub(crate) fn with_memory(memory: MemoryMode) -> Self {
        Self {
            memory,
            ..Self::default()
        }
    }
    fn get(&self, id: SourceId) -> Result<&SourceEntry> {
        self.blends
            .get(id.0)
            .ok_or_else(|| Error::new("usage", "unknown source identity"))
    }

    fn get_mut(&mut self, id: SourceId) -> Result<&mut SourceEntry> {
        self.blends
            .get_mut(id.0)
            .ok_or_else(|| Error::new("usage", "unknown source identity"))
    }

    pub fn reader(&self, id: SourceId) -> Result<&BlendFile> {
        self.get(id)?.reader()
    }

    /// Honest zero-persistent-retention probe: every captured entry must
    /// hold the verified working-set backing, never a private decoded body
    /// hidden in a different holder. Inspection readers are unaffected.
    #[cfg(test)]
    pub(crate) fn captured_entries_retain_no_bodies(&self) -> bool {
        self.blends.iter().all(|entry| match &entry.blend {
            StoredBlend::Captured(snapshot) => {
                matches!(snapshot.reader.data, Backing::Cached(_))
            }
            StoredBlend::Inspection(_) => true,
        })
    }

    /// Arena identity for an already-captured absolute lexical path.
    pub(crate) fn id_for_path(&self, path: &Path) -> Result<SourceId> {
        self.by_path
            .get(path)
            .copied()
            .ok_or_else(|| Error::new("usage", "no captured source for planned path"))
    }

    /// Forced phase-boundary verification of one captured entry's working
    /// body (see `BodyCache::revalidate`): re-read the live source and
    /// compare against the retained physical+decoded digest even when the
    /// body is resident. Owned by logical-reuse checks, physical-container
    /// admission and publication — never by per-field backing reads.
    pub(crate) fn revalidate_working_body(&mut self, id: SourceId) -> Result<()> {
        if self.memory == MemoryMode::Fast {
            return Ok(());
        }
        let verified = match &self.get(id)?.blend {
            StoredBlend::Captured(snapshot) => snapshot.verified_body().cloned(),
            StoredBlend::Inspection(_) => return Ok(()),
        };
        match verified {
            Some(verified) => verified.revalidate(),
            None => Err(Error::new(
                "usage",
                "captured source has no verified working body",
            )),
        }
    }

    /// Pack-admissible snapshot. Inspection entries refuse: no promotion
    /// from a selective reader to a captured proof.
    pub fn snapshot(&self, id: SourceId) -> Result<&Snapshot> {
        match &self.get(id)?.blend {
            StoredBlend::Captured(snapshot) => Ok(snapshot),
            StoredBlend::Inspection(_) => Err(Error::new(
                "usage",
                "inspection reader is not a captured pack input",
            )),
        }
    }

    fn insert_captured(&mut self, path: PathBuf, mut snapshot: Snapshot) -> SourceId {
        let id = SourceId(self.blends.len());
        if self.memory == MemoryMode::Bounded {
            self.adopt_working_body(&path, &mut snapshot);
        }
        self.blends.push(SourceEntry {
            source_path: path.clone(),
            changed: None,
            blend: StoredBlend::Captured(snapshot),
        });
        self.by_path.insert(path, id);
        id
    }

    /// Verified-on-reuse adoption: drop the capture-time decoded body into
    /// the single working slot and keep only the parsed index, content
    /// digest and capture signature in the entry. The freshly decoded
    /// bytes seed the slot, so the immediately following walk reuses them
    /// without a second physical read. No bytes are hidden in another
    /// holder: the arena holds none, the slot at most one.
    fn adopt_working_body(&mut self, path: &Path, snapshot: &mut Snapshot) {
        let bytes = match std::mem::replace(&mut snapshot.reader.data, Backing::Bytes(Vec::new())) {
            Backing::Bytes(bytes) => bytes,
            other => {
                snapshot.reader.data = other;
                return;
            }
        };
        let len = bytes.len();
        snapshot.reader.data =
            Backing::Cached(std::sync::Arc::new(crate::body_cache::VerifiedBody::new(
                self.bodies.clone(),
                path.to_path_buf(),
                len,
                snapshot.identity,
                crate::fetch::capture_signature(&snapshot.metadata),
            )));
        self.bodies.seed(path.to_path_buf(), bytes);
    }

    pub(crate) fn insert_inspection(&mut self, path: PathBuf, reader: BlendFile) -> SourceId {
        let id = SourceId(self.blends.len());
        self.blends.push(SourceEntry {
            source_path: path.clone(),
            changed: None,
            blend: StoredBlend::Inspection(reader),
        });
        self.by_path.insert(path, id);
        id
    }

    /// Resolve `path` against the operation-frozen cwd into the absolute
    /// lexical key shared by every traversal. Native byte paths are kept.
    pub fn resolve(op: &OpCtx, path: &Path) -> PathBuf {
        let joined = if path.is_absolute() {
            path.to_path_buf()
        } else {
            op.cwd.join(path)
        };
        crate::trace::lexical(&joined)
    }

    /// Capture or reuse one source. New captures parse fully before insert;
    /// capture/index failure inserts nothing.
    pub fn ensure(
        &mut self,
        batch: &mut CaptureBatch,
        path: &Path,
        captured: bool,
        low_memory: bool,
        op: &mut OpCtx,
    ) -> Result<SourceId> {
        if let Some(&id) = self.by_path.get(path) {
            if op.policy == GuardPolicy::Session {
                op.record_touch(id);
            }
            self.check_reuse(id, op)?;
            return Ok(id);
        }
        if captured {
            // Working-set bound: drop the idle resident body BEFORE the new
            // capture decodes its own body, so a decoding capture body and a
            // resident body never coexist (the bound is the largest single
            // source, never their sum). Live scan guards hold their own
            // verified bytes and are unaffected.
            if self.memory == MemoryMode::Bounded {
                self.bodies.evict_idle();
            }
            batch.memory = self.memory;
            // Ephemeral selective routing is construction-scoped: the
            // store owns the policy, the batch carries it to the capture.
            // Sessions keep this false (F4); CLI pack sets it true.
            batch.selective_raw = self.selective_raw_ephemeral;
            let snapshot = capture_blend(batch, path)?;
            let id = self.insert_captured(path.to_path_buf(), snapshot);
            op.record_touch(id);
            Ok(id)
        } else {
            // Inspection readers stage spill/backing decode under the
            // operation-frozen temp root.
            let temp = op.temp_root.clone();
            let reader = if low_memory {
                BlendFile::open_trace_low_memory(path, &op.sink, &temp)?
            } else {
                BlendFile::open_trace(path, &op.sink, &temp)?
            };
            let id = self.insert_inspection(path.to_path_buf(), reader);
            op.record_touch(id);
            Ok(id)
        }
    }

    /// Revalidate an already-captured entry. Session policy latches detected
    /// changes and reports typed diagnostics; legacy policy keeps the
    /// historical check behavior and error precedence untouched.
    /// Revalidate one already-captured entry. Each failed check announces
    /// its own typed notice; fresh changes latch, while io failures do not.
    /// Known latches refuse without refreshing the captured reader. Callers only
    /// propagate the error. Every retained-hit check emits a `reader.reuse`
    /// phase (source detail): path, guard policy and captured state with
    /// the explicit check outcome — proof of reuse without reparse.
    pub(crate) fn check_reuse(&mut self, id: SourceId, op: &mut OpCtx) -> Result<()> {
        let path = self.get(id)?.source_path.clone();
        let (captured, policy) = match self.get(id) {
            Ok(entry) => (
                matches!(entry.blend, StoredBlend::Captured(_)),
                match op.policy {
                    GuardPolicy::Session => "session",
                    GuardPolicy::Legacy => "legacy",
                },
            ),
            Err(_) => (false, "unknown"),
        };
        let mut span = crate::diagnostics::Span::new(
            "reader.reuse",
            if crate::diagnostics::enabled() {
                serde_json::json!({"path":path.to_string_lossy(),"path_hex":crate::value::hex(path.as_os_str().as_encoded_bytes()),"policy":policy,"captured":captured})
            } else {
                serde_json::Value::Null
            },
        );
        let result = self.check_reuse_inner(id, op, &path);
        span.record_result(&result);
        result
    }
    fn check_reuse_inner(&mut self, id: SourceId, op: &mut OpCtx, path: &Path) -> Result<()> {
        if op.policy != GuardPolicy::Session {
            // Legacy captured pack inputs keep their snapshot check; legacy
            // inspection walks reuse without a per-encounter check and rely
            // on the existing end guards. The typed notice is still
            // centralized here; legacy wrappers drain it silently.
            let entry = self.get(id)?;
            let result = match &entry.blend {
                StoredBlend::Captured(snapshot) => snapshot.check(path),
                StoredBlend::Inspection(_) => return Ok(()),
            }
            .and_then(|()| self.revalidate_working_body(id));
            if let Err(error) = &result {
                op.announce(path.to_path_buf(), error.clone());
            }
            return result;
        }
        if let Some(error) = self.get(id)?.changed.clone() {
            op.announce(path.to_path_buf(), error.clone());
            return Err(error);
        }
        // Established working-body verification failure (digest/signature
        // mismatch or disappearance at re-materialization): same refusal and
        // latch vocabulary as a signature change, owned by this check site.
        // No filesystem check runs on a latched source.
        if self.memory == MemoryMode::Bounded
            && let Some(error) = self.bodies.latched(path)
        {
            if error.kind == "input_changed"
                && let Some(entry) = self.get_mut(id).ok()
            {
                entry.changed = Some(error.clone());
            }
            op.announce(path.to_path_buf(), error.clone());
            return Err(error);
        }
        let check = match &self.get(id)?.blend {
            StoredBlend::Captured(snapshot) => snapshot.check(path),
            StoredBlend::Inspection(reader) => reader.check_source(),
        };
        if let Err(error) = check {
            let error = session_change(path, &error, self.get(id)?).unwrap_or(error);
            if error.kind == "input_changed" {
                self.get_mut(id)?.changed = Some(error.clone());
            }
            op.announce(path.to_path_buf(), error.clone());
            return Err(error);
        }
        // Phase-boundary verification: logical reuse must prove the live
        // source still matches the retained content digest even when the
        // working body is resident. Metadata signatures cannot express a
        // same-signature content change (filesystem timestamp granularity,
        // e.g. CIFS), and later publication consumes the live physical
        // container; the retained digest is the only proof that covers it.
        // Repeated per-field reads stay on the cheap resident path.
        if let Err(error) = self.revalidate_working_body(id) {
            if error.kind == "input_changed"
                && let Some(entry) = self.get_mut(id).ok()
            {
                entry.changed = Some(error.clone());
            }
            op.announce(path.to_path_buf(), error.clone());
            return Err(error);
        }
        Ok(())
    }

    /// Completion revalidation: root first, then distinct touched sources in
    /// first-consultation order, so every observed change latches and every
    /// failure is recorded even when an earlier one wins the return. Returns
    /// every failure observed; callers pick the primary outcome and, for
    /// pack, append them all to the preserved report.
    pub fn check_all(&mut self, root: SourceId, op: &mut OpCtx) -> Vec<(PathBuf, Error)> {
        self.check_all_roots(&[root], op)
    }
    /// Selected roots first, then distinct touched sources; one owning
    /// completion check implements both single- and multi-root transactions.
    pub fn check_all_roots(&mut self, roots: &[SourceId], op: &mut OpCtx) -> Vec<(PathBuf, Error)> {
        let mut seen = HashSet::new();
        let order = roots
            .iter()
            .copied()
            .chain(op.touched().iter().copied())
            .filter(|id| seen.insert(*id))
            .collect::<Vec<_>>();
        let mut failures = Vec::new();
        for id in order {
            let Some(entry) = self.blends.get(id.0) else {
                failures.push((
                    PathBuf::from(""),
                    Error::new("usage", "unknown source identity"),
                ));
                continue;
            };
            let path = entry.source_path.clone();
            if let Some(error) = entry.changed.clone() {
                // This entry was consulted through check_reuse (or latched
                // by an inner finalizer), which already diagnosed the check.
                // No new filesystem check occurs on an invalidated entry.
                failures.push((path, error));
                continue;
            }
            // An established working-body verification failure from this
            // operation completes as a source failure even when the live
            // metadata signature still matches the capture (identical
            // signature, different content). Announce and latch here; the
            // body work already carried its own typed error.
            if self.memory == MemoryMode::Bounded
                && let Some(error) = self.bodies.latched(&path)
            {
                if error.kind == "input_changed"
                    && let Some(entry) = self.blends.get_mut(id.0)
                {
                    entry.changed = Some(error.clone());
                }
                op.announce(path.clone(), error.clone());
                failures.push((path, error));
                continue;
            }
            let check = match &entry.blend {
                StoredBlend::Captured(snapshot) => snapshot.check(&path),
                StoredBlend::Inspection(reader) => reader.check_source(),
            };
            let captured = matches!(entry.blend, StoredBlend::Captured(_));
            match check {
                Ok(()) => {
                    // Completion is a phase boundary too: force the digest
                    // proof for captured entries (see check_reuse_inner).
                    if captured && let Err(error) = self.revalidate_working_body(id) {
                        if error.kind == "input_changed"
                            && let Some(entry) = self.blends.get_mut(id.0)
                        {
                            entry.changed = Some(error.clone());
                        }
                        op.announce(path.clone(), error.clone());
                        failures.push((path, error));
                    }
                }
                Err(error) => {
                    let latched =
                        session_change(&path, &error, entry).unwrap_or_else(|| error.clone());
                    // Latch only established changes; transient errors fail open
                    // for a later fresh check. This actual completion check owns
                    // its diagnostic, including a new cause after an earlier io.
                    if latched.kind == "input_changed"
                        && let Some(entry) = self.blends.get_mut(id.0)
                    {
                        entry.changed = Some(latched.clone());
                    }
                    op.announce(path.clone(), latched.clone());
                    failures.push((path, latched));
                }
            }
        }
        failures
    }

    /// Primary completion outcome: the first `input_changed` failure wins,
    /// otherwise the first failure. `None` means every check passed.
    pub fn primary_failure(failures: &[(PathBuf, Error)]) -> Option<&(PathBuf, Error)> {
        failures
            .iter()
            .find(|(_, error)| error.kind == "input_changed")
            .or_else(|| failures.first())
    }

    pub fn primary_check(failures: &[(PathBuf, Error)]) -> Option<Error> {
        Self::primary_failure(failures).map(|(_, error)| error.clone())
    }
}

/// Decide whether a failed reuse/completion check establishes a source
/// change. Established change identity/length/times, disappearance or
/// replacement is `input_changed`; permission/device/read failures stay
/// `io` unless a changed signature is actually observed.
fn session_change(path: &Path, error: &Error, entry: &SourceEntry) -> Option<Error> {
    if matches!(error.kind, "input_changed" | "truncated" | "source_changed") {
        if error.kind == "input_changed" {
            return Some(error.clone());
        }
        return Some(Error::new("input_changed", error.message.clone()));
    }
    // Only an observed disappearance or signature divergence proves
    // replacement. Permission/device failures stay `io` exactly as reported
    // and never latch without an established signature change.
    let current = match path.metadata() {
        Ok(current) => current,
        Err(io) if io.kind() == std::io::ErrorKind::NotFound => {
            return Some(Error::new(
                "input_changed",
                format!("captured source disappeared: {}", error.message),
            ));
        }
        Err(_) => return None,
    };
    let stored = match &entry.blend {
        StoredBlend::Captured(snapshot) => Some(snapshot.metadata.clone()),
        StoredBlend::Inspection(_) => None,
    };
    if let Some(stored) = stored
        && crate::fetch::capture_signature(&current) != crate::fetch::capture_signature(&stored)
    {
        return Some(Error::new(
            "input_changed",
            format!("captured source replaced: {}", error.message),
        ));
    }
    None
}

/// Private provenance crosses nested finalizers beside the public Error.
/// Never infer causality from equal kind/message strings: independent work
/// can fail identically to a source check and must still be retained.
#[derive(Debug)]
pub(crate) enum BodyErr {
    Independent(Error),
    Guard { source: PathBuf, error: Error },
}
pub(crate) type BodyResult<T> = std::result::Result<T, BodyErr>;
impl BodyErr {
    pub fn as_error(&self) -> &Error {
        match self {
            Self::Independent(error) | Self::Guard { error, .. } => error,
        }
    }
}
impl From<Error> for BodyErr {
    fn from(error: Error) -> Self {
        Self::Independent(error)
    }
}
impl From<BodyErr> for Error {
    fn from(error: BodyErr) -> Self {
        match error {
            BodyErr::Independent(error) | BodyErr::Guard { error, .. } => error,
        }
    }
}

/// Completion selects the primary cause; only independent displaced work
/// adds SecondaryError. A propagated guard already represented by this
/// source's completion check does not become a fictitious root body error.
/// Keep the selected guard tag until the outer public operation boundary.
pub(crate) fn join_body<T>(
    body: BodyResult<T>,
    checks: &[(PathBuf, Error)],
    op: &mut OpCtx,
    phase: &'static str,
    source: Option<PathBuf>,
) -> BodyResult<T> {
    let Some((check_source, check)) = InputStore::primary_failure(checks) else {
        return body;
    };
    if let Err(body_error) = body {
        let displaced = match body_error {
            BodyErr::Independent(error) => Some((source, error)),
            BodyErr::Guard { source, error } => {
                if checks.iter().any(|(checked, _)| checked == &source) {
                    None
                } else {
                    Some((Some(source), error))
                }
            }
        };
        if let Some((source, error)) = displaced {
            op.emit(Diagnostic::SecondaryError {
                phase,
                source,
                error,
            });
        }
    }
    Err(BodyErr::Guard {
        source: check_source.clone(),
        error: check.clone(),
    })
}

/// Operation-local fetch ownership: worker lanes plus pending and opaque
/// captures. Dropped (workers joined) before the operation returns; linked
/// blend snapshots stay in the store, opaque bytes are observed afresh by
/// the next operation.
#[derive(Default)]
pub(crate) struct CaptureBatch {
    memory: MemoryMode,
    /// L17 Phase 2: when true (ephemeral CLI pack only), blend captures
    /// prefer selective raw windows and decline worker blend prefetch,
    /// mirroring the bounded no-queue rule. Never set for Sessions.
    pub(crate) selective_raw: bool,
    fetcher: Option<crate::fetch::Fetcher>,
    pending: HashMap<PathBuf, mpsc::Receiver<Result<crate::fetch::Raw>>>,
    opaque: HashMap<PathBuf, crate::fetch::Raw>,
}

impl CaptureBatch {
    pub(crate) fn with_memory(memory: MemoryMode) -> Self {
        Self {
            memory,
            ..Self::default()
        }
    }
    fn fetcher(&mut self, hint: usize) -> Result<&crate::fetch::Fetcher> {
        if self.fetcher.is_none() {
            // A fetcher with no lane can never answer: keep at least one.
            self.fetcher = Some(match self.memory {
                MemoryMode::Fast => crate::fetch::Fetcher::new(hint.max(1))?,
                MemoryMode::Bounded => {
                    crate::fetch::Fetcher::with_memory(hint.max(1), self.memory)?
                }
            });
        }
        Ok(self.fetcher.as_ref().unwrap())
    }

    /// Size the worker lanes once, before the first prefetch, mirroring the
    /// historical multi-root admission budget.
    pub fn prime(&mut self, hint: usize) -> Result<()> {
        self.fetcher(hint).map(|_| ())
    }

    pub fn prefetch(&mut self, path: &Path, blend: bool) -> Result<()> {
        if blend && self.selective_raw && self.memory == MemoryMode::Fast {
            // L17 selective fast: no queued blend reply may retain a full
            // decoded body. Roots and linked blends capture on demand
            // through guarded windows; the existing take() path consumes
            // immediately. Opaque (non-blend) prefetch is unaffected.
            return Ok(());
        }
        if blend && self.memory == MemoryMode::Bounded {
            // Working-set bound: blend captures are never queued ahead. A
            // queued blend reply holds a fully decoded body, so a single
            // worker finishing many queued jobs would retain many bodies at
            // once; only the on-demand `take` below may start a blend
            // capture, and it consumes the reply immediately.
            return Ok(());
        }
        if self.opaque.contains_key(path) || self.pending.contains_key(path) {
            if blend {
                self.fetcher(1)?.promote(path);
            }
            return Ok(());
        }
        let reply = self.fetcher(1)?.submit(path, blend);
        self.pending.insert(path.to_path_buf(), reply);
        Ok(())
    }

    /// White-box test access to the operation-local pending queue.
    #[cfg(test)]
    pub(crate) fn take_raw(&mut self, path: &Path, blend: bool) -> Result<crate::fetch::Raw> {
        match self.take(path, blend)? {
            TakenRaw::Opaque(raw) => Ok(raw),
        }
    }
    /// White-box test reinsertion into the operation-local opaque set.
    #[cfg(test)]
    pub(crate) fn stage_raw(&mut self, path: PathBuf, raw: crate::fetch::Raw) {
        self.put_opaque(path, raw)
    }
    /// White-box test proof that no opaque capture retains asset bytes.
    #[cfg(test)]
    pub(crate) fn opaque_bodies_empty(&self) -> bool {
        self.opaque.values().all(|raw| raw.bytes.is_empty())
    }
    /// White-box test probe: no blend reply may ever sit queued.
    #[cfg(test)]
    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }
    fn take(&mut self, path: &Path, blend: bool) -> Result<TakenRaw> {
        // Bounded and L17-selective fast both decline queued blend replies:
        // capture on demand and consume immediately, so no full decoded
        // body ever waits in the queue (selective reaches here only for
        // compressed fallbacks, which keep full bodies by contract).
        if blend && (self.memory == MemoryMode::Bounded || self.selective_raw) {
            // On-demand blend capture: promote an earlier opaque job when
            // one is still queued, then submit-and-consume in one step. No
            // decoded blend reply is ever left waiting in a queue.
            if let Some(reply) = self.pending.remove(path) {
                self.fetcher(1)?.promote(path);
                let raw = reply.recv().map_err(|_| {
                    Error::new("io", "private physical fetch ended without result")
                })??;
                return Ok(TakenRaw::Opaque(raw));
            }
            let reply = self.fetcher(1)?.submit(path, true);
            let raw = reply
                .recv()
                .map_err(|_| Error::new("io", "private physical fetch ended without result"))??;
            return Ok(TakenRaw::Opaque(raw));
        }
        self.prefetch(path, blend)?;
        if let Some(raw) = self.opaque.remove(path) {
            return Ok(TakenRaw::Opaque(raw));
        }
        let reply = self
            .pending
            .remove(path)
            .ok_or_else(|| Error::new("io", "private physical fetch ended without result"))?;
        let raw = reply
            .recv()
            .map_err(|_| Error::new("io", "private physical fetch ended without result"))??;
        Ok(TakenRaw::Opaque(raw))
    }

    fn put_opaque(&mut self, path: PathBuf, raw: crate::fetch::Raw) {
        self.opaque.insert(path, raw);
    }
}

enum TakenRaw {
    Opaque(crate::fetch::Raw),
}

/// Shared capture-one helper: the exact checked blend capture path used by
/// Session entries and typed-edit snapshots, without Session path
/// normalization. Capture/index failure inserts nothing.
pub(crate) fn capture_one(path: &Path) -> Result<Snapshot> {
    let mut batch = CaptureBatch::default();
    let outcome = capture_blend(&mut batch, path);
    // Join workers before returning, including on error paths.
    drop(batch);
    outcome
}

/// Full blend capture with the historical promotion, signature and proof
/// checks: a background opaque job already started before promotion
/// releases its bytes and refetches as blend in the same lane; the original
/// capture signature must still match, with no new cache.
fn capture_blend(batch: &mut CaptureBatch, path: &Path) -> Result<Snapshot> {
    let mut span = crate::diagnostics::Span::new(
        "snapshot",
        if crate::diagnostics::enabled() {
            serde_json::json!({"path":path.to_string_lossy(),"path_hex":crate::value::hex(path.as_os_str().as_encoded_bytes())})
        } else {
            serde_json::Value::Null
        },
    );
    let result = (|| {
        // L17 Phase 2 selective fast path (ephemeral CLI pack only): no
        // worker prefetch, no full-body materialization. Compressed
        // families return None and fall through to the historical full
        // capture below; the snapshot check runs under the standard
        // snapshot/index context, exactly like the full path. Read-site
        // errors already carry the worker-identical fetch/read snapshot
        // context from the selective reader itself; parse errors stay
        // single-context like the full path. Real failures refuse here
        // and never fall back.
        if batch.selective_raw
            && batch.memory == MemoryMode::Fast
            && let Some(snapshot) = crate::input::capture_selective_raw(path)?
        {
            snapshot.check(path)?;
            return Ok(snapshot);
        }
        let taken = batch.take(path, true)?;
        let TakenRaw::Opaque(mut raw) = taken;
        if raw.proof.is_none() {
            let expected = crate::fetch::capture_signature(&raw.metadata);
            drop(raw);
            let reply = batch.fetcher(1)?.submit(path, true);
            raw = reply
                .recv()
                .map_err(|_| Error::new("io", "private physical fetch ended without result"))??;
            if crate::fetch::capture_signature(&raw.metadata) != expected {
                return Err(Error::new(
                    "input_changed",
                    "source signature changed since snapshot",
                ));
            }
        }
        let file = File::open(path).map_err(io_error)?;
        let metadata = raw.metadata;
        if crate::fetch::capture_signature(&file.metadata().map_err(io_error)?)
            != crate::fetch::capture_signature(&metadata)
            || crate::fetch::capture_signature(&path.metadata().map_err(io_error)?)
                != crate::fetch::capture_signature(&metadata)
        {
            return Err(Error::new(
                "input_changed",
                "source signature changed since snapshot",
            ));
        }
        let decoded = raw.bytes;
        // The capture path only admits proof-carrying blends: a missing
        // proof means the fetch lane did not run the checked blend capture.
        let proof = raw.proof.ok_or_else(|| {
            Error::new(
                "input_changed",
                "blend capture produced no container proof",
            )
        })?;
        // The same invariant holds for the content digest computed by the
        // capture reads: without it the working body could never be verified
        // on reuse, so it refuses as a capture identity failure.
        let identity = if batch.memory == MemoryMode::Fast {
            crate::body_cache::SourceDigest { physical: 0, decoded: 0 }
        } else { raw.digest.ok_or_else(|| {
            Error::new(
                "input_changed",
                "blend capture produced no content digest",
            )
        })? };
        // No pack-size-dependent retained descriptor: the checked source ends
        // here, the arena retains the parsed index and identity, and payload
        // reads re-materialize the body through the verified working slot.
        drop(file);
        let snapshot = Snapshot {
            metadata,
            reader: BlendFile::from_bytes(decoded)?.with_source_path(path),
            identity,
            proof,
        };
        snapshot.check(path)?;
        span.data(serde_json::json!({"decoded_bytes":snapshot.reader.decompressed_len(),"blocks":snapshot.reader.blocks().len()}));
        Ok(snapshot)
    })()
    .map_err(|e| crate::edit_output::session_context(e, "snapshot/index", path));
    span.result(result)
}

fn io_error(error: std::io::Error) -> Error {
    Error::new(
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            "truncated"
        } else {
            "io"
        },
        error.to_string(),
    )
}

/// Guarded physical transport for pack members, preserving the historical
/// opaque/blend contracts: signature checks around capture and copy,
/// identity-hash comparison for opaque bodies, kernel transport for parsed
/// blends with the original decoded proof.
pub(crate) fn copy_physical(
    store: &mut InputStore,
    batch: &mut CaptureBatch,
    path: &Path,
    expected: &std::fs::Metadata,
    output: &mut File,
) -> Result<u64> {
    copy_physical_using(
        store,
        batch,
        path,
        expected,
        output,
        crate::kernel_copy::full,
    )
}

pub(crate) fn copy_physical_using(
    store: &mut InputStore,
    batch: &mut CaptureBatch,
    path: &Path,
    expected: &std::fs::Metadata,
    output: &mut File,
    transport: impl FnOnce(
        &File,
        &Path,
        &std::fs::Metadata,
        &mut File,
        Option<&crate::input::Backing>,
    ) -> Result<u64>,
) -> Result<u64> {
    // A captured blend member keeps the guarded kernel transport with its
    // original decoded proof, exactly as planned.
    if store
        .id_for_path(path)
        .ok()
        .and_then(|id| store.snapshot(id).ok())
        .is_some()
    {
        let id = store.id_for_path(path)?;
        let current = File::open(path).map_err(io_error)?;
        if crate::fetch::capture_signature(&current.metadata().map_err(io_error)?)
            != crate::fetch::capture_signature(expected)
        {
            return Err(Error::new(
                "input_changed",
                "asset source changed before captured copy",
            ));
        }
        // Physical-container admission is a phase boundary: prove the live
        // source still matches the retained digest BEFORE any transport
        // runs. Metadata checks cannot express a same-signature content
        // change, and the transport below consumes the live container; a
        // mismatch latches and refuses so old proofs never bless new bytes.
        store.revalidate_working_body(id)?;
        let copied = {
            let snapshot = store.snapshot(id)?;
            // This is actual retained-reader reuse at the physical transport
            // owner, not a new parse. Keep the pre-existing source check and
            // its exact error; the span merely records its real outcome.
            let reuse = crate::diagnostics::Span::new(
                "reader.reuse",
                if crate::diagnostics::enabled() {
                    serde_json::json!({"path":path.to_string_lossy(),"captured":true,"reason":"physical transport reuses retained parsed capture"})
                } else {
                    serde_json::Value::Null
                },
            );
            reuse.result(snapshot.check(path))?;
            if crate::fetch::capture_signature(&snapshot.metadata)
                != crate::fetch::capture_signature(expected)
            {
                return Err(Error::new(
                    "input_changed",
                    "captured blend differs from planned signature",
                ));
            }
            // Propagate transport errors without rewriting them: a
            // kernel signature-guard failure keeps its exact typed origin
            // ("source signature changed before publication"), and a
            // digest failure raised by the verified backing keeps its
            // latched diagnostic. Infer nothing from kind or message; the
            // surrounding FD/PATH guards below carry the copy context.
            transport(&current, path, expected, output, snapshot.raw_capture()).map_err(
                |error| {
                    if store.memory == MemoryMode::Fast && error.kind == "input_changed" {
                        Error::new("input_changed", "asset source changed during copy")
                    } else {
                        error
                    }
                },
            )?
        };
        if copied != expected.len()
            || crate::fetch::capture_signature(&current.metadata().map_err(io_error)?)
                != crate::fetch::capture_signature(expected)
            || crate::fetch::capture_signature(&path.metadata().map_err(io_error)?)
                != crate::fetch::capture_signature(expected)
        {
            return Err(Error::new(
                "input_changed",
                "asset source changed during copy",
            ));
        }
        return Ok(copied);
    }
    let taken = batch.take(path, false)?;
    (|| {
        let current = File::open(path).map_err(io_error)?;
        if crate::fetch::capture_signature(&current.metadata().map_err(io_error)?)
            != crate::fetch::capture_signature(expected)
        {
            return Err(Error::new(
                "input_changed",
                "asset source changed before captured copy",
            ));
        }
        let TakenRaw::Opaque(raw) = taken;
        let copied = if let Some(expected_hash) = raw.asset_hash {
            if crate::fetch::capture_signature(&raw.metadata)
                != crate::fetch::capture_signature(expected)
            {
                batch.put_opaque(path.to_path_buf(), raw);
                return Err(Error::new(
                    "input_changed",
                    "captured asset differs from planned signature",
                ));
            }
            let mut copy_span = crate::diagnostics::Span::new(
                "copy",
                if crate::diagnostics::enabled() {
                    serde_json::json!({"path":path.to_string_lossy(),"captured":true,"mechanism":"opaque_copy"})
                } else {
                    serde_json::Value::Null
                },
            );
            let copy_result = (|| {
                let (copied, hash) = crate::fetch::opaque_copy(&mut &current, output, || false)?;
                if hash != expected_hash {
                    return Err(Error::new(
                        "input_changed",
                        "asset bytes changed since capture",
                    ));
                }
                Ok(copied)
            })();
            if let Ok(bytes) = &copy_result
                && copy_span.active()
            {
                copy_span.data(serde_json::json!({"bytes":bytes}));
            }
            batch.put_opaque(path.to_path_buf(), raw);
            copy_span.result(copy_result)?
        } else {
            // A blend promoted/captured earlier still owns immutable bytes
            // and its container proof; do not discard that proof.
            if crate::fetch::capture_signature(&raw.metadata)
                != crate::fetch::capture_signature(expected)
            {
                batch.put_opaque(path.to_path_buf(), raw);
                return Err(Error::new(
                    "input_changed",
                    "captured asset differs from planned signature",
                ));
            }
            let mut copy_span = crate::diagnostics::Span::new(
                "copy",
                if crate::diagnostics::enabled() {
                    serde_json::json!({"path":path.to_string_lossy(),"captured":true,"mechanism":"captured_bytes"})
                } else {
                    serde_json::Value::Null
                },
            );
            let copied = std::io::copy(&mut Cursor::new(&raw.bytes), output).map_err(io_error);
            if let Ok(bytes) = &copied
                && copy_span.active()
            {
                copy_span.data(serde_json::json!({"bytes":bytes,"mechanism":"captured_bytes"}));
            }
            let copied = copy_span.result(copied)?;
            batch.put_opaque(path.to_path_buf(), raw);
            copied
        };
        if copied != expected.len()
            || crate::fetch::capture_signature(&current.metadata().map_err(io_error)?)
                != crate::fetch::capture_signature(expected)
            || crate::fetch::capture_signature(&path.metadata().map_err(io_error)?)
                != crate::fetch::capture_signature(expected)
        {
            return Err(Error::new(
                "input_changed",
                "asset source changed during copy",
            ));
        }
        Ok(copied)
    })()
}

/// Frozen path configuration sampled once at open. Later Session calls
/// resolve against this stored root instead of re-sampling the process.
#[derive(Debug, Clone)]
pub(crate) struct OperationDefaults {
    pub cwd: PathBuf,
    pub temp_root: PathBuf,
}

/// One captured root with its retained linked-blend arena. Owns every
/// parsed index and proof by value. Fast retains decoded bytes; bounded
/// uses the shared verified working slot. Dropping releases the arena. Python owns
/// deterministic close by dropping its handle under the lock.
pub struct Session {
    pub(crate) root: SourceId,
    pub(crate) roots: Vec<SourceId>,
    pub(crate) store: InputStore,
    pub(crate) defaults: OperationDefaults,
}

impl std::fmt::Debug for Session {
    // Root identity and arena size only: never decoded bytes or proofs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("root", &self.root_path())
            .field("sources", &self.store.blends.len())
            .finish()
    }
}
impl Session {
    /// Eagerly capture and parse one root with the existing checked blend
    /// capture path. Propagates the existing fetch/parse root-open error
    /// kinds unchanged; no new open kind is invented.
    pub fn open(path: impl AsRef<Path>) -> Operation<Session> {
        Self::open_with_memory(path, MemoryMode::Fast)
    }

    /// Open with an explicit retention policy, fixed for this Session lifetime.
    pub fn open_with_memory(path: impl AsRef<Path>, memory: MemoryMode) -> Operation<Session> {
        Self::open_many(std::iter::once(path), memory)
    }

    /// Capture ordered lexical roots together. A single arena retains every
    /// root and later linked capture; failed fresh roots are never cached.
    pub fn open_many<P: AsRef<Path>>(
        paths: impl IntoIterator<Item = P>,
        memory: MemoryMode,
    ) -> Operation<Session> {
        crate::tracing::initialize();
        let paths = paths
            .into_iter()
            .map(|p| p.as_ref().to_path_buf())
            .collect::<Vec<_>>();
        let cwd = std::env::current_dir();
        let temp_root = std::env::temp_dir();
        let mut fields = serde_json::json!({
            "cwd":cwd.as_ref().ok().map(|p|p.to_string_lossy()),
            "temp_root":temp_root.to_string_lossy(),"memory":memory.as_str()
        });
        if let [path] = paths.as_slice() {
            fields["path"] = serde_json::json!(path.to_string_lossy());
            fields["path_hex"] =
                serde_json::json!(crate::value::hex(path.as_os_str().as_encoded_bytes()));
        } else {
            fields["paths_hex"] = serde_json::json!(
                paths
                    .iter()
                    .map(|p| crate::value::hex(p.as_os_str().as_encoded_bytes()))
                    .collect::<Vec<_>>()
            );
        }
        let mut span = crate::diagnostics::Span::new("session.open", fields);
        let cwd = match cwd {
            Ok(cwd) => cwd,
            Err(error) => {
                let result = Err(crate::edit_output::io(error));
                span.record_result(&result);
                return Operation::new(result, Vec::new());
            }
        };
        let mut op = OpCtx::new(GuardPolicy::Session, None, cwd.clone(), temp_root.clone());
        let result = (|| {
            let paths = crate::multi_root::select_roots(
                paths.iter().map(|path| InputStore::resolve(&op, path)),
            );
            if paths.is_empty() {
                return Err(Error::new(
                    "usage",
                    "at least one selected root is required",
                ));
            }
            let mut store = InputStore::with_memory(memory);
            let mut batch = CaptureBatch::with_memory(memory);
            if paths.len() > 1 {
                batch.prime(paths.len())?;
                for path in &paths {
                    batch.prefetch(path, true)?;
                }
            }
            let mut roots = Vec::new();
            for path in &paths {
                roots.push(store.ensure(&mut batch, path, true, false, &mut op)?);
            }
            if let Some(error) = InputStore::primary_check(&store.check_all_roots(&roots, &mut op))
            {
                return Err(error);
            }
            Ok(Session {
                root: roots[0],
                roots,
                store,
                defaults: OperationDefaults { cwd, temp_root },
            })
        })();
        span.record_result(&result);
        op.finish(result)
    }

    /// Effective retention policy, fixed when opened.
    pub fn memory(&self) -> MemoryMode {
        self.store.memory
    }

    /// One checked root transaction returning lifetime-independent `T`. The
    /// closure observes the captured root reader and cannot leak a borrowed
    /// payload, schema or header: `T` owns its bytes. Query failures keep
    /// their original kind and message.
    ///
    /// The borrow-escape refusal below is compiler-enforced: a closure
    /// returning data borrowed from the arena reader does not satisfy the
    /// higher-ranked bound, so no borrowed payload can outlive the call.
    ///
    /// ```compile_fail
    /// # use oxblend::Session;
    /// # let mut session: Session = unreachable!();
    /// // ERROR: borrowed payload cannot escape the checked read.
    /// let leaked = session.read(|file| file.payload(0));
    /// ```
    pub fn read<T>(&mut self, f: impl for<'a> FnOnce(&'a BlendFile) -> Result<T>) -> Operation<T> {
        crate::tracing::initialize();
        if let Err(error) = self.require_single("read") {
            return Operation::new(Err(error), Vec::new());
        }
        // Reading consults only the retained root; no fetch lanes are
        // created and no linked capture can occur here.
        let mut span = crate::diagnostics::Span::new(
            "session.read",
            serde_json::json!({"cwd":self.defaults.cwd.to_string_lossy(),"temp_root":self.defaults.temp_root.to_string_lossy()}),
        );
        let mut op = OpCtx::new(
            GuardPolicy::Session,
            None,
            self.defaults.cwd.clone(),
            self.defaults.temp_root.clone(),
        );
        let result = (|| {
            // The precheck announces its own single diagnostic and never
            // enters the body. Touch is recorded before the check.
            op.record_touch(self.root);
            self.store.check_reuse(self.root, &mut op)?;
            // Retain the body outcome through the completion checks: a
            // body failure plus a competing change must report the change
            // and keep the body error as a SecondaryError, never escape
            // the closure error before finalizing the touched set.
            let body: Result<T> = {
                let reader = self.store.reader(self.root)?;
                f(reader)
            };
            let checks = self.store.check_all(self.root, &mut op);
            join_body(
                body.map_err(BodyErr::Independent),
                &checks,
                &mut op,
                "session.read",
                None,
            )
        })()
        .map_err(Error::from);
        span.record_result(&result);
        op.finish(result)
    }

    /// Absolute lexical identity of the captured root.
    pub fn root_path(&self) -> &Path {
        &self.store.blends[self.root.0].source_path
    }

    /// Every selected captured identity, in first-selection order.
    pub fn root_paths(&self) -> Vec<&Path> {
        self.roots
            .iter()
            .map(|id| self.store.blends[id.0].source_path.as_path())
            .collect()
    }

    /// Scalar operations must never implicitly select the first captured root.
    pub fn require_single(&self, operation: &str) -> Result<()> {
        if self.roots.len() != 1 {
            return Err(Error::new(
                "usage",
                if operation == "pack" {
                    "pack requires one captured root; use pack_sequence for open_many(paths)"
                } else {
                    "scalar operation requires one captured root; use open(path)"
                },
            ));
        }
        Ok(())
    }

    /// Validate every selected source without reading a root's scene data.
    /// No fetch/reopen occurs; all changed sources are latched and diagnosed.
    pub fn validate_sources(&mut self) -> Operation<()> {
        let mut op = self.operation_context(None);
        let checks = self.store.check_all_roots(&self.roots, &mut op);
        let result = InputStore::primary_check(&checks).map_or(Ok(()), Err);
        op.finish(result)
    }

    pub(crate) fn operation_context(&self, observer: Option<fn(&Diagnostic)>) -> OpCtx {
        OpCtx::new(
            GuardPolicy::Session,
            observer,
            self.defaults.cwd.clone(),
            self.defaults.temp_root.clone(),
        )
    }
}

#[cfg(test)]
fn assert_send<T: Send>() {}

#[cfg(test)]
mod ownership_probes {
    use super::*;

    #[test]
    fn join_preserves_origin_not_error_text() {
        let cwd = std::env::current_dir().unwrap();
        let a = cwd.join("source-A");
        let b = cwd.join("source-B");
        let error = || Error::new("input_changed", "identical failure text");
        let cases = [
            (
                BodyErr::Guard {
                    source: a.clone(),
                    error: error(),
                },
                a.clone(),
                false,
            ),
            (
                BodyErr::Guard {
                    source: a.clone(),
                    error: error(),
                },
                b,
                true,
            ),
            (BodyErr::Independent(error()), a, true),
        ];
        for (body, checked, secondary) in cases {
            let mut op = OpCtx::new(
                GuardPolicy::Session,
                None,
                cwd.clone(),
                std::env::temp_dir(),
            );
            let result = join_body::<()>(
                Err(body),
                &[(checked.clone(), error())],
                &mut op,
                "unit.join",
                None,
            );
            assert!(matches!(result, Err(BodyErr::Guard { source, .. }) if source == checked));
            assert_eq!(
                !op.diagnostics.is_empty(),
                secondary,
                "{:?}",
                op.diagnostics
            );
            assert!(
                op.diagnostics
                    .iter()
                    .all(|d| matches!(d, Diagnostic::SecondaryError { .. }))
            );
        }
    }

    #[test]
    fn session_arena_is_send_without_unsafe_or_shared_readers() {
        assert_send::<Session>();
        assert_send::<InputStore>();
        assert_send::<SourceEntry>();
        assert_send::<StoredBlend>();
        assert_send::<Snapshot>();
        assert_send::<BlendFile>();
        assert_send::<crate::fetch::Raw>();
        assert_send::<crate::edit_container::capture::Proof>();
        assert_send::<CaptureBatch>();
        assert_send::<crate::typed_edit::EditSnapshot>();
        // Spans stay thread-bound by construction (`PhantomData<Rc<()>>`
        // in diagnostics::Span, wrapped by tracing::Span): no Session,
        // store, batch, snapshot or fetch payload holds or moves one, and
        // moving a span across threads fails to compile by that marker.
    }
}
