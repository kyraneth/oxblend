"""Offline package guidance; no network or subprocess.

``python -m oxblend skill print`` emits the bundled version-matched agent and
CLI guide (``guidance/SKILL.md``) to stdout. It is the only ``python -m``
entry point; every other operation lives in the ``oxblend`` package API, and
the matching ``oxblend skill`` CLI command prints the same bytes from the
same bundled file.
"""
from importlib.resources import files
import sys

__all__ = ["main"]


def main() -> None:
    """Print the bundled offline skill guide to stdout.

    Args:
        None; the argument vector must be exactly ``skill print``.

    Returns:
        None.

    Raises:
        SystemExit: usage error (anything but ``skill print``).

    CLI: ``oxblend skill print`` prints the same bundled bytes.
    """
    if sys.argv[1:] != ["skill", "print"]:
        raise SystemExit("usage: python -m oxblend skill print")
    print(files("oxblend").joinpath("guidance/SKILL.md").read_text(encoding="utf-8"), end="")

if __name__ == "__main__":
    main()
