pub(crate) const TYPES: &[(&[u8], &[&[u8]])] = &[
    (b"AC", &[b"bAction"]),
    (b"AR", &[b"bArmature"]),
    (b"BR", &[b"Brush"]),
    (b"CA", &[b"Camera"]),
    (b"CF", &[b"CacheFile"]),
    (b"CU", &[b"Curve"]),
    (b"CV", &[b"Curves"]),
    (b"GD", &[b"bGPdata"]),
    (b"GP", &[b"GreasePencil"]),
    (b"GR", &[b"Collection"]),
    (b"ID", &[b"ID"]),
    (b"IM", &[b"Image"]),
    (b"IP", &[b"Ipo"]),
    (b"KE", &[b"Key"]),
    (b"LA", &[b"Light"]),
    (b"LI", &[b"Library"]),
    (b"LP", &[b"LightProbe"]),
    (b"LS", &[b"FreestyleLineStyle"]),
    (b"LT", &[b"Lattice"]),
    (b"MA", &[b"Material"]),
    (b"MB", &[b"MetaBall"]),
    (b"MC", &[b"MovieClip"]),
    (b"ME", &[b"Mesh"]),
    (b"MS", &[b"Mask"]),
    (b"NT", &[b"bNodeTree"]),
    (b"OB", &[b"Object"]),
    (b"PA", &[b"ParticleSettings"]),
    (b"PC", &[b"PaintCurve"]),
    (b"PL", &[b"Palette"]),
    (b"PT", &[b"PointCloud"]),
    (b"SC", &[b"Scene"]),
    (b"SK", &[b"Speaker"]),
    (b"SN", &[b"bScreen"]),
    (b"SR", &[b"bScreen"]),
    (b"SO", &[b"bSound"]),
    (b"TE", &[b"Tex"]),
    (b"TX", &[b"Text"]),
    (b"VF", &[b"VFont"]),
    (b"VO", &[b"Volume"]),
    (b"WM", &[b"wmWindowManager"]),
    (b"WO", &[b"World"]),
    (b"WS", &[b"WorkSpace"]),
];
// Known ID families with no direct external-file declaration. ID/node links
// still use the ordinary graph walk. Camera is deliberately NOT here: modern
// DNA has a mode-qualified custom OSL filepath. See docs/ASSET-FAMILIES.md for
// Blender 5.2.1 saved-DNA/RNA evidence and the historical/runtime exceptions.
pub(crate) fn non_file_family(code: &[u8], ty: &[u8]) -> bool {
    matches!(
        code,
        b"AC"
            | b"AR"
            | b"CU"
            | b"CV"
            | b"GD"
            | b"GP"
            | b"GR"
            | b"KE"
            | b"LA"
            | b"LP"
            | b"LS"
            | b"LT"
            | b"MA"
            | b"MB"
            | b"MS"
            | b"PA"
            | b"PC"
            | b"PL"
            | b"PT"
            | b"SK"
            | b"SN"
            | b"SR"
            | b"TE"
            | b"WM"
            | b"WO"
            | b"WS"
    ) && TYPES.iter().any(|(c, types)| {
        *c == code
            && types
                .iter()
                .any(|expected| canonical(expected) == canonical(ty))
    })
}
pub(crate) const ALIASES: &[(&[u8], &[u8])] = &[
    (b"Group", b"Collection"),
    (b"Lamp", b"Light"),
    (b"Screen", b"bScreen"),
];
pub(crate) const CODE_ALIASES: &[(&[u8], &[u8])] = &[(b"SN", b"SR")];
pub(crate) fn expected(code: &[u8; 4]) -> Option<&'static [&'static [u8]]> {
    (code[2..] == [0, 0])
        .then(|| {
            TYPES
                .iter()
                .find_map(|(c, t)| (*c == &code[..2]).then_some(*t))
        })
        .flatten()
}
pub(crate) fn canonical(ty: &[u8]) -> &[u8] {
    ALIASES
        .iter()
        .find_map(|(old, new)| (*old == ty).then_some(*new))
        .unwrap_or(ty)
}
pub(crate) fn canonical_code(code: &[u8]) -> &[u8] {
    CODE_ALIASES
        .iter()
        .find_map(|(old, new)| (*old == code).then_some(*new))
        .unwrap_or(code)
}
// Strict writer/import admission only. Read roles are proven from the saved
// schema, discriminator, native namespace and target extent; decimal header
// stamps do not prove (or disprove) those contracts.
pub(crate) fn supported_version(version: u32) -> bool {
    matches!(version, 405 | 502)
}
pub(crate) fn expected_for_version(
    code: &[u8; 4],
    version: u32,
) -> Option<&'static [&'static [u8]]> {
    supported_version(version).then(|| expected(code)).flatten()
}
pub(crate) fn canonical_for_version(ty: &[u8], version: u32) -> &[u8] {
    if supported_version(version) {
        canonical(ty)
    } else {
        ty
    }
}
pub(crate) fn canonical_code_for_version(code: &[u8], version: u32) -> &[u8] {
    if supported_version(version) {
        canonical_code(code)
    } else {
        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_vectors_and_refusals() {
        assert_eq!(expected(b"SO\0\0"), Some(&[b"bSound".as_slice()][..]));
        assert_eq!(expected(b"ZZ\0\0"), None);
        assert_eq!(expected(b"SODA"), None);
        assert_eq!(expected(b"USER"), None);
        for version in [405, 502] {
            assert!(expected_for_version(b"GR\0\0", version).is_some());
            assert_eq!(canonical_for_version(b"Group", version), b"Collection");
            assert_eq!(canonical_for_version(b"Lamp", version), b"Light");
            assert_eq!(canonical_code_for_version(b"SN", version), b"SR");
        }
        assert_eq!(expected_for_version(b"GR\0\0", 999), None);
        assert_eq!(canonical_for_version(b"Group", 999), b"Group");
        assert_eq!(canonical_code_for_version(b"SN", 999), b"SN");
    }
    #[test]
    fn registry_and_alias_keys_are_unique() {
        let mut codes = std::collections::BTreeSet::new();
        for (code, types) in TYPES {
            assert_eq!(code.len(), 2);
            assert!(codes.insert(*code));
            assert!(!types.is_empty());
        }
        for (old, new) in ALIASES {
            assert_ne!(old, new);
            assert_eq!(canonical(new), *new);
        }
    }
}
