"""Reader-family error witnesses (L20b, error-inventory-readers lane).

This module owns ONLY the case builders below. It exposes ``CASE_BUILDERS`` in
the same 6-tuple contract used by ``error_inventory.py``:

    (ox, work) -> (kind, name, argv, py_thunk, input_paths, claim_outputs)

The written triage that decides which catalog kinds belong here is
``triage-readers.json`` next to the L20b evidence; the merge owner keeps the
single committed exemptions file. Importing this module must not run the CLI:
only helper imports and constants live at module scope.

Every fixture is constructed from synthetic saved bytes in this module. No
original .blend file is patched, rewritten or repaired. Each case is expected
to be a real CLI refusal whose canonical error object matches the installed
Python package exactly; the shared harness records the actual CLI error, exit
code, stdout and Python exception per case.
"""

from __future__ import annotations

import gzip
import struct
import zlib
from pathlib import Path

from error_inventory import _dna, _write, fixture_minimal_bytes

__all__ = ["CASE_BUILDERS"]


# --------------------------------------------------------------------------- #
# Synthetic saved-byte helpers (local to this lane; no product bytes touched)
# --------------------------------------------------------------------------- #

_HEADER = b"BLENDER-v405"


def _blend(blocks, dna: bytes | None = None) -> bytes:
    """Serialize a little-endian format-0 saved file.

    ``blocks`` entries are ``(code, address, count, payload[, sdna_index])``.
    When ``sdna_index`` is omitted the block is written with index 0 (the
    value/pointer reader still resolves struct 0; the census/role engine treats
    index 0 as an opaque allocation). A DNA1 block and ENDB follow.
    """
    out = bytearray(_HEADER)
    for entry in blocks:
        code, address, count, payload = entry[:4]
        sdna = entry[4] if len(entry) > 4 else 0
        out.extend(struct.pack("<4sIQII", code, len(payload), address, sdna, count))
        out.extend(payload)
    if dna is not None:
        out.extend(struct.pack("<4sIQII", b"DNA1", len(dna), 0, 0, 0))
        out.extend(dna)
    out.extend(struct.pack("<4sIQII", b"ENDB", 0, 0, 0, 0))
    return bytes(out)


def _case_dir(work: Path, name: str) -> Path:
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    return d


def _gzip_with_bad_crc(data: bytes) -> bytes:
    """A gzip member whose payload is well-formed but whose CRC is wrong.

    The reader inflates the member and then compares the trailer; the mismatch
    is a decompression-family refusal (stream_decode), not an OS error.
    """
    compressor = zlib.compressobj(9, zlib.DEFLATED, -15)
    body = compressor.compress(data) + compressor.flush()
    return (
        b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x00\xff"
        + body
        + struct.pack("<II", 0, len(data) & 0xFFFFFFFF)  # CRC intentionally 0
    )


# A minimal ID-bearing Image record: id embed, name, then two pointer fields.
# Struct index 1 is Image (not 0) because the census/role engine reserves
# block sdna index 0 as an opaque allocation.
_ID_NAMES = [b"id", b"*next", b"*prev", b"*lib", b"name[32]", b"*bytes", b"**table", b"pad[8]"]
_ID_TYPES = [b"char", b"ID", b"Image", b"Test"]
_ID_DNA = _dna(
    _ID_NAMES,
    _ID_TYPES,
    [1, 56, 72, 8],
    [
        (1, [(1, 1), (1, 2), (1, 3), (0, 4)]),
        (2, [(1, 0), (0, 5), (3, 6)]),
        (3, [(0, 7)]),
    ],
)


def _image_payload(*, bytes_address: int = 0, table_address: int = 0, name: bytes = b"img") -> bytes:
    payload = struct.pack("<QQQ", 0, 0, 0) + name.ljust(32, b"\0")[:32]
    payload += struct.pack("<QQ", bytes_address, table_address)
    assert len(payload) == 72, len(payload)
    return payload


def _image_with(blocks_after, name: str, work: Path, *, bytes_address: int = 0, table_address: int = 0):
    path = _write(
        _case_dir(work, name) / f"{name}.blend",
        _blend([(b"IM\0\0", 0x1000, 1, _image_payload(
            bytes_address=bytes_address, table_address=table_address), 1)] + blocks_after, _ID_DNA),
    )
    return path


