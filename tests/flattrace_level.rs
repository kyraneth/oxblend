use std::{ffi::OsStr, os::unix::ffi::OsStrExt, path::PathBuf, process::Command};

#[test]
fn cli_honours_upstream_detail_levels_without_changing_product_output() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops/synthetic-empty-seed.blend");
    let work = std::env::temp_dir().join(format!("oxblend-level-{}", std::process::id()));
    let baseline = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("summary")
        .arg(&source)
        .env_remove("FLATTRACE_DIR")
        .env_remove("FLATTRACE_TRACEPARENT")
        .output()
        .unwrap();
    assert!(baseline.status.success());
    let cases: Vec<(Option<&OsStr>, u64, bool)> = vec![
        (None, 1, false),
        (Some(OsStr::new("")), 1, false),
        (Some(OsStr::new("  ")), 1, false),
        (Some(OsStr::new("0")), 0, false),
        (Some(OsStr::new("1")), 1, false),
        (Some(OsStr::new(" 002 ")), 2, false),
        (Some(OsStr::new("18446744073709551615")), u64::MAX, false),
        (Some(OsStr::new("loud")), 1, true),
        (Some(OsStr::new("-1")), 1, true),
        (Some(OsStr::new("+2")), 1, true),
        (Some(OsStr::new("1.5")), 1, true),
        (Some(OsStr::new("18446744073709551616")), 1, true),
        (Some(OsStr::from_bytes(b"\xff")), 1, true),
    ];
    let mut default_events = None;
    for (i, (raw, level, note)) in cases.into_iter().enumerate() {
        let directory = work.join(i.to_string());
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        command
            .arg("summary")
            .arg(&source)
            .env("FLATTRACE_DIR", &directory)
            .env_remove("FLATTRACE_LEVEL")
            .env_remove("FLATTRACE_TRACEPARENT");
        if let Some(raw) = raw {
            command.env("FLATTRACE_LEVEL", raw);
        }
        if level == 0 {
            command.env("FLATTRACE_TRACEPARENT", "invalid");
        }
        let actual = command.output().unwrap();
        assert_eq!(actual.status.code(), baseline.status.code());
        assert_eq!(actual.stdout, baseline.stdout);
        assert_eq!(actual.stderr, baseline.stderr);
        if level == 0 {
            assert!(!directory.exists(), "level zero must do no trace I/O");
            continue;
        }
        let files: Vec<_> = std::fs::read_dir(&directory).unwrap().collect();
        assert_eq!(files.len(), 1);
        let text = std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
        let rows: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows[0]["data"]["level"], level);
        if note {
            let notes = rows[0]["data"]["notes"].as_array().unwrap();
            assert!(
                notes
                    .iter()
                    .any(|v| v.as_str().unwrap().contains("invalid FLATTRACE_LEVEL")
                        && v.as_str().unwrap().ends_with("treated as 1"))
            );
        } else {
            assert!(rows[0]["data"].get("notes").is_none());
        }
        let events: Vec<_> = rows
            .iter()
            .map(|r| (r["event"].clone(), r["state"].clone()))
            .collect();
        if let Some(default) = &default_events {
            assert_eq!(&events, default);
        } else {
            default_events = Some(events);
        }
    }
    std::fs::remove_dir_all(work).unwrap();
}
