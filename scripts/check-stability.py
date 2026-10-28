#!/usr/bin/env python3
"""Check STABILITY's source inventories; stdlib only, no engine execution.

Rust token inventory, not a Rust compiler. Unrecognized kind/exit expressions
fail closed, requiring this checker to be reviewed when their shape changes.
"""
import argparse
import json
import re
from pathlib import Path

TOKEN = re.compile(r'//[^\n]*|/\*[\s\S]*?\*/|(?:b?r)(?P<hash>\#*)"[\s\S]*?"(?P=hash)|b?"(?:\\.|[^"\\])*"|b?\'(?:\\.|[^\'\\])\'|[A-Za-z_][A-Za-z_0-9]*|[0-9]+|\S')


def tokens(path):
    text = path.read_text()
    result = [m.group() for m in TOKEN.finditer(text)
              if not m.group().startswith(('//', '/*'))]
    # Production inventory only: remove each #[cfg(test)] item, including
    # inline modules and #[path = ...] external test modules.
    while True:
        start = next((i for i in range(len(result))
                      if result[i:i+7] == ['#', '[', 'cfg', '(', 'test', ')', ']']), None)
        if start is None:
            return result
        at = start + 7
        while result[at:at+2] == ['#', '[']:
            at = close(result, at+1) + 1
        end = at
        while result[end] not in ('{', ';'):
            end += 1
        end = close(result, end) if result[end] == '{' else end
        del result[start:end+1]


def close(t, at):
    pairs = {'(': ')', '[': ']', '{': '}'}
    stack = [pairs[t[at]]]
    for i in range(at+1, len(t)):
        if t[i] in pairs:
            stack.append(pairs[t[i]])
        elif t[i] == stack[-1]:
            stack.pop()
            if not stack:
                return i
    raise ValueError('unbalanced Rust token group')


def first_arg(t, at):
    end = at
    while t[end] != ',':
        end = close(t, end)+1 if t[end] in ('(', '[', '{') else end+1
    return t[at:end]


def literal(t):
    return json.loads(t) if t.startswith('"') else None


def inventory(root):
    sources = {str(p.relative_to(root)): tokens(p) for p in sorted((root/'src').rglob('*.rs'))
               if not (p.stem.endswith('_tests') or p.stem == 'tests')}
    kinds, schemas, dynamic = set(), {}, []
    for path, t in sources.items():
        for i in range(len(t)):
            if (t[i:i+4] == ['Error', ':', ':', 'new'] and t[i+4:i+5] == ['(']
                    and t[max(0, i-3):i] != ['io', ':', ':']):
                arg = first_arg(t, i+5)
                if len(arg) == 1 and literal(arg[0]) is not None:
                    kinds.add(literal(arg[0]))
                elif arg[0] == 'if':
                    kinds.update(literal(v) for v in arg if v.startswith('"'))
                else:
                    dynamic.append((path, ' '.join(arg)))
            if path == 'src/file_ops/roles.rs' and t[i:i+2] == ['fail', '('] and (i == 0 or t[i-1] != 'fn'):
                arg = first_arg(t, i+2)
                if len(arg) != 1 or literal(arg[0]) is None:
                    raise ValueError('unrecognized roles failure: '+str(arg))
                kinds.add(literal(arg[0]))
            if t[i:i+2] == ['kind', ':'] and t[i+2].startswith('"'):
                kinds.add(literal(t[i+2]))
            if t[i] == '"schema"' and t[i+1:i+2] == [':']:
                value = t[i+2]
                # A leading integer token alone is not a JSON integer literal:
                # 1.0, 1e0, 1+0 and suffix/identifier expressions must not hide
                # behind the other schema-1 sites in the same source file.
                if t[i+3] not in (',', '}'):
                    raise ValueError('unrecognized schema expression: '+path+' '+value)
                if value.isdigit() or value.startswith('"'):
                    key = (path, value)
                    schemas[key] = schemas.get(key, 0) + 1
                else:
                    raise ValueError('unrecognized schema expression: '+path+' '+value)
        # Serialized trace/override error-enum arms, including future variants.
        for i in range(len(t)):
            if t[i] in ('FailureKind', 'OverrideFailureKind') and t[i+1:i+3] == [':', ':']:
                at = i+4
                if t[at:at+1] == ['(']:
                    at = close(t, at)+1
                if t[at:at+2] == ['=', '>'] and t[at+2].startswith('"'):
                    kinds.add(literal(t[at+2]))
    # Forwarded kinds are already produced at their originating constructors.
    # Keep the reviewed forwarding forms exact, not a wildcard over variables.
    expected_dynamic = {
        ('src/file_ops/roles.rs', 'kind'),
        ('src/trace.rs', 'kind'),
        ('src/diff.rs', 'e . kind'),
        ('src/file_ops/graph.rs', 'e . kind'),
        ('src/file_ops/index.rs', 'e . kind'),
        ('src/file_ops.rs', 's . state'),
        # Compact census forwards a cached schema-program Error unchanged.
        ('src/file_ops.rs', 'e . kind'),
        ('src/pack.rs', 'e . kind'),
        # #31 copies a TraceFailure's existing serialized kind into the pack
        # refusal causes. kind_str mirrors TraceFailure.json's enum mapping;
        # Reader(k) stays k and LibraryReader remains library_read. No remap.
        ('src/pack.rs', 'failure . kind_str ( )'),
        # Typed-property context adds the source record/field without remapping
        # the originating checked-reader error kind.
        ('src/query/props.rs', 'e . kind'),
    }
    if set(dynamic) != expected_dynamic:
        raise ValueError('kind forwarding changed: '+repr(dynamic))
    return sources, kinds, schemas


