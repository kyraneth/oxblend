use crate::{
    BlendFile,
    file_ops::{checked_pointer_census as pointer_census, graph::Graph},
};
use std::path::PathBuf;
fn fixture(n: &str) -> BlendFile {
    BlendFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/file-ops")
            .join(format!("{n}.blend")),
    )
    .unwrap()
}
fn roots(f: &BlendFile) -> Vec<usize> {
    f.blocks()
        .iter()
        .enumerate()
        .filter(|(_, b)| b.code != *b"DATA" && b.code != *b"DNA1" && b.code != *b"REND")
        .map(|(i, _)| i)
        .collect()
}
#[test]
fn compositor_rlayers_releases_scene_before_closure_not_in_generic_import() {
    let native = fixture("native-4.5.8/crypto-scene");
    let mut source = Graph::new(&native).unwrap();
    let scene = source.new_typed("Scene", None).unwrap();
    source.records[scene].code = *b"SC\0\0";
    source.cstring(scene, &["id", "name"], "SCSource").unwrap();
    let tree = source.new_typed("bNodeTree", Some(scene)).unwrap();
    source.cstring(tree, &["id", "name"], "NTSource").unwrap();
    source.ptr(scene, &["nodetree"], Some(tree)).unwrap();
    let node = source.new_typed("bNode", Some(scene)).unwrap();
    source
        .cstring(node, &["idname"], "CompositorNodeRLayers")
        .unwrap();
    source.ptr(node, &["id"], Some(scene)).unwrap();
    source.set_list(tree, &["nodes"], &[node]).unwrap();
    let saved = BlendFile::from_bytes(source.decoded().unwrap()).unwrap();
    let bytes = saved.read_decompressed_bytes().unwrap();
    let scene = saved
        .blocks()
        .iter()
        .position(|b| b.code == *b"SC\0\0")
        .unwrap();
    let tree = saved
        .blocks()
        .iter()
        .enumerate()
        .find(|(i, _)| saved.dna_type_id(*i).ok() == Some(b"bNodeTree".as_slice()))
        .unwrap()
        .0;
    let node = crate::file_ops::source_list(&saved, tree, &["nodes"]).unwrap()[0];
    let mut ordinary = Graph::new(&saved).unwrap();
    let map = ordinary.import(&saved, &[tree], None, false).unwrap();
    assert!(map.contains_key(&scene));
    assert_eq!(
        ordinary.target(map[&node], &["id"]).unwrap(),
        Some(map[&scene])
    );
    let plan = crate::file_ops::graph::CompositorRetarget {
        nodes: Default::default(),
        clear_rlayers: true,
        source_scene: scene,
        image: None,
    };
    let mut comp = Graph::new(&saved).unwrap();
    let destination = comp.new_typed("Scene", None).unwrap();
    comp.records[destination].code = *b"SC\0\0";
    comp.cstring(destination, &["id", "name"], "SCComp")
        .unwrap();
    let map = comp
        .import_with_compositor(&saved, &[tree], Some(destination), false, Some(&plan))
        .unwrap();
    assert!(!map.contains_key(&scene));
    assert_eq!(comp.target(map[&node], &["id"]).unwrap(), None);
    comp.verify().unwrap();
    assert_eq!(saved.read_decompressed_bytes().unwrap(), bytes);
}
#[test]
fn closure_import_equals_full_census_restricted_to_closure() {
    for name in [
        "synthetic-control",
        "duplicate-cross-ID-legal",
        "synthetic-empty-seed",
    ] {
        let f = fixture(name);
        let census = pointer_census(&f).unwrap();
        census.require_closed().unwrap();
        let mut g = Graph::new(&f).unwrap();
        let map = g.import(&f, &roots(&f), None, false).unwrap();
        for site in census.sites.iter().filter(|s| map.contains_key(&s.block)) {
            let data = &g.records[map[&site.block]].data;
            let address = crate::checked::number(
                data,
                site.offset,
                f.header().pointer_size,
                f.header().little_endian,
            )
            .unwrap();
            let expected = site
                .target
                .map_or(0, |(target, _)| g.records[map[&target]].address);
            assert_eq!(address, expected, "{name} {}", site.path);
        }
        g.verify().unwrap();
    }
}
#[test]
fn real_graph_mutations_preserve_verified_native_lists() {
    let f = fixture("synthetic-control");
    let mut g = Graph::new(&f).unwrap();
    g.import(&f, &roots(&f), None, false).unwrap();
    let tree = g
        .records
        .iter()
        .enumerate()
        .find(|(i, _)| g.ty(*i) == b"bNodeTree")
        .map(|(i, _)| i)
        .unwrap();
    let nodes = g.list(tree, &["nodes"]).unwrap();
    let output = g.list(nodes[0], &["outputs"]).unwrap()[0];
    let input = g.list(nodes[1], &["inputs"]).unwrap()[0];
    let link = g.list(tree, &["links"]).unwrap()[0];
    g.link_remove(tree, link).unwrap();
    g.verify().unwrap();
    g.link_new(tree, nodes[0], output, nodes[1], input).unwrap();
    g.verify().unwrap();
    let added = g.node_new(tree, &f, "NodeReroute", "Added").unwrap();
    g.verify().unwrap();
    let socket = g
        .socket_new(added, &f, "NodeSocketFloat", "Extra", 0)
        .unwrap();
    g.verify().unwrap();
    g.socket_remove(tree, added, socket, "outputs").unwrap();
    g.verify().unwrap();
    g.node_remove(tree, added).unwrap();
    g.verify().unwrap();
    assert_eq!(g.list(tree, &["nodes"]).unwrap(), nodes);
}

