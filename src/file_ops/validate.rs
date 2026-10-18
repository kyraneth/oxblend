//! Checked site validation for import/receiving verification, and diagnostic
//! census classification. Diagnostics never expand the checked read profile.
use super::{PointerLayout, roles};
use crate::{BlendFile, Error, Result};
pub(crate) fn census_site(
    f: &BlendFile,
    b: usize,
    p: &PointerLayout,
    address: u64,
) -> Result<Option<usize>> {
    if address != 0
        && matches!(
            (p.parent_type.as_slice(), p.field_name.as_slice()),
            (b"SpaceNode", b"nodetree" | b"edittree") | (b"bNodeTreePath", b"nodetree")
        )
    {
        let target =
            f.owner_index()?
                .lookup(f, b, address, &p.path, super::index::Namespace::Id)?;
        roles::qualify(f, b, p, target)?;
        return Ok(Some(target));
    }
    if address != 0
        && (p.parent_type == b"PreviewImage" && p.field_name == b"rect"
            || p.declared_type == b"char")
    {
        let target = f.owner_index()?.resolve(f, b, address, p)?;
        if p.parent_type == b"PreviewImage"
            || f.block(target)?.sdna_index != 0 && f.dna_type_id(target)? != p.declared_type
        {
            return Err(Error::new("pointer_target_type", p.path.clone()));
        }
    }
    let unprofiled = match (p.parent_type.as_slice(), p.field_name.as_slice()) {
        (b"bScreen", b"context")
        | (b"PackedFile", b"data")
        | (b"bNodeTreeInterfaceSocket", b"socket_data")
        | (b"bConstraint", b"data") => p.declared_type == b"void",
        (b"ListBase", b"first" | b"last") => p.list_role.as_ref().is_some_and(|(ty, field)| {
            matches!(
                (ty.as_slice(), field.as_slice()),
                (b"ARegion", b"view_states")
                    | (b"ViewLayer", b"object_bases")
                    | (b"Object", b"modifiers")
                    | (b"Object", b"constraints")
                    | (b"bPoseChannel", b"constraints")
                    | (b"ClothModifierData", b"ptcaches")
                    | (b"SoftBody", b"ptcaches")
                    | (b"SoftBody_Shared", b"ptcaches")
                    | (b"DynamicPaintCanvasSettings", b"ptcaches")
                    | (b"DynamicPaintSurface", b"ptcaches")
                    | (b"FluidDomainSettings", b"ptcaches")
                    | (b"ParticleSystem", b"ptcaches")
            )
        }),
        _ => false,
    };
    let numeric = p.parent_type == b"Mesh"
        && p.field_name == b"poly_offset_indices"
        && p.declared_type == b"int"
        && p.pointer_depth == 1;
    if !unprofiled && !numeric {
        if address != 0 && roles::empty_allocation(f, b, p).unwrap_or(false) {
            let index = f.owner_index()?;
            index.local(f, b, 0)?;
            index.census_data(f, b, address, &p.path)?;
        }
        return match site(f, b, p, address) {
            Err(e)
                if e.kind == "unprofiled_role"
                    && address != 0
                    && p.parent_type == b"ListBase"
                    && matches!(p.field_name.as_slice(), b"first" | b"last") =>
            {
                match f.owner_index()?.census_data(f, b, address, &p.path) {
                    Err(resolution) if resolution.kind == "unresolved_persistent" => {
                        Err(resolution)
                    }
                    _ => Err(e),
                }
            }
            other => other,
        };
    }
    let index = f.owner_index()?;
    index.local(f, b, 0)?;
    if p.parent_type == b"ListBase" && p.field_name == b"first" {
        let fields = super::pointer_layout(f, b)?
            .ok_or_else(|| Error::new("pointer_role", "opaque list source"))?;
        let last = fields
            .iter()
            .find(|q| {
                q.parent_offset == p.parent_offset
                    && q.parent_type == b"ListBase"
                    && q.field_name == b"last"
            })
            .ok_or_else(|| Error::new("pointer_role", "list last absent"))?;
        let end = crate::checked::number(
            &f.payload(b)?,
            last.offset,
            f.header().pointer_size,
            f.header().little_endian,
        )?;
        if (address == 0) != (end == 0) {
            return Err(Error::new("list_boundary", "first/last nullness mismatch"));
        }
        if address != 0 {
            return Err(Error::new("unprofiled_role", p.path.clone()));
        }
    }
    if address == 0 {
        return Ok(None);
    }
    let target = if numeric {
        index.resolve(f, b, address, p)?
    } else {
        index.census_data(f, b, address, &p.path)?
    };
    if unprofiled {
        return Err(Error::new("unprofiled_role", p.path.clone()));
    }
    if f.block(target)?.sdna_index == 0 {
        return Err(Error::new("pointer_target_type", p.path.clone()));
    }
    roles::qualify(f, b, p, target)?;
    Ok(Some(target))
}
pub(crate) fn write_site(
    f: &BlendFile,
    b: usize,
    p: &PointerLayout,
    address: u64,
) -> Result<Option<usize>> {
    if address != 0
        && p.parent_type == b"Scene"
        && p.field_name == b"nodetree"
        && f.header().version != 405
    {
        return Err(Error::new(
            "unprofiled_role",
            "unqualified embedded-ID version",
        ));
    }
    site_impl(f, b, p, address, true)
}
pub(crate) fn site(
    f: &BlendFile,
    b: usize,
    p: &PointerLayout,
    address: u64,
) -> Result<Option<usize>> {
    site_impl(f, b, p, address, false)
}
fn site_impl(
    f: &BlendFile,
    b: usize,
    p: &PointerLayout,
    address: u64,
    write: bool,
) -> Result<Option<usize>> {
    // Touching one site admits this native namespace, never a corrupt partial map.
    f.owner_index()?.local(f, b, 0)?;
    // Absence is declared by the qualified saved element count, not inferred
    // from an address failing to resolve. Keep namespace ambiguity admission
    // above this decision, including for a retained unsaved empty-array handle.
    if (!write || f.header().version == 502) && roles::empty_allocation(f, b, p)? {
        return Ok(None);
    }
    if p.parent_type == b"ListBase" && p.field_name == b"first" {
        if write {
            roles::write_validate_list(f, b, p)?;
        } else {
            roles::validate_list(f, b, p)?;
        }
    }
    if address == 0 {
        return if roles::required(p) {
            Err(Error::new("required_pointer_null", p.path.clone()))
        } else {
            Ok(None)
        };
    }
    let target = f.owner_index()?.resolve(f, b, address, p)?;
    if write {
        roles::write_qualify(f, b, p, target)?;
    } else {
        roles::qualify(f, b, p, target)?;
    }
    Ok(Some(target))
}
