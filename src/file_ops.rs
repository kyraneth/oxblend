//! Source-DNA structural operations. Serialized identities are owner-scoped.
mod embedded;
#[expect(
    dead_code,
    reason = "construction scaffolding for 1.1 structural primitives; used by tests and the private factory"
)]
pub(crate) mod graph;
pub(crate) mod index;
mod owner;
#[cfg(test)]
#[path = "../newtests/schema_program.rs"]
mod schema_program_tests;
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
/// Streaming execution owns only the schema program, never a reader/body lease.
/// All structural failures are checked before yielding the first site.
pub(crate) struct PointerSites {
    program: std::sync::Arc<[PointerLayout]>,
    size: usize,
    count: usize,
    record: usize,
    site: usize,
}
pub(crate) struct ProgramSite {
    program: std::sync::Arc<[PointerLayout]>,
    site: usize,
    record: usize,
    base: usize,
    pub(crate) offset: usize,
}
impl ProgramSite {
    pub(crate) fn template_index(&self) -> usize {
        self.site
    }
    pub(crate) fn record_index(&self) -> usize {
        self.record
    }
    pub(crate) fn layout(&self) -> PointerLayout {
        let template = &self.program[self.site];
        let mut field = template.clone();
        field.offset = self.offset;
        field.parent_offset += self.base;
        field.path = format!("[{}]{}", self.record, template.path);
        field
    }
}
impl Iterator for PointerSites {
    type Item = ProgramSite;
    fn next(&mut self) -> Option<Self::Item> {
        if self.record == self.count || self.program.is_empty() {
            return None;
        }
        let template = &self.program[self.site];
        let base = self.record * self.size; // preflighted by pointer_sites
        let field = ProgramSite {
            program: self.program.clone(),
            site: self.site,
            record: self.record,
            base,
            offset: template.offset + base,
        };
        self.site += 1;
        if self.site == self.program.len() {
            self.site = 0;
            self.record += 1;
        }
        Some(field)
    }
}

