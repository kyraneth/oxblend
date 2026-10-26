"""Typed in-process saved-file operations; no implicit pointers or Blender evaluation.

``open`` and ``open_many`` capture saved roots into a session; every read,
query, trace, list and pack operation below runs against those captured
bytes. Each operation names its CLI twin (``oxblend summary``,
``oxblend trace``, ``oxblend get`` and friends), which runs the same shared
engine over the same inputs. Reports are owned immutable ``Record`` mappings
whose ``to_json`` is the exact CLI JSON projection; failures are stable-kind
``OxblendError`` subclasses shared with the CLI error report, so ``help()``
here names the same remedy the CLI prints. The native session token
(``BlockInfo``) and the native error roots (``OxblendError``,
``ClosedFileError``, ``SourceChangedError``) receive their runtime
documentation here in the initializer, because the extension module itself
carries no per-member Python docstrings.
"""
from __future__ import annotations
from dataclasses import dataclass, field
import json
import os
import struct
from typing import Any, Callable, Iterable, TypeAlias, TypeVar
from collections.abc import Mapping, Iterator
from types import MappingProxyType
import hashlib
from . import _native
from ._native import BlockInfo, ClosedFileError, OxblendError, SourceChangedError

BlockInfo.__doc__ = """Session-tagged saved block token; indices are only valid in their File.

A BlockInfo names one saved allocation inside the session that produced it:
index (physical position in that session's block order), code (four raw
code bytes including NUL padding), old_address (saved pointer value),
sdna_index (DNA type index), count (struct element count), data_offset
(byte offset of the payload) and size (payload byte length). Every query,
pointer and payload read validates the token against its own File and
refuses foreign tokens, so a token must never cross sessions.

Args:
    None. BlockInfo tokens are issued by the session (``File.blocks``,
    ``File.get``, ``File.pointer`` and ``File.pointer_table``), never
    constructed by hand; direct construction always fails.

Returns:
    Never returns; calling ``BlockInfo()`` raises ``TypeError``. Live
    tokens arrive as session method results.

Raises:
    TypeError: direct construction is rejected by the native extension.

CLI: ``oxblend blocks`` lists the same allocations by numeric index and
``oxblend get`` reads them; this token is the typed in-process equivalent
of that CLI block index.
"""

OxblendError.__doc__ = """Root of every stable oxblend failure.

Carries the engine's canonical fields: kind (the stable snake_case code;
match this, never the message), message, operation, context, diagnostics,
diagnostic_ref, source_snapshot, report, effects, exit_code, explanation,
likely_causes, next_steps, known_issue and next_command. The facade below
normalizes every native raise into the matching generated subclass (see
``ERROR_TYPES`` in ``oxblend.errors``) and fills the catalog remedy fields,
so help() on the subclass is the precise reference.

Args:
    *args: Ordinary Python Exception arguments for manual construction.
        Constructing an exception does not execute the engine, assign
        canonical fields, or prove that this failure occurred.

Returns:
    OxblendError: An exception instance retaining its positional args.

Raises:
    TypeError: Unsupported keyword arguments to the inherited constructor.

No CLI equivalent: the base class itself is never raised; every ``oxblend``
CLI error report carries one of its stable-kind subclasses instead.
"""

ClosedFileError.__doc__ = """A use-after-close or foreign-session access was attempted.

Stable kind ``closed_file``. Raised when an operation touches a File whose
native session is already closed, or when a BlockInfo/FieldQuery token from
another session is used: foreign tokens refuse rather than silently read
the wrong bytes.

Args:
    *args: Ordinary Python Exception arguments for manual construction.
        Constructing an exception does not execute the engine, assign
        canonical fields, or prove that this failure occurred.

Returns:
    ClosedFileError: An exception instance retaining its positional args.

Raises:
    TypeError: Unsupported keyword arguments to the inherited constructor.

No CLI equivalent: sessions and their lifetime are in-process concepts;
each ``oxblend`` CLI invocation opens its inputs implicitly and releases
them on exit.
"""

SourceChangedError.__doc__ = """The backing input changed identity or content during a scan.

Stable kind ``input_changed``. The session snapshots its roots at open; any
later read that observes a replaced, truncated or removed backing file
refuses with this error instead of returning mixed bytes.

Args:
    *args: Ordinary Python Exception arguments for manual construction.
        Constructing an exception does not execute the engine, assign
        canonical fields, or prove that this failure occurred.

Returns:
    SourceChangedError: An exception instance retaining its positional args.

Raises:
    TypeError: Unsupported keyword arguments to the inherited constructor.

CLI: any ``oxblend`` read command (``oxblend summary``, ``oxblend trace``,
``oxblend get`` and friends) that observes the same change reports the
identical kind and message.
"""

__version__ = _native.__version__.replace("-dev.", ".dev")
engine_version = _native.engine_version
# Hash the actual installed extension once. The engine's current_exe identity
# is the Python/Blender host, not this native module.
with os.fdopen(os.open(_native.__file__, os.O_RDONLY), "rb") as _module_file:
    _native.initialize_identity(_native.__file__, hashlib.file_digest(_module_file, "sha256").hexdigest())
del _module_file

@dataclass(frozen=True, slots=True)
class Header:
    """Typed ``oxblend summary`` header: container endianness, size and format.

    version: saved file-format version (e.g. 405 for a Blender 5.x save).
    pointer_size: saved pointer width in bytes (4 or 8).
    little_endian: True when the saved integers are little-endian.
    file_format: saved format tag as a single byte value.
    size: total input byte length observed at open.

    Args:
        version: saved file-format version (e.g. 405 for a Blender 5.x save).
        pointer_size: saved pointer width in bytes (4 or 8).
        little_endian: True when the saved integers are little-endian.
        file_format: saved format tag as a single byte value.
        size: total input byte length observed at open.

    Returns:
        Header: the frozen header holder decoded from saved bytes.

    Raises:
        TypeError: a required field is missing or an unexpected keyword is passed.

    No CLI equivalent: typed view of the ``header`` object in the
    ``oxblend summary`` JSON output.
    """
    version: int
    pointer_size: int
    little_endian: bool
    file_format: int
    size: int

@dataclass(frozen=True, slots=True)
class Float32Bits:
    """Saved 32-bit float kept as exact bits, so NaN payloads survive.

    bits: the raw unsigned 32-bit pattern as saved. ``value`` decodes it to
a Python float without normalizing quiet/signalling NaN payloads.

    Args:
        bits: the raw unsigned 32-bit pattern as saved.

    Returns:
        Float32Bits: the frozen exact-bits holder.

    Raises:
        ValueError: ``bits`` is not an unsigned 32-bit integer.
        TypeError: the ``bits`` field is missing or an unexpected keyword is passed.

    No CLI equivalent: typed view of the ``bits_hex`` field in the
    ``oxblend get`` JSON value projection for float32 leaves.
    """
    bits: int
    def __post_init__(self) -> None:
        # u32 is the engine representation, not a workload limit.
        if not isinstance(self.bits, int) or not 0 <= self.bits <= 0xFFFFFFFF:
            raise ValueError("bits must fit an unsigned 32-bit integer")
    @property
    def value(self) -> float:
        """Decode the saved bits to a Python float without normalizing NaN.

        Returns:
            The float value of the saved 32-bit pattern.

        Raises:
            Nothing.

        No CLI equivalent: typed view of the ``bits_hex`` field in the
        ``oxblend get`` JSON value projection for float32 leaves.
        """
        return struct.unpack("!f", struct.pack("!I", self.bits))[0]

