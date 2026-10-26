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
        "exr-scan",
        "exr-pixels",
        "pointer-census",
        "skill",
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
        assert!(command_help.contains("oxblend skill"));
        assert!(!command_help.contains("docs/USAGE.md"));
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
fn closed_output_is_an_explicit_error_not_false_success() {
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
        "skill",
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
        if !cmd.starts_with("--") && cmd != "skill" {
            command.arg(&file);
        }
        let out = command
            .stdout(Stdio::from(OwnedFd::from(writer)))
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{cmd}: {:?}", out.status);
        let error: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(error["error"]["kind"], "broken_pipe", "{cmd}: {error}");
    }
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn bundled_guide_and_summary_do_not_depend_on_cwd_or_environment() {
    let work = std::env::temp_dir().join(format!("oxblend-guidance-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_oxblend"));
    for args in [vec!["skill"], vec!["skill", "print"]] {
        let out = Command::new(&binary)
            .args(args)
            .env_clear()
            .current_dir(&work)
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        assert_eq!(out.stdout, include_bytes!("../skills/oxblend/SKILL.md"));
        assert!(
            String::from_utf8(out.stdout)
                .unwrap()
                .contains(&format!("# Oxblend {}", env!("CARGO_PKG_VERSION")))
        );
    }
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/synthetic-empty-seed.blend");
    let out = Command::new(&binary)
        .arg("summary")
        .arg(&source)
        .env_clear()
        .current_dir(&work)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        report,
        oxblend::summary::summarize(&oxblend::BlendFile::open(source).unwrap()).unwrap()
    );
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn retired_phase_variables_do_not_change_output() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/synthetic-empty-seed.blend");
    for command in ["summary", "trace", "overrides"] {
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg(command)
            .arg(&source)
            .env_remove("FLATTRACE_DIR")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stderr.is_empty(),
            "{command}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let timed = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg(command)
            .arg(&source)
            .env_remove("FLATTRACE_DIR")
            .env("OXBLEND_PHASE_TIMING", "1")
            .env("OXBLEND_PHASES", "1")
            .env("OXBLEND_READ_REPORT", "1")
            .output()
            .unwrap();
        assert!(timed.status.success());
        assert_eq!(out.stdout, timed.stdout);
        assert!(timed.stderr.is_empty());
    }
}

#[test]
fn common_refusals_name_safe_next_commands() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/synthetic-empty-seed.blend");
    let file = oxblend::BlendFile::open(&source).unwrap();
    let scene = file
        .blocks()
        .iter()
        .position(|b| b.code == *b"SC\0\0")
        .unwrap()
        .to_string();
    let bad_block = file.blocks().len().to_string();
    for (args, kind) in [
        (
            vec![
                "get",
                source.to_str().unwrap(),
                &bad_block,
                "[\"id\",\"name\"]",
            ],
            "bounds",
        ),
        (
            vec!["get", source.to_str().unwrap(), &scene, "[\"not_a_field\"]"],
            "missing_field",
        ),
        (
            vec!["get", source.to_str().unwrap(), &scene, "[\"r\"]"],
            "unsupported_type",
        ),
        (
            vec![
                "get",
                source.to_str().unwrap(),
                &scene,
                "[\"id\",\"name\",\"r\"]",
            ],
            "unsupported_type",
        ),
        (vec!["get", source.to_str().unwrap()], "usage"),
        (vec!["skill", "install"], "usage"),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        let error: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(error["error"]["kind"], kind, "{args:?}: {error}");
        let next = &error["error"]["next_command"]["argv"];
        if kind == "usage" {
            assert_eq!(next, &serde_json::json!(["oxblend", args[0], "--help"]));
        } else {
            assert_eq!(
                next,
                &serde_json::json!(["oxblend", "inspect", source.to_str().unwrap()])
            );
        }
        assert!(error["error"]["next_command"]["argv_hex"].is_array());
    }
}

#[test]
fn unknown_command_even_when_mentioned_in_help_suggests_valid_root_help() {
    for command in ["oxblend", "every", "unknown-command"] {
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg(command)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        let error: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(
            error["error"]["next_command"]["argv"],
            serde_json::json!(["oxblend", "--help"])
        );
    }
}

#[test]
fn next_command_preserves_non_utf8_path_without_shell_interpolation() {
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    let work = std::env::temp_dir().join(format!("oxblend-next-command-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/synthetic-empty-seed.blend");
    let path = work.join(std::ffi::OsString::from_vec(
        b"scene-\xff;$(false).blend".to_vec(),
    ));
    symlink(&source, &path).unwrap();
    let bad_block = oxblend::BlendFile::open(source)
        .unwrap()
        .blocks()
        .len()
        .to_string();
    let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("get")
        .arg(&path)
        .args([bad_block.as_str(), "[\"id\",\"name\"]"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let error: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    let next = &error["error"]["next_command"];
    assert!(next["argv"][2].is_null());
    assert_eq!(
        next["argv_hex"][2],
        oxblend::value::hex(path.as_os_str().as_encoded_bytes())
    );
    std::fs::remove_dir_all(work).unwrap();
}
