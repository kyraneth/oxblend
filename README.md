<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/kyraneth/oxblend/main/assets/logo/oxblend-logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/kyraneth/oxblend/main/assets/logo/oxblend-logo-light.svg">
    <img src="https://raw.githubusercontent.com/kyraneth/oxblend/main/assets/logo/oxblend-logo.svg" width="160" alt="Oxblend logo">
  </picture>
</p>

# Oxblend

Read, trace, pack and safely edit Blender `.blend` files without Blender.

A fast Rust library and command-line tool. It tells you what a `.blend` file contains and what it depends on, packs it with its dependencies, and makes guarded edits, all without opening Blender. When it can't be sure, it refuses and says why instead of guessing.

Linux x86_64 · release: **1.4.1**

Offline diagnostics: `FLATTRACE_DIR=/path/to/traces oxblend summary scene.blend`.
Unset/empty is off; no upload. See [local tracing](docs/USAGE.md#local-flattrace-diagnostics).

## Install (Linux x86_64)

```sh
cargo install oxblend --locked
```

This installs the latest **released crate**, not this development checkout.
Or download a released binary from [Releases](https://github.com/kyraneth/oxblend/releases).
Run `oxblend --version`, then `oxblend skill` for that executable's matching
offline guide. Use documentation for the installed version: unreleased examples
and flags are not promises about a released CLI. This checkout's relative docs
links describe its development behavior, not the latest published release.

## Use

```sh
oxblend summary scene.blend  # Every saved Scene: frame start/end/step and FPS.
oxblend skill                # Bundled, version-matched offline guide.
oxblend --help
```

[Usage](docs/USAGE.md) · [Stability](docs/STABILITY.md) · [For AI agents](AGENTS.md)

For many reads from one file, use the [worked single-open batch recipe](docs/USAGE.md#single-open-read-batches).
Discover saved SDNA type names from `inspect` (for example `Lamp`, not a guessed
runtime `Light`), union same-type field paths, and check terminal counts,
`complete` and `input_changed`. Only inspect/payload/get/pointer are batchable;
query/overrides/list are not. An empty selection is not proof of no changes.

Read [dependency and pack results](docs/USAGE.md#reading-dependency-and-pack-results)
as separate warnings, coverage, existence and publication/durability facts.
`missing_link_counts` is a map, not a scalar. A missing cross-platform built-in
application asset is still a missing dependency. `FLATTRACE_DIR` enables local
diagnostic traces; it does not turn incomplete output into a complete result.

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

## Cross-reference four shots before packing

```sh
printf '%s\n' project/seq-a/shot.blend project/seq-b/shot.blend \
  project/seq-c/shot.blend project/seq-d/shot.blend > shots.txt
oxblend trace --files-from shots.txt > dependencies.json 2> dependencies.stderr
status=$?
oxblend pack-sequence --files-from shots.txt -p project \
  -t package-preview-new --dry-run > plan.json 2> plan.stderr
status=$?
```

Inspect every `root_traces` entry and `cross_reference.roots`: shared/unique
assets, missing files, missing linked IDs and typed hard root errors. `trace
--summary` or multi-root `list` gives the complete human matrix; `list --json`
retains per-root listings too. Both commands retain independently readable shots
around a failed shot and exit 2 if any root is incomplete. One distinct root
keeps its existing output. Dependency realpath grouping does not change lexical
pack destinations; inventory bytes are not package bytes. Prefer explicit `-p`
to the inferred common input parent. A parseable plan is not a complete package
or Blender/render verification. See [multi-root details](docs/USAGE.md#multi-root-dependency-reports).

## Licence

Code: [MIT](LICENSE). Docs: [CC BY 4.0](docs/LICENSE.md). © 2026 Mario Hawat.
