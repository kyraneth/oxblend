#!/usr/bin/env python3
"""L20 full-binding parity harness: installed wheel versus the CLI oracle.

Authority: L20/DESIGN.md (as amended by DESIGN-AMENDMENTS.md). Every native
command that the Python binding exposes is executed twice on the same inputs:
once in-process through the public ``oxblend`` facade and once through the
installed ``oxblend`` binary. The CLI is the independent oracle; it is a
test-only subprocess and no product code path shells out.

What is asserted
----------------
* ``report.to_json()`` equals the CLI's parsed stdout exactly, including
  incomplete reports (CLI exit 2 with JSON on stdout) and the projection rule
  that strips only the report-root ``operation_diagnostics``.
* A returned report is immutable, returns *owned* JSON (mutating the returned
  value never changes the next projection), and carries ``diagnostics``
  separately from that projection.
* Documented typed properties exist, are consistent with the projected fact
  they name (booleans/identifiers/counts/arrays), and ``summary``/``get``/
  ``pointer``/``payload``/``inspect.blocks`` keep their exact value projections.
* Errors: kind, class (``ERROR_TYPES``), exit-code rule, catalog doc fields and
  ``next_command`` equal the CLI hard-error envelope for the same operation;
  the full CLI envelope (including ``diagnostic``) is preserved in evidence.
* Writers: the CLI writes a destination, the harness removes only its own
  destination, the binding writes the same destination, and the artifacts are
  compared byte-for-byte (directory trees by member set + member hashes; ZIP
  archives by bytes plus a member-level diagnosis on mismatch).

Coverage rule (no numeric caps)
-------------------------------
Every regular ``*.blend`` and ``*.exr`` file under ``--corpus`` is a target of
every file-scoped command; there is no sample size, byte cap, call cap or
timeout anywhere. Per-block/per-field commands are driven by the file's own
SDNA, never by a fixed number:

``--value-coverage layout`` (default)
    For each file, every distinct SDNA structure gets typed reads for *every*
    declared field at its first block (plus array/pointer-array/raw variants).
    Typed decoding is a function of the DNA layout, so this covers the whole
    value surface of the corpus without re-running identical layouts.

Multi-root tests use exactly two roots: two is the minimum that exercises
ordering across roots, shared-dependency deduplication and a common project path.
That is a semantic minimum, not a cap on which corpus files any file-scoped
command visits.
``--value-coverage block``
    Every block (element 0) of every file, every declared field.
``--value-coverage field``
    Every block, every element, every declared field. The maximal matrix.

Batch route (values stage)
--------------------------
Value parity opens ONE ``oxblend.File`` per input, plans the unchanged
sites()/field_variants() matrix from that File's own ``inspect``, freezes the
plan into an exact canonical-JSON fingerprint plus file witness, frees the
planner tree, then captures the CLI ONCE per input via
``read-batch FILE --requests SPEC_FILE`` with a strict
``{"schema":1,"requests":[...]}`` SPEC_FILE and stream-parses item lines
(never a huge in-RAM capture alongside the planner). The success ``inspect``
item must byte-match the witness fingerprint (a mismatch reloads the witness
for one exact whole-object diff, no field masking); every other success item's
``report`` must equal the retained-File primitive projection exactly. Each
error item is strictly ``{index,op,ok:false,error:<exact unchanged single
error>,diagnostic_ref:<verbatim null or bound pair>}`` (R17: top-level
sibling, no inside-fallback); the exact single error (kind/message/next_command
+prose, NO inside diagnostic_ref) must match the native exception through the
shared exact error helper (typed kind/class/exit/message/prose, never
kind/message-only, no key mask) with the sibling ``diagnostic_ref`` pair
validated by the shared References ledger contract.
Independent item errors may still complete with CLI exit 2 and terminal kind
``incomplete_read_batch``; a source-guard failure stops the batch, keeps its
original kind, leaves trailing requests unobserved and fails the whole input.
Request counts/order and the terminal truth
(``record_type``/``complete``/``counts``/``input``/``input_recheck``/``input_changed``
plus the stderr envelope kind and receipt) are all validated; a truncated
capture fails the whole input. The private ``source_snapshot`` is never
emitted or stored. A small sampled path/File equivalence check replaces
per-value path reopening.

Diagnostic identity (L20/DESIGN-DIAGNOSTICS-AMENDMENT.md)
---------------------------------------------------------
``failure_receipt::begin`` is CLI-only, so a CLI ``diagnostic_ref`` is null or
exactly ``{run_id, failure_id}`` with nonempty identifiers, while the equivalent
native library reference is null today. The harness asserts that rule at *every*
``diagnostic_ref`` occurrence in both projections and at the exception attribute,
never drops the field, and compares every other field exactly. The CLI's own
per-run receipt is validated structurally instead of ignored: schema, exit code,
nonempty unique failure identifiers, a resolving ``caused_by`` graph and
reference shape. Every occurrence is published as ``diagnostic-refs.json``.

Deliberate, stated exceptions
-----------------------------
* ``report`` previews derive a random per-invocation HMAC key by design
  (``src/report.rs``), so two invocations cannot produce equal bytes. Each side
  is validated as a complete snapshot of its own invocation: receipt path,
  bytes, digest and stage layout; every member's digest is checked against the
  real archive bytes once created; ``archive_sha256`` is checked against the
  produced archive; the manifest inventory is checked member by member. Only
  the members' volatile token values (32..64-hex HMAC identities), the stage
  path, ``bundle_id``, archive bytes and ``manifest.created_at`` are excluded
  from the cross-side comparison, and each exclusion is recorded in
  ``report-volatility.json``.
* ``exr-scan`` with more than one worker completes records out of order. The
  harness compares the ordered sequence at ``--jobs 1`` and, for the default
  worker count, compares the record multiset keyed by
  ``absolute_path_hex`` plus the summary.
* ``list --sha256``/``--timing`` and ``trace --summary`` are CLI text
  presentation with no report field; the harness verifies the text semantics
  directly and records that they have no binding counterpart.
* A raised ``OxblendError`` or a warning-carrying report can be handed straight
  to the report API (``oxblend.report(...)`` or ``report_preview(report=...)``),
  with no CLI ledger and no fabricated envelope. Native direct input is
  library-unbound, never invocation-verified, so its report cannot be byte-equal
  to a CLI invocation report: structural diagnostics are compared exactly, every
  difference must fall inside the declared library-unbound/provenance set, and
  the exact differences are published per label. Anything outside that set fails.

Evidence
--------
``--evidence DIR`` receives, for every call, the exact argv/exit/stdout/stderr
(blobs are content-addressed, so duplicate bytes are stored once), every
Python report/exception, the per-command coverage ledger, the corpus manifest,
preflight gaps, fixture validation and a final ``run.json``.
"""
from __future__ import annotations

import argparse
import builtins
import hashlib
import importlib
import importlib.metadata
import inspect as inspect_module
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import time
import traceback
import unittest

HERE = Path(__file__).resolve()
REPO = HERE.parents[3]  # <checkout>/  (file lives in bindings/python/tests/)
CATALOG_PATH = REPO / "docs" / "error-catalog.json"

PARSER = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
PARSER.add_argument("--cli", type=Path, required=True, help="installed oxblend binary acting as the oracle")
PARSER.add_argument("--corpus", type=Path, required=True, help="public corpus root (searched recursively)")
PARSER.add_argument("--blender", type=Path, help="Blender 5.2.1 binary, required for editor receiving proof")
PARSER.add_argument("--evidence", type=Path, required=True, help="evidence directory (harness-owned)")
PARSER.add_argument("--value-coverage", choices=("layout", "block", "field"), default="layout")
PARSER.add_argument("--stage", action="append", default=None,
                    help="run only these stages (repeatable); default: every stage")
PARSER.add_argument("--include", action="append", default=None,
                    help="operator scoping: only corpus relative paths containing this substring")
PARSER.add_argument("--preflight-only", action="store_true")
PARSER.add_argument("--fixtures-only", action="store_true")
PARSER.add_argument("--keep-work", action="store_true")
ARGS = PARSER.parse_args()

STAGES = ("catalog", "read", "values", "trace", "diff", "write", "exr", "errors", "lifecycle", "synthetic",
          "direct-report")


# --------------------------------------------------------------------------- #
# Evidence store
# --------------------------------------------------------------------------- #

def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def fs_text(path: Path) -> str:
    return os.fsdecode(os.fspath(path))


def json_bytes(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(",", ":")).encode()


class Store:
    """Content-addressed evidence bytes plus per-call metadata."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.calls = root / "calls"
        self.blobs = root / "blobs"
        self.calls.mkdir(parents=True, exist_ok=True)
        self.blobs.mkdir(parents=True, exist_ok=True)
        self.seq = 0
        self.blob_count = 0
        self.call_count = 0
        self.bytes_out = 0

    def blob(self, data: bytes) -> dict:
        digest = sha256_bytes(data)
        target = self.blobs / digest
        if not target.exists():
            target.write_bytes(data)
            self.blob_count += 1
            self.bytes_out += len(data)
        return {"sha256": digest, "bytes": len(data), "blob": f"blobs/{digest}"}

    def call(self, kind: str, record: dict, stdout: bytes = b"", stderr: bytes = b"") -> dict:
        self.seq += 1
        ident = f"{self.seq:07d}"
        record = dict(record)
        record["seq"] = self.seq
        record["kind"] = kind
        if stdout:
            record["stdout"] = self.blob(stdout)
        if stderr:
            record["stderr"] = self.blob(stderr)
        directory = self.calls / f"{ident}-{safe_name(kind)}"
        directory.mkdir(parents=True, exist_ok=True)
        (directory / "meta.json").write_bytes(json_bytes(record))
        self.call_count += 1
        return record

    def write(self, name: str, value: object) -> Path:
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(json_bytes(value) if not isinstance(value, (bytes, str)) else
                         (value.encode() if isinstance(value, str) else value))
        return path


STORE: Store


# Platform path components are limited by NAME_MAX; a longer logical name is
# replaced by its digest and the mapping is published as name-map.json so no
# evidence is lost. Nothing else truncates: the only other limits in this file
# are fixed byte widths of the .blend/EXR formats themselves.
NAME_MAX_BYTES = 200  # leave room for the "<seq>-" prefix and ".json" suffix
NAME_MAP: dict[str, str] = {}


def safe_name(text: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9_.-]", "-", text).strip("-") or "unnamed"
    if len(cleaned.encode("utf-8", "surrogateescape")) <= NAME_MAX_BYTES:
        return cleaned
    digest = hashlib.sha256(text.encode("utf-8", "surrogateescape")).hexdigest()[:40]
    NAME_MAP[digest] = text
    return digest


class Ledger:
    def __init__(self) -> None:
        self.rows: list[dict] = []
        self.counts: dict[str, dict[str, int]] = {}

    def add(self, command: str, target: str, variant: str, status: str, note: str | None) -> None:
        row = {"command": command, "target": target, "variant": variant, "status": status}
        if note:
            row["note"] = note
        self.rows.append(row)
        bucket = self.counts.setdefault(command, {"ok": 0, "fail": 0})
        bucket["ok" if status == "ok" else "fail"] += 1

    def summary(self) -> dict:
        return {"counts": self.counts, "rows": len(self.rows)}


LEDGER = Ledger()


# --------------------------------------------------------------------------- #
# CLI oracle
# --------------------------------------------------------------------------- #

class Result:
    __slots__ = ("argv", "exit", "stdout", "stderr", "seconds", "seq")

    def __init__(self, argv: list[str], exit: int, stdout: bytes, stderr: bytes, seconds: float, seq: int) -> None:
        self.argv, self.exit, self.stdout, self.stderr, self.seconds, self.seq = argv, exit, stdout, stderr, seconds, seq


class Oracle:
    def __init__(self, binary: Path) -> None:
        self.binary = binary
        if not binary.is_file():
            raise SystemExit(f"CLI oracle is not a file: {binary}")
        if not os.access(binary, os.X_OK):
            raise SystemExit(f"CLI oracle is not executable: {binary}")
        self.env = {k: v for k, v in os.environ.items()
                    if not k.startswith("FLATTRACE")}
        self.env["PYTHONDONTWRITEBYTECODE"] = "1"
        self.sha256 = sha256_file(binary)
        self.version_output = b""
        self.commands: frozenset[str] = frozenset()

    def verify(self) -> None:
        result = self("--version")
        if result.exit != 0:
            raise SystemExit(f"CLI oracle --version failed ({result.exit}): {result.stderr!r}")
        self.version_output = result.stdout
        listing = self("--help")
        if listing.exit != 0:
            raise SystemExit(f"CLI oracle --help failed ({listing.exit}): {listing.stderr!r}")
        text = listing.stdout.decode("utf-8", "replace")
        marker = text.rsplit("Commands:", 1)
        if len(marker) != 2:
            raise SystemExit("CLI oracle --help does not publish its command set")
        self.commands = frozenset(marker[1].split())

    def supports(self, command: str) -> bool:
        return command in self.commands

    @property
    def version(self) -> str:
        text = self.version_output.decode("utf-8", "replace").strip()
        return text[len("oxblend "):] if text.startswith("oxblend ") else text

    def __call__(self, *argv: object, stdin: bytes | None = None) -> Result:
        args = [fs_text(self.binary)] + [a if isinstance(a, str) else fs_text(Path(a)) for a in argv]
        started = time.monotonic()
        completed = subprocess.run(args, capture_output=True, stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
                                   input=stdin, env=self.env)
        seconds = time.monotonic() - started
        record = STORE.call("cli", {"argv": args, "argv_hex": [a.encode("utf-8", "surrogateescape").hex() for a in args],
                                    "exit": completed.returncode, "seconds": seconds},
                            completed.stdout, completed.stderr)
        return Result(args, completed.returncode, completed.stdout, completed.stderr, seconds, record["seq"])


# --------------------------------------------------------------------------- #
# Python facade invocation
# --------------------------------------------------------------------------- #

def thaw(value: object) -> object:
    """Convert a binding value to plain JSON exactly through to_json()."""
    if hasattr(value, "to_json"):
        return value.to_json()
    if isinstance(value, (tuple, list)):
        return [thaw(v) for v in value]
    raise AssertionError(f"no JSON projection for {type(value)!r}")


class Call:
    __slots__ = ("seq", "ok", "report", "object", "kind", "exit_code", "error", "exception", "msg", "seconds", "gap", "phase")

    def __init__(self, **kw: object) -> None:
        for slot in self.__slots__:
            setattr(self, slot, kw.get(slot))


GAP_PREFIX = "module 'oxblend._native' has no attribute"


def py_call(name: str, fn) -> Call:
    started = time.monotonic()
    try:
        result = fn()
    except Exception as exc:  # noqa: BLE001 - evidence must retain every failure shape
        seconds = time.monotonic() - started
        message = str(exc)
        gap = isinstance(exc, AttributeError) and GAP_PREFIX in message
        ox_error = hasattr(exc, "kind")
        record = {
            "argv": ["python", name],
            "exception": type(exc).__name__,
            "message": message,
            "traceback": traceback.format_exc(),
            "kind": getattr(exc, "kind", None),
            "exit_code": getattr(exc, "exit_code", None),
            "operation": getattr(exc, "operation", None),
            "effects": getattr(exc, "effects", None),
            "gap": gap,
            "seconds": seconds,
        }
        report = getattr(exc, "report", None)
        report_json = None
        if report is not None:
            try:
                report_json = thaw(report)
            except Exception as projection:  # noqa: BLE001
                record["report_projection_error"] = f"{type(projection).__name__}: {projection}"
        record["report"] = report_json
        record["explanation"] = getattr(exc, "explanation", None)
        record["likely_causes"] = list(getattr(exc, "likely_causes", ()) or ())
        record["next_steps"] = list(getattr(exc, "next_steps", ()) or ())
        record["known_issue"] = getattr(exc, "known_issue", None)
        record["next_command"] = _safe(getattr(exc, "next_command", None))
        record["diagnostics"] = _safe(getattr(exc, "diagnostics", ()))
        seq = STORE.call("python-error", record)["seq"]
        return Call(seq=seq, ok=False, report=report_json, object=report, kind=getattr(exc, "kind", None),
                    exit_code=getattr(exc, "exit_code", None), error=ox_error, exception=exc,
                    msg=message, seconds=seconds, gap=gap, phase="call")
    seconds = time.monotonic() - started
    record = {"argv": ["python", name], "ok": True, "type": type(result).__name__, "seconds": seconds}
    try:
        projected = thaw(result)
    except Exception as projection:  # noqa: BLE001
        record["projection_error"] = f"{type(projection).__name__}: {projection}"
        record["kind"] = getattr(projection, "kind", None)
        record["exit_code"] = getattr(projection, "exit_code", None)
        seq = STORE.call("python-projection-error", record)["seq"]
        return Call(seq=seq, ok=False, report=None, object=result, kind=getattr(projection, "kind", None),
                    exit_code=getattr(projection, "exit_code", None),
                    error=hasattr(projection, "kind"), exception=projection, msg=str(projection),
                    seconds=time.monotonic() - started, gap=False, phase="to_json")
    record["report"] = projected
    if hasattr(result, "diagnostics"):
        record["diagnostics"] = _safe(result.diagnostics)
    seq = STORE.call("python", record)["seq"]
    return Call(seq=seq, ok=True, report=projected, object=result, kind=None, exit_code=None,
                error=False, exception=None, msg=None, seconds=time.monotonic() - started, gap=False, phase="call")


def _safe(value: object) -> object:
    try:
        if hasattr(value, "to_json"):
            return value.to_json()
        if isinstance(value, (tuple, list)):
            return [_safe(v) for v in value]
        if isinstance(value, dict):
            return {k: _safe(v) for k, v in value.items()}
        return value
    except Exception as exc:  # noqa: BLE001
        return f"<unprojectable {type(exc).__name__}: {exc}>"


# --------------------------------------------------------------------------- #
# JSON comparison
# --------------------------------------------------------------------------- #

class _Missing:
    """A stable sentinel: evidence must not contain a process-specific repr."""

    def __repr__(self) -> str:
        return "<missing>"


MISSING = _Missing()


REFERENCE = "diagnostic_ref"
REFERENCE_FIELDS = ("run_id", "failure_id")


class References:
    """The approved diagnostic-identity exception, asserted at every occurrence.

    ``failure_receipt::begin`` is CLI-only, so a CLI reference is a real
    ``{run_id,failure_id}`` while the equivalent native library reference is
    null. Every other field is compared exactly; ordinary per-run identifiers
    are validated structurally (see ``assert_cli_receipt``) instead of ignored.
    """

    def __init__(self) -> None:
        self.rows: list[dict] = []
        self.problems = 0

    def check(self, label: str, path: str, cli_value: object, py_value: object) -> list[tuple[str, object, object]]:
        found: list[tuple[str, object, object]] = []
        where = f"{path}.{REFERENCE}"
        if cli_value is not None:
            ok = (isinstance(cli_value, dict) and set(cli_value) == set(REFERENCE_FIELDS)
                  and all(isinstance(cli_value[f], str) and cli_value[f] for f in REFERENCE_FIELDS))
            if not ok:
                found.append((where, cli_value,
                              "CLI diagnostic_ref must be null or exactly {run_id,failure_id} "
                              "with nonempty identifiers"))
        if py_value is not None:
            found.append((where, py_value,
                          "the native library reference must be null today "
                          "(L20/DESIGN-DIAGNOSTICS-AMENDMENT.md; no process-wide ledger in Python)"))
        self.problems += len(found)
        self.rows.append({"label": label, "path": path, "cli": cli_value, "python": py_value,
                          "conforms": not found})
        return found

    def check_cli_only(self, label: str, path: str, value: object) -> list[tuple[str, object, object]]:
        """A CLI-only site (the receipt) with no native counterpart to compare."""
        found: list[tuple[str, object, object]] = []
        if value is not None:
            ok = (isinstance(value, dict) and set(value) == set(REFERENCE_FIELDS)
                  and all(isinstance(value[f], str) and value[f] for f in REFERENCE_FIELDS))
            if not ok:
                found.append((path, value,
                              "a CLI diagnostic_ref must be null or exactly {run_id,failure_id} "
                              "with nonempty identifiers"))
        self.problems += len(found)
        self.rows.append({"label": label, "path": path, "cli": value,
                          "python": "absent (CLI-only receipt site)", "conforms": not found})
        return found

    def missing(self, label: str, path: str, side: str, value: object) -> None:
        self.problems += 1
        self.rows.append({"label": label, "path": path, "missing_on": side, "value": value,
                          "conforms": False})

    def summary(self) -> dict:
        return {"occurrences": len(self.rows), "nonconforming": self.problems}


REFS = References()


def _reference_sites(node: object, path: str = "$"):
    """Every ``diagnostic_ref`` key in a JSON value, at any depth."""
    if isinstance(node, dict):
        for key, value in node.items():
            child = f"{path}.{key}"
            if key == REFERENCE:
                yield child, value
            else:
                yield from _reference_sites(value, child)
    elif isinstance(node, list):
        for index, value in enumerate(node):
            yield from _reference_sites(value, f"{path}[{index}]")


def json_diff(a: object, b: object, path: str = "$", label: str = "-") -> list[tuple[str, object, object]]:
    found: list[tuple[str, object, object]] = []
    if type(a) is not type(b):
        found.append((path, a, b))
        return found
    if isinstance(a, dict):
        for key in a.keys() - b.keys():
            if key == REFERENCE:
                REFS.missing(label, path, "python", a[key])
            found.append((f"{path}.{key}", a[key], MISSING))
        for key in b.keys() - a.keys():
            if key == REFERENCE:
                REFS.missing(label, path, "cli", b[key])
            found.append((f"{path}.{key}", MISSING, b[key]))
        for key in a.keys() & b.keys():
            if key == REFERENCE:
                found.extend(REFS.check(label, path, b[key], a[key]))
                continue
            found.extend(json_diff(a[key], b[key], f"{path}.{key}", label))
    elif isinstance(a, list):
        if len(a) != len(b):
            found.append((f"{path}.length", len(a), len(b)))
        for index, (left, right) in enumerate(zip(a, b)):
            found.extend(json_diff(left, right, f"{path}[{index}]", label))
    elif isinstance(a, float) and isinstance(b, float):
        if a != b and not (a != a and b != b):
            found.append((path, a, b))
    elif a != b:
        found.append((path, a, b))
    return found


def exact(value: object) -> str:
    """Full repr: differences are reported whole, never abbreviated."""
    return repr(value)


def parse_json_stdout(result: Result) -> object:
    if not result.stdout.strip():
        return MISSING
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return MISSING


def policy_stderr_identity_pair(base: bytes, variant: bytes) -> tuple[str, str]:
    """R19: account ONLY the outer receipt's real fresh run_id; all other bytes exact.

    These are two invocations of the SAME binary. Even build provenance must
    remain identical. Never mask warning references or arbitrary nested IDs.
    """
    envelopes = [hard_error_envelope(data) for data in (base, variant)]
    if any(not isinstance(e, dict) or not isinstance(e.get("diagnostic"), dict)
           for e in envelopes):
        raise AssertionError("changed policy stderr lacks an actual failure receipt")
    ids = [e["diagnostic"].get("run_id") for e in envelopes]
    if any(not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{32}", value) is None
           for value in ids) or ids[0] == ids[1]:
        raise AssertionError("policy receipt IDs must be actual well-formed fresh invocation IDs")
    tokens = [b'"run_id":"' + value.encode("ascii") + b'"' for value in ids]
    if base.count(tokens[0]) != 1 or variant.count(tokens[1]) != 1:
        raise AssertionError("policy run_id must occur exactly once; nested identity needs explicit accounting")
    if base.replace(tokens[0], tokens[1], 1) != variant:
        raise AssertionError("policy stderr differs outside the validated outer receipt run_id")
    # Prove the changed byte token actually names that outer field, not another
    # occurrence: every envelope leaf except this exact field must agree too.
    envelopes[0]["diagnostic"]["run_id"] = ids[1]
    if envelopes[0] != envelopes[1]:
        raise AssertionError("policy error envelope differs outside $.diagnostic.run_id")
    return ids[0], ids[1]


def hard_error_envelope(stderr: bytes) -> dict | None:
    """The CLI's final schema-1 envelope; earlier lines may be warnings."""
    for line in reversed(stderr.splitlines()):
        line = line.strip()
        if not line.startswith(b"{"):
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and isinstance(value.get("error"), dict):
            return value
    return None


HEX64 = re.compile(r"^[0-9a-f]{64}$")
KIND_TOKEN = re.compile(r"^kind:[0-9a-f]{64}$")
KIND_TOKEN_RUN = re.compile(r"kind:[0-9a-f]{64}(?![0-9a-f])")
BUNDLE_RE = re.compile(r"^[0-9a-f]{64}$")
STAGE_DIR_RE = re.compile(r"^\.oxblend-report-[0-9a-f]{64}$")


def _is_kind_token(value: object) -> bool:
    return isinstance(value, str) and KIND_TOKEN.fullmatch(value) is not None


class TokenAlias:
    """Bijective alpha-renaming for exactly ``kind:<64hex>`` HMAC tokens.

    Only whole-string tokens and ``kind:<64hex>`` substrings inside free text
    are renamed. Arbitrary hex (file_hex, block_code_hex, addresses,
    digests) is never touched and is compared exactly.
    """

    def __init__(self) -> None:
        self.native_to_cli: dict[str, str] = {}
        self.cli_to_native: dict[str, str] = {}

    def observe(self, native_token: str, cli_token: str, path: str) -> None:
        assert KIND_TOKEN.match(native_token), f"{path}: not a kind token: {native_token!r}"
        assert KIND_TOKEN.match(cli_token), f"{path}: not a kind token: {cli_token!r}"
        known_cli = self.native_to_cli.get(native_token)
        known_native = self.cli_to_native.get(cli_token)
        if known_cli is not None and known_cli != cli_token:
            raise AssertionError(
                f"{path}: native token {native_token} already aliases "
                f"{known_cli}, not {cli_token} (topology mismatch)")
        if known_native is not None and known_native != native_token:
            raise AssertionError(
                f"{path}: cli token {cli_token} already aliases "
                f"{known_native}, not {native_token} (topology mismatch)")
        self.native_to_cli[native_token] = cli_token
        self.cli_to_native[cli_token] = native_token

    def check_bijection(self) -> None:
        assert len(self.native_to_cli) == len(self.cli_to_native)


# --------------------------------------------------------------------------- #
# Corpus and context
# --------------------------------------------------------------------------- #

class Ctx:
    def __init__(self, module, oracle: Oracle, corpus_root: Path, work: Path) -> None:
        self.ox = module  # None until the installed binding imports
        self.oracle = oracle
        self.corpus_root = corpus_root.absolute()
        self.work = work
        self.renamed: list[dict] = []
        self.inputs: list[dict] = []
        self.exrs: list[Path] = []
        self.failures: list[str] = []
        self.gaps: list[str] = []
        self.owned: set[Path] = set()
        self._headers: dict[Path, dict] = {}

    # -- corpus ------------------------------------------------------------
    def discover(self) -> None:
        if not self.corpus_root.is_dir():
            raise SystemExit(f"corpus root is not a directory: {self.corpus_root}")
        filters = ARGS.include or []
        blend_paths: list[Path] = []
        exr_paths: list[Path] = []
        for directory, subdirs, files in os.walk(self.corpus_root, followlinks=False):
            subdirs.sort()
            for name in sorted(files):
                if "provenance" in name.lower() or any("provenance" in part.lower() for part in Path(directory).parts):
                    continue
                path = Path(directory) / name
                if path.is_symlink() or not path.is_file():
                    continue
                if filters and not any(f in fs_text(path) for f in filters):
                    continue
                if name.endswith(".blend"):
                    blend_paths.append(path)
                elif name.endswith(".exr"):
                    exr_paths.append(path)
        if filters and not blend_paths and not exr_paths:
            PARSER.error("--include selected no .blend or .exr inputs; refusing vacuous qualification")
        # Hashing the corpus is deferred until the manifest is required, so
        # preflight and fixture validation do not read the whole corpus.
        self.inputs = [{"path": p, "relative": fs_text(p.relative_to(self.corpus_root)),
                        "bytes": p.stat().st_size} for p in blend_paths]
        self.exrs = exr_paths

    def blends(self) -> list[Path]:
        return [row["path"] for row in self.inputs]

    def small_blend(self) -> Path:
        return min(self.blends(), key=lambda p: p.stat().st_size)

    def header_for(self, path: Path) -> dict:
        # Corpus inputs stay read-only for this run. The engine alone parses
        # both legacy and format-1 headers; unknown facts never mean old.
        if path not in self._headers:
            result = self.cli('summary', path)
            if result.exit != 0:
                raise AssertionError(f'header facts unavailable for {path}: exit {result.exit}')
            document = parse_json_stdout(result)
            header = document.get('header') if isinstance(document, dict) else None
            if (not isinstance(header, dict) or type(header.get('version')) is not int
                    or type(header.get('file_format')) is not int):
                raise AssertionError(f'CLI did not establish header facts for {path}')
            self._headers[path] = dict(header)
        return dict(self._headers[path])

    def by_version(self, minimum: int) -> list[Path]:
        return [path for path in self.blends() if self.header_for(path)['version'] >= minimum]

    # -- calls -------------------------------------------------------------
    def cli(self, *argv: object, stdin: bytes | None = None) -> Result:
        return self.oracle(*argv, stdin=stdin)

    def py(self, name: str, fn) -> Call:
        return py_call(name, fn)

    def claim(self, name: str) -> Path:
        """Reserve a fresh destination. Corpus files can share a basename, so a
        taken name is disambiguated deterministically and the substitution is
        recorded rather than silently reusing or overwriting a destination."""
        target = self.work / "out" / name
        requested = target
        counter = 1
        while target.exists() or target.is_symlink() or target in self.owned:
            counter += 1
            target = requested.with_name(f"{requested.stem}-{counter}{requested.suffix}")
        if target != requested:
            self.renamed.append({"requested": fs_text(requested.relative_to(self.work)),
                                 "reserved": fs_text(target.relative_to(self.work))})
        target.parent.mkdir(parents=True, exist_ok=True)
        self.owned.add(target)
        return target

    def claim_fresh(self, name: str) -> Path:
        target = self.work / "out" / name
        self.remove_owned(target)
        target.parent.mkdir(parents=True, exist_ok=True)
        self.owned.add(target)
        return target

    def remove_owned(self, target: Path) -> None:
        target = Path(target)
        if not (target.exists() or target.is_symlink()):
            return
        resolved = target.resolve()
        if not resolved.is_relative_to(self.work.resolve()):
            raise AssertionError(f"refusing to remove a path outside the harness work area: {resolved}")
        if target.is_dir() and not target.is_symlink():
            shutil.rmtree(target)
        else:
            target.unlink()

    def record_failure(self, label: str, detail: str) -> None:
        self.failures.append(f"{label}: {detail}")
        STORE.write(f"failures/{len(self.failures):05d}-{safe_name(label)}.json",
                    {"label": label, "detail": detail})


# --------------------------------------------------------------------------- #
# Test helpers (shared by every TestCase)
# --------------------------------------------------------------------------- #

