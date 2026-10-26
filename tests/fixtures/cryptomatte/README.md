# Cryptomatte dependency fixtures

Authored by Zamil with the adjacent clean-room `build.py`, run locally in
Blender 5.2.1 LTS (`9e2066aef7ef`) with factory startup. The compressed saves
contain no artist/project data. `base.png` supplies identical tiny dependency
bytes; tests create the named frames/tiles in an isolated source directory.

- `sequence.blend`: Image-source Cryptomatte, SEQUENCE, duration 3, offset 2,
  start 17, auto-refresh enabled. Required saved span: frames 3–5.
- `no-refresh.blend`: same, auto-refresh disabled; existing image-node semantics
  additionally retain saved current frame 0.
- `negative.blend`: offset -1 must remain unknown under existing rules.
- `tiles.blend`: Image-source Cryptomatte with saved UDIM tiles 1001 and 1002.
- `compositor-image.blend`: direct ImageUser control, same sequence parameters.
- `render.blend`: render-source Cryptomatte points at a Scene; no Image ID.

The engine resolves `NodeCryptomatte.iuser` and ImageUser fields by names in
each file's SDNA. No factory-profile offsets are used. The pointer association
already restricts image users to nodes whose `id` points at that particular
Image; a render-source Cryptomatte has a Scene/null ID, not an Image ID.

Blender source checked at tag `v5.2.1` and beta commit `32f5c9c48bca`:
`node_composite_cryptomatte.cc` uses `node_storage(...).iuser` with
`BKE_image_user_frame_calc`; RNA maps duration/offset/start to
`frames`/`offset`/`sfra` and clears `node.id` on source change.
`sfra` controls scene-frame mapping, not the declared offset+duration span.

Other storage audit (no speculative fixes): compositor Image already stores a
**direct ImageUser** (covered here); shader Image/Environment use the existing
embedded-iuser branches. These and Cryptomatte are the only embedded ImageUser
fields in `DNA_node_types.h` for those versions. File Output's
NodeCompositorFileOutput / old NodeImageMultiFile[Socket], NodeImageFile and
NodeImageLayer are output-path/format/pass metadata, not an Image ID plus
ImageUser. NodeImageAnim is deprecated versioning data. Geometry image textures
obtain images and frame values from sockets and construct ImageUser at runtime;
NodeGeometryImageTexture has no saved iuser. Such dynamic-frame interpretation
is not this recognition gap and remains outside this patch.
