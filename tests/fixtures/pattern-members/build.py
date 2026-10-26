import bpy,sys,json,shutil
from pathlib import Path
D=Path(sys.argv[sys.argv.index('--')+1]); D.mkdir(parents=True,exist_ok=False)
bpy.ops.wm.read_factory_settings(use_empty=True)
i=bpy.data.images.new('pixels',width=2,height=2);i.filepath_raw=str(D/'base.png');i.file_format='PNG';i.save();bpy.data.images.remove(i)
for n in [1001,1002]:shutil.copyfile(D/'base.png',D/f'tile.{n}.png')
for n in [1,2,3]:shutil.copyfile(D/'base.png',D/f'shot.{n:04}.png')
mat=bpy.data.materials.new('patterns');mat.use_nodes=True;mat.use_fake_user=True
for name,source,path in [('tiles','TILED','tile.1001.png'),('sequence','SEQUENCE','shot.0001.png')]:
 i=bpy.data.images.load(str(D/path));i.name=name;i.source=source
 i.filepath='//'+('tile.<UDIM>.png' if source=='TILED' else path)
 if source=='TILED':i.tiles.new(1002)
 n=mat.node_tree.nodes.new('ShaderNodeTexImage');n.image=i
 if source=='SEQUENCE':n.image_user.frame_duration=3;n.image_user.use_auto_refresh=True
bpy.context.scene.frame_start=1;bpy.context.scene.frame_end=3
bpy.ops.wm.save_as_mainfile(filepath=str(D/'patterns.blend'),check_existing=False,relative_remap=False)
rows=[]
for i in bpy.data.images:
 if i.source=='TILED': rows.append({'name':i.name,'tiles':[t.number for t in i.tiles]})
for n in mat.node_tree.nodes:
 if n.type=='TEX_IMAGE' and n.image.source=='SEQUENCE':
  rows.append({'name':n.image.name,'duration':n.image_user.frame_duration,'offset':n.image_user.frame_offset,'start':n.image_user.frame_start,'cyclic':n.image_user.use_cyclic})
(D/'native.json').write_text(json.dumps({'blender':bpy.app.version_string,'rows':rows},indent=2))
