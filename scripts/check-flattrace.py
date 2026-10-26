#!/usr/bin/env python3
"""Oxblend semantic gate over the unchanged flattrace 0.1/v1 envelope."""
import argparse
import copy
import json
import os
from pathlib import Path

MANIFEST = Path(__file__).resolve().parents[1] / 'docs/flattrace-manifest.json'


def require(ok, message):
    if not ok:
        raise ValueError(message)


def attributes(data, spec):
    types = {'string': str, 'object': dict, 'array': list, 'boolean': bool,
             'number': (float, int)}
    for key, typ in spec.items():
        require(key in data and (data[key] is None if typ == 'null' else type(data[key]) in (types[typ] if isinstance(types[typ], tuple) else (types[typ],))),
                f'missing/wrong attribute {key}: expected {typ}')
        if typ == 'string' and key in ['kind','reason','item','algorithm','source','scope']:
            require(bool(data[key].strip()), f'empty attribute: {key}')


def matches(pattern, path):
    if not pattern:
        return not path
    if pattern[0] == '**':
        return any(matches(pattern[1:], path[i:]) for i in range(len(path) + 1))
    return bool(path) and pattern[0] == path[0] and matches(pattern[1:], path[1:])


def check(rows, selection, manifest=None):
    m = manifest or json.loads(MANIFEST.read_text())
    command = selection['command']
    require(command in m['commands'], f'unregistered command: {command}')
    contract = m['commands'][command]
    require(selection['mode'] in contract['modes'], 'unregistered mode')
    scenario = selection['scenario']
    require(scenario in contract['scenarios'], 'unregistered scenario')
    requirements = contract['success'] if scenario == 'success' else m['scenarios'][scenario]
    ordered = list(requirements.get('ordered', []))
    event_groups = [list(requirements.get('events', [])), selection.get('events', [])]
    forbidden = list(requirements.get('forbidden', []))
    errors = list(requirements.get('errors', [])) + selection.get('errors', [])
    for condition, enabled in selection.get('conditions', {}).items():
        require(condition in m['conditions'], f'unregistered condition: {condition}')
        require(isinstance(enabled, bool), f'condition must be executable boolean: {condition}')
        if enabled:
            rule = m['conditions'][condition]
            # Conditions are ordered independently, not after the main list.
            event_groups.append(rule.get('events', []))
            forbidden += rule.get('forbidden', [])
    active, starts, points, kinds = {}, [], [], []
    root = None
    aggregate_paths = []
    aggregate_keys = set()
    for seq, row in enumerate(rows):
        require(row['v'] == 1 and row['seq'] == seq, 'envelope/sequence')
        state, span, event = row['state'], row['span'], row['event']
        require(isinstance(row['data'], dict), 'nonobject payload')
        phase = m.get('phases', {}).get(event, {})
        if state == 'start' and 'min_level' in phase:
            require(root is not None and root['data'].get('level', 1) >= phase['min_level'], 'per-call hot span below explicit detail level')
        attributes(row['data'], phase.get(state, {}))
        if state == 'end' and row.get('outcome') == 'ok': attributes(row['data'], phase.get('ok_end', {}))
        if state == 'start' and phase.get('start_any'):
            alternatives = []
            for variant in phase['start_any']:
                try: attributes(row['data'], variant); alternatives.append(True)
                except ValueError: pass
            require(bool(alternatives), f'missing phase identity: {event}')
        require(event not in forbidden, f'forbidden phase: {event}')
        if root is not None:
            require(row['trace'] == root['trace'] and row['run'] == root['run'], 'mixed runs')
        if state == 'start':
            require(span not in active and not any(r['span'] == span for r, _ in starts), 'duplicate start')
            if root is None:
                require(event == 'run', 'root must start first')
                root = row
                attributes(row['data'], m['root']['start'])
                require(row['data']['command'] == command, 'command mismatch')
                require(row['data']['mode'] == selection['mode'], 'mode mismatch')
                require(row['data']['requested_mode'] in ['strict','permissive'], 'requested mode vocabulary')
                require(row['data']['mode_source'] in ['default','explicit'], 'mode source vocabulary')
                path = ['run']
            else:
                require(row['parent'] in active, 'inactive/wrong parent')
                path = active[row['parent']][1] + [event]
                if command != 'library' and event == 'schema':
                    require(active[row['parent']][0]['event'] == 'index', 'SDNA outside index')
                if command != 'library' and event == 'index':
                    require(active[row['parent']][0]['event'] in ['file.open','snapshot','memory.capture'], 'index outside capture owner')
                if command != 'library' and event == 'file.open':
                    require(command in path or (len(path)>1 and path[1]=='fetch') or (command == 'exr-scan' and path == ['run','file.open']), 'open outside command/linked fetch/EXR worker')
            active[span] = (row, path)
            starts.append((row, path))
        elif state == 'end':
            require(span in active, 'duplicate/unpaired end')
            start, _ = active[span]
            require(start['event'] == event and start['parent'] == row['parent'], 'end identity mismatch')
            require(not any(s['parent'] == span for s, _ in active.values()), 'end before child')
            require(row.get('outcome') in ['ok','error','refused','partial','cancelled','unknown'], 'outcome')
            if scenario == 'success' and not (command == 'library' and event == 'run'):
                require(row['outcome'] != 'unknown', f'unknown completed phase in success: {event}')
            if row.get('error'):
                require(any(p['span'] == span and p['event'] == 'error' and p['data'].get('kind') == row['error']['kind'] and p['data'].get('message') == row['error']['message'] for p in points), 'missing span-local error-kind record')
            del active[span]
        elif state == 'point':
            require(span in active, 'point outside active span')
            require(row['parent'] == active[span][0]['parent'], 'point parent mismatch')
            points.append(row)
            if event == 'hot.summary':
                data = row['data']
                require(root['data'].get('level') == 1, 'aggregate outside level 1')
                require(data.get('event') in m.get('aggregated', {}), 'unregistered hot phase')
                require(type(data.get('count')) is int and data['count'] > 0, 'empty aggregate')
                require(data['total_ns'] >= data['max_ns'] >= 0 and data['bytes'] >= 0, 'invalid aggregate measures')
                require(sum(data['outcomes'].values()) == data['count'], 'aggregate outcomes do not reconcile')
                key = (span, data['event'])
                require(key not in aggregate_keys, 'duplicate owner/event aggregate')
                aggregate_keys.add(key)
                error_points = [p for p in points if p['event'] == 'error' and p['span'] == span and p['data'].get('hot_event') == data['event']]
                require(len(error_points) == data['error_records'], 'missing aggregate-owned error-kind record')
                aggregate_paths.append((row, active[span][1] + [data['event']]))
            if event == 'root.summary':
                require(span == root['span'], 'summary must belong to root')
                attributes(row['data'], m['root']['summary'])
            if event in m.get('events', {}):
                attributes(row['data'], m['events'][event]['attributes'])
        else:
            require(False, 'unknown state')
        if row.get('error'):
            kinds.append(row['error']['kind'])
        if event == 'error':
            kinds.append(row['data']['kind'])
    require(root is not None and not active, 'empty or unfinished trace')
    require(rows[-1]['event'] == 'run' and rows[-1]['state'] == 'end', 'root must end last')
    if 'level' in selection:
        require(root['data'].get('level') == selection['level'], 'detail level mismatch')
    if command != 'library':
        command_starts = [r for r,_ in starts if r['event'] == command and r['parent'] == root['span']]
        require(len(command_starts) == 1, 'expected one direct command span')
        command_end = next(r for r in rows if r['state'] == 'end' and r['span'] == command_starts[0]['span'])
        expected_outcome = 'ok'
        if scenario != 'success':
            kind = selection.get('error_kind', '')
            require(bool(kind), 'failure selection needs independent error kind')
            expected_outcome = 'partial' if kind.startswith('incomplete_') else ('refused' if kind in ['usage','unsupported_edit','unsupported_type','output_exists','same_input','pack_destination','pack_project','pack_input','pack_path','edit_spec','pointer_write','node_version','node_expected_graph','node_endpoint','node_socket_type','node_occupied','node_cycle','node_graph','node_constructor'] else 'error')
            require(command_end.get('error', {}).get('kind') == kind, 'terminal command error kind mismatch')
            if scenario == 'usage': require(kind == 'usage', 'usage selection mismatch')
            if scenario == 'sdna_error': require(kind == 'invalid_sdna', 'SDNA selection mismatch')
        require(command_end['outcome'] == expected_outcome and rows[-1]['outcome'] == expected_outcome, 'scenario/terminal outcome mismatch')
    if scenario == 'success':
        required_phases = list(contract.get('required_success_phases', []))
        if not selection.get('conditions', {}).get('dry_run', False):
            required_phases += contract.get('execution_phases', [])
        for required in required_phases:
            require(any(r['event'] == required for r,_ in starts), f'missing successful execution phase: {required}')
    if 'node_tree_operations' in selection:
        operations = selection['node_tree_operations']
        applications = [(r, path) for r, path in starts if r['event'] == 'node_tree.apply']
        applied = [p for p in points if p['event'] == 'node_tree.operation']
        require(len(applications) == len(applied) == len(operations), 'node-tree operation cardinality')
        for i, (operation, (row, path), point) in enumerate(zip(operations, applications, applied)):
            require(path == ['run','edit','node_tree_edit','node_tree.apply'], 'node-tree apply ancestry')
            require(row['data']['operation_index'] == i and row['data']['operation'] == operation['op'], 'node-tree apply identity/order')
            require(point['data']['operation_index'] == i and point['data']['applied'] is True, 'node-tree operation identity/order')
            owner = next(r for r, _ in starts if r['span'] == point['span'])
            require(owner['event'] == 'node_tree_edit', 'node-tree operation owner')
    finals = [p for p in points if p['event'] == 'root.summary' and p['data'].get('summary_phase') == 'final']
    require(len(finals) == 1 and finals[0]['seq'] == len(rows)-2, 'missing/duplicate/misplaced final root summary')
    final = finals[0]['data']
    consumed = set()
    used_identities = set()
    for expected_input in selection.get('inputs', []):
        path = expected_input if isinstance(expected_input,str) else expected_input['path']
        needs_schema = True if isinstance(expected_input,str) else expected_input.get('schema', True)
        signature = next((v.get('signature') for v in root['data']['inputs'] if v.get('path_hex') == os.fsencode(path).hex()), None)
        identity = next((i for i,v in enumerate(final.get('inputs', [])) if i not in used_identities and v.get('identity_kind') == 'stat_signature_not_content_hash' and isinstance(v.get('signature'),dict) and (v.get('path_hex') == os.fsencode(path).hex() or (signature is not None and v.get('signature') == signature))), None)
        require(identity is not None, f'missing captured input identity: {path}')
        used_identities.add(identity)
        if not needs_schema: continue
        for row, ancestry in starts:
            if row['event'] != 'schema' or row['span'] in consumed: continue
            parent = row['parent']; associated = False
            while parent is not None and parent != root['parent']:
                ancestor = next((r for r,_ in starts if r['span'] == parent), None)
                if ancestor is None: break
                if ancestor['event'] in ['file.open','snapshot'] and ancestor['data'].get('path') == path: associated = True; break
                parent = ancestor['parent']
            if associated:
                consumed.add(row['span']); break
        else:
            require(False, f'missing per-input index/SDNA: {path}')
    require(final['diagnostics_complete'] is True, 'diagnostic coverage not complete')
    require(bool(root['data']['env'].get('tool.build.commit')), 'missing build identity')
    require(root['data']['env'].get('service.version') == root['data']['version'], 'service version mismatch')
    resource = final['resource']
    require(bool(resource.get('source')) and bool(resource.get('scope')), 'resource source/scope missing')
    if resource.get('value', 1) is None:
        require(bool(resource.get('reason')), 'unavailable resources need reason')
    else:
        attributes(resource,dict(cpu_user_seconds='number',cpu_system_seconds='number',peak_rss_kib='number',input_blocks='number',output_blocks='number',block_unit='string'))
        for key in ['cpu_user_seconds','cpu_system_seconds','peak_rss_kib','input_blocks','output_blocks']:
            require(resource[key] >= 0, f'negative resource counter: {key}')
    def subst(value):
        return value.replace('{command}', command).replace('{error_kind}', selection.get('error_kind', ''))
    # Aggregates prove hot work at the same owning structural path, not a
    # fabricated per-call span. Keep envelope checks on the real rows above.
    work_evidence = sorted(starts + aggregate_paths, key=lambda pair: pair[0]['seq'])
    def ordered_check(reqs):
        cursor = 0
        for req in reqs:
            path = [subst(p) for p in req['path']]
            alternatives = [path] + [path[:-1] + [event] for event in m.get('work_aliases', {}).get(path[-1], [])]
            found = next((i for i in range(cursor, len(work_evidence)) if any(matches(candidate, work_evidence[i][1]) for candidate in alternatives)), None)
            require(found is not None, f'missing/resequenced span path: {path}')
            cursor = found + 1
    ordered_check(ordered)
    for condition, enabled in selection.get('conditions', {}).items():
        if enabled:
            ordered_check(m['conditions'][condition].get('ordered', []))
    for call in selection.get('calls', []):
        require(call in m.get('binding', {}), f'unregistered binding call: {call}')
        ordered_check(m['binding'][call]['ordered'])
    for events in event_groups:
        cursor = 0
        for spec in events:
            found = next((i for i in range(cursor, len(points)) if points[i]['event'] == spec['event']), None)
            require(found is not None, f'missing/resequenced event: {spec["event"]}')
            attributes(points[found]['data'], spec.get('attributes', {}))
            cursor = found + 1
    for kind in errors:
        require(subst(kind) in kinds, f'missing error kind: {kind}')
    for key, value in selection.get('root_attributes', {}).items():
        require(root['data'].get(key) == value, f'wrong root attribute: {key}')
    used_decisions = set()
    for expected in selection.get('decisions', []):
        found = next((p for p in points if p['event'] == 'decision' and p['seq'] not in used_decisions and all(p['data'].get(k) == v for k,v in expected.items())), None)
        require(found is not None, f'missing decision identity/kind/reason: {expected}')
        used_decisions.add(found['seq'])
    for expected in selection.get('skip_observations', []):
        require(any(p['event'] == 'decision.observation' and all(p['data'].get(k) == v for k,v in expected.items()) for p in points), f'missing skipped-item observation: {expected}')
    used_effects = set()
    for expected in selection.get('effects', []):
        found = next((p for p in points if p['event'] == 'output.effect' and p['seq'] not in used_effects and all(p['data'].get(k) == v for k,v in expected.items())), None)
        require(found is not None, f'missing final output effect: {expected}')
        used_effects.add(found['seq'])
    used_publications = set()
    for item in selection.get('publications', []):
        expected = {'path': item} if isinstance(item, str) else dict(item)
        path = expected.pop('path')
        candidates = [r for r in rows if r['event'] == 'publish' and r['state'] == 'start' and r['data'].get('path') == path and r['span'] not in used_publications]
        require(bool(candidates), f'missing member publication: {path}')
        found = next((start for start in candidates for end in rows if end['state'] == 'end' and end['span'] == start['span'] and all(end['data'].get(k, start['data'].get(k)) == v for k,v in expected.items())), None)
        require(found is not None, f'publication effects mismatch: {path}')
        used_publications.add(found['span'])
    return True


