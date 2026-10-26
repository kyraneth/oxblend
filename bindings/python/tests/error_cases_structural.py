"""Owned real structural-error witnesses, from the clean-room native 5.2.1 save.

Only NEW faulted test copies are authored here. No original is repaired, resaved,
version-spoofed or overwritten. Source SDNA offsets come from Oxblend inspect;
all mutations are explicit fixture faults, never an alternative production writer.
The 209110-byte native base and its generator/provenance stay read-only.

Each CASE_BUILDERS member implements error_inventory's six-tuple protocol.
Rewrite/override diagnostics are witnessed under their actual public terminal
kind, with their nested report preserved, not promoted to fabricated exceptions.
"""
from __future__ import annotations

import copy
import json
import struct
from functools import partial
from pathlib import Path

from error_inventory import _dna, _write, fixture_minimal_bytes, json_bytes, sha256_bytes

BASE = Path(__file__).with_name("fixtures") / "native521" / "qualifier.blend"
BASE_SHA = "6e5deefb1b96c8726a7f205f612711f5cfb0f578e1babfebef8893293359dd09"


class NativeFault:
    """Source-DNA fixture author, NOT a scene repair API."""

    def __init__(self, ox):
        self.original = BASE.read_bytes()
        assert len(self.original) == 209110
        assert sha256_bytes(self.original) == BASE_SHA
        assert self.original[:17] == b"BLENDER17-01v0502"
        self.info = ox.inspect(BASE).to_json()
        self.types = [bytes.fromhex(t).decode() for t in self.info["sdna"]["types_hex"]]
        self.structs = self.info["sdna"]["structs"]
        self.by_type = {self.types[s["type_index"]]: i for i, s in enumerate(self.structs)}
        self.rows = []
        for b in self.info["blocks"]:
            row = dict(b)
            row["data"] = bytearray(self.original[b["data_offset"]:b["data_offset"] + b["size"]])
            self.rows.append(row)
        self.faults = []

    def ty(self, b):
        return self.types[self.structs[self.rows[b]["sdna_index"]]["type_index"]]

    def in_tree(self, block):
        while block >= 0 and self.rows[block]["code_hex"] == b"DATA".hex():
            block -= 1
        return block >= 0 and self.rows[block]["code_hex"] == b"NT\0\0".hex()

    def find(self, ty, **strings):
        hits = [i for i in range(len(self.rows)) if self.rows[i]["code_hex"] not in (b"DNA1".hex(), b"ENDB".hex()) and self.ty(i) == ty
                and (ty != "bNodeTree" or self.rows[i]["code_hex"] == b"NT\0\0".hex())
                and (ty not in ("bNode", "bNodeLink", "bNodeSocket") or self.in_tree(i))
                and all(self.string(i, [k]) == v for k, v in strings.items())]
        assert len(hits) == 1, (ty, strings, hits)
        return hits[0]

    def loc(self, b, path):
        si, offset = self.rows[b]["sdna_index"], 0
        for index, name in enumerate(path):
            f = next(f for f in self.structs[si]["fields"] if bytes.fromhex(f["name_hex"]).decode() == name)
            offset += f["offset"]
            if index + 1 < len(path):
                si = self.by_type[self.types[f["type_index"]]]
        return offset, f

    def raw(self, b, path):
        off, f = self.loc(b, path)
        return bytes(self.rows[b]["data"][off:off + f["size"]])

    def string(self, b, path):
        return self.raw(b, path).split(b"\0", 1)[0].decode()

    def number(self, b, path, *, signed=False):
        return int.from_bytes(self.raw(b, path), "little", signed=signed)

    def setraw(self, b, path, data):
        off, f = self.loc(b, path)
        assert len(data) == f["size"], (path, len(data), f["size"])
        self.rows[b]["data"][off:off + len(data)] = data
        self.faults.append({"block": b, "path": path, "replacement_hex": data.hex()})

    def set(self, b, path, value):
        _, f = self.loc(b, path)
        self.setraw(b, path, int(value).to_bytes(f["size"], "little", signed=value < 0))

    def text(self, b, path, text):
        _, f = self.loc(b, path)
        value = text.encode()
        assert len(value) < f["size"]
        self.setraw(b, path, value + bytes(f["size"] - len(value)))

    def addr(self, b):
        return int(self.rows[b]["address_decimal"])

    def add(self, ty, code=b"DATA"):
        si = self.by_type[ty]
        size = self.info["sdna"]["lengths"][self.structs[si]["type_index"]]
        # New synthetic records use addresses strictly outside every native extent.
        addr = max(int(r["address_decimal"]) + len(r["data"]) for r in self.rows) + 8
        index = next(i for i, r in enumerate(self.rows) if r["code_hex"] == b"DNA1".hex())
        self.rows.insert(index, {"code_hex": code.hex(), "sdna_index": si,
                                 "address_decimal": str(addr), "count": 1,
                                 "data": bytearray(size)})
        return index

    def dna_fault(self, ty=None, field=None, new_name=None, new_type=None):
        names = [bytes.fromhex(n) for n in self.info["sdna"]["names_hex"]]
        types = [bytes.fromhex(t) for t in self.info["sdna"]["types_hex"]]
        structs = []
        for s in self.structs:
            entries = []
            for f in s["fields"]:
                name = bytes.fromhex(f["full_name_hex"])
                ti = f["type_index"]
                if self.types[s["type_index"]] == ty and bytes.fromhex(f["name_hex"]).decode() == field:
                    name = new_name.encode() if new_name is not None else name
                    ti = self.types.index(new_type) if new_type is not None else ti
                if name not in names: names.append(name)
                entries.append((ti, names.index(name)))
            structs.append((s["type_index"], entries))
        # An unused NAME changes DNA identity without altering a single layout.
        if ty is None: names.append(b"l20_negative_unused_name")
        dna = _dna(names, types, self.info["sdna"]["lengths"], structs)
        row = next(r for r in self.rows if r["code_hex"] == b"DNA1".hex())
        row["data"] = bytearray(dna)
        self.faults.append({"source_dna_fault": {"type": ty, "field": field, "new_name": new_name, "new_type": new_type}})

    def save(self, directory):
        data = bytearray(self.original[:17])
        # Keep the original ordered owner runs; new records are inserted before DNA.
        rows = [r for r in self.rows if bytes.fromhex(r["code_hex"]) not in (b"DNA1", b"ENDB")]
        rows += [r for r in self.rows if bytes.fromhex(r["code_hex"]) == b"DNA1"]
        for r in rows:
            data += struct.pack("<4sIQQQ", bytes.fromhex(r["code_hex"]), r["sdna_index"],
                                int(r["address_decimal"]), len(r["data"]), r["count"])
            data += r["data"]
        data += struct.pack("<4sIQQQ", b"ENDB", 0, 0, 0, 0)
        path = _write(directory / "fault.blend", bytes(data))
        _write(directory / "fixture-provenance.json", json_bytes({
            "base": str(BASE), "base_sha256": BASE_SHA,
            "base_provenance": str(BASE.with_name("PROVENANCE.md")),
            "purpose": "negative fixture authoring only; NOT a repair or qualified output",
            "faults": self.faults,
            "authored_blocks": [{"block": i, "code_hex": r["code_hex"], "sdna_index": r["sdna_index"],
                                 "address_decimal": r["address_decimal"], "count": r["count"],
                                 "payload_bytes": len(r["data"]), "payload_sha256": sha256_bytes(bytes(r["data"]))}
                                for i, r in enumerate(self.rows)],
            "fixture_sha256": sha256_bytes(bytes(data))}))
        assert BASE.read_bytes() == self.original
        return path


