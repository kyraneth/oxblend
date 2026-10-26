//! Invocation/source-owned pointer index. Only queried DATA owner runs are indexed.
//! Duplicate candidates stay ambiguous; exceptional cross-owner roles stay explicit.
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_projection_retains_only_nonempty_runs() {
        let f = BlendFile::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/file-ops/synthetic-control.blend"
        ))
        .unwrap();
        let expected = owner::read_owners(&f);
        let runs = f
            .blocks()
            .iter()
            .enumerate()
            .filter(|(i, b)| {
                b.code == *b"DATA"
                    && expected[*i].is_some()
                    && (*i == 0 || f.blocks()[*i - 1].code != *b"DATA")
            })
            .count();
        let ix = Index::new(&f).unwrap();
        assert_eq!(ix.retained_owner_entries(), runs);
    }
    fn compare(f: &BlendFile) {
        let expected = owner::read_owners(f);
        let ix = Index::new(f).unwrap();
        assert_eq!(ix.owners(f), expected);
        for (i, own) in expected.iter().enumerate() {
            assert_eq!(ix.owner(f, i), *own, "ordinal {i}");
            if *own == Some(i) {
                let want: Vec<_> = expected
                    .iter()
                    .enumerate()
                    .filter_map(|(b, o)| (*o == Some(i)).then_some(b))
                    .collect();
                assert_eq!(ix.owned_blocks(f, i).collect::<Vec<_>>(), want);
            }
        }
    }
    #[test]
    fn sparse_runs_equal_every_reference_ordinal_and_owned_block() {
        for name in [
            "file-ops/synthetic-control",
            "file-ops/synthetic-empty-seed",
            "file-ops/duplicate-cross-ID-legal",
            "file-ops/duplicate-same-ID-refused",
            "trace-owner/read-compat/data-before-id",
            "trace-owner/read-compat/data-after-control",
            "trace-owner/read-compat/data-after-placeholder",
            "trace-owner/read-compat/unknown-id",
            "trace-owner/cross-id",
            "trace-owner/same-id",
            "trace-owner/foreign-only",
        ] {
            let f = BlendFile::open(format!(
                "{}/tests/fixtures/{name}.blend",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap();
            compare(&f);
        }
    }
    #[test]
    fn sparse_runs_cover_native_boundary_transitions() {
        let mut f = BlendFile::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/file-ops/synthetic-control.blend"
        ))
        .unwrap();
        // Four-header sequences cover owner/DATA/separator/DATA and forwarding
        // placeholder boundaries; the trailing DATA checks continuation.
        let codes = [
            *b"OB\0\0", *b"ZZ\0\0", *b"USER", *b"ID\0\0", *b"DATA", *b"DNA1", *b"REND",
        ];
        for a in codes {
            for b in codes {
                for c in codes {
                    for d in codes {
                        // L19 adaptation (s1 Box<[Block]>): same elements
                        // and order, collected into the exact-sized storage.
                        f.blocks = [a, b, c, d, *b"DATA"]
                            .into_iter()
                            .map(|code| crate::Block {
                                code,
                                old_address: 0,
                                sdna_index: 0,
                                count: 0,
                                data_offset: 0,
                                size: 0,
                            })
                            .collect::<Box<[crate::Block]>>();
                        compare(&f);
                    }
                }
            }
        }
    }
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
    owner: usize,
    range: Range<usize>,
    addresses: OnceCell<Result<HashMap<u64, usize>>>,
}
#[derive(Debug)]
pub(crate) struct Index {
    runs: Vec<Scope>,
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
    #[cfg(test)]
    fn retained_owner_entries(&self) -> usize {
        self.runs.len()
    }

