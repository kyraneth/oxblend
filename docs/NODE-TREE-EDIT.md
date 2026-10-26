# Expected-graph compositor edits (1.4.0)

`node-tree FILE` reads compositor graphs for the new structural edit mode:

```sh
oxblend node-tree input.blend > graph.json
oxblend edit input.blend --output new.blend --spec transaction.json --mode node-tree --durability each-file
```

Inputs remain read-only. Output is a **new, raw/uncompressed** file, even when
input is compressed. This is a separate schema-1 structural contract; schema-2
fixed-width `format-only`/`comp` edits remain unchanged. It does not run Blender.

## Qualified boundary

Only native **Blender 5.2.1**, build `9e2066aef7ef`, FileGlobal subversion 44,
64-bit little-endian format 1, with the pinned template's exact DNA is admitted.
A header version or a caller's version claim alone is insufficient. The embedded
native template is `assets/template-5.2.1.blend` (SHA-256
`c8f5561e4d4c0fabf44a2c57ec46be6b8ba73b50f9b6bc50ae82d117480321e5`).
Its creation script is `scripts/make-node-tree-fixture.py`.

Supported **add-node constructors** are `ShaderNodeMath`, `ShaderNodeValue`,
`CompositorNodeRGB`, and color `NodeReroute`. These are compositor nodes in this
native version; legacy `CompositorNodeMath` is not silently translated. Nodes,
owned storage and sockets are cloned from this immutable native template, with
its saved settings/default values; no constructor is guessed from a type name.
The RGB prototype's saved color is `(0.2, 0.4, 0.6, 1)`. Other node constructors,
shader/geometry trees, linked trees and older/newer/unverified builds refuse.
Existing nodes may be addressed/removed with exact type guards independently
of constructor support; new socket construction is not exposed.

Only local, standalone `NT` compositor IDs are supported (not embedded legacy
Scene node trees). Tree nodes, sockets and links must have exact SDNA types,
single-record extents, valid owner runs and reciprocal lists. Reused allocation
addresses, duplicate node names/socket identifiers, multi-input links, cycles,
implicit socket-type conversions and dangling endpoints refuse. A whole-file
pointer/evaluation certificate is not claimed: untouched allocations, including
runtime caches, are preserved byte-for-byte.

Library paths are unchanged and retain the existing compositor editor's
same-library-content receiving guard. Packed/archive libraries refuse. Relative
asset paths are not repathed: moving a result to a different directory can
change their resolution. Use a same-directory new output or separately prepare
and verify a package; structural editing is not a relocation operation.

## Specification

Use **this exact input's** `trees[].block_index` for `tree`, and copy its entire
`trees[].graph` into `expected_graph`. The graph includes tree identity, nodes
(sorted by name), each ordered input/output socket identifier/type, and sorted
links with exact endpoints. It guards topology and endpoint identities, **not
scalar parameters, animation state, defaults or all file bytes**. Socket display
names/indices are not selectors. Every object rejects unknown/missing keys.

```json
{
  "schema": 1,
  "tree": 123,
  "expected_graph": {"name":"NTExample","node_type":"CompositorNodeTree","nodes":[],"links":[]},
  "operations": [
    {"op":"add_node","node_type":"NodeReroute","name":"Added"}
  ]
}
```

The numeric block above is illustrative, never reusable. Operations execute in
order, with graph validity rechecked after **each** operation:

| Operation | Exact additional keys |
|---|---|
| `add_node` | `node_type`, `name` (unique, fits native name buffer) |
| `remove_node` | `node_type`, `name` (must match saved type) |
| `add_link` | `from`, `to` |
| `remove_link` | `from`, `to` (the exact link must exist) |
| `relink` | `from`, `to`, `replacement_from`, `replacement_to` |

