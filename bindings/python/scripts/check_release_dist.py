#!/usr/bin/env python3
"""Verify one oxblend release distribution directory.

This is the single packaging gate shared by CI (.github/workflows/release.yml)
and local release builds (bindings/python/scripts/build-release.sh), so the
local build and the workflow cannot drift apart.

What it proves about the bytes on disk:
- exactly one wheel and one sdist exist (nothing merged, nothing partial);
- the wheel filename carries cp311-abi3 and an audited manylinux x86_64 tag;
- the wheel payload contains the Python facade, the native extension, the
  typed-ness markers, the notices file and the bundled skill, plus a complete
  .dist-info whose WHEEL/Tag agrees with the filename;
- the sdist has a single top-level directory, the required members and a
  PKG-INFO for `oxblend`;
- public MIT metadata is correct: Repository URL github.com/kyraneth/oxblend
  is present, and the retired "private captured-session pilot" wording is
  absent from PKG-INFO and METADATA;
- both raw description bodies equal bindings/python/README.md plus exactly
  one writer-framing LF, including the absolute fallback img required when
  PyPI strips picture/source markup;
- the archived readme.file resolves to that same README body;
- SHA256SUMS (when present or requested) matches the actual bytes.

What it does NOT prove: that the wheel builds, imports or renders anywhere.
It reads bytes only. Installed-wheel tests live in the workflow.

Usage:
    check_release_dist.py --dist DIR [--sha256sums FILE] [--sdist-only]

Exit status: 0 when every check passes, 1 otherwise.
"""

from __future__ import annotations

import argparse
from email.parser import BytesParser
import hashlib
import re
import sys
import tarfile
import tomllib
import zipfile
from pathlib import Path, PurePosixPath

# Required sdist members, carried over from the proven python.yml gate. The
# top directory name is maturin's, so members match by suffix.
SDIST_REQUIRED_MEMBERS = (
    "pyproject.toml",
    "Cargo.toml",
    "bindings/python/Cargo.toml",
    "src/lib.rs",
    "python/oxblend/__init__.py",
    "python/oxblend/__main__.py",
    "python/oxblend/_native.pyi",
    "python/oxblend/py.typed",
    "python/oxblend/THIRD-PARTY-NOTICES.txt",
    "python/oxblend/guidance/SKILL.md",
)

WHEEL_REQUIRED_MEMBERS = (
    "oxblend/__init__.py",
    "oxblend/_native.pyi",
    "oxblend/py.typed",
    "oxblend/THIRD-PARTY-NOTICES.txt",
    "oxblend/guidance/SKILL.md",
)

REPOSITORY_URL = "https://github.com/kyraneth/oxblend"
RETIRED_WORDING = "Private captured-session pilot"
BINDING_README = Path(__file__).resolve().parents[1] / "README.md"
LOGO_ROOT = Path(__file__).resolve().parents[3] / "assets" / "logo"
# L21 handoff: PyPI strips picture/source but must retain this absolute img.
FALLBACK_IMG = b'<img src="https://raw.githubusercontent.com/kyraneth/oxblend/main/assets/logo/oxblend-logo.svg" width="160" alt="Oxblend logo">'


class Gate:
    def __init__(self) -> None:
        self.errors: list[str] = []

    def check(self, ok: bool, message: str) -> bool:
        if not ok:
            self.errors.append(message)
            print(f"FAIL: {message}")
        return ok

    def note(self, message: str) -> None:
        print(f"ok:   {message}")


