# Offline explanations and reviewable reports

`oxblend help error KIND [--json]` is version-matched and works offline for every
built-in kind at the source revision. A kind is not a root-cause verdict. For a
wrapper such as `incomplete_pack`, inspect every causal kind. Unknown kinds are
preserved in an explicit unknown-kind usage error; do not alias or suppress them.

Hard-error stderr envelopes retain schema 1, kind, message and the existing exit
semantics. They add catalog explanation/likely_causes/next_steps/known_issue and
an invocation-local `diagnostic` receipt. The receipt is available without local
tracing and records selected exit/outcome, exact accepted stdout-byte SHA-256,
the canonical original error-envelope SHA-256, publication/completeness facts,
causal failure IDs and captured source facts. Canonical stderr means compact
original schema-1 envelope **without diagnostic**, plus newline; this avoids a
self-referential hash. It excludes ancillary diagnostics and is not a digest of
the complete mixed stderr stream. A failed stdout delivery is independent of
already-published output. Failed final stderr delivery is separately recorded
when a trace survives and also keeps coverage partial. Exit is producer-selected;
root completion describes the recorded scope, not observation of a future OS
exit/signal. Signals can prevent a receipt entirely.

Native Errors carry a private exact diagnostic reference through forwarding and
TraceFailure's additive local `diagnostic_ref`. The report copy reuses the same
failure ID; forwarding observations are not duplicate failures. Command owners
supply explicit causal links, never inferred ordering. See DIAGNOSTIC-HOOK.md.

## Keep the evidence; do not rerun a write to obtain it

Retain stdout, stderr and producer exit immediately. Stderr may contain reader
warnings before its final complete schema-1 JSON envelope. Save that **entire
parsed envelope**, not a regex-generated reconstruction, as `failure.json`.
`report --report` expects a single JSON document, not mixed stderr prose. The
private receipt can retain paths; it must go through export before sharing.

```
oxblend help error incomplete_pack
oxblend help error dangling_pointer  # only when actually present as a cause
oxblend report --report failure.json --stdout saved.stdout --output bug.zip --preview
```

`--stdout` is optional but verifies the actual saved bytes against the receipt,
and includes a closed derivative of a saved JSON stdout report. A standalone
receipt plus matching stdout does not independently verify its claimed original
stderr envelope. Incomplete delivery stays partial even when accepted-byte
hashes match. Report-only legacy inputs remain `binding:unverified` with unknown final completion. Missing
trace/source facts stay missing; the exporter never opens a scene path recovered
from evidence, reconstructs an authoritative error, or retries the command.

## Trace-backed evidence

Local tracing remains opt-in with `FLATTRACE_DIR`. The same receipt and its hash
are emitted in a `diagnostic.receipt` point before root completion. Binding uses
L5's stable `{trace, root_span, identity_kind:"trace_root_span"}` pair, **not the
W3C trace ID alone** (which can be shared by multiple processes). A separate
random `diagnostic.run_id` identifies the CLI invocation for user selection.

```
oxblend report --trace selected.jsonl --report failure.json --output bug.zip --preview
oxblend report --run-id EXACT_ID --trace-dir /explicit/local/dir --output bug.zip --preview
oxblend report --last --trace-dir /explicit/local/dir --output bug.zip --preview
```

Selection never scans the machine or another home. `--last` uses validated
recorded UTC completion chronology, not filesystem mtime; it excludes help,
report, skill, version, normalized invalid-command and incomplete/unbound runs,
and refuses equal chronology. Byte-identical copies are deduplicated. Unvalidated
non-candidates, foreign-owned files and symlinks are skipped with local per-file
diagnostics; a malformed artifact claiming the exact requested ID refuses.
Nonidentical claims of an explicit ID, mismatched report/trace receipts, wrong ancestry,
sequence corruption, inconsistent final outcomes and changed hashes refuse.
Interrupted explicitly selected traces remain unknown, never successful. The
trace parser checks envelope/ancestry/binding; L5/L9's independent semantic gate
checks required phases and per-command workload coverage.

