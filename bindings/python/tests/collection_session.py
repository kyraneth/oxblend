"""L3 source-aware nested collection query through the retained provider seam."""
import argparse
from collections.abc import Mapping
import json
from pathlib import Path
import shutil
import subprocess
import oxblend as ox

p = argparse.ArgumentParser()
p.add_argument('--fixture', type=Path, required=True)
p.add_argument('--cli', type=Path, required=True)
p.add_argument('--out', type=Path, required=True)
a = p.parse_args()
a.out.mkdir(parents=True, exist_ok=False)
source = a.out / 'sources'; source.mkdir()
for path in a.fixture.glob('*.blend'): shutil.copyfile(path, source / path.name)
root = source / 'root.blend'
proc = subprocess.run([str(a.cli), 'query', str(root), '--type', 'Scene', '--fields', 'collection.graph'], capture_output=True)
(a.out / 'cli.stdout').write_bytes(proc.stdout)
(a.out / 'cli.stderr').write_bytes(proc.stderr)
(a.out / 'cli.exit').write_text(str(proc.returncode))
assert proc.returncode == 0
expected = json.loads(proc.stdout)

def thaw(v):
    if isinstance(v, Mapping): return {k: thaw(x) for k, x in v.items()}
    if isinstance(v, tuple): return [thaw(x) for x in v]
    return v

def facts(v):
    if isinstance(v, dict):
        return {k: ({f: x[f] for f in ('path', 'path_hex')} if k == 'provider' else facts(x))
                for k, x in v.items() if k not in ('operation_diagnostics', 'diagnostic_ref')}
    if isinstance(v, list): return [facts(x) for x in v]
    return v

with ox.open(root) as file:
    independent = file.query(type='Scene', fields=['id.name'])
    first = file.query(type='Scene', fields=['collection.graph'])
    assert file.query(type='Scene', fields=['collection.graph']) == first
    actual = thaw(first)
    (a.out / 'binding.json').write_text(json.dumps(actual))
    assert facts(actual) == facts(expected)
    graph = first['graphs']['collection.graph']
    assert graph['complete'] and len(graph['sources']) == 3
    assert len(graph['records']) == 7 and len(graph['edges']) == 6  # Native fixture's saved topology oracle.
    for observation in first['linked_id_observations']:
        provider = observation['provider']
        assert provider['owner'] == 'session_retained_library_owner'
        assert provider['identity_kind'] == 'stat_signature_not_content_hash'
        assert provider['signature'] and provider['decoded_bytes']
    nested = source / 'nested-library.blend'
    original = nested.read_bytes()
    for data in (original + b'controlled changed copy', original):
        nested.write_bytes(data)
        try: file.query(type='Scene', fields=['collection.graph'])
        except ox.SourceChangedError as error:
            assert len(error.diagnostics) == 1
            assert error.diagnostics[0]['kind'] == 'source_check_failure'
            assert bytes.fromhex(error.diagnostics[0]['source_hex']) == bytes(nested)
        else: raise AssertionError('nested source did not latch')
        assert file.query(type='Scene', fields=['id.name']) == independent
assert first.complete
result = {'passed': True, 'schema': first['schema'], 'graph_schema': graph['schema'],
          'sources': len(graph['sources']), 'records': len(graph['records']), 'edges': len(graph['edges']),
          'cli_exit': proc.returncode, 'provider': 'session_retained_library_owner',
          'comparison_exclusion': 'only provider-owner/signature detail (separately asserted), operation diagnostics, run-local diagnostic refs'}
(a.out / 'result.json').write_text(json.dumps(result, indent=2))
print(json.dumps(result))
