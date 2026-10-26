"""Typed tour of the complete Oxblend Python API.

This file is also the ``mypy --strict`` proof for the ``oxblend`` stubs: every
value below uses a declared type, an immutable report or a generated exception
class. Reports are ``Mapping`` objects, so any additive engine field stays
readable by key while documented fields keep their types.

    python examples/full_api.py reads shot.blend [more.blend ...]
    python examples/full_api.py writes shot.blend /tmp/new-report-dir
    python examples/full_api.py exr frame.exr [more.exr ...]

``reads`` and ``exr`` are read-only. ``writes`` only ever creates NEW paths and
refuses to reuse an existing destination; review each report before delivery.
"""

from __future__ import annotations

import os
import sys
from collections.abc import Iterable

import oxblend


def read_captured_root(blend: oxblend.File) -> None:
    """Checked reads from one captured Session, never an implicit reopen."""
    header = blend.header
    print("version", header.version, "pointer", header.pointer_size, "size", header.size)
    print("retention policy", blend.memory)
    for block in blend.blocks(code=b"SC"):
        queries = [
            oxblend.FieldQuery(block, 0, (b"r", b"sfra")),
            oxblend.FieldQuery(block, 0, (b"r", b"efra")),
        ]
        for value in blend.get(queries):
            # Float32Bits keeps the saved bits; .value is the decoded float.
            if value.kind == "float32":
                assert isinstance(value.value, oxblend.Float32Bits)
                print(block.index, value.value.bits, value.value.value)
            else:
                print(block.index, value.kind, value.to_json())
    print("open diagnostics", len(blend.open_diagnostics))
    # Frozen absolute lexical identities, captured together; never reopened.
    print("captured roots", blend.root_paths)


def read_typed_summary(blend: oxblend.File) -> None:
    summary = blend.summary()
    print("block codes", summary.block_codes[:3], "objects", len(summary.objects))
    for scene in summary.scenes:
        print("scene", scene.name, scene.frame_start, scene.frame_end, scene.frame_step)
        if scene.fps.base is not None:
            print(" fps bits", scene.fps.base.bits, "effective", scene.fps.effective)
    for warning in summary.warnings:
        print("warning", warning.kind, warning.field, warning.message)
    # Exact CLI projection. A byte-only (non-UTF-8) name still returns the typed
    # Summary; only this projection raises the engine `encoding` error.
    projection = summary.to_json()
    assert isinstance(projection, dict)
    print("summary json", sorted(projection))


def read_reports(blend: oxblend.File, source: str) -> None:
    query = blend.query(type="Object", fields=("type", "id.lib"))
    print("query rows", len(query.rows), "complete", query.complete)

    catalog = oxblend.catalog()
    print("catalog", catalog.catalog_revision, len(catalog.concepts), "concepts")

    explained = blend.explain("scene.compositor")
    print("explain file_aware", explained["file_aware"])

    inspected = blend.inspect()
    print("inspect blocks", len(inspected.blocks), "structs", len(inspected.sdna["structs"]))

    facts = blend.source_facts(allow_incomplete=True)
    if not facts.complete:
        # A failed pointer edge stays unknown at its own site; the typed report
        # and its separate diagnostics are both retained.
        print("facts errors", [error["kind"] for error in facts.errors])
    print("facts diagnostics", len(facts.diagnostics))

    census = blend.pointer_census(full=False)
    print("census sites", census["pointer_sites"], "verified", census.semantic_verified)

    if inspected.blocks:
        # BLOCK is the ordered index in *this* save; never reuse one from
        # another file or an earlier save.
        block_index = 0
        pointer = oxblend.pointer(source, block_index, ("r", "id"))
        print("pointer block_index", pointer.block_index)
        payload = oxblend.payload(source, block_index)
        print("payload hex chars", len(str(payload.value["hex"])))

    traced = blend.trace(recursive=True)
    if isinstance(traced, oxblend.MultiRootReport):
        # One capture can hold several roots; rows stay per root.
        print("multi-root trace rows", len(traced.root_traces))
    else:
        print("trace complete", traced.complete, "usages", len(traced.usages))
        for usage in traced.usages:
            # Hex fields are authoritative for native bytes; text is optional.
            print(" usage", usage["asset_path_hex"], usage["exists"])

    # `list` keeps the engine's actual failure kind instead of a generic
    # incomplete error: missing_asset, missing_library, pattern_members_unknown
    # or incomplete_trace, each carrying this report on `error.report`.
    listed = oxblend.list_dependencies((source,), full_info=True, allow_incomplete=True)
    print("list complete", listed.complete, "diagnostics", len(listed.diagnostics))

    text = oxblend.block_report(source, limit=3)
    print(text.to_json())  # exact CLI text; `blocks` has no JSON CLI schema

    overrides = oxblend.overrides(source, recursive=True, allow_incomplete=True)
    print("override records", len(overrides["records"]), "complete", overrides.complete)

    comparison = oxblend.diff(source, source)
    print("diff equal", comparison.equal, "count", comparison.difference_count)


