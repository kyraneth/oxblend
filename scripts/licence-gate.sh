#!/usr/bin/env bash
# Licence gate for the qualified tree. Offline and cargo-deny-optional:
#
#   1. THIRD-PARTY-NOTICES is regenerated from the vendored licence texts and
#      must be byte-identical to the committed file; and
#   2. every licence identifier declared by a crate in the built graph must be
#      in deny.toml's [licenses] allow list.
#
# Check 2 is deliberately conservative: an expression like "MIT OR GPL-3.0"
# fails until the alternative is added to the allow list on purpose, rather
# than being accepted because one branch happens to be permissive.
#
# When cargo-deny is installed it also runs, so the same configuration is
# exercised by its own SPDX parser.
set -uo pipefail
cd "$(dirname "$0")/.."

tmp=${TMPDIR:-.local-results/tmp}
mkdir -p "$tmp"
work=$(mktemp -d "$tmp/oxblend-licence.XXXXXX")
trap 'rm -rf "$work"' EXIT

if ! scripts/third-party-notices > "$work/notices"; then
    printf 'licence: THIRD-PARTY-NOTICES could not be generated\n'
    exit 1
fi
if ! cmp -s "$work/notices" THIRD-PARTY-NOTICES; then
    printf 'licence: THIRD-PARTY-NOTICES is stale; regenerate it with scripts/third-party-notices\n'
    diff -u THIRD-PARTY-NOTICES "$work/notices" | head -60
    exit 1
fi
printf 'licence: THIRD-PARTY-NOTICES matches the vendored licence texts\n'

allow=$(awk '/^allow = \[/{inside=1; next} inside && /^\]/{inside=0} inside' deny.toml \
    | tr -d ' ",' | tr '\n' ' ')
if [ -z "${allow// /}" ]; then
    printf 'licence: deny.toml has no [licenses] allow list\n'
    exit 1
fi

failures=0
while IFS=$'\t' read -r spec declared; do
    if [ -z "$spec" ]; then
        continue
    fi
    case "$spec" in
        oxblend\ *) continue ;;
    esac
    for token in $(printf '%s' "$declared" | sed 's/[()]//g; s/ OR / /g; s/ AND / /g'); do
        if ! printf '%s\n' $allow | grep -qx -- "$token"; then
            printf 'licence: %s declares %s; deny.toml does not allow %s\n' \
                "$spec" "$declared" "$token"
            failures=$((failures + 1))
        fi
    done
done < <(cargo tree --offline --locked -e normal,build --prefix none --format '{p}|{l}' \
    | sed 's/ ([^)]*)|/|/; s/|/\t/')

if [ "$failures" -ne 0 ]; then
    printf 'licence: %s package(s) outside the allow list\n' "$failures"
    exit 1
fi
printf 'licence: every built package is inside the deny.toml allow list\n'

if command -v cargo-deny >/dev/null 2>&1; then
    printf 'licence: cargo deny check licenses\n'
    cargo deny --offline --locked check licenses || exit 1
else
    printf 'licence: cargo-deny is not installed on this host; deny.toml was not executed\n' >&2
fi

printf 'LICENCE EXIT=0\n'
