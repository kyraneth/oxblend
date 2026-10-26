#!/usr/bin/env python3
"""Real CLI reporting round trips, source/trace bindings and adversarial privacy."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import zipfile

BINARY = Path(sys.argv[1]).resolve()
ROOT = Path(__file__).resolve().parents[1]
checks = []

def call(args, trace=None, expected=0):
    env = os.environ.copy()
    env.pop('FLATTRACE_DIR', None)
    if trace:
        env['FLATTRACE_DIR'] = str(trace)
    result = subprocess.run([str(BINARY), *map(str, args)], env=env, capture_output=True)
    assert result.returncode == expected, (args, result.returncode, result.stderr.decode(errors='replace'))
    return result

def envelope(result):
    records = []
    for line in result.stderr.splitlines():
        try:
            value = json.loads(line)
        except ValueError:
            continue
        if value.get('schema') == 1 and 'error' in value:
            records.append(value)
    assert len(records) == 1
    return records[0]

def preview(args, output):
    result = call(['report', *args, '--output', output, '--preview'])
    value = json.loads(result.stdout)
    assert not output.exists()
    for member in value['members']:
        if member['preview_text'] is not None:
            body = member['preview_text'].encode()
            assert hashlib.sha256(body).hexdigest() == member['sha256']
            assert len(body) == member['bytes']
    return value

def publish(value, output, expected=0):
    receipt = value['receipt_path'] or os.fsdecode(bytes.fromhex(value['receipt_path_hex']))
    return call(['report', '--create', receipt, '--preview-sha256', value['receipt_sha256'], '--output', output], expected=expected)

def contents(value, name):
    return next(m['preview_text'] for m in value['members'] if m['path'] == name)

with tempfile.TemporaryDirectory(prefix='oxblend-report-e2e-') as temporary:
    work = Path(temporary)
    traces = work/'traces'
    traces.mkdir()
    scene = work/'PRIVATE_NAME_SENTINEL.blend'
    shutil.copyfile(ROOT/'tests/fixtures/file-ops/nodes-first-fileglobal.blend', scene)
    failure = call(['pointer', scene, '2', '["nodes","first"]'], trace=traces, expected=2)
    saved = envelope(failure)
    receipt = saved['diagnostic']
    assert receipt['run_id']
    assert receipt['trace_identity']['identity_kind'] == 'trace_root_span'
    assert hashlib.sha256(failure.stdout).hexdigest() == receipt['binding']['stdout_sha256']
    base = dict(saved)
    del base['diagnostic']
    canonical = (json.dumps(base, sort_keys=True, separators=(',', ':'), ensure_ascii=False)+'\n').encode()
    assert hashlib.sha256(canonical).hexdigest() == receipt['binding']['stderr_error_sha256']
    assert any(f['context'] and f['context']['pointer_decimal'] == '8' for f in receipt['failures'])
    owners = [f for f in receipt['failures'] if f['context_origin'] == 'owner_hook']
    assert len(owners) == 1
    assert receipt['failures'][-1]['caused_by'] == [owners[0]['id']]
    assert any(p['check_id'] == 'pointer.resolve.forwarded' for p in owners[0]['propagations'])
    aggregate_run = call(['trace', scene], expected=2)
    aggregate = envelope(aggregate_run)['diagnostic']
    aggregate_report = json.loads(aggregate_run.stdout)
    def assert_carrier(report, diagnostic):
        leaf, = [f for f in diagnostic['failures'] if f['context_origin'] == 'owner_hook']
        assert report['errors'][0]['diagnostic_ref'] == {'run_id': diagnostic['run_id'], 'failure_id': leaf['id']}
        assert diagnostic['failures'][-1]['caused_by'] == [leaf['id']]
        assert not any(f['context_origin'] == 'producer_report' and f['kind'] == 'dangling_pointer' for f in diagnostic['failures'])
    assert aggregate['failures'][-1]['kind'] == 'incomplete_trace'
    assert_carrier(aggregate_report, aggregate)
    seeded = json.loads(json.dumps(aggregate_report))
    seeded['errors'][0].pop('diagnostic_ref')
    try:
        assert_carrier(seeded, aggregate)
    except (AssertionError, KeyError):
        pass
    else:
        raise AssertionError('carrier gate accepted a seeded missing reference')
    report = work/'failure.json'
    report.write_text(json.dumps(saved))
    stdout = work/'stdout.json'
    stdout.write_bytes(failure.stdout)
    trace = next(traces.glob('*.jsonl'))
    (work/'retained-failure.json').write_text(json.dumps(saved, indent=2))
    (work/'retained-native.stdout').write_bytes(failure.stdout)
    (work/'retained-native.stderr').write_bytes(failure.stderr)
    shutil.copyfile(trace, work/'retained-trace.jsonl')
    selected = preview(['--report', report, '--trace', trace, '--stdout', stdout], work/'selected.zip')
    (work/'retained-preview.json').write_text(json.dumps(selected, indent=2))
    manifest = json.loads(contents(selected, 'manifest.json'))
    assert manifest['producer_run']['binding'] == 'verified_trace_receipt'
    assert manifest['trace']['status'] == 'available'
    assert manifest['evidence']['source_status'] == 'partial'
    assert all('PRIVATE_NAME_SENTINEL' not in (m['preview_text'] or '') for m in selected['members'])
    assert 'pointer.resolve.forwarded' in contents(selected, 'error-source.json')
    # A verified trace must never upgrade an unrelated/fabricated saved report.
    legacy = work/'legacy.json'
    legacy.write_text(json.dumps({'schema': 1, 'complete': False, 'errors': [{'kind': 'io', 'message': 'unrelated'}]}))
    unbound = preview(['--report', legacy, '--trace', trace], work/'legacy.zip')
    unbound_manifest = json.loads(contents(unbound, 'manifest.json'))
    assert unbound_manifest['producer_run']['binding'] == 'unverified'
    assert unbound_manifest['producer_run']['command'] is None
    assert unbound_manifest['producer_run']['exit_code'] is None
    assert unbound_manifest['selected_trace_run']['command'] == 'pointer'
    assert 'NOT bound' in unbound_manifest['selected_trace_run']['scope']
    fabricated = work/'fabricated.json'
    fabricated.write_text(json.dumps({'schema': 1, 'complete': True, 'block_index': 999, 'diagnostic': receipt}))
    for extra in [['--trace', trace], ['--stdout', stdout]]:
        call(['report', '--report', fabricated, *extra, '--output', work/'fabricated.zip', '--preview'], expected=2)
    standalone = work/'standalone.json'
    standalone.write_text(json.dumps(receipt))
    standalone_preview = preview(['--report', standalone, '--stdout', stdout], work/'standalone.zip')
    assert json.loads(contents(standalone_preview, 'manifest.json'))['producer_run']['binding'] == 'unverified'
    assert receipt['producer']['binary_sha256'] == hashlib.sha256(BINARY.read_bytes()).hexdigest()
    # A trace can retain a receipt even when final stderr delivery fails.
    silent_dir = work/'silent-stderr-traces'
    silent_env = os.environ.copy()
    silent_env['FLATTRACE_DIR'] = str(silent_dir)
    silent_child = subprocess.Popen([str(BINARY), 'trace', str(scene)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=silent_env)
    silent_child.stderr.close()
    silent_bytes = silent_child.stdout.read()
    silent_child.stdout.close()
    assert silent_child.wait() == 2
    silent_stdout = work/'silent.stdout'
    silent_stdout.write_bytes(silent_bytes)
    silent_trace, = silent_dir.glob('*.jsonl')
    silent_preview = preview(['--trace', silent_trace, '--stdout', silent_stdout], work/'silent.zip')
    silent_manifest = json.loads(contents(silent_preview, 'manifest.json'))['producer_run']
    assert silent_manifest['stderr_delivery_complete'] is False
    assert silent_manifest['stdout_delivery_complete'] is True
    assert silent_manifest['completion'] == 'partial'
    assert silent_manifest['binding'] == 'verified_partial_trace_receipt'
    assert any('STDERR DELIVERY INCOMPLETE' in warning for warning in silent_preview['warnings'])
    # A typo run is not product work and must not displace the last failure.
    typo = envelope(call(['bogus-command'], expected=2, trace=traces))
    assert typo['diagnostic']['command'] == 'invalid'
    # Selection uses recorded identity/chronology, not filenames or mtimes.
    for selector in [['--run-id', receipt['run_id']], ['--last']]:
        p = preview([*selector, '--trace-dir', traces], work/(('id' if len(selector)>1 else 'last')+'.zip'))
        assert json.loads(contents(p, 'manifest.json'))['producer_run']['binding'] == 'verified_trace_receipt'
        assert json.loads(contents(p, 'manifest.json'))['producer_run']['command'] == 'pointer'
    call(['report', '--run-id', '../escape', '--trace-dir', traces, '--output', work/'escape.zip', '--preview'], expected=2)
    original_events = [json.loads(line) for line in trace.read_text().splitlines()]
    envelope_id = original_events[0]['run']
    assert envelope_id != receipt['run_id']
    call(['report', '--run-id', envelope_id, '--trace-dir', traces, '--output', work/'envelope-id.zip', '--preview'], expected=2)
    # Concurrent/malformed non-candidates and byte-identical copies do not poison selection.
    duplicate = traces/'duplicate.jsonl'
    shutil.copyfile(trace, duplicate)
    (traces/'empty.jsonl').write_bytes(b'')
    (traces/'unrelated.jsonl').write_bytes(b'{broken\n')
    linked = traces/'linked.jsonl'
    linked.symlink_to(trace)
    for selector in [['--run-id', receipt['run_id']], ['--last']]:
        p = preview([*selector, '--trace-dir', traces], work/(('dedup-id' if len(selector)>1 else 'dedup-last')+'.zip'))
        assert p['local_input_diagnostics']
        assert json.loads(contents(p, 'manifest.json'))['producer_run']['binding'] == 'verified_trace_receipt'
    call(['report', '--trace', linked, '--output', work/'linked.zip', '--preview'], expected=2)
    alias = work/'trace-alias'
    alias.symlink_to(traces, target_is_directory=True)
    call(['report', '--last', '--trace-dir', alias, '--output', work/'alias.zip', '--preview'], expected=2)
    # Different content claiming the same invocation is still ambiguous.
    conflict = traces/'conflict.jsonl'
    conflict.write_text('\n'.join(json.dumps(e) for e in original_events)+'\n')
    call(['report', '--run-id', receipt['run_id'], '--trace-dir', traces, '--output', work/'conflict.zip', '--preview'], expected=2)
    conflict.unlink()
    interrupted_dir = work/'interrupted'
    interrupted_dir.mkdir()
    interrupted = interrupted_dir/'partial.jsonl'
    interrupted.write_text('\n'.join(json.dumps(e) for e in original_events[:-1])+'\n'+ '{"incomplete":')
    partial = preview(['--run-id', receipt['run_id'], '--trace-dir', interrupted_dir], work/'partial.zip')
    assert json.loads(contents(partial, 'manifest.json'))['producer_run']['completion'] == 'unknown'
    call(['report', '--last', '--trace-dir', interrupted_dir, '--output', work/'partial-last.zip', '--preview'], expected=2)
    # The actual forwarding-ancestry guard carries one cycle identity into
    # the report without inventing a pointer lookup or ancestry cause IDs.
    cycle_dir = work/'long-cycle'
    shutil.copytree(ROOT/'tests/fixtures/forwarding-branches/long-cycle', cycle_dir)
    cycle_run = call(['trace', cycle_dir/'root.blend'], expected=2)
    cycle_receipt = envelope(cycle_run)['diagnostic']
    cycle_sources = [f for f in cycle_receipt['failures'] if f['check_id'] == 'linked_id.forwarding.cycle']
    assert cycle_sources
    for source in cycle_sources:
        assert source['context']['namespace'] == 'id'
        assert source['context']['pointer_decimal'] is None
    cycle_ids = {f['id'] for f in cycle_sources}
    for error in json.loads(cycle_run.stdout)['errors']:
        if error['kind'] == 'cycle':
            assert error['diagnostic_ref']['run_id'] == cycle_receipt['run_id']
            assert error['diagnostic_ref']['failure_id'] in cycle_ids
    assert not any(f['context_origin'] == 'producer_report' and f['kind'] == 'cycle' for f in cycle_receipt['failures'])
    # Ordinary missing-asset causes preserve typed owner/path evidence without
    # requiring a new trace or leaking names by default.
    spec = importlib.util.spec_from_file_location('path_fixture', ROOT/'scripts/check-paths.py')
    fixtures = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fixtures)
    missing_scene = work/'missing.blend'
    missing_spelling = b'//PRIVATE_MISSING_SENTINEL.png'
    missing_scene.write_bytes(fixtures.image(missing_spelling))
    missing_run = call(['trace', missing_scene], expected=2)
    missing_report = work/'missing-error.json'
    missing_report.write_text(json.dumps(envelope(missing_run)))
    missing_stdout = work/'missing.stdout'
    missing_stdout.write_bytes(missing_run.stdout)
    missing_preview = preview(['--report', missing_report, '--stdout', missing_stdout], work/'missing.zip')
    assert 'missing_asset' in contents(missing_preview, 'error-report.json')
    assert all('PRIVATE_MISSING_SENTINEL' not in (m['preview_text'] or '') and missing_spelling.hex() not in (m['preview_text'] or '') for m in missing_preview['members'])
    missing_names = preview(['--report', missing_report, '--stdout', missing_stdout, '--include-names'], work/'missing-names.zip')
    assert missing_spelling.hex() in contents(missing_names, 'error-source.json')
    # A trace from a different invocation must not be silently accepted.
    other = work/'other'
    other.mkdir()
    second = call(['summary', work/'absent.blend'], trace=other, expected=2)
    call(['report', '--report', report, '--trace', next(other.glob('*.jsonl')), '--output', work/'mismatch.zip', '--preview'], expected=2)
    # Tampered stdout must fail before publication.
    stdout.write_bytes(b'tampered')
    call(['report', '--report', report, '--stdout', stdout, '--output', work/'badstdout.zip', '--preview'], expected=2)
    stdout.write_bytes(failure.stdout)
    # Trace mutations preserve seq so detection is structural/binding, not gap-only.
    original_events = [json.loads(line) for line in trace.read_text().splitlines()]
    mutations = []
    events = json.loads(json.dumps(original_events))
    point = next(e for e in events if e['event'] == 'diagnostic.receipt')
    point['data']['receipt']['exit_code'] = 0
    mutations.append(events)
    events = json.loads(json.dumps(original_events))
    next(e for e in events if e['state'] == 'end' and e['parent'] is not None)['parent'] = 'wrong-parent'
    mutations.append(events)
    events = [e for e in original_events if e['event'] != 'diagnostic.invocation']
    for i, event in enumerate(events):
        event['seq'] = i
    mutations.append(events)
    for i, events in enumerate(mutations):
        changed = work/f'mutated-{i}.jsonl'
        changed.write_text('\n'.join(json.dumps(e) for e in events)+'\n')
        call(['report', '--trace', changed, '--output', work/f'mutated-{i}.zip', '--preview'], expected=2)
    # Unknown key/module/name/kind sentinels never pass the closed projection.
    evil = work/'evil.json'
    evil.write_text(json.dumps({'schema': 1, 'error': {'kind': 'PRIVATE_KIND_SENTINEL', 'message': 'PRIVATE_MESSAGE_SENTINEL', 'module': 'PRIVATE_MODULE_SENTINEL', 'PRIVATE_KEY_SENTINEL': 'private'}, 'PRIVATE_KEY_SENTINEL': '/synthetic-private', 'path_hex': 'PRIVATE_HEX_SENTINEL'}))
    sanitized = preview(['--report', evil], work/'evil.zip')
    assert all('SENTINEL' not in (m['preview_text'] or '') for m in sanitized['members'])
    extra = Path(sanitized['receipt_path']).parent/'foreign.txt'
    extra.write_text('UNLISTED')
    publish(sanitized, work/'evil.zip', expected=2)
    assert not (work/'evil.zip').exists()
    extra.unlink()
    # Malicious producer metadata/custom source identifiers cannot bypass tokens.
    forged = json.loads(json.dumps(saved))
    forged['diagnostic']['producer']['version'] = 'PRIVATE_VERSION_SENTINEL'
    forged['diagnostic']['producer']['build_revision'] = 'PRIVATE_REVISION_SENTINEL'
    context = next(f['context'] for f in forged['diagnostic']['failures'] if f['context'])
    marker = 'PRIVATE_SCHEMA_SENTINEL'
    context['file_hex'] = marker.encode().hex()
    context['block_code_hex'] = marker.encode().hex()
    context['field_path_hex'] = [marker.encode().hex()]
    context['PRIVATE_KEY_SENTINEL'] = marker
    forged_path = work/'forged.json'
    forged_path.write_text(json.dumps(forged))
    protected = preview(['--report', forged_path], work/'forged.zip')
    assert all('SENTINEL' not in (m['preview_text'] or '') and marker.encode().hex() not in (m['preview_text'] or '') for m in protected['members'])
    names = preview(['--report', forged_path, '--include-names'], work/'names.zip')
    assert names['privacy']['include_names']
    assert marker.encode().hex() in contents(names, 'error-source.json')
    # Native Unix parent paths remain representable without panicking JSON.
    native_parent = os.fsencode(work)+b'/native-'+bytes.fromhex('ff')
    os.mkdir(native_parent)
    native_output = Path(os.fsdecode(native_parent+b'/native.zip'))
    native = preview(['--report', evil], native_output)
    assert native['receipt_path'] is None and native['receipt_path_hex']
    publish(native, native_output)
    # Whole-file disclosure is explicit, snapshot-bound and never rereads source.
    payload = work/'private.blend'
    payload.write_bytes(b'PRIVATE_WHOLE_FILE_SENTINEL')
    opted = preview(['--report', report, '--include-file', payload], work/'file.zip')
    member = next(m for m in opted['members'] if m['path'] == 'attachments/user-supplied.bin')
    assert member['preview_text'] is None and member['whole_content_disclosure']
    assert opted['privacy']['include_file']
    frozen_file = Path(opted['receipt_path']).parent/'attachment.bin'
    frozen_file.write_bytes(b'CHANGED_AFTER_REVIEW')
    publish(opted, work/'file.zip', expected=2)
    assert not (work/'file.zip').exists()
    frozen_file.write_bytes(b'PRIVATE_WHOLE_FILE_SENTINEL')
    payload.unlink()
    publish(opted, work/'file.zip')
    with zipfile.ZipFile(work/'file.zip') as archive:
        assert archive.read('attachments/user-supplied.bin') == b'PRIVATE_WHOLE_FILE_SENTINEL'
        assert archive.testzip() is None
    # Every ZIP member equals its reviewed snapshot, including manifest.
    report.unlink()
    trace.unlink()
    created = publish(selected, work/'selected.zip')
    (work/'retained-created.json').write_bytes(created.stdout)
    with zipfile.ZipFile(work/'selected.zip') as archive:
        for member in selected['members']:
            body = archive.read(member['path'])
            assert body == member['preview_text'].encode()
            assert hashlib.sha256(body).hexdigest() == member['sha256']
        assert archive.testzip() is None
    # A broken pipe after publication retains actual effects, never invites retry.
    healthy = work/'healthy.blend'
    shutil.copyfile(ROOT/'tests/fixtures/file-ops/synthetic-empty-seed.blend', healthy)
    destination = work/'published-pack'
    read_fd, write_fd = os.pipe()
    os.close(read_fd)
    env = os.environ.copy()
    env.pop('FLATTRACE_DIR', None)
    process = subprocess.Popen([str(BINARY), 'pack', str(healthy), str(destination)], stdout=write_fd, stderr=subprocess.PIPE, env=env)
    os.close(write_fd)
    _, stderr = process.communicate()
    assert process.returncode == 2
    broken = envelope(subprocess.CompletedProcess([], process.returncode, b'', stderr))
    assert broken['error']['kind'] == 'broken_pipe'
    assert destination.exists() and list(destination.iterdir())
    assert broken['diagnostic']['binding']['stdout_delivery_complete'] is False
    assert any(r.get('directory_created') or r.get('archive_written') or r.get('output_written') for r in broken['diagnostic']['operation_state']['reports'])
    assert broken['diagnostic']['trace_identity'] is None
    assert broken['diagnostic']['failures'][-1]['caused_by'] == []
    broken_path = work/'broken.json'
    broken_path.write_text(json.dumps(broken))
    accepted = work/'accepted.stdout'
    accepted.write_bytes(b'')
    broken_preview = preview(['--report', broken_path, '--stdout', accepted], work/'broken.zip')
    broken_manifest = json.loads(contents(broken_preview, 'manifest.json'))
    assert broken_manifest['producer_run']['stdout_delivery_complete'] is False
    assert broken_manifest['producer_run']['completion'] == 'partial'
    assert broken_manifest['producer_run']['binding'] == 'verified_partial_streams'
    assert json.loads(contents(broken_preview, 'error-report.json'))['stdout_delivery_complete'] is False
    assert any('INCOMPLETE' in warning for warning in broken_preview['warnings'])
    checks = ['broken pipe after publication retains actual effects', 'exact private carrier: single owner/forwarding/report identity plus seeded missing-reference rejection', 'owner-declared causal wrappers', 'forwarding-cycle source/ID carrier', 'missing-asset typed source evidence with default privacy', 'stdout and canonical stderr digests', 'trace/root identity binding', 'exact-run and recorded-last selection', 'wrong-run and fabricated/legacy pairing refusal or down-label', 'single invocation-ID namespace', 'poisoned/duplicate/symlink trace directory handling', 'interrupted explicit-run versus last selection', 'stdout mismatch and stdout/stderr incomplete-delivery down-label', 'trace digest/parent/invocation mutation refusals', 'closed default privacy and malicious producer/source/omission-key sentinels', 'native non-UTF-8 parent path receipts', 'unlisted-stage and changed-attachment refusal', 'explicit names/whole-file opt-ins', 'source deletion snapshot immutability', 'independent ZIP parser and exact reviewed member hashes']
    if os.environ.get('OXBLEND_REPORT_EVIDENCE'):
        shutil.copytree(work, Path(os.environ['OXBLEND_REPORT_EVIDENCE']), symlinks=True)
print(json.dumps({'status': 'pass', 'checks': checks}, indent=2))
