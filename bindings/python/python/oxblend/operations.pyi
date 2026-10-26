"""Typed full-operation facade over the shared native engine, never a CLI wrapper.

This stub declares the same module surface as ``operations.py``. Every report is
an immutable :class:`oxblend.Record`: the typed properties below name the
documented fields, while every other engine field (and any additive field a
newer engine emits) stays reachable through the ``Mapping`` interface. No report
class has a generic attribute catch-all.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass
from typing import Any, Literal, TypeAlias

from . import (
    File,
    HostPath,
    OxblendError,
    PackReport,
    QueryReport,
    Record,
    Summary,
    TraceReport,
)

JSONValue: TypeAlias = (
    None | bool | int | float | str | list["JSONValue"] | dict[str, "JSONValue"]
)
Memory: TypeAlias = Literal["fast", "bounded"]
Durability: TypeAlias = Literal["none", "end", "each-file"]
# All paths captured together, or exactly one File (including `[file]`). Any
# other File mix refuses with UsageError naming open_many(paths).
Roots: TypeAlias = HostPath | Iterable[HostPath | File] | File

class CatalogReport(Record):
    """``oxblend.catalog/1``: concepts, declared read operations and limits."""

    @property
    def catalog_revision(self) -> str: ...
    @property
    def concepts(self) -> tuple[Record, ...]: ...
    @property
    def read_operations(self) -> tuple[Record, ...]: ...
    @property
    def raw_projection(self) -> Record: ...
    @property
    def limits(self) -> tuple[str, ...]: ...

class ExplainReport(Record):
    """``oxblend.explain/1``: concept entry, SDNA presence and next actions."""

class InspectReport(Record):
    @property
    def header(self) -> Record: ...
    @property
    def blocks(self) -> tuple[Record, ...]: ...
    @property
    def sdna(self) -> Record: ...

class ValueReport(Record):
    """``get``/``payload`` projection: ``{"schema": 1, "value": {...}}``."""

    @property
    def value(self) -> Record: ...

class PointerReport(Record):
    """``{"schema": 1, "block_index": int | None}`` in this reader's namespace."""

    @property
    def block_index(self) -> int | None: ...

class FactsReport(Record):
    @property
    def complete(self) -> bool: ...
    @property
    def facts(self) -> Record: ...
    @property
    def errors(self) -> tuple[Record, ...]: ...

class CensusReport(Record):
    @property
    def semantic_verified(self) -> bool: ...

class MultiRootReport(Record):
    """``oxblend.multi-root/v1`` with per-root rows and a cross-reference.

    Returned by ``trace``/``list_dependencies`` for a genuinely multi-root
    selection; a single-root call returns ``TraceReport``/``ListReport``.
    """

    @property
    def complete(self) -> bool: ...
    @property
    def operation(self) -> str: ...
    @property
    def root_traces(self) -> tuple[Record, ...]: ...
    @property
    def root_lists(self) -> tuple[Record, ...]: ...
    @property
    def cross_reference(self) -> Record: ...

class ListReport(Record):
    @property
    def complete(self) -> bool: ...

class OverrideReport(Record):
    @property
    def complete(self) -> bool: ...
    @property
    def errors(self) -> tuple[Record, ...]: ...
    @property
    def warnings(self) -> tuple[Record, ...]: ...

class DiffReport(Record):
    @property
    def complete(self) -> bool: ...
    @property
    def equal(self) -> bool | None: ...
    @property
    def differences(self) -> tuple[Record, ...]: ...
    @property
    def difference_count(self) -> int: ...
    @property
    def errors(self) -> tuple[Record, ...]: ...

class EditReport(Record):
    @property
    def complete(self) -> bool: ...
    @property
    def output_written(self) -> bool: ...
    @property
    def durability_completed(self) -> bool: ...
    @property
    def errors(self) -> tuple[Record, ...]: ...

class ExtractReport(EditReport):
    @property
    def selected_ids(self) -> tuple[Record, ...]: ...
    @property
    def refusals(self) -> tuple[Record, ...]: ...

class NodeTreeReport(Record):
    @property
    def trees(self) -> tuple[Record, ...]: ...

class ExrHeaderReport(Record):
    @property
    def parts(self) -> tuple[Record, ...]: ...
    @property
    def pixel_integrity_verified(self) -> bool: ...

class PixelsReport(EditReport):
    @property
    def channels(self) -> tuple[Any, ...]: ...
    @property
    def decode_complete(self) -> bool: ...

@dataclass(frozen=True, slots=True)
class TextReport:
    """CLI text diagnostics; ``blocks`` has no JSON CLI schema to equal."""

    text: str
    def to_json(self) -> str: ...
    def __str__(self) -> str: ...

