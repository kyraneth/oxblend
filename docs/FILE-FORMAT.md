# Serialized Blender data

A `.blend` file carries a BLENDER header, ordered BHead blocks, DNA1 schema metadata and ENDB. Headers describe pointer width, endianness and saved version; the reader supports the legacy and native format-1 header grammars. Physical raw/gzip/zstd containers are distinguished from decoded bytes.

SDNA stores names, type lengths and struct field declarations. Checked layout arithmetic rejects overflow, invalid declarators, inconsistent TLEN and recursive by-value containment. A non-empty by-value field cannot have zero element size; zero-count arrays remain supported. Pointer types are not permission to follow an arbitrary equal-valued address.

Use inspect to discover block indices/schema in the exact input, then get/pointer to query fields. Hex strings preserve raw byte names and paths. A version number alone is not proof of a saved layout or writable profile. Byte edits, diagnostic queries and graph import have distinct qualification boundaries.
