# Default trace structure (#62)

`FLATTRACE_DIR` alone selects level 1. At that level, `query.resolve`,
`query.project`, `read.byte_array`, `read.pointer_table`, `decode.frame` and
`decode.scan` emit **no per-call
span pairs**. Instead, one `hot.summary` point per event is emitted on the
nearest active structural owner immediately before it closes. Its span/parent
identify that owner (for example `walk.edges`, `walk.assets`, `query.evaluate`
or the CLI command); the owner's start record carries source/phase identity.
Calls without a thread-local structural owner accumulate on the process root
and are flushed at process completion, including worker-thread calls.

The point contains `event`, `count`, `total_ns`, `max_ns`, `bytes`,
`input_bytes`, `error_records`, `outcomes`. `error_records` reconciles retained `error` points
whose `hot_event` matches the aggregate on the same structural owner.
Durations are inclusive residence time measured by a monotonic clock; nested
resolve scopes are separate instrumented calls, so their summed duration is
not exclusive CPU time. Counts are instrumented calls, not unique pointers.
`bytes` sums successful byte-array output lengths, checked pointer-table
slot widths, successful decoded frame lengths, or actual decoded scan bytes
(including forward-seek discards and partial work on error). `input_bytes`
counts compressed frame bytes on successful frame decode; it is zero when
input-byte measurement is unavailable. Non-byte phases contribute zero. Outcome counts reconcile to
`count`. Nanosecond counters saturate only at their u64 representation bound.
Interrupted processes need not flush live owners; this is not a durable ledger.

Set **`FLATTRACE_LEVEL=2` explicitly** for the original per-call identities,
attributes, start/end timing and ancestry. This can be extremely large: it is
an investigative choice, not a default performance profile. Level zero remains
a no-op. Hot start payloads and resolved-path formatting are not constructed
at level 1. Failures/decisions and carried diagnostic references are not sampled
or removed: hot scope error points attach to the enclosing structural owner.
Product report bytes and diagnostic receipt contracts do not change.

## Inventory and boundaries

The inner-loop spans are pointer access (`query/access.rs`), address lookup
(`lib.rs`), lexical path resolution (`trace.rs`), record × field projection
(`query.rs`), raw array/table reads (`arrays.rs`), per-frame selective decode
(`seek_input.rs`) and per-range restarting stream scans (`trace_input.rs`). The CLI's historical
`query.resolve` wrapper also aggregates under its command owner. The stable
in-process tracing adapter uses the same event policy.

`query.select` and `walk.{file,select,edges,assets}` remain structural phases.
`query.library.open`, fetch/capture/reuse, pack copy/repath, hash, decode,
write/publication and durability remain per-file/member phases: the trace
continues to prove each actual input and publication. `node_tree.apply` remains
per explicit user-requested edit operation, not per internal field iteration.
File-level `decode` stays structural; detailed frame/scan records formerly
named `decode` are now `decode.frame`/`decode.scan` at explicit level 2.
The manifest's explicit `decode` work aliases let compressed-input conditions
accept file decoding, frame decoding or stream scans as real decoder evidence
(including `blocks`, which has no separate whole-file decode phase). EXR
scan worker detail was already explicit level 2. No span is emitted inside
pointer-table slot loops or block/field indexing loops.

## Consumers and regression gate

Both event declarations list `hot.summary`. The semantic checker accepts an
aggregate as evidence of its declared hot phase at the enclosing owner path,
after validating positive counts, durations, outcome totals and one point per
owner/event. It still checks the real envelope's ancestry; it does not invent
span pairs. Consumers counting work must sum `data.count` rather than count
`query.resolve` starts at level 1. Existing per-call consumers may opt into
level 2. Report/diagnostic_ref and guidance consumers require no schema change.

`scripts/check-trace-structure.py` runs the actual CLI at default and explicit
level 2. It forbids inner-call spans at level 1, bounds aggregate cardinality by
observed structural owners × declared hot phases, and reconciles aggregate
counts with deeper per-call counts. No arbitrary trace byte allowance is used.
`scripts/qualify.sh` always runs this gate, even without the optional reference
checker installed. Its all-target tests also amplify real pointer/array work
without new owners and reconcile compressed scan/frame aggregates with level 2.
The native-array oracle `check-array-native.py` explicitly selects level 2
because it checks individual array call identities.
