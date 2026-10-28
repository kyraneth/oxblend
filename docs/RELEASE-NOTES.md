# Oxblend 1.4.1 — 2026-10-29

This is a performance release. `diff` output remains `oxblend.diff/v1`; there
is no default-format change or general completeness improvement for real changes.

- Decoded-equal inputs can return the existing equal report without building a
  pointer census. Inputs are still validated; captured-generation checks remain.
  S1 measured the container pair at 197.40 s / 570,248 KiB / 25,877,063 output
  bytes with shipped 1.4.0 versus 0.36–0.47 s / 20,516 KiB / 707 bytes after S1.
  This accepted change replaces an inconclusive result with complete/equal and
  zero rows for validated, decoded-identical input.
- Diff traversal indexes outgoing edges instead of repeatedly scanning all edges.
  On the same qualification guest, rename fell from 137.74 s to 9.67 s and the
  versions comparison from 135.82 s to 10.20 s (S2, relative to the S1 prefix).
- Buffered decoded spill writes speed up compressed reads. Same-guest wolf-18
  cold-file requests measured inspect at 11.79–12.99 s before versus 4.67–5.76 s
  after, and query at 9.02–12.60 s versus 1.57–1.97 s (S2b, relative to S2).
  These are measured workloads, not universal speed guarantees; inspect RSS
  overlapped observed run-to-run noise.
- Compact, schema-backed diff census retention reduces memory without dropping
  pointer evidence. Three S3 versions trials peaked at 481,948–482,096 KiB,
  versus 589,396–589,656 KiB before S3, with byte-identical 27,553,702-byte reports.
  Candidate walls were 9.64–10.11 s versus 10.06–10.90 s. These are incremental
  slice measurements, not all comparisons against shipped 1.4.0.
- Deterministic replacement qualification selects the capture phase. This is a
  qualification control, not a new runtime hook.

### Final matched measurements against shipped 1.4.0

Same qualification guest and exact paired inputs; wall seconds / peak RSS KiB /
stdout bytes. These are measured workloads, not universal speed guarantees.

| Pair | Shipped 1.4.0 | 1.4.1 | Result |
|---|---|---|---|
| l24-sh0019 | 48.29 / 337,820 / 15,343,265 | 5.15 / 272,704 / 15,343,265 | Byte-identical report; incomplete/null. |
| l24-sh0073 | 63.25 / 354,244 / 16,231,443 | 6.53 / 286,072 / 16,231,443 | Byte-identical report; incomplete/null. |
| versions-sh0027 | 163.39 / 589,692 / 27,553,708 | 12.32 / 482,100 / 27,553,708 | Byte-identical report; incomplete/null, 3,564 rows. |
| rename | 300.45 / 595,476 / 27,622,420 | 15.21 / 488,356 / 27,622,420 | Byte-identical report; incomplete/null, 5,270 rows. |
| container | 279.22 / 570,248 / 25,877,069 | 0.06 / 20,524 / 713 | Accepted decoded-equal repair: complete/equal, zero rows. |

Known limits: real changed production compositions still return incomplete
results (`complete:false`, `equal:null`). Faster output does not prove authored,
evaluated-scene or rendered-image equivalence. Very large or unrelated comp
pairs still run out of memory in `diff`: public Blender 5.2 splash versus
unrelated was guard-OOM-killed at 17.64 s / 3,268,768 KiB, with no report;
studio sh0419 was also guard-OOM-killed with no report (GNU wall/peak telemetry
was not captured). The former's historical 1.4.0 run took 497.194 s /
7,598,668 KiB before termination, on a different host/guard: this is not a
matched speed or memory-improvement claim. Large sh0103 delivered an
incomplete/null report in 42.52 s / 1,594,268 KiB; its 1.4.0 baseline OOM
delivered no report. Historical public45, native re-save and physical-control
observations likewise do not establish matched report equivalence.
The v2 answers, report/default-format change and further verdict work come
in 1.4.2; they are not included here.

# Oxblend 1.4.0 — 2026-10-26

