"""Check an installed one-session native trace, not Python harness spans."""
import argparse
import collections
import json
from pathlib import Path
import subprocess
import sys

p = argparse.ArgumentParser()
p.add_argument('--traces', type=Path, required=True)
p.add_argument('--events', type=Path, required=True)
p.add_argument('--checker', type=Path, required=True)
p.add_argument('--semantic-checker', type=Path, required=True)
p.add_argument('--commit', required=True)
p.add_argument('--out', type=Path, required=True)
a = p.parse_args()
a.out.mkdir(parents=True, exist_ok=False)
paths = []
for path in a.traces.glob('*.jsonl'):
    with path.open() as stream: first = json.loads(next(stream))
    if first.get('tool') == 'oxblend': paths.append(path)
assert len(paths) == 1, paths
path = paths[0]
rows = [json.loads(line) for line in path.read_text().splitlines()]
starts = {r['span']: r for r in rows if r['state'] == 'start'}
calls = [r for r in starts.values() if r['event'] == 'python.call']
required = {'open': 'session.open', 'get': 'session.read', 'summary': 'session.read',
            'query': 'session.query', 'trace': 'session.trace', 'pack': 'session.pack'}
seen = collections.Counter(c['data']['operation'] for c in calls)
assert required.keys() <= seen.keys(), ('missing native calls', seen)
for call in calls:
    data = call['data']
    assert data['build_commit'] == a.commit, data
    assert data['native_module']['build_commit'] == a.commit
    assert data['run_identity']['root_span'] == rows[0]['span']
    if data['operation'] not in required: continue
    descendants = []
    for row in starts.values():
        parent = row['parent']
        while parent in starts and parent != call['span']:
            parent = starts[parent]['parent']
        if parent == call['span']: descendants.append(row['event'])
    assert required[data['operation']] in descendants, (data['operation'], descendants)
    if data['operation'] == 'open': assert {'snapshot', 'index', 'schema'} <= set(descendants)
assert sum(r['event'] == 'snapshot' for r in starts.values()) == 1
assert any(r['event'] == 'reader.reuse' for r in starts.values())
declarations = json.loads(a.events.read_text())
assert all(r['event'] in declarations['events'] for r in rows)

def run(name, argv):
    proc = subprocess.run(list(map(str, argv)), capture_output=True)
    (a.out / (name + '.stdout')).write_bytes(proc.stdout)
    (a.out / (name + '.stderr')).write_bytes(proc.stderr)
    (a.out / (name + '.receipt.json')).write_text(json.dumps({'argv': list(map(str, argv)), 'exit': proc.returncode}))
    return proc.returncode

assert run('reference', [a.checker, 'check', '--events', a.events, path]) == 0
selection = a.out / 'selection.json'
selection.write_text(json.dumps({'command': 'library', 'mode': 'report', 'scenario': 'success'}))
assert run('semantic', [sys.executable, a.semantic_checker, path, '--selection', selection]) == 0
# Same real trace, one deliberately omitted declaration: must not pass.
del declarations['events']['python.call']
negative_events = a.out / 'missing-python-call.events.json'
negative_events.write_text(json.dumps(declarations))
assert run('missing-declaration-negative', [a.checker, 'check', '--events', negative_events, path]) != 0
mutated = json.loads(json.dumps(rows))
next(r for r in mutated if r['event'] == 'python.call' and r['state'] == 'start')['parent'] = '0000000000000000'
negative_trace = a.out / 'wrong-parent.jsonl'
negative_trace.write_text(''.join(json.dumps(r) + '\n' for r in mutated))
assert run('wrong-parent-negative', [a.checker, 'check', '--events', a.events, negative_trace]) != 0
result = {'passed': True, 'calls': dict(seen), 'trace': str(path), 'reference_exit': 0, 'semantic_exit': 0,
          'missing_declaration_rejected': True, 'wrong_parent_rejected': True, 'commit': a.commit}
(a.out / 'result.json').write_text(json.dumps(result, indent=2))
print(json.dumps(result))
