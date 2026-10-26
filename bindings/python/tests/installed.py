"""Installed-wheel acceptance. All locations are explicit caller inputs.

Test-only dependency: flattrace. No checkout imports or maturin develop.
"""
from __future__ import annotations
import argparse
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from dataclasses import FrozenInstanceError
import functools
import hashlib
import json
import itertools
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import time
import unittest

import oxblend as ox
from flattrace import Writer

P = argparse.ArgumentParser()
P.add_argument("--corpus-manifest", type=Path, required=True)
P.add_argument("--cli-before", type=Path, required=True)
P.add_argument("--cli", type=Path, required=True)
P.add_argument("--seed", type=Path, required=True)
P.add_argument("--evidence", type=Path, required=True)
ARGS = P.parse_args()
ARGS.evidence.mkdir(parents=True, exist_ok=True)
TRACE = Writer.from_env("oxblend-python-tests", ox.__version__, argv=sys.argv)
assert TRACE is not None, "FLATTRACE_DIR must enable the test harness"
TRACE_LOCK = threading.Lock()
LOCAL = threading.local()

@contextmanager
def span(event, **data):
    with TRACE_LOCK:
        token = TRACE.start(event, data, parent=getattr(LOCAL, "parent", None))
    previous = getattr(LOCAL, "parent", None)
    LOCAL.parent = token
    try:
        yield
    except BaseException as exc:
        with TRACE_LOCK:
            TRACE.end(token, "error", error={"kind": type(exc).__name__, "message": str(exc)})
        raise
    else:
        with TRACE_LOCK:
            TRACE.end(token, "ok")
    finally:
        LOCAL.parent = previous

def traced(name, fn):
    @functools.wraps(fn)
    def call(*args, **kwargs):
        with span("python.api", operation=name):
            return fn(*args, **kwargs)
    return call

# Test-only instrumentation. The distributed package has no tracing dependency.
ox.open = traced("open", ox.open)
for name in ("close", "blocks", "get", "pointer", "summary", "__enter__", "__exit__"):
    setattr(ox.File, name, traced(name, getattr(ox.File, name)))
for name in ("header", "closed"):
    setattr(ox.File, name, property(traced(name, getattr(ox.File, name).fget)))

CORPUS = json.loads(ARGS.corpus_manifest.read_text())
REPORT = {"python": sys.version, "module": ox.__file__, "native": ox._native.__file__, "version": ox.__version__, "engine": ox.engine_version, "corpus": []}
CLI_CALLS = ARGS.evidence / "cli-calls"
CLI_CALLS.mkdir()  # Fresh evidence only; never merge a previous test run.
CLI_SEQUENCE = itertools.count()

def cli(binary, command, path, *args):
    argv = [str(binary), command, os.fspath(path), *map(str, args)]
    with span("cli", command=command):
        p = subprocess.run(argv, capture_output=True)
    record = CLI_CALLS / f"{next(CLI_SEQUENCE):06d}"
    record.mkdir()
    (record / "stdout").write_bytes(p.stdout)
    (record / "stderr").write_bytes(p.stderr)
    (record / "result.json").write_text(json.dumps({
        "argv_hex": [os.fsencode(arg).hex() for arg in argv],
        "exit": p.returncode,
        "stdout_sha256": hashlib.sha256(p.stdout).hexdigest(),
        "stderr_sha256": hashlib.sha256(p.stderr).hexdigest(),
    }, indent=2) + "\n")
    return p

def value_json(v):
    if v.kind in ("signed", "unsigned", "pointer"):
        return {"kind": v.kind, "decimal": str(v.value)}
    if v.kind == "bytes":
        return {"kind": "bytes", "hex": v.value.hex()}
    if v.kind == "float32":
        return {"kind": "float32", "bits_hex": f"{v.value.bits:08x}"}
    return {"kind": "array", "items": [value_json(x) for x in v.value]}

def summary_json(s):
    def text(b): return None if b is None else b.decode("utf-8")
    h = s.header
    return dict(schema=1, header=dict(version=h.version, pointer_size=h.pointer_size, little_endian=h.little_endian, file_format=h.file_format),
        block_codes={text(c): n for c, n in s.block_codes},
        **{k: list(map(text, getattr(s, k))) for k in ("objects", "materials", "meshes", "images", "collections")},
        scenes=[dict(name=text(c.name), frame_start=c.frame_start, frame_end=c.frame_end, frame_step=c.frame_step, camera=text(c.camera),
            fps=dict(numerator=c.fps.numerator, base=None if c.fps.base is None or not math.isfinite(c.fps.base.value) else c.fps.base.value,
                effective=c.fps.effective, base_bits_hex=None if c.fps.base is None else f"{c.fps.base.bits:08x}")) for c in s.scenes],
        warnings=[dict(kind=w.kind, block_index=w.block_index, element=w.element, field=w.field, message=w.message) for w in s.warnings])

