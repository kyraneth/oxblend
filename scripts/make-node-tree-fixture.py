"""Create the independently native template/proof input. Run only in Blender 5.2.1.
Native defaults are deliberately preserved; no hand-authored DNA/storage bytes.
"""
import bpy
import json
import pathlib
import sys

assert bpy.app.version == (5, 2, 1)
assert bpy.app.build_hash == b"9e2066aef7ef"
root = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
root.mkdir(parents=True, exist_ok=True)
if (root / "template-5.2.1.blend").exists():
    raise FileExistsError(root / "template-5.2.1.blend")
tree = bpy.data.node_groups.new("EditProof", "CompositorNodeTree")
tree.interface.new_socket(name="Image", in_out="OUTPUT", socket_type="NodeSocketColor")
for kind, name in [("CompositorNodeRGB", "Source"), ("CompositorNodeRGB", "Other"),
                   ("ShaderNodeMath", "Math"), ("ShaderNodeValue", "Value"),
                   ("NodeReroute", "Temporary"), ("NodeGroupOutput", "Output")]:
    node = tree.nodes.new(kind)
    node.name = name
    if name == "Source":
        node.outputs[0].default_value = (0.2, 0.4, 0.6, 1.0)
    if name == "Other":
        node.outputs[0].default_value = (0.8, 0.1, 0.05, 1.0)
tree.links.new(tree.nodes["Source"].outputs[0], tree.nodes["Temporary"].inputs[0])
tree.links.new(tree.nodes["Temporary"].outputs[0], tree.nodes["Output"].inputs[0])
tree.links.new(tree.nodes["Value"].outputs[0], tree.nodes["Math"].inputs[0])
bpy.context.scene.compositing_node_group = tree
bpy.context.scene.render.engine = "CYCLES"
bpy.context.scene.cycles.device = "CPU"
bpy.context.scene.cycles.samples = 1  # Constant compositor output; scene noise is irrelevant.
bpy.context.scene.render.resolution_x = 32
bpy.context.scene.render.resolution_y = 32
bpy.context.scene.render.resolution_percentage = 100
bpy.context.scene.render.image_settings.file_format = "PNG"
bpy.ops.wm.save_as_mainfile(filepath=str(root / "template-5.2.1.blend"), compress=True, check_existing=False)
print("TEMPLATE", root / "template-5.2.1.blend")
