"""Installed Session provider proof using the native synthetic linked fixture.

Build it with scripts/build-query-linked-fixture.py in bundled Blender first.
No CLI/subprocess, mutable provider callback, hidden retry or reader clone here.
All mutations affect test-owned copies. FLATTRACE_LEVEL=2 proves one capture.
"""
from __future__ import annotations
import argparse
from collections.abc import Mapping
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import oxblend as ox

p=argparse.ArgumentParser(); p.add_argument('--fixture',type=Path,required=True); p.add_argument('--evidence',type=Path,required=True); a=p.parse_args()
a.evidence.mkdir(parents=True,exist_ok=True)
inputs=a.evidence/'inputs'; shutil.copytree(a.fixture.parent,inputs)
source=inputs/a.fixture.name; library=inputs/'group-library.blend'
original=library.read_bytes()
def thaw(value):
 if isinstance(value,Mapping): return {key:thaw(item) for key,item in value.items()}
 if isinstance(value,tuple): return [thaw(item) for item in value]
 return value

with ox.open(source) as file:
 first=file.query(type='Scene',fields=['compositor'])
 assert first.complete
 observations=first['linked_subtype_observations']
 local=[entry for entry in observations if entry.get('selected_from')=='library_local']
 assert local and all(entry['provider']['owner']=='session_retained_library_owner' for entry in local)
 assert not any(entry.get('selected_from')=='library_stub' for entry in observations), 'local-first selection regressed'
 assert all(entry['provider'].raw('path')==os.fsencode(library) for entry in local)
 second=file.query(type='Scene',fields=['compositor'])
 assert second==first
 before=file.summary()
 with library.open('ab') as output: output.write(b'controlled library source change')
 errors=[]
 for restore in (False,True):
  if restore: library.write_bytes(original)
  try: file.query(type='Scene',fields=['compositor'],allow_incomplete=True)
  except ox.SourceChangedError as error:
   assert error.kind=='input_changed'
   assert error.report is None and error.effects=='none'
   assert any(entry['kind']=='source_check_failure' and entry.raw('source')==os.fsencode(library) for entry in error.diagnostics)
   errors.append({'kind':error.kind,'diagnostics':thaw(error.diagnostics),'restored':restore})
  else: raise AssertionError('changed captured library was silently accepted/refreshed')
 # A stale unrelated library must not disable independent root-only reads.
 assert file.summary()==before
 assert file.query(type='Scene',fields=['r.sfra']).complete
assert file.closed
assert first.complete and first.rows  # Owned observations survive close.

# A FAILED first-use capture is operation-local, not a permanent negative
# cache. Restoring an initially absent library is a deliberate later call,
# not an automatic replay inside the earlier operation.
missing=a.evidence/'initial-missing'; missing.mkdir()
missing_root=missing/a.fixture.name; shutil.copyfile(a.fixture,missing_root)
with ox.open(missing_root) as file:
 unavailable=file.query(type='Scene',fields=['compositor'],allow_incomplete=True)
 assert not unavailable.complete
 assert any(error['kind']=='missing_library' for error in unavailable['linked_subtype_diagnostics'])
 assert unavailable.rows[0]['values']['compositor']['partial_value']['trees']
 shutil.copyfile(a.fixture.parent/'group-library.blend',missing/'group-library.blend')
 available=file.query(type='Scene',fields=['compositor'])
 assert available.complete

receipt={'status':'PASS','python':sys.version,'module':ox.__file__,'native':ox._native.__file__,
 'native_sha256':hashlib.sha256(Path(ox._native.__file__).read_bytes()).hexdigest(),
 'binding':ox.__version__,'engine':ox.engine_version,'root':str(source),'library':str(library),
 'library_original_sha256':hashlib.sha256(original).hexdigest(),
 'first_query':thaw(first),'latched_errors':errors,'initially_missing':thaw(unavailable),
 'first_success_after_restore':thaw(available)}
(a.evidence/'linked-session-result.json').write_text(json.dumps(receipt,indent=2))
print(json.dumps({'status':'PASS','root':str(source),'library':str(library),'library_owner':local[0]['provider']['owner']}))
