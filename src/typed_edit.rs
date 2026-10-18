use crate::checked::{add, mul};
use crate::edit::{EditOptions, Span};
use crate::edit_output::{AtomicOutput, io, refuse_existing, sync_end};
use crate::input::{PackInputs, Snapshot};
use crate::value::{PathStep, hex};
use crate::{BlendFile, Error, Result};
use serde_json::{Value as Json, json};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub struct EditSnapshot {
    input: PathBuf,
    captured: Rc<Snapshot>,
    owners: Vec<Option<usize>>,
}
#[derive(Clone)]
pub struct FieldRef<'a> {
    snapshot: &'a EditSnapshot,
    block: usize,
    element: usize,
    steps: Vec<PathStep>,
    owner: usize,
}
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TypedValue {
    Signed(i64),
    Unsigned(u64),
    F32Bits(u32),
    F64Bits(u64),
    FixedBytes(Vec<u8>),
    CString(Vec<u8>),
    Array(Vec<TypedValue>),
}
pub struct TypedWrite<'a> {
    pub field: FieldRef<'a>,
    pub sdna_type: Vec<u8>,
    pub width: usize,
    pub expected: TypedValue,
    pub replacement: TypedValue,
}
pub struct BitWrite<'a> {
    pub field: FieldRef<'a>,
    pub sdna_type: Vec<u8>,
    pub width: usize,
    pub expected: TypedValue,
    pub clear: u64,
    pub set: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditMode {
    FormatOnly,
    Comp,
}
pub struct EditPlan<'a> {
    snapshot: &'a EditSnapshot,
    spans: Vec<Span>,
    declarations: Vec<Json>,
    mode: EditMode,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct EditReportV2 {
    pub complete: bool,
    pub output_written: bool,
    pub layout_verified: bool,
    pub container_verified: bool,
    pub native_verified: bool,
    pub semantic_verified: bool,
    pub durability_completed: bool,
    details: Json,
}
impl EditReportV2 {
    pub fn json(&self) -> Json {
        self.details.clone()
    }
}

