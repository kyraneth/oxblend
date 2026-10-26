//! One-reader serialized selection and authored concept discovery.
//! Not Blender evaluation. Explicit collection graphs and narrow linked subtypes
//! share the caller's source-aware acquisition seam.
pub(crate) mod access;
mod collection_graph;
mod compositor;
mod linked;
pub use linked::{LibraryMap, LibraryRequest, LibraryView};
pub(crate) mod props;
mod render;
mod storage;
#[cfg(test)]
mod tests;
mod view_layers;
use crate::value::{Value, hex};
use crate::{BlendFile, Error, Result};
use access::{Record, get, records};
use serde_json::{Value as Json, json};

const CATALOG_REVISION: &str = "2";

fn entries() -> Vec<Json> {
    vec![
        json!({"name":"scene.render","aliases":["render"],"owner":"Scene","disk_paths":["Scene.r","Scene.id.properties","Scene.id.system_properties","Scene.view_settings"],"description":"Saved render controls and explicitly stored engine settings; not evaluated defaults or produced files.","children":["resolution","frame_range","fps","output","engine","common","engine_settings","color_management"]}),
        json!({"name":"scene.view_layers","aliases":["view_layers"],"owner":"Scene","disk_paths":["Scene.view_layers","Scene.master_collection"],"description":"All saved layers, ordered collection paths, memberships and own visibility controls; not evaluated visibility.","children":["layers","collections","objects"]}),
        json!({"name":"scene.compositor","aliases":["compositor"],"owner":"Scene","disk_paths":["Scene.compositing_node_group","Scene.nodetree","bNodeTree.nodes","bNodeTree.links"],"description":"Complete local saved compositor inventory; root binding, nodes, sockets, links, typed properties and version-distinct output models. No execution or produced-path claim.","children":["scene_tree","trees"]}),
        json!({"name":"collection.graph","aliases":["collection_graph"],"owner":"Collection|Scene","owners":["Collection","Scene"],"disk_paths":["Scene.master_collection","Collection.children","Collection.gobject","Object.parent","Object.type","Object.restrictflag","ID.lib","ID.override_library"],"description":"Flat saved collection/object/parent relations through nested lexical library contexts; provenance and saved ownership, not evaluated visibility or studio policy.","children":["records","edges","sources","diagnostics"]}),
        json!({"name":"id.properties","aliases":["id_properties"],"owner":"ID","disk_paths":["ID.properties","ID.system_properties"],"description":"Explicitly stored typed user/system properties. Absent keys are absent, not runtime defaults.","children":["entries","system_entries"]}),
    ]
}
/// Owned catalog; no file, cache, generated RNA export or interpreter needed.
pub fn catalog() -> Json {
    let span = crate::diagnostics::Span::new(
        "catalog.evaluate",
        json!({"catalog_revision":CATALOG_REVISION}),
    );
    let report = json!({"schema":"oxblend.catalog/1","catalog_revision":CATALOG_REVISION,"basis":"saved_controls_not_evaluated_results","concepts":entries(),"read_operations":[{"name":"byte_array","rust_method":"BlendFile::byte_array","cli":"get FILE BLOCK PATH_JSON [ELEMENT] --byte-array COUNT","declared_field":"one char*/uchar*/int8_t* with saved one-byte width","count":"explicit requested items from the actual saved count field; no default","result":"owned exact bytes including NUL","trace_span":"read.byte_array"},{"name":"pointer_table","rust_method":"BlendFile::pointer_table","cli":"get FILE BLOCK PATH_JSON [ELEMENT] --pointer-table COUNT","declared_field":"one struct**; declared pointee type and source-role namespace qualified","count":"explicit requested slots checked against the actual allocation before allocating/reading","result":"ordered source-local block indices with every null preserved","trace_span":"read.pointer_table"}],"raw_projection":{"syntax":"exact.dot.separated.SDNA.fields; numeric components select flattened array indices","pointer_traversal":"explicit; raw pointers remain typed saved addresses"},"limits":["No evaluated visibility, runtime defaults or render validation. External access is limited to explicit collection.graph expansion and narrow source-aware linked node-tree subtype lookup.","Semantic profile qualification is field-specific; inspect explain against the input."]});
    span.end("ok");
    report
}
fn canonical(field: &str) -> Option<&'static str> {
    match field {
        "render" | "scene.render" => Some("scene.render"),
        "view_layers" | "scene.view_layers" => Some("scene.view_layers"),
        "compositor" | "scene.compositor" => Some("scene.compositor"),
        "collection_graph" | "collection.graph" => Some("collection.graph"),
        "id_properties" | "id.properties" => Some("id.properties"),
        _ => None,
    }
}
fn unavailable(error: &Error, field: &str, record: &Record, type_name: &str) -> Json {
    json!({"status":"unavailable","kind":error.kind,"diagnostic_ref":crate::failure_receipt::reference_json(error.diagnostic.get()),"context":{"field":field,"block_index":record.block,"element":record.element,"message":error.message},"next_action":{"command":"explain","concept":canonical(field).unwrap_or(type_name),"arguments":["--file","<same input>","--json"],"guidance":"Inspect actual saved SDNA and the reported pointer/field site; use get/pointer for raw values. Do not infer missing values or repair bytes."}})
}
/// Select every saved struct element in physical order, preserving partial outcomes.
/// Input observation is checked around the whole batch; changed input discards it.
pub fn query(file: &BlendFile, type_name: &str, fields: &[String]) -> Result<Json> {
    query_with_libraries(file, None, type_name, fields, &LibraryMap::new())
}
/// CLI convenience: plan, capture each requested library once, then evaluate.
/// Only explicit collection graphs and the ruled node-tree subtype lookup open external files.
pub fn query_with_source(
    file: &BlendFile,
    source: &std::path::Path,
    type_name: &str,
    fields: &[String],
) -> Result<Json> {
    let mut owned = linked::DefaultLibraries::new();
    loop {
        let libraries = owned.views();
        let requests = library_requests(file, Some(source), type_name, fields, &libraries)?;
        if requests.is_empty() {
            let report = query_with_libraries(file, Some(source), type_name, fields, &libraries)?;
            owned.check()?;
            return Ok(report);
        }
        drop(libraries);
        owned.load(&requests);
    }
}
/// Phase 1: owned requests only. Drop store borrows, capture these paths, rebuild
/// the read-only map, and repeat until no requests remain (forwarding may chain).
/// Failed captures belong in the map too, preventing automatic replay.
pub fn library_requests(
    file: &BlendFile,
    source: Option<&std::path::Path>,
    type_name: &str,
    fields: &[String],
    libraries: &LibraryMap<'_>,
) -> Result<Vec<LibraryRequest>> {
    let linked_fields = fields
        .iter()
        .filter(|f| {
            matches!(
                canonical(f),
                Some("scene.compositor") | Some("collection.graph")
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    if linked_fields.is_empty() {
        return Ok(Vec::new());
    }
    let mut requests = Vec::new();
    query_impl(
        file,
        source,
        type_name,
        &linked_fields,
        libraries,
        Some(&mut requests),
    )?;
    Ok(requests)
}
/// Phase 2: evaluate against borrowed, caller-retained library snapshots.
/// Session owns lexical source latches and checks them around both phases.
pub fn query_with_libraries(
    file: &BlendFile,
    source: Option<&std::path::Path>,
    type_name: &str,
    fields: &[String],
    libraries: &LibraryMap<'_>,
) -> Result<Json> {
    query_impl(file, source, type_name, fields, libraries, None)
}
fn query_impl(
    file: &BlendFile,
    source: Option<&std::path::Path>,
    type_name: &str,
    fields: &[String],
    libraries: &LibraryMap<'_>,
    requests_out: Option<&mut Vec<LibraryRequest>>,
) -> Result<Json> {
    let mut span = crate::diagnostics::Span::new(
        "query.evaluate",
        json!({"type":type_name,"fields":fields,"catalog_revision":CATALOG_REVISION}),
    );
    let result = (|| {
        file.check_source()?;
        if type_name.is_empty() || fields.is_empty() || fields.iter().any(|f| f.is_empty()) {
            return Err(Error::new(
                "usage",
                "query requires a type and nonempty exact fields",
            ));
        }
        let mut unique = std::collections::HashSet::new();
        if fields.iter().any(|f| !unique.insert(f)) {
            return Err(Error::new("usage", "duplicate query fields"));
        }
        if !file
            .schema()
            .types
            .iter()
            .any(|ty| ty == type_name.as_bytes())
            && !crate::id_codes::TYPES.iter().any(|(_, types)| {
                types.iter().any(|ty| {
                    crate::id_codes::canonical(ty)
                        == crate::id_codes::canonical(type_name.as_bytes())
                })
            })
        {
            return Err(Error::new(
                "unsupported_type",
                format!(
                    "SDNA type {type_name} not present; use explain {type_name} --file INPUT --json"
                ),
            ));
        }
        let mut rows = Vec::new();
        let mut errors = Vec::new();
        let mut linked_context = linked::Context::new(file, source, libraries);
        let mut collection_graph = collection_graph::Graph::new();
        let selection_span =
            crate::diagnostics::Span::new("query.select", json!({"type":type_name}));
        let mut selection = records(file, type_name)?;
        // Native linked-ID placeholders are saved as bare ID records, not full
        // records of the requested type. Select by their authoritative ID code.
        let id_codes = crate::id_codes::TYPES
            .iter()
            .filter(|(_, types)| {
                types.iter().any(|ty| {
                    crate::id_codes::canonical(ty)
                        == crate::id_codes::canonical(type_name.as_bytes())
                })
            })
            .map(|(code, _)| *code)
            .collect::<Vec<_>>();
        if type_name != "ID" && !id_codes.is_empty() {
            for record in records(file, "ID")? {
                if let Value::Bytes(name) = get(file, &record, "name")?
                    && id_codes.iter().any(|code| name.starts_with(code))
                {
                    selection.push(record);
                }
            }
            selection.sort_by_key(|r| (r.block, r.element));
        }
        selection_span.end("ok");
        for record in selection {
            let mut values = serde_json::Map::new();
            let name_result = get(file, &record, "id.name").or_else(|e| {
                if e.kind == "missing_field"
                    && file.schema().types
                        [file.schema().structs[file.blocks()[record.block].sdna_index].type_index]
                        == b"ID"
                {
                    get(file, &record, "name")
                } else {
                    Err(e)
                }
            });
            let identity = match name_result {
                Ok(Value::Bytes(name)) => {
                    let mut id = access::id_ref(file, &record)?;
                    id["name_hex"] = json!(hex(&name));
                    Some(id)
                }
                Err(e) if e.kind == "missing_field" => None,
                Err(e) => return Err(e),
                _ => return Err(Error::new("type", "ID name is not bytes")),
            };
            let linked = identity.as_ref().is_some_and(|id| !id["library"].is_null());
            for field in fields {
                let mut projection_span = crate::diagnostics::Span::new_lazy(
                    "query.project",
                    || json!({"field":field,"block_index":record.block,"element":record.element}),
                );
                let concept = canonical(field);
                let mut partial_value = None;
                let mut result = if concept == Some("collection.graph") {
                    match collection_graph.project(file, &record, source, &mut linked_context) {
                        Ok(projection) => {
                            let mut gaps = projection.errors;
                            if let Some(error) = gaps
                                .iter()
                                .find(|e| matches!(e.kind, "input_changed" | "source_changed"))
                            {
                                Err(error.clone())
                            } else if gaps.is_empty() {
                                Ok(projection.value)
                            } else {
                                for extra in gaps.iter().skip(1) {
                                    errors.push(json!({"kind":extra.kind,"block_index":record.block,"element":record.element,"field":field,"message":extra.message}));
                                }
                                partial_value = Some(projection.value);
                                Err(gaps.remove(0))
                            }
                        }
                        Err(error) => Err(error),
                    }
                } else if concept.is_some() && linked {
                    Err(Error::new(
                        "linked_reference_only",
                        "linked ID is a root-file stub; no external content was loaded",
                    ))
                } else if concept.is_some()
                    && type_name != "Scene"
                    && concept != Some("id.properties")
                {
                    Err(Error::new("type", "Scene concept requires --type Scene"))
                } else {
                    project(file, &record, concept, field)
                };
                if concept == Some("scene.compositor")
                    && let Ok(value) = &mut result
                {
                    let mut gaps = linked_context.enrich(value);
                    if let Some(error) = gaps
                        .iter()
                        .find(|e| matches!(e.kind, "input_changed" | "source_changed"))
                    {
                        result = Err(error.clone());
                    } else if !gaps.is_empty() {
                        partial_value = Some(value.clone());
                        result = Err(gaps.remove(0));
                    }
                }
                projection_span.record_result(&result);
                if let Err(error) = &result
                    && matches!(error.kind, "input_changed" | "source_changed")
                {
                    return Err(error.clone());
                }
                match result {
                    Ok(value) => {
                        let disk_paths = concept
                            .and_then(|c| entries().into_iter().find(|e| e["name"] == c))
                            .map(|e| e["disk_paths"].clone())
                            .unwrap_or_else(|| json!([format!("{type_name}.{field}")]));
                        let mut evidence = json!([{"concept":concept.unwrap_or(field),"support":"qualified","disk_paths":disk_paths}]);
                        if concept == Some("scene.compositor") {
                            evidence[0]["undefined_nodes"] =
                                json!(compositor::undefined_nodes(&value));
                        }
                        values.insert(
                            field.clone(),
                            json!({"status":"available","value":value,"evidence":evidence}),
                        );
                    }
                    Err(error) => {
                        let error = if linked
                            && concept != Some("collection.graph")
                            && error.kind == "missing_field"
                        {
                            Error::new(
                                "linked_reference_only",
                                "linked ID is a root-file stub; the requested body field was not loaded",
                            )
                        } else {
                            error
                        };
                        errors.push(json!({"kind":error.kind,"diagnostic_ref":crate::failure_receipt::reference_json(error.diagnostic.get()),"block_index":record.block,"element":record.element,"field":field,"message":error.message}));
                        let mut outcome = unavailable(&error, field, &record, type_name);
                        if let Some(value) = partial_value {
                            outcome["partial_value"] = value;
                        }
                        values.insert(field.clone(), outcome);
                    }
                }
            }
            rows.push(json!({"source_local_ref":{"block_index":record.block,"element":record.element},"id":identity,"content_status":if linked{"linked_reference"}else{"local"},"values":values}));
        }
        let storage_evidence = if fields.iter().any(|field| {
            matches!(
                canonical(field),
                Some("scene.render")
                    | Some("scene.view_layers")
                    | Some("scene.compositor")
                    | Some("id.properties")
            )
        }) {
            json!({"id_properties":storage::properties(file)?,"id_names":storage::identities(file)?})
        } else {
            json!({})
        };
        let graphs = if fields
            .iter()
            .any(|f| canonical(f) == Some("collection.graph"))
        {
            json!({"collection.graph":collection_graph.json()})
        } else {
            json!({})
        };
        let subtype_observations = linked_context
            .observations
            .iter()
            .filter(|v| v.get("reason").is_none() || v["reason"] == "linked_node_tree_subtype")
            .cloned()
            .collect::<Vec<_>>();
        linked_context.check()?;
        if let Some(out) = requests_out {
            *out = linked_context.requests();
        }
        let source_observation = observation(file)?;
        file.check_source()?;
        Ok(
            json!({"graphs":graphs,"linked_id_observations":linked_context.observations,"linked_subtype_observations":subtype_observations,"linked_subtype_diagnostics":linked_context.diagnostics,"storage_evidence":storage_evidence,"schema":"oxblend.query/1","catalog_revision":CATALOG_REVISION,"basis":"saved_controls_not_evaluated_results","source_observation":source_observation,"selection":{"type":type_name},"requested_coverage":{"complete":errors.is_empty()},"errors":errors,"rows":rows}),
        )
    })();
    if let Ok(report) = &result {
        span.data(json!({"row_count":report["rows"].as_array().map(Vec::len),"complete":report["requested_coverage"]["complete"]}));
    }
    span.result(result)
}
fn observation(file: &BlendFile) -> Result<Json> {
    let records = records(file, "FileGlobal")?;
    let mut errors = Vec::new();
    let subversion = match records.first() {
        Some(record) => match access::int(file, record, "subversion") {
            Ok(value) => Some(value),
            Err(error) if error.kind == "missing_field" => {
                errors.push(json!({"kind":error.kind,"message":error.message,"block_index":record.block,"element":record.element,"field":"FileGlobal.subversion","status":"unavailable"}));
                None
            }
            Err(error) => return Err(error),
        },
        None => {
            errors.push(json!({"kind":"missing_field","field":"FileGlobal.subversion","status":"unavailable","message":"No saved FileGlobal record; optional subversion observation unavailable"}));
            None
        }
    };
    Ok(
        json!({"input_version":file.header().version,"identity":{"scope":"retained-reader-observation","decoded_bytes":file.decompressed_len()},"subversion":subversion,"complete":errors.is_empty(),"errors":errors}),
    )
}
fn subversion(file: &BlendFile) -> Result<Option<i64>> {
    match records(file, "FileGlobal")?
        .first()
        .map(|r| access::int(file, r, "subversion"))
        .transpose()
    {
        Err(e) if e.kind == "missing_field" => Ok(None),
        other => other,
    }
}
fn project(file: &BlendFile, record: &Record, concept: Option<&str>, field: &str) -> Result<Json> {
    if concept.is_some() && (file.header().version != 502 || subversion(file)? != Some(44)) {
        return Err(Error::new(
            "unverified_semantics",
            "semantic profile requires saved Blender 5.2 subversion 44; raw fields remain queryable",
        ));
    }
    match concept {
        Some("scene.render") => render::project(file, record),
        Some("scene.view_layers") => view_layers::project(file, record),
        Some("scene.compositor") => compositor::project(file, record),
        Some("id.properties") => {
            let ty = &file.schema().types
                [file.schema().structs[file.blocks()[record.block].sdna_index].type_index];
            match ty.as_slice() {
                b"Scene" => props::scene(file, record),
                b"ViewLayer" => {
                    props::decode_owner(file, record, "id_properties", Some("system_properties"))
                }
                b"ID" => props::decode_owner(file, record, "properties", Some("system_properties")),
                _ => {
                    props::decode_owner(file, record, "id.properties", Some("id.system_properties"))
                }
            }
        }
        Some(_) => Err(Error::new(
            "unverified_semantics",
            "semantic projection not yet qualified for this representation; raw exact SDNA fields remain available",
        )),
        None => get(file, record, field).map(|v| v.json()),
    }
}
/// Explain an authored concept or actual SDNA type with file-aware saved paths.
pub fn explain(file: Option<&BlendFile>, concept: &str) -> Result<Json> {
    let span = crate::diagnostics::Span::new(
        "explain.evaluate",
        json!({"concept":concept,"file_aware":file.is_some()}),
    );
    let result = (|| {
        if let Some(file) = file {
            file.check_source()?;
        }
        let key = canonical(concept).unwrap_or(concept);
        let entry = entries().into_iter().find(|e| e["name"] == key);
        let mut structs = Vec::new();
        if let Some(file) = file {
            for structure in &file.schema().structs {
                let ty = &file.schema().types[structure.type_index];
                if ty == concept.as_bytes()
                    || entry.as_ref().is_some_and(|e| {
                        e["disk_paths"].as_array().is_some_and(|paths| {
                            paths.iter().any(|p| {
                                p.as_str().is_some_and(|p| {
                                    p.split('.')
                                        .next()
                                        .is_some_and(|head| head.as_bytes() == ty)
                                })
                            })
                        })
                    })
                {
                    let fields=structure.fields.iter().map(|f|json!({"name":std::str::from_utf8(&f.name).ok(),"name_hex":hex(&f.name),"declaration_hex":hex(&f.full_name),"type":std::str::from_utf8(&file.schema().types[f.type_index]).ok(),"offset":f.offset,"element_size":f.element_size,"array_len":f.array_len,"pointer":f.pointer})).collect::<Vec<_>>();
                    structs.push(json!({"type":std::str::from_utf8(ty).ok(),"type_hex":hex(ty),"fields":fields}));
                }
            }
        }
        if entry.is_none() && structs.is_empty() {
            return Err(Error::new(
                "unknown_concept",
                format!(
                    "{concept}: no exact catalog concept or file SDNA type; run catalog --json, or inspect INPUT for exact type names"
                ),
            ));
        }
        if let Some(file) = file {
            file.check_source()?;
        }
        Ok(
            json!({"schema":"oxblend.explain/1","catalog_revision":CATALOG_REVISION,"concept":key,"entry":entry,"file_aware":file.is_some(),"sdna":structs,"profile":{"header_version":file.map(|f|f.header().version),"qualification":"field-specific; raw SDNA presence does not certify semantic equivalence"},"next_action":{"query":"query INPUT --type Scene --fields render,view_layers,compositor,id_properties","raw":"query INPUT --type TYPE --fields exact.saved.field","pointers":"pointer INPUT BLOCK PATH_JSON [ELEMENT]","limits":"Saved controls only. No runtime defaults or evaluated visibility; external content only for explicitly requested collection graphs and narrow subtype lookup."}}),
        )
    })();
    span.result(result)
}
