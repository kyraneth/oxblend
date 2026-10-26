"""Author the clean-room L20 editor fixture; never read another .blend.

Run in a new working directory with the chosen Blender 5.2.x or 5.3.x binary:
  blender --background --factory-startup --disable-autoexec --python-exit-code 1 \
    --python /absolute/path/to/generate.py
The native partial-library writer keeps the factory Main's empty source path.
No template, external asset, startup file or corpus input is consumed.
"""
from pathlib import Path
import hashlib
import json

import addon_utils
import bpy

# The CLI-selected executable owns the build: reuse this script in a separate
# directory per build. Generation does not claim engine qualification for it.
assert bpy.app.version[:2] in {(5, 2), (5, 3)}, bpy.app.version_string
assert bpy.data.filepath == '', 'use --factory-startup, not an existing file'
for addon in list(bpy.context.preferences.addons):
    addon_utils.disable(addon.module, default_set=True)
assert not list(bpy.context.preferences.addons)
output = Path('qualifier.blend')
assert not output.exists(), 'fixture generation requires a new working directory'

# Keep only the factory Scene; no geometry, materials, images or external data.
for collection in (bpy.data.objects, bpy.data.meshes, bpy.data.materials,
                   bpy.data.cameras, bpy.data.lights, bpy.data.worlds, bpy.data.images):
    for item in list(collection):
        collection.remove(item, do_unlink=True)
scene = bpy.context.scene
scene.name = 'L20Scene'
scene.render.filepath = '//render'
scene.render.resolution_x = 320
scene.render.resolution_y = 240
scene.render.resolution_percentage = 100
scene.frame_start = 1
scene.frame_end = 2

tree = bpy.data.node_groups.new('L20Tree', 'CompositorNodeTree')
tree.use_fake_user = True
scene.compositing_node_group = tree
value = tree.nodes.new('ShaderNodeValue')
value.name = 'L20Value'
value.label = 'before'
value.location = (-200, 0)
value.outputs[0].default_value = 0.25
math = tree.nodes.new('ShaderNodeMath')
math.name = 'L20Math'
math.operation = 'ADD'
math.location = (0, 0)
math.inputs[1].default_value = 0.5
tree.links.new(value.outputs[0], math.inputs[0])
assert not list(bpy.data.libraries)
assert not list(bpy.data.images)
# This saves native Scene/tree dependency closure, without user UI/preferences,
# and does not assign the absolute destination as the source Main filepath.
bpy.data.libraries.write(str(output), {scene, tree}, path_remap='RELATIVE_ALL',
                         fake_user=True, compress=False)
assert bpy.data.filepath == ''
body = output.read_bytes()
assert body.startswith(b'BLENDER'), 'compression must remain off'
assert Path.cwd().as_posix().encode() not in body, 'save path leaked into fixture'
print('L20_NATIVE_FIXTURE', json.dumps({
    'version': bpy.app.version_string, 'build': bpy.app.build_hash.decode(),
    'file': output.name, 'bytes': len(body), 'sha256': hashlib.sha256(body).hexdigest(),
    'header_hex': body[:16].hex(), 'scene': scene.name, 'tree': tree.name,
    'value': value.outputs[0].default_value, 'label': value.label,
    'nodes': sorted(node.name for node in tree.nodes), 'links': len(tree.links),
}, sort_keys=True))
