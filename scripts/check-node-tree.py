#!/usr/bin/env python3
"""Real CLI + independent native compositor/reference/roundtrip proof.

Usage: check-node-tree.py --binary /path/oxblend --blender /path/blender
                         --output NEW_DIRECTORY
Requires native Blender 5.2.1 build 9e2066aef7ef and Pillow for decoded pixels.
Does not run as part of portable cargo tests; no Blender download or fallback.
"""
import argparse
import hashlib
import json
import pathlib
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--blender", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    root = args.output.resolve()
    binary = args.binary.resolve()
    blender = args.blender.resolve()
    repo = pathlib.Path(__file__).resolve().parents[1]
    source = repo / "assets/template-5.2.1.blend"

    def run(label, argv, expected=0):
        with (root / (label + ".stdout")).open("wb") as out, (root / (label + ".stderr")).open("wb") as err:
            result = subprocess.run([str(v) for v in argv], stdout=out, stderr=err)
        (root / (label + ".exit")).write_text(str(result.returncode) + "\n")
        assert result.returncode == expected, (label, result.returncode)
        return root / (label + ".stdout")

    def read_graph(path, label):
        return json.loads(run(label, [binary, "node-tree", path]).read_text())["trees"][0]

    def endpoint(name, socket, socket_type="NodeSocketColor"):
        return {"node": name, "socket": socket, "socket_type": socket_type}

    def transaction(label, input_path, graph, operations, durability="none"):
        spec = {"schema": 1, "tree": graph["block_index"], "expected_graph": graph["graph"], "operations": operations}
        spec_path = root / (label + ".spec.json")
        spec_path.write_text(json.dumps(spec))
        output = root / (label + ".blend")
        report = json.loads(run(label, [binary, "edit", input_path, "--output", output, "--spec", spec_path, "--mode", "node-tree", "--durability", durability]).read_text())
        assert report["complete"] and report["output_written"]
        if durability == "each-file":
            assert report["durability_completed"]
        return output, report

    graph = read_graph(source, "initial")
    forward = [
        {"op": "add_node", "node_type": "NodeReroute", "name": "Added"},
        {"op": "relink", "from": endpoint("Source", "Color"), "to": endpoint("Temporary", "Input"), "replacement_from": endpoint("Other", "Color"), "replacement_to": endpoint("Added", "Input")},
        {"op": "remove_node", "name": "Temporary", "node_type": "NodeReroute"},
        {"op": "add_link", "from": endpoint("Added", "Output"), "to": endpoint("Output", "Socket_0")},
    ]
    forward += [{"op": "add_node", "node_type": kind, "name": name} for kind, name in [("ShaderNodeMath", "New Math"), ("ShaderNodeValue", "New Value"), ("CompositorNodeRGB", "New RGB")]]
    edited, report = transaction("edit", source, graph, forward, "each-file")

    def native(label, mode, input_path):
        report_path = root / (label + ".json")
        png = root / (label + ".png")
        run(label, [blender, "--background", "--factory-startup", "--disable-autoexec", "--python-exit-code", "1", "--python", repo / "scripts/prove-node-tree-native.py", "--", mode, input_path, root / (label + "-resaved.blend"), report_path, png])
        from PIL import Image
        with Image.open(png) as image:
            pixels = (image.size, image.mode, image.tobytes())
        return json.loads(report_path.read_text()), pixels

    native_edited, edited_pixels = native("native-edited", "input", edited)
    reference, reference_pixels = native("native-reference", "reference", source)
    control, original_pixels = native("native-original", "input", source)
    for result in (native_edited, reference):
        assert result["before_resave"] == report["after"] == result["after_resave"]
    assert native_edited["parameters"] == reference["parameters"]
    assert edited_pixels == reference_pixels and edited_pixels != original_pixels
    assert control["after_resave"] == graph["graph"]

    reverse = [
        {"op": "add_node", "node_type": "NodeReroute", "name": "Temporary"},
        {"op": "relink", "from": endpoint("Other", "Color"), "to": endpoint("Added", "Input"), "replacement_from": endpoint("Source", "Color"), "replacement_to": endpoint("Temporary", "Input")},
        {"op": "remove_link", "from": endpoint("Added", "Output"), "to": endpoint("Output", "Socket_0")},
        {"op": "add_link", "from": endpoint("Temporary", "Output"), "to": endpoint("Output", "Socket_0")},
    ]
    reverse += [{"op": "remove_node", "node_type": kind, "name": name} for kind, name in [("NodeReroute", "Added"), ("ShaderNodeMath", "New Math"), ("ShaderNodeValue", "New Value"), ("CompositorNodeRGB", "New RGB")]]
    reverse_output, reverse_report = transaction("reverse", edited, read_graph(edited, "edited-graph"), reverse)
    assert reverse_report["after"] == graph["graph"]
    native_reverse, reverse_pixels = native("native-reverse", "input", reverse_output)
    assert native_reverse["after_resave"] == graph["graph"] and reverse_pixels == original_pixels

    negatives = [
        ("unsupported", {"op": "add_node", "node_type": "GeometryNodeJoinGeometry", "name": "No"}, "node_constructor"),
        ("type", {"op": "add_link", "from": endpoint("Source", "Color", "NodeSocketFloat"), "to": endpoint("Math", "Value_001", "NodeSocketFloat")}, "node_socket_type"),
        ("dangling", {"op": "add_link", "from": endpoint("Missing", "Color"), "to": endpoint("Output", "Socket_0")}, "node_endpoint"),
        ("unverified", {"op": "add_node", "node_type": "NodeReroute", "name": "No"}, "node_version"),
    ]
    for label, operation, kind in negatives:
        input_path = source
        if label == "unverified":
            data = bytearray((root / "native-original-resaved.blend").read_bytes())
            data[13:17] = b"0503"
            input_path = root / "unverified-input.blend"
            input_path.write_bytes(data)
        spec = root / (label + ".spec.json")
        spec.write_text(json.dumps({"schema": 1, "tree": graph["block_index"], "expected_graph": graph["graph"], "operations": [operation]}))
        output = root / ("refused-" + label + ".blend")
        run(label, [binary, "edit", input_path, "--output", output, "--spec", spec, "--mode", "node-tree"], expected=2)
        assert json.loads((root / (label + ".stderr")).read_text())["error"]["kind"] == kind
        assert not output.exists()
    receipt = {"native_graph_exact": True, "native_parameters_exact": True, "reference_pixels_exact": True, "reverse_graph_pixels_exact": True, "original_control_differs": True, "four_typed_negatives_no_publication": True, "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
    (root / "ACCEPTANCE.json").write_text(json.dumps(receipt, indent=2))
    print(json.dumps(receipt))


if __name__ == "__main__":
    main()
