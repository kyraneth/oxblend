#!/usr/bin/env python3
"""Exercise every CLI command and check real files with flattrace's reference checker.
Set FLATTRACE_CHECKOUT to a checked-out flattrace v1 source tree. No network.
Branch/payload checks live here; the format remains exclusively flattrace-owned.
"""
import json
import importlib.util
import os
from pathlib import Path
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location('oxblend_trace_gate', ROOT / 'scripts/check-flattrace.py')
_gate = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_gate)
CHECKOUT = Path(os.environ['FLATTRACE_CHECKOUT']).resolve()
TARGET = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')).resolve()
BINARY = Path(os.environ.get('OXBLEND_TEST_BINARY', TARGET / 'release/oxblend')).resolve()
WORK = Path(tempfile.mkdtemp(prefix='flattrace-', dir=os.environ.get('TMPDIR')))
TRACES = WORK / 'traces'
print('evidence:', WORK, flush=True)
ENV = dict(os.environ, FLATTRACE_DIR=str(TRACES))
ENV.pop('FLATTRACE_TRACEPARENT', None)
ENV.pop('FLATTRACE_LEVEL', None)
CHECK_ENV = dict(os.environ, PYTHONPATH=str(CHECKOUT))
CONTRACT = ROOT / 'flattrace-events.json'
# Derive the actual dispatcher vocabulary; manifest drift cannot silently pass.
import re
dispatcher = (ROOT / 'src/bat_cli.rs').read_text().split('const COMMANDS:')[1].split('];')[0]
actual_commands = set(re.findall(r'"([a-z-]+)"', dispatcher)) | {'help','version','invalid','library'}
assert actual_commands == set(json.loads(_gate.MANIFEST.read_text())['commands']), 'CLI command-set drift'

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
    names.update(r['data']['event'] for r in rows if r['event'] == 'hot.summary' and r['data']['count'] > 0)
    for event, details in json.loads(_gate.MANIFEST.read_text()).get('work_aliases', {}).items():
        if names.intersection(details): names.add(event)
    assert set(events) <= names, (command, set(events) - names)
    assert rows[-1]['event'] == 'run' and rows[-1]['state'] == 'end'
    root_command = rows[0]['data']['command']
    # Product stderr is independent of the diagnostic trace being checked.
    envelopes = []
    for line in result.stderr.splitlines():
        try: value = json.loads(line)
        except ValueError: continue
        if isinstance(value,dict) and isinstance(value.get('error'),dict): envelopes.append(value)
    terminal_kind = envelopes[-1]['error']['kind'] if envelopes else ''
    if expected == 0: scenario = 'success'
    elif terminal_kind == 'usage': scenario = 'usage'
    elif terminal_kind == 'invalid_sdna': scenario = 'sdna_error'
    elif root_command == 'summary' and args and not Path(args[0]).exists(): scenario = 'open_error'
    else: scenario = 'operation_error'
    conditions = {'dry_run': '--dry-run' in args}
    if root_command == 'help' and args and args[0] == 'error': conditions['error_help'] = True
    if root_command == 'report':
        conditions['report_preview'] = '--preview' in args
        conditions['report_create'] = '--create' in args
    if 'publish' in events: conditions['publish'] = True
    if 'pack.execute' in events: conditions['pack_execute'] = True
    for event, condition in [('decode','compressed'),('hash','hash'),('copy','copy'),('durability','durability'),('query.resolve','resolve')]:
        if event in events: conditions[condition] = True
    requested_flags = list(args[:args.index('--')]) if '--' in args else list(args)
    mode_arg = next((a for a in requested_flags if a in ['--strict','--permissive','--non-strict']), '--strict')
    effective = ('strict' if mode_arg == '--strict' else 'permissive') if root_command in ['pack','pack-sequence'] else 'report'
    selection = dict(command=root_command, mode=effective,
                     scenario=scenario, conditions=conditions,
                     error_kind=terminal_kind, errors=[terminal_kind] if terminal_kind else [],
                     level=int(ENV.get('FLATTRACE_LEVEL','1')) if ENV.get('FLATTRACE_LEVEL','1').isdigit() else 1)
    if expected == 0:
        if root_command == 'explain':
            if '--file' in args:
                selection['inputs'] = [str(args[args.index('--file')+1])]
                conditions['file_query'] = True
        elif root_command in ['exr-header','exr-pixels','exr-scan']:
            selection['inputs'] = [dict(path=str(args[0]),schema=False)]
        elif root_command not in ['catalog','skill','help','version','report']:
            selection['inputs'] = [str(args[0])] if root_command != 'diff' else [str(args[0]),str(args[1])]
        if root_command == 'query':
            conditions['query_project'] = bool(json.loads(result.stdout).get('rows'))
    # Independently parse final directory-member effects. ZIP entries are
    # archive members, not final filesystem paths; its final archive is checked
    # by its own publication span rather than matching staging names.
    if root_command in ('pack','pack-sequence') and result.stdout:
        report = json.loads(result.stdout)
        selection['skip_observations'] = [dict(item=f'/skipped_items/{i}',kind=item.get('kind','skipped'),reason=item.get('reason',item.get('message','report_declared_skip'))) for i,item in enumerate(report.get('skipped_items',[]))]
        selection['decisions'] = [dict(kind='excluded' if item.get('status')=='excluded' else item.get('kind','unknown'),reason=item.get('reason',item.get('message','report_declared_skip'))) for item in report.get('skipped_items',[])]
        selection['effects'] = [dict(role='pack_member',path_hex=f['output_path_hex'],output_written=f['output_written'],durability_completed=f['durability_completed']) for f in report['files']]
        if report.get('pack_info'):
            info = report['pack_info']
            selection['effects'].append(dict(role='pack_info',path_hex=info['output_path_hex'],output_written=info['output_written'],durability_completed=info['durability_completed']))
        if report['archive']:
            selection['effects'].append(dict(role='pack_archive',path_hex=report['directory_path_hex'],output_written=report['archive_written'],durability_completed=report['durability_completed']))
        if not report['archive']:
            selection['publications'] = [dict(path=os.fsdecode(bytes.fromhex(f['output_path_hex'])),output_written=True) for f in report['files'] if f['output_written']]
            if (report.get('pack_info') or {}).get('output_written'):
                selection['publications'].append(dict(path=os.fsdecode(bytes.fromhex(report['pack_info']['output_path_hex'])),output_written=True))
        elif report['archive_written']:
            selection['publications'] = [dict(path=os.fsdecode(bytes.fromhex(report['directory_path_hex'])),output_written=True)]
    if root_command == 'edit' and '--mode' in args and args[args.index('--mode')+1] == 'node-tree':
        if expected == 0:
            conditions['node_tree_edit'] = True
            spec = json.loads(Path(args[args.index('--spec')+1]).read_text())
            selection['node_tree_operations'] = spec['operations']
            report = json.loads(result.stdout)
            assert report['operations'] == spec['operations']
            selection['publications'] = [dict(path=str(args[args.index('--output')+1]), output_written=True)]
        else:
            assert not Path(args[args.index('--output')+1]).exists()
            assert not any(r['event'] in ['write','publish'] for r in rows)
    requested = None
    if root_command in ('trace','list','overrides','pack','pack-sequence'):
        for arg in args:
            if arg == '--': break
            if arg in ('--strict','--permissive','--non-strict'):
                requested = 'strict' if arg == '--strict' else 'permissive'
                break
    selection['root_attributes'] = dict(requested_mode=requested or 'strict',mode_source='explicit' if requested else 'default')
    (WORK / (label + '.selection.json')).write_text(json.dumps(selection, indent=2))
    _gate.check(rows, selection)
    # Every propagated error requires its own exact span-local cause point.
    propagated = next((i for i,r in enumerate(rows) if r['event'] == 'error' and r['data'].get('source') == 'span_result'), None)
    if propagated is not None:
        mutated = json.loads(json.dumps(rows))
        del mutated[propagated]
        for seq,row in enumerate(mutated): row['seq'] = seq
        try: _gate.check(mutated, selection)
        except ValueError: pass
        else: raise AssertionError('removed propagated error-kind point was accepted')
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
# Preserve integration/next f82aa4f's query receiving scenarios.
run('catalog', '--json', events=('catalog.evaluate',))
run('explain', 'scene.compositor', '--file', source, '--json', events=('explain.evaluate', 'file.open'))
run('explain', 'not_a_concept', '--json', expected=2, events=('explain.evaluate',))
run('query', source, '--type', 'Scene', '--fields', 'r.sfra,r.efra',
    events=('query.evaluate', 'query.select', 'query.project', 'file.open'))
