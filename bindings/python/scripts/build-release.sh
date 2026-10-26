#!/usr/bin/env bash
# Build the oxblend release distributions locally, prepare-only.
#
# Release contract: sdist first, then the wheel from the extracted sdist;
# SOURCE_DATE_EPOCH from the source commit; maturin==1.15.0.
# This script never uploads, publishes or contacts any network service beyond
# what plain `cargo build` needs. It mirrors .github/workflows/release.yml so
# that a local build passes the same packaging gate (check_release_dist.py)
# the workflow applies before anything can be published.
#
# Reproducibility: two runs from the same commit produce byte-identical sdist
# and wheel when they use the same toolchain and interpreter, because:
#   - SOURCE_DATE_EPOCH is the source commit's timestamp (maturin honours it
#     for archive member timestamps);
#   - OXBLEND_BUILD_COMMIT / OXBLEND_BINDING_BUILD_COMMIT are injected from
#     HEAD, so the wheel built from the git-less extracted sdist embeds the
#     same revision as the sdist build (build.rs otherwise falls back to
#     `git describe --dirty`, which is not stable);
#   - the extracted source uses an exclusive commit-addressed path under
#     TMPDIR: Cargo package identity and generated SBOM references retain this
#     path even when rustc path remapping is enabled. Local and CI builds use
#     the same extraction identity when TMPDIR is /tmp. Separate clean target
#     directories are supported; runs must be serial;
#   - RUSTFLAGS --remap-path-prefix strips this checkout path, CARGO_HOME,
#     TMPDIR and $HOME from rustc's mapped source paths. It does not normalize
#     Cargo package identity or generated SBOM references.
# Byte equality against CI additionally needs the same extraction identity,
# pinned toolchain (rustc 1.94.1), CPython 3.11.14 and ubuntu-22.04 (including
# native linkage/tooling). Two local builds alone do not prove CI byte equality;
# compare the actual CI artifact before approving its hashes.
#
# Usage:
#   bindings/python/scripts/build-release.sh [--out DIR] [--sdist-only]
#
#   --out DIR       output directory (default: <repo>/dist-release).
#                   Must not already contain files: one build per directory,
#                   so two directories can be compared directly.
#   --sdist-only    stop after the sdist (fast loop; the shared packaging
#                   gate then runs in --sdist-only mode).
#
# Environment:
#   PYTHON_BUILD    interpreter used for the wheel build and helper scripts
#                   (default: python3.11, matching the abi3 floor and CI).
#   MATURIN         maturin executable (default: maturin).
#
# Exit status: 0 only if the build and every packaging check succeeded.

set -euo pipefail

