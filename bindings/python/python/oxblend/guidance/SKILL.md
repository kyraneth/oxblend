---
name: oxblend
description: "Inspects saved Blender files and their dependencies with Oxblend. Use when summarizing a .blend, finding missing files, packing for a render farm, comparing versions, repathing an image or reading EXR header values without starting Blender."
---

# Oxblend 1.4.0

<!-- generated-diagnosis -->
**Diagnose does not mean repair.** `inspect`, `get`, `pointer` and `payload` describe saved bytes; they do not establish the intended scene. If trace or pack reports an invalid/unknown pointer, explain the site and stop. Do not guess a replacement pointer, null it, or bypass an Oxblend refusal with Python, hex editing or another writer unless the user explicitly authorizes a separate repair plan. Keep the original untouched. A clean trace or `complete:true` after a change proves only this command's serialized contract, not that the change is correct or that Blender can open or render it. Prefer a known-good save or repair in compatible Blender, then verify the new file natively before recommending delivery.
<!-- /generated-diagnosis -->

Copyright (c) 2026 Mario Hawat. CC BY 4.0:
https://creativecommons.org/licenses/by/4.0/ . Software is MIT.
This version-matched guide is bundled in the executable: `oxblend skill`
(or `oxblend skill print`). It does not install or modify agent instructions.
`cargo install oxblend --locked` installs the latest released crate, not the
development checkout. Run `oxblend --version` and use its bundled guide first;
unreleased documentation does not imply flags exist in a released binary.
The online [USAGE](../../docs/USAGE.md) and [STABILITY](../../docs/STABILITY.md)
links are checkout-relative (main for unreleased builds), not nonexistent release
tags or installed files.

## Selected-ID partial files