run('query', source, '--type', 'Scene', '--fields', 'r.sfra,no_such_field', expected=2,
    events=('query.evaluate', 'query.project'))
for command in ['summary', 'source-facts', 'library-facts', 'trace', 'overrides', 'pointer-census']:
    run(command, source, events=('file.open', 'index', 'schema'))
run('get', source, scene, '["id","name"]', events=('file.open', 'index'))
run('pointer', source, scene, '["camera"]', events=('file.open', 'index'))
run('payload', source, scene, events=('file.open', 'index'))
run('list', source, '--json', events=('file.open', 'index', 'reader.stats'))
run('blocks', source, events=('file.open', 'index'))
_, diff_rows = run('diff', source, source, events=('file.open', 'index'))
diff_selection = dict(command='diff',mode='report',scenario='success',inputs=[str(source),str(source)])
second_schema = [r['span'] for r in diff_rows if r['event']=='schema' and r['state']=='start'][1]
mutated = [json.loads(json.dumps(r)) for r in diff_rows if r['span'] != second_schema]
for seq,row in enumerate(mutated): row['seq']=seq
try: _gate.check(mutated,diff_selection)
except ValueError: pass
else: raise AssertionError('second-input SDNA omission accepted')
ENV['FLATTRACE_LEVEL'] = '2'
for command in ['pack', 'pack-sequence']:
    args = [source, WORK / command] if command == 'pack' else [source, '-t', WORK / command]
    run(command, *args, '-p', WORK, events=('pack.plan', 'pack.execute', 'pack.repath', 'write', 'publish'))