- Image-source Cryptomatte nodes now resolve their embedded ImageUser through the file's SDNA, enabling the existing sequence/member completeness rules in list, trace and pack; render-source nodes remain non-image users.

- Default fast CLI pack/pack-sequence reads signature-checked selective raw
  windows rather than retaining whole decoded raw sources. Compressed blends
  fall back to full capture; Python Sessions retain captured sources. Bounded
  mode re-reads and verifies bodies, trading speed for lower peak memory.
  Measurements on selected workloads showed 27–53% lower fast-path heap;
  these are workload observations, not universal memory or timing guarantees.

- Replace the shipped logo with the approved Fab Garden studio-family variant b: O1 horns holding the O4 cube and short trace below. Full light/dark lockups retain variant a’s exact scale, spacing and baseline, adding b’s trace; README uses theme-aware SVGs. Penpot remains the design source of truth.

- Typed edit schema 2 adds generic guarded `idprop-string` value edits on
  same-layout 502/503 IDProperties: expected name/value, UTF-8 string type,
  existing-allocation capacity and no-alias guards; atomic payload/len update,
  preserving pointer/totallen/allocation. Structural node gates are unchanged.

- Avoid cloning the complete `inspect` block-array JSON tree. On the measured
  public Einar input (4,360,282 blocks), CLI peak RSS fell from 8,116,032 to
  4,680,676 KiB and wall time from 26.96 to 11.33 seconds, with byte-identical
  output. These are workload measurements, not memory or timing guarantees.

- Add `read-batch FILE --requests SPEC` (additive): one open
executes an ordered `{"schema":1,"requests":[...]}` plan of exactly the
shared `inspect`/`get` (`--raw`/`--byte-array`/`--pointer-table`)/`pointer`/`payload`
projections, streaming one JSONL item per request plus a terminal schema-1
summary with counts, input/input_recheck and input_changed. Empty plans are
valid and still check input. Success reports equal single-command stdout
bytes; failed items equal single CLI error objects exactly (no `diagnostic_ref`
inside) with the exact invocation-local sibling `diagnostic_ref` and no raw source snapshot. Independent failures use
additive `incomplete_read_batch` (exit 2); source-change guards stop
remaining as unobserved and outrank the aggregate; broken pipes stop without
a summary. No new Python batch API and no arbitrary limits.

- report now includes sanitized warning diagnostics. Warning-bearing saved
  reports and direct library results retain ordered warning facts in the shared
  closed report/source projection; errors keep their existing meaning. Only
  report preview/ZIP derivatives change. Default path/name/message redaction
  remains in force, with no source recapture or scene repair.

- Add the full in-process Python binding (`pip install oxblend`, Linux x86_64,
  CPython 3.11+ abi3, MIT): every engine command has a typed Python equivalent
  operating on the same engine, with no CLI subprocess and no Blender. Reports
  are immutable `Record` mappings with typed accessors, an exact CLI `.to_json()`
  projection and separate retained `.diagnostics`; partial reports raise their
  catalog error for the kind the engine actually reported (not always an
  `Incomplete*Error`: `list` preserves `missing_asset`/`missing_library`/
  `pattern_members_unknown`/`incomplete_trace`, and `extract` raises
  `ExtractRefusalError`) with the report attached; `allow_incomplete=True` only
  changes delivery and never suppresses `input_changed`. One generated exception class per catalog kind
  (`ERROR_TYPES`: one unique class per catalog code, plus the native
  `closed_file` lifecycle entry; `input_changed` stays `SourceChangedError`, and
  the distinct `source_changed` code is `OxblendSourceChangedError`). Multi-root work
  captures every root in one Session with `open_many(paths)`; separately captured
  arenas are refused with a typed `UsageError`, never merged. Writers remain
  new-path-only with explicit durability and publication facts, and any changed
  source refuses. Ships complete stubs (`mypy --strict`), `py.typed`, a full API
  reference, and a typed example; `docs/USAGE.md`, `docs/STABILITY.md` and the
  bundled skill guide document the surface. The GIL is released around engine
  work; free-threaded CPython is not claimed. Public package/import name is
  `oxblend`; no npm package is introduced.