#[test]
fn diff_compares_owner_id_in_two_distinct_published_files() {
    let f = fixture("synthetic-control");
    let mut g = Graph::new(&f).unwrap();
    g.import(&f, &roots(&f), None, false).unwrap();
    let scene = g.records.iter().position(|r| r.code == *b"SC\0\0").unwrap();
    let tree = g.target(scene, &["nodetree"]).unwrap().unwrap();
    let dir = crate::test_scratch::scratch_dir(&std::env::temp_dir());
    let a = dir.join("a.blend");
    let b = dir.join("b.blend");
    std::fs::write(&a, g.decoded().unwrap()).unwrap();
    g.ptr(tree, &["owner_id"], Some(scene)).unwrap();
    std::fs::write(&b, g.decoded().unwrap()).unwrap();
    let d = crate::diff::compare(&a, &b).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(d["complete"], true);
    assert_eq!(d["equal"], false);
    assert!(d.to_string().contains("owner_id"), "{d}");
}

#[test]
fn crypto_retarget_detaches_only_planned_scene_edge() {
    for case in ["positive", "wrong_id", "other_edge"] {
        let control = fixture("synthetic-control");
        let mut source = Graph::new(&control).unwrap();
        source
            .import(&control, &roots(&control), None, false)
            .unwrap();
        let scene = source
            .records
            .iter()
            .position(|r| r.code == *b"SC\0\0")
            .unwrap();
        let tree = source.target(scene, &["nodetree"]).unwrap().unwrap();
        let nodes = source.list(tree, &["nodes"]).unwrap();
        let image = source
            .records
            .iter()
            .position(|r| r.code == *b"IM\0\0")
            .unwrap();
        source
            .cstring(nodes[0], &["idname"], "CompositorNodeCryptomatteV2")
            .unwrap();
        let storage = source.new_typed("NodeCryptomatte", Some(scene)).unwrap();
        source.ptr(nodes[0], &["storage"], Some(storage)).unwrap();
        source
            .ptr(
                nodes[0],
                &["id"],
                Some(if case == "wrong_id" { image } else { scene }),
            )
            .unwrap();
        if case == "other_edge" {
            source.ptr(nodes[1], &["id"], Some(scene)).unwrap();
        }
        let tech = BlendFile::from_bytes(source.decoded().unwrap()).unwrap();
        let source_scene = tech
            .blocks()
            .iter()
            .position(|r| r.code == *b"SC\0\0")
            .unwrap();
        let tree_source = crate::file_ops::graph::uint(&tech, source_scene, &["nodetree"]).unwrap();
        let tree_source = tech
            .blocks()
            .iter()
            .position(|r| r.old_address == tree_source)
            .unwrap();
        let node = tech
            .blocks()
            .iter()
            .enumerate()
            .find(|(i, _)| {
                crate::file_ops::graph::text(&tech, *i, &["idname"])
                    .ok()
                    .as_deref()
                    == Some("CompositorNodeCryptomatteV2")
            })
            .unwrap()
            .0;
        let mut dest = Graph::new(&tech).unwrap();
        let comp = dest.new_typed("Scene", None).unwrap();
        dest.records[comp].code = *b"SC\0\0";
        dest.cstring(comp, &["id", "name"], "SCComp").unwrap();
        let target_image = dest.new_typed("Image", None).unwrap();
        dest.records[target_image].code = *b"IM\0\0";
        dest.cstring(target_image, &["id", "name"], "IMcryptos")
            .unwrap();
        let plan = crate::file_ops::graph::CompositorRetarget {
            nodes: [node].into_iter().collect(),
            clear_rlayers: false,
            source_scene,
            image: Some(target_image),
        };
        let result =
            dest.import_with_compositor(&tech, &[tree_source], Some(comp), false, Some(&plan));
        if case != "positive" {
            assert_eq!(
                result.unwrap_err().kind,
                if case == "wrong_id" {
                    "crypto_source_id"
                } else {
                    "crypto_scene_kept_edge"
                }
            );
            continue;
        }
        let imported = result.unwrap();
        assert!(!imported.contains_key(&source_scene));
        assert_eq!(
            dest.target(imported[&node], &["id"]).unwrap(),
            Some(target_image)
        );
        assert_eq!(dest.value(imported[&node], &["custom1"]).unwrap(), 1);
        dest.verify().unwrap();
    }
}

