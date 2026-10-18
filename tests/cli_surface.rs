use std::{
    os::fd::OwnedFd,
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Command, Stdio},
};

#[test]
fn help_version_and_all_commands_are_discoverable() {
    let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success());
    let help = String::from_utf8(out.stdout).unwrap();
    for command in [
        "summary",
        "inspect",
        "get",
        "pointer",
        "payload",
        "source-facts",
        "library-facts",
        "trace",
        "list",
        "blocks",
        "overrides",
        "diff",
        "edit",
        "rewrite",
        "pack",
        "pack-sequence",
        "exr-header",
        "pointer-census",
    ] {
        assert!(
            help.split_whitespace().any(|word| word == command),
            "{command}"
        );
    }
    let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        out.stdout,
        format!("oxblend {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
    );
    let options = oxblend::trace::TraceOptions::default();
    assert_eq!(options.mode, oxblend::trace::TraceMode::Recursive);
    assert_eq!(options.pointers, oxblend::trace::PointerMode::Strict);
}

#[test]
fn closed_output_is_a_clean_exit_not_a_panic() {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trace-owner/cross-id.blend");
    for cmd in [
        "--help",
        "--version",
        "summary",
        "inspect",
        "list",
        "blocks",
        "trace",
    ] {
        let (reader, writer) = UnixStream::pair().unwrap();
        drop(reader); // Broken pipe exists before child entry, no timing race.
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        command.arg(cmd);
        if !cmd.starts_with("--") {
            command.arg(&file);
        }
        let out = command
            .stdout(Stdio::from(OwnedFd::from(writer)))
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{cmd}: {:?} {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stderr.is_empty(),
            "{cmd}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
