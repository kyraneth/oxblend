//! Explicit fixed-array path rewriting. no generic setters or pointer writes.
use crate::checked::add;
/// Stable in 1.x because this policy is reachable from stable pack options/reports.
pub use crate::edit_output::Durability;
use crate::edit_output::{AtomicOutput, io, refuse_existing, sync_end};
use crate::value::{PathStep, Value, hex};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::fs::File;
use std::io::{Read, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
/// Stable options type in 1.x; construct with `new` or `default`.
pub struct EditOptions {
    pub durability: Durability,
}
impl EditOptions {
    pub fn new(durability: Durability) -> Self {
        Self { durability }
    }
}
#[derive(Debug, Clone)]
pub struct PathRewrite {
    pub block_index: usize,
    pub element: usize,
    pub steps: Vec<PathStep>,
    pub expected_path: Vec<u8>,
    pub replacement_path: Vec<u8>,
}
#[derive(Debug, Clone)]
/// Stable in 1.x because this record is reachable from stable pack reports.
#[non_exhaustive]
pub struct EditFailure {
    pub edit_index: Option<usize>,
    pub kind: &'static str,
    pub message: String,
}
#[derive(Debug, Clone)]
/// Stable in 1.x because this record is reachable from stable pack reports.
#[non_exhaustive]
pub struct EditRange {
    pub edit_index: usize,
    pub block_index: usize,
    pub element: usize,
    pub steps: Vec<PathStep>,
    pub offset: usize,
    pub capacity: usize,
    pub before_path: Vec<u8>,
    pub after_path: Vec<u8>,
    pub write_length: usize,
}
#[derive(Debug)]
#[non_exhaustive]
/// Stable in 1.x because this report is reachable from stable pack reports.
pub struct EditReport {
    pub input_path: Vec<u8>,
    pub output_path: Vec<u8>,
    pub edits: Vec<EditRange>,
    pub errors: Vec<EditFailure>,
    pub container_family: &'static str,
    pub input_physical_length: u64,
    pub output_physical_length: Option<u64>,
    pub input_decoded_length: usize,
    pub output_decoded_length: Option<usize>,
    pub container_reencoded: bool,
    /// Full upstream decoder pass before publishing an edited compressed output.
    /// Zstd verifies present frame checksums; absent flags encode no checksum.
    pub output_decoder_verified: bool,
    pub output_written: bool,
    pub durability: Durability,
    pub durability_completed: bool,
    pub complete: bool,
}
impl EditReport {
    pub fn json(&self) -> Json {
        crate::value::with_text(
            json!({"schema":1,"operation":"rewrite_paths","input_path_hex":hex(&self.input_path),"output_path_hex":hex(&self.output_path),"container_family":self.container_family,"input_physical_length":self.input_physical_length,"output_physical_length":self.output_physical_length,"input_decoded_length":self.input_decoded_length,"output_decoded_length":self.output_decoded_length,"container_reencoded":self.container_reencoded,"output_decoder_verified":self.output_decoder_verified,"output_written":self.output_written,"durability":self.durability.as_str(),"durability_mechanism":self.durability.mechanism(),"syncfs_shared_filesystem":self.durability == Durability::End,"durability_completed":self.durability_completed,"complete":self.complete,
        "edits":self.edits.iter().map(|e|json!({"edit_index":e.edit_index,"block_index":e.block_index,"element":e.element,"steps":e.steps.iter().map(|s|match s {PathStep::Field(v)=>json!({"field_hex":hex(v)}),PathStep::Index(v)=>json!({"index":v})}).collect::<Vec<_>>(),"offset":e.offset,"capacity":e.capacity,"write_length":e.write_length,"before_path_hex":hex(&e.before_path),"after_path_hex":hex(&e.after_path)})).collect::<Vec<_>>(),
        "errors":self.errors.iter().map(|e|json!({"edit_index":e.edit_index,"kind":e.kind,"message":e.message})).collect::<Vec<_>>()}),
        )
    }
    fn fail(&mut self, index: Option<usize>, mut e: Error) {
        e.message = format!(
            "rewrite input {} output {}: {}",
            crate::value::display_bytes(&self.input_path),
            crate::value::display_bytes(&self.output_path),
            e.message
        );
        self.errors.push(EditFailure {
            edit_index: index,
            kind: e.kind,
            message: e.message,
        });
    }
}
// Literal stored paths only. Derived PointCache.name, GN char* strings, ID.name
// and arbitrary same-named fields are deliberately NOT in this registry.
fn literal_path(parent: &[u8], name: &[u8]) -> bool {
    matches!(
        (parent, name),
        (b"Image", b"name")
            | (b"Library", b"name")
            | (b"Sound", b"name")
            | (b"bSound", b"name")
            | (b"VFont", b"name")
            | (b"MovieClip", b"name")
            | (b"CacheFile", b"filepath")
            | (b"Volume", b"filepath")
            | (b"CustomDataExternal", b"filename")
            | (b"NodeShaderTexIES", b"filepath")
            | (b"MeshCacheModifierData", b"filepath")
            | (b"OceanModifierData", b"cachepath")
            | (b"PointCache", b"path")
            | (b"FluidDomainSettings", b"cache_directory")
            | (b"FluidsimSettings", b"surfdataPath")
            | (b"Strip", b"dir")
            | (b"StripData", b"dir")
    )
}
// A String socket is not intrinsically a path. Admit only the native static
// Path input of a registered file-import node, in this storage's physical owner.
fn import_path(reader: &BlendFile, edit: &PathRewrite) -> Result<bool> {
    fn steps(names: &[&str]) -> Vec<PathStep> {
        names
            .iter()
            .map(|s| PathStep::Field(s.as_bytes().to_vec()))
            .collect()
    }
    // Native import defaults are element-zero records. The shared resolver
    // rejects interior/dangling pointers rather than inventing virtual records.
    if edit.element != 0 {
        return Ok(false);
    }
    let ix = reader.owner_index()?;
    let Some(owner) = ix.owner(edit.block_index) else {
        return Ok(false);
    };
    for (b, block) in reader.blocks().iter().enumerate() {
        if ix.owner(b) != Some(owner) || &block.code == b"DNA1" {
            continue;
        }
        if reader.dna_type_id(b)? != b"bNode" {
            continue;
        }
        for e in 0..block.count {
            let Value::Bytes(id) = reader.get(b, e, &steps(&["idname"]), true)? else {
                continue;
            };
            if !crate::trace::geometry_import(&id) {
                continue;
            }
            let mut socket = reader.get_pointer(b, e, &steps(&["inputs", "first"]))?;
            let mut seen = std::collections::HashSet::new();
            while let Some(s) = socket {
                if !seen.insert(s) {
                    return Err(Error::new("path_context", "cyclic import input list"));
                }
                if reader.get(s, 0, &steps(&["name"]), true)? == Value::Bytes(b"Path".to_vec())
                    && matches!(
                        reader.get(s, 0, &steps(&["link"]), true)?,
                        Value::Pointer(0)
                    )
                    && reader.get_pointer(s, 0, &steps(&["default_value"]))?
                        == Some(edit.block_index)
                {
                    return Ok(true);
                }
                socket = reader.get_pointer(s, 0, &steps(&["next"]))?;
            }
        }
    }
    Ok(false)
}
fn validate(reader: &BlendFile, edit: &PathRewrite, index: usize) -> Result<EditRange> {
    let location = reader.locate_field(edit.block_index, edit.element, &edit.steps)?;
    let field = location.field;
    if location.parent_type == b"ID" && field.name == b"name" {
        return Err(Error::new(
            "unsupported_edit",
            "ID name fields are not editable; only whitelisted filepath fields are",
        ));
    }
    if field.pointer {
        return Err(Error::new(
            "pointer_write",
            "pointer-field rewriting is forbidden",
        ));
    }
    if location.indexed
        || reader.schema().types[field.type_index] != b"char"
        || field.element_size != 1
        || field.array_len < 2
        || !field.full_name.contains(&b'[')
    {
        return Err(Error::new(
            "unsupported_edit",
            "leaf must be a whole fixed char array",
        ));
    }
    if !(literal_path(location.parent_type, &field.name)
        || location.parent_type == b"bNodeSocketValueString"
            && field.name == b"value"
            && import_path(reader, edit)?)
    {
        return Err(Error::new(
            "unsupported_edit",
            format!(
                "not a whitelisted literal path: {}.{}",
                crate::value::display_bytes(location.parent_type),
                crate::value::display_bytes(&field.name)
            ),
        ));
    }
    if edit.replacement_path.is_empty() {
        return Err(Error::new(
            "empty_clear",
            "clearing a reference is not this operation",
        ));
    }
    if edit.replacement_path.contains(&0) || edit.expected_path.contains(&0) {
        return Err(Error::new(
            "path_nul",
            "path values must exclude embedded NUL",
        ));
    }
    let length = add(edit.replacement_path.len(), 1)?;
    if length > field.array_len {
        return Err(Error::new(
            "path_overflow",
            "replacement plus NUL exceeds fixed capacity; never truncate",
        ));
    }
    let bytes = reader.field_bytes(edit.block_index, location.offset, field.array_len)?;
    let end = bytes.iter().position(|&v| v == 0).ok_or_else(|| {
        Error::new(
            "path_termination",
            "old path has no terminator within fixed field",
        )
    })?;
    if bytes[..end] != edit.expected_path {
        return Err(Error::new(
            "expected_path",
            "stored path does not match expected old bytes",
        ));
    }
    let block = reader.block(edit.block_index)?;
    Ok(EditRange {
        edit_index: index,
        block_index: edit.block_index,
        element: edit.element,
        steps: edit.steps.clone(),
        offset: add(block.data_offset, location.offset)?,
        capacity: field.array_len,
        before_path: bytes[..end].to_vec(),
        after_path: edit.replacement_path.clone(),
        write_length: if bytes[..end] == edit.replacement_path {
            0
        } else {
            length
        },
    })
}
pub(crate) fn prepared(
    reader: &BlendFile,
    requested: &[PathRewrite],
) -> (Vec<EditRange>, Vec<EditFailure>) {
    let mut edits: Vec<EditRange> = Vec::new();
    let mut errors = Vec::new();
    for (index, edit) in requested.iter().enumerate() {
        match validate(reader, edit, index) {
            Ok(range) => {
                let conflict = edits.iter().find(|prior| {
                    range.offset < prior.offset.saturating_add(prior.capacity)
                        && prior.offset < range.offset.saturating_add(range.capacity)
                });
                if let Some(prior) = conflict {
                    if prior.offset == range.offset
                        && prior.capacity == range.capacity
                        && prior.after_path == range.after_path
                        && prior.before_path == range.before_path
                    {
                        continue;
                    }
                    errors.push(EditFailure {
                        edit_index: Some(index),
                        kind: "edit_conflict",
                        message: "overlapping or conflicting field edits".into(),
                    });
                } else {
                    edits.push(range);
                }
            }
            Err(e) => errors.push(EditFailure {
                edit_index: Some(index),
                kind: e.kind,
                message: e.message,
            }),
        }
    }
    edits.sort_by_key(|e| e.offset);
    (edits, errors)
}
pub(crate) struct Span {
    pub offset: usize,
    pub bytes: Vec<u8>,
}
pub(crate) fn path_spans(edits: &[EditRange]) -> Vec<Span> {
    edits
        .iter()
        .filter(|e| e.write_length != 0)
        .map(|e| {
            let mut bytes = e.after_path.clone();
            bytes.push(0);
            Span {
                offset: e.offset,
                bytes,
            }
        })
        .collect()
}
fn copy_exact(reader: &mut dyn Read, writer: &mut dyn Write, length: usize) -> Result<()> {
    let length = u64::try_from(length).map_err(|_| Error::new("overflow", "copy length"))?;
    let copied = std::io::copy(&mut reader.take(length), writer).map_err(io)?;
    if copied != length {
        return Err(Error::new(
            "truncated",
            "source changed or short during output copy",
        ));
    }
    Ok(())
}
pub(crate) fn write_spans(
    reader: &BlendFile,
    edits: &[Span],
    output: &mut dyn Write,
) -> Result<()> {
    reader.data.scan(|source| {
        source.seek(SeekFrom::Start(0)).map_err(io)?;
        let mut position = 0;
        for edit in edits {
            copy_exact(
                source,
                output,
                edit.offset
                    .checked_sub(position)
                    .ok_or_else(|| Error::new("edit_conflict", "unordered edits"))?,
            )?;
            source
                .seek(SeekFrom::Current(
                    i64::try_from(edit.bytes.len())
                        .map_err(|_| Error::new("overflow", "write range"))?,
                ))
                .map_err(io)?;
            output.write_all(&edit.bytes).map_err(io)?;
            position = add(edit.offset, edit.bytes.len())?;
        }
        copy_exact(
            source,
            output,
            reader
                .decompressed_len()
                .checked_sub(position)
                .ok_or_else(|| Error::new("bounds", "edit past body"))?,
        )
    })
}
/// Inputs must remain stable for the call; never modifies an original descriptor.
/// No output name is created until full validation and successful private write.
pub fn rewrite_paths(
    input: &Path,
    new_output: &Path,
    requested: &[PathRewrite],
    options: &EditOptions,
) -> Result<EditReport> {
    let source = File::open(input).map_err(io)?;
    let metadata = source.metadata().map_err(io)?;
    let reader = BlendFile::open(input)?;
    rewrite_opened(
        input,
        new_output,
        requested,
        options,
        &crate::physical::Physical::new(&source),
        &metadata,
        &reader,
        false,
    )
}

/// Pack passes its already parsed snapshot through the SAME SDNA validation,
/// expected-old checks, container verification and publication engine.
#[allow(clippy::too_many_arguments)] // one existing engine, explicit pack container policy
pub(crate) fn rewrite_opened(
    input: &Path,
    new_output: &Path,
    requested: &[PathRewrite],
    options: &EditOptions,
    source: &crate::physical::Physical<'_>,
    metadata: &std::fs::Metadata,
    reader: &BlendFile,
    compress: bool,
) -> Result<EditReport> {
    let family = source.family().map_err(io)?;
    let (edits, errors) = prepared(reader, requested);
    let mut report = EditReport {
        input_path: input.as_os_str().as_bytes().to_vec(),
        output_path: new_output.as_os_str().as_bytes().to_vec(),
        edits,
        errors,
        container_family: if compress { "zstd" } else { family },
        input_physical_length: metadata.len(),
        output_physical_length: None,
        input_decoded_length: reader.decompressed_len(),
        output_decoded_length: None,
        container_reencoded: false,
        output_decoder_verified: false,
        output_written: false,
        durability: options.durability,
        durability_completed: false,
        complete: false,
    };
    if let Err(e) = refuse_existing(metadata, new_output) {
        report.fail(None, e);
    }
    if !report.errors.is_empty() {
        return Ok(report);
    }
    let modified = report.edits.iter().any(|e| e.write_length != 0);
    let inspected;
    let admission = if let Some(admission) = source.captured_admission(input) {
        admission
    } else if family == "zstd" {
        inspected = crate::edit_container::inspect(source, reader);
        inspected
            .as_ref()
            .map(|table| table.as_ref())
            .map_err(Clone::clone)
    } else {
        Ok(None)
    };
    let seek = match admission {
        Ok(table) => table,
        Err(e) => {
            report.fail(None, e);
            return Ok(report);
        }
    };
    let result = (|| {
        let mut staging = AtomicOutput::create(metadata, new_output)?;
        if compress {
            crate::edit_container::compress_seek(source, reader, &report.edits, &mut staging.file)?;
        } else if modified {
            crate::edit_container::emit(
                source,
                reader,
                family,
                seek,
                &report.edits,
                &mut staging.file,
            )?;
        } else {
            source.copy_whole(input, metadata, &mut staging.file)?;
        }
        if compress || (modified && family != "raw") {
            crate::edit_decode::verify_output(
                &staging.file,
                report.container_family,
                reader.decompressed_len(),
            )?;
            report.output_decoder_verified = true;
        }
        let length = staging.file.metadata().map_err(io)?.len();
        // After output verification: guard both the opened inode and path,
        // including ctime, immediately before the no-replace publication.
        source.check_before_publish(input, metadata)?;
        let publication = staging.publish(options.durability);
        report.durability_completed = staging.durability_completed;
        if staging.published {
            report.output_physical_length = Some(length);
            report.output_decoded_length = Some(reader.decompressed_len());
            report.output_written = true;
            report.container_reencoded = compress || (modified && family != "raw");
        }
        publication?;
        if options.durability == Durability::End {
            let parent = new_output
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            sync_end(parent)?;
            report.durability_completed = true;
        }
        Ok(length)
    })();
    match result {
        Ok(length) => {
            report.output_physical_length = Some(length);
            report.output_decoded_length = Some(reader.decompressed_len());
            report.output_written = true;
            report.container_reencoded = compress || (modified && family != "raw");
            report.complete = true;
        }
        Err(e) => report.fail(None, e),
    }
    Ok(report)
}
