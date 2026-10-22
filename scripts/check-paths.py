#!/usr/bin/env python3
"""Authored serialized path regressions through the actual CLI; no Blender required."""
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
BINARY = Path(os.environ.get('OXBLEND_TEST_BINARY', TARGET / 'release/oxblend'))


def saved(structures, records):
    types = ['char', 'short', 'int', 'void'] + [s[0] for s in structures]
    lengths = [1, 2, 4, 0] + [s[1] for s in structures]
    names = list(dict.fromkeys(n for _, _, fs in structures for _, n in fs))
    align = lambda b: b + bytes(-len(b) % 4)
    dna = align(b'SDNANAME' + struct.pack('<I', len(names)) + b''.join(n.encode() + b'\0' for n in names))
    dna = align(dna + b'TYPE' + struct.pack('<I', len(types)) + b''.join(t.encode() + b'\0' for t in types))
    dna = align(dna + b'TLEN' + struct.pack('<' + 'H'*len(lengths), *lengths))
    dna += b'STRC' + struct.pack('<I', len(structures))
    for ty, _, fs in structures:
        dna += struct.pack('<HH', types.index(ty), len(fs))
        for ty, n in fs:
            dna += struct.pack('<HH', types.index(ty), names.index(n))
    def block(code, addr, si, data):
        return struct.pack('<4sIQII', code, len(data), addr, si, 1) + data
    return b'BLENDER-v405' + b''.join(block(*r) for r in records) + block(b'DNA1', 0, 0, dna) + struct.pack('<4sIQII', b'ENDB', 0, 0, 0, 0)


def fixed(v, size):
    return v + bytes(size-len(v))


ID = ('ID', 72, [('char', 'name[64]'), ('void', '*lib')])
LIST = ('ListBase', 16, [('void', '*first'), ('void', '*last')])


def image(path, sequence=False):
    return saved([ID, ('Image', 1100, [('ID', 'id'), ('char', 'name[1024]'), ('int', 'source')])],
                 [(b'IM', 100, 1, fixed(b'IMasset', 72) + fixed(path, 1024) + struct.pack('<i', 2 if sequence else 1))])


