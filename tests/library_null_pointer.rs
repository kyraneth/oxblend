//! Synthetic wire-level linked-library null compatibility, through the real CLI.
//! No Blender installation or production files are used. See the fixture generator.
#[path = "support/diagnostic_metadata.rs"]
mod diagnostic_metadata;
use oxblend::{BlendFile, value::PathStep};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::Write,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let work = std::env::temp_dir().join(format!(
            "oxblend-library-null-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&work).unwrap();
        for sub in ["project", "assets"] {
            std::fs::create_dir(work.join(sub)).unwrap();
        }
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/library-null");
        for entry in std::fs::read_dir(fixtures).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "blend") {
                let sub = if path.file_name().unwrap() == "image.blend" {
                    "assets"
                } else {
                    "project"
                };
                std::fs::copy(&path, work.join(sub).join(path.file_name().unwrap())).unwrap();
            }
        }
        // A valid one-pixel RGBA PNG, including checksummed IHDR/IDAT/IEND chunks.
        std::fs::write(work.join("assets/pixel.png"), unhex("89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c63f8cfc0f01f00050001ff89993d1d0000000049454e44ae426082")).unwrap();
        Self(work)
    }
    fn input(&self, case: &str) -> PathBuf {
        self.0.join("project").join(format!("{case}-root.blend"))
    }
    fn child(&self, case: &str) -> PathBuf {
        self.0.join("project").join(format!("{case}.blend"))
    }
    fn sources(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        ["project", "assets"]
            .into_iter()
            .flat_map(|sub| snapshot(&self.0.join(sub)))
            .collect()
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(snapshot(&path));
        } else {
            files.insert(path.clone(), std::fs::read(path).unwrap());
        }
    }
    files
}
fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn json_path(value: &Value) -> PathBuf {
    PathBuf::from(OsString::from_vec(unhex(value.as_str().unwrap())))
}
fn cli(command: &str, input: &Path, rest: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg(command)
        .arg(input)
        .args(rest)
        .output()
        .unwrap()
}
fn json(output: &Output, exit: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(exit),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}; stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    diagnostic_metadata::normalize(&mut value);
    value
}
fn warnings(report: &Value) -> &Vec<Value> {
    report["warnings"].as_array().unwrap()
}
fn check_native_warnings(report: &Value) {
    assert_eq!(report["complete"], true, "{report}");
    assert!(report["errors"].as_array().unwrap().is_empty(), "{report}");
    let compatibility: Vec<_> = warnings(report)
        .iter()
        .filter(|w| w["kind"] == "library_null_pointer")
        .collect();
    assert_eq!(compatibility.len(), 3, "{report}");
    for warning in compatibility {
        assert_eq!(warning["pack_action"], "clear_to_null");
        assert!(warning["offset"].as_u64().unwrap() > 0);
        assert_ne!(warning["pointer_address"], "0");
        assert!(!warning["library_chain_hex"].as_array().unwrap().is_empty());
    }
    assert!(
        report["usages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["asset_path"] == "//pixel.png")
    );
    assert_eq!(report["referenced_paths_exist"], true);
}

#[test]
fn linked_roles_warn_without_incompleteness_in_strict_nonstrict_and_list() {
    let work = Work::new();
    let before = work.sources();
    let input = work.input("normal");
    let strict = json(&cli("trace", &input, &["--strict".as_ref()]), 0);
    let nonstrict = json(&cli("trace", &input, &["--non-strict".as_ref()]), 0);
    check_native_warnings(&strict);
    assert_eq!(strict, nonstrict);
    for mode in [
        oxblend::trace::PointerMode::Strict,
        oxblend::trace::PointerMode::NonStrict,
    ] {
        let mut options = oxblend::trace::TraceOptions::default();
        options.pointers = mode;
        let api = oxblend::trace::trace(&input, options).unwrap();
        assert!(api.complete);
        assert!(api.errors.is_empty());
        assert_eq!(
            api.warnings
                .iter()
                .filter(|w| matches!(w, oxblend::trace::TraceWarning::LibraryNullPointer { .. }))
                .count(),
            3
        );
    }
    for flags in [
        vec![],
        vec!["--json".as_ref()],
        vec!["--json".as_ref(), "--full-info".as_ref()],
    ] {
        let result = cli("list", &input, &flags);
        assert_eq!(result.status.code(), Some(0), "{result:?}");
        let stderr = std::str::from_utf8(&result.stderr).unwrap();
        let notices: Vec<Value> = stderr
            .lines()
            .map(|line| {
                let mut value = serde_json::from_str(line).unwrap();
                diagnostic_metadata::normalize(&mut value);
                value
            })
            .collect();
        // #31 reports-always-tell-everything: ordinary list no longer hides
        // existing warnings of other kinds. Baseline 97be77a trace/full-info
        // already included this fixture's Camera (CA) unknown-asset warning.
        let emitted: Vec<_> = notices.iter().map(|n| n["warning"].clone()).collect();
        let compatibility: Vec<_> = emitted
            .iter()
            .filter(|w| w["kind"] == "library_null_pointer")
            .collect();
        let expected: Vec<_> = warnings(&strict)
            .iter()
            .filter(|w| w["kind"] == "library_null_pointer")
            .collect();
        assert_eq!(
            compatibility, expected,
            "all three qualified compatibility records must remain exact"
        );
        assert_eq!(
            &emitted,
            warnings(&strict),
            "verbosity must not hide an existing diagnostic"
        );
        if flags.is_empty() {
            let text = std::str::from_utf8(&result.stdout).unwrap();
            assert!(text.contains("pixel.png"));
            assert!(text.contains("image.blend"));
            assert!(!text.contains("[missing]"));
        } else {
            let data = json(&result, 0);
            let files = if flags.len() == 2 {
                &data["files"]
            } else {
                &data
            };
            assert!(
                files
                    .as_object()
                    .unwrap()
                    .values()
                    .flat_map(|v| v.as_array().unwrap())
                    .any(|v| v.as_str().is_some_and(|p| p.ends_with("pixel.png")))
            );
            if flags.len() == 2 {
                assert_eq!(warnings(&data), warnings(&strict));
            } else {
                assert!(
                    data.get("warnings").is_none(),
                    "member-only JSON stays compatible"
                );
            }
        }
    }
    assert_eq!(work.sources(), before);
}

fn clear_paths(file: &BlendFile) -> BTreeMap<usize, (usize, u64)> {
    let mut expected = BTreeMap::new();
    for (block, head) in file.blocks().iter().enumerate() {
        if head.code != *b"DATA" {
            continue;
        }
        let path = match file.dna_type_id(block).unwrap() {
            b"NodesModifierData" => vec![PathStep::Field(b"node_group".to_vec())],
            b"bTransLikeConstraint" => vec![PathStep::Field(b"tar".to_vec())],
            b"DriverVar" => vec![
                PathStep::Field(b"targets".to_vec()),
                PathStep::Index(1),
                PathStep::Field(b"id".to_vec()),
            ],
            _ => continue,
        };
        // Independent wire offsets from generate.py: ModifierData is two
        // pointers + int + name[64]; DriverVar has two pointers + short, then
        // targets of pointer + short + marker[8]. Never trust the clear plan.
        let relative = match file.dna_type_id(block).unwrap() {
            b"NodesModifierData" => 8 + 8 + 4 + 64,
            b"bTransLikeConstraint" => 0,
            b"DriverVar" => (8 + 8 + 2) + (8 + 2 + 8),
            _ => unreachable!(),
        };
        let oxblend::value::Value::Pointer(address) = file.get(block, 0, &path, true).unwrap()
        else {
            panic!("not a pointer")
        };
        assert_ne!(address, 0);
        assert_eq!(
            &file.payload(block).unwrap()[relative..relative + 8],
            &address.to_le_bytes()
        );
        expected.insert(head.data_offset + relative, (block, address));
    }
    expected
}
fn encode_sources(work: &Work, container: &str) {
    if container == "raw" {
        return;
    }
    for (path, bytes) in work.sources() {
        if path.extension().is_none_or(|ext| ext != "blend") {
            continue;
        }
        let encoded = if container == "gzip" {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(&bytes).unwrap();
            encoder.finish().unwrap()
        } else {
            zstd::stream::encode_all(bytes.as_slice(), 0).unwrap()
        };
        std::fs::write(path, encoded).unwrap();
    }
}

#[test]
fn pack_clears_selected_and_unselected_sites_with_guarded_byte_ranges_in_all_containers() {
    for (container, compress) in [
        ("raw", false),
        ("gzip", false),
        ("zstd", false),
        ("raw", true),
    ] {
        let work = Work::new();
        encode_sources(&work, container);
        let before = work.sources();
        let input = work.input("normal");
        let child = work.child("normal");
        let reader = BlendFile::open(&child).unwrap();
        let expected = clear_paths(&reader);
        assert_eq!(
            expected.len(),
            6,
            "two instances of every nullable role, including unselected owners"
        );
        let out = work.0.join("packed");
        let mut args = vec![
            out.as_os_str(),
            "--durability".as_ref(),
            "each-file".as_ref(),
        ];
        if compress {
            args.push("--compress".as_ref());
        }
        let mut dry_args = args.clone();
        dry_args.push("--dry-run".as_ref());
        let dry = json(&cli("pack", &input, &dry_args), 0);
        assert_eq!(dry["complete"], true, "{container}: {dry}");
        assert_eq!(dry["planning_complete"], true);
        assert_eq!(dry["directory_created"], false);
        assert_eq!(dry["noop"], true);
        assert_eq!(dry["durability_completed"], false);
        assert!(
            dry["files"]
                .as_array()
                .unwrap()
                .iter()
                .all(|f| f["output_written"] == false)
        );
        assert_eq!(dry["pack_info"]["output_written"], false);
        assert!(!out.exists());
        assert_eq!(work.sources(), before);
        let packed = json(&cli("pack", &input, &args), 0);
        assert_eq!(packed["complete"], true, "{container}: {packed}");
        assert_eq!(packed["planning_complete"], true);
        assert_eq!(packed["errors"], serde_json::json!([]));
        assert_eq!(packed["missing"], serde_json::json!([]));
        assert_eq!(packed["durability"], "each-file");
        assert_eq!(packed["durability_completed"], true);
        assert_eq!(packed["pack_info"]["output_written"], true);
        assert_eq!(packed["pack_info"]["durability_completed"], true);
        assert_eq!(
            packed["library_pointer_clears"],
            dry["library_pointer_clears"]
        );
        let clears = packed["library_pointer_clears"].as_array().unwrap();
        assert_eq!(clears.len(), expected.len());
        let mut offsets = BTreeSet::new();
        for clear in clears {
            assert_eq!(json_path(&clear["owner_file_hex"]), child);
            assert_eq!(clear["kind"], "library_null_pointer");
            assert_eq!(clear["action"], "clear_to_null");
            assert_eq!(clear["replacement_pointer"], "0");
            assert_eq!(clear["width"], 8);
            let offset = clear["offset"].as_u64().unwrap() as usize;
            assert!(offsets.insert(offset), "duplicate clear site");
            let (block, address) = expected[&offset];
            assert_eq!(clear["block_index"], block);
            assert_eq!(clear["expected_pointer"], address.to_string());
            assert!(!clear["steps"].as_array().unwrap().is_empty());
        }
        assert_eq!(offsets, expected.keys().copied().collect());
        let mut child_output = None;
        let mut image_copied = false;
        for member in packed["files"].as_array().unwrap() {
            assert_eq!(member["output_written"], true);
            assert_eq!(member["durability_completed"], true);
            let source = json_path(&member["input_path_hex"]);
            let output = json_path(&member["output_path_hex"]);
            assert_ne!(source, output);
            assert!(output.starts_with(&out));
            if source.extension().is_some_and(|ext| ext == "png") {
                assert_eq!(std::fs::read(output).unwrap(), before[&source]);
                image_copied = true;
                continue;
            }
            let decoded_before = BlendFile::open(&source)
                .unwrap()
                .read_decompressed_bytes()
                .unwrap();
            let output_reader = BlendFile::open(&output).unwrap();
            let decoded_after = output_reader.read_decompressed_bytes().unwrap();
            assert_eq!(decoded_before.len(), decoded_after.len());
            let mut allowed = vec![false; decoded_before.len()];
            for rewrite in packed["rewrites"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| json_path(&r["input_path_hex"]) == source)
            {
                assert_eq!(rewrite["complete"], true);
                assert_eq!(rewrite["output_written"], true);
                assert_eq!(rewrite["errors"], serde_json::json!([]));
                let family = if compress { "zstd" } else { container };
                assert_eq!(rewrite["container_family"], family);
                if compress || (family != "raw" && member["rewritten"] == true) {
                    assert_eq!(rewrite["output_decoder_verified"], true);
                }
                for edit in rewrite["edits"].as_array().unwrap() {
                    let start = edit["offset"].as_u64().unwrap() as usize;
                    let length = edit["write_length"].as_u64().unwrap() as usize;
                    allowed[start..start + length].fill(true);
                }
            }
            if source == child {
                assert_eq!(member["rewritten"], true);
                assert!(
                    allowed.iter().any(|b| *b),
                    "outside-project image library path is relocated alongside clears"
                );
                for (&offset, &(_, address)) in &expected {
                    assert_eq!(&decoded_before[offset..offset + 8], &address.to_le_bytes());
                    assert_eq!(&decoded_after[offset..offset + 8], &[0; 8]);
                    allowed[offset..offset + 8].fill(true);
                }
                // Typed reads independently confirm all six pointer values, not merely report entries.
                for (block, _) in expected.values() {
                    let path = match output_reader.dna_type_id(*block).unwrap() {
                        b"NodesModifierData" => vec![PathStep::Field(b"node_group".to_vec())],
                        b"bTransLikeConstraint" => vec![PathStep::Field(b"tar".to_vec())],
                        _ => vec![
                            PathStep::Field(b"targets".to_vec()),
                            PathStep::Index(1),
                            PathStep::Field(b"id".to_vec()),
                        ],
                    };
                    assert!(matches!(
                        output_reader.get(*block, 0, &path, true).unwrap(),
                        oxblend::value::Value::Pointer(0)
                    ));
                }
                child_output = Some(output.clone());
            }
            for (offset, (old, new)) in decoded_before.iter().zip(&decoded_after).enumerate() {
                if !allowed[offset] {
                    assert_eq!(
                        old,
                        new,
                        "{container}: {} unexpected byte change at {offset}",
                        source.display()
                    );
                }
            }
        }
        assert!(image_copied);
        assert!(child_output.is_some());
        let root_output = packed["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| json_path(&f["input_path_hex"]) == input)
            .map(|f| json_path(&f["output_path_hex"]))
            .unwrap();
        let retraced = json(&cli("trace", &root_output, &["--strict".as_ref()]), 0);
        assert_eq!(retraced["complete"], true);
        assert_eq!(retraced["errors"], serde_json::json!([]));
        assert!(
            warnings(&retraced)
                .iter()
                .all(|w| w["kind"] != "library_null_pointer")
        );
        assert_eq!(retraced["referenced_paths_exist"], true);
        assert!(retraced["usages"].as_array().unwrap().iter().any(|u| {
            u["asset_path"]
                .as_str()
                .is_some_and(|s| s.ends_with("pixel.png"))
        }));
        let published = snapshot(&out);
        let refused = cli("pack", &input, &args);
        assert_eq!(
            refused.status.code(),
            Some(2),
            "existing output must not be merged: {refused:?}"
        );
        assert_eq!(snapshot(&out), published);
        assert_eq!(
            work.sources(),
            before,
            "all physical source bytes, including compression, stay immutable"
        );
    }
}

#[test]
fn same_pointers_in_main_file_remain_hard_errors_and_never_publish() {
    let work = Work::new();
    let before = work.sources();
    let input = work.child("normal");
    let reader = BlendFile::open(&input).unwrap();
    for &(block, _) in clear_paths(&reader).values() {
        let path = match reader.dna_type_id(block).unwrap() {
            b"NodesModifierData" => vec![PathStep::Field(b"node_group".to_vec())],
            b"bTransLikeConstraint" => vec![PathStep::Field(b"tar".to_vec())],
            _ => vec![
                PathStep::Field(b"targets".to_vec()),
                PathStep::Index(1),
                PathStep::Field(b"id".to_vec()),
            ],
        };
        assert_eq!(
            reader.get_pointer(block, 0, &path).unwrap_err().kind,
            "dangling_pointer"
        );
    }
    for flag in ["--strict", "--non-strict"] {
        // Root asset discovery need not expand local ID dependency edges.
        // Whether visited or not, a main-file pointer never earns this warning.
        let result = cli("trace", &input, &[flag.as_ref()]);
        let code = result.status.code().unwrap();
        assert!(matches!(code, 0 | 2));
        let report = json(&result, code);
        assert!(
            warnings(&report)
                .iter()
                .all(|w| w["kind"] != "library_null_pointer")
        );
    }
    let out = work.0.join("main-refused");
    let report = json(&cli("pack", &input, &[out.as_os_str()]), 2);
    assert_eq!(report["complete"], false);
    assert_eq!(report["planning_complete"], false);
    assert!(!out.exists());
    assert_eq!(work.sources(), before);
}

#[test]
fn wrong_discriminators_inactive_targets_structure_and_present_invalid_targets_refuse() {
    let work = Work::new();
    let before = work.sources();
    for case in [
        "modifier-type",
        "modifier-name",
        "modifier-header",
        "modifier-extent",
        "modifier-owner",
        "constraint-type",
        "constraint-owner",
        "constraint-data",
        "structure",
        "driver-inactive",
        "driver-capacity",
        "driver-idtype",
        "wrong-id",
        "ambiguous",
    ] {
        let input = work.input(case);
        for flag in ["--strict", "--non-strict"] {
            let report = json(&cli("trace", &input, &[flag.as_ref()]), 2);
            assert_eq!(report["complete"], false, "{case}: {report}");
            assert!(
                !report["errors"].as_array().unwrap().is_empty(),
                "{case}: {report}"
            );
        }
        let out = work.0.join(format!("refused-{case}"));
        let report = json(&cli("pack", &input, &[out.as_os_str()]), 2);
        assert_eq!(report["complete"], false, "{case}: {report}");
        assert_eq!(report["planning_complete"], false, "{case}: {report}");
        assert!(report["files"].as_array().unwrap().is_empty());
        assert!(!out.exists(), "{case}: no partial destination");
    }
    assert_eq!(work.sources(), before);
}
