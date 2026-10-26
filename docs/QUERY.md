# Saved-record query and concept discovery

`query`, `catalog` and `explain` share the Rust reader; they do not start Blender.
A query opens its input once, selects every matching saved struct element in
physical block/element order, and returns owned results. Reuse the Rust
`query::query(&BlendFile, type_name, fields)` entry on an already-open reader for
multiple selections without reopening it.

```sh
oxblend catalog --json
oxblend explain scene.compositor --file scene.blend --json
oxblend explain RenderData --file scene.blend --json
oxblend query scene.blend --type Scene --fields r.sfra,r.efra,r.frs_sec_base
oxblend query scene.blend --type Scene --fields render,view_layers,compositor,id_properties
```

Fields are exact authored concept aliases or exact embedded SDNA paths, not
fuzzy RNA names. Dot-separated numeric components index flattened SDNA arrays.
Raw values use the same byte-safe tagged values as `get`: integer decimal
strings, exact float bits, byte hex and saved pointer addresses. Raw pointers
are **not** implicitly traversed; use `pointer` and `get`, or a qualified compound
projection. Ordinary columns leave linked ID stubs as references. Only an
explicit `collection.graph` request expands collection/object content; the
separate narrow subtype lookup reads an available library's actual tree type.
A zero-match selection is a successful empty row array, not a default record.

The Scene columns are `render`, `view_layers`, `compositor`, `id_properties`;
canonical concepts are `scene.render`, `scene.view_layers`, `scene.compositor`,
`id.properties`. The catalog describes the saved paths and the limitations.
Semantic profile qualification is not inferred just from a header version or
the existence of an SDNA member. Unsupported mappings remain unavailable.

## JSON and partial results

The transport schema is [query.schema.json](query.schema.json). An undecodable
library filepath is an explicit `{kind:"bytes",hex,utf8:null}` reference, never
a local-ID null or replacement text. Exact property storage types/subtypes and
scalar float bits, plus authoritative ID name bytes, are separate
`storage_evidence`; they do not overwrite semantic property values.

The envelope discriminator is `oxblend.query/1`, distinct from product version,
`catalog_revision` and input header/subversion. `source_local_ref` is an identity
only within this input observation, not a permanent datablock identifier.
`source_observation` identifies the retained-reader observation; CLI adds the
native-byte input path. It is not a content hash. Both ends of the batch check
source identity; an observed input change discards the whole batch. Missing
optional FileGlobal subversion metadata is null with a precise
`source_observation.errors` site and `source_observation.complete:false`; it does
not erase independently provable raw rows. A semantic profile requiring that
metadata remains unavailable, so its requested coverage is incomplete.

Each requested field has either `status: available`, owned `value` and saved-path
`evidence`, or `status: unavailable`, an extensible `kind`, exact `context` and a
`next_action`. Independent rows and fields survive a requested gap. The report
sets `requested_coverage.complete: false`, lists the sites in `errors`, and the
CLI exits 2 **after writing the report**. Do not discard stdout on exit 2.
Structural/source integrity errors may refuse the whole observation. Output
write/flush failure is an error, never a complete report.

Unknown error kinds and additive envelope/evidence keys must remain consumable.
Existing names, types, saved ordering and byte rules do not silently change
within schema 1. Additional semantic fields require a catalog revision and
corresponding qualification; no arbitrary depth/node/slot truncation is allowed.

## Meaning boundaries

Saved controls are not evaluated results. These reports do not prove rendered
visibility, graph execution, image integrity, produced file paths, loaded OCIO
transforms, installed add-on defaults or native Blender open/render validity.
Disabled layers, excluded subtrees and unused local compositor groups are not
pruned. An absent explicitly stored setting stays absent. The legacy File Output
slot model and modern item model remain different representations, never an
invented common filename template. Diagnose a bad pointer; do not patch it to
bypass the reader's refusal.

## Explicit collection graph

```sh
oxblend explain collection.graph --file scene.blend --json
oxblend query scene.blend --type Scene --fields collection_graph
oxblend query publish.blend --type Collection --fields collection.graph
```

`collection.graph` (`collection_graph`) seeds a Scene's saved master collection,
or each selected Collection. It follows saved child/membership edges and object
parents through nested linked IDs. Its flat records/edges/sources are owned once
at `graphs["collection.graph"]`; each row contains only `{graph_ref,root}`.
Multiple memberships retain their saved edge ordinals and share a source-local
record, not duplicated bodies. A shared DAG is not a cycle. Source-local keys
are query-scoped, never durable IDs or globally unique saved addresses.

Library contexts are absolute lexical paths. An available root reader is reused
when a reference names that exact context; distinct lexical aliases remain
independent even when canonical provenance is equal. Source/record identity,
saved name bytes and library-chain provenance accompany the graph. Local/linked/
override ownership observations describe the containing saved file, not evaluated
root-scene identity. Raw object type/parent/control integers are not enum or
render-visibility interpretations. No studio category/material/sky policy lives
in this projection.

