#!/usr/bin/env python3
"""Authored synthetic SDNA, not Blender output; no production-file inputs.

Regenerate with Python's standard library. The tests supply a real tiny PNG in
an adjacent assets directory; image.blend retains that surviving linked image.
Packed libraries contain selected and unselected instances of all three roles.
"""
import os
import struct
from pathlib import Path

os.umask(0o002)
ROOT = Path(__file__).resolve().parent
P = lambda fmt, *values: struct.pack('<' + fmt, *values)
S = {
    'Opaque': [('char', 'byte')],
    'ID': [('char', 'name[66]'), ('Library', '*lib')],
    'Library': [('ID', 'id'), ('char', 'name[256]')],
    'ListBase': [('void', '*first'), ('void', '*last')],
    'Object': [('ID', 'id'), ('ListBase', 'modifiers'), ('ListBase', 'constraints'), ('Image', '*image')],
    'Camera': [('ID', 'id'), ('AnimData', '*adt')],
    'ModifierData': [('ModifierData', '*next'), ('ModifierData', '*prev'), ('int', 'type'), ('char', 'name[64]')],
    'NodesModifierData': [('ModifierData', 'modifier'), ('bNodeTree', '*node_group'), ('char', 'marker[8]')],
    'bNodeTree': [('ID', 'id')],
    'bConstraint': [('bConstraint', '*next'), ('bConstraint', '*prev'), ('short', 'type'), ('void', '*data')],
    'bTransLikeConstraint': [('Object', '*tar'), ('char', 'marker[8]')],
    'AnimData': [('ListBase', 'drivers')],
    'FCurve': [('FCurve', '*next'), ('FCurve', '*prev'), ('ChannelDriver', '*driver')],
    'ChannelDriver': [('ListBase', 'variables')],
    'DriverVar': [('DriverVar', '*next'), ('DriverVar', '*prev'), ('short', 'num_targets'), ('DriverTarget', 'targets[2]')],
    'DriverTarget': [('ID', '*id'), ('short', 'idtype'), ('char', 'marker[8]')],
    'Image': [('ID', 'id'), ('char', 'name[256]'), ('int', 'source')],
}
L = {'char': 1, 'short': 2, 'int': 4, 'void': 0}


def parts(name):
    sections = name.lstrip('*').split('[')
    return sections[0], name.startswith('*'), int(sections[1][:-1]) if len(sections) > 1 else 1


def size(ty):
    if ty not in L:
        L[ty] = sum((8 if parts(n)[1] else size(t)) * parts(n)[2] for t, n in S[ty])
    return L[ty]


for ty in S:
    size(ty)
T, ST = list(L), list(S)
N = list(dict.fromkeys(n for fields in S.values() for _, n in fields))


def record(ty, values):
    body = b''
    for t, n in S[ty]:
        name, pointer, count = parts(n)
        default = 0 if pointer or t in ('short', 'int') else b'' if t == 'char' else {} if count == 1 else [{}] * count
        value = values.get(name, default)
        if pointer:
            body += P('Q', value)
        elif t in ('short', 'int'):
            body += P('h' if t == 'short' else 'i', value)
        elif t == 'char':
            assert len(value) <= count
            body += value.ljust(count, b'\0')
        elif count == 1:
            body += record(t, value)
        else:
            assert len(value) == count
            body += b''.join(record(t, item) for item in value)
    assert len(body) == L[ty]
    return body


def block(code, ty, address, values, extra=b''):
    body = record(ty, values) + extra
    return P('4siQii', code, len(body), address, ST.index(ty), 1) + body


dna = b'SDNANAME' + P('I', len(N)) + b'\0'.join(n.encode() for n in N) + b'\0'
dna += bytes(-len(dna) % 4)
dna += b'TYPE' + P('I', len(T)) + b'\0'.join(t.encode() for t in T) + b'\0'
dna += bytes(-len(dna) % 4)
dna += b'TLEN' + P('H' * len(T), *(L[t] for t in T))
dna += bytes(-len(dna) % 4)
dna += b'STRC' + P('I', len(ST))
for ty, fields in S.items():
    dna += P('HH', T.index(ty), len(fields))
    for t, n in fields:
        dna += P('HH', T.index(t), N.index(n))
END = P('4siQii', b'DNA1', len(dna), 0, 0, 1) + dna + P('4siQii', b'ENDB', 0, 0, 0, 0)
HEADER = b'BLENDER-v301'
MISSING = 0xDEAD0000


