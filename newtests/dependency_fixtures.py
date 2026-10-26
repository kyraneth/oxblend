#!/usr/bin/env python3
"""authored serialized dependency contracts, not Blender-made files."""
import pathlib
import struct

OUT = pathlib.Path(__file__).resolve().parents[1] / 'tests/fixtures/dependencies'
OUT.mkdir(parents=True, exist_ok=True)


def saved(structures, records, version):
    types = ['char', 'short', 'int', 'void']
    lengths = [1, 2, 4, 0]
    if any(ty == 'float' for _, _, fields in structures for ty, _ in fields):
        types.append('float')
        lengths.append(4)
    names = []
    for ty, length, fields in structures:
        types.append(ty)
        lengths.append(length)
        for _, name in fields:
            if name not in names:
                names.append(name)
    dna = b'SDNANAME' + struct.pack('<I', len(names)) + b''.join(n.encode() + b'\0' for n in names)
    align = lambda data: data + bytes(-len(data) % 4)
    dna = align(dna) + b'TYPE' + struct.pack('<I', len(types)) + b''.join(t.encode() + b'\0' for t in types)
    dna = align(dna) + b'TLEN' + struct.pack('<' + 'H' * len(lengths), *lengths)
    dna = align(dna) + b'STRC' + struct.pack('<I', len(structures))
    for ty, _, fields in structures:
        dna += struct.pack('<HH', types.index(ty), len(fields))
        for ty, name in fields:
            dna += struct.pack('<HH', types.index(ty), names.index(name))
    def block(code, addr, si, data, count=1):
        return struct.pack('<4sIQII', code, len(data), addr, si, count) + data
    return (b'BLENDER-v' + str(version).encode() + b''.join(block(*r) for r in records)
            + block(b'DNA1', 0, 0, dna) + block(b'ENDB', 0, 0, b'', 0))


def fixed(value, width):
    assert len(value) < width
    return value + bytes(width - len(value))


def iddata(name):
    return fixed(name, 64) + bytes(8)


ID = ('ID', 72, [('char', 'name[64]'), ('void', '*lib')])
LIST = ('ListBase', 16, [('void', '*first'), ('void', '*last')])
for field in ['name', 'filepath']:
    # Native Text.flags exists since at least Blender2.25. These cases model
    # external Texts explicitly; absent flags is a separate UNKNOWN fixture.
    structures = [ID, ('Text', 84, [('ID', 'id'), ('char', '*' + field), ('int', 'flags')])]
    records = [(b'TX', 100, 1, iddata(b'TXexternal') + struct.pack('<Qi', 200, 0)),
               (b'DATA', 200, 0, b'//external.py\0')]
    (OUT / ('text-' + field + '.blend')).write_bytes(saved(structures, records, 405))

structures = [ID, LIST, ('bNodeTree', 88, [('ID', 'id'), ('ListBase', 'nodes')]),
              ('bNode', 154, [('bNode', '*next'), ('bNode', '*prev'), ('char', 'name[64]'),
                             ('char', 'idname[64]'), ('short', 'type'), ('void', '*storage')]),
              ('NodeShaderScript', 1032, [('int', 'mode'), ('int', 'flag'), ('char', 'filepath[1024]')])]
records = [(b'NT', 100, 2, iddata(b'NTscripts') + struct.pack('<QQ', 200, 200)),
           (b'DATA', 200, 3, bytes(16) + fixed(b'External', 64) + fixed(b'ShaderNodeScript', 64)
            + struct.pack('<hQ', 171, 300)),
           (b'DATA', 300, 4, struct.pack('<ii', 1, 0) + fixed(b'//shader.osl', 1024))]
(OUT / 'shader-script.blend').write_bytes(saved(structures, records, 405))
# Material-owned embedded tree, reached by a selected linked Material. Unlike
# standalone root asset enumeration, selection expands bNode DATA edges first.
structures = [ID, LIST,
              ('Material', 80, [('ID', 'id'), ('bNodeTree', '*nodetree')]),
              ('bNodeTree', 88, [('ID', 'id'), ('ListBase', 'nodes')]),
              ('bNode', 154, [('bNode', '*next'), ('bNode', '*prev'), ('char', 'name[64]'),
                              ('char', 'idname[64]'), ('short', 'type'), ('void', '*storage')]),
              ('NodeShaderScript', 1104, [('int', 'mode'), ('int', 'flag'),
                                       ('char', 'filepath[1024]'),
                                       ('char', 'bytecode_hash[64]'), ('char', '*bytecode')])]
