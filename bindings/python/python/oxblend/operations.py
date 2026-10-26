"""Typed full-operation facade over the shared native engine, never a CLI wrapper.

Every function below mirrors one ``oxblend`` CLI command over the same
shared engine: same arguments, same reports, same stable error kinds. The
path-or-``File`` convention is uniform: pass a path to open, run and close
for one call, or a captured ``File`` session to reuse its roots. Reports are
owned immutable ``Record`` mappings whose ``to_json`` is the exact CLI JSON
projection; refusals raise the same ``OxblendError`` subclass the CLI prints,
except for the accounted non-regular-source admission difference: captured
``File`` inputs refuse devices and FIFOs with ``pack_path`` before opening;
the CLI's direct read path may instead report a read error (for example,
``summary /dev/null`` reports ``truncated``). See the binding README.
"""
from __future__ import annotations
import os
import json
from dataclasses import dataclass
from collections.abc import Iterable, Mapping, Sequence
from typing import Any, Literal, TypeAlias
from . import _native
from . import (File, HostPath, Record, Summary, TraceReport, PackReport, QueryReport,
               _invoke, _admit, _error, _suggest, _items, open, IncompleteTraceError, IncompletePackError)
from .errors import (IncompleteEditError, IncompleteFactsError, IncompleteDiffError,
                     IncompleteOverridesError, IncompleteExrScanError, ExtractRefusalError,
                     UsageError, EditSpecError, PathError, OxblendError)

JSONValue: TypeAlias = None | bool | int | float | str | list['JSONValue'] | dict[str, 'JSONValue']
Memory: TypeAlias = Literal['fast', 'bounded']
Durability: TypeAlias = Literal['none', 'end', 'each-file']
Roots: TypeAlias = HostPath | Iterable[HostPath | File] | File

class CatalogReport(Record):
    """Typed ``oxblend catalog`` report: concepts, read ops and limits.

    The catalog is engine-owned and versioned; ``catalog_revision`` pins the
    contract the rest of the document follows.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the catalog fields still constructs;
            field access then raises ``KeyError``.

    Returns:
        CatalogReport: the owned immutable mapping over the catalog facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend catalog`` JSON document.
    """
    __slots__ = ()
    @property
    def catalog_revision(self) -> str:
        """The catalog contract revision this document follows.

        Returns:
            The ``catalog_revision`` string.

        Raises:
            KeyError: the facts omit ``catalog_revision``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``catalog_revision`` field in
        the ``oxblend catalog`` JSON output.
        """
        return self['catalog_revision']
    @property
    def concepts(self) -> tuple[Record, ...]:
        """The authored catalog concepts with their remedy entries.

        Returns:
            The ``concepts`` records.

        Raises:
            KeyError: the facts omit ``concepts``. Engine-produced reports
                always carry it; a report built by hand without the field
                (for example ``CatalogReport({})``) refuses here instead of
                claiming a total.

        No CLI equivalent: typed view of the ``concepts`` array in the
        ``oxblend catalog`` JSON output.
        """
        return self['concepts']
    @property
    def read_operations(self) -> tuple[Record, ...]:
        """The declared read operations and their contracts.

        Returns:
            The ``read_operations`` records.

        Raises:
            KeyError: the facts omit ``read_operations``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``read_operations`` array in
        the ``oxblend catalog`` JSON output.
        """
        return self['read_operations']
    @property
    def raw_projection(self) -> Record:
        """The raw catalog projection without typed interpretation.

        Returns:
            The ``raw_projection`` record.

        Raises:
            KeyError: the facts omit ``raw_projection``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``raw_projection`` object in
        the ``oxblend catalog`` JSON output.
        """
        return self['raw_projection']
    @property
    def limits(self) -> tuple[str, ...]:
        """The documented workload limits of this engine build.

        Returns:
            The ``limits`` strings.

        Raises:
            KeyError: the facts omit ``limits``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``limits`` array in the
        ``oxblend catalog`` JSON output.
        """
        return self['limits']

class ExplainReport(Record):
    """Typed ``oxblend explain`` report: concept entry and SDNA presence.

    Args:
        facts: the engine facts mapping to own, like ``Record``.

    Returns:
        ExplainReport: the owned immutable mapping over the explain facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend explain`` JSON document.
    """
    __slots__ = ()

class InspectReport(Record):
    """Typed ``oxblend inspect`` report: header, ordered blocks and SDNA.

    ``blocks`` is ordered: its array index is the BLOCK number the ``get``,
    ``pointer`` and ``payload`` operations take.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the inspect fields still constructs;
            field access then raises ``KeyError``.

    Returns:
        InspectReport: the owned immutable mapping over the inspect facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend inspect`` JSON document.
    """
    __slots__ = ()
    @property
    def header(self) -> Record:
        """The container header facts.

        Returns:
            The ``header`` record.

        Raises:
            KeyError: the facts omit ``header``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``header`` object in the
        ``oxblend inspect`` JSON output.
        """
        return self['header']
    @property
    def blocks(self) -> tuple[Record, ...]:
        """The ordered block facts; the array index is the BLOCK number.

        Returns:
            The ``blocks`` records in physical order.

        Raises:
            KeyError: the facts omit ``blocks``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``blocks`` array in the
        ``oxblend inspect`` JSON output.
        """
        return self['blocks']
    @property
    def sdna(self) -> Record:
        """The file's own SDNA schema facts.

        Returns:
            The ``sdna`` record.

        Raises:
            KeyError: the facts omit ``sdna``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``sdna`` object in the
        ``oxblend inspect`` JSON output.
        """
        return self['sdna']

class ValueReport(Record):
    """Typed ``oxblend get``/``oxblend payload`` value projection.

    Wraps ``{"schema": 1, "value": {...}}``; bytes render as hex and
    float32 leaves keep exact bits, matching CLI stdout.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A value
            built by hand without the ``value`` field still constructs;
            field access then raises ``KeyError``.

    Returns:
        ValueReport: the owned immutable mapping over the value facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend get`` and
    ``oxblend payload`` JSON value documents.
    """
    __slots__ = ()
    @property
    def value(self) -> Record:
        """The projected value object.

        Returns:
            The ``value`` record.

        Raises:
            KeyError: the facts omit ``value``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``value`` object in the
        ``oxblend get`` JSON output.
        """
        return self['value']

class PointerReport(Record):
    """Typed ``oxblend pointer`` report: resolved block index or null.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the ``block_index`` field still constructs;
            field access then raises ``KeyError``.

    Returns:
        PointerReport: the owned immutable mapping over the pointer facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend pointer`` JSON document.
    """
    __slots__ = ()
    @property
    def block_index(self) -> int | None:
        """The resolved block index in this reader's namespace, if any.

        Returns:
            The ``block_index``, or None for a null/inapplicable pointer.

        Raises:
            KeyError: the facts omit ``block_index``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``block_index`` field in the
        ``oxblend pointer`` JSON output.
        """
        return self['block_index']

