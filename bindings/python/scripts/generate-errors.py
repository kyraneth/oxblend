from pathlib import Path
import json, builtins
root=Path(__file__).resolve().parents[3]
entries=json.loads((root/'docs/error-catalog.json').read_text())['entries']
s='''"""Stable exact-code exceptions generated from the engine error catalog.

Regenerate with bindings/python/scripts/generate-errors.py. Never match messages.

Every class below documents its stable ``kind`` (see ``ERROR_TYPES``) and the
CLI catalog-help surface; ``help()`` on the installed package is the runtime
reference. Catalog presence does not imply terminal reachability: a kind may
only occur inside a report or in CLI-only/internal sites. The native base and
lifecycle exceptions receive runtime documentation in the package initializer.
"""
from __future__ import annotations
from ._native import OxblendError, ClosedFileError, SourceChangedError

'''
names={}
for e in sorted(entries,key=lambda e:e['kind']):
 k=e['kind']; n=''.join(x.title() for x in k.split('_'))+'Error'
 if k=='input_changed': n='SourceChangedError'
 elif n in ('OxblendError', 'ClosedFileError', 'SourceChangedError') or hasattr(builtins,n): n='Oxblend'+n
 names[k]=n
 if n=='SourceChangedError': continue
 explanation=e["explanation"].replace(chr(34),chr(39))
 s+=(f'class {n}(OxblendError):\n    """{explanation}\n\n'
      f'    Catalog kind ``{k}`` maps to this exception class in ERROR_TYPES.\n'
      f'    Some catalog kinds are report-only, CLI-only or internal: this mapping\n'
      f'    does not promise that a Python operation can raise every class.\n'
      f'    Engine-emitted errors retain canonical fields and any partial report;\n'
      f'    matching this class or error.kind is safer than matching message text.\n\n'
      f'    Args:\n'
      f'        *args: Ordinary Python Exception arguments for manual construction.\n'
      f'            Constructing an exception does not execute the engine, assign\n'
      f'            canonical fields, or prove that this failure occurred.\n\n'
      f'    Returns:\n'
      f'        {n}: An exception instance retaining its positional args.\n\n'
      f'    Raises:\n'
      f'        TypeError: Unsupported keyword arguments to the inherited constructor.\n\n'
      f'    CLI: ``oxblend help error {k}`` describes this exact catalog kind\n'
      f'    and its likely causes/remedies; this is not an operation invocation.\n'
      f'    """\n\n')
s+='ERROR_TYPES: dict[str, type[OxblendError]] = {\n'
for k,n in names.items(): s+=f'    {k!r}: {n},\n'
s+="    'closed_file': ClosedFileError,\n}\n"
s+='\n__all__ = ["ERROR_TYPES", "OxblendError", "ClosedFileError", "SourceChangedError", '+', '.join(repr(n) for n in names.values() if n!='SourceChangedError')+']\n'
(root/'bindings/python/python/oxblend/errors.py').write_text(s)
