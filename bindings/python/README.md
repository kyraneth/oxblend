<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/kyraneth/oxblend/main/assets/logo/oxblend-logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/kyraneth/oxblend/main/assets/logo/oxblend-logo-light.svg">
  <img src="https://raw.githubusercontent.com/kyraneth/oxblend/main/assets/logo/oxblend-logo.svg" width="160" alt="Oxblend logo">
</picture>

# Oxblend for Python

Typed, explicit reads and writers for saved Blender `.blend` files, without
Blender and without spawning the `oxblend` CLI. Linux x86_64, CPython 3.11+ (abi3
wheel), MIT.

The binding is a thin adapter over the same Rust engine the CLI uses. Every
command mark has a typed Python equivalent, every result keeps the engine's own
report fields, and every stable error kind is its own exception class. Nothing
here evaluates a scene: these are saved bytes, saved pointer values and saved
control data. Blender open, evaluation and rendering remain separate checks.

```python
import oxblend

with oxblend.open("shot.blend") as blend:
    summary = blend.summary()
    for scene in summary.scenes:
        print(scene.name, scene.frame_start, scene.frame_end, scene.fps.effective)
    print(blend.trace().complete)
```

Run `python -m oxblend skill print` for the bundled version-matched CLI/agent
guide, and `examples/full_api.py` for a typed tour of the whole surface.

## Contents