class FactsReport(Record):
    """Typed source/library facts report with independent failure rows.

    Independent saved facts are retained while failed queries land in
    ``errors``; ``complete`` is False when any required query failed.

    Args:
        facts: the engine facts mapping to own, like ``Record``. The
            ``complete`` and ``errors`` views default when absent; the
            ``facts`` view raises ``KeyError`` on a report built by hand
            without it.

    Returns:
        FactsReport: the owned immutable mapping over the facts-report facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend source-facts`` and
    ``oxblend library-facts`` JSON documents.
    """
    __slots__ = ()
    @property
    def complete(self) -> bool:
        """Whether every required fact query succeeded.

        Returns:
            The ``complete`` verdict, defaulting to True when the report
            carries no coverage flag.

        Raises:
            Nothing.

        No CLI equivalent: typed view of the ``complete`` field in the
        ``oxblend source-facts`` JSON output.
        """
        return self.get('complete', True)
    @property
    def facts(self) -> Record:
        """The retained per-file facts.

        Returns:
            The ``facts`` record.

        Raises:
            KeyError: the facts omit ``facts``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``facts`` object in the
        ``oxblend source-facts`` JSON output.
        """
        return self['facts']
    @property
    def errors(self) -> tuple[Record, ...]:
        """Failed fact queries kept beside the retained facts.

        Returns:
            The ``errors`` rows, empty on full success.

        Raises:
            Nothing.

        No CLI equivalent: typed view of the ``errors`` array in the
        ``oxblend source-facts`` JSON output.
        """
        return self.get('errors', ())

class CensusReport(Record):
    """Typed ``oxblend pointer-census`` report: whole-file pointer states.

    A census of pointer states, not a semantic reconstruction; see
    ``oxblend trace`` for the interpreted dependency graph.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the census fields still constructs;
            field access then raises ``KeyError``.

    Returns:
        CensusReport: the owned immutable mapping over the census facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend pointer-census`` JSON
    document.
    """
    __slots__ = ()
    @property
    def semantic_verified(self) -> bool:
        """Whether the census states verified against semantic roles.

        Returns:
            The ``semantic_verified`` verdict.

        Raises:
            KeyError: the facts omit ``semantic_verified``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``semantic_verified`` field in
        the ``oxblend pointer-census`` JSON output.
        """
        return self['semantic_verified']

class MultiRootReport(Record):
    """Typed multi-root report with per-root rows and a cross-reference.

    Returned by ``trace``/``list_dependencies`` for a genuinely multi-root
    selection; a single-root call returns ``TraceReport``/``ListReport``.
    The ``operation`` field names the schema owner (trace or list).

    Args:
        facts: the engine facts mapping to own, like ``Record``. The
            ``root_lists`` view defaults when absent; the other views raise
            ``KeyError`` on a report built by hand without them.

    Returns:
        MultiRootReport: the owned immutable mapping over the multi-root facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the multi-root rows in the
    ``oxblend trace`` and ``oxblend list`` JSON output.
    """
    __slots__ = ()
    @property
    def complete(self) -> bool:
        """Whether every root reached trustworthy full coverage.

        Returns:
            The engine ``complete`` verdict across all roots.

        Raises:
            KeyError: the facts omit ``complete``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``complete`` field in the
        multi-root ``oxblend trace`` JSON output.
        """
        return self['complete']
    @property
    def operation(self) -> str:
        """The schema owner that produced this report.

        Returns:
            ``trace`` or ``list``.

        Raises:
            KeyError: the facts omit ``operation``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``operation`` field in the
        multi-root ``oxblend trace`` JSON output.
        """
        return self['operation']
    @property
    def root_traces(self) -> tuple[Record, ...]:
        """One trace row per captured root, in capture order.

        Returns:
            The ``root_traces`` records.

        Raises:
            KeyError: the facts omit ``root_traces``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``root_traces`` array in the
        multi-root ``oxblend trace`` JSON output.
        """
        return self['root_traces']
    @property
    def root_lists(self) -> tuple[Record, ...]:
        """One list row per captured root, in capture order.

        Returns:
            The ``root_lists`` records, empty for trace-schema reports.

        Raises:
            Nothing.

        No CLI equivalent: typed view of the ``root_lists`` array in the
        multi-root ``oxblend list`` JSON output.
        """
        return self.get('root_lists', ())
    @property
    def cross_reference(self) -> Record:
        """The cross-root observation rows shared by all roots.

        Returns:
            The ``cross_reference`` record.

        Raises:
            KeyError: the facts omit ``cross_reference``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``cross_reference`` object in
        the multi-root ``oxblend trace`` JSON output.
        """
        return self['cross_reference']

class ListReport(Record):
    """Typed ``oxblend list`` report: the dependency closure as a listing.

    Args:
        facts: the engine facts mapping to own, like ``Record``. The
            ``complete`` view defaults when absent, like the engine's
            coverage verdict on reports without a coverage flag.

    Returns:
        ListReport: the owned immutable mapping over the listing facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend list`` JSON document.
    """
    __slots__ = ()
    @property
    def complete(self) -> bool:
        """Whether the listing covers the complete closure.

        Returns:
            The ``complete`` verdict, defaulting to True when the report
            carries no coverage flag.

        Raises:
            Nothing.

        No CLI equivalent: typed view of the ``complete`` field in the
        ``oxblend list`` JSON output.
        """
        return self.get('complete', True)

class OverrideReport(Record):
    """Typed ``oxblend overrides`` report: override inventory and refusals.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the inventory fields still constructs;
            field access then raises ``KeyError``.

    Returns:
        OverrideReport: the owned immutable mapping over the overrides facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend overrides`` JSON document.
    """
    __slots__ = ()
    @property
    def complete(self) -> bool:
        """Whether the inventory resolved every override record.

        Returns:
            The engine ``complete`` verdict.

        Raises:
            KeyError: the facts omit ``complete``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``complete`` field in the
        ``oxblend overrides`` JSON output.
        """
        return self['complete']
    @property
    def errors(self) -> tuple[Record, ...]:
        """Unresolved or refused override records.

        Returns:
            The ``errors`` rows, empty on a complete inventory.

        Raises:
            KeyError: the facts omit ``errors``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``errors`` array in the
        ``oxblend overrides`` JSON output.
        """
        return self['errors']
    @property
    def warnings(self) -> tuple[Record, ...]:
        """Non-fatal observations kept beside the inventory verdict.

        Returns:
            The ``warnings`` rows.

        Raises:
            KeyError: the facts omit ``warnings``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``warnings`` array in the
        ``oxblend overrides`` JSON output.
        """
        return self['warnings']

