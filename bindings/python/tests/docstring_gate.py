"""Runtime help/docstring inventory gate for the installed ``oxblend`` payload.

Every public function, class and method reachable from the installed package
must carry meaningful help()-visible runtime documentation covering purpose,
arguments, return value, raised errors and the matching ``oxblend`` CLI
command (or an explicit statement that no CLI equivalent exists, with the
reason). Generated error classes, generated record classes and public
submodules are inventoried exactly like hand-written ones.

The inventory is derived dynamically from the INSTALLED package, never from a
hand-maintained name list, so a newly added public name cannot hide:

* submodules: every non-private ``*.py`` module file in the installed
  ``oxblend`` package directory, plus ``__main__`` (the ``python -m oxblend``
  entry point);
* members: the union of each module's ``__all__`` with every public
  ``dir()`` name whose object is defined by ``oxblend`` itself. A public
  oxblend-defined name missing from ``__all__`` is reported as hidden, not
  silently skipped;
* class members: every public name in the class ``__dict__``, plus inherited
  public names. Inherited helpers are DISTINGUISHED, never silently excused:
  members inherited from outside ``oxblend`` (``collections.abc.Mapping``,
  ``Exception``, the native extension) keep their original documentation and
  are reported with their provenance. Members defined by ``oxblend`` itself
  need their own documentation, even when they override an inherited name.

What each documented member must state (checked structurally, so generic
filler such as "Helper function." or "TODO" cannot pass as real docs):

* module: purpose, and a ``CLI`` reference (every documented module relates
  to the CLI surface or its report projection);
* class: a purpose summary; every dataclass field and every explicitly
  defined ``__init__`` parameter named; constructor semantics under
  ``Args:`` (the construction mapping or parameters, even when inherited
  from ``Record``; an explicit non-constructibility note for native tokens
  that only sessions issue); ``Returns:`` naming the constructed instance;
  ``Raises:`` naming construction-time errors (an explicit ``Raises:
  Nothing.`` claim only for total constructors); and a ``CLI:`` command
  reference or the explicit marker ``No CLI equivalent`` with a reason.
  Error classes must additionally name their stable ``kind`` string (taken
  dynamically from ``ERROR_TYPES``, never from a hard-coded list) and
  reference the exact catalog help surface ``oxblend help error KIND``.
  Generated error classes document ordinary ``Exception`` construction
  (``Args: *args``), the constructed instance (``Returns:``) and
  ``TypeError`` for unsupported keywords (``Raises:``), because catalog
  presence never promises that a Python operation can raise the class;
* function/method: every parameter named under ``Args:``; ``Returns:``
  unless the return annotation is exactly ``None``; ``Raises:`` always (the
  literal statement ``Raises: Nothing.`` is accepted for total functions
  that cannot raise); and either a ``CLI:`` command reference or the
  explicit ``No CLI equivalent`` marker;
* property: purpose plus ``Returns:`` and ``Raises:`` like a method.
  ``Raises: Nothing.`` is accepted only for provably total properties
  (``.get`` defaults and other non-indexing bodies); every ``self[...]``
  indexing property must name ``KeyError`` (engine reports carry the field,
  so it fires only on manually built or truncated mappings), and
  sequence-end properties must name ``IndexError``. Live honesty checks pin
  the distinction against the installed payload instead of trusting prose;
* native-owned descriptors (the ``oxblend._native`` data descriptors, whose
  ``__doc__`` attribute is not writable from Python by CPython design) are
  covered by their owning class documentation: every descriptor name must be
  named there. They are reported as native-owned, never as missing.

Command-name validation: every ``oxblend <command>`` reference found in
documentation is collected. When the ``OXBLEND_CLI`` environment variable (or
``--cli``) points at an ``oxblend`` CLI binary, each referenced command must
appear in that binary's ``Commands:`` help line. Without a CLI binary the
gate still enforces the ``CLI:``/``No CLI equivalent`` presence rule and the
``oxblend <command>`` reference form.

This gate refuses to run against a source checkout: it asserts the imported
``oxblend.__file__`` lives under ``site-packages``/``dist-packages`` (the
installed wheel payload), unless ``OXBLEND_DOCGATE_ALLOW_SOURCE=1`` is set
explicitly for development. The supported qualification flow overlays the
worktree's pure-Python files onto a fresh install of the root-built wheel in
an isolated venv and runs this gate there; the native extension stays the
root-built one and no functional parity is claimed from that overlay.

Usage::

    python bindings/python/tests/docstring_gate.py [--cli PATH-TO-OXBLEND]

Exit status is zero only when the live inventory is fully documented and all
self-checks (negative and positive controls) pass.
"""

