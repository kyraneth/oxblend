import bpy,sys,json
from pathlib import Path
p=Path(sys.argv[sys.argv.index('--')+1]);bpy.ops.wm.open_mainfile(filepath=str(p/'patterns.blend'),load_ui=False,use_scripts=False)
mat=bpy.data.materials['patterns'];nodes=mat.node_tree.nodes
seq=next(n for n in nodes if n.type=='TEX_IMAGE' and n.image.source=='SEQUENCE')
b=nodes.get('Principled BSDF');mat.node_tree.links.new(seq.outputs['Color'],b.inputs['Base Color'])
bpy.ops.mesh.primitive_plane_add();obj=bpy.context.object;obj.data.materials.append(mat)
bpy.ops.object.camera_add(location=(0,0,3));bpy.context.scene.camera=bpy.context.object
s=bpy.context.scene;s.render.engine='CYCLES';s.cycles.device='CPU';s.render.resolution_x=8;s.render.resolution_y=8;s.render.resolution_percentage=100
rows=[]
for offset,cyclic,start in [(0,False,1),(2,True,5),(-1,False,1)]:
 seq.image_user.frame_offset=offset;seq.image_user.use_cyclic=cyclic;seq.image_user.frame_start=start
 for f in range(start,start+4):
  s.frame_set(f);bpy.ops.render.render()
  # evaluated copy carries runtime ImageUser frame selection after render.
  em=mat.evaluated_get(bpy.context.evaluated_depsgraph_get())
  en=em.node_tree.nodes.get(seq.name)
  rows.append(dict(offset=offset,cyclic=cyclic,start=start,frame=f,current=en.image_user.frame_current,path=en.image.filepath_from_user(image_user=en.image_user)))
(p/'native-evaluated.json').write_text(json.dumps(rows,indent=2))