ENV.pop('FLATTRACE_LEVEL')
run('pack', source, WORK / 'dry-run', '-p', WORK, '--dry-run', events=('pack.plan',))
assert not (WORK / 'dry-run').exists()
run('pack', source, WORK / 'archive.zip', '-p', WORK, events=('write', 'publish'))
ENV['FLATTRACE_LEVEL'] = '2'
for command, spec, flag in [('edit', {'schema': 2, 'writes': [], 'bits': []}, '--spec'),
                            ('rewrite', {'schema': 1, 'edits': []}, '--edits')]:
    spec_path = WORK / (command + '.json')
    spec_path.write_text(json.dumps(spec))
    args = [source, '--output', WORK / (command + '.blend'), flag, spec_path]
    if command == 'edit':
        args += ['--mode', 'format-only']
    run(command, *args, events=('file.open', 'index', 'write', 'publish'))
ENV.pop('FLATTRACE_LEVEL')
# Partial-file success and named reference refusal use the same source snapshot.
extract_source = ROOT / 'tests/fixtures/extract/native-5.2.1.blend'
run('extract', extract_source, '--id', 'NTClosureProbe', '--clear', 'CompositorNodeRLayers.scene', '--output', WORK/'extract.blend',
    events=('extract.select','extract.closure','extract.publish','write','publish','serialize'))