class Result(unittest.TextTestResult):
    def startTest(self, test):
        self._span = span("test", name=test.id())
        self._span.__enter__()
        super().startTest(test)
    def stopTest(self, test):
        super().stopTest(test)
        self._span.__exit__(None, None, None)

class Installed(unittest.TestCase):
    def test_01_corpus_cli_parity(self):
        self.assertEqual(len(CORPUS), 120)  # The supplied qualification corpus contract.
        for item in CORPUS:
            path = Path(item["path"])
            with self.subTest(index=item["index"], path=str(path)):
                before = cli(ARGS.cli_before, "summary", path)
                after = cli(ARGS.cli, "summary", path)
                self.assertEqual((before.returncode, before.stdout), (after.returncode, after.stdout))
                prefix = ARGS.evidence / f'corpus-{item["index"]:03d}'
                prefix.with_suffix(".before.stdout").write_bytes(before.stdout)
                prefix.with_suffix(".after.stdout").write_bytes(after.stdout)
                prefix.with_suffix(".before.stderr").write_bytes(before.stderr)
                prefix.with_suffix(".after.stderr").write_bytes(after.stderr)
                row = {"index": item["index"], "path": str(path), "returncode": after.returncode, "stdout_sha256": hashlib.sha256(after.stdout).hexdigest(), "get_count": 0}
                try:
                    with ox.open(path) as f:
                        s = f.summary()
                        self.assertEqual(after.returncode, 0)
                        self.assertEqual(summary_json(s), json.loads(after.stdout))
                        for block in f.blocks(code=b"SC"):
                            fields = (b"sfra", b"efra", b"frame_step", b"frs_sec", b"frs_sec_base")
                            qs = [ox.FieldQuery(block, 0, (b"r", field)) for field in fields]
                            try:
                                values = f.get(qs)
                            except ox.OxblendError as exc:
                                self.assertEqual(exc.kind, "missing_field")
                                continue
                            for field, v in zip(fields, values):
                                p = cli(ARGS.cli, "get", path, block.index, json.dumps(["r", field.decode()]))
                                self.assertEqual(p.returncode, 0)
                                self.assertEqual(value_json(v), json.loads(p.stdout)["value"])
                                row["get_count"] += 1
                except ox.OxblendError as exc:
                    self.assertNotEqual(after.returncode, 0)
                    errors = [json.loads(line) for line in after.stderr.splitlines() if line.startswith(b'{')]
                    self.assertTrue(any(e.get("error", {}).get("kind") == exc.kind for e in errors), (exc.kind, after.stderr))
                    row["error_kind"] = exc.kind
                REPORT["corpus"].append(row)

    def test_02_close_and_foreign_handles(self):
        with ox.open(ARGS.seed) as a, ox.open(ARGS.seed) as b:
            block = a.blocks(code=b"SC")[0]
            self.assertEqual(block.code, b"SC\0\0")
            self.assertEqual(a.blocks(code=block.code)[0].index, block.index)
            q = ox.FieldQuery(block, 0, (b"r", b"sfra"))
            saved = a.summary()
            with self.assertRaises(ox.OxblendError) as caught:
                b.get([q])
            self.assertEqual(caught.exception.kind, "foreign_block")
            self.assertEqual(dict(caught.exception.context)["query_index"], 0)
            self.assertEqual(caught.exception.operation, "get")
            with self.assertRaises(ox.OxblendError) as pointer_error:
                b.pointer(q)
            self.assertEqual(pointer_error.exception.kind, "foreign_block")
            self.assertEqual(dict(pointer_error.exception.context)["query_index"], 0)
            with self.assertRaises(AttributeError): block.index = 0
            with self.assertRaises((FrozenInstanceError, AttributeError)): saved.objects = ()
            self.assertIsNone(a.pointer(ox.FieldQuery(block, 0, (b"camera",))))
        self.assertTrue(a.closed)
        self.assertIsInstance(saved.scenes, tuple)
        for op in (lambda: a.header, a.blocks, lambda: a.get([q]), lambda: a.pointer(q), a.summary, a.__enter__):
            with self.assertRaises(ox.ClosedFileError): op()
        a.close()  # Idempotent and never reopens.

    def test_03_source_changed(self):
        with tempfile.TemporaryDirectory(dir=ARGS.evidence) as d:
            target = Path(d) / "source.blend"
            for change in ("modify", "replace", "truncate", "unlink"):
                shutil.copyfile(ARGS.seed, target)
                with ox.open(target) as f:
                    if change == "modify":
                        with target.open("ab") as out: out.write(b"x")
                    elif change == "replace":
                        other = Path(d) / "other.blend"
                        shutil.copyfile(ARGS.seed, other)
                        other.replace(target)
                    elif change == "truncate":
                        with target.open("r+b") as out: out.truncate(12)
                    else: target.unlink()
                    with self.assertRaises(ox.SourceChangedError) as caught: f.summary()
                    self.assertEqual(caught.exception.kind, "input_changed")

    def test_04_bytes_float_int(self):
        # Mutate only a test-owned copy. Offsets come from the independent CLI SDNA.
        inspected = json.loads(cli(ARGS.cli, "inspect", ARGS.seed).stdout)
        dna = inspected["sdna"]
        def field(typename, name):
            ty = next(i for i, x in enumerate(dna["types_hex"]) if bytes.fromhex(x) == typename)
            st = next(s for s in dna["structs"] if s["type_index"] == ty)
            return next(f for f in st["fields"] if bytes.fromhex(f["name_hex"]) == name)["offset"]
        with tempfile.TemporaryDirectory(dir=ARGS.evidence) as d:
            target = os.fsencode(d) + b"/invalid-\xff.blend"
            data = bytearray(ARGS.seed.read_bytes())
            with ox.open(ARGS.seed) as original:
                scene = original.blocks(code=b"SC")[0]
            name_offset = scene.data_offset + field(b"Scene", b"id") + field(b"ID", b"name")
            data[name_offset:name_offset + 4] = b"SC\xff\0"
            r = scene.data_offset + field(b"Scene", b"r")
            # Negative zero and a NaN payload: exact saved u32 representation.
            base = r + field(b"RenderData", b"frs_sec_base")
            for bits in (0x80000000, 0x7FC12345):
                data[base:base + 4] = struct.pack("<I", bits)
                with open(target, "wb") as out: out.write(data)
                with ox.open(target) as f:
                    scene = f.blocks(code=b"SC")[0]
                    name, flt = f.get([ox.FieldQuery(scene, 0, (b"id", b"name")), ox.FieldQuery(scene, 0, (b"r", b"frs_sec_base"))])
                    self.assertEqual(name.value, b"SC\xff")
                    with self.assertRaises(UnicodeDecodeError): _ = name.text
                    self.assertEqual(flt.value.bits, bits)
                    self.assertEqual(f.summary().scenes[0].name, b"SC\xff")
                    self.assertEqual(f.summary().scenes[0].fps.base.bits, bits)
                    self.assertIsInstance(scene.old_address, int)

    def test_045_primitive_exactness(self):
        # An independently authored minimal SDNA fixture, not another reader's bytes.
        names = [b"number", b"*next", b"name[8]", b"scalar", b"array[2]"]
        types = [b"char", b"uint64_t", b"float", b"int", b"Test"]
        dna = bytearray(b"SDNANAME" + struct.pack("<I", len(names)) + b"\0".join(names) + b"\0")
        dna.extend(b"\0" * (-len(dna) % 4))
        dna.extend(b"TYPE" + struct.pack("<I", len(types)) + b"\0".join(types) + b"\0")
        dna.extend(b"\0" * (-len(dna) % 4))
        dna.extend(b"TLEN" + struct.pack("<5H", 1, 8, 4, 4, 36))
        dna.extend(b"\0" * (-len(dna) % 4))
        dna.extend(b"STRC" + struct.pack("<IHH", 1, 4, len(names)))
        for ti, ni in ((1, 0), (4, 1), (0, 2), (2, 3), (3, 4)):
            dna.extend(struct.pack("<HH", ti, ni))
        payload = struct.pack("<QQ8sIii", 2**64 - 1, 2**64 - 1, b"N\xff\0xxxxx", 0x7FC12345, -(2**31), 2**31 - 1)
        def block(code, data, address):
            return struct.pack("<4sIQII", code, len(data), address, 0, 1) + data
        raw = b"BLENDER-v405" + block(b"TEST", payload, 2**63 + 1) + block(b"DNA1", dna, 0) + struct.pack("<4sIQII", b"ENDB", 0, 0, 0, 0)
        with tempfile.TemporaryDirectory(dir=ARGS.evidence) as d:
            target = Path(d) / "exact.blend"
            target.write_bytes(raw)
            with ox.open(target) as f:
                b = f.blocks(code=b"TEST")[0]
                self.assertEqual(b.old_address, 2**63 + 1)
                fields = (b"number", b"next", b"name", b"scalar", b"array")
                values = f.get(ox.FieldQuery(b, 0, (field,)) for field in fields)
                self.assertEqual(values[0].value, 2**64 - 1)
                self.assertEqual(values[1].kind, "pointer")
                self.assertEqual(values[1].value, 2**64 - 1)
                self.assertEqual(values[2].value, b"N\xff")
                self.assertEqual(values[3].value.bits, 0x7FC12345)
                self.assertEqual(tuple(v.value for v in values[4].value), (-(2**31), 2**31 - 1))
                self.assertEqual(f.get([ox.FieldQuery(b, 0, (b"array", 1))])[0].value, 2**31 - 1)
                self.assertEqual(f.get([ox.FieldQuery(b, 0, (b"name",), False)])[0].value, b"N\xff\0xxxxx")
                for field, v in zip(fields, values):
                    p = cli(ARGS.cli, "get", target, b.index, json.dumps([field.decode()]))
                    self.assertEqual(p.returncode, 0)
                    self.assertEqual(value_json(v), json.loads(p.stdout)["value"])
                with self.assertRaises(ox.OxblendError): f.pointer(ox.FieldQuery(b, 0, (b"next",)))

    def test_05_threads_and_close(self):
        # Public splash workload from the supplied corpus (not the three-block seed).
        path = Path(CORPUS[1]["path"])
        # Batch size follows its actual block count, not an arbitrary sleep/limit.
        with ox.open(path) as f:
            b = f.blocks(code=b"SC")[0]
            q = ox.FieldQuery(b, 0, (b"r", b"sfra"))
            count = len(f.blocks())
            queries = (q,) * count
            barrier = threading.Barrier(2)
            def same():
                barrier.wait()
                return tuple(v.value for v in f.get(queries))
            with ThreadPoolExecutor(max_workers=2) as pool:  # Two actors exercise the per-handle pair contract.
                a, b = pool.submit(same), pool.submit(same)
                self.assertEqual(a.result(), b.result())
        barrier = threading.Barrier(2)
        def separate():
            with ox.open(path) as f:
                barrier.wait()
                return f.summary().scenes[0].frame_start
        with ThreadPoolExecutor(max_workers=2) as pool:
            a, b = pool.submit(separate), pool.submit(separate)
            self.assertEqual(a.result(), b.result())
        f = ox.open(path)
        b = f.blocks(code=b"SC")[0]
        q = ox.FieldQuery(b, 0, (b"r", b"sfra"))
        barrier = threading.Barrier(2)
        def read_race():
            barrier.wait()
            try: return f.get((q,) * count)
            except ox.ClosedFileError: return "closed-before-read"
        def close_race():
            barrier.wait()
            f.close()
        with ThreadPoolExecutor(max_workers=2) as pool:
            a, b = pool.submit(read_race), pool.submit(close_race)
            outcome = a.result(); b.result()
        self.assertTrue(f.closed)
        REPORT["close_race"] = "completed-read" if isinstance(outcome, tuple) else outcome

    def test_06_skill_and_stubs(self):
        with span("recipe", name="skill print"):
            p = subprocess.run([sys.executable, "-m", "oxblend", "skill", "print"], capture_output=True)
        (ARGS.evidence / "skill.stdout").write_bytes(p.stdout)
        (ARGS.evidence / "skill.stderr").write_bytes(p.stderr)
        (ARGS.evidence / "skill.exit").write_text(str(p.returncode) + "\n")
        self.assertEqual(p.returncode, 0, p.stderr)
        package = Path(ox.__file__).parent
        self.assertEqual(p.stdout, (package / "guidance/SKILL.md").read_bytes())
        self.assertIn(b"## Python binding (in-process, same engine)", p.stdout)
        self.assertTrue((package / "py.typed").is_file())
        self.assertTrue((package / "_native.pyi").is_file())
        notices = (package / "THIRD-PARTY-NOTICES.txt").read_bytes()
        self.assertTrue(notices.startswith(b"THIRD-PARTY NOTICES\n"))
        self.assertIn(b"Oxblend " + ox.engine_version.split("-", 1)[0].encode(), notices)
        self.assertIn(b"=== pyo3 0.30.1 ===", notices)
        self.assertIn(b"Permission is hereby granted", notices)

if __name__ == "__main__":
    try:
        result = unittest.TextTestRunner(verbosity=2, resultclass=Result).run(unittest.defaultTestLoader.loadTestsFromTestCase(Installed))
        REPORT["tests"] = result.testsRun
        REPORT["failures"] = len(result.failures)
        REPORT["errors"] = len(result.errors)
        (ARGS.evidence / "installed-results.json").write_text(json.dumps(REPORT, indent=2))
    finally:
        TRACE.close()
    raise SystemExit(not result.wasSuccessful())
