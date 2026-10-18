# Oxblend for agents

Oxblend is a Rust library and CLI for reading and safely operating on serialized Blender `.blend` files without Blender.

## Use the CLI

Retain **stdout, stderr AND the exit code**. Treat valid JSON as data, not proof of success. Check `complete`, errors, warnings, publication and durability fields. Exit codes: **0:** declared command contract completed (also a clean exit when a downstream pipe closes); **2:** usage/read/write error or incomplete report; **3:** selected input is missing or not a file for list/blocks/pack; **5:** explicit pack project path does not exist; nothing written. **Signal/other:** external termination or unexpected failure; inspect the destination before retrying.

Keep inputs read-only. Use new output names; never overwrite inputs or merge a partial pack. A pack may publish available members and still exit 2. Inspect the report before using its output.

- `summary`: read the header and scene/object overview.
- `inspect`: inspect ordered blocks and the file's own SDNA.
- `get`: read a typed field by block and explicit field path.
- `pointer`: resolve a checked pointer target explicitly.
- `payload`: read a complete block payload as hex.
- `source-facts`: inspect per-file source facts.
- `library-facts`: inspect per-file library facts.
- `trace`: discover dependencies and report incomplete traversal.
- `list`: list existing expanded dependencies; use trace for missing-ID detail.
- `blocks`: inspect block sizes and pointer referrers.
- `overrides`: inspect serialized overrides without evaluating them.
- `diff`: compare allocations/content without guessing unresolved identity.
- `edit`: make expected-old, fixed-width typed or masked edits to a new file.
- `rewrite`: apply explicit expected-old path edits to a new file.
- `pack`: package one root and its available dependencies.
- `pack-sequence`: package multiple roots with deduplication.
- `exr-header`: inspect EXR metadata without decoding pixels.
- `pointer-census`: inspect whole-file pointer states, not semantic reconstruction.

Read [docs/USAGE.md](docs/USAGE.md) for exact arguments and JSON inputs, [docs/STABILITY.md](docs/STABILITY.md) for contracts, and [skills/oxblend/SKILL.md](skills/oxblend/SKILL.md) for agent workflows. Use `oxblend --help` before constructing a command. Never reuse physical block indices from another file or save.

## Contribute

Run `cargo build --release --locked` and `scripts/qualify.sh`; use `--offline` when locked dependencies are cached. Preserve negative tests and source/output integrity checks. Do not copy code from other `.blend` tools. Do not add network crates or telemetry. Keep production-specific workflows out of the product; implement generic capabilities instead.
