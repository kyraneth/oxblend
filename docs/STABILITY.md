# Stability contract

Oxblend 1.4.1 follows semantic versioning. The compatibility baseline starts at
1.0.0. Patch releases fix defects without intentionally
breaking this contract; minor releases add compatible capabilities; incompatible
changes to stable interfaces require a new major release. The source is authoritative; `scripts/qualify.sh` checks the inventories
below. See [USAGE](USAGE.md) for arguments and [ARCHITECTURE](ARCHITECTURE.md) for
reading, publication and durability boundaries.

## IDProperty string value edits

Typed-edit schema 2 optionally accepts `idprop_strings` (`idprop-string`), a
same-allocation UTF-8 value operation on 502/503 layouts checked by saved SDNA
field names/types, not a structural-profile version bypass. Name and old-value
guards, same-owner/no-alias admission, capacity including NUL, and atomic
payload/len updates are required; pointer, totallen and allocation stay fixed.
Unknown layouts/headers refuse with `idprop_string` or the underlying typed
reader error. Existing input-change, declared-diff, container and no-replace
publication contracts apply. See USAGE for the exact transport and conservative
opaque-pointer alias scan. Structural node-tree gates are unchanged.

## JSON generations

A `schema` value identifies a report family, not a global protocol version.
Numeric `1` and `2` coexist; they are JSON numbers, not strings. There is no
schema-negotiation option. Current emitters are below; literal-site counts are
source inventories (not processing limits), so removing one emitter cannot hide
behind another occurrence of the same schema value.

<!-- schemas -->
| Source | Schema value | Literal sites | CLI report |
| --- | --- | --- | --- |
| `src/main.rs` | `1` | `1` | hard-error stderr envelope |
| `src/read_reports.rs` | `1` | `5` | inspect, get, pointer, payload, source-facts, library-facts shared CLI/Python projections |
| `src/exr_pixels.rs` | `1` | `1` | named-pass pixel extraction |
| `src/summary.rs` | `1` | `1` | summary (library-owned) |
| `src/bat_cli.rs` | `1` | `1` | list warning stderr envelope (all warning kinds, independent of verbosity) |
| `src/trace.rs` | `1` | `1` | trace |
| `src/multi_root.rs` | `"oxblend.multi-root/v1"` | `1` | multi-root trace/list |
| `src/multi_root.rs` | `"oxblend.cross-reference/v1"` | `1` | common dependency cross-reference |
| `src/overrides.rs` | `1` | `1` | overrides |
| `src/edit.rs` | `1` | `1` | rewrite |
| `src/pack.rs` | `1` | `1` | pack, pack-sequence; nested trace/rewrite reports retain their own schemas |
| `src/typed_edit.rs` | `2` | `1` | edit (format-only and comp) |
| `src/node_tree_edit.rs` | `1` | `2` | node-tree inspection and structural edit report |
| `src/diff.rs` | `"oxblend.diff/v1"` | `2` | diff and shared returned-error stdout projection |
| `src/file_ops.rs` | `"oxblend.pointer-census/v2"` | `1` | pointer-census |
| `src/query.rs` | `"oxblend.query/1"` | `1` | query; transport schema in query.schema.json |
| `src/query.rs` | `"oxblend.catalog/1"` | `1` | catalog |
| `src/query.rs` | `"oxblend.explain/1"` | `1` | explain |
| `src/error_help.rs` | `"oxblend.error-help/v1"` | `1` | offline error catalog entry |
| `src/report.rs` | `"oxblend.report-created/v1"` | `1` | report publication receipt |
| `src/report_input.rs` | `"oxblend.direct-evidence/v1"` | `1` | internal typed library error evidence; not a CLI invocation envelope |
| `src/report_privacy.rs` | `"oxblend.redacted-error-report/v1"` | `1` | sanitized report derivative; optional warning diagnostics |
| `src/report_privacy.rs` | `"oxblend.error-source/v1"` | `1` | typed source-context derivative; optional warning diagnostics |
| `src/report_privacy.rs` | `"oxblend.bug-bundle/v1"` | `1` | structural bundle manifest |
| `src/failure_receipt.rs` | `"oxblend.failure-receipt/v1"` | `1` | invocation-bound local failure receipt |
| `src/failure_receipt.rs` | `"oxblend.error-source/v1"` | `1` | instrumented source coverage |
| `src/report.rs` | `"oxblend.report-preview/v1"` | `1` | frozen preview receipt |
| `src/query/collection_graph.rs` | `"oxblend.collection_graph/1"` | `1` | shared saved collection graph |
| `src/extract.rs` | `1` | `1` | selected-ID partial file report |
| `src/read_batch.rs` | `1` | `1` | read-batch terminal summary (items reuse shared read-report schemas) |
<!-- /schemas -->

`exr-header` and the generic graph report currently have no top-level schema
discriminator (`src/exr_header.rs`, `src/file_ops/graph.rs`).
`list` defaults to text; single-root `list --json` (including `--full-info`) emits JSON
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

## Named EXR pixels (additive)

`exr-pixels FILE --part N --pass NAME --output NEW.f32` emits schema-1 layout
metadata plus a new atomically published raw f32le file. See USAGE for channel,
base-resolution, sample type and ordering semantics. `decode_complete:true`
requires exactly-once sample coverage and unchanged source signatures; it is not
a runtime claim of independent reference agreement. The `exr_pixels` Rust module
is new and experimental; existing header/scan schemas and pixel-integrity meaning
are unchanged. Integer/subsampled/deep selections refuse explicitly. Pixel output
uses the existing no-replace staging boundary and durability `none`.

## Multi-root dependency reports

Two or more lexically distinct absolute roots select `oxblend.multi-root/v1`
for trace/list. Deduplication preserves first encounter order and never merges
symlink root contexts. With one distinct root the existing JSON/text/exit path
is retained; explicit `trace --summary` is a new presentation request.

`root_traces` retains each single-root report or null plus its hard typed error.
Multi-list JSON also retains the legacy listing value and returned error in
`root_lists`. No bad root suppresses independent roots. Aggregate completeness
is availability-aware, including physical absence, missing IDs and unknown
subtrees; incomplete multi-root reports exit 2. The historical field
`cross_reference.roots[].traversal_complete` mirrors current `trace.complete`,
which already includes physical availability, not obsolete traversal-only proof.

`oxblend.cross-reference/v1` is the same trace-selected closure in trace/list
and the additive `cross_reference` field on multi-root pack-sequence, even on
dry-run and before refusal. Single-root packs have no new field. Asset IDs are
lowercase hex of native-byte realpaths (including resolved existing prefixes
of missing paths); hard links and equal contents do not merge. Root IDs remain
lexical. Assets retain all encountered spellings, root-to-owner library chains
and indices into each root's usages. Sharing counts distinct roots; missing IDs
are diagnostics, not files. Existing regular member sizes count once globally;
missing/nonregular sizes are null. No atomic filesystem or render guarantee.
Pack planning/member paths, strict publication refusal and durability meanings
are unchanged; excluded dependencies remain in the source cross-reference.
Unobservable filesystem identities/members retain every affected usage in
additive per-root `observation_errors`, with the first typed cause in `error`;
independent facts survive and pack refuses before planning/publication.

