"""Check the real get entry against a separately reopened RNA array fixture.

Untouched fixture/oracle bytes. No SDNA parsing or saved-pointer decoding.
Every invocation preserves stdout, stderr, exit and flattrace under a NEW output.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--fixture', type=Path, required=True)
p.add_argument('--out', type=Path, required=True)
a = p.parse_args()
a.out.mkdir(parents=True, exist_ok=False)
source = a.fixture / 'arrays.blend'
built = json.loads((a.fixture / 'built.json').read_text())


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


before = sha(source)
assert before == built['sha256'], 'input differs from the native-built oracle'
commands = []


def run(tag, *args, event=None):
    directory = a.out / tag
    directory.mkdir()
    trace = directory / 'traces'
    trace.mkdir()
    argv = ['nice', '-n', '19', 'ionice', '-c', '3', str(a.binary), *map(str, args)]
    with (directory / 'stdout').open('wb') as stdout, (directory / 'stderr').open('wb') as stderr:
        code = subprocess.run(argv, stdout=stdout, stderr=stderr,
                              env=dict(os.environ, FLATTRACE_DIR=str(trace), FLATTRACE_LEVEL='2'), check=False).returncode
    # This oracle intentionally verifies per-call array identities, not the
    # default structural aggregates introduced by #62.
    receipt = {'argv': argv, 'exit': code, 'trace_level': 2}
    (directory / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    commands.append(receipt)
    assert code == 0, {'tag': tag, 'exit': code, 'stderr': str(directory / 'stderr')}
    if event:
        events = [json.loads(line) for path in trace.glob('*.jsonl')
                  for line in path.read_text().splitlines()]
        starts = [e for e in events if e.get('event') == event and e.get('state') == 'start']
        ends = [e for e in events if e.get('event') == event and e.get('state') == 'end']
        assert len(starts) == len(ends) == 1, {'missing_or_duplicate_event': event}
        assert ends[0]['outcome'] == 'ok'
    return json.loads((directory / 'stdout').read_text())


def row(kind, name):
    report = run('select-' + kind, 'query', source, '--type', kind,
                 '--fields', 'id.name,totcol')
    assert report['requested_coverage']['complete']
    matches = [r for r in report['rows'] if r['id']['name'] == name]
    assert len(matches) == 1
    return matches[0]


def targets(tag, record, count):
    result = run(tag, 'get', source, record['source_local_ref']['block_index'],
                 '["mat"]', '--pointer-table', count, event='read.pointer_table')
    assert result['value']['kind'] == 'pointer_table'
    values = result['value']['block_indices']
    assert len(values) == count
    names = []
    for i, index in enumerate(values):
        if index is None:
            names.append(None)
        else:
            value = run(f'{tag}-name-{i}', 'get', source, index, '["id","name"]')['value']
            assert value['kind'] == 'bytes'
            raw = bytes.fromhex(value['hex'])
            assert raw.startswith(b'MA')
            names.append(raw[2:].decode('utf-8'))
    return names


obj = row('Object', built['rna']['object'])
mesh = row('Mesh', built['rna']['mesh'])
object_count = int(obj['values']['totcol']['value']['decimal'])
mesh_count = int(mesh['values']['totcol']['value']['decimal'])
assert object_count == len(built['rna']['slots'])
assert mesh_count == len(built['rna']['mesh_materials'])
value = run('bytes', 'get', source, obj['source_local_ref']['block_index'],
            '["matbits"]', '--byte-array', object_count, event='read.byte_array')['value']
assert value['kind'] == 'bytes'
bits = bytes.fromhex(value['hex'])
assert len(bits) == object_count
assert list(map(bool, bits)) == [s['link'] == 'OBJECT' for s in built['rna']['slots']]
object_names = targets('object-table', obj, object_count)
mesh_names = targets('mesh-table', mesh, mesh_count)
assert mesh_names == built['rna']['mesh_materials']
# RNA exposes active object slots. DATA-owned slots do not expose any unused
# Object.mat cell, so do not invent a native oracle for those hidden bytes.
for i, slot in enumerate(built['rna']['slots']):
    if slot['link'] == 'OBJECT':
        assert object_names[i] == slot['material']
assert sha(source) == before, 'reader changed the fixture'
summary = {'schema': 'oxblend.array-native-check/1', 'binary_sha256': sha(a.binary),
           'input_sha256': before, 'native_version': built['rna']['native_version'],
           'native_build': built['rna']['native_build'], 'input_unchanged': True,
           'object_slot_bytes_hex': bits.hex(), 'object_table': object_names,
           'mesh_table': mesh_names, 'commands': len(commands), 'passed': True}
(a.out / 'result.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary))
