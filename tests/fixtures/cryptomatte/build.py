"""Clean-room fixture, Blender 5.2.1 LTS 9e2066aef7ef, factory startup.
Run: blender -b --factory-startup --python-exit-code 1 --python build.py -- NEW_DIR
No project inputs; tiny PNGs are sufficient to exercise saved Image pointers.
"""
import bpy
import json
import shutil
import sys
from pathlib import Path

out = Path(sys.argv[sys.argv.index('--') + 1]).resolve()
out.mkdir(parents=True, exist_ok=False)
bpy.ops.wm.read_factory_settings(use_empty=True)
image = bpy.data.images.new('pixels', width=2, height=2)
image.filepath_raw = str(out / 'base.png')
image.file_format = 'PNG'
image.save()
bpy.data.images.remove(image)
for frame in range(1, 6):
    shutil.copyfile(out / 'base.png', out / f'shot.{frame:04}.png')
image = bpy.data.images.load(str(out / 'shot.0001.png'))
image.filepath = '//shot.0001.png'
image.source = 'SEQUENCE'
tree = bpy.data.node_groups.new('Cryptomatte fixture', 'CompositorNodeTree')
bpy.context.scene.compositing_node_group = tree
node = tree.nodes.new('CompositorNodeCryptomatteV2')
node.source = 'IMAGE'
node.image = image
node.frame_duration = 3
node.frame_offset = 2
node.frame_start = 17
node.use_auto_refresh = True
bpy.context.scene.frame_start = 17
bpy.context.scene.frame_end = 19
bpy.context.scene.frame_set(17)

def save(name):
    bpy.ops.wm.save_as_mainfile(filepath=str(out / name), check_existing=False,
                              relative_remap=False, compress=True)

save('sequence.blend')
node.use_auto_refresh = False
save('no-refresh.blend')
node.use_auto_refresh = True
node.frame_offset = -1
save('negative.blend')
node.frame_offset = 2
for tile in (1001, 1002):
    shutil.copyfile(out / 'base.png', out / f'tile.{tile}.png')
image.source = 'TILED'
image.filepath = '//tile.<UDIM>.png'
image.tiles.new(1002)
save('tiles.blend')
image.source = 'SEQUENCE'
image.filepath = '//shot.0001.png'
# Same Image + direct ImageUser storage: the compositor Image path is already supported.
tree.nodes.remove(node)
node = tree.nodes.new('CompositorNodeImage')
node.image = image
node.frame_duration = 3
node.frame_offset = 2
node.frame_start = 17
node.use_auto_refresh = True
save('compositor-image.blend')
tree.nodes.remove(node)
# A fresh render-source node references a Scene; no Image remains in this file.
node = tree.nodes.new('CompositorNodeCryptomatteV2')
node.source = 'RENDER'
node.scene = bpy.context.scene
bpy.data.images.remove(image)
save('render.blend')
(out / 'native.json').write_text(json.dumps({
    'author': 'Zamil', 'version': bpy.app.version_string,
    'build_hash': bpy.app.build_hash.decode(),
    'sequence': {'filepath': '//shot.0001.png', 'source': 'SEQUENCE',
                 'frames': 3, 'offset': 2, 'sfra': 17, 'members': [3, 4, 5]},
    'render': {'source': node.source, 'image': None, 'scene': node.scene.name},
}, indent=2) + '\n')