`--files-from PATH` / `--files-from=PATH` expands in encounter order; `-` reads
stdin. `-0` selects NUL delimiters for all list inputs. LF mode rejects NUL.
Empty entries are ignored; CR, spaces, # and non-UTF-8 bytes are literal. Paths
resolve against cwd; `--` ends options. Human matrix layout is not a protocol.

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
failure warns once and disables tracing. CLI commands keep the historical
seek-table fallback warning on stderr, rendered synchronously at emission.
The new `trace_operation`/`pack_operation`/`trace_session`/`pack_session`
entry points return those notices as `Operation` diagnostics instead;
legacy `Result`-only library calls stay silent. A report can be printed to stdout before failure; Rust
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
| `edit_conflict`, `edit_spec`, `edit_version`, `empty_clear`, `encoding`, `expected_old`, `expected_path`, `exr_decode`, `exr_header`, `exr_header_truncated`, `exr_selection`, `exr_unsupported`, `extract_refusal` |
| `field_owner`, `field_type`, `file_ops_profile`, `flag_mask` |
| `identifier_overflow`, `idprop_string`, `incomplete_diff`, `incomplete_edit`, `incomplete_exr_scan`, `incomplete_facts`, `incomplete_overrides`, `incomplete_pack`, `incomplete_query`, `incomplete_read_batch`, `incomplete_trace`, `input_changed`, `invalid_block`, `invalid_graph`, `invalid_header`, `invalid_layout`, `invalid_saved_structure`, `invalid_sdna`, `io` |
| `json` |
| `kit_dna` |
| `library_read`, `library_reader`, `link_owner`, `link_profile`, `linked_reference_only`, `list_backlink`, `list_boundary`, `list_cycle`, `list_endpoint`, `list_endpoints`, `list_header`, `list_integrity`, `list_reciprocity`, `list_symmetry` |
| `missing_address`, `missing_asset`, `missing_field`, `missing_id`, `missing_input`, `missing_library` |
| `name_collision`, `node_constructor`, `node_cycle`, `node_endpoint`, `node_expected_graph`, `node_graph`, `node_occupied`, `node_owner`, `node_socket_type`, `node_version` |
| `output_compression`, `output_decode_length`, `output_exists`, `output_path`, `overflow`, `owner_header`, `owner_projection` |
| `pack_collision`, `pack_destination`, `pack_input`, `pack_origin`, `pack_path`, `pack_project`, `padding_write`, `path`, `path_context`, `path_nul`, `path_overflow`, `path_resolution_unverified`, `path_termination`, `pattern_members_unknown`, `pointer_count`, `pointer_depth`, `pointer_extent`, `pointer_path`, `pointer_proof`, `pointer_role`, `pointer_target_type`, `pointer_write`, `profile_discriminator`, `profile_field`, `profile_library`, `profile_linked`, `profile_mask`, `profile_owner`, `profile_temporal`, `property_collision`, `property_owner`, `property_selection`, `property_type` |
| `query_cycle`, `query_list` |
| `raw_profile`, `record_extent`, `required_pointer_null`, `retained_removed`, `rlayers_source_id` |
| `same_input`, `seek_table`, `serialization`, `sha256_width`, `snapshot_mismatch`, `socket_constructor`, `socket_owner`, `source_changed`, `spill_space`, `storage_profile`, `stray_data_run`, `string_nul`, `string_termination` |
| `text_constructor`, `truncated`, `type`, `typed_kind`, `typed_overflow`, `typed_width` |
| `unknown_concept`, `unprofiled_role`, `unresolved_persistent`, `unsupported_edit`, `unsupported_format`, `unsupported_type`, `unverified_semantics`, `usage` |
| `zip_encoding`, `zip_path`, `zip_width` |
<!-- /error-kinds -->

Saved Image pattern usages add byte-authoritative `expected_members_hex` and
`missing_members_hex` with UTF-8-or-null member aliases, `pattern_status` and
`pattern_unknown`. Required tiles come from the saved Image tile list; sequence
spans come from associated saved node/texture ImageUser duration and offset,
unioned across checked linked-ID consumers. Disabled automatic refresh also
requires the saved current frame; unavailable refresh/current state is UNKNOWN.
Missing members are physical absence; unproved spans are
`pattern_members_unknown` (UNKNOWN), not successful nonempty glob discovery.
Both make reports incomplete and block strict publication. Other generic cache
patterns retain discovery semantics. These are saved declaration guarantees,
not evaluated scene-time or render guarantees.

A TILED Image whose saved filepath is a concrete tile file (no `<UDIM>` or
`<UVTILE>` token in the filename) expands exactly like Blender on load, not
like Blender on save. Blender requires the literal token only to save
(`image_save.cc:667`, `BKE_image_get_tile_strformat` returning
`UDIM_TILE_FORMAT_NONE` refuses the save); on load and reload it keeps the
saved `Image.tiles` list and auto-tokenizes a concrete basename with its own
filename-only regexes (`BKE_image_ensure_tile_token_filename_only`,
`source/blender/blenkernel/intern/image.cc:3773`: `(.*[._-])([12]\d{3})([._-].*)`
then `(.*)(u\d{1,2}_v\d{1,3})(\D.*)`, reached through
`BKE_image_get_tile_info`, `image.cc:3597`). Oxblend therefore applies the
same two regexes in the same order to the filename portion only, and expands
only the saved tile numbers through the derived pattern (`<UDIM>` becomes the
zero-padded tile number, `<UVTILE>` becomes `u{}_v{}`); such usages report
`is_udim:true`. The directory is never globbed to invent members: a missing
saved tile makes the report incomplete with its concrete missing list and
blocks strict publication. A basename matching neither regex, a tile list that
is unavailable, empty or invalid, and a token appearing only in a directory
component stay `pattern_members_unknown` with the existing reasons; packed
images bypass file expansion. One deliberate deviation: a basename containing
a newline byte is tokenized on the globally rightmost viable site, while
Blender's `std::regex` (whose `.` excludes newline) tokenizes within the
first viable newline-delimited segment and drops the rest on replacement;
Blender's lossy per-line behavior is not replicated.

In particular, trace uses `library_read`, whereas overrides uses
`library_reader` and carries the underlying kind separately as `reader_kind`.
They are different current spellings, not interchangeable aliases.

The reviewed dynamic forwarding forms are exact source inventories, not
permission to invent an error namespace. Existing `e.kind`, role-helper `kind`,
pointer-census `s.state`, and trace selected-root `kind` forwarding keep their
originating values. Query's typed-property decoder adds record/field context
while forwarding the original checked-reader `e.kind`; it introduces no remap.
In 1.4.0 pack also forwards `TraceFailure::kind_str()`
into its refusal causes: it is the same mapping as `TraceFailure::json()`
(`Reader(k)` stays `k`; `LibraryReader(_)` stays `library_read`; the six other
variants retain their established literal kinds). This adds a reviewed call
site, not renamed kinds or a different application exit dispatch. The checker
still rejects every unreviewed expression and checks the complete kind set,
schema emitters and exact exit handler.

## Selected-ID partial writer (1.4.0, additive)

`extract` and `extract::ExtractOptions`/`extract::extract` are experimental.
The schema-1 report preserves all policy decisions and refusals; a refusal
publishes nothing. Exact names must resolve uniquely; `--block` resolves duplicate
names against this source only. Closure uses native owner namespaces and source
SDNA, preserves old addresses, follows typed DATA storage without constructing
new records, and admits opaque bytes only through existing raw-role checks.
Known void-pointer roles retain the shared native ID/DATA namespace (including
older `Object.data` → Mesh); only unprofiled void storage uses owner-local typed
BHeads. A failed known lookup never falls back to another namespace.
Typed pointer arrays are traversed with the declaring element type; void tables
and unknown/callable pointers refuse. External Main IDs require explicit
selection or an explicit typed clear rule. Cross-owner DATA requires the owning
Main ID to be explicitly selected too. Embedded root promotion is refused.
ID names are byte-authoritative (`name_hex`, UTF-8-or-null `name`); block
selection works for non-UTF-8 names. `requested_clear_rules` and
`unused_clear_rules` expose requested policies that changed no nonzero sites.

The shared import-discard profile is applied and enumerated as
`runtime_sanitation`, never mistaken for an explicit caller policy. Additional
extract-only 502 cases were proved with native 5.2.1 libraries.write/open/append:
`bNodeTreeInterfacePanel.items_array` only when `items_num==0` and no saved
allocation exists, and `bNodeSocketValueMenu.enum_items` only for the exact
runtime pointee type and no saved allocation. Nonempty missing interface arrays
refuse. No 503 claim follows from this evidence.

ID next/prev and GLOB context pointers are zeroed and reported as structural
fixups. Saved ID user counts are preserved; native loading recomputes users.
Source header/version/DNA and other retained bytes are unchanged. GLOB and DNA1
are retained, ENDB emitted; REND/TEST/USER are not needed in observed native
5.2.1 libraries.write outputs (both a tree-only and tree-plus-Scene closure).
Atomic new-path publication/durability use the existing editor publisher;
source signatures include device/inode/size/mtime/ctime and nanoseconds before
reading and immediately before publication. Output compression is not preserved.
Serialized completeness does not certify native evaluation/rendering. Opening a
standalone library in Blender synthesizes an empty Scene and emits the expected
`Library file, loading empty scene` warning; no saved Scene/Object is implied.

## Structural compositor profile (1.4.0, additive)

