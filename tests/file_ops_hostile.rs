#[path = "support/scratch.rs"]
mod test_scratch;
use oxblend::{BlendFile, file_ops::pointer_census};
use std::path::PathBuf;
fn fixture(n: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/file-ops")
        .join(format!("{n}.blend"))
}
struct Scratch(PathBuf);
impl Scratch {
    fn new(_name: &str) -> Self {
        let p = test_scratch::scratch_dir(&std::env::temp_dir());
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn legal_cross_id_duplicate_remains_closed() {
    let source = fixture("duplicate-cross-ID-legal");
    let f = BlendFile::open(&source).unwrap();
    pointer_census(&f).unwrap().require_closed().unwrap();
}
#[test]
fn hostile_roles_refuse_without_address_only_acceptance() {
    for (name, kind, path) in [
        (
            "nodes-first-image",
            "pointer_target_type",
            "[0].nodes.first",
        ),
        (
            "nodes-first-fileglobal",
            "unresolved_persistent",
            "[0].nodes.first",
        ),
        ("nodes-cycle", "list_cycle", "[0].nodes.first"),
        ("nodes-bad-prev", "list_backlink", "[0].nodes.first"),
        (
            "link-null-fromsock",
            "required_pointer_null",
            "[0].fromsock",
        ),
        ("nodes-wrong-last", "list_boundary", "[0].nodes.first"),
        (
            "nodes-null-first-live-last",
            "list_boundary",
            "[0].nodes.first",
        ),
        (
            "socket-default-wrong",
            "pointer_target_type",
            "[0].default_value",
        ),
        ("unknown-void-nonnull", "unprofiled_role", "[0].id._pad1"),
        (
            "duplicate-same-ID-refused",
            "duplicate_data_address",
            "[0].nodes.first",
        ),
    ] {
        let f = BlendFile::open(fixture(name)).unwrap();
        match pointer_census(&f) {
            Err(e) => assert_eq!(e.kind, kind, "{name}"),
            Ok(c) => {
                assert!(c.require_closed().is_err(), "{name}");
                assert!(
                    c.sites.iter().any(|s| s.path == path && s.state == kind),
                    "{name}: {:?}",
                    c.sites
                )
            }
        }
    }
}
#[test]
fn reference_free_storage_read_and_pointer_hostile_are_distinct() {
    let source = fixture("unknown-storage-discriminator");
    let original = std::fs::read(&source).unwrap();
    let f = BlendFile::open(&source).unwrap();
    pointer_census(&f).unwrap().require_closed().unwrap();
    let mut site = None;
    for (i, b) in f.blocks().iter().enumerate() {
        if b.sdna_index == 0 {
            continue;
        }
        let st = &f.schema().structs[b.sdna_index];
        if f.schema().types[st.type_index] != b"bNode" {
            continue;
        }
        let id = st.fields.iter().find(|x| x.name == b"idname").unwrap();
        if !f.payload(i).unwrap()[id.offset..id.offset + id.size]
            .starts_with(b"FixtureUnprofiledNodeStorage\0")
        {
            continue;
        }
        let field = st.fields.iter().find(|x| x.name == b"storage").unwrap();
        let target = f
            .get_pointer(
                i,
                0,
                &[oxblend::value::PathStep::Field(b"storage".to_vec())],
            )
            .unwrap()
            .unwrap();
        let tb = f.block(target).unwrap();
        let ts = &f.schema().structs[tb.sdna_index];
        assert_eq!(f.schema().types[ts.type_index], b"bNodeSocketValueFloat");
        assert_eq!(tb.count, 1);
        assert_eq!(tb.size, f.schema().lengths[ts.type_index]);
        assert!(
            ts.fields
                .iter()
                .all(|x| !x.pointer && f.schema().types[x.type_index] != b"char")
        );
        site = Some((b.data_offset + field.offset, b.old_address));
    }
    let (offset, address) = site.unwrap();
    let scratch = Scratch::new("former-hostile");
    // Closest hostile variant: change ONLY the storage address to the SAME
    // owner's exact typed bNode DATA (which has pointers and meaningful arrays).
    let mut body = original.clone();
    assert_eq!(f.header().pointer_size, 8);
    assert!(f.header().little_endian);
    body[offset..offset + 8].copy_from_slice(&address.to_le_bytes());
    let hostile = scratch.0.join("unknown-storage-pointer.blend");
    std::fs::write(&hostile, &body).unwrap();
    let hf = BlendFile::open(&hostile).unwrap();
    let c = pointer_census(&hf).unwrap();
    assert!(c.require_closed().is_err());
    assert!(
        c.sites
            .iter()
            .any(|s| s.path == "[0].storage" && s.state == "unprofiled_role")
    );
    assert_eq!(std::fs::read(source).unwrap(), original);
}
#[test]
fn incomplete_diff_never_claims_equal() {
    let d = oxblend::diff::compare(&fixture("diff-orphan-left"), &fixture("diff-orphan-right"))
        .unwrap();
    assert_eq!(d["complete"], false);
    assert!(d["equal"].is_null());
    assert_eq!(
        d["difference_count"],
        d["differences"].as_array().unwrap().len()
    );
}