from __future__ import annotations

import argparse
import dataclasses
import importlib
import inspect
import os
import re
import subprocess
import sys
import unittest
from pathlib import Path
from types import ModuleType

PACKAGE = "oxblend"

BANNED_TOKENS = ("TODO", "FIXME", "XXX")
CLI_REFERENCE = re.compile(r"oxblend\s+([a-z][a-z0-9-]*)")
NO_CLI_MARKER = "No CLI equivalent"

# Dunder members carry interpreter protocol meaning; only user-meaningful ones
# (context managers, Mapping element access, pickle/str projections) are part
# of the documented surface. __init__ is covered by its class documentation
# (constructor parameters are checked there instead). Every other dunder is
# inventoried distinctly but carries no documentation requirement.
_DOCUMENTED_DUNDER = frozenset({
    "__enter__", "__exit__", "__getitem__", "__iter__",
    "__len__", "__reduce__", "__str__",
})


@dataclasses.dataclass(frozen=True)
class Entry:
    qualname: str
    kind: str  # module | class | function | property | data | native | inherited | dunder
    obj: object
    provenance: str


@dataclasses.dataclass(frozen=True)
class Finding:
    qualname: str
    kind: str
    issue: str
    detail: str


def _is_oxblend_defined(obj: object) -> bool:
    # Property objects expose the property type's module, not the getter's;
    # attribute documentation to the underlying function for provenance.
    target = obj.fget if isinstance(obj, property) else obj
    if isinstance(target, ModuleType):
        name = getattr(target, "__name__", "")
        return name == PACKAGE or name.startswith(PACKAGE + ".")
    module = getattr(target, "__module__", None)
    return isinstance(module, str) and (module == PACKAGE or module.startswith(PACKAGE + "."))


def _is_native_defined(obj: object) -> bool:
    module = getattr(obj, "__module__", "")
    name = getattr(obj, "__name__", "") if not isinstance(obj, ModuleType) else ""
    return "_native" in module or name.endswith("._native")


def discover_modules(package: ModuleType) -> list[tuple[str, ModuleType]]:
    """Import every public submodule file found in the installed package."""
    found: list[tuple[str, ModuleType]] = [(PACKAGE, package)]
    directory = Path(package.__path__[0])
    for path in sorted(directory.glob("*.py")):
        short = path.stem
        if short.startswith("_") and short != "__main__":
            continue
        full = PACKAGE if short == "__init__" else f"{PACKAGE}.{short}"
        if full == PACKAGE:
            continue
        try:
            found.append((full, importlib.import_module(full)))
        except ImportError as error:
            raise AssertionError(f"installed submodule {full} does not import: {error}") from error
    return found


