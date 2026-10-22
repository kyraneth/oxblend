#!/usr/bin/env python3
"""Real CLI proof: walk attribution, pack detail levels, summary and fetch links."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('paths', ROOT/'scripts/check-paths.py')
paths = importlib.util.module_from_spec(spec)
spec.loader.exec_module(paths)
BINARY = paths.BINARY
with tempfile.TemporaryDirectory(prefix='oxblend-phases-') as temp:
    temp = Path(temp)
    source = temp/'root.blend'
    source.write_bytes(paths.image(b'//asset.png'))
    (temp/'asset.png').write_bytes(b'opaque authored asset')
    def run(level, command):
        directory = temp/(command+str(level))
        argv = [str(BINARY), command, str(source)]
        if command == 'pack':
            argv += [str(temp/('pack-output'+str(level))), '-p', str(temp)]
        else:
            argv += ['--json']
        env = dict(os.environ, FLATTRACE_DIR=str(directory), FLATTRACE_LEVEL=str(level))
        env.pop('FLATTRACE_TRACEPARENT', None)
        result = subprocess.run(argv, env=env, capture_output=True)
        assert result.returncode == 0, (result.returncode, result.stdout, result.stderr)
        files = list(directory.glob('*.jsonl'))
        assert len(files) == 1, files
        rows = [json.loads(line) for line in files[0].read_text().splitlines()]
        return rows
    for command in ['list', 'pack']:
        low = run(1, command)
        high = run(2, command)
        names = lambda rows: {r['event'] for r in rows}
        assert 'walk.file' in names(low)
        assert {'walk.select','walk.edges','walk.assets'} <= names(high)
        assert not {'walk.select','walk.edges','walk.assets'} & names(low)
        assert low[0]['data']['env']['tool.build.commit']
        assert low[0]['data']['env']['service.version']
        if command == 'pack':
            assert not {'pack.copy','pack.repath','fetch'} & names(low)
            assert {'pack.copy','pack.repath','fetch'} <= names(high)
            summaries = [r for r in low if r['event']=='pack.execute' and r['state']=='end']
            assert len(summaries)==1 and summaries[0]['data']['files']==2 and summaries[0]['data']['published_files']==2, summaries
            for row in high:
                if row['event']=='fetch' and row['state']=='start':
                    assert row['parent']==high[0]['span'], row
                    assert row['data']['links'] and row['data']['links'][0]['trace']==row['trace'], row
                    assert row['data']['links'][0]['relationship']=='submitted_by', row
print('PHASES PASS: level-1 walk and execution summaries; level-2 phases, members and causality links')
