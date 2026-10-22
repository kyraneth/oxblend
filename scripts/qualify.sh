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
logs=$(cd "$logs" && pwd)
mkdir -p "$logs/traces"
trace_root=$(mktemp -d -p "$logs/traces") || exit $?
printf '%s\n' "$trace_root" > "$logs/trace-root.txt"
# Traces are default evidence even without the optional reference checker.
# Each CLI process owns a fresh file; stages never share a trace file.
export FLATTRACE_LEVEL=${FLATTRACE_LEVEL:-1}
unset FLATTRACE_TRACEPARENT
if [[ -n ${FLATTRACE_CHECKOUT:-} ]]; then
    export PYTHONPATH="$FLATTRACE_CHECKOUT${PYTHONPATH:+:$PYTHONPATH}"
fi
check_traces() {
    local directory=$1
    if python3 -c 'import flattrace' >/dev/null 2>&1; then
        find "$directory" -type f -name '*.jsonl' -print0 |
            xargs -0 -r python3 -m flattrace check --events "$PWD/flattrace-events.json"
    else
        printf 'flattrace checker unavailable; traces retained, validation not performed\n'
    fi
}
run() {
    local name=$1; shift
    if [[ -n ${OXBLEND_STAGE_PROGRESS:-} ]]; then
        printf '%s BEGIN %s\n' "$(date -u +%FT%TZ)" "$name" >> "$OXBLEND_STAGE_PROGRESS"
    fi
    export FLATTRACE_DIR="$trace_root/$name"
    mkdir -p "$FLATTRACE_DIR"
    local timer=${OXBLEND_TIME:-/usr/bin/time}
    local rc
    if "$timer" --version 2>/dev/null | grep -q 'GNU [Tt]ime'; then
        "$timer" -v -o "$logs/$name.time.txt" "$@" > "$logs/$name.log" 2>&1
        rc=$?
    else
        printf 'GNU time unavailable (%s); stage runs without resource metrics\n' "$timer" > "$logs/$name.time.txt"
        printf '%s: GNU time unavailable; resource metrics omitted\n' "$name" >&2
        "$@" > "$logs/$name.log" 2>&1
        rc=$?
    fi
    check_traces "$FLATTRACE_DIR" > "$logs/$name.trace-check.log" 2>&1 || rc=1
    case "$name" in
        tests|malformed|smoke)
            if [[ $FLATTRACE_LEVEL != 0 && -z $(find "$FLATTRACE_DIR" -type f -name '*.jsonl' -print -quit) ]]; then
                printf 'expected Oxblend trace evidence is absent\n' >> "$logs/$name.trace-check.log"
                rc=1
            fi ;;
    esac
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
run smoke "${CARGO_TARGET_DIR:-target}/release/oxblend" summary tests/fixtures/file-ops/synthetic-empty-seed.blend || exit $?
run clippy cargo clippy --offline --locked --all-targets -- -D warnings || exit $?
run fmt cargo fmt --all -- --check || exit $?
run paths python3 scripts/check-paths.py || exit $?
run trace-phases python3 scripts/check-trace-phases.py || exit $?
run timer-fallback python3 scripts/test-qualify-time.py || exit $?
run notices-cache python3 scripts/test-notices.py || exit $?
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
    run writer-drift python3 scripts/check-writer-drift.py || exit $?
    run flattrace python3 scripts/check-flattrace.py || exit $?
fi
if (( armed )); then
    : "${OXBLEND_QUALIFY_EXTRA:?armed qualification requires explicit additional-gates entry point}"
    run additional "$OXBLEND_QUALIFY_EXTRA" "$logs" || exit $?
fi
if (( collect_all )); then printf 'COLLECT_ALL_DIAGNOSTIC EXIT=%s\n' "$status"; exit "$status"; fi
printf 'QUALIFICATION EXIT=0\n'
