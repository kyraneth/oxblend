//! Synthetic saved-file coverage for the generic readonly array readers
//! (`BlendFile::byte_array` / `BlendFile::pointer_table`).
//!
//! The fixture builder below writes a complete minimal `.blend` from scratch
//! (header, BHeads, SDNA) for each pointer width and endianness, so every
//! hostile is a real serialized shape refused at the reader boundary — no
//! mocks, no `assert!(true)` placeholders, no private fixture paths.
//!
//! Schema used by the synthetic DNA (mirrors the saved `totcol`/`**mat`
//! shape these helpers are approved for):
//!
//! ```text
//! Widget { char *label; int totcol; Part **parts; uchar *raw;
//!         Part *single; int *numbers; char *arr[2]; char **blob;
//!         int8_t *ibytes; }
//! Part   { ID id; int x; }        // embeds ID, like saved ID-embedding data
//! ID     { int a; int b; }
//! Other  { int x; }               // wrong-kind ID target
//! ```
//!
//! Widgets in the file:
//! - W1 @0x1000 (two records): positive core; record 0 owns a 3-slot table
//!   `[PartA, null, PartB]`, record 1 a 1-slot table `[PartB]`.
//! - W2 @0x9000: hostile pointers — dangling data address, a foreign owner's
//!   table address, and a data role whose address only an ID block matches.
//! - W3 @0x9500: table address repeated by two DATA blocks in its own scope.
//! - W4 @0xA000: table `[PartA, 0xDEAD]` — dangling entry at index 1.
//! - W5 @0xB000: table `[OB]` — resolvable ID of the wrong kind.
//! - W6 @0xB500: table `[0x8000]` — ambiguous duplicate ID target.
//! - W7 @0xC000: null table and null byte pointer with nonzero totcol.

use oxblend::BlendFile;
use oxblend::value::PathStep;

fn field(name: &str) -> PathStep {
    PathStep::Field(name.as_bytes().to_vec())
}
fn path(names: &[&str]) -> Vec<PathStep> {
    names.iter().map(|n| field(n)).collect()
}
fn element(name: &str, index: usize) -> Vec<PathStep> {
    vec![field(name), PathStep::Index(index)]
}
fn block_at(file: &BlendFile, address: u64) -> usize {
    file.blocks()
        .iter()
        .position(|b| b.old_address == address)
        .unwrap_or_else(|| panic!("no block at {address:#x}"))
}
fn err_kind<T>(result: oxblend::Result<T>) -> &'static str {
    match result {
        Err(e) => e.kind,
        Ok(_) => panic!("expected a typed refusal"),
    }
}

// ------------------------------------------------------------ fixture build

#[path = "support/pointed_arrays.rs"]
mod arrays_fixture;
use arrays_fixture::{synthetic, synthetic_declarators};

#[test]
fn callable_declarators_are_not_byte_or_struct_arrays_even_when_count_is_zero() {
    let file = BlendFile::from_bytes(synthetic_declarators(true, 8, true)).unwrap();
    let owner = block_at(&file, 0x1000);
    let fields = &file.schema().structs[0].fields;
    let bytes_path = vec![PathStep::Field(fields[0].name.clone())];
    let table_path = vec![PathStep::Field(fields[2].name.clone())];
    assert!(fields[0].full_name.contains(&b'('));
    assert!(fields[2].full_name.contains(&b'('));
    for count in [0, 1] {
        assert_eq!(
            err_kind(file.byte_array(owner, 0, &bytes_path, count)),
            "type"
        );
        assert_eq!(
            err_kind(file.pointer_table(owner, 0, &table_path, count)),
            "type"
        );
    }
}
fn synthetic_file(little: bool, ps: usize) -> BlendFile {
    BlendFile::from_bytes(synthetic(little, ps)).expect("synthetic file parses")
}

// ----------------------------------------------------------------- positives

#[test]
fn byte_array_preserves_embedded_zero_bytes() {
    let file = synthetic_file(true, 8);
    let w1 = block_at(&file, 0x1000);
    // First byte zero, embedded NULs at 2/5/6: raw payload, never a string.
    assert_eq!(
        file.byte_array(w1, 0, &path(&["label"]), 8).unwrap(),
        [0x00, 0x41, 0x00, 0x42, 0x43, 0x00, 0x00, 0x44]
    );
    assert_eq!(
        file.byte_array(w1, 0, &path(&["raw"]), 3).unwrap(),
        [0xFF, 0x00, 0x01]
    );
    assert_eq!(
        file.byte_array(w1, 0, &path(&["ibytes"]), 5).unwrap(),
        [0x80, 0x00, 0x7F, 0xFF, 0x00]
    );
}

#[test]
fn byte_array_reads_one_selected_pointer_array_element() {
    let file = synthetic_file(true, 8);
    let w1 = block_at(&file, 0x1000);
    assert_eq!(
        file.byte_array(w1, 0, &element("arr", 0), 4).unwrap(),
        [0x7A, 0x00, 0x7B, 0x00]
    );
    assert_eq!(
        file.byte_array(w1, 0, &element("arr", 1), 4).unwrap(),
        [0x63, 0x64, 0x00, 0x65]
    );
}