# Struct 0 is a dummy so the real struct is index 1 (a census-visible claim).
_RECORD_NAMES = [b"name[8]", b"pad[48]"]
_RECORD_DNA = _dna(_RECORD_NAMES, [b"char", b"Dummy", b"Test"], [1, 8, 48],
                   [(1, [(0, 0)]), (2, [(0, 1)])])

# A FileGlobal whose saved curscene pointer does not resolve in this file.
_GLOB_DNA = _dna([b"*curscene", b"name[64]"], [b"uint64_t", b"char", b"FileGlobal"],
                 [8, 1, 72], [(2, [(0, 0), (1, 1)])])

# An Object-like ID whose SDNA type name is not valid UTF-8.
_ENCODING_DNA = _dna(
    [b"id", b"*next", b"*prev", b"*lib", b"name[32]"],
    [b"char", b"ID", b"Ob\xff"],
    [1, 56, 56],
    [(1, [(1, 1), (1, 2), (1, 3), (0, 4)]), (2, [(1, 0)])],
)


# A bPose DATA block inside an Object run: the checked bPose.chanbase list and
# its bPoseChannel elements are the saved-byte layout the reader qualifies.
_POSE_NAMES = [b"id", b"*next", b"*prev", b"*lib", b"name[32]", b"chanbase",
               b"*first", b"*last", b"pad[8]", b"type"]
_POSE_TYPES = [b"char", b"ID", b"Object", b"bPose", b"ListBase", b"bPoseChannel",
               b"int", b"bConstraint", b"void"]
_POSE_LENGTHS = [1, 56, 56, 16, 16, 16, 4, 16, 1]
_POSE_STRUCTS = [
    (1, [(1, 1), (1, 2), (1, 3), (0, 4)]),   # ID
    (4, [(8, 6), (8, 7)]),                   # ListBase: *first / *last (declared void)
    (5, [(0, 1), (0, 2)]),                   # bPoseChannel: *next / *prev at 0/8
    (3, [(4, 5)]),                           # bPose.chanbase
    (2, [(1, 0)]),                           # Object.id
]
_POSE_DNA = _dna(_POSE_NAMES, _POSE_TYPES, _POSE_LENGTHS, _POSE_STRUCTS)

# Element without a next pointer at offset zero: the intrusive header is absent.
_POSE_HEADER_DNA = _dna(
    _POSE_NAMES, _POSE_TYPES, [1, 56, 56, 16, 16, 20, 4, 16, 1],
    [
        (1, [(1, 1), (1, 2), (1, 3), (0, 4)]),
        (4, [(8, 6), (8, 7)]),
        (5, [(6, 9), (0, 1), (0, 2)]),
        (3, [(4, 5)]),
        (2, [(1, 0)]),
    ],
)

# Element saved as bConstraint, not the profiled bPoseChannel.
_POSE_TARGET_DNA = _dna(
    _POSE_NAMES, _POSE_TYPES, [1, 56, 56, 16, 16, 16, 4, 16, 1],
    [
        (1, [(1, 1), (1, 2), (1, 3), (0, 4)]),
        (4, [(8, 6), (8, 7)]),
        (7, [(0, 1), (0, 2)]),
        (3, [(4, 5)]),
        (2, [(1, 0)]),
    ],
)


def _pose_file(work: Path, name: str, *, first: int, last: int,
               element: bytes, element_count: int = 1, dna: bytes = _POSE_DNA) -> Path:
    obj = struct.pack("<QQQ", 0, 0, 0) + b"ob".ljust(32, b"\0")
    return _write(
        _case_dir(work, name) / f"{name}.blend",
        _blend([
            (b"OB\0\0", 0x1000, 1, obj, 4),                       # Object ID header
            (b"DATA", 0x1100, 1, struct.pack("<QQ", first, last), 3),  # bPose.chanbase
            (b"DATA", 0x2000, element_count, element, 2),           # bPoseChannel element(s)
        ], dna),
    )


# A checked role whose profile field is absent from the source DNA.
_CONSTRAINT_DNA = _dna([b"*data"], [b"char", b"void", b"bConstraint"], [1, 1, 8],
                       [(2, [(1, 0)])])

# An Object that embeds bConstraint{type; *bChildOfConstraint data}: the profile
# requires a saved single void pointer, so this non-void declarator is refused.
_PROFILE_NAMES = [b"id", b"*next", b"*prev", b"*lib", b"name[32]",
                  b"type", b"*data", b"constraint"]
