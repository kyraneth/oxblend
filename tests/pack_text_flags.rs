//! Native Text filepath ownership: saved TXT_ISMEM (bit 2), not dirty/content.
use serde_json::Value;
use std::{path::Path, process::Command};
// A well-formed SDNA container is not proof that Text.flags has its native
// integer-scalar role. These deliberately malformed schemas isolate that role.
fn forged_flags(field_type: u16, field_name: &[u8], value: &[u8]) -> Vec<u8> {
    flags_layout(field_type, field_name, value, true)
}
fn flags_layout(field_type: u16, field_name: &[u8], value: &[u8], include_flags: bool) -> Vec<u8> {
    fn align(bytes: &mut Vec<u8>) {
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
    }
    fn block(out: &mut Vec<u8>, code: &[u8; 4], address: u64, sdna: u32, body: &[u8], count: u32) {
        out.extend(code);
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(address.to_le_bytes());
        out.extend(sdna.to_le_bytes());
        out.extend(count.to_le_bytes());
        out.extend(body);
    }
    let names: [&[u8]; 6] = [
        b"name[64]",
        b"*lib",
        b"id",
        b"*filepath",
        field_name,
        b"bytes[128]",
    ];
    let types: [&[u8]; 8] = [
        b"char", b"void", b"int", b"float", b"short", b"ID", b"Text", b"Bytes",
    ];
    let sizes: [u16; 8] = [
        1,
        0,
        4,
        4,
        2,
        72,
        (80 + if include_flags { value.len() } else { 0 }) as u16,
        128,
    ];
    let mut dna = b"SDNANAME".to_vec();
    dna.extend((names.len() as u32).to_le_bytes());
    for name in names {
        dna.extend(name);
        dna.push(0);
    }
    align(&mut dna);
    dna.extend(b"TYPE");
    dna.extend((types.len() as u32).to_le_bytes());
    for ty in types {
        dna.extend(ty);
        dna.push(0);
    }
    align(&mut dna);
    dna.extend(b"TLEN");
    for size in sizes {
        dna.extend(size.to_le_bytes());
    }
    align(&mut dna);
    dna.extend(b"STRC");
    dna.extend(3u32.to_le_bytes());
    for (ty, fields) in [
        (5u16, vec![(0u16, 0u16), (1, 1)]),
        (
            6,
            if include_flags {
                vec![(5, 2), (0, 3), (field_type, 4)]
            } else {
                vec![(5, 2), (0, 3)]
            },
        ),
        (7, vec![(0, 5)]),
    ] {
        dna.extend(ty.to_le_bytes());
        dna.extend((fields.len() as u16).to_le_bytes());
        for (ty, name) in fields {
            dna.extend(ty.to_le_bytes());
            dna.extend(name.to_le_bytes());
        }
    }
    let mut text = b"TXForgedFlags".to_vec();
    text.resize(64, 0);
    text.extend(0u64.to_le_bytes());
    text.extend(0x2000u64.to_le_bytes());
    if include_flags {
        text.extend(value);
    }
    let mut path = b"//must-exist.txt\0".to_vec();
    path.resize(128, 0);
    let mut out = b"BLENDER-v405".to_vec();
    block(&mut out, b"TX\0\0", 0x1000, 1, &text, 1);
    block(&mut out, b"DATA", 0x2000, 2, &path, 1);
    block(&mut out, b"DNA1", 0, 0, &dna, 1);
    block(&mut out, b"ENDB", 0, 0, &[], 0);
    out
}
#[test]
fn malformed_text_flags_never_prove_embedded_ownership_or_publish_strictly() {
    let work = std::env::temp_dir().join(format!(
        "oxblend-text-flags-malformed-{}",
        std::process::id()
    ));
    std::fs::create_dir(&work).unwrap();
    for (label, ty, name, value) in [
        (
            "pointer",
            1,
            b"*flags".as_slice(),
            4u64.to_le_bytes().to_vec(),
        ),
        ("short", 4, b"flags".as_slice(), 4i16.to_le_bytes().to_vec()),
        ("float", 3, b"flags".as_slice(), 4f32.to_le_bytes().to_vec()),
        (
            "array",
            2,
            b"flags[1]".as_slice(),
            4i32.to_le_bytes().to_vec(),
        ),
    ] {
        let source = work.join(format!("{label}.blend"));
        std::fs::write(&source, forged_flags(ty, name, &value)).unwrap();
        for command in ["trace", "pack"] {
            let target = work.join(format!("{label}-package"));
            let mut argv = Command::new(env!("CARGO_BIN_EXE_oxblend"));
            argv.arg(command).arg(&source);
            if command == "pack" {
                argv.arg(&target).arg("-p").arg(&work);
            }
            let out = argv.output().unwrap();
            let report: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(out.status.code(), Some(2), "{label}/{command}: {report}");
            assert_eq!(report["complete"], false, "{label}/{command}: {report}");
            assert!(
                !target.exists(),
                "malformed flags must never authorize strict publication"
            );
        }
    }
    std::fs::remove_dir_all(work).unwrap();
}
#[test]
fn unknown_text_storage_preserves_path_facts_and_independent_diagnostics() {
    let work = std::env::temp_dir().join(format!("oxblend-text-unknown-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    std::fs::write(work.join("must-exist.txt"), b"known available content").unwrap();
    for (label, ty, name, value, present) in [
        (
            "missing",
            2,
            b"flags".as_slice(),
            0i32.to_le_bytes().to_vec(),
            false,
        ),
        (
            "pointer",
            1,
            b"*flags".as_slice(),
            4u64.to_le_bytes().to_vec(),
            true,
        ),
        (
            "short",
            4,
            b"flags".as_slice(),
            4i16.to_le_bytes().to_vec(),
            true,
        ),
    ] {
        for (site, pointer, expected) in [
            ("known", 0x2000u64, None),
            ("dangling", 0x9999, Some("dangling_pointer")),
            ("wrong-type", 0x1000, Some("pointer_target_type")),
            ("empty", 0, None),
        ] {
            let mut body = flags_layout(ty, name, &value, present);
            // Native format-0 header12+BHead24+ID72 precede Text.filepath.
            body[108..116].copy_from_slice(&pointer.to_le_bytes());
            let source = work.join(format!("{label}-{site}.blend"));
            std::fs::write(&source, body).unwrap();
            let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
                .arg("trace")
                .arg(&source)
                .output()
                .unwrap();
            let j: Value = serde_json::from_slice(&out.stdout).unwrap();
            if site == "empty" {
                assert_eq!(out.status.code(), Some(0), "{j}");
                assert!(j["usages"].as_array().unwrap().is_empty());
                continue;
            }
            assert_eq!(out.status.code(), Some(2), "{j}");
            assert!(
                j["errors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["field_path"] == serde_json::json!(["flags"])
                        && e["site"]["text_storage"] == "unknown"),
                "{j}"
            );
            if let Some(kind) = expected {
                assert_eq!(j["errors"][0]["kind"], kind, "{j}");
            } else {
                assert_eq!(
                    j["usages"].as_array().unwrap().len(),
                    1,
                    "known filepath fact erased: {j}"
                );
                assert_eq!(j["usages"][0]["exists"], true);
            }
            let target = work.join(format!("{label}-{site}-package"));
            let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
                .arg("pack")
                .arg(&source)
                .arg(&target)
                .arg("-p")
                .arg(&work)
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(2));
            assert!(!target.exists());
        }
    }
    std::fs::remove_dir_all(work).unwrap();
}
#[test]
fn embedded_text_history_is_informational_and_external_text_is_required() {
    let work = std::env::temp_dir().join(format!("oxblend-text-flags-{}", std::process::id()));
    std::fs::create_dir(&work).unwrap();
    let source = work.join("texts.blend");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text-flags/texts.blend"),
        &source,
    )
    .unwrap();
    std::fs::write(work.join("external.txt"), "EXTERNAL_ORIGINAL\n").unwrap();
    let call = |command: &str, target: Option<&Path>| {
        let mut argv = Command::new(env!("CARGO_BIN_EXE_oxblend"));
        argv.arg(command).arg(&source);
        if let Some(target) = target {
            argv.arg(target).arg("-p").arg(&work);
        }
        let out = argv.output().unwrap();
        let value: Value = serde_json::from_slice(&out.stdout).unwrap();
        (out.status.code(), value)
    };
    let (exit, trace) = call("trace", None);
    assert_eq!(exit, Some(0), "{trace}");
    let usages = trace["usages"].as_array().unwrap();
    assert_eq!(
        usages.len(),
        2,
        "only the clean and dirty external Texts are dependencies: {trace}"
    );
    assert!(usages.iter().all(|u| u["asset_path"] == "//external.txt"));
    let info = trace["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["kind"] == "embedded_text_filepath")
        .unwrap();
    assert_eq!(info["status"], "informational");
    assert!(
        info["asset_path"]
            .as_str()
            .unwrap()
            .ends_with("historical.py")
    );
    assert_eq!(info["flags"].as_i64().unwrap() & 4, 4);
    let package = work.join("package");
    let (exit, packed) = call("pack", Some(&package));
    assert_eq!(exit, Some(0), "{packed}");
    assert_eq!(packed["complete"], true);
    assert!(package.join("texts.blend").is_file() && package.join("external.txt").is_file());
    // The external text still has a serialized cached body; do not mistake
    // that body (or dirty state) for native TXT_ISMEM storage ownership.
    std::fs::remove_file(work.join("external.txt")).unwrap();
    let refused = work.join("refused");
    let (exit, packed) = call("pack", Some(&refused));
    assert_eq!(exit, Some(2));
    assert_eq!(packed["complete"], false);
    assert!(!refused.exists());
    std::fs::remove_dir_all(work).unwrap();
}