`node-tree FILE` and `edit --mode node-tree` use the separate schema-1
[expected-graph contract](NODE-TREE-EDIT.md). Native build `9e2066aef7ef` (5.2.1),
subversion 44 and exact template DNA are required. Explicit constructors only;
no arbitrary pointer write or generic node constructor is exposed. Inspection
provides topology/endpoint guards, not scalar-state or native-evaluation proof.
Original allocations outside reported mutations remain exact; output is raw,
new-output-only and uses the existing atomic publication/durability mechanism.
`native_verified:false`/`semantic_verified:false` remain honest runtime facts.
Native test evidence does not widen the supported version/type boundary.

## Captured decoded equality (v1 repair)

Diff validates both captured decoded inputs before testing exact stream equality.
Equal decoded bytes return the current v1 complete/equal report with zero rows,
without a pointer census or field inventory. This is serialized proof, not native
interpretation or evaluated/render equivalence. The unequal path retains v1
behavior. The additive Rust API `diff::compare_with_options` accepts
`diff::CompareOptions { temp_root }` and uses that frozen root for both input
opens. Legacy `compare`, the CLI and Python resolve the process temp directory
(`TMPDIR` on Linux, or the platform default) once per comparison. There is no
report spool in this v1 repair. Original source FD guards include ctime and wrap
decode, range reads and the final verdict. Detectable in-place mutation is
`input_changed`; replacement paths are never reopened to certify equality and
may conservatively refuse if unlinking changes the captured inode's ctime.
The root is used when spilling, not prevalidated for raw inputs. Unnamed decoded
spill IO failures remain errors.

## Public Rust API

The crate root exposes `BlendFile`, `Header`, `Block`, `Error`, `Result`,
`Session`, `Operation`, `Diagnostic`, and
these modules (`src/lib.rs`): `bat_cli`, `diff`, `edit`,
`exr_header`, `exr_scan`, `facts`, `file_ops`, `node_tree_edit`, `overrides`, `pack`, `query`, `schema`,
`session`, `summary`, `trace`, `tracing`, `typed_edit`, `typed_spec`, `value`. **Stable in 1.x:** the root types and read-side modules `schema`, `value`,
`trace`, `tracing`, `pack`, `diff`, `exr_header`, `exr_scan`, `facts`, `summary`, `session` and `overrides`.
`summary::summarize(&BlendFile)` returns owned schema-1 `serde_json::Value`,
reusing the supplied reader without reopening it.
`summary::summarize_typed(&BlendFile)` returns the owned `Summary` behind
that JSON without changing its schema-1 rendering or error precedence.
`Error::diagnostic_ref()` returns the optional run-local diagnostic reference as
owned JSON without replacing the error's internal identity. Python exceptions
preserve it as `.diagnostic_ref`; Session guard wrapping does not fabricate a new
cause. `File.byte_array` and `File.pointer_table` are binding adapters for the
same checked engine array reads, with explicit counts and Session-scoped blocks.
Default fast CLI `pack` and `pack-sequence` use signature-bound selective raw
windows for raw blends: only needed raw ranges are read, with no whole decoded
source retained on that path. The identity is the ctime-inclusive stat signature,
not a whole-source content hash or atomic filesystem snapshot. Compressed blends
fall back to full decoded capture. Python/Rust Sessions retain captured sources
as before; bounded additionally re-reads and verifies working bodies, lowering
peak memory at the cost of slower runs. Selected workload measurements showed 27–53% lower fast-path heap;
this is not a universal memory or timing guarantee.

`Session::open` captures one root with the fast retention policy; `Session::open_with_memory` selects `session::MemoryMode::{Fast, Bounded}`, reported by `Session::memory()` and fixed for the Session lifetime. `PackOptions.memory` defaults to fast and must equal the Session's opened policy for `pack_session`; a mismatch refuses as `usage` naming both modes, as does an unknown mode string. Pack reports and CLI failure receipts record `"memory":"bounded"` only; absence of the key means fast and keeps default report bytes. `Session::read` runs one checked root
transaction returning lifetime-independent owned values. `Session::query` plans
owned library requests, ends all arena borrows before capture, and evaluates over
read-only retained views with the same before/after source guards and lexical
source latches. Failed first captures remain operation-local, never automatic
replays or permanent negative cache entries. `trace::trace_session`
and `pack::pack_session` run the same guarded traversal over the retained
arena. `trace::trace_operation` and `pack::pack_operation` are the path-based
operation entries. Every operation returns `Operation<T>` (`result` plus
`diagnostics` on both success and failure); `Diagnostic` reports
seek-table fallbacks, latched source-check failures and displaced secondary
errors with typed paths. `pack::PackOptions::new` names the project root
explicitly for programmatic packs.

**Unstable/internal:** `diagnostics`, `bat_cli`, `edit`, `typed_edit`, `typed_spec`, `node_tree_edit` and `file_ops`. They are public for reuse, but their Rust signatures,
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
`Diagnostic` (crate root, defined in `src/session.rs`);
`OverrideScope`, `PointerState`, `OverrideFailureKind`, `OverrideWarning`,
`ReferenceHopKind` (`src/overrides.rs`). External matches need a fallback arm.
Public options/report structs in the stable surface are non-exhaustive.
Use constructors/defaults where available and read returned reports rather than
constructing them exhaustively. Non-exhaustiveness permits compatible extensions.
Input-derived addresses/indices remain local to that exact file, not persistent
identifiers.

### Stable crate-owned type closure

The stable signatures, public fields and their crate-owned transitive types are:
root `BlendFile`, `Header`, `Block`, `Error`, `Result`; session `Session`,
`Operation`, `Diagnostic`, `MemoryMode`; schema `Schema`, `Struct`,
`Field`; value `Value`, `PathStep`; trace `TraceOptions`, `TraceMode`,
`PointerMode`, `TraceReport`, `Usage`, `FieldReference`, `TraceFailure`,
`FailureKind`, `MissingId`, `TraceWarning`; pack `PackOptions`, `PackReport`,
`PackFile`; summary `Summary`, `SummaryHeader`, `SceneSummary`, `Fps`,
`Warning`; overrides `OverrideOptions`, `OverrideScope`, `OverrideReport`,
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
selected pointer-backed path requiring relocation refuses strict production with
`pointer_write`; permissive retains its original bytes, marks the skipped
relocation unknown and publishes only what is provable.
No pointer-field write admission is added. Explicit path exclusions and
`--relative-only` retain their existing filtering semantics: paths excluded
from local normalization do not generate these warnings.

## Interpreted Object data and linked IDs

Dependency expansion follows `Object.data` as its registered ID union, despite
its `void*` SDNA declaration. `Object.type` determines the ID family; the native
header, count of one, and target layout must agree with the source file's SDNA.
This includes Mesh-owned materials when Object material overrides are empty.
NULL data contributes no edge. Raw DATA, non-ID layouts, mismatched families,
and invalid counts/extents produce unknown-subtree diagnostics rather than silently omitting dependencies or guessing a target.
Generic pointer-only SDK lookup remains namespace resolution, not this typed
interpretation or a write capability.

A linked placeholder is not proof of the actual library target. Requested IDs
must either exist or be reported as external incompleteness; known interpreted
native IDs receive the same header/count/source-layout proof. A missing identity
is `missing_id`, with no physical block address because no such record exists.
The typed record carries root, existing library path, saved ID code/name and the
referencing owner chain (authoritative hex and UTF-8 when valid). Other IDs keep
tracing. Like `missing_library`, it forces incomplete/EXIT 2; only explicit
`--permissive` permits publishing available members. Strict is the default and
refuses all publication on any missing or unknown required item.
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
`list` prints its known listing for `missing_id` with EXIT 2 and the same typed
identity details as `trace`/`pack`. In 1.2.0 and 1.3.0, missing physical assets/libraries also refused before any listing. The explicitly ruled 1.3.1 contract correction below replaces that missing-physical-file behaviour.

## 1.4.0 reports and production policy (intentional changes)

Reports always tell everything; packages refuse unless `--permissive`.
Reading commands `list`, `trace` and `overrides` always report all provable
results and every missing/unknown site. Their `--strict`, `--permissive` and
`--non-strict` flags are accepted compatibility no-ops. Pointer failure stops
its unprovable subtree, never independent sites and never fabricates a target.
Unknown diagnostics add `status:"unknown"` plus a source `site`: file identity
on the enclosing error, block index/code, struct, field path, saved pointer
value (decimal string) and expected target type, with authoritative byte hex.
If a non-pointer invariant fails, pointer/type context may be null rather than
invented. Missing IDs retain their established `referencing_owner_chain` name.
An unreadable/corrupt dependency library is an unknown subtree; a malformed,
truncated or unreadable selected root remains a true input refusal.