_PROFILE_DNA = _dna(
    _PROFILE_NAMES,
    [b"char", b"ID", b"Object", b"int", b"bConstraint", b"bChildOfConstraint", b"void"],
    [1, 56, 68, 4, 12, 16, 1],
    [
        (1, [(1, 1), (1, 2), (1, 3), (0, 4)]),
        (4, [(3, 5), (5, 6)]),
        (5, [(0, 1), (0, 2)]),
        (2, [(1, 0), (4, 7)]),
    ],
)


# --------------------------------------------------------------------------- #
# EXR fixtures (standard single-part scanline header; optional pixel chunk)
# --------------------------------------------------------------------------- #

def _exr_attr(name: bytes, typ: bytes, payload: bytes) -> bytes:
    return name + b"\0" + typ + b"\0" + struct.pack("<i", len(payload)) + payload


def _exr_header() -> bytes:
    out = bytearray(struct.pack("<II", 20000630, 2))  # magic, version 2
    channel = b"R\0" + struct.pack("<i", 2) + b"\0\0\0\0" + struct.pack("<ii", 1, 1) + b"\0"
    out += _exr_attr(b"channels", b"chlist", channel)
    out += _exr_attr(b"compression", b"compression", b"\0")  # NONE: one line per chunk
    window = struct.pack("<iiii", 0, 0, 0, 0)                  # 1x1
    out += _exr_attr(b"dataWindow", b"box2i", window)
    out += _exr_attr(b"displayWindow", b"box2i", window)
    out += _exr_attr(b"lineOrder", b"lineOrder", b"\0")
    out += _exr_attr(b"pixelAspectRatio", b"float", struct.pack("<f", 1.0))
    out += _exr_attr(b"screenWindowCenter", b"v2f", struct.pack("<ff", 0.0, 0.0))
    out += _exr_attr(b"screenWindowWidth", b"float", struct.pack("<f", 1.0))
    out += b"\0"
    return bytes(out)


def _exr_pixels_file(work: Path, name: str, *, chunk_offset: int = 0,
                     chunk_size: int = 4, pixel: float = 1.0) -> Path:
    header = _exr_header()
    chunk = struct.pack("<ii", 0, chunk_size) + struct.pack("<f", pixel)
    if chunk_offset == 0:
        chunk_offset = len(header) + 8
    data = header + struct.pack("<Q", chunk_offset) + chunk
    return _write(_case_dir(work, name) / f"{name}.exr", data)


# --------------------------------------------------------------------------- #
# Cases: header / block / SDNA / decompression
# --------------------------------------------------------------------------- #

def case_unsupported_format_inspect(ox, work: Path):
    name = "readers-unsupported-format"
    path = _write(_case_dir(work, name) / "legacy.blend", b"BLENDER17-02v0405")
    return ("unsupported_format", name, ["inspect", str(path)],
            lambda: ox.inspect(str(path)), [path], [])


def case_invalid_block_negative_size(ox, work: Path):
    name = "readers-invalid-block"
    data = bytearray(fixture_minimal_bytes())
    data[16:20] = struct.pack("<I", 0x80000000)  # BHead length: negative as i32
    path = _write(_case_dir(work, name) / "negative-size.blend", bytes(data))
    return ("invalid_block", name, ["inspect", str(path)],
            lambda: ox.inspect(str(path)), [path], [])


def case_invalid_sdna_missing_dna(ox, work: Path):
    name = "readers-invalid-sdna"
    payload = struct.pack("<QQ8siiiIQ", 1, 0, b"n".ljust(8, b"\0"), 0, 0, 0, 0, 0)
    path = _write(_case_dir(work, name) / "no-dna1.blend",
                  _blend([(b"TEST", 0x1000, 1, payload)], None))
    return ("invalid_sdna", name, ["inspect", str(path)],
            lambda: ox.inspect(str(path)), [path], [])


def case_compression_gzip_crc(ox, work: Path):
    name = "readers-compression"
    path = _write(_case_dir(work, name) / "bad-crc.blend.gz",
                  _gzip_with_bad_crc(_HEADER))
    return ("compression", name, ["inspect", str(path)],
            lambda: ox.inspect(str(path)), [path], [])


# --------------------------------------------------------------------------- #
# Cases: field paths, pointer depth and counted reads
# --------------------------------------------------------------------------- #