def _directory(work, name):
    d = work / "cases" / ("structural-" + name)
    d.mkdir(parents=True, exist_ok=True)
    return d


def _edit_case(ox, d, source, kind, name, spec, mode="format-only"):
    specpath = _write(d / "spec.json", json_bytes(spec))
    output = d / "new-output.blend"
    return (kind, "structural-" + name,
            ["edit", str(source), "--output", str(output), "--mode", mode, "--spec", str(specpath)],
            lambda: ox.edit(source, output, spec, mode=mode), [source, specpath], [output])


def _write_spec(f, block, path, replacement=None, **overrides):
    _, field = f.loc(block, path)
    ty = f.types[field["type_index"]]
    if ty in ("char", "uchar"):
        value = {"kind": "cstring", "hex": f.raw(block, path).split(b"\0", 1)[0].hex()}
    elif ty == "float":
        value = {"kind": "f32", "bits_hex": f"{f.number(block, path):08x}"}
    else:
        value = {"kind": "signed", "value": f.number(block, path, signed=True)}
    row = {"block_index": block, "element": 0, "path": path,
           "sdna_type": ty, "width": field["size"], "expected": value,
           "replacement": replacement or copy.deepcopy(value)}
    row.update(overrides)
    return {"schema": 2, "writes": [row], "bits": []}


