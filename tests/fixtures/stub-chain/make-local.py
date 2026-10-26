# Authored by Zamil: native local-vs-forwarded same-name selection proof.
# Run using Blender 5.2.1 --factory-startup --background --disable-autoexec
# --python-exit-code 1 --python make-local.py. Existing leaf.blend is read-only.
import bpy
import pathlib

out = pathlib.Path(__file__).resolve().parent
bpy.ops.wm.read_factory_settings(use_empty=True)
with bpy.data.libraries.load(str(out / 'leaf.blend'), link=True) as (src, dst):
    dst.objects = ['Leaf']
bridge = bpy.data.collections.new('Bridge')
bridge.objects.link(dst.objects[0])
bpy.context.scene.collection.children.link(bridge)
local = bpy.data.objects.new('Leaf', None)
assert local.name == 'Leaf' and local.library is None
local['origin'] = 'LOCAL'
bpy.context.scene.collection.objects.link(local)
bpy.ops.wm.save_as_mainfile(filepath=str(out / 'both.blend'), compress=False,
                           relative_remap=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
with bpy.data.libraries.load(str(out / 'both.blend'), link=True) as (src, dst):
    dst.objects = ['Leaf']
bpy.context.scene.collection.objects.link(dst.objects[0])
assert dst.objects[0]['origin'] == 'LOCAL'
bpy.ops.wm.save_as_mainfile(filepath=str(out / 'pick.blend'), compress=False,
                           relative_remap=True)
bpy.ops.wm.open_mainfile(filepath=str(out / 'pick.blend'), load_ui=False,
                         use_scripts=False)
assert len(bpy.data.objects) == 1
assert bpy.data.objects[0]['origin'] == 'LOCAL'
paths = bpy.utils.blend_paths(absolute=True, packed=False, local=False)
assert paths == [str(out / 'both.blend')], paths
print('NATIVE_LOCAL_WINS', bpy.app.version_string, paths)
bpy.ops.wm.read_factory_settings(use_empty=True)
with bpy.data.libraries.load(str(out / 'both.blend'), link=True) as (src, dst):
    dst.collections = ['Bridge']
bpy.context.scene.collection.children.link(dst.collections[0])
bpy.ops.wm.save_as_mainfile(filepath=str(out / 'direct.blend'), compress=False,
                           relative_remap=True)
bpy.ops.wm.open_mainfile(filepath=str(out / 'direct.blend'), load_ui=False,
                         use_scripts=False)
assert len(bpy.data.objects) == 1
assert bpy.data.objects[0].library.filepath == '//leaf.blend'
assert not bpy.data.objects[0].is_missing
print('NATIVE_DIRECT_STUB_FOLLOWED', bpy.app.version_string)