Trace now returns `complete:false`, EXIT 2 for physical absence too; per-usage
`problem` gives its kind/reason and `problem_counts` summarizes all reported
kinds. Ordinary and full-info list both show unknown errors/counts and text
`[unknown]` records; verbosity may add detail, never hide a problem. Complete
list keeps its legacy bare map/full-info shape. No report envelope has `mode`,
and spelling a compatibility flag never changes its output shape.
After emitting an incomplete report, trace/list use the existing
`incomplete_trace` terminal error (physical-only list absence retains its
`missing_asset`/`missing_library` terminal kind); overrides uses
`incomplete_overrides`. Typed site causes remain in the report. Package
incompleteness uses the existing `incomplete_pack` terminal error, with typed
causes in its report. All use the unchanged default error exit2. This is the
intentional report-versus-refusal policy above, not a rename of those causes.

Only `pack` and `pack-sequence` use production strictness. Their JSON adds
`mode:"strict"|"permissive"`, `status` and `skipped_items`. Strict is the default;
any missing linked ID, missing file or unknown pointer/library subtree refuses
before any directory, ZIP, member or pack-info is created. Refusals retain typed
causes, catalogue entry anchors and a hint naming `--permissive`. Explicit
permissive (`--non-strict` alias) publishes only provable members, labels the
package INCOMPLETE, exits 2, and lists every skipped item/reason in JSON and
pack-info. Unknown saved bytes are not arbitrary write admission. Input-change,
output-collision and guarded-write safety are unchanged in both modes.

These are intentional default-behavior changes, not a patch silently preserving
old publication semantics: trace continues after unknown dependency edges with
a partial report/EXIT 2 instead of stopping collection; pack stops default
partial publication. Migrate partial-package workflows by adding `--permissive`,
retaining INCOMPLETE/EXIT 2 rather than treating them as success. Library
`TraceOptions.pointers` and `OverrideOptions.pointers` remain accepted for source
compatibility but no longer change report collection. `PackOptions.permissive`
opts in to partial production; its default is false.

Explicit `--relative-only` and `--exclude` narrow scope rather than silently
claiming a portable package. Their `skipped_items` records have `status:"excluded"`,
the flag/pattern reason and a physical-missing fact. Excluded absence is not a
strict failure; unattributable unknown pointers and unreadable libraries are.
`complete` is scoped delivery, separate from `self_contained`, which is false
for any incomplete or restricted package. `excluded_count` counts excluded
usage sites (not repeated link diagnostics). Pack-info states scoped completeness,
self-containment and that count separately. This supervisor interpretation of
strict production preserves intentional 1.3.3 scope controls without hiding
non-portability from a farm consumer.

Library name selection prefers all matching qualified local IDs over same-named
stubs. Only without a local match are all source-qualified forwarding candidates
selected. Distinct fallback libraries add `ambiguous_id`/unknown diagnostics;
every candidate is followed without guessing a unique origin. Direct pointer
references to stubs are followed regardless of the name rule. Native local/stub
fixtures and forwarding/cycle tests establish these distinct operations.
Forwarding selectors resolve to qualified local endpoints before their dependency
edges expand. Only unresolved forwarding-name ancestry participates in `cycle`
detection; endpoint dependencies can reuse a completed selector from the same
batch. True forwarding-only loops remain incomplete and prevent strict pack
publication. Discovery order and first-discovery owner chains can change with
this separation; required physical dependencies and unknown-site reporting are
not relaxed. Diagnostic `walk.file` spans distinguish `phase:"resolve"` from
`phase:"expand"` with the same saved file identity.

## 1.3.3 path and library tracing contract

Serialized Blender paths and native CLI input paths are separate domains.
A native input filename's backslashes are literal POSIX bytes (including
`overrides`). Serialized `//` and extra slashes after it resolve under the
owning blend directory, matching native Image, Library and Sequencer I/O.
Python's `bpy.path.abspath` alone is not equivalent to those loaders.
Relative embedded backslashes are normalized as separators, matching native
Image, Library and Sequencer I/O. Keep saved bytes for provenance; normalize
only at this shared resolution boundary. Absolute POSIX paths containing
backslashes follow the same proven native loader normalization. `Usage.asset_path` and its JSON
`asset_path_hex` now retain the exact saved bytes (including backslashes),
whereas `resolved_path` carries Blender's normalized dependency location.

Foreign Windows drives follow Blender's Linux dependency mapping
(`C:/assets/x` or backslash spelling → `/c/assets/x`); UNC retains its two
leading backslashes and normalizes the remaining separators. Drive forms have
dependency-walker evidence, not native I/O proof. UNC Image I/O was observed on
Linux using a literal POSIX name relative to the process CWD, not a real network
share; Library/Sequencer foreign mapping I/O remains unproven. Consequently UNC
existence is CWD-dependent, and portable relocation is not certified. Trace adds
`path_resolution_unverified` warnings with raw and resolved hex paths.
List emits these warnings on stderr (also in full-info). Default pack refuses
unverified foreign Windows-root relocation with the typed error of the same name,
before publication, rather than guessing across loader/platform semantics.
`--relative-only` leaves foreign absolute declarations unchanged and exposes
`skipped_absolute_paths[]` plus pack-info lines. A restricted complete pack
does not certify that skipped external dependencies are portable.

`get` on an unindexed pointer array returns `Value::Array` / JSON array
`items`, retaining every pointer in order. An indexed element remains a
single pointer. Literal existing directory components take precedence over
glob interpretation during sequence expansion. File outputs with a trailing
slash refuse; outputs preserve the input's ordinary permission bits, not
setuid/setgid/sticky bits. Missing/nonregular selected list/blocks inputs exit
3 before an open that could block on a FIFO.

### Stable in-process local tracing

`oxblend::tracing::{initialize, Span, Outcome, point, root_summary}` is stable in 1.x.
`Span::record_error(kind, message)` emits a structured `error` point on that
exact span, retains the stable kind/message and sets its default outcome to
error. An explicit `end(Outcome)` may refine that outcome. `point(event, Value)`
emits on the current thread scope (root if none). `root_summary(Value)` emits
on the active process root and retains additive caller facts for completion;
its output/resource arrays describe caller-supplied facts, not inferred effects.
Neither free function initializes tracing. All three are no-ops when disabled.
`tracing::run_identity() -> Option<Value>` returns
`{trace, root_span, identity_kind:"trace_root_span"}` for the active writer,
or None when disabled. The trace/root-span pair uniquely identifies this
invocation. It is **not** the private flattrace envelope `run` string, which
0.1 does not expose. No sink scanning or environment mutation is used.
These consume the unchanged flattrace 0.1 start/point/end/finish API; root facts
use a `root.summary` point because 0.1 finish has no payload argument.
`initialize()` reads `FLATTRACE_DIR`, `FLATTRACE_LEVEL` and
`FLATTRACE_TRACEPARENT` exactly once per process, including a remembered
no-op when disabled. It never mutates the environment. One process writer
owns one run; process exit closes it with outcome `unknown`, since a library
cannot infer the application's exit status. Per-call outcomes remain explicit. No implicit initialization occurs
when merely opening a BlendFile. The CLI keeps its own entry/exit behavior.
Do not use the internal `diagnostics` module as a stable API.

`Span::new(name, serde_json::Value)` initializes idempotently and returns a
non-Send/non-Sync synchronous per-call guard. Object attributes are recommended;
other values are wrapped by the writer. `data(Value)` sets end attributes;
`end(Outcome)` consumes the guard. Outcomes are `Ok`, `Error`, `Refused`,
`Partial`, `Cancelled` and `Unknown`; the enum is non-exhaustive.
Dropping without an explicit outcome records unknown, never fabricated success
or cancellation. Partial means an incomplete returned report; inspect that
report's completeness/publication fields. Cancelled requires an observed
cooperative stop, not a missing end after a kill. Process abort/kill, inherited
writers across fork and guards leaked past process exit cannot promise complete
runs. Attributes are caller-owned data, not automatically redacted; review
before sharing. Custom consumer event names need their own event inventory (or
format-only flattrace checking), not the CLI-only inventory. Writer errors warn once through fallible stderr writes, skip the failed
instrumentation call and never panic or fail the consumer. A permanently failed
sink is owned by the writer and stops I/O, not the product.