#[test]
fn count_zero_returns_empty_without_dereferencing() {
    let file = synthetic_file(true, 8);
    let w7 = block_at(&file, 0xC000);
    // Both pointers are null in W7; a zero count must not touch them.
    assert_eq!(
        file.byte_array(w7, 0, &path(&["label"]), 0).unwrap(),
        Vec::<u8>::new()
    );
    assert_eq!(
        file.pointer_table(w7, 0, &path(&["parts"]), 0).unwrap(),
        Vec::<Option<usize>>::new()
    );
}

#[test]
fn pointer_table_preserves_none_slots_and_index_order() {
    let file = synthetic_file(true, 8);
    let w1 = block_at(&file, 0x1000);
    assert_eq!(
        file.pointer_table(w1, 0, &path(&["parts"]), 3).unwrap(),
        vec![
            Some(block_at(&file, 0x5000)),
            None,
            Some(block_at(&file, 0x6000))
        ]
    );
}

#[test]
fn pointer_table_reads_the_selected_elements_own_table() {
    let file = synthetic_file(true, 8);
    let w1 = block_at(&file, 0x1000);
    // Element 1's table has one slot to PartB, not element 0's three slots.
    assert_eq!(
        file.pointer_table(w1, 1, &path(&["parts"]), 1).unwrap(),
        vec![Some(block_at(&file, 0x6000))]
    );
}

#[test]
fn pointer_width_and_endianness_come_from_the_saved_file() {
    for &(little, ps) in &[(true, 4usize), (false, 8usize), (false, 4usize)] {
        let file = synthetic_file(little, ps);
        let w1 = block_at(&file, 0x1000);
        assert_eq!(
            file.byte_array(w1, 0, &path(&["label"]), 8).unwrap(),
            [0x00, 0x41, 0x00, 0x42, 0x43, 0x00, 0x00, 0x44],
            "pointer size {ps}, little endian {little}"
        );
        assert_eq!(
            file.pointer_table(w1, 0, &path(&["parts"]), 3).unwrap(),
            vec![
                Some(block_at(&file, 0x5000)),
                None,
                Some(block_at(&file, 0x6000))
            ],
            "pointer size {ps}, little endian {little}"
        );
    }
}

#[test]
fn source_changes_refuse_both_operations_even_for_zero_items() {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!(
        "oxblend-array-source-change-{}-{}.blend",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    output.write_all(&synthetic(true, 8)).unwrap();
    drop(output);
    let file = BlendFile::open(&path).unwrap();
    let owner = block_at(&file, 0x1000);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&[0])
        .unwrap();
    // Raw File backing preserves its existing source_changed kind (input.rs).
    assert_eq!(
        err_kind(file.byte_array(owner, 0, &[field("label")], 0)),
        "source_changed"
    );
    assert_eq!(
        err_kind(file.pointer_table(owner, 0, &[field("parts")], 0)),
        "source_changed"
    );
    drop(file);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn cli_array_modes_are_mutually_exclusive_before_opening_a_file() {
    let cases: [&[&str]; 8] = [
        &["--raw", "--byte-array", "0"],
        &["--byte-array", "0", "--raw"],
        &["--raw", "--pointer-table", "0"],
        &["--pointer-table", "0", "--raw"],
        &["--byte-array", "0", "--pointer-table", "0"],
        &["--pointer-table", "0", "--byte-array", "0"],
        &["--byte-array"],
        &["--pointer-table"],
    ];
    for suffix in cases {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args(["get", "not-an-opened-fixture.blend", "0", "[\"label\"]"])
            .args(suffix)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2), "{suffix:?}");
        assert!(result.stdout.is_empty());
        let error: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(error["error"]["kind"], "usage", "{error}");
    }
}

// ------------------------------------------------------------------ hostiles

#[test]
fn byte_count_over_the_allocation_is_refused_before_any_read() {
    let file = synthetic_file(true, 8);
    let w1 = block_at(&file, 0x1000);
    // The label allocation holds exactly 8 bytes.
    assert_eq!(
        err_kind(file.byte_array(w1, 0, &path(&["label"]), 9)),
        "pointer_extent"
    );
    assert_eq!(
        err_kind(file.byte_array(w1, 0, &path(&["label"]), usize::MAX)),
        "pointer_extent"
    );
}

#[test]
fn pointer_table_count_never_crosses_into_adjacent_data() {
    let file = synthetic_file(true, 8);
    let w1 = block_at(&file, 0x1000);
    // W1's 0x3000 table holds 3 slots and is immediately followed by the
    // 0x3200 DATA block in the file; a 4th slot would read that block.
    assert_eq!(
        err_kind(file.pointer_table(w1, 0, &path(&["parts"]), 4)),
        "pointer_extent"
    );
}

