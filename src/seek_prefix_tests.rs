//! Inspection must not retain opaque decoded bytes or certify unqueried checksums.
use super::*;
#[test]
fn touched_frame_spills_once_and_skipped_corruption_is_not_certified() {
    let file = tests::fixture(false);
    let input = Input::open(file.try_clone().unwrap()).unwrap().unwrap();
    let second = &input.frames[1];
    let at = (second.physical + second.compressed - 1) as u64;
    let mut byte = [0];
    file.read_exact_at(&mut byte, at).unwrap();
    byte[0] ^= 1;
    file.write_all_at(&byte, at).unwrap();
    let input = Input::open(file).unwrap().unwrap();
    assert_eq!(input.range_prefix(0, 2).unwrap().as_ref(), b"fi");
    let stats = input.prefix_stats();
    assert_eq!(stats["decode_work_bytes"], 5);
    assert_eq!(stats["spill_bytes_written"], 5);
    assert_eq!(stats["source_full_frames_decoded"], 1);
    input.range_prefix(1, 2).unwrap();
    assert_eq!(input.prefix_stats()["decode_work_bytes"], 5);
    assert_eq!(stats["full_source_decoder_verified"], false);
    assert_eq!(input.range_prefix(5, 6).unwrap_err().kind, "seek_table");
}
