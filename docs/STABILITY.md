# Stability contract

Oxblend 1.0 follows semantic versioning. The compatibility baseline starts at
1.0.0. Patch releases fix defects without intentionally
breaking this contract; minor releases add compatible capabilities; incompatible
changes to stable interfaces require a new major release. The source is authoritative; `scripts/qualify.sh` checks the inventories
below. See [USAGE](USAGE.md) for arguments and [ARCHITECTURE](ARCHITECTURE.md) for
reading, publication and durability boundaries.

## JSON generations

A `schema` value identifies a report family, not a global protocol version.
Numeric `1` and `2` coexist; they are JSON numbers, not strings. There is no
schema-negotiation option. Current emitters are below; literal-site counts are
source inventories (not processing limits), so removing one emitter cannot hide
behind another occurrence of the same schema value.

<!-- schemas -->
| Source | Schema value | Literal sites | CLI report |
| --- | --- | --- | --- |
| `src/main.rs` | `1` | `6` | inspect, get, pointer, payload, source-facts, library-facts; hard-error stderr envelope |
| `src/summary.rs` | `1` | `1` | summary (library-owned) |
| `src/trace.rs` | `1` | `1` | trace |
| `src/overrides.rs` | `1` | `1` | overrides |
| `src/edit.rs` | `1` | `1` | rewrite |
| `src/pack.rs` | `1` | `1` | pack, pack-sequence; nested trace/rewrite reports retain their own schemas |
| `src/typed_edit.rs` | `2` | `1` | edit (format-only and comp) |
| `src/diff.rs` | `"oxblend.diff/v1"` | `1` | diff |
| `src/main.rs` | `"oxblend.diff/v1"` | `1` | diff returned-error stdout |
| `src/file_ops.rs` | `"oxblend.pointer-census/v2"` | `1` | pointer-census |
<!-- /schemas -->

`exr-header` and the generic graph report currently have no top-level schema
discriminator (`src/exr_header.rs`, `src/file_ops/graph.rs`).
`list` defaults to text; `list --json` (including `--full-info`) emits JSON
without a schema discriminator. `blocks`, help and version emit text
(`src/main.rs`, `src/bat_cli.rs`). Rewrite input specifications use numeric schema 1; typed-edit
specifications use numeric schema 2 (`src/main.rs`, `src/typed_spec.rs`).

Within a report generation, consumers should tolerate added object fields and
unknown diagnostic kinds; do not treat object key order, timings or human error
messages as stable identifiers. An existing field's removal, rename, type or
meaning change is not an additive change and must be disclosed. Within 1.x, existing report fields retain their names, types and meanings;
minor releases may add fields and diagnostic kinds. Breaking report changes
require both a new major release and a new schema generation for reports with
a discriminator. Reports without one follow the same major-version rule.
This does not freeze values derived from an input file or prevent correctness
fixes from reporting newly discovered dependencies or honest incompleteness. This is an output-consumer policy, **not** permission to add keys
to edit specifications: their parsers reject unknown fields.

## CLI exits and errors

All commands and documented flags in [USAGE](USAGE.md), including their aliases,
argument meanings and the exit-code meanings below, are stable in 1.x. New
commands/flags may be added in minor releases. Text listing layout, help prose,
timings, physical block indices and human diagnostic messages are not machine
interfaces. Use JSON and error kinds for automation. Conservative refusals may
be corrected or newly exposed by stricter validation; this is not a promise
that every file accepted by one version remains accepted despite defects.

These are the complete application-selected exit codes in `src/main.rs` (not
shell/signal/loader failures). The second column names the checked dispatch
cases, not additional error kinds:

<!-- exit-codes -->
| Exit | Dispatch cases | Meaning |
| --- | --- | --- |
| `0` | `success` | Normal return, including help/version; stdout write/flush completed. |
| `2` | `default_error` | Every other returned error, including broken_pipe, usage and incomplete reports. |
| `3` | `pack_input`, `missing_input` | Those two input error kinds only. |
| `5` | `pack_project` | Project-selection error only. |
<!-- /exit-codes -->

Returned errors, including `broken_pipe`, produce a schema-1 JSON stderr envelope
where stderr remains writable, with `error.kind` and `error.message`. Common
refusals add `error.next_command`: structured `argv` (UTF-8 or null) and exact
native-byte `argv_hex`; this is data, not executable shell text. Suggestions do
not retry writes. Local diagnostics use flattrace v1 when `FLATTRACE_DIR` is
nonempty. This does not change stdout schemas or command exit codes; sink
failure warns once and disables tracing. Reader fallback warnings remain. A report can be printed to stdout before failure; Rust
`Ok(report)` can contain `complete:false`. Check completeness and each
publication/durability field, not only whether JSON parsed or a file exists.