class DiffReport(Record):
    """Typed ``oxblend diff`` report: equality verdict plus differences.

    ``equal`` is None when the comparison lacked enough evidence for an
    equality claim; only ``complete`` True with zero differences proves
    semantic equality of the compared scope.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the diff fields still constructs;
            field access then raises ``KeyError``.

    Returns:
        DiffReport: the owned immutable mapping over the diff facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend diff`` JSON document.
    """
    __slots__ = ()
    @property
    def complete(self) -> bool:
        """Whether the comparison gathered enough evidence to judge.

        Returns:
            The engine ``complete`` verdict.

        Raises:
            KeyError: the facts omit ``complete``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``complete`` field in the
        ``oxblend diff`` JSON output.
        """
        return self['complete']
    @property
    def equal(self) -> bool | None:
        """The equality claim, or None when evidence was insufficient.

        Returns:
            True/False on a complete comparison, None otherwise.

        Raises:
            KeyError: the facts omit ``equal``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``equal`` field in the
        ``oxblend diff`` JSON output.
        """
        return self['equal']
    @property
    def differences(self) -> tuple[Record, ...]:
        """The semantic difference rows in engine order.

        Returns:
            The ``differences`` records.

        Raises:
            KeyError: the facts omit ``differences``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``differences`` array in the
        ``oxblend diff`` JSON output.
        """
        return self['differences']
    @property
    def difference_count(self) -> int:
        """How many difference rows the comparison established.

        Returns:
            The ``difference_count`` integer.

        Raises:
            KeyError: the facts omit ``difference_count``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``difference_count`` field in
        the ``oxblend diff`` JSON output.
        """
        return self['difference_count']
    @property
    def errors(self) -> tuple[Record, ...]:
        """Comparison failures that withheld the equality claim, if any.

        Returns:
            The ``errors`` rows, empty on a complete comparison.

        Raises:
            KeyError: the facts omit ``errors``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``errors`` array in the
        ``oxblend diff`` JSON output.
        """
        return self['errors']

