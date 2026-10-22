#!/usr/bin/env python3
"""Exercise the real gate on Cargo's repeated, dual-licensed dependency graph."""
from pathlib import Path
import os
import subprocess

ROOT = Path(__file__).resolve().parents[1]
result = subprocess.run([str(ROOT/'scripts/licence-gate.sh')], cwd=ROOT,
                        env=os.environ, capture_output=True)
assert result.returncode == 0, (result.returncode, result.stdout.decode(), result.stderr.decode())
assert b'LICENCE EXIT=0' in result.stdout
print('LICENCE GATE PASS: repeated Unlicense OR MIT dependency stays licence data, not shell glob')
