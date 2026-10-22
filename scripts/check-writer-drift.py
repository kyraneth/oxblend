#!/usr/bin/env python3
"""Compare the complete vendored writer body to local flattrace master, offline.
Only rustfmt, product lint/header and the three documented visibility changes
are adapters. Every other difference (including comments) is drift.
"""
import argparse
import difflib
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--upstream', type=Path, default=os.environ.get('FLATTRACE_CHECKOUT'))
parser.add_argument('--vendored', type=Path, default=ROOT/'src/flattrace.rs')
args = parser.parse_args()
if args.upstream is None:
    parser.error('set FLATTRACE_CHECKOUT or supply --upstream (local checkout; no network)')


def normalize(source):
    source = source[source.index('// SPDX-License-Identifier: MIT'):].split('#[cfg(test)]')[0]
    source = source.replace('pub(super) fn level_from_env(', 'fn level_from_env(')
    source = source.replace('pub(super) fn warn(', 'fn warn(')
    source = source.replace('pub(super) fn warn_text(', 'fn warn_text(')
    return subprocess.check_output(['rustfmt', '--edition', '2024', '--emit', 'stdout'], input=source.encode()).decode()


upstream = subprocess.check_output(['git', '-C', str(args.upstream), 'show', 'master:crates/flattrace/src/lib.rs']).decode()
revision = subprocess.check_output(['git', '-C', str(args.upstream), 'rev-parse', 'master']).decode().strip()
expected = normalize(upstream)
actual = normalize(args.vendored.read_text())
if expected != actual:
    print('WRITER DRIFT against upstream master', revision)
    print(''.join(difflib.unified_diff(expected.splitlines(True), actual.splitlines(True), fromfile='upstream master', tofile='vendored')))
    raise SystemExit(1)
print('WRITER MATCH: complete body; upstream master', revision)
