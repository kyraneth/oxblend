use super::*;
#[test]
fn numeric_codec_exact_width_signedness_overflow() {
    for (kind, width) in [
        (b"int8_t".as_slice(), 1),
        (b"short", 2),
        (b"int", 4),
        (b"int64_t", 8),
    ] {
        assert_eq!(
            scalar(kind, width, &TypedValue::Signed(-1)).unwrap(),
            vec![255; width]
        );
        assert_eq!(
            scalar(kind, width, &TypedValue::Unsigned(1))
                .unwrap_err()
                .kind,
            "typed_kind"
        );
        if width < 8 {
            assert_eq!(
                scalar(kind, width, &TypedValue::Signed(1i64 << (width * 8 - 1)))
                    .unwrap_err()
                    .kind,
                "typed_overflow"
            );
        }
    }
    for (kind, width) in [
        (b"uchar".as_slice(), 1),
        (b"ushort", 2),
        (b"uint", 4),
        (b"uint64_t", 8),
    ] {
        assert_eq!(scalar(kind, width, &TypedValue::Unsigned(1)).unwrap()[0], 1);
        assert_eq!(
            scalar(kind, width, &TypedValue::Signed(1))
                .unwrap_err()
                .kind,
            "typed_kind"
        );
        if width < 8 {
            assert_eq!(
                scalar(kind, width, &TypedValue::Unsigned(1u64 << (width * 8)))
                    .unwrap_err()
                    .kind,
                "typed_overflow"
            );
        }
    }
    assert_eq!(
        scalar(b"int", 8, &TypedValue::Signed(1)).unwrap_err().kind,
        "typed_width"
    );
    assert_eq!(
        scalar(b"Scene", 4, &TypedValue::Signed(1))
            .unwrap_err()
            .kind,
        "typed_kind"
    );
}
#[test]
fn float_bits_array_dimensions_and_no_string_truncation() {
    for bits in [0u32, 0x80000000, 0x7fc00042] {
        assert_eq!(
            scalar(b"float", 4, &TypedValue::F32Bits(bits)).unwrap(),
            bits.to_le_bytes()
        );
    }
    for bits in [0u64, 0x8000000000000000, 0x7ff8000000000042] {
        assert_eq!(
            scalar(b"double", 8, &TypedValue::F64Bits(bits)).unwrap(),
            bits.to_le_bytes()
        );
    }
    assert!(scalar(b"double", 4, &TypedValue::F64Bits(0)).is_err());
    assert_eq!(
        encode(b"char", 1, 4, &TypedValue::CString(b"abcd".to_vec()))
            .unwrap_err()
            .kind,
        "typed_overflow"
    );
    assert_eq!(
        encode(b"char", 1, 4, &TypedValue::CString(b"a\0b".to_vec()))
            .unwrap_err()
            .kind,
        "string_nul"
    );
    assert_eq!(
        encode(b"char", 1, 4, &TypedValue::CString(Vec::new())).unwrap(),
        vec![0]
    );
    assert_eq!(
        encode(b"char", 1, 4, &TypedValue::FixedBytes(vec![1, 2]))
            .unwrap_err()
            .kind,
        "typed_width"
    );
    assert_eq!(
        encode(
            b"short",
            2,
            2,
            &TypedValue::Array(vec![TypedValue::Signed(1)])
        )
        .unwrap_err()
        .kind,
        "typed_width"
    );
    assert_eq!(
        encode(
            b"short",
            2,
            2,
            &TypedValue::Array(vec![TypedValue::Signed(1), TypedValue::Signed(-1)])
        )
        .unwrap(),
        vec![1, 0, 255, 255]
    );
}
