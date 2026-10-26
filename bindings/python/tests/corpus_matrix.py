"""Full installed-wheel corpus parity, including inside real headless Blender.

Only this test invokes the comparison CLI. The binding has no subprocess route.
Raw CLI stdout/stderr/status and binding facts are retained. Diagnostic references
are run-local identities: compare all other facts while retaining both raw refs.
"""
from __future__ import annotations
import argparse
from collections.abc import Mapping
from dataclasses import asdict
import hashlib
import json
import math
import os
from pathlib import Path
import resource
import subprocess
import sys
import time
import oxblend as ox

p = argparse.ArgumentParser()
p.add_argument('--corpus', type=Path, required=True)
p.add_argument('--cli', type=Path, required=True)
p.add_argument('--out', type=Path, required=True)
a = p.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else None)
a.out.mkdir(parents=True, exist_ok=False)
assert os.environ.get('FLATTRACE_DIR')
corpus = json.loads(a.corpus.read_text())
assert len(corpus) == 120  # Supplied acceptance corpus, not a workload limit.

def thaw(v):
    if isinstance(v, Mapping): return {k: thaw(x) for k, x in v.items()}
    if isinstance(v, (tuple, list)): return [thaw(x) for x in v]
    return v

def comparable(v):
    if isinstance(v, dict): return {k: comparable(x) for k, x in v.items() if k not in ('operation_diagnostics', 'diagnostic_ref')}
    if isinstance(v, list): return [comparable(x) for x in v]
    return v

def summary_json(s):
    def text(b): return None if b is None else b.decode('utf-8')
    h = s.header
    return dict(schema=1, header=dict(version=h.version, pointer_size=h.pointer_size, little_endian=h.little_endian, file_format=h.file_format),
        block_codes={text(c): n for c, n in s.block_codes},
        **{k: list(map(text, getattr(s, k))) for k in ('objects', 'materials', 'meshes', 'images', 'collections')},
        scenes=[dict(name=text(c.name), frame_start=c.frame_start, frame_end=c.frame_end, frame_step=c.frame_step, camera=text(c.camera),
            fps=dict(numerator=c.fps.numerator, base=None if c.fps.base is None or not math.isfinite(c.fps.base.value) else c.fps.base.value,
                effective=c.fps.effective, base_bits_hex=None if c.fps.base is None else f'{c.fps.base.bits:08x}')) for c in s.scenes],
        warnings=[asdict(w) for w in s.warnings])

def cli(directory, label, command, path, *extra):
    argv = [str(a.cli), command, str(path), *map(str, extra)]
    proc = subprocess.run(argv, capture_output=True)
    (directory / (label + '.stdout')).write_bytes(proc.stdout)
    (directory / (label + '.stderr')).write_bytes(proc.stderr)
    (directory / (label + '.receipt.json')).write_text(json.dumps({'argv': argv, 'exit': proc.returncode}))
    return proc, json.loads(proc.stdout) if proc.stdout else None

def expected_error(proc, error):
    assert proc.returncode != 0
    envelopes = [json.loads(line) for line in proc.stderr.splitlines() if line.startswith(b'{')]
    assert any(e.get('error', {}).get('kind') == error.kind for e in envelopes), (error.kind, proc.stderr)

results = []
start = time.monotonic()
load = os.getloadavg()
for item in corpus:
    directory = a.out / str(item['index']); directory.mkdir()
    path = Path(item['path'])
    before = path.stat()
    row = {'index': item['index'], 'path': str(path), 'get_count': 0}
    proc, expected = cli(directory, 'summary', 'summary', path)
    try:
        file = ox.open(path)
    except ox.OxblendError as error:
        expected_error(proc, error)
        row['open_error'] = error.kind
    else:
        with file:
            try:
                actual = summary_json(file.summary())
            except ox.OxblendError as error:
                expected_error(proc, error); row['summary_error'] = error.kind
            else:
                assert proc.returncode == 0
                (directory / 'summary.binding.json').write_text(json.dumps(actual))
                assert comparable(actual) == comparable(expected), ('summary', path)
                row['summary_equal'] = True
            scenes = file.blocks(code=b'SC')
            for block in scenes:
                for field in (b'sfra', b'efra', b'frame_step', b'frs_sec', b'frs_sec_base'):
                    proc, expected = cli(directory, f'get-{block.index}-{field.decode()}', 'get', path, block.index, json.dumps(['r', field.decode()]))
                    try:
                        value = file.get([ox.FieldQuery(block, 0, (b'r', field))])[0]
                    except ox.OxblendError as error: expected_error(proc, error)
                    else:
                        assert proc.returncode == 0
                        if value.kind in ('signed', 'unsigned', 'pointer'): actual = {'kind': value.kind, 'decimal': str(value.value)}
                        elif value.kind == 'float32': actual = {'kind': 'float32', 'bits_hex': f'{value.value.bits:08x}'}
                        else: raise AssertionError(value)
                        assert actual == expected['value'], ('get', path, field)
                        row['get_count'] += 1
            for command in ('query', 'trace'):
                extra = ('--type', 'Scene', '--fields', 'id.name') if command == 'query' else ()
                proc, expected = cli(directory, command, command, path, *extra)
                try:
                    report = file.query(type='Scene', fields=['id.name'], allow_incomplete=True) if command == 'query' else file.trace(allow_incomplete=True)
                except ox.OxblendError as error:
                    expected_error(proc, error); row[command + '_error'] = error.kind
                else:
                    actual = thaw(report)
                    (directory / (command + '.binding.json')).write_text(json.dumps(actual))
                    assert comparable(actual) == comparable(expected), (command, path)
                    assert proc.returncode in (0, 2)
                    row[command + '_equal'] = True
    after = path.stat()
    assert (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_ctime_ns) == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns)
    results.append(row)
    (a.out / 'progress.json').write_text(json.dumps(results))
report = {'passed': True, 'python': sys.version, 'module': ox.__file__, 'native': ox._native.__file__,
          'native_sha256': hashlib.sha256(Path(ox._native.__file__).read_bytes()).hexdigest(),
          'build_commit': ox._native.__build_commit__, 'files': len(results), 'results': results,
          'wall_seconds': time.monotonic() - start, 'load_before': load, 'load_after': os.getloadavg(),
          'peak_rss_kib': resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
          'rss_scope': 'whole host process, getrusage(RUSAGE_SELF), Linux KiB',
          'comparison_exclusion': 'only operation_diagnostics (Session-only envelope) and run-local diagnostic_ref identities; raw reports retained'}
(a.out / 'result.json').write_text(json.dumps(report, indent=2))
print(json.dumps({k: v for k, v in report.items() if k != 'results'}))
