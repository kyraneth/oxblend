# Oxblend usage

Commands are case-sensitive. `oxblend --help` lists all commands; `oxblend COMMAND --help` lists that command's arguments and flags; `--version` prints the package version. Paths accept native Unix bytes. Flags, integers and JSON field paths must be UTF-8. Retain stdout, stderr and the exit status together.

## Exit status

- **0:** declared command contract completed (also a clean exit when a downstream pipe closes).
- **2:** usage/read/write error or incomplete report.
- **3:** selected input is missing or not a file for list/blocks/pack.
- **5:** explicit pack project path does not exist; nothing written.
- **Signal/other:** external termination or unexpected failure; inspect the destination before retrying.

Hard errors are JSON stderr: `{"schema":1,"error":{"kind":"...","message":"..."}}`. Query timings also use stderr. A command can emit JSON stdout and then exit 2. Library `Ok(report)` can have `complete=false`. With two input arguments, `diff` also emits its v1 incomplete stdout report on returned comparison/read errors; argument-count errors remain stderr-only.

## All 18 commands

| Command | Arguments and contract |
|---|---|
| `summary` | `FILE`: header, block counts, object/material/mesh/image/collection name arrays and Scene summaries; missing fields produce typed `warnings`, unavailable Scene values are null; names require UTF-8 |
| `inspect` | `FILE`: complete ordered block index and SDNA layout, byte names in hex |
| `get` | `FILE BLOCK PATH_JSON [ELEMENT] [--raw]`: typed field value; element defaults to 0, char arrays trim at first NUL unless raw |
| `pointer` | `FILE BLOCK PATH_JSON [ELEMENT]`: explicit checked target block or null |
| `payload` | `FILE BLOCK`: complete payload in hex; potentially large output |
| `source-facts` | `FILE`: narrow per-file source collection/object/library closure; unsupported linked layouts refuse with a worded error and hint to use trace |
| `library-facts` | `FILE`: per-file collection/object closure for discovery; same linked-layout refusal, not a partial closure report |
| `trace` | `FILE [--per-file\|--recursive] [--strict\|--non-strict] [--low-memory]`: recursive/strict by default; usages, errors, warnings, complete |
| `list` | `FILE [--json] [--full-info] [--sha256] [--timing] [--low-memory]`: existing expanded dependencies grouped by owner; text default, sorted JSON member sets |
| `blocks` | `FILE [--limit N] [--address ADDRESS] [--dump]`: block statistics, largest blocks, pointer referrers and fields; default limit 10, negative slicing, 32-byte dump lines |
| `overrides` | `FILE [--root-only\|--recursive] [--strict\|--non-strict] [--json]`: serialized override records/reference chains; recursive/strict defaults; always JSON |
| `pointer-census` | `FILE [--full]`: whole-file diagnostic pointer sites/states; semantic_verified remains false |
| `diff` | `A.blend B.blend`: diagnostic allocation/content comparison; unresolved or opaque state can prevent an equality claim |
| `edit` | `INPUT --output NEW --spec JSON --mode format-only\|comp [--durability none\|end\|each-file]`: guarded fixed-width typed edits; see below |
| `rewrite` | `INPUT --output NEW --edits SPEC [--durability none\|end\|each-file]`: explicit expected-old byte-safe path edits |
| `pack` | `INPUT NEW_TARGET [-p PROJECT] [PACK_OPTIONS]`: directory or case-insensitive `.zip`; project defaults to input parent |
| `pack-sequence` | `INPUT... -t NEW_TARGET [-p PROJECT] [PACK_OPTIONS]`: deduplicated multi-root pack, hierarchy by default; inferred common input parent or explicit project |
| `exr-header` | `FILE`: OpenEXR header/part/channel metadata without pixel decode |

