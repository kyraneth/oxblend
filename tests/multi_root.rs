use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/trace-owner/pointer-kinds")
        .join(name)
}
fn run(args: &[&std::ffi::OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .args(args)
        .output()
        .unwrap()
}
fn json(out: &std::process::Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn independent_roots_survive_errors_and_pack_refuses_with_same_inventory() {
    let a = fixture("id-only-data.blend");
    let b = fixture("selected-material-root.blend");
    let work = std::env::temp_dir().join(format!("oxblend-multi-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let bad = work.join("bad.blend");
    std::fs::write(&bad, b"invalid").unwrap();
    let out = run(&[
        "trace".as_ref(),
        a.as_os_str(),
        bad.as_os_str(),
        b.as_os_str(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    let v = json(&out);
    assert_eq!(v["schema"], "oxblend.multi-root/v1");
    assert_eq!(v["root_traces"].as_array().unwrap().len(), 3);
    assert!(v["root_traces"][1]["trace"].is_null());
    assert_eq!(v["root_traces"][1]["error"]["kind"], "invalid_header");
    for (i, p) in [(0, &a), (2, &b)] {
        let single = run(&["trace".as_ref(), p.as_os_str()]);
        assert_eq!(v["root_traces"][i]["trace"], json(&single));
    }
    let listing = run(&[
        "list".as_ref(),
        a.as_os_str(),
        bad.as_os_str(),
        b.as_os_str(),
        "--json".as_ref(),
    ]);
    assert_eq!(listing.status.code(), Some(2));
    let l = json(&listing);
    assert_eq!(l["cross_reference"], v["cross_reference"]);
    let target = work.join("never-published");
    let project = Path::new("/");
    let packed = run(&[
        "pack-sequence".as_ref(),
        a.as_os_str(),
        bad.as_os_str(),
        b.as_os_str(),
        "-t".as_ref(),
        target.as_os_str(),
        "-p".as_ref(),
        project.as_os_str(),
        "--dry-run".as_ref(),
    ]);
    assert_eq!(packed.status.code(), Some(2));
    let p = json(&packed);
    // Pack retains captured-reader error context rather than reopening roots
    // merely to reproduce path-trace message spelling.
    let mut expected = v["cross_reference"].clone();
    expected["roots"][1]["error"]["message"] =
        serde_json::json!(format!("snapshot/index '{}': BLENDER magic", bad.display()));
    let missing = fixture("missing-material-library.blend");
    expected["roots"][2]["missing_links"][0]["message"] = serde_json::json!(format!(
        "snapshot/index '{}': fetch/read snapshot '{}': No such file or directory (os error 2)",
        missing.display(),
        missing.display()
    ));
    assert_eq!(p["cross_reference"], expected);
    assert_eq!(p["planning_complete"], false);
    assert!(!target.exists());
    std::fs::remove_dir_all(work).unwrap();
}
#[test]
fn symlink_root_contexts_remain_distinct_but_dependency_aliases_merge() {
    use std::os::unix::fs::symlink;
    let work = std::env::temp_dir().join(format!("oxblend-contexts-{}", std::process::id()));
    let a = work.join("a");
    let b = work.join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    let shared = work.join("shared.pc2");
    std::fs::write(&shared, b"same bytes").unwrap();
    for d in [&a, &b] {
        symlink(fixture("id-only-data.blend"), d.join("root.blend")).unwrap();
        symlink(&shared, d.join("A.pc2")).unwrap();
    }
    std::fs::hard_link(&shared, a.join("B.pc2")).unwrap();
    std::fs::write(b.join("B.pc2"), b"same bytes").unwrap();
    let out = run(&[
        "trace".as_ref(),
        a.join("root.blend").as_os_str(),
        b.join("root.blend").as_os_str(),
    ]);
    assert_eq!(out.status.code(), Some(0));
    let v = json(&out);
    let x = &v["cross_reference"];
    assert_eq!(x["roots"].as_array().unwrap().len(), 2);
    assert_eq!(x["totals"]["assets"], 3);
    assert_eq!(x["totals"]["shared_assets"], 1);
    assert_eq!(x["totals"]["bytes"], 30);
    let shared_row = x["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["shared"] == true)
        .unwrap();
    assert_eq!(
        shared_row["resolved_paths_hex"].as_array().unwrap().len(),
        2
    );
    assert_eq!(shared_row["used_by"].as_array().unwrap().len(), 2);
    for (i, d) in [&a, &b].iter().enumerate() {
        assert_eq!(
            x["roots"][i]["input_path"],
            d.join("root.blend").to_str().unwrap()
        );
    }
    std::fs::remove_dir_all(work).unwrap();
}
#[test]
fn directory_sequences_expand_members_and_empty_expansions_remain_absent() {
    use std::os::unix::fs::symlink;
    let work =
        std::env::temp_dir().join(format!("oxblend-directory-members-{}", std::process::id()));
    let cache = work.join("bake-cache");
    std::fs::create_dir_all(&cache).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dependencies/bake-modern.blend");
    let a = work.join("a.blend");
    let b = work.join("b.blend");
    symlink(&source, &a).unwrap();
    symlink(&source, &b).unwrap();
    std::fs::write(cache.join("f0001.exr"), b"one").unwrap();
    std::fs::write(cache.join("f0003.exr"), b"three").unwrap();
    let out = run(&["trace".as_ref(), a.as_os_str(), b.as_os_str()]);
    assert_eq!(out.status.code(), Some(0));
    let v = json(&out);
    assert_eq!(v["cross_reference"]["totals"]["assets"], 2);
    assert_eq!(v["cross_reference"]["totals"]["shared_assets"], 2);
    assert_eq!(v["cross_reference"]["totals"]["bytes"], 8);
    for a in v["cross_reference"]["assets"].as_array().unwrap() {
        assert_eq!(a["used_by"].as_array().unwrap().len(), 2);
    }
    std::fs::remove_file(cache.join("f0001.exr")).unwrap();
    std::fs::remove_file(cache.join("f0003.exr")).unwrap();
    let out = run(&["trace".as_ref(), a.as_os_str(), b.as_os_str()]);
    assert_eq!(out.status.code(), Some(2));
    let v = json(&out);
    assert_eq!(v["cross_reference"]["totals"]["assets"], 1);
    assert_eq!(v["cross_reference"]["assets"][0]["unexpanded"], true);
    assert_eq!(v["cross_reference"]["assets"][0]["exists"], false);
    assert!(v["cross_reference"]["assets"][0]["bytes"].is_null());
    std::fs::remove_dir_all(work).unwrap();
}
#[test]
fn native_byte_stdin_nul_selection_matches_positional_roots() {
    use std::{io::Write, os::unix::ffi::OsStringExt, process::Stdio};
    let work = std::env::temp_dir().join(format!("oxblend-stdin-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let a = work.join(std::ffi::OsString::from_vec(b"a\nline.blend".to_vec()));
    let b = work.join(std::ffi::OsString::from_vec(b"b-\xff.blend".to_vec()));
    for p in [&a, &b] {
        std::os::unix::fs::symlink(fixture("id-only-data.blend"), p).unwrap();
    }
    for command in ["trace", "list"] {
        let flags = if command == "list" {
            vec!["--json"]
        } else {
            vec![]
        };
        let positional = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg(command)
            .args(&flags)
            .arg("--")
            .arg(&a)
            .arg(&b)
            .output()
            .unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg(command)
            .args(&flags)
            .args(["--files-from", "-", "-0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        for p in [&a, &b] {
            input.write_all(p.as_os_str().as_encoded_bytes()).unwrap();
            input.write_all(&[0]).unwrap();
        }
        drop(input);
        let listed = child.wait_with_output().unwrap();
        assert_eq!(listed.status.code(), positional.status.code());
        assert_eq!(listed.stdout, positional.stdout);
        assert!(json(&listed)["root_traces"][1]["input_path"].is_null());
    }
    std::fs::remove_dir_all(work).unwrap();
}
#[test]
fn identity_observation_failure_does_not_hide_other_roots() {
    use std::os::unix::fs::symlink;
    let work =
        std::env::temp_dir().join(format!("oxblend-identity-failure-{}", std::process::id()));
    let bad = work.join("bad");
    let good = work.join("good");
    for d in [&bad, &good] {
        std::fs::create_dir_all(d).unwrap();
        symlink(fixture("id-only-data.blend"), d.join("root.blend")).unwrap();
        std::fs::write(d.join("B.pc2"), b"data").unwrap();
    }
    symlink("A.pc2", bad.join("A.pc2")).unwrap();
    std::fs::write(good.join("A.pc2"), b"data").unwrap();
    let args = [
        std::ffi::OsString::from("trace"),
        bad.join("root.blend").into_os_string(),
        good.join("root.blend").into_os_string(),
    ];
    let output = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .args(&args)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let v = json(&output);
    let roots = v["cross_reference"]["roots"].as_array().unwrap();
    assert!(!roots[0]["error"].is_null());
    assert_eq!(roots[1]["complete"], true);
    assert_eq!(roots[1]["unique"].as_array().unwrap().len(), 2);
    std::fs::remove_dir_all(work).unwrap();
}
#[test]
fn list_delivery_receipt_binds_actual_single_and_multi_root_stdout() {
    let a = fixture("id-only-data.blend");
    let b = fixture("selected-material-root.blend");
    for roots in [vec![b.as_os_str()], vec![a.as_os_str(), b.as_os_str()]] {
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("list")
            .args(roots)
            .arg("--json")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(!out.stdout.is_empty());
        let envelope = String::from_utf8(out.stderr)
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|value| value["diagnostic"].is_object())
            .unwrap();
        let binding = &envelope["diagnostic"]["binding"];
        assert_eq!(binding["stdout_bytes"], out.stdout.len());
        assert_eq!(binding["stdout_delivery_complete"], true);
    }
}
#[test]
fn saved_missing_pattern_members_remain_in_shared_union() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pattern-members");
    let a = source.join("patterns.blend");
    let b = source.join("offset.blend");
    for command in ["trace", "list", "pack-sequence"] {
        let target =
            std::env::temp_dir().join(format!("oxblend-multi-pattern-{}", std::process::id()));
        let mut args = vec![
            std::ffi::OsString::from(command),
            a.clone().into_os_string(),
            b.clone().into_os_string(),
        ];
        if command == "list" {
            args.push("--json".into());
        }
        if command == "pack-sequence" {
            args.extend([
                "-t".into(),
                target.clone().into_os_string(),
                "--dry-run".into(),
            ]);
        }
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        let v = json(&out);
        let x = &v["cross_reference"];
        assert_eq!(x["complete"], false);
        for missing in ["tile.1002.png", "shot.0002.png", "shot.0005.png"] {
            let row = x["assets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| {
                    row["identity_path"]
                        .as_str()
                        .is_some_and(|path| path.ends_with(missing))
                })
                .unwrap();
            assert_eq!(row["exists"], false);
        }
        assert!(!target.exists());
    }
}
#[test]
fn single_root_files_from_and_duplicate_paths_keep_legacy_bytes() {
    let input = fixture("id-only-data.blend");
    let work = std::env::temp_dir().join(format!("oxblend-roots-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let list = work.join("roots.txt");
    std::fs::write(&list, format!("{}\n{}\n", input.display(), input.display())).unwrap();
    for command in ["trace", "list"] {
        let mut args = vec![
            std::ffi::OsString::from(command),
            input.as_os_str().to_owned(),
        ];
        if command == "list" {
            args.push("--json".into());
        }
        let single = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args(&args)
            .output()
            .unwrap();
        args[1] = "--files-from".into();
        args.insert(2, list.as_os_str().to_owned());
        let selected = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(selected.status.code(), single.status.code());
        assert_eq!(selected.stdout, single.stdout);
    }
    std::fs::remove_dir_all(work).unwrap();
}