@dataclass(frozen=True, slots=True)
class FieldValue:
    """One typed saved value plus its exact schema-1/``value`` projection.

    kind: engine type tag (``signed``, ``unsigned``, ``pointer``, ``bytes``,
    ``float32`` or ``array``).
    value: the decoded payload: int for integers/pointers, bytes trimmed at
    NUL for strings, ``Float32Bits`` for floats, or a tuple of ``FieldValue``
    for arrays.
    _json: the engine's exact JSON projection wrapped by ``to_json``.

    Args:
        kind: the engine type tag.
        value: the decoded payload.
        _json: the engine's exact JSON projection wrapped by ``to_json``.

    Returns:
        FieldValue: the frozen typed value plus its exact projection.

    Raises:
        TypeError: a required field is missing or an unexpected keyword is passed.

    No CLI equivalent: typed view of one entry in the ``oxblend get`` JSON
    value projection.
    """
    kind: str
    value: int | bytes | Float32Bits | tuple[FieldValue, ...]
    _json: Mapping[str, Any] = field(default_factory=dict, repr=False, compare=False)
    def to_json(self) -> Any:
        """Render the engine's exact schema-1/``value`` JSON projection.

        Returns:
            ``{'schema': 1, 'value': ...}`` with bytes rendered as hex, matching
            the ``oxblend get`` stdout value object byte for byte.

        Raises:
            Nothing.

        No CLI equivalent: typed accessor producing the ``oxblend get`` JSON
        value object for this field.
        """
        return {'schema': 1, 'value': _thaw(self._json)}
    @property
    def text(self) -> str:
        """Decode a bytes value as strict UTF-8 text.

        Returns:
            The decoded string.

        Raises:
            TypeError: the value is not bytes (only saved strings decode).
            UnicodeDecodeError: the saved bytes are not valid UTF-8; use the
                raw ``value`` bytes instead.

        No CLI equivalent: typed view of the decoded text behind the
        ``oxblend get`` JSON hex payload for bytes values.
        """
        if not isinstance(self.value, bytes):
            raise TypeError("text is only available for bytes")
        return self.value.decode("utf-8", errors="strict")

@dataclass(frozen=True, slots=True)
class FieldQuery:
    """One addressed saved-field read: session token, element and path.

    block: the session-tagged ``BlockInfo`` token selecting the allocation;
    tokens from another session refuse as foreign.
    element: nonnegative struct element inside the block's count.
    path: field-name bytes and array-index steps from the struct root; never
a scalar string, which would hide a missing separator.
    null_terminated: when True, trailing NUL bytes are trimmed from char
    arrays (pass False for the raw ``oxblend get --raw`` semantics).

    Args:
        block: the session-tagged ``BlockInfo`` token selecting the allocation.
        element: nonnegative struct element inside the block's count.
        path: field-name bytes and array-index steps from the struct root.
        null_terminated: when True, trailing NUL bytes are trimmed from char
            arrays (pass False for the raw ``oxblend get --raw`` semantics).

    Returns:
        FieldQuery: the frozen addressed-field selector.

    Raises:
        TypeError: ``path`` is a scalar string, ``block`` is not a
            session-tagged ``BlockInfo``, a path step is neither bytes nor
            a nonnegative index, or ``null_terminated`` is not a bool.
        ValueError: ``element`` is not a nonnegative integer.

    No CLI equivalent: typed form of the BLOCK/PATH_JSON/ELEMENT arguments
    to ``oxblend get`` and ``oxblend pointer``.
    """
    block: BlockInfo
    element: int
    path: tuple[bytes | int, ...]
    null_terminated: bool = True
    def __post_init__(self) -> None:
        if isinstance(self.path, (bytes, bytearray, memoryview, str)):
            raise TypeError("path must be a sequence of field names/indices, not a scalar")
        object.__setattr__(self, "path", tuple(self.path))
        if not isinstance(self.block, BlockInfo):
            raise TypeError("block must be a session-tagged BlockInfo")
        if type(self.element) is not int or self.element < 0:
            raise ValueError("element must be a nonnegative integer")
        if any(not isinstance(s, bytes) and (type(s) is not int or s < 0) for s in self.path):
            raise TypeError("path steps must be bytes or nonnegative integer indices")
        if type(self.null_terminated) is not bool:
            raise TypeError("null_terminated must be bool")

@dataclass(frozen=True, slots=True)
class Diagnostic:
    """One non-fatal saved-content warning attached to a summary or report.

    kind: stable warning code.
    block_index: physical block the warning refers to, or -1 when global.
    element: struct element the warning refers to.
    field: field name the warning refers to, empty when not field-specific.
    message: human-readable warning text; never matched programmatically.

    Args:
        kind: stable warning code.
        block_index: physical block the warning refers to, or -1 when global.
        element: struct element the warning refers to.
        field: field name the warning refers to, empty when not field-specific.
        message: human-readable warning text; never matched programmatically.

    Returns:
        Diagnostic: the frozen warning holder.

    Raises:
        TypeError: a required field is missing or an unexpected keyword is passed.

    No CLI equivalent: typed view of one entry in the ``warnings`` array of
    the ``oxblend summary`` JSON output.
    """
    kind: str
    block_index: int
    element: int
    field: str
    message: str

@dataclass(frozen=True, slots=True)
class Fps:
    """Saved Scene frame-rate triple; missing fields stay None, never defaulted.

    numerator: saved frames-per-second numerator, None when the save has none.
    base: saved base rate as exact bits, None when the save has none.
    effective: numerator/base as evaluated from saved bytes, None when the
    rate cannot be formed (missing or non-finite parts).

    Args:
        numerator: saved frames-per-second numerator, None when the save has none.
        base: saved base rate as exact bits, None when the save has none.
        effective: numerator/base as evaluated from saved bytes, None when the
            rate cannot be formed (missing or non-finite parts).

    Returns:
        Fps: the frozen frame-rate holder.

    Raises:
        TypeError: a required field is missing or an unexpected keyword is passed.

    No CLI equivalent: typed view of the ``fps`` object in the
    ``oxblend summary`` JSON output.
    """
    numerator: int | None
    base: Float32Bits | None
    effective: float | None

@dataclass(frozen=True, slots=True)
class SceneSummary:
    """One saved Scene row: identity bytes, frame range, camera and rate.

    name: saved Scene name bytes (raw, may be non-UTF-8), None when absent.
    frame_start: saved start frame, None when the field is absent.
    frame_end: saved end frame, None when the field is absent.
    frame_step: saved frame step, None when the field is absent.
    camera: saved active-camera name bytes, None when absent or unset.
    fps: the saved frame-rate triple for this Scene.

    Args:
        name: saved Scene name bytes (raw, may be non-UTF-8), None when absent.
        frame_start: saved start frame, None when the field is absent.
        frame_end: saved end frame, None when the field is absent.
        frame_step: saved frame step, None when the field is absent.
        camera: saved active-camera name bytes, None when absent or unset.
        fps: the saved frame-rate triple for this Scene.

    Returns:
        SceneSummary: the frozen Scene-row holder.

    Raises:
        TypeError: a required field is missing or an unexpected keyword is passed.

    No CLI equivalent: typed view of one entry in the ``scenes`` array of
    the ``oxblend summary`` JSON output.
    """
    name: bytes | None
    frame_start: int | None
    frame_end: int | None
    frame_step: int | None
    camera: bytes | None
    fps: Fps