Unknown flags are errors. `--target` aliases `-t` for pack-sequence; pack requires its positional target. `--project` aliases `-p`; `--` ends pack option parsing.
`list` SHA/timing with JSON warns and still reports JSON. `--address` accepts decimal or `0x` hexadecimal. All commands accept `--help`/`-h` without an input; help explains arguments, options and defaults.

## Byte-safe queries and edits

```sh
oxblend inspect scene.blend > index.json
oxblend get scene.blend BLOCK '["id","name"]'
oxblend get scene.blend BLOCK '["loc",0]' 0
oxblend pointer scene.blend BLOCK '["parent"]'
```

BLOCK/ELEMENT are nonnegative indices from the exact input. Never reuse them across another save. Paths are field strings and flattened array indices. Pointer traversal is explicit: obtain the target, then query it. Value JSON tags include signed/unsigned/pointer decimal strings, float32 bits_hex, bytes hex and array items. Hex is the authoritative byte representation. Trace/list full-info/pack/edit/rewrite reports add decoded UTF-8-or-null aliases beside `_hex` name/path/field/type keys (including arrays of path components); invalid UTF-8 is null, not replacement text. Ordinary list JSON keeps its existing byte-safe map encoding. Decode hex as bytes before choosing a filename encoding.

The rewrite specification is a schema-1 object with an `edits` array. Each edit contains `block_index`, `element`, `path` (components with exactly one `field_hex` or `index`), `expected_path_hex` and `replacement_path_hex`. The old value must match and the replacement must fit with its NUL terminator. No unrestricted object setter or pointer rewriting is promised. ID name fields are not editable; only whitelisted filepath fields are admitted by `rewrite`. Use Blender for datablock renames. Typed `edit` uses a schema-2 object with `writes` and `bits` arrays (either may be empty); the CLI requires explicit `--mode format-only|comp`. Each write names `block_index`, `element`, `path`, `sdna_type`, `width`, `expected` and `replacement`. Each `bits` entry instead names `block_index`, `element`, `path`, `sdna_type`, `width`, `expected`, `clear` and `set`. Masks are unsigned integers or unsigned decimal strings; the result is `(old & ~clear) | set`. Masks must fit the scalar integer field and cannot clear and set the same bit. `expected` is the exact typed old value, not a mask. Masked writes require `--mode format-only`; comp mode refuses them. Paths accept strings/indices or explicit field_hex/index components. Typed values contain exactly `kind` and one payload: signed/unsigned use `value` (integer or decimal string); f32/f64 use `bits_hex` (8/16 hex digits); cstring/bytes use `hex`; arrays use `items`. Unlike generic value reports, these are strict edit-input tags. Typed editing requires a little-endian header version of at least 502; compositing profiles are qualified on native 5.2.1. The CLI rejects unrecognized keys and ambiguous values; source parser definitions are the exact specification authority.

No-op edits preserve physical bytes. Changed compressed inputs retain their container family and report re-encoding. Fixed-width edits preserve decoded length and all bytes outside declared write ranges. New-output names cannot alias the input or use reserved `.oxblend-stage-` names. Inputs are read-only; existing outputs are never overwritten. Typed byte edits and path packing have separate layout-based admission. A version stamp alone is not proof of compatibility.

## Source-change detection

Write-path source-change detection uses size, mtime, ctime and inode identity. A same-size rewrite that restores mtime within one filesystem timestamp tick of capture is not detectable when ctime also remains unchanged. Signatures are not an atomic filesystem snapshot or content hash.

Read-only trace/list/blocks use size/mtime/inode checks; they intentionally ignore ctime-only metadata changes.

### Worked persistent-data bit edit

For a little-endian file with header version >=502, first run `inspect` to find
the exact Scene block, then `get FILE BLOCK '["r","mode"]' 0` and inspect its
SDNA type and width. Do not reuse another file's indices or assume its old flags.
For example, **if** this file has Scene at block 42, element 0, and `r.mode` is
an `int` of width 4 with signed value 0, save this as `persistent-data.json`:

```json
{"schema":2,"writes":[],"bits":[{"block_index":42,"element":0,"path":["r","mode"],"sdna_type":"int","width":4,"expected":{"kind":"signed","value":0},"clear":0,"set":67108864}]}
```

```sh
oxblend edit scene.blend --output persistent-data-new.blend --spec persistent-data.json --mode format-only
oxblend get persistent-data-new.blend 42 '["r","mode"]' 0
```

Bit 26 (67108864) enables persistent render data; this example yields 67108864
and preserves other bits. To disable it, use the newly observed exact old value,
`clear:67108864` and `set:0`. Use a new destination each time. Replace 42 and the
old value/type/width with facts from your input. This proves a serialized flag
change only, not renderer behavior; 4.5-header files are refused even if the
field happens to look similar.

## Trace completeness and memory

A trace discovers references. `complete` means the trace itself completed, not that all referenced files exist. Each usage adds `resolved_path_hex`, UTF-8-or-null `resolved_path`, and `exists` (on-disk observation when the JSON is produced). For sequence, glob and UDIM declarations, `exists` means at least one expanded regular-file member exists; directories count as present. `missing_path_count` counts usage sites with `exists:false`, not unique missing files. `referenced_paths_exist` summarizes that count; none of these fields verifies file contents, every sequence frame, or Blender evaluation. Empty asset paths represent no external reference; populated paths are never silently dropped by that rule. Unknown codes can be warning-only without clearing complete, so inspect warnings. An ID named by a linked stub but absent from an existing library is typed `missing_id` external incompleteness. `trace` and `pack` retain the root, library, exact saved ID name/code and referencing chain; both exit 2, while pack publishes other available members. Counts describe referencing sites/chains, not unique absent IDs. `list` conservatively exits 2 (`incomplete_trace`) with no listing for `missing_id`: use `trace` or `pack` for typed details. Missing physical assets or libraries also make `list` exit 2 with a typed stderr error before printing a listing. Its existing JSON map (or `--full-info` object) is unchanged; use `trace` for all per-usage availability details. A successful empty listing therefore no longer silently hides a missing referenced file.

A missing/unreadable library can produce an incomplete report; a final input-signature guard can instead return a hard error with no report. Handle both.

Default trace/list retain raw read windows or decoded compressed bytes. Opt-in `--low-memory` uses lazy seekable-zstd frame reads with private unnamed spill storage; gzip/non-seekable zstd use checked full-stream decoding to spill. It trades speed and temporary disk use for lower memory use. A partial read does not certify full-source checksums. `blocks` remains streaming without spill or retained decoded frames. Memory can still grow with indexes and reports; there is no memory ceiling. Whole-file census is diagnostic, not a semantic reconstruction certificate. Read-only pointer profiles do not automatically authorize writes.

## Packing

Options: `--exclude GLOB...`/`-e` (repeatable), `--noop`/`--dry-run`/`-n`, `--relative-only`/`-r`, `--compress`/`-c`, `--keep-hierarchy`, `--durability none|end|each-file`. Short `-nrc` switches may be combined. `--exclude=GLOB`, `--project=PROJECT`, `--target=TARGET` (sequence only) and `--durability=VALUE` are also accepted. Exclusions match resolved usage paths component-wise and right-anchored. Relative-only leaves absolute references uncopied/unmodified. Dry-run writes nothing. Explicit compression emits seekable zstd; ZIP plus compression is refused. URL upload targets are unsupported.

All roots/traces/planning validate before writing. Every selected root must lie lexically within the existing project directory. Single-root project members use relative layout; external dependencies use `_outside_project`. Sequence hierarchy strips the leading root separator: `/project/shots/a.blend` becomes `package/project/shots/a.blend`. Paths use lexical spelling, not canonical symlink targets.