run('extract', extract_source, '--id', 'NTClosureProbe', '--output', WORK/'extract-refused.blend', expected=2,
    events=('extract.select','extract.closure','error'))
assert not (WORK/'extract-refused.blend').exists()
# The immutable native template is a pinned saved input, never overwritten.
node_source = ROOT / 'assets/template-5.2.1.blend'
initial, _ = run('node-tree', node_source, events=('node_tree_inspect','serialize','root.summary'))
tree = json.loads(initial.stdout)['trees'][0]
for label, operations in [('empty', []), ('add', [{'op':'add_node','node_type':'NodeReroute','name':'Trace Added'}])]:
    spec = WORK / ('node-' + label + '.json')
    spec.write_text(json.dumps(dict(schema=1, tree=tree['block_index'], expected_graph=tree['graph'], operations=operations)))
    _, node_rows = run('edit', node_source, '--output', WORK/('node-' + label + '.blend'), '--spec', spec, '--mode', 'node-tree',
                      events=('node_tree_edit','node_tree.plan','node_tree.validate','node_tree.emit','node_tree.verify','write','publish','serialize'))
    selection = json.loads((WORK / ('edit-' + str(len(list(WORK.glob('*.exit')))-1) + '.selection.json')).read_text())
    if operations:
        for omitted in ['node_tree.apply','node_tree.operation','node_tree.verify']:
            mutant = [json.loads(json.dumps(r)) for r in node_rows if r['event'] != omitted]
            for seq, row in enumerate(mutant): row['seq'] = seq
            try: _gate.check(mutant, selection)
            except ValueError: pass
            else: raise AssertionError(('node-tree phase/operation omission accepted', omitted))
for label, operation in [
    ('constructor',dict(op='add_node',node_type='GeometryNodeJoinGeometry',name='No')),
    ('endpoint',dict(op='add_link', **{'from':dict(node='Missing',socket='Color',socket_type='NodeSocketColor'),'to':dict(node='Output',socket='Socket_0',socket_type='NodeSocketColor')})),
    ('type',dict(op='add_link', **{'from':dict(node='Source',socket='Color',socket_type='NodeSocketFloat'),'to':dict(node='Math',socket='Value_001',socket_type='NodeSocketFloat')}))]:
    spec = WORK / ('node-refuse-' + label + '.json')
    spec.write_text(json.dumps(dict(schema=1,tree=tree['block_index'],expected_graph=tree['graph'],operations=[operation])))
    run('edit',node_source,'--output',WORK/('node-refuse-'+label+'.blend'),'--spec',spec,'--mode','node-tree',expected=2,events=('node_tree.apply','error'))
run('node-tree', source, expected=2, events=('node_tree_inspect','error'))
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
# Preserve The pixel refusal/decode and detail-parentage receiving scenarios.
# A complete, raw-stored ZIP pixel chunk (compression not beneficial for 1x1).
pixel = WORK / 'pixel.exr'
body = header + struct.pack('<Q', len(header)+8) + struct.pack('<iif', 0, 4, 0.25)
pixel.write_bytes(body)
_, pixels_rows = run('exr-pixels', pixel, '--part', 0, '--pass', '', '--output', WORK/'pixel.f32',
                     events=('file.open','exr.pixels','exr.select','exr.decode','exr.verify','write','publish','exr.output','root.summary'))
summary = next(r for r in pixels_rows if r['event']=='root.summary')
# Point records are ON an active span (not new child spans): parent is
# inherited from that enclosing span, so root points share root.span.
assert summary['span'] == pixels_rows[0]['span'] and summary['state']=='point'
assert summary.get('parent') == pixels_rows[0].get('parent')
assert isinstance(summary['data']['outputs'],list) and isinstance(summary['data']['resource'],dict)
assert summary['data']['diagnostics_complete'] is True
_, refused_rows = run('exr-pixels', pixel, '--part', 0, '--pass', 'absent', '--output', WORK/'refused.f32',
                      expected=2, events=('exr.select','root.summary'))
