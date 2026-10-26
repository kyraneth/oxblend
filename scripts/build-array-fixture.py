"""Create a NEW tiny pointed-array fixture, or inspect it read-only after reopen.

Run Blender 5.2.1 factory/background/disabled-autoexec, then this script with
-- /new/output/directory [--verify]. Never writes an input or existing fixture.
The oracle uses RNA only: no SDNA parsing, raw memory or pointer arithmetic.
"""
import hashlib
import json
from pathlib import Path
import sys

import bpy

args = sys.argv[sys.argv.index('--') + 1:]
out = Path(args[0]).absolute()
verify = args[1:] == ['--verify']
if args[1:] and not verify:
    raise ValueError('expected OUTPUT_DIRECTORY [--verify]')
if bpy.app.version != (5, 2, 1):
    raise ValueError('native fixture qualification requires Blender 5.2.1')
fixture = out / 'arrays.blend'


def name(value):
    return None if value is None else value.name


def observe():
    obj = bpy.data.objects['ArrayObject']
    return {
        'native_version': bpy.app.version_string,
        'native_build': bpy.app.build_hash.decode('ascii'),
        'object': obj.name,
        'object_type': obj.type,
        'mesh': obj.data.name,
        'mesh_materials': [name(m) for m in obj.data.materials],
        'slots': [{'link': s.link, 'material': name(s.material)}
                  for s in obj.material_slots],
    }


if verify:
    if Path(bpy.data.filepath).absolute() != fixture:
        raise ValueError('verify must open exactly the new arrays.blend fixture')
    actual = observe()
    built = json.loads((out / 'built.json').read_text())
    expected = built['rna']
    if actual != expected:
        raise AssertionError({'built': expected, 'reopened': actual})
    with fixture.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    if digest != built['sha256']:
        raise AssertionError('fixture bytes changed after build')
    print(json.dumps({'schema': 'oxblend.array-native-proof/1', 'reopened': actual,
                      'sha256': digest, 'input_unchanged': True}))
else:
    if fixture.exists() or (out / 'built.json').exists():
        raise FileExistsError('refusing to overwrite an existing fixture or oracle')
    out.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    mesh = bpy.data.meshes.new('ArrayMesh')
    mesh.from_pydata([(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)], [],
                     [(0, 1, 2, 3)])
    data_a = bpy.data.materials.new('DataMaterialA')
    data_c = bpy.data.materials.new('DataMaterialC')
    override = bpy.data.materials.new('ObjectMaterial')
    for material in (data_a, None, data_c):
        mesh.materials.append(material)
    obj = bpy.data.objects.new('ArrayObject', mesh)
    bpy.context.scene.collection.objects.link(obj)
    obj.material_slots[0].link = 'OBJECT'
    obj.material_slots[0].material = override
    obj.material_slots[1].link = 'DATA'
    obj.material_slots[2].link = 'OBJECT'
    obj.material_slots[2].material = None
    rna = observe()
    bpy.ops.wm.save_as_mainfile(filepath=str(fixture), check_existing=False)
    with fixture.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    manifest = {'schema': 'oxblend.array-native-proof/1', 'input': str(fixture),
                'sha256': digest, 'rna': rna}
    (out / 'built.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(manifest))
