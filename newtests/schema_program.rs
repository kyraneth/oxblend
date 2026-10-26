use crate::{
    BlendFile,
    file_ops::{PROGRAM_BUILDS, pointer_layout, pointer_sites, walk},
};

fn fixture(name: &str) -> BlendFile {
    BlendFile::open(format!(
        "{}/tests/fixtures/file-ops/{name}.blend",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
fn oracle(
    f: &BlendFile,
    block: usize,
) -> crate::Result<Option<Vec<crate::file_ops::PointerLayout>>> {
    let b = f.block(block)?;
    if b.code == *b"DNA1" || b.sdna_index == 0 {
        return Ok(None);
    }
    let si = f.struct_index(block)?;
    let size = f.schema().lengths[f.schema().structs[si].type_index];
    if crate::checked::mul(size, b.count)? != b.size {
        return Err(crate::Error::new(
            "record_extent",
            format!("block {block}: typed count/length mismatch"),
        ));
    }
    let mut out = Vec::new();
    for i in 0..b.count {
        walk(
            f,
            si,
            crate::checked::mul(i, size)?,
            &format!("[{i}]"),
            &mut out,
        )?;
    }
    Ok(Some(out))
}
#[test]
fn structural_program_builds_do_not_scale_with_records_or_blocks() {
    let mut f = fixture("synthetic-control");
    let block = f
        .blocks()
        .iter()
        .position(|b| b.sdna_index != 0 && b.code != *b"DNA1" && b.count > 0)
        .unwrap();
    let size = f.blocks[block].size / f.blocks[block].count;
    f.blocks[block].count = f.blocks.len();
    f.blocks[block].size = size * f.blocks.len();
    PROGRAM_BUILDS.with(|n| n.set(0));
    let first = pointer_layout(&f, block).unwrap().unwrap();
    for _ in 0..f.blocks().len() {
        let next = pointer_layout(&f, block).unwrap().unwrap();
        assert_eq!(format!("{first:?}"), format!("{next:?}"));
    }
    PROGRAM_BUILDS.with(|n| assert_eq!(n.get(), 1));
    assert_eq!(
        format!("{first:?}"),
        format!("{:?}", oracle(&f, block).unwrap().unwrap())
    );
}
#[test]
fn compiled_sites_are_elementwise_equal_to_sorted_walk() {
    for name in [
        "synthetic-control",
        "duplicate-cross-ID-legal",
        "synthetic-empty-seed",
        "native-4.5.8/crypto-scene",
    ] {
        let f = fixture(name);
        for b in 0..f.blocks().len() {
            let old = oracle(&f, b);
            let new = pointer_layout(&f, b);
            assert_eq!(format!("{old:?}"), format!("{new:?}"), "{name} block {b}");
        }
    }
}
#[test]
fn failed_layout_has_no_iterator_and_extent_error_is_current_block() {
    let mut f = fixture("synthetic-control");
    let b = f
        .blocks()
        .iter()
        .position(|b| b.sdna_index != 0 && b.code != *b"DNA1" && b.count > 0)
        .unwrap();
    pointer_layout(&f, b).unwrap(); // populate structural program
    f.blocks[b].size += 1;
    let e = pointer_sites(&f, b).err().unwrap();
    assert_eq!(e.kind, "record_extent");
    assert_eq!(e.message, format!("block {b}: typed count/length mismatch"));
    let mut f = fixture("synthetic-control");
    let si = f.struct_index(b).unwrap();
    let ti = f.schema.structs[si].type_index;
    let field = &mut f.schema.structs[si].fields[0];
    field.pointer = false;
    field.array_len = 1;
    field.type_index = ti;
    for _ in 0..2 {
        let old = oracle(&f, b).err().unwrap();
        let new = pointer_sites(&f, b).err().unwrap();
        assert_eq!((new.kind, new.message), (old.kind, old.message));
    }
}
#[test]
fn hostile_nonmonotone_field_offsets_keep_stable_offset_order() {
    let mut f = fixture("synthetic-control");
    for st in &mut f.schema.structs {
        let offsets: Vec<_> = st.fields.iter().map(|field| field.offset).rev().collect();
        for (field, offset) in st.fields.iter_mut().zip(offsets) {
            field.offset = offset;
        }
    }
    for b in 0..f.blocks().len() {
        assert_eq!(
            format!("{:?}", oracle(&f, b)),
            format!("{:?}", pointer_layout(&f, b)),
            "block {b}"
        );
    }
}
