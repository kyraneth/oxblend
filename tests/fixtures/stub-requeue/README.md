# Same-pass stub re-arrival (authored synthetic SDNA)

Zamil's independent GLM review F2 reproduction, no third-party scene data.
Root selects OBx from child.blend. In child, physical block order is:

- OBx@1 → aux OBp2@2
- OBp2@2 → aux stub OBorig@5 and aux2 OBovr@7
- stub OBorig@5 → Library@9 → nested.blend's real OBorig
- OBovr@7 → override definition@8 → reference stub OBorig@5 again

The stub is processed before the later override re-enqueues it. This is a
same-file/same-selection-pass shared edge, not a recursive forwarding cycle.
It must be a visited-cache no-op, not `cycle`. Both references retain their
exact source library; strict pack must copy all three files.

The active-stub prefix inherited from ancestor file passes is different:
re-entering that prefix is a genuine recursive forwarding loop and must still
report `cycle` and terminate. The existing `tests/stub_chain.rs` forwarding-cycle
negative is retained unchanged and runs beside this positive regression.
