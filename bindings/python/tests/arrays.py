"""Installed extension array API receiving checks, against a native RNA fixture."""
import argparse
import json
import sys
from pathlib import Path
import oxblend as ox

p = argparse.ArgumentParser()
p.add_argument('--fixture', type=Path, required=True)
a = p.parse_args()
built = json.loads((a.fixture / 'built.json').read_text())
source = a.fixture / 'arrays.blend'
with ox.open(source) as file:
    blocks = {b.index: b for b in file.blocks()}
    def record(kind, name):
        return next(r for r in file.query(type=kind, fields=['id.name', 'totcol']).rows if r['id']['name'] == name)
    obj = record('Object', built['rna']['object'])
    mesh = record('Mesh', built['rna']['mesh'])
    def query(row, field):
        return ox.FieldQuery(blocks[row['source_local_ref']['block_index']], 0, (field,))
    count = len(built['rna']['slots'])
    bits = file.byte_array(query(obj, b'matbits'), count)
    assert type(bits) is bytes
    assert list(map(bool, bits)) == [s['link'] == 'OBJECT' for s in built['rna']['slots']]
    def names(row, count):
        table = file.pointer_table(query(row, b'mat'), count)
        assert type(table) is tuple and len(table) == count
        return [None if block is None else file.get([ox.FieldQuery(block, 0, (b'id', b'name'))])[0].value[2:].decode() for block in table]
    object_names = names(obj, count)
    assert names(mesh, len(built['rna']['mesh_materials'])) == built['rna']['mesh_materials']
    for index, slot in enumerate(built['rna']['slots']):
        if slot['link'] == 'OBJECT': assert object_names[index] == slot['material']
    for method, q in [(file.byte_array, query(obj, b'matbits')), (file.pointer_table, query(obj, b'mat'))]:
        try:
            method(q, sys.maxsize)  # Host integer domain; must refuse before allocation.
        except ox.OxblendError as error:
            assert error.kind in ('pointer_extent', 'overflow'), error.kind
            assert hasattr(error, 'diagnostic_ref')
        else: raise AssertionError('over-allocation count accepted')
    with ox.open(source) as foreign:
        try: foreign.byte_array(query(obj, b'matbits'), count)
        except ox.OxblendError as error: assert error.kind == 'foreign_block'
        else: raise AssertionError('foreign block accepted')
    owned = bits
file.close()
assert owned == bits
print(json.dumps({'passed': True, 'bytes': bits.hex(), 'object_table': object_names}))
