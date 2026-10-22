# Oxblend 1.3.3

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
  declarations in `list` (fixing the Sprite Village existing-library omission)
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