@dataclass(frozen=True, slots=True)
class Summary:
    """Typed saved summary: header, block census, named sets and warnings.

    header: the container ``Header``.
    block_codes: (four raw code bytes, occurrence count) pairs in saved order.
    objects: saved object name bytes in saved order.
    scenes: one ``SceneSummary`` per saved Scene.
    materials: saved material name bytes in saved order.
    meshes: saved mesh name bytes in saved order.
    images: saved image name bytes in saved order.
    collections: saved collection name bytes in saved order.
    warnings: non-fatal saved-content diagnostics.
    _json: the engine's exact JSON projection wrapped by ``to_json``.
    _json_error: the encoding failure raised by ``to_json`` when a name is
    not valid UTF-8; the typed sets above still carry the raw bytes.

    Args:
        header: the container ``Header``.
        block_codes: (four raw code bytes, occurrence count) pairs in saved order.
        objects: saved object name bytes in saved order.
        scenes: one ``SceneSummary`` per saved Scene.
        materials: saved material name bytes in saved order.
        meshes: saved mesh name bytes in saved order.
        images: saved image name bytes in saved order.
        collections: saved collection name bytes in saved order.
        warnings: non-fatal saved-content diagnostics.

    Returns:
        Summary: the frozen typed summary over the captured input.

    Raises:
        TypeError: a required field is missing or an unexpected keyword is passed.

    No CLI equivalent: typed view of the ``oxblend summary`` JSON document.
    """
    header: Header
    block_codes: tuple[tuple[bytes, int], ...]
    objects: tuple[bytes, ...]
    scenes: tuple[SceneSummary, ...]
    materials: tuple[bytes, ...]
    meshes: tuple[bytes, ...]
    images: tuple[bytes, ...]
    collections: tuple[bytes, ...]
    warnings: tuple[Diagnostic, ...]
    _json: Mapping[str, Any] = field(default_factory=dict, repr=False, compare=False)
    _json_error: OxblendError | None = field(default=None, repr=False, compare=False)
    def to_json(self) -> Any:
        """Render the exact ``oxblend summary`` JSON document.

        Returns:
            The owned CLI projection with UTF-8 names decoded.

        Raises:
            OxblendError: the stored encoding failure when a saved name is
                not valid UTF-8; the CLI refuses the same input identically.

        CLI: ``oxblend summary``.
        """
        if self._json_error is not None: raise self._json_error
        return _thaw(self._json)

_RecordT = TypeVar('_RecordT', bound='Record')

@dataclass(frozen=True, slots=True, eq=False)
class Record(Mapping[str, Any]):
    """Owned immutable engine facts; unknown fields and exact hex bytes survive.

    A Record is a frozen mapping over the engine's JSON-compatible facts:
    construction defensively copies caller mappings, nested mappings become
    nested Records, and byte strings stay exact (``*_hex`` fields) rather
    than lossy text. Additive engine fields remain reachable by key even
    where no typed property names them. ``facts`` is the only constructor
    parameter: the facts mapping to own.

    Args:
        facts: the facts mapping to own. Construction defensively copies
            caller mappings; nested mappings become nested Records.

    Returns:
        Record: the owned immutable mapping over the engine facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: the common in-process view shared by every
    ``oxblend`` JSON report (``oxblend trace``, ``oxblend summary``,
    ``oxblend inspect`` and friends); ``to_json`` renders the CLI bytes.
    """
    _facts: Mapping[str, Any]
    def __post_init__(self) -> None:
        object.__setattr__(self, "_facts", MappingProxyType({k: _freeze(v) for k, v in self._facts.items()}))
    @classmethod
    def _from_native_owned(cls: type[_RecordT], facts: dict[str, Any]) -> _RecordT:
        """Adopt a fresh private native JSON tree, never a caller-owned mapping.

        The native converter creates no mutable aliases. Freezing it in place
        avoids copying millions of block dictionaries; public Record(mapping)
        deliberately keeps its defensive-copy contract.
        """
        result = object.__new__(cls)
        for key, value in facts.items():
            frozen = _freeze_native_owned(value)
            if frozen is not value:
                facts[key] = frozen
        object.__setattr__(result, '_facts', MappingProxyType(facts))
        return result
    def __getitem__(self, key: str) -> Any:
        """Return the owned fact stored under ``key``.

        Args:
            key: the engine field name, e.g. ``complete``.

        Returns:
            The owned value; nested mappings are nested ``Record`` objects.

        Raises:
            KeyError: no such field in this report.

        No CLI equivalent: mapping access over the ``oxblend`` JSON report
        fields; ``to_json`` renders the same document.
        """
        return self._facts[key]
    def __iter__(self) -> Iterator[str]:
        """Iterate the stored field names in engine order.

        Returns:
            An iterator over the field-name strings.

        Raises:
            Nothing.

        No CLI equivalent: mapping protocol over the ``oxblend`` JSON
        report fields.
        """
        return iter(self._facts)
    def __len__(self) -> int:
        """Count the stored fields.

        Returns:
            The number of top-level fields.

        Raises:
            Nothing.

        No CLI equivalent: mapping protocol over the ``oxblend`` JSON
        report fields.
        """
        return len(self._facts)
    def __reduce__(self) -> tuple:
        """Pickle as owned facts, never as an implementation mappingproxy.

        Returns:
            The ``(type(self), (dict(self._facts),))`` reconstruction tuple.

        Raises:
            Nothing.

        No CLI equivalent: process-local pickling of an ``oxblend`` report.
        """
        # Serialize owned facts, not an implementation-specific mappingproxy.
        return type(self), (dict(self._facts),)
    def to_json(self) -> Any:
        """Render the exact CLI JSON value; operation diagnostics stay separate.

        Returns:
            The JSON-compatible value, matching the corresponding
            ``oxblend`` command stdout for the same operation.

        Raises:
            Nothing; encoding failures are raised eagerly at read time, not here.

        No CLI equivalent: produces the ``oxblend`` CLI JSON bytes for this
        report (``operation_diagnostics`` travels out of band).
        """
        return {k: _thaw(v) for k, v in self._facts.items() if k != 'operation_diagnostics'}
    @property
    def diagnostics(self) -> tuple[Record, ...]:
        """Operation diagnostics retained out of band from the JSON value.

        Returns:
            The ``operation_diagnostics`` rows, empty when the engine kept none.

        Raises:
            Nothing.

        No CLI equivalent: in-process view of the diagnostics the
        ``oxblend`` CLI delivers beside its JSON stdout.
        """
        return self._facts.get('operation_diagnostics', ())
    def raw(self, field: str) -> bytes | None:
        """Decode the engine's lossless FIELD_hex, never its optional text.

        Args:
            field: the base field name; the ``field + "_hex"`` fact is decoded.

        Returns:
            The exact saved bytes, or None when the engine reports null.

        Raises:
            KeyError: the report has no such ``*_hex`` field.
            ValueError: the stored hex is malformed (never for engine facts).

        No CLI equivalent: typed accessor over the ``*_hex`` fields of any
        ``oxblend`` JSON report (``oxblend get``, ``oxblend inspect`` and
        friends).
        """
        value = self._facts[field + "_hex"]
        return None if value is None else bytes.fromhex(value)
    def text(self, field: str) -> str | None:
        """Decode a ``raw`` field as strict UTF-8 text.

        Args:
            field: the base field name decoded through ``raw``.

        Returns:
            The decoded string, or None when the engine reports null.

        Raises:
            KeyError: the report has no such ``*_hex`` field.
            UnicodeDecodeError: the saved bytes are not valid UTF-8.

        No CLI equivalent: typed accessor over the ``*_hex`` fields of any
        ``oxblend`` JSON report.
        """
        value = self.raw(field)
        return None if value is None else value.decode("utf-8", errors="strict")

