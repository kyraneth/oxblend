//! Native Blender 5.2.1: linked local override with a removed collection instance.
//! Its original reference still loads both source.blend and nested.blend.
use serde_json::Value;
use std::{path::Path, process::Command};
fn cli(args: &[&Path], tail: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .args(args)
        .args(tail)
        .output()
        .unwrap()
}
fn report(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| panic!("{:?}", output))
}
#[test]
fn native_override_reference_closure_is_planned_and_missing_sources_refuse_strict_pack() {
    let work =
        std::env::temp_dir().join(format!("oxblend-override-sources-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/override-sources");
    for name in ["root.blend", "middle.blend", "source.blend", "nested.blend"] {
        std::fs::copy(fixtures.join(name), work.join(name)).unwrap();
    }
    let root = work.join("root.blend");
    let trace = cli(&[Path::new("trace"), &root], &[]);
    assert!(trace.status.success(), "{:?}", trace);
    let trace = report(&trace);
    let paths: Vec<_> = trace["usages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["resolved_path"].as_str().unwrap())
        .collect();
    for name in ["middle.blend", "source.blend", "nested.blend"] {
        assert!(
            paths.contains(&work.join(name).to_str().unwrap()),
            "omitted native loaded library {name}: {trace}"
        );
    }
    let target = work.join("package");
    let packed = cli(
        &[Path::new("pack"), &root, &target, Path::new("-p"), &work],
        &[],
    );
    assert!(packed.status.success(), "{:?}", packed);
    assert_eq!(report(&packed)["complete"], true);
    for name in ["root.blend", "middle.blend", "source.blend", "nested.blend"] {
        assert!(target.join(name).is_file());
    }
    // Every nested library reached only through the original source is required.
    // Default strict must never publish a falsely complete package without it.
    std::fs::remove_file(work.join("nested.blend")).unwrap();
    let refused = work.join("refused");
    let failure = cli(
        &[Path::new("pack"), &root, &refused, Path::new("-p"), &work],
        &[],
    );
    assert_eq!(failure.status.code(), Some(2));
    assert_eq!(report(&failure)["complete"], false);
    assert!(!refused.exists());
    std::fs::remove_dir_all(work).unwrap();
}
