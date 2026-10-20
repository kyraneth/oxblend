#!/usr/bin/env bash
# Public qualification entry point. Additional receiving gates are caller-owned.
set -uo pipefail
collect_all=0; armed=0; status=0
for argument in "$@"; do
    case "$argument" in
        --collect-all) collect_all=1 ;;
        --armed) armed=1 ;;
        *) printf 'usage: scripts/qualify.sh [--armed] [--collect-all]\n' >&2; exit 2 ;;
    esac
done
if (( collect_all )); then export OXBLEND_COLLECT_ALL=1; else unset OXBLEND_COLLECT_ALL; fi
cd "$(dirname "$0")/.."
logs=${OXBLEND_QUALIFY_LOGS:-.local-results/qualify}
export TMPDIR=${TMPDIR:-"$PWD/.local-results/tmp"}
mkdir -p "$logs" "$TMPDIR"
run() {
    local name=$1; shift
    if [[ -n ${OXBLEND_STAGE_PROGRESS:-} ]]; then
        printf '%s BEGIN %s\n' "$(date -u +%FT%TZ)" "$name" >> "$OXBLEND_STAGE_PROGRESS"
    fi
    /usr/bin/time -v -o "$logs/$name.time.txt" "$@" > "$logs/$name.log" 2>&1
    local rc=$?
    if (( collect_all )) && grep -q '^COLLECT_DELTA ' "$logs/$name.log"; then rc=1; fi
    printf 'EXIT=%s\n' "$rc" >> "$logs/$name.log"
    if [[ -n ${OXBLEND_STAGE_PROGRESS:-} ]]; then
        printf '%s END_EXIT=%s %s\n' "$(date -u +%FT%TZ)" "$rc" "$name" >> "$OXBLEND_STAGE_PROGRESS"
    fi
    printf '%s EXIT=%s\n' "$name" "$rc"
    if grep -q 'SKIP' "$logs/$name.log"; then
        printf '%s contains SKIP; qualification failed\n' "$name"
        rc=1
    fi
    if (( collect_all && rc != 0 )); then status=1; return 0; fi
    return "$rc"
}
run build cargo build --release --offline --locked || exit $?
run tests cargo test --offline --locked --all-targets --no-fail-fast || exit $?
run clippy cargo clippy --offline --locked --all-targets -- -D warnings || exit $?
run fmt cargo fmt --all -- --check || exit $?
# The malformed-input mutation and structure-aware sweep is a gate, not
# a one-off probe. It prints MALFORMED_CASES= and MALFORMED_CLI_RUNS=.
run malformed cargo test --offline --locked --test malformed_input -- --nocapture || exit $?
# deny.toml plus a THIRD-PARTY-NOTICES file that must equal what the
# vendored licence texts produce. Runs without cargo-deny installed.
run licence scripts/licence-gate.sh || exit $?
# The README's no-network/no-telemetry statement is verified here.
run network scripts/no-network-check.sh || exit $?
# The stability tables are inventories of the current source, not a second spec.
run stability python3 scripts/check-stability.py || exit $?
if [[ -n ${FLATTRACE_CHECKOUT:-} ]]; then
    run flattrace python3 scripts/check-flattrace.py || exit $?
fi
if (( armed )); then
    : "${OXBLEND_QUALIFY_EXTRA:?armed qualification requires explicit additional-gates entry point}"
    run additional "$OXBLEND_QUALIFY_EXTRA" "$logs" || exit $?
fi
if (( collect_all )); then printf 'COLLECT_ALL_DIAGNOSTIC EXIT=%s\n' "$status"; exit "$status"; fi
printf 'QUALIFICATION EXIT=0\n'
