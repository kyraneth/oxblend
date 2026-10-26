//! Two-phase linked subtype lookup. Byte ownership stays with CLI or Session.
use super::access::{self, Record};
use crate::value::hex;
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

/// Owned request: capture bytes after dropping any borrowed root/store views.
#[derive(Clone, Debug)]
pub struct LibraryRequest {
    pub path: PathBuf,
    pub key: PathBuf,
    pub id_names: Vec<Vec<u8>>,
    pub reason: String,
}
/// A read-only retained library and its byte owner's provenance.
#[derive(Clone)]
pub struct LibraryView<'a> {
    pub reader: &'a BlendFile,
    pub provenance: Json,
}
/// Failed captures are supplied too: their unavailable outcomes are not retried.
pub type LibraryMap<'a> = BTreeMap<PathBuf, Result<LibraryView<'a>>>;

pub(super) struct DefaultLibraries {
    files: BTreeMap<PathBuf, Result<(BlendFile, Json)>>,
    signatures: BTreeMap<PathBuf, crate::fetch::CaptureSignature>,
}
impl DefaultLibraries {
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            signatures: BTreeMap::new(),
        }
    }
    pub fn views(&self) -> LibraryMap<'_> {
        self.files
            .iter()
            .map(|(key, result)| {
                (
                    key.clone(),
                    match result {
                        Ok((reader, provenance)) => Ok(LibraryView {
                            reader,
                            provenance: provenance.clone(),
                        }),
                        Err(error) => Err(error.clone()),
                    },
                )
            })
            .collect()
    }
    pub fn load(&mut self, requests: &[LibraryRequest]) {
        for request in requests {
            if self.files.contains_key(&request.key) {
                continue;
            }
            let path = &request.path;
            let span = crate::diagnostics::Span::new(
                "query.library.open",
                json!({"path":path.to_str(),"path_hex":hex(path.as_os_str().as_encoded_bytes()),"purpose":request.reason}),
            );
            let result = (|| {
                let meta = path.metadata().map_err(|e| {
                    Error::new(
                        if e.kind() == std::io::ErrorKind::NotFound {
                            "missing_library"
                        } else {
                            "library_read"
                        },
                        format!("{}: {e}", path.display()),
                    )
                })?;
                let signature = crate::fetch::capture_signature(&meta);
                let file = BlendFile::open(path).map_err(|e| {
                    if e.kind == "input_changed" {
                        e
                    } else {
                        Error::new("library_read", format!("{}: {e}", path.display()))
                    }
                })?;
                let after = path
                    .metadata()
                    .map_err(|e| Error::new("input_changed", e.to_string()))?;
                if signature != crate::fetch::capture_signature(&after) {
                    return Err(Error::new(
                        "input_changed",
                        "library changed during subtype admission",
                    ));
                }
                self.signatures.insert(path.clone(), signature);
                Ok((
                    file,
                    json!({"owner":"query-default-library-owner","path":path.to_str(),"path_hex":hex(path.as_os_str().as_encoded_bytes()),"identity_kind":"stat_not_content_hash"}),
                ))
            })();
            self.files.insert(request.key.clone(), span.result(result));
        }
    }
    pub fn check(&self) -> Result<()> {
        for (path, signature) in &self.signatures {
            let now = path
                .metadata()
                .map_err(|e| Error::new("input_changed", e.to_string()))?;
            if &crate::fetch::capture_signature(&now) != signature {
                return Err(Error::new(
                    "input_changed",
                    "library path identity changed during subtype query",
                ));
            }
        }
        Ok(())
    }
}
pub(super) fn absolute_lexical(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| Error::new("io", e.to_string()))?
            .join(path)
    };
    Ok(crate::trace::lexical(&absolute))
}
/// A resolved named ID and the library-chain evidence that identifies it.
pub(super) struct ResolvedId<'a> {
    pub reader: &'a BlendFile,
    pub record: Record,
    pub path: PathBuf,
    pub provenance: Vec<Json>,
}
/// Read the saved Library path without losing non-UTF8 bytes.
pub(super) fn library_bytes(file: &BlendFile, record: &Record) -> Result<Option<Vec<u8>>> {
    let field = if file.dna_type_id(record.block)? == b"ID" {
        "lib"
    } else {
        "id.lib"
    };
    let Some(library) = access::pointer(file, record, field)? else {
        return Ok(None);
    };
    let value = access::get(file, &library, "filepath").or_else(|e| {
        if e.kind == "missing_field" {
            access::get(file, &library, "name")
        } else {
            Err(e)
        }
    })?;
    match value {
        crate::value::Value::Bytes(bytes) => Ok(Some(bytes)),
        _ => Err(Error::new(
            "type",
            "saved Library path is not a byte string",
        )),
    }
}

