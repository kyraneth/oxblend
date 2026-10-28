# Oxblend usage

<!-- generated-diagnosis -->
**Diagnose does not mean repair.** `inspect`, `get`, `pointer` and `payload` describe saved bytes; they do not establish the intended scene. If trace or pack reports an invalid/unknown pointer, explain the site and stop. Do not guess a replacement pointer, null it, or bypass an Oxblend refusal with Python, hex editing or another writer unless the user explicitly authorizes a separate repair plan. Keep the original untouched. A clean trace or `complete:true` after a change proves only this command's serialized contract, not that the change is correct or that Blender can open or render it. Prefer a known-good save or repair in compatible Blender, then verify the new file natively before recommending delivery.
<!-- /generated-diagnosis -->

Commands are case-sensitive. `oxblend --help` lists all commands; `oxblend COMMAND --help` lists that command's arguments and flags; `--version` prints the package version. Paths accept native Unix bytes. Flags, integers and JSON field paths must be UTF-8. Retain stdout, stderr and the exit status together. `oxblend skill` (or `oxblend skill print`) prints the version-matched bundled guide offline; no checkout or separate docs installation is required.

## Selected-ID extraction (experimental)

```sh
oxblend extract source.blend --id NTCompositor --clear CompositorNodeRLayers.scene --output tree-new.blend --json
oxblend extract source.blend --block 123 --id 'IMViewer Node' --output library-new.blend
```

Use exact saved ID names including their two-byte prefix. Repeat `--id` or use
`--block INDEX` from this exact save to disambiguate duplicate names. This copies
the selected IDs and reachable owned DATA allocations at original addresses,
using the source header, version and DNA; output is uncompressed. Cross-owner
DATA references require explicit selection of the owning ID too. Report names
retain `name_hex` plus UTF-8-or-null `name`; use block selection for byte names. Typed `void*`
targets use their own BHead/SDNA, not an invented storage discriminator. Typed
pointer arrays use the declaring element type. Unknown raw roles refuse.

Other Main IDs are **not** imported implicitly: select them too, or explicitly
`--clear Struct.field`. The narrow RNA alias `CompositorNodeRLayers.scene`
clears only Render Layers nodes' saved `bNode.id` slot. `--clear bNode.id` is
broader and affects all node ID slots; review it carefully. Embedded-ID root
promotion and linked-ID detachment are not qualified. Runtime sanitation uses
the existing qualified discard profile, with two native-proved 5.2 cache cases;
every applied reset and structural fixup is reported separately.

GLOB (without current context pointers), DNA1 and ENDB are kept; REND, TEST,
USER and UI IDs are omitted unless explicitly selected through supported ID
selection. This matches observed 5.2.1 partial-library writes. No source file is
modified. Existing destinations refuse, including aliases of the input.
`--durability none|end|each-file` uses the existing atomic no-replace publisher.
Retain stdout/stderr/exit: exit 0 requires `complete:true`; closure refusals
return a schema-1 report and exit 2. Inspect `selected_ids`, block counts,
`applied_policies`, `runtime_sanitation`, `structural_fixups`, `refusals`,
`errors`, `unused_clear_rules`, publication and durability separately. Success proves only the
serialized contract; reopen and append in compatible native Blender before use.

## Local flattrace diagnostics

```sh
FLATTRACE_DIR=/path/to/local-traces oxblend summary scene.blend
# Optional: FLATTRACE_TRACEPARENT=00-<32hex-trace>-<16hex-parent>-<2hex-flags>
```

Nonempty `FLATTRACE_DIR` creates a fresh per-run JSONL file, never overwrites.
Unset/empty performs no trace I/O. `FLATTRACE_LEVEL=0` also disables all trace I/O,
even with a directory set. Unset/empty level defaults to 1; trimmed ASCII digits
select a `u64` level, and invalid/overflow values fall back to 1 with a run note.
Run-start `data.level` records the effective level. Level 1 retains structural
walk/file/member phases and aggregates inner pointer/field/array calls into
`hot.summary` records at owner close. Explicit `FLATTRACE_LEVEL=2` restores
per-call spans (potentially very large) and adds EXR worker detail. See
[trace structure and consumer migration](TRACE-STRUCTURE.md).
Studio launchers can set it persistently;
the public binary remains opt-in. Run data includes sanitized argv, tool
version, binary SHA-256 when readable, OS, architecture and hostname; unknown
reported facts are null. It never dumps the environment. Invalid traceparent
disables tracing with a warning rather than changing command results.

The tool-owned [event list](../flattrace-events.json) describes spans and data.
CLI spans include argument parsing and complete stdout delivery; lower-level
spans distinguish file open, index/schema, pack plan/copy/repath, stage writes
and publication. `reader.stats` and `copy.stats` retain measured transport facts.
Outcomes distinguish ok, error, refused, partial, cancelled and unknown. Scope abandonment
is unknown, not success; SIGKILL leaves missing ends, never a fabricated cancel
or run completion. Durations are residence milliseconds, not CPU time; do not
sum nested/overlapping spans. Flush is not crash durability.

