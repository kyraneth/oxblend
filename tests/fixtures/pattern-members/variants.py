import bpy,sys
from pathlib import Path
p=Path(sys.argv[sys.argv.index('--')+1]);bpy.ops.wm.open_mainfile(filepath=str(p/'patterns.blend'),load_ui=False,use_scripts=False)
mat=bpy.data.materials['patterns'];seq=next(n for n in mat.node_tree.nodes if n.type=='TEX_IMAGE' and n.image.source=='SEQUENCE')
seq.image_user.frame_offset=2;seq.image_user.frame_start=5;seq.image_user.use_cyclic=True
bpy.ops.wm.save_as_mainfile(filepath=str(p/'offset.blend'),compress=True,relative_remap=False)
seq.image_user.frame_offset=-1
bpy.ops.wm.save_as_mainfile(filepath=str(p/'unknown-negative.blend'),compress=True,relative_remap=False)
seq.image_user.frame_offset=0;seq.image_user.use_cyclic=False
image=seq.image;image.use_fake_user=True;mat.node_tree.nodes.remove(seq)
bpy.ops.wm.save_as_mainfile(filepath=str(p/'unknown-user.blend'),compress=True,relative_remap=False)
for im in list(bpy.data.images):
 if im.source=='SEQUENCE':bpy.data.images.remove(im)
 elif im.source=='TILED':im.filepath='//tile.<UVTILE>.png'
bpy.ops.wm.save_as_mainfile(filepath=str(p/'uvtile.blend'),compress=True,relative_remap=False)
