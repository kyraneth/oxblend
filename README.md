# Oxblend

Read, trace, pack and safely edit Blender `.blend` files without Blender.

A fast Rust library and command-line tool. It tells you what a `.blend` file contains and what it depends on, packs it with its dependencies, and makes guarded edits, all without opening Blender. When it can't be sure, it refuses and says why instead of guessing.

Linux x86_64 · version 1.0.0

## Install (Linux x86_64)

```sh
cargo install oxblend --locked
```

Or download a binary from [Releases](https://github.com/kyraneth/oxblend/releases).

## Use

```sh
oxblend --help
```

[Usage](docs/USAGE.md) · [Stability](docs/STABILITY.md) · [For AI agents](AGENTS.md)

## Licence

Code: [MIT](LICENSE). Docs: [CC BY 4.0](docs/LICENSE.md). © 2026 Mario Hawat.
