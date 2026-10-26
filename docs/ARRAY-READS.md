# Checked pointed-array reads

These are generic saved-data reads, not material/visibility policy. They do not
start Blender, expand linked IDs, reinterpret arbitrary payloads or repair data.

## Core API

Methods on `BlendFile`, using the same referencing-record order as `get`:

```rust
pub fn byte_array(&self, block_index: usize, element: usize,
                  path: &[PathStep], count: usize) -> Result<Vec<u8>>;
pub fn pointer_table(&self, block_index: usize, element: usize,
                     path: &[PathStep], count: usize) -> Result<Vec<Option<usize>>>;
```

- `byte_array` accepts one declared `char*`, `uchar*` or `int8_t*`, with a saved
  one-byte primitive width. It returns exact owned bytes, including NUL bytes;
  it does not apply C-string trimming, boolean conversion or text decoding.
- `pointer_table` accepts one declared `struct**` field. It retains index order
  and every null slot as `None`. Returned block indices belong to this reader;
  they are not raw addresses or portable datablock IDs. A linked ID target
  remains a checked stub, not an implicitly opened library body.
- Inline arrays of pointer fields must select a single pointer element through
  the normal field path. Triple pointers, void casts, primitive-pointer tables
  and guessed pointee layouts are not supported.
- `count` is an explicit number of requested items. Obtain it from the actual
  saved owner/count field, not a guessed default. A zero count returns an empty
  result after validating the field type; it need not dereference unused data.
- For a nonzero count, null/dangling allocations, count multiplication overflow,
  out-of-allocation reads, ambiguous namespaces and incompatible target types
  are typed errors. The actual allocation extent is checked **before allocating
  a result or reading its contents**; an adjacent block cannot extend it.
- A pointer-table entry is resolved through the original referencing record's
  source-role namespace, then its declared pointee type is qualified. There is
  no address-only or cross-owner fallback. Existing tracing resolution is not
  changed by these helpers.

The source is checked around each operation. An observed input change refuses
that operation, rather than returning part-old/part-new values. These guards are
observation checks, not a cryptographic content hash or a mutation lock.

## CLI

The existing `get` entry exposes the same operations:

```sh
oxblend get FILE BLOCK PATH_JSON [ELEMENT] --byte-array COUNT
oxblend get FILE BLOCK PATH_JSON [ELEMENT] --pointer-table COUNT
```

The two modes are mutually exclusive with each other and with `--raw`.
Ordinary `get` and its `--raw` behavior are unchanged. `BLOCK`, `ELEMENT` and
field names come from this exact file's `inspect`/file-aware `explain` output.

For example, after selecting an actual Object record and reading its saved
`totcol`, use that count for `["matbits"]` and `["mat"]`.
The first operation reads slot-selection bytes; the second reads nullable
material references. The caller—not this generic API—decides object-versus-data
slot ownership and any shader/image policy.

Byte mode returns the normal schema-1 `value:{kind:"bytes",hex:...}`. Pointer-table
mode returns `value:{kind:"pointer_table",block_indices:[INDEX,null,...]}` under
the same outer schema. No byte or pointer entry is silently skipped. Checked
errors use the existing CLI error envelope and exit-code dispatch.

## Diagnostics

`read.byte_array` and `read.pointer_table` spans carry the referencing block,
element, exact field path and requested count. Normal file/source, depth,
type, extent and pointer errors retain their existing meanings. No new byte
resolver or private substitute for the shared root diagnostics is introduced.