class ParityCase(unittest.TestCase):
    maxDiff = None

    def setUp(self) -> None:
        self.ctx.current_test = self.id()
        self.progress(phase='test_start', done=0, total=len(self.ctx.inputs))

    def tearDown(self) -> None:
        self.progress(phase='test_end')

    def progress(self, **fields) -> None:
        print('L20_PROGRESS ' + json.dumps({'test': self.id(), **fields}),
              file=sys.stderr, flush=True)

    def inputs(self):
        total = len(self.ctx.inputs)
        for index, row in enumerate(self.ctx.inputs):
            self.progress(phase='input_start', done=index, total=total, input=row['relative'])
            try:
                yield row
            finally:
                self.progress(phase='input_end', done=index + 1, total=total, input=row['relative'])

    def input_paths(self, paths):
        paths = list(paths)
        for index, path in enumerate(paths):
            self.progress(phase='input_start', done=index, total=len(paths), input=fs_text(path))
            try:
                yield path
            finally:
                self.progress(phase='input_end', done=index + 1, total=len(paths), input=fs_text(path))

    @property
    def ctx(self) -> Ctx:
        return CTX

    @property
    def ox(self):
        return CTX.ox

    # A single ledger row per (command, target, variant) attempt.
    def row(self, command: str, target: str, variant: str, body) -> None:
        try:
            body()
        except BaseException as exc:  # noqa: BLE001 - recorded then re-raised for unittest
            LEDGER.add(command, target, variant, "fail", f"{type(exc).__name__}: {exc}")
            raise
        LEDGER.add(command, target, variant, "ok", None)

    def assert_same(self, label: str, py_value: object, cli_value: object) -> None:
        if cli_value is MISSING:
            self.fail(f"{label}: CLI printed no JSON but Python produced a report")
        diffs = json_diff(py_value, cli_value, label=label)
        if diffs:
            STORE.write(f"diffs/{len(CTX.failures):05d}.json",
                        {"label": label, "count": len(diffs),
                         "diffs": [{"path": p, "python": exact(a), "cli": exact(b)} for p, a, b in diffs]})
            shown = "; ".join(f"{p}: python={exact(a)} cli={exact(b)}" for p, a, b in diffs)
            CTX.record_failure(label, f"{len(diffs)} JSON difference(s): {shown}")
            self.fail(f"{label}: {len(diffs)} JSON difference(s): {shown}")

    def assert_report_contract(self, label: str, call: Call) -> None:
        """Owned, stable, immutable projection with diagnostics kept apart."""
        report = call.object
        self.assertIsNotNone(report)
        self.assertTrue(hasattr(report, "to_json"), f"{label}: report has no to_json")
        first = call.report
        second = thaw(report)
        self.assertEqual(first, second, f"{label}: to_json is not stable")
        try:
            json.dumps(first, allow_nan=False)
        except (TypeError, ValueError) as exc:
            self.fail(f"{label}: to_json is not owned JSON ({exc})")
        if isinstance(first, dict):
            probe = dict(first)
            probe["__harness_probe__"] = 1
            self.assertNotIn("__harness_probe__", thaw(report), f"{label}: to_json aliases internal state")
            self.assertEqual(thaw(report), second, f"{label}: projection changed after caller mutation")
        elif isinstance(first, list):
            first.append("__harness_probe__")
            self.assertEqual(thaw(report), second, f"{label}: projection aliases internal state")
        if isinstance(report, self.ox.Record):
            self.assertNotIn("operation_diagnostics", first, f"{label}: diagnostics leaked into projection")
            self.assertIsInstance(report.diagnostics, tuple)
        with self.assertRaises((TypeError, AttributeError)):
            setattr(report, "__harness_probe__", 1)

    def assert_admitted_refusal(self, label: str, report, kind: str | None,
                                cli_exit: int, command: str) -> None:
        admitted = str(kind).startswith('incomplete_')
        if command == 'list':
            admitted |= kind in {'missing_asset', 'missing_library', 'pattern_members_unknown'}
        if command == 'extract':
            admitted |= kind == 'extract_refusal'
        self.assertTrue(admitted, f'{label}: CLI failed ({kind}) but Python succeeded')
        self.assertEqual(cli_exit, 2, f'{label}: admitted incomplete report must retain CLI exit 2')
        if isinstance(report, dict) and 'complete' in report:
            self.assertFalse(report['complete'], f'{label}: refusal report claims complete')

    def parity(self, label: str, fn, argv: tuple, *, expect_exit: int | None = None,
               report_contract: bool = True) -> tuple[Result, Call]:
        """Compare one CLI invocation with one Python call for the same input."""
        cli = self.ctx.cli(*argv)
        call = self.ctx.py(label, fn)
        cli_json = parse_json_stdout(cli)
        envelope = hard_error_envelope(cli.stderr)
        cli_kind = None if envelope is None else envelope["error"].get("kind")
        if call.ok:
            self.assert_same(label, call.report, cli_json)
            if report_contract:
                self.assert_report_contract(label, call)
            if envelope is not None:
                # The one intended asymmetry: the CLI refuses every incomplete
                # report, while the binding returns it under allow_incomplete.
                self.assert_admitted_refusal(label, call.report, cli_kind, cli.exit, argv[0])
            if expect_exit is not None:
                self.assertEqual(cli.exit, expect_exit, f"{label}: CLI exit {cli.exit} != {expect_exit}")
        else:
            if call.gap:
                CTX.gaps.append(f"{label}: {call.msg}")
            if cli.exit == 0 and cli_kind is None:
                self.fail(f"{label}: Python raised {call.exception!r} but the CLI succeeded")
            if cli_json is not MISSING and call.report is not None:
                self.assert_same(label + " [error report]", call.report, cli_json)
            if cli_kind is not None:
                self.assertEqual(call.kind, cli_kind, f"{label}: kind {call.kind!r} != CLI {cli_kind!r}")
                self.assert_error_matches_envelope(label, call, envelope, cli)
            else:
                self.assertIsNotNone(call.kind, f"{label}: non-CLI exception without an Oxblend kind")
            if expect_exit is not None and call.exit_code is not None:
                self.assertEqual(call.exit_code, expect_exit, f"{label}: Python exit_code != {expect_exit}")
        return cli, call

    def assert_native_matches_error_object(self, label: str, exc, error: dict,
                                           expected_exit: int = 2) -> None:
        """Shared exact native-vs-public-error comparison (never kind/message-only).

        R17 strict: the public single error is EXACT kind/message/next_command
        +prose with NO inside diagnostic_ref. All canonical fields must be
        present and equal (no key mask). Callers validate the sibling
        diagnostic_ref/receipt contract separately on top.
        """
        self.assertNotIn("diagnostic_ref", error,
                         f"{label}: single error must not carry inside diagnostic_ref "
                         f"(R17 sibling-only, no inside fallback)")
        for field in ("kind", "message", "explanation", "likely_causes",
                      "next_steps", "known_issue", "next_command"):
            self.assertIn(field, error,
                          f"{label}: canonical error drops {field} (no key mask, R17)")
        kind = error["kind"]
        self.assertEqual(getattr(exc, "kind", None), kind,
                         f"{label}: native kind differs from the public error object")
        expected_class = self.ox.ERROR_TYPES.get(kind)
        self.assertIsNotNone(expected_class, f"{label}: {kind!r} is not in ERROR_TYPES")
        self.assertIsInstance(exc, expected_class, f"{label}: wrong exception class")
        self.assertEqual(exc.exit_code, expected_exit,
                         f"{label}: exit_code does not follow the documented rule")
        # The CLI owns the entire canonical field inventory, including future
        # additions. Compare every field with strict JSON leaf types; never let
        # a fixed hand-picked list silently omit a new field or bool/int drift.
        actual = {}
        for field in error:
            self.assertTrue(hasattr(exc, field),
                            f"{label}: native error lacks canonical field {field!r}")
            actual[field] = _safe(getattr(exc, field))
        self.assert_same(label + " [full canonical error]", actual, error)

    def assert_error_matches_envelope(self, label: str, call: Call, envelope: dict,
                                      cli: Result | None = None) -> None:
        error = envelope["error"]
        # L20/DESIGN-DIAGNOSTICS-AMENDMENT.md: the exception's own reference is
        # null in-process today, because failure_receipt::begin is CLI-only.
        native_reference = getattr(call.exception, "diagnostic_ref", MISSING)
        self.assertIsNot(native_reference, MISSING,
                         f"{label}: {call.kind} exception exposes no diagnostic_ref attribute")
        REFS.rows.append({"label": label, "path": "$.exception.diagnostic_ref",
                          "cli": "absent by design (a CLI error carries its receipt, not a reference)",
                          "python": native_reference, "conforms": native_reference is None})
        if native_reference is not None:
            REFS.problems += 1
            self.fail(f"{label}: native diagnostic_ref must be null today, got {exact(native_reference)}")
        if cli is not None:
            self.assert_cli_receipt(label, cli, envelope)
        self.assert_native_matches_error_object(
            label, call.exception, error,
            expected_exit=(3 if call.kind in ("pack_input", "missing_input")
                           else 5 if call.kind == "pack_project" else 2))

    def assert_authored_pixels(self, label: str, produced: bytes) -> None:
        """The recovered stream is the authored pixel data, exactly.

        Both defined orders are accepted so the check proves the values without
        assuming an order the binary does not produce; which order was produced
        is recorded, including a disagreement with the documented one.
        """
        documented = exr_pixel_bytes(FIXTURES.red, FIXTURES.green)
        on_disk = exr_pixel_bytes(FIXTURES.red, FIXTURES.green, layout="on-disk")
        self.assertIn(produced, (documented, on_disk),
                      f"{label}: recovered samples are not the authored pixels in either "
                      f"defined order: {produced.hex()}")
        STORE.write("exr-layout.json", {
            "documented_layout": produced == documented,
            "on_disk_channel_order": produced == on_disk,
            "note": ("docs/USAGE.md and src/exr_pixels.rs describe pixel-interleaved output; "
                     "the binary emits row-major channel-planar samples. Parity is unaffected "
                     "(both sides share the engine); recorded as a documentation/implementation "
                     "disagreement, not silently blessed.")})

    def two_roots(self) -> list[Path]:
        """The minimum input set that exercises multi-root semantics: ordering
        across roots, deduplication of a shared dependency and a common project
        path. This is a semantic minimum, not a corpus cap — every file-scoped
        command still runs over every corpus input in its own test."""
        return [row["path"] for row in self.ctx.inputs[:2]]

    def assert_aliased_equal(self, cli_value: object, py_value: object,
                             alias: TokenAlias, path: str) -> None:
        """Exact comparison with bijective kind-token renaming only.

        Only ``kind:<64hex>`` occurrences are paired in order; every literal
        segment between them is compared exactly, so arbitrary hex, addresses
        and digests are never masked. Primitive types are strict
        (``bool`` is not ``int``). Whole-string kind tokens are observed
        through the shared ``TokenAlias`` to preserve cross-site topology.
        """
        # Strict None handling.
        if cli_value is None or py_value is None:
            self.assertTrue(cli_value is None and py_value is None,
                            f"{path}: None mismatch: python={py_value!r} cli={cli_value!r}")
            return
        # Whole-string kind tokens: observe bijection.
        if _is_kind_token(cli_value) or _is_kind_token(py_value):
            self.assertTrue(_is_kind_token(cli_value),
                            f"{path}: CLI not a kind token: {cli_value!r}")
            self.assertTrue(_is_kind_token(py_value),
                            f"{path}: Python not a kind token: {py_value!r}")
            alias.observe(py_value, cli_value, path)  # type: ignore[arg-type]
            return
        # Strings: pair occurrences in order, compare literal segments exactly.
        if isinstance(cli_value, str) and isinstance(py_value, str):
            py_hits = KIND_TOKEN_RUN.findall(py_value)
            cli_hits = KIND_TOKEN_RUN.findall(cli_value)
            if not py_hits and not cli_hits:
                self.assertEqual(py_value, cli_value, path)
                return
            self.assertEqual(len(py_hits), len(cli_hits),
                             f"{path}: kind-token count differs: python={py_hits} cli={cli_hits}")
            for native_tok, cli_tok in zip(py_hits, cli_hits):
                alias.observe(native_tok, cli_tok, path)
            py_parts = KIND_TOKEN_RUN.split(py_value)
            cli_parts = KIND_TOKEN_RUN.split(cli_value)
            self.assertEqual(py_parts, cli_parts,
                             f"{path}: non-token literal segments differ")
            return
        # Dicts and lists recurse; lengths and keys exact.
        if isinstance(cli_value, dict) and isinstance(py_value, dict):
            self.assertEqual(sorted(cli_value), sorted(py_value), f"{path}: keys differ")
            for key in cli_value:
                self.assert_aliased_equal(cli_value[key], py_value[key], alias,
                                          f"{path}.{key}")
            return
        if isinstance(cli_value, list) and isinstance(py_value, list):
            self.assertEqual(len(cli_value), len(py_value), f"{path}.length differs")
            for index, (left, right) in enumerate(zip(cli_value, py_value)):
                self.assert_aliased_equal(left, right, alias, f"{path}[{index}]")
            return
        # Strict primitives: bool is not int.
        self.assertTrue(type(cli_value) is type(py_value),
                        f"{path}: type mismatch: python={type(py_value).__name__} "
                        f"({py_value!r}) cli={type(cli_value).__name__} ({cli_value!r})")
        self.assertEqual(py_value, cli_value, path)

    def assert_cli_receipt(self, label: str, cli: Result, envelope: dict) -> None:
        """Validate the CLI's per-run identifiers instead of ignoring them.

        The receipt is the CLI's own ledger artifact and has no native
        counterpart, so it is not byte-compared; its structural claims are
        checked here and its whole body is retained in the call evidence.
        """
        receipt = envelope.get("diagnostic")
        if receipt is None:
            return
        self.assertEqual(receipt.get("schema"), "oxblend.failure-receipt/v1", f"{label}: receipt schema")
        self.assertEqual(receipt.get("exit_code"), cli.exit, f"{label}: receipt exit_code != process exit")
        run_id = receipt.get("run_id")
        self.assertIsInstance(run_id, str, f"{label}: receipt run_id is not a string")
        self.assertTrue(run_id, f"{label}: receipt run_id is empty")
        failures = receipt.get("failures")
        self.assertIsInstance(failures, list, f"{label}: receipt failures is not a list")
        self.assertTrue(failures, f"{label}: receipt has no terminal failure")
        ids = [row.get("id") for row in failures]
        for value in ids:
            self.assertIsInstance(value, str, f"{label}: failure id is not a string")
            self.assertTrue(value, f"{label}: failure id is empty")
        self.assertEqual(len(ids), len(set(ids)), f"{label}: failure ids are not unique")
        self.assertEqual(failures[-1].get("kind"), envelope["error"].get("kind"),
                         f"{label}: receipt terminal kind != envelope kind")
        for row in failures:
            for cause in row.get("caused_by") or ():
                self.assertIn(cause, ids,
                              f"{label}: caused_by {exact(cause)} names no failure in this receipt")
        for path, value in _reference_sites(receipt):
            found = REFS.check_cli_only(label, path, value)
            if found:
                self.fail(f"{label}: receipt reference at {path} violates the shape rule: {exact(value)}")
        REFS.rows.append({"label": label, "path": "$.diagnostic", "receipt_run_id": run_id,
                          "failure_ids": ids, "conforms": True})

    # -- artifacts ---------------------------------------------------------
    def tree_snapshot(self, path: Path) -> dict:
        if not (path.exists() or path.is_symlink()):
            return {"kind": "absent"}
        if path.is_symlink():
            return {"kind": "symlink", "target": os.readlink(path)}
        if path.is_file():
            return {"kind": "file", "bytes": path.stat().st_size, "sha256": sha256_file(path)}
        members = {}
        for directory, subdirs, files in os.walk(path):
            subdirs.sort()
            for name in sorted(files):
                member = Path(directory) / name
                members[fs_text(member.relative_to(path))] = {
                    "bytes": member.stat().st_size, "sha256": sha256_file(member),
                    "mode": member.stat().st_mode & 0o777}
        return {"kind": "dir", "members": members}

    def assert_same_artifact(self, label: str, cli_snap: dict, py_snap: dict) -> None:
        if cli_snap == py_snap:
            return
        detail = {"differences": _snapshot_diff(cli_snap, py_snap)}
        if cli_snap.get("kind") == "file" and py_snap.get("kind") == "file":
            detail["zip"] = _zip_diagnosis(self, label, cli_snap, py_snap)
        STORE.write(f"artifacts/{len(CTX.failures):05d}-{safe_name(label)}.json", detail)
        rendered = json.dumps(detail)
        CTX.record_failure(label, rendered)
        self.fail(f"{label}: artifacts differ: {rendered}")

    def write_parity(self, label: str, destination: Path, py_fn, cli_argv: tuple,
                     *, expect_exit: int | None = None) -> tuple[dict, dict]:
        """Same logical destination, used sequentially: CLI then binding."""
        self.assertFalse(destination.exists(), f"{label}: destination must not pre-exist")
        cli = self.ctx.cli(*cli_argv)
        cli_json = parse_json_stdout(cli)
        cli_snapshot = self.tree_snapshot(destination)
        self.ctx.remove_owned(destination)
        self.assertFalse(destination.exists(), f"{label}: harness failed to clear its destination")
        call = self.ctx.py(label, py_fn)
        py_snapshot = self.tree_snapshot(destination)
        if call.ok:
            self.assert_same(label, call.report, cli_json)
            self.assert_report_contract(label, call)
            envelope_ok = hard_error_envelope(cli.stderr)
            if envelope_ok is not None:
                self.assert_admitted_refusal(label, call.report, envelope_ok['error'].get('kind'), cli.exit, cli_argv[0])
        else:
            if call.gap:
                CTX.gaps.append(f"{label}: {call.msg}")
            envelope = hard_error_envelope(cli.stderr)
            if cli.exit == 0 and envelope is None:
                self.fail(f"{label}: Python raised {call.exception!r} but the CLI succeeded")
            if envelope is not None:
                self.assertEqual(call.kind, envelope["error"].get("kind"),
                                 f"{label}: kind differs from CLI")
            if cli_json is not MISSING and call.report is not None:
                self.assert_same(label + " [error report]", call.report, cli_json)
        if expect_exit is not None:
            self.assertEqual(cli.exit, expect_exit, f"{label}: CLI exit")
        self.assert_same_artifact(label, cli_snapshot, py_snapshot)
        self.ctx.remove_owned(destination)
        return cli_snapshot, py_snapshot

    def policy_noop(self, label: str, base_argv: tuple, variant_argv: tuple) -> None:
        """Exact flag parity, accounting only validated per-invocation receipt IDs."""
        base = self.ctx.cli(*base_argv)
        variant = self.ctx.cli(*variant_argv)
        self.assertEqual(variant.exit, base.exit, f"{label}: exit changed")
        self.assertEqual(variant.stdout, base.stdout, f"{label}: stdout changed")
        if variant.stderr == base.stderr:
            return
        base_envelope = hard_error_envelope(base.stderr)
        variant_envelope = hard_error_envelope(variant.stderr)
        self.assertIsNotNone(base_envelope, f"{label}: changed stderr without base envelope")
        self.assertIsNotNone(variant_envelope, f"{label}: changed stderr without variant envelope")
        for result, envelope in ((base, base_envelope), (variant, variant_envelope)):
            self.assert_cli_receipt(label, result, envelope)
            self.assertEqual(envelope["diagnostic"]["producer"]["binary_sha256"],
                             self.ctx.oracle.sha256, f"{label}: receipt producer is not the actual CLI")
        base_id, variant_id = policy_stderr_identity_pair(base.stderr, variant.stderr)
        REFS.rows.append({"label": label, "path": "$.diagnostic.run_id",
                          "base_run_id": base_id, "variant_run_id": variant_id,
                          "canonical_identical_identity_accounted": True})


def _snapshot_diff(a: dict, b: dict) -> list:
    diffs = []
    if a.get("kind") != b.get("kind"):
        return [{"path": "$.kind", "cli": a.get("kind"), "python": b.get("kind")}]
    if a.get("kind") == "file":
        if a.get("sha256") != b.get("sha256"):
            diffs.append({"path": "$.sha256", "cli": a.get("sha256"), "python": b.get("sha256"),
                          "cli_bytes": a.get("bytes"), "python_bytes": b.get("bytes")})
        return diffs
    left = a.get("members", {})
    right = b.get("members", {})
    for name in sorted(set(left) - set(right)):
        diffs.append({"path": name, "cli": left[name], "python": MISSING, "note": "missing from python"})
    for name in sorted(set(right) - set(left)):
        diffs.append({"path": name, "cli": MISSING, "python": right[name], "note": "extra in python"})
    for name in sorted(set(left) & set(right)):
        if left[name] != right[name]:
            diffs.append({"path": name, "cli": left[name], "python": right[name]})
    return diffs


def _zip_diagnosis(case: ParityCase, label: str, cli_snap: dict, py_snap: dict) -> dict:
    """Member-level comparison when two zip destinations differ byte-wise."""
    cli_path, py_path = case._zip_paths.get(label, (None, None))
    if cli_path is None:
        return {"status": "no retained copies"}
    import zipfile

    def members(path: Path) -> dict:
        out = {}
        with zipfile.ZipFile(path) as archive:
            for info in archive.infolist():
                out[info.filename] = {"crc": info.CRC, "bytes": info.file_size,
                                      "sha256": sha256_bytes(archive.read(info.filename))}
        return out

    left, right = members(cli_path), members(py_path)
    differing = sorted(n for n in set(left) | set(right) if left.get(n) != right.get(n))
    return {"status": "compared", "differing_members": differing,
            "members": {"cli": left, "python": right}}


# --------------------------------------------------------------------------- #
# Synthetic fixtures (authored here; no third-party reader or asset is used)
# --------------------------------------------------------------------------- #

def _dna(names: list[bytes], types: list[bytes], lengths: list[int],
         structs: list[tuple[int, list[tuple[int, int]]]]) -> bytes:
    """One DNA1 payload: NAMES, TYPE, TLEN and STRC in the saved order."""
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


def build_blend(blocks: list[tuple[bytes, int, int, bytes]], dna: bytes, *, sdna: int = 0) -> bytes:
    out = bytearray(b"BLENDER-v405")
    for code, address, count, payload in blocks:
        out.extend(struct.pack("<4sIQII", code, len(payload), address, sdna, count))
        out.extend(payload)
    for code, payload in ((b"DNA1", dna), (b"ENDB", b"")):
        out.extend(struct.pack("<4sIQII", code, len(payload), 0, 0, 0))
        out.extend(payload)
    return bytes(out)


TEST_NAMES = [b"number", b"*next", b"name[8]", b"scalar", b"array[2]", b"f32", b"*chain"]
TEST_TYPES = [b"char", b"uint64_t", b"float", b"int", b"Test"]
TEST_FIELDS = [(1, 0), (1, 1), (0, 2), (3, 3), (3, 4), (2, 5), (4, 6)]
TEST_DNA = _dna(TEST_NAMES, TEST_TYPES, [1, 8, 4, 4, 48], [(4, TEST_FIELDS)])


def test_payload(*, number: int, next_address: int, name: bytes, scalar: int,
                 array: tuple[int, int], f32_bits: int, chain_address: int) -> bytes:
    payload = struct.pack("<QQ8siiiIQ", number, next_address, name.ljust(8, b"\0")[:8],
                          scalar, array[0], array[1], f32_bits, chain_address)
    assert len(payload) == 48, len(payload)
    return payload


def fixture_minimal() -> bytes:
    return build_blend([(b"TEST", 0x1000, 1, test_payload(
        number=2**64 - 1, next_address=0, name=b"N\xff", scalar=-(2**31),
        array=(2**31 - 1, -(2**31)), f32_bits=0x7FC12345, chain_address=0))], TEST_DNA)


# A small but structurally real ID graph: Image starts with an embedded ID and
# the engine only resolves a linked block when the target is ID-headed.
ID_NAMES = [b"id", b"*next", b"*prev", b"*lib", b"name[32]"]
ID_TYPES = [b"char", b"ID", b"Image"]
ID_DNA = _dna(ID_NAMES, ID_TYPES, [1, 24, 56],
              [(1, [(1, 1), (1, 2), (1, 3)]), (2, [(1, 0), (0, 4)])])


def image_payload(*, next_address: int, prev_address: int, library_address: int, name: bytes) -> bytes:
    payload = struct.pack("<QQQ", next_address, prev_address, library_address)
    payload += name.ljust(32, b"\0")[:32]
    assert len(payload) == 56, len(payload)
    return payload


def fixture_pointer_chain() -> bytes:
    return build_blend([(b"IM\0\0", 0x1000, 1, image_payload(
        next_address=0x2000, prev_address=0, library_address=0, name=b"first")),
        (b"IM\0\0", 0x2000, 1, image_payload(
            next_address=0, prev_address=0x1000, library_address=0, name=b"second"))],
        ID_DNA, sdna=1)


def fixture_dangling() -> bytes:
    """A linked identifier whose Library pointer has no matching block."""
    return build_blend([(b"IM\0\0", 0x1000, 1, image_payload(
        next_address=0, prev_address=0, library_address=0xDEADBEEF, name=b"orphan"))],
        ID_DNA, sdna=1)


REWRITE_NAMES = [b"name[32]", b"width"]
REWRITE_TYPES = [b"char", b"int", b"Image"]
REWRITE_DNA = _dna(REWRITE_NAMES, REWRITE_TYPES, [1, 4, 36], [(2, [(0, 0), (1, 1)])])


def fixture_rewrite() -> bytes:
    name = b"//textures/old.png"
    payload = name.ljust(32, b"\0") + struct.pack("<i", 512)
    return build_blend([(b"IM\0\0", 0x3000, 1, payload)], REWRITE_DNA)


def fixture_truncated(source: bytes) -> bytes:
    return source[: 12 + 24 + 10]


def fixture_garbage() -> bytes:
    return b"not a blend file at all\n"


def build_exr() -> tuple[bytes, list[float], list[float]]:
    """A minimal single-part, uncompressed, float32 scanline EXR (2x2)."""
    red = [1.0, 2.0, 3.0, 4.0]
    green = [0.5, 1.5, 2.5, 3.5]
    channels = [("G", green), ("R", red)]  # chlist is sorted by name
    header = bytearray(struct.pack("<II", 0x01312F76, 2))  # magic, version 2, no flags

    def attribute(name: bytes, kind: bytes, value: bytes) -> None:
        header.extend(name + b"\0" + kind + b"\0" + struct.pack("<I", len(value)) + value)

    chlist = bytearray()
    for channel, _ in channels:
        chlist.extend(channel.encode() + b"\0" + struct.pack("<i", 2) + b"\0\0\0\0" + struct.pack("<ii", 1, 1))
    chlist.extend(b"\0")
    attribute(b"channels", b"chlist", bytes(chlist))
    attribute(b"compression", b"compression", b"\0")
    box = struct.pack("<iiii", 0, 0, 1, 1)
    attribute(b"dataWindow", b"box2i", box)
    attribute(b"displayWindow", b"box2i", box)
    attribute(b"lineOrder", b"lineOrder", b"\0")
    attribute(b"pixelAspectRatio", b"float", struct.pack("<f", 1.0))
    attribute(b"screenWindowCenter", b"v2f", struct.pack("<ff", 0.0, 0.0))
    attribute(b"screenWindowWidth", b"float", struct.pack("<f", 1.0))
    header.extend(b"\0")

    rows = []
    for y in range(2):
        samples = bytearray()
        for x in range(2):
            for _, values in channels:
                samples.extend(struct.pack("<f", values[y * 2 + x]))
        rows.append(struct.pack("<ii", y, len(samples)) + bytes(samples))
    table_at = len(header)
    offset = table_at + 8 * 2
    offsets = bytearray()
    for row in rows:
        offsets.extend(struct.pack("<Q", offset))
        offset += len(row)
    body = bytearray()
    for row in rows:
        body.extend(row)
    return bytes(header + offsets + body), red, green


def exr_pixel_bytes(red: list[float], green: list[float], layout: str = "documented") -> bytes:
    """The authored 2x2 sample stream, in one of the two defined orders.

    ``documented`` is what docs/USAGE.md promises and what ``exr_pixels.rs``
    describes: pixel-interleaved in the report's ``channels`` order (G,R), x
    increasing then y. ``on-disk`` is EXR's own scanline layout, which is what
    the current binary actually emits: row-major, and channel-planar within each
    row. The harness accepts either exact stream (so the authored values are
    still proven byte for byte) and records which one was produced.
    """
    if layout == "documented":
        return b"".join(struct.pack("<ff", green[index], red[index]) for index in range(4))
    out = bytearray()
    for y in range(2):
        for values in (green, red):  # header channel order
            for x in range(2):
                out.extend(struct.pack("<f", values[y * 2 + x]))
    return bytes(out)


def setup_library_null_project(work: Path) -> Path:
    """One owner for the authored warning/asset graph, shared by flag and report proofs."""
    if work.exists():
        shutil.rmtree(work)
    (work / "project").mkdir(parents=True)
    (work / "assets").mkdir(parents=True)
    fixture_root = REPO / "tests" / "fixtures" / "library-null"
    for entry in fixture_root.iterdir():
        if entry.suffix != ".blend":
            continue
        dest = work / ("assets" if entry.name == "image.blend" else "project") / entry.name
        shutil.copyfile(entry, dest)
    (work / "assets" / "pixel.png").write_bytes(bytes.fromhex(
        "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c489"
        "0000000d49444154789c63f8cfc0f01f00050001ff89993d1d0000000049454e44ae426082"))
    return work / "project" / "normal-root.blend"


class Fixtures:
    def __init__(self, root: Path) -> None:
        self.root = root
        root.mkdir(parents=True, exist_ok=True)
        self.minimal = root / "minimal.blend"
        self.chain = root / "pointer-chain.blend"
        self.dangling = root / "dangling.blend"
        self.rewrite = root / "rewrite.blend"
        self.truncated = root / "truncated.blend"
        self.garbage = root / "garbage.blend"
        self.image = root / "image.exr"
        self.not_exr = root / "not-an-exr.bin"
        self.red: list[float] = []
        self.green: list[float] = []
        self.bytes: dict[str, bytes] = {}

    def build(self) -> None:
        self.bytes = {
            "minimal": fixture_minimal(),
            "chain": fixture_pointer_chain(),
            "dangling": fixture_dangling(),
            "rewrite": fixture_rewrite(),
            "truncated": fixture_truncated(fixture_minimal()),
            "garbage": fixture_garbage(),
        }
        for name, path in (("minimal", self.minimal), ("chain", self.chain), ("dangling", self.dangling),
                           ("rewrite", self.rewrite), ("truncated", self.truncated), ("garbage", self.garbage)):
            path.write_bytes(self.bytes[name])
        exr, self.red, self.green = build_exr()
        self.bytes["image"] = exr
        self.image.write_bytes(exr)
        self.not_exr.write_bytes(b"not an exr\n")

    def manifest(self) -> dict:
        return {name: {"bytes": len(data), "sha256": sha256_bytes(data)} for name, data in sorted(self.bytes.items())}


FIXTURES: Fixtures


# --------------------------------------------------------------------------- #
# Preflight
# --------------------------------------------------------------------------- #

PUBLIC_REQUIRED = (
    "open", "open_many", "File", "Record", "FieldQuery", "FieldValue", "BlockInfo",
    "Summary", "QueryReport", "TraceReport", "PackReport", "TextReport", "ScanReport",
    "InspectReport", "ValueReport", "PointerReport", "FactsReport", "CensusReport",
    "ListReport", "OverrideReport", "DiffReport", "EditReport", "ExtractReport",
    "NodeTreeReport", "ExrHeaderReport", "PixelsReport", "ReportPreview", "ReportCreated",
    "ReportDiscarded", "OxblendError", "SourceChangedError", "ClosedFileError", "ERROR_TYPES",
    "summary", "query", "catalog", "explain", "inspect", "get", "pointer", "payload",
    "source_facts", "library_facts", "trace", "list_dependencies", "block_report",
    "overrides", "diff", "edit", "node_tree", "extract", "rewrite", "pack", "pack_sequence",
    "exr_header", "exr_scan", "exr_pixels", "pointer_census", "report_preview",
    "report_create", "report_discard", "read_path_list",
)

