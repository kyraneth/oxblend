use serde_json::Value;
use std::{path::PathBuf, process::Command};

#[test]
fn saved_pattern_members_are_required_not_only_globbed() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = std::env::temp_dir().join(format!("oxblend-patterns-{}", std::process::id()));
    let source = work.join("source");
    std::fs::create_dir_all(&source).unwrap();
    for name in [
        "patterns.blend",
        "base.png",
        "tile.1001.png",
        "shot.0001.png",
        "shot.0003.png",
        "offset.blend",
        "unknown-negative.blend",
        "unknown-user.blend",
        "uvtile.blend",
    ] {
        std::fs::copy(
            root.join("tests/fixtures/pattern-members").join(name),
            source.join(name),
        )
        .unwrap();
    }
    for command in ["list", "trace", "pack", "permissive"] {
        let target = work.join(command);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        cmd.arg(if command == "permissive" {
            "pack"
        } else {
            command
        })
        .arg(source.join("patterns.blend"));
        if matches!(command, "pack" | "permissive") {
            cmd.arg(&target);
        }
        if command == "permissive" {
            cmd.arg("--permissive");
        }
        if command == "list" {
            cmd.arg("--json");
        }
        let output = cmd.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{command}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["complete"], false, "{command}");
        let text = String::from_utf8_lossy(&output.stdout);
        for member in ["tile.1002.png", "shot.0002.png"] {
            assert!(text.contains(member), "{command}: missing {member}");
        }
        if command == "pack" {
            assert!(!target.exists());
        }
        if command == "permissive" {
            let info = std::fs::read_to_string(target.join("pack-info.txt")).unwrap();
            assert!(info.contains("INCOMPLETE"));
            for member in ["tile.1002.png", "shot.0002.png"] {
                assert!(info.contains(member));
            }
            assert!(target.join("shot.0001.png").is_file());
            assert!(target.join("shot.0003.png").is_file());
        }
    }
    for (name, required) in [
        ("offset.blend", "shot.0005.png"),
        ("uvtile.blend", "tile.u2_v1.png"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("trace")
            .arg(source.join(name))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stdout).contains(required));
    }
    for name in ["unknown-negative.blend", "unknown-user.blend"] {
        for command in ["trace", "list", "pack"] {
            let target = work.join(format!("{name}-{command}"));
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_oxblend"));
            cmd.arg(command).arg(source.join(name));
            if command == "list" {
                cmd.arg("--json");
            }
            if command == "pack" {
                cmd.arg(&target).arg("--permissive");
            }
            let output = cmd.output().unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert!(
                String::from_utf8_lossy(&output.stdout).contains("pattern_members_unknown"),
                "{name} {command}"
            );
        }
    }
    for name in ["tile.1002.png", "shot.0002.png"] {
        std::fs::copy(source.join("base.png"), source.join(name)).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("pack")
        .arg(source.join("patterns.blend"))
        .arg(work.join("complete"))
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["complete"], true);
    std::fs::remove_dir_all(work).unwrap();
}
