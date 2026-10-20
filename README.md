<p align="center"><img src="https://raw.githubusercontent.com/kyraneth/oxblend/main/assets/logo/oxblend-logo.svg" width="160" alt="Oxblend logo"></p>

# Oxblend

Read, trace, pack and safely edit Blender `.blend` files without Blender.

A fast Rust library and command-line tool. It tells you what a `.blend` file contains and what it depends on, packs it with its dependencies, and makes guarded edits, all without opening Blender. When it can't be sure, it refuses and says why instead of guessing.

Linux x86_64 · version 1.3.0

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

## Licence

Code: [MIT](LICENSE). Docs: [CC BY 4.0](docs/LICENSE.md). © 2026 Mario Hawat.