def case_path_empty_get(ox, work: Path):
    name = "readers-path-empty"
    path = _write(_case_dir(work, name) / "minimal.blend", fixture_minimal_bytes())
    return ("path", name, ["get", str(path), "0", "[]"],
            lambda: ox.get(str(path), 0, []), [path], [])


def case_type_pointer_non_pointer(ox, work: Path):
    name = "readers-type-not-pointer"
    path = _write(_case_dir(work, name) / "minimal.blend", fixture_minimal_bytes())
    return ("type", name, ["pointer", str(path), "0", '["name"]'],
            lambda: ox.pointer(str(path), 0, ["name"]), [path], [])


def case_pointer_path_dereference(ox, work: Path):
    name = "readers-pointer-path"
    path = _write(_case_dir(work, name) / "minimal.blend", fixture_minimal_bytes())
    return ("pointer_path", name, ["get", str(path), "0", '["next","name"]'],
            lambda: ox.get(str(path), 0, ["next", "name"]), [path], [])


def case_pointer_depth_table_on_single(ox, work: Path):
    name = "readers-pointer-depth"
    path = _write(_case_dir(work, name) / "minimal.blend", fixture_minimal_bytes())
    return ("pointer_depth", name,
            ["get", str(path), "0", '["chain"]', "--pointer-table", "1"],
            lambda: ox.get(str(path), 0, ["chain"], pointer_table=1), [path], [])


def case_required_pointer_null(ox, work: Path):
    name = "readers-required-pointer-null"
    path = _image_with([], name, work)  # bytes pointer is zero
    return ("required_pointer_null", name,
            ["get", str(path), "0", '["bytes"]', "--byte-array", "4"],
            lambda: ox.get(str(path), 0, ["bytes"], byte_array=4), [path], [])


def case_dangling_pointer_bytes(ox, work: Path):
    name = "readers-dangling-pointer"
    path = _image_with([], name, work, bytes_address=0xDEAD)
    return ("dangling_pointer", name,
            ["get", str(path), "0", '["bytes"]', "--byte-array", "4"],
            lambda: ox.get(str(path), 0, ["bytes"], byte_array=4), [path], [])


def case_pointer_extent_bytes(ox, work: Path):
    name = "readers-pointer-extent"
    path = _image_with([(b"DATA", 0x2000, 1, b"0123456789")], name, work,
                       bytes_address=0x2000)
    return ("pointer_extent", name,
            ["get", str(path), "0", '["bytes"]', "--byte-array", "20"],
            lambda: ox.get(str(path), 0, ["bytes"], byte_array=20), [path], [])


def case_overflow_table_count(ox, work: Path):
    name = "readers-overflow"
    path = _image_with([(b"DATA", 0x2000, 1, b"\0" * 16)], name, work,
                       table_address=0x2000)
    count = 2**64 - 1  # a usize that overflows count*element_size
    return ("overflow", name,
            ["get", str(path), "0", '["table"]', "--pointer-table", str(count)],
            lambda: ox.get(str(path), 0, ["table"], pointer_table=count), [path], [])


# --------------------------------------------------------------------------- #
# Cases: block query and namespace addresses
# --------------------------------------------------------------------------- #

def case_missing_address_blocks(ox, work: Path):
    name = "readers-missing-address"
    path = _write(_case_dir(work, name) / "minimal.blend", fixture_minimal_bytes())
    return ("missing_address", name, ["blocks", str(path), "--address", "0"],
            lambda: ox.block_report(str(path), address=0), [path], [])


def case_ambiguous_pointer_blocks(ox, work: Path):
    name = "readers-ambiguous-pointer"
    payload = struct.pack("<QQQ8s", 0, 0x2000, 0, b"n".ljust(8, b"\0"))
    path = _write(_case_dir(work, name) / "duplicate.blend",
                  _blend([(b"TEST", 0x1000, 1, payload), (b"TEST", 0x1000, 1, payload)], _ID_DNA))
    return ("ambiguous_pointer", name, ["blocks", str(path), "--address", "4096"],
            lambda: ox.block_report(str(path), address=4096), [path], [])


# --------------------------------------------------------------------------- #
# Cases: whole-file pointer census
# --------------------------------------------------------------------------- #

