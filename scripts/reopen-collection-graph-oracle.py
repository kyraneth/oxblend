"""Native-after-load collection/object oracle; no save, render or evaluation.

Factory/background/disabled-autoexec Blender: -- ROOT --out NEW_JSON.
Only RNA is inspected. Missing RNA-only attributes are explicitly inventoried;
other getter failures fail the oracle rather than becoming empty/default facts.
"""
import hashlib
import json
from pathlib import Path
import sys

import bpy

unexposed = set()


def optional(block, attribute):
    try:
        return getattr(block, attribute)
    except AttributeError:
        unexposed.add(type(block).__name__ + '.' + attribute)
        return None


def library_chain(block):
    library = getattr(block, 'library', None)
    if library is None:
        return None
    chain = []
    current = library
    seen = set()
    while current is not None and current.as_pointer() not in seen:
        seen.add(current.as_pointer())
        chain.append({'name': current.name, 'filepath': current.filepath,
                      'filepath_raw': optional(current, 'filepath_raw')})
        current = getattr(current, 'parent', None)
    return {'name': library.name, 'filepath': library.filepath,
            'filepath_raw': optional(library, 'filepath_raw'), 'chain': chain}


def reference(block):
    return None if block is None else {'name': block.name, 'library': library_chain(block)}


def override_json(block):
    override = getattr(block, 'override_library', None)
    if override is None:
        return {'present': False}
    return {'present': True, 'flag': optional(override, 'flag'),
            'reference': reference(override.reference),
            'hierarchy_root': reference(getattr(override, 'hierarchy_root', None))}


def collection_row(collection):
    row = {'name': collection.name, 'library': library_chain(collection),
           'flag': optional(collection, 'flag'),
           'hide_viewport': collection.hide_viewport,
           'hide_render': collection.hide_render,
           'hide_select': collection.hide_select,
           'override': override_json(collection),
           'children': [c.name for c in collection.children],
           'objects': [o.name for o in collection.objects]}
    try:
        owner = collection.owner_id
    except AttributeError:
        unexposed.add('Collection.owner_id')
    else:
        row['owner'] = reference(owner)
    return row


def object_row(obj):
    return {'name': obj.name, 'library': library_chain(obj), 'type': obj.type,
            'restrictflag': optional(obj, 'restrictflag'),
            'parent': obj.parent.name if obj.parent else None,
            'parent_library': library_chain(obj.parent) if obj.parent else None,
            'partype': optional(obj, 'partype'),
            'hide_viewport': obj.hide_viewport, 'hide_render': obj.hide_render,
            'override': override_json(obj)}


args = sys.argv[sys.argv.index('--') + 1:]
if len(args) != 3 or args[1] != '--out':
    raise SystemExit('usage: ... -- ROOT --out NEW_JSON')
root = Path(args[0]).absolute()
output = Path(args[2]).absolute()
if output.exists():
    raise FileExistsError('refusing to overwrite a native oracle')
with root.open('rb') as stream:
    before = hashlib.file_digest(stream, 'sha256').hexdigest()
bpy.ops.wm.open_mainfile(filepath=str(root), load_ui=False, use_scripts=False)

# Enumerate each actual collection once, including Scene masters outside the
# global collection table. No recursive walk, DAG duplication or depth cap.
collections = {c.as_pointer(): c for c in bpy.data.collections}
scenes = []
for scene in bpy.data.scenes:
    master = scene.collection
    collections.setdefault(master.as_pointer(), master)
    scenes.append({'name': scene.name, 'library': library_chain(scene),
                   'master_collection': master.name, 'master_library': library_chain(master)})
rows = []
edges = []
for collection in collections.values():
    rows.append(collection_row(collection))
    owner = library_chain(collection) or {}
    for kind, targets in (('collection_child', collection.children),
                          ('collection_object', collection.objects)):
        for ordinal, target in enumerate(targets):
            destination = library_chain(target) or {}
            edges.append({'from': collection.name, 'from_filepath': owner.get('filepath'),
                          'kind': kind, 'ordinal': ordinal, 'to': target.name,
                          'to_filepath': destination.get('filepath')})
objects = [object_row(obj) for obj in bpy.data.objects]
with root.open('rb') as stream:
    after = hashlib.file_digest(stream, 'sha256').hexdigest()
if before != after:
    raise AssertionError('native oracle input bytes changed')
report = {'schema': 'oxblend.collection-graph-oracle/1', 'root': str(root),
          'input_sha256': before, 'blender': bpy.app.version_string,
          'build_hash': bpy.app.build_hash.decode(),
          'basis': 'native_after_load_not_original_saved_bytes',
          'coverage': {'opened_root': True, 'rendered': False, 'evaluated': False,
                       'saved': False, 'edited': False, 'input_unchanged': True,
                       'unexposed_rna_attributes': sorted(unexposed),
                       'uncompared_saved_fields': ['Collection.owner_id', 'Collection.flag',
                                                  'Object.type', 'Object.partype', 'Object.restrictflag']},
          'scenes': scenes,
          'collections': sorted(rows, key=lambda r: (r['name'], str(r['library']))),
          'objects': sorted(objects, key=lambda r: (r['name'], str(r['library']))),
          'membership_edges': edges}
with output.open('x') as stream:
    json.dump(report, stream, indent=2, sort_keys=True)
    stream.write('\n')
print(json.dumps({'oracle': str(output), 'collections': len(rows),
                  'objects': len(objects), 'edges': len(edges), 'input_unchanged': True}))
