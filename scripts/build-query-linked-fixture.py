"""Synthetic saved/reopen fixture for linked node-tree subtype lookup.
Run in factory/background Blender, disable-autoexec; argv after -- is a NEW dir.
No artist file, render, network, pixels or external assets are used.
"""
import bpy
import hashlib
import json
from pathlib import Path
import sys
out = Path(sys.argv[sys.argv.index('--') + 1]).resolve()
if '--verify' in sys.argv:
    assert bpy.data.version == (5, 2, 44)
    root = bpy.context.scene.compositing_node_group
    group = next(n for n in root.nodes if n.bl_idname == 'CompositorNodeGroup')
    assert group.node_tree.bl_idname == 'CompositorNodeTree'
    assert group.node_tree.library is not None
    print(json.dumps({'native_reopen': True, 'subtype': group.node_tree.bl_idname,
                      'library': group.node_tree.library.filepath,
                      'blender': bpy.app.version_string, 'build_hash': bpy.app.build_hash.decode()}))
else:
    out.mkdir(parents=True, exist_ok=False)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    forwarded = bpy.data.node_groups.new('SharedTree', 'CompositorNodeTree')
    bpy.data.libraries.write(str(out / 'forwarded.blend'), {forwarded})
    bpy.data.node_groups.remove(forwarded)
    with bpy.data.libraries.load(str(out / 'forwarded.blend'), link=True) as (source, target):
        target.node_groups = ['SharedTree']
    forwarded = target.node_groups[0]
    local = bpy.data.node_groups.new('SharedTree', 'CompositorNodeTree')
    nested = local.nodes.new('CompositorNodeGroup')
    nested.node_tree = forwarded
    # Includes a same-named forwarding stub: #30 must select the LOCAL tree.
    bpy.data.libraries.write(str(out / 'group-library.blend'), {local}, path_remap='RELATIVE_ALL')
    bpy.ops.wm.read_factory_settings(use_empty=True)
    with bpy.data.libraries.load(str(out / 'group-library.blend'), link=True) as (source, target):
        target.node_groups = ['SharedTree']
    linked = target.node_groups[0]
    root = bpy.data.node_groups.new('RootTree', 'CompositorNodeTree')
    group = root.nodes.new('CompositorNodeGroup')
    group.node_tree = linked
    bpy.context.scene.compositing_node_group = root
    bpy.ops.wm.save_as_mainfile(filepath=str(out / 'root.blend'), relative_remap=True)
    manifest = {'blender': bpy.app.version_string, 'build_hash': bpy.app.build_hash.decode(),
                'files': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in out.glob('*.blend')}}
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(manifest))