def case_record_extent_census(ox, work: Path):
    name = "readers-record-extent"
    path = _write(_case_dir(work, name) / "count-mismatch.blend",
                  _blend([(b"TEST", 0x1000, 2, b"\0" * 48, 1)], _RECORD_DNA))
    return ("record_extent", name, ["pointer-census", str(path)],
            lambda: ox.pointer_census(str(path)), [path], [])


def case_stray_data_run_census(ox, work: Path):
    name = "readers-stray-data-run"
    path = _write(_case_dir(work, name) / "leading-data.blend",
                  _blend([(b"DATA", 0x5000, 1, b"\0" * 8)], _RECORD_DNA))
    return ("stray_data_run", name, ["pointer-census", str(path)],
            lambda: ox.pointer_census(str(path)), [path], [])


def case_owner_header_census(ox, work: Path):
    name = "readers-owner-header"
    payload = struct.pack("<QQQ", 0, 0, 0) + b"scene".ljust(32, b"\0") + struct.pack("<QQ", 0, 0)
    # Scene code with an Image struct: the native header contradicts its code.
    path = _write(_case_dir(work, name) / "wrong-owner.blend",
                  _blend([(b"SC\0\0", 0x1000, 1, payload, 1)], _ID_DNA))
    return ("owner_header", name, ["pointer-census", str(path)],
            lambda: ox.pointer_census(str(path)), [path], [])


# --------------------------------------------------------------------------- #
# Cases: per-file facts / query / concept lookup
# --------------------------------------------------------------------------- #

def case_encoding_library_facts(ox, work: Path):
    name = "readers-encoding"
    payload = struct.pack("<QQQ", 0, 0, 0) + b"ob".ljust(32, b"\0")
    path = _write(_case_dir(work, name) / "non-utf8-type.blend",
                  _blend([(b"OB\0\0", 0x1000, 1, payload, 1)], _ENCODING_DNA))
    return ("encoding", name, ["library-facts", str(path)],
            lambda: ox.library_facts(str(path)), [path], [])


def case_incomplete_facts_source_facts(ox, work: Path):
    name = "readers-incomplete-facts"
    payload = struct.pack("<Q", 0xDEAD) + b"g".ljust(64, b"\0")
    path = _write(_case_dir(work, name) / "dangling-curscene.blend",
                  _blend([(b"GLOB", 0x1000, 1, payload)], _GLOB_DNA))
    return ("incomplete_facts", name, ["source-facts", str(path)],
            lambda: ox.source_facts(str(path)), [path], [])


def case_incomplete_query_missing_field(ox, work: Path):
    name = "readers-incomplete-query"
    path = _write(_case_dir(work, name) / "minimal.blend", fixture_minimal_bytes())
    return ("incomplete_query", name,
            ["query", str(path), "--type", "Test", "--fields", "nosuch"],
            lambda: ox.query(str(path), type="Test", fields=["nosuch"]), [path], [])


def case_unknown_concept_explain(ox, work: Path):
    name = "readers-unknown-concept"
    return ("unknown_concept", name, ["explain", "NoSuchConcept"],
            lambda: ox.explain("NoSuchConcept"), [], [])


# --------------------------------------------------------------------------- #
# Cases: EXR reader and scan
# --------------------------------------------------------------------------- #

def case_exr_unsupported_version(ox, work: Path):
    name = "readers-exr-unsupported"
    path = _write(_case_dir(work, name) / "version.exr",
                  struct.pack("<II", 20000630, 0x00002000))
    return ("exr_unsupported", name, ["exr-header", str(path)],
            lambda: ox.exr_header(str(path)), [path], [])


def case_incomplete_exr_scan_io(ox, work: Path):
    name = "readers-incomplete-exr-scan"
    d = _case_dir(work, name) / "scan"
    d.mkdir(parents=True, exist_ok=True)
    unreadable = d / "unreadable.exr"
    unreadable.write_bytes(struct.pack("<II", 20000630, 2))
    unreadable.chmod(0o000)  # directory walk records this file as an io_error
    return ("incomplete_exr_scan", name, ["exr-scan", str(d)],
            lambda: ox.exr_scan(str(d)), [d], [])


# --------------------------------------------------------------------------- #
# Cases: checked intrusive lists, roles and namespace collisions
# --------------------------------------------------------------------------- #