def table(doc, name):
    match = re.search(r'<!-- '+name+r' -->\n(.*?)<!-- /'+name+r' -->', doc, re.S)
    if not match:
        raise ValueError('missing table '+name)
    rows = []
    for line in match[1].splitlines():
        if line.startswith('| `'):
            cells = [v.strip() for v in line.strip('|').split('|')]
            rows.append([re.findall(r'`([^`]+)`', cell) for cell in cells])
    return rows


def check(root):
    sources, kinds, schemas = inventory(root)
    doc = (root/'docs/STABILITY.md').read_text()
    rows = table(doc, 'error-kinds')
    documented = [v for row in rows for v in row[0]]
    if len(documented) != len(set(documented)) or set(documented) != kinds:
        raise ValueError('error-kind drift: missing='+str(sorted(kinds-set(documented)))+' extra='+str(sorted(set(documented)-kinds)))
    rows = table(doc, 'schemas')
    if any(len(r[0]) != 1 or len(r[1]) != 1 or len(r[2]) != 1
           or not r[2][0].isdigit() for r in rows):
        raise ValueError('schema drift: each row needs source, literal and literal-site count')
    declared_schema = {(r[0][0], r[1][0]): int(r[2][0]) for r in rows}
    if len(rows) != len(declared_schema) or declared_schema != schemas:
        raise ValueError('schema drift: source='+repr(schemas)+' docs='+repr(declared_schema))
    main = sources['src/main.rs']
    # Exit expressions are small enough to compare exactly: changes fail closed.
    exits = []
    for i in range(len(main)-6):
        if main[i:i+8] == ['std', ':', ':', 'process', ':', ':', 'exit', '(']:
            exits.append(main[i+8:close(main, i+7)])
    expected = [['if', 'error', '.', 'kind', '=', '=', '"pack_project"', '{', '5', '}', 'else', 'if', 'error', '.', 'kind', '=', '=', '"pack_input"', '|', '|', 'error', '.', 'kind', '=', '=', '"missing_input"', '{', '3', '}', 'else', '{', '2', '}']]
    if exits != expected:
        raise ValueError('exit dispatch changed: '+repr(exits))
    # Every run error must reach the dispatch, including broken_pipe. Bind the
    # complete handler prefix, so any early-return bypass fails this inventory.
    # The error object itself is owned by error_help::public_error (single
    # kind/message/next/prose shape, no diagnostic_ref inside, no snapshots);
    # the handler must still build a schema-1 base, finish the receipt and
    # record delivery.
    start = next(i for i in range(len(main)) if main[i:i+4] == ["fn", "main", "(", ")"])
    joined = " ".join(main[start:])
    prefix = 'fn main ( ) { let args = std : : env : : args_os ( ) . collect : : < Vec < _ > > ( ) ;'
    if (not joined.startswith(prefix) or 'return' in main[start:]
            or 'let result = span . result ( run ( & args ) ) ;' not in joined
            or 'if let Err ( error ) = result { let base = json !' not in joined
            or 'failure_receipt : : finish ( & error , & base , exit_code )' not in joined
            or 'let delivered = writeln !' not in joined):
        raise ValueError("error handler bypass/shape changed")
    rows = table(doc, 'exit-codes')
    declared = {r[0][0]: r[1] for r in rows}
    expected_rows = {'0': ['success'], '2': ['default_error'], '3': ['pack_input', 'missing_input'], '5': ['pack_project']}
    if len(rows) != len(declared) or declared != expected_rows:
        raise ValueError('exit-code table drift: '+repr(declared))
    print(f'STABILITY PASS: {len(kinds)} error kinds; {len(schemas)} schema emitters / {sum(schemas.values())} literal sites; 4 exit codes')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root)
    except (ValueError, IndexError, KeyError) as error:
        parser.exit(1, 'STABILITY FAIL: '+str(error)+'\n')


if __name__ == '__main__':
    main()
