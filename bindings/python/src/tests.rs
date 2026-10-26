//! Narrow mapper contracts; receiving-wheel suites own full API qualification.
use super::*;
use std::io::Write;

#[test]
fn displaced_get_failure_keeps_query_context_and_secondary_error() {
    let directory = std::env::temp_dir().join(format!(
        "oxblend-binding-dual-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("root.blend");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/file-ops/synthetic-empty-seed.blend"),
        &path,
    )
    .unwrap();
    let mut session = Session::open(&path).into_result().unwrap();
    let result: NativeResult<()> = read_adapter(&mut session, "get", |file| {
        let scene = file
            .blocks()
            .iter()
            .position(|b| b.code == *b"SC\0\0")
            .unwrap();
        let mut failure = Failure::from(
            file.get(
                scene,
                0,
                &[PathStep::Field(b"missing_binding_test_field".to_vec())],
                true,
            )
            .unwrap_err(),
        );
        failure.query = Some(0); // One actual failing batch item.
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"changed")
            .unwrap();
        Err(failure)
    });
    let error = result.unwrap_err();
    assert_eq!(error.kind, "input_changed");
    assert_eq!(error.query, Some(0));
    assert!(error.diagnostics.iter().any(|d| matches!(d, oxblend::Diagnostic::SecondaryError { error, .. } if error.kind == "missing_field")));
    assert!(error.diagnostics.iter().any(|d| matches!(d, oxblend::Diagnostic::SourceCheckFailure { error, .. } if error.kind == "input_changed")));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn changed_pack_report_keeps_every_effect_and_diagnostic() {
    let report = json!({"complete":false,"planning_complete":true,"directory_created":true,
        "files":[{"output_written":true,"durability_completed":true,"input_path_hex":"61","output_path_hex":"62"}],
        "archive_written":false,"info_written":true,"info_durability_completed":true,
        "durability_completed":true,"future_field":{"preserved":true},
        "errors":[{"kind":"input_changed","message":"changed after member publication"}]});
    let notices = vec![json!({"kind":"source_check_failure","source_hex":"61"})];
    let failure = checked_pack_report(report.clone(), notices.clone()).unwrap_err();
    assert_eq!(failure.kind, "input_changed");
    let mut expected = report;
    expected["operation_diagnostics"] = json!(notices);
    assert_eq!(failure.report, Some(expected));
}
