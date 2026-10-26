# Python runtime help/docstring gate (R41)

`bindings/python/tests/docstring_gate.py` is the runtime documentation gate
for the installed `oxblend` wheel payload. It runs without compiling anything
and without opening any `.blend` file: it imports the INSTALLED package and
checks that `help()` shows meaningful documentation for every public name.

## What it enforces

Every public function, class and method reachable from the installed package
— including generated error classes, generated record classes and public
submodules — must carry help-visible runtime documentation covering purpose,
arguments, return value, raised errors and the matching `oxblend` CLI command
(or an explicit `No CLI equivalent` statement with the reason).

The inventory is derived dynamically from the installed package, never from a
hand-maintained name list, so a newly added public name cannot hide:

- submodules: every non-private `*.py` module file in the installed package
  directory, plus `__main__` (the `python -m oxblend` entry point);
- members: the union of each module's `__all__` with every public `dir()`
  name whose object is defined by `oxblend` itself. A public oxblend-defined
  name missing from `__all__` is reported as hidden, not silently skipped;
- class members: every public name in the class `__dict__`, plus inherited
  names. Inherited helpers are DISTINGUISHED, never silently excused:
  members inherited from outside `oxblend` (`collections.abc.Mapping`,
  `Exception`, the native extension) keep their original documentation and
  are reported with provenance; members defined by `oxblend` need their own
  documentation even when they override an inherited name. `TraceReport`
  and `PackReport` override `Record.diagnostics` with a direct index, so
  their docs must name `KeyError` where the `Record` default documents
  totality: the override is checked at its own site, never inherited.
- native-owned data descriptors (the `oxblend._native` getset descriptors,
  whose `__doc__` is not writable from Python by CPython design) are covered
  by their owning class documentation: every descriptor name must be named
  there. They are reported as native-owned and covered, never as missing.

Structural substance rules (so generic filler such as "Helper function" or
"TODO" cannot pass): functions document every parameter under `Args:`,
carry `Returns:` unless annotated `-> None`, always carry `Raises:` (the
literal `Raises: Nothing.` is accepted only for total functions), and carry
either a `CLI:` command reference or the explicit `No CLI equivalent`
marker. Classes document dataclass fields, constructor parameters under
`Args:` (the construction mapping or parameters, even when inherited from
`Record`; an explicit non-constructibility note for native tokens that only
sessions issue), the constructed instance under `Returns:`, and
construction-time errors under `Raises:` — a missing section fails, so a
generic filler class doc cannot pass. Error classes additionally name
their stable `kind` and reference the exact catalog help surface
`oxblend help error KIND` (catalog presence never promises that a Python
operation can raise the class); generated error classes document ordinary
`Exception` construction (`Args: *args`), the instance (`Returns:`) and
`TypeError` for unsupported keywords (`Raises:`). Properties carry
`Returns:` and `Raises:` like methods, but `Raises: Nothing.` is accepted
only for provably total properties (`.get` defaults and other
non-indexing bodies): every `self[...]` indexing property must name
`KeyError`, and sequence-end properties must name `IndexError`. Modules
document purpose and the CLI relationship. No two documented objects share
verbatim text.

## Running it

The gate refuses a source checkout: it asserts the imported `oxblend`
lives under `site-packages`/`dist-packages` (override with
`OXBLEND_DOCGATE_ALLOW_SOURCE=1` for development only).

Qualification flow (pure-Python overlay, no compile): install the
root-built wheel into an isolated venv, overlay the worktree's
`oxblend/*.py` runtime files onto the installed payload (the native
extension stays root-built), then run:

```sh
python bindings/python/tests/docstring_gate.py --cli /path/to/oxblend
```

With `--cli`, every `oxblend <command>` reference found in documentation is
validated against that CLI binary's `Commands:` help line. Without it, the
gate still enforces the `CLI:`/`No CLI equivalent` presence rule and the
reference form. Exit status is zero only when the live inventory is fully
documented AND all self-checks pass: a missing-doc negative, a filler-doc
negative, a placeholder-token negative, a missing-CLI-marker negative, a
duplicate-docs check, an unknown-CLI-command check (against a fake binary),
a hidden-public check, class-constructor-section negatives (missing `Args:`
/ `Returns:` / `Raises:` fail), an exact-error-help-reference negative, and
a well-formed class positive — plus live honesty pins proving `Raises:`
claims against installed behavior: a manually built `CatalogReport({})`
refuses `KeyError` on index properties (docs must name it, never claim
totality), `.get`-defaulted properties are total, the `TraceReport`
`diagnostics` override refuses where the `Record` default does not, an
empty `ScanReport` refuses `IndexError` on `summary`, `BlockInfo()` refuses
`TypeError`, and every generated error class carries the constructor
contract sampled dynamically over `ERROR_TYPES` (no hard-coded kind list).