CLI_REQUIRED = (
    "summary", "inspect", "get", "pointer", "payload", "read-batch", "source-facts", "library-facts",
    "trace", "list", "blocks", "overrides", "diff", "edit", "node-tree", "extract",
    "rewrite", "pack", "pack-sequence", "exr-header", "exr-scan", "exr-pixels",
    "pointer-census", "query", "catalog", "explain", "report",
)

FILE_REQUIRED = (
    "summary", "query", "explain", "inspect", "get", "pointer", "blocks", "payload",
    "source_facts", "library_facts", "pointer_census", "trace", "list_dependencies",
    "pack", "pack_sequence", "header", "closed", "close", "memory", "open_diagnostics",
    "byte_array", "pointer_table",
)


def expected_class_name(kind: str) -> str:
    """The binding's stable class for a kind (mirrors scripts/generate-errors.py).

    Two kinds are not generated: the engine's ``input_changed`` is the native
    ``SourceChangedError`` and the binding's own ``closed_file`` is the native
    ``ClosedFileError``, so neither takes the catalog's collision prefix.
    """
    if kind == "input_changed":
        return "SourceChangedError"
    if kind == "closed_file":
        return "ClosedFileError"
    name = "".join(part.title() for part in kind.split("_")) + "Error"
    if name in ("OxblendError", "ClosedFileError", "SourceChangedError") or hasattr(builtins, name):
        name = "Oxblend" + name
    return name


DIRECT_REPORT_NEEDED = (
    "oxblend.report(error_or_report, output, ...) or report_preview(output, report=error_or_report) "
    "must accept a raised OxblendError or a warning-carrying report and publish a report "
    "without a CLI ledger, temporary envelope or ledger identifier "
    "(L20/DESIGN-DIAGNOSTICS-AMENDMENT.md)"
)


def direct_report_api(module) -> str | None:
    """The direct-entry report signature this facade exposes, if any."""
    if hasattr(module, "report"):
        return "oxblend.report(error_or_report, output, ...)"
    preview = getattr(module, "report_preview", None)
    if preview is not None:
        try:
            parameters = inspect_module.signature(preview).parameters
        except (TypeError, ValueError):
            parameters = {}
        if "report" in parameters:
            return "report_preview(output, report=error_or_report)"
    return None


def oracle_gaps() -> list[str]:
    """The oracle must publish every required primitive/composed CLI surface."""
    gaps = []
    missing = [name for name in CLI_REQUIRED if not CTX.oracle.supports(name)]
    if missing:
        gaps.append("CLI oracle lacks commands the binding exposes: " + ", ".join(missing))
    return gaps


def preflight(module) -> list[str]:
    gaps: list[str] = []
    if direct_report_api(module) is None:
        gaps.append("missing direct bug-report API: " + DIRECT_REPORT_NEEDED)
    for name in PUBLIC_REQUIRED:
        if not hasattr(module, name):
            gaps.append(f"missing public name: oxblend.{name}")
    for name in FILE_REQUIRED:
        if not hasattr(module.File, name):
            gaps.append(f"missing File method: oxblend.File.{name}")
    for name in ("open_many",):
        if hasattr(module, name):
            try:
                signature = inspect_module.signature(module.open_many)
            except (TypeError, ValueError):
                continue
            if "memory" not in signature.parameters:
                gaps.append("open_many has no memory= parameter (amendment B)")
    if hasattr(module, "ERROR_TYPES"):
        types = module.ERROR_TYPES
        catalog = module._native.error_catalog()
        kinds = {row["kind"] for row in catalog["entries"]}
        if set(types) != kinds | {"closed_file"}:
            gaps.append("ERROR_TYPES keys are not the catalog kinds plus closed_file: "
                        f"missing={sorted((kinds | {'closed_file'}) - set(types))} "
                        f"extra={sorted(set(types) - kinds - {'closed_file'})}")
        if len(set(types.values())) != len(types):
            duplicates = sorted({cls.__name__ for cls in types.values()
                                 if list(types.values()).count(cls) > 1})
            gaps.append(f"ERROR_TYPES maps distinct kinds to one class: {duplicates}")
        for kind, cls in types.items():
            expected = expected_class_name(kind)
            if cls.__name__ != expected:
                gaps.append(f"ERROR_TYPES[{kind!r}] -> {cls.__name__}, expected {expected}")
            if not issubclass(cls, module.OxblendError):
                gaps.append(f"{cls.__name__} is not an OxblendError")
            if getattr(builtins, cls.__name__, None) is not None and cls.__name__ != "SourceChangedError":
                gaps.append(f"{cls.__name__} shadows a builtin exception")
    return gaps


def installed_wheel_problem(module) -> str | None:
    try:
        distribution = importlib.metadata.distribution("oxblend")
    except importlib.metadata.PackageNotFoundError:
        return "oxblend distribution metadata is not installed"
    direct = distribution.read_text("direct_url.json")
    if direct:
        try:
            url = json.loads(direct)
        except json.JSONDecodeError:
            return "direct_url.json is malformed"
        if url.get("dir_info", {}).get("editable"):
            return "installed distribution is editable (direct_url.json dir_info.editable)"
    module_path = Path(module.__file__).resolve()
    if "site-packages" not in module_path.parts and "dist-packages" not in module_path.parts:
        return f"module is not loaded from site-packages: {module_path}"
    return None


# --------------------------------------------------------------------------- #
# Corpus command parity
# --------------------------------------------------------------------------- #

def scene_fields(inspect_json: dict) -> list[str]:
    """Every field name declared by this file's own ``Scene`` structure."""
    sdna = inspect_json["sdna"]
    types = [bytes.fromhex(t) for t in sdna["types_hex"]]
    for struct in sdna["structs"]:
        if types[struct["type_index"]] != b"Scene":
            continue
        return [bytes.fromhex(field["name_hex"]).decode("latin-1") for field in struct["fields"]]
    return []


class CatalogParity(ParityCase):
    def assert_catalog_docs(self, kind, schema, published):
        for field in ('explanation', 'likely_causes', 'next_steps', 'known_issue'):
            self.assertEqual(schema.get(field), published.get(field),
                             f'{kind}: installed catalog {field} != CLI help')

    def test_01_catalog_and_explain(self) -> None:
        cli = self.ctx.cli("catalog")
        call = self.ctx.py("catalog", self.ox.catalog)
        self.assertEqual(cli.exit, 0, cli.stderr)
        cli_json = parse_json_stdout(cli)
        self.assert_same("catalog", call.report, cli_json)
        self.assert_report_contract("catalog", call)
        self.assertTrue(call.ok, call.msg)
        concepts = cli_json["concepts"]
        self.assertTrue(concepts, "catalog must publish at least one concept")

        conformance = []
        for concept in concepts:
            key = concept.get("concept") or concept.get("name") or concept.get("key")
            self.assertIsInstance(key, str, f"concept without a name: {exact(concept)}")
            with self.subTest(concept=key):
                self.row("explain", key, "no-file", lambda: self.parity(
                    f"explain[{key}]", lambda: self.ox.explain(key), ("explain", key)))
                conformance.append({"concept": key, "fields": sorted(concept)})

        sample = self.ctx.small_blend()
        for concept in [concepts[0].get("concept") or concepts[0].get("name")]:
            with self.subTest(concept=concept, file="corpus"):
                self.row("explain", concept, "with-file", lambda: self.parity(
                    f"explain[{concept}]+file", lambda: self.ox.explain(concept, file=sample),
                    ("explain", concept, "--file", sample)))
        with self.subTest(concept="unknown"):
            self.row("explain", "<unknown>", "error", lambda: self.parity(
                "explain[unknown]", lambda: self.ox.explain("definitely-not-a-concept"),
                ("explain", "definitely-not-a-concept")))
        STORE.write("catalog.json", {"concepts": len(concepts), "fields": conformance})

    def test_02_every_catalog_error_kind(self) -> None:
        """Catalog bijection plus the doc fields the CLI publishes per kind."""
        module = self.ox
        # Native owned arrays are tuples before the public JSON projection.
        # Compare JSON shapes, without changing any catalog text or leaf value.
        catalog = _safe(module._native.error_catalog())
        entries = {row["kind"]: row for row in catalog["entries"]}
        self.assertEqual(len(entries), len(catalog["entries"]), "catalog kinds are not unique")
        rows = []
        for kind in sorted(entries):
            with self.subTest(kind=kind):
                cls = module.ERROR_TYPES[kind]
                self.assertEqual(cls.__name__, expected_class_name(kind))
                cli = self.ctx.cli("help", "error", kind, "--json")
                self.assertEqual(cli.exit, 0, f"help error {kind} failed: {cli.stderr!r}")
                published = parse_json_stdout(cli)["error"]
                self.assertEqual(cls.__name__, expected_class_name(kind))
                schema = entries[kind]
                self.assert_catalog_docs(kind, schema, published)
                # The first paragraph of each generated class is the exact
                # catalog explanation. Runtime help adds usage/CLI paragraphs;
                # docstring_gate.py independently requires their coverage.
                # Native exception classes have attached runtime documentation
                # and are checked here by identity/base rather than catalog text.
                if cls.__module__.endswith("_native"):
                    self.assertTrue(issubclass(cls, module.OxblendError), cls.__name__)
                else:
                    documented = (cls.__doc__ or "").strip()
                    self.assertTrue(documented, f"{kind}: {cls.__name__} has no docstring")
                    self.assertEqual(documented.partition("\n\n")[0], schema["explanation"].replace('"', "'"),
                                     f"{kind}: class docstring must begin with the exact catalog explanation")
                rows.append({"kind": kind, "class": cls.__name__,
                             "explanation": schema["explanation"]})
        self.assertEqual(len(module.ERROR_TYPES), len(entries) + 1)
        self.assertIn("closed_file", module.ERROR_TYPES)
        self.assertIs(module.ERROR_TYPES["input_changed"], module.SourceChangedError)
        self.assertIs(module.ERROR_TYPES["closed_file"], module.ClosedFileError)
        STORE.write("error-catalog-rows.json", rows)
        # A changed ZIP-width explanation must trip this SAME comparison.
        published_zip = parse_json_stdout(self.ctx.cli('help', 'error', 'zip_width', '--json'))['error']
        corrupt = dict(entries['zip_width'])
        corrupt['explanation'] += ' deliberately changed negative'
        with self.assertRaisesRegex(AssertionError, 'zip_width.*explanation'):
            self.assert_catalog_docs('zip_width', corrupt, published_zip)
        STORE.write('zip-width-negative.json', {'changed_explanation_rejected': True,
                    'original_catalog': entries['zip_width'], 'cli_help': published_zip})


class ReadParity(ParityCase):
    def test_01_summary_inspect(self) -> None:
        for row in self.inputs():
            path = row["path"]
            with self.subTest(path=row["relative"]):
                self.row("summary", row["relative"], "default", lambda: self.parity(
                    f"summary:{row['relative']}", lambda: self.ox.summary(path), ("summary", path)))
                self.row("inspect", row["relative"], "default", lambda: self.parity(
                    f"inspect:{row['relative']}", lambda: self.ox.inspect(path), ("inspect", path)))

    def test_02_structured_reads(self) -> None:
        for row in self.inputs():
            path = row["path"]
            label = row["relative"]
            for command, fn in (
                ("source-facts", lambda p=path: self.ox.source_facts(p, allow_incomplete=True)),
                ("library-facts", lambda p=path: self.ox.library_facts(p, allow_incomplete=True)),
                ("pointer-census", lambda p=path: self.ox.pointer_census(p)),
                ("pointer-census-full", lambda p=path: self.ox.pointer_census(p, full=True)),
                ("overrides", lambda p=path: self.ox.overrides(p, allow_incomplete=True)),
                ("overrides-root", lambda p=path: self.ox.overrides(p, recursive=False, allow_incomplete=True)),
                ("node-tree", lambda p=path: self.ox.node_tree(p)),
            ):
                argv_command = {"pointer-census-full": "pointer-census",
                                "overrides-root": "overrides"}.get(command, command)
                argv: tuple = (argv_command, path)
                if command == "pointer-census-full":
                    argv = ("pointer-census", path, "--full")
                elif command == "overrides-root":
                    argv = ("overrides", path, "--json", "--root-only")
                elif command == "overrides":
                    argv = ("overrides", path, "--json")
                with self.subTest(path=label, command=command):
                    self.row(command, label, "default", lambda a=argv, f=fn, c=command: self.parity(
                        f"{c}:{label}", f, a))

    def test_03_query(self) -> None:
        for row in self.inputs():
            inspect_cli = parse_json_stdout(self.ctx.cli("inspect", row["path"]))
            fields = [f for f in scene_fields(inspect_cli) if "," not in f]
            for fieldset in (["render"], ["scene"], ["render", "view_layers"], fields):
                if not fieldset:
                    continue
                argv = ("query", row["path"], "--type", "Scene", "--fields", ",".join(fieldset))
                with self.subTest(path=row["relative"], fields=fieldset):
                    self.row("query", row["relative"], ",".join(fieldset), lambda a=argv, f=tuple(fieldset): self.parity(
                        f"query:{row['relative']}:{','.join(f)}",
                        lambda: self.ox.query(row["path"], type="Scene", fields=f, allow_incomplete=True), a))
        # A type this file does not declare: engine refusal parity on the real input.
        path = self.ctx.small_blend()
        self.row("query", "unsupported_type", "error", lambda: self.parity(
            "query[unsupported_type]",
            lambda: self.ox.query(path, type="NoSuchSavedStruct", fields=["anything"]),
            ("query", path, "--type", "NoSuchSavedStruct", "--fields", "anything")))


class ValueParity(ParityCase):
    """Batch value parity: ONE retained File + ONE read-batch CLI call per input.

    Actual engine contract (read-batch worker; coordinated, no aliases):
    * CLI spelling: ``read-batch FILE --requests SPEC_FILE``.
    * SPEC_FILE is UTF-8 JSON ``{"schema":1,"requests":[...]}`` in plan order.
      Ops: ``inspect`` / ``payload`` / ``pointer`` / ``get``. ``get`` carries
      ``block``/``element``/``path`` (field strings plus nonnegative flattened
      indices) with exclusive ``raw``/``byte_array``/``pointer_table``;
      ``element`` defaults to 0. Unknown top-level/spec fields refuse the whole
      batch; per-item structural problems are independent item errors.
    * stdout is JSONL: one item line per request in input order, then a final
      terminal summary line ``{"schema":1,"record_type":"summary",
      "complete":bool,"counts":{"total","observed","succeeded","failed",
      "unobserved"},"input","input_recheck","input_changed"}``.
      Success items carry the exact single-report JSON; error items carry
      strictly ``{index,op,ok:false,error:<exact unchanged single error>,
      diagnostic_ref:<verbatim null or bound pair>}`` with the canonical
      public error object (kind, message, next_command, catalog prose, NO
      inside diagnostic_ref) plus the top-level sibling diagnostic_ref
      (R17 strict, no inside-fallback). The private
      ``source_snapshot`` is never emitted and never stored here.
    * Terminal truth: ``succeeded``+``failed``==``observed``,
      ``observed``+``unobserved``==``total``==len(requests),
      ``complete``==(``failed``==0 and not ``input_changed``).
      Independent item errors complete with CLI exit 2 and terminal stderr
      kind ``incomplete_read_batch``; a source-guard failure stops the batch,
      keeps its ORIGINAL kind, leaves trailing requests unobserved and fails
      the whole input (``input`` vs ``input_recheck`` end check is mandatory).
    Anything outside this contract fails loudly with evidence; no fake-CLI
    acceptance exists anywhere in this class.
    """

    BATCH_COMMAND = "read-batch"
    BATCH_SPEC_SCHEMA = 1
    # Cumulative value counts across inputs in this process (per-input rows go
    # to value-batch-ledger.json; the running total is also progress-printed).
    batch_totals = {"inputs": 0, "planned": 0, "observed": 0, "succeeded": 0,
                    "failed": 0, "compared": 0}

    def sites(self, path: Path, inspect_json: dict):
        """Deterministic, SDNA-driven site selection; never a numeric cap."""
        blocks = inspect_json["blocks"]
        structures = inspect_json["sdna"]["structs"]
        types = [bytes.fromhex(t) for t in inspect_json["sdna"]["types_hex"]]
        seen = set()
        for index, block in enumerate(blocks):
            if not block["count"] or block["sdna_index"] >= len(structures):
                continue
            struct = structures[block["sdna_index"]]
            if ARGS.value_coverage == "layout" and block["sdna_index"] in seen:
                continue
            seen.add(block["sdna_index"])
            elements = range(block["count"]) if ARGS.value_coverage == "field" else (0,)
            for element in elements:
                for field in struct["fields"]:
                    yield index, element, types[struct["type_index"]], field

    def field_variants(self, field: dict):
        name = bytes.fromhex(field["name_hex"]).decode("latin-1")
        pointer = bool(field["pointer"])
        array_len = field["array_len"]
        variants = [{"steps": [name], "kwargs": {}}]
        if array_len > 1 and not pointer:
            variants.append({"steps": [name, 0], "kwargs": {}})
            variants.append({"steps": [name, array_len - 1], "kwargs": {}})
        variants.append({"steps": [name], "kwargs": {"raw": True}})
        if pointer:
            variants.append({"steps": [name], "kwargs": {"byte_array": 1}})
            variants.append({"steps": [name], "kwargs": {"pointer_table": 1}})
        return variants

    def require_batch_oracle(self) -> None:
        if not self.ctx.oracle.supports(self.BATCH_COMMAND):
            self.fail(
                f"{self.BATCH_COMMAND} unavailable in the CLI oracle "
                f"({self.ctx.oracle.version} {self.ctx.oracle.sha256}): "
                "value parity needs the real engine-worker binary; "
                "no fake-CLI acceptance (engine-worker terminal fields "
                "pending root coordination)")

    def plan_requests(self, path: Path, inspect_json: dict) -> list[dict]:
        """Ordered batch plan from the retained File's own inspect metadata.

        Coverage is the UNCHANGED sites()/field_variants() matrix (layout by
        default; block/field select more elements, never a numeric cap). The
        inspect op stays first so the batch itself proves planning metadata.
        """
        planned = [{"op": "inspect", "request": {"op": "inspect"}, "py": ("inspect", {})}]
        blocks = inspect_json["blocks"]
        # Payload: one read per distinct SDNA layout (the widest block wins).
        widest: dict[int, int] = {}
        for index, block in enumerate(blocks):
            if not block["count"]:
                continue
            if (block["sdna_index"] not in widest
                    or block["size"] > blocks[widest[block["sdna_index"]]]["size"]):
                widest[block["sdna_index"]] = index
        for index in sorted(widest.values()):
            planned.append({"op": "payload",
                            "request": {"op": "payload", "block": index},
                            "py": ("payload", {"block": index})})
        # Get: every declared field at every selected element, every variant.
        for index, element, _parent_type, field in self.sites(path, inspect_json):
            for variant in self.field_variants(field):
                steps, kwargs = variant["steps"], variant["kwargs"]
                # SDNA field names are ASCII C identifiers, so the CLI JSON
                # string and these UTF-8 bytes name the same field; any
                # non-ASCII name surfaces as an exact error comparison, never
                # a silent pass.
                py_steps = tuple(step if isinstance(step, int) else step.encode("utf-8")
                                 for step in steps)
                request: dict = {"op": "get", "block": index, "element": element,
                                 "path": list(steps)}
                if kwargs.get("raw"):
                    request["raw"] = True
                if "byte_array" in kwargs:
                    request["byte_array"] = kwargs["byte_array"]
                if "pointer_table" in kwargs:
                    request["pointer_table"] = kwargs["pointer_table"]
                if "byte_array" in kwargs:
                    py_kind = "byte_array"
                elif "pointer_table" in kwargs:
                    py_kind = "pointer_table"
                else:
                    py_kind = "get"
                planned.append({"op": "get", "request": request,
                                "py": (py_kind, {"block": index, "element": element,
                                                 "steps": py_steps,
                                                 "null_terminated": not kwargs.get("raw", False),
                                                 "count": kwargs.get("byte_array")
                                                 or kwargs.get("pointer_table")})})
        # Pointer: the checked block resolution, not the raw address.
        for index, element, _parent_type, field in self.sites(path, inspect_json):
            if not field["pointer"]:
                continue
            name = bytes.fromhex(field["name_hex"]).decode("latin-1")
            planned.append({"op": "pointer",
                            "request": {"op": "pointer", "block": index,
                                        "element": element, "path": [name]},
                            "py": ("pointer", {"block": index, "element": element,
                                               "steps": (name.encode("utf-8"),),
                                               "null_terminated": True})})
        return planned

    def run_read_batch(self, label: str, path: Path, planned: list[dict]) -> tuple[Result, Path]:
        """ONE CLI invocation per input; the raw JSONL is stored to FILE."""
        spec = {"schema": self.BATCH_SPEC_SCHEMA,
                "requests": [entry["request"] for entry in planned]}
        spec_name = f"read-batch/{safe_name(label)}-requests.json"
        spec_path = self.ctx.work / spec_name
        spec_path.parent.mkdir(parents=True, exist_ok=True)
        spec_path.write_bytes(json.dumps(spec, sort_keys=True, ensure_ascii=True,
                                         separators=(",", ":")).encode("utf-8"))
        STORE.write(spec_name, spec)
        result = self.ctx.cli(self.BATCH_COMMAND, path, "--requests", spec_path)
        raw_path = self.ctx.work / f"read-batch/{safe_name(label)}.jsonl"
        raw_path.write_bytes(result.stdout)
        STORE.write(f"read-batch/{safe_name(label)}.jsonl", result.stdout)
        return result, raw_path

    @staticmethod
    def _key_sites(node: object, key: str, path: str = "$"):
        if isinstance(node, dict):
            for name, value in node.items():
                child = f"{path}.{name}"
                if name == key:
                    yield child
                yield from ValueParity._key_sites(value, key, child)
        elif isinstance(node, list):
            for index, value in enumerate(node):
                yield from ValueParity._key_sites(value, key, f"{path}[{index}]")

    def assert_no_source_snapshot(self, label: str, position: int, item: dict) -> None:
        for site in self._key_sites(item, "source_snapshot"):
            self.fail(f"{label}: batch item {position} leaks private source_snapshot "
                      f"at {site} (value withheld from evidence)")

    def parse_batch_stream(self, label: str, planned: list[dict],
                          result: Result, raw_path: Path) -> tuple[list, dict]:
        """Stream item lines from the stored JSONL; validate framing and order.

        Only one line is parsed at a time: payload-hex lines can be huge and
        must never sit beside a whole parsed capture in RAM. The terminal
        summary has the actual engine shape (``record_type``/``summary`` with
        total/observed/succeeded/failed/unobserved); no alias spellings.
        """
        items: list = []
        summary: dict | None = None
        with raw_path.open("rb") as handle:
            for lineno, line in enumerate(handle):
                stripped = line.strip()
                if not stripped:
                    continue
                try:
                    document = json.loads(stripped)
                except json.JSONDecodeError as exc:
                    self.fail(f"{label}: batch JSONL line {lineno} is not JSON "
                              f"(truncated capture fails the whole input): {exc}")
                if isinstance(document, dict) and document.get("record_type") == "summary":
                    if summary is not None:
                        self.fail(f"{label}: batch has two terminal summaries")
                    summary = document
                    continue
                if summary is not None:
                    self.fail(f"{label}: batch item after the terminal summary (line {lineno})")
                items.append(document)
        if summary is None:
            envelope = hard_error_envelope(result.stderr)
            kind = None if envelope is None else envelope["error"].get("kind")
            self.fail(f"{label}: batch has no terminal summary "
                      f"(truncated capture fails the whole input): exit={result.exit} kind={kind}")
        if summary.get("schema") != 1 or summary.get("record_type") != "summary":
            self.fail(f"{label}: terminal line is not the schema-1 summary record")
        counts = summary.get("counts")
        if (not isinstance(counts, dict)
                or any(type(counts.get(key)) is not int
                       for key in ("total", "observed", "succeeded", "failed", "unobserved"))):
            self.fail(f"{label}: terminal summary lacks the actual integer counts "
                      f"total/observed/succeeded/failed/unobserved: "
                      f"{counts!r} (no aliases accepted)")
        if counts["total"] != len(planned):
            self.fail(f"{label}: terminal total {counts['total']} != {len(planned)} planned requests")
        if len(items) != counts["observed"]:
            self.fail(f"{label}: {len(items)} streamed item lines != "
                      f"observed {counts['observed']} (truncated capture fails the whole input)")
        for position, (entry, item) in enumerate(zip(planned, items)):
            if not isinstance(item, dict):
                self.fail(f"{label}: batch item {position} is not an object")
            if type(item.get("index")) is not int or item["index"] != position:
                self.fail(f"{label}: batch item {position} has index {item.get('index')!r} "
                          f"(order check)")
            if item.get("op") != entry["op"]:
                self.fail(f"{label}: batch item {position} has op {item.get('op')!r}, "
                          f"planned {entry['op']!r} (order check)")
            ok = item.get("ok")
            if ok is True:
                if "error" in item or "diagnostic_ref" in item:
                    self.fail(f"{label}: batch item {position} ok:true must not carry "
                              f"error/diagnostic_ref (R17 strict)")
                report = item.get("report")
                if not isinstance(report, dict) or report.get("schema") != 1:
                    self.fail(f"{label}: batch item {position} ok:true has no schema-1 report")
            elif ok is False:
                error = item.get("error")
                if (not isinstance(error, dict) or not isinstance(error.get("kind"), str)
                        or not error["kind"]):
                    self.fail(f"{label}: batch item {position} ok:false has no typed error")
                if "diagnostic_ref" not in item:
                    self.fail(f"{label}: batch item {position} ok:false lacks top-level "
                              f"diagnostic_ref sibling (R17 strict, no inside fallback)")
                if "diagnostic_ref" in error:
                    self.fail(f"{label}: batch item {position} error carries inside "
                              f"diagnostic_ref (R17 forbids, sibling-only)")
                if "report" in item:
                    self.fail(f"{label}: batch item {position} ok:false must not carry report")
                self.assert_no_source_snapshot(label, position, item)
            else:
                self.fail(f"{label}: batch item {position} has ok={ok!r} (must be bool)")
        return items, summary

    def validate_terminal(self, label: str, planned: list[dict], items: list,
                          summary: dict, result: Result):
        """Validate the terminal truth, exit rule and stderr envelope/receipt.

        Counts truth: succeeded+failed==observed, observed+unobserved==total,
        and both tallies must equal the streamed items. complete==(failed==0
        and not input_changed). Clean batches exit 0 with no error envelope;
        item-error batches exit 2 with terminal kind ``incomplete_read_batch``;
        guard batches exit 2 with the ORIGINAL guard kind (trailing requests
        stay unobserved) and the mandatory end input check.
        """
        counts = summary["counts"]
        succeeded = sum(1 for item in items if item.get("ok") is True)
        failed = sum(1 for item in items if item.get("ok") is False)
        self.assertEqual(counts["succeeded"], succeeded,
                         f"{label}: succeeded {counts['succeeded']} != {succeeded} true items")
        self.assertEqual(counts["failed"], failed,
                         f"{label}: failed {counts['failed']} != {failed} false items")
        if counts["succeeded"] + counts["failed"] != counts["observed"]:
            self.fail(f"{label}: succeeded+failed != observed in {counts}")
        if counts["observed"] + counts["unobserved"] != counts["total"]:
            self.fail(f"{label}: observed+unobserved != total in {counts}")
        for key in ("input", "input_recheck"):
            if key not in summary:
                self.fail(f"{label}: terminal summary drops {key} (no field ignored)")
        changed = summary.get("input_changed")
        done = summary.get("complete")
        if type(changed) is not bool or type(done) is not bool:
            self.fail(f"{label}: terminal summary lacks boolean complete/input_changed")
        if not changed and summary["input_recheck"] != summary["input"]:
            self.fail(f"{label}: clean batch input_recheck differs from input "
                      f"with input_changed=false")
        if done != (failed == 0 and not changed):
            self.fail(f"{label}: terminal complete={done} disagrees with "
                      f"failed={failed} input_changed={changed}")
        envelope = hard_error_envelope(result.stderr)
        guard_items = [item for item in items
                       if item.get("ok") is False
                       and item["error"].get("kind") in ("input_changed", "source_changed")]
        if len(guard_items) > 1:
            self.fail(f"{label}: more than one guard item; the guard stops the batch once")
        if failed == 0 and not changed:
            if result.exit != 0:
                self.fail(f"{label}: CLI exit {result.exit} != 0 for a clean batch")
            if envelope is not None:
                self.fail(f"{label}: clean batch carries an error envelope "
                          f"({envelope['error'].get('kind')})")
            return None
        if result.exit != 2:
            self.fail(f"{label}: CLI exit {result.exit} != 2 for failed={failed} "
                      f"input_changed={changed}")
        if envelope is None:
            self.fail(f"{label}: non-clean batch has no terminal error envelope")
        if changed:
            if guard_items:
                expected = guard_items[0]["error"]["kind"]
            elif summary["input_recheck"] is None:
                # End signature recheck itself failed (no mid-batch guard row
                # exists); the kind is whatever the recheck error was. The
                # receipt is still validated; the kind is recorded, not aliased.
                expected = envelope["error"].get("kind")
            else:
                expected = "input_changed"
            self.assertEqual(envelope["error"].get("kind"), expected,
                             f"{label}: guard terminal kind must keep the original kind")
        else:
            self.assertEqual(envelope["error"].get("kind"), "incomplete_read_batch",
                             f"{label}: item-error terminal kind must be incomplete_read_batch")
        self.assert_cli_receipt(label, result, envelope)
        return envelope

    def _variant_name(self, position: int, entry: dict, params: dict) -> str:
        op = entry["op"]
        if op == "inspect":
            return "inspect"
        if op == "payload":
            return f"block={params['block']}"
        steps = params.get("steps", ())
        rendered = [step if isinstance(step, int) else bytes(step).decode("utf-8", "replace")
                    for step in steps]
        return (f"block={params.get('block')} element={params.get('element')} "
                f"steps={rendered} mode={entry['py'][0]} index={position}")

    def assert_owned_projection(self, tag: str, fetch) -> None:
        """The retained-File projection is stable and owned (no aliasing)."""
        first = fetch()
        second = fetch()
        self.assertEqual(first, second, f"{tag}: File projection is not stable")
        if isinstance(first, dict):
            first["__harness_probe__"] = 1
            third = fetch()
            self.assertNotIn("__harness_probe__", third,
                             f"{tag}: projection aliases engine state")
            self.assertEqual(third, second,
                             f"{tag}: projection changed after caller mutation")

    def compare_witness_inspect(self, tag: str, cli_report: dict, witness: dict) -> None:
        """The CLI inspect item must byte-match the canonical witness fingerprint.

        The fingerprint is computed over canonical bytes (sorted keys, compact
        separators), so a match proves whole-object equality with no field
        masking and no witness reload. A mismatch reloads the file witness
        exactly once for one exact whole-object diff (which also settles
        float-repr differences, since equality is then decided on values).
        """
        if sha256_bytes(json_bytes(cli_report)) == witness["sha256"]:
            return
        expected = json.loads(Path(witness["work_path"]).read_bytes())
        self.assert_same(tag, expected, cli_report)

    def compare_native_error(self, tag: str, exc, item: dict, position: int) -> None:
        """A batch error item must equal the typed native exception exactly.

        R17 strict: the failed row is ``{index,op,ok:false,
        error:<exact unchanged single error>,
        diagnostic_ref:<verbatim null or bound pair>}``. The shared exact
        helper covers kind/message/class/exit/prose with no key mask and
        forbids any inside diagnostic_ref; the top-level sibling
        diagnostic_ref pair is validated by the shared References ledger
        contract (CLI null or {run_id,failure_id}; native null; no invented
        refs, no aliases, no inside fallback).
        """
        self.assertIsInstance(item, dict, f"{tag}: batch failed row is not an object")
        self.assertIn("diagnostic_ref", item,
                      f"{tag}: batch failed row lacks top-level diagnostic_ref sibling "
                      f"(R17 strict, no inside fallback)")
        self.assertIn("error", item, f"{tag}: batch failed row lacks error")
        cli_error = item["error"]
        self.assertIsInstance(cli_error, dict, f"{tag}: batch error is not an object")
        self.assertNotIn("diagnostic_ref", cli_error,
                         f"{tag}: batch error carries inside diagnostic_ref "
                         f"(R17 forbids, sibling-only)")
        self.assert_native_matches_error_object(tag, exc, cli_error, expected_exit=2)
        native_ref = getattr(exc, "diagnostic_ref", MISSING)
        sibling_ref = item.get("diagnostic_ref")
        found = REFS.check(tag, "$.diagnostic_ref",
                           sibling_ref, native_ref)
        if found:
            shown = "; ".join(f"{p}: cli={exact(a)} python={exact(b)}" for p, a, b in found)
            self.fail(f"{tag}: batch diagnostic_ref violates the shared ledger contract: {shown}")

    def compare_batch_item(self, label: str, handle, kept: dict, position: int,
                           entry: dict, item: dict, witness: dict | None) -> None:
        op = entry["op"]
        py_kind, params = entry["py"]
        tag = f"{op}:{label}:{position}"
        variant = self._variant_name(position, entry, params)
        if item.get("ok") is True and op == "inspect":
            # Planning-metadata proof: the CLI inspect item must match the
            # canonical witness taken from this same File before the capture.
            self.row(op, label, variant,
                     lambda t=tag, c=item["report"], w=witness:
                     self.compare_witness_inspect(t, c, w))
            return
        if item.get("ok") is True:
            cli_report = item["report"]
            if py_kind == "payload":
                thunk = lambda: handle.payload(params["block"]).to_json()
            else:
                query = self.ox.FieldQuery(kept[params["block"]], params["element"],
                                           params["steps"], params["null_terminated"])
                if py_kind == "get":
                    thunk = lambda: handle.get([query])[0].to_json()
                elif py_kind == "byte_array":
                    def thunk(q=query, h=handle, n=params["count"]):
                        return {"schema": 1, "value": {"kind": "bytes",
                                                     "hex": h.byte_array(q, n).hex()}}
                elif py_kind == "pointer_table":
                    def thunk(q=query, h=handle, n=params["count"]):
                        rows = h.pointer_table(q, n)
                        return {"schema": 1, "value": {"kind": "pointer_table",
                            "block_indices": [None if row is None else row.index for row in rows]}}
                elif py_kind == "pointer":
                    def thunk(q=query, h=handle):
                        resolved = h.pointer(q)
                        return {"schema": 1, "block_index":
                              None if resolved is None else resolved.index}
                else:
                    self.fail(f"{tag}: unknown planned kind {py_kind!r}")
            py_report = thunk()
            self.row(op, label, variant,
                     lambda t=tag, p=py_report, c=cli_report: self.assert_same(t, p, c))
            self.assert_owned_projection(tag, thunk)
        else:
            cli_error = item["error"]
            cli_kind = cli_error["kind"]
            query = None
            if py_kind in ("get", "byte_array", "pointer_table", "pointer"):
                query = self.ox.FieldQuery(kept[params["block"]], params["element"],
                                           params["steps"], params["null_terminated"])
            if py_kind == "payload":
                thunk = lambda: handle.payload(params["block"]).to_json()
            elif py_kind == "get":
                thunk = lambda: handle.get([query])[0].to_json()
            elif py_kind == "byte_array":
                thunk = lambda: handle.byte_array(query, params["count"])
            elif py_kind == "pointer_table":
                thunk = lambda: handle.pointer_table(query, params["count"])
            elif py_kind == "pointer":
                thunk = lambda: handle.pointer(query)
            else:
                self.fail(f"{tag}: unknown planned kind {py_kind!r}")
            try:
                thunk()
            except Exception as exc:
                if not hasattr(exc, "kind"):
                    raise
                self.row(op, label, variant,
                         lambda: self.compare_native_error(tag, exc, item, position))
            else:
                self.fail(f"{tag}: CLI item failed ({cli_kind}) but Python succeeded")

    def compare_planning_refusal(self, label: str, path: Path, exc) -> None:
        """Both sides refuse an unplannable input through the full exact contract.

        Bounded to ONE inspect probe (not a value matrix): planning already
        failed, so there is nothing to plan. The probe batch runs the actual
        parse + terminal validation, then the native exception is compared
        against the probe item error with the shared exact helper. Any other
        outcome fails loudly.
        """
        probe = [{"op": "inspect", "request": {"op": "inspect"}, "py": ("inspect", {})}]
        result, raw_path = self.run_read_batch(label, path, probe)
        items, summary = self.parse_batch_stream(label, probe, result, raw_path)
        self.validate_terminal(label, probe, items, summary, result)
        item = items[0]
        tag = f"inspect:{label}:0"
        self.assertFalse(item.get("ok") is True,
                         f"{tag}: Python planning refused ({getattr(exc, 'kind', '?')}) "
                         f"but the CLI batch succeeded")
        self.compare_native_error(tag, exc, item, 0)
        self.progress(phase="values_planning_refused", input=label,
                      kind=getattr(exc, "kind", None))

    def record_input_ledger(self, label: str, planned: int, observed: int,
                            succeeded: int, failed: int, compared: int,
                            status: str, cli_exit=None, envelope_kind=None,
                            counts=None, complete=None, input_changed=None,
                            witness=None, item_errors=()) -> None:
        """Planned requests vs unique CLI observations vs values actually compared."""
        totals = ValueParity.batch_totals
        totals["inputs"] += 1
        totals["planned"] += planned
        totals["observed"] += observed
        totals["succeeded"] += succeeded
        totals["failed"] += failed
        totals["compared"] += compared
        STORE.write(f"read-batch/{safe_name(label)}-ledger.json", {
            "input": label, "status": status, "planned_requests": planned,
            "cli_invocations": 1 if cli_exit is not None else 0,
            "cli_exit": cli_exit, "terminal_envelope_kind": envelope_kind,
            "cli_item_observations": observed,
            "items_succeeded": succeeded, "items_failed": failed,
            "values_compared": compared, "summary_counts": counts,
            "summary_complete": complete, "summary_input_changed": input_changed,
            "inspect_witness": witness,
            # Picked public fields only: the private source_snapshot is never
            # serialized here (see assert_no_source_snapshot).
            "item_errors": list(item_errors)})
        STORE.write("value-batch-ledger.json", {
            "totals": dict(totals),
            "note": ("planned_requests: unchanged sites()/field_variants() matrix "
                       "(inspect first); cli_item_observations: unique batch item "
                       "lines; values_compared: success/error item comparisons "
                       "executed (inspect success compares the witness)")})
        self.progress(phase="values_compared", input=label, planned=planned,
                      observed=observed, succeeded=succeeded, failed=failed,
                      compared=compared,
                      cumulative_compared=totals["compared"], status=status)

    def compare_input_batch(self, label: str, path: Path) -> None:
        """ONE retained File + ONE batch CLI call; identity/inventory/summary in the same File.

        The planner inspect tree is fingerprinted to an exact canonical-JSON
        file witness and freed before the CLI capture, so the 8GB-scale CLI
        bytes never sit beside the redundant planner tree. Only the BlockInfo
        tokens actually referenced by the plan are retained (no new native
        API: the session-tagged tokens cannot be rebuilt from indices). The
        CLI inspect item is then proven against the witness fingerprint.
        """
        planned: list[dict] = []
        observed = 0
        succeeded = 0
        failed = 0
        compared = 0
        status = "ok"
        cli_exit = None
        envelope_kind = None
        counts = None
        complete = None
        input_changed = None
        witness = None
        item_errors: list[dict] = []
        opened = False
        try:
            with self.ox.open(path) as handle:
                opened = True
                self.row("file-facade", label, "root-identity", lambda: self.assertEqual(
                    tuple(handle.root_paths), (fs_text(path),),
                    f"{label}: captured root identity"))
                try:
                    blocks_tuple = handle.blocks()
                    inspect_json = handle.inspect().to_json()
                except Exception as exc:
                    if hasattr(exc, "kind"):
                        self.compare_planning_refusal(label, path, exc)
                        status = "planning_refused"
                        observed = 1
                        failed = 1
                        compared = 1
                        return
                    raise
                self.row("file-facade", label, "block-inventory", lambda: (
                    self.assertEqual([b.index for b in blocks_tuple],
                                     list(range(len(blocks_tuple))),
                                     f"{label}: block indices"),
                    self.assertEqual([b.old_address for b in blocks_tuple],
                                     [int(b["address_decimal"]) for b in inspect_json["blocks"]],
                                     f"{label}: block addresses")))
                planned = self.plan_requests(path, inspect_json)
                needed = sorted({params["block"]
                                 for _, params in (entry["py"] for entry in planned)
                                 if "block" in params})
                kept = {index: blocks_tuple[index] for index in needed}
                canon = json_bytes(inspect_json)
                witness_path = f"read-batch/{safe_name(label)}-inspect-witness.json"
                STORE.write(witness_path, canon)
                work_witness = self.ctx.work / witness_path
                work_witness.parent.mkdir(parents=True, exist_ok=True)
                work_witness.write_bytes(canon)
                witness = {"sha256": sha256_bytes(canon), "bytes": len(canon),
                           "work_path": fs_text(work_witness)}
                self.progress(phase="values_planned", input=label, planned=len(planned),
                              witness_sha256=witness["sha256"], witness_bytes=witness["bytes"])
                del inspect_json, blocks_tuple
                result, raw_path = self.run_read_batch(label, path, planned)
                cli_exit = result.exit
                items, summary = self.parse_batch_stream(label, planned, result, raw_path)
                envelope = self.validate_terminal(label, planned, items, summary, result)
                envelope_kind = None if envelope is None else envelope["error"].get("kind")
                counts = dict(summary["counts"])
                complete = summary["complete"]
                input_changed = summary["input_changed"]
                observed = len(items)
                for position, (entry, item) in enumerate(zip(planned, items)):
                    if item.get("ok") is False:
                        failed += 1
                        error = item["error"]
                        item_errors.append({
                            "index": position, "op": entry["op"],
                            "kind": error.get("kind"), "message": error.get("message"),
                            "diagnostic_ref": item.get("diagnostic_ref")})
                    else:
                        succeeded += 1
                    self.compare_batch_item(label, handle, kept, position, entry, item,
                                            witness)
                    compared += 1
                self.assertEqual(succeeded, counts["succeeded"])
                self.assertEqual(failed, counts["failed"])
                call = self.ctx.py(f"summary:{label}", handle.summary)
                self.assertTrue(call.ok, f"{label}: File.summary raised {call.msg}")
                self.row("file-facade", label, "summary-contract",
                         lambda c=call, l=label:
                         self.assert_report_contract(f"summary:{l}", c))
        except AssertionError:
            status = "failed"
            raise
        except Exception as exc:
            if not opened and hasattr(exc, "kind"):
                self.compare_planning_refusal(label, path, exc)
                status = "planning_refused"
                observed = 1
                failed = 1
                compared = 1
            else:
                status = "failed"
                raise
        finally:
            self.record_input_ledger(label, planned=len(planned), observed=observed,
                                     succeeded=succeeded, failed=failed,
                                     compared=compared, status=status, cli_exit=cli_exit,
                                     envelope_kind=envelope_kind,
                                     counts=counts, complete=complete,
                                     input_changed=input_changed, witness=witness,
                                     item_errors=item_errors)

    def test_01_get_pointer_payload(self) -> None:
        """ONE retained File + ONE batch CLI call per input; no per-value processes."""
        self.require_batch_oracle()
        for row in self.inputs():
            self.compare_input_batch(row["relative"], row["path"])
        self.progress(phase="values_totals", **ValueParity.batch_totals)

    def compare_path_file_sample(self, sample_label: str, path_fn, file_fn) -> None:
        path_call = self.ctx.py("path:" + sample_label, path_fn)
        file_call = self.ctx.py("file:" + sample_label, file_fn)

        def body() -> None:
            self.assertEqual(file_call.ok, path_call.ok,
                             f"{sample_label}: File ok={file_call.ok} path ok={path_call.ok} "
                             f"({file_call.msg!r} vs {path_call.msg!r})")
            if path_call.ok:
                self.assert_same(sample_label, file_call.report, path_call.report)
            else:
                self.assertEqual(file_call.kind, path_call.kind,
                                 f"{sample_label}: kind differs")
                self.assertEqual(file_call.msg, path_call.msg,
                                 f"{sample_label}: message differs")

        self.row("path-file-sample", sample_label, "equivalence", body)

    def test_02_path_file_sample(self) -> None:
        """SMALL sampled path-facade vs File-facade equivalence (no corpus loops, no CLI).

        One authored read per API variant (default/raw/index get, byte_array,
        pointer_table, pointer, payload, inspect). This replaces millions of
        path reparses; value coverage itself lives in test_01's batch. A
        sampled refusal (same typed kind/message on both facades) counts as
        agreement, never as coverage of that value.
        """
        minimal = FIXTURES.minimal
        chain = FIXTURES.chain
        with self.ox.open(minimal) as mfile, self.ox.open(chain) as cfile:
            mb = mfile.blocks()[0]
            cb = cfile.blocks()[0]
            lib_query = self.ox.FieldQuery(cb, 0, (b"id", b"lib"))

            def chain_block_index() -> object:
                resolved = cfile.pointer(self.ox.FieldQuery(cb, 0, (b"id", b"next")))
                return self.ox.Record({"schema": 1, "block_index":
                                        None if resolved is None else resolved.index})
            samples = [
                ("get:number",
                 lambda: self.ox.get(minimal, 0, ["number"]),
                 lambda: mfile.get([self.ox.FieldQuery(mb, 0, (b"number",))])[0]),
                ("get:name:raw",
                 lambda: self.ox.get(minimal, 0, ["name"], raw=True),
                 lambda: mfile.get([self.ox.FieldQuery(mb, 0, (b"name",), False)])[0]),
                ("get:array:index",
                 lambda: self.ox.get(minimal, 0, ["array", 1]),
                 lambda: mfile.get([self.ox.FieldQuery(mb, 0, (b"array", 1))])[0]),
                ("get:next:byte-array",
                 lambda: self.ox.get(minimal, 0, ["next"], byte_array=1),
                 lambda: self.ox.Record({"schema": 1, "value": {
                     "kind": "bytes",
                     "hex": mfile.byte_array(
                         self.ox.FieldQuery(mb, 0, (b"next",)), 1).hex()}})),
                ("get:next:pointer-table",
                 lambda: self.ox.get(minimal, 0, ["next"], pointer_table=1),
                 lambda: self.ox.Record({"schema": 1, "value": {
                     "kind": "pointer_table", "block_indices": [
                         None if row is None else row.index for row in mfile.pointer_table(
                             self.ox.FieldQuery(mb, 0, (b"next",)), 1)]}})),
                ("payload:0",
                 lambda: self.ox.payload(minimal, 0),
                 lambda: mfile.payload(0)),
                ("inspect",
                 lambda: self.ox.inspect(minimal),
                 lambda: mfile.inspect()),
                ("chain:get:name",
                 lambda: self.ox.get(chain, 0, ["name"]),
                 lambda: cfile.get([self.ox.FieldQuery(cb, 0, (b"name",))])[0]),
                ("chain:pointer:id:next",
                 lambda: self.ox.pointer(chain, 0, ["id", "next"]),
                 lambda: chain_block_index()),
                ("chain:get:lib:byte-array",
                 lambda: self.ox.get(chain, 0, ["id", "lib"], byte_array=1),
                 lambda: self.ox.Record({"schema": 1, "value": {
                     "kind": "bytes", "hex": cfile.byte_array(lib_query, 1).hex()}})),
                ("chain:get:lib:pointer-table",
                 lambda: self.ox.get(chain, 0, ["id", "lib"], pointer_table=1),
                 lambda: self.ox.Record({"schema": 1, "value": {
                     "kind": "pointer_table", "block_indices": [
                         None if row is None else row.index
                         for row in cfile.pointer_table(lib_query, 1)]}})),
            ]
            for sample_label, path_fn, file_fn in samples:
                self.compare_path_file_sample(sample_label, path_fn, file_fn)


