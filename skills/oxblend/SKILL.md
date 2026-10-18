---
name: oxblend
description: Inspect serialized Blender files, trace assets, create guarded new-output edits and directory or ZIP packs without starting Blender.
---

# Oxblend

Use the selected `oxblend` binary and retain its version, stdout, stderr and exit status. Start with `--help`, inspect and trace before writing. See [usage](../../docs/USAGE.md) for all commands and options; keep this skill with its source/docs tree.

Inputs are read-only. Use a new output destination; never overwrite artist files. Trace completeness discovers supported references, not on-disk availability or exhaustive scene semantics. Pack can publish available members with missing assets and exit2; inspect missing/errors. An untrustworthy trace publishes nothing.

Handle byte paths as bytes/hex, not lossy decoded names. Use block indices only from the exact input. For edits, specify expected-old values and verify untouched byte ranges. Default durability does not promise crash persistence. Directory and ZIP packs are supported; remote uploads are not.

Read-only reflected state is not Blender evaluation. Reopen writes with the compatible Blender version and inspect exact edited RNA values and untouched sentinels before claiming receiving correctness.
