use std::{ffi::OsString, os::unix::ffi::OsStringExt, process::Command};

#[test]
fn uri_scheme_is_not_hidden_by_non_utf8_authority_bytes() {
    let result = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .args(["pack", "/missing-selected-input.blend"])
        .arg(OsString::from_vec(b"shaman://host/\xff".to_vec()))
        .arg("--noop")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(report["error"]["kind"], "usage");
    assert!(result.stdout.is_empty());
}
