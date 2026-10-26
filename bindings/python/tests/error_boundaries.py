#!/usr/bin/env python3
'''R35/R36/R38 boundary pins: output_path canonical + native arg domain + shared-op negatives.

R35: exr-pixels output_path trailing separator must carry the shared
AtomicOutput prefix exactly as the CLI emits it, on both surfaces.
Other AtomicOutput variants (parent-not-dir, existing destination) keep the
same prefix. No stage is created and no original is overwritten.

R36: PyO3 pre-engine conversion errors (blocks address -1, 2**64, wrong
type) are Python-native TypeError/OverflowError, outside the canonical
contract. In-range invalid addresses still raise the canonical engine error.

R38: shared operation negatives keep the exact CLI command kind/message.
summary/inspect of a directory refuse io / Is a directory (os error 21)
on both surfaces. pack of an existing invalid blend keeps the full pack
prefix over snapshot/index on both (Session open INSIDE the engine pack
call). Missing pack directory stays pack_input, missing project stays
pack_project (gate order before capture); the captured File input-change
guard keeps its pack prefix.

Safe: all outputs live under a fresh work dir; inputs are read-only fixtures.
CLI diagnostic_ref is bound (run_id/failure_id) while native is None; only
that asymmetry is allowed, with actual receipt binding checked separately.
'''
from __future__ import annotations
import json
import os
import struct
import subprocess
import sys
from pathlib import Path


def build_exr(path: Path) -> None:
    Z = bytes([0])
    def attr(n, t, p):
        return n + Z + t + Z + struct.pack('<i', len(p)) + p
    o = bytearray(struct.pack('<II', 20000630, 2))
    ch = b'R' + Z + struct.pack('<i', 2) + bytes([0, 0, 0, 0]) + struct.pack('<ii', 1, 1) + Z
    o += attr(b'channels', b'chlist', ch)
    o += attr(b'compression', b'compression', Z)
    w = struct.pack('<iiii', 0, 0, 0, 0)
    o += attr(b'dataWindow', b'box2i', w)
    o += attr(b'displayWindow', b'box2i', w)
    o += attr(b'lineOrder', b'lineOrder', Z)
    o += attr(b'pixelAspectRatio', b'float', struct.pack('<f', 1.0))
    o += attr(b'screenWindowCenter', b'v2f', struct.pack('<ff', 0.0, 0.0))
    o += attr(b'screenWindowWidth', b'float', struct.pack('<f', 1.0))
    o += Z
    h = bytes(o)
    off = len(h) + 8
    chunk = struct.pack('<ii', 0, 4) + struct.pack('<f', 1.0)
    path.write_bytes(h + struct.pack('<Q', off) + chunk)


def cli_envelope(stderr: bytes):
    for line in reversed(stderr.splitlines()):
        if b'"error"' not in line or b'"diagnostic"' not in line:
            continue
        try:
            doc = json.loads(line)
        except Exception:
            continue
        if isinstance(doc, dict) and isinstance(doc.get('error'), dict):
            return doc
    return None


