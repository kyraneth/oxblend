# Query ownership and interpretation decisions

The initial root-only read contract remains `query(&BlendFile, type, fields)`.
Source-aware query adds a narrowly scoped exception: obtain an available linked
node-tree's subtype from actual library bytes instead of guessing it from the
root stub or compositor context. No linked graph expansion or unrelated asset
walk is implied. Missing/unreadable libraries retain partial saved detail and
precise unavailable sites.

Library acquisition is two-phase because a retained input store may be backed
by a Vec: borrowing the root while mutably capturing another input is invalid.
The query first returns owned requests; the caller drops views, captures through
its existing byte owner, then supplies a read-only map for evaluation. Forwarding
can require another planning phase. A supplied capture failure is terminal for
that observation, not a retry request. CLI convenience owns temporary captures;
a Session uses its existing captured sources and latches. The existing tracing
name resolver owns local-first/stub-fallback selection in both routes.

Saved idnames and IDProperty reference targets remain saved facts even when
Blender specializes sockets, repairs undefined nodes or remaps overrides after
load. These differences are explicit qualification assertions, not silent
normalization or edited native goldens. Undefined saved nodes are marked in
query evidence. The legacy and item-based File Output models remain distinct;
unqualified legacy semantics stay unavailable. Absent engine settings never
become installed add-on defaults.
