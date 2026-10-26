//! Clean-room Blender 5.2.1 saves; generation/provenance live beside the fixtures.
#[path = "support/scratch.rs"]
mod test_scratch;
use serde_json::Value;
use std::{path::PathBuf, process::Command};

struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let dir = test_scratch::scratch_dir(&std::env::temp_dir()).join("source");
        std::fs::create_dir(&dir).unwrap();
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cryptomatte");
        for name in [
            "sequence.blend",
            "no-refresh.blend",
            "negative.blend",
            "tiles.blend",
            "compositor-image.blend",
            "render.blend",
        ] {
            std::fs::copy(fixtures.join(name), dir.join(name)).unwrap();
        }
        for frame in 0..=6 {
            std::fs::copy(
                fixtures.join("base.png"),
                dir.join(format!("shot.{frame:04}.png")),
            )
            .unwrap();
        }
        for tile in [1001, 1002] {
            std::fs::copy(
                fixtures.join("base.png"),
                dir.join(format!("tile.{tile}.png")),
            )
            .unwrap();
        }
        Self(dir)
    }
    fn run(&self, command: &str, name: &str, exit: i32) -> Value {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        cmd.arg(command).arg(self.0.join(name));
        if command == "list" {
            cmd.arg("--json");
        } else if command == "pack" {
            cmd.arg(self.0.parent().unwrap().join(format!("pack-{name}")));
        }
        let out = cmd.output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(exit),
            "{command} {name}: {} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(self.0.parent().unwrap()).unwrap();
    }
}

#[test]
fn cryptomatte_image_sequence_uses_embedded_image_user() {
    let work = Work::new();
    for name in ["sequence.blend", "compositor-image.blend"] {
        let report = work.run("trace", name, 0);
        assert_eq!(report["complete"], true);
        let usages = report["usages"].as_array().unwrap();
        assert_eq!(usages.len(), 1);
        let usage = &usages[0];
        assert_eq!(usage["pattern_status"], "available");
        assert_eq!(usage["is_sequence"], true);
        let expected: Vec<String> = (3..=5)
            .map(|n| {
                work.0
                    .join(format!("shot.{n:04}.png"))
                    .to_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(usage["expected_members"], serde_json::json!(expected));
        assert_eq!(usage["missing_members"], serde_json::json!([]));
        work.run("list", name, 0);
        assert_eq!(work.run("pack", name, 0)["complete"], true);
        let package = work.0.parent().unwrap().join(format!("pack-{name}"));
        for frame in 3..=5 {
            assert!(package.join(format!("shot.{frame:04}.png")).is_file());
        }
        // As for other image nodes, pack may also include existing glob matches;
        // expected_members above is the exact saved requirement, not that glob.
    }
    // Missing declared members must still make all consumers incomplete.
    std::fs::remove_file(work.0.join("shot.0004.png")).unwrap();
    let report = work.run("trace", "sequence.blend", 2);
    assert_eq!(report["complete"], false);
    assert_eq!(
        report["usages"][0]["missing_members"],
        serde_json::json!([work.0.join("shot.0004.png").to_str().unwrap()])
    );
    work.run("list", "sequence.blend", 2);
    // Pack refuses a fresh target, rather than silently publishing a complete package.
    std::fs::remove_dir_all(work.0.parent().unwrap().join("pack-sequence.blend")).unwrap();
    assert_eq!(work.run("pack", "sequence.blend", 2)["complete"], false);
}

#[test]
fn cryptomatte_unknown_rules_are_preserved() {
    let work = Work::new();
    let negative = work.run("trace", "negative.blend", 2);
    assert_eq!(negative["usages"][0]["pattern_status"], "unknown");
    assert!(
        negative["usages"][0]["pattern_unknown"]
            .as_str()
            .unwrap()
            .contains("negative offset")
    );
    let no_refresh = work.run("trace", "no-refresh.blend", 0);
    let members = no_refresh["usages"][0]["expected_members"]
        .as_array()
        .unwrap();
    // The existing path retains the saved current member as well as the declared span.
    assert!(
        members
            .iter()
            .any(|v| v.as_str().unwrap().ends_with("shot.0000.png"))
    );
    for n in 3..=5 {
        assert!(
            members
                .iter()
                .any(|v| v.as_str().unwrap().ends_with(&format!("shot.{n:04}.png")))
        );
    }
}

#[test]
fn cryptomatte_tiles_keep_the_saved_tile_requirements() {
    let work = Work::new();
    let trace = work.run("trace", "tiles.blend", 0);
    let usage = &trace["usages"][0];
    assert_eq!(usage["is_udim"], true);
    assert_eq!(
        usage["expected_members"],
        serde_json::json!([
            work.0.join("tile.1001.png").to_str().unwrap(),
            work.0.join("tile.1002.png").to_str().unwrap()
        ])
    );
    std::fs::remove_file(work.0.join("tile.1002.png")).unwrap();
    let trace = work.run("trace", "tiles.blend", 2);
    assert_eq!(
        trace["usages"][0]["missing_members"],
        serde_json::json!([work.0.join("tile.1002.png").to_str().unwrap()])
    );
}

#[test]
fn cryptomatte_render_source_has_no_image_usage() {
    let work = Work::new();
    let trace = work.run("trace", "render.blend", 0);
    assert_eq!(trace["complete"], true);
    assert_eq!(trace["usages"], serde_json::json!([]));
    work.run("list", "render.blend", 0);
    assert_eq!(work.run("pack", "render.blend", 0)["complete"], true);
}
