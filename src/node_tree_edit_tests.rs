use super::*;
fn fixture() -> Vec<u8> {
    zstd::stream::decode_all(KIT).unwrap()
}
fn setup() -> (std::path::PathBuf, std::path::PathBuf, Value) {
    let dir = crate::test_scratch::scratch_dir(&std::env::temp_dir());
    let input = dir.join("input.blend");
    std::fs::write(&input, fixture()).unwrap();
    let inspected = inspect(&input).unwrap();
    let tree = &inspected["trees"][0];
    let spec = json!({"schema":1,"tree":tree["block_index"],"expected_graph":tree["graph"],"operations":[]});
    (dir, input, spec)
}
fn ep(node: &str, socket: &str, ty: &str) -> Value {
    json!({"node":node,"socket":socket,"socket_type":ty})
}
#[test]
fn supported_native_template_add_relink_remove_and_exact_unchanged_allocations() {
    let (dir, input, mut spec) = setup();
    let output = dir.join("new.blend");
    spec["operations"] = json!([
        {"op":"add_node","node_type":"NodeReroute","name":"Added"},
        {"op":"relink","from":ep("Source","Color","NodeSocketColor"),"to":ep("Temporary","Input","NodeSocketColor"),"replacement_from":ep("Other","Color","NodeSocketColor"),"replacement_to":ep("Added","Input","NodeSocketColor")},
        {"op":"remove_node","name":"Temporary","node_type":"NodeReroute"},
        {"op":"add_link","from":ep("Added","Output","NodeSocketColor"),"to":ep("Output","Socket_0","NodeSocketColor")},
        {"op":"add_node","node_type":"ShaderNodeMath","name":"New Math"},
        {"op":"add_node","node_type":"ShaderNodeValue","name":"New Value"},
        {"op":"add_node","node_type":"CompositorNodeRGB","name":"New RGB"}
    ]);
    let report = edit(&input, &output, &spec, &EditOptions::default()).unwrap();
    assert_eq!(report["complete"], true);
    assert_eq!(report["native_verified"], false);
    let before = BlendFile::open(&input).unwrap();
    let after = BlendFile::open(&output).unwrap();
    let changed = report["changed_original_blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.as_u64().unwrap() as usize)
        .collect::<BTreeSet<_>>();
    for (i, b) in before.blocks().iter().enumerate() {
        if !changed.contains(&i) {
            let j = after
                .blocks()
                .iter()
                .position(|a| a.code == b.code && a.old_address == b.old_address)
                .unwrap();
            assert_eq!(
                before.payload(i).unwrap(),
                after.payload(j).unwrap(),
                "original {i}"
            );
        }
    }
    assert_eq!(std::fs::read(&input).unwrap(), fixture());
    assert_eq!(
        edit(&input, &output, &spec, &EditOptions::default())
            .unwrap_err()
            .kind,
        "output_exists"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn required_typed_refusals_publish_nothing() {
    for (op, kind) in [
        (
            json!({"op":"add_node","node_type":"GeometryNodeJoinGeometry","name":"No"}),
            "node_constructor",
        ),
        (
            json!({"op":"add_link","from":ep("Source","Color","NodeSocketFloat"),"to":ep("Math","Value_001","NodeSocketFloat")}),
            "node_socket_type",
        ),
        (
            json!({"op":"add_link","from":ep("Absent","Color","NodeSocketColor"),"to":ep("Output","Socket_0","NodeSocketColor")}),
            "node_endpoint",
        ),
        (
            json!({"op":"add_link","from":ep("Source","Color","NodeSocketColor"),"to":ep("Temporary","Input","NodeSocketColor")}),
            "node_occupied",
        ),
        (
            json!({"op":"remove_link","from":ep("Other","Color","NodeSocketColor"),"to":ep("Output","Socket_0","NodeSocketColor")}),
            "node_endpoint",
        ),
    ] {
        let (dir, input, mut spec) = setup();
        spec["operations"] = json!([op]);
        let output = dir.join("refused.blend");
        assert_eq!(
            edit(&input, &output, &spec, &EditOptions::default())
                .unwrap_err()
                .kind,
            kind
        );
        assert!(!output.exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
    let (dir, input, mut spec) = setup();
    let output = dir.join("version.blend");
    let mut data = fixture();
    data[13..17].copy_from_slice(b"0503");
    std::fs::write(&input, data).unwrap();
    assert_eq!(
        edit(&input, &output, &spec, &EditOptions::default())
            .unwrap_err()
            .kind,
        "node_version"
    );
    assert!(!output.exists());
    std::fs::write(&input, fixture()).unwrap();
    spec["expected_graph"]["name"] = json!("Wrong");
    assert_eq!(
        edit(&input, &output, &spec, &EditOptions::default())
            .unwrap_err()
            .kind,
        "node_expected_graph"
    );
    assert!(!output.exists());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn unknown_build_and_cycles_are_refused() {
    let (dir, input, spec) = setup();
    let output = dir.join("bad.blend");
    let f = BlendFile::from_bytes(fixture()).unwrap();
    let glob = f.blocks().iter().position(|b| b.code == *b"GLOB").unwrap();
    let mut g = preserved(&f).unwrap();
    g.cstring(glob, &["build_hash"], "unverified").unwrap();
    let data = emit(&g, g.records.len(), spec["tree"].as_u64().unwrap() as usize).unwrap();
    std::fs::write(&input, data).unwrap();
    assert_eq!(
        edit(&input, &output, &spec, &EditOptions::default())
            .unwrap_err()
            .kind,
        "node_version"
    );
    assert!(!output.exists());
    std::fs::write(&input, fixture()).unwrap();
    let mut spec = spec;
    spec["operations"] = json!([{"op":"add_link","from":ep("Math","Value","NodeSocketFloat"),"to":ep("Math","Value_001","NodeSocketFloat")}]);
    assert_eq!(
        edit(&input, &output, &spec, &EditOptions::default())
            .unwrap_err()
            .kind,
        "node_cycle"
    );
    assert!(!output.exists());
    std::fs::remove_dir_all(dir).unwrap();
}
