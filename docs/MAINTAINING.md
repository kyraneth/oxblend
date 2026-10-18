# Verification

## Public suite

Run `scripts/qualify.sh` for the self-contained public suite: offline/locked build and tests, clippy, formatting and command regression gates. Cached dependencies and the existing Rust toolchain are required. Every command logs its real EXIT status and a failure propagates to the caller.

The `malformed` gate runs `tests/malformed_input.rs`: a deterministic, seeded, structure-aware mutation sweep of the reader plus the real CLI. It asserts that every generated case terminates, does not panic, that grammar-violating input is refused with exit 2 and structured JSON stderr, and that anything accepted is structurally self-consistent. It needs no fuzzer, no nightly toolchain and no network; the log lines `MALFORMED_CASES=` and `MALFORMED_CLI_RUNS=` are the observed corpus sizes.

The `licence` gate runs `scripts/licence-gate.sh`: it regenerates `THIRD-PARTY-NOTICES` from the licence texts vendored beside the locked crates and requires the committed file to be byte-identical, then checks every licence identifier declared by a package in the built graph against deny.toml's `[licenses] allow` list. It needs no network and runs whether or not `cargo-deny` is installed (when it is, `cargo deny check licenses` also runs). After changing dependencies, regenerate the notices with `scripts/third-party-notices`.

The `network` gate runs `scripts/no-network-check.sh`, the no-network/no-telemetry source gate: it checks the locked graph for network crates and `src/` for socket, DNS, HTTP and telemetry identifiers.

For edits, retain input hashes, decode containers independently and verify header/schema/index, declared byte ranges, untouched bytes and no-op physical identity. For packs, inspect complete/errors/missing, actual output members, copied-asset hashes and ZIP extraction/CRC. A process exit or JSON parse alone does not prove publication or completeness.

## Release assets

Run `scripts/release-build --output .local-results/release` from a source checkout.
It builds only `x86_64-unknown-linux-gnu`, offline and locked, using the active
Rust toolchain (Cargo honours `rust-toolchain` or `rust-toolchain.toml` if present).
There is currently no toolchain file; release builds are qualified with Rust 1.94.1. Python 3.11+, a Linux
x86_64 host, a C compiler and cached dependencies are required. The script does
not install or download tools. Use `cargo fetch --locked --target
x86_64-unknown-linux-gnu` first on a connected machine when preparing a cache.

`SOURCE_DATE_EPOCH` defaults to the latest commit's Unix timestamp. Supply it
explicitly when building from a source archive without Git. Rust source, target
and Cargo cache paths are remapped; timestamps, ownership, archive ordering and
gzip headers are deterministic. `RUSTFLAGS` and `CARGO_ENCODED_RUSTFLAGS` are
replaced for this release build so local host-tuning flags do not leak in.
Reproducibility requires identical source bytes, toolchain, C compiler and
system libraries; it does not promise byte identity across Linux distributions.
`CARGO_TARGET_DIR`, `TMPDIR` and `CARGO_HOME` remain caller-controlled.

Outputs are the versioned tarball, CycloneDX 1.5 `.cdx.json` SBOM, `.build.json`
compiler/binary/lock receipt and `SHA256SUMS`. The tarball includes the binary,
README, licences/notices, documentation and `BUILD-INFO.json`. The SBOM comes
from offline Cargo metadata plus Cargo.lock checksums. It describes the resolved
Linux graph, including build dependencies; it is not a vulnerability scan or an
inventory of host system libraries. No SBOM tool installation is required.

To compare two clean builds, set the same `SOURCE_DATE_EPOCH`, use separate
`CARGO_TARGET_DIR` and output directories, then `diff` their `SHA256SUMS` files.
Extract a tarball, run `oxblend --version` and `--help`, and retain its notices.
The GNU binary dynamically links libc; check `ldd`/`readelf` for the build's
actual runtime requirements. Do not claim universal Linux compatibility.

Version 1.0 ships without a CI workflow. Qualification and release builds are
run locally with the commands above. CI is planned as a separate follow-up:
every action must be pinned to an exact commit SHA, and the workflow must run
once and be verified before it is added publicly. No CI execution or hosted
artifact availability is claimed for 1.0.

Publication is deliberate and requires maintainer approval. Never publish a
crate or repository from a development checkout containing non-public files or
history.

Reopen edited outputs in the compatible Blender version and inspect exact RNA values and untouched sentinels. A successful open is not a semantic or pixel certificate. Timing comparisons require symmetric inputs, sync barriers, recorded temporary filesystem and binary hashes; numbers are observations, not universal speed guarantees.