def selftest():
    # Valid control deliberately contains both kinds and ordered point events.
    m = json.loads(MANIFEST.read_text())
    root = {k: {'string':'x','object':{},'array':[],'boolean':True}[v] for k,v in m['root']['start'].items()}
    root.update(command='summary', mode='report',requested_mode='strict',mode_source='default',version='fixture',env={'tool.build.commit':'fixture','service.version':'fixture'})
    rows = []
    def row(event, state, span, parent, data=None, **extra):
        rows.append(dict(v=1, seq=len(rows), trace='t', run='r', event=event, state=state, span=span, parent=parent, data=data or {}, **extra))
    row('run','start','0',None,root)
    row('summary','start','1','0')
    row('file.open','start','2','1',dict(path='fixture'))
    row('index','start','3','2',dict(decoded_bytes=100))
    row('schema','start','4','3',dict(bytes=40))
    for e,s,p in [('schema','4','3'),('index','3','2'),('file.open','2','1')]:
        row(e,'end',s,p,outcome='ok')
    row('decision','point','1','0',dict(item='a',kind='excluded',reason='explicit exclusion'))
    row('error','point','1','0',dict(kind='fixture_kind',message='fixture only'))
    row('summary','end','1','0',outcome='ok')
    row('serialize','start','5','0',dict(role='stdout'))
    row('serialize','end','5','0',outcome='ok')
    row('root.summary','point','0',None,dict(outputs=[],resource=dict(source='fixture',scope='process',value=None,reason='synthetic control'),diagnostics_complete=True,summary_phase='final'))
    row('run','end','0',None,outcome='ok')
    selection = dict(command='summary',mode='report',scenario='success', errors=['fixture_kind'],events=[dict(event='decision'),dict(event='error')])
    # root.summary is mandatory independently; event order here is decision,error.
    control = copy.deepcopy(m)
    control['commands']['summary']['success']['events'] = []
    check(rows, selection, control)
    mutations = {}
    mutations['remove-schema'] = [r for r in rows if r['event'] != 'schema']
    mutations['remove-decision'] = [r for r in rows if r['event'] != 'decision']
    mutations['remove-error-kind'] = [r for r in rows if r['event'] != 'error']
    bad = copy.deepcopy(rows); del bad[0]['data']['inputs']; mutations['remove-attribute'] = bad
    bad = copy.deepcopy(rows); del bad[2]['data']['path']; mutations['remove-phase-attribute'] = bad
    bad = copy.deepcopy(rows); bad[8],bad[9] = bad[9],bad[8]; mutations['resequence-events'] = bad
    bad = copy.deepcopy(rows); bad[4]['parent'] = '1'; mutations['wrong-parent'] = bad
    bad = copy.deepcopy(rows); bad.insert(-1,copy.deepcopy(bad[10])); mutations['duplicate-end'] = bad
    mutations['unfinished'] = rows[:-1]
    bad = copy.deepcopy(rows); bad[3],bad[4] = bad[4],bad[3]; mutations['resequence-spans'] = bad
    rejected = {}
    for name, candidate in mutations.items():
        candidate = copy.deepcopy(candidate)
        for i, r in enumerate(candidate): r['seq'] = i
        try:
            check(candidate, selection, control)
        except ValueError as e:
            rejected[name] = str(e)
        else:
            raise AssertionError(f'mutation accepted: {name}')
    def reject(label, candidate, chosen):
        candidate = copy.deepcopy(candidate)
        for i,r in enumerate(candidate): r['seq'] = i
        try: check(candidate, chosen, control)
        except ValueError as e: rejected[label] = str(e)
        else: raise AssertionError(f'mutation accepted: {label}')
    bad = copy.deepcopy(rows)
    for r in bad:
        if r['state']=='end' and r['event'] in ['run','summary']: r['outcome']='error'
    reject('wrong-terminal-outcome',bad,selection)
    bad = copy.deepcopy(selection); bad['inputs'] = ['fixture','fixture']
    reject('missing-second-input-sdna',rows,bad)
    bad = copy.deepcopy(rows)
    extra = dict(v=1,seq=0,trace='t',run='r',event='root.summary',state='point',span='0',parent=None,data=dict(outputs=[],resource={},diagnostics_complete=True,summary_phase='final'))
    bad.insert(1,extra)
    reject('duplicate-early-final-summary',bad,selection)
    bad = copy.deepcopy(rows); bad[-2]['data']['resource']={}
    reject('empty-resources',bad,selection)
    published = copy.deepcopy(rows)
    anchor = next(i for i,r in enumerate(published) if r['event']=='summary' and r['state']=='end')
    published[anchor:anchor] = [
        dict(v=1,seq=0,trace='t',run='r',event='publish',state='start',span='p',parent='1',data=dict(path='/fixture/output')),
        dict(v=1,seq=0,trace='t',run='r',event='publish',state='end',span='p',parent='1',data=dict(output_written=True,durability_completed=False),outcome='ok')]
    for i,r in enumerate(published): r['seq']=i
    once = copy.deepcopy(selection); once['publications']=['/fixture/output']
    check(published,once,control)
    twice = copy.deepcopy(once); twice['publications'] *= 2
    reject('reused-publication-identity',published,twice)
    removed = [r for r in published if r['event']!='publish']
    reject('missing-member-publication',removed,once)
    missing_effect = copy.deepcopy(selection)
    missing_effect['effects']=[dict(role='pack_member',path_hex='61',output_written=True,durability_completed=True)]
    reject('missing-final-end-durability-effect',rows,missing_effect)
    chosen = copy.deepcopy(selection); chosen['decisions']=[dict(kind='excluded',reason='explicit exclusion')]
    check(rows,chosen,control)
    bad = [r for r in rows if r['event']!='decision']
    reject('removed-primary-decision-despite-observation',bad,chosen)
    print(json.dumps(dict(control='pass', rejected=rejected), indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('trace', nargs='?')
    parser.add_argument('--selection', help='JSON file with command/mode/scenario/conditions and independent report expectations')
    parser.add_argument('--selftest', action='store_true')
    args = parser.parse_args()
    if args.selftest:
        selftest()
    elif args.trace is None and args.selection is None:
        # Preserve the existing no-argument qualification entry point.
        import runpy
        runpy.run_path(str(Path(__file__).with_name('qualify-flattrace.py')), run_name='__main__')
    else:
        try:
            check([json.loads(line) for line in Path(args.trace).read_text().splitlines()], json.loads(Path(args.selection).read_text()))
        except (ValueError, KeyError) as error:
            parser.exit(1, f'flattrace semantic gate: {error}\n')
        print('flattrace semantic gate: pass')