1. [Install and versions](#install-and-versions)
2. [Lifecycle: captured roots](#lifecycle-captured-roots)
3. [Reports, JSON and diagnostics](#reports-json-and-diagnostics)
4. [Errors](#errors)
5. [Command and option mapping](#command-and-option-mapping)
6. [Report field reference](#report-field-reference)
7. [Writers, durability and no-replace publication](#writers-durability-and-no-replace-publication)
8. [Multi-root selection](#multi-root-selection)
9. [Typing](#typing)
10. [Limits](#limits)
11. [Release and CI status](#release-and-ci-status)

## Install and versions

```sh
python -m pip install oxblend
```

- `oxblend.__version__` is the binding version. It is owned only by
  `bindings/python/Cargo.toml`; crate and wheel ship together as 1.4.1.
- `oxblend.engine_version` is the Rust engine version the wheel was built from.
- The packaged native module's own SHA-256 is hashed at import and reported to
  the engine's identity spans; the hosting Python/Blender executable is *not*
  claimed as the Oxblend binary identity.

The first binding release is **1.4.0**, aligned with the engine crate.
The API documented here is stable from that first release: a
public name, signature, documented report field or stated behaviour is not
removed or repurposed in a later release of the same major version. Breaking one
requires a **major-version bump**, with the crate and wheel sharing the release
version and no `0.x` or development exemption.

## Lifecycle: captured roots

`oxblend.open(path, *, memory="fast")` captures one root. `oxblend.open_many(roots,
*, memory="fast")` captures several roots **together in one Session/InputStore**;
`open` is the one-element case of that same capture. A `File` owns:

- the decoded root bytes, parsed indexes and block inventory,
- retained library captures taken on first use,
- the source signature latches used to detect mutation,
- the retention policy chosen at open time.

Consequences that matter in practice:

- **No implicit reopen.** `File` methods never re-read the path. Repeated
  `get`/`trace`/`pack` calls reuse the captured Session, so one parse is not one
  filesystem read, and a changed file on disk is never silently refreshed.
- **Source change is always a refusal.** Before and after each operation the
  engine checks the source signature; a mismatch raises `SourceChangedError`
  (`kind="input_changed"`). There is no "reload and continue" mode and no
  automatic retry.
- **`root_paths`** exposes the frozen absolute lexical identities captured
  together (one entry for `open`, several for `open_many`). They are identity
  facts, not a way to reopen or to add a root later.
- **`memory`** selects decoded-source retention: `"fast"` (default, unchanged
  behaviour) or `"bounded"` (lower peak memory, more IO and time). The policy is
  fixed for the Session lifetime and reported by `File.memory`. `pack(memory=...)`
  may omit it (keep the Session policy) or repeat it; a different mode refuses as
  a usage error naming both modes.
- **One handle, one mutex.** Calls on the same `File` serialize; the native
  module releases the GIL around long engine work, so other Python threads keep
  running. Reports are plain owned values and callbacks are never invoked while a
  lock is held.
- **Closing** is explicit (`close()`, or the context manager). Reads after close
  raise `ClosedFileError` (`kind="closed_file"`) without reopening.
- **Killed work has unknown effects.** An interrupted writer may have published
  some files. Oxblend never retries or cleans up on your behalf; inspect the
  destination and the retained report.

A retained `File` costs memory even before a report is built: it owns the full
captured source arena for subsequent calls. The CLI's ephemeral `inspect` path
indexes the input without retaining that arena. In fresh-process measurements
on the public Scrapyard input (760,588,050 physical bytes), CPython 3.11
`open(..., memory="bounded")` alone peaked at 783,668 KiB, versus 297,576 KiB for
CLI `inspect` (2.63×); closing the reader returned its RSS to 27,784 KiB. Report
objects and explicit `.to_json()` projections allocate additional owned facts.
`"bounded"` is a retention policy, not a promise of CLI-sized RSS or a fixed
memory ceiling. This captured-source cost is intentionally retained.

Host paths may be `str`, `bytes`, `os.PathLike[str]` or `os.PathLike[bytes]`.
Unix path bytes are preserved (reports always carry the authoritative `*_hex`
fields), so a non-UTF-8 path is never silently transcoded.

## Reports, JSON and diagnostics

Every report is an immutable `oxblend.Record`, a `Mapping[str, Any]`:

- **Typed accessors** name the documented fields (`report.complete`,
  `report.files`, `report.errors`, …). They read the engine's own keys.
- **Every other field stays reachable** by key: `report["missing"]`,
  `report["root_traces"]`. Additive engine fields need no stub change and no
  re-release to be readable.
- **`.to_json()`** returns an owned, JSON-compatible `dict`/`list`/scalar that is
  exactly the CLI's JSON projection for that command. It is not encoded text and
  never includes binding-only fields.
- **`.diagnostics`** is the binding's operation diagnostics tuple. It is preserved
  on the report, and is removed only from `.to_json()` so your JSON still matches
  the CLI byte-for-byte in shape. Diagnostics are never discarded.
- **`.raw(field)` / `.text(field)`** decode the engine's lossless `FIELD_hex`
  provenance. Prefer `raw()` when a value may not be UTF-8; `text()` raises the
  engine `encoding` error instead of substituting replacement characters.
- Reports are immutable and cheap to pass between threads, but they are **not**
  durable identifiers: block indices, element indices and addresses belong to
  that exact save. Report subclasses declare `__slots__ = ()`, and the two
  non-`Record` reports, `TextReport` and `ScanReport`, are frozen dataclasses.

`allow_incomplete=False` (the default on every operation that can be partial)
raises the catalog error for the kind the engine actually reported — which is
not always an `Incomplete*Error`. `list_dependencies` preserves `missing_asset`,
`missing_library`, `pattern_members_unknown` or `incomplete_trace`, and
`extract` raises `ExtractRefusalError` (`extract_refusal`); `pack`, `query`,
`overrides`, `facts`, `trace`, `diff`, `edit` and `exr_scan` use their own
`Incomplete*Error`. The full report is retained as `error.report` and the
operation diagnostics as `error.diagnostics`. Passing `allow_incomplete=True`
returns that report instead of raising for incomplete coverage; it never
suppresses `input_changed`/`SourceChangedError`. Delivery changes only: engine
strictness, publication and completeness fields are unchanged, so an incomplete
report is still an incomplete report.

A few reports have no CLI JSON schema to equal:

- `block_report(path)` returns `TextReport`; `.to_json()` is the exact CLI text.
- `exr_scan(...)` returns a frozen `ScanReport`; `.records` is the whole CLI
  JSONL sequence (file records in arrival order, summary record last) and
  `.to_json()` is that sequence. The native adapter returns this one sequence
  for success and for failure alike, so nothing is reordered or reconstructed.

## Errors

`oxblend` exposes one exception class per stable kind in `docs/error-catalog.json`,
generated deterministically (`snake_case` → `PascalCase` + `Error`).

- `ERROR_TYPES` maps the exact engine `kind` string to its class. Every one of
  the 164 catalog codes maps to exactly one unique class, and no two codes share
  a class, so code↔class is a bijection over the catalog. The map additionally
  carries the native `closed_file` lifecycle kind (`ClosedFileError`), which is a
  runtime state rather than a catalog entry. `input_changed` maps to the
  pre-existing `SourceChangedError`; `source_changed` is reserved away from that
  base name and maps to `OxblendSourceChangedError` — two distinct codes, two
  distinct classes, no shadowing.
- Names that would collide with a Python builtin are prefixed `Oxblend`
  (`OxblendTypeError`, `OxblendOverflowError`, `OxblendBrokenPipeError`).
- An unknown future kind still raises `OxblendError`; the mapping never guesses an
  alias. Match on `kind` (or the class) — never on message text.

Every raised error carries:

| Attribute | Meaning |
|---|---|
| `kind` | exact catalog kind string |
| `message` | engine message (never parse it) |
| `operation` | operation that failed (`pack`, `trace`, `edit`, …) |
| `exit_code` | CLI-equivalent exit: `3` for `pack_input`/`missing_input`, `5` for `pack_project`, otherwise `2` |
| `report` | typed partial report when the engine produced one, else `None` |
| `diagnostics` | tuple of operation diagnostics (may be empty) |
| `diagnostic_ref` | `None` for native library calls today; the invocation ledger is CLI-only (never fabricated) |
| `source_snapshot` | `SourceDiagnostic \| None`: owned local source evidence, carried through Error clones; raw paths/messages are private |
| `effects` | `"none"`, `"reported"` (report carries effect facts) or `"unknown"` |
| `explanation`, `likely_causes`, `next_steps`, `known_issue` | catalog guidance text |
| `next_command` | immutable mapping with exact `argv` and native-byte `argv_hex`, or `None`; generated by the same engine helper as the CLI |
| `context` | `(key, value)` pairs, e.g. the failing query index |

`effects="unknown"` means the outcome of an interrupted or hard failure cannot be
proved; inspect the destination before retrying. A writer failure is never
retried automatically.

Typed usage refusals come from the same catalog. In particular, combining
separately captured roots raises `UsageError` and names the remedy:

```text
Separately opened Files cannot be combined; use open_many(paths)
```

Argument type/range errors are Python-native exceptions, outside the canonical
contract. An out-of-range or wrong-typed argument rejected by PyO3 BEFORE the
engine (for example `block_report(address=-1)`, `address=2**64`, or a
non-integer address) raises native `TypeError`/`OverflowError`, idiomatic
Python, without emulating CLI usage prose. Values that pass conversion but
fail engine validation still raise the canonical engine error (for example
`address=0` on a file without that address raises `MissingAddressError` with
the exact CLI kind/message). Coded-typed semantic validation in Python
(`FieldQuery`, path steps, `_items`) raises its own typed errors before the
engine and never becomes a native `TypeError` merely because both share
`ValueError`.

## Invocation identity and owned source evidence

Native library calls have no CLI failure ledger: `diagnostic_ref` is **null** today.
The CLI may produce null or exactly `{run_id, failure_id}`. Qualification checks
those exact shapes instead of silently dropping the field, then compares every
other report field exactly. No ledger ID or receipt is synthesized in Python.

`error.source_snapshot` retains instrumented source evidence even without a
ledger. It retains the first producer kind/message; a later reclassification
appears in `propagations` rather than overwriting that source evidence.
`SourceDiagnostic` exposes `kind`, `message`, `check_id`,
`context_origin`, `context: SourceContext`, `caused_by: tuple[str, ...]` and
`propagations: tuple[SourcePropagation, ...]`. Propagations expose `kind`,
`check_id` and `context`. Missing instrumentation is `None`, never a guessed site.

`SourceContext` exposes optional integer fields `block_index`, `sdna_index`,
`count`, `payload_size`, `data_offset`, `element`, `offset_in_block`,
`decoded_file_offset`, `width` and `blend_version`; optional `little_endian: bool`;
optional hex strings `file_hex`, `block_code_hex`; optional decimal strings
`old_address`, `pointer_decimal`; `field_path_hex: tuple[str, ...]`; and
`namespace`, `resolution` strings. All objects are immutable owned mappings.
**Raw snapshots contain private local paths/messages.** Use `report(error, output)`
(or a warning-bearing result) for the same closed privacy projection as the CLI.
Direct reports honestly state library-unbound evidence, not verified CLI invocation
binding. Structural diagnostics match; binding/provenance differences are retained
and checked as expected differences. A CLI ledger can additionally contain a
terminal aggregate and invocation IDs; the library never manufactures those.
An aggregate error's carried partial `.report` also contributes its existing
producer fields, with `context_origin="producer_report"`; no source is reopened
to fill missing facts. Warning-bearing results and saved reports retain ordered
sanitized `warnings` arrays in the report/source derivatives, separately from
`failures`; a real three-warning trace produces three warning records. The same
closed sanitizer preserves explicit coordinates but never recaptures source or
forwards raw warning messages/paths by default. Empty warnings preserve the
no-warning derivative shape. Report creation remains preview → review →
`report_create(preview, sha256, reviewed_output)`, with no upload or replay.
The output parent and basename must match the reviewed preview; create cannot
redirect it to an arbitrary new destination.

## Command and option mapping

Every native engine command has a Python entry point. `skill`, `help`,
`--help`/`-h` and `--version`/`-V` are CLI presentation, not engine operations:
they have no native parity target. The offline guide is still available inside
Python as `python -m oxblend skill print`.

Options that Python replaces with first-class values keep their exact CLI
meaning:

- `--files-from PATH` / `-0` → pass an explicit iterable, or decode a path list
  with `read_path_list(path, nul=False)`.
- `--strict` / `--permissive` / `--non-strict` on trace/list/overrides are legacy
  compatibility switches and are no-ops; use `allow_incomplete` for delivery and
  `pack(permissive=...)` for real pack strictness.
- `--sha256` / `--timing` are CLI text-only; they have no report field and are
  irrelevant to the JSON projection.
- Argument type/range errors are Python-native exceptions, outside the canonical
  contract: an out-of-range or wrong-typed argument rejected by PyO3 before the
  engine raises `TypeError`/`OverflowError`, not a `UsageError` emulating CLI
  prose. In-range values that fail engine validation still raise the canonical
  engine error.

| Error boundary | CLI / Python parity |
|---|---|
| Engine semantic validation | Full canonical error object is identical; Session-only read context is not substituted for the command's message. |
| Pre-engine Python argument type/range conversion | Python-native `TypeError` / `OverflowError`; not a CLI prose emulation. |
| `spill_space` | **canonical-identical, dynamic fields accounted**: only the shared-format available/shortfall integers may differ. Each process's available bytes must fall within its own immediately bracketing `statvfs` observations; required bytes are identical and shortfall equals required minus available. Every other byte and field is exact. |

| CLI command and options | Python entry points | Result |
|---|---|---|
| `summary FILE` | `File.summary()`, `summary(path)` | `Summary`; `.to_json()` exact CLI |
| `query FILE --type TYPE --fields F,... [--json]` | `File.query(type=..., fields=..., allow_incomplete=False)`, `query(path, ...)` | `QueryReport` |
| `catalog [--json]` | `catalog()` | `CatalogReport` |
| `explain CONCEPT [--file FILE] [--json]` | `explain(concept, file=...)`, `File.explain(concept)` | `ExplainReport` |
| `inspect FILE` | `File.inspect()`, `inspect(path)` | `InspectReport` |
| `get FILE BLOCK PATH_JSON [ELEMENT] [--raw \| --byte-array N \| --pointer-table N]` | `File.get([FieldQuery, ...])`, `get(path, block, steps, element=0, raw=..., byte_array=..., pointer_table=...)` | tuple of `FieldValue`, or `ValueReport`; the three array modes are mutually exclusive |
| `pointer FILE BLOCK PATH_JSON [ELEMENT]` | `File.pointer(FieldQuery)`, `pointer(path, block, steps, element=0)` | `BlockInfo \| None`, or `PointerReport` |
| `payload FILE BLOCK` | `File.payload(block)`, `payload(path, block)` | `ValueReport` (`value.hex` is the exact payload) |
| `read-batch FILE --requests SPEC` | Compose the existing `inspect`/`get`/`pointer`/`payload` methods on one retained `File`; no additional Python batch API | CLI-only ordered JSONL transport; primitive projections use the same engine |
| `source-facts FILE` | `File.source_facts()`, `source_facts(path)` | `FactsReport` |
| `library-facts FILE` | `File.library_facts()`, `library_facts(path)` | `FactsReport` |
| `trace FILE... [--summary] [--per-file\|--recursive] [--low-memory] [--strict\|--permissive\|--non-strict]` | `trace(roots, recursive=True, low_memory=False, allow_incomplete=False)`, `File.trace(...)` | `TraceReport` for one root, `MultiRootReport` for several |
| `list FILE... [--json] [--full-info] [--low-memory] [--sha256] [--timing]` | `list_dependencies(roots, full_info=False, low_memory=False, allow_incomplete=False)`, `File.list_dependencies(...)` | `ListReport` for one root, `MultiRootReport` for several |
| `blocks FILE [--limit N] [--address ADDRESS] [--dump]` | `block_report(path, limit=10, address=None, dump=False)`; `File.blocks(code=...)` is the typed inventory | `TextReport` / `tuple[BlockInfo, ...]` |
| `overrides FILE [--root-only\|--recursive] [--json]` | `overrides(path, recursive=True, allow_incomplete=False)` | `OverrideReport` |
| `pointer-census FILE [--full]` | `File.pointer_census(full=False)`, `pointer_census(path, full=False)` | `CensusReport` |
| `diff A B` | `diff(a, b, allow_incomplete=False)` | `DiffReport` |
| `edit INPUT --output NEW --spec JSON --mode format-only\|comp\|node-tree [--durability ...]` | `edit(input, output, spec, mode=..., durability="none", allow_incomplete=False)` | `EditReport` |
| `node-tree FILE` | `node_tree(path)` | `NodeTreeReport` |
| `rewrite INPUT --output NEW --edits SPEC [--durability ...]` | `rewrite(input, output, edits, durability="none", allow_incomplete=False)` | `EditReport` |
| `extract SRC (--id NAME \| --block INDEX)... [--clear STRUCT.FIELD...] --output NEW [--durability ...]` | `extract(input, output, ids=(), blocks=(), clear=(), durability="none", allow_incomplete=False)` | `ExtractReport` |
| `pack INPUT NEW [-p PROJECT] [-e GLOB]... [-n] [-r] [-c] [--keep-hierarchy] [--strict\|--permissive] [--durability ...] [--memory fast\|bounded] [--]` | `File.pack(destination, project_root=None, exclude=(), dry_run=False, relative_only=False, compress=False, keep_hierarchy=False, permissive=False, durability="none", memory=None, allow_incomplete=False)`, `pack(input, output, ...)` | `PackReport` |
| `pack-sequence INPUT... -t NEW_TARGET [same pack options]` | `File.pack_sequence(output, ..., keep_hierarchy=True)`, `pack_sequence(roots, output, ...)` | `PackReport` plus multi-root fields |
| `exr-header FILE` | `exr_header(path)` | `ExrHeaderReport` |
| `exr-scan [DIR\|FILES...] [--files-from PATH\|-] [-0] [--jobs N]` | `exr_scan(inputs, jobs=None, allow_incomplete=False)`; `inputs` is one `HostPath` or an iterable of them | frozen `ScanReport` (`None` jobs = available cores; no invented limit) |
| `exr-pixels FILE --part N --pass NAME --output NEW.f32` | `exr_pixels(input, output, part=..., pass_name=...)` | `PixelsReport` |
| `report --preview` (with `--report`/`--stdout`/`--trace`/`--trace-dir`/`--run-id`/`--last`/`--include-names`/`--include-file`) | `report_preview(output, report=None, stdout=None, trace=None, trace_dir=None, run_id=None, last=False, include_names=False, include_file=None)` | `ReportPreview` |
| Direct typed library evidence through the same preview engine (no fabricated CLI invocation) | `report(source, output, include_names=False, include_file=None)`; `source` is a saved path, `OxblendError`, `Record`, `Summary` or `ScanReport` | `ReportPreview`, never automatic publication |
| `report --create PREVIEW --preview-sha256 SHA --output NEW` | `report_create(preview, preview_sha256, output)` | `ReportCreated` |
| `report --discard-preview PREVIEW --preview-sha256 SHA` | `report_discard(preview, preview_sha256)` | `ReportDiscarded` |
| `skill`/`help`/`--version` | CLI presentation; `python -m oxblend skill print` inside Python | not a native parity target |

The `report` family invokes the already-public engine report API in-process; it
never shells out, never uploads, never files an issue, and never re-runs the
failed command. `include_names`/`include_file` are explicit disclosure opt-ins.

## Report field reference

Field names below are the engine's. Individual reports may add fields in a minor
release; `Record` preserves them, `.to_json()` shows them, and nothing here
should be read as an exhaustive frozen schema. `schema` values are quoted
exactly as the engine writes them.

### Summary

`Summary` (typed) and `summary(...).to_json()`:

- `schema` = `1`, `header` (`version`, `pointer_size`, `little_endian`,
  `file_format`), `block_codes` (pairs of code bytes and count),
  `objects`/`materials`/`meshes`/`images`/`collections` (name arrays),
  `scenes`, `warnings`.
- Each scene: `name`, `frame_start`, `frame_end`, `frame_step`, `camera`, `fps`
  with `numerator`, `base` (saved float bits) and `effective`.
- Each warning: `kind`, `block_index`, `element`, `field`, `message`.
- A non-UTF-8 saved name still produces the typed `Summary`; `.to_json()` raises
  the engine `encoding` error, exactly as the CLI refuses to render it.
- One `summarize_typed` engine pass produces both the typed fields and the
  stored projection, so `.to_json()` never re-reads the file; the encoding error
  is deferred to the moment you ask for JSON.

### Query, catalog and explain

- `QueryReport` / `.to_json()`: `schema` = `"oxblend.query/1"`, `catalog_revision`,
  `basis`, `graphs`, `linked_id_observations`, `linked_subtype_observations`,
  `linked_subtype_diagnostics`, `storage_evidence`, `source_observation`,
  `selection`, `requested_coverage` (`{"complete": bool}`), `errors`, `rows`.
  `QueryReport.complete` reads `requested_coverage.complete`.
- `CatalogReport` / `.to_json()`: `schema` = `"oxblend.catalog/1"`,
  `catalog_revision`, `basis`, `concepts`, `read_operations`, `raw_projection`,
  `limits`.
- `ExplainReport` / `.to_json()`: `schema` = `"oxblend.explain/1"`,
  `catalog_revision`, `concept`, `entry`, `file_aware`, `sdna`, `profile`,
  `next_action`.

### Inspect, get, pointer and payload

- `InspectReport`: `schema` = `1`, `header`, `blocks`, `sdna`.
  - block: `code_hex` (all four code bytes including NUL padding),
    `address_decimal` (string), `sdna_index`, `count`, `size`, `data_offset`.
  - `sdna`: `names_hex`, `types_hex`, `lengths`, `structs`; each struct has
    `type_index` and `fields` (`name_hex`, `full_name_hex`, `type_index`,
    `offset`, `size`, `array_len`, `element_size`, `pointer`).
- `FieldValue`: `kind` and the typed `value` (`int`, `bytes`, `Float32Bits`, or a
  tuple of `FieldValue` for arrays); `to_json()` wraps the engine projection as
  `{"schema": 1, "value": ...}`. `FieldValue.text` is available only for bytes.
- `get(...)`/`payload(...)` → `ValueReport`: `schema` = `1`, `value`.
  Byte-array reads return the byte value shape; pointer-table reads return
  `{"kind": "pointer_table", "block_indices": [INDEX|null, ...]}` with reader-local
  indices and nulls preserved.
- `pointer(...)` → `PointerReport`: `schema` = `1`, `block_index` (int or null) in
  this reader's namespace. It never follows a pointer implicitly.

### Facts

`FactsReport` / `.to_json()` (source or library facts):

- `schema` = `1`, `query` (`"source-facts"` or `"library-facts"`), `semantic`
  (`"oxblend-source-1"` / `"oxblend-library-1"`), `reader`, `input`, `stats`,
  `facts`.
- `facts` contains `blocks` and `complete`; an unknown pointer edge is a
  `{"status": "unknown", "error": …}` site, not `null`.
- When the facts are incomplete the report additionally carries `complete:false`
  and an `errors` array; failed edges stay unknown at their own site and
  independent facts are retained.

### Trace, list and multi-root

- `TraceReport` / `.to_json()`: `schema` = `1`, `complete`, `usages`, `errors`,
  `warnings`, plus `problem_counts` when failures were counted.
  - Usage: `owner_file`, `owner_file_hex`, `library_chain_hex`,
    `block_code_hex`, `block_address` (string), `block_offset`, `block_type`,
    `block_name_hex`, `asset_path_hex`, `is_sequence`, `is_udim`, `optional`,
    `path_full_field` / `path_dir_field` / `path_base_field`,
    `field_provenance`, `resolved_path_hex`, `exists`, plus
    `expected_members*`/`missing_members*` when a pattern applies.
  - Failure: `kind`, `owner_file_hex`, `block_address`, `field_path_hex`,
    `library_path_hex`, `message`, `diagnostic_ref`, `status`, `site`, and, for
    missing IDs, the root/identity fields.
- `ListReport` (single root) / `.to_json()`:
  - default: owner-path → member array (strings, or
    `{"path": ..., "missing": bool}` objects when the report is incomplete).
  - `full_info=True`: `files`, `usages`, `packed`, `warnings`, plus `errors`,
    `problem_counts` and `missing_links`/`missing_link_counts` when present, and
    `complete:false` when the listing is incomplete.
  - `--sha256`/`--timing` have no JSON report meaning.
- Multi-root (`trace`/`list_dependencies`/`pack_sequence` over several roots):
  delivered as `MultiRootReport`, `schema` = `"oxblend.multi-root/v1"`,
  `operation`, `complete`, `root_traces`, `cross_reference`; `list`
  additionally carries `root_lists` (its accessor returns `()` when absent).
  - `root_traces`/`root_lists` rows: `root_id`, `input_path_hex`, `input_path`,
    `traversal_complete`, `complete`, `shared`, `unique`, `missing_files`,
    `missing_links`, `error` and, when observed, `observation_errors`.
  - `cross_reference`: `schema` = `"oxblend.cross-reference/v1"`, `complete`,
    `assets`, `roots`, `shared`, `totals`. Each asset: `asset_id`,
    `identity_path_hex`, `identity_path`, `resolved_paths*`, `unexpanded`,
    `exists`, `bytes`, `shared`, `used_by`.

### Overrides, diff, edits and extraction

- `OverrideReport` / `.to_json()`: `schema` = `1`,
  `semantic` = `"serialized-overrides-1"`, `records`, `errors`, `warnings`,
  `complete`. Records keep `id`, `library_chain_hex`, `definition`, `reference`,
  `hierarchy_root`, `flags`, `is_system_override`, `properties`,
  `reference_chain`. Serialized facts only — no evaluation, no guessed targets.
- `DiffReport` / `.to_json()`: `schema` = `"oxblend.diff/v1"`, `read_only`,
  `left`, `right`, `matching`, `matched_allocations`, `node_identifier_bijection`,
  `differences`, `difference_count`, `complete`, `unresolved`,
  `explicit_ignored_fields`, `equal`, `opaque_contract`, `native_contract`, and
  `errors` on a returned-error projection. `equal=None` means inconclusive, not
  different. A comparison/read error raises with that same partial report on
  `error.report`; `allow_incomplete=True` returns it.
- `edit(...)` (`format-only`/`comp`) → `EditReport` / `.to_json()`: `schema` = `2`,
  `operation` = `"typed_edit"`, `mode`, `profile_build`, `complete`,
  `output_written`, `layout_verified`, `container_verified`,
  `declared_diff_verified`, `native_verified`, `semantic_verified`,
  `structural_verified`, `input_path_hex`, `output_path_hex`,
  `input_decoded_length`, `output_decoded_length`, `output_physical_length`,
  `container_family`, `container_reencoded`, `edits`, `durability`,
  `durability_completed`, `errors`.
- `node-tree` editing returns the schema-1 structural report (`operation` =
  `"node_tree_edit"`, before/after graphs, changed/added blocks, retention policy,
  verification flags, `durability*`).
- `rewrite(...)` → `EditReport` with the schema-1 `rewrite_paths` report
  (`complete`, per-edit ranges/errors, publication and durability fields).
- `ExtractReport` / `.to_json()`: `schema` = `1`, `requested_clear_rules`,
  `unused_clear_rules`, `complete`, `selected_ids` (`block`, `name`,
  `name_hex`), `selected_blocks`, `source_blocks`, `applied_policies`,
  `runtime_sanitation`, `structural_fixups`, `refusals`, `output_written`,
  `durability_requested`, `durability_completed`, `errors`. A refusal is retried
  by nothing: with `allow_incomplete=False` the non-empty `refusals` raise
  `ExtractRefusalError` carrying this report.
- `NodeTreeReport` / `.to_json()`: `schema` = `1`,
  `operation` = `"node_tree_inspect"`, `profile_build`, `trees`, `complete`,
  `native_verified` (`false`: structural guards only).

### Structural editing qualification

Structural inspection/editing is **currently qualified on Blender 5.2.1 only**,
build `9e2066aef7ef`, subversion **44**, 64-bit little-endian format 1 with the
pinned DNA. **Support for every 5.2.x and 5.3.x build is planned** separately;
this is a temporary qualification limit, not the intended support boundary. A saved version of 502 alone is currently insufficient. Both
real public 5.2 examples presently refuse with `NodeVersionError`
(`kind="node_version"`, code 2): the splash saves subversion 44/build
`b2d38940ccd4`, and the monk example saves subversion 45/build `d13f752e3b9c`.
Both saved build identities differ from the currently qualified build; the
splash does **not** have a different subversion. These refusals remain exact
CLI/Python parity checks until the engine is extended.
Typed `format-only`/`comp` editing has its own guards.

### Guarded IDProperty strings

`edit(input, output, spec, mode="format-only")` (or `File.edit(...)`) accepts
schema-2 `idprop_strings` through the same `Mapping` spec as the CLI. Use
`mode="comp"` to include it in the compositor-oriented typed-edit profile. There is no separate
setter or pointer-write API. For example, after deriving `property_block` and
`property_element` from this exact saved input:

```python
spec = {"schema": 2, "writes": [], "bits": [], "idprop_strings": [{
    "operation": "idprop-string", "block_index": property_block,
    "element": property_element, "path": ["data", "pointer"],
    "name_hex": b"label".hex(), "expected_hex": b"old".hex(),
    "replacement_hex": b"new".hex(),
}]}
result = oxblend.edit(source, new_output, spec, mode="format-only")
```

The engine proves the saved UTF-8 property name/value, string type, 502/503
little-endian SDNA layout, existing allocation capacity (including NUL),
no-alias ownership and unchanged input. It updates payload and saved length
atomically without changing the pointer, `totallen` or allocation. Guard refusals
raise `IdpropStringError` or the underlying catalog error, with the exact CLI
canonical object. Structural node-profile gates are unchanged; serialized success
is not proof of native Blender evaluation. See the engine's
[IDProperty contract](../../docs/USAGE.md#guarded-idproperty-string-values).

The additional clean-room native fixture qualifies real typed and structural
round trips through Python, CLI readback and headless Blender. Run the full
writer qualification with `full_parity.py --stage write --blender /path/to/blender-5.2.1`
(and the required CLI, corpus and evidence arguments). Engine report flags
`native_verified:false` remain truthful: a separate receiving test does not
make every API edit natively verified.

### Pack

`PackReport` / `.to_json()`:

- `schema` = `1`, `operation` = `"pack"`, `input_path_hex`,
  `legacy_input_trace_scope`, `input_paths_hex`, `root_traces`,
  `keep_hierarchy`, `directory_path_hex`, `planning_complete`,
  `directory_created`, `noop`, `compression_requested`, `archive`,
  `archive_written`, `archive_entry_paths_not_filesystem_paths`, `complete`,
  `durability`, `durability_mechanism`, `syncfs_shared_filesystem`,
  `durability_completed`, `trace`, `files`, `rewrites`,
  `library_pointer_clears`, `missing_links`, `missing_link_counts`,
  `skipped_absolute_paths`, `missing`, `warnings`, `pack_info`, `errors`.
- Each packed file: `input_path_hex`, `output_path_hex`, `output_written`,
  `durability_completed`, `rewritten`.
- `"memory":"bounded"` appears only when bounded retention was used; absence means
  fast and keeps default report bytes.
- Partial publication is *reported*, not hidden: a report can have
  `complete:false` with `archive_written:true` and per-file effect facts.
- `pack-sequence` returns the same report shape plus the multi-root fields.

### EXR

- `ExrHeaderReport` / `.to_json()`: `version`, `extent`, `header_end`,
  `file_size`, `header_complete`, `pixel_integrity_verified` (always `false` —
  header parsing does not decode pixels), `tiled`, `multipart`, `deep`,
  `placeholder_1x1`, `parts`, then the engine's structural offset/chunk keys.
  Each part: `tiled`, `multipart`, `deep`, `placeholder_1x1`, `channels`,
  `attributes`, and decoded `dataWindow`, `displayWindow`, `compression`,
  `lineOrder`, `pixelAspectRatio`, `screenWindowCenter`, `screenWindowWidth`.
  Channels keep `name`/`name_hex`, `pixel_type`, `pixel_type_name`, `linear`,
  `pLinear`, `reserved`, `sampling`, `xSampling`, `ySampling`. Attributes keep
  `name`/`name_hex`, `type`/`type_hex`, `offset`, `length`, `decoded`,
  `decoded_value`.
- `ScanReport.records` (the sequence, summary included): one
  `record_type:"file"` record per input
  (`status`, `file_size`, `header_end`, `extent`, `header_complete`, and the
  header facts for readable files) followed by a `record_type:"summary"` record
  with `schema` = `1`, `files`, `counts_by_status`, `placeholders`, `truncated`,
  `zero_byte`, `non_exr`, `complete`, `jobs`, `relative_root*`, `groups`.
  `.to_json()` returns the JSONL sequence, `.summary` is `records[-1]`, and
  `ScanReport.complete` reads that summary record's `complete`; with
  `allow_incomplete=False` an unfinished scan raises `IncompleteExrScanError`
  carrying the same `ScanReport`.
- `PixelsReport` / `.to_json()`: `schema` = `1`, `complete`, `decode_complete`,
  `codec`, `part`, `pass`, `channels`, `width`, `height`, `data_window`,
  `samples`, `format` (`"f32le-interleaved"`), `order`, `output`, `output_written`,
  `bytes`, `durability`. Output is raw f32le with no colour transform.

### Pointer census and report family

- `CensusReport` / `.to_json()`: `schema` = `"oxblend.pointer-census/v2"`,
  `whole_file`, `blocks`, `pointer_sites`, `states`, `opaque_allocations`,
  `semantic_verified` (always `false`). `full=True` includes all sites.
- `ReportPreview`: `schema` = `"oxblend.report-preview/v1"`, `bundle_id`,
  `output_basename`, `privacy`, `warnings`, `members`, plus the frozen receipt
  (`receipt_path`, `receipt_path_hex`, `receipt_sha256`). Pass that
  `receipt_sha256` to `report_create` only for a preview you just produced.
- `ReportCreated`: `schema` = `"oxblend.report-created/v1"`, `members`,
  `output_written`, `durability_completed`, `archive_sha256`, `output_path*`,
  `snapshot_consumed`, `preview_authenticity`, `issue_url`, `privacy`,
  `warnings`, `filing` (`"manual; nothing uploaded"`).
- `ReportDiscarded`: `{"discarded": true}`.

## Writers, durability and no-replace publication

All writers (`edit`, `rewrite`, `extract`, `pack`, `pack_sequence`, `exr_pixels`,
`report_preview`/`report_create`) publish to a **new** path:

- Existing destinations and archives refuse (`output_exists`); nothing is
  overwritten or merged. Atomic publication is the engine boundary.
- `durability="none" | "end" | "each-file"` means exactly the CLI meanings.
  `durability_completed` reports what actually happened; `syncfs_shared_filesystem`
  and `durability_mechanism` describe the mechanism used.
- Strict pack (default) publishes nothing when a requirement is missing or
  unknown. `permissive=True` may publish the available members and the report is
  labelled incomplete. `dry_run=True` is `--noop`: planning only.
- `exclude` patterns, `relative_only`, `compress` and `keep_hierarchy` keep their
  CLI meanings. `project_root=None` defaults to the input's parent (the CLI
  default) rather than inventing a project.
- Pack never retries, never deletes and never cleans up. An interrupted pack can
  leave a partial destination that the report describes; verify it yourself.
- A changed input always refuses, including between planning and publication.
  There is no "write anyway" mode.

## Multi-root selection

`roots` is an explicit iterable of host paths, a single host path, or **exactly
one** `File` created by `open_many` — including a one-element list `[file]`:

- Paths are captured together in one Session/InputStore; shared roots and
  libraries deduplicate inside that store.
- A single `File` (multi-root or not) is accepted by `File.trace`,
  `File.list_dependencies` and `File.pack_sequence`, and by the path-based
  functions when you pass that `File` or `[file]`. Its captured root set is used
  as-is.
- A list containing more than one `File`, or a `File` mixed with paths, refuses
  with `UsageError` naming `open_many(paths)`.
- Separately captured arenas are never merged, and a `File` is never reopened to
  satisfy a multi-root call.
- The CLI's `--files-from PATH` / `-0` are replaced by explicit iterables;
  `read_path_list(path, nul=False)` decodes a path list when you actually have
  one. Stdin is never read implicitly.

## Typing

The wheel ships `py.typed`, complete stubs (`__init__.pyi`, `operations.pyi`,
`_native.pyi`) and generated error classes. `mypy --strict` passes on the stubs
and on `examples/full_api.py`.

- Documented fields are typed properties; additional engine fields are typed
  `Mapping[str, Any]` lookups. There is no `__getattr__` catch-all, so a typo in
  an accessor is a type error rather than a silent `Any`.
- `HostPath` accepts `str | bytes | os.PathLike[str] | os.PathLike[bytes]`.
- `Memory` is `Literal["fast", "bounded"]`; `Durability` is
  `Literal["none", "end", "each-file"]`; `JSONValue` is the recursive JSON type.
- Reports are not generic: `PackReport.files` is `tuple[Record, ...]`, not
  `Any`. `trace`/`list_dependencies` are typed unions (`TraceReport |
  MultiRootReport`, `ListReport | MultiRootReport`); `isinstance(report,
  oxblend.MultiRootReport)` narrows to the per-root form.

## Limits

- Saved serialized state only. There is no evaluated-scene, dependency-graph
  evaluation, render or frame verification, and no Blender process.
- Pointer reads are explicit and checked; Oxblend never invents a target, never
  nulls an unknown pointer and never expands arbitrary pointer graphs.
- Linux x86_64 and CPython 3.11+ (abi3). Other platforms are not claimed.
- The GIL is released around long engine work. A free-threaded CPython build is
  **not** qualified by this document; qualification is claimed only if it was
  actually run for the release.
- Writers prove the serialized contract they declare. They do not prove that
  Blender can open, evaluate or render the result; verify writer output in a
  compatible Blender before delivery.
- Reports preserve addresses, topology and paths. Review a report before
  sharing it.

## Release and CI status

Building and publishing a wheel/sdist is an explicitly approved release step, not
something this package does at runtime. The lane's release workflow is
`workflow_dispatch`-only, builds the sdist first and the manylinux x86_64
`abi3-py311` wheel from that sdist, verifies hashes, and publishes through PyPI
trusted publishing in a protected environment. No token or password is stored.
Nothing publishes by default, and every publication needs its own approval.

For the authoritative CLI contracts behind every report, see
[`docs/USAGE.md`](../../docs/USAGE.md) and
[`docs/STABILITY.md`](../../docs/STABILITY.md).


<!-- generated-guidance -->
**Accounted non-regular-source difference (Release A).** FIFOs and device files are outside the Python captured-`File` admission contract. On Linux, `oxblend summary /dev/null` refuses with `truncated` / `range 0+7 exceeds 0 bytes`, while `oxblend.open("/dev/null")` (and Python `summary`) refuses before opening the device with `pack_path` / `pack source is not regular file`. No FIFO/device special case, capture-policy widening or general canonical-error exception is implied. Ordinary admitted file errors retain canonical parity; this specific CLI-versus-Session difference is pinned separately. A shared non-regular-source policy remains follow-up work.
<!-- /generated-guidance -->
