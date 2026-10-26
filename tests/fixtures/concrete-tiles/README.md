# Concrete-tile fixtures (L18)

Authored by Zamil for L18 with Blender 5.2.1 LTS (factory startup,
background, `--disable-autoexec`); no studio files. Reproduce into a **new**
scratch directory (never overwrite these inputs):

```
blender --factory-startup --background --disable-autoexec --python-exit-code 1 \
  --python build.py -- /new/scratch/fixture
```

`concrete.blend` saves two TILED images whose filepaths are concrete tile
files (no `<UDIM>`/`<UVTILE>` token):
`ConcUDIM` (`//conc_udim_1005.png`, tiles 1001/1005) and `ConcUV`
(`//conc_uv_u2_v1.png`, tiles 1001/1002). Blender refuses to *save* such
images (image_save.cc:667) but keeps the saved `Image.tiles` list on load;
Oxblend must auto-tokenize the concrete basename with Blender's filename-only
regexes (image.cc:3773) and expand only the saved tile numbers. The committed
tree deliberately omits `conc_udim_1001.png` (`conc_udim_1001_supplied.png`
is the same 16x16 red tile_fill); supplying it as `conc_udim_1001.png` makes
the closure complete.

`unknown-concrete.blend` saves `Plain` (`//plain.png`, untokenizable) and
`DirTok` (`//<UDIM>/still.png`, token only in the directory component),
both with tiles 1001/1002. Both stay `pattern_members_unknown`: a directory
token is not a UDIM set (filename-only rule, image.cc:3767).

`native.json` records the Blender version and the saved tile lists.