class TraceListParity(ParityCase):
    def test_01_trace_and_list(self) -> None:
        for row in self.inputs():
            path = row["path"]
            label = row["relative"]
            for name, fn, argv in (
                ("trace", lambda p=path: self.ox.trace(p, allow_incomplete=True), ("trace", path)),
                ("trace-per-file", lambda p=path: self.ox.trace(p, recursive=False, allow_incomplete=True),
                 ("trace", path, "--per-file")),
                ("trace-low-memory", lambda p=path: self.ox.trace(p, low_memory=True, allow_incomplete=True),
                 ("trace", path, "--low-memory")),
                ("list", lambda p=path: self.ox.list_dependencies(p, allow_incomplete=True), ("list", path, "--json")),
                ("list-full", lambda p=path: self.ox.list_dependencies(p, full_info=True, allow_incomplete=True),
                 ("list", path, "--json", "--full-info")),
                ("list-low-memory", lambda p=path: self.ox.list_dependencies(p, low_memory=True, allow_incomplete=True),
                 ("list", path, "--json", "--low-memory")),
            ):
                with self.subTest(path=label, command=name):
                    self.row(name, label, "default", lambda f=fn, a=argv, n=name: self.parity(
                        f"{n}:{label}", f, a))

    def test_02_multi_root(self) -> None:
        roots = self.two_roots()
        if len(roots) < 2:
            self.skipTest("corpus needs at least two roots for multi-root parity")
        for command, fn, argv in (
            ("trace", lambda: self.ox.trace(roots, allow_incomplete=True), ("trace", *roots)),
            ("list", lambda: self.ox.list_dependencies(roots, allow_incomplete=True), ("list", "--json", *roots)),
            ("trace-recursive-off", lambda: self.ox.trace(roots, recursive=False, allow_incomplete=True),
             ("trace", *roots, "--per-file")),
        ):
            with self.subTest(command=command):
                self.row(command, "multi-root", "two-roots", lambda f=fn, a=argv, c=command: self.parity(
                    f"{c}:multi-root", f, a))

    def test_03_file_list_helper(self) -> None:
        """read_path_list must reproduce CLI --files-from expansion."""
        roots = self.two_roots()
        list_file = self.ctx.work / "inputs.lst"
        nul_file = self.ctx.work / "inputs.nul"
        list_file.write_bytes(b"\n".join(fs_text(p).encode() for p in roots) + b"\n")
        nul_file.write_bytes(b"\0".join(fs_text(p).encode() for p in roots) + b"\0")
        for name, path, nul in (("lf", list_file, False), ("nul", nul_file, True)):
            with self.subTest(delimiter=name):
                argv = ("list", "--json", "--files-from", path) + (("-0",) if nul else ())
                (self.parity(f"read_path_list:{name}",
                             lambda p=path, n=nul: self.ox.list_dependencies(
                                 self.ox.read_path_list(p, nul=n), allow_incomplete=True), argv))
                self.assertEqual(list(self.ox.read_path_list(path, nul=nul)), [fs_text(p) for p in roots])


class DiffParity(ParityCase):
    def test_01_diff(self) -> None:
        blends = self.ctx.blends()
        pairs = [(blends[0], blends[0]), (blends[0], blends[1])] if len(blends) > 1 else [(blends[0], blends[0])]
        pairs.append((FIXTURES.minimal, FIXTURES.chain))
        pairs.append((FIXTURES.minimal, FIXTURES.truncated))
        pairs.append((FIXTURES.minimal, self.ctx.work / "does-not-exist.blend"))
        for left, right in pairs:
            with self.subTest(left=fs_text(left), right=fs_text(right)):
                self.row("diff", fs_text(right), "pair", lambda a=left, b=right: self.parity(
                    f"diff:{Path(a).name}:{Path(b).name}",
                    lambda: self.ox.diff(a, b, allow_incomplete=True), ("diff", a, b)))


class FlagSemantics(ParityCase):
    def test_00_policy_receipt_identity_negative(self) -> None:
        """Identity accounting rejects changed semantics, producer, framing and nested IDs."""
        first = {"schema": 1, "error": {"kind": "incomplete_trace", "message": "fixture"},
                 "diagnostic": {"run_id": "1" * 32,
                                "producer": {"binary_sha256": "a" * 64}}}
        second = json.loads(json_bytes(first))
        second["diagnostic"]["run_id"] = "2" * 32
        a, b = json_bytes(first) + b"\n", json_bytes(second) + b"\n"
        self.assertEqual(policy_stderr_identity_pair(a, b), ("1" * 32, "2" * 32))
        for section, key, value in (("error", "message", "changed"),
                                    ("diagnostic", "extra", True)):
            changed = json.loads(b)
            changed[section][key] = value
            with self.assertRaises(AssertionError):
                policy_stderr_identity_pair(a, json_bytes(changed) + b"\n")
        changed = json.loads(b)
        changed["diagnostic"]["producer"]["binary_sha256"] = "b" * 64
        with self.assertRaises(AssertionError):
            policy_stderr_identity_pair(a, json_bytes(changed) + b"\n")
        for changed in (a, b + b"\n", b.replace(b'"2' + b"2" * 31 + b'"', b'"not-an-id"'),
                        b.replace(b'"schema":1', b'"nested":{"run_id":"' + b"2" * 32
                                  + b'"},"schema":1')):
            with self.assertRaises(AssertionError):
                policy_stderr_identity_pair(a, changed)

    def test_01_blocks_text(self) -> None:
        path = FIXTURES.minimal
        inspect_json = parse_json_stdout(self.ctx.cli("inspect", path))
        count = len(inspect_json["blocks"])
        address = inspect_json["blocks"][0]["address_decimal"]
        cases = [
            ("default", {}, ()),
            ("all", {"limit": count}, ("--limit", str(count))),
            ("one", {"limit": 1}, ("--limit", "1")),
            ("negative", {"limit": -1}, ("--limit", "-1")),
            ("negative-all", {"limit": -count}, ("--limit", str(-count))),
            ("address", {"address": int(address)}, ("--address", address)),
            ("dump", {"dump": True, "limit": count}, ("--dump", "--limit", str(count))),
        ]
        for name, kwargs, flags in cases:
            with self.subTest(variant=name):
                cli = self.ctx.cli("blocks", path, *flags)
                call = self.ctx.py(f"block_report:{name}", lambda k=kwargs: self.ox.block_report(path, **k))
                self.assertTrue(call.ok, call.msg)
                self.assertEqual(call.report, cli.stdout.decode("utf-8", "surrogateescape"),
                                 f"blocks {name}: text differs from CLI stdout")
                self.assertEqual(call.object.text, call.report)
                self.assertEqual(str(call.object), call.report)
                self.assertEqual(call.report, call.object.to_json())
                self.assertEqual(call.exit_code, None)
                LEDGER.add("blocks", "fixture", name, "ok", None)
        # Pinned public-corpus regression: sprite_fright utility.blend.
        # Input hash pinned from CLI/manifest facts (sample1 run.json
        # 42c2dfd68910cfb5bfa5ad83029744340ae721df30013dc14ddbe0021a98fec2,
        # 709972B), not invented. When the utility file is selected/available,
        # BOTH CLI stdout and the Python report MUST contain the exact 3
        # referrer lines below; equality alone would mask a both-empty
        # regression. The 3 lines are currently CLI-only (engine
        # bat_cli::references global-println bug, Root fixing); do NOT mask
        # the Python gap.
        utility_relative = "sprite_fright_030_0020_A/030_0020_A/lib/nodes/utility.blend"
        utility = self.ctx.corpus_root / utility_relative
        pinned_lines = [
            "     <BlendFileBlock.bNode (DATA), size=472 at 0x7f5514451a08> (b'inputs', b'first')",
            "     <BlendFileBlock.bNodeSocket (DATA), size=488 at 0x7f5514451e08> b'prev'",
            "     <BlendFileBlock.bNodeLink (DATA), size=56 at 0x7f5514463208> b'tosock'",
        ]
        if utility.is_file():
            pinned_sha = "42c2dfd68910cfb5bfa5ad83029744340ae721df30013dc14ddbe0021a98fec2"
            self.assertEqual(sha256_file(utility), pinned_sha,
                             f"utility input hash pin mismatch for {utility_relative}")
            cli = self.ctx.cli("blocks", utility)
            call = self.ctx.py("block_report:utility", lambda: self.ox.block_report(utility))
            self.assertTrue(call.ok, call.msg)
            cli_text = cli.stdout.decode("utf-8", "surrogateescape")
            for line in pinned_lines:
                self.assertIn(line, cli_text,
                              f"pinned blocks referrer missing from CLI for {utility_relative}: {line!r}")
                self.assertIn(line, call.report,
                              f"pinned blocks referrer missing from Python for {utility_relative}: {line!r}")
            self.assertEqual(call.report, cli_text,
                             f"blocks utility: text differs from CLI stdout for {utility_relative}")
        # General corpus workload: text parity is exact for the real workload.
        # In the utility-only sample this is the same file (no second open);
        # otherwise it covers the smallest input as before.
        corpus = self.ctx.small_blend()
        if not utility.is_file() or fs_text(corpus) != fs_text(utility):
            cli = self.ctx.cli("blocks", corpus)
            call = self.ctx.py("block_report", lambda: self.ox.block_report(corpus))
            self.assertTrue(call.ok, call.msg)
            self.assertEqual(call.report, cli.stdout.decode("utf-8", "surrogateescape"))

    def _asset_bearing_list_path(self) -> Path:
        """CLI-fact asset fixture for list --sha256 (R16).

        Inspects authored FIXTURES first via CLI list --json facts, then the
        known public UDIM asset file, then the filtered corpus inputs. Returns
        the first path whose CLI --json facts contain >=1 EXISTING asset file.
        Fails loudly if none is available (never silently accepts empty).
        """
        # This authored dependency graph owns an existing pixel asset. Do not
        # make native-only qualification depend on an unrelated corpus mount.
        asset_root = setup_library_null_project(self.ctx.work / "list-asset-project")
        candidates: list[Path] = [asset_root, FIXTURES.rewrite, FIXTURES.chain, FIXTURES.minimal]
        udim = self.ctx.corpus_root / "UDIM_monster/UDIM_monster/udim-monster.blend"
        if udim.is_file():
            candidates.append(udim)
        candidates.extend(self.ctx.blends())
        seen: set[str] = set()
        for candidate in candidates:
            key = fs_text(candidate)
            if key in seen:
                continue
            seen.add(key)
            if not candidate.is_file():
                continue
            joined = parse_json_stdout(self.ctx.cli("list", candidate, "--json"))
            if joined is MISSING or not isinstance(joined, dict):
                continue
            listed = [p for group in joined.values()
                      for p in (group if isinstance(group, list) else [group])]
            existing: list[Path] = []
            for entry in listed:
                if isinstance(entry, dict):
                    raw = entry.get("path", entry)
                elif isinstance(entry, str):
                    raw = entry
                else:
                    continue
                if not isinstance(raw, (str, bytes, os.PathLike)):
                    continue
                candidate_path = Path(fs_text(raw))
                if candidate_path.is_file():
                    existing.append(candidate_path)
            if existing:
                return candidate
        self.fail("R16: no CLI list --json fixture with >=1 existing asset available "
                  f"(checked {len(seen)} candidates incl authored fixtures and UDIM)")

    def test_02_list_presentation_flags(self) -> None:
        path = self._asset_bearing_list_path()
        text = self.ctx.cli("list", path)
        hashed = self.ctx.cli("list", path, "--sha256")
        timed = self.ctx.cli("list", path, "--timing")
        self.assertEqual(hashed.exit, text.exit)
        self.assertEqual(timed.exit, text.exit)
        joined = parse_json_stdout(self.ctx.cli("list", path, "--json"))
        sha_json = self.ctx.cli("list", path, "--json", "--sha256")
        timing_json = self.ctx.cli("list", path, "--json", "--timing")
        self.assertEqual(sha_json.stdout, self.ctx.cli("list", path, "--json").stdout,
                         "--sha256 must not change JSON stdout")
        self.assertEqual(timing_json.stdout, self.ctx.cli("list", path, "--json").stdout,
                         "--timing must not change JSON stdout")
        self.assertIn(b"--sha256/--timing cannot be used with --json", sha_json.stderr)
        text_out = text.stdout.decode("utf-8", "surrogateescape")
        hashed_out = hashed.stdout.decode("utf-8", "surrogateescape")
        self.assertTrue(text_out.strip(),
                        f"list text empty for asset fixture {path} (R16 needs >=1 asset)")
        self.assertTrue(hashed_out.strip(),
                        f"list --sha256 text empty for asset fixture {path} (R16 needs >=1 asset)")
        self.assertNotEqual(hashed_out, text_out, "--sha256 text output must add digests")
        self.assertIsNot(joined, MISSING,
                         f"list --json produced no JSON for asset fixture {path}")
        self.assertTrue(isinstance(joined, dict) and joined,
                        f"list --json empty for asset fixture {path} (R16 needs >=1 asset)")
        listed = [p for group in joined.values() for p in (group if isinstance(group, list) else [group])]
        paths = [Path(fs_text(entry.get("path", entry) if isinstance(entry, dict) else entry)) for entry in listed]
        existing = [p for p in paths if p.is_file()]
        self.assertTrue(existing,
                        f"CLI list --json for {path} produced no EXISTING asset (R16)")
        hashes = {sha256_file(p) for p in existing}
        self.assertTrue(hashes, "list --json reported no member files to hash")
        for digest in hashes:
            self.assertIn(digest, hashed_out, "--sha256 text is missing a member digest")
        self.assertRegex(timed.stdout.decode("utf-8", "surrogateescape"), r"(?i)time|ms|s\b")

    def test_03_trace_presentation(self) -> None:
        path = self.ctx.small_blend()
        plain = self.ctx.cli("trace", path)
        summary = self.ctx.cli("trace", path, "--summary")
        self.assertEqual(summary.exit, plain.exit, "--summary is presentation only")
        self.assertIs(parse_json_stdout(summary), MISSING, "--summary prints text, not JSON")
        for flag in ("--strict", "--permissive", "--non-strict"):
            with self.subTest(flag=flag, command="trace"):
                self.policy_noop(f"trace {flag}", ("trace", path), ("trace", path, flag))
            with self.subTest(flag=flag, command="list"):
                self.policy_noop(f"list {flag}", ("list", path, "--json"), ("list", path, "--json", flag))
            with self.subTest(flag=flag, command="overrides"):
                self.policy_noop(f"overrides {flag}", ("overrides", path, "--json"),
                                 ("overrides", path, "--json", flag))
        self.policy_noop("overrides --recursive", ("overrides", path, "--json"),
                        ("overrides", path, "--json", "--recursive"))
        # Scope changes dependency traversal, not just presentation. A readable
        # root can be complete while recursive library traversal is incomplete.
        # Compare each scope against its own Python operation, including exit
        # and retained partial-report semantics; never force equal outcomes.
        for recursive, flag in ((True, "--recursive"), (False, "--root-only")):
            with self.subTest(scope=flag):
                self.parity(f"overrides {flag}",
                            lambda r=recursive: self.ox.overrides(
                                path, recursive=r, allow_incomplete=True),
                            ("overrides", path, "--json", flag))

    def test_04_get_array_modes(self) -> None:
        path = FIXTURES.minimal
        inspect_json = parse_json_stdout(self.ctx.cli("inspect", path))
        block = 0
        name = "name"
        for name, kwargs, flags in (
            ("raw", {"raw": True}, ("--raw",)),
            ("byte-array", {"byte_array": 4}, ("--byte-array", "4")),
            ("pointer-table", {"pointer_table": 1}, ("--pointer-table", "1")),
        ):
            with self.subTest(mode=name):
                self.row("get", "fixture", name, lambda k=kwargs, f=flags: self.parity(
                    f"get:{name}", lambda: self.ox.get(path, block, ["name" if name != "pointer-table" else "chain"], **k),
                    ("get", path, str(block), json.dumps(["name" if name != "pointer-table" else "chain"]), *f)))
        # Mutually exclusive modes are a usage refusal on both sides.
        with self.subTest(mode="exclusive"):
            self.row("get", "fixture", "mode-conflict", lambda: self.parity(
                "get:exclusive", lambda: self.ox.get(path, block, ["name"], raw=True, byte_array=1),
                ("get", path, str(block), json.dumps(["name"]), "--raw", "--byte-array", "1")))