The following is the complete current **built-in source-defined error-kind
vocabulary**, across the crate and CLI, including report errors. It is not a
closed Rust enum or a promise that every kind is reachable from every command.
`Error::new` and public string fields also permit callers to construct other
kinds. Warnings, pointer states, operation tags and reference-hop kinds are not
error kinds. Source: production `Error::new`/edit-failure construction under
`src/`, the kind-taking helper in `src/file_ops/roles.rs`, and serialization of
`FailureKind`/`OverrideFailureKind` in `src/trace.rs`/`src/overrides.rs`.

<!-- error-kinds -->
| Error kinds (alphabetical) |
| --- |
| `ambiguous_id`, `ambiguous_pointer` |
| `bounds`, `broken_pipe` |
| `closure_owner`, `compression`, `conflicting_edits`, `constructor_type`, `crypto_retarget`, `crypto_scene_kept_edge`, `crypto_source_id`, `cstring_width`, `cycle` |
| `dangling_pointer`, `declared_diff`, `decoded_proof`, `diff_extent`, `diff_identity`, `diff_pointer_array`, `duplicate_data_address`, `durability` |
| `edit_conflict`, `edit_spec`, `edit_version`, `empty_clear`, `encoding`, `expected_old`, `expected_path`, `exr_header`, `exr_unsupported` |
| `field_owner`, `field_type`, `file_ops_profile`, `flag_mask` |
| `identifier_overflow`, `incomplete_diff`, `incomplete_edit`, `incomplete_overrides`, `incomplete_pack`, `incomplete_trace`, `input_changed`, `invalid_block`, `invalid_graph`, `invalid_header`, `invalid_layout`, `invalid_sdna`, `io` |
| `json` |
| `kit_dna` |
| `library_read`, `library_reader`, `link_owner`, `link_profile`, `list_backlink`, `list_boundary`, `list_cycle`, `list_endpoint`, `list_endpoints`, `list_header`, `list_integrity`, `list_reciprocity`, `list_symmetry` |
| `missing_address`, `missing_asset`, `missing_field`, `missing_id`, `missing_input`, `missing_library` |
| `name_collision`, `node_constructor`, `node_owner` |
| `output_compression`, `output_decode_length`, `output_exists`, `output_path`, `overflow`, `owner_header`, `owner_projection` |
| `pack_collision`, `pack_destination`, `pack_input`, `pack_origin`, `pack_path`, `pack_project`, `padding_write`, `path`, `path_context`, `path_nul`, `path_overflow`, `path_termination`, `pointer_count`, `pointer_depth`, `pointer_extent`, `pointer_path`, `pointer_proof`, `pointer_role`, `pointer_target_type`, `pointer_write`, `profile_discriminator`, `profile_field`, `profile_library`, `profile_linked`, `profile_mask`, `profile_owner`, `profile_temporal`, `property_collision`, `property_owner`, `property_selection`, `property_type` |
| `raw_profile`, `record_extent`, `required_pointer_null`, `retained_removed`, `rlayers_source_id` |
| `same_input`, `seek_table`, `serialization`, `sha256_width`, `snapshot_mismatch`, `socket_constructor`, `socket_owner`, `source_changed`, `spill_space`, `storage_profile`, `stray_data_run`, `string_nul`, `string_termination` |
| `text_constructor`, `truncated`, `type`, `typed_kind`, `typed_overflow`, `typed_width` |
| `unprofiled_role`, `unresolved_persistent`, `unsupported_edit`, `unsupported_format`, `unsupported_type`, `usage` |
| `zip_encoding`, `zip_path`, `zip_width` |
<!-- /error-kinds -->

In particular, trace uses `library_read`, whereas overrides uses
`library_reader` and carries the underlying kind separately as `reader_kind`.
They are different current spellings, not interchangeable aliases.

## Public Rust API

The crate root exposes `BlendFile`, `Header`, `Block`, `Error`, `Result`, and
these modules (`src/lib.rs`): `bat_cli`, `diff`, `edit`,
`exr_header`, `facts`, `file_ops`, `overrides`, `pack`, `schema`,
`summary`, `trace`, `typed_edit`, `typed_spec`, `value`. **Stable in 1.x:** the root types and read-side modules `schema`, `value`,
`trace`, `pack`, `diff`, `exr_header`, `facts`, `summary` and `overrides`.
`summary::summarize(&BlendFile)` returns owned schema-1 `serde_json::Value`,
reusing the supplied reader without reopening it.

