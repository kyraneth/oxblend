"""Remaining-12 witnesses: diff_identity + incomplete_diff (L20b).

Owns ONLY the case builders below. Exposes ``CASE_BUILDERS`` in the same
6-tuple contract used by ``error_inventory.py``:

    (ox, work) -> (kind, name, argv, py_thunk, input_paths, claim_outputs)

Both fixtures are derived from the read-only clean-room native 5.2.1
qualifier (209110 B, SHA 6e5deefb...). No original is repaired, resaved,
version-spoofed or overwritten. SDNA offsets come from Oxblend inspect;
all mutations are explicit fixture faults, never an alternative production
writer. The unmodified generator/provenance are independently hashed as
readonly inputs alongside each case's own files.

- diff_identity (nonbijective): two bNodes share the same tree-relative
  name+idname identity (L20Math:ShaderNodeMath duplicated). Both the CLI
  ``diff`` and installed Python ``ox.diff`` return the terminal kind
  ``diff_identity`` with message ``nonbijective identity ...``.
- incomplete_diff: clean vs single-field variant (Scene r.xsch +1) with
  identical unresolved sets. Both surfaces return the terminal kind
  ``incomplete_diff`` with the shared-owner message via the strict
  refusal seam (engine-owned, not Python prose).
"""

from __future__ import annotations

import shutil
import struct as _struct
from pathlib import Path

from error_inventory import _dna, _write, json_bytes, sha256_bytes

__all__ = ["CASE_BUILDERS"]


BASE = Path(__file__).with_name("fixtures") / "native521" / "qualifier.blend"
BASE_SHA = "6e5deefb1b96c8726a7f205f612711f5cfb0f578e1babfebef8893293359dd09"


def _load_native(ox):
    data = BASE.read_bytes()
    assert len(data) == 209110, len(data)
    assert sha256_bytes(data) == BASE_SHA
    assert data[:17] == b"BLENDER17-01v0502"
    info = ox.inspect(BASE).to_json()
    types = [bytes.fromhex(t).decode() for t in info["sdna"]["types_hex"]]
    structs = info["sdna"]["structs"]
    by_type = {types[s["type_index"]]: i for i, s in enumerate(structs)}
    rows = []
    import struct as _struct
    for b in info["blocks"]:
        row = dict(b)
        row["data"] = bytearray(data[b["data_offset"]:b["data_offset"] + b["size"]])
        rows.append(row)
    return info, types, structs, by_type, rows, data


class _Fault:
    """Minimal native-fault author (read-only base, owned copy only)."""

    def __init__(self, ox):
        self.info, self.types, self.structs, self.by_type, self.rows, self.original = _load_native(ox)
        self.faults = []

    def find(self, ty, **strings):
        hits = [i for i in range(len(self.rows))
                if self.rows[i]["code_hex"] not in (b"DNA1".hex(), b"ENDB".hex())
                and self.types[self.structs[self.rows[i]["sdna_index"]]["type_index"]] == ty
                and all(self.string(i, [k]) == v for k, v in strings.items())]
        # Restrict bNode/bNodeSocket/bNodeLink to the compositor tree like structural lane.
        if ty in ("bNode", "bNodeLink", "bNodeSocket"):
            def _in_tree(b):
                while b >= 0 and self.rows[b]["code_hex"] == b"DATA".hex():
                    b -= 1
                return b >= 0 and self.rows[b]["code_hex"] == b"NT\0\0".hex()
            hits = [i for i in hits if _in_tree(i)]
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

    def save(self, directory):
        import struct as _struct
        directory.mkdir(parents=True, exist_ok=True)
        data = bytearray(self.original[:17])
        rows = [r for r in self.rows if bytes.fromhex(r["code_hex"]) not in (b"DNA1", b"ENDB")]
        rows += [r for r in self.rows if bytes.fromhex(r["code_hex"]) == b"DNA1"]
        for r in rows:
            data += _struct.pack("<4sIQQQ", bytes.fromhex(r["code_hex"]), r["sdna_index"],
                                 int(r["address_decimal"]), len(r["data"]), r["count"])
            data += r["data"]
        data += _struct.pack("<4sIQQQ", b"ENDB", 0, 0, 0, 0)
        path = _write(directory / "fault.blend", bytes(data))
        _write(directory / "fixture-provenance.json", json_bytes({
            "base": str(BASE), "base_sha256": BASE_SHA,
            "base_provenance": str(BASE.with_name("PROVENANCE.md")),
            "purpose": "negative fixture authoring only; NOT a repair or qualified output",
            "faults": self.faults,
            "fixture_sha256": sha256_bytes(bytes(data))}))
        assert BASE.read_bytes() == self.original
        return path


def _case_dir(work: Path, name: str) -> Path:
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    return d


def _readonly_inputs():
    return [BASE, BASE.with_name("PROVENANCE.md"), BASE.with_name("generate.py")]