def main() -> int:
    if len(sys.argv) < 3:
        print('usage: error_boundaries.py CLI WORK', file=sys.stderr)
        return 2
    cli = Path(sys.argv[1])
    work = Path(sys.argv[2])
    work.mkdir(parents=True, exist_ok=True)
    import oxblend
    failures: list[str] = []
    # Build valid EXR input, read-only after.
    valid = work / 'valid.exr'
    if not valid.exists():
        build_exr(valid)
    before = valid.read_bytes()
    # R35: trailing separator.
    out_trailing = str(work / 'outdir') + '/'
    trace_cli = work / 'traces-cli'
    trace_cli.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, FLATTRACE_DIR=str(trace_cli))
    proc = subprocess.run([str(cli), 'exr-pixels', str(valid), '--part', '0', '--pass', '', '--output', out_trailing],
                          cwd=str(work), stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    if proc.returncode != 2:
        failures.append('CLI trailing exit %r != 2' % (proc.returncode,))
    doc = cli_envelope(proc.stderr)
    if doc is None:
        failures.append('CLI trailing no envelope')
        cli_kind = cli_msg = None
    else:
        cli_kind = doc['error'].get('kind')
        cli_msg = doc['error'].get('message')
        if cli_kind != 'output_path':
            failures.append('CLI trailing kind %r != output_path' % (cli_kind,))
        want = 'create output stage %r: file output has a trailing separator' % (out_trailing,)
        # CLI displays path without repr quotes: check prefix + base.
        if cli_msg != want:
            failures.append('CLI trailing message mismatch: %r != %r' % (cli_msg, want))
    try:
        oxblend.exr_pixels(str(valid), out_trailing, part=0, pass_name='')
        failures.append('PY trailing unexpectedly succeeded')
        py_kind = py_msg = None
    except Exception as e:
        py_kind = getattr(e, 'kind', None)
        py_msg = getattr(e, 'message', None)
        if py_kind != 'output_path':
            failures.append('PY trailing kind %r != output_path' % (py_kind,))
        if py_msg != cli_msg:
            failures.append('PY trailing message %r != CLI %r' % (py_msg, cli_msg))
        # Diagnostic preservation: native ref is None, CLI bound.
        if getattr(e, 'diagnostic_ref', 'missing') is not None:
            failures.append('PY trailing diagnostic_ref should be None, got %r' % (getattr(e, 'diagnostic_ref', None),))
    # No stage created, input unchanged, no output clobber.
    if valid.read_bytes() != before:
        failures.append('valid.exr changed')
    if (work / 'outdir').exists() and not (work / 'outdir').is_dir():
        failures.append('outdir unexpectedly a file')
    # Count entries: valid.exr only plus traces dir, no staging names.
    names = sorted(p.name for p in work.iterdir())
    staged = [n for n in names if n.startswith('.oxblend-stage-')]
    if staged:
        failures.append('staging names left: %r' % (staged,))
    # R35 other variants: parent-not-a-directory (io with stage prefix).
    parent_file = work / 'parent-file'
    parent_file.write_bytes(b'file')
    bad_child = str(parent_file / 'child.f32')
    proc2 = subprocess.run([str(cli), 'exr-pixels', str(valid), '--part', '0', '--pass', '', '--output', bad_child],
                           cwd=str(work), stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    doc2 = cli_envelope(proc2.stderr)
    if doc2 is None:
        failures.append('CLI parent-not-dir no envelope')
    else:
        if doc2['error'].get('kind') != 'io':
            failures.append('CLI parent-not-dir kind %r != io' % (doc2['error'].get('kind'),))
        if not str(doc2['error'].get('message', '')).startswith('create output stage'):
            failures.append('CLI parent-not-dir missing stage prefix: %r' % (doc2['error'].get('message'),))
        try:
            oxblend.exr_pixels(str(valid), bad_child, part=0, pass_name='')
            failures.append('PY parent-not-dir unexpectedly succeeded')
        except Exception as e2:
            if getattr(e2, 'kind', None) != 'io':
                failures.append('PY parent-not-dir kind %r != io' % (getattr(e2, 'kind', None),))
            if getattr(e2, 'message', None) != doc2['error'].get('message'):
                failures.append('PY parent-not-dir message %r != CLI %r' % (getattr(e2, 'message', None), doc2['error'].get('message')))
    # Existing destination: output_exists with same stage prefix, no overwrite.
    existing = work / 'existing.f32'
    existing.write_bytes(b'occupied')
    before_existing = existing.read_bytes()
    proc3 = subprocess.run([str(cli), 'exr-pixels', str(valid), '--part', '0', '--pass', '', '--output', str(existing)],
                           cwd=str(work), stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    doc3 = cli_envelope(proc3.stderr)
    if doc3 is None:
        failures.append('CLI existing no envelope')
    else:
        if doc3['error'].get('kind') != 'output_exists':
            failures.append('CLI existing kind %r != output_exists' % (doc3['error'].get('kind'),))
        if not str(doc3['error'].get('message', '')).startswith('create output stage'):
            failures.append('CLI existing missing stage prefix: %r' % (doc3['error'].get('message'),))
        try:
            oxblend.exr_pixels(str(valid), str(existing), part=0, pass_name='')
            failures.append('PY existing unexpectedly succeeded')
        except Exception as e3:
            if getattr(e3, 'kind', None) != 'output_exists':
                failures.append('PY existing kind %r' % (getattr(e3, 'kind', None),))
            if getattr(e3, 'message', None) != doc3['error'].get('message'):
                failures.append('PY existing message %r != CLI %r' % (getattr(e3, 'message', None), doc3['error'].get('message')))
    if existing.read_bytes() != before_existing:
        failures.append('existing output overwritten')
    # R36: native arg domain.
    qual = Path(__file__).resolve().parent / 'fixtures/native521/qualifier.blend'
    def expect_native(label, fn, types):
        try:
            fn()
            failures.append('%s unexpectedly succeeded' % (label,))
        except Exception as e:
            if not isinstance(e, types):
                failures.append('%s wrong exc %s (kind=%r msg=%r), want %r' % (label, type(e).__name__, getattr(e, 'kind', None), getattr(e, 'message', str(e))[:200], [t.__name__ for t in types]))
            if getattr(e, 'kind', None) == 'usage':
                failures.append('%s must not be UsageError' % (label,))
    expect_native('blocks -1', lambda: oxblend.block_report(str(qual), address=-1), (TypeError, OverflowError))
    expect_native('blocks 2**64', lambda: oxblend.block_report(str(qual), address=2**64), (TypeError, OverflowError, OverflowError))
    expect_native('blocks wrongtype', lambda: oxblend.block_report(str(qual), address='foo'), (TypeError,))
    # In-range invalid still canonical engine error.
    try:
        oxblend.block_report(str(qual), address=0)
        failures.append('blocks 0 unexpectedly succeeded')
    except Exception as e:
        if getattr(e, 'kind', None) != 'missing_address':
            failures.append('blocks 0 kind %r != missing_address' % (getattr(e, 'kind', None),))
    # R38a: summary(directory) is the CLI direct-open OS error on both
    # surfaces: io / Is a directory (os error 21). The Session capture
    # uses the OS directory error instead of refusing directories
    # as pack_path; Session presentation strips to the same base.
    somedir = work / 'r38dir'
    somedir.mkdir(exist_ok=True)
    dir_cli = subprocess.run([str(cli), 'summary', str(somedir)],
                             cwd=str(work), stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    if dir_cli.returncode != 2:
        failures.append('CLI summary-dir exit %r != 2' % (dir_cli.returncode,))
    dir_doc = cli_envelope(dir_cli.stderr)
    if dir_doc is None:
        failures.append('CLI summary-dir no envelope')
        dir_kind = dir_msg = None
    else:
        dir_kind = dir_doc['error'].get('kind')
        dir_msg = dir_doc['error'].get('message')
        if dir_kind != 'io':
            failures.append('CLI summary-dir kind %r != io' % (dir_kind,))
        if dir_msg != 'Is a directory (os error 21)':
            failures.append('CLI summary-dir message %r' % (dir_msg,))
    for label, fn in (('summary-path', lambda: oxblend.summary(str(somedir))),
                      ('summary-open', lambda: oxblend.open(str(somedir)).summary())):
        try:
            fn()
            failures.append('PY %s unexpectedly succeeded' % (label,))
        except Exception as e:
            if getattr(e, 'kind', None) != 'io':
                failures.append('PY %s kind %r != io' % (label, getattr(e, 'kind', None)))
            if getattr(e, 'message', None) != dir_msg:
                failures.append('PY %s message %r != CLI %r' % (label, getattr(e, 'message', None), dir_msg))
    # R38b: inspect(directory) matches the same CLI base via the
    # single-shot native capture (no Session pre-check prose).
    try:
        oxblend.inspect(str(somedir))
        failures.append('PY inspect-dir unexpectedly succeeded')
    except Exception as e:
        if getattr(e, 'kind', None) != 'io':
            failures.append('PY inspect-dir kind %r != io' % (getattr(e, 'kind', None),))
        if getattr(e, 'message', None) != dir_msg:
            failures.append('PY inspect-dir message %r != CLI %r' % (getattr(e, 'message', None), dir_msg))
    # R38c: pack(existing invalid blend) keeps the full CLI pack path on
    # both surfaces: the Session open happens INSIDE the engine pack call
    # and its snapshot/index error is wrapped with the exact pack prefix.
    garbage = work / 'r38-garbage.blend'
    garbage.write_bytes(b'R38-NOT-A-BLEND-FILE....................................\n')
    garbage_before = garbage.read_bytes()
    garbage_out = str(work / 'r38-pack-garbage-out')
    g_cli = subprocess.run([str(cli), 'pack', str(garbage), garbage_out],
                           cwd=str(work), stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    if g_cli.returncode != 2:
        failures.append('CLI pack-garbage exit %r != 2' % (g_cli.returncode,))
    g_doc = cli_envelope(g_cli.stderr)
    if g_doc is None:
        failures.append('CLI pack-garbage no envelope')
        g_kind = g_msg = None
    else:
        g_kind = g_doc['error'].get('kind')
        g_msg = g_doc['error'].get('message')
        if g_kind != 'invalid_header':
            failures.append('CLI pack-garbage kind %r != invalid_header' % (g_kind,))
        want_g = "pack '%s': snapshot/index '%s': BLENDER magic" % (garbage_out, garbage)
        if g_msg != want_g:
            failures.append('CLI pack-garbage message %r != %r' % (g_msg, want_g))
    try:
        oxblend.pack(str(garbage), garbage_out)
        failures.append('PY pack-garbage unexpectedly succeeded')
    except Exception as e:
        if getattr(e, 'kind', None) != 'invalid_header':
            failures.append('PY pack-garbage kind %r != invalid_header' % (getattr(e, 'kind', None),))
        if getattr(e, 'message', None) != g_msg:
            failures.append('PY pack-garbage message %r != CLI %r' % (getattr(e, 'message', None), g_msg))
        if getattr(e, 'operation', None) != 'pack':
            failures.append('PY pack-garbage operation %r != pack' % (getattr(e, 'operation', None),))
    if garbage.read_bytes() != garbage_before:
        failures.append('garbage input changed')
    if Path(garbage_out).exists():
        failures.append('pack-garbage published output')
    # R38d: project gate fires before capture on both surfaces, even for
    # an invalid selected blend: pack_project, exit 5, nothing written.
    no_project = str(work / 'r38-no-project')
    gp_cli = subprocess.run([str(cli), 'pack', str(garbage), str(work / 'r38-pack-proj-out'),
                             '--project', no_project],
                            cwd=str(work), stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    if gp_cli.returncode != 5:
        failures.append('CLI pack-project exit %r != 5' % (gp_cli.returncode,))
    gp_doc = cli_envelope(gp_cli.stderr)
    if gp_doc is None:
        failures.append('CLI pack-project no envelope')
        gp_msg = None
    else:
        if gp_doc['error'].get('kind') != 'pack_project':
            failures.append('CLI pack-project kind %r' % (gp_doc['error'].get('kind'),))
        gp_msg = gp_doc['error'].get('message')
    try:
        oxblend.pack(str(garbage), str(work / 'r38-pack-proj-py-out'), project_root=no_project)
        failures.append('PY pack-project unexpectedly succeeded')
    except Exception as e:
        if getattr(e, 'kind', None) != 'pack_project':
            failures.append('PY pack-project kind %r != pack_project' % (getattr(e, 'kind', None),))
        if getattr(e, 'message', None) != gp_msg:
            failures.append('PY pack-project message %r != CLI %r' % (getattr(e, 'message', None), gp_msg))
    # R38e: pack(missing directory) stays pack_input on both, exit 3.
    missing_dir = str(work / 'r38-missing-dir')
    md_cli = subprocess.run([str(cli), 'pack', missing_dir, str(work / 'r38-pack-mdir-out')],
                            cwd=str(work), stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    if md_cli.returncode != 3:
        failures.append('CLI pack-missing-dir exit %r != 3' % (md_cli.returncode,))
    md_doc = cli_envelope(md_cli.stderr)
    md_msg = md_doc['error'].get('message') if md_doc is not None else None
    if md_doc is None or md_doc['error'].get('kind') != 'pack_input':
        failures.append('CLI pack-missing-dir not pack_input')
    try:
        oxblend.pack(missing_dir, str(work / 'r38-pack-mdir-py-out'))
        failures.append('PY pack-missing-dir unexpectedly succeeded')
    except Exception as e:
        if getattr(e, 'kind', None) != 'pack_input':
            failures.append('PY pack-missing-dir kind %r != pack_input' % (getattr(e, 'kind', None),))
        if getattr(e, 'message', None) != md_msg:
            failures.append('PY pack-missing-dir message %r != CLI %r' % (getattr(e, 'message', None), md_msg))
    # R38f: captured File pack input-change guard keeps the shared pack
    # prefix (no regression of the R35 shape after the strip change).
    cap_copy = work / 'r38-cap.blend'
    cap_copy.write_bytes(qual.read_bytes())
    cap_out = str(work / 'r38-cap-out')
    try:
        with oxblend.open(str(cap_copy)) as f:
            cap_copy.write_bytes(b'R38-CHANGED' + cap_copy.read_bytes()[11:])
            f.pack(cap_out)
        failures.append('PY cap-changed pack unexpectedly succeeded')
    except Exception as e:
        if getattr(e, 'kind', None) != 'input_changed':
            failures.append('PY cap-changed kind %r != input_changed' % (getattr(e, 'kind', None),))
        want_cap = "pack '%s': source signature changed since snapshot" % (cap_out,)
        if getattr(e, 'message', None) != want_cap:
            failures.append('PY cap-changed message %r != %r' % (getattr(e, 'message', None), want_cap))
    if Path(cap_out).exists():
        failures.append('cap-changed published output')
    # R38g: valid pack still succeeds path-based (native pack owner
    # covers the happy path, not only refusals); report complete, input
    # unchanged, destination published.
    ok_copy = work / 'r38-ok.blend'
    ok_copy.write_bytes(qual.read_bytes())
    ok_before = ok_copy.read_bytes()
    ok_out = str(work / 'r38-ok-out')
    try:
        rep = oxblend.pack(str(ok_copy), ok_out)
    except Exception as e:
        failures.append('PY pack-valid failed: kind=%r msg=%r' % (getattr(e, 'kind', None), getattr(e, 'message', None)))
        rep = None
    if rep is not None and not rep.get('complete', False):
        failures.append('PY pack-valid incomplete')
    if ok_copy.read_bytes() != ok_before:
        failures.append('valid pack input changed')
    if not Path(ok_out).is_dir():
        failures.append('valid pack output missing')
    print('cases=15 failures=%d' % (len(failures),))
    for f in failures:
        print('FAIL %s' % (f,))
    return 1 if failures else 0


if __name__ == '__main__':
    raise SystemExit(main())
