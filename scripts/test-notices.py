#!/usr/bin/env python3
"""Licence output must not depend on an unbuilt target crate in Cargo's cache."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
HOME = Path(os.environ.get('CARGO_HOME', Path.home()/'.cargo'))
CARGO = shutil.which('cargo')
with tempfile.TemporaryDirectory(prefix='oxblend-notices-') as temp:
    temp = Path(temp)
    sources = temp/'registry/src/test'
    sources.mkdir(parents=True)
    for source_root in sorted((HOME/'registry/src').iterdir()):
        for crate in source_root.iterdir():
            if crate.name != 'r-efi-6.0.0' and not (sources/crate.name).exists():
                (sources/crate.name).symlink_to(crate)
    # The graph is intentionally identical in both cases. Only the notices
    # source-cache lookup varies; cargo still reads its real locked cache.
    wrapper = temp/'bin'
    wrapper.mkdir()
    cargo = wrapper/'cargo'
    cargo.write_text('#!/bin/sh\nCARGO_HOME="$NOTICE_REAL_HOME" exec "$NOTICE_REAL_CARGO" "$@"\n')
    cargo.chmod(0o755)
    env = dict(os.environ, CARGO_HOME=str(temp), PATH=str(wrapper)+os.pathsep+os.environ['PATH'],
               NOTICE_REAL_HOME=str(HOME), NOTICE_REAL_CARGO=CARGO)
    before = subprocess.check_output([str(ROOT/'scripts/third-party-notices')], cwd=ROOT, env=env)
    extra = sources/'r-efi-6.0.0'
    extra.mkdir()
    (extra/'Cargo.toml').write_text('[package]\nname = "r-efi"\nversion = "6.0.0"\nlicense = "MIT"\n')
    (extra/'LICENSE').write_text('Synthetic unbuilt-cache marker, never distributed.\n')
    after = subprocess.check_output([str(ROOT/'scripts/third-party-notices')], cwd=ROOT, env=env)
    assert before == after, 'notices changed merely because an unbuilt r-efi appeared in the cache'
print('NOTICES PASS: identical locked built graph, with and without unbuilt cached r-efi')
