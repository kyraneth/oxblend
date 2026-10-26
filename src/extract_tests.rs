use super::*;
use std::fs;

const SOURCE: &[u8] = include_bytes!("../tests/fixtures/extract/native-5.2.1.blend");
fn workspace() -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = crate::test_scratch::scratch_dir(&std::env::temp_dir());
    let src = dir.join("source.blend");
    fs::write(&src, SOURCE).unwrap();
    (src, dir.join("extracted.blend"))
}
fn options() -> ExtractOptions {
    ExtractOptions {
        ids: vec!["NTClosureProbe".into()],
        clear: BTreeSet::from(["CompositorNodeRLayers.scene".into()]),
        ..Default::default()
    }
}
#[test]
fn external_id_refuses_with_named_site_and_nothing_written() {
    let (src, out) = workspace();
    let mut opts = options();
    opts.clear.clear();
    let r = extract(&src, &out, &opts).unwrap();
    assert_eq!(r["complete"], false);
    assert!(!out.exists());
    assert!(
        r["refusals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["cause"] == "external_id"
                && r["site"]["struct_field"] == "bNode.id"
                && r["target_id"].as_str().unwrap().starts_with("SC"))
    );
    assert_eq!(fs::read(src).unwrap(), SOURCE);
}
#[test]
fn unknown_selection_and_existing_destination_refuse() {
    let (src, out) = workspace();
    let mut opts = options();
    opts.ids = vec!["NTMissing".into()];
    let r = extract(&src, &out, &opts).unwrap();
    assert_eq!(r["refusals"][0]["cause"], "selection");
    assert!(!out.exists());
    fs::write(&out, b"occupied").unwrap();
    assert_eq!(
        extract(&src, &out, &options()).unwrap_err().kind,
        "output_exists"
    );
    assert_eq!(fs::read(out).unwrap(), b"occupied");
}
#[test]
fn closure_preserves_header_dna_addresses_and_clears_scene() {
    let (src, out) = workspace();
    let r = extract(&src, &out, &options()).unwrap();
    assert_eq!(r["complete"], true, "{r}");
    assert!(!r["applied_policies"].as_array().unwrap().is_empty());
    let old = BlendFile::open(&src).unwrap();
    let new = BlendFile::open(&out).unwrap();
    assert_eq!(
        old.serialized_header().unwrap(),
        new.serialized_header().unwrap()
    );
    assert!(
        new.blocks()
            .iter()
            .all(|b| !matches!(&b.code, b"SC\0\0" | b"OB\0\0"))
    );
    for (i, b) in new.blocks().iter().enumerate() {
        let source = old
            .blocks()
            .iter()
            .enumerate()
            .find(|(_, q)| q.code == b.code && q.old_address == b.old_address)
            .unwrap()
            .0;
        if b.code == *b"DNA1" {
            assert_eq!(new.payload(i).unwrap(), old.payload(source).unwrap());
        }
    }
    assert_eq!(fs::read(src).unwrap(), SOURCE);
}
#[test]
fn compressed_input_and_block_selection() {
    let (src, out) = workspace();
    let f = BlendFile::open(&src).unwrap();
    let block = (0..f.blocks().len())
        .find(|&b| id(&f, b).as_deref() == Some("NTClosureProbe"))
        .unwrap();
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(SOURCE).unwrap();
    fs::write(&src, encoder.finish().unwrap()).unwrap();
    let mut opts = options();
    opts.ids.clear();
    opts.blocks.push(block);
    let r = extract(&src, &out, &opts).unwrap();
    assert_eq!(r["complete"], true, "{r}");
    assert!(fs::read(&out).unwrap().starts_with(b"BLENDER"));
}
#[test]
fn source_changed_after_staging_never_publishes() {
    let (src, out) = workspace();
    let e = extract_inner(&src, &out, &options(), || {
        fs::write(&src, b"changed").unwrap()
    })
    .unwrap_err();
    assert_eq!(e.kind, "input_changed");
    assert!(!out.exists());
    assert_eq!(fs::read_dir(out.parent().unwrap()).unwrap().count(), 1);
}
#[test]
fn known_void_id_reference_uses_main_namespace() {
    // Older source DNA declares Object.data as void*, unlike 5.2's ID*.
    let seed = BlendFile::from_bytes(
        include_bytes!("../tests/fixtures/file-ops/native-4.5.8/crypto-scene.blend").to_vec(),
    )
    .unwrap();
    let mut g = graph::Graph::new(&seed).unwrap();
    let object = g.new_typed("Object", None).unwrap();
    g.records[object].code = *b"OB\0\0";
    g.cstring(object, &["id", "name"], "OBCube").unwrap();
    g.set(object, &["type"], 1).unwrap();
    let mesh = g.new_typed("Mesh", None).unwrap();
    g.records[mesh].code = *b"ME\0\0";
    g.cstring(mesh, &["id", "name"], "MECube").unwrap();
    g.ptr(object, &["data"], Some(mesh)).unwrap();
    let f = BlendFile::from_bytes(g.decoded().unwrap()).unwrap();
    let object = (0..f.blocks().len())
        .find(|&b| id(&f, b).as_deref() == Some("OBCube"))
        .unwrap();
    let mesh = (0..f.blocks().len())
        .find(|&b| id(&f, b).as_deref() == Some("MECube"))
        .unwrap();
    let site = pointer_layout(&f, object)
        .unwrap()
        .unwrap()
        .into_iter()
        .find(|p| p.parent_type == b"Object" && p.field_name == b"data")
        .unwrap();
    assert_eq!(site.declared_type, b"void");
    let address = graph::uint(&f, object, &["data"]).unwrap();
    let roots = BTreeSet::from([object, mesh]);
    let target = resolve(&f, object, &site, address).unwrap().unwrap();
    assert_eq!(target, mesh);
    assert!(roots.contains(&target));
    assert!(!cross_owner(&f, object, target, &roots).unwrap());
}
#[test]
fn cross_owner_data_requires_its_owner_selected() {
    let f = BlendFile::from_bytes(SOURCE.to_vec()).unwrap();
    let roots = (0..f.blocks().len())
        .filter(|&b| id(&f, b).is_some() && f.blocks()[b].code != *b"DATA")
        .collect::<Vec<_>>();
    let a = roots[0];
    let other = roots
        .iter()
        .copied()
        .find(|&b| {
            b != a
                && f.blocks().iter().enumerate().any(|(i, block)| {
                    block.code == *b"DATA" && f.owner_index().unwrap().owner(&f, i) == Some(b)
                })
        })
        .unwrap();
    let target = f
        .blocks()
        .iter()
        .enumerate()
        .find(|(i, block)| {
            block.code == *b"DATA" && f.owner_index().unwrap().owner(&f, *i) == Some(other)
        })
        .unwrap()
        .0;
    assert!(cross_owner(&f, a, target, &BTreeSet::from([a])).unwrap());
    assert!(!cross_owner(&f, a, target, &BTreeSet::from([a, other])).unwrap());
}
#[test]
fn rlayer_alias_is_record_aware() {
    let f = BlendFile::from_bytes(SOURCE.to_vec()).unwrap();
    let node = (0..f.blocks().len())
        .find(|&b| graph::text(&f, b, &["idname"]).ok().as_deref() == Some("CompositorNodeRLayers"))
        .unwrap();
    let blur = (0..f.blocks().len())
        .find(|&b| graph::text(&f, b, &["idname"]).ok().as_deref() == Some("CompositorNodeBlur"))
        .unwrap();
    let block = f.block(node).unwrap();
    let mut bytes = SOURCE.to_vec();
    bytes.splice(
        block.data_offset + block.size..block.data_offset + block.size,
        f.payload(blur).unwrap().iter().copied(),
    );
    bytes[block.data_offset - 16..block.data_offset - 8]
        .copy_from_slice(&((block.size * 2) as u64).to_le_bytes());
    bytes[block.data_offset - 8..block.data_offset].copy_from_slice(&2u64.to_le_bytes());
    let f = BlendFile::from_bytes(bytes).unwrap();
    let sites = pointer_layout(&f, node)
        .unwrap()
        .unwrap()
        .into_iter()
        .filter(|p| p.field_name == b"id")
        .collect::<Vec<_>>();
    let rules = BTreeSet::from(["CompositorNodeRLayers.scene".into()]);
    assert!(policy(&f, node, &sites[0], &rules).unwrap());
    assert!(!policy(&f, node, &sites[1], &rules).unwrap());
}
#[test]
fn byte_id_names_remain_selectable_by_block() {
    let (src, out) = workspace();
    let f = BlendFile::from_bytes(SOURCE.to_vec()).unwrap();
    let block = (0..f.blocks().len())
        .find(|&b| id(&f, b).as_deref() == Some("NTClosureProbe"))
        .unwrap();
    let steps = ["id", "name"].map(|s| crate::value::PathStep::Field(s.as_bytes().to_vec()));
    let field = f.locate_field(block, 0, &steps).unwrap();
    let offset = f.blocks()[block].data_offset + field.offset;
    let mut bytes = SOURCE.to_vec();
    bytes[offset + 2] = 0xff;
    fs::write(&src, bytes).unwrap();
    let mut opts = options();
    opts.ids.clear();
    opts.blocks.push(block);
    let r = extract(&src, &out, &opts).unwrap();
    assert_eq!(r["complete"], true, "{r}");
    assert!(r["selected_ids"][0]["name"].is_null());
    assert!(
        r["selected_ids"][0]["name_hex"]
            .as_str()
            .unwrap()
            .starts_with("4e54ff")
    );
}
#[test]
fn unused_policy_is_reported() {
    let (src, out) = workspace();
    let mut opts = options();
    opts.clear.insert("Object.mat".into());
    let r = extract(&src, &out, &opts).unwrap();
    assert_eq!(r["complete"], true);
    assert!(
        r["unused_clear_rules"]
            .as_array()
            .unwrap()
            .contains(&json!("Object.mat"))
    );
}
#[test]
fn runtime_looking_nonempty_interface_is_not_discarded() {
    let (src, out) = workspace();
    let f = BlendFile::open(&src).unwrap();
    let b = (0..f.blocks().len())
        .find(|&b| id(&f, b).as_deref() == Some("NTClosureProbe"))
        .unwrap();
    let steps = ["tree_interface", "root_panel", "items_num"]
        .map(|s| crate::value::PathStep::Field(s.as_bytes().to_vec()));
    let loc = f.locate_field(b, 0, &steps).unwrap();
    let offset = f.blocks()[b].data_offset + loc.offset;
    let mut bytes = SOURCE.to_vec();
    bytes[offset..offset + 4].copy_from_slice(&1u32.to_le_bytes());
    fs::write(&src, bytes).unwrap();
    let r = extract(&src, &out, &options()).unwrap();
    assert_eq!(r["complete"], false);
    assert!(!out.exists());
    assert!(
        r["refusals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["site"]["struct_field"] == "bNodeTreeInterfacePanel.items_array")
    );
}
