//! Verified-on-reuse working bodies for captured blend sources.
//!
//! Explicit bounded-mode retention policy: a captured entry retains its
//! parsed index plus a content digest, never a
//! private per-entry decoded body. Payload reads borrow a single
//! operation-wide working-body slot: at most one decoded body stays resident,
//! so the retained working set is bounded by the largest source instead of
//! the sum of all decoded bytes.
//!
//! Verification is bound to re-materialization, not to every payload read: a
//! resident body was verified against its source when it was decoded (at
//! capture or on a later re-read), and continued reads keep using exactly
//! those verified bytes — the same never-mix guarantee the retained-arena
//! design gave. When a body must be re-read from the physical source, the
//! re-read rides the existing capture decode, recomputes the physical+decoded
//! digests, and compares them with the retained identity before the bytes are
//! served. A mismatch is the existing `input_changed` refusal: it latches per
//! source until a new owning arena exists, so stale parsed indexes never
//! interpret changed bytes and old/new data are never mixed. Established
//! disappearances and signature divergence latch identically; plain io
//! failures stay transient and fail open, mirroring `session_change`.
//!
//! Re-materialization alone is not enough at phase boundaries: metadata
//! signatures cannot express a same-signature content change (filesystem
//! timestamp granularity, e.g. CIFS), and publication consumes the live
//! physical container. Logical reuse checks, physical-container admission
//! and publication therefore FORCE a re-read+digest proof through
//! [`BodyCache::revalidate`] even when the body is resident, while ordinary
//! per-field reads keep the cheap resident path.
//!
//! The retained digest covers BOTH the physical container bytes and the
//! decoded stream. A recompressed container with identical decoded content
//! still mismatches, so the retained original-container proof (seek verdict)
//! can never be silently applied to a different physical file.
//!
//! Guards and diagnostics keep their owners: this cache records typed facts
//! (latch, counters, resident slot) silently; `InputStore::check_reuse` and
//! `check_all` announce them through the existing `SourceCheckFailure`
//! channel, and origin is carried by the latched per-source error object,
//! never inferred from kind/message strings.
//!
//! No private spool, reflink or persistent file cache exists here: the only
//! retained bytes are the single in-memory resident body plus bodies actively
//! held by scan/range guards (bounded by scan nesting depth).
use crate::fetch::CaptureSignature;
use crate::{Error, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Content identity of one captured source: xxh64 (seed zero) over the
/// physical container bytes and over the decoded stream, both computed by
/// the existing capture reads themselves (no second pass).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceDigest {
    pub physical: u64,
    pub decoded: u64,
}

/// Per-source working-body counters. `physical_rereads` counts re-reads of
/// the live source; `decoded_bytes` accumulates decoded bytes materialized
/// by those re-reads; `digest_verifications` counts executed digest
/// comparisons (one per re-materialization).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SourceCounters {
    pub physical_rereads: u64,
    pub decoded_bytes: u64,
    pub digest_verifications: u64,
}

/// Injectable re-materialization seam. Production uses `real_recheck`
/// (signature check + the shared capture decode with inline hashing); tests
/// substitute deterministic providers to prove digest-mismatch refusals that
/// metadata signature checks alone cannot express (identical signatures,
/// different content).
pub(crate) type RecheckProvider = fn(&Path, &CaptureSignature) -> Result<(Vec<u8>, SourceDigest)>;

#[derive(Debug)]
struct Inner {
    /// The single working-body slot: at most one verified decoded body.
    resident: Option<(PathBuf, Vec<u8>)>,
    /// Established verification failures per source. Only `input_changed`
    /// errors are ever latched here; io failures stay transient.
    latched: HashMap<PathBuf, Error>,
    counters: HashMap<PathBuf, SourceCounters>,
    recheck: RecheckProvider,
}

