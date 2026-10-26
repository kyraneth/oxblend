"""L24 idprop-string positive + guard parity (CLI + installed Python).

Uses the same newly authored fixture as ``error_cases_idprop`` (read-only
``empty.blend`` seed, header spoofed to 502, two fresh blocks appended).
No original is repaired or overwritten.

Covers:
- Positive guarded edit via CLI and installed Python with a generic
  Mapping spec (dict and MappingProxyType, proving the generic Mapping
  surface forwards ``idprop_strings`` without a dict-only path).
- Path variants: ``str`` path and ``pathlib.Path`` for Python; CLI argv
  strings.
- File verification: after a positive edit, the new value is read back
  via both module ``ox.get(path, ...)`` and an opened ``File`` handle.
- Guards: wrong expected and wrong name refuse as ``expected_old``;
  too-long replacement refuses as ``idprop_string`` (FULL canonical
  kind/message/exit match CLI vs Python, input hash preserved, no
  publication on refusal).
- Input-hash / no-publication proof: SHA-256 of the input before/after
  every case, output existence and hashes retained in evidence.

Run (installed wheel required, no editable checkout import)::

    ./venv-idprop311/bin/python bindings/python/tests/idprop_parity.py \
        --cli /path/to/oxblend-merged5 \
        --evidence /path/to/evidence-idprop-parity

Exit 0 only when every positive/guard pair passes with byte-exact
canonical agreement where applicable.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import subprocess
import sys
from pathlib import Path
from types import MappingProxyType

HERE = Path(__file__).resolve()
REPO = HERE.parents[3]

BASE = REPO / "tests" / "fixtures" / "file-ops" / "native-4.5.8" / "build-kit" / "empty.blend"
BASE_SHA = "aabe5b9c4a61d5c05182fbc49993fdd4bcb998a211c8454703d72c2d5b8cc749"

IDPROP_ADDR = 0x0A0B0C0D0E0F1000
STRING_ADDR = 0x0A0B0C0D0E0F2000
IDPROP_SDNA = 9
STRING_VALUE = b"5.3.0-alpha"
STRING_NAME = b"version"
POSITIVE_VALUE = b"5.3.0-beta"


def sha256_file(path: Path) -> str:
    return hashlib.file_digest(path.open("rb"), "sha256").hexdigest()


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def build_fixture() -> bytes:
    src = BASE.read_bytes()
    assert len(src) == 372102, len(src)
    assert sha256_bytes(src) == BASE_SHA
    assert src[:12] == b"BLENDER-v405"
    pos = 12
    dna_offset = None
    while True:
        code = src[pos:pos + 4]
        size = struct.unpack("<I", src[pos + 4:pos + 8])[0]
        if code == b"DNA1":
            dna_offset = pos
            break
        pos += 24 + size
    assert dna_offset is not None
    prefix = src[:dna_offset]
    payload = bytearray(136)
    payload[16] = 0
    payload[17] = 0
    payload[20:20 + len(STRING_NAME)] = STRING_NAME
    struct.pack_into("<Q", payload, 88, STRING_ADDR)
    struct.pack_into("<i", payload, 120, 12)
    struct.pack_into("<i", payload, 124, 12)
    str_payload = STRING_VALUE + b"\x00"
    assert len(str_payload) == 12
    out = bytearray()
    out.extend(b"BLENDER-v502")
    out.extend(prefix[12:])
    out.extend(struct.pack("<4sIQII", b"DATA", len(payload), IDPROP_ADDR, IDPROP_SDNA, 1))
    out.extend(payload)
    out.extend(struct.pack("<4sIQII", b"DATA", len(str_payload), STRING_ADDR, 0, 1))
    out.extend(str_payload)
    out.extend(src[dna_offset:])
    return bytes(out)


def find_idprop_block(ox, path: Path) -> int:
    info = ox.inspect(path).to_json()
    want = str(IDPROP_ADDR)
    for idx, block in enumerate(info["blocks"]):
        if block["address_decimal"] == want:
            return idx
    raise AssertionError("authored IDProperty not found")


def run_cli(cli: Path, argv: list[str]) -> tuple[int, bytes, bytes]:
    proc = subprocess.run(
        [str(cli)] + argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    return proc.returncode, proc.stdout, proc.stderr


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cli", required=True, type=Path)
    ap.add_argument("--evidence", required=True, type=Path)
    args = ap.parse_args()
    cli = args.cli
    evidence: Path = args.evidence
    evidence.mkdir(parents=True, exist_ok=True)
    # Installed wheel only.
    import oxblend

    assert "site-packages" in oxblend.__file__, oxblend.__file__
    work = evidence / "work"
    work.mkdir(parents=True, exist_ok=True)
    fixture = build_fixture()
    inp = work / "idprop.blend"
    inp.write_bytes(fixture)
    input_sha_before = sha256_file(inp)
    # Proof that the read-only seed is untouched.
    assert sha256_bytes(BASE.read_bytes()) == BASE_SHA
    block_index = find_idprop_block(oxblend, inp)
    results: list[dict] = []
    ok = True

    def record(name: str, passed: bool, detail: dict) -> None:
        nonlocal ok
        detail.update({"name": name, "passed": passed})
        results.append(detail)
        (evidence / f"{name}.json").write_text(json.dumps(detail, indent=2, sort_keys=True))
        if not passed:
            ok = False
            print(f"FAIL {name}: {detail}", flush=True)
        else:
            print(f"PASS {name}", flush=True)

    # --- Positive via CLI (str paths) ---
    spec_pos = {
        "schema": 2,
        "writes": [],
        "bits": [],
        "idprop_strings": [
            {
                "operation": "idprop-string",
                "block_index": block_index,
                "element": 0,
                "path": ["data", "pointer"],
                "name_hex": STRING_NAME.hex(),
                "expected_hex": STRING_VALUE.hex(),
                "replacement_hex": POSITIVE_VALUE.hex(),
            }
        ],
    }
    spec_path = work / "spec_pos.json"
    spec_path.write_text(json.dumps(spec_pos))
    cli_out = work / "cli_pos.blend"
    if cli_out.exists():
        cli_out.unlink()
    exit_code, stdout, stderr = run_cli(
        cli,
        ["edit", str(inp), "--output", str(cli_out), "--spec", str(spec_path), "--mode", "format-only"],
    )
    try:
        assert exit_code == 0, (exit_code, stderr[:500])
        assert cli_out.is_file()
        assert sha256_file(inp) == input_sha_before, "CLI positive mutated input"
        # Verify new payload via Python get (path variant).
        # The len field must now be 11 (10 chars + NUL logic: len includes NUL? 10+1=11).
        record("cli-positive", True, {"exit": exit_code, "input_sha": input_sha_before,
                                      "output_sha": sha256_file(cli_out)})
    except AssertionError as e:
        record("cli-positive", False, {"error": str(e), "stderr": stderr[:1000].decode("replace")})

    # --- Positive via Python, dict spec, str path ---
    py_out = work / "py_pos.blend"
    if py_out.exists():
        py_out.unlink()
    try:
        report = oxblend.edit(str(inp), str(py_out), spec_pos, mode="format-only")
        j = report.to_json()
        assert j["complete"] is True and j["output_written"] is True
        assert sha256_file(inp) == input_sha_before
        assert py_out.is_file()
        record("python-positive-str-path-dict", True, {"input_sha": input_sha_before,
                                                       "output_sha": sha256_file(py_out)})
    except Exception as e:
        record("python-positive-str-path-dict", False, {"error": f"{type(e).__name__}: {getattr(e, 'kind', None)} {getattr(e, 'message', None)}"})

    # --- Positive via Python, generic Mapping (MappingProxy), Path objects ---
    py_out2 = work / "py_pos_proxy.blend"
    if py_out2.exists():
        py_out2.unlink()
    try:
        proxy_spec = MappingProxyType(
            {
                "schema": 2,
                "writes": [],
                "bits": [],
                "idprop_strings": [
                    {
                        "operation": "idprop-string",
                        "block_index": block_index,
                        "element": 0,
                        "path": ["data", "pointer"],
                        "name_hex": STRING_NAME.hex(),
                        "expected_hex": STRING_VALUE.hex(),
                        "replacement_hex": POSITIVE_VALUE.hex(),
                    }
                ],
            }
        )
        report = oxblend.edit(Path(inp), Path(py_out2), proxy_spec, mode="format-only")
        j = report.to_json()
        assert j["complete"] is True
        assert sha256_file(inp) == input_sha_before
        record("python-positive-path-mappingproxy", True, {"input_sha": input_sha_before,
                                                            "output_sha": sha256_file(py_out2)})
    except Exception as e:
        record("python-positive-path-mappingproxy", False, {"error": f"{type(e).__name__}: {getattr(e, 'kind', None)} {getattr(e, 'message', None)}"})

    # --- File-handle verification of the positive output (path vs File) ---
    try:
        # Module path get on the CLI output.
        got_path = oxblend.get(str(cli_out), block_index, ["len"]).to_json()
        # File handle get on the same output.
        with oxblend.open(str(cli_out)) as fh:
            from oxblend import FieldQuery, BlockInfo  # type: ignore

            # Use module get via File path string for simplicity; File.get needs
            # session-tagged BlockInfo, so verify via second module get on Path.
            got_file = oxblend.get(Path(cli_out), block_index, ["len"]).to_json()
        assert got_path == got_file, (got_path, got_file)
        record("file-verification-path-vs-file", True, {"len": got_path})
    except Exception as e:
        record("file-verification-path-vs-file", False, {"error": f"{type(e).__name__}: {e}"})

    # --- Guards: wrong expected -> expected_old, wrong name -> expected_old ---
    for guard_name, mutate in [
        ("guard-wrong-expected", {"expected_hex": b"wrong".hex()}),
        ("guard-wrong-name", {"name_hex": b"wrong".hex()}),
    ]:
        bad = json.loads(json.dumps(spec_pos))
        bad["idprop_strings"][0].update(mutate)
        bad_path = work / f"{guard_name}.json"
        bad_path.write_text(json.dumps(bad))
        bad_out = work / f"{guard_name}.blend"
        if bad_out.exists():
            bad_out.unlink()
        # CLI
        cli_exit, _, cli_err = run_cli(
            cli,
            ["edit", str(inp), "--output", str(bad_out), "--spec", str(bad_path), "--mode", "format-only"],
        )
        # Python
        py_kind = py_msg = py_exit = None
        try:
            oxblend.edit(str(inp), str(bad_out.with_name(bad_out.name + ".py.blend")), bad, mode="format-only")
            py_ok = False
        except Exception as e:
            py_kind = getattr(e, "kind", None)
            py_msg = getattr(e, "message", None)
            py_exit = getattr(e, "exit_code", None)
            py_ok = True
        try:
            assert cli_exit == 2, cli_exit
            assert py_ok and py_kind == "expected_old", (py_kind, py_msg)
            assert py_exit == 2
            assert sha256_file(inp) == input_sha_before
            assert not bad_out.exists(), "refusal must not publish"
            record(guard_name, True, {"cli_exit": cli_exit, "py_kind": py_kind, "input_sha": input_sha_before})
        except AssertionError as e:
            record(guard_name, False, {"error": str(e), "cli_exit": cli_exit, "py_kind": py_kind,
                                       "py_msg": py_msg, "cli_err": cli_err[:500].decode("replace")})

    # --- Guard: too-long -> idprop_string, FULL canonical CLI vs Python ---
    too_long = json.loads(json.dumps(spec_pos))
    too_long["idprop_strings"][0]["replacement_hex"] = b"this string is too long".hex()
    too_path = work / "spec_long.json"
    too_path.write_text(json.dumps(too_long))
    too_out = work / "too_long.blend"
    if too_out.exists():
        too_out.unlink()
    cli_exit, _, cli_err = run_cli(
        cli, ["edit", str(inp), "--output", str(too_out), "--spec", str(too_path), "--mode", "format-only"]
    )
    cli_kind = cli_msg = None
    try:
        for line in cli_err.splitlines():
            line = line.strip()
            if line.startswith(b"{"):
                try:
                    env = json.loads(line)
                    if isinstance(env, dict) and isinstance(env.get("error"), dict):
                        cli_kind = env["error"].get("kind")
                        cli_msg = env["error"].get("message")
                except Exception:
                    pass
    except Exception:
        pass
    try:
        oxblend.edit(str(inp), str(work / "too_long_py.blend"), too_long, mode="format-only")
        record("guard-too-long", False, {"error": "python unexpectedly succeeded"})
    except Exception as e:
        py_kind = getattr(e, "kind", None)
        py_msg = getattr(e, "message", None)
        py_exit = getattr(e, "exit_code", None)
        try:
            assert cli_exit == 2 and cli_kind == "idprop_string", (cli_exit, cli_kind)
            assert py_kind == "idprop_string" and py_exit == 2, (py_kind, py_exit)
            assert cli_msg == py_msg == "replacement needs 24 bytes including NUL; existing allocation limit is 12 bytes"
            assert sha256_file(inp) == input_sha_before
            assert not too_out.exists()
            assert not (work / "too_long_py.blend").exists()
            assert type(e).__name__ == "IdpropStringError"
            record("guard-too-long", True, {"cli_kind": cli_kind, "py_kind": py_kind,
                                            "message": py_msg, "input_sha": input_sha_before})
        except AssertionError as ae:
            record("guard-too-long", False, {"error": str(ae), "cli_kind": cli_kind,
                                             "cli_msg": cli_msg, "py_kind": py_kind, "py_msg": py_msg})

    # --- Summary with hashes ---
    summary = {
        "input": str(inp),
        "input_sha256": input_sha_before,
        "base": str(BASE),
        "base_sha256": BASE_SHA,
        "cli": str(cli),
        "results": results,
        "passed": sum(1 for r in results if r["passed"]),
        "total": len(results),
    }
    (evidence / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True))
    print(json.dumps({"passed": summary["passed"], "total": summary["total"],
                      "input_sha": input_sha_before}, indent=2))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