**Unstable/internal:** `bat_cli`, `edit`, `typed_edit`, `typed_spec` and `file_ops`. They are public for reuse, but their Rust signatures,
types and behavior may change in minor releases without a semver guarantee;
rustdoc flags this boundary. Structural editing planned for 1.1 may reshape
these construction/editing APIs. Exception: `edit::EditOptions`,
`edit::EditReport`, `edit::EditRange`, `edit::EditFailure` and `edit::Durability`
are **stable because reachable from stable APIs** (including pack's nested
rewrite reports and persistence policy); their re-export path remains stable.
`EditOptions` supplies `new`/`default`; `Durability` supplies named variants,
`default` and `FromStr`. Reports/ranges/failures are returned by operations,
not externally constructed. These structs and the enum are non-exhaustive.
Their CLI commands and JSON contracts remain
stable as described above. This is not a claim of Blender evaluation or support
for every serialized layout. Private and
`pub(crate)` modules are not external API. Exact current signatures can be
browsed with `cargo doc --offline --locked --no-deps`.

The current public enums are marked `#[non_exhaustive]`: `Value`, `PathStep`
(`src/value.rs`); `Durability` (re-exported by `edit`, defined in
`src/edit_output.rs`); `TypedValue`, `EditMode` (`src/typed_edit.rs`);
`TraceMode`, `PointerMode`, `FailureKind`, `TraceWarning` (`src/trace.rs`);
`OverrideScope`, `PointerState`, `OverrideFailureKind`, `OverrideWarning`,
`ReferenceHopKind` (`src/overrides.rs`). External matches need a fallback arm.
Public options/report structs in the stable surface are non-exhaustive.
Use constructors/defaults where available and read returned reports rather than
constructing them exhaustively. Non-exhaustiveness permits compatible extensions.
Input-derived addresses/indices remain local to that exact file, not persistent
identifiers.

### Stable crate-owned type closure

The stable signatures, public fields and their crate-owned transitive types are:
root `BlendFile`, `Header`, `Block`, `Error`, `Result`; schema `Schema`, `Struct`,
`Field`; value `Value`, `PathStep`; trace `TraceOptions`, `TraceMode`,
`PointerMode`, `TraceReport`, `Usage`, `FieldReference`, `TraceFailure`,
`FailureKind`, `MissingId`, `TraceWarning`; pack `PackOptions`, `PackReport`,
`PackFile`; overrides `OverrideOptions`, `OverrideScope`, `OverrideReport`,
`OverrideRecord`, `OverrideProperty`, `OverrideOperation`, `Provenance`,
`IdIdentity`, `OverrideFailure`, `OverrideFailureKind`, `OverrideWarning`,
`LibraryLink`, `ReferenceHop`, `ReferenceHopKind`, `PointerState`; exr_header
`ExrHeader`, `Part`, `Attribute`, `Channel`, `Pass`; and the five stable edit
types named above. `diff`, `facts` and `summary` return `serde_json::Value`; standard-library
and dependency types retain their own contracts. Private and pub(crate) helper
types are not external interfaces. Options have defaults/constructors; reader
and report values come from the parsing/report operations.

## Truthful no-op rewrites

For path rewriting (`src/edit.rs::rewrite_opened`), no effective edit means
all prepared ranges have `write_length == 0`, including an empty request.
Without forced compression, that path copies the **physical source bytes** to
a new destination, with source guards and no-replace publication; it does not
re-encode the container. Thus zero edits can truthfully yield
`output_written:true` and `complete:true`. That is a successful new-output copy,
not a claim that a path changed. Failed validation is not a no-op success.

Read `edits`, `write_length`, `container_reencoded`, `output_decoder_verified`,
`output_written`, `durability_completed`, `errors` and `complete` separately.
Unchanged copies and successful changed raw-file edits do not claim a fresh
full output decoder pass (`output_decoder_verified:false`). Changed compressed
outputs are decoder-verified before publication and report true. False is a
verification limit, not a failed publication or an automatic incompleteness signal. Independently
query the new field and verify decoded bytes; native Blender validation is separate.
For raw inputs, compare equal-length files and permit differences only inside the
reported decoded `offset..offset+write_length` ranges. For compressed inputs, use
`inspect` on input and output to verify identical headers, SDNA and ordered block
metadata (`data_offset`, `size`, code, address, SDNA index, count). For every block,
run `payload FILE BLOCK` on each file and hex-decode `value.hex`. Require equal
payload lengths. At payload byte index i, the decoded-file offset is
`blocks[BLOCK].data_offset + i`; require equal bytes unless that offset lies in a
reported write range. Verify each replacement independently with `get`. This
compares complete decoded block payloads, not physical compressed offsets or
container bytes; it is not an independent full-container decoder certificate.
A pack's explicit compression request may
re-encode even without path changes. Neither an empty edit list nor successful
publication implies Blender evaluation or native semantic verification.