def write_new_paths(blend: oxblend.File, source: str, outdir: str) -> None:
    if os.path.exists(outdir):
        raise SystemExit(f"refusing to write into an existing path: {outdir}")
    os.makedirs(outdir)

    # Strict by default: a missing/unknown dependency publishes nothing.
    packed = blend.pack(
        os.path.join(outdir, "package"),
        project_root=os.path.dirname(os.path.abspath(source)),
        dry_run=True,
        permissive=False,
        # None keeps the capture's retention policy; an explicit mode that
        # differs from it refuses as a usage error naming both modes.
        memory=None,
        durability="end",
    )
    print("pack planning", packed.planning_complete, "files", len(packed.files))
    print("pack effects", packed.directory_created, packed.archive_written)

    trees = oxblend.node_tree(source)
    print("node trees", len(trees.trees))

    preview = oxblend.report_preview(
        os.path.join(outdir, "preview.json"), report=None, include_names=False
    )
    print("preview members", len(preview["members"]))
    # Only publish a preview this session just created, after reviewing it.
    digest = str(preview["receipt_sha256"])
    discarded = oxblend.report_discard(os.path.join(outdir, "preview.json"), digest)
    print("discarded", discarded["discarded"])


def exr_tour(paths: Iterable[str]) -> None:
    for path in paths:
        header = oxblend.exr_header(path)
        print(path, "parts", len(header.parts), "verified", header.pixel_integrity_verified)
    # One sequence, summary last; `inputs` may be a single HostPath or an
    # iterable. `ScanReport` is frozen: records, summary and complete agree.
    scan = oxblend.exr_scan(paths, jobs=None, allow_incomplete=False)
    print("scan records", len(scan.records), "complete", scan.complete)
    print("scan summary", scan.summary["counts_by_status"])
    print("JSONL projection", scan.to_json())


def multi_root_tour(roots: Iterable[str]) -> None:
    paths = tuple(roots)
    if len(paths) < 2:
        return
    # One captured Session/InputStore owns every root; no arena is merged.
    with oxblend.open_many(paths, memory="fast") as blend:
        print("multi-root trace", blend.trace().complete)
        print("multi-root list", blend.list_dependencies().complete)
    aliases = oxblend.read_path_list(os.devnull, nul=False)
    print("explicit path list entries", len(aliases))
    with oxblend.open(paths[0]) as first, oxblend.open(paths[0]) as second:
        try:
            # Separately captured Files refuse, even for the same path.
            oxblend.trace([first, second])
        except oxblend.UsageError as error:
            print("refusal", error.kind, "|", error.next_steps)


def error_contract() -> None:
    kinds: dict[str, type[oxblend.OxblendError]] = oxblend.ERROR_TYPES
    # input_changed keeps the original SourceChangedError class; source_changed
    # is a distinct catalog class. Names are unique across every catalog code.
    print(kinds["input_changed"].__name__, kinds["source_changed"].__name__)
    try:
        oxblend.open("definitely-missing.blend")
    except oxblend.MissingInputError as error:
        print("missing input", error.exit_code, error.effects, len(error.diagnostics))
    except oxblend.SourceChangedError as error:
        print("source changed", error.kind, error.diagnostics)
    except oxblend.OxblendError as error:
        print("engine failure", error.kind, error.operation, error.report is None)


def main(argv: list[str]) -> int:
    if len(argv) < 2 or argv[0] not in {"reads", "writes", "exr"}:
        print(__doc__)
        return 2
    mode, rest = argv[0], argv[1:]
    if mode == "exr":
        exr_tour(rest)
        return 0
    source = rest[0]
    if mode == "writes":
        if len(rest) != 2:
            print(__doc__)
            return 2
        with oxblend.open(source) as blend:
            write_new_paths(blend, source, rest[1])
        return 0
    with oxblend.open(source) as blend:
        read_captured_root(blend)
        read_typed_summary(blend)
        read_reports(blend, source)
    multi_root_tour(rest)
    error_contract()
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
