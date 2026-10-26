//! Two-phase query adapter over Session-owned captures. No independent reader
//! owner: planning borrows end before any arena insertion, then evaluation
//! borrows only retained readers. Failed first captures are operation-local.
use crate::query::{LibraryMap, LibraryRequest, LibraryView};
use crate::session::{BodyErr, BodyResult, CaptureBatch, InputStore, SourceId, join_body};
use crate::{Error, Operation, Result, Session};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

type Admitted = BTreeMap<PathBuf, Result<(SourceId, Json)>>;

fn views<'a>(store: &'a InputStore, admitted: &Admitted) -> LibraryMap<'a> {
    admitted
        .iter()
        .map(|(key, entry)| {
            let view = match entry {
                Ok((id, provenance)) => store.reader(*id).map(|reader| LibraryView {
                    reader,
                    provenance: provenance.clone(),
                }),
                Err(error) => Err(error.clone()),
            };
            (key.clone(), view)
        })
        .collect()
}

impl Session {
    /// Saved-record query over this Session's retained root and first-used
    /// linked libraries. Captures and source latches are shared with trace and
    /// pack; semantic subtype lookup never opens a second independent reader.
    /// Unavailable field outcomes remain data. Source guard failures refuse
    /// the operation, even when the evaluator produced otherwise useful rows.
    pub fn query(&mut self, type_name: &str, fields: &[String]) -> Operation<Json> {
        if let Err(error) = self.require_single("query") {
            return Operation::new(Err(error), Vec::new());
        }
        crate::tracing::initialize();
        let root = self.root_path().to_path_buf();
        let mut span = crate::diagnostics::Span::new(
            "session.query",
            json!({
                "type":type_name,"field_count":fields.len(),"library_owner":"retained_session",
                "root_path_hex":crate::value::hex(root.as_os_str().as_encoded_bytes())
            }),
        );
        let mut op = self.operation_context(None);
        op.record_touch(self.root);
        if let Err(error) = self.store.check_reuse(self.root, &mut op) {
            let result = Err(error);
            span.record_result(&result);
            return op.finish(result);
        }
        let mut batch = CaptureBatch::with_memory(self.memory());
        let mut admitted = Admitted::new();
        let body = (|| {
            loop {
                let requests = {
                    let libraries = views(&self.store, &admitted);
                    let reader = self.store.reader(self.root)?;
                    crate::query::library_requests(
                        reader,
                        Some(&root),
                        type_name,
                        fields,
                        &libraries,
                    )?
                }; // Every root/library borrow ends before arena insertion.
                if requests.is_empty() {
                    break;
                }
                let mut added = false;
                for request in requests {
                    if admitted.contains_key(&request.key) {
                        continue;
                    }
                    let result =
                        match capture_request(&mut self.store, &mut batch, &mut op, &request) {
                            Ok(capture) => Ok(capture),
                            Err(BodyErr::Independent(error)) => Err(error),
                            Err(guard @ BodyErr::Guard { .. }) => return Err(guard),
                        };
                    admitted.insert(request.key, result);
                    added = true;
                }
                // A structural fixed point, not a retry/iteration budget. An
                // already supplied failed key must never be captured again.
                if !added {
                    break;
                }
            }
            let libraries = views(&self.store, &admitted);
            let reader = self.store.reader(self.root)?;
            crate::query::query_with_libraries(reader, Some(&root), type_name, fields, &libraries)
                .map_err(BodyErr::Independent)
        })();
        drop(batch); // Join existing fetch lanes before completing diagnostics.
        let checks = self.store.check_all(self.root, &mut op);
        let result =
            join_body(body, &checks, &mut op, "session.query", Some(root)).map_err(Error::from);
        match &result {
            Ok(report) => {
                let complete = report["requested_coverage"]["complete"] == true;
                span.data(json!({"complete":complete,"row_count":report["rows"].as_array().map(Vec::len),
                    "library_request_count":admitted.len(),
                    "retained_library_count":admitted.values().filter(|entry| entry.is_ok()).count()}));
                span.end(if complete { "ok" } else { "partial" });
            }
            Err(_) => span.record_result(&result),
        }
        op.finish(result)
    }
}

fn capture_request(
    store: &mut InputStore,
    batch: &mut CaptureBatch,
    op: &mut crate::session::OpCtx,
    request: &LibraryRequest,
) -> BodyResult<(SourceId, Json)> {
    let path = InputStore::resolve(op, &request.path);
    let captured = store.id_for_path(&path).is_ok();
    // Existing entries must pass their retained-source checks, including a
    // disappearance. Never relabel a captured-source change as first-use miss.
    if !captured {
        std::fs::metadata(&path).map_err(|error| {
            Error::new(
                if error.kind() == std::io::ErrorKind::NotFound {
                    "missing_library"
                } else {
                    "library_read"
                },
                format!("{}: {error}", path.display()),
            )
        })?;
    }
    let id = store
        .ensure(batch, &path, true, false, op)
        .map_err(|error| {
            let error = if captured {
                error
            } else {
                error.without_context()
            };
            if captured {
                BodyErr::Guard {
                    source: path.clone(),
                    error,
                }
            } else if matches!(error.kind, "input_changed" | "missing_library") {
                BodyErr::Independent(error)
            } else {
                // Match L3's default owner: field unavailability has the same
                // library_read vocabulary irrespective of who owns the reader.
                // A retained-source guard change must never be normalized away.
                BodyErr::Independent(error.reclassify("library_read"))
            }
        })?;
    let snapshot = store.snapshot(id)?;
    Ok((
        id,
        json!({"owner":"session_retained_library_owner",
        "path":path.to_str(),"path_hex":crate::value::hex(path.as_os_str().as_encoded_bytes()),
        "identity_kind":"stat_signature_not_content_hash",
        "signature":crate::fetch::capture_signature(&snapshot.metadata),
        "decoded_bytes":snapshot.reader.decompressed_len()}),
    ))
}
