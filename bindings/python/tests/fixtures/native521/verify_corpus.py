"""Read-only native check of one explicitly expected public-corpus edit.

After --: NEW_EDITED_FILE EXPECTATIONS_JSON. Never opens/resaves an original.
"""
import json
import sys
from pathlib import Path

import addon_utils
import bpy

assert bpy.app.version == (5, 2, 1), bpy.app.version_string
assert bpy.data.filepath == '', 'use --factory-startup'
for addon in list(bpy.context.preferences.addons):
    addon_utils.disable(addon.module, default_set=True)
assert not list(bpy.context.preferences.addons)
filename, expectation_file = sys.argv[sys.argv.index('--') + 1:]
expected = json.loads(Path(expectation_file).read_text())
bpy.ops.wm.open_mainfile(filepath=str(Path(filename).resolve(strict=True)), load_ui=False)
scene = bpy.data.scenes[expected['scene']]
assert scene.render.resolution_x == expected['resolution_x']
assert scene.render.resolution_y == expected['resolution_y']
print('L20_NATIVE_CORPUS_RECEIVED', json.dumps({
    'version': bpy.app.version_string, 'build': bpy.app.build_hash.decode(),
    'scene': scene.name, 'resolution_x': scene.render.resolution_x,
    'resolution_y': scene.render.resolution_y,
}, sort_keys=True))