def _thaw(value: Any) -> Any:
    # Native JSON leaves are exact builtins: avoid millions of ABC Mapping
    # checks. Subclasses still follow the original container-first rules.
    if value is None or type(value) in (str, bytes, bool, int, float):
        return value
    if type(value) is Record:
        # Nested engine maps preserve every key, including a coincidental
        # operation_diagnostics key. Bypass Mapping.items' per-key callbacks.
        return {k: _thaw(v) for k, v in value._facts.items()}
    if isinstance(value, Record):
        # Preserve the existing Mapping behavior of caller-defined subclasses.
        return {k: _thaw(v) for k, v in value.items()}
    if isinstance(value, Mapping): return {k: _thaw(v) for k, v in value.items()}
    if isinstance(value, tuple): return [_thaw(v) for v in value]
    return value

def _freeze_native_owned(value: Any) -> Any:
    """Freeze exclusively owned native JSON without a second dictionary tree."""
    if value is None or type(value) in (str, bool, int, float):
        return value
    if type(value) is dict:
        return Record._from_native_owned(value)
    if type(value) is tuple:
        return tuple(_freeze_native_owned(item) for item in value)
    raise TypeError('native JSON must contain only scalars, dictionaries and tuples')

def _freeze(value: Any) -> Any:
    # Preserve defensive copying and subclass semantics while making the
    # overwhelmingly common immutable native scalar a constant fast path.
    if value is None or type(value) in (str, bytes, bool, int, float):
        return value
    if isinstance(value, Record):
        return value
    if isinstance(value, Mapping):
        return Record(value)
    if isinstance(value, (list, tuple)):
        return tuple(_freeze(v) for v in value)
    if isinstance(value, (bytearray, memoryview)):
        return bytes(value)
    if value is None or isinstance(value, (str, bytes, bool, int, float)):
        return value
    raise TypeError("record values must be immutable scalars, mappings or sequences")

class TraceReport(Record):
    """Typed ``oxblend trace`` report: coverage verdict plus usages/errors.

    ``complete`` is the only trustworthy-coverage signal; every other field
    stays available on partial reports so callers can inspect refusals.
    A multi-root selection returns ``MultiRootReport`` instead.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the trace fields still constructs;
            field access then raises ``KeyError``.

    Returns:
        TraceReport: the owned immutable mapping over the trace facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend trace`` JSON document.
    """
    __slots__ = ()
    @property
    def complete(self) -> bool:
        """Whether the walk established trustworthy full coverage.

        Returns:
            The engine ``complete`` verdict; strict callers refuse when False
            unless ``allow_incomplete`` was passed.

        Raises:
            KeyError: the facts omit ``complete``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``complete`` field in the
        ``oxblend trace`` JSON output.
        """
        return self["complete"]
    @property
    def usages(self) -> tuple[Record, ...]:
        """Proved dependency usages in engine order.

        Returns:
            The ``usages`` rows.

        Raises:
            KeyError: the facts omit ``usages``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``usages`` array in the
        ``oxblend trace`` JSON output.
        """
        return self["usages"]
    @property
    def errors(self) -> tuple[Record, ...]:
        """Refusal/error rows that made the walk incomplete, if any.

        Returns:
            The ``errors`` rows, empty on a complete walk.

        Raises:
            KeyError: the facts omit ``errors``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``errors`` array in the
        ``oxblend trace`` JSON output.
        """
        return self["errors"]
    @property
    def warnings(self) -> tuple[Record, ...]:
        """Non-fatal observations kept beside the coverage verdict.

        Returns:
            The ``warnings`` rows.

        Raises:
            KeyError: the facts omit ``warnings``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``warnings`` array in the
        ``oxblend trace`` JSON output.
        """
        return self["warnings"]
    @property
    def diagnostics(self) -> tuple[Record, ...]:
        """Operation diagnostics for this trace, out of band from JSON.

        Returns:
            The ``operation_diagnostics`` rows.

        Raises:
            KeyError: the facts omit ``operation_diagnostics``. Unlike the
                ``Record`` default view, this override indexes directly, so
                a report built by hand without the field refuses here
                instead of claiming a total.

        No CLI equivalent: in-process view of the diagnostics the
        ``oxblend trace`` run retains beside its JSON stdout.
        """
        return self["operation_diagnostics"]

class PackReport(Record):
    """Typed pack report: delivery verdict plus publication side effects.

    Also returned by ``pack_sequence``. Side-effect flags (directory,
    archive, durability) describe what publication actually performed, so a
    refused pack never claims outputs it did not write.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the pack fields still constructs;
            field access then raises ``KeyError``.

    Returns:
        PackReport: the owned immutable mapping over the pack facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend pack`` and
    ``oxblend pack-sequence`` JSON documents.
    """
    __slots__ = ()
    @property
    def complete(self) -> bool:
        """Whether packing delivered the complete dependency closure.

        Returns:
            The engine ``complete`` verdict.

        Raises:
            KeyError: the facts omit ``complete``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``complete`` field in the
        ``oxblend pack`` and ``oxblend pack-sequence`` JSON output.
        """
        return self["complete"]
    @property
    def files(self) -> tuple[Record, ...]:
        """Planned/published member rows in engine order.

        Returns:
            The ``files`` rows.

        Raises:
            KeyError: the facts omit ``files``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``files`` array in the
        ``oxblend pack`` JSON output.
        """
        return self["files"]
    @property
    def errors(self) -> tuple[Record, ...]:
        """Refusal/error rows that made publication incomplete, if any.

        Returns:
            The ``errors`` rows, empty on a complete pack.

        Raises:
            KeyError: the facts omit ``errors``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``errors`` array in the
        ``oxblend pack`` JSON output.
        """
        return self["errors"]
    @property
    def planning_complete(self) -> bool:
        """Whether dependency planning itself finished before publication.

        Returns:
            The engine ``planning_complete`` flag.

        Raises:
            KeyError: the facts omit ``planning_complete``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``planning_complete`` field in
        the ``oxblend pack`` JSON output.
        """
        return self["planning_complete"]
    @property
    def directory_created(self) -> bool:
        """Whether this call created the destination directory.

        Returns:
            True only when publication created it.

        Raises:
            KeyError: the facts omit ``directory_created``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``directory_created`` field in
        the ``oxblend pack`` JSON output.
        """
        return self["directory_created"]
    @property
    def archive_written(self) -> bool:
        """Whether this call wrote the output archive file.

        Returns:
            True only when publication wrote it.

        Raises:
            KeyError: the facts omit ``archive_written``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``archive_written`` field in
        the ``oxblend pack`` JSON output.
        """
        return self["archive_written"]
    @property
    def durability_completed(self) -> bool:
        """Whether the requested persistence synchronization completed.

        Returns:
            The engine ``durability_completed`` flag.

        Raises:
            KeyError: the facts omit ``durability_completed``.
                Engine-produced reports always carry it; a report built by
                hand without the field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``durability_completed`` field
        in the ``oxblend pack`` JSON output.
        """
        return self["durability_completed"]
    @property
    def diagnostics(self) -> tuple[Record, ...]:
        """Operation diagnostics for this pack, out of band from JSON.

        Returns:
            The ``operation_diagnostics`` rows.

        Raises:
            KeyError: the facts omit ``operation_diagnostics``. Unlike the
                ``Record`` default view, this override indexes directly, so
                a report built by hand without the field refuses here
                instead of claiming a total.

        No CLI equivalent: in-process view of the diagnostics the
        ``oxblend pack`` run retains beside its JSON stdout.
        """
        return self["operation_diagnostics"]

