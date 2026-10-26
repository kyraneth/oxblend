use std::{
    path::PathBuf,
    process::{Command, Output},
};
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .args(args)
        .env_remove("FLATTRACE_DIR")
        .output()
        .unwrap()
}
fn temp() -> PathBuf {
    let root = std::env::temp_dir().join(format!("oxblend-report-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    root
}
#[test]
fn every_error_has_offline_help() {
    let catalog = oxblend::error_help::catalog();
    for entry in catalog["entries"].as_array().unwrap() {
        let output = run(&["help", "error", entry["kind"].as_str().unwrap(), "--json"]);
        assert!(output.status.success(), "{:?}", output);
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["error"], *entry);
    }
    let unknown = run(&["help", "error", "new_future_kind"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("new_future_kind"));
}
#[test]
fn report_preview_binds_reviewed_bytes_and_refuses_overwrite() {
    let root = temp();
    let source = root.join("saved.json");
    let output = root.join("bug.zip");
    std::fs::write(&source,r#"{"schema":1,"error":{"kind":"dangling_pointer","message":"SECRET /home/alice/project.blend","block_index":2,"other_SECRET":{"token":"SECRET"},"causes":[{"kind":"SECRET_private_kind","message":"SECRET"}]},"complete":false,"SECRET":{"path_hex":"534543524554"}}"#).unwrap();
    let preview = run(&[
        "report",
        "--report",
        source.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--preview",
    ]);
    assert!(preview.status.success(), "{:?}", preview);
    let value: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert!(!output.exists());
    for member in value["members"].as_array().unwrap() {
        let text = member["preview_text"].as_str().unwrap();
        assert!(!text.contains("SECRET"));
        assert!(!text.contains("/home/alice"));
    }
    // Creation reads the frozen snapshot, not changed or deleted sources.
    std::fs::remove_file(&source).unwrap();
    let receipt = value["receipt_path"].as_str().unwrap();
    let hash = value["receipt_sha256"].as_str().unwrap();
    let wrong = run(&[
        "report",
        "--create",
        receipt,
        "--preview-sha256",
        "wrong",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert_eq!(wrong.status.code(), Some(2));
    assert!(!output.exists());
    let created = run(&[
        "report",
        "--create",
        receipt,
        "--preview-sha256",
        hash,
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(created.status.success(), "{:?}", created);
    assert_eq!(&std::fs::read(&output).unwrap()[..4], b"PK\x03\x04");
    // Cross-language ZIP parser verifies member bytes and their manifest hashes.
    let check=Command::new("python3").args(["-c", "import json,zipfile,hashlib,sys; z=zipfile.ZipFile(sys.argv[1]); m=json.loads(z.read('manifest.json')); assert len(z.namelist())==6; assert z.testzip() is None; [(lambda b: (b.__len__()==x['bytes'] and hashlib.sha256(b).hexdigest()==x['sha256']) or (_ for _ in ()).throw(AssertionError(x)))(z.read(x['path'])) for x in m['members']]; assert not any(b'SECRET' in z.read(n) for n in z.namelist())", output.to_str().unwrap()]).output().unwrap();
    assert!(check.status.success(), "{:?}", check);
    let before = std::fs::read(&output).unwrap();
    let refused = run(&[
        "report",
        "--report",
        source.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--preview",
    ]);
    assert_eq!(refused.status.code(), Some(2));
    assert_eq!(std::fs::read(&output).unwrap(), before);
    std::fs::remove_dir_all(root).unwrap();
}
