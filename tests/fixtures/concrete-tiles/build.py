"""L18 concrete-tile fixtures (public, no studio files).

Builds two blends with Blender 5.2.1 (factory startup, background,
--disable-autoexec):
  concrete.blend         TILED ConcUDIM  filepath //conc_udim_1005.png  tiles [1001,1005]
                         TILED ConcUV   filepath //conc_uv_u2_v1.png    tiles [1001,1002]
  unknown-concrete.blend TILED Plain     filepath //plain.png           tiles [1001,1002]
                         TILED DirTok   filepath //<UDIM>/still.png   tiles [1001,1002]
                         (concrete basename carries no token; the directory
                         token must not count, per filename-only rule)

Tile PNGs are 16x16 tile_fill renders. Token-path saves are copied to the
committed concrete names; the committed tree deliberately omits
conc_udim_1001.png so the first trace/pack is missing-member incomplete.
Run: blender --factory-startup --background --disable-autoexec \
       --python-exit-code 1 --python build.py -- OUTDIR
"""
import bpy
import json
import shutil
import sys
from pathlib import Path

OUT = Path(sys.argv[sys.argv.index("--") + 1])
OUT.mkdir(parents=True, exist_ok=False)

bpy.ops.wm.read_factory_settings(use_empty=True)


def tile_png(image, number, color):
    if number != 1001:
        image.tiles.new(tile_number=number, label="")
    for tile in image.tiles:
        if tile.number != number:
            continue
        image.tiles.active = tile
        with bpy.context.temp_override(edit_image=image):
            bpy.ops.image.tile_fill(color=color, width=16, height=16,
                                    float=False, alpha=True)


def make(name, token_path, tiles_colors):
    image = bpy.data.images.new(name, width=16, height=16, alpha=True,
                                float_buffer=False, tiled=True)
    image.source = "TILED"
    image.use_fake_user = True
    image.filepath = str(token_path)
    image.file_format = "PNG"
    for number, color in tiles_colors:
        tile_png(image, number, color)
    image.save()
    return image


RED = (1.0, 0.0, 0.0, 1.0)
GREEN = (0.0, 1.0, 0.0, 1.0)

conc_udim = make("ConcUDIM", OUT / "tok_udim.<UDIM>.png",
                 [(1001, RED), (1005, GREEN)])
conc_uv = make("ConcUV", OUT / "tok_uv.<UVTILE>.png",
               [(1001, RED), (1002, GREEN)])

bpy.ops.wm.save_as_mainfile(
    filepath=str(OUT / "concrete.blend"), check_existing=False,
    relative_remap=False)

# Repoint the saved filepaths at concrete tile files (never re-saved via
# image.save, which would refuse the token-less path just like Blender does).
conc_udim.filepath = "//conc_udim_1005.png"
conc_uv.filepath = "//conc_uv_u2_v1.png"
concrete_info = {i.name: {"filepath": i.filepath,
                          "tiles": [t.number for t in i.tiles]}
                for i in (conc_udim, conc_uv)}
bpy.ops.wm.save_as_mainfile(
    filepath=str(OUT / "concrete.blend"), check_existing=False,
    relative_remap=False)

for image in list(bpy.data.images):
    bpy.data.images.remove(image)

plain = make("Plain", OUT / "tok_plain.<UDIM>.png",
             [(1001, RED), (1002, GREEN)])
dirtok = make("DirTok", OUT / "tok_dir.<UDIM>.png",
              [(1001, RED), (1002, GREEN)])
plain.filepath = "//plain.png"
dirtok.filepath = "//<UDIM>/still.png"
unknown_info = {i.name: {"filepath": i.filepath,
                         "tiles": [t.number for t in i.tiles]}
                for i in (plain, dirtok)}
bpy.ops.wm.save_as_mainfile(
    filepath=str(OUT / "unknown-concrete.blend"), check_existing=False,
    relative_remap=False)

shutil.copy(OUT / "tok_udim.1005.png", OUT / "conc_udim_1005.png")
shutil.copy(OUT / "tok_uv.u1_v1.png", OUT / "conc_uv_u1_v1.png")
shutil.copy(OUT / "tok_uv.u2_v1.png", OUT / "conc_uv_u2_v1.png")
shutil.copy(OUT / "tok_udim.1001.png", OUT / "conc_udim_1001_supplied.png")
for extra in ("tok_udim.1001.png", "tok_udim.1005.png",
              "tok_uv.u1_v1.png", "tok_uv.u2_v1.png",
              "tok_plain.1001.png", "tok_plain.1002.png",
              "tok_dir.1001.png", "tok_dir.1002.png"):
    (OUT / extra).unlink(missing_ok=True)

(OUT / "native.json").write_text(json.dumps({
    "blender": bpy.app.version_string,
    "concrete": concrete_info,
    "unknown": unknown_info,
}, indent=2) + "\n")
print("wrote", OUT)
(OUT / "DONE").write_text("ok\n")
