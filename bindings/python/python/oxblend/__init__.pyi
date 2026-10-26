"""Typed in-process saved-file operations; no implicit pointers or Blender evaluation.

Public type surface for ``oxblend``. Runtime classes live in ``__init__.py``,
``operations.py`` and ``errors.py``; this stub names every public class,
function, attribute and report accessor. Report objects are immutable
:class:`Record` mappings, so additive engine fields remain reachable by key even
though they are not named here. There is deliberately no ``__getattr__``
catch-all: an unknown attribute is a type error, an unknown *field* is a
``Mapping`` lookup.
"""

from __future__ import annotations

import os
from types import TracebackType
from typing import Any, Iterable, Iterator, Mapping, Sequence, TypeAlias, final

from . import _native
from ._native import (
    BlockInfo as BlockInfo,
    ClosedFileError as ClosedFileError,
    OxblendError as OxblendError,
    SourceChangedError as SourceChangedError,
)

from .diagnostics import (SourceContext as SourceContext, SourcePropagation as SourcePropagation,
                          SourceDiagnostic as SourceDiagnostic)

__version__: str
engine_version: str
__all__: list[str]

HostPath: TypeAlias = str | bytes | os.PathLike[str] | os.PathLike[bytes]

@final
class Header:
    version: int
    pointer_size: int
    little_endian: bool
    file_format: int
    size: int
    def __init__(
        self,
        version: int,
        pointer_size: int,
        little_endian: bool,
        file_format: int,
        size: int,
    ) -> None: ...

@final
class Float32Bits:
    """Saved float bits, so a NaN payload is never silently normalized."""

    bits: int
    def __init__(self, bits: int) -> None: ...
    @property
    def value(self) -> float: ...

@final
class FieldValue:
    """One typed saved value plus its exact ``schema``/``value`` projection."""

    kind: str
    value: int | bytes | Float32Bits | tuple[FieldValue, ...]
    def __init__(
        self,
        kind: str,
        value: int | bytes | Float32Bits | tuple[FieldValue, ...],
        _json: Mapping[str, Any] = ...,
    ) -> None: ...
    def to_json(self) -> JSONValue: ...
    @property
    def text(self) -> str: ...

@final
class FieldQuery:
    block: BlockInfo
    element: int
    path: tuple[bytes | int, ...]
    null_terminated: bool
    def __init__(
        self,
        block: BlockInfo,
        element: int,
        path: Sequence[bytes | int],
        null_terminated: bool = True,
    ) -> None: ...

@final
class Diagnostic:
    kind: str
    block_index: int
    element: int
    field: str
    message: str
    def __init__(
        self, kind: str, block_index: int, element: int, field: str, message: str
    ) -> None: ...

@final
class Fps:
    numerator: int | None
    base: Float32Bits | None
    effective: float | None
    def __init__(
        self, numerator: int | None, base: Float32Bits | None, effective: float | None
    ) -> None: ...

@final
class SceneSummary:
    name: bytes | None
    frame_start: int | None
    frame_end: int | None
    frame_step: int | None
    camera: bytes | None
    fps: Fps
    def __init__(
        self,
        name: bytes | None,
        frame_start: int | None,
        frame_end: int | None,
        frame_step: int | None,
        camera: bytes | None,
        fps: Fps,
    ) -> None: ...

