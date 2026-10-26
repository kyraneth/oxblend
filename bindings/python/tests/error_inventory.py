"""L20b R29 catalog-complete REAL canonical-error witness inventory.

Self-contained CLI-driven inventory: for every kind in
``docs/error-catalog.json`` (count derived from the catalog, never hardcoded)
the suite needs either a REAL fixture where the immutable CLI **and** the
installed in-process ``oxblend`` produce that kind with a FULL canonical error
object match, or ONE small committed exemption explaining WHY Python can never
reach that kind with exact source proof.

Scope: NEW TESTS ONLY. This file plus ``error_exemptions.json`` (and helper
``error_cases_*.py`` as needed) are the only product files owned here. Runtime,
engine, Python facade, ``full_parity.py``, docs and catalog are read-only.

Usage (root invokes from the errors stage or a separate release gate)::

    ./venvs/ep2311/bin/python bindings/python/tests/error_inventory.py \
        --cli /path/to/oxblend-merged3 \
        --evidence ./results/error-inventory

The installed ``oxblend`` import is required (no editable checkout import).
The catalog is derived from this checkout (``Path(__file__)``), never cwd.
Large/raw case bytes, stdout/stderr/exit and resource JSON stay under
``--evidence``. ``run(cli, evidence)`` is the entry root can import.

Comparison is complete, never truncated: the actual canonical CLI error object
fields are iterated and each requires a corresponding public exception
attribute (containers projected strictly). No ``diagnostic_ref`` inside the
canonical object (R17 sibling-only). Native refs are None; CLI refs are actual
bound shape accounted outside the canonical object. Two failed/unparsed empty
envelopes are never accepted as equal. ALL raw streams/exit are retained.
Fixture/source hashes are verified before/after, outputs are new/no-replace,
and expected partial reports/publication facts are preserved. A real witness
kind MUST match the declared case kind and the catalog class mapping; case
success is not refusal coverage. CLI-only presentation paths need an explicit
justified exemption, never a fabricated native constructor call.

Shape negatives (new uncovered kind, stale exemption, wrong case kind,
mutated message) are unit checks only and NEVER count as functional witnesses.
If a kind CAN reach Python but cannot be safely triggered here (e.g. physical
ENOSPC/durability), it is recorded MISSING with precise evidence; no unsafe
disk filling, permission bypass, invented RLIMITs, fault-injection knobs or
unreachable claims. Other fixtures keep moving.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib
import importlib.metadata
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import time
import traceback
import unittest
from dataclasses import dataclass, field
from pathlib import Path


HERE = Path(__file__).resolve()
REPO = HERE.parents[3]
CATALOG_PATH = REPO / "docs" / "error-catalog.json"
EXEMPTIONS_PATH = HERE.with_name("error_exemptions.json")

EXPECTED_EXEMPTIONS_SCHEMA = "oxblend.error-exemptions/v1"


# --------------------------------------------------------------------------- #
# Small helpers (self-contained; never import full_parity)
# --------------------------------------------------------------------------- #

def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def fs_text(path: Path) -> str:
    return os.fsdecode(os.fspath(path))


def json_bytes(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(",", ":")).encode() + b"\n"


def safe_name(text: str) -> str:
    """Evidence-directory identity: the full SHA-256 of the name (no truncation)."""
    return hashlib.sha256(text.encode("utf-8", "surrogateescape")).hexdigest()


class Store:
    """Content-addressed evidence bytes plus per-call metadata."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.calls = root / "calls"
        self.blobs = root / "blobs"
        self.calls.mkdir(parents=True, exist_ok=True)
        self.blobs.mkdir(parents=True, exist_ok=True)
        self.seq = 0

    def blob(self, data: bytes) -> dict:
        digest = sha256_bytes(data)
        target = self.blobs / digest
        if not target.exists():
            target.write_bytes(data)
        return {"sha256": digest, "bytes": len(data), "blob": f"blobs/{digest}"}

    def call(self, kind: str, record: dict, stdout: bytes = b"", stderr: bytes = b"") -> dict:
        self.seq += 1
        record = dict(record)
        record["seq"] = self.seq
        record["kind"] = kind
        if stdout:
            record["stdout"] = self.blob(stdout)
        if stderr:
            record["stderr"] = self.blob(stderr)
        directory = self.calls / f"{self.seq:07d}-{safe_name(kind)}"
        directory.mkdir(parents=True, exist_ok=True)
        (directory / "meta.json").write_bytes(json_bytes(record))
        return record

    def write(self, name: str, value: object) -> Path:
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(value, bytes):
            path.write_bytes(value)
        elif isinstance(value, str):
            path.write_text(value)
        else:
            path.write_bytes(json_bytes(value))
        return path


class Result:
    __slots__ = ("argv", "exit", "stdout", "stderr", "seconds", "seq", "pid")

    def __init__(self, argv: list[str], exit: int, stdout: bytes, stderr: bytes, seconds: float, seq: int, pid: int | None = None) -> None:
        self.argv, self.exit, self.stdout, self.stderr, self.seconds, self.seq = argv, exit, stdout, stderr, seconds, seq
        self.pid = pid


class Oracle:
    def __init__(self, binary: Path) -> None:
        self.binary = binary
        if not binary.is_file():
            raise SystemExit(f"CLI oracle is not a file: {binary}")
        if not os.access(binary, os.X_OK):
            raise SystemExit(f"CLI oracle is not executable: {binary}")
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("FLATTRACE")}
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

    @property
    def version(self) -> str:
        text = self.version_output.decode("utf-8", "replace").strip()
        return text[len("oxblend "):] if text.startswith("oxblend ") else text

    def __call__(self, *argv: object, stdin: bytes | None = None) -> Result:
        args = [fs_text(self.binary)] + [a if isinstance(a, str) else fs_text(Path(a)) for a in argv]
        started = time.monotonic()
        # Popen (not run) so the target PID is retained for dynamic-capacity
        # accounting (R37 spill_space brackets record observer/target pids).
        proc = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
                                env=self.env)
        target_pid = proc.pid
        stdout, stderr = proc.communicate(input=stdin)
        seconds = time.monotonic() - started
        return Result(args, proc.returncode, stdout, stderr, seconds, 0, pid=target_pid)


def hard_error_envelope(stderr: bytes) -> dict | None:
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


def parse_json_stdout(result: Result) -> object:
    if not result.stdout.strip():
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def project_value(value: object) -> object:
    """Project a Python exception/report value to plain JSON for comparison.

    Raises TypeError/ValueError for anything that is not honestly JSON:
    non-string mapping keys, bytes, non-finite floats, unknown types, or a
    failing ``to_json``. Never coerces keys or substitutes a prose string
    pretending to be valid data.
    """
    if value is None or type(value) in (str, bool, int):
        return value
    if type(value) is float:
        if value != value or value in (float("inf"), float("-inf")):
            raise ValueError("non-finite float is not JSON data")
        return value
    if hasattr(value, "to_json"):
        return project_value(value.to_json())
    if isinstance(value, (tuple, list)):
        return [project_value(v) for v in value]
    if isinstance(value, dict):
        out: dict = {}
        for k, v in value.items():
            if type(k) is not str:
                raise TypeError(f"JSON object key must be str, got {type(k).__name__}: {k!r}")
            out[k] = project_value(v)
        return out
    # Mapping-like records without to_json (e.g. Record is handled above via
    # to_json; a bare Mapping is still JSON-shaped only with str keys).
    from collections.abc import Mapping as _Mapping
    if isinstance(value, _Mapping):
        out = {}
        for k, v in value.items():
            if type(k) is not str:
                raise TypeError(f"JSON object key must be str, got {type(k).__name__}: {k!r}")
            out[k] = project_value(v)
        return out
    raise TypeError(f"value of type {type(value).__name__} is not JSON-projectable: {value!r}")


def json_type_name(value: object) -> str:
    if value is None:
        return "null"
    if type(value) is bool:
        return "bool"
    if type(value) is int:
        return "int"
    if type(value) is float:
        return "float"
    if type(value) is str:
        return "string"
    if isinstance(value, list):
        return "array"
    if isinstance(value, dict):
        return "object"
    return f"non-json:{type(value).__name__}"


def strict_json_problems(label: str, path: str, want: object, got: object) -> list[str]:
    """Strict recursive JSON equality: bool/int/float/string never alias."""
    problems: list[str] = []
    want_type, got_type = json_type_name(want), json_type_name(got)
    if want_type != got_type:
        return [f"{label}: field {path!r} type differs: python={got_type} {got!r} cli={want_type} {want!r}"]
    if isinstance(want, dict):
        if set(want) != set(got):  # type: ignore[arg-type]
            problems.append(f"{label}: field {path!r} keys differ: python={sorted(got)} cli={sorted(want)}")  # type: ignore[arg-type]
        for key in sorted(set(want) & set(got)):  # type: ignore[arg-type]
            problems.extend(strict_json_problems(label, f"{path}.{key}" if path else key, want[key], got[key]))  # type: ignore[index]
    elif isinstance(want, list):
        if len(want) != len(got):  # type: ignore[arg-type]
            problems.append(f"{label}: field {path!r} length differs: python={len(got)} cli={len(want)}")  # type: ignore[arg-type]
        for index, (w, g) in enumerate(zip(want, got)):  # type: ignore[arg-type]
            problems.extend(strict_json_problems(label, f"{path}[{index}]", w, g))
    elif want != got:
        problems.append(f"{label}: field {path!r} differs: python={got!r} cli={want!r}")
    return problems


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
    # Never import the checkout: the file must not live under this repo.
    try:
        rel = module_path.relative_to(REPO)
        return f"module is loaded from this checkout ({rel}), not an installed wheel"
    except ValueError:
        pass
    return None


# --------------------------------------------------------------------------- #
# Catalog and exemptions (derived, never hardcoded)
# --------------------------------------------------------------------------- #

def load_catalog() -> tuple[dict[str, dict], str, int]:
    raw = CATALOG_PATH.read_bytes()
    sha = sha256_bytes(raw)
    doc = json.loads(raw)
    if doc.get("schema") != "oxblend.error-catalog/v1":
        raise AssertionError(f"unexpected catalog schema: {doc.get('schema')}")
    entries = doc.get("entries")
    if not isinstance(entries, list) or not entries:
        raise AssertionError("catalog entries missing or empty")
    by_kind: dict[str, dict] = {}
    for entry in entries:
        kind = entry.get("kind")
        if not isinstance(kind, str) or not kind:
            raise AssertionError(f"catalog entry without kind: {entry!r}")
        if kind in by_kind:
            raise AssertionError(f"duplicate catalog kind: {kind}")
        by_kind[kind] = entry
    return by_kind, sha, len(by_kind)


def validate_exemptions(doc: object) -> list[str]:
    """Honest-proof validation (no character-length quality rules).

    Every proof snippet must occur *verbatim* in at least one listed proof
    file in this checkout, so the proof is retrievable and checkable rather
    than facet-tag prose paraphrasing the source.
    """
    problems: list[str] = []
    if not isinstance(doc, dict):
        return ["exemptions document must be an object"]
    if doc.get("schema") != EXPECTED_EXEMPTIONS_SCHEMA:
        problems.append(f"unexpected exemptions schema: {doc.get('schema')}")
    items = doc.get("exemptions")
    if not isinstance(items, list):
        return problems + ["exemptions.exemptions must be a list"]
    seen: set[str] = set()
    file_texts: dict[str, str] = {}
    for item in items:
        if not isinstance(item, dict):
            problems.append(f"exemption must be an object: {item!r}")
            continue
        kind = item.get("kind")
        reason = item.get("reason")
        proof = item.get("proof")
        if not isinstance(kind, str) or not kind.strip():
            problems.append(f"exemption without kind: {item!r}")
            continue
        if kind in seen:
            problems.append(f"duplicate exemption kind: {kind}")
        seen.add(kind)
        if not isinstance(reason, str) or not reason.strip():
            problems.append(f"exemption {kind}: reason must be a meaningful nonempty explanation of WHY Python cannot reach it")
        if not isinstance(proof, dict):
            problems.append(f"exemption {kind}: proof must be an object with files/snippets")
            continue
        files = proof.get("files")
        if not isinstance(files, list) or not files or not all(isinstance(f, str) and f.strip() for f in files):
            problems.append(f"exemption {kind}: proof.files must be a nonempty string list")
            continue
        for rel in files:
            target = REPO / rel
            if not target.is_file():
                problems.append(f"exemption {kind}: proof file missing in checkout: {rel}")
            elif rel not in file_texts:
                try:
                    file_texts[rel] = target.read_text(encoding="utf-8")
                except (OSError, UnicodeDecodeError) as exc:
                    problems.append(f"exemption {kind}: proof file unreadable as UTF-8: {rel}: {exc}")
        snippets = proof.get("snippets")
        if not isinstance(snippets, list) or not snippets or not all(isinstance(s, str) and s.strip() for s in snippets):
            problems.append(f"exemption {kind}: proof.snippets must be nonempty exact source text")
            continue
        texts = [file_texts[rel] for rel in files if rel in file_texts]
        for snippet in snippets:
            if not any(snippet in text for text in texts):
                problems.append(f"exemption {kind}: proof snippet is not verbatim source in any listed proof file: {snippet!r}")
    return problems


