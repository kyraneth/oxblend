//! Read-only native owner-address probe; large receiving fixtures stay external.
use oxblend::BlendFile;
use oxblend::value::{PathStep, Value, hex};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("input path required")?;
    let address = u64::from_str_radix(
        args.next()
            .ok_or("hex address required")?
            .trim_start_matches("0x"),
        16,
    )?;
    let f = BlendFile::open(&path)?;
    let mut owners = Vec::new();
    let mut pointer_sites = Vec::new();
    for (i, b) in f.blocks().iter().enumerate() {
        if b.code == *b"DATA" && b.old_address == address {
            owners.push(serde_json::json!({"block":i,"payload_offset":b.data_offset}));
        }
        if f.dna_type_id(i).is_ok_and(|t| t == b"NodesModifierData") {
            let p = [
                PathStep::Field(b"settings".to_vec()),
                PathStep::Field(b"properties".to_vec()),
            ];
            if matches!(f.get(i,0,&p,true)?,Value::Pointer(a) if a==address) {
                let target = f.get_pointer(i, 0, &p)?.ok_or("nonnull properties lost")?;
                assert_eq!(f.blocks()[target].old_address, address);
                pointer_sites.push(serde_json::json!({"source_block":i,"target":target,"target_payload_offset":f.blocks()[target].data_offset,"target_type_hex":hex(f.dna_type_id(target)?)}));
            }
        }
    }
    let mut id_properties_sites = Vec::new();
    for (i, b) in f.blocks().iter().enumerate() {
        if b.code[2..] != [0, 0] || b.code == *b"DATA" {
            continue;
        }
        let names = if f.dna_type_id(i).is_ok_and(|t| t == b"ID") {
            vec![b"properties".to_vec()]
        } else {
            vec![b"id".to_vec(), b"properties".to_vec()]
        };
        let p = names.into_iter().map(PathStep::Field).collect::<Vec<_>>();
        if matches!(f.get(i,0,&p,true),Ok(Value::Pointer(a)) if a==address) {
            let target = f
                .get_pointer(i, 0, &p)?
                .ok_or("nonnull ID properties lost")?;
            assert_eq!(f.blocks()[target].old_address, address);
            id_properties_sites.push(serde_json::json!({"source_block":i,"target":target,"target_payload_offset":f.blocks()[target].data_offset}));
        }
    }
    assert!(!owners.is_empty());
    assert!(!pointer_sites.is_empty());
    let raw = f
        .dereference(address)
        .expect_err("context-free API must preserve ambiguity");
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"path":path,"address":format!("{address:#x}"),"owners":owners,"settings_properties_pointer_sites":pointer_sites,"id_properties_pointer_sites":id_properties_sites,"context_free_error":raw.kind,"all_target_addresses_match":true})
        )?
    );
    Ok(())
}