def typed_case(ox, work, variant):
    d = _directory(work, "typed-" + variant)
    f = NativeFault(ox)
    scene = f.find("Scene")
    block, path = scene, ["r", "xsch"]
    mode = "format-only"
    kind = variant
    if variant in ("string_nul", "string_termination"):
        path = ["r", "pic"]
        if variant == "string_termination":
            _, field = f.loc(scene, path)
            f.setraw(scene, path, b"x" * field["size"])
    elif variant == "padding_write":
        candidates = [bytes.fromhex(x["name_hex"]).decode() for x in f.structs[f.by_type["Scene"]]["fields"]]
        path = [next(x for x in candidates if x.startswith("_pad") or x == "pad" or x.startswith("pad") and x[3:].isdigit())]
    elif variant == "pointer_write":
        path = ["compositing_node_group"]
    elif variant == "field_owner":
        block, path = f.find("FileGlobal"), ["subversion"]
    elif variant == "unsupported_edit":
        path = ["id", "name"]
    elif variant in ("profile_field", "profile_linked", "profile_temporal", "profile_mask", "profile_owner", "profile_library", "profile_discriminator"):
        mode = "comp"
        if variant == "profile_field": path = ["r", "xsch"]
        if variant == "profile_linked": f.set(scene, ["id", "lib"], 0xF00D)
        if variant == "profile_temporal": path = ["r", "sfra"]
        if variant == "profile_owner": block, path = f.add("UserDef", b"USER"), ["flag"]
        if variant == "profile_library":
            block = f.add("Library", b"LI\0\0")
            f.text(block, ["id", "name"], "LIL20Negative")
            path = ["name"]
        if variant == "profile_discriminator":
            block = f.add("Image", b"IM\0\0")
            f.text(block, ["id", "name"], "IML20Negative")
            f.set(block, ["source"], 1)
            f.text(block, ["name"], "//negative.png")
            path = ["name"]
    source = f.save(d)
    spec = _write_spec(f, block, path)
    row = spec["writes"][0]
    if variant == "edit_spec": spec["unknown"] = True
    elif variant == "expected_old": row["expected"]["value"] += 1
    elif variant == "typed_kind": row["replacement"] = {"kind": "unsigned", "value": 320}
    elif variant == "typed_overflow": row["replacement"] = {"kind": "signed", "value": 2**40}
    elif variant == "typed_width": row["width"] += 1
    elif variant == "string_nul": row["replacement"] = {"kind": "cstring", "hex": "610062"}
    elif variant == "string_termination":
        row["expected"] = row["replacement"] = {"kind": "cstring", "hex": "78"}
    elif variant == "profile_field": row["replacement"] = {"kind": "signed", "value": 1}
    elif variant == "profile_temporal": row["replacement"] = {"kind": "signed", "value": 3}
    elif variant == "profile_library": row["replacement"] = {"kind": "cstring", "hex": "2f2f6c69622e626c656e64"}
    elif variant == "profile_discriminator": row["replacement"] = {"kind": "cstring", "hex": "2f2f6f746865722e706e67"}
    elif variant == "edit_conflict": spec["writes"].append(copy.deepcopy(row))
    elif variant in ("flag_mask", "profile_mask"):
        bit = {k: v for k, v in row.items() if k != "replacement"}
        bit.update(clear=1, set=1 if variant == "flag_mask" else 0)
        spec.update(writes=[], bits=[bit])
    return _edit_case(ox, d, source, kind, "typed-" + variant, spec, mode)


