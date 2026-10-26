# Oxblend for agents

<!-- generated-diagnosis -->
**Diagnose does not mean repair.** `inspect`, `get`, `pointer` and `payload` describe saved bytes; they do not establish the intended scene. If trace or pack reports an invalid/unknown pointer, explain the site and stop. Do not guess a replacement pointer, null it, or bypass an Oxblend refusal with Python, hex editing or another writer unless the user explicitly authorizes a separate repair plan. Keep the original untouched. A clean trace or `complete:true` after a change proves only this command's serialized contract, not that the change is correct or that Blender can open or render it. Prefer a known-good save or repair in compatible Blender, then verify the new file natively before recommending delivery.
<!-- /generated-diagnosis -->

Oxblend is a Rust library and CLI for reading and safely operating on serialized Blender `.blend` files without Blender.

## Use the CLI

Retain **stdout, stderr AND the exit code**. Treat valid JSON as data, not proof of success. Check `complete`, errors, warnings, publication and durability fields. Exit codes: **0:** declared command contract completed (including complete stdout write/flush); **2:** usage/read/write error, broken pipe or incomplete report; **3:** selected input is missing or not a file for list/blocks/pack; **5:** explicit pack project path does not exist; nothing written. **Signal/other:** external termination or unexpected failure; inspect the destination before retrying.

Keep inputs read-only. Use new output names; never overwrite inputs or merge a partial pack. Reports retain every provable fact and mark each missing/unknown site. Strict
pack (default) publishes nothing on missing/unknown requirements; explicit
`--permissive` may publish available members, labelled INCOMPLETE, exit 2.
Inspect the report before using its output. Even a complete pack has not verified
Blender open, evaluation or rendering.

- `summary`: read the header and every saved Scene, including frame start/end/step and FPS numerator/base/effective, in one call.
- `skill`: print the bundled version-matched guide offline.
- `inspect`: inspect ordered blocks and the file's own SDNA.
- `get`: read a typed field by block and explicit field path.
- `pointer`: resolve a checked pointer target explicitly.
- `payload`: read a complete block payload as hex.
- `source-facts`: inspect per-file source facts.
- `library-facts`: inspect per-file library facts.
- `trace`: discover dependencies and report incomplete traversal.
- `list`: list expanded dependencies, marking missing physical declarations (exit 2); use trace for missing-ID detail.
- `blocks`: inspect block sizes and pointer referrers.
- `overrides`: inspect serialized overrides without evaluating them.
- `diff`: compare allocations/content without guessing unresolved identity.
- `edit`: make expected-old, fixed-width typed or masked edits to a new file.
- `rewrite`: apply explicit expected-old path edits to a new file.
- `pack`: package one root and its available dependencies.
- `pack-sequence`: package multiple roots with deduplication.
- `exr-header`: inspect EXR metadata without decoding pixels.
- `pointer-census`: inspect whole-file pointer states, not semantic reconstruction.

Read [docs/USAGE.md](docs/USAGE.md) for exact arguments and JSON inputs, [docs/STABILITY.md](docs/STABILITY.md) for contracts, and [skills/oxblend/SKILL.md](skills/oxblend/SKILL.md) for agent workflows. Use `oxblend skill` for bundled task recipes and `oxblend COMMAND --help` for syntax. Never reuse physical block indices from another file or save.

## Contribute

Run `cargo build --release --locked` and `scripts/qualify.sh`; use `--offline` when locked dependencies are cached. Preserve negative tests and source/output integrity checks. Do not copy code from other `.blend` tools. Do not add network crates or telemetry. Keep production-specific workflows out of the product; implement generic capabilities instead.


<!-- generated-guidance -->
Dependency package status: COMPLETE / INCOMPLETE (see report). “Complete” concerns the selected serialized dependency closure only. Blender open, evaluation and rendering have NOT been verified by this command. Before farm delivery, open the isolated package in compatible Blender with original project sources unavailable; check missing data and run the required render checks. A repaired or byte-edited input needs its own native validation.
<!-- /generated-guidance -->
