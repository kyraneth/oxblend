#!/usr/bin/env python3
"""synthetic serialized diagnostic sites; no native-source definitions.
Field spelling and role states are from retained black-box inspect/census.
Widths, counts and addresses below describe fixture topology, not limits.
"""
import hashlib
import json
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parent
S = {
    'Opaque': [('char', 'byte')],  # sdna=0 remains opaque
    'ID': [('char', 'name[66]'), ('Library', '*lib')],
    'Library': [('ID', 'id'), ('char', 'name[128]')],
    'ListBase': [('void', '*first'), ('void', '*last')],
    'bScreen': [('ID', 'id'), ('void', '*context'), ('ListBase', 'regionbase')],
    'ARegion': [('ARegion', '*next'), ('ARegion', '*prev'), ('ListBase', 'view_states')],
    'uiViewStateLink': [('uiViewStateLink', '*next'), ('uiViewStateLink', '*prev')],
    'Scene': [('ID', 'id'), ('ListBase', 'view_layers')],
    'ViewLayer': [('ViewLayer', '*next'), ('ViewLayer', '*prev'), ('ListBase', 'object_bases')],
    'Base': [('Base', '*next'), ('Base', '*prev')],
    'Mesh': [('ID', 'id'), ('int', '*poly_offset_indices')],
    'Image': [('ID', 'id'), ('PackedFile', '*packedfile'), ('char', 'name[128]'), ('int', 'source')],
    'Object': [('ID', 'id'), ('void', '*adt'), ('int', 'totcol'), ('void', '*mat'), ('void', '*data'), ('int', 'transflag'), ('void', '*dup_group'), ('void', '*proxy'), ('void', '*proxy_group'), ('void', '*pose'), ('ListBase', 'particlesystem'), ('ListBase', 'modifiers')],
    'ModifierData': [('ModifierData', '*next'), ('ModifierData', '*prev'), ('int', 'type')],
    'UVProjectModifierData': [('ModifierData', 'modifier'), ('Image', '*image')],
    'PackedFile': [('int', 'size'), ('void', '*data')],
}
L = {'char': 1, 'void': 0, 'int': 4}

def parts(n):
    ptr = n.startswith('*')
    name = n.lstrip('*').split('[')[0]
    count = int(n.split('[')[1][:-1]) if '[' in n else 1
    return name, ptr, count

def size(t):
    if t not in L:
        L[t] = sum((8 if parts(n)[1] else size(ty)) * parts(n)[2] for ty, n in S[t])
    return L[t]

for t in S:
    size(t)
T = list(L)
N = list(dict.fromkeys(n for fs in S.values() for _, n in fs))
ST = list(S)
pack = lambda fmt, *args: struct.pack('<' + fmt, *args)

def dna():
    b = b'SDNANAME' + pack('I', len(N)) + b'\0'.join(n.encode() for n in N) + b'\0'
    b += b'\0' * (-len(b) % 4)
    b += b'TYPE' + pack('I', len(T)) + b'\0'.join(t.encode() for t in T) + b'\0'
    b += b'\0' * (-len(b) % 4)
    b += b'TLEN' + pack('H' * len(T), *(L[t] for t in T))
    b += b'\0' * (-len(b) % 4)
    b += b'STRC' + pack('I', len(S))
    for t, fields in S.items():
        b += pack('HH', T.index(t), len(fields))
        for ty, n in fields:
            b += pack('HH', T.index(ty), N.index(n))
    return b

def record(t, values):
    b = b''
    for ty, n in S[t]:
        name, ptr, count = parts(n)
        v = values.get(name, 0 if ptr or ty == 'int' else b'' if ty == 'char' else {})
        if ptr:
            b += pack('Q', v)
        elif ty == 'char':
            b += v.ljust(count, b'\0')
        elif ty == 'int':
            b += pack('i', v)
        else:
            b += record(ty, v)
    assert len(b) == L[t]
    return b

def head(code, ty, addr, data):
    return pack('4siQii', code, len(data), addr, ST.index(ty) if ty else 0, 1) + data

def fixture(mode):
    # Three cases change only the seven reproduced sites; endpoints stay paired.
    choose = lambda address: 0 if mode == 'null' else 0xFFFF if mode == 'dangling' else 0x100 if mode == 'id-namespace' else address
    b = b'BLENDER-v405'
    entries = [
        (b'SR', 'bScreen', 0x100, {'id': {'name': b'SRscreen'}, 'context': 0x110 if mode == 'allocated-context' else choose(0xFFFF), 'regionbase': {'first': 0x110, 'last': 0x110}}),
        (b'DATA', 'ARegion', 0x110, {'view_states': {'first': 0 if mode == 'broken-first' else choose(0x120), 'last': 0 if mode == 'broken-last' else choose(0x120)}}),
        (b'DATA', 'uiViewStateLink', 0x120, {}),
        (b'SC', 'Scene', 0x200, {'id': {'name': b'SCscene'}, 'view_layers': {'first': 0x210, 'last': 0x210}}),
        (b'DATA', 'ViewLayer', 0x210, {'object_bases': {'first': 0 if mode == 'broken-first' else choose(0x220), 'last': 0 if mode == 'broken-last' else choose(0x220)}}),
        (b'DATA', 'Base', 0x220, {}),
        (b'ME', 'Mesh', 0x300, {'id': {'name': b'MEmesh'}, 'poly_offset_indices': choose(0x310)}),
        (b'DATA', None, 0x310, b'\0' * 8),
        (b'IM', 'Image', 0x400, {'id': {'name': b'IMimage'}, 'packedfile': 0x410}),
        (b'DATA', 'PackedFile', 0x410, {'size': 8, 'data': choose(0x420)}),
        (b'DATA', None, 0x420, b'packed!\0'),
    ]
    for code, ty, addr, values in entries:
        b += head(code, ty, addr, record(ty, values) if ty else values)
    return b + head(b'DNA1', None, 0, dna()) + pack('4siQii', b'ENDB', 0, 0, 0, 0)

def asset_fixture(dangling):
    b = b'BLENDER-v405'
    b += head(b'OB', 'Object', 0x100, record('Object', {'id': {'name': b'OBobject'}, 'modifiers': {'first': 0x200, 'last': 0x200}}))
    b += head(b'DATA', 'UVProjectModifierData', 0x200, record('UVProjectModifierData', {'modifier': {'type': 15}, 'image': 0xFFFF if dangling else 0x300}))
    b += head(b'IM', 'Image', 0x300, record('Image', {'id': {'name': b'IMasset'}, 'name': b'//asset.png', 'source': 1}))
    return b + head(b'DNA1', None, 0, dna()) + pack('4siQii', b'ENDB', 0, 0, 0, 0)

if __name__ == '__main__':
    manifest = {}
    for mode in ['allocated', 'allocated-context', 'null', 'dangling', 'id-namespace', 'broken-first', 'broken-last']:
        p = ROOT / (mode + '.blend')
        p.write_bytes(fixture(mode))
        manifest[p.name] = {'bytes': p.stat().st_size, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
    for dangling in [False, True]:
        p = ROOT / 'assets' / ('dangling-asset.blend' if dangling else 'live-asset.blend')
        p.parent.mkdir(exist_ok=True)
        p.write_bytes(asset_fixture(dangling))
        manifest[str(p.relative_to(ROOT))] = {'bytes': p.stat().st_size, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
    (ROOT / 'manifest.json').write_text(json.dumps({'author': 'oxblend', 'scope': 'synthetic serialized census and self-diff contracts', 'files': manifest}, indent=2) + '\n')