class QueryReport(Record):
    """Typed ``oxblend query`` report: saved rows plus coverage verdict.

    Partial outcomes preserve independent rows and fields; ``complete`` is
    False when any requested field was unavailable.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the query fields still constructs;
            field access then raises ``KeyError``.

    Returns:
        QueryReport: the owned immutable mapping over the query facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend query`` JSON document.
    """
    __slots__ = ()
    @property
    def complete(self) -> bool:
        """Whether every requested field resolved on every matching record.

        Returns:
            The requested-coverage ``complete`` verdict.

        Raises:
            KeyError: the facts omit ``requested_coverage``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the coverage verdict in the
        ``oxblend query`` JSON output.
        """
        return self["requested_coverage"]["complete"]
    @property
    def rows(self) -> tuple[Record, ...]:
        """Matching saved records in engine order, partial fields kept.

        Returns:
            The ``rows`` records.

        Raises:
            KeyError: the facts omit ``rows``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``rows`` array in the
        ``oxblend query`` JSON output.
        """
        return self["rows"]
    @property
    def errors(self) -> tuple[Record, ...]:
        """Unavailable-field outcomes that made coverage incomplete.

        Returns:
            The ``errors`` rows, empty on full coverage.

        Raises:
            KeyError: the facts omit ``errors``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``errors`` array in the
        ``oxblend query`` JSON output.
        """
        return self["errors"]

from .diagnostics import SourceContext, SourcePropagation, SourceDiagnostic
from .errors import *
from .errors import ERROR_TYPES
_ERROR_CATALOG = {row['kind']: row for row in _native.error_catalog()['entries']}

def _error(error: OxblendError) -> OxblendError:
    kind = getattr(error, 'kind', None)
    if kind is None:
        kind = next((k for k, cls in ERROR_TYPES.items() if type(error) is cls), 'usage')
    cls = ERROR_TYPES.get(kind, OxblendError)
    if not isinstance(error, cls):
        original = error
        error = cls(str(original))
        error.__dict__.update(original.__dict__)
    error.kind = kind
    error.message = getattr(error, 'message', str(error))
    error.exit_code = 3 if kind in ('pack_input', 'missing_input') else 5 if kind == 'pack_project' else 2
    error.diagnostics = _freeze(getattr(error, 'diagnostics', ()))
    error.diagnostic_ref = _freeze(getattr(error, 'diagnostic_ref', None))
    snapshot = getattr(error, 'source_snapshot', None)
    error.source_snapshot = None if snapshot is None else SourceDiagnostic(snapshot)
    error.operation = getattr(error, 'operation', '')
    error.effects = getattr(error, 'effects', 'none')
    error.report = getattr(error, 'report', None)
    error.context = getattr(error, 'context', ())
    info = _ERROR_CATALOG.get(kind, {})
    error.explanation = info.get('explanation')
    error.likely_causes = tuple(info.get('likely_causes', ()))
    error.next_steps = tuple(info.get('next_steps', ()))
    error.known_issue = info.get('known_issue')
    error.next_command = getattr(error, 'next_command', None)
    return error

def _suggest(error: OxblendError, command: str, source: HostPath | None) -> OxblendError:
    suggestion = _native.next_command(error.kind, command, None if source is None else os.fsdecode(os.fspath(source)))
    error.next_command = None if suggestion is None else _freeze(suggestion)
    return error

def _invoke(fn: Callable[..., Any], *args: Any, **kwargs: Any) -> Any:
    name = getattr(fn, '__name__', '')
    command = {'get_json': 'get', 'pointer_json': 'pointer', 'byte_array': 'get',
               'pointer_table': 'get', 'trace_path': 'trace', 'block_report': 'blocks',
               'list_dependencies': 'list', 'trace_many': 'trace', 'list_many': 'list',
               'report_preview': 'report', 'report_create': 'report', 'report_discard': 'report',
               'report_direct_preview': 'report'}.get(name, name.replace('_', '-'))
    bound = getattr(fn, '__self__', None)
    source = bound.source_spelling if isinstance(bound, _native.File) else None
    if not isinstance(bound, _native.File) and source is None and args and name in ('inspect', 'payload', 'source_facts', 'library_facts',
        'pointer_census', 'trace_path', 'list_dependencies', 'block_report', 'overrides',
        'diff', 'edit', 'node_tree', 'extract', 'rewrite', 'exr_header', 'exr_pixels'):
        source = args[0]
    try:
        return fn(*args, **kwargs)
    except OxblendError as error:
        error = _error(error)
        if getattr(error, 'report', None) is not None and isinstance(error.report, Mapping):
            report_type = {'pack': PackReport, 'pack_sequence': PackReport, 'trace': TraceReport,
                           'query': QueryReport, 'diff': globals().get('DiffReport', Record),
                           'edit': globals().get('EditReport', Record), 'rewrite': globals().get('EditReport', Record),
                           'extract': globals().get('ExtractReport', Record),
                           'list_dependencies': globals().get('ListReport', Record)}.get(error.operation, Record)
            if error.report.get('schema') == 'oxblend.multi-root/v1':
                report_type = globals().get('MultiRootReport', Record)
            error.report = report_type(error.report)
        elif error.operation == 'exr_scan' and getattr(error, 'report', None) is not None:
            error.report = ScanReport(error.report)
        raise _suggest(error, command, source) from None
    # R36: Python argument-conversion errors (an out-of-range or wrong-typed
    # argument rejected by PyO3 BEFORE the engine, e.g. blocks(address=-1 or
    # 2**64)) are OUTSIDE the canonical contract. They propagate as native
    # TypeError/OverflowError, idiomatic Python, without emulating CLI usage
    # prose or double-parsing the CLI. Values that PASS conversion but FAIL
    # engine validation still raise the canonical engine OxblendError
    # (kind/message/catalog) via the branch above. Coded-typed semantic
    # validation in Python (FieldQuery, _items, _spec_path) raises its own
    # typed errors before _invoke and never becomes a native TypeError merely
    # because both share ValueError.

def _admit(report: Any, error_type: type[OxblendError], operation: str, allow_incomplete: bool,
           source: HostPath | None = None, command: str | None = None) -> Any:
    # Only declared diagnostic sites, never arbitrary saved user-value objects.
    errors = list(report.get('errors', ())) if isinstance(report, Mapping) else []
    if isinstance(report, Mapping) and report.get('schema') == 'oxblend.multi-root/v1':
        for root in report.get('root_traces', ()):
            if isinstance(root.get('error'), Mapping): errors.append(root['error'])
            if isinstance(root.get('trace'), Mapping): errors.extend(root['trace'].get('errors', ()))
        for root in report.get('root_lists', ()):
            if isinstance(root.get('error'), Mapping): errors.append(root['error'])
        for root in report['cross_reference'].get('roots', ()):
            for observed in root.get('observation_errors', ()):
                errors.append(observed.get('error', observed))
    changed = next((e for e in errors if e.get('kind') == 'input_changed'), None)
    if changed is not None or (not report.complete and not allow_incomplete):
        if changed is not None:
            # The message is the engine report's own input_changed diagnosis;
            # a missing message fails loudly via the mapping lookup below,
            # never a synthesized fallback string.
            error = SourceChangedError(changed["message"])
            error.kind = "input_changed"
        else:
            # One coherent engine seam for every strict refusal: the
            # identical canonical kind/message the CLI delivery check emits,
            # over the serialized incomplete report. No Python prose, no
            # per-operation fallback strings, no second variant policy.
            input_display = None if source is None else os.fsdecode(os.fspath(source))
            kind, message = _invoke(
                _native.strict_refusal,
                operation, json.dumps(report.to_json()), command, input_display,
            )
            cls = ERROR_TYPES.get(kind)
            if cls is None:
                raise _error(UsageError(f"unknown strict refusal kind {kind!r}"))
            error = cls(message)
            error.kind = kind
        error.message = str(error)
        error.operation = operation
        error.context = ()
        error.diagnostic_ref = changed.get("diagnostic_ref") if changed is not None else None
        error.report = report
        error.diagnostics = report.get("operation_diagnostics", ())
        error.effects = "reported" if operation in ('pack', 'edit', 'rewrite', 'extract', 'exr_pixels', 'report_create', 'report_discard') else "none"
        raise _suggest(_error(error), command or operation, source)
    return report

