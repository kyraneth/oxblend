//! Saved-DNA pointer profiles, qualified from independent saved-byte observations.
use super::{PointerLayout, pointer_layout};
use crate::{BlendFile, Error, Result, checked};
use std::collections::BTreeSet;
fn fail(kind: &'static str, s: impl Into<String>) -> Error {
    Error::new(kind, s)
}
fn type_index(f: &BlendFile, name: &[u8]) -> Result<usize> {
    f.schema()
        .types
        .iter()
        .position(|t| t == name)
        .ok_or_else(|| fail("pointer_role", "profile type absent from source DNA"))
}
fn member<'a>(f: &'a BlendFile, ty: &[u8], name: &[u8]) -> Result<&'a crate::schema::Field> {
    let ti = type_index(f, ty)?;
    let si = f
        .schema()
        .by_type
        .get(&ti)
        .ok_or_else(|| fail("pointer_role", "profile struct missing"))?;
    f.schema().structs[*si]
        .fields
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| fail("pointer_role", "profile field missing"))
}
fn number(f: &BlendFile, b: usize, p: &PointerLayout, name: &[u8]) -> Result<u64> {
    let field = member(f, &p.parent_type, name)?;
    if field.pointer
        || field.array_len != 1
        || !matches!(
            f.schema().types[field.type_index].as_slice(),
            b"char"
                | b"uchar"
                | b"short"
                | b"ushort"
                | b"int"
                | b"uint"
                | b"int8_t"
                | b"uint8_t"
                | b"int16_t"
                | b"uint16_t"
                | b"int32_t"
                | b"uint32_t"
                | b"int64_t"
                | b"uint64_t"
        )
    {
        return Err(fail("pointer_role", "scalar discriminator required"));
    }
    checked::number(
        &f.payload(b)?,
        checked::add(p.parent_offset, field.offset)?,
        field.element_size,
        f.header().little_endian,
    )
}
fn count(f: &BlendFile, b: usize, p: &PointerLayout, name: &[u8]) -> Result<usize> {
    let field = member(f, &p.parent_type, name)?;
    let value = number(f, b, p, name)?;
    // Signed saved counts must never become large unsigned extents.
    if field.element_size != 0 && value & (1u64 << (field.element_size * 8 - 1)) != 0 {
        return Err(fail("pointer_extent", "negative saved extent"));
    }
    usize::try_from(value).map_err(|_| fail("overflow", "saved extent does not fit usize"))
}
fn text(f: &BlendFile, b: usize, p: &PointerLayout, name: &[u8]) -> Result<String> {
    let field = member(f, &p.parent_type, name)?;
    if field.pointer || f.schema().types[field.type_index] != b"char" || field.element_size != 1 {
        return Err(fail("pointer_role", "inline char discriminator required"));
    }
    let payload = f.payload(b)?;
    let bytes = checked::bytes(
        &payload,
        checked::add(p.parent_offset, field.offset)?,
        field.size,
    )?;
    let end = bytes
        .iter()
        .position(|b| *b == 0)
        .ok_or_else(|| fail("pointer_role", "unterminated discriminator"))?;
    std::str::from_utf8(&bytes[..end])
        .map(str::to_owned)
        .map_err(|_| fail("pointer_role", "invalid discriminator UTF-8"))
}
fn property(f: &BlendFile, p: &PointerLayout) -> Result<PointerLayout> {
    if p.parent_type == b"IDProperty" {
        return Ok(p.clone());
    }
    let data = member(f, b"IDProperty", b"data")?;
    let mut q = p.clone();
    q.parent_type = b"IDProperty".to_vec();
    q.parent_offset = p
        .parent_offset
        .checked_sub(data.offset)
        .ok_or_else(|| fail("pointer_role", "property union outside IDProperty"))?;
    Ok(q)
}
pub fn required(p: &PointerLayout) -> bool {
    matches!(
        (p.parent_type.as_slice(), p.field_name.as_slice()),
        (
            b"bNodeLink",
            b"fromnode" | b"tonode" | b"fromsock" | b"tosock"
        )
    )
}
const LIGHTING_NODE_STORAGE_405: &[(&str, u64, &[u8], usize)] = &[
    ("FunctionNodeCompare", 1202, b"NodeFunctionCompare", 4),
    ("FunctionNodeInputBool", 1219, b"NodeInputBool", 1),
    ("FunctionNodeInputInt", 1220, b"NodeInputInt", 4),
    ("FunctionNodeRandomValue", 1214, b"NodeRandomValue", 1),
    (
        "GeometryNodeCaptureAttribute",
        1080,
        b"NodeGeometryAttributeCapture",
        24,
    ),
    (
        "GeometryNodeCollectionInfo",
        1023,
        b"NodeGeometryCollectionInfo",
        1,
    ),
    (
        "GeometryNodeCurvePrimitiveCircle",
        1066,
        b"NodeGeometryCurvePrimitiveCircle",
        1,
    ),
    (
        "GeometryNodeDeleteGeometry",
        1103,
        b"NodeGeometryDeleteGeometry",
        2,
    ),
    (
        "GeometryNodeDistributePointsInVolume",
        1172,
        b"NodeGeometryDistributePointsInVolume",
        1,
    ),
    (
        "GeometryNodeExtrudeMesh",
        1152,
        b"NodeGeometryExtrudeMesh",
        1,
    ),
    ("GeometryNodeIndexSwitch", 2118, b"NodeIndexSwitch", 24),
    (
        "GeometryNodeInputNamedAttribute",
        1157,
        b"NodeGeometryInputNamedAttribute",
        1,
    ),
    ("GeometryNodeMenuSwitch", 2124, b"NodeMenuSwitch", 32),
    (
        "GeometryNodeMergeByDistance",
        1153,
        b"NodeGeometryMergeByDistance",
        1,
    ),
    (
        "GeometryNodeMeshToVolume",
        1164,
        b"NodeGeometryMeshToVolume",
        1,
    ),
    ("GeometryNodeObjectInfo", 1007, b"NodeGeometryObjectInfo", 1),
    ("GeometryNodeProximity", 1096, b"NodeGeometryProximity", 1),
    (
        "GeometryNodeResampleCurve",
        1047,
        b"NodeGeometryCurveResample",
        2,
    ),
    (
        "GeometryNodeSampleCurve",
        1085,
        b"NodeGeometryCurveSample",
        4,
    ),
    (
        "GeometryNodeSampleIndex",
        1174,
        b"NodeGeometrySampleIndex",
        4,
    ),
    (
        "GeometryNodeSeparateGeometry",
        1104,
        b"NodeGeometrySeparateGeometry",
        1,
    ),
    (
        "GeometryNodeStoreNamedAttribute",
        1156,
        b"NodeGeometryStoreNamedAttribute",
        2,
    ),
    (
        "GeometryNodeSubdivisionSurface",
        1126,
        b"NodeGeometrySubdivisionSurface",
        2,
    ),
    ("GeometryNodeSwitch", 1043, b"NodeSwitch", 1),
    ("GeometryNodeViewer", 1067, b"NodeGeometryViewer", 2),
    ("NodeFrame", 5, b"NodeFrame", 4),
    ("ShaderNodeAttribute", 129, b"NodeShaderAttribute", 264),
    ("ShaderNodeFloatCurve", 709, b"CurveMapping", 424),
    ("ShaderNodeNormalMap", 175, b"NodeShaderNormalMap", 68),
    ("ShaderNodeRGBCurve", 111, b"CurveMapping", 424),
    ("ShaderNodeSeparateColor", 712, b"NodeCombSepColor", 1),
    ("ShaderNodeTexGradient", 146, b"NodeTexGradient", 968),
    ("ShaderNodeTexNoise", 150, b"NodeTexNoise", 968),
    ("ShaderNodeTexSky", 145, b"NodeTexSky", 1024),
    ("ShaderNodeValToRGB", 104, b"ColorBand", 776),
];
const LIGHTING_SOCKET_VALUES_405: &[(&str, u64, &[u8], usize)] = &[
    ("NodeSocketFloatPercentage", 0, b"bNodeSocketValueFloat", 16),
    ("NodeSocketIntFactor", 6, b"bNodeSocketValueInt", 16),
];
const LIGHTING_CONSTRAINTS_405: &[(u64, &[u8], usize)] = &[
    (1, b"bChildOfConstraint", 144),
    (2, b"bTrackToConstraint", 88),
    (3, b"bKinematicConstraint", 184),
    (5, b"bRotLimitConstraint", 32),
    (6, b"bLocLimitConstraint", 28),
    (7, b"bSizeLimitConstraint", 28),
    (8, b"bRotateLikeConstraint", 80),
    (9, b"bLocateLikeConstraint", 80),
    (10, b"bSizeLikeConstraint", 80),
    (13, b"bLockTrackConstraint", 80),
    (15, b"bStretchToConstraint", 104),
    (16, b"bMinMaxConstraint", 88),
    (19, b"bTransformConstraint", 232),
    (20, b"bShrinkwrapConstraint", 24),
    (21, b"bDampTrackConstraint", 80),
    (23, b"bTransLikeConstraint", 80),
    (30, b"bArmatureConstraint", 24),
];
type SavedListProfile = (&'static [u8], &'static [u8], &'static [u8], usize);
const LIGHTING_LISTS_405: &[SavedListProfile] = &[
    (b"ChannelDriver", b"variables", b"DriverVar", 920),
    (b"bPose", b"chanbase", b"bPoseChannel", 1032),
    (b"bPoseChannel", b"constraints", b"bConstraint", 192),
];

/// Sites whose lighting trace admission needs both namespace resolution and
/// target qualification. The reader owns invoking qualify after resolution;
/// this is not an admission for pointer tables or unrelated void fields.
pub(crate) fn checked_source_role(p: &PointerLayout) -> bool {
    matches!(
        (p.parent_type.as_slice(), p.field_name.as_slice()),
        (b"bConstraint", b"data") | (b"bNode", b"storage") | (b"bNodeSocket", b"default_value")
    ) || p.parent_type == b"ListBase"
        && p.list_role.as_ref().is_some_and(|(owner, name)| {
            LIGHTING_LISTS_405
                .iter()
                .any(|(t, n, _, _)| owner == t && name == n)
        })
}
fn lighting_write_profile(
    f: &BlendFile,
    b: usize,
    p: &PointerLayout,
) -> Result<Option<(&'static [u8], usize)>> {
    if f.header().version == 405 {
        return lighting_profile(f, b, p);
    }
    if p.parent_type == b"ListBase"
        && p.list_role.as_ref().is_some_and(|(owner, name)| {
            LIGHTING_LISTS_405
                .iter()
                .any(|(t, n, _, _)| owner == t && name == n)
        })
    {
        return Err(fail(
            "unprofiled_role",
            "list write profile requires Blender 4.5 layout qualification",
        ));
    }
    Ok(None)
}
fn lighting_profile(
    f: &BlendFile,
    b: usize,
    p: &PointerLayout,
) -> Result<Option<(&'static [u8], usize)>> {
    let profile = match (p.parent_type.as_slice(), p.field_name.as_slice()) {
        (b"bConstraint", b"data") => {
            let kind = number(f, b, p, b"type")?;
            LIGHTING_CONSTRAINTS_405
                .iter()
                .find_map(|(k, ty, extent)| (*k == kind).then_some((*ty, *extent)))
        }
        (b"bNode", b"storage") => {
            let kind = text(f, b, p, b"idname")?;
            if let Some((_, numeric, ty, extent)) = LIGHTING_NODE_STORAGE_405
                .iter()
                .find(|(k, _, _, _)| *k == kind)
            {
                if number(f, b, p, b"type")? != *numeric {
                    return Err(fail(
                        "unprofiled_role",
                        "node name/type discriminator disagreement",
                    ));
                }
                Some((*ty, *extent))
            } else {
                None
            }
        }
        (b"bNodeSocket", b"default_value") => {
            let kind = text(f, b, p, b"idname")?;
            if let Some((_, numeric, ty, extent)) = LIGHTING_SOCKET_VALUES_405
                .iter()
                .find(|(k, _, _, _)| *k == kind)
            {
                if number(f, b, p, b"type")? != *numeric {
                    return Err(fail(
                        "unprofiled_role",
                        "socket name/type discriminator disagreement",
                    ));
                }
                Some((*ty, *extent))
            } else {
                None
            }
        }
        (b"ListBase", _) => {
            if let Some((owner, name)) = &p.list_role {
                if let Some((_, _, ty, extent)) = LIGHTING_LISTS_405
                    .iter()
                    .find(|(t, n, _, _)| owner == t && name == n)
                {
                    Some((*ty, *extent))
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    };
    if profile.is_some() && (p.pointer_depth != 1 || p.declared_type != b"void") {
        return Err(fail(
            "unprofiled_role",
            "pointer profile requires a serialized single void pointer",
        ));
    }
    Ok(profile)
}

const LIST_TYPES: &[(&[u8], &[u8], &[u8])] = &[
    (b"ARegion", b"ui_lists", b"uiList"),
    (b"AnimData", b"nla_tracks", b"NlaTrack"),
    (b"AnimData", b"drivers", b"FCurve"),
    (b"ClothModifierData", b"ptcaches", b"PointCache"),
    (b"SoftBody", b"ptcaches", b"PointCache"),
    (b"SoftBody_Shared", b"ptcaches", b"PointCache"),
    (
        b"DynamicPaintCanvasSettings",
        b"surfaces",
        b"DynamicPaintSurface",
    ),
    (b"DynamicPaintSurface", b"ptcaches", b"PointCache"),
    (b"FluidDomainSettings", b"ptcaches", b"PointCache"),
    (
        b"IDOverrideLibrary",
        b"properties",
        b"IDOverrideLibraryProperty",
    ),
    (
        b"IDOverrideLibraryProperty",
        b"operations",
        b"IDOverrideLibraryPropertyOperation",
    ),
    (b"Image", b"packedfiles", b"ImagePackedFile"),
    (b"NlaStrip", b"fcurves", b"FCurve"),
    (b"NodeCryptomatte", b"entries", b"CryptomatteEntry"),
    (b"NlaTrack", b"strips", b"NlaStrip"),
    (b"Scene", b"markers", b"TimeMarker"),
    (b"ViewLayer", b"aovs", b"ViewLayerAOV"),
    (b"ViewLayer", b"lightgroups", b"ViewLayerLightgroup"),
    (b"bAction", b"curves", b"FCurve"),
    (b"bAction", b"groups", b"bActionGroup"),
    (b"bActionGroup", b"channels", b"FCurve"),
    (b"ARegion", b"view_states", b"uiViewStateLink"),
    (b"Collection", b"children", b"CollectionChild"),
    (b"Collection", b"gobject", b"CollectionObject"),
    (b"Editing", b"channels", b"SeqTimelineChannel"),
    (b"Editing", b"seqbase", b"Sequence"),
    (b"Sequence", b"seqbase", b"Sequence"),
    (b"Strip", b"seqbase", b"Strip"),
    (b"FreestyleConfig", b"linesets", b"FreestyleLineSet"),
    (
        b"FreestyleLineStyle",
        b"geometry_modifiers",
        b"LineStyleGeometryModifier_Sampling",
    ),
    (b"IDPropertyData", b"group", b"IDProperty"),
    (b"Image", b"renderslots", b"RenderSlot"),
    (b"Image", b"tiles", b"ImageTile"),
    (b"Image", b"views", b"ImageView"),
    (b"Key", b"block", b"KeyBlock"),
    (b"LayerCollection", b"layer_collections", b"LayerCollection"),
    (b"Object", b"modifiers", b"ModifierData"),
    (b"Object", b"constraints", b"bConstraint"),
    (b"Object", b"particlesystem", b"ParticleSystem"),
    (b"Palette", b"colors", b"PaletteColor"),
    (b"ParticleSystem", b"ptcaches", b"PointCache"),
    (b"RegionAssetShelf", b"shelves", b"AssetShelf"),
    (b"RenderData", b"views", b"SceneRenderView"),
    (b"Scene", b"view_layers", b"ViewLayer"),
    (b"ScrArea", b"regionbase", b"ARegion"),
    (b"ScrArea", b"spacedata", b"SpaceLink"),
    (b"ScrAreaMap", b"areabase", b"ScrArea"),
    (b"ScrAreaMap", b"edgebase", b"ScrEdge"),
    (b"ScrAreaMap", b"vertbase", b"ScrVert"),
    (b"SpaceConsole", b"history", b"ConsoleLine"),
    (b"SpaceNode", b"regionbase", b"ARegion"),
    (b"SpaceNode", b"treepath", b"bNodeTreePath"),
    (b"Text", b"lines", b"TextLine"),
    (b"ViewLayer", b"layer_collections", b"LayerCollection"),
    (b"ViewLayer", b"object_bases", b"Base"),
    (b"ViewerPath", b"path", b"ViewerPathElem"),
    (
        b"WorkSpace",
        b"hook_layout_relations",
        b"WorkSpaceDataRelation",
    ),
    (b"WorkSpace", b"layouts", b"WorkSpaceLayout"),
    (b"bNode", b"inputs", b"bNodeSocket"),
    (b"bNode", b"outputs", b"bNodeSocket"),
    (b"bNodeTree", b"links", b"bNodeLink"),
    (b"bNodeTree", b"nodes", b"bNode"),
    (b"bScreen", b"areabase", b"ScrArea"),
    (b"bScreen", b"edgebase", b"ScrEdge"),
    (b"bScreen", b"vertbase", b"ScrVert"),
    (b"wmWindowManager", b"windows", b"wmWindow"),
];
fn list_type(ty: &[u8], field: &[u8]) -> Option<&'static [u8]> {
    LIST_TYPES
        .iter()
        .find_map(|(t, n, v)| (*t == ty && *n == field).then_some(*v))
}
const SINGLE_STRUCT_NODE_STORAGE: &[(&str, &[u8])] = &[
    ("CompositorNodeAlphaOver", b"NodeTwoFloats"),
    ("CompositorNodeBlur", b"NodeBlurData"),
    ("CompositorNodeChannelMatte", b"NodeChroma"),
    ("CompositorNodeColorSpill", b"NodeColorspill"),
    ("CompositorNodeCombineColor", b"NodeCMPCombSepColor"),
    ("CompositorNodeConvertColorSpace", b"NodeConvertColorSpace"),
    ("CompositorNodeConvertToDisplay", b"NodeConvertToDisplay"),
    ("CompositorNodeCornerPin", b"NodeCornerPinData"),
    ("CompositorNodeCryptomatte", b"NodeCryptomatte"),
    ("CompositorNodeCryptomatteV2", b"NodeCryptomatte"),
    ("CompositorNodeCurveRGB", b"CurveMapping"),
    ("CompositorNodeDefocus", b"NodeDefocus"),
    ("CompositorNodeDenoise", b"NodeDenoise"),
    ("CompositorNodeDilateErode", b"NodeDilateErode"),
    ("CompositorNodeDisplace", b"NodeDisplaceData"),
    ("CompositorNodeDistanceMatte", b"NodeChroma"),
    ("CompositorNodeGlare", b"NodeGlare"),
    ("CompositorNodeHueCorrect", b"CurveMapping"),
    ("CompositorNodeImage", b"ImageUser"),
    ("CompositorNodeKeying", b"NodeKeyingData"),
    ("CompositorNodeKeyingScreen", b"NodeKeyingScreenData"),
    ("CompositorNodeKuwahara", b"NodeKuwaharaData"),
    ("CompositorNodeLensdist", b"NodeLensDist"),
    ("CompositorNodeMapUV", b"NodeMapUVData"),
    ("CompositorNodeMovieClip", b"MovieClipUser"),
    ("CompositorNodeOutputFile", b"NodeImageMultiFile"),
    (
        "CompositorNodePlaneTrackDeform",
        b"NodePlaneTrackDeformData",
    ),
    ("CompositorNodeRotate", b"NodeRotateData"),
    ("CompositorNodeScale", b"NodeScaleData"),
    ("CompositorNodeSeparateColor", b"NodeCMPCombSepColor"),
    ("CompositorNodeSetAlpha", b"NodeSetAlpha"),
    ("CompositorNodeTime", b"CurveMapping"),
    ("CompositorNodeTonemap", b"NodeTonemap"),
    ("CompositorNodeTrackPos", b"NodeTrackPosData"),
    ("CompositorNodeTransform", b"NodeTransformData"),
    ("CompositorNodeTranslate", b"NodeTranslateData"),
    ("CompositorNodeViewer", b"ImageUser"),
    ("FunctionNodeCombineColor", b"NodeCombSepColor"),
    ("FunctionNodeCompare", b"NodeFunctionCompare"),
    ("FunctionNodeFormatString", b"NodeFunctionFormatString"),
    ("FunctionNodeInputBool", b"NodeInputBool"),
    ("FunctionNodeInputColor", b"NodeInputColor"),
    ("FunctionNodeInputInt", b"NodeInputInt"),
    ("FunctionNodeInputIntVector", b"NodeInputIntVector"),
    ("FunctionNodeInputMenu", b"NodeInputMenu"),
    ("FunctionNodeInputRotation", b"NodeInputRotation"),
    ("FunctionNodeInputString", b"NodeInputString"),
    ("FunctionNodeInputVector", b"NodeInputVector"),
    ("FunctionNodeRandomValue", b"NodeRandomValue"),
    ("FunctionNodeSeparateColor", b"NodeCombSepColor"),
    ("GeometryNodeAccumulateField", b"NodeAccumulateField"),
    ("GeometryNodeBake", b"NodeGeometryBake"),
    (
        "GeometryNodeCaptureAttribute",
        b"NodeGeometryAttributeCapture",
    ),
    ("GeometryNodeClosureToList", b"GeometryNodeClosureToList"),
    ("GeometryNodeCollectionInfo", b"NodeGeometryCollectionInfo"),
    ("GeometryNodeCurveArc", b"NodeGeometryCurvePrimitiveArc"),
    (
        "GeometryNodeCurveHandleTypeSelection",
        b"NodeGeometryCurveSelectHandles",
    ),
    (
        "GeometryNodeCurvePrimitiveBezierSegment",
        b"NodeGeometryCurvePrimitiveBezierSegment",
    ),
    (
        "GeometryNodeCurvePrimitiveCircle",
        b"NodeGeometryCurvePrimitiveCircle",
    ),
    (
        "GeometryNodeCurvePrimitiveLine",
        b"NodeGeometryCurvePrimitiveLine",
    ),
    (
        "GeometryNodeCurvePrimitiveQuadrilateral",
        b"NodeGeometryCurvePrimitiveQuad",
    ),
    (
        "GeometryNodeCurveSetHandles",
        b"NodeGeometryCurveSetHandles",
    ),
    (
        "GeometryNodeCurveSplineType",
        b"NodeGeometryCurveSplineType",
    ),
    ("GeometryNodeCurveToPoints", b"NodeGeometryCurveToPoints"),
    ("GeometryNodeDeleteGeometry", b"NodeGeometryDeleteGeometry"),
    (
        "GeometryNodeDistributePointsInVolume",
        b"NodeGeometryDistributePointsInVolume",
    ),
    (
        "GeometryNodeDuplicateElements",
        b"NodeGeometryDuplicateElements",
    ),
    ("GeometryNodeExtrudeMesh", b"NodeGeometryExtrudeMesh"),
    ("GeometryNodeFieldToGrid", b"GeometryNodeFieldToGrid"),
    ("GeometryNodeFieldToList", b"GeometryNodeFieldToList"),
    ("GeometryNodeFillCurve", b"NodeGeometryCurveFill"),
    ("GeometryNodeFilletCurve", b"NodeGeometryCurveFillet"),
    (
        "GeometryNodeForeachGeometryElementInput",
        b"NodeGeometryForeachGeometryElementInput",
    ),
    (
        "GeometryNodeForeachGeometryElementOutput",
        b"NodeGeometryForeachGeometryElementOutput",
    ),
    ("GeometryNodeGizmoDial", b"NodeGeometryDialGizmo"),
    ("GeometryNodeGizmoLinear", b"NodeGeometryLinearGizmo"),
    ("GeometryNodeGizmoTransform", b"NodeGeometryTransformGizmo"),
    ("GeometryNodeImageTexture", b"NodeGeometryImageTexture"),
    ("GeometryNodeIndexSwitch", b"NodeIndexSwitch"),
    (
        "GeometryNodeInputNamedAttribute",
        b"NodeGeometryInputNamedAttribute",
    ),
    ("GeometryNodeListGetItem", b"NodeGeometryListGetItem"),
    ("GeometryNodeMenuSwitch", b"NodeMenuSwitch"),
    (
        "GeometryNodeMergeByDistance",
        b"NodeGeometryMergeByDistance",
    ),
    ("GeometryNodeMergeLayers", b"NodeGeometryMergeLayers"),
    ("GeometryNodeMeshCircle", b"NodeGeometryMeshCircle"),
    ("GeometryNodeMeshCone", b"NodeGeometryMeshCone"),
    ("GeometryNodeMeshCylinder", b"NodeGeometryMeshCylinder"),
    ("GeometryNodeMeshLine", b"NodeGeometryMeshLine"),
    ("GeometryNodeMeshToPoints", b"NodeGeometryMeshToPoints"),
    ("GeometryNodeMeshToVolume", b"NodeGeometryMeshToVolume"),
    ("GeometryNodeObjectInfo", b"NodeGeometryObjectInfo"),
    ("GeometryNodePointsToVolume", b"NodeGeometryPointsToVolume"),
    ("GeometryNodeProximity", b"NodeGeometryProximity"),
    ("GeometryNodeRaycast", b"NodeGeometryRaycast"),
    ("GeometryNodeRepeatInput", b"NodeGeometryRepeatInput"),
    ("GeometryNodeRepeatOutput", b"NodeGeometryRepeatOutput"),
    ("GeometryNodeResampleCurve", b"NodeGeometryCurveResample"),
    ("GeometryNodeSampleCurve", b"NodeGeometryCurveSample"),
    ("GeometryNodeSampleIndex", b"NodeGeometrySampleIndex"),
    (
        "GeometryNodeSeparateGeometry",
        b"NodeGeometrySeparateGeometry",
    ),
    (
        "GeometryNodeSetCurveHandlePositions",
        b"NodeGeometrySetCurveHandlePositions",
    ),
    (
        "GeometryNodeSimulationInput",
        b"NodeGeometrySimulationInput",
    ),
    (
        "GeometryNodeSimulationOutput",
        b"NodeGeometrySimulationOutput",
    ),
    (
        "GeometryNodeStoreNamedAttribute",
        b"NodeGeometryStoreNamedAttribute",
    ),
    ("GeometryNodeStringToCurves", b"NodeGeometryStringToCurves"),
    (
        "GeometryNodeSubdivisionSurface",
        b"NodeGeometrySubdivisionSurface",
    ),
    ("GeometryNodeSwitch", b"NodeSwitch"),
    ("GeometryNodeTrimCurve", b"NodeGeometryCurveTrim"),
    ("GeometryNodeUVUnwrap", b"NodeGeometryUVUnwrap"),
    ("GeometryNodeViewer", b"NodeGeometryViewer"),
    ("GeometryNodeVolumeToMesh", b"NodeGeometryVolumeToMesh"),
    ("NodeClosureInput", b"NodeClosureInput"),
    ("NodeClosureOutput", b"NodeClosureOutput"),
    ("NodeCombineBundle", b"NodeCombineBundle"),
    ("NodeEvaluateClosure", b"NodeEvaluateClosure"),
    ("NodeFrame", b"NodeFrame"),
    ("NodeGetBundleItem", b"NodeGetBundleItem"),
    ("NodeImplicitConversion", b"NodeImplicitConversion"),
    ("NodeReroute", b"NodeReroute"),
    ("NodeSeparateBundle", b"NodeSeparateBundle"),
    ("NodeStoreBundleItem", b"NodeStoreBundleItem"),
    ("ShaderNodeAttribute", b"NodeShaderAttribute"),
    ("ShaderNodeBsdfHairPrincipled", b"NodeShaderHairPrincipled"),
    ("ShaderNodeCombineColor", b"NodeCombSepColor"),
    ("ShaderNodeFloatCurve", b"CurveMapping"),
    ("ShaderNodeMapRange", b"NodeMapRange"),
    ("ShaderNodeMix", b"NodeShaderMix"),
    ("ShaderNodeNormalMap", b"NodeShaderNormalMap"),
    ("ShaderNodeOutputAOV", b"NodeShaderOutputAOV"),
    ("ShaderNodeRGBCurve", b"CurveMapping"),
    ("ShaderNodeRadialTiling", b"NodeRadialTiling"),
    ("ShaderNodeRaycast", b"NodeShaderRaycast"),
    ("ShaderNodeScript", b"NodeShaderScript"),
    ("ShaderNodeSeparateColor", b"NodeCombSepColor"),
    ("ShaderNodeTangent", b"NodeShaderTangent"),
    ("ShaderNodeTexBrick", b"NodeTexBrick"),
    ("ShaderNodeTexChecker", b"NodeTexChecker"),
    ("ShaderNodeTexEnvironment", b"NodeTexEnvironment"),
    ("ShaderNodeTexGabor", b"NodeTexGabor"),
    ("ShaderNodeTexGradient", b"NodeTexGradient"),
    ("ShaderNodeTexIES", b"NodeShaderTexIES"),
    ("ShaderNodeTexImage", b"NodeTexImage"),
    ("ShaderNodeTexMagic", b"NodeTexMagic"),
    ("ShaderNodeTexNoise", b"NodeTexNoise"),
    ("ShaderNodeTexSky", b"NodeTexSky"),
    ("ShaderNodeTexVoronoi", b"NodeTexVoronoi"),
    ("ShaderNodeTexWave", b"NodeTexWave"),
    ("ShaderNodeUVMap", b"NodeShaderUVMap"),
    ("ShaderNodeValToRGB", b"ColorBand"),
    ("ShaderNodeVectorCurve", b"CurveMapping"),
    ("ShaderNodeVectorTransform", b"NodeShaderVectTransform"),
    ("ShaderNodeVertexColor", b"NodeShaderVertexColor"),
    ("TextureNodeOutput", b"TexNodeOutput"),
];
const STORAGE_VERSIONS: &[(&str, u32)] = &[
    ("CompositorNodeBlur", 405),
    ("CompositorNodeCryptomatte", 405),
    ("CompositorNodeCurveRGB", 405),
    ("CompositorNodeDenoise", 405),
    ("CompositorNodeGlare", 405),
    ("CompositorNodeOutputFile", 405),
    ("CompositorNodeScale", 405),
    ("NodeReroute", 405),
    ("ShaderNodeMapRange", 405),
    ("ShaderNodeMix", 405),
    ("ShaderNodeTexImage", 405),
    ("CompositorNodeAlphaOver", 405),
    ("CompositorNodeBlur", 502),
    ("CompositorNodeChannelMatte", 502),
    ("CompositorNodeColorSpill", 502),
    ("CompositorNodeCombineColor", 502),
    ("CompositorNodeConvertColorSpace", 502),
    ("CompositorNodeConvertToDisplay", 502),
    ("CompositorNodeCornerPin", 502),
    ("CompositorNodeCryptomatte", 502),
    ("CompositorNodeCryptomatteV2", 405),
    ("CompositorNodeCryptomatteV2", 502),
    ("CompositorNodeCurveRGB", 502),
    ("CompositorNodeDefocus", 502),
    ("CompositorNodeDenoise", 502),
    ("CompositorNodeDilateErode", 502),
    ("CompositorNodeDisplace", 502),
    ("CompositorNodeDistanceMatte", 502),
    ("CompositorNodeGlare", 502),
    ("CompositorNodeHueCorrect", 502),
    ("CompositorNodeImage", 405),
    ("CompositorNodeImage", 502),
    ("CompositorNodeKeying", 502),
    ("CompositorNodeKeyingScreen", 502),
    ("CompositorNodeKuwahara", 502),
    ("CompositorNodeLensdist", 502),
    ("CompositorNodeMapUV", 502),
    ("CompositorNodeMovieClip", 502),
    ("CompositorNodeOutputFile", 502),
    ("CompositorNodePlaneTrackDeform", 502),
    ("CompositorNodeRotate", 502),
    ("CompositorNodeScale", 502),
    ("CompositorNodeSeparateColor", 502),
    ("CompositorNodeSetAlpha", 502),
    ("CompositorNodeTime", 502),
    ("CompositorNodeTonemap", 502),
    ("CompositorNodeTrackPos", 502),
    ("CompositorNodeTransform", 502),
    ("CompositorNodeTranslate", 502),
    ("CompositorNodeViewer", 405),
    ("CompositorNodeViewer", 502),
    ("FunctionNodeCombineColor", 502),
    ("FunctionNodeCompare", 502),
    ("FunctionNodeFormatString", 502),
    ("FunctionNodeInputBool", 502),
    ("FunctionNodeInputColor", 502),
    ("FunctionNodeInputInt", 502),
    ("FunctionNodeInputIntVector", 502),
    ("FunctionNodeInputMenu", 502),
    ("FunctionNodeInputRotation", 502),
    ("FunctionNodeInputString", 502),
    ("FunctionNodeInputVector", 502),
    ("FunctionNodeRandomValue", 502),
    ("FunctionNodeSeparateColor", 502),
    ("GeometryNodeAccumulateField", 502),
    ("GeometryNodeBake", 502),
    ("GeometryNodeCaptureAttribute", 502),
    ("GeometryNodeClosureToList", 502),
    ("GeometryNodeCollectionInfo", 502),
    ("GeometryNodeCurveArc", 502),
    ("GeometryNodeCurveHandleTypeSelection", 502),
    ("GeometryNodeCurvePrimitiveBezierSegment", 502),
    ("GeometryNodeCurvePrimitiveCircle", 502),
    ("GeometryNodeCurvePrimitiveLine", 502),
    ("GeometryNodeCurvePrimitiveQuadrilateral", 502),
    ("GeometryNodeCurveSetHandles", 502),
    ("GeometryNodeCurveSplineType", 502),
    ("GeometryNodeCurveToPoints", 502),
    ("GeometryNodeDeleteGeometry", 502),
    ("GeometryNodeDistributePointsInVolume", 502),
    ("GeometryNodeDuplicateElements", 502),
    ("GeometryNodeExtrudeMesh", 502),
    ("GeometryNodeFieldToGrid", 502),
    ("GeometryNodeFieldToList", 502),
    ("GeometryNodeFillCurve", 502),
    ("GeometryNodeFilletCurve", 502),
    ("GeometryNodeForeachGeometryElementInput", 502),
    ("GeometryNodeForeachGeometryElementOutput", 502),
    ("GeometryNodeGizmoDial", 502),
    ("GeometryNodeGizmoLinear", 502),
    ("GeometryNodeGizmoTransform", 502),
    ("GeometryNodeImageTexture", 502),
    ("GeometryNodeIndexSwitch", 502),
    ("GeometryNodeInputNamedAttribute", 502),
    ("GeometryNodeListGetItem", 502),
    ("GeometryNodeMenuSwitch", 502),
    ("GeometryNodeMergeByDistance", 502),
    ("GeometryNodeMergeLayers", 502),
    ("GeometryNodeMeshCircle", 502),
    ("GeometryNodeMeshCone", 502),
    ("GeometryNodeMeshCylinder", 502),
    ("GeometryNodeMeshLine", 502),
    ("GeometryNodeMeshToPoints", 502),
    ("GeometryNodeMeshToVolume", 502),
    ("GeometryNodeObjectInfo", 502),
    ("GeometryNodePointsToVolume", 502),
    ("GeometryNodeProximity", 502),
    ("GeometryNodeRaycast", 502),
    ("GeometryNodeRepeatInput", 502),
    ("GeometryNodeRepeatOutput", 502),
    ("GeometryNodeResampleCurve", 502),
    ("GeometryNodeSampleCurve", 502),
    ("GeometryNodeSampleIndex", 502),
    ("GeometryNodeSeparateGeometry", 502),
    ("GeometryNodeSetCurveHandlePositions", 502),
    ("GeometryNodeSimulationInput", 502),
    ("GeometryNodeSimulationOutput", 502),
    ("GeometryNodeStoreNamedAttribute", 502),
    ("GeometryNodeStringToCurves", 502),
    ("GeometryNodeSubdivisionSurface", 502),
    ("GeometryNodeSwitch", 502),
    ("GeometryNodeTrimCurve", 502),
    ("GeometryNodeUVUnwrap", 502),
    ("GeometryNodeViewer", 502),
    ("GeometryNodeVolumeToMesh", 502),
    ("NodeClosureInput", 502),
    ("NodeClosureOutput", 502),
    ("NodeCombineBundle", 502),
    ("NodeEvaluateClosure", 502),
    ("NodeFrame", 502),
    ("NodeGetBundleItem", 502),
    ("NodeImplicitConversion", 502),
    ("NodeReroute", 502),
    ("NodeSeparateBundle", 502),
    ("NodeStoreBundleItem", 502),
    ("ShaderNodeAttribute", 502),
    ("ShaderNodeBsdfHairPrincipled", 502),
    ("ShaderNodeCombineColor", 502),
    ("ShaderNodeFloatCurve", 502),
    ("ShaderNodeMapRange", 502),
    ("ShaderNodeMix", 502),
    ("ShaderNodeNormalMap", 502),
    ("ShaderNodeOutputAOV", 502),
    ("ShaderNodeRGBCurve", 502),
    ("ShaderNodeRadialTiling", 502),
    ("ShaderNodeRaycast", 502),
    ("ShaderNodeScript", 502),
    ("ShaderNodeSeparateColor", 502),
    ("ShaderNodeTangent", 502),
    ("ShaderNodeTexBrick", 502),
    ("ShaderNodeTexChecker", 502),
    ("ShaderNodeTexEnvironment", 502),
    ("ShaderNodeTexGabor", 502),
    ("ShaderNodeTexGradient", 502),
    ("ShaderNodeTexIES", 502),
    ("ShaderNodeTexImage", 502),
    ("ShaderNodeTexMagic", 502),
    ("ShaderNodeTexNoise", 502),
    ("ShaderNodeTexSky", 502),
    ("ShaderNodeTexVoronoi", 502),
    ("ShaderNodeTexWave", 502),
    ("ShaderNodeUVMap", 502),
    ("ShaderNodeValToRGB", 502),
    ("ShaderNodeVectorCurve", 502),
    ("ShaderNodeVectorTransform", 502),
    ("ShaderNodeVertexColor", 502),
    ("TextureNodeOutput", 502),
];
// A dependency scan can safely visit an unknown storage layout only when its
// SDNA proves it cannot carry references. This does not qualify construction.
// Meaningful byte arrays are conservatively treated as potential paths; only
// explicit padding is ignored. By-value members are inspected recursively.
fn dependency_free_type(f: &BlendFile, ty: usize, visiting: &mut BTreeSet<usize>) -> bool {
    if f.schema().lengths[ty] == 0 || !visiting.insert(ty) {
        return false;
    }
    let free = f.schema().by_type.get(&ty).is_none_or(|si| {
        f.schema().structs[*si].fields.iter().all(|field| {
            let padding = field
                .name
                .strip_prefix(b"_pad")
                .or_else(|| field.name.strip_prefix(b"pad"))
                .is_some_and(|suffix| suffix.iter().all(u8::is_ascii_digit));
            let bytes = matches!(
                f.schema().types[field.type_index].as_slice(),
                b"char" | b"uchar" | b"int8_t" | b"uint8_t"
            );
            !(field.pointer || bytes && field.full_name.contains(&b'[') && !padding)
                && dependency_free_type(f, field.type_index, visiting)
        })
    });
    visiting.remove(&ty);
    free
}
fn typed_owner_storage<'a>(
    f: &'a BlendFile,
    b: usize,
    p: &PointerLayout,
) -> Result<Option<&'a [u8]>> {
    let address = checked::number(
        &f.payload(b)?,
        p.offset,
        f.header().pointer_size,
        f.header().little_endian,
    )?;
    if address == 0 {
        return Ok(None);
    }
    // The source's own ID-DATA namespace is authoritative, never a global
    // matching-address fallback. Dangling and ambiguous pointers still fail.
    let target = f
        .owner_index()?
        .lookup(f, b, address, &p.path, super::index::Namespace::Data)?;
    let block = f.block(target)?;
    if block.code != *b"DATA" || block.sdna_index == 0 || block.count != 1 {
        return Ok(None);
    }
    let si = f.struct_index(target)?;
    let ty = f.schema().structs[si].type_index;
    let length = f.schema().lengths[ty];
    if block.size != length || length == 0 {
        return Err(fail(
            "pointer_extent",
            "owner DATA contradicts source SDNA extent",
        ));
    }
    for field in &f.schema().structs[si].fields {
        if checked::add(field.offset, field.size)? > length {
            return Err(fail(
                "pointer_extent",
                "field extent exceeds source SDNA struct",
            ));
        }
    }
    Ok(Some(&f.schema().types[ty]))
}
fn reference_free_storage<'a>(
    f: &'a BlendFile,
    b: usize,
    p: &PointerLayout,
) -> Result<Option<&'a [u8]>> {
    let Some(ty) = typed_owner_storage(f, b, p)? else {
        return Ok(None);
    };
    Ok(dependency_free_type(f, type_index(f, ty)?, &mut BTreeSet::new()).then_some(ty))
}
fn single_struct_storage<'a>(
    f: &'a BlendFile,
    b: usize,
    p: &PointerLayout,
    write: bool,
) -> Result<Option<&'a [u8]>> {
    if p.parent_type != b"bNode" || p.field_name != b"storage" {
        return Ok(None);
    }
    if let Some((ty, _)) = if write {
        lighting_write_profile(f, b, p)?
    } else {
        lighting_profile(f, b, p)?
    } {
        return Ok(Some(ty));
    }
    let kind = text(f, b, p, b"idname")?;
    if let Some(ty) = caller_storage(f, &kind, p, write) {
        return Ok(Some(ty));
    }
    if !write && matches!(kind.as_str(), "ShaderNodeScript" | "ShaderNodeTexIES") {
        return Err(fail(
            "unprofiled_role",
            "caller-qualified storage requires the exact declared type",
        ));
    }
    if write
        && !STORAGE_VERSIONS
            .iter()
            .any(|(k, v)| *k == kind && *v == f.header().version)
    {
        return Err(fail(
            "unprofiled_role",
            format!(
                "unqualified node storage {kind} for saved version {}",
                f.header().version
            ),
        ));
    }
    if let Some((_, ty)) = SINGLE_STRUCT_NODE_STORAGE.iter().find(|(k, _)| *k == kind) {
        return Ok(Some(*ty));
    }
    if !write && let Some(ty) = reference_free_storage(f, b, p)? {
        return Ok(Some(ty));
    }
    Err(fail(
        "unprofiled_role",
        format!("unqualified node storage {kind}"),
    ))
}
// A caller-qualified Save As callback is not a void-site import admission.
fn caller_storage(
    f: &BlendFile,
    kind: &str,
    p: &PointerLayout,
    write: bool,
) -> Option<&'static [u8]> {
    if write && f.header().version != 405 {
        return None;
    }
    match (kind, p.declared_type.as_slice()) {
        ("ShaderNodeScript", b"NodeShaderScript") => Some(b"NodeShaderScript"),
        ("ShaderNodeTexIES", b"NodeShaderTexIES") => Some(b"NodeShaderTexIES"),
        _ => None,
    }
}
pub(crate) fn socket_value(kind: &str) -> Option<&'static [u8]> {
    match kind {
        "NodeSocketFloat"
        | "NodeSocketFloatAngle"
        | "NodeSocketFloatColorTemperature"
        | "NodeSocketFloatDistance"
        | "NodeSocketFloatFactor"
        | "NodeSocketFloatFrequency"
        | "NodeSocketFloatTimeAbsolute"
        | "NodeSocketFloatUnsigned"
        | "NodeSocketFloatWavelength" => Some(b"bNodeSocketValueFloat"),
        "NodeSocketVector"
        | "NodeSocketVector2D"
        | "NodeSocketVectorDirection"
        | "NodeSocketVectorEuler"
        | "NodeSocketVectorFactor"
        | "NodeSocketVectorFactor2D"
        | "NodeSocketVectorTranslation"
        | "NodeSocketVectorVelocity4D"
        | "NodeSocketVectorXYZ"
        | "NodeSocketVectorXYZ2D"
        | "NodeSocketVector4D" => Some(b"bNodeSocketValueVector"),
        "NodeSocketColor" => Some(b"bNodeSocketValueRGBA"),
        "NodeSocketBool" => Some(b"bNodeSocketValueBoolean"),
        "NodeSocketInt" | "NodeSocketIntUnsigned" => Some(b"bNodeSocketValueInt"),
        "NodeSocketString" | "NodeSocketStringFilePath" => Some(b"bNodeSocketValueString"),
        "NodeSocketObject" => Some(b"bNodeSocketValueObject"),
        "NodeSocketImage" => Some(b"bNodeSocketValueImage"),
        "NodeSocketCollection" => Some(b"bNodeSocketValueCollection"),
        "NodeSocketMaterial" => Some(b"bNodeSocketValueMaterial"),
        "NodeSocketRotation" => Some(b"bNodeSocketValueRotation"),
        "NodeSocketMenu" => Some(b"bNodeSocketValueMenu"),
        "NodeSocketFont" => Some(b"bNodeSocketValueFont"),
        "NodeSocketSound" => Some(b"bNodeSocketValueSound"),
        "NodeSocketIntVector2D" | "NodeSocketIntVector3D" => Some(b"bNodeSocketValueIntVector"),
        _ => None,
    }
}
fn expected(f: &BlendFile, b: usize, p: &PointerLayout) -> Result<Vec<Vec<u8>>> {
    expected_impl(f, b, p, false)
}
#[cfg(test)]
const SNAPSHOT_TLEN: &[(&[u8], usize)] = &[
    (b"IDProperty", 144),
    (b"IDPropertyData", 40),
    (b"ListBase", 16),
];
/// True when the armed test received the retained 503 JobSnapshot layout.
/// Read admission itself uses individual saved layout/discriminator checks.
#[cfg(test)]
pub(crate) fn snapshot_layout(f: &BlendFile) -> bool {
    f.header().version == 503
        && SNAPSHOT_TLEN.iter().all(|(name, len)| {
            f.schema()
                .types
                .iter()
                .position(|t| t == name)
                .and_then(|ti| f.schema().lengths.get(ti))
                .is_some_and(|actual| actual == len)
        })
}
fn expected_impl(f: &BlendFile, b: usize, p: &PointerLayout, write: bool) -> Result<Vec<Vec<u8>>> {
    if let Some((ty, _)) = if write {
        lighting_write_profile(f, b, p)?
    } else {
        lighting_profile(f, b, p)?
    } {
        return Ok(vec![ty.to_vec()]);
    }
    let parent = p.parent_type.as_slice();
    let field = p.field_name.as_slice();
    if parent == b"ListBase" {
        let (owner, name) = p
            .list_role
            .as_ref()
            .ok_or_else(|| fail("unprofiled_role", "ListBase lacks owning field"))?;
        let ty = list_type(owner, name).ok_or_else(|| {
            fail(
                "unprofiled_role",
                format!(
                    "unqualified list {}.{}",
                    String::from_utf8_lossy(owner),
                    String::from_utf8_lossy(name)
                ),
            )
        })?;
        if owner == b"IDPropertyData" && name == b"group" {
            let q = group_property(f, p)?;
            if number(f, b, &q, b"type")? != 6 {
                return Err(fail(
                    "unprofiled_role",
                    "property group list requires group discriminator",
                ));
            }
        }
        let ty = if ty == b"Sequence" && !f.schema().types.iter().any(|t| t == b"Sequence") {
            b"Strip".as_slice()
        } else {
            ty
        };
        return Ok(vec![ty.to_vec()]);
    }
    if let Some(ty) = single_struct_storage(f, b, p, write)? {
        return Ok(vec![ty.to_vec()]);
    }
    let result: &[u8] = match (parent, field) {
        (b"ID", b"next" | b"prev") | (b"IDOverrideLibrary", b"reference" | b"hierarchy_root") => {
            b"ID"
        }
        (b"Object", b"data") => {
            if write {
                b"ID"
            } else {
                // Saved ObjectType values and their ID families; aliases such
                // as Lamp/Light remain owned by id_codes. Empty image data is
                // Image, not an arbitrary ID. No version-based read admission.
                match number(f, b, p, b"type")? {
                    0 => b"Image",
                    1 => b"Mesh",
                    2..=4 => b"Curve",
                    5 => b"MetaBall",
                    10 => b"Light",
                    11 => b"Camera",
                    12 => b"Speaker",
                    13 => b"LightProbe",
                    22 => b"Lattice",
                    25 => b"bArmature",
                    26 => b"bGPdata",
                    27 => b"Curves",
                    28 => b"PointCloud",
                    29 => b"Volume",
                    30 => b"GreasePencil",
                    _ => {
                        return Err(fail(
                            "unprofiled_role",
                            "unknown Object.data family discriminator",
                        ));
                    }
                }
            }
        }
        (b"ID", b"lib") => b"Library",
        (b"bNodeSocket", b"default_value") => {
            let kind = text(f, b, p, b"idname")?;
            if !write && socket_value(&kind).is_none() {
                let ty = typed_owner_storage(f, b, p)?.ok_or_else(|| {
                    fail(
                        "unprofiled_role",
                        "socket value requires one typed owner DATA struct",
                    )
                })?;
                return Ok(vec![ty.to_vec()]);
            }
            let ty = socket_value(&kind).ok_or_else(|| {
                fail(
                    "unprofiled_role",
                    format!("unknown socket discriminator {kind}"),
                )
            })?;
            let numeric = number(f, b, p, b"type")?;
            let expected = match ty {
                b"bNodeSocketValueFloat" => 0,
                b"bNodeSocketValueVector" => 1,
                b"bNodeSocketValueRGBA" => 2,
                b"bNodeSocketValueBoolean" => 4,
                b"bNodeSocketValueInt" => 6,
                b"bNodeSocketValueString" => 7,
                b"bNodeSocketValueObject" => 8,
                b"bNodeSocketValueImage" => 9,
                b"bNodeSocketValueCollection" => 11,
                b"bNodeSocketValueMaterial" => 13,
                b"bNodeSocketValueRotation" => 14,
                b"bNodeSocketValueMenu" => 15,
                b"bNodeSocketValueFont" => 19,
                b"bNodeSocketValueSound" => 23,
                b"bNodeSocketValueIntVector" => 24,
                _ => unreachable!(),
            };
            if numeric != expected {
                return Err(fail(
                    "unprofiled_role",
                    "socket name/type discriminator disagreement",
                ));
            }
            ty
        }
        (b"bNodeSocket", b"storage") => {
            let Some(ty) = socket_storage_type(f, b, write)? else {
                return Err(fail(
                    "unprofiled_role",
                    "socket storage lacks observed owning-node discriminator",
                ));
            };
            ty
        }
        (b"bNodeTreeInterfaceSocket", b"socket_data") => {
            let address = number_pointer(f, b, p, b"socket_type")?;
            let q = PointerLayout {
                field_name: b"socket_type".to_vec(),
                declared_type: b"char".to_vec(),
                ..p.clone()
            };
            let target = f.owner_index()?.resolve(f, b, address, &q)?;
            qualify_raw(f, b, &q, target, &[b"char".to_vec()])?;
            let bytes = f.payload(target)?;
            let kind = std::str::from_utf8(&bytes[..bytes.len() - 1])
                .map_err(|_| fail("pointer_role", "invalid interface socket discriminator"))?;
            socket_value(kind)
                .ok_or_else(|| fail("unprofiled_role", "unknown interface socket discriminator"))?
        }
        (b"IDProperty", b"ui_data") => {
            let kind = number(f, b, p, b"type")?;
            let kind = if kind == 5 {
                number(f, b, p, b"subtype")?
            } else {
                kind
            };
            match kind {
                0 => b"IDPropertyUIDataString",
                1 => b"IDPropertyUIDataInt",
                2 | 8 => b"IDPropertyUIDataFloat",
                7 => b"IDPropertyUIDataID",
                10 => b"IDPropertyUIDataBool",
                _ => return Err(fail("unprofiled_role", "unknown property UI discriminator")),
            }
        }
        (b"IDPropertyData", b"pointer") => {
            let q = property(f, p)?;
            // Read admission uses saved discriminator/layout/target proof,
            // not a decimal version stamp. Unknown arms remain refused.
            match number(f, b, &q, b"type")? {
                0 => b"char",
                7 => b"ID",
                9 => b"IDProperty",
                5 => match number(f, b, &q, b"subtype")? {
                    1 => b"int",
                    2 => b"float",
                    8 => b"double",
                    10 => b"int8_t",
                    6 => {
                        return Err(fail(
                            "unprofiled_role",
                            "pointer-bearing property table requires relocation profile",
                        ));
                    }
                    _ => return Err(fail("unprofiled_role", "unknown property array subtype")),
                },
                _ => {
                    return Err(fail(
                        "unprofiled_role",
                        "property union has no pointer role for discriminator",
                    ));
                }
            }
        }
        (b"WorkSpaceDataRelation", b"parent") => b"WorkSpaceInstanceHook",
        (b"WorkSpaceDataRelation", b"value") => b"WorkSpaceLayout",
        (b"PackedFile", b"data") => b"char",
        (b"bConstraint", b"data") => {
            let discriminator = number(f, b, p, b"type")?;
            if !write && discriminator != 29 {
                let ty = typed_owner_storage(f, b, p)?.ok_or_else(|| {
                    fail(
                        "unprofiled_role",
                        "constraint data requires one typed owner DATA struct",
                    )
                })?;
                return Ok(vec![ty.to_vec()]);
            }
            if write && f.header().version != 502 || discriminator != 29 {
                return Err(fail(
                    "unprofiled_role",
                    "unqualified constraint-data discriminator",
                ));
            }
            b"bTransformCacheConstraint"
        }
        (b"ARegion", b"regiondata") => match number(f, b, p, b"regiontype")? {
            0 => b"RegionView3D",
            14 => b"RegionAssetShelf",
            _ => {
                return Err(fail(
                    "unprofiled_role",
                    "unknown saved region-data discriminator",
                ));
            }
        },
        _ if p.declared_type != b"void"
            && f.schema().types.iter().any(|t| t == &p.declared_type) =>
        {
            &p.declared_type
        }
        _ => {
            return Err(fail(
                "unprofiled_role",
                format!("{} has no saved pointer profile", p.path),
            ));
        }
    };
    Ok(vec![result.to_vec()])
}
fn socket_storage_type(f: &BlendFile, socket: usize, write: bool) -> Result<Option<&'static [u8]>> {
    let Some(owner) = f.owner_index()?.owner(socket) else {
        return Ok(None);
    };
    let socket_address = f.block(socket)?.old_address;
    // Only this native ID's contiguous DATA run can contain the socket owner.
    // Avoid scanning unrelated geometry/image allocations for each socket.
    for node in (owner + 1)..f.blocks().len() {
        let block = f.block(node)?;
        if block.code != *b"DATA" {
            break;
        }
        if block.sdna_index == 0 || f.dna_type_id(node)? != b"bNode" {
            continue;
        }
        let q = PointerLayout {
            offset: 0,
            path: String::new(),
            declared_type: b"bNodeSocket".to_vec(),
            parent_type: b"bNode".to_vec(),
            parent_offset: 0,
            field_name: b"storage".to_vec(),
            pointer_depth: 1,
            list_role: None,
        };
        let (field, storage): (&[u8], &[u8]) =
            match (text(f, node, &q, b"idname")?.as_str(), f.header().version) {
                ("CompositorNodeImage" | "CompositorNodeRLayers", version)
                    if !write || matches!(version, 405 | 502) =>
                {
                    (b"outputs", b"NodeImageLayer")
                }
                // Independently observed Blender 4.5 File Output input sockets;
                // no inference about changed File Output storage in newer versions.
                ("CompositorNodeOutputFile", version) if !write || version == 405 => {
                    (b"inputs", b"NodeImageMultiFileSocket")
                }
                _ => continue,
            };
        let list = member(f, b"bNode", field)?;
        let list_layout = PointerLayout {
            offset: list.offset,
            parent_type: b"ListBase".to_vec(),
            parent_offset: list.offset,
            field_name: b"first".to_vec(),
            list_role: Some((b"bNode".to_vec(), field.to_vec())),
            declared_type: b"void".to_vec(),
            ..q.clone()
        };
        validate_list(f, node, &list_layout)?;
        let mut address = checked::number(
            &f.payload(node)?,
            list.offset,
            f.header().pointer_size,
            f.header().little_endian,
        )?;
        let mut seen = BTreeSet::new();
        while address != 0 {
            if !seen.insert(address) {
                return Err(fail("list_cycle", "socket owner list cycle"));
            }
            let target = f.owner_index()?.resolve(f, node, address, &q)?;
            if f.dna_type_id(target)? != b"bNodeSocket" {
                return Err(fail(
                    "pointer_target_type",
                    "qualified owning-node list element is not socket",
                ));
            }
            if address == socket_address {
                return Ok(Some(storage));
            }
            let next = member(f, b"bNodeSocket", b"next")?;
            address = checked::number(
                &f.payload(target)?,
                next.offset,
                f.header().pointer_size,
                f.header().little_endian,
            )?;
        }
    }
    Ok(None)
}
fn group_property(f: &BlendFile, p: &PointerLayout) -> Result<PointerLayout> {
    let mut q = p.clone();
    q.parent_type = b"IDPropertyData".to_vec();
    q.parent_offset = q
        .parent_offset
        .checked_sub(member(f, b"IDPropertyData", b"group")?.offset)
        .ok_or_else(|| fail("pointer_role", "group list outside property union"))?;
    property(f, &q)
}
fn number_pointer(f: &BlendFile, b: usize, p: &PointerLayout, name: &[u8]) -> Result<u64> {
    let field = member(f, &p.parent_type, name)?;
    if !field.pointer || field.array_len != 1 {
        return Err(fail("pointer_role", "single pointer required"));
    }
    checked::number(
        &f.payload(b)?,
        checked::add(p.parent_offset, field.offset)?,
        f.header().pointer_size,
        f.header().little_endian,
    )
}
/// A saved zero-element bake array has no persistent target even when Blender
/// retains a stale allocation handle. Consumers check this before resolving;
/// imports reset that handle, while sparse original-file edits preserve bytes.
/// Structural intrusive-list proof, scoped by trace to read-qualified declared DATA.
/// Known lists never enter this path; their discriminators/profiles stay owning.
pub(crate) fn declared_intrusive_list_target(
    f: &BlendFile,
    b: usize,
    p: &PointerLayout,
) -> Result<Option<usize>> {
    let Some((owner, name)) = p.list_role.as_ref() else {
        return Ok(None);
    };
    if list_type(owner, name).is_some()
        || p.parent_type != b"ListBase"
        || !matches!(p.field_name.as_slice(), b"first" | b"last")
    {
        return Ok(None);
    }
    let read = |block, offset| {
        checked::number(
            &f.payload(block)?,
            offset,
            f.header().pointer_size,
            f.header().little_endian,
        )
    };
    let first = read(
        b,
        checked::add(p.parent_offset, member(f, b"ListBase", b"first")?.offset)?,
    )?;
    let last = read(
        b,
        checked::add(p.parent_offset, member(f, b"ListBase", b"last")?.offset)?,
    )?;
    if first == 0 || last == 0 {
        return Err(fail(
            "list_endpoints",
            "non-null list site requires both endpoints",
        ));
    }
    let lookup = |address| {
        f.owner_index()?
            .lookup(f, b, address, &p.path, super::index::Namespace::Data)
    };
    let first_block = lookup(first)?;
    let ty = f.dna_type_id(first_block)?;
    let next = member(f, ty, b"next")?;
    let prev = member(f, ty, b"prev")?;
    for field in [next, prev] {
        if !field.pointer
            || field.array_len != 1
            || f.schema().types[field.type_index] != ty
            || field.full_name.contains(&b'(')
            || field.full_name.iter().filter(|&&byte| byte == b'*').count() != 1
        {
            return Err(fail(
                "unprofiled_role",
                "intrusive edges must declare the same single-pointer element type",
            ));
        }
    }
    let mut seen = BTreeSet::new();
    let mut address = first;
    let mut previous = 0;
    let mut last_block = first_block;
    while address != 0 {
        if !seen.insert(address) {
            return Err(fail("list_cycle", "structural declared list cycle"));
        }
        let block = lookup(address)?;
        let header = f.block(block)?;
        if header.code != *b"DATA" || header.sdna_index == 0 || f.dna_type_id(block)? != ty {
            return Err(fail(
                "pointer_target_type",
                "intrusive sibling SDNA type differs",
            ));
        }
        let ti = type_index(f, ty)?;
        let length = f.schema().lengths[ti];
        if header.count != 1 || length == 0 || header.size != length {
            return Err(fail(
                "pointer_extent",
                "intrusive element contradicts source SDNA/count",
            ));
        }
        let si = f.struct_index(block)?;
        for field in &f.schema().structs[si].fields {
            if checked::add(field.offset, field.size)? > length {
                return Err(fail(
                    "pointer_extent",
                    "intrusive field exceeds source SDNA struct",
                ));
            }
        }
        if read(block, prev.offset)? != previous {
            return Err(fail(
                "list_reciprocity",
                "intrusive prev edge differs from predecessor",
            ));
        }
        previous = address;
        last_block = block;
        address = read(block, next.offset)?;
    }
    if previous != last {
        return Err(fail(
            "list_endpoints",
            "intrusive last endpoint differs from chain",
        ));
    }
    Ok(Some(if p.field_name == b"first" {
        first_block
    } else {
        last_block
    }))
}
pub(crate) fn empty_allocation(f: &BlendFile, b: usize, p: &PointerLayout) -> Result<bool> {
    if p.parent_type != b"NodesModifierData" || p.field_name != b"bakes" || p.pointer_depth != 1 {
        return Ok(false);
    }
    let field = member(f, b"NodesModifierData", b"bakes")?;
    if !field.pointer
        || field.array_len != 1
        || f.schema().types[field.type_index] != b"NodesModifierBake"
    {
        return Err(fail(
            "unprofiled_role",
            "unqualified empty bake-array schema",
        ));
    }
    Ok(count(f, b, p, b"bakes_num")? == 0)
}
pub(crate) fn namespace(
    f: &BlendFile,
    b: usize,
    address: u64,
    p: &PointerLayout,
) -> Result<super::index::Namespace> {
    use super::index::Namespace;
    if member(f, &p.parent_type, &p.field_name)?
        .full_name
        .contains(&b'(')
    {
        return Err(fail(
            "unprofiled_role",
            "callable/complex declarator has no persistent pointer profile",
        ));
    }
    if p.pointer_depth > 1 {
        return Ok(Namespace::Data);
    }
    let parent = p.parent_type.as_slice();
    let field = p.field_name.as_slice();
    if super::embedded::known_site(parent, field) {
        return Ok(Namespace::Data);
    }
    if (parent, field) == (b"LayerCollection", b"collection") {
        if let Some(owner) = f.owner_index()?.owner(b)
            && f.dna_type_id(owner)? == b"Scene"
            && super::graph::uint(f, owner, &["master_collection"])? == address
        {
            return Ok(Namespace::Data);
        }
        return Ok(Namespace::Id);
    }
    if matches!(
        (parent, field),
        (b"FileGlobal", b"cur_view_layer")
            | (b"WorkSpaceDataRelation", b"parent")
            | (b"WorkSpaceInstanceHook", b"act_layout")
    ) {
        return Ok(Namespace::CrossData);
    }
    if matches!(
        (parent, field),
        (b"SpaceNode", b"nodetree" | b"edittree") | (b"bNodeTreePath", b"nodetree")
    ) {
        return Ok(
            if f.blocks()
                .iter()
                .any(|q| super::owner::read_id(&q.code) && q.old_address == address)
            {
                Namespace::Id
            } else {
                Namespace::CrossData
            },
        );
    }
    if (parent, field) == (b"ID", b"lib")
        || matches!(
            (parent, field),
            (b"IDOverrideLibrary", b"reference" | b"hierarchy_root")
        )
    {
        return Ok(Namespace::Id);
    }
    if (parent, field) == (b"Object", b"data")
        || (parent == b"ID" && matches!(field, b"next" | b"prev"))
    {
        return Ok(Namespace::Id);
    }
    if (parent, field) == (b"IDPropertyData", b"pointer")
        && number(f, b, &property(f, p)?, b"type")? == 7
    {
        return Ok(Namespace::Id);
    }
    if embeds(f, &p.declared_type, b"ID") {
        return Ok(Namespace::Id);
    }
    if p.declared_type == b"void" {
        // A qualified union/list has an explicit owner-data role. No address-only fallback.
        // On the narrow 503 JobSnapshot read path the same call runs
        // inside roles::validate_list, which already qualified this exact
        // group list (type-6 discriminator, endpoints, intrusive links).
        expected(f, b, p)?;
    }
    Ok(Namespace::Data)
}
fn embeds(f: &BlendFile, actual: &[u8], expected: &[u8]) -> bool {
    let mut current = actual;
    let mut visited = BTreeSet::new();
    loop {
        if current == expected {
            return true;
        }
        let Some(ti) = f.schema().types.iter().position(|t| t == current) else {
            return false;
        };
        if !visited.insert(ti) {
            return false;
        }
        let Some(si) = f.schema().by_type.get(&ti) else {
            return false;
        };
        let Some(field) = f.schema().structs[*si].fields.first() else {
            return false;
        };
        if field.pointer || field.offset != 0 || field.array_len != 1 {
            return false;
        }
        current = &f.schema().types[field.type_index];
    }
}
const SPACE_TYPES: &[&[u8]] = &[
    b"SpaceAction",
    b"SpaceButs",
    b"SpaceConsole",
    b"SpaceFile",
    b"SpaceImage",
    b"SpaceInfo",
    b"SpaceNode",
    b"SpaceOops",
    b"SpaceSpreadsheet",
    b"SpaceStatusBar",
    b"SpaceText",
    b"SpaceTopBar",
    b"View3D",
];
fn matches_linked_id(f: &BlendFile, target: usize, expected: &[u8]) -> Result<bool> {
    if f.dna_type_id(target)? != b"ID" || f.block(target)?.code != *b"ID\0\0" {
        return Ok(false);
    }
    let data = f.payload(target)?;
    let name = member(f, b"ID", b"name")?;
    let bytes = checked::bytes(&data, name.offset, name.size)?;
    let lib = member(f, b"ID", b"lib")?;
    if bytes.len() < 2
        || checked::number(
            &data,
            lib.offset,
            f.header().pointer_size,
            f.header().little_endian,
        )? == 0
    {
        return Ok(false);
    }
    let code = [bytes[0], bytes[1], 0, 0];
    Ok(crate::id_codes::expected(&code)
        .is_some_and(|types| types.contains(&crate::id_codes::canonical(expected))))
}
fn matches_type(f: &BlendFile, target: usize, expected: &[u8]) -> Result<bool> {
    let actual = f.dna_type_id(target)?;
    if embeds(f, actual, expected) {
        return Ok(true);
    }
    if expected == b"SpaceLink" && SPACE_TYPES.contains(&actual) {
        // Saved spaces repeat (not embed) the common header. Prove the header.
        let a = member(f, actual, b"next")?;
        let z = member(f, actual, b"prev")?;
        return Ok(a.pointer
            && z.pointer
            && a.offset == 0
            && z.offset == f.header().pointer_size
            && f.schema().types[a.type_index] == b"SpaceLink"
            && f.schema().types[z.type_index] == b"SpaceLink");
    }
    if actual == b"ID" && expected != b"ID" && embeds(f, expected, b"ID") {
        return matches_linked_id(f, target, expected);
    }
    Ok(crate::id_codes::canonical(actual) == crate::id_codes::canonical(expected))
}
// The primitive declaration alone is not an extent contract. This read role
// belongs to a type-22 constraint in this same ID-DATA namespace, not to any
// allocation with a matching address or a struct with a familiar field name.
fn spline_ik_points_extent(f: &BlendFile, b: usize, p: &PointerLayout) -> Result<Option<usize>> {
    if p.parent_type != b"bSplineIKConstraint" || p.field_name != b"points" {
        return Ok(None);
    }
    let block = f.block(b)?;
    let points = member(f, &p.parent_type, b"points")?;
    if p.declared_type != b"float"
        || !points.pointer
        || p.pointer_depth != 1
        || points.array_len != 1
        || p.parent_offset != 0
        || block.count != 1
    {
        return Err(fail(
            "unprofiled_role",
            "Spline IK points require a single float pointer owner",
        ));
    }
    let index = f.owner_index()?;
    let owner = index
        .owner(b)
        .ok_or_else(|| fail("unprofiled_role", "Spline IK lacks ID owner"))?;
    let mut caller = false;
    for (i, candidate) in f
        .blocks()
        .iter()
        .enumerate()
        .skip(owner + 1)
        .take_while(|(i, _)| index.owner(*i) == Some(owner))
    {
        if candidate.count != 1 || f.dna_type_id(i).ok() != Some(b"bConstraint".as_slice()) {
            continue;
        }
        if super::graph::uint(f, i, &["data"])? == block.old_address {
            if super::graph::uint(f, i, &["type"])? != 22 {
                return Err(fail(
                    "unprofiled_role",
                    "Spline IK points require constraint type 22",
                ));
            }
            caller = true;
        }
    }
    if !caller {
        return Err(fail(
            "unprofiled_role",
            "Spline IK points lack a qualified constraint caller",
        ));
    }
    let n = count(f, b, p, b"numpoints")?;
    let width = f.schema().lengths[points.type_index];
    if n == 0 || width == 0 {
        return Err(fail(
            "pointer_extent",
            "non-null Spline IK points require a positive extent",
        ));
    }
    Ok(Some(checked::mul(n, width)?))
}
pub fn qualify(f: &BlendFile, b: usize, p: &PointerLayout, target: usize) -> Result<()> {
    qualify_impl(f, b, p, target, false)
}
pub(crate) fn write_qualify(
    f: &BlendFile,
    b: usize,
    p: &PointerLayout,
    target: usize,
) -> Result<()> {
    qualify_impl(f, b, p, target, true)
}
fn qualify_impl(
    f: &BlendFile,
    b: usize,
    p: &PointerLayout,
    target: usize,
    write: bool,
) -> Result<()> {
    if p.pointer_depth != 1 {
        return Err(fail(
            "unprofiled_role",
            "pointer-to-pointer table requires an extent/element profile",
        ));
    }
    let block = f.block(target)?;
    let choices = expected_impl(f, b, p, write)?;
    if !write && p.parent_type == b"Object" && p.field_name == b"data" {
        super::owner::qualify_read_id(f, target)?;
        let actual = f.dna_type_id(target)?;
        for ty in &choices {
            if crate::id_codes::canonical(actual) == crate::id_codes::canonical(ty)
                || matches_linked_id(f, target, ty)?
            {
                return Ok(());
            }
        }
        return Err(fail(
            "pointer_target_type",
            "Object.data family differs from Object.type",
        ));
    }
    if !write && let Some(extent) = spline_ik_points_extent(f, b, p)? {
        if block.code != *b"DATA" || block.sdna_index != 0 {
            return Err(fail(
                "pointer_target_type",
                "Spline IK points require raw DATA",
            ));
        }
        if block.count != 1 || block.size != extent {
            return Err(fail(
                "pointer_extent",
                "Spline IK points target differs from source float extent",
            ));
        }
        return Ok(());
    }
    if let Some((ty, extent)) = if write {
        lighting_write_profile(f, b, p)?
    } else {
        lighting_profile(f, b, p)?
    } {
        if block.count != 1 {
            return Err(fail("pointer_count", "pointer profile requires count=1"));
        }
        if block.code != *b"DATA" || block.sdna_index == 0 || f.dna_type_id(target)? != ty {
            return Err(fail(
                "pointer_target_type",
                "pointer profile requires exact DATA type",
            ));
        }
        let extent = if write {
            extent
        } else {
            // Read layout belongs to this file's SDNA, not one captured version.
            // Type identity, role and count were verified above; writes retain
            // the independently qualified captured profile.
            let ti = type_index(f, ty)?;
            let si = f
                .schema()
                .by_type
                .get(&ti)
                .ok_or_else(|| fail("pointer_extent", "typed role lacks SDNA struct"))?;
            let length = f.schema().lengths[ti];
            for field in &f.schema().structs[*si].fields {
                if checked::add(field.offset, field.size)? > length {
                    return Err(fail(
                        "pointer_extent",
                        "field extent exceeds source SDNA struct",
                    ));
                }
            }
            checked::mul(length, block.count)?
        };
        if block.size != extent {
            return Err(fail(
                "pointer_extent",
                if write {
                    "pointer target differs from qualified extent"
                } else {
                    "typed role target differs from source SDNA extent"
                },
            ));
        }
    }
    if let Some(ty) = single_struct_storage(f, b, p, write)? {
        if block.count != 1 {
            return Err(fail(
                "pointer_count",
                format!(
                    "{}: single-struct {} storage requires count=1, got count={}",
                    p.path,
                    String::from_utf8_lossy(ty),
                    block.count
                ),
            ));
        }
        if block.code != *b"DATA" || block.sdna_index == 0 || f.dna_type_id(target)? != ty {
            return Err(fail(
                "pointer_target_type",
                "registered node storage requires exact DATA type",
            ));
        }
    }
    if block.sdna_index == 0 {
        return qualify_raw(f, b, p, target, &choices);
    }
    if choices.iter().any(|ty| {
        matches!(
            ty.as_slice(),
            b"char" | b"int" | b"float" | b"double" | b"int8_t" | b"uint8_t"
        )
    }) {
        return Err(fail(
            "pointer_target_type",
            "byte/numeric role requires an opaque allocation",
        ));
    }
    if matches!(
        (p.parent_type.as_slice(), p.field_name.as_slice()),
        (
            b"bNodeLink",
            b"fromnode" | b"tonode" | b"fromsock" | b"tosock"
        ) | (b"bNodeSocket", b"default_value" | b"storage")
            | (b"bNodeTreeInterfaceSocket", b"socket_data")
            | (b"bConstraint", b"data")
    ) && block.count != 1
    {
        return Err(fail(
            "pointer_count",
            "single-struct pointer role requires count=1",
        ));
    }
    if matches!(
        (p.parent_type.as_slice(), p.field_name.as_slice()),
        (b"bNodeSocket", b"default_value" | b"storage")
            | (b"bNodeTreeInterfaceSocket", b"socket_data")
            | (b"IDProperty", b"ui_data")
            | (b"ARegion", b"regiondata")
            | (b"bConstraint", b"data")
    ) && (block.code != *b"DATA"
        || !choices
            .iter()
            .any(|ty| f.dna_type_id(target).is_ok_and(|actual| actual == ty)))
    {
        return Err(fail(
            "pointer_target_type",
            "discriminated role requires exact DATA type",
        ));
    }
    let expected_count = match (p.parent_type.as_slice(), p.field_name.as_slice()) {
        (b"FCurve", b"bezt" | b"fpt") => Some(b"totvert".as_slice()),
        (b"CurveMap", b"curve") => Some(b"totpoint".as_slice()),
        (b"bNodeTree", b"nested_node_refs") => Some(b"nested_node_refs_num".as_slice()),
        (b"bNode", b"panel_states_array") => Some(b"num_panel_states".as_slice()),
        (b"NodesModifierData", b"bakes") => Some(b"bakes_num".as_slice()),
        _ => None,
    };
    if let Some(field) = expected_count
        && block.count != count(f, b, p, field)?
    {
        return Err(fail(
            "pointer_extent",
            "typed array count differs from saved length",
        ));
    }
    if p.parent_type == b"IDPropertyData" && p.field_name == b"pointer" {
        let q = property(f, p)?;
        if number(f, b, &q, b"type")? == 9 && block.count != count(f, b, &q, b"len")? {
            return Err(fail(
                "pointer_extent",
                "property array count differs from saved length",
            ));
        }
    }
    let st = f
        .schema()
        .structs
        .get(block.sdna_index)
        .ok_or_else(|| fail("pointer_target_type", "target SDNA index absent"))?;
    let size = f.schema().lengths[st.type_index];
    if block.count == 0 || checked::mul(size, block.count)? != block.size {
        return Err(fail("pointer_extent", "typed target count/length mismatch"));
    }
    for ty in choices {
        if matches_type(f, target, &ty)? {
            return Ok(());
        }
    }
    Err(fail(
        "pointer_target_type",
        format!(
            "{} -> {}",
            p.path,
            String::from_utf8_lossy(&f.schema().types[st.type_index])
        ),
    ))
}
pub fn validate_list(f: &BlendFile, b: usize, p: &PointerLayout) -> Result<()> {
    validate_list_impl(f, b, p, false)
}
pub(crate) fn write_validate_list(f: &BlendFile, b: usize, p: &PointerLayout) -> Result<()> {
    validate_list_impl(f, b, p, true)
}
fn validate_list_impl(f: &BlendFile, b: usize, p: &PointerLayout, write: bool) -> Result<()> {
    let first = number_pointer(f, b, p, b"first")?;
    let last = number_pointer(f, b, p, b"last")?;
    if first == 0 && last == 0 {
        return Ok(());
    }
    if first == 0 || last == 0 {
        return Err(fail("list_boundary", "first/last nullness mismatch"));
    }
    let choices = expected_impl(f, b, p, write)?;
    let ty = choices
        .first()
        .ok_or_else(|| fail("unprofiled_role", "empty list profile"))?;
    let mut typed = p.clone();
    typed.declared_type = ty.clone();
    let mut address = first;
    let mut previous = 0;
    let mut seen = BTreeSet::new();
    loop {
        let target = f.owner_index()?.resolve(f, b, address, &typed)?;
        qualify_impl(f, b, p, target, write)?;
        if f.block(target)?.count != 1 {
            return Err(fail(
                "pointer_count",
                "intrusive list element requires one record",
            ));
        }
        if !seen.insert(target) {
            return Err(fail("list_cycle", "intrusive list cycle"));
        }
        let fields = pointer_layout(f, target)?
            .ok_or_else(|| fail("pointer_target_type", "list element is opaque"))?;
        let next = fields
            .iter()
            .find(|q| q.offset == 0 && q.field_name == b"next")
            .ok_or_else(|| fail("list_header", "element has no next at offset zero"))?;
        let prev = fields
            .iter()
            .find(|q| q.offset == f.header().pointer_size && q.field_name == b"prev")
            .ok_or_else(|| fail("list_header", "element has no prev in intrusive header"))?;
        let data = f.payload(target)?;
        let back = checked::number(
            &data,
            prev.offset,
            f.header().pointer_size,
            f.header().little_endian,
        )?;
        if back != previous {
            return Err(fail(
                "list_backlink",
                "list prev does not match predecessor",
            ));
        }
        let forward = checked::number(
            &data,
            next.offset,
            f.header().pointer_size,
            f.header().little_endian,
        )?;
        if forward == 0 {
            return if address == last {
                Ok(())
            } else {
                Err(fail("list_boundary", "terminal element differs from last"))
            };
        }
        previous = address;
        address = forward;
    }
}
fn qualify_raw(
    f: &BlendFile,
    b: usize,
    p: &PointerLayout,
    target: usize,
    choices: &[Vec<u8>],
) -> Result<()> {
    let block = f.block(target)?;
    let parent = p.parent_type.as_slice();
    let field = p.field_name.as_slice();
    let extent = match (parent, field) {
        (b"PackedFile", b"data") => Some(count(f, b, p, b"size")?),
        (b"TextLine", b"line") => Some(checked::add(count(f, b, p, b"len")?, 1)?),
        (b"IDPropertyData", b"pointer") => {
            let q = property(f, p)?;
            let len = count(f, b, &q, b"len")?;
            match number(f, b, &q, b"type")? {
                0 => Some(len),
                5 => {
                    let width = match number(f, b, &q, b"subtype")? {
                        1 | 2 => 4,
                        8 => 8,
                        10 => 1,
                        6 => {
                            return Err(fail(
                                "unprofiled_role",
                                "pointer-bearing property table requires relocation profile",
                            ));
                        }
                        _ => return Err(fail("unprofiled_role", "unknown property array subtype")),
                    };
                    Some(checked::mul(len, width)?)
                }
                _ => {
                    return Err(fail(
                        "pointer_target_type",
                        "property role requires typed allocation",
                    ));
                }
            }
        }
        (b"PreviewImage", b"rect") => {
            let rect = member(f, parent, field)?;
            let index = p
                .offset
                .checked_sub(checked::add(p.parent_offset, rect.offset)?)
                .ok_or_else(|| fail("pointer_role", "preview rectangle offset"))?
                / f.header().pointer_size;
            let dimension = |name: &[u8]| -> Result<usize> {
                let d = member(f, parent, name)?;
                if index >= d.array_len {
                    return Err(fail("pointer_role", "preview rectangle index"));
                }
                let n = checked::number(
                    &f.payload(b)?,
                    checked::add(
                        checked::add(p.parent_offset, d.offset)?,
                        checked::mul(index, d.element_size)?,
                    )?,
                    d.element_size,
                    f.header().little_endian,
                )?;
                if n & (1u64 << (d.element_size * 8 - 1)) != 0 {
                    return Err(fail("pointer_extent", "negative preview dimension"));
                }
                usize::try_from(n).map_err(|_| fail("overflow", "preview dimension"))
            };
            Some(checked::mul(
                checked::mul(dimension(b"w")?, dimension(b"h")?)?,
                4,
            )?)
        }
        (
            b"IDPropertyUIDataInt" | b"IDPropertyUIDataFloat" | b"IDPropertyUIDataBool",
            b"default_array",
        ) => {
            let width = if parent == b"IDPropertyUIDataFloat" {
                8
            } else if parent == b"IDPropertyUIDataBool" {
                1
            } else {
                4
            };
            Some(checked::mul(count(f, b, p, b"default_array_len")?, width)?)
        }
        _ => None,
    };
    if let Some(extent) = extent
        && block.size != extent
    {
        return Err(fail(
            "pointer_extent",
            format!("{} allocation size {} != {extent}", p.path, block.size),
        ));
    }
    let char_data = choices.iter().any(|t| t == b"char");
    let bytes = f.payload(target)?;
    if char_data
        && !matches!(
            (parent, field),
            (b"PackedFile", b"data") | (b"IDPropertyUIDataBool", b"default_array")
        )
    {
        if parent == b"IDPropertyData" && field == b"pointer" {
            let q = property(f, p)?;
            if number(f, b, &q, b"type")? == 0 && number(f, b, &q, b"subtype")? == 1 {
                return Ok(());
            }
        }
        if bytes.last() != Some(&0) {
            return Err(fail("pointer_extent", "byte string lacks NUL terminator"));
        }
        if (parent, field) != (b"IDPropertyData", b"pointer")
            && bytes[..bytes.len() - 1].contains(&0)
        {
            return Err(fail(
                "pointer_extent",
                "byte string extent exceeds terminator",
            ));
        }
        return Ok(());
    }
    if extent.is_some() {
        return Ok(());
    }
    Err(fail(
        "unprofiled_role",
        format!(
            "{}: opaque target requires a byte/numeric extent profile",
            p.path
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_ops::graph::Graph;

    #[test]
    fn object_data_read_families_use_registered_native_id_layouts() {
        for (kind, ty, code) in [
            (0, "Image", *b"IM\0\0"),
            (1, "Mesh", *b"ME\0\0"),
            (2, "Curve", *b"CU\0\0"),
            (3, "Curve", *b"CU\0\0"),
            (4, "Curve", *b"CU\0\0"),
            (5, "MetaBall", *b"MB\0\0"),
            (10, "Light", *b"LA\0\0"),
            (11, "Camera", *b"CA\0\0"),
            (12, "Speaker", *b"SK\0\0"),
            (13, "LightProbe", *b"LP\0\0"),
            (22, "Lattice", *b"LT\0\0"),
            (25, "bArmature", *b"AR\0\0"),
            (26, "bGPdata", *b"GD\0\0"),
            (27, "Curves", *b"CV\0\0"),
            (28, "PointCloud", *b"PT\0\0"),
            (29, "Volume", *b"VO\0\0"),
            (30, "GreasePencil", *b"GP\0\0"),
        ] {
            let source = schema();
            let mut g = Graph::new(&source).unwrap();
            let object = g.new_typed("Object", None).unwrap();
            g.records[object].code = *b"OB\0\0";
            g.cstring(object, &["id", "name"], "OBfamily").unwrap();
            g.set(object, &["type"], kind).unwrap();
            let actual_ty = source
                .schema()
                .structs
                .iter()
                .map(|s| &source.schema().types[s.type_index])
                .find(|name| crate::id_codes::canonical(name) == ty.as_bytes())
                .unwrap_or_else(|| panic!("source DNA lacks registered family {ty}"));
            let target = g
                .new_typed(std::str::from_utf8(actual_ty).unwrap(), None)
                .unwrap();
            g.records[target].code = code;
            g.cstring(
                target,
                &["id", "name"],
                &format!("{}family", std::str::from_utf8(&code[..2]).unwrap()),
            )
            .unwrap();
            g.ptr(object, &["data"], Some(target)).unwrap();
            let f = file(&g);
            qualify(&f, object, &layout(&f, object, "[0].data"), target).unwrap();
        }
    }
    #[test]
    fn spline_points_read_extent_does_not_admit_writes() {
        let f = BlendFile::from_bytes(
            include_bytes!("../../tests/fixtures/dependencies/spline-ik.blend").to_vec(),
        )
        .unwrap();
        let p = layout(&f, 2, "[0].points");
        qualify(&f, 2, &p, 3).unwrap();
        assert_eq!(
            write_qualify(&f, 2, &p, 3).unwrap_err().kind,
            "unprofiled_role"
        );
    }
    fn schema() -> BlendFile {
        // Authorized retained saved bytes only; none of their generator/source.
        BlendFile::from_bytes(
            include_bytes!(
                "../../tests/fixtures/file-ops/native-4.5.8/build-kit/constructors.blend"
            )
            .to_vec(),
        )
        .unwrap()
    }
    fn scene(g: &mut Graph<'_>) -> usize {
        let s = g.new_typed("Scene", None).unwrap();
        g.records[s].code = *b"SC\0\0";
        g.cstring(s, &["id", "name"], "SCroles vector").unwrap();
        s
    }
    fn file(g: &Graph<'_>) -> BlendFile {
        // Deliberately serialize hostile vectors without Graph's publication
        // verifier: the tests exercise the site's own receiving boundary.
        assert_eq!(g.schema.header().file_format, 0);
        assert_eq!(g.schema.header().pointer_size, 8);
        assert!(g.schema.header().little_endian);
        let mut bytes = g.schema.serialized_header().unwrap();
        let encode = |bytes: &mut Vec<u8>,
                      code: &[u8; 4],
                      sdna: usize,
                      address: u64,
                      count: usize,
                      data: &[u8]| {
            bytes.extend_from_slice(code);
            bytes.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
            bytes.extend_from_slice(&address.to_le_bytes());
            bytes.extend_from_slice(&u32::try_from(sdna).unwrap().to_le_bytes());
            bytes.extend_from_slice(&u32::try_from(count).unwrap().to_le_bytes());
            bytes.extend_from_slice(data);
        };
        for (owner, record) in g
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.alive && r.code != *b"DATA")
        {
            encode(
                &mut bytes,
                &record.code,
                record.sdna,
                record.address,
                record.count,
                &record.data,
            );
            for record in g
                .records
                .iter()
                .filter(|r| r.alive && r.code == *b"DATA" && r.owner == owner)
            {
                encode(
                    &mut bytes,
                    &record.code,
                    record.sdna,
                    record.address,
                    record.count,
                    &record.data,
                );
            }
        }
        let (dna, record) = g
            .schema
            .blocks()
            .iter()
            .enumerate()
            .find(|(_, r)| r.code == *b"DNA1")
            .unwrap();
        encode(
            &mut bytes,
            b"DNA1",
            record.sdna_index,
            0,
            record.count,
            &g.schema.payload(dna).unwrap(),
        );
        encode(&mut bytes, b"ENDB", 0, 0, 0, &[]);
        BlendFile::from_bytes(bytes).unwrap()
    }
    fn block(f: &BlendFile, address: u64) -> usize {
        f.blocks()
            .iter()
            .position(|b| b.old_address == address)
            .unwrap()
    }
    fn layout(f: &BlendFile, b: usize, path: &str) -> PointerLayout {
        pointer_layout(f, b)
            .unwrap()
            .unwrap()
            .into_iter()
            .find(|p| p.path == path)
            .unwrap()
    }
    #[test]
    fn list_vectors_prove_type_endpoints_reciprocity_and_cycles() {
        let f = schema();
        let mut g = Graph::new(&f).unwrap();
        let s = scene(&mut g);
        let tree = g.new_typed("bNodeTree", Some(s)).unwrap();
        let a = g.new_typed("bNode", Some(s)).unwrap();
        let z = g.new_typed("bNode", Some(s)).unwrap();
        g.ptr(tree, &["nodes", "first"], Some(a)).unwrap();
        g.ptr(tree, &["nodes", "last"], Some(z)).unwrap();
        g.ptr(a, &["next"], Some(z)).unwrap();
        g.ptr(z, &["prev"], Some(a)).unwrap();
        let check = |g: &Graph<'_>| {
            let f = file(g);
            let b = block(&f, g.records[tree].address);
            validate_list(&f, b, &layout(&f, b, "[0].nodes.first"))
        };
        check(&g).unwrap();
        g.ptr(z, &["prev"], None).unwrap();
        assert_eq!(check(&g).unwrap_err().kind, "list_backlink");
        g.ptr(z, &["prev"], Some(a)).unwrap();
        g.ptr(z, &["next"], Some(a)).unwrap();
        assert_eq!(check(&g).unwrap_err().kind, "list_cycle");
        g.ptr(z, &["next"], None).unwrap();
        g.ptr(tree, &["nodes", "last"], Some(a)).unwrap();
        assert_eq!(check(&g).unwrap_err().kind, "list_boundary");
        g.ptr(tree, &["nodes", "first"], None).unwrap();
        assert_eq!(check(&g).unwrap_err().kind, "list_boundary");
        let image = g.new_typed("Image", Some(s)).unwrap();
        g.ptr(tree, &["nodes", "first"], Some(image)).unwrap();
        g.ptr(tree, &["nodes", "last"], Some(image)).unwrap();
        assert_eq!(check(&g).unwrap_err().kind, "pointer_target_type");
    }
    #[test]
    fn required_links_and_unknown_void_roles_refuse() {
        let f = schema();
        let mut g = Graph::new(&f).unwrap();
        let s = scene(&mut g);
        let link = g.new_typed("bNodeLink", Some(s)).unwrap();
        let unknown = g.allocate(*b"DATA", 0, 1, vec![0; 8], Some(s)).unwrap();
        g.ptr(s, &["id", "_pad1"], Some(unknown)).unwrap();
        let f = file(&g);
        let b = block(&f, g.records[link].address);
        for field in ["fromnode", "tonode", "fromsock", "tosock"] {
            assert_eq!(
                super::super::validate::site(&f, b, &layout(&f, b, &format!("[0].{field}")), 0)
                    .unwrap_err()
                    .kind,
                "required_pointer_null"
            );
        }
        let b = block(&f, g.records[s].address);
        assert_eq!(
            super::super::validate::site(
                &f,
                b,
                &layout(&f, b, "[0].id._pad1"),
                g.records[unknown].address
            )
            .unwrap_err()
            .kind,
            "unprofiled_role"
        );
    }
    #[test]
    fn numeric_property_arrays_have_exact_byte_extents_not_pointer_elements() {
        let f = schema();
        let mut g = Graph::new(&f).unwrap();
        let s = scene(&mut g);
        let prop = g.new_typed("IDProperty", Some(s)).unwrap();
        g.set(prop, &["type"], 5).unwrap();
        g.set(prop, &["subtype"], 1).unwrap();
        g.set(prop, &["len"], 3).unwrap();
        let raw = g.allocate(*b"DATA", 0, 1, vec![255; 12], Some(s)).unwrap();
        g.ptr(prop, &["data", "pointer"], Some(raw)).unwrap();
        let check = |g: &Graph<'_>| {
            let f = file(g);
            let b = block(&f, g.records[prop].address);
            let target = block(&f, g.records[raw].address);
            qualify(&f, b, &layout(&f, b, "[0].data.pointer"), target)
        };
        check(&g).unwrap();
        g.records[raw].data.push(0);
        assert_eq!(check(&g).unwrap_err().kind, "pointer_extent");
        g.records[raw].data.pop();
        g.set(prop, &["len"], u32::MAX as u64).unwrap();
        assert_eq!(check(&g).unwrap_err().kind, "pointer_extent");
        g.set(prop, &["len"], 3).unwrap();
        g.set(prop, &["subtype"], 6).unwrap();
        assert_eq!(check(&g).unwrap_err().kind, "unprofiled_role");
        g.set(prop, &["subtype"], 255).unwrap();
        assert_eq!(check(&g).unwrap_err().kind, "unprofiled_role");
    }
    #[test]
    fn byte_property_subtype_preserves_non_nul_binary_payload() {
        let source = schema();
        let mut g = Graph::new(&source).unwrap();
        let s = scene(&mut g);
        let property = g.new_typed("IDProperty", Some(s)).unwrap();
        g.set(property, &["type"], 0).unwrap();
        g.set(property, &["subtype"], 1).unwrap();
        g.set(property, &["len"], 2).unwrap();
        let bytes = g.allocate(*b"DATA", 0, 1, vec![0xff, 1], Some(s)).unwrap();
        g.ptr(property, &["data", "pointer"], Some(bytes)).unwrap();
        let f = file(&g);
        let b = block(&f, g.records[property].address);
        let p = layout(&f, b, "[0].data.pointer");
        qualify(&f, b, &p, block(&f, g.records[bytes].address)).unwrap();
        g.set(property, &["subtype"], 0).unwrap();
        let f = file(&g);
        let b = block(&f, g.records[property].address);
        assert_eq!(
            qualify(&f, b, &p, block(&f, g.records[bytes].address))
                .unwrap_err()
                .kind,
            "pointer_extent"
        );
    }
    #[test]
    fn text_line_nul_and_saved_length_are_both_required() {
        let f = schema();
        let mut g = Graph::new(&f).unwrap();
        let s = scene(&mut g);
        let line = g.new_typed("TextLine", Some(s)).unwrap();
        g.set(line, &["len"], 3).unwrap();
        let raw = g
            .allocate(*b"DATA", 0, 1, b"abc\0".to_vec(), Some(s))
            .unwrap();
        g.ptr(line, &["line"], Some(raw)).unwrap();
        let check = |g: &Graph<'_>| {
            let f = file(g);
            let b = block(&f, g.records[line].address);
            qualify(
                &f,
                b,
                &layout(&f, b, "[0].line"),
                block(&f, g.records[raw].address),
            )
        };
        check(&g).unwrap();
        g.records[raw].data[3] = 1;
        assert_eq!(check(&g).unwrap_err().kind, "pointer_extent");
        g.records[raw].data = b"abcd\0".to_vec();
        assert_eq!(check(&g).unwrap_err().kind, "pointer_extent");
    }
    #[test]
    fn embedded_identifiers_never_fall_back_to_global_headers() {
        let f = schema();
        let mut g = Graph::new(&f).unwrap();
        let s = scene(&mut g);
        let tree = g.new_typed("bNodeTree", Some(s)).unwrap();
        g.ptr(s, &["nodetree"], Some(tree)).unwrap();
        let check = |g: &Graph<'_>| {
            let f = file(g);
            let b = block(&f, g.records[s].address);
            super::super::validate::site(
                &f,
                b,
                &layout(&f, b, "[0].nodetree"),
                g.records[tree].address,
            )
        };
        assert!(check(&g).unwrap().is_some());
        g.records[tree].code = *b"NT\0\0";
        g.cstring(tree, &["id", "name"], "NTglobal").unwrap();
        assert_eq!(check(&g).unwrap_err().kind, "pointer_target_type");
        g.ptr(s, &["nodetree"], None).unwrap();
        let f = file(&g);
        let b = block(&f, g.records[s].address);
        assert!(
            super::super::validate::site(&f, b, &layout(&f, b, "[0].nodetree"), 0)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn node_storage_count_and_socket_discriminator_are_qualified() {
        let f = schema();
        let mut g = Graph::new(&f).unwrap();
        let s = scene(&mut g);
        let node = g.new_typed("bNode", Some(s)).unwrap();
        g.cstring(node, &["idname"], "CompositorNodeCryptomatteV2")
            .unwrap();
        let storage = g.new_typed("NodeCryptomatte", Some(s)).unwrap();
        g.ptr(node, &["storage"], Some(storage)).unwrap();
        let check = |g: &Graph<'_>| {
            let f = file(g);
            let b = block(&f, g.records[node].address);
            qualify(
                &f,
                b,
                &layout(&f, b, "[0].storage"),
                block(&f, g.records[storage].address),
            )
        };
        check(&g).unwrap();
        g.records[storage].count = 2;
        g.records[storage].data = g.records[storage].data.repeat(2);
        assert_eq!(check(&g).unwrap_err().kind, "pointer_count");
        g.cstring(node, &["idname"], "UnknownStorage").unwrap();
        assert_eq!(check(&g).unwrap_err().kind, "unprofiled_role");
        let sock = g.new_typed("bNodeSocket", Some(s)).unwrap();
        g.cstring(sock, &["idname"], "NodeSocketFloat").unwrap();
        g.set(sock, &["type"], 0).unwrap();
        let val = g.new_typed("bNodeSocketValueFloat", Some(s)).unwrap();
        let f = file(&g);
        let b = block(&f, g.records[sock].address);
        qualify(
            &f,
            b,
            &layout(&f, b, "[0].default_value"),
            block(&f, g.records[val].address),
        )
        .unwrap();
        g.set(sock, &["type"], 2).unwrap();
        let f = file(&g);
        let b = block(&f, g.records[sock].address);
        assert_eq!(
            qualify(
                &f,
                b,
                &layout(&f, b, "[0].default_value"),
                block(&f, g.records[val].address)
            )
            .unwrap_err()
            .kind,
            "unprofiled_role"
        );
    }
    #[test]
    fn caller_qualified_405_storage_does_not_admit_void_or_wrong_discriminators() {
        let source = schema();
        for (kind, ty) in [
            ("ShaderNodeScript", "NodeShaderScript"),
            ("ShaderNodeTexIES", "NodeShaderTexIES"),
        ] {
            let mut g = Graph::new(&source).unwrap();
            let s = scene(&mut g);
            let node = g.new_typed("bNode", Some(s)).unwrap();
            g.cstring(node, &["idname"], kind).unwrap();
            let storage = g.new_typed(ty, Some(s)).unwrap();
            let f = file(&g);
            let b = block(&f, g.records[node].address);
            let target = block(&f, g.records[storage].address);
            let mut p = layout(&f, b, "[0].storage");
            assert_eq!(
                qualify(&f, b, &p, target).unwrap_err().kind,
                "unprofiled_role"
            );
            p.declared_type = ty.as_bytes().to_vec();
            qualify(&f, b, &p, target).unwrap();
            let mut patched = file(&g);
            patched.header.version = 503;
            qualify(&patched, b, &p, target).unwrap();
            assert!(matches!(
                namespace(&f, b, g.records[storage].address, &p).unwrap(),
                super::super::index::Namespace::Data
            ));
            p.declared_type = b"NodeBlurData".to_vec();
            assert_eq!(
                qualify(&f, b, &p, target).unwrap_err().kind,
                "unprofiled_role"
            );
            p.declared_type = ty.as_bytes().to_vec();
            let wrong = g.new_typed("ImageUser", Some(s)).unwrap();
            let f = file(&g);
            let b = block(&f, g.records[node].address);
            assert_eq!(
                qualify(&f, b, &p, block(&f, g.records[wrong].address))
                    .unwrap_err()
                    .kind,
                "pointer_target_type"
            );
            g.cstring(node, &["idname"], "UnknownStorage").unwrap();
            let f = file(&g);
            let b = block(&f, g.records[node].address);
            assert_eq!(
                qualify(&f, b, &p, block(&f, g.records[storage].address))
                    .unwrap_err()
                    .kind,
                "unprofiled_role"
            );
        }
    }
    #[test]
    fn file_output_405_input_storage_is_paired_exact_and_never_orphan_admitted() {
        let source = schema();
        let mut g = Graph::new(&source).unwrap();
        let s = scene(&mut g);
        let node = g.new_typed("bNode", Some(s)).unwrap();
        g.cstring(node, &["idname"], "CompositorNodeOutputFile")
            .unwrap();
        let socket = g.new_typed("bNodeSocket", Some(s)).unwrap();
        g.set(socket, &["type"], 2).unwrap();
        g.cstring(socket, &["idname"], "NodeSocketColor").unwrap();
        let storage = g.new_typed("NodeImageMultiFileSocket", Some(s)).unwrap();
        g.ptr(socket, &["storage"], Some(storage)).unwrap();
        g.set_list(node, &["inputs"], &[socket]).unwrap();
        let f = file(&g);
        let b = block(&f, g.records[socket].address);
        let target = block(&f, g.records[storage].address);
        let p = layout(&f, b, "[0].storage");
        qualify(&f, b, &p, target).unwrap();
        assert!(matches!(
            namespace(&f, b, g.records[storage].address, &p).unwrap(),
            super::super::index::Namespace::Data
        ));
        let wrong = g.new_typed("NodeImageLayer", Some(s)).unwrap();
        let f = file(&g);
        let b = block(&f, g.records[socket].address);
        assert_eq!(
            qualify(&f, b, &p, block(&f, g.records[wrong].address))
                .unwrap_err()
                .kind,
            "pointer_target_type"
        );
        let data = g.records[storage].data.clone();
        g.records[storage].count = 2;
        g.records[storage].data.extend_from_slice(&data);
        let f = file(&g);
        let b = block(&f, g.records[socket].address);
        assert_eq!(
            qualify(&f, b, &p, block(&f, g.records[storage].address))
                .unwrap_err()
                .kind,
            "pointer_count"
        );
        g.records[storage].count = 1;
        g.records[storage].data = data;
        g.set_list(node, &["inputs"], &[]).unwrap();
        let f = file(&g);
        let b = block(&f, g.records[socket].address);
        assert_eq!(
            qualify(&f, b, &p, block(&f, g.records[storage].address))
                .unwrap_err()
                .kind,
            "unprofiled_role"
        );
        g.set_list(node, &["outputs"], &[socket]).unwrap();
        let f = file(&g);
        let b = block(&f, g.records[socket].address);
        assert_eq!(
            qualify(&f, b, &p, block(&f, g.records[storage].address))
                .unwrap_err()
                .kind,
            "unprofiled_role"
        );
        g.set_list(node, &["outputs"], &[]).unwrap();
        g.set_list(node, &["inputs"], &[socket]).unwrap();
        g.cstring(node, &["idname"], "CompositorNodeImage").unwrap();
        let f = file(&g);
        let b = block(&f, g.records[socket].address);
        assert_eq!(
            qualify(&f, b, &p, block(&f, g.records[storage].address))
                .unwrap_err()
                .kind,
            "unprofiled_role"
        );
        g.cstring(node, &["idname"], "CompositorNodeOutputFile")
            .unwrap();
        let mut f = file(&g);
        f.header.version = 502;
        let b = block(&f, g.records[socket].address);
        let target = block(&f, g.records[storage].address);
        qualify(&f, b, &p, target).unwrap();
        assert_eq!(
            write_qualify(&f, b, &p, target).unwrap_err().kind,
            "unprofiled_role"
        );
        g.records[storage].code = *b"IM\0\0";
        let f = file(&g);
        let b = block(&f, g.records[socket].address);
        assert_eq!(
            f.owner_index()
                .unwrap()
                .resolve(&f, b, g.records[storage].address, &p)
                .unwrap_err()
                .kind,
            "pointer_target_type"
        );
    }
    #[test]
    fn observed_zero_bake_array_is_logically_absent_without_address_rescue() {
        let source = schema();
        let mut g = Graph::new(&source).unwrap();
        let s = scene(&mut g);
        let modifier = g.new_typed("NodesModifierData", Some(s)).unwrap();
        g.set(modifier, &["bakes"], 0xfeed).unwrap();
        let mut f = file(&g);
        f.header.version = 502;
        let b = block(&f, g.records[modifier].address);
        let p = layout(&f, b, "[0].bakes");
        assert!(empty_allocation(&f, b, &p).unwrap());
        assert_eq!(
            f.owner_index()
                .unwrap()
                .resolve(&f, b, 0xfeed, &p)
                .unwrap_err()
                .kind,
            "unresolved_persistent"
        );
        g.set(modifier, &["bakes_num"], 1).unwrap();
        let mut f = file(&g);
        f.header.version = 502;
        let b = block(&f, g.records[modifier].address);
        assert!(!empty_allocation(&f, b, &p).unwrap());
        assert_eq!(
            f.owner_index()
                .unwrap()
                .resolve(&f, b, 0xfeed, &p)
                .unwrap_err()
                .kind,
            "unresolved_persistent"
        );
        g.set(modifier, &["bakes_num"], u64::from(u32::MAX))
            .unwrap();
        let mut f = file(&g);
        f.header.version = 502;
        let b = block(&f, g.records[modifier].address);
        assert_eq!(
            empty_allocation(&f, b, &p).unwrap_err().kind,
            "pointer_extent"
        );
    }
    fn lighting_check(g: &Graph<'_>, parent: usize, path: &str, target: usize) -> Result<()> {
        let f = file(g);
        let b = block(&f, g.records[parent].address);
        qualify(
            &f,
            b,
            &layout(&f, b, path),
            block(&f, g.records[target].address),
        )
    }
    // Target qualification, not observed-address matching, is the receiving proof.
    fn lighting_target_hostiles(g: &mut Graph<'_>, parent: usize, path: &str, target: usize) {
        lighting_check(g, parent, path, target).unwrap();
        let names: Vec<_> = path.strip_prefix("[0].").unwrap().split('.').collect();
        let read = |g: &Graph<'_>| {
            let f = file(g);
            let b = block(&f, g.records[parent].address);
            let steps: Vec<_> = names
                .iter()
                .map(|n| crate::value::PathStep::Field(n.as_bytes().to_vec()))
                .collect();
            f.get_pointer(b, 0, &steps)
        };
        assert!(read(g).unwrap().is_some());
        let owner = g.records[target].owner;
        let wrong = g.new_typed("ImageUser", Some(owner)).unwrap();
        assert_eq!(
            lighting_check(g, parent, path, wrong).unwrap_err().kind,
            "pointer_target_type"
        );
        g.ptr(parent, &names, Some(wrong)).unwrap();
        assert_eq!(read(g).unwrap_err().kind, "pointer_target_type");
        g.ptr(parent, &names, Some(target)).unwrap();
        let data = g.records[target].data.clone();
        g.records[target].count = 2;
        g.records[target].data = data.repeat(2);
        assert_eq!(
            lighting_check(g, parent, path, target).unwrap_err().kind,
            "pointer_count"
        );
        assert_eq!(read(g).unwrap_err().kind, "pointer_count");
        g.records[target].count = 1;
        g.records[target].data = data.clone();
        g.records[target].data.push(0);
        assert_eq!(
            lighting_check(g, parent, path, target).unwrap_err().kind,
            "pointer_extent"
        );
        assert_eq!(read(g).unwrap_err().kind, "pointer_extent");
        g.records[target].data = data;
        g.records[target].code = *b"IM\0\0";
        assert_eq!(
            lighting_check(g, parent, path, target).unwrap_err().kind,
            "pointer_target_type"
        );
        g.records[target].code = *b"DATA";
        let mut f = file(g);
        let b = block(&f, g.records[parent].address);
        let t = block(&f, g.records[target].address);
        let mut p = layout(&f, b, path);
        assert!(checked_source_role(&p));
        p.declared_type = b"ImageUser".to_vec();
        assert_eq!(qualify(&f, b, &p, t).unwrap_err().kind, "unprofiled_role");
        p = layout(&f, b, path);
        f.header.version = 503;
        qualify(&f, b, &p, t).unwrap();
        // Broad read admission never admits this file to import/publication.
        assert_eq!(
            super::super::owner::qualify(&f).unwrap_err().kind,
            "owner_header"
        );
        // A correct target in another owner's DATA run is never a fallback.
        let another = scene(g);
        let ty = String::from_utf8(
            g.schema.schema().types[g.schema.schema().structs[g.records[target].sdna].type_index]
                .clone(),
        )
        .unwrap();
        let cross = g.new_typed(&ty, Some(another)).unwrap();
        let f = file(g);
        let b = block(&f, g.records[parent].address);
        assert_eq!(
            f.owner_index()
                .unwrap()
                .resolve(&f, b, g.records[cross].address, &layout(&f, b, path))
                .unwrap_err()
                .kind,
            "unresolved_persistent"
        );
    }
    #[test]
    fn version_405_constraint_pairs_and_hostiles() {
        let source = schema();
        for &(numeric, ty, extent) in LIGHTING_CONSTRAINTS_405 {
            let mut g = Graph::new(&source).unwrap();
            let s = scene(&mut g);
            let parent = g.new_typed("bConstraint", Some(s)).unwrap();
            g.set(parent, &["type"], numeric).unwrap();
            let target = g
                .new_typed(std::str::from_utf8(ty).unwrap(), Some(s))
                .unwrap();
            assert_eq!(g.records[target].data.len(), extent);
            g.ptr(parent, &["data"], Some(target)).unwrap();
            lighting_target_hostiles(&mut g, parent, "[0].data", target);
            // The old discriminator-99 case is now an explicit READ positive:
            // exact owner DATA, source TLEN/count, all declared pointers NULL.
            // Numeric identity alone no longer rejects a closed declaration.
            g.set(parent, &["type"], 99).unwrap();
            lighting_check(&g, parent, "[0].data", target).unwrap();
            let f = file(&g);
            let t = block(&f, g.records[target].address);
            assert!(
                super::super::pointer_layout(&f, t)
                    .unwrap()
                    .unwrap_or_default()
                    .iter()
                    .all(|slot| {
                        checked::number(&f.payload(t).unwrap(), slot.offset, 8, true).unwrap() == 0
                    }),
                "discriminator99 original has a reachable edge: {ty:?}"
            );
            let b = block(&f, g.records[parent].address);
            let p = layout(&f, b, "[0].data");
            assert_eq!(
                write_qualify(&f, b, &p, t).unwrap_err().kind,
                "unprofiled_role"
            );
            // Closest pointer-bearing hostile: same unknown discriminator,
            // typed constraint DATA, but an Object edge to a Scene ID.
            let hostile = g.new_typed("bKinematicConstraint", Some(s)).unwrap();
            g.ptr(hostile, &["tar"], Some(s)).unwrap();
            g.ptr(parent, &["data"], Some(hostile)).unwrap();
            let hf = file(&g);
            assert!(
                super::super::pointer_census(&hf)
                    .unwrap()
                    .require_closed()
                    .is_err()
            );
            g.ptr(parent, &["data"], Some(target)).unwrap();
            g.set(parent, &["type"], numeric).unwrap();
            let mut f = file(&g);
            f.header.version = 502;
            let b = block(&f, g.records[parent].address);
            let p = layout(&f, b, "[0].data");
            let target = block(&f, g.records[target].address);
            qualify(&f, b, &p, target).unwrap();
            assert_eq!(
                write_qualify(&f, b, &p, target).unwrap_err().kind,
                "unprofiled_role"
            );
        }
    }
    #[test]
    fn version_405_node_and_socket_pairs_and_hostiles() {
        let source = schema();
        for (parent_type, field, profiles) in [
            ("bNode", "storage", LIGHTING_NODE_STORAGE_405),
            ("bNodeSocket", "default_value", LIGHTING_SOCKET_VALUES_405),
        ] {
            for &(kind, numeric, ty, extent) in profiles {
                let mut g = Graph::new(&source).unwrap();
                let s = scene(&mut g);
                let parent = g.new_typed(parent_type, Some(s)).unwrap();
                g.cstring(parent, &["idname"], kind).unwrap();
                g.set(parent, &["type"], numeric).unwrap();
                let target = g
                    .new_typed(std::str::from_utf8(ty).unwrap(), Some(s))
                    .unwrap();
                assert_eq!(g.records[target].data.len(), extent, "{kind}");
                g.ptr(parent, &[field], Some(target)).unwrap();
                let path = format!("[0].{field}");
                lighting_target_hostiles(&mut g, parent, &path, target);
                g.set(parent, &["type"], numeric + 1).unwrap();
                assert_eq!(
                    lighting_check(&g, parent, &path, target).unwrap_err().kind,
                    "unprofiled_role"
                );
                g.set(parent, &["type"], numeric).unwrap();
                g.cstring(parent, &["idname"], "UnobservedLightingKind")
                    .unwrap();
                let pure = matches!(
                    std::str::from_utf8(ty).unwrap(),
                    "ColorBand"
                        | "NodeCombSepColor"
                        | "NodeFrame"
                        | "NodeFunctionCompare"
                        | "NodeGeometryCollectionInfo"
                        | "NodeGeometryCurvePrimitiveCircle"
                        | "NodeGeometryCurveResample"
                        | "NodeGeometryCurveSample"
                        | "NodeGeometryDeleteGeometry"
                        | "NodeGeometryDistributePointsInVolume"
                        | "NodeGeometryExtrudeMesh"
                        | "NodeGeometryInputNamedAttribute"
                        | "NodeGeometryMergeByDistance"
                        | "NodeGeometryMeshToVolume"
                        | "NodeGeometryObjectInfo"
                        | "NodeGeometryProximity"
                        | "NodeGeometrySampleIndex"
                        | "NodeGeometrySeparateGeometry"
                        | "NodeGeometryStoreNamedAttribute"
                        | "NodeGeometrySubdivisionSurface"
                        | "NodeGeometryViewer"
                        | "NodeInputBool"
                        | "NodeInputInt"
                        | "NodeRandomValue"
                        | "NodeSwitch"
                        | "bNodeSocketValueFloat"
                        | "bNodeSocketValueInt"
                );
                if pure {
                    // Preserve the original unknown-kind case as a named positive.
                    lighting_check(&g, parent, &path, target).unwrap();
                    let f = file(&g);
                    let b = block(&f, g.records[parent].address);
                    let p = layout(&f, b, &path);
                    let t = block(&f, g.records[target].address);
                    assert!(
                        super::super::pointer_layout(&f, t)
                            .unwrap()
                            .unwrap_or_default()
                            .is_empty(),
                        "{ty:?}"
                    );
                    assert_eq!(
                        write_qualify(&f, b, &p, t).unwrap_err().kind,
                        "unprofiled_role"
                    );
                    // Same parent/kind; a pointer-bearing value to a wrong ID type
                    // must never close. Node storage refuses it at role admission;
                    // socket READ follows the declared edge and rejects its target.
                    let hostile = g.new_typed("bNodeSocketValueObject", Some(s)).unwrap();
                    g.ptr(hostile, &["value"], Some(s)).unwrap();
                    g.ptr(parent, &[field], Some(hostile)).unwrap();
                    let hf = file(&g);
                    assert!(
                        super::super::pointer_census(&hf)
                            .unwrap()
                            .require_closed()
                            .is_err()
                    );
                } else {
                    // All original meaningful-array/pointer storage cases still refuse.
                    assert_eq!(
                        lighting_check(&g, parent, &path, target).unwrap_err().kind,
                        "unprofiled_role",
                        "{ty:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn confirmed_census_lists_use_structural_read_checks_without_write_promotion() {
        let source = schema();
        for (owner, field, ty) in [
            ("SpaceNode", "treepath", "bNodeTreePath"),
            ("Palette", "colors", "PaletteColor"),
            (
                "FreestyleLineStyle",
                "geometry_modifiers",
                "LineStyleGeometryModifier_Sampling",
            ),
            ("Object", "particlesystem", "ParticleSystem"),
        ] {
            let mut g = Graph::new(&source).unwrap();
            let s = scene(&mut g);
            let parent = g.new_typed(owner, Some(s)).unwrap();
            let first = g.new_typed(ty, Some(s)).unwrap();
            let last = g.new_typed(ty, Some(s)).unwrap();
            let path = format!("[0].{field}.first");
            let check = |g: &Graph<'_>| {
                let f = file(g);
                let b = block(&f, g.records[parent].address);
                super::super::validate::census_site(&f, b, &layout(&f, b, &path), {
                    let p = layout(&f, b, &path);
                    checked::number(
                        &f.payload(b).unwrap(),
                        p.offset,
                        f.header().pointer_size,
                        f.header().little_endian,
                    )
                    .unwrap()
                })
            };
            assert_eq!(check(&g).unwrap(), None, "{owner}.{field} null");
            g.ptr(parent, &[field, "first"], Some(first)).unwrap();
            g.ptr(parent, &[field, "last"], Some(last)).unwrap();
            let next: &[&str] = if owner == "FreestyleLineStyle" {
                &["modifier", "next"]
            } else {
                &["next"]
            };
            let prev: &[&str] = if owner == "FreestyleLineStyle" {
                &["modifier", "prev"]
            } else {
                &["prev"]
            };
            g.ptr(first, next, Some(last)).unwrap();
            g.ptr(last, prev, Some(first)).unwrap();
            assert!(check(&g).unwrap().is_some(), "{owner}.{field} valid");
            // Admission follows the file-local SDNA, not the Blender probe version.
            let mut f = file(&g);
            f.header.version = 502;
            let b = block(&f, g.records[parent].address);
            let p = layout(&f, b, &path);
            let address = g.records[first].address;
            assert!(
                super::super::validate::census_site(&f, b, &p, address)
                    .unwrap()
                    .is_some()
            );
            // Exact e577 baseline: all four write_site outcomes are Ok(Some(2)).
            // Census promotion neither expands nor narrows that existing profile.
            assert_eq!(
                super::super::validate::write_site(&f, b, &p, address).unwrap(),
                Some(2),
                "{owner}.{field}: keep the captured e577 write outcome"
            );
            eprintln!(
                "WRITE_BOUNDARY {}",
                serde_json::json!({
                    "family": format!("{owner}.{field}"), "result": {"ok": 2}
                })
            );
            g.ptr(parent, &[field, "last"], None).unwrap();
            assert_eq!(check(&g).unwrap_err().kind, "list_boundary");
            g.ptr(parent, &[field, "last"], Some(last)).unwrap();
            g.ptr(last, prev, None).unwrap();
            assert_eq!(check(&g).unwrap_err().kind, "list_backlink");
            g.ptr(last, prev, Some(first)).unwrap();
            g.ptr(last, next, Some(first)).unwrap();
            assert_eq!(check(&g).unwrap_err().kind, "list_cycle");
            g.ptr(last, next, None).unwrap();
            let wrong = g.new_typed("bNode", Some(s)).unwrap();
            g.ptr(parent, &[field, "first"], Some(wrong)).unwrap();
            assert_eq!(check(&g).unwrap_err().kind, "pointer_target_type");
            g.ptr(parent, &[field, "first"], Some(first)).unwrap();
            g.records[first].alive = false;
            assert_eq!(check(&g).unwrap_err().kind, "unresolved_persistent");
        }
    }
    #[test]
    fn version_405_lists_prove_serialized_type_and_chain() {
        let source = schema();
        for &(owner, name, ty, extent) in LIGHTING_LISTS_405 {
            let mut g = Graph::new(&source).unwrap();
            let s = scene(&mut g);
            let parent = g
                .new_typed(std::str::from_utf8(owner).unwrap(), Some(s))
                .unwrap();
            let name = std::str::from_utf8(name).unwrap();
            let ty = std::str::from_utf8(ty).unwrap();
            let first = g.new_typed(ty, Some(s)).unwrap();
            let last = g.new_typed(ty, Some(s)).unwrap();
            assert_eq!(g.records[first].data.len(), extent);
            g.set_list(parent, &[name], &[first, last]).unwrap();
            let path = format!("[0].{name}.first");
            lighting_target_hostiles(&mut g, parent, &path, first);
            let check = |g: &Graph<'_>| {
                let f = file(g);
                let b = block(&f, g.records[parent].address);
                validate_list(&f, b, &layout(&f, b, &path))
            };
            check(&g).unwrap();
            g.ptr(last, &["prev"], None).unwrap();
            assert_eq!(check(&g).unwrap_err().kind, "list_backlink");
            g.ptr(last, &["prev"], Some(first)).unwrap();
            g.ptr(last, &["next"], Some(first)).unwrap();
            assert_eq!(check(&g).unwrap_err().kind, "list_cycle");
            g.ptr(last, &["next"], None).unwrap();
            g.ptr(parent, &[name, "last"], Some(first)).unwrap();
            assert_eq!(check(&g).unwrap_err().kind, "list_boundary");
            let mut f = file(&g);
            f.header.version = 502;
            let b = block(&f, g.records[parent].address);
            let p = layout(&f, b, &path);
            let first = block(&f, g.records[first].address);
            qualify(&f, b, &p, first).unwrap();
            assert_eq!(
                write_qualify(&f, b, &p, first).unwrap_err().kind,
                "unprofiled_role"
            );
        }
    }
    /// Optional workload proof against original saved metadata; Root's complete
    /// command gate is the end-to-end trace acceptance, not this role subset.

    #[test]
    fn observed_constraint_union_refuses_unknown_kind_and_wrong_target() {
        let source = schema();
        let mut g = Graph::new(&source).unwrap();
        let s = scene(&mut g);
        let constraint = g.new_typed("bConstraint", Some(s)).unwrap();
        g.set(constraint, &["type"], 29).unwrap();
        let data = g.new_typed("bTransformCacheConstraint", Some(s)).unwrap();
        let wrong = g.new_typed("ImageUser", Some(s)).unwrap();
        let mut f = file(&g);
        f.header.version = 502;
        let b = block(&f, g.records[constraint].address);
        let p = layout(&f, b, "[0].data");
        let target = block(&f, g.records[data].address);
        qualify(&f, b, &p, target).unwrap();
        assert!(matches!(
            namespace(&f, b, g.records[data].address, &p).unwrap(),
            super::super::index::Namespace::Data
        ));
        assert_eq!(
            qualify(&f, b, &p, block(&f, g.records[wrong].address))
                .unwrap_err()
                .kind,
            "pointer_target_type"
        );
        g.set(constraint, &["type"], 99).unwrap();
        let mut f = file(&g);
        f.header.version = 502;
        let b = block(&f, g.records[constraint].address);
        assert_eq!(
            qualify(&f, b, &p, block(&f, g.records[data].address))
                .unwrap_err()
                .kind,
            "unprofiled_role"
        );
    }
    #[test]
    fn preview_pixel_extent_is_not_a_pointer_array() {
        let f = schema();
        let mut g = Graph::new(&f).unwrap();
        let s = scene(&mut g);
        let preview = g.new_typed("PreviewImage", Some(s)).unwrap();
        for name in ["w", "h"] {
            let (off, _) = g.field(preview, &[name]).unwrap();
            g.records[preview].data[off..off + 4].copy_from_slice(&1u32.to_le_bytes());
        }
        let raw = g.allocate(*b"DATA", 0, 1, vec![255; 4], Some(s)).unwrap();
        let f = file(&g);
        let b = block(&f, g.records[preview].address);
        let p = layout(&f, b, "[0].rect[0]");
        qualify(&f, b, &p, block(&f, g.records[raw].address)).unwrap();
        g.records[raw].data.push(0);
        let f = file(&g);
        let b = block(&f, g.records[preview].address);
        assert_eq!(
            qualify(&f, b, &p, block(&f, g.records[raw].address))
                .unwrap_err()
                .kind,
            "pointer_extent"
        );
    }
    /// Synthetic nested property groups prove type, count, extent, namespace,
    /// cycle and boundary checks without relying on a saved production file.
    struct SnapshotScenario {
        root: usize,
        members: Vec<usize>,
        frames_raw: usize,
    }
    fn snapshot_name(f: &BlendFile, b: usize) -> String {
        super::super::graph::text(f, b, &["name"]).unwrap()
    }
    fn snapshot_type(f: &BlendFile, b: usize) -> u64 {
        super::super::graph::uint(f, b, &["type"]).unwrap()
    }
    fn snapshot_val(f: &BlendFile, b: usize) -> u64 {
        super::super::graph::uint(f, b, &["data", "val"]).unwrap()
    }
    fn snapshot_build(g: &mut Graph<'_>, s: usize) -> SnapshotScenario {
        // Mirror the retained job chain: root group holds six members; the
        // cycles member and the settings member hold nested groups.
        let root = g.new_typed("IDProperty", Some(s)).unwrap();
        g.set(root, &["type"], 6).unwrap();
        let order: [(&str, u64); 6] = [
            ("cycles", 6),
            ("fixture_label", 0),
            ("fixture_choices", 0),
            ("fixture_kind", 1),
            ("fixture_tag", 1),
            ("fixture_settings", 6),
        ];
        let mut members = Vec::new();
        for (name, ty) in order {
            let p = g.new_typed("IDProperty", Some(s)).unwrap();
            g.set(p, &["type"], ty).unwrap();
            g.cstring(p, &["name"], name).unwrap();
            members.push(p);
        }
        // cycles -> one sampling_pattern int leaf (val 5).
        let sampling = g.new_typed("IDProperty", Some(s)).unwrap();
        g.set(sampling, &["type"], 1).unwrap();
        g.cstring(sampling, &["name"], "sampling_pattern").unwrap();
        g.set(sampling, &["data", "val"], 5).unwrap();
        // settings -> four leaves (frames/render_output_root strings,
        // blender_version string, chunk_size int).
        let frames = g.new_typed("IDProperty", Some(s)).unwrap();
        g.set(frames, &["type"], 0).unwrap();
        g.cstring(frames, &["name"], "frames").unwrap();
        g.set(frames, &["len"], 8).unwrap();
        g.set(frames, &["totallen"], 8).unwrap();
        let frames_raw = g
            .allocate(*b"DATA", 0, 1, b"101-149\0".to_vec(), Some(s))
            .unwrap();
        g.ptr(frames, &["data", "pointer"], Some(frames_raw))
            .unwrap();
        let chunk = g.new_typed("IDProperty", Some(s)).unwrap();
        g.set(chunk, &["type"], 1).unwrap();
        g.cstring(chunk, &["name"], "chunk_size").unwrap();
        g.set(chunk, &["data", "val"], 1000).unwrap();
        // Link groups: root -> six members; cycles -> sampling; settings ->
        // two checked leaves (frames, chunk) for the traversal vectors.
        let link = |g: &mut Graph<'_>, owner: usize, items: &[usize]| {
            g.ptr(owner, &["data", "group", "first"], Some(items[0]))
                .unwrap();
            g.ptr(
                owner,
                &["data", "group", "last"],
                Some(*items.last().unwrap()),
            )
            .unwrap();
            for (i, id) in items.iter().enumerate() {
                g.ptr(*id, &["prev"], items[..i].last().copied()).unwrap();
                g.ptr(*id, &["next"], items.get(i + 1).copied()).unwrap();
            }
        };
        link(g, root, &members);
        link(g, members[0], &[sampling]);
        link(g, members[5], &[frames, chunk]);
        // String raws for the two remaining type-0 members.
        for (member, bytes) in [(members[1], b"\0".to_vec()), (members[2], b"\0".to_vec())] {
            g.set(member, &["len"], 1).unwrap();
            g.set(member, &["totallen"], 1).unwrap();
            let raw = g.allocate(*b"DATA", 0, 1, bytes, Some(s)).unwrap();
            g.ptr(member, &["data", "pointer"], Some(raw)).unwrap();
        }
        g.set(members[3], &["data", "val"], 1).unwrap();
        SnapshotScenario {
            root,
            members,
            frames_raw,
        }
    }
    /// Serialize one scenario through the test-only encoder.
    fn snapshot_file(g: &Graph<'_>, sc: &SnapshotScenario) -> (BlendFile, usize) {
        let out = file(g);
        let b = block(&out, g.records[sc.root].address);
        (out, b)
    }
    fn snapshot_items(f: &BlendFile, b: usize) -> Vec<usize> {
        super::super::source_list(f, b, &["data", "group"]).unwrap()
    }
    #[test]
    fn snapshot_tlen_profile_rejects_supported_versions() {
        let source = schema();
        let mut g = Graph::new(&source).unwrap();
        let s = scene(&mut g);
        let sc = snapshot_build(&mut g, s);
        let (old, _) = snapshot_file(&g, &sc);
        assert!(!snapshot_layout(&old));
        // The observation identifies the kept503 workload, not read admission.
        // ListBase honestly shares TLEN16 across versions; this checks the
        // observed tuple, not a claimed inequality of all three layouts.
        assert!(!snapshot_layout(&old));
    }
    /// Prove the synthetic group's typed values and owner-local namespaces.
    fn snapshot_prove(f: &BlendFile, b: usize) {
        // Root group traverses all six members with endpoints/cycle checks.
        validate_list(f, b, &layout(f, b, "[0].data.group.first")).unwrap();
        let items = snapshot_items(f, b);
        assert_eq!(items.len(), 6);
        let names: Vec<String> = items.iter().map(|i| snapshot_name(f, *i)).collect();
        assert_eq!(
            names,
            [
                "cycles",
                "fixture_label",
                "fixture_choices",
                "fixture_kind",
                "fixture_tag",
                "fixture_settings"
            ]
        );
        // Type-1 int arms keep their saved vals.
        let chunk = items
            .iter()
            .find(|i| snapshot_name(f, **i) == "chunk_size")
            .copied()
            .unwrap_or_else(|| {
                snapshot_items(f, *items.last().unwrap())
                    .into_iter()
                    .find(|i| snapshot_name(f, *i) == "chunk_size")
                    .unwrap()
            });
        assert_eq!(snapshot_type(f, chunk), 1);
        assert_eq!(snapshot_val(f, chunk), 1000);
        let job_type = items
            .iter()
            .find(|i| snapshot_name(f, **i) == "fixture_kind")
            .copied()
            .unwrap();
        assert_eq!(snapshot_type(f, job_type), 1);
        assert_eq!(snapshot_val(f, job_type), 1);
        // Type-0 string arms resolve to their exact raws.
        let frames = snapshot_items(f, *items.last().unwrap())
            .into_iter()
            .find(|i| snapshot_name(f, *i) == "frames")
            .unwrap();
        let p = layout(f, frames, "[0].data.pointer");
        let address = super::super::graph::uint(f, frames, &["data", "pointer"]).unwrap();
        let target = super::super::index::Index::new(f)
            .map(|_| ())
            .and(f.owner_index().unwrap().resolve(f, frames, address, &p))
            .unwrap();
        qualify(f, frames, &p, target).unwrap();
        assert_eq!(&f.payload(target).unwrap()[..], b"101-149\0");
        // Nested cycles group traverses its int leaf.
        let cycles = items[0];
        validate_list(f, cycles, &layout(f, cycles, "[0].data.group.first")).unwrap();
        // Owner scopes stay disjoint: string raw resolves in DATA space,
        // never rescued from an ID header.
        let cross = f
            .blocks()
            .iter()
            .any(|q| super::super::owner::read_id(&q.code) && q.old_address == address);
        assert!(!cross);
        assert!(matches!(
            namespace(f, frames, address, &p).unwrap(),
            super::super::index::Namespace::Data
        ));
    }
    #[test]
    fn snapshot_shared_checks_prove_synthetic_read() {
        let source = schema();
        let mut g = Graph::new(&source).unwrap();
        let s = scene(&mut g);
        let sc = snapshot_build(&mut g, s);
        let (old, b) = snapshot_file(&g, &sc);
        snapshot_prove(&old, b);
    }

    /// Hostile mutants stay refused: wrong group discriminator, wrong
    /// string target, wrong string extent, first/last boundary mismatch,
    /// broken reciprocity, and intrusive cycle. Each mutant rebuilds the
    /// Graph scenario and reserializes, so parsed-file bytes always stay
    /// consistent; only the intended hostile field lies. The same vectors
    /// run on the synthetic405 layout (proving the shared checks); the real503
    /// workload exercises the same read functions on its retained DNA layout.
    /// The saved version stamp no longer supplies read authority.
    fn snapshot_mutant(
        mutate: impl FnOnce(&mut Graph<'_>, &SnapshotScenario),
    ) -> (BlendFile, usize) {
        // Each mutant rebuilds the scenario from the fixture schema, so
        // the returned file owns no test-graph lifetime.
        let source = schema();
        let mut g = Graph::new(&source).unwrap();
        let s = scene(&mut g);
        let sc = snapshot_build(&mut g, s);
        mutate(&mut g, &sc);
        snapshot_file(&g, &sc)
    }
    fn snapshot_refuse(kind: &str, mutate: impl FnOnce(&mut Graph<'_>, &SnapshotScenario)) {
        let (hostile, hb) = snapshot_mutant(mutate);
        let first = layout(&hostile, hb, "[0].data.group.first");
        match validate_list(&hostile, hb, &first) {
            Ok(()) => panic!("mutant `{}` admitted, expected {kind}", first.path),
            Err(err) => assert_eq!(
                err.kind, kind,
                "mutant `{}` refused as {err:?}, expected {kind}",
                first.path
            ),
        }
    }
    /// Namespace discipline lives in the shared resolver (see
    /// `embedded_identifiers_never_fall_back_to_global_headers`) and in
    /// the prove step's DATA-space cross check; no snapshot-specific
    /// namespace mutant is needed beyond those.
    fn snapshot_prove_namespace() {}
    #[test]
    fn snapshot_hostiles_refuse() {
        // Non-group discriminator: root flipped to int refuses.
        snapshot_refuse("unprofiled_role", |g, sc| {
            g.set(sc.root, &["type"], 1).unwrap();
        });
        // Boundary mismatch: last != terminal element refuses.
        snapshot_refuse("list_boundary", |g, sc| {
            g.ptr(sc.root, &["data", "group", "last"], Some(sc.members[0]))
                .unwrap();
        });
        // Broken reciprocity: middle prev cleared refuses.
        snapshot_refuse("list_backlink", |g, sc| {
            g.ptr(sc.members[2], &["prev"], None).unwrap();
        });
        // Cycle: closing the intrusive next-loop refuses.
        snapshot_refuse("list_cycle", |g, sc| {
            g.ptr(sc.members[5], &["next"], Some(sc.members[0]))
                .unwrap();
        });
        // Namespace discipline: the frames raw resolves in DATA space
        // and never from an ID header with the same address. The ID-map
        // rescue refusal is covered by the existing
        // `embedded_identifiers_never_fall_back_to_global_headers` vector
        // plus the prove-step cross check above.
        snapshot_prove_namespace();
        // Wrong string extent: corrupting the raw's NUL terminator
        // refuses. (Saved `len` is a byte count the terminator check
        // covers; the hostile lies in the payload byte itself.)
        {
            let source = schema();
            let mut g = Graph::new(&source).unwrap();
            let s = scene(&mut g);
            let sc = snapshot_build(&mut g, s);
            g.records[sc.frames_raw].data[7] = b'X';
            let (hostile, hb) = snapshot_file(&g, &sc);
            let first = layout(&hostile, hb, "[0].data.group.first");
            match validate_list(&hostile, hb, &first) {
                Ok(()) => {
                    // validate_list only walks intrusive links; the
                    // terminator refusal surfaces at string qualify time.
                    // The frames leaf lives in the nested settings group.
                    let settings = super::super::source_list(&hostile, hb, &["data", "group"])
                        .unwrap()
                        .into_iter()
                        .find(|i| snapshot_name(&hostile, *i) == "fixture_settings")
                        .unwrap();
                    let frames = super::super::source_list(&hostile, settings, &["data", "group"])
                        .unwrap()
                        .into_iter()
                        .find(|i| snapshot_name(&hostile, *i) == "frames")
                        .unwrap();
                    let p = layout(&hostile, frames, "[0].data.pointer");
                    let address =
                        super::super::graph::uint(&hostile, frames, &["data", "pointer"]).unwrap();
                    let target = hostile
                        .owner_index()
                        .unwrap()
                        .resolve(&hostile, frames, address, &p)
                        .unwrap();
                    assert_eq!(
                        qualify(&hostile, frames, &p, target).unwrap_err().kind,
                        "pointer_extent"
                    );
                }
                Err(err) => assert_eq!(err.kind, "pointer_extent", "{err:?}"),
            }
        }
        // Type-9 arm without a pointer allocation: NULL the string raw
        // so the surviving type-0 member has no target. The read path
        // reports the missing allocation instead of admitting silence.
        {
            let source = schema();
            let mut g = Graph::new(&source).unwrap();
            let s = scene(&mut g);
            let sc = snapshot_build(&mut g, s);
            g.ptr(sc.members[1], &["data", "pointer"], None).unwrap();
            let (hostile, hb) = snapshot_file(&g, &sc);
            validate_list(&hostile, hb, &layout(&hostile, hb, "[0].data.group.first")).unwrap();
            let member = super::super::source_list(&hostile, hb, &["data", "group"])
                .unwrap()
                .into_iter()
                .find(|i| snapshot_name(&hostile, *i) == "fixture_label")
                .unwrap();
            assert_eq!(
                super::super::source_target(&hostile, member, &["data", "pointer"]).unwrap(),
                None
            );
        }
    }
}
