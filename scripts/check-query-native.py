#!/usr/bin/env python3
"""Assert exact documented saved/native differences after the untouched oracle runner.
This does not turn native-golden mismatches into native-equivalence passes.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path


def at_pointer(value, pointer):
    for token in pointer.lstrip('/').split('/'):
        token = token.replace('~1', '/').replace('~0', '~')
        value = value[int(token)] if isinstance(value, list) else value[token]
    return value


def validate(receipt, expected_differences, native, report):
    actual = receipt['mismatches']
    assert len(actual) == len(expected_differences), (receipt['case'], actual)
    keyed = {(d['path'], d['kind']): d for d in actual}
    assert len(keyed) == len(actual), 'duplicate mismatch sites'
    for declared in expected_differences:
        got = keyed[(declared['path'], declared['kind'])]
        wanted = {k: v for k, v in declared.items() if k not in ['reason', 'expected_from_native']}
        if declared.get('expected_from_native'):
            wanted['expected'] = at_pointer(native, declared['path'])
            assert isinstance(wanted['expected'], str) and wanted['expected']
        assert got == wanted, (receipt['case'], got, wanted)
    assert receipt['exit'] == 0 and report['requested_coverage']['complete'] is True
    assert report['errors'] == []
    if any(d['reason'] == 'source_aware_subtype_library_open' for d in expected_differences):
        events = [json.loads(line) for path in receipt['traces'] for line in Path(path).read_text().splitlines()]
        opens = [e for e in events if e.get('event') == 'file.open' and e.get('state') == 'start']
        libraries = [e for e in events if e.get('event') == 'query.library.open' and e.get('state') == 'start']
        assert len(opens) == len(libraries) + 1 == 2
        assert len({e['data']['path'] for e in opens}) == len(opens)
        assert sum(e['data']['path'] == report['source_observation']['path'] for e in opens) == 1
        assert any(o['selected_from'] == 'library_local' for o in report['linked_subtype_observations'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', required=True, type=Path)
    parser.add_argument('--results', required=True, type=Path)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('native_comparator', args.spec / 'acceptance.py')
    comparator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(comparator)
    manifest = json.loads((args.spec / 'manifest.json').read_text())
    known = json.loads((Path(__file__).resolve().parents[1] / 'tests/query-native-differences.json').read_text())['cases']
    selected = [c for c in manifest['cases'] if c['role'] == 'saved_5_2_reopen']
    assert len(list(args.results.glob('*.receipt.json'))) == len(selected)
    native_passed = 0
    assertions = 0
    negative_controls = 0
    for case in selected:
        expected_path = args.spec / case['expected']
        assert hashlib.sha256(expected_path.read_bytes()).hexdigest() == case['expected_sha256']
        assert hashlib.sha256(Path(case['input']).read_bytes()).hexdigest() == case['sha256']
        native = comparator.normalize_projection(json.loads(expected_path.read_text())['projection'])
        receipt = json.loads((args.results / (case['id'] + '.receipt.json')).read_text())
        report = json.loads((args.results / (case['id'] + '.stdout')).read_text())
        declared = known.get(case['id'], [])
        validate(receipt, declared, native, report)
        # A new or missing mismatch must fail: this is not a case-wide waiver.
        bad = dict(receipt, mismatches=receipt['mismatches'] + [{'path': '/unexpected', 'kind': 'extra'}])
        try:
            validate(bad, declared, native, report)
        except AssertionError:
            negative_controls += 1
        else:
            raise AssertionError('extra-mismatch negative control accepted')
        if declared:
            bad = dict(receipt, mismatches=receipt['mismatches'][:-1])
            try:
                validate(bad, declared, native, report)
            except AssertionError:
                negative_controls += 1
            else:
                raise AssertionError('missing-documented-difference control accepted')
        native_passed += bool(receipt['passed'])
        assertions += len(declared)
    print(json.dumps({'cases': len(selected), 'native_golden_passed': native_passed,
                      'native_golden_mismatched': len(selected) - native_passed,
                      'exact_documented_difference_assertions': assertions,
                      'all_case_contracts_verified': True, 'negative_controls': negative_controls}))


if __name__ == '__main__':
    main()
