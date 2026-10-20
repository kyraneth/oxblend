//! Narrow native-null compatibility for absent ID references in copied libraries.
//! No structural/list/DATA pointer, ambiguous target or present invalid ID is repaired.
use crate::value::{PathStep, Value};
use crate::{BlendFile, Result};

#[derive(Debug, Clone)]
pub(crate) struct PointerClear {
    pub block: usize,
    pub offset: usize,
    pub address: u64,
    pub steps: Vec<PathStep>,
    native_action: &'static str,
}
impl PointerClear {
    pub fn native_action(&self) -> &'static str {
        self.native_action
    }
}
fn native_id_action(f: &BlendFile, owner: usize) -> Result<&'static str> {
    let path = steps(&["id", "override_library"]);
    match f.locate_field(owner, 0, &path) {
        Err(e) if e.kind == "missing_field" => return Ok("clear_to_null"),
        Err(e) => return Err(e),
        Ok(_) => (),
    }
    if let Some(definition) = crate::file_ops::source_target(f, owner, &["id", "override_library"])?
        && let Some(reference) = crate::file_ops::source_target(f, definition, &["reference"])?
    {
        crate::file_ops::qualify_read_id(f, reference)?;
        return Ok("reapply_library_override");
    }
    Ok("clear_to_null")
}
fn steps(names: &[&str]) -> Vec<PathStep> {
    names
        .iter()
        .map(|n| PathStep::Field(n.as_bytes().to_vec()))
        .collect()
}
fn integer(f: &BlendFile, block: usize, path: &[PathStep]) -> Result<Option<i64>> {
    Ok(match f.get(block, 0, path, true)? {
        Value::Signed(n) => Some(n),
        Value::Unsigned(n) => i64::try_from(n).ok(),
        _ => None,
    })
}
fn one_record(f: &BlendFile, block: usize) -> Result<bool> {
    let b = f.block(block)?;
    let si = f.struct_index(block)?;
    Ok(b.count == 1 && b.size == f.schema().lengths[f.schema().structs[si].type_index])
}
fn sites(f: &BlendFile, block: usize) -> Result<Vec<(Vec<PathStep>, &'static [u8])>> {
    Ok(match f.dna_type_id(block)? {
        b"NodesModifierData" => {
            let path = steps(&["node_group"]);
            match f.locate_field(block, 0, &path) {
                Ok(_) => vec![(path, b"bNodeTree")],
                Err(e) if e.kind == "missing_field" => Vec::new(),
                Err(e) => return Err(e),
            }
        }
        b"bTransLikeConstraint" => vec![(steps(&["tar"]), b"Object")],
        b"DriverVar" => {
            let targets = match f.locate_field(block, 0, &steps(&["targets"])) {
                Ok(targets) => targets,
                Err(e) if e.kind == "missing_field" => return Ok(Vec::new()),
                Err(e) => return Err(e),
            };
            if targets.field.pointer
                || f.schema().types[targets.field.type_index] != b"DriverTarget"
            {
                return Ok(Vec::new());
            }
            (0..targets.field.array_len)
                .map(|i| {
                    (
                        vec![
                            PathStep::Field(b"targets".to_vec()),
                            PathStep::Index(i),
                            PathStep::Field(b"id".to_vec()),
                        ],
                        b"ID".as_slice(),
                    )
                })
                .collect()
        }
        _ => Vec::new(),
    })
}
/// The generic SDK lookup is namespace-only for some ID fields. Known
/// compatibility slots must still reject a present target of the wrong family.
pub(crate) fn validate_present(
    f: &BlendFile,
    block: usize,
    offset: usize,
    target: usize,
) -> Result<()> {
    for (path, _) in sites(f, block)? {
        let loc = f.locate_field(block, 0, &path)?;
        if loc.offset != offset {
            continue;
        }
        if !one_record(f, block)? {
            return Err(crate::Error::new(
                "pointer_extent",
                "compatibility source differs from source SDNA extent",
            ));
        }
        crate::file_ops::qualify_read_id(f, target)?;
        let layout = crate::file_ops::PointerLayout {
            offset,
            path: crate::value::display_path(&path),
            declared_type: f.schema().types[loc.field.type_index].clone(),
            parent_type: loc.parent_type.to_vec(),
            parent_offset: loc.parent_offset,
            field_name: loc.field.name.clone(),
            pointer_depth: loc.field.full_name.iter().filter(|&&c| c == b'*').count(),
            list_role: loc.list_role.map(|(a, b)| (a.to_vec(), b.to_vec())),
        };
        crate::file_ops::roles::qualify(f, block, &layout, target)?;
        break;
    }
    Ok(())
}
/// Prove an observed nullable ID role and its saved caller before treating an
/// absent lib-map entry as NULL, as native Blender does. Context (linked versus
/// main file) is deliberately the caller's responsibility.
pub(crate) fn missing_id(
    f: &BlendFile,
    block: usize,
    offset: usize,
) -> Result<Option<PointerClear>> {
    let b = f.block(block)?;
    if b.code != *b"DATA" || !one_record(f, block)? {
        return Ok(None);
    }
    let mut site = None;
    for (path, expected) in sites(f, block)? {
        let loc = f.locate_field(block, 0, &path)?;
        if loc.offset == offset
            && loc.field.pointer
            && loc.field.array_len == 1
            && loc.field.full_name.iter().filter(|&&c| c == b'*').count() == 1
            && f.schema().types[loc.field.type_index] == expected
        {
            site = Some(path);
            break;
        }
    }
    let Some(path) = site else { return Ok(None) };
    let Value::Pointer(address) = f.get(block, 0, &path, true)? else {
        return Ok(None);
    };
    if address == 0 {
        return Ok(None);
    }
    match f.get_pointer(block, 0, &path) {
        Err(e) if e.kind == "dangling_pointer" => (),
        Err(e) => return Err(e),
        Ok(Some(target)) => {
            validate_present(f, block, offset, target)?;
            return Ok(None);
        }
        Ok(None) => return Ok(None),
    }
    let Some(owner) = f.owner_index()?.owner(block) else {
        return Ok(None);
    };
    crate::file_ops::qualify_read_id(f, owner)?;
    match f.dna_type_id(block)? {
        b"NodesModifierData" => {
            if f.dna_type_id(owner)? != b"Object"
                || f.header().version != 301
                || integer(f, block, &steps(&["modifier", "type"]))? != Some(57)
                || !matches!(f.get(block, 0, &steps(&["modifier", "name"]), true)?, Value::Bytes(name) if name == b"Realize Instances 2.93 Legacy")
            {
                return Ok(None);
            }
        }
        b"bTransLikeConstraint" => {
            if f.dna_type_id(owner)? != b"Object" {
                return Ok(None);
            }
            let mut callers = 0;
            for i in owner + 1..f.blocks().len() {
                if f.blocks()[i].code != *b"DATA" {
                    break;
                }
                if f.dna_type_id(i)? == b"bConstraint"
                    && one_record(f, i)?
                    && integer(f, i, &steps(&["type"]))? == Some(23)
                    && matches!(f.get(i, 0, &steps(&["data"]), true)?, Value::Pointer(a) if a == b.old_address)
                    && f.get_pointer(i, 0, &steps(&["data"]))? == Some(block)
                {
                    callers += 1;
                }
            }
            if callers != 1 {
                return Ok(None);
            }
        }
        b"DriverVar" => {
            let PathStep::Index(index) = path[1] else {
                return Ok(None);
            };
            let count = integer(f, block, &steps(&["num_targets"]))?;
            let capacity = f
                .locate_field(block, 0, &steps(&["targets"]))?
                .field
                .array_len;
            if !count.is_some_and(|n| n > index as i64 && n <= capacity as i64) {
                return Ok(None);
            }
            let mut kind = path.clone();
            kind[2] = PathStep::Field(b"idtype".to_vec());
            // Native ID_OB: the observed missing target is an Object, not an
            // arbitrary void handle. Other driver-ID families remain strict.
            if integer(f, block, &kind)? != Some(i64::from(u16::from_le_bytes(*b"OB"))) {
                return Ok(None);
            }
        }
        _ => return Ok(None),
    }
    Ok(Some(PointerClear {
        block,
        offset: b.data_offset + offset,
        address,
        steps: path,
        native_action: if f.dna_type_id(block)? == b"NodesModifierData" {
            "reconstruct_legacy_node_group"
        } else {
            native_id_action(f, owner)?
        },
    }))
}
/// Full copied-library scan includes unselected records: a pack must not retain
/// these known garbage ID references merely because they were not selected.
pub(crate) fn clears(f: &BlendFile) -> Result<Vec<PointerClear>> {
    let mut out = Vec::new();
    for (i, b) in f.blocks().iter().enumerate() {
        if b.code != *b"DATA" {
            continue;
        }
        for (path, _) in sites(f, i)? {
            let loc = f.locate_field(i, 0, &path)?;
            if let Some(clear) = missing_id(f, i, loc.offset)? {
                out.push(clear)
            } else {
                // An unselected known slot is not permission to copy a bad
                // pointer. If the narrow repair does not qualify, retain the
                // original strict resolution refusal before any publication.
                if let Some(target) = f.get_pointer(i, 0, &path)? {
                    validate_present(f, i, loc.offset, target)?;
                }
            }
        }
    }
    Ok(out)
}