- TILED Images whose saved filepath is a concrete tile file (for example
  `monster_x_1005.png`, with no `<UDIM>`/`<UVTILE>` token) now expand exactly
  like Blender on load: when the saved `Image.tiles` list is available,
  non-empty and valid, Oxblend auto-tokenizes the concrete basename with
  Blender's own filename-only regexes
  (`BKE_image_ensure_tile_token_filename_only`,
  `source/blender/blenkernel/intern/image.cc:3773`) and expands only the
  saved tile numbers, reporting `is_udim:true`. Blender requires the literal
  token only to save (`image_save.cc:667`); loading keeps the saved tiles, so
  a missing saved tile is now concrete incompleteness (exit 2 with the missing
  tile listed) instead of `pattern_members_unknown`. The directory is never
  globbed to invent members; untokenizable basenames, unavailable/empty/
  invalid tile lists and directory-only tokens stay UNKNOWN with the existing
  reasons. Proven with Blender 5.2.1 save/reopen checks.

- Add experimental `extract`: exact ID-name or physical-block selection, same-schema owned closure, explicit external-reference clear/refuse policy, reported runtime sanitation and structural fixups, and atomic new-output publication. No Python binding in this change. Embedded-ID root promotion remains refused; native proof is version-specific, not a general Blender compatibility certificate.

- Add explicit `exr-pixels FILE --part N --pass NAME --output NEW.f32` with
  pure-Rust `exr` 1.74.2 pixel codecs, exactly-once selected sample coverage,
  source-identity checks and atomic no-replace raw f32le output. Own header and
  batch structure readers remain independent of the pixel codec. No implicit
  beauty/first-pass choice, colour transform or silent integer conversion.

