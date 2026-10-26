"""Trace/list/pack/pack-sequence/report/explain case builders (lead domain).

Self-contained synthetic fixtures; every builder returns the inventory's
``(kind, name, argv, thunk, inputs, claimed_absent[, options])`` contract.
Cases are ordered by how likely a user is to hit the kind: everyday
missing-dependency listings first, then pack/report/explain surface kinds.
Multiroot trace/list fixtures are deliberately absent while the runtime
owner repairs the root-reproduced multiroot canonical-message challenge at
8a20e5b; single-root canonical parity is the accepted baseline here.
"""

from __future__ import annotations

import json
import struct
from pathlib import Path


# --------------------------------------------------------------------------- #
# Synthetic blend construction (fixture code only; no comparison protocol)
# --------------------------------------------------------------------------- #

def _dna(names, types, lengths, structs) -> bytes:
    out = bytearray(b"SDNANAME" + struct.pack("<I", len(names)) + b"\0".join(names) + b"\0")
    out.extend(b"\0" * (-len(out) % 4))
    out.extend(b"TYPE" + struct.pack("<I", len(types)) + b"\0".join(types) + b"\0")
    out.extend(b"\0" * (-len(out) % 4))
    out.extend(b"TLEN" + struct.pack(f"<{len(lengths)}H", *lengths))
    out.extend(b"\0" * (-len(out) % 4))
    out.extend(b"STRC" + struct.pack("<I", len(structs)))
    for struct_type, fields in structs:
        out.extend(struct.pack("<HH", struct_type, len(fields)))
        for type_index, name_index in fields:
            out.extend(struct.pack("<HH", type_index, name_index))
    return bytes(out)


def _build(blocks, dna) -> bytes:
    out = bytearray(b"BLENDER-v405")
    for code, address, sdna, count, payload in blocks:
        out.extend(struct.pack("<4sIQII", code, len(payload), address, sdna, count))
        out.extend(payload)
    for code, payload in ((b"DNA1", dna), (b"ENDB", b"")):
        out.extend(struct.pack("<4sIQII", code, len(payload), 0, 0, 0))
        out.extend(payload)
    return bytes(out)


def _id_payload(name: bytes, lib: int) -> bytes:
    return name.ljust(64, b"\0")[:64] + struct.pack("<Q", lib)


def _lib_payload(name: bytes, path: bytes) -> bytes:
    return _id_payload(name, 0) + path.ljust(128, b"\0")[:128]


# ID(name[64], *lib) / Library(id, name[128]) / VFont(id, name[128]) layouts.
_LINK_NAMES = [b"name[64]", b"*lib", b"id", b"name[128]"]
_LINK_TYPES = [b"char", b"ID", b"Library", b"VFont"]
_LINK_DNA = _dna(
    _LINK_NAMES, _LINK_TYPES, [1, 72, 200, 200],
    [(1, [(0, 0), (2, 1)]), (2, [(1, 2), (0, 3)]), (3, [(1, 2), (0, 3)])],
)


def _minimal_bytes() -> bytes:
    names = [b"number", b"*next", b"name[8]", b"scalar", b"array[2]", b"f32", b"*chain"]
    types = [b"char", b"uint64_t", b"float", b"int", b"Test"]
    fields = [(1, 0), (1, 1), (0, 2), (3, 3), (3, 4), (2, 5), (4, 6)]
    dna = _dna(names, types, [1, 8, 4, 4, 48], [(4, fields)])
    payload = struct.pack("<QQ8siiiIQ", 7, 0, b"root", 1, 2, 3, 0x3F800000, 0)
    return _build([(b"TEST", 0x1000, 0, 1, payload)], dna)


def _case_dir(work: Path, name: str) -> Path:
    directory = work / "cases" / name
    directory.mkdir(parents=True, exist_ok=True)
    return directory


# --------------------------------------------------------------------------- #
# Cases, ordered by likely user impact
# --------------------------------------------------------------------------- #

