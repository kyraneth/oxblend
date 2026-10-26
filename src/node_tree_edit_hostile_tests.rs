use super::*;
#[test]
fn malformed_saved_link_and_late_failure_never_publish() {
    let mut bytes = Vec::new();
    bytes.extend(zstd::stream::decode_all(KIT).unwrap());
    let f = BlendFile::from_bytes(bytes.clone()).unwrap();
    let mut g = preserved(&f).unwrap();
    let tree = g.records.iter().position(|r| r.code == *b"NT\0\0").unwrap();
    let before = describe(&g, tree).unwrap();
    let l = g.list(tree, &["links"]).unwrap()[0];
    g.set(l, &["fromsock"], 1).unwrap();
    let dir = crate::test_scratch::scratch_dir(&std::env::temp_dir());
    let input = dir.join("bad.blend");
    let output = dir.join("output.blend");
    std::fs::write(&input, emit(&g, g.records.len(), tree).unwrap()).unwrap();
    let spec = json!({"schema":1,"tree":tree,"expected_graph":before,"operations":[]});
    assert_eq!(
        edit(&input, &output, &spec, &EditOptions::default())
            .unwrap_err()
            .kind,
        "dangling_pointer"
    );
    assert!(!output.exists());
    std::fs::write(&input, &bytes).unwrap();
    let mut spec = spec;
    spec["operations"] = json!([
        {"op":"add_node","node_type":"NodeReroute","name":"Would Be Added"},
        {"op":"add_node","node_type":"UndefinedNode","name":"Refuses Whole Transaction"}
    ]);
    assert_eq!(
        edit(&input, &output, &spec, &EditOptions::default())
            .unwrap_err()
            .kind,
        "node_constructor"
    );
    assert!(!output.exists());
    assert_eq!(std::fs::read(&input).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn add_then_remove_discloses_retained_islands_and_counts_emitted_blocks() {
    let bytes = zstd::stream::decode_all(KIT).unwrap();
    let f = BlendFile::from_bytes(bytes.clone()).unwrap();
    let g = preserved(&f).unwrap();
    let tree = g.records.iter().position(|r| r.code == *b"NT\0\0").unwrap();
    let before = describe(&g, tree).unwrap();
    let dir = crate::test_scratch::scratch_dir(&std::env::temp_dir());
    let input = dir.join("input.blend");
    let output = dir.join("output.blend");
    std::fs::write(&input, &bytes).unwrap();
    let spec = json!({"schema":1,"tree":tree,"expected_graph":before,"operations":[
        {"op":"add_node","node_type":"ShaderNodeMath","name":"Removed Within Transaction"},
        {"op":"remove_node","node_type":"ShaderNodeMath","name":"Removed Within Transaction"}
    ]});
    let report = edit(&input, &output, &spec, &EditOptions::default()).unwrap();
    let out = BlendFile::open(&output).unwrap();
    let emitted = out
        .blocks()
        .iter()
        .filter(|a| !f.blocks().iter().any(|b| a.old_address == b.old_address))
        .count();
    assert_eq!(report["added_blocks"].as_u64().unwrap() as usize, emitted);
    assert_eq!(emitted, 8); // Native Math prototype: four sockets and their four defaults.
    assert_eq!(
        report["removed_node_retention"][0]["retained_allocation_roots"]
            .as_array()
            .unwrap()
            .len(),
        emitted
    );
    assert_eq!(report["after"], before);
    assert_eq!(std::fs::read(&input).unwrap(), bytes);
    std::fs::remove_dir_all(dir).unwrap();
}