usage() {
  awk 'NR > 1 && !/^#/ { exit } NR > 1 { sub(/^# ?/, ""); print }' "${BASH_SOURCE[0]}"
}

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
BINDING_DIR="bindings/python"
OUT="$REPO_ROOT/dist-release"
SDIST_ONLY=0
PYTHON_BUILD="${PYTHON_BUILD:-python3.11}"
MATURIN="${MATURIN:-maturin}"
MATURIN_VERSION="1.15.0"   # Must match bindings/python/pyproject.toml build-system.
RUST_TOOLCHAIN="1.94.1"    # Workflow pin; rust-version in Cargo.toml is "1.94".

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      [[ $# -ge 2 ]] || { echo "FAIL: --out needs a directory" >&2; exit 2; }
      OUT="$2"
      shift 2
      ;;
    --sdist-only)
      SDIST_ONLY=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "FAIL: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

note() {
  echo "==> $*"
}

# Keep caller-relative --out stable when the build later enters the sdist.
[[ "$OUT" == /* ]] || OUT="$PWD/$OUT"
cd "$REPO_ROOT"
[[ -d .git || -f .git ]] || fail "not a git checkout: $REPO_ROOT"

for command in git tar sha256sum "$MATURIN" "$PYTHON_BUILD"; do
  command -v "$command" >/dev/null 2>&1 || fail "missing required command: $command"
done
OUT="$("$PYTHON_BUILD" -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$OUT")"

# --- Read-only repository gates (same rules the workflow enforces) ----------

dirty="$(git status --porcelain --untracked-files=no || true)"
[[ -z "$dirty" ]] || {
  echo "FAIL: tracked tree is not clean; a release build must come from committed bytes:" >&2
  echo "$dirty" >&2
  exit 1
}
git diff --exit-code --quiet -- Cargo.lock "$BINDING_DIR/Cargo.lock" \
  || fail "Cargo.lock or binding Cargo.lock differ from HEAD"

# --- Assert the pinned build backend (the workflow asserts the same) --------

"$PYTHON_BUILD" - "$BINDING_DIR/pyproject.toml" <<'PY'
import sys, tomllib
from pathlib import Path
config = tomllib.loads(Path(sys.argv[1]).read_text())
assert config["build-system"] == {
    "requires": ["maturin==1.15.0"], "build-backend": "maturin"
}, "pyproject build-system must pin maturin==1.15.0"
PY

maturin_version="$("$MATURIN" --version)"
[[ "$maturin_version" == "maturin $MATURIN_VERSION" ]] \
  || fail "expected 'maturin $MATURIN_VERSION', got '$maturin_version'"
note "$maturin_version"

# --- Reproducible build environment ----------------------------------------

SOURCE_DATE_EPOCH="$(git show -s --format=%ct HEAD)"
SOURCE_COMMIT="$(git rev-parse HEAD)"
export SOURCE_DATE_EPOCH
export OXBLEND_BUILD_COMMIT="$SOURCE_COMMIT"
export OXBLEND_BINDING_BUILD_COMMIT="$SOURCE_COMMIT"
export FLATTRACE_LEVEL="${FLATTRACE_LEVEL:-2}"   # Matches the workflow env.

CARGO_HOME_DIR="${CARGO_HOME:-$HOME/.cargo}"
TMP_DIR="${TMPDIR:-/tmp}"
# rustc uses the LAST matching --remap-path-prefix rule.
# Put broader prefixes first so the specific source/cargo mappings win.
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--remap-path-prefix=$HOME=<home> --remap-path-prefix=$TMP_DIR=<tmp> --remap-path-prefix=$CARGO_HOME_DIR=<cargo-home> --remap-path-prefix=$REPO_ROOT=<oxblend-src>"

rustc_version="$(rustc --version 2>/dev/null || echo 'rustc missing')"
cargo_version="$(cargo --version 2>/dev/null || echo 'cargo missing')"
python_version="$("$PYTHON_BUILD" --version 2>/dev/null || echo 'python missing')"
case "$rustc_version" in
  "rustc $RUST_TOOLCHAIN "*) ;;
  *) echo "WARN: toolchain is '$rustc_version', CI pins rustc $RUST_TOOLCHAIN; two local builds still compare, CI byte-parity needs $RUST_TOOLCHAIN" >&2 ;;
esac

if [[ ! -d "$OUT" ]]; then
  mkdir -p "$OUT"
fi
[[ -z "$(ls -A "$OUT")" ]] || fail "output directory is not empty: $OUT (remove it so builds never mix)"

note "source commit: $SOURCE_COMMIT"
note "SOURCE_DATE_EPOCH: $SOURCE_DATE_EPOCH"
note "$rustc_version / $cargo_version / $python_version"

# --- sdist first, then the wheel from the EXTRACTED sdist -------------------

"$PYTHON_BUILD" "$BINDING_DIR/scripts/guidance.py" --check
"$PYTHON_BUILD" "$BINDING_DIR/tests/metadata_gate.py"
note "building raw sdist (retained unchanged beside the distribution)"
RAW_OUT="${OUT}.maturin-input"
[[ ! -e "$RAW_OUT" ]] || fail "raw sdist directory already exists: $RAW_OUT"
mkdir -p "$RAW_OUT"
"$MATURIN" sdist --manifest-path "$BINDING_DIR/Cargo.toml" --out "$RAW_OUT"
raw_sdists=("$RAW_OUT"/*.tar.gz)
[[ ${#raw_sdists[@]} -eq 1 && -f "${raw_sdists[0]}" ]] || fail "one raw maturin sdist required"
"$PYTHON_BUILD" "$BINDING_DIR/scripts/normalize_sdist.py" "${raw_sdists[0]}" \
  "$OUT/sdist/$(basename "${raw_sdists[0]}")" > "$OUT/normalization.json"

if [[ "$SDIST_ONLY" -eq 0 ]]; then
  work="$TMP_DIR/oxblend-src-$SOURCE_COMMIT"
  # Do not reuse or remove another run's extraction, even for the same commit.
  mkdir -- "$work" || fail "extraction directory already exists or cannot be created: $work"
  trap 'rm -rf -- "$work"' EXIT
  tar -xzf "$OUT"/sdist/*.tar.gz -C "$work"
  sdist_dir="$(echo "$work"/*/)"
  [[ -f "${sdist_dir}pyproject.toml" ]] || fail "extracted sdist has no top-level pyproject.toml"
  note "building wheel from extracted sdist: $sdist_dir"
  # The PEP 517 project is the extracted root, not the nested Rust crate: a
  # direct --manifest-path build here emits a wheel without the oxblend facade.
  (
    cd "$sdist_dir"
    # Map the commit-addressed extraction directory, not just its temp parent.
    export RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=${sdist_dir%/}=<oxblend-src>"
    "$PYTHON_BUILD" bindings/python/scripts/guidance.py --check
    "$MATURIN" pep517 build-wheel --profile release --locked \
      --interpreter "$PYTHON_BUILD" \
      --out "$OUT/wheels"
    "$PYTHON_BUILD" bindings/python/scripts/notices.py check
  )
fi

# --- Reproducibility metadata (no timestamps: stays diffable) ---------------

{
  echo "source_commit=$SOURCE_COMMIT"
  echo "source_date_epoch=$SOURCE_DATE_EPOCH"
  echo "rustc=$rustc_version"
  echo "cargo=$cargo_version"
  echo "python=$python_version"
  echo "maturin=$maturin_version"
  echo "rustflags=$RUSTFLAGS"
  echo "sdist_only=$SDIST_ONLY"
} > "$OUT/BUILD-INFO.txt"

# --- Checksums, then the shared packaging gate ------------------------------

(
  cd "$OUT"
  if [[ "$SDIST_ONLY" -eq 1 ]]; then
    sha256sum sdist/*.tar.gz > SHA256SUMS
  else
    sha256sum sdist/*.tar.gz wheels/*.whl > SHA256SUMS
  fi
)

helper_args=(--dist "$OUT")
[[ "$SDIST_ONLY" -eq 1 ]] && helper_args+=(--sdist-only)
note "running packaging gate"
"$PYTHON_BUILD" "$BINDING_DIR/scripts/check_release_dist.py" "${helper_args[@]}"

# --- Optional local checks the workflow also runs ---------------------------

if [[ "$SDIST_ONLY" -eq 0 ]]; then
  if "$PYTHON_BUILD" -c "import twine" >/dev/null 2>&1; then
    note "twine check"
    "$PYTHON_BUILD" -m twine check --strict "$OUT"/sdist/*.tar.gz "$OUT"/wheels/*.whl
  else
    echo "WARN: twine is not available to $PYTHON_BUILD; CI runs 'twine check --strict'" >&2
  fi

  if command -v readelf >/dev/null 2>&1; then
    wheel="$(echo "$OUT"/wheels/*.whl)"
    probe="$(mktemp -d "${TMPDIR:-/tmp}/oxblend-wheel-probe.XXXXXX")"
    "$PYTHON_BUILD" - "$wheel" "$probe/module.so" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    names = [n for n in archive.namelist() if n.startswith("oxblend/") and n.endswith(".so")]
    if len(names) != 1:
        raise SystemExit(f"FAIL: expected one native extension, found {names}")
    with open(sys.argv[2], "wb") as handle:
        handle.write(archive.read(names[0]))
PY
    echo "--- NEEDED entries (actual linkage) ---"
    readelf -d -W "$probe/module.so" | grep -E 'NEEDED|SONAME' || true
    echo "--- GLIBC symbol versions referenced (actual floor) ---"
    readelf --dyn-syms -W "$probe/module.so" | grep -o 'GLIBC_[0-9][0-9.]*' | sort -Vu || true
    rm -rf "$probe"
  else
    echo "WARN: readelf not available; CI prints the linkage and GLIBC floor" >&2
  fi
fi

note "done: $OUT"
cat "$OUT/SHA256SUMS"
