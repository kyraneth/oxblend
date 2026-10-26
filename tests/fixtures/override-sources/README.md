# Native override-source closure fixture

Authored by Zamil in Blender 5.2.1 LTS, factory startup, autoexec disabled.
Regenerate into a new directory with `tests/support/make-override-sources.py`.
No third-party scene bytes or executable Text datablocks.

`root.blend` instances the local override collection in `middle.blend`. The
original EMPTY Object in `source.blend` instances a collection in `nested.blend`.
The local override clears that instance. Blender nevertheless loads the original
reference, source and nested libraries. Trace/pack must include both, and default
strict pack must refuse if either is unavailable. No unrelated Library census
or fallback-by-basename is valid.

Native source-hidden save/reopen evidence is retained privately in the requested
pack-trust acceptance resource directory; the Rust CLI regression uses only
these portable relative paths and requires no Blender installation.
