# Changelog

## 1.4.1 — 2026-10-29

- Faster decoded-equal comparisons, indexed diff traversal and buffered decoded
  spill writes; compact schema-backed census retention reduces peak memory.
- `diff` stays oxblend.diff/v1. The container equality repair is the documented
  S1 behavior change; changed real comps still return incomplete/null.
- Matched shipped-1.4.0 measurements: container 279.22 → 0.06 s,
  rename 300.45 → 15.21 s, versions 163.39 → 12.32 s. See
  docs/RELEASE-NOTES.md for report sizes, memory and comparison limits.
- Very large or unrelated composition pairs still run out of memory in `diff`,
  including public Blender 5.2 splash versus unrelated and studio sh0419.
  Changed real comps still return incomplete/null. v2 answers, the default-format
  change and further verdict work come in 1.4.2.

## 1.4.0 — 2026-10-26

- Full typed Python API, shared 1.4.0 crate/wheel version, selective raw pack windows.
- Export the binding and reviewed manual trusted-publishing workflow.

## 1.3.3 — 2026-10-22

- Correct serialized Blender path handling, pointer-array reads, literal glob
  folders, output separators and ordinary permissions; refuse nonregular inputs.
- Add per-file walk attribution, level-2 dependency and pack detail, explicit
  fetch links, build identity and a stable opt-in in-process tracing API.
- Keep tracing best-effort after writer errors and preserve unknown automatic
  library run outcomes instead of fabricating success.
- Harden offline qualification, deterministic licence notices and public export
  completeness. See [release notes](docs/RELEASE-NOTES.md) for details and limits.

## Earlier releases

See [release notes](docs/RELEASE-NOTES.md), including 1.3.2 linked-library
compatibility and the earlier release history.