Each endpoint is exactly:
`{"node":"NAME","socket":"IDENTIFIER","socket_type":"NodeSocketColor"}`.
Source endpoints must be in the named node's outputs; targets in its inputs.
Their actual types must equal the declared type and one another. An occupied
input refuses `add_link`: explicitly remove/relink its guarded existing link.
No implicit replacement or implicit float/color conversion occurs. A relink
removes exactly one existing link and inserts its replacement; any refusal
abandons the in-memory transaction without publishing any intermediate state.

Removing a node removes its incident links. Node-owned unreferenced allocations
are retained, not garbage-collected, including when a node is added and then
removed within the same transaction. `removed_node_retention` reports the direct
socket/default/storage/property allocation roots by saved address and transaction
record index (not output block index); transitive storage/property children are
retained too. Socket disconnection can rewrite these retained records; such
original-record changes are included in `changed_original_blocks`. The explicit
`removal_allocation_policy` prevents these orphan islands being mistaken for a
full node-allocation destructor. `added_blocks` counts emitted live allocations,
not every intermediate allocation made by the transaction. Removal refuses any surviving typed pointer
to a removed allocation, and refuses animated trees (`adt != NULL`); animation
paths are never guessed, orphaned deliberately, or rewritten. Use native Blender
for those cases. Other scalar edits remain separate expected-old transactions.

## Publication and report

Source signatures are checked at capture and immediately before publication.
Original allocation order, addresses, headers and payloads stay unchanged except
for the reported changed/removed tree records. New DATA is inserted into exactly
the edited tree's owner run. The complete staged bytes are reread, reparsed and
the exact final graph checked before the existing atomic no-replace publication.
`none`, `end` and `each-file` retain their established persistence semantics.
Durability failure can leave `output_written:true` with `complete:false`; inspect
before retrying. Inputs/new-output files are never overwritten.

The schema-1 edit report includes `before`, `after`, `changed_original_blocks`,
`added_blocks`, `removed_node_retention`, `removal_allocation_policy`, `operations`,
exact input/output path hex, decoded lengths,
`layout_verified`, `graph_verified`, `unchanged_original_allocations_preserved`,
`container_family`, completeness/publication/durability and typed `errors`.
`native_verified:false` and `semantic_verified:false` are intentional: the
editor checks saved structure, not native evaluation or pixels. Inspection has
schema 1, `operation:"node_tree_inspect"`, `trees` and `complete:true` on success;
an unsupported/invalid graph refuses rather than inventing a partial guard.

Stable refusals include `node_version`, `node_expected_graph`, `node_constructor`,
`node_endpoint`, `node_socket_type`, `node_occupied`, `node_cycle`, and `node_graph`.
All refuse with exit 2 before publication; normal parser/pointer/publication kinds
can also occur. Constructor support is deliberately enumerated, not an arbitrary
pointer setter.

## Native receiving proof

Run the reproducible real-entry proof (Pillow required for decoded pixels):

```sh
python3 scripts/check-node-tree.py --binary /path/to/oxblend --blender /path/to/blender-5.2.1 --output /path/to/new-proof
```

It retains every stdout/stderr/exit, native graph/parameter dump, resaved file,
reference/control/reverse render and a hash-bound `ACCEPTANCE.json`.

The native oracle is `scripts/prove-node-tree-native.py`, launched in a separate
Blender with `--background --factory-startup --disable-autoexec --python-exit-code 1`.
Its arguments after `--` are `input|reference INPUT NEW_RESAVE JSON PNG`.
It opens the edited copy, dumps exact socket/link topology plus saved scalar
parameters, saves to a new path, reopens, asserts the same graph/parameters, and
renders. Reference mode performs the equivalent operations through native RNA.
Compare decoded RGBA bytes, not PNG metadata or RGBA `getbbox()` (which can ignore
RGB differences when the alpha difference is zero). Require the original control
to differ, so matching renders cannot certify an accidental no-op.

This profile's qualification covers native open/resave graph/parameter equality,
exact reference pixels, reverse remove/relink graph/render roundtrip and the four
typed unsupported/type/dangling/unverified negatives. It does **not** qualify
Blender 5.3/4.5, every compositor type, production assets or animation.
