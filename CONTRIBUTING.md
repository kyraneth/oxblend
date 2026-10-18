# Contributing to Oxblend

Keep changes scoped to the serialized-data contract. Inputs stay read-only, new outputs never replace existing files, and incomplete reports remain explicit. Preserve raw path/name bytes and checked layout arithmetic. Add a regression through the real CLI or public API for every reproduced behavior change.

The qualified toolchain is Rust 1.94.1. Build and test with:

```sh
cargo build --release --locked
cargo test --locked
scripts/qualify.sh
```

The first run needs network access or a warm Cargo cache; `scripts/qualify.sh` uses `--offline`.

Use the locked dependency set; do not silently introduce new crates. A new crate must pass `scripts/licence-gate.sh` (the deny.toml allow list) and its vendored licence texts must be added to `THIRD-PARTY-NOTICES` with `scripts/third-party-notices`. Run `scripts/qualify.sh` and inspect its EXIT lines. Public default tests are self-contained. Reopen write results with compatible Blender before making evaluated-behavior claims.

Code contributions follow the MIT licence; documentation/media follow CC BY 4.0. See LICENSE, docs/LICENSE.md and NOTICE.
