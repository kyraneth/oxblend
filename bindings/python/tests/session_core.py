"""Installed-wheel P0/P2 receiving proof; no checkout import or test monkeypatch.

Every native operation emits python.call and joined engine spans. The external
runner sets FLATTRACE_DIR/LEVEL before this process starts. This program does
not run CI or publish anything. All output destinations are new under evidence.
"""
from __future__ import annotations
import argparse
from collections.abc import Mapping
from concurrent.futures import ThreadPoolExecutor
from dataclasses import FrozenInstanceError
import hashlib
import json
import os
import copy
import pickle
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
import zipfile
import oxblend as ox

parser = argparse.ArgumentParser()
parser.add_argument("--seed", type=Path, required=True)
parser.add_argument("--cli", type=Path, required=True)
parser.add_argument("--evidence", type=Path, required=True)
parser.add_argument("--corpus-manifest", type=Path)
parser.add_argument("--missing-asset", type=Path, required=True)
args = parser.parse_args()
args.evidence.mkdir(parents=True, exist_ok=True)
assert os.environ.get("FLATTRACE_DIR"), "native tracing must be enabled before import"
report = {"python": sys.version, "module": ox.__file__, "native": ox._native.__file__,
          "native_sha256": hashlib.sha256(Path(ox._native.__file__).read_bytes()).hexdigest(),
          "binding": ox.__version__, "engine": ox.engine_version, "trace_corpus": []}

def thaw(value):
    if isinstance(value, Mapping): return {k: thaw(v) for k, v in value.items()}
    if isinstance(value, tuple): return [thaw(v) for v in value]
    return value

def cli(command, path, *extra):
    p = subprocess.run([str(args.cli), command, os.fspath(path), *map(os.fspath, extra)], capture_output=True)
    return p, json.loads(p.stdout) if p.stdout else None

