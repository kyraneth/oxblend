//! Same-schema partial files. No address remapping or implicit ID dependency import.
use crate::edit::{Durability, EditOptions};
use crate::edit_output::{AtomicOutput, io, sync_end};
use crate::file_ops::{graph, pointer_layout};
use crate::{BlendFile, Error, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::Write;
use std::path::Path;

/// Exact saved ID names and explicit pointer-field clear policies.
#[derive(Debug, Default)]
pub struct ExtractOptions {
    pub ids: Vec<String>,
    /// Physical indices from this exact source; disambiguates duplicate names.
    pub blocks: Vec<usize>,
    pub clear: BTreeSet<String>,
    pub edit: EditOptions,
}
fn fail(message: impl Into<String>) -> Error {
    Error::new("extract_refusal", message)
}
fn key(p: &crate::file_ops::PointerLayout) -> String {
    format!(
        "{}.{}",
        String::from_utf8_lossy(&p.parent_type),
        String::from_utf8_lossy(&p.field_name)
    )
}
fn saved_id(f: &BlendFile, b: usize) -> Option<Vec<u8>> {
    let s = f.schema().structs.get(f.block(b).ok()?.sdna_index)?;
    if s.fields.first()?.name != b"id" {
        return None;
    }
    let steps = ["id", "name"].map(|s| crate::value::PathStep::Field(s.as_bytes().to_vec()));
    match f.get(b, 0, &steps, true).ok()? {
        crate::value::Value::Bytes(bytes) => Some(bytes),
        _ => None,
    }
}
fn id(f: &BlendFile, b: usize) -> Option<String> {
    String::from_utf8(saved_id(f, b)?).ok()
}
fn parent_text(
    f: &BlendFile,
    b: usize,
    p: &crate::file_ops::PointerLayout,
    name: &[u8],
) -> Result<String> {
    let field = f
        .schema()
        .structs
        .iter()
        .find(|s| f.schema().types[s.type_index] == p.parent_type)
        .and_then(|s| s.fields.iter().find(|q| q.name == name))
        .ok_or_else(|| fail("parent field absent"))?;
    let payload = f.payload(b)?;
    let start = crate::checked::add(p.parent_offset, field.offset)?;
    let end = crate::checked::add(start, field.size)?;
    let bytes = payload
        .get(start..end)
        .ok_or_else(|| fail("parent field out of bounds"))?;
    std::str::from_utf8(bytes.split(|b| *b == 0).next().unwrap_or_default())
        .map(str::to_owned)
        .map_err(|_| fail("parent field is not UTF-8"))
}
fn cross_owner(f: &BlendFile, b: usize, t: usize, roots: &BTreeSet<usize>) -> Result<bool> {
    if f.block(t)?.code != *b"DATA" {
        return Ok(false);
    }
    let index = f.owner_index()?;
    Ok(index.owner(f, t) != index.owner(f, b)
        && !index
            .owner(f, t)
            .is_some_and(|owner| roots.contains(&owner)))
}
fn policy(
    f: &BlendFile,
    b: usize,
    p: &crate::file_ops::PointerLayout,
    rules: &BTreeSet<String>,
) -> Result<bool> {
    if rules.contains(&key(p)) {
        return Ok(true);
    }
    // RNA's scene property is serialized as bNode.id, not a Scene* member.
    Ok(rules.contains("CompositorNodeRLayers.scene")
        && p.parent_type == b"bNode"
        && p.field_name == b"id"
        && parent_text(f, b, p, b"idname")? == "CompositorNodeRLayers")
}
/// Writes a new uncompressed file, preserving source header, DNA and old addresses.
/// Refusals return a complete report without creating an output; IO errors are typed errors.
pub fn extract(input: &Path, output: &Path, options: &ExtractOptions) -> Result<Value> {
    let span =
        crate::diagnostics::Span::new("extract.operation", json!({"path":input.to_string_lossy()}));
    span.result(extract_inner(input, output, options, || {}))
}
fn extract_inner(
    input: &Path,
    output: &Path,
    options: &ExtractOptions,
    before_publish: impl FnOnce(),
) -> Result<Value> {
    let metadata = input.metadata().map_err(io)?;
    crate::edit_output::refuse_existing(&metadata, output)?;
    let signature = crate::fetch::capture_signature(&metadata);
    let check = || -> Result<()> {
        if crate::fetch::capture_signature(&input.metadata().map_err(io)?) != signature {
            return Err(Error::new("input_changed", "source changed during extract"));
        }
        Ok(())
    };
    let reader = BlendFile::open(input)?;
    let bytes = reader.read_decompressed_bytes()?;
    check()?;
    let f = BlendFile::from_bytes(bytes)?;
    let select =
        crate::diagnostics::Span::new("extract.select", json!({"requested_ids":options.ids}));
    let mut selected = BTreeSet::new();
    let mut refusals = Vec::new();
    for name in &options.ids {
        let matches = (0..f.blocks().len())
            .filter(|&b| id(&f, b).as_ref() == Some(name))
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            refusals.push(json!({"kind":"extract_refusal","cause":"selection","name":name,"matches":matches,"message":"exactly one saved ID required"}));
        } else {
            selected.insert(matches[0]);
        }
    }
    for &b in &options.blocks {
        if saved_id(&f, b).is_none() {
            refusals.push(json!({"kind":"extract_refusal","cause":"selection","block":b,"message":"selected block lacks a readable saved ID name"}));
        } else {
            selected.insert(b);
        }
    }
    if options.ids.is_empty() && options.blocks.is_empty() {
        return Err(Error::new("usage", "extract requires --id or --block"));
    }
    for &b in &selected {
        if graph::uint(&f, b, &["id", "lib"])? != 0 {
            refusals.push(json!({"kind":"extract_refusal","block":b,"message":"linked-ID detachment is not qualified"}));
        }
        if f.block(b)?.code == *b"DATA" {
            refusals.push(json!({"kind":"extract_refusal","block":b,"message":"embedded ID promotion is not qualified; select a standalone ID"}));
        }
    }
    select.result(Ok(()))?;
    let mut valid = BTreeSet::from(["CompositorNodeRLayers.scene".to_owned()]);
    for s in &f.schema().structs {
        for field in &s.fields {
            if field.pointer {
                valid.insert(format!(
                    "{}.{}",
                    String::from_utf8_lossy(&f.schema().types[s.type_index]),
                    String::from_utf8_lossy(&field.name)
                ));
            }
        }
    }
    for rule in &options.clear {
        if !valid.contains(rule) {
            return Err(fail(format!("unknown pointer policy {rule}")));
        }
    }
    let roots = selected.clone();
    let mut queue = VecDeque::from_iter(selected.iter().copied());
    let mut edits = BTreeMap::<usize, BTreeSet<usize>>::new();
    let mut policies = Vec::new();
    let mut sanitation = Vec::new();
    let mut fixups = Vec::new();
    let mut traverse = crate::diagnostics::Span::new("extract.closure", json!({}));
    while let Some(b) = queue.pop_front() {
        let Some(layout) = pointer_layout(&f, b)? else {
            continue;
        };
        let payload = f.payload(b)?;
        for p in layout {
            let address = crate::checked::number(
                &payload,
                p.offset,
                f.header().pointer_size,
                f.header().little_endian,
            )?;
            if address == 0 {
                continue;
            }
            let site = json!({"block":b,"struct_field":key(&p),"field":p.path,"offset":p.offset,"target_address":address});
            let structural =
                p.parent_type == b"ID" && matches!(p.field_name.as_slice(), b"next" | b"prev");
            let discard = graph::discarded(&f, b, &p)? || discard_502(&f, b, &p, address)?;
            if structural || discard || policy(&f, b, &p, &options.clear)? {
                edits.entry(b).or_default().insert(p.offset);
                if structural {
                    fixups.push(site);
                } else if discard {
                    sanitation.push(site);
                } else {
                    let mut site = site;
                    site["rule"] = json!(if options.clear.contains(&key(&p)) {
                        key(&p)
                    } else {
                        "CompositorNodeRLayers.scene".to_owned()
                    });
                    policies.push(site);
                }
                continue;
            }
            match targets(&f, b, &p, address) {
                Ok(targets) => {
                    for t in targets {
                        if cross_owner(&f, b, t, &roots)? {
                            refusals.push(json!({"kind":"extract_refusal","cause":"external_owner","site":site,"target_block":t,"target_owner":f.owner_index()?.owner(&f, t),"message":"cross-owner DATA requires explicit selection of its owning ID"}));
                        } else if saved_id(&f, t).is_some()
                            && f.block(t)?.code != *b"DATA"
                            && !roots.contains(&t)
                        {
                            refusals.push(json!({"kind":"extract_refusal","cause":"external_id","site":site,"target_block":t,"target_id":id(&f,t),"target_id_hex":saved_id(&f,t).map(|b|crate::value::hex(&b)),"message":"ID outside selection requires explicit clear"}));
                        } else if selected.insert(t) {
                            queue.push_back(t);
                        }
                    }
                }
                Err(e) => refusals.push(json!({"kind":e.kind,"site":site,"message":e.message})),
            }
        }
    }
    if traverse.active() {
        traverse.data(json!({"blocks":selected.len(),"explicit_clears":policies.len(),"runtime_resets":sanitation.len(),"structural_fixups":fixups.len(),"refusals":refusals.len()}));
    }
    traverse.result(Ok(()))?;
    let unused = options
        .clear
        .iter()
        .filter(|rule| {
            !policies
                .iter()
                .any(|site| site["rule"].as_str() == Some(rule.as_str()))
        })
        .collect::<Vec<_>>();
    let mut report = json!({"schema":1,"requested_clear_rules":options.clear,"unused_clear_rules":unused,"complete":false,"selected_ids":roots.iter().map(|&b|json!({"block":b,"name":id(&f,b),"name_hex":saved_id(&f,b).map(|bytes|crate::value::hex(&bytes))})).collect::<Vec<_>>(),"selected_blocks":selected.len(),"source_blocks":f.blocks().len(),"applied_policies":policies,"runtime_sanitation":sanitation,"structural_fixups":fixups,"refusals":refusals,"output_written":false,"durability_requested":options.edit.durability.as_str(),"durability_completed":false,"errors":[]});
    if !refusals.is_empty() {
        return Ok(report);
    }
    // Black-box libraries.write (5.2.1): GLOB, IDs/DATA, DNA1, ENDB;
    // standalone node libraries contain neither REND nor TEST. GLOB's current
    // context pointers must not import editor scenes/screens into this file.
    let mut output_bytes = f.serialized_header()?;
    for (b, block) in f.blocks().iter().enumerate() {
        let global = block.code == *b"GLOB";
        if !selected.contains(&b) && !global && block.code != *b"DNA1" {
            continue;
        }
        let mut payload = f.payload(b)?.into_owned();
        if global {
            for p in pointer_layout(&f, b)?.unwrap_or_default() {
                let a = crate::checked::number(
                    &payload,
                    p.offset,
                    f.header().pointer_size,
                    f.header().little_endian,
                )?;
                if a != 0
                    && !(p.parent_type == b"FileGlobal"
                        && matches!(
                            p.field_name.as_slice(),
                            b"curscreen" | b"curscene" | b"cur_view_layer"
                        ))
                    && !policy(&f, b, &p, &options.clear)?
                {
                    return Err(fail(format!(
                        "block {b}, {} {}, target {a:#x}: unknown file-level pointer",
                        key(&p),
                        p.path
                    )));
                }
                if a != 0 {
                    report["structural_fixups"].as_array_mut().unwrap().push(json!({"block":b,"struct_field":key(&p),"field":p.path,"offset":p.offset,"target_address":a}));
                }
                payload[p.offset..p.offset + f.header().pointer_size].fill(0);
            }
        }
        for &offset in edits.get(&b).into_iter().flatten() {
            payload[offset..offset + f.header().pointer_size].fill(0);
        }
        encode(&f, block, &payload, &mut output_bytes)?;
    }
    let mut end = vec![
        0;
        if f.header().file_format == 1 {
            32
        } else {
            16 + f.header().pointer_size
        }
    ];
    end[..4].copy_from_slice(b"ENDB");
    output_bytes.extend(end);
    BlendFile::from_bytes(output_bytes.clone())?;
    check()?;
    let publish =
        crate::diagnostics::Span::new("extract.publish", json!({"bytes":output_bytes.len()}));
    let mut stage = AtomicOutput::create(&metadata, output)?;
    stage.file.write_all(&output_bytes).map_err(io)?;
    before_publish();
    check()?;
    let mut error = stage.publish(options.edit.durability).err();
    if error.is_none() && options.edit.durability == Durability::End {
        match sync_end(
            output
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        ) {
            Ok(()) => stage.durability_completed = true,
            Err(e) => error = Some(e),
        }
    }
    report["output_written"] = json!(stage.published);
    report["complete"] = json!(error.is_none());
    report["durability_completed"] = json!(stage.durability_completed);
    report["output_bytes"] = json!(output_bytes.len());
    report["errors"] = json!(
        error
            .iter()
            .map(|e| json!({"kind":e.kind,"message":e.message}))
            .collect::<Vec<_>>()
    );
    publish.result(Ok(report))
}

