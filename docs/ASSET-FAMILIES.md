# File-bearing and known non-file ID families

Authored by Zamil from **Blender 5.2.1 LTS, build `9e2066aef7ef`**, native
RNA and the SDNA serialized by that executable, not another dependency tool.
The classification concerns **direct file declarations**, not absence of ID,
node, animation or library edges. Those still use the ordinary graph walk.
Unknown codes still emit `unknown_asset_code`; this is a semantic registry,
not a warning-count filter.

## Known non-file families

| Code | Saved DNA type | Why no direct file usage is emitted |
| --- | --- | --- |
| AC | bAction | Animation records and ID links; no filepath declaration. |
| AR | bArmature | Bone/pose settings and ID links; no filepath declaration. |
| CU | Curve | Geometry/text data; font files belong to linked VFont IDs, not the curve. |
| CV | Curves | Embedded geometry/custom data; no filepath declaration. |
| GD / GP | bGPdata / GreasePencil | Saved drawings/material/ID links; no direct filepath. |
| GR | Collection (legacy Group alias) | Object/collection membership, not a file. |
| KE | Key | Embedded shape-key geometry/ID links, not a file. |
| LA | Lamp (Light alias) | Light settings/node-tree links; image files belong to Image IDs. |
| LP | LightProbe | Probe settings/data, no external-file declaration. |
| LS | FreestyleLineStyle | Line/material/node links, no direct filepath. |
| LT | Lattice | Embedded deform geometry, no filepath declaration. |
| MA | Material | Node/image/ID links, no direct filepath. |
| MB | MetaBall | Embedded geometry, no filepath declaration. |
| MS | Mask | Mask layers/animation, no direct filepath. |
| PA | ParticleSettings | Simulation/texture/ID settings; caches are traced at their Object/modifier owners. |
| PC / PL | PaintCurve / Palette | Embedded curve/color data, no filepath declaration. |
| PT | PointCloud | Embedded custom data/geometry, no direct filepath. |
| SK | Speaker | The sound file belongs to its bSound ID. |
| SN / SR | bScreen | UI layout/runtime state, not a project asset filepath. |
| TE | Tex | Texture/image/node links, no direct filepath in this native DNA. |
| WM | wmWindowManager | UI/runtime state; process-only pointers are not file declarations. |
| WO | World | World/node/image links, no direct filepath. |
| WS | WorkSpace | UI/layout/asset-library selector metadata, not a project asset file. |

The code/type pair is checked against the existing ID registry and its aliases;
a familiar code is not permission to interpret an arbitrary struct as that family.
Existing placeholder `ID` handling and legacy `CO` diagnostic compatibility remain.
This is not a claim to implement every historical nested-data layout or every
future ID family. In particular Brush/other unsupported families are not silently
classified by a name heuristic.

## Camera is a qualified exception, not a no-file family

Older Camera SDNA lacks `custom_filepath`; that is a known Camera, not an unknown
asset code. Modern native DNA includes `char custom_filepath[1024]`,
`int custom_mode`, `Text *custom_shader`, and scalar `char type`. Native RNA enums:

- `custom_mode`: **INTERNAL=0**, **EXTERNAL=1**.
- `type`: PERSP=0, ORTHO=1, PANO=2, **CUSTOM=3**.

A nonempty EXTERNAL filepath is a required saved declaration, even for a non-CUSTOM
camera. Its usage has normal owner/field provenance and resolution, plus
`camera_type` and `active` (true only for CUSTOM). Strict pack requires it either
way. `pack-info.txt` preserves that distinction. Empty paths emit no usage.
The fixed char-array path may be relocated by the existing expected-old guarded
path writer; this grants no pointer or structural editing.

INTERNAL follows the checked `custom_shader` Text ID through the ordinary Text
handling, including its proven storage flags. A stale camera filepath emits
informational `embedded_camera_filepath`, not a required dependency. Unknown or
malformed mode is UNKNOWN: retain a known path fact plus a typed site error;
strict pack refuses. Actual pointer failures remain unknown, never guessed null.

## Evidence and reproducibility

The native ID RNA enum identifies the two-byte codes (its numeric value decoded
little-endian); saved BHeads identify their actual DNA structs. Full SDNA field
lists, including by-value embedded leaves, were inspected alongside native RNA
FILE_PATH/DIR_PATH properties. In particular, treating Camera as unconditionally
non-file would have hidden a real shader dependency.

Native saved-DNA evidence file SHA-256:
`e897610eb2b39d0168a66ec17e7b59cbd264f3e8808c6ab12e471ae796d804a2`.
Full inspect report SHA-256:
`39568f1eaddf99dc272528a9fdf9033c77491b53e90de42f0781b62177de791c`.
Native RNA evidence SHA-256:
`ed1900cedba380078734018d283169a8049c9902a518dc933be322d03d3ae358`.
Native ID-code enum evidence SHA-256:
`4be3f388b05439ab1a1ff773fffd4b8bd8173ceaf4cb3f97631bd9f7f2ca5314`.
These identify qualification artifacts, not runtime allowlists or version checks.

The reproducible native procedure is to create fresh datablocks in factory-startup
Blender, save an uncompressed blend, run `oxblend inspect` on it and inspect that
file's own SDNA; enumerate `bpy.types.ID.bl_rna.properties['id_type'].enum_items`
and each family's RNA properties in the same executable. Unit wire vectors
exercise the code/type and Camera mode contracts independently of Blender.

**Native limit:** `bpy.utils.blend_paths()` is a conservative saved-path census,
not a liveness proof. It includes an EXTERNAL declaration even with PERSP type;
it also includes a stale custom_filepath in INTERNAL mode. The latter is
explicitly not used as evidence that the internal shader needs that file.
Hidden-source reopen preserved the mode, shader Text and embedded contents for
INTERNAL, while EXTERNAL kept the now-absent declared path. This proves saved
classification/retention and native reopen, **not rendering or shader evaluation**.