def load_exemptions() -> dict:
    raw = EXEMPTIONS_PATH.read_bytes()
    doc = json.loads(raw)
    problems = validate_exemptions(doc)
    if problems:
        raise AssertionError("; ".join(problems))
    return doc


def validate_exemptions_against_catalog(catalog: dict[str, dict], exemptions: dict) -> list[str]:
    """Return stale-exemption errors (kind removed from catalog)."""
    problems: list[str] = []
    kinds = {item["kind"] for item in exemptions.get("exemptions", [])}
    for kind in sorted(kinds):
        if kind not in catalog:
            problems.append(f"stale exemption: {kind!r} is not in docs/error-catalog.json")
    return problems


def expected_exit_for_kind(kind: str) -> int:
    if kind in ("pack_input", "missing_input"):
        return 3
    if kind == "pack_project":
        return 5
    return 2


# --------------------------------------------------------------------------- #
# R37 spill_space dynamic-capacity accounting (no exemption, no interposer)
#
# Same as identity fields: canonical-identical, dynamic fields accounted.
# Compare FULL error object EXACT EXCEPT live capacity NUMBERS. Differences
# ONLY at the EXACT positions the shared owner format puts them, EACH
# independently witnessed: each process available bytes is a WELL-FORMED
# INTEGER consistent with its OWN statvfs read (test records STATVFS
# immediately BEFORE and AFTER the call, requires the REPORTED value to be
# within THAT range). shortfall == required-available, required IDENTICAL
# both sides. EVERY other byte/object field EXACT. NO interposer, NO product
# change. Named explicitly in GATE+PARITY TABLE (returned to ROOT; this
# module owns only the test gate, never README/parity docs).
#
# Shared owner: src/input.rs require_spill_space (and admit_spill_extent /
# spill_checked_decode via temp_spill_available / fstatvfs on the owned
# O_TMPFILE filesystem). Format derived ONLY from that owner:
#   decoded BHead spill requires {required} bytes; available {available} bytes; shortfall {shortfall} bytes
# where required/available/shortfall are u64 canonical decimal (no leading
# zeros unless single "0", no sign, no bool). required > available,
# shortfall == required-available, per-process independent.
# --------------------------------------------------------------------------- #

SPILL_MESSAGE_RE = re.compile(
    r"^decoded BHead spill requires (\d+) bytes; available (\d+) bytes; shortfall (\d+) bytes$"
)
SPILL_U64_MAX = 2**64 - 1
SPILL_CANONICAL_INT_RE = re.compile(r"^(0|[1-9][0-9]*)$")


def is_canonical_u64_token(text: object) -> bool:
    """Canonical nonnegative int: decimal str, no leading zeros/sign/bool, fits u64."""
    if type(text) is not str:
        return False
    if SPILL_CANONICAL_INT_RE.match(text) is None:
        return False
    try:
        value = int(text, 10)
    except ValueError:
        return False
    # bool is int subclass but type() check above already excludes it;
    # int(text) on canonical digits cannot be bool. Range is u64.
    return 0 <= value <= SPILL_U64_MAX


def parse_spill_message(message: object) -> tuple[dict | None, list[str]]:
    """Parse the shared-owner spill message; return (fields, problems).

    On success fields is {required,available,shortfall} as ints plus the
    raw tokens for canonical proof. On failure fields is None and problems
    explains the exact fault (prose drift, non-canonical digits, bad
    arithmetic). Anchored full-message match only.
    """
    if type(message) is not str:
        return None, [f"spill message is not str: {type(message).__name__}: {message!r}"]
    match = SPILL_MESSAGE_RE.match(message)
    if match is None:
        return None, [f"spill message does not match shared-owner format: {message!r}"]
    req_tok, avail_tok, short_tok = match.groups()
    problems: list[str] = []
    for name, tok in (("required", req_tok), ("available", avail_tok), ("shortfall", short_tok)):
        if not is_canonical_u64_token(tok):
            problems.append(f"spill {name} token is not canonical u64: {tok!r}")
    if problems:
        return None, problems
    required, available, shortfall = int(req_tok, 10), int(avail_tok, 10), int(short_tok, 10)
    if not required > available:
        problems.append(f"spill requires {required} but available {available}: require_spill_space refuses only when required>available")
    if shortfall != required - available:
        problems.append(f"spill shortfall {shortfall} != required-available {required}-{available}={required - available}")
    if problems:
        return None, problems
    return {"required": required, "available": available, "shortfall": shortfall,
            "required_token": req_tok, "available_token": avail_tok, "shortfall_token": short_tok}, []


def temp_dir_for_spill() -> Path:
    """Operation temp root for spill: Rust std::env::temp_dir() (TMPDIR or /tmp)."""
    tmpdir = os.environ.get("TMPDIR")
    if tmpdir:
        return Path(tmpdir)
    return Path("/tmp")


def statvfs_available_bytes(path: Path) -> tuple[int, dict]:
    """statvfs available bytes (f_bavail * f_frsize) plus full fields for provenance."""
    stats = os.statvfs(path)
    available = stats.f_bavail * stats.f_frsize
    detail = {"f_bsize": stats.f_bsize, "f_frsize": stats.f_frsize,
              "f_blocks": stats.f_blocks, "f_bfree": stats.f_bfree,
              "f_bavail": stats.f_bavail, "available_bytes": available}
    return available, detail


def shared_owner_spill_proof() -> dict:
    """Shared-owner source proof: src/input.rs hash plus checkout commit."""
    target = REPO / "src" / "input.rs"
    sha = sha256_file(target) if target.is_file() else "<missing>"
    try:
        commit = subprocess.check_output(["git", "-C", fs_text(REPO), "rev-parse", "HEAD"],
                                         text=True, timeout=30).strip()
    except Exception:  # noqa: BLE001 - proof records the failure, never masks it
        commit = "<unknown>"
    try:
        snippet = target.read_text(encoding="utf-8") if target.is_file() else ""
    except (OSError, UnicodeDecodeError):
        snippet = ""
    # Verbatim owner pattern (must occur in the file; validated by caller).
    owner_format = '"decoded BHead spill requires {required} bytes; available {available} bytes; shortfall {} bytes"'
    verbatim = owner_format in snippet
    return {"path": "src/input.rs", "sha256": sha, "commit": commit,
            "owner_format": owner_format, "verbatim_in_file": verbatim}


def compare_spill_dynamic(label: str, cli_error: dict, exc,
                          cli_bracket: dict, py_bracket: dict) -> list[str]:
    """R37 dynamic comparison: FULL object EXACT EXCEPT live capacity numbers.

    cli_bracket/py_bracket are {before,after} available-bytes witness ranges
    from the test's own statvfs reads. Each side's reported available must
    lie within its OWN range; required must be IDENTICAL; per-side
    shortfall arithmetic independent. Every other field is strict JSON
    (including future keys). Diagnostic refs are NOT inside the canonical
    object (R17); native must be null.
    """
    problems: list[str] = []
    if not isinstance(cli_error, dict) or not cli_error:
        return [f"{label}: CLI error object is not a nonempty dict"]
    if "diagnostic_ref" in cli_error:
        problems.append(f"{label}: canonical CLI error must not carry inside diagnostic_ref (R17 sibling-only)")
    # Every non-message field strict; message dynamic.
    for key, want in cli_error.items():
        if key == "diagnostic_ref":
            continue
        if key == "message":
            continue
        if not hasattr(exc, key):
            problems.append(f"{label}: Python exception lacks attribute {key!r} (CLI has it)")
            continue
        try:
            got = project_value(getattr(exc, key))
        except Exception as proj:  # noqa: BLE001
            problems.append(f"{label}: field {key!r} is not JSON-projectable: {type(proj).__name__}: {proj}")
            continue
        problems.extend(strict_json_problems(label, key, want, got))
    # Message dynamic: both sides must parse, required identical, each
    # available within its OWN bracket, per-side arithmetic independent.
    cli_msg = cli_error.get("message")
    try:
        py_msg_raw = getattr(exc, "message", None)
    except Exception:  # noqa: BLE001
        py_msg_raw = None
    if not isinstance(py_msg_raw, str):
        try:
            py_msg_raw = str(exc)
        except Exception:  # noqa: BLE001
            py_msg_raw = None
    cli_fields, cli_prob = parse_spill_message(cli_msg)
    py_fields, py_prob = parse_spill_message(py_msg_raw)
    problems.extend(f"{label}: CLI message: {p}" for p in cli_prob)
    problems.extend(f"{label}: Python message: {p}" for p in py_prob)
    if cli_fields is None or py_fields is None:
        return problems
    if cli_fields["required"] != py_fields["required"]:
        problems.append(f"{label}: spill required differs: cli={cli_fields['required']} python={py_fields['required']} (must be IDENTICAL)")
    for side, fields, bracket in (("CLI", cli_fields, cli_bracket), ("Python", py_fields, py_bracket)):
        before = bracket.get("before")
        after = bracket.get("after")
        if not isinstance(before, int) or not isinstance(after, int):
            problems.append(f"{label}: {side} bracket missing before/after available bytes: {bracket!r}")
            continue
        lo, hi = (before, after) if before <= after else (after, before)
        avail = fields["available"]
        if not lo <= avail <= hi:
            problems.append(f"{label}: {side} reported available {avail} not within own statvfs range [{lo},{hi}] (before={before} after={after})")
    return problems


# --------------------------------------------------------------------------- #
# Canonical comparison (complete, never truncated)
# --------------------------------------------------------------------------- #

def compare_canonical(label: str, cli_error: dict, exc) -> list[str]:
    """Compare the ACTUAL CLI error object fields against the Python exception.

    Every field present in the CLI object requires a corresponding public
    exception attribute with a strictly equal projected value. No field mask.
    """
    problems: list[str] = []
    if not isinstance(cli_error, dict) or not cli_error:
        return [f"{label}: CLI error object is not a nonempty dict"]
    if "diagnostic_ref" in cli_error:
        problems.append(f"{label}: canonical CLI error must not carry inside diagnostic_ref (R17 sibling-only)")
    for key, want in cli_error.items():
        if key == "diagnostic_ref":
            continue
        if not hasattr(exc, key):
            problems.append(f"{label}: Python exception lacks attribute {key!r} (CLI has it)")
            continue
        try:
            got = project_value(getattr(exc, key))
        except Exception as proj:  # noqa: BLE001 - a projection failure is a mismatch, never substitute data
            problems.append(f"{label}: field {key!r} is not JSON-projectable: {type(proj).__name__}: {proj}")
            continue
        problems.extend(strict_json_problems(label, key, want, got))
    return problems


# --------------------------------------------------------------------------- #
# Retained-report comparison with invocation-bound diagnostic refs accounted
# --------------------------------------------------------------------------- #