def case_missing_library_list(ox, work: Path):
    name = "missing_library-list-missing-library"
    d = _case_dir(work, name)
    root = d / "root.blend"
    root.write_bytes(_build([
        (b"IM\0\0", 0x1000, 0, 1, _id_payload(b"IMlinked", 0x2000)),
        (b"LI\0\0", 0x2000, 1, 1, _lib_payload(b"LImissing", b"//no-such-lib.blend")),
    ], _LINK_DNA))
    argv = ["list", str(root), "--json"]
    return ("missing_library", name, argv, lambda: ox.list_dependencies(root), [root], [])


def case_missing_asset_list(ox, work: Path):
    name = "missing_asset-list-missing-font"
    d = _case_dir(work, name)
    root = d / "root.blend"
    vfont = _id_payload(b"VFfont", 0) + b"//fonts/missing.ttf".ljust(128, b"\0")[:128]
    root.write_bytes(_build([(b"VF\0\0", 0x1000, 2, 1, vfont)], _LINK_DNA))
    argv = ["list", str(root), "--json"]
    return ("missing_asset", name, argv, lambda: ox.list_dependencies(root), [root], [])


def case_pattern_members_unknown_list(ox, work: Path):
    name = "pattern_members_unknown-list-sequence-image"
    d = _case_dir(work, name)
    root = d / "root.blend"
    names = [b"name[64]", b"*lib", b"id", b"filepath[128]", b"source"]
    types = [b"char", b"ID", b"Image", b"int"]
    dna = _dna(names, types, [1, 72, 204, 4],
               [(1, [(0, 0), (2, 1)]), (2, [(1, 2), (3, 4), (0, 3)])])
    image = (_id_payload(b"IMseq", 0) + struct.pack("<i", 2)
             + b"//seq/frame_0001.png".ljust(128, b"\0")[:128])
    root.write_bytes(_build([(b"IM\0\0", 0x1000, 1, 1, image)], dna))
    argv = ["list", str(root), "--json"]
    return ("pattern_members_unknown", name, argv, lambda: ox.list_dependencies(root), [root], [])


def case_usage_pack_memory_bogus(ox, work: Path):
    name = "usage-pack-memory-bogus"
    d = _case_dir(work, name)
    minimal = d / "minimal.blend"
    minimal.write_bytes(_minimal_bytes())
    dest = d / "out-pkg"
    argv = ["pack", str(minimal), str(dest), "--memory", "bogus"]

    def thunk() -> None:
        # An opened File carries the pack command context (and its canonical
        # next_command suggestion); the module-level pack() would refuse at
        # open() before any pack context exists.
        with ox.open(minimal) as handle:
            handle.pack(dest, memory="bogus")

    return ("usage", name, argv, thunk, [minimal], [dest])


def case_unknown_concept_explain(ox, work: Path):
    name = "unknown_concept-explain-bad-concept"
    _case_dir(work, name)
    argv = ["explain", "NoSuchConceptXYZ", "--json"]
    return ("unknown_concept", name, argv, lambda: ox.explain("NoSuchConceptXYZ"), [], [])


def case_incomplete_pack_permissive_partial(ox, work: Path):
    """A real partial publication: both surfaces publish the available
    members plus pack-info.txt and still refuse as incomplete_pack. The
    framework snapshots the CLI publication, cleans only this case's own
    destination, and requires the Python publication to match it."""
    name = "incomplete_pack-permissive-partial"
    d = _case_dir(work, name)
    root = d / "root.blend"
    vfont = _id_payload(b"VFfont", 0) + b"//fonts/missing.ttf".ljust(128, b"\0")[:128]
    root.write_bytes(_build([(b"VF\0\0", 0x1000, 2, 1, vfont)], _LINK_DNA))
    dest = d / "out-pkg"
    argv = ["pack", str(root), str(dest), "--permissive"]
    return ("incomplete_pack", name, argv,
            lambda: ox.pack(root, dest, permissive=True),
            [root], [], {"expected_outputs": [dest]})


CASE_BUILDERS = [
    case_missing_library_list,
    case_missing_asset_list,
    case_pattern_members_unknown_list,
    case_usage_pack_memory_bogus,
    case_unknown_concept_explain,
    case_incomplete_pack_permissive_partial,
]
