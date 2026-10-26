//! Active forwarding state belongs to a library resolution branch, not a file pass.
use serde_json::Value;
use std::{collections::BTreeSet, path::Path, process::Command};
fn check_case(case: &str, cycle: bool) -> BTreeSet<String> {
    let work =
        std::env::temp_dir().join(format!("oxblend-forwarding-{case}-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/forwarding-branches")
        .join(case);
    for entry in std::fs::read_dir(fixtures).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().is_some_and(|e| e == "blend") {
            std::fs::copy(&p, work.join(p.file_name().unwrap())).unwrap();
        }
    }
    let root = work.join("root.blend");
    let target = work.join("package");
    let mut members = BTreeSet::new();
    for cmd in ["trace", "list", "pack"] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        c.arg(cmd).arg(&root);
        if cmd == "list" {
            c.arg("--json");
        }
        if cmd == "pack" {
            c.arg(&target).arg("-p").arg(&work);
        }
        let result = c.output().unwrap();
        let j: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(
            result.status.code(),
            Some(if cycle { 2 } else { 0 }),
            "{case}/{cmd}: {j}"
        );
        if cmd == "list" {
            // Plain successful list JSON is the owner → physical members map.
            if !cycle {
                for paths in j.as_object().unwrap().values() {
                    for path in paths.as_array().unwrap() {
                        members.insert(
                            Path::new(path.as_str().unwrap())
                                .file_name()
                                .unwrap()
                                .to_str()
                                .unwrap()
                                .to_owned(),
                        );
                    }
                }
            }
            continue;
        }
        assert_eq!(j["complete"], !cycle, "{j}");
        if cycle {
            assert!(
                j["errors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["kind"] == "cycle"),
                "{j}"
            );
            assert!(!target.exists(), "strict must publish nothing");
        } else if cmd == "pack" {
            for entry in std::fs::read_dir(&work).unwrap() {
                let p = entry.unwrap().path();
                if p.extension().is_some_and(|e| e == "blend") {
                    let packed = target.join(p.file_name().unwrap());
                    assert!(packed.is_file());
                    // These relative fixtures need no path rewrite.
                    assert_eq!(std::fs::read(&p).unwrap(), std::fs::read(packed).unwrap());
                }
            }
        }
    }
    std::fs::remove_dir_all(work).unwrap();
    members
}
#[test]
fn completed_sibling_forwarding_diamond_is_not_a_cycle_but_recursive_aliases_are() {
    for (case, cycle) in [
        ("diamond", false),
        ("direct-control", false),
        ("self-cycle", true),
        ("long-cycle", true),
    ] {
        check_case(case, cycle);
    }
}
#[test]
fn mixed_selector_batch_resolves_before_expanding_dependencies() {
    // A.X is local; A.Y refers back through F.X. This is dependency reuse,
    // not the forwarding-only loop exercised by the self/long-cycle controls.
    // Serialized-graph contract, not a native-loader certificate: an authored
    // cyclic mixed Library topology in Blender 5.2.1 loads with F.X missing.
    // Native sanity uses the source-hidden direct-forwarding package control.
    let mixed = check_case("mixed-batch-known-35", false);
    let direct = check_case("mixed-direct-control", false);
    assert_eq!(
        mixed, direct,
        "mixed forwarding and full-ID control must have the same physical closure"
    );
    assert_eq!(
        mixed,
        BTreeSet::from(["a.blend".into(), "forward.blend".into()])
    );
}