def case_diff_identity_nonbijective(ox, work: Path):
    """Two bNodes share one tree-relative name+idname identity.

    L20Value is renamed to L20Math:ShaderNodeMath (both name and idname),
    so Inventory::new assigns the same ``{root}/NODE/...`` key to two
    different blocks and returns terminal ``diff_identity`` with message
    ``nonbijective identity ...`` on both surfaces.
    """
    name = "remaining-diff-identity-nonbijective"
    d = _case_dir(work, name)
    clean = d / "clean.blend"
    shutil.copy(BASE, clean)
    f = _Fault(ox)
    math = f.find("bNode", name="L20Math")
    value = f.find("bNode", name="L20Value")
    # Duplicate both name and idname so the tree-relative identity collides.
    math_idname = f.string(math, ["idname"])
    f.text(value, ["name"], "L20Math")
    f.text(value, ["idname"], math_idname)
    faulty = f.save(d / "faulty")
    argv = ["diff", str(clean), str(faulty)]
    return ("diff_identity", name, argv,
            lambda: ox.diff(clean, faulty),
            _readonly_inputs() + [clean, faulty], [])


def case_incomplete_diff_unresolved(ox, work: Path):
    """Clean vs single-field variant with identical unresolved sets.

    Scene r.xsch +1 changes one compared field (difference_count 1) while
    both files retain the same 13 unresolved persistent pointers (panel
    states, IDProperty children, bNodeTree owner, etc.). Compare succeeds
    with complete=false, so both surfaces raise terminal
    ``incomplete_diff`` via the shared strict-refusal seam.
    """
    name = "remaining-incomplete-diff-unresolved"
    d = _case_dir(work, name)
    clean = d / "clean.blend"
    shutil.copy(BASE, clean)
    f = _Fault(ox)
    scene = f.find("Scene")
    old = f.number(scene, ["r", "xsch"])
    f.set(scene, ["r", "xsch"], old + 1)
    faulty = f.save(d / "variant")
    argv = ["diff", str(clean), str(faulty)]
    return ("incomplete_diff", name, argv,
            lambda: ox.diff(clean, faulty),
            _readonly_inputs() + [clean, faulty], [])


def case_diff_pointer_array_shadowed_odd_table(ox, work: Path):
    """**table odd-size shadow proof (diff_pointer_array unreachable).

    Owned BGen Image with **table pointing to 7-byte odd DATA (vs 16-byte
    even clean). Census records [0].table as unprofiled_role with target
    None (shared role qualifier refuses depth 2 before expansion), so
    expand_pointer_arrays skips it and diff returns incomplete_diff
    (unresolved [0].table, opaque compared as bytes) on both surfaces,
    never diff_pointer_array. This is the negative control proving the
    exemption census-refusal chain; expected kind is honestly
    incomplete_diff, not diff_pointer_array.
    """
    name = "remaining-diff-pointer-array-shadowed"
    d = _case_dir(work, name)
    _names = [b"id", b"*next", b"*prev", b"*lib", b"name[32]", b"*bytes", b"**table", b"pad[8]"]
    _types = [b"char", b"ID", b"Image", b"Test"]
    _dna_bytes = _dna(_names, _types, [1, 56, 72, 8],
                      [(1, [(1, 1), (1, 2), (1, 3), (0, 4)]),
                       (2, [(1, 0), (0, 5), (3, 6)]),
                       (3, [(0, 7)])])
    def _blend(table_addr: int, data_bytes: bytes) -> bytes:
        out = bytearray(b"BLENDER-v405")
        head = _struct.pack("<QQQ", 0, 0, 0) + b"IMtest".ljust(32, b"\0") + _struct.pack("<QQ", 0, table_addr)
        assert len(head) == 72, len(head)
        out.extend(_struct.pack("<4sIQII", b"IM\0\0", len(head), 0x1000, 1, 1))
        out.extend(head)
        out.extend(_struct.pack("<4sIQII", b"DATA", len(data_bytes), table_addr, 0, 1))
        out.extend(data_bytes)
        out.extend(_struct.pack("<4sIQII", b"DNA1", len(_dna_bytes), 0, 0, 0))
        out.extend(_dna_bytes)
        out.extend(_struct.pack("<4sIQII", b"ENDB", 0, 0, 0, 0))
        return bytes(out)
    clean = _write(d / "clean.blend", _blend(0x2000, b"\0" * 16))
    odd = _write(d / "odd.blend", _blend(0x2000, b"\x01" * 7))
    argv = ["diff", str(clean), str(odd)]
    return ("incomplete_diff", name, argv,
            lambda: ox.diff(clean, odd),
            [clean, odd], [])


CASE_BUILDERS = [
    case_diff_identity_nonbijective,
    case_incomplete_diff_unresolved,
    case_diff_pointer_array_shadowed_odd_table,
]