struct PendingRequest {
    path: PathBuf,
    names: BTreeSet<Vec<u8>>,
    reasons: BTreeSet<String>,
}

pub(super) struct Context<'a, 'b> {
    root: &'b BlendFile,
    source: Option<PathBuf>,
    libraries: &'a LibraryMap<'b>,
    requests: BTreeMap<PathBuf, PendingRequest>,
    pub observations: Vec<Json>,
    pub diagnostics: Vec<Json>,
}
impl<'a, 'b> Context<'a, 'b> {
    pub fn new(root: &'b BlendFile, source: Option<&Path>, libraries: &'a LibraryMap<'b>) -> Self {
        Self {
            root,
            source: source.map(Path::to_path_buf),
            libraries,
            requests: BTreeMap::new(),
            observations: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
    pub fn requests(&self) -> Vec<LibraryRequest> {
        self.requests
            .iter()
            .map(|(key, pending)| LibraryRequest {
                path: pending.path.clone(),
                key: key.clone(),
                id_names: pending.names.iter().cloned().collect(),
                reason: pending
                    .reasons
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(","),
            })
            .collect()
    }
    fn view(&mut self, path: &Path, name: &[u8], reason: &str) -> Result<LibraryView<'b>> {
        let key = absolute_lexical(path)?;
        match self.libraries.get(&key) {
            Some(result) => result.clone(),
            None if self
                .source
                .as_deref()
                .map(absolute_lexical)
                .transpose()?
                .as_deref()
                == Some(key.as_path()) =>
            {
                Ok(LibraryView {
                    reader: self.root,
                    provenance: json!({"owner":"query-root-reader"}),
                })
            }
            None => {
                let entry = self.requests.entry(key).or_insert_with(|| PendingRequest {
                    path: path.to_owned(),
                    names: BTreeSet::new(),
                    reasons: BTreeSet::new(),
                });
                entry.names.insert(name.to_vec());
                entry.reasons.insert(reason.to_owned());
                Err(Error::new(
                    "library_read",
                    format!(
                        "library {} must be supplied by the byte owner before ID evaluation",
                        path.display()
                    ),
                ))
            }
        }
    }
    pub fn check(&self) -> Result<()> {
        for view in self.libraries.values().flatten() {
            view.reader.check_source()?;
        }
        Ok(())
    }
    /// One iterative named-ID resolver for subtype and explicit graph queries.
    pub(super) fn resolve_id(
        &mut self,
        owner: Option<&Path>,
        library: &[u8],
        id_name: &[u8],
        reason: &str,
    ) -> Result<ResolvedId<'b>> {
        if id_name.len() < 2 {
            return Err(Error::new("type", "linked ID lacks its two-byte code"));
        }
        let mut owner = owner.map(Path::to_owned);
        let mut library = library.to_vec();
        let mut active = HashSet::new();
        let mut chain = Vec::new();
        loop {
            let source = match owner.as_deref() {
                Some(source) => source,
                None if library.starts_with(b"/") && !library.starts_with(b"//") => Path::new("/"),
                None => {
                    return Err(Error::new(
                        "unverified_semantics",
                        "relative linked ID library requires a source-aware query",
                    ));
                }
            };
            let path = absolute_lexical(&crate::trace::resolve(source, &library))?;
            if !active.insert((path.clone(), id_name.to_vec())) {
                return Err(Error::new(
                    "query_cycle",
                    "cyclic linked ID forwarding chain",
                ));
            }
            let view = self.view(&path, id_name, reason)?;
            let file = view.reader;
            let candidates = crate::trace::named_blocks(file, &[id_name.to_vec()])?;
            if candidates.is_empty() {
                return Err(Error::new(
                    "missing_id",
                    format!("saved ID {} absent from {}", hex(id_name), path.display()),
                ));
            }
            if candidates.len() != 1 {
                return Err(Error::new(
                    "ambiguous_id",
                    format!(
                        "saved ID {} has multiple local-first candidates in {}",
                        hex(id_name),
                        path.display()
                    ),
                ));
            }
            let record = Record::new(*candidates.iter().next().unwrap(), 0);
            let reference = access::id_ref(file, &record)?;
            let ty = file.dna_type_id(record.block)?;
            let observation = json!({"name":std::str::from_utf8(&id_name[2..]).ok(),"id_name_hex":hex(id_name),"reason":reason,"library_path":path.to_str(),"library_path_hex":hex(path.as_os_str().as_encoded_bytes()),"canonical_identity":path.canonicalize().ok().map(|p|json!({"path":p.to_str(),"path_hex":hex(p.as_os_str().as_encoded_bytes())})),"selected_from":if ty==b"ID"{"library_stub"}else{"library_local"},"source_local_ref":{"block_index":record.block,"element":0},"saved_reference":reference,"provider":view.provenance});
            self.observations.push(observation.clone());
            chain.push(observation);
            if ty == b"ID" {
                library = library_bytes(file, &record)?.ok_or_else(|| {
                    Error::new(
                        "required_pointer_null",
                        "forwarding ID has no saved library",
                    )
                })?;
                owner = Some(path);
            } else {
                return Ok(ResolvedId {
                    reader: file,
                    record,
                    path,
                    provenance: chain,
                });
            }
        }
    }
    fn subtype(&mut self, owner: Option<&Path>, library: &str, name: &str) -> Result<String> {
        let id_name = [b"NT".as_slice(), name.as_bytes()].concat();
        let resolved = self.resolve_id(
            owner,
            library.as_bytes(),
            &id_name,
            "linked_node_tree_subtype",
        )?;
        if resolved.reader.dna_type_id(resolved.record.block)? != b"bNodeTree" {
            return Err(Error::new(
                "type",
                "selected node-tree name is not a saved bNodeTree",
            ));
        }
        access::text(resolved.reader, &resolved.record, "idname")
    }
    pub fn enrich(&mut self, value: &mut Json) -> Vec<Error> {
        let mut errors = Vec::new();
        self.visit(value, "", &mut errors);
        errors
    }
    fn visit(&mut self, value: &mut Json, path: &str, errors: &mut Vec<Error>) {
        if value["kind"] == "NodeTree" && !value["library"].is_null() {
            let reference = value.clone();
            self.observations.push(
                json!({"selected_from":"local_stub","path":path,"saved_reference":reference}),
            );
            let result = match (value["library"].as_str(), value["name"].as_str()) {
                (Some(library), Some(name)) => {
                    let source = self.source.clone();
                    self.subtype(source.as_deref(), library, name)
                }
                _ => Err(Error::new(
                    "encoding",
                    "linked subtype needs decodable name and library identity",
                )),
            };
            match result {
                Ok(kind) => value["kind"] = json!(kind),
                Err(error) => {
                    self.diagnostics.push(json!({"kind":error.kind,"message":error.message,"path":path,"reference":reference,"status":"unavailable"}));
                    errors.push(error);
                }
            }
        }
        match value {
            Json::Object(fields) => {
                for (key, item) in fields {
                    self.visit(item, &format!("{path}/{key}"), errors)
                }
            }
            Json::Array(items) => {
                for (index, item) in items.iter_mut().enumerate() {
                    self.visit(item, &format!("{path}/{index}"), errors)
                }
            }
            _ => (),
        }
    }
}

#[cfg(test)]
#[path = "linked_tests.rs"]
mod tests;
