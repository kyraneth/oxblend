use oxblend::value::PathStep;
use oxblend::{BlendFile, Result};
fn main() -> Result<()> {
    let f = BlendFile::open(std::env::args().nth(1).unwrap())?;
    for (i, b) in f.blocks().iter().enumerate() {
        let name =
            String::from_utf8_lossy(&f.schema().types[f.schema().structs[b.sdna_index].type_index]);
        if matches!(
            name.as_ref(),
            "Scene" | "bNodeTree" | "bNode" | "Library" | "Image" | "IDProperty" | "raw_data"
        ) && (std::env::args().nth(2).is_none() || ((6563..6840).contains(&i)))
        {
            let mut vals = Vec::new();
            for p in [
                vec!["id", "name"],
                vec!["name"],
                vec!["idname"],
                vec!["id"],
                vec!["storage"],
                vec!["owner_id"],
                vec!["type"],
                vec!["data", "val"],
                vec!["num_panel_states"],
                vec!["panel_states_array"],
            ] {
                let steps = p
                    .iter()
                    .map(|s| PathStep::Field(s.as_bytes().to_vec()))
                    .collect::<Vec<_>>();
                if let Ok(v) = f.get(i, 0, &steps, true) {
                    vals.push((p, v.json()));
                }
            }
            println!(
                "{i} {name} {:?} {}",
                b.code,
                serde_json::to_string(&vals).unwrap()
            );
        }
    }
    Ok(())
}
