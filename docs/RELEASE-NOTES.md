# Oxblend 1.0.0

The first release reads, inspects, traces, packs and makes guarded edits to serialized Blender files without Blender evaluation.

- Read raw, gzip and zstd containers using each file's own SDNA.
- Inspect typed fields, pointer targets, serialized overrides and EXR header values.
- Trace supported dependency families conservatively, including linked IDs and object data. Report missing libraries and linked IDs as incomplete rather than inventing success.
- Create directory or ZIP packs with explicit missing-file diagnostics, no-replace publication and selectable durability.
- Make expected-old path, fixed-width typed and masked-bit edits to new outputs. No pointer writes or unrestricted setters.
- Use the optional low-memory reader when memory pressure matters more than speed.

The CLI has 18 commands. See [USAGE](USAGE.md), [STABILITY](STABILITY.md) and the [agent guide](../AGENTS.md). Code is MIT; documentation is CC BY 4.0.

## Limits

Only supported serialized structures and dependency families are interpreted. Unsupported or damaged layouts refuse conservatively; completeness is not Blender evaluation or render equivalence. Pre-2.5 files are not supported. Typed edits require a little-endian header version of at least 502; composition field profiles are qualified on native 5.2.1. Generic structural editing of existing graphs is not part of 1.0.

The Linux x86_64 binary dynamically links libc. Other platforms and universal/static Linux compatibility are not claimed. CI is a separate follow-up with exact commit-SHA action pins and a verified run before public integration; local qualification and reproducible builds are available now.
