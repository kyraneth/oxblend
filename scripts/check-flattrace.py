#!/usr/bin/env python3
"""Exercise every CLI command and check real files with flattrace's reference checker.
Set FLATTRACE_CHECKOUT to a checked-out flattrace v1 source tree. No network.
Branch/payload checks live here; the format remains exclusively flattrace-owned.
"""
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CHECKOUT = Path(os.environ['FLATTRACE_CHECKOUT']).resolve()
TARGET = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')).resolve()
BINARY = Path(os.environ.get('OXBLEND_TEST_BINARY', TARGET / 'release/oxblend')).resolve()
WORK = Path(tempfile.mkdtemp(prefix='flattrace-', dir=os.environ.get('TMPDIR')))
TRACES = WORK / 'traces'
ENV = dict(os.environ, FLATTRACE_DIR=str(TRACES))
ENV.pop('FLATTRACE_TRACEPARENT', None)
ENV.pop('FLATTRACE_LEVEL', None)
CHECK_ENV = dict(os.environ, PYTHONPATH=str(CHECKOUT))
CONTRACT = ROOT / 'flattrace-events.json'

def checker(paths, events=True):
    argv = ['python3', '-m', 'flattrace', 'check']
    if events:
        argv += ['--events', str(CONTRACT)]
    return subprocess.run(argv + list(map(str, paths)), env=CHECK_ENV, cwd=CHECKOUT, capture_output=True)


def run(command, *args, expected=0, events=()):
    before = set(TRACES.glob('*.jsonl'))
    result = subprocess.run([str(BINARY), command, *map(str, args)], env=ENV, capture_output=True)
    label = command + '-' + str(len(list(WORK.glob('*.exit'))))
    (WORK / (label + '.stdout')).write_bytes(result.stdout)
    (WORK / (label + '.stderr')).write_bytes(result.stderr)
    (WORK / (label + '.exit')).write_text(str(result.returncode) + '\n')
    assert result.returncode == expected, (command, result.returncode, result.stderr)
    files = set(TRACES.glob('*.jsonl')) - before
    assert len(files) == 1, files
    checked = checker(files)
    assert checked.returncode == 0, (checked.stdout, checked.stderr)
    rows = [json.loads(line) for line in next(iter(files)).read_text().splitlines()]
    names = {r['event'] for r in rows}
    assert set(events) <= names, (command, set(events) - names)
    assert rows[-1]['event'] == 'run' and rows[-1]['state'] == 'end'
    return result, rows


seed = ROOT / 'tests/fixtures/file-ops/synthetic-empty-seed.blend'
# A hand-derived synthetic header variant exercises format-only edit transport;
# it makes no native-layout or semantic verification claim.
raw = bytearray(seed.read_bytes())
raw[9:12] = b'502'
source = WORK / 'synthetic-502.blend'
source.write_bytes(raw)
result, rows = run('inspect', source, events=('file.open', 'index', 'schema'))
index = json.loads(result.stdout)
scene = next(i for i, b in enumerate(index['blocks']) if b['code_hex'] == '53430000')
for command in ['summary', 'source-facts', 'library-facts', 'trace', 'overrides', 'pointer-census']:
    run(command, source, events=('file.open', 'index', 'schema'))
run('get', source, scene, '["id","name"]', events=('file.open', 'index'))
run('pointer', source, scene, '["camera"]', events=('file.open', 'index'))
run('payload', source, scene, events=('file.open', 'index'))
run('list', source, '--json', events=('file.open', 'index', 'reader.stats'))
run('blocks', source, events=('file.open', 'index'))
run('diff', source, source, events=('file.open', 'index'))
for command in ['pack', 'pack-sequence']:
    args = [source, WORK / command] if command == 'pack' else [source, '-t', WORK / command]
    run(command, *args, '-p', WORK, events=('pack.plan', 'pack.execute', 'pack.repath', 'write', 'publish'))
run('pack', source, WORK / 'dry-run', '-p', WORK, '--dry-run', events=('pack.plan',))
assert not (WORK / 'dry-run').exists()
run('pack', source, WORK / 'archive.zip', '-p', WORK, events=('write', 'publish'))
for command, spec, flag in [('edit', {'schema': 2, 'writes': [], 'bits': []}, '--spec'),
                            ('rewrite', {'schema': 1, 'edits': []}, '--edits')]:
    spec_path = WORK / (command + '.json')
    spec_path.write_text(json.dumps(spec))
    args = [source, '--output', WORK / (command + '.blend'), flag, spec_path]
    if command == 'edit':
        args += ['--mode', 'format-only']
    run(command, *args, events=('file.open', 'index', 'write', 'publish'))
