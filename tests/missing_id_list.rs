//! Native collection A/B: library B removed without re-saving the shot.
use serde_json::Value;
use std::{path::PathBuf, process::Command};
#[test]
fn missing_id_text_is_one_byte_safe_line_even_with_control_or_non_utf8_names() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/missing-links");
    let work = std::env::temp_dir().join(format!("oxblend-id-bytes-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    std::fs::copy(dir.join("library.blend"), work.join("library.blend")).unwrap();
    let reader = oxblend::BlendFile::open(dir.join("shot.blend")).unwrap();
    let field_path = [oxblend::value::PathStep::Field(b"name".to_vec())];
    let stub=reader.blocks().iter().enumerate().find(|(i,b)| b.code==*b"ID\0\0" && matches!(reader.get(*i,0,&field_path,true),Ok(oxblend::value::Value::Bytes(ref name)) if name==b"GRB")).unwrap().0;
    let field = reader.schema().structs[reader.struct_index(stub).unwrap()]
        .fields
        .iter()
        .find(|f| f.name == b"name")
        .unwrap();
    for name in [b"GRbad\nname".as_slice(), b"GRbad\xffname"] {
        let mut bytes = reader.read_decompressed_bytes().unwrap();
        let at = reader.blocks()[stub].data_offset + field.offset;
        bytes[at..at + field.size].fill(0);
        bytes[at..at + name.len()].copy_from_slice(name);
        let input = work.join("shot.blend");
        std::fs::write(&input, bytes).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("list")
            .arg(&input)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        let text = String::from_utf8(out.stdout).unwrap();
        assert_eq!(
            text.lines().count(),
            3,
            "one owner, one library, one byte-safe ID marker: {text}"
        );
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("[missing linked ID]"))
                .count(),
            1
        );
        assert!(
            text.contains(&oxblend::value::hex(name)),
            "raw identity must survive: {text}"
        );
    }
    std::fs::remove_dir_all(work).unwrap();
}
#[test]
fn missing_id_listing_preserves_existing_file_and_marks_only_the_absent_id() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/missing-links");
    for flags in [
        vec!["--json"],
        vec!["--json", "--full-info"],
        vec!["--json", "--low-memory"],
        vec![],
        vec!["--full-info"],
        vec!["--low-memory"],
    ] {
        let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
            .arg("list")
            .arg(dir.join("shot.blend"))
            .args(&flags)
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(2));
        assert!(
            !r.stdout.is_empty(),
            "{}",
            String::from_utf8_lossy(&r.stderr)
        );
        if flags.contains(&"--json") {
            let j: Value = serde_json::from_slice(&r.stdout).unwrap();
            assert_eq!(j["complete"], false);
            assert_eq!(
                j["missing_link_counts"],
                serde_json::json!({"missing_id":1,"missing_library":0})
            );
            let link = &j["missing_links"][0];
            assert_eq!(link["kind"], "missing_id");
            assert_eq!(link["id_name"], "GRB");
            assert_eq!(link["id_code"], "GR");
            assert_eq!(
                link["library_path"],
                dir.join("library.blend").to_str().unwrap()
            );
            assert_eq!(
                link["referencing_owner_chain"],
                serde_json::json!([dir.join("shot.blend")])
            );
            assert_eq!(
                j["files"][dir.join("shot.blend").to_str().unwrap()],
                serde_json::json!([{"path":dir.join("library.blend"),"missing":false}])
            );
            if flags.contains(&"--full-info") {
                assert!(!j["usages"].as_array().unwrap().is_empty());
                assert!(j["warnings"].is_array());
                assert!(j["packed"].is_array());
            }
        } else {
            let text = String::from_utf8(r.stdout).unwrap();
            assert!(text.contains("[missing linked ID]"));
            for needle in ["GRB", "GR", "library.blend", "shot.blend"] {
                assert!(text.contains(needle));
            }
            assert!(!text.contains("library.blend [missing]"));
        }
    }
}
