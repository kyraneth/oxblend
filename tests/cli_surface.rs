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
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{command}");
        let command_help = String::from_utf8(out.stdout).unwrap();
        assert!(
            command_help.starts_with(&format!("oxblend {command} ")),
            "{command_help}"
        );
        let usage = include_str!("../docs/USAGE.md");
        let row = usage
            .lines()
            .find(|line| line.starts_with(&format!("| `{command}` |")))
            .unwrap();
        let mut documented = row.to_owned();
        if matches!(command, "pack" | "pack-sequence") {
            // PACK_OPTIONS and aliases are documented once for both commands.
            documented.push_str(usage.lines().find(|l| l.starts_with("Options:")).unwrap());
            documented.push_str(
                usage
                    .lines()
                    .find(|l| l.starts_with("Unknown flags"))
                    .unwrap(),
            );
        }
        for flag in documented
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
            .filter(|word| word.starts_with('-') && word.len() > 1)
        {
            assert!(
                command_help
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                    .any(|word| word == flag),
                "{command} help omits documented flag {flag}: {command_help}"
            );
        }
        assert!(command_help.contains("--help"));
        assert!(!command_help.contains("See docs/USAGE.md for contracts and examples."));
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
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trace-owner/cross-id.blend");
    let work = std::env::temp_dir().join(format!("oxblend-closed-output-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    let file = work.join("cross-id.blend");
    std::fs::copy(source, &file).unwrap();
    // Disk completeness is incidental to this output-error proof. Supply the
    // declared resources so list reaches its actual stdout writer.
    for name in ["A.pc2", "B.pc2"] {
        std::fs::write(work.join(name), []).unwrap();
    }
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
    std::fs::remove_dir_all(work).unwrap();
}