def sha256_file(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def single_artifact(dist: Path, pattern: str, kind: str, gate: Gate) -> Path | None:
    matches = sorted(p for p in dist.rglob(pattern) if p.is_file())
    if not gate.check(len(matches) == 1, f"expected exactly one {kind} under {dist}, found {len(matches)}"):
        for extra in matches:
            print(f"      {extra}")
        return None
    gate.note(f"one {kind}: {matches[0].relative_to(dist)}")
    return matches[0]


def metadata_fields(payload: bytes, gate: Gate, where: str) -> str:
    text = payload.decode("utf-8")
    metadata = BytesParser().parsebytes(payload)
    _, separator, description = payload.partition(b"\n\n")
    expected = BINDING_README.read_bytes()
    # Core metadata has email-style headers and a message body. Maturin adds
    # exactly one terminating LF as writer framing, beyond the README bytes.
    # Release contract: no stripping or other normalization; missing
    # framing LF, two framing LFs, or any changed content byte MUST fail.
    matches = bool(separator) and description == expected + b"\n"
    gate.check(matches, f"{where} raw body must equal binding README bytes plus exactly one framing LF")
    gate.check(FALLBACK_IMG in (description or b""), f"{where} is missing the L21 absolute fallback img")
    gate.check(str(metadata.get("Description-Content-Type", "")).split(";", 1)[0].strip() == "text/markdown", f"{where} description is not text/markdown")
    if matches:
        gate.note(f"{where} README description SHA256 {hashlib.sha256(expected).hexdigest()}")
    gate.check(RETIRED_WORDING not in text, f"{where} still contains retired wording {RETIRED_WORDING!r}")
    gate.check(
        f"Repository, {REPOSITORY_URL}" in metadata.get_all("Project-URL", []),
        f"{where} is missing Project-URL Repository {REPOSITORY_URL}",
    )
    gate.check(metadata.get("Name") == "oxblend", f"{where} is missing Name: oxblend")
    gate.check(metadata.get("License-Expression") == "MIT", f"{where} is missing License-Expression: MIT")
    return text


def check_wheel(wheel: Path, gate: Gate) -> None:
    name = wheel.name
    gate.check("-cp311-abi3-" in name, f"wheel filename does not carry the abi3-py311 tag: {name}")
    gate.check(
        re.search(r"manylinux.*x86_64", name) is not None,
        f"wheel lacks an audited manylinux x86_64 tag: {name}",
    )

    with zipfile.ZipFile(wheel) as archive:
        names = archive.namelist()
        for member in WHEEL_REQUIRED_MEMBERS:
            gate.check(member in names, f"wheel is missing {member}")
        native = [n for n in names if re.fullmatch(r"oxblend/_native(\.[A-Za-z0-9_]+)?\.so", n)]
        if gate.check(len(native) == 1, f"expected exactly one native extension in the wheel, found {native}"):
            gate.note(f"native extension: {native[0]}")

        dist_infos = sorted({n.split("/", 1)[0] for n in names if "/" in n and n.split("/", 1)[0].endswith(".dist-info")})
        if gate.check(len(dist_infos) == 1, f"expected exactly one .dist-info directory, found {dist_infos}"):
            root = dist_infos[0]
            for required in (f"{root}/METADATA", f"{root}/WHEEL", f"{root}/RECORD"):
                gate.check(required in names, f"wheel is missing {required}")
            if f"{root}/METADATA" in names:
                metadata_fields(archive.read(f"{root}/METADATA"), gate, "wheel METADATA")
            if f"{root}/WHEEL" in names:
                wheel_meta = archive.read(f"{root}/WHEEL").decode("utf-8", errors="replace")
                gate.check(
                    re.search(r"(?mi)^Root-Is-Purelib:\s*false\s*$", wheel_meta) is not None,
                    "wheel WHEEL file does not mark a platform wheel (Root-Is-Purelib: false)",
                )
                tag_match = re.search(r"(?mi)^Tag:\s*(\S+)\s*$", wheel_meta)
                if gate.check(tag_match is not None, "wheel WHEEL file has no Tag: line"):
                    tag = tag_match.group(1)
                    gate.check(
                        "cp311-abi3" in tag and re.search(r"manylinux.*x86_64", tag) is not None,
                        f"wheel WHEEL Tag disagrees with the release contract: {tag}",
                    )


def check_sdist(sdist: Path, gate: Gate) -> None:
    with tarfile.open(sdist, "r:gz") as archive:
        members = [m.name for m in archive.getmembers() if m.isfile()]
    tops = sorted({name.split("/", 1)[0] for name in members})
    if not gate.check(len(tops) == 1, f"sdist must have one top-level directory, found {tops}"):
        return
    top = tops[0]

    for required in SDIST_REQUIRED_MEMBERS:
        gate.check(any(name.endswith(f"/{required}") for name in members), f"sdist is missing {required}")
    gate.note(f"sdist top-level directory: {top}")

    info_name = f"{top}/PKG-INFO"
    with tarfile.open(sdist, "r:gz") as archive:
        logos = sorted(path for path in LOGO_ROOT.iterdir() if path.is_file())
        gate.check(bool(logos), "source logo inventory is empty")
        for logo in logos:
            suffix = f"/assets/logo/{logo.name}"
            matches = [name for name in members if name.endswith(suffix)]
            if gate.check(len(matches) == 1, f"sdist needs one source logo: {logo.name}"):
                stream = archive.extractfile(matches[0])
                body = stream.read() if stream else None
                gate.check(body == logo.read_bytes(), f"sdist logo differs from source: {logo.name}")
        if gate.check(info_name in members, "sdist is missing PKG-INFO"):
            extracted = archive.extractfile(info_name)
            payload = extracted.read() if extracted is not None else b""
            metadata_fields(payload, gate, "sdist PKG-INFO")
        project_name = f"{top}/pyproject.toml"
        if gate.check(project_name in members, "sdist is missing root pyproject.toml"):
            stream = archive.extractfile(project_name)
            config = tomllib.loads(stream.read().decode("utf-8")) if stream else {}
            readme = config.get("project", {}).get("readme", {})
            if gate.check(isinstance(readme, dict) and isinstance(readme.get("file"), str) and readme.get("content-type") == "text/markdown", "sdist readme must be a Markdown file reference"):
                path = PurePosixPath(readme["file"])
                contained = not path.is_absolute() and ".." not in path.parts
                name = f"{top}/{path}"
                if gate.check(contained and name in members, "sdist readme.file must resolve to a contained regular archive member"):
                    stream = archive.extractfile(name)
                    body = stream.read() if stream else None
                    gate.check(body == BINDING_README.read_bytes(), "sdist readme.file resolves to the wrong README body")


def check_sha256sums(dist: Path, sums_path: Path, artifacts: list[Path], gate: Gate) -> None:
    if not gate.check(sums_path.is_file(), f"missing checksum manifest {sums_path}"):
        return
    listed: dict[str, str] = {}
    for line in sums_path.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        digest, _, rel = line.partition("  ")
        listed[rel] = digest
    gate.check(
        len(listed) == len(artifacts),
        f"{sums_path.name} lists {len(listed)} entries for {len(artifacts)} artifacts",
    )
    for artifact in artifacts:
        rel = str(artifact.relative_to(dist))
        if rel not in listed:
            gate.check(False, f"{rel} is not listed in {sums_path.name}")
            continue
        actual = sha256_file(artifact)
        gate.check(actual == listed[rel], f"{rel} hash mismatch in {sums_path.name}")
        if actual == listed.get(rel):
            gate.note(f"{rel}: {actual}")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dist", required=True, type=Path, help="distribution directory to verify")
    parser.add_argument("--sha256sums", type=Path, default=None, help="checksum manifest (default: DIST/SHA256SUMS if present)")
    parser.add_argument("--sdist-only", action="store_true", help="verify the sdist only (fast local loop)")
    args = parser.parse_args(argv)

    dist: Path = args.dist
    gate = Gate()
    if not gate.check(dist.is_dir(), f"not a directory: {dist}"):
        return 1

    sdist = single_artifact(dist, "*.tar.gz", "sdist", gate)
    if sdist is not None:
        check_sdist(sdist, gate)

    wheel: Path | None = None
    if not args.sdist_only:
        wheel = single_artifact(dist, "*.whl", "wheel", gate)
        if wheel is not None:
            check_wheel(wheel, gate)

    artifacts = [p for p in (sdist, wheel) if p is not None]
    sums_path = args.sha256sums if args.sha256sums is not None else dist / "SHA256SUMS"
    if artifacts and (sums_path.exists() or args.sha256sums is not None):
        check_sha256sums(dist, sums_path, artifacts, gate)

    print()
    if gate.errors:
        print(f"FAIL: {len(gate.errors)} packaging check(s) failed")
        for error in gate.errors:
            print(f"  - {error}")
        return 1

    print("PASS: packaging checks succeeded")
    for artifact in artifacts:
        print(f"{sha256_file(artifact)}  {artifact.relative_to(dist)}")
    if sdist is not None and wheel is not None:
        print()
        print("workflow_dispatch inputs for target=pypi (from this build):")
        print(f"  expected_wheel_sha256={sha256_file(wheel)}")
        print(f"  expected_sdist_sha256={sha256_file(sdist)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
