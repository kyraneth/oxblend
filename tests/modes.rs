//! Production strictness is separate from reports; real CLI/publication gates.
#[path = "support/diagnostic_metadata.rs"]
mod diagnostic_metadata;
#[path = "support/scratch.rs"]
mod scratch;
use oxblend::{BlendFile, value::PathStep};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        Self(scratch::scratch_dir(&std::env::temp_dir()))
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn fixture(s: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(s)
}
fn run(command: &str, input: &Path, flags: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg(command)
        .arg(input)
        .args(flags)
        .output()
        .unwrap()
}
fn report(r: &Output) -> Value {
    assert_eq!(r.status.code(), Some(2), "{:?}", r);
    let mut value = serde_json::from_slice(&r.stdout).unwrap();
    diagnostic_metadata::normalize(&mut value);
    value
}
fn invalid_pointer(input: &Path) {
    // Joseph-style stale saved pointer, independently authored and sanitized.
    // Two unknown Text.name sites, plus a separate valid Text with a known file.
    let source = fixture("dependencies/text-name.blend");
    let f = BlendFile::open(&source).unwrap();
    let mut body = std::fs::read(&source).unwrap();
    let b = &f.blocks()[0];
    let si = f.struct_index(0).unwrap();
    let field = f.schema().structs[si]
        .fields
        .iter()
        .find(|f| f.name == b"name")
        .unwrap();
    assert!(field.pointer);
    let mut extra = body[b.data_offset - 24..b.data_offset + b.size].to_vec();
    let address = f
        .blocks()
        .iter()
        .map(|b| b.old_address)
        .max()
        .unwrap()
        .checked_add(1)
        .unwrap();
    extra[8..16].copy_from_slice(&address.to_le_bytes());
    let dna = f
        .blocks()
        .iter()
        .find(|b| b.code == *b"DNA1")
        .unwrap()
        .data_offset
        - 24;
    // Each Text owns its DATA allocation. A valid independent clone gets
    // its own checked DATA rather than borrowing the corrupt owner's bytes.
    body[b.data_offset + field.offset..b.data_offset + field.offset + 8]
        .copy_from_slice(&9999u64.to_le_bytes());
    extra[24 + field.offset..24 + field.offset + 8].copy_from_slice(&(address + 1).to_le_bytes());
    let mut second = extra.clone();
    second[8..16].copy_from_slice(&(address + 2).to_le_bytes());
    second[24 + field.offset..24 + field.offset + 8].copy_from_slice(&9998u64.to_le_bytes());
    let data = &f.blocks()[1];
    assert_eq!(data.code, *b"DATA");
    let mut allocation = body[data.data_offset - 24..data.data_offset + data.size].to_vec();
    allocation[8..16].copy_from_slice(&(address + 1).to_le_bytes());
    extra.extend(allocation);
    extra.extend(second);
    body.splice(dna..dna, extra);
    std::fs::write(input, body).unwrap();
    std::fs::write(
        input.parent().unwrap().join("external.py"),
        b"# available\n",
    )
    .unwrap();
    // Confirm that the known pointer still resolves in the original fixture.
    assert!(
        f.get_pointer(0, 0, &[PathStep::Field(b"name".to_vec())])
            .unwrap()
            .is_some()
    );
}
#[test]
fn reports_never_erase_pointer_problems_or_independent_provable_items() {
    let work = Work::new();
    let input = work.0.join("invalid.blend");
    invalid_pointer(&input);
    let mut expected = None;
    for flag in [
        None,
        Some("--strict"),
        Some("--permissive"),
        Some("--non-strict"),
    ] {
        let flags = flag.into_iter().collect::<Vec<_>>();
        let t = report(&run("trace", &input, &flags));
        assert_eq!(t["complete"], false);
        assert!(t.get("mode").is_none());
        assert_eq!(t["errors"].as_array().unwrap().len(), 2, "{t}");
        assert_eq!(
            t["usages"].as_array().unwrap().len(),
            1,
            "valid independent asset must survive: {t}"
        );
        for error in t["errors"].as_array().unwrap() {
            assert_eq!(error["kind"], "dangling_pointer");
            assert_eq!(error["status"], "unknown");
            assert!(error["site"]["block_index"].is_number());
            assert_eq!(error["site"]["block_code"], "TX");
            assert_eq!(error["site"]["struct"], "Text");
            assert_eq!(error["site"]["expected_target_type"], "char");
            assert!(
                ["9998", "9999"]
                    .contains(&error["site"]["pointer_value_decimal"].as_str().unwrap())
            );
        }
        if let Some(ref expected) = expected {
            assert_eq!(&t, expected);
        } else {
            expected = Some(t);
        }
        for full in [false, true] {
            let mut json_flags = flags.clone();
            json_flags.push("--json");
            if full {
                json_flags.push("--full-info");
            }
            let l = report(&run("list", &input, &json_flags));
            assert_eq!(l["complete"], false);
            assert!(l.get("mode").is_none());
            assert_eq!(l["errors"].as_array().unwrap().len(), 2);
            assert_eq!(l["problem_counts"]["dangling_pointer"], 2);
            let mut text_flags = flags.clone();
            if full {
                text_flags.push("--full-info");
            }
            let text = run("list", &input, &text_flags);
            assert_eq!(text.status.code(), Some(2));
            let text = String::from_utf8(text.stdout).unwrap();
            assert_eq!(text.matches("[unknown]").count(), 2);
            assert!(text.contains("external.py"));
        }
    }
}
fn pack(input: &Path, out: &Path, flag: Option<&str>) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_oxblend"));
    c.arg("pack").arg(input).arg(out);
    if let Some(flag) = flag {
        c.arg(flag);
    }
    c.output().unwrap()
}
#[test]
fn strict_has_no_publication_and_permissive_discloses_every_unknown_or_missing_item() {
    let work = Work::new();
    for case in ["pointer", "id", "file", "library"] {
        let source = work.0.join(case);
        std::fs::create_dir(&source).unwrap();
        let input = source.join("shot.blend");
        match case {
            "pointer" => invalid_pointer(&input),
            "id" => {
                for name in ["shot.blend", "library.blend"] {
                    std::fs::copy(fixture(&format!("missing-links/{name}")), source.join(name))
                        .unwrap();
                }
            }
            "file" => {
                std::fs::copy(fixture("dependencies/text-name.blend"), &input).unwrap();
            }
            "library" => {
                std::fs::copy(fixture("missing-links/shot.blend"), &input).unwrap();
                std::fs::write(source.join("library.blend"), b"not a blend").unwrap();
            }
            _ => unreachable!(),
        }
        for (index, flag) in [
            None,
            Some("--strict"),
            Some("--permissive"),
            Some("--non-strict"),
        ]
        .into_iter()
        .enumerate()
        {
            for suffix in ["directory", "archive.zip"] {
                let out = source.join(format!("{index}-{suffix}"));
                let j = report(&pack(&input, &out, flag));
                let permissive = matches!(flag, Some("--permissive" | "--non-strict"));
                assert_eq!(j["mode"], if permissive { "permissive" } else { "strict" });
                assert_eq!(j["complete"], false);
                assert!(
                    !j["skipped_items"].as_array().unwrap().is_empty(),
                    "{case}: {j}"
                );
                assert_eq!(out.exists(), permissive, "{case}: {j}");
                if permissive {
                    assert!(
                        j["files"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .all(|f| f["output_written"] == true),
                        "{case}: {j}"
                    );
                    if suffix == "directory" {
                        let info = std::fs::read_to_string(out.join("pack-info.txt")).unwrap();
                        assert!(info.contains("INCOMPLETE"));
                        for item in j["skipped_items"].as_array().unwrap() {
                            assert!(info.contains(item["kind"].as_str().unwrap()));
                        }
                    }
                } else {
                    assert!(!j["directory_created"].as_bool().unwrap());
                    assert_eq!(j["archive_written"], false);
                    assert!(
                        j["files"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .all(|f| f["output_written"] == false)
                    );
                    assert!(
                        j["errors"].as_array().unwrap().iter().any(|e| e["hint"]
                            .as_str()
                            .is_some_and(|s| s.contains("--permissive"))),
                        "{j}"
                    );
                }
            }
        }
    }
}
#[test]
fn complete_list_keeps_exact_bytes_and_run_attributes_record_effective_behavior() {
    let work = Work::new();
    let input = fixture("file-ops/synthetic-empty-seed.blend");
    let baseline = run("list", &input, &["--json"]);
    assert!(baseline.status.success());
    for (index, flag) in [
        None,
        Some("--strict"),
        Some("--permissive"),
        Some("--non-strict"),
    ]
    .into_iter()
    .enumerate()
    {
        let traces = work.0.join(index.to_string());
        std::fs::create_dir(&traces).unwrap();
        let mut c = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        c.arg("list")
            .arg(&input)
            .arg("--json")
            .env("FLATTRACE_DIR", &traces)
            .env("FLATTRACE_LEVEL", "1")
            .env_remove("FLATTRACE_TRACEPARENT");
        if let Some(flag) = flag {
            c.arg(flag);
        }
        let r = c.output().unwrap();
        assert!(r.status.success());
        assert_eq!(r.stdout, baseline.stdout);
        let path = std::fs::read_dir(&traces)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let rows = std::fs::read_to_string(path).unwrap();
        let root: Value = serde_json::from_str(rows.lines().next().unwrap()).unwrap();
        assert_eq!(root["data"]["mode"], "report");
        assert_eq!(
            root["data"]["requested_mode"],
            if matches!(flag, Some("--permissive" | "--non-strict")) {
                "permissive"
            } else {
                "strict"
            }
        );
        assert_eq!(
            root["data"]["mode_source"],
            if flag.is_some() {
                "explicit"
            } else {
                "default"
            }
        );
    }
    // No guessed filesystem output for invalid root bytes in either mode.
    let bad = work.0.join("bad.blend");
    std::fs::write(&bad, b"not blend").unwrap();
    for cmd in ["trace", "list", "overrides"] {
        let r = run(cmd, &bad, &[]);
        assert_eq!(r.status.code(), Some(2));
        assert!(r.stdout.is_empty());
    }
    let _ = json!({});
}

#[test]
fn every_command_has_effective_and_requested_mode_on_the_existing_run_root() {
    let help = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    let commands = help
        .lines()
        .find_map(|line| line.strip_prefix("Commands: "))
        .unwrap();
    let work = Work::new();
    for command in commands.split_whitespace() {
        let traces = work.0.join(command);
        std::fs::create_dir(&traces).unwrap();
        let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .args([command, "--help"])
            .env("FLATTRACE_DIR", &traces)
            .env("FLATTRACE_LEVEL", "1")
            .env_remove("FLATTRACE_TRACEPARENT")
            .output()
            .unwrap();
        assert!(r.status.success(), "{command}");
        let path = std::fs::read_dir(traces)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let body = std::fs::read_to_string(path).unwrap();
        let row: Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
        assert_eq!(row["event"], "run");
        assert_eq!(row["state"], "start");
        // COMMAND --help is normalized to the existing help event, not a
        // package-production attempt. The effective behavior is reporting.
        assert_eq!(row["data"]["command"], "help", "{command}");
        assert_eq!(row["data"]["mode"], "report", "{command}");
        assert_eq!(row["data"]["requested_mode"], "strict");
        assert_eq!(row["data"]["mode_source"], "default");
    }
}
#[test]
fn rejected_duplicate_mode_flags_do_not_relabel_the_first_parsed_request() {
    let work = Work::new();
    let input = fixture("file-ops/synthetic-empty-seed.blend");
    for (label, first, second, expected) in [
        ("strict", "--strict", "--permissive", "strict"),
        ("permissive", "--permissive", "--strict", "permissive"),
    ] {
        let traces = work.0.join(label);
        std::fs::create_dir(&traces).unwrap();
        let out = work.0.join(format!("{label}-package"));
        let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("pack")
            .arg(&input)
            .arg(&out)
            .args([first, second])
            .env("FLATTRACE_DIR", &traces)
            .env("FLATTRACE_LEVEL", "1")
            .env_remove("FLATTRACE_TRACEPARENT")
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(2));
        assert!(r.stdout.is_empty());
        assert!(!out.exists());
        let path = std::fs::read_dir(traces)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let body = std::fs::read_to_string(path).unwrap();
        let row: Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
        assert_eq!(row["data"]["requested_mode"], expected);
        assert_eq!(row["data"]["mode"], expected);
        assert_eq!(row["data"]["mode_source"], "explicit");
    }
}
#[test]
fn explicit_scopes_disclose_missing_exclusions_and_never_hide_unknown_pointers() {
    let work = Work::new();
    let input = work.0.join("image.blend");
    let original = fixture("file-ops/native-4.5.8/build-kit/constructors.blend");
    let reader = BlendFile::open(&original).unwrap();
    let i = reader
        .blocks()
        .iter()
        .position(|b| b.code == *b"IM\0\0")
        .unwrap();
    let st = &reader.schema().structs[reader.struct_index(i).unwrap()];
    let path = st
        .fields
        .iter()
        .find(|f| f.name == b"filepath" || f.name == b"name")
        .unwrap();
    let packed = st.fields.iter().find(|f| f.name == b"packedfile").unwrap();
    let source = work.0.join("outside.png");
    let raw = source.as_os_str().as_encoded_bytes();
    let mut body = reader.read_decompressed_bytes().unwrap();
    let at = reader.blocks()[i].data_offset + path.offset;
    body[at..at + path.size].fill(0);
    body[at..at + raw.len()].copy_from_slice(raw);
    for present in [true, false] {
        if present {
            std::fs::write(&source, b"present").unwrap();
        } else {
            std::fs::remove_file(&source).unwrap();
        }
        std::fs::write(&input, &body).unwrap();
        let out = work.0.join(if present { "present" } else { "missing" });
        let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("pack")
            .arg(&input)
            .arg(&out)
            .arg("--relative-only")
            .output()
            .unwrap();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stdout));
        let j: Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(j["complete"], true);
        assert_eq!(j["self_contained"], false);
        assert_eq!(j["excluded_count"], 1);
        assert_eq!(j["skipped_items"][0]["status"], "excluded");
        assert_eq!(j["skipped_items"][0]["missing"], !present);
        assert!(
            j["skipped_items"][0]["reason"]
                .as_str()
                .unwrap()
                .contains("--relative-only")
        );
        let info = std::fs::read_to_string(out.join("pack-info.txt")).unwrap();
        assert!(info.contains("Complete (requested scope): true"));
        assert!(info.contains("Self-contained: false"));
    }
    // A nonzero packedfile pointer is not proof of packed contents. Inability
    // to prove that state leaves this image subtree unknown, even out of scope.
    let at = reader.blocks()[i].data_offset + packed.offset;
    body[at..at + 8].copy_from_slice(&9999u64.to_le_bytes());
    std::fs::write(&input, &body).unwrap();
    let trace = report(&run("trace", &input, &[]));
    assert_eq!(trace["errors"][0]["status"], "unknown");
    assert_eq!(trace["errors"][0]["site"]["pointer_value_decimal"], "9999");
    let out = work.0.join("unknown");
    let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("pack")
        .arg(&input)
        .arg(&out)
        .arg("--relative-only")
        .output()
        .unwrap();
    let j = report(&r);
    assert_eq!(j["complete"], false);
    assert!(!out.exists());
}

