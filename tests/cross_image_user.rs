use std::{path::PathBuf, process::Command};
#[test]
fn linked_image_uses_saved_consumers_from_both_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pattern-members");
    let work = std::env::temp_dir().join(format!("oxblend-cross-image-{}", std::process::id()));
    let source = work.join("source");
    std::fs::create_dir_all(&source).unwrap();
    for name in [
        "patterns.blend",
        "cross-user.blend",
        "shot.0001.png",
        "shot.0003.png",
    ] {
        std::fs::copy(root.join(name), source.join(name)).unwrap();
    }
    std::fs::copy(root.join("base.png"), source.join("shot.0002.png")).unwrap();
    for command in ["trace", "list", "pack", "permissive"] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        cmd.arg(if command == "permissive" {
            "pack"
        } else {
            command
        })
        .arg(source.join("cross-user.blend"));
        let target = work.join(command);
        if command == "list" {
            cmd.arg("--json");
        }
        if matches!(command, "pack" | "permissive") {
            cmd.arg(&target);
        }
        if command == "permissive" {
            cmd.arg("--permissive");
        }
        let out = cmd.output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{command}: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        for name in ["shot.0004.png", "shot.0005.png"] {
            assert!(text.contains(name), "{command} {name}");
        }
        if command == "pack" {
            assert!(!target.exists());
        }
        if command == "permissive" {
            let info = std::fs::read_to_string(target.join("pack-info.txt")).unwrap();
            assert!(info.contains("shot.0004.png") && info.contains("shot.0005.png"));
        }
    }
    // With auto-refresh disabled, the saved current frame is required too,
    // even when it lies outside the playback span. Native 5.2.1 keeps frame0.
    for name in ["shot.0004.png", "shot.0005.png"] {
        std::fs::copy(root.join("base.png"), source.join(name)).unwrap();
    }
    let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("trace")
        .arg(source.join("cross-user.blend"))
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("shot.0000.png"));
    std::fs::copy(root.join("base.png"), source.join("shot.0000.png")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("trace")
        .arg(source.join("cross-user.blend"))
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    std::fs::remove_dir_all(work).unwrap();
}
