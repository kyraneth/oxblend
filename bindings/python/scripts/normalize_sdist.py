#!/usr/bin/env python3
"""Temporary maturin nested-project README relocation fix.

Cargo-generated sdists relocate pyproject.toml to a common dependency root,
but maturin 1.15.0 leaves project.readme.file relative to its former owner.
Only that archived string changes. Every other uncompressed tar byte is kept,
except the changed entry's size/checksum and required zero padding. Raw input
is never rewritten, output must be new, and gzip timestamps use the source
commit's SOURCE_DATE_EPOCH. Remove this workaround when unmodified maturin
sdists pass the README-description check in check_release_dist.py.
"""
from __future__ import annotations
import argparse
import copy
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys
import tarfile
import tomllib

# POSIX ustar header/block widths, not workload or source-size limits.
BLOCK = 512
SIZE_FIELD = slice(124, 136)
CHECKSUM_FIELD = slice(148, 156)


def normalize(source: Path, output: Path) -> dict[str, object]:
    if output.exists() or output.is_symlink():
        raise ValueError('output already exists; use a new archive path')
    epoch = int(os.environ['SOURCE_DATE_EPOCH'])
    # RFC 1952 MTIME is an unsigned 32-bit field.
    if not 0 <= epoch <= 0xFFFFFFFF:
        raise ValueError('SOURCE_DATE_EPOCH does not fit gzip MTIME')
    before = source.stat()
    compressed = source.read_bytes()
    after = source.stat()
    signature = lambda stat: (stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns)
    if signature(before) != signature(after) or len(compressed) != before.st_size:
        raise ValueError('source archive changed during capture')
    raw = gzip.decompress(compressed)
    with tarfile.open(fileobj=io.BytesIO(raw), mode='r:') as archive:
        members = archive.getmembers()
        names = [member.name for member in members]
        if len(names) != len(set(names)):
            raise ValueError('duplicate archive members are not an expected maturin layout')
        if any(PurePosixPath(name).is_absolute() or '..' in PurePosixPath(name).parts for name in names):
            raise ValueError('unsafe archive path')
        roots = {PurePosixPath(name).parts[0] for name in names if PurePosixPath(name).parts}
        if len(roots) != 1:
            raise ValueError('one maturin archive root required')
        root = next(iter(roots))
        by_name = {member.name: member for member in members}
        def payload(name: str) -> bytes:
            member = by_name.get(name)
            if member is None or not member.isfile():
                raise ValueError('required regular archive member absent: ' + name)
            stream = archive.extractfile(member)
            if stream is None:
                raise ValueError('unreadable archive member: ' + name)
            return stream.read()
        project_name = root + '/pyproject.toml'
        body = payload(project_name)
        config = tomllib.loads(body.decode('utf-8'))
        readme = config['project']['readme']
        if readme != {'file': 'README.md', 'content-type': 'text/markdown'}:
            raise ValueError('unexpected README metadata; remove/update the workaround rather than guessing')
        tool = config['tool']['maturin']
        if tool.get('sdist-generator', 'cargo') != 'cargo':
            raise ValueError('only the cargo generator retains this expected nested dependency layout')
        manifest = PurePosixPath(tool['manifest-path'])
        if manifest.is_absolute() or '..' in manifest.parts or manifest.parent == PurePosixPath('.'):
            raise ValueError('nested, contained maturin manifest path required')
        target = str(manifest.parent / readme['file'])
        binding_readme = payload(root + '/' + target)
        if payload(root + '/README.md') == binding_readme:
            raise ValueError('README is already correct; remove the now-unnecessary workaround')
        pattern = rb'(?m)^(readme[ \t]*=[ \t]*\{[ \t]*file[ \t]*=[ \t]*)"README[.]md"'
        matches = list(re.finditer(pattern, body))
        if len(matches) != 1:
            raise ValueError('unexpected maturin TOML spelling; refusing a broad rewrite')
        match = matches[0]
        start = match.start() + len(match.group(1))
        end = match.end()
        changed = body[:start] + json.dumps(target).encode('ascii') + body[end:]
        expected = copy.deepcopy(config)
        expected['project']['readme']['file'] = target
        if tomllib.loads(changed.decode('utf-8')) != expected:
            raise ValueError('normalization changed more than project.readme.file')
        member = by_name[project_name]
        if 'size' in member.pax_headers:
            raise ValueError('PAX size override is outside the expected small metadata-entry layout')
        header_offset = member.offset_data - BLOCK
        header = bytearray(raw[header_offset:member.offset_data])
        if len(header) != BLOCK or header[156:157] not in (b'0', b'\0'):
            raise ValueError('regular ustar data header required')
        if int(header[SIZE_FIELD].rstrip(b'\0 '), 8) != len(body):
            raise ValueError('tar size/header mismatch')
        size = format(len(changed), '011o').encode('ascii') + b'\0'
        if len(size) != SIZE_FIELD.stop - SIZE_FIELD.start:
            raise ValueError('metadata entry does not fit the POSIX tar size field')
        header[SIZE_FIELD] = size
        header[CHECKSUM_FIELD] = b' ' * (CHECKSUM_FIELD.stop - CHECKSUM_FIELD.start)
        checksum = format(sum(header), '06o').encode('ascii') + b'\0 '
        if len(checksum) != CHECKSUM_FIELD.stop - CHECKSUM_FIELD.start:
            raise ValueError('tar checksum does not fit the POSIX field')
        header[CHECKSUM_FIELD] = checksum
        old_end = member.offset_data + ((len(body) + BLOCK - 1) // BLOCK) * BLOCK
        if any(raw[member.offset_data + len(body):old_end]):
            raise ValueError('nonzero tar padding is outside the expected layout')
        padding = b'\0' * ((-len(changed)) % BLOCK)
        normalized = raw[:header_offset] + bytes(header) + changed + padding + raw[old_end:]
        # All other headers, payloads, PAX records and trailing bytes are copied
        # literally by construction. Confirm the archive's sole payload change.
        with tarfile.open(fileobj=io.BytesIO(normalized), mode='r:') as result:
            rebuilt = result.getmembers()
            if [item.name for item in rebuilt] != names:
                raise ValueError('archive member order changed')
            for old, new in zip(members, rebuilt, strict=True):
                expected_size = len(changed) if old.name == project_name else old.size
                if new.size != expected_size:
                    raise ValueError('unexpected member size change: ' + old.name)
                if old.isfile():
                    expected_body = changed if old.name == project_name else payload(old.name)
                    stream = result.extractfile(new)
                    if stream is None or stream.read() != expected_body:
                        raise ValueError('unexpected member payload change: ' + old.name)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open('xb') as stream:
        with gzip.GzipFile(filename='', fileobj=stream, mode='wb', mtime=epoch) as writer:
            writer.write(normalized)
    if source.read_bytes() != compressed:
        raise ValueError('source changed; inspect the new output before any retry')
    return {'source_sha256': hashlib.sha256(compressed).hexdigest(),
            'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest(),
            'changed_member': project_name, 'field': 'project.readme.file',
            'expected_old': 'README.md', 'replacement': target,
            'other_tar_bytes': 'preserved literally outside the changed entry header/payload/padding',
            'source_date_epoch': epoch}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(normalize(args.source, args.output), indent=2))
    except (OSError, ValueError, KeyError, tarfile.TarError, tomllib.TOMLDecodeError) as error:
        print('sdist normalization refused: ' + str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