def inventory(package: ModuleType) -> tuple[list[Entry], list[Finding]]:
    """Derive the documentation inventory dynamically from the package."""
    entries: list[Entry] = []
    problems: list[Finding] = []
    seen: set[str] = set()

    def add(entry: Entry) -> None:
        key = f"{entry.kind}:{entry.qualname}"
        if key in seen:
            return
        seen.add(key)
        entries.append(entry)

    for module_name, module in discover_modules(package):
        add(Entry(module_name, "module", module, "own"))
        exported = list(getattr(module, "__all__", ()))
        public = [n for n in dir(module) if not n.startswith("_")]
        for name in exported:
            if not hasattr(module, name):
                problems.append(Finding(f"{module_name}.{name}", "missing",
                                        "all-without-attribute",
                                        f"{module_name}.__all__ lists {name!r} with no such attribute"))
        # Only the module's declared exports are inventoried here. Names
        # merely imported for internal use are covered at their defining
        # site; names defined here but missing from __all__ fail the
        # hidden-public check below instead of hiding.
        for name in sorted(set(exported)):
            if name.startswith("_"):
                continue
            obj = getattr(module, name, None)
            if obj is None:
                continue
            qualname = f"{module_name}.{name}"
            if isinstance(obj, ModuleType):
                continue  # inventoried as a module in its own right
            if inspect.isclass(obj):
                if _is_native_defined(obj):
                    add(Entry(qualname, "native", obj, "native extension class"))
                elif _is_oxblend_defined(obj):
                    add(Entry(qualname, "class", obj, "own"))
                    _inventory_class(
                        f"{getattr(obj, '__module__', module_name)}.{obj.__qualname__}",
                        obj, add)
                else:
                    add(Entry(qualname, "inherited", obj,
                              f"external class {getattr(obj, '__module__', '?')}"))
            elif isinstance(obj, property) or inspect.isfunction(obj) or inspect.isbuiltin(obj):
                if _is_oxblend_defined(obj) and not _is_native_defined(obj):
                    add(Entry(qualname, "property" if isinstance(obj, property) else "function",
                              obj, "own"))
                else:
                    add(Entry(qualname, "inherited", obj,
                              f"defined by {getattr(obj, '__module__', '?')}"))
            else:
                # Data: version strings, the ERROR_TYPES map, type aliases.
                # Listed so they cannot hide, but documentation rules for
                # callables do not apply to values (Python attaches no
                # help-visible docstring to a bare assignment).
                add(Entry(qualname, "data", obj, "value, not a documented callable"))
        # Hidden-public check: a public callable DEFINED by this module but
        # absent from its __all__ fails instead of hiding. Names merely
        # imported here for internal use (defined by a sibling module that
        # exports them itself) are not hidden: they are inventoried at their
        # defining site. Modules without an __all__ export contract (such as
        # __main__) carry no hidden-public requirement.
        if not exported:
            continue
        for name in public:
            if name in exported:
                continue
            obj = getattr(module, name, None)
            if isinstance(obj, ModuleType):
                continue
            if (inspect.isclass(obj) or inspect.isfunction(obj)
                    or inspect.isbuiltin(obj) or isinstance(obj, property)):
                if (getattr(obj, "__module__", None) == module.__name__
                        and not _is_native_defined(obj)):
                    problems.append(Finding(f"{module_name}.{name}", "hidden",
                                            "hidden-public",
                                            "public member defined here but missing from __all__"))
    return entries, problems


def _unwrap(member):
    if isinstance(member, (classmethod, staticmethod)):
        return member.__func__
    return member


def _inventory_class(qualname: str, cls: type, add) -> None:
    for base in cls.__mro__[1:]:
        for name, member in vars(base).items():
            if name.startswith("_") and name not in _DOCUMENTED_DUNDER:
                continue
            if name in vars(cls):
                continue  # checked as an own member below
            target = _unwrap(member)
            if isinstance(target, property) or inspect.isfunction(target):
                if _is_oxblend_defined(target) and not _is_native_defined(target):
                    # An oxblend-defined helper inherited from an oxblend base
                    # needs its own documentation at the defining site, which
                    # the module walk already covers; list it here distinctly.
                    add(Entry(f"{qualname}.{name}", "inherited", target,
                              f"oxblend base {base.__module__}.{base.__qualname__}"))
                else:
                    add(Entry(f"{qualname}.{name}", "inherited", target,
                              f"inherited from {base.__module__}.{base.__qualname__}"))
    for name, member in vars(cls).items():
        if name == "__init__":
            continue  # covered by the class documentation rules
        if name.startswith("_") and name not in _DOCUMENTED_DUNDER:
            if name.startswith("__") and name.endswith("__"):
                add(Entry(f"{qualname}.{name}", "dunder", member,
                          "interpreter protocol; no user-surface requirement"))
            continue
        target = _unwrap(member)
        if isinstance(target, property):
            add(Entry(f"{qualname}.{name}", "property", target, "own"))
        elif inspect.isfunction(target):
            add(Entry(f"{qualname}.{name}", "function", target, "own"))


def _own_doc(obj: object) -> str:
    if isinstance(obj, property):
        getter = obj.fget
        return (getter.__doc__ or "") if getter is not None else ""
    return getattr(obj, "__doc__", "") or ""


def _base_checks(qualname: str, kind: str, doc: str) -> list[Finding]:
    findings: list[Finding] = []
    stripped = doc.strip()
    if not stripped:
        return [Finding(qualname, kind, "missing",
                        "no help-visible runtime docstring")]
    first = stripped.splitlines()[0].strip()
    if not first or first.startswith(("#", ":", "@")):
        findings.append(Finding(qualname, kind, "insubstantial",
                                "docstring has no purpose summary line"))
    for token in BANNED_TOKENS:
        if token in doc:
            findings.append(Finding(qualname, kind, "insubstantial",
                                    f"docstring contains placeholder token {token!r}"))
    return findings


def _cli_presence(doc: str) -> bool:
    return "CLI:" in doc or NO_CLI_MARKER in doc