#[test]
fn linked_id_proxy_and_preview405_contracts_close() {
    let f = fixture("synthetic-empty-seed");
    let mut g = Graph::new(&f).unwrap();
    g.import(&f, &roots(&f), None, false).unwrap();
    let scene = g.records.iter().position(|r| r.code == *b"SC\0\0").unwrap();
    let lib = g.new_typed("Library", None).unwrap();
    g.records[lib].code = *b"LI\0\0";
    g.cstring(lib, &["id", "name"], "LIfixture").unwrap();
    let proxy = g.new_typed("ID", None).unwrap();
    g.records[proxy].code = *b"ID\0\0";
    g.cstring(proxy, &["name"], "OBfixtureCamera").unwrap();
    g.ptr(proxy, &["lib"], Some(lib)).unwrap();
    g.ptr(scene, &["camera"], Some(proxy)).unwrap();
    g.verify().unwrap();
    // Exercise native ID-header qualification directly, independently of the
    // redundant typed-role proxy check. Removing owner.rs's lib!=0 condition
    // must turn this exact missing-library negative into Ok and kill the test.
    let mut missing_library = g.decoded().unwrap();
    let valid = BlendFile::from_bytes(missing_library.clone()).unwrap();
    let header = valid
        .blocks()
        .iter()
        .find(|b| b.code == *b"ID\0\0")
        .unwrap();
    let (offset, size) = g.field(proxy, &["lib"]).unwrap();
    missing_library[header.data_offset + offset..header.data_offset + offset + size].fill(0);
    let invalid = BlendFile::from_bytes(missing_library).unwrap();
    // Reading ownership is permissive; write/census admission retains the native
    // typed placeholder contract. This is not an ID-vs-DATA lookup fallback.
    let index = invalid.owner_index().unwrap();
    let e = index.admit_write(&invalid).unwrap_err();
    assert_eq!(e.kind, "owner_header");
    assert_eq!(
        e.message,
        "bare ID placeholder lacks qualified kind/library"
    );
    g.ptr(proxy, &["lib"], None).unwrap();
    assert_eq!(g.verify().unwrap_err().kind, "owner_header");
    g.ptr(proxy, &["lib"], Some(lib)).unwrap();
    let preview = g.new_typed("PreviewImage", Some(scene)).unwrap();
    g.ptr(scene, &["preview"], Some(preview)).unwrap();
    let (offset, size) = g.field(preview, &["w"]).unwrap();
    g.records[preview].data[offset..offset + size / 2].copy_from_slice(&1u32.to_le_bytes());
    let (offset, size) = g.field(preview, &["h"]).unwrap();
    g.records[preview].data[offset..offset + size / 2].copy_from_slice(&1u32.to_le_bytes());
    let raw = g
        .allocate(*b"DATA", 0, 1, vec![255; 4], Some(scene))
        .unwrap();
    let (offset, _) = g.field(preview, &["rect"]).unwrap();
    let address = g.records[raw].address.to_le_bytes();
    g.records[preview].data[offset..offset + 8].copy_from_slice(&address);
    g.verify().unwrap();
    g.records[raw].data.push(0);
    assert_eq!(g.verify().unwrap_err().kind, "pointer_extent");
}

