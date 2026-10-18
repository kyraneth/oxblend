# Pointer namespace contracts

Synthetic serialized files, not native Blender fixtures or templates. `generate.py` derives the small schema from the parent generator. The alias fixture (SHA `1b1632b8c4c51f19dbd0f78afde72b3dbb3f186f796081b3dc5f2984659cdf61`) gives one address both owner-local DATA and global ID meanings; the tests must resolve it by pointer kind, not address alone.

- `owner-id-alias`: Object A's ID-valued `data` (declared char* in this crafted fixture) aliases owner DATA and Mesh ID; it must return the Mesh, never the DATA.
- `id-only-data`: no Mesh header; the same ID-valued pointer must refuse rather than choose local DATA.
- `data-only-id`: A's modifiers and override DATA pointers target an address with only a Mesh ID. Getters, trace, overrides and pack refuse.
- `material-pointer-table`: Material** outer DATA address and its inner Material* address both collide with global/local allocations. The outer hop must select DATA, the inner hop ID. `selected-material-root` and `selected-material-child` make the inner hop observable through actual linked-ID selection: only the correct Material ID reaches the deliberately missing downstream library. A mutation retaining the outer DATA depth must fail.
- `typed-*`: adds an actual Mesh* field and an IDOverrideLibrary* field. These prove leading-ID type classification independently of Object.data's untyped profile. IDOverrideLibrary does not embed ID, despite its name.

Literal addresses/counts/widths define fixture topology, not runtime limits. All native source bodies stay unchanged. Native embedded nodetree proof separately uses the committed 4.5.8 crypto-image fixture.