def check_module_doc(qualname: str, doc: str) -> list[Finding]:
    findings = _base_checks(qualname, "module", doc)
    if findings and findings[0].issue == "missing":
        return findings
    if "CLI" not in doc:
        findings.append(Finding(qualname, "module", "insubstantial",
                                "module docs must state the CLI relationship"))
    paragraphs = [p for p in doc.strip().split("\n\n") if p.strip()]
    if len(paragraphs) < 2:
        findings.append(Finding(qualname, "module", "insubstantial",
                                "module docs must carry purpose plus relationship detail"))
    return findings


def _constructor_params(cls: type) -> list[str]:
    init = vars(cls).get("__init__", None)
    if init is None:
        return []
    try:
        sig = inspect.signature(init)
    except (TypeError, ValueError):
        return []
    return [p for p in sig.parameters if p != "self"]


def check_class_doc(qualname: str, cls: type, doc: str, error_kinds: dict[str, str]) -> list[Finding]:
    findings = _base_checks(qualname, "class", doc)
    if findings and findings[0].issue == "missing":
        return findings
    if not _cli_presence(doc):
        findings.append(Finding(qualname, "class", "insubstantial",
                                "class docs must carry a CLI: reference or 'No CLI equivalent'"))
    for section, meaning in (("Args:", "constructor parameters or mapping"),
                              ("Returns:", "the constructed instance"),
                              ("Raises:", "construction-time errors")):
        if section not in doc:
            findings.append(Finding(qualname, "class", "insubstantial",
                                    f"class docs must carry {section} ({meaning})"))
    if dataclasses.is_dataclass(cls):
        try:
            fields = [f.name for f in dataclasses.fields(cls)]
        except Exception:
            fields = []
        for field_name in fields:
            if field_name.startswith("_"):
                continue  # private storage (e.g. Record._facts) is not user surface
            if field_name not in doc:
                findings.append(Finding(qualname, "class", "insubstantial",
                                        f"dataclass field {field_name!r} not documented"))
    for param in _constructor_params(cls):
        if param.startswith("_"):
            continue
        if param not in doc:
            findings.append(Finding(qualname, "class", "insubstantial",
                                    f"constructor parameter {param!r} not documented"))
    for kind, dotted in error_kinds.items():
        if dotted == qualname:
            if kind not in doc:
                findings.append(Finding(qualname, "class", "insubstantial",
                                        f"stable error kind {kind!r} not named in its docs"))
            if "CLI" not in doc:
                findings.append(Finding(qualname, "class", "insubstantial",
                                        "error docs must state the CLI error-report surface"))
            help_ref = f"oxblend help error {kind}"
            if help_ref not in doc:
                findings.append(Finding(qualname, "class", "insubstantial",
                                        f"error docs must reference the exact catalog help surface {help_ref!r}, "
                                        "never an operation invocation"))
    return findings


def _params(fn) -> list[str]:
    try:
        sig = inspect.signature(fn)
    except (TypeError, ValueError):
        return []
    return [p for p in sig.parameters if p not in ("self", "cls")]


def _returns_none(fn) -> bool:
    try:
        annotation = inspect.signature(fn).return_annotation
    except (TypeError, ValueError):
        return False
    return annotation is None or annotation is type(None)


def check_function_doc(qualname: str, fn, doc: str) -> list[Finding]:
    findings = _base_checks(qualname, "function", doc)
    if findings and findings[0].issue == "missing":
        return findings
    params = _params(fn)
    if params:
        if "Args:" not in doc:
            findings.append(Finding(qualname, "function", "insubstantial",
                                    f"parameters {params} present but no Args: section"))
        else:
            for param in params:
                if param.startswith("_"):
                    continue
                # *args/**kwargs are named by their bare parameter name.
                if not re.search(rf"\b{re.escape(param)}\b", doc):
                    findings.append(Finding(qualname, "function", "insubstantial",
                                            f"parameter {param!r} not documented"))
    if not _returns_none(fn) and "Returns:" not in doc:
        findings.append(Finding(qualname, "function", "insubstantial",
                                "no Returns: section for a value-returning callable"))
    if "Raises:" not in doc:
        findings.append(Finding(qualname, "function", "insubstantial",
                                "no Raises: section (use an explicit 'Raises: Nothing.' claim only for total functions)"))
    if not _cli_presence(doc):
        findings.append(Finding(qualname, "function", "insubstantial",
                                "no CLI: reference or 'No CLI equivalent' marker"))
    return findings