HostPath: TypeAlias = str | bytes | os.PathLike[str] | os.PathLike[bytes]

def _items(values: Iterable[Any], label: str) -> tuple[Any, ...]:
    if isinstance(values, (str, bytes, bytearray, memoryview)):
        raise _error(UsageError(f'{label} must be an iterable of items, not a scalar string'))
    return tuple(values)

def _query(q: FieldQuery) -> tuple:
    if not isinstance(q, FieldQuery):
        raise TypeError("expected FieldQuery")
    return q.block, q.element, q.path, q.null_terminated

def _value(row: tuple, projection: Mapping[str, Any] | None = None) -> FieldValue:
    if projection is None:
        kind, value, projection = row
        projection = _freeze(projection)
    else:
        kind, value = row
    if kind == "float32":
        value = Float32Bits(value)
    elif kind == "array":
        value = tuple(_value(v, p) for v, p in zip(value, projection['items'], strict=True))
    return FieldValue(kind, value, projection)

class File:
    """One captured root set and shared library arena, serialized per handle.

    Build with ``open`` (one root) or ``open_many`` (ordered roots sharing
    one engine arena); the constructor itself takes the owned native
    ``handle`` and the optional ``source_path`` spelling captured for
    diagnostics. Methods never reopen the path: reads, trace, list and pack
    reuse the captured roots and retained libraries, and a changed source
    always raises ``SourceChangedError``. ``handle`` is the owned native
    session; ``source_path`` is the captured display spelling, None for
    multi-root sessions without a single source.

    Args:
        handle: the owned native session. Build with ``open`` (one root)
            or ``open_many`` (ordered roots sharing one engine arena),
            never by hand.
        source_path: the captured display spelling for diagnostics, None
            for multi-root sessions without a single source.

    Returns:
        File: the capturing session over the owned native handle.

    Raises:
        TypeError: the ``handle`` argument is missing. The constructor
            stores without validating; misuse surfaces on first use.

    No CLI equivalent: sessions are an in-process concept; each ``oxblend``
    CLI invocation opens its inputs implicitly for one command.
    """
    __slots__ = ("_handle", "_source_path")
    def __init__(self, handle: _native.File, source_path: HostPath | None = None) -> None:
        self._handle = handle
        self._source_path = source_path
    def __enter__(self) -> File:
        """Revalidate captured sources on context entry without reopening.

        Returns:
            This File, when every captured source still matches its snapshot.

        Raises:
            ClosedFileError: the session is already closed.
            SourceChangedError: a backing file changed since capture.

        No CLI equivalent: session lifetime is in-process; the ``oxblend``
        CLI holds its inputs for exactly one command.
        """
        # Native access checks closed/source state without silently reopening.
        _invoke(self._handle.validate_sources)
        return self
    def __exit__(self, exc_type, exc_value, traceback) -> None:
        """Close the session on context exit; never suppresses the exception.

        Args:
            exc_type: the in-flight exception type, if any.
            exc_value: the in-flight exception, if any.
            traceback: the in-flight traceback, if any.

        Returns:
            None; the original exception, if any, propagates unless closing raises.

        Raises:
            OxblendError: ``lock_poisoned`` if the native session mutex is poisoned.

        No CLI equivalent: session lifetime is in-process; the ``oxblend``
        CLI holds its inputs for exactly one command.
        """
        self.close()
    def close(self) -> None:
        """Release the native session; idempotent and never reopens.

        Returns:
            None.

        Raises:
            OxblendError: ``lock_poisoned`` if the native session mutex is poisoned.
            Closing an already-closed File otherwise succeeds.

        No CLI equivalent: session lifetime is in-process; the ``oxblend``
        CLI holds its inputs for exactly one command.
        """
        _invoke(self._handle.close)
    @property
    def closed(self) -> bool:
        """Whether the native session is released.

        Returns:
            True after ``close`` or context exit.

        Raises:
            OxblendError: ``lock_poisoned`` if the native session mutex is poisoned.

        No CLI equivalent: session state is in-process only.
        """
        return _invoke(lambda: self._handle.closed)
    @property
    def header(self) -> Header:
        """The typed container header of the captured input.

        Returns:
            The ``Header`` decoded from saved bytes.

        Raises:
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        No CLI equivalent: typed view of the ``header`` object in the
        ``oxblend summary`` JSON output.
        """
        return Header(*_invoke(self._handle.header))
    def blocks(self, *, code: bytes | None = None) -> tuple[BlockInfo, ...]:
        """List captured block tokens in physical order, optionally filtered.

        Args:
            code: four raw code bytes (including NUL padding) selecting one
                block family, or None for every block.

        Returns:
            The session-tagged ``BlockInfo`` tokens.

        Raises:
            TypeError: ``code`` is neither bytes nor None.
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend blocks``.
        """
        if code is not None and not isinstance(code, bytes):
            raise TypeError("code must be bytes or None")
        return _invoke(self._handle.blocks, code)
    def get(self, queries: Iterable[FieldQuery]) -> tuple[FieldValue, ...]:
        """Read addressed saved fields; foreign tokens refuse, values stay exact.

        Args:
            queries: one ``FieldQuery`` per value, in order.

        Returns:
            One ``FieldValue`` per query, in order.

        Raises:
            TypeError: a query is not a ``FieldQuery`` or a token is foreign.
            OxblendError: the engine's diagnosed failure (e.g. missing_field
                for an absent field, bounds for an out-of-range element).
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend get``.
        """
        return tuple(_value(row) for row in _invoke(self._handle.get, tuple(_query(q) for q in queries)))
    def pointer(self, query: FieldQuery) -> BlockInfo | None:
        """Resolve one saved pointer step to its target token, if any.

        Args:
            query: the addressed pointer field to resolve.

        Returns:
            The target ``BlockInfo``, or None for a null/inapplicable pointer.

        Raises:
            TypeError: the query is not a ``FieldQuery`` or the token is foreign.
            OxblendError: the engine's diagnosed failure for an unresolvable path.
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend pointer``.
        """
        return _invoke(self._handle.pointer, (_query(query),))
    def byte_array(self, query: FieldQuery, count: int) -> bytes:
        """Explicit counted one-byte pointer read, including saved NUL bytes.

        Args:
            query: the addressed byte-pointer field.
            count: exact bytes to read; from a saved count field, never guessed.

        Returns:
            Exactly ``count`` raw bytes.

        Raises:
            TypeError: the query is not a ``FieldQuery`` or the token is foreign.
            OxblendError: wrong depth/type, overflow, or an extent crossing an
                allocation boundary.
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend get --byte-array``.
        """
        return _invoke(self._handle.byte_array, (_query(query),), count)
    def pointer_table(self, query: FieldQuery, count: int) -> tuple[BlockInfo | None, ...]:
        """Counted source-local struct** slots; preserves order and nulls.

        Args:
            query: the addressed pointer-to-pointer field.
            count: exact slots to resolve; from a saved count field, never guessed.

        Returns:
            One target token or None per slot, in saved order.

        Raises:
            TypeError: the query is not a ``FieldQuery`` or the token is foreign.
            OxblendError: wrong depth/type, overflow, or bad targets.
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend get --pointer-table``.
        """
        return tuple(_invoke(self._handle.pointer_table, (_query(query),), count))
    @property
    def open_diagnostics(self) -> tuple[Record, ...]:
        """Capture-time diagnostics retained from opening this session.

        Returns:
            The owned open-diagnostic rows.

        Raises:
            ClosedFileError: the session is closed.

        No CLI equivalent: in-process view of the capture diagnostics the
        ``oxblend`` CLI folds into its per-command error envelope.
        """
        return _freeze(_invoke(lambda: self._handle.open_diagnostics))
    @property
    def root_paths(self) -> tuple[str, ...]:
        """Frozen lexical absolute identities captured together by this File.

        Returns:
            The decoded root path spellings in capture order.

        Raises:
            ClosedFileError: the session is closed.

        No CLI equivalent: session capture state is in-process only.
        """
        return tuple(os.fsdecode(p) for p in _invoke(lambda: self._handle.root_paths))
    @property
    def memory(self) -> str:
        """Effective decoded-source retention policy, fixed when opened.

        Returns:
            ``fast`` or ``bounded``, as selected at open time.

        Raises:
            ClosedFileError: the session is closed.

        No CLI equivalent: session lifetime policy is in-process only.
        """
        return _invoke(lambda: self._handle.memory)
    def trace(self, *, recursive: bool = True, low_memory: bool = False, allow_incomplete: bool = False) -> TraceReport | MultiRootReport:
        """Fresh strict walk over captured roots and retained linked files.

        Args:
            recursive: follow referenced libraries; False limits the walk to
                the captured roots.
            low_memory: trade IO/time for a lower decoded-source peak.
            allow_incomplete: return the partial report instead of raising
                ``IncompleteTraceError`` on incomplete coverage.

        Returns:
            The ``TraceReport`` (single root) or ``MultiRootReport``.

        Raises:
            IncompleteTraceError: incomplete coverage without ``allow_incomplete``.
            SourceChangedError: a backing file changed since capture.
            ClosedFileError: the session is closed.

        CLI: ``oxblend trace``.
        """
        data = _invoke(self._handle.trace, recursive, low_memory)
        report = MultiRootReport(data) if data.get('schema') == 'oxblend.multi-root/v1' else TraceReport(data)
        return _admit(report, IncompleteTraceError, "trace", allow_incomplete)
    def list_dependencies(self, *, full_info: bool = False, low_memory: bool = False,
                          allow_incomplete: bool = False) -> ListReport | MultiRootReport:
        """List the captured dependency closure without walking usage edges.

        Args:
            full_info: include the full per-member detail projection.
            low_memory: trade IO/time for a lower decoded-source peak.
            allow_incomplete: return the partial report instead of raising
                ``IncompleteTraceError`` on incomplete coverage.

        Returns:
            The ``ListReport`` (single root) or ``MultiRootReport``.

        Raises:
            IncompleteTraceError: incomplete coverage without ``allow_incomplete``.
            SourceChangedError: a backing file changed since capture.
            ClosedFileError: the session is closed.

        CLI: ``oxblend list``.
        """
        try:
            data = _invoke(self._handle.list_dependencies, full_info, low_memory)
        except OxblendError as error:
            if allow_incomplete and error.kind != 'input_changed' and isinstance(error.report, (ListReport, MultiRootReport)):
                return error.report
            raise
        report = MultiRootReport(data) if data.get('schema') == 'oxblend.multi-root/v1' else ListReport(data)
        return _admit(report, IncompleteTraceError, 'trace', allow_incomplete)

    def pack_sequence(self, destination: HostPath, *, project_root: HostPath | None = None,
                      exclude: Iterable[str] = (), dry_run: bool = False,
                      relative_only: bool = False, compress: bool = False,
                      keep_hierarchy: bool = True, permissive: bool = False,
                      durability: str = 'none', memory: str | None = None,
                      allow_incomplete: bool = False) -> PackReport:
        """Publish several captured roots together with deduplication.

        Args:
            destination: output directory or archive path.
            project_root: explicit project directory for relocation planning.
            exclude: member patterns excluded from the plan.
            dry_run: plan without writing any output.
            relative_only: keep only workspace-relative member paths.
            compress: write the staged output compressed.
            keep_hierarchy: preserve source hierarchy under the destination.
            permissive: publish available members and mark INCOMPLETE instead
                of refusing outright.
            durability: persistence synchronization (``none``, ``end`` or
                ``each-file``).
            memory: decoded-source policy (``fast`` or ``bounded``); None keeps
                the Session's opened policy, an explicit differing mode refuses.
            allow_incomplete: return the partial report instead of raising
                ``IncompletePackError`` on incomplete publication.

        Returns:
            The ``PackReport`` with deduplicated member rows.

        Raises:
            IncompletePackError: incomplete publication without ``allow_incomplete``
                or ``permissive``.
            SourceChangedError: a backing file changed since capture.
            ClosedFileError: the session is closed.

        CLI: ``oxblend pack-sequence``.
        """
        data = _invoke(self._handle.pack_sequence, os.fsdecode(os.fspath(destination)),
                       None if project_root is None else os.fsdecode(os.fspath(project_root)),
                       _items(exclude, 'exclude'), dry_run, relative_only, compress,
                       keep_hierarchy, permissive, durability, memory)
        return _admit(PackReport(data), IncompletePackError, 'pack', allow_incomplete)

    def pack(self, destination: HostPath, *, project_root: HostPath | None = None,
             dry_run: bool = False, permissive: bool = False, compress: bool = False,
             keep_hierarchy: bool = False, memory: str | None = None,
             allow_incomplete: bool = False, exclude: Iterable[str] = (),
             relative_only: bool = False, durability: str = 'none') -> PackReport:
        """Publish the captured closure; no retry/cleanup on refusal.

        Args:
            destination: output directory or archive path.
            project_root: explicit project directory for relocation planning.
            dry_run: plan without writing any output.
            permissive: publish available members and mark INCOMPLETE instead
                of refusing outright.
            compress: write the staged output compressed.
            keep_hierarchy: preserve source hierarchy under the destination.
            memory: decoded-source policy (``fast`` or ``bounded``); None keeps
                the Session's opened policy, an explicit differing mode refuses.
            allow_incomplete: return the partial report instead of raising
                ``IncompletePackError`` on incomplete publication.
            exclude: member patterns excluded from the plan.
            relative_only: keep only workspace-relative member paths.
            durability: persistence synchronization (``none``, ``end`` or
                ``each-file``).

        Returns:
            The ``PackReport``; incomplete publication is preserved in its
            ``report`` on the raised error when refusing.

        Raises:
            IncompletePackError: incomplete publication without ``allow_incomplete``
                or ``permissive``.
            SourceChangedError: a backing file changed since capture.
            ClosedFileError: the session is closed.

        CLI: ``oxblend pack``.
        """
        report = _invoke(self._handle.pack, os.fsdecode(os.fspath(destination)),
                         None if project_root is None else os.fsdecode(os.fspath(project_root)), dry_run=dry_run,
                         permissive=permissive, compress=compress, keep_hierarchy=keep_hierarchy,
                         memory=memory, exclude=_items(exclude, 'exclude'), relative_only=relative_only, durability=durability)
        return _admit(PackReport(report), IncompletePackError, "pack", allow_incomplete)
    def query(self, *, type: str, fields: Iterable[str], allow_incomplete: bool = False) -> QueryReport:
        """Read saved records of one type; saved controls, never evaluation.

        Args:
            type: the saved record type to match.
            fields: exact raw SDNA field names to project per record.
            allow_incomplete: return the partial report instead of raising
                ``IncompleteQueryError`` when fields are unavailable.

        Returns:
            The ``QueryReport``; partial rows are kept.

        Raises:
            IncompleteQueryError: unavailable fields without ``allow_incomplete``.
            TypeError: ``fields`` is a scalar string rather than an iterable.
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend query``.
        """
        if isinstance(fields, (str, bytes, bytearray, memoryview)):
            raise TypeError("fields must be an iterable of field-name strings, not a scalar")
        report = QueryReport(_invoke(self._handle.query, type, tuple(fields)))
        return _admit(report, IncompleteQueryError, "query", allow_incomplete)
    def explain(self, concept: str) -> ExplainReport:
        """Explain one catalog concept against this session's saved SDNA.

        Args:
            concept: the exact catalog concept name.

        Returns:
            The ``ExplainReport`` with the concept entry and SDNA presence.

        Raises:
            OxblendError: ``unknown_concept`` for an unrecognized name.
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend explain``.
        """
        return ExplainReport(_invoke(self._handle.explain, concept))
    def inspect(self) -> InspectReport:
        """Read the ordered blocks and the file's own SDNA schema.

        Returns:
            The ``InspectReport`` with header, blocks and SDNA facts.

        Raises:
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend inspect``.
        """
        return InspectReport._from_native_owned(_invoke(self._handle.inspect))
    def payload(self, block: BlockInfo | int) -> ValueReport:
        """Read one complete block payload as hex bytes.

        Args:
            block: a session-tagged ``BlockInfo`` token (validated, never a
                foreign token) or a physical block index.

        Returns:
            The ``ValueReport`` wrapping the payload bytes.

        Raises:
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend payload``.
        """
        if isinstance(block, BlockInfo):
            # Native inventory tokens are validated by a field query before
            # their numeric index is admitted; never accept foreign tokens.
            _invoke(self._handle.validate_block, block)
            block = block.index
        return ValueReport(_invoke(self._handle.payload, block))
    def source_facts(self, *, allow_incomplete: bool = False) -> FactsReport:
        """Read per-file source facts for the captured roots.

        Args:
            allow_incomplete: return the partial report instead of raising
                ``IncompleteFactsError`` when queries fail.

        Returns:
            The ``FactsReport`` for this session's sources.

        Raises:
            IncompleteFactsError: failed queries without ``allow_incomplete``.
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend source-facts``.
        """
        return _admit(FactsReport(_invoke(self._handle.source_facts)), IncompleteFactsError, 'facts', allow_incomplete, self._source_path, 'source-facts')
    def library_facts(self, *, allow_incomplete: bool = False) -> FactsReport:
        """Read per-file facts for the libraries this session retains.

        Args:
            allow_incomplete: return the partial report instead of raising
                ``IncompleteFactsError`` when queries fail.

        Returns:
            The ``FactsReport`` for the retained libraries.

        Raises:
            IncompleteFactsError: failed queries without ``allow_incomplete``.
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend library-facts``.
        """
        return _admit(FactsReport(_invoke(self._handle.library_facts)), IncompleteFactsError, 'facts', allow_incomplete, self._source_path, 'library-facts')
    def pointer_census(self, *, full: bool = False) -> CensusReport:
        """Count whole-file pointer states; semantic reconstruction is separate.

        Args:
            full: include the full per-pointer detail projection.

        Returns:
            The ``CensusReport`` with the semantic-verified verdict.

        Raises:
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend pointer-census``.
        """
        return CensusReport(_invoke(self._handle.pointer_census, full))
    def summary(self) -> Summary:
        """Summarize the captured input: header, census, scenes and warnings.

        Returns:
            The typed ``Summary``.

        Raises:
            ClosedFileError: the session is closed.
            SourceChangedError: a backing file changed since capture.

        CLI: ``oxblend summary``.
        """
        row = _invoke(self._handle.summary)
        row["header"] = Header(*row["header"])
        row["scenes"] = tuple(SceneSummary(s[0], s[1], s[2], s[3], s[4], Fps(s[5], None if s[6] is None else Float32Bits(s[6]), s[7])) for s in row["scenes"])
        row["warnings"] = tuple(Diagnostic(*w) for w in row["warnings"])
        if '_json' in row: row['_json'] = _freeze(row['_json'])
        if '_json_error' in row: row['_json_error'] = _error(row['_json_error'])
        return Summary(**row)

