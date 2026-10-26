#!/usr/bin/env python3
"""Author an independent native partial-library fixture, never a main-file save.
Blender --background --factory-startup --disable-autoexec --python this -- NEW
"""
import bpy
import pathlib
import sys

assert bpy.app.version == (5, 2, 1), bpy.app.version
output = pathlib.Path(sys.argv[sys.argv.index('--') + 1])
assert not output.exists(), output
tree = bpy.data.node_groups.new('ClosureProbe', 'CompositorNodeTree')
render = tree.nodes.new('CompositorNodeRLayers')
render.scene = bpy.context.scene
blur = tree.nodes.new('CompositorNodeBlur')
tree.links.new(render.outputs['Image'], blur.inputs['Image'])
bpy.data.libraries.write(str(output), {tree}, fake_user=True, compress=False)
