# Oxblend flattrace acceptance manifest v1

Machine contract: [flattrace-manifest.json](flattrace-manifest.json).
Oxblend consumes the existing flattrace **0.1** Writer API and v1 envelope.
No upstream tracing development or new collector is part of G7.

## Executable selection

Select one command, effective mode (report/strict/permissive), scenario and
explicit condition booleans per invocation. Conditions are executed fixture
selections, not prose waivers: compressed, hash, copy, durability, publish,
resolve, pack_execute, retained_reuse, dry_run, strict_refusal.
Success phase requirements apply independently to each run, never a union of
several invocations. Failure scenarios replace the success phase list; selected
conditions add required or forbidden phases. Unknown commands/scenarios fail.

An ordered span path is an ancestry path of **start** records in recorded
sequence. `**` means zero or more actual ancestors; `{command}` is substituted.
Each requirement consumes a distinct start, in order; parents must be active.
Ends must pair exactly once, after their children. Point events must have an
active enclosing span. Sequence is contiguous. Root summaries belong to root.
Caller checkpoints are labelled `summary_phase:checkpoint`; exactly one
`summary_phase:final` is immediately before root end. Final process resources
and coverage cannot be replaced by caller values. Per-input schema captures and
publication identities are consumed independently, including identical diff
arguments. Command outcomes/kinds are checked against product stderr/exit,
not inferred from the trace under test.

Required attributes are payload keys with JSON type checks (null is not a
measured number). Required errors are stable kind records, not message matches.
Root records requested/effective policy and source, input identities and build.
The 0.1 `finish(outcome)` API cannot attach root end data: a `root.summary`
point on the still-active root carries output inventory, resource facts and
diagnostic completeness. This is an Oxblend payload, not an upstream change.
Resource fields declare units/source/scope; unavailable facts use null + reason.
Input stat signatures are not called content hashes. In-memory captures used
for staged-output verification are explicit `memory.capture` scopes, not fake
file opens. Atomic publish data describes the publication moment; `end` sync
is still pending there. Subsequent `output.effect` records describe final
product-report effects after requested sync. ZIP member/staging/archive roles
remain distinct.

Error records distinguish primary source observations, propagation, and
`report.diagnostic` report observations. Warnings use `warning`, not `error`.
Only primary records belong in occurrence counts. Native Error values can carry
a private invocation-qualified diagnostic reference without changing public
kind/message/Display. Span-result propagation uses that exact reference when
present; otherwise it explicitly remains unattributed. Forwarding and serialized
report copies reuse the source identity, never nearest/last/message matching.
Repeated report views are not new incidents.

## Gate and mutation proof

The semantic checker must accept the real positive control and reject removed
or resequenced spans, missing/reordered required events, deleted/changed required
attributes, deleted error kinds, wrong parentage, duplicate ends and unfinished
runs. Mutations repair envelope sequence numbers: rejection must not merely
detect a sequence gap. Every member publication/skip reconciles item identity
with the independently parsed product report; pack-info cannot stand in for
another published member. Conditional dry-run/refusal forbids publication.
New command/API entry paths require their own manifest selection and proof.

## Native observation source mutation

`tests/native_input_facts.rs` exercises ordinary open and native physical-fetch
capture with empty CLI/root requested inputs. Run the committed counterfactual
gate from a checkout:

```sh
FLATTRACE_CHECKOUT=/path/to/unchanged/flattrace-0.1 scripts/check-native-input-mutation.sh
```

It archives HEAD into retained TMPDIR evidence, bypasses only fetch's
`observe_input` call, preserves the successful capture and identity payload,
then requires the native semantic test to reject the absent root input ledger.
The unchanged reference checker must still accept the envelope. Cargo stays
offline/locked and uses a distinct sibling of caller CARGO_TARGET_DIR (or an
evidence-local target), never the positive executable or source tree. The
printed evidence directory contains source hashes, logs, selection, trace and
`proof.json`. L4 still owns installed Session/binding receiving proof.

## Structural node-tree transactions

`node-tree` requires `run/node-tree/node_tree_inspect`, input capture and
complete serialization. Successful `edit --mode node-tree` additionally selects
the `node_tree_edit` condition: plan, capture, validation, emit, staged verify
under write, and publication. Every requested operation is reconciled separately
against its ordered apply span and owner-scoped `node_tree.operation` point;
empty transactions require neither. Constructor/type/endpoint/version refusals
retain typed causes and publish nothing. The CLI gate exercises these paths and
rejects removed apply, operation and verification records. Native Blender proof
remains separate in `scripts/check-node-tree.py`.

## Privacy and measurement

Public/default execution is off. A nonempty FLATTRACE_DIR with level 1/2 opts
into local detailed paths/argv; level 0 is off. No upload, whole environment
capture, credentials or default content hashing. Export requires review and
existing 0.1 redaction. Reversible hex path forms need explicit reviewed
`--path` additions too; the canary qualification covers plain and hex additions,
not automatic anonymization or arbitrary-payload safety. Enabled and disabled arms must preserve product output,
exit and effects. Repeat on/off measurements, record host load and resource
counters, retain noisy samples; no universal overhead assertion.

## Source and status

The approved contract preceded gate/instrumentation implementation.
The public manifests name the current event/span inventory.
`scripts/check-flattrace.py --selftest` checks seeded mutations;
its no-argument entry runs the real CLI plus unchanged reference checker via
`scripts/qualify-flattrace.py`. Full acceptance evidence is lane-owned; this
contract alone never establishes product, binding or native Blender correctness.