@dataclass(frozen=True, slots=True, init=False)
class ScanReport:
    """The complete EXR JSONL sequence, summary row included.

    ``records`` is the CLI JSONL order (file rows in arrival order, summary row
    last), so ``summary`` is that last row and ``complete`` is its own
    ``complete``. Native ``exr_scan`` returns this one sequence for both success
    and failure; nothing is reconstructed or re-ordered here.
    """

    records: tuple[Record, ...]
    def __init__(self, records: Iterable[Mapping[str, Any]]) -> None: ...
    @property
    def summary(self) -> Record: ...
    @property
    def complete(self) -> bool: ...
    def to_json(self) -> list[JSONValue]: ...

class ReportPreview(Record): ...
class ReportCreated(Record): ...
class ReportDiscarded(Record): ...
ReportInput: TypeAlias = HostPath | OxblendError | Record | Summary | ScanReport

def summary(path: HostPath | File) -> Summary: ...
def catalog() -> CatalogReport: ...
def query(
    path: HostPath | File,
    *,
    type: str,
    fields: Iterable[str],
    allow_incomplete: bool = False,
) -> QueryReport: ...
def explain(
    concept: str, *, file: HostPath | File | None = None
) -> ExplainReport: ...
def inspect(path: HostPath | File) -> InspectReport: ...
def payload(path: HostPath | File, block: int) -> ValueReport: ...
def get(
    path: HostPath,
    block: int,
    path_steps: Sequence[str | int],
    element: int = 0,
    *,
    raw: bool = False,
    byte_array: int | None = None,
    pointer_table: int | None = None,
) -> ValueReport: ...
def pointer(
    path: HostPath,
    block: int,
    path_steps: Sequence[str | int],
    element: int = 0,
) -> PointerReport: ...
def source_facts(
    path: HostPath | File, *, allow_incomplete: bool = False
) -> FactsReport: ...
def library_facts(
    path: HostPath | File, *, allow_incomplete: bool = False
) -> FactsReport: ...
def pointer_census(
    path: HostPath | File, *, full: bool = False
) -> CensusReport: ...
def list_dependencies(
    roots: Roots,
    *,
    full_info: bool = False,
    low_memory: bool = False,
    allow_incomplete: bool = False,
) -> ListReport | MultiRootReport: ...
def trace(
    roots: Roots,
    *,
    recursive: bool = True,
    low_memory: bool = False,
    allow_incomplete: bool = False,
) -> TraceReport | MultiRootReport: ...
def block_report(
    path: HostPath,
    *,
    limit: int = 10,
    address: int | None = None,
    dump: bool = False,
) -> TextReport: ...
def overrides(
    path: HostPath, *, recursive: bool = True, allow_incomplete: bool = False
) -> OverrideReport: ...
def diff(
    a: HostPath, b: HostPath, *, allow_incomplete: bool = False
) -> DiffReport: ...
def edit(
    input: HostPath,
    output: HostPath,
    spec: Mapping[str, JSONValue],
    *,
    mode: Literal["format-only", "comp", "node-tree"],
    durability: Durability = "none",
    allow_incomplete: bool = False,
) -> EditReport: ...
def node_tree(path: HostPath) -> NodeTreeReport: ...
def extract(
    input: HostPath,
    output: HostPath,
    *,
    ids: Iterable[str] = (),
    blocks: Iterable[int] = (),
    clear: Iterable[str] = (),
    durability: Durability = "none",
    allow_incomplete: bool = False,
) -> ExtractReport: ...
def rewrite(
    input: HostPath,
    output: HostPath,
    edits: Mapping[str, JSONValue],
    *,
    durability: Durability = "none",
    allow_incomplete: bool = False,
) -> EditReport: ...
def pack(
    input: HostPath | File,
    output: HostPath,
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
    roots: Roots,
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
def exr_header(path: HostPath) -> ExrHeaderReport: ...
def exr_scan(
    inputs: HostPath | Iterable[HostPath],
    *,
    jobs: int | None = None,
    allow_incomplete: bool = False,
) -> ScanReport: ...
def exr_pixels(
    input: HostPath, output: HostPath, *, part: int, pass_name: str
) -> PixelsReport: ...
def read_path_list(path: HostPath, *, nul: bool = False) -> tuple[str, ...]: ...
def report(source: ReportInput, output: HostPath, *, include_names: bool = False, include_file: HostPath | None = None) -> ReportPreview: ...
def report_preview(
    output: HostPath,
    *,
    report: HostPath | None = None,
    stdout: HostPath | None = None,
    trace: HostPath | None = None,
    trace_dir: HostPath | None = None,
    run_id: str | None = None,
    last: bool = False,
    include_names: bool = False,
    include_file: HostPath | None = None,
) -> ReportPreview: ...
def report_create(
    preview: HostPath, preview_sha256: str, output: HostPath
) -> ReportCreated: ...
def report_discard(preview: HostPath, preview_sha256: str) -> ReportDiscarded: ...
