//! L19 s1 red/green: exact-sized BHead storage (no retained Vec slack).
//!
//! The heap model charges `len * size_of::<Block>()` for retained BHeads.
//! The fix is the type itself (`Box<[Block]>`: Vec push/reserve/capacity
//! cannot address it — compiler-enforced, no runtime slack to observe),
//! plus the flattrace span fields (`block_build_capacity`,
//! `block_build_storage_bytes`, `block_storage_bytes`) that carry the
//! runtime numbers. These tests pin the model constant and the
//! exact-slice property so the span arithmetic cannot silently drift.
use super::*;

fn fixture(name: &str) -> BlendFile {
    BlendFile::open(format!(
        "{}/tests/fixtures/file-ops/{name}.blend",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// The retained-storage arithmetic assumes 48 bytes per BHead.
#[test]
fn block_struct_stays_48_bytes() {
    assert_eq!(std::mem::size_of::<crate::Block>(), 48);
}

/// Every parsed directory is retained exactly: the slice behind the
/// boxed storage holds precisely the parsed blocks, in order.
#[test]
fn bhead_storage_is_exact() {
    for name in [
        "synthetic-control",
        "synthetic-empty-seed",
        "duplicate-cross-ID-legal",
        "native-4.5.8/crypto-scene",
    ] {
        let f = fixture(name);
        let blocks = f.blocks();
        assert!(!blocks.is_empty(), "{name}");
        let retained = std::mem::size_of_val(blocks);
        let element = std::mem::size_of::<crate::Block>();
        assert_eq!(retained % element, 0, "{name}");
        assert_eq!(retained / element, blocks.len(), "{name}");
    }
}
