# Authored by Zamil: native packed Image with a real PNG byte allocation.
# Blender --factory-startup --background --python-exit-code 1 --python SCRIPT.
import bpy
import pathlib
out = pathlib.Path(__file__).resolve().parent
bpy.ops.wm.read_factory_settings(use_empty=True)
image = bpy.data.images.new('Generated', width=1, height=1)
image.filepath_raw = str(out / 'packed.png')
image.file_format = 'PNG'
image.save()
bpy.data.images.remove(image)
image = bpy.data.images.load(str(out / 'packed.png'))
image.use_fake_user = True
# The packed-file record copies this path verbatim; make it public before pack.
image.filepath = '//packed.png'
png = (out / 'packed.png').read_bytes()
image.pack(data=png, data_len=len(png))
assert image.source == 'FILE' and image.packed_file is not None
bpy.ops.wm.save_as_mainfile(filepath=str(out / 'packed.blend'), compress=False,
                           relative_remap=True)
(out / 'packed.png').unlink()
bpy.ops.wm.open_mainfile(filepath=str(out / 'packed.blend'), load_ui=False,
                         use_scripts=False)
assert bpy.data.images[0].packed_file is not None
print('NATIVE_PACKED_IMAGE_OK', bpy.app.version_string)