def rewrite_spec_document() -> dict:
    """The authored path rewrite both sides must apply identically."""
    return {"schema": 1, "operation": "rewrite_paths", "edits": [{
        "block_index": 0, "element": 0,
        "path": [{"field_hex": b"name".hex()}],
        "expected_path_hex": b"//textures/old.png".hex(),
        "replacement_path_hex": b"//textures/new.png".hex(),
    }]}


class WriteParity(ParityCase):

    def test_01_rewrite(self) -> None:
        spec = self.ctx.work / "rewrite-spec.json"
        spec.write_text(json.dumps(rewrite_spec_document()))
        for durability in ("none", "end", "each-file"):
            destination = self.ctx.claim(f"rewrite-{durability}/out.blend")
            with self.subTest(durability=durability):
                self.row("rewrite", "fixture", durability, lambda d=destination, u=durability: self.write_parity(
                    f"rewrite:{u}", d,
                    lambda: self.ox.rewrite(FIXTURES.rewrite, d, rewrite_spec_document(), durability=u),
                    ("rewrite", FIXTURES.rewrite, "--output", d, "--edits", spec,
                     *(() if u == "none" else ("--durability", u))), expect_exit=0))
        # Independent proof of the written bytes: the replacement path is present
        # at the authored offset and nothing else moved.
        destination = self.ctx.claim("rewrite-verify/out.blend")
        report = self.ox.rewrite(FIXTURES.rewrite, destination, rewrite_spec_document())
        self.assertTrue(report.output_written)
        self.assertTrue(report.complete)
        data = destination.read_bytes()
        original = FIXTURES.rewrite.read_bytes()
        self.assertEqual(len(data), len(original))
        offset = 12 + 24
        self.assertEqual(data[offset:offset + 32], b"//textures/new.png".ljust(32, b"\0"))
        self.assertNotEqual(data, original)
        self.assertEqual(data[:offset], original[:offset])
        self.assertEqual(data[offset + 32:], original[offset + 32:])
        self.ctx.remove_owned(destination)


    def test_02_edit(self) -> None:
        # Additional clean-room native fixture, not a relabelled corpus input.
        source = REPO / 'bindings/python/tests/fixtures/native521/qualifier.blend'
        self.assertTrue(source.is_file(), 'clean-room native editor fixture missing')
        self.assertIsNotNone(ARGS.blender, '--blender is required for native editor round trips')
        native = Oracle(ARGS.blender)
        verifier = source.with_name('verify.py')

        def scene_block(path):
            view = parse_json_stdout(self.ctx.cli('inspect', path))
            indices = [i for i, block in enumerate(view['blocks'])
                       if block['code_hex'] == b'SC\0\0'.hex()]
            self.assertEqual(len(indices), 1, 'authored fixture must have one Scene')
            return indices[0]

        def received(mode, destination):
            result = native('--background', '--factory-startup', '--disable-autoexec',
                            '--python-exit-code', '1', '--python', verifier,
                            '--', mode, destination)
            self.assertEqual(result.exit, 0, result.stderr.decode('utf-8', 'replace'))
            observations = [json.loads(line[len('L20_NATIVE_RECEIVED '):])
                            for line in result.stdout.decode().splitlines()
                            if line.startswith('L20_NATIVE_RECEIVED ')]
            self.assertEqual(len(observations), 1, 'native receiving proof missing')
            STORE.write('native-edit/' + mode + '.json',
                        {'binary_sha256': native.sha256, 'observation': observations[0]})

        spec = {'schema': 2, 'bits': [], 'writes': [{
            'block_index': scene_block(source), 'element': 0,
            'path': [{'field_hex': b'r'.hex()}, {'field_hex': b'xsch'.hex()}],
            'sdna_type': 'int', 'width': 4,
            'expected': {'kind': 'signed', 'value': 320},
            'replacement': {'kind': 'signed', 'value': 640},
        }]}
        spec_path = self.ctx.work / 'edit-spec.json'
        spec_path.write_text(json.dumps(spec))
        def typed_edit_and_receive(mode, destination):
            report = self.ox.edit(source, destination, spec, mode=mode)
            self.assertTrue(report.output_written and report.complete)
            self.assertTrue(destination.is_file(), 'typed edit did not publish')
            index = scene_block(destination)  # resolve in THIS output, never reuse source indices
            cli = self.ctx.cli('get', destination, str(index), '["r","xsch"]')
            self.assertEqual(cli.exit, 0)
            readback = self.ox.get(destination, index, ['r', 'xsch']).to_json()
            self.assert_same('typed receiving:' + mode, readback, parse_json_stdout(cli))
            self.assertEqual(readback['value'], {'kind': 'signed', 'decimal': '640'})
            received(mode, destination)
            return report

        for mode in ('format-only', 'comp'):
            destination = self.ctx.claim('edit-' + mode + '/out.blend')
            with self.subTest(mode=mode):
                self.row('edit', 'authored-native521', mode, lambda d=destination, m=mode:
                         self.write_parity('edit:' + m, d,
                             lambda: typed_edit_and_receive(m, d),
                             ('edit', source, '--output', d, '--spec', spec_path, '--mode', m),
                             expect_exit=0))

        trees = parse_json_stdout(self.ctx.cli('node-tree', source))['trees']
        tree = next(t for t in trees if t['graph']['name'] == 'NTL20Tree')
        tree_spec = {'schema': 1, 'tree': tree['block_index'], 'expected_graph': tree['graph'],
                     'operations': [{'op': 'remove_node', 'node_type': 'ShaderNodeMath', 'name': 'L20Math'}]}
        tree_path = self.ctx.work / 'node-tree-spec.json'
        tree_path.write_text(json.dumps(tree_spec))
        destination = self.ctx.claim('edit-node-tree/out.blend')
        def structural_edit_and_receive():
            report = self.ox.edit(source, destination, tree_spec, mode='node-tree')
            self.assertTrue(report.output_written and report.complete)
            self.assertTrue(destination.is_file(), 'structural edit did not publish')
            cli = self.ctx.cli('node-tree', destination)
            self.assertEqual(cli.exit, 0)
            readback = self.ox.node_tree(destination).to_json()
            self.assert_same('node-tree receiving', readback, parse_json_stdout(cli))
            after = next(t['graph'] for t in readback['trees'] if t['graph']['name'] == 'NTL20Tree')
            self.assertEqual([n['name'] for n in after['nodes']], ['L20Value'])
            self.assertEqual(after['links'], [])
            received('node-tree', destination)
            return report

        self.row('edit', 'authored-native521', 'node-tree', lambda: self.write_parity(
            'edit:node-tree', destination, structural_edit_and_receive,
            ('edit', source, '--output', destination, '--spec', tree_path, '--mode', 'node-tree'),
            expect_exit=0))

        # Corpus accounting remains separate: every older input must refuse
        # typed editing, with a valid empty spec containing no borrowed index.
        empty = {'schema': 2, 'bits': [], 'writes': []}
        empty_path = self.ctx.work / 'edit-empty-spec.json'
        empty_path.write_text(json.dumps(empty))
        modern = set(self.ctx.by_version(502))
        for path in self.input_paths(self.ctx.blends()):
            if path in modern:
                continue
            destination = self.ctx.claim('edit-version/' + path.name)
            with self.subTest(unsupported=path):
                _, call = self.parity('edit:version:' + str(path),
                    lambda p=path, d=destination: self.ox.edit(p, d, empty, mode='format-only'),
                    ('edit', path, '--output', destination, '--spec', empty_path,
                     '--mode', 'format-only'), expect_exit=2)
                self.assertFalse(call.ok, 'unsupported version unexpectedly published')
                self.assertEqual(call.kind, 'edit_version')
                self.assertFalse(destination.exists())

    def test_08_modern_corpus_edit(self) -> None:
        authored = REPO / 'bindings/python/tests/fixtures/native521/qualifier.blend'
        modern = [path for path in self.ctx.by_version(502) if path.resolve() != authored.resolve()]
        coverage = []
        verifier = REPO / 'bindings/python/tests/fixtures/native521/verify_corpus.py'
        for source in self.input_paths(modern):
            with self.subTest(modern=source):
                self.assertIsNotNone(ARGS.blender, '--blender required for native corpus receiving proof')
                native = Oracle(ARGS.blender)
                view = parse_json_stdout(self.ctx.cli('inspect', source))
                globals_ = [i for i, b in enumerate(view['blocks']) if b['code_hex'] == b'GLOB'.hex()]
                scenes = [i for i, b in enumerate(view['blocks']) if b['code_hex'] == b'SC\0\0'.hex()]
                self.assertEqual(len(globals_), 1)
                self.assertTrue(scenes, 'modern corpus input has no Scene edit target')

                def value(path, index, fields):
                    call = self.ctx.cli('get', path, str(index), json.dumps(fields))
                    self.assertEqual(call.exit, 0, call.stderr)
                    return parse_json_stdout(call)['value']

                subversion = int(value(source, globals_[0], ['subversion'])['decimal'])
                build_hash = bytes.fromhex(value(source, globals_[0], ['build_hash'])['hex']).rstrip(b'\0').decode('ascii')
                row = {'path': fs_text(source), 'header': self.ctx.header_for(source),
                       'subversion': subversion, 'build_hash': build_hash,
                       'pinned_profile': {'version': '5.2.1', 'build_hash': '9e2066aef7ef', 'subversion': 44},
                       'typed': []}
                STORE.write('modern-header/' + source.name + '.json', row)
                scene = scenes[0]
                name = bytes.fromhex(value(source, scene, ['id', 'name'])['hex']).rstrip(b'\0')
                self.assertTrue(name.startswith(b'SC'))
                scene_name = name[2:].decode('utf-8')
                old_x = int(value(source, scene, ['r', 'xsch'])['decimal'])
                old_y = int(value(source, scene, ['r', 'ysch'])['decimal'])
                new_x = 640 if old_x != 640 else 320
                spec = {'schema': 2, 'bits': [], 'writes': [{
                    'block_index': scene, 'element': 0,
                    'path': [{'field_hex': b'r'.hex()}, {'field_hex': b'xsch'.hex()}],
                    'sdna_type': 'int', 'width': 4,
                    'expected': {'kind': 'signed', 'value': old_x},
                    'replacement': {'kind': 'signed', 'value': new_x},
                }]}
                spec_path = self.ctx.claim('modern-spec/' + source.name + '.json')
                spec_path.parent.mkdir(parents=True, exist_ok=True)
                spec_path.write_text(json.dumps(spec))
                expected = {'scene': scene_name, 'resolution_x': new_x, 'resolution_y': old_y}
                expectation = self.ctx.claim('modern-native/' + source.name + '.json')
                expectation.parent.mkdir(parents=True, exist_ok=True)
                expectation.write_text(json.dumps(expected))
                for mode in ('format-only', 'comp'):
                    destination = self.ctx.claim('modern-' + mode + '/' + source.name)

                    def edit_and_receive(m=mode, d=destination):
                        try:
                            report = self.ox.edit(source, d, spec, mode=m)
                        except self.ox.OxblendError as error:
                            row['typed'].append({'mode': m, 'admitted': False, 'kind': error.kind})
                            raise
                        self.assertTrue(report.complete and report.output_written)
                        out_view = parse_json_stdout(self.ctx.cli('inspect', d))
                        out_scenes = [i for i, b in enumerate(out_view['blocks'])
                                      if b['code_hex'] == b'SC\0\0'.hex()]
                        matches = [i for i in out_scenes if bytes.fromhex(
                            value(d, i, ['id', 'name'])['hex']).rstrip(b'\0') == name]
                        self.assertEqual(len(matches), 1)
                        cli_value = value(d, matches[0], ['r', 'xsch'])
                        py_value = self.ox.get(d, matches[0], ['r', 'xsch']).to_json()['value']
                        self.assertEqual(cli_value, py_value)
                        self.assertEqual(cli_value, {'kind': 'signed', 'decimal': str(new_x)})
                        received = native('--background', '--factory-startup', '--disable-autoexec',
                            '--python-exit-code', '1', '--python', verifier, '--', d, expectation)
                        self.assertEqual(received.exit, 0, received.stderr)
                        self.assertIn(b'L20_NATIVE_CORPUS_RECEIVED ', received.stdout)
                        row['typed'].append({'mode': m, 'admitted': True,
                            'native_verified': True, 'binary_sha256': native.sha256})
                        return report

                    self.write_parity('modern:' + source.name + ':' + mode, destination,
                        edit_and_receive, ('edit', source, '--output', destination,
                                         '--spec', spec_path, '--mode', mode))
                _, refusal = self.parity('modern-node-profile:' + source.name,
                    lambda: self.ox.node_tree(source), ('node-tree', source), expect_exit=2)
                self.assertFalse(refusal.ok)
                self.assertEqual(refusal.kind, 'node_version')
                row['structural'] = {'admitted': False, 'kind': refusal.kind,
                                     'message': refusal.msg, 'exit_code': refusal.exit_code}
                coverage.append(row)
        STORE.write('modern-corpus-edit-coverage.json', {'count': len(modern), 'files': coverage})

    def test_03_extract(self) -> None:
        source = self.ctx.small_blend()
        inspect_json = parse_json_stdout(self.ctx.cli("inspect", source))
        chosen = None
        for index, block in enumerate(inspect_json["blocks"]):
            code = bytes.fromhex(block["code_hex"]).rstrip(b"\0")
            if block["count"] and code in (b"OB", b"ME", b"MA", b"SC"):
                chosen = index
                break
        self.assertIsNotNone(chosen, "corpus root has no selectable ID block")
        for durability in ("none", "each-file"):
            destination = self.ctx.claim(f"extract-{durability}/out.blend")
            with self.subTest(durability=durability):
                self.row("extract", "corpus", durability, lambda d=destination, u=durability: self.write_parity(
                    f"extract:{u}", d,
                    lambda: self.ox.extract(source, d, blocks=(chosen,), durability=u, allow_incomplete=True),
                    ("extract", source, "--block", str(chosen), "--output", d, "--json",
                     *(() if u == "none" else ("--durability", u)))))

    def test_04_pack_dry_run_all(self) -> None:
        for row in self.inputs():
            path = row["path"]
            destination = self.ctx.claim_fresh(f"pack-dry/{row['relative'].replace('/', '_')}")
            self.ctx.remove_owned(destination)
            with self.subTest(path=row["relative"]):
                self.row("pack", row["relative"], "dry-run", lambda p=path, d=destination: self.parity(
                    f"pack-dry:{row['relative']}",
                    lambda: self.ox.pack(p, d, dry_run=True, allow_incomplete=True),
                    ("pack", p, d, "--dry-run")))

    def test_05_pack_write(self) -> None:
        """Real packages: synthetic fixtures plus corpus roots whose plan fits free space."""
        import shutil as _shutil
        cases = [("fixture", FIXTURES.minimal, None)]
        corpus = self.ctx.by_version(400)
        for path in corpus:
            plan = parse_json_stdout(self.ctx.cli("pack", path, self.ctx.work / "plan-probe", "--dry-run"))
            needed = 0
            for entry in (plan or {}).get("files", []) if isinstance(plan, dict) else []:
                hexed = entry.get("input_path_hex")
                if hexed:
                    try:
                        needed += Path(bytes.fromhex(hexed).decode("utf-8", "surrogateescape")).stat().st_size
                    except OSError:
                        pass
            free = _shutil.disk_usage(self.ctx.work).free
            if needed and needed > free:
                STORE.write(f"deferred/pack-{path.name}.json",
                            {"path": fs_text(path), "needed_bytes": needed, "free_bytes": free,
                             "reason": "the declared package needs more free space than this filesystem has; "
                                       "recorded here rather than silently skipped"})
                continue
            cases.append(("corpus", path, None))
        for kind, path, _ in cases:
            name = f"pack-{kind}-{path.stem}"
            for flags, kwargs, label in (
                ((), {}, "plain"),
                (("--relative-only",), {"relative_only": True}, "relative-only"),
                (("-n", "-r", "-c"), {"dry_run": True, "relative_only": True, "compress": True}, "combined-nrc"),
            ):
                destination = self.ctx.claim(f"{name}-{label}/pkg")
                with self.subTest(path=fs_text(path), variant=label):
                    self.row("pack", path.name, label, lambda p=path, d=destination, f=flags, k=dict(kwargs): self.write_parity(
                        f"pack:{p.stem}:{label}", d,
                        lambda: self.ox.pack(p, d, allow_incomplete=True, **k),
                        ("pack", p, d, *f, *(() if "--dry-run" not in f else ()))))
            # A ZIP destination exercises the archive writer byte-for-byte.
            destination = self.ctx.claim(f"{name}-zip/pkg.zip")
            with self.subTest(path=fs_text(path), variant="zip"):
                self.row("pack", path.name, "zip", lambda p=path, d=destination: self.write_parity(
                    f"pack-zip:{p.stem}", d,
                    lambda: self.ox.pack(p, d, allow_incomplete=True),
                    ("pack", p, d)))
            # Explicit project root parity.
            destination = self.ctx.claim(f"{name}-project/pkg")
            project = path.parent
            with self.subTest(path=fs_text(path), variant="project"):
                self.row("pack", path.name, "project", lambda p=path, d=destination, r=project: self.write_parity(
                    f"pack-project:{p.stem}", d,
                    lambda: self.ox.pack(p, d, project_root=r, allow_incomplete=True),
                    ("pack", p, d, "--project", r)))

    def test_06_pack_sequence(self) -> None:
        roots = self.two_roots()
        project = Path(os.path.commonpath([fs_text(p) for p in roots]))
        destination = self.ctx.claim("pack-sequence/pkg")
        self.row("pack_sequence", "multi-root", "default", lambda: self.write_parity(
            "pack-sequence", destination,
            lambda: self.ox.pack_sequence(roots, destination, project_root=project, allow_incomplete=True),
            ("pack-sequence", *roots, "-t", destination, "--project", project)))
        # A single distinct root keeps the legacy report shape.
        destination = self.ctx.claim("pack-sequence-single/pkg")
        self.row("pack_sequence", "single-root", "default", lambda: self.write_parity(
            "pack-sequence-single", destination,
            lambda: self.ox.pack_sequence([roots[0]], destination, allow_incomplete=True),
            ("pack-sequence", roots[0], "-t", destination)))

    def _saved_report(self) -> Path:
        """A real, complete schema-1 report the CLI itself accepted as input."""
        saved = self.ctx.work / "saved-report.json"
        result = self.ctx.cli("diff", FIXTURES.minimal, FIXTURES.minimal)
        self.assertNotEqual(parse_json_stdout(result), MISSING, result.stderr)
        self.assertEqual(result.exit, 0, result.stderr)
        saved.write_bytes(result.stdout)
        return saved

    def _preview_stage(self, preview: dict, label: str) -> Path:
        for field in ("receipt_path", "receipt_path_hex", "receipt_sha256", "receipt_bytes", "bundle_id"):
            self.assertIn(field, preview, f"{label}: preview omits {field}")
        receipt = Path(preview["receipt_path"])
        self.assertTrue(receipt.is_file(), f"{label}: receipt_path is not a file: {receipt}")
        data = receipt.read_bytes()
        self.assertEqual(preview["receipt_bytes"], len(data), f"{label}: receipt_bytes")
        self.assertEqual(preview["receipt_sha256"], sha256_bytes(data), f"{label}: receipt_sha256")
        self.assertEqual(bytes.fromhex(preview["receipt_path_hex"]), os.fsencode(receipt),
                         f"{label}: receipt_path_hex")
        self.assertEqual(receipt.name, "preview.json")
        self.assertRegex(os.fsdecode(receipt.parent.name), r"^\.oxblend-report-[0-9a-f]{64}$",
                         f"{label}: stage directory name")
        self.assertEqual(sorted(p.name for p in receipt.parent.iterdir()), ["preview.json"],
                         f"{label}: stage holds members beyond the reviewed receipt")
        return receipt

    def _compare_previews(self, cli_preview: dict, py_preview: dict) -> None:
        # Strict shared-preview comparison for the SAME saved input.
        # Volatile identity is witnessed, never cross-compared equal; every
        # other byte is exact with ONE shared TokenAlias across all members.
        self.assertEqual(sorted(cli_preview), sorted(py_preview), "preview keys differ")
        self.assertEqual(cli_preview["schema"], py_preview["schema"])
        self.assertEqual(cli_preview["schema"], "oxblend.report-preview/v1")
        self.assertEqual(cli_preview["output_basename"], py_preview["output_basename"])
        for label, preview in (("cli", cli_preview), ("python", py_preview)):
            self.assertRegex(preview["bundle_id"], r"^[0-9a-f]{64}$", f"{label} bundle_id shape")
            self.assertRegex(Path(preview["receipt_path"]).parent.name,
                             r"^\.oxblend-report-[0-9a-f]{64}$", f"{label} stage shape")
            self.assertIn(preview["bundle_id"], Path(preview["receipt_path"]).parent.name,
                          f"{label}: stage directory must carry the bundle_id")
        self.assertNotEqual(cli_preview["bundle_id"], py_preview["bundle_id"],
                            "bundle_id must be per-invocation volatile, not reusable bytes")
        self.assertNotEqual(cli_preview["receipt_sha256"], py_preview["receipt_sha256"],
                            "receipt bytes embed the volatile bundle identity")
        # Exporter witnesses: actual CLI/Python ELF hashes, validated build-ID shape.
        cli_build = json.loads(next(m["preview_text"] for m in cli_preview["members"]
                                     if m["path"] == "build.json"))
        py_build = json.loads(next(m["preview_text"] for m in py_preview["members"]
                                    if m["path"] == "build.json"))
        self.assertEqual(cli_build["exporter"]["binary_sha256"], self.ctx.oracle.sha256,
                         "CLI preview exporter must be the oracle binary hash")
        self.assertEqual(py_build["exporter"]["binary_sha256"],
                         sha256_file(Path(sys.executable)),
                         "Python preview exporter must be the running interpreter hash")
        for side, build in (("cli", cli_build), ("python", py_build)):
            self.assertRegex(build["exporter"]["build_revision"], r"^[0-9a-f]{40}$",
                             f"{side} build_revision shape")
            self.assertEqual(build["exporter"]["name"], "oxblend", f"{side} exporter name")
        STORE.write("report-preview-exporters.json",
                    {"cli_exporter": cli_build["exporter"],
                     "python_exporter": py_build["exporter"],
                     "oracle_sha256": self.ctx.oracle.sha256,
                     "python_exe_sha256": sha256_file(Path(sys.executable))})
        # Shared alias across privacy and all members.
        alias = TokenAlias()
        self.assert_aliased_equal(cli_preview["privacy"], py_preview["privacy"], alias,
                                   "preview privacy")
        self.assertEqual(cli_preview["privacy"]["include_names"], False)
        self.assertEqual(py_preview["privacy"]["include_names"], False)
        self.assertEqual(cli_preview["privacy"]["include_file"], False)
        self.assertEqual(py_preview["privacy"]["include_file"], False)
        self.assertEqual(cli_preview["warnings"], py_preview["warnings"],
                         "saved-input preview warnings must be identical (no provenance split here)")
        left, right = cli_preview["members"], py_preview["members"]
        self.assertEqual([m["path"] for m in left], [m["path"] for m in right])
        # Member inventory: same paths, same media/disclosure; bytes lengths
        # equal for same input (tokens fixed-length); digests validated below
        # against each side's own manifest, not cross-compared.
        for a, b in zip(left, right):
            self.assertEqual((a["media_type"], a["whole_content_disclosure"]),
                             (b["media_type"], b["whole_content_disclosure"]), a["path"])
            self.assertEqual(a["bytes"], b["bytes"], f"{a['path']}: member byte length")
            self.assertEqual(a["path"], b["path"])
        # Build member: exact key set and every leaf, except the two exporter
        # identities already witnessed above. Do not silently ignore future keys.
        builds = [json.loads(build["preview_text"]) for build in (next(m for m in left if m["path"] == "build.json"),
                                                                 next(m for m in right if m["path"] == "build.json"))]
        for build in builds:
            for field in ("binary_sha256", "build_revision"):
                build["exporter"].pop(field)
        self.assert_aliased_equal(builds[0], builds[1], alias, "build.json")
        # Manifest member: bundle_id equals own preview, integer timestamp,
        # each own inventory matches own preview/member bytes.
        for preview, tag in ((cli_preview, "cli"), (py_preview, "python")):
            manifest = json.loads(next(m["preview_text"] for m in preview["members"]
                                        if m["path"] == "manifest.json"))
            self.assertEqual(manifest["bundle_id"], preview["bundle_id"],
                             f"{tag} manifest bundle_id must equal own preview bundle_id")
            self.assertTrue(type(manifest["created_at"]) is int and manifest["created_at"] >= 0,
                            f"{tag} manifest created_at must be integer timestamp")
            self.assertEqual(manifest["created_at_unit"], "unix_seconds_utc")
            inv = {row["path"]: row for row in manifest.get("members", [])}
            self.assertEqual(len(inv), len(manifest["members"]), f"{tag} duplicate inventory path")
            self.assertEqual(set(inv), {m["path"] for m in preview["members"]} - {"manifest.json"})
            for member in preview["members"]:
                if member["preview_text"] is not None:
                    body = member["preview_text"].encode("utf-8")
                    self.assertEqual(len(body), member["bytes"], f"{tag} actual preview body size")
                    self.assertEqual(hashlib.sha256(body).hexdigest(), member["sha256"],
                                     f"{tag} actual preview body digest")
                if member["path"] == "manifest.json":
                    continue
                self.assertIn(member["path"], inv, f"{tag} manifest omits {member['path']}")
                self.assertEqual(inv[member["path"]]["sha256"], member["sha256"],
                                 f"{tag} manifest digest for {member['path']} must match own preview")
                self.assertEqual(inv[member["path"]]["bytes"], member["bytes"],
                                 f"{tag} manifest size for {member['path']}")
            STORE.write(f"report-preview-manifest-{tag}.json",
                        {"bundle_id": manifest["bundle_id"], "created_at": manifest["created_at"],
                         "members": manifest.get("members", [])})
        # Cross-side manifest compare excluding witnessed volatile paths.
        cli_manifest = json.loads(next(m["preview_text"] for m in left if m["path"] == "manifest.json"))
        py_manifest = json.loads(next(m["preview_text"] for m in right if m["path"] == "manifest.json"))
        # Exclude only witnessed entropy and inventory digests: each digest was
        # checked against actual own member bytes; member content is compared below.
        def stripped(manifest: dict) -> dict:
            copy = json.loads(json.dumps(manifest))
            copy.pop("bundle_id", None)
            copy.pop("created_at", None)
            for row in copy.get("members", []):
                row.pop("sha256")
            return copy
        self.assert_aliased_equal(stripped(cli_manifest), stripped(py_manifest), alias,
                                   "manifest.json (excluding witnessed bundle/created/inventory-sha)")
        # Remaining members: exact with shared alias (covers error-report,
        # error-source, README, issue.md).
        for a, b in zip(left, right):
            if a["path"] in ("build.json", "manifest.json"):
                continue
            if a["preview_text"] is None:
                self.assertIsNone(b["preview_text"], a["path"])
                continue
            try:
                a_json, b_json = json.loads(a["preview_text"]), json.loads(b["preview_text"])
            except json.JSONDecodeError:
                self.assertEqual(a["preview_text"], b["preview_text"], a["path"])
            else:
                self.assert_aliased_equal(a_json, b_json, alias, a["path"])
        alias.check_bijection()

    def test_07_report(self) -> None:
        saved = self._saved_report()
        bundle = self.ctx.claim("report/bundle.zip")
        bundle.parent.mkdir(parents=True, exist_ok=True)

        cli = self.ctx.cli("report", "--report", saved, "--output", bundle, "--preview")
        cli_preview = parse_json_stdout(cli)
        self.assertIsNot(cli_preview, MISSING, cli.stderr)
        self.assertEqual(cli.exit, 0, cli.stderr)
        cli_receipt = self._preview_stage(cli_preview, "cli preview")
        cli_digest = sha256_file(cli_receipt)

        call = self.ctx.py("report_preview", lambda: self.ox.report_preview(bundle, report=saved))
        self.assertTrue(call.ok, call.msg)
        py_preview = call.report
        py_receipt = self._preview_stage(py_preview, "python preview")
        py_digest = sha256_file(py_receipt)
        self._compare_previews(cli_preview, py_preview)
        self.assertNotEqual(cli_digest, py_digest,
                            "previews must be per-invocation snapshots, not reusable bytes")
        for member in py_preview["members"]:
            self.assertRegex(member["sha256"], r"^[0-9a-f]{64}$", member["path"])
            if member["whole_content_disclosure"] and member["preview_text"] is not None:
                # Only a whole-content disclosure claims that the shown text *is*
                # the member; a truncated preview must disclose a different digest
                # target, so it is validated against the real archive below.
                self.assertEqual(member["sha256"], sha256_bytes(member["preview_text"].encode()),
                                 f"whole-content member digest is not its disclosed text: {member['path']}")

        # CLI create from the CLI stage, then the binding from its own stage.
        created_cli = self.ctx.cli("report", "--create", cli_receipt, "--preview-sha256", cli_digest,
                                   "--output", bundle)
        cli_report = parse_json_stdout(created_cli)
        self.assertEqual(created_cli.exit, 0, created_cli.stderr)
        cli_members = _zip_members(bundle)
        self.assertFalse(cli_receipt.exists(), "CLI create did not consume its preview")
        self.ctx.remove_owned(bundle)

        created = self.ctx.py("report_create", lambda: self.ox.report_create(py_receipt, py_digest, bundle))
        self.assertTrue(created.ok, created.msg)
        py_report = created.report
        py_members = _zip_members(bundle)
        self.assertFalse(py_receipt.exists(), "binding create did not consume its preview")
        self.assertFalse(py_receipt.parent.exists(), "binding create left the stage directory")
        for field in ("schema", "output_written", "snapshot_consumed", "filing"):
            self.assertEqual(py_report[field], cli_report[field], f"created report {field}")
        self.assertEqual([m["path"] for m in py_report["members"]], [m["path"] for m in cli_report["members"]])
        for a, b in zip(cli_report["members"], py_report["members"]):
            self.assertEqual((a["bytes"], a["media_type"]), (b["bytes"], b["media_type"]), a["path"])
        self.assertEqual(py_report["archive_sha256"], sha256_file(bundle),
                         "created report archive_sha256 is not the archive it published")
        self.assertEqual(sorted(py_members), sorted(cli_members), "archive member set differs")
        manifest = json.loads(py_members["manifest.json"]["bytes"])
        inventory = {row["path"]: row for row in manifest.get("members", [])}
        self.assertTrue(inventory, "manifest declares no members")
        for name, member in py_members.items():
            if name in inventory:
                self.assertEqual(inventory[name]["sha256"], member["sha256"], f"manifest digest for {name}")
                self.assertEqual(inventory[name]["bytes"], len(member["bytes"]), f"manifest size for {name}")
        self.ctx.remove_owned(bundle)

        # Discard: each facade gets a fresh stage, because discard consumes it.
        cli = self.ctx.cli("report", "--report", saved, "--output", bundle, "--preview")
        cli_preview2 = parse_json_stdout(cli)
        cli_receipt2 = self._preview_stage(cli_preview2, "cli discard preview")
        discarded_cli = self.ctx.cli("report", "--discard-preview", cli_receipt2,
                                     "--preview-sha256", sha256_file(cli_receipt2))
        self.assertEqual(discarded_cli.exit, 0, discarded_cli.stderr)
        self.assertFalse(cli_receipt2.exists(), "CLI did not consume the preview")
        self.assertFalse(cli_receipt2.parent.exists(), "CLI left the stage directory")

        staged = self.ctx.py("report_preview", lambda: self.ox.report_preview(bundle, report=saved))
        self.assertTrue(staged.ok, staged.msg)
        py_receipt2 = self._preview_stage(staged.report, "python discard preview")
        discarded = self.ctx.py("report_discard",
                                lambda: self.ox.report_discard(py_receipt2, sha256_file(py_receipt2)))
        self.assertTrue(discarded.ok, discarded.msg)
        self.assertFalse(py_receipt2.exists(), "binding did not consume the preview")
        self.assertFalse(py_receipt2.parent.exists(), "binding left the stage directory")
        self.assertEqual(discarded.report, parse_json_stdout(discarded_cli))

        STORE.write("report-volatility.json", {
            "reason": ("src/report.rs reads 32 random bytes per invocation: bundle_id, the stage "
                       "directory name and every HMAC-derived identity differ between invocations by design"),
            "compared": ["schema", "output_basename", "privacy", "warnings", "member path set",
                         "member byte counts, media types and disclosure flags",
                         "member preview_text with 32..64-hex tokens normalised",
                         "receipt_bytes length, receipt_sha256 over the actual receipt bytes",
                         "created-report schema/publication fields and member inventory",
                         "manifest inventory digests against the real archive members"],
            "not_compared": ["bundle_id", "stage path", "HMAC identities", "archive bytes",
                             "manifest created_at (wall clock)"],
        })