def case_list_boundary(ox, work: Path):
    name = "readers-list-boundary"
    path = _pose_file(work, name, first=0x2000, last=0x9999,
                      element=struct.pack("<QQ", 0, 0))
    return ("list_boundary", name, ["pointer", str(path), "1", '["chanbase","first"]'],
            lambda: ox.pointer(str(path), 1, ["chanbase", "first"]), [path], [])


def case_list_header(ox, work: Path):
    name = "readers-list-header"
    path = _pose_file(work, name, first=0x2000, last=0x2000,
                      element=struct.pack("<iQQ", 0, 0, 0), dna=_POSE_HEADER_DNA)
    return ("list_header", name, ["pointer", str(path), "1", '["chanbase","first"]'],
            lambda: ox.pointer(str(path), 1, ["chanbase", "first"]), [path], [])


def case_list_backlink(ox, work: Path):
    name = "readers-list-backlink"
    path = _pose_file(work, name, first=0x2000, last=0x2000,
                      element=struct.pack("<QQ", 0, 0x7777))
    return ("list_backlink", name, ["pointer", str(path), "1", '["chanbase","first"]'],
            lambda: ox.pointer(str(path), 1, ["chanbase", "first"]), [path], [])


def case_list_cycle(ox, work: Path):
    name = "readers-list-cycle"
    path = _pose_file(work, name, first=0x2000, last=0x2000,
                      element=struct.pack("<QQ", 0x2000, 0))  # self-referential next
    return ("list_cycle", name, ["pointer", str(path), "1", '["chanbase","first"]'],
            lambda: ox.pointer(str(path), 1, ["chanbase", "first"]), [path], [])


def case_pointer_count_list_element(ox, work: Path):
    name = "readers-pointer-count"
    path = _pose_file(work, name, first=0x2000, last=0x2000,
                      element=struct.pack("<QQ", 0, 0) * 2, element_count=2)
    return ("pointer_count", name, ["pointer", str(path), "1", '["chanbase","first"]'],
            lambda: ox.pointer(str(path), 1, ["chanbase", "first"]), [path], [])


def case_pointer_target_type_list_element(ox, work: Path):
    name = "readers-pointer-target-type"
    path = _pose_file(work, name, first=0x2000, last=0x2000,
                      element=struct.pack("<QQ", 0, 0), dna=_POSE_TARGET_DNA)
    return ("pointer_target_type", name, ["pointer", str(path), "1", '["chanbase","first"]'],
            lambda: ox.pointer(str(path), 1, ["chanbase", "first"]), [path], [])


def case_pointer_role_missing_profile_field(ox, work: Path):
    name = "readers-pointer-role"
    path = _write(_case_dir(work, name) / "constraint.blend",
                  _blend([(b"TEST", 0x1000, 1, struct.pack("<Q", 0x2000))], _CONSTRAINT_DNA))
    return ("pointer_role", name, ["pointer", str(path), "0", '["data"]'],
            lambda: ox.pointer(str(path), 0, ["data"]), [path], [])


def case_unprofiled_role_constraint_profile(ox, work: Path):
    name = "readers-unprofiled-role"
    obj = struct.pack("<QQQ", 0, 0, 0) + b"ob".ljust(32, b"\0") + struct.pack("<iQ", 1, 0x2000)
    path = _write(_case_dir(work, name) / "constraint.blend",
                  _blend([
                      (b"OB\0\0", 0x1000, 1, obj, 3),
                      (b"DATA", 0x2000, 1, struct.pack("<QQ", 0, 0), 2),
                  ], _PROFILE_DNA))
    return ("unprofiled_role", name, ["pointer", str(path), "0", '["constraint","data"]'],
            lambda: ox.pointer(str(path), 0, ["constraint", "data"]), [path], [])


def case_duplicate_data_address(ox, work: Path):
    name = "readers-duplicate-data-address"
    path = _write(_case_dir(work, name) / "duplicate.blend",
                  _blend([
                      (b"IM\0\0", 0x1000, 1,
                       _image_payload(bytes_address=0x2000), 1),
                      (b"DATA", 0x2000, 1, b"a"),
                      (b"DATA", 0x2000, 1, b"b"),  # repeats the same native address
                  ], _ID_DNA))
    return ("duplicate_data_address", name,
            ["get", str(path), "0", '["bytes"]', "--byte-array", "1"],
            lambda: ox.get(str(path), 0, ["bytes"], byte_array=1), [path], [])


# --------------------------------------------------------------------------- #
# Cases: EXR part/pass selection, decode and output path
# --------------------------------------------------------------------------- #

