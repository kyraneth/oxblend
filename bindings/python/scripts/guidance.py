#!/usr/bin/env python3
"""Mirror the engine-owned offline skill into the wheel; never a second authority."""
import argparse
import notices


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    source = notices.ENGINE / 'skills/oxblend/SKILL.md'
    target = notices.notice_default().parent / 'guidance/SKILL.md'
    body = source.read_bytes()
    if args.check:
        if target.read_bytes() != body:
            raise SystemExit('bundled Python skill drift; run bindings/python/scripts/guidance.py')
    else:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(body)
    print('Python bundled guidance matches engine-owned skill')


if __name__ == '__main__':
    main()