def _walk_strings(value: object):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for item in value:
            yield from _walk_strings(item)
    elif isinstance(value, dict):
        for item in value.values():
            yield from _walk_strings(item)


def _zip_members(path: Path) -> dict:
    import zipfile
    out = {}
    with zipfile.ZipFile(path) as archive:
        for info in archive.infolist():
            data = archive.read(info.filename)
            out[info.filename] = {"bytes": data, "sha256": sha256_bytes(data)}
    return out


class DirectReportParity(ParityCase):
    """Strict direct-report parity: library-unbound native vs CLI invocation.

    Intended truthful contract per the diagnostic-identity amendment:

    * ``ox.report(error_or_record, output)`` returns PREVIEW only (no ZIP);
      explicit ``ox.report_create(receipt_path, receipt_sha256, output)``
      publishes the archive and consumes the stage.
    * Strong real source failure is ``ox.pointer(FIXTURES.dangling, 0,
      ['id','lib'])``: ``exception.source_snapshot`` is public immutable
      ``SourceDiagnostic`` with kind ORIGINAL ``unresolved_persistent`` and a
      propagation carrying ``dangling_pointer``; final ``exception.kind`` is
      ``dangling_pointer``. Raw snapshot ``to_json()`` exactly matches CLI
      ``diagnostic.failures[0]`` core fields; CLI adds ``id`` plus catalog docs
      and a terminal ``f1 caused_by f0``. Native ``diagnostic_ref`` stays None;
      CLI refs are null or exact ``{run_id,failure_id}``; no fake begin/ledger.
    * Real warning result is the library-null project (normal-root.blend plus
      normal.blend under temp project, image.blend under sibling assets,
      assets/pixel.png per tests/library_null_pointer.rs): complete trace with
      exactly 3 ``library_null_pointer`` warnings. Raw CLI saved JSON pairs with
      the native trace report; previews/creates compare sanitized contexts,
      kinds and coordinates precisely.
    * Uninstrumented missing-file error is honest unavailable source
      (``source_snapshot`` None, ``report`` None, error-source status
      unavailable). Carried partials are truthful: strict-trace
      ``error.report`` (2 producer errors) survives the source sanitizer with
      status partial, ``input_origin direct_error_report``,
      ``source_pointer /reports/0/errors/{i}`` and
      ``context_origin producer_report``; direct ``allow_incomplete`` partials
      use ``input_origin direct_result_report`` with ``source_pointer
      /errors/{i}`` and the same ``producer_report`` origin.
    * Binding label is exactly ``library call; no CLI invocation binding``.
      Native ``input_origin`` is ``engine_error_snapshot``,
      ``direct_result_report`` or the fixed ``direct_error_report``; CLI is
      ``invocation_receipt`` for a saved error envelope or
      ``legacy_saved_report`` for a raw trace. Every actual difference is
      enumerated with precise paths/values plus provenance/identity/inventory
      witnesses; all remaining structure/data is exact. Raw snapshot vs genuine
      CLI owner record plus sanitized-context comparison is the crux.
    * Only ``kind:<64hex>`` HMAC tokens are bijectively alias-renamed; arbitrary
      hex (file_hex, block_code_hex, addresses, digests) is never erased.
    * Default names/file consent discloses no raw paths or path hex.
    * Negative checks prove a changed source block/pointer coordinate and a
      token-alias topology mismatch are both detected.
    """

    BINDING_LABEL = "library call; no CLI invocation binding"

    # -- shared strict helpers -------------------------------------------

    def _preview_stage(self, preview: dict, label: str) -> Path:
        for field in ("receipt_path", "receipt_path_hex", "receipt_sha256",
                      "receipt_bytes", "bundle_id"):
            self.assertIn(field, preview, f"{label}: preview omits {field}")
        receipt = Path(preview["receipt_path"])
        self.assertTrue(receipt.is_file(), f"{label}: receipt_path missing: {receipt}")
        data = receipt.read_bytes()
        self.assertEqual(preview["receipt_bytes"], len(data), f"{label}: receipt_bytes")
        self.assertEqual(preview["receipt_sha256"], sha256_bytes(data),
                         f"{label}: receipt_sha256")
        self.assertEqual(bytes.fromhex(preview["receipt_path_hex"]), os.fsencode(receipt),
                         f"{label}: receipt_path_hex")
        self.assertEqual(receipt.name, "preview.json")
        self.assertRegex(os.fsdecode(receipt.parent.name),
                         r"^\.oxblend-report-[0-9a-f]{64}$", f"{label}: stage dir")
        self.assertRegex(preview["bundle_id"], r"^[0-9a-f]{64}$", f"{label}: bundle_id shape")
        self.assertIn(preview["bundle_id"], receipt.parent.name,
                      f"{label}: stage must carry bundle_id")
        self.assertEqual(sorted(p.name for p in receipt.parent.iterdir()), ["preview.json"],
                         f"{label}: stage holds beyond preview.json")
        return receipt


    def _validate_exporter_and_bundle(self, cli_preview: dict, py_preview: dict,
                                      label: str) -> None:
        for side, preview in (("cli", cli_preview), ("python", py_preview)):
            self.assertRegex(preview["bundle_id"], r"^[0-9a-f]{64}$",
                             f"{label} {side} bundle_id shape")
            self.assertIn(preview["bundle_id"], Path(preview["receipt_path"]).parent.name,
                          f"{label} {side}: stage must carry bundle_id")
        self.assertNotEqual(cli_preview["bundle_id"], py_preview["bundle_id"],
                            f"{label}: bundle_id must be volatile per invocation")
        self.assertNotEqual(cli_preview["receipt_sha256"], py_preview["receipt_sha256"],
                            f"{label}: receipt binds volatile bundle identity")
        cli_build = json.loads(next(m["preview_text"] for m in cli_preview["members"]
                                     if m["path"] == "build.json"))
        py_build = json.loads(next(m["preview_text"] for m in py_preview["members"]
                                    if m["path"] == "build.json"))
        self.assertEqual(cli_build["exporter"]["binary_sha256"], self.ctx.oracle.sha256,
                         f"{label}: CLI exporter must be oracle hash")
        self.assertEqual(py_build["exporter"]["binary_sha256"],
                         sha256_file(Path(sys.executable)),
                         f"{label}: Python exporter must be interpreter hash")
        self.assertTrue(cli_build["exporter"]["build_revision"], f"{label}: CLI build_revision empty")
        self.assertTrue(py_build["exporter"]["build_revision"], f"{label}: Python build_revision empty")
        STORE.write(f"direct-report/{safe_name(label)}-exporters.json",
                    {"cli_exporter": cli_build["exporter"],
                     "python_exporter": py_build["exporter"],
                     "cli_bundle": cli_preview["bundle_id"],
                     "python_bundle": py_preview["bundle_id"],
                     "oracle_sha256": self.ctx.oracle.sha256,
                     "python_exe_sha256": sha256_file(Path(sys.executable)),
                     "python_exe": sys.executable})

    def _validate_created_archive(self, created: dict, bundle: Path, label: str) -> dict:
        self.assertEqual(created.get("schema"), "oxblend.report-created/v1",
                         f"{label}: created schema")
        self.assertTrue(created.get("output_written"), f"{label}: output_written")
        self.assertTrue(created.get("snapshot_consumed"), f"{label}: snapshot_consumed")
        self.assertTrue(bundle.is_file(), f"{label}: archive missing: {bundle}")
        self.assertEqual(created["archive_sha256"], sha256_file(bundle),
                         f"{label}: archive_sha256 must be the published bytes")
        members = _zip_members(bundle)
        self.assertTrue(members, f"{label}: archive has no members")
        manifest = json.loads(members["manifest.json"]["bytes"])
        inventory = {row["path"]: row for row in manifest.get("members", [])}
        self.assertTrue(inventory, f"{label}: manifest declares no members")
        for name, member in members.items():
            if name in inventory:
                self.assertEqual(inventory[name]["sha256"], member["sha256"],
                                 f"{label}: manifest digest for {name}")
                self.assertEqual(inventory[name]["bytes"], len(member["bytes"]),
                                 f"{label}: manifest size for {name}")
        return members

    def _setup_library_null_project(self) -> Path:
        return setup_library_null_project(self.ctx.work / "direct-library-null")


    def test_01_error_snapshot_is_exact_owner_record(self) -> None:
        api = direct_report_api(self.ox)
        if api is None:
            CTX.gaps.append("direct report API absent: " + DIRECT_REPORT_NEEDED)
            self.skipTest("no direct report API: " + DIRECT_REPORT_NEEDED)
        self.row("direct-report", "dangling", "snapshot", lambda: None)
        # Strong real source failure: no sampling, no invented address.
        failure = self.ctx.py("direct:snapshot-source",
                              lambda: self.ox.pointer(FIXTURES.dangling, 0, ["id", "lib"]))
        self.assertFalse(failure.ok, "ox.pointer(dangling,0,['id','lib']) must refuse")
        self.assertTrue(failure.error, "must be an OxblendError")
        exc = failure.exception
        self.assertIsNotNone(exc)
        self.assertEqual(exc.kind, "dangling_pointer")
        self.assertIsNone(exc.diagnostic_ref,
                          "native diagnostic_ref must remain None (no ledger)")
        REFS.rows.append({"label": "direct:snapshot", "path": "$.exception.diagnostic_ref",
                          "cli": "absent by design (CLI error carries receipt, not reference)",
                          "python": exc.diagnostic_ref, "conforms": exc.diagnostic_ref is None})
        snap = getattr(exc, "source_snapshot", None)
        self.assertIsNotNone(snap, "source_snapshot must be present for instrumented failure")
        # Public immutable SourceDiagnostic: Mapping, no item assignment, owned to_json.
        self.assertEqual(type(snap).__name__, "SourceDiagnostic")
        try:
            snap["kind"] = "mutated"  # type: ignore[index]
            self.fail("SourceDiagnostic must be immutable (item assignment succeeded)")
        except TypeError:
            pass
        first = snap.to_json()
        second = snap.to_json()
        self.assertEqual(first, second, "source_snapshot to_json must be stable/owned")
        first["kind"] = "mutated"
        self.assertEqual(snap.to_json()["kind"], "unresolved_persistent",
                         "to_json must return owned JSON (caller mutation leaked)")
        self.assertEqual(snap["kind"], "unresolved_persistent",
                         "ORIGINAL kind must be unresolved_persistent")
        self.assertEqual(snap.get("kind"), "unresolved_persistent")
        self.assertEqual(snap["context_origin"], "owner_hook")
        props = list(snap["propagations"])
        self.assertEqual(len(props), 1)
        self.assertEqual(props[0]["kind"], "dangling_pointer",
                         "propagation must carry dangling_pointer")
        self.assertEqual(exc.kind, "dangling_pointer")
        # Genuine CLI owner record for the same bytes, same invocation inputs.
        cli = self.ctx.cli("pointer", FIXTURES.dangling, "0", json.dumps(["id", "lib"]), "0")
        envelope = hard_error_envelope(cli.stderr)
        self.assertIsNotNone(envelope, "CLI must emit a schema-1 envelope for dangling pointer")
        self.assertEqual(cli.exit, 2)
        self.assertEqual(envelope["error"]["kind"], "dangling_pointer")
        receipt = envelope.get("diagnostic")
        self.assertIsNotNone(receipt, "CLI envelope must carry its failure receipt")
        self.assertEqual(receipt.get("schema"), "oxblend.failure-receipt/v1")
        self.assertEqual(receipt.get("exit_code"), cli.exit)
        run_id = receipt.get("run_id")
        self.assertIsInstance(run_id, str)
        self.assertTrue(run_id)
        failures = receipt.get("failures")
        self.assertIsInstance(failures, list)
        self.assertEqual(len(failures), 2, "CLI must have ORIGINAL f0 plus terminal f1")
        f0, f1 = failures
        self.assertEqual(f0.get("id"), "f0")
        self.assertEqual(f1.get("id"), "f1")
        self.assertEqual(f1.get("caused_by"), ["f0"])
        self.assertEqual(f0.get("kind"), "unresolved_persistent")
        self.assertEqual(f1.get("kind"), "dangling_pointer")
        # Raw snapshot exactly matches f0 core fields; CLI adds only id+catalog+terminal.
        snap_json = snap.to_json()
        for field in ("caused_by", "check_id", "context", "context_origin", "kind", "message"):
            self.assertEqual(snap_json[field], f0[field],
                             f"raw snapshot core field {field} must equal CLI owner f0")
        # Propagations core must also be exact.
        self.assertEqual(snap_json["propagations"], f0["propagations"],
                         "raw propagations must equal CLI owner f0 propagations")
        # Explicit invocation-only additions on f0: id plus catalog docs.
        self.assertIn("id", f0)
        for field in ("explanation", "likely_causes", "next_steps", "known_issue"):
            self.assertIn(field, f0, f"CLI f0 must carry catalog field {field}")
        # Terminal f1 is invocation-only: no context, terminal origin.
        self.assertIsNone(f1.get("check_id"))
        self.assertIsNone(f1.get("context"))
        self.assertEqual(f1.get("context_origin"), "terminal")
        # diagnostic_ref at every occurrence: CLI null or exact, native None.
        for path, value in _reference_sites(receipt):
            found = REFS.check_cli_only("direct:snapshot-cli-receipt", path, value)
            self.assertFalse(found, f"receipt ref violates shape at {path}: {value!r}")
        self.assertIsNone(exc.diagnostic_ref)
        # No fake begin/ledger/envelope on the native side.
        snap_text = json.dumps(snap_json, sort_keys=True)
        self.assertNotIn("begin", snap_text.lower(), "native snapshot must not fabricate begin")
        self.assertNotIn("ledger", snap_text.lower(), "native snapshot must not fabricate ledger")
        STORE.write("direct-report/snapshot-owner-record.json",
                    {"native_snapshot": snap_json,
                     "cli_f0_core": {k: f0[k] for k in
                                     ("caused_by", "check_id", "context", "context_origin",
                                      "kind", "message", "propagations")},
                     "cli_f0_id": f0.get("id"),
                     "cli_f1": f1,
                     "cli_run_id": run_id,
                     "artefacts": {"cli_stderr_sha256": sha256_bytes(cli.stderr),
                                   "cli_stdout_sha256": sha256_bytes(cli.stdout),
                                   "cli_exit": cli.exit}})
        LEDGER.add("direct-report", "dangling", "snapshot", "ok", None)

    def test_02_direct_error_preview_create_strict(self) -> None:
        api = direct_report_api(self.ox)
        if api is None:
            self.skipTest("no direct report API: " + DIRECT_REPORT_NEEDED)
        self.row("direct-report", "dangling-error", "preview-create", lambda: None)
        failure = self.ctx.py("direct:error-source",
                              lambda: self.ox.pointer(FIXTURES.dangling, 0, ["id", "lib"]))
        self.assertFalse(failure.ok)
        exc = failure.exception
        self.assertEqual(exc.kind, "dangling_pointer")
        # Native preview: PREVIEW only, no ZIP yet.
        native_out = self.ctx.claim("direct-report/error-native/bundle.zip")
        native_out.parent.mkdir(parents=True, exist_ok=True)
        if native_out.exists():
            self.ctx.remove_owned(native_out)
        preview_call = self.ctx.py("direct:native-preview",
                                   lambda: self.ox.report(exc, native_out))
        self.assertTrue(preview_call.ok, preview_call.msg)
        py_preview = preview_call.report
        self.assertEqual(py_preview.get("schema"), "oxblend.report-preview/v1")
        self.assertEqual(py_preview.get("output_basename"), native_out.name)
        self.assertFalse(native_out.exists(),
                         "ox.report must return PREVIEW only; ZIP appears after report_create")
        py_receipt = self._preview_stage(py_preview, "native error preview")
        py_digest = sha256_file(py_receipt)
        self.assertEqual(py_preview["receipt_sha256"], py_digest)
        # Native create: publishes archive, consumes stage.
        create_call = self.ctx.py("direct:native-create",
                                  lambda: self.ox.report_create(py_receipt, py_digest, native_out))
        self.assertTrue(create_call.ok, create_call.msg)
        py_created = create_call.report
        self.assertFalse(py_receipt.exists(), "create must consume its preview")
        self.assertFalse(py_receipt.parent.exists(), "create must remove stage dir")
        py_members = self._validate_created_archive(py_created, native_out, "native error create")
        # Genuine CLI owner envelope for the same bytes.
        cli = self.ctx.cli("pointer", FIXTURES.dangling, "0", json.dumps(["id", "lib"]), "0")
        envelope = hard_error_envelope(cli.stderr)
        self.assertIsNotNone(envelope)
        cli_saved = self.ctx.work / "direct-report-error-cli-envelope.json"
        cli_saved.write_bytes(cli.stderr)
        # CLI preview from that saved envelope (invocation_receipt path).
        cli_out = self.ctx.claim("direct-report/error-cli/bundle.zip")
        cli_out.parent.mkdir(parents=True, exist_ok=True)
        if cli_out.exists():
            self.ctx.remove_owned(cli_out)
        # Use a common basename so output_basename compares exactly.
        # Both destinations already share basename bundle.zip by claim layout.
        self.assertEqual(native_out.name, cli_out.name)
        cli_res = self.ctx.cli("report", "--report", cli_saved, "--output", cli_out, "--preview")
        cli_preview = parse_json_stdout(cli_res)
        self.assertIsNot(cli_preview, MISSING, cli_res.stderr)
        self.assertEqual(cli_res.exit, 0, cli_res.stderr)
        self.assertEqual(cli_preview["output_basename"], cli_out.name)
        cli_receipt = self._preview_stage(cli_preview, "cli error preview")
        cli_digest = sha256_file(cli_receipt)
        self._validate_exporter_and_bundle(cli_preview, py_preview, "direct:error")
        # CLI create from its own stage.
        cli_created_res = self.ctx.cli("report", "--create", cli_receipt,
                                       "--preview-sha256", cli_digest, "--output", cli_out)
        cli_created = parse_json_stdout(cli_created_res)
        self.assertEqual(cli_created_res.exit, 0, cli_created_res.stderr)
        self.assertFalse(cli_receipt.exists(), "CLI create must consume its preview")
        cli_members = _zip_members(cli_out)
        self.assertEqual(sorted(py_members), sorted(cli_members),
                         "native vs CLI archive member sets must match")
        # Sanitized comparison: exact except kind-token aliasing plus enumerated
        # provenance/identity/inventory splits. No substring masking.
        py_err = json.loads(py_members["error-report.json"]["bytes"])
        cli_err = json.loads(cli_members["error-report.json"]["bytes"])
        py_src = json.loads(py_members["error-source.json"]["bytes"])
        cli_src = json.loads(cli_members["error-source.json"]["bytes"])
        # Binding label is exact.
        self.assertEqual(py_err["binding"], self.BINDING_LABEL)
        self.assertEqual(py_src["failures"][0]["input_binding"], self.BINDING_LABEL)
        self.assertEqual(cli_err["binding"], "unverified")
        # Native input_origin engine_error_snapshot; CLI invocation_receipt.
        self.assertEqual(py_src["failures"][0]["input_origin"], "engine_error_snapshot")
        self.assertEqual(cli_src["failures"][0]["input_origin"], "invocation_receipt")
        self.assertEqual(py_src["failures"][0]["context_origin"], "owner_hook")
        self.assertEqual(cli_src["failures"][0]["context_origin"], "owner_hook")
        # Native id null; CLI id is kind token.
        self.assertIsNone(py_src["failures"][0]["id"])
        self.assertTrue(_is_kind_token(cli_src["failures"][0]["id"]),
                         f"CLI f0 id must be kind token: {cli_src['failures'][0]['id']!r}")
        # Native diagnostic_ref None; CLI null or exact.
        self.assertIsNone(exc.diagnostic_ref)
        for path, value in _reference_sites(cli_src):
            found = REFS.check_cli_only("direct:error-cli-source", path, value)
            self.assertFalse(found, f"CLI source ref shape at {path}: {value!r}")
        # Sanitized contexts: exact after kind-token aliasing.
        alias = TokenAlias()
        self.assert_aliased_equal(cli_src["failures"][0]["context"],
                                   py_src["failures"][0]["context"], alias,
                                   "direct:error failures[0].context")
        self.assert_aliased_equal(cli_src["failures"][0]["propagations"],
                                   py_src["failures"][0]["propagations"], alias,
                                   "direct:error failures[0].propagations")
        alias.check_bijection()
        # Kinds/coordinates are exact (never redacted).
        for side, src in (("native", py_src), ("cli", cli_src)):
            ctx = src["failures"][0]["context"]
            self.assertEqual(ctx["block_index"], 0, f"{side} block_index")
            self.assertEqual(ctx["offset_in_block"], 16, f"{side} offset_in_block")
            self.assertEqual(ctx["pointer_decimal"], "3735928559", f"{side} pointer")
            self.assertEqual(ctx["old_address"], "4096", f"{side} old_address")
            self.assertEqual(src["failures"][0]["kind"], "unresolved_persistent")
            self.assertEqual(src["failures"][0]["propagations"][0]["kind"], "dangling_pointer")
        # Terminal CLI failure is invocation-only: enumerate precisely.
        self.assertEqual(len(py_src["failures"]), 1, "native direct error has ORIGINAL only")
        self.assertEqual(len(cli_src["failures"]), 2, "CLI envelope adds terminal f1")
        term = cli_src["failures"][1]
        self.assertEqual(term["kind"], "dangling_pointer")
        self.assertEqual(term["context"], None)
        self.assertEqual(term["context_origin"], "terminal")
        self.assertEqual(term["caused_by"], [cli_src["failures"][0]["id"]])
        self.assertTrue(_is_kind_token(term["id"]))
        # Top-level provenance splits, enumerated exactly.
        expected_top = {
            "$.binding": ("library call; no CLI invocation binding", "unverified",
                          "provenance: library-unbound vs invocation-bound"),
            "$.exit_code": (None, 2, "identity: no process exit for library call"),
            "$.operation_state": (None, {"reports": []}, "inventory: no operation ledger locally"),
            "$.saved_kind": ("direct_error", "error_envelope",
                             "provenance: direct typed error vs saved envelope"),
            "$.source_schema": (None, 1, "inventory: no saved-stream schema locally"),
            "$.stdout_bytes": (None, 0, "inventory: no captured stdout locally"),
            "$.stdout_delivery_complete": (None, True, "inventory: no delivery locally"),
        }
        diffs = json_diff(py_err, cli_err, label="direct:error-top")
        recorded = [{"path": p, "python": exact(a), "cli": exact(b)} for p, a, b in diffs]
        # Filter token-aliased paths (file_token/id) which are validated above.
        non_token = [r for r in recorded if "kind:" not in r["python"] and "kind:" not in r["cli"]]
        STORE.write("direct-report/error-top-diffs.json",
                    {"differences": recorded,
                     "expected_top": {k: {"python": v[0], "cli": v[1], "why": v[2]}
                                      for k, v in expected_top.items()},
                     "witnesses": {"native_binding": py_err["binding"],
                                   "cli_binding": cli_err["binding"],
                                   "native_saved": py_err["saved_kind"],
                                   "cli_saved": cli_err["saved_kind"]}})
        for path, want_py, want_cli, _why in [(k, v[0], v[1], v[2]) for k, v in expected_top.items()]:
            # Resolve path like $.binding
            key = path[2:]
            self.assertIn(key, py_err)
            self.assertIn(key, cli_err)
            self.assertEqual(py_err[key], want_py, f"{path} python")
            self.assertEqual(cli_err[key], want_cli, f"{path} cli")
        # Default consent discloses no raw paths.
        self.assertEqual(py_preview["privacy"]["include_names"], False)
        self.assertEqual(py_preview["privacy"]["include_file"], False)
        self.assertEqual(cli_preview["privacy"]["include_names"], False)
        self.assertEqual(cli_preview["privacy"]["include_file"], False)
        self.ctx.remove_owned(native_out)
        self.ctx.remove_owned(cli_out)
        LEDGER.add("direct-report", "dangling-error", "preview-create", "ok", None)

    def test_03_direct_warning_preview_create_strict(self) -> None:
        api = direct_report_api(self.ox)
        if api is None:
            self.skipTest("no direct report API: " + DIRECT_REPORT_NEEDED)
        self.row("direct-report", "library-null", "preview-create", lambda: None)
        root = self._setup_library_null_project()
        native_call = self.ctx.py("direct:warning-trace",
                                  lambda: self.ox.trace(root, allow_incomplete=True))
        self.assertTrue(native_call.ok, native_call.msg)
        native_report = native_call.report
        self.assertTrue(native_report["complete"], "library-null trace must be complete")
        self.assertEqual(native_report["errors"], [])
        kinds = [w["kind"] for w in native_report["warnings"]]
        self.assertEqual(len(kinds), 3, f"must have 3 library_null_pointer warnings: {kinds}")
        self.assertEqual(kinds, ["library_null_pointer"] * 3)
        for warning in native_report["warnings"]:
            self.assertEqual(warning["pack_action"], "clear_to_null")
            self.assertTrue(warning["offset"] > 0)
            self.assertNotEqual(warning["pointer_address"], "0")
            self.assertTrue(warning["library_chain_hex"])
            self.assertIsNone(warning["failure"]["diagnostic_ref"],
                              "native warning diagnostic_ref must be None")
        REFS.rows.append({"label": "direct:warning-trace", "path": "$.warnings[*].failure.diagnostic_ref",
                          "cli": "checked per-warning below", "python": None, "conforms": True})
        # Raw CLI saved JSON for the same root.
        cli_trace = self.ctx.cli("trace", root)
        self.assertEqual(cli_trace.exit, 0, cli_trace.stderr)
        cli_saved_path = self.ctx.work / "direct-warning-cli-saved.json"
        cli_saved_path.write_bytes(cli_trace.stdout)
        cli_json = parse_json_stdout(cli_trace)
        self.assertIsNot(cli_json, MISSING)
        self.assertTrue(cli_json["complete"])
        self.assertEqual([w["kind"] for w in cli_json["warnings"]],
                         ["library_null_pointer"] * 3)
        # Raw comparison is exact except diagnostic_ref (CLI null or exact).
        self.assertEqual(len(native_report["usages"]), len(cli_json["usages"]))
        self.assertEqual(native_report["usages"], cli_json["usages"],
                         "usages must be exact (no redaction)")
        self.assertEqual(len(native_report["warnings"]), len(cli_json["warnings"]))
        for index, (native_w, cli_w) in enumerate(zip(native_report["warnings"], cli_json["warnings"])):
            base = f"$.warnings[{index}]"
            native_f, cli_f = dict(native_w), dict(cli_w)
            native_ref, cli_ref = native_f["failure"].pop("diagnostic_ref"), cli_f["failure"].pop("diagnostic_ref")
            self.assertIsNone(native_ref, f"{base} native ref")
            if cli_ref is not None:
                self.assertEqual(set(cli_ref), {"run_id", "failure_id"}, f"{base} cli ref keys")
                self.assertTrue(cli_ref["run_id"] and cli_ref["failure_id"], f"{base} cli ref empty")
            REFS.rows.append({"label": "direct:warning-trace", "path": base + ".failure.diagnostic_ref",
                              "cli": cli_ref, "python": native_ref,
                              "conforms": native_ref is None and (
                                  cli_ref is None or (isinstance(cli_ref, dict) and
                                                      bool(cli_ref.get("run_id")) and
                                                      bool(cli_ref.get("failure_id"))))})
            self.assertEqual(native_f, cli_f, f"{base}: warning must be exact except diagnostic_ref")
            # Coordinates are real source facts, never redacted.
            self.assertEqual(native_w["kind"], "library_null_pointer")
            self.assertEqual(native_w["failure"]["kind"], "dangling_pointer")
        # Native preview/create from the trace report (direct_result_report path).
        native_out = self.ctx.claim("direct-report/warning-native/bundle.zip")
        native_out.parent.mkdir(parents=True, exist_ok=True)
        if native_out.exists():
            self.ctx.remove_owned(native_out)
        preview_call = self.ctx.py("direct:warning-native-preview",
                                   lambda: self.ox.report(native_call.object, native_out))
        self.assertTrue(preview_call.ok, preview_call.msg)
        py_preview = preview_call.report
        self.assertFalse(native_out.exists(), "preview must not publish ZIP")
        py_receipt = self._preview_stage(py_preview, "native warning preview")
        py_digest = sha256_file(py_receipt)
        create_call = self.ctx.py("direct:warning-native-create",
                                  lambda: self.ox.report_create(py_receipt, py_digest, native_out))
        self.assertTrue(create_call.ok, create_call.msg)
        py_created = create_call.report
        py_members = self._validate_created_archive(py_created, native_out, "native warning create")
        # CLI preview/create from the raw saved trace (legacy_saved_report path).
        cli_out = self.ctx.claim("direct-report/warning-cli/bundle.zip")
        cli_out.parent.mkdir(parents=True, exist_ok=True)
        if cli_out.exists():
            self.ctx.remove_owned(cli_out)
        self.assertEqual(native_out.name, cli_out.name)
        cli_res = self.ctx.cli("report", "--report", cli_saved_path,
                               "--output", cli_out, "--preview")
        cli_preview = parse_json_stdout(cli_res)
        self.assertIsNot(cli_preview, MISSING, cli_res.stderr)
        self.assertEqual(cli_res.exit, 0, cli_res.stderr)
        cli_receipt = self._preview_stage(cli_preview, "cli warning preview")
        cli_digest = sha256_file(cli_receipt)
        self._validate_exporter_and_bundle(cli_preview, py_preview, "direct:warning")
        cli_created_res = self.ctx.cli("report", "--create", cli_receipt,
                                       "--preview-sha256", cli_digest, "--output", cli_out)
        cli_created = parse_json_stdout(cli_created_res)
        self.assertEqual(cli_created_res.exit, 0, cli_created_res.stderr)
        cli_members = _zip_members(cli_out)
        # Sanitized error-report comparison: both carry complete:true, no failures.
        py_err = json.loads(py_members["error-report.json"]["bytes"])
        cli_err = json.loads(cli_members["error-report.json"]["bytes"])
        self.assertEqual(py_err["binding"], self.BINDING_LABEL)
        self.assertEqual(cli_err["binding"], "unverified")
        self.assertEqual(py_err["saved_kind"], "direct_result")
        self.assertEqual(cli_err["binding"], "unverified")
        # CLI saved_kind for raw trace is legacy (legacy_saved_report provenance).
        self.assertEqual(cli_err["saved_kind"], "legacy")
        for document in (py_err, cli_err):
            self.assertTrue(document["report"]["complete"])
            self.assertEqual(document["report"]["errors"], [])
            self.assertEqual(len(document["report"]["warnings"]), 3)
            self.assertEqual([row["kind"] for row in document["report"]["warnings"]],
                             ["library_null_pointer"] * 3)
        # Warning diagnostics are real retained facts, not accepted-but-dropped
        # inputs. The same closed sanitizer serves CLI and direct native results.
        py_src = json.loads(py_members["error-source.json"]["bytes"])
        cli_src = json.loads(cli_members["error-source.json"]["bytes"])
        self.assertEqual(py_src["failures"], [])
        self.assertEqual(cli_src["failures"], [])
        self.assertEqual(py_src["status"], "partial")
        self.assertEqual(cli_src["status"], "partial")
        self.assertEqual(len(py_src["warnings"]), 3)
        self.assertEqual(len(cli_src["warnings"]), 3)
        self.assertEqual(py_err["warnings"], py_src["warnings"])
        self.assertEqual(cli_err["warnings"], cli_src["warnings"])
        aliases = TokenAlias()
        for index, (native_row, cli_row, original) in enumerate(zip(
                py_src["warnings"], cli_src["warnings"], native_report["warnings"])):
            for row in (native_row, cli_row):
                self.assertEqual(row["kind"], "library_null_pointer")
                self.assertTrue(row["message_redacted"])
                self.assertEqual(row["source_pointer"], f"/warnings/{index}")
                self.assertEqual(row["context"]["decoded_file_offset"], original["offset"])
                self.assertEqual(row["context"]["pointer_decimal"], original["pointer_address"])
                self.assertIsNone(row["context"]["file_path_hex"])
            self.assertEqual(native_row["context_origin"], "producer_report")
            self.assertEqual(native_row["input_origin"], "direct_result_report")
            self.assertEqual(native_row["input_binding"], self.BINDING_LABEL)
            self.assertEqual(cli_row["context_origin"], "legacy_saved_report")
            self.assertEqual(cli_row["input_origin"], "legacy_saved_report")
            self.assertEqual(cli_row["input_binding"], "unverified")
            left, right = dict(cli_row), dict(native_row)
            for field in ("context_origin", "input_origin", "input_binding"):
                left.pop(field); right.pop(field)
            self.assert_aliased_equal(left, right, aliases, f"warnings[{index}]")
        aliases.check_bijection()
        # Consent: no raw paths or path hex in any member.
        for name, member in list(py_members.items()) + list(cli_members.items()):
            if not name.endswith(".json"):
                continue
            text = member["bytes"].decode("utf-8", "replace")
            self.assertNotIn(str(self.ctx.work), text, f"{name} discloses work path")
            self.assertNotIn(os.fsencode(self.ctx.work).hex(), text,
                             f"{name} discloses raw native-byte work path")
        # Work and corpus roots must not appear as raw strings; hex would be
        # the engine's exact field, which is allowed only inside sanitized
        # contexts that we already compared exactly above.
        STORE.write("direct-report/warning-pair.json",
                    {"native_warnings": len(native_report["warnings"]),
                     "cli_warnings": len(cli_json["warnings"]),
                     "native_binding": py_err["binding"],
                     "cli_binding": cli_err["binding"],
                     "native_saved": py_err["saved_kind"],
                     "cli_saved": cli_err["saved_kind"]})
        self.ctx.remove_owned(native_out)
        self.ctx.remove_owned(cli_out)
        LEDGER.add("direct-report", "library-null", "preview-create", "ok", None)

    def test_04_missing_file_honest_and_partial_carried(self) -> None:
        api = direct_report_api(self.ox)
        if api is None:
            self.skipTest("no direct report API: " + DIRECT_REPORT_NEEDED)
        self.row("direct-report", "missing-file", "honest-unavailable", lambda: None)
        missing = self.ctx.work / "direct-report-missing.blend"
        if missing.exists():
            missing.unlink()
        failure = self.ctx.py("direct:missing-source", lambda: self.ox.summary(missing))
        self.assertFalse(failure.ok, "missing file must refuse")
        self.assertTrue(failure.error)
        exc = failure.exception
        self.assertIsNone(getattr(exc, "source_snapshot", None),
                          "uninstrumented missing-file error must have no snapshot (honest)")
        self.assertIsNone(exc.report, "missing-file error carries no partial report")
        self.assertIsNone(exc.diagnostic_ref)
        native_out = self.ctx.claim("direct-report/missing-native/bundle.zip")
        native_out.parent.mkdir(parents=True, exist_ok=True)
        if native_out.exists():
            self.ctx.remove_owned(native_out)
        preview_call = self.ctx.py("direct:missing-preview",
                                   lambda: self.ox.report(exc, native_out))
        self.assertTrue(preview_call.ok, preview_call.msg)
        py_preview = preview_call.report
        py_receipt = self._preview_stage(py_preview, "native missing preview")
        py_digest = sha256_file(py_receipt)
        create_call = self.ctx.py("direct:missing-create",
                                  lambda: self.ox.report_create(py_receipt, py_digest, native_out))
        self.assertTrue(create_call.ok, create_call.msg)
        py_members = self._validate_created_archive(create_call.report, native_out,
                                                    "native missing create")
        py_src = json.loads(py_members["error-source.json"]["bytes"])
        self.assertEqual(py_src["failures"], [], "unavailable source must stay empty, not fabricated")
        self.assertEqual(py_src["status"], "unavailable")
        # CLI envelope for the same missing input (honest comparison, not equality).
        cli = self.ctx.cli("summary", missing)
        envelope = hard_error_envelope(cli.stderr)
        self.assertIsNotNone(envelope, "CLI must emit envelope for missing file")
        self.assertIn(envelope["error"]["kind"], ("missing_input", "pack_input", "io"),
                      f"unexpected missing kind: {envelope['error']['kind']}")
        # Carried partial: strict trace error carries 2 producer errors (fixed wheel).
        strict_target = self.ctx.work / "direct-partial.blend"
        shutil.copyfile(FIXTURES.dangling, strict_target)
        strict_call = self.ctx.py("direct:strict-source", lambda: self.ox.trace(strict_target))
        self.assertFalse(strict_call.ok, "strict trace on dangling must raise")
        strict_exc = strict_call.exception
        self.assertIsNotNone(strict_exc.report, "strict error must retain its partial report")
        partial = strict_exc.report.to_json()
        self.assertFalse(partial["complete"])
        self.assertEqual(len(partial["errors"]), 2, "partial must carry 2 producer errors")
        self.assertIsNone(getattr(strict_exc, "source_snapshot", None),
                          "strict partial uses carried report, not a single snapshot")
        partial_out = self.ctx.claim("direct-report/partial-native/bundle.zip")
        partial_out.parent.mkdir(parents=True, exist_ok=True)
        if partial_out.exists():
            self.ctx.remove_owned(partial_out)
        partial_preview_call = self.ctx.py("direct:partial-preview",
                                           lambda: self.ox.report(strict_exc, partial_out))
        # Fixed contract: direct_error_report with producer_report failures.
        # Old venv313 leaves error-source empty; document, do not weaken.
        if not partial_preview_call.ok:
            STORE.write("direct-report/partial-known-pending.json",
                        {"note": ("old venv313 cannot publish direct_error_report partials; "
                                  "fixed venv311 proves status partial, input_origin "
                                  "direct_error_report, source_pointer /reports/0/errors/{i}, "
                                  "context_origin producer_report (parent L20/direct-partial-after/*)"),
                         "error": partial_preview_call.msg})
            self.fail("old wheel cannot publish carried partial (known pending engine fix; "
                      "use venv311 fixed wheel for intended contract): "
                      + str(partial_preview_call.msg))
        self.assertTrue(partial_preview_call.ok, partial_preview_call.msg)
        part_preview = partial_preview_call.report
        part_receipt = self._preview_stage(part_preview, "native partial preview")
        part_digest = sha256_file(part_receipt)
        part_create = self.ctx.py("direct:partial-create",
                                  lambda: self.ox.report_create(part_receipt, part_digest, partial_out))
        self.assertTrue(part_create.ok, part_create.msg)
        part_members = self._validate_created_archive(part_create.report, partial_out,
                                                      "native partial create")
        part_src = json.loads(part_members["error-source.json"]["bytes"])
        part_err = json.loads(part_members["error-report.json"]["bytes"])
        # Intended truthful contract (fixed venv311 wheel; parent proof
        # L20/direct-partial-after/* vs old L20/direct-partial-probe/*).
        # Old venv313 leaves status unavailable with empty failures; document,
        # do not weaken to it.
        if part_src.get("status") != "partial":
            STORE.write("direct-report/partial-known-pending.json",
                        {"note": ("known pending engine fix: strict direct_error_report "
                                  "partials must sanitize to status partial with 2 "
                                  "producer_report failures at /reports/0/errors/{i}; "
                                  "old venv313 yields unavailable/empty (see parent "
                                  "L20/direct-partial-probe/* vs fixed "
                                  "L20/direct-partial-after/*)"),
                         "observed_status": part_src.get("status"),
                         "observed_failures": len(part_src.get("failures", []))})
        self.assertEqual(part_src["status"], "partial",
                         "carried partial must sanitize to status partial, not unavailable "
                         "(known pending fix on old venv313; use fixed venv311 wheel)")
        self.assertEqual(len(part_src["failures"]), 2,
                         "both producer errors must survive the source sanitizer")
        for index, fail in enumerate(part_src["failures"]):
            self.assertEqual(fail["input_origin"], "direct_error_report",
                             f"partial failures[{index}] input_origin")
            self.assertEqual(fail["context_origin"], "producer_report",
                             f"partial failures[{index}] context_origin")
            self.assertEqual(fail["source_pointer"], f"/reports/0/errors/{index}",
                             f"partial failures[{index}] source_pointer")
            self.assertEqual(fail["input_binding"], self.BINDING_LABEL)
            self.assertTrue(_is_kind_token(fail["id"]), f"partial failures[{index}] id")
            self.assertTrue(_is_kind_token(fail["context"]["file_token"]))
        # Direct allow_incomplete partial (same bytes, no raise) uses direct_result_report.
        allow_call = self.ctx.py("direct:allow-partial",
                                 lambda: self.ox.trace(strict_target, allow_incomplete=True))
        self.assertTrue(allow_call.ok, allow_call.msg)
        allow_out = self.ctx.claim("direct-report/allow-native/bundle.zip")
        allow_out.parent.mkdir(parents=True, exist_ok=True)
        if allow_out.exists():
            self.ctx.remove_owned(allow_out)
        allow_preview = self.ctx.py("direct:allow-preview",
                                    lambda: self.ox.report(allow_call.object, allow_out))
        self.assertTrue(allow_preview.ok, allow_preview.msg)
        allow_receipt = self._preview_stage(allow_preview.report, "allow partial preview")
        allow_digest = sha256_file(allow_receipt)
        allow_created = self.ctx.py("direct:allow-create",
                                    lambda: self.ox.report_create(allow_receipt, allow_digest, allow_out))
        self.assertTrue(allow_created.ok, allow_created.msg)
        allow_members = self._validate_created_archive(allow_created.report, allow_out,
                                                      "allow partial create")
        allow_src = json.loads(allow_members["error-source.json"]["bytes"])
        self.assertEqual(allow_src["status"], "partial")
        for index, fail in enumerate(allow_src["failures"]):
            self.assertEqual(fail["input_origin"], "direct_result_report",
                             f"allow failures[{index}] input_origin")
            self.assertEqual(fail["context_origin"], "producer_report")
            self.assertEqual(fail["source_pointer"], f"/errors/{index}")
        STORE.write("direct-report/partial-contracts.json",
                    {"strict_input_origin": part_src["failures"][0]["input_origin"] if part_src["failures"] else None,
                     "strict_source_pointer": part_src["failures"][0].get("source_pointer") if part_src["failures"] else None,
                     "allow_input_origin": allow_src["failures"][0]["input_origin"] if allow_src["failures"] else None,
                     "allow_source_pointer": allow_src["failures"][0].get("source_pointer") if allow_src["failures"] else None})
        for path in (native_out, partial_out, allow_out):
            self.ctx.remove_owned(path)
        LEDGER.add("direct-report", "missing-and-partial", "honest", "ok", None)

    def test_05_negative_mutation_and_alias_topology(self) -> None:
        api = direct_report_api(self.ox)
        if api is None:
            self.skipTest("no direct report API: " + DIRECT_REPORT_NEEDED)
        self.row("direct-report", "negative", "mutation", lambda: None)
        failure = self.ctx.py("direct:negative-source",
                              lambda: self.ox.pointer(FIXTURES.dangling, 0, ["id", "lib"]))
        self.assertFalse(failure.ok)
        snap = failure.exception.source_snapshot.to_json()
        cli = self.ctx.cli("pointer", FIXTURES.dangling, "0", json.dumps(["id", "lib"]), "0")
        envelope = hard_error_envelope(cli.stderr)
        self.assertIsNotNone(envelope)
        f0 = envelope["diagnostic"]["failures"][0]
        # Mutating a source block coordinate must be detected, not normalized away.
        mutated = json.loads(json.dumps(snap))
        mutated["context"]["block_index"] = 999
        diffs = json_diff(mutated, f0, label="negative:block_index")
        self.assertTrue(any(p.endswith("block_index") for p, _a, _b in diffs),
                        "changing block_index must produce a diff")
        STORE.write("direct-report/negative-block-index.json",
                    {"mutated_path": "$.context.block_index", "mutated_to": 999,
                     "diffs": [{"path": p, "python": exact(a), "cli": exact(b)}
                               for p, a, b in diffs]})
        mutated2 = json.loads(json.dumps(snap))
        mutated2["context"]["pointer_decimal"] = "0"
        diffs2 = json_diff(mutated2, f0, label="negative:pointer")
        self.assertTrue(any("pointer_decimal" in p for p, _a, _b in diffs2),
                        "changing pointer_decimal must produce a diff")
        # Token alias topology mismatch must be detected: swapping two distinct
        # kind tokens at different sites must fail the bijection check.
        py_src_probe = {"a": "kind:" + "0" * 64, "b": "kind:" + "1" * 64}
        cli_src_probe = {"a": "kind:" + "2" * 64, "b": "kind:" + "2" * 64}
        alias = TokenAlias()
        # First site maps 0* -> 2*; second site reuses 2* for a different native
        # token, which must raise a topology mismatch.
        alias.observe(py_src_probe["a"], cli_src_probe["a"], "$.a")
        with self.assertRaises(AssertionError) as mismatch:
            alias.observe(py_src_probe["b"], cli_src_probe["b"], "$.b")
        STORE.write("direct-report/negative-alias-topology.json",
                    {"mismatch": str(mismatch.exception),
                     "native": py_src_probe, "cli": cli_src_probe})

        # Pre-established mapping plus swapped two-token string must fail:
        # N1->C1 and N2->C2 are already observed; pairing N2,N1 against C1,C2
        # in order must raise (positional pairing, not set matching).
        pre = TokenAlias()
        n1, n2 = "kind:" + "3" * 64, "kind:" + "4" * 64
        c1, c2 = "kind:" + "5" * 64, "kind:" + "6" * 64
        pre.observe(n1, c1, "$.pre[0]")
        pre.observe(n2, c2, "$.pre[1]")
        swapped_native = f"prefix {n2} middle {n1} suffix"
        swapped_cli = f"prefix {c1} middle {c2} suffix"
        with self.assertRaises(AssertionError) as swapped_mismatch:
            self.assert_aliased_equal(swapped_cli, swapped_native, pre, "$.swapped")
        STORE.write("direct-report/negative-swapped-order.json",
                    {"mismatch": str(swapped_mismatch.exception),
                     "native": swapped_native, "cli": swapped_cli})
        # Primitive type strictness: bool is not int.
        with self.assertRaises(AssertionError) as type_mismatch:
            self.assert_aliased_equal(1, True, TokenAlias(), "$.bool-int")
        STORE.write("direct-report/negative-bool-int.json",
                    {"mismatch": str(type_mismatch.exception)})
        # Neither a trailing newline nor a 65th hex digit is part of the
        # protocol's exact kind:<64hex> token; literal differences must survive.
        with self.assertRaises(AssertionError):
            self.assert_aliased_equal(c1, n1 + "\n", TokenAlias(), "$.token-newline")
        with self.assertRaises(AssertionError):
            self.assert_aliased_equal(c1 + "f", n1 + "f", TokenAlias(), "$.token-too-long")
        # Real aliasing still works when topology matches.
        good = TokenAlias()
        good.observe("kind:" + "0" * 64, "kind:" + "a" * 64, "$.a")
        good.observe("kind:" + "0" * 64, "kind:" + "a" * 64, "$.b-same")
        good.check_bijection()
        LEDGER.add("direct-report", "negative", "mutation", "ok", None)

    def test_06_disclosure_consent_is_exact(self) -> None:
        api = direct_report_api(self.ox)
        if api is None:
            self.skipTest("no direct report API: " + DIRECT_REPORT_NEEDED)
        self.row("direct-report", "disclosure", "consent", lambda: None)
        failure = self.ctx.py("direct:disclosure-source",
                              lambda: self.ox.pointer(FIXTURES.dangling, 0, ["id", "lib"]))
        self.assertFalse(failure.ok)
        for output_name in ("direct-report/disclosure-native/bundle.zip",):
            destination = self.ctx.claim(output_name)
            destination.parent.mkdir(parents=True, exist_ok=True)
            if destination.exists():
                self.ctx.remove_owned(destination)
            call = self.ctx.py("direct:disclosure-preview",
                               lambda: self.ox.report(failure.exception, destination))
            self.assertTrue(call.ok, call.msg)
            preview = call.report
            self.assertEqual(preview["privacy"]["include_names"], False)
            self.assertEqual(preview["privacy"]["include_file"], False)
            self.assertEqual(preview["privacy"]["profile"], "structural")
            self.assertEqual(preview["privacy"]["pseudonym_scheme"], "HMAC-SHA-256/bundle-local")
            receipt = self._preview_stage(preview, "disclosure preview")
            digest = sha256_file(receipt)
            created = self.ctx.py("direct:disclosure-create",
                                  lambda: self.ox.report_create(receipt, digest, destination))
            self.assertTrue(created.ok, created.msg)
            members = self._validate_created_archive(created.report, destination,
                                                     "disclosure create")
            for name, member in members.items():
                if not name.endswith(".json"):
                    continue
                text = member["bytes"].decode("utf-8", "replace")
                self.assertNotIn(fs_text(self.ctx.work), text,
                                 f"{name} discloses work path")
                self.assertNotIn(fs_text(self.ctx.corpus_root), text,
                                 f"{name} discloses corpus root")
                # Raw path hex for the harness work dir must not appear as a
                # disclosed absolute path; engine hex fields already compared
                # exactly in test_02, so any raw work path here is a leak.
                self.assertNotIn(os.fsdecode(os.fsencode(self.ctx.work)), text,
                                 f"{name} discloses work path bytes")
            # Consent is exact, not merely falsy.
            manifest = json.loads(members["manifest.json"]["bytes"])
            self.assertEqual(manifest["consent"], {"file_opt_in": False, "names_opt_in": False})
            self.assertFalse(members["build.json"]["bytes"] == 0)
            self.ctx.remove_owned(destination)
        LEDGER.add("direct-report", "disclosure", "consent", "ok", None)


