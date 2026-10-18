// narrow namespace-resolution contracts; CLI corpus proves the whole path.
use super::IdIndex;

#[test]
fn local_wins_over_ambiguous_linked() {
    let mut i = IdIndex::default();
    i.insert(b"OBx".to_vec(), 0, false);
    i.insert(b"OBx".to_vec(), 1, false);
    i.insert(b"OBx".to_vec(), 2, true);
    assert_eq!(i.candidates(b"OBx"), &[2]);
}
#[test]
fn duplicate_locals_remain_ambiguous_in_block_order() {
    let mut i = IdIndex::default();
    i.insert(b"OBx".to_vec(), 3, true);
    i.insert(b"OBx".to_vec(), 4, false);
    i.insert(b"OBx".to_vec(), 5, true);
    assert_eq!(i.candidates(b"OBx"), &[3, 5]);
}
#[test]
fn linked_only_unique_and_ambiguous_exports() {
    let mut i = IdIndex::default();
    i.insert(b"OBx".to_vec(), 7, false);
    assert_eq!(i.candidates(b"OBx"), &[7]);
    i.insert(b"OBx".to_vec(), 9, false);
    assert_eq!(i.candidates(b"OBx"), &[7, 9]);
    assert!(i.candidates(b"OBabsent").is_empty());
}
#[test]
fn raw_names_and_per_file_ownership() {
    let mut a = IdIndex::default();
    let mut b = IdIndex::default();
    a.insert(b"OB\xff".to_vec(), 1, true);
    b.insert(b"OB\xff".to_vec(), 2, false);
    assert_eq!(a.candidates(b"OB\xff"), &[1]);
    assert_eq!(b.candidates(b"OB\xff"), &[2]);
    assert!(a.candidates(b"OB\xfe").is_empty());
}