pub(crate) fn owner_code(code: &[u8; 4]) -> bool {
    code == b"USER" || crate::id_codes::expected(code).is_some()
}
pub(crate) fn owner_runs(f: &BlendFile) -> Result<Vec<Option<usize>>> {
    let mut result = Vec::new();
    let mut owner = None;
    for (i, b) in f.blocks().iter().enumerate() {
        if b.code != *b"DATA" {
            owner = owner_code(&b.code).then_some(i);
        } else if owner.is_none() {
            return Err(Error::new(
                "stray_data_run",
                "DATA must immediately follow a qualified ID/USER run, not DNA1/control metadata",
            ));
        }
        result.push(owner);
    }
    Ok(result)
}
pub fn snapshot(input: &Path) -> Result<EditSnapshot> {
    let captured = PackInputs::new(&[input.to_path_buf()])?.get(input)?;
    let f = &captured.reader;
    if f.header().version < 502 || !f.header().little_endian {
        return Err(Error::new(
            "edit_version",
            "Typed editing requires little-endian Blender 5.2 or later; profiles are native-qualified on 5.2.1",
        ));
    }
    let owners = owner_runs(f)?;
    Ok(EditSnapshot {
        input: input.to_path_buf(),
        captured,
        owners,
    })
}
impl EditSnapshot {
    pub fn reader(&self) -> &BlendFile {
        &self.captured.reader
    }
    pub fn field(
        &self,
        block: usize,
        element: usize,
        steps: Vec<PathStep>,
    ) -> Result<FieldRef<'_>> {
        self.reader().locate_field(block, element, &steps)?;
        let owner = self
            .owners
            .get(block)
            .copied()
            .flatten()
            .ok_or_else(|| Error::new("field_owner", "metadata is not an editable owner"))?;
        Ok(FieldRef {
            snapshot: self,
            block,
            element,
            steps,
            owner,
        })
    }
}
fn integer_kind(kind: &[u8], width: usize) -> Result<bool> {
    let (signed, required) = match kind {
        b"char" | b"uchar" | b"unsigned char" | b"uint8_t" => (false, 1),
        b"int8_t" | b"signed char" => (true, 1),
        b"short" | b"int16_t" => (true, 2),
        b"ushort" | b"unsigned short" | b"uint16_t" => (false, 2),
        b"int" | b"int32_t" => (true, 4),
        b"uint" | b"unsigned int" | b"uint32_t" => (false, 4),
        b"int64_t" | b"long long" => (true, 8),
        b"uint64_t" | b"unsigned long long" => (false, 8),
        b"long" => (true, width),
        b"ulong" | b"unsigned long" => (false, width),
        _ => {
            return Err(Error::new(
                "typed_kind",
                "not a supported integer SDNA type",
            ));
        }
    };
    if width != required || !matches!(width, 1 | 2 | 4 | 8) {
        return Err(Error::new("typed_width", "SDNA primitive width mismatch"));
    }
    Ok(signed)
}
fn scalar(kind: &[u8], width: usize, v: &TypedValue) -> Result<Vec<u8>> {
    let bits = match (kind, v) {
        (b"float", TypedValue::F32Bits(v)) if width == 4 => return Ok(v.to_le_bytes().to_vec()),
        (b"double", TypedValue::F64Bits(v)) if width == 8 => return Ok(v.to_le_bytes().to_vec()),
        (_, TypedValue::Signed(v)) => {
            if !integer_kind(kind, width)? {
                return Err(Error::new("typed_kind", "signed value for unsigned field"));
            }
            let n = width * 8;
            if n < 64 && (*v < -(1i64 << (n - 1)) || *v > (1i64 << (n - 1)) - 1) {
                return Err(Error::new(
                    "typed_overflow",
                    "signed value does not fit; never truncate",
                ));
            }
            *v as u64
        }
        (_, TypedValue::Unsigned(v)) => {
            if integer_kind(kind, width)? {
                return Err(Error::new("typed_kind", "unsigned value for signed field"));
            }
            if width < 8 && *v >= (1u64 << (width * 8)) {
                return Err(Error::new(
                    "typed_overflow",
                    "unsigned value does not fit; never truncate",
                ));
            }
            *v
        }
        _ => {
            return Err(Error::new(
                "typed_kind",
                "value kind must exactly match primitive SDNA type",
            ));
        }
    };
    Ok(bits.to_le_bytes()[..width].to_vec())
}
fn encode(kind: &[u8], width: usize, count: usize, v: &TypedValue) -> Result<Vec<u8>> {
    match v {
        TypedValue::CString(bytes) => {
            if kind != b"char" || width != 1 || count < 2 {
                return Err(Error::new(
                    "typed_kind",
                    "CString needs a whole fixed char array",
                ));
            }
            if bytes.contains(&0) {
                return Err(Error::new("string_nul", "CString excludes embedded NUL"));
            }
            if add(bytes.len(), 1)? > count {
                return Err(Error::new(
                    "typed_overflow",
                    "CString plus NUL exceeds actual SDNA capacity",
                ));
            }
            let mut b = bytes.clone();
            b.push(0);
            Ok(b)
        }
        TypedValue::FixedBytes(b) => {
            if kind != b"char" || width != 1 || b.len() != count {
                return Err(Error::new(
                    "typed_width",
                    "FixedBytes needs exact whole char array width",
                ));
            }
            Ok(b.clone())
        }
        TypedValue::Array(values) => {
            if values.len() != count {
                return Err(Error::new(
                    "typed_width",
                    "array length must match all SDNA dimensions",
                ));
            }
            let mut b = Vec::new();
            for v in values {
                b.extend(scalar(kind, width, v)?);
            }
            Ok(b)
        }
        _ if count == 1 => scalar(kind, width, v),
        _ => Err(Error::new(
            "typed_width",
            "whole array requires exact Array or fixed char bytes",
        )),
    }
}
fn candidate<'a>(
    s: &'a EditSnapshot,
    field: &FieldRef<'a>,
    kind: &[u8],
    width: usize,
    expected: &TypedValue,
    replacement: &TypedValue,
) -> Result<(Span, usize, Json)> {
    if !std::ptr::eq(s, field.snapshot) {
        return Err(Error::new(
            "snapshot_mismatch",
            "field belongs to another immutable snapshot",
        ));
    }
    let f = s.reader();
    let loc = f.locate_field(field.block, field.element, &field.steps)?;
    if loc.field.name.starts_with(b"_pad")
        || loc.field.name == b"pad"
        || loc
            .field
            .name
            .strip_prefix(b"pad")
            .is_some_and(|n| !n.is_empty() && n.iter().all(u8::is_ascii_digit))
    {
        return Err(Error::new(
            "padding_write",
            "serialized padding is not a typed editing leaf",
        ));
    }
    if loc.field.pointer {
        return Err(Error::new(
            "pointer_write",
            "Typed editing never writes pointers/function pointers",
        ));
    }
    let count = if loc.indexed { 1 } else { loc.field.array_len };
    let extent = mul(count, loc.field.element_size)?;
    if f.schema().types[loc.field.type_index] != kind || width != extent {
        return Err(Error::new(
            "typed_width",
            "expected SDNA type and total width must match exactly",
        ));
    }
    let before = encode(kind, loc.field.element_size, count, expected)?;
    let after = encode(kind, loc.field.element_size, count, replacement)?;
    let old = f.field_bytes(field.block, loc.offset, extent)?;
    if matches!(expected, TypedValue::CString(_)) {
        let n = old.iter().position(|&b| b == 0).ok_or_else(|| {
            Error::new(
                "string_termination",
                "old fixed char array lacks bounded NUL",
            )
        })?;
        if old[..=n] != before {
            return Err(Error::new("expected_old", "CString old guard differs"));
        }
    } else if old.as_ref() != before {
        return Err(Error::new("expected_old", "exact typed old bits differ"));
    }

    if std::mem::discriminant(expected) != std::mem::discriminant(replacement) {
        return Err(Error::new(
            "typed_kind",
            "expected/replacement representation differs",
        ));
    }
    let offset = add(f.block(field.block)?.data_offset, loc.offset)?;
    let declaration = json!({"block_index":field.block,"owner_block_index":field.owner,"element":field.element,
        "path":field.steps.iter().map(|p|match p {PathStep::Field(n)=>json!({"field_hex":hex(n)}),PathStep::Index(i)=>json!({"index":i})}).collect::<Vec<_>>(),
        "sdna_type_hex":hex(kind),"offset":offset,"extent":extent,"before_hex":hex(&old),"replacement_hex":hex(&after)});
    let bytes = if old[..after.len()] == after {
        Vec::new()
    } else {
        after
    };
    Ok((Span { offset, bytes }, extent, declaration))
}
fn local(s: &EditSnapshot, r: &FieldRef<'_>) -> Result<()> {
    let f = s.reader();
    let code = f.block(r.owner)?.code;
    if code == *b"USER" || code == *b"ID\0\0" {
        return Err(Error::new(
            "profile_owner",
            "comp profile requires local scene/image/node owner",
        ));
    }
    for key in ["lib", "override_library"] {
        let path = [
            PathStep::Field(b"id".to_vec()),
            PathStep::Field(key.as_bytes().to_vec()),
        ];
        let v = f.get(r.owner, 0, &path, true)?;
        if !matches!(v, crate::value::Value::Pointer(0)) {
            return Err(Error::new(
                "profile_linked",
                "linked/override-protected owner refused",
            ));
        }
    }
    Ok(())
}
fn fields(p: &[PathStep]) -> Result<Vec<&[u8]>> {
    p.iter()
        .map(|p| match p {
            PathStep::Field(n) => Ok(n.as_slice()),
            _ => Err(Error::new(
                "profile_field",
                "indexed profile target not supported",
            )),
        })
        .collect()
}
fn profile(s: &EditSnapshot, w: &TypedWrite<'_>) -> Result<&'static str> {
    local(s, &w.field)?;
    let f = s.reader();
    let loc = f.locate_field(w.field.block, w.field.element, &w.field.steps)?;
    let p = fields(&w.field.steps)?;
    let bounded = |v: &TypedValue, lo, hi| matches!(v,TypedValue::Signed(n) if *n>=lo&&*n<=hi);

    let float_range = |v: &TypedValue, lo: f32, hi: f32| matches!(v,TypedValue::F32Bits(b) if f32::from_bits(*b).is_finite() && f32::from_bits(*b)>=lo && f32::from_bits(*b)<=hi);
    let ty =
        f.schema().types[f.schema().structs[f.struct_index(w.field.block)?].type_index].as_slice();
    let result = match (ty, p.as_slice()) {
        (b"Scene", [b"r", b"sfra" | b"efra"]) if bounded(&w.replacement, 0, 1048574) => {
            "comp.scene-temporal"
        }
        (b"Scene", [b"r", b"cfra"]) if bounded(&w.replacement, -1048574, 1048574) => {
            "comp.scene-temporal"
        }
        (b"Scene", [b"r", b"frame_step"]) if bounded(&w.replacement, 1, 1048574) => {
            "comp.scene-temporal"
        }
        (b"Scene", [b"r", b"xsch" | b"ysch"]) if bounded(&w.replacement, 4, 65536) => {
            "comp.render-format-output"
        }
        (b"Scene", [b"r", b"size" | b"frs_sec"]) if bounded(&w.replacement, 1, 32767) => {
            "comp.render-format-output"
        }
        (b"Scene", [b"r", b"frs_sec_base"]) if float_range(&w.replacement, 0.00001, 1000000.0) => {
            "comp.render-format-output"
        }
        (b"Scene", [b"r", b"xasp" | b"yasp"]) if float_range(&w.replacement, 1.0, 200.0) => {
            "comp.render-format-output"
        }
        (b"Scene", [b"r", b"pic"]) if matches!(&w.replacement,TypedValue::CString(b) if !b.is_empty()) => {
            "comp.render-format-output"
        }
        (b"Image", [b"name"]) if matches!(&w.replacement,TypedValue::CString(b) if !b.is_empty()) =>
        {
            sequence_image(s, w.field.block)?;
            "comp.sequence-reader"
        }

        (b"Image", [b"lastframe"]) if matches!(&w.replacement, TypedValue::Signed(_)) => {
            sequence_image(s, w.field.block)?;
            "comp.sequence-reader"
        }
        (b"ImageUser", [b"frames"]) if bounded(&w.replacement, 1, 1048574) => {
            image_user(s, &w.field)?;
            "comp.sequence-reader"
        }
        (b"ImageUser", [b"sfra" | b"offset"]) if bounded(&w.replacement, -1048574, 1048574) => {
            image_user(s, &w.field)?;
            "comp.sequence-reader"
        }
        (b"Library", [b"name"]) => {
            compatible_library(s, w)?;
            "comp.compatible-library-retarget"
        }
        _ => {
            return Err(Error::new(
                "profile_field",
                format!(
                    "unqualified comp field {}.{}",
                    hex(loc.parent_type),
                    hex(&loc.field.name)
                ),
            ));
        }
    };
    Ok(result)
}
/// SDNA struct type of the record serialized at `block`.
fn record_type(f: &BlendFile, block: usize) -> Result<&[u8]> {
    let index = f.struct_index(block)?;
    Ok(f.schema().types[f.schema().structs[index].type_index].as_slice())
}
/// Compositor image reader node that stores `user` (an ImageUser record) inside
/// `owner`. A qualified reader is a `CompositorNodeImage` whose generated name
/// carries the `IN_` reader prefix and whose `storage` names that reader state.
/// A node is not a reader merely because its SDNA field is called `storage`.
fn reader_for_user(s: &EditSnapshot, user: usize, owner: usize) -> Result<Option<usize>> {
    let f = s.reader();
    let address = f.block(user)?.old_address;
    for (i, o) in s.owners.iter().enumerate() {
        if *o != Some(owner) || record_type(f, i)? != b"bNode" {
            continue;
        }
        if !matches!(f.get(i,0,&[PathStep::Field(b"idname".to_vec())],true)?,crate::value::Value::Bytes(ref b) if b==b"CompositorNodeImage")
        {
            continue;
        }
        if !matches!(f.get(i,0,&[PathStep::Field(b"name".to_vec())],true)?,crate::value::Value::Bytes(ref b) if b.starts_with(b"IN_"))
        {
            continue;
        }
        if matches!(f.get(i,0,&[PathStep::Field(b"storage".to_vec())],true)?,crate::value::Value::Pointer(p) if p==address)
        {
            return Ok(Some(i));
        }
    }
    Ok(None)
}
/// Image record a qualified reader names. A reader without an existing Image
/// never admits a sequence/compositor profile, whatever the byte width.
fn reader_image(s: &EditSnapshot, node: usize) -> Result<usize> {
    let f = s.reader();
    let path = [PathStep::Field(b"id".to_vec())];
    let crate::value::Value::Pointer(address) = f.get(node, 0, &path, true)? else {
        return Err(Error::new(
            "profile_discriminator",
            "Image reader lacks Image ID",
        ));
    };
    if address == 0 {
        return Err(Error::new(
            "profile_discriminator",
            "Image reader lacks Image ID",
        ));
    }
    let image = f.resolve_pointer(node, 0, &path, address)?.ok_or_else(|| {
        Error::new(
            "profile_discriminator",
            "reader does not reference Image ID",
        )
    })?;
    if record_type(f, image)? != b"Image" {
        return Err(Error::new(
            "profile_discriminator",
            "reader does not reference Image ID",
        ));
    }
    Ok(image)
}
/// A source field equal to `IMA_SRC_SEQUENCE` is the image's own, independently
/// serialized provenance; an equal byte width from a generated image is not.
fn sequence_source(s: &EditSnapshot, image: usize) -> Result<bool> {
    Ok(s.reader()
        .get(image, 0, &[PathStep::Field(b"source".to_vec())], true)?
        == crate::value::Value::Signed(2))
}
fn sequence_image(s: &EditSnapshot, block: usize) -> Result<()> {
    if !sequence_source(s, block)? {
        return Err(Error::new(
            "profile_discriminator",
            "sequence profile requires existing SEQUENCE Image",
        ));
    }
    let f = s.reader();
    for (user, owner) in s.owners.iter().enumerate() {
        let Some(owner) = *owner else { continue };
        if record_type(f, user)? != b"ImageUser" {
            continue;
        }
        if let Some(node) = reader_for_user(s, user, owner)? {
            let image = reader_image(s, node)?;
            if !sequence_source(s, image)? {
                return Err(Error::new(
                    "profile_discriminator",
                    "sequence profile requires existing SEQUENCE Image",
                ));
            }
        }
    }
    Ok(())
}
fn build_reader(s: &EditSnapshot, block: usize, owner: usize) -> Result<bool> {
    let f = s.reader();
    let address = f.block(block)?.old_address;
    for (i, o) in s.owners.iter().enumerate() {
        if *o != Some(owner)
            || f.schema().types[f.schema().structs[f.struct_index(i)?].type_index] != b"bNode"
        {
            continue;
        }
        if !matches!(f.get(i,0,&[PathStep::Field(b"storage".to_vec())],true)?,crate::value::Value::Pointer(p) if p==address)
        {
            continue;
        }
        if matches!(f.get(i,0,&[PathStep::Field(b"idname".to_vec())],true)?,crate::value::Value::Bytes(ref b) if b==b"CompositorNodeImage")
            && matches!(f.get(i,0,&[PathStep::Field(b"name".to_vec())],true)?,crate::value::Value::Bytes(ref b) if b.starts_with(b"IN_"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn image_user(s: &EditSnapshot, r: &FieldRef<'_>) -> Result<()> {
    let node = reader_for_user(s, r.block, r.owner)?.ok_or_else(|| {
        Error::new(
            "profile_discriminator",
            "ImageUser storage is not a compositor Image reader",
        )
    })?;
    if !sequence_source(s, reader_image(s, node)?)? {
        return Err(Error::new(
            "profile_discriminator",
            "sequence profile requires existing SEQUENCE Image",
        ));
    }
    Ok(())
}
fn compatible_library(_s: &EditSnapshot, w: &TypedWrite<'_>) -> Result<()> {
    let (TypedValue::CString(old), TypedValue::CString(new)) = (&w.expected, &w.replacement) else {
        return Err(Error::new(
            "profile_library",
            "library relocation requires CString",
        ));
    };
    if old.is_empty() || new.is_empty() {
        return Err(Error::new(
            "profile_library",
            "external library paths must be nonempty",
        ));
    }

    Ok(())
}
fn library_context(p: &EditPlan<'_>, output: &Path) -> Result<()> {
    if p.mode != EditMode::Comp {
        return Ok(());
    }
    let s = p.snapshot;
    let f = s.reader();
    for (block, b) in f.blocks().iter().enumerate() {
        if b.code != *b"LI\0\0" {
            continue;
        }
        for key in ["packedfile", "archive_parent_library"] {
            if !matches!(
                f.get(block, 0, &[PathStep::Field(key.as_bytes().to_vec())], true)?,
                crate::value::Value::Pointer(0)
            ) {
                return Err(Error::new(
                    "profile_library",
                    "packed/archive libraries require their own profile",
                ));
            }
        }
        let loc = f.locate_field(block, 0, &[PathStep::Field(b"name".to_vec())])?;
        let raw = f.field_bytes(block, loc.offset, loc.field.size)?;
        let end = raw
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| Error::new("string_termination", "Library path lacks bounded NUL"))?;
        let old = &raw[..end];
        let offset = add(b.data_offset, loc.offset)?;
        let replacement = p
            .spans
            .iter()
            .find(|s| s.offset == offset)
            .map(|s| s.bytes.as_slice())
            .unwrap_or(&raw);
        let end = replacement.iter().position(|&b| b == 0).ok_or_else(|| {
            Error::new("string_termination", "new Library path lacks bounded NUL")
        })?;
        let new = &replacement[..end];
        same_library_content(s, old, new, output)?;
    }
    Ok(())
}
fn same_library_content(s: &EditSnapshot, old: &[u8], new: &[u8], output: &Path) -> Result<()> {
    if old.is_empty() || new.is_empty() {
        return Err(Error::new(
            "profile_library",
            "external library paths must be nonempty",
        ));
    }

    let a = crate::trace::resolve(&s.input, old);
    let b = crate::trace::resolve(output, new);
    if a == b {
        return Ok(());
    }
    let mut a = File::open(a).map_err(io)?;
    let mut b = File::open(b).map_err(io)?;
    if a.metadata().map_err(io)?.len() != b.metadata().map_err(io)?.len() {
        return Err(Error::new(
            "profile_library",
            "different library content requires native recompilation",
        ));
    }
    let mut aa = std::io::BufReader::new(&mut a);
    let mut bb = std::io::BufReader::new(&mut b);
    use std::io::BufRead;
    loop {
        let x = aa.fill_buf().map_err(io)?;
        let y = bb.fill_buf().map_err(io)?;
        let n = x.len().min(y.len());
        if n == 0 {
            if x.len() != y.len() {
                return Err(Error::new("profile_library", "library source changed"));
            }
            break;
        }
        if x[..n] != y[..n] {
            return Err(Error::new(
                "profile_library",
                "different library content requires native recompilation",
            ));
        }
        aa.consume(n);
        bb.consume(n);
    }
    Ok(())
}
pub fn plan_fields<'a>(
    s: &'a EditSnapshot,
    writes: &[TypedWrite<'a>],
    bits: &[BitWrite<'a>],
    mode: EditMode,
) -> Result<EditPlan<'a>> {
    let mut spans = Vec::new();
    let mut declarations = Vec::new();
    let mut extents = Vec::new();
    for w in writes {
        let (span, n, mut d) = candidate(
            s,
            &w.field,
            &w.sdna_type,
            w.width,
            &w.expected,
            &w.replacement,
        )?;
        if mode == EditMode::Comp {
            d["profile"] = json!(profile(s, w)?);
        }
        extents.push((span.offset, n));
        spans.push(span);
        declarations.push(d);
    }
    for w in bits {
        let loc = s
            .reader()
            .locate_field(w.field.block, w.field.element, &w.field.steps)?;
        let signed = integer_kind(&w.sdna_type, loc.field.element_size)?;
        if !loc.indexed && loc.field.array_len != 1 {
            return Err(Error::new(
                "typed_width",
                "masked flag write requires scalar integer",
            ));
        }
        let old = encode(&w.sdna_type, loc.field.element_size, 1, &w.expected)?;
        let mut a = [0u8; 8];
        a[..old.len()].copy_from_slice(&old);
        let old = u64::from_le_bytes(a);
        let width = loc.field.element_size;
        let allowed = if width == 8 {
            u64::MAX
        } else {
            (1u64 << (width * 8)) - 1
        };
        if (w.clear | w.set) & !allowed != 0 || w.clear & w.set != 0 {
            return Err(Error::new(
                "flag_mask",
                "mask exceeds field or sets and clears same bit",
            ));
        }
        let value = (old & !w.clear) | w.set;
        let replacement = if signed {
            let shift = 64 - width * 8;
            TypedValue::Signed(((value << shift) as i64) >> shift)
        } else {
            TypedValue::Unsigned(value)
        };
        let t = TypedWrite {
            field: w.field.clone(),
            sdna_type: w.sdna_type.clone(),
            width: w.width,
            expected: w.expected.clone(),
            replacement,
        };
        let (span, n, mut d) = candidate(
            s,
            &t.field,
            &t.sdna_type,
            t.width,
            &t.expected,
            &t.replacement,
        )?;

        if mode == EditMode::Comp {
            return Err(Error::new(
                "profile_mask",
                "comp flag mutations are not yet qualified; preserve template constants",
            ));
        }
        d["clear_decimal"] = json!(w.clear.to_string());
        d["set_decimal"] = json!(w.set.to_string());
        extents.push((span.offset, n));
        spans.push(span);
        declarations.push(d);
    }
    extents.sort_unstable();
    for pair in extents.windows(2) {
        if add(pair[0].0, pair[0].1)? > pair[1].0 {
            return Err(Error::new(
                "edit_conflict",
                "overlapping declared leaf extents",
            ));
        }
    }
    spans.retain(|s| !s.bytes.is_empty());
    spans.sort_by_key(|s| s.offset);
    let plan = EditPlan {
        snapshot: s,
        spans,
        declarations,
        mode,
    };
    if mode == EditMode::Comp {
        temporal(&plan)?;
    }
    Ok(plan)
}
fn planned_int(p: &EditPlan<'_>, block: usize, names: &[&str]) -> Result<i32> {
    let f = p.snapshot.reader();
    let path = names
        .iter()
        .map(|n| PathStep::Field(n.as_bytes().to_vec()))
        .collect::<Vec<_>>();
    let loc = f.locate_field(block, 0, &path)?;
    if loc.field.pointer
        || loc.field.element_size != 4
        || loc.field.array_len != 1
        || f.schema().types[loc.field.type_index] != b"int"
    {
        return Err(Error::new(
            "typed_width",
            "temporal profile requires exact int32 leaf",
        ));
    }
    let off = add(f.block(block)?.data_offset, loc.offset)?;
    let raw = f.field_bytes(block, loc.offset, 4)?;
    let data = p
        .spans
        .iter()
        .find(|s| s.offset == off)
        .map(|s| s.bytes.as_slice())
        .unwrap_or(&raw);
    Ok(i32::from_le_bytes(data.try_into().map_err(|_| {
        Error::new("typed_width", "temporal int width")
    })?))
}
fn temporal(p: &EditPlan<'_>) -> Result<()> {
    let s = p.snapshot;
    let f = s.reader();
    let mut scenes = Vec::new();
    let touched = p
        .declarations
        .iter()
        .filter_map(|d| {
            d["block_index"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
        })
        .collect::<std::collections::BTreeSet<_>>();
    for (i, b) in f.blocks().iter().enumerate() {
        if b.code != *b"SC\0\0" {
            continue;
        }
        let start = planned_int(p, i, &["r", "sfra"])?;
        let end = planned_int(p, i, &["r", "efra"])?;
        if start > end || planned_int(p, i, &["r", "frame_step"])? <= 0 {
            return Err(Error::new(
                "profile_temporal",
                "final frame range reversed or step nonpositive",
            ));
        }
        if let crate::value::Value::Pointer(tree) = f.get(
            i,
            0,
            &[PathStep::Field(b"compositing_node_group".to_vec())],
            true,
        )? {
            scenes.push((i, tree, start, end));
        }
    }
    for (i, _) in f.blocks().iter().enumerate() {
        let Some(owner) = s.owners[i] else {
            continue;
        };
        if f.schema().types[f.schema().structs[f.struct_index(i)?].type_index] != b"ImageUser" {
            continue;
        }
        if !build_reader(s, i, owner)? {
            continue;
        }
        let reader = s.field(i, 0, vec![PathStep::Field(b"frames".to_vec())])?;
        image_user(s, &reader)?;
        let tree = f.block(owner)?.old_address;
        let users = scenes
            .iter()
            .filter(|(sc, t, _, _)| {
                *t == tree || (*sc == owner && f.blocks()[owner].code == *b"SC\0\0")
            })
            .collect::<Vec<_>>();
        if !touched.contains(&i) && !users.iter().any(|(sc, _, _, _)| touched.contains(sc)) {
            continue;
        }
        if users.is_empty() {
            return Err(Error::new(
                "profile_temporal",
                "edited ImageUser has no active compositor Scene",
            ));
        }

        for (_, _, start, end) in users {
            if i64::from(planned_int(p, i, &["frames"])?) != i64::from(*end) - i64::from(*start) + 1
                || planned_int(p, i, &["sfra"])? != *start
                || i64::from(planned_int(p, i, &["offset"])?) != i64::from(*start) - 1
            {
                return Err(Error::new(
                    "profile_temporal",
                    "final scene/reader range, duration and offset disagree",
                ));
            }
        }
    }
    Ok(())
}
pub fn write_plan(
    s: &EditSnapshot,
    p: &EditPlan<'_>,
    output: &Path,
    options: &EditOptions,
) -> Result<EditReportV2> {
    if !std::ptr::eq(s, p.snapshot) {
        return Err(Error::new(
            "snapshot_mismatch",
            "plan belongs to another snapshot",
        ));
    }
    s.captured.check(&s.input)?;
    library_context(p, output)?;
    refuse_existing(&s.captured.metadata, output)?;
    let source = s.captured.open_checked(&s.input)?;
    let physical = crate::physical::Physical::captured(&source, &s.captured);
    let family = physical.family().map_err(io)?;
    let inspected;
    let seek = if let Some(admission) = physical.captured_admission(&s.input) {
        admission?
    } else if family == "zstd" {
        inspected = crate::edit_container::inspect(&physical, s.reader())?;
        inspected.as_ref()
    } else {
        None
    };
    let mut stage = AtomicOutput::create(&s.captured.metadata, output)?;
    if p.spans.is_empty() {
        physical.copy_whole(&s.input, &s.captured.metadata, &mut stage.file)?;
    } else {
        crate::edit_container::emit_spans(
            &physical,
            s.reader(),
            family,
            seek,
            &p.spans,
            &mut stage.file,
        )?;
    }
    if family != "raw" {
        crate::edit_decode::verify_output(&stage.file, family, s.reader().decompressed_len())?;
    } else if stage.file.metadata().map_err(io)?.len() != s.reader().decompressed_len() as u64 {
        return Err(Error::new("declared_diff", "raw output length changed"));
    }

    stage.file.seek(SeekFrom::Start(0)).map_err(io)?;
    let mut encoded = Vec::new();
    stage.file.read_to_end(&mut encoded).map_err(io)?;
    let mut decoded = Vec::new();
    match family {
        "gzip" => {
            flate2::read::MultiGzDecoder::new(encoded.as_slice())
                .read_to_end(&mut decoded)
                .map_err(io)?;
        }
        "zstd" => {
            zstd::stream::read::Decoder::new(encoded.as_slice())
                .map_err(io)?
                .read_to_end(&mut decoded)
                .map_err(io)?;
        }
        _ => decoded = encoded,
    }
    let reread = BlendFile::from_bytes(decoded)?;
    owner_runs(&reread)?;
    let mut position = 0;
    for span in &p.spans {
        if reread
            .data
            .range(position, span.offset - position)?
            .as_ref()
            != s.reader()
                .data
                .range(position, span.offset - position)?
                .as_ref()
            || reread.data.range(span.offset, span.bytes.len())?.as_ref() != span.bytes
        {
            return Err(Error::new("declared_diff", "unexpected staged bytes"));
        }
        position = add(span.offset, span.bytes.len())?;
    }
    if reread
        .data
        .range(position, reread.decompressed_len() - position)?
        .as_ref()
        != s.reader()
            .data
            .range(position, s.reader().decompressed_len() - position)?
            .as_ref()
    {
        return Err(Error::new("declared_diff", "unexpected staged tail bytes"));
    }
    let length = stage.file.metadata().map_err(io)?.len();
    physical.check_before_publish(&s.input, &s.captured.metadata)?;
    let publication = stage.publish(options.durability);
    let mut error = publication.err();
    if error.is_none() && options.durability == crate::edit::Durability::End {
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
    let complete = error.is_none();
    let details = json!({"schema":2,"operation":"typed_edit","mode":if p.mode==EditMode::Comp {"comp"}else{"format-only"},
        "profile_build":"5.2.1/9e2066aef7ef; later SDNA layouts checked, native qualification separate",
        "complete":complete,"output_written":stage.published,"layout_verified":true,"container_verified":true,
        "declared_diff_verified":true,"native_verified":false,"semantic_verified":false,"structural_verified":false,
        "input_path_hex":hex(s.input.as_os_str().as_encoded_bytes()),"output_path_hex":hex(output.as_os_str().as_encoded_bytes()),
        "input_decoded_length":s.reader().decompressed_len(),"output_decoded_length":reread.decompressed_len(),"output_physical_length":length,
        "container_family":family,"container_reencoded":!p.spans.is_empty()&&family!="raw","edits":p.declarations,
        "durability":options.durability.as_str(),"durability_completed":stage.durability_completed,
        "errors":error.iter().map(|e|json!({"kind":e.kind,"message":e.message})).collect::<Vec<_>>()});
    Ok(EditReportV2 {
        complete,
        output_written: stage.published,
        layout_verified: true,
        container_verified: true,
        native_verified: false,
        semantic_verified: false,
        durability_completed: stage.durability_completed,
        details,
    })
}

#[cfg(test)]
mod enum_regression {
    use super::*;
    use crate::file_ops::graph::Graph;

    /// Typed composition profiles are native-qualified on 5.2. The committed seed carries native 4.5.8 DNA whose
    /// serialized records for these types share this layout, so only the declared
    /// header version is raised to reach the qualified sequence/compositor paths.
    fn seeded() -> Vec<u8> {
        let mut bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/file-ops/native-4.5.8/build-kit/empty.blend"),
        )
        .unwrap();
        bytes[9..12].copy_from_slice(b"502");
        bytes
    }
    fn record(f: &BlendFile, ty: &[u8]) -> usize {
        f.blocks()
            .iter()
            .enumerate()
            .find(|(i, _)| {
                f.struct_index(*i)
                    .map(|si| f.schema().types[f.schema().structs[si].type_index] == ty)
                    .unwrap_or(false)
            })
            .map(|(i, _)| i)
            .unwrap()
    }
    /// A qualified `IN_` compositor image reader over a sequence image, plus one
    /// probe reader kind whose own reader state is the tested subject.
    fn composed(probe: Option<&str>) -> (Vec<u8>, u64, Option<u64>) {
        let schema = BlendFile::from_bytes(seeded()).unwrap();
        let mut g = Graph::new(&schema).unwrap();
        let scene = g.new_typed("Scene", None).unwrap();
        g.records[scene].code = *b"SC\0\0";
        g.cstring(scene, &["id", "name"], "SCtest").unwrap();
        let image = g.new_typed("Image", Some(scene)).unwrap();
        g.records[image].code = *b"IM\0\0";
        g.cstring(image, &["id", "name"], "IMseq").unwrap();
        g.set(image, &["source"], 2).unwrap();
        let add = |g: &mut Graph<'_>, idname: &str, name: &str, id: Option<usize>| {
            let user = g.new_typed("ImageUser", Some(scene)).unwrap();
            g.set(user, &["frames"], 1).unwrap();
            g.set(user, &["sfra"], 1).unwrap();
            let node = g.new_typed("bNode", Some(scene)).unwrap();
            g.cstring(node, &["idname"], idname).unwrap();
            g.cstring(node, &["name"], name).unwrap();
            g.ptr(node, &["id"], id).unwrap();
            g.ptr(node, &["storage"], Some(user)).unwrap();
            g.records[user].address
        };
        let sequence = add(&mut g, "CompositorNodeImage", "IN_seq", Some(image));
        let probe_user = match probe {
            None | Some("absent") => None,
            Some("generated") => {
                let other = g.new_typed("Image", Some(scene)).unwrap();
                g.records[other].code = *b"IM\0\0";
                g.cstring(other, &["id", "name"], "IMgen").unwrap();
                g.set(other, &["source"], 4).unwrap();
                Some(add(&mut g, "CompositorNodeImage", "IN_gen", Some(other)))
            }
            Some("missing-id") => Some(add(&mut g, "CompositorNodeImage", "IN_gen", None)),
            Some("scene-id") => Some(add(&mut g, "CompositorNodeImage", "IN_gen", Some(scene))),
            Some("viewer") => Some(add(
                &mut g,
                "CompositorNodeViewer",
                "ViewerSentinel",
                Some(image),
            )),
            Some("shader") => {
                // A shader image node keeps its reader state inside its node storage,
                // so a standalone ImageUser is never its compositor reader state.
                let node = g.new_typed("bNode", Some(scene)).unwrap();
                g.cstring(node, &["idname"], "ShaderNodeTexImage").unwrap();
                g.cstring(node, &["name"], "Image").unwrap();
                g.ptr(node, &["id"], Some(image)).unwrap();
                let storage = g.new_typed("NodeTexImage", Some(scene)).unwrap();
                g.ptr(node, &["storage"], Some(storage)).unwrap();
                let loose = g.new_typed("ImageUser", Some(scene)).unwrap();
                g.set(loose, &["frames"], 1).unwrap();
                Some(g.records[loose].address)
            }
            Some("unprefixed") => Some(add(&mut g, "CompositorNodeImage", "XX_image", Some(image))),
            Some(other) => panic!("unknown probe {other}"),
        };
        (g.decoded().unwrap(), sequence, probe_user)
    }
    struct Opened {
        path: PathBuf,
        snapshot: EditSnapshot,
    }
    impl Drop for Opened {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
    fn open(bytes: Vec<u8>, tag: &str) -> Opened {
        let path = std::env::temp_dir().join(format!(
            "oxblend-typed-enum-{tag}-{}-{:?}.blend",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, &bytes).unwrap();
        let snapshot = snapshot(&path).unwrap();
        Opened { path, snapshot }
    }
    fn block_at(o: &Opened, address: u64) -> usize {
        o.snapshot
            .reader()
            .blocks()
            .iter()
            .position(|b| b.old_address == address)
            .unwrap()
    }
    fn kind(e: Error) -> String {
        e.kind.to_string()
    }

    /// Sequence-source is the image's own serialized provenance: a generated image
    /// of equal schema width is refused, and one generated reader withholds the
    /// profile from every reader in the file.
    #[test]
    fn sequence_profile_needs_every_qualified_reader_to_name_a_sequence_image() {
        for (probe, admitted) in [
            (None, true),
            (Some("absent"), true),
            (Some("viewer"), true),
            (Some("shader"), true),
            (Some("unprefixed"), true),
            (Some("generated"), false),
            (Some("missing-id"), false),
            (Some("scene-id"), false),
        ] {
            let (bytes, _, _) = composed(probe);
            let o = open(bytes, probe.unwrap_or("seq"));
            let image = record(o.snapshot.reader(), b"Image");
            assert_eq!(
                sequence_image(&o.snapshot, image).is_ok(),
                admitted,
                "{probe:?}"
            );
        }
        // The image's own source, not the reader, withholds the sequence profile.
        let (bytes, _, _) = composed(None);
        let o = open(bytes, "source");
        let image = record(o.snapshot.reader(), b"Image");
        let off = o
            .snapshot
            .reader()
            .locate_field(image, 0, &[PathStep::Field(b"source".to_vec())])
            .map(|l| o.snapshot.reader().block(image).unwrap().data_offset + l.offset)
            .unwrap();
        let mut changed = std::fs::read(&o.path).unwrap();
        changed[off..off + 2].copy_from_slice(&4u16.to_le_bytes());
        std::fs::write(&o.path, &changed).unwrap();
        let rebound = snapshot(&o.path).unwrap();
        assert_eq!(
            kind(sequence_image(&rebound, image).unwrap_err()),
            "profile_discriminator"
        );
    }

    /// ImageUser admission comes from the independently serialized storage link to a
    /// compositor image reader, never from the ImageUser SDNA type alone.
    #[test]
    fn image_user_needs_a_compositor_image_reader_storage() {
        for (probe, message) in [
            (None, "ok"),
            (
                Some("viewer"),
                "ImageUser storage is not a compositor Image reader",
            ),
            (
                Some("shader"),
                "ImageUser storage is not a compositor Image reader",
            ),
            (
                Some("unprefixed"),
                "ImageUser storage is not a compositor Image reader",
            ),
            (
                Some("generated"),
                "sequence profile requires existing SEQUENCE Image",
            ),
            (Some("missing-id"), "Image reader lacks Image ID"),
            (Some("scene-id"), "reader does not reference Image ID"),
        ] {
            let (bytes, sequence, probe_user) = composed(probe);
            let o = open(bytes, probe.unwrap_or("seq"));
            let address = probe_user.unwrap_or(sequence);
            let user = block_at(&o, address);
            let field = o
                .snapshot
                .field(user, 0, vec![PathStep::Field(b"frames".to_vec())])
                .unwrap();
            match image_user(&o.snapshot, &field) {
                Ok(()) => assert_eq!(message, "ok", "{probe:?}"),
                Err(e) => assert_eq!(e.message, message, "{probe:?}"),
            }
        }
    }

    /// A generated reader is not this reader's storage link, so its own state still
    /// admits while the file-wide sequence profile does not.
    #[test]
    fn image_user_is_scoped_to_its_own_storage_link() {
        let (bytes, sequence, _) = composed(Some("generated"));
        let o = open(bytes, "scope");
        let user = block_at(&o, sequence);
        let field = o
            .snapshot
            .field(user, 0, vec![PathStep::Field(b"frames".to_vec())])
            .unwrap();
        image_user(&o.snapshot, &field).unwrap();
        let image = record(o.snapshot.reader(), b"Image");
        assert!(sequence_image(&o.snapshot, image).is_err());
    }
}

#[cfg(test)]
#[path = "typed_edit/tests.rs"]
mod tests;
