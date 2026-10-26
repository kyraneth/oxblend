# Native Text storage flags

Authored by Zamil with Blender 5.2.1 LTS, factory startup, autoexec disabled.
Generator: `tests/support/make-text-flags.py` into a fresh output directory.

| Text | Saved flags | Native is_in_memory | Native is_dirty |
| --- | ---: | --- | --- |
| Embedded | 1029 | true | true |
| External | 1024 | false | false |
| ExternalDirty | 1025 | false | true |
| LoadedInternal | 1029 | true | true |

The isolated difference between dirty external1025 and dirty embedded1029 is
TXT_ISMEM (bit2, value4), not the dirty flag. Both external and internal Texts
retain serialized lines when the backing files are hidden; cached lines alone
therefore cannot decide dependency ownership. The Windows-looking historical
location on Embedded must be informational, never a required external file.
External and ExternalDirty remain disk-backed declarations and require
`external.txt` (the Rust regression creates that file in its private scratch).
No Text body is executed.