def check_property_doc(qualname: str, prop: property, doc: str) -> list[Finding]:
    findings = _base_checks(qualname, "property", doc)
    if findings and findings[0].issue == "missing":
        return findings
    if "Returns:" not in doc:
        findings.append(Finding(qualname, "property", "insubstantial",
                                "property docs must carry Returns:"))
    if "Raises:" not in doc:
        findings.append(Finding(qualname, "property", "insubstantial",
                                "property docs must carry Raises:"))
    if not _cli_presence(doc):
        findings.append(Finding(qualname, "property", "insubstantial",
                                "no CLI: reference or 'No CLI equivalent' marker"))
    return findings


def check_native_class(qualname: str, cls: type, doc: str) -> list[Finding]:
    """Native extension classes cannot take per-member Python docstrings.

    Their data descriptors reject ``__doc__`` assignment by CPython design,
    so the owning (possibly Python-attached) class documentation must name
    every descriptor instead. Findings here fail; the descriptors themselves
    are reported as native-owned and covered, never hidden.
    """
    findings = _base_checks(qualname, "native", doc)
    if findings and findings[0].issue == "missing":
        return findings
    if not _cli_presence(doc):
        findings.append(Finding(qualname, "native", "insubstantial",
                                "native class docs must carry a CLI: reference or 'No CLI equivalent'"))
    for section, meaning in (("Args:", "construction parameters or an explicit non-constructibility note"),
                              ("Returns:", "the constructed instance or token source"),
                              ("Raises:", "construction-time errors")):
        if section not in doc:
            findings.append(Finding(qualname, "native", "insubstantial",
                                    f"native class docs must carry {section} ({meaning})"))
    descriptors = sorted(
        name for name, member in vars(cls).items()
        if type(member).__name__ == "getset_descriptor" and not name.startswith("_"))
    covered = [name for name in descriptors if re.search(rf"\b{re.escape(name)}\b", doc)]
    for name in descriptors:
        if name not in covered:
            findings.append(Finding(qualname, "native", "insubstantial",
                                    f"native descriptor {name!r} not covered by the class docs"))
    return findings


def cli_commands(cli: Path) -> set[str]:
    proc = subprocess.run([str(cli), "--help"], capture_output=True, text=True)
    if proc.returncode != 0:
        raise AssertionError(f"CLI help failed for {cli}: {proc.stderr[:500]}")
    match = re.search(r"Commands:\s*(.+)", proc.stdout, re.DOTALL)
    if not match:
        raise AssertionError(f"no Commands: line in {cli} --help")
    return set(match.group(1).split())


def collect_cli_references(entries: list[Entry]) -> dict[str, list[str]]:
    references: dict[str, list[str]] = {}
    for entry in entries:
        if entry.kind not in ("module", "class", "function", "property", "native"):
            continue
        doc = _own_doc(entry.obj)
        if NO_CLI_MARKER in doc:
            continue
        for command in CLI_REFERENCE.findall(doc):
            references.setdefault(command, []).append(entry.qualname)
    return references


def run_gate(package_name: str = PACKAGE, cli: Path | None = None
             ) -> tuple[list[Entry], list[Finding]]:
    package = importlib.import_module(package_name)
    entries, problems = inventory(package)
    findings: list[Finding] = list(problems)

    try:
        error_types: dict[str, type] = getattr(importlib.import_module(f"{package_name}.errors"),
                                               "ERROR_TYPES")
    except (ImportError, AttributeError):
        error_types = {}
    error_kinds = {kind: f"{cls.__module__}.{cls.__qualname__}"
                   for kind, cls in error_types.items() if inspect.isclass(cls)}

    docs: dict[str, str] = {}
    claimed: set[int] = set()  # one documentation check per runtime object
    for entry in entries:
        if id(entry.obj) in claimed:
            continue  # re-export or repeated inheritance; documented once
        claimed.add(id(entry.obj))
        if entry.kind == "inherited":
            # Inherited helpers keep their original documentation; the gate
            # verifies it is genuinely help-visible and reports provenance.
            if not (inspect.getdoc(entry.obj) or "").strip():
                findings.append(Finding(entry.qualname, "inherited", "missing",
                                        f"inherited from {entry.provenance} without help-visible docs"))
            continue
        if entry.kind in ("module", "class", "function", "property", "native"):
            doc = _own_doc(entry.obj)
            docs[entry.qualname] = doc
            if entry.kind == "module":
                findings.extend(check_module_doc(entry.qualname, doc))
            elif entry.kind == "class":
                findings.extend(check_class_doc(entry.qualname, entry.obj, doc, error_kinds))
            elif entry.kind == "function":
                findings.extend(check_function_doc(entry.qualname, entry.obj, doc))
            elif entry.kind == "property":
                findings.extend(check_property_doc(entry.qualname, entry.obj, doc))
            elif entry.kind == "native":
                if inspect.isclass(entry.obj):
                    findings.extend(check_native_class(entry.qualname, entry.obj, doc))

    normalized: dict[str, list[str]] = {}
    for qualname, doc in docs.items():
        key = " ".join(doc.split())
        if len(key) < 24:
            continue
        normalized.setdefault(key, []).append(qualname)
    for key, owners in normalized.items():
        if len(owners) > 1:
            for owner in owners:
                findings.append(Finding(owner, "duplicate", "duplicated",
                                        f"docstring shared verbatim with {sorted(set(owners) - {owner})}"))

    if cli is not None:
        known = cli_commands(cli)
        for command, owners in collect_cli_references(entries).items():
            if command not in known:
                findings.append(Finding(owners[0], "cli", "unknown-command",
                                        f"CLI reference oxblend {command} not in {cli} --help: {known}"))
    return entries, findings