class SessionCore(unittest.TestCase):
    def test_record_and_report_mapper_contracts(self):
        mutable = bytearray(b"saved")
        record = ox.Record({"payload": mutable, "nested": {"value": 1}})
        mutable.extend(b"changed")
        self.assertEqual(record["payload"], b"saved")
        self.assertEqual(record, dict(record))
        self.assertEqual(copy.deepcopy(record), record)
        self.assertEqual(pickle.loads(pickle.dumps(record)), record)
        facts = {"complete": False, "files": ({"output_written": True},),
                 "errors": ({"kind": "input_changed", "message": "source changed after effects"},),
                 "operation_diagnostics": (), "directory_created": True}
        report = ox.PackReport(facts)
        self.assertEqual(report, ox.Record(facts))
        for allow in (False, True):
            with self.assertRaises(ox.SourceChangedError) as error:
                ox._admit(report, ox.IncompletePackError, "pack", allow)
            self.assertIs(error.exception.report, report)
            self.assertEqual(error.exception.kind, "input_changed")
            self.assertEqual(error.exception.effects, "reported")
            self.assertTrue(error.exception.report.files[0]["output_written"])
        with ox.open(args.seed) as f:
            block = f.blocks(code=b"SC")[0]
            with self.assertRaises(TypeError): ox.FieldQuery(block, 0, b"id")
            with self.assertRaises(TypeError): f.query(type="Scene", fields="r.sfra")

    def test_repeated_read_trace_pack(self):
        with tempfile.TemporaryDirectory(dir=args.evidence) as temp:
            root = Path(temp) / "root.blend"
            shutil.copyfile(args.seed, root)
            destination = Path(temp) / "new-pack"
            with ox.open(root) as f:
                initial = f.summary()
                scene = f.blocks(code=b"SC")[0]
                q = ox.FieldQuery(scene, 0, (b"r", b"sfra"))
                values = f.get([q])
                self.assertEqual(values[0].value, initial.scenes[0].frame_start)
                first = f.trace()
                second = f.trace()
                self.assertEqual(first, second)
                p, expected = cli("trace", root)
                self.assertEqual(p.returncode, 0, p.stderr)
                actual = thaw(first); actual.pop("operation_diagnostics")
                self.assertEqual(actual, expected)
                planned = f.pack(destination, project_root=Path(temp), dry_run=True)
                self.assertTrue(planned.complete)
                self.assertFalse(destination.exists())
                delivered = f.pack(destination, project_root=Path(temp))
                self.assertTrue(delivered.complete)
                self.assertTrue(delivered.directory_created)
                self.assertTrue(all(item["output_written"] for item in delivered.files))
                for item in delivered.files:
                    self.assertTrue(Path(os.fsdecode(item.raw("output_path"))).is_file())
                self.assertEqual(f.summary(), initial)
                self.assertEqual(f.get([q]), values)
                self.assertIsInstance(f.open_diagnostics, tuple)
            self.assertTrue(f.closed)
            self.assertTrue(delivered.complete)  # Owned results survive close.
            with self.assertRaises((FrozenInstanceError, AttributeError, TypeError)):
                first.complete = False
            with self.assertRaises(TypeError): first["complete"] = False
            with self.assertRaises(TypeError): delivered.files[0]._facts["output_written"] = False
            with self.assertRaises(ox.ClosedFileError): f.trace()
            with self.assertRaises(ox.ClosedFileError): f.pack(Path(temp) / "closed", project_root=Path(temp))

    def test_unavailable_cwd_is_not_replaced_with_filesystem_root(self):
        with tempfile.TemporaryDirectory(dir=args.evidence) as temp:
            source = Path(temp).resolve() / "scene.blend"
            shutil.copyfile(args.seed, source)
            vanished = Path(temp) / "vanished-cwd"
            vanished.mkdir()
            old_cwd = Path.cwd()
            try:
                with ox.open(source) as f:
                    os.chdir(vanished)
                    vanished.rmdir()
                    # The old '/' fallback would incorrectly open this real
                    # test-owned file despite an unresolvable relative source.
                    with self.assertRaises(ox.OxblendError) as error:
                        unexpected = ox.open(source.relative_to("/"))
                        unexpected.close()
                    self.assertEqual(error.exception.kind, "io")
                    target = source.parent / "must-not-publish"
                    with self.assertRaises(ox.OxblendError) as error:
                        f.pack(target.relative_to("/"), project_root=source.parent)
                    self.assertEqual(error.exception.kind, "io")
                    self.assertFalse(target.exists())
                    # Captured root-only reads do not depend on a new cwd.
                    self.assertTrue(f.summary().scenes)
            finally:
                os.chdir(old_cwd)

    def test_session_freezes_temp_root_for_zip_and_verification(self):
        with tempfile.TemporaryDirectory(dir=args.evidence) as temp:
            frozen = Path(temp) / "frozen-temp"
            frozen.mkdir()
            absent = Path(temp) / "not-a-temp-directory"
            old = os.environ.get("TMPDIR")
            try:
                os.environ["TMPDIR"] = str(frozen)
                with ox.open(args.seed) as f:
                    os.environ["TMPDIR"] = str(absent)
                    target = Path(temp) / "captured.zip"
                    packed = f.pack(target, project_root=args.seed.parent)
                    self.assertTrue(packed.complete)
                    self.assertTrue(packed.archive_written)
                    self.assertTrue(target.is_file())
                    self.assertFalse(absent.exists())
                    with zipfile.ZipFile(target) as archive:
                        self.assertIsNone(archive.testzip())
                    # ZIP intentionally rejects --compress: exercise rewritten
                    # compressed-blend verification separately, with the same
                    # Session-frozen temp root and still-invalid new TMPDIR.
                    compressed = f.pack(Path(temp) / "compressed-folder",
                                        project_root=args.seed.parent, compress=True)
                    self.assertTrue(compressed.complete)
                    self.assertTrue(any(item["output_written"] for item in compressed.files))
                    self.assertFalse(absent.exists())
            finally:
                if old is None: os.environ.pop("TMPDIR", None)
                else: os.environ["TMPDIR"] = old

    def test_pack_resolves_destination_at_call_entry(self):
        with tempfile.TemporaryDirectory(dir=args.evidence) as temp:
            source_directory = Path(temp) / "source"
            call_directory = Path(temp) / "call"
            source_directory.mkdir(); call_directory.mkdir()
            shutil.copyfile(args.seed, source_directory / "scene.blend")
            old_cwd = Path.cwd()
            try:
                os.chdir(source_directory)
                with ox.open("scene.blend") as f:
                    saved = f.summary()
                    os.chdir(call_directory)
                    packed = f.pack("new-output", project_root=source_directory)
                    self.assertTrue(packed.complete)
                    self.assertTrue((call_directory / "new-output").is_dir())
                    self.assertFalse((source_directory / "new-output").exists())
                    self.assertEqual(f.summary(), saved)
            finally:
                os.chdir(old_cwd)

    def test_pack_incomplete_effects_are_preserved(self):
        with tempfile.TemporaryDirectory(dir=args.evidence) as temp:
            root = Path(temp) / "text.blend"
            shutil.copyfile(args.missing_asset, root)
            self.assertFalse((Path(temp) / "external.py").exists())
            with ox.open(root) as f:
                strict_target = Path(temp) / "strict"
                with self.assertRaises(ox.IncompletePackError) as error:
                    f.pack(strict_target, project_root=Path(temp))
                strict = error.exception.report
                self.assertFalse(strict.complete)
                self.assertFalse(any(item["output_written"] for item in strict.files))
                self.assertFalse(strict.archive_written)
                self.assertEqual(error.exception.effects, "reported")
                partial_target = Path(temp) / "partial"
                with self.assertRaises(ox.IncompletePackError) as error:
                    f.pack(partial_target, project_root=Path(temp), permissive=True)
                partial = error.exception.report
                self.assertFalse(partial.complete)
                self.assertTrue(any(item["output_written"] for item in partial.files))
                for item in partial.files:
                    self.assertEqual(Path(os.fsdecode(item.raw("output_path"))).exists(), item["output_written"])
                # Separate new output, deliberate allowed partial return. Never
                # retry/merge the uncertain or already-published destination.
                returned = f.pack(Path(temp) / "partial-return", project_root=Path(temp),
                                  permissive=True, allow_incomplete=True)
                self.assertFalse(returned.complete)
                self.assertEqual(tuple(item["output_written"] for item in returned.files),
                                 tuple(item["output_written"] for item in partial.files))

    def test_changed_root_is_latched(self):
        with tempfile.TemporaryDirectory(dir=args.evidence) as temp:
            path = Path(temp) / "root.blend"
            data = args.seed.read_bytes(); path.write_bytes(data)
            with ox.open(path) as f:
                with path.open("ab") as output: output.write(b"change")
                with self.assertRaises(ox.SourceChangedError) as error: f.trace()
                self.assertEqual(error.exception.kind, "input_changed")
                path.write_bytes(data)
                with self.assertRaises(ox.SourceChangedError): f.summary()
                with self.assertRaises(ox.SourceChangedError) as error:
                    f.pack(Path(temp) / "refused", project_root=Path(temp))
                self.assertEqual(error.exception.effects, "unknown")
                self.assertIsNone(error.exception.report)
                self.assertFalse((Path(temp) / "refused").exists())
                self.assertIsInstance(error.exception.diagnostics, tuple)

    def test_optional_query_metadata_keeps_rows(self):
        # L3 regression: this real supplied fixture has FileGlobal but omits
        # subversion. Missing observation metadata must not discard raw rows.
        source = args.missing_asset.with_name("mesh-material-root.blend")
        with ox.open(source) as f:
            known = f.query(type="Scene", fields=["id.name"])
            self.assertTrue(known.complete)
            self.assertTrue(known.rows)
            self.assertFalse(known["source_observation"]["complete"])
            self.assertIsNone(known["source_observation"]["subversion"])
            self.assertTrue(known["source_observation"]["errors"])
            missing = f.query(type="Scene", fields=["r.sfra"], allow_incomplete=True)
            self.assertFalse(missing.complete)
            self.assertTrue(missing.rows)
            self.assertEqual(missing.rows[0]["values"]["r.sfra"]["status"], "unavailable")

    def test_query_preserves_engine_envelope(self):
        with ox.open(args.seed) as f:
            queried = f.query(type="Scene", fields=["r.sfra", "r.efra"], allow_incomplete=True)
            self.assertEqual(queried["schema"], "oxblend.query/1")
            self.assertEqual(queried["basis"], "saved_controls_not_evaluated_results")
            self.assertIsInstance(queried.rows, tuple)
            p, expected = cli("query", args.seed, "--type", "Scene", "--fields", "r.sfra,r.efra")
            actual = thaw(queried); actual.pop("operation_diagnostics")
            self.assertEqual(actual, expected, p.stderr)
            unavailable = f.query(type="Scene", fields=["r.__missing_field__"], allow_incomplete=True)
            self.assertFalse(unavailable.complete)
            with self.assertRaises(ox.IncompleteQueryError) as error:
                f.query(type="Scene", fields=["r.__missing_field__"])
            self.assertEqual(error.exception.report, unavailable)
            explanation = f.explain("RenderData")
            self.assertIsInstance(explanation, ox.Record)
            self.assertIsInstance(ox.catalog(), ox.Record)
        self.assertEqual(queried["schema"], "oxblend.query/1")

    def test_same_and_separate_handles(self):
        with ox.open(args.seed) as f:
            barrier = threading.Barrier(2)
            def same():
                barrier.wait(); return f.summary(), f.trace()
            with ThreadPoolExecutor(max_workers=2) as pool:
                a, b = pool.submit(same), pool.submit(same)
                self.assertEqual(a.result(), b.result())
        barrier = threading.Barrier(2)
        def separate():
            with ox.open(args.seed) as f:
                barrier.wait(); return f.summary(), f.trace()
        with ThreadPoolExecutor(max_workers=2) as pool:
            a, b = pool.submit(separate), pool.submit(separate)
            self.assertEqual(a.result(), b.result())

    def test_trace_corpus(self):
        if args.corpus_manifest is None: self.skipTest("no supplied corpus")
        corpus = json.loads(args.corpus_manifest.read_text())
        self.assertEqual(len(corpus), 120)  # Supplied design acceptance corpus.
        for item in corpus:
            path = Path(item["path"])
            with self.subTest(path=str(path)):
                p, expected = cli("trace", path)
                prefix = args.evidence / ("trace-" + str(item["index"]))
                prefix.with_suffix(".stdout").write_bytes(p.stdout)
                prefix.with_suffix(".stderr").write_bytes(p.stderr)
                try:
                    with ox.open(path) as f: actual = f.trace(allow_incomplete=True)
                except ox.OxblendError as error:
                    self.assertNotEqual(p.returncode, 0)
                    envelopes = [json.loads(line) for line in p.stderr.splitlines() if line.startswith(b"{")]
                    self.assertTrue(any(e.get("error", {}).get("kind") == error.kind for e in envelopes), (error.kind, p.stderr))
                    report["trace_corpus"].append({"index": item["index"], "kind": error.kind})
                else:
                    native = thaw(actual); native.pop("operation_diagnostics")
                    self.assertEqual(native, expected)
                    report["trace_corpus"].append({"index": item["index"], "complete": actual.complete, "returncode": p.returncode})

if __name__ == "__main__":
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(SessionCore))
    report.update(tests=result.testsRun, failures=len(result.failures), errors=len(result.errors))
    (args.evidence / "session-results.json").write_text(json.dumps(report, indent=2))
    raise SystemExit(not result.wasSuccessful())
