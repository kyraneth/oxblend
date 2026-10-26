#!/usr/bin/env python3
"""#62: default trace cardinality follows structural owners, not inner calls.

Run the real CLI at default level and explicit level 2 on the same saved input.
Every hot event must aggregate to at most one record per structural owner at L1;
the exact count must reconcile with the deeper trace. No byte/record allowance.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

HOT = {'query.resolve', 'query.project', 'read.byte_array', 'read.pointer_table', 'decode.frame', 'decode.scan'}
ROOT = Path(__file__).resolve().parents[1]


def check(binary):
    with tempfile.TemporaryDirectory(prefix='oxblend-structure-') as work:
        source = Path(work) / 'image.blend'
        source.write_bytes((ROOT / 'tests/fixtures/library-null/image.blend').read_bytes())
        (Path(work) / 'pixel.png').write_bytes(b'opaque-fixture\n')
        traces = []
        outputs = []
        for level in (None, '2'):
            directory = Path(work) / ('default' if level is None else level)
            env = dict(os.environ, FLATTRACE_DIR=str(directory))
            env.pop('FLATTRACE_LEVEL', None)
            env.pop('FLATTRACE_TRACEPARENT', None)
            if level is not None:
                env['FLATTRACE_LEVEL'] = level
            result = subprocess.run([binary, 'trace', str(source)], env=env, capture_output=True)
            assert result.returncode == 0, (result.returncode, result.stderr)
            outputs.append(result.stdout)
            files = list(directory.glob('*.jsonl'))
            assert len(files) == 1, files
            traces.append([json.loads(line) for line in files[0].read_text().splitlines()])
        assert outputs[0] == outputs[1], 'tracing changed product report'
        light, deep = traces
        assert light[0]['data']['level'] == 1
        owners = {r['span'] for r in light if r['state'] == 'start' and r['event'] not in HOT}
        assert not any(r['event'] in HOT for r in light), 'L1 emits per-inner-call span records'
        summaries = [r for r in light if r['event'] == 'hot.summary']
        assert summaries, 'missing aggregate work evidence'
        keys = [(r['span'], r['data']['event']) for r in summaries]
        assert len(keys) == len(set(keys)), 'multiple summaries per owner/event'
        assert len(summaries) <= len(owners) * len(HOT), 'not bounded by observed owners and declared hot phases'
        for row in summaries:
            data = row['data']
            assert row['span'] in owners and data['event'] in HOT
            assert data['count'] > 0
            assert data['total_ns'] >= data['max_ns'] >= 0
        for event in HOT:
            calls = sum(r['state'] == 'start' and r['event'] == event for r in deep)
            count = sum(r['data']['count'] for r in summaries if r['data']['event'] == event)
            assert calls == count, (event, calls, count)
        print(json.dumps(dict(status='PASS', structural_owners=len(owners), aggregate_records=len(summaries), default_records=len(light), deep_records=len(deep), calls=sum(r['data']['count'] for r in summaries))))


if __name__ == '__main__':
    check(sys.argv[1])
