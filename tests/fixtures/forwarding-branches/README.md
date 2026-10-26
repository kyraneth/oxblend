# Library-resolution branch lifetime (authored synthetic SDNA)

Zamil's graph counterexample extends the same-pass requeue regression.
`root` selects X/Y from `forward`; `forward.X` resolves to real `a.X` and
completes; `forward.Y` resolves to `b.Y`, whose dependency reaches X through
`forward.X` again. That is a completed-sibling diamond, not an unresolved alias
cycle. Keeping X active for the whole forward-file pass falsely refuses it.

Cases:
- diamond: B's X dependency forwards through F.X to the already-resolved A.X.
- direct-control: identical but B's X dependency links A directly.
- self-cycle: F.X → A.X (stub) → F.X; must report cycle and terminate.
- long-cycle: F.X → A.X (stub) → C.X (stub) → F.X; same requirement.

Active stubs are pushed for one forwarding-name resolution request and popped
before local endpoint dependencies expand. The previous same-pass visited-cache
correction and existing real-cycle tests remain. No native Blender producer is
claimed for these deliberately minimal SDNA graph fixtures.

The #35 regression `mixed-batch-known-35` batches X/Y into the same target A;
A.X is already a real endpoint while A.Y reaches it via F.X. Both this case and
`mixed-direct-control` (A.Y points directly at its full local A.X) must complete
trace/list/strict pack, with equal physical dependency sets. True self/long
forwarding-only cycles still refuse with no strict publication.
