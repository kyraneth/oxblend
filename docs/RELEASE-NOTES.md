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
