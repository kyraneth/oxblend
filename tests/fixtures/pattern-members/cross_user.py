import bpy,sys
from pathlib import Path
p=Path(sys.argv[sys.argv.index('--')+1]);bpy.ops.wm.read_factory_settings(use_empty=True)
with bpy.data.libraries.load(str(p/'patterns.blend'),link=True) as (a,b):b.images=['sequence']
mat=bpy.data.materials.new('root-user');mat.use_fake_user=True;mat.use_nodes=True
n=mat.node_tree.nodes.new('ShaderNodeTexImage');n.image=b.images[0];n.image_user.frame_duration=5
bpy.context.scene.frame_start=1;bpy.context.scene.frame_end=5
bpy.ops.wm.save_as_mainfile(filepath=str(p/'cross-user.blend'),relative_remap=True,compress=True)
