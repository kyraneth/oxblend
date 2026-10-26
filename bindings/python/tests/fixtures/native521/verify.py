"""Read-only native receiving check for L20's own edited fixture.

Arguments after --: format-only|comp|node-tree NEW_EDITED_FILE.
This checker is separate from generate.py: the generator never opens a blend.
"""
import json
import sys
from pathlib import Path

import addon_utils
import bpy

assert bpy.app.version == (5, 2, 1), bpy.app.version_string
assert bpy.data.filepath == '', 'start with --factory-startup'
for addon in list(bpy.context.preferences.addons):
    addon_utils.disable(addon.module, default_set=True)
assert not list(bpy.context.preferences.addons)
mode, filename = sys.argv[sys.argv.index('--') + 1:]
assert mode in {'format-only', 'comp', 'node-tree'}
source = Path(filename).resolve(strict=True)
bpy.ops.wm.open_mainfile(filepath=str(source), load_ui=False)
scene = bpy.data.scenes['L20Scene']
tree = bpy.data.node_groups['L20Tree']
assert scene.compositing_node_group == tree
assert not list(bpy.data.libraries)
assert scene.render.resolution_x == (320 if mode == 'node-tree' else 640)
assert scene.render.resolution_y == 240
assert tree.nodes['L20Value'].outputs[0].default_value == 0.25
assert tree.nodes['L20Value'].label == 'before'
assert set(tree.nodes.keys()) == ({'L20Value'} if mode == 'node-tree' else {'L20Value', 'L20Math'})
assert len(tree.links) == (0 if mode == 'node-tree' else 1)
print('L20_NATIVE_RECEIVED', json.dumps({
    'version': bpy.app.version_string, 'build': bpy.app.build_hash.decode(),
    'mode': mode, 'resolution': [scene.render.resolution_x, scene.render.resolution_y],
    'nodes': sorted(tree.nodes.keys()), 'links': len(tree.links),
    'value': tree.nodes['L20Value'].outputs[0].default_value,
    'label': tree.nodes['L20Value'].label,
}, sort_keys=True))