#[test]
fn pointer_table_count_overflow_is_refused_before_allocation() {
    let file = synthetic_file(true, 8);
    let w1 = block_at(&file, 0x1000);
    assert_eq!(
        err_kind(file.pointer_table(w1, 0, &path(&["parts"]), usize::MAX)),
        "overflow"
    );
}

#[test]
fn byte_array_requires_a_width_one_pointer_field() {
    let file = synthetic_file(true, 8);
    let w1 = block_at(&file, 0x1000);
    assert_eq!(
        err_kind(file.byte_array(w1, 0, &path(&["totcol"]), 1)),
        "type"
    );
    assert_eq!(
        err_kind(file.byte_array(w1, 0, &path(&["numbers"]), 1)),
        "type"
    );
    assert_eq!(
        err_kind(file.byte_array(w1, 0, &path(&["single"]), 1)),
        "type"
    );
}

#[test]
fn whole_pointer_array_field_is_rejected_not_first_element_read() {
    let file = synthetic_file(true, 8);
    let w1 = block_at(&file, 0x1000);
    assert_eq!(err_kind(file.byte_array(w1, 0, &path(&["arr"]), 1)), "type");
    assert_eq!(
        err_kind(file.pointer_table(w1, 0, &path(&["arr"]), 1)),
        "type"
    );
}

#[test]
fn pointer_table_requires_a_struct_pointer_to_pointer_field() {
    let file = synthetic_file(true, 8);
    let w1 = block_at(&file, 0x1000);
    assert_eq!(
        err_kind(file.pointer_table(w1, 0, &path(&["totcol"]), 1)),
        "type"
    );
    assert_eq!(
        err_kind(file.pointer_table(w1, 0, &path(&["label"]), 1)),
        "pointer_depth"
    );
    assert_eq!(
        err_kind(file.pointer_table(w1, 0, &path(&["single"]), 1)),
        "pointer_depth"
    );
    assert_eq!(
        err_kind(file.pointer_table(w1, 0, &path(&["blob"]), 1)),
        "type"
    );
}

#[test]
fn null_pointer_with_nonzero_count_is_a_typed_refusal() {
    let file = synthetic_file(true, 8);
    let w7 = block_at(&file, 0xC000);
    assert_eq!(
        err_kind(file.byte_array(w7, 0, &path(&["label"]), 1)),
        "required_pointer_null"
    );
    assert_eq!(
        err_kind(file.pointer_table(w7, 0, &path(&["parts"]), 1)),
        "required_pointer_null"
    );
}

#[test]
fn byte_data_allocation_must_resolve_in_the_owners_data_run() {
    let file = synthetic_file(true, 8);
    let w2 = block_at(&file, 0x9000);
    // No allocation at this address anywhere.
    assert_eq!(
        err_kind(file.byte_array(w2, 0, &path(&["label"]), 1)),
        "dangling_pointer"
    );
    // 0x5000 exists only as an ID header; a data role cannot be rescued by it.
    assert_eq!(
        err_kind(file.byte_array(w2, 0, &path(&["raw"]), 1)),
        "pointer_target_type"
    );
}

#[test]
fn pointer_table_allocation_is_owner_scoped() {
    let file = synthetic_file(true, 8);
    let w2 = block_at(&file, 0x9000);
    // 0x3000 is a real table — in W1's DATA run, not W2's.
    assert_eq!(
        err_kind(file.pointer_table(w2, 0, &path(&["parts"]), 1)),
        "dangling_pointer"
    );
    // W3's own scope repeats one DATA address; the run itself is unresolvable.
    let w3 = block_at(&file, 0x9500);
    assert_eq!(
        err_kind(file.pointer_table(w3, 0, &path(&["parts"]), 1)),
        "duplicate_data_address"
    );
}

#[test]
fn table_entries_are_typed_and_order_preserved() {
    let file = synthetic_file(true, 8);
    // W4: entry 0 resolves, entry 1 dangles.
    let w4 = block_at(&file, 0xA000);
    assert_eq!(
        err_kind(file.pointer_table(w4, 0, &path(&["parts"]), 2)),
        "dangling_pointer"
    );
    // W5: entry resolves to an ID of the wrong kind.
    let w5 = block_at(&file, 0xB000);
    assert_eq!(
        err_kind(file.pointer_table(w5, 0, &path(&["parts"]), 1)),
        "pointer_target_type"
    );
    // W6: entry address is claimed by two ID blocks.
    let w6 = block_at(&file, 0xB500);
    assert_eq!(
        err_kind(file.pointer_table(w6, 0, &path(&["parts"]), 1)),
        "ambiguous_pointer"
    );
}

#[test]
fn element_outside_the_saved_records_is_bounds() {
    let file = synthetic_file(true, 8);
    let w1 = block_at(&file, 0x1000);
    assert_eq!(
        err_kind(file.byte_array(w1, 2, &path(&["label"]), 1)),
        "bounds"
    );
    assert_eq!(
        err_kind(file.pointer_table(w1, 2, &path(&["parts"]), 1)),
        "bounds"
    );
}