/// Shared working-body owner. One instance per `InputStore` arena; every
/// `Backing::Cached` of that arena holds it by `Arc`, so the guard must be
/// `Sync` (`Session` is `Send`). The lock is held only across field access,
/// never across file IO or user closures; nested acquires take it again
/// after the outer section released it, so they cannot deadlock either.
/// A poisoned lock is recovered, not propagated: every critical section is
/// panic-free (no user code under the lock), so the inner state is always
/// structurally consistent.
#[derive(Debug)]
pub(crate) struct BodyCache {
    inner: Mutex<Inner>,
}

impl Default for BodyCache {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                resident: None,
                latched: HashMap::new(),
                counters: HashMap::new(),
                recheck: real_recheck,
            }),
        }
    }
}

impl BodyCache {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // See the struct docs: critical sections are panic-free, so
        // poisoning cannot represent an inconsistent state.
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl BodyCache {
    /// Place freshly captured decoded bytes into the working slot. The
    /// capture decode already verified them against the source signature,
    /// so they enter resident without a second physical read. A newer
    /// capture simply replaces the slot; the evicted body is dropped, never
    /// stored per entry.
    pub(crate) fn seed(&self, path: PathBuf, bytes: Vec<u8>) {
        self.lock().resident = Some((path, bytes));
    }

    /// Established verification failure for a source, if any.
    pub(crate) fn latched(&self, path: &Path) -> Option<Error> {
        self.lock().latched.get(path).cloned()
    }

    fn latch(&self, path: &Path, error: Error) {
        self.lock().latched.insert(path.to_path_buf(), error);
    }

    /// Drop the idle resident body. Called before a new capture decodes its
    /// own body so the two never coexist: the working set stays bounded by
    /// the largest single source. Live guards hold their own bytes.
    pub(crate) fn evict_idle(&self) {
        self.lock().resident = None;
    }

    /// Take the resident body when it belongs to `path`. The caller owns the
    /// returned bytes exclusively; the slot is left empty until a guard
    /// releases a body back into it. A miss EVICTS the current resident
    /// (whatever source it belongs to): a single-slot cache must bind to
    /// the source being read, or every newly materialized body would be
    /// dropped at release and consecutive reads of one source would
    /// re-materialize forever.
    fn take_resident(&self, path: &Path) -> Option<Vec<u8>> {
        let mut inner = self.lock();
        match inner.resident.take() {
            Some((resident, bytes)) if resident == path => Some(bytes),
            evicted => {
                drop(evicted);
                None
            }
        }
    }

    /// Return a released body into the slot iff it is still empty; a body
    /// materialized by a nested acquire may have claimed it meanwhile, in
    /// which case the older body is dropped rather than retained twice.
    fn release(&self, path: &Path, bytes: Vec<u8>) {
        let mut inner = self.lock();
        if inner.resident.is_none() {
            inner.resident = Some((path.to_path_buf(), bytes));
        }
    }

    fn acquire(&self, verified: &VerifiedBody) -> Result<Vec<u8>> {
        if let Some(error) = self.latched(&verified.path) {
            return Err(error);
        }
        if let Some(bytes) = self.take_resident(&verified.path) {
            return Ok(bytes);
        }
        self.materialize(verified)
    }

    /// Forced phase-boundary verification: re-read the live source and
    /// prove it still matches the retained digest even when a resident body
    /// exists. Metadata signatures cannot express a same-signature content
    /// change (timestamp granularity), and publication consumes the live
    /// physical container, so reuse checks, admission and publication own a
    /// digest proof instead of trusting resident bytes. The freshly
    /// re-verified body re-enters the slot for cheap subsequent field reads;
    /// a mismatch latches exactly like a re-materialization mismatch.
    pub(crate) fn revalidate(&self, verified: &VerifiedBody) -> Result<()> {
        if let Some(error) = self.latched(&verified.path) {
            return Err(error);
        }
        drop(self.take_resident(&verified.path));
        let bytes = self.materialize(verified)?;
        self.release(&verified.path, bytes);
        Ok(())
    }

    /// Re-materialize and verify: run the injected provider (production:
    /// signature guards + shared capture decode with inline digest
    /// hashing), compare the recomputed content identity with the retained
    /// digest, latch any established change, and count every re-read.
    fn materialize(&self, verified: &VerifiedBody) -> Result<Vec<u8>> {
        // Re-materialization: re-read the live source through the injected
        // provider (production: signature check + shared capture decode with
        // inline digest hashing), then compare recomputed content identity
        // with the retained digest before serving anything.
        let recheck = self.lock().recheck;
        let (bytes, digest) = match recheck(&verified.path, &verified.signature) {
            Ok(materialized) => materialized,
            Err(error) => {
                // Established change vocabulary from the provider (source
                // disappearance, signature divergence) latches exactly like
                // a digest mismatch; transient io failures fail open.
                if error.kind == "input_changed" {
                    self.latch(&verified.path, error.clone());
                }
                return Err(error);
            }
        };
        let counters = SourceCounters {
            physical_rereads: 1,
            decoded_bytes: bytes.len() as u64,
            digest_verifications: 1,
        };
        if digest != verified.digest {
            let error = Error::new(
                "input_changed",
                format!(
                    "source content digest changed since capture (physical {:016x}->{:016x}, decoded {:016x}->{:016x})",
                    verified.digest.physical,
                    digest.physical,
                    verified.digest.decoded,
                    digest.decoded
                ),
            );
            self.bump(&verified.path, counters, verified.signature.2);
            self.latch(&verified.path, error.clone());
            return Err(error);
        }
        self.bump(&verified.path, counters, verified.signature.2);
        Ok(bytes)
    }

    fn bump(&self, path: &Path, add: SourceCounters, physical_size: u64) {
        let totals = {
            let mut inner = self.lock();
            let entry = inner.counters.entry(path.to_path_buf()).or_default();
            entry.physical_rereads += add.physical_rereads;
            entry.decoded_bytes += add.decoded_bytes;
            entry.digest_verifications += add.digest_verifications;
            *entry
        };
        // Reuse the registered reader.stats shape, once per full proof (not
        // per scalar field). No product JSON or public API changes. Stable
        // source physical reread bytes are count × capture-time size.
        if crate::diagnostics::enabled() {
            crate::diagnostics::point(
                "reader.stats",
                serde_json::json!({
                    "path":path.to_string_lossy(),
                    "path_hex":crate::value::hex(path.as_os_str().as_encoded_bytes()),
                    "source":{"mode":"verified_on_reuse",
                        "physical_rereads":totals.physical_rereads,
                        "decoded_bytes":totals.decoded_bytes,
                        "digest_verifications":totals.digest_verifications,
                        "physical_source_bytes":physical_size}
                }),
            );
        }
    }

    /// Working-set facts for the zero-persistent-retention assertions: how
    /// many bodies are resident (never more than one) and their total size.
    #[cfg(test)]
    pub(crate) fn resident_bodies(&self) -> usize {
        self.lock().resident.is_some() as usize
    }
    #[cfg(test)]
    pub(crate) fn resident_bytes(&self) -> usize {
        self.lock()
            .resident
            .as_ref()
            .map(|(_, bytes)| bytes.len())
            .unwrap_or(0)
    }
    pub(crate) fn counters(&self, path: &Path) -> SourceCounters {
        self.lock().counters.get(path).copied().unwrap_or_default()
    }

    /// Unit seam: replace the re-materialization provider.
    #[cfg(test)]
    pub(crate) fn set_recheck(&self, provider: RecheckProvider) {
        self.lock().recheck = provider;
    }
}

/// Per-source verified body identity held by `Backing::Cached`. Carries the
/// retained parsed-index boundary (decoded length), the capture-time
/// signature and digest, and the shared working-body cache.
#[derive(Debug)]
pub(crate) struct VerifiedBody {
    cache: std::sync::Arc<BodyCache>,
    pub(crate) path: PathBuf,
    pub(crate) len: usize,
    pub(crate) digest: SourceDigest,
    pub(crate) signature: CaptureSignature,
}

impl VerifiedBody {
    pub(crate) fn new(
        cache: std::sync::Arc<BodyCache>,
        path: PathBuf,
        len: usize,
        digest: SourceDigest,
        signature: CaptureSignature,
    ) -> Self {
        Self {
            cache,
            path,
            len,
            digest,
            signature,
        }
    }