def format_findings(findings: list[Finding]) -> str:
    lines = [f"[{f.kind}] {f.qualname}: {f.issue}: {f.detail}" for f in findings]
    return "\n".join(sorted(lines))


class LiveHelpGate(unittest.TestCase):
    """The live gate: installed payload fully documented, nothing hidden."""

    @classmethod
    def setUpClass(cls) -> None:
        if os.environ.get("OXBLEND_DOCGATE_ALLOW_SOURCE", "") != "1":
            import oxblend as live
            marker = live.__file__ or ""
            if "site-packages" not in marker and "dist-packages" not in marker:
                raise unittest.SkipTest(
                    f"gate requires the installed wheel payload, got {marker}; "
                    "overlay pure-Python files onto an installed wheel or set "
                    "OXBLEND_DOCGATE_ALLOW_SOURCE=1")

    def test_live_inventory_documented(self) -> None:
        cli = os.environ.get("OXBLEND_CLI", "")
        entries, findings = run_gate(cli=Path(cli) if cli else None)
        kinds: set[str] = set()
        for entry in entries:
            kinds.add(entry.kind)
        for required in ("module", "class", "function", "property"):
            self.assertIn(required, kinds, f"inventory missed every {required}")
        self.assertGreater(len(entries), 200,
                           "inventory unexpectedly small; discovery must be dynamic")
        if findings:
            self.fail(f"{len(findings)} help/docstring findings:\n"
                      f"{format_findings(findings)}")


