#!/usr/bin/env python3
"""Optional independent OpenImageIO/OpenEXR pixel oracle; not a runtime decoder.

Requires OpenImageIO and numpy in the receiving Python interpreter. Inputs are
read-only. Each output directory must be new; stdout is a JSON comparison report.
No lossy tolerance is silently chosen: equality means exact f32 bit agreement
between two decoders of the SAME compressed input, aligned by channel name.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

import OpenImageIO as oiio
import numpy as np

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output-dir', type=Path, required=True)
parser.add_argument('--part', type=int, required=True)
parser.add_argument('--pass', dest='pass_name', required=True)
parser.add_argument('inputs', type=Path, nargs='+')
args = parser.parse_args()
args.output_dir.mkdir(parents=True, exist_ok=False)
rows = []
for ordinal, source in enumerate(args.inputs):
    with source.open('rb') as stream:
        identity = hashlib.file_digest(stream, 'sha256').hexdigest()
    reference_input = oiio.ImageInput.open(str(source))
    assert reference_input, oiio.geterror()
    assert reference_input.seek_subimage(args.part, 0), reference_input.geterror()
    spec = reference_input.spec()
    names = list(spec.channelnames)
    # OpenImageIO presents RGB(A) first even when the EXR channel list differs.
    selected = sorted([i for i, name in enumerate(names)
                       if (name.startswith(args.pass_name + '.') if args.pass_name
                           else '.' not in name)], key=lambda i: names[i])
    assert selected, 'reference has no channels for explicit pass'
    reference = reference_input.read_image(format=oiio.FLOAT)
    assert reference is not None, reference_input.geterror()
    assert reference_input.close(), reference_input.geterror()
    target = args.output_dir / (str(ordinal) + '.f32')
    command = [str(args.binary), 'exr-pixels', str(source), '--part', str(args.part),
               '--pass', args.pass_name, '--output', str(target)]
    result = subprocess.run(command, capture_output=True)
    (args.output_dir / (str(ordinal) + '.stdout')).write_bytes(result.stdout)
    (args.output_dir / (str(ordinal) + '.stderr')).write_bytes(result.stderr)
    row = {'input': str(source), 'sha256': identity, 'argv': command,
           'exit': result.returncode, 'reference': 'OpenImageIO ' + oiio.VERSION_STRING,
           'reference_channels': [names[i] for i in selected]}
    if result.returncode == 0:
        report = json.loads(result.stdout)
        actual = np.fromfile(target, dtype='<f4').reshape(spec.height, spec.width, len(selected))
        expected = reference[:, :, selected]
        row.update(samples=int(actual.size),
                   bit_disagreements=int(np.count_nonzero(actual.view('u4') != expected.view('u4'))),
                   metadata_agree=report['channels'] == row['reference_channels'] and
                   report['data_window'] == [spec.x, spec.y, spec.x+spec.width-1, spec.y+spec.height-1])
        with target.open('rb') as stream:
            row['output_sha256'] = hashlib.file_digest(stream, 'sha256').hexdigest()
    with source.open('rb') as stream:
        assert hashlib.file_digest(stream, 'sha256').hexdigest() == identity, 'source changed'
    rows.append(row)
(args.output_dir / 'comparison.json').write_text(json.dumps(rows, indent=2) + '\n')
print(json.dumps(rows, indent=2))
raise SystemExit(0 if all(row['exit'] == 0 and row['metadata_agree'] and
                         row['bit_disagreements'] == 0 for row in rows) else 1)