A trustworthy plan with missing disk assets/libraries still publishes available members, exits 2, and supplies one `missing_asset`/`missing_library` error per resolved path with its referencing block in the JSON report. For both single- and multi-root packs, pack-info states `INCOMPLETE` when physical files or linked IDs are missing. It includes `Missing files: N\n` for nonzero distinct missing physical paths and a `Missing file: PATH\n` line for each. PATH is quoted/escaped decoded UTF-8, or `hex:...` for non-UTF-8 native bytes. Do not ship an incomplete pack; retain the JSON report too. Both forms append missing-link diagnostics: nonzero `Missing libraries: N\n`, then nonzero `Missing linked IDs: N\n`, then one `KIND: RECORD\n` per `missing_links` record in report order. KIND is `missing_library` or `missing_id`; RECORD is the exact compact JSON record with sorted keys, no spacing, authoritative hex and UTF-8-or-null aliases. Zero-count headings are omitted; unrelocated-path warnings follow. Counts are reference sites/chains, not unique absent identities. `planning_complete:true` is honest here; `complete:false` is separate. An incomplete/untrustworthy trace (including an existing unreadable library) returns `incomplete_trace`, `planning_complete:false` and publishes no files, directory, info or ZIP.

ZIP is delivered: private staging, validated UTF-8 member names, ZIP64 widths, CRC and atomic no-replace outer publication. Images `.jpg/.jpeg/.exr` and `bat-debug.log` STORE; other members DEFLATE. Report member paths are virtual ZIP entries, not filesystem files. `archive_written` and outer durability own publication/persistence. Later IO failure in a directory pack may leave completed members; no package transaction, merge/resume or automatic retry is promised.

`input_path_hex`/`trace` cover first root; `input_paths_hex`/`root_traces` cover the whole sequence. Inspect each publication, missing/error, planning and durability property independently.

## Durability

- `none` (default): complete-file no-replace publication, no sync, no crash persistence claim.
- `end`: Linux syncfs on destination filesystem, including foreign dirty data.
- `each-file`: staged-file fsync before link, plus parent/created-ancestor fsync.

A durability failure after publication can leave `output_written:true` and `complete:false`. Inspect before retrying.

## Public Rust API

`BlendFile::{open,from_bytes,header,blocks,schema,payload,get,get_pointer,read_decompressed_bytes}` exposes checked serialized reflection. `from_bytes` takes decoded bytes. `TraceOptions::default()` is recursive/strict; `TraceOptions::new(mode,pointers)` and `OverrideOptions::new(scope,pointers)` provide explicit constructors. `EditOptions::new(durability)` and defaults construct edit options. For `PackOptions`, start from default then set public fields. Public enums and stable options/reports are non-exhaustive; include a fallback arm in external enum matches. The read-side Rust API is stable in 1.x; editing/construction modules and `bat_cli` remain explicitly unstable/internal, except named types reachable from stable signatures; see [STABILITY.md](STABILITY.md) for the exact stability and JSON contracts. Generate exact current signatures with `cargo doc --offline --locked --no-deps`.

```rust
let mut options = oxblend::pack::PackOptions::default();
options.project_root = std::path::PathBuf::from("/path/to/project");
let report = oxblend::pack::pack(&["scene.blend".into()], std::path::Path::new("package-new"), &options)?;
if !report.complete { /* inspect missing and errors */ }
```

`diff` first validates both input containers/layouts. Byte-identical files then report `equal:true`, `complete:true`, `difference_count:0` without requiring semantic identity reconstruction. Otherwise `difference_count` is the length of `differences`; unresolved allocations can still make `equal:null`, which means inconclusive, not different. A returned comparison/read error emits a v1 report with `equal:null`, `complete:false`, `errors` containing its typed kind/message, and an empty `differences` array/count 0 (no completed comparison), then exits 2. This does not change the Rust `diff::compare` returned-error API. Byte equality does not certify a valid evaluated scene.

Native Blender readback and evaluated/render behavior are separate from these serialized guarantees. See [verification](MAINTAINING.md).