    pub fn new(f: &BlendFile) -> Result<Self> {
        let mut runs: Vec<Scope> = Vec::new();
        let mut ids = HashMap::new();
        let mut data_owner = None;
        for (i, b) in f.blocks().iter().enumerate() {
            if b.code == *b"DATA" {
                let Some(owner) = data_owner else {
                    continue; // top-level DATA is unowned for reads
                };
                if let Some(run) = runs.last_mut().filter(|run| run.owner == owner) {
                    run.range.end = i + 1;
                } else {
                    runs.push(Scope {
                        owner,
                        range: i..i + 1,
                        addresses: OnceCell::new(),
                    });
                }
            } else {
                let id = owner::read_id(&b.code);
                data_owner = ((id || b.code == *b"USER") && b.code != *b"ID\0\0").then_some(i);
                if id {
                    insert(&mut ids, b.old_address, i);
                }
            }
        }
        Ok(Self {
            runs,
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
    pub fn owner(&self, f: &BlendFile, b: usize) -> Option<usize> {
        let code = &f.blocks()[b].code;
        if *code != *b"DATA" {
            return (owner::read_id(code) || *code == *b"USER").then_some(b);
        }
        self.data_run(b).map(|run| run.owner)
    }
    fn data_run(&self, b: usize) -> Option<&Scope> {
        let after = self.runs.partition_point(|run| run.range.start <= b);
        after
            .checked_sub(1)
            .and_then(|i| self.runs.get(i))
            .filter(|run| b < run.range.end)
    }
    fn owner_run(&self, owner: usize) -> Option<&Scope> {
        let i = self.runs.partition_point(|run| run.owner < owner);
        self.runs.get(i).filter(|run| run.owner == owner)
    }
    /// Ordered native header followed by its DATA, without a full-file scan.
    pub fn owned_blocks(&self, f: &BlendFile, owner: usize) -> impl Iterator<Item = usize> {
        let header = (self.owner(f, owner) == Some(owner)).then_some(owner);
        let data = self.owner_run(owner).map_or(0..0, |run| run.range.clone());
        header.into_iter().chain(data)
    }
    /// Census explicitly requests a dense result; derive it with a linear run cursor.
    pub fn owners(&self, f: &BlendFile) -> Vec<Option<usize>> {
        let mut cursor = 0;
        f.blocks()
            .iter()
            .enumerate()
            .map(|(i, b)| {
                while self.runs.get(cursor).is_some_and(|run| run.range.end <= i) {
                    cursor += 1;
                }
                if b.code == *b"DATA" {
                    self.runs
                        .get(cursor)
                        .filter(|run| run.range.contains(&i))
                        .map(|run| run.owner)
                } else {
                    (owner::read_id(&b.code) || b.code == *b"USER").then_some(i)
                }
            })
            .collect()
    }
    pub fn local(&self, f: &BlendFile, b: usize, address: u64) -> Result<Option<usize>> {
        let scope = if f.blocks()[b].code == *b"DATA" {
            self.data_run(b)
        } else {
            self.owner(f, b).and_then(|o| self.owner_run(o))
        };
        let Some(scope) = scope else {
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
        let result = (|| {
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
                            for run in &self.runs {
                                for i in run.range.clone() {
                                    insert(&mut map, f.blocks()[i].old_address, i);
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
        })();
        if let Err(error) = &result {
            use crate::failure_receipt::{
                Namespace as DiagnosticNamespace, Resolution, SourceContext, record_source_failure,
            };
            let mut source = SourceContext::block(f, b);
            source.pointer = Some(address);
            // `path` is only a display label here, not authoritative saved
            // byte components. The typed forwarding hook retains those.
            source.field_path = Vec::new();
            source.namespace = match namespace {
                Namespace::Id => DiagnosticNamespace::Id,
                Namespace::Data => DiagnosticNamespace::OwnerData,
                Namespace::CrossData => DiagnosticNamespace::QualifiedCrossData,
            };
            let (check_id, resolution) = match error.kind {
                "unresolved_persistent" => {
                    ("pointer.namespace.lookup.missing", Resolution::Missing)
                }
                "ambiguous_pointer" => {
                    ("pointer.namespace.lookup.ambiguous", Resolution::Ambiguous)
                }
                "pointer_target_type" => (
                    "pointer.namespace.lookup.wrong_namespace",
                    Resolution::WrongType,
                ),
                _ => ("pointer.namespace.lookup.forwarded", Resolution::Unknown),
            };
            source.resolution = resolution;
            record_source_failure(error, check_id, source, &[]);
        }
        result
    }
}