pub(crate) fn pointer_layout(f: &BlendFile, block: usize) -> Result<Option<Vec<PointerLayout>>> {
    Ok(pointer_sites(f, block)?.map(|sites| sites.map(|site| site.layout()).collect()))
}
pub(crate) fn pointer_sites(f: &BlendFile, block: usize) -> Result<Option<PointerSites>> {
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
    // A zero-count block did not walk its type in the compatibility path.
    let program = if b.count == 0 {
        std::sync::Arc::from([])
    } else {
        f.schema().pointer_programs[si]
            .get_or_init(|| {
                let mut out = Vec::new();
                walk(f, si, 0, "", &mut out)?;
                Ok(std::sync::Arc::from(out))
            })
            .clone()?
    };
    let base = mul(b.count.saturating_sub(1), size)?;
    for field in program.iter() {
        add(base, field.offset)?;
        add(base, field.parent_offset)?;
    }
    Ok(Some(PointerSites {
        program,
        size,
        count: b.count,
        record: 0,
        site: 0,
    }))
}
#[cfg(test)]
thread_local! {
    static PROGRAM_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) fn walk(
    f: &BlendFile,
    si: usize,
    base: usize,
    path: &str,
    out: &mut Vec<PointerLayout>,
) -> Result<()> {
    #[cfg(test)]
    PROGRAM_BUILDS.with(|n| n.set(n.get() + 1));
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
// S3 unified base traversal: single owner of pointer iteration plus validation.
// Both the compatibility Census and the diff compact collector share this walker;
// admission (qualify_read / admit_write) stays with each caller.
fn for_each_base_site(
    f: &BlendFile,
    validate: fn(&BlendFile, usize, &PointerLayout, u64) -> Result<Option<usize>>,
    mut visit: impl FnMut(
        usize,
        Option<usize>,
        usize,
        usize,
        usize,
        PointerLayout,
        u64,
        Option<(usize, usize)>,
        &'static str,
    ),
) -> Result<(Vec<Option<usize>>, Vec<usize>)> {
    let index = f.owner_index()?;
    let owners = index.owners(f);
    let mut opaque = Vec::new();
    for (i, b) in f.blocks().iter().enumerate() {
        if b.code == *b"DNA1" {
            continue;
        }
        let Some(sites) = pointer_sites(f, i)? else {
            opaque.push(i);
            continue;
        };
        // L19: borrowed view (zero-copy for in-memory bodies and
        // single-window selective blocks) instead of one owned copy per
        // block. Guards and byte identity match `payload()`.
        let view = f.payload_view(i)?;
        let payload = view.as_slice();
        for site in sites {
            let template = site.template_index();
            let record = site.record_index();
            let offset = site.offset;
            let layout = site.layout();
            let address = number(
                payload,
                offset,
                f.header().pointer_size,
                f.header().little_endian,
            )?;
            let (target, state) = match validate(f, i, &layout, address) {
                Ok(Some(target)) => (Some((target, 0)), "resolved"),
                Ok(None) => (None, "null"),
                Err(e) => (None, e.kind),
            };
            visit(
                i, owners[i], template, record, offset, layout, address, target, state,
            );
        }
    }
    Ok((owners, opaque))
}
// Compact diff edge: source-schema handles, no per-site owned path/type.
// Base edges reference their schema program template plus record; expanded
// array elements reference their parent edge. Paths/types render on demand.
pub(crate) const COMPACT_EXPANDED_TEMPLATE: usize = usize::MAX;
pub(crate) const COMPACT_BASE_PARENT: usize = usize::MAX;
#[derive(Debug, Clone, Copy)]
pub(crate) struct CompactSite {
    pub block: usize,
    pub offset: usize,
    pub address: u64,
    pub target: Option<(usize, usize)>,
    // Retained evidence is checked by compact_retention_accounting even when
    // the report does not consume the state/type of every compact edge.
    #[allow(
        dead_code,
        reason = "preserved pointer-state evidence for retention qualification"
    )]
    pub state: &'static str,
    pub template: usize,
    pub record: usize,
    #[allow(
        dead_code,
        reason = "preserved expanded-edge declared-type parent handle"
    )]
    pub parent: usize,
}
pub(crate) fn compact_template(
    f: &BlendFile,
    block: usize,
    template: usize,
) -> Result<&PointerLayout> {
    let si = f.struct_index(block)?;
    let cell = f
        .schema()
        .pointer_programs
        .get(si)
        .ok_or_else(|| Error::new("diff_extent", "schema program absent"))?;
    let cached = cell
        .get()
        .ok_or_else(|| Error::new("diff_extent", "schema program not built"))?;
    let program = cached
        .as_ref()
        .map_err(|e| Error::new(e.kind, e.message.clone()))?;
    program
        .get(template)
        .ok_or_else(|| Error::new("diff_extent", "schema template out of range"))
}
pub(crate) fn compact_path(f: &BlendFile, edges: &[CompactSite], idx: usize) -> Result<String> {
    let edge = &edges[idx];
    if edge.template != COMPACT_EXPANDED_TEMPLATE {
        let template = compact_template(f, edge.block, edge.template)?;
        Ok(format!("[{0}]{1}", edge.record, template.path))
    } else {
        Ok(format!("[{0}]", edge.record))
    }
}
#[allow(
    dead_code,
    reason = "retained declared-type evidence checked by qualification"
)]
pub(crate) fn compact_declared<'a>(
    f: &'a BlendFile,
    edges: &[CompactSite],
    idx: usize,
) -> Result<&'a [u8]> {
    let edge = &edges[idx];
    if edge.template != COMPACT_EXPANDED_TEMPLATE {
        Ok(compact_template(f, edge.block, edge.template)?
            .declared_type
            .as_slice())
    } else {
        let parent = edges
            .get(edge.parent)
            .ok_or_else(|| Error::new("diff_extent", "expanded parent absent"))?;
        debug_assert!(parent.template != COMPACT_EXPANDED_TEMPLATE);
        Ok(compact_template(f, parent.block, parent.template)?
            .declared_type
            .as_slice())
    }
}
/// Diff compact base collection with the compatibility qualify_read boundary.
/// Shares for_each_base_site with Census; no duplicated resolver.
type CompactBase = (Vec<Option<usize>>, Vec<CompactSite>, Vec<usize>);
pub(crate) fn collect_compact_base(f: &BlendFile) -> Result<CompactBase> {
    owner::qualify_read(f)?;
    let mut edges = Vec::new();
    let (owners, opaque) = for_each_base_site(
        f,
        validate::census_site,
        |block, _owner, template, record, offset, _layout, address, target, state| {
            edges.push(CompactSite {
                block,
                offset,
                address,
                target,
                state,
                template,
                record,
                parent: COMPACT_BASE_PARENT,
            });
        },
    )?;
    Ok((owners, edges, opaque))
}
fn census(
    f: &BlendFile,
    validate: fn(&BlendFile, usize, &PointerLayout, u64) -> Result<Option<usize>>,
) -> Result<Census> {
    let mut sites = Vec::new();
    let (owners, opaque) = for_each_base_site(
        f,
        validate,
        |block, owner, _template, _record, _offset, layout, address, target, state| {
            let offset = layout.offset;
            let path = layout.path;
            let declared_type = layout.declared_type;
            sites.push(PointerSite {
                block,
                owner,
                offset,
                path,
                declared_type,
                address,
                target,
                state,
            });
        },
    )?;
    Ok(Census {
        owners,
        sites,
        opaque,
    })
}
