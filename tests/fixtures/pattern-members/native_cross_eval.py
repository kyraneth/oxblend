import bpy,sys,json
from pathlib import Path
p=Path(sys.argv[sys.argv.index('--')+1]);bpy.ops.wm.open_mainfile(filepath=str(p/'cross-user.blend'),load_ui=False,use_scripts=False)
mat=bpy.data.materials['root-user'];seq=next(n for n in mat.node_tree.nodes if n.type=='TEX_IMAGE')
mat.node_tree.links.new(seq.outputs['Color'],mat.node_tree.nodes.get('Principled BSDF').inputs['Base Color'])
bpy.ops.mesh.primitive_plane_add();bpy.context.object.data.materials.append(mat)
bpy.ops.object.camera_add(location=(0,0,3));bpy.context.scene.camera=bpy.context.object
s=bpy.context.scene;s.render.engine='CYCLES';s.cycles.device='CPU';s.render.resolution_x=8;s.render.resolution_y=8;s.render.resolution_percentage=100
rows=[]
for f in range(1,6):
 s.frame_set(f);bpy.ops.render.render();em=mat.evaluated_get(bpy.context.evaluated_depsgraph_get());en=em.node_tree.nodes.get(seq.name)
 rows.append(dict(frame=f,duration=en.image_user.frame_duration,current=en.image_user.frame_current,path=en.image.filepath_from_user(image_user=en.image_user)))
(p/'native-cross-evaluated.json').write_text(json.dumps({'blender':bpy.app.version_string,'rows':rows},indent=2))