class ExrParity(ParityCase):
    def test_01_header(self) -> None:
        targets = list(self.ctx.exrs) + [FIXTURES.image]
        self.assertTrue(targets, "no EXR inputs found")
        for path in targets:
            with self.subTest(path=fs_text(path)):
                self.row("exr-header", Path(path).name, "default", lambda p=path: self.parity(
                    f"exr-header:{Path(p).name}", lambda: self.ox.exr_header(p), ("exr-header", p)))
        for path in (FIXTURES.truncated, FIXTURES.garbage):
            with self.subTest(path=fs_text(path)):
                self.row("exr-header", Path(path).name, "invalid", lambda p=path: self.parity(
                    f"exr-header:{Path(p).name}", lambda: self.ox.exr_header(p), ("exr-header", p)))

    def test_02_scan(self) -> None:
        # jobs=1 is deterministic: compare the exact ordered JSONL sequence.
        cli = self.ctx.cli("exr-scan", FIXTURES.root, "--jobs", "1")
        records = [json.loads(line) for line in cli.stdout.splitlines() if line.strip()]
        call = self.ctx.py("exr_scan", lambda: self.ox.exr_scan([FIXTURES.root], jobs=1))
        self.assertTrue(call.ok, call.msg)
        self.assertEqual(call.report, records, "exr-scan ordered record sequence differs")
        self.assert_report_contract("exr-scan", call)
        self.assertEqual(call.object.summary.to_json(), records[-1])
        # Default worker count may reorder file records; compare the keyed multiset
        # and the summary separately.
        explicit = [row["path"] for row in self.ctx.inputs] + list(self.ctx.exrs) + [FIXTURES.image]
        list_file = self.ctx.work / "exr-inputs.lst"
        list_file.write_bytes(b"\n".join(fs_text(p).encode() for p in explicit) + b"\n")
        cli = self.ctx.cli("exr-scan", "--files-from", list_file)
        default_records = [json.loads(line) for line in cli.stdout.splitlines() if line.strip()]
        call = self.ctx.py("exr_scan", lambda: self.ox.exr_scan(self.ox.read_path_list(list_file)))
        self.assertTrue(call.ok, call.msg)
        left = {row["absolute_path_hex"]: row for row in call.report[:-1]}
        right = {row["absolute_path_hex"]: row for row in default_records[:-1]}
        self.assertEqual(sorted(left), sorted(right), "exr-scan file set differs")
        for key in left:
            self.assertEqual(left[key], right[key], f"exr-scan record differs for {key}")
        self.assertEqual({k: v for k, v in call.report[-1].items() if k != "jobs"},
                         {k: v for k, v in default_records[-1].items() if k != "jobs"},
                         "exr-scan summary differs")
        STORE.write("exr-scan-ordering.json", {
            "reason": "exr-scan writes records as workers finish; order is not part of the contract at jobs>1",
            "ordered_comparison": "jobs=1",
            "unordered_comparison": "records keyed by absolute_path_hex plus summary"})

    def test_03_pixels(self) -> None:
        targets = [FIXTURES.image] + list(self.ctx.exrs)
        for path in targets:
            header = parse_json_stdout(self.ctx.cli("exr-header", path))
            if header is MISSING or header.get("pixel_integrity_verified") is not False:
                continue
            part = 0
            channels = header["parts"][part]["channels"]
            prefixed = [c["name"] for c in channels if c["name"] and "." in c["name"]]
            pass_name = prefixed[0].rsplit(".", 1)[0] if prefixed else ""
            destination = self.ctx.claim(f"exr-pixels/{Path(path).name}.f32")
            with self.subTest(path=fs_text(path), pass_name=pass_name):
                self.row("exr-pixels", Path(path).name, pass_name or "<empty>", lambda p=path, d=destination, s=pass_name: self.write_parity(
                    f"exr-pixels:{Path(p).name}", d,
                    lambda: self.ox.exr_pixels(p, d, part=part, pass_name=s),
                    ("exr-pixels", p, "--part", str(part), "--pass", s, "--output", d)))
        # The synthetic image has authored pixels: compare them exactly.
        destination = self.ctx.claim("exr-pixels/synthetic.f32")
        call = self.ctx.py("exr_pixels", lambda: self.ox.exr_pixels(FIXTURES.image, destination, part=0, pass_name=""))
        self.assertTrue(call.ok, call.msg)
        self.assert_authored_pixels("exr-pixels:synthetic", destination.read_bytes())
        self.assertTrue(call.report["decode_complete"])
        self.ctx.remove_owned(destination)


class ErrorParity(ParityCase):
    def test_01_input_and_project_errors(self) -> None:
        missing = self.ctx.work / "missing.blend"
        destination = self.ctx.claim("errors/pack-out")
        for command, fn, argv in (
            ("summary", lambda: self.ox.summary(missing), ("summary", missing)),
            ("inspect", lambda: self.ox.inspect(missing), ("inspect", missing)),
            ("trace", lambda: self.ox.trace(missing), ("trace", missing)),
            ("list", lambda: self.ox.list_dependencies(missing), ("list", missing, "--json")),
            ("blocks", lambda: self.ox.block_report(missing), ("blocks", missing)),
            ("pack", lambda: self.ox.pack(missing, destination), ("pack", missing, destination)),
        ):
            with self.subTest(command=command):
                self.row(command, "missing-input", "error", lambda f=fn, a=argv, c=command: self.parity(
                    f"{c}:missing-input", f, a))
        project = self.ctx.work / "no-such-project"
        with self.subTest(command="pack-project"):
            self.row("pack", "missing-project", "error", lambda: self.parity(
                "pack:missing-project",
                lambda: self.ox.pack(FIXTURES.minimal, destination, project_root=project),
                ("pack", FIXTURES.minimal, destination, "--project", project)))
        # A missing corpus input selected for a multi-root pack is pack_input (exit 3).
        with self.subTest(command="pack-sequence-input"):
            self.row("pack_sequence", "missing-input", "error", lambda: self.parity(
                "pack-sequence:missing-input",
                lambda: self.ox.pack_sequence([FIXTURES.minimal, missing], destination),
                ("pack-sequence", FIXTURES.minimal, missing, "-t", destination)))

    def test_02_output_exists(self) -> None:
        spec = self.ctx.work / "rewrite-spec.json"
        spec.write_text(json.dumps(rewrite_spec_document()))
        existing = self.ctx.claim("errors/existing.blend")
        existing.write_bytes(b"occupied")
        with self.subTest(command="rewrite"):
            self.row("rewrite", "existing-output", "error", lambda: self.parity(
                "rewrite:existing-output",
                lambda: self.ox.rewrite(FIXTURES.rewrite, existing, self._spec()),
                ("rewrite", FIXTURES.rewrite, "--output", existing, "--edits", spec)))
        with self.subTest(command="extract"):
            self.row("extract", "existing-output", "error", lambda: self.parity(
                "extract:existing-output",
                lambda: self.ox.extract(FIXTURES.minimal, existing, blocks=(0,)),
                ("extract", FIXTURES.minimal, "--block", "0", "--output", existing, "--json")))
        with self.subTest(command="pack"):
            self.row("pack", "existing-output", "error", lambda: self.parity(
                "pack:existing-output",
                lambda: self.ox.pack(FIXTURES.minimal, existing),
                ("pack", FIXTURES.minimal, existing)))

    def _spec(self) -> dict:
        return rewrite_spec_document()

    def test_03_structural_errors(self) -> None:
        cases = [
            ("invalid_header", lambda: self.ox.inspect(FIXTURES.garbage), ("inspect", FIXTURES.garbage)),
            ("truncated", lambda: self.ox.inspect(FIXTURES.truncated), ("inspect", FIXTURES.truncated)),
            ("get-bad-field", lambda: self.ox.get(FIXTURES.minimal, 0, ["no_such_field"]),
             ("get", FIXTURES.minimal, "0", json.dumps(["no_such_field"]))),
            ("get-bad-block", lambda: self.ox.get(FIXTURES.minimal, 2**31 - 1, ["name"]),
             ("get", FIXTURES.minimal, str(2**31 - 1), json.dumps(["name"]))),
            ("pointer-bad-field", lambda: self.ox.pointer(FIXTURES.chain, 0, ["different"]),
             ("pointer", FIXTURES.chain, "0", json.dumps(["different"]))),
            ("payload-bad-block", lambda: self.ox.payload(FIXTURES.minimal, 9999),
             ("payload", FIXTURES.minimal, "9999")),
            ("edit-version", lambda: self.ox.edit(FIXTURES.minimal, self.ctx.claim("errors/edit.blend"),
                                                  {"schema": 2, "writes": [], "bits": []}, mode="format-only"),
             None),
        ]
        for name, fn, argv in cases:
            with self.subTest(case=name):
                if argv is None:
                    call = self.ctx.py(name, fn)
                    self.assertFalse(call.ok, f"{name}: expected a refusal")
                    self.assertIsNotNone(call.kind, f"{name}: refusal without a catalog kind")
                    self.assertIn(call.kind, self.ox.ERROR_TYPES)
                    self.row(name, "fixture", "error", lambda: None)
                    continue
                self.row(name.split("-")[0], "fixture", name, lambda f=fn, a=argv: self.parity(name, f, a))

    def test_04_incomplete_reports_and_strict_delivery(self) -> None:
        # Authored fixtures make both single- and multi-root refusal variants
        # mandatory even when the selected corpus contains only one root.
        roots = (FIXTURES.dangling, FIXTURES.minimal)
        with self.ox.open_many(roots) as captured:
            cases = (
                ("trace:single", "trace",
                 lambda: self.ox.trace(FIXTURES.dangling, allow_incomplete=True),
                 lambda: self.ox.trace(FIXTURES.dangling), ("trace", FIXTURES.dangling)),
                ("trace:paths:multi", "trace",
                 lambda: self.ox.trace(roots, allow_incomplete=True),
                 lambda: self.ox.trace(roots), ("trace", *roots)),
                ("trace:File:multi", "trace",
                 lambda: captured.trace(allow_incomplete=True),
                 lambda: captured.trace(), ("trace", *roots)),
                ("list:paths:multi", "list",
                 lambda: self.ox.list_dependencies(roots, allow_incomplete=True),
                 lambda: self.ox.list_dependencies(roots), ("list", *roots, "--json")),
                ("list:File:multi", "list",
                 lambda: captured.list_dependencies(allow_incomplete=True),
                 lambda: captured.list_dependencies(), ("list", *roots, "--json")),
            )
            for label, command, allow, strict, argv in cases:
                with self.subTest(case=label):
                    cli = self.ctx.cli(*argv)
                    report = parse_json_stdout(cli)
                    self.assertNotEqual(cli.exit, 0, f"{label}: authored incomplete CLI case succeeded")
                    self.assertFalse(report["complete"], f"{label}: authored report became complete")
                    permitted = self.ctx.py(f"{label}:allow", allow)
                    self.assertTrue(permitted.ok, permitted.msg)
                    self.assert_same(f"{label}:allow", permitted.report, report)
                    self.assert_report_contract(f"{label}:allow", permitted)
                    strict_call = self.ctx.py(f"{label}:strict", strict)
                    self.assertFalse(strict_call.ok, f"{label}: strict delivery must raise")
                    envelope = hard_error_envelope(cli.stderr)
                    self.assertIsNotNone(envelope,
                                         f"{label}: strict CLI refusal lacks a canonical error envelope")
                    self.assert_error_matches_envelope(
                        f"{label}:strict full error", strict_call, envelope, cli)
                    self.assertIsNotNone(strict_call.report, f"{label}: error must retain the report")
                    self.assert_same(f"{label}:strict report", strict_call.report, report)
                    self.assertEqual(strict_call.exception.effects, "none")
                    self.row(command, "dangling-fixture", label, lambda: None)

    def test_05_incomplete_corpus_root(self) -> None:
        """A root copied without its libraries is a real incomplete case."""
        copied = None
        for row in self.inputs():
            source = row["path"]
            target_dir = self.ctx.work / "isolated" / row["relative"].replace("/", "__")
            target_dir.mkdir(parents=True, exist_ok=True)
            target = target_dir / source.name
            shutil.copyfile(source, target)
            cli = self.ctx.cli("trace", target)
            if cli.exit != 0 and parse_json_stdout(cli) is not MISSING:
                copied = target
                break
            shutil.rmtree(target_dir)
        if copied is None:
            STORE.write("incomplete-corpus.json", {"found": False,
                        "note": "no corpus root became incomplete when copied without its libraries"})
            self.skipTest("no corpus root becomes incomplete in isolation; authored fixture covers the path")
        permitted = self.ctx.py("trace:isolated", lambda: self.ox.trace(copied, allow_incomplete=True))
        cli = self.ctx.cli("trace", copied)
        report = parse_json_stdout(cli)
        self.assertTrue(permitted.ok, permitted.msg)
        self.assert_same("trace:isolated", permitted.report, report)
        self.assertFalse(report["complete"], "isolation did not produce an incomplete report")
        # Package refusal parity for the same requirement failure.
        destination = self.ctx.claim("errors/isolated-pack")
        self.row("pack", "incomplete", "strict", lambda: self.parity(
            "pack:isolated-strict",
            lambda: self.ox.pack(copied, destination, project_root=copied.parent),
            ("pack", copied, destination, "--project", copied.parent)))
        STORE.write("incomplete-corpus.json", {"found": True, "path": fs_text(copied)})

    def test_06_binding_lifecycle_errors(self) -> None:
        with self.ox.open(FIXTURES.minimal) as first, self.ox.open(FIXTURES.chain) as second:
            block = first.blocks()[0]
            query = self.ox.FieldQuery(block, 0, (b"name",))
            with self.subTest(kind="foreign_block"):
                call = self.ctx.py("foreign_block", lambda: second.get([query]))
                self.assertFalse(call.ok)
                self.assertIn(call.kind, ("usage", "foreign_block"))
                self.assertIsInstance(call.exception, self.ox.OxblendError)
            with self.subTest(kind="closed_file"):
                first.close()
                call = self.ctx.py("closed_file", lambda: first.summary())
                self.assertFalse(call.ok)
                self.assertEqual(call.kind, "closed_file")
                self.assertIsInstance(call.exception, self.ox.ClosedFileError)
                self.assertEqual(call.exception.exit_code, 2)
        with self.subTest(kind="input_changed"):
            target = self.ctx.work / "changed.blend"
            shutil.copyfile(FIXTURES.minimal, target)
            handle = self.ox.open(target)
            with target.open("ab") as stream:
                stream.write(b"x")
            call = self.ctx.py("input_changed", handle.summary)
            self.assertFalse(call.ok)
            self.assertEqual(call.kind, "input_changed")
            self.assertIsInstance(call.exception, self.ox.SourceChangedError)
            self.assertEqual(self.ox.ERROR_TYPES["input_changed"], self.ox.SourceChangedError)
            handle.close()