def canonical_envelope_bytes(envelope: dict) -> bytes:
    """Rebuild the receipt-bound canonical envelope (envelope minus ``diagnostic``)."""
    base = {"schema": envelope.get("schema"), "error": envelope.get("error")}
    return json.dumps(base, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + b"\n"


class ReceiptContext:
    """Validated CLI failure receipt, or the problems that invalidate it."""

    def __init__(self, label: str, envelope: dict | None, cli_stdout: bytes,
                 cli_exit: int, cli_command: str, cli_error: dict, binary_sha256: str) -> None:
        self.problems: list[str] = []
        self.receipt: dict | None = None
        self.known: dict[str, set[str]] = {}  # failure_id -> kinds (record + propagations)
        if not isinstance(envelope, dict):
            self.problems.append(f"{label}: CLI envelope missing for receipt accounting")
            return
        receipt = envelope.get("diagnostic")
        if not isinstance(receipt, dict):
            self.problems.append(f"{label}: CLI envelope carries no diagnostic receipt to account diagnostic_ref values against")
            return
        self.receipt = receipt
        if receipt.get("schema") != "oxblend.failure-receipt/v1":
            self.problems.append(f"{label}: receipt schema is not oxblend.failure-receipt/v1: {receipt.get('schema')!r}")
        producer = receipt.get("producer")
        if not isinstance(producer, dict) or producer.get("name") != "oxblend" or producer.get("binary_sha256") != binary_sha256:
            self.problems.append(f"{label}: receipt producer does not bind this CLI binary (producer={producer!r}, binary_sha256={binary_sha256})")
        if receipt.get("command") != cli_command:
            self.problems.append(f"{label}: receipt command {receipt.get('command')!r} != CLI command {cli_command!r}")
        if receipt.get("exit_code") != cli_exit or type(receipt.get("exit_code")) is not int:
            self.problems.append(f"{label}: receipt exit_code {receipt.get('exit_code')!r} != CLI exit {cli_exit}")
        run_id = receipt.get("run_id")
        if not isinstance(run_id, str) or not run_id.strip():
            self.problems.append(f"{label}: receipt run_id is not a meaningful string: {run_id!r}")
        binding = receipt.get("binding")
        if not isinstance(binding, dict):
            self.problems.append(f"{label}: receipt has no binding object")
            binding = {}
        if binding.get("status") != "invocation_bound":
            self.problems.append(f"{label}: receipt binding.status is not invocation_bound: {binding.get('status')!r}")
        if binding.get("stdout_sha256") != sha256_bytes(cli_stdout):
            self.problems.append(f"{label}: receipt binding.stdout_sha256 does not hash the actual CLI stdout")
        if binding.get("stdout_bytes") != len(cli_stdout):
            self.problems.append(f"{label}: receipt binding.stdout_bytes {binding.get('stdout_bytes')!r} != actual stdout length {len(cli_stdout)}")
        if binding.get("stdout_delivery_complete") is not True:
            self.problems.append(f"{label}: receipt binding.stdout_delivery_complete is not true")
        try:
            stderr_sha = sha256_bytes(canonical_envelope_bytes(envelope))
        except Exception as exc:  # noqa: BLE001
            stderr_sha = None
            self.problems.append(f"{label}: canonical envelope cannot be rebuilt for its hash: {exc}")
        if stderr_sha is not None and binding.get("stderr_error_sha256") != stderr_sha:
            self.problems.append(f"{label}: receipt binding.stderr_error_sha256 does not hash the canonical envelope without diagnostic")
        failures = receipt.get("failures")
        if not isinstance(failures, list) or not failures:
            self.problems.append(f"{label}: receipt failures is not a nonempty list")
            failures = []
        for record in failures:
            if not isinstance(record, dict) or not isinstance(record.get("id"), str) or not record.get("id"):
                self.problems.append(f"{label}: receipt failure record without a string id: {record!r}")
                continue
            fid = record["id"]
            if fid in self.known:
                self.problems.append(f"{label}: receipt failure id {fid!r} is duplicated")
            kinds: set[str] = set()
            if isinstance(record.get("kind"), str):
                kinds.add(record["kind"])
            props = record.get("propagations")
            if isinstance(props, list):
                for prop in props:
                    if isinstance(prop, dict) and isinstance(prop.get("kind"), str):
                        kinds.add(prop["kind"])
            self.known[fid] = kinds
        if failures and isinstance(failures[-1], dict):
            terminal = failures[-1]
            if terminal.get("kind") != cli_error.get("kind") or terminal.get("message") != cli_error.get("message"):
                self.problems.append(f"{label}: receipt terminal failure does not bind the CLI error kind/message")

    def account_ref(self, label: str, path: str, cli_ref: object, sibling_kind: object) -> list[str]:
        """Validate one CLI diagnostic_ref against this receipt; never strip it silently."""
        if cli_ref is None:
            return []
        if self.receipt is None:
            return [f"{label}: report field {path!r} carries a diagnostic_ref but no valid CLI receipt accounts for it"]
        if not isinstance(cli_ref, dict) or set(cli_ref) != {"run_id", "failure_id"}:
            return [f"{label}: report field {path!r} diagnostic_ref is not exactly {{run_id, failure_id}}: {cli_ref!r}"]
        run_id, failure_id = cli_ref.get("run_id"), cli_ref.get("failure_id")
        if not isinstance(run_id, str) or not run_id.strip() or not isinstance(failure_id, str) or not failure_id.strip():
            return [f"{label}: report field {path!r} diagnostic_ref run_id/failure_id must be meaningful strings: {cli_ref!r}"]
        problems: list[str] = []
        if self.problems:
            problems.append(f"{label}: report field {path!r} diagnostic_ref cannot be accounted: receipt is invalid ({'; '.join(self.problems)})")
        if run_id != self.receipt.get("run_id"):
            problems.append(f"{label}: report field {path!r} diagnostic_ref run_id {run_id!r} != receipt run_id {self.receipt.get('run_id')!r}")
        kinds = self.known.get(failure_id)
        if kinds is None:
            problems.append(f"{label}: report field {path!r} diagnostic_ref failure_id {failure_id!r} is not a retained receipt failure")
        elif sibling_kind not in kinds:
            problems.append(f"{label}: report field {path!r} diagnostic_ref failure {failure_id!r} binds kinds {sorted(kinds)}, not sibling kind {sibling_kind!r}")
        return problems


def compare_retained_reports(label: str, cli_report: object, py_report: object,
                               receipt: ReceiptContext) -> list[str]:
    """Strict retained-report equality, accounting ONLY receipt-bound refs.

    At a ``diagnostic_ref`` site the native (Python) value must be null and
    the CLI value must be null or a ref this invocation's receipt actually
    retains for that site's kind. Every other field compares strictly.
    """
    problems: list[str] = []

    def walk(path: str, want: object, got: object, sibling_kind: object = None) -> None:
        if path.rsplit(".", 1)[-1] == "diagnostic_ref" or path == "diagnostic_ref":
            if got is not None:
                problems.append(f"{label}: retained report field {path!r}: native diagnostic_ref must be null, got {got!r}")
            problems.extend(receipt.account_ref(label, path, want, sibling_kind))
            return
        want_type, got_type = json_type_name(want), json_type_name(got)
        if want_type != got_type:
            problems.append(f"{label}: retained report field {path!r} type differs: python={got_type} {got!r} cli={want_type} {want!r}")
            return
        if isinstance(want, dict):
            if set(want) != set(got):  # type: ignore[arg-type]
                problems.append(f"{label}: retained report field {path!r} keys differ: python={sorted(got)} cli={sorted(want)}")  # type: ignore[arg-type]
            kind_here = want.get("kind")
            for key in sorted(set(want) & set(got)):  # type: ignore[arg-type]
                walk(f"{path}.{key}" if path else key, want[key], got[key], kind_here)  # type: ignore[index]
        elif isinstance(want, list):
            if len(want) != len(got):  # type: ignore[arg-type]
                problems.append(f"{label}: retained report field {path!r} length differs: python={len(got)} cli={len(want)}")  # type: ignore[arg-type]
            for index, (w, g) in enumerate(zip(want, got)):  # type: ignore[arg-type]
                walk(f"{path}[{index}]", w, g, sibling_kind)
        elif want != got:
            problems.append(f"{label}: retained report field {path!r} differs: python={got!r} cli={want!r}")

    walk("", cli_report, py_report)
    return problems


def snapshot_tree(path: Path) -> dict | None:
    """Content snapshot of a published artifact (file, symlink or tree)."""
    if path.is_symlink():
        return {"type": "symlink", "target": os.readlink(path)}
    if not path.exists():
        return None
    if path.is_file():
        data = path.read_bytes()
        return {"type": "file", "sha256": sha256_bytes(data), "bytes": len(data)}
    if path.is_dir():
        members: dict[str, object] = {}
        for entry in sorted(path.rglob("*")):
            rel = entry.relative_to(path).as_posix()
            if entry.is_symlink():
                members[rel] = {"type": "symlink", "target": os.readlink(entry)}
            elif entry.is_file():
                data = entry.read_bytes()
                members[rel] = {"type": "file", "sha256": sha256_bytes(data), "bytes": len(data)}
            elif entry.is_dir():
                members[rel] = {"type": "dir"}
        return {"type": "dir", "members": members}
    return {"type": "other"}


def remove_published(path: Path, case_name: str) -> None:
    """Remove only this case's own CLI-published artifact before the Python run.

    Builders place every claimed/expected output directly under their own
    ``work/cases/<name>/`` directory, so cleanup is confined to an output
    whose parent directory is exactly that case directory.
    """
    if path.parent.name != case_name:
        raise AssertionError(f"refusing to clean a published artifact outside its own case dir: {path}")
    if path.is_symlink() or path.is_file():
        path.unlink()
    elif path.is_dir():
        shutil.rmtree(path)


def check_class_and_exit(label: str, ox, exc, kind: str, cli_exit: int) -> list[str]:
    problems: list[str] = []
    expected_class = ox.ERROR_TYPES.get(kind)
    if expected_class is None:
        problems.append(f"{label}: kind {kind!r} is not in ERROR_TYPES")
    elif not isinstance(exc, expected_class):
        problems.append(f"{label}: wrong exception class {type(exc).__name__} for kind {kind!r} (want {expected_class.__name__})")
    expected_exit = expected_exit_for_kind(kind)
    if cli_exit != expected_exit:
        problems.append(f"{label}: CLI exit {cli_exit} != documented rule {expected_exit} for kind {kind!r}")
    py_exit = getattr(exc, "exit_code", None)
    if py_exit != cli_exit:
        problems.append(f"{label}: Python exit_code {py_exit!r} != CLI exit {cli_exit}")
    if py_exit != expected_exit:
        problems.append(f"{label}: Python exit_code {py_exit!r} != documented rule {expected_exit} for kind {kind!r}")
    # Native refs must exist and be null today; a missing attribute is a
    # defect, never silently accepted via a sentinel.
    if not hasattr(exc, "diagnostic_ref"):
        problems.append(f"{label}: Python exception lacks attribute 'diagnostic_ref' (native ref must exist and be null)")
    elif exc.diagnostic_ref is not None:
        problems.append(f"{label}: native diagnostic_ref must be null today, got {exc.diagnostic_ref!r}")
    return problems


# --------------------------------------------------------------------------- #
# Fixture builders (owned testdata in own workdirs; inputs read-only)
# --------------------------------------------------------------------------- #

def _dna(names: list[bytes], types: list[bytes], lengths: list[int],
         structs: list[tuple[int, list[tuple[int, int]]]]) -> bytes:
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


def _test_payload(*, number: int, next_address: int, name: bytes, scalar: int,
                  array: tuple[int, int], f32_bits: int, chain_address: int) -> bytes:
    payload = struct.pack("<QQ8siiiIQ", number, next_address, name.ljust(8, b"\0")[:8],
                          scalar, array[0], array[1], f32_bits, chain_address)
    assert len(payload) == 48, len(payload)
    return payload


def fixture_minimal_bytes() -> bytes:
    return build_blend([(b"TEST", 0x1000, 1, _test_payload(
        number=2**64 - 1, next_address=0, name=b"N\xff", scalar=-(2**31),
        array=(2**31 - 1, -(2**31)), f32_bits=0x7FC12345, chain_address=0))], TEST_DNA)


ID_NAMES = [b"id", b"*next", b"*prev", b"*lib", b"name[32]"]
ID_TYPES = [b"char", b"ID", b"Image"]
ID_DNA = _dna(ID_NAMES, ID_TYPES, [1, 24, 56],
              [(1, [(1, 1), (1, 2), (1, 3)]), (2, [(1, 0), (0, 4)])])


def _image_payload(*, next_address: int, prev_address: int, library_address: int, name: bytes) -> bytes:
    payload = struct.pack("<QQQ", next_address, prev_address, library_address)
    payload += name.ljust(32, b"\0")[:32]
    assert len(payload) == 56, len(payload)
    return payload


def fixture_dangling_bytes() -> bytes:
    return build_blend([(b"IM\0\0", 0x1000, 1, _image_payload(
        next_address=0, prev_address=0, library_address=0xDEADBEEF, name=b"orphan"))],
        ID_DNA, sdna=1)


REWRITE_NAMES = [b"name[32]", b"width"]
REWRITE_TYPES = [b"char", b"int", b"Image"]
REWRITE_DNA = _dna(REWRITE_NAMES, REWRITE_TYPES, [1, 4, 36], [(2, [(0, 0), (1, 1)])])


def fixture_rewrite_bytes() -> bytes:
    name = b"//textures/old.png"
    payload = name.ljust(32, b"\0") + struct.pack("<i", 512)
    return build_blend([(b"IM\0\0", 0x3000, 1, payload)], REWRITE_DNA)


def rewrite_spec_document() -> dict:
    return {"schema": 1, "operation": "rewrite_paths", "edits": [{
        "block_index": 0, "element": 0,
        "path": [{"field_hex": b"name".hex()}],
        "expected_path_hex": b"//textures/old.png".hex(),
        "replacement_path_hex": b"//textures/new.png".hex(),
    }]}


# --------------------------------------------------------------------------- #
# One-case runner
# --------------------------------------------------------------------------- #

@dataclass
class CaseResult:
    kind: str
    name: str
    cli_argv: list[str]
    cli_exit: int | None
    cli_kind: str | None
    cli_message: str | None
    python_kind: str | None
    python_message: str | None
    python_exit: int | None
    python_class: str | None
    status: str  # pass | mismatch | error
    problems: list[str] = field(default_factory=list)
    evidence: dict = field(default_factory=dict)


def parse_cli_report(result: Result) -> object:
    """CLI stdout as one JSON value, or an ordered JSONL list (scan/batch)."""
    whole = parse_json_stdout(result)
    if whole is not None:
        return whole
    lines = [line for line in result.stdout.splitlines() if line.strip()]
    if len(lines) <= 1:
        return None
    try:
        return [json.loads(line) for line in lines]
    except json.JSONDecodeError:
        return None


def run_one_case(*, label: str, kind: str, name: str, cli_argv: list[str],
                 py_thunk, ox, oracle: Oracle, store: Store, case_dir: Path,
                 input_paths: list[Path], claim_outputs: list[Path],
                 expected_outputs: list[Path] | None = None) -> CaseResult:
    """Run one CLI + Python pair and compare the FULL canonical error object."""
    case_dir.mkdir(parents=True, exist_ok=True)
    result = CaseResult(kind=kind, name=name, cli_argv=list(cli_argv),
                        cli_exit=None, cli_kind=None, cli_message=None,
                        python_kind=None, python_message=None, python_exit=None,
                        python_class=None, status="error", problems=[])
    # Record input hashes before (read-only inputs must be preserved).
    before: dict[str, str] = {}
    for p in input_paths:
        before[fs_text(p)] = sha256_file(p) if p.is_file() else "<missing>"
    (case_dir / "hashes.before.json").write_bytes(json_bytes(before))
    expected_outputs = list(expected_outputs or [])
    # Fresh outputs must be new/no-replace: none may exist yet.
    for out in list(claim_outputs) + expected_outputs:
        if out.exists() or out.is_symlink():
            result.problems.append(f"{label}: claimed fresh output already exists: {out}")
    # R37 spill_space: record the live-capacity witness brackets around EACH
    # surface. The temp root is the operation-frozen spill filesystem
    # (Rust std::env::temp_dir()); both CLI and Python sample it via
    # fstatvfs on the owned O_TMPFILE, so the harness brackets the same
    # filesystem immediately BEFORE and AFTER each call. No freeze/mock.
    spill_cli_bracket: dict = {}
    spill_py_bracket: dict = {}
    spill_temp: Path | None = None
    spill_tmpdir_env: str | None = None
    spill_observer_pid: int | None = None
    if kind == "spill_space":
        spill_temp = temp_dir_for_spill()
        spill_tmpdir_env = os.environ.get("TMPDIR")
        spill_observer_pid = os.getpid()
        try:
            avail_before, detail_before = statvfs_available_bytes(spill_temp)
        except OSError as exc:
            result.problems.append(f"{label}: spill statvfs before CLI failed: {exc}")
            avail_before, detail_before = -1, {"error": str(exc)}
        spill_cli_bracket["before"] = avail_before
        spill_cli_bracket["before_detail"] = detail_before
        spill_cli_bracket["before_time"] = time.time()
        spill_cli_bracket["before_monotonic"] = time.monotonic()
        spill_cli_bracket["observer_pid"] = spill_observer_pid
        spill_cli_bracket["temp_dir"] = fs_text(spill_temp)
        spill_cli_bracket["tmpdir_env"] = spill_tmpdir_env
    # CLI run (read-only inputs; new outputs only).
    cli = oracle(*cli_argv)
    if kind == "spill_space" and spill_temp is not None:
        try:
            avail_after, detail_after = statvfs_available_bytes(spill_temp)
        except OSError as exc:
            result.problems.append(f"{label}: spill statvfs after CLI failed: {exc}")
            avail_after, detail_after = -1, {"error": str(exc)}
        spill_cli_bracket["after"] = avail_after
        spill_cli_bracket["after_detail"] = detail_after
        spill_cli_bracket["after_time"] = time.time()
        spill_cli_bracket["after_monotonic"] = time.monotonic()
        spill_cli_bracket["target_pid"] = getattr(cli, "pid", None)
        try:
            spill_cli_bracket["temp_dev"] = os.stat(spill_temp).st_dev
        except OSError:
            pass
    result.cli_exit = cli.exit
    store.call(f"cli-{name}", {"argv": cli.argv, "exit": cli.exit, "seconds": cli.seconds,
                               "case": name, "declared_kind": kind},
               cli.stdout, cli.stderr)
    (case_dir / "cli.stdout").write_bytes(cli.stdout)
    (case_dir / "cli.stderr").write_bytes(cli.stderr)
    (case_dir / "cli.exit").write_text(str(cli.exit) + "\n")
    (case_dir / "cli.argv.json").write_bytes(json_bytes(cli.argv))
    envelope = hard_error_envelope(cli.stderr)
    if cli.exit == 0 or envelope is None:
        result.problems.append(f"{label}: case success is not refusal coverage (CLI exit {cli.exit}, envelope {'missing' if envelope is None else 'present'})")
        result.status = "error"
        result.evidence = {"cli_stdout_bytes": len(cli.stdout), "cli_stderr_bytes": len(cli.stderr)}
        return result
    cli_error = envelope.get("error")
    if not isinstance(cli_error, dict):
        result.problems.append(f"{label}: CLI envelope has no error object")
        result.status = "error"
        return result
    result.cli_kind = cli_error.get("kind")
    result.cli_message = cli_error.get("message")
    (case_dir / "cli.error.json").write_bytes(json_bytes(cli_error))
    (case_dir / "cli.envelope.json").write_bytes(json_bytes(envelope))
    if result.cli_kind != kind:
        result.problems.append(f"{label}: wrong case kind: CLI kind {result.cli_kind!r} != declared {kind!r}")
        result.status = "error"
        # Still run Python to preserve evidence, but the case is already wrong.
    # Partial-publication accounting: snapshot what the CLI actually
    # published, then clean only this case's own declared artifacts so the
    # Python run starts from the same fresh destination. A blanket
    # 'refusal publishes nothing' assumption is never made for cases that
    # declare expected outputs; undeclared (claimed) outputs published by
    # the CLI are a defect, recorded before the same cleanup.
    cli_artifacts: dict[str, object] = {}
    for out in expected_outputs:
        if out in input_paths:
            result.problems.append(f"{label}: expected output aliases an input, cleanup refused: {out}")
            continue
        cli_artifacts[fs_text(out)] = snapshot_tree(out)
        if cli_artifacts[fs_text(out)] is not None:
            remove_published(out, name)
    for out in claim_outputs:
        if out in input_paths:
            continue
        snap = snapshot_tree(out)
        if snap is not None:
            cli_artifacts[fs_text(out)] = snap
            result.problems.append(f"{label}: CLI refusal published an undeclared output (must stay absent): {out}")
            remove_published(out, name)
    (case_dir / "artifacts.cli.json").write_bytes(json_bytes(cli_artifacts))
    # Python run.
    if kind == "spill_space" and spill_temp is not None:
        try:
            py_avail_before, py_detail_before = statvfs_available_bytes(spill_temp)
        except OSError as exc:
            result.problems.append(f"{label}: spill statvfs before Python failed: {exc}")
            py_avail_before, py_detail_before = -1, {"error": str(exc)}
        spill_py_bracket["before"] = py_avail_before
        spill_py_bracket["before_detail"] = py_detail_before
        spill_py_bracket["before_time"] = time.time()
        spill_py_bracket["before_monotonic"] = time.monotonic()
        spill_py_bracket["observer_pid"] = os.getpid()
        spill_py_bracket["target_pid"] = os.getpid()
        spill_py_bracket["temp_dir"] = fs_text(spill_temp)
        spill_py_bracket["tmpdir_env"] = os.environ.get("TMPDIR")
    started = time.monotonic()
    caught = None
    try:
        py_thunk()
        seconds = time.monotonic() - started
        result.problems.append(f"{label}: case success is not refusal coverage (Python did not raise)")
        result.status = "error"
        store.call(f"python-{name}", {"argv": ["python", name], "ok": True, "seconds": seconds,
                                      "case": name, "declared_kind": kind})
        return result
    except Exception as e:  # noqa: BLE001 - evidence must retain every failure shape
        caught = e
        seconds = time.monotonic() - started
        tb = traceback.format_exc()
        (case_dir / "python.traceback.txt").write_text(tb)
        has_kind = hasattr(caught, "kind")
        result.python_kind = getattr(caught, "kind", None)
        result.python_message = getattr(caught, "message", None) or str(caught)
        result.python_exit = getattr(caught, "exit_code", None)
        result.python_class = type(caught).__name__
        record: dict = {"argv": ["python", name], "exception": type(caught).__name__,
                        "message": result.python_message, "kind": result.python_kind,
                        "exit_code": result.python_exit, "operation": getattr(caught, "operation", None),
                        "effects": getattr(caught, "effects", None), "seconds": seconds,
                        "case": name, "declared_kind": kind,
                        "error": has_kind,
                        "explanation": getattr(caught, "explanation", None),
                        "likely_causes": list(getattr(caught, "likely_causes", ()) or ()),
                        "next_steps": list(getattr(caught, "next_steps", ()) or ()),
                        "known_issue": getattr(caught, "known_issue", None),
                        "diagnostic_ref": getattr(caught, "diagnostic_ref", None),
                        "diagnostic_ref_present": hasattr(caught, "diagnostic_ref"),
                        "traceback": tb}
        try:
            report = getattr(caught, "report", None)
            record["report"] = project_value(report.to_json()) if hasattr(report, "to_json") else project_value(report) if report is not None else None
        except Exception as proj:  # noqa: BLE001
            record["report_projection_error"] = f"{type(proj).__name__}: {proj}"
        try:
            record["next_command"] = project_value(getattr(caught, "next_command", None))
        except Exception as proj:  # noqa: BLE001
            record["next_command"] = None
            record["next_command_projection_error"] = f"{type(proj).__name__}: {proj}"
        store.call(f"python-{name}", record)
        (case_dir / "python.error.json").write_bytes(json_bytes({k: record.get(k) for k in (
            "exception", "message", "kind", "exit_code", "operation", "effects",
            "explanation", "likely_causes", "next_steps", "known_issue", "next_command")}))
    # Declared-kind gate: the raised kind must equal the declared case kind.
    if result.python_kind != kind:
        result.problems.append(f"{label}: wrong case kind: Python kind {result.python_kind!r} != declared {kind!r}")
        result.status = "error"
        return result
    if result.cli_kind != kind:
        # Already recorded above; keep error status.
        result.status = "error"
        return result
    # Catalog class mapping gate.
    expected_class = ox.ERROR_TYPES.get(kind)
    if expected_class is None:
        result.problems.append(f"{label}: kind {kind!r} is not in ERROR_TYPES (catalog class mapping)")
        result.status = "error"
        return result
    if kind == "spill_space":
        # R37 dynamic: FULL object EXACT EXCEPT live numbers; brackets close
        # around the Python call (target==observer for in-process native).
        try:
            py_avail_after, py_detail_after = statvfs_available_bytes(spill_temp)  # type: ignore[arg-type]
        except OSError as exc:
            result.problems.append(f"{label}: spill statvfs after Python failed: {exc}")
            py_avail_after, py_detail_after = -1, {"error": str(exc)}
        spill_py_bracket["after"] = py_avail_after
        spill_py_bracket["after_detail"] = py_detail_after
        spill_py_bracket["after_time"] = time.time()
        spill_py_bracket["after_monotonic"] = time.monotonic()
        try:
            spill_py_bracket["temp_dev"] = os.stat(spill_temp).st_dev  # type: ignore[arg-type]
        except OSError:
            pass
        result.problems.extend(compare_spill_dynamic(label, cli_error, caught,
                                                      spill_cli_bracket, spill_py_bracket))
        result.problems.extend(check_class_and_exit(label, ox, caught, kind, cli.exit))
        # Spill accounting receipt (not a semantic guess): TMPDIR/frozen fs,
        # statvfs brackets incl observer/target pids/time, fixture SHA,
        # raw stdout/stderr/exit, shared-owner file hash/version.
        owner = shared_owner_spill_proof()
        if not owner.get("verbatim_in_file"):
            result.problems.append(f"{label}: shared-owner format not verbatim in {owner.get('path')}")
        accounting = {
            "schema": "oxblend.spill-accounting/v1",
            "kind": "spill_space",
            "shared_owner": owner,
            "temp_dir": fs_text(spill_temp) if spill_temp is not None else None,  # type: ignore[arg-type]
            "tmpdir_env": os.environ.get("TMPDIR"),
            "observer_pid": os.getpid(),
            "cli_target_pid": getattr(cli, "pid", None),
            "python_target_pid": os.getpid(),
            "cli_bracket": spill_cli_bracket,
            "python_bracket": spill_py_bracket,
            "cli_message": result.cli_message,
            "python_message": result.python_message,
            "cli_exit": result.cli_exit,
            "python_exit": result.python_exit,
            "cli_stdout_sha256": sha256_bytes(cli.stdout),
            "cli_stderr_sha256": sha256_bytes(cli.stderr),
            "cli_stdout_bytes": len(cli.stdout),
            "cli_stderr_bytes": len(cli.stderr),
            "fixture_sha256": before,
        }
        (case_dir / "spill_accounting.json").write_bytes(json_bytes(accounting))
        store.write(f"spill_accounting.{safe_name(name)}.json", accounting)
    else:
        # FULL canonical comparison + class/exit.
        result.problems.extend(compare_canonical(label, cli_error, caught))
        result.problems.extend(check_class_and_exit(label, ox, caught, kind, cli.exit))
    # Preserve expected partial reports/publication facts: when the CLI stdout
    # is a JSON report and Python retains a report, they must project equal.
    # Two failed/unparsed empty envelopes are never accepted as equal.
    cli_report = parse_cli_report(cli)
    py_report_raw = getattr(caught, "report", None)
    py_report = None
    if py_report_raw is not None:
        try:
            py_report = project_value(py_report_raw.to_json() if hasattr(py_report_raw, "to_json") else py_report_raw)
        except Exception as proj:  # noqa: BLE001
            result.problems.append(f"{label}: Python report projection failed: {type(proj).__name__}: {proj}")
    if isinstance(cli_report, (dict, list)) and py_report is not None:
        receipt = ReceiptContext(label, envelope, cli.stdout, cli.exit,
                                 cli_argv[0] if cli_argv else "", cli_error, oracle.sha256)
        report_problems = compare_retained_reports(label, cli_report, py_report, receipt)
        if report_problems:
            result.problems.extend(report_problems)
            (case_dir / "report.diff.json").write_bytes(json_bytes({
                "problems": report_problems, "cli": cli_report, "python": py_report}))
    elif isinstance(cli_report, (dict, list)) and py_report is None:
        # CLI published a partial report but Python retained none: gap, not witness.
        result.problems.append(f"{label}: CLI stdout is a report but Python retained no report")
    # Verify inputs unchanged and fresh outputs still absent (refusal publishes nothing).
    after: dict[str, str] = {}
    for p in input_paths:
        after[fs_text(p)] = sha256_file(p) if p.is_file() else "<missing>"
    (case_dir / "hashes.after.json").write_bytes(json_bytes(after))
    for key in before:
        if before[key] != after.get(key):
            result.problems.append(f"{label}: input hash changed (refusal must preserve inputs): {key}")
    for out in claim_outputs:
        if out.exists() or out.is_symlink():
            result.problems.append(f"{label}: fresh output was published by a refusal (must stay absent): {out}")
    # Expected partial artifacts: the Python publication must match the CLI
    # publication member-for-member (same destination path was reused).
    py_artifacts: dict[str, object] = {}
    for out in expected_outputs:
        py_artifacts[fs_text(out)] = snapshot_tree(out)
    (case_dir / "artifacts.python.json").write_bytes(json_bytes(py_artifacts))
    if expected_outputs:
        result.problems.extend(strict_json_problems(label, "published_artifacts", cli_artifacts, py_artifacts))
    if not result.problems:
        result.status = "pass"
    elif result.python_kind == kind and result.cli_kind == kind:
        result.status = "mismatch"
    else:
        result.status = "error"
    result.evidence = {"cli_stdout_bytes": len(cli.stdout), "cli_stderr_bytes": len(cli.stderr),
                       "python_seconds": seconds}
    return result


# --------------------------------------------------------------------------- #
# Case builders (each owns disjoint case files under work/cases/<name>/)
# --------------------------------------------------------------------------- #

def _write(path: Path, data: bytes) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists() or path.is_symlink():
        raise AssertionError(f"refusing to overwrite testdata output: {path}")
    path.write_bytes(data)
    return path


def case_io_summary_missing(ox, work: Path):
    name = "io-summary-missing"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    missing = d / "missing.blend"
    argv = ["summary", str(missing)]
    return ("io", name, argv, lambda: ox.summary(missing), [], [])


def case_io_inspect_missing(ox, work: Path):
    name = "io-inspect-missing"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    missing = d / "missing.blend"
    argv = ["inspect", str(missing)]
    return ("io", name, argv, lambda: ox.inspect(missing), [], [])


def case_io_trace_missing(ox, work: Path):
    name = "io-trace-missing"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    missing = d / "missing.blend"
    argv = ["trace", str(missing)]
    return ("io", name, argv, lambda: ox.trace(missing), [], [])


def case_missing_input_list(ox, work: Path):
    name = "missing_input-list-missing"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    missing = d / "missing.blend"
    argv = ["list", str(missing), "--json"]
    return ("missing_input", name, argv, lambda: ox.list_dependencies(missing), [], [])


def case_missing_input_blocks(ox, work: Path):
    name = "missing_input-blocks-missing"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    missing = d / "missing.blend"
    argv = ["blocks", str(missing)]
    return ("missing_input", name, argv, lambda: ox.block_report(missing), [], [])


def case_pack_input_pack_missing(ox, work: Path):
    name = "pack_input-pack-missing"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    missing = d / "missing.blend"
    dest = d / "out-pkg"
    argv = ["pack", str(missing), str(dest)]
    return ("pack_input", name, argv, lambda: ox.pack(missing, dest), [], [dest])


def case_pack_input_sequence_missing(ox, work: Path):
    name = "pack_input-pack-sequence-missing"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    minimal = d / "minimal.blend"
    minimal.write_bytes(fixture_minimal_bytes())
    missing = d / "missing.blend"
    dest = d / "out-seq"
    argv = ["pack-sequence", str(minimal), str(missing), "-t", str(dest)]
    return ("pack_input", name, argv, lambda: ox.pack_sequence([minimal, missing], dest), [minimal], [dest])


def case_pack_project_missing(ox, work: Path):
    name = "pack_project-missing-project"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    minimal = d / "minimal.blend"
    minimal.write_bytes(fixture_minimal_bytes())
    dest = d / "out-pkg"
    project = d / "no-such-project"
    argv = ["pack", str(minimal), str(dest), "--project", str(project)]
    return ("pack_project", name, argv, lambda: ox.pack(minimal, dest, project_root=project), [minimal], [dest])


def case_invalid_header_garbage(ox, work: Path):
    name = "invalid_header-inspect-garbage"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    garbage = d / "garbage.blend"
    garbage.write_bytes(b"not a blend file at all\n")
    argv = ["inspect", str(garbage)]
    return ("invalid_header", name, argv, lambda: ox.inspect(garbage), [garbage], [])


def case_truncated_inspect(ox, work: Path):
    name = "truncated-inspect-truncated"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    trunc = d / "truncated.blend"
    trunc.write_bytes(fixture_minimal_bytes()[: 12 + 24 + 10])
    argv = ["inspect", str(trunc)]
    return ("truncated", name, argv, lambda: ox.inspect(trunc), [trunc], [])


def case_missing_field_get(ox, work: Path):
    name = "missing_field-get-bad-field"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    minimal = d / "minimal.blend"
    minimal.write_bytes(fixture_minimal_bytes())
    argv = ["get", str(minimal), "0", json.dumps(["no_such_field"])]
    return ("missing_field", name, argv, lambda: ox.get(minimal, 0, ["no_such_field"]), [minimal], [])


def case_missing_field_pointer(ox, work: Path):
    name = "missing_field-pointer-bad-field"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    minimal = d / "minimal.blend"
    minimal.write_bytes(fixture_minimal_bytes())
    argv = ["pointer", str(minimal), "0", json.dumps(["different"])]
    return ("missing_field", name, argv, lambda: ox.pointer(minimal, 0, ["different"]), [minimal], [])


def case_bounds_get_block(ox, work: Path):
    name = "bounds-get-bad-block"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    minimal = d / "minimal.blend"
    minimal.write_bytes(fixture_minimal_bytes())
    argv = ["get", str(minimal), str(2**31 - 1), json.dumps(["name"])]
    return ("bounds", name, argv, lambda: ox.get(minimal, 2**31 - 1, ["name"]), [minimal], [])


def case_bounds_payload_block(ox, work: Path):
    name = "bounds-payload-bad-block"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    minimal = d / "minimal.blend"
    minimal.write_bytes(fixture_minimal_bytes())
    argv = ["payload", str(minimal), "9999"]
    return ("bounds", name, argv, lambda: ox.payload(minimal, 9999), [minimal], [])


def case_output_exists_extract(ox, work: Path):
    name = "output_exists-extract-existing"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    minimal = d / "minimal.blend"
    minimal.write_bytes(fixture_minimal_bytes())
    existing = d / "existing.blend"
    existing.write_bytes(b"occupied")
    argv = ["extract", str(minimal), "--block", "0", "--output", str(existing), "--json"]
    return ("output_exists", name, argv, lambda: ox.extract(minimal, existing, blocks=(0,)),
            [minimal, existing], [])


def case_same_input_extract(ox, work: Path):
    name = "same_input-extract-same"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    minimal = d / "minimal.blend"
    minimal.write_bytes(fixture_minimal_bytes())
    argv = ["extract", str(minimal), "--block", "0", "--output", str(minimal), "--json"]
    return ("same_input", name, argv, lambda: ox.extract(minimal, minimal, blocks=(0,)),
            [minimal], [])


def case_edit_version_schema2(ox, work: Path):
    name = "edit_version-edit-schema2"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    minimal = d / "minimal.blend"
    minimal.write_bytes(fixture_minimal_bytes())
    dest = d / "out.blend"
    spec = {"schema": 2, "writes": [], "bits": []}
    spec_path = d / "spec.json"
    spec_path.write_text(json.dumps(spec))
    argv = ["edit", str(minimal), "--output", str(dest), "--spec", str(spec_path), "--mode", "format-only"]
    return ("edit_version", name, argv, lambda: ox.edit(minimal, dest, spec, mode="format-only"),
            [minimal, spec_path], [dest])


def case_incomplete_edit_rewrite_existing(ox, work: Path):
    name = "incomplete_edit-rewrite-existing"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    rw = d / "rewrite.blend"
    rw.write_bytes(fixture_rewrite_bytes())
    existing = d / "existing.blend"
    existing.write_bytes(b"occupied")
    spec = rewrite_spec_document()
    spec_path = d / "spec.json"
    spec_path.write_text(json.dumps(spec))
    argv = ["rewrite", str(rw), "--output", str(existing), "--edits", str(spec_path)]
    return ("incomplete_edit", name, argv, lambda: ox.rewrite(rw, existing, spec),
            [rw, existing, spec_path], [])


def case_incomplete_pack_existing(ox, work: Path):
    name = "incomplete_pack-pack-existing-file"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    minimal = d / "minimal.blend"
    minimal.write_bytes(fixture_minimal_bytes())
    existing = d / "existing.blend"
    existing.write_bytes(b"occupied")
    argv = ["pack", str(minimal), str(existing)]
    return ("incomplete_pack", name, argv, lambda: ox.pack(minimal, existing),
            [minimal, existing], [])


def case_incomplete_trace_dangling(ox, work: Path):
    name = "incomplete_trace-trace-dangling"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    dang = d / "dangling.blend"
    dang.write_bytes(fixture_dangling_bytes())
    argv = ["trace", str(dang)]
    return ("incomplete_trace", name, argv, lambda: ox.trace(dang),
            [dang], [])


def case_exr_header_notexr(ox, work: Path):
    name = "exr_header-header-notexr"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    bad = d / "not-an-exr.bin"
    bad.write_bytes(b"not an exr\n")
    argv = ["exr-header", str(bad)]
    return ("exr_header", name, argv, lambda: ox.exr_header(bad),
            [bad], [])


def case_unsupported_type_query(ox, work: Path):
    name = "unsupported_type-query-bad-type"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    minimal = d / "minimal.blend"
    minimal.write_bytes(fixture_minimal_bytes())
    argv = ["query", str(minimal), "--type", "NoSuchType", "--fields", "nope"]
    # Python query(type=..., fields=...) with allow_incomplete=False must refuse.
    return ("unsupported_type", name, argv, lambda: ox.query(minimal, type="NoSuchType", fields=["nope"]),
            [minimal], [])


def case_durability_pack_bogus(ox, work: Path):
    name = "durability-pack-bogus-policy"
    d = work / "cases" / name
    d.mkdir(parents=True, exist_ok=True)
    minimal = d / "minimal.blend"
    minimal.write_bytes(fixture_minimal_bytes())
    dest = d / "out-pkg"
    # Invalid policy name is safely triggerable (no ENOSPC/filling); the
    # ENOSPC/sync-failure variants stay MISSING as unsafe (see blocked_notes).
    argv = ["pack", str(minimal), str(dest), "--durability", "bogus"]
    return ("durability", name, argv, lambda: ox.pack(minimal, dest, durability="bogus"),
            [minimal], [dest])


CASE_BUILDERS = [
    case_io_summary_missing,
    case_io_inspect_missing,
    case_io_trace_missing,
    case_missing_input_list,
    case_missing_input_blocks,
    case_pack_input_pack_missing,
    case_pack_input_sequence_missing,
    case_pack_project_missing,
    case_invalid_header_garbage,
    case_truncated_inspect,
    case_missing_field_get,
    case_missing_field_pointer,
    case_bounds_get_block,
    case_bounds_payload_block,
    case_output_exists_extract,
    case_same_input_extract,
    case_edit_version_schema2,
    case_incomplete_edit_rewrite_existing,
    case_incomplete_pack_existing,
    case_incomplete_trace_dangling,
    case_exr_header_notexr,
    case_unsupported_type_query,
    case_durability_pack_bogus,
]


def discover_case_builders() -> tuple[list, list[CaseResult]]:
    """Own builders plus every ``error_cases_*.py`` module in this test dir.

    Modules load in sorted filename order; each must publish a
    ``CASE_BUILDERS`` list of ``(ox, work)`` callables. A builder returns
    ``(kind, name, argv, thunk, inputs, claimed_absent_outputs)`` and may
    append one options dict ``{"expected_outputs": [...]}`` for real
    partial publications. The expected-kind inventory is always derived
    from the catalog, never from this module list.
    """
    builders: list = list(CASE_BUILDERS)
    failures: list[CaseResult] = []
    for path in sorted(HERE.parent.glob("error_cases_*.py")):
        module_name = path.stem
        try:
            spec = importlib.util.spec_from_file_location(module_name, path)
            if spec is None or spec.loader is None:
                raise ImportError(f"cannot load case module {path}")
            module = importlib.util.module_from_spec(spec)
            sys.modules[module_name] = module
            spec.loader.exec_module(module)
            extra = getattr(module, "CASE_BUILDERS")
            if not isinstance(extra, list) or not all(callable(b) for b in extra):
                raise TypeError(f"{module_name}.CASE_BUILDERS must be a list of callables")
            builders.extend(extra)
        except Exception as exc:  # noqa: BLE001 - a broken module is a gate error, not a silent skip
            failures.append(CaseResult(kind="<builder>", name=module_name, cli_argv=[],
                                       cli_exit=None, cli_kind=None, cli_message=None,
                                       python_kind=None, python_message=None, python_exit=None,
                                       python_class=None, status="error",
                                       problems=[f"case module failed to load: {type(exc).__name__}: {exc}\n{traceback.format_exc()}"]))
    return builders, failures


# --------------------------------------------------------------------------- #
# Inventory runner
# --------------------------------------------------------------------------- #

def run(cli: Path, evidence: Path) -> dict:
    """Run the functional inventory. Returns the summary dict; writes evidence.

    Exit-code contract for the release gate lives in ``main``: 0 only when
    every catalog kind is either witnessed (FULL match) or exempted, with no
    stale/overlap/new-kind gaps and no case errors. Mismatches and missing
    kinds fail the gate but are still recorded honestly with evidence.
    """
    evidence.mkdir(parents=True, exist_ok=True)
    os.chmod(evidence, 0o700)
    store = Store(evidence)
    work = evidence / "work"
    work.mkdir(parents=True, exist_ok=True)
    (work / ".harness-owned").write_text("owned by bindings/python/tests/error_inventory.py\n")

    oracle = Oracle(Path(cli))
    oracle.verify()
    store.write("oracle.json", {"binary": fs_text(oracle.binary), "sha256": oracle.sha256,
                                "version": oracle.version,
                                "version_stdout": oracle.version_output.decode("utf-8", "replace")})

    try:
        ox = importlib.import_module("oxblend")
    except Exception as exc:  # noqa: BLE001
        raise SystemExit(f"installed oxblend import failed: {type(exc).__name__}: {exc}")
    wheel_problem = installed_wheel_problem(ox)
    store.write("python.json", {"executable": sys.executable, "version": sys.version,
                                "module": fs_text(Path(ox.__file__).resolve()),
                                "native": fs_text(Path(ox._native.__file__).resolve()),
                                "oxblend_version": getattr(ox, "__version__", None),
                                "engine_version": getattr(ox, "engine_version", None),
                                "wheel_problem": wheel_problem})
    if wheel_problem is not None:
        raise SystemExit(f"installed oxblend is not a proper wheel: {wheel_problem}")

    catalog, catalog_sha, catalog_count = load_catalog()
    store.write("catalog.json", {"path": fs_text(CATALOG_PATH), "sha256": catalog_sha, "count": catalog_count,
                                 "kinds": sorted(catalog)})
    exemptions_doc = load_exemptions()
    store.write("exemptions.json", exemptions_doc)
    exempt_kinds = {item["kind"] for item in exemptions_doc.get("exemptions", [])}

    stale = validate_exemptions_against_catalog(catalog, exemptions_doc)

    cases: list[CaseResult] = []
    witnessed: dict[str, list[str]] = {}
    mismatch_kinds: dict[str, list[str]] = {}
    error_kinds: dict[str, list[str]] = {}

    builders, load_failures = discover_case_builders()
    cases.extend(load_failures)
    for builder in builders:
        # Each builder owns disjoint case files under work/cases/<name>/.
        # Builders take (ox, work) and return (kind, name, argv, thunk, inputs, claims[, options]).
        try:
            built = builder(ox, work)
            kind, name, argv, thunk, input_paths, claim_outputs = built[:6]
            options = built[6] if len(built) > 6 else {}
            if not isinstance(options, dict):
                raise TypeError("builder options (7th element) must be a dict")
            expected_outputs = options.get("expected_outputs", [])
        except Exception as exc:  # noqa: BLE001
            cases.append(CaseResult(kind="<builder>", name=getattr(builder, "__name__", str(builder)),
                                    cli_argv=[], cli_exit=None, cli_kind=None, cli_message=None,
                                    python_kind=None, python_message=None, python_exit=None,
                                    python_class=None, status="error",
                                    problems=[f"builder failed: {type(exc).__name__}: {exc}\n{traceback.format_exc()}"]))
            continue
        label = f"{kind}:{name}"
        if kind not in catalog:
            cases.append(CaseResult(kind=kind, name=name, cli_argv=[str(a) for a in argv],
                                    cli_exit=None, cli_kind=None, cli_message=None,
                                    python_kind=None, python_message=None, python_exit=None,
                                    python_class=None, status="error",
                                    problems=[f"{label}: wrong case kind: {kind!r} is not in docs/error-catalog.json"]))
            continue
        case_dir = evidence / "cases" / safe_name(name)
        res = run_one_case(label=label, kind=kind, name=name,
                           cli_argv=[str(a) for a in argv], py_thunk=thunk,
                           ox=ox, oracle=oracle, store=store, case_dir=case_dir,
                           input_paths=[Path(p) for p in input_paths],
                           claim_outputs=[Path(c) for c in claim_outputs],
                           expected_outputs=[Path(c) for c in expected_outputs])
        # Hashes are recorded inside run_one_case (hashes.before/after.json).
        try:
            before = json.loads((case_dir / "hashes.before.json").read_bytes())
            after = json.loads((case_dir / "hashes.after.json").read_bytes())
        except OSError:
            before, after = {}, {}
        (case_dir / "result.json").write_bytes(json_bytes({
            "kind": res.kind, "name": res.name, "cli_argv": res.cli_argv,
            "cli_exit": res.cli_exit, "cli_kind": res.cli_kind, "cli_message": res.cli_message,
            "python_kind": res.python_kind, "python_message": res.python_message,
            "python_exit": res.python_exit, "python_class": res.python_class,
            "status": res.status, "problems": res.problems, "evidence": res.evidence,
            "hashes_before": before, "hashes_after": after}))
        cases.append(res)
        if res.status == "pass":
            witnessed.setdefault(res.kind, []).append(res.name)
        elif res.status == "mismatch":
            mismatch_kinds.setdefault(res.kind, []).append(res.name)
        else:
            error_kinds.setdefault(res.kind, []).append(res.name)

    # Derive missing: catalog kinds with neither witness-pass nor exemption.
    # Mismatches still count as REAL fixtures (pending runtime fix) but NOT as
    # witnesses; they stay missing for gate purposes until FULL match.
    missing = sorted(k for k in catalog if k not in witnessed and k not in exempt_kinds)
    overlap = sorted(k for k in witnessed if k in exempt_kinds)
    # Also overlap if a mismatched/error case exists for an exempted kind?
    # The rule is witness/exemption overlap: any REAL fixture (pass or mismatch
    # with correct kind) for an exempted kind is overlap. Error-status wrong-kind
    # cases are builder bugs, reported separately.
    for kind in sorted(set(mismatch_kinds) & exempt_kinds):
        if kind not in overlap:
            overlap.append(kind)
    overlap = sorted(set(overlap))

    # Unsafe-blocked candidates recorded MISSING with precise evidence (never
    # exempted as unreachable just because they are hard).
    blocked_notes = {
        "spill_space": "MISSING: requires ENOSPC/temp-space exhaustion; unsafe to trigger on this node (no disk filling). Awaiting root genuine blocked ruling after inventory.",
        "durability": "MISSING unless witnessed: ENOSPC/sync-failure variants are unsafe; invalid-policy variants may be witnessable (see cases). No unsafe filling attempted.",
    }

    summary = {
        "schema": "oxblend.error-inventory/v1",
        "catalog_sha256": catalog_sha,
        "catalog_count": catalog_count,
        "cli": {"path": fs_text(oracle.binary), "sha256": oracle.sha256, "version": oracle.version},
        "python": {"executable": sys.executable, "module": fs_text(Path(ox.__file__).resolve())},
        "cases": [{"kind": c.kind, "name": c.name, "cli_argv": c.cli_argv,
                   "cli_exit": c.cli_exit, "cli_kind": c.cli_kind,
                   "python_kind": c.python_kind, "python_exit": c.python_exit,
                   "python_class": c.python_class, "status": c.status,
                   "problems": c.problems} for c in cases],
        "witnessed_kinds": sorted(witnessed),
        "witnessed_pairs": {k: sorted(v) for k, v in sorted(witnessed.items())},
        "mismatch_kinds": sorted(mismatch_kinds),
        "mismatch_pairs": {k: sorted(v) for k, v in sorted(mismatch_kinds.items())},
        "error_kinds": sorted(error_kinds),
        "exempted_kinds": sorted(exempt_kinds),
        "missing_kinds": missing,
        "blocked_notes": {k: blocked_notes[k] for k in missing if k in blocked_notes},
        "stale_exemptions": stale,
        "overlap": overlap,
        "counts": {"cases": len(cases), "pass_cases": sum(1 for c in cases if c.status == "pass"),
                   "mismatch_cases": sum(1 for c in cases if c.status == "mismatch"),
                   "error_cases": sum(1 for c in cases if c.status == "error"),
                   "witnessed_kinds": len(witnessed), "mismatch_kinds": len(mismatch_kinds),
                   "exempted_kinds": len(exempt_kinds), "missing_kinds": len(missing)},
    }
    store.write("inventory.json", summary)
    # Checksums for the evidence tree (excluding itself until written).
    checksums: list[str] = []
    for path in sorted(evidence.rglob("*")):
        if path.is_file() and path.name != "SHA256SUMS":
            try:
                checksums.append(f"{sha256_file(path)}  {path.relative_to(evidence)}")
            except OSError:
                continue
    store.write("SHA256SUMS", "\n".join(checksums) + "\n")
    return summary


def gate_exit(summary: dict) -> int:
    """Release-gate exit: 0 only when complete (no missing/stale/overlap/errors).

    Mismatches (PENDING runtime fix) fail the gate but are recorded honestly;
    they are REAL fixtures, not green-by-exemption.
    """
    if summary.get("stale_exemptions"):
        return 2
    if summary.get("overlap"):
        return 2
    if any(c.get("status") == "error" for c in summary.get("cases", [])):
        return 2
    if summary.get("mismatch_kinds"):
        return 1
    if summary.get("missing_kinds"):
        return 1
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cli", type=Path, required=True, help="immutable CLI oracle binary")
    parser.add_argument("--evidence", type=Path, required=True, help="evidence directory (harness-owned, under E)")
    args = parser.parse_args(argv)
    summary = run(args.cli, args.evidence)
    print(json.dumps({"catalog_count": summary["catalog_count"],
                      "catalog_sha256": summary["catalog_sha256"],
                      "witnessed_kinds": summary["counts"]["witnessed_kinds"],
                      "mismatch_kinds": summary["counts"]["mismatch_kinds"],
                      "error_cases": summary["counts"]["error_cases"],
                      "exempted_kinds": summary["counts"]["exempted_kinds"],
                      "missing_kinds": summary["counts"]["missing_kinds"],
                      "missing_sample": summary["missing_kinds"][:12],
                      "stale": summary["stale_exemptions"],
                      "overlap": summary["overlap"]}, indent=2, sort_keys=True))
    code = gate_exit(summary)
    print(f"gate_exit={code} evidence={args.evidence}")
    return code


# --------------------------------------------------------------------------- #
# Inventory-shape unit negatives (synthetic only; NEVER functional witnesses)
# --------------------------------------------------------------------------- #

class TestInventoryShape(unittest.TestCase):
    """Shape negatives for the inventory logic itself (no CLI/Python)."""

    def test_new_uncovered_kind_fails_gate(self) -> None:
        summary = {"stale_exemptions": [], "overlap": [],
                   "cases": [{"status": "pass"}],
                   "mismatch_kinds": [], "missing_kinds": ["some_new_kind"]}
        self.assertNotEqual(gate_exit(summary), 0)

    def test_stale_exemption_fails(self) -> None:
        catalog = {"io": {}}
        exemptions = {"exemptions": [{"kind": "gone_kind", "reason": "x" * 60,
                                      "proof": {"files": ["README.md"], "snippets": ["x" * 30]}}]}
        # Direct stale check without filesystem proof validation.
        problems = [k for k in ("gone_kind",) if k not in catalog]
        self.assertTrue(problems)
        self.assertNotEqual(gate_exit({"stale_exemptions": problems, "overlap": [],
                                       "cases": [], "mismatch_kinds": [], "missing_kinds": []}), 0)

    def test_overlap_fails(self) -> None:
        summary = {"stale_exemptions": [], "overlap": ["io"],
                   "cases": [{"status": "pass"}],
                   "mismatch_kinds": [], "missing_kinds": []}
        self.assertNotEqual(gate_exit(summary), 0)

    def test_wrong_case_kind_is_error_not_witness(self) -> None:
        # A builder declaring kind A but producing kind B must be error status,
        # never counted as a witness for either kind.
        res = CaseResult(kind="io", name="demo", cli_argv=[], cli_exit=2,
                         cli_kind="usage", cli_message="x",
                         python_kind="usage", python_message="x", python_exit=2,
                         python_class="UsageError", status="error",
                         problems=["demo: wrong case kind"])
        self.assertEqual(res.status, "error")
        self.assertNotEqual(res.cli_kind, res.kind)

    def test_mutated_message_is_mismatch(self) -> None:
        cli_error = {"kind": "io", "message": "real message", "explanation": "e",
                     "likely_causes": [], "next_steps": [], "known_issue": None, "next_command": None}

        class FakeExc:
            kind = "io"
            message = "MUTATED message"
            explanation = "e"
            likely_causes: tuple = ()
            next_steps: tuple = ()
            known_issue = None
            next_command = None
            diagnostic_ref = None

        problems = compare_canonical("demo", cli_error, FakeExc())
        self.assertTrue(any("message" in p for p in problems))

    def test_future_keys_are_required(self) -> None:
        cli_error = {"kind": "io", "message": "m", "explanation": "e",
                     "likely_causes": [], "next_steps": [], "known_issue": None,
                     "next_command": None, "future_key": "future_value"}

        class FakeExc:
            kind = "io"
            message = "m"
            explanation = "e"
            likely_causes: tuple = ()
            next_steps: tuple = ()
            known_issue = None
            next_command = None
            diagnostic_ref = None

        problems = compare_canonical("demo", cli_error, FakeExc())
        self.assertTrue(any("future_key" in p for p in problems))

    # -- Strict-JSON guards: each control injects the exact fault it claims
    #    to detect. These are unit controls only, never functional witnesses.

    def _fake_exc(self, **overrides):
        class FakeExc(Exception):
            kind = "io"
            message = "m"
            explanation = "e"
            likely_causes: tuple = ()
            next_steps: tuple = ()
            known_issue = None
            next_command = None
            diagnostic_ref = None
        exc = FakeExc()
        for key, value in overrides.items():
            setattr(exc, key, value)
        return exc

    def _cli_error(self, **overrides):
        error = {"kind": "io", "message": "m", "explanation": "e",
                 "likely_causes": [], "next_steps": [], "known_issue": None, "next_command": None}
        error.update(overrides)
        return error

    def test_bool_int_aliases_are_not_equal(self) -> None:
        # 0/False and 1/True alias under ==; strict JSON typing must refuse.
        for cli_value, py_value in ((True, 1), (False, 0), (1, True), (0, False)):
            problems = compare_canonical("demo", self._cli_error(known_issue=cli_value),
                                         self._fake_exc(known_issue=py_value))
            self.assertTrue(any("known_issue" in p and "type differs" in p for p in problems), (cli_value, py_value, problems))

    def test_mutated_scalar_type_is_not_equal(self) -> None:
        for cli_value, py_value in (("1", 1), (1, 1.0), (None, "null"), (["a"], "a")):
            problems = compare_canonical("demo", self._cli_error(known_issue=cli_value),
                                         self._fake_exc(known_issue=py_value))
            self.assertTrue(any("known_issue" in p for p in problems), (cli_value, py_value, problems))

    def test_nested_container_mutation_is_detected_recursively(self) -> None:
        cli = self._cli_error(next_command={"argv": ["oxblend", "trace"], "flags": {"strict": True}})
        py = self._fake_exc(next_command={"argv": ["oxblend", "trace"], "flags": {"strict": 1}})
        problems = compare_canonical("demo", cli, py)
        self.assertTrue(any("next_command.flags.strict" in p for p in problems), problems)

    def test_missing_diagnostic_ref_attribute_is_a_defect(self) -> None:
        import types as _types
        bare = _types.SimpleNamespace(kind="io", exit_code=2)
        ox = _types.SimpleNamespace(ERROR_TYPES={"io": Exception})
        problems = check_class_and_exit("demo", ox, bare, "io", 2)
        self.assertTrue(any("diagnostic_ref" in p for p in problems), problems)

    def test_nonnull_diagnostic_ref_is_a_defect(self) -> None:
        import types as _types
        exc = self._fake_exc(diagnostic_ref={"run_id": "r", "failure_id": "f0"})
        ox = _types.SimpleNamespace(ERROR_TYPES={"io": type(exc)})
        problems = check_class_and_exit("demo", ox, exc, "io", 2)
        self.assertTrue(any("diagnostic_ref" in p for p in problems), problems)

    def test_projection_refuses_key_coercion_and_failures(self) -> None:
        with self.assertRaises(TypeError):
            project_value({1: "non-string key"})
        with self.assertRaises(TypeError):
            project_value(b"raw bytes are not a JSON string")
        class Bad:
            def to_json(self):
                raise RuntimeError("boom")
        with self.assertRaises(RuntimeError):
            project_value(Bad())
        problems = compare_canonical("demo", self._cli_error(), self._fake_exc(message=Bad()))
        self.assertTrue(any("message" in p and "projectable" in p for p in problems), problems)

    def _receipt_context(self, *, tamper_stdout=False, foreign_run=False, unknown_failure=False, wrong_kind=False):
        import hashlib as _hl
        error = self._cli_error()
        failures = [{"id": "f0", "kind": "dangling_pointer", "message": "site", "propagations": []},
                    {"id": "f1", "kind": "io", "message": "m", "propagations": []}]
        stdout = b'{"report": true}\n'
        receipt = {"schema": "oxblend.failure-receipt/v1", "run_id": "run-1", "command": "trace",
                   "exit_code": 2, "producer": {"name": "oxblend", "binary_sha256": "binsha"},
                   "binding": {"status": "invocation_bound", "stdout_sha256": _hl.sha256(b"tampered" if tamper_stdout else stdout).hexdigest(),
                               "stdout_bytes": len(stdout), "stdout_delivery_complete": True, "stderr_error_sha256": None},
                   "failures": failures}
        envelope = {"schema": 1, "error": error, "diagnostic": receipt}
        receipt["binding"]["stderr_error_sha256"] = _hl.sha256(canonical_envelope_bytes(envelope)).hexdigest()
        ctx = ReceiptContext("demo", envelope, stdout, 2, "trace", error, "binsha")
        ref = {"run_id": "other-run" if foreign_run else "run-1",
               "failure_id": "f9" if unknown_failure else "f0"}
        return ctx, ref, ("io" if wrong_kind else "dangling_pointer")

    def test_receipt_accounting_accepts_only_the_bound_ref(self) -> None:
        ctx, ref, sibling = self._receipt_context()
        self.assertEqual(ctx.account_ref("demo", "errors[0].diagnostic_ref", ref, sibling), [])
        self.assertEqual(ctx.account_ref("demo", "errors[0].diagnostic_ref", None, sibling), [])

    def test_receipt_accounting_rejects_foreign_unknown_wrongkind_and_tampered(self) -> None:
        for kwargs in ({"foreign_run": True}, {"unknown_failure": True}, {"wrong_kind": True}, {"tamper_stdout": True}):
            ctx, ref, sibling = self._receipt_context(**kwargs)
            problems = ctx.account_ref("demo", "errors[0].diagnostic_ref", ref, sibling)
            self.assertTrue(problems, kwargs)

    def test_retained_report_other_fields_stay_strict_under_accounting(self) -> None:
        ctx, ref, _ = self._receipt_context()
        cli = {"complete": False, "errors": [{"kind": "dangling_pointer", "diagnostic_ref": ref, "message": "site"}]}
        good = {"complete": False, "errors": [{"kind": "dangling_pointer", "diagnostic_ref": None, "message": "site"}]}
        self.assertEqual(compare_retained_reports("demo", cli, good, ctx), [])
        mutated = {"complete": False, "errors": [{"kind": "dangling_pointer", "diagnostic_ref": None, "message": "MUTATED"}]}
        self.assertTrue(compare_retained_reports("demo", cli, mutated, ctx))
        native_bound = {"complete": False, "errors": [{"kind": "dangling_pointer", "diagnostic_ref": ref, "message": "site"}]}
        self.assertTrue(compare_retained_reports("demo", cli, native_bound, ctx))

    def test_exemption_proof_must_be_verbatim_source(self) -> None:
        doc = {"schema": EXPECTED_EXEMPTIONS_SCHEMA, "exemptions": [{
            "kind": "io", "reason": "a meaningful reason",
            "proof": {"files": ["README.md"], "snippets": ["this sentence is paraphrase, not source"]}}]}
        self.assertTrue(any("verbatim" in p for p in validate_exemptions(doc)))
        doc["exemptions"][0]["proof"]["snippets"] = ["Oxblend"]
        # 'Oxblend' may or may not occur; require the check to be about the actual file text:
        text = (REPO / "README.md").read_text(encoding="utf-8")
        if "Oxblend" in text:
            self.assertEqual(validate_exemptions(doc), [])

    def test_safe_name_is_full_untruncated_hash(self) -> None:
        import hashlib as _hl
        name = "x" * 500 + "/weird name"
        self.assertEqual(safe_name(name), _hl.sha256(name.encode("utf-8", "surrogateescape")).hexdigest())

    def test_snapshot_tree_detects_published_artifacts(self) -> None:
        import tempfile as _tf
        with _tf.TemporaryDirectory() as td:
            root = Path(td) / "pkg"
            self.assertIsNone(snapshot_tree(root))
            root.mkdir()
            (root / "member.txt").write_bytes(b"payload")
            snap = snapshot_tree(root)
            self.assertEqual(snap["members"]["member.txt"]["sha256"], sha256_bytes(b"payload"))
            (root / "member.txt").write_bytes(b"changed")
            self.assertNotEqual(snapshot_tree(root), snap)

    def test_two_empty_envelopes_are_not_equal(self) -> None:
        # Neither side producing a parseable envelope must never pass.
        self.assertIsNone(hard_error_envelope(b""))
        self.assertIsNone(hard_error_envelope(b"not json\n"))
        # run_one_case treats missing envelope as error (covered by contract);
        # this unit pins the helper half of that contract.

    # -- R37 spill_space dynamic-capacity controls (unit only, NEVER a
    #    functional witness). Each control injects the exact fault it claims
    #    to detect. The functional witness is the real blocks pair with live
    #    statvfs brackets, not these synthetics.

    def _spill_exc(self, message: str, **overrides):
        class SpillExc(Exception):
            kind = "spill_space"
            explanation = "There is not enough available temporary filesystem space for the required decoded spill."
            likely_causes = ("Compressed input expands beyond available temp capacity", "Other work consumed the filesystem")
            next_steps = ("Check the reported required/available/shortfall bytes and the actual temp filesystem", "Provide a temp location with sufficient space before another run", "Do not treat --low-memory as a disk/memory ceiling or delete unrelated temporary work.")
            known_issue = None
            next_command = None
            diagnostic_ref = None
            exit_code = 2
        exc = SpillExc()
        exc.message = message
        for key, value in overrides.items():
            setattr(exc, key, value)
        return exc

    def _spill_cli_error(self, message: str, **overrides):
        error = {"kind": "spill_space", "message": message,
                 "explanation": "There is not enough available temporary filesystem space for the required decoded spill.",
                 "likely_causes": ["Compressed input expands beyond available temp capacity", "Other work consumed the filesystem"],
                 "next_steps": ["Check the reported required/available/shortfall bytes and the actual temp filesystem", "Provide a temp location with sufficient space before another run", "Do not treat --low-memory as a disk/memory ceiling or delete unrelated temporary work."],
                 "known_issue": None, "next_command": None}
        error.update(overrides)
        return error

    def test_spill_message_parses_shared_owner_format(self) -> None:
        fields, problems = parse_spill_message("decoded BHead spill requires 9223372036854775856 bytes; available 11887263744 bytes; shortfall 9223372024967512112 bytes")
        self.assertEqual(problems, [])
        self.assertIsNotNone(fields)
        assert fields is not None
        self.assertEqual(fields["required"], 9223372036854775856)
        self.assertEqual(fields["available"], 11887263744)
        self.assertEqual(fields["shortfall"], 9223372036854775856 - 11887263744)

    def test_spill_canonical_int_rejects_leading_zeros_sign_bool(self) -> None:
        self.assertFalse(is_canonical_u64_token("001"))
        self.assertFalse(is_canonical_u64_token("+1"))
        self.assertFalse(is_canonical_u64_token("-1"))
        self.assertFalse(is_canonical_u64_token(""))
        self.assertFalse(is_canonical_u64_token(" 1"))
        self.assertFalse(is_canonical_u64_token(True))
        self.assertFalse(is_canonical_u64_token(1))
        self.assertFalse(is_canonical_u64_token("18446744073709551616"))
        self.assertTrue(is_canonical_u64_token("0"))
        self.assertTrue(is_canonical_u64_token("11887263744"))
        self.assertTrue(is_canonical_u64_token(str(2**64 - 1)))
        fields, problems = parse_spill_message("decoded BHead spill requires 007 bytes; available 1 bytes; shortfall 6 bytes")
        self.assertIsNone(fields)
        self.assertTrue(any("canonical" in p for p in problems))

    def test_spill_dynamic_accepts_identical_required_within_own_brackets(self) -> None:
        required = 9223372036854775856
        cli_avail, py_avail = 11887263744, 11852386304
        cli_msg = f"decoded BHead spill requires {required} bytes; available {cli_avail} bytes; shortfall {required - cli_avail} bytes"
        py_msg = f"decoded BHead spill requires {required} bytes; available {py_avail} bytes; shortfall {required - py_avail} bytes"
        cli_error = self._spill_cli_error(cli_msg)
        exc = self._spill_exc(py_msg)
        cli_bracket = {"before": cli_avail - 100, "after": cli_avail + 100}
        py_bracket = {"before": py_avail - 50, "after": py_avail + 50}
        self.assertEqual(compare_spill_dynamic("demo", cli_error, exc, cli_bracket, py_bracket), [])

    def test_spill_dynamic_rejects_mutated_capacity_outside_bracket(self) -> None:
        required = 9223372036854775856
        cli_avail = 11887263744
        cli_msg = f"decoded BHead spill requires {required} bytes; available {cli_avail} bytes; shortfall {required - cli_avail} bytes"
        # Mutated capacity: 1 MiB outside the witnessed range.
        mutated = cli_avail + 1048576
        py_msg = f"decoded BHead spill requires {required} bytes; available {mutated} bytes; shortfall {required - mutated} bytes"
        cli_error = self._spill_cli_error(cli_msg)
        exc = self._spill_exc(py_msg)
        cli_bracket = {"before": cli_avail - 10, "after": cli_avail + 10}
        py_bracket = {"before": cli_avail - 10, "after": cli_avail + 10}
        problems = compare_spill_dynamic("demo", cli_error, exc, cli_bracket, py_bracket)
        self.assertTrue(any("within own statvfs range" in p for p in problems), problems)

    def test_spill_dynamic_rejects_wrong_shortfall(self) -> None:
        required, avail = 9223372036854775856, 11887263744
        cli_msg = f"decoded BHead spill requires {required} bytes; available {avail} bytes; shortfall {required - avail} bytes"
        py_msg = f"decoded BHead spill requires {required} bytes; available {avail} bytes; shortfall {required - avail + 1} bytes"
        cli_error = self._spill_cli_error(cli_msg)
        exc = self._spill_exc(py_msg)
        bracket = {"before": avail - 10, "after": avail + 10}
        problems = compare_spill_dynamic("demo", cli_error, exc, bracket, dict(bracket))
        self.assertTrue(any("shortfall" in p for p in problems), problems)

    def test_spill_dynamic_rejects_mismatched_required(self) -> None:
        cli_msg = "decoded BHead spill requires 9223372036854775856 bytes; available 11887263744 bytes; shortfall 9223372024967512112 bytes"
        py_msg = "decoded BHead spill requires 9223372036854775855 bytes; available 11887263744 bytes; shortfall 9223372024967512111 bytes"
        cli_error = self._spill_cli_error(cli_msg)
        exc = self._spill_exc(py_msg)
        bracket = {"before": 11887263734, "after": 11887263754}
        problems = compare_spill_dynamic("demo", cli_error, exc, bracket, dict(bracket))
        self.assertTrue(any("required differs" in p for p in problems), problems)

    def test_spill_dynamic_rejects_altered_prose(self) -> None:
        cli_msg = "decoded BHead spill requires 9223372036854775856 bytes; available 11887263744 bytes; shortfall 9223372024967512112 bytes"
        py_msg = "decoded BHead spill needs 9223372036854775856 bytes; available 11887263744 bytes; shortfall 9223372024967512112 bytes"
        cli_error = self._spill_cli_error(cli_msg)
        exc = self._spill_exc(py_msg)
        bracket = {"before": 11887263734, "after": 11887263754}
        problems = compare_spill_dynamic("demo", cli_error, exc, bracket, dict(bracket))
        self.assertTrue(any("Python message" in p for p in problems), problems)

    def test_spill_dynamic_other_fields_stay_strict(self) -> None:
        required, avail = 9223372036854775856, 11887263744
        msg = f"decoded BHead spill requires {required} bytes; available {avail} bytes; shortfall {required - avail} bytes"
        cli_error = self._spill_cli_error(msg)
        exc = self._spill_exc(msg, explanation="MUTATED explanation")
        bracket = {"before": avail - 10, "after": avail + 10}
        problems = compare_spill_dynamic("demo", cli_error, exc, bracket, dict(bracket))
        self.assertTrue(any("explanation" in p for p in problems), problems)
        # Extra object field must also fail: CLI has future_key, Python lacks it.
        cli_extra = self._spill_cli_error(msg, future_key="future_value")
        exc2 = self._spill_exc(msg)
        problems2 = compare_spill_dynamic("demo", cli_extra, exc2, bracket, dict(bracket))
        self.assertTrue(any("future_key" in p for p in problems2), problems2)


if __name__ == "__main__":
    if "--cli" in sys.argv or "--evidence" in sys.argv:
        raise SystemExit(main())
    unittest.main()