assert not (WORK/'refused.f32').exists()
assert not any(r['event'] in ('write','publish') for r in refused_rows)
assert any((r.get('error') or {}).get('kind')=='exr_selection' for r in refused_rows)
# Structure is complete but the ZIP payload is invalid: no garbage publication.
pixel.write_bytes(header + struct.pack('<Q',len(header)+8) + struct.pack('<ii',0,5) + bytes(5))
_, corrupt_rows = run('exr-pixels',pixel,'--part',0,'--pass','','--output',WORK/'corrupt.f32',
                      expected=2,events=('exr.decode','root.summary'))
assert not (WORK/'corrupt.f32').exists()
assert any((r.get('error') or {}).get('kind')=='exr_decode' for r in corrupt_rows)
_, scan_level1 = run('exr-scan', exr, '--jobs', '1', events=('file.open',))
assert not any(r['event'] == 'exr.file' for r in scan_level1)
ENV['FLATTRACE_LEVEL'] = '2'
_, scan_level2 = run('exr-scan', exr, '--jobs', '1', events=('exr.file', 'file.open'))
scan_parent = next(r['span'] for r in scan_level2 if r['event'] == 'exr-scan' and r['state'] == 'start')
assert all(r['parent'] == scan_parent for r in scan_level2 if r['event'] == 'exr.file' and r['state'] == 'start')
ENV.pop('FLATTRACE_LEVEL')
run('skill')
run('skill', 'print')
run('--help')
run('help', 'error', 'io', '--json', events=('error.help',))
run('help', 'error', 'not-a-builtin-kind', '--json', expected=2, events=('error.help',))
saved = WORK / 'saved-error.json'
saved.write_text(json.dumps({'schema':1,'error':{'kind':'io','message':'local-only'}}))
report_output = WORK / 'reviewed.zip'
preview, report_rows = run('report', '--report', saved, '--output', report_output, '--preview', events=('report.snapshot',))
reviewed = json.loads(preview.stdout)
run('report', '--create', reviewed['receipt_path'], '--preview-sha256', reviewed['receipt_sha256'], '--output', report_output, events=('write','publish'))
preview, _ = run('report', '--report', saved, '--output', WORK/'discarded.zip', '--preview', events=('report.snapshot',))
reviewed = json.loads(preview.stdout)
run('report', '--discard-preview', reviewed['receipt_path'], '--preview-sha256', reviewed['receipt_sha256'])
# Seeded omission must fail the selected preview phase, not a union-of-log names.
removed = {r['span']:r['parent'] for r in report_rows if r['event']=='report.snapshot' and r['state']=='start'}
without_snapshot = [json.loads(json.dumps(r)) for r in report_rows if r['span'] not in removed]
for seq, row in enumerate(without_snapshot, start=report_rows[0]['seq']):
    row['seq'] = seq
    if row['parent'] in removed: row['parent'] = removed[row['parent']]
try: _gate.check(without_snapshot, dict(command='report',mode='report',scenario='success',conditions={'report_preview':True}))
except ValueError as error: assert 'report.snapshot' in str(error), str(error)
else: raise AssertionError('report snapshot omission accepted')
run('--version')
run('not-a-command', expected=2)
run('get', expected=2)
# Actual selected conditional scenarios, not optional prose.
import gzip, shutil
compressed = WORK / 'compressed.blend'
compressed.write_bytes(gzip.compress(source.read_bytes()))
for cmd in ['summary','trace','list','blocks']:
    run(cmd, compressed, events=('decode',))