Run metadata includes version and git SHA (or explicit `unknown` in archives
without a supplied build identity). Every run root also records `data.mode`
(the effective behavior: `report` except package production's `strict` or
`permissive`), `data.requested_mode` (`strict`/`permissive`, always present) and
`data.mode_source` (`default`/`explicit`). Report mode flags have no effect;
these attributes preserve intent without changing stdout or the trace format.
These are existing root data attributes, not new tracing API or format versions.
Level 1 includes all executed essential phases: walk selection/edges/assets,
pack fetch/snapshot/member work, index/SDNA, resolve, hash/copy, publication
and requested durability. Level 2 may add detail, but cannot erase an essential
phase or decision from Level 1. The executable command/scenario contract is
[FLATTRACE-MANIFEST](FLATTRACE-MANIFEST.md). Root start identifies requested
inputs; actual capture identities and final effects/resources are root-owned
`root.summary` points. A stat signature is not a content hash. Atomic publication
moments (including temporary ZIP staging) and final product-report effects are
separately labelled; staging is not delivery. Process RSS/CPU counters declare
their scope/units; unavailable trace-byte accounting is explicit (measure the
retained trace externally). Paths are full local detail only after opt-in;
public/default/unset/empty/level-zero execution writes no tracing files.
Fetch spans are rooted in the process run and carry `data.links` with
trace/span IDs and `relationship: submitted_by`, which remains truthful when
the submitting scope has already ended.

## 1.3.1 incomplete listing contract

Missing physical assets/libraries no longer suppress the listing. `list` prints
all discovered existing members and the resolved absent declarations, then keeps
EXIT 2 and the typed `missing_asset`/`missing_library` stderr error. Text marks
absent members with ` [missing]`. Incomplete `--json` output is an object with
`complete:false` and `files`, an owner map of arrays of
`{"path":"MEMBER","missing":true|false}` records. Paths keep the same byte-safe
encoding as successful lists. Missing sequence/glob/UDIM expansions keep the
resolved declaration, not invented frames. Incomplete `--full-info` adds
`complete:false` and uses these member records in `files`, retaining `usages`
and `packed`. Saved override references in selected library IDs retain their
physical source-library declarations in `list`; the overridden source asset
graph is not expanded, and `trace`/`pack` selection is unchanged. Unrelated
unused Library headers remain excluded. Unknown dependencies inside absent
libraries cannot be listed.

