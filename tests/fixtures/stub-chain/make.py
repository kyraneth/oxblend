# Authored by Zamil. Run with Blender 5.2.1 --factory-startup --background
# --disable-autoexec --python-exit-code 1 --python make.py.
# Generates native saved ID stubs. The root's indirect Library path is patched
# to name the intermediate library: Blender follows that stub to the leaf.
import bpy
import pathlib

out = pathlib.Path(__file__).resolve().parent
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.wm.save_as_mainfile(filepath=str(out / 'empty.blend'), compress=False)
obj = bpy.data.objects.new('Leaf', None)
bpy.context.scene.collection.objects.link(obj)
bpy.ops.wm.save_as_mainfile(filepath=str(out / 'leaf.blend'), compress=False)
bpy.ops.wm.read_factory_settings(use_empty=True)
with bpy.data.libraries.load(str(out / 'leaf.blend'), link=True) as (src, dst):
    dst.objects = ['Leaf']
collection = bpy.data.collections.new('Bridge')
collection.objects.link(dst.objects[0])
bpy.context.scene.collection.children.link(collection)
bpy.ops.wm.save_as_mainfile(filepath=str(out / 'midl.blend'), compress=False,
                           relative_remap=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
with bpy.data.libraries.load(str(out / 'midl.blend'), link=True) as (src, dst):
    dst.collections = ['Bridge']
bpy.context.scene.collection.children.link(dst.collections[0])
bpy.ops.wm.save_as_mainfile(filepath=str(out / 'root.blend'), compress=False,
                           relative_remap=True)
root = out / 'root.blend'
data = root.read_bytes()
assert data.count(b'//leaf.blend\0') == 1
root.write_bytes(data.replace(b'//leaf.blend\0', b'//midl.blend\0'))
bpy.ops.wm.open_mainfile(filepath=str(root), load_ui=False, use_scripts=False)
assert len(bpy.data.objects) == 1
obj = bpy.data.objects[0]
assert obj.name == 'Leaf' and not obj.is_missing
assert obj.library.filepath == '//leaf.blend'
print('NATIVE_STUB_CHAIN_OK', bpy.app.version_string)