records = [(b'MA', 100, 2, iddata(b'MAscript') + struct.pack('<Q', 200)),
           (b'DATA', 200, 3, iddata(b'NTembedded') + struct.pack('<QQ', 300, 300)),
           (b'DATA', 300, 4, bytes(16) + fixed(b'Internal', 64) + fixed(b'ShaderNodeScript', 64)
            + struct.pack('<hQ', 171, 400)),
           (b'DATA', 400, 5, struct.pack('<ii', 0, 0) + fixed(b'C:\\', 1024) + bytes(72))]
(OUT / 'linked-script-material.blend').write_bytes(saved(structures, records, 405))
root_structures = [('ID', 72, [('char', 'name[64]'), ('Library', '*lib')]),
                   ('Library', 1096, [('ID', 'id'), ('char', 'name[1024]')])]
(OUT / 'linked-script-root.blend').write_bytes(saved(root_structures,
    [(b'ID', 100, 0, fixed(b'MAscript', 64) + struct.pack('<Q', 200)),
     (b'LI', 200, 1, iddata(b'LIshader') + fixed(b'//linked-script-material.blend', 1024))], 405))

structures = [ID, LIST,
              ('Object', 88, [('ID', 'id'), ('ListBase', 'constraints')]),
              ('bConstraint', 26, [('bConstraint', '*next'), ('bConstraint', '*prev'),
                                   ('short', 'type'), ('void', '*data')]),
              ('bSplineIKConstraint', 18, [('Object', '*tar'), ('float', '*points'),
                                         ('short', 'numpoints')]),
              ('FloatPoints', 128, [('float', 'values[32]')])]
(OUT / 'spline-ik.blend').write_bytes(saved(structures,
    [(b'OB', 100, 2, iddata(b'OBspline') + struct.pack('<QQ', 200, 200)),
     (b'DATA', 200, 3, bytes(16) + struct.pack('<hQ', 22, 300)),
     (b'DATA', 300, 4, struct.pack('<QQh', 0, 400, 32)),
     (b'DATA', 400, 0, bytes(128))], 405))
(OUT / 'spline-ik-root.blend').write_bytes(saved(root_structures,
    [(b'ID', 100, 0, fixed(b'OBspline', 64) + struct.pack('<Q', 200)),
     (b'LI', 200, 1, iddata(b'LIspline') + fixed(b'//spline-ik.blend', 1024))], 405))

structures = [ID, ('Material', 80, [('ID', 'id'), ('Image', '*image')]),
              ('Image', 1100, [('ID', 'id'), ('char', 'name[1024]'), ('int', 'source')]),
              ('Text', 84, [('ID', 'id'), ('char', '*name'), ('int', 'flags')])]
(OUT / 'pack-unselected-library.blend').write_bytes(saved(structures,
    [(b'MA', 100, 1, iddata(b'MAselected') + struct.pack('<Q', 200)),
     (b'IM', 200, 2, iddata(b'IMselected') + fixed(b'//texture.png', 1024) + struct.pack('<i', 1)),
     (b'TX', 300, 3, iddata(b'TXunused') + struct.pack('<Qi', 400, 0)),
     (b'DATA', 400, 0, b'/unselected/source.py\0')], 405))
(OUT / 'pack-unselected-root.blend').write_bytes(saved(root_structures,
    [(b'ID', 100, 0, fixed(b'MAselected', 64) + struct.pack('<Q', 200)),
     (b'LI', 200, 1, iddata(b'LIselected') + fixed(b'//pack-unselected-library.blend', 1024))], 405))