This introduces a partial-list protocol only where previous versions emitted
no stdout. Successful lists keep their existing string-array map/full-info
shape and EXIT 0 when all discovered members exist. Correctness fixes can expose
additional dependencies; no existing successful JSON field changes type.
Missing linked IDs now retain the listing with `complete:false`, EXIT 2,
`missing_links` (pack's existing identity/owner-chain shape) and
`missing_link_counts`. All members use the physical `{path,missing}` shape;
an existing library with a missing ID is `missing:false`. Text adds one
`[missing linked ID]` line per diagnostic, with the full identity and chain.
Linked `ID` stubs are followed through their checked Library pointers;
absence is reported only at the endpoint and forwarding cycles stay structural.
Unreadable/corrupt dependency libraries and incomplete pointer subtrees now
retain known results with unknown-site diagnostics. True selected-root input
errors still refuse without a report. Output delivery failure still takes precedence over a
missing-file result. Consumers must retain stdout, stderr and exit together.

Environment flattrace writers now honour `FLATTRACE_LEVEL`: 0 disables trace
I/O, unset/empty defaults to 1, ASCII digits select a `u64`, and invalid/overflow
values use 1 with a run-start note. Run-start `data.level` records the effective
level. In 1.3.1, levels 1 and 2 emitted the same events. The 1.3.3 detail-level
contract above now distinguishes them. Default-level hot pointer/field/array
calls are aggregated per structural owner; explicit level 2 retains per-call
spans. [Trace structure](TRACE-STRUCTURE.md) defines `hot.summary`, inclusive
count/timing semantics, error ownership and consumer migration. Product report
and diagnostic-reference schemas are unchanged.

## 1.4.0 asset family classification and Camera declarations

[Asset family evidence](ASSET-FAMILIES.md) names the explicit code/type pairs
known to have no direct file fields in native Blender 5.2.1 DNA/RNA. Those stop
emitting `unknown_asset_code`; genuinely unknown codes retain that warning.
ID/node/library edges remain independently traced. Camera (CA) is recognized
explicitly, not silently placed in that set: old DNA without custom_filepath
has no declaration; modern EXTERNAL=1 emits nonempty custom_filepath usage with
normal field provenance and resolution, additive `camera_type` and `active`.
The required saved declaration remains required with `active:false` (type is
not CUSTOM=3). Pack-info includes those facts. INTERNAL=0 follows custom_shader
Text through the existing checked Text handling; nonempty historical camera
paths add informational `embedded_camera_filepath`, not a missing dependency.
Unknown/malformed mode is UNKNOWN and retains a known path fact plus typed site
error; strict pack refuses. Literal Camera.custom_filepath char arrays join the
expected-old guarded path whitelist; no pointer/write admission is added.
Numeric schema 1 and existing fields and exit meanings remain. Existing
warning kinds keep their meanings; `embedded_camera_filepath` is additive.
`Usage` adds optional `camera_type`/`active` Rust fields; non-camera JSON is
unchanged. Native reopen/path-census evidence is not render/evaluation proof.

## 1.4.0 per-file partial fact reports (#25)

`blocks --address VALUE` now accepts the documented separate argument as well
as `--address=VALUE`, for decimal and `0x` hexadecimal addresses. Invalid or
absent values remain usage errors.

`facts::source_facts` and `facts::library_facts` retain independent closure facts
when a pointer edge cannot be resolved. That pointer value is an object with
`status:"unknown"` and a typed `error`, never a proved null/address. The error's
site includes block index/address/code/name, struct, readable and hex field path,
saved pointer decimal (or null if unreadable), and expected target type. The
facts object adds `complete:false` and `errors`; successful shapes are unchanged.
Unknown parent ancestry does not produce a `root` claim, including descendants
of that unknown site. CLI incomplete schema-1 fact reports also expose
`complete:false` and `errors` at top level, are delivered before exit 2, and use
additive terminal kind `incomplete_facts` with a structured trace suggestion.
Root/container/unsupported-layout errors can still refuse extraction; these
narrow per-file facts do not replace recursive dependency reporting.

## 1.4.0 single-open read batch (additive)

`read-batch FILE --requests SPEC` executes one ordered plan with one open:
SPEC is a JSON file `{"schema":1,"requests":[...]}`; empty arrays are valid
(zero observations, input check still runs). Each item is exactly one of
`{"op":"inspect"}`, `{"op":"payload","block":N}`,
`{"op":"pointer","block":N,"element"?:M,"path":[...]}`, or
`{"op":"get","block":N,"element"?:M,"path":[...],"raw"?:bool,"byte_array"?:COUNT,"pointer_table"?:COUNT}`
with the exact single-command meanings (ELEMENT default 0, exclusive
raw/byte_array/pointer_table, path as field strings plus nonnegative
flattened indexes). Unknown ops/keys refuse that item as `usage`/`path`;
unknown top-level SPEC keys, wrong schema or missing requests array refuse
the whole batch as `edit_spec`; SPEC/input file failures keep the
single-read `io`/exit-2 semantics.

Stdout is JSONL in plan order plus a terminal summary: success items carry
`{"index","op","ok":true,"report"}` where `report` reserializes to the
identical single-command stdout bytes; failed items carry
`{"index","op","ok":false,"error","diagnostic_ref"}` where `error` equals the single CLI
error object exactly (kind, message, next_command plus catalog prose, no
`diagnostic_ref` inside, no raw source snapshot) and `diagnostic_ref` is the
exact invocation-local sibling (null or `{run_id,failure_id}`).
The summary is schema-1 `{"schema":1,"record_type":"summary","complete",counts,input,input_recheck,input_changed}`
with `counts:{total,observed,succeeded,failed,unobserved}`. Input signature
is captured before open and rechecked after; a mismatch is `input_changed`.
A mid-batch guard (`check_source` failure or per-item
`input_changed`/`source_changed`) stops remaining requests as explicitly
counted unobserved and its original kind outranks the aggregate;
independent item errors otherwise use additive terminal `incomplete_read_batch`,
exit 2. Broken-pipe delivery stops without a summary, exit 2. Empty plans
still prove the input check. No new Python batch API; per-item engines are
only `read_reports::{get,pointer,payload,inspect}` with no arbitrary limits.

## 1.4.0 agent-facing guidance

Pack and pack-sequence schema-1 reports add `native_validation:"not_performed"`
on every returned report (plans, refusals and publications). No existing field
changes meaning: `complete` concerns the selected serialized dependency closure,
not Blender open, evaluation or rendering. Help and pack-info state that boundary;
native checks require compatible Blender with original sources unavailable and
the required render checks. Human incomplete-pack messages distinguish dry-run
(no writes), refusal (no publication) and observed publication, without changing
`incomplete_pack`, exit 2 or the structured `next_command` envelope. Trace's
`incomplete_trace` also remains stable; wording describes retained partial facts,
not an authorization to repair. Summary's render-output question is documented
as an inspect/get recipe; **no summary field was added**.

## 1.4.0 pack trust corrections

Selected local overrides now follow their original reference ID graph in both
trace and list. Native Blender still loads original dependencies after a local
override removes an edge. Pack uses that closure, including nested source
libraries; missing/unprovable sources refuse default-strict publication.
Unrelated Library headers and append-origin metadata are not selected.

Text path ownership is determined by saved `Text.flags` TXT_ISMEM (bit2/value4).
For embedded Texts, nonempty historical filepaths appear as informational
`embedded_text_filepath` records in trace warnings, list stderr/full-info and
pack's trace. They carry owner/block identity, raw path, saved flags and
`status:informational`; they are not asset usages, missing files or foreign-path
relocation blockers. Externally backed Texts retain ordinary dependency behavior.
Dirty state and serialized cached lines do not imply embedded ownership: native
Blender preserves cached lines for external Texts too. Missing or malformed flags
make storage UNKNOWN, not external or embedded. Path facts and independent
pointer/type diagnostics are still reported; the flags error's site carries
`text_storage:unknown`, and strict pack refuses the unprovable obligation. A
proved empty path has no filepath obligation. These are intentional membership/
diagnostic changes, with no terminal error kind or exit-map change.

Native `Text.flags` is not a new-version requirement: official Blender DNA
headers declare `int flags` from at least2.25 through5.0, and5.2.1 uses an
int-backed `eText_Flag` (serialized as int in the native fixture). All define
TXT_ISMEM as value4. The checked primary files, retrieved2026-10-23, are
`https://raw.githubusercontent.com/blender/blender/TAG/source/blender/makesdna/DNA_text_types.h`:

| TAG | Response-body SHA256 |
|---|---|
| v2.25 | `1bbca7f15e1b1f27c01cc655dad907bdb2500e46a1b771909ad3de804da0a980` |
| v2.49b | `9b4d2d8cc8036cb89dc7469c0fa9142348cc977a77eeda450eb9d18d6dab6685` |
| v2.79b | `fa05f02fcaf19616aa786fdacc6ccc958b002d1d74a42f001c7bde6bd8ddf465` |
| v2.80 | `112884c74cfceff1a1000b1cc86a17a871e948428f0448173f6b1910b2e42bc1` |
| v3.6.0 | `f49234c7b0559be2f899802c34d23ae60f6f32cf5011907de39d4c3295ae50ff` |
| v4.5.0 | `ae8bd1e4f5e2e6650a82e7bacbd041d1849a92197ecf877996d618bfd14fbb8b` |
| v5.0.0 | `38b0b1c4bf3955013160e60146a8512d3fa7f802af72ec3d12c85657c5074f7f` |
| v5.2.1 | `9c5814b4e57e787885ca4758f44222927280eb9d4fed0b1cfaed4203a8238cde` |

This is checked historical-boundary coverage, not an assertion that arbitrary
custom SDNA must contain flags. The minimal external-Text fixtures now explicitly
include native int flags=0; their existing pointer/provenance assertions are
unchanged. Truly absent/wrong-type flags have separate UNKNOWN regressions.

Opaque asset prefetch/copy retains metadata and an identity digest rather than
whole bodies. The streamed copied bytes must match that digest and unchanged
ctime-inclusive source signatures/length. Parsed blend container/hash proofs,
exclusive staging and no-replace publication are unchanged.

## 1.3.2 linked-library compatibility

Strict traversal now records `library_null_pointer` warnings for precisely
qualified absent ID-map references in linked libraries: saved header version 301
`NodesModifierData.node_group` with type 57 and the native
`Realize Instances 2.93 Legacy` marker; type-23 copy-transforms
`bTransLikeConstraint.tar`; and active Object-ID `DriverVar.targets[i].id` slots.
The source record must have its exact source-SDNA extent and native ID owner;
constraint DATA additionally requires its matching same-owner typed caller.
These warning sites do not make traversal incomplete. Diagnostics retain the
owner, source block, field, decoded offset, saved address and library chain.
`native_action` annotates the compatibility profile: legacy node-group recovery,
library-override reapplication, or clearing an ordinary missing ID to NULL.
In native readback, the qualified factory-floor override restores its non-NULL
camera-rig target; derived shot overrides retain their original NULL targets.
The `reconstruct_legacy_node_group` label is not a certificate of Blender's
internal phase ordering or fresh group creation. The verified corpus recovers
non-NULL reference-library groups on override owners; a non-override Nodes case
was not natively verified. Reopen in Blender before claiming that outcome for
another file. No missing external file or linked ID is invented for an address
with no serialized identity. Other dangling pointers,
ambiguous or invalid present targets, and structural faults remain refusals.
The selected main file does not receive this library-only tolerance.

Ordinary `list` preserves its member-only stdout shape and emits these structured
warning envelopes on stderr; `--full-info` also adds `warnings`. Pack reports the
same selected-usage warnings in its nested traces. Additive
`library_pointer_clears` records describe the guarded repair plan, including
unselected records in copied libraries. Pack writes NULL into those exact
pointer-width slots after rechecking the expected saved address, role and source
snapshot. It never copies these known dangling addresses into output. Path edits
and pointer clears share checked non-overlapping decoded spans, container
verification and no-replace publication. `rewrites[].edits` still describes path
edits only; `library_pointer_clears` describes pointer changes, and each owning
`files[].output_written` reports actual publication (a dry-run changes nothing).
`files[].rewritten` includes either operation. Pack-info lists the clear count
and records. In the verified corpus, native Blender recovers legacy groups and
reapplies library data on open; Oxblend does not synthesize node graphs or claim
evaluation/render equivalence. This is
not a public pointer-editing API. A known invalid slot in a selected main file
refuses strict publication; permissive preserves the unqualified bytes with an
unknown skipped-item record, never broadens the copied-library clearing profile.

The guarantee covers recognized dependency ID slots, not every serialized
pointer-shaped value. Non-dependency UI and runtime caches are preserved as
stored. An observed example is a stale `TreeStoreElem.id` Outliner cache entry
in a linked library: Blender does not load that library's UI cache, and Oxblend
does not treat it as an asset edge or clear it. Pointer-census diagnostics may
therefore still report unresolved cache/runtime values in a native-equivalent
package; this is not a claim of a literally all-pointer garbage-free file.

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

Trace usages add `resolved_path_hex`, UTF-8-or-null `resolved_path`, and `exists`. The top-level `missing_path_count` counts absent usage sites, not distinct files; `referenced_paths_exist` is true when that count is zero. These are disk observations at JSON serialization time, not an atomic filesystem snapshot: sequence/glob/UDIM existence means a nonempty expansion, not every possible member. Directories count as present. Since 1.4.0, missing discovered files also make `complete:false` and EXIT 2; this intentionally replaces the earlier traversal-only completeness meaning. Nested trace reports and list full-info usages expose the same fields.

Trace/list full-info/pack/edit/rewrite add UTF-8-or-null aliases beside authoritative hex path/name/field/type fields, recursively. Component/path arrays are null if any component is invalid UTF-8. Existing aliases retain their meanings. Binary payload/bit fields remain hex. Ordinary list JSON retains its native-byte map encoding.

Diff adds `difference_count`, equal to the number of reported differences (not proof that unresolved content is equal). CLI returned comparison/read errors also emit the existing `oxblend.diff/v1` schema (not numeric schema 1) with `equal:null`, `complete:false`, typed `errors`, and empty differences/count 0 because no comparison completed. The original typed stderr error and exit 2 are retained. Argument-count/usage refusals remain stderr-only; Rust `diff::compare` still returns `Err` on those underlying failures. After both captured input containers/layouts validate, exact decoded-byte equality bypasses semantic matching and proves `equal:true` even for opaque/unanchored allocations. The equality `matching` text now says "decoded byte equality; no semantic identity reconstruction needed", replacing the earlier physical-byte wording. This is not a Blender evaluation claim.

## EXR attribute values

`exr-header` keeps its existing header/channel metadata and adds attribute
`name`, `type` (UTF-8 when valid) and `decoded_value`. That value is either
`status:decoded` with a typed value, or `status:refused` with a typed refusal
kind and raw hex. Supported types are string, int, float, double, v2i/v2f,
v3i/v3f, box2i/box2f, compression, lineOrder, chlist, m33f, m44f,
rational, timecode, keycode, tiledesc, preview dimensions, chromaticities, envmap,
stringvector and bytes.
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
`header_complete:true` and the per-attribute value status do not verify pixel
samples or manifest semantics. The public `Part.values` map retains attribute
payloads; preview is the exception: only its eight-byte dimension prefix is
retained, not its embedded pixel samples. Preview dimension/declared-length
mismatches refuse locally and expose `raw_prefix_hex`, never purported full
sample bytes.

## 1.4.0 EXR facts and scanning

`header_end` is the offset immediately after the complete header list, including
the final empty header for multipart files. `file_size` is the physical size.
`extent` remains a deprecated 1.x alias **equal to `header_end`**, never file size.
The existing `version`, ordered parts, channels and attribute offset/length
fields remain. Attributes add `decoded` (boolean); unknown types retain raw hex
with `decoded:false`. Required facts are also projected directly per part:
`dataWindow`, `displayWindow`, compression and line-order enum names,
`pixelAspectRatio`, `screenWindowCenter`, `screenWindowWidth`, tiled/multipart/deep
flags and `placeholder_1x1`. Channels retain numeric pixel type/sampling/linear
fields and add readable names, `pixel_type_name`, `pLinear`, `xSampling` and
`ySampling`. The file placeholder flag means **any part** has a 1×1 data window;
it does not explain why that image exists or establish render validity.

Chunk structure is inspected without decoding samples: `chunks_declared` counts
offset slots, `chunks_present` counts valid offsets with readable, geometrically
valid fixed chunk headers (even if a packed payload is cut short), and
`bytes_expected` is a **lower bound**, not an uncompressed byte count or a guessed
compressed size. It includes the complete offset table and mandatory chunk
headers; readable packed sizes strengthen it to the largest referenced chunk
end. Missing compressed sizes cannot be inferred. `truncated` detects incomplete
tables, missing/invalid offsets, duplicate/overlapping chunks, invalid coordinates
and packed payloads extending past EOF. `ok` is structural, not a pixel checksum:
`pixel_integrity_verified` is always false. Deep data returns
`deep_unsupported`, with unknown chunk presence, expected bytes and truncation
(null); no flat-image rules are applied to deep chunks.

`exr-scan` emits JSONL (`schema:1`): unordered `record_type:file` records, then one
`record_type:summary`. Every regular file is inspected, regardless of extension;
directory symlinks are not followed. Nonregular inputs (including explicitly
selected directory symlinks, FIFOs and devices) are skipped, not opened; an empty
regular-file inventory legitimately completes with zero files. Native-byte
absolute paths have authoritative
`absolute_path_hex` plus UTF-8-or-null `absolute_path` and `relative_path` aliases.
Relative paths use the summary's `relative_root`. Repeated exact input paths are
emitted once; aliases/hard links are separate paths. File records include `size`
(alias of `file_size`), status and the same facts as `exr-header` on valid headers.
Malformed headers retain an error and unavailable fields as null/empty.

Statuses distinguish `ok`, `zero_byte`, `not_exr`, `header_truncated`,
`header_only_no_chunks` (complete header but no readable chunk headers),
`chunks_truncated`, and `deep_unsupported`. Additional honest refusal classes
are `header_invalid`, `exr_unsupported`, `io_error` and `input_changed`.
The summary reports files, `counts_by_status`, placeholders, truncated, zero-byte,
non-EXR counts and `groups` by absolute output-tree parent. A
`render/v<digits>/<family>/...` path groups at that family tree; other paths group
by immediate parent. Counts are files, not parts. All statuses are data; exit 0
means inventory and stdout delivery completed, **not all files are healthy**.
Filesystem/identity failures make summary `complete:false` and exit 2. Broken
stdout exits 2 and cannot promise a final summary. Positive `--jobs` selects
parallelism; the default is available CPU parallelism. `--files-from PATH|-`
appends literal paths from a file or stdin, newline-delimited by default;
`-0` uses NUL delimiters. Relative list entries use cwd, empty entries are
ignored. This avoids the operating system's argv-size limit without filtering
or silently narrowing the selection. Results stream with worker-count
backpressure rather than retaining every decoded record. Tables are streamed
through a bounded-by-table standard buffered reader; checking chunk overlap
retains chunk ranges (and tiled-coordinate identities), so structure working
memory still scales with the number of chunks, not merely the header size.
A run/command span covers stdout delivery; `exr.file` spans are level-2 details.

## Checked pointed-array reads (additive)

Stable root methods `BlendFile::byte_array(&self, block_index, element, path,
count) -> Result<Vec<u8>>` and `BlendFile::pointer_table(&self, block_index,
element, path, count) -> Result<Vec<Option<usize>>>` use the same referencing
record/path order as `get`. See [ARRAY-READS.md](ARRAY-READS.md). The CLI exposes
both through `get ... --byte-array COUNT` / `--pointer-table COUNT`, mutually
exclusive with each other and `--raw`; ordinary get output is unchanged.

Only simple declared byte-primitive pointers and struct double pointers are
accepted, respectively. Counts are requested data cardinalities, not inferred
limits. Allocation extents and overflow are checked before result allocation or
array-content reads. Null pointer-table slots retain their position; targets
resolve and qualify through the original source-role namespace. CLI byte values
retain the existing schema-1 bytes shape; table mode adds the requested
`value:{kind:"pointer_table",block_indices:[INDEX,null,...]}` shape. Indices are
reader-local. Source drift is a hard refusal. New flattrace details are
`read.byte_array` and `read.pointer_table`; no error kind or exit is added.

## Query, catalog and explain

The additive `oxblend.query/1`, `oxblend.catalog/1` and `oxblend.explain/1`
contracts are described in [QUERY.md](QUERY.md). Query schema, catalog revision,
product version and source version/subversion are distinct. Every matching saved
record element is selected in physical order, including identifiable linked ID
stubs. Exact raw SDNA projections retain byte hex, decimal integers and float
bits. Source-local block/element references are observation-scoped, not durable
identity. No implicit pointer/external-library traversal occurs for raw fields.

Requested field gaps retain independent rows/outcomes, mark requested coverage
incomplete and exit 2 after complete report delivery. Unavailable outcomes retain
extensible error kinds, exact site context and next steps. Source integrity loss
may refuse the whole batch. Input changes discard the observation. Unknown kinds
and additive envelope/evidence keys remain forward-compatible. No silent
truncation or runtime-default substitution is permitted. Semantic projections
are qualified per representation and preserve saved-vs-evaluated limitations.
The additive `collection.graph` concept (alias `collection_graph`) owns a shared
source-local record/edge graph at `graphs["collection.graph"]`, with `{graph_ref,
root}` references in selected Scene/Collection rows. Membership order, saved
parent/ownership relations and provenance remain explicit; missing bodies/cycles
retain partial facts, never invented defaults. Catalogue revision 2 also adds
machine-readable checked-read descriptors. The transport remains query schema 1.

The Rust `query::{query,catalog,explain}` entries return owned JSON and do not
reopen the provided reader; their schema-1 serialized meanings are shared by CLI
and in-process consumers.

`query()` performs no external IO. `query_with_source()` permits explicit
`collection.graph` expansion and the separate narrowly scoped lookup of a linked
node-tree's actual subtype. The four earlier Scene columns stay root-local.
Both use tracing's one local-first/stub-fallback resolver; required library
contexts are absolute lexical paths, retained once per query. Canonical identity
is separate provenance, never a cache/cycle key. The already-open root is reused
for its exact lexical context. Session callers instead use `library_requests`
(owned requests) and `query_with_libraries` (read-only `LibraryMap` of retained
`LibraryView` snapshots). Failed captures are supplied rather than replayed;
forwarding chains may add requests in a subsequent planning phase. The byte
owner retains source-change latches; a changed source discards the observation.
Library provenance and unavailable sites are explicit, and a missing subtype
retains all saved graph facts under `partial_value`.

Saved socket idnames may become specialized Pixel/Pixel2D classes on native
load, and saved `Undefined[...]` node idnames may become `NodeUndefined`.
The query preserves the saved names and marks saved undefined nodes in evidence.
The `splash-3.4-multilayer-resaved-5.2` qualification case has 16 saved local
override-IDProperty targets which native reopen remaps to linked references.
Those saved references are not rewritten to mimic post-load behavior. Native
qualification asserts exact documented differences, never changes the goldens.
See QUERY.md for case-specific meaning and the independent qualification ledger.

## Sanitized report warning diagnostics (additive v1)

Warning-bearing saved producer reports and direct library results now retain
sanitized warning diagnostics. Only report preview/ZIP derivatives change;
trace/list/pack and other command outputs are unaffected. `failures` retains its
existing meaning. `oxblend.redacted-error-report/v1` and the report derivative
`oxblend.error-source/v1` add an optional `warnings` array with the same closed
source-record projection as failures; the projected `report` also retains its
nonempty `warnings`. Absent/empty warning arrays do not add these fields.
This is additive within v1, not a schema-generation or error-catalog change.

Records preserve producer order and `source_pointer`; warning kind/message
handling, typed coordinates, bundle-local pseudonyms and explicit path/name
opt-ins share the error sanitizer. Published TraceWarning identifiers have a
closed report-only allowlist; unknown identifiers remain pseudonymized, not
new error kinds. A library-null record retains only its explicit decoded slot
offset and saved pointer plus available nested failure owner/field/address.
No source is recaptured, missing coordinates are not inferred, and warnings do
not become failures. Source status is `partial` when retained context exists,
otherwise `unavailable`; warning records may still exist without source context.
Receipt versus retained producer provenance and library/no-CLI binding remain
explicit. Default derivatives contain no raw paths, names or messages.

## Python binding stable surface (additive)

The `oxblend` distribution is a typed adapter over the same engine, not a second
implementation. **The Python API declared here is stable from its first
release.** A public name, signature, documented report field or stated behaviour
is not removed or repurposed in a later release of the same major version;
breaking one requires a **major-version bump**. The crate and wheel share the
1.4.1 release version; the first Python release is stable, with no development
or `0.x` exemption from compatibility for callers. Additive engine fields, new optional keywords and new names remain
compatible.

**Stable names and behaviour.** `oxblend.open`, `open_many`, `File` (all read and
write methods, including `root_paths`, `memory`, `closed` and
`open_diagnostics`), `Record` and every declared report class
(`TraceReport`, `ListReport`, `MultiRootReport`, `PackReport`, `QueryReport`,
`CatalogReport`, `ExplainReport`, `InspectReport`, `ValueReport`,
`PointerReport`, `FactsReport`, `CensusReport`, `OverrideReport`, `DiffReport`,
`EditReport`, `ExtractReport`, `NodeTreeReport`, `ExrHeaderReport`,
`PixelsReport`, `TextReport`, `ScanReport`, `ReportPreview`, `ReportCreated`,
`ReportDiscarded`), every module-level operation function, `FieldQuery`,
`FieldValue`, `Float32Bits`, `Header`, `Summary`, `SceneSummary`, `Fps`,
`Diagnostic`, `HostPath`, `JSONValue`, `Memory`, `Durability`, `Roots`,
`__version__`, `engine_version`, and the generated error classes plus
`ERROR_TYPES`.

- **Reports are `Mapping` values, not a frozen schema.** The typed accessors and
  the documented field meanings are stable; additive engine fields remain
  readable by key and appear in `.to_json()`. Report objects are immutable:
  `Record` subclasses declare `__slots__ = ()`, and the non-`Record` reports
  `TextReport` and `ScanReport` are frozen dataclasses with slots. Removing or repurposing a
  documented field is a breaking change and follows the JSON-generation rules
  above.
- **`.to_json()` is the CLI projection.** For a report whose command has JSON
  output, `.to_json()` is exactly that value (owned `dict`/`list`/scalars, never
  encoded text). `TextReport.to_json()` is the exact CLI text for `blocks`, which
  has no JSON CLI schema to equal; `ScanReport.to_json()` is the EXR JSONL record
  sequence with the summary record last.
- **Diagnostics are separate and retained.** Engine/binding operation diagnostics
  stay on `.diagnostics` (and on `error.diagnostics`). Only the binding-only
  `operation_diagnostics` key is removed from `.to_json()`, and never from the
  report object.
- **Partial work raises the reported kind, and `allow_incomplete` is delivery
  only.** An operation that can be partial raises the catalog error matching
  what the engine actually reported — not always an `Incomplete*Error`:
  `list_dependencies` preserves `missing_asset`, `missing_library`,
  `pattern_members_unknown` or `incomplete_trace`, and `extract` raises
  `ExtractRefusalError`; `pack`, `query`, `overrides`, `facts`, `trace`, `diff`,
  `edit` and `exr_scan` use their `Incomplete*Error`. The full report is on
  `.report`. `allow_incomplete=True` returns that report instead of raising for
  incomplete coverage, but never suppresses `input_changed`, which always raises
  `SourceChangedError`. Engine strictness, publication and completeness fields
  are unaffected; an incomplete report stays incomplete.
- **Errors are one unique class per catalog kind.** Every
  `docs/error-catalog.json` code maps to exactly one class and no two codes share
  a class; the map additionally carries the native `closed_file` lifecycle kind
  (`ClosedFileError`). `input_changed` keeps the original `SourceChangedError`;
  `source_changed` is `OxblendSourceChangedError` because the base name is
  reserved; builtin name collisions are prefixed `Oxblend`; unknown kinds stay
  `OxblendError`.
  Errors expose `kind`, `message`, `operation`, `exit_code`, `report`,
  `diagnostics`, `diagnostic_ref`, `effects`, the catalog remedy fields and
  `next_command` (`None` when there is no real suggestion). Messages are not a
  contract; `kind` and class are.
- **Effects and publication are reported, never assumed.** `effects="unknown"`
  after an interrupted or hard failure is an honest statement; writers never
  retry or clean up. Writer output is new-path-only and no-replace; durability is
  reported per operation.
- **Captured roots and latched sources.** A `File` owns one captured Session and
  never silently reopens. Changed inputs refuse as `input_changed`. Multi-root
  work requires `open_many(paths)` (or a single `File` it created); separately
  captured arenas are refused with a typed `UsageError`, never merged.
- **Release-aligned version.** The binding and engine ship as 1.4.1, with the
  version declared in `bindings/python/Cargo.toml`; `engine_version` names the
  engine it wraps. Because the API is stable from its first release, a breaking
  change needs a major
  bump rather than a pre-1.0 bump.
- **Excluded from parity.** `skill`, `help`, `--help`/`-h` and `--version`/`-V`
  are CLI presentation. `python -m oxblend skill print` remains available offline.

**Invocation identity and direct diagnostic export.** `diagnostic_ref` is null
for library calls; the CLI ledger is CLI-only. Parity asserts that the CLI value
is null or exactly `{run_id,failure_id}`, and that Python preserves native null;
it does not silently drop the field or invent an invocation reference. Every
other report field is compared exactly apart from separately validated run IDs.

`OxblendError.source_snapshot: SourceDiagnostic | None` is stable owned source
evidence, populated by instrumented checks and preserved through Error clones.
`SourceDiagnostic`, `SourceContext` and `SourcePropagation` are typed immutable
mappings; missing instrumentation remains None. Raw snapshots include private
local paths/messages and are not sharing artifacts. Direct `report(error, output)`
and warning-result export use the same closed `report_privacy` handling as CLI
reports. They state library-unbound evidence, never verified CLI invocation
binding. Structural diagnostics are compared exactly; binding/provenance fields
are documented expected differences and are retained rather than normalized
away. Preview/review/create and no-replace publication remain unchanged.

A scoped, reentrant per-call diagnostic owner propagated to workers is the
explicit future shape for ledger-backed Python references; it is not supplied
by Error-owned snapshots or by this release.

**Not claimed.** Blender open, evaluation or render verification of any result;
support for platforms other than Linux x86_64 with CPython 3.11+ (abi3); a
free-threaded CPython qualification (the GIL is released around engine work, but
free-threaded support is claimed only when a release actually ran it); implicit
pointer following; or automatic recovery from a changed/closed source.
