#!/usr/bin/env python3
"""Pin Release A's one accounted CLI-versus-captured-File admission difference.

Linux /dev/null is a device, not a capturable regular blend source. The CLI
summary reads it and refuses truncated; Python open/summary refuse pack_path
before device open. No FIFO/device policy widening or special case is tested.
Usage: nonregular_admission.py CLI NEW_WORK
"""
from __future__ import annotations
import json
from pathlib import Path
import stat
import subprocess
import sys
import oxblend


def main():
    cli, work = map(Path, sys.argv[1:])
    work.mkdir(parents=True, exist_ok=False)
    source = Path('/dev/null')
    metadata = source.stat()
    assert stat.S_ISCHR(metadata.st_mode), '/dev/null must be the Linux device'
    result = subprocess.run([str(cli), 'summary', str(source)], capture_output=True)
    (work / 'cli.stdout').write_bytes(result.stdout)
    (work / 'cli.stderr').write_bytes(result.stderr)
    (work / 'cli.exit').write_text(str(result.returncode) + '\n')
    assert result.returncode == 2
    assert result.stdout == b''
    envelopes = [json.loads(line) for line in result.stderr.splitlines() if line.startswith(b'{')]
    canonical = next(row['error'] for row in reversed(envelopes) if 'error' in row)
    assert canonical['kind'] == 'truncated'
    assert canonical['message'] == 'range 0+7 exceeds 0 bytes'
    rows = []
    for operation, call in [('open', lambda: oxblend.open(source)),
                            ('summary', lambda: oxblend.summary(source))]:
        try:
            call()
        except oxblend.OxblendError as error:
            assert error.kind == 'pack_path', (operation, error.kind)
            assert error.message == 'pack source is not regular file', (operation, error.message)
            assert error.exit_code == 2
            rows.append({'operation': operation, 'kind': error.kind,
                         'message': error.message, 'exit_code': error.exit_code})
        else:
            raise AssertionError(operation + ' admitted a nonregular source')
    (work / 'result.json').write_text(json.dumps({'complete': True,
        'difference': 'nonregular sources are outside the captured-File contract',
        'cli_error': canonical, 'python': rows}, indent=2) + '\n')
    print('nonregular_admission: CLI truncated; Python open/summary pack_path PASS')


if __name__ == '__main__':
    main()
