#!/usr/bin/env python3
"""Exercise qualification's real run function with a non-GNU time executable."""
import argparse
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--script', type=Path, default=ROOT/'scripts/qualify.sh')
a = p.parse_args()
with tempfile.TemporaryDirectory(prefix='oxblend-timer-') as directory:
    work = Path(directory)
    fake = work/'time'
    fake.write_text('#!/bin/sh\nexit 64\n')
    fake.chmod(0o755)
    source = a.script.read_text()
    function = source[source.index('run() {'):source.index('run build ')]
    # The baseline hard-coded path is redirected in this disposable copy only;
    # candidate uses the documented OXBLEND_TIME override instead.
    function = function.replace('/usr/bin/time', str(fake))
    setup = 'set -uo pipefail\ncollect_all=0; status=0\nlogs="$1"; trace_root="$1"; FLATTRACE_LEVEL=0; OXBLEND_TIME="$2"\ncheck_traces() { return 0; }\n'
    result = subprocess.run(['bash', '-c', setup+function+'\nrun probe bash -c "exit 7"\n', 'probe', str(work), str(fake)], capture_output=True)
    assert result.returncode == 7, (result.returncode, result.stdout, result.stderr)
    assert 'without resource metrics' in (work/'probe.time.txt').read_text()
    assert 'EXIT=7' in (work/'probe.log').read_text()
print('TIME PASS: non-GNU timer degrades explicitly, preserving stage execution and exit')