def graph_case(ox, work, variant):
    d = _directory(work, "graph-" + variant)
    f = NativeFault(ox)
    tree, math = f.find("bNodeTree"), f.find("bNode", name="L20Math")
    if variant == "identifier_overflow": f.set(math, ["identifier"], 2**31 - 1)
    if variant == "link_profile":
        sock = next(i for i in range(len(f.rows)) if f.in_tree(i) and f.ty(i) == "bNodeSocket" and f.string(i, ["identifier"]) == "Value_001")
        f.set(sock, ["flag"], f.number(sock, ["flag"]) | 2048)
    source = f.save(d)
    graph = ox.node_tree(source).to_json()["trees"][0]["graph"]
    spec = {"schema": 1, "tree": tree, "expected_graph": graph, "operations": []}
    start = {"node": "L20Value", "socket": "Value", "socket_type": "NodeSocketFloat"}
    end = {"node": "L20Math", "socket": "Value", "socket_type": "NodeSocketFloat"}
    operation = {"op": "add_link", "from": start, "to": end}
    if variant == "node_expected_graph": spec["expected_graph"] = {}
    elif variant == "node_graph": spec["tree"] = f.find("Scene")
    elif variant in ("node_endpoint", "node_socket_type", "node_occupied"):
        if variant == "node_endpoint": start["node"] = "NoSuchNode"
        if variant == "node_socket_type": start["socket_type"] = "NodeSocketColor"
        spec["operations"] = [operation]
    elif variant == "node_cycle":
        # A self-cycle uses Math's unoccupied second input, not the occupied first.
        operation["from"] = dict(start, node="L20Math")
        operation["to"] = dict(end, socket="Value_001")
        spec["operations"] = [operation]
    elif variant == "link_profile":
        operation["to"] = dict(end, socket="Value_001")
        spec["operations"] = [operation]
    elif variant in ("node_constructor", "name_collision", "cstring_width", "identifier_overflow"):
        op = {"op": "add_node", "node_type": "ShaderNodeMath", "name": "L20New"}
        if variant == "node_constructor": op["node_type"] = "UnsupportedNode"
        if variant == "name_collision": op["name"] = "L20Math"
        if variant == "cstring_width": op["name"] = "x" * f.loc(math, ["name"])[1]["size"]
        spec["operations"] = [op]
    elif variant == "edit_spec": spec["operations"] = [{"op": "not_an_operation"}]
    return _edit_case(ox, d, source, variant, "graph-" + variant, spec, "node-tree")


def fault_case(ox, work, variant, command):
    d = _directory(work, variant + "-" + command)
    f = NativeFault(ox)
    tree, math = f.find("bNodeTree"), f.find("bNode", name="L20Math")
    value = f.find("bNode", name="L20Value")
    link = f.find("bNodeLink")
    target = None
    path = ["nodes", "first"]
    block = tree
    if command == "pointer" and variant in ("list_boundary", "list_backlink", "list_cycle", "list_header"):
        block = f.add("ChannelDriver")
        element = f.add("DriverVar")
        f.set(block, ["variables", "first"], f.addr(element))
        f.set(block, ["variables", "last"], f.addr(element))
        path = ["variables", "first"]
        if variant == "list_boundary": f.set(block, ["variables", "last"], 0)
        elif variant == "list_backlink": f.set(element, ["prev"], f.addr(element))
        elif variant == "list_cycle": f.set(element, ["next"], f.addr(element))
        else: f.dna_fault("DriverVar", "next", new_name="*l20_wrong_next")
    elif variant == "list_symmetry": f.set(value, ["prev"], f.addr(math))
    elif variant == "list_endpoint": f.set(tree, ["nodes", "last"], f.addr(value))
    elif variant == "list_cycle": f.set(math, ["next"], f.addr(value))
    elif variant == "list_boundary": f.set(tree, ["nodes", "last"], 0)
    elif variant == "list_backlink": f.set(value, ["prev"], f.addr(math))
    elif variant == "node_endpoint": f.set(link, ["tonode"], 0)
    elif variant == "node_cycle":
        f.set(link, ["fromnode"], f.addr(math))
        output = f.number(math, ["outputs", "first"])
        f.set(link, ["fromsock"], output)
    elif variant == "node_socket_type":
        output = f.number(value, ["outputs", "first"])
        socket = next(i for i in range(len(f.rows)) if f.addr(i) == output)
        f.text(socket, ["idname"], "NodeSocketColor")
    elif variant == "node_graph": f.text(value, ["name"], "L20Math")
    elif variant == "identifier_overflow": f.set(math, ["identifier"], 2**31 - 1)
    elif variant == "owner_header": f.rows[tree]["code_hex"] = b"IM\0\0".hex()
    elif variant == "stray_data_run": f.rows[1]["code_hex"] = b"DATA".hex()  # DATA after GLOB, without changing GLOB/build admission.
    elif variant == "record_extent": f.rows[tree if command == "extract" else math]["count"] = 2
    elif variant == "duplicate_data_address": f.rows[math]["address_decimal"] = f.rows[value]["address_decimal"]
    elif variant in ("pointer_count", "pointer_extent", "pointer_target_type", "pointer_role", "unprofiled_role"):
        block = next(i for i in range(len(f.rows)) if f.in_tree(i) and f.ty(i) == "bNodeSocket" and f.number(i, ["default_value"]) != 0)
        path = ["default_value"]
        address = f.number(block, path)
        target = next(i for i in range(len(f.rows)) if f.addr(i) == address)
        if variant == "pointer_count": f.rows[target]["count"] = 2; f.rows[target]["data"] *= 2
        if variant == "pointer_extent": f.rows[target]["data"] += b"\0"
        if variant == "pointer_target_type": f.set(block, path, f.addr(value))
        if variant == "pointer_role":
            _, field = f.loc(block, ["idname"])
            f.setraw(block, ["idname"], b"x" * field["size"])
        if variant == "unprofiled_role":
            f.text(block, ["idname"], "NodeSocketFloatPercentage")
            f.set(block, ["type"], 6)  # Float-percentage requires numeric float=0, not int=6.
    elif variant == "incomplete_overrides": f.set(f.find("Scene"), ["id", "override_library"], 0xDEADBEEF)
    elif variant == "list_header": f.dna_fault("bNode", "next", new_name="*l20_wrong_next")
    elif variant == "field_type": f.dna_fault("bNode", "outputs", new_name="outputs[4]", new_type="int")
    elif variant == "storage_profile":
        f.set(math, ["panel_states_array"], 0xDEADBEEF)
        f.dna_fault("bNode", "num_panel_states", new_name="l20_wrong_count")
    elif variant == "kit_dna": f.dna_fault()
    else:
        raise AssertionError(variant)
    source = f.save(d)
    name = "structural-" + variant + "-" + command
    if command == "pointer":
        return (variant, name, ["pointer", str(source), str(block), json.dumps(path)],
                lambda: ox.pointer(source, block, path), [source], [])
    if command == "node-tree":
        return (variant, name, ["node-tree", str(source)], lambda: ox.node_tree(source), [source], [])
    if command == "pointer-census":
        return (variant, name, [command, str(source)], lambda: ox.pointer_census(source), [source], [])
    if command == "overrides":
        return (variant, name, [command, str(source)], lambda: ox.overrides(source), [source], [])
    if command == "edit":
        return _edit_case(ox, d, source, variant, variant + "-edit", {"schema": 2, "writes": [], "bits": []})
    if command == "node-edit":
        return _edit_case(ox, d, source, variant, variant + "-node-edit", {"schema": 1, "tree": tree, "expected_graph": {}, "operations": []}, "node-tree")
    if command == "extract":
        output = d / "new-output.blend"
        # Extract checks selected IDs before pointer_layout. An invalid root
        # extent is a retained selection refusal, not terminal record_extent.
        kind = "extract_refusal" if variant == "record_extent" else variant
        return (kind, name, ["extract", str(source), "--block", str(tree), "--output", str(output)],
                lambda: ox.extract(source, output, blocks=[tree]), [source], [output])
    raise AssertionError(command)


