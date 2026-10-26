# Clean-room native editor qualifier

Authored by Zamil for L20, with Mario's explicit approval. `generate.py` starts
with factory data, disables factory add-ons, deletes unused IDs and authors only
one Scene and a small standalone compositor tree. It never opens, appends or
links any `.blend`, including the corpus and the engine template. The native
partial-library writer saves uncompressed data without setting the source Main's
empty filepath. No bytes are patched after writing.

- Blender: **5.2.1 LTS**, build **9e2066aef7ef**.
- Executable: `blender-5.2.1-linux-x64/blender`; SHA-256
  `c2fd82553c979a7f6ba85202c487aa1173c90db588a67d74d70cc7b0c2bea01c`.
- Generator revision: `eb095b9`; script SHA-256
  `5e62299284deaf08762151af6e5a5d6ebd0ebd0eb56932b0a210f8b232b9f434`.
- `qualifier.blend`: **209110 bytes**; SHA-256
  `6e5deefb1b96c8726a7f205f612711f5cfb0f578e1babfebef8893293359dd09`.
- Header: version **502**, format **1**, 64-bit little-endian.
- An independent second generation produced SHA-256
  `93816986ca701a45987b33da2a7442fa00c07b43aa86864c731e83ff29d02a16`.
  **Generation is not byte-stable.** Tests assert authored semantics, not a
  regenerated file hash or allocation addresses.
- Both generated files were scanned for absolute path strings and local
  usernames; none were found. The sole authored render path is relative.

The qualification's private receiving evidence retains the exact executable
path, both commands/stdout/stderr/exits and full hashes beside the working
copies (`L20/native521/provenance.json` and `privacy-and-stability.json`). The
portable executable locator above avoids putting the host's account path in
this distributable fixture note.

The original fixture above was generated at revision `eb095b9`. The current
generator also accepts caller-selected 5.2.x and 5.3.x binaries for future
per-build fixtures; generation is not a claim of engine compatibility. Structural
editing is currently qualified on 5.2.1 only; support for every 5.2.x and 5.3.x
build is planned separately. The receiving checks here remain pinned to the actually
qualified build until that work lands.

Run from a new directory per build with the script's absolute path:

```sh
blender --background --factory-startup --disable-autoexec --python-exit-code 1 \
  --python /path/to/generate.py
```

`full_parity.py --stage write --blender /path/to/blender-5.2.1 ...` edits the
Scene resolution through the Python binding in both typed modes and removes a
node through structural editing. It requires actual publication, reads each new
file through the CLI and Python, then invokes `verify.py` in isolated factory
Blender to check the values and graph. Receiving proof is a separate observed
result, not established merely by this fixture's hashes. This authored qualifier
adds to, and never replaces, the public-corpus accounting.
