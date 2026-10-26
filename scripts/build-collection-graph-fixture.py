"""Generic synthetic fixture for the saved collection graph.

Run only in factory/background Blender with autoexec disabled. The argv after
``--`` is a NEW directory; nothing is overwritten. The builder creates this
nested saved shape:

    root.blend Scene master
      -> SetCollection (linked from set-library.blend)
           -> NestedCollection (linked from nested-library.blend)
                -> NestedLocalObject (local to nested-library.blend)
           -> SetLocalObject (local to set-library.blend)
      -> RootLocalObject (local to root.blend)
      -> RootParentObject (local to root.blend, outside membership; parent of
         RootLocalObject)

The same SetLocalObject is also linked into the root scene collection, so the
graph has two saved membership edges to one deduplicated record. Names and
paths are generic; no artist or studio file is read. No render/evaluation is
performed. A manifest pins every produced byte.
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
        raise SystemExit("usage: blender ... --python build-collection-graph-fixture.py -- NEW_DIR")
    out = Path(args[0]).resolve()
    out.mkdir(parents=True, exist_ok=False)

    # Library B: a local collection and its local object.
    clear()
    nested = bpy.data.collections.new("NestedCollection")
    bpy.context.scene.collection.children.link(nested)
    nested_object = bpy.data.objects.new("NestedLocalObject", None)
    nested.objects.link(nested_object)
    save(out / "nested-library.blend")

    # Library A: local SetCollection containing the linked NestedCollection and
    # its own local object.
    clear()
    nested_linked = linked_collection(out / "nested-library.blend", "NestedCollection")
    set_collection = bpy.data.collections.new("SetCollection")
    bpy.context.scene.collection.children.link(set_collection)
    set_collection.children.link(nested_linked)
    set_object = bpy.data.objects.new("SetLocalObject", None)
    set_collection.objects.link(set_object)
    save(out / "set-library.blend")

    # Root: linked SetCollection in the scene master, a root-local object, a
    # parent-only object outside membership, and a second membership edge to
    # the same library-A object.
    clear()
    set_linked = linked_collection(out / "set-library.blend", "SetCollection")
    master = bpy.context.scene.collection
    master.children.link(set_linked)

    root_object = bpy.data.objects.new("RootLocalObject", None)
    master.objects.link(root_object)
    parent_object = bpy.data.objects.new("RootParentObject", None)
    # Keep the parent-only object saved without creating a membership edge.
    parent_object.use_fake_user = True
    root_object.parent = parent_object

    shared = next(
        (obj for obj in set_linked.objects if obj.name == "SetLocalObject"), None
    )
    shared_linked_twice = shared is not None
    if shared is not None:
        master.objects.link(shared)
    save(out / "root.blend")

    files = sorted(out.glob("*.blend"))
    manifest = {
        "schema": "oxblend.collection-graph-fixture/1",
        "blender": bpy.app.version_string,
        "build_hash": bpy.app.build_hash.decode()
        if isinstance(bpy.app.build_hash, bytes)
        else str(bpy.app.build_hash),
        "factory_empty": True,
        "autoexec_required": False,
        "rendered": False,
        "evaluated": False,
        "names": {
            "root_scene": bpy.context.scene.name,
            "master_collection": master.name,
            "set_collection": "SetCollection",
            "nested_collection": "NestedCollection",
            "set_object": "SetLocalObject",
            "nested_object": "NestedLocalObject",
            "root_object": "RootLocalObject",
            "parent_object": "RootParentObject",
        },
        "shared_object_linked_twice": shared_linked_twice,
        "files": {
            path.name: {
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "size": path.stat().st_size,
            }
            for path in files
        },
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest))


main()
