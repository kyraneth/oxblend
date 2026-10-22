<p align="center"><img src="https://raw.githubusercontent.com/kyraneth/oxblend/main/assets/logo/oxblend-logo.svg" width="160" alt="Oxblend logo"></p>

# Oxblend

Read, trace, pack and safely edit Blender `.blend` files without Blender.

A fast Rust library and command-line tool. It tells you what a `.blend` file contains and what it depends on, packs it with its dependencies, and makes guarded edits, all without opening Blender. When it can't be sure, it refuses and says why instead of guessing.

Linux x86_64 · version 1.3.3

Offline diagnostics: `FLATTRACE_DIR=/path/to/traces oxblend summary scene.blend`.
Unset/empty is off; no upload. See [local tracing](docs/USAGE.md#local-flattrace-diagnostics).

## Install (Linux x86_64)

```sh
cargo install oxblend --locked
```

Or download a binary from [Releases](https://github.com/kyraneth/oxblend/releases).

## Use

```sh
oxblend summary scene.blend  # Every saved Scene: frame start/end/step and FPS.
oxblend skill                # Bundled, version-matched offline guide.
oxblend --help
```

[Usage](docs/USAGE.md) · [Stability](docs/STABILITY.md) · [For AI agents](AGENTS.md)

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

## Licence

Code: [MIT](LICENSE). Docs: [CC BY 4.0](docs/LICENSE.md). © 2026 Mario Hawat.
