"""Native receiving proof. Invoked in a separate factory/background Blender.
Args after --: MODE(input/reference) INPUT NEW_RESAVE OUTPUT_JSON OUTPUT_PNG.
"""
import bpy
import json
import pathlib
import sys

assert bpy.app.version == (5, 2, 1) and bpy.app.build_hash == b"9e2066aef7ef"
mode, source, resave, report, png = sys.argv[sys.argv.index("--") + 1:]
assert mode in ("input", "reference")
for destination in (resave, report, png):
    if pathlib.Path(destination).exists():
        raise FileExistsError(destination)
bpy.ops.wm.open_mainfile(filepath=source, load_ui=False)
tree = bpy.data.node_groups["EditProof"]
if mode == "reference":
    added = tree.nodes.new("NodeReroute")
    added.name = "Added"
    old = next(link for link in tree.links if link.from_node.name == "Source" and link.to_node.name == "Temporary")
    tree.links.remove(old)
    tree.links.new(tree.nodes["Other"].outputs["Color"], added.inputs["Input"])
    tree.nodes.remove(tree.nodes["Temporary"])
    tree.links.new(added.outputs["Output"], tree.nodes["Output"].inputs["Image"])
    for kind, name in [("ShaderNodeMath", "New Math"), ("ShaderNodeValue", "New Value"), ("CompositorNodeRGB", "New RGB")]:
        node = tree.nodes.new(kind)
        node.name = name
        if kind == "CompositorNodeRGB":
            # Constructor is a clone of the pinned native Source prototype,
            # including its explicit color, rather than an implicit zero/default.
            node.outputs[0].default_value = tree.nodes["Source"].outputs[0].default_value

def endpoint(n, s):
    return {"node": n.name, "socket": s.identifier, "socket_type": s.bl_idname}

def dump(tree):
    def sockets(items):
        return [{"identifier": s.identifier, "socket_type": s.bl_idname} for s in items]
    nodes = [{"name": n.name, "node_type": n.bl_idname, "inputs": sockets(n.inputs), "outputs": sockets(n.outputs)} for n in tree.nodes]
    links = [{"from": endpoint(l.from_node, l.from_socket), "to": endpoint(l.to_node, l.to_socket)} for l in tree.links]
    # Match serde_json's map-key-sorted compact representation used by the editor.
    links.sort(key=lambda v: json.dumps(v, sort_keys=True, separators=(",", ":")))
    nodes.sort(key=lambda v: v["name"])
    return {"name": "NT" + tree.name, "node_type": tree.bl_idname, "nodes": nodes, "links": links}

def parameters(tree):
    def value(socket):
        if not hasattr(socket, "default_value"):
            return None
        v = socket.default_value
        return list(v) if hasattr(v, "__iter__") else v
    return {n.name: {"operation": getattr(n, "operation", None), "inputs": [value(s) for s in n.inputs], "outputs": [value(s) for s in n.outputs]} for n in tree.nodes}

before = dump(tree)
before_parameters = parameters(tree)
# Only dedicated new proof paths are ever saved; user/source files stay unchanged.
bpy.ops.wm.save_as_mainfile(filepath=resave, compress=False, check_existing=False)
bpy.ops.wm.open_mainfile(filepath=resave, load_ui=False)
after = dump(bpy.data.node_groups["EditProof"])
assert before == after, (before, after)
after_parameters = parameters(bpy.data.node_groups["EditProof"])
assert before_parameters == after_parameters
pathlib.Path(report).write_text(json.dumps({"build": bpy.app.build_hash.decode(), "before_resave": before, "after_resave": after, "parameters": after_parameters}, indent=2))
bpy.context.scene.render.filepath = png
bpy.ops.render.render(write_still=True)
print("NATIVE_NODE_TREE_PROOF", report, png)