#[test]
fn least_verbose_list_keeps_every_warning_kind_visible() {
    let work = Work::new();
    let original = fixture("stub-chain/packed.blend");
    let reader = BlendFile::open(&original).unwrap();
    let workspace = reader
        .blocks()
        .iter()
        .find(|b| b.code == *b"WS\0\0")
        .unwrap();
    let mut bytes = reader.read_decompressed_bytes().unwrap();
    let width = if reader.header().file_format == 1 {
        32
    } else {
        16 + reader.header().pointer_size
    };
    // Use a genuinely unknown family, not incidental WS/CA noise that a more
    // accurate registry rightly removes. The warning-visibility contract stays.
    let at = workspace.data_offset - width;
    bytes[at..at + 4].copy_from_slice(b"ZZ\0\0");
    let input = work.0.join("unknown-code.blend");
    std::fs::write(&input, bytes).unwrap();
    let full = run("list", &input, &["--json", "--full-info"]);
    assert!(full.status.success());
    let j: Value = serde_json::from_slice(&full.stdout).unwrap();
    let expected = j["warnings"].as_array().unwrap();
    assert!(!expected.is_empty());
    for flags in [vec![], vec!["--json"], vec!["--full-info"]] {
        let r = run("list", &input, &flags);
        assert!(r.status.success());
        let warnings = String::from_utf8(r.stderr)
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|row| row.get("warning").cloned())
            .collect::<Vec<_>>();
        assert_eq!(warnings, *expected);
    }
}
#[test]
fn packed_image_requires_proven_bytes_not_just_a_valid_list_head() {
    let original = fixture("stub-chain/packed.blend");
    let reader = BlendFile::open(&original).unwrap();
    let step = |n: &str| PathStep::Field(n.as_bytes().to_vec());
    let image = reader
        .blocks()
        .iter()
        .position(|b| b.code == *b"IM\0\0")
        .unwrap();
    let entry = reader
        .get_pointer(image, 0, &[step("packedfiles"), step("first")])
        .unwrap()
        .unwrap();
    let packed = reader
        .get_pointer(entry, 0, &[step("packedfile")])
        .unwrap()
        .unwrap();
    let field = reader.schema().structs[reader.struct_index(packed).unwrap()]
        .fields
        .iter()
        .find(|f| f.name == b"data")
        .unwrap();
    let work = Work::new();
    let input = work.0.join("broken-packed.blend");
    let mut body = reader.read_decompressed_bytes().unwrap();
    let at = reader.blocks()[packed].data_offset + field.offset;
    body[at..at + 8].copy_from_slice(&9999u64.to_le_bytes());
    std::fs::write(&input, body).unwrap();
    let t = report(&run("trace", &input, &[]));
    assert_eq!(t["complete"], false);
    let error = t["errors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["site"]["struct"] == "PackedFile")
        .unwrap();
    assert_eq!(error["status"], "unknown");
    assert_eq!(error["site"]["pointer_value_decimal"], "9999");
    for flags in [vec!["--json"], vec!["--json", "--full-info"]] {
        let l = report(&run("list", &input, &flags));
        assert_eq!(l["problem_counts"], t["problem_counts"]);
        if flags.contains(&"--full-info") {
            assert!(
                l["packed"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|p| p["packed"].is_null())
            );
        }
    }
    let out = work.0.join("strict");
    let j = report(&pack(&input, &out, None));
    assert!(!out.exists());
    assert_eq!(j["complete"], false);
    let out = work.0.join("partial");
    let j = report(&pack(&input, &out, Some("--permissive")));
    assert!(out.exists());
    assert_eq!(j["complete"], false);
}
#[test]
fn overrides_mode_flags_are_noops_and_package_sequence_obeys_production_policy() {
    let source = fixture("trace-owner/pointer-kinds/id-only-data.blend");
    let plain = run("overrides", &source, &[]);
    for flag in ["--strict", "--permissive", "--non-strict"] {
        let r = run("overrides", &source, &[flag]);
        assert_eq!(r.status.code(), plain.status.code());
        assert_eq!(r.stdout, plain.stdout);
    }
    let work = Work::new();
    for (name, flag, permissive) in [
        ("strict", None, false),
        ("partial", Some("--permissive"), true),
    ] {
        let out = work.0.join(name);
        let mut c = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        c.arg("pack-sequence")
            .arg(fixture("missing-links/shot.blend"))
            .arg("-t")
            .arg(&out);
        if let Some(flag) = flag {
            c.arg(flag);
        }
        let j = report(&c.output().unwrap());
        assert_eq!(j["complete"], false);
        assert_eq!(out.exists(), permissive);
        assert_eq!(j["mode"], if permissive { "permissive" } else { "strict" });
    }
}