damaged = bytearray(source.read_bytes())
dna = next(b for b in index['blocks'] if b['code_hex'] == '444e4131')
damaged[dna['data_offset']:dna['data_offset']+4] = b'BAD!'
bad_sdna = WORK / 'bad-sdna.blend'
bad_sdna.write_bytes(damaged)
run('summary', bad_sdna, expected=2)
assets = WORK / 'assets'; assets.mkdir()
image = assets / 'image.blend'
shutil.copyfile(ROOT / 'tests/fixtures/library-null/image.blend', image)
(assets / 'pixel.png').write_bytes(b'opaque-copy-fixture\\n')
run('list', image, '--sha256', events=('hash','query.resolve'))
run('pack', image, WORK / 'asset-pack', '-p', assets, '--durability','each-file',events=('copy','durability','publish'))
run('pack', image, WORK / 'asset-pack-end', '-p', assets, '--durability','end',events=('durability','publish'))
run('pack', image, WORK / 'asset-archive-end.zip', '-p', assets, '--durability','end',events=('durability','publish'))
run('pack', image, WORK / 'excluded-pack', '-p', assets,'--exclude','pixel.png',events=('decision',))
for mode in ['--strict','--permissive','--non-strict']:
    for cmd in ['trace','list','overrides']:
        run(cmd, image, mode)
    run('pack', image, WORK / ('mode-' + mode[2:]), '-p', assets, '--dry-run', mode)
# Essential scenarios must satisfy the same executable semantics at both levels.
for level in ['1','2']:
    ENV['FLATTRACE_LEVEL'] = level
    for cmd in ['trace','list']:
        run(cmd, source)
    run('pack', source, WORK / ('both-levels-' + level), '-p', WORK)
ENV.pop('FLATTRACE_LEVEL')
# Existing 0.1 export with explicit reviewed plain/hex additions. This proves
# the declared canary workflow, not automatic anonymization of arbitrary logs.
canary = 'G7_PRIVATE_CANARY'
canary_input = WORK / (canary + '.blend')
shutil.copyfile(source, canary_input)
_, canary_rows = run('summary', canary_input)
canary_trace = next(TRACES.glob('*' + canary_rows[0]['run'] + '.jsonl'))
exported = WORK / 'reviewed-export'
export_result = subprocess.run(['python3','-m','flattrace','export',str(canary_trace),'--out',str(exported),'--path',canary,'--path',canary.encode().hex()],env=CHECK_ENV,cwd=CHECKOUT,capture_output=True)
assert export_result.returncode == 0, (export_result.stdout,export_result.stderr)
export_bytes = b''.join(p.read_bytes() for p in exported.glob('*.jsonl'))
assert canary.encode() not in export_bytes and canary.encode().hex().encode() not in export_bytes
# Returned refusal and read failure retain correct outcomes and error kinds.
_, refused = run('get', source, scene, '["absent"]', expected=2)
assert any(r.get('error', {}).get('kind') == 'missing_field' for r in refused)
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
    if level == 1:
        assert [(r['event'], r['state']) for r in rows] == [(r['event'], r['state']) for r in default_level]
    else:
        assert any(r['event'] == 'query.resolve' and r['state'] == 'start' for r in rows)
        assert not any(r['event'] == 'hot.summary' for r in rows)
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
_gate.selftest()
# Real positive control mutations repair seq; semantic coverage, not a seq gap.
control = default_level
selection = dict(command='summary',mode='report',scenario='success')
for label in ['schema', 'inputs', 'mode']:
    mutated = json.loads(json.dumps(control))
    if label == 'schema': mutated = [r for r in mutated if r['event'] != 'schema']
    elif label == 'inputs': del mutated[0]['data']['inputs']
    else: mutated[0]['data']['mode'] = 'permissive'
    for seq, row in enumerate(mutated): row['seq'] = seq
    try: _gate.check(mutated, selection)
    except ValueError: pass
    else: raise AssertionError(('semantic mutation accepted', label))
    (WORK / ('mutant-' + label + '.jsonl')).write_text(''.join(json.dumps(r)+'\\n' for r in mutated))
print('FLATTRACE PASS: every command; read/write/archive/dry-run/refusal/parentage/sink/negative gates')
print('Evidence:', WORK)
