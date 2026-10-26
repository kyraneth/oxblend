use crate::checked::{Cursor, add, mul};
use crate::{Error, Result};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
#[non_exhaustive]
pub struct Field {
    pub name: Vec<u8>,
    pub full_name: Vec<u8>,
    pub type_index: usize,
    pub offset: usize,
    pub size: usize,
    pub element_size: usize,
    pub array_len: usize,
    pub pointer: bool,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct Struct {
    pub type_index: usize,
    pub fields: Vec<Field>,
    #[cfg_attr(feature = "wasm-read-only", allow(dead_code))]
    pub(crate) by_name: HashMap<Vec<u8>, usize>,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct Schema {
    pub names: Vec<Vec<u8>>,
    pub types: Vec<Vec<u8>>,
    pub lengths: Vec<usize>,
    pub structs: Vec<Struct>,
    #[cfg_attr(feature = "wasm-read-only", allow(dead_code))]
    pub(crate) by_type: HashMap<usize, usize>,
    #[cfg(feature = "native")]
    pub(crate) pointer_programs:
        Vec<std::sync::OnceLock<Result<std::sync::Arc<[crate::file_ops::PointerLayout]>>>>,
    #[cfg(feature = "native")]
    unhandled_paths: Vec<std::sync::OnceLock<bool>>,
}
fn name_info(full: &[u8]) -> Result<(Vec<u8>, bool, usize)> {
    let invalid = |message| Error::new("invalid_sdna", message);
    let overflow = || Error::new("overflow", "array dimension");
    let pointer = full.contains(&b'*');
    let mut rest = full.trim_ascii();
    while rest.first().is_some_and(|b| b"*()".contains(b)) {
        rest = &rest[1..];
    }
    while rest.last().is_some_and(|b| b"*()".contains(b)) {
        rest = &rest[..rest.len() - 1];
    }
    let name_end = rest.iter().position(|&b| b == b'[').unwrap_or(rest.len());
    let name = &rest[..name_end];
    if name.is_empty() {
        return Err(invalid("empty field identifier"));
    }
    rest = &rest[name_end..];

    let mut array_len = 1usize;
    while !rest.is_empty() {
        if rest.first() != Some(&b'[') {
            return Err(invalid("array declarator"));
        }
        rest = &rest[1..];
        let close = rest
            .iter()
            .position(|&b| b == b']')
            .ok_or_else(|| invalid("array dimension"))?;
        let digits = &rest[..close];
        if digits.is_empty() || !digits.iter().all(|&b| b.is_ascii_digit()) {
            return Err(invalid("nondecimal array dimension"));
        }
        let mut dimension: u64 = 0;
        for &byte in digits {
            dimension = dimension
                .checked_mul(10)
                .and_then(|value| value.checked_add(u64::from(byte - b'0')))
                .ok_or_else(overflow)?;
        }
        let dimension = usize::try_from(dimension).map_err(|_| overflow())?;
        array_len = mul(array_len, dimension)?;
        rest = &rest[close + 1..];
    }
    Ok((name.to_vec(), pointer, array_len))
}
impl Schema {
    #[cfg(feature = "native")]
    pub(crate) fn unhandled_path(&self, si: usize) -> bool {
        fn visit(s: &Schema, ti: usize, visiting: &mut HashSet<usize>) -> bool {
            if !visiting.insert(ti) {
                return true;
            }
            let result = s.by_type.get(&ti).is_some_and(|si| {
                s.structs[*si].fields.iter().any(|field| {
                    if field.pointer {
                        return false;
                    }
                    let byte = matches!(
                        s.types[field.type_index].as_slice(),
                        b"char" | b"uchar" | b"int8_t" | b"uint8_t"
                    );
                    let path = [b"path".as_slice(), b"file", b"dir"].iter().any(|token| {
                        field
                            .name
                            .windows(token.len())
                            .any(|part| part.eq_ignore_ascii_case(token))
                    });
                    (byte && field.full_name.contains(&b'[') && path)
                        || visit(s, field.type_index, visiting)
                })
            });
            visiting.remove(&ti);
            result
        }
        *self.unhandled_paths[si]
            .get_or_init(|| visit(self, self.structs[si].type_index, &mut HashSet::new()))
    }

    pub(crate) fn parse(data: &[u8], little: bool, pointer_size: usize) -> Result<Self> {
        let mut cursor = Cursor {
            data,
            offset: 0,
            little,
        };
        cursor.tag(b"SDNA")?;
        cursor.tag(b"NAME")?;
        let count =
            usize::try_from(cursor.uint(4)?).map_err(|_| Error::new("overflow", "name count"))?;
        cursor.remaining_records(count, 1)?;
        let mut names = Vec::new();
        for _ in 0..count {
            names.push(cursor.string()?);
        }
        cursor.aligned()?;
        cursor.tag(b"TYPE")?;
        let count =
            usize::try_from(cursor.uint(4)?).map_err(|_| Error::new("overflow", "type count"))?;
        cursor.remaining_records(count, 1)?;
        let mut types = Vec::new();
        for _ in 0..count {
            types.push(cursor.string()?);
        }
        let mut type_identifiers = HashSet::new();
        for name in &types {
            if !type_identifiers.insert(name.as_slice()) {
                return Err(Error::new("invalid_sdna", "duplicate type identifier"));
            }
        }
        cursor.aligned()?;
        cursor.tag(b"TLEN")?;
        cursor.remaining_records(types.len(), 2)?;
        let mut lengths = Vec::new();
        for _ in &types {
            lengths.push(cursor.uint(2)? as usize);
        }
        cursor.aligned()?;
        cursor.tag(b"STRC")?;
        let count =
            usize::try_from(cursor.uint(4)?).map_err(|_| Error::new("overflow", "struct count"))?;
        cursor.remaining_records(count, 4)?;
        let mut structs = Vec::new();
        let mut by_type = HashMap::new();
        for si in 0..count {
            let type_index = cursor.uint(2)? as usize;
            let declared = *lengths
                .get(type_index)
                .ok_or_else(|| Error::new("invalid_sdna", "struct type index"))?;
            let fields_count = cursor.uint(2)? as usize;
            cursor.remaining_records(fields_count, 4)?;
            let mut fields = Vec::new();
            let mut by_name = HashMap::new();
            let mut offset = 0;
            for _ in 0..fields_count {
                let field_type = cursor.uint(2)? as usize;
                let name_index = cursor.uint(2)? as usize;
                let full_name = names
                    .get(name_index)
                    .ok_or_else(|| Error::new("invalid_sdna", "field name index"))?
                    .clone();
                let declared_size = *lengths
                    .get(field_type)
                    .ok_or_else(|| Error::new("invalid_sdna", "field type index"))?;
                let (name, pointer, array_len) = name_info(&full_name)?;
                let element_size = if pointer { pointer_size } else { declared_size };
                let size = mul(element_size, array_len)?;
                let next = add(offset, size)?;
                if next > declared {
                    return Err(Error::new("invalid_sdna", "fields exceed TLEN"));
                }
                if by_name.insert(name.clone(), fields.len()).is_some() {
                    return Err(Error::new("invalid_sdna", "duplicate field name"));
                }
                fields.push(Field {
                    name,
                    full_name,
                    type_index: field_type,
                    offset,
                    size,
                    element_size,
                    array_len,
                    pointer,
                });
                offset = next;
            }

            if offset != declared {
                return Err(Error::new("invalid_sdna", "field sizes do not equal TLEN"));
            }
            if by_type.insert(type_index, si).is_some() {
                return Err(Error::new("invalid_sdna", "duplicate struct type"));
            }
            structs.push(Struct {
                type_index,
                fields,
                by_name,
            });
        }
        if cursor.offset != data.len() {
            return Err(Error::new("invalid_sdna", "trailing DNA bytes"));
        }

        let mut indegree = vec![0usize; structs.len()];
        let mut edges = vec![Vec::new(); structs.len()];
        for (i, st) in structs.iter().enumerate() {
            for field in &st.fields {
                if !field.pointer
                    && let Some(&nested) = by_type.get(&field.type_index)
                {
                    edges[i].push(nested);
                    indegree[nested] += 1;
                }
            }
        }
        let mut queue = std::collections::VecDeque::from_iter(
            indegree
                .iter()
                .enumerate()
                .filter(|(_, n)| **n == 0)
                .map(|(i, _)| i),
        );
        let mut visited = 0;
        while let Some(i) = queue.pop_front() {
            visited += 1;
            for &j in &edges[i] {
                indegree[j] -= 1;
                if indegree[j] == 0 {
                    queue.push_back(j)
                }
            }
        }
        if visited != structs.len() {
            return Err(Error::new(
                "invalid_sdna",
                "recursive by-value struct containment",
            ));
        }
        if structs
            .iter()
            .flat_map(|s| &s.fields)
            .any(|f| !f.pointer && f.array_len != 0 && f.element_size == 0)
        {
            return Err(Error::new(
                "invalid_sdna",
                "non-empty by-value field has zero element size",
            ));
        }
        Ok(Self {
            #[cfg(feature = "native")]
            pointer_programs: (0..structs.len())
                .map(|_| std::sync::OnceLock::new())
                .collect(),
            #[cfg(feature = "native")]
            unhandled_paths: (0..structs.len())
                .map(|_| std::sync::OnceLock::new())
                .collect(),
            names,
            types,
            lengths,
            structs,
            by_type,
        })
    }
}

#[cfg(all(test, feature = "native"))]
#[path = "../newtests/panel_declarators.rs"]
mod panel_declarators;

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic SDNA block. Types carry their declared lengths
    /// (TLEN); struct declarations reference them. The declared struct length
    /// is supplied by the caller through `types`, independently of the
    /// declarator arithmetic under test.
    type SavedStructure<'a> = (&'a [u8], &'a [(&'a [u8], &'a [u8])]);
    fn sdna(names: &[&[u8]], types: &[(&[u8], u16)], structs: &[SavedStructure<'_>]) -> Vec<u8> {
        let mut out = b"SDNA".to_vec();
        out.extend_from_slice(b"NAME");
        out.extend_from_slice(&(names.len() as u32).to_le_bytes());
        for name in names {
            out.extend_from_slice(name);
            out.push(0);
        }
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        out.extend_from_slice(b"TYPE");
        out.extend_from_slice(&(types.len() as u32).to_le_bytes());
        for (ty, _) in types {
            out.extend_from_slice(ty);
            out.push(0);
        }
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        out.extend_from_slice(b"TLEN");
        for (_, len) in types {
            out.extend_from_slice(&len.to_le_bytes());
        }
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        out.extend_from_slice(b"STRC");
        out.extend_from_slice(&(structs.len() as u32).to_le_bytes());
        let tindex = |ty: &[u8]| types.iter().position(|(n, _)| *n == ty).unwrap() as u16;
        let nindex = |name: &[u8]| names.iter().position(|n| *n == name).unwrap() as u16;
        for (sty, fields) in structs {
            out.extend_from_slice(&tindex(sty).to_le_bytes());
            out.extend_from_slice(&(fields.len() as u16).to_le_bytes());
            for (fty, fname) in *fields {
                out.extend_from_slice(&tindex(fty).to_le_bytes());
                out.extend_from_slice(&nindex(fname).to_le_bytes());
            }
        }
        out
    }

    #[test]
    fn declarator_vectors() {
        for (input, name, pointer, len) in [
            (b"value".as_slice(), &b"value"[..], false, 1usize),
            (b"*value", b"value", true, 1),
            (b"grid[2][3]", b"grid", false, 6),
            (b"(*callback)()", b"callback", true, 1),
            // Observed saved spellings.
            (b"**chan_array", b"chan_array", true, 1),
            (b"*mtex[18]", b"mtex", true, 18),
            (b"drw_corners[2][4][2]", b"drw_corners", false, 16),
            (b"pad_1", b"pad_1", false, 1),
            (b"library_filepath[1024]", b"library_filepath", false, 1024),
            // Zero extent is a count of zero, not one.
            (b"grid[0]", b"grid", false, 0),
            (b"grid[0][5]", b"grid", false, 0),
            (b"*grid[0]", b"grid", true, 0),
            (b"grid[012]", b"grid", false, 12),
        ] {
            let (parsed, is_pointer, array_len) = name_info(input).unwrap();
            assert_eq!(
                parsed,
                name,
                "declarator {}",
                String::from_utf8_lossy(input)
            );
            assert_eq!(
                is_pointer,
                pointer,
                "pointer {}",
                String::from_utf8_lossy(input)
            );
            assert_eq!(array_len, len, "len {}", String::from_utf8_lossy(input));
        }
    }

    #[test]
    fn malformed_declarators_are_refused_before_payloads() {
        for input in [
            b"grid[x]".as_slice(),
            b"grid[]",
            b"grid[2",
            b"grid[2][",
            b"grid][",
            b"grid[2]x",
            b"grid[-1]",
            b"grid[ 2]",
            b"grid[2 ]",
            b"[grid]",
            b"*",
            b"**",
            b"",
            b"()",
            b"(*x[2])junk",
            b"x[2]()junk",
        ] {
            let error = name_info(input).unwrap_err();
            assert_eq!(
                error.kind,
                "invalid_sdna",
                "declarator {}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn declarator_overflow_is_explicit() {
        for input in [
            b"grid[99999999999999999999]".as_slice(),
            b"grid[18446744073709551616]",
            // Dimension product 2^64 exceeds the representable extent.
            b"grid[4294967296][4294967296]",
        ] {
            let error = name_info(input).unwrap_err();
            assert_eq!(
                error.kind,
                "overflow",
                "declarator {}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn saved_declaration_widths() {
        let dna = sdna(
            &[b"*arr[2]", b"grid[2][3]"],
            &[(b"float", 4), (b"Holder", 16), (b"Grid", 24)],
            &[
                (b"Holder", &[(b"float", b"*arr[2]")]),
                (b"Grid", &[(b"float", b"grid[2][3]")]),
            ],
        );
        let schema = Schema::parse(&dna, true, 8).unwrap();
        let holder = &schema.structs[schema.by_type[&1]];
        let field = &holder.fields[0];
        assert_eq!(field.name, b"arr");
        assert!(field.pointer);
        assert_eq!(field.array_len, 2);
        assert_eq!(field.element_size, 8);
        assert_eq!(field.size, 16);
        let grid = &schema.structs[schema.by_type[&2]];
        let field = &grid.fields[0];
        assert_eq!(field.name, b"grid");
        assert!(!field.pointer);
        assert_eq!(field.array_len, 6);
        assert_eq!(field.element_size, 4);
        assert_eq!(field.size, 24);
    }

    /// A zero dimension is accepted exactly when the saved enclosing layout
    /// stays consistent: the zero-extent field occupies nothing, and the
    /// declared struct length must still match the sum.
    #[test]
    fn zero_extent_layout_acceptance() {
        let dna = sdna(
            &[b"c", b"z[0]"],
            &[(b"char", 1), (b"int", 4), (b"Holder", 1)],
            &[(b"Holder", &[(b"char", b"c"), (b"int", b"z[0]")])],
        );
        let schema = Schema::parse(&dna, true, 8).unwrap();
        let holder = &schema.structs[schema.by_type[&2]];
        assert_eq!(holder.fields.len(), 2);
        let zero = &holder.fields[1];
        assert_eq!(zero.array_len, 0);
        assert_eq!(zero.size, 0);
        assert_eq!(zero.offset, 1);
        // The same zero-extent declarator with an inconsistent saved length
        // is refused as an invalid layout, not silently widened.
        let dna = sdna(
            &[b"c", b"z[0]"],
            &[(b"char", 1), (b"int", 4), (b"Holder", 5)],
            &[(b"Holder", &[(b"char", b"c"), (b"int", b"z[0]")])],
        );
        let error = Schema::parse(&dna, true, 8).unwrap_err();
        assert_eq!(error.kind, "invalid_sdna");
    }

    /// Overflow and malformed saved declarations are refused end to end,
    /// before any payload is accessed.
    #[test]
    fn saved_declaration_overflow_and_malformation() {
        // Dimension product overflows the representable extent.
        let dna = sdna(
            &[b"big[4294967296][4294967296]"],
            &[(b"int", 4), (b"Holder", 4)],
            &[(b"Holder", &[(b"int", b"big[4294967296][4294967296]")])],
        );
        assert_eq!(Schema::parse(&dna, true, 8).unwrap_err().kind, "overflow");
        // Element count fits, but the checked layout extent overflows.
        let dna = sdna(
            &[b"wide[4611686018427387904][2]"],
            &[(b"int", 4), (b"Holder", 4)],
            &[(b"Holder", &[(b"int", b"wide[4611686018427387904][2]")])],
        );
        assert_eq!(Schema::parse(&dna, true, 8).unwrap_err().kind, "overflow");
        for name in [b"grid[x]".as_slice(), b"grid[]", b"grid[2"] {
            let dna = sdna(
                &[name],
                &[(b"int", 4), (b"Holder", 4)],
                &[(b"Holder", &[(b"int", name)])],
            );
            assert_eq!(
                Schema::parse(&dna, true, 8).unwrap_err().kind,
                "invalid_sdna",
                "declarator {}",
                String::from_utf8_lossy(name)
            );
        }
    }
}
