#!/usr/bin/env python3
"""Narrow contract tests for the shared sdist/wheel metadata description gate."""
from __future__ import annotations
import contextlib
import importlib.util
import io
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts' / 'check_release_dist.py'
SPEC = importlib.util.spec_from_file_location('release_metadata_gate', SCRIPT)
assert SPEC is not None and SPEC.loader is not None
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)


class MetadataFraming(unittest.TestCase):
    def check_body(self, body: bytes, accepted: bool) -> None:
        headers = (b'Metadata-Version: 2.4\nName: oxblend\n'
                   b'Project-URL: Repository, https://github.com/kyraneth/oxblend\n'
                   b'License-Expression: MIT\nDescription-Content-Type: text/markdown\n\n')
        for where in ('sdist PKG-INFO', 'wheel METADATA'):
            with self.subTest(where=where):
                gate = GATE.Gate()
                with contextlib.redirect_stdout(io.StringIO()):
                    GATE.metadata_fields(headers + body, gate, where)
                if accepted:
                    self.assertEqual(gate.errors, [])
                else:
                    self.assertEqual(gate.errors, [f'{where} raw body must equal binding README bytes plus exactly one framing LF'])

    def test_one_writer_lf_passes(self) -> None:
        self.check_body(GATE.BINDING_README.read_bytes() + b'\n', True)

    def test_missing_writer_lf_fails(self) -> None:
        self.check_body(GATE.BINDING_README.read_bytes(), False)

    def test_two_writer_lfs_fail(self) -> None:
        self.check_body(GATE.BINDING_README.read_bytes() + b'\n\n', False)

    def test_any_changed_byte_fails(self) -> None:
        body = bytearray(GATE.BINDING_README.read_bytes())
        body[0] ^= 1
        self.check_body(bytes(body) + b'\n', False)


if __name__ == '__main__':
    unittest.main()