def extract_case(ox, work, variant):
    d = _directory(work, "extract-" + variant)
    f = NativeFault(ox)
    source = f.save(d)
    output = d / "new-output.blend"
    kwargs = {"ids": ["NTNoSuchID"]} if variant == "selection" else {"ids": ["NTL20Tree"], "clear": ["NotAType.not_a_pointer"]}
    argv = ["extract", str(source), "--output", str(output)]
    for key, values in kwargs.items():
        for val in values: argv += ["--id" if key == "ids" else "--clear", val]
    return ("extract_refusal", "structural-extract-" + variant, argv,
            lambda: ox.extract(source, output, **kwargs), [source], [output])


def version_case(ox, work, command):
    d = _directory(work, "version-" + command)
    source = _write(d / "authored-old-format.blend", fixture_minimal_bytes())
    # This is an authored old-format fixture, NEVER a version-patched native file.
    if command == "node-tree":
        return ("node_version", "structural-node_version-inspect", ["node-tree", str(source)], lambda: ox.node_tree(source), [source], [])
    if command == "node-edit":
        return _edit_case(ox, d, source, "node_version", "node_version-edit", {"schema": 1, "tree": 0, "expected_graph": {}, "operations": []}, "node-tree")
    return _edit_case(ox, d, source, "edit_version", "edit_version-edit", {"schema": 2, "writes": [], "bits": []})