// Same-schema copies can follow a void* into a typed BHead without inventing
// a storage discriminator. Opaque targets still need the shared raw-role proof.
fn discard_502(
    f: &BlendFile,
    b: usize,
    p: &crate::file_ops::PointerLayout,
    address: u64,
) -> Result<bool> {
    if f.header().version != 502 || f.owner_index()?.local(f, b, address)?.is_some() {
        return Ok(false);
    }
    let field = f
        .schema()
        .structs
        .iter()
        .find(|s| f.schema().types[s.type_index] == p.parent_type)
        .and_then(|s| s.fields.iter().find(|field| field.name == p.field_name))
        .ok_or_else(|| fail("discard field absent"))?;
    if field.array_len != 1
        || field.size != f.header().pointer_size
        || field.full_name.contains(&b'(')
    {
        return Ok(false);
    }
    // Native 5.2.1 reopen/append proof: empty panel and menu cache handles
    // are nonzero without BHeads; clearing preserves interface/menu RNA dumps.
    if p.parent_type == b"bNodeSocketValueMenu"
        && p.field_name == b"enum_items"
        && p.declared_type == b"bke::RuntimeNodeEnumItems"
        && p.pointer_depth == 1
    {
        return Ok(true);
    }
    if p.parent_type == b"bNodeTreeInterfacePanel"
        && p.field_name == b"items_array"
        && p.declared_type == b"bNodeTreeInterfaceItem"
        && p.pointer_depth == 2
    {
        let count = f
            .schema()
            .structs
            .iter()
            .find(|s| f.schema().types[s.type_index] == p.parent_type)
            .and_then(|s| s.fields.iter().find(|field| field.name == b"items_num"))
            .ok_or_else(|| fail("interface count absent"))?;
        if count.pointer
            || count.array_len != 1
            || count.size != 4
            || f.schema().types[count.type_index] != b"int"
        {
            return Ok(false);
        }
        return Ok(crate::checked::number(
            &f.payload(b)?,
            p.parent_offset + count.offset,
            count.size,
            f.header().little_endian,
        )? == 0);
    }
    Ok(false)
}
fn targets(
    f: &BlendFile,
    b: usize,
    p: &crate::file_ops::PointerLayout,
    address: u64,
) -> Result<Vec<usize>> {
    if p.pointer_depth != 2 {
        return resolve(f, b, p, address).map(|v| v.into_iter().collect());
    }
    if p.declared_type == b"void" {
        return Err(fail("void pointer table has no element type"));
    }
    let table = f
        .owner_index()?
        .local(f, b, address)?
        .ok_or_else(|| fail("missing pointer table"))?;
    let block = f.block(table)?;
    if block.sdna_index != 0 || block.size % f.header().pointer_size != 0 {
        return Err(fail("pointer table must be an exact raw pointer array"));
    }
    let bytes = f.payload(table)?;
    let mut result = vec![table];
    let mut element = p.clone();
    element.pointer_depth = 1;
    for offset in (0..bytes.len()).step_by(f.header().pointer_size) {
        let value = crate::checked::number(
            &bytes,
            offset,
            f.header().pointer_size,
            f.header().little_endian,
        )?;
        if value != 0 {
            result.extend(resolve(f, b, &element, value)?);
        }
    }
    Ok(result)
}
fn resolve(
    f: &BlendFile,
    b: usize,
    p: &crate::file_ops::PointerLayout,
    address: u64,
) -> Result<Option<usize>> {
    if p.pointer_depth != 1 {
        return Err(fail("pointer table requires an element/extent profile"));
    }
    let field = f
        .schema()
        .structs
        .iter()
        .find(|s| f.schema().types[s.type_index] == p.parent_type)
        .and_then(|s| s.fields.iter().find(|q| q.name == p.field_name))
        .ok_or_else(|| fail("pointer field absent from source SDNA"))?;
    if field.full_name.contains(&b'(') {
        return Err(fail("callable pointer is not a serialized allocation edge"));
    }
    let target = if p.declared_type == b"void" {
        let index = f.owner_index()?;
        match crate::file_ops::roles::namespace(f, b, address, p) {
            Ok(namespace) => index.lookup(f, b, address, &p.path, namespace)?,
            // Copying unprofiled void storage needs only its concrete typed
            // BHead, not constructor semantics. Never substitute DATA for a
            // known ID namespace or for a failed/ambiguous known lookup.
            Err(e) if e.kind == "unprofiled_role" => index
                .local(f, b, address)?
                .ok_or_else(|| fail(format!("missing owner-local target {address:#x}")))?,
            Err(e) => return Err(e),
        }
    } else {
        f.owner_index()?.resolve(f, b, address, p)?
    };
    let block = f.block(target)?;
    if block.sdna_index == 0 {
        crate::file_ops::roles::write_qualify(f, b, p, target)?;
    } else {
        let si = f.struct_index(target)?;
        let st = &f.schema().structs[si];
        let length = f.schema().lengths[st.type_index];
        if length.checked_mul(block.count) != Some(block.size) {
            return Err(fail("target BHead/SDNA extent mismatch"));
        }
        if p.declared_type != b"void" {
            let mut ty = st.type_index;
            loop {
                if f.schema().types[ty] == p.declared_type {
                    break;
                }
                let nested = f
                    .schema()
                    .structs
                    .iter()
                    .find(|s| s.type_index == ty)
                    .and_then(|s| s.fields.first());
                match nested {
                    Some(field) if field.offset == 0 && !field.pointer && field.array_len == 1 => {
                        ty = field.type_index
                    }
                    _ => return Err(fail("declared pointer type differs from target SDNA")),
                }
            }
        }
    }
    Ok(Some(target))
}

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;

fn encode(f: &BlendFile, b: &crate::Block, data: &[u8], out: &mut Vec<u8>) -> Result<()> {
    let little = f.header().little_endian;
    // Widths and source values already passed the reader's checked BHead parser.
    out.extend_from_slice(&b.code);
    let mut put = |v: u64, n: usize| {
        let bytes = if little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        };
        if little {
            out.extend_from_slice(&bytes[..n]);
        } else {
            out.extend_from_slice(&bytes[8 - n..]);
        }
    };
    if f.header().file_format == 1 {
        put(b.sdna_index as u64, 4);
        put(b.old_address, 8);
        put(data.len() as u64, 8);
        put(b.count as u64, 8);
    } else {
        put(data.len() as u64, 4);
        put(b.old_address, f.header().pointer_size);
        put(b.sdna_index as u64, 4);
        put(b.count as u64, 4);
    }
    out.extend_from_slice(data);
    Ok(())
}