@final
class Summary:
    """Typed saved summary. ``to_json`` is the exact CLI projection.

    Byte-only (non-UTF-8) names still produce a typed Summary; ``to_json`` then
    raises the engine ``encoding`` error instead of inventing text.
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
    def __init__(
        self,
        header: Header,
        block_codes: tuple[tuple[bytes, int], ...],
        objects: tuple[bytes, ...],
        scenes: tuple[SceneSummary, ...],
        materials: tuple[bytes, ...],
        meshes: tuple[bytes, ...],
        images: tuple[bytes, ...],
        collections: tuple[bytes, ...],
        warnings: tuple[Diagnostic, ...],
        _json: Mapping[str, Any] = ...,
        _json_error: OxblendError | None = ...,
    ) -> None: ...
    def to_json(self) -> JSONValue: ...

class Record(Mapping[str, Any]):
    """Owned immutable engine facts; unknown fields and exact hex bytes survive."""

    def __init__(self, _facts: Mapping[str, Any]) -> None: ...
    def __getitem__(self, key: str) -> Any: ...
    def __iter__(self) -> Iterator[str]: ...
    def __len__(self) -> int: ...
    def to_json(self) -> JSONValue: ...
    @property
    def diagnostics(self) -> tuple[Record, ...]: ...
    def raw(self, field: str) -> bytes | None: ...
    def text(self, field: str) -> str | None: ...

class TraceReport(Record):
    @property
    def complete(self) -> bool: ...
    @property
    def usages(self) -> tuple[Record, ...]: ...
    @property
    def errors(self) -> tuple[Record, ...]: ...
    @property
    def warnings(self) -> tuple[Record, ...]: ...
    @property
    def diagnostics(self) -> tuple[Record, ...]: ...

class PackReport(Record):
    @property
    def complete(self) -> bool: ...
    @property
    def files(self) -> tuple[Record, ...]: ...
    @property
    def errors(self) -> tuple[Record, ...]: ...
    @property
    def planning_complete(self) -> bool: ...
    @property
    def directory_created(self) -> bool: ...
    @property
    def archive_written(self) -> bool: ...
    @property
    def durability_completed(self) -> bool: ...
    @property
    def diagnostics(self) -> tuple[Record, ...]: ...

class QueryReport(Record):
    @property
    def complete(self) -> bool: ...
    @property
    def rows(self) -> tuple[Record, ...]: ...
    @property
    def errors(self) -> tuple[Record, ...]: ...

from .operations import (  # noqa: E402 - public re-exports of the operation facade
    JSONValue as JSONValue,
    Memory as Memory,
    Durability as Durability,
    Roots as Roots,
    ReportInput as ReportInput,
    CatalogReport as CatalogReport,
    ExplainReport as ExplainReport,
    InspectReport as InspectReport,
    ValueReport as ValueReport,
    PointerReport as PointerReport,
    FactsReport as FactsReport,
    CensusReport as CensusReport,
    ListReport as ListReport,
    MultiRootReport as MultiRootReport,
    OverrideReport as OverrideReport,
    DiffReport as DiffReport,
    EditReport as EditReport,
    ExtractReport as ExtractReport,
    NodeTreeReport as NodeTreeReport,
    ExrHeaderReport as ExrHeaderReport,
    PixelsReport as PixelsReport,
    TextReport as TextReport,
    ScanReport as ScanReport,
    ReportPreview as ReportPreview,
    ReportCreated as ReportCreated,
    ReportDiscarded as ReportDiscarded,
    summary as summary,
    catalog as catalog,
    query as query,
    explain as explain,
    inspect as inspect,
    payload as payload,
    get as get,
    pointer as pointer,
    source_facts as source_facts,
    library_facts as library_facts,
    pointer_census as pointer_census,
    list_dependencies as list_dependencies,
    trace as trace,
    block_report as block_report,
    overrides as overrides,
    diff as diff,
    edit as edit,
    node_tree as node_tree,
    extract as extract,
    rewrite as rewrite,
    pack as pack,
    pack_sequence as pack_sequence,
    exr_header as exr_header,
    exr_scan as exr_scan,
    exr_pixels as exr_pixels,
    read_path_list as read_path_list,
    report as report,
    report_preview as report_preview,
    report_create as report_create,
    report_discard as report_discard,
)

from .errors import (  # exact stable error kinds, generated from the catalog
    ERROR_TYPES as ERROR_TYPES,
    AmbiguousIdError as AmbiguousIdError, AmbiguousPointerError as AmbiguousPointerError, BoundsError as BoundsError, OxblendBrokenPipeError as OxblendBrokenPipeError,
    ClosureOwnerError as ClosureOwnerError, CompressionError as CompressionError, ConflictingEditsError as ConflictingEditsError, ConstructorTypeError as ConstructorTypeError,
    CryptoRetargetError as CryptoRetargetError, CryptoSceneKeptEdgeError as CryptoSceneKeptEdgeError, CryptoSourceIdError as CryptoSourceIdError, CstringWidthError as CstringWidthError,
    CycleError as CycleError, DanglingPointerError as DanglingPointerError, DeclaredDiffError as DeclaredDiffError, DecodedProofError as DecodedProofError,
    DiffExtentError as DiffExtentError, DiffIdentityError as DiffIdentityError, DiffPointerArrayError as DiffPointerArrayError, DuplicateDataAddressError as DuplicateDataAddressError,
    DurabilityError as DurabilityError, EditConflictError as EditConflictError, EditSpecError as EditSpecError, EditVersionError as EditVersionError,
    EmptyClearError as EmptyClearError, EncodingError as EncodingError, ExpectedOldError as ExpectedOldError, ExpectedPathError as ExpectedPathError,
    ExrDecodeError as ExrDecodeError, ExrHeaderError as ExrHeaderError, ExrHeaderTruncatedError as ExrHeaderTruncatedError, ExrSelectionError as ExrSelectionError,
    ExrUnsupportedError as ExrUnsupportedError, ExtractRefusalError as ExtractRefusalError, FieldOwnerError as FieldOwnerError, FieldTypeError as FieldTypeError,
    FileOpsProfileError as FileOpsProfileError, FlagMaskError as FlagMaskError, IdentifierOverflowError as IdentifierOverflowError, IncompleteDiffError as IncompleteDiffError,
    IncompleteEditError as IncompleteEditError, IncompleteExrScanError as IncompleteExrScanError, IncompleteFactsError as IncompleteFactsError, IncompleteOverridesError as IncompleteOverridesError,
    IncompletePackError as IncompletePackError, IncompleteQueryError as IncompleteQueryError, IncompleteTraceError as IncompleteTraceError, InvalidBlockError as InvalidBlockError,
    InvalidGraphError as InvalidGraphError, InvalidHeaderError as InvalidHeaderError, InvalidLayoutError as InvalidLayoutError, InvalidSavedStructureError as InvalidSavedStructureError,
    InvalidSdnaError as InvalidSdnaError, IoError as IoError, JsonError as JsonError, KitDnaError as KitDnaError,
    LibraryReadError as LibraryReadError, LibraryReaderError as LibraryReaderError, LinkOwnerError as LinkOwnerError, LinkProfileError as LinkProfileError,
    LinkedReferenceOnlyError as LinkedReferenceOnlyError, ListBacklinkError as ListBacklinkError, ListBoundaryError as ListBoundaryError, ListCycleError as ListCycleError,
    ListEndpointError as ListEndpointError, ListEndpointsError as ListEndpointsError, ListHeaderError as ListHeaderError, ListIntegrityError as ListIntegrityError,
    ListReciprocityError as ListReciprocityError, ListSymmetryError as ListSymmetryError, MissingAddressError as MissingAddressError, MissingAssetError as MissingAssetError,
    MissingFieldError as MissingFieldError, MissingIdError as MissingIdError, MissingInputError as MissingInputError, MissingLibraryError as MissingLibraryError,
    NameCollisionError as NameCollisionError, NodeConstructorError as NodeConstructorError, NodeCycleError as NodeCycleError, NodeEndpointError as NodeEndpointError,
    NodeExpectedGraphError as NodeExpectedGraphError, NodeGraphError as NodeGraphError, NodeOccupiedError as NodeOccupiedError, NodeOwnerError as NodeOwnerError,
    NodeSocketTypeError as NodeSocketTypeError, NodeVersionError as NodeVersionError, OutputCompressionError as OutputCompressionError, OutputDecodeLengthError as OutputDecodeLengthError,
    OutputExistsError as OutputExistsError, OutputPathError as OutputPathError, OxblendOverflowError as OxblendOverflowError, OwnerHeaderError as OwnerHeaderError,
    OwnerProjectionError as OwnerProjectionError, PackCollisionError as PackCollisionError, PackDestinationError as PackDestinationError, PackInputError as PackInputError,
    PackOriginError as PackOriginError, PackPathError as PackPathError, PackProjectError as PackProjectError, PaddingWriteError as PaddingWriteError,
    PathError as PathError, PathContextError as PathContextError, PathNulError as PathNulError, PathOverflowError as PathOverflowError,
    PathResolutionUnverifiedError as PathResolutionUnverifiedError, PathTerminationError as PathTerminationError, PatternMembersUnknownError as PatternMembersUnknownError, PointerCountError as PointerCountError,
    PointerDepthError as PointerDepthError, PointerExtentError as PointerExtentError, PointerPathError as PointerPathError, PointerProofError as PointerProofError,
    PointerRoleError as PointerRoleError, PointerTargetTypeError as PointerTargetTypeError, PointerWriteError as PointerWriteError, ProfileDiscriminatorError as ProfileDiscriminatorError,
    ProfileFieldError as ProfileFieldError, ProfileLibraryError as ProfileLibraryError, ProfileLinkedError as ProfileLinkedError, ProfileMaskError as ProfileMaskError,
    ProfileOwnerError as ProfileOwnerError, ProfileTemporalError as ProfileTemporalError, PropertyCollisionError as PropertyCollisionError, PropertyOwnerError as PropertyOwnerError,
    PropertySelectionError as PropertySelectionError, PropertyTypeError as PropertyTypeError, QueryCycleError as QueryCycleError, QueryListError as QueryListError,
    RawProfileError as RawProfileError, RecordExtentError as RecordExtentError, RequiredPointerNullError as RequiredPointerNullError, RetainedRemovedError as RetainedRemovedError,
    RlayersSourceIdError as RlayersSourceIdError, SameInputError as SameInputError, SeekTableError as SeekTableError, SerializationError as SerializationError,
    Sha256WidthError as Sha256WidthError, SnapshotMismatchError as SnapshotMismatchError, SocketConstructorError as SocketConstructorError, SocketOwnerError as SocketOwnerError,
    OxblendSourceChangedError as OxblendSourceChangedError, SpillSpaceError as SpillSpaceError, StorageProfileError as StorageProfileError, StrayDataRunError as StrayDataRunError,
    StringNulError as StringNulError, StringTerminationError as StringTerminationError, TextConstructorError as TextConstructorError, TruncatedError as TruncatedError,
    OxblendTypeError as OxblendTypeError, TypedKindError as TypedKindError, TypedOverflowError as TypedOverflowError, TypedWidthError as TypedWidthError,
    UnknownConceptError as UnknownConceptError, UnprofiledRoleError as UnprofiledRoleError, UnresolvedPersistentError as UnresolvedPersistentError, UnsupportedEditError as UnsupportedEditError,
    UnsupportedFormatError as UnsupportedFormatError, UnsupportedTypeError as UnsupportedTypeError, UnverifiedSemanticsError as UnverifiedSemanticsError, UsageError as UsageError,
    ZipEncodingError as ZipEncodingError, ZipPathError as ZipPathError, ZipWidthError as ZipWidthError,
)


class File:
    """One captured Session and its root set, serialized per handle.

    Construct with :func:`open` or :func:`open_many`. Methods never reopen the
    path: reads, trace, list and pack reuse the captured roots and retained
    libraries, and a changed source always raises ``SourceChangedError``.
    """

    def __init__(
        self, handle: _native.File, source_path: HostPath | None = None
    ) -> None: ...
    def __enter__(self) -> File: ...
    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_value: BaseException | None,
        traceback: TracebackType | None,
    ) -> None: ...
    def close(self) -> None: ...
    @property
    def closed(self) -> bool: ...
    @property
    def header(self) -> Header: ...
    def blocks(self, *, code: bytes | None = None) -> tuple[BlockInfo, ...]: ...
    def get(self, queries: Iterable[FieldQuery]) -> tuple[FieldValue, ...]: ...
    def pointer(self, query: FieldQuery) -> BlockInfo | None: ...
    def byte_array(self, query: FieldQuery, count: int) -> bytes: ...
    def pointer_table(
        self, query: FieldQuery, count: int
    ) -> tuple[BlockInfo | None, ...]: ...
    @property
    def open_diagnostics(self) -> tuple[Record, ...]: ...
    @property
    def root_paths(self) -> tuple[str, ...]: ...
    @property
    def memory(self) -> Memory: ...
    def summary(self) -> Summary: ...
    def query(
        self, *, type: str, fields: Iterable[str], allow_incomplete: bool = False
    ) -> QueryReport: ...
    def explain(self, concept: str) -> ExplainReport: ...
    def inspect(self) -> InspectReport: ...
    def payload(self, block: BlockInfo | int) -> ValueReport: ...
    def source_facts(self, *, allow_incomplete: bool = False) -> FactsReport: ...
    def library_facts(self, *, allow_incomplete: bool = False) -> FactsReport: ...
    def pointer_census(self, *, full: bool = False) -> CensusReport: ...
    def trace(
        self,
        *,
        recursive: bool = True,
        low_memory: bool = False,
        allow_incomplete: bool = False,
    ) -> TraceReport | MultiRootReport: ...
    def list_dependencies(
        self,
        *,
        full_info: bool = False,
        low_memory: bool = False,
        allow_incomplete: bool = False,
    ) -> ListReport | MultiRootReport: ...
    def pack(
        self,
        destination: HostPath,
        *,
        project_root: HostPath | None = None,
        exclude: Iterable[str] = (),
        dry_run: bool = False,
        relative_only: bool = False,
        compress: bool = False,
        keep_hierarchy: bool = False,
        permissive: bool = False,
        durability: Durability = "none",
        memory: Memory | None = None,
        allow_incomplete: bool = False,
    ) -> PackReport: ...
    def pack_sequence(
        self,
        output: HostPath,
        *,
        project_root: HostPath | None = None,
        exclude: Iterable[str] = (),
        dry_run: bool = False,
        relative_only: bool = False,
        compress: bool = False,
        keep_hierarchy: bool = True,
        permissive: bool = False,
        durability: Durability = "none",
        memory: Memory | None = None,
        allow_incomplete: bool = False,
    ) -> PackReport: ...

def open(path: HostPath, *, memory: Memory = "fast") -> File:
    """Open one root once and capture it."""
    ...

def open_many(roots: Iterable[HostPath], *, memory: Memory = "fast") -> File:
    """Capture every root in one Session/InputStore (no arena merge).

    Shared roots and libraries deduplicate inside that single store. Multi-root
    operations (trace, list_dependencies, pack_sequence) run over the retained
    root set. Separately opened Files cannot be combined.
    """
    ...