class EditReport(Record):
    """Typed edit-family report: verdict plus publication side effects.

    Shared by ``edit``, ``rewrite`` and their pixel siblings: ``complete``
    and the side-effect flags describe what the writer actually published,
    so a refused edit never claims an output it did not write.

    Args:
        facts: the engine facts mapping to own, like ``Record``. The
            ``errors`` view defaults when absent; the other views raise
            ``KeyError`` on a report built by hand without them.

    Returns:
        EditReport: the owned immutable mapping over the edit facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend edit`` and
    ``oxblend rewrite`` JSON documents.
    """
    __slots__ = ()
    @property
    def complete(self) -> bool:
        """Whether the edit applied and published completely.

        Returns:
            The engine ``complete`` verdict.

        Raises:
            KeyError: the facts omit ``complete``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``complete`` field in the
        ``oxblend edit`` JSON output.
        """
        return self['complete']
    @property
    def output_written(self) -> bool:
        """Whether this call wrote the output file.

        Returns:
            True only when publication wrote it.

        Raises:
            KeyError: the facts omit ``output_written``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``output_written`` field in
        the ``oxblend edit`` JSON output.
        """
        return self['output_written']
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
        in the ``oxblend edit`` JSON output.
        """
        return self['durability_completed']
    @property
    def errors(self) -> tuple[Record, ...]:
        """Edit failures that withheld publication, if any.

        Returns:
            The ``errors`` rows, empty on a complete edit.

        Raises:
            Nothing.

        No CLI equivalent: typed view of the ``errors`` array in the
        ``oxblend edit`` JSON output.
        """
        return self.get('errors', ())

class ExtractReport(EditReport):
    """Typed ``oxblend extract`` report: selected IDs plus refusal rows.

    Inherits the ``EditReport`` publication verdict; ``refusals`` names the
    reference and pointer policies that withheld the partial file.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the extract fields still constructs;
            field access then raises ``KeyError``.

    Returns:
        ExtractReport: the owned immutable mapping over the extract facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend extract`` JSON document.
    """
    __slots__ = ()
    @property
    def selected_ids(self) -> tuple[Record, ...]:
        """The IDs selected for the partial file.

        Returns:
            The ``selected_ids`` records.

        Raises:
            KeyError: the facts omit ``selected_ids``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``selected_ids`` array in the
        ``oxblend extract`` JSON output.
        """
        return self['selected_ids']
    @property
    def refusals(self) -> tuple[Record, ...]:
        """The reference/pointer policies that withheld extraction.

        Returns:
            The ``refusals`` rows, empty on a complete extract.

        Raises:
            KeyError: the facts omit ``refusals``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``refusals`` array in the
        ``oxblend extract`` JSON output.
        """
        return self['refusals']

class NodeTreeReport(Record):
    """Typed ``oxblend node-tree`` report: the qualified compositor trees.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the ``trees`` field still constructs;
            field access then raises ``KeyError``.

    Returns:
        NodeTreeReport: the owned immutable mapping over the node-tree facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend node-tree`` JSON document.
    """
    __slots__ = ()
    @property
    def trees(self) -> tuple[Record, ...]:
        """The qualified node-tree records in engine order.

        Returns:
            The ``trees`` records.

        Raises:
            KeyError: the facts omit ``trees``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``trees`` array in the
        ``oxblend node-tree`` JSON output.
        """
        return self['trees']

class ExrHeaderReport(Record):
    """Typed ``oxblend exr-header`` report: parts plus integrity verdict.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the header fields still constructs;
            field access then raises ``KeyError``.

    Returns:
        ExrHeaderReport: the owned immutable mapping over the header facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend exr-header`` JSON document.
    """
    __slots__ = ()
    @property
    def parts(self) -> tuple[Record, ...]:
        """The EXR part records in file order.

        Returns:
            The ``parts`` records.

        Raises:
            KeyError: the facts omit ``parts``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``parts`` array in the
        ``oxblend exr-header`` JSON output.
        """
        return self['parts']
    @property
    def pixel_integrity_verified(self) -> bool:
        """Whether the header pixel-integrity proof verified.

        Returns:
            The ``pixel_integrity_verified`` verdict.

        Raises:
            KeyError: the facts omit ``pixel_integrity_verified``.
                Engine-produced reports always carry it; a report built by
                hand without the field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``pixel_integrity_verified``
        field in the ``oxblend exr-header`` JSON output.
        """
        return self['pixel_integrity_verified']

class PixelsReport(EditReport):
    """Typed ``oxblend exr-pixels`` report: channels plus decode verdict.

    Inherits the ``EditReport`` publication verdict for the decoded output.

    Args:
        facts: the engine facts mapping to own, like ``Record``. A report
            built by hand without the pixel fields still constructs;
            field access then raises ``KeyError``.

    Returns:
        PixelsReport: the owned immutable mapping over the pixel facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend exr-pixels`` JSON document.
    """
    __slots__ = ()
    @property
    def channels(self) -> tuple[Any, ...]:
        """The decoded channel payloads in engine order.

        Returns:
            The ``channels`` values.

        Raises:
            KeyError: the facts omit ``channels``. Engine-produced reports
                always carry it; a report built by hand without the field
                refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``channels`` array in the
        ``oxblend exr-pixels`` JSON output.
        """
        return self['channels']
    @property
    def decode_complete(self) -> bool:
        """Whether pixel decoding completed under the selected contract.

        Returns:
            The ``decode_complete`` verdict.

        Raises:
            KeyError: the facts omit ``decode_complete``. Engine-produced
                reports always carry it; a report built by hand without the
                field refuses here instead of claiming a total.

        No CLI equivalent: typed view of the ``decode_complete`` field in
        the ``oxblend exr-pixels`` JSON output.
        """
        return self['decode_complete']

@dataclass(frozen=True, slots=True)
class TextReport:
    """CLI text diagnostics; blocks has no JSON CLI schema.

    text: the exact CLI stdout text for the operation.

    Args:
        text: the exact CLI stdout text for the operation.

    Returns:
        TextReport: the frozen text holder for the plain-text stdout.

    Raises:
        TypeError: a required field is missing or an unexpected keyword is passed.

    No CLI equivalent: wraps the plain-text ``oxblend blocks`` stdout, which
    has no JSON schema to project.
    """
    text: str
    def to_json(self) -> str:
        """Return the raw CLI stdout text unchanged.

        Returns:
            The stored ``text`` bytes-decoded string.

        Raises:
            Nothing.

        No CLI equivalent: accessor over the plain-text ``oxblend blocks``
        stdout.
        """
        return self.text
    def __str__(self) -> str:
        """Render the stored CLI stdout text.

        Returns:
            The stored ``text`` string.

        Raises:
            Nothing.

        No CLI equivalent: string form of the ``oxblend blocks`` stdout.
        """
        return self.text

@dataclass(frozen=True, slots=True, init=False)
class ScanReport:
    """Ordered EXR JSONL records including its final summary.

    records: the streamed per-file records in order with the terminal summary
    row last. Built from the engine's ordered JSONL output, so the
    constructor takes exactly that ``records`` sequence.

    Args:
        records: the ordered per-file mappings with the terminal summary
            row last; each row becomes an owned ``Record``.

    Returns:
        ScanReport: the frozen ordered view over the streamed records.

    Raises:
        TypeError: ``records`` is not an iterable of mappings.
        AttributeError: a row is not a mapping.

    No CLI equivalent: typed view of the ``oxblend exr-scan`` JSONL stream.
    """
    records: tuple[Record, ...]
    def __init__(self, records: Iterable[Mapping[str, Any]]) -> None:
        object.__setattr__(self, 'records', tuple(Record(row) for row in records))
    @property
    def summary(self) -> Record:
        """The terminal summary row of the scan stream.

        Returns:
            The last ``records`` entry.

        Raises:
            IndexError: the stream holds no records.

        No CLI equivalent: typed view of the summary row in the
        ``oxblend exr-scan`` JSONL stream.
        """
        return self.records[-1]
    @property
    def complete(self) -> bool:
        """Whether the scan established every requested member.

        Returns:
            The summary ``complete`` verdict.

        Raises:
            IndexError: the stream holds no records.
            KeyError: the terminal summary row carries no ``complete`` verdict.

        No CLI equivalent: typed view of the ``complete`` field in the
        ``oxblend exr-scan`` summary row.
        """
        return self.summary['complete']
    def to_json(self) -> list[JSONValue]:
        """Render every stream record as JSON values in order.

        Returns:
            The per-file records plus the summary, matching the
            ``oxblend exr-scan`` JSONL lines.

        Raises:
            Nothing.

        CLI: ``oxblend exr-scan``.
        """
        return [r.to_json() for r in self.records]

class ReportPreview(Record):
    """Typed frozen preview snapshot: sanitized text reviewed before filing.

    Args:
        facts: the engine facts mapping to own, like ``Record``.

    Returns:
        ReportPreview: the owned immutable mapping over the preview facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend report --preview`` output
    bundle manifest.
    """
    __slots__ = ()
class ReportCreated(Record):
    """Typed publication receipt for an accepted preview snapshot.

    Args:
        facts: the engine facts mapping to own, like ``Record``.

    Returns:
        ReportCreated: the owned immutable mapping over the receipt facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend report --create`` output
    receipt.
    """
    __slots__ = ()
class ReportDiscarded(Record):
    """Typed discard receipt for a rejected preview snapshot.

    Args:
        facts: the engine facts mapping to own, like ``Record``.

    Returns:
        ReportDiscarded: the owned immutable mapping over the discard facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: typed view of the ``oxblend report`` discard output
    receipt.
    """
    __slots__ = ()

ReportInput: TypeAlias = HostPath | OxblendError | Record | Summary | ScanReport


def _path(path: HostPath) -> str:
    return os.fsdecode(os.fspath(path))

def _spec(spec: Mapping[str, Any]) -> str:
    try:
        return json.dumps(dict(spec), ensure_ascii=True, allow_nan=False)
    except (TypeError, ValueError) as error:
        raise _error(EditSpecError(str(error))) from None

def _roots(roots: Roots) -> File | tuple[str, ...]:
    if isinstance(roots, File): return roots
    if isinstance(roots, (str, bytes, os.PathLike)):
        return (_path(roots),)
    values = tuple(roots)
    if len(values) == 1 and isinstance(values[0], File): return values[0]
    if any(isinstance(p, File) for p in values):
        raise _error(UsageError('Separately opened Files cannot be combined; use open_many(paths)'))
    return tuple(_path(p) for p in values)


def summary(path: HostPath | File) -> Any:
    """Summarize one saved input: header, block census, scenes and warnings.

    Args:
        path: the saved blend file, or a captured ``File`` whose summary is read.

    Returns:
        The typed ``Summary``.

    Raises:
        OxblendError: the engine's diagnosed open/read failure.

    CLI: ``oxblend summary``.
    """
    if isinstance(path, File): return path.summary()
    with open(path) as f: return f.summary()

def query(path: HostPath | File, *, type: str, fields: Iterable[str], allow_incomplete: bool = False) -> QueryReport:
    """Read saved records of one type; saved controls, never evaluation.

    Args:
        path: the saved blend file, or a captured ``File``.
        type: the saved record type to match.
        fields: exact raw SDNA field names to project per record.
        allow_incomplete: return the partial report instead of raising
            ``IncompleteQueryError`` when fields are unavailable.

    Returns:
        The ``QueryReport``; partial rows are kept.

    Raises:
        IncompleteQueryError: unavailable fields without ``allow_incomplete``.
        TypeError: ``fields`` is a scalar string rather than an iterable.
        OxblendError: the engine's diagnosed open/read failure.

    CLI: ``oxblend query``.
    """
    if isinstance(path, File): return path.query(type=type, fields=fields, allow_incomplete=allow_incomplete)
    with open(path) as f: return f.query(type=type, fields=fields, allow_incomplete=allow_incomplete)

def catalog() -> CatalogReport:
    """Read the engine-owned catalog: concepts, read ops and limits.

    Returns:
        The ``CatalogReport`` pinned to its ``catalog_revision``.

    Raises:
        OxblendError: the engine's diagnosed catalog failure (never for a
            healthy build; the catalog is compiled in).

    CLI: ``oxblend catalog``.
    """
    return CatalogReport(_invoke(_native.catalog))

def explain(concept: str, *, file: HostPath | File | None = None) -> ExplainReport:
    """Explain one catalog concept, optionally against a file's SDNA.

    Args:
        concept: the exact catalog concept name.
        file: an optional saved file or captured ``File`` checked for SDNA
            presence of the concept.

    Returns:
        The ``ExplainReport`` with the concept entry and next actions.

    Raises:
        OxblendError: ``unknown_concept`` for an unrecognized name, or the
            engine's diagnosed open failure for ``file``.

    CLI: ``oxblend explain``.
    """
    if file is None: return ExplainReport(_invoke(_native.explain, concept))
    if isinstance(file, File): return file.explain(concept)
    with open(file) as f: return f.explain(concept)

def inspect(path: HostPath | File) -> InspectReport:
    """Read the ordered blocks and the file's own SDNA schema.

    Args:
        path: the saved blend file, or a captured ``File``.

    Returns:
        The ``InspectReport`` whose ``blocks`` order defines BLOCK numbers.

    Raises:
        OxblendError: the engine's diagnosed open/read failure.

    CLI: ``oxblend inspect``.
    """
    if isinstance(path, File): return path.inspect()
    return InspectReport._from_native_owned(_invoke(_native.inspect, _path(path)))

def payload(path: HostPath | File, block: int) -> ValueReport:
    """Read one complete block payload as hex bytes.

    Args:
        path: the saved blend file, or a captured ``File``.
        block: the physical block index (the ``inspect`` array position).

    Returns:
        The ``ValueReport`` wrapping the payload bytes.

    Raises:
        OxblendError: the engine's diagnosed failure (e.g. bounds for an
            out-of-range index).

    CLI: ``oxblend payload``.
    """
    if isinstance(path, File): return path.payload(block)
    return ValueReport(_invoke(_native.payload, _path(path), block))

def get(path: HostPath, block: int, path_steps: Sequence[str | int], element: int = 0, *, raw: bool = False, byte_array: int | None = None, pointer_table: int | None = None) -> ValueReport:
    """Read one addressed saved value; NUL trimming is explicit, never silent.

    Args:
        path: the saved blend file.
        block: the physical block index (the ``inspect`` array position).
        path_steps: field-name strings and flattened array indices from the
            struct root.
        element: nonnegative struct element, default 0.
        raw: keep full char-array bytes instead of trimming at NUL.
        byte_array: counted one-byte pointer read of exactly this many bytes;
            mutually exclusive with ``raw`` and ``pointer_table``.
        pointer_table: counted struct** slot read of exactly this many
            targets; mutually exclusive with the other modes.

    Returns:
        The ``ValueReport`` with the typed value projection.

    Raises:
        PathError: ``path_steps`` is a scalar string rather than a sequence.
        TypeError: the native argument conversion rejects a wrong-typed
            argument before the engine runs (idiomatic Python exception,
            outside the canonical CLI contract).
        OxblendError: the engine's diagnosed failure (e.g. missing_field,
            bounds) with kind/message/catalog.

    CLI: ``oxblend get``.
    """
    with open(path) as f:
        return ValueReport(_invoke(f._handle.get_json, block, _spec_path(path_steps), element, raw, byte_array, pointer_table))

def _spec_path(steps: Sequence[str | int]) -> str:
    if isinstance(steps, (str, bytes)):
        raise _error(PathError('JSON array required; pass a sequence of field names and indices'))
    try:
        return json.dumps(list(steps), ensure_ascii=True, allow_nan=False)
    except (TypeError, ValueError) as error:
        raise _error(PathError(str(error))) from None

def pointer(path: HostPath, block: int, path_steps: Sequence[str | int], element: int = 0) -> PointerReport:
    """Resolve one saved pointer step to its target block index, if any.

    Args:
        path: the saved blend file.
        block: the physical block index (the ``inspect`` array position).
        path_steps: field-name strings and flattened array indices from the
            struct root to the pointer field.
        element: nonnegative struct element, default 0.

    Returns:
        The ``PointerReport`` with the target index or null.

    Raises:
        PathError: ``path_steps`` is a scalar string rather than a sequence.
        OxblendError: the engine's diagnosed failure for an unresolvable path.

    CLI: ``oxblend pointer``.
    """
    with open(path) as f:
        return PointerReport(_invoke(f._handle.pointer_json, block, _spec_path(path_steps), element))

def source_facts(path: HostPath | File, *, allow_incomplete: bool = False) -> FactsReport:
    """Read per-file source facts, keeping independent facts on failure.

    Args:
        path: the saved blend file, or a captured ``File``.
        allow_incomplete: return the partial report instead of raising
            ``IncompleteFactsError`` when queries fail.

    Returns:
        The ``FactsReport`` for the selected sources.

    Raises:
        IncompleteFactsError: failed queries without ``allow_incomplete``.
        OxblendError: the engine's diagnosed open/read failure.

    CLI: ``oxblend source-facts``.
    """
    if isinstance(path, File): return path.source_facts(allow_incomplete=allow_incomplete)
    return _admit(FactsReport(_invoke(_native.source_facts, _path(path))), IncompleteFactsError, 'facts', allow_incomplete, path, 'source-facts')

def library_facts(path: HostPath | File, *, allow_incomplete: bool = False) -> FactsReport:
    """Read per-file facts for the libraries a selection retains.

    Args:
        path: the saved blend file, or a captured ``File``.
        allow_incomplete: return the partial report instead of raising
            ``IncompleteFactsError`` when queries fail.

    Returns:
        The ``FactsReport`` for the retained libraries.

    Raises:
        IncompleteFactsError: failed queries without ``allow_incomplete``.
        OxblendError: the engine's diagnosed open/read failure.

    CLI: ``oxblend library-facts``.
    """
    if isinstance(path, File): return path.library_facts(allow_incomplete=allow_incomplete)
    return _admit(FactsReport(_invoke(_native.library_facts, _path(path))), IncompleteFactsError, 'facts', allow_incomplete, path, 'library-facts')

def pointer_census(path: HostPath | File, *, full: bool = False) -> CensusReport:
    """Count whole-file pointer states; semantic reconstruction is separate.

    Args:
        path: the saved blend file, or a captured ``File``.
        full: include the full per-pointer detail projection.

    Returns:
        The ``CensusReport`` with the semantic-verified verdict.

    Raises:
        OxblendError: the engine's diagnosed open/read failure.

    CLI: ``oxblend pointer-census``.
    """
    if isinstance(path, File): return path.pointer_census(full=full)
    return CensusReport(_invoke(_native.pointer_census, _path(path), full))

def list_dependencies(roots: Roots, *, full_info: bool = False, low_memory: bool = False, allow_incomplete: bool = False) -> ListReport | MultiRootReport:
    """List the dependency closure of one root set without walking usages.

    Args:
        roots: one path, one captured ``File`` (or a single-element ``[file]``),
            or several paths for a multi-root report. Separately opened Files
            can never be combined; use ``open_many``.
        full_info: include the full per-member detail projection.
        low_memory: trade IO/time for a lower decoded-source peak.
        allow_incomplete: return the partial report instead of raising
            ``IncompleteTraceError`` on incomplete coverage.

    Returns:
        The ``ListReport`` (single root) or ``MultiRootReport``.

    Raises:
        IncompleteTraceError: incomplete coverage without ``allow_incomplete``.
        UsageError: separately opened Files mixed in ``roots``.
        OxblendError: the engine's diagnosed open/read failure.

    CLI: ``oxblend list``.
    """
    paths = _roots(roots)
    if isinstance(paths, File): return paths.list_dependencies(full_info=full_info, low_memory=low_memory, allow_incomplete=allow_incomplete)
    if len(paths) != 1:
        data = _invoke(_native.list_many, paths, full_info, low_memory)
        report = MultiRootReport(data) if data.get('schema') == 'oxblend.multi-root/v1' else ListReport(data)
        return _admit(report, IncompleteTraceError, 'trace', allow_incomplete)
    try:
        report = ListReport(_invoke(_native.list_dependencies, paths[0], full_info, low_memory))
    except OxblendError as error:
        if allow_incomplete and error.kind != 'input_changed' and isinstance(error.report, ListReport): return error.report
        raise
    return _admit(report, IncompleteTraceError, 'trace', allow_incomplete)

def trace(roots: Roots, *, recursive: bool = True, low_memory: bool = False, allow_incomplete: bool = False) -> TraceReport | MultiRootReport:
    """Walk the dependency closure of one root set over retained libraries.

    Args:
        roots: one path, one captured ``File`` (or a single-element ``[file]``),
            or several paths for a multi-root report. Separately opened Files
            can never be combined; use ``open_many``.
        recursive: follow referenced libraries; False limits the walk to the
            captured roots.
        low_memory: trade IO/time for a lower decoded-source peak.
        allow_incomplete: return the partial report instead of raising
            ``IncompleteTraceError`` on incomplete coverage.

    Returns:
        The ``TraceReport`` (single root) or ``MultiRootReport``.

    Raises:
        IncompleteTraceError: incomplete coverage without ``allow_incomplete``.
        UsageError: separately opened Files mixed in ``roots``.
        OxblendError: the engine's diagnosed open/read failure.

    CLI: ``oxblend trace``.
    """
    paths = _roots(roots)
    if isinstance(paths, File): return paths.trace(recursive=recursive, low_memory=low_memory, allow_incomplete=allow_incomplete)
    if len(paths) != 1:
        data = _invoke(_native.trace_many, paths, recursive, low_memory)
        report = MultiRootReport(data) if data.get('schema') == 'oxblend.multi-root/v1' else TraceReport(data)
        return _admit(report, IncompleteTraceError, 'trace', allow_incomplete)
    return _admit(TraceReport(_invoke(_native.trace_path, paths[0], recursive, low_memory)), IncompleteTraceError, 'trace', allow_incomplete)

def block_report(path: HostPath, *, limit: int = 10, address: int | None = None, dump: bool = False) -> TextReport:
    """Describe block statistics, largest blocks and pointer referrers.

    Args:
        path: the saved blend file.
        limit: rows shown; supports negative slicing like the CLI.
        address: inspect referrers to this decimal or 0x hexadecimal address.
        dump: include payload bytes in 32-byte lines.

    Returns:
        The ``TextReport`` wrapping the exact CLI stdout text.

    Raises:
        OxblendError: the engine's diagnosed open/read failure.

    CLI: ``oxblend blocks``.
    """
    return TextReport(_invoke(_native.block_report, _path(path), limit, address, dump)['text'])

def overrides(path: HostPath, *, recursive: bool = True, allow_incomplete: bool = False) -> OverrideReport:
    """Inventory override records without evaluating the scene.

    Args:
        path: the saved blend file.
        recursive: follow referenced libraries.
        allow_incomplete: return the partial report instead of raising
            ``IncompleteOverridesError`` on unresolved records.

    Returns:
        The ``OverrideReport``.

    Raises:
        IncompleteOverridesError: unresolved records without ``allow_incomplete``.
        OxblendError: the engine's diagnosed open/read failure.

    CLI: ``oxblend overrides``.
    """
    return _admit(OverrideReport(_invoke(_native.overrides, _path(path), recursive)), IncompleteOverridesError, 'overrides', allow_incomplete)

def diff(a: HostPath, b: HostPath, *, allow_incomplete: bool = False) -> DiffReport:
    """Compare two saved files semantically; never byte equality alone.

    Args:
        a: the first saved blend file.
        b: the second saved blend file.
        allow_incomplete: return the partial report instead of raising
            ``IncompleteDiffError`` when evidence is insufficient.

    Returns:
        The ``DiffReport`` with the equality verdict and differences.

    Raises:
        IncompleteDiffError: insufficient evidence without ``allow_incomplete``.
        OxblendError: the engine's diagnosed open/read failure.

    CLI: ``oxblend diff``.
    """
    return _admit(DiffReport(_invoke(_native.diff, _path(a), _path(b))), IncompleteDiffError, 'diff', allow_incomplete)

def edit(input: HostPath, output: HostPath, spec: Mapping[str, Any], *, mode: Literal['format-only', 'comp', 'node-tree'], durability: Durability = 'none', allow_incomplete: bool = False) -> EditReport:
    """Apply a typed edit plan and publish the output with no-replace safety.

    Args:
        input: the saved blend file to edit; never modified in place.
        output: the new output path; an existing path refuses.
        spec: the strict edit specification mapping for the selected mode.
        mode: ``format-only``, ``comp``, or the ``node-tree`` CLI variant;
            the engine selects the exact variant, never the caller.
        durability: persistence synchronization (``none``, ``end`` or
            ``each-file``).
        allow_incomplete: return the partial report instead of raising
            ``IncompleteEditError`` on failure.

    Returns:
        The ``EditReport`` with the publication verdict.

    Raises:
        IncompleteEditError: a refused edit without ``allow_incomplete``.
        EditSpecError: ``spec`` is not the strict schema the mode expects.
        OxblendError: the engine's diagnosed open/read/write failure.

    CLI: ``oxblend edit`` (``oxblend node-tree`` when mode is ``node-tree``).
    """
    # The engine selects the exact CLI variant: node-tree vs typed.
    command = 'node-tree' if mode == 'node-tree' else 'edit'
    return _admit(EditReport(_invoke(_native.edit, _path(input), _path(output), _spec(spec), mode, durability)), IncompleteEditError, 'edit', allow_incomplete, command=command)

def node_tree(path: HostPath) -> NodeTreeReport:
    """Read the qualified compositor node trees of one saved file.

    Args:
        path: the saved blend file.

    Returns:
        The ``NodeTreeReport`` with the qualified trees.

    Raises:
        OxblendError: the engine's diagnosed failure (e.g. node_graph for a
            file outside the supported structural profile).

    CLI: ``oxblend node-tree``.
    """
    return NodeTreeReport(_invoke(_native.node_tree, _path(path)))

def extract(input: HostPath, output: HostPath, *, ids: Iterable[str] = (), blocks: Iterable[int] = (), clear: Iterable[str] = (), durability: Durability = 'none', allow_incomplete: bool = False) -> ExtractReport:
    """Write a selected partial file under explicit reference policies.

    Args:
        input: the saved blend file to select from; never modified in place.
        output: the new partial-file path; an existing path refuses.
        ids: linked ID identities to include.
        blocks: physical block indices to include.
        clear: references to clear under the typed clear policy.
        durability: persistence synchronization (``none``, ``end`` or
            ``each-file``).
        allow_incomplete: return the partial report instead of raising on
            refusal.

    Returns:
        The ``ExtractReport`` with selected IDs and refusal rows.

    Raises:
        ExtractRefusalError: the selection cannot be written under its
            reference and pointer policies.
        OxblendError: the engine's diagnosed open/read/write failure.

    CLI: ``oxblend extract``.
    """
    return _admit(ExtractReport(_invoke(_native.extract, _path(input), _path(output), _items(ids, 'ids'), _items(blocks, 'blocks'), _items(clear, 'clear'), durability)), ExtractRefusalError, 'extract', allow_incomplete)

def rewrite(input: HostPath, output: HostPath, edits: Mapping[str, Any], *, durability: Durability = 'none', allow_incomplete: bool = False) -> EditReport:
    """Apply path-rewrite edits; not an arbitrary setter, whitelist only.

    Args:
        input: the saved blend file to rewrite; never modified in place.
        output: the new output path; an existing path refuses.
        edits: the strict rewrite mapping with expected old paths.
        durability: persistence synchronization (``none``, ``end`` or
            ``each-file``).
        allow_incomplete: return the partial report instead of raising
            ``IncompleteEditError`` on failure.

    Returns:
        The ``EditReport`` with the publication verdict.

    Raises:
        IncompleteEditError: a refused rewrite without ``allow_incomplete``.
        EditSpecError: ``edits`` is not the strict schema rewrite expects.
        OxblendError: the engine's diagnosed open/read/write failure.

    CLI: ``oxblend rewrite``.
    """
    return _admit(EditReport(_invoke(_native.rewrite, _path(input), _path(output), _spec(edits), durability)), IncompleteEditError, 'edit', allow_incomplete, command='rewrite')

def pack(input: HostPath | File, output: HostPath, *, project_root: HostPath | None = None, exclude: Iterable[str] = (), dry_run: bool = False, relative_only: bool = False, compress: bool = False, keep_hierarchy: bool = False, permissive: bool = False, durability: Durability = 'none', memory: Memory | None = None, allow_incomplete: bool = False) -> PackReport:
    """Publish the dependency closure; no retry/cleanup on refusal.

    Args:
        input: the saved blend file, or a captured ``File`` whose session packs.
        output: the destination directory or archive path.
        project_root: explicit project directory for relocation planning.
        exclude: member patterns excluded from the plan.
        dry_run: plan without writing any output.
        relative_only: keep only workspace-relative member paths.
        compress: write the staged output compressed.
        keep_hierarchy: preserve source hierarchy under the destination.
        permissive: publish available members and mark INCOMPLETE instead of
            refusing outright.
        durability: persistence synchronization (``none``, ``end`` or
            ``each-file``).
        memory: decoded-source policy (``fast`` or ``bounded``); None keeps
            the Session's opened policy, an explicit differing mode refuses.
        allow_incomplete: return the partial report instead of raising
            ``IncompletePackError`` on incomplete publication.

    Returns:
        The ``PackReport``; incomplete publication is preserved in its
        ``report`` on the raised error when refusing.

    Raises:
        IncompletePackError: incomplete publication without ``allow_incomplete``
            or ``permissive``.
        OxblendError: the engine's diagnosed open/read/write failure, including
            the canonical directory and invalid-blend selections.

    CLI: ``oxblend pack``.
    """
    kwargs = dict(exclude=exclude, dry_run=dry_run, relative_only=relative_only, compress=compress, keep_hierarchy=keep_hierarchy, permissive=permissive, durability=durability, memory=memory, allow_incomplete=allow_incomplete)
    if isinstance(input, File): return input.pack(output, project_root=project_root, **kwargs)
    if not os.path.isfile(_path(input)):
        # Canonical CLI refusal, prose owned by the engine; the same gate
        # runs first inside the native pack call below, so both spellings
        # stay byte-identical by construction, never synthesized here.
        try:
            _invoke(_native.pack_input_error)
        except OxblendError as error:
            error.operation = 'pack'
            raise _suggest(_error(error), 'pack', input) from None
    # The Session open happens INSIDE the engine pack call, so an invalid
    # selected blend refuses with the exact CLI pack path (pack prefix
    # over snapshot/index), never a bare Session-open base.
    return _admit(PackReport(_invoke(_native.pack, _path(input), _path(output), None if project_root is None else _path(project_root), _items(exclude, 'exclude'), dry_run, relative_only, compress, keep_hierarchy, permissive, durability, memory or 'fast')), IncompletePackError, 'pack', allow_incomplete)

def pack_sequence(roots: Roots, output: HostPath, *, project_root: HostPath | None = None, exclude: Iterable[str] = (), dry_run: bool = False, relative_only: bool = False, compress: bool = False, keep_hierarchy: bool = True, permissive: bool = False, durability: Durability = 'none', memory: Memory | None = None, allow_incomplete: bool = False) -> PackReport:
    """Publish several roots together with deduplication.

    Args:
        roots: one path, one captured ``File``, or several paths for a
            multi-root pack. Separately opened Files can never be combined;
            use ``open_many``.
        output: the destination directory or archive path.
        project_root: explicit project directory for relocation planning.
        exclude: member patterns excluded from the plan.
        dry_run: plan without writing any output.
        relative_only: keep only workspace-relative member paths.
        compress: write the staged output compressed.
        keep_hierarchy: preserve source hierarchy under the destination.
        permissive: publish available members and mark INCOMPLETE instead of
            refusing outright.
        durability: persistence synchronization (``none``, ``end`` or
            ``each-file``).
        memory: decoded-source policy (``fast`` or ``bounded``); None keeps
            the Session's opened policy (``fast`` for bare paths).
        allow_incomplete: return the partial report instead of raising
            ``IncompletePackError`` on incomplete publication.

    Returns:
        The ``PackReport`` with deduplicated member rows.

    Raises:
        IncompletePackError: incomplete publication without ``allow_incomplete``
            or ``permissive``.
        UsageError: separately opened Files mixed in ``roots``.
        OxblendError: the engine's diagnosed open/read/write failure.

    CLI: ``oxblend pack-sequence``.
    """
    roots = _roots(roots)
    if isinstance(roots, File):
        return roots.pack_sequence(output, project_root=project_root, exclude=exclude, dry_run=dry_run, relative_only=relative_only, compress=compress, keep_hierarchy=keep_hierarchy, permissive=permissive, durability=durability, memory=memory, allow_incomplete=allow_incomplete)
    return _admit(PackReport(_invoke(_native.pack_sequence, roots, _path(output), None if project_root is None else _path(project_root), _items(exclude, 'exclude'), dry_run, relative_only, compress, keep_hierarchy, permissive, durability, memory or 'fast')), IncompletePackError, 'pack', allow_incomplete)

def exr_header(path: HostPath) -> ExrHeaderReport:
    """Parse one EXR header; malformed headers refuse, never guess.

    Args:
        path: the EXR file to parse.

    Returns:
        The ``ExrHeaderReport`` with parts and the integrity verdict.

    Raises:
        OxblendError: the engine's diagnosed failure (e.g. exr_header or
            exr_header_truncated for malformed input).

    CLI: ``oxblend exr-header``.
    """
    return ExrHeaderReport(_invoke(_native.exr_header, _path(path)))

def exr_scan(inputs: HostPath | Iterable[HostPath], *, jobs: int | None = None, allow_incomplete: bool = False) -> ScanReport:
    """Inventory EXR inputs: one facts record per file plus a summary.

    Args:
        inputs: one EXR path or an iterable of paths to inventory.
        jobs: worker count; None uses the engine default.
        allow_incomplete: return the partial ``ScanReport`` instead of
            raising ``IncompleteExrScanError`` when members are unestablished.

    Returns:
        The ``ScanReport`` with per-file records and the terminal summary.

    Raises:
        IncompleteExrScanError: unestablished members without
            ``allow_incomplete``.
        OxblendError: the engine's diagnosed filesystem/changed-input failure.

    CLI: ``oxblend exr-scan``.
    """
    paths = (inputs,) if isinstance(inputs, (str, bytes, os.PathLike)) else inputs
    try:
        return ScanReport(_invoke(_native.exr_scan, tuple(_path(p) for p in paths), jobs))
    except IncompleteExrScanError as error:
        if allow_incomplete and isinstance(error.report, ScanReport): return error.report
        raise

def exr_pixels(input: HostPath, output: HostPath, *, part: int, pass_name: str) -> PixelsReport:
    """Decode one EXR part/pass selection and publish the decoded output.

    Args:
        input: the EXR file to decode.
        output: the new decoded-output path; an existing path refuses.
        part: the EXR part index to decode.
        pass_name: the requested pass (channel selection) name.

    Returns:
        The ``PixelsReport`` with channels and the decode verdict.

    Raises:
        OxblendError: the engine's diagnosed failure (e.g. exr_selection for
            an absent part/pass, exr_decode when decoding cannot complete).

    CLI: ``oxblend exr-pixels``.
    """
    return PixelsReport(_invoke(_native.exr_pixels, _path(input), _path(output), part, pass_name))

def read_path_list(path: HostPath, *, nul: bool = False) -> tuple[str, ...]:
    """Read a path-list file for harness batching; no existence checks.

    Args:
        path: the list file whose lines (or NUL-separated records) name paths.
        nul: split on NUL bytes instead of newlines.

    Returns:
        The decoded path spellings in file order.

    Raises:
        OxblendError: the engine's diagnosed read failure for the list file.

    No CLI equivalent: in-process helper reading the same list layout
    ``oxblend exr-scan --files-from`` consumes; batch reading itself is
    ``oxblend read-batch`` with a JSON spec.
    """
    return tuple(os.fsdecode(p) for p in _invoke(_native.read_path_list, _path(path), nul))

def report(source: ReportInput, output: HostPath, *, include_names: bool = False, include_file: HostPath | None = None) -> ReportPreview:
    """Freeze a sanitized preview, never publish automatically.

    Args:
        source: a saved CLI report path, a raised ``OxblendError``, or an
            owned result (``Record``, ``Summary`` or ``ScanReport``).
        output: the new preview bundle path.
        include_names: explicitly opt into names in the bundle (default
            excludes them; never enable without an informed choice).
        include_file: explicitly opted-in whole file disclosed as a
            user-supplied attachment, never a certified input.

    Returns:
        The ``ReportPreview`` snapshot; review it, then call
        ``report_create`` with its digest.

    Raises:
        OxblendError: the engine's diagnosed preview failure.

    CLI: ``oxblend report`` (``--preview``).
    """
    # Direct native evidence is explicitly library-unbound; no ledger ID or
    # CLI invocation receipt is fabricated. Review, then call report_create.
    if isinstance(source, (str, bytes, os.PathLike)):
        return report_preview(output, report=source, include_names=include_names, include_file=include_file)
    error = source if isinstance(source, OxblendError) else None
    result = None if error is not None else source
    return ReportPreview(_invoke(_native.report_direct_preview, _path(output), error=error, result=result,
                                 include_names=include_names,
                                 include_file=None if include_file is None else _path(include_file)))

def report_preview(output: HostPath, *, report: HostPath | None = None, stdout: HostPath | None = None, trace: HostPath | None = None, trace_dir: HostPath | None = None, run_id: str | None = None, last: bool = False, include_names: bool = False, include_file: HostPath | None = None) -> ReportPreview:
    """Freeze a sanitized preview from saved CLI evidence, never publish.

    Args:
        output: the new preview bundle path.
        report: a saved JSON report file to include.
        stdout: saved stdout verified against the invocation digest.
        trace: a saved JSONL trace file to include.
        trace_dir: local directory restricting trace selection.
        run_id: the bound receipt identity selecting the trace.
        last: select the last completed failed/partial run with a bound receipt.
        include_names: explicitly opt into names in the bundle.
        include_file: explicitly opted-in whole file disclosed as a
            user-supplied attachment.

    Returns:
        The ``ReportPreview`` snapshot; review it, then call
        ``report_create`` with its digest.

    Raises:
        OxblendError: the engine's diagnosed preview failure (missing
            evidence stays explicit, never silently skipped).

    CLI: ``oxblend report`` (``--preview``).
    """
    return ReportPreview(_invoke(_native.report_preview, _path(output), None if report is None else _path(report), None if stdout is None else _path(stdout), None if trace is None else _path(trace), None if trace_dir is None else _path(trace_dir), run_id, last, include_names, None if include_file is None else _path(include_file)))

def report_create(preview: HostPath, preview_sha256: str, output: HostPath) -> ReportCreated:
    """Publish exactly the reviewed preview snapshot; never re-sanitize.

    Args:
        preview: the reviewed preview bundle path.
        preview_sha256: the exact digest of that preview; a mismatch refuses.
        output: the new published bundle path; an existing path refuses.

    Returns:
        The ``ReportCreated`` publication receipt.

    Raises:
        OxblendError: the engine's diagnosed failure (digest mismatch,
            existing output, or changed bytes).

    CLI: ``oxblend report`` (``--create``).
    """
    return ReportCreated(_invoke(_native.report_create, _path(preview), preview_sha256, _path(output)))

def report_discard(preview: HostPath, preview_sha256: str) -> ReportDiscarded:
    """Discard a reviewed preview snapshot without publishing it.

    Args:
        preview: the reviewed preview bundle path.
        preview_sha256: the exact digest of that preview; a mismatch refuses.

    Returns:
        The ``ReportDiscarded`` discard receipt.

    Raises:
        OxblendError: the engine's diagnosed failure for the discard.

    CLI: ``oxblend report`` (discard-preview).
    """
    return ReportDiscarded(_invoke(_native.report_discard, _path(preview), preview_sha256))

__all__ = [name for name in (
    'JSONValue Memory Durability Roots ReportInput CatalogReport ExplainReport InspectReport ValueReport PointerReport FactsReport CensusReport MultiRootReport ListReport OverrideReport DiffReport EditReport ExtractReport NodeTreeReport ExrHeaderReport PixelsReport TextReport ScanReport ReportPreview ReportCreated ReportDiscarded summary query catalog explain inspect payload get pointer source_facts library_facts pointer_census list_dependencies trace block_report overrides diff edit node_tree extract rewrite pack pack_sequence exr_header exr_scan exr_pixels read_path_list report report_preview report_create report_discard'
).split()]
