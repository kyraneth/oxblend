# Dedicated native 4.5.8 fixtures; no production file opened.
import bpy,sys,json,hashlib
from pathlib import Path
D=Path(sys.argv[sys.argv.index('--')+1]);D.mkdir(parents=True,exist_ok=False)
assert bpy.app.version[:3]==(4,5,8)
def save(name):
 for scene in bpy.data.scenes:scene.render.filepath='//render/'
 for screen in bpy.data.screens:
  for area in screen.areas:
   for space in area.spaces:
    if space.type=='FILE_BROWSER' and space.params:space.params.directory=b''
 bpy.ops.wm.save_as_mainfile(filepath=str(D/name),compress=False,check_existing=False)
bpy.ops.wm.read_factory_settings(use_empty=True)
save('empty-native-seed.blend')
s=bpy.context.scene;s.use_nodes=True;s.node_tree.nodes.clear()
c=s.node_tree.nodes.new('CompositorNodeCryptomatteV2');c.name='NativeCryptomatte';c.source='RENDER';c.scene=s
save('crypto-scene.blend')
c.source='IMAGE';c.scene=None;c.image=bpy.data.images.new('NativeCryptos',width=1,height=1)
save('crypto-image.blend')
manifest={'author':'oxblend','bpy_version':list(bpy.app.version),'build_hash':bpy.app.build_hash.decode(),'scope':'native constructors only; not evaluated/render acceptance','files':{p.name:{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in D.glob('*.blend')}}
(D/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');print(json.dumps(manifest),flush=True)