def open(path: HostPath, *, memory: str = "fast") -> File:
    """Open one root once and capture it into a session.

    Args:
        path: the saved blend file; native bytes paths and os.PathLike are
            supported on Linux.
        memory: decoded-source retention policy, ``fast`` (default, unchanged)
            or ``bounded`` (lower peak at more IO/time), fixed for the
            Session lifetime.

    Returns:
        The capturing ``File`` session; use it as a context manager.

    Raises:
        OxblendError: the engine's diagnosed open failure (e.g. missing_input,
            invalid_header) with kind/message/catalog.

    No CLI equivalent: sessions are an in-process concept; each ``oxblend``
    CLI command (``oxblend summary``, ``oxblend trace`` and friends) opens
    its inputs implicitly for that one command.
    """
    # os.fsdecode preserves Unix path bytes with surrogateescape. PyO3's
    # PathBuf extractor re-encodes through Python's filesystem conversion.
    return File(_invoke(_native.open, os.fsdecode(os.fspath(path)), memory=memory), path)

def open_many(paths: Iterable[HostPath], *, memory: str = "fast") -> File:
    """Capture ordered roots together in one engine-owned library arena.

    Args:
        paths: saved blend files captured together, in order; shared roots
            and libraries deduplicate inside that single store.
        memory: decoded-source retention policy, ``fast`` (default, unchanged)
            or ``bounded`` (lower peak at more IO/time), fixed for the
            Session lifetime.

    Returns:
        The capturing multi-root ``File`` session.

    Raises:
        UsageError: ``paths`` is a File, or mixes captured Files with paths;
            separately opened Files can never be combined.
        OxblendError: the engine's diagnosed open failure with kind/message.

    No CLI equivalent: multi-root capture is in-process; the ``oxblend`` CLI
    commands ``oxblend trace``, ``oxblend list`` and ``oxblend pack-sequence``
    accept several inputs per invocation instead.
    """
    if isinstance(paths, File):
        raise _error(UsageError('open_many takes paths, not a File; pass the captured File directly to the operation'))
    values = _items(paths, 'paths')
    if any(isinstance(path, File) for path in values):
        raise _error(UsageError('Captured Files cannot be merged; use open_many(paths) with paths only'))
    selected = tuple(os.fsdecode(os.fspath(path)) for path in values)
    return File(_invoke(_native.open_many, selected, memory=memory), values[0] if len(values) == 1 else None)

__all__ = ["open", "open_many", "File", "Header", "BlockInfo", "FieldQuery", "FieldValue",
           "Float32Bits", "Summary", "SceneSummary", "Fps", "Diagnostic",
           "OxblendError", "ClosedFileError", "SourceChangedError", "engine_version", "__version__",
           "Record", "TraceReport", "PackReport", "QueryReport", "trace", "catalog",
           "IncompleteTraceError", "IncompletePackError", "IncompleteQueryError"]

from .operations import *
from .operations import __all__ as _operation_exports
from .errors import __all__ as _error_exports
__all__ = list(dict.fromkeys(__all__ + _operation_exports + _error_exports +
                            ['SourceContext', 'SourcePropagation', 'SourceDiagnostic']))