Writer-call errors warn once and skip that call; they do not discard the run.
A failed sink stops trace I/O without changing stdout
or the command's exit status. The writer holds its trace file and an OS-entropy descriptor;
under an exhausted process FD limit it can reduce product resource headroom.
Command/run `partial` means an incomplete report, not necessarily published
files: inspect operation-span outcomes and publication fields separately.
Raw files contain local paths/hostnames. The
flattrace v1 reference checker/exporter is a separate MIT checkout; set
`PYTHONPATH` to its root (no Python package installation required):
```sh
PYTHONPATH=/path/to/flattrace python3 -m flattrace check --events flattrace-events.json /path/to/traces/*.jsonl
PYTHONPATH=/path/to/flattrace python3 -m flattrace export /path/to/traces/*.jsonl --out /path/to/fresh-export --path PRIVATE_PATH --host PRIVATE_HOST --user PRIVATE_USER
# Every-command qualification (uses the release binary by default):
FLATTRACE_CHECKOUT=/path/to/flattrace python3 scripts/check-flattrace.py
# Or include it in the whole suite:
FLATTRACE_CHECKOUT=/path/to/flattrace scripts/qualify.sh
```
Review the exported bytes and scan private markers before sharing. This generic
flattrace export and Oxblend's closed structural `report --preview` are distinct
reviewed export routes, neither a secret detector. Originals stay local. No
upload, deletion, retention policy, viewer or collector is implied.
Checker/export exit 1 retains unfinished evidence; it is not successful work.
Command coverage requirements deliberately apply to early failures too;
qualification separately verifies successful branches' lower-level spans.
Library users opt in through stable `oxblend::tracing::initialize()` or
`oxblend::tracing::Span::new(name, attributes)`. Initialization is idempotent and
reads the FLATTRACE_* environment once without mutating it. Use
`span.end(oxblend::tracing::Outcome::Ok)` (or Error/Refused/Partial/Cancelled/Unknown)
for the actual per-call result. One process owns one run, ended at exit as
`unknown`: a library cannot infer whether the host application succeeded.
An un-ended guard drops as unknown; kill/abort remains unfinished. See
[the stable tracing contract](STABILITY.md#stable-in-process-local-tracing).
Build metadata contains version and git SHA; source archives without a supplied
build identity explicitly record `unknown`.

## Exit status

- **0:** declared command contract completed, including complete stdout delivery.
- **2:** usage/read/write error, broken pipe or incomplete report.
- **3:** selected input is missing or not a file for list/blocks/pack.
- **5:** explicit pack project path does not exist; nothing written.
- **Signal/other:** external termination or unexpected failure; inspect the destination before retrying.

Hard errors are JSON stderr: `{"schema":1,"error":{"kind":"...","message":"..."}}`. Common refusals add `error.next_command` with structured `argv` (UTF-8 or null) and native-byte `argv_hex`; do not interpolate these arguments into a shell script. Missing/type-mismatched fields and invalid block indices point to `inspect FILE`; usage errors point to command help. Suggestions do not execute anything. Set `FLATTRACE_DIR` for local flattrace v1 diagnostics; unset/empty means no trace I/O. Sink failure warns once without changing command output or exit status. Old phase/read-report variables are removed; reader fallback warnings remain. A command can emit JSON stdout and then exit 2. Library `Ok(report)` can have `complete=false`. With two input arguments, `diff` also emits its v1 incomplete stdout report on returned comparison/read errors; argument-count errors remain stderr-only.

## All commands

| Command | Arguments and contract |
|---|---|
| `summary` | `FILE`: header, block counts, object/material/mesh/image/collection name arrays and every saved Scene, including frame range/step and FPS; missing fields produce typed `warnings`, unavailable Scene values are null; names require UTF-8 |
| `inspect` | `FILE`: complete ordered block index and SDNA layout, byte names in hex |
| `get` | `FILE BLOCK PATH_JSON [ELEMENT] [--raw \| --byte-array COUNT \| --pointer-table COUNT]`: typed value; element defaults to0, char arrays trim at NUL unless raw. The exclusive array modes read one declared byte pointer or struct** using an explicit saved count; preserve bytes/ordered nulls, prove actual allocation bounds and source-role targets. See [ARRAY-READS](ARRAY-READS.md). |
| `pointer` | `FILE BLOCK PATH_JSON [ELEMENT]`: explicit checked target block or null |
| `payload` | `FILE BLOCK`: complete payload in hex; potentially large output |
| `read-batch` | `FILE --requests SPEC`: one open, ordered inspect/payload/get/pointer plan from SPEC `{"schema":1,"requests":[...]}`; JSONL items plus terminal summary; same per-item bytes as singles |
| `source-facts` | `FILE`: narrow per-file source collection/object/library closure; failed pointer edges are unknown at their site, independent facts retained, complete:false/exit 2; unsupported root layouts may still refuse |
| `library-facts` | `FILE`: per-file collection/object closure for discovery; same per-site partial reporting, not cross-file resolution |
| `trace` | `FILE... [--files-from PATH] [-0] [--summary] [--per-file\|--recursive] [--strict\|--permissive\|--non-strict] [--low-memory]`: recursive by default; mode flags are compatibility no-ops; usages, errors, warnings, complete |
| `list` | `FILE... [--files-from PATH] [-0] [--json] [--full-info] [--sha256] [--timing] [--low-memory]`: expanded dependencies grouped by owner, including marked missing declarations; text default, sorted JSON member sets |
| `blocks` | `FILE [--limit N] [--address ADDRESS] [--dump]`: block statistics, largest blocks, pointer referrers and fields; default limit 10, negative slicing, 32-byte dump lines |
| `overrides` | `FILE [--root-only\|--recursive] [--strict\|--permissive\|--non-strict] [--json]`: serialized override records/reference chains; recursive default; mode flags are compatibility no-ops; always JSON |
| `pointer-census` | `FILE [--full]`: whole-file diagnostic pointer sites/states; semantic_verified remains false |
| `diff` | `A.blend B.blend`: diagnostic allocation/content comparison; unresolved or opaque state can prevent an equality claim |
| `edit` | `INPUT --output NEW --spec JSON --mode format-only\|comp\|node-tree [--durability none\|end\|each-file]`: fixed-width schema-2 edits or separate schema-1 structural compositor transactions; see below and [structural profile](NODE-TREE-EDIT.md) |
| `node-tree` | `FILE`: read exact compositor graphs for structural expected-old guards; pinned native 5.2.1 profile, no Blender evaluation |
| `rewrite` | `INPUT --output NEW --edits SPEC [--durability none\|end\|each-file]`: explicit expected-old byte-safe path edits |
| `pack` | `INPUT NEW_TARGET [-p PROJECT] [PACK_OPTIONS]`: directory or case-insensitive `.zip`; project defaults to input parent |
| `pack-sequence` | `INPUT... [--files-from PATH] [-0] -t NEW_TARGET [-p PROJECT] [PACK_OPTIONS]`: deduplicated multi-root pack, hierarchy by default; inferred common input parent or explicit project |
| `exr-header` | `FILE`: decoded OpenEXR per-part header facts and offset-table/chunk structure, without pixel decode |
| `exr-pixels` | `FILE --part N --pass NAME --output NEW.f32`: explicit named pass, raw interleaved f32le plus layout JSON; atomic no-replace output |
| `exr-scan` | `[DIR\|FILES...] [--files-from PATH\|-] [-0] [--jobs N]`: recursive parallel JSONL inventory, plus status/output-tree summary; default jobs is available cores |
| `skill` | `[print]`: print the bundled version-matched Markdown guide without network access or writing/installing files |

Unknown flags are errors. `--target` aliases `-t` for pack-sequence; pack requires its positional target. `--project` aliases `-p`; `--` ends pack option parsing. Pack targets are filesystem paths only: any valid `scheme://` URI target is refused as a usage error (exit 2) before inputs or target are interpreted, also with `--noop`; S3/HTTP(S) transports are not implemented.
`source-facts`/`library-facts` incomplete reports retain typed `errors` and
`facts.blocks` entries. An unknown pointer is `{status:"unknown",error:…}`, not
null; names/fields and saved pointer values identify its site. Unknown parent
ancestry has no proved `root`. CLI exit 2 / `incomplete_facts` follows report
delivery. Successful report shapes are unchanged.

`list` SHA/timing with JSON warns and still reports JSON. `--address` accepts decimal or `0x` hexadecimal. All commands accept `--help`/`-h` without an input; help explains arguments, options and defaults.

## Explicit named EXR pass pixels

```sh
oxblend exr-pixels frame.exr --part 0 --pass ViewLayer.Combined --output beauty.f32 > pixels.json 2> pixels.stderr
pixel_status=$?
# For unprefixed RGB(A), explicitly supply --pass '' (no implicit default).
```

Part is an explicit zero-based header index; pass selects every channel whose
name starts with `NAME.`. An empty pass selects only unprefixed channel names.
The output is **not an EXR or a display image**: it is raw little-endian f32,
interleaved in the JSON report's `channels` order (EXR header order), increasing
x then y from `data_window` minimum. No colour transform, alpha synthesis,
padding or first-pass fallback occurs. Finite half values widen exactly; signaling half NaNs may quiet during
widening. Existing float bits, including nonfinite values, remain data. Selected uint channels, subsampled
parts and deep pixels refuse rather than convert silently. Tiled/multipart
base resolution is supported; mip/rip lower resolutions are not selected.

The own header/structure reader runs first; the pinned pure-Rust `exr` crate
alone decodes pixel compression, serially. Every selected sample must be present
exactly once. Codec errors use `exr_decode`, selection failures `exr_selection`,
unsupported features `exr_unsupported`, and input mutation `input_changed`.
A completely decoded buffer is staged and atomically published to a **new**
output only; no overwrite, including a raced destination. `complete:true` proves
this decode/output contract, not a reference comparison or scene evaluation.
Compare named channels against an independent OpenEXR-based reference, not an
unmatched RGB order: some references reorder RGB(A) ahead of EXR's channel list.
Existing `exr-header` / `exr-scan` remain structural and always report
`pixel_integrity_verified:false`. Cryptomatte semantic analysis is not exposed.

## EXR facts and recursive inventory

```sh
oxblend exr-header image.exr > header.json 2> header.stderr
header_status=$?
FLATTRACE_DIR=traces FLATTRACE_LEVEL=1 oxblend exr-scan renders > scan.jsonl 2> scan.stderr
scan_status=$?
# Large/native-byte path lists, without command-line length limits:
find renders -type f -print0 > files.nul
oxblend exr-scan --files-from files.nul -0 > listed.jsonl 2> listed.stderr
listed_status=$?
# Or select multiple files and caller-chosen positive parallelism:
oxblend exr-scan a.exr b.exr --jobs "$JOBS" > selected.jsonl 2> selected.stderr
selected_status=$?
```

`header_end` ends the complete header list; `file_size` is physical size.
`extent` is a deprecated 1.x alias equal to `header_end`, **not** file size.
Per-part decoded fields include windows, compression enum name, channels
(pixel type, x/y sampling, pLinear), line order, pixel aspect ratio and screen
window. Attributes preserve name/type hex, offset/length and typed values;
unknown types retain raw hex and `decoded:false`. Strings include text/hex,
including application metadata and Cryptomatte manifests (not semantic validation).
Preview metadata exposes dimensions only, not embedded preview samples.

Inspect `status`, `chunks_declared`, `chunks_present`, `bytes_expected`,
`truncated`, and `placeholder_1x1`. `bytes_expected` is a lower bound from table
size, mandatory chunk headers and readable packed sizes, not an estimate of
missing compressed bytes. Deep files are `deep_unsupported` with unknown chunk
integrity. A 1×1 flag means any part's data window is 1×1; it is not a diagnosis.
`pixel_integrity_verified` always stays false.

The scan reads all regular files, not just `.exr`, without following directory
symlinks. Records stream in completion order with absolute/relative paths,
physical size and facts; the final summary counts statuses, placeholders,
truncated, zero-byte, non-EXRs and output trees. Paths under
`render/v<digits>/<family>/...` group at the family tree; other files group at
their parent. `--files-from PATH` appends newline-delimited paths from a file;
`--files-from -` reads stdin. `-0` selects NUL delimiters for all list inputs,
including native-byte/newline-containing filenames. Empty entries are ignored;
paths are otherwise literal, relative to cwd (not the list's directory).
`--jobs=N`, `--files-from=PATH` and `--` are accepted. A completed scan can exit 0 while
reporting damaged or unsupported files; inspect the summary and every relevant
file status. IO/input-change errors set `complete:false` and exit 2. Broken
stdout exits 2, possibly without a summary. See STABILITY for all status and
JSONL contracts. Traces include a command span and optional level-2 per-file
`exr.file` detail.

## Structural compositor transactions

Use `node-tree FILE` to obtain exact tree block indices and complete topology/
endpoint guards, then `edit --mode node-tree` with its separate schema-1
specification. Native-template constructors, version/type guards, add/remove/
relink, refusal boundaries and native proof instructions are documented in
[NODE-TREE-EDIT.md](NODE-TREE-EDIT.md). This adds no arbitrary setter and does
not change existing schema-2 fixed-width specifications.

## Frame range and FPS in one call

```sh
oxblend summary scene.blend > summary.json 2> summary.stderr
status=$?
```

`scenes[]` preserves saved block order and element order within each block. Each
entry retains `name`, `camera`, `frame_end` and adds `frame_start`, `frame_step`,
and `fps: {numerator, base, effective, base_bits_hex}`. Numerator is the signed
saved `Scene.r.frs_sec`, base is `Scene.r.frs_sec_base` converted exactly from
float32 to a JSON number, and effective is numerator/base computed as float64.
Range start/end/step are `r.sfra`, `r.efra`, `r.frame_step`. All scenes return;
none is implicitly active. Missing leaves are null with `missing_field` warnings.
A zero/nonfinite FPS base produces a `type` warning and null effective FPS;
nonfinite base is null too. Exact saved base bits remain in `base_bits_hex`.
This additive extension retains numeric schema 1 and all previous keys/warnings.
Other malformed/type failures still refuse. No Blender evaluation is implied.

Capture the producer status before filtering saved output. A closed stdout now
returns exit 2 / `broken_pipe`, including help/version and text commands. Shell
pipeline status alone may still hide the producer failure without pipefail.
Published edit/pack effects are not rolled back when stdout delivery fails.

### Saved render output paths

`summary` supplies each Scene's range/FPS, but not render output paths. Save
`oxblend inspect FILE`, then find **every** Scene entry in `blocks[]`: its
`code_hex` is `53430000`. For each element from zero through `count - 1`, run:

```sh
oxblend get FILE BLOCK '["r","pic"]' ELEMENT > output.json 2> output.stderr
status=$?
```

BLOCK is the array index in this exact input's `inspect.blocks`; do not reuse
example/other-save indices. Decode `value.hex` as the saved path bytes. Report
both its saved spelling and resolved path: `//` is relative to the owning blend
file, not the shell cwd. This does not verify native/evaluated output behavior.

## Byte-safe queries and edits

```sh
oxblend inspect scene.blend > index.json
oxblend get scene.blend BLOCK '["id","name"]'
oxblend get scene.blend BLOCK '["loc",0]' 0
oxblend pointer scene.blend BLOCK '["parent"]'
```

`inspect.blocks` is an ordered array: the array index is BLOCK. `code_hex`
contains all four code bytes including NUL padding (Scene: `53430000`, not
`5343`). `address_decimal` is a decimal string; parse it as an integer before
numeric formatting. To obtain a struct name, use the block's `sdna_index` into
`sdna.structs`, then `type_index` into `sdna.types_hex`. Read actual saved records;
do not assume `index`, `code`, `type`, `source_path` or `destination_path` aliases.

BLOCK/ELEMENT are nonnegative indices from the exact input. Never reuse them across another save. Paths are field strings and flattened array indices. Pointer traversal is explicit: obtain the target, then query it. Value JSON tags include signed/unsigned/pointer decimal strings, float32 bits_hex, bytes hex and array items. Hex is the authoritative byte representation. Trace/list full-info/pack/edit/rewrite reports add decoded UTF-8-or-null aliases beside `_hex` name/path/field/type keys (including arrays of path components); invalid UTF-8 is null, not replacement text. Ordinary list JSON keeps its existing byte-safe map encoding. Decode hex as bytes before choosing a filename encoding.

The rewrite specification is a schema-1 object with an `edits` array. Each edit contains `block_index`, `element`, `path` (components with exactly one `field_hex` or `index`), `expected_path_hex` and `replacement_path_hex`. The old value must match and the replacement must fit with its NUL terminator. No unrestricted object setter or pointer rewriting is promised. ID name fields are not editable; only whitelisted filepath fields are admitted by `rewrite`. Use Blender for datablock renames. Typed `edit` uses a schema-2 object with `writes` and `bits` arrays (either may be empty); the CLI requires explicit `--mode format-only|comp`. Each write names `block_index`, `element`, `path`, `sdna_type`, `width`, `expected` and `replacement`. Each `bits` entry instead names `block_index`, `element`, `path`, `sdna_type`, `width`, `expected`, `clear` and `set`. Masks are unsigned integers or unsigned decimal strings; the result is `(old & ~clear) | set`. Masks must fit the scalar integer field and cannot clear and set the same bit. `expected` is the exact typed old value, not a mask. Masked writes require `--mode format-only`; comp mode refuses them. Paths accept strings/indices or explicit field_hex/index components. Typed values contain exactly `kind` and one payload: signed/unsigned use `value` (integer or decimal string); f32/f64 use `bits_hex` (8/16 hex digits); cstring/bytes use `hex`; arrays use `items`. Unlike generic value reports, these are strict edit-input tags. Typed editing requires a little-endian header version of at least 502; compositing profiles are qualified on native 5.2.1. The CLI rejects unrecognized keys and ambiguous values; source parser definitions are the exact specification authority.

No-op edits preserve physical bytes. Changed compressed inputs retain their container family and report re-encoding. Fixed-width edits preserve decoded length and all bytes outside declared write ranges. New-output names cannot alias the input or use reserved `.oxblend-stage-` names. Inputs are read-only; existing outputs are never overwritten. Typed byte edits and path packing have separate layout-based admission. A version stamp alone is not proof of compatibility.

### Single-open read batches

The following worked recipe runs from the repository root with the bundled
native template. Set `OXBLEND` to the candidate packaged executable (or keep
`oxblend` from PATH). For your own file, replace `FILE` and select fields from
its saved schema. The discovery open is separate: its output is needed to plan
the batch. Do not pretend that index-dependent requests were known beforehand.

```sh
OXBLEND=${OXBLEND:-oxblend}
FILE=assets/template-5.2.1.blend
mkdir -p .local-results/read-batch-demo
"$OXBLEND" inspect "$FILE" > .local-results/read-batch-demo/index.json 2> .local-results/read-batch-demo/inspect.stderr
inspect_status=$?
test "$inspect_status" -eq 0 || exit "$inspect_status"
python3 - <<'PY'
import json
from pathlib import Path
out = Path('.local-results/read-batch-demo')
index = json.loads((out / 'index.json').read_text())
sdna = index['sdna']
types = [bytes.fromhex(s).decode('utf-8', 'strict') for s in sdna['types_hex']]
def saved_type(block):
    return types[sdna['structs'][block['sdna_index']]['type_index']]
print('Saved types with blocks:', sorted({saved_type(b) for b in index['blocks']}))
# Two consumers need Object fields. Union their paths once, not two opens.
consumers = [{('id', 'name'), ('loc',)}, {('loc',), ('rot',)}]
paths = sorted(set().union(*consumers))
requests = [{'op': 'inspect'}]
selected = 0
for block_index, block in enumerate(index['blocks']):
    if saved_type(block) != 'Object':
        continue
    selected += 1
    requests.append({'op': 'payload', 'block': block_index})
    for element in range(block['count']):
        for path in paths:
            requests.append({'op': 'get', 'block': block_index,
                             'element': element, 'path': list(path)})
        requests.append({'op': 'pointer', 'block': block_index,
                         'element': element, 'path': ['parent']})
assert selected, 'No saved Object blocks selected; check the actual schema/type spelling'
(out / 'requests.json').write_text(json.dumps({'schema': 1, 'requests': requests}) + '\n')
print('Requests:', len(requests), 'selected Object blocks:', selected)
PY
plan_status=$?
test "$plan_status" -eq 0 || exit "$plan_status"
FLATTRACE_DIR=.local-results/read-batch-demo/traces "$OXBLEND" read-batch "$FILE" --requests .local-results/read-batch-demo/requests.json > .local-results/read-batch-demo/batch.jsonl 2> .local-results/read-batch-demo/batch.stderr
batch_status=$?
python3 - "$batch_status" <<'PY'
import json, sys
from pathlib import Path
rows = [json.loads(line) for line in Path('.local-results/read-batch-demo/batch.jsonl').read_text().splitlines()]
assert rows and rows[-1].get('record_type') == 'summary', 'No terminal record: result is not proven complete'
summary = rows[-1]
counts = summary['counts']
print('exit:', sys.argv[1], 'counts:', counts, 'complete:', summary['complete'],
      'input_changed:', summary['input_changed'])
assert counts['observed'] == counts['succeeded'] + counts['failed']
assert counts['total'] == counts['observed'] + counts['unobserved']
assert counts['observed'] == len(rows) - 1
assert int(sys.argv[1]) == 0 and summary['complete'] is True
assert summary['input_changed'] is False and summary['input_recheck'] == summary['input']
assert counts['failed'] == counts['unobserved'] == 0
PY
```

The request document is exactly `{"schema":1,"requests":[...]}`. Batchable
operations today are **inspect, payload, get and pointer** only. `query`,
`overrides` and `list` are not batch operations. A same-type field union means
one `get` request per unique field path per selected element, all in the same
batch; it does not mean adding an unsupported `fields` key to a request.

Discover saved SDNA types through `blocks[].sdna_index` → `sdna.structs[]` →
`type_index` → `sdna.types_hex[]`, as above. Blender runtime names are not the
serialized names: a light may be saved as `Lamp`, not `Light`, and sky-node
storage as `NodeTexSky`, not `NodeShaderTexSky`. Inspect the actual schema and
field declarations; do not replace names by guesswork. An empty selected map
proves only that your selector found no records, **not** that nothing changed.
Keep discovery and batch tied to the same unchanged input; cross-open discovery
is not an atomic snapshot. If the file changes, discard the plan and re-inspect.

A per-item `usage` error means the request is invalid, not an OOM or an unknown
write outcome. Check the failed item index and its `error`, then repair that
request: `inspect` allows only `op`; `payload` needs `op` and `block`;
`pointer` needs `op`, `block`, `path` and optional `element`; `get` additionally
allows `raw`, `byte_array` or `pointer_table` (mutually exclusive). Indices are
nonnegative integers; paths are field strings/array indexes. Do not send CLI
flags, a `type` selector or a `fields` union as extra keys. The current message
may say only `unknown request field`; inspect the submitted object rather than
assuming that message names the offending key. The engine-message improvement
is separate future work. Invalid top-level document structure can refuse before
any item/terminal output. Independent item errors can still produce later items;
exit 2 and `incomplete_read_batch` are not success. A missing terminal, failed or
unobserved items, or `input_changed:true` must not be called a complete batch.

`requests` is ordered; empty arrays are valid and still prove the input
check. Each item uses the exact single-command meanings (ELEMENT default 0,
exclusive `raw`/`byte_array`/`pointer_table`, path as field strings plus
nonnegative indexes). Stdout is JSONL in plan order plus a terminal summary:
success items carry the identical single-command `report` bytes; failed items
carry the identical single CLI `error` object exactly (kind, message,
`next_command` plus catalog prose, no `diagnostic_ref` inside) plus the exact
invocation-local sibling `diagnostic_ref` (null or `{run_id,failure_id}`, never a raw source snapshot). The summary is schema-1 with
`counts:{total,observed,succeeded,failed,unobserved}`, `input`,
`input_recheck` and `input_changed`. Independent errors exit 2 with additive
`incomplete_read_batch`; a source-change guard stops remaining requests as
explicitly counted unobserved and its original `input_changed`/`source_changed`
kind outranks the aggregate. A broken pipe stops without a summary, exit 2.
Use `inspect` for planning verification first; never reuse another save's
indices. This is the same serialized contract as the singles, not Blender
evaluation.

### Camera shader declarations and non-file families

See [asset family evidence](ASSET-FAMILIES.md). Known non-file code/type pairs
are not unknown assets; their ID/node/library dependencies still get traced.
CA is known but schema-qualified: EXTERNAL custom_mode=1 emits a nonempty
custom_filepath usage, including provenance, resolved path, `camera_type` and
`active` (CUSTOM type=3). It stays required even with `active:false`, because it
is a saved external declaration. Pack-info makes this visible. INTERNAL=0 follows
its checked custom_shader Text using Text's storage rules; stale camera paths
are informational `embedded_camera_filepath`, not dependencies. Unknown/malformed
mode is UNKNOWN and blocks strict pack, never inferred from path spelling.
This is serialized classification, not native shader/render verification.

### Guarded IDProperty string values

Schema 2 optionally accepts `idprop_strings` alongside the required `writes` and
`bits` arrays. Example (derive block/element/name from this exact input):

```json
{"schema":2,"writes":[],"bits":[],"idprop_strings":[{"operation":"idprop-string","block_index":788,"element":0,"path":["data","pointer"],"name_hex":"626c656e6465725f76657273696f6e","expected_hex":"352e332e302d616c706861","replacement_hex":"352e332e302d62657461"}]}
```

Use `edit INPUT --output NEW --spec SPEC --mode format-only` (also supported in
`comp`). This changes a **value**, never an ID name or pointer. Names and values
must be UTF-8 without embedded NUL. The saved property must be type 0, subtype 0,
with the named IDProperty/IDPropertyData fields proved from the file's own SDNA.
Only little-endian 502/503 files qualify; unknown headers/layouts refuse. The
exact property name and old string (including its saved length/terminator) are
guards. `path` must be `["data","pointer"]`; no traversal or arbitrary byte offset
is accepted. The resolved raw DATA allocation must begin at that pointer, belong
to the same local, non-override ID owner, have `1 <= len <= totallen == size`,
and have no other/interior reference. Every typed pointer is checked; opaque
DATA is conservatively scanned for possible pointer-table aliases (false positives
refuse). Replacement plus NUL must fit the existing allocation; larger strings
refuse with `idprop_string` naming the actual capacity. `len` includes NUL and is
updated atomically with the payload; `totallen`, pointer and allocation stay
unchanged. Bytes after the new NUL remain untouched. Conflicting declarations,
including duplicate no-ops, refuse. Existing source-change, declared-diff,
container and no-replace publication guards apply. Native validation remains
separate; structural node-tree admission is unchanged. Raw `get` and `payload`
can inspect storage even where semantic `query` projection is unavailable.

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

## Reports versus production strictness (1.4.0)

**Reports always tell everything; packages refuse unless `--permissive`.**

| Reading (`list`, `trace`, `overrides`) | Producing (`pack`, `pack-sequence`) |
| --- | --- |
| Always retain all provable results and report missing files, missing linked IDs and unknown pointer/library subtrees at their sites. | `--strict` is the default: any missing or unknown required item refuses before creating a directory, member, pack-info or ZIP. |
| `--strict`, `--permissive` and `--non-strict` are accepted compatibility **no-ops**. | `--permissive` (`--non-strict` alias) publishes provable members only, marked **INCOMPLETE**, exit 2, with every skipped item/reason. |

Unknown means unknown, not NULL, absent or a guessed dependency. A damaged edge
stops only its unprovable subtree; independent known sites remain reportable.
Pointer diagnostics retain file, block index/code, struct, field path, saved
pointer value and expected target type. Text list shows `[unknown]` records;
JSON list has `errors` and `problem_counts` without requiring `--full-info`.
Missing identities and files likewise remain visible at the least verbosity.
True selected-root input errors (not a blend, malformed/truncated input, unreadable
root, I/O or usage) can still refuse. An unreadable/corrupt dependency library
is instead an unknown subtree with its path and cause. Input-change guards
and output/write safety remain enforced in both package modes.

Complete list retains its legacy bare owner map (and full-info shape) with no
`mode`; spelling a compatibility flag never changes output shape. Report
envelopes have no mode either. Only pack/pack-sequence JSON has `mode`.
Every flattrace run root records `mode` (`report`, `strict` or `permissive`),
`requested_mode` (`strict`/`permissive`) and `mode_source` (`default`/`explicit`).
A report's requested package mode has no behavioral effect.

Explicit `--relative-only`/`--exclude` narrow the requested package scope; they
are not proof failures. Each excluded entry records the flag/pattern, whether
the file is also missing, and `status:"excluded"`. Unknown pointers and unreadable
libraries remain unprovable regardless of exclusions. Package `complete` means
everything **in scope** was delivered; `self_contained:false` plus `excluded_count`
(in declared usage sites) and the same pack-info facts prevent a scoped complete
package being mistaken for a portable farm package.

## Reading dependency and pack results

An empty `list` map is not proof that everything is embedded. Use `trace` and
read its warnings and coverage, then check physical existence. These are separate
questions: warnings describe observations or unsupported families;
`complete` describes the report's coverage; `exists`/missing records describe
on-disk dependencies. A warning can coexist with `complete:true`, and a successful
`summary` does not establish dependency closure. `missing_link_counts` is a
**map**: its length counts map entries/categories, not missing links. Inspect its
keys and numeric values alongside the actual `missing_links` records.

A built-in application asset can still be a required dependency. For example, a
saved macOS Blender application-bundle `assets/nodes/*.blend` library path may
not exist on Linux. Report it as missing; do not declare it absent-but-safe
because it shipped with Blender on the authoring platform. Locate a compatible
asset and explicitly validate any remapping through the existing workflow.
This guidance adds no resolver behavior and does not prove version compatibility.

For packs, check `planning_complete`, `complete`, missing/error/warning records,
publication fields such as `archive_written`/`output_written`, and durability
separately. `durability:none` is not an fsync guarantee; a delivered archive does
not erase unknown coverage or earlier warnings. Preserve stdout, stderr and exit
together. Activate local diagnostic traces with nonempty `FLATTRACE_DIR`, as in
the batch recipe; traces do not upgrade incomplete output to complete.

## Trace completeness and memory

A trace discovers references. `complete` means the discovered dependency closure has no missing or unknown required items. Each usage adds `resolved_path_hex`, UTF-8-or-null `resolved_path`, and `exists` (on-disk observation when the JSON is produced). For saved tiled Images, the checked `Image.tiles` list declares concrete `<UDIM>`/`<UVTILE>` members. For sequence Images, associated saved node/texture `ImageUser` duration and nonnegative offset declare the full playback span (start and cyclic timing do not narrow that span). These spans are unioned across the image's file and all checked referring linked-ID forwarding branches: a shorter library-local user cannot hide a longer root-file user. When automatic refresh is disabled, the serialized current frame (`ImageUser.framenr`) is required as well—even frame 0 outside the playback span. An unavailable refresh flag/current frame, or a negative unqualified current frame, is UNKNOWN. `expected_members`, `missing_members`, `pattern_status` and `pattern_unknown` describe this evidence. Every missing concrete member makes trace/list/pack incomplete; strict pack publishes nothing and permissive pack lists each missing member in pack-info. An unavailable/unsupported tile list, ImageUser association, numeric suffix, nonpositive duration or negative offset is UNKNOWN (`pattern_members_unknown`), never proof from a nonempty glob. Other non-Image glob/cache declarations retain discovery semantics; directories count as present. Existing pattern matches remain included, but do not establish completeness by themselves. `missing_path_count` counts usage sites with `exists:false`, not unique missing files. `referenced_paths_exist` summarizes that count; none of these fields verifies file contents, every sequence frame, or Blender evaluation. Empty asset paths represent no external reference; populated paths are never silently dropped by that rule. Unknown codes can be warning-only without clearing complete, so inspect warnings. An ID named by a linked stub but absent from an existing library is typed `missing_id` external incompleteness. `trace` and `pack` retain the root, library, exact saved ID name/code and referencing chain; both exit 2; only an explicitly permissive pack publishes available members. Counts describe referencing sites/chains, not unique absent IDs. `list` prints the full known listing for `missing_id`, exits 2, and adds `missing_links` (the same entry shape as pack) and `missing_link_counts` to its `complete:false` envelope. An existing library remains `missing:false`; a missing ID is a separate identity, not a missing physical file. Text emits one `[missing linked ID]` line with ID code/name, source library, root, owner and full referencing owner chain. Name-based library selection prefers qualified local matching IDs; only if none exists does it follow every matching forwarded `ID` stub through its checked Library pointer. A directly pointer-reached stub is always followed. Distinct fallback source libraries are reported as `ambiguous_id`/unknown while every candidate is retained; no arbitrary winner is chosen. Absence is reported only at a real endpoint; source-qualified cycles are structural diagnostics. Since 1.3.1, missing physical assets or libraries make `list` print the full available listing plus missing declarations, then exit 2 with a typed stderr error. Text appends ` [missing]` to each absent member. In this incomplete case, `--json` emits `{"complete":false,"files":{"OWNER":[{"path":"MEMBER","missing":false},{"path":"ABSENT","missing":true}]}}`. Every member has an explicit boolean; missing unexpanded sequence/glob/UDIM declarations retain their resolved pattern. `--full-info` uses the same incomplete `files` records and adds its existing `usages`/`packed` fields. Owner and member paths retain the ordinary list native-byte encoding (`bytes-hex:` and `utf8:` escapes). Successful listings retain exactly the existing map of string arrays (or full-info object), without a new envelope. Saved override references in selected library IDs also retain their physical source-library declarations in `list`, without expanding the overridden source asset graph or changing `trace`/`pack` selection. Unrelated unused Library headers remain excluded. Invalid pointers and unreadable/corrupt dependency libraries retain the known listing with unknown-site errors and counts; only true selected-root input errors refuse without a listing. Use `trace` for per-usage diagnostic details; an incomplete list cannot enumerate unknown contents of an absent library.

Since 1.3.2, specific native-tolerated missing ID-map slots in linked libraries
produce `library_null_pointer` warnings, not incomplete traversal: legacy header-301
`Realize Instances 2.93 Legacy` node modifiers, copy-transforms constraint targets
and active Object driver targets. Diagnostics identify each usage and distinguish
the legacy node-group compatibility profile, saved override reapplication, and
ordinary missing-ID clearing. The legacy label does not prove fresh generation
or native phase ordering; the verified groups are recovered from reference
libraries on override owners. A missing saved slot need not remain NULL after
Blender reapplies its library override.
Ordinary list keeps stdout unchanged and prints every structured warning on
stderr, including unknown asset-code warnings; full-info also includes the same
`warnings`. Verbosity never hides a diagnostic. Structural pointers, ambiguous/present-invalid targets and unqualified missing
slots remain unknown with `complete:false` and exit 2; no repair is inferred. See [STABILITY](STABILITY.md#132-linked-library-compatibility) for exact
qualification and pack's guarded pointer-clearing contract.

A missing/unreadable library can produce an incomplete report; a final input-signature guard can instead return a hard error with no report. Handle both.

Default trace/list retain raw read windows or decoded compressed bytes. Opt-in `--low-memory` uses lazy seekable-zstd frame reads with private unnamed spill storage; gzip/non-seekable zstd use checked full-stream decoding to spill. It trades speed and temporary disk use for lower memory use. A partial read does not certify full-source checksums. `blocks` remains streaming without spill or retained decoded frames. Memory can still grow with indexes and reports; there is no memory ceiling. Whole-file census is diagnostic, not a semantic reconstruction certificate. Read-only pointer profiles do not automatically authorize writes.

## Multi-root dependency reports

```sh
oxblend trace --files-from shots.txt > dependencies.json 2> dependencies.stderr
status=$?
oxblend trace --files-from shots.txt --summary > matrix.txt 2> matrix.stderr
status=$?
oxblend list shot-a.blend shot-b.blend --json --full-info > lists.json 2> lists.stderr
status=$?
oxblend pack-sequence --files-from shots.txt -p project \
  -t package-preview-new --dry-run > plan.json 2> plan.stderr
status=$?
```

Put the four shot paths in `shots.txt`, one per LF-delimited entry. Repeated
`--files-from PATH` (also `--files-from=PATH`) expands in encounter order among
positionals; `-` reads stdin. `-0` switches all list inputs to NUL delimiters
(for paths containing LF). Empty entries are ignored; no trimming, comments,
quoting, escaping or shell expansion occurs. CR, spaces, # and arbitrary native
bytes are literal. LF mode rejects embedded NUL. All relative paths resolve
against invocation cwd, not the list directory. `--` ends option parsing.
Unreadable lists are invocation errors, not silently skipped roots.

Lexical absolute duplicates collapse in first-selected order. Different symlink
root contexts remain separate, because `//` paths resolve against that selected
owner location. One distinct root keeps the legacy report/exit (no new wrapper).
Two or more roots produce `schema:"oxblend.multi-root/v1"`, `operation`,
`complete`, `root_traces` and `cross_reference`. Each root entry retains the exact
single-root trace or null plus `{kind,message}` for a hard error. Multi-list JSON
also has `root_lists`, with unchanged per-root listing values and returned errors.
Missing linked IDs do not suppress healthy listings. `--full-info` affects only
listings; the common cross-reference always uses trace-selected dependencies.

`cross_reference` (`schema:"oxblend.cross-reference/v1"`) contains:
- `assets`: native-byte realpath identities, all resolved spellings, existence,
  regular-file bytes or null, and `used_by` root/chain/usage-index evidence;
- `roots`: lexical root IDs, shared/unique asset IDs, missing file identities,
  exact missing-ID/library records and hard error; `traversal_complete` is the
  legacy-named mirror of current availability-aware `trace.complete`;
- `shared`: IDs used by multiple distinct roots, and `totals`: distinct assets,
  existing/missing, shared/unique, known bytes and unknown-size count.

If filesystem identity/member observations fail, each exact site is retained in
that root's additive `observation_errors` (usage index, resolved path hex, typed
kind/message); its first error also appears in `error`. Independent sites/roots
remain visible, xref is incomplete, and pack refuses planning/publication.
Sequence-directory usages enumerate regular members; an empty expansion retains
one absent declaration. A non-sequence directory is present with unknown bytes.

Sets are sorted/unique. A library chain runs from selected root through linked
libraries to the usage owner; the asset is separate. Sequence/UDIM/glob members
inherit their declaration's usage indices. Empty patterned expansion retains
one absent declaration, not invented intermediate frames. Missing IDs are never
counted as files. Hard-link names and equal file contents stay distinct. Hex is
authoritative; invalid UTF-8 aliases are null. Realpath grouping does not rewrite
saved paths or change pack's lexical plan. Sizes/existence are observations, not
an atomic snapshot, file-integrity proof or native evaluation.

`trace --summary` and default multi-root `list` show all shared rows and root
columns: D direct, L via library, D+L both, ! absent. Root-key paths, chain details,
known bytes and missing file/ID details follow; no rows are truncated. The table
is not a machine protocol. JSON/timing/SHA flags retain their legacy list meaning.

Any root gap makes multi-root report/xref complete false, exit 2, while retaining
all obtainable roots. Pack-sequence adds the same source `cross_reference`,
including dry-runs/refusals; first-root legacy `trace` and existing plan fields
keep their scopes. On hard multi-root failures, `root_traces` includes null/error
entries and planning/publication refuses (even permissive). A missing selected
pack root remains exit 3; invalid explicit project remains stderr-only exit 5.
Exclusions change the plan, not dependency truth, so plan completeness can differ
from xref completeness. Dry-run creates no destination or pack-info. Choose
explicit `-p project` when the inferred common input parent is not the intended
project boundary. Retain exits and inspect every root before using a plan.

## Packing

Pack does not require `/proc`. It uses `/proc/self/fd` only to count occupied
descriptor slots quickly; without it, `fcntl(F_GETFD)` probes the OS descriptor
range under `RLIMIT_NOFILE`. I/O diagnostics identify the operation and path.

Options: `--strict` (default), `--permissive` (`--non-strict` alias), `--exclude GLOB...`/`-e` (repeatable), `--noop`/`--dry-run`/`-n`, `--relative-only`/`-r`, `--compress`/`-c`, `--keep-hierarchy`, `--durability none|end|each-file`, `--memory fast|bounded` (default fast). Short `-nrc` switches may be combined. `--exclude=GLOB`, `--project=PROJECT`, `--target=TARGET` (sequence only), `--durability=VALUE` and `--memory=VALUE` are also accepted. Exclusions match resolved usage paths component-wise and right-anchored. Relative-only leaves absolute references uncopied/unmodified. Dry-run writes nothing. Explicit compression emits seekable zstd; ZIP plus compression is refused. URL upload targets are unsupported. An unknown `--memory` value is a usage refusal naming the expected modes.

All roots/traces/planning validate before writing. Every selected root must lie lexically within the existing project directory. Single-root project members use relative layout; external dependencies use `_outside_project`. Sequence hierarchy strips the leading root separator: `/project/shots/a.blend` becomes `package/project/shots/a.blend`. Paths use lexical spelling, not canonical symlink targets.

With explicit `--permissive`, a plan with missing disk assets/libraries publishes available members, exits 2, and supplies one `missing_asset`/`missing_library` error per resolved path with its referencing block in the JSON report. For both single- and multi-root packs, pack-info states `INCOMPLETE` when physical files or linked IDs are missing. It includes `Missing files: N\n` for nonzero distinct missing physical paths and a `Missing file: PATH\n` line for each. PATH is quoted/escaped decoded UTF-8, or `hex:...` for non-UTF-8 native bytes. Do not ship an incomplete pack; retain the JSON report too. Both forms append missing-link diagnostics: nonzero `Missing libraries: N\n`, then nonzero `Missing linked IDs: N\n`, then one `KIND: RECORD\n` per `missing_links` record in report order. KIND is `missing_library` or `missing_id`; RECORD is the exact compact JSON record with sorted keys, no spacing, authoritative hex and UTF-8-or-null aliases. Zero-count headings are omitted; unrelocated-path warnings follow. Counts are reference sites/chains, not unique absent identities. `planning_complete:true` is honest here; `complete:false` is separate. Strict mode refuses unknown or missing dependency sites before any publication, with the typed cause, catalogue anchor and a hint naming `--permissive`. Permissive mode retains `skipped_items` with exact missing/unknown sites and reasons in both JSON and pack-info; it skips unreadable dependency libraries, never guesses their contents, and preserves unresolved saved bytes rather than zeroing arbitrary pointers. Package JSON has `mode` and `status`. Unsafe output paths, input changes and inability to prove a requested write still enforce their safety guards. Migration from the earlier default partial publication: add `--permissive` explicitly and treat INCOMPLETE/exit 2 as not farm-ready.

Pack clears those precisely qualified copied-library slots to NULL rather than
publishing their garbage addresses. `library_pointer_clears` describes each
expected-old pointer, decoded offset/width, field and native action; per-file
`output_written` remains the publication evidence. Dry-run records only the plan.
The scan includes unselected library records; it never authorizes arbitrary
main-file pointer repairs. Strict refuses unqualified sites; permissive retains
their bytes and marks the affected dependency subtree unknown.
`rewrites[].edits` contains path edits only, while `files[].rewritten` includes
path edits or pointer clears. Pack-info records the clears. Native Blender
reconstructs legacy node groups and reapplies library overrides when opening
the package; no arbitrary pointer
setter or graph synthesis is exposed. Reopen in Blender to validate its linked
data; ordinary missing-file incompleteness remains independent of these warnings.
The safety guarantee concerns recognized dependency ID slots. Non-dependency
UI/runtime caches remain byte-for-byte as stored, including an observed stale
`TreeStoreElem.id` Outliner cache in a linked library whose UI Blender does not
load. A pointer census can still show such unresolved cache/runtime values;
pack does not promise that every pointer-shaped value in a file has a target.

ZIP is delivered: private staging, validated UTF-8 member names, ZIP64 widths, CRC and atomic no-replace outer publication. Images `.jpg/.jpeg/.exr` and `bat-debug.log` STORE; other members DEFLATE. Report member paths are virtual ZIP entries, not filesystem files. `archive_written` and outer durability own publication/persistence. Later IO failure in a directory pack may leave completed members; no package transaction, merge/resume or automatic retry is promised.

`input_path_hex`/`trace` cover first root; `input_paths_hex`/`root_traces` cover the whole sequence. Inspect each publication, missing/error, planning and durability property independently.

## Durability

- `none` (default): complete-file no-replace publication, no sync, no crash persistence claim.
- `end`: Linux syncfs on destination filesystem, including foreign dirty data.
- `each-file`: staged-file fsync before link, plus parent/created-ancestor fsync.

A durability failure after publication can leave `output_written:true` and `complete:false`. Inspect before retrying.

## Pack memory

`--memory fast` (default) CLI pack and pack-sequence use signature-bound selective
raw windows for raw blends, reading only needed raw ranges and retaining no whole
decoded sources on that path. Compressed blends fall back to full decoded capture.
Python Sessions retain captured sources as before. `--memory bounded` additionally
re-reads and re-verifies a single working body on reuse, retaining digests and
parsed indexes instead of decoded sources; an incompatible change latches the
existing `input_changed` refusal. It trades more IO and time for lower peak memory.
Selected workload measurements showed 27–53% lower fast-path heap than the
previous implementation. These are workload observations, not universal memory
or timing guarantees, and not a fresh bounded-versus-fast comparison.
It is not a promised hard RAM ceiling: peak
working storage is roughly the largest source plus indexes, active guards and
owned ranges. Blend prefetch is disabled under bounded; cross-device fallback
copies the verified backing. The JSON report records `"memory":"bounded"`
only; absence of the key means fast. A pack whose requested mode differs from
the Session's opened policy refuses as a usage error naming both modes; retention
is fixed when the Session opens. The Python `open` and `File.pack` calls accept
the same `memory` spelling (`pack` defaults to the Session's opened policy).

## Pack shots from several sequences

Select the shots explicitly, even when they come from different sequence folders.
Shared libraries, textures and caches are deduplicated in the resulting pack.
Choose `-p` as the common project root explicitly: inference from the selected
shot folders may omit the larger project context.

```sh
oxblend pack-sequence project/seq-a/shot-010.blend project/seq-b/shot-020.blend \
  -t package-preview -p project --dry-run > plan.json 2> plan.stderr
status=$?
# Inspect the plan and retain this exit before creating a fresh destination.
oxblend pack-sequence project/seq-a/shot-010.blend project/seq-b/shot-020.blend \
  -t package-new -p project > pack.json 2> pack.stderr
status=$?
```

Dry-run writes nothing; it is not publication. Never merge or resume a partially
written destination. Read `root_traces[]`: each entry pairs an `input_path`
(or authoritative `input_path_hex`) with that root's `trace`. The legacy
top-level `trace` describes only the first root, not the whole selection.
For **each** root inspect `trace.complete`, errors, warnings,
`missing_path_count` and `referenced_paths_exist`; a clean first root cannot
certify another shot. Then judge the whole pack's `planning_complete`,
`complete`, `missing`, `missing_links`, errors, publication and durability.
Exit 2 or any incomplete root is not farm-ready even when files were written.
Reopen the isolated package in compatible Blender before delivery; serialized
closure is not render or every-frame verification.

## Public Rust API

`Session::open_with_memory(path, memory)` opens with an explicit `session::MemoryMode` (`fast` default, `bounded` opt-in); `Session::memory()` reports the effective policy, fixed for the Session lifetime. `PackOptions.memory` selects the same policy for a pack; a mode differing from the Session's opened policy refuses as usage. Pack reports record `"memory":"bounded"` only, never the fast default. `summary::summarize(&BlendFile) -> Result<serde_json::Value>` owns the same summary as the CLI and reuses an already-open reader without reopening it; the owned report outlives the reader. `BlendFile::{open,from_bytes,header,blocks,schema,payload,get,get_pointer,read_decompressed_bytes}` exposes checked serialized reflection. `from_bytes` takes decoded bytes. `TraceOptions::default()` is recursive reporting (its legacy `Strict` enum value is a no-op); `TraceOptions::new(mode,pointers)` and `OverrideOptions::new(scope,pointers)` provide explicit constructors. `EditOptions::new(durability)` and defaults construct edit options. For `PackOptions`, start from default then set public fields. Public enums and stable options/reports are non-exhaustive; include a fallback arm in external enum matches. The read-side Rust API is stable in 1.x; editing/construction modules and `bat_cli` remain explicitly unstable/internal, except named types reachable from stable signatures; see [STABILITY.md](STABILITY.md) for the exact stability and JSON contracts. Generate exact current signatures with `cargo doc --offline --locked --no-deps`.

```rust
let mut options = oxblend::pack::PackOptions::default();
options.project_root = std::path::PathBuf::from("/path/to/project");
let report = oxblend::pack::pack(&["scene.blend".into()], std::path::Path::new("package-new"), &options)?;
if !report.complete { /* inspect missing and errors */ }
```

`diff` first validates both captured input containers/layouts. Exact decoded-byte equality (including raw/gzip/zstd without seek tables) then reports `equal:true`, `complete:true`, `difference_count:0` without requiring semantic identity reconstruction. Otherwise `difference_count` is the length of `differences`; unresolved allocations can still make `equal:null`, which means inconclusive, not different. A returned comparison/read error emits a v1 report with `equal:null`, `complete:false`, `errors` containing its typed kind/message, and an empty `differences` array/count 0 (no completed comparison), then exits 2. This does not change the Rust `diff::compare` returned-error API. Byte equality does not certify a valid evaluated scene.

The additive Rust API `diff::compare_with_options(a, b, &diff::CompareOptions { temp_root })` chooses one frozen scratch directory for both input opens. Legacy `diff::compare`, the CLI and Python resolve the process temp directory (`TMPDIR` on Linux, or the platform default) once per comparison. There is no report spool yet. The root is used for spills, not prevalidated for raw inputs. Decoded spills are unnamed and close with the operation. Captured descriptors prevent path replacement from switching generations; replacement may conservatively refuse if unlinking changes the captured inode's ctime. Detected in-place changes refuse with `input_changed`. Validation and comparison read the inputs in full on equality; no sublinear reading is promised.

Native Blender readback and evaluated/render behavior are separate from these serialized guarantees. See [verification](MAINTAINING.md).

## One-reader query and concept discovery

See [QUERY.md](QUERY.md) for the full byte-safe and partial-result contract.

```sh
oxblend catalog --json
oxblend explain scene.compositor --file scene.blend --json
oxblend explain RenderData --file scene.blend --json
oxblend query scene.blend --type Scene --fields render,view_layers,compositor,id_properties
oxblend query scene.blend --type Object --fields type,id.lib,id.override_library
```

One query opens once and selects every saved element in order. Exact raw SDNA
fields and authored concept aliases share the same query. `requested_coverage`
is separate from rendered correctness; preserve stdout on exit 2 and follow
unavailable outcomes' exact next actions. No runtime defaults or implicit
external reads. In-process `query::query(&file, type_name, &fields)` reuses the
already-open reader and returns owned results. Catalog/explain are also core
APIs; the CLI and bindings do not maintain independent semantic maps.

## Python binding

`pip install oxblend` installs an abi3 (CPython 3.11+) Linux x86_64 wheel of the
same engine. It never spawns this CLI and never starts Blender; reports are the
engine's own values. The full API reference, every command mapping, report field
set and typing notes live in [bindings/python/README.md](../bindings/python/README.md).

```python
import oxblend

with oxblend.open("shot.blend", memory="fast") as blend:
    summary = blend.summary()                     # typed; .to_json() = CLI JSON
    print(summary.scenes[0].frame_end)
    for scene in blend.blocks(code=b"SC"):
        value, = blend.get([oxblend.FieldQuery(scene, 0, (b"r", b"sfra"))])
        print(value.value)
    trace = blend.trace(recursive=True, allow_incomplete=True)
    if isinstance(trace, oxblend.TraceReport) and not trace.complete:
        print([error["kind"] for error in trace.errors])
```

- **Every command has a Python entry point.** `summary`, `query`, `catalog`,
  `explain`, `inspect`, `get`, `pointer`, `payload`, `source-facts`,
  `library-facts`, `trace`, `list`, `blocks`, `overrides`, `pointer-census`,
  `diff`, `edit`, `node-tree`, `rewrite`, `extract`, `pack`, `pack-sequence`,
  `exr-header`, `exr-scan`, `exr-pixels` and the `report` preview/create/discard
  family are callable in-process. `skill`, `help` and `--version` stay CLI
  presentation; `python -m oxblend skill print` still prints the offline guide.
- **Options stay faithful.** `--files-from`/`-0` become explicit iterables (or
  `read_path_list(path, nul=False)`); the legacy trace/list/overrides
  `--strict`/`--permissive`/`--non-strict` switches are no-ops;
  `--sha256`/`--timing` have no report meaning. `pack` keeps strict default,
  permissive opt-in, `--noop`, `-r`, `-c`, `--keep-hierarchy`, `--exclude`,
  `--memory` and `--durability` semantics exactly.
- **Reports are immutable `Record` mappings.** Documented fields are typed
  properties; additive engine fields stay readable by key. `.to_json()` returns
  the exact CLI JSON value. Binding operation diagnostics are preserved as
  `.diagnostics` and removed only from that projection. `Record.raw(field)`/
  `.text(field)` decode the lossless `*_hex` provenance.
- **Incomplete is explicit.** Operations that can be partial raise the catalog
  error for the kind the engine actually reported, with the full report on
  `.report` — not always an `Incomplete*Error`: `list` preserves
  `missing_asset`/`missing_library`/`pattern_members_unknown`/`incomplete_trace`
  and `extract` raises `ExtractRefusalError`. `allow_incomplete=True` returns
  that report instead of raising, except for `input_changed`, which always
  raises. Delivery changes, strictness and publication facts do not.
- **Errors are typed per catalog kind.** One unique class per
  `docs/error-catalog.json` code (`ERROR_TYPES` is that mapping, plus the native
  `closed_file` lifecycle entry), builtin name collisions prefixed `Oxblend`,
  unknown kinds still `OxblendError`.
  `input_changed` is the original `SourceChangedError`; `source_changed` is the
  distinct `OxblendSourceChangedError`. Errors expose `kind`, `message`,
  `operation`, `exit_code`, `report`, `diagnostics`, `diagnostic_ref`, `effects`
  and the catalog remedy fields; match `kind`/class, never the message.
- **Captures are owned, sources are latched.** `File` reuses one captured
  Session (roots and retained libraries), exposes their frozen absolute lexical
  identities as `File.root_paths`, and never silently reopens; a changed input
  always refuses. Multi-root work captures every root together with
  `open_many(paths)`; one captured `File` (including `[file]`) is accepted as
  roots, while separately opened `File` objects are refused with a typed
  `UsageError` naming that remedy. No arenas are merged. Several roots return a
  `MultiRootReport` (`root_traces`, `root_lists`, `cross_reference`).
- **Writers are new-path only.** No overwrite, merge or retry. Durability and
  every effect (`output_written`, `durability_completed`, `archive_written`,
  `missing`, `errors`) is reported; a partial publication is never described as
  complete. Writer output is not Blender-open or render verification.
- **Limits.** Saved state only; no evaluated or rendered result, no implicit
  pointer following. The GIL is released around engine work; free-threaded
  CPython is not claimed unless a release actually qualified it.

<!-- generated-guidance -->
Dependency package status: COMPLETE / INCOMPLETE (see report). “Complete” concerns the selected serialized dependency closure only. Blender open, evaluation and rendering have NOT been verified by this command. Before farm delivery, open the isolated package in compatible Blender with original project sources unavailable; check missing data and run the required render checks. A repaired or byte-edited input needs its own native validation.

`summary` supplies every saved Scene's range and FPS, but this version does not include render output paths. Save `oxblend inspect FILE`, find every `SC` block (ASCII code bytes with NUL padding), and for each element run `oxblend get FILE BLOCK '["r","pic"]' ELEMENT`. `BLOCK` and element count come from this exact input; never reuse the example's indices. Decode `value.hex` as the saved path bytes. `//` is relative to the owning blend file, not the shell's working directory. Report both the saved spelling and its resolution, and label native/evaluated output behavior unverified.

`inspect.blocks` is an ordered array: its array index is BLOCK. `code_hex` includes all four code bytes, including NUL padding; Scene is `53430000`, not `5343`. Addresses are in `address_decimal`; parse decimal strings as integers before numeric formatting. Struct names are obtained from the SDNA type/struct tables. Read an actual saved record before selecting field names; do not assume `index`, `code`, `type`, `source_path` or `destination_path` aliases.

On refusal, retain stdout, stderr and exit status. Run `oxblend help error KIND` for every nested causal kind. Save the complete final schema-1 JSON stderr envelope (not mixed diagnostic prose): it includes an invocation receipt even when tracing is off. `oxblend report --report failure.json --stdout saved.stdout --output bug.zip --preview` verifies saved stdout and the canonical original error-envelope digest. With tracing, pass `--trace TRACE.jsonl`, or explicitly select `--run-id ID` / `--last` within `--trace-dir LOCAL_DIR`. The receipt binds the exact trace/root-span identity; legacy or interrupted evidence remains unverified/unknown. Incomplete stdout delivery stays explicitly partial even when its accepted-byte digest matches. A legacy/unrelated saved report never inherits verification from another trace. Review every returned member and privacy warning, then publish only that snapshot with `report --create PREVIEW.json --preview-sha256 SHA256 --output bug.zip`. Names, paths, raw messages, argv, payload and scene bytes are excluded by default; typed identifiers are bundle-local HMAC-SHA-256 tokens. Never add `--include-names` or `--include-file FILE` without an explicit informed choice; whole-file opt-in discloses all contents as a user-supplied, unverified attachment, not a certified input scene. Only accept a preview obtained directly from your own report command; create publishes caller-reviewed bytes but does not authenticate or re-sanitize a preview+digest supplied by another party. The CLI never reruns the failed command, recaptures source files, launches a browser, uploads or files. Show the fixed public GitHub prefill and reviewed local ZIP to the user; filing and attachment remain manual. Reporting does not repair or validate a scene.

`run LABEL COMMAND [ARGS...]` uses LABEL only for log filenames; COMMAND is still required. Correct: `run overview summary scene.blend`. Incorrect: `run summary scene.blend`. The helper is a shell function, not an executable: put optional launch wrappers around `$OXBLEND` inside the function, not around `run`. Direct `oxblend … >stdout 2>stderr; status=$?` calls are equally valid.

`cargo install oxblend --locked` installs the released crate, not GitHub main. After installation run `oxblend --version`, then `oxblend skill` for its matching offline guide. Use documentation for that exact version. Examples for an unreleased build do not imply that its flags exist in the released CLI. Checkout links stay main-relative until a matching release tag exists.
<!-- /generated-guidance -->

### Warning-bearing report inputs

A saved warning-bearing producer report or Python `report(result, output)`
retains its warnings, not just its completion/error flags. The sanitized report
and `error-source.json` include optional `warnings` arrays in producer order;
errors remain in `failures`. The same closed sanitizer serves CLI and Python:
no raw names, paths or messages by default, and no source recapture. Explicit
library-null slot offsets/saved pointers survive; unavailable context stays
unknown. Empty warning arrays preserve the earlier no-warning derivative shape.
Always review these members before creating the ZIP. Warnings do not certify
Blender compatibility and reporting does not repair a scene.

### Python diagnostic export without a CLI invocation

`OxblendError.source_snapshot` is optional owned source evidence with typed
`SourceDiagnostic`/`SourceContext` accessors. It can contain private paths and
messages: do not share it raw. `diagnostic_ref` is null for library calls because
the invocation ledger is CLI-only. Python does not fabricate a ledger ID.

```python
import oxblend
try:
    oxblend.inspect("scene.blend")
except oxblend.OxblendError as error:
    preview = oxblend.report(error, "bug-new.zip")
    # Review every sanitized member and the privacy/binding fields first.
    # Then call report_create(preview['receipt_path'],
    #                         preview['receipt_sha256'], 'bug-new.zip').
```

The same `report(result, output)` API accepts results with warnings. It uses the
same privacy projector as the CLI and explicitly records library-unbound source
evidence, not a verified CLI invocation. Structural diagnostics are equivalent;
invocation binding/provenance differences remain visible. No upload, repair,
rerun or automatic publication occurs.