def rewrite_case(ox, work, variant):
    d = _directory(work, "rewrite-" + variant)
    f = NativeFault(ox)
    image = f.add("Image", b"IM\0\0")
    f.text(image, ["id", "name"], "IML20Negative")
    f.text(image, ["name"], "//old.png")
    _, field = f.loc(image, ["name"])
    old, new = b"//old.png", b"//new.png"
    if variant == "empty_clear": new = b""
    elif variant == "path_nul": new = b"//bad\0name"
    elif variant == "path_overflow": new = b"x" * field["size"]
    elif variant == "path_termination": f.setraw(image, ["name"], b"x" * field["size"])
    elif variant == "expected_path": old = b"//stale.png"
    source = f.save(d)
    spec = {"schema": 1, "edits": [{"block_index": image, "element": 0,
             "path": [{"field_hex": b"name".hex()}], "expected_path_hex": old.hex(),
             "replacement_path_hex": new.hex()}]}
    if variant == "edit_spec": spec["schema"] = 0
    specpath = _write(d / "spec.json", json_bytes(spec))
    output = d / "new-output.blend"
    def thunk():
        try:
            ox.rewrite(source, output, spec)
        except ox.OxblendError as e:
            if variant != "edit_spec":
                assert variant in [r["kind"] for r in e.report.errors], e.report
            raise
    return ("edit_spec" if variant == "edit_spec" else "incomplete_edit", "structural-rewrite-" + variant,
            ["rewrite", str(source), "--output", str(output), "--edits", str(specpath)],
            thunk, [source, specpath], [output])


CASE_BUILDERS = []
for _v in ("edit_spec", "expected_old", "typed_kind", "typed_overflow", "typed_width", "edit_conflict", "string_nul", "string_termination", "padding_write", "pointer_write", "field_owner", "unsupported_edit", "flag_mask", "profile_field", "profile_linked", "profile_temporal", "profile_mask", "profile_owner", "profile_library", "profile_discriminator"):
    CASE_BUILDERS.append(partial(typed_case, variant=_v))
for _v in ("node_expected_graph", "node_graph", "node_endpoint", "node_socket_type", "node_occupied", "node_cycle", "node_constructor", "name_collision", "cstring_width", "identifier_overflow", "link_profile", "edit_spec"):
    CASE_BUILDERS.append(partial(graph_case, variant=_v))
for _v, _cmd in (("list_symmetry", "node-tree"), ("list_endpoint", "node-tree"), ("list_cycle", "node-tree"), ("list_boundary", "pointer"), ("list_backlink", "pointer"), ("list_cycle", "pointer"), ("node_endpoint", "node-tree"), ("node_graph", "node-tree"), ("owner_header", "pointer-census"), ("record_extent", "pointer-census"), ("stray_data_run", "edit"), ("duplicate_data_address", "pointer"), ("pointer_count", "pointer"), ("pointer_extent", "pointer"), ("pointer_target_type", "pointer"), ("pointer_role", "pointer"), ("unprofiled_role", "pointer"), ("incomplete_overrides", "overrides")):
    CASE_BUILDERS.append(partial(fault_case, variant=_v, command=_cmd))
for _v, _cmd in (("list_header", "pointer"), ("field_type", "node-tree"), ("storage_profile", "extract"), ("kit_dna", "node-edit")):
    CASE_BUILDERS.append(partial(fault_case, variant=_v, command=_cmd))
for _v, _cmd in (("node_cycle", "node-tree"), ("node_socket_type", "node-tree"), ("record_extent", "node-tree"), ("record_extent", "extract"), ("owner_header", "node-tree"), ("stray_data_run", "node-tree"), ("stray_data_run", "pointer-census")):
    CASE_BUILDERS.append(partial(fault_case, variant=_v, command=_cmd))
for _v in ("empty_clear", "path_nul", "path_overflow", "path_termination", "expected_path", "edit_spec"):
    CASE_BUILDERS.append(partial(rewrite_case, variant=_v))
for _v in ("selection", "policy"): CASE_BUILDERS.append(partial(extract_case, variant=_v))
for _cmd in ("node-tree", "node-edit", "edit"): CASE_BUILDERS.append(partial(version_case, command=_cmd))


def _named_builder(bound):
    def build(ox, work):
        kind, name, argv, thunk, inputs, outputs = bound(ox, work)
        # The unmodified generator and provenance are independently hashed too.
        originals = [BASE, BASE.with_name("PROVENANCE.md"), BASE.with_name("generate.py")]
        return kind, name, argv, thunk, originals + inputs, outputs
    build.__name__ = "case_structural_" + bound.func.__name__ + "_" + "_".join(str(v).replace("-", "_") for v in bound.keywords.values())
    return build


CASE_BUILDERS = [_named_builder(builder) for builder in CASE_BUILDERS]