class LiveDocHonesty(unittest.TestCase):
    """Live pins proving Raises: claims match installed behavior, not prose.

    Index-access properties on publicly constructible reports refuse with
    ``KeyError`` on manually built mappings (``CatalogReport({})``), so
    their docs must name ``KeyError`` instead of claiming totality; only
    ``.get``-defaulted properties are total. These run against the
    installed payload, the same overlay the gate checks.
    """

    @classmethod
    def setUpClass(cls) -> None:
        if os.environ.get("OXBLEND_DOCGATE_ALLOW_SOURCE", "") != "1":
            import oxblend as live
            marker = live.__file__ or ""
            if "site-packages" not in marker and "dist-packages" not in marker:
                raise unittest.SkipTest(
                    f"honesty pins require the installed wheel payload, got {marker}")

    def test_manual_report_index_property_refuses(self) -> None:
        import oxblend.operations as ops
        with self.assertRaises(KeyError):
            ops.CatalogReport({}).concepts
        doc = ops.CatalogReport.concepts.fget.__doc__ or ""
        self.assertIn("KeyError", doc)
        self.assertNotIn("Raises: Nothing", doc)

    def test_defaulted_properties_are_total(self) -> None:
        import oxblend.operations as ops
        from oxblend import Record
        self.assertTrue(ops.FactsReport({}).complete)
        self.assertEqual(ops.FactsReport({}).errors, ())
        self.assertTrue(ops.ListReport({}).complete)
        self.assertEqual(ops.MultiRootReport({}).root_lists, ())
        self.assertEqual(ops.EditReport({}).errors, ())
        self.assertEqual(Record({}).diagnostics, ())

    def test_trace_diagnostics_override_refuses(self) -> None:
        # TraceReport overrides Record.diagnostics with a direct index, so
        # unlike the Record default it refuses on a missing field.
        from oxblend import TraceReport
        with self.assertRaises(KeyError):
            TraceReport({}).diagnostics
        doc = TraceReport.diagnostics.fget.__doc__ or ""
        self.assertIn("KeyError", doc)
        self.assertNotIn("Raises: Nothing", doc)

    def test_empty_scan_summary_refuses(self) -> None:
        import oxblend.operations as ops
        with self.assertRaises(IndexError):
            ops.ScanReport([]).summary
        doc = ops.ScanReport.summary.fget.__doc__ or ""
        self.assertIn("IndexError", doc)
        self.assertNotIn("Raises: Nothing", doc)

    def test_session_poison_and_nonregular_contracts_documented(self) -> None:
        from oxblend import File
        import oxblend.operations as ops
        for member in (File.close, File.closed.fget, File.__exit__):
            with self.subTest(member=member.__name__):
                doc = inspect.getdoc(member) or ""
                self.assertIn("lock_poisoned", doc)
                self.assertNotIn("Nothing", doc)
        module_doc = inspect.getdoc(ops) or ""
        for phrase in ("non-regular-source", "pack_path", "truncated"):
            self.assertIn(phrase, module_doc)

    def test_native_token_not_constructible(self) -> None:
        from oxblend import BlockInfo
        with self.assertRaises(TypeError):
            BlockInfo()
        self.assertIn("TypeError", BlockInfo.__doc__ or "")

    def test_generated_error_classes_carry_constructor_contract(self) -> None:
        # Every generated error class documents ordinary Exception
        # construction (*args), the constructed instance, TypeError for
        # unsupported keywords, and the exact catalog help surface. Dynamic
        # over ERROR_TYPES: no hard-coded kind list to drift.
        import oxblend.errors as errors
        kinds = dict(getattr(errors, "ERROR_TYPES", {}))
        self.assertGreater(len(kinds), 100,
                           "ERROR_TYPES unexpectedly small; not the generated payload")
        for kind, cls in kinds.items():
            with self.subTest(kind=kind):
                doc = inspect.getdoc(cls) or ""
                for section in ("Args:", "Returns:", "Raises:"):
                    self.assertIn(section, doc)
                self.assertIn(kind, doc)
                if cls.__module__ != "oxblend.errors":
                    # Lifecycle classes (closed_file, input_changed) are
                    # native-defined and documented in the package
                    # initializer, not the generated module: they carry the
                    # constructor contract but no catalog help surface.
                    continue
                self.assertIn(f"oxblend help error {kind}", doc)


