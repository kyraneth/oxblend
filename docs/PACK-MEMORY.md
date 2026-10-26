# Pack memory policy (#33)

The opt-in bounded policy remains available as `--memory fast|bounded` on pack and pack-sequence,
and `memory="fast"` on Python Sessions. Unknown values refuse as usage errors.

Fast (default) CLI pack and pack-sequence use signature-bound selective raw
windows for raw blends: read only needed raw ranges and retain no whole decoded
sources on that path. The signature includes dev/ino/length/mtime/ctime, not a
whole-source content hash or an atomic snapshot guarantee. Compressed blends
fall back to full decoded capture. Python/Rust Sessions retain captured sources
as before; their repeated-query lifetime is not the ephemeral CLI pack lifetime.

Bounded retains digests and parsed indexes instead of decoded sources. It
additionally re-reads and verifies a single working-body cache on reuse, latching
the existing source-changed refusal. Opaque files do not retain bodies. Blend
prefetch remains disabled. Cross-device fallback copies verified backing.
Working storage includes the largest source, indexes, active guards and owned
ranges: this is a retention policy, not a hard process memory ceiling.

Bounded trades more IO and slower runs for lower peak memory on large sequences.
Selected workload measurements showed 27–53% lower fast-path heap than the
previous implementation. These are workload observations, not universal memory
or timing guarantees, and not a fresh bounded-versus-fast comparison.

Same package member bytes and refusals are required; no repair or new admission
is implied. Reports and CLI failure receipts record `"memory":"bounded"` only;
absence means fast. Trace records the policy. Retained XXH64 digests are
accidental-change detectors, not adversarial integrity guarantees. Existing
transport signature guards remain; same-signature changes entirely between
verification and live compressed-container consumption are not excluded.
Decoder-unconsumed trailers are outside digest coverage.

Hot-path counters remain aggregated in hot.summary; new events must be declared
in both manifests. Quiet timing, package/report parity, mutation refusal and
bounded zero-retention checks remain separate evidence surfaces.
