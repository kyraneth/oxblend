# One-time version-bound generic constructors; no project/shot data.
import bpy, json, sys, hashlib
from pathlib import Path
root=Path(sys.argv[sys.argv.index('--')+1]); root.mkdir(parents=True,exist_ok=True)
assert bpy.app.version[:3] == (4,5,8)
def save_portable(name):
 for scene in bpy.data.scenes:
  scene.render.filepath='//render/'
 for screen in bpy.data.screens:
  for area in screen.areas:
   for space in area.spaces:
    if space.type=='FILE_BROWSER' and space.params:
     space.params.directory=b''
 bpy.ops.wm.save_as_mainfile(filepath=str(root/name),compress=False)
bpy.ops.wm.read_homefile(use_empty=True)
save_portable('empty.blend')
scene=bpy.context.scene
scene.use_nodes=True
tree=scene.node_tree
tree.nodes.clear()
for kind in ('CompositorNodeImage','CompositorNodeViewer','CompositorNodeComposite','CompositorNodeCryptomatteV2','CompositorNodeValue','CompositorNodeNormal','CompositorNodeRGB'):
 n=tree.nodes.new(kind);n.name=kind
im=bpy.data.images.new('ConstructorImage',width=1,height=1)
im.source='FILE';im.filepath=''
imnode=tree.nodes['CompositorNodeImage']; imnode.image=im
text=bpy.data.texts.new('ConstructorText');text.write('constructor\n');text.use_module=True
scene['constructor_int']=1000
scene['constructor_string']='constructor'
scene['constructor_group']={'chunk_size':1000}
save_portable('constructors.blend')
bpy.ops.wm.read_homefile(use_empty=True)
camera=bpy.data.objects.new('ConstructorCamera',bpy.data.cameras.new('ConstructorCameraData'))
focus=bpy.data.objects.new('ConstructorFocus',None)
camera.data.dof.focus_object=focus
bpy.context.scene.collection.objects.link(camera);bpy.context.scene.collection.objects.link(focus)
save_portable('library-source.blend')
bpy.ops.wm.read_homefile(use_empty=True)
with bpy.data.libraries.load(str(root/'library-source.blend'),link=True) as (source,target):
 target.objects=['ConstructorCamera','ConstructorFocus']
for obj in target.objects:bpy.context.scene.collection.objects.link(obj)
bpy.context.scene.camera=target.objects[0]
save_portable('library.blend')
# Relative library filename is the portable serialized reference. Do not embed
# a workstation binary path or generation directory in the manifest.
for library in bpy.data.libraries:
 library.filepath='//'+Path(library.filepath).name
save_portable('library.blend')
manifest={'author':'oxblend','version':list(bpy.app.version),'hash':bpy.app.build_hash.decode(),'purpose':'Generic version-bound initialization kit; no shot inputs, EXRs, project paths, or artistic graph.'}
for p in root.glob('*.blend'):manifest[p.name]={'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
(root/'manifest.json').write_text(json.dumps(manifest,indent=2))
