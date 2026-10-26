"""Generic gap fixture for shared-subgraph diagnostics.

This is the companion to build-collection-graph-fixture.py. It creates a real
linked collection in a temporary library, saves a root that references it, and
then deliberately removes the library file. The saved root therefore contains a
known membership edge plus an unresolvable linked collection.

    root Scene master
      -> RootGapCollection (local)
           -> SharedGapCollection (local)
                -> MissingGapCollection (linked stub to the now-absent library)

Both local collections are selected by `--type Collection`; whichever row is
visited first expands the shared child, and the other must still receive the
saved gap diagnostic. No artist file is read, nothing is rendered and the root
is never saved by the test.

Run only in factory/background Blender with autoexec disabled:

    blender --background --factory-startup --disable-autoexec \
      --python build-collection-graph-gap-fixture.py -- NEW_DIR

The builder refuses an existing output directory. `missing-library.blend` is
intentionally absent from the final directory; the manifest records its hash
before deletion and the expected diagnostic.
"""

import hashlib
import json
import sys
from pathlib import Path

import bpy


def clear():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def save(path):
    path.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(
        filepath=str(path), relative_remap=True, check_existing=False, compress=False
    )


def linked_collection(path, name):
    with bpy.data.libraries.load(str(path), link=True, relative=True) as (
        source,
        target,
    ):
        target.collections = [name]
    return target.collections[0]


def main():
    args = sys.argv[sys.argv.index("--") + 1 :]
    if len(args) != 1:
        raise SystemExit(
            "usage: blender ... --python build-collection-graph-gap-fixture.py -- NEW_DIR"
        )
    out = Path(args[0]).resolve()
    out.mkdir(parents=True, exist_ok=False)

    # Real linked collection, saved before the root.
    clear()
    missing = bpy.data.collections.new("MissingGapCollection")
    bpy.context.scene.collection.children.link(missing)
    missing_object = bpy.data.objects.new("MissingGapObject", None)
    missing.objects.link(missing_object)
    missing_path = out / "missing-library.blend"
    save(missing_path)
    missing_sha256 = hashlib.sha256(missing_path.read_bytes()).hexdigest()
    missing_size = missing_path.stat().st_size

    # Root retains a saved library pointer to the file we remove below.
    clear()
    missing_linked = linked_collection(missing_path, "MissingGapCollection")
    shared = bpy.data.collections.new("SharedGapCollection")
    bpy.context.scene.collection.children.link(shared)
    shared.children.link(missing_linked)
    root_a = bpy.data.collections.new("RootGapCollection")
    bpy.context.scene.collection.children.link(root_a)
    root_a.children.link(shared)
    save(out / "root.blend")

    # The root now contains the saved edge and stub; remove the bytes so the
    # graph must diagnose a missing library instead of resolving content.
    missing_path.unlink()

    manifest = {
        "schema": "oxblend.collection-graph-gap-fixture/1",
        "blender": bpy.app.version_string,
        "build_hash": bpy.app.build_hash.decode()
        if isinstance(bpy.app.build_hash, bytes)
        else str(bpy.app.build_hash),
        "factory_empty": True,
        "autoexec_required": False,
        "rendered": False,
        "evaluated": False,
        "deleted_library": {
            "name": missing_path.name,
            "sha256": missing_sha256,
            "size": missing_size,
        },
        "expected": {
            "field": "Collection.children",
            "kind": "missing_library",
            "from": "SharedGapCollection",
            "to": "MissingGapCollection",
            "row_roots": ["RootGapCollection", "SharedGapCollection"],
        },
        "files": {
            "root.blend": {
                "sha256": hashlib.sha256((out / "root.blend").read_bytes()).hexdigest(),
                "size": (out / "root.blend").stat().st_size,
            }
        },
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest))


main()