## Pack warnings for unselected paths

The schema-1 pack report has a `warnings` array. An
`unrelocated_unselected_path` warning identifies a non-relative path in an
unselected datablock of a copied library whose pointer-backed field is retained
byte-for-byte rather than rewritten. Owner, ID name, field path and asset path
have authoritative hex forms and UTF-8 forms when valid (otherwise null).
`pack-info.txt` includes `Unrelocated unselected paths: N` when N is nonzero.
These warnings do not make an otherwise complete selected-closure pack
incomplete. They do not claim the entire copied library is portable for other
uses: selected Image/Library/other dependency fields still relocate, and a
selected pointer-backed path requiring relocation still refuses `pointer_write`.
No pointer-field write admission is added. Explicit path exclusions and
`--relative-only` retain their existing filtering semantics: paths excluded
from local normalization do not generate these warnings.

## Interpreted Object data and linked IDs

Dependency expansion follows `Object.data` as its registered ID union, despite
its `void*` SDNA declaration. `Object.type` determines the ID family; the native
header, count of one, and target layout must agree with the source file's SDNA.
This includes Mesh-owned materials when Object material overrides are empty.
NULL data contributes no edge. Raw DATA, non-ID layouts, mismatched families,
and invalid counts/extents refuse rather than silently omitting dependencies.
Generic pointer-only SDK lookup remains namespace resolution, not this typed
interpretation or a write capability.

A linked placeholder is not proof of the actual library target. Requested IDs
must either exist or be reported as external incompleteness; known interpreted
native IDs receive the same header/count/source-layout proof. A missing identity
is `missing_id`, with no physical block address because no such record exists.
The typed record carries root, existing library path, saved ID code/name and the
referencing owner chain (authoritative hex and UTF-8 when valid). Other IDs keep
tracing. Like `missing_library`, it forces incomplete/EXIT 2 but permits a pack to
publish its existing members. Corrupt present IDs still refuse all publication.
Trace JSON includes `missing_link_counts`; pack JSON also has aggregate
`missing_links`, and pack-info lists both classes with nonzero counts/details.
Both single- and multi-root `pack-info.txt` mark known missing-file/linked-ID
packs `INCOMPLETE`. Nonzero distinct missing physical paths produce
`Missing files: N\n`, followed by `Missing file: PATH\n` for each sorted path.
PATH is quoted/escaped decoded UTF-8 or `hex:...` for non-UTF-8 native bytes;
JSON keeps its existing missing usage records. The missing-link footer follows
this opening/missing-file text. It emits `Missing libraries: N\n` when that count is nonzero, then
`Missing linked IDs: N\n` when that count is nonzero. It then emits one
`KIND: RECORD\n` line per `missing_links` record, in report order, where KIND is
`missing_library` or `missing_id` and RECORD is compact UTF-8 JSON with sorted
object keys and no spacing. The record is the exact typed pack-JSON record,
including authoritative hex fields and UTF-8-or-null aliases; zero-count headings
are omitted. Existing unrelocated-path diagnostics, when present, follow this
footer. These are additional bytes, not normalization or a completeness claim.
An explicit native ID BHead may use the file's first SDNA struct at index zero;
that does not turn an opaque DATA0 allocation into an ID, or admit a write.
`FailureKind::MissingId` belongs to the non-exhaustive enum. The baseline includes
`TraceFailure.missing_id: Option<MissingId>`: this report is now
non-exhaustive, so external clients consume returned failures rather than
constructing them with struct literals. This is part of the 1.0
baseline, not a precedent for adding required public struct fields in 1.x. JSON remains schema 1;
the structured missing-ID fields and derived missing-link views are additive.

Saved linked stubs are followed even when their collection is unused: keeping
their dependencies is conservative collection, not an evaluation of the scene.

