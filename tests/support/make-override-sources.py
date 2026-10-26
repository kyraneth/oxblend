# Zamil: challenge declaration-only source selection against native nested loads.
import bpy,json,sys
from pathlib import Path
E=Path(sys.argv[sys.argv.index('--')+1]);E.mkdir()
def reset():bpy.ops.wm.read_factory_settings(use_empty=True)
def save(n):bpy.ops.wm.save_as_mainfile(filepath=str(E/n),compress=False)
reset();c=bpy.data.collections.new('Nested');bpy.context.scene.collection.children.link(c)
c.objects.link(bpy.data.objects.new('NestedObject',None));save('nested.blend')
reset()
with bpy.data.libraries.load(str(E/'nested.blend'),link=True) as (a,b):b.collections=['Nested']
c=bpy.data.collections.new('Source');bpy.context.scene.collection.children.link(c)
o=bpy.data.objects.new('SourceObject',None);c.objects.link(o);o.instance_type='COLLECTION';o.instance_collection=b.collections[0]
save('source.blend')
reset()
with bpy.data.libraries.load(str(E/'source.blend'),link=True) as (a,b):b.collections=['Source']
linked=b.collections[0];bpy.context.scene.collection.children.link(linked)
override=linked.override_hierarchy_create(bpy.context.scene,bpy.context.view_layer,do_fully_editable=True)
bpy.context.scene.collection.children.unlink(linked);override.name='Overridden'
for o in override.objects:o.instance_collection=None;o.instance_type='NONE'
save('middle.blend')
reset()
with bpy.data.libraries.load(str(E/'middle.blend'),link=True) as (a,b):b.collections=['Overridden']
o=bpy.data.objects.new('Instance',None);bpy.context.scene.collection.objects.link(o);o.instance_type='COLLECTION';o.instance_collection=b.collections[0]
save('root.blend')
(E/'created.json').write_text(json.dumps(dict(version=bpy.app.version_string,libraries=[dict(name=l.name,filepath=l.filepath,absolute=bpy.path.abspath(l.filepath)) for l in bpy.data.libraries]),indent=2))