def generate(case):
    body = b'BLENDER-v302' if case == 'modifier-header' else HEADER
    for unused, base in ((False, 0x1000), (True, 0x3000)):
        suffix = b'unselected' if unused else b'selected'
        bad = lambda name: not unused and case == name
        node_pointer = base + 0x900 if bad('wrong-id') or bad('ambiguous') else MISSING + base
        body += block(b'OB\0\0', 'Object', base, {
            'id': {'name': b'OBnodes-' + suffix},
            'modifiers': {'first': base + 0x100, 'last': base + 0x100},
            'image': 0 if unused else 0x8000,
        })
        if bad('modifier-owner'):
            body += block(b'CA\0\0', 'Camera', base + 0x90, {'id': {'name': b'CAnot-object'}})
        body += block(b'DATA', 'NodesModifierData', base + 0x100, {
            'modifier': {'type': 56 if bad('modifier-type') else 57, 'name': b'Ordinary Nodes' if bad('modifier-name') else b'Realize Instances 2.93 Legacy'}, 
            'node_group': node_pointer, 'marker': b'nodesOK',
        }, b'\0' if bad('modifier-extent') else b'')
        body += block(b'OB\0\0', 'Object', base + 0x200, {
            'id': {'name': b'OBconstraint-' + suffix},
            'constraints': {'first': MISSING if bad('structure') else base + 0x300, 'last': base + 0x300},
        })
        body += block(b'DATA', 'bConstraint', base + 0x300, {
            'type': 22 if bad('constraint-type') else 23,
            'data': MISSING if bad('constraint-data') else base + 0x400,
        })
        if bad('constraint-owner'):
            body += block(b'OB\0\0', 'Object', base + 0x390, {'id': {'name': b'OBdifferent-owner'}})
        body += block(b'DATA', 'bTransLikeConstraint', base + 0x400, {
            'tar': MISSING + base + 1, 'marker': b'consOK',
        })
        body += block(b'CA\0\0', 'Camera', base + 0x500, {
            'id': {'name': b'CAdriver-' + suffix}, 'adt': base + 0x600,
        })
        body += block(b'DATA', 'AnimData', base + 0x600, {'drivers': {'first': base + 0x650, 'last': base + 0x650}})
        body += block(b'DATA', 'FCurve', base + 0x650, {'driver': base + 0x700})
        body += block(b'DATA', 'ChannelDriver', base + 0x700, {'variables': {'first': base + 0x800, 'last': base + 0x800}})
        body += block(b'DATA', 'DriverVar', base + 0x800, {
            'num_targets': 1 if bad('driver-inactive') else 3 if bad('driver-capacity') else 2,
            'targets': [{'marker': b'nullOK'}, {'id': MISSING + base + 2, 'idtype': 16707 if bad('driver-idtype') else 16975, 'marker': b'driverOK'}],
        })
        if bad('wrong-id'):
            body += block(b'OB\0\0', 'Object', base + 0x900, {'id': {'name': b'OBnot-a-tree'}})
        if bad('ambiguous'):
            for name in (b'NTduplicate-a', b'NTduplicate-b'):
                body += block(b'NT\0\0', 'bNodeTree', base + 0x900, {'id': {'name': name}})
    body += block(b'LI\0\0', 'Library', 0x8100, {'id': {'name': b'LIimages'}, 'name': b'//../assets/image.blend'})
    body += block(b'ID\0\0', 'ID', 0x8000, {'name': b'IMsurvivor', 'lib': 0x8100})
    root = HEADER + block(b'LI\0\0', 'Library', 0xA000, {'id': {'name': b'LIchild'}, 'name': ('//' + case + '.blend').encode()})
    for address, name in ((0xA100, b'OBnodes-selected'), (0xA200, b'OBconstraint-selected'), (0xA300, b'CAdriver-selected')):
        root += block(b'ID\0\0', 'ID', address, {'name': name, 'lib': 0xA000})
    (ROOT / (case + '.blend')).write_bytes(body + END)
    (ROOT / (case + '-root.blend')).write_bytes(root + END)


for case in ('normal', 'modifier-type', 'modifier-name', 'modifier-header', 'modifier-extent', 'modifier-owner', 'constraint-type', 'constraint-owner', 'constraint-data', 'structure', 'driver-inactive', 'driver-capacity', 'driver-idtype', 'wrong-id', 'ambiguous'):
    generate(case)
(ROOT / 'image.blend').write_bytes(HEADER + block(b'IM\0\0', 'Image', 0x100, {'id': {'name': b'IMsurvivor'}, 'name': b'//pixel.png', 'source': 1}) + END)