Missing-link counts describe referencing sites/chains, not unique absent IDs.
`list` retains its conservative EXIT 2/no-listing policy for `missing_id`;
`trace`/`pack` expose typed details. Missing physical assets and libraries now also refuse `list` with EXIT 2 and a typed stderr error before any listing. This corrects misleading success, without changing the existing JSON dependency-map or full-info shape. Trace remains the detailed inventory; no partial-list protocol is introduced.

## Additive usability fields

Summary adds `materials`, `meshes`, `images`, `collections` and typed `warnings` arrays. Names retain their saved ID prefixes, as existing Object names do. Missing summary fields are warning-only; unavailable Scene fields are null, and other failure kinds still refuse.

## 1.2 task and delivery fixes

Summary stays numeric schema 1: existing fields retain their types/meaning.
Every saved Scene element returns in block/element order, with additive
`frame_start`, `frame_step`, and nested `fps` containing `numerator` (saved
`frs_sec`), `base` (saved float32 `frs_sec_base` as a JSON number), `effective`
(float64 numerator/base) and `base_bits_hex` (exact saved float32 bits).
Missing values are null with typed warnings carrying block/element/field context.
A zero/nonfinite base warns and makes effective FPS null; nonfinite base is null
with exact bits retained. No active-Scene choice or evaluated animation is claimed.

Broken-pipe exit 0 was a delivery defect, repaired to exit 2 / `broken_pipe` in
1.2.0, including text/help/version. Capture producer status before filtering;
a downstream shell pipeline can still hide it. Published pack/edit effects are
not undone by output failure. `skill [print]` is a new text command; its embedded
guide is version-matched. Version 1.3.0 removes the interim
`OXBLEND_PHASE_TIMING`, `OXBLEND_PHASES` and `OXBLEND_READ_REPORT` emitters;
those variables no longer activate output. Their measurements belong in local
flattrace span data and reader/copy statistic points.

Trace usages add `resolved_path_hex`, UTF-8-or-null `resolved_path`, and `exists`. The top-level `missing_path_count` counts absent usage sites, not distinct files; `referenced_paths_exist` is true when that count is zero. These are disk observations at JSON serialization time, not an atomic filesystem snapshot: sequence/glob/UDIM existence means a nonempty expansion, not every possible member. Directories count as present. `complete` retains its traversal meaning and does not promise disk availability. Nested trace reports and list full-info usages expose the same fields.

Trace/list full-info/pack/edit/rewrite add UTF-8-or-null aliases beside authoritative hex path/name/field/type fields, recursively. Component/path arrays are null if any component is invalid UTF-8. Existing aliases retain their meanings. Binary payload/bit fields remain hex. Ordinary list JSON retains its native-byte map encoding.

Diff adds `difference_count`, equal to the number of reported differences (not proof that unresolved content is equal). CLI returned comparison/read errors also emit the existing `oxblend.diff/v1` schema (not numeric schema 1) with `equal:null`, `complete:false`, typed `errors`, and empty differences/count 0 because no comparison completed. The original typed stderr error and exit 2 are retained. Argument-count/usage refusals remain stderr-only; Rust `diff::compare` still returns `Err` on those underlying failures. After input container/layout validation, physical byte equality bypasses semantic matching and proves `equal:true` even for opaque/unanchored allocations. `matching` identifies that basis; no Blender evaluation is implied.

## EXR attribute values

`exr-header` keeps its existing header/channel metadata and adds attribute
`name`, `type` (UTF-8 when valid) and `decoded_value`. That value is either
`status:decoded` with a typed value, or `status:refused` with a typed refusal
kind and raw hex. Supported types are string, int, float, double, v2i/v2f,
v3i/v3f, box2i/box2f, compression, lineOrder, chlist, m33f and m44f.
Fixed-width values require exactly their format-defined byte lengths;
nonfinite numbers and unknown enum codes refuse rather than becoming JSON null.
Unknown types refuse locally, without invalidating an otherwise valid header.

String payloads are exactly their declared attribute bytes, not NUL-terminated
or length-prefixed a second time. They include hex and `utf8_valid`; invalid
UTF-8 has null text. Cryptomatte manifests and Blender File/Date/RenderTime
strings remain strings: their contents are not parsed, validated as JSON, or
claimed correct. Vectors use file component order; boxes expose min/max;
matrices expose rows. Reading stays within each declared attribute size.

Structural header failures (including truncated/out-of-file payloads and the
existing required-attribute/channel constraints) remain whole-file errors.
`header_complete:true` and the per-attribute value status do not verify any
pixel data, chunk offsets or manifest semantics. The public `Part.values` map
now retains every raw attribute payload instead of only selected names.