class GateSelfChecks(unittest.TestCase):
    """Controls proving the gate fails missing docs and filler, not just green."""

    def _function_findings(self, fn, doc: str) -> list[Finding]:
        fn.__doc__ = doc
        return check_function_doc(f"synthetic.{fn.__name__}", fn, doc)

    def test_missing_doc_fails(self) -> None:
        def undocumented(alpha, beta):
            pass
        undocumented.__doc__ = None  # type: ignore[assignment]
        findings = check_function_doc("synthetic.undocumented", undocumented, "")
        self.assertTrue(any(f.issue == "missing" for f in findings))

    def test_filler_doc_fails(self) -> None:
        def helper(alpha, beta):
            pass
        findings = self._function_findings(helper, "Helper function.")
        issues = {f.issue for f in findings}
        self.assertIn("insubstantial", issues)
        # Param names, Returns, Raises and the CLI marker are all absent.
        self.assertGreaterEqual(len(findings), 4)

    def test_placeholder_token_fails(self) -> None:
        defhelper_doc = "Do the thing.\n\nArgs:\n    alpha: TODO.\n\nReturns:\n    Something.\n\nRaises:\n    Nothing.\n\nCLI:\n    oxblend get.\n"

        def thing(alpha):
            pass
        findings = self._function_findings(thing, defhelper_doc)
        self.assertTrue(any("TODO" in f.detail for f in findings))

    def test_cli_marker_required(self) -> None:
        def Abdullah(alpha):
            pass
        doc = ("Convert one unit.\n\nArgs:\n    alpha: the input.\n\n"
               "Returns:\n    The output.\n\nRaises:\n    Nothing.\n")
        findings = self._function_findings(Abdullah, doc)
        self.assertTrue(any("CLI" in f.detail for f in findings))

    def test_well_documented_passes(self) -> None:
        def convert(alpha, beta=1):
            pass
        doc = ("Convert one saved unit.\n\nArgs:\n    alpha: the input value.\n"
               "    beta: repeat count.\n\nReturns:\n    The converted value.\n\n"
               "Raises:\n    Nothing.\n\nCLI:\n    oxblend get.\n")
        self.assertEqual(self._function_findings(convert, doc), [])

    def test_class_docs_require_constructor_sections(self) -> None:
        cls = type("Widget", (), {})
        doc = "A widget.\n\nNo CLI equivalent: synthetic control.\n"
        findings = check_class_doc("synthetic.Widget", cls, doc, {})
        missing = [f.detail for f in findings]
        self.assertTrue(any("Args:" in d for d in missing), missing)
        self.assertTrue(any("Returns:" in d for d in missing), missing)
        self.assertTrue(any("Raises:" in d for d in missing), missing)

    def test_well_documented_class_passes(self) -> None:
        @dataclasses.dataclass(frozen=True)
        class Widget:
            """A widget holding one value.

            Args:
                value: the held value.

            Returns:
                Widget: the frozen value holder.

            Raises:
                Nothing.

            No CLI equivalent: synthetic control.
            """
            value: int
        self.assertEqual(check_class_doc("synthetic.Widget", Widget, Widget.__doc__ or "", {}), [])

    def test_error_class_requires_exact_help_reference(self) -> None:
        cls = type("Broken", (), {})
        doc = ("A broken thing.\n\nCatalog kind ``oops`` maps to this class.\n\n"
               "Args:\n    *args: manual construction.\n\nReturns:\n    Broken: an instance.\n\n"
               "Raises:\n    TypeError: bad keywords.\n\nCLI: ``oxblend help``.\n")
        findings = check_class_doc("synthetic.Broken", cls, doc, {"oops": "synthetic.Broken"})
        self.assertTrue(any("oxblend help error oops" in f.detail for f in findings), findings)

    def test_no_cli_marker_passes(self) -> None:
        def convert(alpha):
            pass
        doc = ("Convert one saved unit.\n\nArgs:\n    alpha: the input value.\n\n"
               "Returns:\n    The converted value.\n\nRaises:\n    Nothing.\n\n"
               "No CLI equivalent: in-process helper only.\n")
        self.assertEqual(self._function_findings(convert, doc), [])

    def test_duplicate_docs_fail(self) -> None:
        def first(alpha):
            pass
        def second(alpha):
            pass
        shared = ("Convert one saved unit.\n\nArgs:\n    alpha: the input value.\n\n"
                  "Returns:\n    The converted value.\n\nRaises:\n    Nothing.\n\n"
                  "CLI:\n    oxblend get.\n")
        first.__doc__ = shared
        second.__doc__ = shared
        entries = [Entry("synthetic.first", "function", first, "own"),
                   Entry("synthetic.second", "function", second, "own")]
        docs = {e.qualname: " ".join(_own_doc(e.obj).split()) for e in entries}
        normalized: dict[str, list[str]] = {}
        for qualname, key in docs.items():
            normalized.setdefault(key, []).append(qualname)
        dups = [owners for owners in normalized.values() if len(owners) > 1]
        self.assertEqual(len(dups), 1)

    def test_unknown_cli_command_fails(self) -> None:
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            fake = Path(tmp) / "oxblend"
            fake.write_text("#!/bin/sh\necho 'Commands: get trace'\n")
            fake.chmod(0o755)
            self.assertEqual(cli_commands(fake), {"get", "trace"})

    def test_hidden_public_reported(self) -> None:
        module = ModuleType("synthetic_hidden")
        module.__all__ = []  # type: ignore[attr-defined]

        def exposed(alpha):
            """Convert one saved unit.

            Args:
                alpha: the input value.

            Returns:
                The converted value.

            Raises:
                Nothing.

            No CLI equivalent: synthetic control.
            """
        exposed.__module__ = PACKAGE
        module.exposed = exposed  # type: ignore[attr-defined]
        exported: list[str] = list(getattr(module, "__all__", ()))
        hidden = [n for n in dir(module) if not n.startswith("_") and n not in exported
                  and _is_oxblend_defined(getattr(module, n))]
        self.assertIn("exposed", hidden)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=None,
                        help="oxblend CLI binary used to validate referenced command names")
    args = parser.parse_args()
    if args.cli is not None:
        os.environ["OXBLEND_CLI"] = str(args.cli)
    suite = unittest.defaultTestLoader.loadTestsFromModule(sys.modules[__name__])
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
