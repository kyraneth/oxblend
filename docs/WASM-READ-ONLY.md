# Single-file wasm32 groundwork

This is an experimental read-only Rust boundary, **not a shipped browser app**.
Native CLI behavior and stable APIs remain under the default `native` feature.

```
cargo build --offline --locked --target wasm32-unknown-unknown \
  --no-default-features --features wasm-read-only --lib
```

`native` and `wasm-read-only` are mutually exclusive. The CLI requires `native`.
The browser subset exposes `BlendFile::from_bytes(Vec<u8>)`, header, ordered
blocks, the source SDNA and checked payload slices. Its parser reuses the native
header/layout helpers and SDNA parser; its byte-backed block scan is intentionally
separate from native FD/seek/spill readers. The subset accepts **raw BLENDER
containers only**. Gzip/zstd, filesystem paths, cross-file dependencies, threads,
local flattrace, writers, packing and implicit pointer following do not compile
into this boundary. A browser decoder or JS binding is not yet provided.

## Host contract

The browser host reads only a file the person explicitly selects. It passes
owned bytes to a worker, receives owned display facts, then discards the worker
when closing the file. No automatic linked-file access, upload, telemetry, saved
scene persistence or network requests. Relative path spelling is displayed
without inventing a filesystem resolution. Saved values are not evaluated
Blender state. Unknown targets stay unknown. No repair controls are proposed.
Browser memory allocation/wasm32 addressability are platform constraints, not an
invented file-size budget; allocation failures must be presented honestly.

## Proposed UI and remaining work

The three screenshot concepts show one interface in Overview, Blocks & fields
and Diagnostics states. They use fictional demo facts and are labelled static.
The editable single-file HTML and rendered screenshot paths are retained in the
private lane acceptance evidence, not shipped with the crate. Nothing is hosted.

A browser product still needs a JS/Wasm ABI, worker lifecycle/error handling,
read-only typed field API, compressed-container decision, browser end-to-end
fixtures, and accessibility/large-file testing. Native-only exports must remain
unavailable, not runtime stubs. A wasm32 compilation proves separation and target
compatibility; it does not prove a browser application or Blender validity.