    /// Guarded body access: the returned guard exclusively owns verified
    /// bytes for exactly this source. Latched sources refuse; resident hits
    /// continue the already-verified body; misses re-read and verify first.
    pub(crate) fn acquire(&self) -> Result<BodyGuard<'_>> {
        Ok(BodyGuard {
            owner: self,
            bytes: self.cache.acquire(self)?,
        })
    }

    /// Forced phase-boundary verification of this source (see
    /// [`BodyCache::revalidate`]): reuse checks, admission and publication
    /// call this even when the body is resident.
    pub(crate) fn revalidate(&self) -> Result<()> {
        self.cache.revalidate(self)
    }

    /// Established-failure consult for backing-level source checks.
    pub(crate) fn check(&self) -> Result<()> {
        match self.cache.latched(&self.path) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    pub(crate) fn counters(&self) -> SourceCounters {
        self.cache.counters(&self.path)
    }
}

/// Exclusive verified-body handle. Dropping it returns the body to the
/// working slot iff the slot is empty; a nested acquire may have claimed the
/// slot meanwhile, in which case these bytes are simply dropped. The guard
/// never holds the cache lock, so nested acquisition cannot deadlock and
/// can never be served another source's bytes.
pub(crate) struct BodyGuard<'a> {
    owner: &'a VerifiedBody,
    bytes: Vec<u8>,
}

