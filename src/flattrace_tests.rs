//! Adapted upstream sink test without its checkout-relative Python dependency.
use super::*;
#[test]
fn later_sink_failure_disables_without_failing_or_inventing_completion() {
    let path = std::env::temp_dir().join(format!("oxblend-sink-{}.jsonl", id(16).unwrap()));
    let mut writer = Writer::create_at(
        &path,
        "oxblend",
        "test",
        vec![],
        "test",
        json!({}),
        id(16).unwrap(),
        true,
    )
    .unwrap();
    writer.file = Some(OpenOptions::new().write(true).open("/dev/full").unwrap());
    writer.point("reader.stats", json!({}), None, None).unwrap();
    assert!(writer.file.is_none());
    writer.point("reader.stats", json!({}), None, None).unwrap();
    writer.finish("ok").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 1);
    std::fs::remove_file(path).unwrap();
}