class LifecycleParity(ParityCase):
    def test_00_engine_header_classification(self) -> None:
        fixture = REPO / 'bindings/python/tests/fixtures/native521/qualifier.blend'
        known = [(fixture, 502, 1)]
        older = self.ctx.corpus_root / 'UDIM_monster/UDIM_monster/udim-monster.blend'
        if older.is_file():
            known.append((older, 282, 0))
        probe = Ctx(self.ox, self.ctx.oracle, self.ctx.corpus_root, self.ctx.work)
        probe.inputs = [{'path': path} for path, _, _ in known]
        observations = []
        for path, version, file_format in known:
            fact = probe.header_for(path)
            self.assertEqual(fact['version'], version)
            self.assertEqual(fact['file_format'], file_format)
            self.assertIn(path, probe.by_version(version))
            self.assertNotIn(path, probe.by_version(version + 1))
            observations.append({'path': fs_text(path), 'expected_version': version,
                                 'expected_format': file_format, 'header': fact})
        self.assertEqual(probe.by_version(502), [fixture])
        invalid = self.ctx.work / 'header-unknown.blend'
        invalid.write_bytes(b'not a Blender file')
        with self.assertRaisesRegex(AssertionError, 'header facts unavailable'):
            probe.header_for(invalid)
        STORE.write('header-classifier-regression.json', {
            'known': observations, 'older_public_corpus_covered': older.is_file(),
            'unknown_fails_loudly': True})

    def test_01_open_many_contract(self) -> None:
        roots = self.two_roots()
        self.assertGreaterEqual(len(roots), 1)
        if not hasattr(self.ox, "open_many"):
            CTX.gaps.append("open_many is absent: multi-root File retention (amendment B) is untested end to end")
            return
        many = self.ox.open_many(roots)
        try:
            self.assertEqual(many.memory, "fast")
            cli = self.ctx.cli("trace", *roots)
            call = self.ctx.py("File.trace", lambda: many.trace(allow_incomplete=True))
            self.assertTrue(call.ok, call.msg)
            self.assert_same("File.trace:multi-root", call.report, parse_json_stdout(cli))
            listed = self.ctx.py("File.list", lambda: many.list_dependencies(allow_incomplete=True))
            self.assertTrue(listed.ok, listed.msg)
            self.assert_same("File.list:multi-root", listed.report,
                             parse_json_stdout(self.ctx.cli("list", "--json", *roots)))
        finally:
            many.close()
        # Separately captured Files must refuse combination with a named remedy.
        with self.ox.open(roots[0]) as one, self.ox.open(roots[-1]) as two:
            call = self.ctx.py("roots:mixed", lambda: self.ox.trace([one, two]))
            self.assertFalse(call.ok, "multiple Files must not be combined")
            self.assertEqual(call.kind, "usage")
            self.assertIn("open_many", call.msg)
        with self.ox.open(roots[0]) as one:
            call = self.ctx.py("roots:mixed", lambda: self.ox.list_dependencies([fs_text(roots[0]), one]))
            self.assertFalse(call.ok, "a File mixed with paths must not be combined")
            self.assertIn("open_many", call.msg)

    def test_02_memory_policy(self) -> None:
        path = self.ctx.small_blend()
        with self.ox.open(path, memory="bounded") as handle:
            self.assertEqual(handle.memory, "bounded")
            call = self.ctx.py("summary:bounded", handle.summary)
            self.assertTrue(call.ok, call.msg)
            self.assert_same("summary:bounded", call.report, parse_json_stdout(self.ctx.cli("summary", path)))
        destination = self.ctx.claim("memory/mismatch")
        with self.ox.open(FIXTURES.minimal, memory="fast") as handle:
            call = self.ctx.py("pack:memory-mismatch", lambda: handle.pack(destination, memory="bounded"))
            self.assertFalse(call.ok, "an explicit differing memory policy must refuse")
            self.assertIn(call.kind, self.ox.ERROR_TYPES)
            self.assertIn("memory", call.msg.lower())
        # File.pack(memory=None) keeps the captured policy and matches the CLI bytes.
        destination = self.ctx.claim("memory/inherit/pkg")
        self.row("pack", "fixture", "memory-inherit", lambda: self.write_parity(
            "pack:memory-inherit", destination,
            lambda: self.ox.pack(FIXTURES.minimal, destination, allow_incomplete=True),
            ("pack", FIXTURES.minimal, destination)))
        # claim() allocates a NEW owned name on every call; select once so
        # both producers receive literally identical destination bytes.
        bounded_destination = self.ctx.claim("memory/bounded/pkg")
        cli = self.ctx.cli("pack", FIXTURES.minimal, bounded_destination, "--memory", "bounded",
                           "--dry-run")
        call = self.ctx.py("pack:bounded", lambda: self.ox.pack(
            FIXTURES.minimal, bounded_destination, memory="bounded", dry_run=True,
            allow_incomplete=True))
        self.assertTrue(call.ok, call.msg)
        self.assert_same("pack:bounded", call.report, parse_json_stdout(cli))

    def test_03_threads(self) -> None:
        path = self.ctx.small_blend()
        with self.ox.open(path) as handle:
            expected = handle.summary().to_json()
            barrier = threading.Barrier(2)
            results: list[object] = []
            errors: list[BaseException] = []

            def read() -> None:
                try:
                    barrier.wait()
                    results.append(handle.summary().to_json())
                except BaseException as exc:  # noqa: BLE001
                    errors.append(exc)

            threads = [threading.Thread(target=read) for _ in range(2)]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join()
            self.assertFalse(errors, errors)
            self.assertEqual(results, [expected, expected])
        # Concurrent reads on one captured File must not corrupt or serialize
        # into different bytes; the module declares gil_used=false, so this also
        # exercises the free-threaded call path (timing is not asserted).
        native = importlib.import_module("oxblend._native")
        self.assertEqual(native.__spec__.name, "oxblend._native")


    def test_04_multi_scalar_arity_and_entry(self) -> None:
        first = self.ctx.claim('lifecycle/first.blend')
        second = self.ctx.claim('lifecycle/second.blend')
        first.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(FIXTURES.minimal, first)
        shutil.copyfile(FIXTURES.chain, second)
        with self.ox.open(first) as single:
            query = self.ox.FieldQuery(single.blocks()[0], 0, (b'name',))
            with self.ox.open_many([first, second, first]) as many:
                self.assertEqual(many.root_paths, (str(first), str(second)))
                operations = {
                    'header': lambda: many.header, 'summary': many.summary,
                    'blocks': many.blocks, 'get': lambda: many.get([]),
                    'pointer': lambda: many.pointer(query),
                    'byte_array': lambda: many.byte_array(query, 1),
                    'pointer_table': lambda: many.pointer_table(query, 1),
                    'query': lambda: many.query(type='Scene', fields=['id.name']),
                    'explain': lambda: many.explain('Scene'), 'inspect': many.inspect,
                    'payload': lambda: many.payload(0), 'source_facts': many.source_facts,
                    'library_facts': many.library_facts, 'census': many.pointer_census,
                    'pack': lambda: many.pack(self.ctx.claim('lifecycle/refused-pack')),
                }
                for name, operation in operations.items():
                    with self.subTest(scalar=name):
                        call = self.ctx.py('multi-scalar:' + name, operation)
                        self.assertFalse(call.ok, f'{name} silently selected root zero')
                        self.assertEqual(call.kind, 'usage')
                        self.assertEqual(call.exit_code, 2)
                        self.assertIn('pack_sequence' if name == 'pack' else 'open(path)', call.msg)
                self.assertFalse(self.ctx.claim('lifecycle/refused-pack').exists())
                with second.open('ab') as stream:
                    stream.write(b'changed nonzero captured root')
                call = self.ctx.py('multi:enter-changed-second', many.__enter__)
                self.assertFalse(call.ok)
                self.assertEqual(call.kind, 'input_changed')
                self.assertIsInstance(call.exception, self.ox.SourceChangedError)
        with self.ox.open_many([first, first]) as one:
            self.assertEqual(one.root_paths, (str(first),))
            self.assertGreater(one.header.version, 0)
        call = self.ctx.py('multi:empty-selection', lambda: self.ox.open_many([]))
        self.assertFalse(call.ok)
        self.assertEqual(call.kind, 'usage')


    def test_05_captured_pack_sequence(self) -> None:
        first = self.ctx.claim('lifecycle-pack/first.blend')
        second = self.ctx.claim('lifecycle-pack/second.blend')
        first.parent.mkdir(parents=True, exist_ok=True)
        # The chain fixture declares unresolved image assets; use two complete
        # authored inputs so refusal parity cannot masquerade as publication.
        shutil.copyfile(FIXTURES.minimal, first)
        shutil.copyfile(FIXTURES.minimal, second)
        destination = self.ctx.claim('lifecycle-pack/package')
        with self.ox.open_many([first, second]) as many:
            _, artifact = self.write_parity('File.pack_sequence', destination,
                              lambda: many.pack_sequence(destination, project_root=first.parent),
                              ('pack-sequence', first, second, '-t', destination, '-p', first.parent))
            self.assertEqual(artifact['kind'], 'dir', 'captured sequence did not publish')
            self.assertEqual(many.root_paths, (str(first), str(second)))
            self.assertTrue(many.trace().complete)
        self.ctx.remove_owned(destination)


class SyntheticParity(ParityCase):
    def test_01_authored_values(self) -> None:
        """Ground truth authored here, independent of the CLI's own JSON."""
        path = FIXTURES.minimal
        with self.ox.open(path) as handle:
            block = handle.blocks()[0]
            values = handle.get([
                self.ox.FieldQuery(block, 0, (b"number",)),
                self.ox.FieldQuery(block, 0, (b"name",)),
                self.ox.FieldQuery(block, 0, (b"name",), False),
                self.ox.FieldQuery(block, 0, (b"scalar",)),
                self.ox.FieldQuery(block, 0, (b"array", 1)),
                self.ox.FieldQuery(block, 0, (b"f32",)),
            ])
            self.assertEqual(values[0].value, 2**64 - 1)
            self.assertEqual(values[1].value, b"N\xff")
            self.assertEqual(values[2].value, b"N\xff".ljust(8, b"\0"))
            self.assertEqual(values[3].value, -(2**31))
            self.assertEqual(values[4].value, -(2**31))
            self.assertEqual(values[5].value.bits, 0x7FC12345)
            self.assertEqual(values[5].kind, "float32")
            payload = self.ox.payload(path, 0)
            original = FIXTURES.bytes["minimal"]
            expected = original[12 + 24: 12 + 24 + 48].hex()
            self.assertEqual(payload.to_json()["value"]["hex"], expected)
        # Authored ID graph: block 0 -> block 1 via id.next, and back via id.prev.
        chain = FIXTURES.chain
        with self.ox.open(chain) as handle:
            # Two authored ID blocks plus DNA1 exist physically; select IDs
            # explicitly, preserving their physical block indices.
            blocks = handle.blocks(code=b"IM\0\0")
            self.assertEqual(len(blocks), 2)
            self.assertEqual([block.code for block in blocks], [b"IM\0\0", b"IM\0\0"])
            forward = handle.pointer(self.ox.FieldQuery(blocks[0], 0, (b"id", b"next")))
            self.assertEqual(forward.index, 1, "id.next must resolve to the second block")
            self.assertEqual(handle.pointer(self.ox.FieldQuery(blocks[1], 0, (b"id", b"next"))), None)
            backward = handle.pointer(self.ox.FieldQuery(blocks[1], 0, (b"id", b"prev")))
            self.assertEqual(backward.index, 0, "id.prev must resolve to the first block")
            self.assertEqual(handle.get([self.ox.FieldQuery(blocks[0], 0, (b"name",))])[0].value, b"first")
            self.assertEqual(handle.get([self.ox.FieldQuery(blocks[1], 0, (b"name",))])[0].value, b"second")
        self.assertEqual(self.ox.pointer(chain, 0, ["id", "next"]).block_index, 1)
        self.assertIsNone(self.ox.pointer(chain, 1, ["id", "next"]).block_index)
        # The authored dangling Library pointer must be refused, not invented.
        with self.ox.open(FIXTURES.dangling) as handle:
            call = self.ctx.py("pointer:dangling", lambda: handle.pointer(
                self.ox.FieldQuery(handle.blocks()[0], 0, (b"id", b"lib"))))
            self.assertFalse(call.ok, "an unresolvable Library pointer must refuse")
            self.assertEqual(call.kind, "dangling_pointer")

    def test_02_exr_authored_pixels(self) -> None:
        report = self.ox.exr_header(FIXTURES.image)
        self.assertFalse(report.pixel_integrity_verified)
        self.assertEqual(len(report.parts), 1)
        names = [channel["name"] for channel in report.parts[0]["channels"]]
        self.assertEqual(names, ["G", "R"])
        destination = self.ctx.claim("synthetic/pixels.f32")
        pixels = self.ox.exr_pixels(FIXTURES.image, destination, part=0, pass_name="")
        self.assertTrue(pixels.complete)
        self.assertTrue(pixels.decode_complete)
        self.assert_authored_pixels("exr-pixels:synthetic", destination.read_bytes())
        self.assertEqual(pixels["samples"], 8, "four pixels times two channels")
        self.assertEqual(pixels["bytes"], destination.stat().st_size)
        self.assertEqual(list(pixels["channels"]), ["G", "R"])
        self.ctx.remove_owned(destination)


# --------------------------------------------------------------------------- #
# Fixture validation (runs without the binding being ready)
# --------------------------------------------------------------------------- #

def validate_fixtures(oracle: Oracle) -> dict:
    report: dict = {"fixtures": FIXTURES.manifest(), "checks": []}

    def check(name: str, argv: tuple, *, ok: bool) -> None:
        result = oracle(*argv)
        envelope = hard_error_envelope(result.stderr)
        entry = {"name": name, "argv": list(result.argv), "exit": result.exit,
                 "stdout_sha256": sha256_bytes(result.stdout), "stderr_sha256": sha256_bytes(result.stderr),
                 "kind": None if envelope is None else envelope["error"].get("kind")}
        if ok:
            entry["ok"] = result.exit == 0
        else:
            entry["ok"] = result.exit != 0 and envelope is not None and "kind" in envelope["error"]
        report["checks"].append(entry)

    check("minimal:inspect", ("inspect", FIXTURES.minimal), ok=True)
    check("minimal:summary", ("summary", FIXTURES.minimal), ok=True)
    check("minimal:get", ("get", FIXTURES.minimal, "0", json.dumps(["number"])), ok=True)
    check("minimal:payload", ("payload", FIXTURES.minimal, "0"), ok=True)
    check("chain:pointer", ("pointer", FIXTURES.chain, "0", json.dumps(["id", "next"]), "0"), ok=True)
    check("chain:get", ("get", FIXTURES.chain, "0", json.dumps(["name"]), "0"), ok=True)
    check("dangling:pointer", ("pointer", FIXTURES.dangling, "0", json.dumps(["id", "lib"]), "0"), ok=False)
    check("dangling:trace", ("trace", FIXTURES.dangling), ok=False)
    check("truncated:inspect", ("inspect", FIXTURES.truncated), ok=False)
    check("garbage:inspect", ("inspect", FIXTURES.garbage), ok=False)
    check("image:exr-header", ("exr-header", FIXTURES.image), ok=True)
    if oracle.supports("exr-pixels"):
        pixels = FIXTURES.root / "fixture-pixels.f32"
        check("image:exr-pixels", ("exr-pixels", FIXTURES.image, "--part", "0", "--pass", "",
                                   "--output", str(pixels)), ok=True)
        if pixels.exists():
            produced = pixels.read_bytes()
            documented = exr_pixel_bytes(FIXTURES.red, FIXTURES.green)
            on_disk = exr_pixel_bytes(FIXTURES.red, FIXTURES.green, layout="on-disk")
            report["synthetic_pixel_bytes"] = {
                "bytes": len(produced), "sha256": sha256_file(pixels),
                "matches_documented_layout": produced == documented,
                "matches_on_disk_channel_order": produced == on_disk,
                "matches_authored": produced in (documented, on_disk)}
            pixels.unlink()
    else:
        report["checks"].append({"name": "image:exr-pixels", "skipped": "this oracle has no exr-pixels command"})
    if oracle.supports("exr-scan"):
        check("image:exr-scan", ("exr-scan", FIXTURES.image, "--jobs", "1"), ok=True)
    else:
        report["checks"].append({"name": "image:exr-scan", "skipped": "this oracle has no exr-scan command"})
    check("not-exr:header", ("exr-header", FIXTURES.not_exr), ok=False)
    check("rewrite:inspect", ("inspect", FIXTURES.rewrite), ok=True)
    # The pointer chain must resolve to the authored target block, not merely succeed.
    pointer = oracle("pointer", FIXTURES.chain, "0", json.dumps(["id", "next"]), "0")
    if pointer.exit == 0:
        value = parse_json_stdout(pointer)
        report["chain_pointer"] = value
        report["chain_pointer_resolved"] = isinstance(value, dict) and value.get("block_index") == 1
    report["ok"] = all(entry["ok"] for entry in report["checks"] if "ok" in entry)
    return report


# --------------------------------------------------------------------------- #
# Runner
# --------------------------------------------------------------------------- #

CTX: Ctx
MANIFEST: dict | None = None


def stage_selected(name: str) -> bool:
    return ARGS.stage is None or name in ARGS.stage


def build_context() -> Ctx:
    """Environment and authored fixtures; the binding is imported separately."""
    global STORE, FIXTURES, CTX
    ARGS.evidence.mkdir(parents=True, exist_ok=True)
    os.chmod(ARGS.evidence, 0o700)
    STORE = Store(ARGS.evidence)
    work = ARGS.evidence / "work"
    marker = work / ".harness-owned"
    if work.exists() and not marker.exists():
        raise SystemExit(f"refusing to reuse a non-harness work directory: {work}")
    if work.exists() and not ARGS.keep_work:
        shutil.rmtree(work)
    work.mkdir(parents=True, exist_ok=True)
    os.chmod(work, 0o700)
    marker.write_text("owned by bindings/python/tests/full_parity.py\n")
    FIXTURES = Fixtures(work / "fixtures")
    FIXTURES.build()
    oracle = Oracle(ARGS.cli)
    oracle.verify()
    CTX = Ctx(None, oracle, ARGS.corpus, work)
    CTX.discover()
    return CTX


def load_binding() -> tuple[object | None, str | None]:
    """Import the installed distribution only; never the checkout (see docstring)."""
    try:
        return importlib.import_module("oxblend"), None
    except Exception as exc:  # noqa: BLE001 - a missing binding is an environment fact
        return None, f"{type(exc).__name__}: {exc}"


def build_readiness() -> dict:
    """Whether this checkout's extension has a complete module set to build from."""
    source = REPO / "bindings" / "python" / "src"
    declared = sorted(set(re.findall(r"^(?:pub\(crate\) )?mod (\w+);", (source / "lib.rs").read_text(), re.M)))
    missing = [name for name in declared if not (source / f"{name}.rs").is_dir()
               and not (source / f"{name}.rs").is_file()
               and not (source / name / "mod.rs").is_file()]
    return {"lib_declares": declared, "missing_modules": missing,
            "complete": not missing}


def environment_report(module) -> dict:
    oracle = CTX.oracle
    return {
        "python": sys.version,
        "executable": sys.executable,
        "platform": sys.platform,
        "module": str(module.__file__),
        "native": module._native.__file__,
        "version": module.__version__,
        "engine_version": module.engine_version,
        "editable": installed_wheel_problem(module),
        "cli": {"path": fs_text(oracle.binary), "sha256": oracle.sha256, "version": oracle.version,
                "version_stdout": oracle.version_output.decode("utf-8", "replace"),
                "commands": sorted(oracle.commands),
                "missing_required_commands": [name for name in CLI_REQUIRED if not oracle.supports(name)]},
        "engine_version_matches_cli": module.engine_version == oracle.version,
    }


def manifest() -> dict:
    global MANIFEST
    if MANIFEST is None:
        MANIFEST = {
            "schema": "oxblend.python.full-parity/1",
            "argv": sys.argv,
            "scope": {"corpus": fs_text(ARGS.corpus), "include": ARGS.include,
                      "value_coverage": ARGS.value_coverage, "stages": ARGS.stage or list(STAGES)},
            "corpus": {
                "root": fs_text(ARGS.corpus),
                "blends": len(CTX.inputs),
                "bytes": sum(row["bytes"] for row in CTX.inputs),
                "exrs": len(CTX.exrs),
                "inputs": [{"relative": row["relative"], "bytes": row["bytes"],
                            "sha256": sha256_file(row["path"])} for row in CTX.inputs],
                "exr_inputs": [{"relative": fs_text(p.relative_to(CTX.corpus_root)),
                                "bytes": p.stat().st_size, "sha256": sha256_file(p)} for p in CTX.exrs],
            },
            "fixtures": FIXTURES.manifest(),
        }
    return MANIFEST


def main() -> int:
    ctx = build_context()
    readiness = build_readiness()
    STORE.write("build-readiness.json", readiness)

    fixture_report = validate_fixtures(ctx.oracle)
    STORE.write("fixtures.json", fixture_report)
    if ARGS.fixtures_only:
        print(f"oracle {ctx.oracle.version} ({ctx.oracle.sha256}) "
              f"publishes {len(ctx.oracle.commands)} commands")
        for entry in fixture_report["checks"]:
            if "skipped" in entry:
                print(f"  skip  {entry['name']:<24} {entry['skipped']}")
            else:
                print(f"  {'ok  ' if entry['ok'] else 'FAIL'}  {entry['name']:<24} "
                      f"exit={entry['exit']:<3} kind={entry['kind'] or '-'}")
        print(f"fixtures ok: {fixture_report['ok']}  (evidence: {ARGS.evidence / 'fixtures.json'})")
        return 0 if fixture_report["ok"] else 2
    if not fixture_report["ok"]:
        print("fixture validation failed; see evidence/fixtures.json", file=sys.stderr)
        return 2

    module, import_error = load_binding()
    ctx.ox = module
    if module is None:
        gaps = [f"installed binding import failed: {import_error}"] + oracle_gaps()
        if not readiness["complete"]:
            gaps.append("checkout extension has no implementation for: "
                        + ", ".join(readiness["missing_modules"]))
        STORE.write("preflight.json", {"gaps": gaps, "count": len(gaps), "readiness": readiness})
        if ARGS.preflight_only:
            print(json.dumps({"gaps": gaps, "readiness": readiness}, indent=2))
            return 2
        print("binding preflight gaps (suite is not runnable):\n  " + "\n  ".join(gaps), file=sys.stderr)
        return 2

    environment = environment_report(module)
    environment["wheel_problem"] = installed_wheel_problem(module)
    environment["readiness"] = readiness
    STORE.write("environment.json", environment)

    if ARGS.stage is not None and set(ARGS.stage) == {"direct-report"}:
        gaps = direct_report_preflight(module, ctx.oracle)
    else:
        gaps = oracle_gaps() + preflight(module)
    if not environment["engine_version_matches_cli"]:
        gaps.insert(0, f"engine version mismatch: wheel {module.engine_version!r} "
                       f"vs CLI {ctx.oracle.version!r} (the CLI must be built from this tree)")
    if environment["wheel_problem"]:
        gaps.insert(0, f"installed-wheel contract: {environment['wheel_problem']}")
    STORE.write("preflight.json", {"gaps": gaps, "count": len(gaps), "readiness": readiness})
    if ARGS.preflight_only:
        print(json.dumps({"gaps": gaps, "environment": environment}, indent=2))
        return 2 if gaps else 0
    if gaps:
        print("binding preflight gaps (suite is not runnable):\n  " + "\n  ".join(gaps), file=sys.stderr)
        return 2

    STORE.write("corpus-manifest.json", manifest())
    selected = [case for case in (CatalogParity, ReadParity, ValueParity, TraceListParity, DiffParity,
                                  FlagSemantics, WriteParity, DirectReportParity, ExrParity, ErrorParity,
                                  LifecycleParity, SyntheticParity)
                if any(stage_selected(s) for s in STAGE_OWNERS.get(case.__name__, ()))]
    loader = unittest.TestLoader()
    suite = unittest.TestSuite(loader.loadTestsFromTestCase(case) for case in selected)
    started = time.time()
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    finished = time.time()

    summary = {
        **manifest(),
        "started_at": started,
        "finished_at": finished,
        "seconds": finished - started,
        "destination_name_collisions": ctx.renamed,
        "diagnostic_references": REFS.summary(),
        "evidence_name_map": NAME_MAP,
        "environment": environment,
        "coverage": LEDGER.summary(),
        "gaps": gaps,
        "failures": ctx.failures,
        "tests": {"run": result.testsRun, "failures": len(result.failures), "errors": len(result.errors),
                  "skipped": len(result.skipped)},
        "evidence": {"calls": STORE.call_count, "unique_blobs": STORE.blob_count, "blob_bytes": STORE.bytes_out},
    }
    STORE.write("failures.json", {"failures": ctx.failures, "gaps": gaps})
    STORE.write("diagnostic-refs.json",
                {"rule": "CLI null or exactly {run_id,failure_id} with nonempty identifiers; "
                         "native null today (L20/DESIGN-DIAGNOSTICS-AMENDMENT.md)",
                 **REFS.summary(), "occurrences": REFS.rows})
    if NAME_MAP:
        STORE.write("name-map.json", NAME_MAP)
    STORE.write("run.json", summary)
    checksums = []
    for path in sorted(ARGS.evidence.rglob("*")):
        if path.is_file() and path.name != "SHA256SUMS":
            checksums.append(f"{sha256_file(path)}  {path.relative_to(ARGS.evidence)}")
    (ARGS.evidence / "SHA256SUMS").write_text("\n".join(checksums) + "\n")
    print(json.dumps({"tests": summary["tests"], "coverage": LEDGER.counts, "gaps": len(gaps),
                      "failures": len(ctx.failures), "evidence": summary["evidence"]}, indent=2))
    return 0 if result.wasSuccessful() else 1


STAGE_OWNERS = {
    "CatalogParity": ("catalog",),
    "ReadParity": ("read",),
    "ValueParity": ("values",),
    "TraceListParity": ("trace",),
    "DiffParity": ("diff",),
    "FlagSemantics": ("read", "values"),
    "WriteParity": ("write",),
    "DirectReportParity": ("direct-report",),
    "ExrParity": ("exr",),
    "ErrorParity": ("errors",),
    "LifecycleParity": ("lifecycle",),
    "SyntheticParity": ("synthetic",),
}

DIRECT_REPORT_STAGE_REQUIRED = (
    "report",
    "report_preview",
    "report_create",
    "report_discard",
    "pointer",
    "trace",
    "ERROR_TYPES",
)


def direct_report_preflight(module, oracle) -> list[str]:
    gaps: list[str] = []
    if direct_report_api(module) is None:
        gaps.append("missing direct bug-report API: " + DIRECT_REPORT_NEEDED)
    for name in ("report", "report_create", "report_discard", "pointer", "trace"):
        if not hasattr(module, name):
            gaps.append(f"missing public name for direct-report stage: oxblend.{name}")
    if not hasattr(module, "ERROR_TYPES"):
        gaps.append("missing ERROR_TYPES for direct-report stage")
    for cmd in ("report", "pointer", "trace"):
        if not oracle.supports(cmd):
            gaps.append(f"CLI oracle lacks required command for direct-report stage: {cmd}")
    return gaps

if __name__ == "__main__":
    try:
        raise SystemExit(main())
    finally:
        pass