def case_exr_selection_absent_part(ox, work: Path):
    name = "readers-exr-selection"
    path = _write(_case_dir(work, name) / "header-only.exr", _exr_header())
    return ("exr_selection", name,
            ["exr-pixels", str(path), "--part", "5", "--pass", "", "--output",
             str(_case_dir(work, name) / "out.f32")],
            lambda: ox.exr_pixels(str(path), str(_case_dir(work, name) / "out.f32"),
                                  part=5, pass_name=""), [path], [])


def case_exr_decode_truncated_chunk(ox, work: Path):
    name = "readers-exr-decode"
    header = _exr_header()
    path = _write(_case_dir(work, name) / "bad-table.exr", header + struct.pack("<Q", 0))
    return ("exr_decode", name,
            ["exr-pixels", str(path), "--part", "0", "--pass", "", "--output",
             str(_case_dir(work, name) / "out.f32")],
            lambda: ox.exr_pixels(str(path), str(_case_dir(work, name) / "out.f32"),
                                  part=0, pass_name=""), [path], [])


def case_output_path_trailing_separator(ox, work: Path):
    name = "readers-output-path"
    path = _exr_pixels_file(work, name)
    output = str(_case_dir(work, name) / "out") + "/"
    return ("output_path", name,
            ["exr-pixels", str(path), "--part", "0", "--pass", "", "--output", output],
            lambda: ox.exr_pixels(str(path), output, part=0, pass_name=""),
            [path], [Path(output)])


# --------------------------------------------------------------------------- #
# R37 spill_space: real crafted compressed format-1 BHead, native i64 max
# extent, pre-write spill refusal (no disk fill, no freeze/mock).
#
# Uncompressed layout mirrors the shared-owner unit proof
# src/input.rs spill_tests::live_spill_entry_refuses_format_max_before_inflating_payload:
#   BLENDER17-01v0503 (17-byte format-1 header) + DATA code + SDNA 0u32 +
#   old-address 0u64 + extent i64::MAX u64 + count 0u64.
# gzip (stdlib) is the compressed family; both the CLI `blocks` and the
# installed Python `block_report` share oxblend::bat_cli::blocks_to_writer,
# which spill-checks through src/input.rs require_spill_space before any
# payload copy. The message embeds live available bytes, so the harness
# accounts it dynamically (R37) instead of strict equality.
# --------------------------------------------------------------------------- #

def case_spill_space_blocks(ox, work: Path):
    name = "readers-spill-space"
    raw = b"BLENDER17-01v0503DATA"
    raw += struct.pack("<I", 0)
    raw += struct.pack("<Q", 0)
    raw += struct.pack("<Q", (1 << 63) - 1)
    raw += struct.pack("<Q", 0)
    compressed = gzip.compress(raw, compresslevel=9)
    path = _write(_case_dir(work, name) / "spill-max-extent.blend.gz", compressed)
    return ("spill_space", name, ["blocks", str(path)],
            lambda: ox.block_report(str(path)), [path], [])


CASE_BUILDERS = [
    case_unsupported_format_inspect,
    case_invalid_block_negative_size,
    case_invalid_sdna_missing_dna,
    case_compression_gzip_crc,
    case_path_empty_get,
    case_type_pointer_non_pointer,
    case_pointer_path_dereference,
    case_pointer_depth_table_on_single,
    case_required_pointer_null,
    case_dangling_pointer_bytes,
    case_pointer_extent_bytes,
    case_overflow_table_count,
    case_missing_address_blocks,
    case_ambiguous_pointer_blocks,
    case_record_extent_census,
    case_stray_data_run_census,
    case_owner_header_census,
    case_encoding_library_facts,
    case_incomplete_facts_source_facts,
    case_incomplete_query_missing_field,
    case_unknown_concept_explain,
    case_exr_unsupported_version,
    case_incomplete_exr_scan_io,
    case_list_boundary,
    case_list_header,
    case_list_backlink,
    case_list_cycle,
    case_pointer_count_list_element,
    case_pointer_target_type_list_element,
    case_pointer_role_missing_profile_field,
    case_unprofiled_role_constraint_profile,
    case_duplicate_data_address,
    case_exr_selection_absent_part,
    case_exr_decode_truncated_chunk,
    case_output_path_trailing_separator,
    case_spill_space_blocks,
]
