# Native diagnostic hook and exact failure carrier

This is an internal, invocation-local adapter, not a global last-failure store.
The stable public `Error.kind`, `Error.message`, `Display` and CLI exits are
unchanged. The non-exhaustive native Error has a private, lazily initialized
reference; the wasm-read-only Error has no diagnostic storage or dependency.

## Owner interface

```rust
use oxblend::failure_receipt::{
    record_source_failure, SourceContext, Namespace, Resolution,
};
let mut source = SourceContext::block(&file, block);
source.element = Some(element);
source.namespace = Namespace::Id;
source.resolution = Resolution::Missing;
let failure_id = record_source_failure(
    &error, "pointer.namespace.lookup.missing", source, &[],
);
```

`record_source_failure(&Error, &'static str, SourceContext, &[String]) -> String`
returns the actual local ID, or empty if capture is unavailable. Supply real
child IDs only when the owner knows the relation. Do not manufacture ancestry
IDs, infer edges from ordering, or guess a target/namespace. The CLI initializes
capture once; normal library calls without initialization create no receipt.

`SourceContext` is Default and has optional file, block index/code, old address,
SDNA index, count, payload size, decoded data offset, element, field offset,
decoded file offset, pointer width, Blender version/endian and pointer value;
byte field components and namespace/resolution are explicit. Use the builder
and Default so newly available optional coordinates are additive.

`SourceContext::block` uses the reader's path captured at its real open/capture
boundary. It never reopens that path. A path is its saved/opened spelling, not a
verified content identity. The exporter never follows a path found in evidence.

## Private carrier: no post-hoc identity guessing

Operation/path presentation keeps the first producer message as private typed
provenance, beside the existing public formatted message. A Session's first-use
library failure can therefore project the same report message as path inspection
without parsing prefixes or replacing the Error. Formatting, cloning, projection
and reclassification retain the native reference; this is not error deduplication.

The first owner hook tags the actual Error with an immutable native reference
`{run_id, failure_id}`. Error cloning and the internal `reclassify` operation
retain that reference. A later forwarding hook enriches the *same* record by
that exact reference, retaining the original check and namespace; its kind,
check and context are a `propagations` observation, not another failure.
Only missing coordinates at the same captured file/block/pointer are enriched;
a distinct forwarding frame is retained separately, never used to overwrite a
known original fact.

`TraceFailure` carries that reference and emits an additive `diagnostic_ref` in
its local JSON. The typed report collector reuses an ID only if invocation,
failure ID and original/explicitly propagated kind all match. It never uses
nearest/last/message matching. Foreign, missing or kind-mismatched references
cannot adopt another failure. Repeated serialized report views are deduplicated.

The command owner explicitly calls `record_terminal` with the exact emitted
report's collected cause IDs (returned by the shared CLI deliver_json owner). A direct propagated Error can also link to
its carried source ID. A later IO/broken_pipe cannot inherit unrelated scene
errors merely because those errors came first. Unknown relations remain absent.

## Instrumented sites

| Owner | Check IDs | Meaning |
| --- | --- | --- |
| `file_ops/index.rs::Index::lookup` | `pointer.namespace.lookup.missing`, `.ambiguous`, `.wrong_namespace`, `.forwarded` | Actual selected Id/OwnerData/QualifiedCrossData lookup; no cross-namespace fallback |
| `value.rs::resolve_pointer_hop` | `pointer.resolve.forwarded` | Typed saved field path, element and decoded offsets enrich the carried lookup failure |
| `lib.rs::dereference` | `pointer.namespace.lookup.missing`, `.ambiguous` | Explicit GlobalDiagnostic lookup, not ID-role qualification |
| `trace.rs` active forwarding ancestry check | `linked_id.forwarding.cycle` | Namespace Id, resolution Unknown, current file/block/element, no fabricated pointer or field path |

Uninstrumented checks still have catalog explanations and retain typed producer
report fields. Exported coverage is explicitly partial/unavailable. Candidate
headers, observed target, declared type and content identity are null and listed
unavailable when not captured. No exporter-time reconstruction supplies them.

## Stream and trace binding

All CLI stdout goes through the actual OS write boundary below stdlib buffering;
only accepted bytes are hashed. The receipt follows attempted product output,
so output_written, durability and incomplete stream delivery remain independent.
The stderr digest covers the compact original schema-1 envelope **without**
`diagnostic`, plus newline; ancillary stderr is outside that digest. The same
complete receipt plus SHA-256 is retained in `diagnostic.receipt` before finish.
Final stderr delivery is separately recorded in root.summary when possible.

L5 base: `32c4313e86d6aef0440240e9cc93909a6fca0e2b`, directly descended from
integration/next `f82aa4f`. No cherry-picked L5 copies remain on this lane. Its stable identity is
`{trace, root_span, identity_kind:"trace_root_span"}`, or null with tracing off.
The product's random `diagnostic.run_id` is an invocation nonce, not an alias for
flattrace's envelope `run`. --run-id selects only that product namespace. Root
completion means the recorded root scope ended, not a claim to have observed
an eventual process signal. Exit is the producer's selected exit; the caller
must still retain the actual return status.

New points: `diagnostic.invocation`, `diagnostic.source`,
`diagnostic.propagation`, `diagnostic.receipt`. L5's automatic Span::result
error-kind propagation points are preserved. L5 owns root output/resource
measurements; L8's report member/control facts live under root.summary.report.
See `flattrace-l8-additions.json` for L5/L9's manifest handoff.