# Live scene collection -> linked Object; the Object's override slots are empty,
# while its Mesh owns the sole material. This needs the Object.data ID union.
structures = [('Opaque', 1, [('char', 'byte')]), ID, LIST,
              ('Object', 92, [('ID', 'id'), ('short', 'type'), ('void', '*data'),
                              ('Material', '**mat'), ('short', 'totcol')]),
              ('Mesh', 82, [('ID', 'id'), ('Material', '**mat'), ('short', 'totcol')]),
              ('Material', 80, [('ID', 'id'), ('bNodeTree', '*nodetree')]),
              ('bNodeTree', 88, [('ID', 'id'), ('ListBase', 'nodes')]),
              ('bNode', 162, [('bNode', '*next'), ('bNode', '*prev'), ('char', 'name[64]'),
                              ('char', 'idname[64]'), ('short', 'type'), ('ID', '*id'),
                              ('void', '*storage')]),
              ('Image', 1100, [('ID', 'id'), ('char', 'name[1024]'), ('int', 'source')]),
              ('NonID', 16, [('char', 'bytes[16]')]),
              ('Library', 1096, [('ID', 'id'), ('char', 'name[1024]')])]
(OUT / 'mesh-material-library.blend').write_bytes(saved(structures,
    [(b'OB', 100, 3, iddata(b'OBmesh') + struct.pack('<hQQh', 1, 300, 0, 0)),
     (b'DATA', 350, 9, bytes(16)),
     (b'ME', 300, 4, iddata(b'MEmesh') + struct.pack('<Qh', 400, 1)),
     (b'DATA', 400, 0, struct.pack('<Q', 500)),
     (b'MA', 500, 5, iddata(b'MAmesh') + struct.pack('<Q', 600)),
     (b'DATA', 600, 6, iddata(b'NTmesh') + struct.pack('<QQ', 700, 700)),
     (b'DATA', 700, 7, bytes(16) + fixed(b'Image', 64) + fixed(b'ShaderNodeTexImage', 64)
      + struct.pack('<hQQ', 143, 900, 0)),
     (b'IM', 900, 8, iddata(b'IMmesh') + fixed(b'//texture.png', 1024) + struct.pack('<i', 1))], 405))
(OUT / 'mesh-material-indirect.blend').write_bytes(saved(structures,
    [(b'OB', 100, 3, iddata(b'OBmesh') + struct.pack('<hQQh', 1, 300, 0, 0)),
     (b'ID', 300, 1, fixed(b'MEmesh', 64) + struct.pack('<Q', 1100)),
     (b'LI', 1100, 10, iddata(b'LImeshdata') + fixed(b'//mesh-data.blend', 1024))], 405))
live_root = [('Opaque', 1, [('char', 'byte')]),
             ('ID', 72, [('char', 'name[64]'), ('Library', '*lib')]), LIST,
             ('Library', 1096, [('ID', 'id'), ('char', 'name[1024]')]),
             ('Scene', 80, [('ID', 'id'), ('Collection', '*master_collection')]),
             ('Collection', 88, [('ID', 'id'), ('ListBase', 'gobject')]),
             ('CollectionObject', 24, [('CollectionObject', '*next'),
                                       ('CollectionObject', '*prev'), ('Object', '*ob')]),
             ('Object', 72, [('ID', 'id')]),
             ('FileGlobal', 8, [('Scene', '*curscene')])]
(OUT / 'mesh-material-root.blend').write_bytes(saved(live_root,
    [(b'SC', 100, 4, iddata(b'SClive') + struct.pack('<Q', 200)),
     (b'GR', 200, 5, iddata(b'GRlive') + struct.pack('<QQ', 500, 500)),
     (b'DATA', 500, 6, struct.pack('<QQQ', 0, 0, 1000)),
     (b'LI', 600, 3, iddata(b'LImesh') + fixed(b'//mesh-material-library.blend', 1024)),
     (b'ID', 1000, 1, fixed(b'OBmesh', 64) + struct.pack('<Q', 600)),
     (b'GLOB', 0, 8, struct.pack('<Q', 100))], 405))

structures = [ID, ('Script', 1096, [('ID', 'id'), ('char', 'scriptname[1024]')])]
(OUT / 'script.blend').write_bytes(saved(structures, [(b'PY', 100, 1, iddata(b'PYexternal') + fixed(b'//external.py', 1024))], 405))