#[test]
fn physical_origin_of_native_embedded_id_uses_data_namespace() {
    let f = BlendFile::from_bytes(
        include_bytes!("../tests/fixtures/file-ops/native-4.5.8/crypto-image.blend").to_vec(),
    )
    .unwrap();
    let scene = f
        .blocks()
        .iter()
        .position(|b| b.code == *b"SC\0\0")
        .unwrap();
    let tree = f
        .get_pointer(
            scene,
            0,
            &[crate::value::PathStep::Field(b"nodetree".to_vec())],
        )
        .unwrap()
        .unwrap();
    assert_eq!(f.blocks()[tree].code, *b"DATA");
    assert_eq!(f.dna_type_id(tree).unwrap(), b"bNodeTree");
    assert_eq!(
        f.owner_index()
            .unwrap()
            .resolve_origin(&f, tree, f.blocks()[tree].old_address)
            .unwrap(),
        tree
    );
}
#[test]
fn owner_projection_detects_address_alias_hidden_by_closed_census() {
    let f = fixture("synthetic-empty-seed");
    let mut g = Graph::new(&f).unwrap();
    g.import(&f, &roots(&f), None, false).unwrap();
    let scene = g.records.iter().position(|r| r.code == *b"SC\0\0").unwrap();
    let sdna = f
        .schema()
        .structs
        .iter()
        .position(|s| f.schema().types[s.type_index] == b"LinkData")
        .unwrap();
    // Native ID and local DATA maps are separate. A zero-count DATA allocation
    // aliases its ID address: closed census alone cannot detect the projection
    // ambiguity. This is deliberately invalid transaction state, not a native
    // source rejection. Removing the projection loop must make this test fail.
    g.records.push(crate::file_ops::graph::Record {
        code: *b"DATA",
        sdna,
        count: 0,
        data: Vec::new(),
        address: g.records[scene].address,
        owner: scene,
        pointers: Vec::new(),
        alive: true,
    });
    assert_eq!(g.verify().unwrap_err().kind, "owner_projection");
}

#[test]
fn walk_cycle_guard_is_independent_of_schema_parser() {
    let mut f = fixture("synthetic-empty-seed");
    let si = f
        .schema
        .structs
        .iter()
        .position(|s| f.schema.types[s.type_index] == b"ListBase")
        .unwrap();
    let ti = f.schema.structs[si].type_index;
    let field = &mut f.schema.structs[si].fields[0];
    field.pointer = false;
    field.type_index = ti;
    // If the recursion guard is removed, the second iteration deterministically
    // overflows rather than hanging or exhausting memory. No runtime cap.
    field.offset = usize::MAX;
    let e = crate::file_ops::walk(&f, si, 0, "cycle", &mut Vec::new()).unwrap_err();
    assert_eq!(e.kind, "invalid_sdna");
    assert_eq!(e.message, "recursive by-value pointer layout");
}

#[test]
fn schema_zero_extent_cycle_must_be_rejected_before_walk() {
    let mut dna = b"SDNANAME".to_vec();
    dna.extend_from_slice(&1u32.to_le_bytes());
    dna.extend_from_slice(b"self\0");
    while !dna.len().is_multiple_of(4) {
        dna.push(0);
    }
    dna.extend_from_slice(b"TYPE");
    dna.extend_from_slice(&1u32.to_le_bytes());
    dna.extend_from_slice(b"Self\0");
    while !dna.len().is_multiple_of(4) {
        dna.push(0);
    }
    dna.extend_from_slice(b"TLEN");
    dna.extend_from_slice(&0u16.to_le_bytes());
    while !dna.len().is_multiple_of(4) {
        dna.push(0);
    }
    dna.extend_from_slice(b"STRC");
    dna.extend_from_slice(&1u32.to_le_bytes());
    for n in [0u16, 1, 0, 0] {
        dna.extend_from_slice(&n.to_le_bytes());
    }
    let e = crate::schema::Schema::parse(&dna, true, 8).unwrap_err();
    assert_eq!(e.kind, "invalid_sdna");
    assert_eq!(e.message, "recursive by-value struct containment");
}
