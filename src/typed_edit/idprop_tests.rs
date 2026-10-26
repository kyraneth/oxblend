use super::*;
use crate::file_ops::graph::Graph;

fn fixture(version: &[u8; 3], mutation: &str) -> Vec<u8> {
    // Synthetic fixtures use the committed native 4.5.8 SDNA, with a
    // declared 502/503 header. Native 5.2.1 and beta proofs are separate.
    let mut seed = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/file-ops/native-4.5.8/build-kit/empty.blend"),
    )
    .unwrap();
    seed[9..12].copy_from_slice(b"502");
    let f = BlendFile::from_bytes(seed).unwrap();
    let mut g = Graph::new(&f).unwrap();
    let scene = g.new_typed("Scene", None).unwrap();
    g.records[scene].code = *b"SC\0\0";
    g.cstring(scene, &["id", "name"], "SCtest").unwrap();
    let group = g.property_group(scene, "settings").unwrap();
    g.ptr(scene, &["id", "properties"], Some(group)).unwrap();
    let p = g.property_string(group, "version", "5.3.0-alpha").unwrap();
    let raw = g.target(p, &["data", "pointer"]).unwrap().unwrap();
    let other = g.property_string(group, "alias", "5.3.0-alpha").unwrap();
    let second = g.new_typed("Scene", None).unwrap();
    g.records[second].code = *b"SC\0\0";
    g.cstring(second, &["id", "name"], "SCother").unwrap();
    let second_group = g.property_group(second, "settings").unwrap();
    g.ptr(second, &["id", "properties"], Some(second_group))
        .unwrap();
    let second_property = g
        .property_string(second_group, "foreign", "5.3.0-alpha")
        .unwrap();
    let foreign = g
        .target(second_property, &["data", "pointer"])
        .unwrap()
        .unwrap();
    let mut decoded = g.decoded().unwrap();
    // Deliberately corrupt a valid fixture AFTER constructor validation; these
    // bytes are hostile test inputs, not a production edit/write surface.
    let parsed = BlendFile::from_bytes(decoded.clone()).unwrap();
    let block = |record: usize| {
        parsed
            .blocks()
            .iter()
            .position(|b| b.old_address == g.records[record].address)
            .unwrap()
    };
    let mut patch = |record: usize, names: &[&str], value: u64| {
        let b = block(record);
        let steps = names
            .iter()
            .map(|n| PathStep::Field(n.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        let loc = parsed.locate_field(b, 0, &steps).unwrap();
        let offset = parsed.block(b).unwrap().data_offset + loc.offset;
        decoded[offset..offset + loc.field.size]
            .copy_from_slice(&value.to_le_bytes()[..loc.field.size]);
    };
    match mutation {
        "type" => patch(p, &["type"], 1),
        "subtype" => patch(p, &["subtype"], 1),
        "len" => patch(p, &["len"], 11),
        "capacity" => patch(p, &["totallen"], 99),
        "alias" => patch(other, &["data", "pointer"], g.records[raw].address),
        "opaque-alias" => {
            let b = block(raw);
            let offset = parsed.block(b).unwrap().data_offset;
            decoded[offset..offset + 8].copy_from_slice(&g.records[raw].address.to_le_bytes());
        }
        "cross-owner" => patch(p, &["data", "pointer"], g.records[foreign].address),
        "" => (),
        _ => panic!("unknown mutation"),
    }
    decoded[9..12].copy_from_slice(version);
    decoded
}
fn exercise(version: &[u8; 3], mutation: &str, change: &str) -> Result<()> {
    let root = std::env::temp_dir().join(format!(
        "oxblend-idprop-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("in.blend");
    let output = root.join("out.blend");
    std::fs::write(&input, fixture(version, mutation)).unwrap();
    let run = || -> Result<()> {
        let s = snapshot(&input)?;
        let b = s
            .reader()
            .blocks()
            .iter()
            .enumerate()
            .find(|(b, _)| {
                record_type(s.reader(), *b).ok() == Some(b"IDProperty")
                    && s.reader()
                        .get(*b, 0, &[PathStep::Field(b"name".to_vec())], true)
                        .ok()
                        == Some(crate::value::Value::Bytes(b"version".to_vec()))
            })
            .unwrap()
            .0;
        let mut spec = json!({"schema":2,"writes":[],"bits":[],"idprop_strings":[{"operation":"idprop-string","block_index":b,"element":0,"path":["data","pointer"],"name_hex":hex(b"version"),"expected_hex":hex(b"5.3.0-alpha"),"replacement_hex":hex(b"5.3.0-beta")}]});
        match change {
            "old" => spec["idprop_strings"][0]["expected_hex"] = json!(hex(b"wrong")),
            "name" => spec["idprop_strings"][0]["name_hex"] = json!(hex(b"wrong")),
            "long" => {
                spec["idprop_strings"][0]["replacement_hex"] =
                    json!(hex(b"this string is too long"))
            }
            "nul" => spec["idprop_strings"][0]["replacement_hex"] = json!("610062"),
            "utf8" => spec["idprop_strings"][0]["replacement_hex"] = json!("ff"),
            "pointer" => spec["idprop_strings"][0]["path"] = json!(["data", "group"]),
            "conflict" => {
                let duplicate = spec["idprop_strings"][0].clone();
                spec["idprop_strings"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
            "noop" => spec["idprop_strings"][0]["replacement_hex"] = json!(hex(b"5.3.0-alpha")),
            "" | "source" => (),
            _ => panic!("unknown change"),
        }
        let p = crate::typed_spec::plan(&s, &spec, EditMode::FormatOnly)?;
        if change == "source" {
            std::fs::write(&input, b"changed").unwrap();
        }
        let report = write_plan(&s, &p, &output, &EditOptions::default())?;
        assert!(report.complete && report.output_written);
        let reread = BlendFile::open(&output)?;
        assert_eq!(
            reread.get(b, 0, &[PathStep::Field(b"totallen".to_vec())], false)?,
            crate::value::Value::Signed(12)
        );
        assert_eq!(
            reread.get(b, 0, &[PathStep::Field(b"len".to_vec())], false)?,
            crate::value::Value::Signed(if change == "noop" { 12 } else { 11 })
        );
        // Regression: logical len 11, totallen 12, allocation 12 remains
        // a resolvable string after shrinking; diff must retain its identity.
        let pointer_path = vec![
            PathStep::Field(b"data".to_vec()),
            PathStep::Field(b"pointer".to_vec()),
        ];
        let crate::value::Value::Pointer(address) = reread.get(b, 0, &pointer_path, true)? else {
            unreachable!()
        };
        let payload = reread
            .resolve_pointer(b, 0, &pointer_path, address)?
            .unwrap();
        assert_eq!(reread.block(payload)?.size, 12);
        assert!(reread.payload(payload)?.starts_with(if change == "noop" {
            b"5.3.0-alpha\0".as_slice()
        } else {
            b"5.3.0-beta\0".as_slice()
        }));
        if change == "noop" {
            assert_eq!(
                std::fs::read(&input).unwrap(),
                std::fs::read(&output).unwrap()
            );
        }
        Ok(())
    };
    let result = run();
    if result.is_err() {
        assert!(!output.exists());
    }
    std::fs::remove_dir_all(root).unwrap();
    result
}
#[test]
fn same_allocation_string_edits_are_guarded() {
    for version in [b"502", b"503"] {
        exercise(version, "", "").unwrap();
        exercise(version, "", "noop").unwrap();
    }
    for change in [
        "old", "name", "long", "nul", "utf8", "pointer", "conflict", "source",
    ] {
        assert!(exercise(b"503", "", change).is_err(), "{change}");
    }
    for mutation in [
        "type",
        "subtype",
        "len",
        "capacity",
        "alias",
        "opaque-alias",
        "cross-owner",
    ] {
        assert!(exercise(b"503", mutation, "").is_err(), "{mutation}");
    }
    assert!(exercise(b"504", "", "").is_err());
}
