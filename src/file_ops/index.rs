//! Invocation/source-owned pointer index. Only queried DATA owner runs are indexed.
//! Duplicate candidates stay ambiguous; exceptional cross-owner roles stay explicit.
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_never_becomes_last_wins() {
        let mut m = HashMap::new();
        insert(&mut m, 0, 0);
        assert!(m.is_empty());
        insert(&mut m, 7, 1);
        assert_eq!(m[&7], Some(1));
        insert(&mut m, 7, 2);
        assert_eq!(m[&7], None);
        insert(&mut m, 7, 3);
        assert_eq!(m[&7], None);
    }
}
use super::{PointerLayout, owner, roles};
#[derive(Clone, Copy)]
pub(crate) enum Namespace {
    Id,
    Data,
    CrossData,
}
use crate::{BlendFile, Error, Result};
use std::cell::OnceCell;
use std::collections::HashMap;
use std::ops::Range;
#[derive(Debug)]
struct Scope {
    range: Range<usize>,
    addresses: OnceCell<Result<HashMap<u64, usize>>>,
}
#[derive(Debug)]
pub(crate) struct Index {
    owners: Vec<Option<usize>>,
    scopes: HashMap<usize, Scope>,
    ids: HashMap<u64, Option<usize>>,
    cross: OnceCell<HashMap<u64, Option<usize>>>,
    admission: OnceCell<Result<()>>,
}
fn insert(map: &mut HashMap<u64, Option<usize>>, address: u64, block: usize) {
    if address != 0 {
        map.entry(address)
            .and_modify(|v| *v = None)
            .or_insert(Some(block));
    }
}
impl Index {
    pub fn new(f: &BlendFile) -> Result<Self> {
        let owners = owner::read_owners(f);
        let mut scopes = HashMap::<usize, Scope>::new();
        let mut ids = HashMap::new();
        for (i, b) in f.blocks().iter().enumerate() {
            if b.code == *b"DATA" {
                let Some(owner) = owners[i] else {
                    // Readfile's top-level DATA case skips these unowned blocks.
                    continue;
                };
                let scope = scopes.entry(owner).or_insert_with(|| Scope {
                    range: i..i,
                    addresses: OnceCell::new(),
                });
                scope.range.end = i + 1;
            } else if owner::read_id(&b.code) {
                insert(&mut ids, b.old_address, i);
            }
        }
        Ok(Self {
            owners,
            scopes,
            ids,
            cross: OnceCell::new(),
            admission: OnceCell::new(),
        })
    }
    pub fn admit_write(&self, f: &BlendFile) -> Result<()> {
        match self.admission.get_or_init(|| owner::qualify(f)) {
            Ok(()) => Ok(()),
            Err(e) => Err(Error::new(e.kind, e.message.clone())),
        }
    }
    pub fn owner(&self, b: usize) -> Option<usize> {
        self.owners[b]
    }
    pub fn local(&self, f: &BlendFile, b: usize, address: u64) -> Result<Option<usize>> {
        let Some(scope) = self.owners[b].and_then(|o| self.scopes.get(&o)) else {
            return Ok(None);
        };
        match scope.addresses.get_or_init(|| {
            let mut map = HashMap::with_capacity(scope.range.len());
            for i in scope.range.clone() {
                let a = f.blocks()[i].old_address;
                if a != 0 && map.insert(a, i).is_some() {
                    return Err(Error::new(
                        "duplicate_data_address",
                        format!("same native ID namespace repeats {a:#x}"),
                    ));
                }
            }
            Ok(map)
        }) {
            Ok(map) => Ok(map.get(&address).copied()),
            Err(e) => Err(Error::new(e.kind, e.message.clone())),
        }
    }
    pub fn resolve(
        &self,
        f: &BlendFile,
        b: usize,
        address: u64,
        p: &PointerLayout,
    ) -> Result<usize> {
        self.lookup(f, b, address, &p.path, roles::namespace(f, b, address, p)?)
    }
    /// Explicit diagnostic-only owner-DATA lookup. Census has no persistent
    /// role for these serialized handles; a missing allocation still precedes
    /// its role refusal. This is not a type/address fallback for trace/import.
    pub fn census_data(&self, f: &BlendFile, b: usize, address: u64, path: &str) -> Result<usize> {
        self.local(f, b, address)?.ok_or_else(|| {
            Error::new(
                "unresolved_persistent",
                format!("source block {b}, {path}: {address:#x}"),
            )
        })
    }
    /// Trace already identifies a physical BHead, not a pointer's DNA pointee.
    /// An embedded leading-ID record can be DATA: use its native BHead kind.
    pub fn resolve_origin(&self, f: &BlendFile, b: usize, address: u64) -> Result<usize> {
        let namespace = if f.blocks()[b].code == *b"DATA" {
            Namespace::Data
        } else {
            Namespace::Id
        };
        self.lookup(f, b, address, "pack physical origin", namespace)
    }
    pub(crate) fn lookup(
        &self,
        f: &BlendFile,
        b: usize,
        address: u64,
        path: &str,
        namespace: Namespace,
    ) -> Result<usize> {
        // ID lib-map and owner datamap are disjoint lookup contracts. A matching
        // address in the wrong namespace cannot shadow or rescue a pointer.
        let found = match namespace {
            Namespace::Id => self.ids.get(&address).copied(),
            Namespace::Data => match self.local(f, b, address)? {
                Some(target) => Some(Some(target)),
                None if self.ids.contains_key(&address) => {
                    return Err(Error::new(
                        "pointer_target_type",
                        format!(
                            "{}: owner DATA required, only an ID header matches {address:#x}",
                            path
                        ),
                    ));
                }
                None => None,
            },
            Namespace::CrossData => self.local(f, b, address)?.map(Some).or_else(|| {
                self.cross
                    .get_or_init(|| {
                        let mut map = HashMap::new();
                        for (i, block) in f.blocks().iter().enumerate() {
                            if block.code == *b"DATA" && self.owners[i].is_some() {
                                insert(&mut map, block.old_address, i);
                            }
                        }
                        map
                    })
                    .get(&address)
                    .copied()
            }),
        };
        match found {
            Some(Some(i)) => Ok(i),
            Some(None) => Err(Error::new(
                "ambiguous_pointer",
                format!("source block {b}, {address:#x}"),
            )),
            None => Err(Error::new(
                "unresolved_persistent",
                format!("source block {b}, {path}: {address:#x}"),
            )),
        }
    }
}
