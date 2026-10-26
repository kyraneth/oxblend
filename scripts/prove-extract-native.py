#!/usr/bin/env python3
"""Native partial-library equality, reopen and append proof; never saves a main file.
Run Blender background/factory-startup/disable-autoexec --python this --
SOURCE EXTRACTED TREE_NAME REFERENCE_NEW REPORT_NEW
"""
import bpy
import json
import os
import pathlib
import subprocess
import sys


def scalar(value):
    if isinstance(value, (str, int, bool)) or value is None:
        return value
    if isinstance(value, float):
        return value if abs(value) != float('inf') and value == value else repr(value)
    if isinstance(value, bpy.types.ID):
        return {'id_type': value.bl_rna.identifier, 'name': value.name}
    try:
        return [scalar(v) for v in value]
    except TypeError:
        return None


def properties(obj, active=None):
    active = set() if active is None else active
    identity = (obj.bl_rna.identifier, obj.as_pointer())
    if identity in active:
        return {'cycle': obj.bl_rna.identifier}
    active.add(identity)
    result = {}
    for prop in obj.bl_rna.properties:
        name = prop.identifier
        if name in {'rna_type', 'scene', 'dimensions', 'internal_links', 'inputs', 'outputs', 'id_data'}:
            continue
        try:
            value = getattr(obj, name)
            if prop.type in {'STRING', 'BOOLEAN', 'INT', 'FLOAT', 'ENUM'}:
                result[name] = scalar(value)
            elif prop.type == 'POINTER':
                if value is None or isinstance(value, bpy.types.ID):
                    result[name] = scalar(value)
                elif isinstance(value, (bpy.types.Node, bpy.types.NodeSocket)):
                    result[name] = {'reference': value.name}
                else:
                    result[name] = properties(value, active)
            elif prop.type == 'COLLECTION' and name not in {'links', 'nodes'}:
                result[name] = [properties(v, active) for v in value]
        except (AttributeError, ValueError, TypeError, RuntimeError) as error:
            result[name] = {'unreadable': type(error).__name__}
    active.remove(identity)
    return result


def dump(tree):
    return {
        'nodes': [dict(properties(n),
                       scene=None if n.bl_idname == 'CompositorNodeRLayers' else 'not_rlayers',
                       inputs=[properties(s) for s in n.inputs],
                       outputs=[properties(s) for s in n.outputs]) for n in tree.nodes],
        'links': [(l.from_node.name, l.from_socket.identifier,
                   l.to_node.name, l.to_socket.identifier) for l in tree.links],
        'interface': [properties(i) for i in tree.interface.items_tree],
        'properties': {k: scalar(tree[k]) for k in tree.keys()},
    }


source, extracted, name, reference, report = sys.argv[sys.argv.index('--') + 1:]
if os.environ.get('OXBLEND_PROOF_BINARY'):
    result = subprocess.run([os.environ['OXBLEND_PROOF_BINARY'], 'summary', extracted], capture_output=True, check=True)
    codes = json.loads(result.stdout)['block_codes']
    assert not codes.get('SC', 0) and not codes.get('OB', 0), codes
expected_version = tuple(int(v) for v in os.environ.get('OXBLEND_PROOF_BLENDER_VERSION', '5.2.1').split('.'))
assert bpy.app.version[:len(expected_version)] == expected_version, bpy.app.version
bpy.ops.wm.open_mainfile(filepath=source, load_ui=False, use_scripts=False)
tree = bpy.data.node_groups.get(name)
assert tree, (name, list(bpy.data.node_groups.keys()))
for node in tree.nodes:
    if node.bl_idname == 'CompositorNodeRLayers':
        node.scene = None
expected = dump(tree)
assert not pathlib.Path(reference).exists()
bpy.data.libraries.write(reference, {tree}, fake_user=True, compress=False)
bpy.ops.wm.open_mainfile(filepath=extracted, load_ui=False, use_scripts=False)
# Opening a standalone library synthesizes an empty Scene in native Blender.
# Both files must emit the expected 'Library file, loading empty scene' warning.
assert not bpy.data.objects
inventory = {p.identifier: len(getattr(bpy.data, p.identifier))
             for p in bpy.data.bl_rna.properties if p.type == 'COLLECTION'}
actual = dump(bpy.data.node_groups[name])
assert actual == expected, ('extracted differs', actual, expected)
assert all(n.scene is None for n in bpy.data.node_groups[name].nodes if n.bl_idname == 'CompositorNodeRLayers')
bpy.ops.wm.read_factory_settings(use_empty=True)
with bpy.data.libraries.load(extracted, link=False) as (src, dst):
    assert name in src.node_groups
    dst.node_groups = [name]
assert dump(dst.node_groups[0]) == expected, 'append differs'
bpy.ops.wm.open_mainfile(filepath=reference, load_ui=False, use_scripts=False)
assert dump(bpy.data.node_groups[name]) == expected, 'native reference differs'
reference_inventory = {p.identifier: len(getattr(bpy.data, p.identifier))
                       for p in bpy.data.bl_rna.properties if p.type == 'COLLECTION'}
assert inventory == reference_inventory, (inventory, reference_inventory)
pathlib.Path(report).write_text(json.dumps({'complete': True, 'version': bpy.app.version_string,
    'runtime_inventory': inventory, 'expected_warning': 'Library file, loading empty scene',
    'nodes': len(expected['nodes']), 'links': len(expected['links']), 'canonical': expected}, indent=2))
print('EXTRACT_NATIVE_PASS')