# Hand-authored public EXR header; no pixels or native process.
header = bytearray(struct.pack('<II', 20000630, 2))
def attr(name, kind, value):
    header.extend(name.encode() + b'\0' + kind.encode() + b'\0' + struct.pack('<i', len(value)) + value)
attr('channels', 'chlist', b'R\0' + struct.pack('<i4xii', 2, 1, 1) + b'\0')
for name, kind, value in [('compression', 'compression', b'\3'), ('dataWindow', 'box2i', bytes(16)),
                          ('displayWindow', 'box2i', bytes(16)), ('lineOrder', 'lineOrder', b'\0'),
                          ('pixelAspectRatio', 'float', struct.pack('<f', 1)),
                          ('screenWindowCenter', 'v2f', bytes(8)), ('screenWindowWidth', 'float', struct.pack('<f', 1))]:
    attr(name, kind, value)
header.append(0)
exr = WORK / 'header.exr'
exr.write_bytes(header)
run('exr-header', exr, events=('file.open',))
run('skill')
run('--help')
run('--version')
# Returned refusal and read failure retain correct outcomes and error kinds.
_, refused = run('get', source, scene, '["absent"]', expected=2)
assert refused[-2]['outcome'] == 'error' and refused[-2]['error']['kind'] == 'missing_field'
_, failed = run('summary', WORK / 'absent.blend', expected=2)
assert failed[-1]['outcome'] == 'error'
run('pack', source, WORK / 'pack', '-p', WORK, expected=2)
# Traceparent reuse is at the actual CLI entry, not a writer-only test.
ENV['FLATTRACE_TRACEPARENT'] = '00-11111111111111111111111111111111-2222222222222222-01'
_, child = run('summary', source)
assert child[0]['trace'] == '11111111111111111111111111111111' and child[0]['parent'] == '2222222222222222'
ENV.pop('FLATTRACE_TRACEPARENT')
# Same stdout/stderr/exit when unset, retired variables set, or sink unavailable.
baseline_env = dict(os.environ)
baseline_env.pop('FLATTRACE_DIR', None)
baseline = subprocess.run([str(BINARY), 'summary', str(source)], env=baseline_env, capture_output=True)
blocked = WORK / 'not-a-directory'
blocked.write_text('blocked')
for env in [dict(baseline_env, FLATTRACE_DIR=''),
            dict(baseline_env, OXBLEND_PHASE_TIMING='1', OXBLEND_PHASES='1', OXBLEND_READ_REPORT='1')]:
    actual = subprocess.run([str(BINARY), 'summary', str(source)], env=env, capture_output=True)
    assert (actual.returncode, actual.stdout, actual.stderr) == (baseline.returncode, baseline.stdout, baseline.stderr)
for directory, parent in [(str(blocked), None), (str(WORK / 'bad-context'), 'invalid')]:
    env = dict(baseline_env, FLATTRACE_DIR=directory)
    if parent:
        env['FLATTRACE_TRACEPARENT'] = parent
    actual = subprocess.run([str(BINARY), 'summary', str(source)], env=env, capture_output=True)
    assert (actual.returncode, actual.stdout) == (baseline.returncode, baseline.stdout)
    assert actual.stderr.count(b'flattrace: tracing disabled:') == 1
# Detail levels exercise the actual CLI and the reference event checker.
_, default_level = run('summary', source)
assert default_level[0]['data']['level'] == 1
for raw, level, note in [('', 1, False), ('2', 2, False), ('invalid', 1, True)]:
    ENV['FLATTRACE_LEVEL'] = raw
    _, rows = run('summary', source)
    assert rows[0]['data']['level'] == level
    assert bool(rows[0]['data'].get('notes')) == note
    assert [(r['event'], r['state']) for r in rows] == [(r['event'], r['state']) for r in default_level]
zero = WORK / 'level-zero'
actual = subprocess.run([str(BINARY), 'summary', str(source)],
                        env=dict(ENV, FLATTRACE_DIR=str(zero), FLATTRACE_LEVEL='0', FLATTRACE_TRACEPARENT='invalid'), capture_output=True)
assert (actual.returncode, actual.stdout, actual.stderr) == (baseline.returncode, baseline.stdout, baseline.stderr)
assert not zero.exists()
ENV.pop('FLATTRACE_LEVEL')
# Seed a missing end and a missing command event: checker must reject both.
valid = next(TRACES.glob('*.jsonl')).read_text().splitlines()
for label, lines in [('unfinished', valid[:-1]),
                     ('missing-command', [line for line in valid if json.loads(line)['event'] != json.loads(valid[0])['data']['command']])]:
    path = WORK / (label + '.jsonl')
    path.write_text('\n'.join(lines) + '\n')
    assert checker([path]).returncode == 1
print('FLATTRACE PASS: every command; read/write/archive/dry-run/refusal/parentage/sink/negative gates')
print('Evidence:', WORK)
