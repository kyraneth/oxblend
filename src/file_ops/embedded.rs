pub(super) const OWNED: &[(&[u8], &[u8])] = &[
    (b"Scene", b"nodetree"),
    (b"Scene", b"master_collection"),
    (b"Material", b"nodetree"),
    (b"World", b"nodetree"),
    (b"Lamp", b"nodetree"),
    (b"Tex", b"nodetree"),
    (b"FreestyleLineStyle", b"nodetree"),
];
pub(super) const ALIASES: &[(&[u8], &[u8])] = &[(b"Light", b"Lamp")];
fn canonical(parent: &[u8]) -> &[u8] {
    ALIASES
        .iter()
        .find_map(|(a, b)| (*a == parent).then_some(*b))
        .unwrap_or(parent)
}
pub(super) fn known_site(parent: &[u8], field: &[u8]) -> bool {
    OWNED.contains(&(canonical(parent), field))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed_read_membership_is_site_qualified() {
        for &(parent, field) in OWNED {
            assert!(known_site(parent, field));
        }
        assert!(known_site(b"Light", b"nodetree"));
        assert!(!known_site(b"Unknown", b"nodetree"));
        assert!(!known_site(b"LayerCollection", b"collection"));
        assert!(!known_site(b"Brush", b"nodetree"));
    }
}
