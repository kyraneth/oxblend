"""idprop_string witness: guarded same-allocation string refusal (L24).

Owns ONLY the case builder below. Exposes ``CASE_BUILDERS`` in the same
6-tuple contract used by ``error_inventory.py``:

    (ox, work) -> (kind, name, argv, py_thunk, input_paths, claim_outputs)

The fixture is newly authored from the read-only ``empty.blend`` seed
(372102 B, SHA aabe5b9c...; native 4.5.8 SDNA, header spoofed to 502 as in
``src/typed_edit/idprop_tests.rs``). No original is repaired, resaved or
overwritten. Two fresh blocks are appended before DNA1:

- IDProperty string ``version`` = ``5.3.0-alpha`` (len 12, totallen 12,
  same-owner DATA allocation 12 B, single pointer reference, no alias).
- Typeless DATA allocation with the NUL-terminated payload.

The witness case attempts a too-long replacement (23 B + NUL = 24 B
> 12 B allocation) via the generic Mapping spec surface. Both the CLI
``edit`` and installed Python ``ox.edit`` refuse with terminal kind
``idprop_string`` (``replacement needs 24 bytes including NUL; existing
allocation limit is 12 bytes``), FULL canonical match, exit 2, no
publication. Positive guarded edits (same/shorter replacement) are
covered separately in ``idprop_parity.py``; this module owns only the
refusal witness for inventory.
"""

from __future__ import annotations

import struct
from pathlib import Path

from error_inventory import _write, json_bytes, sha256_bytes

__all__ = ["CASE_BUILDERS"]

BASE = Path(__file__).parents[3] / "tests" / "fixtures" / "file-ops" / "native-4.5.8" / "build-kit" / "empty.blend"
BASE_LEN = 372102
BASE_SHA = "aabe5b9c4a61d5c05182fbc49993fdd4bcb998a211c8454703d72c2d5b8cc749"

IDPROP_ADDR = 0x0A0B0C0D0E0F1000
STRING_ADDR = 0x0A0B0C0D0E0F2000
IDPROP_SDNA = 9
STRING_VALUE = b"5.3.0-alpha"
STRING_NAME = b"version"


def _build_fixture() -> bytes:
    src = BASE.read_bytes()
    assert len(src) == BASE_LEN, len(src)
    assert sha256_bytes(src) == BASE_SHA
    assert src[:12] == b"BLENDER-v405"
    # Find DNA1 offset by scanning block headers (old format, 24 B header).
    pos = 12
    dna_offset = None
    while True:
        code = src[pos:pos + 4]
        size = struct.unpack("<I", src[pos + 4:pos + 8])[0]
        if code == b"DNA1":
            dna_offset = pos
            break
        pos += 24 + size
    assert dna_offset is not None
    prefix = src[:dna_offset]
    # IDProperty payload (136 B, empty.blend SDNA index 9 layout).
    payload = bytearray(136)
    payload[16] = 0  # type string
    payload[17] = 0  # subtype UTF-8
    payload[20:20 + len(STRING_NAME)] = STRING_NAME
    # NUL terminator already zero at payload[20+len].
    struct.pack_into("<Q", payload, 88, STRING_ADDR)
    struct.pack_into("<i", payload, 120, 12)
    struct.pack_into("<i", payload, 124, 12)
    str_payload = STRING_VALUE + b"\x00"
    assert len(str_payload) == 12
    out = bytearray()
    out.extend(b"BLENDER-v502")
    out.extend(prefix[12:])
    out.extend(struct.pack("<4sIQII", b"DATA", len(payload), IDPROP_ADDR, IDPROP_SDNA, 1))
    out.extend(payload)
    out.extend(struct.pack("<4sIQII", b"DATA", len(str_payload), STRING_ADDR, 0, 1))
    out.extend(str_payload)
    out.extend(src[dna_offset:])
    return bytes(out)


def _find_idprop_block(ox, path: Path) -> int:
    info = ox.inspect(path).to_json()
    want = str(IDPROP_ADDR)
    for idx, block in enumerate(info["blocks"]):
        if block["address_decimal"] == want:
            return idx
    raise AssertionError(f"authored IDProperty address {want} not in inspect blocks")


def _case_dir(work: Path, name: str) -> Path:
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    return d


def case_idprop_string_allocation_limit(ox, work: Path):
    """Too-long replacement exceeds the same-allocation limit (idprop_string).

    Both surfaces refuse with kind ``idprop_string`` and the byte-exact
    message ``replacement needs 24 bytes including NUL; existing
    allocation limit is 12 bytes``. No output is published; the input hash
    is preserved.
    """
    name = "idprop_string-allocation-limit"
    d = _case_dir(work, name)
    fixture = _build_fixture()
    inp = _write(d / "idprop.blend", fixture)
    block_index = _find_idprop_block(ox, inp)
    spec = {
        "schema": 2,
        "writes": [],
        "bits": [],
        "idprop_strings": [
            {
                "operation": "idprop-string",
                "block_index": block_index,
                "element": 0,
                "path": ["data", "pointer"],
                "name_hex": STRING_NAME.hex(),
                "expected_hex": STRING_VALUE.hex(),
                "replacement_hex": b"this string is too long".hex(),
            }
        ],
    }
    spec_path = d / "spec.json"
    spec_path.write_bytes(json_bytes(spec))
    dest = d / "out.blend"
    argv = ["edit", str(inp), "--output", str(dest), "--spec", str(spec_path), "--mode", "format-only"]
    return (
        "idprop_string",
        name,
        argv,
        lambda: ox.edit(inp, dest, spec, mode="format-only"),
        [BASE, inp, spec_path],
        [dest],
    )


CASE_BUILDERS = [
    case_idprop_string_allocation_limit,
]