- Text filepath ownership follows saved `TXT_ISMEM`, not dirty state or presence
  of cached lines. Embedded Text historical locations are informational
  `embedded_text_filepath` records, not dependencies or pack blockers. Externally
  backed Texts remain required, even when their cached content is serialized.
  This intentionally removes embedded historical locations from list/trace/pack
  dependency membership (#34). Missing or malformed Text flags leave storage
  UNKNOWN: all path facts/pointer diagnostics survive and strict pack refuses;
  no missing-flags fallback guesses external ownership. A proved empty path has
  no filepath obligation. Native Text flags are documented across2.25–5.2.1 in
  STABILITY; minimal external-Text test fixtures now explicitly declare flags=0,
  preserving their original pointer/provenance assertions.
- Pack streams opaque assets instead of retaining queued/completed asset bodies.
  Capture retains only metadata and an identity digest; copying checks that digest,
  length and ctime-inclusive signatures before no-replace publication. Existing
  blend container proofs and guarded rewrite verification are unchanged (#33).
- Pack accepts `--memory fast|bounded` (default `fast`) on both pack
  commands, and the Python `open`/`File.pack` calls accept the same `memory`
  spelling (`pack` defaults to the Session's opened policy). Bounded retains
  digests and parsed indexes instead of decoded sources, re-reading and
  re-verifying the single working body on reuse; it trades more IO and time for
  lower peak memory on large sequences. Reports record `"memory":"bounded"`
  only; absence of the key means fast. Unknown modes, and a pack mode differing
  from the Session's opened policy, refuse as usage errors naming the cause (#33).

- Trace/list/pack follow a selected local override's original reference ID and
  its dependency graph. Native Blender loads those sources even when the local
  override removes an edge; omitted sources now prevent strict publication.
  Unrelated Library headers remain unselected (#32).
- Reaching a completed linked stub again within one selection pass is no longer
  mistaken for a recursive forwarding cycle. Ancestor-pass forwarding cycles
  still report `cycle` and terminate (independent GLM review F2). Active stubs
  are scoped to the actual library resolution branch, so completed sibling
  resolutions no longer remain active for the entire file pass.
- Forwarding-selector resolution now finishes before local endpoint dependency
  expansion (#35). Mixed-selector batches may reuse an endpoint through a
  completed forwarding alias without a false `cycle`. Actual forwarding-only
  loops still report `cycle`; strict pack refuses them without publication.
  This phase separation can change dependency discovery order and which saved
  owner chain first discovers a shared library, without guessing away unknowns.
- Follow linked ID stubs through their Library pointers, retaining every
  qualified same-name source and reporting absence only at the chain endpoint.
- List now prints its known file set when a linked ID is missing, with separate
  `missing_links`/counts and `[missing linked ID]` text markers, completeness false
  and exit 2. Existing physical libraries are never mislabeled missing.

## Reports versus package production (intentional behavior changes)

- Reports always tell everything; packages refuse unless `--permissive`.
  `list`, `trace`, and `overrides` keep provable results and report missing files,
  missing linked IDs and invalid/uninterpretable pointer subtrees at their sites.
  Unknown means unknown, never a guessed dependency. All such reports are
  `complete:false`, exit 2. Mode flags remain accepted compatibility no-ops.
  Typed site kinds are retained; terminal report-incompleteness uses existing
  `incomplete_trace` / `incomplete_overrides` kinds, and packages use existing
  `incomplete_pack`. Physical-only list absence keeps its existing missing-file
  terminal kind. The 0/2/3/5 application exit dispatch is unchanged. Pack's new
  typed-cause forwarding mirrors trace JSON kinds rather than renaming them.
- **Changed trace default:** unknown dependency edges no longer stop the rest of
  collection; independent known results and unknown-site errors are retained.
  Physical absence now makes trace incomplete too. True selected-root input
  errors still refuse; corrupt dependency libraries become unknown subtrees.
- **Changed pack default:** `pack` and `pack-sequence` now publish nothing on any
  missing or unknown required item. Migration: explicitly add `--permissive`
  (`--non-strict` alias) for a partial package marked INCOMPLETE, exit 2.
  `mode`, `status` and `skipped_items` disclose the production policy and every
  skipped reason; refusal hints name the typed cause and permissive escape.
- Complete listings retain their legacy shape (membership corrections for
  override sources and embedded Text historical paths are described above). Ordinary and
  full-info list both retain every problem; verbosity cannot suppress a problem.
  Reports carry no mode. All flattrace roots record effective `mode`,
  `requested_mode` and default/explicit `mode_source` in existing attributes.
- Explicit exclusions narrow requested scope rather than fail strict delivery.
  `self_contained:false`, `excluded_count` and per-item flag/pattern/missing facts
  disclose that a scoped complete package is not portable. Unreadable libraries
  and unknown pointers remain strict refusals, even with exclusions.
- Native name lookup now prefers local IDs over same-named forwarding stubs.
  Fallback preserves every stub/library identity and reports ambiguity; directly
  reached stubs always follow their actual Library pointers.
- Release version is 1.4.0. Publication and tagging require separate approval.

## Correctness: paths and tracing

- Centralized serialized Blender path interpretation, preserving sequencer `//`
  and matching native I/O for extra slashes and serialized backslash separators.
  Native CLI input filenames keep literal POSIX backslashes.
  Foreign Windows drive/UNC paths carry explicit unverified-I/O warnings;
  unsafe relocation refuses before publication rather than guessing.
- Pointer-array `get` returns all entries. Sequence expansion preserves literal
  glob characters in existing folders. File outputs reject trailing separators
  and retain ordinary input permissions. List/blocks reject nonregular inputs
  before opening them.
- Dependency walks now have per-file summaries and detailed selection, edge and
  asset phases. Pack summaries stay at level 1; member/fetch detail is level 2.
  Fetch causality uses explicit links. Metadata includes version and git SHA.
- Added stable in-process `tracing::{initialize, Span, Outcome}`. Initialization
  is once per process, opt-in and best-effort; cancellation is explicit and
  partial reports remain distinct from successful publication.
- Writer errors skip the failed call instead of discarding the whole run.
  Vendored writer is synchronized with upstream; an offline drift gate compares
  the complete writer body and detects a deliberately mutated copy.
- Qualification explicitly degrades without GNU time, uses current Clippy's
  preferred key sort, and generates deterministic notices independent of cached
  unbuilt target crates. Pack-sequence recipes cover cross-sequence shared
  assets and completeness per selected root.
- Licence qualification handles repeated dependency nodes without shell glob
  expansion; the existing licence allowlist is unchanged.
- Public source and crate exports include the build-identity script and all
  public qualification helpers, plus a versioned changelog.

## EXR facts

- **EXR facts:** decoded per-part windows, compression, channels and standard
  metadata; rational, timecode, keycode, tiledesc and preview dimensions join the
  existing typed attribute reader. Unknown types keep raw hex and `decoded:false`.
- Add `header_end` and `file_size`; keep `extent` as a documented deprecated 1.x
  alias equal to `header_end`. Detect 1×1 data windows without guessing their cause.
- Inspect offset tables and fixed chunk headers for scanline, tiled and multipart
  storage, including all fourteen current compression codes (LJ2K and ZSTD
  included), distinguishing header-only files and truncated chunks. Deep integrity
  is explicitly unsupported. `pixel_integrity_verified` stays false.
- `exr-scan [DIR|FILES...] [--files-from PATH|-] [-0] [--jobs N]` recursively
  streams JSONL facts and a final status/output-tree summary in one parallel
  process. File/stdin lists avoid OS argv limits. No pixel decoding or new
  production dependency. Default parallelism is available cores.
- Buffer header reads; reuse the shared descriptor/path source-signature check.
  Declare scan tracing, with per-file detail spans at level 2.

# Oxblend 1.3.2

- Precisely qualified absent ID references in linked libraries now produce
  per-usage diagnostics rather than refusing the entire dependency result:
  saved legacy realization node groups, copy-transforms targets and active
  Object driver targets. Main-file structure and other invalid pointers remain
  strict. List preserves stdout shapes and emits structured warnings on stderr.
- Pack rechecks and clears these exact copied-library pointer slots to NULL,
  including unselected records, instead of publishing garbage addresses. The
  additive `library_pointer_clears` plan and pack-info disclose every change;
  native Blender reconstructs legacy node groups and reapplies library overrides
  on open, including restored non-NULL targets. No general pointer
  editing API is added. Missing physical files/linked IDs retain their existing
  incompleteness and exit contracts.
- Added synthetic coverage for indirect override-source library declarations
  with distinct paths sharing one basename; the existing listing behavior is
  preserved without widening override-source graph traversal.
- Updated bundled guide and usage/stability contracts for 1.3.2.

# Oxblend 1.3.1

- Missing physical dependencies no longer suppress `list` output.
  Text marks absent declarations `[missing]`; incomplete JSON has `complete:false`
  and owner-grouped `files` with explicit `path`/`missing` member records. Available
  members are retained, and exit 2 plus typed stderr diagnostics remain. Successful
  listings keep their existing JSON shape; missing IDs and hard trace failures
  still refuse. Saved override references retain their physical source-library
  declarations in `list` (fixing an existing-library omission)
  without changing `trace`/`pack` selection or censusing unused Library headers.
- Re-vendored the MIT flattrace Rust writer's `FLATTRACE_LEVEL` support: 0 disables
  trace I/O, unset/empty defaults to 1, digits select a u64, invalid/overflow falls
  back to 1 with a run note; run-start `data.level` records the effective value.
  Oxblend's events are identical at levels 1 and 2 (none are debug-only).
- Updated the bundled guide, help and stability contract for 1.3.1.

# Oxblend 1.3.0

- Local flattrace v1 diagnostics via nonempty `FLATTRACE_DIR`; W3C
  `FLATTRACE_TRACEPARENT` parents the run. The MIT writer is vendored from
  flattrace v1 (MIT), https://github.com/kyraneth/flattrace, the format owner;
  no new dependency or network.
- Command, file open, index/schema, serialization, pack plan/copy/repath, staged
  write and publication spans retain honest outcomes. A kill stays unfinished;
  sink failure warns once and never changes command output or exit status.
- Removed `OXBLEND_PHASE_TIMING`, `OXBLEND_PHASES`, `OXBLEND_READ_REPORT`
  emitters (including the old RX-interface diagnostic). Their timing, byte,
  block and copy facts now live in flattrace data; fallback warnings remain.
- Pack no longer requires procfs: descriptor counting uses `fcntl(F_GETFD)`
  when `/proc/self/fd` is unavailable. Pack I/O errors identify operation/path.
- Bundled guide adds trace/list extraction using `python3`, distinguishes UDIM
  tiles from animation frames, and separates weak import origins and runtime
  brushes from project dependencies.
- `flattrace-events.json` and reference-checker qualification cover every
  command plus read/write/archive/dry-run/error branches. See USAGE.

# Oxblend 1.2.0

- Library-owned `summary::summarize(&BlendFile)` reuses one reader; the CLI only renders it.
- Every saved Scene returns in block/element order, with start/end/step and nested FPS numerator/base/effective plus exact base bits. Existing fields and missing-field warnings remain; numeric schema 1 is extended additively.
- `oxblend skill` / `oxblend skill print` print the version-matched bundled guide offline. Root and command help point there; the first recipe answers range and FPS with one summary call.
- Common CLI refusals provide structured next-command arguments, including byte-exact path evidence, without executing anything.
- **Delivery correction:** broken stdout writes/flushes now exit 2 / `broken_pipe`, not silent success. Capture producer exit before filtering; pack/edit effects are not rolled back.
- Default stderr phase JSON is removed from reflection, trace and overrides. `OXBLEND_PHASE_TIMING=1` explicitly enables interim timing diagnostics until T4 trace mode replaces them. Reader warnings/explicit controls remain.
- No new dependencies, write admission, general query/explain or Python binding are introduced.

# Oxblend 1.1.0

- Partial summaries with typed missing-field warnings and material/mesh/image/collection names.
- Missing dependencies no longer silently succeed in `list`; trace separates traversal completeness from disk availability.
- Add readable UTF-8-or-null name/path aliases, clearer field/file and EXR errors, and explicit ID-name rewrite refusal.
- Byte-identical diffs prove equality; all diff reports include `difference_count`. Returned CLI comparison/read errors now emit an inconclusive v1 report with typed errors, retaining exit 2.
- Incomplete pack-info explicitly says INCOMPLETE and lists every missing physical path, decoded or hex for non-UTF-8 bytes, for single and multiple roots.
- Full argument/option/default help for every command, checked against documented flags.
- Linked-layout source/library facts retain their worded refusal and trace fallback; no partial closure is claimed.
- Reviewed task-oriented skill, relative offline documentation links, and decoded-payload verification instructions for changed/compressed rewrites.
- Updated usage/stability contracts and an absolute README logo URL for package rendering.

No structural editing, datablock rename or dangling-pointer policy change is included.

# Oxblend 1.0.0

The first release reads, inspects, traces, packs and makes guarded edits to serialized Blender files without Blender evaluation.

- Read raw, gzip and zstd containers using each file's own SDNA.
- Inspect typed fields, pointer targets, serialized overrides and EXR header values.
- Trace supported dependency families conservatively, including linked IDs and object data. Report missing libraries and linked IDs as incomplete rather than inventing success.
- Create directory or ZIP packs with explicit missing-file diagnostics, no-replace publication and selectable durability.
- Make expected-old path, fixed-width typed and masked-bit edits to new outputs. No pointer writes or unrestricted setters.
- Use the optional low-memory reader when memory pressure matters more than speed.

The CLI has 18 commands. See [USAGE](USAGE.md), [STABILITY](STABILITY.md) and the [agent guide](../AGENTS.md). Code is MIT; documentation is CC BY 4.0.

## Limits

Only supported serialized structures and dependency families are interpreted. Unsupported or damaged layouts refuse conservatively; completeness is not Blender evaluation or render equivalence. Pre-2.5 files are not supported. Typed edits require a little-endian header version of at least 502; composition field profiles are qualified on native 5.2.1. Generic structural editing of existing graphs is not part of 1.0.

The Linux x86_64 binary dynamically links libc. Other platforms and universal/static Linux compatibility are not claimed. CI is a separate follow-up with exact commit-SHA action pins and a verified run before public integration; local qualification and reproducible builds are available now.