class Paths(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='oxblend-paths-')
        self.root = Path(self.tmp.name)
        self.input = self.root / 'root.blend'
    def tearDown(self):
        self.tmp.cleanup()
    def run_cli(self, *args):
        r = subprocess.run([str(BINARY), *map(str, args)], capture_output=True)
        return r, json.loads(r.stdout) if r.stdout and args[0] != 'blocks' else None
    def test_bug1_strip_double_slash(self):
        structures = [ID, LIST, ('Scene', 80, [('ID', 'id'), ('Editing', '*ed')]),
                      ('Editing', 16, [('ListBase', 'seqbase')]),
                      ('Sequence', 42, [('Sequence', '*next'), ('Sequence', '*prev'), ('int', 'type'), ('Strip', '*strip'), ('ListBase', 'seqbase')]),
                      ('Strip', 1032, [('char', 'dir[1024]'), ('StripElem', '*stripdata')]),
                      ('StripElem', 256, [('char', 'name[256]')])]
        # Sequence width is 8+8+4+8+16 = 44 (serialized packed test SDNA).
        structures[4] = ('Sequence', 44, structures[4][2])
        self.input.write_bytes(saved(structures, [(b'SC', 100, 2, fixed(b'SCscene', 72)+struct.pack('<Q', 200)),
            (b'DATA', 200, 3, struct.pack('<QQ', 300, 300)),
            (b'DATA', 300, 4, struct.pack('<QQiQ', 0, 0, 3, 400)+bytes(16)),
            (b'DATA', 400, 5, fixed(b'//', 1024)+struct.pack('<Q', 500)),
            (b'DATA', 500, 6, fixed(b'clip.mov', 256))]))
        r, data = self.run_cli('trace', self.input)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(bytes.fromhex(data['usages'][0]['asset_path_hex']), b'//clip.mov')
    def test_bug2_triple_slash_stays_owner_relative(self):
        self.input.write_bytes(image(b'///asset.png'))
        (self.root/'asset.png').write_bytes(b'asset')
        r, data = self.run_cli('pack', self.input, self.root/'pack', '-p', self.root, '--relative-only', '--dry-run')
        self.assertEqual(r.returncode, 0, (r.stderr, data))
        self.assertIn(str(self.root/'asset.png').encode().hex(), [f['input_path_hex'] for f in data['files']])
    def test_bug3_pointer_array(self):
        self.input.write_bytes(saved([('Pointers', 16, [('void', '*values[2]')])], [(b'DATA', 100, 0, struct.pack('<QQ', 123, 456))]))
        r, data = self.run_cli('get', self.input, 0, '["values"]')
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(data['value'], {'kind':'array', 'items':[{'kind':'pointer', 'decimal':'123'}, {'kind':'pointer', 'decimal':'456'}]})
        r, data = self.run_cli('get', self.input, 0, '["values",1]')
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(data['value'], {'kind':'pointer', 'decimal':'456'})
    def test_bug4_windows_saved_paths(self):
        for raw, expected in [(b'C:\\assets\\tex.png', b'/c/assets/tex.png'), (b'C:/assets/tex.png', b'/c/assets/tex.png'),
                              (b'\\\\server\\share\\tex.png', b'\\\\server/share/tex.png'), (b'//assets\\tex.png', str(self.root/'assets/tex.png').encode())]:
            with self.subTest(raw=raw):
                self.input.write_bytes(image(raw))
                r, data = self.run_cli('list', self.input, '--json')
                self.assertEqual(r.returncode, 2, (r.stderr, data))
                self.assertEqual(data['files'][str(self.input)][0]['path'], os.fsdecode(expected))
                if raw.startswith(b'//'):
                    continue
                self.assertIn(b'path_resolution_unverified', r.stderr)
                target = self.root/('refused-'+str(len(raw)))
                r, data = self.run_cli('pack', self.input, target, '-p', self.root)
                self.assertEqual(r.returncode, 2, (r.stderr, data))
                self.assertIn('path_resolution_unverified', r.stderr.decode()+json.dumps(data))
                self.assertFalse(target.exists())
        self.input.write_bytes(image(b'C:\\assets\\tex.png'))
        r, data = self.run_cli('pack', self.input, self.root/'relative-only', '-p', self.root, '--relative-only')
        self.assertEqual(r.returncode, 0, (r.stderr, data))
        self.assertEqual(len(data['skipped_absolute_paths']), 1)
        self.assertEqual(len(data['files']), 1)
        self.assertEqual((self.root/'relative-only/root.blend').read_bytes(), self.input.read_bytes())
        self.assertIn('Unrelocated absolute path', (self.root/'relative-only/pack-info.txt').read_text())
    def test_relative_backslashes_follow_native_loader_and_pack(self):
        (self.root/'assets').mkdir()
        (self.root/'assets/tex.png').write_bytes(b'asset')
        self.input.write_bytes(image(b'//assets\\tex.png'))
        r, data = self.run_cli('pack', self.input, self.root/'portable', '-p', self.root)
        self.assertEqual(r.returncode, 0, (r.stderr, data))
        self.assertEqual(len(data['files']), 2)
        r, traced = self.run_cli('trace', self.root/'portable/root.blend')
        self.assertEqual(r.returncode, 0, (r.stderr, traced))
        self.assertTrue(traced['referenced_paths_exist'])
        self.assertTrue(Path(traced['usages'][0]['resolved_path']).is_relative_to(self.root/'portable'))
    def test_bug5_literal_glob_folder(self):
        for name in ['frames[final]', 'frames?final', 'frames*final']:
            with self.subTest(folder=name):
                folder = self.root/name
                folder.mkdir()
                for n in ['001.png', '002.png']:
                    (folder/n).write_bytes(b'frame')
                self.input.write_bytes(image(('//'+name+'/001.png').encode(), True))
                r, data = self.run_cli('pack', self.input, self.root/'pack', '-p', self.root, '--dry-run')
                self.assertEqual(r.returncode, 0, (r.stderr, data))
                self.assertEqual(len(data['files']), 3)
    def test_bug6_output_trailing_slash(self):
        self.input.write_bytes(image(b''))
        spec = self.root/'edits.json'
        spec.write_text('{"schema":1,"edits":[]}')
        r, _ = self.run_cli('rewrite', self.input, '--output', str(self.root/'new.blend')+'/', '--edits', spec)
        self.assertEqual(r.returncode, 2, r.stderr)
        self.assertFalse((self.root/'new.blend').exists())
    def test_bug7_input_permissions(self):
        self.input.write_bytes(image(b''))
        self.input.chmod(0o600)
        spec = self.root/'edits.json'
        spec.write_text('{"schema":1,"edits":[]}')
        r, data = self.run_cli('rewrite', self.input, '--output', self.root/'new.blend', '--edits', spec)
        self.assertEqual(r.returncode, 0, (r.stderr, data))
        self.assertEqual((self.root/'new.blend').stat().st_mode & 0o777, 0o600)
    def test_bug8_nonregular_input(self):
        fifo = self.root/'fifo'
        os.mkfifo(fifo)
        # A baseline blocking open is released by a daemon writer with invalid
        # bytes. The fixed CLI refuses before open; no writer is ever needed.
        def release_baseline():
            try:
                with fifo.open('wb') as f:
                    f.write(bytes(64))
            except OSError:
                pass
        threading.Thread(target=release_baseline, daemon=True).start()
        for p in [self.root, fifo]:
            with self.subTest(path=p.name):
                r, _ = self.run_cli('blocks', p)
                self.assertEqual(r.returncode, 3, r.stderr)
    def test_pack_sequence_recipe_cross_sequence_shared_asset(self):
        (self.root/'shared.png').write_bytes(b'shared')
        roots = [self.root/'seq-a/shot.blend', self.root/'seq-b/shot.blend']
        for root in roots:
            root.parent.mkdir()
            root.write_bytes(image(b'//../shared.png'))
        for dry in [True, False]:
            target = self.root/('preview' if dry else 'delivery')
            r, data = self.run_cli('pack-sequence', *roots, '-t', target, '-p', self.root, *(['--dry-run'] if dry else []))
            self.assertEqual(r.returncode, 0, (r.stderr, data))
            self.assertEqual(len(data['files']), 3)
            self.assertEqual(len(data['root_traces']), 2)
            self.assertTrue(all(t['trace']['complete'] and t['trace']['referenced_paths_exist'] for t in data['root_traces']))
            self.assertEqual(target.exists(), not dry)
    def test_bug9_overrides_native_backslash(self):
        p = self.root/'literal\\name.blend'
        p.write_bytes(image(b''))
        r, data = self.run_cli('overrides', p)
        self.assertEqual(r.returncode, 0, (r.stderr, data))


if __name__ == '__main__':
    unittest.main(verbosity=2)