Use `oxblend extract SRC --id NTCompositor --clear CompositorNodeRLayers.scene --output NEW --json` for a same-schema library. Repeat exact prefixed ID names, or use `--block INDEX` from this exact save. External ID references require explicit selection or a reviewed `--clear Struct.field`; unknown pointer roles refuse, never guess. Inspect every policy, sanitation, fixup and refusal site plus publication/durability. A complete report is not native proof: open and append in compatible Blender. See [usage](../../docs/USAGE.md#selected-id-extraction-experimental) for boundaries.

## Frame range and FPS — one summary call
```sh
oxblend summary scene.blend > summary.json 2> summary.stderr
status=$? # Retain this producer exit before reading/filtering summary.json.
```
Reuse a saved report when the input is unchanged; follow-up questions need no reopen.
Read **every** `scenes[]` entry, in saved block/element order: `name`, `camera`,
`frame_start`, `frame_end`, `frame_step` and `fps.{numerator,base,effective}`.
`numerator` is saved `RenderData.frs_sec`; `base` is `frs_sec_base` decoded from
float32; effective FPS = numerator / base. `fps.base_bits_hex` preserves the
exact saved float32 bits. No block index, float decoding or extra `get` call is
needed. A file can contain several Scenes; none is claimed to be active.
Inspect `warnings`: unavailable fields are null, never guessed values. A zero
or nonfinite base has null effective FPS and a typed warning. Nonfinite base is
also null, with its exact bits retained. Saved ranges are not evaluated animation.

Oxblend reads serialized state, not Blender evaluation, render equivalence or
native validation. Use an explicitly supplied executable, or `oxblend` on PATH;
do not silently switch binaries. See the [repository](https://github.com/kyraneth/oxblend).

## Capture evidence first
Work in writable scratch space with read-only inputs and fresh output names.
Retain version, exact argv, stdout, stderr and exit status together. This helper
captures failures even with shell error-exit enabled; use fresh log labels.
It is optional: direct `oxblend … >stdout 2>stderr; status=$?` calls are equally
valid. `run LABEL COMMAND [ARGS...]` uses LABEL only for log filenames; COMMAND
is still required. Correct: `run overview summary scene.blend`. Incorrect:
`run summary scene.blend`. `run` is a shell function, not an executable: put
optional nice/ionice wrappers around `"$OXBLEND"` inside it, never around `run`.

```sh
umask 022
OXBLEND=oxblend # Or the explicitly supplied executable.
run() {
    label=$1; shift
    printf '%s\0' "$OXBLEND" "$@" > "$label.argv"
    if "$OXBLEND" "$@" > "$label.stdout" 2> "$label.stderr"; then
        status=$?
    else
        status=$?
    fi
    printf '%s\n' "$status" > "$label.exit"
    return "$status"
}
run version --version
```
All commands have `COMMAND --help`; `oxblend skill` prints this guide offline.
Examples below use the optional `run` helper; substitute actual inputs.
Capture the producer before filtering: a closed stdout now exits 2 with
`broken_pipe`, not success. Pipelines can hide that producer status unless the
shell uses pipefail. Read/filter saved stdout after the command finishes.

Hex fields encode bytes: decode name/path/code hex before display, not payload or
float-bit hex as text. Readable aliases beside `_hex` fields are UTF-8 or null;
hex stays authoritative. Preserve invalid UTF-8 bytes rather than guessing text.

## Cross-reference several shots before packing

Create `shots.txt` with the four shot paths, one per LF-delimited entry:
```sh
run dependencies trace --files-from shots.txt
run matrix trace --files-from shots.txt --summary
run lists list --files-from shots.txt --json --full-info
run preview pack-sequence --files-from shots.txt -p project \
  -t package-preview-new --dry-run
```
`--files-from -` reads stdin; `-0` selects NUL delimiters for all lists. Empty
entries are ignored; paths are literal native bytes relative to cwd, not the list
folder. CR, spaces and # are not stripped; LF mode rejects NUL. Repeated lists
expand in encounter order. `--` protects leading-dash positional paths.

With multiple distinct lexical roots, inspect `root_traces` and
`cross_reference.roots` individually, including hard null/error roots. Multi-list
also retains `root_lists`. Shared/unique assets use dependency realpath identity,
not contents or basenames; root contexts and pack member paths remain lexical.
Missing IDs are not missing files. All known roots survive another root's failure;
any gap exits 2. One distinct root retains its legacy output. The historical xref
`traversal_complete` field mirrors current availability-aware `trace.complete`.
Pack-sequence adds the same source xref, even before strict refusal/dry-run;
exclusions do not hide source requirements. Use explicit `-p` instead of inferred
common parent when project context matters. A plan is not publication; parseable
JSON, known bytes or a clean first root is not whole-package/native/render proof.

## What is in this file?
```sh
run overview summary scene.blend
run index inspect scene.blend
run sizes blocks scene.blend
```
Start with `summary`: header, block counts, Scene details and object/material/
mesh/image/collection names. Inspect `warnings`; missing fields degrade to warnings
and unavailable Scene values to null, not evidence of an empty scene. Other errors
can still refuse. If summary fails, use `inspect` + `blocks`, not repeated retries.
For a missing detail, find its block in this exact input's ordered index and SDNA,
then use `get FILE BLOCK '["id","name"]'`. Block codes are ASCII bytes with NUL
padding: OB=Object, ME=Mesh, MA=Material, IM=Image, SC=Scene, LI=Library, GR=Collection.
ID names retain two-letter type prefixes; strip only for display. `get` byte values
are `value.hex`. Indices/addresses are local to this exact save, not persistent IDs.
`pointer` resolves a checked target or null; query that target explicitly afterward.
`inspect.blocks` is an ordered array: its array index is BLOCK. `code_hex` includes
all four bytes including NUL padding: Scene is `53430000`, not `5343`.
`address_decimal` is a decimal string; parse it as an integer before numeric
formatting. Struct names come from the SDNA type/struct tables. Do not assume
`index`, `code`, `type`, `source_path` or `destination_path` aliases.

For many reads from one input, plan once from that exact `inspect` index and run
once: save `{"schema":1,"requests":[{"op":"inspect"},{"op":"get","block":B,"path":[...]}]}`
as a SPEC file, then `oxblend read-batch FILE --requests SPEC > batch.jsonl`.
Each JSONL line is one observed item in plan order plus a terminal summary with
counts and the input check; success reports equal single-command bytes and
failed items equal single CLI error objects exactly (no `diagnostic_ref` inside)
with the exact invocation-local sibling `diagnostic_ref` (null or `{run_id,failure_id}`). Independent failures exit 2 with
`incomplete_read_batch`; a source-change guard stops remaining as unobserved.

### Saved render output (recipe, not a summary field)

`summary` supplies all Scene ranges/FPS but does not include render output paths.
Save `oxblend inspect FILE` and find every `blocks[]` entry whose `code_hex` is
`53430000`. For each of its `count` elements run:

```sh
oxblend get FILE BLOCK '["r","pic"]' ELEMENT > output.json 2> output.stderr
status=$?
```

BLOCK is that exact array index; ELEMENT runs from zero to `count - 1`. Never
reuse another file's indices. Decode `value.hex` to get the saved path bytes.
`//` is relative to the owning blend, not the shell cwd. Report the saved spelling
and its resolution; native/evaluated output behavior remains unverified.

## Reports versus packages: choose the operation, not a reporting strictness

**Reports always tell everything; packages refuse unless `--permissive`.**

Pack targets are local directories or ZIP files only. Every URI `scheme://`
target (including `shaman://`, `s3://` and unknown schemes) is refused as usage,
exit 2, even with `--noop`. Shaman/S3 upload transports are excluded from this
release; use a filesystem target, not a URI disguised as a local pathname.

| Reports: `trace`, `list`, `overrides` | Packages: `pack`, `pack-sequence` |
| --- | --- |
| Report all provable results and each missing/unknown site, `complete:false`, exit 2 on any problem. | `--strict` (default): any missing or unknown required item publishes **nothing**. |
| `--strict`, `--permissive`, `--non-strict` are compatibility no-ops. | `--permissive` (`--non-strict` alias): publish provable members only, **INCOMPLETE**, exit 2, every skipped item/reason listed. |

Unknown pointers retain the source file, block index/code, struct, field path,
saved pointer and expected target type. Stop at the unknown subtree; never guess
its contents or treat an unresolvable pointer as proved NULL. Independent sites
still appear. Ordinary list includes missing-ID markers and unknown errors/counts;
`--full-info` only adds detail, never changes whether a problem is reported.
True root input/usage/I/O errors still refuse; corrupt/unreadable dependency
libraries are unknown subtrees. Only package JSON has `mode`; complete list
keeps its exact legacy shape and report envelopes carry no mode.

## Which files are needed or missing?
```sh
run dependencies trace scene.blend
run existing list scene.blend --json
```
Start here, not with `inspect`/`library-facts`. Use `trace` as the declared
reference inventory; `list` expands existing files and marks missing physical declarations. After retaining
both exits, extract saved reports with **python3**, not an assumed `python`:
```sh
python3 - <<'PY'
import json
t = json.load(open("dependencies.stdout"))
print("traversal", t["complete"], "missing sites", t["missing_path_count"])
for u in t["usages"]:
    print(u["resolved_path"] or "hex:" + u["resolved_path_hex"], u["exists"])
# Successful output is the original owner map of string arrays.
# Missing-file output is an envelope with explicit per-member availability.
from pathlib import Path
if Path("existing.stdout").stat().st_size:
    f = json.load(open("existing.stdout"))
    if int(open("existing.exit").read()) == 0:
        members = sorted({p for paths in f.values() for p in paths})
        print("expanded members", len(members), *members, sep="\n")
    else:
        assert f["complete"] is False
        for owner, members in f["files"].items():
            for member in members:
                print(owner, member["path"], "MISSING" if member["missing"] else "present")
else:
    print("list refused: use trace's declared references and errors above")
PY
```
List paths tagged `bytes-hex:` encode native bytes; `utf8:` escapes literal
tag-prefixed names. Normalize these before comparing sets. Include the selected
root explicitly in a complete package inventory; it need not appear in the map.
Trace defaults to recursive reporting. Read errors and warnings even on exit 0.
Saved Image tile lists and associated ImageUser duration/offset define required
pattern members, not just existing glob matches. ImageUser spans are unioned
across the image's file and checked referring linked-ID branches, including
root-file consumers of library-linked images. With automatic refresh disabled,
the saved current frame is also required (even frame0 outside the playback
span); unavailable refresh/current state is UNKNOWN. Inspect `expected_members`,
`missing_members`, `pattern_status` and `pattern_unknown`. A missing tile/frame
makes list/trace/pack incomplete; strict pack publishes nothing; permissive pack
lists each missing member in pack-info. Unsupported or unprovable saved spans
(including negative offsets) are UNKNOWN (`pattern_members_unknown`), never
COMPLETE merely because another tile/frame exists. This is saved playback-span
coverage, not a claim about evaluated scene timing or rendered pixels.
`complete` means no discovered missing/unknown required dependency. Inspect `problem_counts` and each usage's `exists`,
`resolved_path`, and top-level `missing_path_count` / `referenced_paths_exist`.
The count is missing usage sites, not distinct files. `//` resolves relative to the
owning .blend's directory, including inside libraries, not the root or shell cwd.
Use resolved paths (or decode their hex) to stat again when availability matters.
Existence is non-atomic: glob/UDIM/sequence success needs only one expanded
file; directories count as present. UDIM members are spatial **tiles**, not
animation frames. Tile expansion proves no animation-sequence coverage; check
required tiles and frames separately.
Unused saved linked stubs are conservatively traced/packed, not proven render needs.
Since 1.3.1, missing physical assets/libraries print the full discovered listing
and exit 2 with a typed stderr error. Text appends ` [missing]`; incomplete JSON
is `{"complete":false,"files":{"OWNER":[{"path":"MEMBER","missing":false},
{"path":"ABSENT","missing":true}]}}`. Full-info retains `usages`/`packed`, adding
`complete:false` and the same member records. Successful JSON shape is unchanged.
Saved override references in selected library IDs retain their physical source
libraries in `list`, without expanding the overridden source asset graph;
`trace`/`pack` selection and unused-header exclusion are unchanged.
Missing expansions retain their resolved declaration/pattern; absent libraries'
unknown contents cannot be discovered. Missing IDs add `missing_links` and
`missing_link_counts`, using pack's existing identity/chain shape; an existing
library stays physically `missing:false`. Text adds `[missing linked ID]` with
the full ID/library/root/owner chain. Linked `ID` stubs follow their checked
Library pointers until a real endpoint; cycles stay unknown, not false absence.
Name lookup prefers qualified local IDs over same-named stubs; without a local
match it retains all forwarding candidates and explicitly reports distinct-library
ambiguity as unknown. Direct pointer-reached stubs always follow their own source.
Invalid pointers and corrupt dependency libraries give `errors`/`problem_counts`
and `[unknown]` text records even at ordinary verbosity. Use trace for details.
Since 1.3.3, precisely qualified legacy node-group, copy-transforms and Object
driver ID slots in linked libraries can emit `library_null_pointer` warnings
without incomplete traversal. Keep stderr: ordinary list emits structured
warning envelopes there (all warning kinds remain visible without full-info);
full-info also includes the same `warnings`. Trace retains each
site's owner, field, offset, saved pointer and library chain. `native_action`
distinguishes legacy node-group reconstruction, saved override reapplication,
and ordinary missing-ID clearing. Overrides can restore a non-NULL native
target even though the saved slot was unresolvable.
This does not authorize repairing corrupt main-file structure or arbitrary dangling slots;
those dependency sites are reported unknown, not silently treated as compatible.
Only trace/list accept `--low-memory`; it trades CPU/IO/temp disk for lower memory,
not a memory ceiling. Pack accepts `--memory fast|bounded` instead. Fast (default) CLI pack and
pack-sequence use signature-bound selective raw windows for raw blends, reading
only needed raw ranges and retaining no whole decoded sources on that path.
Compressed blends fall back to full decoded capture. Python Sessions retain
captured sources as before. Bounded additionally re-reads and verifies sources,
retaining digests/indexes rather than decoded bodies: lower peak memory, slower
runs, not a hard RAM ceiling. Selected workload measurements showed 27–53% lower fast heap;
this is not a universal memory or timing guarantee. Pack reports record
`"memory":"bounded"` only; absence means fast. `source-facts` answers per-file collection/object/library
closure; `library-facts` answers library collection/object discovery. Neither is a
general scene summary. Failed pointer edges retain independent facts and expose
unknown values at their sites (`complete:false`, typed `errors`, exit 2 /
`incomplete_facts`); unknown parent ancestry has no proved `root`. Successful
shapes are unchanged. Unsupported root layouts can still refuse extraction.
Use trace for cross-file library questions.

Camera CA is known, not unknown by code. Modern `custom_mode=1` (EXTERNAL)
traces nonempty `custom_filepath` as a required saved declaration; usages add
`camera_type` and `active` (true for CUSTOM=3). `active:false` does not exempt the
saved file from strict packing. INTERNAL=0 follows checked `custom_shader` Text
and its storage flags; stale camera paths are informational
`embedded_camera_filepath`, not file requirements. Unknown/malformed mode stays
UNKNOWN and blocks strict pack. Older DNA without that field emits no Camera
path. See [asset-family evidence](../../docs/ASSET-FAMILIES.md); none of this is
native shader evaluation or render proof.

## Pack for a farm and judge completeness
```sh
run plan pack project/scene.blend package-preview -p project --dry-run
run package pack project/scene.blend package-new -p project # strict: nothing on any missing/unknown item
# Diagnostic partial package only, not farm-ready:
run partial pack project/scene.blend package-incomplete -p project --permissive
```
Dry-run writes nothing. Project must exist and lexically contain selected roots.
`--exclude GLOB...` and `--relative-only` narrow what is packed; the package
reports itself as not self-contained. Excluded entries retain the flag/pattern,
`status:"excluded"` and whether the file is also missing. Their absence does not
fail strict scope delivery; unknown pointers/unreadable libraries still do.
Inspect `complete` (in-scope delivery), `self_contained` and `excluded_count`
separately, and the corresponding pack-info facts. Use `pack-sequence` for multiple
roots, `--keep-hierarchy` for layout, or a fresh `.zip` target for an archive.
Add `--memory bounded` for large sequences where peak memory matters more than
speed; an unknown mode, or a mode differing from the Session's opened policy,
refuses as usage.
Never overwrite, merge or resume a partial destination; remote uploads unsupported.
Judge JSON `planning_complete`, `complete`, `errors`, `warnings`, `missing`,
`missing_links`, publication and durability fields separately. Report missing paths
from `missing[].absolute_path` (or authoritative `absolute_path_hex`), not just prose.
Strict (default) refuses before any directory/ZIP/member/info publication on a
missing file, linked ID or unknown pointer/library subtree. Refusals give the
cause, catalogue anchor and `--permissive` hint. Explicit permissive publishes
provable members only, exits 2: **not farm-ready**. JSON `mode`, `status` and
`skipped_items` and `pack-info.txt` disclose INCOMPLETE and every skipped item
with its missing/unknown reason, including unreadable dependency libraries.
Unknown bytes are retained, not arbitrary pointer repair. Migration from older
default partial publication: add `--permissive` explicitly and handle exit 2.
It is still not a native validation certificate. ZIP member paths
are virtual. `output_written`/`archive_written` do not imply completeness or crash
persistence; durability defaults to `none`. Inspect destinations before retrying.
Inspect `library_pointer_clears` too: this is the guarded repair plan for those
specific linked-library slots, including unselected copied records. Where that exact copied-library repair qualifies, pack zeros
the saved garbage pointer; unknown/unqualified pointers are never arbitrarily cleared; Blender recreates legacy
node groups and reapplies saved library overrides on open. Dry-run writes nothing. `rewrites[].edits` remains path-only;
`files[].rewritten` includes path edits or clears, and `output_written` proves
publication. Pack-info also records clears. This is not general pointer editing.
Even a complete pack covers the selected serialized closure, not render validation;
review unrelocated-path warnings and reopen/test in compatible Blender before use.
For native verification, isolate the package from original sources and traverse
known dependency-bearing channels with version-correct Blender APIs, not every
RNA property. Weak historical import/append sources and runtime-added bundled
brush libraries are **not project dependencies**; classify them separately.
Keep external Text declarations and report totals from the normalized final set.
Native reopen alone is not render/evaluation proof.

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

Foreign Windows roots carry `path_resolution_unverified` warnings. Native
loader behavior can differ from Python's `bpy.path.abspath`; use reported
resolved bytes rather than that helper alone. Relative backslashes normalize
as separators. Default pack refuses unsafe/unproven foreign-root relocation. `--relative-only` leaves foreign absolute
paths unchanged: inspect `skipped_absolute_paths` and pack-info, and disclose
those external dependencies. A restricted complete pack is not self-contained.

## Compare two versions
```sh
run comparison diff before.blend after.blend
```
Read `equal` and `complete` first. After container/layout validation, identical
physical bytes prove `equal:true`, `complete:true`, `difference_count:0`.
For other inputs, unresolved/opaque allocations can yield exit 2 / `equal:null`:
**inconclusive**, never "different" or "equal", even with zero differences.
Returned comparison/read errors also emit an incomplete v1 report with typed
`errors` and zero reported differences; argument-count errors remain stderr-only.
Summarize `difference_count` and `differences[]`
by identity; changes are UNASSESSED, not a visual/semantic impact verdict.

## Repath one image: guarded rewrite
First inspect the exact input and get the old filepath. This worked fixture
(`tests/fixtures/dependencies/mesh-material-library.blend` in the repository) has
Image at block 7, element 0; SDNA `name` is a 1024-byte char array:

```sh
run image-index inspect mesh-material-library.blend
run old-path get mesh-material-library.blend 7 '["name"]'
```
Old hex is `2f2f746578747572652e706e67` (`//texture.png`). Save this complete
schema-1 object as `paths.json` to replace it with `//new.png`:

```json
{"schema":1,"edits":[{"block_index":7,"element":0,"path":[{"field_hex":"6e616d65"}],"expected_path_hex":"2f2f746578747572652e706e67","replacement_path_hex":"2f2f6e65772e706e67"}]}
```

```sh
run repath rewrite mesh-material-library.blend --output image-new.blend --edits paths.json
run new-path get image-new.blend 7 '["name"]'
```
For other inputs derive index, field and expected bytes afresh. Rewrite paths use
objects with exactly one `field_hex` or `index`, **not plain strings**. Only
whitelisted literal filepath fields qualify; replacement plus NUL must fit.
An expected-old mismatch refuses publication. Never edit the input or reuse output.
Verify `complete`, errors, publication, durability, and nonzero `write_length`;
a no-op copy is not a changed value. Successful changed **raw-file** edits report
`output_decoder_verified:false`: no fresh full output decoder pass is claimed,
not a failed publication. Changed compressed outputs are decoder-verified and
report true. Independently verify the field and decoded bytes below.
Compare decoded lengths and bytes: outside each reported `offset` through
`offset + write_length` range, bytes must be identical. Offsets are decoded-file
coordinates, never physical compressed offsets. For raw files compare bytes
directly. For compressed inputs, follow STABILITY's decoded-payload procedure:

1. Save `inspect` for input and output; require equal headers, SDNA and complete
   ordered block metadata (`data_offset`, `size`, code, address, SDNA index, count).
2. For **every** block, save `payload FILE BLOCK` on input and output. Hex-decode
   each `value.hex` into bytes and require equal payload lengths. At payload
   index i the decoded offset is `blocks[BLOCK].data_offset + i`; bytes must match
   unless that offset is in a reported write range. Do not compare compressed
   file byte indices with decoded offsets. This checks all decoded payloads,
   not an independent full-container decoder certificate.
3. Get the output field: replacement hex must match.

For the worked image example, first obtain its complete payloads:
```sh
run input-image-payload payload mesh-material-library.blend 7
run output-image-payload payload image-new.blend 7
```
Repeat for every block from the two saved indexes when checking the full file. Finally reopen in compatible Blender, checking the intended value and
untouched sentinels; disclose native reopen/evaluation as unverified if not done.

## Extract an explicit EXR pixel pass

```sh
run beauty exr-pixels frame.exr --part 0 --pass ViewLayer.Combined --output beauty.f32
# For unprefixed RGB(A), use an explicitly empty --pass ''.
```

Choose the exact part index and pass from header facts, never an assumed first
pass. The new output is raw interleaved f32 little-endian, not a display image.
Read JSON `channels`, `data_window`, `width`, `height`, `samples`, `complete` and
`output_written` before consuming it. Channel order follows the EXR header, not
an assumed RGB(A) order. Pixels traverse increasing x then y from the data-window
minimum. There is no colour transform, padding or invented alpha. Deep,
subsampled and selected uint pixels refuse. `exr_decode` / `exr_selection`
failures exit 2; do not substitute another pass or consume a previous output.
An optional independent OpenImageIO/OpenEXR check is available through
`scripts/check-exr-pixels.py` in a source checkout; it is never a product fallback.
Header/structure `pixel_integrity_verified:false` remains unchanged.

## Read EXR facts or scan a tree
```sh
run header exr-header image.exr
run scan exr-scan renders
# Large/native-byte selections without shell argv limits:
find renders -type f -print0 > files.nul
run listed exr-scan --files-from files.nul -0
# Optional caller-selected positive parallelism (default is available cores):
run selected exr-scan a.exr b.exr --jobs "$JOBS"
```
Read `status`, per-part windows/compression/channels/metadata, and
`chunks_declared`, `chunks_present`, `bytes_expected`, `truncated`.
`bytes_expected` is a structural lower bound; missing compressed sizes are not
knowable. `header_end` ends the header list; deprecated `extent` remains equal
to `header_end` in 1.x, never `file_size`. `placeholder_1x1` means any part has a
1×1 data window, not why it exists. `pixel_integrity_verified` is always false;
no samples are decoded, including preview pixels. Deep integrity is unsupported.
Attributes retain offsets/lengths and `decoded_value.status`; unknown types keep
hex and `decoded:false`. Strings (including manifests) are not validated as JSON.

Scan JSONL streams one file record per regular file (not extension-filtered),
then a summary with status counts, placeholders, truncated, zero-byte, non-EXR
counts and family/output-tree groups. Directory symlinks are not followed.
Exit 0 means the inventory completed, not that files are all healthy. Inspect
`counts_by_status` and relevant records; `deep_unsupported` has unknown/null
chunk integrity. Filesystem/input-change errors make `complete:false` and exit 2;
stdout failure can prevent a final summary. See STABILITY's 1.4.0 EXR contract.

## Not supported
No datablock renames, pointer changes, create/delete or structural graph edits;
route these and evaluation/render questions to Blender, not an improvised spec.
Typed `edit` is guarded fixed-width editing, not an arbitrary setter: it requires
little-endian header version >=502 (pre-5.2 refuses `edit_version`). Read USAGE for
schema-2 expected/type/width/mask rules before using it; it does not bypass these limits.

## Exit codes and failures
| Exit | Meaning |
| --- | --- |
| 0 | Declared command contract completed, including complete stdout delivery. |
| 2 | Usage/read/write error, broken pipe or incomplete report. |
| 3 | Selected input is missing or not a file for list/blocks/pack. |
| 5 | Explicit pack project path does not exist; nothing written. |
| Signal/other | External termination or unexpected failure; inspect the destination before retrying. |

JSON/file existence alone proves nothing. Hard errors use schema-1 stderr envelopes
with `error.kind`, worded `error.message` and optional `error.next_command`.
The latter contains structured `argv` (UTF-8 or null) and exact native-byte
`argv_hex`; treat them as arguments, not a shell script. Usage errors point to
command help; field/type/index refusals point to `inspect FILE` (fact-closure
refusals to `trace FILE`). Suggestions never execute automatically.
Set `FLATTRACE_DIR` to retain local flattrace v1 diagnostics; unset/empty is off.
`FLATTRACE_LEVEL=0` also means no trace I/O. Unset/empty defaults to 1; trimmed
ASCII digits select a u64, invalid/overflow uses 1 plus a run note. Run-start
`data.level` records it. Every run root records `data.mode` (`report` for reading,
`strict`/`permissive` for package production), `data.requested_mode`
(`strict`/`permissive`, including ignored compatibility flags), and
`data.mode_source` (`default`/`explicit`). These are ordinary root attributes,
not a trace format change. Level 1 includes per-file dependency walks and a pack
execution summary; level 2 adds walk phases, per-member writes/copies and fetch
causality links. `partial` means a returned incomplete report, not a usable pack;
`cancelled` is an observed cooperative stop, not a missing end after a kill.
`FLATTRACE_TRACEPARENT` joins an enclosing span. Writer-call errors warn and skip
that call; a failed sink stops trace I/O. Neither changes the command result.
A sink failure warns once without
changing the command result. Killed runs stay unfinished. Old phase/read-report
variables are removed. Export and review traces before sharing: they contain paths.
Preserve unknown kinds/added report fields; edit specs reject unknown keys.
Report reproducible bugs to the repository's [GitHub issues](https://github.com/kyraneth/oxblend/issues)
with safe inputs, version, argv, stdout/stderr, exit and completeness/publication
facts. Never attach private scenes or paths without permission.

## Python binding (in-process, same engine)

When the work is already in Python — a pipeline, a DCC script, an agent tool —
use `oxblend` instead of shelling out. It needs no CLI binary, no Blender and no
subprocess, and it returns the engine's own reports.

```python
import oxblend

with oxblend.open("shot.blend") as blend:          # one capture, never reopened
    summary = blend.summary()                      # typed; .to_json() = CLI JSON
    for scene in blend.blocks(code=b"SC"):
        start, end = blend.get([
            oxblend.FieldQuery(scene, 0, (b"r", b"sfra")),
            oxblend.FieldQuery(scene, 0, (b"r", b"efra")),
        ])
        print(start.value, end.value)
```

- Every engine command has a Python equivalent: `summary`, `query`, `catalog`,
  `explain`, `inspect`, `get`, `pointer`, `payload`, `source_facts`,
  `library_facts`, `trace`, `list_dependencies`, `block_report`, `overrides`,
  `pointer_census`, `diff`, `edit`, `node_tree`, `rewrite`, `extract`, `pack`,
  `pack_sequence`, `exr_header`, `exr_scan`, `exr_pixels`, `report`, `report_preview`,
  `report_create`, `report_discard`. `skill`/`help`/`--version` stay CLI text;
  `python -m oxblend skill print` prints this guide offline.
- Reports are immutable `Record` mappings: typed accessors for documented fields,
  key access for everything else. `.to_json()` is the exact CLI JSON;
  `.diagnostics` is separate and always retained. `Record.raw(field)` reads the
  lossless `*_hex` bytes, so non-UTF-8 names survive.
- Incomplete work raises the catalog error for the reported kind with the
  report on `.report` — not always an `Incomplete*Error` (`list` preserves
  `missing_asset`/`missing_library`/`pattern_members_unknown`/`incomplete_trace`;
  `extract` raises `ExtractRefusalError`). `allow_incomplete=True` returns that
  report, except for `input_changed`, which always raises. That changes delivery
  only, not strictness or publication.
- Match `error.kind` or the exception class (`ERROR_TYPES`, one class per
  catalog code; `input_changed` is `SourceChangedError`, `source_changed` is
  `OxblendSourceChangedError`). Never parse `message`. Check `error.effects`:
  `"unknown"` means verify the destination before retrying.
- Instrumented failures retain immutable local `error.source_snapshot` evidence,
  including original kind/context and later propagations, without a CLI ledger.
  Raw snapshots can contain private paths/messages. `report(error_or_result, new_zip)`
  returns a sanitized preview, never publishes automatically; carried partial
  result facts survive without reopening inputs. Review, then use `report_create`.
  Direct evidence is explicitly library-unbound, not a fabricated CLI invocation.
- Multi-root needs one capture: `oxblend.open_many(paths)`, then
  `blend.trace()`, `blend.list_dependencies()` or `blend.pack_sequence(...)`.
  Several roots return a `MultiRootReport` (`root_traces`, `cross_reference`);
  one captured `File` (even `[file]`) is accepted as roots. Passing two
  separately opened `File` objects raises `UsageError` naming `open_many(paths)`;
  arenas are never merged.
- Writers are new-path only: strict pack publishes nothing when a requirement is
  missing, `permissive=True` may publish partial members (labelled INCOMPLETE),
  and every effect plus durability is in the report. Nothing retries or cleans
  up. Python output is not Blender-open or render proof — verify it natively.
- The GIL is released around engine work. Free-threaded CPython is not claimed
  unless the release actually qualified it.

Full reference: [Python API](../../bindings/python/README.md); typed example:
[`full_api.py`](../../bindings/python/examples/full_api.py).

<!-- generated-guidance -->
Dependency package status: COMPLETE / INCOMPLETE (see report). “Complete” concerns the selected serialized dependency closure only. Blender open, evaluation and rendering have NOT been verified by this command. Before farm delivery, open the isolated package in compatible Blender with original project sources unavailable; check missing data and run the required render checks. A repaired or byte-edited input needs its own native validation.

`summary` supplies every saved Scene's range and FPS, but this version does not include render output paths. Save `oxblend inspect FILE`, find every `SC` block (ASCII code bytes with NUL padding), and for each element run `oxblend get FILE BLOCK '["r","pic"]' ELEMENT`. `BLOCK` and element count come from this exact input; never reuse the example's indices. Decode `value.hex` as the saved path bytes. `//` is relative to the owning blend file, not the shell's working directory. Report both the saved spelling and its resolution, and label native/evaluated output behavior unverified.

`inspect.blocks` is an ordered array: its array index is BLOCK. `code_hex` includes all four code bytes, including NUL padding; Scene is `53430000`, not `5343`. Addresses are in `address_decimal`; parse decimal strings as integers before numeric formatting. Struct names are obtained from the SDNA type/struct tables. Read an actual saved record before selecting field names; do not assume `index`, `code`, `type`, `source_path` or `destination_path` aliases.

On refusal, retain stdout, stderr and exit status. Run `oxblend help error KIND` for every nested causal kind. Save the complete final schema-1 JSON stderr envelope (not mixed diagnostic prose): it includes an invocation receipt even when tracing is off. `oxblend report --report failure.json --stdout saved.stdout --output bug.zip --preview` verifies saved stdout and the canonical original error-envelope digest. With tracing, pass `--trace TRACE.jsonl`, or explicitly select `--run-id ID` / `--last` within `--trace-dir LOCAL_DIR`. The receipt binds the exact trace/root-span identity; legacy or interrupted evidence remains unverified/unknown. Incomplete stdout delivery stays explicitly partial even when its accepted-byte digest matches. A legacy/unrelated saved report never inherits verification from another trace. Review every returned member and privacy warning, then publish only that snapshot with `report --create PREVIEW.json --preview-sha256 SHA256 --output bug.zip`. Names, paths, raw messages, argv, payload and scene bytes are excluded by default; typed identifiers are bundle-local HMAC-SHA-256 tokens. Never add `--include-names` or `--include-file FILE` without an explicit informed choice; whole-file opt-in discloses all contents as a user-supplied, unverified attachment, not a certified input scene. Only accept a preview obtained directly from your own report command; create publishes caller-reviewed bytes but does not authenticate or re-sanitize a preview+digest supplied by another party. The CLI never reruns the failed command, recaptures source files, launches a browser, uploads or files. Show the fixed public GitHub prefill and reviewed local ZIP to the user; filing and attachment remain manual. Reporting does not repair or validate a scene.

`run LABEL COMMAND [ARGS...]` uses LABEL only for log filenames; COMMAND is still required. Correct: `run overview summary scene.blend`. Incorrect: `run summary scene.blend`. The helper is a shell function, not an executable: put optional launch wrappers around `$OXBLEND` inside the function, not around `run`. Direct `oxblend … >stdout 2>stderr; status=$?` calls are equally valid.

`cargo install oxblend --locked` installs the released crate, not GitHub main. After installation run `oxblend --version`, then `oxblend skill` for its matching offline guide. Use documentation for that exact version. Examples for an unreleased build do not imply that its flags exist in the released CLI. Checkout links stay main-relative until a matching release tag exists.
<!-- /generated-guidance -->

### Python failures and shareable reports

A Python `OxblendError` may carry `source_snapshot: SourceDiagnostic | None`
with owned source-site observations. The raw snapshot can disclose local paths
and messages. Use `oxblend.report(error_or_warning_result, new_zip)` to prepare
a sanitized preview through the same engine privacy projector as the CLI;
review it before `report_create`. Direct reports honestly state library-unbound
evidence. Native `diagnostic_ref` is null because the CLI invocation ledger is
CLI-only; never synthesize an ID or imply invocation verification.

Warning-bearing producer reports retain ordered sanitized `warnings` in the
report and `error-source.json`, separately from `failures`. These are real
warning facts, not merely acceptance of the result object. Explicit saved
pointer/slot coordinates survive; missing context remains unknown. The same
closed privacy sanitizer handles CLI and Python inputs; no raw paths, names or
messages by default, no recapture. Empty warnings preserve the prior shape.
