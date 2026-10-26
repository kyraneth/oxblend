# Saved pattern requirements (Blender 5.2.1 LTS)

Authored by Zamil for L10, not copied from another blend reader.
`patterns.blend` saves two Image tiles (1001/1002) and a shader sequence
ImageUser (duration 3, offset 0, start 1). The committed assets deliberately omit
tile1002 and frame0002. `offset.blend` saves duration3/offset2/cyclic/start5;
`unknown-negative.blend` and `unknown-user.blend` cover unsupported/unassociated
spans. `uvtile.blend` saves the two tiles with the UVTILE filename token.
`cross-user.blend` directly links the sequence Image from `patterns.blend` but
saves a root-local ImageUser duration5; frames4/5 are required even though the
library-local ImageUser duration is only3. Its automatic refresh is disabled,
so the saved frozen current frame0 is also required: native_cross_eval.py
confirms that frame0 remains selected while rendering. The test first proves
missing4/5, then supplies1..5 and proves that missing0 still blocks COMPLETE.
Reproduce it with `cross_user.py`
after build.py; all library paths remain relative.

Reproduce into a **new** scratch directory (never overwrite these inputs):

```
blender --factory-startup --background --disable-autoexec --python-exit-code 1 --python build.py -- /new/scratch/fixture
blender --factory-startup --background --disable-autoexec --python-exit-code 1 --python variants.py -- /new/scratch/fixture
blender --factory-startup --background --disable-autoexec --python-exit-code 1 --python native_eval.py -- /new/scratch/fixture
```

`native_eval.py` renders tiny CPU frames, reads evaluated ImageUser state and
calls native `Image.filepath_from_user`; it never saves the loaded blend.
Observed Blender5.2.1: duration3/offset0 uses frames1..3 (then clamps);
offset2/cyclic/start5 uses frames3..5 then repeats. Negative offset is explicitly
UNKNOWN in Oxblend pending broader boundary qualification, not guessed from a
directory glob. The scene range does not narrow the saved playback declaration.
These are member-selection probes, not render-equivalence qualification.