A legacy saved report never adopts the selected trace's producer metadata or
verification. Its producer_run remains unknown/unverified; selected_trace_run
and build.selected_trace_producer describe only that separate trace. Individual
source observations name input_origin/input_binding. Legacy retained source
coordinates remain explicitly unverified, not upgraded by an unrelated trace.

## Privacy and exact preview/create

Default export is a closed structural allowlist. It omits raw messages, argv,
environment/host/user, payload, source-content hashes and scene bytes. Name/path,
custom schema/code, unknown kind and identity strings become fresh bundle-local
HMAC-SHA-256 tokens or are omitted. Key/map are never persisted or exported.
Omission coordinates use trusted keys/numeric positions or tokens, never unknown
caller keys. Saved pointer values and block coordinates can disclose allocation
topology; this is not a secret detector. Built-in catalog prose replaces messages.
Only instrumented checks have typed source context: coverage is explicitly
partial/unavailable, and absent facts are null, never reconstructed.

Preview returns every text member in full, exact sizes/hashes and privacy facts.
Its owner-private snapshot lives under the selected output parent; no ZIP is yet
published. `--include-names` explicitly discloses typed names/path hex and selected
trace paths. `--include-file FILE` explicitly captures **all** file contents,
including scripts, names, metadata and artwork; the binary member's size/hash and
whole-content warning are shown, not a misleading textual safety claim. The
member is generic user-supplied.bin/octet-stream: its format and relation to the
failed run are UNVERIFIED, not a certified input scene. Native
non-UTF-8 parent paths use a null readable receipt/output path plus an exact
`*_path_hex` alias; scene-derived archive member names are never used. Both
options apply only to that acquisition; creation cannot add or change them.

Use only a preview obtained directly from **your own** report command. Create
is exact-byte publication, not a re-sanitizer, producer-authentication or consent
gate for a preview+digest supplied by another party. Never accept such a foreign
preview as trusted just because its digest agrees.

After reviewing all members and warnings:

```
oxblend report --create PREVIEW.json --preview-sha256 SHA256 --output bug.zip
# Or discard that same owned snapshot deliberately:
oxblend report --discard-preview PREVIEW.json --preview-sha256 SHA256
```

Create reads only frozen snapshot bytes, verifies each hash/size and caller-held
receipt digest, rejects unexpected staged entries/symlinks, and atomically
publishes a fresh ZIP without replacement. Source changes/deletion after preview
cannot enter it. Create hashes the exact bytes held for writing, not a pathname
that is reopened after checking. Output parent/basename must match the review.
Success consumes its snapshot; failures preserve evidence for inspection. There
is no automatic expiry, retry or global last-failure database. Reports reuse the
native ZIP64 writer and atomic publication owner, accepting verified immutable
member bytes rather than reopening checked source paths. No report-size or
runtime budget is invented. Acquisition/creation currently hold selected member
bodies in memory; archive output streams to its owned stage. The archive receipt says durability was not requested/proved.
A cleanup failure after publication is reported separately. Publication facts
are captured at the native atomic owner even if later reporting fails; atomic
file-step facts may describe staging and are not a final-package certificate.
Build facts distinguish producer/exporter and include the actual Linux binary
inode's SHA-256 when readable; unavailable facts remain null.

`manifest.json` lists every other member with actual exported-byte SHA-256;
`created_at` is UTC Unix seconds with `created_at_unit:unix_seconds_utc`.
`error-report.json`, `error-source.json`, `build.json`, README/issue draft and an
optional redacted `trace/events.jsonl` are derivatives, not verbatim originals.
`attachments/user-supplied.bin` exists only for explicit whole-file opt-in. No original
source names appear in archive entry names. Hash binding is integrity evidence,
not producer authentication or proof of user consent against an attacker who can
replace both the snapshot and the expected digest.

The returned GitHub URL contains only fixed public wording. The CLI never
opens it, sends data, files an issue, uploads, or calls Blender. Review the ZIP,
open the prefill yourself and attach manually. Reporting does not fix the scene
or certify an incomplete package, Blender opening, evaluation or rendering.
