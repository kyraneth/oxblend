//! serialized declarator byte-boundary contracts.
use super::name_info;

#[test]
fn byte_names_and_function_spellings() {
    for (full, name, pointer, count) in [
        (b"r\x92t".as_slice(), b"r\x92t".as_slice(), false, 1),
        (b"r t", b"r t", false, 1),
        (b"r*t", b"r*t", true, 1),
        (b"value()", b"value", false, 1),
        (b"(x)()", b"x", false, 1),
        (b"(*x)", b"x", true, 1),
        (b"(*x)(void)", b"x)(void", true, 1),
        (b"(*x)()()", b"x", true, 1),
        (b"(*x[2][3])()", b"x", true, 6),
        (b"(*x)()[2]", b"x)()", true, 2),
        (b" x [2] ", b"x ", false, 2),
        (b" ( *x)() ", b" *x", true, 1),
        (b"x\x0b", b"x\x0b", false, 1),
        (b"x[2](*)", b"x", true, 2),
    ] {
        assert_eq!(name_info(full).unwrap(), (name.to_vec(), pointer, count));
    }
    // Whitespace is trimmed once BEFORE edge '*()' bytes, not as one set.
    assert_eq!(name_info(b"x[2]\t*()").unwrap_err().kind, "invalid_sdna");
}

#[test]
fn all_non_nul_identifier_byte_boundaries() {
    // NUL is the on-disk NAME terminator, not an identifier byte.
    for byte in 1u8..=255 {
        let edge_trimmed = byte.is_ascii_whitespace() || b"*()".contains(&byte);
        for (full, expected) in [
            (
                vec![byte, b'x', b't'],
                if edge_trimmed {
                    b"xt".to_vec()
                } else {
                    vec![byte, b'x', b't']
                },
            ),
            (vec![b'r', byte, b't'], vec![b'r', byte, b't']),
            (
                vec![b'r', b'x', byte],
                if edge_trimmed {
                    b"rx".to_vec()
                } else {
                    vec![b'r', b'x', byte]
                },
            ),
        ] {
            if byte == b'[' {
                assert_eq!(name_info(&full).unwrap_err().kind, "invalid_sdna");
            } else {
                assert_eq!(
                    name_info(&full).unwrap(),
                    (expected, byte == b'*', 1),
                    "{full:?}"
                );
            }
        }
    }
}

#[test]
fn all_dimension_byte_boundaries_remain_decimal_only() {
    for byte in 1u8..=255 {
        // A byte inside a dimension cannot be normalized away.
        for full in [
            vec![b'x', b'[', byte, b']'],
            vec![b'x', b'[', b'2', byte, b']'],
        ] {
            if byte.is_ascii_digit() {
                assert!(name_info(&full).is_ok(), "{full:?}");
            } else {
                assert_eq!(
                    name_info(&full).unwrap_err().kind,
                    "invalid_sdna",
                    "{full:?}"
                );
            }
        }
    }
}

#[test]
fn non_utf8_field_is_addressable_through_byte_native_reader_api() {
    use crate::{BlendFile, value::PathStep};
    let mutated = BlendFile::from_bytes(
        include_bytes!("../tests/fixtures/declarators/mechanism-729274-1.blend").to_vec(),
    )
    .unwrap();
    let ordinary = BlendFile::from_bytes(
        include_bytes!("../tests/fixtures/declarators/mechanism-726f74-1.blend").to_vec(),
    )
    .unwrap();
    for raw in [false, true] {
        assert_eq!(
            mutated
                .get(0, 0, &[PathStep::Field(b"r\x92t".to_vec())], raw)
                .unwrap()
                .json(),
            ordinary
                .get(0, 0, &[PathStep::Field(b"rot".to_vec())], raw)
                .unwrap()
                .json(),
        );
    }
    assert_eq!(mutated.schema().structs[0].fields[0].name, b"r\x92t");
}

#[test]
fn zero_product_does_not_hide_dimension_overflow() {
    assert_eq!(name_info(b"x[18446744073709551615][0]").unwrap().2, 0);
    assert_eq!(name_info(b"x[0][18446744073709551615]").unwrap().2, 0);
    assert_eq!(
        name_info(b"x[18446744073709551615][2][0]")
            .unwrap_err()
            .kind,
        "overflow"
    );
    assert_eq!(
        name_info(b"x[0][18446744073709551616]").unwrap_err().kind,
        "overflow"
    );
}
