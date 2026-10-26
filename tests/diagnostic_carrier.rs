use std::process::Command;

// The public Error's kind/message/Display remain unchanged by native-only
// diagnostic storage. The real CLI graph/link assertions live in test-report.py.
#[test]
fn public_error_contract_is_unchanged() {
    let error = oxblend::Error::new("unresolved_persistent", "same message");
    assert_eq!(error.kind, "unresolved_persistent");
    assert_eq!(error.message, "same message");
    assert_eq!(error.to_string(), "unresolved_persistent: same message");
    assert_eq!(error.clone().to_string(), error.to_string());
    let output = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
}