Unknown subtrees retain known records, edges and precise diagnostics, with an
unavailable row's `partial_value` still referring to the shared graph. Gaps in a
shared subgraph affect every row that reaches it. No missing body is replaced by
an empty collection, absent setting, fabricated parent or guessed reference.
Collection graph traversal does not change the four existing Scene columns:
those keep their original root-local content scope. Blender's RNA does not expose
some raw saved fields, including `Collection.owner_id`, `Collection.flag`,
`Object.partype` and `Object.restrictflag` in the captured 5.2.1 reference.
RNA exposes `Object.type` as an enum string, not its saved integer. These remain
raw saved facts, **not native-qualified RNA comparisons**. An unexposed RNA attribute is a declared oracle coverage gap,
never a null/zero default or a claim that the saved field is absent. The deep
chain test covers iterative diagnostic reachability metadata; the serialized
traversal has cycle/dangling/DAG controls, not a deep-chain native qualification.

## Library ownership and source acquisition

The CLI uses `query_with_source` for explicitly requested collection-graph
content and for a linked node-tree subtype that its root stub cannot store. Each absolute lexical
library path is retained once per query. Distinct lexical aliases keep their own
relative-library bases and source latches; realpath identity appears only in
provenance and never collapses those contexts. The same tracing name resolver selects local
IDs first, forwarding stubs only when no local ID matches. Missing/unreadable
libraries produce exact site diagnostics and retain the saved graph as
`partial_value`; no subtype is guessed from a compositor context.
`linked_subtype_observations` keeps subtype-only evidence; generic
`linked_id_observations` also records collection-graph ID resolution. Evidence
identifies local-stub versus actual-library facts, physical path, source-local
target and byte-owner provenance. Neither operation loads unrelated assets.

Session callers use two phases: `library_requests` returns owned requests;
drop root/store borrows and capture them in the existing input store; then
`query_with_libraries` evaluates against a read-only `LibraryMap`. Repeat planning
after captures until requests are empty, since a library may forward to another
stub. Supply failed captures too so they are not automatically retried. The
Session owns source-change latches and checks its store around both phases.
The core checks the supplied snapshots; it never reparses a Session-owned input.
`query()` without source context leaves relative-library subtype/graph bodies
unavailable while retaining provable root facts. The two-phase API is shared by
both explicit acquisition scopes; it is not another resolver or byte store.

## Pointed-array reads

Catalogue revision 2 also describes the two generic checked read operations
`BlendFile::byte_array` and `BlendFile::pointer_table`. Their CLI modes are
`get ... --byte-array COUNT` and `get ... --pointer-table COUNT`. See
[ARRAY-READS.md](ARRAY-READS.md) for declared-type, null, count, extent, namespace
and source-change guarantees. These operations read the selected file only;
they do not implicitly load a linked target's body.

## Saved facts versus native-reopen differences

The saved/reopened native suite deliberately also reveals load-time behavior.
Do not rewrite saved facts to force native-runtime equivalence:

- Saved `NodeSocketInt`, `NodeSocketFloat` and `NodeSocketVector2D` idnames can
  become native Pixel/Pixel2D socket classes on load. The report retains the
  actual saved idname. This appears in the 4.2/3.6 compositing libraries,
  lone-monk, shading-3.6 and splash-4.2 resaves.
- Saved `Undefined[CompositorNodeTexture]` becomes native `NodeUndefined` on
  reopen in shading-3.6. The saved type remains unchanged; `evidence` explicitly
  marks these nodes `undefined_in_saved_file:true`.
- In `splash-3.4-multilayer-resaved-5.2`, 16 IDProperty reference-library leaves
  differ: saved local override references are remapped to linked library
  references on native reopen. The reader preserves the saved local targets,
  not the post-load remapping. Source/type/name/storage evidence stays separate.

Qualification asserts these exact differences against untouched native goldens,
not a broad skip or a replacement expected file. These are not older-version
qualification: the resaves qualify only their new saved 5.2 representation.

## Errors and next steps

- `incomplete_query`: keep partial stdout; follow each unavailable outcome's
  `next_action`, preferably `explain CONCEPT --file INPUT --json`.
- `unknown_concept`: use `catalog --json`, or file-aware `explain TYPE` with an
  exact SDNA type from `inspect`.
- `unverified_semantics`: mapping/profile has not been qualified; inspect the
  raw saved paths without inferring defaults or modern meanings.
- `linked_reference_only`: only a linked stub was saved in this input; explicitly
  open its owning library if authorized. There is no implicit network/file walk.
- `query_cycle`, `query_list`: saved list/tree structure is cyclic or inconsistent;
  independent outcomes remain, but that projection is not complete.
- Existing `missing_field`, `type`, `dangling_pointer`, `ambiguous_pointer`,
  `pointer_target_type`, `encoding`, `input_changed` and bounds errors retain
  their checked-reader meanings. A display decoding failure is never replacement
  UTF-8 or a fabricated value.
