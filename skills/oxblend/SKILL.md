---
name: oxblend
description: "Inspects saved Blender files and their dependencies with Oxblend. Use when summarizing a .blend, finding missing files, packing for a render farm, comparing versions, repathing an image or reading EXR header values without starting Blender."
---

# Oxblend 1.2.0

Copyright (c) 2026 Mario Hawat. CC BY 4.0:
https://creativecommons.org/licenses/by/4.0/ . Software is MIT.
This version-matched guide is bundled in the executable: `oxblend skill`
(or `oxblend skill print`). It does not install or modify agent instructions.

## Frame range and FPS — one summary call
```sh
oxblend summary scene.blend > summary.json 2> summary.stderr
status=$? # Retain this producer exit before reading/filtering summary.json.
```
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
captures failures even with shell error-exit enabled; use fresh log labels:

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
Optional extended references: [USAGE](https://github.com/kyraneth/oxblend/blob/v1.2.0/docs/USAGE.md)
and [STABILITY](https://github.com/kyraneth/oxblend/blob/v1.2.0/docs/STABILITY.md).
These links are not installed files. Examples below use `run`; substitute actual inputs.
Capture the producer before filtering: a closed stdout now exits 2 with
`broken_pipe`, not success. Pipelines can hide that producer status unless the
shell uses pipefail. Read/filter saved stdout after the command finishes.

Hex fields encode bytes: decode name/path/code hex before display, not payload or
float-bit hex as text. Readable aliases beside `_hex` fields are UTF-8 or null;
hex stays authoritative. Preserve invalid UTF-8 bytes rather than guessing text.

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

## Which files are needed or missing?
```sh
run dependencies trace scene.blend
run existing list scene.blend --json
```
Use `trace` as the reference inventory; `list` is an expanded existing-file view.
Trace defaults to recursive/strict. Read errors and warnings even on exit 0.
`complete` means traversal completed. Separately inspect each usage's `exists`,
`resolved_path`, and top-level `missing_path_count` / `referenced_paths_exist`.
The count is missing usage sites, not distinct files. `//` resolves relative to the
owning .blend's directory, including inside libraries, not the root or shell cwd.
Use resolved paths (or decode their hex) to stat again when availability matters.
Existence is a non-atomic observation: glob/UDIM/sequence success needs only one
expanded file; directories count as present. Check required frames separately.
Unused saved linked stubs are conservatively traced/packed, not proven render needs.
Missing assets/libraries make `list` exit 2 with a typed stderr error and no listing;
missing IDs/incomplete traversal also refuse. Use trace for full missing details.
Only trace/list accept `--low-memory`; it trades CPU/IO/temp disk for lower memory,
not a memory ceiling. `source-facts` answers per-file collection/object/library
closure; `library-facts` answers library collection/object discovery. Neither is a
general scene summary; both can refuse with exit 2/no stdout on linked layouts
(e.g. missing_field/pointer_target_type). Fall back to trace for library questions.

## Pack for a farm and judge completeness
```sh
run plan pack project/scene.blend package-preview -p project --dry-run
run package pack project/scene.blend package-new -p project
```
Dry-run writes nothing. Project must exist and lexically contain selected roots.
`--exclude GLOB...` excludes paths; `--relative-only` leaves absolute references
uncopied/unmodified: disclose either restriction. Use `pack-sequence` for multiple
roots, `--keep-hierarchy` for layout, or a fresh `.zip` target for an archive.
Never overwrite, merge or resume a partial destination; remote uploads unsupported.
Judge JSON `planning_complete`, `complete`, `errors`, `warnings`, `missing`,
`missing_links`, publication and durability fields separately. Report missing paths
from `missing[].absolute_path` (or authoritative `absolute_path_hex`), not just prose.
Missing files/linked IDs can publish available members yet exit 2: **not farm-ready**.
An untrustworthy trace refuses publication. `pack-info.txt` marks missing-file/
linked-ID packs INCOMPLETE and lists missing physical paths (decoded or hex).
It is still not a native validation certificate. ZIP member paths
are virtual. `output_written`/`archive_written` do not imply completeness or crash
persistence; durability defaults to `none`. Inspect destinations before retrying.
Even a complete pack covers the selected serialized closure, not render validation;
review unrelocated-path warnings and reopen/test in compatible Blender before use.

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

## Read EXR header values
```sh
run header exr-header image.exr
```
Read `header_complete`, `pixel_integrity_verified` (always false), and each
attribute's `decoded_value.status`: `decoded` has a typed value; `refused` gives a
reason kind/raw hex. Read STABILITY's "EXR attribute values" before interpreting.
Strings (including manifests) are not parsed as JSON or verified semantically.
No pixels, chunks or render integrity are checked; malformed headers can refuse.

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
Phase timing JSON is off by default. `OXBLEND_PHASE_TIMING=1 oxblend summary FILE`
opts into interim stderr timings (also trace/overrides/reflection commands),
until the planned T4 diagnostic trace mode replaces them. These timings are not
a stable interface. Other reader warnings/explicit diagnostic controls remain.
Preserve unknown kinds/added report fields; edit specs reject unknown keys.
Report reproducible bugs to the repository's [GitHub issues](https://github.com/kyraneth/oxblend/issues)
with safe inputs, version, argv, stdout/stderr, exit and completeness/publication
facts. Never attach private scenes or paths without permission.
