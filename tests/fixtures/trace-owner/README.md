# Serialized owner-scope contracts

`generate.py` creates the small, from-scratch SDNA/BHead bodies; `manifest.json` pins their bytes. These are **synthetic serialized branch contracts, not Blender-certified native fixtures or production templates**.

- `cross-id.blend`: two qualified Object IDs own separate DATA runs sharing modifier, override-definition/property and string addresses. Trace must emit both paths; overrides must keep both property names and follow the reference chain in the right owner.
- `same-id.blend`: duplicates the modifier address inside one Object owner; resolution must refuse corruption.
- `foreign-only.blend`: the first Object's modifier address exists only in the second owner's DATA; global unique-address fallback must not admit it.

`tests/trace_owner.rs` drives actual trace/override entries, plus the public pack CLI with two absolute-path relocation writes. The packed source/asset bytes and re-traced publication are checked. Separate Blender-saved scene checks provide independent native-format coverage; their large input bodies are not committed.