impl std::ops::Deref for BodyGuard<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}

impl Drop for BodyGuard<'_> {
    fn drop(&mut self) {
        let bytes = std::mem::take(&mut self.bytes);
        self.owner.cache.release(&self.owner.path, bytes);
    }
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

/// Production re-materialization: open the live source, verify the
/// capture-time signature (disappearance and replacement keep their existing
/// `input_changed` vocabulary), then decode through the same shared capture
/// machinery that computed the retained digest inline. The decode result is
/// held while the post-read FD+PATH signature guards run — exactly like the
/// capture fetch — so an established source change dominates a decode
/// failure, and a raw-family length divergence refuses identically.
pub(crate) fn real_recheck(
    path: &Path,
    expected: &CaptureSignature,
) -> Result<(Vec<u8>, SourceDigest)> {
    let mut file = std::fs::File::open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            Error::new(
                "input_changed",
                format!("captured source disappeared: {error}"),
            )
        } else {
            io_error(error)
        }
    })?;
    let metadata = file.metadata().map_err(io_error)?;
    if crate::fetch::capture_signature(&metadata) != *expected {
        return Err(Error::new(
            "input_changed",
            "source signature changed since capture",
        ));
    }
    let captured = crate::fetch::capture_body(&mut file, &metadata, || false);
    if crate::fetch::capture_signature(&file.metadata().map_err(io_error)?)
        != crate::fetch::capture_signature(&metadata)
        || crate::fetch::capture_signature(&path.metadata().map_err(io_error)?)
            != crate::fetch::capture_signature(&metadata)
    {
        return Err(Error::new(
            "input_changed",
            "source signature changed during snapshot",
        ));
    }
    let (bytes, proof, digest) = captured?;
    if proof.family == "raw" && bytes.len() as u64 != metadata.len() {
        return Err(Error::new(
            "input_changed",
            "source signature changed during snapshot",
        ));
    }
    Ok((bytes, digest))
}

#[cfg(test)]
#[path = "body_cache_tests.rs"]
mod tests;
