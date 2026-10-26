#!/usr/bin/env python3
"""R38: preserve directory-library CLI bytes for both pack commands/memory modes.

Owned fixture copies only; no original is edited. Compare old/new stdout bytes
and the complete canonical error object, then installed Python path/File errors.
Raw streams, exit codes, input and binary hashes are retained under new WORK.
Usage: directory_library_parity.py BASELINE_CLI CLI WORK
"""
from __future__ import annotations
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import oxblend


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def project(value):
    if hasattr(value, 'to_json'):
        return value.to_json()
    if isinstance(value, (tuple, list)):
        return [project(v) for v in value]
    if isinstance(value, dict):
        return {k: project(v) for k, v in value.items()}
    return value


def exact(a, b):
    assert type(a) is type(b), (type(a), type(b), a, b)
    if isinstance(a, dict):
        assert a.keys() == b.keys(), (a, b)
        for key in a:
            exact(a[key], b[key])
    elif isinstance(a, list):
        assert len(a) == len(b)
        for x, y in zip(a, b):
            exact(x, y)
    else:
        assert a == b, (a, b)


def envelope(raw):
    rows = [json.loads(line) for line in raw.splitlines() if line.startswith(b'{')]
    return next(row for row in reversed(rows) if 'error' in row)


def main():
    old, new, work = map(Path, sys.argv[1:])
    work.mkdir(parents=True, exist_ok=False)
    fixture = Path(__file__).resolve().parents[3] / 'tests/fixtures/dependencies/mesh-material-root.blend'
    root = work / fixture.name
    root.write_bytes(fixture.read_bytes())
    (work / 'mesh-material-library.blend').mkdir()
    witness = {'baseline': sha(old), 'candidate': sha(new), 'fixture': sha(fixture), 'root_before': sha(root)}
    rows = []
    for memory in ('fast', 'bounded'):
        for command in ('pack', 'pack-sequence'):
            label = command + '-' + memory
            destination = work / (label + '-out')
            args = ([command, str(root), str(destination)] if command == 'pack'
                    else [command, str(root), '-t', str(destination)])
            args += ['--memory', memory]
            results = []
            for tag, binary in [('old', old), ('new', new)]:
                result = subprocess.run([str(binary), *args], cwd=work, capture_output=True)
                (work / f'{label}-{tag}.stdout').write_bytes(result.stdout)
                (work / f'{label}-{tag}.stderr').write_bytes(result.stderr)
                (work / f'{label}-{tag}.exit').write_text(str(result.returncode) + '\n')
                assert result.returncode == 2
                assert not destination.exists(), destination
                results.append(result)
            assert results[0].stdout == results[1].stdout, label
            before, after = [envelope(r.stderr)['error'] for r in results]
            exact(before, after)
            # The report must retain the directory library failure, not silently
            # become complete or replace it with a different refusal.
            report = json.loads(results[1].stdout)
            assert report['complete'] is False
            assert 'pack source is not regular file' in results[1].stdout.decode()
            for retained in (False, True):
                file = oxblend.open(root, memory=memory) if retained else None
                try:
                    if command == 'pack':
                        oxblend.pack(file if retained else root, destination, memory=memory)
                    elif retained:
                        file.pack_sequence(destination, memory=memory)
                    else:
                        oxblend.pack_sequence([root], destination, memory=memory)
                except oxblend.OxblendError as error:
                    exact({key: project(getattr(error, key)) for key in after}, after)
                    assert error.exit_code == 2
                    assert not destination.exists()
                    rows.append({'command': command, 'memory': memory, 'retained': retained, 'kind': error.kind})
                else:
                    raise AssertionError('directory library unexpectedly packed')
                finally:
                    if file is not None:
                        file.close()
    witness['root_after'] = sha(root)
    assert witness['root_before'] == witness['root_after'] == witness['fixture']
    (work / 'result.json').write_text(json.dumps({'complete': True, 'witness': witness, 'rows': rows}, indent=2) + '\n')
    print(f'directory_library_pairs={len(rows)} PASS')


if __name__ == '__main__':
    main()
