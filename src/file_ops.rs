//! Source-DNA structural operations. Serialized identities are owner-scoped.
mod embedded;
#[expect(
    dead_code,
    reason = "construction scaffolding for 1.1 structural primitives; used by tests and the private factory"
)]
pub(crate) mod graph;
pub(crate) mod index;
mod owner;
pub(crate) use owner::qualify_read_id;
pub(crate) mod roles;
pub(crate) mod validate;
use crate::checked::{add, mul, number};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet};
// Shared source traversal. Contextual resolution, type qualification and ListBase
// checks have one owner; consumers do not reconstruct another pointer resolver.
pub(crate) fn source_target(f: &BlendFile, b: usize, p: &[&str]) -> Result<Option<usize>> {
    let a = graph::uint(f, b, p)?;
    if a == 0 {
        return Ok(None);
    }
    let name = format!("[0].{}", p.join("."));
    let layout = crate::file_ops::pointer_layout(f, b)?
        .ok_or_else(|| Error::new("pointer_role", "source is opaque"))?;
    let field = layout
        .iter()
        .find(|x| x.path == name)
        .ok_or_else(|| Error::new("pointer_role", format!("source path {name}")))?;
    let target = f.owner_index()?.resolve(f, b, a, field)?;
    crate::file_ops::roles::qualify(f, b, field, target)?;
    if field.parent_type == b"ListBase" && field.field_name == b"first" {
        crate::file_ops::roles::validate_list(f, b, field)?;
    }
    Ok(Some(target))
}
pub(crate) fn source_list(f: &BlendFile, owner: usize, p: &[&str]) -> Result<Vec<usize>> {
    let mut first = p.to_vec();
    first.push("first");
    let mut last = p.to_vec();
    last.push("last");
    let mut current = source_target(f, owner, &first)?;
    let end = source_target(f, owner, &last)?;
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut previous = None;
    while let Some(id) = current {
        if !seen.insert(id) || source_target(f, id, &["prev"])? != previous {
            return Err(Error::new("list_integrity", "source cycle/symmetry"));
        }
        out.push(id);
        previous = current;
        current = source_target(f, id, &["next"])?;
    }
    if previous != end {
        return Err(Error::new("list_integrity", "source endpoint"));
    }
    Ok(out)
}
#[derive(Debug, Clone)]
pub struct PointerSite {
    pub block: usize,
    pub owner: Option<usize>,
    pub offset: usize,
    pub path: String,
    pub declared_type: Vec<u8>,
    pub address: u64,
    pub target: Option<(usize, usize)>,
    pub state: &'static str,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct Census {
    pub owners: Vec<Option<usize>>,
    pub sites: Vec<PointerSite>,
    pub opaque: Vec<usize>,
}
#[derive(Debug, Clone)]
pub(crate) struct PointerLayout {
    pub offset: usize,
    pub path: String,
    pub declared_type: Vec<u8>,
    pub parent_type: Vec<u8>,
    pub parent_offset: usize,
    pub field_name: Vec<u8>,
    pub pointer_depth: usize,
    pub list_role: Option<(Vec<u8>, Vec<u8>)>,
}
#[derive(Clone)]
struct Work {
    si: usize,
    base: usize,
    path: String,
    role: Option<(Vec<u8>, Vec<u8>)>,
}
enum Walk {
    Enter(Work),
    Exit(usize),
}
pub(crate) fn pointer_layout(f: &BlendFile, block: usize) -> Result<Option<Vec<PointerLayout>>> {
    let b = f.block(block)?;
    if b.code == *b"DNA1" || b.sdna_index == 0 {
        return Ok(None);
    }
    let si = f.struct_index(block)?;
    let size = f.schema().lengths[f.schema().structs[si].type_index];
    if mul(size, b.count)? != b.size {
        return Err(Error::new(
            "record_extent",
            format!("block {block}: typed count/length mismatch"),
        ));
    }
    let mut out = Vec::new();
    for i in 0..b.count {
        walk(f, si, mul(i, size)?, &format!("[{i}]"), &mut out)?;
    }
    Ok(Some(out))
}
pub(crate) fn walk(
    f: &BlendFile,
    si: usize,
    base: usize,
    path: &str,
    out: &mut Vec<PointerLayout>,
) -> Result<()> {
    let mut stack = vec![Walk::Enter(Work {
        si,
        base,
        path: path.to_owned(),
        role: None,
    })];
    let mut active = BTreeSet::new();
    while let Some(work) = stack.pop() {
        let w = match work {
            Walk::Exit(si) => {
                active.remove(&si);
                continue;
            }
            Walk::Enter(w) => w,
        };
        if !active.insert(w.si) {
            return Err(Error::new(
                "invalid_sdna",
                "recursive by-value pointer layout",
            ));
        }
        stack.push(Walk::Exit(w.si));
        let st = &f.schema().structs[w.si];
        let ty = &f.schema().types[st.type_index];
        for field in st.fields.iter().rev() {
            for index in (0..field.array_len).rev() {
                let offset = add(add(w.base, field.offset)?, mul(index, field.element_size)?)?;
                let name = String::from_utf8_lossy(&field.name);
                let path = if field.array_len == 1 {
                    format!("{}.{name}", w.path)
                } else {
                    format!("{}.{name}[{index}]", w.path)
                };
                if field.pointer {
                    out.push(PointerLayout {
                        offset,
                        path,
                        declared_type: f.schema().types[field.type_index].clone(),
                        parent_type: ty.clone(),
                        parent_offset: w.base,
                        field_name: field.name.clone(),
                        pointer_depth: field.full_name.iter().filter(|b| **b == b'*').count(),
                        list_role: w.role.clone(),
                    });
                } else if let Some(&nested) = f.schema().by_type.get(&field.type_index) {
                    let role = if f.schema().types[field.type_index] == b"ListBase" {
                        Some((ty.clone(), field.name.clone()))
                    } else {
                        w.role.clone()
                    };
                    stack.push(Walk::Enter(Work {
                        si: nested,
                        base: offset,
                        path,
                        role,
                    }));
                }
            }
        }
    }
    out.sort_by_key(|p| p.offset);
    Ok(())
}
impl Census {
    pub fn json(&self, full: bool) -> Json {
        let mut states = BTreeMap::<&str, usize>::new();
        for s in &self.sites {
            *states.entry(s.state).or_default() += 1;
        }
        let mut result = json!({"schema":"oxblend.pointer-census/v2","whole_file":true,"blocks":self.owners.len(),"pointer_sites":self.sites.len(),"states":states,"opaque_allocations":self.opaque,"semantic_verified":false});
        if full {
            result["sites"]=json!(self.sites.iter().map(|s|json!({"block":s.block,"owner":s.owner,"offset":s.offset,"path":s.path,"type":String::from_utf8_lossy(&s.declared_type),"address":s.address,"target":s.target,"state":s.state})).collect::<Vec<_>>());
        }
        result
    }
    pub fn require_closed(&self) -> Result<()> {
        for s in &self.sites {
            if !matches!(s.state, "resolved" | "null") {
                return Err(Error::new(
                    s.state,
                    format!(
                        "block {} {} ({})",
                        s.block,
                        s.path,
                        String::from_utf8_lossy(&s.declared_type)
                    ),
                ));
            }
        }
        Ok(())
    }
}
pub fn pointer_census(f: &BlendFile) -> Result<Census> {
    owner::qualify_read(f)?;
    census(f, validate::census_site)
}
/// Writer/receiving proof keeps the checked role admission, independent of
/// diagnostic report profiles. Graph publication uses this boundary.
pub(crate) fn checked_pointer_census(f: &BlendFile) -> Result<Census> {
    f.owner_index()?.admit_write(f)?;
    census(f, validate::write_site)
}
fn census(
    f: &BlendFile,
    validate: fn(&BlendFile, usize, &PointerLayout, u64) -> Result<Option<usize>>,
) -> Result<Census> {
    let index = f.owner_index()?;
    let mut sites = Vec::new();
    let mut opaque = Vec::new();
    for (i, b) in f.blocks().iter().enumerate() {
        if b.code == *b"DNA1" {
            continue;
        }
        let Some(layout) = pointer_layout(f, i)? else {
            opaque.push(i);
            continue;
        };
        let payload = f.payload(i)?;
        for field in layout {
            let address = number(
                &payload,
                field.offset,
                f.header().pointer_size,
                f.header().little_endian,
            )?;
            let (target, state) = match validate(f, i, &field, address) {
                Ok(Some(target)) => (Some((target, 0)), "resolved"),
                Ok(None) => (None, "null"),
                Err(e) => (None, e.kind),
            };
            sites.push(PointerSite {
                block: i,
                owner: index.owner(i),
                offset: field.offset,
                path: field.path,
                declared_type: field.declared_type,
                address,
                target,
                state,
            });
        }
    }
    Ok(Census {
        owners: (0..f.blocks().len()).map(|i| index.owner(i)).collect(),
        sites,
        opaque,
    })
}