for modern in [False, True]:
    structures = [ID, LIST,
                  ('Object', 88, [('ID', 'id'), ('ListBase', 'modifiers')]),
                  ('ModifierData', 84, [('ModifierData', '*next'), ('ModifierData', '*prev'),
                                        ('int', 'type'), ('char', 'name[64]')]),
                  ('NodesModifierData', 106 if modern else 104,
                   [('ModifierData', 'modifier'), ('char', '*simulation_bake_directory'),
                    ('int', 'bakes_num'), ('NodesModifierBake', '*bakes')]
                   + ([('short', 'bake_target')] if modern else [])),
                  ('NodesModifierBake', 14 if modern else 12,
                   [('int', 'flag'), ('char', '*directory')]
                   + ([('short', 'bake_target')] if modern else []))]
    records = [(b'OB', 100, 2, iddata(b'OBbake') + struct.pack('<QQ', 200, 200)),
               (b'DATA', 200, 4, bytes(16) + struct.pack('<i', 57) + fixed(b'Bake', 64)
                + struct.pack('<QiQ', 400, 1, 300) + (struct.pack('<h', 2) if modern else b'')),
               (b'DATA', 300, 5, struct.pack('<iQ', 0, 0) + (struct.pack('<h', 0) if modern else b'')),
               (b'DATA', 400, 0, b'//bake-cache\0')]
    (OUT / ('bake-modern.blend' if modern else 'bake-old.blend')).write_bytes(saved(structures, records, 405 if modern else 402))

structures = [ID, LIST, ('Group', 88, [('ID', 'id'), ('ListBase', 'gobject')]),
              ('GroupObject', 24, [('GroupObject', '*next'), ('GroupObject', '*prev'), ('Object', '*ob')]),
              ('Object', 72, [('ID', 'id')]),
              ('OtherLink', 16, [('OtherLink', '*next'), ('OtherLink', '*prev')])]
records = [(b'GR', 100, 2, iddata(b'GRobjects') + struct.pack('<QQ', 200, 300)),
           (b'DATA', 200, 3, struct.pack('<QQQ', 300, 0, 400)),
           (b'DATA', 300, 3, struct.pack('<QQQ', 0, 200, 400)),
           (b'OB', 400, 4, iddata(b'OBmember'))]
(OUT / 'group.blend').write_bytes(saved(structures, records, 279))
(OUT / 'group-wrong-type.blend').write_bytes(saved(structures,
    [(b'GR', 100, 2, iddata(b'GRobjects') + struct.pack('<QQ', 200, 200)),
     (b'DATA', 200, 5, bytes(16))], 279))
structures = [('ID', 72, [('char', 'name[64]'), ('Library', '*lib')]),
              ('Library', 1096, [('ID', 'id'), ('char', 'name[1024]')])]
(OUT / 'group-root.blend').write_bytes(saved(structures,
    [(b'ID', 100, 0, fixed(b'GRobjects', 64) + struct.pack('<Q', 200)),
     (b'LI', 200, 1, iddata(b'LIgroup') + fixed(b'//group.blend', 1024))], 405))
(OUT / 'unused-library-root.blend').write_bytes(saved(structures,
    [(b'LI', 200, 1, iddata(b'LIunused') + fixed(b'//unused.blend', 1024))], 405))
structures = [ID, LIST, ('Group', 88, [('ID', 'id'), ('ListBase', 'gobject')]),
              ('GroupObject', 24, [('GroupObject', '*next'), ('GroupObject', '*prev'), ('Object', '*ob')]),
              ('Object', 72, [('ID', 'id')]),
              ('Library', 1096, [('ID', 'id'), ('char', 'name[1024]')])]
(OUT / 'unused-library-child.blend').write_bytes(saved(structures,
    [(b'GR', 100, 2, iddata(b'GRobjects') + bytes(16)),
     (b'LI', 200, 5, iddata(b'LIunused') + fixed(b'//unused.blend', 1024))], 405))

structures = [('ID', 80, [('char', 'name[64]'), ('void', '*lib'), ('LibraryWeakReference', '*library_weak_reference')]),
              ('Object', 80, [('ID', 'id')]),
              ('LibraryWeakReference', 1088, [('char', 'library_filepath[1024]'), ('char', 'library_id_name[64]')])]
for name, value in [('weak-drive', b'C:/Users/Artist/library.blend'), ('weak-copybuffer', b'/tmp/copybuffer.blend')]:
    records = [(b'OB', 100, 1, iddata(b'OBappended') + struct.pack('<Q', 200)),
               (b'DATA', 200, 2, fixed(value, 1024) + fixed(b'OBoriginal', 64))]
    (OUT / (name + '.blend')).write_bytes(saved(structures, records, 405))
