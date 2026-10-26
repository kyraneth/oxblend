//! One native BHead ownership model, with a separate strict write-admission contract.
//! Readfile consumes contiguous DATA after an ID/USER and clears the datamap afterward.
//! Top-level DATA (before an ID or after control metadata) is skipped, not an owner error.
//! ID-family checks follow the serialized BHead and the source file's SDNA.
use crate::value::{PathStep, Value};
use crate::{BlendFile, Error, Result};

// Unknown/legacy two-byte ID codes still delimit scopes in read-only consumers.
// Their code alone is not grounds to refuse a file; qualification belongs to writes.
pub(crate) fn read_id(code: &[u8; 4]) -> bool {
    code[2..] == [0, 0] && code[..2] != [0, 0]
}
#[cfg(test)]
pub(crate) fn read_owners(f: &BlendFile) -> Vec<Option<usize>> {
    let mut data_owner = None;
    f.blocks()
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if b.code == *b"DATA" {
                return data_owner;
            }
            let owner = (read_id(&b.code) || b.code == *b"USER").then_some(i);
            // read_libblock's ID_LINK_PLACEHOLDER branch returns immediately:
            // its following DATA is skipped by the top-level DATA case.
            data_owner = owner.filter(|_| b.code != *b"ID\0\0");
            owner
        })
        .collect()
}
/// Strict typed qualification for FILE-OPS source import, census and publication.
/// It does not construct another resolver or a competing ownership projection.
pub(crate) fn qualify(f: &BlendFile) -> Result<()> {
    if !crate::id_codes::supported_version(f.header().version) {
        return Err(Error::new("owner_header", "unqualified saved-file version"));
    }
    qualify_headers(f, true)
}
/// Diagnostic read admission retains native header/layout/ownership checks,
/// without confusing a decimal saved-version stamp with a layout proof.
/// Writer/import admission continues to use `qualify` above.
pub(crate) fn qualify_read(f: &BlendFile) -> Result<()> {
    qualify_headers(f, false)
}
fn qualify_headers(f: &BlendFile, write: bool) -> Result<()> {
    let mut owner = None;
    for (i, b) in f.blocks().iter().enumerate() {
        if b.code != *b"DATA" {
            owner = None;
            if read_id(&b.code) || b.code == *b"USER" {
                qualify_id(f, i, write)?;
                owner = (b.code != *b"ID\0\0").then_some(i);
            }
        } else if owner.is_none() {
            return Err(Error::new(
                "stray_data_run",
                "DATA does not follow a native qualified ID/USER header",
            ));
        }
    }
    Ok(())
}
/// Qualify one interpreted read-side ID, including a linked-library selection.
/// Native ID BHeads use their file's struct table even at index zero; only DATA
/// allocations use the opaque-zero convention. Writes keep their old admission.
pub(crate) fn qualify_read_id(f: &BlendFile, b: usize) -> Result<()> {
    qualify_id(f, b, false)
}
fn qualify_id(f: &BlendFile, b: usize, write: bool) -> Result<()> {
    let block = f.block(b)?;
    if (write && block.sdna_index == 0) || block.count != 1 {
        return Err(Error::new(
            "owner_header",
            "ID/USER header is not one typed struct",
        ));
    }
    let si = f.struct_index(b)?;
    let structure = &f.schema().structs[si];
    let actual = &f.schema().types[structure.type_index];
    if !write {
        // Reject a contradictory record before any payload/name read. One
        // owner supplies this proof to both local roles and library selection.
        let length = f.schema().lengths[structure.type_index];
        if length == 0 || block.size != length {
            return Err(Error::new(
                "pointer_extent",
                "ID record contradicts source SDNA extent",
            ));
        }
        for field in &structure.fields {
            if crate::checked::add(field.offset, field.size)? > length {
                return Err(Error::new(
                    "pointer_extent",
                    "ID field exceeds source SDNA extent",
                ));
            }
        }
    }
    if block.code == *b"USER" {
        return if actual == b"UserDef" {
            Ok(())
        } else {
            Err(Error::new("owner_header", "USER is not UserDef"))
        };
    }
    let version = f.header().version;
    let expected = if write {
        crate::id_codes::expected_for_version(&block.code, version)
    } else {
        crate::id_codes::expected(&block.code)
    }
    .ok_or_else(|| Error::new("owner_header", "unqualified native/legacy ID code"))?;
    let canonical = if write {
        crate::id_codes::canonical_for_version(actual, version)
    } else {
        crate::id_codes::canonical(actual)
    };
    if !expected.contains(&canonical) {
        return Err(Error::new(
            "owner_header",
            format!(
                "code {:?} expects {:?}, got {}",
                block.code,
                expected
                    .iter()
                    .map(|t| String::from_utf8_lossy(t))
                    .collect::<Vec<_>>(),
                String::from_utf8_lossy(actual)
            ),
        ));
    }
    let prefix = if actual == b"ID" {
        Vec::new()
    } else {
        vec![PathStep::Field(b"id".to_vec())]
    };
    let mut path = prefix.clone();
    path.push(PathStep::Field(b"name".to_vec()));
    let Value::Bytes(name) = f.get(b, 0, &path, false)? else {
        return Err(Error::new("owner_header", "ID.name is not bytes"));
    };
    if name.len() < 2 {
        return Err(Error::new(
            "owner_header",
            "ID name has no two-byte kind prefix",
        ));
    }
    if actual == b"ID" {
        let mut p = prefix;
        p.push(PathStep::Field(b"lib".to_vec()));
        if !crate::typed_edit::owner_code(&[name[0], name[1], 0, 0])
            || !matches!(f.get(b,0,&p,false)?,Value::Pointer(n)if n!=0)
        {
            return Err(Error::new(
                "owner_header",
                "bare ID placeholder lacks qualified kind/library",
            ));
        }
    } else if !name.starts_with(if write {
        crate::id_codes::canonical_code_for_version(&block.code[..2], version)
    } else {
        crate::id_codes::canonical_code(&block.code[..2])
    }) {
        return Err(Error::new(
            "owner_header",
            "ID name prefix and BHead code disagree",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod read_id_tests {
    use super::*;
    #[test]
    fn explicit_id_code_uses_source_struct_zero_without_write_admission() {
        let f = BlendFile::from_bytes(
            include_bytes!("../../tests/fixtures/dependencies/group-root.blend").to_vec(),
        )
        .unwrap();
        assert_eq!(f.blocks()[0].sdna_index, 0);
        qualify_read_id(&f, 0).unwrap();
        assert_eq!(qualify(&f).unwrap_err().kind, "owner_header");
        let raw = BlendFile::from_bytes(
            include_bytes!("../../tests/fixtures/dependencies/spline-ik.blend").to_vec(),
        )
        .unwrap();
        assert_eq!(raw.blocks()[3].code, *b"DATA");
        assert!(qualify_read_id(&raw, 3).is_err());
    }
}
